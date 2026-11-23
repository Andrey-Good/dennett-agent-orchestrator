//! Real live `agy acp 1.5.15+` end-to-end tests for Dennett D1 Slice 2 (`V04`, `A01–A06`, `A09`).
//!
//! Marked `#[ignore]` so offline unit/integration suites stay deterministic; run via:
//! `uv run --project ops/runner dennett-check runtime-probe`
//!
//! Spawns REAL standalone `dennett-memoryd` + REAL `dennett-head --runtime antigravity` +
//! REAL `dennett-memory-mcp` + REAL `dennettctl` against isolated Docker PostgreSQL and an
//! isolated `--gemini-dir` profile (leaving `~/.gemini/antigravity-cli/conversations` untouched).

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use dennett_contracts::{
    CreateAgentRequest, CreateArtifactRequest, CreateNoteRequest, CreateProjectRequest,
    CreateSessionRequest, DennettError, IdempotencyKey, MemoryCapability, MemoryScope,
    MemoryStatus, PolicyEffect, PrincipalId, RunStatus, RuntimeAuthProfile, SearchNotesRequest,
    SendMessageRequest, TimestampMs, WorkspaceWritePolicy,
};
use dennett_head_client::HeadClient;
use dennett_memory_client::MemoryClient;
use dennett_memory_core::{AuthConfig, DEFAULT_AUDIENCE, DEFAULT_ISSUER, StaticTokenEntryConfig};
use dennett_testkit::profile::{FaultInjectionGuard, TEST_PROFILE_ENV, TestProfile};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::time;
use uuid::Uuid;

const LIVE_PROBE_ENV: &str = "DENNETT_LIVE_ACP_PROBE";

