//! PostgreSQL control-plane persistence helpers for `dennett-head`.
//!
//! Uses `control_stream_counters` for monotonic message, audit, entity, and global journal
//! ordering (Architecture 81 §8.1, V15–V18), never holds SQL transactions across network/runtime
//! calls, and maps all PostgreSQL errors through secret-redacting sanitization.

use std::collections::{BTreeSet, HashMap};
use std::str::FromStr;

use deadpool_postgres::GenericClient;
use dennett_contracts::{
    AgentConfigVersion, AgentConfigVersionId, AgentId, AgentMessageId, AgentMessageRecord,
    AgentRecord, ArtifactCompleteness, ArtifactId, ArtifactLifecycleState, ArtifactRecord,
    AuditEventId, AutonomyMode, CapabilityGrantId, CapabilityGrantRecord, ControlEventId,
    ControlEventRecord, CorrelationId, DennettError, DiagnosticEvent, EffectClaimId,
    EffectClaimRecord, ExternalEffectStatus, FollowupId, FollowupStatus, IdempotencyKey,
    InboxCardId, InboxCardKind, InboxCardRecord, InboxCardStatus, InboxCardUrgency, MemoryScope,
    MessageId, MessageRecord, MessageRole, MissedRunPolicy, OrderedOutboxItem, OrderedOutboxItemId,
    OrderedOutboxStatus, PermissionDecisionId, PermissionDecisionRecord, PermissionRequestId,
    PermissionRequestRecord, PermissionRequestStatus, PolicyEffect, PolicyRule, PolicyRuleId,
    ProjectId, ProjectLifecycleState, ProjectRecord, ProviderConnectionId, RunId, RunRecord,
    RunStatus, RuntimeAuthProfile, RuntimeBinding, RuntimeBindingId, SessionFollowupRecord,
    SessionId, SessionKind, SessionRecord, SessionStatus, Sha256Hex, TaskBudget, TaskId,
    TaskRecord, TaskSchedule, TaskSignalId, TaskSignalOutcome, TaskSignalRecord, TaskStatus,
    TaskTimerId, TaskTimerRecord, TaskTimerStatus, TaskTrigger, TaskUsage, ThinkingLevel,
    TimerPauseReason, TimestampMs, UpdateUserPreferencesRequest, UsageConfidence, UserPreferences,
    WorkspaceWritePolicy,
};
use serde_json::Value as JsonValue;
use tokio_postgres::Row;
use uuid::Uuid;

use crate::pg_target::ValidatedControlPgTarget;

pub(crate) fn format_pg_error_detail(err: &tokio_postgres::Error) -> String {
    if let Some(db) = err.as_db_error() {
        let mut msg = format!("db error [{}]: {}", db.code().code(), db.message());
        if let Some(constraint) = db.constraint() {
            msg.push_str(" (constraint=");
            msg.push_str(constraint);
            msg.push(')');
        }
        if let Some(detail) = db.detail()
            && !detail.is_empty()
        {
            msg.push_str(": ");
            msg.push_str(detail);
        }
        msg
    } else {
        err.to_string()
    }
}

pub(crate) fn map_pg_err(
    target: &ValidatedControlPgTarget,
    err: &tokio_postgres::Error,
) -> DennettError {
    let safe_detail = target.sanitize_error(&format_pg_error_detail(err));
    if let Some(db) = err.as_db_error() {
        let code = db.code().code();
        // D2-BUG-05: Map PostgreSQL data exceptions (22xxx) and check constraint violations (23514)
        // to HTTP 400 InvalidInput instead of misleading 503 DatabaseUnavailable.
        if code.starts_with("22") || code == "23514" {
            return DennettError::InvalidInput {
                field: db.constraint().unwrap_or("database").to_owned(),
                reason: safe_detail,
            };
        }
        if code == "23505" {
            return DennettError::Conflict {
                resource: db.constraint().unwrap_or("unique_constraint").to_owned(),
                expected_revision: None,
                actual_revision: None,
                reason: safe_detail,
            };
        }
    }
    DennettError::DatabaseUnavailable {
        detail: safe_detail,
    }
}

/// Returns `true` when `err` is PostgreSQL unique violation (`SQLSTATE 23505`).
pub(crate) fn is_pg_unique_violation(err: &tokio_postgres::Error) -> bool {
    err.code()
        .is_some_and(|code| code.code() == tokio_postgres::error::SqlState::UNIQUE_VIOLATION.code())
}

/// Converts a Unix epoch millisecond timestamp (or bounded millisecond duration) into PostgreSQL
/// `double precision` epoch seconds for `to_timestamp($N::double precision)`.
#[expect(
    clippy::cast_precision_loss,
    reason = "Unix epoch millisecond timestamps stay exact within IEEE-754 f64 53-bit mantissa precision until year 287,396"
)]
pub(crate) fn ms_to_pg_epoch_secs(ms: i64) -> f64 {
    ms as f64 / 1000.0
}

/// Allocates the next monotonic sequence number inside `control_stream_counters` transactionally
/// (Architecture 81 §8.1 — never uses PostgreSQL `SERIAL` or `GENERATED AS IDENTITY`).
pub(crate) async fn next_stream_seq<C: GenericClient>(
    client: &C,
    stream_id: &str,
    target: &ValidatedControlPgTarget,
) -> Result<i64, DennettError> {
    let row = client
        .query_one(
            "INSERT INTO control_stream_counters (stream_id, committed_seq)
             VALUES ($1, 1)
             ON CONFLICT (stream_id)
             DO UPDATE SET committed_seq = control_stream_counters.committed_seq + 1
             RETURNING committed_seq",
            &[&stream_id],
        )
        .await
        .map_err(|err| map_pg_err(target, &err))?;
    Ok(row.get(0))
}

/// Allocates the next gapless global journal cursor `(cursor_seq, authority_epoch)` from
/// `control_stream_counters` (`stream_id = 'control:journal'`).
///
/// MUST be invoked as the LAST counter lock before `COMMIT` to uphold V15 gapless visibility
/// and deadlock-free lock hierarchy (`domain row -> entity:<key> -> control:journal`).
pub(crate) async fn next_global_journal_cursor<C: GenericClient>(
    client: &C,
    target: &ValidatedControlPgTarget,
) -> Result<(i64, i64), DennettError> {
    let row = client
        .query_one(
            "INSERT INTO control_stream_counters (stream_id, committed_seq, authority_epoch, retained_from_seq)
             VALUES ('control:journal', 1, 1, 1)
             ON CONFLICT (stream_id)
             DO UPDATE SET committed_seq = control_stream_counters.committed_seq + 1
             RETURNING committed_seq, authority_epoch",
            &[],
        )
        .await
        .map_err(|err| map_pg_err(target, &err))?;
    Ok((row.get(0), row.get(1)))
}

