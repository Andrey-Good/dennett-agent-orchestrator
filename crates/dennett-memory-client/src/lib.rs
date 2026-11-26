//! Typed async HTTP client for standalone `dennett-memoryd` (`/v1/memory/*` and `/healthz`).
//!
//! Architectural boundary (Architecture 80 §6): depends ONLY on `dennett-contracts` and
//! HTTP/JSON transport crates — NEVER on `dennett-memory-core` or PostgreSQL drivers.

use core::fmt;
use std::time::Duration;

use dennett_contracts::redact::{REDACTED_PLACEHOLDER, redact_secrets};
use dennett_contracts::{
    ApiErrorResponse, ClaimId, ConsolidationJob, ConsolidationJobId, CorrectNoteRequest,
    CorrelationId, CreateClaimRequest, CreateConsolidationJobRequest, CreateDocumentRequest,
    CreateNoteRequest, DennettError, DocumentId, EvidenceId, EvidenceObject, ExportRequest,
    ExportResponse, ForgetRequest, ForgetResponse, GetDocumentVersionsResponse, ImportRequest,
    ImportResponse, IndexState, IngestEvidenceRequest, IngestEvidenceResponse, ListClaimsResponse,
    ListConflictsResponse, ListConsolidationJobsResponse, ListDocumentsResponse,
    ListTombstonesResponse, MemoryClaim, MemoryConflict, MemoryDocument, MemoryEvent,
    MemoryEventId, MemoryNote, MemoryNoteId, MemoryScope, MemoryStatsSummary, NoteHistoryResponse,
    NoteMutationResponse, ResolveConflictRequest, SearchNotesRequest, SearchNotesResponse,
    ServiceHealth, SupersedeClaimRequest, TriggerIndexRebuildRequest, TriggerIndexRebuildResponse,
    UpdateDocumentRequest,
};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Method, StatusCode};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Default total request timeout for [`MemoryClient`] HTTP calls.
pub const DEFAULT_CLIENT_TIMEOUT: Duration = Duration::from_secs(15);
/// Default TCP connect timeout for [`MemoryClient`].
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Maximum HTTP response body size accepted from `dennett-memoryd` (32 MiB).
///
/// Prevents a misconfigured or adversarial upstream from exhausting client memory.
pub const MAX_RESPONSE_BODY_BYTES: usize = 32 * 1024 * 1024;

static HEADER_CORRELATION_ID: HeaderName = HeaderName::from_static("x-correlation-id");

/// Typed asynchronous HTTP client communicating with a standalone `dennett-memoryd` daemon.
///
/// Cheap to clone via internal connection pool `Arc`. `Debug` output strictly redacts the
/// configured bearer token.
#[derive(Clone)]
pub struct MemoryClient {
    base_url: String,
    bearer_token: Option<String>,
    http: Client,
}

impl fmt::Debug for MemoryClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemoryClient")
            .field("base_url", &self.base_url)
            .field(
                "bearer_token",
                &self.bearer_token.as_ref().map(|_| REDACTED_PLACEHOLDER),
            )
            .finish_non_exhaustive()
    }
}

impl MemoryClient {
    /// Creates a new authenticated [`MemoryClient`] targeting `base_url` with `bearer_token`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `base_url` does not start with `http://` or
    /// `https://`, if `bearer_token` is empty or contains whitespace/control characters, or
    /// [`DennettError::Internal`] if the HTTP client fails to initialize.
    pub fn new(
        base_url: impl AsRef<str>,
        bearer_token: impl Into<String>,
    ) -> Result<Self, DennettError> {
        let token = bearer_token.into();
        if token.is_empty()
            || token
                .bytes()
                .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
        {
            return Err(DennettError::InvalidInput {
                field: String::from("bearer_token"),
                reason: String::from(
                    "bearer token must be non-empty without whitespace or control characters",
                ),
            });
        }
        Self::build(base_url.as_ref(), Some(token), DEFAULT_CLIENT_TIMEOUT)
    }

    /// Creates an unauthenticated [`MemoryClient`] suitable for `/healthz` probes or negative
    /// authentication checks.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `base_url` is malformed.
    pub fn unauthenticated(base_url: impl AsRef<str>) -> Result<Self, DennettError> {
        Self::build(base_url.as_ref(), None, DEFAULT_CLIENT_TIMEOUT)
    }

