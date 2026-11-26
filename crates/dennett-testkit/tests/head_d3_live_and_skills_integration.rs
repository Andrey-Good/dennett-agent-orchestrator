//! End-to-end real PostgreSQL 16 + multi-process `dennett-head` + `dennett-control-mcp` +
//! `HeadClient` + `dennettctl` integration tests for D3+ Live Agent Polish & Provider-Independent
//! Session Skills (`41 §7.10`):
//! - Live mid-turn `streaming_text` polling (`GET /v1/sessions/{id}/live`) with clean `\n\n`
//!   paragraph separation after silent `ToolObserved` (ZERO tool-call feed in `LiveRunSnapshot`;
//!   server-side `tool_observed` stays in `audit_events`) + `context_used_tokens` /
//!   `context_window_tokens` persisted on `runs`;
//! - Durable `session_followups` queue (enqueue, edit, delete, clear, idempotency dedup/mismatch,
//!   idle auto-dispatch, and V03-proof server-side FIFO auto-dispatch on `Succeeded`);
//! - Safe `paused` transition on explicit `Stop` and `Failed` runs + `POST /followups/resume`
//!   auto-start returning `dispatched: Some(SendMessageResponse)`;
//! - Live `POST /v1/sessions/{id}/steer` mid-run with ZERO `409 Conflict` and preservation of
//!   queued follow-ups;
//! - Provider-independent session skills (`dennett_skill_list` & `dennett_skill_read` via real
//!   `dennett-control-mcp` stdio process + `[Dennett Active Skills Index]` prompt injection) across
//!   both project sessions (project-over-global override, disabled/blocked/foreign-project denied)
//!   and standalone chats (global-only skills, project events/delegation still strictly denied);
//! - `dennettctl` CLI parity (`sessions live`, `sessions followup add|list|update|delete|clear|resume`,
//!   `sessions steer`).

use std::collections::BTreeSet;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use dennett_contracts::{
    CapabilityActivation, CapabilityOrigin, CapabilityOwnership, CapabilityScopeKind,
    CapabilitySpec, CapabilityTrust, CreateAgentRequest, CreateCapabilityRequest,
    CreateProjectRequest, CreateSessionRequest, DennettError, EnqueueFollowupRequest,
    FollowupStatus, MemoryScope, MessageRole, PolicyEffect, RunStatus, SendMessageRequest,
    SessionKind, SteerSessionRequest, UpdateFollowupRequest, WorkspaceWritePolicy,
};
use dennett_head_client::HeadClient;
use dennett_testkit::profile::{TEST_PROFILE_ENV, TestProfile};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::time;
use uuid::Uuid;

fn with_isolated_control_schema(base_url: &str, prefix: &str) -> String {
    let short = &Uuid::new_v4().simple().to_string()[..12];
    let schema = format!("iso_{prefix}_{short}");
    let sep = if base_url.contains('?') { '&' } else { '?' };
    format!("{base_url}{sep}options=-csearch_path%3D{schema}")
}

