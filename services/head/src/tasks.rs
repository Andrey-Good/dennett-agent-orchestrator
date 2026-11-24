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

use std::fmt::Write as _;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::{Duration, Instant};

use deadpool_postgres::GenericClient;
use dennett_contracts::{
    AgentId, AgentMessageId, AgentMessageRecord, ArtifactCompleteness, BudgetExceedPolicy,
    CancelTaskResponse, CorrelationId, CreateArtifactRequest, CreateTaskRequest, DennettError,
    DispatchEffectRequest, EffectClaimRecord, ExternalEffectStatus, IdempotencyKey, InboxCardId,
    ListAgentMessagesResponse, ListTasksResponse, MessageId, MissedRunPolicy, ProjectId,
    ProjectLifecycleState, ReplyTaskRequest, RunId, RunStatus, ScheduleKind, SchedulerTickResponse,
    SendAgentMessageRequest, SendAgentMessageResponse, SessionId, SimEffectBehavior,
    TaskDetailResponse, TaskId, TaskRecord, TaskSchedule, TaskSignalId, TaskSignalOutcome,
    TaskSignalRequest, TaskSignalResponse, TaskStatus, TaskTimerId, TaskUsage, TimestampMs,
    UpdateTaskRequest, UsageConfidence, WorkspaceWritePolicy, write_policy_is_subset_of,
};
use dennett_runtime_antigravity::{RuntimeEvent, SessionStartRequest, TurnOutcome, TurnRequest};
use serde_json::json;
use tokio::sync::watch;
use tokio::time;

use crate::approvals::{DEFAULT_APPROVAL_WAIT_TIMEOUT, InteractivePolicyEvaluator};
use crate::effects::SIMULATED_EXTERNAL_CONNECTOR_ID;
use crate::events::{ControlEventAppend, append_control_event_in_tx};
use crate::migrations::sha256_hex;
use crate::policy::PolicyContext;
use crate::service::{
    ActiveRunGuard, ActiveRunHandle, ActiveTaskGuard, ActiveTaskHandle, DEFAULT_PG_OP_TIMEOUT,
    DEFAULT_TURN_TIMEOUT, HeadService, derive_auto_session_title, insert_permission_decision,
};
use crate::store::{
    AGENT_MESSAGE_SELECT_COLS, EFFECT_CLAIM_SELECT_COLS, INBOX_CARD_SELECT_COLS, TASK_SELECT_COLS,
    TASK_SIGNAL_SELECT_COLS, TASK_TIMER_SELECT_COLS, append_audit_event, decode_agent_message_row,
    decode_effect_claim_row, decode_inbox_card_row, decode_task_row, decode_task_signal_row,
    decode_task_timer_row, fetch_agent, fetch_policy_rules, fetch_project, fetch_runtime_binding,
    fetch_session, fetch_session_messages, fetch_task, fetch_task_runs, map_pg_err,
    ms_to_pg_epoch_secs, next_stream_seq, truncate_utf8,
};

