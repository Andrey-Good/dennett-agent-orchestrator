//! End-to-end process integration tests for standalone `dennett-memoryd` + `MemoryClient` +
//! `dennett-memory-cli` + `dennett-memory-mcp` against real PostgreSQL (V01, V06, V19).
//!
//! Runs completely WITHOUT `dennett-head` or any UI component.
//! Requires `DENNETT_TEST_PROFILE=1` and `DENNETT_TEST_MEMORY_PG_URL`.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use dennett_contracts::{
    CorrectNoteRequest, CreateNoteRequest, DennettError, EvidenceAnchor, EvidenceLinkInput,
    EvidenceRelationship, IdempotencyKey, IngestEvidenceRequest, IngestEvidenceResponse,
    MemoryCapability, MemoryNote, MemoryScope, NoteHistoryResponse, NoteMutationResponse,
    PrincipalId, SearchNotesRequest, SearchNotesResponse, TimestampMs,
};
use dennett_memory_client::MemoryClient;
use dennett_memory_core::{
    AuthConfig, DEFAULT_AUDIENCE, DEFAULT_ISSUER, SignedTokenClaims, StandaloneAuthVerifier,
    StaticTokenEntryConfig,
};
use dennett_testkit::profile::TEST_PROFILE_ENV;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::time;
use uuid::Uuid;

fn require_memory_pg_url() -> String {
    assert_eq!(
        env::var(TEST_PROFILE_ENV).ok().as_deref(),
        Some("1"),
        "{TEST_PROFILE_ENV}=1 is required for Memory Fabric E2E tests"
    );
    match env::var("DENNETT_TEST_MEMORY_PG_URL") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => panic!("DENNETT_TEST_MEMORY_PG_URL is missing; run via `dennett-check integration`"),
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
        "failed to build `{bin_name}` binary for E2E tests"
    );

    find_binary_near_current_exe(&exe_name)
        .unwrap_or_else(|| panic!("could not locate compiled `{bin_name}` binary"))
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

        let stdout = child.stdout.take().expect("memoryd stdout piped");
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        match time::timeout(Duration::from_secs(10), reader.read_line(&mut line)).await {
            Ok(Ok(n)) if n > 0 => {}
            other => panic!("did not receive LISTENING line from memoryd: {other:?}"),
        }
        let addr = line
            .trim()
            .strip_prefix("LISTENING ")
            .unwrap_or_else(|| panic!("unexpected memoryd startup line: `{line}`"))
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
            .expect("connect for shutdown")
            .expect("tcp connect");
        stream
            .write_all(req.as_bytes())
            .await
            .expect("write shutdown request");
        let mut resp = Vec::new();
        let _ = time::timeout(Duration::from_secs(5), stream.read_to_end(&mut resp)).await;
        let status = time::timeout(Duration::from_secs(5), self.child.wait())
            .await
            .expect("wait shutdown exit")
            .expect("child wait");
        assert!(status.success(), "memoryd did not exit cleanly: {status}");
    }
}

struct McpSession {
    child: Child,
    stdin: tokio::process::ChildStdin,
    stdout: BufReader<tokio::process::ChildStdout>,
    next_id: i64,
}

impl McpSession {
    fn spawn(base_url: &str, token: Option<&str>) -> Self {
        let bin = locate_binary("dennett-memory-mcp", "dennett-memory-mcp");
        let mut cmd = Command::new(&bin);
        cmd.env("DENNETT_MEMORY_URL", base_url)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        match token {
            Some(tok) => {
                cmd.env("DENNETT_MEMORY_TOKEN", tok);
            }
            None => {
                cmd.env_remove("DENNETT_MEMORY_TOKEN");
            }
        }
        let mut child = cmd.spawn().expect("spawn dennett-memory-mcp");
        let stdin = child.stdin.take().expect("mcp stdin");
        let stdout = BufReader::new(child.stdout.take().expect("mcp stdout"));
        Self {
            child,
            stdin,
            stdout,
            next_id: 1,
        }
    }

    async fn send_raw_line(&mut self, line: &str) -> Value {
        self.stdin
            .write_all(line.as_bytes())
            .await
            .expect("write raw line to mcp");
        self.stdin
            .write_all(b"\n")
            .await
            .expect("write newline to mcp");
        self.stdin.flush().await.expect("flush mcp stdin");

        let mut resp_line = String::new();
        let read = time::timeout(
            Duration::from_secs(10),
            self.stdout.read_line(&mut resp_line),
        )
        .await
        .expect("mcp response timeout")
        .expect("read mcp stdout");
        assert!(read > 0, "unexpected EOF from dennett-memory-mcp");
        serde_json::from_str(resp_line.trim())
            .unwrap_or_else(|err| panic!("invalid MCP JSON `{resp_line}`: {err}"))
    }

    async fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });
        let resp = self.send_raw_line(&msg.to_string()).await;
        assert_eq!(resp["id"], json!(id));
        resp
    }

    async fn call_tool(&mut self, name: &str, arguments: Value) -> (bool, Value) {
        let resp = self
            .request(
                "tools/call",
                json!({
                    "name": name,
                    "arguments": arguments
                }),
            )
            .await;
        let is_error = resp["result"]["isError"].as_bool().unwrap_or(true);
        let text = resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("missing MCP tool text content: {resp}"));
        let parsed: Value = serde_json::from_str(text)
            .unwrap_or_else(|err| panic!("MCP tool text was not JSON `{text}`: {err}"));
        (is_error, parsed)
    }

    async fn close(mut self) {
        drop(self.stdin);
        let status = time::timeout(Duration::from_secs(5), self.child.wait())
            .await
            .expect("mcp exit timeout")
            .expect("mcp wait");
        assert!(
            status.success(),
            "dennett-memory-mcp exited abnormally: {status}"
        );
    }
}