/// Appends one secret-free diagnostic/audit event into `audit_events`.
#[expect(
    clippy::too_many_arguments,
    reason = "internal helper mirrors audit_events columns directly"
)]
pub(crate) async fn append_audit_event<C: GenericClient>(
    client: &C,
    target: &ValidatedControlPgTarget,
    correlation_id: &CorrelationId,
    session_id: Option<SessionId>,
    run_id: Option<RunId>,
    component: &str,
    event_kind: &str,
    safe_summary: &str,
) -> Result<DiagnosticEvent, DennettError> {
    let seq = next_stream_seq(client, "control:audit_events", target).await?;
    let event_id = AuditEventId::new();
    let trimmed_summary = truncate_utf8(safe_summary, 2000);
    let summary_non_empty = if trimmed_summary.is_empty() {
        "event"
    } else {
        trimmed_summary
    };

    let row = client
        .query_one(
            "INSERT INTO audit_events (
                event_id, stream_seq, correlation_id, session_id, run_id,
                component, event_kind, safe_summary
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             RETURNING TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint",
            &[
                &event_id.as_uuid(),
                &seq,
                &correlation_id.as_str(),
                &session_id.map(SessionId::as_uuid),
                &run_id.map(RunId::as_uuid),
                &component,
                &event_kind,
                &summary_non_empty,
            ],
        )
        .await
        .map_err(|err| map_pg_err(target, &err))?;

    Ok(DiagnosticEvent {
        event_id,
        correlation_id: correlation_id.clone(),
        session_id,
        run_id,
        component: component.to_owned(),
        event_kind: event_kind.to_owned(),
        safe_summary: summary_non_empty.to_owned(),
        created_at: TimestampMs::from_millis(row.get(0)),
    })
}

pub(crate) const AGENT_JOIN_SELECT: &str = "
    SELECT a.agent_id, a.name, a.description, a.current_config_version_id,
           TRUNC(EXTRACT(EPOCH FROM a.created_at) * 1000)::bigint,
           TRUNC(EXTRACT(EPOCH FROM a.updated_at) * 1000)::bigint,
           v.config_version_id, v.version_number, v.system_prompt,
           v.model_preference, v.thinking_level,
           v.allowed_memory_scopes, v.workspace_write_policy,
           v.default_budget, v.attached_skills,
           TRUNC(EXTRACT(EPOCH FROM v.created_at) * 1000)::bigint,
           v.preferred_connection_id,
           v.autonomy_mode
    FROM agents a
    JOIN agent_config_versions v ON v.config_version_id = a.current_config_version_id
";

pub(crate) const PROJECT_SELECT_COLS: &str = "
    project_id, name, workspace_path, memory_scope, lifecycle_state,
    TRUNC(EXTRACT(EPOCH FROM archived_at) * 1000)::bigint,
    pinned,
    TRUNC(EXTRACT(EPOCH FROM pinned_at) * 1000)::bigint,
    TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint
";

pub(crate) const SESSION_SELECT_COLS: &str = "
    session_id, agent_id, project_id, title, session_kind, status,
    pinned,
    TRUNC(EXTRACT(EPOCH FROM pinned_at) * 1000)::bigint,
    TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
    TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint
";

pub(crate) const ARTIFACT_SELECT_COLS: &str = "
    artifact_id, session_id, project_id, run_id, name, mime_type,
    revision, content, relative_path, sha256,
    completeness_status, lifecycle_state,
    TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint
";

pub(crate) const RUN_SELECT_COLS: &str = "
    run_id, session_id, agent_id, agent_config_version_id,
    user_message_id, assistant_message_id, status, stop_reason,
    error_code, error_summary, rehydrated,
    task_id, task_generation, tokens_used, wall_ms,
    correlation_id,
    TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
    TRUNC(EXTRACT(EPOCH FROM finished_at) * 1000)::bigint,
    context_used_tokens, context_window_tokens
";

pub(crate) const SESSION_FOLLOWUP_SELECT_COLS: &str = "
    followup_id, session_id, content, status, position,
    idempotency_key, dispatched_run_id,
    TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
    TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint
";

pub(crate) const TASK_SELECT_COLS: &str = "
    task_id, project_id, session_id, agent_id, parent_task_id,
    title, goal, generation, status, wait_detail,
    partial_result_summary, error_code, error_summary, active_run_id,
    schedule_json, budget_json, turns_used, tokens_used, wall_ms_used,
    usage_confidence, attached_skills, auth_profile_override,
    TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
    TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint,
    TRUNC(EXTRACT(EPOCH FROM finished_at) * 1000)::bigint,
    expected_signal_name,
    target_session_id,
    notify_session_id,
    model_override,
    thinking_override,
    connection_override_id,
    skill_slugs_override_json,
    trigger_json
";

pub(crate) const TASK_TIMER_SELECT_COLS: &str = "
    timer_id, task_id, task_generation, idempotency_key,
    TRUNC(EXTRACT(EPOCH FROM fire_at) * 1000)::bigint,
    timezone, original_expression, recurrence_kind, missed_run_policy,
    status, lease_owner, lease_generation,
    TRUNC(EXTRACT(EPOCH FROM lease_expires_at) * 1000)::bigint,
    TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
    TRUNC(EXTRACT(EPOCH FROM fired_at) * 1000)::bigint,
    pause_reason
";

pub(crate) const TASK_SIGNAL_SELECT_COLS: &str = "
    signal_id, task_id, idempotency_key, signal_name, payload_text,
    payload_sha256, applied_generation, outcome_status, correlation_id,
    TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint
";

pub(crate) const CAPABILITY_GRANT_SELECT_COLS: &str = "
    grant_id, scope, project_id, agent_id, session_id, task_id,
    tool_pattern, target_prefixes, max_uses, uses_consumed, reason,
    TRUNC(EXTRACT(EPOCH FROM expires_at) * 1000)::bigint,
    TRUNC(EXTRACT(EPOCH FROM revoked_at) * 1000)::bigint,
    revoked_reason,
    TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint
";

pub(crate) const PERMISSION_REQUEST_SELECT_COLS: &str = "
    request_id, project_id, session_id, run_id, task_id, agent_id,
    tool_call_id, tool_name, tool_kind, target_summary, normalized_targets,
    parameters_sha256, reason, status, issued_grant_id, decision_id,
    revision,
    TRUNC(EXTRACT(EPOCH FROM expires_at) * 1000)::bigint,
    TRUNC(EXTRACT(EPOCH FROM resolved_at) * 1000)::bigint,
    correlation_id,
    TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
    preview_snippet
";

pub(crate) const EFFECT_CLAIM_SELECT_COLS: &str = "
    effect_id, project_id, session_id, task_id, run_id, agent_id,
    effect_idempotency_key, connector_id, operation, target_recipient,
    parameters_json, parameters_sha256, status, provider_operation_id,
    receipt_json, error_summary,
    TRUNC(EXTRACT(EPOCH FROM reconciled_at) * 1000)::bigint,
    correlation_id,
    TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
    TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint
";

pub(crate) const INBOX_CARD_SELECT_COLS: &str = "
    card_id, project_id, session_id, task_id, run_id, agent_id,
    kind, status, urgency, subject, detail,
    permission_request_id, effect_id, resolution_summary, revision,
    TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
    TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint,
    TRUNC(EXTRACT(EPOCH FROM resolved_at) * 1000)::bigint
";

pub(crate) const AGENT_MESSAGE_SELECT_COLS: &str = "
    agent_message_id, sender_agent_id, recipient_agent_id, project_id,
    related_task_id, idempotency_key, body_text, delivered_in_run_id,
    correlation_id,
    TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
    TRUNC(EXTRACT(EPOCH FROM delivered_at) * 1000)::bigint
