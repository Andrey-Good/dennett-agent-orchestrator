//! Ordered delivery journal (`control_events`), per-key ordered outbox (`ordered_outbox` +
//! `ordered_outbox_heads`), and consistent `REPEATABLE READ` snapshot + cursor subscription
//! (`V15`, `V16`, `V17`, `V18`).
//!
//! Concurrency & visibility invariants:
//! - `cursor_seq` is NEVER allocated via `BIGSERIAL` / `MAX(id)`. It is allocated strictly at the
//!   end of the write transaction via `control_stream_counters('control:journal')`, following the
//!   deadlock-free lock hierarchy `domain row -> entity:<entity_key> -> control:journal`.
//! - `ordered_outbox` workers claim strictly via `JOIN ordered_outbox_heads ... FOR UPDATE OF h, o SKIP LOCKED`
//!   so two concurrent workers NEVER overtake each other on the same `ordering_key`, while
//!   independent keys progress in parallel and stale leases are fenced by `lease_generation`.
//! - `GET /v1/state/snapshot` executes inside a single `ISOLATION LEVEL REPEATABLE READ READ ONLY`
//!   transaction so `snapshot_cursor` matches every returned entity collection without holes or dupes.

use deadpool_postgres::GenericClient;
use dennett_contracts::{
    ControlEventId, ControlEventRecord, CorrelationId, DennettError, EventStreamResponse,
    IdempotencyKey, OrderedOutboxItem, OrderedOutboxItemId, ProfileMode, ProjectId, SessionId,
    StateSnapshotResponse, StreamCursor, TaskId, TimestampMs,
};
use serde_json::Value as JsonValue;
use tokio_postgres::IsolationLevel;

use crate::pg_target::ValidatedControlPgTarget;
use crate::service::{DEFAULT_PG_OP_TIMEOUT, HeadService};
use crate::store::{
    AGENT_JOIN_SELECT, CAPABILITY_GRANT_SELECT_COLS, CONTROL_EVENT_SELECT_COLS,
    INBOX_CARD_SELECT_COLS, ORDERED_OUTBOX_SELECT_COLS, PROJECT_SELECT_COLS, SESSION_SELECT_COLS,
    TASK_SELECT_COLS, decode_agent_join_row, decode_capability_grant_row, decode_control_event_row,
    decode_inbox_card_row, decode_ordered_outbox_row, decode_project_row, decode_session_row,
    decode_task_row, fetch_all_runtime_bindings, map_pg_err, ms_to_pg_epoch_secs,
    next_global_journal_cursor, next_stream_seq, truncate_utf8,
};

/// Default page limit for `GET /v1/events`.
pub const DEFAULT_EVENTS_LIMIT: u32 = 100;
/// Maximum page limit for `GET /v1/events`.
pub const MAX_EVENTS_LIMIT: u32 = 500;

/// Parameters for appending a journaled [`ControlEventRecord`] inside an active SQL transaction.
#[derive(Debug, Clone)]
pub struct ControlEventAppend<'a> {
    /// Partition key for per-entity monotonic ordering (e.g. `"task:<uuid>"`, `"project:<uuid>"`).
    pub entity_key: &'a str,
    /// Stable dotted event type name (e.g. `"task.created"`, `"permission.requested"`).
    pub event_type: &'a str,
    /// Optional owning project identifier for scoped filtering without leaking foreign payloads.
    pub project_id: Option<ProjectId>,
    /// Optional owning session identifier.
    pub session_id: Option<SessionId>,
    /// Optional owning task identifier.
    pub task_id: Option<TaskId>,
    /// Request correlation identifier.
    pub correlation_id: &'a CorrelationId,
    /// Already secret-redacted JSON event payload.
    pub payload: JsonValue,
}

/// Owned pending control-journal event buffered during a SQL transaction so ALL domain writes
/// finish before acquiring `entity:*` counters (in deterministic sorted order) and finally
/// `control:journal` right before `tx.commit().await` (V15 deadlock-free lock hierarchy).
#[derive(Debug, Clone)]
pub struct PendingControlEvent {
    /// Partition key for per-entity monotonic ordering (e.g. `"task:<uuid>"`, `"project:<uuid>"`).
    pub entity_key: String,
    /// Stable dotted event type name (e.g. `"task.created"`, `"permission.requested"`).
    pub event_type: &'static str,
    /// Optional owning project identifier for scoped filtering without leaking foreign payloads.
    pub project_id: Option<ProjectId>,
    /// Optional owning session identifier.
    pub session_id: Option<SessionId>,
    /// Optional owning task identifier.
    pub task_id: Option<TaskId>,
    /// Already secret-redacted JSON event payload.
    pub payload: JsonValue,
}

