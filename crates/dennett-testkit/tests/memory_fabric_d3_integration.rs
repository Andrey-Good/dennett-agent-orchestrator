//! Phase D3 Memory Fabric real PostgreSQL 16 + standalone `dennett-memoryd` integration,
//! property-based, and fault-injection test suite (100% independent of `dennett-head`).
//!
//! Requires `DENNETT_TEST_PROFILE=1` and `DENNETT_TEST_MEMORY_PG_URL`.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use dennett_contracts::{
    AuthPrincipal, ConflictStatus, ConsolidationJobStatus, ConsolidationJobType,
    CorrectNoteRequest, CorrelationId, CreateClaimRequest, CreateConsolidationJobRequest,
    CreateDocumentRequest, CreateNoteRequest, DennettError, EpistemicStatus, ExportRequest,
    ForgetPropagationStatus, ForgetRequest, ForgetTargetType, IdempotencyKey, ImportRequest,
    IndexStatus, IngestEvidenceRequest, MemoryCapability, MemoryClaim, MemoryConflict,
    MemoryDocument, MemoryScope, PrincipalId, ProfileMode, ResolveConflictRequest,
    SearchNotesRequest, SupersedeClaimRequest, TimestampMs, TriggerIndexRebuildRequest, TrustLevel,
    UpdateDocumentRequest,
};
use dennett_memory_client::MemoryClient;
use dennett_memory_core::{
    AuthConfig, DEFAULT_AUDIENCE, DEFAULT_ISSUER, ExportedDocument, LATEST_SCHEMA_VERSION,
    MAX_IMPORT_SAFE_BYTES, MemoryService, PortablePackagePayload, SignedTokenClaims,
    StandaloneAuthVerifier, StaticTokenEntryConfig, ValidatedPgTarget,
    parse_and_validate_import_package, prepare_search_query, serialize_canonical_package,
};
use dennett_testkit::profile::TEST_PROFILE_ENV;
use proptest::prelude::*;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::task::JoinSet;
use tokio::time;
use tokio_postgres::NoTls;
use uuid::Uuid;

fn require_memory_pg_url() -> String {
    assert_eq!(
        env::var(TEST_PROFILE_ENV).ok().as_deref(),
        Some("1"),
        "{TEST_PROFILE_ENV}=1 is required for D3 Memory Fabric integration tests"
    );
    match env::var("DENNETT_TEST_MEMORY_PG_URL") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => panic!("DENNETT_TEST_MEMORY_PG_URL is missing"),
    }
}

fn pg_target() -> ValidatedPgTarget {
    ValidatedPgTarget::parse(&require_memory_pg_url(), ProfileMode::TestIsolated)
        .expect("valid *_test PG URL")
}

async fn connect_raw_pg(
    target: &ValidatedPgTarget,
) -> (tokio_postgres::Client, tokio::task::JoinHandle<()>) {
    let (client, conn) = target
        .config()
        .connect(NoTls)
        .await
        .expect("connect raw PG");
    let handle = tokio::spawn(async move {
        let _ = conn.await;
    });
    (client, handle)
}

fn unique_scope(prefix: &str) -> MemoryScope {
    let suffix = &Uuid::new_v4().simple().to_string()[..12];
    MemoryScope::try_new(format!("{prefix}-{suffix}")).expect("valid unique scope")
}

fn principal_with_caps(
    name: &str,
    scopes: &[MemoryScope],
    caps: &[MemoryCapability],
) -> AuthPrincipal {
    AuthPrincipal {
        principal_id: PrincipalId::try_new(name).expect("valid principal"),
        issuer: String::from(DEFAULT_ISSUER),
        token_id: format!("tok-{name}"),
        allowed_scopes: scopes.iter().cloned().collect(),
        capabilities: caps.iter().copied().collect(),
        expires_at: None,
    }
}

fn full_principal(name: &str, scopes: &[MemoryScope]) -> AuthPrincipal {
    principal_with_caps(
        name,
        scopes,
        &[
            MemoryCapability::Read,
            MemoryCapability::Write,
            MemoryCapability::Admin,
            MemoryCapability::Export,
            MemoryCapability::Forget,
        ],
    )
}

fn make_note_req(scope: &MemoryScope, title: &str, body: &str) -> CreateNoteRequest {
    CreateNoteRequest {
        scope: scope.clone(),
        title: title.to_owned(),
        body: body.to_owned(),
        tags: vec![],
        observed_at: None,
        evidence_links: vec![],
        author_label: None,
        admission_status: None,
        salience: None,
        topic_key: None,
        idempotency_key: IdempotencyKey::generate(),
    }
}

fn make_claim_req(scope: &MemoryScope, topic: &str, text: &str) -> CreateClaimRequest {
    CreateClaimRequest {
        scope: scope.clone(),
        topic_key: topic.to_owned(),
        claim_key: None,
        natural_language: text.to_owned(),
        epistemic_status: EpistemicStatus::Asserted,
        supersedes_claim_id: None,
        valid_from: None,
        valid_to: None,
        source_note_id: None,
        caveats: vec![],
        provenance: BTreeMap::new(),
        evidence_links: vec![],
        idempotency_key: IdempotencyKey::generate(),
    }
}

fn find_binary_near_current_exe(exe_name: &str) -> Option<PathBuf> {
    let current = env::current_exe().ok()?;
    for dir in current.ancestors().skip(1).take(6) {
        let candidate = dir.join(exe_name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn locate_binary(bin_name: &str, package: &str) -> PathBuf {
    let exe_name = if cfg!(windows) {
        format!("{bin_name}.exe")
    } else {
        bin_name.to_owned()
    };
    if let Some(found) = find_binary_near_current_exe(&exe_name) {
        return found;
    }
    let status = std::process::Command::new("cargo")
        .args(["build", "-p", package, "--bin", bin_name])
        .status();
    assert!(
        matches!(status, Ok(s) if s.success()),
        "failed to build `{bin_name}`"
    );
    find_binary_near_current_exe(&exe_name)
        .unwrap_or_else(|| panic!("could not locate compiled `{bin_name}`"))
}

struct RunningMemoryd {
    child: Child,
    addr: String,
    base_url: String,
}

impl RunningMemoryd {
    async fn spawn(pg_url: &str, auth_config: &AuthConfig) -> Self {
        let bin = locate_binary("dennett-memoryd", "dennett-memoryd");
        let auth_json = serde_json::to_string(auth_config).expect("serialize auth config");
        let mut child = Command::new(&bin)
            .args(["--serve", "127.0.0.1:0"])
            .env(TEST_PROFILE_ENV, "1")
            .env("DENNETT_TEST_MEMORY_PG_URL", pg_url)
            .env("DENNETT_MEMORY_AUTH_CONFIG", &auth_json)
            .env_remove("DENNETT_MEMORY_PG_URL")
            .env_remove("DENNETT_MEMORY_AUTH_CONFIG_FILE")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("spawn dennett-memoryd");

        let stdout = child.stdout.take().expect("memoryd stdout");
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        match time::timeout(Duration::from_secs(10), reader.read_line(&mut line)).await {
            Ok(Ok(n)) if n > 0 => {}
            other => panic!("did not receive LISTENING line from memoryd: {other:?}"),
        }
        let addr = line
            .trim()
            .strip_prefix("LISTENING ")
            .unwrap_or_else(|| panic!("unexpected startup line `{line}`"))
            .to_owned();
        let base_url = format!("http://{addr}");
        Self {
            child,
            addr,
            base_url,
        }
    }

    async fn shutdown(mut self) {
        let req = format!(
            "POST /shutdown HTTP/1.1\r\nHost: {}\r\nX-Dennett-Shutdown: 1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            self.addr
        );
        let mut stream = time::timeout(Duration::from_secs(5), TcpStream::connect(&self.addr))
            .await
            .expect("connect shutdown")
            .expect("tcp connect");
        stream
            .write_all(req.as_bytes())
            .await
            .expect("write shutdown");
        let mut resp = Vec::new();
        let _ = time::timeout(Duration::from_secs(5), stream.read_to_end(&mut resp)).await;
        let status = time::timeout(Duration::from_secs(5), self.child.wait())
            .await
            .expect("wait shutdown")
            .expect("child wait");
        assert!(status.success(), "memoryd shutdown failed: {status}");
    }
}

struct McpSession {
    child: Child,
    stdin: tokio::process::ChildStdin,
    stdout: BufReader<tokio::process::ChildStdout>,
    next_id: i64,
}

impl McpSession {
    fn spawn(base_url: &str, token: &str) -> Self {
        let bin = locate_binary("dennett-memory-mcp", "dennett-memory-mcp");
        let mut child = Command::new(&bin)
            .env("DENNETT_MEMORY_URL", base_url)
            .env("DENNETT_MEMORY_TOKEN", token)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("spawn dennett-memory-mcp");
        let stdin = child.stdin.take().expect("mcp stdin");
        let stdout = BufReader::new(child.stdout.take().expect("mcp stdout"));
        Self {
            child,
            stdin,
            stdout,
            next_id: 1,
        }
    }

    async fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let line = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        })
        .to_string();
        self.stdin.write_all(line.as_bytes()).await.expect("write");
        self.stdin.write_all(b"\n").await.expect("newline");
        self.stdin.flush().await.expect("flush");

        let mut resp = String::new();
        let n = time::timeout(Duration::from_secs(10), self.stdout.read_line(&mut resp))
            .await
            .expect("mcp timeout")
            .expect("mcp read");
        assert!(n > 0, "unexpected EOF from memory-mcp");
        serde_json::from_str(resp.trim()).expect("valid MCP JSON")
    }

    async fn call_tool(&mut self, name: &str, arguments: Value) -> (bool, Value) {
        let resp = self
            .request(
                "tools/call",
                json!({ "name": name, "arguments": arguments }),
            )
            .await;
        let is_error = resp["result"]["isError"].as_bool().unwrap_or(true);
        let text = resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("missing MCP tool text: {resp}"));
        let parsed: Value = serde_json::from_str(text).expect("tool text JSON");
        (is_error, parsed)
    }

    async fn close(mut self) {
        drop(self.stdin);
        let status = time::timeout(Duration::from_secs(5), self.child.wait())
            .await
            .expect("mcp close timeout")
            .expect("mcp wait");
        assert!(status.success(), "memory-mcp exited abnormally: {status}");
    }
}

