//! WAL-backed SQLite control-store adapter for local Dennett profiles.

use async_trait::async_trait;
use dennett_agent_core::{OpaqueContinuation, RuntimeContinuationError};
use dennett_contracts::{CommandId, ProjectId, SessionEventId, SessionId};
use dennett_memory_core::session::{
    CommittedSessionEvent, PendingSessionEvent, SESSION_EVENT_PAYLOAD_VERSION, SessionEventBody,
    SessionEventStore, SessionJournalError,
};
use dennett_sync_core::admission::{
    CommandAdmissionError, CommandAdmissionPort, CommandAdmissionReceipt, CommandAdmissionRequest,
};
use dennett_sync_core::draft::{
    DraftCacheError, DraftCachePort, DraftCacheSaveOutcome, DraftRecord,
};
use sha2::{Digest, Sha256};
use sqlx::{
    Row, SqlitePool,
    migrate::Migrator,
    sqlite::{
        SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow, SqliteSynchronous,
    },
};
use std::{
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

pub const CONTROL_SCHEMA_VERSION: u32 = 2;

#[derive(Clone)]
pub struct SqliteControlStore {
    pool: SqlitePool,
}

impl SqliteControlStore {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, SessionJournalError> {
        let options = SqliteConnectOptions::new()
            .filename(path.as_ref())
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Full)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(5))
            .connect_with(options)
            .await
            .map_err(|_| SessionJournalError::StorageUnavailable)?;

        let before = schema_version_for(&pool).await?;
        if before > CONTROL_SCHEMA_VERSION {
            pool.close().await;
            return Err(SessionJournalError::UnsupportedSchemaVersion {
                found: before,
                supported: CONTROL_SCHEMA_VERSION,
            });
        }
        if before == 0 && has_unversioned_application_tables(&pool).await? {
            pool.close().await;
            return Err(SessionJournalError::MigrationFailure);
        }

        let migration_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let migrator = Migrator::new(migration_path.as_path())
            .await
            .map_err(|_| SessionJournalError::MigrationFailure)?;
        migrator
            .run(&pool)
            .await
            .map_err(|_| SessionJournalError::MigrationFailure)?;
        if schema_version_for(&pool).await? != CONTROL_SCHEMA_VERSION {
            pool.close().await;
            return Err(SessionJournalError::MigrationFailure);
        }

        let store = Self { pool };
        store.verify_integrity().await?;
        Ok(store)
    }

    pub async fn close(&self) {
        self.pool.close().await;
    }

    pub async fn schema_version(&self) -> Result<u32, SessionJournalError> {
        schema_version_for(&self.pool).await
    }

    pub async fn journal_mode(&self) -> Result<String, SessionJournalError> {
        sqlx::query_scalar::<_, String>("PRAGMA journal_mode")
            .fetch_one(&self.pool)
            .await
            .map_err(|_| SessionJournalError::StorageUnavailable)
    }

    pub async fn verify_integrity(&self) -> Result<(), SessionJournalError> {
        let quick_check = sqlx::query_scalar::<_, String>("PRAGMA quick_check")
            .fetch_one(&self.pool)
            .await
            .map_err(|_| SessionJournalError::StorageUnavailable)?;
        if quick_check != "ok" {
            return Err(SessionJournalError::IntegrityFailure(
                "sqlite quick_check failed",
            ));
        }
        if !sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&self.pool)
            .await
            .map_err(|_| SessionJournalError::StorageUnavailable)?
            .is_empty()
        {
            return Err(SessionJournalError::IntegrityFailure(
                "sqlite foreign key check failed",
            ));
        }
        for session_id in self.list_session_ids().await? {
            self.load_session(session_id).await?;
        }
        Ok(())
    }
}

async fn schema_version_for(pool: &SqlitePool) -> Result<u32, SessionJournalError> {
    let version = sqlx::query_scalar::<_, i64>("PRAGMA user_version")
        .fetch_one(pool)
        .await
        .map_err(|_| SessionJournalError::StorageUnavailable)?;
    u32::try_from(version).map_err(|_| SessionJournalError::MigrationFailure)
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

async fn has_unversioned_application_tables(
    pool: &SqlitePool,
) -> Result<bool, SessionJournalError> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sqlite_master \
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name != '_sqlx_migrations'",
    )
    .fetch_one(pool)
    .await
    .map_err(|_| SessionJournalError::StorageUnavailable)?;
    Ok(count != 0)
}

fn parse_uuid(value: &str) -> Result<Uuid, SessionJournalError> {
    Uuid::parse_str(value)
        .map_err(|_| SessionJournalError::IntegrityFailure("invalid stable identifier"))
}