/// Appends one or more [`ControlEventRecord`]s as the **final** operation before `tx.commit().await`,
/// enforcing the V15 deadlock-free lock hierarchy:
/// 1. All domain rows & `audit_events` are already locked/written by the caller.
/// 2. All distinct `entity:<key>` rows in `control_stream_counters` are inserted-if-missing and
///    locked in strict lexicographical order BEFORE acquiring `control:journal`.
/// 3. `control_stream_counters('control:journal')` is acquired strictly last right before `COMMIT`.
///
/// # Errors
///
/// Returns [`DennettError::DatabaseUnavailable`] on PostgreSQL execution errors.
pub async fn append_control_events_batch_in_tx<C: GenericClient>(
    tx: &C,
    target: &ValidatedControlPgTarget,
    correlation_id: &CorrelationId,
    events: &[PendingControlEvent],
) -> Result<Vec<ControlEventRecord>, DennettError> {
    if events.is_empty() {
        return Ok(Vec::new());
    }

    let mut unique_streams: Vec<String> = events
        .iter()
        .map(|ev| format!("entity:{}", truncate_utf8(&ev.entity_key, 160)))
        .collect();
    unique_streams.sort();
    unique_streams.dedup();

    for stream_id in &unique_streams {
        tx.execute(
            "INSERT INTO control_stream_counters (stream_id, committed_seq, authority_epoch, retained_from_seq)
             VALUES ($1, 0, 1, 1)
             ON CONFLICT (stream_id) DO NOTHING",
            &[stream_id],
        )
        .await
        .map_err(|err| map_pg_err(target, &err))?;
    }

    // Lock all `entity:*` counters in strict sorted order before touching `control:journal`.
    let _ = tx
        .query(
            "SELECT stream_id
             FROM control_stream_counters
             WHERE stream_id = ANY($1)
             ORDER BY stream_id ASC
             FOR UPDATE",
            &[&unique_streams],
        )
        .await
        .map_err(|err| map_pg_err(target, &err))?;

    let mut records = Vec::with_capacity(events.len());
    for ev in events {
        let rec = append_control_event_in_tx(
            tx,
            target,
            ControlEventAppend {
                entity_key: &ev.entity_key,
                event_type: ev.event_type,
                project_id: ev.project_id,
                session_id: ev.session_id,
                task_id: ev.task_id,
                correlation_id,
                payload: ev.payload.clone(),
            },
        )
        .await?;
        records.push(rec);
    }
    Ok(records)
}

/// Appends one append-only [`ControlEventRecord`] inside an open SQL transaction following the
/// strict deadlock-free lock hierarchy (`domain row -> entity:<key> -> control:journal`).
///
/// MUST be invoked at the very end of the transaction right before `COMMIT` so the row lock on
/// `control_stream_counters('control:journal')` is held for the shortest possible window (V15).
///
/// # Errors
///
/// Returns [`DennettError::DatabaseUnavailable`] on PostgreSQL execution errors.
pub async fn append_control_event_in_tx<C: GenericClient>(
    tx: &C,
    target: &ValidatedControlPgTarget,
    params: ControlEventAppend<'_>,
) -> Result<ControlEventRecord, DennettError> {
    let entity_key = truncate_utf8(params.entity_key, 160);
    let event_type = truncate_utf8(params.event_type, 128);
    let entity_stream = format!("entity:{entity_key}");

    // Lock order step 2: per-entity counter.
    let entity_seq = next_stream_seq(tx, &entity_stream, target).await?;
    // Lock order step 3 (LAST before COMMIT): global gapless journal counter (V15).
    let (cursor_seq, authority_epoch) = next_global_journal_cursor(tx, target).await?;

    let event_id = ControlEventId::new();
    let row = tx
        .query_one(
            "INSERT INTO control_events (
                cursor_seq, event_id, authority_epoch, entity_key, entity_seq,
                event_type, project_id, session_id, task_id, correlation_id, payload_json
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             RETURNING TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint",
            &[
                &cursor_seq,
                &event_id.as_uuid(),
                &authority_epoch,
                &entity_key,
                &entity_seq,
                &event_type,
                &params.project_id.map(ProjectId::as_uuid),
                &params.session_id.map(SessionId::as_uuid),
                &params.task_id.map(TaskId::as_uuid),
                &params.correlation_id.as_str(),
                &params.payload,
            ],
        )
        .await
        .map_err(|err| map_pg_err(target, &err))?;

    Ok(ControlEventRecord {
        cursor_seq,
        event_id,
        authority_epoch,
        entity_key: entity_key.to_owned(),
        entity_seq,
        event_type: event_type.to_owned(),
        project_id: params.project_id,
        session_id: params.session_id,
        task_id: params.task_id,
        correlation_id: params.correlation_id.clone(),
        payload_json: params.payload.to_string(),
        created_at: TimestampMs::from_millis(row.get(0)),
    })
}