async fn run_cli(base_url: &str, token: &str, args: &[&str]) -> (i32, String, String) {
    let bin = locate_binary("dennett-memory-cli", "dennett-memory-client");
    let output = time::timeout(
        Duration::from_secs(10),
        Command::new(&bin)
            .args(args)
            .env("DENNETT_MEMORY_URL", base_url)
            .env("DENNETT_MEMORY_TOKEN", token)
            .output(),
    )
    .await
    .expect("cli timeout")
    .expect("spawn cli");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

// ============================================================================
// 3.1 Search Index: Degraded, Fresh-before-Reindex, Rebuild, Lagging, Multi-lang FTS
// ============================================================================

#[tokio::test]
async fn d3_index_degraded_still_searchable() {
    let target = pg_target();
    let svc = MemoryService::new(target.clone()).expect("service");
    let scope = unique_scope("d3-degraded");
    let principal = full_principal("agent:d3-deg", std::slice::from_ref(&scope));
    let cid = CorrelationId::generate();

    let note = svc
        .create_note(
            &principal,
            &cid,
            make_note_req(&scope, "Degraded search marker", "Fresh tail keeps working"),
        )
        .await
        .expect("create note");

    let (pg, handle) = connect_raw_pg(&target).await;
    pg.execute(
        "UPDATE memory_index_state SET status = 'degraded' WHERE scope = $1",
        &[&scope.as_str()],
    )
    .await
    .expect("mark degraded");
    pg.execute(
        "DELETE FROM memory_search_index WHERE scope = $1",
        &[&scope.as_str()],
    )
    .await
    .expect("clear search index");

    let res = svc
        .search_notes(
            &principal,
            SearchNotesRequest {
                scope: Some(scope.clone()),
                query: Some(String::from("Fresh tail")),
                limit: Some(10),
                include_history: false,
            },
        )
        .await
        .expect("search must still succeed when degraded");

    assert_eq!(res.index_status, IndexStatus::Degraded);
    assert!(res.stale_warning.is_some());
    assert!(
        res.notes.iter().any(|n| n.note_id == note.note.note_id),
        "degraded index must still return note from fresh tail"
    );

    drop(pg);
    handle.abort();
}

#[tokio::test]
async fn d3_fresh_before_reindex() {
    let target = pg_target();
    let svc = MemoryService::new(target.clone()).expect("service");
    let scope = unique_scope("d3-fresh");
    let principal = full_principal("agent:d3-fresh", std::slice::from_ref(&scope));
    let cid = CorrelationId::generate();

    let version = svc.ensure_migrated().await.expect("migrate");
    assert_eq!(version, LATEST_SCHEMA_VERSION);

    // Put scope in `rebuilding` state so newly created notes are NOT added to `memory_search_index`.
    let (pg, handle) = connect_raw_pg(&target).await;
    pg.execute(
        "INSERT INTO memory_index_state (scope, watermark_seq, model_version, status)
         VALUES ($1, 0, 'fts-v1', 'rebuilding')
         ON CONFLICT (scope) DO UPDATE SET status = 'rebuilding'",
        &[&scope.as_str()],
    )
    .await
    .expect("set rebuilding");

    let fresh = svc
        .create_note(
            &principal,
            &cid,
            make_note_req(
                &scope,
                "Unindexed fresh note",
                "Visible immediately before reindex",
            ),
        )
        .await
        .expect("create fresh note");

    // Verify 0 rows in memory_search_index for this note.
    let idx_count: i64 = pg
        .query_one(
            "SELECT COUNT(*)::bigint FROM memory_search_index WHERE source_id = $1",
            &[&fresh.note.note_id.as_uuid()],
        )
        .await
        .expect("count index")
        .get(0);
    assert_eq!(
        idx_count, 0,
        "fresh note must not yet be in memory_search_index"
    );

    let search = svc
        .search_notes(
            &principal,
            SearchNotesRequest {
                scope: Some(scope.clone()),
                query: Some(String::from("immediately before reindex")),
                limit: Some(10),
                include_history: false,
            },
        )
        .await
        .expect("fresh-tail search");
    assert_eq!(search.index_status, IndexStatus::Rebuilding);
    assert!(search.notes.iter().any(|n| n.note_id == fresh.note.note_id));

    drop(pg);
    handle.abort();
}

#[tokio::test]
async fn d3_index_rebuild_no_data_loss() {
    let target = pg_target();
    let svc = MemoryService::new(target.clone()).expect("service");
    let scope = unique_scope("d3-rebuild");
    let admin = full_principal("agent:d3-admin", std::slice::from_ref(&scope));
    let non_admin = principal_with_caps(
        "agent:d3-writer",
        std::slice::from_ref(&scope),
        &[MemoryCapability::Read, MemoryCapability::Write],
    );
    let cid = CorrelationId::generate();

    let note = svc
        .create_note(
            &admin,
            &cid,
            make_note_req(&scope, "Rebuild Note", "Canonical note body"),
        )
        .await
        .expect("note");
    let claim = svc
        .create_claim(
            &admin,
            &cid,
            make_claim_req(&scope, "storage", "PostgreSQL 16 canonical"),
        )
        .await
        .expect("claim");
    let doc = svc
        .create_document(
            &admin,
            &cid,
            CreateDocumentRequest {
                scope: scope.clone(),
                document_key: String::from("dossier:rebuild"),
                title: String::from("Rebuild Dossier"),
                body: String::from("Canonical dossier body"),
                provenance_claim_ids: vec![claim.claim_id],
                provenance_note_ids: vec![note.note.note_id],
                idempotency_key: IdempotencyKey::generate(),
            },
        )
        .await
        .expect("doc");

    // Non-admin MUST be rejected on rebuild.
    let denied = svc
        .trigger_index_rebuild(
            &non_admin,
            TriggerIndexRebuildRequest {
                scope: scope.clone(),
            },
        )
        .await;
    assert!(matches!(denied, Err(DennettError::ForbiddenScope { .. })));

    // Wipe search index for scope completely.
    let (pg, handle) = connect_raw_pg(&target).await;
    pg.execute(
        "DELETE FROM memory_search_index WHERE scope = $1",
        &[&scope.as_str()],
    )
    .await
    .expect("wipe search index");

    let status_before = svc
        .get_index_status(&admin, Some(&scope))
        .await
        .expect("degraded status");
    assert_eq!(status_before.status, IndexStatus::Degraded);

    // Rebuild restores all 3 entries (note + claim + document) with zero canonical data loss.
    let rebuilt = svc
        .trigger_index_rebuild(
            &admin,
            TriggerIndexRebuildRequest {
                scope: scope.clone(),
            },
        )
        .await
        .expect("rebuild");
    assert_eq!(rebuilt.index_state.status, IndexStatus::Healthy);
    assert_eq!(rebuilt.entries_rebuilt, 3);
    assert!(rebuilt.index_state.last_rebuild_at.is_some());

    let indexed_ids: Vec<Uuid> = pg
        .query(
            "SELECT source_id FROM memory_search_index WHERE scope = $1",
            &[&scope.as_str()],
        )
        .await
        .expect("query rebuilt index")
        .into_iter()
        .map(|r| r.get(0))
        .collect();
    assert!(indexed_ids.contains(&note.note.note_id.as_uuid()));
    assert!(indexed_ids.contains(&claim.claim_id.as_uuid()));
    assert!(indexed_ids.contains(&doc.document_id.as_uuid()));

    drop(pg);
    handle.abort();
}

#[tokio::test]
async fn d3_index_lagging_status() {
    let target = pg_target();
    let svc = MemoryService::new(target.clone()).expect("service");
    let scope = unique_scope("d3-lag");
    let principal = full_principal("agent:d3-lag", std::slice::from_ref(&scope));
    let cid = CorrelationId::generate();

    svc.create_note(
        &principal,
        &cid,
        make_note_req(&scope, "Lag note 1", "body 1"),
    )
    .await
    .expect("n1");
    svc.create_note(
        &principal,
        &cid,
        make_note_req(&scope, "Lag note 2", "body 2"),
    )
    .await
    .expect("n2");

    let (pg, handle) = connect_raw_pg(&target).await;
    pg.execute(
        "UPDATE memory_index_state SET watermark_seq = 1 WHERE scope = $1",
        &[&scope.as_str()],
    )
    .await
    .expect("set lagging watermark");

    let status = svc
        .get_index_status(&principal, Some(&scope))
        .await
        .expect("index status");
    assert_eq!(status.status, IndexStatus::Lagging);

    let search = svc
        .search_notes(
            &principal,
            SearchNotesRequest {
                scope: Some(scope),
                query: Some(String::from("Lag note")),
                limit: Some(10),
                include_history: false,
            },
        )
        .await
        .expect("search lagging");
    assert_eq!(search.index_status, IndexStatus::Lagging);
    assert!(search.stale_warning.is_some());
    assert_eq!(search.notes.len(), 2);

    drop(pg);
    handle.abort();
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "integration test covers English, Russian, Unicode/CJK FTS, and include_history in one PG16 scope"
)]
async fn d3_multi_language_fts_and_include_history() {
    let target = pg_target();
    let svc = MemoryService::new(target).expect("service");
    let scope = unique_scope("d3-fts");
    let principal = full_principal("agent:d3-fts", std::slice::from_ref(&scope));
    let cid = CorrelationId::generate();

    let ru = svc
        .create_note(
            &principal,
            &cid,
            make_note_req(
                &scope,
                "Распределённые вычисления",
                "Быстрые алгоритмы обработки потоковых данных",
            ),
        )
        .await
        .expect("ru note");
    let en = svc
        .create_note(
            &principal,
            &cid,
            make_note_req(
                &scope,
                "Server configuration guide",
                "Configuring resilient production databases cleanly",
            ),
        )
        .await
        .expect("en note");
    let mixed = svc
        .create_note(
            &principal,
            &cid,
            make_note_req(
                &scope,
                "Mixed PG16-fts-v1 note",
                "OldHistoricTermAlpha before correction",
            ),
        )
        .await
        .expect("mixed note");

    // Correct `mixed` so `OldHistoricTermAlpha` only exists in revision 1.
    svc.correct_note(
        &principal,
        &cid,
        CorrectNoteRequest {
            note_id: mixed.note.note_id,
            scope: scope.clone(),
            expected_revision: 1,
            title: String::from("Mixed PG16-fts-v1 note updated"),
            body: String::from("NewCurrentTermBeta after correction"),
            tags: vec![],
            rationale: String::from("Updated terminology"),
            evidence_links: vec![],
            author_label: None,
            idempotency_key: IdempotencyKey::generate(),
        },
    )
    .await
    .expect("correct mixed");

    // Russian stemming: "вычисление" matches "вычисления" via tsv_russian.
    let ru_hits = svc
        .search_notes(
            &principal,
            SearchNotesRequest {
                scope: Some(scope.clone()),
                query: Some(String::from("вычисление")),
                limit: Some(10),
                include_history: false,
            },
        )
        .await
        .expect("ru search");
    assert_eq!(ru_hits.index_status, IndexStatus::Healthy);
    assert!(ru_hits.notes.iter().any(|n| n.note_id == ru.note.note_id));

    // English stemming: "configure" matches "Configuring" via tsv_english.
    let en_hits = svc
        .search_notes(
            &principal,
            SearchNotesRequest {
                scope: Some(scope.clone()),
                query: Some(String::from("configure")),
                limit: Some(10),
                include_history: false,
            },
        )
        .await
        .expect("en search");
    assert!(en_hits.notes.iter().any(|n| n.note_id == en.note.note_id));

    // Simple token match: "PG16-fts-v1".
    let mix_hits = svc
        .search_notes(
            &principal,
            SearchNotesRequest {
                scope: Some(scope.clone()),
                query: Some(String::from("PG16-fts-v1")),
                limit: Some(10),
                include_history: false,
            },
        )
        .await
        .expect("mixed search");
    assert!(
        mix_hits
            .notes
            .iter()
            .any(|n| n.note_id == mixed.note.note_id)
    );

    // Historical term absent from current search (`include_history=false`), present when `include_history=true`.
    let no_hist = svc
        .search_notes(
            &principal,
            SearchNotesRequest {
                scope: Some(scope.clone()),
                query: Some(String::from("OldHistoricTermAlpha")),
                limit: Some(10),
                include_history: false,
            },
        )
        .await
        .expect("no hist search");
    assert_eq!(no_hist.notes.len(), 0);

    let with_hist = svc
        .search_notes(
            &principal,
            SearchNotesRequest {
                scope: Some(scope),
                query: Some(String::from("OldHistoricTermAlpha")),
                limit: Some(10),
                include_history: true,
            },
        )
        .await
        .expect("with hist search");
    assert!(
        with_hist
            .notes
            .iter()
            .any(|n| n.note_id == mixed.note.note_id)
    );
}