fn parse_event(row: SqliteRow) -> Result<CommittedSessionEvent, SessionJournalError> {
    let event_id: String = row
        .try_get("event_id")
        .map_err(|_| SessionJournalError::IntegrityFailure("event id is missing"))?;
    let session_id: String = row
        .try_get("session_id")
        .map_err(|_| SessionJournalError::IntegrityFailure("session id is missing"))?;
    let revision: i64 = row
        .try_get("revision")
        .map_err(|_| SessionJournalError::IntegrityFailure("revision is missing"))?;
    let payload_version: i64 = row
        .try_get("payload_version")
        .map_err(|_| SessionJournalError::IntegrityFailure("event payload version is missing"))?;
    let command_id: Option<String> = row
        .try_get("command_id")
        .map_err(|_| SessionJournalError::IntegrityFailure("command id is malformed"))?;
    let body_json: String = row
        .try_get("body_json")
        .map_err(|_| SessionJournalError::IntegrityFailure("event body is missing"))?;
    let stored_hash: Vec<u8> = row
        .try_get("event_sha256")
        .map_err(|_| SessionJournalError::IntegrityFailure("event hash is missing"))?;
    let committed_at: i64 = row
        .try_get("committed_at_unix_ms")
        .map_err(|_| SessionJournalError::IntegrityFailure("event time is missing"))?;

    let revision = u64::try_from(revision)
        .map_err(|_| SessionJournalError::IntegrityFailure("negative revision"))?;
    let payload_version = u32::try_from(payload_version)
        .map_err(|_| SessionJournalError::IntegrityFailure("invalid event payload version"))?;
    let committed_at_unix_ms = u64::try_from(committed_at)
        .map_err(|_| SessionJournalError::IntegrityFailure("negative event time"))?;
    let expected_hash = stored_event_hash(
        &event_id,
        &session_id,
        revision,
        command_id.as_deref(),
        payload_version,
        &body_json,
        committed_at_unix_ms,
    )?;
    if stored_hash.as_slice() != expected_hash {
        return Err(SessionJournalError::IntegrityFailure(
            "event checksum mismatch",
        ));
    }
    if payload_version != SESSION_EVENT_PAYLOAD_VERSION {
        return Err(SessionJournalError::UnsupportedEventPayloadVersion {
            found: payload_version,
            supported: SESSION_EVENT_PAYLOAD_VERSION,
        });
    }

    Ok(CommittedSessionEvent {
        event_id: SessionEventId(parse_uuid(&event_id)?),
        session_id: SessionId(parse_uuid(&session_id)?),
        revision,
        payload_version,
        command_id: command_id
            .as_deref()
            .map(parse_uuid)
            .transpose()?
            .map(CommandId),
        body: serde_json::from_str::<SessionEventBody>(&body_json)
            .map_err(|_| SessionJournalError::IntegrityFailure("event body is malformed"))?,
        committed_at_unix_ms,
    })
}

fn stored_event_hash(
    event_id: &str,
    session_id: &str,
    revision: u64,
    command_id: Option<&str>,
    payload_version: u32,
    body_json: &str,
    committed_at_unix_ms: u64,
) -> Result<[u8; 32], SessionJournalError> {
    fn field(hasher: &mut Sha256, value: &[u8]) -> Result<(), SessionJournalError> {
        let length = u64::try_from(value.len())
            .map_err(|_| SessionJournalError::IntegrityFailure("event field is too large"))?;
        hasher.update(length.to_be_bytes());
        hasher.update(value);
        Ok(())
    }

    let mut hasher = Sha256::new();
    hasher.update(b"dennett.project-session-event.v1\0");
    field(&mut hasher, event_id.as_bytes())?;
    field(&mut hasher, session_id.as_bytes())?;
    hasher.update(revision.to_be_bytes());
    match command_id {
        Some(command_id) => {
            hasher.update([1]);
            field(&mut hasher, command_id.as_bytes())?;
        }
        None => hasher.update([0]),
    }
    hasher.update(payload_version.to_be_bytes());
    field(&mut hasher, body_json.as_bytes())?;
    hasher.update(committed_at_unix_ms.to_be_bytes());
    Ok(hasher.finalize().into())
}