/// Enqueues one per-key ordered outbox item inside an open SQL transaction (V16, V17).
///
/// Ensures `ordered_outbox_heads(destination, ordering_key)` exists, allocates a strictly
/// monotonic `key_seq` via `control_stream_counters`, and inserts into `ordered_outbox`.
/// Duplicate `(destination, idempotency_key)` is idempotently returned without allocating a gap.
///
/// # Errors
///
/// Returns [`DennettError::DatabaseUnavailable`] on PostgreSQL execution errors.
#[expect(
    clippy::too_many_arguments,
    reason = "outbox insertion binds all transactional ordering columns cleanly"
)]
pub async fn enqueue_ordered_outbox_in_tx<C: GenericClient>(
    tx: &C,
    target: &ValidatedControlPgTarget,
    destination: &str,
    ordering_key: &str,
    cursor_seq: i64,
    idempotency_key: &IdempotencyKey,
    payload: &JsonValue,
    correlation_id: &CorrelationId,
) -> Result<OrderedOutboxItem, DennettError> {
    let dest = truncate_utf8(destination, 64);
    let key = truncate_utf8(ordering_key, 160);

    // Check idempotency BEFORE allocating a per-key sequence number so duplicates never create
    // a hole in `key_seq` that would stall `ordered_outbox_heads.next_deliver_seq`!
    let existing_sql = format!(
        "SELECT {ORDERED_OUTBOX_SELECT_COLS}
         FROM ordered_outbox
         WHERE destination = $1 AND ordering_key = $2 AND idempotency_key = $3"
    );
    if let Some(existing) = tx
        .query_opt(&existing_sql, &[&dest, &key, &idempotency_key.as_str()])
        .await
        .map_err(|err| map_pg_err(target, &err))?
    {
        return decode_ordered_outbox_row(&existing);
    }

    tx.execute(
        "INSERT INTO ordered_outbox_heads (destination, ordering_key, next_deliver_seq)
         VALUES ($1, $2, 1)
         ON CONFLICT (destination, ordering_key) DO NOTHING",
        &[&dest, &key],
    )
    .await
    .map_err(|err| map_pg_err(target, &err))?;

    let seq_stream = format!("outbox:{dest}:{key}");
    let key_seq = next_stream_seq(tx, &seq_stream, target).await?;
    let item_id = OrderedOutboxItemId::new();
    let available_secs = ms_to_pg_epoch_secs(TimestampMs::now().as_millis());

    let insert_sql = format!(
        "INSERT INTO ordered_outbox (
            item_id, destination, ordering_key, key_seq, cursor_seq,
            idempotency_key, payload_json, status, available_at, correlation_id
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, 'pending', to_timestamp($8::double precision), $9)
         RETURNING {ORDERED_OUTBOX_SELECT_COLS}"
    );
    let row = tx
        .query_one(
            &insert_sql,
            &[
                &item_id.as_uuid(),
                &dest,
                &key,
                &key_seq,
                &cursor_seq,
                &idempotency_key.as_str(),
                payload,
                &available_secs,
                &correlation_id.as_str(),
            ],
        )
        .await
        .map_err(|err| map_pg_err(target, &err))?;

    decode_ordered_outbox_row(&row)
}