// ============================================================================
// 3.2 Claims & Conflicts: Temporal Guard, Supersession Chain, Auto-Conflict, Resolution
// ============================================================================

#[tokio::test]
async fn d3_old_note_vs_new_decision_and_supersession_chain() {
    let target = pg_target();
    let svc = MemoryService::new(target).expect("service");
    let scope = unique_scope("d3-claims");
    let principal = full_principal("agent:d3-claims", std::slice::from_ref(&scope));
    let cid = CorrelationId::generate();

    // Newer decision B (valid_from = 2000).
    let mut req_b = make_claim_req(&scope, "db-engine", "Use PostgreSQL 16");
    req_b.valid_from = Some(TimestampMs::from_millis(1_700_000_002_000));
    let claim_b = svc
        .create_claim(&principal, &cid, req_b)
        .await
        .expect("create claim B");

    // Older claim A (valid_from = 1000) MUST NOT supersede newer B without explicit resolution_event_id!
    let mut old_req = make_claim_req(&scope, "db-engine", "Use SQLite on server");
    old_req.valid_from = Some(TimestampMs::from_millis(1_700_000_001_000));
    let rejected = svc
        .supersede_claim(
            &principal,
            &cid,
            SupersedeClaimRequest {
                claim_id: claim_b.claim_id,
                scope: scope.clone(),
                new_claim: old_req,
                resolution_event_id: None,
            },
        )
        .await;
    assert!(
        matches!(rejected, Err(DennettError::Conflict { .. })),
        "older claim must NEVER silently supersede newer decision, got: {rejected:?}"
    );

    // Supersession chain: B -> C -> D (ascending valid_from).
    let mut req_c = make_claim_req(&scope, "db-engine", "Use PostgreSQL 16 with GIN FTS");
    req_c.valid_from = Some(TimestampMs::from_millis(1_700_000_003_000));
    let claim_c = svc
        .supersede_claim(
            &principal,
            &cid,
            SupersedeClaimRequest {
                claim_id: claim_b.claim_id,
                scope: scope.clone(),
                new_claim: req_c,
                resolution_event_id: None,
            },
        )
        .await
        .expect("B -> C");
    assert_eq!(claim_c.supersedes_claim_id, Some(claim_b.claim_id));
    assert_eq!(claim_c.epistemic_status, EpistemicStatus::Asserted);

    let mut req_d = make_claim_req(&scope, "db-engine", "Use PostgreSQL 16 + fresh tail");
    req_d.valid_from = Some(TimestampMs::from_millis(1_700_000_004_000));
    let claim_d = svc
        .supersede_claim(
            &principal,
            &cid,
            SupersedeClaimRequest {
                claim_id: claim_c.claim_id,
                scope: scope.clone(),
                new_claim: req_d,
                resolution_event_id: None,
            },
        )
        .await
        .expect("C -> D");
    assert_eq!(claim_d.supersedes_claim_id, Some(claim_c.claim_id));

    let fetched_b = svc
        .get_claim(&principal, claim_b.claim_id)
        .await
        .expect("get B");
    let fetched_c = svc
        .get_claim(&principal, claim_c.claim_id)
        .await
        .expect("get C");
    let fetched_d = svc
        .get_claim(&principal, claim_d.claim_id)
        .await
        .expect("get D");
    assert_eq!(fetched_b.epistemic_status, EpistemicStatus::Superseded);
    assert_eq!(fetched_c.epistemic_status, EpistemicStatus::Superseded);
    assert_eq!(fetched_d.epistemic_status, EpistemicStatus::Asserted);
}

#[tokio::test]
async fn d3_conflict_detection_contested_status_and_resolution() {
    let target = pg_target();
    let svc = MemoryService::new(target).expect("service");
    let scope = unique_scope("d3-conflicts");
    let principal = full_principal("agent:d3-conf", std::slice::from_ref(&scope));
    let cid = CorrelationId::generate();

    let mut c1_req = make_claim_req(&scope, "release-branch", "Release from main branch");
    c1_req.claim_key = Some(String::from("branch"));
    let c1 = svc
        .create_claim(&principal, &cid, c1_req)
        .await
        .expect("c1");
    assert_eq!(c1.epistemic_status, EpistemicStatus::Asserted);

    let mut c2_req = make_claim_req(&scope, "release-branch", "Release from staging branch");
    c2_req.claim_key = Some(String::from("branch"));
    let c2 = svc
        .create_claim(&principal, &cid, c2_req)
        .await
        .expect("c2");
    assert_eq!(
        c2.epistemic_status,
        EpistemicStatus::Contested,
        "contradicting claim must be marked contested"
    );

    // Original claim c1 must also be marked contested.
    let c1_after = svc
        .get_claim(&principal, c1.claim_id)
        .await
        .expect("c1 after");
    assert_eq!(c1_after.epistemic_status, EpistemicStatus::Contested);

    let open = svc
        .list_open_conflicts(&principal, Some(&scope))
        .await
        .expect("open conflicts");
    assert_eq!(open.conflicts.len(), 1);
    let conflict = &open.conflicts[0];
    assert_eq!(conflict.status, ConflictStatus::Open);
    assert!(conflict.candidate_claim_ids.contains(&c1.claim_id));
    assert!(conflict.candidate_claim_ids.contains(&c2.claim_id));

    // Resolve conflict choosing c2 as winner.
    let resolved = svc
        .resolve_conflict(
            &principal,
            &cid,
            ResolveConflictRequest {
                conflict_id: conflict.conflict_id,
                scope: scope.clone(),
                resolution_note: String::from("Owner confirmed staging branch"),
                winning_claim_id: Some(c2.claim_id),
                target_status: None,
                idempotency_key: IdempotencyKey::generate(),
            },
        )
        .await
        .expect("resolve conflict");
    assert_eq!(resolved.status, ConflictStatus::Resolved);
    assert!(resolved.resolution_event_id.is_some());
    assert!(resolved.resolved_at.is_some());

    let c1_final = svc
        .get_claim(&principal, c1.claim_id)
        .await
        .expect("c1 final");
    let c2_final = svc
        .get_claim(&principal, c2.claim_id)
        .await
        .expect("c2 final");
    assert_eq!(c1_final.epistemic_status, EpistemicStatus::Superseded);
    assert_eq!(c2_final.epistemic_status, EpistemicStatus::Asserted);

    let open_after = svc
        .list_open_conflicts(&principal, Some(&scope))
        .await
        .expect("open after");
    assert_eq!(open_after.conflicts.len(), 0);

    // Now verify that an explicit `resolution_event_id` authorizes superseding a newer timestamp.
    let mut newer_req = make_claim_req(&scope, "override-test", "Newer automated claim");
    newer_req.valid_from = Some(TimestampMs::from_millis(1_700_000_050_000));
    let newer = svc
        .create_claim(&principal, &cid, newer_req)
        .await
        .expect("newer");

    let mut older_auth_req = make_claim_req(&scope, "override-test", "Authoritative older rule");
    older_auth_req.valid_from = Some(TimestampMs::from_millis(1_700_000_010_000));
    let authorized = svc
        .supersede_claim(
            &principal,
            &cid,
            SupersedeClaimRequest {
                claim_id: newer.claim_id,
                scope,
                new_claim: older_auth_req,
                resolution_event_id: resolved.resolution_event_id,
            },
        )
        .await
        .expect("explicit resolution_event_id must authorize temporal override");
    assert_eq!(authorized.supersedes_claim_id, Some(newer.claim_id));
}

// ============================================================================
// 3.3 Content Fingerprint Dedup Preserving Episodes
// ============================================================================

