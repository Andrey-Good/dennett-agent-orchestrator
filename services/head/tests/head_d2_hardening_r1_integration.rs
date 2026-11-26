//! Comprehensive real PostgreSQL 16 integration tests for D2 Hardening Pass (Slice R1):
//! - `D2-ARCH-02`: `archive_project` / `unarchive_project`, `pause_reason`, periodic mid-run safety;
//! - `D2-ARCH-03`: `dispatch_external_effect` `PolicyEngine` fail-closed, `StopTask` guard on
//!   `UnknownEffect`, `remove`/`delete` & `finalize_turn` `UNKNOWN` preservation, concurrent `reconcile`;
//! - `D2-SEC-01` & `D2-SEC-02`: empty `allowed_memory_scopes` isolation, `commit_memory_note` policy,
//!   delegated child `parent_write_ceiling` blocking broad `CapabilityGrant`;
//! - `D2-BUG-01`..`05`: durable `reply_to_task` & `task_signals` across restart, `update_project`
//!   rebind & mid-turn archive TOCTOU, human approval wait excluded from RPC timeout & `wall_ms_used`
//!   and redacted `preview_snippet`, failed-run `agent_messages` release & atomic card revision checks
//!   and complete `control_events`, `OnceAt` 400 validation, insufficient budget increase rejection,
//!   and PG16 `WeeklyWallTime` schedule computation;
//! - `D2-OVER-01`: `v5 -> v6` forward migration from `memory_commit_outbox` into `ordered_outbox`
//!   and automatic background drain via `HeadScheduler`;
//! - Agent-Native Event MCP (`dennett-control-mcp`): real stdio binary spawned by `SimulatedRuntime`
//!   creating a durable scheduled task + `SessionKind::TaskRun` session over loopback HTTP with
//!   `X-Dennett-Control-Token` (`dct1.`) project isolation & anti-recursion guards.

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod};
use dennett_contracts::{
    ArtifactCompleteness, ArtifactLifecycleAction, ArtifactLifecycleRequest,
    ArtifactLifecycleState, BudgetExceedPolicy, CommitMemoryNoteRequest, CorrelationId,
    CreateAgentRequest, CreateArtifactRequest, CreatePolicyRuleRequest, CreateProjectRequest,
    CreateSessionRequest, CreateTaskRequest, DennettError, DispatchEffectRequest,
    ExternalEffectStatus, IdempotencyKey, InboxCardKind, InboxCardStatus, InboxResolutionAction,
    ManualEffectResolution, MemoryOutboxStatus, MemoryScope, MissedRunPolicy,
    PermissionRequestStatus, PermissionResolutionDecision, PolicyEffect, ProfileMode,
    ProjectLifecycleState, ReconcileEffectRequest, ReplyTaskRequest, ResolveInboxCardRequest,
    ResolvePermissionRequest, RunStatus, RuntimeAuthProfile, ScheduleKind, SendAgentMessageRequest,
    SendMessageRequest, SessionId, SessionKind, SessionStatus, SimEffectBehavior, TaskBudget,
    TaskSchedule, TaskSignalRequest, TaskStatus, TimerPauseReason, TimestampMs,
    UpdateProjectRequest, UpdateSessionRequest, WorkspaceWritePolicy,
};
use dennett_head::{
    CONTROL_MIGRATIONS, HeadAppState, HeadService, HeadServiceExtras, MAX_CONCURRENT_REQUESTS,
    MemoryMcpWiring, MemoryTokenSigner, ValidatedControlPgTarget, build_head_router,
    compute_next_fire_at_pg,
};
use dennett_head_client::HeadClient;
use dennett_memory_client::MemoryClient;
use dennett_runtime_antigravity::{
    AgentRuntimePort, BoxFuture, PolicyEvaluator, RuntimeDescriptor, RuntimeEvent,
    SessionStartOutcome, SessionStartRequest, SimulatedRuntime, TurnOutcome, TurnRequest,
};
use tokio::net::TcpListener;
use tokio::sync::{Barrier, Semaphore, watch};
use tokio::time;
use tokio_postgres::NoTls;
use uuid::Uuid;

fn require_control_pg_target() -> (String, ValidatedControlPgTarget) {
    assert_eq!(
        env::var("DENNETT_TEST_PROFILE").ok().as_deref(),
        Some("1"),
        "hardening integration tests must run under DENNETT_TEST_PROFILE=1"
    );
    let url = env::var("DENNETT_TEST_CONTROL_PG_URL")
        .expect("DENNETT_TEST_CONTROL_PG_URL must be set for PG16 integration tests");
    let target = ValidatedControlPgTarget::resolve_from_sources(
        None,
        Some(&url),
        None,
        ProfileMode::TestIsolated,
    )
    .expect("resolve test PG target")
    .expect("validated test PG target");
    (url, target)
}

fn with_isolated_schema_url(base_url: &str, schema: &str) -> String {
    let sep = if base_url.contains('?') { '&' } else { '?' };
    format!("{base_url}{sep}options=-csearch_path%3D{schema}")
}

async fn make_isolated_schema_service_with_extras(
    prefix: &str,
    memory_client: Option<MemoryClient>,
    extras: HeadServiceExtras,
) -> (
    String,
    ValidatedControlPgTarget,
    HeadService,
    String,
    String,
) {
    let (base_url, _) = require_control_pg_target();
    let short = &Uuid::new_v4().simple().to_string()[..12];
    let schema = format!("r1_{prefix}_{short}");

    let (client, conn) = tokio_postgres::connect(&base_url, NoTls)
        .await
        .expect("connect control PG to create isolated schema");
    tokio::spawn(async move {
        let _ = conn.await;
    });
    client
        .batch_execute(&format!("CREATE SCHEMA {schema}"))
        .await
        .expect("create isolated schema");
    drop(client);

    let iso_url = with_isolated_schema_url(&base_url, &schema);
    let iso_target = ValidatedControlPgTarget::resolve_from_sources(
        None,
        Some(&iso_url),
        None,
        ProfileMode::TestIsolated,
    )
    .expect("resolve isolated schema target")
    .expect("validated isolated schema target");

    let svc = HeadService::new_with_extras(
        iso_target.clone(),
        Arc::new(SimulatedRuntime::new(RuntimeAuthProfile::OAuthPersonal)),
        memory_client,
        None,
        ProfileMode::TestIsolated,
        vec![String::from("SUPER_SECRET_R1_TOKEN_999")],
        extras,
    )
    .expect("construct HeadService");
    let _ = svc
        .ensure_migrated()
        .await
        .expect("apply migrations v1..v6");
    (iso_url, iso_target, svc, base_url, schema)
}

async fn drop_isolated_schema(base_url: &str, schema: &str) {
    if let Ok((client, conn)) = tokio_postgres::connect(base_url, NoTls).await {
        tokio::spawn(async move {
            let _ = conn.await;
        });
        let _ = client
            .batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
            .await;
    }
}

fn unique_scope(prefix: &str) -> MemoryScope {
    let short = &Uuid::new_v4().simple().to_string()[..10];
    MemoryScope::try_new(format!("project:{prefix}-{short}")).expect("valid memory scope")
}

