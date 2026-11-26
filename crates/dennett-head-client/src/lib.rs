//! Typed async HTTP client for `dennett-head` (`/v1/*` and `/healthz`).
//!
//! Architectural boundary (Architecture 80 §6): depends ONLY on `dennett-contracts` and
//! HTTP/JSON transport crates — NEVER on `dennett-head`, `dennett-memory-core`, or PostgreSQL
//! drivers, so `dennettctl` and future `apps/desktop` UI reuse one canonical typed client.

use core::fmt;
use std::time::Duration;

use dennett_contracts::redact::{REDACTED_PLACEHOLDER, redact_secrets};
use dennett_contracts::{
    ActivateConnectionRequest, AgentConfigHistoryResponse, AgentId, AgentRecord, ApiErrorResponse,
    ArtifactId, ArtifactLifecycleRequest, ArtifactLifecycleResponse, ArtifactRecord,
    CancelRunResponse, CancelTaskResponse, CapabilityGrantId, CapabilityGrantRecord, CapabilityId,
    CapabilityKind, CapabilityProjectBinding, CapabilityRecord, CapabilityScopeKind,
    CommitMemoryNoteRequest, CommitMemoryNoteResponse, ConsolidationJob, ConsolidationJobId,
    CorrectNoteRequest, CorrelationId, CreateAgentRequest, CreateArtifactRequest,
    CreateCapabilityRequest, CreateClaimRequest, CreateConsolidationJobRequest,
    CreateDocumentRequest, CreatePolicyRuleRequest, CreateProjectRequest, CreateSessionRequest,
    CreateTaskRequest, DeleteCapabilityResponse, DeleteProviderConnectionResponse, DennettError,
    DiagnosticsResponse, DiscoverCapabilitiesRequest, DiscoverCapabilitiesResponse,
    DispatchEffectRequest, DispatchEffectResponse, DocumentId, EffectClaimId,
    EmergencyStopResponse, EnqueueFollowupRequest, EventStreamResponse, ExportRequest,
    ExportResponse, FlushOutboxResponse, FollowupId, ForgetRequest, ForgetResponse,
    ForkSessionRequest, GetDocumentVersionsResponse, HeadHealth, HeadMemorySearchRequest,
    HeadMemorySearchResponse, ImportRequest, ImportResponse, InboxCardId, IndexState,
    ListAgentMessagesResponse, ListAgentsResponse, ListArtifactsResponse, ListCapabilitiesResponse,
    ListCapabilityGrantsResponse, ListClaimsResponse, ListConflictsResponse,
    ListConsolidationJobsResponse, ListDocumentsResponse, ListEffectsResponse, ListInboxResponse,
    ListOutboxResponse, ListPermissionRequestsResponse, ListPolicyRulesResponse,
    ListProjectFilesResponse, ListProjectsResponse, ListProviderConnectionsResponse,
    ListSecretsResponse, ListSessionsResponse, ListTasksResponse, ListTombstonesResponse,
    LiveRunResponse, MemoryClaim, MemoryConflict, MemoryDocument, MemoryNoteId,
    MemoryOverviewResponse, MemoryScope, NoteHistoryResponse, NoteMutationResponse,
    PermissionRequestId, PolicyRule, PolicyRuleId, ProbeConnectionResponse,
    ProjectFileContentResponse, ProjectId, ProjectLifecycleResponse, ProjectRecord,
    ProviderConnectionId, ProviderConnectionRecord, RebindProjectRequest, ReconcileEffectRequest,
    ReconcileEffectResponse, ReplyTaskRequest, ResolveConflictRequest, ResolveInboxCardRequest,
    ResolveInboxCardResponse, ResolvePermissionRequest, ResolvePermissionResponse,
    ResumeFollowupsResponse, RevokeGrantRequest, RuntimeCatalogResponse, SchedulerTickRequest,
    SchedulerTickResponse, SecretMetadataRecord, SendAgentMessageRequest, SendAgentMessageResponse,
    SendMessageRequest, SendMessageResponse, SessionDetailResponse, SessionFollowupRecord,
    SessionId, SessionRecord, SessionSkillDetailResponse, SessionSkillSummary,
    SetProjectCapabilityBindingRequest, SetSecretRequest, Sha256Hex, StateSnapshotResponse,
    SteerSessionRequest, SupersedeClaimRequest, TaskDetailResponse, TaskId, TaskRecord,
    TaskSignalRequest, TaskSignalResponse, TriggerIndexRebuildRequest, TriggerIndexRebuildResponse,
    UpdateAgentConfigRequest, UpdateCapabilityRequest, UpdateDocumentRequest,
    UpdateFollowupRequest, UpdateProjectRequest, UpdateSessionRequest, UpdateTaskRequest,
    UpdateUserPreferencesRequest, UpsertProviderConnectionRequest, UserPreferences,
};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Method, StatusCode};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Default HTTP request timeout for [`HeadClient`] calls (covers live runtime turns).
pub const DEFAULT_HEAD_CLIENT_TIMEOUT: Duration = Duration::from_secs(180);
/// Default TCP connect timeout for [`HeadClient`].
pub const DEFAULT_HEAD_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Maximum HTTP response body size accepted from `dennett-head` (32 MiB).
pub const MAX_RESPONSE_BODY_BYTES: usize = 32 * 1024 * 1024;
/// Environment variable carrying an optional owner bearer token for `dennett-head`.
pub const HEAD_TOKEN_ENV: &str = "DENNETT_HEAD_TOKEN";

static HEADER_CORRELATION_ID: HeaderName = HeaderName::from_static("x-correlation-id");
static HEADER_CONTROL_TOKEN: HeaderName = HeaderName::from_static("x-dennett-control-token");
static HEADER_ARTIFACT_SHA256: HeaderName = HeaderName::from_static("x-dennett-artifact-sha256");

/// Immutable raw byte snapshot returned by `GET /v1/sessions/{session_id}/artifacts/{artifact_id}/content`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactContentResponse {
    /// Exact immutable bytes captured when the artifact revision was registered.
    pub bytes: Vec<u8>,
    /// Reported MIME media type (`Content-Type` response header).
    pub mime_type: String,
    /// Lowercase SHA-256 hex digest (`X-Dennett-Artifact-Sha256` response header).
    pub sha256: Sha256Hex,
}

/// Typed asynchronous HTTP client communicating with a `dennett-head` daemon.
#[derive(Clone)]
pub struct HeadClient {
    base_url: String,
    http: Client,
    control_token: Option<String>,
    owner_token: Option<String>,
}

impl fmt::Debug for HeadClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HeadClient")
            .field("base_url", &self.base_url)
            .field(
                "control_token",
                &self.control_token.as_ref().map(|_| REDACTED_PLACEHOLDER),
            )
            .field(
                "owner_token",
                &self.owner_token.as_ref().map(|_| REDACTED_PLACEHOLDER),
            )
            .finish_non_exhaustive()
    }
}

impl HeadClient {
    /// Creates a new [`HeadClient`] targeting `base_url` with [`DEFAULT_HEAD_CLIENT_TIMEOUT`].
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `base_url` is malformed or
    /// [`DennettError::Internal`] if HTTP client initialization fails.
    pub fn new(base_url: impl AsRef<str>) -> Result<Self, DennettError> {
        Self::with_timeout(base_url, DEFAULT_HEAD_CLIENT_TIMEOUT)
    }

    /// Creates a new [`HeadClient`] with an explicit per-request timeout.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `base_url` is malformed.
    pub fn with_timeout(
        base_url: impl AsRef<str>,
        timeout: Duration,
    ) -> Result<Self, DennettError> {
        let trimmed = base_url.as_ref().trim().trim_end_matches('/');
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
            .connect_timeout(DEFAULT_HEAD_CONNECT_TIMEOUT)
            .timeout(timeout)
            .no_proxy()
            .build()
            .map_err(|err| DennettError::Internal {
                detail: format!("failed constructing Head HTTP client: {err}"),
            })?;

        Ok(Self {
            base_url: trimmed.to_owned(),
            http,
            control_token: None,
            owner_token: None,
        })
    }

    /// Attaches a project-scoped `dct1.` control MCP bearer token (`X-Dennett-Control-Token`)
    /// to every outgoing HTTP request made by this client.
    #[must_use]
    pub fn with_control_token(mut self, token: impl Into<String>) -> Self {
        let trimmed = token.into().trim().to_owned();
        self.control_token = (!trimmed.is_empty()).then_some(trimmed);
        self
    }