#[async_trait]
impl SessionEventStore for SqliteControlStore {
    async fn append(
        &self,
        expected_revision: u64,
        pending: PendingSessionEvent,
    ) -> Result<CommittedSessionEvent, SessionJournalError> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|_| SessionJournalError::StorageUnavailable)?;

        let by_event = sqlx::query(
            "SELECT event_id, session_id, revision, payload_version, command_id, body_json, event_sha256, \
                    committed_at_unix_ms \
             FROM session_events WHERE event_id = ?",
        )
        .bind(pending.event_id.0.to_string())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| SessionJournalError::StorageUnavailable)?;
        if let Some(row) = by_event {
            let existing = parse_event(row)?;
            return if existing.matches_pending(&pending) {
                Ok(existing)
            } else {
                Err(SessionJournalError::IdempotencyConflict)
            };
        }

        if let Some(command_id) = pending.command_id {
            let by_command = sqlx::query(
                "SELECT event_id, session_id, revision, payload_version, command_id, body_json, event_sha256, \
                        committed_at_unix_ms \
                 FROM session_events WHERE command_id = ?",
            )
            .bind(command_id.0.to_string())
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|_| SessionJournalError::StorageUnavailable)?;
            if let Some(row) = by_command {
                let existing = parse_event(row)?;
                return if existing.matches_pending(&pending) {
                    Ok(existing)
                } else {
                    Err(SessionJournalError::IdempotencyConflict)
                };
            }
        }

        let session_key = pending.session_id.0.to_string();
        let actual_revision =
            sqlx::query_scalar::<_, i64>("SELECT revision FROM session_heads WHERE session_id = ?")
                .bind(&session_key)
                .fetch_optional(&mut *transaction)
                .await
                .map_err(|_| SessionJournalError::StorageUnavailable)?
                .map_or(Ok(0), |revision| {
                    u64::try_from(revision).map_err(|_| {
                        SessionJournalError::IntegrityFailure("negative session head revision")
                    })
                })?;
        if actual_revision != expected_revision {
            return Err(SessionJournalError::RevisionConflict {
                expected: expected_revision,
                actual: actual_revision,
            });
        }

        let revision = actual_revision + 1;
        if actual_revision == 0 {
            sqlx::query("INSERT INTO session_heads(session_id, revision) VALUES (?, ?)")
                .bind(&session_key)
                .bind(
                    i64::try_from(revision)
                        .map_err(|_| SessionJournalError::IntegrityFailure("revision overflow"))?,
                )
                .execute(&mut *transaction)
                .await
                .map_err(|_| SessionJournalError::StorageUnavailable)?;
        } else {
            let updated = sqlx::query(
                "UPDATE session_heads SET revision = ? WHERE session_id = ? AND revision = ?",
            )
            .bind(
                i64::try_from(revision)
                    .map_err(|_| SessionJournalError::IntegrityFailure("revision overflow"))?,
            )
            .bind(&session_key)
            .bind(
                i64::try_from(actual_revision)
                    .map_err(|_| SessionJournalError::IntegrityFailure("revision overflow"))?,
            )
            .execute(&mut *transaction)
            .await
            .map_err(|_| SessionJournalError::StorageUnavailable)?;
            if updated.rows_affected() != 1 {
                return Err(SessionJournalError::RevisionConflict {
                    expected: expected_revision,
                    actual: actual_revision,
                });
            }
        }

        let committed = CommittedSessionEvent {
            event_id: pending.event_id,
            session_id: pending.session_id,
            revision,
            payload_version: SESSION_EVENT_PAYLOAD_VERSION,
            command_id: pending.command_id,
            body: pending.body,
            committed_at_unix_ms: pending.committed_at_unix_ms,
        };
        let body_json = serde_json::to_string(&committed.body)
            .map_err(|_| SessionJournalError::IntegrityFailure("event serialization failed"))?;
        let checksum = stored_event_hash(
            &committed.event_id.0.to_string(),
            &session_key,
            revision,
            committed.command_id.map(|id| id.0.to_string()).as_deref(),
            committed.payload_version,
            &body_json,
            committed.committed_at_unix_ms,
        )?;
        sqlx::query(
            "INSERT INTO session_events(\
                event_id, session_id, revision, payload_version, command_id, body_json, event_sha256, committed_at_unix_ms\
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(committed.event_id.0.to_string())
        .bind(&session_key)
        .bind(
            i64::try_from(revision)
                .map_err(|_| SessionJournalError::IntegrityFailure("revision overflow"))?,
        )
        .bind(i64::from(committed.payload_version))
        .bind(committed.command_id.map(|id| id.0.to_string()))
        .bind(body_json)
        .bind(checksum.as_slice())
        .bind(i64::try_from(committed.committed_at_unix_ms).map_err(|_| {
            SessionJournalError::IntegrityFailure("event time overflow")
        })?)
        .execute(&mut *transaction)
        .await
        .map_err(|_| SessionJournalError::StorageUnavailable)?;
        transaction
            .commit()
            .await
            .map_err(|_| SessionJournalError::StorageUnavailable)?;
        Ok(committed)
    }

    async fn load_session(
        &self,
        session_id: SessionId,
    ) -> Result<Vec<CommittedSessionEvent>, SessionJournalError> {
        let rows = sqlx::query(
            "SELECT event_id, session_id, revision, payload_version, command_id, body_json, event_sha256, \
                    committed_at_unix_ms \
             FROM session_events WHERE session_id = ? ORDER BY revision",
        )
        .bind(session_id.0.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|_| SessionJournalError::StorageUnavailable)?;
        let events = rows
            .into_iter()
            .map(parse_event)
            .collect::<Result<Vec<_>, _>>()?;
        for (index, event) in events.iter().enumerate() {
            let expected_revision = u64::try_from(index)
                .map_err(|_| SessionJournalError::IntegrityFailure("revision overflow"))?
                + 1;
            if event.revision != expected_revision {
                return Err(SessionJournalError::IntegrityFailure(
                    "non-contiguous stored revision",
                ));
            }
        }
        let head =
            sqlx::query_scalar::<_, i64>("SELECT revision FROM session_heads WHERE session_id = ?")
                .bind(session_id.0.to_string())
                .fetch_optional(&self.pool)
                .await
                .map_err(|_| SessionJournalError::StorageUnavailable)?;
        let event_revision = events.last().map_or(0, |event| event.revision);
        let head_revision = head
            .map(|revision| {
                u64::try_from(revision).map_err(|_| {
                    SessionJournalError::IntegrityFailure("negative session head revision")
                })
            })
            .transpose()?
            .unwrap_or(0);
        if head_revision != event_revision {
            return Err(SessionJournalError::IntegrityFailure(
                "session head does not match event journal",
            ));
        }
        Ok(events)
    }

    async fn event_for_command(
        &self,
        command_id: CommandId,
    ) -> Result<Option<CommittedSessionEvent>, SessionJournalError> {
        sqlx::query(
            "SELECT event_id, session_id, revision, payload_version, command_id, body_json, event_sha256, \
                    committed_at_unix_ms \
             FROM session_events WHERE command_id = ?",
        )
        .bind(command_id.0.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| SessionJournalError::StorageUnavailable)?
        .map(parse_event)
        .transpose()
    }

    async fn list_session_ids(&self) -> Result<Vec<SessionId>, SessionJournalError> {
        let rows = sqlx::query_scalar::<_, String>(
            "SELECT session_id FROM session_heads ORDER BY session_id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|_| SessionJournalError::StorageUnavailable)?;
        rows.into_iter()
            .map(|value| parse_uuid(&value).map(SessionId))
            .collect()
    }
}