impl HeadService {
    /// Captures a single MVCC-consistent `REPEATABLE READ READ ONLY` control-plane snapshot
    /// together with its exact gapless `snapshot_cursor` (`GET /v1/state/snapshot`, V18).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::DatabaseUnavailable`] on PostgreSQL query failures.
    #[expect(
        clippy::too_many_lines,
        reason = "reads all authoritative control-plane collections inside one REPEATABLE READ READ ONLY transaction alongside the gapless snapshot_cursor"
    )]
    pub async fn get_state_snapshot(
        &self,
        project_id: Option<ProjectId>,
        include_archived: bool,
    ) -> Result<StateSnapshotResponse, DennettError> {
        let _ = self.ensure_migrated().await?;
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .build_transaction()
            .isolation_level(IsolationLevel::RepeatableRead)
            .read_only(true)
            .start()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let cursor_row = tx
            .query_opt(
                "SELECT authority_epoch, committed_seq, retained_from_seq
                 FROM control_stream_counters
                 WHERE stream_id = 'control:journal'",
                &[],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let snapshot_cursor = match cursor_row {
            Some(row) => StreamCursor {
                authority_epoch: row.get(0),
                committed_seq: row.get(1),
                retained_from_seq: row.get(2),
            },
            None => StreamCursor {
                authority_epoch: 1,
                committed_seq: 0,
                retained_from_seq: 1,
            },
        };

        let standalone_uuid = crate::policy::STANDALONE_SYSTEM_PROJECT_UUID;
        let project_rows = if let Some(pid) = project_id {
            let sql = format!(
                "SELECT {PROJECT_SELECT_COLS} FROM projects
                 WHERE project_id = $1
                   AND project_id <> $2
                   AND lifecycle_state <> 'deleted_record'"
            );
            tx.query(&sql, &[&pid.as_uuid(), &standalone_uuid])
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?
        } else if include_archived {
            let sql = format!(
                "SELECT {PROJECT_SELECT_COLS} FROM projects
                 WHERE project_id <> $1
                   AND lifecycle_state <> 'deleted_record'
                 ORDER BY pinned DESC, pinned_at DESC NULLS LAST, updated_at DESC, created_at DESC"
            );
            tx.query(&sql, &[&standalone_uuid])
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?
        } else {
            let sql = format!(
                "SELECT {PROJECT_SELECT_COLS} FROM projects
                 WHERE project_id <> $1
                   AND lifecycle_state IN ('active', 'detached')
                 ORDER BY pinned DESC, pinned_at DESC NULLS LAST, updated_at DESC, created_at DESC"
            );
            tx.query(&sql, &[&standalone_uuid])
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?
        };
        let projects = project_rows
            .iter()
            .map(decode_project_row)
            .collect::<Result<Vec<_>, _>>()?;

        let agent_sql = format!("{AGENT_JOIN_SELECT} ORDER BY a.created_at DESC");
        let agent_rows = tx
            .query(&agent_sql, &[])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let agents = agent_rows
            .iter()
            .map(decode_agent_join_row)
            .collect::<Result<Vec<_>, _>>()?;

        let mut bindings = fetch_all_runtime_bindings(&tx, self.pg_target()).await?;
        let session_rows = if let Some(pid) = project_id {
            let sql = format!(
                "SELECT {SESSION_SELECT_COLS}
                 FROM sessions
                 WHERE deleted_at IS NULL
                   AND project_id = $1
                   AND EXISTS (
                       SELECT 1 FROM projects p
                       WHERE p.project_id = sessions.project_id
                         AND p.lifecycle_state <> 'deleted_record'
                   )
                 ORDER BY pinned DESC, pinned_at DESC NULLS LAST, updated_at DESC"
            );
            tx.query(&sql, &[&pid.as_uuid()])
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?
        } else {
            let sql = format!(
                "SELECT {SESSION_SELECT_COLS}
                 FROM sessions
                 WHERE deleted_at IS NULL
                   AND (
                       sessions.project_id IS NULL
                       OR EXISTS (
                           SELECT 1 FROM projects p
                           WHERE p.project_id = sessions.project_id
                             AND p.lifecycle_state <> 'deleted_record'
                       )
                   )
                 ORDER BY pinned DESC, pinned_at DESC NULLS LAST, updated_at DESC"
            );
            tx.query(&sql, &[])
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?
        };
        let mut sessions = Vec::with_capacity(session_rows.len());
        for row in &session_rows {
            let sid = SessionId::from_uuid(row.get(0));
            sessions.push(decode_session_row(row, bindings.remove(&sid))?);
        }

        let task_rows = if let Some(pid) = project_id {
            let sql = format!(
                "SELECT {TASK_SELECT_COLS} FROM tasks
                 WHERE deleted_at IS NULL
                   AND project_id = $1
                   AND EXISTS (
                       SELECT 1 FROM projects p
                       WHERE p.project_id = tasks.project_id
                         AND p.lifecycle_state <> 'deleted_record'
                   )
                 ORDER BY updated_at DESC"
            );
            tx.query(&sql, &[&pid.as_uuid()])
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?
        } else {
            let sql = format!(
                "SELECT {TASK_SELECT_COLS} FROM tasks
                 WHERE deleted_at IS NULL
                   AND EXISTS (
                       SELECT 1 FROM projects p
                       WHERE p.project_id = tasks.project_id
                         AND p.lifecycle_state <> 'deleted_record'
                   )
                 ORDER BY updated_at DESC"
            );
            tx.query(&sql, &[])
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?
        };
        let tasks = task_rows
            .iter()
            .map(decode_task_row)
            .collect::<Result<Vec<_>, _>>()?;

        let inbox_rows = if let Some(pid) = project_id {
            let sql = format!(
                "SELECT {INBOX_CARD_SELECT_COLS} FROM inbox_cards
                 WHERE status = 'open' AND project_id = $1
                   AND (
                       kind = 'unknown_effect'
                       OR EXISTS (
                           SELECT 1 FROM projects p
                           WHERE p.project_id = inbox_cards.project_id
                             AND p.lifecycle_state <> 'deleted_record'
                       )
                   )
                 ORDER BY created_at DESC"
            );
            tx.query(&sql, &[&pid.as_uuid()])
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?
        } else {
            let sql = format!(
                "SELECT {INBOX_CARD_SELECT_COLS} FROM inbox_cards
                 WHERE status = 'open'
                   AND (
                       kind = 'unknown_effect'
                       OR EXISTS (
                           SELECT 1 FROM projects p
                           WHERE p.project_id = inbox_cards.project_id
                             AND p.lifecycle_state <> 'deleted_record'
                       )
                   )
                 ORDER BY created_at DESC"
            );
            tx.query(&sql, &[])
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?
        };
        let inbox_cards = inbox_rows
            .iter()
            .map(decode_inbox_card_row)
            .collect::<Result<Vec<_>, _>>()?;

        let grant_rows = if let Some(pid) = project_id {
            let sql = format!(
                "SELECT {CAPABILITY_GRANT_SELECT_COLS} FROM capability_grants
                 WHERE project_id = $1
                   AND EXISTS (
                       SELECT 1 FROM projects p
                       WHERE p.project_id = capability_grants.project_id
                         AND p.lifecycle_state <> 'deleted_record'
                   )
                   AND revoked_at IS NULL
                   AND (expires_at IS NULL OR expires_at > now())
                   AND (max_uses IS NULL OR uses_consumed < max_uses)
                 ORDER BY created_at DESC"
            );
            tx.query(&sql, &[&pid.as_uuid()])
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?
        } else {
            let sql = format!(
                "SELECT {CAPABILITY_GRANT_SELECT_COLS} FROM capability_grants
                 WHERE EXISTS (
                     SELECT 1 FROM projects p
                     WHERE p.project_id = capability_grants.project_id
                       AND p.lifecycle_state <> 'deleted_record'
                 )
                   AND revoked_at IS NULL
                   AND (expires_at IS NULL OR expires_at > now())
                   AND (max_uses IS NULL OR uses_consumed < max_uses)
                 ORDER BY created_at DESC"
            );
            tx.query(&sql, &[])
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?
        };
        let active_grants = grant_rows
            .iter()
            .map(decode_capability_grant_row)
            .collect::<Result<Vec<_>, _>>()?;

        let running_rows = if let Some(pid) = project_id {
            tx.query(
                "SELECT DISTINCT r.session_id
                 FROM runs r
                 JOIN sessions s ON s.session_id = r.session_id
                 JOIN projects p ON p.project_id = s.project_id
                 WHERE r.status = 'running'
                   AND s.project_id = $1
                   AND p.lifecycle_state <> 'deleted_record'",
                &[&pid.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?
        } else {
            tx.query(
                "SELECT DISTINCT r.session_id
                 FROM runs r
                 JOIN sessions s ON s.session_id = r.session_id
                 LEFT JOIN projects p ON p.project_id = s.project_id
                 WHERE r.status = 'running'
                   AND (s.project_id IS NULL OR p.lifecycle_state <> 'deleted_record')",
                &[],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?
        };
        let running_session_ids: Vec<SessionId> = running_rows
            .iter()
            .map(|r| SessionId::from_uuid(r.get(0)))
            .collect();

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let unresolved_inbox_count = inbox_cards.len();
        Ok(StateSnapshotResponse {
            snapshot_cursor,
            projects,
            agents,
            sessions,
            tasks,
            inbox_cards,
            active_grants,
            unresolved_inbox_count,
            running_session_ids,
        })
    }

    /// Reads ordered control journal events strictly after `after_cursor` (`GET /v1/events`, V15/V18).
    ///
    /// Guarantees:
    /// - Rejects mismatched `expected_epoch` (`409 Conflict`, `resource = "stream_epoch"`).
    /// - Rejects expired cursors (`after_cursor + 1 < retained_from_seq`, `409 Conflict`,
    ///   `resource = "stream_cursor_expired"`).
    /// - When `project_id` filtering is active, scans sequentially across the global journal,
    ///   returns ONLY visible rows for that project, and advances `scanned_through_seq` / `next_cursor`
    ///   past hidden rows without leaking foreign payloads!
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::Conflict`] (resync required),
    /// or [`DennettError::DatabaseUnavailable`].
    #[expect(
        clippy::too_many_lines,
        reason = "validates authority epoch and retention watermark, reads ordered journal page, filters by optional project without leaking payloads, and computes scanned_through_seq"
    )]
    pub async fn get_events(
        &self,
        after_cursor: i64,
        expected_epoch: Option<i64>,
        project_id: Option<ProjectId>,
        limit: Option<u32>,
    ) -> Result<EventStreamResponse, DennettError> {
        if after_cursor < 0 {
            return Err(DennettError::InvalidInput {
                field: String::from("after_cursor"),
                reason: String::from("must be greater than or equal to zero"),
            });
        }
        let capped_limit = limit
            .unwrap_or(DEFAULT_EVENTS_LIMIT)
            .clamp(1, MAX_EVENTS_LIMIT);
        let fetch_limit = i64::from(capped_limit) + 1;

        let _ = self.ensure_migrated().await?;
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .build_transaction()
            .isolation_level(IsolationLevel::RepeatableRead)
            .read_only(true)
            .start()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let cursor_row = tx
            .query_opt(
                "SELECT authority_epoch, committed_seq, retained_from_seq
                 FROM control_stream_counters
                 WHERE stream_id = 'control:journal'",
                &[],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let (authority_epoch, _committed_seq, retained_from_seq) = match cursor_row {
            Some(row) => (
                row.get::<_, i64>(0),
                row.get::<_, i64>(1),
                row.get::<_, i64>(2),
            ),
            None => (1_i64, 0_i64, 1_i64),
        };

        if let Some(caller_epoch) = expected_epoch
            && caller_epoch != authority_epoch
        {
            return Err(DennettError::Conflict {
                resource: String::from("stream_epoch"),
                expected_revision: Some(caller_epoch),
                actual_revision: Some(authority_epoch),
                reason: format!(
                    "stream authority_epoch changed (caller={caller_epoch}, current={authority_epoch}); snapshot resync required"
                ),
            });
        }

        if after_cursor.saturating_add(1) < retained_from_seq {
            return Err(DennettError::Conflict {
                resource: String::from("stream_cursor_expired"),
                expected_revision: Some(after_cursor),
                actual_revision: Some(retained_from_seq),
                reason: format!(
                    "cursor {after_cursor} precedes retained_from_seq {retained_from_seq}; snapshot resync required"
                ),
            });
        }

        let sql = format!(
            "SELECT {CONTROL_EVENT_SELECT_COLS}
             FROM control_events
             WHERE cursor_seq > $1 AND cursor_seq >= $2
             ORDER BY cursor_seq ASC
             LIMIT $3"
        );
        let rows = tx
            .query(&sql, &[&after_cursor, &retained_from_seq, &fetch_limit])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let max_page = usize::try_from(capped_limit).unwrap_or(100);
        let has_more = rows.len() > max_page;
        let page_rows = if has_more {
            &rows[..max_page]
        } else {
            &rows[..]
        };

        let mut scanned_through_seq = after_cursor;
        let mut events = Vec::with_capacity(page_rows.len());
        for row in page_rows {
            let ev = decode_control_event_row(row)?;
            if ev.cursor_seq > scanned_through_seq {
                scanned_through_seq = ev.cursor_seq;
            }
            if let Some(pid) = project_id {
                if ev.project_id == Some(pid) {
                    events.push(ev);
                }
            } else {
                events.push(ev);
            }
        }

        Ok(EventStreamResponse {
            authority_epoch,
            retained_from_seq,
            after_cursor,
            scanned_through_seq,
            next_cursor: scanned_through_seq,
            has_more,
            events,
        })
    }

    /// Enqueues a single per-key ordered outbox item inside its own transaction (V16/V17).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::DatabaseUnavailable`] on PostgreSQL error.
    pub async fn enqueue_ordered_outbox(
        &self,
        destination: &str,
        ordering_key: &str,
        cursor_seq: i64,
        idempotency_key: &IdempotencyKey,
        payload: &JsonValue,
        correlation_id: &CorrelationId,
    ) -> Result<OrderedOutboxItem, DennettError> {
        let _ = self.ensure_migrated().await?;
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let item = enqueue_ordered_outbox_in_tx(
            &tx,
            self.pg_target(),
            destination,
            ordering_key,
            cursor_seq,
            idempotency_key,
            payload,
            correlation_id,
        )
        .await?;
        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        Ok(item)
    }

    /// Claims up to `batch_size` head-of-key `ordered_outbox` items strictly via
    /// `JOIN ordered_outbox_heads ... FOR UPDATE OF h, o SKIP LOCKED` (V16, V17).
    ///
    /// Two concurrent workers NEVER overtake each other on the same `ordering_key`, while
    /// different `ordering_key`s proceed concurrently and expired leases bump `lease_generation`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] or [`DennettError::DatabaseUnavailable`].
    pub async fn claim_ordered_outbox_batch(
        &self,
        destination: &str,
        worker_id: &str,
        batch_size: i64,
        lease_ms: i64,
        now: TimestampMs,
    ) -> Result<Vec<OrderedOutboxItem>, DennettError> {
        let capped_batch = batch_size.clamp(1, 128);
        let safe_lease_ms = lease_ms.clamp(100, 600_000);
        let now_secs = ms_to_pg_epoch_secs(now.as_millis());
        let lease_secs = ms_to_pg_epoch_secs(safe_lease_ms);

        let _ = self.ensure_migrated().await?;
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let sql = format!(
            "WITH claimable_head AS (
                SELECT o.item_id AS claimed_item_id
                FROM ordered_outbox_heads h
                JOIN ordered_outbox o
                  ON o.destination = h.destination
                 AND o.ordering_key = h.ordering_key
                 AND o.key_seq = h.next_deliver_seq
                WHERE h.destination = $1
                  AND o.available_at <= to_timestamp($2::double precision)
                  AND (
                      o.status = 'pending'
                      OR (o.status = 'leased' AND o.lease_expires_at <= to_timestamp($2::double precision))
                  )
                ORDER BY o.cursor_seq ASC
                LIMIT $3
                FOR UPDATE OF h, o SKIP LOCKED
            )
            UPDATE ordered_outbox o
            SET status = 'leased',
                lease_owner = $4,
                lease_generation = o.lease_generation + 1,
                lease_expires_at = to_timestamp($2::double precision + $5::double precision),
                attempts = o.attempts + 1,
                updated_at = to_timestamp($2::double precision)
            FROM claimable_head ch
            WHERE o.item_id = ch.claimed_item_id
            RETURNING {ORDERED_OUTBOX_SELECT_COLS}"
        );
        let rows = tx
            .query(
                &sql,
                &[
                    &truncate_utf8(destination, 64),
                    &now_secs,
                    &capped_batch,
                    &truncate_utf8(worker_id, 128),
                    &lease_secs,
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        rows.iter().map(decode_ordered_outbox_row).collect()
    }

    /// Acknowledges a leased `ordered_outbox` item strictly fenced by `expected_lease_generation`
    /// and advances `ordered_outbox_heads.next_deliver_seq` atomically (V16, V17).
    ///
    /// Lock order strictly matches `claim_ordered_outbox_batch` (`ordered_outbox_heads` -> `ordered_outbox`).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`] if `item_id` does not exist, [`DennettError::Conflict`]
    /// if the lease was lost/superseded (`lease_generation` mismatch), or [`DennettError::DatabaseUnavailable`].
    pub async fn ack_ordered_outbox_item(
        &self,
        item_id: OrderedOutboxItemId,
        expected_lease_generation: i64,
    ) -> Result<OrderedOutboxItem, DennettError> {
        self.ack_ordered_outbox_item_with_payload(item_id, expected_lease_generation, None)
            .await
    }

    /// Acknowledges a leased `ordered_outbox` item fenced by `expected_lease_generation`,
    /// optionally updating `payload_json` (e.g. recording `delivered_event_id`/`delivered_note_id`)
    /// and advancing `ordered_outbox_heads.next_deliver_seq` atomically.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`], [`DennettError::Conflict`], or [`DennettError::DatabaseUnavailable`].
    pub async fn ack_ordered_outbox_item_with_payload(
        &self,
        item_id: OrderedOutboxItemId,
        expected_lease_generation: i64,
        updated_payload: Option<&JsonValue>,
    ) -> Result<OrderedOutboxItem, DennettError> {
        let _ = self.ensure_migrated().await?;
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let meta_row = tx
            .query_opt(
                "SELECT destination, ordering_key, key_seq, status, lease_generation
                 FROM ordered_outbox
                 WHERE item_id = $1",
                &[&item_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let Some(meta) = meta_row else {
            return Err(DennettError::NotFound {
                resource: String::from("ordered_outbox"),
                id: item_id.to_string(),
            });
        };
        let destination: String = meta.get(0);
        let ordering_key: String = meta.get(1);
        let key_seq: i64 = meta.get(2);

        // Lock head FIRST (same order as claim_ordered_outbox_batch -> zero deadlocks).
        let head_seq: i64 = tx
            .query_one(
                "SELECT next_deliver_seq
                 FROM ordered_outbox_heads
                 WHERE destination = $1 AND ordering_key = $2
                 FOR UPDATE",
                &[&destination, &ordering_key],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?
            .get(0);

        let update_sql = format!(
            "UPDATE ordered_outbox
             SET status = 'delivered',
                 payload_json = COALESCE($3, payload_json),
                 last_error = NULL,
                 updated_at = now()
             WHERE item_id = $1
               AND status = 'leased'
               AND lease_generation = $2
             RETURNING {ORDERED_OUTBOX_SELECT_COLS}"
        );
        let updated = tx
            .query_opt(
                &update_sql,
                &[
                    &item_id.as_uuid(),
                    &expected_lease_generation,
                    &updated_payload,
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let Some(row) = updated else {
            let actual_gen: i64 = meta.get(4);
            let actual_status: String = meta.get(3);
            return Err(DennettError::Conflict {
                resource: String::from("ordered_outbox"),
                expected_revision: Some(expected_lease_generation),
                actual_revision: Some(actual_gen),
                reason: format!(
                    "stale outbox lease ack rejected (status={actual_status}, expected_gen={expected_lease_generation}, actual_gen={actual_gen})"
                ),
            });
        };

        if head_seq == key_seq {
            tx.execute(
                "UPDATE ordered_outbox_heads
                 SET next_deliver_seq = next_deliver_seq + 1
                 WHERE destination = $1 AND ordering_key = $2",
                &[&destination, &ordering_key],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        }

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        decode_ordered_outbox_row(&row)
    }

    /// Fails or parks a leased `ordered_outbox` item fenced by `expected_lease_generation` (V16).
    ///
    /// When `attempts >= max_attempts`, transitions strictly this item to `parked`, halting ONLY
    /// its own `ordering_key` while all other keys continue unobstructed.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::Conflict`] on stale lease generation or [`DennettError::DatabaseUnavailable`].
    pub async fn nack_ordered_outbox_item(
        &self,
        item_id: OrderedOutboxItemId,
        expected_lease_generation: i64,
        error_detail: &str,
        max_attempts: i32,
        retry_delay_ms: i64,
        now: TimestampMs,
    ) -> Result<OrderedOutboxItem, DennettError> {
        self.nack_ordered_outbox_item_with_payload(
            item_id,
            expected_lease_generation,
            error_detail,
            max_attempts,
            retry_delay_ms,
            now,
            None,
        )
        .await
    }

    /// Fails or parks a leased `ordered_outbox` item fenced by `expected_lease_generation`,
    /// optionally atomically replacing `payload_json` (e.g. scrubbing a forgotten note's payload
    /// when parking a lost-ACK retry).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::Conflict`] on stale lease generation or [`DennettError::DatabaseUnavailable`].
    #[expect(
        clippy::too_many_arguments,
        reason = "fenced outbox nack carries lease generation, retry bounds, clock, and optional scrubbed payload"
    )]
    pub async fn nack_ordered_outbox_item_with_payload(
        &self,
        item_id: OrderedOutboxItemId,
        expected_lease_generation: i64,
        error_detail: &str,
        max_attempts: i32,
        retry_delay_ms: i64,
        now: TimestampMs,
        updated_payload: Option<&JsonValue>,
    ) -> Result<OrderedOutboxItem, DennettError> {
        let _ = self.ensure_migrated().await?;
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let safe_err = truncate_utf8(&self.redact(error_detail), 2000).to_owned();
        let next_avail_secs = ms_to_pg_epoch_secs(now.as_millis() + retry_delay_ms.max(0));
        let now_secs = ms_to_pg_epoch_secs(now.as_millis());

        let sql = format!(
            "UPDATE ordered_outbox
             SET status = CASE WHEN attempts >= $3 THEN 'parked' ELSE 'pending' END,
                 payload_json = COALESCE($7, payload_json),
                 last_error = $4,
                 available_at = to_timestamp($5),
                 lease_expires_at = NULL,
                 updated_at = to_timestamp($6)
             WHERE item_id = $1
               AND status = 'leased'
               AND lease_generation = $2
             RETURNING {ORDERED_OUTBOX_SELECT_COLS}"
        );
        let row = tx
            .query_opt(
                &sql,
                &[
                    &item_id.as_uuid(),
                    &expected_lease_generation,
                    &max_attempts.max(1),
                    &safe_err,
                    &next_avail_secs,
                    &now_secs,
                    &updated_payload,
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let Some(row) = row else {
            return Err(DennettError::Conflict {
                resource: String::from("ordered_outbox"),
                expected_revision: Some(expected_lease_generation),
                actual_revision: None,
                reason: String::from("stale outbox lease nack rejected"),
            });
        };

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        decode_ordered_outbox_row(&row)
    }

    /// Test-isolated helper advancing `retained_from_seq` on `'control:journal'` without deleting
    /// append-only `control_events` rows, enabling deterministic V18 expired-cursor resync tests.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::SafetyGuardRejected`] outside [`ProfileMode::TestIsolated`],
    /// [`DennettError::InvalidInput`] if `retained_from_seq < 1`, or [`DennettError::DatabaseUnavailable`].
    pub async fn advance_retention_watermark(
        &self,
        retained_from_seq: i64,
    ) -> Result<StreamCursor, DennettError> {
        if self.profile_mode() != ProfileMode::TestIsolated {
            return Err(DennettError::SafetyGuardRejected {
                reason: String::from(
                    "advance_retention_watermark is only permitted in test_isolated profile",
                ),
            });
        }
        if retained_from_seq < 1 {
            return Err(DennettError::InvalidInput {
                field: String::from("retained_from_seq"),
                reason: String::from("must be >= 1"),
            });
        }
        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let row = conn
            .query_one(
                "UPDATE control_stream_counters
                 SET retained_from_seq = GREATEST(retained_from_seq, $1)
                 WHERE stream_id = 'control:journal'
                 RETURNING authority_epoch, committed_seq, retained_from_seq",
                &[&retained_from_seq],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        Ok(StreamCursor {
            authority_epoch: row.get(0),
            committed_seq: row.get(1),
            retained_from_seq: row.get(2),
        })
    }

    /// Test-isolated helper incrementing `authority_epoch` on `'control:journal'` to verify V18
    /// epoch-mismatch resync detection.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::SafetyGuardRejected`] outside [`ProfileMode::TestIsolated`].
    pub async fn bump_authority_epoch(&self) -> Result<StreamCursor, DennettError> {
        if self.profile_mode() != ProfileMode::TestIsolated {
            return Err(DennettError::SafetyGuardRejected {
                reason: String::from(
                    "bump_authority_epoch is only permitted in test_isolated profile",
                ),
            });
        }
        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let row = conn
            .query_one(
                "UPDATE control_stream_counters
                 SET authority_epoch = authority_epoch + 1
                 WHERE stream_id = 'control:journal'
                 RETURNING authority_epoch, committed_seq, retained_from_seq",
                &[],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        Ok(StreamCursor {
            authority_epoch: row.get(0),
            committed_seq: row.get(1),
            retained_from_seq: row.get(2),
        })
    }
}