    /// Creates a [`MemoryClient`] with a custom request timeout and optional bearer token.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `base_url` or `bearer_token` is invalid.
    pub fn with_timeout(
        base_url: impl AsRef<str>,
        bearer_token: Option<String>,
        timeout: Duration,
    ) -> Result<Self, DennettError> {
        if let Some(ref token) = bearer_token
            && (token.is_empty()
                || token
                    .bytes()
                    .any(|b| b.is_ascii_whitespace() || b.is_ascii_control()))
        {
            return Err(DennettError::InvalidInput {
                field: String::from("bearer_token"),
                reason: String::from(
                    "bearer token must be non-empty without whitespace or control characters",
                ),
            });
        }
        Self::build(base_url.as_ref(), bearer_token, timeout)
    }

    fn build(
        raw_base_url: &str,
        bearer_token: Option<String>,
        timeout: Duration,
    ) -> Result<Self, DennettError> {
        let trimmed = raw_base_url.trim().trim_end_matches('/');
        if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
            return Err(DennettError::InvalidInput {
                field: String::from("base_url"),
                reason: String::from("must start with http:// or https://"),
            });
        }
        if trimmed.len() <= "http://".len() || trimmed.bytes().any(|b| b.is_ascii_whitespace()) {
            return Err(DennettError::InvalidInput {
                field: String::from("base_url"),
                reason: String::from("must be a valid HTTP URL without whitespace"),
            });
        }

        let http = Client::builder()
            .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
            .timeout(timeout)
            .no_proxy()
            .build()
            .map_err(|err| DennettError::Internal {
                detail: format!("failed to construct HTTP client: {err}"),
            })?;