";

pub(crate) const CONTROL_EVENT_SELECT_COLS: &str = "
    cursor_seq, event_id, authority_epoch, entity_key, entity_seq,
    event_type, project_id, session_id, task_id, correlation_id,
    payload_json,
    TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint
";

pub(crate) const ORDERED_OUTBOX_SELECT_COLS: &str = "
    item_id, destination, ordering_key, key_seq, cursor_seq,
    idempotency_key, payload_json, status, attempts,
    TRUNC(EXTRACT(EPOCH FROM available_at) * 1000)::bigint,
    lease_owner, lease_generation,
    TRUNC(EXTRACT(EPOCH FROM lease_expires_at) * 1000)::bigint,
    last_error, correlation_id,
    TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
    TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint
";

pub(crate) async fn fetch_agent<C: GenericClient>(
    client: &C,
    agent_id: AgentId,
    target: &ValidatedControlPgTarget,
) -> Result<AgentRecord, DennettError> {
    let sql = format!("{AGENT_JOIN_SELECT} WHERE a.agent_id = $1");
    let row = client
        .query_opt(&sql, &[&agent_id.as_uuid()])
        .await
        .map_err(|err| map_pg_err(target, &err))?;

    let Some(row) = row else {
        return Err(DennettError::NotFound {
            resource: String::from("agent"),
            id: agent_id.to_string(),
        });
    };
    decode_agent_join_row(&row)
}

pub(crate) async fn fetch_project<C: GenericClient>(
    client: &C,
    project_id: ProjectId,
    target: &ValidatedControlPgTarget,
) -> Result<ProjectRecord, DennettError> {
    let sql = format!("SELECT {PROJECT_SELECT_COLS} FROM projects WHERE project_id = $1");
    let row = client
        .query_opt(&sql, &[&project_id.as_uuid()])
        .await
        .map_err(|err| map_pg_err(target, &err))?;

    let Some(row) = row else {
        return Err(DennettError::NotFound {
            resource: String::from("project"),
            id: project_id.to_string(),
        });
    };
    decode_project_row(&row)
}

/// Verifies that `project_id` exists and is NOT in `archived` or `deleted_record` lifecycle state
/// inside an open transaction (`D2-BUG-02` / `D2-ARCH-02`).
pub(crate) async fn ensure_project_active_in_tx<C: GenericClient>(
    client: &C,
    project_id: ProjectId,
    action: &str,
    target: &ValidatedControlPgTarget,
) -> Result<ProjectRecord, DennettError> {
    let project = fetch_project(client, project_id, target).await?;
    if matches!(
        project.lifecycle_state,
        ProjectLifecycleState::Archived | ProjectLifecycleState::DeletedRecord
    ) {
        return Err(DennettError::Conflict {
            resource: String::from("project"),
            expected_revision: None,
            actual_revision: None,
            reason: format!(
                "cannot {action}: project `{}` is in `{}` lifecycle state",
                project.project_id, project.lifecycle_state
            ),
        });
    }
    Ok(project)
}

pub(crate) async fn fetch_session<C: GenericClient>(
    client: &C,
    session_id: SessionId,
    target: &ValidatedControlPgTarget,
) -> Result<SessionRecord, DennettError> {
    let sql = format!(
        "SELECT {SESSION_SELECT_COLS} FROM sessions WHERE session_id = $1 AND deleted_at IS NULL"
    );
    let row = client
        .query_opt(&sql, &[&session_id.as_uuid()])
        .await
        .map_err(|err| map_pg_err(target, &err))?;

    let Some(row) = row else {
        return Err(DennettError::NotFound {
            resource: String::from("session"),
            id: session_id.to_string(),
        });
    };

    let binding = fetch_runtime_binding(client, session_id, target).await?;
    decode_session_row(&row, binding)
}

pub(crate) async fn fetch_runtime_binding<C: GenericClient>(
    client: &C,
    session_id: SessionId,
    target: &ValidatedControlPgTarget,
) -> Result<Option<RuntimeBinding>, DennettError> {
    let row = client
        .query_opt(
            "SELECT binding_id, session_id, adapter_id, auth_profile, tier, agy_version,
                    provider_session_id, workspace_cwd, is_simulated, rehydrate_count,
                    TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
                    TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint,
                    connection_id, endpoint_fingerprint
             FROM runtime_bindings
             WHERE session_id = $1",
            &[&session_id.as_uuid()],
        )
        .await
        .map_err(|err| map_pg_err(target, &err))?;

    row.as_ref().map(decode_runtime_binding_row).transpose()
}

pub(crate) async fn fetch_all_runtime_bindings<C: GenericClient>(
    client: &C,
    target: &ValidatedControlPgTarget,
) -> Result<HashMap<SessionId, RuntimeBinding>, DennettError> {
    let rows = client
        .query(
            "SELECT binding_id, session_id, adapter_id, auth_profile, tier, agy_version,
                    provider_session_id, workspace_cwd, is_simulated, rehydrate_count,
                    TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
                    TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint,
                    connection_id, endpoint_fingerprint
             FROM runtime_bindings",
            &[],
        )
        .await
        .map_err(|err| map_pg_err(target, &err))?;

    let mut out = HashMap::with_capacity(rows.len());
    for row in &rows {
        let binding = decode_runtime_binding_row(row)?;
        out.insert(binding.session_id, binding);
    }
    Ok(out)
}

pub(crate) async fn fetch_session_messages<C: GenericClient>(
    client: &C,
    session_id: SessionId,
    target: &ValidatedControlPgTarget,
) -> Result<Vec<MessageRecord>, DennettError> {
    let rows = client
        .query(
            "SELECT message_id, session_id, role, content, run_id, client_message_id, correlation_id,
                    TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint
             FROM messages
             WHERE session_id = $1
             ORDER BY stream_seq ASC",
            &[&session_id.as_uuid()],
        )
        .await
        .map_err(|err| map_pg_err(target, &err))?;

    rows.iter().map(decode_message_row).collect()
}

pub(crate) async fn fetch_session_runs<C: GenericClient>(
    client: &C,
    session_id: SessionId,
    target: &ValidatedControlPgTarget,
) -> Result<Vec<RunRecord>, DennettError> {
    let sql =
        format!("SELECT {RUN_SELECT_COLS} FROM runs WHERE session_id = $1 ORDER BY created_at ASC");
    let rows = client
        .query(&sql, &[&session_id.as_uuid()])
        .await
        .map_err(|err| map_pg_err(target, &err))?;

    rows.iter().map(decode_run_row).collect()
}

pub(crate) async fn fetch_active_session_followups<C: GenericClient>(
    client: &C,
    session_id: SessionId,
    target: &ValidatedControlPgTarget,
) -> Result<Vec<SessionFollowupRecord>, DennettError> {
    let sql = format!(
        "SELECT {SESSION_FOLLOWUP_SELECT_COLS}
         FROM session_followups
         WHERE session_id = $1 AND status IN ('queued', 'paused')
         ORDER BY position ASC, created_at ASC"
    );
    let rows = client
        .query(&sql, &[&session_id.as_uuid()])
        .await
        .map_err(|err| map_pg_err(target, &err))?;

    rows.iter().map(decode_session_followup_row).collect()
}

