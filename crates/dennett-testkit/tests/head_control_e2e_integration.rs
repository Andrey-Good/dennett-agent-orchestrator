//! Real PostgreSQL + multi-process integration tests for `dennett-head`, standalone
//! `dennett-memoryd`, `HeadClient`, and `dennettctl` (V02, V05, V06, V29, A04, A09).
//!
//! Uses `dennett-head --runtime simulated` (explicitly marked `is_simulated: true`) so offline
//! CI runs remain 100% deterministic while testing real PostgreSQL, real HTTP daemons, and
//! real external policy/outbox/restart invariants.

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use dennett_contracts::{
    AgentRecord, ArtifactRecord, CommitMemoryNoteRequest, CorrelationId, CreateAgentRequest,
    CreateArtifactRequest, CreatePolicyRuleRequest, CreateProjectRequest, CreateSessionRequest,
    DennettError, HeadHealth, HeadMemorySearchRequest, IdempotencyKey, MemoryCapability,
    MemoryOutboxStatus, MemoryScope, MemoryStatus, PolicyEffect, PrincipalId, ProjectRecord,
    RunStatus, SearchNotesRequest, SendMessageRequest, SessionDetailResponse, SessionKind,
    SessionRecord, ThinkingLevel, TimestampMs, UpdateAgentConfigRequest, UpdateProjectRequest,
    UpdateSessionRequest, WorkspaceWritePolicy,
};
use dennett_head_client::HeadClient;
use dennett_memory_client::MemoryClient;
use dennett_memory_core::{AuthConfig, DEFAULT_AUDIENCE, DEFAULT_ISSUER, StaticTokenEntryConfig};
use dennett_testkit::profile::{TEST_PROFILE_ENV, TestProfile};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::time;
use tokio_postgres::NoTls;
use uuid::Uuid;

fn with_isolated_control_schema(base_url: &str, prefix: &str) -> String {
    let short = &Uuid::new_v4().simple().to_string()[..12];
    let schema = format!("iso_{prefix}_{short}");
    let sep = if base_url.contains('?') { '&' } else { '?' };
    format!("{base_url}{sep}options=-csearch_path%3D{schema}")
}

