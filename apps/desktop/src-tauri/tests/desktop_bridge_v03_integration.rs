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
    CreateAgentRequest, CreateArtifactRequest, CreateProjectRequest, CreateSessionRequest,
    MemoryScope, MemoryStatus, SendMessageRequest, ThinkingLevel, UpdateAgentConfigRequest,
    WorkspaceWritePolicy,
};
use dennett_desktop_lib::{HeadBridge, HeadEndpointPolicy};
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::time;
use uuid::Uuid;

const TEST_PROFILE_ENV: &str = "DENNETT_TEST_PROFILE";

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
    (mem, ctrl)
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
            workspace_path: workspace_dir.to_string_lossy().into_owned(),
            memory_scope: scope.clone(),
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
            model_preference: Some(String::from("gemini-2.5-pro")),
            thinking_level: Some(ThinkingLevel::Medium),
            allowed_memory_scopes: allowed.clone(),
            workspace_write_policy: WorkspaceWritePolicy::AskBeforeWrite,
        })
        .await
        .expect("create_agent");

    let updated_agent = bridge
        .update_agent_config(
            agent.agent_id,
            &UpdateAgentConfigRequest {
                system_prompt: String::from("Be concise and factual."),
                model_preference: Some(String::from("gemini-3.8-pro")),
                thinking_level: Some(ThinkingLevel::High),
                allowed_memory_scopes: allowed,
                workspace_write_policy: WorkspaceWritePolicy::AskBeforeWrite,
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
        })
        .await
        .expect("create_session");

    let turn = bridge
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: String::from("Remember V03 desktop lifecycle invariant"),
                timeout_ms: None,
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
        Some("gemini-3.8-pro")
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
            workspace_path: workspace_dir.to_string_lossy().into_owned(),
            memory_scope: scope.clone(),
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
        })
        .await
        .expect("create_agent");

    let session = bridge
        .create_session(&CreateSessionRequest {
            agent_id: agent.agent_id,
            project_id: project.project_id,
            title: String::from("V02 Outage Session"),
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