#[tokio::test]
async fn d3_same_content_two_episodes_and_fingerprint_dedup() {
    let target = pg_target();
    let svc = MemoryService::new(target.clone()).expect("service");
    let scope = unique_scope("d3-dedup");
    let principal = full_principal("agent:d3-dedup", std::slice::from_ref(&scope));
    let cid1 = CorrelationId::generate();
    let cid2 = CorrelationId::generate();
    let shared_content = "Identical evidence payload observed in two distinct episodes";

    let ep1 = svc
        .ingest_evidence(
            &principal,
            &cid1,
            IngestEvidenceRequest {
                scope: scope.clone(),
                media_type: String::from("text/plain"),
                content: String::from(shared_content),
                source_uri: Some(String::from("session://ep1")),
                observed_at: Some(TimestampMs::from_millis(1_700_000_100_000)),
                metadata: BTreeMap::new(),
                idempotency_key: IdempotencyKey::generate(),
            },
        )
        .await
        .expect("episode 1");

    let ep2 = svc
        .ingest_evidence(
            &principal,
            &cid2,
            IngestEvidenceRequest {
                scope: scope.clone(),
                media_type: String::from("text/plain"),
                content: String::from(shared_content),
                source_uri: Some(String::from("session://ep2")),
                observed_at: Some(TimestampMs::from_millis(1_700_000_200_000)),
                metadata: BTreeMap::new(),
                idempotency_key: IdempotencyKey::generate(),
            },
        )
        .await
        .expect("episode 2");

    // Same evidence_id (content fingerprint deduplicated), TWO distinct episode events!
    assert_eq!(ep1.evidence.evidence_id, ep2.evidence.evidence_id);
    assert_ne!(ep1.event_id, ep2.event_id);
    assert!(!ep1.idempotent_replay);
    assert!(!ep2.idempotent_replay);

    let ev1 = svc.get_event(&principal, ep1.event_id).await.expect("ev1");
    let ev2 = svc.get_event(&principal, ep2.event_id).await.expect("ev2");
    assert_eq!(
        ev1.observed_at,
        Some(TimestampMs::from_millis(1_700_000_100_000))
    );
    assert_eq!(
        ev2.observed_at,
        Some(TimestampMs::from_millis(1_700_000_200_000))
    );
    assert_eq!(ev1.correlation_id, cid1);
    assert_eq!(ev2.correlation_id, cid2);

    let (pg, handle) = connect_raw_pg(&target).await;
    let ev_row_count: i64 = pg
        .query_one(
            "SELECT COUNT(*)::bigint FROM evidence_objects WHERE scope = $1",
            &[&scope.as_str()],
        )
        .await
        .expect("count evidence")
        .get(0);
    assert_eq!(
        ev_row_count, 1,
        "must store exactly 1 deduplicated evidence row"
    );

    drop(pg);
    handle.abort();
}

// ============================================================================
// 3.4 Consolidation: Checkpoint/Resume, Caveats Preserved, Interruption No Data Loss
// ============================================================================

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "integration test validates partial consolidation checkpoint, pause/resume, caveats preservation, and canonical durability"
)]
async fn d3_consolidation_checkpoint_resume_caveats_and_interruption_safety() {
    let target = pg_target();
    let svc = MemoryService::new(target.clone()).expect("service");
    let scope = unique_scope("d3-consol");
    let principal = full_principal("agent:d3-consol", std::slice::from_ref(&scope));
    let cid = CorrelationId::generate();

    // Seed canonical evidence + 2 topics with claims carrying explicit caveats.
    let ev = svc
        .ingest_evidence(
            &principal,
            &cid,
            IngestEvidenceRequest {
                scope: scope.clone(),
                media_type: String::from("text/plain"),
                content: String::from("Canonical primary evidence that must never be touched"),
                source_uri: None,
                observed_at: None,
                metadata: BTreeMap::new(),
                idempotency_key: IdempotencyKey::generate(),
            },
        )
        .await
        .expect("ev");

    let mut c1 = make_claim_req(&scope, "alpha-topic", "Use Rust for all backend services");
    c1.caveats = vec![
        String::from("Except ops/runner CLI which uses Python uv"),
        String::from("Except desktop frontend UI which uses TypeScript React"),
    ];
    svc.create_claim(&principal, &cid, c1).await.expect("c1");

    let mut c2 = make_claim_req(&scope, "beta-topic", "PostgreSQL 16 is required");
    c2.caveats = vec![String::from("Local offline client cache may use SQLite")];
    svc.create_claim(&principal, &cid, c2).await.expect("c2");

    // Step-limited execution simulates interruption after topic 1 (`max_steps = 1`).
    let partial_job = svc
        .create_consolidation_job_with_step_limit(
            &principal,
            &cid,
            CreateConsolidationJobRequest {
                scope: scope.clone(),
                job_type: ConsolidationJobType::DossierUpdate,
                idempotency_key: Some(IdempotencyKey::generate()),
            },
            1,
        )
        .await
        .expect("step-limited job");

    assert_eq!(partial_job.status, ConsolidationJobStatus::Paused);
    assert!(partial_job.caveats_preserved);
    let cp_val: Value = serde_json::from_str(
        partial_job
            .checkpoint_json
            .as_deref()
            .expect("checkpoint json present"),
    )
    .expect("parse checkpoint");
    assert_eq!(cp_val["incomplete"], json!(true));
    assert_eq!(cp_val["processed_topics"].as_array().expect("arr").len(), 1);

    // Partial projection is marked incomplete; canonical evidence is completely untouched.
    let partial_docs = svc
        .list_documents(&principal, Some(&scope))
        .await
        .expect("partial docs");
    assert_eq!(partial_docs.documents.len(), 1);
    assert!(
        partial_docs.documents[0]
            .current_body
            .contains("[INCOMPLETE CONSOLIDATION — RESUME PENDING]")
    );
    let intact_ev = svc
        .get_evidence(&principal, ev.evidence.evidence_id)
        .await
        .expect("canonical evidence intact");
    assert_eq!(
        intact_ev.content,
        "Canonical primary evidence that must never be touched"
    );

    // Resume continues strictly from saved checkpoint and finishes remaining topic!
    let finished_job = svc
        .resume_consolidation_job(&principal, &cid, partial_job.job_id)
        .await
        .expect("resume job");
    assert_eq!(finished_job.status, ConsolidationJobStatus::Completed);
    assert!(finished_job.caveats_preserved);
    assert!(finished_job.completed_at.is_some());
    let cp_final: Value = serde_json::from_str(
        finished_job
            .checkpoint_json
            .as_deref()
            .expect("final checkpoint"),
    )
    .expect("parse final cp");
    assert_eq!(cp_final["incomplete"], json!(false));
    assert_eq!(
        cp_final["processed_topics"].as_array().expect("arr").len(),
        2
    );

    // Verify caveats are preserved verbatim in BOTH generated topic dossiers!
    let all_docs = svc
        .list_documents(&principal, Some(&scope))
        .await
        .expect("all docs");
    assert_eq!(all_docs.documents.len(), 2);
    let alpha_doc = all_docs
        .documents
        .iter()
        .find(|d| d.document_key == "dossier:alpha-topic")
        .expect("alpha dossier");
    assert!(
        alpha_doc
            .current_body
            .contains("Except ops/runner CLI which uses Python uv")
    );
    assert!(
        alpha_doc
            .current_body
            .contains("Except desktop frontend UI which uses TypeScript React")
    );

    let beta_doc = all_docs
        .documents
        .iter()
        .find(|d| d.document_key == "dossier:beta-topic")
        .expect("beta dossier");
    assert!(
        beta_doc
            .current_body
            .contains("Local offline client cache may use SQLite")
    );
}