pub(crate) async fn fetch_task_runs<C: GenericClient>(
    client: &C,
    task_id: TaskId,
    target: &ValidatedControlPgTarget,
) -> Result<Vec<RunRecord>, DennettError> {
    let sql =
        format!("SELECT {RUN_SELECT_COLS} FROM runs WHERE task_id = $1 ORDER BY created_at ASC");
    let rows = client
        .query(&sql, &[&task_id.as_uuid()])
        .await
        .map_err(|err| map_pg_err(target, &err))?;

    rows.iter().map(decode_run_row).collect()
}

pub(crate) async fn fetch_session_artifacts<C: GenericClient>(
    client: &C,
    session_id: SessionId,
    target: &ValidatedControlPgTarget,
) -> Result<Vec<ArtifactRecord>, DennettError> {
    let sql = format!(
        "SELECT {ARTIFACT_SELECT_COLS} FROM artifacts WHERE session_id = $1 ORDER BY created_at ASC"
    );
    let rows = client
        .query(&sql, &[&session_id.as_uuid()])
        .await
        .map_err(|err| map_pg_err(target, &err))?;

    rows.iter().map(decode_artifact_row).collect()
}

pub(crate) async fn fetch_policy_rules<C: GenericClient>(
    client: &C,
    target: &ValidatedControlPgTarget,
) -> Result<Vec<PolicyRule>, DennettError> {
    let rows = client
        .query(
            "SELECT rule_id, scope, tool_name, target_pattern, effect, reason,
                    TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint
             FROM policy_rules
             ORDER BY created_at ASC",
            &[],
        )
        .await
        .map_err(|err| map_pg_err(target, &err))?;

    rows.iter().map(decode_policy_rule_row).collect()
}

pub(crate) async fn fetch_session_permission_decisions<C: GenericClient>(
    client: &C,
    session_id: SessionId,
    target: &ValidatedControlPgTarget,
) -> Result<Vec<PermissionDecisionRecord>, DennettError> {
    let rows = client
        .query(
            "SELECT decision_id, session_id, run_id, tool_call_id, tool_name,
                    target_pattern, scope, effect, matched_rule_id, reason,
                    evaluated_outside_model, correlation_id,
                    TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint
             FROM permission_decisions
             WHERE session_id = $1
             ORDER BY created_at ASC",
            &[&session_id.as_uuid()],
        )
        .await
        .map_err(|err| map_pg_err(target, &err))?;

    rows.iter().map(decode_permission_decision_row).collect()
}

pub(crate) async fn fetch_task<C: GenericClient>(
    client: &C,
    task_id: TaskId,
    target: &ValidatedControlPgTarget,
) -> Result<TaskRecord, DennettError> {
    let sql =
        format!("SELECT {TASK_SELECT_COLS} FROM tasks WHERE task_id = $1 AND deleted_at IS NULL");
    let row = client
        .query_opt(&sql, &[&task_id.as_uuid()])
        .await
        .map_err(|err| map_pg_err(target, &err))?;
    let Some(row) = row else {
        return Err(DennettError::NotFound {
            resource: String::from("task"),
            id: task_id.to_string(),
        });
    };
    decode_task_row(&row)
}

pub(crate) async fn fetch_active_project_grants<C: GenericClient>(
    client: &C,
    project_id: ProjectId,
    target: &ValidatedControlPgTarget,
) -> Result<Vec<CapabilityGrantRecord>, DennettError> {
    let sql = format!(
        "SELECT {CAPABILITY_GRANT_SELECT_COLS}
         FROM capability_grants
         WHERE project_id = $1
           AND revoked_at IS NULL
           AND (expires_at IS NULL OR expires_at > now())
           AND (max_uses IS NULL OR uses_consumed < max_uses)
         ORDER BY created_at DESC"
    );
    let rows = client
        .query(&sql, &[&project_id.as_uuid()])
        .await
        .map_err(|err| map_pg_err(target, &err))?;
    rows.iter().map(decode_capability_grant_row).collect()
}

pub(crate) fn decode_agent_join_row(row: &Row) -> Result<AgentRecord, DennettError> {
    let agent_id = AgentId::from_uuid(row.get(0));
    let current_config_version_id = AgentConfigVersionId::from_uuid(row.get(3));
    let scopes_vec: Vec<MemoryScope> = serde_json::from_value(row.get::<_, JsonValue>(11))
        .map_err(|err| DennettError::Internal {
            detail: format!("corrupt allowed_memory_scopes JSON: {err}"),
        })?;
    let write_policy: WorkspaceWritePolicy = serde_json::from_value(row.get::<_, JsonValue>(12))
        .map_err(|err| DennettError::Internal {
            detail: format!("corrupt workspace_write_policy JSON: {err}"),
        })?;
    let default_budget: Option<TaskBudget> = row
        .get::<_, Option<JsonValue>>(13)
        .map(|val| {
            serde_json::from_value(val).map_err(|err| DennettError::Internal {
                detail: format!("corrupt default_budget JSON: {err}"),
            })
        })
        .transpose()?;
    let attached_skills: Vec<String> = serde_json::from_value(row.get::<_, JsonValue>(14))
        .map_err(|err| DennettError::Internal {
            detail: format!("corrupt attached_skills JSON: {err}"),
        })?;
    let thinking_level = row
        .get::<_, Option<String>>(10)
        .map(|s| ThinkingLevel::from_str(&s))
        .transpose()?;

    let current_config = AgentConfigVersion {
        config_version_id: AgentConfigVersionId::from_uuid(row.get(6)),
        agent_id,
        version_number: row.get(7),
        system_prompt: row.get(8),
        model_preference: row.get(9),
        thinking_level,
        allowed_memory_scopes: scopes_vec.into_iter().collect::<BTreeSet<_>>(),
        workspace_write_policy: write_policy,
        autonomy_mode: AutonomyMode::from_str(&row.get::<_, String>(17))?,
        default_budget,
        attached_skills,
        preferred_connection_id: row
            .get::<_, Option<Uuid>>(16)
            .map(ProviderConnectionId::from_uuid),
        created_at: TimestampMs::from_millis(row.get(15)),
    };

    Ok(AgentRecord {
        agent_id,
        name: row.get(1),
        description: row.get(2),
        current_config_version_id,
        current_config,
        created_at: TimestampMs::from_millis(row.get(4)),
        updated_at: TimestampMs::from_millis(row.get(5)),
    })
}

