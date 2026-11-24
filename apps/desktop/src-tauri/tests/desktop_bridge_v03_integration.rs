//! Real PostgreSQL + multi-process integration tests for `dennett-desktop`'s [`HeadBridge`]
//! (V02 memory outage honesty and V03 daemon survival across desktop client teardown).
//!
//! Strictly exercises the real `dennett-memoryd` and `dennett-head` binaries without linking
//! either server crate into `dennett-desktop`.

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use dennett_contracts::{
    ArtifactCompleteness, ArtifactLifecycleAction, ArtifactLifecycleRequest,
    ArtifactLifecycleState, CreateAgentRequest, CreateArtifactRequest, CreatePolicyRuleRequest,
    CreateProjectRequest, CreateSessionRequest, CreateTaskRequest, DispatchEffectRequest,
    ExternalEffectStatus, IdempotencyKey, InboxCardKind, MemoryScope, MemoryStatus,
    PermissionResolutionDecision, PolicyEffect, ProjectLifecycleState, RebindProjectRequest,
    ReconcileEffectRequest, ResolvePermissionRequest, RevokeGrantRequest, SendMessageRequest,
    SessionKind, SimEffectBehavior, TaskStatus, ThinkingLevel, UpdateAgentConfigRequest,
    UpdateTaskRequest, WorkspaceWritePolicy,
};
use dennett_desktop_lib::{BridgeError, HeadBridge, HeadEndpointPolicy};
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::time;
use uuid::Uuid;

const TEST_PROFILE_ENV: &str = "DENNETT_TEST_PROFILE";

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
        "{TEST_PROFILE_ENV}=1 is required for desktop bridge integration tests"
    );
    let mem = env::var("DENNETT_TEST_MEMORY_PG_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .expect("DENNETT_TEST_MEMORY_PG_URL is required");
    let ctrl = env::var("DENNETT_TEST_CONTROL_PG_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .expect("DENNETT_TEST_CONTROL_PG_URL is required");
    (mem, with_isolated_control_schema(&ctrl, "desk"))
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
        "failed building `{bin_name}` binary for desktop V03 integration test"
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
    async fn spawn_memoryd(pg_url: &str, token: &str, scope: &str) -> Self {
        let bin = locate_binary("dennett-memoryd", "dennett-memoryd");
        let auth_json = json!({
            "audience": "dennett-memoryd",
            "allowed_issuers": ["dennett-standalone"],
            "tokens": [
                {
                    "token_id": "tok-desktop-v03",
                    "token": token,
                    "principal_id": "head-v03-test",
                    "issuer": "dennett-standalone",
                    "allowed_scopes": [scope],
                    "capabilities": ["read", "write"],
                    "revoked": false
                }
            ]
        })
        .to_string();

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

    fn assert_still_running(&mut self, label: &str) {
        let status = self
            .child
            .try_wait()
            .unwrap_or_else(|err| panic!("failed polling {label} child status: {err}"));
        assert!(
            status.is_none(),
            "V03 VIOLATION: {label} exited unexpectedly when desktop bridge closed: {status:?}"
        );
    }

    async fn assert_healthz_ok(&self, label: &str) {
        let req = format!(
            "GET /healthz HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            self.addr
        );
        let mut stream = time::timeout(Duration::from_secs(5), TcpStream::connect(&self.addr))
            .await
            .unwrap_or_else(|_| panic!("{label} healthz TCP connect timed out"))
            .unwrap_or_else(|err| panic!("{label} healthz TCP connect failed: {err}"));
        stream
            .write_all(req.as_bytes())
            .await
            .expect("write healthz");
        let mut resp = Vec::new();
        time::timeout(Duration::from_secs(5), stream.read_to_end(&mut resp))
            .await
            .expect("read healthz timeout")
            .expect("read healthz");
        let text = String::from_utf8_lossy(&resp);
        assert!(
            text.starts_with("HTTP/1.1 200 OK"),
            "{label} /healthz expected 200 OK, got: {text}"
        );
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

struct CreatedV03Fixtures {
    project_id: dennett_contracts::ProjectId,
    agent_id: dennett_contracts::AgentId,
    session_id: dennett_contracts::SessionId,
    artifact_id: dennett_contracts::ArtifactId,
    expected_sha: dennett_contracts::Sha256Hex,
}

#[expect(
    clippy::too_many_lines,
    reason = "creates V03 project/agent/config/session/message/artifact fixtures over live Head before dropping UI bridge"
)]
async fn run_v03_session_and_drop_bridge(
    head_base_url: &str,
    workspace_dir: &std::path::Path,
    scope: &MemoryScope,
    suffix: &str,
) -> CreatedV03Fixtures {
    let bridge =
        HeadBridge::new(head_base_url, HeadEndpointPolicy::LoopbackOnly).expect("construct bridge");

    let health_snap = bridge.get_head_health().await;
    assert!(health_snap.connected, "HeadBridge must connect to head");
    let health = health_snap.health.expect("health payload");
    assert!(health.postgres_connected);
    assert_eq!(health.memory_status, MemoryStatus::Healthy);
    assert!(
        health.runtime_is_simulated,
        "simulated head must never hide simulation flag"
    );

    let project = bridge
        .create_project(&CreateProjectRequest {
            name: format!("V03 Project {}", &suffix[..6]),
            workspace_path: Some(workspace_dir.to_string_lossy().into_owned()),
            memory_scope: Some(scope.clone()),
        })
        .await
        .expect("create_project");

    let mut allowed = BTreeSet::new();
    allowed.insert(scope.clone());
    let agent = bridge
        .create_agent(&CreateAgentRequest {
            name: format!("V03 Agent {}", &suffix[..6]),
            description: String::from("Desktop bridge V03 verification agent"),
            system_prompt: String::from("Be concise and factual."),
            model_preference: Some(String::from("sim-model-alpha")),
            thinking_level: Some(ThinkingLevel::Medium),
            allowed_memory_scopes: allowed.clone(),
            workspace_write_policy: WorkspaceWritePolicy::AskBeforeWrite,
            default_budget: None,
            attached_skills: Vec::new(),
        })
        .await
        .expect("create_agent");

    let updated_agent = bridge
        .update_agent_config(
            agent.agent_id,
            &UpdateAgentConfigRequest {
                system_prompt: String::from("Be concise and factual."),
                model_preference: Some(String::from("sim-model-beta")),
                thinking_level: Some(ThinkingLevel::High),
                allowed_memory_scopes: allowed,
                workspace_write_policy: WorkspaceWritePolicy::AskBeforeWrite,
                default_budget: None,
                attached_skills: Vec::new(),
            },
        )
        .await
        .expect("update_agent_config");
    assert_eq!(updated_agent.agent_id, agent.agent_id);
    assert_eq!(updated_agent.current_config.version_number, 2);

    let session = bridge
        .create_session(&CreateSessionRequest {
            agent_id: agent.agent_id,
            project_id: project.project_id,
            title: String::from("V03 Persistence Chat"),
            session_kind: SessionKind::UserChat,
        })
        .await
        .expect("create_session");

    let turn = bridge
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: String::from("Remember V03 desktop lifecycle invariant"),
                timeout_ms: None,
                client_message_id: None,
            },
        )
        .await
        .expect("send_message");
    assert!(turn.runtime_binding.is_simulated);
    assert!(turn.assistant_message.is_some());

    let artifact = bridge
        .create_artifact(
            session.session_id,
            &CreateArtifactRequest {
                run_id: Some(turn.run.run_id),
                name: String::from("v03-summary.md"),
                mime_type: String::from("text/markdown"),
                content: Some(String::from("# V03 Summary\nDaemons survived UI close.")),
                relative_path: None,
                completeness_status: dennett_contracts::ArtifactCompleteness::Final,
            },
        )
        .await
        .expect("create_artifact");

    let fetched = bridge
        .get_artifact(session.session_id, artifact.artifact_id)
        .await
        .expect("get_artifact");
    assert_eq!(fetched.name, "v03-summary.md");
    assert_eq!(fetched.revision, 1);

    CreatedV03Fixtures {
        project_id: project.project_id,
        agent_id: agent.agent_id,
        session_id: session.session_id,
        artifact_id: artifact.artifact_id,
        expected_sha: artifact.sha256,
    }
}