async fn run_cli(base_url: &str, token: Option<&str>, args: &[&str]) -> (i32, String, String) {
    let bin = locate_binary("dennett-memory-cli", "dennett-memory-client");
    let mut cmd = Command::new(&bin);
    cmd.args(args).env("DENNETT_MEMORY_URL", base_url);
    match token {
        Some(tok) => {
            cmd.env("DENNETT_MEMORY_TOKEN", tok);
        }
        None => {
            cmd.env_remove("DENNETT_MEMORY_TOKEN");
        }
    }
    let output = time::timeout(Duration::from_secs(10), cmd.output())
        .await
        .expect("cli timeout")
        .expect("run dennett-memory-cli");
    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (code, stdout, stderr)
}

fn unique_scope(prefix: &str) -> MemoryScope {
    let suffix = &Uuid::new_v4().simple().to_string()[..12];
    MemoryScope::try_new(format!("{prefix}-{suffix}")).expect("valid unique scope")
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "end-to-end V01/V06/V19 scenario exercises Client, CLI, and MCP against one live daemon lifecycle"
)]
async fn v01_v06_v19_standalone_memoryd_consistent_across_client_cli_and_mcp() {
    let pg_url = require_memory_pg_url();
    let scope_alpha = unique_scope("e2e-alpha");
    let scope_beta = unique_scope("e2e-beta");

    let alpha_rw_tok = "tok_e2e_alpha_rw_839201";
    let alpha_readonly_tok = "tok_e2e_alpha_ro_839202";
    let beta_rw_tok = "tok_e2e_beta_rw_839203";
    let expired_tok = "tok_e2e_expired_839204";
    let revoked_tok = "tok_e2e_revoked_839205";
    let hmac_secret = "e2e-standalone-hmac-secret-key-0123456789";

    let now_ms = TimestampMs::now().as_millis();
    let auth_config = AuthConfig {
        audience: String::from(DEFAULT_AUDIENCE),
        allowed_issuers: vec![String::from(DEFAULT_ISSUER)],
        hmac_secret: Some(String::from(hmac_secret)),
        tokens: vec![
            StaticTokenEntryConfig {
                token_id: String::from("e2e-alpha-rw"),
                token: Some(String::from(alpha_rw_tok)),
                token_sha256: None,
                principal_id: PrincipalId::try_new("agent:alpha-rw").expect("principal"),
                issuer: String::from(DEFAULT_ISSUER),
                allowed_scopes: BTreeSet::from([scope_alpha.clone()]),
                capabilities: BTreeSet::from([MemoryCapability::Read, MemoryCapability::Write]),
                expires_at: Some(TimestampMs::from_millis(now_ms + 3_600_000)),
                revoked: false,
            },
            StaticTokenEntryConfig {
                token_id: String::from("e2e-alpha-ro"),
                token: Some(String::from(alpha_readonly_tok)),
                token_sha256: None,
                principal_id: PrincipalId::try_new("agent:alpha-ro").expect("principal"),
                issuer: String::from(DEFAULT_ISSUER),
                allowed_scopes: BTreeSet::from([scope_alpha.clone()]),
                capabilities: BTreeSet::from([MemoryCapability::Read]),
                expires_at: None,
                revoked: false,
            },
            StaticTokenEntryConfig {
                token_id: String::from("e2e-beta-rw"),
                token: Some(String::from(beta_rw_tok)),
                token_sha256: None,
                principal_id: PrincipalId::try_new("agent:beta-rw").expect("principal"),
                issuer: String::from(DEFAULT_ISSUER),
                allowed_scopes: BTreeSet::from([scope_beta.clone()]),
                capabilities: BTreeSet::from([MemoryCapability::Read, MemoryCapability::Write]),
                expires_at: None,
                revoked: false,
            },
            StaticTokenEntryConfig {
                token_id: String::from("e2e-expired"),
                token: Some(String::from(expired_tok)),
                token_sha256: None,
                principal_id: PrincipalId::try_new("agent:expired").expect("principal"),
                issuer: String::from(DEFAULT_ISSUER),
                allowed_scopes: BTreeSet::from([scope_alpha.clone()]),
                capabilities: BTreeSet::from([MemoryCapability::Read, MemoryCapability::Write]),
                expires_at: Some(TimestampMs::from_millis(now_ms - 1_000)),
                revoked: false,
            },
            StaticTokenEntryConfig {
                token_id: String::from("e2e-revoked"),
                token: Some(String::from(revoked_tok)),
                token_sha256: None,
                principal_id: PrincipalId::try_new("agent:revoked").expect("principal"),
                issuer: String::from(DEFAULT_ISSUER),
                allowed_scopes: BTreeSet::from([scope_alpha.clone()]),
                capabilities: BTreeSet::from([MemoryCapability::Read, MemoryCapability::Write]),
                expires_at: None,
                revoked: true,
            },
        ],
        revoked_token_ids: BTreeSet::new(),
    };

    // Also mint a real HMAC-SHA256 signed scoped token (`dnt1.*`).
    let issuer_verifier =
        StandaloneAuthVerifier::from_config(auth_config.clone()).expect("build issuer verifier");
    let signed_alpha_tok = issuer_verifier
        .issue_signed_token(&SignedTokenClaims {
            token_id: String::from("jti-e2e-signed-alpha"),
            principal_id: PrincipalId::try_new("agent:signed-alpha").expect("principal"),
            issuer: String::from(DEFAULT_ISSUER),
            audience: String::from(DEFAULT_AUDIENCE),
            allowed_scopes: BTreeSet::from([scope_alpha.clone()]),
            capabilities: BTreeSet::from([MemoryCapability::Read, MemoryCapability::Write]),
            issued_at: TimestampMs::from_millis(now_ms - 5_000),
            expires_at: TimestampMs::from_millis(now_ms + 3_600_000),
        })
        .expect("mint signed token");

    let daemon = RunningMemoryd::spawn(&pg_url, &auth_config).await;
    let base_url = daemon.base_url.clone();

    // 1. CLI --help works without requiring auth or server.
    let (help_code, help_out, _) = run_cli(&base_url, None, &["--help"]).await;
    assert_eq!(help_code, 0);
    assert!(help_out.contains("DENNETT_MEMORY_TOKEN"));

    // 2. Ingest evidence via `dennett-memory-cli ingest-evidence --json`.
    let (ing_code, ing_out, ing_err) = run_cli(
        &base_url,
        Some(alpha_rw_tok),
        &[
            "--json",
            "ingest-evidence",
            "--scope",
            scope_alpha.as_str(),
            "--media-type",
            "text/markdown",
            "--content",
            "Исходный документ: PostgreSQL 16 обязателен. Вторая цитата: memoryd без Head.",
            "--source-uri",
            "docs/architecture/81.md",
            "--idempotency-key",
            "e2e-ev-1",
        ],
    )
    .await;
    assert_eq!(ing_code, 0, "CLI ingest-evidence failed: stderr={ing_err}");
    let ingested: IngestEvidenceResponse =
        serde_json::from_str(ing_out.trim()).expect("parse ingest JSON");
    assert!(!ingested.idempotent_replay);

    // 3. Create note with 2 V19 anchors + NULL ordinal on the SAME evidence via HMAC-signed MemoryClient.
    let signed_client =
        MemoryClient::new(&base_url, &signed_alpha_tok).expect("signed MemoryClient");
    let supports = EvidenceRelationship::try_new("supports").expect("rel");
    let create_key = IdempotencyKey::try_new("e2e-create-note-1").expect("key");
    let create_req = CreateNoteRequest {
        scope: scope_alpha.clone(),
        title: String::from("Архитектурное решение PG16"),
        body: String::from("Первоначальная версия заметки: standalone memoryd."),
        tags: vec![String::from("postgres"), String::from("v01")],
        observed_at: None,
        evidence_links: vec![
            EvidenceLinkInput {
                evidence_id: ingested.evidence.evidence_id,
                relationship: supports.clone(),
                anchor: Some(EvidenceAnchor {
                    kind: String::from("line_range"),
                    start: Some(1),
                    end: Some(1),
                    locator: Some(String::from("PostgreSQL 16")),
                    attributes: BTreeMap::new(),
                }),
                ordinal: None,
            },
            EvidenceLinkInput {
                evidence_id: ingested.evidence.evidence_id,
                relationship: supports,
                anchor: Some(EvidenceAnchor {
                    kind: String::from("line_range"),
                    start: Some(2),
                    end: Some(2),
                    locator: Some(String::from("memoryd без Head")),
                    attributes: BTreeMap::new(),
                }),
                ordinal: None,
            },
        ],
        // Forged display label must NEVER override verified principal `agent:signed-alpha`!
        author_label: Some(String::from("spoofed-super-admin")),
        idempotency_key: create_key.clone(),
        admission_status: None,
        salience: None,
        topic_key: None,
    };

    let created = signed_client
        .create_note(&create_req, None)
        .await
        .expect("create note via signed MemoryClient");
    assert!(!created.idempotent_replay);
    assert_eq!(created.note.current_revision, 1);
    assert_eq!(created.note.recorded_by.as_str(), "agent:signed-alpha");
    assert_eq!(created.note.evidence_links.len(), 2);
    assert_ne!(
        created.note.evidence_links[0].link_id,
        created.note.evidence_links[1].link_id
    );
    let note_id = created.note.note_id;

    // 4. Idempotent replay over HTTP returns exact same IDs without duplicating; mismatch returns 409.
    let replayed = signed_client
        .create_note(&create_req, None)
        .await
        .expect("idempotent replay over HTTP");
    assert!(replayed.idempotent_replay);
    assert_eq!(replayed.note.note_id, note_id);
    assert_eq!(replayed.event_id, created.event_id);

    let mismatch = signed_client
        .create_note(
            &CreateNoteRequest {
                body: String::from("Conflicting body for same idempotency_key"),
                ..create_req
            },
            None,
        )
        .await;
    assert!(matches!(
        mismatch,
        Err(DennettError::IdempotencyMismatch { .. })
    ));

    // 5. Read current note via CLI and MCP -> identical state (V06).
    let (get_code, get_out, get_err) = run_cli(
        &base_url,
        Some(alpha_readonly_tok),
        &["--json", "get", "--id", &note_id.to_string()],
    )
    .await;
    assert_eq!(get_code, 0, "CLI get failed: {get_err}");
    let cli_note: MemoryNote = serde_json::from_str(get_out.trim()).expect("parse CLI note");
    assert_eq!(cli_note.note_id, note_id);
    assert_eq!(cli_note.current_revision, 1);
    assert_eq!(cli_note.evidence_links.len(), 2);

    let mut mcp_alpha = McpSession::spawn(&base_url, Some(alpha_rw_tok));
    let init = mcp_alpha
        .request("initialize", json!({"protocolVersion": "2024-11-05"}))
        .await;
    assert_eq!(init["result"]["serverInfo"]["name"], "dennett-memory-mcp");

    let (mcp_get_err, mcp_note_val) = mcp_alpha
        .call_tool(
            "memory_get_current",
            json!({"note_id": note_id.to_string()}),
        )
        .await;
    assert!(
        !mcp_get_err,
        "MCP memory_get_current failed: {mcp_note_val}"
    );
    let mcp_note: MemoryNote =
        serde_json::from_value(mcp_note_val).expect("parse MCP current note");
    assert_eq!(
        mcp_note, cli_note,
        "CLI and MCP must return identical current note"
    );

    // 6. Correct note rev 1 -> rev 2 via CLI, then rev 2 -> rev 3 via MCP.
    let (corr_code, corr_out, corr_err) = run_cli(
        &base_url,
        Some(alpha_rw_tok),
        &[
            "--json",
            "correct",
            "--id",
            &note_id.to_string(),
            "--scope",
            scope_alpha.as_str(),
            "--expected-revision",
            "1",
            "--title",
            "Архитектурное решение PG16 (ред. 2)",
            "--body",
            "Исправлено через CLI: standalone memoryd + axum.",
            "--rationale",
            "Уточнили HTTP-транспорт axum",
            "--agent",
            "cli-agent-label",
            "--idempotency-key",
            "e2e-corr-cli-2",
        ],
    )
    .await;
    assert_eq!(corr_code, 0, "CLI correct failed: {corr_err}");
    let cli_corr: NoteMutationResponse =
        serde_json::from_str(corr_out.trim()).expect("parse CLI correction");
    assert_eq!(cli_corr.note.current_revision, 2);
    assert_eq!(cli_corr.note.recorded_by.as_str(), "agent:alpha-rw");

    let (mcp_corr_err, mcp_corr_val) = mcp_alpha
        .call_tool(
            "memory_correct_note",
            json!({
                "note_id": note_id.to_string(),
                "scope": scope_alpha.as_str(),
                "expected_revision": 2,
                "title": "Архитектурное решение PG16 (ред. 3)",
                "body": "Исправлено через MCP: полный цикл V01+V06+V19.",
                "rationale": "Подтверждена консистентность MCP",
                "author_label": "mcp-agent-label",
                "idempotency_key": "e2e-corr-mcp-3"
            }),
        )
        .await;
    assert!(
        !mcp_corr_err,
        "MCP memory_correct_note failed: {mcp_corr_val}"
    );
    let mcp_corr: NoteMutationResponse =
        serde_json::from_value(mcp_corr_val).expect("parse MCP correction");
    assert_eq!(mcp_corr.note.current_revision, 3);

    // 7. Verify full immutable history (revisions 1, 2, 3 preserved!) across Client, CLI, and MCP.
    let client_hist = signed_client
        .get_note_history(note_id, None)
        .await
        .expect("client history");
    assert_eq!(client_hist.current_revision, 3);
    assert_eq!(client_hist.revisions.len(), 3);
    assert_eq!(
        client_hist.revisions[0].body,
        "Первоначальная версия заметки: standalone memoryd."
    );
    assert_eq!(
        client_hist.revisions[1].rationale.as_deref(),
        Some("Уточнили HTTP-транспорт axum")
    );
    assert_eq!(
        client_hist.revisions[2].rationale.as_deref(),
        Some("Подтверждена консистентность MCP")
    );

    let (hist_code, hist_out, _) = run_cli(
        &base_url,
        Some(alpha_readonly_tok),
        &["--json", "history", "--id", &note_id.to_string()],
    )
    .await;
    assert_eq!(hist_code, 0);
    let cli_hist: NoteHistoryResponse =
        serde_json::from_str(hist_out.trim()).expect("parse CLI history");
    assert_eq!(cli_hist, client_hist);

    let (mcp_hist_err, mcp_hist_val) = mcp_alpha
        .call_tool(
            "memory_get_history",
            json!({"note_id": note_id.to_string()}),
        )
        .await;
    assert!(!mcp_hist_err);
    let mcp_hist: NoteHistoryResponse =
        serde_json::from_value(mcp_hist_val).expect("parse MCP history");
    assert_eq!(mcp_hist, client_hist);

    // 8. Scoped Unicode search (both body and correction rationale) works across Client, CLI, and MCP.
    let client_body_search = signed_client
        .search_notes(
            &SearchNotesRequest {
                scope: Some(scope_alpha.clone()),
                query: Some(String::from("полный цикл")),
                limit: Some(10),
                include_history: false,
            },
            None,
        )
        .await
        .expect("client body search");
    assert!(
        client_body_search
            .notes
            .iter()
            .any(|n| n.note_id == note_id),
        "expected Russian body query to find updated rev 3 note"
    );

    let client_rationale_search = signed_client
        .search_notes(
            &SearchNotesRequest {
                scope: Some(scope_alpha.clone()),
                query: Some(String::from("консистентность")),
                limit: Some(10),
                include_history: false,
            },
            None,
        )
        .await
        .expect("client rationale search");
    assert!(
        client_rationale_search
            .notes
            .iter()
            .any(|n| n.note_id == note_id),
        "expected Russian rationale query to find updated rev 3 note"
    );

    let (cli_search_code, cli_search_out, _) = run_cli(
        &base_url,
        Some(alpha_readonly_tok),
        &[
            "--json",
            "search",
            "--scope",
            scope_alpha.as_str(),
            "--query",
            "PG16",
        ],
    )
    .await;
    assert_eq!(cli_search_code, 0);
    let cli_search: SearchNotesResponse =
        serde_json::from_str(cli_search_out.trim()).expect("parse CLI search");
    assert!(cli_search.notes.iter().any(|n| n.note_id == note_id));

    let (mcp_search_err, mcp_search_val) = mcp_alpha
        .call_tool(
            "memory_search",
            json!({"scope": scope_alpha.as_str(), "query": "PG16"}),
        )
        .await;
    assert!(!mcp_search_err);
    let mcp_search: SearchNotesResponse =
        serde_json::from_value(mcp_search_val).expect("parse MCP search");
    assert!(mcp_search.notes.iter().any(|n| n.note_id == note_id));

    // 9. Stale expected_revision=1 fails with Conflict across Client, CLI (exit 6), and MCP.
    let stale_client = signed_client
        .correct_note(
            &CorrectNoteRequest {
                note_id,
                scope: scope_alpha.clone(),
                expected_revision: 1,
                title: String::from("Stale"),
                body: String::from("Must fail"),
                tags: vec![],
                rationale: String::from("Stale test"),
                evidence_links: vec![],
                author_label: None,
                idempotency_key: IdempotencyKey::generate(),
            },
            None,
        )
        .await;
    assert!(matches!(
        stale_client,
        Err(DennettError::Conflict {
            expected_revision: Some(1),
            actual_revision: Some(3),
            ..
        })
    ));

    let (stale_cli_code, _, _) = run_cli(
        &base_url,
        Some(alpha_rw_tok),
        &[
            "correct",
            "--id",
            &note_id.to_string(),
            "--scope",
            scope_alpha.as_str(),
            "--expected-revision",
            "1",
            "--title",
            "Stale",
            "--body",
            "Must fail",
            "--rationale",
            "Stale CLI",
        ],
    )
    .await;
    assert_eq!(
        stale_cli_code, 6,
        "CLI stale revision must exit 6 (Conflict)"
    );

    let (stale_mcp_err, stale_mcp_val) = mcp_alpha
        .call_tool(
            "memory_correct_note",
            json!({
                "note_id": note_id.to_string(),
                "scope": scope_alpha.as_str(),
                "expected_revision": 1,
                "title": "Stale",
                "body": "Must fail",
                "rationale": "Stale MCP"
            }),
        )
        .await;
    assert!(stale_mcp_err);
    assert_eq!(stale_mcp_val["code"], "CONFLICT");

    // 10. Negative auth & scope isolation tests (V06):
    // 10a. Foreign scope with forged `--agent` flag via CLI -> exit 4 (ForbiddenScope), never leaks token.
    let (forged_cli_code, _, forged_cli_err) = run_cli(
        &base_url,
        Some(alpha_rw_tok),
        &[
            "write",
            "--scope",
            scope_beta.as_str(),
            "--title",
            "Forbidden write",
            "--body",
            "Trying to bypass via --agent",
            "--agent",
            "agent:beta-rw",
        ],
    )
    .await;
    assert_eq!(
        forged_cli_code, 4,
        "foreign scope with spoofed --agent must exit 4 (ForbiddenScope), stderr={forged_cli_err}"
    );
    assert!(!forged_cli_err.contains(alpha_rw_tok));

    // 10b. Foreign scope via MCP with forged author_label -> FORBIDDEN_SCOPE.
    let (forged_mcp_err, forged_mcp_val) = mcp_alpha
        .call_tool(
            "memory_write_note",
            json!({
                "scope": scope_beta.as_str(),
                "title": "Forbidden MCP write",
                "body": "Trying to bypass",
                "author_label": "agent:beta-rw"
            }),
        )
        .await;
    assert!(forged_mcp_err);
    assert_eq!(forged_mcp_val["code"], "FORBIDDEN_SCOPE");

    // 10c. Read-only token cannot write.
    let ro_client = MemoryClient::new(&base_url, alpha_readonly_tok).expect("ro client");
    let ro_write = ro_client
        .create_note(
            &CreateNoteRequest {
                scope: scope_alpha.clone(),
                title: String::from("RO write"),
                body: String::from("Must fail"),
                tags: vec![],
                observed_at: None,
                evidence_links: vec![],
                author_label: None,
                idempotency_key: IdempotencyKey::generate(),
                admission_status: None,
                salience: None,
                topic_key: None,
            },
            None,
        )
        .await;
    assert!(matches!(ro_write, Err(DennettError::ForbiddenScope { .. })));

    // 10d. Beta token cannot read alpha note by UUID and does NOT see alpha scope name or alpha search hits.
    let beta_client = MemoryClient::new(&base_url, beta_rw_tok).expect("beta client");
    let foreign_read = beta_client.get_note_current(note_id, None).await;
    let Err(DennettError::ForbiddenScope {
        scope: reported_scope,
        ..
    }) = foreign_read
    else {
        panic!("expected ForbiddenScope on cross-scope UUID read, got: {foreign_read:?}");
    };
    assert_ne!(
        reported_scope,
        scope_alpha.as_str(),
        "403 by UUID must not disclose foreign scope name"
    );

    let beta_unscoped_search = beta_client
        .search_notes(
            &SearchNotesRequest {
                scope: None,
                query: Some(String::from("PG16")),
                limit: Some(50),
                include_history: false,
            },
            None,
        )
        .await
        .expect("beta scoped search");
    assert!(
        beta_unscoped_search
            .notes
            .iter()
            .all(|n| n.scope == scope_beta),
        "unscoped search must never return foreign-scope notes"
    );

    // 10e. Expired, revoked, and missing tokens all return 401 Unauthorized (exit 3 in CLI).
    for bad_tok in [
        Some(expired_tok),
        Some(revoked_tok),
        Some("bogus-token"),
        None,
    ] {
        let client = match bad_tok {
            Some(t) => MemoryClient::new(&base_url, t).expect("client"),
            None => MemoryClient::unauthenticated(&base_url).expect("unauth client"),
        };
        let res = client.get_note_current(note_id, None).await;
        assert!(
            matches!(res, Err(DennettError::Unauthorized { .. })),
            "expected Unauthorized for token={bad_tok:?}, got: {res:?}"
        );

        let (cli_code, _, cli_err) =
            run_cli(&base_url, bad_tok, &["get", "--id", &note_id.to_string()]).await;
        assert_eq!(
            cli_code, 3,
            "expected CLI exit code 3 (Unauthorized), stderr={cli_err}"
        );
        if let Some(secret) = bad_tok {
            assert!(
                !cli_err.contains(secret),
                "CLI leaked bearer token in stderr"
            );
        }
    }

    let mut mcp_revoked = McpSession::spawn(&base_url, Some(revoked_tok));
    let (rev_mcp_err, rev_mcp_val) = mcp_revoked
        .call_tool(
            "memory_get_current",
            json!({"note_id": note_id.to_string()}),
        )
        .await;
    assert!(rev_mcp_err);
    assert_eq!(rev_mcp_val["code"], "UNAUTHORIZED");
    assert!(!rev_mcp_val.to_string().contains(revoked_tok));
    mcp_revoked.close().await;

    // 10f. Malformed NUL bytes and extreme observed_at over HTTP return InvalidInput (400), never 503.
    let nul_http = signed_client
        .create_note(
            &CreateNoteRequest {
                scope: scope_alpha.clone(),
                title: String::from("bad\0title"),
                body: String::from("ok"),
                tags: vec![],
                observed_at: None,
                evidence_links: vec![],
                author_label: None,
                idempotency_key: IdempotencyKey::generate(),
                admission_status: None,
                salience: None,
                topic_key: None,
            },
            None,
        )
        .await;
    assert!(
        matches!(nul_http, Err(DennettError::InvalidInput { .. })),
        "HTTP NUL byte must return InvalidInput (400), got: {nul_http:?}"
    );
    let extreme_ts_http = signed_client
        .ingest_evidence(
            &IngestEvidenceRequest {
                scope: scope_alpha,
                media_type: String::from("text/plain"),
                content: String::from("ts"),
                source_uri: None,
                observed_at: Some(TimestampMs::from_millis(i64::MAX)),
                metadata: BTreeMap::new(),
                idempotency_key: IdempotencyKey::generate(),
            },
            None,
        )
        .await;
    assert!(
        matches!(extreme_ts_http, Err(DennettError::InvalidInput { .. })),
        "HTTP extreme observed_at must return InvalidInput (400), got: {extreme_ts_http:?}"
    );

    mcp_alpha.close().await;
    daemon.shutdown().await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "restart durability test seeds evidence, note, and correction on daemon 1 and verifies all persisted entities on daemon 2"
)]
async fn memoryd_restart_durability_preserves_notes_history_and_evidence_links() {
    let pg_url = require_memory_pg_url();
    let scope = unique_scope("restart");
    let token = "tok_restart_durability_741852";

    let auth_config = AuthConfig {
        tokens: vec![StaticTokenEntryConfig {
            token_id: String::from("restart-tok"),
            token: Some(String::from(token)),
            token_sha256: None,
            principal_id: PrincipalId::try_new("agent:restart").expect("principal"),
            issuer: String::from(DEFAULT_ISSUER),
            allowed_scopes: BTreeSet::from([scope.clone()]),
            capabilities: BTreeSet::from([MemoryCapability::Read, MemoryCapability::Write]),
            expires_at: None,
            revoked: false,
        }],
        ..AuthConfig::default()
    };

    // Process #1: ingest evidence, write note with anchor, correct to rev 2, then shut down.
    let daemon1 = RunningMemoryd::spawn(&pg_url, &auth_config).await;
    let client1 = MemoryClient::new(&daemon1.base_url, token).expect("client1");

    let ev = client1
        .ingest_evidence(
            &IngestEvidenceRequest {
                scope: scope.clone(),
                media_type: String::from("text/plain"),
                content: String::from("Durable source artifact across restarts"),
                source_uri: Some(String::from("file://durable.txt")),
                observed_at: Some(TimestampMs::from_millis(1_700_000_000_000)),
                metadata: BTreeMap::from([(String::from("phase"), String::from("before-restart"))]),
                idempotency_key: IdempotencyKey::generate(),
            },
            None,
        )
        .await
        .expect("ingest evidence on process 1");

    let created = client1
        .create_note(
            &CreateNoteRequest {
                scope: scope.clone(),
                title: String::from("Survives restart v1"),
                body: String::from("Original decision before restart"),
                tags: vec![String::from("durability")],
                observed_at: None,
                evidence_links: vec![EvidenceLinkInput {
                    evidence_id: ev.evidence.evidence_id,
                    relationship: EvidenceRelationship::try_new("source").expect("rel"),
                    anchor: Some(EvidenceAnchor {
                        kind: String::from("line_range"),
                        start: Some(1),
                        end: Some(1),
                        locator: Some(String::from("durable.txt")),
                        attributes: BTreeMap::new(),
                    }),
                    ordinal: None,
                }],
                author_label: None,
                idempotency_key: IdempotencyKey::generate(),
                admission_status: None,
                salience: None,
                topic_key: None,
            },
            None,
        )
        .await
        .expect("create note on process 1");

    let corrected = client1
        .correct_note(
            &CorrectNoteRequest {
                note_id: created.note.note_id,
                scope: scope.clone(),
                expected_revision: 1,
                title: String::from("Survives restart v2"),
                body: String::from("Corrected decision before restart"),
                tags: vec![String::from("durability"), String::from("v2")],
                rationale: String::from("Refined before shutdown"),
                evidence_links: vec![EvidenceLinkInput {
                    evidence_id: ev.evidence.evidence_id,
                    relationship: EvidenceRelationship::try_new("correction_basis").expect("rel"),
                    anchor: None,
                    ordinal: Some(0),
                }],
                author_label: None,
                idempotency_key: IdempotencyKey::generate(),
            },
            None,
        )
        .await
        .expect("correct note on process 1");

    let addr1 = daemon1.addr.clone();
    daemon1.shutdown().await;

    // Process #2: brand-new `dennett-memoryd` process against the same PostgreSQL database.
    let daemon2 = RunningMemoryd::spawn(&pg_url, &auth_config).await;
    assert_ne!(
        daemon2.addr, addr1,
        "new process should bind its own ephemeral port"
    );
    let client2 = MemoryClient::new(&daemon2.base_url, token).expect("client2");

    let current = client2
        .get_note_current(created.note.note_id, None)
        .await
        .expect("read current note after restart");
    assert_eq!(current, corrected.note);

    let history = client2
        .get_note_history(created.note.note_id, None)
        .await
        .expect("read history after restart");
    assert_eq!(history.current_revision, 2);
    assert_eq!(history.revisions.len(), 2);
    assert_eq!(history.revisions[0].title, "Survives restart v1");
    assert_eq!(history.revisions[0].evidence_links.len(), 1);
    assert_eq!(history.revisions[1].title, "Survives restart v2");
    assert_eq!(history.revisions[1].evidence_links.len(), 1);

    let fetched_ev = client2
        .get_evidence(ev.evidence.evidence_id, None)
        .await
        .expect("read evidence after restart");
    assert_eq!(fetched_ev, ev.evidence);

    let fetched_event = client2
        .get_event(created.event_id, None)
        .await
        .expect("read creation event after restart");
    assert_eq!(fetched_event.evidence_links.len(), 1);
    assert!(fetched_event.evidence_links[0].ordinal.is_none());

    daemon2.shutdown().await;
}