// ============================================================================
// 3.5 Forgetting: Propagation, No Resurrection, Audit Tombstone & Trigger Guard
// ============================================================================

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "integration test validates forget propagation, revision scrub, index purge, no resurrection after rebuild, and content-free tombstones"
)]
async fn d3_forget_propagation_no_resurrection_and_content_free_tombstone() {
    let target = pg_target();
    let svc = MemoryService::new(target.clone()).expect("service");
    let scope = unique_scope("d3-forget");
    let principal = full_principal("agent:d3-forget", std::slice::from_ref(&scope));
    let cid = CorrelationId::generate();

    let secret_phrase = "UltraSensitivePersonalSecret998877";
    let note = svc
        .create_note(
            &principal,
            &cid,
            make_note_req(&scope, "Private note", &format!("Contains {secret_phrase}")),
        )
        .await
        .expect("note");
    svc.correct_note(
        &principal,
        &cid,
        CorrectNoteRequest {
            note_id: note.note.note_id,
            scope: scope.clone(),
            expected_revision: 1,
            title: String::from("Private note rev 2"),
            body: format!("Still mentions {secret_phrase}"),
            tags: vec![String::from("private")],
            rationale: format!("Rationale with {secret_phrase}"),
            evidence_links: vec![],
            author_label: None,
            idempotency_key: IdempotencyKey::generate(),
        },
    )
    .await
    .expect("rev 2");

    // Verify append-only trigger STILL blocks arbitrary UPDATE and DELETE on memory_note_revisions!
    let (pg, handle) = connect_raw_pg(&target).await;
    let bad_upd = pg
        .execute(
            "UPDATE memory_note_revisions SET body = 'tampered' WHERE note_id = $1",
            &[&note.note.note_id.as_uuid()],
        )
        .await;
    assert!(
        bad_upd.is_err(),
        "arbitrary UPDATE on memory_note_revisions must fail"
    );
    let bad_del = pg
        .execute(
            "DELETE FROM memory_note_revisions WHERE note_id = $1",
            &[&note.note.note_id.as_uuid()],
        )
        .await;
    assert!(
        bad_del.is_err(),
        "DELETE on memory_note_revisions must fail"
    );

    // Execute forget.
    let forgot = svc
        .forget(
            &principal,
            &cid,
            ForgetRequest {
                scope: scope.clone(),
                target_type: ForgetTargetType::Note,
                target_id: note.note.note_id.as_uuid(),
                reason: String::from("Owner GDPR deletion request"),
                idempotency_key: IdempotencyKey::generate(),
            },
        )
        .await
        .expect("forget note");
    assert_eq!(
        forgot.propagation_status,
        ForgetPropagationStatus::Completed
    );
    assert_ne!(forgot.residual_backup_notice, "");

    // Direct lookup & history -> NotFound.
    assert!(matches!(
        svc.get_note_current(&principal, note.note.note_id).await,
        Err(DennettError::NotFound { .. })
    ));
    assert!(matches!(
        svc.get_note_history(&principal, note.note.note_id).await,
        Err(DennettError::NotFound { .. })
    ));

    // Search (current AND include_history AND after marking index degraded for fresh tail) -> 0 hits!
    for include_history in [false, true] {
        let hits = svc
            .search_notes(
                &principal,
                SearchNotesRequest {
                    scope: Some(scope.clone()),
                    query: Some(String::from(secret_phrase)),
                    limit: Some(20),
                    include_history,
                },
            )
            .await
            .expect("search after forget");
        assert!(hits.notes.is_empty(), "forgotten note must never resurface");
    }

    // Raw DB check: ZERO occurrences of secret_phrase in memory_notes, memory_note_revisions,
    // memory_search_index, or memory_forget_tombstones!
    let rev_leaks: i64 = pg
        .query_one(
            "SELECT COUNT(*)::bigint FROM memory_note_revisions
             WHERE note_id = $1 AND (title <> '[forgotten]' OR body <> '[forgotten]' OR rationale IS NOT NULL)",
            &[&note.note.note_id.as_uuid()],
        )
        .await
        .expect("check revisions")
        .get(0);
    assert_eq!(
        rev_leaks, 0,
        "all note revisions must be scrubbed to [forgotten]"
    );

    let idx_leaks: i64 = pg
        .query_one(
            "SELECT COUNT(*)::bigint FROM memory_search_index WHERE source_id = $1",
            &[&note.note.note_id.as_uuid()],
        )
        .await
        .expect("check index")
        .get(0);
    assert_eq!(idx_leaks, 0, "search index must be purged immediately");

    // Audit tombstone has principal, reason, notice, and ZERO forgotten content.
    // Poll briefly so cluster-wide V15 `pg_snapshot_xmin` settles if sibling nextest tests hold a tx.
    let tomb_deadline = time::Instant::now() + Duration::from_secs(5);
    let tombstones = loop {
        let resp = svc
            .list_tombstones(&principal, Some(&scope))
            .await
            .expect("list tombstones");
        if !resp.tombstones.is_empty() || time::Instant::now() >= tomb_deadline {
            break resp;
        }
        time::sleep(Duration::from_millis(25)).await;
    };
    assert_eq!(tombstones.tombstones.len(), 1);
    let tomb = &tombstones.tombstones[0];
    assert_eq!(tomb.tombstone_id, forgot.tombstone_id);
    assert_eq!(tomb.principal_id, principal.principal_id);
    assert_eq!(tomb.reason, "Owner GDPR deletion request");
    assert!(
        !serde_json::to_string(tomb)
            .expect("json")
            .contains(secret_phrase)
    );

    // Export excludes forgotten note completely.
    let exported = svc
        .export_package(
            &principal,
            &cid,
            ExportRequest {
                scope,
                format_version: None,
            },
        )
        .await
        .expect("export");
    assert!(!exported.package_json.contains(secret_phrase));
    assert_eq!(exported.item_count, 0);

    drop(pg);
    handle.abort();
}

// ============================================================================
// 3.6 Cross-Scope Denial (Claims, Forget, Export)
// ============================================================================

#[tokio::test]
async fn d3_cross_scope_claim_forget_and_export_denied() {
    let target = pg_target();
    let svc = MemoryService::new(target).expect("service");
    let scope_a = unique_scope("d3-cs-a");
    let scope_b = unique_scope("d3-cs-b");
    let alpha = full_principal("agent:alpha", std::slice::from_ref(&scope_a));
    let beta = full_principal("agent:beta", std::slice::from_ref(&scope_b));
    let cid = CorrelationId::generate();

    let beta_claim = svc
        .create_claim(
            &beta,
            &cid,
            make_claim_req(&scope_b, "secret-topic", "Beta secret"),
        )
        .await
        .expect("beta claim");

    // Alpha cannot create or read claim in Beta.
    assert!(matches!(
        svc.create_claim(&alpha, &cid, make_claim_req(&scope_b, "t", "denied"))
            .await,
        Err(DennettError::ForbiddenScope { .. })
    ));
    let read_foreign = svc.get_claim(&alpha, beta_claim.claim_id).await;
    assert!(matches!(
        read_foreign,
        Err(DennettError::ForbiddenScope { ref scope, .. }) if scope != scope_b.as_str()
    ));

    // Alpha cannot forget in Beta.
    assert!(matches!(
        svc.forget(
            &alpha,
            &cid,
            ForgetRequest {
                scope: scope_b.clone(),
                target_type: ForgetTargetType::Claim,
                target_id: beta_claim.claim_id.as_uuid(),
                reason: String::from("cross-scope attack"),
                idempotency_key: IdempotencyKey::generate(),
            }
        )
        .await,
        Err(DennettError::ForbiddenScope { .. })
    ));

    // Alpha cannot export Beta.
    assert!(matches!(
        svc.export_package(
            &alpha,
            &cid,
            ExportRequest {
                scope: scope_b,
                format_version: None,
            }
        )
        .await,
        Err(DennettError::ForbiddenScope { .. })
    ));
}

// ============================================================================
// 3.7 Portable Export / Import (Contract J)
// ============================================================================

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "integration test validates deterministic Contract J export, secret scrubbing, scope isolation, untrusted import, and malicious payload rejection"
)]
async fn d3_export_deterministic_secret_free_scope_filtered_and_import_untrusted_validation() {
    let target = pg_target();
    let svc = MemoryService::new(target).expect("service");
    let scope_a = unique_scope("d3-exp-a");
    let scope_b = unique_scope("d3-exp-b");
    let scope_dest = unique_scope("d3-imp-dest");
    let principal = full_principal(
        "agent:d3-port",
        &[scope_a.clone(), scope_b.clone(), scope_dest.clone()],
    );
    let cid = CorrelationId::generate();

    // Seed scope A with notes, claims, documents including secret patterns that MUST be scrubbed.
    svc.create_note(
        &principal,
        &cid,
        make_note_req(
            &scope_a,
            "Deploy config",
            "Use dnt1.eyJ0b2tlbiI6Inh5eiJ9.sig1234567890 and dct1.payload1234.sig12345678 \
             GEMINI_API_KEY=AIzaSyDummySecretKey1234567890 Authorization: Bearer topsecret_bearer_99 \
             ghp_1234567890abcdefghij sk-1234567890abcdefghij",
        ),
    )
    .await
    .expect("secret note");
    svc.create_claim(
        &principal,
        &cid,
        make_claim_req(&scope_a, "arch", "Standalone memoryd is mandatory"),
    )
    .await
    .expect("claim A");
    svc.create_document(
        &principal,
        &cid,
        CreateDocumentRequest {
            scope: scope_a.clone(),
            document_key: String::from("dossier:arch"),
            title: String::from("Architecture"),
            body: String::from("Clean portable architecture dossier"),
            provenance_claim_ids: vec![],
            provenance_note_ids: vec![],
            idempotency_key: IdempotencyKey::generate(),
        },
    )
    .await
    .expect("doc A");

    // Seed scope B with distinct data that MUST NOT appear in scope A export.
    svc.create_note(
        &principal,
        &cid,
        make_note_req(&scope_b, "Foreign Scope B", "FOREIGN_SCOPE_B_SECRET_MARKER"),
    )
    .await
    .expect("note B");

    // 1. Deterministic export: two calls produce identical manifest_sha256 & package_json.
    let exp1 = svc
        .export_package(
            &principal,
            &cid,
            ExportRequest {
                scope: scope_a.clone(),
                format_version: None,
            },
        )
        .await
        .expect("export 1");
    let exp2 = svc
        .export_package(
            &principal,
            &cid,
            ExportRequest {
                scope: scope_a,
                format_version: None,
            },
        )
        .await
        .expect("export 2");
    assert_eq!(exp1.manifest_sha256, exp2.manifest_sha256);
    assert_eq!(exp1.package_json, exp2.package_json);
    assert!(exp1.secrets_stripped);
    assert_eq!(exp1.item_count, 3);

    // 2. Secret-free & scope-filtered.
    for forbidden in [
        "dnt1.",
        "dct1.",
        "GEMINI_API_KEY",
        "AIza",
        "Bearer ",
        "topsecret_bearer_99",
        "ghp_",
        "sk-123456",
        "FOREIGN_SCOPE_B_SECRET_MARKER",
    ] {
        assert!(
            !exp1.package_json.contains(forbidden),
            "export leaked forbidden substring `{forbidden}`: {}",
            exp1.package_json
        );
    }

    // 3. Import into fresh scope_dest -> untrusted + instruction_active = false.
    let imp = svc
        .import_package(
            &principal,
            &cid,
            ImportRequest {
                scope: scope_dest.clone(),
                package_json: exp1.package_json.clone(),
            },
        )
        .await
        .expect("import");
    assert_eq!(imp.trust_level, TrustLevel::ExternalUntrusted);
    assert!(!imp.instruction_active);
    assert_eq!(imp.items_imported, 3);
    assert_eq!(imp.items_rejected, 0);

    let dest_notes = svc
        .search_notes(
            &principal,
            SearchNotesRequest {
                scope: Some(scope_dest.clone()),
                query: None,
                limit: Some(10),
                include_history: false,
            },
        )
        .await
        .expect("dest notes");
    assert_eq!(dest_notes.notes.len(), 1);
    assert_eq!(
        dest_notes.notes[0].trust_level,
        TrustLevel::ExternalUntrusted
    );
    assert!(!dest_notes.notes[0].instruction_active);

    let dest_claims = svc
        .list_claims(&principal, Some(&scope_dest), None)
        .await
        .expect("dest claims");
    assert_eq!(dest_claims.claims.len(), 1);
    assert_eq!(
        dest_claims.claims[0].trust_level,
        TrustLevel::ExternalUntrusted
    );
    assert!(!dest_claims.claims[0].instruction_active);

    let dest_docs = svc
        .list_documents(&principal, Some(&scope_dest))
        .await
        .expect("dest docs");
    assert_eq!(dest_docs.documents.len(), 1);
    assert_eq!(
        dest_docs.documents[0].trust_level,
        TrustLevel::ExternalUntrusted
    );
    assert!(!dest_docs.documents[0].instruction_active);

    // 4. Roundtrip export(scope_dest) has IDENTICAL manifest_sha256 to export(scope_a)!
    let exp_roundtrip = svc
        .export_package(
            &principal,
            &cid,
            ExportRequest {
                scope: scope_dest.clone(),
                format_version: None,
            },
        )
        .await
        .expect("roundtrip export");
    assert_eq!(
        exp_roundtrip.manifest_sha256, exp1.manifest_sha256,
        "export -> import -> export must preserve identical manifest_sha256"
    );

    // 5. Validation on bad path traversal, excessive depth, and oversized (> 8 MiB) package!
    let bad_path_json = serialize_canonical_package(PortablePackagePayload {
        notes: vec![],
        claims: vec![],
        documents: vec![ExportedDocument {
            body: String::from("evil"),
            document_key: String::from("../escape/secret"),
            title: String::from("bad"),
        }],
    })
    .expect("serialize bad path");
    let bad_path_res = svc
        .import_package(
            &principal,
            &cid,
            ImportRequest {
                scope: scope_dest.clone(),
                package_json: bad_path_json,
            },
        )
        .await
        .expect("import validation returns ImportResponse");
    assert_eq!(bad_path_res.items_imported, 0);
    assert!(bad_path_res.items_rejected > 0);
    assert_ne!(bad_path_res.validation_errors, Vec::<String>::new());

    let deep_json = format!("{}{}", "[".repeat(40), "]".repeat(40));
    let deep_res = svc
        .import_package(
            &principal,
            &cid,
            ImportRequest {
                scope: scope_dest.clone(),
                package_json: deep_json,
            },
        )
        .await
        .expect("deep import rejected cleanly");
    assert!(deep_res.items_rejected > 0);
    assert_ne!(deep_res.validation_errors, Vec::<String>::new());

    let oversized = format!(
        "{{\"format_version\":\"1.0\",\"pad\":\"{}\"}}",
        "x".repeat(MAX_IMPORT_SAFE_BYTES + 128)
    );
    let over_res = svc
        .import_package(
            &principal,
            &cid,
            ImportRequest {
                scope: scope_dest,
                package_json: oversized,
            },
        )
        .await
        .expect("oversized import rejected cleanly");
    assert_eq!(over_res.items_imported, 0);
    assert!(over_res.items_rejected > 0);
    assert_ne!(over_res.validation_errors, Vec::<String>::new());
}