fn require_live_e2e_env(
    label: &str,
) -> (
    String,
    String,
    TestProfile,
    dennett_testkit::profile::TempProfileDir,
    FaultInjectionGuard,
) {
    assert_eq!(
        env::var(TEST_PROFILE_ENV).ok().as_deref(),
        Some("1"),
        "{TEST_PROFILE_ENV}=1 is required"
    );
    assert_eq!(
        env::var(LIVE_PROBE_ENV).ok().as_deref(),
        Some("1"),
        "{LIVE_PROBE_ENV}=1 is required"
    );
    let mem_pg = env::var("DENNETT_TEST_MEMORY_PG_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .expect("DENNETT_TEST_MEMORY_PG_URL is required (run via `dennett-check runtime-probe`)");
    let ctrl_pg = env::var("DENNETT_TEST_CONTROL_PG_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .expect("DENNETT_TEST_CONTROL_PG_URL is required (run via `dennett-check runtime-probe`)");

    let (profile, temp_dir) =
        TestProfile::create_on_disk(label).expect("create isolated test profile");
    let guard = FaultInjectionGuard::acquire(&profile).expect("acquire fault injection guard");
    (mem_pg, ctrl_pg, profile, temp_dir, guard)
}

fn locate_binary(bin_name: &str) -> PathBuf {
    let exe_name = if cfg!(windows) {
        format!("{bin_name}.exe")
    } else {
        bin_name.to_owned()
    };
    let current = env::current_exe().expect("current_exe");
    for dir in current.ancestors().skip(1).take(6) {
        let candidate = dir.join(&exe_name);
        if candidate.is_file() {
            return candidate;
        }
    }
    panic!("could not locate prebuilt binary `{exe_name}` near test executable");
}

struct LiveDaemon {
    child: Child,
    addr: String,
    base_url: String,
}

impl LiveDaemon {
    async fn spawn_memoryd(pg_url: &str, auth_config: &AuthConfig) -> Self {
        let bin = locate_binary("dennett-memoryd");
        let auth_json = serde_json::to_string(auth_config).expect("serialize auth");
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

    async fn spawn_head_antigravity(
        control_pg_url: &str,
        memory_url: &str,
        memory_token: &str,
        auth_profile: RuntimeAuthProfile,
        profile_root: &Path,
    ) -> Self {
        let bin = locate_binary("dennett-head");
        let mcp_bin = locate_binary("dennett-memory-mcp");
        let mut cmd = Command::new(&bin);
        cmd.args([
            "--serve",
            "127.0.0.1:0",
            "--runtime",
            "antigravity",
            "--auth-profile",
            auth_profile.as_method_id(),
            "--profile-root",
            &profile_root.to_string_lossy(),
            "--memory-url",
            memory_url,
            "--memory-mcp-bin",
            &mcp_bin.to_string_lossy(),
        ])
        .env(TEST_PROFILE_ENV, "1")
        .env("DENNETT_TEST_CONTROL_PG_URL", control_pg_url)
        .env("DENNETT_MEMORY_TOKEN", memory_token)
        .env_remove("DENNETT_CONTROL_PG_URL")
        .env_remove("GEMINI_API_KEY")
        .env_remove("DENNETT_GEMINI_API_KEY")
        .env_remove("GOOGLE_API_KEY")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

        let child = cmd.spawn().expect("spawn live dennett-head");
        Self::await_listening(child, "dennett-head").await
    }

    async fn await_listening(mut child: Child, label: &str) -> Self {
        let stdout = child.stdout.take().expect("stdout piped");
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        match time::timeout(Duration::from_secs(15), reader.read_line(&mut line)).await {
            Ok(Ok(n)) if n > 0 => {}
            other => panic!("did not receive LISTENING from {label}: {other:?}"),
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
        if let Ok(Ok(mut stream)) =
            time::timeout(Duration::from_secs(5), TcpStream::connect(&self.addr)).await
        {
            let _ = stream.write_all(req.as_bytes()).await;
            let mut resp = Vec::new();
            let _ = time::timeout(Duration::from_secs(5), stream.read_to_end(&mut resp)).await;
        }
        let status = time::timeout(Duration::from_secs(10), self.child.wait())
            .await
            .expect("wait shutdown exit")
            .expect("child wait");
        assert!(status.success(), "daemon exited abnormally: {status}");
    }
}

#[tokio::test]
#[ignore = "live agy acp 1.5.15+ D1 E2E probe; run via dennett-check runtime-probe"]
#[expect(
    clippy::too_many_lines,
    reason = "end-to-end live agy acp D1 vertical slice verifies V01/V04/V05/A01-A06 across one realistic daemon lifecycle"
)]
async fn live_d1_e2e_preseeded_memoryd_head_agy_mcp_restart_and_policies() {
    let (mem_pg, ctrl_pg, profile, _tmp, _guard) = require_live_e2e_env("d1-live-e2e");
    let suffix = &Uuid::new_v4().simple().to_string()[..10];
    let scope = MemoryScope::try_new(format!("d1-live-{suffix}")).expect("scope");
    let mem_token = format!("tok_d1_live_scoped_{suffix}");
    let constraint_marker = format!("DENNETT_D1_CONSTRAINT_{suffix}");
    let updated_decision_marker = format!("DENNETT_D1_UPDATED_{suffix}");

    let auth_cfg = AuthConfig {
        audience: String::from(DEFAULT_AUDIENCE),
        allowed_issuers: vec![String::from(DEFAULT_ISSUER)],
        hmac_secret: None,
        tokens: vec![StaticTokenEntryConfig {
            token_id: format!("live-{suffix}"),
            token: Some(mem_token.clone()),
            token_sha256: None,
            principal_id: PrincipalId::try_new("agent:d1-live-agy").expect("principal"),
            issuer: String::from(DEFAULT_ISSUER),
            allowed_scopes: BTreeSet::from([scope.clone()]),
            capabilities: BTreeSet::from([MemoryCapability::Read, MemoryCapability::Write]),
            expires_at: Some(TimestampMs::from_millis(
                TimestampMs::now().as_millis() + 3_600_000,
            )),
            revoked: false,
        }],
        revoked_token_ids: BTreeSet::new(),
    };

    // STEP 1 (V01 / A03): Start standalone `dennett-memoryd` while `dennett-head` is STOPPED,
    // and pre-seed a project constraint directly into PostgreSQL via `MemoryClient`!
    let memoryd = LiveDaemon::spawn_memoryd(&mem_pg, &auth_cfg).await;
    let mem_client = MemoryClient::new(&memoryd.base_url, &mem_token).expect("memory client");
    let seeded = mem_client
        .create_note(
            &CreateNoteRequest {
                scope: scope.clone(),
                title: String::from("Mandatory Project Constraint"),
                body: format!(
                    "Project constraint for scope `{scope}`: whenever asked to generate the summary \
                     report, you MUST write `docs/summary.txt` containing the exact token `{constraint_marker}`."
                ),
                tags: vec![String::from("constraint"), String::from("d1")],
                observed_at: None,
                evidence_links: Vec::new(),
                author_label: Some(String::from("external-preseed")),
                idempotency_key: IdempotencyKey::generate(),
            },
            None,
        )
        .await
        .expect("pre-seed note in standalone memoryd while Head is stopped");
    assert_eq!(seeded.note.current_revision, 1);

    // STEP 2 (A01 / A02 / A03 / V04 / V05): Start live `dennett-head` wired to `AntigravityAcpRuntime`
    // (`OAuthPersonal` Google AI Ultra) + real `dennett-memory-mcp` in an isolated `--gemini-dir`!
    let acp_profile_root = profile.data_dir().join("head-acp-profiles");
    let head = LiveDaemon::spawn_head_antigravity(
        &ctrl_pg,
        &memoryd.base_url,
        &mem_token,
        RuntimeAuthProfile::OAuthPersonal,
        &acp_profile_root,
    )
    .await;
    let head_client = HeadClient::new(&head.base_url).expect("head client");

    let health = head_client.healthz(None).await.expect("live head healthz");
    assert!(health.postgres_connected);
    assert_eq!(health.memory_status, MemoryStatus::Healthy);
    assert_eq!(health.runtime_adapter_id, "antigravity-acp");
    assert!(
        !health.runtime_is_simulated,
        "live Head MUST report runtime_is_simulated=false"
    );

    let project = head_client
        .create_project(
            &CreateProjectRequest {
                name: String::from("Live D1 Project"),
                workspace_path: profile.workspace_dir().to_string_lossy().into_owned(),
                memory_scope: scope.clone(),
            },
            None,
        )
        .await
        .expect("create project");

    // Non-coding agent persona (A02) with AllowPaths(["docs/"]) so writes inside `docs/` are
    // permitted via PolicyEngine while writes outside `docs/` are blocked!
    let agent = head_client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Project Archivist"),
                description: String::from("Non-coding archivist consulting Memory Fabric"),
                system_prompt: String::from(
                    "You are a precise Dennett Project Archivist (not a coding assistant). \
                     Always consult the `dennett-memory` MCP tools when asked for project constraints.",
                ),
                model_preference: Some(String::from("flash")),
                thinking_level: Some(dennett_contracts::ThinkingLevel::Low),
                allowed_memory_scopes: BTreeSet::from([scope.clone()]),
                workspace_write_policy: WorkspaceWritePolicy::AllowPaths {
                    allowed_prefixes: vec![String::from("docs/")],
                },
            },
            None,
        )
        .await
        .expect("create agent");

    let session = head_client
        .create_session(
            &CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: project.project_id,
                title: String::from("D1 Live Vertical Slice"),
            },
            None,
        )
        .await
        .expect("create session");

    // STEP 3 (V04 / A03 / A05 AllowOnce): Real instruction through Head -> live `agy acp` ->
    // real `dennett-memory-mcp` (`memory_search` + `memory_write_note`) + allowed `write_file` into `docs/summary.txt`!
    let prompt_turn1 = format!(
        "Do these three steps in order:\n\
         1. Call the MCP tool `memory_search` with scope \"{scope}\" to read the Mandatory Project Constraint.\n\
         2. Use `write_file` to create `docs/summary.txt` containing the exact constraint token you found in memory.\n\
         3. Call the MCP tool `memory_write_note` with scope \"{scope}\", title \"Archivist Decision\", \
            and body \"Completed report with {updated_decision_marker}\".\n\
         Then reply briefly confirming all three steps."
    );
    let turn1 = head_client
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: prompt_turn1,
                timeout_ms: Some(120_000),
            },
            None,
        )
        .await
        .expect("live agy acp turn 1");

    assert_eq!(turn1.run.status, RunStatus::Succeeded);
    let turn1_assistant = turn1
        .assistant_message
        .as_ref()
        .expect("turn 1 succeeded and must persist an assistant reply");
    assert_eq!(turn1.runtime_binding.adapter_id, "antigravity-acp");
    assert!(!turn1.runtime_binding.is_simulated);
    assert_eq!(
        turn1.runtime_binding.auth_profile,
        RuntimeAuthProfile::OAuthPersonal
    );
    assert!(
        turn1
            .runtime_binding
            .tier
            .as_deref()
            .is_some_and(|t| !t.trim().is_empty()),
        "live oauth-personal must report non-empty Google AI Ultra tier: {:?}",
        turn1.runtime_binding
    );
    assert!(
        turn1
            .permission_decisions
            .iter()
            .any(|d| d.effect == PolicyEffect::Allow && d.evaluated_outside_model),
        "expected external PolicyEngine AllowOnce decisions, got: {:?}",
        turn1.permission_decisions
    );

    let summary_file = profile.workspace_dir().join("docs").join("summary.txt");
    let summary_text = fs::read_to_string(&summary_file).unwrap_or_else(|err| {
        panic!(
            "live agy acp did not create `{}`: {err}; reply={:?}; decisions={:?}",
            summary_file.display(),
            turn1.assistant_message,
            turn1.permission_decisions
        )
    });
    assert!(
        summary_text.contains(&constraint_marker),
        "docs/summary.txt did not contain pre-seeded memory constraint `{constraint_marker}`; got `{summary_text}`"
    );

    // Register the real workspace file as an ArtifactRecord in Head!
    let art = head_client
        .create_artifact(
            session.session_id,
            &CreateArtifactRequest {
                run_id: Some(turn1.run.run_id),
                name: String::from("summary.txt"),
                mime_type: String::from("text/plain"),
                content: None,
                relative_path: Some(String::from("docs/summary.txt")),
            },
            None,
        )
        .await
        .expect("register summary.txt artifact");
    assert_eq!(art.relative_path.as_deref(), Some("docs/summary.txt"));

    // Confirm the agent's new decision note was ACTUALLY persisted in standalone `dennett-memoryd`!
    let updated_notes = mem_client
        .search_notes(
            &SearchNotesRequest {
                scope: Some(scope.clone()),
                query: Some(updated_decision_marker.clone()),
                limit: Some(10),
            },
            None,
        )
        .await
        .expect("query memoryd for agent's written decision");
    assert!(
        updated_notes
            .notes
            .iter()
            .any(|n| n.body.contains(&updated_decision_marker)),
        "expected live agy acp to persist `{updated_decision_marker}` in memoryd via dennett-memory-mcp; found={:?}",
        updated_notes.notes
    );

    // STEP 4 (A05 / V05 — D0 finding fix verified LIVE!):
    // Ask live `agy acp` to write a file OUTSIDE `docs/` (`forbidden/blocked.txt`) AND execute a shell command.
    // Our isolated PreToolUse gate + PolicyEngine MUST block BOTH (`RejectOnce`)!
    let blocked_file = profile
        .workspace_dir()
        .join("forbidden")
        .join("blocked.txt");
    let turn2 = head_client
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: String::from(
                    "Attempt to create `forbidden/blocked.txt` containing `should_fail` using `write_file` right now. \
                     If that is denied, also try `run_command` to echo `should_fail`. Report the denial.",
                ),
                timeout_ms: Some(90_000),
            },
            None,
        )
        .await
        .expect("live deny turn 2");
    assert!(
        !blocked_file.exists(),
        "PolicyEngine + isolated write-gate hook MUST prevent creating `{}`!",
        blocked_file.display()
    );
    assert!(
        turn2
            .permission_decisions
            .iter()
            .any(|d| d.effect == PolicyEffect::Deny && d.evaluated_outside_model),
        "expected audited external Deny decision on live agy acp, got: {:?}",
        turn2.permission_decisions
    );

    // STEP 5 (A04): Restart BOTH `dennett-head` and `dennett-memoryd` and verify persistence!
    head.shutdown().await;
    memoryd.shutdown().await;

    let memoryd2 = LiveDaemon::spawn_memoryd(&mem_pg, &auth_cfg).await;
    let head2 = LiveDaemon::spawn_head_antigravity(
        &ctrl_pg,
        &memoryd2.base_url,
        &mem_token,
        RuntimeAuthProfile::OAuthPersonal,
        &acp_profile_root,
    )
    .await;
    let head_client2 = HeadClient::new(&head2.base_url).expect("head client 2");
    let mem_client2 = MemoryClient::new(&memoryd2.base_url, &mem_token).expect("mem client 2");

    let detail = head_client2
        .get_session_detail(session.session_id, None)
        .await
        .expect("session detail after restart");
    // In `agy acp 1.5.15`, rejecting a tool permission (`reject_once`) may immediately end turn 2
    // without emitting assistant prose, so turn 2 always persists its user message + run + deny
    // decision and optionally an assistant message.
    let expected_messages = 2 + 1 + usize::from(turn2.assistant_message.is_some());
    assert_eq!(
        detail.messages.len(),
        expected_messages,
        "lossless message persistence across restart failed: turn2.status={:?}, turn2.assistant={:?}, persisted={:?}",
        turn2.run.status,
        turn2.assistant_message,
        detail.messages
    );
    assert_eq!(detail.runs.len(), 2);
    assert!(
        detail
            .messages
            .iter()
            .any(|m| m.message_id == turn1.user_message.message_id)
            && detail
                .messages
                .iter()
                .any(|m| m.message_id == turn1_assistant.message_id)
            && detail
                .messages
                .iter()
                .any(|m| m.message_id == turn2.user_message.message_id)
    );
    assert_eq!(detail.artifacts.len(), 1);
    assert_eq!(detail.artifacts[0].artifact_id, art.artifact_id);
    assert!(
        detail
            .permission_decisions
            .iter()
            .any(|d| d.effect == PolicyEffect::Allow)
            && detail
                .permission_decisions
                .iter()
                .any(|d| d.effect == PolicyEffect::Deny)
    );

    let persisted_notes = mem_client2
        .search_notes(
            &SearchNotesRequest {
                scope: Some(scope),
                query: Some(updated_decision_marker.clone()),
                limit: Some(10),
            },
            None,
        )
        .await
        .expect("memory search after restart");
    assert!(
        persisted_notes
            .notes
            .iter()
            .any(|n| n.body.contains(&updated_decision_marker))
    );

    // STEP 6 (A06): Live Stop / `session/cancel` via Head HTTP API!
    let client_cancel = head_client2.clone();
    let sid = session.session_id;
    let long_task = tokio::spawn(async move {
        client_cancel
            .send_message(
                sid,
                &SendMessageRequest {
                    content: String::from(
                        "Write a detailed numbered catalog from 1 to 350 of distinct fictional \
                         botanical species with three full sentences each. Do not abbreviate.",
                    ),
                    timeout_ms: Some(90_000),
                },
                None,
            )
            .await
    });

    // Wait until the 3rd run is visibly recorded as Running, then allow agy acp time to resume
    // and start streaming before sending Stop!
    let run_deadline = time::Instant::now() + Duration::from_secs(15);
    loop {
        let snap = head_client2
            .get_session_detail(session.session_id, None)
            .await
            .expect("poll session detail for active run");
        if snap.runs.len() >= 3 && snap.runs.iter().any(|r| r.status == RunStatus::Running) {
            break;
        }
        assert!(
            time::Instant::now() < run_deadline,
            "timed out waiting for live turn 3 to enter Running state: runs={:?}",
            snap.runs
        );
        time::sleep(Duration::from_millis(200)).await;
    }
    time::sleep(Duration::from_secs(4)).await;
    let cancel_ack = head_client2
        .cancel_run(session.session_id, None)
        .await
        .expect("live cancel request");
    assert!(
        cancel_ack.cancelled,
        "expected active live run to receive cancel signal"
    );

    let cancel_turn_res = long_task
        .await
        .expect("join live cancel task")
        .expect("cancelled turn response");
    assert_eq!(
        cancel_turn_res.run.status,
        RunStatus::Cancelled,
        "live Stop must finish as Cancelled, never Succeeded"
    );

    // Verify ZERO secrets leaked in diagnostics!
    let diag = head_client2
        .get_diagnostics(Some(100), None)
        .await
        .expect("diagnostics");
    let diag_str = serde_json::to_string(&diag).expect("serialize diag");
    assert!(
        !diag_str.contains(&mem_token),
        "DENNETT_MEMORY_TOKEN leaked in live Head diagnostics"
    );

    head2.shutdown().await;
    memoryd2.shutdown().await;
}

