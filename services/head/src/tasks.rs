//! Durable tasks, PostgreSQL 16 IANA-timezone timers & background scheduler (`HeadScheduler`),
//! idempotent signals, generation fencing (`V07`), native clarification, external budgets,
//! targeted `Stop`, helper delegation (`child_scope ⊆ parent_scope`), and plain-text
//! inter-agent messages (`A07`, `A10`).
//!
//! Architectural boundary (Architecture 80 §1–2, §10):
//! - Zero custom workflow DSL or consumer-cancellation DAG! Cancellation of shared work is
//!   coordinated via plain-text `agent_messages`, never automatic cascading kills.
//! - Semantic decisions stay with the agent; `Head` persists commitments, enforces external
//!   budgets & permissions, and fences stale generations cleanly.

use std::collections::BTreeSet;
use std::str::FromStr;
use std::time::Duration;

use deadpool_postgres::GenericClient;
use dennett_contracts::{
    AgentId, AgentMessageId, AgentMessageRecord, AutonomyMode, BudgetExceedPolicy,
    CancelTaskResponse, CorrelationId, CreateTaskRequest, DennettError, EffectClaimRecord,
    ExternalEffectStatus, InboxCardId, ListAgentMessagesResponse, ListTasksResponse, MessageId,
    MissedRunPolicy, ProjectId, ProjectLifecycleState, ProviderConnectionId, ProviderKind,
    ReplyTaskRequest, RunId, RunStatus, ScheduleKind, SchedulerTickResponse,
    SendAgentMessageRequest, SendAgentMessageResponse, SessionId, SessionKind, TaskDetailResponse,
    TaskId, TaskRecord, TaskSchedule, TaskSignalId, TaskSignalOutcome, TaskSignalRequest,
    TaskSignalResponse, TaskStatus, TaskTimerId, TaskTrigger, TaskUsage, TimerPauseReason,
    TimestampMs, UpdateTaskRequest, UsageConfidence, write_policy_is_subset_of,
};
use dennett_runtime_antigravity::{RuntimeEvent, TurnOutcome};
use serde_json::json;
use tokio::sync::watch;
use tokio::time;

use crate::approvals::DEFAULT_APPROVAL_WAIT_TIMEOUT;
use crate::events::{
    ControlEventAppend, PendingControlEvent, append_control_event_in_tx,
    append_control_events_batch_in_tx,
};
use crate::migrations::sha256_hex;
use crate::service::{
    ActiveRunGuard, ActiveRunHandle, ActiveTaskGuard, ActiveTaskHandle, CoreTurnInput,
    DEFAULT_PG_OP_TIMEOUT, DEFAULT_TURN_TIMEOUT, HeadService, TurnTrigger,
    derive_auto_session_title, insert_permission_decision,
};
use crate::store::{
    AGENT_MESSAGE_SELECT_COLS, EFFECT_CLAIM_SELECT_COLS, INBOX_CARD_SELECT_COLS, TASK_SELECT_COLS,
    TASK_SIGNAL_SELECT_COLS, TASK_TIMER_SELECT_COLS, append_audit_event, decode_agent_message_row,
    decode_effect_claim_row, decode_inbox_card_row, decode_task_row, decode_task_signal_row,
    decode_task_timer_row, ensure_project_active_in_tx, fetch_agent, fetch_policy_rules,
    fetch_project, fetch_runtime_binding, fetch_session, fetch_session_messages, fetch_task,
    fetch_task_runs, map_pg_err, ms_to_pg_epoch_secs, next_stream_seq, truncate_utf8,
};

/// Grace threshold (60s) beyond `fire_at` before an overdue timer triggers `missed_run_policy` (`Skip`/`Ask`).
pub const MISFIRE_GRACE_MS: i64 = 60_000;
/// Lease window held by a scheduler tick while claiming due `task_timers`.
pub const TIMER_CLAIM_LEASE_SECS: f64 = 30.0;
/// Poll interval for the background `HeadScheduler` task inside `dennett-head`.
pub const BACKGROUND_SCHEDULER_POLL_INTERVAL: Duration = Duration::from_millis(500);
/// Maximum concurrent active follow-up scheduled timers a background `TaskRun` may spawn.
pub const MAX_TASK_FOLLOWUP_TIMERS: i64 = 3;

/// Validates an IANA timezone identifier against PostgreSQL 16's native `pg_timezone_names` catalog.
///
/// # Errors
///
/// Returns [`DennettError::InvalidInput`] if PostgreSQL does not recognize `tz`, or
/// [`DennettError::DatabaseUnavailable`] on database failure.
pub async fn verify_pg_timezone<C: GenericClient>(
    client: &C,
    tz: &str,
    target: &crate::pg_target::ValidatedControlPgTarget,
) -> Result<(), DennettError> {
    let exists: bool = client
        .query_one(
            "SELECT EXISTS(SELECT 1 FROM pg_timezone_names WHERE name = $1)",
            &[&tz],
        )
        .await
        .map_err(|err| map_pg_err(target, &err))?
        .get(0);
    if !exists {
        return Err(DennettError::InvalidInput {
            field: String::from("schedule.timezone"),
            reason: format!("unknown IANA timezone `{tz}` in PostgreSQL pg_timezone_names"),
        });
    }
    Ok(())
}

/// Computes the next UTC fire timestamp strictly `> reference_now` (or exact `fire_at` for `OnceAt`)
/// using PostgreSQL 16's native DST-aware `AT TIME ZONE` engine (Contract I, zero extra Rust crates).
///
/// # Errors
///
/// Returns [`DennettError::InvalidInput`] or [`DennettError::DatabaseUnavailable`].
#[expect(
    clippy::too_many_lines,
    reason = "computes OnceAt, Interval, DailyWallTime, WeeklyWallTime, and YearlyWallTime next fire timestamps inside PostgreSQL 16 IANA timezone engine"
)]
pub async fn compute_next_fire_at_pg<C: GenericClient>(
    client: &C,
    schedule: &TaskSchedule,
    reference_now: TimestampMs,
    target: &crate::pg_target::ValidatedControlPgTarget,
) -> Result<Option<TimestampMs>, DennettError> {
    match schedule.kind {
        ScheduleKind::OnceAt { fire_at } => Ok(Some(fire_at)),
        ScheduleKind::Interval { interval_secs } => {
            let delta_ms = i64::try_from(interval_secs.saturating_mul(1_000)).unwrap_or(86_400_000);
            Ok(Some(TimestampMs::from_millis(
                reference_now.as_millis().saturating_add(delta_ms),
            )))
        }
        ScheduleKind::DailyWallTime { hour, minute } => {
            let ref_secs = ms_to_pg_epoch_secs(reference_now.as_millis());
            let h = i32::from(hour);
            let m = i32::from(minute);
            let next_ms: i64 = client
                .query_one(
                    "WITH ref AS (
                         SELECT to_timestamp($1::double precision) AS ref_utc
                     ),
                     local_ref AS (
                         SELECT ref_utc, (ref_utc AT TIME ZONE $2) AS local_ts
                         FROM ref
                     ),
                     candidate AS (
                         SELECT
                             ref_utc,
                             ((date_trunc('day', local_ts) + make_interval(hours => $3, mins => $4)) AT TIME ZONE $2) AS today_utc,
                             ((date_trunc('day', local_ts) + INTERVAL '1 day' + make_interval(hours => $3, mins => $4)) AT TIME ZONE $2) AS tomorrow_utc
                         FROM local_ref
                     )
                     SELECT TRUNC(EXTRACT(EPOCH FROM (CASE WHEN today_utc > ref_utc THEN today_utc ELSE tomorrow_utc END)) * 1000)::bigint
                     FROM candidate",
                    &[&ref_secs, &schedule.timezone.as_str(), &h, &m],
                )
                .await
                .map_err(|err| map_pg_err(target, &err))?
                .get(0);
            Ok(Some(TimestampMs::from_millis(next_ms)))
        }
        ScheduleKind::WeeklyWallTime {
            weekdays_mask,
            hour,
            minute,
        } => {
            let ref_secs = ms_to_pg_epoch_secs(reference_now.as_millis());
            let h = i32::from(hour);
            let m = i32::from(minute);
            let mask = i32::from(weekdays_mask);
            let next_ms: i64 = client
                .query_one(
                    "WITH ref AS (
                         SELECT to_timestamp($1::double precision) AS ref_utc
                     ),
                     local_ref AS (
                         SELECT ref_utc, (ref_utc AT TIME ZONE $2) AS local_ts
                         FROM ref
                     ),
                     candidates AS (
                         SELECT
                             ref_utc,
                             (date_trunc('day', local_ts) + (d * INTERVAL '1 day')) AS local_day,
                             ((date_trunc('day', local_ts) + (d * INTERVAL '1 day') + make_interval(hours => $3, mins => $4)) AT TIME ZONE $2) AS cand_utc
                         FROM local_ref, generate_series(0, 7) AS d
                     )
                     SELECT TRUNC(EXTRACT(EPOCH FROM MIN(cand_utc)) * 1000)::bigint
                     FROM candidates
                     WHERE cand_utc > ref_utc
                       AND ((1 << (EXTRACT(ISODOW FROM local_day)::int - 1)) & $5::int) <> 0",
                    &[&ref_secs, &schedule.timezone.as_str(), &h, &m, &mask],
                )
                .await
                .map_err(|err| map_pg_err(target, &err))?
                .get(0);
            Ok(Some(TimestampMs::from_millis(next_ms)))
        }
        ScheduleKind::YearlyWallTime {
            month,
            day,
            hour,
            minute,
        } => {
            let ref_secs = ms_to_pg_epoch_secs(reference_now.as_millis());
            let mo = i32::from(month);
            let dy = i32::from(day);
            let h = i32::from(hour);
            let m = i32::from(minute);
            let next_ms: i64 = client
                .query_one(
                    "WITH ref AS (
                         SELECT to_timestamp($1::double precision) AS ref_utc
                     ),
                     local_ref AS (
                         SELECT ref_utc, EXTRACT(YEAR FROM (ref_utc AT TIME ZONE $2))::int AS base_year
                         FROM ref
                     ),
                     years AS (
                         SELECT ref_utc, (base_year + offset_y) AS yr
                         FROM local_ref, generate_series(0, 8) AS offset_y
                     ),
                     candidates AS (
                         SELECT
                             ref_utc,
                             CASE
                                 WHEN $3 = 2 AND $4 = 29
                                      AND NOT ((yr % 4 = 0 AND yr % 100 <> 0) OR yr % 400 = 0)
                                     THEN NULL::timestamptz
                                 ELSE (make_timestamp(yr, $3, $4, $5, $6, 0.0) AT TIME ZONE $2)
                             END AS cand_utc
                         FROM years
                     )
                     SELECT TRUNC(EXTRACT(EPOCH FROM MIN(cand_utc)) * 1000)::bigint
                     FROM candidates
                     WHERE cand_utc IS NOT NULL AND cand_utc > ref_utc",
                    &[&ref_secs, &schedule.timezone.as_str(), &mo, &dy, &h, &m],
                )
                .await
                .map_err(|err| map_pg_err(target, &err))?
                .get(0);
            Ok(Some(TimestampMs::from_millis(next_ms)))
        }
        _ => Err(DennettError::InvalidInput {
            field: String::from("schedule.kind"),
            reason: String::from("unsupported schedule kind"),
        }),
    }
}