        Ok(Self {
            base_url: trimmed.to_owned(),
            bearer_token,
            http,
        })
    }

    /// Returns the normalized base URL (without trailing slash).
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Redacts generic secret patterns and this client's configured bearer token from `text`.
    #[must_use]
    pub fn redact_text(&self, text: &str) -> String {
        self.redact(text)
    }

    /// Queries `GET /healthz` on the target `dennett-memoryd`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::DatabaseUnavailable`] when `memoryd` reports `503` or cannot
    /// be reached over the network.
    pub async fn healthz(
        &self,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ServiceHealth, DennettError> {
        self.request_no_body(Method::GET, "/healthz", correlation_id)
            .await
    }

    /// Calls `POST /v1/memory/evidence` to ingest an immutable evidence object.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by `dennett-memoryd` or transport failure.
    pub async fn ingest_evidence(
        &self,
        req: &IngestEvidenceRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<IngestEvidenceResponse, DennettError> {
        self.request_json(Method::POST, "/v1/memory/evidence", req, correlation_id)
            .await
    }

    /// Calls `GET /v1/memory/evidence/{evidence_id}` to fetch an evidence object.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by `dennett-memoryd` or transport failure.
    pub async fn get_evidence(
        &self,
        evidence_id: EvidenceId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<EvidenceObject, DennettError> {
        let path = format!("/v1/memory/evidence/{evidence_id}");
        self.request_no_body(Method::GET, &path, correlation_id)
            .await
    }

    /// Calls `POST /v1/memory/notes` to create a revision-1 memory note.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by `dennett-memoryd` or transport failure.
    pub async fn create_note(
        &self,
        req: &CreateNoteRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<NoteMutationResponse, DennettError> {
        self.request_json(Method::POST, "/v1/memory/notes", req, correlation_id)
            .await
    }

    /// Calls `GET /v1/memory/notes/{note_id}` to read the current projection of a note.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by `dennett-memoryd` or transport failure.
    pub async fn get_note_current(
        &self,
        note_id: MemoryNoteId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<MemoryNote, DennettError> {
        let path = format!("/v1/memory/notes/{note_id}");
        self.request_no_body(Method::GET, &path, correlation_id)
            .await
    }

    /// Calls `GET /v1/memory/notes/{note_id}/history` to read all immutable revisions of a note.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by `dennett-memoryd` or transport failure.
    pub async fn get_note_history(
        &self,
        note_id: MemoryNoteId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<NoteHistoryResponse, DennettError> {
        let path = format!("/v1/memory/notes/{note_id}/history");
        self.request_no_body(Method::GET, &path, correlation_id)
            .await
    }

    /// Calls `POST /v1/memory/notes/{note_id}/corrections` to append an immutable correction.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by `dennett-memoryd` or transport failure.
    pub async fn correct_note(
        &self,
        req: &CorrectNoteRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<NoteMutationResponse, DennettError> {
        let path = format!("/v1/memory/notes/{}/corrections", req.note_id);
        self.request_json(Method::POST, &path, req, correlation_id)
            .await
    }

    /// Calls `POST /v1/memory/notes/search` to search notes strictly within authorized scopes.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by `dennett-memoryd` or transport failure.
    pub async fn search_notes(
        &self,
        req: &SearchNotesRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<SearchNotesResponse, DennettError> {
        self.request_json(Method::POST, "/v1/memory/notes/search", req, correlation_id)
            .await
    }

    /// Calls `GET /v1/memory/events/{event_id}` to fetch an event and its evidence links.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by `dennett-memoryd` or transport failure.
    pub async fn get_event(
        &self,
        event_id: MemoryEventId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<MemoryEvent, DennettError> {
        let path = format!("/v1/memory/events/{event_id}");
        self.request_no_body(Method::GET, &path, correlation_id)
            .await
    }

    // ------------------------------------------------------------------------
    // Phase D3 Memory Fabric Endpoints
    // ------------------------------------------------------------------------

    /// Calls `POST /v1/memory/claims` to create a structured [`MemoryClaim`].
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by `dennett-memoryd` or transport failure.
    pub async fn create_claim(
        &self,
        req: &CreateClaimRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<MemoryClaim, DennettError> {
        self.request_json(Method::POST, "/v1/memory/claims", req, correlation_id)
            .await
    }

    /// Calls `GET /v1/memory/claims/{claim_id}` to fetch a single [`MemoryClaim`].
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by `dennett-memoryd` or transport failure.
    pub async fn get_claim(
        &self,
        claim_id: ClaimId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<MemoryClaim, DennettError> {
        let path = format!("/v1/memory/claims/{claim_id}");
        self.request_no_body(Method::GET, &path, correlation_id)
            .await
    }

    /// Calls `POST /v1/memory/claims/{claim_id}/supersede` to supersede an existing claim.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by `dennett-memoryd` or transport failure.
    pub async fn supersede_claim(
        &self,
        req: &SupersedeClaimRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<MemoryClaim, DennettError> {
        let path = format!("/v1/memory/claims/{}/supersede", req.claim_id);
        self.request_json(Method::POST, &path, req, correlation_id)
            .await
    }

    /// Calls `GET /v1/memory/claims` with optional `scope` and `topic_key` filters.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by `dennett-memoryd` or transport failure.
    pub async fn list_claims(
        &self,
        scope: Option<&MemoryScope>,
        topic_key: Option<&str>,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ListClaimsResponse, DennettError> {
        let mut params = Vec::new();
        if let Some(sc) = scope {
            params.push(format!("scope={}", encode_query_component(sc.as_str())));
        }
        if let Some(topic) = topic_key.map(str::trim).filter(|t| !t.is_empty()) {
            params.push(format!("topic_key={}", encode_query_component(topic)));
        }
        let path = build_query_path("/v1/memory/claims", &params);
        self.request_no_body(Method::GET, &path, correlation_id)
            .await
    }

    /// Calls `GET /v1/memory/claims/by-topic?scope=...&topic_key=...` to list claims for one topic.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by `dennett-memoryd` or transport failure.
    pub async fn list_claims_by_topic(
        &self,
        scope: &MemoryScope,
        topic_key: &str,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ListClaimsResponse, DennettError> {
        let params = [
            format!("scope={}", encode_query_component(scope.as_str())),
            format!("topic_key={}", encode_query_component(topic_key)),
        ];
        let path = build_query_path("/v1/memory/claims/by-topic", &params);
        self.request_no_body(Method::GET, &path, correlation_id)
            .await
    }

    /// Calls `GET /v1/memory/conflicts` with an optional `scope` filter.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by `dennett-memoryd` or transport failure.
    pub async fn list_open_conflicts(
        &self,
        scope: Option<&MemoryScope>,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ListConflictsResponse, DennettError> {
        let path = scoped_query_path("/v1/memory/conflicts", scope);
        self.request_no_body(Method::GET, &path, correlation_id)
            .await
    }

    /// Calls `POST /v1/memory/conflicts/{conflict_id}/resolve` to adjudicate an open conflict.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by `dennett-memoryd` or transport failure.
    pub async fn resolve_conflict(
        &self,
        req: &ResolveConflictRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<MemoryConflict, DennettError> {
        let path = format!("/v1/memory/conflicts/{}/resolve", req.conflict_id);
        self.request_json(Method::POST, &path, req, correlation_id)
            .await
    }

    /// Calls `GET /v1/memory/documents` with an optional `scope` filter.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by `dennett-memoryd` or transport failure.
    pub async fn list_documents(
        &self,
        scope: Option<&MemoryScope>,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ListDocumentsResponse, DennettError> {
        let path = scoped_query_path("/v1/memory/documents", scope);
        self.request_no_body(Method::GET, &path, correlation_id)
            .await
    }

    /// Calls `POST /v1/memory/documents` to create a version-1 [`MemoryDocument`] dossier.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by `dennett-memoryd` or transport failure.
    pub async fn create_document(
        &self,
        req: &CreateDocumentRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<MemoryDocument, DennettError> {
        self.request_json(Method::POST, "/v1/memory/documents", req, correlation_id)
            .await
    }

    /// Calls `GET /v1/memory/documents/{document_id}` to fetch a [`MemoryDocument`].
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by `dennett-memoryd` or transport failure.
    pub async fn get_document(
        &self,
        document_id: DocumentId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<MemoryDocument, DennettError> {
        let path = format!("/v1/memory/documents/{document_id}");
        self.request_no_body(Method::GET, &path, correlation_id)
            .await
    }

    /// Calls `PUT /v1/memory/documents/{document_id}` to update a dossier with optimistic concurrency.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by `dennett-memoryd` or transport failure.
    pub async fn update_document(
        &self,
        req: &UpdateDocumentRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<MemoryDocument, DennettError> {
        let path = format!("/v1/memory/documents/{}", req.document_id);
        self.request_json(Method::PUT, &path, req, correlation_id)
            .await
    }

    /// Calls `GET /v1/memory/documents/{document_id}/versions` to fetch full immutable version history.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by `dennett-memoryd` or transport failure.
    pub async fn get_document_versions(
        &self,
        document_id: DocumentId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<GetDocumentVersionsResponse, DennettError> {
        let path = format!("/v1/memory/documents/{document_id}/versions");
        self.request_no_body(Method::GET, &path, correlation_id)
            .await
    }

    /// Calls `POST /v1/memory/forget` to forget a memory entity with search purge and audit tombstone.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by `dennett-memoryd` or transport failure.
    pub async fn forget(
        &self,
        req: &ForgetRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ForgetResponse, DennettError> {
        self.request_json(Method::POST, "/v1/memory/forget", req, correlation_id)
            .await
    }

    /// Calls `GET /v1/memory/forget/tombstones` with an optional `scope` filter.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by `dennett-memoryd` or transport failure.
    pub async fn list_tombstones(
        &self,
        scope: Option<&MemoryScope>,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ListTombstonesResponse, DennettError> {
        self.list_tombstones_since(scope, None, None, correlation_id)
            .await
    }

    /// Calls `GET /v1/memory/forget/tombstones` with optional `scope`, monotonic `after_seq`
    /// cursor (V15 commit-barrier stream), and `limit` query parameters.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by `dennett-memoryd` or transport failure.
    pub async fn list_tombstones_since(
        &self,
        scope: Option<&MemoryScope>,
        after_seq: Option<i64>,
        limit: Option<u32>,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ListTombstonesResponse, DennettError> {
        let mut params = Vec::new();
        if let Some(sc) = scope {
            params.push(format!("scope={}", encode_query_component(sc.as_str())));
        }
        if let Some(seq) = after_seq {
            params.push(format!("after_seq={seq}"));
        }
        if let Some(lim) = limit {
            params.push(format!("limit={lim}"));
        }
        let path = build_query_path("/v1/memory/forget/tombstones", &params);
        self.request_no_body(Method::GET, &path, correlation_id)
            .await
    }

    /// Calls `POST /v1/memory/export` to export a deterministic, secret-stripped package (Contract J).
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by `dennett-memoryd` or transport failure.
    pub async fn export_package(
        &self,
        req: &ExportRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ExportResponse, DennettError> {
        self.request_json(Method::POST, "/v1/memory/export", req, correlation_id)
            .await
    }

    /// Calls `POST /v1/memory/import` to import an external memory package as `external_untrusted` data.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by `dennett-memoryd` or transport failure.
    pub async fn import_package(
        &self,
        req: &ImportRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ImportResponse, DennettError> {
        self.request_json(Method::POST, "/v1/memory/import", req, correlation_id)
            .await
    }

    /// Calls `GET /v1/memory/index/status` with an optional `scope` filter.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by `dennett-memoryd` or transport failure.
    pub async fn get_index_status(
        &self,
        scope: Option<&MemoryScope>,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<IndexState, DennettError> {
        let path = scoped_query_path("/v1/memory/index/status", scope);
        self.request_no_body(Method::GET, &path, correlation_id)
            .await
    }

    /// Calls `POST /v1/memory/index/rebuild` to rebuild the separable search index (`Admin` capability).
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by `dennett-memoryd` or transport failure.
    pub async fn trigger_index_rebuild(
        &self,
        req: &TriggerIndexRebuildRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<TriggerIndexRebuildResponse, DennettError> {
        self.request_json(
            Method::POST,
            "/v1/memory/index/rebuild",
            req,
            correlation_id,
        )
        .await
    }

    /// Calls `GET /v1/memory/stats` with an optional `scope` filter for live UI/diagnostics summary counts.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by `dennett-memoryd` or transport failure.
    pub async fn get_stats(
        &self,
        scope: Option<&MemoryScope>,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<MemoryStatsSummary, DennettError> {
        let path = scoped_query_path("/v1/memory/stats", scope);
        self.request_no_body(Method::GET, &path, correlation_id)
            .await
    }

    /// Calls `GET /v1/memory/consolidation/jobs` with an optional `scope` filter.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by `dennett-memoryd` or transport failure.
    pub async fn list_consolidation_jobs(
        &self,
        scope: Option<&MemoryScope>,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ListConsolidationJobsResponse, DennettError> {
        let path = scoped_query_path("/v1/memory/consolidation/jobs", scope);
        self.request_no_body(Method::GET, &path, correlation_id)
            .await
    }

    /// Calls `POST /v1/memory/consolidation/jobs` to create an interruptible consolidation job.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by `dennett-memoryd` or transport failure.
    pub async fn create_consolidation_job(
        &self,
        req: &CreateConsolidationJobRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ConsolidationJob, DennettError> {
        self.request_json(
            Method::POST,
            "/v1/memory/consolidation/jobs",
            req,
            correlation_id,
        )
        .await
    }

    /// Calls `POST /v1/memory/consolidation/jobs/{job_id}/pause` to pause a job at its checkpoint.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by `dennett-memoryd` or transport failure.
    pub async fn pause_consolidation_job(
        &self,
        job_id: ConsolidationJobId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ConsolidationJob, DennettError> {
        let path = format!("/v1/memory/consolidation/jobs/{job_id}/pause");
        self.request_no_body(Method::POST, &path, correlation_id)
            .await
    }

    /// Calls `POST /v1/memory/consolidation/jobs/{job_id}/resume` to resume a paused or failed job.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by `dennett-memoryd` or transport failure.
    pub async fn resume_consolidation_job(
        &self,
        job_id: ConsolidationJobId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ConsolidationJob, DennettError> {
        let path = format!("/v1/memory/consolidation/jobs/{job_id}/resume");
        self.request_no_body(Method::POST, &path, correlation_id)
            .await
    }

    async fn request_no_body<Resp: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<Resp, DennettError> {
        let url = format!("{}{path}", self.base_url);
        let headers = self.build_headers(correlation_id)?;
        let resp = self
            .http
            .request(method, &url)
            .headers(headers)
            .send()
            .await
            .map_err(|err| self.map_transport_error(&err))?;
        self.decode_response(resp).await
    }

    async fn request_json<Req: Serialize, Resp: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: &Req,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<Resp, DennettError> {
        let url = format!("{}{path}", self.base_url);
        let headers = self.build_headers(correlation_id)?;
        let resp = self
            .http
            .request(method, &url)
            .headers(headers)
            .json(body)
            .send()
            .await
            .map_err(|err| self.map_transport_error(&err))?;
        self.decode_response(resp).await
    }

    fn build_headers(
        &self,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<HeaderMap, DennettError> {
        let mut headers = HeaderMap::new();
        let cid = correlation_id
            .cloned()
            .unwrap_or_else(CorrelationId::generate);
        let cid_val =
            HeaderValue::from_str(cid.as_str()).map_err(|_| DennettError::InvalidInput {
                field: String::from("correlation_id"),
                reason: String::from("invalid HTTP header value"),
            })?;
        headers.insert(HEADER_CORRELATION_ID.clone(), cid_val);

        if let Some(ref token) = self.bearer_token {
            let auth_header = format!("Bearer {token}");
            let auth_val =
                HeaderValue::from_str(&auth_header).map_err(|_| DennettError::InvalidInput {
                    field: String::from("bearer_token"),
                    reason: String::from("bearer token contains disallowed HTTP header characters"),
                })?;
            headers.insert(AUTHORIZATION, auth_val);
        }

        Ok(headers)
    }

    async fn decode_response<Resp: DeserializeOwned>(
        &self,
        resp: reqwest::Response,
    ) -> Result<Resp, DennettError> {
        self.decode_response_with_limit(resp, MAX_RESPONSE_BODY_BYTES)
            .await
    }

    async fn decode_response_with_limit<Resp: DeserializeOwned>(
        &self,
        mut resp: reqwest::Response,
        max_bytes: usize,
    ) -> Result<Resp, DennettError> {
        let status = resp.status();
        let body_bytes = self.read_bounded_body(&mut resp, max_bytes).await?;

        if status.is_success() {
            return serde_json::from_slice(&body_bytes).map_err(|err| DennettError::Internal {
                detail: self.redact(&format!(
                    "failed to decode JSON response from memoryd: {err}"
                )),
            });
        }

        if let Ok(envelope) = serde_json::from_slice::<ApiErrorResponse>(&body_bytes) {
            return Err(envelope.error);
        }
        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&body_bytes)
            && let Some(err_val) = value.get("error")
            && let Ok(parsed_err) = serde_json::from_value::<DennettError>(err_val.clone())
        {
            return Err(parsed_err);
        }
        if let Ok(direct_err) = serde_json::from_slice::<DennettError>(&body_bytes) {
            return Err(direct_err);
        }

        let safe_body = self.redact(&String::from_utf8_lossy(&body_bytes));
        Err(match status {
            StatusCode::UNAUTHORIZED => DennettError::Unauthorized {
                reason: if safe_body.is_empty() {
                    String::from("unauthorized")
                } else {
                    safe_body
                },
            },
            StatusCode::FORBIDDEN => DennettError::ForbiddenScope {
                scope: String::from("unknown"),
                reason: if safe_body.is_empty() {
                    String::from("forbidden")
                } else {
                    safe_body
                },
            },
            StatusCode::NOT_FOUND => DennettError::NotFound {
                resource: String::from("memory"),
                id: String::from("unknown"),
            },
            StatusCode::CONFLICT => DennettError::Conflict {
                resource: String::from("memory"),
                expected_revision: None,
                actual_revision: None,
                reason: safe_body,
            },
            StatusCode::SERVICE_UNAVAILABLE => {
                DennettError::DatabaseUnavailable { detail: safe_body }
            }
            StatusCode::GATEWAY_TIMEOUT | StatusCode::REQUEST_TIMEOUT => DennettError::Timeout {
                operation: String::from("memory_client"),
            },
            _ => DennettError::Internal {
                detail: format!("unexpected HTTP {}: {safe_body}", status.as_u16()),
            },
        })
    }

    async fn read_bounded_body(
        &self,
        resp: &mut reqwest::Response,
        max_bytes: usize,
    ) -> Result<Vec<u8>, DennettError> {
        if let Some(content_len) = resp.content_length()
            && content_len > max_bytes as u64
        {
            return Err(DennettError::Internal {
                detail: format!(
                    "memoryd response Content-Length ({content_len} bytes) exceeded client limit ({max_bytes} bytes)"
                ),
            });
        }

        let initial_cap = resp
            .content_length()
            .and_then(|len| usize::try_from(len).ok())
            .map_or(0, |len| len.min(max_bytes));
        let mut buf = Vec::with_capacity(initial_cap);

        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|err| self.map_transport_error(&err))?
        {
            if buf.len().saturating_add(chunk.len()) > max_bytes {
                return Err(DennettError::Internal {
                    detail: format!(
                        "memoryd response body exceeded client limit of {max_bytes} bytes"
                    ),
                });
            }
            buf.extend_from_slice(&chunk);
        }

        Ok(buf)
    }

    fn map_transport_error(&self, err: &reqwest::Error) -> DennettError {
        if err.is_timeout() {
            return DennettError::Timeout {
                operation: String::from("memory_client::http"),
            };
        }
        DennettError::DatabaseUnavailable {
            detail: self.redact(&format!("memoryd request failed: {err}")),
        }
    }

    fn redact(&self, text: &str) -> String {
        match self.bearer_token.as_deref() {
            Some(tok) => redact_secrets(text, &[tok]),
            None => redact_secrets(text, &[]),
        }
    }
}

fn scoped_query_path(base: &str, scope: Option<&MemoryScope>) -> String {
    match scope {
        Some(sc) => format!("{base}?scope={}", encode_query_component(sc.as_str())),
        None => base.to_owned(),
    }
}

fn build_query_path(base: &str, params: &[String]) -> String {
    if params.is_empty() {
        base.to_owned()
    } else {
        format!("{base}?{}", params.join("&"))
    }
}

fn encode_query_component(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~' | b'/') {
            out.push(char::from(byte));
        } else {
            use core::fmt::Write as _;
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

const _: () = {
    const fn assert_send_sync<T: Send + Sync + 'static>() {}
    assert_send_sync::<MemoryClient>();
};

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    #[test]
    fn memory_client_debug_redacts_bearer_token_and_validates_url() {
        let secret = "dnt_super_secret_bearer_token_987654";
        let client = MemoryClient::new("http://127.0.0.1:18080/", secret).expect("valid client");
        assert_eq!(client.base_url(), "http://127.0.0.1:18080");

        let dbg = format!("{client:?}");
        assert!(!dbg.contains(secret), "Debug leaked bearer token: {dbg}");
        assert!(dbg.contains(REDACTED_PLACEHOLDER));

        assert!(MemoryClient::new("ftp://127.0.0.1:18080", secret).is_err());
        assert!(MemoryClient::new("http://127.0.0.1:18080", "").is_err());
        assert!(MemoryClient::new("http://127.0.0.1:18080", "bad token").is_err());
    }

    async fn spawn_raw_http_server(raw_response: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback test server");
        let addr = listener.local_addr().expect("local addr");
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut req_buf = [0_u8; 2048];
                let _ = stream.read(&mut req_buf).await;
                let _ = stream.write_all(raw_response.as_bytes()).await;
                let _ = stream.shutdown().await;
            }
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn decode_response_rejects_oversized_content_length_and_chunked_streams() {
        // 1. Oversized Content-Length rejected immediately before reading payload.
        let url_cl = spawn_raw_http_server(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 9999\r\nConnection: close\r\n\r\n{}",
        )
        .await;
        let client = MemoryClient::unauthenticated(&url_cl).expect("client");
        let resp = client
            .http
            .get(format!("{url_cl}/healthz"))
            .send()
            .await
            .expect("send request");
        let err = client
            .decode_response_with_limit::<serde_json::Value>(resp, 64)
            .await;
        assert!(
            matches!(err, Err(DennettError::Internal { ref detail }) if detail.contains("exceeded client limit")),
            "expected bounded Content-Length rejection, got: {err:?}"
        );

        // 2. Oversized chunked response without Content-Length rejected while streaming.
        let url_chunked = spawn_raw_http_server(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n50\r\n{\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\":1}\r\n0\r\n\r\n",
        )
        .await;
        let client2 = MemoryClient::unauthenticated(&url_chunked).expect("client");
        let resp2 = client2
            .http
            .get(format!("{url_chunked}/healthz"))
            .send()
            .await
            .expect("send request");
        let err2 = client2
            .decode_response_with_limit::<serde_json::Value>(resp2, 64)
            .await;
        assert!(
            matches!(err2, Err(DennettError::Internal { ref detail }) if detail.contains("exceeded client limit")),
            "expected bounded chunked stream rejection, got: {err2:?}"
        );

        // 3. Response within limit decodes normally.
        let url_ok = spawn_raw_http_server(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}",
        )
        .await;
        let client3 = MemoryClient::unauthenticated(&url_ok).expect("client");
        let resp3 = client3
            .http
            .get(format!("{url_ok}/healthz"))
            .send()
            .await
            .expect("send request");
        let val = client3
            .decode_response_with_limit::<serde_json::Value>(resp3, 64)
            .await
            .expect("valid response within limit");
        assert_eq!(val["ok"], true);
    }
}