#[async_trait]
impl CommandAdmissionPort for SqliteControlStore {
    async fn admit(
        &self,
        request: CommandAdmissionRequest,
    ) -> Result<CommandAdmissionReceipt, CommandAdmissionError> {
        request.validate()?;
        let command_id = request.command_id.0.to_string();
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|_| CommandAdmissionError::StorageUnavailable)?;
        let rows = sqlx::query(
            "SELECT accepted_revision, command_id, idempotency_key, operation_kind, intent_sha256 \
             FROM command_admissions WHERE command_id = ? OR idempotency_key = ?",
        )
        .bind(&command_id)
        .bind(&request.idempotency_key)
        .fetch_all(&mut *transaction)
        .await
        .map_err(|_| CommandAdmissionError::StorageUnavailable)?;
        if rows.len() > 1 {
            return Err(CommandAdmissionError::IntegrityFailure);
        }
        if let Some(row) = rows.first() {
            let accepted_revision = u64::try_from(
                row.try_get::<i64, _>("accepted_revision")
                    .map_err(|_| CommandAdmissionError::IntegrityFailure)?,
            )
            .map_err(|_| CommandAdmissionError::IntegrityFailure)?;
            let existing_command: String = row
                .try_get("command_id")
                .map_err(|_| CommandAdmissionError::IntegrityFailure)?;
            let existing_key: String = row
                .try_get("idempotency_key")
                .map_err(|_| CommandAdmissionError::IntegrityFailure)?;
            let existing_kind: String = row
                .try_get("operation_kind")
                .map_err(|_| CommandAdmissionError::IntegrityFailure)?;
            let existing_intent: Vec<u8> = row
                .try_get("intent_sha256")
                .map_err(|_| CommandAdmissionError::IntegrityFailure)?;
            if existing_command != command_id
                || existing_key != request.idempotency_key
                || existing_kind != request.operation_kind
                || existing_intent.as_slice() != request.intent_hash
            {
                return Err(CommandAdmissionError::IdempotencyConflict);
            }
            return Ok(admission_receipt(&request, accepted_revision));
        }
        let accepted_revision = sqlx::query_scalar::<_, i64>(
            "INSERT INTO command_admissions(\
                command_id, idempotency_key, correlation_id, operation_kind, intent_sha256, admitted_at_unix_ms\
             ) VALUES (?, ?, ?, ?, ?, ?) RETURNING accepted_revision",
        )
        .bind(&command_id)
        .bind(&request.idempotency_key)
        .bind(&request.correlation_id)
        .bind(&request.operation_kind)
        .bind(request.intent_hash.as_slice())
        .bind(
            i64::try_from(request.admitted_at_unix_ms)
                .map_err(|_| CommandAdmissionError::InvalidRequest)?,
        )
        .fetch_one(&mut *transaction)
        .await
        .map_err(|_| CommandAdmissionError::StorageUnavailable)?;
        transaction
            .commit()
            .await
            .map_err(|_| CommandAdmissionError::StorageUnavailable)?;
        let accepted_revision = u64::try_from(accepted_revision)
            .map_err(|_| CommandAdmissionError::IntegrityFailure)?;
        Ok(admission_receipt(&request, accepted_revision))
    }
}

fn admission_receipt(
    request: &CommandAdmissionRequest,
    accepted_revision: u64,
) -> CommandAdmissionReceipt {
    CommandAdmissionReceipt {
        command_id: request.command_id,
        operation_id: request.command_id,
        correlation_id: request.correlation_id.clone(),
        accepted_revision,
    }
}

#[async_trait]
impl dennett_agent_core::RuntimeContinuationPort for SqliteControlStore {
    async fn load(
        &self,
        session_id: &str,
    ) -> Result<Option<OpaqueContinuation>, RuntimeContinuationError> {
        if session_id.trim().is_empty() {
            return Err(RuntimeContinuationError::InvalidRequest);
        }
        let row = sqlx::query(
            "SELECT adapter_id, opaque_handle FROM runtime_continuations WHERE session_id = ?",
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| RuntimeContinuationError::StorageUnavailable)?;
        row.map(|row| {
            let adapter_id: String = row
                .try_get("adapter_id")
                .map_err(|_| RuntimeContinuationError::IntegrityFailure)?;
            let handle: String = row
                .try_get("opaque_handle")
                .map_err(|_| RuntimeContinuationError::IntegrityFailure)?;
            OpaqueContinuation::new(adapter_id, handle)
                .map_err(|_| RuntimeContinuationError::IntegrityFailure)
        })
        .transpose()
    }

    async fn save(
        &self,
        session_id: &str,
        continuation: &OpaqueContinuation,
    ) -> Result<(), RuntimeContinuationError> {
        if session_id.trim().is_empty() {
            return Err(RuntimeContinuationError::InvalidRequest);
        }
        let adapter_id = continuation.adapter_id();
        let handle = continuation
            .handle_for(adapter_id)
            .map_err(|_| RuntimeContinuationError::IntegrityFailure)?;
        sqlx::query(
            "INSERT INTO runtime_continuations(session_id, adapter_id, opaque_handle, updated_at_unix_ms) \
             VALUES (?, ?, ?, ?) ON CONFLICT(session_id) DO UPDATE SET \
             adapter_id = excluded.adapter_id, opaque_handle = excluded.opaque_handle, \
             updated_at_unix_ms = excluded.updated_at_unix_ms",
        )
        .bind(session_id)
        .bind(adapter_id)
        .bind(handle)
        .bind(
            i64::try_from(unix_time_ms())
                .map_err(|_| RuntimeContinuationError::StorageUnavailable)?,
        )
        .execute(&self.pool)
        .await
        .map_err(|_| RuntimeContinuationError::StorageUnavailable)?;
        Ok(())
    }
}