pub(crate) fn decode_config_version_row(row: &Row) -> Result<AgentConfigVersion, DennettError> {
    let scopes_vec: Vec<MemoryScope> =
        serde_json::from_value(row.get::<_, JsonValue>(6)).map_err(|err| {
            DennettError::Internal {
                detail: format!("corrupt allowed_memory_scopes JSON: {err}"),
            }
        })?;
    let write_policy: WorkspaceWritePolicy = serde_json::from_value(row.get::<_, JsonValue>(7))
        .map_err(|err| DennettError::Internal {
            detail: format!("corrupt workspace_write_policy JSON: {err}"),
        })?;
    let default_budget: Option<TaskBudget> = row
        .get::<_, Option<JsonValue>>(8)
        .map(|val| {
            serde_json::from_value(val).map_err(|err| DennettError::Internal {
                detail: format!("corrupt default_budget JSON: {err}"),
            })
        })
        .transpose()?;
    let attached_skills: Vec<String> =
        serde_json::from_value(row.get::<_, JsonValue>(9)).map_err(|err| {
            DennettError::Internal {
                detail: format!("corrupt attached_skills JSON: {err}"),
            }
        })?;
    let thinking_level = row
        .get::<_, Option<String>>(5)
        .map(|s| ThinkingLevel::from_str(&s))
        .transpose()?;

    Ok(AgentConfigVersion {
        config_version_id: AgentConfigVersionId::from_uuid(row.get(0)),
        agent_id: AgentId::from_uuid(row.get(1)),
        version_number: row.get(2),
        system_prompt: row.get(3),
        model_preference: row.get(4),
        thinking_level,
        allowed_memory_scopes: scopes_vec.into_iter().collect(),
        workspace_write_policy: write_policy,
        autonomy_mode: AutonomyMode::from_str(&row.get::<_, String>(12))?,
        default_budget,
        attached_skills,
        preferred_connection_id: row
            .get::<_, Option<Uuid>>(11)
            .map(ProviderConnectionId::from_uuid),
        created_at: TimestampMs::from_millis(row.get(10)),
    })
}

pub(crate) fn decode_project_row(row: &Row) -> Result<ProjectRecord, DennettError> {
    Ok(ProjectRecord {
        project_id: ProjectId::from_uuid(row.get(0)),
        name: row.get(1),
        workspace_path: row.get(2),
        memory_scope: MemoryScope::try_new(row.get::<_, String>(3))?,
        lifecycle_state: ProjectLifecycleState::from_str(&row.get::<_, String>(4))?,
        archived_at: row.get::<_, Option<i64>>(5).map(TimestampMs::from_millis),
        pinned: row.get(6),
        pinned_at: row.get::<_, Option<i64>>(7).map(TimestampMs::from_millis),
        created_at: TimestampMs::from_millis(row.get(8)),
    })
}

pub(crate) fn decode_session_row(
    row: &Row,
    runtime_binding: Option<RuntimeBinding>,
) -> Result<SessionRecord, DennettError> {
    Ok(SessionRecord {
        session_id: SessionId::from_uuid(row.get(0)),
        agent_id: AgentId::from_uuid(row.get(1)),
        project_id: row.get::<_, Option<Uuid>>(2).map(ProjectId::from_uuid),
        title: row.get(3),
        session_kind: SessionKind::from_str(&row.get::<_, String>(4))?,
        status: SessionStatus::from_str(&row.get::<_, String>(5))?,
        runtime_binding,
        pinned: row.get(6),
        pinned_at: row.get::<_, Option<i64>>(7).map(TimestampMs::from_millis),
        created_at: TimestampMs::from_millis(row.get(8)),
        updated_at: TimestampMs::from_millis(row.get(9)),
    })
}

pub(crate) fn decode_runtime_binding_row(row: &Row) -> Result<RuntimeBinding, DennettError> {
    let raw_profile: String = row.get(3);
    let auth_profile = decode_auth_profile(&raw_profile)?;
    Ok(RuntimeBinding {
        binding_id: RuntimeBindingId::from_uuid(row.get(0)),
        session_id: SessionId::from_uuid(row.get(1)),
        adapter_id: row.get(2),
        auth_profile,
        tier: row.get(4),
        agy_version: row.get(5),
        provider_session_id: row.get(6),
        workspace_cwd: row.get(7),
        is_simulated: row.get(8),
        rehydrate_count: row.get(9),
        connection_id: row
            .get::<_, Option<Uuid>>(12)
            .map(ProviderConnectionId::from_uuid),
        endpoint_fingerprint: row.get(13),
        created_at: TimestampMs::from_millis(row.get(10)),
        updated_at: TimestampMs::from_millis(row.get(11)),
    })
}

pub(crate) fn decode_auth_profile(raw: &str) -> Result<RuntimeAuthProfile, DennettError> {
    match raw {
        "oauth-personal" => Ok(RuntimeAuthProfile::OAuthPersonal),
        "gemini-api-key" => Ok(RuntimeAuthProfile::GeminiApiKey),
        "local-endpoint" => Ok(RuntimeAuthProfile::LocalEndpoint),
        other => Err(DennettError::Internal {
            detail: format!("unknown runtime auth_profile in DB: `{other}`"),
        }),
    }
}

pub(crate) fn decode_message_row(row: &Row) -> Result<MessageRecord, DennettError> {
    Ok(MessageRecord {
        message_id: MessageId::from_uuid(row.get(0)),
        session_id: SessionId::from_uuid(row.get(1)),
        role: MessageRole::from_str(&row.get::<_, String>(2))?,
        content: row.get(3),
        run_id: row.get::<_, Option<Uuid>>(4).map(RunId::from_uuid),
        client_message_id: row.get(5),
        correlation_id: CorrelationId::try_new(row.get::<_, String>(6))?,
        created_at: TimestampMs::from_millis(row.get(7)),
    })
}

pub(crate) fn decode_run_row(row: &Row) -> Result<RunRecord, DennettError> {
    Ok(RunRecord {
        run_id: RunId::from_uuid(row.get(0)),
        session_id: SessionId::from_uuid(row.get(1)),
        agent_id: AgentId::from_uuid(row.get(2)),
        agent_config_version_id: AgentConfigVersionId::from_uuid(row.get(3)),
        user_message_id: MessageId::from_uuid(row.get(4)),
        assistant_message_id: row.get::<_, Option<Uuid>>(5).map(MessageId::from_uuid),
        status: RunStatus::from_str(&row.get::<_, String>(6))?,
        stop_reason: row.get(7),
        error_code: row.get(8),
        error_summary: row.get(9),
        rehydrated: row.get(10),
        task_id: row.get::<_, Option<Uuid>>(11).map(TaskId::from_uuid),
        task_generation: row.get(12),
        tokens_used: row
            .get::<_, Option<i64>>(13)
            .and_then(|v| u64::try_from(v).ok()),
        wall_ms: row
            .get::<_, Option<i64>>(14)
            .and_then(|v| u64::try_from(v).ok()),
        context_used_tokens: row
            .get::<_, Option<i64>>(18)
            .and_then(|v| u64::try_from(v).ok()),
        context_window_tokens: row
            .get::<_, Option<i64>>(19)
            .and_then(|v| u64::try_from(v).ok()),
        correlation_id: CorrelationId::try_new(row.get::<_, String>(15))?,
        created_at: TimestampMs::from_millis(row.get(16)),
        finished_at: row.get::<_, Option<i64>>(17).map(TimestampMs::from_millis),
    })
}

