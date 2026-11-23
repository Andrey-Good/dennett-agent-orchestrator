//! Canonical PostgreSQL-backed Memory Fabric domain service (`MemoryService`).
//!
//! Enforces scope and capability authorization against a verified [`AuthPrincipal`],
//! executes atomic PostgreSQL transactions with `stream_counters`-backed monotonic
//! event ordering, handles optimistic revision locking and concurrent idempotency races,
//! and redacts all PostgreSQL credentials from every error path.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Duration;

use deadpool_postgres::{GenericClient, Manager, ManagerConfig, Pool, RecyclingMethod};
use dennett_contracts::{
    AuthPrincipal, CorrectNoteRequest, CorrelationId, CreateNoteRequest, DennettError,
    EventEvidenceLink, EvidenceAnchor, EvidenceId, EvidenceLinkId, EvidenceLinkInput,
    EvidenceObject, EvidenceRelationship, IdempotencyKey, IngestEvidenceRequest,
    IngestEvidenceResponse, MemoryCapability, MemoryEvent, MemoryEventId, MemoryEventKind,
    MemoryNote, MemoryNoteId, MemoryScope, NoteHistoryResponse, NoteMutationResponse, NoteRevision,
    NoteRevisionId, PrincipalId, ProfileMode, SearchNotesRequest, SearchNotesResponse,
    ServiceHealth, ServiceKind, Sha256Hex, TimestampMs,
};
use serde_json::Value as JsonValue;
use tokio::sync::Mutex;
use tokio::time;
use tokio_postgres::error::SqlState;
use tokio_postgres::{NoTls, Row};
use uuid::Uuid;

use crate::fingerprint::{
    fingerprint_correct_note, fingerprint_create_note, fingerprint_ingest_evidence, sha256_hex,
};
use crate::migrations::{query_schema_version, run_migrations};
use crate::pg_target::ValidatedPgTarget;

/// Current implementation milestone label advertised in [`ServiceHealth`].
pub const MEMORYD_STAGE_LABEL: &str = "D1";
/// Default maximum PostgreSQL connection pool size for standalone `dennett-memoryd`.
pub const DEFAULT_POOL_MAX_SIZE: usize = 16;
/// Default timeout applied to PostgreSQL operations inside [`MemoryService`].
pub const DEFAULT_PG_OP_TIMEOUT: Duration = Duration::from_secs(10);
/// Shorter timeout applied to `/healthz` and `--health-once` probes.
pub const DEFAULT_PG_PROBE_TIMEOUT: Duration = Duration::from_secs(3);

const IDEMPOTENCY_UNIQUE_CONSTRAINT: &str = "memory_events_scope_idempotency_uniq";

struct ServiceInner {
    pg_target: ValidatedPgTarget,
    pool: Pool,
    migrated_version: Mutex<Option<i64>>,
    op_timeout: Duration,
    probe_timeout: Duration,
}

/// Canonical Memory Fabric domain service operating directly against PostgreSQL.
///
/// Cheap to clone via internal `Arc`. Must ONLY be instantiated inside `services/memoryd`
/// and isolated integration tests — never inside Head, clients, or MCP adapters.
#[derive(Clone)]
pub struct MemoryService {
    inner: Arc<ServiceInner>,
}

impl MemoryService {
    /// Builds a new [`MemoryService`] targeting a validated PostgreSQL instance.
    ///
    /// Does not require PostgreSQL to be immediately reachable so `dennett-memoryd` can
    /// start up and report honest `503 Service Unavailable` health/API errors during outages.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::Internal`] if connection pool initialization fails.
    pub fn new(pg_target: ValidatedPgTarget) -> Result<Self, DennettError> {
        let mgr_cfg = ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        };
        let manager = Manager::from_config(pg_target.config().clone(), NoTls, mgr_cfg);
        let pool = Pool::builder(manager)
            .max_size(DEFAULT_POOL_MAX_SIZE)
            .build()
            .map_err(|err| DennettError::Internal {
                detail: format!("failed to construct PostgreSQL pool: {err}"),
            })?;

