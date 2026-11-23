//! PostgreSQL control-plane persistence helpers for `dennett-head`.
//!
//! Uses `control_stream_counters` for monotonic message and audit ordering (Architecture 81 §8.1),
//! never holds SQL transactions across network/runtime calls, and maps all PostgreSQL errors
//! through secret-redacting sanitization.

use std::collections::{BTreeSet, HashMap};
use std::str::FromStr;

use deadpool_postgres::GenericClient;
use dennett_contracts::{
    AgentConfigVersion, AgentConfigVersionId, AgentId, AgentRecord, ArtifactId, ArtifactRecord,
    AuditEventId, CorrelationId, DennettError, DiagnosticEvent, IdempotencyKey, MemoryEventId,
    MemoryNoteId, MemoryOutboxItem, MemoryOutboxStatus, MemoryScope, MessageId, MessageRecord,
    MessageRole, OutboxItemId, PermissionDecisionId, PermissionDecisionRecord, PolicyEffect,
    PolicyRule, PolicyRuleId, ProjectId, ProjectRecord, RunId, RunRecord, RunStatus,
    RuntimeAuthProfile, RuntimeBinding, RuntimeBindingId, SessionId, SessionRecord, SessionStatus,
    Sha256Hex, ThinkingLevel, TimestampMs, WorkspaceWritePolicy,
};
use serde_json::Value as JsonValue;
use tokio_postgres::Row;
use uuid::Uuid;

use crate::pg_target::ValidatedControlPgTarget;

pub(crate) fn map_pg_err(
    target: &ValidatedControlPgTarget,
    err: &tokio_postgres::Error,
) -> DennettError {
    DennettError::DatabaseUnavailable {
        detail: target.sanitize_error(&err.to_string()),
    }
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

pub(crate) async fn fetch_agent<C: GenericClient>(
    client: &C,
    agent_id: AgentId,
    target: &ValidatedControlPgTarget,
) -> Result<AgentRecord, DennettError> {
    let row = client
        .query_opt(
            "SELECT a.agent_id, a.name, a.description, a.current_config_version_id,
                    TRUNC(EXTRACT(EPOCH FROM a.created_at) * 1000)::bigint,
                    TRUNC(EXTRACT(EPOCH FROM a.updated_at) * 1000)::bigint,
                    v.config_version_id, v.version_number, v.system_prompt,
                    v.model_preference, v.thinking_level,
                    v.allowed_memory_scopes, v.workspace_write_policy,
                    TRUNC(EXTRACT(EPOCH FROM v.created_at) * 1000)::bigint
             FROM agents a
             JOIN agent_config_versions v ON v.config_version_id = a.current_config_version_id
             WHERE a.agent_id = $1",
            &[&agent_id.as_uuid()],
        )
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
    let row = client
        .query_opt(
            "SELECT project_id, name, workspace_path, memory_scope,
                    TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint
             FROM projects
             WHERE project_id = $1",
            &[&project_id.as_uuid()],
        )
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

pub(crate) async fn fetch_session<C: GenericClient>(
    client: &C,
    session_id: SessionId,
    target: &ValidatedControlPgTarget,
) -> Result<SessionRecord, DennettError> {
    let row = client
        .query_opt(
            "SELECT session_id, agent_id, project_id, title, status,
                    TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
                    TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint
             FROM sessions
             WHERE session_id = $1",
            &[&session_id.as_uuid()],
        )
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
                    TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint
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
                    TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint
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
            "SELECT message_id, session_id, role, content, run_id, correlation_id,
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
    let rows = client
        .query(
            "SELECT run_id, session_id, agent_id, agent_config_version_id,
                    user_message_id, assistant_message_id, status, stop_reason,
                    error_code, error_summary, rehydrated, correlation_id,
                    TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
                    TRUNC(EXTRACT(EPOCH FROM finished_at) * 1000)::bigint
             FROM runs
             WHERE session_id = $1
             ORDER BY created_at ASC",
            &[&session_id.as_uuid()],
        )
        .await
        .map_err(|err| map_pg_err(target, &err))?;

    rows.iter().map(decode_run_row).collect()
}

pub(crate) async fn fetch_session_artifacts<C: GenericClient>(
    client: &C,
    session_id: SessionId,
    target: &ValidatedControlPgTarget,
) -> Result<Vec<ArtifactRecord>, DennettError> {
    let rows = client
        .query(
            "SELECT artifact_id, session_id, project_id, run_id, name, mime_type,
                    revision, content, relative_path, sha256,
                    TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint
             FROM artifacts
             WHERE session_id = $1
             ORDER BY created_at ASC",
            &[&session_id.as_uuid()],
        )
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
        created_at: TimestampMs::from_millis(row.get(13)),
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
        created_at: TimestampMs::from_millis(row.get(8)),
    })
}