#[tokio::test]
async fn v03_closing_desktop_bridge_keeps_daemons_alive_and_restores_conversation_and_artifact() {
    let (mem_pg, ctrl_pg) = require_pg_urls();
    let suffix = Uuid::new_v4().simple().to_string();
    let scope_str = format!("project:desktop-v03-{}", &suffix[..8]);
    let scope = MemoryScope::try_new(&scope_str).expect("valid scope");
    let token = format!("tok-v03-{suffix}");

    let workspace_dir = env::temp_dir().join(format!("dennett-v03-ws-{}", &suffix[..8]));
    fs::create_dir_all(&workspace_dir).expect("create temp workspace");

    let mut memoryd = RunningDaemon::spawn_memoryd(&mem_pg, &token, &scope_str).await;
    let mut head = RunningDaemon::spawn_head_simulated(&ctrl_pg, &memoryd.base_url, &token).await;

    let created =
        run_v03_session_and_drop_bridge(&head.base_url, &workspace_dir, &scope, &suffix).await;

    // V03 invariant: BOTH daemons MUST remain alive and healthy after desktop client closes
    head.assert_still_running("dennett-head");
    memoryd.assert_still_running("dennett-memoryd");
    head.assert_healthz_ok("dennett-head").await;
    memoryd.assert_healthz_ok("dennett-memoryd").await;

    // Reopen a completely fresh HeadBridge and verify full state restoration
    let reopened = HeadBridge::new(&head.base_url, HeadEndpointPolicy::LoopbackOnly)
        .expect("reopen HeadBridge");
    let projects = reopened.list_projects().await.expect("list_projects");
    assert!(
        projects
            .projects
            .iter()
            .any(|p| p.project_id == created.project_id)
    );

    let agents = reopened.list_agents().await.expect("list_agents");
    let restored_agent = agents
        .agents
        .iter()
        .find(|a| a.agent_id == created.agent_id)
        .expect("agent restored");
    assert_eq!(restored_agent.current_config.version_number, 2);
    assert_eq!(
        restored_agent.current_config.model_preference.as_deref(),
        Some("sim-model-beta")
    );

    let detail = reopened
        .get_session_detail(created.session_id)
        .await
        .expect("get_session_detail");
    assert_eq!(detail.session.session_id, created.session_id);
    assert!(
        detail.messages.iter().any(|m| m
            .content
            .contains("Remember V03 desktop lifecycle invariant")),
        "user message must be restored after reopening desktop bridge"
    );
    assert!(
        detail.messages.len() >= 2,
        "assistant reply must be restored after reopening desktop bridge"
    );

    let restored_art = reopened
        .get_artifact(created.session_id, created.artifact_id)
        .await
        .expect("restored artifact");
    assert_eq!(restored_art.sha256, created.expected_sha);
    assert_eq!(
        restored_art.content.as_deref(),
        Some("# V03 Summary\nDaemons survived UI close.")
    );

    let diag = reopened
        .list_diagnostics(Some(10))
        .await
        .expect("list_diagnostics");
    assert_ne!(diag.events.len(), 0);

    head.shutdown().await;
    memoryd.shutdown().await;
    let _ = fs::remove_dir_all(&workspace_dir);
}