#[tokio::test]
async fn memoryd_fault_path_pg_down_reports_honest_errors_without_leaking_secrets() {
    let _pg_url = require_memory_pg_url();
    let scope = unique_scope("fault");
    let pg_secret = "D1_FAULT_PG_SECRET_9911";
    let bearer_secret = "d1_fault_bearer_secret_8822";
    let broken_pg_url =
        format!("postgres://dennett_memory_role:{pg_secret}@127.0.0.1:1/dennett_memory_test");

    let auth_config = AuthConfig {
        tokens: vec![StaticTokenEntryConfig {
            token_id: String::from("fault-tok"),
            token: Some(String::from(bearer_secret)),
            token_sha256: None,
            principal_id: PrincipalId::try_new("agent:fault").expect("principal"),
            issuer: String::from(DEFAULT_ISSUER),
            allowed_scopes: BTreeSet::from([scope.clone()]),
            capabilities: BTreeSet::from([MemoryCapability::Read, MemoryCapability::Write]),
            expires_at: None,
            revoked: false,
        }],
        ..AuthConfig::default()
    };

    let daemon = RunningMemoryd::spawn(&broken_pg_url, &auth_config).await;

    // 1. MemoryClient gets honest DatabaseUnavailable without leaking secrets.
    let client = MemoryClient::new(&daemon.base_url, bearer_secret).expect("client");
    let client_res = client
        .create_note(
            &CreateNoteRequest {
                scope: scope.clone(),
                title: String::from("Should fail"),
                body: String::from("PG is unreachable"),
                tags: vec![],
                observed_at: None,
                evidence_links: vec![],
                author_label: None,
                idempotency_key: IdempotencyKey::generate(),
                admission_status: None,
                salience: None,
                topic_key: None,
            },
            None,
        )
        .await;
    let Err(err) = client_res else {
        panic!("write must fail honestly when PG is down");
    };
    assert!(
        matches!(err, DennettError::DatabaseUnavailable { .. }),
        "expected DatabaseUnavailable, got: {err:?}"
    );
    let err_str = err.to_string();
    assert!(
        !err_str.contains(pg_secret),
        "PG password leaked: {err_str}"
    );
    assert!(
        !err_str.contains(bearer_secret),
        "bearer token leaked: {err_str}"
    );

    // 2. CLI exits with code 7 (DatabaseUnavailable) and leaks zero secrets.
    let (cli_code, cli_out, cli_err) = run_cli(
        &daemon.base_url,
        Some(bearer_secret),
        &[
            "write",
            "--scope",
            scope.as_str(),
            "--title",
            "Should fail CLI",
            "--body",
            "PG is down",
        ],
    )
    .await;
    assert_eq!(
        cli_code, 7,
        "CLI must exit 7 when PG is down: stderr={cli_err}"
    );
    assert!(!cli_out.contains(pg_secret) && !cli_err.contains(pg_secret));
    assert!(!cli_out.contains(bearer_secret) && !cli_err.contains(bearer_secret));

    // 3. MCP returns structured DATABASE_UNAVAILABLE tool error without leaking secrets.
    let mut mcp = McpSession::spawn(&daemon.base_url, Some(bearer_secret));
    let (mcp_is_err, mcp_val) = mcp
        .call_tool(
            "memory_write_note",
            json!({
                "scope": scope.as_str(),
                "title": "Should fail MCP",
                "body": "PG is down"
            }),
        )
        .await;
    assert!(mcp_is_err);
    assert_eq!(mcp_val["code"], "DATABASE_UNAVAILABLE");
    let mcp_text = mcp_val.to_string();
    assert!(!mcp_text.contains(pg_secret));
    assert!(!mcp_text.contains(bearer_secret));
    mcp.close().await;

    daemon.shutdown().await;
}

