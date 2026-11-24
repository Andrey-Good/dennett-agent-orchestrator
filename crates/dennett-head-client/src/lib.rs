//! Typed async HTTP client for `dennett-head` (`/v1/*` and `/healthz`).
//!
//! Architectural boundary (Architecture 80 §6): depends ONLY on `dennett-contracts` and
//! HTTP/JSON transport crates — NEVER on `dennett-head`, `dennett-memory-core`, or PostgreSQL
//! drivers, so `dennettctl` and future `apps/desktop` UI reuse one canonical typed client.

use core::fmt;
use std::time::Duration;

use dennett_contracts::redact::{REDACTED_PLACEHOLDER, redact_secrets};
use dennett_contracts::{
    AgentConfigHistoryResponse, AgentId, AgentRecord, ApiErrorResponse, ArtifactId,
    ArtifactLifecycleRequest, ArtifactLifecycleResponse, ArtifactRecord, CancelRunResponse,
    CancelTaskResponse, CapabilityGrantId, CapabilityGrantRecord, CommitMemoryNoteRequest,
    CommitMemoryNoteResponse, CorrelationId, CreateAgentRequest, CreateArtifactRequest,
    CreatePolicyRuleRequest, CreateProjectRequest, CreateSessionRequest, CreateTaskRequest,
    DennettError, DiagnosticsResponse, DispatchEffectRequest, DispatchEffectResponse,
    EffectClaimId, EventStreamResponse, FlushOutboxResponse, HeadHealth, HeadMemorySearchRequest,
    HeadMemorySearchResponse, InboxCardId, ListAgentMessagesResponse, ListAgentsResponse,
    ListArtifactsResponse, ListCapabilityGrantsResponse, ListEffectsResponse, ListInboxResponse,
    ListOutboxResponse, ListPermissionRequestsResponse, ListPolicyRulesResponse,
    ListProjectFilesResponse, ListProjectsResponse, ListSessionsResponse, ListTasksResponse,
    PermissionRequestId, PolicyRule, ProjectId, ProjectLifecycleResponse, ProjectRecord,
    RebindProjectRequest, ReconcileEffectRequest, ReconcileEffectResponse, ReplyTaskRequest,
    ResolveInboxCardRequest, ResolveInboxCardResponse, ResolvePermissionRequest,
    ResolvePermissionResponse, RevokeGrantRequest, RuntimeCatalogResponse, SchedulerTickRequest,
    SchedulerTickResponse, SendAgentMessageRequest, SendAgentMessageResponse, SendMessageRequest,
    SendMessageResponse, SessionDetailResponse, SessionId, SessionRecord, StateSnapshotResponse,
    TaskDetailResponse, TaskId, TaskRecord, TaskSignalRequest, TaskSignalResponse,
    UpdateAgentConfigRequest, UpdateProjectRequest, UpdateSessionRequest, UpdateTaskRequest,
};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Method, StatusCode};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Default HTTP request timeout for [`HeadClient`] calls (covers live runtime turns).
pub const DEFAULT_HEAD_CLIENT_TIMEOUT: Duration = Duration::from_secs(180);
/// Default TCP connect timeout for [`HeadClient`].
pub const DEFAULT_HEAD_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Maximum HTTP response body size accepted from `dennett-head` (32 MiB).
pub const MAX_RESPONSE_BODY_BYTES: usize = 32 * 1024 * 1024;

static HEADER_CORRELATION_ID: HeaderName = HeaderName::from_static("x-correlation-id");
static HEADER_CONTROL_TOKEN: HeaderName = HeaderName::from_static("x-dennett-control-token");

/// Typed asynchronous HTTP client communicating with a `dennett-head` daemon.
#[derive(Clone)]
pub struct HeadClient {
    base_url: String,
    http: Client,
    control_token: Option<String>,
}

impl fmt::Debug for HeadClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HeadClient")
            .field("base_url", &self.base_url)
            .field(
                "control_token",
                &self.control_token.as_ref().map(|_| REDACTED_PLACEHOLDER),
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

    async fn request_no_body<Resp: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<Resp, DennettError> {
        let url = format!("{}{path}", self.base_url);
        let headers = build_headers(correlation_id, self.control_token.as_deref())?;
        let resp = self
            .http
            .request(method, &url)
            .headers(headers)
            .send()
            .await
            .map_err(|err| map_transport_error(&err))?;
        decode_response_with_limit(resp, MAX_RESPONSE_BODY_BYTES).await
    }

    async fn request_json<Req: Serialize, Resp: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: &Req,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<Resp, DennettError> {
        let url = format!("{}{path}", self.base_url);
        let headers = build_headers(correlation_id, self.control_token.as_deref())?;
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

    let safe_body = redact_secrets(&String::from_utf8_lossy(&body_bytes), &[]);
    Err(match status {
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
        StatusCode::SERVICE_UNAVAILABLE => DennettError::DatabaseUnavailable { detail: safe_body },
        StatusCode::GATEWAY_TIMEOUT | StatusCode::REQUEST_TIMEOUT => DennettError::Timeout {
            operation: String::from("head_client"),
        },
        _ => DennettError::Internal {
            detail: format!("unexpected HTTP {}: {safe_body}", status.as_u16()),
        },
    })
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
}