    /// Attaches an owner bearer token (`Authorization: Bearer <token>`) to outgoing HTTP requests.
    #[must_use]
    pub fn with_owner_token(mut self, token: impl Into<String>) -> Self {
        let trimmed = token.into().trim().to_owned();
        self.owner_token = (!trimmed.is_empty()).then_some(trimmed);
        self
    }

    /// Attaches an optional owner bearer token (`Authorization: Bearer <token>`), clearing any
    /// previous token when `None` or blank.
    #[must_use]
    pub fn with_optional_owner_token(mut self, token: Option<String>) -> Self {
        self.owner_token = token.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty());
        self
    }

    /// Clears any configured owner bearer token (useful when testing unauthenticated `401` flows).
    #[must_use]
    pub fn without_owner_token(mut self) -> Self {
        self.owner_token = None;
        self
    }

    /// Loads an optional owner bearer token from [`HEAD_TOKEN_ENV`] (`DENNETT_HEAD_TOKEN`).
    #[must_use]
    pub fn with_owner_token_from_env(self) -> Self {
        self.with_optional_owner_token(std::env::var(HEAD_TOKEN_ENV).ok())
    }

    /// Updates or clears the configured owner bearer token in place.
    pub fn set_owner_token(&mut self, token: Option<String>) {
        self.owner_token = token.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty());
    }

    /// Returns the currently configured owner bearer token, when present.
    #[must_use]
    pub fn owner_token(&self) -> Option<&str> {
        self.owner_token.as_deref()
    }

    /// Returns the normalized base URL without trailing slash.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Queries `GET /healthz` on `dennett-head`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::DatabaseUnavailable`] when Head is unreachable or reports `503`.
    pub async fn healthz(
        &self,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<HeadHealth, DennettError> {
        self.request_no_body(Method::GET, "/healthz", correlation_id)
            .await
    }

    /// Queries `GET /v1/runtime/catalog` (with optional `?refresh=true`) for live runtime models,
    /// thinking-level choices, account label, and optional quota windows.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn get_runtime_catalog(
        &self,
        refresh: bool,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<RuntimeCatalogResponse, DennettError> {
        let path = if refresh {
            "/v1/runtime/catalog?refresh=true"
        } else {
            "/v1/runtime/catalog"
        };
        self.request_no_body(Method::GET, path, correlation_id)
            .await
    }

    /// Calls `POST /v1/agents`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn create_agent(
        &self,
        req: &CreateAgentRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<AgentRecord, DennettError> {
        self.request_json(Method::POST, "/v1/agents", req, correlation_id)
            .await
    }

    /// Calls `GET /v1/agents`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn list_agents(
        &self,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ListAgentsResponse, DennettError> {
        self.request_no_body(Method::GET, "/v1/agents", correlation_id)
            .await
    }

    /// Calls `GET /v1/agents/{agent_id}`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn get_agent(
        &self,
        agent_id: AgentId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<AgentRecord, DennettError> {
        self.request_no_body(
            Method::GET,
            &format!("/v1/agents/{agent_id}"),
            correlation_id,
        )
        .await
    }

    /// Calls `POST /v1/agents/{agent_id}/config-versions` (never changes `AgentId`).
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn update_agent_config(
        &self,
        agent_id: AgentId,
        req: &UpdateAgentConfigRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<AgentRecord, DennettError> {
        self.request_json(
            Method::POST,
            &format!("/v1/agents/{agent_id}/config-versions"),
            req,
            correlation_id,
        )
        .await
    }

    /// Calls `GET /v1/agents/{agent_id}/config-versions`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn get_agent_config_history(
        &self,
        agent_id: AgentId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<AgentConfigHistoryResponse, DennettError> {
        self.request_no_body(
            Method::GET,
            &format!("/v1/agents/{agent_id}/config-versions"),
            correlation_id,
        )
        .await
    }

    /// Calls `POST /v1/projects`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn create_project(
        &self,
        req: &CreateProjectRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ProjectRecord, DennettError> {
        self.request_json(Method::POST, "/v1/projects", req, correlation_id)
            .await
    }

    /// Calls `GET /v1/projects`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn list_projects(
        &self,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ListProjectsResponse, DennettError> {
        self.request_no_body(Method::GET, "/v1/projects", correlation_id)
            .await
    }

    /// Calls `GET /v1/projects/{project_id}`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn get_project(
        &self,
        project_id: ProjectId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ProjectRecord, DennettError> {
        self.request_no_body(
            Method::GET,
            &format!("/v1/projects/{project_id}"),
            correlation_id,
        )
        .await
    }

    /// Calls `PATCH /v1/projects/{project_id}` to rename a project or attach/rebind its folder.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn update_project(
        &self,
        project_id: ProjectId,
        req: &UpdateProjectRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ProjectRecord, DennettError> {
        self.request_json(
            Method::PATCH,
            &format!("/v1/projects/{project_id}"),
            req,
            correlation_id,
        )
        .await
    }

    /// Calls `GET /v1/projects/{project_id}/files` (with optional `?subpath=...`).
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn list_project_files(
        &self,
        project_id: ProjectId,
        subpath: Option<&str>,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ListProjectFilesResponse, DennettError> {
        let path = match subpath.map(str::trim).filter(|s| !s.is_empty()) {
            Some(sub) => format!(
                "/v1/projects/{project_id}/files?subpath={}",
                encode_query_component(sub)
            ),
            None => format!("/v1/projects/{project_id}/files"),
        };
        self.request_no_body(Method::GET, &path, correlation_id)
            .await
    }

    /// Calls `POST /v1/sessions`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn create_session(
        &self,
        req: &CreateSessionRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<SessionRecord, DennettError> {
        self.request_json(Method::POST, "/v1/sessions", req, correlation_id)
            .await
    }

    /// Calls `PATCH /v1/sessions/{session_id}` to rename a session inline.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn update_session(
        &self,
        session_id: SessionId,
        req: &UpdateSessionRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<SessionRecord, DennettError> {
        self.request_json(
            Method::PATCH,
            &format!("/v1/sessions/{session_id}"),
            req,
            correlation_id,
        )
        .await
    }

    /// Calls `GET /v1/sessions`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn list_sessions(
        &self,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ListSessionsResponse, DennettError> {
        self.request_no_body(Method::GET, "/v1/sessions", correlation_id)
            .await
    }

    /// Calls `GET /v1/sessions/{session_id}`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn get_session_detail(
        &self,
        session_id: SessionId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<SessionDetailResponse, DennettError> {
        self.request_no_body(
            Method::GET,
            &format!("/v1/sessions/{session_id}"),
            correlation_id,
        )
        .await
    }

    /// Calls `POST /v1/sessions/{session_id}/messages`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn send_message(
        &self,
        session_id: SessionId,
        req: &SendMessageRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<SendMessageResponse, DennettError> {
        self.request_json(
            Method::POST,
            &format!("/v1/sessions/{session_id}/messages"),
            req,
            correlation_id,
        )
        .await
    }

    /// Calls `POST /v1/sessions/{session_id}/cancel`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn cancel_run(
        &self,
        session_id: SessionId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<CancelRunResponse, DennettError> {
        self.request_no_body(
            Method::POST,
            &format!("/v1/sessions/{session_id}/cancel"),
            correlation_id,
        )
        .await
    }

    /// Calls `POST /v1/sessions/{session_id}/artifacts`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn create_artifact(
        &self,
        session_id: SessionId,
        req: &CreateArtifactRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ArtifactRecord, DennettError> {
        self.request_json(
            Method::POST,
            &format!("/v1/sessions/{session_id}/artifacts"),
            req,
            correlation_id,
        )
        .await
    }

    /// Calls `GET /v1/sessions/{session_id}/artifacts`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn list_artifacts(
        &self,
        session_id: SessionId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ListArtifactsResponse, DennettError> {
        self.request_no_body(
            Method::GET,
            &format!("/v1/sessions/{session_id}/artifacts"),
            correlation_id,
        )
        .await
    }

    /// Calls `GET /v1/sessions/{session_id}/artifacts/{artifact_id}/content` to download the exact
    /// content-addressed immutable bytes of one artifact revision.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head (`NotFound` on missing/deleted artifacts)
    /// or transport failure.
    pub async fn get_artifact_content(
        &self,
        session_id: SessionId,
        artifact_id: ArtifactId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ArtifactContentResponse, DennettError> {
        let url = format!(
            "{}/v1/sessions/{session_id}/artifacts/{artifact_id}/content",
            self.base_url
        );
        let headers = build_headers(
            correlation_id,
            self.control_token.as_deref(),
            self.owner_token.as_deref(),
        )?;
        let mut resp = self
            .http
            .request(Method::GET, &url)
            .headers(headers)
            .send()
            .await
            .map_err(|err| map_transport_error(&err))?;
        let status = resp.status();
        let mime_type = resp
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map_or_else(|| String::from("application/octet-stream"), str::to_owned);
        let sha_header = resp
            .headers()
            .get(&HEADER_ARTIFACT_SHA256)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let bytes = read_bounded_body(&mut resp, MAX_RESPONSE_BODY_BYTES).await?;

        if !status.is_success() {
            return Err(decode_error_response(status, &bytes));
        }
        let sha_raw = sha_header.ok_or_else(|| DennettError::Internal {
            detail: String::from(
                "dennett-head artifact content response missing X-Dennett-Artifact-Sha256 header",
            ),
        })?;
        let sha256 = Sha256Hex::try_new(sha_raw)?;
        Ok(ArtifactContentResponse {
            bytes,
            mime_type,
            sha256,
        })
    }

    /// Calls `POST /v1/policies`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn create_policy_rule(
        &self,
        req: &CreatePolicyRuleRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<PolicyRule, DennettError> {
        self.request_json(Method::POST, "/v1/policies", req, correlation_id)
            .await
    }

    /// Calls `GET /v1/policies`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn list_policy_rules(
        &self,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ListPolicyRulesResponse, DennettError> {
        self.request_no_body(Method::GET, "/v1/policies", correlation_id)
            .await
    }

    /// Calls `POST /v1/memory/commits`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn commit_memory_note(
        &self,
        req: &CommitMemoryNoteRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<CommitMemoryNoteResponse, DennettError> {
        self.request_json(Method::POST, "/v1/memory/commits", req, correlation_id)
            .await
    }

    /// Calls `GET /v1/memory/outbox`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn list_memory_outbox(
        &self,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ListOutboxResponse, DennettError> {
        self.request_no_body(Method::GET, "/v1/memory/outbox", correlation_id)
            .await
    }

    /// Calls `POST /v1/memory/outbox/flush`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn flush_memory_outbox(
        &self,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<FlushOutboxResponse, DennettError> {
        self.request_no_body(Method::POST, "/v1/memory/outbox/flush", correlation_id)
            .await
    }

    /// Calls `POST /v1/memory/search`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn search_memory(
        &self,
        req: &HeadMemorySearchRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<HeadMemorySearchResponse, DennettError> {
        self.request_json(Method::POST, "/v1/memory/search", req, correlation_id)
            .await
    }

    /// Calls `GET /v1/diagnostics`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn get_diagnostics(
        &self,
        limit: Option<i64>,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<DiagnosticsResponse, DennettError> {
        let path = match limit {
            Some(l) => format!("/v1/diagnostics?limit={l}"),
            None => String::from("/v1/diagnostics"),
        };
        self.request_no_body(Method::GET, &path, correlation_id)
            .await
    }

    /// Calls `GET /v1/projects?include_archived=...` (Contract C).
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn list_projects_with_options(
        &self,
        include_archived: bool,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ListProjectsResponse, DennettError> {
        let path = if include_archived {
            "/v1/projects?include_archived=true"
        } else {
            "/v1/projects"
        };
        self.request_no_body(Method::GET, path, correlation_id)
            .await
    }

    /// Calls `POST /v1/projects/{project_id}/rebind` (Contract C).
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn rebind_project(
        &self,
        project_id: ProjectId,
        req: &RebindProjectRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ProjectLifecycleResponse, DennettError> {
        self.request_json(
            Method::POST,
            &format!("/v1/projects/{project_id}/rebind"),
            req,
            correlation_id,
        )
        .await
    }

    /// Calls `POST /v1/projects/{project_id}/detach-workspace` (Contract C).
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn detach_project_workspace(
        &self,
        project_id: ProjectId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ProjectLifecycleResponse, DennettError> {
        self.request_no_body(
            Method::POST,
            &format!("/v1/projects/{project_id}/detach-workspace"),
            correlation_id,
        )
        .await
    }

    /// Calls `POST /v1/projects/{project_id}/archive` (Contract C).
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn archive_project(
        &self,
        project_id: ProjectId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ProjectLifecycleResponse, DennettError> {
        self.request_no_body(
            Method::POST,
            &format!("/v1/projects/{project_id}/archive"),
            correlation_id,
        )
        .await
    }

    /// Calls `POST /v1/projects/{project_id}/unarchive` (Contract C).
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn unarchive_project(
        &self,
        project_id: ProjectId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ProjectLifecycleResponse, DennettError> {
        self.request_no_body(
            Method::POST,
            &format!("/v1/projects/{project_id}/unarchive"),
            correlation_id,
        )
        .await
    }

    /// Calls `POST /v1/projects/{project_id}/remove` (Contract C — never deletes user workspace files).
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn remove_project(
        &self,
        project_id: ProjectId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ProjectLifecycleResponse, DennettError> {
        self.request_no_body(
            Method::POST,
            &format!("/v1/projects/{project_id}/remove"),
            correlation_id,
        )
        .await
    }

    /// Calls `DELETE /v1/projects/{project_id}?confirm=...` (Contract C — never deletes user workspace files).
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn delete_project(
        &self,
        project_id: ProjectId,
        confirm: bool,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ProjectLifecycleResponse, DennettError> {
        let path = format!("/v1/projects/{project_id}?confirm={confirm}");
        self.request_no_body(Method::DELETE, &path, correlation_id)
            .await
    }

    /// Calls `GET /v1/sessions/{session_id}/artifacts/{artifact_id_or_name}/revisions`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn list_artifact_revisions(
        &self,
        session_id: SessionId,
        artifact_id_or_name: &str,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ListArtifactsResponse, DennettError> {
        let encoded = encode_path_segment(artifact_id_or_name);
        self.request_no_body(
            Method::GET,
            &format!("/v1/sessions/{session_id}/artifacts/{encoded}/revisions"),
            correlation_id,
        )
        .await
    }

    /// Calls `POST /v1/sessions/{session_id}/artifacts/{artifact_id}/lifecycle` (Contract D).
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn apply_artifact_lifecycle(
        &self,
        session_id: SessionId,
        artifact_id: ArtifactId,
        req: &ArtifactLifecycleRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ArtifactLifecycleResponse, DennettError> {
        self.request_json(
            Method::POST,
            &format!("/v1/sessions/{session_id}/artifacts/{artifact_id}/lifecycle"),
            req,
            correlation_id,
        )
        .await
    }

    /// Calls `GET /v1/state/snapshot` (`REPEATABLE READ` consistent state + `snapshot_cursor`).
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn get_state_snapshot(
        &self,
        project_id: Option<ProjectId>,
        include_archived: bool,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<StateSnapshotResponse, DennettError> {
        let mut params: Vec<String> = Vec::new();
        if let Some(pid) = project_id {
            params.push(format!("project_id={pid}"));
        }
        if include_archived {
            params.push(String::from("include_archived=true"));
        }
        let path = build_query_path("/v1/state/snapshot", &params);
        self.request_no_body(Method::GET, &path, correlation_id)
            .await
    }

    /// Calls `GET /v1/events?after_cursor=...` for gapless cursor tailing (`V15`/`V18`).
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head (including `409 Conflict` on epoch/retention
    /// resync) or transport failure.
    pub async fn get_events(
        &self,
        after_cursor: i64,
        expected_epoch: Option<i64>,
        project_id: Option<ProjectId>,
        limit: Option<u32>,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<EventStreamResponse, DennettError> {
        let mut params = vec![format!("after_cursor={after_cursor}")];
        if let Some(ep) = expected_epoch {
            params.push(format!("epoch={ep}"));
        }
        if let Some(pid) = project_id {
            params.push(format!("project_id={pid}"));
        }
        if let Some(lim) = limit {
            params.push(format!("limit={lim}"));
        }
        let path = build_query_path("/v1/events", &params);
        self.request_no_body(Method::GET, &path, correlation_id)
            .await
    }

    /// Calls `POST /v1/tasks`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn create_task(
        &self,
        req: &CreateTaskRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<TaskRecord, DennettError> {
        self.request_json(Method::POST, "/v1/tasks", req, correlation_id)
            .await
    }

    /// Calls `GET /v1/tasks`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn list_tasks(
        &self,
        project_id: Option<ProjectId>,
        status: Option<&str>,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ListTasksResponse, DennettError> {
        let mut params = Vec::new();
        if let Some(pid) = project_id {
            params.push(format!("project_id={pid}"));
        }
        if let Some(st) = status.map(str::trim).filter(|s| !s.is_empty()) {
            params.push(format!("status={}", encode_query_component(st)));
        }
        let path = build_query_path("/v1/tasks", &params);
        self.request_no_body(Method::GET, &path, correlation_id)
            .await
    }

    /// Calls `GET /v1/tasks/{task_id}`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn get_task_detail(
        &self,
        task_id: TaskId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<TaskDetailResponse, DennettError> {
        self.request_no_body(Method::GET, &format!("/v1/tasks/{task_id}"), correlation_id)
            .await
    }

    /// Calls `PATCH /v1/tasks/{task_id}` (bumps `generation` on goal/schedule change, fencing old runs).
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn update_task(
        &self,
        task_id: TaskId,
        req: &UpdateTaskRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<TaskRecord, DennettError> {
        self.request_json(
            Method::PATCH,
            &format!("/v1/tasks/{task_id}"),
            req,
            correlation_id,
        )
        .await
    }

    /// Calls `POST /v1/tasks/{task_id}/signals` (idempotent by `(task_id, idempotency_key)`).
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn send_task_signal(
        &self,
        task_id: TaskId,
        req: &TaskSignalRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<TaskSignalResponse, DennettError> {
        self.request_json(
            Method::POST,
            &format!("/v1/tasks/{task_id}/signals"),
            req,
            correlation_id,
        )
        .await
    }

    /// Calls `POST /v1/tasks/{task_id}/reply` (answers native clarification and resumes task).
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn reply_to_task(
        &self,
        task_id: TaskId,
        req: &ReplyTaskRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<TaskRecord, DennettError> {
        self.request_json(
            Method::POST,
            &format!("/v1/tasks/{task_id}/reply"),
            req,
            correlation_id,
        )
        .await
    }

    /// Calls `POST /v1/tasks/{task_id}/cancel` (targeted `Stop` for one task only).
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn cancel_task(
        &self,
        task_id: TaskId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<CancelTaskResponse, DennettError> {
        self.request_no_body(
            Method::POST,
            &format!("/v1/tasks/{task_id}/cancel"),
            correlation_id,
        )
        .await
    }

    /// Calls `POST /v1/tasks/scheduler/tick` (available strictly under `ProfileMode::TestIsolated`).
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn scheduler_tick(
        &self,
        req: &SchedulerTickRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<SchedulerTickResponse, DennettError> {
        self.request_json(
            Method::POST,
            "/v1/tasks/scheduler/tick",
            req,
            correlation_id,
        )
        .await
    }

    /// Calls `GET /v1/inbox`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn list_inbox_cards(
        &self,
        status: Option<&str>,
        project_id: Option<ProjectId>,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ListInboxResponse, DennettError> {
        let mut params = Vec::new();
        if let Some(st) = status.map(str::trim).filter(|s| !s.is_empty()) {
            params.push(format!("status={}", encode_query_component(st)));
        }
        if let Some(pid) = project_id {
            params.push(format!("project_id={pid}"));
        }
        let path = build_query_path("/v1/inbox", &params);
        self.request_no_body(Method::GET, &path, correlation_id)
            .await
    }

    /// Calls `POST /v1/inbox/{card_id}/resolve`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn resolve_inbox_card(
        &self,
        card_id: InboxCardId,
        req: &ResolveInboxCardRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ResolveInboxCardResponse, DennettError> {
        self.request_json(
            Method::POST,
            &format!("/v1/inbox/{card_id}/resolve"),
            req,
            correlation_id,
        )
        .await
    }

    /// Calls `GET /v1/permissions/requests`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn list_permission_requests(
        &self,
        status: Option<&str>,
        session_id: Option<SessionId>,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ListPermissionRequestsResponse, DennettError> {
        let mut params = Vec::new();
        if let Some(st) = status.map(str::trim).filter(|s| !s.is_empty()) {
            params.push(format!("status={}", encode_query_component(st)));
        }
        if let Some(sid) = session_id {
            params.push(format!("session_id={sid}"));
        }
        let path = build_query_path("/v1/permissions/requests", &params);
        self.request_no_body(Method::GET, &path, correlation_id)
            .await
    }

    /// Calls `POST /v1/permissions/requests/{request_id}/resolve`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn resolve_permission_request(
        &self,
        request_id: PermissionRequestId,
        req: &ResolvePermissionRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ResolvePermissionResponse, DennettError> {
        self.request_json(
            Method::POST,
            &format!("/v1/permissions/requests/{request_id}/resolve"),
            req,
            correlation_id,
        )
        .await
    }

    /// Calls `GET /v1/grants`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn list_capability_grants(
        &self,
        project_id: Option<ProjectId>,
        include_revoked: bool,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ListCapabilityGrantsResponse, DennettError> {
        let mut params = Vec::new();
        if let Some(pid) = project_id {
            params.push(format!("project_id={pid}"));
        }
        if include_revoked {
            params.push(String::from("include_revoked=true"));
        }
        let path = build_query_path("/v1/grants", &params);
        self.request_no_body(Method::GET, &path, correlation_id)
            .await
    }

    /// Calls `POST /v1/grants/{grant_id}/revoke`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn revoke_capability_grant(
        &self,
        grant_id: CapabilityGrantId,
        req: &RevokeGrantRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<CapabilityGrantRecord, DennettError> {
        self.request_json(
            Method::POST,
            &format!("/v1/grants/{grant_id}/revoke"),
            req,
            correlation_id,
        )
        .await
    }

    /// Calls `POST /v1/agents/{recipient_agent_id}/messages` (plain-text inter-agent coordination).
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn send_agent_message(
        &self,
        recipient_agent_id: AgentId,
        req: &SendAgentMessageRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<SendAgentMessageResponse, DennettError> {
        self.request_json(
            Method::POST,
            &format!("/v1/agents/{recipient_agent_id}/messages"),
            req,
            correlation_id,
        )
        .await
    }

    /// Calls `GET /v1/agents/{recipient_agent_id}/messages`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn list_agent_messages(
        &self,
        recipient_agent_id: AgentId,
        undelivered_only: bool,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ListAgentMessagesResponse, DennettError> {
        let path = if undelivered_only {
            format!("/v1/agents/{recipient_agent_id}/messages?undelivered_only=true")
        } else {
            format!("/v1/agents/{recipient_agent_id}/messages")
        };
        self.request_no_body(Method::GET, &path, correlation_id)
            .await
    }

    /// Calls `POST /v1/effects/dispatch`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn dispatch_effect(
        &self,
        req: &DispatchEffectRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<DispatchEffectResponse, DennettError> {
        self.request_json(Method::POST, "/v1/effects/dispatch", req, correlation_id)
            .await
    }

    /// Calls `GET /v1/effects`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn list_effects(
        &self,
        task_id: Option<TaskId>,
        session_id: Option<SessionId>,
        status: Option<&str>,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ListEffectsResponse, DennettError> {
        let mut params = Vec::new();
        if let Some(tid) = task_id {
            params.push(format!("task_id={tid}"));
        }
        if let Some(sid) = session_id {
            params.push(format!("session_id={sid}"));
        }
        if let Some(st) = status.map(str::trim).filter(|s| !s.is_empty()) {
            params.push(format!("status={}", encode_query_component(st)));
        }
        let path = build_query_path("/v1/effects", &params);
        self.request_no_body(Method::GET, &path, correlation_id)
            .await
    }

    /// Calls `POST /v1/effects/{effect_id}/reconcile`.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn reconcile_effect(
        &self,
        effect_id: EffectClaimId,
        req: &ReconcileEffectRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ReconcileEffectResponse, DennettError> {
        self.request_json(
            Method::POST,
            &format!("/v1/effects/{effect_id}/reconcile"),
            req,
            correlation_id,
        )
        .await
    }

    // ------------------------------------------------------------------------
    // Phase D3: SecretBroker, Provider Connections, Capabilities, Memory Proxy
    // ------------------------------------------------------------------------

    /// Calls `GET /v1/secrets` to list write-only secret slot metadata (never plaintext values).
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn list_secrets(
        &self,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ListSecretsResponse, DennettError> {
        self.request_no_body(Method::GET, "/v1/secrets", correlation_id)
            .await
    }

    /// Calls `PUT /v1/secrets/{slot_key}` to store or rotate an encrypted secret value.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn set_secret(
        &self,
        slot_key: &str,
        req: &SetSecretRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<SecretMetadataRecord, DennettError> {
        let encoded = encode_path_segment(slot_key);
        self.request_json(
            Method::PUT,
            &format!("/v1/secrets/{encoded}"),
            req,
            correlation_id,
        )
        .await
    }

    /// Calls `DELETE /v1/secrets/{slot_key}` to remove a persisted secret from the vault.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn delete_secret(
        &self,
        slot_key: &str,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<SecretMetadataRecord, DennettError> {
        let encoded = encode_path_segment(slot_key);
        self.request_no_body(
            Method::DELETE,
            &format!("/v1/secrets/{encoded}"),
            correlation_id,
        )
        .await
    }

    /// Calls `GET /v1/runtime/connections` to list active provider connections and planned descriptors.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn list_provider_connections(
        &self,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ListProviderConnectionsResponse, DennettError> {
        self.request_no_body(Method::GET, "/v1/runtime/connections", correlation_id)
            .await
    }

    /// Calls `POST /v1/runtime/connections` to create a new runtime provider connection.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn create_provider_connection(
        &self,
        req: &UpsertProviderConnectionRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ProviderConnectionRecord, DennettError> {
        self.request_json(Method::POST, "/v1/runtime/connections", req, correlation_id)
            .await
    }

    /// Calls `PATCH /v1/runtime/connections/{connection_id}` to update a runtime provider connection.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn update_provider_connection(
        &self,
        connection_id: ProviderConnectionId,
        req: &UpsertProviderConnectionRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ProviderConnectionRecord, DennettError> {
        self.request_json(
            Method::PATCH,
            &format!("/v1/runtime/connections/{connection_id}"),
            req,
            correlation_id,
        )
        .await
    }

    /// Calls `DELETE /v1/runtime/connections/{connection_id}` to remove a runtime provider connection.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn delete_provider_connection(
        &self,
        connection_id: ProviderConnectionId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<DeleteProviderConnectionResponse, DennettError> {
        self.request_no_body(
            Method::DELETE,
            &format!("/v1/runtime/connections/{connection_id}"),
            correlation_id,
        )
        .await
    }

    /// Calls `POST /v1/runtime/connections/{connection_id}/probe` to verify reachability and discover live models.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn probe_provider_connection(
        &self,
        connection_id: ProviderConnectionId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ProbeConnectionResponse, DennettError> {
        self.request_no_body(
            Method::POST,
            &format!("/v1/runtime/connections/{connection_id}/probe"),
            correlation_id,
        )
        .await
    }

    /// Calls `POST /v1/runtime/connections/{connection_id}/activate` to promote a connection as active default.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn activate_provider_connection(
        &self,
        connection_id: ProviderConnectionId,
        req: &ActivateConnectionRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ProviderConnectionRecord, DennettError> {
        self.request_json(
            Method::POST,
            &format!("/v1/runtime/connections/{connection_id}/activate"),
            req,
            correlation_id,
        )
        .await
    }

    /// Calls `GET /v1/capabilities` with optional `project_id`, `scope_kind`, and `kind` filters.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn list_capabilities(
        &self,
        project_id: Option<ProjectId>,
        scope_kind: Option<CapabilityScopeKind>,
        kind: Option<CapabilityKind>,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ListCapabilitiesResponse, DennettError> {
        let mut params = Vec::new();
        if let Some(pid) = project_id {
            params.push(format!("project_id={pid}"));
        }
        if let Some(sk) = scope_kind {
            params.push(format!("scope_kind={}", sk.as_str()));
        }
        if let Some(k) = kind {
            params.push(format!("kind={}", k.as_str()));
        }
        let path = build_query_path("/v1/capabilities", &params);
        self.request_no_body(Method::GET, &path, correlation_id)
            .await
    }

    /// Calls `POST /v1/capabilities` to register a skill, MCP server, or plugin capability.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn create_capability(
        &self,
        req: &CreateCapabilityRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<CapabilityRecord, DennettError> {
        self.request_json(Method::POST, "/v1/capabilities", req, correlation_id)
            .await
    }

    /// Calls `POST /v1/capabilities/discover` to scan project-local and read-only personal candidates.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn discover_capabilities(
        &self,
        req: &DiscoverCapabilitiesRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<DiscoverCapabilitiesResponse, DennettError> {
        self.request_json(
            Method::POST,
            "/v1/capabilities/discover",
            req,
            correlation_id,
        )
        .await
    }

    /// Calls `PATCH /v1/capabilities/{capability_id}` to update metadata, version/spec, scope, trust, or activation.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn update_capability(
        &self,
        capability_id: CapabilityId,
        req: &UpdateCapabilityRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<CapabilityRecord, DennettError> {
        self.request_json(
            Method::PATCH,
            &format!("/v1/capabilities/{capability_id}"),
            req,
            correlation_id,
        )
        .await
    }

    /// Calls `DELETE /v1/capabilities/{capability_id}` to remove a capability from Dennett's registry.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn delete_capability(
        &self,
        capability_id: CapabilityId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<DeleteCapabilityResponse, DennettError> {
        self.request_no_body(
            Method::DELETE,
            &format!("/v1/capabilities/{capability_id}"),
            correlation_id,
        )
        .await
    }

    /// Calls `POST /v1/capabilities/{capability_id}/rollback` to restore the previous capability snapshot.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn rollback_capability(
        &self,
        capability_id: CapabilityId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<CapabilityRecord, DennettError> {
        self.request_no_body(
            Method::POST,
            &format!("/v1/capabilities/{capability_id}/rollback"),
            correlation_id,
        )
        .await
    }

    /// Calls `PUT /v1/projects/{project_id}/capabilities/{capability_id}` to set a per-project activation binding.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn set_project_capability_binding(
        &self,
        project_id: ProjectId,
        capability_id: CapabilityId,
        req: &SetProjectCapabilityBindingRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<CapabilityProjectBinding, DennettError> {
        self.request_json(
            Method::PUT,
            &format!("/v1/projects/{project_id}/capabilities/{capability_id}"),
            req,
            correlation_id,
        )
        .await
    }

    /// Calls `GET /v1/memory/overview` (V02-safe Head proxy returning `DegradedUnreachable` when `memoryd` is down).
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn get_memory_overview(
        &self,
        scope: Option<&MemoryScope>,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<MemoryOverviewResponse, DennettError> {
        let path = scoped_query_path("/v1/memory/overview", scope);
        self.request_no_body(Method::GET, &path, correlation_id)
            .await
    }

    /// Calls `POST /v1/memory/notes/{note_id}/correct` via Head's Memory Fabric proxy.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn correct_memory_note(
        &self,
        req: &CorrectNoteRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<NoteMutationResponse, DennettError> {
        self.request_json(
            Method::POST,
            &format!("/v1/memory/notes/{}/correct", req.note_id),
            req,
            correlation_id,
        )
        .await
    }

    /// Calls `GET /v1/memory/notes/{note_id}/history` via Head's Memory Fabric proxy.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn get_memory_note_history(
        &self,
        note_id: MemoryNoteId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<NoteHistoryResponse, DennettError> {
        self.request_no_body(
            Method::GET,
            &format!("/v1/memory/notes/{note_id}/history"),
            correlation_id,
        )
        .await
    }

    /// Calls `GET /v1/memory/claims` with optional `scope` and `topic_key` filters via Head.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn list_memory_claims(
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

    /// Calls `POST /v1/memory/claims` via Head's Memory Fabric proxy.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn create_memory_claim(
        &self,
        req: &CreateClaimRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<MemoryClaim, DennettError> {
        self.request_json(Method::POST, "/v1/memory/claims", req, correlation_id)
            .await
    }

    /// Calls `POST /v1/memory/claims/{claim_id}/supersede` via Head's Memory Fabric proxy.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn supersede_memory_claim(
        &self,
        req: &SupersedeClaimRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<MemoryClaim, DennettError> {
        self.request_json(
            Method::POST,
            &format!("/v1/memory/claims/{}/supersede", req.claim_id),
            req,
            correlation_id,
        )
        .await
    }

    /// Calls `GET /v1/memory/conflicts` with an optional `scope` filter via Head.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn list_memory_conflicts(
        &self,
        scope: Option<&MemoryScope>,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ListConflictsResponse, DennettError> {
        let path = scoped_query_path("/v1/memory/conflicts", scope);
        self.request_no_body(Method::GET, &path, correlation_id)
            .await
    }

    /// Calls `POST /v1/memory/conflicts/{conflict_id}/resolve` via Head.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn resolve_memory_conflict(
        &self,
        req: &ResolveConflictRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<MemoryConflict, DennettError> {
        self.request_json(
            Method::POST,
            &format!("/v1/memory/conflicts/{}/resolve", req.conflict_id),
            req,
            correlation_id,
        )
        .await
    }

    /// Calls `GET /v1/memory/documents` with an optional `scope` filter via Head.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn list_memory_documents(
        &self,
        scope: Option<&MemoryScope>,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ListDocumentsResponse, DennettError> {
        let path = scoped_query_path("/v1/memory/documents", scope);
        self.request_no_body(Method::GET, &path, correlation_id)
            .await
    }

    /// Calls `POST /v1/memory/documents` via Head.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn create_memory_document(
        &self,
        req: &CreateDocumentRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<MemoryDocument, DennettError> {
        self.request_json(Method::POST, "/v1/memory/documents", req, correlation_id)
            .await
    }

    /// Calls `GET /v1/memory/documents/{document_id}` via Head.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn get_memory_document(
        &self,
        document_id: DocumentId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<MemoryDocument, DennettError> {
        self.request_no_body(
            Method::GET,
            &format!("/v1/memory/documents/{document_id}"),
            correlation_id,
        )
        .await
    }

    /// Calls `PUT /v1/memory/documents/{document_id}` via Head.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn update_memory_document(
        &self,
        req: &UpdateDocumentRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<MemoryDocument, DennettError> {
        self.request_json(
            Method::PUT,
            &format!("/v1/memory/documents/{}", req.document_id),
            req,
            correlation_id,
        )
        .await
    }

    /// Calls `GET /v1/memory/documents/{document_id}/versions` via Head.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn get_memory_document_versions(
        &self,
        document_id: DocumentId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<GetDocumentVersionsResponse, DennettError> {
        self.request_no_body(
            Method::GET,
            &format!("/v1/memory/documents/{document_id}/versions"),
            correlation_id,
        )
        .await
    }

    /// Calls `POST /v1/memory/forget` via Head.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn forget_memory_item(
        &self,
        req: &ForgetRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ForgetResponse, DennettError> {
        self.request_json(Method::POST, "/v1/memory/forget", req, correlation_id)
            .await
    }

    /// Calls `GET /v1/memory/forget/tombstones` with an optional `scope` filter via Head.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn list_memory_tombstones(
        &self,
        scope: Option<&MemoryScope>,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ListTombstonesResponse, DennettError> {
        let path = scoped_query_path("/v1/memory/forget/tombstones", scope);
        self.request_no_body(Method::GET, &path, correlation_id)
            .await
    }

    /// Calls `POST /v1/memory/export` via Head (Contract J).
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn export_memory_package(
        &self,
        req: &ExportRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ExportResponse, DennettError> {
        self.request_json(Method::POST, "/v1/memory/export", req, correlation_id)
            .await
    }

    /// Calls `POST /v1/memory/import` via Head (Contract J).
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn import_memory_package(
        &self,
        req: &ImportRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ImportResponse, DennettError> {
        self.request_json(Method::POST, "/v1/memory/import", req, correlation_id)
            .await
    }

    /// Calls `POST /v1/memory/index/rebuild` via Head.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn trigger_memory_index_rebuild(
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

    /// Calls `GET /v1/memory/consolidation/jobs` with an optional `scope` filter via Head.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn list_memory_consolidation_jobs(
        &self,
        scope: Option<&MemoryScope>,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ListConsolidationJobsResponse, DennettError> {
        let path = scoped_query_path("/v1/memory/consolidation/jobs", scope);
        self.request_no_body(Method::GET, &path, correlation_id)
            .await
    }

    /// Calls `POST /v1/memory/consolidation/jobs` via Head.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn create_memory_consolidation_job(
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

    /// Calls `POST /v1/memory/consolidation/jobs/{job_id}/pause` via Head.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn pause_memory_consolidation_job(
        &self,
        job_id: ConsolidationJobId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ConsolidationJob, DennettError> {
        self.request_no_body(
            Method::POST,
            &format!("/v1/memory/consolidation/jobs/{job_id}/pause"),
            correlation_id,
        )
        .await
    }

    /// Calls `POST /v1/memory/consolidation/jobs/{job_id}/resume` via Head.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn resume_memory_consolidation_job(
        &self,
        job_id: ConsolidationJobId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ConsolidationJob, DennettError> {
        self.request_no_body(
            Method::POST,
            &format!("/v1/memory/consolidation/jobs/{job_id}/resume"),
            correlation_id,
        )
        .await
    }

    // ------------------------------------------------------------------------
    // Phase D3+: Live Session Runs, Durable Follow-ups, Steering & Session Skills
    // ------------------------------------------------------------------------

    /// Calls `GET /v1/sessions/{session_id}/live` to fetch the active streaming run snapshot
    /// and durable follow-up queue for a session.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn get_session_live(
        &self,
        session_id: SessionId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<LiveRunResponse, DennettError> {
        self.request_no_body(
            Method::GET,
            &format!("/v1/sessions/{session_id}/live"),
            correlation_id,
        )
        .await
    }

    /// Calls `POST /v1/sessions/{session_id}/followups` to append a durable follow-up message.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn enqueue_followup(
        &self,
        session_id: SessionId,
        request: &EnqueueFollowupRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<SessionFollowupRecord, DennettError> {
        self.request_json(
            Method::POST,
            &format!("/v1/sessions/{session_id}/followups"),
            request,
            correlation_id,
        )
        .await
    }

    /// Calls `PATCH /v1/sessions/{session_id}/followups/{followup_id}` to edit a queued or paused follow-up.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn update_followup(
        &self,
        session_id: SessionId,
        followup_id: FollowupId,
        request: &UpdateFollowupRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<SessionFollowupRecord, DennettError> {
        self.request_json(
            Method::PATCH,
            &format!("/v1/sessions/{session_id}/followups/{followup_id}"),
            request,
            correlation_id,
        )
        .await
    }

    /// Calls `DELETE /v1/sessions/{session_id}/followups/{followup_id}` to cancel and remove one follow-up.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn delete_followup(
        &self,
        session_id: SessionId,
        followup_id: FollowupId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<(), DennettError> {
        self.request_empty(
            Method::DELETE,
            &format!("/v1/sessions/{session_id}/followups/{followup_id}"),
            correlation_id,
        )
        .await
    }

    /// Calls `DELETE /v1/sessions/{session_id}/followups` to clear all pending follow-ups for a session.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn clear_followups(
        &self,
        session_id: SessionId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<(), DennettError> {
        self.request_empty(
            Method::DELETE,
            &format!("/v1/sessions/{session_id}/followups"),
            correlation_id,
        )
        .await
    }

    /// Calls `POST /v1/sessions/{session_id}/followups/resume` to resume paused follow-ups.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn resume_followups(
        &self,
        session_id: SessionId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ResumeFollowupsResponse, DennettError> {
        self.request_no_body(
            Method::POST,
            &format!("/v1/sessions/{session_id}/followups/resume"),
            correlation_id,
        )
        .await
    }

    /// Calls `POST /v1/sessions/{session_id}/steer` to interrupt any active run and send an immediate steering message.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn steer_session(
        &self,
        session_id: SessionId,
        request: &SteerSessionRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<SendMessageResponse, DennettError> {
        self.request_json(
            Method::POST,
            &format!("/v1/sessions/{session_id}/steer"),
            request,
            correlation_id,
        )
        .await
    }

    /// Calls `GET /v1/capabilities/session-skills` to list skills active for the scoped session.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn list_session_skills(
        &self,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<Vec<SessionSkillSummary>, DennettError> {
        self.request_no_body(
            Method::GET,
            "/v1/capabilities/session-skills",
            correlation_id,
        )
        .await
    }

    /// Calls `GET /v1/capabilities/session-skills/{slug}` to read a scoped session skill's `SKILL.md` body.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `slug` is empty or contains path separators/traversal,
    /// or the typed [`DennettError`] reported by Head or transport failure.
    pub async fn read_session_skill(
        &self,
        slug: &str,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<SessionSkillDetailResponse, DennettError> {
        let trimmed = slug.trim();
        if trimmed.is_empty()
            || trimmed.contains('/')
            || trimmed.contains('\\')
            || trimmed.contains("..")
            || trimmed.as_bytes().contains(&0)
        {
            return Err(DennettError::InvalidInput {
                field: String::from("slug"),
                reason: String::from(
                    "must be a non-empty single path segment without '/', '\\', '..', or NUL bytes",
                ),
            });
        }
        let encoded = encode_path_segment(trimmed);
        self.request_no_body(
            Method::GET,
            &format!("/v1/capabilities/session-skills/{encoded}"),
            correlation_id,
        )
        .await
    }

    // ------------------------------------------------------------------------
    // Phase D3+ Audit Closure: Fork, Preferences, File Preview, Controls & Index Status
    // ------------------------------------------------------------------------

    /// Calls `POST /v1/sessions/{session_id}/fork` to fork a session from an existing message.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn fork_session(
        &self,
        session_id: SessionId,
        req: &ForkSessionRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<SessionRecord, DennettError> {
        self.request_json(
            Method::POST,
            &format!("/v1/sessions/{session_id}/fork"),
            req,
            correlation_id,
        )
        .await
    }

    /// Calls `DELETE /v1/sessions/{session_id}` to delete a session record.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn delete_session(
        &self,
        session_id: SessionId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<(), DennettError> {
        self.request_empty(
            Method::DELETE,
            &format!("/v1/sessions/{session_id}"),
            correlation_id,
        )
        .await
    }

    /// Calls `GET /v1/preferences` to retrieve persisted owner locale, timezone, and composer preferences.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn get_preferences(
        &self,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<UserPreferences, DennettError> {
        self.request_no_body(Method::GET, "/v1/preferences", correlation_id)
            .await
    }

    /// Calls `PATCH /v1/preferences` to update persisted owner preferences.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn update_preferences(
        &self,
        req: &UpdateUserPreferencesRequest,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<UserPreferences, DennettError> {
        self.request_json(Method::PATCH, "/v1/preferences", req, correlation_id)
            .await
    }

    /// Calls `GET /v1/projects/{project_id}/file?path=...` to read a bounded workspace file preview.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `relative_path` is empty or contains NUL bytes,
    /// or the typed [`DennettError`] reported by Head or transport failure.
    pub async fn get_project_file_content(
        &self,
        project_id: ProjectId,
        relative_path: &str,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<ProjectFileContentResponse, DennettError> {
        let trimmed = relative_path.trim();
        if trimmed.is_empty() || trimmed.as_bytes().contains(&0) {
            return Err(DennettError::InvalidInput {
                field: String::from("relative_path"),
                reason: String::from("must be non-empty and must not contain NUL bytes"),
            });
        }
        let path = format!(
            "/v1/projects/{project_id}/file?path={}",
            encode_query_component(trimmed)
        );
        self.request_no_body(Method::GET, &path, correlation_id)
            .await
    }

    /// Calls `DELETE /v1/policies/{rule_id}` to remove an external policy rule.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn delete_policy_rule(
        &self,
        rule_id: PolicyRuleId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<(), DennettError> {
        self.request_empty(
            Method::DELETE,
            &format!("/v1/policies/{rule_id}"),
            correlation_id,
        )
        .await
    }

    /// Calls `POST /v1/runs/emergency-stop` to cancel all active runs and tasks immediately.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn emergency_stop_all(
        &self,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<EmergencyStopResponse, DennettError> {
        self.request_no_body(Method::POST, "/v1/runs/emergency-stop", correlation_id)
            .await
    }

    /// Calls `POST /v1/tasks/{task_id}/run-now` to trigger immediate execution of a task.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn run_task_now(
        &self,
        task_id: TaskId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<TaskRecord, DennettError> {
        self.request_no_body(
            Method::POST,
            &format!("/v1/tasks/{task_id}/run-now"),
            correlation_id,
        )
        .await
    }

    /// Calls `POST /v1/tasks/{task_id}/pause` to pause a scheduled or queued task.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn pause_task(
        &self,
        task_id: TaskId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<TaskRecord, DennettError> {
        self.request_no_body(
            Method::POST,
            &format!("/v1/tasks/{task_id}/pause"),
            correlation_id,
        )
        .await
    }

    /// Calls `POST /v1/tasks/{task_id}/resume` to resume a paused task.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn resume_task(
        &self,
        task_id: TaskId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<TaskRecord, DennettError> {
        self.request_no_body(
            Method::POST,
            &format!("/v1/tasks/{task_id}/resume"),
            correlation_id,
        )
        .await
    }

    /// Calls `DELETE /v1/tasks/{task_id}` to remove a task record.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn delete_task(
        &self,
        task_id: TaskId,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<(), DennettError> {
        self.request_empty(
            Method::DELETE,
            &format!("/v1/tasks/{task_id}"),
            correlation_id,
        )
        .await
    }

    /// Calls `GET /v1/memory/index/status` with an optional `scope` filter via Head.
    ///
    /// # Errors
    ///
    /// Returns the typed [`DennettError`] reported by Head or transport failure.
    pub async fn get_memory_index_status(
        &self,
        scope: Option<&MemoryScope>,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<IndexState, DennettError> {
        let path = scoped_query_path("/v1/memory/index/status", scope);
        self.request_no_body(Method::GET, &path, correlation_id)
            .await
    }

    async fn request_no_body<Resp: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<Resp, DennettError> {
        let url = format!("{}{path}", self.base_url);
        let headers = build_headers(
            correlation_id,
            self.control_token.as_deref(),
            self.owner_token.as_deref(),
        )?;
        let resp = self
            .http
            .request(method, &url)
            .headers(headers)
            .send()
            .await
            .map_err(|err| map_transport_error(&err))?;
        decode_response_with_limit(resp, MAX_RESPONSE_BODY_BYTES).await
    }

    async fn request_empty(
        &self,
        method: Method,
        path: &str,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<(), DennettError> {
        let url = format!("{}{path}", self.base_url);
        let headers = build_headers(
            correlation_id,
            self.control_token.as_deref(),
            self.owner_token.as_deref(),
        )?;
        let resp = self
            .http
            .request(method, &url)
            .headers(headers)
            .send()
            .await
            .map_err(|err| map_transport_error(&err))?;
        decode_empty_response_with_limit(resp, MAX_RESPONSE_BODY_BYTES).await
    }

    async fn request_json<Req: Serialize, Resp: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: &Req,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<Resp, DennettError> {
        let url = format!("{}{path}", self.base_url);
        let headers = build_headers(
            correlation_id,
            self.control_token.as_deref(),
            self.owner_token.as_deref(),
        )?;
        let resp = self
            .http
            .request(method, &url)
            .headers(headers)
            .json(body)
            .send()
            .await
            .map_err(|err| map_transport_error(&err))?;
        decode_response_with_limit(resp, MAX_RESPONSE_BODY_BYTES).await
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

fn encode_path_segment(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(byte));
        } else {
            use core::fmt::Write as _;
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
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

fn build_headers(
    correlation_id: Option<&CorrelationId>,
    control_token: Option<&str>,
    owner_token: Option<&str>,
) -> Result<HeaderMap, DennettError> {
    let mut headers = HeaderMap::new();
    let cid = correlation_id
        .cloned()
        .unwrap_or_else(CorrelationId::generate);
    let val = HeaderValue::from_str(cid.as_str()).map_err(|_| DennettError::InvalidInput {
        field: String::from("correlation_id"),
        reason: String::from("invalid HTTP header value"),
    })?;
    headers.insert(HEADER_CORRELATION_ID.clone(), val);
    if let Some(token) = control_token {
        let tok_val = HeaderValue::from_str(token).map_err(|_| DennettError::InvalidInput {
            field: String::from("control_token"),
            reason: String::from("invalid control token HTTP header value"),
        })?;
        headers.insert(HEADER_CONTROL_TOKEN.clone(), tok_val);
    }
    if let Some(token) = owner_token {
        let auth_val = HeaderValue::from_str(&format!("Bearer {token}")).map_err(|_| {
            DennettError::InvalidInput {
                field: String::from("owner_token"),
                reason: String::from("invalid owner token HTTP header value"),
            }
        })?;
        headers.insert(AUTHORIZATION, auth_val);
    }
    Ok(headers)
}

async fn decode_response_with_limit<Resp: DeserializeOwned>(
    mut resp: reqwest::Response,
    max_bytes: usize,
) -> Result<Resp, DennettError> {
    let status = resp.status();
    let body_bytes = read_bounded_body(&mut resp, max_bytes).await?;

    if status.is_success() {
        return serde_json::from_slice(&body_bytes).map_err(|err| DennettError::Internal {
            detail: redact_secrets(
                &format!("failed decoding JSON response from dennett-head: {err}"),
                &[],
            ),
        });
    }

    Err(decode_error_response(status, &body_bytes))
}

async fn decode_empty_response_with_limit(
    mut resp: reqwest::Response,
    max_bytes: usize,
) -> Result<(), DennettError> {
    let status = resp.status();
    let body_bytes = read_bounded_body(&mut resp, max_bytes).await?;

    if status.is_success() {
        return Ok(());
    }

    Err(decode_error_response(status, &body_bytes))
}

fn decode_error_response(status: StatusCode, body_bytes: &[u8]) -> DennettError {
    if let Ok(envelope) = serde_json::from_slice::<ApiErrorResponse>(body_bytes) {
        return envelope.error;
    }
    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(body_bytes)
        && let Some(err_val) = value.get("error")
        && let Ok(parsed_err) = serde_json::from_value::<DennettError>(err_val.clone())
    {
        return parsed_err;
    }
    if let Ok(direct_err) = serde_json::from_slice::<DennettError>(body_bytes) {
        return direct_err;
    }

    let safe_body = redact_secrets(&String::from_utf8_lossy(body_bytes), &[]);
    match status {
        StatusCode::BAD_REQUEST => DennettError::InvalidInput {
            field: String::from("request"),
            reason: safe_body,
        },
        StatusCode::UNAUTHORIZED => DennettError::Unauthorized { reason: safe_body },
        StatusCode::FORBIDDEN => DennettError::AccessDenied {
            scope: String::from("head"),
            reason: safe_body,
        },
        StatusCode::NOT_FOUND => DennettError::NotFound {
            resource: String::from("head"),
            id: String::from("unknown"),
        },
        StatusCode::CONFLICT => DennettError::Conflict {
            resource: String::from("head"),
            expected_revision: None,
            actual_revision: None,
            reason: safe_body,
        },
        StatusCode::TOO_MANY_REQUESTS => DennettError::ProviderQuotaExceeded {
            provider: String::from("head"),
            detail: safe_body,
            retry_after_secs: None,
        },
        StatusCode::SERVICE_UNAVAILABLE => DennettError::DatabaseUnavailable { detail: safe_body },
        StatusCode::GATEWAY_TIMEOUT | StatusCode::REQUEST_TIMEOUT => DennettError::Timeout {
            operation: String::from("head_client"),
        },
        _ => DennettError::Internal {
            detail: format!("unexpected HTTP {}: {safe_body}", status.as_u16()),
        },
    }
}

async fn read_bounded_body(
    resp: &mut reqwest::Response,
    max_bytes: usize,
) -> Result<Vec<u8>, DennettError> {
    if let Some(content_len) = resp.content_length()
        && content_len > max_bytes as u64
    {
        return Err(DennettError::Internal {
            detail: format!(
                "dennett-head response Content-Length ({content_len} bytes) exceeded client limit ({max_bytes} bytes)"
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
        .map_err(|err| map_transport_error(&err))?
    {
        if buf.len().saturating_add(chunk.len()) > max_bytes {
            return Err(DennettError::Internal {
                detail: format!(
                    "dennett-head response body exceeded client limit of {max_bytes} bytes"
                ),
            });
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

fn map_transport_error(err: &reqwest::Error) -> DennettError {
    if err.is_timeout() {
        return DennettError::Timeout {
            operation: String::from("head_client::http"),
        };
    }
    DennettError::DatabaseUnavailable {
        detail: redact_secrets(&format!("dennett-head request failed: {err}"), &[]),
    }
}

const _: () = {
    const fn assert_send_sync<T: Send + Sync + 'static>() {}
    assert_send_sync::<HeadClient>();
};

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    #[test]
    fn head_client_validates_base_url() {
        let ok = HeadClient::new("http://127.0.0.1:18081/").expect("valid");
        assert_eq!(ok.base_url(), "http://127.0.0.1:18081");
        assert!(HeadClient::new("ftp://127.0.0.1:18081").is_err());
        assert!(HeadClient::new("http://127.0.0.1:18081 bad").is_err());
    }

    #[tokio::test]
    async fn head_client_rejects_oversized_response_body() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test server");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            if let Ok((mut s, _)) = listener.accept().await {
                let mut buf = [0_u8; 1024];
                let _ = s.read(&mut buf).await;
                let _ = s
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 5000\r\nConnection: close\r\n\r\n{}")
                    .await;
                let _ = s.shutdown().await;
            }
        });
        let url = format!("http://{addr}");
        let client = HeadClient::new(&url).expect("client");
        let resp = client
            .http
            .get(format!("{url}/healthz"))
            .send()
            .await
            .expect("send");
        let err = decode_response_with_limit::<serde_json::Value>(resp, 64).await;
        assert!(
            matches!(err, Err(DennettError::Internal { ref detail }) if detail.contains("exceeded client limit"))
        );
    }

    #[tokio::test]
    async fn head_client_handles_204_followup_deletes_and_validates_skill_slug() {
        let client = HeadClient::new("http://127.0.0.1:1").expect("client");
        for bad_slug in ["", "   ", "foo/bar", "foo\\bar", "../escape", "a\0b"] {
            let err = client.read_session_skill(bad_slug, None).await;
            assert!(
                matches!(err, Err(DennettError::InvalidInput { ref field, .. }) if field == "slug"),
                "expected InvalidInput(slug) for {bad_slug:?}, got {err:?}"
            );
        }

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test server");
        let addr = listener.local_addr().expect("addr");
        let session_id = SessionId::new();
        let followup_id = FollowupId::new();
        let expected_path = format!("/v1/sessions/{session_id}/followups/{followup_id}");
        let expected_path_clone = expected_path.clone();

        tokio::spawn(async move {
            if let Ok((mut s, _)) = listener.accept().await {
                let mut buf = [0_u8; 1024];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                assert!(
                    req.starts_with(&format!("DELETE {expected_path_clone} HTTP/1.1")),
                    "unexpected request line: {req}"
                );
                let _ = s
                    .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    .await;
                let _ = s.shutdown().await;
            }
        });

        let live_client = HeadClient::new(format!("http://{addr}")).expect("client");
        live_client
            .delete_followup(session_id, followup_id, None)
            .await
            .expect("204 No Content should succeed");
    }

    #[tokio::test]
    async fn head_client_owner_token_redacts_debug_and_sends_bearer_on_stage0_endpoints() {
        let secret_token = "owner-secret-bearer-token-999";
        let mut client = HeadClient::new("http://127.0.0.1:1")
            .expect("client")
            .with_owner_token(secret_token);
        assert_eq!(client.owner_token(), Some(secret_token));
        let dbg = format!("{client:?}");
        assert!(
            !dbg.contains(secret_token),
            "Debug output leaked owner token: {dbg}"
        );
        assert!(dbg.contains(REDACTED_PLACEHOLDER));

        client.set_owner_token(None);
        assert!(client.owner_token().is_none());

        let err = client
            .get_project_file_content(ProjectId::new(), "   ", None)
            .await;
        assert!(matches!(
            err,
            Err(DennettError::InvalidInput { ref field, .. }) if field == "relative_path"
        ));

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test server");
        let addr = listener.local_addr().expect("addr");
        let project_id = ProjectId::new();
        let rule_id = PolicyRuleId::new();
        let task_id = TaskId::new();
        let expected_auth = format!("authorization: Bearer {secret_token}");
        let expected_file_req = format!("GET /v1/projects/{project_id}/file?path=docs/readme.md ");
        let expected_rule_req = format!("DELETE /v1/policies/{rule_id} ");
        let expected_task_del = format!("DELETE /v1/tasks/{task_id} ");
        let expected_stop = String::from("POST /v1/runs/emergency-stop ");

        let server = tokio::spawn(async move {
            let responses = [
                (
                    expected_file_req,
                    format!(
                        r#"{{"relative_path":"docs/readme.md","mime_type":"text/markdown","byte_size":4,"sha256":"{}","content":"test","truncated":false,"is_binary":false}}"#,
                        "a".repeat(64)
                    ),
                ),
                (expected_rule_req, String::new()),
                (expected_task_del, String::new()),
                (
                    expected_stop,
                    String::from(r#"{"cancelled_session_ids":[],"cancelled_task_ids":[]}"#),
                ),
            ];
            for (expected_prefix, body) in responses {
                let (mut s, _) = listener.accept().await.expect("accept");
                let mut buf = [0_u8; 2048];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                assert!(
                    req.starts_with(&expected_prefix),
                    "expected `{expected_prefix}`, got `{req}`"
                );
                assert!(
                    req.to_ascii_lowercase()
                        .contains(&expected_auth.to_ascii_lowercase()),
                    "missing Authorization Bearer header in request: {req}"
                );
                let raw_resp = if body.is_empty() {
                    String::from(
                        "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                } else {
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                };
                let _ = s.write_all(raw_resp.as_bytes()).await;
                let _ = s.shutdown().await;
            }
        });

        let authed = HeadClient::new(format!("http://{addr}"))
            .expect("client")
            .with_owner_token(secret_token);
        let preview = authed
            .get_project_file_content(project_id, "docs/readme.md", None)
            .await
            .expect("file preview");
        assert_eq!(preview.relative_path, "docs/readme.md");
        authed
            .delete_policy_rule(rule_id, None)
            .await
            .expect("delete policy rule");
        authed
            .delete_task(task_id, None)
            .await
            .expect("delete task");
        let stopped = authed
            .emergency_stop_all(None)
            .await
            .expect("emergency stop");
        assert_eq!(stopped.cancelled_session_ids, Vec::new());
        assert_eq!(stopped.cancelled_task_ids, Vec::new());
        server.await.expect("server task");
    }
}
