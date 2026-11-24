//! Canonical PostgreSQL-backed Memory Fabric domain service (`MemoryService`).
//!
//! Enforces scope and capability authorization against a verified [`AuthPrincipal`],
//! executes atomic PostgreSQL transactions with `stream_counters`-backed monotonic
//! event ordering, handles optimistic revision locking and concurrent idempotency races,
//! supports D3 Memory Fabric v2 (claims, conflicts, dossiers, consolidation, separable
//! multi-language FTS with fresh-tail overlay, forgetting, and Contract J portable
//! export/import), and redacts all PostgreSQL credentials from every error path.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Duration;

use deadpool_postgres::{GenericClient, Manager, ManagerConfig, Pool, RecyclingMethod};
use dennett_contracts::{
    AdmissionStatus, AuthPrincipal, ClaimId, ConsolidationJob, ConsolidationJobId,
    CorrectNoteRequest, CorrelationId, CreateClaimRequest, CreateConsolidationJobRequest,
    CreateDocumentRequest, CreateNoteRequest, DEFAULT_NOTE_SALIENCE, DennettError, DocumentId,
    EventEvidenceLink, EvidenceAnchor, EvidenceId, EvidenceLinkId, EvidenceLinkInput,
    EvidenceObject, EvidenceRelationship, ExportRequest, ExportResponse, ForgetRequest,
    ForgetResponse, GetDocumentVersionsResponse, IdempotencyKey, ImportRequest, ImportResponse,
    IndexState, IndexStatus, IngestEvidenceRequest, IngestEvidenceResponse, ListClaimsResponse,
    ListConflictsResponse, ListConsolidationJobsResponse, ListDocumentsResponse,
    ListTombstonesResponse, MAX_NOTE_SALIENCE, MemoryCapability, MemoryClaim, MemoryConflict,
    MemoryDocument, MemoryEvent, MemoryEventId, MemoryEventKind, MemoryNote, MemoryNoteId,
    MemoryScope, MemoryStatsSummary, NoteHistoryResponse, NoteMutationResponse, NoteRevision,
    NoteRevisionId, PrincipalId, ProfileMode, ResolveConflictRequest, SearchNotesRequest,
    SearchNotesResponse, ServiceHealth, ServiceKind, Sha256Hex, SupersedeClaimRequest, TimestampMs,
    TriggerIndexRebuildRequest, TriggerIndexRebuildResponse, UpdateDocumentRequest,
};
use serde_json::Value as JsonValue;
use tokio::sync::Mutex;
use tokio::time;
use tokio_postgres::error::SqlState;
use tokio_postgres::{NoTls, Row};
use uuid::Uuid;

use crate::claims::{
    execute_create_claim, execute_get_claim, execute_list_claims, execute_list_open_conflicts,
    execute_resolve_conflict, execute_supersede_claim,
};
use crate::consolidation::{
    execute_create_consolidation_job, execute_list_consolidation_jobs,
    execute_pause_consolidation_job, execute_resume_consolidation_job,
};
use crate::documents::{
    execute_create_document, execute_get_document, execute_get_document_versions,
    execute_list_documents, execute_update_document,
};
use crate::fingerprint::{
    fingerprint_correct_note, fingerprint_create_note, fingerprint_ingest_evidence, sha256_hex,
};
use crate::forget::{execute_forget, execute_list_tombstones};
use crate::migrations::{query_schema_version, run_migrations};
use crate::pg_target::ValidatedPgTarget;
use crate::portable::{execute_export_package, execute_import_package};
use crate::search::{
    compute_aggregate_index_status, execute_get_index_status, execute_search_notes,
    execute_trigger_index_rebuild, maybe_advance_watermark_only, maybe_index_note_in_tx,
};