// ============================================================================
// 3.9 Fault Injection: Concurrent Supersession, Index Corruption Recovery, PG Down
// ============================================================================

#[tokio::test]
async fn d3_fi_concurrent_supersession() {
    let target = pg_target();
    let svc = MemoryService::new(target).expect("service");
    let scope = unique_scope("d3-fi-sup");
    let principal = full_principal("agent:d3-fi-sup", std::slice::from_ref(&scope));
    let cid = CorrelationId::generate();

    let base = svc
        .create_claim(
            &principal,
            &cid,
            make_claim_req(&scope, "race-topic", "Initial claim"),
        )
        .await
        .expect("base claim");

    let mut set = JoinSet::new();
    for idx in 0..6_u32 {
        let svc_c = svc.clone();
        let p = principal.clone();
        let c = cid.clone();
        let sc = scope.clone();
        let base_id = base.claim_id;
        set.spawn(async move {
            svc_c
                .supersede_claim(
                    &p,
                    &c,
                    SupersedeClaimRequest {
                        claim_id: base_id,
                        scope: sc.clone(),
                        new_claim: make_claim_req(
                            &sc,
                            "race-topic",
                            &format!("Concurrent replacement #{idx}"),
                        ),
                        resolution_event_id: None,
                    },
                )
                .await
        });
    }

    let mut wins = 0_usize;
    let mut conflicts = 0_usize;
    while let Some(res) = set.join_next().await {
        match res.expect("join") {
            Ok(_) => wins += 1,
            Err(DennettError::Conflict { .. }) => conflicts += 1,
            other => panic!("unexpected result in concurrent supersession: {other:?}"),
        }
    }
    assert_eq!(wins, 1, "exactly one concurrent supersession must win");
    assert_eq!(conflicts, 5, "all five losers must receive Conflict");
}

#[tokio::test]
async fn d3_fi_index_corruption_recovery_and_pg_down_consolidation() {
    let target = pg_target();
    let svc = MemoryService::new(target.clone()).expect("service");
    let scope = unique_scope("d3-fi-idx");
    let principal = full_principal("agent:d3-fi-idx", std::slice::from_ref(&scope));
    let cid = CorrelationId::generate();

    let note = svc
        .create_note(
            &principal,
            &cid,
            make_note_req(
                &scope,
                "Fault recovery note",
                "Must survive index truncation",
            ),
        )
        .await
        .expect("note");

    // Simulate index corruption/truncation for this scope.
    let (pg, handle) = connect_raw_pg(&target).await;
    pg.execute(
        "DELETE FROM memory_search_index WHERE scope = $1",
        &[&scope.as_str()],
    )
    .await
    .expect("corrupt/truncate index");

    let degraded_search = svc
        .search_notes(
            &principal,
            SearchNotesRequest {
                scope: Some(scope.clone()),
                query: Some(String::from("survive index truncation")),
                limit: Some(10),
                include_history: false,
            },
        )
        .await
        .expect("search works during corruption");
    assert_eq!(degraded_search.index_status, IndexStatus::Degraded);
    assert!(
        degraded_search
            .notes
            .iter()
            .any(|n| n.note_id == note.note.note_id)
    );

    let recovered = svc
        .trigger_index_rebuild(
            &principal,
            TriggerIndexRebuildRequest {
                scope: scope.clone(),
            },
        )
        .await
        .expect("rebuild recovery");
    assert_eq!(recovered.index_state.status, IndexStatus::Healthy);
    assert!(recovered.entries_rebuilt >= 1);

    // PG-down consolidation check: broken PG URL fails honestly without leaking password;
    // failed job in live PG recovers cleanly via resume_consolidation_job!
    let broken_secret = "D3_CONSOL_PG_DOWN_SECRET_4455";
    let broken_target = ValidatedPgTarget::parse(
        &format!("postgres://u:{broken_secret}@127.0.0.1:1/dennett_d3_test"),
        ProfileMode::TestIsolated,
    )
    .expect("valid broken target");
    let broken_svc = MemoryService::new(broken_target).expect("broken service");
    let down_err = broken_svc
        .create_consolidation_job(
            &principal,
            &cid,
            CreateConsolidationJobRequest {
                scope: scope.clone(),
                job_type: ConsolidationJobType::EpisodeSummary,
                idempotency_key: None,
            },
        )
        .await
        .expect_err("must fail when PG is down");
    assert!(matches!(down_err, DennettError::DatabaseUnavailable { .. }));
    assert!(!down_err.to_string().contains(broken_secret));

    // Simulate a consolidation job that transitioned to `failed` mid-flight with a saved checkpoint,
    // and prove `resume_consolidation_job` recovers it to `completed`!
    let failed_job_id = Uuid::new_v4();
    pg.execute(
        "INSERT INTO memory_consolidation_jobs (
            job_id, scope, job_type, status, checkpoint_data, error_detail, started_at
         ) VALUES ($1, $2, 'episode_summary', 'failed',
            '{\"processed_topics\":[],\"processed_ids\":[],\"last_stream_seq\":0,\"items_processed\":0,\"incomplete\":true,\"stage\":\"failed\"}'::jsonb,
            'simulated PG connection drop', now())",
        &[&failed_job_id, &scope.as_str()],
    )
    .await
    .expect("seed failed consolidation job");

    let resumed = svc
        .resume_consolidation_job(
            &principal,
            &cid,
            dennett_contracts::ConsolidationJobId::from_uuid(failed_job_id),
        )
        .await
        .expect("recover failed consolidation job");
    assert_eq!(resumed.status, ConsolidationJobStatus::Completed);
    assert!(resumed.error_detail.is_none());

    drop(pg);
    handle.abort();
}