pub(crate) fn decode_session_followup_row(
    row: &Row,
) -> Result<SessionFollowupRecord, DennettError> {
    Ok(SessionFollowupRecord {
        followup_id: FollowupId::from_uuid(row.get(0)),
        session_id: SessionId::from_uuid(row.get(1)),
        content: row.get(2),
        status: FollowupStatus::from_str(&row.get::<_, String>(3))?,
        position: row.get(4),
        idempotency_key: row.get(5),
        dispatched_run_id: row.get::<_, Option<Uuid>>(6).map(RunId::from_uuid),
        created_at: TimestampMs::from_millis(row.get(7)),
        updated_at: TimestampMs::from_millis(row.get(8)),
    })
}

pub(crate) fn decode_artifact_row(row: &Row) -> Result<ArtifactRecord, DennettError> {
    Ok(ArtifactRecord {
        artifact_id: ArtifactId::from_uuid(row.get(0)),
        session_id: SessionId::from_uuid(row.get(1)),
        project_id: row.get::<_, Option<Uuid>>(2).map(ProjectId::from_uuid),
        run_id: row.get::<_, Option<Uuid>>(3).map(RunId::from_uuid),
        name: row.get(4),
        mime_type: row.get(5),
        revision: row.get(6),
        content: row.get(7),
        relative_path: row.get(8),
        sha256: Sha256Hex::try_new(row.get::<_, String>(9))?,
        completeness_status: ArtifactCompleteness::from_str(&row.get::<_, String>(10))?,
        lifecycle_state: ArtifactLifecycleState::from_str(&row.get::<_, String>(11))?,
        created_at: TimestampMs::from_millis(row.get(12)),
    })
}

pub(crate) fn decode_policy_rule_row(row: &Row) -> Result<PolicyRule, DennettError> {
    Ok(PolicyRule {
        rule_id: PolicyRuleId::from_uuid(row.get(0)),
        scope: row.get(1),
        tool_name: row.get(2),
        target_pattern: row.get(3),
        effect: PolicyEffect::from_str(&row.get::<_, String>(4))?,
        reason: row.get(5),
        created_at: TimestampMs::from_millis(row.get(6)),
    })
}

pub(crate) fn decode_permission_decision_row(
    row: &Row,
) -> Result<PermissionDecisionRecord, DennettError> {
    Ok(PermissionDecisionRecord {
        decision_id: PermissionDecisionId::from_uuid(row.get(0)),
        session_id: SessionId::from_uuid(row.get(1)),
        run_id: row.get::<_, Option<Uuid>>(2).map(RunId::from_uuid),
        tool_call_id: row.get(3),
        tool_name: row.get(4),
        target_pattern: row.get(5),
        scope: row.get(6),
        effect: PolicyEffect::from_str(&row.get::<_, String>(7))?,
        matched_rule_id: row.get::<_, Option<Uuid>>(8).map(PolicyRuleId::from_uuid),
        reason: row.get(9),
        evaluated_outside_model: row.get(10),
        correlation_id: CorrelationId::try_new(row.get::<_, String>(11))?,
        created_at: TimestampMs::from_millis(row.get(12)),
    })
}

pub(crate) fn decode_audit_event_row(row: &Row) -> Result<DiagnosticEvent, DennettError> {
    Ok(DiagnosticEvent {
        event_id: AuditEventId::from_uuid(row.get(0)),
        correlation_id: CorrelationId::try_new(row.get::<_, String>(1))?,
        session_id: row.get::<_, Option<Uuid>>(2).map(SessionId::from_uuid),
        run_id: row.get::<_, Option<Uuid>>(3).map(RunId::from_uuid),
        component: row.get(4),
        event_kind: row.get(5),
        safe_summary: row.get(6),
        created_at: TimestampMs::from_millis(row.get(7)),
    })
}

pub(crate) fn decode_task_row(row: &Row) -> Result<TaskRecord, DennettError> {
    let schedule: Option<TaskSchedule> = row
        .get::<_, Option<JsonValue>>(14)
        .map(|val| {
            serde_json::from_value(val).map_err(|err| DennettError::Internal {
                detail: format!("corrupt tasks.schedule_json: {err}"),
            })
        })
        .transpose()?;
    let budget: TaskBudget =
        serde_json::from_value(row.get::<_, JsonValue>(15)).map_err(|err| {
            DennettError::Internal {
                detail: format!("corrupt tasks.budget_json: {err}"),
            }
        })?;
    let turns_used = u32::try_from(row.get::<_, i32>(16)).unwrap_or(0);
    let tokens_used = u64::try_from(row.get::<_, i64>(17)).unwrap_or(0);
    let wall_ms_used = u64::try_from(row.get::<_, i64>(18)).unwrap_or(0);
    let confidence = UsageConfidence::from_str(&row.get::<_, String>(19))?;
    let attached_skills: Vec<String> = serde_json::from_value(row.get::<_, JsonValue>(20))
        .map_err(|err| DennettError::Internal {
            detail: format!("corrupt tasks.attached_skills: {err}"),
        })?;
    let auth_profile_override = row
        .get::<_, Option<String>>(21)
        .map(|s| decode_auth_profile(&s))
        .transpose()?;
    let thinking_override = row
        .get::<_, Option<String>>(29)
        .map(|s| ThinkingLevel::from_str(&s))
        .transpose()?;
    let skill_slugs_override: Option<Vec<String>> = row
        .get::<_, Option<String>>(31)
        .map(|raw| {
            serde_json::from_str(&raw).map_err(|err| DennettError::Internal {
                detail: format!("corrupt tasks.skill_slugs_override_json: {err}"),
            })
        })
        .transpose()?;
    let trigger: Option<TaskTrigger> = row
        .get::<_, Option<String>>(32)
        .map(|raw| {
            serde_json::from_str(&raw).map_err(|err| DennettError::Internal {
                detail: format!("corrupt tasks.trigger_json: {err}"),
            })
        })
        .transpose()?;

    Ok(TaskRecord {
        task_id: TaskId::from_uuid(row.get(0)),
        project_id: ProjectId::from_uuid(row.get(1)),
        session_id: SessionId::from_uuid(row.get(2)),
        agent_id: AgentId::from_uuid(row.get(3)),
        parent_task_id: row.get::<_, Option<Uuid>>(4).map(TaskId::from_uuid),
        title: row.get(5),
        goal: row.get(6),
        generation: row.get(7),
        status: TaskStatus::from_str(&row.get::<_, String>(8))?,
        wait_detail: row.get(9),
        expected_signal_name: row.get(25),
        partial_result_summary: row.get(10),
        error_code: row.get(11),
        error_summary: row.get(12),
        active_run_id: row.get::<_, Option<Uuid>>(13).map(RunId::from_uuid),
        schedule,
        budget,
        usage: TaskUsage {
            turns_used,
            tokens_used,
            wall_ms_used,
            confidence,
        },
        attached_skills,
        auth_profile_override,
        target_session_id: row.get::<_, Option<Uuid>>(26).map(SessionId::from_uuid),
        notify_session_id: row.get::<_, Option<Uuid>>(27).map(SessionId::from_uuid),
        model_override: row.get(28),
        thinking_override,
        connection_override_id: row
            .get::<_, Option<Uuid>>(30)
            .map(ProviderConnectionId::from_uuid),
        skill_slugs_override,
        trigger,
        created_at: TimestampMs::from_millis(row.get(22)),
        updated_at: TimestampMs::from_millis(row.get(23)),
        finished_at: row.get::<_, Option<i64>>(24).map(TimestampMs::from_millis),
    })
}