        Ok(Self {
            inner: Arc::new(ServiceInner {
                pg_target,
                pool,
                migrated_version: Mutex::new(None),
                op_timeout: DEFAULT_PG_OP_TIMEOUT,
                probe_timeout: DEFAULT_PG_PROBE_TIMEOUT,
            }),
        })
    }

    /// Returns the validated PostgreSQL target descriptor.
    #[must_use]
    pub fn pg_target(&self) -> &ValidatedPgTarget {
        &self.inner.pg_target
    }

    /// Ensures all Memory Fabric PostgreSQL schema migrations are applied idempotently.
    ///
    /// Caches the confirmed schema version after the first successful migration pass; if
    /// PostgreSQL is unreachable, leaves the cached version unset so subsequent calls retry.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::DatabaseUnavailable`] or [`DennettError::SafetyGuardRejected`]
    /// if PostgreSQL cannot be reached or schema validation fails.
    pub async fn ensure_migrated(&self) -> Result<i64, DennettError> {
        let mut guard = self.inner.migrated_version.lock().await;
        if let Some(version) = *guard {
            return Ok(version);
        }

        let mut conn = self.get_conn(self.inner.op_timeout).await?;
        let version = time::timeout(
            self.inner.op_timeout,
            run_migrations(
                &mut conn,
                Some(self.inner.pg_target.raw_url()),
                self.inner.pg_target.parsed_password(),
            ),
        )
        .await
        .map_err(|_| DennettError::DatabaseUnavailable {
            detail: String::from("PostgreSQL migration timed out"),
        })??;

        *guard = Some(version);
        Ok(version)
    }

    /// Performs a strictly read-only live PostgreSQL health probe (`SELECT 1` + schema version).
    ///
    /// Safe against read-only standbys: never mutates tables or runs DDL.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::DatabaseUnavailable`] with credentials stripped if PostgreSQL
    /// is unreachable or times out.
    pub async fn health_check(
        &self,
        profile_mode: ProfileMode,
        correlation_id: CorrelationId,
    ) -> Result<ServiceHealth, DennettError> {
        let fut = async {
            let conn = self.get_conn(self.inner.probe_timeout).await?;
            let row = conn
                .query_one(
                    "SELECT 1::int4, current_user::text, current_database()::text",
                    &[],
                )
                .await
                .map_err(|err| self.map_pg_err(&err))?;

            let check: i32 = row.get(0);
            let role: String = row.get(1);
            let database: String = row.get(2);
            if check != 1 {
                return Err(DennettError::DatabaseUnavailable {
                    detail: String::from("unexpected SELECT 1 result from PostgreSQL"),
                });
            }

            let schema_version = query_schema_version(
                &conn,
                Some(self.inner.pg_target.raw_url()),
                self.inner.pg_target.parsed_password(),
            )
            .await?;

            Ok(ServiceHealth {
                service: ServiceKind::Memoryd,
                stage: String::from(MEMORYD_STAGE_LABEL),
                profile_mode,
                memory_fabric_implemented: true,
                postgres_connected: true,
                postgres_role: Some(role),
                postgres_database: Some(database),
                schema_version,
                capabilities: d1_memoryd_capabilities(),
                correlation_id,
            })
        };

        time::timeout(self.inner.probe_timeout, fut)
            .await
            .map_err(|_| DennettError::DatabaseUnavailable {
                detail: String::from("PostgreSQL health probe timed out"),
            })?
    }

    /// Ingests an immutable inline evidence object into a scoped memory domain.
    ///
    /// Repeating the request with the same `(scope, idempotency_key)` and identical payload
    /// returns the original object with `idempotent_replay = true`; reusing the key with a
    /// different payload returns [`DennettError::IdempotencyMismatch`].
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::ForbiddenScope`],
    /// [`DennettError::IdempotencyMismatch`], or [`DennettError::DatabaseUnavailable`].
    pub async fn ingest_evidence(
        &self,
        principal: &AuthPrincipal,
        correlation_id: &CorrelationId,
        req: IngestEvidenceRequest,
    ) -> Result<IngestEvidenceResponse, DennettError> {
        req.validate()?;
        require_scope_capability(principal, &req.scope, MemoryCapability::Write)?;
        self.ensure_migrated().await?;

        let content_sha = sha256_hex(req.content.as_bytes());
        let byte_len_i64 =
            i64::try_from(req.content.len()).map_err(|_| DennettError::InvalidInput {
                field: String::from("content"),
                reason: String::from("content length exceeds i64 range"),
            })?;
        let fingerprint = fingerprint_ingest_evidence(&req);
        let metadata_json =
            serde_json::to_value(&req.metadata).map_err(|err| DennettError::Internal {
                detail: format!("failed to serialize evidence metadata: {err}"),
            })?;

        let op_fut = async {
            let mut conn = self.get_conn(self.inner.op_timeout).await?;
            let tx = conn
                .transaction()
                .await
                .map_err(|err| self.map_pg_err(&err))?;

            if let Some(existing) =
                lookup_idempotency(&tx, &req.scope, &req.idempotency_key, self).await?
            {
                return self
                    .resolve_evidence_replay(&tx, existing, &req.idempotency_key, &fingerprint)
                    .await;
            }

            let evidence_id = EvidenceId::new();
            let event_id = MemoryEventId::new();
            let observed_ms = req.observed_at.map(TimestampMs::as_millis);

            tx.execute(
                "INSERT INTO evidence_objects (
                    evidence_id, scope, sha256, media_type, byte_len, content,
                    source_uri, observed_at, ingested_by, metadata
                 ) VALUES (
                    $1, $2, $3, $4, $5, $6, $7,
                    TIMESTAMPTZ '1970-01-01 00:00:00+00' + ($8::bigint * INTERVAL '1 millisecond'),
                    $9, $10
                 )",
                &[
                    &evidence_id.as_uuid(),
                    &req.scope.as_str(),
                    &content_sha.as_str(),
                    &req.media_type,
                    &byte_len_i64,
                    &req.content,
                    &req.source_uri,
                    &observed_ms,
                    &principal.principal_id.as_str(),
                    &metadata_json,
                ],
            )
            .await
            .map_err(|err| self.map_pg_err(&err))?;

            let event_inserted = insert_memory_event(
                &tx,
                NewMemoryEvent {
                    event_id,
                    scope: &req.scope,
                    kind: MemoryEventKind::EvidenceIngested,
                    principal_id: &principal.principal_id,
                    correlation_id,
                    idempotency_key: &req.idempotency_key,
                    fingerprint: &fingerprint,
                    target_entity_id: evidence_id.as_uuid(),
                    target_revision: None,
                    observed_ms,
                },
                self,
            )
            .await?;

            if !event_inserted {
                let _ = tx.rollback().await;
                return self
                    .recover_evidence_idempotency_race(
                        &conn,
                        &req.scope,
                        &req.idempotency_key,
                        &fingerprint,
                    )
                    .await;
            }

            let evidence = fetch_evidence_row(&tx, evidence_id, self).await?;
            tx.commit().await.map_err(|err| self.map_pg_err(&err))?;

            Ok(IngestEvidenceResponse {
                evidence,
                event_id,
                idempotent_replay: false,
            })
        };

        time::timeout(self.inner.op_timeout, op_fut)
            .await
            .map_err(|_| DennettError::Timeout {
                operation: String::from("memory_service::ingest_evidence"),
            })?
    }

    /// Fetches an immutable [`EvidenceObject`] by identifier after verifying read scope access.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`], [`DennettError::ForbiddenScope`], or
    /// [`DennettError::DatabaseUnavailable`].
    pub async fn get_evidence(
        &self,
        principal: &AuthPrincipal,
        evidence_id: EvidenceId,
    ) -> Result<EvidenceObject, DennettError> {
        self.ensure_migrated().await?;

        let op_fut = async {
            let conn = self.get_conn(self.inner.op_timeout).await?;
            let evidence = fetch_evidence_row(&conn, evidence_id, self).await?;
            require_resource_scope_capability(principal, &evidence.scope, MemoryCapability::Read)?;
            Ok(evidence)
        };

        time::timeout(self.inner.op_timeout, op_fut)
            .await
            .map_err(|_| DennettError::Timeout {
                operation: String::from("memory_service::get_evidence"),
            })?
    }

    /// Creates a new revision-1 [`MemoryNote`] together with its append-only event and evidence links.
    ///
    /// Supports `ordinal: None` and multiple distinct anchors pointing at the same `evidence_id`
    /// (Architecture 81 §3.3 / V19). Rejects links referencing missing or cross-scope evidence.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::ForbiddenScope`],
    /// [`DennettError::NotFound`], [`DennettError::IdempotencyMismatch`], or
    /// [`DennettError::DatabaseUnavailable`].
    pub async fn create_note(
        &self,
        principal: &AuthPrincipal,
        correlation_id: &CorrelationId,
        req: CreateNoteRequest,
    ) -> Result<NoteMutationResponse, DennettError> {
        req.validate()?;
        require_scope_capability(principal, &req.scope, MemoryCapability::Write)?;
        self.ensure_migrated().await?;

        let fingerprint = fingerprint_create_note(&req);
        let tags_json = serde_json::to_value(&req.tags).map_err(|err| DennettError::Internal {
            detail: format!("failed to serialize note tags: {err}"),
        })?;

        let op_fut = self.create_note_tx(principal, correlation_id, &req, &fingerprint, &tags_json);

        time::timeout(self.inner.op_timeout, op_fut)
            .await
            .map_err(|_| DennettError::Timeout {
                operation: String::from("memory_service::create_note"),
            })?
    }

    async fn create_note_tx(
        &self,
        principal: &AuthPrincipal,
        correlation_id: &CorrelationId,
        req: &CreateNoteRequest,
        fingerprint: &Sha256Hex,
        tags_json: &JsonValue,
    ) -> Result<NoteMutationResponse, DennettError> {
        let mut conn = self.get_conn(self.inner.op_timeout).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| self.map_pg_err(&err))?;

        if let Some(existing) =
            lookup_idempotency(&tx, &req.scope, &req.idempotency_key, self).await?
        {
            return self
                .resolve_note_replay(
                    &tx,
                    existing,
                    MemoryEventKind::NoteCreated,
                    &req.idempotency_key,
                    fingerprint,
                )
                .await;
        }

        verify_evidence_links_in_scope(&tx, &req.scope, &req.evidence_links, self).await?;

        let note_id = MemoryNoteId::new();
        let revision_id = NoteRevisionId::new();
        let event_id = MemoryEventId::new();

        let event_inserted = insert_memory_event(
            &tx,
            NewMemoryEvent {
                event_id,
                scope: &req.scope,
                kind: MemoryEventKind::NoteCreated,
                principal_id: &principal.principal_id,
                correlation_id,
                idempotency_key: &req.idempotency_key,
                fingerprint,
                target_entity_id: note_id.as_uuid(),
                target_revision: Some(1),
                observed_ms: req.observed_at.map(TimestampMs::as_millis),
            },
            self,
        )
        .await?;

        if !event_inserted {
            let _ = tx.rollback().await;
            return self
                .recover_note_idempotency_race(
                    &conn,
                    &req.scope,
                    MemoryEventKind::NoteCreated,
                    &req.idempotency_key,
                    fingerprint,
                )
                .await;
        }

        insert_evidence_links(&tx, event_id, 1, &req.evidence_links, self).await?;

        tx.execute(
            "INSERT INTO memory_notes (
                note_id, scope, current_revision, current_revision_id,
                title, body, tags, rationale, last_event_id,
                recorded_by, author_label
             ) VALUES ($1, $2, 1, $3, $4, $5, $6, NULL, $7, $8, $9)",
            &[
                &note_id.as_uuid(),
                &req.scope.as_str(),
                &revision_id.as_uuid(),
                &req.title,
                &req.body,
                &tags_json,
                &event_id.as_uuid(),
                &principal.principal_id.as_str(),
                &req.author_label,
            ],
        )
        .await
        .map_err(|err| self.map_pg_err(&err))?;

        insert_note_revision_row(
            &tx,
            NewNoteRevision {
                revision_id,
                note_id,
                revision: 1,
                title: &req.title,
                body: &req.body,
                tags_json,
                rationale: None,
                event_id,
                recorded_by: &principal.principal_id,
                author_label: req.author_label.as_deref(),
            },
            self,
        )
        .await?;

        let note = fetch_current_note_row(&tx, note_id, self).await?;
        tx.commit().await.map_err(|err| self.map_pg_err(&err))?;

        Ok(NoteMutationResponse {
            note,
            event_id,
            idempotent_replay: false,
        })
    }

    /// Reads the current projection of a [`MemoryNote`] after verifying read scope access.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`], [`DennettError::ForbiddenScope`], or
    /// [`DennettError::DatabaseUnavailable`].
    pub async fn get_note_current(
        &self,
        principal: &AuthPrincipal,
        note_id: MemoryNoteId,
    ) -> Result<MemoryNote, DennettError> {
        self.ensure_migrated().await?;

        let op_fut = async {
            let conn = self.get_conn(self.inner.op_timeout).await?;
            let note = fetch_current_note_row(&conn, note_id, self).await?;
            require_resource_scope_capability(principal, &note.scope, MemoryCapability::Read)?;
            Ok(note)
        };

        time::timeout(self.inner.op_timeout, op_fut)
            .await
            .map_err(|_| DennettError::Timeout {
                operation: String::from("memory_service::get_note_current"),
            })?
    }

    /// Reads the complete immutable revision history of a [`MemoryNote`] in ascending revision order.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`], [`DennettError::ForbiddenScope`], or
    /// [`DennettError::DatabaseUnavailable`].
    pub async fn get_note_history(
        &self,
        principal: &AuthPrincipal,
        note_id: MemoryNoteId,
    ) -> Result<NoteHistoryResponse, DennettError> {
        self.ensure_migrated().await?;

        let op_fut = async {
            let conn = self.get_conn(self.inner.op_timeout).await?;
            let head_row = conn
                .query_opt(
                    "SELECT scope, current_revision FROM memory_notes WHERE note_id = $1",
                    &[&note_id.as_uuid()],
                )
                .await
                .map_err(|err| self.map_pg_err(&err))?;

            let Some(head) = head_row else {
                return Err(DennettError::NotFound {
                    resource: String::from("memory_note"),
                    id: note_id.to_string(),
                });
            };

            let scope = MemoryScope::try_new(head.get::<_, String>(0))?;
            let current_revision: i64 = head.get(1);
            require_resource_scope_capability(principal, &scope, MemoryCapability::Read)?;

            let rev_rows = conn
                .query(
                    "SELECT revision_id, note_id, revision, title, body, tags, rationale,
                            event_id, recorded_by, author_label,
                            TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint
                     FROM memory_note_revisions
                     WHERE note_id = $1
                     ORDER BY revision ASC",
                    &[&note_id.as_uuid()],
                )
                .await
                .map_err(|err| self.map_pg_err(&err))?;

            let event_uuids: Vec<Uuid> = rev_rows.iter().map(|r| r.get::<_, Uuid>(7)).collect();
            let mut links_by_event = fetch_links_for_events(&conn, &event_uuids, self).await?;

            let mut revisions = Vec::with_capacity(rev_rows.len());
            for row in rev_rows {
                let event_uuid: Uuid = row.get(7);
                let tags = decode_tags(row.get::<_, JsonValue>(5))?;
                revisions.push(NoteRevision {
                    revision_id: NoteRevisionId::from_uuid(row.get(0)),
                    note_id: MemoryNoteId::from_uuid(row.get(1)),
                    revision: row.get(2),
                    title: row.get(3),
                    body: row.get(4),
                    tags,
                    rationale: row.get(6),
                    event_id: MemoryEventId::from_uuid(event_uuid),
                    recorded_by: PrincipalId::try_new(row.get::<_, String>(8))?,
                    author_label: row.get(9),
                    created_at: TimestampMs::from_millis(row.get(10)),
                    evidence_links: links_by_event.remove(&event_uuid).unwrap_or_default(),
                });
            }

            Ok(NoteHistoryResponse {
                note_id,
                scope,
                current_revision,
                revisions,
            })
        };

        time::timeout(self.inner.op_timeout, op_fut)
            .await
            .map_err(|_| DennettError::Timeout {
                operation: String::from("memory_service::get_note_history"),
            })?
    }

    /// Appends a new immutable correction revision to a [`MemoryNote`] under optimistic locking.
    ///
    /// Preserves every prior revision intact in `memory_note_revisions`, verifies
    /// `expected_revision` against the locked `memory_notes` row, and checks idempotency first so
    /// network retries of an already-committed correction replay cleanly instead of failing with
    /// a spurious `409 Conflict`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::ForbiddenScope`],
    /// [`DennettError::NotFound`], [`DennettError::Conflict`],
    /// [`DennettError::IdempotencyMismatch`], or [`DennettError::DatabaseUnavailable`].
    pub async fn correct_note(
        &self,
        principal: &AuthPrincipal,
        correlation_id: &CorrelationId,
        req: CorrectNoteRequest,
    ) -> Result<NoteMutationResponse, DennettError> {
        req.validate()?;
        require_scope_capability(principal, &req.scope, MemoryCapability::Write)?;
        self.ensure_migrated().await?;

        let fingerprint = fingerprint_correct_note(&req);
        let tags_json = serde_json::to_value(&req.tags).map_err(|err| DennettError::Internal {
            detail: format!("failed to serialize note tags: {err}"),
        })?;

        let op_fut =
            self.correct_note_tx(principal, correlation_id, &req, &fingerprint, &tags_json);

        time::timeout(self.inner.op_timeout, op_fut)
            .await
            .map_err(|_| DennettError::Timeout {
                operation: String::from("memory_service::correct_note"),
            })?
    }

    async fn correct_note_tx(
        &self,
        principal: &AuthPrincipal,
        correlation_id: &CorrelationId,
        req: &CorrectNoteRequest,
        fingerprint: &Sha256Hex,
        tags_json: &JsonValue,
    ) -> Result<NoteMutationResponse, DennettError> {
        let mut conn = self.get_conn(self.inner.op_timeout).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| self.map_pg_err(&err))?;

        if let Some(replay) = self
            .try_replay_note_idempotency(
                &tx,
                &req.scope,
                MemoryEventKind::NoteCorrected,
                &req.idempotency_key,
                fingerprint,
            )
            .await?
        {
            return Ok(replay);
        }

        let (stored_scope, actual_revision) =
            lock_note_scope_and_revision(&tx, principal, req, self).await?;

        // Re-check idempotency AFTER acquiring `FOR UPDATE` so a concurrent retry with the same
        // `(scope, idempotency_key)` that blocked behind T1 observes T1's committed event instead
        // of failing with a spurious optimistic-revision 409 Conflict.
        if let Some(replay) = self
            .try_replay_note_idempotency(
                &tx,
                &stored_scope,
                MemoryEventKind::NoteCorrected,
                &req.idempotency_key,
                fingerprint,
            )
            .await?
        {
            return Ok(replay);
        }

        ensure_expected_revision(req.expected_revision, actual_revision)?;

        verify_evidence_links_in_scope(&tx, &stored_scope, &req.evidence_links, self).await?;

        let new_revision =
            actual_revision
                .checked_add(1)
                .ok_or_else(|| DennettError::Internal {
                    detail: String::from("note revision counter overflowed i64"),
                })?;
        let revision_id = NoteRevisionId::new();
        let event_id = MemoryEventId::new();

        let event_inserted = insert_memory_event(
            &tx,
            NewMemoryEvent {
                event_id,
                scope: &stored_scope,
                kind: MemoryEventKind::NoteCorrected,
                principal_id: &principal.principal_id,
                correlation_id,
                idempotency_key: &req.idempotency_key,
                fingerprint,
                target_entity_id: req.note_id.as_uuid(),
                target_revision: Some(new_revision),
                observed_ms: None,
            },
            self,
        )
        .await?;

        if !event_inserted {
            let _ = tx.rollback().await;
            return self
                .recover_note_idempotency_race(
                    &conn,
                    &stored_scope,
                    MemoryEventKind::NoteCorrected,
                    &req.idempotency_key,
                    fingerprint,
                )
                .await;
        }

        insert_evidence_links(&tx, event_id, new_revision, &req.evidence_links, self).await?;
        let rev = NewNoteRevision {
            revision_id,
            note_id: req.note_id,
            revision: new_revision,
            title: &req.title,
            body: &req.body,
            tags_json,
            rationale: Some(&req.rationale),
            event_id,
            recorded_by: &principal.principal_id,
            author_label: req.author_label.as_deref(),
        };
        persist_correction_revision(&tx, req, principal, rev, actual_revision, self).await?;

        let note = fetch_current_note_row(&tx, req.note_id, self).await?;
        tx.commit().await.map_err(|err| self.map_pg_err(&err))?;

        Ok(NoteMutationResponse {
            note,
            event_id,
            idempotent_replay: false,
        })
    }

    /// Searches or lists current [`MemoryNote`] projections strictly within authorized scopes.
    ///
    /// Pre-filters by authorized scope(s) in PostgreSQL before evaluating text predicates and
    /// re-verifies scope authorization on every returned record as defense-in-depth (Architecture 81 §5).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::ForbiddenScope`], or
    /// [`DennettError::DatabaseUnavailable`].
    pub async fn search_notes(
        &self,
        principal: &AuthPrincipal,
        req: SearchNotesRequest,
    ) -> Result<SearchNotesResponse, DennettError> {
        let limit = req.validated_limit()?;
        let allowed_scopes = resolve_search_scopes(principal, req.scope.as_ref())?;
        if allowed_scopes.is_empty() {
            return Ok(SearchNotesResponse {
                notes: Vec::new(),
                total_matched: 0,
                truncated: false,
            });
        }

        self.ensure_migrated().await?;
        let trimmed_query = req
            .query
            .as_deref()
            .map(str::trim)
            .filter(|q| !q.is_empty())
            .map(ToOwned::to_owned);
        let limit_i64 = i64::from(limit);

        let op_fut = async {
            let conn = self.get_conn(self.inner.op_timeout).await?;
            let rows = query_scoped_notes(
                &conn,
                &allowed_scopes,
                trimmed_query.as_deref(),
                limit_i64,
                self,
            )
            .await?;

            let total_matched: u64 = rows
                .first()
                .map_or(0, |r| u64::try_from(r.get::<_, i64>(13)).unwrap_or(0));

            let event_uuids: Vec<Uuid> = rows.iter().map(|r| r.get::<_, Uuid>(8)).collect();
            let mut links_by_event = fetch_links_for_events(&conn, &event_uuids, self).await?;

            let mut notes = Vec::with_capacity(rows.len());
            for row in rows {
                let event_uuid: Uuid = row.get(8);
                let links = links_by_event.remove(&event_uuid).unwrap_or_default();
                let note = decode_note_row(&row, links)?;
                // Defense-in-depth post-filter verification (Architecture 81 §5).
                require_scope_capability(principal, &note.scope, MemoryCapability::Read)?;
                notes.push(note);
            }

            let returned_count = u64::try_from(notes.len()).unwrap_or(u64::MAX);
            Ok(SearchNotesResponse {
                notes,
                total_matched,
                truncated: total_matched > returned_count,
            })
        };

        time::timeout(self.inner.op_timeout, op_fut)
            .await
            .map_err(|_| DennettError::Timeout {
                operation: String::from("memory_service::search_notes"),
            })?
    }

    /// Fetches an append-only [`MemoryEvent`] and all of its [`EventEvidenceLink`]s.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`], [`DennettError::ForbiddenScope`], or
    /// [`DennettError::DatabaseUnavailable`].
    pub async fn get_event(
        &self,
        principal: &AuthPrincipal,
        event_id: MemoryEventId,
    ) -> Result<MemoryEvent, DennettError> {
        self.ensure_migrated().await?;

        let op_fut = async {
            let conn = self.get_conn(self.inner.op_timeout).await?;
            let row = conn
                .query_opt(
                    "SELECT event_id, stream_id, stream_seq, scope, event_kind, principal_id,
                            correlation_id, idempotency_key, payload_fingerprint,
                            target_entity_id, target_revision,
                            TRUNC(EXTRACT(EPOCH FROM observed_at) * 1000)::bigint,
                            TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint
                     FROM memory_events
                     WHERE event_id = $1",
                    &[&event_id.as_uuid()],
                )
                .await
                .map_err(|err| self.map_pg_err(&err))?;

            let Some(row) = row else {
                return Err(DennettError::NotFound {
                    resource: String::from("memory_event"),
                    id: event_id.to_string(),
                });
            };

            let scope = MemoryScope::try_new(row.get::<_, String>(3))?;
            require_resource_scope_capability(principal, &scope, MemoryCapability::Read)?;

            let event_uuid: Uuid = row.get(0);
            let mut links_map = fetch_links_for_events(&conn, &[event_uuid], self).await?;
            let evidence_links = links_map.remove(&event_uuid).unwrap_or_default();

            Ok(MemoryEvent {
                event_id: MemoryEventId::from_uuid(event_uuid),
                stream_id: row.get(1),
                stream_seq: row.get(2),
                scope,
                kind: row.get::<_, String>(4).parse()?,
                principal_id: PrincipalId::try_new(row.get::<_, String>(5))?,
                correlation_id: CorrelationId::try_new(row.get::<_, String>(6))?,
                idempotency_key: IdempotencyKey::try_new(row.get::<_, String>(7))?,
                payload_fingerprint: Sha256Hex::try_new(row.get::<_, String>(8))?,
                target_entity_id: row.get(9),
                target_revision: row.get(10),
                observed_at: row.get::<_, Option<i64>>(11).map(TimestampMs::from_millis),
                created_at: TimestampMs::from_millis(row.get(12)),
                evidence_links,
            })
        };

        time::timeout(self.inner.op_timeout, op_fut)
            .await
            .map_err(|_| DennettError::Timeout {
                operation: String::from("memory_service::get_event"),
            })?
    }

    async fn get_conn(&self, timeout: Duration) -> Result<deadpool_postgres::Object, DennettError> {
        time::timeout(timeout, self.inner.pool.get())
            .await
            .map_err(|_| DennettError::DatabaseUnavailable {
                detail: String::from("timed out acquiring PostgreSQL connection"),
            })?
            .map_err(|err| DennettError::DatabaseUnavailable {
                detail: self.inner.pg_target.sanitize_error(&err.to_string()),
            })
    }

    fn map_pg_err(&self, err: &tokio_postgres::Error) -> DennettError {
        let safe = self.inner.pg_target.sanitize_error(&err.to_string());
        if let Some(db_err) = err.as_db_error() {
            let code = db_err.code().code();
            if code.starts_with("22") || code == SqlState::CHECK_VIOLATION.code() {
                return DennettError::InvalidInput {
                    field: String::from("payload"),
                    reason: safe,
                };
            }
        }
        DennettError::DatabaseUnavailable { detail: safe }
    }

    async fn resolve_evidence_replay<C: GenericClient>(
        &self,
        client: &C,
        existing: StoredIdempotencyRecord,
        key: &IdempotencyKey,
        incoming_fingerprint: &Sha256Hex,
    ) -> Result<IngestEvidenceResponse, DennettError> {
        if existing.kind != MemoryEventKind::EvidenceIngested
            || existing.payload_fingerprint != incoming_fingerprint.as_str()
        {
            return Err(DennettError::IdempotencyMismatch {
                idempotency_key: key.to_string(),
                reason: String::from(
                    "idempotency_key was already used with a different operation or payload",
                ),
            });
        }
        let evidence = fetch_evidence_row(
            client,
            EvidenceId::from_uuid(existing.target_entity_id),
            self,
        )
        .await?;
        Ok(IngestEvidenceResponse {
            evidence,
            event_id: existing.event_id,
            idempotent_replay: true,
        })
    }

    async fn recover_evidence_idempotency_race<C: GenericClient>(
        &self,
        client: &C,
        scope: &MemoryScope,
        key: &IdempotencyKey,
        incoming_fingerprint: &Sha256Hex,
    ) -> Result<IngestEvidenceResponse, DennettError> {
        let Some(existing) = lookup_idempotency(client, scope, key, self).await? else {
            return Err(DennettError::Conflict {
                resource: String::from("evidence"),
                expected_revision: None,
                actual_revision: None,
                reason: String::from("concurrent idempotency conflict; retry request"),
            });
        };
        self.resolve_evidence_replay(client, existing, key, incoming_fingerprint)
            .await
    }

    async fn try_replay_note_idempotency<C: GenericClient>(
        &self,
        client: &C,
        scope: &MemoryScope,
        expected_kind: MemoryEventKind,
        key: &IdempotencyKey,
        incoming_fingerprint: &Sha256Hex,
    ) -> Result<Option<NoteMutationResponse>, DennettError> {
        let Some(existing) = lookup_idempotency(client, scope, key, self).await? else {
            return Ok(None);
        };
        self.resolve_note_replay(client, existing, expected_kind, key, incoming_fingerprint)
            .await
            .map(Some)
    }

    async fn resolve_note_replay<C: GenericClient>(
        &self,
        client: &C,
        existing: StoredIdempotencyRecord,
        expected_kind: MemoryEventKind,
        key: &IdempotencyKey,
        incoming_fingerprint: &Sha256Hex,
    ) -> Result<NoteMutationResponse, DennettError> {
        if existing.kind != expected_kind
            || existing.payload_fingerprint != incoming_fingerprint.as_str()
        {
            return Err(DennettError::IdempotencyMismatch {
                idempotency_key: key.to_string(),
                reason: String::from(
                    "idempotency_key was already used with a different operation or payload",
                ),
            });
        }

        let note_id = MemoryNoteId::from_uuid(existing.target_entity_id);
        let revision = existing.target_revision.unwrap_or(1);
        let note = fetch_note_at_revision(client, note_id, revision, self).await?;
        Ok(NoteMutationResponse {
            note,
            event_id: existing.event_id,
            idempotent_replay: true,
        })
    }

    async fn recover_note_idempotency_race<C: GenericClient>(
        &self,
        client: &C,
        scope: &MemoryScope,
        expected_kind: MemoryEventKind,
        key: &IdempotencyKey,
        incoming_fingerprint: &Sha256Hex,
    ) -> Result<NoteMutationResponse, DennettError> {
        let Some(existing) = lookup_idempotency(client, scope, key, self).await? else {
            return Err(DennettError::Conflict {
                resource: String::from("memory_note"),
                expected_revision: None,
                actual_revision: None,
                reason: String::from("concurrent idempotency conflict; retry request"),
            });
        };
        self.resolve_note_replay(client, existing, expected_kind, key, incoming_fingerprint)
            .await
    }
}