/// Current implementation milestone label advertised in [`ServiceHealth`].
pub const MEMORYD_STAGE_LABEL: &str = "D3";
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

    pub(crate) fn op_timeout(&self) -> Duration {
        self.inner.op_timeout
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
    /// Content-fingerprint deduplication preserves episode distinctness: re-ingesting identical
    /// content within the same scope under a different `idempotency_key` reuses the existing
    /// active `evidence_id` while ALWAYS appending a distinct `memory_events` episode row with
    /// its own `event_id`, `observed_at`, and `correlation_id`.
    ///
    /// Repeating the request with the same `(scope, idempotency_key)` and identical payload
    /// returns the original response with `idempotent_replay = true`; reusing the key with a
    /// different payload returns [`DennettError::IdempotencyMismatch`].
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::ForbiddenScope`],
    /// [`DennettError::IdempotencyMismatch`], or [`DennettError::DatabaseUnavailable`].
    #[expect(
        clippy::too_many_lines,
        reason = "evidence ingest performs transactional content-fingerprint dedup while always recording a fresh episode event"
    )]
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

            // Serialize same-(scope, sha256) ingestion so concurrent episodes reuse one evidence row.
            let dedup_lock_key =
                format!("ev-dedup:{}:{}", req.scope.as_str(), content_sha.as_str());
            tx.execute(
                "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
                &[&dedup_lock_key],
            )
            .await
            .map_err(|err| self.map_pg_err(&err))?;

            // Re-check idempotency after acquiring advisory lock.
            if let Some(existing) =
                lookup_idempotency(&tx, &req.scope, &req.idempotency_key, self).await?
            {
                return self
                    .resolve_evidence_replay(&tx, existing, &req.idempotency_key, &fingerprint)
                    .await;
            }

            let existing_ev = tx
                .query_opt(
                    "SELECT evidence_id
                     FROM evidence_objects
                     WHERE scope = $1 AND sha256 = $2 AND forgotten_at IS NULL
                     ORDER BY ingested_at ASC, evidence_id ASC
                     LIMIT 1",
                    &[&req.scope.as_str(), &content_sha.as_str()],
                )
                .await
                .map_err(|err| self.map_pg_err(&err))?;

            let observed_ms = req.observed_at.map(TimestampMs::as_millis);
            let evidence_id = if let Some(row) = existing_ev {
                EvidenceId::from_uuid(row.get(0))
            } else {
                let fresh_id = EvidenceId::new();
                tx.execute(
                    "INSERT INTO evidence_objects (
                        evidence_id, scope, sha256, media_type, byte_len, content,
                        source_uri, observed_at, ingested_by, metadata,
                        trust_level, instruction_active
                     ) VALUES (
                        $1, $2, $3, $4, $5, $6, $7,
                        TIMESTAMPTZ '1970-01-01 00:00:00+00' + ($8::bigint * INTERVAL '1 millisecond'),
                        $9, $10, 'native', true
                     )",
                    &[
                        &fresh_id.as_uuid(),
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
                fresh_id
            };

            let event_id = MemoryEventId::new();
            let Some(stream_seq) = insert_memory_event(
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
            .await?
            else {
                let _ = tx.rollback().await;
                return self
                    .recover_evidence_idempotency_race(
                        &conn,
                        &req.scope,
                        &req.idempotency_key,
                        &fingerprint,
                    )
                    .await;
            };

            maybe_advance_watermark_only(&tx, &req.scope, event_id, stream_seq, self).await?;

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

    /// Fetches an active (non-forgotten) [`EvidenceObject`] by identifier after verifying read scope access.
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
    /// (Architecture 81 §3.3 / V19). Rejects links referencing missing, forgotten, or cross-scope evidence.
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

    #[expect(
        clippy::too_many_lines,
        reason = "transactional note creation validates evidence links, inserts note + revision 1, emits event, and updates search index"
    )]
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
        let admission_status = req.admission_status.unwrap_or(AdmissionStatus::Admitted);
        let salience = i16::from(req.salience.unwrap_or(DEFAULT_NOTE_SALIENCE));

        let Some(stream_seq) = insert_memory_event(
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
        .await?
        else {
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
        };

        insert_evidence_links(&tx, event_id, 1, &req.evidence_links, self).await?;

        tx.execute(
            "INSERT INTO memory_notes (
                note_id, scope, current_revision, current_revision_id,
                title, body, tags, rationale, last_event_id,
                recorded_by, author_label,
                admission_status, salience, topic_key, trust_level, instruction_active
             ) VALUES ($1, $2, 1, $3, $4, $5, $6, NULL, $7, $8, $9, $10, $11, $12, 'native', true)",
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
                &admission_status.as_str(),
                &salience,
                &req.topic_key,
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

        maybe_index_note_in_tx(
            &tx,
            &req.scope,
            note_id.as_uuid(),
            admission_status,
            event_id,
            stream_seq,
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

    /// Reads the current projection of an active (non-forgotten) [`MemoryNote`] after verifying read scope access.
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

    /// Reads the complete immutable revision history of a non-forgotten [`MemoryNote`] in ascending revision order.
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
                    "SELECT scope, current_revision
                     FROM memory_notes
                     WHERE note_id = $1 AND forgotten_at IS NULL",
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

    #[expect(
        clippy::too_many_lines,
        reason = "transactional note correction enforces optimistic revision lock, inserts revision, emits event, and updates search index"
    )]
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

        let (stored_scope, actual_revision, admission_status) =
            lock_note_scope_and_revision(&tx, principal, req, self).await?;

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

        let Some(stream_seq) = insert_memory_event(
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
        .await?
        else {
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
        };

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

        maybe_index_note_in_tx(
            &tx,
            &stored_scope,
            req.note_id.as_uuid(),
            admission_status,
            event_id,
            stream_seq,
            self,
        )
        .await?;

        let note = fetch_current_note_row(&tx, req.note_id, self).await?;
        tx.commit().await.map_err(|err| self.map_pg_err(&err))?;

        Ok(NoteMutationResponse {
            note,
            event_id,
            idempotent_replay: false,
        })
    }

    /// Searches or lists active [`MemoryNote`] projections strictly within authorized scopes,
    /// combining the separable multi-language FTS index with the fresh-tail overlay and
    /// returning honest live [`IndexStatus`].
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
        execute_search_notes(self, principal, req).await
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

    // ========================================================================
    // Phase D3 Memory Fabric APIs
    // ========================================================================

    /// Creates a structured [`MemoryClaim`], detecting contradictions against active competing claims.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::ForbiddenScope`],
    /// [`DennettError::NotFound`], [`DennettError::Conflict`], [`DennettError::IdempotencyMismatch`],
    /// or [`DennettError::DatabaseUnavailable`].
    pub async fn create_claim(
        &self,
        principal: &AuthPrincipal,
        correlation_id: &CorrelationId,
        req: CreateClaimRequest,
    ) -> Result<MemoryClaim, DennettError> {
        execute_create_claim(self, principal, correlation_id, req).await
    }

    /// Fetches a single non-forgotten [`MemoryClaim`] by identifier.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`], [`DennettError::ForbiddenScope`], or
    /// [`DennettError::DatabaseUnavailable`].
    pub async fn get_claim(
        &self,
        principal: &AuthPrincipal,
        claim_id: ClaimId,
    ) -> Result<MemoryClaim, DennettError> {
        execute_get_claim(self, principal, claim_id).await
    }

    /// Lists non-forgotten [`MemoryClaim`] records across authorized scopes with optional filters.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::ForbiddenScope`] or [`DennettError::DatabaseUnavailable`].
    pub async fn list_claims(
        &self,
        principal: &AuthPrincipal,
        scope: Option<&MemoryScope>,
        topic_key: Option<&str>,
    ) -> Result<ListClaimsResponse, DennettError> {
        execute_list_claims(self, principal, scope, topic_key).await
    }

    /// Lists non-forgotten [`MemoryClaim`] records for a single authorized `scope` and `topic_key`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::ForbiddenScope`], or
    /// [`DennettError::DatabaseUnavailable`].
    pub async fn list_claims_by_topic(
        &self,
        principal: &AuthPrincipal,
        scope: &MemoryScope,
        topic_key: &str,
    ) -> Result<ListClaimsResponse, DennettError> {
        if topic_key.trim().is_empty() {
            return Err(DennettError::InvalidInput {
                field: String::from("topic_key"),
                reason: String::from("must not be empty"),
            });
        }
        execute_list_claims(self, principal, Some(scope), Some(topic_key)).await
    }

    /// Atomically supersedes an active [`MemoryClaim`] under row-level locking and temporal guards.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::ForbiddenScope`],
    /// [`DennettError::NotFound`], [`DennettError::Conflict`], [`DennettError::IdempotencyMismatch`],
    /// or [`DennettError::DatabaseUnavailable`].
    pub async fn supersede_claim(
        &self,
        principal: &AuthPrincipal,
        correlation_id: &CorrelationId,
        req: SupersedeClaimRequest,
    ) -> Result<MemoryClaim, DennettError> {
        execute_supersede_claim(self, principal, correlation_id, req).await
    }

    /// Lists open [`MemoryConflict`] rows across authorized scopes.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::ForbiddenScope`] or [`DennettError::DatabaseUnavailable`].
    pub async fn list_open_conflicts(
        &self,
        principal: &AuthPrincipal,
        scope: Option<&MemoryScope>,
    ) -> Result<ListConflictsResponse, DennettError> {
        execute_list_open_conflicts(self, principal, scope).await
    }

    /// Resolves or adjudicates an open [`MemoryConflict`].
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::ForbiddenScope`],
    /// [`DennettError::NotFound`], [`DennettError::Conflict`], [`DennettError::IdempotencyMismatch`],
    /// or [`DennettError::DatabaseUnavailable`].
    pub async fn resolve_conflict(
        &self,
        principal: &AuthPrincipal,
        correlation_id: &CorrelationId,
        req: ResolveConflictRequest,
    ) -> Result<MemoryConflict, DennettError> {
        execute_resolve_conflict(self, principal, correlation_id, req).await
    }

    /// Creates a version-1 human-readable [`MemoryDocument`] dossier.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::ForbiddenScope`],
    /// [`DennettError::NotFound`], [`DennettError::Conflict`], [`DennettError::IdempotencyMismatch`],
    /// or [`DennettError::DatabaseUnavailable`].
    pub async fn create_document(
        &self,
        principal: &AuthPrincipal,
        correlation_id: &CorrelationId,
        req: CreateDocumentRequest,
    ) -> Result<MemoryDocument, DennettError> {
        execute_create_document(self, principal, correlation_id, req).await
    }

    /// Fetches a non-forgotten [`MemoryDocument`] by identifier.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`], [`DennettError::ForbiddenScope`], or
    /// [`DennettError::DatabaseUnavailable`].
    pub async fn get_document(
        &self,
        principal: &AuthPrincipal,
        document_id: DocumentId,
    ) -> Result<MemoryDocument, DennettError> {
        execute_get_document(self, principal, document_id).await
    }

    /// Lists non-forgotten [`MemoryDocument`] dossiers across authorized scopes.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::ForbiddenScope`] or [`DennettError::DatabaseUnavailable`].
    pub async fn list_documents(
        &self,
        principal: &AuthPrincipal,
        scope: Option<&MemoryScope>,
    ) -> Result<ListDocumentsResponse, DennettError> {
        execute_list_documents(self, principal, scope).await
    }

    /// Updates a [`MemoryDocument`] under optimistic concurrency (`expected_version`).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::ForbiddenScope`],
    /// [`DennettError::NotFound`], [`DennettError::Conflict`], [`DennettError::IdempotencyMismatch`],
    /// or [`DennettError::DatabaseUnavailable`].
    pub async fn update_document(
        &self,
        principal: &AuthPrincipal,
        correlation_id: &CorrelationId,
        req: UpdateDocumentRequest,
    ) -> Result<MemoryDocument, DennettError> {
        execute_update_document(self, principal, correlation_id, req).await
    }

    /// Fetches immutable version history for a non-forgotten [`MemoryDocument`].
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`], [`DennettError::ForbiddenScope`], or
    /// [`DennettError::DatabaseUnavailable`].
    pub async fn get_document_versions(
        &self,
        principal: &AuthPrincipal,
        document_id: DocumentId,
    ) -> Result<GetDocumentVersionsResponse, DennettError> {
        execute_get_document_versions(self, principal, document_id).await
    }

    /// Creates and executes an interruptible deterministic [`ConsolidationJob`].
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::ForbiddenScope`], [`DennettError::IdempotencyMismatch`], or
    /// [`DennettError::DatabaseUnavailable`].
    pub async fn create_consolidation_job(
        &self,
        principal: &AuthPrincipal,
        correlation_id: &CorrelationId,
        req: CreateConsolidationJobRequest,
    ) -> Result<ConsolidationJob, DennettError> {
        execute_create_consolidation_job(self, principal, correlation_id, req, None).await
    }

    /// Creates and runs at most `max_steps` work items of a [`ConsolidationJob`], pausing cleanly
    /// at a durable checkpoint if remaining items exist (useful for step-budgeted runs and fault tests).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::ForbiddenScope`], [`DennettError::IdempotencyMismatch`], or
    /// [`DennettError::DatabaseUnavailable`].
    pub async fn create_consolidation_job_with_step_limit(
        &self,
        principal: &AuthPrincipal,
        correlation_id: &CorrelationId,
        req: CreateConsolidationJobRequest,
        max_steps: usize,
    ) -> Result<ConsolidationJob, DennettError> {
        execute_create_consolidation_job(self, principal, correlation_id, req, Some(max_steps))
            .await
    }

    /// Lists [`ConsolidationJob`] records across authorized scopes.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::ForbiddenScope`] or [`DennettError::DatabaseUnavailable`].
    pub async fn list_consolidation_jobs(
        &self,
        principal: &AuthPrincipal,
        scope: Option<&MemoryScope>,
    ) -> Result<ListConsolidationJobsResponse, DennettError> {
        execute_list_consolidation_jobs(self, principal, scope).await
    }

    /// Pauses an active (`pending` or `running`) [`ConsolidationJob`] at its durable checkpoint.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`], [`DennettError::ForbiddenScope`],
    /// [`DennettError::Conflict`], or [`DennettError::DatabaseUnavailable`].
    pub async fn pause_consolidation_job(
        &self,
        principal: &AuthPrincipal,
        job_id: ConsolidationJobId,
    ) -> Result<ConsolidationJob, DennettError> {
        execute_pause_consolidation_job(self, principal, job_id).await
    }

    /// Resumes a `paused`, `failed`, or `pending` [`ConsolidationJob`] from its saved checkpoint.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`], [`DennettError::ForbiddenScope`],
    /// [`DennettError::Conflict`], or [`DennettError::DatabaseUnavailable`].
    pub async fn resume_consolidation_job(
        &self,
        principal: &AuthPrincipal,
        correlation_id: &CorrelationId,
        job_id: ConsolidationJobId,
    ) -> Result<ConsolidationJob, DennettError> {
        execute_resume_consolidation_job(self, principal, correlation_id, job_id).await
    }

    /// Queries honest live search index health and watermark status for an authorized scope.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::ForbiddenScope`] or [`DennettError::DatabaseUnavailable`].
    pub async fn get_index_status(
        &self,
        principal: &AuthPrincipal,
        scope: Option<&MemoryScope>,
    ) -> Result<IndexState, DennettError> {
        execute_get_index_status(self, principal, scope).await
    }

    /// Rebuilds the separable search index for a scope from canonical active tables (`Admin`).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::ForbiddenScope`] or [`DennettError::DatabaseUnavailable`].
    pub async fn trigger_index_rebuild(
        &self,
        principal: &AuthPrincipal,
        req: TriggerIndexRebuildRequest,
    ) -> Result<TriggerIndexRebuildResponse, DennettError> {
        execute_trigger_index_rebuild(self, principal, req).await
    }

    /// Computes live scoped summary statistics across authorized scopes (`GET /v1/memory/stats`).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::ForbiddenScope`] or [`DennettError::DatabaseUnavailable`].
    pub async fn get_stats(
        &self,
        principal: &AuthPrincipal,
        scope: Option<&MemoryScope>,
    ) -> Result<MemoryStatsSummary, DennettError> {
        let allowed_scopes = resolve_search_scopes(principal, scope)?;
        if allowed_scopes.is_empty() {
            return Ok(MemoryStatsSummary {
                scope: scope.cloned(),
                sources_count: 0,
                active_notes_count: 0,
                active_claims_count: 0,
                documents_count: 0,
                open_conflicts_count: 0,
                imported_untrusted_count: 0,
                index_status: IndexStatus::Healthy,
                watermark_seq: 0,
                last_rebuild_at: None,
            });
        }

        self.ensure_migrated().await?;
        let requested_scope = scope.cloned();

        let op_fut = async {
            let conn = self.get_conn(self.inner.op_timeout).await?;
            let row = conn
                .query_one(
                    "SELECT
                        (SELECT COUNT(*)::bigint FROM evidence_objects
                         WHERE scope = ANY($1::text[]) AND forgotten_at IS NULL),
                        (SELECT COUNT(*)::bigint FROM memory_notes
                         WHERE scope = ANY($1::text[]) AND forgotten_at IS NULL AND admission_status <> 'rejected'),
                        (SELECT COUNT(*)::bigint FROM memory_claims
                         WHERE scope = ANY($1::text[]) AND forgotten_at IS NULL AND epistemic_status NOT IN ('superseded', 'retracted')),
                        (SELECT COUNT(*)::bigint FROM memory_documents
                         WHERE scope = ANY($1::text[]) AND forgotten_at IS NULL),
                        (SELECT COUNT(*)::bigint FROM memory_conflicts
                         WHERE scope = ANY($1::text[]) AND status = 'open'),
                        (SELECT COUNT(*)::bigint FROM memory_notes
                         WHERE scope = ANY($1::text[]) AND forgotten_at IS NULL AND trust_level = 'external_untrusted')
                        + (SELECT COUNT(*)::bigint FROM memory_claims
                           WHERE scope = ANY($1::text[]) AND forgotten_at IS NULL AND trust_level = 'external_untrusted')
                        + (SELECT COUNT(*)::bigint FROM memory_documents
                           WHERE scope = ANY($1::text[]) AND forgotten_at IS NULL AND trust_level = 'external_untrusted')",
                    &[&allowed_scopes],
                )
                .await
                .map_err(|err| self.map_pg_err(&err))?;

            let (index_status, _, watermark_seq, last_rebuild_at) =
                compute_aggregate_index_status(&conn, &allowed_scopes, self).await?;

            Ok(MemoryStatsSummary {
                scope: requested_scope,
                sources_count: u64::try_from(row.get::<_, i64>(0)).unwrap_or(0),
                active_notes_count: u64::try_from(row.get::<_, i64>(1)).unwrap_or(0),
                active_claims_count: u64::try_from(row.get::<_, i64>(2)).unwrap_or(0),
                documents_count: u64::try_from(row.get::<_, i64>(3)).unwrap_or(0),
                open_conflicts_count: u64::try_from(row.get::<_, i64>(4)).unwrap_or(0),
                imported_untrusted_count: u64::try_from(row.get::<_, i64>(5)).unwrap_or(0),
                index_status,
                watermark_seq,
                last_rebuild_at,
            })
        };

        time::timeout(self.inner.op_timeout, op_fut)
            .await
            .map_err(|_| DennettError::Timeout {
                operation: String::from("memory_service::get_stats"),
            })?
    }

    /// Forgets a memory entity with immediate content scrub, search-index purge, and audit tombstone.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::ForbiddenScope`],
    /// [`DennettError::NotFound`], [`DennettError::IdempotencyMismatch`], or
    /// [`DennettError::DatabaseUnavailable`].
    pub async fn forget(
        &self,
        principal: &AuthPrincipal,
        correlation_id: &CorrelationId,
        req: ForgetRequest,
    ) -> Result<ForgetResponse, DennettError> {
        execute_forget(self, principal, correlation_id, req).await
    }

    /// Lists content-free [`dennett_contracts::ForgetTombstone`] audit records for authorized scopes.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::ForbiddenScope`] or [`DennettError::DatabaseUnavailable`].
    pub async fn list_tombstones(
        &self,
        principal: &AuthPrincipal,
        scope: Option<&MemoryScope>,
    ) -> Result<ListTombstonesResponse, DennettError> {
        execute_list_tombstones(self, principal, scope).await
    }

    /// Exports one authorized scope as a deterministic, secret-stripped Contract J package (`Export`).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::ForbiddenScope`], or
    /// [`DennettError::DatabaseUnavailable`].
    pub async fn export_package(
        &self,
        principal: &AuthPrincipal,
        correlation_id: &CorrelationId,
        req: ExportRequest,
    ) -> Result<ExportResponse, DennettError> {
        execute_export_package(self, principal, correlation_id, req).await
    }

    /// Imports an external Contract J package strictly as `external_untrusted` data (`Write`).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::ForbiddenScope`], or
    /// [`DennettError::DatabaseUnavailable`].
    pub async fn import_package(
        &self,
        principal: &AuthPrincipal,
        correlation_id: &CorrelationId,
        req: ImportRequest,
    ) -> Result<ImportResponse, DennettError> {
        execute_import_package(self, principal, correlation_id, req).await
    }

    pub(crate) async fn get_conn(
        &self,
        timeout: Duration,
    ) -> Result<deadpool_postgres::Object, DennettError> {
        time::timeout(timeout, self.inner.pool.get())
            .await
            .map_err(|_| DennettError::DatabaseUnavailable {
                detail: String::from("timed out acquiring PostgreSQL connection"),
            })?
            .map_err(|err| DennettError::DatabaseUnavailable {
                detail: self.inner.pg_target.sanitize_error(&err.to_string()),
            })
    }

    pub(crate) fn map_pg_err(&self, err: &tokio_postgres::Error) -> DennettError {
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

/// Capabilities advertised by standalone `dennett-memoryd`.
#[must_use]
pub fn d1_memoryd_capabilities() -> Vec<String> {
    vec![
        String::from("healthz"),
        String::from("postgres-connectivity"),
        String::from("memory-v1"),
        String::from("memory-v2"),
        String::from("scoped-auth"),
        String::from("evidence-multi-anchor"),
        String::from("note-revisions"),
        String::from("idempotency"),
        String::from("claims-conflicts"),
        String::from("dossiers"),
        String::from("consolidation"),
        String::from("rebuildable-fts"),
        String::from("forgetting"),
        String::from("portable-export-import"),
    ]
}

pub(crate) struct StoredIdempotencyRecord {
    pub(crate) event_id: MemoryEventId,
    pub(crate) kind: MemoryEventKind,
    pub(crate) payload_fingerprint: String,
    pub(crate) target_entity_id: Uuid,
    pub(crate) target_revision: Option<i64>,
}

pub(crate) fn require_scope_capability(
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
pub(crate) fn require_resource_scope_capability(
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

pub(crate) fn resolve_search_scopes(
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

pub(crate) struct NewMemoryEvent<'a> {
    pub(crate) event_id: MemoryEventId,
    pub(crate) scope: &'a MemoryScope,
    pub(crate) kind: MemoryEventKind,
    pub(crate) principal_id: &'a PrincipalId,
    pub(crate) correlation_id: &'a CorrelationId,
    pub(crate) idempotency_key: &'a IdempotencyKey,
    pub(crate) fingerprint: &'a Sha256Hex,
    pub(crate) target_entity_id: Uuid,
    pub(crate) target_revision: Option<i64>,
    pub(crate) observed_ms: Option<i64>,
}

/// Inserts a row into `memory_events`, returning `Ok(Some(stream_seq))` on success or `Ok(None)`
/// when a concurrent idempotency race tripped `memory_events_scope_idempotency_uniq`.
pub(crate) async fn insert_memory_event<C: GenericClient>(
    client: &C,
    ev: NewMemoryEvent<'_>,
    svc: &MemoryService,
) -> Result<Option<i64>, DennettError> {
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
        Ok(_) => Ok(Some(stream_seq)),
        Err(err) if is_idempotency_unique_violation(&err) => Ok(None),
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
) -> Result<(MemoryScope, i64, AdmissionStatus), DennettError> {
    let locked = client
        .query_opt(
            "SELECT scope, current_revision, admission_status
             FROM memory_notes
             WHERE note_id = $1 AND forgotten_at IS NULL
             FOR UPDATE",
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
    let admission_status: AdmissionStatus = note_row.get::<_, String>(2).parse()?;

    require_resource_scope_capability(principal, &stored_scope, MemoryCapability::Write)?;
    if stored_scope != req.scope {
        return Err(DennettError::ForbiddenScope {
            scope: req.scope.to_string(),
            reason: String::from("note does not belong to the requested scope"),
        });
    }

    Ok((stored_scope, actual_revision, admission_status))
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
             WHERE note_id = $10 AND current_revision = $11 AND forgotten_at IS NULL",
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

pub(crate) fn scope_stream_id(scope: &MemoryScope) -> String {
    format!("memory:scope:{}", scope.as_str())
}

fn is_idempotency_unique_violation(err: &tokio_postgres::Error) -> bool {
    err.as_db_error().is_some_and(|db_err| {
        db_err.code() == &SqlState::UNIQUE_VIOLATION
            && db_err.constraint() == Some(IDEMPOTENCY_UNIQUE_CONSTRAINT)
    })
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

pub(crate) async fn lookup_idempotency<C: GenericClient>(
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

pub(crate) async fn verify_evidence_links_in_scope<C: GenericClient>(
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
            "SELECT evidence_id, scope
             FROM evidence_objects
             WHERE evidence_id = ANY($1::uuid[]) AND forgotten_at IS NULL",
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

pub(crate) async fn insert_evidence_links<C: GenericClient>(
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
             WHERE evidence_id = $1 AND forgotten_at IS NULL",
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
                    TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint,
                    admission_status, salience, topic_key,
                    trust_level, instruction_active
             FROM memory_notes
             WHERE note_id = $1 AND forgotten_at IS NULL",
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
                    TRUNC(EXTRACT(EPOCH FROM r.created_at) * 1000)::bigint,
                    n.admission_status, n.salience, n.topic_key,
                    n.trust_level, n.instruction_active
             FROM memory_notes n
             JOIN memory_note_revisions r ON r.note_id = n.note_id
             WHERE n.note_id = $1 AND r.revision = $2 AND n.forgotten_at IS NULL",
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

pub(crate) async fn fetch_links_for_events<C: GenericClient>(
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

pub(crate) fn decode_note_row(
    row: &Row,
    evidence_links: Vec<EventEvidenceLink>,
) -> Result<MemoryNote, DennettError> {
    let salience_i16: i16 = row.get(14);
    let salience = u8::try_from(salience_i16.clamp(0, i16::from(MAX_NOTE_SALIENCE)))
        .unwrap_or(DEFAULT_NOTE_SALIENCE);

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
        admission_status: row.get::<_, String>(13).parse()?,
        salience,
        topic_key: row.get(15),
        trust_level: row.get::<_, String>(16).parse()?,
        instruction_active: row.get(17),
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
    use crate::search::escape_like_pattern;

    #[test]
    fn escape_like_pattern_escapes_wildcards_and_backslashes() {
        assert_eq!(escape_like_pattern("plain"), "plain");
        assert_eq!(escape_like_pattern("100%_done\\ok"), "100\\%\\_done\\\\ok");
    }
}