fn locate_control_mcp_bin() -> PathBuf {
    let exe_name = if cfg!(windows) {
        "dennett-control-mcp.exe"
    } else {
        "dennett-control-mcp"
    };
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root");
    let target_dir = env::var_os("CARGO_TARGET_DIR")
        .map_or_else(|| workspace_root.join("target"), PathBuf::from);
    let debug_candidate = target_dir.join("debug").join(exe_name);
    if debug_candidate.is_file() {
        return debug_candidate;
    }
    if let Ok(cur) = env::current_exe()
        && let Some(parent) = cur.parent()
    {
        let sibling = parent.join(exe_name);
        if sibling.is_file() {
            return sibling;
        }
        if let Some(grandparent) = parent.parent() {
            let up = grandparent.join(exe_name);
            if up.is_file() {
                return up;
            }
        }
    }
    panic!(
        "dennett-control-mcp binary not found at {}; build with `cargo build --bin dennett-control-mcp`",
        debug_candidate.display()
    );
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "verifies D2-ARCH-02 full archive/unarchive semantics, pause_reason separation, mid-run periodic task protection, and D2-BUG-02 archived mutation blocking"
)]
async fn r1_arch_02_archive_unarchive_pause_reason_and_periodic_task_mid_run() {
    let (iso_url, _target, svc, base_url, schema) =
        make_isolated_schema_service_with_extras("arch02", None, HeadServiceExtras::default())
            .await;
    let cid = CorrelationId::generate();
    let scope = unique_scope("arch02");

    let proj = svc
        .create_project(
            CreateProjectRequest {
                name: String::from("Archive Semantics Project"),
                workspace_path: None,
                memory_scope: Some(scope.clone()),
            },
            &cid,
        )
        .await
        .expect("project");
    let agent = svc
        .create_agent(
            CreateAgentRequest {
                name: String::from("Archive Agent"),
                description: String::from("Tests archive/unarchive semantics"),
                system_prompt: String::from("Work reliably."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope]),
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
                autonomy_mode: None,
            },
            &cid,
        )
        .await
        .expect("agent");

    // 1. Periodic task armed with a pending timer.
    let periodic_task = svc
        .create_task(
            CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Periodic Heartbeat")),
                goal: String::from("Check pulse"),
                schedule: Some(TaskSchedule {
                    kind: ScheduleKind::Interval { interval_secs: 60 },
                    timezone: String::from("UTC"),
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
            &cid,
        )
        .await
        .expect("periodic task");

    // 2. Overdue task with MissedRunPolicy::Ask -> pauses with `pause_reason = 'misfire_ask'`.
    let past_fire_ms = TimestampMs::now().as_millis() - 300_000;
    let misfire_task = svc
        .create_task(
            CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Misfired Ask Task")),
                goal: String::from("Check missed window"),
                schedule: Some(TaskSchedule {
                    kind: ScheduleKind::OnceAt {
                        fire_at: TimestampMs::from_millis(past_fire_ms),
                    },
                    timezone: String::from("UTC"),
                    original_expression: None,
                    missed_run_policy: MissedRunPolicy::Ask,
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
            &cid,
        )
        .await
        .expect("misfire task");
    let tick_res = svc
        .tick_scheduler_at(TimestampMs::now(), &cid)
        .await
        .expect("tick misfire");
    assert_eq!(tick_res.asked_timers, 1);

    let misfire_detail_before = svc
        .get_task_detail(misfire_task.task_id)
        .await
        .expect("misfire detail before archive");
    assert_eq!(
        misfire_detail_before.timers[0].pause_reason,
        Some(TimerPauseReason::MisfireAsk)
    );

    // 3. Start an active slow chat run in this project, then archive the project mid-run!
    let chat_session = svc
        .create_session(
            CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: Some(proj.project_id),
                title: String::from("Active Chat Before Archive"),
                session_kind: SessionKind::UserChat,
            },
            &cid,
        )
        .await
        .expect("chat session");
    let svc_bg = svc.clone();
    let cid_bg = cid.clone();
    let chat_sid = chat_session.session_id;
    let running_turn = tokio::spawn(async move {
        svc_bg
            .send_message(
                chat_sid,
                SendMessageRequest {
                    content: String::from("[SIM:SLOW:8000]"),
                    timeout_ms: Some(15_000),
                    client_message_id: None,
                },
                cid_bg,
            )
            .await
    });
    let mut entered_running = false;
    for _ in 0..40 {
        let snap = svc
            .get_state_snapshot(Some(proj.project_id), false)
            .await
            .expect("snapshot");
        if snap.running_session_ids.contains(&chat_sid) {
            entered_running = true;
            break;
        }
        time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        entered_running,
        "StateSnapshotResponse.running_session_ids must report active running session"
    );

    let archived = svc
        .archive_project(proj.project_id, &cid)
        .await
        .expect("archive_project");
    assert_eq!(
        archived.project.lifecycle_state,
        ProjectLifecycleState::Archived
    );

    let turn_res = time::timeout(Duration::from_secs(5), running_turn)
        .await
        .expect("archive must cancel active run promptly")
        .expect("join running_turn")
        .expect("cancelled turn completes cleanly");
    assert_eq!(turn_res.run.status, RunStatus::Cancelled);

    // Verify periodic timer has `pause_reason = ArchivedProject`, while misfire timer STILL has `MisfireAsk`!
    let periodic_after_archive = svc
        .get_task_detail(periodic_task.task_id)
        .await
        .expect("periodic detail");
    assert_eq!(periodic_after_archive.task.status, TaskStatus::WaitingTimer);
    assert_eq!(
        periodic_after_archive.timers[0].pause_reason,
        Some(TimerPauseReason::ArchivedProject)
    );

    let misfire_after_archive = svc
        .get_task_detail(misfire_task.task_id)
        .await
        .expect("misfire detail after archive");
    assert_eq!(
        misfire_after_archive.timers[0].pause_reason,
        Some(TimerPauseReason::MisfireAsk)
    );

    // D2-BUG-02: Archived project rejects task/project mutations!
    let mut_while_archived = svc
        .update_project(
            proj.project_id,
            UpdateProjectRequest {
                name: Some(String::from("Blocked Rename")),
                workspace_path: None,
                pinned: None,
            },
            &cid,
        )
        .await;
    assert!(matches!(
        mut_while_archived,
        Err(DennettError::Conflict { .. })
    ));

    // D2-ARCH-02: `complete_task_run` while periodic timer is `paused` NEVER marks task `Succeeded`!
    let fake_run_id = dennett_contracts::RunId::new();
    let fake_msg_id = dennett_contracts::MessageId::new();
    let (pg, conn) = tokio_postgres::connect(&iso_url, NoTls)
        .await
        .expect("connect pg");
    tokio::spawn(async move {
        let _ = conn.await;
    });
    pg.execute(
        "INSERT INTO runs (
            run_id, session_id, agent_id, agent_config_version_id,
            user_message_id, status, task_id, task_generation, correlation_id
         ) VALUES ($1, $2, $3, $4, $5, 'running', $6, 1, $7)",
        &[
            &fake_run_id.as_uuid(),
            &periodic_task.session_id.as_uuid(),
            &agent.agent_id.as_uuid(),
            &agent.current_config_version_id.as_uuid(),
            &fake_msg_id.as_uuid(),
            &periodic_task.task_id.as_uuid(),
            &cid.as_str(),
        ],
    )
    .await
    .expect("seed mid-run");
    let completed_status = svc
        .complete_task_run(
            periodic_task.task_id,
            fake_run_id,
            1,
            fake_msg_id,
            false,
            &dennett_runtime_antigravity::TurnOutcome {
                assistant_text: String::from("Heartbeat done"),
                status: RunStatus::Succeeded,
                stop_reason: String::from("end_turn"),
                events: Vec::new(),
                usage: Some(serde_json::json!({ "totalTokens": 15 })),
                human_wait_ms: 0,
            },
            50,
            None,
            None,
            &[],
            false,
            &cid,
        )
        .await
        .expect("complete_task_run while archived");
    assert_eq!(completed_status, RunStatus::Succeeded);
    let periodic_after_complete = svc
        .get_task_detail(periodic_task.task_id)
        .await
        .expect("periodic after mid-run complete");
    assert_eq!(
        periodic_after_complete.task.status,
        TaskStatus::WaitingTimer,
        "D2-ARCH-02: periodic task with paused timer must transition to WaitingTimer, NEVER Succeeded!"
    );

    // 4. Unarchive restores ONLY `archived_project` timers; `misfire_ask` stays paused!
    let unarchived = svc
        .unarchive_project(proj.project_id, &cid)
        .await
        .expect("unarchive_project");
    assert_eq!(
        unarchived.project.lifecycle_state,
        ProjectLifecycleState::Detached
    );

    let periodic_after_unarchive = svc
        .get_task_detail(periodic_task.task_id)
        .await
        .expect("periodic after unarchive");
    assert_eq!(
        periodic_after_unarchive.timers[0].status,
        dennett_contracts::TaskTimerStatus::Pending
    );
    assert_eq!(periodic_after_unarchive.timers[0].pause_reason, None);

    let misfire_after_unarchive = svc
        .get_task_detail(misfire_task.task_id)
        .await
        .expect("misfire after unarchive");
    assert_eq!(
        misfire_after_unarchive.timers[0].status,
        dennett_contracts::TaskTimerStatus::Paused,
        "D2-ARCH-02: misfire_ask timer MUST remain paused after unarchive_project!"
    );
    assert_eq!(
        misfire_after_unarchive.timers[0].pause_reason,
        Some(TimerPauseReason::MisfireAsk)
    );

    drop(pg);
    drop(svc);
    drop_isolated_schema(&base_url, &schema).await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "verifies D2-ARCH-03 default-deny PolicyEngine, StopTask rejection on UnknownEffect, remove/delete UNKNOWN preservation, and concurrent reconcile serialization"
)]
async fn r1_arch_03_effect_policy_stop_guard_delete_preservation_and_concurrent_reconcile() {
    let (_iso_url, _target, svc, base_url, schema) =
        make_isolated_schema_service_with_extras("arch03", None, HeadServiceExtras::default())
            .await;
    let cid = CorrelationId::generate();
    let scope = unique_scope("arch03");
    let ws_dir = env::temp_dir().join(format!("dennett-r1-arch03-{}", Uuid::new_v4().simple()));
    fs::create_dir_all(&ws_dir).expect("create ws");

    let proj = svc
        .create_project(
            CreateProjectRequest {
                name: String::from("Effects Safety Project"),
                workspace_path: Some(ws_dir.to_string_lossy().into_owned()),
                memory_scope: Some(scope.clone()),
            },
            &cid,
        )
        .await
        .expect("project");
    let other_proj = svc
        .create_project(
            CreateProjectRequest {
                name: String::from("Foreign Project"),
                workspace_path: Some(ws_dir.to_string_lossy().into_owned()),
                memory_scope: Some(unique_scope("arch03-foreign")),
            },
            &cid,
        )
        .await
        .expect("foreign project");
    let agent = svc
        .create_agent(
            CreateAgentRequest {
                name: String::from("Effects Agent"),
                description: String::from("External effect verifier"),
                system_prompt: String::from("Dispatch safely."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope]),
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
                autonomy_mode: None,
            },
            &cid,
        )
        .await
        .expect("agent");
    let task = svc
        .create_task(
            CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Webhook Task")),
                goal: String::from("Send webhook"),
                schedule: None,
                wait_for_signal: Some(String::from("go")),
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
            &cid,
        )
        .await
        .expect("task");

    // 1. Cross-project session mismatch is rejected fail-closed!
    let cross_err = svc
        .dispatch_external_effect(
            DispatchEffectRequest {
                project_id: other_proj.project_id,
                session_id: task.session_id,
                task_id: Some(task.task_id),
                run_id: None,
                agent_id: agent.agent_id,
                effect_idempotency_key: IdempotencyKey::generate(),
                connector_id: None,
                operation: String::from("send_webhook"),
                target_recipient: String::from("https://hooks.example/a"),
                parameters_json: String::from("{}"),
                simulated_behavior: Some(SimEffectBehavior::Success),
            },
            &cid,
        )
        .await;
    assert!(matches!(cross_err, Err(DennettError::AccessDenied { .. })));

    // 2. Default-deny without explicit Allow rule: returns AccessDenied, creates 0 claims, records Deny decision!
    let denied = svc
        .dispatch_external_effect(
            DispatchEffectRequest {
                project_id: proj.project_id,
                session_id: task.session_id,
                task_id: Some(task.task_id),
                run_id: None,
                agent_id: agent.agent_id,
                effect_idempotency_key: IdempotencyKey::generate(),
                connector_id: None,
                operation: String::from("send_webhook"),
                target_recipient: String::from("https://hooks.example/a"),
                parameters_json: String::from("{}"),
                simulated_behavior: Some(SimEffectBehavior::Success),
            },
            &cid,
        )
        .await;
    assert!(matches!(denied, Err(DennettError::AccessDenied { .. })));
    assert!(
        svc.list_effects(Some(task.task_id), None, None)
            .await
            .expect("list effects")
            .effects
            .is_empty(),
        "denied effect must create 0 effect_claims"
    );
    let sess_detail = svc
        .get_session_detail(task.session_id)
        .await
        .expect("session detail");
    assert!(
        sess_detail.permission_decisions.iter().any(|d| d.tool_name
            == "external_effect:send_webhook"
            && d.effect == PolicyEffect::Deny),
        "denied effect dispatch must record a Deny row in permission_decisions"
    );

    // 3. Add Allow rule -> dispatch with LostAck -> Unknown + UnknownExternal + open UnknownEffect card!
    svc.create_policy_rule(
        CreatePolicyRuleRequest {
            scope: format!("project:{}", proj.project_id),
            tool_name: String::from("external_effect:send_webhook"),
            target_pattern: Some(String::from("https://hooks.example/*")),
            effect: PolicyEffect::Allow,
            reason: String::from("Permit webhook for R1 test"),
        },
        &cid,
    )
    .await
    .expect("allow rule");

    let dispatched = svc
        .dispatch_external_effect(
            DispatchEffectRequest {
                project_id: proj.project_id,
                session_id: task.session_id,
                task_id: Some(task.task_id),
                run_id: None,
                agent_id: agent.agent_id,
                effect_idempotency_key: IdempotencyKey::generate(),
                connector_id: None,
                operation: String::from("send_webhook"),
                target_recipient: String::from("https://hooks.example/a"),
                parameters_json: String::from("{}"),
                simulated_behavior: Some(SimEffectBehavior::LostAck),
            },
            &cid,
        )
        .await
        .expect("dispatch LostAck");
    assert_eq!(dispatched.effect.status, ExternalEffectStatus::Unknown);

    let cards = svc
        .list_inbox_cards(Some("open"), Some(proj.project_id))
        .await
        .expect("open cards");
    let unk_card = cards
        .cards
        .iter()
        .find(|c| c.kind == InboxCardKind::UnknownEffect)
        .expect("unknown_effect card");

    // 4. StopTask MUST NOT close an `UnknownEffect` card!
    let stop_card_err = svc
        .resolve_inbox_card(
            unk_card.card_id,
            ResolveInboxCardRequest {
                expected_revision: unk_card.revision,
                resolution: InboxResolutionAction::StopTask {
                    reason: Some(String::from("try bypassing reconcile")),
                },
            },
            &cid,
        )
        .await;
    assert!(
        matches!(stop_card_err, Err(DennettError::Conflict { .. })),
        "D2-ARCH-03: StopTask on UnknownEffect card MUST fail with 409 Conflict, got: {stop_card_err:?}"
    );

    // 5. `remove_project` AND `delete_project(confirm=true)` MUST preserve `unknown_external` & open card!
    let _ = svc
        .remove_project(proj.project_id, &cid)
        .await
        .expect("remove project");
    let _ = svc
        .delete_project(proj.project_id, true, &cid)
        .await
        .expect("delete project");

    let task_after_del = svc
        .get_task_detail(task.task_id)
        .await
        .expect("task detail after delete");
    assert_eq!(
        task_after_del.task.status,
        TaskStatus::UnknownExternal,
        "D2-ARCH-03: delete_project MUST NEVER overwrite unknown_external into cancelled"
    );
    let inbox_after_del = svc
        .list_inbox_cards(Some("open"), None)
        .await
        .expect("open inbox after delete");
    assert!(
        inbox_after_del
            .cards
            .iter()
            .any(|c| c.card_id == unk_card.card_id && c.status == InboxCardStatus::Open),
        "D2-ARCH-03: open unknown_effect card MUST survive project remove/delete!"
    );

    // 6. Concurrent `reconcile_external_effect`: exactly 1 succeeds, 2nd gets 409 Conflict!
    let barrier = Arc::new(Barrier::new(2));
    let mut handles = Vec::new();
    for _ in 0..2 {
        let s = svc.clone();
        let b = Arc::clone(&barrier);
        let eff_id = dispatched.effect.effect_id;
        let c = cid.clone();
        handles.push(tokio::spawn(async move {
            b.wait().await;
            s.reconcile_external_effect(
                eff_id,
                ReconcileEffectRequest {
                    manual_resolution: Some(ManualEffectResolution::MarkConfirmed),
                    reason: Some(String::from("Checked partner dashboard")),
                },
                &c,
            )
            .await
        }));
    }
    let mut ok_count = 0_usize;
    let mut conflict_count = 0_usize;
    for h in handles {
        match h.await.expect("join reconcile") {
            Ok(resp) => {
                ok_count += 1;
                assert_eq!(resp.effect.status, ExternalEffectStatus::Confirmed);
                assert_ne!(resp.resolution_summary, "");
            }
            Err(DennettError::Conflict { .. }) => conflict_count += 1,
            Err(other) => panic!("unexpected reconcile error: {other:?}"),
        }
    }
    assert_eq!(ok_count, 1, "exactly one concurrent reconcile must succeed");
    assert_eq!(
        conflict_count, 1,
        "second concurrent reconcile must fail with 409 Conflict"
    );

    let _ = fs::remove_dir_all(&ws_dir);
    drop(svc);
    drop_isolated_schema(&base_url, &schema).await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "verifies D2-SEC-01 empty scope denial on agent_messages & commit_memory_note, plus D2-SEC-02 parent_write_ceiling blocking child write even with broad project CapabilityGrant"
)]
async fn r1_sec_01_sec_02_memory_scope_isolation_and_delegated_child_ceiling() {
    let (_iso_url, _target, svc, base_url, schema) =
        make_isolated_schema_service_with_extras("sec0102", None, HeadServiceExtras::default())
            .await;
    let cid = CorrelationId::generate();
    let scope = unique_scope("sec0102");
    let ws_dir = env::temp_dir().join(format!("dennett-r1-sec-{}", Uuid::new_v4().simple()));
    fs::create_dir_all(ws_dir.join("docs")).expect("create docs");
    fs::create_dir_all(ws_dir.join("src")).expect("create src");

    let proj = svc
        .create_project(
            CreateProjectRequest {
                name: String::from("Security Isolation Project"),
                workspace_path: Some(ws_dir.to_string_lossy().into_owned()),
                memory_scope: Some(scope.clone()),
            },
            &cid,
        )
        .await
        .expect("project");

    let no_scope_agent = svc
        .create_agent(
            CreateAgentRequest {
                name: String::from("Zero-Scope Agent"),
                description: String::from("Has empty allowed_memory_scopes"),
                system_prompt: String::from("No memory access."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::new(),
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
                autonomy_mode: None,
            },
            &cid,
        )
        .await
        .expect("no-scope agent");
    let parent_agent = svc
        .create_agent(
            CreateAgentRequest {
                name: String::from("Parent Docs-Only Agent"),
                description: String::from("Can write only under docs/"),
                system_prompt: String::from("Docs only."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope.clone()]),
                workspace_write_policy: WorkspaceWritePolicy::AllowPaths {
                    allowed_prefixes: vec![String::from("docs/")],
                },
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
                autonomy_mode: None,
            },
            &cid,
        )
        .await
        .expect("parent agent");
    let ask_agent = svc
        .create_agent(
            CreateAgentRequest {
                name: String::from("AskBeforeWrite Agent"),
                description: String::from("Cannot be delegated under AllowPaths parent"),
                system_prompt: String::from("Ask before write."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope.clone()]),
                workspace_write_policy: WorkspaceWritePolicy::AskBeforeWrite,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
                autonomy_mode: None,
            },
            &cid,
        )
        .await
        .expect("ask agent");
    let child_docs_agent = svc
        .create_agent(
            CreateAgentRequest {
                name: String::from("Child Docs Agent"),
                description: String::from("Allowed docs/ under parent"),
                system_prompt: String::from("Child worker."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope.clone()]),
                workspace_write_policy: WorkspaceWritePolicy::AllowPaths {
                    allowed_prefixes: vec![String::from("docs/")],
                },
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
                autonomy_mode: None,
            },
            &cid,
        )
        .await
        .expect("child docs agent");

    // 1. D2-SEC-01: Empty `allowed_memory_scopes` sender MUST be denied `send_agent_message`!
    let msg_denied = svc
        .send_agent_message(
            parent_agent.agent_id,
            SendAgentMessageRequest {
                sender_agent_id: Some(no_scope_agent.agent_id),
                project_id: proj.project_id,
                related_task_id: None,
                idempotency_key: IdempotencyKey::generate(),
                body_text: String::from("unauthorized cross-scope ping"),
            },
            &cid,
        )
        .await;
    assert!(
        matches!(msg_denied, Err(DennettError::ForbiddenScope { .. })),
        "D2-SEC-01: empty allowed_memory_scopes must be DENIED, got: {msg_denied:?}"
    );

    // 2. D2-SEC-01: `commit_memory_note` bound to a zero-scope agent's session MUST fail closed!
    let zero_sess = svc
        .create_session(
            CreateSessionRequest {
                agent_id: no_scope_agent.agent_id,
                project_id: Some(proj.project_id),
                title: String::from("Zero Scope Chat"),
                session_kind: SessionKind::UserChat,
            },
            &cid,
        )
        .await
        .expect("zero scope session");
    let mem_denied = svc
        .commit_memory_note(
            CommitMemoryNoteRequest {
                session_id: Some(zero_sess.session_id),
                run_id: None,
                scope: scope.clone(),
                title: String::from("Should be denied"),
                body: String::from("Cannot write memory without allowed_memory_scopes"),
                tags: Vec::new(),
                idempotency_key: Some(IdempotencyKey::generate()),
            },
            &cid,
        )
        .await;
    assert!(
        matches!(mem_denied, Err(DennettError::AccessDenied { .. })),
        "D2-SEC-01: commit_memory_note on zero-scope session must fail closed, got: {mem_denied:?}"
    );

    // 3. D2-SEC-02: Delegating `AskBeforeWrite` child under `AllowPaths` parent MUST fail!
    let parent_task = svc
        .create_task(
            CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: parent_agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Parent Docs Task")),
                goal: String::from("Own docs"),
                schedule: None,
                wait_for_signal: Some(String::from("idle")),
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
            &cid,
        )
        .await
        .expect("parent task");

    let bad_delegate = svc
        .create_task(
            CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: ask_agent.agent_id,
                session_id: None,
                parent_task_id: Some(parent_task.task_id),
                title: Some(String::from("Disallowed Ask Child")),
                goal: String::from("Should fail subset check"),
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
            &cid,
        )
        .await;
    assert!(
        matches!(bad_delegate, Err(DennettError::AccessDenied { .. })),
        "D2-SEC-02: (AskBeforeWrite, AllowPaths) delegation must be rejected, got: {bad_delegate:?}"
    );

    // 4. D2-SEC-02: Even with a project-wide Allow rule for `write_file`, delegated child under
    //    `AllowPaths(["docs/"])` parent MUST be denied writing `src/escape.txt`!
    svc.create_policy_rule(
        CreatePolicyRuleRequest {
            scope: format!("project:{}", proj.project_id),
            tool_name: String::from("write_file"),
            target_pattern: Some(String::from("src/*")),
            effect: PolicyEffect::Allow,
            reason: String::from("Broad rule that must NOT bypass parent_write_ceiling"),
        },
        &cid,
    )
    .await
    .expect("broad project rule");

    let child_task = svc
        .create_task(
            CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: child_docs_agent.agent_id,
                session_id: None,
                parent_task_id: Some(parent_task.task_id),
                title: Some(String::from("Child Escape Attempt")),
                goal: String::from("[SIM:TOOL:write_file:src/escape.txt|should-not-write]"),
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
            &cid,
        )
        .await
        .expect("child task");
    svc.execute_task_generation(child_task.task_id, 1, cid.clone())
        .await
        .expect("execute child task");
    assert!(
        !ws_dir.join("src/escape.txt").exists(),
        "D2-SEC-02: delegated child must NEVER write outside parent AllowPaths ceiling!"
    );

    let _ = fs::remove_dir_all(&ws_dir);
    drop(svc);
    drop_isolated_schema(&base_url, &schema).await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "verifies D2-BUG-01..05 across durable restart reply/signal, update_project rebind, human_wait exclusion + preview_snippet, agent_messages release, OnceAt 400, insufficient budget increase, and WeeklyWallTime PG16 calculation"
)]
async fn r1_bug_01_through_05_durability_human_wait_rebind_and_weekly_schedule() {
    let (_iso_url, iso_target, svc, base_url, schema) =
        make_isolated_schema_service_with_extras("bugs", None, HeadServiceExtras::default()).await;
    let cid = CorrelationId::generate();
    let scope = unique_scope("bugs");
    let ws_dir = env::temp_dir().join(format!("dennett-r1-bugs-{}", Uuid::new_v4().simple()));
    fs::create_dir_all(&ws_dir).expect("create ws");

    let proj = svc
        .create_project(
            CreateProjectRequest {
                name: String::from("Bugs Verification Project"),
                workspace_path: Some(ws_dir.to_string_lossy().into_owned()),
                memory_scope: Some(scope.clone()),
            },
            &cid,
        )
        .await
        .expect("project");
    let detached_proj = svc
        .detach_project_workspace(proj.project_id, &cid)
        .await
        .expect("detach workspace");
    assert_eq!(
        detached_proj.project.lifecycle_state,
        ProjectLifecycleState::Detached
    );

    // D2-BUG-02: `update_project` with `workspace_path` transitions `Detached -> Active`!
    let bound_proj = svc
        .update_project(
            proj.project_id,
            UpdateProjectRequest {
                name: None,
                workspace_path: Some(ws_dir.to_string_lossy().into_owned()),
                pinned: None,
            },
            &cid,
        )
        .await
        .expect("bind workspace via update_project");
    assert_eq!(bound_proj.lifecycle_state, ProjectLifecycleState::Active);

    let agent = svc
        .create_agent(
            CreateAgentRequest {
                name: String::from("Bugs Agent"),
                description: String::from("AskBeforeWrite agent"),
                system_prompt: String::from("Execute carefully."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope.clone()]),
                workspace_write_policy: WorkspaceWritePolicy::AskBeforeWrite,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
                autonomy_mode: None,
            },
            &cid,
        )
        .await
        .expect("agent");

    // D2-BUG-05: Out-of-range `OnceAt` returns `400 InvalidInput` (never 503)!
    let bad_once = svc
        .create_task(
            CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Bad OnceAt")),
                goal: String::from("Fail validation"),
                schedule: Some(TaskSchedule {
                    kind: ScheduleKind::OnceAt {
                        fire_at: TimestampMs::from_millis(i64::MAX / 2),
                    },
                    timezone: String::from("UTC"),
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
            &cid,
        )
        .await;
    assert!(
        matches!(bad_once, Err(DennettError::InvalidInput { .. })),
        "D2-BUG-05: out-of-range OnceAt must fail with 400 InvalidInput, got: {bad_once:?}"
    );

    // D2-BUG-05 / Events: WeeklyWallTime PG16 calculation (Monday=1 + Wednesday=4 at 09:30 UTC).
    // Reference: 2026-11-24T10:00:00Z (Tuesday) -> next matching weekday is Wednesday 2026-11-25T09:30:00Z!
    let mgr = Manager::from_config(
        iso_target.config().clone(),
        NoTls,
        ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        },
    );
    let pg_pool = Pool::builder(mgr).max_size(4).build().expect("pg pool");
    let pg = pg_pool.get().await.expect("pooled pg conn");
    let tuesday_ms = 1_795_514_400_000_i64; // 2026-11-24T10:00:00Z (Tuesday)
    let expected_wed_ms = 1_795_599_000_000_i64; // 2026-11-25T09:30:00Z (Wednesday)
    let weekly_sched = TaskSchedule {
        kind: ScheduleKind::WeeklyWallTime {
            weekdays_mask: (1 << 0) | (1 << 2), // Mon + Wed
            hour: 9,
            minute: 30,
        },
        timezone: String::from("UTC"),
        original_expression: None,
        missed_run_policy: MissedRunPolicy::ExecuteOnce,
        next_fire_at: None,
    };
    let computed = compute_next_fire_at_pg(
        &pg,
        &weekly_sched,
        TimestampMs::from_millis(tuesday_ms),
        &iso_target,
    )
    .await
    .expect("compute weekly fire time");
    assert_eq!(
        computed.map(TimestampMs::as_millis),
        Some(expected_wed_ms),
        "WeeklyWallTime must compute exact next ISO weekday fire time in PG16"
    );

    // D2-BUG-03 + D2-UX-05: Task with `max_wall_secs: Some(1)` waits 1.2s for human approval,
    // populates redacted `preview_snippet` on `permission_requests`, and completes WITHOUT tripping
    // RPC timeout or `max_wall_secs` budget (`wall_ms_used < 1000`)!
    let wait_task = svc
        .create_task(
            CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Human Wait Budget Task")),
                goal: String::from(
                    "[SIM:TOOL:write_file:approved.txt|safe text SUPER_SECRET_R1_TOKEN_999]",
                ),
                schedule: None,
                wait_for_signal: None,
                budget: Some(TaskBudget {
                    max_turns: Some(5),
                    max_tokens: None,
                    max_wall_secs: Some(1),
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
            &cid,
        )
        .await
        .expect("wait task");

    let svc_exec = svc.clone();
    let cid_exec = cid.clone();
    let wait_tid = wait_task.task_id;
    let exec_handle = tokio::spawn(async move {
        svc_exec
            .execute_task_generation(wait_tid, 1, cid_exec)
            .await
    });

    let mut pending_req = None;
    for _ in 0..40 {
        let perms = svc
            .list_permission_requests(Some("pending"), Some(wait_task.session_id))
            .await
            .expect("list pending perms");
        if let Some(req) = perms.requests.into_iter().next() {
            pending_req = Some(req);
            break;
        }
        time::sleep(Duration::from_millis(50)).await;
    }
    let perm_req = pending_req.expect("pending ask_before_write request");
    assert_eq!(perm_req.status, PermissionRequestStatus::Pending);
    let snippet = perm_req
        .preview_snippet
        .as_deref()
        .expect("D2-UX-05: preview_snippet must be populated on write_file approval request");
    assert!(
        !snippet.contains("SUPER_SECRET_R1_TOKEN_999"),
        "preview_snippet must redact configured secrets: {snippet}"
    );
    assert!(snippet.contains("safe text"));

    // Sleep 1250ms (> max_wall_secs=1s!) before approving to prove human wait is excluded!
    time::sleep(Duration::from_millis(1_250)).await;
    svc.resolve_permission_request(
        perm_req.request_id,
        ResolvePermissionRequest {
            decision: PermissionResolutionDecision::ApproveOnce,
            expected_revision: Some(perm_req.revision),
            reason: Some(String::from("Approved after thinking")),
            max_uses: None,
            expires_in_secs: None,
        },
        &cid,
    )
    .await
    .expect("approve after 1.25s human wait");

    exec_handle
        .await
        .expect("join exec_handle")
        .expect("task turn succeeds cleanly");
    let wait_detail = svc
        .get_task_detail(wait_tid)
        .await
        .expect("wait task detail");
    assert_eq!(
        wait_detail.task.status,
        TaskStatus::Succeeded,
        "D2-BUG-03: human wait must NOT trip max_wall_secs or turn timeout!"
    );
    assert!(
        wait_detail.task.usage.wall_ms_used < 1_000,
        "D2-BUG-03: wall_ms_used ({}) must exclude human wait time",
        wait_detail.task.usage.wall_ms_used
    );
    assert!(ws_dir.join("approved.txt").is_file());

    // D2-BUG-01: Durable signal & clarification continuation across simulated Head restart!
    let sig_task = svc
        .create_task(
            CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Durable Signal Task")),
                goal: String::from("Process release signal"),
                schedule: None,
                wait_for_signal: Some(String::from("release.ready")),
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
            &cid,
        )
        .await
        .expect("signal task");

    // Queue a non-matching signal (queued_on_task) and verify durable consumption on next run!
    let _queued_sig = svc
        .send_task_signal(
            sig_task.task_id,
            TaskSignalRequest {
                signal_name: String::from("pre.context"),
                payload_text: String::from("DURABLE_SIGNAL_PAYLOAD_XYZ"),
                idempotency_key: IdempotencyKey::generate(),
            },
            &cid,
        )
        .await
        .expect("queued signal");
    // Manually mark task queued (simulating Head crashed right after DB commit before spawn ran)
    pg.execute(
        "UPDATE tasks SET status = 'queued', pending_continuation_input = $2 WHERE task_id = $1",
        &[
            &sig_task.task_id.as_uuid(),
            &"[User Clarification Reply]\nDURABLE_REPLY_ABC",
        ],
    )
    .await
    .expect("seed pending continuation");

    // Construct a brand-new HeadService instance (fresh memory, zero in-memory state!) on same PG!
    let fresh_svc = HeadService::new_with_extras(
        iso_target.clone(),
        Arc::new(SimulatedRuntime::new(RuntimeAuthProfile::OAuthPersonal)),
        None,
        None,
        ProfileMode::TestIsolated,
        Vec::new(),
        HeadServiceExtras::default(),
    )
    .expect("fresh HeadService");
    fresh_svc
        .execute_task_generation(sig_task.task_id, 1, cid.clone())
        .await
        .expect("fresh Head executes task from PG");

    let sig_sess = fresh_svc
        .get_session_detail(sig_task.session_id)
        .await
        .expect("sig session detail");
    assert!(
        sig_sess.messages.iter().any(|m| {
            m.content.contains("DURABLE_SIGNAL_PAYLOAD_XYZ")
                && m.content.contains("DURABLE_REPLY_ABC")
        }),
        "D2-BUG-01: fresh Head after restart MUST consume both pending_continuation_input and queued task_signals from PG!"
    );

    // Also exercise live `reply_to_task` on a `WaitingClarification` task!
    let clarify_task = svc
        .create_task(
            CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Clarify Reply Task")),
                goal: String::from("[SIM:CLARIFY:Need target env?]"),
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
            &cid,
        )
        .await
        .expect("clarify task");
    svc.execute_task_generation(clarify_task.task_id, 1, cid.clone())
        .await
        .expect("run clarify turn");
    let replied = svc
        .reply_to_task(
            clarify_task.task_id,
            ReplyTaskRequest {
                answer_text: String::from("USE_STAGING_ENV_R1"),
            },
            &cid,
        )
        .await
        .expect("reply_to_task");
    assert_eq!(replied.status, TaskStatus::Queued);
    let mut clarify_done = false;
    for _ in 0..40 {
        let d = svc
            .get_task_detail(clarify_task.task_id)
            .await
            .expect("clarify detail");
        if d.task.status == TaskStatus::Succeeded {
            clarify_done = true;
            break;
        }
        time::sleep(Duration::from_millis(50)).await;
    }
    assert!(clarify_done, "replied clarification task should finish");
    let clarify_sess = svc
        .get_session_detail(clarify_task.session_id)
        .await
        .expect("clarify sess");
    assert!(
        clarify_sess
            .messages
            .iter()
            .any(|m| m.content.contains("USE_STAGING_ENV_R1")),
        "D2-BUG-01: reply_to_task must deliver persisted continuation input"
    );

    // D2-BUG-04: Failed run MUST release `agent_messages.delivered_in_run_id = NULL`, and
    // user chat MUST NOT steal task-scoped (`related_task_id IS NOT NULL`) agent messages!
    let fail_msg_task = svc
        .create_task(
            CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Failing Msg Delivery Task")),
                goal: String::from("[SIM:ERROR:simulated crash]"),
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
            &cid,
        )
        .await
        .expect("fail msg task");
    let scoped_agent_msg = svc
        .send_agent_message(
            agent.agent_id,
            SendAgentMessageRequest {
                sender_agent_id: Some(agent.agent_id),
                project_id: proj.project_id,
                related_task_id: Some(fail_msg_task.task_id),
                idempotency_key: IdempotencyKey::generate(),
                body_text: String::from("MUST_NOT_BE_LOST_ON_FAIL_OR_CHAT"),
            },
            &cid,
        )
        .await
        .expect("scoped agent message");

    // First verify an unrelated user chat does NOT steal the task-scoped agent message!
    let user_chat = svc
        .create_session(
            CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: Some(proj.project_id),
                title: String::from("Unrelated User Chat"),
                session_kind: SessionKind::UserChat,
            },
            &cid,
        )
        .await
        .expect("user chat");
    let chat_turn = svc
        .send_message(
            user_chat.session_id,
            SendMessageRequest {
                content: String::from("hello plain chat"),
                timeout_ms: Some(10_000),
                client_message_id: None,
            },
            cid.clone(),
        )
        .await
        .expect("plain chat turn");
    assert_eq!(chat_turn.run.status, RunStatus::Succeeded);
    let delivered_after_chat: Option<Uuid> = pg
        .query_one(
            "SELECT delivered_in_run_id FROM agent_messages WHERE agent_message_id = $1",
            &[&scoped_agent_msg.message.agent_message_id.as_uuid()],
        )
        .await
        .expect("query agent_message after chat")
        .get(0);
    assert!(
        delivered_after_chat.is_none(),
        "D2-BUG-04: user chat must NOT consume task-scoped agent_messages!"
    );

    // Now run the failing task: message is temporarily claimed and MUST be released back to NULL on failure!
    let _ = svc
        .execute_task_generation(fail_msg_task.task_id, 1, cid.clone())
        .await;
    let delivered_after_fail: Option<Uuid> = pg
        .query_one(
            "SELECT delivered_in_run_id FROM agent_messages WHERE agent_message_id = $1",
            &[&scoped_agent_msg.message.agent_message_id.as_uuid()],
        )
        .await
        .expect("query agent_message after failed run")
        .get(0);
    assert!(
        delivered_after_fail.is_none(),
        "D2-BUG-04: failed task run MUST reset agent_messages.delivered_in_run_id back to NULL!"
    );

    // D2-BUG-05: Insufficient `IncreaseBudget` rejected with 400 InvalidInput!
    let budget_task = svc
        .create_task(
            CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Budget Exceeded Task")),
                goal: String::from("[SIM:TOKENS:50]"),
                schedule: None,
                wait_for_signal: None,
                budget: Some(TaskBudget {
                    max_turns: None,
                    max_tokens: Some(10),
                    max_wall_secs: None,
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
            &cid,
        )
        .await
        .expect("budget task");
    svc.execute_task_generation(budget_task.task_id, 1, cid.clone())
        .await
        .expect("run 1 hits token budget");
    let budget_card = svc
        .list_inbox_cards(Some("open"), Some(proj.project_id))
        .await
        .expect("inbox")
        .cards
        .into_iter()
        .find(|c| c.kind == InboxCardKind::BudgetExceeded && c.task_id == Some(budget_task.task_id))
        .expect("budget_exceeded card");

    // Adding only turns does NOT relieve the exceeded `max_tokens` dimension -> 400 InvalidInput!
    let insuff = svc
        .resolve_inbox_card(
            budget_card.card_id,
            ResolveInboxCardRequest {
                expected_revision: budget_card.revision,
                resolution: InboxResolutionAction::IncreaseBudget {
                    additional_turns: Some(5),
                    additional_tokens: None,
                    additional_wall_secs: None,
                },
            },
            &cid,
        )
        .await;
    assert!(
        matches!(insuff, Err(DennettError::InvalidInput { .. })),
        "D2-BUG-05: increasing wrong budget dimension must return 400 InvalidInput, got: {insuff:?}"
    );

    // Increasing the actually exceeded `tokens` dimension succeeds cleanly!
    let relieved = svc
        .resolve_inbox_card(
            budget_card.card_id,
            ResolveInboxCardRequest {
                expected_revision: budget_card.revision,
                resolution: InboxResolutionAction::IncreaseBudget {
                    additional_turns: None,
                    additional_tokens: Some(100),
                    additional_wall_secs: None,
                },
            },
            &cid,
        )
        .await;
    assert!(
        relieved.is_ok(),
        "D2-BUG-05: relieving exceeded tokens dimension must succeed, got: {relieved:?}"
    );

    // D2-BUG-04: `update_session` emits `session.updated` control event!
    let before_cursor = svc
        .get_state_snapshot(Some(proj.project_id), false)
        .await
        .expect("snap")
        .snapshot_cursor
        .committed_seq;
    let renamed = svc
        .update_session(
            wait_task.session_id,
            UpdateSessionRequest {
                title: Some(String::from("Renamed Session Event Check")),
                pinned: None,
                status: None,
            },
            &cid,
        )
        .await
        .expect("update session");
    assert_eq!(renamed.title, "Renamed Session Event Check");
    let tail = svc
        .get_events(before_cursor, None, Some(proj.project_id), Some(50))
        .await
        .expect("events tail");
    assert!(
        tail.events
            .iter()
            .any(|e| e.event_type == "session.updated"),
        "D2-BUG-04: update_session must emit session.updated control event"
    );

    drop(pg);
    drop(pg_pool);
    drop(fresh_svc);
    drop(svc);
    let _ = fs::remove_dir_all(&ws_dir);
    drop_isolated_schema(&base_url, &schema).await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "verifies D2-OVER-01 forward migration from schema v5 memory_commit_outbox into ordered_outbox and automatic flush delivery"
)]
async fn r1_over_01_v5_to_v6_memory_outbox_migration_and_ordered_drain() {
    let (base_url, _) = require_control_pg_target();
    let short = &Uuid::new_v4().simple().to_string()[..12];
    let schema = format!("r1_over01_{short}");

    let (pg, conn) = tokio_postgres::connect(&base_url, NoTls)
        .await
        .expect("connect control PG");
    tokio::spawn(async move {
        let _ = conn.await;
    });
    pg.batch_execute(&format!("CREATE SCHEMA {schema}"))
        .await
        .expect("create schema");
    drop(pg);

    let iso_url = with_isolated_schema_url(&base_url, &schema);
    let iso_target = ValidatedControlPgTarget::resolve_from_sources(
        None,
        Some(&iso_url),
        None,
        ProfileMode::TestIsolated,
    )
    .expect("resolve target")
    .expect("validated target");

    // 1. Apply ONLY migrations v1..=v5 manually and insert a pending row into legacy `memory_commit_outbox`!
    let mgr = Manager::from_config(
        iso_target.config().clone(),
        NoTls,
        ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        },
    );
    let pool = Pool::builder(mgr).max_size(4).build().expect("pool");
    {
        let mut c = pool.get().await.expect("conn");
        let tx = c.transaction().await.expect("tx");
        tx.batch_execute(
            "CREATE TABLE IF NOT EXISTS control_schema_migrations (
                version BIGINT PRIMARY KEY CHECK (version > 0),
                name TEXT NOT NULL CHECK (octet_length(name) > 0),
                checksum TEXT NOT NULL CHECK (checksum ~ '^[0-9a-f]{64}$'),
                applied_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )",
        )
        .await
        .expect("migrations table");
        for mig in CONTROL_MIGRATIONS.iter().filter(|m| m.version <= 5) {
            tx.batch_execute(mig.sql).await.expect("apply v1..v5");
            let checksum = dennett_head::sha256_hex(mig.sql.as_bytes());
            tx.execute(
                "INSERT INTO control_schema_migrations (version, name, checksum) VALUES ($1, $2, $3)",
                &[&mig.version, &mig.name, &checksum.as_str()],
            )
            .await
            .expect("record migration");
        }
        let scope = unique_scope("legacy-mig");
        let legacy_idem = IdempotencyKey::generate();
        let note_req = dennett_contracts::CreateNoteRequest {
            scope: scope.clone(),
            title: String::from("Legacy v5 Outbox Note"),
            body: String::from("Must migrate cleanly to ordered_outbox in v6!"),
            tags: vec![String::from("migration")],
            observed_at: None,
            evidence_links: Vec::new(),
            author_label: Some(String::from("dennett-head")),
            admission_status: None,
            salience: None,
            topic_key: None,
            idempotency_key: legacy_idem.clone(),
        };
        let payload_str = serde_json::to_string(&note_req).expect("serialize note");
        tx.execute(
            "INSERT INTO memory_commit_outbox (
                outbox_id, scope, idempotency_key, operation, payload_json, status, correlation_id
             ) VALUES ($1, $2, $3, 'create_note', $4, 'pending', 'corr-legacy-v5')",
            &[
                &Uuid::new_v4(),
                &scope.as_str(),
                &legacy_idem.as_str(),
                &payload_str,
            ],
        )
        .await
        .expect("insert legacy v5 outbox row");
        tx.commit().await.expect("commit v5 state");
    }

    // 2. Start a lightweight loopback mock `memoryd` HTTP server that accepts `/healthz` & `POST /v1/notes`!
    let mock_listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock memoryd");
    let mock_addr = mock_listener.local_addr().expect("mock addr");
    let mock_app = axum::Router::new()
        .route(
            "/healthz",
            axum::routing::get(|| async {
                axum::Json(serde_json::json!({
                    "service": "memoryd",
                    "stage": "D1",
                    "profile_mode": "test_isolated",
                    "postgres_connected": true,
                    "postgres_role": "dennett_memory_role",
                    "postgres_database": "dennett_memory_test",
                    "schema_version": 1,
                    "capabilities": [],
                    "correlation_id": "corr-mock"
                }))
            }),
        )
        .route(
            "/v1/memory/notes",
            axum::routing::post(
                |axum::Json(req): axum::Json<dennett_contracts::CreateNoteRequest>| async move {
                    let note_id = dennett_contracts::MemoryNoteId::new();
                    let event_id = dennett_contracts::MemoryEventId::new();
                    let rev_id = dennett_contracts::NoteRevisionId::new();
                    let now = TimestampMs::now();
                    let resp = dennett_contracts::NoteMutationResponse {
                        note: dennett_contracts::MemoryNote {
                            note_id,
                            scope: req.scope,
                            current_revision: 1,
                            current_revision_id: rev_id,
                            title: req.title,
                            body: req.body,
                            tags: req.tags,
                            rationale: None,
                            last_event_id: event_id,
                            recorded_by: dennett_contracts::PrincipalId::try_new("head:test")
                                .expect("principal"),
                            author_label: req.author_label,
                            created_at: now,
                            updated_at: now,
                            admission_status: dennett_contracts::AdmissionStatus::Admitted,
                            salience: dennett_contracts::DEFAULT_NOTE_SALIENCE,
                            topic_key: None,
                            trust_level: dennett_contracts::TrustLevel::Native,
                            instruction_active: false,
                            evidence_links: Vec::new(),
                        },
                        event_id,
                        idempotent_replay: false,
                    };
                    (axum::http::StatusCode::CREATED, axum::Json(resp))
                },
            ),
        );
    let mock_handle = tokio::spawn(async move {
        let _ = axum::serve(mock_listener, mock_app).await;
    });

    // 3. Boot `HeadService` -> automatically applies v6 migration forwarding legacy row into `ordered_outbox`!
    let mem_client = MemoryClient::new(format!("http://{mock_addr}"), "tok-mock-r1")
        .expect("mock memory client");
    let svc = HeadService::new_with_extras(
        iso_target,
        Arc::new(SimulatedRuntime::new(RuntimeAuthProfile::OAuthPersonal)),
        Some(mem_client),
        None,
        ProfileMode::TestIsolated,
        Vec::new(),
        HeadServiceExtras::default(),
    )
    .expect("construct HeadService");
    let cid = CorrelationId::generate();
    let _ = svc.ensure_migrated().await.expect("apply v7 migration");

    let health_before = svc.health_check(cid.clone()).await.expect("health before");
    assert_eq!(
        health_before.schema_version,
        Some(dennett_head::LATEST_CONTROL_SCHEMA_VERSION)
    );
    assert_eq!(
        health_before.pending_outbox_count, 1,
        "D2-OVER-01: legacy v5 row must appear as pending_outbox_count=1 in ordered_outbox"
    );

    let list_before = svc.list_memory_outbox(&cid).await.expect("list outbox");
    assert_eq!(list_before.items.len(), 1);
    assert_eq!(list_before.items[0].status, MemoryOutboxStatus::Pending);

    // 4. Flush via unified `ordered_outbox` engine -> delivered cleanly!
    let flushed = svc.flush_memory_outbox(&cid).await.expect("flush outbox");
    assert_eq!(flushed.delivered, 1);
    assert_eq!(flushed.pending, 0);
    assert_eq!(flushed.failed, 0);

    let health_after = svc.health_check(cid.clone()).await.expect("health after");
    assert_eq!(health_after.pending_outbox_count, 0);
    let list_after = svc.list_memory_outbox(&cid).await.expect("list after");
    assert_eq!(list_after.items[0].status, MemoryOutboxStatus::Delivered);
    assert!(list_after.items[0].delivered_note_id.is_some());

    mock_handle.abort();
    drop(svc);
    drop(pool);
    drop_isolated_schema(&base_url, &schema).await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "end-to-end test proving an agent in chat spawns real dennett-control-mcp over stdio, creates a real durable schedule & TaskRun session in PG, and enforces dct1. project isolation + anti-recursion"
)]
async fn r1_control_mcp_e2e_agent_creates_event_and_enforces_token_isolation() {
    let control_mcp_bin = locate_control_mcp_bin();
    let (_iso_url, _target, svc, base_url, schema) = make_isolated_schema_service_with_extras(
        "ctrlmcp",
        None,
        HeadServiceExtras {
            control_mcp_bin: Some(control_mcp_bin),
            ..HeadServiceExtras::default()
        },
    )
    .await;

    // Bind a real loopback HTTP server for `Head` and register its bound address on `HeadService`!
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind head listener");
    let bound_addr = listener.local_addr().expect("bound addr");
    let head_url = format!("http://{bound_addr}");
    svc.set_self_http_url(&head_url);

    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
    let app = build_head_router(HeadAppState {
        service: Some(svc.clone()),
        profile_mode: ProfileMode::TestIsolated,
        runtime_adapter_id: String::from("simulated"),
        runtime_is_simulated: true,
        allow_non_loopback: false,
        shutdown_tx: shutdown_tx.clone(),
        request_limit: Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS)),
    });
    let server_handle = tokio::spawn(async move {
        let _ = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
            while !*shutdown_rx.borrow() {
                if shutdown_rx.changed().await.is_err() {
                    break;
                }
            }
        })
        .await;
    });

    let client = HeadClient::new(&head_url).expect("head client");
    let scope_a = unique_scope("ctrlmcp-a");
    let scope_b = unique_scope("ctrlmcp-b");

    let proj_a = client
        .create_project(
            &CreateProjectRequest {
                name: String::from("Control MCP Project A"),
                workspace_path: None,
                memory_scope: Some(scope_a.clone()),
            },
            None,
        )
        .await
        .expect("proj A");
    let proj_b = client
        .create_project(
            &CreateProjectRequest {
                name: String::from("Control MCP Project B"),
                workspace_path: None,
                memory_scope: Some(scope_b),
            },
            None,
        )
        .await
        .expect("proj B");
    let agent = client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Event Assistant"),
                description: String::from("Creates durable events via dennett-control-mcp"),
                system_prompt: String::from("Use dennett_event_create to schedule events."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope_a]),
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
    let chat_session = client
        .create_session(
            &CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: Some(proj_a.project_id),
                title: String::from("Schedule Request Chat"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("chat session");

    // 1. Chat turn invokes `[SIM:EVENT:CREATE ...]` -> PolicyEngine Step 4b -> SimulatedRuntime
    //    spawns real `dennett-control-mcp` binary -> HeadClient + `dct1.` token -> PG durable task!
    let turn_resp = client
        .send_message(
            chat_session.session_id,
            &SendMessageRequest {
                content: String::from(
                    r#"[SIM:EVENT:CREATE title="Morning Test Digest" goal="Run smoke tests and summarize" interval_secs=300]"#,
                ),
                timeout_ms: Some(15_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("chat turn invoking dennett-control-mcp");
    assert_eq!(turn_resp.run.status, RunStatus::Succeeded);
    let reply_text = turn_resp
        .assistant_message
        .as_ref()
        .map_or_default(|m| m.content.as_str());
    assert!(
        reply_text.contains("dennett_event_create ALLOWED")
            && reply_text.contains("Morning Test Digest"),
        "expected real MCP event creation confirmation, got: {reply_text}"
    );

    // 2. Verify durable task & dedicated `SessionKind::TaskRun` session exist in PG!
    let tasks_a = client
        .list_tasks(Some(proj_a.project_id), None, None)
        .await
        .expect("list tasks A");
    let created_task = tasks_a
        .tasks
        .iter()
        .find(|t| t.title == "Morning Test Digest")
        .expect("durable task created by dennett-control-mcp");
    assert_eq!(created_task.status, TaskStatus::WaitingTimer);
    assert_ne!(
        created_task.session_id, chat_session.session_id,
        "event task must run in its own isolated TaskRun session, never the live UserChat session"
    );

    let snap = client
        .get_state_snapshot(Some(proj_a.project_id), false, None)
        .await
        .expect("snapshot A");
    let task_session = snap
        .sessions
        .iter()
        .find(|s| s.session_id == created_task.session_id)
        .expect("task_run session in snapshot");
    assert_eq!(task_session.session_kind, SessionKind::TaskRun);
    let user_session = snap
        .sessions
        .iter()
        .find(|s| s.session_id == chat_session.session_id)
        .expect("user_chat session in snapshot");
    assert_eq!(user_session.session_kind, SessionKind::UserChat);

    // 3. Verify `X-Dennett-Control-Token` (`dct1.`) cryptographic project isolation & anti-recursion!
    let token_a_chat = svc
        .control_token_signer()
        .mint_control_token(
            Some(proj_a.project_id),
            agent.agent_id,
            chat_session.session_id,
            SessionKind::UserChat,
        )
        .expect("mint dct1 token A");
    let client_tok_a = HeadClient::new(&head_url)
        .expect("client")
        .with_control_token(token_a_chat);

    // Cross-project list/create/delete MUST fail closed with 403 AccessDenied!
    let cross_list = client_tok_a
        .list_tasks(Some(proj_b.project_id), None, None)
        .await;
    assert!(
        matches!(cross_list, Err(DennettError::AccessDenied { .. })),
        "dct1. token for Project A must be denied listing Project B tasks: {cross_list:?}"
    );
    let cross_create = client_tok_a
        .create_task(
            &CreateTaskRequest {
                project_id: proj_b.project_id,
                agent_id: agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Escalation")),
                goal: String::from("Should fail"),
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
        matches!(cross_create, Err(DennettError::AccessDenied { .. })),
        "dct1. token for Project A must be denied creating task in Project B: {cross_create:?}"
    );
    let admin_delete = client_tok_a
        .delete_project(proj_a.project_id, true, None)
        .await;
    assert!(
        matches!(admin_delete, Err(DennettError::AccessDenied { .. })),
        "dct1. token must be denied unrelated admin routes: {admin_delete:?}"
    );

    // Anti-self-recursion: a `dct1.` token minted for `SessionKind::TaskRun` cannot create new tasks!
    let token_task_run = svc
        .control_token_signer()
        .mint_control_token(
            Some(proj_a.project_id),
            agent.agent_id,
            created_task.session_id,
            SessionKind::TaskRun,
        )
        .expect("mint task_run token");
    let client_task_run = HeadClient::new(&head_url)
        .expect("client")
        .with_control_token(token_task_run);
    let recursive_create = client_task_run
        .create_task(
            &CreateTaskRequest {
                project_id: proj_a.project_id,
                agent_id: agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Recursive Spawn")),
                goal: String::from("Should fail anti-recursion"),
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
        matches!(recursive_create, Err(DennettError::AccessDenied { .. })),
        "dct1. token from TaskRun session must be denied creating new tasks: {recursive_create:?}"
    );

    let _ = shutdown_tx.send(true);
    let _ = server_handle.await;
    drop(svc);
    drop_isolated_schema(&base_url, &schema).await;
}

struct InspectingSimulatedRuntime {
    inner: SimulatedRuntime,
    captured_starts: Arc<std::sync::Mutex<Vec<SessionStartRequest>>>,
}

impl InspectingSimulatedRuntime {
    fn new() -> (Self, Arc<std::sync::Mutex<Vec<SessionStartRequest>>>) {
        let captured = Arc::new(std::sync::Mutex::new(Vec::new()));
        (
            Self {
                inner: SimulatedRuntime::new(RuntimeAuthProfile::OAuthPersonal),
                captured_starts: Arc::clone(&captured),
            },
            captured,
        )
    }
}

impl AgentRuntimePort for InspectingSimulatedRuntime {
    fn adapter_id(&self) -> &str {
        self.inner.adapter_id()
    }

    fn auth_profile(&self) -> RuntimeAuthProfile {
        self.inner.auth_profile()
    }

    fn is_simulated(&self) -> bool {
        self.inner.is_simulated()
    }

    fn probe(&self) -> BoxFuture<'_, Result<RuntimeDescriptor, DennettError>> {
        self.inner.probe()
    }

    fn start_or_resume_session(
        &self,
        req: SessionStartRequest,
    ) -> BoxFuture<'_, Result<SessionStartOutcome, DennettError>> {
        if let Ok(mut guard) = self.captured_starts.lock() {
            guard.push(req.clone());
        }
        self.inner.start_or_resume_session(req)
    }

    fn send_prompt_turn(
        &self,
        req: TurnRequest,
        policy: Arc<dyn PolicyEvaluator>,
        event_tx: Option<tokio::sync::mpsc::Sender<RuntimeEvent>>,
    ) -> BoxFuture<'_, Result<TurnOutcome, DennettError>> {
        self.inner.send_prompt_turn(req, policy, event_tx)
    }

    fn cancel_turn(&self, session_id: SessionId) -> BoxFuture<'_, Result<bool, DennettError>> {
        self.inner.cancel_turn(session_id)
    }

    fn close_session(&self, session_id: SessionId) -> BoxFuture<'_, Result<(), DennettError>> {
        self.inner.close_session(session_id)
    }
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "verifies standalone chat creation, DenyAll runtime isolation, memory MCP scoping, auto-title, inline artifacts, task rejection, running snapshot visibility, and survival across project archive/delete against real PG16"
)]
async fn v7_standalone_chats_enforce_deny_all_inline_artifacts_and_survive_project_delete() {
    let (base_url, _) = require_control_pg_target();
    let short = &Uuid::new_v4().simple().to_string()[..12];
    let schema = format!("r1_standalone_{short}");
    let (pg_client, pg_conn) = tokio_postgres::connect(&base_url, NoTls)
        .await
        .expect("connect PG");
    tokio::spawn(async move {
        let _ = pg_conn.await;
    });
    pg_client
        .batch_execute(&format!("CREATE SCHEMA {schema}"))
        .await
        .expect("create schema");
    drop(pg_client);

    let iso_url = with_isolated_schema_url(&base_url, &schema);
    let iso_target = ValidatedControlPgTarget::resolve_from_sources(
        None,
        Some(&iso_url),
        None,
        ProfileMode::TestIsolated,
    )
    .expect("resolve")
    .expect("target");

    let managed_root = env::temp_dir().join(format!("dennett-standalone-root-{short}"));
    let proj_ws = env::temp_dir().join(format!("dennett-standalone-projws-{short}"));
    fs::create_dir_all(&proj_ws).expect("create proj ws");
    fs::write(proj_ws.join("readme.txt"), "hello").expect("write readme");

    let (inspecting_rt, captured_starts) = InspectingSimulatedRuntime::new();
    let hmac_signer =
        MemoryTokenSigner::new(b"standalone-chat-hmac-secret-32-bytes!".to_vec()).expect("signer");
    let control_mcp_bin = locate_control_mcp_bin();
    let svc = HeadService::new_with_extras(
        iso_target,
        Arc::new(inspecting_rt),
        None,
        Some(MemoryMcpWiring {
            binary_path: PathBuf::from("dennett-memory-mcp"),
            memory_url: String::from("http://127.0.0.1:18080"),
            memory_token: String::new(),
        }),
        ProfileMode::TestIsolated,
        Vec::new(),
        HeadServiceExtras {
            memory_hmac_signer: Some(hmac_signer),
            control_mcp_bin: Some(control_mcp_bin),
            detached_projects_root: Some(managed_root.clone()),
            ..HeadServiceExtras::default()
        },
    )
    .expect("construct HeadService");
    let _ = svc.ensure_migrated().await.expect("migrate v7");

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let head_url = format!("http://{}", listener.local_addr().expect("addr"));
    svc.set_self_http_url(&head_url);
    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
    let app = build_head_router(HeadAppState {
        service: Some(svc.clone()),
        profile_mode: ProfileMode::TestIsolated,
        runtime_adapter_id: String::from("simulated"),
        runtime_is_simulated: true,
        allow_non_loopback: false,
        shutdown_tx: shutdown_tx.clone(),
        request_limit: Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS)),
    });
    let server_handle = tokio::spawn(async move {
        let _ = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
            while !*shutdown_rx.borrow() {
                if shutdown_rx.changed().await.is_err() {
                    break;
                }
            }
        })
        .await;
    });

    let client = HeadClient::new(&head_url).expect("client");
    let health = client.healthz(None).await.expect("healthz");
    assert_eq!(
        health.schema_version,
        Some(dennett_head::LATEST_CONTROL_SCHEMA_VERSION)
    );

    let project_scope = unique_scope("standalone-proj");
    let standalone_scope = MemoryScope::try_new("chat:standalone").expect("standalone scope");
    let project = client
        .create_project(
            &CreateProjectRequest {
                name: String::from("Disposable Project"),
                workspace_path: Some(proj_ws.to_string_lossy().into_owned()),
                memory_scope: Some(project_scope.clone()),
            },
            None,
        )
        .await
        .expect("create project");

    // Deliberately give the standalone agent `AllowWorkspace` to prove Head FORCES `DenyAll`!
    let standalone_agent = client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Standalone Assistant"),
                description: String::from("General chat without bound project"),
                system_prompt: String::from("Answer questions concisely."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([standalone_scope.clone()]),
                workspace_write_policy: WorkspaceWritePolicy::AllowWorkspace,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
                autonomy_mode: None,
            },
            None,
        )
        .await
        .expect("create standalone agent");

    // 1. Standalone chat creation with `project_id: None` succeeds; `TaskRun` with `project_id: None` fails.
    let bad_task_run = client
        .create_session(
            &CreateSessionRequest {
                agent_id: standalone_agent.agent_id,
                project_id: None,
                title: String::from("Illegal Standalone TaskRun"),
                session_kind: SessionKind::TaskRun,
            },
            None,
        )
        .await;
    assert!(
        matches!(bad_task_run, Err(DennettError::InvalidInput { .. })),
        "standalone TaskRun session must be rejected: {bad_task_run:?}"
    );

    let standalone_session = client
        .create_session(
            &CreateSessionRequest {
                agent_id: standalone_agent.agent_id,
                project_id: None,
                title: String::from("Новый чат"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("create standalone session");
    assert!(standalone_session.project_id.is_none());
    assert!(!standalone_session.pinned);
    assert!(standalone_session.pinned_at.is_none());

    // Project chat in the disposable project (will be closed when project is deleted).
    let project_session = client
        .create_session(
            &CreateSessionRequest {
                agent_id: standalone_agent.agent_id,
                project_id: Some(project.project_id),
                title: String::from("Project Bound Chat"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("create project session");
    assert_eq!(project_session.project_id, Some(project.project_id));

    // 2. Creating a durable task on a standalone session MUST be rejected with InvalidInput!
    let bad_task = client
        .create_task(
            &CreateTaskRequest {
                project_id: project.project_id,
                agent_id: standalone_agent.agent_id,
                session_id: Some(standalone_session.session_id),
                parent_task_id: None,
                title: Some(String::from("Blocked Task")),
                goal: String::from("Must fail"),
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
        matches!(bad_task, Err(DennettError::InvalidInput { .. })),
        "create_task on standalone session must fail with InvalidInput: {bad_task:?}"
    );

    // 3. Live turn on standalone chat:
    //    - Auto-titles from first user message;
    //    - Forces `DenyAll` and isolated `<managed_root>/standalone-sessions/<session_id>` cwd;
    //    - Mounts ONLY `dennett-memory` scoped to `chat:standalone` (NO `dennett-control-mcp`!);
    //    - Emits partial inline artifact cleanly with `project_id = None`;
    //    - Denies `write_file` outside the model even though agent config says `AllowWorkspace`!
    let turn1 = client
        .send_message(
            standalone_session.session_id,
            &SendMessageRequest {
                content: String::from(
                    "План архитектуры кэширования\n[SIM:TOOL:write_file:escape.txt|blocked]\n[SIM:PARTIAL_ARTIFACT:draft.md|# Draft Note]",
                ),
                timeout_ms: Some(15_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("standalone turn 1");
    assert_eq!(turn1.run.status, RunStatus::Succeeded);
    assert_eq!(turn1.session_title, "План архитектуры кэширования");
    assert!(
        turn1
            .permission_decisions
            .iter()
            .any(|d| d.tool_name == "write_file" && d.effect == PolicyEffect::Deny),
        "standalone chat must deny write_file outside model even if agent has AllowWorkspace: {:?}",
        turn1.permission_decisions
    );
    assert!(
        turn1
            .artifacts
            .iter()
            .any(|a| a.name == "draft.md" && a.project_id.is_none()),
        "emitted partial artifact must be persisted with project_id=None: {:?}",
        turn1.artifacts
    );

    // Inspect exact runtime start request captured by InspectingSimulatedRuntime!
    let starts = captured_starts.lock().expect("lock captured").clone();
    assert_eq!(starts.len(), 1);
    let expected_suffix =
        PathBuf::from("standalone-sessions").join(standalone_session.session_id.to_string());
    assert!(
        starts[0].workspace_cwd.ends_with(&expected_suffix),
        "expected cwd under standalone-sessions/<session_id>, got: {:?}",
        starts[0].workspace_cwd
    );
    assert_eq!(
        starts[0].workspace_write_policy,
        WorkspaceWritePolicy::DenyAll
    );
    let ctrl_mcp = starts[0]
        .mcp_servers
        .iter()
        .find(|m| m.name == "dennett-control")
        .expect("dennett-control MCP must be mounted for standalone chats (global skills only)");
    assert!(
        !ctrl_mcp
            .env
            .iter()
            .any(|e| e.name == "DENNETT_CONTROL_PROJECT_ID"),
        "standalone dennett-control MCP must NOT receive DENNETT_CONTROL_PROJECT_ID: {:?}",
        ctrl_mcp.env
    );
    let ctrl_token = ctrl_mcp
        .env
        .iter()
        .find(|e| e.name == "DENNETT_CONTROL_TOKEN")
        .map(|e| e.value.as_str())
        .expect("DENNETT_CONTROL_TOKEN");
    let ctrl_payload_b64 = ctrl_token.split('.').nth(1).expect("dct1 payload segment");
    let ctrl_claims_bytes = URL_SAFE_NO_PAD
        .decode(ctrl_payload_b64)
        .expect("decode dct1 claims");
    let ctrl_claims_json: serde_json::Value =
        serde_json::from_slice(&ctrl_claims_bytes).expect("parse dct1 claims JSON");
    assert!(
        ctrl_claims_json.get("project_id").is_none() || ctrl_claims_json["project_id"].is_null(),
        "standalone dct1 token must have no project_id claim: {ctrl_claims_json}"
    );
    let mem_mcp = starts[0]
        .mcp_servers
        .iter()
        .find(|m| m.name == "dennett-memory")
        .expect("dennett-memory MCP must still be mounted for standalone agent scopes");
    let mem_token = mem_mcp
        .env
        .iter()
        .find(|e| e.name == "DENNETT_MEMORY_TOKEN")
        .map(|e| e.value.as_str())
        .expect("DENNETT_MEMORY_TOKEN");
    let payload_b64 = mem_token.split('.').nth(1).expect("dnt1 payload segment");
    let claims_bytes = URL_SAFE_NO_PAD
        .decode(payload_b64)
        .expect("decode dnt1 claims");
    let claims_json: serde_json::Value =
        serde_json::from_slice(&claims_bytes).expect("parse claims JSON");
    assert_eq!(
        claims_json["allowed_scopes"],
        serde_json::json!(["chat:standalone"]),
        "standalone memory token must carry strictly agent's own scopes without leaking project scopes"
    );

    // 4. Inline artifact creation + lifecycle work cleanly; workspace-backed `relative_path` is rejected!
    let bad_file_artifact = client
        .create_artifact(
            standalone_session.session_id,
            &CreateArtifactRequest {
                run_id: None,
                name: String::from("readme.txt"),
                mime_type: String::from("text/plain"),
                content: None,
                relative_path: Some(String::from("readme.txt")),
                completeness_status: ArtifactCompleteness::Final,
            },
            None,
        )
        .await;
    assert!(
        matches!(bad_file_artifact, Err(DennettError::InvalidInput { .. })),
        "workspace-backed relative_path artifact must be rejected on standalone chat: {bad_file_artifact:?}"
    );

    let inline_art = client
        .create_artifact(
            standalone_session.session_id,
            &CreateArtifactRequest {
                run_id: Some(turn1.run.run_id),
                name: String::from("summary.md"),
                mime_type: String::from("text/markdown"),
                content: Some(String::from("# Standalone Summary")),
                relative_path: None,
                completeness_status: ArtifactCompleteness::Partial,
            },
            None,
        )
        .await
        .expect("create inline artifact on standalone session");
    assert!(inline_art.project_id.is_none());
    let finalized = client
        .apply_artifact_lifecycle(
            standalone_session.session_id,
            inline_art.artifact_id,
            &ArtifactLifecycleRequest {
                action: ArtifactLifecycleAction::Finalize,
                reason: Some(String::from("done")),
            },
            None,
        )
        .await
        .expect("finalize standalone artifact");
    assert_eq!(
        finalized.artifact.completeness_status,
        ArtifactCompleteness::Final
    );
    assert!(finalized.artifact.project_id.is_none());

    // 5. While a slow standalone turn is running, `get_state_snapshot(None, false)` MUST include it in `running_session_ids`!
    let slow_client = HeadClient::new(&head_url).expect("slow client");
    let slow_sid = standalone_session.session_id;
    let slow_handle = tokio::spawn(async move {
        slow_client
            .send_message(
                slow_sid,
                &SendMessageRequest {
                    content: String::from("[SIM:SLOW:700] check running snapshot"),
                    timeout_ms: Some(15_000),
                    client_message_id: None,
                },
                None,
            )
            .await
    });
    let mut observed_running = false;
    for _ in 0..20 {
        let snap = client
            .get_state_snapshot(None, false, None)
            .await
            .expect("snapshot during slow standalone run");
        if snap
            .running_session_ids
            .contains(&standalone_session.session_id)
        {
            observed_running = true;
            break;
        }
        time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        observed_running,
        "get_state_snapshot must include active standalone session in running_session_ids"
    );
    let slow_res = slow_handle
        .await
        .expect("join slow")
        .expect("slow standalone turn succeeded");
    assert_eq!(slow_res.run.status, RunStatus::Succeeded);

    // 6. Archive then delete the project -> project session disappears from snapshot, standalone chat + artifacts survive 100%!
    let _ = client
        .archive_project(project.project_id, None)
        .await
        .expect("archive project");
    let _ = client
        .delete_project(project.project_id, true, None)
        .await
        .expect("delete project");

    let snap_after = client
        .get_state_snapshot(None, true, None)
        .await
        .expect("snapshot after project delete");
    assert!(
        !snap_after
            .sessions
            .iter()
            .any(|s| s.session_id == project_session.session_id),
        "deleted project's session must be excluded from snapshot"
    );
    let surviving = snap_after
        .sessions
        .iter()
        .find(|s| s.session_id == standalone_session.session_id)
        .expect("standalone chat must survive project archive & delete");
    assert!(surviving.project_id.is_none());
    assert_eq!(surviving.status, SessionStatus::Active);

    let detail_after = client
        .get_session_detail(standalone_session.session_id, None)
        .await
        .expect("standalone session detail after project delete");
    assert_eq!(detail_after.session.status, SessionStatus::Active);
    assert!(
        detail_after
            .artifacts
            .iter()
            .any(|a| a.artifact_id == inline_art.artifact_id
                && a.lifecycle_state == ArtifactLifecycleState::Active),
        "standalone artifacts must survive project deletion untouched"
    );

    // Standalone chat still executes new turns after project deletion!
    let turn_after = client
        .send_message(
            standalone_session.session_id,
            &SendMessageRequest {
                content: String::from("Still alive after project deletion"),
                timeout_ms: Some(15_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("standalone chat still works after project delete");
    assert_eq!(turn_after.run.status, RunStatus::Succeeded);

    let _ = shutdown_tx.send(true);
    let _ = server_handle.await;
    drop(svc);
    let _ = fs::remove_dir_all(&managed_root);
    let _ = fs::remove_dir_all(&proj_ws);
    drop_isolated_schema(&base_url, &schema).await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "verifies deterministic pinned-first sorting, pinned_at lifecycle, PATCH pin-only/title-only/combined/empty semantics, and live control_events for projects and sessions against PG16"
)]
async fn v7_pinning_projects_and_sessions_sorts_deterministically_and_emits_events() {
    let (_iso_url, _target, svc, base_url, schema) =
        make_isolated_schema_service_with_extras("pins", None, HeadServiceExtras::default()).await;

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let head_url = format!("http://{}", listener.local_addr().expect("addr"));
    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
    let app = build_head_router(HeadAppState {
        service: Some(svc.clone()),
        profile_mode: ProfileMode::TestIsolated,
        runtime_adapter_id: String::from("simulated"),
        runtime_is_simulated: true,
        allow_non_loopback: false,
        shutdown_tx: shutdown_tx.clone(),
        request_limit: Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS)),
    });
    let server_handle = tokio::spawn(async move {
        let _ = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
            while !*shutdown_rx.borrow() {
                if shutdown_rx.changed().await.is_err() {
                    break;
                }
            }
        })
        .await;
    });

    let client = HeadClient::new(&head_url).expect("client");
    let p1 = client
        .create_project(
            &CreateProjectRequest {
                name: String::from("Project 1"),
                workspace_path: None,
                memory_scope: None,
            },
            None,
        )
        .await
        .expect("p1");
    time::sleep(Duration::from_millis(15)).await;
    let p2 = client
        .create_project(
            &CreateProjectRequest {
                name: String::from("Project 2"),
                workspace_path: None,
                memory_scope: None,
            },
            None,
        )
        .await
        .expect("p2");
    time::sleep(Duration::from_millis(15)).await;
    let p3 = client
        .create_project(
            &CreateProjectRequest {
                name: String::from("Project 3"),
                workspace_path: None,
                memory_scope: None,
            },
            None,
        )
        .await
        .expect("p3");

    let initial_projects = client.list_projects(None).await.expect("list projects");
    let initial_ids: Vec<_> = initial_projects
        .projects
        .iter()
        .map(|p| p.project_id)
        .collect();
    assert_eq!(
        initial_ids,
        vec![p3.project_id, p2.project_id, p1.project_id]
    );

    let cursor_before_pin = client
        .get_state_snapshot(None, false, None)
        .await
        .expect("snap before pin")
        .snapshot_cursor
        .committed_seq;

    // 1. Pin P1 then P2 (pin-only PATCH) -> order must be [P2, P1, P3] in both list_projects and snapshot!
    let p1_pinned = client
        .update_project(
            p1.project_id,
            &UpdateProjectRequest {
                name: None,
                workspace_path: None,
                pinned: Some(true),
            },
            None,
        )
        .await
        .expect("pin p1");
    assert!(p1_pinned.pinned);
    let p1_pinned_at = p1_pinned.pinned_at.expect("p1 pinned_at set");

    time::sleep(Duration::from_millis(15)).await;
    let p2_pinned = client
        .update_project(
            p2.project_id,
            &UpdateProjectRequest {
                name: None,
                workspace_path: None,
                pinned: Some(true),
            },
            None,
        )
        .await
        .expect("pin p2");
    assert!(p2_pinned.pinned);
    assert!(p2_pinned.pinned_at.expect("p2 pinned_at") > p1_pinned_at);

    let after_pin_list = client.list_projects(None).await.expect("list after pin");
    assert_eq!(
        after_pin_list
            .projects
            .iter()
            .map(|p| p.project_id)
            .collect::<Vec<_>>(),
        vec![p2.project_id, p1.project_id, p3.project_id]
    );
    let after_pin_snap = client
        .get_state_snapshot(None, false, None)
        .await
        .expect("snap after pin");
    assert_eq!(
        after_pin_snap
            .projects
            .iter()
            .map(|p| p.project_id)
            .collect::<Vec<_>>(),
        vec![p2.project_id, p1.project_id, p3.project_id]
    );

    // 2. Rename pinned P1 without specifying `pinned` -> preserves `pinned = true` and original `pinned_at`!
    let p1_renamed = client
        .update_project(
            p1.project_id,
            &UpdateProjectRequest {
                name: Some(String::from("Project 1 Renamed")),
                workspace_path: None,
                pinned: None,
            },
            None,
        )
        .await
        .expect("rename pinned p1");
    assert!(p1_renamed.pinned);
    assert_eq!(p1_renamed.pinned_at, Some(p1_pinned_at));
    assert_eq!(p1_renamed.name, "Project 1 Renamed");

    // 3. Unpin P2 -> pinned=false, pinned_at=None, P1 becomes first!
    let p2_unpinned = client
        .update_project(
            p2.project_id,
            &UpdateProjectRequest {
                name: None,
                workspace_path: None,
                pinned: Some(false),
            },
            None,
        )
        .await
        .expect("unpin p2");
    assert!(!p2_unpinned.pinned);
    assert!(p2_unpinned.pinned_at.is_none());
    let after_unpin_projects = client.list_projects(None).await.expect("list after unpin");
    assert_eq!(
        after_unpin_projects
            .projects
            .iter()
            .map(|p| p.project_id)
            .collect::<Vec<_>>(),
        vec![p1.project_id, p3.project_id, p2.project_id]
    );

    // 4. Sessions (both standalone & project chats): pin-only, title-only, combined, and empty rejection!
    let agent = client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Pin Test Agent"),
                description: String::from("Test"),
                system_prompt: String::from("Test"),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::new(),
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

    let c1 = client
        .create_session(
            &CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: None,
                title: String::from("Chat 1"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("c1");
    time::sleep(Duration::from_millis(15)).await;
    let c2 = client
        .create_session(
            &CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: None,
                title: String::from("Chat 2"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("c2");
    time::sleep(Duration::from_millis(15)).await;
    let c3 = client
        .create_session(
            &CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: None,
                title: String::from("Chat 3"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("c3");

    // Empty PATCH /v1/sessions/{id} fails with InvalidInput!
    let empty_patch = client
        .update_session(
            c1.session_id,
            &UpdateSessionRequest {
                title: None,
                pinned: None,
                status: None,
            },
            None,
        )
        .await;
    assert!(
        matches!(empty_patch, Err(DennettError::InvalidInput { .. })),
        "empty PATCH /v1/sessions/{{id}} must fail with InvalidInput: {empty_patch:?}"
    );

    // Pin-only on C1, then pin-only on C2 -> order becomes [C2, C1, C3]!
    let c1_pinned = client
        .update_session(
            c1.session_id,
            &UpdateSessionRequest {
                title: None,
                pinned: Some(true),
                status: None,
            },
            None,
        )
        .await
        .expect("pin c1");
    assert!(c1_pinned.pinned);
    assert_eq!(c1_pinned.title, "Chat 1");
    let c1_pinned_at = c1_pinned.pinned_at.expect("c1 pinned_at");

    time::sleep(Duration::from_millis(15)).await;
    let c2_pinned = client
        .update_session(
            c2.session_id,
            &UpdateSessionRequest {
                title: None,
                pinned: Some(true),
                status: None,
            },
            None,
        )
        .await
        .expect("pin c2");
    assert!(c2_pinned.pinned);
    assert!(c2_pinned.pinned_at.expect("c2 pinned_at") > c1_pinned_at);

    // Title-only on C1 preserves pin & pinned_at!
    let c1_renamed = client
        .update_session(
            c1.session_id,
            &UpdateSessionRequest {
                title: Some(String::from("Chat 1 Renamed")),
                pinned: None,
                status: None,
            },
            None,
        )
        .await
        .expect("rename c1");
    assert_eq!(c1_renamed.title, "Chat 1 Renamed");
    assert!(c1_renamed.pinned);
    assert_eq!(c1_renamed.pinned_at, Some(c1_pinned_at));

    let ordered_sessions = client.list_sessions(None).await.expect("list sessions");
    assert_eq!(
        ordered_sessions
            .sessions
            .iter()
            .map(|s| s.session_id)
            .collect::<Vec<_>>(),
        vec![c2.session_id, c1.session_id, c3.session_id]
    );
    let snap_sessions = client
        .get_state_snapshot(None, false, None)
        .await
        .expect("snap sessions");
    assert_eq!(
        snap_sessions
            .sessions
            .iter()
            .map(|s| s.session_id)
            .collect::<Vec<_>>(),
        vec![c2.session_id, c1.session_id, c3.session_id]
    );

    // Combined title + unpin on C2 -> unpinned and renamed; C1 stays #1!
    let c2_unpinned = client
        .update_session(
            c2.session_id,
            &UpdateSessionRequest {
                title: Some(String::from("Chat 2 Unpinned")),
                pinned: Some(false),
                status: None,
            },
            None,
        )
        .await
        .expect("unpin and rename c2");
    assert_eq!(c2_unpinned.title, "Chat 2 Unpinned");
    assert!(!c2_unpinned.pinned);
    assert!(c2_unpinned.pinned_at.is_none());

    let final_sessions = client.list_sessions(None).await.expect("final sessions");
    assert_eq!(final_sessions.sessions[0].session_id, c1.session_id);
    assert!(final_sessions.sessions[0].pinned);

    // 5. Verify live `project.updated` and `session.updated` control events were emitted with pin metadata!
    let ev_stream = client
        .get_events(cursor_before_pin, None, None, Some(200), None)
        .await
        .expect("get_events");
    assert!(
        ev_stream.events.iter().any(|e| {
            e.event_type == "project.updated"
                && e.project_id == Some(p1.project_id)
                && e.payload_json.contains(r#""pinned":true"#)
        }),
        "expected project.updated control event with pinned=true"
    );
    assert!(
        ev_stream.events.iter().any(|e| {
            e.event_type == "session.updated"
                && e.session_id == Some(c1.session_id)
                && e.project_id.is_none()
                && e.payload_json.contains(r#""pinned":true"#)
        }),
        "expected session.updated control event for standalone session with pinned=true"
    );

    let _ = shutdown_tx.send(true);
    let _ = server_handle.await;
    drop(svc);
    drop_isolated_schema(&base_url, &schema).await;
}