/// Capabilities advertised by standalone `dennett-memoryd` in milestone D1.
#[must_use]
pub fn d1_memoryd_capabilities() -> Vec<String> {
    vec![
        String::from("healthz"),
        String::from("postgres-connectivity"),
        String::from("memory-v1"),
        String::from("scoped-auth"),
        String::from("evidence-multi-anchor"),
        String::from("note-revisions"),
        String::from("idempotency"),
    ]
}

struct StoredIdempotencyRecord {
    event_id: MemoryEventId,
    kind: MemoryEventKind,
    payload_fingerprint: String,
    target_entity_id: Uuid,
    target_revision: Option<i64>,
}

fn require_scope_capability(
    principal: &AuthPrincipal,
    scope: &MemoryScope,
    capability: MemoryCapability,
) -> Result<(), DennettError> {
    if principal.can_access(scope, capability) {
        return Ok(());
    }
    let reason = if principal.has_capability(capability) {
        format!("principal is not authorized for scope `{scope}`")
    } else {
        format!("principal lacks `{capability}` capability for scope `{scope}`")
    };
    Err(DennettError::ForbiddenScope {
        scope: scope.to_string(),
        reason,
    })
}

/// Checks scope/capability on an existing stored resource without disclosing the name of a
/// foreign scope the principal is not allowed to see.
fn require_resource_scope_capability(
    principal: &AuthPrincipal,
    resource_scope: &MemoryScope,
    capability: MemoryCapability,
) -> Result<(), DennettError> {
    if principal.can_access(resource_scope, capability) {
        return Ok(());
    }
    let visible_scope = if principal.allowed_scopes.contains(resource_scope) {
        resource_scope.to_string()
    } else {
        String::from("restricted")
    };
    let reason = if principal.has_capability(capability) {
        String::from("principal is not authorized for the resource's scope")
    } else {
        format!("principal lacks `{capability}` capability for this resource")
    };
    Err(DennettError::ForbiddenScope {
        scope: visible_scope,
        reason,
    })
}

