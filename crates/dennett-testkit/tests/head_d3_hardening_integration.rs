//! Real PostgreSQL 16 + live `dennett-head` + live `dennett-memoryd` integration tests
//! verifying D3 hardening invariants:
//! 1. Content-addressed immutable artifact snapshots (`artifact_blobs` + `GET .../artifacts/{id}/content`):
//!    workspace text AND binary (non-UTF-8) artifacts survive file overwrite, deletion, and full
//!    workspace/worktree removal with exact byte-for-byte SHA-256 verification and `nosniff`,
//!    while `Delete` lifecycle scrubs the unreferenced blob and returns `NotFound`.
//! 2. Zero-leak `ordered_outbox` scrubbing + monotonic tombstone sync + lost-ACK no-resurrection:
//!    - Immediately after delivery to `memoryd`, `ordered_outbox.payload_json` contains `[delivered]`
//!      with ZERO plaintext note title/body/tags while preserving idempotent replay & mismatch checks.
//!    - Forgetting via Head (`POST /v1/memory/forget`) immediately scrubs `ordered_outbox` to `[forgotten]`.
//!    - Forgetting out-of-band directly on `memoryd` + `flush_memory_outbox` syncs tombstones, scrubs
//!      `ordered_outbox` to `[forgotten]`, and advances `memory_tombstone_sync_state.last_tombstone_seq`.
//!    - Lost-ACK retry of a forgotten note's `idempotency_key` fails closed without resurrecting
//!      the note in `memoryd` AND physically scrubs the pending outbox row to `[forgotten]`.

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use dennett_contracts::{
    ArtifactCompleteness, ArtifactLifecycleAction, ArtifactLifecycleRequest,
    CommitMemoryNoteRequest, CreateAgentRequest, CreateArtifactRequest, CreateNoteRequest,
    CreateProjectRequest, CreateSessionRequest, DennettError, ForgetRequest, ForgetTargetType,
    IdempotencyKey, MemoryOutboxStatus, MemoryScope, SearchNotesRequest, SessionKind,
    WorkspaceWritePolicy,
};
use dennett_head_client::HeadClient;
use dennett_memory_client::MemoryClient;
use dennett_testkit::profile::{TEST_PROFILE_ENV, TestProfile};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::time;
use tokio_postgres::NoTls;
use uuid::Uuid;

fn with_isolated_schema(base_url: &str, prefix: &str) -> (String, String) {
    let short = &Uuid::new_v4().simple().to_string()[..12];
    let schema = format!("iso_{prefix}_{short}");
    let sep = if base_url.contains('?') { '&' } else { '?' };
    (
        format!("{base_url}{sep}options=-csearch_path%3D{schema}"),
        schema,
    )
}