pub(crate) fn decode_task_timer_row(row: &Row) -> Result<TaskTimerRecord, DennettError> {
    let pause_reason = row
        .get::<_, Option<String>>(15)
        .map(|raw| TimerPauseReason::from_str(&raw))
        .transpose()?;
    Ok(TaskTimerRecord {
        timer_id: TaskTimerId::from_uuid(row.get(0)),
        task_id: TaskId::from_uuid(row.get(1)),
        task_generation: row.get(2),
        idempotency_key: IdempotencyKey::try_new(row.get::<_, String>(3))?,
        fire_at: TimestampMs::from_millis(row.get(4)),
        timezone: row.get(5),
        original_expression: row.get(6),
        recurrence_kind: row.get(7),
        missed_run_policy: MissedRunPolicy::from_str(&row.get::<_, String>(8))?,
        status: TaskTimerStatus::from_str(&row.get::<_, String>(9))?,
        pause_reason,
        lease_owner: row.get(10),
        lease_generation: row.get(11),
        lease_expires_at: row.get::<_, Option<i64>>(12).map(TimestampMs::from_millis),
        created_at: TimestampMs::from_millis(row.get(13)),
        fired_at: row.get::<_, Option<i64>>(14).map(TimestampMs::from_millis),
    })
}

pub(crate) fn decode_task_signal_row(row: &Row) -> Result<TaskSignalRecord, DennettError> {
    Ok(TaskSignalRecord {
        signal_id: TaskSignalId::from_uuid(row.get(0)),
        task_id: TaskId::from_uuid(row.get(1)),
        idempotency_key: IdempotencyKey::try_new(row.get::<_, String>(2))?,
        signal_name: row.get(3),
        payload_text: row.get(4),
        payload_sha256: Sha256Hex::try_new(row.get::<_, String>(5))?,
        applied_generation: row.get(6),
        outcome_status: TaskSignalOutcome::from_str(&row.get::<_, String>(7))?,
        correlation_id: CorrelationId::try_new(row.get::<_, String>(8))?,
        created_at: TimestampMs::from_millis(row.get(9)),
    })
}

pub(crate) fn decode_capability_grant_row(
    row: &Row,
) -> Result<CapabilityGrantRecord, DennettError> {
    let target_prefixes: Vec<String> =
        serde_json::from_value(row.get::<_, JsonValue>(7)).map_err(|err| {
            DennettError::Internal {
                detail: format!("corrupt capability_grants.target_prefixes: {err}"),
            }
        })?;
    Ok(CapabilityGrantRecord {
        grant_id: CapabilityGrantId::from_uuid(row.get(0)),
        scope: row.get(1),
        project_id: ProjectId::from_uuid(row.get(2)),
        agent_id: row.get::<_, Option<Uuid>>(3).map(AgentId::from_uuid),
        session_id: row.get::<_, Option<Uuid>>(4).map(SessionId::from_uuid),
        task_id: row.get::<_, Option<Uuid>>(5).map(TaskId::from_uuid),
        tool_pattern: row.get(6),
        target_prefixes,
        max_uses: row.get(8),
        uses_consumed: row.get(9),
        reason: row.get(10),
        expires_at: row.get::<_, Option<i64>>(11).map(TimestampMs::from_millis),
        revoked_at: row.get::<_, Option<i64>>(12).map(TimestampMs::from_millis),
        revoked_reason: row.get(13),
        created_at: TimestampMs::from_millis(row.get(14)),
    })
}

pub(crate) fn decode_permission_request_row(
    row: &Row,
) -> Result<PermissionRequestRecord, DennettError> {
    let normalized_targets: Vec<String> = serde_json::from_value(row.get::<_, JsonValue>(10))
        .map_err(|err| DennettError::Internal {
            detail: format!("corrupt permission_requests.normalized_targets: {err}"),
        })?;
    Ok(PermissionRequestRecord {
        request_id: PermissionRequestId::from_uuid(row.get(0)),
        project_id: ProjectId::from_uuid(row.get(1)),
        session_id: SessionId::from_uuid(row.get(2)),
        run_id: RunId::from_uuid(row.get(3)),
        task_id: row.get::<_, Option<Uuid>>(4).map(TaskId::from_uuid),
        agent_id: AgentId::from_uuid(row.get(5)),
        tool_call_id: row.get(6),
        tool_name: row.get(7),
        tool_kind: row.get(8),
        target_summary: row.get(9),
        normalized_targets,
        parameters_sha256: Sha256Hex::try_new(row.get::<_, String>(11))?,
        preview_snippet: row.get(21),
        reason: row.get(12),
        status: PermissionRequestStatus::from_str(&row.get::<_, String>(13))?,
        issued_grant_id: row
            .get::<_, Option<Uuid>>(14)
            .map(CapabilityGrantId::from_uuid),
        decision_id: row
            .get::<_, Option<Uuid>>(15)
            .map(PermissionDecisionId::from_uuid),
        revision: row.get(16),
        expires_at: TimestampMs::from_millis(row.get(17)),
        resolved_at: row.get::<_, Option<i64>>(18).map(TimestampMs::from_millis),
        correlation_id: CorrelationId::try_new(row.get::<_, String>(19))?,
        created_at: TimestampMs::from_millis(row.get(20)),
    })
}

pub(crate) fn decode_effect_claim_row(row: &Row) -> Result<EffectClaimRecord, DennettError> {
    let params_val: JsonValue = row.get(10);
    let receipt_val: Option<JsonValue> = row.get(14);
    Ok(EffectClaimRecord {
        effect_id: EffectClaimId::from_uuid(row.get(0)),
        project_id: ProjectId::from_uuid(row.get(1)),
        session_id: SessionId::from_uuid(row.get(2)),
        task_id: row.get::<_, Option<Uuid>>(3).map(TaskId::from_uuid),
        run_id: row.get::<_, Option<Uuid>>(4).map(RunId::from_uuid),
        agent_id: AgentId::from_uuid(row.get(5)),
        effect_idempotency_key: IdempotencyKey::try_new(row.get::<_, String>(6))?,
        connector_id: row.get(7),
        operation: row.get(8),
        target_recipient: row.get(9),
        parameters_json: params_val.to_string(),
        parameters_sha256: Sha256Hex::try_new(row.get::<_, String>(11))?,
        status: ExternalEffectStatus::from_str(&row.get::<_, String>(12))?,
        provider_operation_id: row.get(13),
        receipt_json: receipt_val.map(|v| v.to_string()),
        error_summary: row.get(15),
        reconciled_at: row.get::<_, Option<i64>>(16).map(TimestampMs::from_millis),
        correlation_id: CorrelationId::try_new(row.get::<_, String>(17))?,
        created_at: TimestampMs::from_millis(row.get(18)),
        updated_at: TimestampMs::from_millis(row.get(19)),
    })
}