fn resolve_search_scopes(
    principal: &AuthPrincipal,
    requested: Option<&MemoryScope>,
) -> Result<Vec<String>, DennettError> {
    if let Some(scope) = requested {
        require_scope_capability(principal, scope, MemoryCapability::Read)?;
        return Ok(vec![scope.as_str().to_owned()]);
    }
    if !principal.has_capability(MemoryCapability::Read) {
        return Err(DennettError::ForbiddenScope {
            scope: String::from("*"),
            reason: String::from("principal lacks `read` capability"),
        });
    }
    Ok(principal
        .allowed_scopes
        .iter()
        .map(|s| s.as_str().to_owned())
        .collect())
}

struct NewMemoryEvent<'a> {
    event_id: MemoryEventId,
    scope: &'a MemoryScope,
    kind: MemoryEventKind,
    principal_id: &'a PrincipalId,
    correlation_id: &'a CorrelationId,
    idempotency_key: &'a IdempotencyKey,
    fingerprint: &'a Sha256Hex,
    target_entity_id: Uuid,
    target_revision: Option<i64>,
    observed_ms: Option<i64>,
}

/// Inserts a row into `memory_events`, returning `Ok(false)` when a concurrent idempotency race
/// tripped `memory_events_scope_idempotency_uniq`.
async fn insert_memory_event<C: GenericClient>(
    client: &C,
    ev: NewMemoryEvent<'_>,
    svc: &MemoryService,
) -> Result<bool, DennettError> {
    let stream_id = scope_stream_id(ev.scope);
    let stream_seq = allocate_stream_seq(client, &stream_id, svc).await?;

    let res = client
        .execute(
            "INSERT INTO memory_events (
                event_id, stream_id, stream_seq, scope, event_kind, principal_id,
                correlation_id, idempotency_key, payload_fingerprint,
                target_entity_id, target_revision, observed_at
             ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11,
                TIMESTAMPTZ '1970-01-01 00:00:00+00' + ($12::bigint * INTERVAL '1 millisecond')
             )",
            &[
                &ev.event_id.as_uuid(),
                &stream_id,
                &stream_seq,
                &ev.scope.as_str(),
                &ev.kind.as_str(),
                &ev.principal_id.as_str(),
                &ev.correlation_id.as_str(),
                &ev.idempotency_key.as_str(),
                &ev.fingerprint.as_str(),
                &ev.target_entity_id,
                &ev.target_revision,
                &ev.observed_ms,
            ],
        )
        .await;

    match res {
        Ok(_) => Ok(true),
        Err(err) if is_idempotency_unique_violation(&err) => Ok(false),
        Err(err) => Err(svc.map_pg_err(&err)),
    }
}