// ============================================================================
// E2E Standalone Process Test: memoryd + MemoryClient + CLI + MCP (ZERO Head!)
// ============================================================================

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "end-to-end standalone D3 flow validates HTTP, CLI, and MCP against one live memoryd without Head"
)]
async fn d3_e2e_standalone_memoryd_http_cli_and_mcp_without_head() {
    let pg_url = require_memory_pg_url();
    let scope_a = unique_scope("d3-e2e-a");
    let scope_b = unique_scope("d3-e2e-b");
    let hmac_secret = "d3-e2e-standalone-hmac-secret-0123456789";
    let admin_tok = "tok_d3_e2e_admin_998877";

    let now_ms = TimestampMs::now().as_millis();
    let auth_config = AuthConfig {
        audience: String::from(DEFAULT_AUDIENCE),
        allowed_issuers: vec![String::from(DEFAULT_ISSUER)],
        hmac_secret: Some(String::from(hmac_secret)),
        tokens: vec![StaticTokenEntryConfig {
            token_id: String::from("d3-e2e-admin"),
            token: Some(String::from(admin_tok)),
            token_sha256: None,
            principal_id: PrincipalId::try_new("agent:d3-e2e").expect("principal"),
            issuer: String::from(DEFAULT_ISSUER),
            allowed_scopes: BTreeSet::from([scope_a.clone(), scope_b.clone()]),
            capabilities: BTreeSet::from([
                MemoryCapability::Read,
                MemoryCapability::Write,
                MemoryCapability::Admin,
                MemoryCapability::Export,
                MemoryCapability::Forget,
            ]),
            expires_at: None,
            revoked: false,
        }],
        revoked_token_ids: BTreeSet::new(),
    };

    let verifier = StandaloneAuthVerifier::from_config(auth_config.clone()).expect("verifier");
    let signed_tok = verifier
        .issue_signed_token(&SignedTokenClaims {
            token_id: String::from("d3-e2e-signed"),
            principal_id: PrincipalId::try_new("agent:d3-signed").expect("principal"),
            issuer: String::from(DEFAULT_ISSUER),
            audience: String::from(DEFAULT_AUDIENCE),
            allowed_scopes: BTreeSet::from([scope_a.clone(), scope_b.clone()]),
            capabilities: BTreeSet::from([
                MemoryCapability::Read,
                MemoryCapability::Write,
                MemoryCapability::Admin,
                MemoryCapability::Export,
                MemoryCapability::Forget,
            ]),
            issued_at: TimestampMs::from_millis(now_ms - 5_000),
            expires_at: TimestampMs::from_millis(now_ms + 3_600_000),
        })
        .expect("mint dnt1 token");

    let daemon = RunningMemoryd::spawn(&pg_url, &auth_config).await;
    let client = MemoryClient::new(&daemon.base_url, &signed_tok).expect("client");

    // 1. Healthz reports schema_version = 2.
    let health = client.healthz(None).await.expect("healthz");
    assert_eq!(health.schema_version, Some(LATEST_SCHEMA_VERSION));

    // 2. CLI: claim create, claim list, conflicting claim -> conflict list -> conflict resolve!
    let (c1_code, c1_out, c1_err) = run_cli(
        &daemon.base_url,
        admin_tok,
        &[
            "--json",
            "claim",
            "create",
            "--scope",
            scope_a.as_str(),
            "--topic-key",
            "runtime",
            "--claim-key",
            "default",
            "--text",
            "Use Antigravity ACP",
            "--caveat",
            "Except local loopback endpoint",
        ],
    )
    .await;
    assert_eq!(c1_code, 0, "CLI claim create failed: {c1_err}");
    let claim1: MemoryClaim = serde_json::from_str(c1_out.trim()).expect("claim1 json");

    // Create contradicting claim via MCP!
    let mut mcp = McpSession::spawn(&daemon.base_url, &signed_tok);
    let tools_list = mcp.request("tools/list", json!({})).await;
    assert_eq!(
        tools_list["result"]["tools"].as_array().expect("arr").len(),
        16
    );

    let (c2_err, c2_val) = mcp
        .call_tool(
            "memory_claim_create",
            json!({
                "scope": scope_a.as_str(),
                "topic_key": "runtime",
                "claim_key": "default",
                "natural_language": "Use Local HTTP Runtime"
            }),
        )
        .await;
    assert!(!c2_err, "MCP claim create failed: {c2_val}");
    let claim2: MemoryClaim = serde_json::from_value(c2_val).expect("claim2");
    assert_eq!(claim2.epistemic_status, EpistemicStatus::Contested);

    let (conf_err, conf_val) = mcp
        .call_tool("memory_conflict_list", json!({"scope": scope_a.as_str()}))
        .await;
    assert!(!conf_err);
    let conflicts = conf_val["conflicts"].as_array().expect("conflicts");
    assert_eq!(conflicts.len(), 1);
    let conflict_id = conflicts[0]["conflict_id"].as_str().expect("conflict_id");

    // Resolve conflict via CLI!
    let (res_code, res_out, res_err) = run_cli(
        &daemon.base_url,
        admin_tok,
        &[
            "--json",
            "conflict",
            "resolve",
            conflict_id,
            "--winning-claim",
            &claim1.claim_id.to_string(),
            "--note",
            "Confirmed Antigravity ACP default",
        ],
    )
    .await;
    assert_eq!(res_code, 0, "CLI conflict resolve failed: {res_err}");
    let resolved: MemoryConflict = serde_json::from_str(res_out.trim()).expect("resolved");
    assert_eq!(resolved.status, ConflictStatus::Resolved);

    // 3. MCP document write (create + update) & read + versions!
    let (doc_err, doc_val) = mcp
        .call_tool(
            "memory_document_write",
            json!({
                "scope": scope_a.as_str(),
                "document_key": "dossier:runtime",
                "title": "Runtime Dossier",
                "body": "Version 1 body",
                "provenance_claim_ids": [claim1.claim_id.to_string()]
            }),
        )
        .await;
    assert!(!doc_err, "MCP doc create failed: {doc_val}");
    let doc_v1: MemoryDocument = serde_json::from_value(doc_val).expect("doc v1");

    let (doc_upd_err, doc_upd_val) = mcp
        .call_tool(
            "memory_document_write",
            json!({
                "scope": scope_a.as_str(),
                "document_id": doc_v1.document_id.to_string(),
                "expected_version": 1,
                "title": "Runtime Dossier v2",
                "body": "Version 2 body",
                "diff_summary": "Refined via MCP"
            }),
        )
        .await;
    assert!(!doc_upd_err, "MCP doc update failed: {doc_upd_val}");
    let doc_v2: MemoryDocument = serde_json::from_value(doc_upd_val).expect("doc v2");
    assert_eq!(doc_v2.current_version, 2);

    let (ver_err, ver_val) = mcp
        .call_tool(
            "memory_document_read",
            json!({
                "document_id": doc_v1.document_id.to_string(),
                "include_versions": true
            }),
        )
        .await;
    assert!(!ver_err);
    assert_eq!(ver_val["versions"].as_array().expect("vers").len(), 2);

    // Verify wire compatibility for every D3 MemoryClient HTTP route:
    // list_claims, list_claims_by_topic, get_claim, supersede_claim, list_documents,
    // update_document optimistic-lock conflict, get_index_status, consolidation jobs!
    let listed_claims = client
        .list_claims(Some(&scope_a), None, None)
        .await
        .expect("list_claims");
    assert_eq!(listed_claims.claims.len(), 2);
    let by_topic = client
        .list_claims_by_topic(&scope_a, "runtime", None)
        .await
        .expect("list_claims_by_topic");
    assert_eq!(by_topic.claims.len(), 2);
    let fetched_claim1 = client
        .get_claim(claim1.claim_id, None)
        .await
        .expect("get_claim");
    assert_eq!(fetched_claim1.claim_id, claim1.claim_id);

    let superseded_http = client
        .supersede_claim(
            &SupersedeClaimRequest {
                claim_id: claim1.claim_id,
                scope: scope_a.clone(),
                new_claim: make_claim_req(&scope_a, "runtime", "Use Antigravity ACP v2"),
                resolution_event_id: None,
            },
            None,
        )
        .await
        .expect("supersede_claim over HTTP");
    assert_eq!(superseded_http.supersedes_claim_id, Some(claim1.claim_id));

    let listed_docs = client
        .list_documents(Some(&scope_a), None)
        .await
        .expect("list_documents");
    assert_eq!(listed_docs.documents.len(), 1);
    let stale_doc_err = client
        .update_document(
            &UpdateDocumentRequest {
                document_id: doc_v1.document_id,
                scope: scope_a.clone(),
                expected_version: 1,
                title: String::from("Stale overwrite"),
                body: String::from("Must fail optimistic lock"),
                diff_summary: None,
                provenance_claim_ids: vec![],
                provenance_note_ids: vec![],
                idempotency_key: IdempotencyKey::generate(),
            },
            None,
        )
        .await;
    assert!(matches!(stale_doc_err, Err(DennettError::Conflict { .. })));

    let idx_status_http = client
        .get_index_status(Some(&scope_a), None)
        .await
        .expect("get_index_status");
    assert_eq!(idx_status_http.status, IndexStatus::Healthy);
    let (idx_mcp_err, idx_mcp_val) = mcp
        .call_tool("memory_index_status", json!({"scope": scope_a.as_str()}))
        .await;
    assert!(!idx_mcp_err);
    assert_eq!(idx_mcp_val["status"], "healthy");

    let consol_job = client
        .create_consolidation_job(
            &CreateConsolidationJobRequest {
                scope: scope_a.clone(),
                job_type: ConsolidationJobType::DossierUpdate,
                idempotency_key: None,
            },
            None,
        )
        .await
        .expect("create_consolidation_job over HTTP");
    assert_eq!(consol_job.status, ConsolidationJobStatus::Completed);
    let consol_list = client
        .list_consolidation_jobs(Some(&scope_a), None)
        .await
        .expect("list_consolidation_jobs over HTTP");
    assert!(
        consol_list
            .jobs
            .iter()
            .any(|j| j.job_id == consol_job.job_id)
    );
    // Completed job rejects pause/resume with honest Conflict (verifies HTTP routes wired).
    assert!(matches!(
        client
            .pause_consolidation_job(consol_job.job_id, None)
            .await,
        Err(DennettError::Conflict { .. })
    ));
    assert!(matches!(
        client
            .resume_consolidation_job(consol_job.job_id, None)
            .await,
        Err(DennettError::Conflict { .. })
    ));

    // 4. Stats, index status, index rebuild via CLI & MemoryClient!
    let stats = client.get_stats(Some(&scope_a), None).await.expect("stats");
    assert_eq!(stats.active_claims_count, 1);
    assert!(stats.documents_count >= 1);
    assert_eq!(stats.open_conflicts_count, 0);
    assert_eq!(stats.index_status, IndexStatus::Healthy);

    let (rebuild_status, _, rebuild_stderr) = run_cli(
        &daemon.base_url,
        admin_tok,
        &["--json", "index", "rebuild", "--scope", scope_a.as_str()],
    )
    .await;
    assert_eq!(
        rebuild_status, 0,
        "CLI index rebuild failed: {rebuild_stderr}"
    );

    // 5. MCP export & import into scope B + >512 KiB HTTP body limit check on /v1/memory/import!
    let (exp_err, exp_val) = mcp
        .call_tool("memory_export", json!({"scope": scope_a.as_str()}))
        .await;
    assert!(!exp_err, "MCP export failed: {exp_val}");
    let pkg_json = exp_val["package_json"].as_str().expect("package_json");

    let (imp_err, imp_val) = mcp
        .call_tool(
            "memory_import",
            json!({"scope": scope_b.as_str(), "package_json": pkg_json}),
        )
        .await;
    assert!(!imp_err, "MCP import failed: {imp_val}");
    assert_eq!(imp_val["trust_level"], "external_untrusted");
    assert_eq!(imp_val["instruction_active"], false);

    // Verify >512 KiB payload reaches /v1/memory/import domain validator (not rejected 413 at 512 KiB).
    let large_payload = format!(
        "{{\"format_version\":\"1.0\",\"manifest_sha256\":\"{}\",\"payload\":{{\"notes\":[],\"claims\":[],\"documents\":[]}},\"pad\":\"{}\"}}",
        "0".repeat(64),
        "a".repeat(700 * 1024)
    );
    let large_imp = client
        .import_package(
            &ImportRequest {
                scope: scope_b.clone(),
                package_json: large_payload,
            },
            None,
        )
        .await
        .expect("700 KiB import payload must reach domain validator cleanly");
    assert!(large_imp.items_rejected > 0);

    // 6. Forget via MCP!
    let (fgt_err, fgt_val) = mcp
        .call_tool(
            "memory_forget",
            json!({
                "scope": scope_a.as_str(),
                "target_type": "document",
                "target_id": doc_v1.document_id.to_string(),
                "reason": "e2e cleanup"
            }),
        )
        .await;
    assert!(!fgt_err, "MCP forget failed: {fgt_val}");
    assert_eq!(fgt_val["propagation_status"], "completed");

    assert!(matches!(
        client.get_document(doc_v1.document_id, None).await,
        Err(DennettError::NotFound { .. })
    ));
    let tomb_deadline = time::Instant::now() + Duration::from_secs(5);
    let tombstones = loop {
        let resp = client
            .list_tombstones(Some(&scope_a), None)
            .await
            .expect("list_tombstones over HTTP");
        if resp
            .tombstones
            .iter()
            .any(|t| t.target_id == doc_v1.document_id.as_uuid())
            || time::Instant::now() >= tomb_deadline
        {
            break resp;
        }
        time::sleep(Duration::from_millis(25)).await;
    };
    assert!(
        tombstones
            .tombstones
            .iter()
            .any(|t| t.target_id == doc_v1.document_id.as_uuid())
    );

    mcp.close().await;
    daemon.shutdown().await;
}