#[tokio::test]
async fn v02_desktop_bridge_reports_memoryd_outage_honestly_and_keeps_cancel_unblocked() {
    let (mem_pg, ctrl_pg) = require_pg_urls();
    let suffix = Uuid::new_v4().simple().to_string();
    let scope_str = format!("project:desktop-v02-{}", &suffix[..8]);
    let scope = MemoryScope::try_new(&scope_str).expect("valid scope");
    let token = format!("tok-v02-{suffix}");

    let workspace_dir = env::temp_dir().join(format!("dennett-v02-ws-{}", &suffix[..8]));
    fs::create_dir_all(&workspace_dir).expect("create temp workspace");

    let memoryd = RunningDaemon::spawn_memoryd(&mem_pg, &token, &scope_str).await;
    let head = RunningDaemon::spawn_head_simulated(&ctrl_pg, &memoryd.base_url, &token).await;

    let bridge = HeadBridge::new(&head.base_url, HeadEndpointPolicy::LoopbackOnly)
        .expect("construct HeadBridge");

    let project = bridge
        .create_project(&CreateProjectRequest {
            name: format!("V02 Project {}", &suffix[..6]),
            workspace_path: Some(workspace_dir.to_string_lossy().into_owned()),
            memory_scope: Some(scope.clone()),
        })
        .await
        .expect("create_project");

    let mut allowed = BTreeSet::new();
    allowed.insert(scope);
    let agent = bridge
        .create_agent(&CreateAgentRequest {
            name: format!("V02 Agent {}", &suffix[..6]),
            description: String::from("V02 outage test agent"),
            system_prompt: String::from("Handle memoryd outages honestly."),
            model_preference: None,
            thinking_level: Some(ThinkingLevel::Low),
            allowed_memory_scopes: allowed,
            workspace_write_policy: WorkspaceWritePolicy::DenyAll,
            default_budget: None,
            attached_skills: Vec::new(),
        })
        .await
        .expect("create_agent");

    let session = bridge
        .create_session(&CreateSessionRequest {
            agent_id: agent.agent_id,
            project_id: project.project_id,
            title: String::from("V02 Outage Session"),
            session_kind: SessionKind::UserChat,
        })
        .await
        .expect("create_session");

    // Kill standalone memoryd while Head stays running
    memoryd.kill_hard().await;

    let degraded_snap = bridge.get_head_health().await;
    assert!(
        degraded_snap.connected,
        "Head remains reachable when memoryd dies"
    );
    let degraded_health = degraded_snap.health.expect("health payload");
    assert!(degraded_health.postgres_connected);
    assert_eq!(
        degraded_health.memory_status,
        MemoryStatus::DegradedUnreachable,
        "V02 VIOLATION: desktop bridge must never report healthy memory when memoryd is dead"
    );

    // Conversation still persists and completes honestly during memoryd outage
    let turn = bridge
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: String::from("Continue safely during memory outage"),
                timeout_ms: None,
                client_message_id: None,
            },
        )
        .await
        .expect("send_message during memoryd outage");
    assert_eq!(turn.memory_status, MemoryStatus::DegradedUnreachable);

    // Stop/cancel remains completely unblocked during memoryd outage
    let cancel = bridge
        .cancel_run(session.session_id)
        .await
        .expect("cancel_run during memoryd outage");
    assert_eq!(cancel.memory_status, MemoryStatus::DegradedUnreachable);

    // Manual outbox flush reports degraded honestly instead of hanging or crashing
    let flush = bridge
        .flush_memory_outbox()
        .await
        .expect("flush_memory_outbox during outage");
    assert_eq!(flush.memory_status, MemoryStatus::DegradedUnreachable);

    head.shutdown().await;
    let _ = fs::remove_dir_all(&workspace_dir);
}