struct NewNoteRevision<'a> {
    revision_id: NoteRevisionId,
    note_id: MemoryNoteId,
    revision: i64,
    title: &'a str,
    body: &'a str,
    tags_json: &'a JsonValue,
    rationale: Option<&'a str>,
    event_id: MemoryEventId,
    recorded_by: &'a PrincipalId,
    author_label: Option<&'a str>,
}

async fn insert_note_revision_row<C: GenericClient>(
    client: &C,
    rev: NewNoteRevision<'_>,
    svc: &MemoryService,
) -> Result<(), DennettError> {
    client
        .execute(
            "INSERT INTO memory_note_revisions (
                revision_id, note_id, revision, title, body, tags, rationale,
                event_id, recorded_by, author_label
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
            &[
                &rev.revision_id.as_uuid(),
                &rev.note_id.as_uuid(),
                &rev.revision,
                &rev.title,
                &rev.body,
                &rev.tags_json,
                &rev.rationale,
                &rev.event_id.as_uuid(),
                &rev.recorded_by.as_str(),
                &rev.author_label,
            ],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;
    Ok(())
}

async fn lock_note_scope_and_revision<C: GenericClient>(
    client: &C,
    principal: &AuthPrincipal,
    req: &CorrectNoteRequest,
    svc: &MemoryService,
) -> Result<(MemoryScope, i64), DennettError> {
    let locked = client
        .query_opt(
            "SELECT scope, current_revision FROM memory_notes WHERE note_id = $1 FOR UPDATE",
            &[&req.note_id.as_uuid()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let Some(note_row) = locked else {
        return Err(DennettError::NotFound {
            resource: String::from("memory_note"),
            id: req.note_id.to_string(),
        });
    };

    let stored_scope = MemoryScope::try_new(note_row.get::<_, String>(0))?;
    let actual_revision: i64 = note_row.get(1);

    require_resource_scope_capability(principal, &stored_scope, MemoryCapability::Write)?;
    if stored_scope != req.scope {
        return Err(DennettError::ForbiddenScope {
            scope: req.scope.to_string(),
            reason: String::from("note does not belong to the requested scope"),
        });
    }

    Ok((stored_scope, actual_revision))
}

fn ensure_expected_revision(
    expected_revision: i64,
    actual_revision: i64,
) -> Result<(), DennettError> {
    if actual_revision != expected_revision {
        return Err(DennettError::Conflict {
            resource: String::from("memory_note"),
            expected_revision: Some(expected_revision),
            actual_revision: Some(actual_revision),
            reason: format!(
                "optimistic revision mismatch: expected revision {expected_revision}, current revision is {actual_revision}"
            ),
        });
    }
    Ok(())
}

async fn persist_correction_revision<C: GenericClient>(
    client: &C,
    req: &CorrectNoteRequest,
    principal: &AuthPrincipal,
    rev: NewNoteRevision<'_>,
    actual_revision: i64,
    svc: &MemoryService,
) -> Result<(), DennettError> {
    let revision_id = rev.revision_id;
    let new_revision = rev.revision;
    let event_id = rev.event_id;
    let tags_json = rev.tags_json;
    insert_note_revision_row(client, rev, svc).await?;
    update_note_head(
        client,
        req,
        principal,
        tags_json,
        actual_revision,
        new_revision,
        revision_id,
        event_id,
        svc,
    )
    .await
}

#[expect(
    clippy::too_many_arguments,
    reason = "internal helper mirrors the exact column update set on memory_notes"
)]
async fn update_note_head<C: GenericClient>(
    client: &C,
    req: &CorrectNoteRequest,
    principal: &AuthPrincipal,
    tags_json: &JsonValue,
    actual_revision: i64,
    new_revision: i64,
    revision_id: NoteRevisionId,
    event_id: MemoryEventId,
    svc: &MemoryService,
) -> Result<(), DennettError> {
    let updated_rows = client
        .execute(
            "UPDATE memory_notes
             SET current_revision = $1,
                 current_revision_id = $2,
                 title = $3,
                 body = $4,
                 tags = $5,
                 rationale = $6,
                 last_event_id = $7,
                 recorded_by = $8,
                 author_label = $9,
                 updated_at = now()
             WHERE note_id = $10 AND current_revision = $11",
            &[
                &new_revision,
                &revision_id.as_uuid(),
                &req.title,
                &req.body,
                &tags_json,
                &req.rationale,
                &event_id.as_uuid(),
                &principal.principal_id.as_str(),
                &req.author_label,
                &req.note_id.as_uuid(),
                &actual_revision,
            ],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    if updated_rows != 1 {
        return Err(DennettError::Conflict {
            resource: String::from("memory_note"),
            expected_revision: Some(req.expected_revision),
            actual_revision: None,
            reason: String::from("concurrent update modified note revision"),
        });
    }
    Ok(())
}

async fn query_scoped_notes<C: GenericClient>(
    client: &C,
    allowed_scopes: &[String],
    query_text: Option<&str>,
    limit_i64: i64,
    svc: &MemoryService,
) -> Result<Vec<Row>, DennettError> {
    if let Some(q) = query_text {
        let like_pattern = format!("%{}%", escape_like_pattern(q));
        client
            .query(
                "SELECT note_id, scope, current_revision, current_revision_id,
                        title, body, tags, rationale, last_event_id,
                        recorded_by, author_label,
                        TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
                        TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint,
                        COUNT(*) OVER()::bigint AS total_matched
                 FROM memory_notes
                 WHERE scope = ANY($1::text[])
                   AND (
                        to_tsvector('simple', title || ' ' || body || ' ' || COALESCE(rationale, ''))
                            @@ plainto_tsquery('simple', $2)
                        OR title ILIKE $3 ESCAPE '\\'
                        OR body ILIKE $3 ESCAPE '\\'
                        OR COALESCE(rationale, '') ILIKE $3 ESCAPE '\\'
                        OR EXISTS (
                            SELECT 1
                            FROM jsonb_array_elements_text(tags) AS tag_val
                            WHERE tag_val ILIKE $3 ESCAPE '\\'
                        )
                   )
                 ORDER BY updated_at DESC, note_id ASC
                 LIMIT $4",
                &[&allowed_scopes, &q, &like_pattern, &limit_i64],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))
    } else {
        client
            .query(
                "SELECT note_id, scope, current_revision, current_revision_id,
                        title, body, tags, rationale, last_event_id,
                        recorded_by, author_label,
                        TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
                        TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint,
                        COUNT(*) OVER()::bigint AS total_matched
                 FROM memory_notes
                 WHERE scope = ANY($1::text[])
                 ORDER BY updated_at DESC, note_id ASC
                 LIMIT $2",
                &[&allowed_scopes, &limit_i64],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))
    }
}