fn require_pg_urls() -> (String, String) {
    assert_eq!(
        env::var(TEST_PROFILE_ENV).ok().as_deref(),
        Some("1"),
        "{TEST_PROFILE_ENV}=1 is required for D3 hardening integration tests"
    );
    let mem = env::var("DENNETT_TEST_MEMORY_PG_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .expect("DENNETT_TEST_MEMORY_PG_URL is required");
    let ctrl = env::var("DENNETT_TEST_CONTROL_PG_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .expect("DENNETT_TEST_CONTROL_PG_URL is required");
    (mem, ctrl)
}

fn unique_scope(prefix: &str) -> MemoryScope {
    let short = &Uuid::new_v4().simple().to_string()[..12];
    MemoryScope::try_new(format!("project:{prefix}-{short}")).expect("valid scope")
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
        "failed building `{bin_name}` binary"
    );
    find_binary_near_current_exe(&exe_name)
        .unwrap_or_else(|| panic!("could not locate `{bin_name}` binary"))
}

struct RunningDaemon {
    child: Child,
    addr: String,
    base_url: String,
}

impl RunningDaemon {
    async fn spawn_memoryd(mem_pg_url: &str, auth_config_json: &str) -> Self {
        let bin = locate_binary("dennett-memoryd", "dennett-memoryd");
        let mut child = Command::new(&bin)
            .args(["--serve", "127.0.0.1:0"])
            .env(TEST_PROFILE_ENV, "1")
            .env("DENNETT_TEST_MEMORY_PG_URL", mem_pg_url)
            .env("DENNETT_MEMORY_AUTH_CONFIG", auth_config_json)
            .env_remove("DENNETT_MEMORY_PG_URL")
            .env_remove("DENNETT_MEMORY_AUTH_CONFIG_FILE")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("spawn dennett-memoryd");

        Self::wait_listening(&mut child, "dennett-memoryd").await
    }

    async fn spawn_head_simulated(
        ctrl_pg_url: &str,
        profile_root: &Path,
        memory_url: Option<&str>,
        memory_token: Option<&str>,
        memory_hmac_secret: Option<&str>,
    ) -> Self {
        let bin = locate_binary("dennett-head", "dennett-head");
        let mut cmd = Command::new(&bin);
        cmd.args([
            "--serve",
            "127.0.0.1:0",
            "--runtime",
            "simulated",
            "--profile-root",
            &profile_root.to_string_lossy(),
        ]);
        if let Some(url) = memory_url {
            cmd.args(["--memory-url", url]);
        }
        cmd.env(TEST_PROFILE_ENV, "1")
            .env("DENNETT_TEST_CONTROL_PG_URL", ctrl_pg_url)
            .env_remove("DENNETT_CONTROL_PG_URL")
            .env_remove("DENNETT_MEMORY_TOKEN")
            .env_remove("DENNETT_MEMORY_HMAC_SECRET");
        if let Some(tok) = memory_token {
            cmd.env("DENNETT_MEMORY_TOKEN", tok);
        }
        if let Some(secret) = memory_hmac_secret {
            cmd.env("DENNETT_MEMORY_HMAC_SECRET", secret);
        }
        let mut child = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("spawn dennett-head");

        Self::wait_listening(&mut child, "dennett-head").await
    }

    async fn wait_listening(child: &mut Child, name: &str) -> Self {
        let stdout = child.stdout.take().expect("stdout piped");
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        match time::timeout(Duration::from_secs(15), reader.read_line(&mut line)).await {
            Ok(Ok(n)) if n > 0 => {}
            other => panic!("did not receive LISTENING line from {name}: {other:?}"),
        }
        let addr = line
            .trim()
            .strip_prefix("LISTENING ")
            .unwrap_or_else(|| panic!("unexpected {name} startup line: `{line}`"))
            .to_owned();
        tokio::spawn(async move {
            let mut sink = String::new();
            while let Ok(n) = reader.read_line(&mut sink).await {
                if n == 0 {
                    break;
                }
                sink.clear();
            }
        });
        Self {
            child: std::mem::replace(
                child,
                Command::new(if cfg!(windows) { "cmd" } else { "true" })
                    .args(if cfg!(windows) {
                        &["/C", "exit 0"][..]
                    } else {
                        &[][..]
                    })
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .expect("dummy placeholder"),
            ),
            base_url: format!("http://{addr}"),
            addr,
        }
    }

    async fn shutdown(mut self) {
        let req = format!(
            "POST /shutdown HTTP/1.1\r\nHost: {}\r\nX-Dennett-Shutdown: 1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            self.addr
        );
        if let Ok(mut stream) = TcpStream::connect(&self.addr).await {
            let _ = stream.write_all(req.as_bytes()).await;
            let mut sink = Vec::new();
            let _ = time::timeout(Duration::from_secs(2), stream.read_to_end(&mut sink)).await;
        }
        let _ = time::timeout(Duration::from_secs(5), self.child.wait()).await;
    }
}

async fn connect_pg(url: &str) -> (tokio_postgres::Client, tokio::task::JoinHandle<()>) {
    let (client, conn) = tokio_postgres::connect(url, NoTls)
        .await
        .expect("connect PG");
    let handle = tokio::spawn(async move {
        let _ = conn.await;
    });
    (client, handle)
}

async fn prepare_schema(base_url: &str, schema: &str) {
    let (pg, handle) = connect_pg(base_url).await;
    pg.batch_execute(&format!("CREATE SCHEMA IF NOT EXISTS {schema}"))
        .await
        .expect("create isolated schema");
    drop(pg);
    handle.abort();
}

async fn drop_schema(base_url: &str, schema: &str) {
    let (pg, handle) = connect_pg(base_url).await;
    let _ = pg
        .batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
        .await;
    drop(pg);
    handle.abort();
}

/// Proves Contract D §D.10 immutable artifact snapshots (`artifact_blobs` + `GET .../content`):
/// both a workspace UTF-8 text file AND a binary (non-UTF-8) file survive file overwrite,
/// file deletion, and complete removal of the workspace directory, returning exact original
/// bytes + matching SHA-256 + `X-Content-Type-Options: nosniff`, and `Delete` lifecycle
/// garbage-collects unreferenced blobs and rejects subsequent content reads with `NotFound`.
#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "end-to-end PG16 + HTTP artifact immutability test verifies text+binary snapshot capture, disk overwrite/deletion, worktree removal, header checks, and Delete blob GC"
)]
async fn d3_artifact_immutable_snapshots_survive_overwrite_delete_and_worktree_removal() {
    let (_mem_base, ctrl_base) = require_pg_urls();
    let (ctrl_pg, ctrl_schema) = with_isolated_schema(&ctrl_base, "artblob");
    prepare_schema(&ctrl_base, &ctrl_schema).await;

    let (profile, _tmp) = TestProfile::create_on_disk("d3-artblob").expect("temp profile");
    let worktree_dir = profile.workspace_dir().join("ephemeral-worktree");
    fs::create_dir_all(&worktree_dir).expect("create worktree dir");

    let text_bytes =
        b"# Immutable Artifact Snapshot\nExact UTF-8 payload v1 (survives worktree delete)\n"
            .to_vec();
    let bin_bytes: Vec<u8> = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG header
        0xFF, 0xFE, 0x00, 0x80, 0xDE, 0xAD, 0xBE, 0xEF, // invalid UTF-8 bytes
    ];
    let text_path = worktree_dir.join("notes.md");
    let bin_path = worktree_dir.join("capture.png");
    fs::write(&text_path, &text_bytes).expect("write text artifact");
    fs::write(&bin_path, &bin_bytes).expect("write binary artifact");

    let head =
        RunningDaemon::spawn_head_simulated(&ctrl_pg, profile.data_dir(), None, None, None).await;
    let client = HeadClient::new(&head.base_url).expect("head client");

    let health = client.healthz(None).await.expect("healthz");
    assert_eq!(health.schema_version, Some(10));

    let scope = unique_scope("artblob");
    let project = client
        .create_project(
            &CreateProjectRequest {
                name: String::from("Worktree Artifact Project"),
                workspace_path: Some(worktree_dir.to_string_lossy().into_owned()),
                memory_scope: Some(scope.clone()),
            },
            None,
        )
        .await
        .expect("create project");
    let agent = client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Artifact Agent"),
                description: String::from("Artifact immutability test agent"),
                system_prompt: String::from("Snapshot artifacts cleanly."),
                model_preference: None,
                thinking_level: None,
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
                allowed_memory_scopes: BTreeSet::from([scope]),
                attached_skills: Vec::new(),
                default_budget: None,
                preferred_connection_id: None,
            },
            None,
        )
        .await
        .expect("create agent");
    let session = client
        .create_session(
            &CreateSessionRequest {
                project_id: Some(project.project_id),
                agent_id: agent.agent_id,
                session_kind: SessionKind::UserChat,
                title: String::from("Artifact Immutability Session"),
            },
            None,
        )
        .await
        .expect("create session");

    // 1. Register both workspace files with `content: None, relative_path: Some(...)`.
    let text_art = client
        .create_artifact(
            session.session_id,
            &CreateArtifactRequest {
                run_id: None,
                name: String::from("notes.md"),
                mime_type: String::from("text/markdown"),
                content: None,
                relative_path: Some(String::from("notes.md")),
                completeness_status: ArtifactCompleteness::Final,
            },
            None,
        )
        .await
        .expect("register text workspace artifact");
    assert_eq!(
        text_art.content.as_deref(),
        Some(std::str::from_utf8(&text_bytes).expect("utf8")),
        "UTF-8 workspace file <= 256 KiB should populate inline preview content"
    );

    let bin_art = client
        .create_artifact(
            session.session_id,
            &CreateArtifactRequest {
                run_id: None,
                name: String::from("capture.png"),
                mime_type: String::from("image/png"),
                content: None,
                relative_path: Some(String::from("capture.png")),
                completeness_status: ArtifactCompleteness::Final,
            },
            None,
        )
        .await
        .expect("register binary workspace artifact");
    assert!(
        bin_art.content.is_none(),
        "non-UTF-8 binary file must leave inline text content None while storing exact blob bytes"
    );

    // 2. Overwrite BOTH files on disk, delete both files, and delete the entire worktree directory!
    fs::write(&text_path, b"MUTATED AFTER CAPTURE").expect("overwrite text file");
    fs::write(&bin_path, vec![0x00_u8; 64]).expect("overwrite binary file");
    fs::remove_file(&text_path).expect("delete text file");
    fs::remove_file(&bin_path).expect("delete binary file");
    fs::remove_dir_all(&worktree_dir).expect("remove entire worktree directory");
    assert!(!worktree_dir.exists());

    // 3. Prove `GET /v1/sessions/{sid}/artifacts/{aid}/content` serves the EXACT immutable bytes
    //    and matching SHA-256 for BOTH text and binary artifacts after complete disk destruction!
    let got_text = client
        .get_artifact_content(session.session_id, text_art.artifact_id, None)
        .await
        .expect("fetch immutable text snapshot");
    assert_eq!(got_text.bytes, text_bytes);
    assert_eq!(got_text.sha256, text_art.sha256);
    assert_eq!(got_text.mime_type, "text/markdown");

    let got_bin = client
        .get_artifact_content(session.session_id, bin_art.artifact_id, None)
        .await
        .expect("fetch immutable binary snapshot");
    assert_eq!(got_bin.bytes, bin_bytes);
    assert_eq!(got_bin.sha256, bin_art.sha256);
    assert_eq!(got_bin.mime_type, "image/png");

    // Verify raw HTTP response headers include `X-Content-Type-Options: nosniff` & `X-Dennett-Artifact-Sha256`.
    let raw_req = format!(
        "GET /v1/sessions/{}/artifacts/{}/content HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        session.session_id, bin_art.artifact_id, head.addr
    );
    let mut stream = TcpStream::connect(&head.addr).await.expect("connect head");
    stream
        .write_all(raw_req.as_bytes())
        .await
        .expect("write req");
    let mut raw_resp = Vec::new();
    stream.read_to_end(&mut raw_resp).await.expect("read resp");
    let header_end = raw_resp
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("HTTP header separator");
    let header_text = String::from_utf8_lossy(&raw_resp[..header_end]).to_ascii_lowercase();
    assert!(header_text.contains("x-content-type-options: nosniff"));
    assert!(header_text.contains(&format!(
        "x-dennett-artifact-sha256: {}",
        bin_art.sha256.as_str()
    )));
    assert_eq!(&raw_resp[header_end + 4..], bin_bytes.as_slice());

    // 4. Delete lifecycle scrubs unreferenced blob and refuses content download with `NotFound`.
    let _ = client
        .apply_artifact_lifecycle(
            session.session_id,
            bin_art.artifact_id,
            &ArtifactLifecycleRequest {
                action: ArtifactLifecycleAction::Delete,
                reason: Some(String::from("cleanup binary blob")),
            },
            None,
        )
        .await
        .expect("delete binary artifact");
    assert!(matches!(
        client
            .get_artifact_content(session.session_id, bin_art.artifact_id, None)
            .await,
        Err(DennettError::NotFound { .. })
    ));

    let (pg, pg_handle) = connect_pg(&ctrl_pg).await;
    let remaining_bin_blobs: i64 = pg
        .query_one(
            "SELECT COUNT(*)::bigint FROM artifact_blobs WHERE sha256 = $1",
            &[&bin_art.sha256.as_str()],
        )
        .await
        .expect("query artifact_blobs")
        .get(0);
    assert_eq!(
        remaining_bin_blobs, 0,
        "deleted unreferenced artifact blob must be garbage-collected"
    );
    drop(pg);
    pg_handle.abort();

    head.shutdown().await;
    drop_schema(&ctrl_base, &ctrl_schema).await;
}