/// Grace threshold (60s) beyond `fire_at` before an overdue timer triggers `missed_run_policy` (`Skip`/`Ask`).
pub const MISFIRE_GRACE_MS: i64 = 60_000;
/// Lease window held by a scheduler tick while claiming due `task_timers`.
pub const TIMER_CLAIM_LEASE_SECS: f64 = 30.0;
/// Poll interval for the background `HeadScheduler` task inside `dennett-head`.
pub const BACKGROUND_SCHEDULER_POLL_INTERVAL: Duration = Duration::from_millis(500);

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
            let parent_agent = fetch_agent(&tx, parent_task.agent_id, self.pg_target()).await?;
            if !agent
                .current_config
                .allowed_memory_scopes
                .is_subset(&parent_agent.current_config.allowed_memory_scopes)
            {
                return Err(DennettError::ForbiddenScope {
                    scope: format!("agent:{}", agent.agent_id),
                    reason: String::from(
                        "delegated helper agent's allowed_memory_scopes must be a subset of parent agent's scopes",
                    ),
                });
            }
            if !write_policy_is_subset_of(
                &agent.current_config.workspace_write_policy,
                &parent_agent.current_config.workspace_write_policy,
            ) {
                return Err(DennettError::AccessDenied {
                    scope: format!("agent:{}", agent.agent_id),
                    reason: String::from(
                        "delegated helper agent's workspace_write_policy cannot exceed parent agent's permissions",
                    ),
                });
            }
            effective_budget = effective_budget.clamp_to_parent(&parent_task.budget);
        }

        let title = req
            .title
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| derive_auto_session_title(&req.goal))
            .unwrap_or_else(|| String::from("Поручение"));

        let session_id = if let Some(sid) = req.session_id {
            let sess = fetch_session(&tx, sid, self.pg_target()).await?;
            if sess.project_id != req.project_id {
                return Err(DennettError::InvalidInput {
                    field: String::from("session_id"),
                    reason: String::from("session does not belong to the specified project"),
                });
            }
            sid
        } else {
            let sid = SessionId::new();
            tx.execute(
                "INSERT INTO sessions (session_id, agent_id, project_id, title, status)
                 VALUES ($1, $2, $3, $4, 'active')",
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

        let now = TimestampMs::now();
        let mut schedule_opt = req.schedule.clone();
        let (initial_status, wait_detail, initial_fire_at) =
            if let Some(ref mut sched) = schedule_opt {
                verify_pg_timezone(&tx, &sched.timezone, self.pg_target()).await?;
                let fire_at = compute_next_fire_at_pg(&tx, sched, now, self.pg_target()).await?;
                sched.next_fire_at = fire_at;
                (
                    TaskStatus::WaitingTimer,
                    Some(format!("Ожидает расписания ({})", sched.timezone)),
                    fire_at,
                )
            } else if let Some(ref sig_name) = req.wait_for_signal {
                (
                    TaskStatus::WaitingSignal,
                    Some(format!("Ожидает сигнал `{}`", sig_name.trim())),
                    None,
                )
            } else {
                (TaskStatus::Queued, None, None)
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

        let insert_sql = format!(
            "INSERT INTO tasks (
                task_id, project_id, session_id, agent_id, parent_task_id,
                title, goal, generation, status, wait_detail,
                schedule_json, budget_json, attached_skills, auth_profile_override
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, 1, $8, $9, $10, $11, $12, $13)
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
                    &schedule_json,
                    &budget_json,
                    &skills_json,
                    &auth_override_str,
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
            self.spawn_task_execution(task.task_id, task.generation, None, correlation_id.clone());
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
             WHERE ($1::uuid IS NULL OR project_id = $1)
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

        let lock_sql =
            format!("SELECT {TASK_SELECT_COLS} FROM tasks WHERE task_id = $1 FOR UPDATE");
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
        let (next_status, next_wait_detail) = if goal_or_schedule_changed {
            // Cancel stale timers of previous generations.
            tx.execute(
                "UPDATE task_timers
                 SET status = 'cancelled'
                 WHERE task_id = $1 AND status IN ('pending', 'claimed', 'paused')",
                &[&task_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

            // Supersede stale open clarification / missed_schedule cards.
            tx.execute(
                "UPDATE inbox_cards
                 SET status = 'superseded',
                     resolution_summary = 'Цель поручения обновлена владельцем',
                     revision = revision + 1,
                     resolved_at = now(),
                     updated_at = now()
                 WHERE task_id = $1
                   AND status = 'open'
                   AND kind IN ('clarification', 'missed_schedule')",
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
                )
            } else {
                (TaskStatus::Queued, None)
            }
        } else {
            (existing.status, existing.wait_detail.clone())
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
                 schedule_json = $7,
                 budget_json = $8,
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
            self.signal_old_task_generation_preempt(task_id, updated.generation)
                .await;
            if updated.status == TaskStatus::Queued && req.execute_immediately.unwrap_or(true) {
                self.spawn_task_execution(
                    task_id,
                    updated.generation,
                    None,
                    correlation_id.clone(),
                );
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
        let lock_sql =
            format!("SELECT {TASK_SELECT_COLS} FROM tasks WHERE task_id = $1 FOR UPDATE");
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

        let (outcome_status, should_wake, updated_task) = match task.status {
            TaskStatus::Cancelled | TaskStatus::UnknownExternal => {
                (TaskSignalOutcome::RejectedStopped, false, task)
            }
            TaskStatus::WaitingSignal => {
                let wake_sql = format!(
                    "UPDATE tasks
                     SET status = 'queued',
                         wait_detail = NULL,
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
            let extra = format!(
                "[Signal `{}` received]\n{}",
                signal.signal_name, signal.payload_text
            );
            self.spawn_task_execution(
                task_id,
                updated_task.generation,
                Some(extra),
                correlation_id.clone(),
            );
        }

        Ok(TaskSignalResponse {
            signal,
            task: updated_task,
            deduplicated: false,
        })
    }

    /// Answers an agent's native clarification request (`POST /v1/tasks/{task_id}/reply`),
    /// resolving the open `clarification` Inbox card and continuing task execution cleanly.
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
        req.validate()?;
        let _ = self.ensure_migrated().await?;

        let safe_answer = self.redact(req.answer_text.trim());
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let lock_sql =
            format!("SELECT {TASK_SELECT_COLS} FROM tasks WHERE task_id = $1 FOR UPDATE");
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

        let update_sql = format!(
            "UPDATE tasks
             SET status = 'queued',
                 wait_detail = NULL,
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

        let continuation_input = format!("[User Clarification Reply]\n{safe_answer}");
        self.spawn_task_execution(
            task_id,
            updated.generation,
            Some(continuation_input),
            correlation_id.clone(),
        );

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
            let _ = self.runtime().cancel_turn(session_id).await;
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

        let lock_sql =
            format!("SELECT {TASK_SELECT_COLS} FROM tasks WHERE task_id = $1 FOR UPDATE");
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
        for srow in &stranded_rows {
            let claim = decode_effect_claim_row(srow)?;
            self.mark_unknown_effect_side_effects_in_tx(&tx, &claim, correlation_id)
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

        let _ = append_control_event_in_tx(
            &tx,
            self.pg_target(),
            ControlEventAppend {
                entity_key: &format!("task:{task_id}"),
                event_type,
                project_id: Some(updated_task.project_id),
                session_id: Some(updated_task.session_id),
                task_id: Some(task_id),
                correlation_id,
                payload: json!({
                    "task_id": task_id,
                    "status": updated_task.status,
                    "external_outcome_unknown": external_outcome_unknown,
                    "committed_effects_count": committed_effects.len(),
                    "unknown_effects_count": unknown_effects.len(),
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

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
                WHERE p.lifecycle_state IN ('active', 'detached')
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

        for row in &claimed_rows {
            let timer = decode_task_timer_row(row)?;
            let task_sql =
                format!("SELECT {TASK_SELECT_COLS} FROM tasks WHERE task_id = $1 FOR UPDATE");
            let task_row = tx
                .query_one(&task_sql, &[&timer.task_id.as_uuid()])
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;
            let task = decode_task_row(&task_row)?;

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

                let _ = append_control_event_in_tx(
                    &tx,
                    self.pg_target(),
                    ControlEventAppend {
                        entity_key: &format!("task:{}", task.task_id),
                        event_type: "task.timer_skipped",
                        project_id: Some(task.project_id),
                        session_id: Some(task.session_id),
                        task_id: Some(task.task_id),
                        correlation_id,
                        payload: json!({
                            "timer_id": timer.timer_id,
                            "overdue_ms": overdue_ms,
                            "policy": "skip",
                        }),
                    },
                )
                .await?;
                skipped_timers += 1;
            } else if is_misfired && timer.missed_run_policy == MissedRunPolicy::Ask {
                tx.execute(
                    "UPDATE task_timers SET status = 'paused' WHERE timer_id = $1",
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
                    "Таймер поручения `{}` ({}) просрочен на {} сек. Запустить сейчас один раз или пропустить до следующего окна?",
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

                let _ = append_control_event_in_tx(
                    &tx,
                    self.pg_target(),
                    ControlEventAppend {
                        entity_key: &format!("task:{}", task.task_id),
                        event_type: "task.schedule_missed",
                        project_id: Some(task.project_id),
                        session_id: Some(task.session_id),
                        task_id: Some(task.task_id),
                        correlation_id,
                        payload: json!({
                            "timer_id": timer.timer_id,
                            "card_id": card_id,
                            "overdue_ms": overdue_ms,
                            "policy": "ask",
                        }),
                    },
                )
                .await?;
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

                let _ = append_control_event_in_tx(
                    &tx,
                    self.pg_target(),
                    ControlEventAppend {
                        entity_key: &format!("task:{}", task.task_id),
                        event_type: "task.timer_fired",
                        project_id: Some(task.project_id),
                        session_id: Some(task.session_id),
                        task_id: Some(task.task_id),
                        correlation_id,
                        payload: json!({
                            "timer_id": timer.timer_id,
                            "task_generation": task.generation,
                            "overdue_ms": overdue_ms,
                            "policy": timer.missed_run_policy,
                        }),
                    },
                )
                .await?;
                woken_tasks += 1;
                to_spawn.push((task.task_id, task.generation));
            }
        }

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        for (tid, gen_num) in to_spawn {
            self.spawn_task_execution(tid, gen_num, None, correlation_id.clone());
        }

        Ok(SchedulerTickResponse {
            now,
            claimed_timers,
            woken_tasks,
            skipped_timers,
            asked_timers,
        })
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
            let _ = svc.recover_stale_dispatching_effects(&boot_cid).await;
            let mut interval = time::interval(BACKGROUND_SCHEDULER_POLL_INTERVAL);
            interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    biased;
                    changed = shutdown_rx.changed() => {
                        if changed.is_err() || *shutdown_rx.borrow() {
                            break;
                        }
                    }
                    _ = interval.tick() => {
                        let cid = CorrelationId::generate();
                        let _ = svc.tick_scheduler_at(TimestampMs::now(), &cid).await;
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
                 fired_at = CASE WHEN $2 = 'fired' THEN now() ELSE fired_at END
             WHERE task_id = $1 AND status = 'paused'",
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

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        if run_now {
            self.spawn_task_execution(
                task_id,
                updated_task.generation,
                None,
                correlation_id.clone(),
            );
        }
        Ok(updated_task)
    }

    pub(crate) fn spawn_task_execution(
        &self,
        task_id: TaskId,
        expected_generation: i64,
        extra_input: Option<String>,
        correlation_id: CorrelationId,
    ) {
        let svc = self.clone();
        tokio::spawn(async move {
            let _ = svc
                .execute_task_generation(task_id, expected_generation, extra_input, correlation_id)
                .await;
        });
    }

    /// Executes one queued task turn for `(task_id, expected_generation)` with external budget
    /// enforcement, interactive `ask_before_write` support, plain-text `agent_messages` delivery,
    /// `auth_profile_override` fail-closed separation, and strict `generation` fencing (V07).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError`] if control PostgreSQL fails.
    #[expect(
        clippy::too_many_lines,
        reason = "task execution orchestrates per-session serialization, pre-turn budget/auth guards, out-of-tx runtime turn, partial artifacts/effects, and generation-fenced completion"
    )]
    pub async fn execute_task_generation(
        &self,
        task_id: TaskId,
        expected_generation: i64,
        extra_input: Option<String>,
        correlation_id: CorrelationId,
    ) -> Result<(), DennettError> {
        let _ = self.ensure_migrated().await?;

        // Look up session_id first so we serialize turns on the same session cleanly without 409 races
        // when generation N+1 queues right behind generation N!
        let session_id = {
            let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
            let task = fetch_task(&conn, task_id, self.pg_target()).await?;
            task.session_id
        };
        let session_lock = self.session_exec_lock(session_id).await;
        let _session_guard = session_lock.lock().await;

        let run_id = RunId::new();
        let (
            task,
            agent,
            project,
            existing_binding,
            prior_history,
            policy_rules,
            user_msg_id,
            prompt_text,
        ) = {
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

            // Only start if still queued for THIS exact generation and not cancelled/unknown!
            if task.generation != expected_generation || task.status != TaskStatus::Queued {
                tx.commit()
                    .await
                    .map_err(|err| map_pg_err(self.pg_target(), &err))?;
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
                return Ok(());
            }

            // 1. Pre-turn external budget check strictly outside the model (Contract G).
            if let Some(violation) = task.budget.check_before_next_turn(&task.usage) {
                self.record_budget_exceeded_in_tx(&tx, &task, &violation, &correlation_id)
                    .await?;
                tx.commit()
                    .await
                    .map_err(|err| map_pg_err(self.pg_target(), &err))?;
                return Ok(());
            }

            // 2. Auth profile separation check (A10: fail closed if gemini-api-key not configured!).
            if let Err(auth_err) = self.runtime_for_profile(task.auth_profile_override) {
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
                return Ok(());
            }

            let agent = fetch_agent(&tx, task.agent_id, self.pg_target()).await?;
            let binding = fetch_runtime_binding(&tx, task.session_id, self.pg_target()).await?;
            let history = fetch_session_messages(&tx, task.session_id, self.pg_target()).await?;
            let rules = fetch_policy_rules(&tx, self.pg_target()).await?;

            // Collect undelivered plain-text inter-agent messages for `(agent_id, project_id)` (A07, V07)!
            let undelivered_sql = format!(
                "SELECT {AGENT_MESSAGE_SELECT_COLS}
                 FROM agent_messages
                 WHERE recipient_agent_id = $1
                   AND project_id = $2
                   AND delivered_in_run_id IS NULL
                 ORDER BY created_at ASC
                 FOR UPDATE"
            );
            let msg_rows = tx
                .query(
                    &undelivered_sql,
                    &[&agent.agent_id.as_uuid(), &project.project_id.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;
            let incoming_agent_msgs: Vec<AgentMessageRecord> = msg_rows
                .iter()
                .map(decode_agent_message_row)
                .collect::<Result<Vec<_>, _>>()?;

            let mut prompt_sections = Vec::new();
            if !incoming_agent_msgs.is_empty() {
                let mut block = String::from("[Inter-Agent Messages]\n");
                for m in &incoming_agent_msgs {
                    let _ = writeln!(block, "- From {:?}: {}", m.sender_agent_id, m.body_text);
                }
                prompt_sections.push(block);
            }
            let is_clarification_reply = extra_input
                .as_deref()
                .is_some_and(|extra| extra.starts_with("[User Clarification Reply]"));
            if let Some(ref extra) = extra_input {
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
            let stream_id = format!("session:{}:messages", task.session_id);
            let msg_seq = next_stream_seq(&tx, &stream_id, self.pg_target()).await?;

            tx.execute(
                "INSERT INTO runs (
                    run_id, session_id, agent_id, agent_config_version_id,
                    user_message_id, status, task_id, task_generation, correlation_id
                 ) VALUES ($1, $2, $3, $4, $5, 'running', $6, $7, $8)",
                &[
                    &run_id.as_uuid(),
                    &task.session_id.as_uuid(),
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
                    &task.session_id.as_uuid(),
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
                agent,
                project,
                binding,
                history,
                rules,
                user_msg_id,
                full_prompt,
            )
        };

        let (cancel_tx, cancel_rx) = watch::channel(false);
        self.register_active_task(
            task_id,
            ActiveTaskHandle {
                generation: expected_generation,
                session_id: task.session_id,
                run_id,
                cancel_tx: cancel_tx.clone(),
            },
        )
        .await;
        let _task_guard = ActiveTaskGuard::new(self.active_tasks_map(), task_id, run_id);

        self.register_active_session_run(
            task.session_id,
            ActiveRunHandle {
                run_id,
                cancel_tx: cancel_tx.clone(),
            },
        )
        .await;
        let _run_guard = ActiveRunGuard::new(self.active_runs_map(), task.session_id, run_id);

        let runtime = self.runtime_for_profile(task.auth_profile_override)?;
        let (effective_workspace_cwd, effective_write_policy) = match project.workspace_path {
            Some(ref ws_path) => (
                PathBuf::from(ws_path),
                agent.current_config.workspace_write_policy.clone(),
            ),
            None => (
                self.ensure_detached_project_dir(project.project_id)?,
                WorkspaceWritePolicy::DenyAll,
            ),
        };

        let mcp_servers = self.build_memory_mcp_specs(&agent);
        let turn_secrets = self.collect_turn_secrets(&mcp_servers);
        let started_instant = Instant::now();

        let start_outcome = match runtime
            .start_or_resume_session(SessionStartRequest {
                dennett_session_id: task.session_id,
                workspace_cwd: effective_workspace_cwd.clone(),
                system_prompt: agent.current_config.system_prompt.clone(),
                model_preference: agent.current_config.model_preference.clone(),
                thinking_level: agent.current_config.thinking_level,
                workspace_write_policy: effective_write_policy.clone(),
                mcp_servers,
                existing_binding,
                history_for_rehydrate: prior_history,
                allow_rehydrate: true,
            })
            .await
        {
            Ok(outcome) => outcome,
            Err(err) => {
                let elapsed_ms = u64::try_from(started_instant.elapsed().as_millis()).unwrap_or(0);
                self.complete_task_run_error(
                    task_id,
                    run_id,
                    expected_generation,
                    &err,
                    elapsed_ms,
                    &[],
                    &correlation_id,
                )
                .await;
                return Ok(());
            }
        };

        let _ = self
            .upsert_runtime_binding(
                task.session_id,
                &start_outcome,
                &correlation_id,
                Some(run_id),
            )
            .await;

        let policy_ctx = PolicyContext {
            project_id: project.project_id,
            agent_id: agent.agent_id,
            session_id: task.session_id,
            task_id: Some(task_id),
            workspace_root: effective_workspace_cwd,
            workspace_write_policy: effective_write_policy,
            allowed_memory_scopes: agent.current_config.allowed_memory_scopes.clone(),
            rules: policy_rules,
        };
        let (interactive_policy, decisions_buf) = InteractivePolicyEvaluator::new(
            self.clone(),
            policy_ctx,
            Some(task_id),
            correlation_id.clone(),
            turn_secrets,
            cancel_rx.clone(),
            DEFAULT_APPROVAL_WAIT_TIMEOUT,
        );

        let turn_timeout = task
            .budget
            .max_wall_secs
            .map_or(DEFAULT_TURN_TIMEOUT, |limit_secs| {
                let limit_ms = limit_secs.saturating_mul(1_000);
                let rem_ms = limit_ms.saturating_sub(task.usage.wall_ms_used).max(500);
                Duration::from_millis(rem_ms).min(DEFAULT_TURN_TIMEOUT)
            });

        let turn_result = runtime
            .send_prompt_turn(
                TurnRequest {
                    dennett_session_id: task.session_id,
                    provider_session_id: start_outcome.provider_session_id.clone(),
                    run_id,
                    prompt_text,
                    timeout: turn_timeout,
                    cancel_rx: cancel_rx.clone(),
                },
                std::sync::Arc::new(interactive_policy),
                None,
            )
            .await;

        let elapsed_ms = u64::try_from(started_instant.elapsed().as_millis()).unwrap_or(0);
        let captured = decisions_buf.lock().map_or_default(|g| g.clone());

        let turn_outcome = match turn_result {
            Ok(outcome) => outcome,
            Err(err) => {
                self.complete_task_run_error(
                    task_id,
                    run_id,
                    expected_generation,
                    &err,
                    elapsed_ms,
                    &captured,
                    &correlation_id,
                )
                .await;
                return Ok(());
            }
        };

        // Process any emitted partial artifacts and external effects (strictly checking cancel FIRST!).
        let mut hit_unknown_external = false;
        for ev in &turn_outcome.events {
            match ev {
                RuntimeEvent::PartialArtifactEmitted { name, content } => {
                    let _ = self
                        .create_artifact(
                            task.session_id,
                            CreateArtifactRequest {
                                run_id: Some(run_id),
                                name: name.clone(),
                                mime_type: String::from("text/markdown"),
                                content: Some(content.clone()),
                                relative_path: None,
                                completeness_status: ArtifactCompleteness::Partial,
                            },
                            &correlation_id,
                        )
                        .await;
                }
                RuntimeEvent::ExternalEffectRequested {
                    recipient,
                    operation,
                    behavior,
                    idempotency_key,
                } => {
                    if *cancel_rx.borrow() || self.is_task_cancel_signalled(task_id).await {
                        break;
                    }
                    if let Ok(idem) = IdempotencyKey::try_new(idempotency_key.clone()) {
                        let sim_behavior =
                            SimEffectBehavior::from_str(behavior).unwrap_or_default();
                        if let Ok(eff_resp) = self
                            .dispatch_external_effect(
                                DispatchEffectRequest {
                                    project_id: task.project_id,
                                    session_id: task.session_id,
                                    task_id: Some(task_id),
                                    run_id: Some(run_id),
                                    agent_id: task.agent_id,
                                    effect_idempotency_key: idem,
                                    connector_id: Some(String::from(
                                        SIMULATED_EXTERNAL_CONNECTOR_ID,
                                    )),
                                    operation: operation.clone(),
                                    target_recipient: recipient.clone(),
                                    parameters_json: String::from("{}"),
                                    simulated_behavior: Some(sim_behavior),
                                },
                                &correlation_id,
                            )
                            .await
                            && eff_resp.external_outcome_unknown
                        {
                            hit_unknown_external = true;
                        }
                    }
                }
                _ => {}
            }
        }

        let _ = self
            .complete_task_run(
                task_id,
                run_id,
                expected_generation,
                user_msg_id,
                start_outcome.rehydrated,
                &turn_outcome,
                elapsed_ms,
                &captured,
                hit_unknown_external,
                &correlation_id,
            )
            .await?;

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
                     finished_at = now()
                 WHERE run_id = $1",
                &[
                    &run_id.as_uuid(),
                    &assistant_msg_id.map(MessageId::as_uuid),
                    &rehydrated,
                    &tokens_i64,
                    &wall_i64,
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

        // Persist assistant message into session timeline when non-empty.
        let safe_text = self.redact(&outcome.assistant_text);
        let assistant_msg_id = if safe_text.trim().is_empty() {
            None
        } else {
            let mid = MessageId::new();
            let stream_id = format!("session:{}:messages", current_task.session_id);
            let seq = next_stream_seq(&tx, &stream_id, self.pg_target()).await?;
            tx.execute(
                "INSERT INTO messages (
                    message_id, session_id, stream_seq, role, content, run_id, correlation_id
                 ) VALUES ($1, $2, $3, 'assistant', $4, $5, $6)",
                &[
                    &mid.as_uuid(),
                    &current_task.session_id.as_uuid(),
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

        let partial_summary = if safe_text.trim().is_empty() {
            current_task.partial_result_summary.clone()
        } else {
            Some(truncate_utf8(&safe_text, 16_000).to_owned())
        };

        // Check if task was already cancelled or hit unknown_external.
        if current_task.status == TaskStatus::Cancelled || outcome.status == RunStatus::Cancelled {
            tx.execute(
                "UPDATE runs
                 SET assistant_message_id = $2,
                     status = 'cancelled',
                     stop_reason = 'cancelled',
                     rehydrated = $3,
                     tokens_used = $4,
                     wall_ms = $5,
                     finished_at = now()
                 WHERE run_id = $1",
                &[
                    &run_id.as_uuid(),
                    &assistant_msg_id.map(MessageId::as_uuid),
                    &rehydrated,
                    &tokens_i64,
                    &wall_i64,
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

            tx.execute(
                "UPDATE tasks
                 SET status = CASE WHEN status = 'unknown_external' THEN 'unknown_external' ELSE 'cancelled' END,
                     active_run_id = CASE WHEN active_run_id = $2 THEN NULL ELSE active_run_id END,
                     partial_result_summary = COALESCE($3, partial_result_summary),
                     finished_at = COALESCE(finished_at, now()),
                     updated_at = now()
                 WHERE task_id = $1",
                &[
                    &task_id.as_uuid(),
                    &run_id.as_uuid(),
                    &partial_summary.as_deref(),
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

            tx.commit()
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;
            return Ok(RunStatus::Cancelled);
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

        if has_unknown_effect {
            tx.execute(
                "UPDATE tasks SET status = 'unknown_external', updated_at = now() WHERE task_id = $1",
                &[&task_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        } else if let Some(violation) = current_task.budget.check_after_turn(&next_usage) {
            self.record_budget_exceeded_in_tx(&tx, &updated_task_view, &violation, correlation_id)
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

            let _ = append_control_event_in_tx(
                &tx,
                self.pg_target(),
                ControlEventAppend {
                    entity_key: &format!("task:{task_id}"),
                    event_type: "task.clarification_requested",
                    project_id: Some(current_task.project_id),
                    session_id: Some(current_task.session_id),
                    task_id: Some(task_id),
                    correlation_id,
                    payload: json!({
                        "task_id": task_id,
                        "card_id": card_id,
                        "question": safe_q,
                    }),
                },
            )
            .await?;
        } else {
            // Check whether a recurring future timer is already armed for this task.
            let has_pending_timer: bool = tx
                .query_one(
                    "SELECT EXISTS(
                        SELECT 1 FROM task_timers
                        WHERE task_id = $1 AND status = 'pending' AND task_generation = $2
                    )",
                    &[&task_id.as_uuid(), &run_generation],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?
                .get(0);

            let (next_status, event_type) = if has_pending_timer {
                (TaskStatus::WaitingTimer, "task.turn_completed")
            } else {
                (TaskStatus::Succeeded, "task.succeeded")
            };
            tx.execute(
                "UPDATE tasks
                 SET status = $2,
                     wait_detail = CASE WHEN $2 = 'waiting_timer' THEN 'Ожидает следующего запуска по расписанию' ELSE NULL END,
                     finished_at = CASE WHEN $2 = 'succeeded' THEN now() ELSE finished_at END,
                     updated_at = now()
                 WHERE task_id = $1",
                &[&task_id.as_uuid(), &next_status.as_str()],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

            let _ = append_control_event_in_tx(
                &tx,
                self.pg_target(),
                ControlEventAppend {
                    entity_key: &format!("task:{task_id}"),
                    event_type,
                    project_id: Some(current_task.project_id),
                    session_id: Some(current_task.session_id),
                    task_id: Some(task_id),
                    correlation_id,
                    payload: json!({
                        "task_id": task_id,
                        "run_id": run_id,
                        "generation": run_generation,
                        "status": next_status,
                        "turns_used": next_usage.turns_used,
                        "tokens_used": next_usage.tokens_used,
                    }),
                },
            )
            .await?;
        }

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        Ok(final_run_status)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "records failed task run safely respecting generation fencing"
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
        let safe_err = truncate_utf8(&self.redact(&err.to_string()), 2000).to_owned();
        let wall_i64 = i64::try_from(elapsed_ms).ok();
        let task_res = fetch_task(&tx, task_id, self.pg_target()).await;
        if let Ok(task) = task_res {
            if task.generation == run_generation {
                let _ = tx
                    .execute(
                        "UPDATE runs
                         SET status = 'failed',
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
                    TaskStatus::Cancelled | TaskStatus::UnknownExternal
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
                             WHERE task_id = $1",
                            &[
                                &task_id.as_uuid(),
                                &err.code(),
                                &safe_err,
                                &run_id.as_uuid(),
                            ],
                        )
                        .await;
                    let _ = append_control_event_in_tx(
                        &tx,
                        self.pg_target(),
                        ControlEventAppend {
                            entity_key: &format!("task:{task_id}"),
                            event_type: "task.failed",
                            project_id: Some(task.project_id),
                            session_id: Some(task.session_id),
                            task_id: Some(task_id),
                            correlation_id,
                            payload: json!({
                                "task_id": task_id,
                                "run_id": run_id,
                                "error_code": err.code(),
                                "error_summary": safe_err,
                            }),
                        },
                    )
                    .await;
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
        }
        let _ = tx.commit().await;
    }

    async fn record_budget_exceeded_in_tx<C: GenericClient>(
        &self,
        tx: &C,
        task: &TaskRecord,
        violation: &dennett_contracts::BudgetViolation,
        correlation_id: &CorrelationId,
    ) -> Result<(), DennettError> {
        let summary = format!(
            "Лимит бюджета `{}` исчерпан: использовано {} из {}",
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

        let _ = append_control_event_in_tx(
            tx,
            self.pg_target(),
            ControlEventAppend {
                entity_key: &format!("task:{}", task.task_id),
                event_type: "task.budget_exceeded",
                project_id: Some(task.project_id),
                session_id: Some(task.session_id),
                task_id: Some(task.task_id),
                correlation_id,
                payload: json!({
                    "task_id": task.task_id,
                    "dimension": violation.dimension,
                    "used": violation.used,
                    "limit": violation.limit,
                    "exceed_policy": task.budget.exceed_policy,
                }),
            },
        )
        .await?;
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
        tx.execute(
            "INSERT INTO task_timers (
                timer_id, task_id, task_generation, idempotency_key,
                fire_at, timezone, original_expression, recurrence_kind,
                recurrence_spec, missed_run_policy, status
             ) VALUES (
                $1, $2, $3, $4,
                to_timestamp($5), $6, $7, $8,
                $9, $10, 'pending'
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

        let _recipient = fetch_agent(&tx, recipient_agent_id, self.pg_target()).await?;
        let project = fetch_project(&tx, req.project_id, self.pg_target()).await?;

        if let Some(sender_id) = req.sender_agent_id {
            let sender = fetch_agent(&tx, sender_id, self.pg_target()).await?;
            if !sender.current_config.allowed_memory_scopes.is_empty()
                && !sender
                    .current_config
                    .allowed_memory_scopes
                    .contains(&project.memory_scope)
            {
                return Err(DennettError::ForbiddenScope {
                    scope: project.memory_scope.to_string(),
                    reason: format!(
                        "sender agent `{sender_id}` lacks access to project scope `{}`",
                        project.memory_scope
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
}