fn require_pg_urls() -> (String, String) {
    assert_eq!(
        env::var(TEST_PROFILE_ENV).ok().as_deref(),
        Some("1"),
        "{TEST_PROFILE_ENV}=1 is required for Head E2E integration tests"
    );
    let mem = env::var("DENNETT_TEST_MEMORY_PG_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .expect("DENNETT_TEST_MEMORY_PG_URL is required");
    let ctrl = env::var("DENNETT_TEST_CONTROL_PG_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .expect("DENNETT_TEST_CONTROL_PG_URL is required");
    (mem, with_isolated_control_schema(&ctrl, "e2e"))
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
        "failed building `{bin_name}` binary for integration test"
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
    async fn spawn_memoryd(pg_url: &str, auth_config: &AuthConfig) -> Self {
        let bin = locate_binary("dennett-memoryd", "dennett-memoryd");
        let auth_json = serde_json::to_string(auth_config).expect("serialize auth config");
        let child = Command::new(&bin)
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
        Self::await_listening(child, "dennett-memoryd").await
    }

    async fn spawn_head_simulated(
        control_pg_url: &str,
        memory_url: &str,
        memory_token: &str,
    ) -> Self {
        let bin = locate_binary("dennett-head", "dennett-head");
        let mcp_bin = locate_binary("dennett-memory-mcp", "dennett-memory-mcp");
        let child = Command::new(&bin)
            .args([
                "--serve",
                "127.0.0.1:0",
                "--runtime",
                "simulated",
                "--memory-url",
                memory_url,
                "--memory-mcp-bin",
                &mcp_bin.to_string_lossy(),
            ])
            .env(TEST_PROFILE_ENV, "1")
            .env("DENNETT_TEST_CONTROL_PG_URL", control_pg_url)
            .env("DENNETT_MEMORY_TOKEN", memory_token)
            .env_remove("DENNETT_CONTROL_PG_URL")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("spawn dennett-head");
        Self::await_listening(child, "dennett-head").await
    }

    async fn spawn_head_simulated_with_hmac(
        control_pg_url: &str,
        memory_url: &str,
        memory_token: &str,
        hmac_secret: &str,
    ) -> Self {
        let bin = locate_binary("dennett-head", "dennett-head");
        let mcp_bin = locate_binary("dennett-memory-mcp", "dennett-memory-mcp");
        let child = Command::new(&bin)
            .args([
                "--serve",
                "127.0.0.1:0",
                "--runtime",
                "simulated",
                "--memory-url",
                memory_url,
                "--memory-mcp-bin",
                &mcp_bin.to_string_lossy(),
            ])
            .env(TEST_PROFILE_ENV, "1")
            .env("DENNETT_TEST_CONTROL_PG_URL", control_pg_url)
            .env("DENNETT_MEMORY_TOKEN", memory_token)
            .env("DENNETT_MEMORY_HMAC_SECRET", hmac_secret)
            .env_remove("DENNETT_CONTROL_PG_URL")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("spawn dennett-head");
        Self::await_listening(child, "dennett-head").await
    }

    async fn await_listening(mut child: Child, label: &str) -> Self {
        let stdout = child.stdout.take().expect("daemon stdout piped");
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        match time::timeout(Duration::from_secs(15), reader.read_line(&mut line)).await {
            Ok(Ok(n)) if n > 0 => {}
            other => panic!("did not receive LISTENING line from {label}: {other:?}"),
        }
        let addr = line
            .trim()
            .strip_prefix("LISTENING ")
            .unwrap_or_else(|| panic!("unexpected {label} startup line: `{line}`"))
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
            .expect("write shutdown");
        let mut resp = Vec::new();
        let _ = time::timeout(Duration::from_secs(5), stream.read_to_end(&mut resp)).await;
        let status = time::timeout(Duration::from_secs(5), self.child.wait())
            .await
            .expect("wait shutdown exit")
            .expect("child wait");
        assert!(status.success(), "daemon did not exit cleanly: {status}");
    }

    async fn kill_hard(mut self) {
        let _ = self.child.kill().await;
        let _ = time::timeout(Duration::from_secs(5), self.child.wait()).await;
    }
}

async fn run_dennettctl(head_url: &str, args: &[&str]) -> (i32, String, String) {
    let bin = locate_binary("dennettctl", "dennett-head-client");
    let output = time::timeout(
        Duration::from_secs(20),
        Command::new(&bin)
            .args(args)
            .env("DENNETT_HEAD_URL", head_url)
            .output(),
    )
    .await
    .expect("dennettctl timeout")
    .expect("run dennettctl");
    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (code, stdout, stderr)
}

fn unique_scope(prefix: &str) -> MemoryScope {
    let suffix = &Uuid::new_v4().simple().to_string()[..12];
    MemoryScope::try_new(format!("{prefix}-{suffix}")).expect("valid scope")
}

fn make_memory_auth(scope: &MemoryScope, token: &str) -> AuthConfig {
    AuthConfig {
        audience: String::from(DEFAULT_AUDIENCE),
        allowed_issuers: vec![String::from(DEFAULT_ISSUER)],
        hmac_secret: None,
        tokens: vec![StaticTokenEntryConfig {
            token_id: format!("head-int-{}", &Uuid::new_v4().simple().to_string()[..8]),
            token: Some(token.to_owned()),
            token_sha256: None,
            principal_id: PrincipalId::try_new("agent:head-integration").expect("principal"),
            issuer: String::from(DEFAULT_ISSUER),
            allowed_scopes: BTreeSet::from([scope.clone()]),
            capabilities: BTreeSet::from([MemoryCapability::Read, MemoryCapability::Write]),
            expires_at: Some(TimestampMs::from_millis(
                TimestampMs::now().as_millis() + 3_600_000,
            )),
            revoked: false,
        }],
        revoked_token_ids: BTreeSet::new(),
    }
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "end-to-end lifecycle test exercises project/agent/session/message/artifact + full daemon restart"
)]
async fn head_full_lifecycle_restart_and_dennettctl_persistence() {
    let (mem_pg, ctrl_pg) = require_pg_urls();
    let (profile, _tmp) = TestProfile::create_on_disk("head-lifecycle").expect("temp profile");
    let scope = unique_scope("head-life");
    let mem_token = "tok_head_lifecycle_991100";
    let auth_cfg = make_memory_auth(&scope, mem_token);

    let memoryd = RunningDaemon::spawn_memoryd(&mem_pg, &auth_cfg).await;
    let head = RunningDaemon::spawn_head_simulated(&ctrl_pg, &memoryd.base_url, mem_token).await;

    // 1. Check health via dennettctl --json: explicitly shows is_simulated=true and healthy memoryd!
    let (code, out, err) = run_dennettctl(&head.base_url, &["--json", "health"]).await;
    assert_eq!(code, 0, "dennettctl health failed: {err}");
    let health: HeadHealth = serde_json::from_str(out.trim()).expect("parse health");
    assert!(health.postgres_connected);
    assert_eq!(health.memory_status, MemoryStatus::Healthy);
    assert_eq!(health.runtime_adapter_id, "simulated-ci-runtime");
    assert!(
        health.runtime_is_simulated,
        "simulated runtime MUST never masquerade as live agy acp"
    );

    // 2. Create project, non-coding agent, session, message, and artifact via dennettctl CLI!
    let ws_str = profile.workspace_dir().to_string_lossy().into_owned();
    let (p_code, p_out, p_err) = run_dennettctl(
        &head.base_url,
        &[
            "--json",
            "project",
            "create",
            "--name",
            "Botany Study",
            "--workspace",
            &ws_str,
            "--scope",
            scope.as_str(),
        ],
    )
    .await;
    assert_eq!(p_code, 0, "project create failed: {p_err}");
    let project: ProjectRecord = serde_json::from_str(p_out.trim()).expect("parse project");

    let (a_code, a_out, a_err) = run_dennettctl(
        &head.base_url,
        &[
            "--json",
            "agent",
            "create",
            "--name",
            "Field Botanist",
            "--system-prompt",
            "You are a botanical archivist, not a coding assistant.",
            "--model",
            "sim-model-alpha",
            "--thinking",
            "medium",
            "--scope",
            scope.as_str(),
            "--write-policy",
            "paths:notes/",
        ],
    )
    .await;
    assert_eq!(a_code, 0, "agent create failed: {a_err}");
    let agent: AgentRecord = serde_json::from_str(a_out.trim()).expect("parse agent");
    assert_eq!(agent.current_config.version_number, 1);

    let (s_code, s_out, s_err) = run_dennettctl(
        &head.base_url,
        &[
            "--json",
            "session",
            "create",
            "--agent",
            &agent.agent_id.to_string(),
            "--project",
            &project.project_id.to_string(),
            "--title",
            "Alpine Flora Catalog",
        ],
    )
    .await;
    assert_eq!(s_code, 0, "session create failed: {s_err}");
    let session: SessionRecord = serde_json::from_str(s_out.trim()).expect("parse session");

    let (m_code, m_out, m_err) = run_dennettctl(
        &head.base_url,
        &[
            "--json",
            "send",
            "--session",
            &session.session_id.to_string(),
            "--message",
            "Catalog Edelweiss specimen #42",
        ],
    )
    .await;
    assert_eq!(m_code, 0, "send failed: {m_err}");
    assert!(m_out.contains("Catalog Edelweiss specimen #42"));

    let (art_code, art_out, art_err) = run_dennettctl(
        &head.base_url,
        &[
            "--json",
            "artifact",
            "create",
            "--session",
            &session.session_id.to_string(),
            "--name",
            "edelweiss.md",
            "--mime-type",
            "text/markdown",
            "--content",
            "# Specimen 42\nLeontopodium nivale",
        ],
    )
    .await;
    assert_eq!(art_code, 0, "artifact create failed: {art_err}");
    let artifact: ArtifactRecord = serde_json::from_str(art_out.trim()).expect("parse artifact");
    assert_eq!(artifact.revision, 1);

    // 3. Restart BOTH `dennett-head` and `dennett-memoryd` and confirm full restoration!
    head.shutdown().await;
    memoryd.shutdown().await;

    let memoryd2 = RunningDaemon::spawn_memoryd(&mem_pg, &auth_cfg).await;
    let head2 = RunningDaemon::spawn_head_simulated(&ctrl_pg, &memoryd2.base_url, mem_token).await;

    let (show_code, show_out, show_err) = run_dennettctl(
        &head2.base_url,
        &[
            "--json",
            "session",
            "show",
            "--id",
            &session.session_id.to_string(),
        ],
    )
    .await;
    assert_eq!(
        show_code, 0,
        "session show after restart failed: {show_err}"
    );
    let detail: SessionDetailResponse =
        serde_json::from_str(show_out.trim()).expect("parse session detail");

    assert_eq!(detail.session.session_id, session.session_id);
    assert_eq!(
        detail.messages.len(),
        2,
        "user + assistant messages must survive restart"
    );
    assert_eq!(detail.artifacts.len(), 1);
    assert_eq!(detail.artifacts[0].artifact_id, artifact.artifact_id);
    let binding = detail
        .session
        .runtime_binding
        .expect("runtime binding must persist across restart");
    assert!(binding.is_simulated);
    assert_eq!(binding.adapter_id, "simulated-ci-runtime");

    head2.shutdown().await;
    memoryd2.shutdown().await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "end-to-end V02 outage test verifies degraded health, honest search, unblocked Stop, outbox queue, and idempotent flush"
)]
async fn v02_memoryd_outage_degraded_stop_and_idempotent_outbox_recovery() {
    let (mem_pg, ctrl_pg) = require_pg_urls();
    let (profile, _tmp) = TestProfile::create_on_disk("head-v02").expect("temp profile");
    let scope = unique_scope("head-v02");
    let mem_token = "tok_head_v02_outage_772200";
    let auth_cfg = make_memory_auth(&scope, mem_token);

    let memoryd = RunningDaemon::spawn_memoryd(&mem_pg, &auth_cfg).await;
    let head = RunningDaemon::spawn_head_simulated(&ctrl_pg, &memoryd.base_url, mem_token).await;
    let head_client = HeadClient::new(&head.base_url).expect("head client");

    let project = head_client
        .create_project(
            &CreateProjectRequest {
                name: String::from("V02 Resilience"),
                workspace_path: Some(profile.workspace_dir().to_string_lossy().into_owned()),
                memory_scope: Some(scope.clone()),
            },
            None,
        )
        .await
        .expect("create project");

    let agent = head_client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Resilient Agent"),
                description: String::from("Tests V02 memory outage"),
                system_prompt: String::from("Be concise."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope.clone()]),
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
            },
            None,
        )
        .await
        .expect("create agent");

    let session = head_client
        .create_session(
            &CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: Some(project.project_id),
                title: String::from("Outage Test Session"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("create session");

    let baseline = head_client
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: String::from("Baseline persisted message before outage"),
                timeout_ms: None,
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("baseline turn");
    assert_eq!(baseline.memory_status, MemoryStatus::Healthy);

    // KILL `dennett-memoryd` while `dennett-head` stays running!
    let old_memory_addr = memoryd.addr.clone();
    memoryd.kill_hard().await;

    // 1. Head stays UP, reports honest `degraded_unreachable` while control PG is healthy.
    let degraded_health = head_client.healthz(None).await.expect("head stays up");
    assert!(degraded_health.postgres_connected);
    assert_eq!(
        degraded_health.memory_status,
        MemoryStatus::DegradedUnreachable
    );

    // 2. Searching memory while `memoryd` is down returns explicit DegradedUnreachable — NEVER fabricated memory!
    let search_outage = head_client
        .search_memory(
            &HeadMemorySearchRequest {
                scope: Some(scope.clone()),
                query: Some(String::from("anything")),
                limit: Some(5),
                include_history: false,
            },
            None,
        )
        .await
        .expect("honest degraded search response");
    assert_eq!(
        search_outage.memory_status,
        MemoryStatus::DegradedUnreachable
    );
    assert!(search_outage.degraded_reason.is_some());
    assert_eq!(search_outage.notes.len(), 0);

    // 3. Stop / cancel works unblocked while `memoryd` is DOWN!
    let client_for_turn = head_client.clone();
    let sid = session.session_id;
    let slow_task = tokio::spawn(async move {
        client_for_turn
            .send_message(
                sid,
                &SendMessageRequest {
                    content: String::from("[SIM:SLOW:15000]"),
                    timeout_ms: Some(20_000),
                    client_message_id: None,
                },
                None,
            )
            .await
    });
    let wait_deadline = time::Instant::now() + Duration::from_secs(5);
    loop {
        let snap = head_client
            .get_state_snapshot(None, false, None)
            .await
            .expect("poll snapshot for active slow V02 run");
        if snap.running_session_ids.contains(&session.session_id) {
            break;
        }
        assert!(
            time::Instant::now() < wait_deadline,
            "timed out waiting for slow V02 run to register"
        );
        time::sleep(Duration::from_millis(25)).await;
    }
    let cancel_started = time::Instant::now();
    let cancel_resp = head_client
        .cancel_run(session.session_id, None)
        .await
        .expect("cancel must succeed even when memoryd is down");
    assert!(
        cancel_started.elapsed() < Duration::from_secs(3),
        "Stop must NOT block on unreachable memoryd"
    );
    assert!(cancel_resp.cancelled);
    assert_eq!(
        cancel_resp.memory_status,
        MemoryStatus::DegradedUnreachable,
        "Stop must honestly report cached DegradedUnreachable during memoryd outage without blocking"
    );

    let turn_resp = slow_task
        .await
        .expect("join slow task")
        .expect("cancelled turn response");
    assert_eq!(turn_resp.run.status, RunStatus::Cancelled);
    assert_eq!(turn_resp.memory_status, MemoryStatus::DegradedUnreachable);

    // 4. Explicit memory commit while `memoryd` is DOWN queues durably into `memory_commit_outbox`.
    let idem_key = IdempotencyKey::try_new("v02-outbox-unique-key-1").expect("key");
    let queued = head_client
        .commit_memory_note(
            &CommitMemoryNoteRequest {
                session_id: Some(session.session_id),
                run_id: Some(baseline.run.run_id),
                scope: scope.clone(),
                title: String::from("Decision During Memory Outage"),
                body: String::from(
                    "Queued safely in dennett_control outbox for idempotent replay.",
                ),
                tags: vec![String::from("v02")],
                idempotency_key: Some(idem_key.clone()),
            },
            None,
        )
        .await
        .expect("queue memory commit");
    assert!(!queued.delivered_immediately);
    assert_eq!(queued.outbox_item.status, MemoryOutboxStatus::Pending);
    assert_eq!(queued.memory_status, MemoryStatus::DegradedUnreachable);

    // 4b. Reusing the SAME `(scope, idempotency_key)` with a conflicting payload MUST return
    //     `IdempotencyMismatch` (409 Conflict) — NEVER silently overwrite or return the old item!
    let conflict_pending = head_client
        .commit_memory_note(
            &CommitMemoryNoteRequest {
                session_id: Some(session.session_id),
                run_id: Some(baseline.run.run_id),
                scope: scope.clone(),
                title: String::from("Decision During Memory Outage"),
                body: String::from("CONFLICTING DIFFERENT PAYLOAD!"),
                tags: vec![String::from("v02")],
                idempotency_key: Some(idem_key.clone()),
            },
            None,
        )
        .await;
    assert!(
        matches!(
            conflict_pending,
            Err(DennettError::IdempotencyMismatch { .. })
        ),
        "conflicting payload for pending outbox key must fail with IdempotencyMismatch, got: {conflict_pending:?}"
    );

    // 5. Previously persisted conversation is intact during outage.
    let detail_during_outage = head_client
        .get_session_detail(session.session_id, None)
        .await
        .expect("session detail during outage");
    assert!(
        detail_during_outage
            .messages
            .iter()
            .any(|m| m.content == "Baseline persisted message before outage")
    );

    // 6. Restart `dennett-memoryd` on the SAME port (by binding the saved loopback address) OR
    //    restart Head pointing to the recovered `dennett-memoryd` — both test durable PG outbox!
    //    Here we restart `dennett-memoryd` on a fresh ephemeral port and restart `dennett-head`
    //    pointing at the recovered `dennett-memoryd` to simultaneously prove outbox survives Head restart!
    let _ = old_memory_addr;
    head.shutdown().await;

    let recovered_memoryd = RunningDaemon::spawn_memoryd(&mem_pg, &auth_cfg).await;
    let recovered_head =
        RunningDaemon::spawn_head_simulated(&ctrl_pg, &recovered_memoryd.base_url, mem_token).await;
    let recovered_client = HeadClient::new(&recovered_head.base_url).expect("head client");

    let health_back = recovered_client
        .healthz(None)
        .await
        .expect("recovered health");
    assert_eq!(health_back.memory_status, MemoryStatus::Healthy);
    assert!(health_back.pending_outbox_count >= 1);

    // Flush outbox TWICE -> second flush must be a no-op, and `memoryd` must contain EXACTLY ONE note!
    let flush1 = recovered_client
        .flush_memory_outbox(None)
        .await
        .expect("first flush");
    assert_eq!(flush1.memory_status, MemoryStatus::Healthy);
    assert!(flush1.delivered >= 1);
    assert_eq!(flush1.pending, 0);
    assert_eq!(flush1.failed, 0);

    let flush2 = recovered_client
        .flush_memory_outbox(None)
        .await
        .expect("second flush");
    assert_eq!(flush2.attempted, 0);
    assert_eq!(flush2.delivered, 0);

    // Also re-submit the exact same `commit_memory_note` with the same `idempotency_key` -> ZERO duplicates!
    let replay_commit = recovered_client
        .commit_memory_note(
            &CommitMemoryNoteRequest {
                session_id: Some(session.session_id),
                run_id: Some(baseline.run.run_id),
                scope: scope.clone(),
                title: String::from("Decision During Memory Outage"),
                body: String::from(
                    "Queued safely in dennett_control outbox for idempotent replay.",
                ),
                tags: vec![String::from("v02")],
                idempotency_key: Some(idem_key.clone()),
            },
            None,
        )
        .await
        .expect("idempotent commit replay");
    assert!(replay_commit.delivered_immediately);
    assert_eq!(
        replay_commit.outbox_item.status,
        MemoryOutboxStatus::Delivered
    );

    // And reusing the delivered `(scope, idempotency_key)` with a conflicting title/body MUST fail closed!
    let conflict_delivered = recovered_client
        .commit_memory_note(
            &CommitMemoryNoteRequest {
                session_id: Some(session.session_id),
                run_id: Some(baseline.run.run_id),
                scope: scope.clone(),
                title: String::from("Tampered Title After Delivery"),
                body: String::from(
                    "Queued safely in dennett_control outbox for idempotent replay.",
                ),
                tags: vec![String::from("v02")],
                idempotency_key: Some(idem_key),
            },
            None,
        )
        .await;
    assert!(
        matches!(
            conflict_delivered,
            Err(DennettError::IdempotencyMismatch { .. })
        ),
        "conflicting payload for delivered outbox key must fail with IdempotencyMismatch, got: {conflict_delivered:?}"
    );

    let mem_client =
        MemoryClient::new(&recovered_memoryd.base_url, mem_token).expect("memory client");
    let found = mem_client
        .search_notes(
            &SearchNotesRequest {
                scope: Some(scope),
                query: Some(String::from("Decision During Memory Outage")),
                limit: Some(10),
                include_history: false,
            },
            None,
        )
        .await
        .expect("search recovered memoryd");
    assert_eq!(
        found.notes.len(),
        1,
        "idempotent outbox recovery must create EXACTLY ONE note in memoryd, got: {:?}",
        found.notes
    );

    recovered_head.shutdown().await;
    recovered_memoryd.shutdown().await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "end-to-end V05/V06/A09 policy test exercises allow, prefix deny, traversal block, shell deny, error status, and secret-free audit"
)]
async fn v05_v06_policy_engine_allow_deny_path_escape_and_secret_free_audit_trace() {
    let (mem_pg, ctrl_pg) = require_pg_urls();
    let (profile, _tmp) = TestProfile::create_on_disk("head-v05").expect("temp profile");
    let scope = unique_scope("head-v05");
    let mem_token = "tok_head_v05_secret_554433";
    let auth_cfg = make_memory_auth(&scope, mem_token);

    let memoryd = RunningDaemon::spawn_memoryd(&mem_pg, &auth_cfg).await;
    let head = RunningDaemon::spawn_head_simulated(&ctrl_pg, &memoryd.base_url, mem_token).await;
    let client = HeadClient::new(&head.base_url).expect("head client");

    let project = client
        .create_project(
            &CreateProjectRequest {
                name: String::from("Policy Sandbox"),
                workspace_path: Some(profile.workspace_dir().to_string_lossy().into_owned()),
                memory_scope: Some(scope.clone()),
            },
            None,
        )
        .await
        .expect("create project");

    let agent = client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Scoped Writer"),
                description: String::from("May only write inside docs/"),
                system_prompt: String::from("Follow workspace boundaries."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope.clone()]),
                workspace_write_policy: WorkspaceWritePolicy::AllowPaths {
                    allowed_prefixes: vec![String::from("docs/")],
                },
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
            },
            None,
        )
        .await
        .expect("create agent");

    let session = client
        .create_session(
            &CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: Some(project.project_id),
                title: String::from("Policy Verification"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("create session");

    // 1. Allowed write inside `docs/` succeeds and creates file once.
    let cid_allow = CorrelationId::try_new("trace-v05-allow-1").expect("cid");
    let allow_resp = client
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: String::from("[SIM:TOOL:write_file:docs/allowed.txt|hello_v05]"),
                timeout_ms: None,
                client_message_id: None,
            },
            Some(&cid_allow),
        )
        .await
        .expect("allowed write turn");
    assert_eq!(allow_resp.run.status, RunStatus::Succeeded);
    assert_eq!(allow_resp.permission_decisions.len(), 1);
    assert_eq!(
        allow_resp.permission_decisions[0].effect,
        PolicyEffect::Allow
    );
    assert!(allow_resp.permission_decisions[0].evaluated_outside_model);
    assert_eq!(
        fs::read_to_string(profile.workspace_dir().join("docs").join("allowed.txt"))
            .expect("allowed file created"),
        "hello_v05"
    );

    // 2. Write outside `docs/` prefix is DENIED outside model!
    let cid_deny_prefix = CorrelationId::try_new("trace-v05-deny-prefix").expect("cid");
    let deny_prefix = client
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: String::from("[SIM:TOOL:write_file:src/forbidden.txt|nope]"),
                timeout_ms: None,
                client_message_id: None,
            },
            Some(&cid_deny_prefix),
        )
        .await
        .expect("denied prefix turn");
    assert_eq!(
        deny_prefix.permission_decisions[0].effect,
        PolicyEffect::Deny
    );
    assert!(deny_prefix.permission_decisions[0].evaluated_outside_model);
    assert!(
        !profile
            .workspace_dir()
            .join("src")
            .join("forbidden.txt")
            .exists()
    );

    // 3. Traversal escape `../escape.txt` is HARD DENIED even if an Allow rule exists!
    let _allow_rule = client
        .create_policy_rule(
            &CreatePolicyRuleRequest {
                scope: format!("session:{}", session.session_id),
                tool_name: String::from("write_file"),
                target_pattern: Some(String::from("*")),
                effect: PolicyEffect::Allow,
                reason: String::from("broad rule must NOT override workspace boundary"),
            },
            None,
        )
        .await
        .expect("create broad rule");

    let escape_resp = client
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: String::from("[SIM:TOOL:write_file:../escaped_file.txt|evil]"),
                timeout_ms: None,
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("escape turn");
    assert_eq!(
        escape_resp.permission_decisions[0].effect,
        PolicyEffect::Deny
    );
    assert!(!profile.root_dir().join("escaped_file.txt").exists());

    // 4. Dangerous shell tool is default-denied outside the model — INCLUDING when command/title
    //    contains `dennett-memory` or `memory_search` substrings!
    let shell_resp = client
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: String::from("[SIM:TOOL:run_command:rm -rf /]"),
                timeout_ms: None,
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("denied shell turn");
    assert_eq!(
        shell_resp.permission_decisions[0].effect,
        PolicyEffect::Deny
    );

    let spoofed_shell_resp = client
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: String::from(
                    "[SIM:TOOL:run_command:powershell -c whoami # dennett-memory memory_search]",
                ),
                timeout_ms: None,
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("spoofed shell turn");
    assert_eq!(
        spoofed_shell_resp.permission_decisions[0].effect,
        PolicyEffect::Deny,
        "shell command containing dennett-memory/memory_search must NEVER bypass default-deny"
    );

    // 4b. File reads fail closed when target is missing, and relative `Deny` rules (`secrets/*`)
    //     MUST block absolute paths inside the workspace!
    fs::create_dir_all(profile.workspace_dir().join("secrets")).expect("create secrets dir");
    let secret_file = profile.workspace_dir().join("secrets").join("key.pem");
    fs::write(&secret_file, "TOP_SECRET").expect("write secret file");
    let _deny_secrets = client
        .create_policy_rule(
            &CreatePolicyRuleRequest {
                scope: format!("session:{}", session.session_id),
                tool_name: String::from("read_file"),
                target_pattern: Some(String::from("secrets/*")),
                effect: PolicyEffect::Deny,
                reason: String::from("protect secrets directory"),
            },
            None,
        )
        .await
        .expect("create deny secrets rule");

    let abs_read_deny = client
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: format!("[SIM:TOOL:read_file:{}]", secret_file.display()),
                timeout_ms: None,
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("denied abs read turn");
    assert_eq!(
        abs_read_deny.permission_decisions[0].effect,
        PolicyEffect::Deny,
        "relative Deny rule `secrets/*` must block absolute workspace path"
    );

    let empty_read_deny = client
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: String::from("[SIM:TOOL:read_file:]"),
                timeout_ms: None,
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("denied missing-target read turn");
    assert_eq!(
        empty_read_deny.permission_decisions[0].effect,
        PolicyEffect::Deny,
        "read_file with missing target must fail closed"
    );

    let missing_scope_mem_write = client
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: String::from("[SIM:TOOL:memory_write_note:]"),
                timeout_ms: None,
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("denied missing-scope memory_write_note");
    assert_eq!(
        missing_scope_mem_write.permission_decisions[0].effect,
        PolicyEffect::Deny,
        "memory_write_note without mandatory scope must fail closed"
    );

    // 5. Partial text followed by runtime error -> HTTP error + RunStatus::Failed + secret redaction!
    let fake_secret = "AIzaSyFakeSecretKey9876543210987654321";
    let cid_err = CorrelationId::try_new("trace-a09-err-1").expect("cid");
    let err_res = client
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: format!("[SIM:ERROR:upstream crash GEMINI_API_KEY={fake_secret}]"),
                timeout_ms: None,
                client_message_id: None,
            },
            Some(&cid_err),
        )
        .await;
    assert!(
        matches!(err_res, Err(DennettError::RuntimeProtocol { .. })),
        "runtime error must fail closed, got: {err_res:?}"
    );

    // 6. Verify audit trace links correlation_id -> message -> decision -> outcome with ZERO secrets!
    let diag = client
        .get_diagnostics(Some(100), None)
        .await
        .expect("diagnostics");
    let diag_json = serde_json::to_string(&diag).expect("serialize diagnostics");
    assert!(
        !diag_json.contains(fake_secret),
        "fake GEMINI_API_KEY leaked in diagnostics: {diag_json}"
    );
    assert!(
        !diag_json.contains(mem_token),
        "DENNETT_MEMORY_TOKEN leaked in diagnostics: {diag_json}"
    );
    assert!(
        diag.events
            .iter()
            .any(|e| e.correlation_id == cid_allow && e.event_kind == "permission_allowed")
    );
    assert!(
        diag.events
            .iter()
            .any(|e| e.correlation_id == cid_deny_prefix && e.event_kind == "permission_denied")
    );
    assert!(
        diag.events
            .iter()
            .any(|e| e.correlation_id == cid_err && e.event_kind == "run_failed")
    );

    let detail = client
        .get_session_detail(session.session_id, None)
        .await
        .expect("session detail");
    assert!(
        detail
            .runs
            .iter()
            .any(|r| r.correlation_id == cid_err && r.status == RunStatus::Failed),
        "errored run with partial text must persist as Failed, NEVER Succeeded"
    );

    head.shutdown().await;
    memoryd.shutdown().await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "end-to-end A04/V29 test verifies two isolated sessions, AgentConfigVersion invariance, and explicit rehydration after restart"
)]
async fn a04_v29_session_isolation_model_config_version_invariance_and_rehydrate() {
    let (mem_pg, ctrl_pg) = require_pg_urls();
    let (profile, _tmp) = TestProfile::create_on_disk("head-a04").expect("temp profile");
    let scope = unique_scope("head-a04");
    let mem_token = "tok_head_a04_332211";
    let auth_cfg = make_memory_auth(&scope, mem_token);

    let memoryd = RunningDaemon::spawn_memoryd(&mem_pg, &auth_cfg).await;
    let head = RunningDaemon::spawn_head_simulated(&ctrl_pg, &memoryd.base_url, mem_token).await;
    let client = HeadClient::new(&head.base_url).expect("head client");

    let project = client
        .create_project(
            &CreateProjectRequest {
                name: String::from("Shared Workspace"),
                workspace_path: Some(profile.workspace_dir().to_string_lossy().into_owned()),
                memory_scope: Some(scope.clone()),
            },
            None,
        )
        .await
        .expect("create project");

    let agent_v1 = client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Versioned Analyst"),
                description: String::from("Tests A04 config versioning"),
                system_prompt: String::from("Prompt v1"),
                model_preference: Some(String::from("sim-model-alpha")),
                thinking_level: Some(ThinkingLevel::Low),
                allowed_memory_scopes: BTreeSet::from([scope.clone()]),
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
            },
            None,
        )
        .await
        .expect("create agent v1");

    // Create TWO isolated sessions in the SAME project workspace!
    let sess_a = client
        .create_session(
            &CreateSessionRequest {
                agent_id: agent_v1.agent_id,
                project_id: Some(project.project_id),
                title: String::from("Session Alpha"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("session A");
    let sess_b = client
        .create_session(
            &CreateSessionRequest {
                agent_id: agent_v1.agent_id,
                project_id: Some(project.project_id),
                title: String::from("Session Beta"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("session B");

    let first_alpha_turn = client
        .send_message(
            sess_a.session_id,
            &SendMessageRequest {
                content: String::from("FACT_ALPHA_ONLY_991"),
                timeout_ms: None,
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("turn A1");
    let first_beta_turn = client
        .send_message(
            sess_b.session_id,
            &SendMessageRequest {
                content: String::from("FACT_BETA_ONLY_773"),
                timeout_ms: None,
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("turn B1");

    assert_ne!(
        first_alpha_turn.runtime_binding.provider_session_id,
        first_beta_turn.runtime_binding.provider_session_id,
        "two sessions in same workspace must hold distinct provider_session_id bindings"
    );

    // Update model & thinking level -> creates new AgentConfigVersion with SAME AgentId!
    let agent_v2 = client
        .update_agent_config(
            agent_v1.agent_id,
            &UpdateAgentConfigRequest {
                system_prompt: String::from("Prompt v2 upgraded"),
                model_preference: Some(String::from("sim-model-beta")),
                thinking_level: Some(ThinkingLevel::High),
                allowed_memory_scopes: BTreeSet::from([scope]),
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
            },
            None,
        )
        .await
        .expect("update agent config");

    assert_eq!(
        agent_v2.agent_id, agent_v1.agent_id,
        "changing model/config MUST NOT create a new AgentId (A04 / 82 §2)"
    );
    assert_ne!(
        agent_v2.current_config_version_id,
        agent_v1.current_config_version_id
    );
    assert_eq!(agent_v2.current_config.version_number, 2);

    let history = client
        .get_agent_config_history(agent_v1.agent_id, None)
        .await
        .expect("config history");
    assert_eq!(history.versions.len(), 2);
    assert_eq!(history.versions[0].version_number, 1);
    assert_eq!(history.versions[1].version_number, 2);

    // Next turn on session A picks up new config version v2 while keeping same provider session!
    let turn_a2 = client
        .send_message(
            sess_a.session_id,
            &SendMessageRequest {
                content: String::from("Check updated model"),
                timeout_ms: None,
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("turn A2");
    assert_eq!(
        turn_a2.run.agent_config_version_id,
        agent_v2.current_config_version_id
    );
    assert!(turn_a2.assistant_message.as_ref().is_some_and(|m| {
        m.content.contains("model=sim-model-beta") && m.content.contains("thinking=high")
    }));
    assert!(!turn_a2.run.rehydrated);

    // Restart Head -> previous in-memory simulated provider session is gone -> triggers EXPLICIT rehydrate
    // using ONLY session A's history (zero leak from session B!).
    head.shutdown().await;
    let head2 = RunningDaemon::spawn_head_simulated(&ctrl_pg, &memoryd.base_url, mem_token).await;
    let client2 = HeadClient::new(&head2.base_url).expect("head client 2");

    let turn_a3 = client2
        .send_message(
            sess_a.session_id,
            &SendMessageRequest {
                content: String::from("After restart follow-up"),
                timeout_ms: None,
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("turn A3 rehydrated");
    assert!(
        turn_a3.run.rehydrated,
        "stale/missing provider session after restart must trigger explicit rehydration"
    );
    assert!(turn_a3.runtime_binding.rehydrate_count >= 1);
    let reply_a3 = turn_a3
        .assistant_message
        .expect("rehydrated assistant reply")
        .content;
    assert!(
        !reply_a3.contains("FACT_BETA_ONLY_773"),
        "session B fact leaked into rehydrated session A: {reply_a3}"
    );

    head2.shutdown().await;
    memoryd.shutdown().await;
}

#[tokio::test]
async fn head_pg_migration_idempotency_and_append_only_triggers() {
    let (mem_pg, ctrl_pg) = require_pg_urls();
    let scope = unique_scope("head-mig");
    let mem_token = "tok_head_mig_112233";
    let auth_cfg = make_memory_auth(&scope, mem_token);

    let memoryd = RunningDaemon::spawn_memoryd(&mem_pg, &auth_cfg).await;
    // Start Head once (applies migrations v1..v4), shut down, start again (must be idempotent!).
    let head1 = RunningDaemon::spawn_head_simulated(&ctrl_pg, &memoryd.base_url, mem_token).await;
    let c1 = HeadClient::new(&head1.base_url).expect("client1");
    let h1 = c1.healthz(None).await.expect("health1");
    assert_eq!(h1.schema_version, Some(9));
    head1.shutdown().await;

    let head2 = RunningDaemon::spawn_head_simulated(&ctrl_pg, &memoryd.base_url, mem_token).await;
    let c2 = HeadClient::new(&head2.base_url).expect("client2");
    let h2 = c2.healthz(None).await.expect("health2");
    assert_eq!(h2.schema_version, Some(9));

    // Seed one project inside this isolated schema so `audit_events` has a real row to guard.
    let _proj = c2
        .create_project(
            &CreateProjectRequest {
                name: String::from("Migration Trigger Seed"),
                workspace_path: None,
                memory_scope: Some(scope),
            },
            None,
        )
        .await
        .expect("seed audit row for append-only trigger");

    // Verify PostgreSQL append-only triggers reject UPDATE/DELETE on audit_events
    // and migration v3 installed the unique artifact revision constraint!
    let (pg_client, connection) = tokio_postgres::connect(&ctrl_pg, NoTls)
        .await
        .expect("connect control PG");
    tokio::spawn(async move {
        let _ = connection.await;
    });
    let update_err = pg_client
        .execute("UPDATE audit_events SET safe_summary = 'tampered'", &[])
        .await;
    assert!(
        update_err.is_err(),
        "append-only trigger on audit_events must reject UPDATE"
    );
    let unique_exists: bool = pg_client
        .query_one(
            "SELECT EXISTS(
                SELECT 1 FROM pg_constraint
                WHERE conname = 'artifacts_session_name_revision_unique'
            )",
            &[],
        )
        .await
        .expect("check unique constraint")
        .get(0);
    assert!(
        unique_exists,
        "migration v3 must install artifacts_session_name_revision_unique constraint"
    );

    head2.shutdown().await;
    memoryd.shutdown().await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "end-to-end integration test verifies catalog, folderless project, HMAC dynamic scope auth, inline rename, folder attach, and workspace file listing across real PG + memoryd + Head"
)]
async fn head_d1_rework_catalog_folderless_projects_inline_rename_and_hmac_scopes() {
    let (mem_pg, ctrl_pg) = require_pg_urls();
    let (profile, _tmp) = TestProfile::create_on_disk("head-d1-rework").expect("test profile");
    let ws_dir = profile.workspace_dir().join("attached-repo");
    let outside_dir = profile.workspace_dir().join("outside-target");
    fs::create_dir_all(ws_dir.join("src")).expect("create attached repo src");
    fs::create_dir_all(&outside_dir).expect("create outside dir");
    fs::write(ws_dir.join("README.md"), "# Attached Repo\n").expect("write readme");
    fs::write(ws_dir.join("src/app.rs"), "pub fn run() {}\n").expect("write app.rs");

    let link_root = profile.workspace_dir().join("evil-link-root");
    #[cfg(windows)]
    {
        let status = std::process::Command::new("cmd")
            .args([
                "/C",
                "mklink",
                "/J",
                &link_root.to_string_lossy(),
                &outside_dir.to_string_lossy(),
            ])
            .status()
            .expect("spawn mklink /J");
        assert!(status.success(), "mklink /J should create NTFS junction");
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&outside_dir, &link_root).expect("create symlink root");
    }

    let hmac_secret = "dennett-test-hmac-secret-0123456789abcdef";
    let static_token = "tok_rework_static_999";
    let auth_cfg = AuthConfig {
        audience: String::from(DEFAULT_AUDIENCE),
        allowed_issuers: vec![String::from(DEFAULT_ISSUER)],
        hmac_secret: Some(String::from(hmac_secret)),
        tokens: vec![StaticTokenEntryConfig {
            token_id: String::from("tok-static-only-default"),
            token: Some(String::from(static_token)),
            token_sha256: None,
            principal_id: PrincipalId::try_new("head-static").expect("principal"),
            issuer: String::from(DEFAULT_ISSUER),
            allowed_scopes: BTreeSet::from([
                MemoryScope::try_new("project:default").expect("scope")
            ]),
            capabilities: BTreeSet::from([MemoryCapability::Read, MemoryCapability::Write]),
            expires_at: None,
            revoked: false,
        }],
        revoked_token_ids: BTreeSet::new(),
    };

    let memoryd = RunningDaemon::spawn_memoryd(&mem_pg, &auth_cfg).await;
    let head = RunningDaemon::spawn_head_simulated_with_hmac(
        &ctrl_pg,
        &memoryd.base_url,
        static_token,
        hmac_secret,
    )
    .await;
    let client = HeadClient::new(&head.base_url).expect("head client");

    // 1. Dynamic runtime catalog returns honest SIMULATED models & thinking options (zero fake Gemini 3.8 Pro).
    let catalog = client
        .get_runtime_catalog(false, None)
        .await
        .expect("catalog");
    assert!(catalog.is_simulated);
    assert_eq!(catalog.tier.as_deref(), Some("SIMULATED_CI_ONLY"));
    assert!(
        catalog
            .models
            .iter()
            .any(|m| m.model_id == "sim-model-alpha")
    );
    assert_ne!(catalog.thinking_options.len(), 0);
    assert_eq!(catalog.quotas, Vec::new());

    // 2. Folderless project creation ("Пока без папки") with auto-generated unique memory_scope.
    let project = client
        .create_project(
            &CreateProjectRequest {
                name: String::from("Новый проект"),
                workspace_path: None,
                memory_scope: None,
            },
            None,
        )
        .await
        .expect("create folderless project");
    assert!(project.workspace_path.is_none());
    assert!(project.memory_scope.as_str().starts_with("project:"));

    // Unbound project files listing reports workspace_bound=false cleanly.
    let unbound_files = client
        .list_project_files(project.project_id, None, None)
        .await
        .expect("unbound files");
    assert!(!unbound_files.workspace_bound);
    assert_eq!(unbound_files.entries, Vec::new());

    // 3. Dynamic HMAC `dnt1.` scope auth: committing & searching in the freshly auto-generated
    //    project scope succeeds immediately WITHOUT `403 ForbiddenScope`!
    let commit = client
        .commit_memory_note(
            &CommitMemoryNoteRequest {
                session_id: None,
                run_id: None,
                scope: project.memory_scope.clone(),
                title: String::from("Dynamic scope note"),
                body: String::from("Signed dnt1 token works for newly created project."),
                tags: vec![String::from("rework")],
                idempotency_key: Some(IdempotencyKey::generate()),
            },
            None,
        )
        .await
        .expect("commit dynamic scope note");
    assert!(commit.delivered_immediately);
    assert_eq!(commit.outbox_item.status, MemoryOutboxStatus::Delivered);

    let search = client
        .search_memory(
            &HeadMemorySearchRequest {
                scope: Some(project.memory_scope.clone()),
                query: Some(String::from("Dynamic scope")),
                limit: Some(5),
                include_history: false,
            },
            None,
        )
        .await
        .expect("search dynamic scope");
    assert_eq!(search.memory_status, MemoryStatus::Healthy);
    assert_eq!(search.notes.len(), 1);

    // 4. Create agent + blank "Новый чат", send first message -> auto-titles session & enforces DenyAll while folderless.
    let agent = client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Проектный агент"),
                description: String::from("Dynamic scope agent"),
                system_prompt: String::from("Be helpful."),
                model_preference: Some(String::from("sim-model-alpha")),
                thinking_level: Some(ThinkingLevel::High),
                allowed_memory_scopes: BTreeSet::from([project.memory_scope.clone()]),
                workspace_write_policy: WorkspaceWritePolicy::AllowWorkspace,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
            },
            None,
        )
        .await
        .expect("create agent");

    let session = client
        .create_session(
            &CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: Some(project.project_id),
                title: String::from("Новый чат"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("create session");

    let turn = client
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: String::from("## План рефакторинга фронтенда\nПодробности"),
                timeout_ms: Some(15_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("first turn");
    assert_eq!(turn.session_title, "План рефакторинга фронтенда");
    assert!(
        turn.assistant_message
            .as_ref()
            .is_some_and(|m| m.content.contains("write_policy=DenyAll")),
        "folderless project must force DenyAll write policy, got: {:?}",
        turn.assistant_message
    );

    // 5. Manual inline session rename via PATCH /v1/sessions/{id}.
    let renamed_session = client
        .update_session(
            session.session_id,
            &UpdateSessionRequest {
                title: Some(String::from("Переименованный чат")),
                pinned: None,
            },
            None,
        )
        .await
        .expect("rename session");
    assert_eq!(renamed_session.title, "Переименованный чат");

    // 6. Attach folder + inline rename project via PATCH /v1/projects/{id}, then list real files & verify traversal rejection.
    let updated_project = client
        .update_project(
            project.project_id,
            &UpdateProjectRequest {
                name: Some(String::from("Мой репозиторий")),
                workspace_path: Some(ws_dir.to_string_lossy().into_owned()),
                pinned: None,
            },
            None,
        )
        .await
        .expect("update project");
    assert_eq!(updated_project.name, "Мой репозиторий");
    assert!(updated_project.workspace_path.is_some());

    let files = client
        .list_project_files(project.project_id, None, None)
        .await
        .expect("list bound project files");
    assert!(files.workspace_bound);
    assert!(files.entries.iter().any(|e| e.relative_path == "README.md"));
    assert!(
        files
            .entries
            .iter()
            .any(|e| e.relative_path == "src/app.rs")
    );

    let traversal_err = client
        .list_project_files(project.project_id, Some("../outside"), None)
        .await;
    assert!(
        matches!(traversal_err, Err(DennettError::AccessDenied { .. })),
        "path traversal must return 403 AccessDenied, got: {traversal_err:?}"
    );

    // 7. BOTH POST /v1/projects and PATCH /v1/projects/{id} reject relative, `..`, UNC, and symlink/junction roots with 403 AccessDenied.
    let link_root_str = link_root.to_string_lossy().into_owned();
    for bad_workspace in [
        ".",
        "../outside",
        r"\\attacker\share",
        link_root_str.as_str(),
    ] {
        let create_err = client
            .create_project(
                &CreateProjectRequest {
                    name: String::from("Unsafe Create"),
                    workspace_path: Some(bad_workspace.to_owned()),
                    memory_scope: None,
                },
                None,
            )
            .await;
        assert!(
            matches!(create_err, Err(DennettError::AccessDenied { .. })),
            "POST /v1/projects must reject `{bad_workspace}` with 403 AccessDenied, got: {create_err:?}"
        );

        let patch_err = client
            .update_project(
                project.project_id,
                &UpdateProjectRequest {
                    name: None,
                    workspace_path: Some(bad_workspace.to_owned()),
                    pinned: None,
                },
                None,
            )
            .await;
        assert!(
            matches!(patch_err, Err(DennettError::AccessDenied { .. })),
            "PATCH /v1/projects/{{id}} must reject `{bad_workspace}` with 403 AccessDenied, got: {patch_err:?}"
        );
    }

    head.shutdown().await;
    memoryd.shutdown().await;
}

async fn raw_http_request(base_url: &str, raw_request: &str) -> (u16, String) {
    let addr = base_url
        .strip_prefix("http://")
        .expect("loopback http:// base_url");
    let mut stream = TcpStream::connect(addr).await.expect("connect loopback");
    stream
        .write_all(raw_request.as_bytes())
        .await
        .expect("write raw http");
    stream.flush().await.expect("flush raw http");

    let mut raw_resp = Vec::new();
    stream
        .read_to_end(&mut raw_resp)
        .await
        .expect("read raw http response");
    let text = String::from_utf8_lossy(&raw_resp).into_owned();
    let status_code = text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .expect("valid HTTP status line");
    let body = text
        .split_once("\r\n\r\n")
        .map_or_else(String::new, |(_, b)| b.to_owned());
    (status_code, body)
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "verifies H1-01 Host/Origin/Sec-Fetch-Site guards, H1-05 xhigh JSON wire roundtrip, H1-07 concurrent artifact serialization and streaming hash, and H1-08 dennettctl parity subcommands against live PG + memoryd + Head"
)]
async fn head_h1_security_guards_concurrent_artifacts_and_dennettctl_parity() {
    let (mem_pg, ctrl_pg) = require_pg_urls();
    let (profile, _tmp) = TestProfile::create_on_disk("head-h1-hardening").expect("temp profile");
    let ws_dir = profile.workspace_dir().join("parity-repo");
    fs::create_dir_all(ws_dir.join("src")).expect("create src");
    fs::write(ws_dir.join("README.md"), "# Parity Repo\n").expect("write readme");
    fs::write(ws_dir.join("src/lib.rs"), "pub fn ok() {}\n").expect("write lib.rs");

    let scope = unique_scope("head-h1-parity");
    let mem_token = "tok_h1_parity_556677";
    let auth_cfg = make_memory_auth(&scope, mem_token);

    let memoryd = RunningDaemon::spawn_memoryd(&mem_pg, &auth_cfg).await;
    let head = RunningDaemon::spawn_head_simulated(&ctrl_pg, &memoryd.base_url, mem_token).await;
    let client = HeadClient::new(&head.base_url).expect("head client");
    let host_port = head.base_url.strip_prefix("http://").expect("host:port");

    // H1-01: DNS-rebinding Host, cross-site Origin, and Sec-Fetch-Site: cross-site are rejected with 403.
    let (rebind_status, _) = raw_http_request(
        &head.base_url,
        "GET /v1/projects HTTP/1.1\r\nHost: evil.rebind.attacker:41771\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert_eq!(rebind_status, 403);

    let (evil_origin_status, _) = raw_http_request(
        &head.base_url,
        &format!(
            "POST /v1/memory/outbox/flush HTTP/1.1\r\nHost: {host_port}\r\nOrigin: https://evil.example\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        ),
    )
    .await;
    assert_eq!(evil_origin_status, 403);

    let (cross_site_status, _) = raw_http_request(
        &head.base_url,
        &format!(
            "POST /v1/memory/outbox/flush HTTP/1.1\r\nHost: {host_port}\r\nSec-Fetch-Site: cross-site\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        ),
    )
    .await;
    assert_eq!(cross_site_status, 403);

    let (tauri_origin_status, _) = raw_http_request(
        &head.base_url,
        &format!(
            "GET /v1/projects HTTP/1.1\r\nHost: {host_port}\r\nOrigin: http://tauri.localhost\r\nSec-Fetch-Site: same-origin\r\nConnection: close\r\n\r\n"
        ),
    )
    .await;
    assert_eq!(tauri_origin_status, 200);

    // H1-05: Raw HTTP JSON uses canonical `"xhigh"` (never `"x_high"`).
    let (cat_status, cat_json) = raw_http_request(
        &head.base_url,
        &format!(
            "GET /v1/runtime/catalog HTTP/1.1\r\nHost: {host_port}\r\nConnection: close\r\n\r\n"
        ),
    )
    .await;
    assert_eq!(cat_status, 200);
    assert!(cat_json.contains("\"xhigh\""));
    assert!(!cat_json.contains("\"x_high\""));

    let xhigh_body = serde_json::json!({
        "name": "XHigh Agent",
        "description": "Wire test",
        "system_prompt": "Test xhigh wire serialization.",
        "thinking_level": "xhigh",
        "allowed_memory_scopes": [scope.as_str()],
        "workspace_write_policy": { "mode": "allow_workspace" }
    })
    .to_string();
    let (xhigh_status, xhigh_json) = raw_http_request(
        &head.base_url,
        &format!(
            "POST /v1/agents HTTP/1.1\r\nHost: {host_port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{xhigh_body}",
            xhigh_body.len()
        ),
    )
    .await;
    assert_eq!(xhigh_status, 201, "create xhigh agent body: {xhigh_json}");
    let xhigh_agent: AgentRecord = serde_json::from_str(&xhigh_json).expect("parse agent");
    assert_eq!(
        xhigh_agent.current_config.thinking_level,
        Some(ThinkingLevel::XHigh)
    );

    // H1-07: Concurrent create_artifact calls serialize cleanly with sequential revisions [1..=4],
    // and workspace-backed streaming SHA-256 hashes regular files while rejecting directories.
    let project = client
        .create_project(
            &CreateProjectRequest {
                name: String::from("Parity Project"),
                workspace_path: Some(ws_dir.to_string_lossy().into_owned()),
                memory_scope: Some(scope.clone()),
            },
            None,
        )
        .await
        .expect("create project");
    let session = client
        .create_session(
            &CreateSessionRequest {
                agent_id: xhigh_agent.agent_id,
                project_id: Some(project.project_id),
                title: String::from("Concurrency Session"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("create session");

    let mk_art = |idx: u32| {
        let c = client.clone();
        let sid = session.session_id;
        async move {
            c.create_artifact(
                sid,
                &CreateArtifactRequest {
                    run_id: None,
                    name: String::from("concurrent.md"),
                    mime_type: String::from("text/markdown"),
                    content: Some(format!("revision body {idx}")),
                    relative_path: None,
                    completeness_status: dennett_contracts::ArtifactCompleteness::Final,
                },
                None,
            )
            .await
        }
    };
    let (a1, a2, a3, a4) = tokio::try_join!(mk_art(1), mk_art(2), mk_art(3), mk_art(4))
        .expect("concurrent create_artifact calls must serialize cleanly");
    let mut revs = vec![a1.revision, a2.revision, a3.revision, a4.revision];
    revs.sort_unstable();
    assert_eq!(revs, vec![1, 2, 3, 4]);

    let file_art = client
        .create_artifact(
            session.session_id,
            &CreateArtifactRequest {
                run_id: None,
                name: String::from("readme-ref"),
                mime_type: String::from("text/markdown"),
                content: None,
                relative_path: Some(String::from("README.md")),
                completeness_status: dennett_contracts::ArtifactCompleteness::Final,
            },
            None,
        )
        .await
        .expect("workspace-backed streaming artifact");
    assert_eq!(file_art.relative_path.as_deref(), Some("README.md"));
    assert_eq!(file_art.sha256.as_str().len(), 64);

    let dir_art_err = client
        .create_artifact(
            session.session_id,
            &CreateArtifactRequest {
                run_id: None,
                name: String::from("dir-ref"),
                mime_type: String::from("text/plain"),
                content: None,
                relative_path: Some(String::from("src")),
                completeness_status: dennett_contracts::ArtifactCompleteness::Final,
            },
            None,
        )
        .await;
    assert!(
        matches!(dir_art_err, Err(DennettError::InvalidInput { .. })),
        "directory target must be rejected for workspace artifact, got: {dir_art_err:?}"
    );

    // H1-08: dennettctl CLI parity for runtime catalog, project update, project files, session update.
    let (cat_code, cat_out, cat_err) =
        run_dennettctl(&head.base_url, &["--json", "runtime", "catalog"]).await;
    assert_eq!(cat_code, 0, "dennettctl runtime catalog failed: {cat_err}");
    assert!(cat_out.contains("sim-model-alpha"));

    let pid_str = project.project_id.to_string();
    let (proj_upd_code, proj_upd_out, proj_upd_err) = run_dennettctl(
        &head.base_url,
        &[
            "--json",
            "project",
            "update",
            "--id",
            &pid_str,
            "--name",
            "Renamed Via CLI",
        ],
    )
    .await;
    assert_eq!(
        proj_upd_code, 0,
        "dennettctl project update failed: {proj_upd_err}"
    );
    assert!(proj_upd_out.contains("Renamed Via CLI"));

    let (files_code, files_out, files_err) = run_dennettctl(
        &head.base_url,
        &["--json", "project", "files", "--id", &pid_str],
    )
    .await;
    assert_eq!(
        files_code, 0,
        "dennettctl project files failed: {files_err}"
    );
    assert!(files_out.contains("README.md"));
    assert!(files_out.contains("src/lib.rs"));

    let sid_str = session.session_id.to_string();
    let (sess_upd_code, sess_upd_out, sess_upd_err) = run_dennettctl(
        &head.base_url,
        &[
            "--json",
            "session",
            "update",
            "--id",
            &sid_str,
            "--title",
            "Renamed Session Via CLI",
        ],
    )
    .await;
    assert_eq!(
        sess_upd_code, 0,
        "dennettctl session update failed: {sess_upd_err}"
    );
    assert!(sess_upd_out.contains("Renamed Session Via CLI"));

    head.shutdown().await;
    memoryd.shutdown().await;
}