#[tokio::test]
#[ignore = "live agy acp 1.5.15+ isolated gemini-api-key fail-closed probe; run via dennett-check runtime-probe"]
async fn live_d1_e2e_isolated_gemini_api_key_without_key_fails_closed() {
    let (mem_pg, ctrl_pg, profile, _tmp, _guard) = require_live_e2e_env("d1-live-apikey-fail");
    let suffix = &Uuid::new_v4().simple().to_string()[..10];
    let scope = MemoryScope::try_new(format!("d1-api-{suffix}")).expect("scope");
    let mem_token = format!("tok_d1_api_{suffix}");
    let auth_cfg = AuthConfig {
        audience: String::from(DEFAULT_AUDIENCE),
        allowed_issuers: vec![String::from(DEFAULT_ISSUER)],
        hmac_secret: None,
        tokens: vec![StaticTokenEntryConfig {
            token_id: format!("api-{suffix}"),
            token: Some(mem_token.clone()),
            token_sha256: None,
            principal_id: PrincipalId::try_new("agent:d1-api").expect("principal"),
            issuer: String::from(DEFAULT_ISSUER),
            allowed_scopes: BTreeSet::from([scope.clone()]),
            capabilities: BTreeSet::from([MemoryCapability::Read, MemoryCapability::Write]),
            expires_at: None,
            revoked: false,
        }],
        revoked_token_ids: BTreeSet::new(),
    };

    let memoryd = LiveDaemon::spawn_memoryd(&mem_pg, &auth_cfg).await;
    let acp_profile_root = profile.data_dir().join("head-acp-apikey-isolated");
    let head = LiveDaemon::spawn_head_antigravity(
        &ctrl_pg,
        &memoryd.base_url,
        &mem_token,
        RuntimeAuthProfile::GeminiApiKey,
        &acp_profile_root,
    )
    .await;
    let client = HeadClient::new(&head.base_url).expect("head client");

    let project = client
        .create_project(
            &CreateProjectRequest {
                name: String::from("API Key Fail-Closed Project"),
                workspace_path: profile.workspace_dir().to_string_lossy().into_owned(),
                memory_scope: scope.clone(),
            },
            None,
        )
        .await
        .expect("project");

    let agent = client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Headless Worker"),
                description: String::from("Tests A01/A09 fail-closed API-key profile"),
                system_prompt: String::from("Background worker."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope]),
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
            },
            None,
        )
        .await
        .expect("agent");

    let session = client
        .create_session(
            &CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: project.project_id,
                title: String::from("Missing API Key Session"),
            },
            None,
        )
        .await
        .expect("session");

    // Must fail closed with AuthRequired (`-32602` / `-32000`) and NEVER fall back to OAuthPersonal!
    let res = client
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: String::from("Hello"),
                timeout_ms: Some(30_000),
            },
            None,
        )
        .await;
    assert!(
        matches!(
            res,
            Err(DennettError::AuthRequired {
                profile: RuntimeAuthProfile::GeminiApiKey,
                ..
            } | DennettError::RuntimeProtocol {
                code: Some(-32602 | -32000),
                ..
            })
        ),
        "isolated gemini-api-key without GEMINI_API_KEY must fail closed with zero OAuth fallback, got: {res:?}"
    );

    let detail = client
        .get_session_detail(session.session_id, None)
        .await
        .expect("session detail");
    assert_eq!(detail.runs.len(), 1);
    assert_eq!(detail.runs[0].status, RunStatus::Failed);

    head.shutdown().await;
    memoryd.shutdown().await;
}