#[async_trait]
impl DraftCachePort for SqliteControlStore {
    async fn save(&self, draft: DraftRecord) -> Result<DraftCacheSaveOutcome, DraftCacheError> {
        if draft.revision == 0 {
            return Err(DraftCacheError::StorageUnavailable);
        }
        let session_id = draft.session_id.0.to_string();
        let command_id = draft.command_id.0.to_string();
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|_| DraftCacheError::StorageUnavailable)?;
        let tombstone_owner = sqlx::query_scalar::<_, String>(
            "SELECT session_id FROM discarded_draft_commands WHERE command_id = ?",
        )
        .bind(&command_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| DraftCacheError::StorageUnavailable)?;
        if let Some(owner) = tombstone_owner {
            return if owner == session_id {
                Ok(DraftCacheSaveOutcome::Discarded)
            } else {
                Err(DraftCacheError::StableCommandMismatch)
            };
        }
        let existing =
            sqlx::query("SELECT command_id, revision FROM client_drafts WHERE session_id = ?")
                .bind(&session_id)
                .fetch_optional(&mut *transaction)
                .await
                .map_err(|_| DraftCacheError::StorageUnavailable)?;
        if let Some(row) = existing {
            let existing_command: String = row
                .try_get("command_id")
                .map_err(|_| DraftCacheError::StorageUnavailable)?;
            let existing_revision: i64 = row
                .try_get("revision")
                .map_err(|_| DraftCacheError::StorageUnavailable)?;
            if existing_command != command_id {
                return Err(DraftCacheError::StableCommandMismatch);
            }
            if u64::try_from(existing_revision).map_err(|_| DraftCacheError::StorageUnavailable)?
                >= draft.revision
            {
                return Ok(DraftCacheSaveOutcome::StaleIgnored);
            }
        }
        let command_owner = sqlx::query_scalar::<_, String>(
            "SELECT session_id FROM client_drafts WHERE command_id = ?",
        )
        .bind(&command_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| DraftCacheError::StorageUnavailable)?;
        if command_owner
            .as_deref()
            .is_some_and(|value| value != session_id)
        {
            return Err(DraftCacheError::StableCommandMismatch);
        }
        sqlx::query(
            "INSERT INTO client_drafts(session_id, project_id, command_id, text, updated_at_unix_ms, revision) \
             VALUES (?, ?, ?, ?, ?, ?) \
             ON CONFLICT(session_id) DO UPDATE SET \
               project_id = excluded.project_id, text = excluded.text, \
               updated_at_unix_ms = excluded.updated_at_unix_ms, revision = excluded.revision",
        )
        .bind(&session_id)
        .bind(draft.project_id.0.to_string())
        .bind(&command_id)
        .bind(draft.text)
        .bind(
            i64::try_from(draft.updated_at_unix_ms)
                .map_err(|_| DraftCacheError::StorageUnavailable)?,
        )
        .bind(i64::try_from(draft.revision).map_err(|_| DraftCacheError::StorageUnavailable)?)
        .execute(&mut *transaction)
        .await
        .map_err(|_| DraftCacheError::StorageUnavailable)?;
        transaction
            .commit()
            .await
            .map_err(|_| DraftCacheError::StorageUnavailable)?;
        Ok(DraftCacheSaveOutcome::Saved)
    }

    async fn load(&self, session_id: SessionId) -> Result<Option<DraftRecord>, DraftCacheError> {
        let row = sqlx::query(
            "SELECT project_id, session_id, command_id, text, updated_at_unix_ms, revision \
             FROM client_drafts WHERE session_id = ?",
        )
        .bind(session_id.0.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| DraftCacheError::StorageUnavailable)?;
        row.map(|row| {
            let project_id: String = row
                .try_get("project_id")
                .map_err(|_| DraftCacheError::StorageUnavailable)?;
            let session_id: String = row
                .try_get("session_id")
                .map_err(|_| DraftCacheError::StorageUnavailable)?;
            let command_id: String = row
                .try_get("command_id")
                .map_err(|_| DraftCacheError::StorageUnavailable)?;
            let updated_at: i64 = row
                .try_get("updated_at_unix_ms")
                .map_err(|_| DraftCacheError::StorageUnavailable)?;
            Ok(DraftRecord {
                project_id: ProjectId(
                    Uuid::parse_str(&project_id)
                        .map_err(|_| DraftCacheError::StorageUnavailable)?,
                ),
                session_id: SessionId(
                    Uuid::parse_str(&session_id)
                        .map_err(|_| DraftCacheError::StorageUnavailable)?,
                ),
                command_id: CommandId(
                    Uuid::parse_str(&command_id)
                        .map_err(|_| DraftCacheError::StorageUnavailable)?,
                ),
                text: row
                    .try_get("text")
                    .map_err(|_| DraftCacheError::StorageUnavailable)?,
                revision: u64::try_from(
                    row.try_get::<i64, _>("revision")
                        .map_err(|_| DraftCacheError::StorageUnavailable)?,
                )
                .map_err(|_| DraftCacheError::StorageUnavailable)?,
                updated_at_unix_ms: u64::try_from(updated_at)
                    .map_err(|_| DraftCacheError::StorageUnavailable)?,
            })
        })
        .transpose()
    }

    async fn discard(
        &self,
        session_id: SessionId,
        command_id: CommandId,
    ) -> Result<bool, DraftCacheError> {
        let session_id = session_id.0.to_string();
        let command_id = command_id.0.to_string();
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|_| DraftCacheError::StorageUnavailable)?;
        let existing = sqlx::query_scalar::<_, String>(
            "SELECT command_id FROM client_drafts WHERE session_id = ?",
        )
        .bind(&session_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| DraftCacheError::StorageUnavailable)?;
        if existing.as_deref().is_some_and(|value| value != command_id) {
            return Err(DraftCacheError::StableCommandMismatch);
        }
        let tombstone_owner = sqlx::query_scalar::<_, String>(
            "SELECT session_id FROM discarded_draft_commands WHERE command_id = ?",
        )
        .bind(&command_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| DraftCacheError::StorageUnavailable)?;
        if tombstone_owner
            .as_deref()
            .is_some_and(|value| value != session_id)
        {
            return Err(DraftCacheError::StableCommandMismatch);
        }
        let deleted = sqlx::query("DELETE FROM client_drafts WHERE session_id = ?")
            .bind(&session_id)
            .execute(&mut *transaction)
            .await
            .map_err(|_| DraftCacheError::StorageUnavailable)?;
        sqlx::query(
            "INSERT INTO discarded_draft_commands(command_id, session_id, discarded_at_unix_ms) \
             VALUES (?, ?, ?) ON CONFLICT(command_id) DO NOTHING",
        )
        .bind(&command_id)
        .bind(&session_id)
        .bind(i64::try_from(unix_time_ms()).map_err(|_| DraftCacheError::StorageUnavailable)?)
        .execute(&mut *transaction)
        .await
        .map_err(|_| DraftCacheError::StorageUnavailable)?;
        transaction
            .commit()
            .await
            .map_err(|_| DraftCacheError::StorageUnavailable)?;
        Ok(deleted.rows_affected() != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dennett_contracts::TurnId;
    use dennett_memory_core::session::{
        SafeSessionError, SessionJournal, SessionResult, SessionTurnOutcome, SessionTurnState,
    };
    use std::sync::Arc;
    use tempfile::TempDir;

    fn database_path(temp: &TempDir) -> std::path::PathBuf {
        temp.path().join("control.sqlite")
    }

    fn pending(
        session_id: SessionId,
        command_id: Option<CommandId>,
        body: SessionEventBody,
        time: u64,
    ) -> PendingSessionEvent {
        PendingSessionEvent {
            event_id: SessionEventId::new(),
            session_id,
            command_id,
            body,
            committed_at_unix_ms: time,
        }
    }

    async fn persist_terminal_session(
        journal: &SessionJournal,
        state: SessionTurnState,
        time: u64,
    ) -> SessionId {
        let session_id = SessionId::new();
        journal
            .append(pending(
                session_id,
                Some(CommandId::new()),
                SessionEventBody::SessionCreated {
                    project_id: ProjectId::new(),
                    title: format!("{state:?}"),
                },
                time,
            ))
            .await
            .expect("create durable session");
        let turn_command = CommandId::new();
        let agent_turn_id = TurnId::new();
        journal
            .append(pending(
                session_id,
                Some(turn_command),
                SessionEventBody::TurnAccepted {
                    user_turn_id: TurnId::new(),
                    agent_turn_id,
                    command_id: turn_command,
                    text: "persist me".to_owned(),
                },
                time + 1,
            ))
            .await
            .expect("persist accepted turn");
        journal
            .append(pending(
                session_id,
                None,
                SessionEventBody::AgentTextAppended {
                    turn_id: agent_turn_id,
                    text: "partial".to_owned(),
                },
                time + 2,
            ))
            .await
            .expect("persist partial text");
        let outcome = match state {
            SessionTurnState::Completed => Some(SessionTurnOutcome::Result(SessionResult {
                summary: "complete".to_owned(),
                partial: false,
                artifact_handles: Vec::new(),
                evidence_handles: Vec::new(),
            })),
            SessionTurnState::Cancelled | SessionTurnState::TimedOut => {
                Some(SessionTurnOutcome::Result(SessionResult {
                    summary: "partial".to_owned(),
                    partial: true,
                    artifact_handles: Vec::new(),
                    evidence_handles: Vec::new(),
                }))
            }
            SessionTurnState::Failed => Some(SessionTurnOutcome::Error(SafeSessionError {
                code: "provider_failed".to_owned(),
                message_key: "session.provider_failed".to_owned(),
                details_handle: None,
            })),
            SessionTurnState::Accepted | SessionTurnState::Streaming => {
                panic!("test requires terminal state")
            }
        };
        journal
            .append(pending(
                session_id,
                None,
                SessionEventBody::TurnFinished {
                    turn_id: agent_turn_id,
                    state,
                    outcome,
                },
                time + 3,
            ))
            .await
            .expect("persist terminal outcome");
        session_id
    }

    #[tokio::test]
    async fn test_project_session_restore_001_restores_all_terminal_states_after_reopen() {
        let temp = TempDir::new().expect("temporary directory");
        let path = database_path(&temp);
        let store = SqliteControlStore::open(&path).await.expect("open store");
        assert_eq!(
            store.schema_version().await.expect("schema version"),
            CONTROL_SCHEMA_VERSION
        );
        assert_eq!(store.journal_mode().await.expect("journal mode"), "wal");
        assert_eq!(
            sqlx::query_scalar::<_, i64>("PRAGMA synchronous")
                .fetch_one(&store.pool)
                .await
                .expect("synchronous mode"),
            2
        );
        let journal = SessionJournal::new(Arc::new(store.clone()));
        let states = [
            SessionTurnState::Completed,
            SessionTurnState::Cancelled,
            SessionTurnState::TimedOut,
            SessionTurnState::Failed,
        ];
        let mut expected = Vec::new();
        for (index, state) in states.into_iter().enumerate() {
            expected.push((
                persist_terminal_session(&journal, state, 100 + index as u64 * 10).await,
                state,
            ));
        }
        drop(journal);
        store.close().await;

        let reopened = SqliteControlStore::open(&path)
            .await
            .expect("reopen durable store");
        let restored = SessionJournal::new(Arc::new(reopened.clone()));
        for (session_id, state) in expected {
            let snapshot = restored.restore(session_id).await.expect("restore session");
            assert_eq!(snapshot.session.revision, 4);
            assert_eq!(snapshot.turns.last().expect("agent turn").state, state);
            assert_eq!(snapshot.turns.last().expect("agent turn").text, "partial");
        }
        reopened.verify_integrity().await.expect("integrity check");
    }

    #[tokio::test]
    async fn revision_conflict_rolls_back_without_creating_a_session_head() {
        let temp = TempDir::new().expect("temporary directory");
        let store = SqliteControlStore::open(database_path(&temp))
            .await
            .expect("open store");
        let session_id = SessionId::new();
        let result = store
            .append(
                1,
                pending(
                    session_id,
                    Some(CommandId::new()),
                    SessionEventBody::SessionCreated {
                        project_id: ProjectId::new(),
                        title: "conflict".to_owned(),
                    },
                    1,
                ),
            )
            .await;
        assert_eq!(
            result,
            Err(SessionJournalError::RevisionConflict {
                expected: 1,
                actual: 0,
            })
        );
        assert!(
            store
                .list_session_ids()
                .await
                .expect("list sessions")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn failed_event_insert_rolls_back_the_session_head_update() {
        let temp = TempDir::new().expect("temporary directory");
        let store = SqliteControlStore::open(database_path(&temp))
            .await
            .expect("open store");
        sqlx::query(
            "CREATE TRIGGER reject_session_event BEFORE INSERT ON session_events \
             BEGIN SELECT RAISE(ABORT, 'injected failure'); END",
        )
        .execute(&store.pool)
        .await
        .expect("install failure trigger");
        let session_id = SessionId::new();
        let result = store
            .append(
                0,
                pending(
                    session_id,
                    Some(CommandId::new()),
                    SessionEventBody::SessionCreated {
                        project_id: ProjectId::new(),
                        title: "rollback".to_owned(),
                    },
                    1,
                ),
            )
            .await;
        assert_eq!(result, Err(SessionJournalError::StorageUnavailable));
        assert!(
            store
                .list_session_ids()
                .await
                .expect("list sessions")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn corrupted_event_stops_reopen_instead_of_dropping_history() {
        let temp = TempDir::new().expect("temporary directory");
        let path = database_path(&temp);
        let store = SqliteControlStore::open(&path).await.expect("open store");
        let journal = SessionJournal::new(Arc::new(store.clone()));
        let session_id = SessionId::new();
        journal
            .append(pending(
                session_id,
                Some(CommandId::new()),
                SessionEventBody::SessionCreated {
                    project_id: ProjectId::new(),
                    title: "integrity".to_owned(),
                },
                1,
            ))
            .await
            .expect("persist event");
        sqlx::query("UPDATE session_events SET body_json = '{}' WHERE session_id = ?")
            .bind(session_id.0.to_string())
            .execute(&store.pool)
            .await
            .expect("inject corruption");
        drop(journal);
        store.close().await;

        let reopened = SqliteControlStore::open(&path).await;
        assert!(matches!(
            reopened,
            Err(SessionJournalError::IntegrityFailure(_))
        ));
    }

    #[tokio::test]
    async fn unsupported_event_payload_version_is_not_silently_reinterpreted() {
        let temp = TempDir::new().expect("temporary directory");
        let store = SqliteControlStore::open(database_path(&temp))
            .await
            .expect("open store");
        let journal = SessionJournal::new(Arc::new(store.clone()));
        let session_id = SessionId::new();
        let event = journal
            .append(pending(
                session_id,
                Some(CommandId::new()),
                SessionEventBody::SessionCreated {
                    project_id: ProjectId::new(),
                    title: "future payload".to_owned(),
                },
                1,
            ))
            .await
            .expect("persist event")
            .event;
        let body_json = serde_json::to_string(&event.body).expect("serialize event");
        let future_version = SESSION_EVENT_PAYLOAD_VERSION + 1;
        let checksum = stored_event_hash(
            &event.event_id.0.to_string(),
            &session_id.0.to_string(),
            event.revision,
            event.command_id.map(|id| id.0.to_string()).as_deref(),
            future_version,
            &body_json,
            event.committed_at_unix_ms,
        )
        .expect("hash future payload");
        sqlx::query(
            "UPDATE session_events SET payload_version = ?, event_sha256 = ? WHERE event_id = ?",
        )
        .bind(i64::from(future_version))
        .bind(checksum.as_slice())
        .bind(event.event_id.0.to_string())
        .execute(&store.pool)
        .await
        .expect("inject future payload");

        assert_eq!(
            journal.restore(session_id).await,
            Err(SessionJournalError::UnsupportedEventPayloadVersion {
                found: future_version,
                supported: SESSION_EVENT_PAYLOAD_VERSION,
            })
        );
    }

    #[tokio::test]
    async fn future_schema_is_rejected_without_deleting_existing_data() {
        let temp = TempDir::new().expect("temporary directory");
        let path = database_path(&temp);
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        let raw = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("create future database");
        sqlx::query("CREATE TABLE sentinel(value TEXT NOT NULL)")
            .execute(&raw)
            .await
            .expect("create sentinel");
        sqlx::query("INSERT INTO sentinel(value) VALUES ('keep')")
            .execute(&raw)
            .await
            .expect("insert sentinel");
        sqlx::query("PRAGMA user_version = 99")
            .execute(&raw)
            .await
            .expect("mark future schema");
        raw.close().await;

        assert!(matches!(
            SqliteControlStore::open(&path).await,
            Err(SessionJournalError::UnsupportedSchemaVersion {
                found: 99,
                supported: CONTROL_SCHEMA_VERSION,
            })
        ));
        let raw = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&path)
                    .create_if_missing(false),
            )
            .await
            .expect("reopen future database");
        let sentinel = sqlx::query_scalar::<_, String>("SELECT value FROM sentinel")
            .fetch_one(&raw)
            .await
            .expect("read sentinel");
        assert_eq!(sentinel, "keep");
    }

    #[tokio::test]
    async fn migration_checksum_mismatch_stops_startup() {
        let temp = TempDir::new().expect("temporary directory");
        let path = database_path(&temp);
        let store = SqliteControlStore::open(&path).await.expect("open store");
        sqlx::query("UPDATE _sqlx_migrations SET checksum = X'00' WHERE version = 1")
            .execute(&store.pool)
            .await
            .expect("corrupt migration checksum");
        store.close().await;
        assert_eq!(
            SqliteControlStore::open(&path).await.err(),
            Some(SessionJournalError::MigrationFailure)
        );
    }

    #[test]
    fn crash_writer_helper() {
        let Some(path) = std::env::var_os("DENNETT_TEST_CRASH_DB") else {
            return;
        };
        let runtime = tokio::runtime::Runtime::new().expect("child runtime");
        runtime.block_on(async move {
            let store = SqliteControlStore::open(path).await.expect("child open");
            let journal = SessionJournal::new(Arc::new(store));
            let session_id = SessionId(
                Uuid::parse_str(&std::env::var("DENNETT_TEST_CRASH_SESSION").expect("session id"))
                    .expect("valid session id"),
            );
            journal
                .append(pending(
                    session_id,
                    Some(CommandId::new()),
                    SessionEventBody::SessionCreated {
                        project_id: ProjectId::new(),
                        title: "crash committed".to_owned(),
                    },
                    1,
                ))
                .await
                .expect("commit before crash");
        });
        std::process::exit(0);
    }

    #[tokio::test]
    async fn committed_wal_event_restores_after_process_exit_without_cleanup() {
        let temp = TempDir::new().expect("temporary directory");
        let path = database_path(&temp);
        let session_id = SessionId::new();
        let status =
            std::process::Command::new(std::env::current_exe().expect("current test executable"))
                .arg("--exact")
                .arg("tests::crash_writer_helper")
                .arg("--nocapture")
                .env("DENNETT_TEST_CRASH_DB", &path)
                .env("DENNETT_TEST_CRASH_SESSION", session_id.0.to_string())
                .status()
                .expect("run crash writer");
        assert!(status.success());

        let reopened = SqliteControlStore::open(&path)
            .await
            .expect("recover after process exit");
        let snapshot = SessionJournal::new(Arc::new(reopened))
            .restore(session_id)
            .await
            .expect("restore committed session");
        assert_eq!(snapshot.session.title, "crash committed");
        assert_eq!(snapshot.session.revision, 1);
    }

    #[tokio::test]
    async fn test_desktop_draft_recovery_001_restores_only_noncanonical_draft_state() {
        let temp = TempDir::new().expect("temporary directory");
        let path = database_path(&temp);
        let store = SqliteControlStore::open(&path).await.expect("open store");
        let draft = DraftRecord {
            project_id: ProjectId::new(),
            session_id: SessionId::new(),
            command_id: CommandId::new(),
            text: "unsent".to_owned(),
            revision: 1,
            updated_at_unix_ms: 42,
        };
        store.save(draft.clone()).await.expect("save draft");
        assert!(
            store
                .list_session_ids()
                .await
                .expect("list sessions")
                .is_empty()
        );
        store.close().await;

        let reopened = SqliteControlStore::open(&path).await.expect("reopen store");
        assert_eq!(
            reopened
                .load(draft.session_id)
                .await
                .expect("restore draft"),
            Some(draft.clone())
        );
        assert!(
            reopened
                .list_session_ids()
                .await
                .expect("list sessions")
                .is_empty()
        );
        reopened
            .discard(draft.session_id, draft.command_id)
            .await
            .expect("discard draft");
        assert_eq!(
            reopened
                .save(DraftRecord {
                    text: "late stale save".to_owned(),
                    revision: 2,
                    ..draft.clone()
                })
                .await
                .expect("classify late save"),
            DraftCacheSaveOutcome::Discarded
        );
        assert_eq!(
            reopened
                .load(draft.session_id)
                .await
                .expect("load discarded"),
            None
        );
    }

    #[tokio::test]
    async fn command_admission_survives_restart_and_rejects_changed_intent() {
        let temp = TempDir::new().expect("temporary directory");
        let path = database_path(&temp);
        let command_id = CommandId::new();
        let request = CommandAdmissionRequest {
            command_id,
            idempotency_key: "stable-key".to_owned(),
            correlation_id: "first-correlation".to_owned(),
            operation_kind: "send_turn".to_owned(),
            intent_hash: [7; 32],
            admitted_at_unix_ms: 42,
        };
        let store = SqliteControlStore::open(&path).await.expect("open store");
        let first = store.admit(request.clone()).await.expect("admit command");
        store.close().await;

        let reopened = SqliteControlStore::open(&path).await.expect("reopen store");
        let replay = reopened
            .admit(CommandAdmissionRequest {
                correlation_id: "retry-correlation".to_owned(),
                ..request.clone()
            })
            .await
            .expect("replay command");
        assert_eq!(first.accepted_revision, replay.accepted_revision);
        assert_eq!(replay.correlation_id, "retry-correlation");
        assert_eq!(
            reopened
                .admit(CommandAdmissionRequest {
                    intent_hash: [8; 32],
                    ..request
                })
                .await,
            Err(CommandAdmissionError::IdempotencyConflict)
        );
    }

    #[tokio::test]
    async fn opaque_runtime_continuation_survives_restart() {
        let temp = TempDir::new().expect("temporary directory");
        let path = database_path(&temp);
        let continuation =
            OpaqueContinuation::new("runtime.adapter", "opaque-thread").expect("continuation");
        let store = SqliteControlStore::open(&path).await.expect("open store");
        dennett_agent_core::RuntimeContinuationPort::save(&store, "session-a", &continuation)
            .await
            .expect("save continuation");
        store.close().await;

        let reopened = SqliteControlStore::open(&path).await.expect("reopen store");
        assert_eq!(
            dennett_agent_core::RuntimeContinuationPort::load(&reopened, "session-a")
                .await
                .expect("load continuation"),
            Some(continuation)
        );
    }
}