async fn wait_for_task_terminal(
    bridge: &HeadBridge,
    task_id: dennett_contracts::TaskId,
    timeout: Duration,
) -> dennett_contracts::TaskRecord {
    let deadline = time::Instant::now() + timeout;
    loop {
        let detail = bridge
            .get_task_detail(task_id)
            .await
            .expect("get_task_detail");
        if detail.task.status.is_terminal() {
            return detail.task;
        }
        assert!(
            time::Instant::now() < deadline,
            "timed out waiting for task {task_id} to reach terminal state (last={:?})",
            detail.task.status
        );
        time::sleep(Duration::from_millis(40)).await;
    }
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "verifies full interactive ask_before_write approval lifecycle, grant creation/revocation, and Stop cancellation through HeadBridge"
)]
async fn d2_desktop_bridge_interactive_ask_before_write_approval_and_grant_revocation() {
    let (mem_pg, ctrl_pg) = require_pg_urls();
    let suffix = Uuid::new_v4().simple().to_string();
    let scope_str = format!("project:desktop-d2-apr-{}", &suffix[..8]);
    let scope = MemoryScope::try_new(&scope_str).expect("valid scope");
    let token = format!("tok-d2-apr-{suffix}");

    let workspace_dir = env::temp_dir().join(format!("dennett-d2-apr-ws-{}", &suffix[..8]));
    fs::create_dir_all(&workspace_dir).expect("create temp workspace");

    let memoryd = RunningDaemon::spawn_memoryd(&mem_pg, &token, &scope_str).await;
    let head = RunningDaemon::spawn_head_simulated(&ctrl_pg, &memoryd.base_url, &token).await;

    let bridge = HeadBridge::new(&head.base_url, HeadEndpointPolicy::LoopbackOnly)
        .expect("construct HeadBridge");

    let snap_before = bridge
        .get_state_snapshot(None, true)
        .await
        .expect("initial state snapshot");

    let project = bridge
        .create_project(&CreateProjectRequest {
            name: format!("D2 Approval Proj {}", &suffix[..6]),
            workspace_path: Some(workspace_dir.to_string_lossy().into_owned()),
            memory_scope: Some(scope.clone()),
        })
        .await
        .expect("create_project");

    let mut allowed = BTreeSet::new();
    allowed.insert(scope);
    let agent = bridge
        .create_agent(&CreateAgentRequest {
            name: format!("D2 Approval Agent {}", &suffix[..6]),
            description: String::from("Interactive ask_before_write desktop test agent"),
            system_prompt: String::from("Ask before writing."),
            model_preference: Some(String::from("sim-model-alpha")),
            thinking_level: Some(ThinkingLevel::Medium),
            allowed_memory_scopes: allowed,
            workspace_write_policy: WorkspaceWritePolicy::AskBeforeWrite,
            default_budget: None,
            attached_skills: Vec::new(),
        })
        .await
        .expect("create_agent");

    let session = bridge
        .create_session(&CreateSessionRequest {
            agent_id: agent.agent_id,
            project_id: project.project_id,
            title: String::from("D2 Interactive Approval Chat"),
            session_kind: SessionKind::UserChat,
        })
        .await
        .expect("create_session");

    // Start turn that triggers interactive ask_before_write permission request concurrently.
    let send_bridge = bridge.clone();
    let sid = session.session_id;
    let turn_handle = tokio::spawn(async move {
        send_bridge
            .send_message(
                sid,
                &SendMessageRequest {
                    content: String::from("[SIM:TOOL:write_file:approved.txt|approved via bridge]"),
                    timeout_ms: Some(15_000),
                    client_message_id: None,
                },
            )
            .await
    });

    // Poll pending permission request through cloned HeadBridge (proving zero lock contention).
    let deadline = time::Instant::now() + Duration::from_secs(10);
    let pending_req = loop {
        let list = bridge
            .list_permission_requests(Some("pending"), Some(session.session_id))
            .await
            .expect("list_permission_requests");
        if let Some(req) = list.requests.into_iter().next() {
            break req;
        }
        assert!(
            time::Instant::now() < deadline,
            "timed out waiting for pending permission request"
        );
        time::sleep(Duration::from_millis(30)).await;
    };

    // File MUST NOT exist on disk before owner approval!
    let approved_path = workspace_dir.join("approved.txt");
    assert!(
        !approved_path.exists(),
        "file must not be written before interactive approval"
    );

    let inbox = bridge
        .list_inbox_cards(Some("open"), Some(project.project_id))
        .await
        .expect("list_inbox_cards");
    assert!(
        inbox
            .cards
            .iter()
            .any(|c| c.kind == InboxCardKind::PermissionApproval
                && c.permission_request_id == Some(pending_req.request_id)),
        "Action Inbox must expose open permission_approval card"
    );

    // Approve for project via HeadBridge -> issues revocable grant + unblocks write_file once.
    let resolved = bridge
        .resolve_permission_request(
            pending_req.request_id,
            &ResolvePermissionRequest {
                expected_revision: Some(pending_req.revision),
                decision: PermissionResolutionDecision::ApproveProject,
                reason: Some(String::from("Owner approved project write via desktop UI")),
                max_uses: None,
                expires_in_secs: None,
            },
        )
        .await
        .expect("resolve_permission_request");
    let grant = resolved
        .issued_grant
        .expect("ApproveProject must issue a revocable CapabilityGrant");

    let turn_resp = turn_handle
        .await
        .expect("turn task join")
        .expect("send_message should succeed after approval");
    assert!(turn_resp.assistant_message.is_some());
    assert_eq!(
        fs::read_to_string(&approved_path).expect("approved file exists"),
        "approved via bridge"
    );

    // Events stream observed the permission.requested / permission.resolved events cleanly.
    let feed = bridge
        .get_events(
            snap_before.snapshot_cursor.committed_seq,
            Some(snap_before.snapshot_cursor.authority_epoch),
            Some(project.project_id),
            Some(100),
        )
        .await
        .expect("get_events");
    assert!(
        feed.events
            .iter()
            .any(|ev| ev.event_type == "permission.requested"),
        "event journal must contain permission.requested"
    );
    assert!(
        feed.events
            .iter()
            .any(|ev| ev.event_type == "permission.resolved"),
        "event journal must contain permission.resolved"
    );

    // Verify grant is visible and immediately revocable through HeadBridge.
    let grants = bridge
        .list_capability_grants(Some(project.project_id), false)
        .await
        .expect("list_capability_grants");
    assert!(grants.grants.iter().any(|g| g.grant_id == grant.grant_id));

    let revoked = bridge
        .revoke_capability_grant(
            grant.grant_id,
            &RevokeGrantRequest {
                reason: Some(String::from("revoked in desktop settings")),
            },
        )
        .await
        .expect("revoke_capability_grant");
    assert!(revoked.revoked_at.is_some());

    // After revocation, a second write MUST require approval again; cancelling via Stop unblocks cleanly.
    let second_bridge = bridge.clone();
    let second_handle = tokio::spawn(async move {
        second_bridge
            .send_message(
                sid,
                &SendMessageRequest {
                    content: String::from("[SIM:TOOL:write_file:after-revoke.txt|blocked]"),
                    timeout_ms: Some(15_000),
                    client_message_id: None,
                },
            )
            .await
    });

    let deadline2 = time::Instant::now() + Duration::from_secs(10);
    loop {
        let list = bridge
            .list_permission_requests(Some("pending"), Some(session.session_id))
            .await
            .expect("list_permission_requests after revoke");
        if !list.requests.is_empty() {
            break;
        }
        assert!(
            time::Instant::now() < deadline2,
            "revoked grant must force a fresh pending permission request"
        );
        time::sleep(Duration::from_millis(30)).await;
    }

    let cancel_resp = bridge
        .cancel_run(session.session_id)
        .await
        .expect("cancel_run during pending approval");
    assert!(cancel_resp.cancelled);
    let _ = second_handle.await.expect("second turn join");
    assert!(
        !workspace_dir.join("after-revoke.txt").exists(),
        "cancelled pending approval must never write file"
    );

    head.shutdown().await;
    memoryd.shutdown().await;
    let _ = fs::remove_dir_all(&workspace_dir);
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "verifies targeted per-task Stop leaving sibling alive plus inline goal update generation bump over real PG16"
)]
async fn d2_desktop_bridge_targeted_task_stop_leaves_sibling_alive_and_updates_goal_generation() {
    let (mem_pg, ctrl_pg) = require_pg_urls();
    let suffix = Uuid::new_v4().simple().to_string();
    let scope_str = format!("project:desktop-d2-tsk-{}", &suffix[..8]);
    let scope = MemoryScope::try_new(&scope_str).expect("valid scope");
    let token = format!("tok-d2-tsk-{suffix}");

    let workspace_dir = env::temp_dir().join(format!("dennett-d2-tsk-ws-{}", &suffix[..8]));
    fs::create_dir_all(&workspace_dir).expect("create temp workspace");

    let memoryd = RunningDaemon::spawn_memoryd(&mem_pg, &token, &scope_str).await;
    let head = RunningDaemon::spawn_head_simulated(&ctrl_pg, &memoryd.base_url, &token).await;

    let bridge = HeadBridge::new(&head.base_url, HeadEndpointPolicy::LoopbackOnly)
        .expect("construct HeadBridge");

    let project = bridge
        .create_project(&CreateProjectRequest {
            name: format!("D2 Tasks Proj {}", &suffix[..6]),
            workspace_path: Some(workspace_dir.to_string_lossy().into_owned()),
            memory_scope: Some(scope.clone()),
        })
        .await
        .expect("create_project");

    let mut allowed = BTreeSet::new();
    allowed.insert(scope);
    let agent = bridge
        .create_agent(&CreateAgentRequest {
            name: format!("D2 Tasks Agent {}", &suffix[..6]),
            description: String::from("Durable task desktop bridge test agent"),
            system_prompt: String::from("Execute tasks accurately."),
            model_preference: Some(String::from("sim-model-alpha")),
            thinking_level: Some(ThinkingLevel::Low),
            allowed_memory_scopes: allowed,
            workspace_write_policy: WorkspaceWritePolicy::DenyAll,
            default_budget: None,
            attached_skills: Vec::new(),
        })
        .await
        .expect("create_agent");

    let task_a = bridge
        .create_task(&CreateTaskRequest {
            project_id: project.project_id,
            session_id: None,
            agent_id: agent.agent_id,
            parent_task_id: None,
            title: Some(String::from("Slow Task A")),
            goal: String::from("[SIM:SLOW:6000] Long running Task A"),
            schedule: None,
            wait_for_signal: None,
            budget: None,
            attached_skills: Vec::new(),
            auth_profile_override: None,
            execute_immediately: Some(true),
        })
        .await
        .expect("create task A");

    let task_b = bridge
        .create_task(&CreateTaskRequest {
            project_id: project.project_id,
            session_id: None,
            agent_id: agent.agent_id,
            parent_task_id: None,
            title: Some(String::from("Sibling Task B")),
            goal: String::from("[SIM:SLOW:350] Sibling Task B must survive"),
            schedule: None,
            wait_for_signal: None,
            budget: None,
            attached_skills: Vec::new(),
            auth_profile_override: None,
            execute_immediately: Some(true),
        })
        .await
        .expect("create task B");

    // Wait briefly so Task A is running, then issue targeted Stop strictly on Task A.
    time::sleep(Duration::from_millis(80)).await;
    let stop_a = bridge
        .cancel_task(task_a.task_id)
        .await
        .expect("cancel_task A");
    assert_eq!(stop_a.task.status, TaskStatus::Cancelled);
    assert!(!stop_a.external_outcome_unknown);

    // Sibling Task B MUST complete normally without being disturbed by Task A's Stop!
    let final_b = wait_for_task_terminal(&bridge, task_b.task_id, Duration::from_secs(10)).await;
    assert_eq!(
        final_b.status,
        TaskStatus::Succeeded,
        "cancelling Task A must never kill sibling Task B"
    );

    // Inline goal update via HeadBridge bumps `generation` cleanly (V07).
    let task_c = bridge
        .create_task(&CreateTaskRequest {
            project_id: project.project_id,
            session_id: None,
            agent_id: agent.agent_id,
            parent_task_id: None,
            title: Some(String::from("Signal Task C")),
            goal: String::from("Original goal v1"),
            schedule: None,
            wait_for_signal: Some(String::from("go")),
            budget: None,
            attached_skills: Vec::new(),
            auth_profile_override: None,
            execute_immediately: None,
        })
        .await
        .expect("create waiting_signal task C");
    assert_eq!(task_c.generation, 1);
    assert_eq!(task_c.status, TaskStatus::WaitingSignal);

    let updated_c = bridge
        .update_task(
            task_c.task_id,
            &UpdateTaskRequest {
                title: None,
                goal: Some(String::from("Refined goal v2")),
                schedule: None,
                clear_schedule: false,
                budget: None,
                execute_immediately: Some(false),
            },
        )
        .await
        .expect("update_task C");
    assert_eq!(updated_c.generation, 2);
    assert_eq!(updated_c.goal, "Refined goal v2");

    head.shutdown().await;
    memoryd.shutdown().await;
    let _ = fs::remove_dir_all(&workspace_dir);
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "verifies LostAck -> unknown_external inbox visibility, honest Stop, and reconcile without blind retries over real PG16"
)]
async fn d2_desktop_bridge_lost_ack_unknown_external_visibility_honest_stop_and_reconcile() {
    let (mem_pg, ctrl_pg) = require_pg_urls();
    let suffix = Uuid::new_v4().simple().to_string();
    let scope_str = format!("project:desktop-d2-unk-{}", &suffix[..8]);
    let scope = MemoryScope::try_new(&scope_str).expect("valid scope");
    let token = format!("tok-d2-unk-{suffix}");

    let workspace_dir = env::temp_dir().join(format!("dennett-d2-unk-ws-{}", &suffix[..8]));
    fs::create_dir_all(&workspace_dir).expect("create temp workspace");

    let memoryd = RunningDaemon::spawn_memoryd(&mem_pg, &token, &scope_str).await;
    let head = RunningDaemon::spawn_head_simulated(&ctrl_pg, &memoryd.base_url, &token).await;

    let bridge = HeadBridge::new(&head.base_url, HeadEndpointPolicy::LoopbackOnly)
        .expect("construct HeadBridge");

    let project = bridge
        .create_project(&CreateProjectRequest {
            name: format!("D2 Unknown Proj {}", &suffix[..6]),
            workspace_path: Some(workspace_dir.to_string_lossy().into_owned()),
            memory_scope: Some(scope.clone()),
        })
        .await
        .expect("create_project");

    let mut allowed = BTreeSet::new();
    allowed.insert(scope);
    let agent = bridge
        .create_agent(&CreateAgentRequest {
            name: format!("D2 Unknown Agent {}", &suffix[..6]),
            description: String::from("LostAck UNKNOWN reconciliation desktop test agent"),
            system_prompt: String::from("Track external effects safely."),
            model_preference: None,
            thinking_level: Some(ThinkingLevel::Low),
            allowed_memory_scopes: allowed,
            workspace_write_policy: WorkspaceWritePolicy::DenyAll,
            default_budget: None,
            attached_skills: Vec::new(),
        })
        .await
        .expect("create_agent");

    let task = bridge
        .create_task(&CreateTaskRequest {
            project_id: project.project_id,
            session_id: None,
            agent_id: agent.agent_id,
            parent_task_id: None,
            title: Some(String::from("External Webhook Task")),
            goal: String::from("Dispatch external notification"),
            schedule: None,
            wait_for_signal: Some(String::from("ready")),
            budget: None,
            attached_skills: Vec::new(),
            auth_profile_override: None,
            execute_immediately: None,
        })
        .await
        .expect("create task");

    let idem_key = IdempotencyKey::try_new(format!("desktop-lost-ack-{}", &suffix[..8]))
        .expect("valid idempotency key");
    let dispatch_req = DispatchEffectRequest {
        project_id: project.project_id,
        session_id: task.session_id,
        task_id: Some(task.task_id),
        run_id: None,
        agent_id: agent.agent_id,
        effect_idempotency_key: idem_key,
        connector_id: Some(String::from("simulated_external")),
        operation: String::from("send_webhook"),
        target_recipient: String::from("ops@example.test"),
        parameters_json: String::from("{\"msg\":\"deploy-ready\"}"),
        simulated_behavior: Some(SimEffectBehavior::LostAck),
    };
    // D2-ARCH-03: External effects fail closed until explicitly authorized by policy.
    let denied_before_rule = bridge.dispatch_effect(&dispatch_req).await;
    assert!(
        matches!(
            denied_before_rule,
            Err(BridgeError::HeadApi { ref code, .. }) if code == "ACCESS_DENIED"
        ),
        "expected default-deny before Allow policy rule, got: {denied_before_rule:?}"
    );
    let head_client =
        dennett_head_client::HeadClient::new(&head.base_url).expect("head client for policy");
    head_client
        .create_policy_rule(
            &CreatePolicyRuleRequest {
                scope: format!("project:{}", project.project_id),
                tool_name: String::from("external_effect:send_webhook"),
                target_pattern: Some(String::from("ops@example.test")),
                effect: PolicyEffect::Allow,
                reason: String::from("Allow webhook dispatch for desktop LostAck test"),
            },
            None,
        )
        .await
        .expect("create allow policy rule");

    let dispatched = bridge
        .dispatch_effect(&dispatch_req)
        .await
        .expect("dispatch_effect LostAck");
    assert_eq!(dispatched.effect.status, ExternalEffectStatus::Unknown);
    assert!(dispatched.external_outcome_unknown);

    // Task transitions to unknown_external and Action Inbox exposes an UnknownEffect card.
    let detail = bridge
        .get_task_detail(task.task_id)
        .await
        .expect("get_task_detail");
    assert_eq!(detail.task.status, TaskStatus::UnknownExternal);
    assert!(
        detail
            .open_inbox_cards
            .iter()
            .any(|c| c.kind == InboxCardKind::UnknownEffect
                && c.effect_id == Some(dispatched.effect.effect_id)),
        "open Action Inbox must show UnknownEffect card"
    );

    // Honest Stop reports external_outcome_unknown = true instead of pretending the webhook was undone.
    let stop_resp = bridge
        .cancel_task(task.task_id)
        .await
        .expect("cancel_task with unknown external effect");
    assert!(stop_resp.external_outcome_unknown);
    assert_eq!(stop_resp.task.status, TaskStatus::UnknownExternal);
    assert_eq!(stop_resp.unknown_effects.len(), 1);

    // Reconcile via HeadBridge resolves to Confirmed with provider_execution_count == 1 (ZERO blind retries).
    let reconciled = bridge
        .reconcile_effect(
            dispatched.effect.effect_id,
            &ReconcileEffectRequest {
                manual_resolution: None,
                reason: Some(String::from("reconciled from desktop inbox")),
            },
        )
        .await
        .expect("reconcile_effect");
    assert_eq!(reconciled.effect.status, ExternalEffectStatus::Confirmed);
    assert!(reconciled.provider_found);
    assert_eq!(reconciled.provider_execution_count, 1);

    let listed = bridge
        .list_effects(Some(task.task_id), None, None)
        .await
        .expect("list_effects");
    assert_eq!(listed.effects.len(), 1);
    assert_eq!(listed.effects[0].status, ExternalEffectStatus::Confirmed);

    head.shutdown().await;
    memoryd.shutdown().await;
    let _ = fs::remove_dir_all(&workspace_dir);
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "verifies client_message_id dedup, full Contract C project lifecycle without touching disk files, and Contract D artifact revisions"
)]
async fn d2_desktop_bridge_client_message_id_dedup_project_lifecycle_and_artifact_revisions() {
    let (mem_pg, ctrl_pg) = require_pg_urls();
    let suffix = Uuid::new_v4().simple().to_string();
    let scope_str = format!("project:desktop-d2-lc-{}", &suffix[..8]);
    let scope = MemoryScope::try_new(&scope_str).expect("valid scope");
    let token = format!("tok-d2-lc-{suffix}");

    let ws_a = env::temp_dir().join(format!("dennett-d2-lca-{}", &suffix[..8]));
    let ws_b = env::temp_dir().join(format!("dennett-d2-lcb-{}", &suffix[..8]));
    fs::create_dir_all(&ws_a).expect("create ws_a");
    fs::create_dir_all(&ws_b).expect("create ws_b");
    let sentinel_a = ws_a.join("user-precious.txt");
    let sentinel_b = ws_b.join("user-rebound.txt");
    fs::write(&sentinel_a, "do not delete A").expect("write sentinel A");
    fs::write(&sentinel_b, "do not delete B").expect("write sentinel B");

    let memoryd = RunningDaemon::spawn_memoryd(&mem_pg, &token, &scope_str).await;
    let head = RunningDaemon::spawn_head_simulated(&ctrl_pg, &memoryd.base_url, &token).await;

    let bridge = HeadBridge::new(&head.base_url, HeadEndpointPolicy::LoopbackOnly)
        .expect("construct HeadBridge");

    let project = bridge
        .create_project(&CreateProjectRequest {
            name: format!("D2 Lifecycle Proj {}", &suffix[..6]),
            workspace_path: Some(ws_a.to_string_lossy().into_owned()),
            memory_scope: Some(scope.clone()),
        })
        .await
        .expect("create_project");

    let mut allowed = BTreeSet::new();
    allowed.insert(scope);
    let agent = bridge
        .create_agent(&CreateAgentRequest {
            name: format!("D2 Lifecycle Agent {}", &suffix[..6]),
            description: String::from("Dedup & lifecycle test agent"),
            system_prompt: String::from("Be concise."),
            model_preference: None,
            thinking_level: Some(ThinkingLevel::Low),
            allowed_memory_scopes: allowed,
            workspace_write_policy: WorkspaceWritePolicy::DenyAll,
            default_budget: None,
            attached_skills: Vec::new(),
        })
        .await
        .expect("create_agent");

    let session = bridge
        .create_session(&CreateSessionRequest {
            agent_id: agent.agent_id,
            project_id: project.project_id,
            title: String::from("D2 Dedup Session"),
            session_kind: SessionKind::UserChat,
        })
        .await
        .expect("create_session");

    // 1. Offline queue `client_message_id` deduplication guarantees exactly one execution.
    let client_msg_id = format!("offline-queue-{}", &suffix[..8]);
    let first = bridge
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: String::from("Send exactly once across reconnect"),
                timeout_ms: None,
                client_message_id: Some(client_msg_id.clone()),
            },
        )
        .await
        .expect("first send_message");
    assert!(!first.deduplicated);

    let replay = bridge
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: String::from("Send exactly once across reconnect"),
                timeout_ms: None,
                client_message_id: Some(client_msg_id),
            },
        )
        .await
        .expect("replayed send_message");
    assert!(replay.deduplicated);
    assert_eq!(replay.run.run_id, first.run.run_id);
    assert_eq!(
        replay.user_message.message_id,
        first.user_message.message_id
    );

    // 2. Artifact revisions v1 (partial) -> v2 (final) and Contract D lifecycle actions.
    let v1 = bridge
        .create_artifact(
            session.session_id,
            &CreateArtifactRequest {
                run_id: Some(first.run.run_id),
                name: String::from("draft-spec.md"),
                mime_type: String::from("text/markdown"),
                content: Some(String::from("# Draft v1")),
                relative_path: None,
                completeness_status: ArtifactCompleteness::Partial,
            },
        )
        .await
        .expect("create v1");
    assert_eq!(v1.revision, 1);
    assert_eq!(v1.completeness_status, ArtifactCompleteness::Partial);

    let v2 = bridge
        .create_artifact(
            session.session_id,
            &CreateArtifactRequest {
                run_id: Some(first.run.run_id),
                name: String::from("draft-spec.md"),
                mime_type: String::from("text/markdown"),
                content: Some(String::from("# Final v2")),
                relative_path: None,
                completeness_status: ArtifactCompleteness::Final,
            },
        )
        .await
        .expect("create v2");
    assert_eq!(v2.revision, 2);

    let revs = bridge
        .list_artifact_revisions(session.session_id, "draft-spec.md")
        .await
        .expect("list_artifact_revisions");
    assert_eq!(revs.artifacts.len(), 2);
    assert_eq!(revs.artifacts[0].revision, 1);
    assert_eq!(revs.artifacts[1].revision, 2);

    let finalized = bridge
        .apply_artifact_lifecycle(
            session.session_id,
            v1.artifact_id,
            &ArtifactLifecycleRequest {
                action: ArtifactLifecycleAction::Finalize,
                reason: Some(String::from("promoted v1")),
            },
        )
        .await
        .expect("finalize v1");
    assert_eq!(
        finalized.artifact.completeness_status,
        ArtifactCompleteness::Final
    );
    assert!(!finalized.user_workspace_files_deleted);

    let archived_art = bridge
        .apply_artifact_lifecycle(
            session.session_id,
            v1.artifact_id,
            &ArtifactLifecycleRequest {
                action: ArtifactLifecycleAction::Archive,
                reason: None,
            },
        )
        .await
        .expect("archive artifact");
    assert_eq!(
        archived_art.artifact.lifecycle_state,
        ArtifactLifecycleState::Archived
    );

    // 3. Contract C project lifecycle: archive -> unarchive -> rebind -> detach -> delete(confirm=true)
    //    NEVER deletes user files on disk!
    let archived = bridge
        .archive_project(project.project_id)
        .await
        .expect("archive_project");
    assert_eq!(
        archived.project.lifecycle_state,
        ProjectLifecycleState::Archived
    );
    assert!(!archived.user_workspace_files_deleted);

    let with_archived = bridge
        .list_projects_with_options(true)
        .await
        .expect("list_projects_with_options(true)");
    assert!(
        with_archived
            .projects
            .iter()
            .any(|p| p.project_id == project.project_id
                && p.lifecycle_state == ProjectLifecycleState::Archived)
    );

    let unarchived = bridge
        .unarchive_project(project.project_id)
        .await
        .expect("unarchive_project");
    assert_eq!(
        unarchived.project.lifecycle_state,
        ProjectLifecycleState::Active
    );

    let rebound = bridge
        .rebind_project(
            project.project_id,
            &RebindProjectRequest {
                workspace_path: ws_b.to_string_lossy().into_owned(),
                reason: Some(String::from("moved folder")),
            },
        )
        .await
        .expect("rebind_project");
    assert!(!rebound.user_workspace_files_deleted);
    assert!(rebound.project.workspace_path.is_some());

    let detached = bridge
        .detach_project_workspace(project.project_id)
        .await
        .expect("detach_project_workspace");
    assert_eq!(
        detached.project.lifecycle_state,
        ProjectLifecycleState::Detached
    );
    assert!(detached.project.workspace_path.is_none());
    assert!(!detached.user_workspace_files_deleted);

    let deleted = bridge
        .delete_project(project.project_id, true)
        .await
        .expect("delete_project(confirm=true)");
    assert_eq!(
        deleted.project.lifecycle_state,
        ProjectLifecycleState::DeletedRecord
    );
    assert!(!deleted.user_workspace_files_deleted);

    // User files on disk remain 100% untouched!
    assert_eq!(
        fs::read_to_string(&sentinel_a).expect("sentinel A intact"),
        "do not delete A"
    );
    assert_eq!(
        fs::read_to_string(&sentinel_b).expect("sentinel B intact"),
        "do not delete B"
    );

    head.shutdown().await;
    memoryd.shutdown().await;
    let _ = fs::remove_dir_all(&ws_a);
    let _ = fs::remove_dir_all(&ws_b);
}