impl HeadService {
    /// Creates a new durable task/obligation (`POST /v1/tasks`), optionally armed on a PostgreSQL 16
    /// IANA-timezone timer, waiting for an idempotent signal, or queued for immediate execution.
    ///
    /// Enforces helper delegation scope inheritance (`child_scope ⊆ parent_scope`, A07) whenever
    /// `parent_task_id` is provided!
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::NotFound`],
    /// [`DennettError::ForbiddenScope`], [`DennettError::Conflict`], or [`DennettError::DatabaseUnavailable`].
    #[expect(
        clippy::too_many_lines,
        reason = "task creation validates project/agent/delegation boundaries, creates dedicated session if omitted, arms PG timer, and records journal event atomically"
    )]
    pub async fn create_task(
        &self,
        req: CreateTaskRequest,
        correlation_id: &CorrelationId,
    ) -> Result<TaskRecord, DennettError> {
        req.validate()?;
        let _ = self.ensure_migrated().await?;

        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let project = fetch_project(&tx, req.project_id, self.pg_target()).await?;
        if matches!(
            project.lifecycle_state,
            ProjectLifecycleState::Archived | ProjectLifecycleState::DeletedRecord
        ) {
            return Err(DennettError::Conflict {
                resource: String::from("project"),
                expected_revision: None,
                actual_revision: None,
                reason: format!(
                    "project `{}` is `{}` and does not accept new tasks",
                    project.project_id, project.lifecycle_state
                ),
            });
        }
        let agent = fetch_agent(&tx, req.agent_id, self.pg_target()).await?;

        // Enforce helper delegation scope narrowing: `child_scope ⊆ parent_scope` (A07)!
        let mut effective_budget = req
            .budget
            .clone()
            .or_else(|| agent.current_config.default_budget.clone())
            .unwrap_or_default();
        if let Some(parent_id) = req.parent_task_id {
            let parent_task = fetch_task(&tx, parent_id, self.pg_target()).await?;
            if parent_task.project_id != req.project_id {
                return Err(DennettError::ForbiddenScope {
                    scope: format!("project:{}", req.project_id),
                    reason: String::from(
                        "delegated helper task cannot cross outside its parent task's project",
                    ),
                });
            }
            self.enforce_delegated_agent_scope_subset_on_client(
                &tx,
                parent_task.agent_id,
                req.agent_id,
                req.connection_override_id,
            )
            .await?;
            effective_budget = effective_budget.clamp_to_parent(&parent_task.budget);
        }

        if let Some(conn_id) = req.connection_override_id {
            let exists: bool = tx
                .query_one(
                    "SELECT EXISTS(SELECT 1 FROM provider_connections WHERE connection_id = $1)",
                    &[&conn_id.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?
                .get(0);
            if !exists {
                return Err(DennettError::NotFound {
                    resource: String::from("provider_connection"),
                    id: conn_id.to_string(),
                });
            }
        }

        let standalone_pid = crate::policy::standalone_system_project_id();
        if let Some(target_sid) = req.target_session_id {
            let target_sess = fetch_session(&tx, target_sid, self.pg_target()).await?;
            let target_ok = target_sess.project_id == Some(req.project_id)
                || (target_sess.project_id.is_none() && req.project_id == standalone_pid);
            if !target_ok {
                return Err(DennettError::InvalidInput {
                    field: String::from("target_session_id"),
                    reason: String::from("target_session_id does not belong to the task's project"),
                });
            }
        }
        if let Some(notify_sid) = req.notify_session_id {
            let _ = fetch_session(&tx, notify_sid, self.pg_target()).await?;
        }

        let title = req
            .title
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| derive_auto_session_title(&req.goal))
            .unwrap_or_else(|| String::from("Поручение"));

        let mut caller_is_task_run_session = false;
        let session_id = if let Some(sid) = req.session_id {
            let sess = fetch_session(&tx, sid, self.pg_target()).await?;
            let belongs_to_project = sess.project_id == Some(req.project_id)
                || (sess.project_id.is_none() && req.project_id == standalone_pid);
            if !belongs_to_project {
                return Err(DennettError::InvalidInput {
                    field: String::from("session_id"),
                    reason: String::from("session does not belong to the specified project"),
                });
            }
            caller_is_task_run_session = sess.session_kind == SessionKind::TaskRun;
            sid
        } else {
            let sid = SessionId::new();
            tx.execute(
                "INSERT INTO sessions (session_id, agent_id, project_id, title, status, session_kind)
                 VALUES ($1, $2, $3, $4, 'active', 'task_run')",
                &[
                    &sid.as_uuid(),
                    &req.agent_id.as_uuid(),
                    &req.project_id.as_uuid(),
                    &truncate_utf8(&title, 256),
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
            sid
        };

        // Bounded follow-up timers from background `TaskRun` sessions (A-AGT-3): cap active
        // follow-up scheduled tasks per lineage to prevent unbounded timer amplification loops.
        if req.schedule.is_some() && (req.parent_task_id.is_some() || caller_is_task_run_session) {
            let active_followups: i64 = tx
                .query_one(
                    "SELECT COUNT(*)::bigint
                     FROM tasks
                     WHERE deleted_at IS NULL
                       AND schedule_json IS NOT NULL
                       AND status NOT IN ('cancelled', 'succeeded', 'failed')
                       AND (
                           ($1::uuid IS NOT NULL AND parent_task_id = $1)
                           OR ($2::uuid IS NOT NULL AND notify_session_id = $2)
                           OR session_id = $3
                       )",
                    &[
                        &req.parent_task_id.map(TaskId::as_uuid),
                        &req.notify_session_id.map(SessionId::as_uuid),
                        &session_id.as_uuid(),
                    ],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?
                .get(0);
            if active_followups >= MAX_TASK_FOLLOWUP_TIMERS {
                return Err(DennettError::SafetyGuardRejected {
                    reason: format!(
                        "background task_run follow-up timer limit ({MAX_TASK_FOLLOWUP_TIMERS}) reached; refusing unbounded timer spawning"
                    ),
                });
            }
        }

        let now = TimestampMs::now();
        let mut schedule_opt = req.schedule.clone();
        let (initial_status, wait_detail, expected_signal_name, initial_fire_at) =
            if let Some(ref mut sched) = schedule_opt {
                verify_pg_timezone(&tx, &sched.timezone, self.pg_target()).await?;
                let fire_at = compute_next_fire_at_pg(&tx, sched, now, self.pg_target()).await?;
                sched.next_fire_at = fire_at;
                (
                    TaskStatus::WaitingTimer,
                    Some(format!("Ожидает расписания ({})", sched.timezone)),
                    None,
                    fire_at,
                )
            } else if let Some(TaskTrigger::OnRunCompleted {
                run_id: watch_run_id,
            }) = req.trigger
            {
                let run_row = tx
                    .query_opt(
                        "SELECT status FROM runs WHERE run_id = $1",
                        &[&watch_run_id.as_uuid()],
                    )
                    .await
                    .map_err(|err| map_pg_err(self.pg_target(), &err))?;
                let Some(rrow) = run_row else {
                    return Err(DennettError::NotFound {
                        resource: String::from("run"),
                        id: watch_run_id.to_string(),
                    });
                };
                let run_status_str: String = rrow.get(0);
                if run_status_str == "running" {
                    (
                        TaskStatus::WaitingSignal,
                        Some(format!("Ожидает завершения запуска {watch_run_id}")),
                        None,
                        None,
                    )
                } else {
                    (TaskStatus::Queued, None, None, None)
                }
            } else if let Some(ref sig_name) = req.wait_for_signal {
                let trimmed_sig = sig_name.trim().to_owned();
                (
                    TaskStatus::WaitingSignal,
                    Some(format!("Ожидает сигнал {trimmed_sig}")),
                    Some(trimmed_sig),
                    None,
                )
            } else {
                (TaskStatus::Queued, None, None, None)
            };

        // Combine agent's attached skills with task-specific attached skills (deduplicated).
        let mut merged_skills = agent.current_config.attached_skills.clone();
        for skill in &req.attached_skills {
            let s = skill.trim().to_owned();
            if !s.is_empty() && !merged_skills.contains(&s) {
                merged_skills.push(s);
            }
        }

        let task_id = TaskId::new();
        let schedule_json = schedule_opt
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .map_err(|err| DennettError::Internal {
                detail: format!("failed serializing task schedule: {err}"),
            })?;
        let budget_json =
            serde_json::to_value(&effective_budget).map_err(|err| DennettError::Internal {
                detail: format!("failed serializing task budget: {err}"),
            })?;
        let skills_json =
            serde_json::to_value(&merged_skills).map_err(|err| DennettError::Internal {
                detail: format!("failed serializing attached_skills: {err}"),
            })?;
        let auth_override_str = req
            .auth_profile_override
            .map(|p| p.as_method_id().to_owned());
        let thinking_override_str = req.thinking_override.map(|t| t.as_str().to_owned());
        let model_override_trimmed = req
            .model_override
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned);
        let skill_slugs_override_json = req
            .skill_slugs_override
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|err| DennettError::Internal {
                detail: format!("failed serializing skill_slugs_override: {err}"),
            })?;
        let trigger_json = req
            .trigger
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|err| DennettError::Internal {
                detail: format!("failed serializing task trigger: {err}"),
            })?;

        let insert_sql = format!(
            "INSERT INTO tasks (
                task_id, project_id, session_id, agent_id, parent_task_id,
                title, goal, generation, status, wait_detail, expected_signal_name,
                schedule_json, budget_json, attached_skills, auth_profile_override,
                target_session_id, notify_session_id, model_override, thinking_override,
                connection_override_id, skill_slugs_override_json, trigger_json
             ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, 1, $8, $9, $10, $11, $12, $13, $14,
                $15, $16, $17, $18, $19, $20, $21
             )
             RETURNING {TASK_SELECT_COLS}"
        );
        let task_row = tx
            .query_one(
                &insert_sql,
                &[
                    &task_id.as_uuid(),
                    &req.project_id.as_uuid(),
                    &session_id.as_uuid(),
                    &req.agent_id.as_uuid(),
                    &req.parent_task_id.map(TaskId::as_uuid),
                    &truncate_utf8(&title, 256),
                    &req.goal.trim(),
                    &initial_status.as_str(),
                    &wait_detail.as_deref(),
                    &expected_signal_name.as_deref(),
                    &schedule_json,
                    &budget_json,
                    &skills_json,
                    &auth_override_str,
                    &req.target_session_id.map(SessionId::as_uuid),
                    &req.notify_session_id.map(SessionId::as_uuid),
                    &model_override_trimmed.as_deref(),
                    &thinking_override_str.as_deref(),
                    &req.connection_override_id
                        .map(ProviderConnectionId::as_uuid),
                    &skill_slugs_override_json.as_deref(),
                    &trigger_json.as_deref(),
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let task = decode_task_row(&task_row)?;

        if let (Some(sched), Some(fire_at)) = (schedule_opt.as_ref(), initial_fire_at) {
            self.insert_task_timer_in_tx(&tx, task_id, 1, "gen:1:init", sched, fire_at)
                .await?;
        }

        let _ = append_audit_event(
            &tx,
            self.pg_target(),
            correlation_id,
            Some(task.session_id),
            None,
            "head",
            "task_created",
            &self.redact(&format!(
                "created durable task `{}` ({task_id}) status={} gen=1",
                task.title, task.status
            )),
        )
        .await?;

        let _ = append_control_event_in_tx(
            &tx,
            self.pg_target(),
            ControlEventAppend {
                entity_key: &format!("task:{task_id}"),
                event_type: "task.created",
                project_id: Some(task.project_id),
                session_id: Some(task.session_id),
                task_id: Some(task.task_id),
                correlation_id,
                payload: json!({
                    "task_id": task.task_id,
                    "title": task.title,
                    "generation": task.generation,
                    "status": task.status,
                    "parent_task_id": task.parent_task_id,
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        if task.status == TaskStatus::Queued && req.execute_immediately.unwrap_or(true) {
            self.spawn_task_execution(task.task_id, task.generation, correlation_id.clone());
        }

        Ok(task)
    }

    /// Lists persisted durable tasks (`GET /v1/tasks`) with optional `project_id` and `status` filters.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] or [`DennettError::DatabaseUnavailable`].
    pub async fn list_tasks(
        &self,
        project_id: Option<ProjectId>,
        status_filter: Option<&str>,
    ) -> Result<ListTasksResponse, DennettError> {
        let parsed_status = status_filter
            .map(str::trim)
            .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("all"))
            .map(TaskStatus::from_str)
            .transpose()?;
        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let sql = format!(
            "SELECT {TASK_SELECT_COLS}
             FROM tasks
             WHERE deleted_at IS NULL
               AND EXISTS (
                   SELECT 1 FROM projects p
                   WHERE p.project_id = tasks.project_id
                     AND p.lifecycle_state <> 'deleted_record'
               )
               AND ($1::uuid IS NULL OR project_id = $1)
               AND ($2::text IS NULL OR status = $2)
             ORDER BY updated_at DESC"
        );
        let status_str = parsed_status.map(|s| s.as_str().to_owned());
        let rows = conn
            .query(&sql, &[&project_id.map(ProjectId::as_uuid), &status_str])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let tasks = rows
            .iter()
            .map(decode_task_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ListTasksResponse { tasks })
    }

    /// Fetches full durable task detail (`GET /v1/tasks/{task_id}`): task, runs, timers, signals,
    /// tracked external effects, and open Action Inbox cards.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`] or [`DennettError::DatabaseUnavailable`].
    pub async fn get_task_detail(
        &self,
        task_id: TaskId,
    ) -> Result<TaskDetailResponse, DennettError> {
        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let task = fetch_task(&conn, task_id, self.pg_target()).await?;
        let runs = fetch_task_runs(&conn, task_id, self.pg_target()).await?;

        let timer_sql = format!(
            "SELECT {TASK_TIMER_SELECT_COLS} FROM task_timers WHERE task_id = $1 ORDER BY created_at ASC"
        );
        let timers = conn
            .query(&timer_sql, &[&task_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?
            .iter()
            .map(decode_task_timer_row)
            .collect::<Result<Vec<_>, _>>()?;

        let signal_sql = format!(
            "SELECT {TASK_SIGNAL_SELECT_COLS} FROM task_signals WHERE task_id = $1 ORDER BY created_at ASC"
        );
        let signals = conn
            .query(&signal_sql, &[&task_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?
            .iter()
            .map(decode_task_signal_row)
            .collect::<Result<Vec<_>, _>>()?;

        let effect_sql = format!(
            "SELECT {EFFECT_CLAIM_SELECT_COLS} FROM effect_claims WHERE task_id = $1 ORDER BY created_at ASC"
        );
        let effects = conn
            .query(&effect_sql, &[&task_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?
            .iter()
            .map(decode_effect_claim_row)
            .collect::<Result<Vec<_>, _>>()?;

        let inbox_sql = format!(
            "SELECT {INBOX_CARD_SELECT_COLS} FROM inbox_cards WHERE task_id = $1 AND status = 'open' ORDER BY created_at DESC"
        );
        let open_inbox_cards = conn
            .query(&inbox_sql, &[&task_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?
            .iter()
            .map(decode_inbox_card_row)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(TaskDetailResponse {
            task,
            runs,
            timers,
            signals,
            effects,
            open_inbox_cards,
        })
    }

    /// Updates a task's goal, title, schedule, or budget (`PATCH /v1/tasks/{task_id}`), atomically
    /// incrementing `generation` to fence any late-finishing older worker (`RunStatus::Superseded`, V07).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::NotFound`], [`DennettError::Conflict`],
    /// or [`DennettError::DatabaseUnavailable`].
    #[expect(
        clippy::too_many_lines,
        reason = "task goal/schedule update atomically bumps generation, cancels stale timers/cards, arms replacement schedule, and signals old-generation preemption"
    )]
    pub async fn update_task(
        &self,
        task_id: TaskId,
        req: UpdateTaskRequest,
        correlation_id: &CorrelationId,
    ) -> Result<TaskRecord, DennettError> {
        req.validate()?;
        let _ = self.ensure_migrated().await?;

        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let lock_sql = format!(
            "SELECT {TASK_SELECT_COLS} FROM tasks WHERE task_id = $1 AND deleted_at IS NULL FOR UPDATE"
        );
        let Some(row) = tx
            .query_opt(&lock_sql, &[&task_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?
        else {
            return Err(DennettError::NotFound {
                resource: String::from("task"),
                id: task_id.to_string(),
            });
        };
        let existing = decode_task_row(&row)?;
        let _ =
            ensure_project_active_in_tx(&tx, existing.project_id, "update task", self.pg_target())
                .await?;

        if existing.status == TaskStatus::UnknownExternal {
            return Err(DennettError::Conflict {
                resource: String::from("task"),
                expected_revision: Some(existing.generation),
                actual_revision: Some(existing.generation),
                reason: String::from(
                    "cannot update task goal while an external effect is in `unknown_external`; reconcile effect first",
                ),
            });
        }
        if existing.status == TaskStatus::Cancelled {
            return Err(DennettError::Conflict {
                resource: String::from("task"),
                expected_revision: Some(existing.generation),
                actual_revision: Some(existing.generation),
                reason: String::from(
                    "cannot modify a stopped/cancelled task; create a new task instead",
                ),
            });
        }

        let goal_or_schedule_changed =
            req.goal.is_some() || req.schedule.is_some() || req.clear_schedule;
        let next_generation = if goal_or_schedule_changed {
            existing.generation + 1
        } else {
            existing.generation
        };
        let next_title = req
            .title
            .as_deref()
            .map(str::trim)
            .map_or_else(|| existing.title.clone(), ToOwned::to_owned);
        let next_goal = req
            .goal
            .as_deref()
            .map(str::trim)
            .map_or_else(|| existing.goal.clone(), ToOwned::to_owned);
        let next_budget = req
            .budget
            .clone()
            .unwrap_or_else(|| existing.budget.clone());

        let now = TimestampMs::now();
        let mut next_schedule = if req.clear_schedule {
            None
        } else if req.schedule.is_some() {
            req.schedule.clone()
        } else {
            existing.schedule.clone()
        };

        let mut new_timer_fire_at: Option<TimestampMs> = None;
        let mut expired_perm_ids: Vec<dennett_contracts::PermissionRequestId> = Vec::new();
        let (next_status, next_wait_detail, next_expected_signal_name) = if goal_or_schedule_changed
        {
            // Cancel stale timers of previous generations.
            tx.execute(
                "UPDATE task_timers
                 SET status = 'cancelled'
                 WHERE task_id = $1 AND status IN ('pending', 'claimed', 'paused')",
                &[&task_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

            // Expire pending interactive permission requests from superseded generation (H2-04).
            let expired_perm_rows = tx
                .query(
                    "UPDATE permission_requests
                     SET status = 'expired',
                         revision = revision + 1,
                         resolved_at = now()
                     WHERE task_id = $1 AND status = 'pending'
                     RETURNING request_id",
                    &[&task_id.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;
            expired_perm_ids = expired_perm_rows
                .iter()
                .map(|r| dennett_contracts::PermissionRequestId::from_uuid(r.get(0)))
                .collect();

            // Supersede stale open clarification / missed_schedule / budget_exceeded / permission_approval cards (H2-04).
            tx.execute(
                "UPDATE inbox_cards
                 SET status = 'superseded',
                     resolution_summary = 'Цель поручения обновлена владельцем',
                     revision = revision + 1,
                     resolved_at = now(),
                     updated_at = now()
                 WHERE task_id = $1
                   AND status = 'open'
                   AND kind IN ('clarification', 'missed_schedule', 'budget_exceeded', 'permission_approval')",
                &[&task_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

            if let Some(ref mut sched) = next_schedule {
                verify_pg_timezone(&tx, &sched.timezone, self.pg_target()).await?;
                let fire_at = compute_next_fire_at_pg(&tx, sched, now, self.pg_target()).await?;
                sched.next_fire_at = fire_at;
                new_timer_fire_at = fire_at;
                (
                    TaskStatus::WaitingTimer,
                    Some(format!("Ожидает расписания ({})", sched.timezone)),
                    None,
                )
            } else if existing.status == TaskStatus::WaitingSignal && !req.clear_schedule {
                // Keep WaitingSignal parked on its expected signal when only goal/budget is edited (H2-04).
                (
                    TaskStatus::WaitingSignal,
                    existing.wait_detail.clone(),
                    existing.expected_signal_name.clone(),
                )
            } else {
                (TaskStatus::Queued, None, None)
            }
        } else {
            (
                existing.status,
                existing.wait_detail.clone(),
                existing.expected_signal_name.clone(),
            )
        };

        let schedule_json = next_schedule
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .map_err(|err| DennettError::Internal {
                detail: format!("failed serializing updated task schedule: {err}"),
            })?;
        let budget_json =
            serde_json::to_value(&next_budget).map_err(|err| DennettError::Internal {
                detail: format!("failed serializing updated task budget: {err}"),
            })?;

        let update_sql = format!(
            "UPDATE tasks
             SET title = $2,
                 goal = $3,
                 generation = $4,
                 status = $5,
                 wait_detail = $6,
                 expected_signal_name = $7,
                 schedule_json = $8,
                 budget_json = $9,
                 finished_at = CASE WHEN $5 IN ('queued', 'waiting_timer', 'waiting_signal') THEN NULL ELSE finished_at END,
                 updated_at = now()
             WHERE task_id = $1
             RETURNING {TASK_SELECT_COLS}"
        );
        let updated_row = tx
            .query_one(
                &update_sql,
                &[
                    &task_id.as_uuid(),
                    &truncate_utf8(&next_title, 256),
                    &next_goal,
                    &next_generation,
                    &next_status.as_str(),
                    &next_wait_detail.as_deref(),
                    &next_expected_signal_name.as_deref(),
                    &schedule_json,
                    &budget_json,
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let updated = decode_task_row(&updated_row)?;

        if let (Some(sched), Some(fire_at)) = (next_schedule.as_ref(), new_timer_fire_at) {
            let timer_key = format!("gen:{next_generation}:init");
            self.insert_task_timer_in_tx(&tx, task_id, next_generation, &timer_key, sched, fire_at)
                .await?;
        }

        let _ = append_control_event_in_tx(
            &tx,
            self.pg_target(),
            ControlEventAppend {
                entity_key: &format!("task:{task_id}"),
                event_type: "task.updated",
                project_id: Some(updated.project_id),
                session_id: Some(updated.session_id),
                task_id: Some(task_id),
                correlation_id,
                payload: json!({
                    "task_id": task_id,
                    "previous_generation": existing.generation,
                    "generation": updated.generation,
                    "status": updated.status,
                    "goal": updated.goal,
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        // Non-blocking preemption signal for any in-flight older generation run; correctness does
        // NOT depend on whether preemption beats the model response because `complete_task_run`
        // strictly fences on `task.generation == run_generation` (V07)!
        if goal_or_schedule_changed {
            for perm_id in expired_perm_ids {
                self.wake_approval_waiter(
                    perm_id,
                    crate::approvals::ApprovalResolutionSignal {
                        outcome: dennett_runtime_antigravity::PolicyEvaluationOutcome {
                            effect: dennett_contracts::PolicyEffect::Deny,
                            matched_rule_id: None,
                            reason: String::from(
                                "superseded: task goal or schedule updated by owner",
                            ),
                            scope: format!("task:{task_id}"),
                        },
                        decision_record: None,
                    },
                )
                .await;
            }
            self.signal_old_task_generation_preempt(task_id, updated.generation)
                .await;
            if updated.status == TaskStatus::Queued && req.execute_immediately.unwrap_or(true) {
                self.spawn_task_execution(task_id, updated.generation, correlation_id.clone());
            }
        }

        Ok(updated)
    }

    /// Delivers an idempotent signal to a task (`POST /v1/tasks/{task_id}/signals`).
    ///
    /// Enforces `UNIQUE (task_id, idempotency_key)` + SHA-256 payload conflict check:
    /// - Same key + same payload -> `deduplicated = true`, ZERO double execution!
    /// - Same key + different payload -> `409 IdempotencyMismatch`!
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::NotFound`],
    /// [`DennettError::IdempotencyMismatch`], or [`DennettError::DatabaseUnavailable`].
    #[expect(
        clippy::too_many_lines,
        reason = "signal delivery locks task, enforces idempotency + SHA-256 conflict check, wakes waiting_signal tasks, and appends journal event atomically"
    )]
    pub async fn send_task_signal(
        &self,
        task_id: TaskId,
        req: TaskSignalRequest,
        correlation_id: &CorrelationId,
    ) -> Result<TaskSignalResponse, DennettError> {
        req.validate()?;
        let _ = self.ensure_migrated().await?;

        let safe_payload = self.redact(&req.payload_text);
        let fp_src = format!("{}\0{safe_payload}", req.signal_name.trim());
        let payload_sha = sha256_hex(fp_src.as_bytes());

        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        // Lock task row FIRST so concurrent retries with the same `idempotency_key` serialize cleanly!
        let lock_sql = format!(
            "SELECT {TASK_SELECT_COLS} FROM tasks WHERE task_id = $1 AND deleted_at IS NULL FOR UPDATE"
        );
        let Some(task_row) = tx
            .query_opt(&lock_sql, &[&task_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?
        else {
            return Err(DennettError::NotFound {
                resource: String::from("task"),
                id: task_id.to_string(),
            });
        };
        let task = decode_task_row(&task_row)?;
        let _ =
            ensure_project_active_in_tx(&tx, task.project_id, "send task signal", self.pg_target())
                .await?;

        let existing_sig_sql = format!(
            "SELECT {TASK_SIGNAL_SELECT_COLS}
             FROM task_signals
             WHERE task_id = $1 AND idempotency_key = $2"
        );
        if let Some(existing_row) = tx
            .query_opt(
                &existing_sig_sql,
                &[&task_id.as_uuid(), &req.idempotency_key.as_str()],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?
        {
            let existing_sig = decode_task_signal_row(&existing_row)?;
            if existing_sig.payload_sha256 != payload_sha {
                return Err(DennettError::IdempotencyMismatch {
                    idempotency_key: req.idempotency_key.to_string(),
                    reason: String::from(
                        "reused signal idempotency_key with different signal_name or payload_text",
                    ),
                });
            }
            tx.commit()
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;
            return Ok(TaskSignalResponse {
                signal: existing_sig,
                task,
                deduplicated: true,
            });
        }

        let incoming_sig_name = req.signal_name.trim();
        let (outcome_status, should_wake, updated_task) = match task.status {
            TaskStatus::Cancelled | TaskStatus::UnknownExternal => {
                (TaskSignalOutcome::RejectedStopped, false, task)
            }
            TaskStatus::WaitingSignal => {
                let signal_matches = task
                    .expected_signal_name
                    .as_deref()
                    .map(str::trim)
                    .is_none_or(|expected| expected == incoming_sig_name);
                if signal_matches {
                    let wake_sql = format!(
                        "UPDATE tasks
                         SET status = 'queued',
                             wait_detail = NULL,
                             expected_signal_name = NULL,
                             updated_at = now()
                         WHERE task_id = $1
                         RETURNING {TASK_SELECT_COLS}"
                    );
                    let woke_row = tx
                        .query_one(&wake_sql, &[&task_id.as_uuid()])
                        .await
                        .map_err(|err| map_pg_err(self.pg_target(), &err))?;
                    (
                        TaskSignalOutcome::WokeTask,
                        true,
                        decode_task_row(&woke_row)?,
                    )
                } else {
                    // Non-matching signal is recorded idempotently without waking WaitingSignal (H2-04).
                    (TaskSignalOutcome::QueuedOnTask, false, task)
                }
            }
            _ => (TaskSignalOutcome::QueuedOnTask, false, task),
        };

        let signal_id = TaskSignalId::new();
        let insert_sig_sql = format!(
            "INSERT INTO task_signals (
                signal_id, task_id, idempotency_key, signal_name, payload_text,
                payload_sha256, applied_generation, outcome_status, correlation_id
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             RETURNING {TASK_SIGNAL_SELECT_COLS}"
        );
        let sig_row = tx
            .query_one(
                &insert_sig_sql,
                &[
                    &signal_id.as_uuid(),
                    &task_id.as_uuid(),
                    &req.idempotency_key.as_str(),
                    &req.signal_name.trim(),
                    &safe_payload,
                    &payload_sha.as_str(),
                    &Some(updated_task.generation),
                    &outcome_status.as_str(),
                    &correlation_id.as_str(),
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let signal = decode_task_signal_row(&sig_row)?;

        let _ = append_control_event_in_tx(
            &tx,
            self.pg_target(),
            ControlEventAppend {
                entity_key: &format!("task:{task_id}"),
                event_type: "task.signal_received",
                project_id: Some(updated_task.project_id),
                session_id: Some(updated_task.session_id),
                task_id: Some(task_id),
                correlation_id,
                payload: json!({
                    "signal_id": signal.signal_id,
                    "signal_name": signal.signal_name,
                    "outcome_status": signal.outcome_status,
                    "generation": updated_task.generation,
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        if should_wake {
            // D2-BUG-01: Signal payload is durably persisted in `task_signals (consumed_in_generation IS NULL)`
            // and atomically consumed inside `execute_task_generation` — surviving any Head restart!
            self.spawn_task_execution(task_id, updated_task.generation, correlation_id.clone());
        }

        Ok(TaskSignalResponse {
            signal,
            task: updated_task,
            deduplicated: false,
        })
    }

    /// Answers an agent's native clarification request (`POST /v1/tasks/{task_id}/reply`),
    /// resolving the open `clarification` Inbox card, persisting the reply in
    /// `tasks.pending_continuation_input` (`D2-BUG-01`), and continuing task execution cleanly.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::NotFound`], [`DennettError::Conflict`],
    /// or [`DennettError::DatabaseUnavailable`].
    pub async fn reply_to_task(
        &self,
        task_id: TaskId,
        req: ReplyTaskRequest,
        correlation_id: &CorrelationId,
    ) -> Result<TaskRecord, DennettError> {
        self.reply_to_task_with_card_guard(task_id, req, None, correlation_id)
            .await
    }

    /// Internal clarification-reply implementation supporting an atomic optimistic-concurrency
    /// `(card_id, expected_revision)` guard when invoked via `resolve_inbox_card` (`D2-BUG-04`).
    #[expect(
        clippy::too_many_lines,
        reason = "persists clarification continuation input, enforces optimistic card guard, emits journal events, and schedules next task generation"
    )]
    pub(crate) async fn reply_to_task_with_card_guard(
        &self,
        task_id: TaskId,
        req: ReplyTaskRequest,
        card_revision_guard: Option<(InboxCardId, i64)>,
        correlation_id: &CorrelationId,
    ) -> Result<TaskRecord, DennettError> {
        req.validate()?;
        let _ = self.ensure_migrated().await?;

        let safe_answer = self.redact(req.answer_text.trim());
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let lock_sql = format!(
            "SELECT {TASK_SELECT_COLS} FROM tasks WHERE task_id = $1 AND deleted_at IS NULL FOR UPDATE"
        );
        let Some(row) = tx
            .query_opt(&lock_sql, &[&task_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?
        else {
            return Err(DennettError::NotFound {
                resource: String::from("task"),
                id: task_id.to_string(),
            });
        };
        let existing = decode_task_row(&row)?;
        let _ = ensure_project_active_in_tx(
            &tx,
            existing.project_id,
            "reply to task",
            self.pg_target(),
        )
        .await?;

        if existing.status != TaskStatus::WaitingClarification {
            return Err(DennettError::Conflict {
                resource: String::from("task"),
                expected_revision: None,
                actual_revision: None,
                reason: format!(
                    "task `{task_id}` is `{}` and not awaiting clarification",
                    existing.status
                ),
            });
        }

        let summary = truncate_utf8(&safe_answer, 2000).to_owned();
        if let Some((card_id, expected_rev)) = card_revision_guard {
            let updated_cards = tx
                .execute(
                    "UPDATE inbox_cards
                     SET status = 'resolved',
                         resolution_summary = $2,
                         revision = revision + 1,
                         resolved_at = now(),
                         updated_at = now()
                     WHERE card_id = $1
                       AND task_id = $3
                       AND kind = 'clarification'
                       AND status = 'open'
                       AND revision = $4",
                    &[
                        &card_id.as_uuid(),
                        &summary,
                        &task_id.as_uuid(),
                        &expected_rev,
                    ],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;
            if updated_cards == 0 {
                return Err(DennettError::Conflict {
                    resource: String::from("inbox_card"),
                    expected_revision: Some(expected_rev),
                    actual_revision: None,
                    reason: String::from(
                        "clarification inbox card was modified or already resolved concurrently",
                    ),
                });
            }
        } else {
            tx.execute(
                "UPDATE inbox_cards
                 SET status = 'resolved',
                     resolution_summary = $2,
                     revision = revision + 1,
                     resolved_at = now(),
                     updated_at = now()
                 WHERE task_id = $1 AND kind = 'clarification' AND status = 'open'",
                &[&task_id.as_uuid(), &summary],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        }

        let continuation_input = format!("[User Clarification Reply]\n{safe_answer}");
        let update_sql = format!(
            "UPDATE tasks
             SET status = 'queued',
                 wait_detail = NULL,
                 pending_continuation_input = $2,
                 updated_at = now()
             WHERE task_id = $1
             RETURNING {TASK_SELECT_COLS}"
        );
        let updated_row = tx
            .query_one(&update_sql, &[&task_id.as_uuid(), &continuation_input])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let updated = decode_task_row(&updated_row)?;

        let _ = append_control_event_in_tx(
            &tx,
            self.pg_target(),
            ControlEventAppend {
                entity_key: &format!("task:{task_id}"),
                event_type: "task.clarification_answered",
                project_id: Some(updated.project_id),
                session_id: Some(updated.session_id),
                task_id: Some(task_id),
                correlation_id,
                payload: json!({
                    "task_id": task_id,
                    "generation": updated.generation,
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        self.spawn_task_execution(task_id, updated.generation, correlation_id.clone());

        Ok(updated)
    }

    /// Targeted `Stop` for ONE durable task (`POST /v1/tasks/{task_id}/cancel`).
    ///
    /// Guarantees:
    /// - Halts ONLY `task_id` — NEVER touches sibling running/waiting tasks or helper child tasks!
    /// - Immediately blocks any new tools/effects and cancels pending approval requests & timers.
    /// - Honestly reports `external_outcome_unknown = true` (`TaskStatus::UnknownExternal`) if any
    ///   external effect is `unknown` or was `dispatching` (NEVER claiming guaranteed external cancel!).
    /// - NEVER calls or blocks on `dennett-memoryd`!
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`] or [`DennettError::DatabaseUnavailable`].
    #[expect(
        clippy::too_many_lines,
        reason = "targeted Stop coordinates immediate in-memory signal, pending approval withdrawal, timer cancel, honest effect inspection, and atomic status transition"
    )]
    pub async fn cancel_task(
        &self,
        task_id: TaskId,
        correlation_id: &CorrelationId,
    ) -> Result<CancelTaskResponse, DennettError> {
        let _ = self.ensure_migrated().await?;

        // 1. Signal immediate cancellation ONLY for this `task_id` BEFORE touching PG!
        let active_handle_info = self.signal_task_cancel_only(task_id).await;
        if let Some((session_id, run_id)) = active_handle_info {
            let _ = self
                .signal_session_run_if_matching(session_id, run_id)
                .await;
            let _ = self.runtime_router().cancel_turn_all(session_id).await;
        }

        // 2. Withdraw pending interactive approval requests for THIS task ONLY!
        let _ = self
            .cancel_pending_permissions_for_scope(
                None,
                Some(task_id),
                "task stopped by owner",
                correlation_id,
            )
            .await?;

        // 3. Persist cancellation, cancel timers, inspect external effects honestly.
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let lock_sql = format!(
            "SELECT {TASK_SELECT_COLS} FROM tasks WHERE task_id = $1 AND deleted_at IS NULL FOR UPDATE"
        );
        let Some(row) = tx
            .query_opt(&lock_sql, &[&task_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?
        else {
            return Err(DennettError::NotFound {
                resource: String::from("task"),
                id: task_id.to_string(),
            });
        };
        let existing = decode_task_row(&row)?;

        // Cancel armed/paused timers for this task ONLY.
        tx.execute(
            "UPDATE task_timers
             SET status = 'cancelled'
             WHERE task_id = $1 AND status IN ('pending', 'claimed', 'paused')",
            &[&task_id.as_uuid()],
        )
        .await
        .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        // Withdraw open non-effect inbox cards for this task (keep `unknown_effect` cards open!).
        tx.execute(
            "UPDATE inbox_cards
             SET status = 'withdrawn',
                 resolution_summary = 'Поручение остановлено владельцем',
                 revision = revision + 1,
                 resolved_at = now(),
                 updated_at = now()
             WHERE task_id = $1
               AND status = 'open'
               AND kind <> 'unknown_effect'",
            &[&task_id.as_uuid()],
        )
        .await
        .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        // Any external effect currently `dispatching` transitions honestly to `unknown`!
        let stranded_sql = format!(
            "UPDATE effect_claims
             SET status = 'unknown',
                 error_summary = COALESCE(error_summary, 'остановка вызвана во время внешнего запроса; исход неизвестен'),
                 updated_at = now()
             WHERE task_id = $1 AND status = 'dispatching'
             RETURNING {EFFECT_CLAIM_SELECT_COLS}"
        );
        let stranded_rows = tx
            .query(&stranded_sql, &[&task_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let mut pending_events = Vec::with_capacity(stranded_rows.len() + 1);
        for srow in &stranded_rows {
            let claim = decode_effect_claim_row(srow)?;
            self.mark_unknown_effect_side_effects_in_tx(&tx, &claim, &mut pending_events)
                .await?;
        }

        let effects_sql = format!(
            "SELECT {EFFECT_CLAIM_SELECT_COLS}
             FROM effect_claims
             WHERE task_id = $1
             ORDER BY created_at ASC"
        );
        let all_effects: Vec<EffectClaimRecord> = tx
            .query(&effects_sql, &[&task_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?
            .iter()
            .map(decode_effect_claim_row)
            .collect::<Result<Vec<_>, _>>()?;

        let committed_effects: Vec<EffectClaimRecord> = all_effects
            .iter()
            .filter(|e| e.status == ExternalEffectStatus::Confirmed)
            .cloned()
            .collect();
        let unknown_effects: Vec<EffectClaimRecord> = all_effects
            .iter()
            .filter(|e| {
                matches!(
                    e.status,
                    ExternalEffectStatus::Unknown | ExternalEffectStatus::Dispatching
                )
            })
            .cloned()
            .collect();
        let external_outcome_unknown = !unknown_effects.is_empty();

        let (next_status, wait_detail, event_type) = if external_outcome_unknown {
            (
                TaskStatus::UnknownExternal,
                Some(String::from(
                    "Остановлено: новые действия заблокированы, но внешний исход уже отправленного эффекта неизвестен (требуется сверка)",
                )),
                "task.stopped_with_unknown_external",
            )
        } else {
            (TaskStatus::Cancelled, None, "task.cancelled")
        };

        let update_sql = format!(
            "UPDATE tasks
             SET status = $2,
                 wait_detail = $3,
                 active_run_id = NULL,
                 finished_at = CASE WHEN $2 = 'cancelled' THEN COALESCE(finished_at, now()) ELSE finished_at END,
                 updated_at = now()
             WHERE task_id = $1
             RETURNING {TASK_SELECT_COLS}"
        );
        let updated_row = tx
            .query_one(
                &update_sql,
                &[
                    &task_id.as_uuid(),
                    &next_status.as_str(),
                    &wait_detail.as_deref(),
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let updated_task = decode_task_row(&updated_row)?;

        let _ = append_audit_event(
            &tx,
            self.pg_target(),
            correlation_id,
            Some(updated_task.session_id),
            active_handle_info.map(|(_, rid)| rid).or(existing.active_run_id),
            "head",
            event_type,
            &format!(
                "targeted Stop on task {task_id} -> status={} external_outcome_unknown={external_outcome_unknown}",
                updated_task.status
            ),
        )
        .await?;

        pending_events.push(PendingControlEvent {
            entity_key: format!("task:{task_id}"),
            event_type,
            project_id: Some(updated_task.project_id),
            session_id: Some(updated_task.session_id),
            task_id: Some(task_id),
            payload: json!({
                "task_id": task_id,
                "status": updated_task.status,
                "external_outcome_unknown": external_outcome_unknown,
                "committed_effects_count": committed_effects.len(),
                "unknown_effects_count": unknown_effects.len(),
            }),
        });
        let _ = append_control_events_batch_in_tx(
            &tx,
            self.pg_target(),
            correlation_id,
            &pending_events,
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        if active_handle_info.is_none() {
            self.clear_stopped_task(task_id).await;
        }

        Ok(CancelTaskResponse {
            task: updated_task,
            cancelled_run_id: active_handle_info
                .map(|(_, rid)| rid)
                .or(existing.active_run_id),
            external_outcome_unknown,
            committed_effects,
            unknown_effects,
            memory_status: self.cached_memory_status(),
        })
    }

    /// Triggers immediate execution of an idle/scheduled/paused task (`POST /v1/tasks/{task_id}/run-now`).
    ///
    /// Rejects tasks that are actively `running` or blocked on `unknown_external` with `409 Conflict`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`], [`DennettError::Conflict`], or [`DennettError::DatabaseUnavailable`].
    pub async fn run_task_now(
        &self,
        task_id: TaskId,
        correlation_id: &CorrelationId,
    ) -> Result<TaskRecord, DennettError> {
        let _ = self.ensure_migrated().await?;
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let lock_sql = format!(
            "SELECT {TASK_SELECT_COLS} FROM tasks WHERE task_id = $1 AND deleted_at IS NULL FOR UPDATE"
        );
        let Some(row) = tx
            .query_opt(&lock_sql, &[&task_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?
        else {
            return Err(DennettError::NotFound {
                resource: String::from("task"),
                id: task_id.to_string(),
            });
        };
        let existing = decode_task_row(&row)?;
        let _ =
            ensure_project_active_in_tx(&tx, existing.project_id, "run task now", self.pg_target())
                .await?;
        if matches!(
            existing.status,
            TaskStatus::Running | TaskStatus::UnknownExternal
        ) {
            return Err(DennettError::Conflict {
                resource: String::from("task"),
                expected_revision: Some(existing.generation),
                actual_revision: Some(existing.generation),
                reason: format!(
                    "cannot run task `{task_id}` now while status is `{}`",
                    existing.status
                ),
            });
        }

        let update_sql = format!(
            "UPDATE tasks
             SET status = 'queued',
                 paused_manually = FALSE,
                 wait_detail = NULL,
                 error_code = NULL,
                 error_summary = NULL,
                 finished_at = NULL,
                 updated_at = now()
             WHERE task_id = $1
             RETURNING {TASK_SELECT_COLS}"
        );
        let updated_row = tx
            .query_one(&update_sql, &[&task_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let updated = decode_task_row(&updated_row)?;

        let _ = append_control_event_in_tx(
            &tx,
            self.pg_target(),
            ControlEventAppend {
                entity_key: &format!("task:{task_id}"),
                event_type: "task.run_now_triggered",
                project_id: Some(updated.project_id),
                session_id: Some(updated.session_id),
                task_id: Some(task_id),
                correlation_id,
                payload: json!({
                    "task_id": task_id,
                    "generation": updated.generation,
                    "status": updated.status,
                }),
            },
        )
        .await?;
        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        self.spawn_task_execution(task_id, updated.generation, correlation_id.clone());
        Ok(updated)
    }

    /// Manually pauses a scheduled or waiting task (`POST /v1/tasks/{task_id}/pause`), suspending
    /// its pending timers without erasing its schedule definition.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`], [`DennettError::Conflict`], or [`DennettError::DatabaseUnavailable`].
    pub async fn pause_task(
        &self,
        task_id: TaskId,
        correlation_id: &CorrelationId,
    ) -> Result<TaskRecord, DennettError> {
        self.pause_task_with_detail(task_id, "Приостановлено владельцем", correlation_id)
            .await
    }

    /// Pauses a non-terminal task with a caller-specified localized `wait_detail` (used by both
    /// manual `POST /v1/tasks/{task_id}/pause` and non-destructive `POST /v1/runs/emergency-stop`).
    pub(crate) async fn pause_task_with_detail(
        &self,
        task_id: TaskId,
        wait_detail: &str,
        correlation_id: &CorrelationId,
    ) -> Result<TaskRecord, DennettError> {
        let _ = self.ensure_migrated().await?;
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let lock_sql = format!(
            "SELECT {TASK_SELECT_COLS} FROM tasks WHERE task_id = $1 AND deleted_at IS NULL FOR UPDATE"
        );
        let Some(row) = tx
            .query_opt(&lock_sql, &[&task_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?
        else {
            return Err(DennettError::NotFound {
                resource: String::from("task"),
                id: task_id.to_string(),
            });
        };
        let existing = decode_task_row(&row)?;
        if matches!(
            existing.status,
            TaskStatus::Cancelled
                | TaskStatus::Succeeded
                | TaskStatus::Failed
                | TaskStatus::UnknownExternal
        ) {
            return Err(DennettError::Conflict {
                resource: String::from("task"),
                expected_revision: Some(existing.generation),
                actual_revision: Some(existing.generation),
                reason: format!(
                    "cannot pause task `{task_id}` in terminal/unknown state `{}`",
                    existing.status
                ),
            });
        }

        tx.execute(
            "UPDATE task_timers
             SET status = 'paused',
                 pause_reason = NULL
             WHERE task_id = $1 AND status = 'pending'",
            &[&task_id.as_uuid()],
        )
        .await
        .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let next_status = if existing.status == TaskStatus::Queued {
            TaskStatus::WaitingTimer
        } else {
            existing.status
        };
        let safe_wait_detail = truncate_utf8(wait_detail, 2048);
        let update_sql = format!(
            "UPDATE tasks
             SET paused_manually = TRUE,
                 status = $2,
                 wait_detail = $3,
                 updated_at = now()
             WHERE task_id = $1
             RETURNING {TASK_SELECT_COLS}"
        );
        let updated_row = tx
            .query_one(
                &update_sql,
                &[&task_id.as_uuid(), &next_status.as_str(), &safe_wait_detail],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let updated = decode_task_row(&updated_row)?;

        let _ = append_control_event_in_tx(
            &tx,
            self.pg_target(),
            ControlEventAppend {
                entity_key: &format!("task:{task_id}"),
                event_type: "task.paused",
                project_id: Some(updated.project_id),
                session_id: Some(updated.session_id),
                task_id: Some(task_id),
                correlation_id,
                payload: json!({
                    "task_id": task_id,
                    "status": updated.status,
                }),
            },
        )
        .await?;
        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        Ok(updated)
    }

    /// Resumes a manually paused task (`POST /v1/tasks/{task_id}/resume`), re-arming its pending
    /// schedule timers (or queuing it when unscheduled).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`], [`DennettError::Conflict`], or [`DennettError::DatabaseUnavailable`].
    #[expect(
        clippy::too_many_lines,
        reason = "resuming a task restores manually paused timers or recomputes next future schedule atomically"
    )]
    pub async fn resume_task(
        &self,
        task_id: TaskId,
        correlation_id: &CorrelationId,
    ) -> Result<TaskRecord, DennettError> {
        let _ = self.ensure_migrated().await?;
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let lock_sql = format!(
            "SELECT {TASK_SELECT_COLS} FROM tasks WHERE task_id = $1 AND deleted_at IS NULL FOR UPDATE"
        );
        let Some(row) = tx
            .query_opt(&lock_sql, &[&task_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?
        else {
            return Err(DennettError::NotFound {
                resource: String::from("task"),
                id: task_id.to_string(),
            });
        };
        let existing = decode_task_row(&row)?;
        let _ =
            ensure_project_active_in_tx(&tx, existing.project_id, "resume task", self.pg_target())
                .await?;
        if matches!(
            existing.status,
            TaskStatus::Cancelled
                | TaskStatus::Succeeded
                | TaskStatus::Failed
                | TaskStatus::UnknownExternal
        ) {
            return Err(DennettError::Conflict {
                resource: String::from("task"),
                expected_revision: Some(existing.generation),
                actual_revision: Some(existing.generation),
                reason: format!(
                    "cannot resume task `{task_id}` in terminal/unknown state `{}`",
                    existing.status
                ),
            });
        }

        let restored = tx
            .execute(
                "UPDATE task_timers
                 SET status = 'pending'
                 WHERE task_id = $1
                   AND status = 'paused'
                   AND pause_reason IS NULL",
                &[&task_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let has_pending_timer: bool = restored > 0
            || tx
                .query_one(
                    "SELECT EXISTS(
                        SELECT 1 FROM task_timers
                        WHERE task_id = $1 AND status = 'pending'
                    )",
                    &[&task_id.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?
                .get(0);

        let mut next_sched_opt = existing.schedule.clone();
        let mut armed_timer = has_pending_timer;
        if !armed_timer
            && let Some(ref mut sched) = next_sched_opt
            && let Some(next_fire) =
                compute_next_fire_at_pg(&tx, sched, TimestampMs::now(), self.pg_target()).await?
        {
            sched.next_fire_at = Some(next_fire);
            let next_key = format!(
                "gen:{}:resume:{}",
                existing.generation,
                next_fire.as_millis()
            );
            self.insert_task_timer_in_tx(
                &tx,
                task_id,
                existing.generation,
                &next_key,
                sched,
                next_fire,
            )
            .await?;
            armed_timer = true;
        }

        let (next_status, next_wait_detail) = if armed_timer {
            let tz = next_sched_opt
                .as_ref()
                .map_or("UTC", |s| s.timezone.as_str());
            (
                TaskStatus::WaitingTimer,
                Some(format!("Ожидает расписания ({tz})")),
            )
        } else if existing.status == TaskStatus::WaitingSignal
            || existing.status == TaskStatus::WaitingClarification
            || existing.status == TaskStatus::WaitingApproval
        {
            (existing.status, existing.wait_detail.clone())
        } else {
            (TaskStatus::Queued, None)
        };

        let sched_json = next_sched_opt
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .map_err(|err| DennettError::Internal {
                detail: format!("failed serializing resumed schedule: {err}"),
            })?;
        let update_sql = format!(
            "UPDATE tasks
             SET paused_manually = FALSE,
                 status = $2,
                 wait_detail = $3,
                 schedule_json = COALESCE($4, schedule_json),
                 updated_at = now()
             WHERE task_id = $1
             RETURNING {TASK_SELECT_COLS}"
        );
        let updated_row = tx
            .query_one(
                &update_sql,
                &[
                    &task_id.as_uuid(),
                    &next_status.as_str(),
                    &next_wait_detail.as_deref(),
                    &sched_json,
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let updated = decode_task_row(&updated_row)?;

        let _ = append_control_event_in_tx(
            &tx,
            self.pg_target(),
            ControlEventAppend {
                entity_key: &format!("task:{task_id}"),
                event_type: "task.resumed",
                project_id: Some(updated.project_id),
                session_id: Some(updated.session_id),
                task_id: Some(task_id),
                correlation_id,
                payload: json!({
                    "task_id": task_id,
                    "status": updated.status,
                }),
            },
        )
        .await?;
        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        if updated.status == TaskStatus::Queued {
            self.spawn_task_execution(task_id, updated.generation, correlation_id.clone());
        }
        Ok(updated)
    }

    /// Soft-deletes a non-running durable task (`DELETE /v1/tasks/{task_id}`), rejecting active
    /// `running` or `unknown_external` tasks with `409 Conflict`, cancelling pending timers and
    /// open non-effect inbox cards, and preserving V15 append-only audit/journal invariants.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`], [`DennettError::Conflict`], or [`DennettError::DatabaseUnavailable`].
    #[expect(
        clippy::too_many_lines,
        reason = "soft-deletes task atomically while rejecting active runs, cleaning open inbox/permissions, and emitting journal + audit events"
    )]
    pub async fn delete_task(
        &self,
        task_id: TaskId,
        correlation_id: &CorrelationId,
    ) -> Result<(), DennettError> {
        let _ = self.ensure_migrated().await?;
        {
            let active = self.active_tasks_map();
            let guard = active.lock().await;
            if guard.contains_key(&task_id) {
                return Err(DennettError::Conflict {
                    resource: String::from("task"),
                    expected_revision: None,
                    actual_revision: None,
                    reason: format!(
                        "cannot delete task `{task_id}` while it is actively running; cancel it first"
                    ),
                });
            }
        }

        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let lock_sql = format!(
            "SELECT {TASK_SELECT_COLS} FROM tasks WHERE task_id = $1 AND deleted_at IS NULL FOR UPDATE"
        );
        let Some(row) = tx
            .query_opt(&lock_sql, &[&task_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?
        else {
            return Err(DennettError::NotFound {
                resource: String::from("task"),
                id: task_id.to_string(),
            });
        };
        let existing = decode_task_row(&row)?;
        if matches!(
            existing.status,
            TaskStatus::Running | TaskStatus::UnknownExternal
        ) {
            return Err(DennettError::Conflict {
                resource: String::from("task"),
                expected_revision: Some(existing.generation),
                actual_revision: Some(existing.generation),
                reason: format!(
                    "cannot delete task `{task_id}` while status is `{}`",
                    existing.status
                ),
            });
        }

        tx.execute(
            "UPDATE task_timers
             SET status = 'cancelled'
             WHERE task_id = $1 AND status IN ('pending', 'claimed', 'paused')",
            &[&task_id.as_uuid()],
        )
        .await
        .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        tx.execute(
            "UPDATE inbox_cards
             SET status = 'withdrawn',
                 resolution_summary = 'Поручение удалено владельцем',
                 revision = revision + 1,
                 resolved_at = now(),
                 updated_at = now()
             WHERE task_id = $1 AND status = 'open' AND kind <> 'unknown_effect'",
            &[&task_id.as_uuid()],
        )
        .await
        .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        tx.execute(
            "UPDATE tasks
             SET deleted_at = now(),
                 status = CASE
                     WHEN status IN ('succeeded', 'failed', 'cancelled') THEN status
                     ELSE 'cancelled'
                 END,
                 finished_at = COALESCE(finished_at, now()),
                 updated_at = now()
             WHERE task_id = $1",
            &[&task_id.as_uuid()],
        )
        .await
        .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let _ = append_audit_event(
            &tx,
            self.pg_target(),
            correlation_id,
            Some(existing.session_id),
            None,
            "head",
            "task_deleted",
            &format!("deleted task `{}` ({task_id})", existing.title),
        )
        .await?;
        let _ = append_control_event_in_tx(
            &tx,
            self.pg_target(),
            ControlEventAppend {
                entity_key: &format!("task:{task_id}"),
                event_type: "task.deleted",
                project_id: Some(existing.project_id),
                session_id: Some(existing.session_id),
                task_id: Some(task_id),
                correlation_id,
                payload: json!({ "task_id": task_id }),
            },
        )
        .await?;
        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        Ok(())
    }

    /// Wakes any non-deleted tasks parked in `WaitingSignal` with
    /// `trigger = TaskTrigger::OnRunCompleted { run_id }` right after `run_id` finishes.
    pub(crate) async fn wake_tasks_waiting_for_run(
        &self,
        completed_run_id: RunId,
        correlation_id: &CorrelationId,
    ) {
        let Ok(mut conn) = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await else {
            return;
        };
        let Ok(tx) = conn.transaction().await else {
            return;
        };
        let sql = format!(
            "SELECT {TASK_SELECT_COLS}
             FROM tasks
             WHERE deleted_at IS NULL
               AND NOT paused_manually
               AND status = 'waiting_signal'
               AND trigger_json IS NOT NULL
             FOR UPDATE SKIP LOCKED"
        );
        let Ok(rows) = tx.query(&sql, &[]).await else {
            return;
        };

        let mut to_spawn: Vec<(TaskId, i64)> = Vec::new();
        let mut pending_events: Vec<PendingControlEvent> = Vec::new();
        for row in &rows {
            let Ok(task) = decode_task_row(row) else {
                continue;
            };
            let matches_run = matches!(
                task.trigger,
                Some(TaskTrigger::OnRunCompleted { run_id }) if run_id == completed_run_id
            );
            if !matches_run {
                continue;
            }
            if tx
                .execute(
                    "UPDATE tasks
                     SET status = 'queued',
                         wait_detail = NULL,
                         updated_at = now()
                     WHERE task_id = $1",
                    &[&task.task_id.as_uuid()],
                )
                .await
                .is_ok()
            {
                pending_events.push(PendingControlEvent {
                    entity_key: format!("task:{}", task.task_id),
                    event_type: "task.woke_on_run_completed",
                    project_id: Some(task.project_id),
                    session_id: Some(task.session_id),
                    task_id: Some(task.task_id),
                    payload: json!({
                        "task_id": task.task_id,
                        "completed_run_id": completed_run_id,
                        "generation": task.generation,
                    }),
                });
                to_spawn.push((task.task_id, task.generation));
            }
        }

        if to_spawn.is_empty() {
            let _ = tx.commit().await;
            return;
        }
        let _ = append_control_events_batch_in_tx(
            &tx,
            self.pg_target(),
            correlation_id,
            &pending_events,
        )
        .await;
        if tx.commit().await.is_ok() {
            for (tid, gen_num) in to_spawn {
                self.spawn_task_execution(tid, gen_num, correlation_id.clone());
            }
        }
    }

    /// Executes one deterministic scheduler pass at `now` (`HeadScheduler`), claiming due
    /// `task_timers` strictly via `FOR UPDATE OF t SKIP LOCKED`, applying `missed_run_policy`
    /// (`execute_once | skip | ask`), advancing recurring schedules via PostgreSQL 16 IANA TZ,
    /// and waking ready tasks without duplicate storms (A10, Contract I).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::DatabaseUnavailable`] on PostgreSQL error.
    #[expect(
        clippy::too_many_lines,
        reason = "scheduler tick coordinates SKIP LOCKED claim, generation check, misfire policy branching, next-occurrence scheduling, and post-commit task spawn"
    )]
    pub async fn tick_scheduler_at(
        &self,
        now: TimestampMs,
        correlation_id: &CorrelationId,
    ) -> Result<SchedulerTickResponse, DennettError> {
        let _ = self.ensure_migrated().await?;
        let now_secs = ms_to_pg_epoch_secs(now.as_millis());
        let lease_owner = format!("head-{}", std::process::id());

        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let claim_sql = format!(
            "WITH due AS (
                SELECT t.timer_id AS due_timer_id
                FROM task_timers t
                JOIN tasks tk ON tk.task_id = t.task_id
                JOIN projects p ON p.project_id = tk.project_id
                WHERE tk.deleted_at IS NULL
                  AND NOT tk.paused_manually
                  AND p.lifecycle_state IN ('active', 'detached')
                  AND t.fire_at <= to_timestamp($1::double precision)
                  AND (
                      t.status = 'pending'
                      OR (t.status = 'claimed' AND t.lease_expires_at <= to_timestamp($1::double precision))
                  )
                ORDER BY t.fire_at ASC
                LIMIT 32
                FOR UPDATE OF t SKIP LOCKED
             )
             UPDATE task_timers t
             SET status = 'claimed',
                 lease_owner = $2,
                 lease_generation = t.lease_generation + 1,
                 lease_expires_at = to_timestamp($1::double precision + $3::double precision)
             FROM due
             WHERE t.timer_id = due.due_timer_id
             RETURNING {TASK_TIMER_SELECT_COLS}"
        );
        let claimed_rows = tx
            .query(
                &claim_sql,
                &[&now_secs, &lease_owner, &TIMER_CLAIM_LEASE_SECS],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let claimed_timers = claimed_rows.len();
        let mut woken_tasks = 0_usize;
        let mut skipped_timers = 0_usize;
        let mut asked_timers = 0_usize;
        let mut to_spawn: Vec<(TaskId, i64)> = Vec::new();
        let mut pending_events: Vec<PendingControlEvent> = Vec::with_capacity(claimed_timers);

        for row in &claimed_rows {
            let timer = decode_task_timer_row(row)?;
            let task_sql =
                format!("SELECT {TASK_SELECT_COLS} FROM tasks WHERE task_id = $1 FOR UPDATE");
            let task_row = tx
                .query_one(&task_sql, &[&timer.task_id.as_uuid()])
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;
            let task = decode_task_row(&task_row)?;
            let is_paused: bool = tx
                .query_one(
                    "SELECT paused_manually FROM tasks WHERE task_id = $1",
                    &[&timer.task_id.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?
                .get(0);
            if is_paused {
                tx.execute(
                    "UPDATE task_timers
                     SET status = 'paused',
                         pause_reason = NULL
                     WHERE timer_id = $1",
                    &[&timer.timer_id.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;
                continue;
            }

            // Fence stale generation or stopped/unknown tasks!
            if timer.task_generation != task.generation
                || matches!(
                    task.status,
                    TaskStatus::Cancelled | TaskStatus::UnknownExternal
                )
            {
                tx.execute(
                    "UPDATE task_timers SET status = 'cancelled' WHERE timer_id = $1",
                    &[&timer.timer_id.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;
                continue;
            }

            let overdue_ms = now.as_millis().saturating_sub(timer.fire_at.as_millis());
            let is_misfired = overdue_ms > MISFIRE_GRACE_MS;

            if is_misfired && timer.missed_run_policy == MissedRunPolicy::Skip {
                tx.execute(
                    "UPDATE task_timers SET status = 'skipped' WHERE timer_id = $1",
                    &[&timer.timer_id.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;

                if let Some(ref sched) = task.schedule
                    && !matches!(sched.kind, ScheduleKind::OnceAt { .. })
                    && let Some(next_fire) =
                        compute_next_fire_at_pg(&tx, sched, now, self.pg_target()).await?
                {
                    let mut next_sched = sched.clone();
                    next_sched.next_fire_at = Some(next_fire);
                    let sched_val = serde_json::to_value(&next_sched).map_err(|err| {
                        DennettError::Internal {
                            detail: format!("failed serializing schedule: {err}"),
                        }
                    })?;
                    tx.execute(
                        "UPDATE tasks SET schedule_json = $2, updated_at = now() WHERE task_id = $1",
                        &[&task.task_id.as_uuid(), &sched_val],
                    )
                    .await
                    .map_err(|err| map_pg_err(self.pg_target(), &err))?;
                    let next_key =
                        format!("gen:{}:next:{}", task.generation, next_fire.as_millis());
                    self.insert_task_timer_in_tx(
                        &tx,
                        task.task_id,
                        task.generation,
                        &next_key,
                        &next_sched,
                        next_fire,
                    )
                    .await?;
                }

                pending_events.push(PendingControlEvent {
                    entity_key: format!("task:{}", task.task_id),
                    event_type: "task.timer_skipped",
                    project_id: Some(task.project_id),
                    session_id: Some(task.session_id),
                    task_id: Some(task.task_id),
                    payload: json!({
                        "timer_id": timer.timer_id,
                        "overdue_ms": overdue_ms,
                        "policy": "skip",
                    }),
                });
                skipped_timers += 1;
            } else if is_misfired && timer.missed_run_policy == MissedRunPolicy::Ask {
                tx.execute(
                    "UPDATE task_timers
                     SET status = 'paused',
                         pause_reason = 'misfire_ask'
                     WHERE timer_id = $1",
                    &[&timer.timer_id.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;

                tx.execute(
                    "UPDATE tasks
                     SET status = 'waiting_approval',
                         wait_detail = 'Пропущено окно расписания — подтвердите запуск или пропуск',
                         updated_at = now()
                     WHERE task_id = $1",
                    &[&task.task_id.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;

                let card_id = InboxCardId::new();
                let subject = truncate_utf8(
                    &format!("Пропущен запуск по расписанию: {}", task.title),
                    500,
                )
                .to_owned();
                let detail = format!(
                    "Таймер поручения «{}» ({}) просрочен на {} сек. Запустить сейчас один раз или пропустить до следующего окна?",
                    task.title,
                    timer.timezone,
                    overdue_ms / 1_000
                );
                tx.execute(
                    "INSERT INTO inbox_cards (
                        card_id, project_id, session_id, task_id, agent_id,
                        kind, status, urgency, subject, detail
                     ) VALUES ($1, $2, $3, $4, $5, 'missed_schedule', 'open', 'normal', $6, $7)",
                    &[
                        &card_id.as_uuid(),
                        &task.project_id.as_uuid(),
                        &task.session_id.as_uuid(),
                        &task.task_id.as_uuid(),
                        &task.agent_id.as_uuid(),
                        &subject,
                        &detail,
                    ],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;

                pending_events.push(PendingControlEvent {
                    entity_key: format!("task:{}", task.task_id),
                    event_type: "task.schedule_missed",
                    project_id: Some(task.project_id),
                    session_id: Some(task.session_id),
                    task_id: Some(task.task_id),
                    payload: json!({
                        "timer_id": timer.timer_id,
                        "card_id": card_id,
                        "overdue_ms": overdue_ms,
                        "policy": "ask",
                    }),
                });
                asked_timers += 1;
            } else {
                // On-time firing OR `MissedRunPolicy::ExecuteOnce` (coalesces any downtime into 1 run!).
                tx.execute(
                    "UPDATE task_timers
                     SET status = 'fired',
                         fired_at = to_timestamp($2)
                     WHERE timer_id = $1",
                    &[&timer.timer_id.as_uuid(), &now_secs],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;

                if let Some(ref sched) = task.schedule
                    && !matches!(sched.kind, ScheduleKind::OnceAt { .. })
                    && let Some(next_fire) =
                        compute_next_fire_at_pg(&tx, sched, now, self.pg_target()).await?
                {
                    let mut next_sched = sched.clone();
                    next_sched.next_fire_at = Some(next_fire);
                    let sched_val = serde_json::to_value(&next_sched).map_err(|err| {
                        DennettError::Internal {
                            detail: format!("failed serializing next schedule: {err}"),
                        }
                    })?;
                    tx.execute(
                        "UPDATE tasks SET schedule_json = $2, updated_at = now() WHERE task_id = $1",
                        &[&task.task_id.as_uuid(), &sched_val],
                    )
                    .await
                    .map_err(|err| map_pg_err(self.pg_target(), &err))?;
                    let next_key =
                        format!("gen:{}:next:{}", task.generation, next_fire.as_millis());
                    self.insert_task_timer_in_tx(
                        &tx,
                        task.task_id,
                        task.generation,
                        &next_key,
                        &next_sched,
                        next_fire,
                    )
                    .await?;
                }

                tx.execute(
                    "UPDATE tasks
                     SET status = 'queued',
                         wait_detail = NULL,
                         finished_at = NULL,
                         updated_at = now()
                     WHERE task_id = $1",
                    &[&task.task_id.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;

                pending_events.push(PendingControlEvent {
                    entity_key: format!("task:{}", task.task_id),
                    event_type: "task.timer_fired",
                    project_id: Some(task.project_id),
                    session_id: Some(task.session_id),
                    task_id: Some(task.task_id),
                    payload: json!({
                        "timer_id": timer.timer_id,
                        "task_generation": task.generation,
                        "overdue_ms": overdue_ms,
                        "policy": timer.missed_run_policy,
                    }),
                });
                woken_tasks += 1;
                to_spawn.push((task.task_id, task.generation));
            }
        }

        // H2-02: Also pick up durable `tasks.status = 'queued'` that survived Head restart
        // (excluding tasks already scheduled in this tick or currently active in memory).
        let (_active_run_ids, active_task_ids) = self.active_run_and_task_ids().await;
        let queued_rows = tx
            .query(
                "SELECT tk.task_id, tk.generation
                 FROM tasks tk
                 JOIN projects p ON p.project_id = tk.project_id
                 WHERE tk.deleted_at IS NULL
                   AND NOT tk.paused_manually
                   AND tk.status = 'queued'
                   AND p.lifecycle_state IN ('active', 'detached')
                 ORDER BY tk.updated_at ASC
                 LIMIT 16",
                &[],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        for qrow in queued_rows {
            let tid = TaskId::from_uuid(qrow.get(0));
            let gen_num: i64 = qrow.get(1);
            if active_task_ids.contains(&tid) || to_spawn.iter().any(|(id, _)| *id == tid) {
                continue;
            }
            woken_tasks += 1;
            to_spawn.push((tid, gen_num));
        }

        let _ = append_control_events_batch_in_tx(
            &tx,
            self.pg_target(),
            correlation_id,
            &pending_events,
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        for (tid, gen_num) in to_spawn {
            self.spawn_task_execution(tid, gen_num, correlation_id.clone());
        }

        Ok(SchedulerTickResponse {
            now,
            claimed_timers,
            woken_tasks,
            skipped_timers,
            asked_timers,
        })
    }

    /// Atomic startup crash-recovery pass executed on `dennett-head` boot and scheduler startup
    /// BEFORE the first timer tick (H2-02 / A10).
    ///
    /// Safely excludes any in-memory `active_runs` / `active_tasks` belonging to the live process so
    /// repeated calls are 100% idempotent:
    /// 1. Marks stale `dispatching` external effects as `unknown` (`UNKNOWN`, never blindly retried).
    /// 2. Expires orphaned `pending` `permission_requests` and their `open` `permission_approval` inbox cards.
    /// 3. Finalizes stranded `runs.status = 'running'` as `failed` (`HEAD_RESTARTED_DURING_RUN`).
    /// 4. Transitions interrupted `running` and orphaned-approval `waiting_approval` tasks safely:
    ///    - to `unknown_external` if any `unknown`/`prepared`/`dispatching` effect exists,
    ///    - to `failed` (`HEAD_RESTARTED_DURING_RUN`) if a `confirmed` external effect already ran on that run,
    ///    - to `queued` when zero external effects occurred (ready for automatic scheduler pickup!).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::DatabaseUnavailable`] on PostgreSQL failure.
    /// Runs [`Self::recover_after_startup`] at most once per `HeadService` process lifecycle,
    /// serialized behind `startup_recovery_lock()` so `run_serve` and `spawn_background_scheduler`
    /// never execute duplicate concurrent recovery passes against live HTTP traffic.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::DatabaseUnavailable`] on PostgreSQL failure.
    pub async fn recover_after_startup_once(
        &self,
        correlation_id: &CorrelationId,
    ) -> Result<usize, DennettError> {
        let mut done = self.startup_recovery_lock().lock().await;
        if *done {
            return Ok(0);
        }
        let recovered = self.recover_after_startup(correlation_id).await?;
        *done = true;
        Ok(recovered)
    }

    /// Atomic startup crash-recovery pass executed on `dennett-head` boot BEFORE accepting traffic
    /// (H2-02 / A10).
    ///
    /// Safely excludes any in-memory `active_runs` / `active_tasks` belonging to the live process so
    /// repeated calls are idempotent:
    /// 1. Marks stale `dispatching` external effects as `unknown` (`UNKNOWN`, never blindly retried).
    /// 2. Expires orphaned `pending` `permission_requests` and their `open` `permission_approval` cards.
    /// 3. Finalizes stranded `runs.status = 'running'` as `failed` (`HEAD_RESTARTED_DURING_RUN`).
    /// 4. Transitions interrupted `running` and orphaned-approval `waiting_approval` tasks safely
    ///    (`unknown_external`, `failed` if confirmed effect occurred, or `queued` for scheduler pickup).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::DatabaseUnavailable`] on PostgreSQL failure.
    #[expect(
        clippy::too_many_lines,
        reason = "startup crash recovery atomically reconciles orphaned approvals, stranded runs, and interrupted tasks in one transaction"
    )]
    pub async fn recover_after_startup(
        &self,
        correlation_id: &CorrelationId,
    ) -> Result<usize, DennettError> {
        let _ = self.ensure_migrated().await?;
        let stale_effects = self
            .recover_stale_dispatching_effects(correlation_id)
            .await?;
        let (active_run_ids, active_task_ids) = self.active_run_and_task_ids().await;
        let active_run_uuids: Vec<uuid::Uuid> =
            active_run_ids.iter().map(|id| id.as_uuid()).collect();
        let active_task_uuids: Vec<uuid::Uuid> =
            active_task_ids.iter().map(|id| id.as_uuid()).collect();

        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let mut pending_events: Vec<PendingControlEvent> = Vec::new();
        let restart_perm_summary =
            "Процесс dennett-head был перезапущен во время ожидания подтверждения";
        let expired_perm_rows = tx
            .query(
                "UPDATE permission_requests
                 SET status = 'expired',
                     revision = revision + 1,
                     resolved_at = now()
                 WHERE status = 'pending'
                   AND NOT (run_id = ANY($1))
                 RETURNING request_id, project_id, session_id, task_id",
                &[&active_run_uuids],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        for row in &expired_perm_rows {
            let perm_id = dennett_contracts::PermissionRequestId::from_uuid(row.get(0));
            let project_id = ProjectId::from_uuid(row.get(1));
            let session_id = SessionId::from_uuid(row.get(2));
            let task_id = row.get::<_, Option<uuid::Uuid>>(3).map(TaskId::from_uuid);
            tx.execute(
                "UPDATE inbox_cards
                 SET status = 'expired',
                     resolution_summary = $2,
                     revision = revision + 1,
                     resolved_at = now(),
                     updated_at = now()
                 WHERE permission_request_id = $1 AND status = 'open'",
                &[&perm_id.as_uuid(), &restart_perm_summary],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

            let entity_key = task_id.map_or_else(
                || format!("session:{session_id}"),
                |tid| format!("task:{tid}"),
            );
            pending_events.push(PendingControlEvent {
                entity_key,
                event_type: "permission.expired",
                project_id: Some(project_id),
                session_id: Some(session_id),
                task_id,
                payload: json!({
                    "request_id": perm_id,
                    "reason": restart_perm_summary,
                }),
            });
        }

        // Belt-and-suspenders: close any open `permission_approval` inbox card from dead runs.
        tx.execute(
            "UPDATE inbox_cards
             SET status = 'expired',
                 resolution_summary = $2,
                 revision = revision + 1,
                 resolved_at = now(),
                 updated_at = now()
             WHERE kind = 'permission_approval'
               AND status = 'open'
               AND (run_id IS NULL OR NOT (run_id = ANY($1)))",
            &[&active_run_uuids, &restart_perm_summary],
        )
        .await
        .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let stranded_run_rows = tx
            .query(
                "UPDATE runs
                 SET status = 'failed',
                     error_code = 'HEAD_RESTARTED_DURING_RUN',
                     error_summary = 'Выполнение прервано перезапуском dennett-head',
                     finished_at = now()
                 WHERE status = 'running'
                   AND NOT (run_id = ANY($1))
                 RETURNING run_id, session_id",
                &[&active_run_uuids],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let stranded_runs = stranded_run_rows.len() as u64;
        if !stranded_run_rows.is_empty() {
            let stranded_ids: Vec<uuid::Uuid> =
                stranded_run_rows.iter().map(|r| r.get(0)).collect();
            let stranded_session_ids: Vec<uuid::Uuid> =
                stranded_run_rows.iter().map(|r| r.get(1)).collect();
            // D2-BUG-04: Return undelivered inter-agent messages from crashed runs back to queue!
            tx.execute(
                "UPDATE agent_messages
                 SET delivered_in_run_id = NULL, delivered_at = NULL
                 WHERE delivered_in_run_id = ANY($1)",
                &[&stranded_ids],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
            // Safe pause any queued follow-ups on sessions whose in-flight run died on restart.
            tx.execute(
                "UPDATE session_followups
                 SET status = 'paused', updated_at = now()
                 WHERE session_id = ANY($1) AND status = 'queued'",
                &[&stranded_session_ids],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        }

        let interrupted_sql = format!(
            "SELECT {TASK_SELECT_COLS}
             FROM tasks
             WHERE deleted_at IS NULL
               AND status IN ('running', 'waiting_approval')
               AND NOT (task_id = ANY($1))
             FOR UPDATE"
        );
        let interrupted_rows = tx
            .query(&interrupted_sql, &[&active_task_uuids])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let mut recovered_tasks = 0_usize;
        for row in &interrupted_rows {
            let task = decode_task_row(row)?;
            if task.status == TaskStatus::WaitingApproval {
                // Keep tasks paused on durable `budget_exceeded` or `missed_schedule` inbox cards intact!
                let has_durable_approval_card: bool = tx
                    .query_one(
                        "SELECT EXISTS (
                            SELECT 1 FROM inbox_cards
                            WHERE task_id = $1
                              AND status = 'open'
                              AND kind IN ('budget_exceeded', 'missed_schedule')
                        )",
                        &[&task.task_id.as_uuid()],
                    )
                    .await
                    .map_err(|err| map_pg_err(self.pg_target(), &err))?
                    .get(0);
                if has_durable_approval_card {
                    continue;
                }
            }

            let has_unknown_effect: bool = tx
                .query_one(
                    "SELECT EXISTS (
                        SELECT 1 FROM effect_claims
                        WHERE task_id = $1
                          AND status IN ('unknown', 'prepared', 'dispatching')
                    )",
                    &[&task.task_id.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?
                .get(0);

            let has_confirmed_effect_on_run: bool = if let Some(rid) = task.active_run_id {
                tx.query_one(
                    "SELECT EXISTS (
                        SELECT 1 FROM effect_claims
                        WHERE task_id = $1
                          AND run_id = $2
                          AND status = 'confirmed'
                    )",
                    &[&task.task_id.as_uuid(), &rid.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?
                .get(0)
            } else {
                false
            };

            let (next_status, audit_msg) = if has_unknown_effect {
                tx.execute(
                    "UPDATE tasks
                     SET status = 'unknown_external',
                         active_run_id = NULL,
                         wait_detail = 'Неизвестный исход внешнего действия после перезапуска dennett-head',
                         updated_at = now()
                     WHERE task_id = $1",
                    &[&task.task_id.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;
                (
                    TaskStatus::UnknownExternal,
                    "recovered interrupted task to unknown_external after Head restart",
                )
            } else if has_confirmed_effect_on_run {
                tx.execute(
                    "UPDATE tasks
                     SET status = 'failed',
                         active_run_id = NULL,
                         wait_detail = NULL,
                         error_code = 'HEAD_RESTARTED_DURING_RUN',
                         error_summary = 'Поручение прервано перезапуском dennett-head после внешнего действия; автоматический повтор отключён',
                         finished_at = now(),
                         updated_at = now()
                     WHERE task_id = $1",
                    &[&task.task_id.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;
                (
                    TaskStatus::Failed,
                    "marked interrupted task failed (HEAD_RESTARTED_DURING_RUN) after confirmed external effect",
                )
            } else {
                tx.execute(
                    "UPDATE tasks
                     SET status = 'queued',
                         active_run_id = NULL,
                         wait_detail = NULL,
                         error_code = NULL,
                         error_summary = NULL,
                         finished_at = NULL,
                         updated_at = now()
                     WHERE task_id = $1",
                    &[&task.task_id.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;
                (
                    TaskStatus::Queued,
                    "re-queued interrupted task cleanly after Head restart (zero external effects)",
                )
            };

            let _ = append_audit_event(
                &tx,
                self.pg_target(),
                correlation_id,
                Some(task.session_id),
                task.active_run_id,
                "head_recovery",
                "task_recovered_after_restart",
                &format!("task {} -> {next_status}: {audit_msg}", task.task_id),
            )
            .await?;
            pending_events.push(PendingControlEvent {
                entity_key: format!("task:{}", task.task_id),
                event_type: "task.recovered_after_restart",
                project_id: Some(task.project_id),
                session_id: Some(task.session_id),
                task_id: Some(task.task_id),
                payload: json!({
                    "task_id": task.task_id,
                    "previous_status": task.status,
                    "status": next_status,
                    "reason": "HEAD_RESTARTED_DURING_RUN",
                }),
            });
            recovered_tasks += 1;
        }

        // Strict V15 lock hierarchy: emit all journal events in one sorted batch right before COMMIT!
        let _ = append_control_events_batch_in_tx(
            &tx,
            self.pg_target(),
            correlation_id,
            &pending_events,
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        Ok(stale_effects
            + expired_perm_rows.len()
            + usize::try_from(stranded_runs).unwrap_or(0)
            + recovered_tasks)
    }

    /// Spawns the background PostgreSQL timer scheduler and startup crash-recovery pass,
    /// terminating cleanly when `shutdown_rx` receives `true`.
    #[must_use]
    pub fn spawn_background_scheduler(
        &self,
        mut shutdown_rx: watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()> {
        let svc = self.clone();
        tokio::spawn(async move {
            let boot_cid = CorrelationId::generate();
            let _ = svc.recover_after_startup_once(&boot_cid).await;
            let mut interval = time::interval(BACKGROUND_SCHEDULER_POLL_INTERVAL);
            interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
            let mut tick_count: u64 = 0;
            loop {
                tokio::select! {
                    biased;
                    changed = shutdown_rx.changed() => {
                        if changed.is_err() || *shutdown_rx.borrow() {
                            break;
                        }
                    }
                    _ = interval.tick() => {
                        tick_count = tick_count.wrapping_add(1);
                        let cid = CorrelationId::generate();
                        let _ = svc.tick_scheduler_at(TimestampMs::now(), &cid).await;
                        // D2-OVER-01: Automatic background drain of unified `ordered_outbox`
                        // (`destination = 'memoryd'`) every ~3s when pending items exist.
                        if tick_count.is_multiple_of(6) && svc.has_pending_memory_outbox().await {
                            let flush_cid = CorrelationId::generate();
                            let _ = svc.flush_memory_outbox(&flush_cid).await;
                        }
                    }
                }
            }
        })
    }

    #[expect(
        clippy::too_many_lines,
        reason = "resolves missed_schedule inbox card with optimistic concurrency, updates task/timer state, emits journal event, and optionally spawns execution"
    )]
    pub(crate) async fn resolve_missed_schedule_card(
        &self,
        card_id: InboxCardId,
        expected_revision: i64,
        task_id: TaskId,
        run_now: bool,
        correlation_id: &CorrelationId,
    ) -> Result<TaskRecord, DennettError> {
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let lock_sql =
            format!("SELECT {TASK_SELECT_COLS} FROM tasks WHERE task_id = $1 FOR UPDATE");
        let task_row = tx
            .query_one(&lock_sql, &[&task_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let task = decode_task_row(&task_row)?;
        let _ = ensure_project_active_in_tx(
            &tx,
            task.project_id,
            "resolve missed schedule card",
            self.pg_target(),
        )
        .await?;

        let summary = if run_now {
            "Владелец подтвердил однократный запуск пропущенного расписания"
        } else {
            "Владелец пропустил просроченное окно расписания"
        };
        let updated_cards = tx
            .execute(
                "UPDATE inbox_cards
                 SET status = 'resolved',
                     resolution_summary = $2,
                     revision = revision + 1,
                     resolved_at = now(),
                     updated_at = now()
                 WHERE card_id = $1 AND status = 'open' AND revision = $3",
                &[&card_id.as_uuid(), &summary, &expected_revision],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        if updated_cards == 0 {
            return Err(DennettError::Conflict {
                resource: String::from("inbox_card"),
                expected_revision: Some(expected_revision),
                actual_revision: None,
                reason: String::from("missed_schedule card was already resolved"),
            });
        }

        let now = TimestampMs::now();
        let next_timer_status = if run_now { "fired" } else { "skipped" };
        tx.execute(
            "UPDATE task_timers
             SET status = $2,
                 pause_reason = NULL,
                 fired_at = CASE WHEN $2 = 'fired' THEN now() ELSE fired_at END
             WHERE task_id = $1
               AND status = 'paused'
               AND (pause_reason = 'misfire_ask' OR pause_reason IS NULL)",
            &[&task_id.as_uuid(), &next_timer_status],
        )
        .await
        .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let mut has_next_timer = false;
        if let Some(ref sched) = task.schedule
            && !matches!(sched.kind, ScheduleKind::OnceAt { .. })
            && let Some(next_fire) =
                compute_next_fire_at_pg(&tx, sched, now, self.pg_target()).await?
        {
            let mut next_sched = sched.clone();
            next_sched.next_fire_at = Some(next_fire);
            let sched_val =
                serde_json::to_value(&next_sched).map_err(|err| DennettError::Internal {
                    detail: format!("failed serializing schedule: {err}"),
                })?;
            tx.execute(
                "UPDATE tasks SET schedule_json = $2 WHERE task_id = $1",
                &[&task_id.as_uuid(), &sched_val],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
            let next_key = format!("gen:{}:next:{}", task.generation, next_fire.as_millis());
            self.insert_task_timer_in_tx(
                &tx,
                task_id,
                task.generation,
                &next_key,
                &next_sched,
                next_fire,
            )
            .await?;
            has_next_timer = true;
        }

        let next_status = if run_now {
            TaskStatus::Queued
        } else if has_next_timer {
            TaskStatus::WaitingTimer
        } else {
            TaskStatus::Succeeded
        };
        let update_sql = format!(
            "UPDATE tasks
             SET status = $2,
                 wait_detail = NULL,
                 updated_at = now()
             WHERE task_id = $1
             RETURNING {TASK_SELECT_COLS}"
        );
        let updated_row = tx
            .query_one(&update_sql, &[&task_id.as_uuid(), &next_status.as_str()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let updated_task = decode_task_row(&updated_row)?;

        // D2-BUG-04: Emit ordered journal events for missed-schedule card resolution + task transition.
        let pending_events = vec![
            PendingControlEvent {
                entity_key: format!("inbox_card:{card_id}"),
                event_type: "inbox_card.resolved",
                project_id: Some(updated_task.project_id),
                session_id: Some(updated_task.session_id),
                task_id: Some(task_id),
                payload: json!({
                    "card_id": card_id,
                    "kind": "missed_schedule",
                    "run_now": run_now,
                    "resolution_summary": summary,
                }),
            },
            PendingControlEvent {
                entity_key: format!("task:{task_id}"),
                event_type: "task.missed_schedule_resolved",
                project_id: Some(updated_task.project_id),
                session_id: Some(updated_task.session_id),
                task_id: Some(task_id),
                payload: json!({
                    "task_id": task_id,
                    "card_id": card_id,
                    "run_now": run_now,
                    "status": updated_task.status,
                }),
            },
        ];
        let _ = append_control_events_batch_in_tx(
            &tx,
            self.pg_target(),
            correlation_id,
            &pending_events,
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        if run_now {
            self.spawn_task_execution(task_id, updated_task.generation, correlation_id.clone());
        }
        Ok(updated_task)
    }

    pub(crate) fn spawn_task_execution(
        &self,
        task_id: TaskId,
        expected_generation: i64,
        correlation_id: CorrelationId,
    ) {
        let svc = self.clone();
        tokio::spawn(async move {
            let _ = svc
                .execute_task_generation(task_id, expected_generation, correlation_id)
                .await;
        });
    }

    /// Executes one queued task turn for `(task_id, expected_generation)` with durable PG
    /// continuation/signal pickup (`D2-BUG-01`), external budget enforcement, unified turn core
    /// (`D2-OVER-02`), `human_wait_ms` exclusion (`D2-BUG-03`), `auth_profile_override` fail-closed
    /// separation, and strict `generation` fencing (`V07`).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError`] if control PostgreSQL fails.
    #[expect(
        clippy::too_many_lines,
        reason = "task execution orchestrates per-session serialization, pre-turn budget/auth guards, durable continuation/signal pickup, unified turn core, and generation-fenced completion"
    )]
    pub async fn execute_task_generation(
        &self,
        task_id: TaskId,
        expected_generation: i64,
        correlation_id: CorrelationId,
    ) -> Result<(), DennettError> {
        let _ = self.ensure_migrated().await?;

        // Look up execution session_id first (`target_session_id` if targeting an existing chat,
        // else the task's dedicated `session_id`) so we serialize turns cleanly without 409 races!
        let session_id = {
            let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
            let task = fetch_task(&conn, task_id, self.pg_target()).await?;
            task.target_session_id.unwrap_or(task.session_id)
        };
        let session_lock = self.session_exec_lock(session_id).await;
        let session_guard = session_lock.lock().await;

        let run_id = RunId::new();
        let (
            task,
            exec_session_id,
            session,
            agent,
            project,
            existing_binding,
            prior_history,
            policy_rules,
            resolved_runtime,
            user_msg_id,
            prompt_text,
        ) = {
            let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
            let tx = conn
                .transaction()
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;

            let lock_sql = format!(
                "SELECT {TASK_SELECT_COLS} FROM tasks WHERE task_id = $1 AND deleted_at IS NULL FOR UPDATE"
            );
            let Some(task_row) = tx
                .query_opt(&lock_sql, &[&task_id.as_uuid()])
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?
            else {
                tx.commit()
                    .await
                    .map_err(|err| map_pg_err(self.pg_target(), &err))?;
                drop(session_guard);
                self.prune_session_lock(session_id).await;
                return Ok(());
            };
            let task = decode_task_row(&task_row)?;
            let exec_session_id = task.target_session_id.unwrap_or(task.session_id);

            // Only start if still queued for THIS exact generation and not cancelled/unknown!
            if task.generation != expected_generation || task.status != TaskStatus::Queued {
                tx.commit()
                    .await
                    .map_err(|err| map_pg_err(self.pg_target(), &err))?;
                drop(session_guard);
                self.prune_session_lock(session_id).await;
                return Ok(());
            }

            let project = fetch_project(&tx, task.project_id, self.pg_target()).await?;
            if matches!(
                project.lifecycle_state,
                ProjectLifecycleState::Archived | ProjectLifecycleState::DeletedRecord
            ) {
                tx.commit()
                    .await
                    .map_err(|err| map_pg_err(self.pg_target(), &err))?;
                drop(session_guard);
                self.prune_session_lock(session_id).await;
                return Ok(());
            }

            // 1. Pre-turn external budget check strictly outside the model (Contract G).
            if let Some(violation) = task.budget.check_before_next_turn(&task.usage) {
                let mut pending_events = Vec::with_capacity(1);
                self.record_budget_exceeded_in_tx(&tx, &task, &violation, &mut pending_events)
                    .await?;
                let _ = append_control_events_batch_in_tx(
                    &tx,
                    self.pg_target(),
                    &correlation_id,
                    &pending_events,
                )
                .await?;
                tx.commit()
                    .await
                    .map_err(|err| map_pg_err(self.pg_target(), &err))?;
                drop(session_guard);
                self.prune_session_lock(session_id).await;
                return Ok(());
            }

            let session = fetch_session(&tx, exec_session_id, self.pg_target()).await?;
            let agent = fetch_agent(&tx, task.agent_id, self.pg_target()).await?;

            // 2. Runtime & auth profile separation check (A09/A10/M: fail closed without silent fallback!).
            let resolved_runtime = match self
                .resolve_runtime_for_turn(
                    task.connection_override_id
                        .or(agent.current_config.preferred_connection_id),
                    task.auth_profile_override,
                )
                .await
            {
                Ok(rt) => rt,
                Err(auth_err) => {
                    let err_summary =
                        truncate_utf8(&self.redact(&auth_err.to_string()), 2000).to_owned();
                    tx.execute(
                        "UPDATE tasks
                         SET status = 'failed',
                             error_code = $2,
                             error_summary = $3,
                             finished_at = now(),
                             updated_at = now()
                         WHERE task_id = $1",
                        &[&task_id.as_uuid(), &auth_err.code(), &err_summary],
                    )
                    .await
                    .map_err(|err| map_pg_err(self.pg_target(), &err))?;
                    let _ = append_control_event_in_tx(
                        &tx,
                        self.pg_target(),
                        ControlEventAppend {
                            entity_key: &format!("task:{task_id}"),
                            event_type: "task.failed",
                            project_id: Some(task.project_id),
                            session_id: Some(task.session_id),
                            task_id: Some(task_id),
                            correlation_id: &correlation_id,
                            payload: json!({
                                "task_id": task_id,
                                "error_code": auth_err.code(),
                                "error_summary": err_summary,
                            }),
                        },
                    )
                    .await?;
                    tx.commit()
                        .await
                        .map_err(|err| map_pg_err(self.pg_target(), &err))?;
                    drop(session_guard);
                    self.prune_session_lock(session_id).await;
                    return Ok(());
                }
            };

            let binding = fetch_runtime_binding(&tx, exec_session_id, self.pg_target()).await?;
            let history = fetch_session_messages(&tx, exec_session_id, self.pg_target()).await?;
            let rules = fetch_policy_rules(&tx, self.pg_target()).await?;

            // D2-BUG-01: Atomically fetch & clear durable `pending_continuation_input` plus any
            // unconsumed `task_signals` rows under the task's `FOR UPDATE` lock!
            let pending_continuation: Option<String> = tx
                .query_one(
                    "SELECT pending_continuation_input FROM tasks WHERE task_id = $1",
                    &[&task_id.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?
                .get(0);

            let signal_rows = tx
                .query(
                    "UPDATE task_signals
                     SET consumed_in_generation = $2
                     WHERE signal_id IN (
                         SELECT signal_id
                         FROM task_signals
                         WHERE task_id = $1
                           AND outcome_status IN ('woke_task', 'queued_on_task')
                           AND consumed_in_generation IS NULL
                         ORDER BY created_at ASC
                         FOR UPDATE
                     )
                     RETURNING signal_name, payload_text",
                    &[&task_id.as_uuid(), &expected_generation],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;

            // Collect undelivered plain-text inter-agent messages scoped to general OR this task (`D2-BUG-04`).
            let undelivered_sql = format!(
                "SELECT {AGENT_MESSAGE_SELECT_COLS}
                 FROM agent_messages
                 WHERE recipient_agent_id = $1
                   AND project_id = $2
                   AND (related_task_id IS NULL OR related_task_id = $3)
                   AND delivered_in_run_id IS NULL
                 ORDER BY created_at ASC
                 FOR UPDATE"
            );
            let msg_rows = tx
                .query(
                    &undelivered_sql,
                    &[
                        &agent.agent_id.as_uuid(),
                        &project.project_id.as_uuid(),
                        &task_id.as_uuid(),
                    ],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;
            let incoming_agent_msgs: Vec<AgentMessageRecord> = msg_rows
                .iter()
                .map(decode_agent_message_row)
                .collect::<Result<Vec<_>, _>>()?;

            let mut prompt_sections = Vec::new();
            if !incoming_agent_msgs.is_empty() {
                prompt_sections.push(self.format_inter_agent_messages_block(&incoming_agent_msgs));
            }
            for srow in &signal_rows {
                let sig_name: String = srow.get(0);
                let sig_payload: String = srow.get(1);
                prompt_sections.push(format!("[Signal `{sig_name}` received]\n{sig_payload}"));
            }
            let is_clarification_reply = pending_continuation
                .as_deref()
                .is_some_and(|extra| extra.starts_with("[User Clarification Reply]"));
            if let Some(ref extra) = pending_continuation {
                prompt_sections.push(extra.clone());
            }
            // Clarification replies continue an existing session where `task.goal` is already
            // persisted in turn 1's conversation history; re-sending the raw goal would re-trigger
            // the original ambiguity instead of advancing with the owner's answer.
            if !is_clarification_reply {
                prompt_sections.push(task.goal.clone());
            }
            let full_prompt = prompt_sections.join("\n");

            let user_msg_id = MessageId::new();
            let stream_id = format!("session:{exec_session_id}:messages");
            let msg_seq = next_stream_seq(&tx, &stream_id, self.pg_target()).await?;

            tx.execute(
                "INSERT INTO runs (
                    run_id, session_id, agent_id, agent_config_version_id,
                    user_message_id, status, task_id, task_generation, correlation_id
                 ) VALUES ($1, $2, $3, $4, $5, 'running', $6, $7, $8)",
                &[
                    &run_id.as_uuid(),
                    &exec_session_id.as_uuid(),
                    &agent.agent_id.as_uuid(),
                    &agent.current_config_version_id.as_uuid(),
                    &user_msg_id.as_uuid(),
                    &task_id.as_uuid(),
                    &expected_generation,
                    &correlation_id.as_str(),
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

            tx.execute(
                "INSERT INTO messages (
                    message_id, session_id, stream_seq, role, content, run_id, correlation_id
                 ) VALUES ($1, $2, $3, 'user', $4, $5, $6)",
                &[
                    &user_msg_id.as_uuid(),
                    &exec_session_id.as_uuid(),
                    &msg_seq,
                    &full_prompt,
                    &run_id.as_uuid(),
                    &correlation_id.as_str(),
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

            for m in &incoming_agent_msgs {
                tx.execute(
                    "UPDATE agent_messages
                     SET delivered_in_run_id = $2,
                         delivered_at = now()
                     WHERE agent_message_id = $1",
                    &[&m.agent_message_id.as_uuid(), &run_id.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;
            }

            tx.execute(
                "UPDATE tasks
                 SET status = 'running',
                     active_run_id = $2,
                     wait_detail = NULL,
                     pending_continuation_input = NULL,
                     updated_at = now()
                 WHERE task_id = $1",
                &[&task_id.as_uuid(), &run_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

            let _ = append_control_event_in_tx(
                &tx,
                self.pg_target(),
                ControlEventAppend {
                    entity_key: &format!("task:{task_id}"),
                    event_type: "task.run_started",
                    project_id: Some(task.project_id),
                    session_id: Some(task.session_id),
                    task_id: Some(task_id),
                    correlation_id: &correlation_id,
                    payload: json!({
                        "task_id": task_id,
                        "run_id": run_id,
                        "generation": expected_generation,
                    }),
                },
            )
            .await?;

            tx.commit()
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;

            (
                task,
                exec_session_id,
                session,
                agent,
                project,
                binding,
                history,
                rules,
                resolved_runtime,
                user_msg_id,
                full_prompt,
            )
        };

        let (cancel_tx, cancel_rx) = watch::channel(false);
        let (slot_released_tx, slot_released_rx) = watch::channel(false);
        let _slot_released = crate::service::SlotReleasedNotifier::new(slot_released_tx);
        self.try_register_active_session_run(
            exec_session_id,
            ActiveRunHandle {
                run_id,
                client_message_id: None,
                cancel_tx: cancel_tx.clone(),
                cancel_reason: std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0)),
                completion_rx: None,
                slot_released_rx,
            },
        )
        .await?;
        let mut run_guard = ActiveRunGuard::new(
            self.clone(),
            exec_session_id,
            run_id,
            cancel_tx.clone(),
            correlation_id.clone(),
        );

        self.register_active_task(
            task_id,
            ActiveTaskHandle {
                generation: expected_generation,
                session_id: exec_session_id,
                run_id,
                cancel_tx,
            },
        )
        .await;
        let _task_guard = ActiveTaskGuard::new(self.active_tasks_map(), task_id, run_id);

        let turn_timeout = task
            .budget
            .max_wall_secs
            .map_or(DEFAULT_TURN_TIMEOUT, |limit_secs| {
                let limit_ms = limit_secs.saturating_mul(1_000);
                let rem_ms = limit_ms.saturating_sub(task.usage.wall_ms_used).max(500);
                Duration::from_millis(rem_ms).min(DEFAULT_TURN_TIMEOUT)
            });

        let core_project = if project.project_id == crate::policy::standalone_system_project_id() {
            None
        } else {
            Some(project)
        };

        // D2-BUG-03: Give interactive human approval the full `DEFAULT_APPROVAL_WAIT_TIMEOUT`,
        // independent of `max_wall_secs` — RPC deadline & wall_ms budget pause while human decides!
        let core_res = self
            .execute_session_turn_core(CoreTurnInput {
                trigger: TurnTrigger::TaskGeneration { task_id },
                session_id: exec_session_id,
                session_kind: session.session_kind,
                run_id,
                agent,
                project: core_project,
                existing_binding,
                prior_history,
                policy_rules,
                prompt_text,
                runtime: resolved_runtime.runtime,
                connection_id: resolved_runtime.connection_id,
                model_override: task.model_override.clone(),
                thinking_override: task.thinking_override,
                skill_slugs_override: task.skill_slugs_override.clone(),
                turn_timeout,
                approval_timeout: DEFAULT_APPROVAL_WAIT_TIMEOUT,
                cancel_rx,
                correlation_id: correlation_id.clone(),
            })
            .await;

        let core = match core_res {
            Ok(out) => out,
            Err(failure) => {
                self.complete_task_run_error(
                    task_id,
                    run_id,
                    expected_generation,
                    &failure.error,
                    failure.effective_elapsed_ms,
                    &failure.captured_decisions,
                    &correlation_id,
                )
                .await;
                run_guard.mark_completed();
                self.unregister_active_session_run(exec_session_id, run_id)
                    .await;
                drop(session_guard);
                self.prune_session_lock(session_id).await;
                return Ok(());
            }
        };

        if let Err(err) = self
            .complete_task_run(
                task_id,
                run_id,
                expected_generation,
                user_msg_id,
                core.start_outcome.rehydrated,
                &core.turn_outcome,
                core.effective_elapsed_ms,
                core.context_used_tokens,
                core.context_window_tokens,
                &core.captured_decisions,
                core.hit_unknown_external,
                &correlation_id,
            )
            .await
        {
            // D2-BUG-04: Fallback so a PG commit failure never strands the task in `running`!
            self.complete_task_run_error(
                task_id,
                run_id,
                expected_generation,
                &err,
                core.effective_elapsed_ms,
                &core.captured_decisions,
                &correlation_id,
            )
            .await;
        }
        run_guard.mark_completed();
        self.unregister_active_session_run(exec_session_id, run_id)
            .await;
        drop(session_guard);
        self.prune_session_lock(session_id).await;

        Ok(())
    }

    /// Generation-fenced atomic completion of one task run (V07).
    ///
    /// CRITICAL INVARIANT: locks `tasks WHERE task_id = $1 FOR UPDATE`. If `task.generation != run_generation`
    /// or `task.status == 'cancelled'`, marks the run `RunStatus::Superseded` (`superseded_by_generation`)
    /// or `Cancelled` WITHOUT overwriting the current task's `goal`, `status`, or `partial_result_summary`!
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::DatabaseUnavailable`] on PostgreSQL error.
    #[expect(
        clippy::too_many_arguments,
        clippy::too_many_lines,
        reason = "generation-fenced task run completion atomically persists decisions, assistant reply, run status, budget accounting, clarification cards, and control journal events"
    )]
    pub async fn complete_task_run(
        &self,
        task_id: TaskId,
        run_id: RunId,
        run_generation: i64,
        _user_message_id: MessageId,
        rehydrated: bool,
        outcome: &TurnOutcome,
        elapsed_ms: u64,
        context_used_tokens: Option<u64>,
        context_window_tokens: Option<u64>,
        captured: &[crate::approvals::CapturedPermissionDecision],
        hit_unknown_external: bool,
        correlation_id: &CorrelationId,
    ) -> Result<RunStatus, DennettError> {
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        for cap in captured {
            if !cap.already_persisted {
                insert_permission_decision(&tx, self.pg_target(), &cap.record).await?;
            }
        }

        let lock_sql =
            format!("SELECT {TASK_SELECT_COLS} FROM tasks WHERE task_id = $1 FOR UPDATE");
        let task_row = tx
            .query_one(&lock_sql, &[&task_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let current_task = decode_task_row(&task_row)?;

        let turn_tokens = outcome.total_tokens();
        let tokens_i64 = turn_tokens.and_then(|v| i64::try_from(v).ok());
        let wall_i64 = i64::try_from(elapsed_ms).ok();
        let ctx_used_i64 = context_used_tokens.and_then(|v| i64::try_from(v).ok());
        let ctx_window_i64 = context_window_tokens.and_then(|v| i64::try_from(v).ok());

        // V07 GENERATION FENCE: late worker from an older generation MUST NOT overwrite current goal/state!
        if current_task.generation != run_generation {
            let assistant_msg_id = if outcome.assistant_text.trim().is_empty() {
                None
            } else {
                let mid = MessageId::new();
                let stream_id = format!("session:{}:messages", current_task.session_id);
                let seq = next_stream_seq(&tx, &stream_id, self.pg_target()).await?;
                let tagged = self.redact(&format!(
                    "[superseded gen={run_generation} run={run_id}] {}",
                    outcome.assistant_text
                ));
                tx.execute(
                    "INSERT INTO messages (
                        message_id, session_id, stream_seq, role, content, run_id, correlation_id
                     ) VALUES ($1, $2, $3, 'assistant', $4, $5, $6)",
                    &[
                        &mid.as_uuid(),
                        &current_task.session_id.as_uuid(),
                        &seq,
                        &tagged,
                        &run_id.as_uuid(),
                        &correlation_id.as_str(),
                    ],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;
                Some(mid)
            };

            tx.execute(
                "UPDATE runs
                 SET assistant_message_id = $2,
                     status = 'superseded',
                     stop_reason = 'superseded_by_generation',
                     rehydrated = $3,
                     tokens_used = $4,
                     wall_ms = $5,
                     context_used_tokens = $6,
                     context_window_tokens = $7,
                     finished_at = now()
                 WHERE run_id = $1",
                &[
                    &run_id.as_uuid(),
                    &assistant_msg_id.map(MessageId::as_uuid),
                    &rehydrated,
                    &tokens_i64,
                    &wall_i64,
                    &ctx_used_i64,
                    &ctx_window_i64,
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

            // Clear active_run_id ONLY if it still points to this superseded run; leave goal/status UNTOUCHED!
            if current_task.active_run_id == Some(run_id) {
                tx.execute(
                    "UPDATE tasks SET active_run_id = NULL WHERE task_id = $1 AND active_run_id = $2",
                    &[&task_id.as_uuid(), &run_id.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;
            }

            let _ = append_control_event_in_tx(
                &tx,
                self.pg_target(),
                ControlEventAppend {
                    entity_key: &format!("task:{task_id}"),
                    event_type: "task.run_superseded",
                    project_id: Some(current_task.project_id),
                    session_id: Some(current_task.session_id),
                    task_id: Some(task_id),
                    correlation_id,
                    payload: json!({
                        "task_id": task_id,
                        "run_id": run_id,
                        "run_generation": run_generation,
                        "current_generation": current_task.generation,
                    }),
                },
            )
            .await?;

            tx.commit()
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;
            return Ok(RunStatus::Superseded);
        }

        // Persist assistant message into the execution session timeline when non-empty.
        let exec_session_id = current_task
            .target_session_id
            .unwrap_or(current_task.session_id);
        let safe_text = self.redact(&outcome.assistant_text);
        let assistant_msg_id = if safe_text.trim().is_empty() {
            None
        } else {
            let mid = MessageId::new();
            let stream_id = format!("session:{exec_session_id}:messages");
            let seq = next_stream_seq(&tx, &stream_id, self.pg_target()).await?;
            tx.execute(
                "INSERT INTO messages (
                    message_id, session_id, stream_seq, role, content, run_id, correlation_id
                 ) VALUES ($1, $2, $3, 'assistant', $4, $5, $6)",
                &[
                    &mid.as_uuid(),
                    &exec_session_id.as_uuid(),
                    &seq,
                    &safe_text,
                    &run_id.as_uuid(),
                    &correlation_id.as_str(),
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
            Some(mid)
        };

        // D2-OVER-02: Persist `tool_observed` audit trace for task runs identically to user chats.
        for ev in &outcome.events {
            if let RuntimeEvent::ToolObserved {
                tool_name,
                title,
                status,
                ..
            } = ev
            {
                let _ = append_audit_event(
                    &tx,
                    self.pg_target(),
                    correlation_id,
                    Some(current_task.session_id),
                    Some(run_id),
                    self.runtime().adapter_id(),
                    "tool_observed",
                    &self.redact(&format!("tool `{tool_name}` ({title}) status={status}")),
                )
                .await?;
            }
        }

        let partial_summary = if safe_text.trim().is_empty() {
            current_task.partial_result_summary.clone()
        } else {
            Some(truncate_utf8(&safe_text, 16_000).to_owned())
        };

        // Check if task was already cancelled or hit unknown_external.
        if current_task.status == TaskStatus::Cancelled || outcome.status == RunStatus::Cancelled {
            if assistant_msg_id.is_none() {
                tx.execute(
                    "UPDATE agent_messages
                     SET delivered_in_run_id = NULL, delivered_at = NULL
                     WHERE delivered_in_run_id = $1",
                    &[&run_id.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;
            }
            tx.execute(
                "UPDATE runs
                 SET assistant_message_id = $2,
                     status = CASE WHEN status = 'unknown_external' THEN 'unknown_external' ELSE 'cancelled' END,
                     stop_reason = 'cancelled',
                     rehydrated = $3,
                     tokens_used = $4,
                     wall_ms = $5,
                     context_used_tokens = $6,
                     context_window_tokens = $7,
                     finished_at = now()
                 WHERE run_id = $1",
                &[
                    &run_id.as_uuid(),
                    &assistant_msg_id.map(MessageId::as_uuid),
                    &rehydrated,
                    &tokens_i64,
                    &wall_i64,
                    &ctx_used_i64,
                    &ctx_window_i64,
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

            let has_unknown_effect_on_cancel: bool = hit_unknown_external
                || current_task.status == TaskStatus::UnknownExternal
                || tx
                    .query_one(
                        "SELECT EXISTS(
                            SELECT 1 FROM effect_claims
                            WHERE task_id = $1 AND status IN ('unknown', 'dispatching')
                        )",
                        &[&task_id.as_uuid()],
                    )
                    .await
                    .map_err(|err| map_pg_err(self.pg_target(), &err))?
                    .get(0);
            let final_task_status = if has_unknown_effect_on_cancel {
                TaskStatus::UnknownExternal
            } else {
                TaskStatus::Cancelled
            };

            tx.execute(
                "UPDATE tasks
                 SET status = CASE WHEN status = 'unknown_external' THEN 'unknown_external' ELSE $4 END,
                     active_run_id = CASE WHEN active_run_id = $2 THEN NULL ELSE active_run_id END,
                     partial_result_summary = COALESCE($3, partial_result_summary),
                     finished_at = CASE WHEN $4 = 'cancelled' THEN COALESCE(finished_at, now()) ELSE finished_at END,
                     updated_at = now()
                 WHERE task_id = $1",
                &[
                    &task_id.as_uuid(),
                    &run_id.as_uuid(),
                    &partial_summary.as_deref(),
                    &final_task_status.as_str(),
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

            // D2-BUG-04: Emit ordered journal event on task run cancellation.
            let mut cancel_events = vec![PendingControlEvent {
                entity_key: format!("task:{task_id}"),
                event_type: if final_task_status == TaskStatus::UnknownExternal {
                    "task.stopped_with_unknown_external"
                } else {
                    "task.cancelled"
                },
                project_id: Some(current_task.project_id),
                session_id: Some(current_task.session_id),
                task_id: Some(task_id),
                payload: json!({
                    "task_id": task_id,
                    "run_id": run_id,
                    "generation": run_generation,
                    "status": final_task_status,
                }),
            }];
            let expired_perms = self
                .expire_pending_approvals_for_run_in_tx(
                    &tx,
                    run_id,
                    "withdrawn",
                    "Ход поручения отменён",
                    &mut cancel_events,
                )
                .await?;
            let _ = append_control_events_batch_in_tx(
                &tx,
                self.pg_target(),
                correlation_id,
                &cancel_events,
            )
            .await?;

            tx.commit()
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;

            if !expired_perms.is_empty() {
                self.wake_expired_approval_waiters(
                    &expired_perms,
                    current_task.session_id,
                    "task run cancelled",
                )
                .await;
            }
            self.clear_stopped_task(task_id).await;
            return Ok(if has_unknown_effect_on_cancel {
                RunStatus::UnknownExternal
            } else {
                RunStatus::Cancelled
            });
        }

        let has_unknown_effect: bool = hit_unknown_external
            || current_task.status == TaskStatus::UnknownExternal
            || tx
                .query_one(
                    "SELECT EXISTS(
                        SELECT 1 FROM effect_claims
                        WHERE task_id = $1 AND status IN ('unknown', 'dispatching')
                    )",
                    &[&task_id.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?
                .get(0);

        let next_usage = TaskUsage {
            turns_used: current_task.usage.turns_used.saturating_add(1),
            tokens_used: current_task
                .usage
                .tokens_used
                .saturating_add(turn_tokens.unwrap_or(0)),
            wall_ms_used: current_task.usage.wall_ms_used.saturating_add(elapsed_ms),
            confidence: if turn_tokens.is_some() {
                UsageConfidence::ProviderReported
            } else {
                current_task.usage.confidence
            },
        };

        let final_run_status = if has_unknown_effect {
            RunStatus::UnknownExternal
        } else {
            outcome.status
        };
        let stop_reason = truncate_utf8(&outcome.stop_reason, 128).to_owned();

        tx.execute(
            "UPDATE runs
             SET assistant_message_id = $2,
                 status = $3,
                 stop_reason = $4,
                 rehydrated = $5,
                 tokens_used = $6,
                 wall_ms = $7,
                 context_used_tokens = $8,
                 context_window_tokens = $9,
                 finished_at = now()
             WHERE run_id = $1",
            &[
                &run_id.as_uuid(),
                &assistant_msg_id.map(MessageId::as_uuid),
                &final_run_status.as_str(),
                &Some(stop_reason.as_str()),
                &rehydrated,
                &tokens_i64,
                &wall_i64,
                &ctx_used_i64,
                &ctx_window_i64,
            ],
        )
        .await
        .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        // Update cumulative task usage counters first.
        tx.execute(
            "UPDATE tasks
             SET turns_used = $2,
                 tokens_used = $3,
                 wall_ms_used = $4,
                 usage_confidence = $5,
                 active_run_id = CASE WHEN active_run_id = $6 THEN NULL ELSE active_run_id END,
                 partial_result_summary = COALESCE($7, partial_result_summary),
                 updated_at = now()
             WHERE task_id = $1",
            &[
                &task_id.as_uuid(),
                &i32::try_from(next_usage.turns_used).unwrap_or(i32::MAX),
                &i64::try_from(next_usage.tokens_used).unwrap_or(i64::MAX),
                &i64::try_from(next_usage.wall_ms_used).unwrap_or(i64::MAX),
                &next_usage.confidence.as_str(),
                &run_id.as_uuid(),
                &partial_summary.as_deref(),
            ],
        )
        .await
        .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let mut updated_task_view = current_task.clone();
        updated_task_view.usage = next_usage.clone();
        let mut pending_events: Vec<PendingControlEvent> = Vec::new();

        if has_unknown_effect {
            tx.execute(
                "UPDATE tasks SET status = 'unknown_external', updated_at = now() WHERE task_id = $1",
                &[&task_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        } else if let Some(violation) = current_task.budget.check_after_turn(&next_usage) {
            self.record_budget_exceeded_in_tx(
                &tx,
                &updated_task_view,
                &violation,
                &mut pending_events,
            )
            .await?;
        } else if let Some(question) = outcome.clarification_question() {
            let safe_q = truncate_utf8(&self.redact(question), 2000).to_owned();
            tx.execute(
                "UPDATE tasks
                 SET status = 'waiting_clarification',
                     wait_detail = $2,
                     updated_at = now()
                 WHERE task_id = $1",
                &[&task_id.as_uuid(), &safe_q],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

            let card_id = InboxCardId::new();
            let subject = truncate_utf8(
                &format!("Уточнение по поручению: {}", current_task.title),
                500,
            )
            .to_owned();
            tx.execute(
                "INSERT INTO inbox_cards (
                    card_id, project_id, session_id, task_id, run_id, agent_id,
                    kind, status, urgency, subject, detail
                 ) VALUES ($1, $2, $3, $4, $5, $6, 'clarification', 'open', 'normal', $7, $8)",
                &[
                    &card_id.as_uuid(),
                    &current_task.project_id.as_uuid(),
                    &current_task.session_id.as_uuid(),
                    &task_id.as_uuid(),
                    &run_id.as_uuid(),
                    &current_task.agent_id.as_uuid(),
                    &subject,
                    &safe_q,
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

            pending_events.push(PendingControlEvent {
                entity_key: format!("task:{task_id}"),
                event_type: "task.clarification_requested",
                project_id: Some(current_task.project_id),
                session_id: Some(current_task.session_id),
                task_id: Some(task_id),
                payload: json!({
                    "task_id": task_id,
                    "card_id": card_id,
                    "question": safe_q,
                }),
            });
        } else {
            // D2-ARCH-02: Recognize BOTH `pending` AND `paused` recurring timers so a periodic
            // task whose project was archived mid-run transitions to `waiting_timer` instead of
            // being erroneously closed as `succeeded`!
            let timer_state_row = tx
                .query_opt(
                    "SELECT status, pause_reason
                     FROM task_timers
                     WHERE task_id = $1
                       AND task_generation = $2
                       AND status IN ('pending', 'paused')
                     ORDER BY CASE WHEN status = 'pending' THEN 0 ELSE 1 END, fire_at ASC
                     LIMIT 1",
                    &[&task_id.as_uuid(), &run_generation],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;

            let (next_status, next_wait_detail, event_type) = match timer_state_row {
                Some(row) => {
                    let t_status: String = row.get(0);
                    let pause_reason = row
                        .get::<_, Option<String>>(1)
                        .and_then(|s| TimerPauseReason::from_str(&s).ok());
                    let detail = if t_status == "paused" {
                        match pause_reason {
                            Some(TimerPauseReason::ArchivedProject) => {
                                Some("Проект в архиве — расписание приостановлено")
                            }
                            Some(TimerPauseReason::MisfireAsk) => {
                                Some("Пропущено окно расписания — ожидает решения")
                            }
                            None | Some(_) => Some("Расписание приостановлено"),
                        }
                    } else {
                        Some("Ожидает следующего запуска по расписанию")
                    };
                    (TaskStatus::WaitingTimer, detail, "task.turn_completed")
                }
                None => (TaskStatus::Succeeded, None, "task.succeeded"),
            };
            tx.execute(
                "UPDATE tasks
                 SET status = $2,
                     wait_detail = $3,
                     finished_at = CASE WHEN $2 = 'succeeded' THEN now() ELSE finished_at END,
                     updated_at = now()
                 WHERE task_id = $1",
                &[&task_id.as_uuid(), &next_status.as_str(), &next_wait_detail],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

            pending_events.push(PendingControlEvent {
                entity_key: format!("task:{task_id}"),
                event_type,
                project_id: Some(current_task.project_id),
                session_id: Some(current_task.session_id),
                task_id: Some(task_id),
                payload: json!({
                    "task_id": task_id,
                    "run_id": run_id,
                    "generation": run_generation,
                    "status": next_status,
                    "turns_used": next_usage.turns_used,
                    "tokens_used": next_usage.tokens_used,
                }),
            });

            // Deliver concise system completion summary into `notify_session_id` when running
            // isolated from the caller session (e.g., delegated subagent or isolated reminder).
            if let Some(notify_sid) = current_task.notify_session_id
                && notify_sid != exec_session_id
            {
                let summary_snippet = if safe_text.trim().is_empty() {
                    String::from("(без текстового ответа)")
                } else {
                    truncate_utf8(safe_text.trim(), 1500).to_owned()
                };
                let notice = format!(
                    "[Поручение завершено: {} (task_id={task_id}, status={next_status})]\n{summary_snippet}",
                    current_task.title
                );
                let _ = self
                    .deliver_task_completion_notice_in_tx(
                        &tx,
                        notify_sid,
                        &current_task,
                        &notice,
                        correlation_id,
                        &mut pending_events,
                    )
                    .await;
            }
        }

        let expired_perms = self
            .expire_pending_approvals_for_run_in_tx(
                &tx,
                run_id,
                if final_run_status == RunStatus::Cancelled {
                    "withdrawn"
                } else {
                    "expired"
                },
                "Ход поручения завершён",
                &mut pending_events,
            )
            .await?;

        let _ = append_control_events_batch_in_tx(
            &tx,
            self.pg_target(),
            correlation_id,
            &pending_events,
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        if !expired_perms.is_empty() {
            self.wake_expired_approval_waiters(
                &expired_perms,
                exec_session_id,
                "task run completed",
            )
            .await;
        }
        self.clear_stopped_task(task_id).await;
        self.wake_tasks_waiting_for_run(run_id, correlation_id)
            .await;
        Ok(final_run_status)
    }

    #[expect(
        clippy::too_many_arguments,
        clippy::too_many_lines,
        reason = "records failed task run safely under FOR UPDATE generation fencing, releasing agent messages and expiring pending approvals"
    )]
    async fn complete_task_run_error(
        &self,
        task_id: TaskId,
        run_id: RunId,
        run_generation: i64,
        err: &DennettError,
        elapsed_ms: u64,
        captured: &[crate::approvals::CapturedPermissionDecision],
        correlation_id: &CorrelationId,
    ) {
        let Ok(mut conn) = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await else {
            return;
        };
        let Ok(tx) = conn.transaction().await else {
            return;
        };
        for cap in captured {
            if !cap.already_persisted {
                let _ = insert_permission_decision(&tx, self.pg_target(), &cap.record).await;
            }
        }
        // D2-BUG-04: Return undelivered inter-agent messages stamped by this failed run!
        let _ = tx
            .execute(
                "UPDATE agent_messages
                 SET delivered_in_run_id = NULL, delivered_at = NULL
                 WHERE delivered_in_run_id = $1",
                &[&run_id.as_uuid()],
            )
            .await;

        let safe_err = truncate_utf8(&self.redact(&err.to_string()), 2000).to_owned();
        let wall_i64 = i64::try_from(elapsed_ms).ok();
        let mut pending_events: Vec<PendingControlEvent> = Vec::new();
        // D2-BUG-04: Lock task row `FOR UPDATE` and fence strictly on `generation = $5`!
        let lock_sql =
            format!("SELECT {TASK_SELECT_COLS} FROM tasks WHERE task_id = $1 FOR UPDATE");
        let task_res = match tx.query_opt(&lock_sql, &[&task_id.as_uuid()]).await {
            Ok(Some(row)) => decode_task_row(&row).ok(),
            _ => None,
        };
        if let Some(task) = task_res {
            if task.generation == run_generation {
                let _ = tx
                    .execute(
                        "UPDATE runs
                         SET status = CASE WHEN status = 'unknown_external' THEN 'unknown_external' ELSE 'failed' END,
                             error_code = $2,
                             error_summary = $3,
                             wall_ms = $4,
                             finished_at = now()
                         WHERE run_id = $1",
                        &[&run_id.as_uuid(), &err.code(), &safe_err, &wall_i64],
                    )
                    .await;
                if !matches!(
                    task.status,
                    TaskStatus::Cancelled | TaskStatus::Succeeded | TaskStatus::UnknownExternal
                ) {
                    let _ = tx
                        .execute(
                            "UPDATE tasks
                             SET status = 'failed',
                                 error_code = $2,
                                 error_summary = $3,
                                 active_run_id = CASE WHEN active_run_id = $4 THEN NULL ELSE active_run_id END,
                                 finished_at = now(),
                                 updated_at = now()
                             WHERE task_id = $1 AND generation = $5",
                            &[
                                &task_id.as_uuid(),
                                &err.code(),
                                &safe_err,
                                &run_id.as_uuid(),
                                &run_generation,
                            ],
                        )
                        .await;
                    pending_events.push(PendingControlEvent {
                        entity_key: format!("task:{task_id}"),
                        event_type: "task.failed",
                        project_id: Some(task.project_id),
                        session_id: Some(task.session_id),
                        task_id: Some(task_id),
                        payload: json!({
                            "task_id": task_id,
                            "run_id": run_id,
                            "error_code": err.code(),
                            "error_summary": safe_err,
                        }),
                    });
                    let exec_sid = task.target_session_id.unwrap_or(task.session_id);
                    if let Some(notify_sid) = task.notify_session_id
                        && notify_sid != exec_sid
                    {
                        let notice = format!(
                            "[Поручение завершилось ошибкой: {} (task_id={task_id}, code={})]\n{safe_err}",
                            task.title,
                            err.code()
                        );
                        let _ = self
                            .deliver_task_completion_notice_in_tx(
                                &tx,
                                notify_sid,
                                &task,
                                &notice,
                                correlation_id,
                                &mut pending_events,
                            )
                            .await;
                    }
                }
            } else {
                let _ = tx
                    .execute(
                        "UPDATE runs
                         SET status = 'superseded',
                             stop_reason = 'superseded_by_generation',
                             error_code = $2,
                             error_summary = $3,
                             wall_ms = $4,
                             finished_at = now()
                         WHERE run_id = $1",
                        &[&run_id.as_uuid(), &err.code(), &safe_err, &wall_i64],
                    )
                    .await;
            }
            let expired_perms = self
                .expire_pending_approvals_for_run_in_tx(
                    &tx,
                    run_id,
                    "expired",
                    &format!(
                        "Ход поручения завершился ошибкой ({}): {safe_err}",
                        err.code()
                    ),
                    &mut pending_events,
                )
                .await
                .unwrap_or_default();
            let _ = append_control_events_batch_in_tx(
                &tx,
                self.pg_target(),
                correlation_id,
                &pending_events,
            )
            .await;
            let _ = tx.commit().await;
            if !expired_perms.is_empty() {
                self.wake_expired_approval_waiters(&expired_perms, task.session_id, &safe_err)
                    .await;
            }
            self.clear_stopped_task(task_id).await;
            self.wake_tasks_waiting_for_run(run_id, correlation_id)
                .await;
            return;
        }
        let _ = self
            .expire_pending_approvals_for_run_in_tx(
                &tx,
                run_id,
                "expired",
                &format!(
                    "Ход поручения завершился ошибкой ({}): {safe_err}",
                    err.code()
                ),
                &mut pending_events,
            )
            .await;
        let _ = append_control_events_batch_in_tx(
            &tx,
            self.pg_target(),
            correlation_id,
            &pending_events,
        )
        .await;
        let _ = tx.commit().await;
        self.clear_stopped_task(task_id).await;
    }

    async fn record_budget_exceeded_in_tx<C: GenericClient>(
        &self,
        tx: &C,
        task: &TaskRecord,
        violation: &dennett_contracts::BudgetViolation,
        pending_events: &mut Vec<PendingControlEvent>,
    ) -> Result<(), DennettError> {
        let summary = format!(
            "Лимит бюджета «{}» исчерпан: использовано {} из {}",
            violation.dimension, violation.used, violation.limit
        );
        if task.budget.exceed_policy == BudgetExceedPolicy::Fail {
            tx.execute(
                "UPDATE tasks
                 SET status = 'failed',
                     error_code = 'BUDGET_EXCEEDED',
                     error_summary = $2,
                     finished_at = now(),
                     updated_at = now()
                 WHERE task_id = $1",
                &[&task.task_id.as_uuid(), &summary],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        } else {
            tx.execute(
                "UPDATE tasks
                 SET status = 'waiting_approval',
                     wait_detail = $2,
                     error_code = 'BUDGET_EXCEEDED',
                     error_summary = $2,
                     updated_at = now()
                 WHERE task_id = $1",
                &[&task.task_id.as_uuid(), &summary],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

            let card_id = InboxCardId::new();
            let subject = truncate_utf8(&format!("Исчерпан бюджет поручения: {}", task.title), 500)
                .to_owned();
            tx.execute(
                "INSERT INTO inbox_cards (
                    card_id, project_id, session_id, task_id, agent_id,
                    kind, status, urgency, subject, detail
                 ) VALUES ($1, $2, $3, $4, $5, 'budget_exceeded', 'open', 'high', $6, $7)",
                &[
                    &card_id.as_uuid(),
                    &task.project_id.as_uuid(),
                    &task.session_id.as_uuid(),
                    &task.task_id.as_uuid(),
                    &task.agent_id.as_uuid(),
                    &subject,
                    &summary,
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        }

        pending_events.push(PendingControlEvent {
            entity_key: format!("task:{}", task.task_id),
            event_type: "task.budget_exceeded",
            project_id: Some(task.project_id),
            session_id: Some(task.session_id),
            task_id: Some(task.task_id),
            payload: json!({
                "task_id": task.task_id,
                "dimension": violation.dimension,
                "used": violation.used,
                "limit": violation.limit,
                "exceed_policy": task.budget.exceed_policy,
            }),
        });
        Ok(())
    }

    async fn insert_task_timer_in_tx<C: GenericClient>(
        &self,
        tx: &C,
        task_id: TaskId,
        task_generation: i64,
        idempotency_key: &str,
        sched: &TaskSchedule,
        fire_at: TimestampMs,
    ) -> Result<TaskTimerId, DennettError> {
        let timer_id = TaskTimerId::new();
        let fire_secs = ms_to_pg_epoch_secs(fire_at.as_millis());
        let spec_json =
            serde_json::to_value(&sched.kind).map_err(|err| DennettError::Internal {
                detail: format!("failed serializing timer recurrence_spec: {err}"),
            })?;
        let weekdays_mask: Option<i16> = match sched.kind {
            ScheduleKind::WeeklyWallTime { weekdays_mask, .. } => Some(i16::from(weekdays_mask)),
            _ => None,
        };
        tx.execute(
            "INSERT INTO task_timers (
                timer_id, task_id, task_generation, idempotency_key,
                fire_at, timezone, original_expression, recurrence_kind,
                recurrence_spec, missed_run_policy, weekdays_mask, status
             ) VALUES (
                $1, $2, $3, $4,
                to_timestamp($5), $6, $7, $8,
                $9, $10, $11, 'pending'
             )
             ON CONFLICT (task_id, idempotency_key) DO NOTHING",
            &[
                &timer_id.as_uuid(),
                &task_id.as_uuid(),
                &task_generation,
                &truncate_utf8(idempotency_key, 128),
                &fire_secs,
                &sched.timezone.as_str(),
                &sched.original_expression.as_deref(),
                &sched.kind.recurrence_label(),
                &spec_json,
                &sched.missed_run_policy.as_str(),
                &weekdays_mask,
            ],
        )
        .await
        .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        Ok(timer_id)
    }

    /// Sends a plain-text inter-agent coordination message (`POST /v1/agents/{agent_id}/messages`,
    /// V07/A07 — zero custom consumer DAG!).
    ///
    /// Enforces per-recipient idempotency (`UNIQUE (recipient_agent_id, idempotency_key)`) and
    /// scope boundary checks without privilege escalation.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::NotFound`],
    /// [`DennettError::ForbiddenScope`], [`DennettError::IdempotencyMismatch`], or [`DennettError::DatabaseUnavailable`].
    #[expect(
        clippy::too_many_lines,
        reason = "validates recipient/sender/project, enforces idempotent insert with payload verification, and emits audit + journal events atomically"
    )]
    pub async fn send_agent_message(
        &self,
        recipient_agent_id: AgentId,
        req: SendAgentMessageRequest,
        correlation_id: &CorrelationId,
    ) -> Result<SendAgentMessageResponse, DennettError> {
        req.validate()?;
        let _ = self.ensure_migrated().await?;

        let safe_body = self.redact(req.body_text.trim());
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let recipient = fetch_agent(&tx, recipient_agent_id, self.pg_target()).await?;
        let project = ensure_project_active_in_tx(
            &tx,
            req.project_id,
            "send agent message",
            self.pg_target(),
        )
        .await?;

        // D2-SEC-01: Empty `allowed_memory_scopes` is DENY (never full access!), and sender &
        // recipient must share at least one permitted scope including the project's scope.
        if let Some(sender_id) = req.sender_agent_id {
            let sender = fetch_agent(&tx, sender_id, self.pg_target()).await?;
            let sender_scopes = &sender.current_config.allowed_memory_scopes;
            let recipient_scopes = &recipient.current_config.allowed_memory_scopes;
            if sender_scopes.is_empty() || !sender_scopes.contains(&project.memory_scope) {
                return Err(DennettError::ForbiddenScope {
                    scope: project.memory_scope.to_string(),
                    reason: format!(
                        "sender agent `{sender_id}` lacks access to project scope `{}`",
                        project.memory_scope
                    ),
                });
            }
            if sender_scopes.is_disjoint(recipient_scopes) {
                return Err(DennettError::ForbiddenScope {
                    scope: project.memory_scope.to_string(),
                    reason: format!(
                        "sender agent `{sender_id}` and recipient agent `{recipient_agent_id}` share no allowed_memory_scopes"
                    ),
                });
            }
        }

        let existing_sql = format!(
            "SELECT {AGENT_MESSAGE_SELECT_COLS}
             FROM agent_messages
             WHERE recipient_agent_id = $1 AND idempotency_key = $2"
        );
        if let Some(row) = tx
            .query_opt(
                &existing_sql,
                &[&recipient_agent_id.as_uuid(), &req.idempotency_key.as_str()],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?
        {
            let existing = decode_agent_message_row(&row)?;
            if existing.body_text != safe_body || existing.project_id != req.project_id {
                return Err(DennettError::IdempotencyMismatch {
                    idempotency_key: req.idempotency_key.to_string(),
                    reason: String::from(
                        "reused agent message idempotency_key with different body or project_id",
                    ),
                });
            }
            tx.commit()
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;
            return Ok(SendAgentMessageResponse {
                message: existing,
                deduplicated: true,
            });
        }

        let msg_id = AgentMessageId::new();
        let insert_sql = format!(
            "INSERT INTO agent_messages (
                agent_message_id, sender_agent_id, recipient_agent_id, project_id,
                related_task_id, idempotency_key, body_text, correlation_id
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             RETURNING {AGENT_MESSAGE_SELECT_COLS}"
        );
        let row = tx
            .query_one(
                &insert_sql,
                &[
                    &msg_id.as_uuid(),
                    &req.sender_agent_id.map(AgentId::as_uuid),
                    &recipient_agent_id.as_uuid(),
                    &req.project_id.as_uuid(),
                    &req.related_task_id.map(TaskId::as_uuid),
                    &req.idempotency_key.as_str(),
                    &safe_body,
                    &correlation_id.as_str(),
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let message = decode_agent_message_row(&row)?;

        let _ = append_control_event_in_tx(
            &tx,
            self.pg_target(),
            ControlEventAppend {
                entity_key: &format!("agent:{recipient_agent_id}"),
                event_type: "agent.message_sent",
                project_id: Some(req.project_id),
                session_id: None,
                task_id: req.related_task_id,
                correlation_id,
                payload: json!({
                    "agent_message_id": message.agent_message_id,
                    "sender_agent_id": message.sender_agent_id,
                    "recipient_agent_id": message.recipient_agent_id,
                    "related_task_id": message.related_task_id,
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        Ok(SendAgentMessageResponse {
            message,
            deduplicated: false,
        })
    }

    /// Lists incoming plain-text coordination messages for `recipient_agent_id`
    /// (`GET /v1/agents/{agent_id}/messages?undelivered_only=true`).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`] or [`DennettError::DatabaseUnavailable`].
    pub async fn list_agent_messages(
        &self,
        recipient_agent_id: AgentId,
        undelivered_only: bool,
    ) -> Result<ListAgentMessagesResponse, DennettError> {
        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let _ = fetch_agent(&conn, recipient_agent_id, self.pg_target()).await?;
        let sql = format!(
            "SELECT {AGENT_MESSAGE_SELECT_COLS}
             FROM agent_messages
             WHERE recipient_agent_id = $1
               AND (NOT $2::boolean OR delivered_in_run_id IS NULL)
             ORDER BY created_at ASC"
        );
        let rows = conn
            .query(&sql, &[&recipient_agent_id.as_uuid(), &undelivered_only])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let messages = rows
            .iter()
            .map(decode_agent_message_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ListAgentMessagesResponse { messages })
    }

    /// Enforces `child ⊆ parent` (A07, A10) when an agent delegates work to another agent
    /// via `dennett-control-mcp` or `parent_task_id`:
    /// - `allowed_memory_scopes`: `child ⊆ parent`
    /// - `workspace_write_policy`: `child ⊆ parent`
    /// - `autonomy_mode`: `child` cannot escalate to `FullAccess` when `parent` is `Auto`
    /// - Agent-scoped MCP/plugin capabilities: `child ⊆ parent`
    /// - Provider connections: child cannot escalate from local/free connection to a paid/cloud
    ///   connection (`GeminiApiKey` / `AntigravityAcp`) that differs from parent's connection.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::ForbiddenScope`] or [`DennettError::AccessDenied`] on privilege escalation.
    pub(crate) async fn enforce_delegated_agent_scope_subset(
        &self,
        parent_agent_id: AgentId,
        child_agent_id: AgentId,
        connection_override_id: Option<ProviderConnectionId>,
    ) -> Result<(), DennettError> {
        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        self.enforce_delegated_agent_scope_subset_on_client(
            &conn,
            parent_agent_id,
            child_agent_id,
            connection_override_id,
        )
        .await
    }

    #[expect(
        clippy::too_many_lines,
        reason = "enforces full A10 child <= parent delegation ceiling across memory scopes, write policy, autonomy mode, agent-scoped MCPs, and paid connections"
    )]
    pub(crate) async fn enforce_delegated_agent_scope_subset_on_client<C: GenericClient>(
        &self,
        client: &C,
        parent_agent_id: AgentId,
        child_agent_id: AgentId,
        connection_override_id: Option<ProviderConnectionId>,
    ) -> Result<(), DennettError> {
        let parent = fetch_agent(client, parent_agent_id, self.pg_target()).await?;
        let child = if parent_agent_id == child_agent_id {
            parent.clone()
        } else {
            fetch_agent(client, child_agent_id, self.pg_target()).await?
        };

        if !child
            .current_config
            .allowed_memory_scopes
            .is_subset(&parent.current_config.allowed_memory_scopes)
        {
            return Err(DennettError::ForbiddenScope {
                scope: format!("agent:{child_agent_id}"),
                reason: String::from(
                    "delegated helper agent's allowed_memory_scopes must be a subset of delegating agent's scopes",
                ),
            });
        }
        if !write_policy_is_subset_of(
            &child.current_config.workspace_write_policy,
            &parent.current_config.workspace_write_policy,
        ) {
            return Err(DennettError::AccessDenied {
                scope: format!("agent:{child_agent_id}"),
                reason: String::from(
                    "delegated helper agent's workspace_write_policy cannot exceed delegating agent's permissions",
                ),
            });
        }
        if child.current_config.autonomy_mode == AutonomyMode::FullAccess
            && parent.current_config.autonomy_mode != AutonomyMode::FullAccess
        {
            return Err(DennettError::AccessDenied {
                scope: format!("agent:{child_agent_id}"),
                reason: String::from(
                    "delegated helper agent cannot escalate autonomy_mode to `full_access` above parent agent's `auto` ceiling",
                ),
            });
        }

        // A10: Verify child's agent-scoped executable MCP/plugin capabilities ⊆ parent's.
        if parent_agent_id != child_agent_id {
            let child_cap_rows = client
                .query(
                    "SELECT slug FROM capability_records
                     WHERE scope_kind = 'agent'
                       AND agent_id = $1
                       AND kind IN ('mcp_server', 'plugin')
                       AND activation_state = 'enabled'",
                    &[&child_agent_id.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;
            if !child_cap_rows.is_empty() {
                let parent_cap_rows = client
                    .query(
                        "SELECT slug FROM capability_records
                         WHERE scope_kind = 'agent'
                           AND agent_id = $1
                           AND kind IN ('mcp_server', 'plugin')
                           AND activation_state = 'enabled'",
                        &[&parent_agent_id.as_uuid()],
                    )
                    .await
                    .map_err(|err| map_pg_err(self.pg_target(), &err))?;
                let parent_slugs: BTreeSet<String> =
                    parent_cap_rows.iter().map(|r| r.get(0)).collect();
                for row in &child_cap_rows {
                    let slug: String = row.get(0);
                    if !parent_slugs.contains(&slug) {
                        return Err(DennettError::AccessDenied {
                            scope: format!("agent:{child_agent_id}"),
                            reason: format!(
                                "delegated helper agent has agent-scoped MCP/plugin capability `{slug}` not granted to parent agent"
                            ),
                        });
                    }
                }
            }
        }

        // A10: Verify effective child provider connection does not escalate to a paid/cloud
        // connection beyond parent's connection ceiling.
        let child_conn_id = connection_override_id.or(child.current_config.preferred_connection_id);
        let parent_conn_id = parent.current_config.preferred_connection_id;
        if let Some(c_id) = child_conn_id
            && Some(c_id) != parent_conn_id
        {
            let child_conn_row = client
                .query_opt(
                    "SELECT provider_kind, auth_profile FROM provider_connections WHERE connection_id = $1",
                    &[&c_id.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;
            if let Some(row) = child_conn_row {
                let child_kind = ProviderKind::from_str(&row.get::<_, String>(0))?;
                let child_profile =
                    dennett_contracts::RuntimeAuthProfile::from_str(&row.get::<_, String>(1))?;
                let child_is_paid_cloud = matches!(child_kind, ProviderKind::AntigravityAcp)
                    || child_profile == dennett_contracts::RuntimeAuthProfile::GeminiApiKey;
                if child_is_paid_cloud {
                    let parent_allows_paid = match parent_conn_id {
                        Some(p_id) => {
                            let p_row = client
                                .query_opt(
                                    "SELECT provider_kind, auth_profile FROM provider_connections WHERE connection_id = $1",
                                    &[&p_id.as_uuid()],
                                )
                                .await
                                .map_err(|err| map_pg_err(self.pg_target(), &err))?;
                            p_row.is_some_and(|r| {
                                ProviderKind::from_str(&r.get::<_, String>(0)).ok()
                                    == Some(child_kind)
                                    && dennett_contracts::RuntimeAuthProfile::from_str(
                                        &r.get::<_, String>(1),
                                    )
                                    .ok()
                                        == Some(child_profile)
                            })
                        }
                        None => false,
                    };
                    if !parent_allows_paid {
                        return Err(DennettError::AccessDenied {
                            scope: format!("agent:{child_agent_id}"),
                            reason: format!(
                                "delegated helper task cannot escalate to paid/cloud provider connection `{c_id}` ({child_kind}/{child_profile}) above parent agent's connection ceiling"
                            ),
                        });
                    }
                }
            }
        }
        Ok(())
    }

    async fn deliver_task_completion_notice_in_tx<C: GenericClient>(
        &self,
        tx: &C,
        notify_session_id: SessionId,
        task: &TaskRecord,
        notice_text: &str,
        correlation_id: &CorrelationId,
        pending_events: &mut Vec<PendingControlEvent>,
    ) -> Result<(), DennettError> {
        let exists: bool = tx
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM sessions WHERE session_id = $1 AND deleted_at IS NULL)",
                &[&notify_session_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?
            .get(0);
        if !exists {
            return Ok(());
        }
        let msg_id = MessageId::new();
        let stream_id = format!("session:{notify_session_id}:messages");
        let seq = next_stream_seq(tx, &stream_id, self.pg_target()).await?;
        let safe_notice = self.redact(notice_text);
        tx.execute(
            "INSERT INTO messages (
                message_id, session_id, stream_seq, role, content, run_id, correlation_id
             ) VALUES ($1, $2, $3, 'system', $4, NULL, $5)",
            &[
                &msg_id.as_uuid(),
                &notify_session_id.as_uuid(),
                &seq,
                &safe_notice,
                &correlation_id.as_str(),
            ],
        )
        .await
        .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        tx.execute(
            "UPDATE sessions SET updated_at = now() WHERE session_id = $1",
            &[&notify_session_id.as_uuid()],
        )
        .await
        .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        pending_events.push(PendingControlEvent {
            entity_key: format!("session:{notify_session_id}"),
            event_type: "task.notified_parent_session",
            project_id: Some(task.project_id),
            session_id: Some(notify_session_id),
            task_id: Some(task.task_id),
            payload: json!({
                "task_id": task.task_id,
                "notify_session_id": notify_session_id,
                "message_id": msg_id,
                "title": task.title,
            }),
        });
        Ok(())
    }
}