pub(crate) fn decode_project_row(row: &Row) -> Result<ProjectRecord, DennettError> {
    Ok(ProjectRecord {
        project_id: ProjectId::from_uuid(row.get(0)),
        name: row.get(1),
        workspace_path: row.get(2),
        memory_scope: MemoryScope::try_new(row.get::<_, String>(3))?,
        created_at: TimestampMs::from_millis(row.get(4)),
    })
}

pub(crate) fn decode_session_row(
    row: &Row,
    runtime_binding: Option<RuntimeBinding>,
) -> Result<SessionRecord, DennettError> {
    Ok(SessionRecord {
        session_id: SessionId::from_uuid(row.get(0)),
        agent_id: AgentId::from_uuid(row.get(1)),
        project_id: ProjectId::from_uuid(row.get(2)),
        title: row.get(3),
        status: SessionStatus::from_str(&row.get::<_, String>(4))?,
        runtime_binding,
        created_at: TimestampMs::from_millis(row.get(5)),
        updated_at: TimestampMs::from_millis(row.get(6)),
    })
}

pub(crate) fn decode_runtime_binding_row(row: &Row) -> Result<RuntimeBinding, DennettError> {
    let raw_profile: String = row.get(3);
    let auth_profile = match raw_profile.as_str() {
        "oauth-personal" => RuntimeAuthProfile::OAuthPersonal,
        "gemini-api-key" => RuntimeAuthProfile::GeminiApiKey,
        other => {
            return Err(DennettError::Internal {
                detail: format!("unknown runtime auth_profile in DB: `{other}`"),
            });
        }
    };
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
        created_at: TimestampMs::from_millis(row.get(10)),
        updated_at: TimestampMs::from_millis(row.get(11)),
    })
}

pub(crate) fn decode_message_row(row: &Row) -> Result<MessageRecord, DennettError> {
    Ok(MessageRecord {
        message_id: MessageId::from_uuid(row.get(0)),
        session_id: SessionId::from_uuid(row.get(1)),
        role: MessageRole::from_str(&row.get::<_, String>(2))?,
        content: row.get(3),
        run_id: row.get::<_, Option<Uuid>>(4).map(RunId::from_uuid),
        correlation_id: CorrelationId::try_new(row.get::<_, String>(5))?,
        created_at: TimestampMs::from_millis(row.get(6)),
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
        correlation_id: CorrelationId::try_new(row.get::<_, String>(11))?,
        created_at: TimestampMs::from_millis(row.get(12)),
        finished_at: row.get::<_, Option<i64>>(13).map(TimestampMs::from_millis),
    })
}

pub(crate) fn decode_artifact_row(row: &Row) -> Result<ArtifactRecord, DennettError> {
    Ok(ArtifactRecord {
        artifact_id: ArtifactId::from_uuid(row.get(0)),
        session_id: SessionId::from_uuid(row.get(1)),
        project_id: ProjectId::from_uuid(row.get(2)),
        run_id: row.get::<_, Option<Uuid>>(3).map(RunId::from_uuid),
        name: row.get(4),
        mime_type: row.get(5),
        revision: row.get(6),
        content: row.get(7),
        relative_path: row.get(8),
        sha256: Sha256Hex::try_new(row.get::<_, String>(9))?,
        created_at: TimestampMs::from_millis(row.get(10)),
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

pub(crate) fn decode_outbox_row(row: &Row) -> Result<MemoryOutboxItem, DennettError> {
    Ok(MemoryOutboxItem {
        outbox_id: OutboxItemId::from_uuid(row.get(0)),
        session_id: row.get::<_, Option<Uuid>>(1).map(SessionId::from_uuid),
        run_id: row.get::<_, Option<Uuid>>(2).map(RunId::from_uuid),
        scope: MemoryScope::try_new(row.get::<_, String>(3))?,
        idempotency_key: IdempotencyKey::try_new(row.get::<_, String>(4))?,
        operation: row.get(5),
        payload_json: row.get(6),
        status: MemoryOutboxStatus::from_str(&row.get::<_, String>(7))?,
        attempts: row.get(8),
        last_error: row.get(9),
        delivered_event_id: row.get::<_, Option<Uuid>>(10).map(MemoryEventId::from_uuid),
        delivered_note_id: row.get::<_, Option<Uuid>>(11).map(MemoryNoteId::from_uuid),
        correlation_id: CorrelationId::try_new(row.get::<_, String>(12))?,
        created_at: TimestampMs::from_millis(row.get(13)),
        updated_at: TimestampMs::from_millis(row.get(14)),
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