fn scope_stream_id(scope: &MemoryScope) -> String {
    format!("memory:scope:{}", scope.as_str())
}

fn is_idempotency_unique_violation(err: &tokio_postgres::Error) -> bool {
    err.as_db_error().is_some_and(|db_err| {
        db_err.code() == &SqlState::UNIQUE_VIOLATION
            && db_err.constraint() == Some(IDEMPOTENCY_UNIQUE_CONSTRAINT)
    })
}

fn escape_like_pattern(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if matches!(ch, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

async fn allocate_stream_seq<C: GenericClient>(
    client: &C,
    stream_id: &str,
    svc: &MemoryService,
) -> Result<i64, DennettError> {
    let row = client
        .query_one(
            "INSERT INTO stream_counters (stream_id, committed_seq)
             VALUES ($1, 1)
             ON CONFLICT (stream_id)
             DO UPDATE SET committed_seq = stream_counters.committed_seq + 1
             RETURNING committed_seq",
            &[&stream_id],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;
    Ok(row.get(0))
}

async fn lookup_idempotency<C: GenericClient>(
    client: &C,
    scope: &MemoryScope,
    key: &IdempotencyKey,
    svc: &MemoryService,
) -> Result<Option<StoredIdempotencyRecord>, DennettError> {
    let row = client
        .query_opt(
            "SELECT event_id, event_kind, payload_fingerprint, target_entity_id, target_revision
             FROM memory_events
             WHERE scope = $1 AND idempotency_key = $2",
            &[&scope.as_str(), &key.as_str()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let Some(row) = row else {
        return Ok(None);
    };

    Ok(Some(StoredIdempotencyRecord {
        event_id: MemoryEventId::from_uuid(row.get(0)),
        kind: row.get::<_, String>(1).parse()?,
        payload_fingerprint: row.get(2),
        target_entity_id: row.get(3),
        target_revision: row.get(4),
    }))
}

async fn verify_evidence_links_in_scope<C: GenericClient>(
    client: &C,
    scope: &MemoryScope,
    links: &[EvidenceLinkInput],
    svc: &MemoryService,
) -> Result<(), DennettError> {
    if links.is_empty() {
        return Ok(());
    }

    let unique_ids: BTreeSet<Uuid> = links.iter().map(|l| l.evidence_id.as_uuid()).collect();
    let id_vec: Vec<Uuid> = unique_ids.into_iter().collect();

    let rows = client
        .query(
            "SELECT evidence_id, scope FROM evidence_objects WHERE evidence_id = ANY($1::uuid[])",
            &[&id_vec],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let mut scopes_by_id: HashMap<Uuid, String> = HashMap::with_capacity(rows.len());
    for row in rows {
        scopes_by_id.insert(row.get(0), row.get(1));
    }

    for link in links {
        let Some(ev_scope) = scopes_by_id.get(&link.evidence_id.as_uuid()) else {
            return Err(DennettError::NotFound {
                resource: String::from("evidence"),
                id: link.evidence_id.to_string(),
            });
        };
        if ev_scope != scope.as_str() {
            return Err(DennettError::ForbiddenScope {
                scope: scope.to_string(),
                reason: format!(
                    "evidence `{}` does not belong to scope `{scope}`",
                    link.evidence_id
                ),
            });
        }
    }

    Ok(())
}

async fn insert_evidence_links<C: GenericClient>(
    client: &C,
    event_id: MemoryEventId,
    revision: i64,
    links: &[EvidenceLinkInput],
    svc: &MemoryService,
) -> Result<(), DennettError> {
    for link in links {
        let link_id = EvidenceLinkId::new();
        let anchor_json: Option<JsonValue> = link
            .anchor
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .map_err(|err| DennettError::Internal {
                detail: format!("failed to serialize evidence anchor: {err}"),
            })?;

        client
            .execute(
                "INSERT INTO event_evidence_links (
                    link_id, event_id, evidence_id, relationship, anchor, ordinal, revision
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7)",
                &[
                    &link_id.as_uuid(),
                    &event_id.as_uuid(),
                    &link.evidence_id.as_uuid(),
                    &link.relationship.as_str(),
                    &anchor_json,
                    &link.ordinal,
                    &revision,
                ],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))?;
    }
    Ok(())
}

async fn fetch_evidence_row<C: GenericClient>(
    client: &C,
    evidence_id: EvidenceId,
    svc: &MemoryService,
) -> Result<EvidenceObject, DennettError> {
    let row = client
        .query_opt(
            "SELECT evidence_id, scope, sha256, media_type, byte_len, content, source_uri,
                    TRUNC(EXTRACT(EPOCH FROM observed_at) * 1000)::bigint,
                    TRUNC(EXTRACT(EPOCH FROM ingested_at) * 1000)::bigint,
                    ingested_by, metadata
             FROM evidence_objects
             WHERE evidence_id = $1",
            &[&evidence_id.as_uuid()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let Some(row) = row else {
        return Err(DennettError::NotFound {
            resource: String::from("evidence"),
            id: evidence_id.to_string(),
        });
    };

    let metadata: BTreeMap<String, String> = serde_json::from_value(row.get::<_, JsonValue>(10))
        .map_err(|err| DennettError::Internal {
            detail: format!("corrupt evidence metadata JSON: {err}"),
        })?;
    let byte_len = u64::try_from(row.get::<_, i64>(4)).unwrap_or(0);

    Ok(EvidenceObject {
        evidence_id: EvidenceId::from_uuid(row.get(0)),
        scope: MemoryScope::try_new(row.get::<_, String>(1))?,
        sha256: Sha256Hex::try_new(row.get::<_, String>(2))?,
        media_type: row.get(3),
        byte_len,
        content: row.get(5),
        source_uri: row.get(6),
        observed_at: row.get::<_, Option<i64>>(7).map(TimestampMs::from_millis),
        ingested_at: TimestampMs::from_millis(row.get(8)),
        ingested_by: PrincipalId::try_new(row.get::<_, String>(9))?,
        metadata,
    })
}

async fn fetch_current_note_row<C: GenericClient>(
    client: &C,
    note_id: MemoryNoteId,
    svc: &MemoryService,
) -> Result<MemoryNote, DennettError> {
    let row = client
        .query_opt(
            "SELECT note_id, scope, current_revision, current_revision_id,
                    title, body, tags, rationale, last_event_id,
                    recorded_by, author_label,
                    TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
                    TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint
             FROM memory_notes
             WHERE note_id = $1",
            &[&note_id.as_uuid()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let Some(row) = row else {
        return Err(DennettError::NotFound {
            resource: String::from("memory_note"),
            id: note_id.to_string(),
        });
    };

    let last_event_uuid: Uuid = row.get(8);
    let mut links = fetch_links_for_events(client, &[last_event_uuid], svc).await?;
    decode_note_row(&row, links.remove(&last_event_uuid).unwrap_or_default())
}

async fn fetch_note_at_revision<C: GenericClient>(
    client: &C,
    note_id: MemoryNoteId,
    revision: i64,
    svc: &MemoryService,
) -> Result<MemoryNote, DennettError> {
    let row = client
        .query_opt(
            "SELECT n.note_id, n.scope, r.revision, r.revision_id,
                    r.title, r.body, r.tags, r.rationale, r.event_id,
                    r.recorded_by, r.author_label,
                    TRUNC(EXTRACT(EPOCH FROM n.created_at) * 1000)::bigint,
                    TRUNC(EXTRACT(EPOCH FROM r.created_at) * 1000)::bigint
             FROM memory_notes n
             JOIN memory_note_revisions r ON r.note_id = n.note_id
             WHERE n.note_id = $1 AND r.revision = $2",
            &[&note_id.as_uuid(), &revision],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let Some(row) = row else {
        return Err(DennettError::NotFound {
            resource: String::from("memory_note_revision"),
            id: format!("{note_id}@{revision}"),
        });
    };

    let event_uuid: Uuid = row.get(8);
    let mut links = fetch_links_for_events(client, &[event_uuid], svc).await?;
    decode_note_row(&row, links.remove(&event_uuid).unwrap_or_default())
}

async fn fetch_links_for_events<C: GenericClient>(
    client: &C,
    event_ids: &[Uuid],
    svc: &MemoryService,
) -> Result<HashMap<Uuid, Vec<EventEvidenceLink>>, DennettError> {
    let mut out: HashMap<Uuid, Vec<EventEvidenceLink>> = HashMap::new();
    if event_ids.is_empty() {
        return Ok(out);
    }

    let rows = client
        .query(
            "SELECT link_id, event_id, evidence_id, relationship, anchor, ordinal, revision
             FROM event_evidence_links
             WHERE event_id = ANY($1::uuid[])
             ORDER BY ordinal ASC NULLS LAST, link_id ASC",
            &[&event_ids],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    for row in rows {
        let event_uuid: Uuid = row.get(1);
        let anchor_val: Option<JsonValue> = row.get(4);
        let anchor: Option<EvidenceAnchor> = anchor_val
            .map(serde_json::from_value)
            .transpose()
            .map_err(|err| DennettError::Internal {
                detail: format!("corrupt evidence anchor JSON: {err}"),
            })?;

        let link = EventEvidenceLink {
            link_id: EvidenceLinkId::from_uuid(row.get(0)),
            event_id: MemoryEventId::from_uuid(event_uuid),
            evidence_id: EvidenceId::from_uuid(row.get(2)),
            relationship: EvidenceRelationship::try_new(row.get::<_, String>(3))?,
            anchor,
            ordinal: row.get(5),
            revision: row.get(6),
        };
        out.entry(event_uuid).or_default().push(link);
    }

    Ok(out)
}

fn decode_note_row(
    row: &Row,
    evidence_links: Vec<EventEvidenceLink>,
) -> Result<MemoryNote, DennettError> {
    Ok(MemoryNote {
        note_id: MemoryNoteId::from_uuid(row.get(0)),
        scope: MemoryScope::try_new(row.get::<_, String>(1))?,
        current_revision: row.get(2),
        current_revision_id: NoteRevisionId::from_uuid(row.get(3)),
        title: row.get(4),
        body: row.get(5),
        tags: decode_tags(row.get::<_, JsonValue>(6))?,
        rationale: row.get(7),
        last_event_id: MemoryEventId::from_uuid(row.get(8)),
        recorded_by: PrincipalId::try_new(row.get::<_, String>(9))?,
        author_label: row.get(10),
        created_at: TimestampMs::from_millis(row.get(11)),
        updated_at: TimestampMs::from_millis(row.get(12)),
        evidence_links,
    })
}

fn decode_tags(value: JsonValue) -> Result<Vec<String>, DennettError> {
    serde_json::from_value(value).map_err(|err| DennettError::Internal {
        detail: format!("corrupt note tags JSON: {err}"),
    })
}

const _: () = {
    const fn assert_send_sync<T: Send + Sync + 'static>() {}
    assert_send_sync::<MemoryService>();
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_like_pattern_escapes_wildcards_and_backslashes() {
        assert_eq!(escape_like_pattern("plain"), "plain");
        assert_eq!(escape_like_pattern("100%_done\\ok"), "100\\%\\_done\\\\ok");
    }
}