#[tokio::test]
async fn memory_mcp_rejects_oversized_stdin_line_and_continues_serving() {
    let _pg_url = require_memory_pg_url();
    let mut mcp = McpSession::spawn("http://127.0.0.1:1", None);

    let oversized = "x".repeat(300 * 1024);
    let err_resp = mcp.send_raw_line(&oversized).await;
    assert_eq!(err_resp["error"]["code"], -32600);

    // Non-object and missing-method frames return -32600 immediately without hanging.
    for malformed in ["123", "[]", "{}"] {
        let resp = mcp.send_raw_line(malformed).await;
        assert_eq!(
            resp["error"]["code"], -32600,
            "malformed frame `{malformed}` must return -32600, got: {resp}"
        );
        assert_eq!(resp["id"], Value::Null);
    }

    // Subsequent valid JSON-RPC request still succeeds cleanly.
    let ping = mcp.request("ping", json!({})).await;
    assert_eq!(ping["result"], json!({}));

    mcp.close().await;
}

#[tokio::test]
async fn mcp_fails_closed_on_invalid_token_env_and_cli_redacts_token_on_parse_error() {
    let _pg_url = require_memory_pg_url();
    let mcp_bin = locate_binary("dennett-memory-mcp", "dennett-memory-mcp");
    let bad_secret = "Bearer secret_invalid_tok_4242";

    let mcp_out = time::timeout(
        Duration::from_secs(5),
        Command::new(&mcp_bin)
            .env("DENNETT_MEMORY_URL", "http://127.0.0.1:1")
            .env("DENNETT_MEMORY_TOKEN", bad_secret)
            .stdin(Stdio::null())
            .output(),
    )
    .await
    .expect("mcp invalid-token startup timeout")
    .expect("spawn dennett-memory-mcp");

    assert_eq!(
        mcp_out.status.code(),
        Some(2),
        "dennett-memory-mcp must fail closed with exit 2 on invalid DENNETT_MEMORY_TOKEN"
    );
    let mcp_stderr = String::from_utf8_lossy(&mcp_out.stderr);
    assert!(
        !mcp_stderr.contains("secret_invalid_tok_4242"),
        "invalid token leaked in MCP startup stderr: {mcp_stderr}"
    );
    let err_json: Value = serde_json::from_str(mcp_stderr.trim()).unwrap_or_else(|err| {
        panic!("MCP startup stderr must be valid JSON `{mcp_stderr}`: {err}")
    });
    assert_eq!(err_json["code"], "INVALID_INPUT");

    // CLI pre-HTTP parse error must also redact DENNETT_MEMORY_TOKEN if echoed in bad arg.
    let cli_secret = "cli_env_secret_tok_887766";
    let (cli_code, cli_out, cli_err) = run_cli(
        "http://127.0.0.1:1",
        Some(cli_secret),
        &[&format!("unknown-cmd-{cli_secret}")],
    )
    .await;
    assert_eq!(cli_code, 2);
    assert!(!cli_out.contains(cli_secret) && !cli_err.contains(cli_secret));
}