fn require_control_pg_url() -> String {
    assert_eq!(
        env::var(TEST_PROFILE_ENV).ok().as_deref(),
        Some("1"),
        "{TEST_PROFILE_ENV}=1 is required for D3+ Live Head integration tests"
    );
    let ctrl = env::var("DENNETT_TEST_CONTROL_PG_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            env::var("DENNETT_TEST_DATABASE_URL")
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
        .expect("DENNETT_TEST_CONTROL_PG_URL (or DENNETT_TEST_DATABASE_URL) is required");
    with_isolated_control_schema(&ctrl, "d3live")
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

struct RunningHead {
    child: Child,
    addr: String,
    base_url: String,
}

impl RunningHead {
    async fn spawn_simulated(
        control_pg_url: &str,
        profile_root: Option<&Path>,
        with_control_mcp: bool,
    ) -> Self {
        let bin = locate_binary("dennett-head", "dennett-head");
        let mut cmd = Command::new(&bin);
        cmd.args(["--serve", "127.0.0.1:0", "--runtime", "simulated"]);
        if let Some(root) = profile_root {
            cmd.args(["--profile-root", &root.to_string_lossy()]);
        }
        if with_control_mcp {
            let ctrl_mcp = locate_binary("dennett-control-mcp", "dennett-control-mcp");
            cmd.args(["--control-mcp-bin", &ctrl_mcp.to_string_lossy()]);
        }
        let mut child = cmd
            .env(TEST_PROFILE_ENV, "1")
            .env("DENNETT_TEST_CONTROL_PG_URL", control_pg_url)
            .env_remove("DENNETT_CONTROL_PG_URL")
            .env_remove("DENNETT_MEMORY_URL")
            .env_remove("DENNETT_MEMORY_TOKEN")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("spawn dennett-head");

        let stdout = child.stdout.take().expect("stdout piped");
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        match time::timeout(Duration::from_secs(15), reader.read_line(&mut line)).await {
            Ok(Ok(n)) if n > 0 => {}
            other => panic!("did not receive LISTENING line from dennett-head: {other:?}"),
        }
        let addr = line
            .trim()
            .strip_prefix("LISTENING ")
            .unwrap_or_else(|| panic!("unexpected startup line: `{line}`"))
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
        if time::timeout(Duration::from_secs(5), self.child.wait())
            .await
            .is_err()
        {
            let _ = self.child.kill().await;
            let _ = self.child.wait().await;
        }
    }
}

async fn run_dennettctl(head_url: &str, args: &[&str]) -> (i32, String, String) {
    let bin = locate_binary("dennettctl", "dennett-head-client");
    let out = Command::new(&bin)
        .args(["--url", head_url])
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .expect("execute dennettctl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn unique_scope(prefix: &str) -> MemoryScope {
    let short = &Uuid::new_v4().simple().to_string()[..10];
    MemoryScope::try_new(format!("{prefix}-{short}")).expect("scope")
}

async fn wait_for_active_live_run(
    client: &HeadClient,
    session_id: dennett_contracts::SessionId,
    timeout_secs: u64,
) -> dennett_contracts::LiveRunSnapshot {
    let deadline = time::Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        let live = client
            .get_session_live(session_id, None)
            .await
            .expect("poll live");
        if let Some(active) = live.active {
            return active;
        }
        assert!(
            time::Instant::now() < deadline,
            "timed out waiting for active live run on session {session_id}"
        );
        time::sleep(Duration::from_millis(25)).await;
    }
}

async fn wait_for_assistant_message_containing(
    client: &HeadClient,
    session_id: dennett_contracts::SessionId,
    needle: &str,
    timeout_secs: u64,
) -> dennett_contracts::SessionDetailResponse {
    let deadline = time::Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        let detail = client
            .get_session_detail(session_id, None)
            .await
            .expect("poll session detail");
        if detail
            .messages
            .iter()
            .any(|m| m.role == MessageRole::Assistant && m.content.contains(needle))
        {
            return detail;
        }
        assert!(
            time::Instant::now() < deadline,
            "timed out waiting for assistant message containing `{needle}`; messages={:?}",
            detail.messages
        );
        time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "end-to-end test verifies mid-turn streaming_text, tool paragraph separation, context usage persistence, audit logging, and CLI parity"
)]
async fn d3_live_streaming_text_tool_break_and_context_usage_persisted() {
    let ctrl_pg = require_control_pg_url();
    let (profile, _tmp) = TestProfile::create_on_disk("d3-live-stream").expect("temp profile");
    let head = RunningHead::spawn_simulated(&ctrl_pg, Some(profile.data_dir()), false).await;
    let client = HeadClient::new(&head.base_url).expect("client");

    let health = client.healthz(None).await.expect("health");
    assert_eq!(health.schema_version, Some(10));

    let scope = unique_scope("live-stream");
    let project = client
        .create_project(
            &CreateProjectRequest {
                name: String::from("Live Stream Project"),
                workspace_path: Some(profile.workspace_dir().to_string_lossy().into_owned()),
                memory_scope: Some(scope),
            },
            None,
        )
        .await
        .expect("project");
    let agent = client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Live Agent"),
                description: String::from("Streaming verifier"),
                system_prompt: String::from("Respond concisely."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::new(),
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
            },
            None,
        )
        .await
        .expect("agent");
    let session = client
        .create_session(
            &CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: Some(project.project_id),
                title: String::from("Live Stream Chat"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("session");

    // Start streaming turn with silent tool call between two prose chunks.
    let turn_client = HeadClient::new(&head.base_url).expect("turn client");
    let sid = session.session_id;
    let turn_handle = tokio::spawn(async move {
        turn_client
            .send_message(
                sid,
                &SendMessageRequest {
                    content: String::from("[SIM:STREAM_TOOL:Checking repo...|All checks passed.]"),
                    timeout_ms: Some(10_000),
                    client_message_id: None,
                },
                None,
            )
            .await
    });

    // Poll `GET /v1/sessions/{id}/live` mid-turn to observe clean visible streaming_text.
    let mut saw_mid_stream = false;
    let poll_deadline = time::Instant::now() + Duration::from_secs(5);
    while time::Instant::now() < poll_deadline {
        let live = client
            .get_session_live(session.session_id, None)
            .await
            .expect("get live");
        if let Some(ref snap) = live.active {
            // Confirm ZERO tool-call steps/feeds or phase fields exist in wire JSON.
            let snap_json = serde_json::to_value(snap).expect("serialize snapshot");
            let obj = snap_json.as_object().expect("snapshot object");
            assert!(!obj.contains_key("tool_calls"));
            assert!(!obj.contains_key("steps"));
            assert!(!obj.contains_key("phase"));
            assert!(!snap.streaming_text.contains("read_file"));
            if snap.streaming_text.contains("Checking repo...") {
                saw_mid_stream = true;
                break;
            }
        }
        if turn_handle.is_finished() {
            break;
        }
        time::sleep(Duration::from_millis(20)).await;
    }

    let resp = turn_handle
        .await
        .expect("join turn")
        .expect("streaming turn succeeds");
    assert!(
        saw_mid_stream,
        "expected to observe mid-turn streaming_text via GET /v1/sessions/{{id}}/live"
    );
    assert_eq!(resp.run.status, RunStatus::Succeeded);
    assert_eq!(
        resp.assistant_message.as_ref().map(|m| m.content.as_str()),
        Some("Checking repo...\n\nAll checks passed.")
    );
    assert_eq!(resp.run.context_used_tokens, Some(4096));
    assert_eq!(resp.run.context_window_tokens, Some(1_048_576));

    // After completion: live snapshot MUST be cleaned up, runs persisted in PG, and
    // server-side `tool_observed` recorded strictly in `audit_events`!
    let live_after = client
        .get_session_live(session.session_id, None)
        .await
        .expect("live after");
    assert!(live_after.active.is_none());

    let detail = client
        .get_session_detail(session.session_id, None)
        .await
        .expect("detail");
    let persisted_run = detail
        .runs
        .iter()
        .find(|r| r.run_id == resp.run.run_id)
        .expect("persisted run");
    assert_eq!(persisted_run.context_used_tokens, Some(4096));
    assert_eq!(persisted_run.context_window_tokens, Some(1_048_576));

    let diag = client.get_diagnostics(Some(100), None).await.expect("diag");
    assert!(
        diag.events
            .iter()
            .any(|ev| ev.run_id == Some(resp.run.run_id) && ev.event_kind == "tool_observed"),
        "expected server-side tool_observed in audit_events"
    );

    // CLI parity: `dennettctl --json sessions live`
    let (code, stdout, stderr) = run_dennettctl(
        &head.base_url,
        &["--json", "sessions", "live", "--session", &sid.to_string()],
    )
    .await;
    assert_eq!(code, 0, "dennettctl sessions live failed: {stderr}");
    let cli_live: dennett_contracts::LiveRunResponse =
        serde_json::from_str(&stdout).expect("parse cli live json");
    assert!(cli_live.active.is_none());

    head.shutdown().await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "end-to-end test verifies follow-up CRUD, idempotency, V03 auto-dispatch, idle enqueue, and CLI parity"
)]
async fn d3_durable_followups_crud_dedup_v03_auto_dispatch_and_idle_enqueue() {
    let ctrl_pg = require_control_pg_url();
    let (profile, _tmp) = TestProfile::create_on_disk("d3-followups").expect("temp profile");
    let head = RunningHead::spawn_simulated(&ctrl_pg, Some(profile.data_dir()), false).await;
    let client = HeadClient::new(&head.base_url).expect("client");

    let agent = client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Followup Agent"),
                description: String::from("Queue verifier"),
                system_prompt: String::from("Follow instructions."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::new(),
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
            },
            None,
        )
        .await
        .expect("agent");
    let session = client
        .create_session(
            &CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: None,
                title: String::from("Followup Queue Chat"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("session");
    let sid = session.session_id;

    // 1. Start a slow turn and immediately detach/ignore caller HTTP completion (proves V03!).
    let slow_client = HeadClient::new(&head.base_url).expect("slow client");
    let slow_task = tokio::spawn(async move {
        let _ = slow_client
            .send_message(
                sid,
                &SendMessageRequest {
                    content: String::from("[SIM:SLOW:900] initial work"),
                    timeout_ms: Some(10_000),
                    client_message_id: None,
                },
                None,
            )
            .await;
    });
    let _active = wait_for_active_live_run(&client, sid, 5).await;

    // 2. Enqueue 3 follow-ups while run is active, test idempotency dedup & mismatch.
    let f1 = client
        .enqueue_followup(
            sid,
            &EnqueueFollowupRequest {
                content: String::from("FOLLOWUP_ONE_PAYLOAD"),
                idempotency_key: Some(String::from("idem-f1")),
            },
            None,
        )
        .await
        .expect("enqueue f1");
    assert_eq!(f1.status, FollowupStatus::Queued);

    let f1_dup = client
        .enqueue_followup(
            sid,
            &EnqueueFollowupRequest {
                content: String::from("FOLLOWUP_ONE_PAYLOAD"),
                idempotency_key: Some(String::from("idem-f1")),
            },
            None,
        )
        .await
        .expect("dedup f1");
    assert_eq!(f1_dup.followup_id, f1.followup_id);

    let f1_mismatch = client
        .enqueue_followup(
            sid,
            &EnqueueFollowupRequest {
                content: String::from("DIFFERENT_BODY"),
                idempotency_key: Some(String::from("idem-f1")),
            },
            None,
        )
        .await;
    assert!(
        matches!(f1_mismatch, Err(DennettError::IdempotencyMismatch { .. })),
        "expected IdempotencyMismatch, got {f1_mismatch:?}"
    );

    let f2 = client
        .enqueue_followup(
            sid,
            &EnqueueFollowupRequest {
                content: String::from("FOLLOWUP_TWO_ORIGINAL"),
                idempotency_key: None,
            },
            None,
        )
        .await
        .expect("enqueue f2");
    let f3 = client
        .enqueue_followup(
            sid,
            &EnqueueFollowupRequest {
                content: String::from("FOLLOWUP_THREE_DELETE_ME"),
                idempotency_key: None,
            },
            None,
        )
        .await
        .expect("enqueue f3");

    // 3. Edit F2 and delete F3 via HTTP API.
    let f2_edited = client
        .update_followup(
            sid,
            f2.followup_id,
            &UpdateFollowupRequest {
                content: String::from("FOLLOWUP_TWO_EDITED"),
            },
            None,
        )
        .await
        .expect("edit f2");
    assert_eq!(f2_edited.content, "FOLLOWUP_TWO_EDITED");

    client
        .delete_followup(sid, f3.followup_id, None)
        .await
        .expect("delete f3");

    // Verify `SessionDetailResponse.followups` reflects [F1, F2_edited] in FIFO order.
    let detail_mid = client
        .get_session_detail(sid, None)
        .await
        .expect("detail mid");
    assert_eq!(detail_mid.followups.len(), 2);
    assert_eq!(detail_mid.followups[0].followup_id, f1.followup_id);
    assert_eq!(detail_mid.followups[1].followup_id, f2.followup_id);
    assert_eq!(detail_mid.followups[1].content, "FOLLOWUP_TWO_EDITED");

    // Drop the slow HTTP awaiter explicitly to prove V03: Head still finishes & dispatches queue!
    slow_task.abort();

    // 4. Wait for V03 server-side auto-dispatch of F1 AND F2_edited in FIFO order!
    let detail_done =
        wait_for_assistant_message_containing(&client, sid, "FOLLOWUP_TWO_EDITED", 10).await;
    let assistant_msgs: Vec<&str> = detail_done
        .messages
        .iter()
        .filter(|m| m.role == MessageRole::Assistant)
        .map(|m| m.content.as_str())
        .collect();
    let idx_f1 = assistant_msgs
        .iter()
        .position(|m| m.contains("FOLLOWUP_ONE_PAYLOAD"))
        .expect("F1 auto-dispatched");
    let idx_f2 = assistant_msgs
        .iter()
        .position(|m| m.contains("FOLLOWUP_TWO_EDITED"))
        .expect("F2 auto-dispatched");
    assert!(
        idx_f1 < idx_f2,
        "expected FIFO auto-dispatch order F1 < F2, got {assistant_msgs:?}"
    );
    assert!(
        !assistant_msgs
            .iter()
            .any(|m| m.contains("FOLLOWUP_THREE_DELETE_ME")),
        "deleted F3 must never execute"
    );

    // Wait briefly for F2's worker to release active slot.
    let idle_deadline = time::Instant::now() + Duration::from_secs(5);
    while client
        .get_session_live(sid, None)
        .await
        .expect("poll idle")
        .active
        .is_some()
    {
        assert!(time::Instant::now() < idle_deadline);
        time::sleep(Duration::from_millis(25)).await;
    }

    // Editing an already-dispatched follow-up MUST return 409 Conflict!
    let edit_dispatched = client
        .update_followup(
            sid,
            f1.followup_id,
            &UpdateFollowupRequest {
                content: String::from("Too late"),
            },
            None,
        )
        .await;
    assert!(
        matches!(edit_dispatched, Err(DennettError::Conflict { .. })),
        "expected Conflict editing dispatched follow-up, got {edit_dispatched:?}"
    );

    // 5. Idle enqueue auto-dispatch (race right after run end must never strand a message!).
    let (code, stdout, stderr) = run_dennettctl(
        &head.base_url,
        &[
            "--json",
            "sessions",
            "followup",
            "add",
            "--session",
            &sid.to_string(),
            "--message",
            "IDLE_AUTO_DISPATCH_CLI",
        ],
    )
    .await;
    assert_eq!(code, 0, "dennettctl followup add failed: {stderr}");
    let cli_fup: dennett_contracts::SessionFollowupRecord =
        serde_json::from_str(&stdout).expect("parse followup add");
    assert_eq!(cli_fup.status, FollowupStatus::Dispatched);
    assert!(cli_fup.dispatched_run_id.is_some());

    let _ = wait_for_assistant_message_containing(&client, sid, "IDLE_AUTO_DISPATCH_CLI", 10).await;

    head.shutdown().await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "end-to-end test verifies safe pause on both Stop and Failed runs, resume auto-start, and clear"
)]
async fn d3_followups_safe_pause_on_stop_and_failure_plus_resume_auto_start() {
    let ctrl_pg = require_control_pg_url();
    let (profile, _tmp) = TestProfile::create_on_disk("d3-fup-pause").expect("temp profile");
    let head = RunningHead::spawn_simulated(&ctrl_pg, Some(profile.data_dir()), false).await;
    let client = HeadClient::new(&head.base_url).expect("client");

    let agent = client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Pause Agent"),
                description: String::from("Safe pause verifier"),
                system_prompt: String::from("Follow instructions."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::new(),
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
            },
            None,
        )
        .await
        .expect("agent");
    let session = client
        .create_session(
            &CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: None,
                title: String::from("Safe Pause Chat"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("session");
    let sid = session.session_id;

    // --- Part A: Explicit Stop pauses queued follow-ups ---
    let slow_client = HeadClient::new(&head.base_url).expect("slow client");
    let slow_handle = tokio::spawn(async move {
        slow_client
            .send_message(
                sid,
                &SendMessageRequest {
                    content: String::from("[SIM:SLOW:6000] stop me"),
                    timeout_ms: Some(10_000),
                    client_message_id: None,
                },
                None,
            )
            .await
    });
    let _ = wait_for_active_live_run(&client, sid, 5).await;

    let q1 = client
        .enqueue_followup(
            sid,
            &EnqueueFollowupRequest {
                content: String::from("PAUSED_AFTER_STOP_1"),
                idempotency_key: None,
            },
            None,
        )
        .await
        .expect("enqueue q1");
    let q2 = client
        .enqueue_followup(
            sid,
            &EnqueueFollowupRequest {
                content: String::from("PAUSED_AFTER_STOP_2"),
                idempotency_key: None,
            },
            None,
        )
        .await
        .expect("enqueue q2");

    let cancel_resp = client.cancel_run(sid, None).await.expect("explicit stop");
    assert!(cancel_resp.cancelled);
    let stopped_turn = slow_handle
        .await
        .expect("join slow")
        .expect("cancelled turn");
    assert_eq!(stopped_turn.run.status, RunStatus::Cancelled);

    let live_paused = client
        .get_session_live(sid, None)
        .await
        .expect("live paused");
    assert!(live_paused.active.is_none());
    assert_eq!(live_paused.followups.len(), 2);
    assert!(
        live_paused
            .followups
            .iter()
            .all(|f| f.status == FollowupStatus::Paused),
        "explicit Stop must pause all queued follow-ups: {:?}",
        live_paused.followups
    );

    // Enqueueing while paused items exist must NOT skip ahead of paused items!
    let q3 = client
        .enqueue_followup(
            sid,
            &EnqueueFollowupRequest {
                content: String::from("QUEUED_BEHIND_PAUSED_3"),
                idempotency_key: None,
            },
            None,
        )
        .await
        .expect("enqueue q3 behind paused");
    assert_eq!(q3.status, FollowupStatus::Queued);
    assert!(q3.dispatched_run_id.is_none());

    // Delete Q3 so we resume [Q1, Q2], then invoke `POST /followups/resume`!
    client
        .delete_followup(sid, q3.followup_id, None)
        .await
        .expect("del q3");
    let resume_resp = client.resume_followups(sid, None).await.expect("resume");
    let dispatched_q1 = resume_resp
        .dispatched
        .expect("resume on idle session must auto-start head follow-up");
    assert_eq!(dispatched_q1.user_message.content, q1.content);
    assert_eq!(dispatched_q1.run.status, RunStatus::Succeeded);

    // Q2 chains automatically after Q1 succeeds!
    let _ = wait_for_assistant_message_containing(&client, sid, &q2.content, 10).await;
    let idle_deadline = time::Instant::now() + Duration::from_secs(5);
    while client
        .get_session_live(sid, None)
        .await
        .expect("poll idle")
        .active
        .is_some()
    {
        assert!(time::Instant::now() < idle_deadline);
        time::sleep(Duration::from_millis(25)).await;
    }

    // --- Part B: Failed run pauses remaining queued follow-ups ---
    let err_lead_client = HeadClient::new(&head.base_url).expect("err lead client");
    let err_lead_task = tokio::spawn(async move {
        let _ = err_lead_client
            .send_message(
                sid,
                &SendMessageRequest {
                    content: String::from("[SIM:SLOW:400] lead into failing followup"),
                    timeout_ms: Some(10_000),
                    client_message_id: None,
                },
                None,
            )
            .await;
    });
    let _ = wait_for_active_live_run(&client, sid, 5).await;

    let _f_err = client
        .enqueue_followup(
            sid,
            &EnqueueFollowupRequest {
                content: String::from("[SIM:ERROR:boom_followup_failure]"),
                idempotency_key: None,
            },
            None,
        )
        .await
        .expect("enqueue failing followup");
    let f_after = client
        .enqueue_followup(
            sid,
            &EnqueueFollowupRequest {
                content: String::from("MUST_PAUSE_AFTER_ERROR"),
                idempotency_key: None,
            },
            None,
        )
        .await
        .expect("enqueue followup after error");
    let _ = err_lead_task.await;

    // Wait for F_ERR to fail and pause F_AFTER.
    let pause_deadline = time::Instant::now() + Duration::from_secs(10);
    loop {
        let live = client
            .get_session_live(sid, None)
            .await
            .expect("poll fail pause");
        if live.active.is_none()
            && live
                .followups
                .iter()
                .any(|f| f.followup_id == f_after.followup_id && f.status == FollowupStatus::Paused)
        {
            break;
        }
        assert!(
            time::Instant::now() < pause_deadline,
            "timed out waiting for F_AFTER to pause after failed run; live={live:?}"
        );
        time::sleep(Duration::from_millis(40)).await;
    }

    // Clear all remaining follow-ups via `dennettctl sessions followup clear`.
    let (code, _, stderr) = run_dennettctl(
        &head.base_url,
        &[
            "--json",
            "sessions",
            "followup",
            "clear",
            "--session",
            &sid.to_string(),
        ],
    )
    .await;
    assert_eq!(code, 0, "dennettctl clear failed: {stderr}");

    let after_clear = client
        .get_session_live(sid, None)
        .await
        .expect("after clear");
    assert_eq!(after_clear.followups.len(), 0);

    head.shutdown().await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "end-to-end test verifies mid-run live steer without 409 Conflict, preservation of queued follow-ups, and CLI parity"
)]
async fn d3_live_steer_mid_run_without_409_preserving_queued_followups() {
    let ctrl_pg = require_control_pg_url();
    let (profile, _tmp) = TestProfile::create_on_disk("d3-live-steer").expect("temp profile");
    let head = RunningHead::spawn_simulated(&ctrl_pg, Some(profile.data_dir()), false).await;
    let client = HeadClient::new(&head.base_url).expect("client");

    let agent = client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Steer Agent"),
                description: String::from("Live steer verifier"),
                system_prompt: String::from("Adapt immediately."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::new(),
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
            },
            None,
        )
        .await
        .expect("agent");
    let session = client
        .create_session(
            &CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: None,
                title: String::from("Live Steer Chat"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("session");
    let sid = session.session_id;

    let slow_client = HeadClient::new(&head.base_url).expect("slow client");
    let slow_handle = tokio::spawn(async move {
        slow_client
            .send_message(
                sid,
                &SendMessageRequest {
                    content: String::from("[SIM:SLOW:8000] obsolete plan A"),
                    timeout_ms: Some(15_000),
                    client_message_id: None,
                },
                None,
            )
            .await
    });
    let _ = wait_for_active_live_run(&client, sid, 5).await;

    // Queue a follow-up BEFORE steering; steer MUST NOT pause or drop it!
    let preserved = client
        .enqueue_followup(
            sid,
            &EnqueueFollowupRequest {
                content: String::from("QUEUED_PRESERVED_AFTER_STEER"),
                idempotency_key: None,
            },
            None,
        )
        .await
        .expect("enqueue preserved followup");
    assert_eq!(preserved.status, FollowupStatus::Queued);

    // Invoke `POST /v1/sessions/{id}/steer` mid-run: ZERO 409 Conflict!
    let steered = client
        .steer_session(
            sid,
            &SteerSessionRequest {
                content: String::from("STEERED_PLAN_B"),
                client_message_id: Some(String::from("steer-idem-1")),
                timeout_ms: Some(10_000),
            },
            None,
        )
        .await
        .expect("steer must succeed cleanly without 409 Conflict");
    assert_eq!(steered.run.status, RunStatus::Succeeded);
    assert!(
        steered
            .assistant_message
            .as_ref()
            .is_some_and(|m| m.content.contains("STEERED_PLAN_B"))
    );

    let orig_slow = slow_handle
        .await
        .expect("join slow")
        .expect("orig cancelled");
    assert_eq!(orig_slow.run.status, RunStatus::Cancelled);

    // Preserved queued follow-up MUST auto-dispatch after the steered run succeeded!
    let detail_after =
        wait_for_assistant_message_containing(&client, sid, "QUEUED_PRESERVED_AFTER_STEER", 10)
            .await;
    assert!(
        detail_after
            .messages
            .iter()
            .any(|m| m.role == MessageRole::Assistant && m.content.contains("STEERED_PLAN_B"))
    );

    // CLI parity: `dennettctl --json sessions steer` works cleanly when idle too.
    let idle_deadline = time::Instant::now() + Duration::from_secs(5);
    while client
        .get_session_live(sid, None)
        .await
        .expect("poll idle")
        .active
        .is_some()
    {
        assert!(time::Instant::now() < idle_deadline);
        time::sleep(Duration::from_millis(25)).await;
    }
    let (code, stdout, stderr) = run_dennettctl(
        &head.base_url,
        &[
            "--json",
            "sessions",
            "steer",
            "--session",
            &sid.to_string(),
            "--message",
            "CLI_STEER_CHECK",
        ],
    )
    .await;
    assert_eq!(code, 0, "dennettctl sessions steer failed: {stderr}");
    let cli_steer: dennett_contracts::SendMessageResponse =
        serde_json::from_str(&stdout).expect("parse cli steer");
    assert_eq!(cli_steer.run.status, RunStatus::Succeeded);

    head.shutdown().await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "end-to-end test proves dennett_skill_list/read MCP isolation, project override, standalone global-only + event denial, and prompt index injection"
)]
async fn d3_session_skills_delivery_mcp_scope_isolation_standalone_and_prompt_index() {
    let ctrl_pg = require_control_pg_url();
    let (profile, _tmp) = TestProfile::create_on_disk("d3-session-skills").expect("temp profile");
    let head = RunningHead::spawn_simulated(&ctrl_pg, Some(profile.data_dir()), true).await;
    let client = HeadClient::new(&head.base_url).expect("client");

    // Direct call to `/v1/capabilities/session-skills` without `dct1.` token MUST fail 401!
    let unauth = client.list_session_skills(None).await;
    assert!(
        matches!(unauth, Err(DennettError::Unauthorized { .. })),
        "expected Unauthorized without dct1. token, got {unauth:?}"
    );

    let proj_a = client
        .create_project(
            &CreateProjectRequest {
                name: String::from("Skills Project A"),
                workspace_path: Some(profile.workspace_dir().to_string_lossy().into_owned()),
                memory_scope: Some(unique_scope("skills-a")),
            },
            None,
        )
        .await
        .expect("proj_a");
    let proj_b = client
        .create_project(
            &CreateProjectRequest {
                name: String::from("Skills Project B"),
                workspace_path: None,
                memory_scope: Some(unique_scope("skills-b")),
            },
            None,
        )
        .await
        .expect("proj_b");

    let agent = client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Skillful Agent"),
                description: String::from("Skill isolation verifier"),
                system_prompt: String::from("Base agent instructions."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::new(),
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
            },
            None,
        )
        .await
        .expect("agent");

    // Register test skills covering pinned, active, on_demand, override, disabled, blocked, foreign.
    let skill_defs = [
        (
            "global-pinned",
            "Global Pinned",
            "Always inline",
            "1.0.0",
            CapabilityScopeKind::Global,
            None,
            CapabilityTrust::Trusted,
            CapabilityActivation::Pinned,
            "# Global Pinned Rules\nFollow strict mathematical rigor.",
        ),
        (
            "global-active",
            "Global Active",
            "Available globally",
            "1.2.0",
            CapabilityScopeKind::Global,
            None,
            CapabilityTrust::Trusted,
            CapabilityActivation::Active,
            "# Global Active Body\nGlobal body marker.",
        ),
        (
            "shared-override",
            "Global Shared",
            "Global version",
            "1.0.0",
            CapabilityScopeKind::Global,
            None,
            CapabilityTrust::Trusted,
            CapabilityActivation::Active,
            "# Shared Global Version\nGLOBAL_OVERRIDE_BODY",
        ),
        (
            "shared-override",
            "Project A Shared",
            "Project A override version",
            "2.0.0",
            CapabilityScopeKind::Project,
            Some(proj_a.project_id),
            CapabilityTrust::Trusted,
            CapabilityActivation::Active,
            "# Shared Project A Version\nPROJECT_A_OVERRIDE_BODY",
        ),
        (
            "proj-a-ondemand",
            "Project A OnDemand",
            "Deep audit checklist",
            "1.1.0",
            CapabilityScopeKind::Project,
            Some(proj_a.project_id),
            CapabilityTrust::Trusted,
            CapabilityActivation::OnDemand,
            "# Project A OnDemand\nPROJ_A_ONDEMAND_BODY",
        ),
        (
            "proj-a-disabled",
            "Disabled Skill",
            "Should never be readable",
            "1.0.0",
            CapabilityScopeKind::Project,
            Some(proj_a.project_id),
            CapabilityTrust::Trusted,
            CapabilityActivation::Disabled,
            "# Disabled\nDISABLED_SECRET",
        ),
        (
            "proj-a-blocked",
            "Blocked Skill",
            "Restricted untrusted skill",
            "1.0.0",
            CapabilityScopeKind::Project,
            Some(proj_a.project_id),
            CapabilityTrust::Restricted,
            CapabilityActivation::Active,
            "# Blocked\nBLOCKED_SECRET",
        ),
        (
            "proj-b-only",
            "Project B Secret",
            "Foreign project skill",
            "1.0.0",
            CapabilityScopeKind::Project,
            Some(proj_b.project_id),
            CapabilityTrust::Trusted,
            CapabilityActivation::Active,
            "# Project B\nFOREIGN_B_SECRET",
        ),
    ];

    for (slug, name, desc, ver, scope_kind, project_id, trust, act, md) in skill_defs {
        let _ = client
            .create_capability(
                &CreateCapabilityRequest {
                    slug: slug.to_owned(),
                    display_name: name.to_owned(),
                    description: desc.to_owned(),
                    version: ver.to_owned(),
                    origin_kind: CapabilityOrigin::UserManual,
                    ownership: if project_id.is_some() {
                        CapabilityOwnership::Project
                    } else {
                        CapabilityOwnership::User
                    },
                    scope_kind,
                    project_id,
                    trust_state: trust,
                    activation_state: act,
                    source_path: None,
                    spec: CapabilitySpec::Skill {
                        skill_markdown: md.to_owned(),
                        has_scripts: false,
                        entry_files: vec![String::from("SKILL.md")],
                    },
                },
                None,
            )
            .await
            .unwrap_or_else(|e| panic!("register skill `{slug}` failed: {e:?}"));
    }

    let sess_a = client
        .create_session(
            &CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: Some(proj_a.project_id),
                title: String::from("Project A Skill Session"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("sess_a");

    let sess_standalone = client
        .create_session(
            &CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: None,
                title: String::from("Standalone Skill Chat"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("sess_standalone");

    // 1. Project A: verify `[Dennett Active Skills Index]` prompt injection!
    let echo_a = client
        .send_message(
            sess_a.session_id,
            &SendMessageRequest {
                content: String::from("[SIM:ECHO_SYSTEM_PROMPT]"),
                timeout_ms: Some(10_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("echo prompt A");
    let prompt_a = echo_a.assistant_message.expect("prompt A text").content;
    assert!(prompt_a.contains("Base agent instructions."));
    assert!(prompt_a.contains("[Dennett Active Skills Index]"));
    assert!(prompt_a.contains("### Pinned Skill: global-pinned (v1.0.0)"));
    assert!(prompt_a.contains("Follow strict mathematical rigor."));
    assert!(prompt_a.contains("- global-active (v1.2.0): Available globally"));
    assert!(prompt_a.contains("- shared-override (v2.0.0): Project A override version"));
    assert!(prompt_a.contains("- proj-a-ondemand (v1.1.0): Deep audit checklist"));
    assert!(!prompt_a.contains("proj-a-disabled"));
    assert!(!prompt_a.contains("proj-a-blocked"));
    assert!(!prompt_a.contains("proj-b-only"));

    // 2. Project A: real `dennett-control-mcp` `dennett_skill_list` & `dennett_skill_read`!
    let list_a = client
        .send_message(
            sess_a.session_id,
            &SendMessageRequest {
                content: String::from("[SIM:CONTROL_MCP:dennett_skill_list|{}]"),
                timeout_ms: Some(10_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("list skills A");
    let list_a_text = list_a.assistant_message.expect("list A").content;
    assert!(list_a_text.contains("ALLOWED"));
    assert!(list_a_text.contains("global-pinned"));
    assert!(list_a_text.contains("shared-override"));
    assert!(list_a_text.contains("proj-a-ondemand"));
    assert!(!list_a_text.contains("proj-a-disabled"));
    assert!(!list_a_text.contains("proj-a-blocked"));
    assert!(!list_a_text.contains("proj-b-only"));

    let read_override_a = client
        .send_message(
            sess_a.session_id,
            &SendMessageRequest {
                content: String::from(
                    r#"[SIM:CONTROL_MCP:dennett_skill_read|{"slug":"shared-override"}]"#,
                ),
                timeout_ms: Some(10_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("read shared-override A");
    let read_override_a_text = read_override_a
        .assistant_message
        .expect("override A")
        .content;
    assert!(read_override_a_text.contains("PROJECT_A_OVERRIDE_BODY"));
    assert!(!read_override_a_text.contains("GLOBAL_OVERRIDE_BODY"));

    let read_ondemand_a = client
        .send_message(
            sess_a.session_id,
            &SendMessageRequest {
                content: String::from(
                    r#"[SIM:CONTROL_MCP:dennett_skill_read|{"slug":"proj-a-ondemand"}]"#,
                ),
                timeout_ms: Some(10_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("read proj-a-ondemand");
    assert!(
        read_ondemand_a
            .assistant_message
            .expect("ondemand A")
            .content
            .contains("PROJ_A_ONDEMAND_BODY")
    );

    // Disabled, blocked (restricted), and foreign-project skills MUST be denied!
    for forbidden_slug in ["proj-a-disabled", "proj-a-blocked", "proj-b-only"] {
        let res = client
            .send_message(
                sess_a.session_id,
                &SendMessageRequest {
                    content: format!(
                        r#"[SIM:CONTROL_MCP:dennett_skill_read|{{"slug":"{forbidden_slug}"}}]"#
                    ),
                    timeout_ms: Some(10_000),
                    client_message_id: None,
                },
                None,
            )
            .await
            .expect("read forbidden skill roundtrip");
        let text = res.assistant_message.expect("forbidden text").content;
        assert!(
            text.contains("ACCESS_DENIED") || text.contains("isError"),
            "expected ACCESS_DENIED for `{forbidden_slug}`, got: {text}"
        );
        assert!(!text.contains("DISABLED_SECRET"));
        assert!(!text.contains("BLOCKED_SECRET"));
        assert!(!text.contains("FOREIGN_B_SECRET"));
    }

    // 3. Standalone chat: global skills ONLY + project events STILL strictly denied!
    let echo_standalone = client
        .send_message(
            sess_standalone.session_id,
            &SendMessageRequest {
                content: String::from("[SIM:ECHO_SYSTEM_PROMPT]"),
                timeout_ms: Some(10_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("echo prompt standalone");
    let prompt_standalone = echo_standalone
        .assistant_message
        .expect("standalone prompt")
        .content;
    assert!(prompt_standalone.contains("[Dennett Active Skills Index]"));
    assert!(prompt_standalone.contains("### Pinned Skill: global-pinned (v1.0.0)"));
    assert!(prompt_standalone.contains("- shared-override (v1.0.0): Global version"));
    assert!(!prompt_standalone.contains("proj-a-ondemand"));
    assert!(!prompt_standalone.contains("proj-b-only"));

    let list_standalone = client
        .send_message(
            sess_standalone.session_id,
            &SendMessageRequest {
                content: String::from("[SIM:CONTROL_MCP:dennett_skill_list|{}]"),
                timeout_ms: Some(10_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("list standalone skills");
    let list_standalone_text = list_standalone
        .assistant_message
        .expect("standalone list")
        .content;
    assert!(list_standalone_text.contains("ALLOWED"));
    assert!(list_standalone_text.contains("global-pinned"));
    assert!(list_standalone_text.contains("global-active"));
    assert!(!list_standalone_text.contains("proj-a-ondemand"));
    assert!(!list_standalone_text.contains("proj-b-only"));

    let read_override_standalone = client
        .send_message(
            sess_standalone.session_id,
            &SendMessageRequest {
                content: String::from(
                    r#"[SIM:CONTROL_MCP:dennett_skill_read|{"slug":"shared-override"}]"#,
                ),
                timeout_ms: Some(10_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("read global override standalone");
    assert!(
        read_override_standalone
            .assistant_message
            .expect("override standalone")
            .content
            .contains("GLOBAL_OVERRIDE_BODY")
    );

    let read_proj_from_standalone = client
        .send_message(
            sess_standalone.session_id,
            &SendMessageRequest {
                content: String::from(
                    r#"[SIM:CONTROL_MCP:dennett_skill_read|{"slug":"proj-a-ondemand"}]"#,
                ),
                timeout_ms: Some(10_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("standalone read project skill");
    let denied_proj_text = read_proj_from_standalone
        .assistant_message
        .expect("standalone denied proj skill")
        .content;
    assert!(
        denied_proj_text.contains("ACCESS_DENIED") || denied_proj_text.contains("isError"),
        "standalone chat must be denied project skill, got: {denied_proj_text}"
    );
    assert!(!denied_proj_text.contains("PROJ_A_ONDEMAND_BODY"));

    // Standalone chat MUST STILL DENY `dennett_event_create` at PolicyEngine boundary!
    let standalone_evt = client
        .send_message(
            sess_standalone.session_id,
            &SendMessageRequest {
                content: String::from(
                    r#"[SIM:EVENT:CREATE title="Standalone Escape" goal="Must be denied"]"#,
                ),
                timeout_ms: Some(10_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("standalone event attempt");
    let evt_text = standalone_evt.assistant_message.expect("evt text").content;
    assert!(
        evt_text.contains("DENIED"),
        "standalone chat must deny dennett_event_create, got: {evt_text}"
    );
    assert!(
        standalone_evt
            .permission_decisions
            .iter()
            .any(|d| d.tool_name == "dennett_event_create" && d.effect == PolicyEffect::Deny)
    );

    head.shutdown().await;
}