/// Proves V15 commit-visibility barrier & monotonic cursor streaming on `memory_forget_tombstones`
/// (`tombstone_seq ASC`, delayed uncommitted transaction never creates a skipped-tombstone hole),
/// AND proves lost-ACK retry of a forgotten note's `idempotency_key` fails closed with `Conflict`
/// instead of resurrecting the note.
#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "end-to-end PG16 V15 commit-barrier test orchestrates concurrent uncommitted/committed transactions plus lost-ACK no-resurrection check"
)]
async fn d3_tombstone_stream_v15_commit_barrier_and_lost_ack_no_resurrection() {
    let target = pg_target();
    let svc = MemoryService::new(target.clone()).expect("connect service");
    svc.ensure_migrated().await.expect("apply v3 migration");

    let scope = unique_scope("v15-tomb");
    let principal = full_principal("v15-owner", std::slice::from_ref(&scope));
    let cid = CorrelationId::generate();

    // Create two notes and record baseline tombstone cursor.
    let n1_req = make_note_req(&scope, "V15 Note One", "Secret payload one");
    let n1 = svc
        .create_note(&principal, &cid, n1_req.clone())
        .await
        .expect("create n1");
    let n2 = svc
        .create_note(
            &principal,
            &cid,
            make_note_req(&scope, "V15 Note Two", "Secret payload two"),
        )
        .await
        .expect("create n2");

    let initial = svc
        .list_tombstones_since(&principal, Some(&scope), Some(0), Some(50))
        .await
        .expect("initial tombstone stream");
    assert_eq!(initial.tombstones.len(), 0);
    assert_eq!(initial.next_after_seq, Some(0));

    // Open a raw PG transaction `tx1` that allocates an EARLIER `tombstone_seq` and holds it UNCOMMITTED.
    // Reuses `n2.event_id` as the FK reference so `tx1` holds zero locks on `memory_stream_counters`
    // while `tx2` (`svc.forget` for `n1`) executes and commits concurrently!
    let (mut raw_pg, raw_handle) = connect_raw_pg(&target).await;
    let tx1 = raw_pg.transaction().await.expect("begin tx1");
    let tomb1_id = Uuid::new_v4();
    let fake_target_1 = Uuid::new_v4();
    let seq1: i64 = tx1
        .query_one(
            "INSERT INTO memory_forget_tombstones (
                tombstone_id, scope, target_type, target_id, forget_event_id,
                principal_id, reason, residual_backup_notice, propagation_status
             ) VALUES ($1, $2, 'note', $3, $4, $5, 'delayed tx1 forget', 'backup notice', 'completed')
             RETURNING tombstone_seq",
            &[
                &tomb1_id,
                &scope.as_str(),
                &fake_target_1,
                &n2.event_id.as_uuid(),
                &principal.principal_id.as_str(),
            ],
        )
        .await
        .expect("insert uncommitted tombstone")
        .get(0);

    // While `tx1` is STILL UNCOMMITTED, commit a LATER forget (`tx2`) for `n1` via `MemoryService`.
    let forgot_n1 = svc
        .forget(
            &principal,
            &cid,
            ForgetRequest {
                scope: scope.clone(),
                target_type: ForgetTargetType::Note,
                target_id: n1.note.note_id.as_uuid(),
                reason: String::from("committed tx2 forget"),
                idempotency_key: IdempotencyKey::generate(),
            },
        )
        .await
        .expect("forget n1 while tx1 is uncommitted");

    // V15 BARRIER PROOF: streaming with `after_seq = 0` MUST NOT leapfrog past uncommitted `seq1`
    // to deliver `forgot_n1` (which has `seq2 > seq1` and `commit_xid >= xmin(tx1)`)!
    let during_uncommitted = svc
        .list_tombstones_since(&principal, Some(&scope), Some(0), Some(50))
        .await
        .expect("poll during uncommitted tx1");
    assert!(
        during_uncommitted.tombstones.is_empty(),
        "V15 commit barrier must withhold later committed tombstone while earlier xid is uncommitted, got: {:?}",
        during_uncommitted.tombstones
    );
    assert_eq!(during_uncommitted.next_after_seq, Some(0));

    // Commit `tx1` -> BOTH tombstones become visible immediately once cluster-wide xmin passes
    // their commit_xids, in strict ascending `tombstone_seq` order!
    tx1.commit().await.expect("commit tx1");
    let after_commit_deadline = time::Instant::now() + Duration::from_secs(5);
    let after_commit = loop {
        let resp = svc
            .list_tombstones_since(&principal, Some(&scope), Some(0), Some(50))
            .await
            .expect("poll after tx1 commit");
        if resp.tombstones.len() == 2 || time::Instant::now() >= after_commit_deadline {
            break resp;
        }
        time::sleep(Duration::from_millis(25)).await;
    };
    assert_eq!(after_commit.tombstones.len(), 2);
    assert_eq!(after_commit.tombstones[0].tombstone_id.as_uuid(), tomb1_id);
    assert_eq!(after_commit.tombstones[0].tombstone_seq, seq1);
    assert_eq!(
        after_commit.tombstones[1].tombstone_id,
        forgot_n1.tombstone_id
    );
    let seq2 = after_commit.tombstones[1].tombstone_seq;
    assert!(seq2 > seq1, "tombstone_seq must increase monotonically");
    assert_eq!(after_commit.next_after_seq, Some(seq2));

    // Incremental pagination from `after_seq = seq1` returns ONLY `seq2`!
    let page_after_1 = svc
        .list_tombstones_since(&principal, Some(&scope), Some(seq1), Some(50))
        .await
        .expect("poll after seq1");
    assert_eq!(page_after_1.tombstones.len(), 1);
    assert_eq!(page_after_1.tombstones[0].tombstone_seq, seq2);
    assert_eq!(page_after_1.next_after_seq, Some(seq2));

    // Forget `n2` -> incremental poll from `seq2` yields ONLY `n2`'s tombstone!
    let forgot_n2 = svc
        .forget(
            &principal,
            &cid,
            ForgetRequest {
                scope: scope.clone(),
                target_type: ForgetTargetType::Note,
                target_id: n2.note.note_id.as_uuid(),
                reason: String::from("incremental forget n2"),
                idempotency_key: IdempotencyKey::generate(),
            },
        )
        .await
        .expect("forget n2");
    let after_n2_deadline = time::Instant::now() + Duration::from_secs(5);
    let page_after_2 = loop {
        let resp = svc
            .list_tombstones_since(&principal, Some(&scope), Some(seq2), Some(50))
            .await
            .expect("poll after seq2");
        if !resp.tombstones.is_empty() || time::Instant::now() >= after_n2_deadline {
            break resp;
        }
        time::sleep(Duration::from_millis(25)).await;
    };
    assert_eq!(page_after_2.tombstones.len(), 1);
    assert_eq!(
        page_after_2.tombstones[0].tombstone_id,
        forgot_n2.tombstone_id
    );
    assert!(page_after_2.tombstones[0].tombstone_seq > seq2);
    assert_eq!(
        page_after_2.next_after_seq,
        Some(page_after_2.tombstones[0].tombstone_seq)
    );

    // LOST-ACK NO-RESURRECTION PROOF: retrying `create_note` with `n1`'s original `idempotency_key`
    // MUST fail closed with `Conflict` and NEVER resurrect or return the forgotten note!
    let replay_forgotten = svc.create_note(&principal, &cid, n1_req).await;
    assert!(
        matches!(
            replay_forgotten,
            Err(DennettError::Conflict { ref reason, .. })
                if reason.contains("previously forgotten")
        ),
        "lost-ACK replay of forgotten note must return Conflict, got: {replay_forgotten:?}"
    );
    assert!(matches!(
        svc.get_note_current(&principal, n1.note.note_id).await,
        Err(DennettError::NotFound { .. })
    ));

    drop(raw_pg);
    raw_handle.abort();
}

// ============================================================================
// 3.8 Pure Property Tests (no DB required)
// ============================================================================

proptest! {
    #[test]
    fn proptest_claim_natural_language_validate_never_panics(
        text in "\\PC{0,512}",
        topic in "[a-z0-9-]{1,32}",
    ) {
        let scope = MemoryScope::try_new("prop-scope").expect("scope");
        let req = CreateClaimRequest {
            scope,
            topic_key: topic,
            claim_key: None,
            natural_language: text,
            epistemic_status: EpistemicStatus::Asserted,
            supersedes_claim_id: None,
            valid_from: None,
            valid_to: None,
            source_note_id: None,
            caveats: vec![],
            provenance: BTreeMap::new(),
            evidence_links: vec![],
            idempotency_key: IdempotencyKey::generate(),
        };
        let _ = req.validate();
    }

    #[test]
    fn proptest_import_parser_and_search_query_builder_never_panic(raw in "\\PC{0,1024}") {
        let _ = parse_and_validate_import_package(&raw);
        let _ = prepare_search_query(&raw);
    }
}