pub(crate) fn decode_inbox_card_row(row: &Row) -> Result<InboxCardRecord, DennettError> {
    Ok(InboxCardRecord {
        card_id: InboxCardId::from_uuid(row.get(0)),
        project_id: ProjectId::from_uuid(row.get(1)),
        session_id: row.get::<_, Option<Uuid>>(2).map(SessionId::from_uuid),
        task_id: row.get::<_, Option<Uuid>>(3).map(TaskId::from_uuid),
        run_id: row.get::<_, Option<Uuid>>(4).map(RunId::from_uuid),
        agent_id: row.get::<_, Option<Uuid>>(5).map(AgentId::from_uuid),
        kind: InboxCardKind::from_str(&row.get::<_, String>(6))?,
        status: InboxCardStatus::from_str(&row.get::<_, String>(7))?,
        urgency: InboxCardUrgency::from_str(&row.get::<_, String>(8))?,
        subject: row.get(9),
        detail: row.get(10),
        permission_request_id: row
            .get::<_, Option<Uuid>>(11)
            .map(PermissionRequestId::from_uuid),
        effect_id: row.get::<_, Option<Uuid>>(12).map(EffectClaimId::from_uuid),
        resolution_summary: row.get(13),
        revision: row.get(14),
        created_at: TimestampMs::from_millis(row.get(15)),
        updated_at: TimestampMs::from_millis(row.get(16)),
        resolved_at: row.get::<_, Option<i64>>(17).map(TimestampMs::from_millis),
    })
}

pub(crate) fn decode_agent_message_row(row: &Row) -> Result<AgentMessageRecord, DennettError> {
    Ok(AgentMessageRecord {
        agent_message_id: AgentMessageId::from_uuid(row.get(0)),
        sender_agent_id: row.get::<_, Option<Uuid>>(1).map(AgentId::from_uuid),
        recipient_agent_id: AgentId::from_uuid(row.get(2)),
        project_id: ProjectId::from_uuid(row.get(3)),
        related_task_id: row.get::<_, Option<Uuid>>(4).map(TaskId::from_uuid),
        idempotency_key: IdempotencyKey::try_new(row.get::<_, String>(5))?,
        body_text: row.get(6),
        delivered_in_run_id: row.get::<_, Option<Uuid>>(7).map(RunId::from_uuid),
        correlation_id: CorrelationId::try_new(row.get::<_, String>(8))?,
        created_at: TimestampMs::from_millis(row.get(9)),
        delivered_at: row.get::<_, Option<i64>>(10).map(TimestampMs::from_millis),
    })
}

pub(crate) fn decode_control_event_row(row: &Row) -> Result<ControlEventRecord, DennettError> {
    let payload: JsonValue = row.get(10);
    Ok(ControlEventRecord {
        cursor_seq: row.get(0),
        event_id: ControlEventId::from_uuid(row.get(1)),
        authority_epoch: row.get(2),
        entity_key: row.get(3),
        entity_seq: row.get(4),
        event_type: row.get(5),
        project_id: row.get::<_, Option<Uuid>>(6).map(ProjectId::from_uuid),
        session_id: row.get::<_, Option<Uuid>>(7).map(SessionId::from_uuid),
        task_id: row.get::<_, Option<Uuid>>(8).map(TaskId::from_uuid),
        correlation_id: CorrelationId::try_new(row.get::<_, String>(9))?,
        payload_json: payload.to_string(),
        created_at: TimestampMs::from_millis(row.get(11)),
    })
}

pub(crate) fn decode_ordered_outbox_row(row: &Row) -> Result<OrderedOutboxItem, DennettError> {
    let payload: JsonValue = row.get(6);
    Ok(OrderedOutboxItem {
        item_id: OrderedOutboxItemId::from_uuid(row.get(0)),
        destination: row.get(1),
        ordering_key: row.get(2),
        key_seq: row.get(3),
        cursor_seq: row.get(4),
        idempotency_key: IdempotencyKey::try_new(row.get::<_, String>(5))?,
        payload_json: payload.to_string(),
        status: OrderedOutboxStatus::from_str(&row.get::<_, String>(7))?,
        attempts: row.get(8),
        available_at: TimestampMs::from_millis(row.get(9)),
        lease_owner: row.get(10),
        lease_generation: row.get(11),
        lease_expires_at: row.get::<_, Option<i64>>(12).map(TimestampMs::from_millis),
        last_error: row.get(13),
        correlation_id: CorrelationId::try_new(row.get::<_, String>(14))?,
        created_at: TimestampMs::from_millis(row.get(15)),
        updated_at: TimestampMs::from_millis(row.get(16)),
    })
}

pub(crate) async fn fetch_user_preferences<C: GenericClient>(
    client: &C,
    target: &ValidatedControlPgTarget,
) -> Result<UserPreferences, DennettError> {
    let row = client
        .query_opt(
            "SELECT ui_language, agent_language, timezone, enter_to_send,
                    TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint
             FROM user_preferences
             WHERE id = 1",
            &[],
        )
        .await
        .map_err(|err| map_pg_err(target, &err))?;
    match row {
        Some(r) => Ok(UserPreferences {
            ui_language: r.get(0),
            agent_language: r.get(1),
            timezone: r.get(2),
            enter_to_send: r.get(3),
            updated_at: TimestampMs::from_millis(r.get(4)),
        }),
        None => Ok(UserPreferences::default()),
    }
}

pub(crate) async fn update_user_preferences_in_tx<C: GenericClient>(
    tx: &C,
    req: &UpdateUserPreferencesRequest,
    target: &ValidatedControlPgTarget,
) -> Result<UserPreferences, DennettError> {
    if let Some(ref tz) = req.timezone {
        let exists: bool = tx
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM pg_timezone_names WHERE name = $1)",
                &[&tz.as_str()],
            )
            .await
            .map_err(|err| map_pg_err(target, &err))?
            .get(0);
        if !exists {
            return Err(DennettError::InvalidInput {
                field: String::from("timezone"),
                reason: format!("unknown IANA timezone `{tz}` in PostgreSQL pg_timezone_names"),
            });
        }
    }

    tx.execute(
        "INSERT INTO user_preferences (id) VALUES (1) ON CONFLICT (id) DO NOTHING",
        &[],
    )
    .await
    .map_err(|err| map_pg_err(target, &err))?;

    let row = tx
        .query_one(
            "UPDATE user_preferences
             SET ui_language = COALESCE($1, ui_language),
                 agent_language = COALESCE($2, agent_language),
                 timezone = COALESCE($3, timezone),
                 enter_to_send = COALESCE($4, enter_to_send),
                 updated_at = now()
             WHERE id = 1
             RETURNING ui_language, agent_language, timezone, enter_to_send,
                       TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint",
            &[
                &req.ui_language.as_deref(),
                &req.agent_language.as_deref(),
                &req.timezone.as_deref(),
                &req.enter_to_send,
            ],
        )
        .await
        .map_err(|err| map_pg_err(target, &err))?;

    Ok(UserPreferences {
        ui_language: row.get(0),
        agent_language: row.get(1),
        timezone: row.get(2),
        enter_to_send: row.get(3),
        updated_at: TimestampMs::from_millis(row.get(4)),
    })
}

pub(crate) fn truncate_utf8(input: &str, max_bytes: usize) -> &str {
    if input.len() <= max_bytes {
        return input;
    }
    let mut cut = max_bytes;
    while cut > 0 && !input.is_char_boundary(cut) {
        cut -= 1;
    }
    &input[..cut]
}
