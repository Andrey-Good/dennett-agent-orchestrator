//! End-to-end real PostgreSQL 16 + multi-process `dennett-head` + `HeadClient` + `dennettctl`
//! integration tests for Phase D2 Slice A:
//! - Durable timers surviving full `dennett-head` restart, PG 16 IANA timezones, misfire policies
//!   (`ExecuteOnce` / `Skip` / `Ask`), and concurrent idempotent signals;
//! - Goal updates bumping `generation` and fencing stale runs (`RunStatus::Superseded`);
//! - Targeted `Stop` halting one task without touching sibling tasks, delegation scope narrowing
//!   (`child_scope ⊆ parent_scope`), and plain-text inter-agent messages (zero custom DAG);
//! - Live interactive `ask_before_write` approvals, revocable `CapabilityGrants`, and Stop withdrawal;
//! - `LostAck -> UNKNOWN` external effect persisting across daemon restart, honest `Stop`
//!   (`external_outcome_unknown`), refusal of blind retry, and explicit `/reconcile` (`count == 1`);
//! - Native clarification, external `TaskBudget`, `client_message_id` idempotency, strict
//!   Contracts C & D lifecycle separation (user workspace files NEVER deleted), artifact revisions,
//!   and `dennettctl` D2 CLI parity.

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use dennett_contracts::{
    ArtifactCompleteness, ArtifactLifecycleAction, ArtifactLifecycleRequest, BudgetExceedPolicy,
    CreateAgentRequest, CreateArtifactRequest, CreatePolicyRuleRequest, CreateProjectRequest,
    CreateSessionRequest, CreateTaskRequest, DennettError, DispatchEffectRequest,
    ExternalEffectStatus, IdempotencyKey, InboxCardKind, InboxCardStatus, InboxResolutionAction,
    MemoryScope, MissedRunPolicy, PermissionRequestStatus, PermissionResolutionDecision,
    PolicyEffect, ProjectLifecycleState, RebindProjectRequest, ReconcileEffectRequest,
    ReplyTaskRequest, ResolveInboxCardRequest, ResolvePermissionRequest, RevokeGrantRequest,
    RunStatus, ScheduleKind, SchedulerTickRequest, SendAgentMessageRequest, SendMessageRequest,
    SessionKind, SimEffectBehavior, TaskBudget, TaskId, TaskSchedule, TaskSignalRequest,
    TaskStatus, TimestampMs, UpdateTaskRequest, WorkspaceWritePolicy,
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
        "{TEST_PROFILE_ENV}=1 is required for D2 Head integration tests"
    );
    let ctrl = env::var("DENNETT_TEST_CONTROL_PG_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .expect("DENNETT_TEST_CONTROL_PG_URL is required");
    with_isolated_control_schema(&ctrl, "d2")
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
    owner_token: String,
}

impl RunningHead {
    fn client(&self) -> HeadClient {
        HeadClient::new(&self.base_url)
            .expect("head client")
            .with_owner_token(&self.owner_token)
    }