/// Proves zero-leak `ordered_outbox` scrubbing on delivery & forget (both Head-proxied and
/// out-of-band on `memoryd` via `sync_memory_tombstones`), watermark persistence in
/// `memory_tombstone_sync_state`, and lost-ACK retry fail-closed scrubbing without resurrection.
#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "end-to-end PG16 + dual-daemon test verifies delivery scrub, Head forget scrub, out-of-band tombstone sync scrub, and lost-ACK pending scrub without resurrection"
)]
async fn d3_outbox_zero_leak_scrubbing_tombstone_sync_and_lost_ack_no_resurrection() {
    let (mem_base, ctrl_base) = require_pg_urls();
    let (mem_pg, mem_schema) = with_isolated_schema(&mem_base, "memscrub");
    let (ctrl_pg, ctrl_schema) = with_isolated_schema(&ctrl_base, "ctrlscrub");
    prepare_schema(&mem_base, &mem_schema).await;
    prepare_schema(&ctrl_base, &ctrl_schema).await;

    let (profile, _tmp) = TestProfile::create_on_disk("d3-outbox-scrub").expect("temp profile");
    let scope = unique_scope("scrub");
    let hmac_secret = "dennett_test_hmac_secret_zero_leak_9876543210";
    let direct_token = "tok_direct_memoryd_full_caps_123456";
    let auth_cfg = serde_json::json!({
        "audience": "dennett-memoryd",
        "allowed_issuers": ["dennett-standalone", "dennett-head"],
        "hmac_secret": hmac_secret,
        "tokens": [{
            "token_id": "tok-direct-full",
            "token": direct_token,
            "principal_id": "direct-owner",
            "issuer": "dennett-standalone",
            "allowed_scopes": [scope.as_str()],
            "capabilities": ["read", "write", "admin", "export", "forget"],
            "revoked": false
        }]
    })
    .to_string();

    let memoryd = RunningDaemon::spawn_memoryd(&mem_pg, &auth_cfg).await;
    let head = RunningDaemon::spawn_head_simulated(
        &ctrl_pg,
        profile.data_dir(),
        Some(&memoryd.base_url),
        Some(direct_token),
        Some(hmac_secret),
    )
    .await;

    let head_client = HeadClient::new(&head.base_url).expect("head client");
    let mem_client = MemoryClient::new(&memoryd.base_url, direct_token).expect("memory client");
    let (ctrl_conn, ctrl_handle) = connect_pg(&ctrl_pg).await;

    // ------------------------------------------------------------------------
    // Scenario 1: Delivery immediately scrubs plaintext -> `[delivered]`;
    // Head-proxied Forget immediately scrubs -> `[forgotten]`.
    // ------------------------------------------------------------------------
    let secret_alpha = "ULTRA_SECRET_ALPHA_PHRASE_4199";
    let idem_alpha = IdempotencyKey::generate();
    let commit_alpha = head_client
        .commit_memory_note(
            &CommitMemoryNoteRequest {
                session_id: None,
                run_id: None,
                scope: scope.clone(),
                title: format!("Title {secret_alpha}"),
                body: format!("Sensitive body {secret_alpha}"),
                tags: vec![String::from("secret-tag-alpha")],
                idempotency_key: Some(idem_alpha.clone()),
            },
            None,
        )
        .await
        .expect("commit alpha note");
    assert!(commit_alpha.delivered_immediately);
    assert_eq!(
        commit_alpha.outbox_item.status,
        MemoryOutboxStatus::Delivered
    );
    let note_alpha_id = commit_alpha
        .outbox_item
        .delivered_note_id
        .expect("delivered note id");

    let alpha_payload_delivered: String = ctrl_conn
        .query_one(
            "SELECT payload_json::text FROM ordered_outbox WHERE item_id = $1",
            &[&commit_alpha.outbox_item.outbox_id.as_uuid()],
        )
        .await
        .expect("read alpha outbox row")
        .get(0);
    assert!(
        alpha_payload_delivered.contains("[delivered]"),
        "delivered outbox payload must carry [delivered] marker: {alpha_payload_delivered}"
    );
    assert!(
        !alpha_payload_delivered.contains(secret_alpha)
            && !alpha_payload_delivered.contains("secret-tag-alpha"),
        "ZERO plaintext or tags may remain in ordered_outbox after delivery: {alpha_payload_delivered}"
    );

    // Forget note alpha via Head proxy -> outbox row immediately transitions to `[forgotten]`.
    let _ = head_client
        .forget_memory_item(
            &ForgetRequest {
                scope: scope.clone(),
                target_type: ForgetTargetType::Note,
                target_id: note_alpha_id.as_uuid(),
                reason: String::from("owner forget via head proxy"),
                idempotency_key: IdempotencyKey::generate(),
            },
            None,
        )
        .await
        .expect("forget alpha via head proxy");

    let alpha_payload_forgotten: String = ctrl_conn
        .query_one(
            "SELECT payload_json::text FROM ordered_outbox WHERE item_id = $1",
            &[&commit_alpha.outbox_item.outbox_id.as_uuid()],
        )
        .await
        .expect("read alpha outbox after forget")
        .get(0);
    assert!(
        alpha_payload_forgotten.contains("[forgotten]")
            && !alpha_payload_forgotten.contains("[delivered]"),
        "Head forget proxy must scrub outbox row to [forgotten]: {alpha_payload_forgotten}"
    );

    // Re-submitting the forgotten idempotency_key via Head MUST fail closed with Conflict.
    let alpha_resurrect = head_client
        .commit_memory_note(
            &CommitMemoryNoteRequest {
                session_id: None,
                run_id: None,
                scope: scope.clone(),
                title: format!("Title {secret_alpha}"),
                body: format!("Sensitive body {secret_alpha}"),
                tags: vec![String::from("secret-tag-alpha")],
                idempotency_key: Some(idem_alpha),
            },
            None,
        )
        .await;
    assert!(
        matches!(alpha_resurrect, Err(DennettError::Conflict { .. })),
        "re-submitting forgotten note key via Head must fail with Conflict, got: {alpha_resurrect:?}"
    );

    // ------------------------------------------------------------------------
    // Scenario 2: Out-of-band Forget directly on `memoryd` + `flush_memory_outbox`
    // (`sync_memory_tombstones`) scrubs `ordered_outbox` to `[forgotten]` and
    // advances `memory_tombstone_sync_state.last_tombstone_seq`.
    // ------------------------------------------------------------------------
    let secret_beta = "ULTRA_SECRET_BETA_PHRASE_5288";
    let commit_beta = head_client
        .commit_memory_note(
            &CommitMemoryNoteRequest {
                session_id: None,
                run_id: None,
                scope: scope.clone(),
                title: format!("Title {secret_beta}"),
                body: format!("Sensitive body {secret_beta}"),
                tags: vec![String::from("secret-tag-beta")],
                idempotency_key: Some(IdempotencyKey::generate()),
            },
            None,
        )
        .await
        .expect("commit beta note");
    assert!(commit_beta.delivered_immediately);
    let note_beta_id = commit_beta
        .outbox_item
        .delivered_note_id
        .expect("beta delivered note id");

    // Forget beta OUT-OF-BAND directly against `memoryd` (bypassing Head!).
    let _ = mem_client
        .forget(
            &ForgetRequest {
                scope: scope.clone(),
                target_type: ForgetTargetType::Note,
                target_id: note_beta_id.as_uuid(),
                reason: String::from("out-of-band memoryd forget"),
                idempotency_key: IdempotencyKey::generate(),
            },
            None,
        )
        .await
        .expect("out-of-band forget on memoryd");

    // Flush memory outbox on Head -> triggers `sync_memory_tombstones`!
    // Poll briefly because the V15 commit-visibility barrier (`commit_xid < pg_snapshot_xmin`)
    // may transiently withhold a newly committed tombstone while a sibling parallel test holds an
    // unrelated write transaction open on the shared PostgreSQL cluster.
    let scrub_deadline = time::Instant::now() + Duration::from_secs(5);
    let beta_payload_forgotten: String = loop {
        let _ = head_client
            .flush_memory_outbox(None)
            .await
            .expect("flush memory outbox triggers tombstone sync");
        let payload: String = ctrl_conn
            .query_one(
                "SELECT payload_json::text FROM ordered_outbox WHERE item_id = $1",
                &[&commit_beta.outbox_item.outbox_id.as_uuid()],
            )
            .await
            .expect("read beta outbox after oob forget sync")
            .get(0);
        if payload.contains("[forgotten]") || time::Instant::now() >= scrub_deadline {
            break payload;
        }
        time::sleep(Duration::from_millis(25)).await;
    };
    assert!(
        beta_payload_forgotten.contains("[forgotten]")
            && !beta_payload_forgotten.contains(secret_beta),
        "sync_memory_tombstones must scrub out-of-band forgotten outbox row to [forgotten]: {beta_payload_forgotten}"
    );
    let sync_watermark: i64 = ctrl_conn
        .query_one(
            "SELECT last_tombstone_seq FROM memory_tombstone_sync_state WHERE singleton = TRUE",
            &[],
        )
        .await
        .expect("read tombstone sync watermark")
        .get(0);
    assert!(
        sync_watermark >= 2,
        "memory_tombstone_sync_state.last_tombstone_seq must advance across both tombstones, got: {sync_watermark}"
    );

    // ------------------------------------------------------------------------
    // Scenario 3: Lost-ACK retry of a forgotten note's `idempotency_key`
    // fails closed WITHOUT resurrecting the note in `memoryd` AND physically
    // scrubs the pending outbox row in `dennett_control` to `[forgotten]`.
    // ------------------------------------------------------------------------
    let secret_gamma = "ULTRA_SECRET_LOST_ACK_GAMMA_6377";
    let idem_gamma = IdempotencyKey::generate();
    let created_gamma = mem_client
        .create_note(
            &CreateNoteRequest {
                scope: scope.clone(),
                title: format!("Title {secret_gamma}"),
                body: format!("Sensitive body {secret_gamma}"),
                tags: vec![String::from("lost-ack-tag")],
                observed_at: None,
                evidence_links: Vec::new(),
                author_label: Some(String::from("dennett-head")),
                admission_status: None,
                salience: None,
                topic_key: None,
                idempotency_key: idem_gamma.clone(),
            },
            None,
        )
        .await
        .expect("direct create gamma simulating delivered-but-lost-ACK");

    // Note is forgotten in `memoryd` before Head retries delivery with the same `idempotency_key`.
    let _ = mem_client
        .forget(
            &ForgetRequest {
                scope: scope.clone(),
                target_type: ForgetTargetType::Note,
                target_id: created_gamma.note.note_id.as_uuid(),
                reason: String::from("forgotten before lost-ACK retry"),
                idempotency_key: IdempotencyKey::generate(),
            },
            None,
        )
        .await
        .expect("forget gamma on memoryd");

    // Head enqueues & attempts delivery with the same `idempotency_key` and plaintext payload.
    let lost_ack_commit = head_client
        .commit_memory_note(
            &CommitMemoryNoteRequest {
                session_id: None,
                run_id: None,
                scope: scope.clone(),
                title: format!("Title {secret_gamma}"),
                body: format!("Sensitive body {secret_gamma}"),
                tags: vec![String::from("lost-ack-tag")],
                idempotency_key: Some(idem_gamma),
            },
            None,
        )
        .await
        .expect("commit lost-ACK note parks cleanly");
    assert!(!lost_ack_commit.delivered_immediately);
    assert_eq!(
        lost_ack_commit.outbox_item.status,
        MemoryOutboxStatus::Failed,
        "lost-ACK forgotten note must park cleanly as Failed"
    );

    // 1. Outbox row in `dennett_control` MUST be physically scrubbed to `[forgotten]` with ZERO plaintext!
    let gamma_payload_scrubbed: String = ctrl_conn
        .query_one(
            "SELECT payload_json::text FROM ordered_outbox WHERE item_id = $1",
            &[&lost_ack_commit.outbox_item.outbox_id.as_uuid()],
        )
        .await
        .expect("read gamma outbox row")
        .get(0);
    assert!(
        gamma_payload_scrubbed.contains("[forgotten]"),
        "parked lost-ACK forgotten outbox row must be scrubbed to [forgotten]: {gamma_payload_scrubbed}"
    );
    assert!(
        !gamma_payload_scrubbed.contains(secret_gamma)
            && !gamma_payload_scrubbed.contains("lost-ack-tag"),
        "ZERO plaintext may remain in parked lost-ACK outbox row: {gamma_payload_scrubbed}"
    );

    // 2. Note MUST NOT be resurrected in `memoryd`!
    assert!(matches!(
        mem_client
            .get_note_current(created_gamma.note.note_id, None)
            .await,
        Err(DennettError::NotFound { .. })
    ));
    let search_gamma = mem_client
        .search_notes(
            &SearchNotesRequest {
                scope: Some(scope),
                query: Some(String::from(secret_gamma)),
                limit: Some(10),
                include_history: true,
            },
            None,
        )
        .await
        .expect("search gamma");
    assert!(
        search_gamma.notes.is_empty(),
        "forgotten note must never be resurrected in search results"
    );

    drop(ctrl_conn);
    ctrl_handle.abort();
    head.shutdown().await;
    memoryd.shutdown().await;
    drop_schema(&ctrl_base, &ctrl_schema).await;
    drop_schema(&mem_base, &mem_schema).await;
}