    async fn spawn_simulated(
        control_pg_url: &str,
        profile_root: Option<&Path>,
        effects_ledger: Option<&Path>,
    ) -> Self {
        let bin = locate_binary("dennett-head", "dennett-head");
        let owner_token = format!("d2-head-tok-{}", Uuid::new_v4().simple());
        let mut cmd = Command::new(&bin);
        cmd.args(["--serve", "127.0.0.1:0", "--runtime", "simulated"]);
        if let Some(root) = profile_root {
            cmd.args(["--profile-root", &root.to_string_lossy()]);
        }
        if let Some(ledger) = effects_ledger {
            cmd.args(["--effects-ledger", &ledger.to_string_lossy()]);
        }
        let mut child = cmd
            .env(TEST_PROFILE_ENV, "1")
            .env("DENNETT_TEST_CONTROL_PG_URL", control_pg_url)
            .env("DENNETT_HEAD_TOKEN", &owner_token)
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
            owner_token,
        }
    }

    async fn shutdown(mut self) {
        let req = format!(
            "POST /shutdown HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\nX-Dennett-Shutdown: 1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            self.addr, self.owner_token
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

async fn run_dennettctl(head: &RunningHead, args: &[&str]) -> (i32, String, String) {
    let bin = locate_binary("dennettctl", "dennett-head-client");
    let out = Command::new(&bin)
        .args(["--url", &head.base_url])
        .args(args)
        .env("DENNETT_HEAD_TOKEN", &head.owner_token)
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

async fn wait_for_task_status(
    client: &HeadClient,
    task_id: TaskId,
    expected: &[TaskStatus],
    timeout_secs: u64,
) -> dennett_contracts::TaskDetailResponse {
    let deadline = time::Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        let detail = client
            .get_task_detail(task_id, None)
            .await
            .expect("poll task detail");
        if expected.contains(&detail.task.status) {
            return detail;
        }
        assert!(
            time::Instant::now() < deadline,
            "timed out waiting for task {task_id} status in {expected:?}; last={:?}",
            detail.task
        );
        time::sleep(Duration::from_millis(75)).await;
    }
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "end-to-end test proves timers survive full Head restart, PG 16 IANA TZ, ExecuteOnce/Skip/Ask misfire policies, and concurrent idempotent signals"
)]
async fn d2_timers_survive_head_restart_misfire_policies_and_concurrent_idempotent_signals() {
    let ctrl_pg = require_control_pg_url();
    let (profile, _tmp) = TestProfile::create_on_disk("d2-timers").expect("temp profile");
    let scope = unique_scope("d2-timers");

    // 1. Start Head #1, verify schema v4 & D2 capabilities, create scheduled tasks.
    let head1 = RunningHead::spawn_simulated(&ctrl_pg, Some(profile.data_dir()), None).await;
    let c1 = head1.client();
    let health1 = c1.healthz(None).await.expect("health1");
    assert_eq!(health1.stage, "D3");
    assert_eq!(health1.schema_version, Some(11));
    assert!(health1.capabilities.iter().any(|c| c == "control.tasks.v1"));

    let proj = c1
        .create_project(
            &CreateProjectRequest {
                name: String::from("Timer Restart Project"),
                workspace_path: Some(profile.workspace_dir().to_string_lossy().into_owned()),
                memory_scope: Some(scope.clone()),
            },
            None,
        )
        .await
        .expect("create project");
    let agent = c1
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Scheduler Agent"),
                description: String::from("Durable timer verifier"),
                system_prompt: String::from("Complete scheduled work cleanly."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope]),
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
                autonomy_mode: None,
            },
            None,
        )
        .await
        .expect("create agent");

    // Verify PostgreSQL 16 `pg_timezone_names` rejects invalid IANA timezone names!
    let bad_tz = c1
        .create_task(
            &CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Bad TZ")),
                goal: String::from("Should fail"),
                schedule: Some(TaskSchedule {
                    kind: ScheduleKind::DailyWallTime { hour: 9, minute: 0 },
                    timezone: String::from("Mars/Olympus_Mons"),
                    original_expression: None,
                    missed_run_policy: MissedRunPolicy::ExecuteOnce,
                    next_fire_at: None,
                }),
                wait_for_signal: None,
                budget: None,
                attached_skills: Vec::new(),
                auth_profile_override: None,
                execute_immediately: Some(false),
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
            },
            None,
        )
        .await;
    assert!(
        matches!(bad_tz, Err(DennettError::InvalidInput { .. })),
        "invalid IANA timezone must be rejected by PG 16 pg_timezone_names, got: {bad_tz:?}"
    );

    // Arm timers in the future (+10 min) so background scheduler doesn't race our controlled restart + tick!
    let base_ms = TimestampMs::now().as_millis();
    let fire_target_ms = base_ms + 600_000;

    let once_task = c1
        .create_task(
            &CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("OnceAt Restart Task")),
                goal: String::from("ONCE_AT_SURVIVED_RESTART"),
                schedule: Some(TaskSchedule {
                    kind: ScheduleKind::OnceAt {
                        fire_at: TimestampMs::from_millis(fire_target_ms),
                    },
                    timezone: String::from("Asia/Yekaterinburg"),
                    original_expression: Some(String::from("in 10m Yekaterinburg")),
                    missed_run_policy: MissedRunPolicy::ExecuteOnce,
                    next_fire_at: None,
                }),
                wait_for_signal: None,
                budget: None,
                attached_skills: Vec::new(),
                auth_profile_override: None,
                execute_immediately: Some(false),
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
            },
            None,
        )
        .await
        .expect("create once task");
    assert_eq!(once_task.status, TaskStatus::WaitingTimer);

    let mk_interval_task = |title: &str, goal: &str, policy: MissedRunPolicy| {
        let client = c1.clone();
        let pid = proj.project_id;
        let aid = agent.agent_id;
        let t = title.to_owned();
        let g = goal.to_owned();
        async move {
            client
                .create_task(
                    &CreateTaskRequest {
                        project_id: pid,
                        agent_id: aid,
                        session_id: None,
                        parent_task_id: None,
                        title: Some(t),
                        goal: g,
                        schedule: Some(TaskSchedule {
                            kind: ScheduleKind::Interval { interval_secs: 60 },
                            timezone: String::from("Asia/Yekaterinburg"),
                            original_expression: None,
                            missed_run_policy: policy,
                            next_fire_at: None,
                        }),
                        wait_for_signal: None,
                        budget: None,
                        attached_skills: Vec::new(),
                        auth_profile_override: None,
                        execute_immediately: Some(false),
                        target_session_id: None,
                        notify_session_id: None,
                        model_override: None,
                        thinking_override: None,
                        connection_override_id: None,
                        skill_slugs_override: None,
                        trigger: None,
                    },
                    None,
                )
                .await
                .expect("create interval task")
        }
    };

    let exec_once_task = mk_interval_task(
        "Misfire ExecuteOnce",
        "COALESCE_ONCE",
        MissedRunPolicy::ExecuteOnce,
    )
    .await;
    let skip_task = mk_interval_task("Misfire Skip", "SHOULD_SKIP", MissedRunPolicy::Skip).await;
    let ask_task = mk_interval_task("Misfire Ask", "ASK_THEN_RUN", MissedRunPolicy::Ask).await;

    // 2. Full `dennett-head` restart! All armed timers MUST survive in PostgreSQL!
    head1.shutdown().await;
    let head2 = RunningHead::spawn_simulated(&ctrl_pg, Some(profile.data_dir()), None).await;
    let c2 = head2.client();

    // Advance virtual scheduler time +20 min (> 60s misfire grace past all 4 timers!).
    let tick_ms = fire_target_ms + 600_000;
    let tick1 = c2
        .scheduler_tick(
            &SchedulerTickRequest {
                now_ms: Some(tick_ms),
            },
            None,
        )
        .await
        .expect("scheduler tick 1");
    assert!(
        tick1.woken_tasks >= 2,
        "expected OnceAt and ExecuteOnce tasks to wake on tick1: {tick1:?}"
    );
    assert!(
        tick1.skipped_timers >= 1,
        "Skip policy must skip overdue firing: {tick1:?}"
    );
    assert!(
        tick1.asked_timers >= 1,
        "Ask policy must open missed_schedule inbox card: {tick1:?}"
    );

    // Wait for OnceAt + ExecuteOnce to finish cleanly.
    let once_done =
        wait_for_task_status(&c2, once_task.task_id, &[TaskStatus::Succeeded], 10).await;
    assert_eq!(
        once_done.runs.len(),
        1,
        "OnceAt timer must execute exactly once"
    );

    // Repeating tick at the SAME `tick_ms` MUST NOT double-fire OnceAt or ExecuteOnce!
    let tick2 = c2
        .scheduler_tick(
            &SchedulerTickRequest {
                now_ms: Some(tick_ms),
            },
            None,
        )
        .await
        .expect("scheduler tick 2");
    assert_eq!(
        tick2.woken_tasks, 0,
        "re-ticking at the same timestamp must not double-fire already-claimed timers: {tick2:?}"
    );

    let exec_once_detail = wait_for_task_status(
        &c2,
        exec_once_task.task_id,
        &[TaskStatus::WaitingTimer, TaskStatus::Succeeded],
        10,
    )
    .await;
    assert_eq!(
        exec_once_detail.runs.len(),
        1,
        "ExecuteOnce must coalesce 10+ missed intervals into EXACTLY ONE run (zero storm)"
    );

    let skip_detail = c2
        .get_task_detail(skip_task.task_id, None)
        .await
        .expect("skip detail");
    assert_eq!(
        skip_detail.runs.len(),
        0,
        "Skip policy must not run overdue interval"
    );
    assert_eq!(skip_detail.task.status, TaskStatus::WaitingTimer);

    // Resolve `Ask` missed_schedule inbox card -> verify stale revision fails 409, valid revision runs once!
    let ask_inbox = c2
        .list_inbox_cards(Some("open"), Some(proj.project_id), None)
        .await
        .expect("list inbox");
    let missed_card = ask_inbox
        .cards
        .iter()
        .find(|c| c.task_id == Some(ask_task.task_id) && c.kind == InboxCardKind::MissedSchedule)
        .expect("missed_schedule inbox card created");

    let stale_err = c2
        .resolve_inbox_card(
            missed_card.card_id,
            &ResolveInboxCardRequest {
                expected_revision: missed_card.revision + 99,
                resolution: InboxResolutionAction::MissedSchedule { run_now: true },
            },
            None,
        )
        .await;
    assert!(
        matches!(stale_err, Err(DennettError::Conflict { .. })),
        "stale inbox revision must fail closed with 409 Conflict"
    );

    let resolved_missed = c2
        .resolve_inbox_card(
            missed_card.card_id,
            &ResolveInboxCardRequest {
                expected_revision: missed_card.revision,
                resolution: InboxResolutionAction::MissedSchedule { run_now: true },
            },
            None,
        )
        .await
        .expect("resolve missed schedule run_now");
    assert_eq!(resolved_missed.card.status, InboxCardStatus::Resolved);
    let ask_done = wait_for_task_status(
        &c2,
        ask_task.task_id,
        &[TaskStatus::WaitingTimer, TaskStatus::Succeeded],
        10,
    )
    .await;
    assert_eq!(ask_done.runs.len(), 1);

    // 3. Concurrent duplicate `POST /v1/tasks/{id}/signals` -> executes EXACTLY ONCE + payload mismatch -> 409!
    let sig_task = c2
        .create_task(
            &CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Signal Waiter")),
                goal: String::from("WAIT_FOR_WEBHOOK_SIGNAL"),
                schedule: None,
                wait_for_signal: Some(String::from("ci.passed")),
                budget: None,
                attached_skills: Vec::new(),
                auth_profile_override: None,
                execute_immediately: Some(false),
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
            },
            None,
        )
        .await
        .expect("create signal task");
    assert_eq!(sig_task.status, TaskStatus::WaitingSignal);

    let sig_key = IdempotencyKey::try_new("sig-idem-race-001").expect("key");
    let send_sig = || {
        let client = c2.clone();
        let tid = sig_task.task_id;
        let key = sig_key.clone();
        async move {
            client
                .send_task_signal(
                    tid,
                    &TaskSignalRequest {
                        signal_name: String::from("ci.passed"),
                        payload_text: String::from("build=green-42"),
                        idempotency_key: key,
                    },
                    None,
                )
                .await
                .expect("send signal")
        }
    };
    let (s1, s2, s3, s4) = tokio::join!(send_sig(), send_sig(), send_sig(), send_sig());
    let fresh_count = [&s1, &s2, &s3, &s4]
        .iter()
        .filter(|r| !r.deduplicated)
        .count();
    let dedup_count = [&s1, &s2, &s3, &s4]
        .iter()
        .filter(|r| r.deduplicated)
        .count();
    assert_eq!(
        fresh_count, 1,
        "exactly one concurrent signal caller must be fresh"
    );
    assert_eq!(
        dedup_count, 3,
        "other 3 concurrent signal callers must be deduplicated"
    );

    let sig_done = wait_for_task_status(&c2, sig_task.task_id, &[TaskStatus::Succeeded], 10).await;
    assert_eq!(
        sig_done.runs.len(),
        1,
        "duplicate concurrent signals must execute the task EXACTLY ONCE"
    );

    let mismatch_err = c2
        .send_task_signal(
            sig_task.task_id,
            &TaskSignalRequest {
                signal_name: String::from("ci.passed"),
                payload_text: String::from("TAMPERED_PAYLOAD"),
                idempotency_key: sig_key,
            },
            None,
        )
        .await;
    assert!(
        matches!(mismatch_err, Err(DennettError::IdempotencyMismatch { .. })),
        "reusing signal idempotency_key with different payload must return 409 IdempotencyMismatch, got: {mismatch_err:?}"
    );

    head2.shutdown().await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "verifies live generation fencing on goal update, targeted Stop isolation, delegation scope narrowing, and plain-text inter-agent coordination"
)]
async fn d2_goal_update_generation_fence_targeted_stop_delegation_and_agent_messages() {
    let ctrl_pg = require_control_pg_url();
    let (profile, _tmp) = TestProfile::create_on_disk("d2-stop-fence").expect("temp profile");
    let scope_a = unique_scope("d2-scope-a");
    let scope_b = unique_scope("d2-scope-b");

    let head = RunningHead::spawn_simulated(&ctrl_pg, Some(profile.data_dir()), None).await;
    let client = head.client();

    let proj = client
        .create_project(
            &CreateProjectRequest {
                name: String::from("Stop & Fence Project"),
                workspace_path: Some(profile.workspace_dir().to_string_lossy().into_owned()),
                memory_scope: Some(scope_a.clone()),
            },
            None,
        )
        .await
        .expect("project");

    let lead_agent = client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Lead Agent"),
                description: String::from("Scoped to docs/"),
                system_prompt: String::from("Coordinate safely."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope_a.clone()]),
                workspace_write_policy: WorkspaceWritePolicy::AllowPaths {
                    allowed_prefixes: vec![String::from("docs/")],
                },
                default_budget: Some(TaskBudget {
                    max_turns: Some(3),
                    max_tokens: Some(10_000),
                    max_wall_secs: Some(120),
                    exceed_policy: BudgetExceedPolicy::AskApproval,
                }),
                attached_skills: vec![String::from("research")],
                preferred_connection_id: None,
                autonomy_mode: None,
            },
            None,
        )
        .await
        .expect("lead agent");

    // 1. Live goal update during running gen-1 -> gen-1 fenced as `Superseded`, gen-2 succeeds!
    let fenced_task = client
        .create_task(
            &CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: lead_agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Goal Update Task")),
                goal: String::from("[SIM:SLOW:8000] STALE_GOAL_V1"),
                schedule: None,
                wait_for_signal: None,
                budget: None,
                attached_skills: Vec::new(),
                auth_profile_override: None,
                execute_immediately: Some(true),
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
            },
            None,
        )
        .await
        .expect("create fenced task");
    let _ = wait_for_task_status(&client, fenced_task.task_id, &[TaskStatus::Running], 10).await;

    let bumped = client
        .update_task(
            fenced_task.task_id,
            &UpdateTaskRequest {
                title: Some(String::from("Goal Update Task v2")),
                goal: Some(String::from("FRESH_GOAL_V2_OK")),
                schedule: None,
                clear_schedule: false,
                budget: None,
                execute_immediately: Some(true),
            },
            None,
        )
        .await
        .expect("update task goal");
    assert_eq!(bumped.generation, 2);

    let fenced_done =
        wait_for_task_status(&client, fenced_task.task_id, &[TaskStatus::Succeeded], 15).await;
    assert_eq!(fenced_done.task.generation, 2);
    assert_eq!(fenced_done.task.goal, "FRESH_GOAL_V2_OK");
    assert!(
        fenced_done
            .runs
            .iter()
            .any(|r| r.task_generation == Some(1) && r.status == RunStatus::Superseded),
        "gen-1 run must be marked Superseded after goal update: {:?}",
        fenced_done.runs
    );
    assert!(
        fenced_done
            .runs
            .iter()
            .any(|r| r.task_generation == Some(2) && r.status == RunStatus::Succeeded),
        "gen-2 run must finish Succeeded: {:?}",
        fenced_done.runs
    );

    // 2. Targeted `Stop`: halt Task A while sibling Task B runs concurrently -> Task B MUST succeed!
    let task_a = client
        .create_task(
            &CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: lead_agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Task A To Stop")),
                goal: String::from("[SIM:SLOW:15000] STOP_ME"),
                schedule: None,
                wait_for_signal: None,
                budget: None,
                attached_skills: Vec::new(),
                auth_profile_override: None,
                execute_immediately: Some(true),
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
            },
            None,
        )
        .await
        .expect("task A");
    let task_b = client
        .create_task(
            &CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: lead_agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Sibling Task B")),
                goal: String::from("[SIM:SLOW:700] SIBLING_SURVIVES"),
                schedule: None,
                wait_for_signal: None,
                budget: None,
                attached_skills: Vec::new(),
                auth_profile_override: None,
                execute_immediately: Some(true),
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
            },
            None,
        )
        .await
        .expect("task B");

    let _ = wait_for_task_status(&client, task_a.task_id, &[TaskStatus::Running], 10).await;
    let _ = wait_for_task_status(&client, task_b.task_id, &[TaskStatus::Running], 10).await;

    let cancel_a = client
        .cancel_task(task_a.task_id, None)
        .await
        .expect("cancel task A");
    assert_eq!(cancel_a.task.status, TaskStatus::Cancelled);
    assert!(!cancel_a.external_outcome_unknown);

    let done_b = wait_for_task_status(&client, task_b.task_id, &[TaskStatus::Succeeded], 10).await;
    assert_eq!(
        done_b.task.status,
        TaskStatus::Succeeded,
        "Stop on Task A MUST NOT harm sibling Task B!"
    );

    // 3. Delegation scope narrowing (`child_scope ⊆ parent_scope`) + plain-text agent messages!
    let broader_agent = client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Too Broad Helper"),
                description: String::from("Has broader workspace/memory rights"),
                system_prompt: String::from("Helper"),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope_a.clone(), scope_b]),
                workspace_write_policy: WorkspaceWritePolicy::AllowWorkspace,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
                autonomy_mode: None,
            },
            None,
        )
        .await
        .expect("broader agent");

    let narrower_helper = client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Scoped Helper"),
                description: String::from("Subset of lead scope"),
                system_prompt: String::from("Helper"),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope_a]),
                workspace_write_policy: WorkspaceWritePolicy::AllowPaths {
                    allowed_prefixes: vec![String::from("docs/sub/")],
                },
                default_budget: None,
                attached_skills: vec![String::from("research")],
                preferred_connection_id: None,
                autonomy_mode: None,
            },
            None,
        )
        .await
        .expect("narrower helper");

    let parent_task = client
        .create_task(
            &CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: lead_agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Parent Task")),
                goal: String::from("Parent coordinator"),
                schedule: None,
                wait_for_signal: Some(String::from("parent.done")),
                budget: Some(TaskBudget {
                    max_turns: Some(2),
                    max_tokens: Some(5000),
                    max_wall_secs: Some(60),
                    exceed_policy: BudgetExceedPolicy::AskApproval,
                }),
                attached_skills: Vec::new(),
                auth_profile_override: None,
                execute_immediately: Some(false),
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
            },
            None,
        )
        .await
        .expect("parent task");

    // Escalation attempt (`AllowWorkspace` + extra scope) MUST fail closed with 403 ForbiddenScope!
    let escalation_err = client
        .create_task(
            &CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: broader_agent.agent_id,
                session_id: None,
                parent_task_id: Some(parent_task.task_id),
                title: Some(String::from("Escalated Child")),
                goal: String::from("Should be denied"),
                schedule: None,
                wait_for_signal: None,
                budget: None,
                attached_skills: Vec::new(),
                auth_profile_override: None,
                execute_immediately: Some(false),
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
            },
            None,
        )
        .await;
    assert!(
        matches!(escalation_err, Err(DennettError::ForbiddenScope { .. })),
        "delegation to broader helper MUST fail with 403 ForbiddenScope, got: {escalation_err:?}"
    );

    // Narrower helper succeeds with budget monotonically clamped `<= parent.budget`!
    let child_task = client
        .create_task(
            &CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: narrower_helper.agent_id,
                session_id: None,
                parent_task_id: Some(parent_task.task_id),
                title: Some(String::from("Delegated Helper Task")),
                goal: String::from("Summarize helper findings"),
                schedule: None,
                wait_for_signal: Some(String::from("helper.go")),
                budget: Some(TaskBudget {
                    max_turns: Some(50),
                    max_tokens: Some(999_999),
                    max_wall_secs: Some(999),
                    exceed_policy: BudgetExceedPolicy::AskApproval,
                }),
                attached_skills: Vec::new(),
                auth_profile_override: None,
                execute_immediately: Some(false),
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
            },
            None,
        )
        .await
        .expect("delegated child task");
    assert_eq!(child_task.budget.max_turns, Some(2));
    assert_eq!(child_task.budget.max_tokens, Some(5000));
    assert_eq!(child_task.budget.max_wall_secs, Some(60));

    // Stopping parent task MUST NOT automatically kill delegated child task (zero consumer DAG!).
    let _ = client
        .cancel_task(parent_task.task_id, None)
        .await
        .expect("cancel parent");
    let child_still_waiting = client
        .get_task_detail(child_task.task_id, None)
        .await
        .expect("child detail");
    assert_eq!(child_still_waiting.task.status, TaskStatus::WaitingSignal);

    // Coordinate via plain-text `POST /v1/agents/{id}/messages` (idempotent!) and verify delivery!
    let msg_key = IdempotencyKey::try_new("agent-msg-coord-1").expect("key");
    let coord_msg = client
        .send_agent_message(
            narrower_helper.agent_id,
            &SendAgentMessageRequest {
                sender_agent_id: Some(lead_agent.agent_id),
                project_id: proj.project_id,
                related_task_id: Some(child_task.task_id),
                idempotency_key: msg_key.clone(),
                body_text: String::from("COORD_TOKEN_WRAP_UP_987"),
            },
            None,
        )
        .await
        .expect("send agent message");
    assert!(!coord_msg.deduplicated);
    let coord_dup = client
        .send_agent_message(
            narrower_helper.agent_id,
            &SendAgentMessageRequest {
                sender_agent_id: Some(lead_agent.agent_id),
                project_id: proj.project_id,
                related_task_id: Some(child_task.task_id),
                idempotency_key: msg_key,
                body_text: String::from("COORD_TOKEN_WRAP_UP_987"),
            },
            None,
        )
        .await
        .expect("dup agent message");
    assert!(coord_dup.deduplicated);

    // Trigger helper task -> plain-text message is delivered into its turn context and marked delivered!
    let _ = client
        .send_task_signal(
            child_task.task_id,
            &TaskSignalRequest {
                signal_name: String::from("helper.go"),
                payload_text: String::from("proceed"),
                idempotency_key: IdempotencyKey::try_new("helper-go-1").expect("key"),
            },
            None,
        )
        .await
        .expect("signal helper");
    let child_done =
        wait_for_task_status(&client, child_task.task_id, &[TaskStatus::Succeeded], 10).await;
    assert!(
        child_done
            .task
            .partial_result_summary
            .as_deref()
            .is_some_and(|s| s.contains("COORD_TOKEN_WRAP_UP_987")),
        "plain-text inter-agent message must be injected into helper turn context: {:?}",
        child_done.task.partial_result_summary
    );
    let undelivered = client
        .list_agent_messages(narrower_helper.agent_id, true, None)
        .await
        .expect("undelivered");
    assert_eq!(undelivered.messages.len(), 0);

    head.shutdown().await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "verifies live interactive ask_before_write pause, approve_project grant issuance, standing grant reuse, instant revocation, and Stop withdrawal"
)]
async fn d2_interactive_ask_before_write_approvals_revocable_grants_and_stop_withdrawal() {
    let ctrl_pg = require_control_pg_url();
    let (profile, _tmp) = TestProfile::create_on_disk("d2-approvals").expect("temp profile");
    let scope = unique_scope("d2-approvals");

    let head = RunningHead::spawn_simulated(&ctrl_pg, Some(profile.data_dir()), None).await;
    let client = head.client();

    let proj = client
        .create_project(
            &CreateProjectRequest {
                name: String::from("Interactive Approvals Project"),
                workspace_path: Some(profile.workspace_dir().to_string_lossy().into_owned()),
                memory_scope: Some(scope.clone()),
            },
            None,
        )
        .await
        .expect("project");
    let agent = client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Ask-Before-Write Agent"),
                description: String::from("Requires interactive approval"),
                system_prompt: String::from("Ask before writing."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope]),
                workspace_write_policy: WorkspaceWritePolicy::AskBeforeWrite,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
                autonomy_mode: None,
            },
            None,
        )
        .await
        .expect("agent");
    let session = client
        .create_session(
            &CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: Some(proj.project_id),
                title: String::from("Approval Chat"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("session");

    // 1. First risky external command pauses in `pending` -> MUST NOT execute before approval!
    let c_turn1 = client.clone();
    let sid = session.session_id;
    let turn1_handle = tokio::spawn(async move {
        c_turn1
            .send_message(
                sid,
                &SendMessageRequest {
                    content: String::from("[SIM:TOOL:run_command:git push origin branch-a]"),
                    timeout_ms: Some(30_000),
                    client_message_id: None,
                },
                None,
            )
            .await
    });

    let perm1 = {
        let deadline = time::Instant::now() + Duration::from_secs(10);
        loop {
            let list = client
                .list_permission_requests(Some("pending"), Some(session.session_id), None)
                .await
                .expect("list pending perms");
            if let Some(p) = list.requests.first().cloned() {
                break p;
            }
            assert!(
                time::Instant::now() < deadline,
                "timed out waiting for pending permission_request"
            );
            time::sleep(Duration::from_millis(50)).await;
        }
    };
    assert_eq!(perm1.status, PermissionRequestStatus::Pending);

    // Wrong optimistic revision -> 409 Conflict!
    let stale_perm_err = client
        .resolve_permission_request(
            perm1.request_id,
            &ResolvePermissionRequest {
                expected_revision: Some(perm1.revision + 5),
                decision: PermissionResolutionDecision::ApproveProject,
                reason: None,
                max_uses: None,
                expires_in_secs: None,
            },
            None,
        )
        .await;
    assert!(matches!(stale_perm_err, Err(DennettError::Conflict { .. })));

    // Approve with `ApproveProject` -> resumes turn, executes once, issues revocable grant!
    let resolved1 = client
        .resolve_permission_request(
            perm1.request_id,
            &ResolvePermissionRequest {
                expected_revision: Some(perm1.revision),
                decision: PermissionResolutionDecision::ApproveProject,
                reason: Some(String::from("allow external git push in this project")),
                max_uses: Some(5),
                expires_in_secs: Some(600),
            },
            None,
        )
        .await
        .expect("resolve approve_project");
    let grant = resolved1
        .issued_grant
        .expect("ApproveProject must issue a revocable CapabilityGrantRecord");
    assert_eq!(
        grant.uses_consumed, 1,
        "initial approved command consumes 1 of 5 uses on the newly issued grant"
    );

    let turn1_resp = turn1_handle
        .await
        .expect("join turn1")
        .expect("turn1 succeeded");
    assert_eq!(turn1_resp.run.status, RunStatus::Succeeded);
    assert!(
        turn1_resp
            .assistant_message
            .as_ref()
            .is_some_and(|m| m.content.contains("ALLOWED") && m.content.contains("branch-a")),
        "approved command must run cleanly: {:?}",
        turn1_resp.assistant_message
    );

    // 2. Second risky command succeeds IMMEDIATELY via the standing grant (zero new pending approval)!
    let turn2_resp = client
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: String::from("[SIM:TOOL:run_command:git push origin branch-b]"),
                timeout_ms: Some(15_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("turn2 via grant");
    assert_eq!(turn2_resp.run.status, RunStatus::Succeeded);
    assert!(
        turn2_resp
            .assistant_message
            .as_ref()
            .is_some_and(|m| m.content.contains("ALLOWED") && m.content.contains("branch-b")),
        "second command via standing grant must succeed: {:?}",
        turn2_resp.assistant_message
    );

    let grants_after_use = client
        .list_capability_grants(Some(proj.project_id), false, None)
        .await
        .expect("list grants");
    let used_grant = grants_after_use
        .grants
        .iter()
        .find(|g| g.grant_id == grant.grant_id)
        .expect("grant present");
    assert_eq!(
        used_grant.uses_consumed, 2,
        "second command via standing grant increments uses_consumed from 1 to 2"
    );

    // 3. Revoke grant -> third risky command MUST pause for approval again, and Stop cancels it cleanly!
    let revoked = client
        .revoke_capability_grant(
            grant.grant_id,
            &RevokeGrantRequest {
                reason: Some(String::from("revoking external command permission")),
            },
            None,
        )
        .await
        .expect("revoke grant");
    assert!(revoked.revoked_at.is_some());

    let c_turn3 = client.clone();
    let turn3_handle = tokio::spawn(async move {
        c_turn3
            .send_message(
                sid,
                &SendMessageRequest {
                    content: String::from("[SIM:TOOL:run_command:git push origin branch-c]"),
                    timeout_ms: Some(20_000),
                    client_message_id: None,
                },
                None,
            )
            .await
    });

    let perm3 = {
        let deadline = time::Instant::now() + Duration::from_secs(10);
        loop {
            let list = client
                .list_permission_requests(Some("pending"), Some(session.session_id), None)
                .await
                .expect("poll pending perm3");
            if let Some(p) = list.requests.first().cloned() {
                break p;
            }
            assert!(
                time::Instant::now() < deadline,
                "revoked grant MUST cause third risky command to pause for interactive approval!"
            );
            time::sleep(Duration::from_millis(50)).await;
        }
    };

    // Stop cancels waiting turn and withdraws pending permission_request!
    let cancel_resp = client
        .cancel_run(session.session_id, None)
        .await
        .expect("cancel during approval wait");
    assert!(cancel_resp.cancelled);

    let turn3_resp = turn3_handle
        .await
        .expect("join turn3")
        .expect("cancelled turn3");
    assert_eq!(turn3_resp.run.status, RunStatus::Cancelled);

    // Late resolve after Stop MUST fail closed with 409 Conflict!
    let late_resolve = client
        .resolve_permission_request(
            perm3.request_id,
            &ResolvePermissionRequest {
                expected_revision: Some(perm3.revision),
                decision: PermissionResolutionDecision::ApproveOnce,
                reason: None,
                max_uses: None,
                expires_in_secs: None,
            },
            None,
        )
        .await;
    assert!(
        matches!(late_resolve, Err(DennettError::Conflict { .. })),
        "late approval after Stop must fail with 409 Conflict, got: {late_resolve:?}"
    );

    head.shutdown().await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "proves LostAck -> UNKNOWN persists across Head restart, refuses blind retries, reports honest Stop external_outcome_unknown, and reconciles with provider_execution_count == 1"
)]
async fn d2_lost_ack_unknown_external_effect_survives_restart_honest_stop_and_reconcile() {
    let ctrl_pg = require_control_pg_url();
    let (profile, _tmp) = TestProfile::create_on_disk("d2-lost-ack").expect("temp profile");
    let ledger_path = profile.data_dir().join("shared-effects-ledger.json");
    let scope = unique_scope("d2-effects");

    let head1 =
        RunningHead::spawn_simulated(&ctrl_pg, Some(profile.data_dir()), Some(&ledger_path)).await;
    let c1 = head1.client();

    let proj = c1
        .create_project(
            &CreateProjectRequest {
                name: String::from("Effects LostAck Project"),
                workspace_path: None,
                memory_scope: Some(scope.clone()),
            },
            None,
        )
        .await
        .expect("project");
    let agent = c1
        .create_agent(
            &CreateAgentRequest {
                name: String::from("External Effect Agent"),
                description: String::from("Tests Contract B LostAck"),
                system_prompt: String::from("Be careful with external side effects."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope]),
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
                autonomy_mode: None,
            },
            None,
        )
        .await
        .expect("agent");
    let task = c1
        .create_task(
            &CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Send External Webhook")),
                goal: String::from("Notify partner system"),
                schedule: None,
                wait_for_signal: Some(String::from("manual.wait")),
                budget: None,
                attached_skills: Vec::new(),
                auth_profile_override: None,
                execute_immediately: Some(false),
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
            },
            None,
        )
        .await
        .expect("task");

    let idem_key =
        IdempotencyKey::try_new(format!("eff-lost-ack-{}", Uuid::new_v4().simple())).expect("key");
    let dispatch_req = DispatchEffectRequest {
        project_id: proj.project_id,
        session_id: task.session_id,
        task_id: Some(task.task_id),
        run_id: None,
        agent_id: agent.agent_id,
        effect_idempotency_key: idem_key.clone(),
        connector_id: None,
        operation: String::from("send_webhook"),
        target_recipient: String::from("https://partner.example/hook"),
        parameters_json: String::from(r#"{"invoice": 42}"#),
        simulated_behavior: Some(SimEffectBehavior::LostAck),
    };

    // 0. D2-ARCH-03: Without an explicit Allow policy rule, external effect dispatch MUST fail closed!
    let denied_first = c1.dispatch_effect(&dispatch_req, None).await;
    assert!(
        matches!(denied_first, Err(DennettError::AccessDenied { .. })),
        "external effect without Allow rule MUST fail closed with AccessDenied, got: {denied_first:?}"
    );
    let _allow_rule = c1
        .create_policy_rule(
            &CreatePolicyRuleRequest {
                scope: format!("project:{}", proj.project_id),
                tool_name: String::from("external_effect:send_webhook"),
                target_pattern: Some(String::from("https://partner.example/*")),
                effect: PolicyEffect::Allow,
                reason: String::from("Allow partner webhook for LostAck reconciliation test"),
            },
            None,
        )
        .await
        .expect("create allow policy rule for external_effect:send_webhook");

    // 1. Dispatch with LostAck -> persisted as `Unknown`, task -> `UnknownExternal`, Inbox card opened!
    let first = c1
        .dispatch_effect(&dispatch_req, None)
        .await
        .expect("dispatch LostAck");
    assert_eq!(first.effect.status, ExternalEffectStatus::Unknown);
    assert!(first.external_outcome_unknown);
    assert!(!first.deduplicated);

    let task_after_lost = c1
        .get_task_detail(task.task_id, None)
        .await
        .expect("task detail");
    assert_eq!(task_after_lost.task.status, TaskStatus::UnknownExternal);

    let inbox_unknown = c1
        .list_inbox_cards(Some("open"), Some(proj.project_id), None)
        .await
        .expect("inbox");
    assert!(
        inbox_unknown
            .cards
            .iter()
            .any(|c| c.kind == InboxCardKind::UnknownEffect
                && c.effect_id == Some(first.effect.effect_id)),
        "LostAck must create an open unknown_effect Inbox card"
    );

    // 2. Retrying SAME key before restart MUST refuse blind re-send!
    let retry_before_restart = c1
        .dispatch_effect(&dispatch_req, None)
        .await
        .expect("retry before restart");
    assert!(retry_before_restart.deduplicated);
    assert!(retry_before_restart.external_outcome_unknown);
    assert_eq!(
        retry_before_restart.effect.effect_id,
        first.effect.effect_id
    );

    // 3. Full `dennett-head` restart with SAME PG + SAME `--effects-ledger`!
    head1.shutdown().await;
    let head2 =
        RunningHead::spawn_simulated(&ctrl_pg, Some(profile.data_dir()), Some(&ledger_path)).await;
    let c2 = head2.client();

    // Retry AFTER full restart MUST STILL refuse blind re-send!
    let retry_after_restart = c2
        .dispatch_effect(&dispatch_req, None)
        .await
        .expect("retry after restart");
    assert!(retry_after_restart.deduplicated);
    assert!(retry_after_restart.external_outcome_unknown);

    // 4. Honest `Stop` reports `external_outcome_unknown = true` (NEVER claiming external cancel)!
    let stop_resp = c2
        .cancel_task(task.task_id, None)
        .await
        .expect("honest Stop on task with unknown external effect");
    assert!(
        stop_resp.external_outcome_unknown,
        "Stop MUST explicitly report external_outcome_unknown=true when an external effect is Unknown!"
    );
    assert_eq!(stop_resp.unknown_effects.len(), 1);

    // 5. Explicit `/reconcile` queries provider ledger WITHOUT re-sending -> `provider_execution_count == 1`!
    let reconciled = c2
        .reconcile_effect(
            first.effect.effect_id,
            &ReconcileEffectRequest {
                manual_resolution: None,
                reason: Some(String::from("reconciling after restart")),
            },
            None,
        )
        .await
        .expect("reconcile effect");
    assert!(reconciled.provider_found);
    assert_eq!(
        reconciled.provider_execution_count, 1,
        "provider MUST have executed the effect EXACTLY ONCE despite retries and full daemon restart!"
    );
    assert_eq!(reconciled.effect.status, ExternalEffectStatus::Confirmed);
    assert!(reconciled.effect.reconciled_at.is_some());

    head2.shutdown().await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "verifies native clarification, external budgets, client_message_id deduplication, Contracts C & D lifecycle separation with disk preservation, artifact revisions, and dennettctl D2 CLI parity"
)]
async fn d2_clarification_budgets_client_idempotency_strict_lifecycle_and_dennettctl_parity() {
    let ctrl_pg = require_control_pg_url();
    let (profile, _tmp) = TestProfile::create_on_disk("d2-lifecycle").expect("temp profile");
    let ws1 = profile.workspace_dir().join("user-repo-1");
    let ws2 = profile.workspace_dir().join("user-repo-2");
    fs::create_dir_all(&ws1).expect("create ws1");
    fs::create_dir_all(&ws2).expect("create ws2");
    let precious_file = ws1.join("important_user_file.txt");
    fs::write(&precious_file, "NEVER_DELETE_USER_FILES_42").expect("write precious file");
    let ws2_file = ws2.join("artifact_ref.txt");
    fs::write(&ws2_file, "WORKSPACE_ARTIFACT_IMMORTAL").expect("write ws2 file");

    let scope = unique_scope("d2-life");
    let head = RunningHead::spawn_simulated(&ctrl_pg, Some(profile.data_dir()), None).await;
    let client = head.client();

    let proj = client
        .create_project(
            &CreateProjectRequest {
                name: String::from("Lifecycle Project"),
                workspace_path: Some(ws1.to_string_lossy().into_owned()),
                memory_scope: Some(scope.clone()),
            },
            None,
        )
        .await
        .expect("project");
    let agent = client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Lifecycle Agent"),
                description: String::from("Tests clarification, budgets, idempotency, lifecycle"),
                system_prompt: String::from("Be precise."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope]),
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
                autonomy_mode: None,
            },
            None,
        )
        .await
        .expect("agent");

    // 1. Native Clarification (`[SIM:CLARIFY:...] -> waiting_clarification -> /reply -> succeeded`)
    let clarify_task = client
        .create_task(
            &CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Clarification Task")),
                goal: String::from("[SIM:CLARIFY:Which deployment region?]"),
                schedule: None,
                wait_for_signal: None,
                budget: None,
                attached_skills: Vec::new(),
                auth_profile_override: None,
                execute_immediately: Some(true),
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
            },
            None,
        )
        .await
        .expect("create clarify task");
    let waiting_clar = wait_for_task_status(
        &client,
        clarify_task.task_id,
        &[TaskStatus::WaitingClarification],
        10,
    )
    .await;
    assert!(
        waiting_clar
            .open_inbox_cards
            .iter()
            .any(|c| c.kind == InboxCardKind::Clarification && c.status == InboxCardStatus::Open)
    );
    let _replied = client
        .reply_to_task(
            clarify_task.task_id,
            &ReplyTaskRequest {
                answer_text: String::from("Use region eu-west-1"),
            },
            None,
        )
        .await
        .expect("reply to clarification");
    let clarified_done =
        wait_for_task_status(&client, clarify_task.task_id, &[TaskStatus::Succeeded], 10).await;
    assert!(
        clarified_done
            .task
            .partial_result_summary
            .as_deref()
            .is_some_and(|s| s.contains("eu-west-1")),
        "clarification answer must reach resumed turn: {:?}",
        clarified_done.task.partial_result_summary
    );

    // 2. External `TaskBudget` enforcement (`AskApproval` + `IncreaseBudget` AND `Fail` policy!)
    let budget_task = client
        .create_task(
            &CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Token Budget Task")),
                goal: String::from("[SIM:TOKENS:250] Heavy analysis"),
                schedule: None,
                wait_for_signal: None,
                budget: Some(TaskBudget {
                    max_turns: Some(5),
                    max_tokens: Some(100),
                    max_wall_secs: Some(60),
                    exceed_policy: BudgetExceedPolicy::AskApproval,
                }),
                attached_skills: Vec::new(),
                auth_profile_override: None,
                execute_immediately: Some(true),
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
            },
            None,
        )
        .await
        .expect("budget task");
    let waiting_budget = wait_for_task_status(
        &client,
        budget_task.task_id,
        &[TaskStatus::WaitingApproval],
        10,
    )
    .await;
    assert_eq!(
        waiting_budget.task.error_code.as_deref(),
        Some("BUDGET_EXCEEDED")
    );
    let budget_card = waiting_budget
        .open_inbox_cards
        .iter()
        .find(|c| c.kind == InboxCardKind::BudgetExceeded && c.status == InboxCardStatus::Open)
        .expect("budget_exceeded inbox card");
    let _increased = client
        .resolve_inbox_card(
            budget_card.card_id,
            &ResolveInboxCardRequest {
                expected_revision: budget_card.revision,
                resolution: InboxResolutionAction::IncreaseBudget {
                    additional_turns: None,
                    additional_tokens: Some(1000),
                    additional_wall_secs: None,
                },
            },
            None,
        )
        .await
        .expect("increase budget");
    let budget_done =
        wait_for_task_status(&client, budget_task.task_id, &[TaskStatus::Succeeded], 10).await;
    assert_eq!(budget_done.task.budget.max_tokens, Some(1100));

    let fail_budget_task = client
        .create_task(
            &CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Hard Fail Budget Task")),
                goal: String::from("[SIM:TOKENS:500] Exceed immediately"),
                schedule: None,
                wait_for_signal: None,
                budget: Some(TaskBudget {
                    max_turns: Some(5),
                    max_tokens: Some(50),
                    max_wall_secs: Some(60),
                    exceed_policy: BudgetExceedPolicy::Fail,
                }),
                attached_skills: Vec::new(),
                auth_profile_override: None,
                execute_immediately: Some(true),
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
            },
            None,
        )
        .await
        .expect("fail budget task");
    let failed_budget =
        wait_for_task_status(&client, fail_budget_task.task_id, &[TaskStatus::Failed], 10).await;
    assert_eq!(
        failed_budget.task.error_code.as_deref(),
        Some("BUDGET_EXCEEDED")
    );

    // 3. `client_message_id` deduplication under network drop/retry + 409 on payload mismatch!
    let session = client
        .create_session(
            &CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: Some(proj.project_id),
                title: String::from("Idempotency Session"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("session");
    let cmid = format!("client-retry-{}", Uuid::new_v4().simple());
    let m1 = client
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: String::from("Send once cleanly"),
                timeout_ms: None,
                client_message_id: Some(cmid.clone()),
            },
            None,
        )
        .await
        .expect("first send");
    assert!(!m1.deduplicated);

    let m2 = client
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: String::from("Send once cleanly"),
                timeout_ms: None,
                client_message_id: Some(cmid.clone()),
            },
            None,
        )
        .await
        .expect("deduplicated retry");
    assert!(m2.deduplicated);
    assert_eq!(m2.user_message.message_id, m1.user_message.message_id);
    assert_eq!(m2.run.run_id, m1.run.run_id);

    let m_conflict = client
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: String::from("DIFFERENT CONTENT ON SAME ID"),
                timeout_ms: None,
                client_message_id: Some(cmid),
            },
            None,
        )
        .await;
    assert!(
        matches!(m_conflict, Err(DennettError::IdempotencyMismatch { .. })),
        "client_message_id with different content must return 409 IdempotencyMismatch, got: {m_conflict:?}"
    );

    // 4. Artifact `partial -> final` revisions + Contract D lifecycle (user file untouched!).
    let rebind_for_art = client
        .rebind_project(
            proj.project_id,
            &RebindProjectRequest {
                workspace_path: ws2.to_string_lossy().into_owned(),
                reason: Some(String::from("rebind to ws2")),
            },
            None,
        )
        .await
        .expect("rebind project to ws2");
    assert!(!rebind_for_art.user_workspace_files_deleted);
    assert!(
        precious_file.exists(),
        "rebind must never delete old workspace files"
    );

    let rev1 = client
        .create_artifact(
            session.session_id,
            &CreateArtifactRequest {
                run_id: Some(m1.run.run_id),
                name: String::from("analysis.md"),
                mime_type: String::from("text/markdown"),
                content: Some(String::from("draft partial")),
                relative_path: None,
                completeness_status: ArtifactCompleteness::Partial,
            },
            None,
        )
        .await
        .expect("create partial rev1");
    assert_eq!(rev1.completeness_status, ArtifactCompleteness::Partial);

    let rev2 = client
        .create_artifact(
            session.session_id,
            &CreateArtifactRequest {
                run_id: Some(m1.run.run_id),
                name: String::from("analysis.md"),
                mime_type: String::from("text/markdown"),
                content: Some(String::from("final complete")),
                relative_path: None,
                completeness_status: ArtifactCompleteness::Final,
            },
            None,
        )
        .await
        .expect("create final rev2");
    assert_eq!(rev2.revision, 2);
    assert_eq!(rev2.completeness_status, ArtifactCompleteness::Final);

    let revision_list = client
        .list_artifact_revisions(session.session_id, "analysis.md", None)
        .await
        .expect("list revisions");
    assert_eq!(revision_list.artifacts.len(), 2);
    assert_eq!(revision_list.artifacts[0].revision, 1);
    assert_eq!(revision_list.artifacts[1].revision, 2);

    let finalized_rev1 = client
        .apply_artifact_lifecycle(
            session.session_id,
            rev1.artifact_id,
            &ArtifactLifecycleRequest {
                action: ArtifactLifecycleAction::Finalize,
                reason: Some(String::from("promote draft")),
            },
            None,
        )
        .await
        .expect("finalize rev1");
    assert_eq!(
        finalized_rev1.artifact.completeness_status,
        ArtifactCompleteness::Final
    );

    let ws_art = client
        .create_artifact(
            session.session_id,
            &CreateArtifactRequest {
                run_id: None,
                name: String::from("ws-ref"),
                mime_type: String::from("text/plain"),
                content: None,
                relative_path: Some(String::from("artifact_ref.txt")),
                completeness_status: ArtifactCompleteness::Final,
            },
            None,
        )
        .await
        .expect("workspace-backed artifact");
    let deleted_art = client
        .apply_artifact_lifecycle(
            session.session_id,
            ws_art.artifact_id,
            &ArtifactLifecycleRequest {
                action: ArtifactLifecycleAction::Delete,
                reason: Some(String::from("delete artifact record only")),
            },
            None,
        )
        .await
        .expect("delete artifact record");
    assert!(!deleted_art.user_workspace_files_deleted);
    assert_eq!(
        fs::read_to_string(&ws2_file).expect("ws2_file still intact on disk"),
        "WORKSPACE_ARTIFACT_IMMORTAL"
    );

    // 5. `dennettctl` D2 CLI parity smoke against live Head!
    for cli_args in [
        &["--json", "events", "snapshot"][..],
        &["--json", "events", "tail", "--after", "0", "--limit", "10"][..],
        &["--json", "task", "list"][..],
        &["--json", "inbox", "list", "--status", "all"][..],
        &["--json", "grant", "list", "--include-revoked"][..],
        &["--json", "effect", "list"][..],
        &["--json", "project", "list", "--include-archived"][..],
    ] {
        let (code, stdout, stderr) = run_dennettctl(&head, cli_args).await;
        assert_eq!(
            code, 0,
            "dennettctl {cli_args:?} failed (code={code}): stderr={stderr}"
        );
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(stdout.trim());
        assert!(
            parsed.is_ok(),
            "dennettctl {cli_args:?} did not emit valid JSON: {stdout}"
        );
    }

    // 6. Strict Contract C Project Lifecycle Separation (`archive` -> `unarchive` -> `detach` -> `remove` -> `delete?confirm=true`),
    //    proving user workspace files on disk remain 100% UNTOUCHED at every step!
    let archived = client
        .archive_project(proj.project_id, None)
        .await
        .expect("archive project");
    assert_eq!(
        archived.project.lifecycle_state,
        ProjectLifecycleState::Archived
    );
    assert!(!archived.user_workspace_files_deleted);

    let default_list = client.list_projects(None).await.expect("list default");
    assert!(
        !default_list
            .projects
            .iter()
            .any(|p| p.project_id == proj.project_id),
        "archived project must be excluded from default project list"
    );
    let with_archived = client
        .list_projects_with_options(true, None)
        .await
        .expect("list include_archived");
    assert!(
        with_archived
            .projects
            .iter()
            .any(|p| p.project_id == proj.project_id),
        "archived project must appear when include_archived=true"
    );

    let unarchived = client
        .unarchive_project(proj.project_id, None)
        .await
        .expect("unarchive project");
    assert_eq!(
        unarchived.project.lifecycle_state,
        ProjectLifecycleState::Active
    );

    let detached = client
        .detach_project_workspace(proj.project_id, None)
        .await
        .expect("detach workspace");
    assert_eq!(detached.project.workspace_path, None);
    assert_eq!(
        detached.project.lifecycle_state,
        ProjectLifecycleState::Detached
    );

    let removed = client
        .remove_project(proj.project_id, None)
        .await
        .expect("remove project");
    assert_eq!(
        removed.project.lifecycle_state,
        ProjectLifecycleState::DeletedRecord
    );
    assert!(!removed.user_workspace_files_deleted);

    // Delete without `confirm=true` MUST fail closed!
    let no_confirm_err = client.delete_project(proj.project_id, false, None).await;
    assert!(matches!(
        no_confirm_err,
        Err(DennettError::InvalidInput { .. })
    ));

    let deleted = client
        .delete_project(proj.project_id, true, None)
        .await
        .expect("delete project with confirm=true");
    assert_eq!(
        deleted.project.lifecycle_state,
        ProjectLifecycleState::DeletedRecord
    );
    assert!(!deleted.user_workspace_files_deleted);

    // CRITICAL SAFETY INVARIANT: BOTH external user directories & files are 100% untouched on disk!
    assert_eq!(
        fs::read_to_string(&precious_file).expect("precious file must survive project delete"),
        "NEVER_DELETE_USER_FILES_42"
    );
    assert_eq!(
        fs::read_to_string(&ws2_file).expect("ws2 file must survive project delete"),
        "WORKSPACE_ARTIFACT_IMMORTAL"
    );

    head.shutdown().await;
}
