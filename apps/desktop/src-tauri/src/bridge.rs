//! Thin, secret-safe bridge between the Tauri 2 desktop UI and an already-running
//! `dennett-head` HTTP daemon.
//!
//! Architectural invariants (Architecture 83 §1, V03):
//! - Never owns, spawns, or shuts down `dennett-head` or `dennett-memoryd`.
//! - Never calls `POST /shutdown` on any daemon; dropping [`HeadBridge`] or closing the
//!   desktop window leaves both server processes untouched.
//! - Enforces loopback-only Head URLs by default unless explicitly opted in.
//! - Redacts potential secret patterns before returning errors across the IPC boundary.

use std::env;
use std::fs;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use dennett_contracts::redact::redact_secrets;
use dennett_contracts::{
    ActivateConnectionRequest, AgentConfigHistoryResponse, AgentId, AgentRecord, ArtifactId,
    ArtifactLifecycleRequest, ArtifactLifecycleResponse, ArtifactRecord, CancelRunResponse,
    CancelTaskResponse, CapabilityGrantId, CapabilityGrantRecord, CapabilityId, CapabilityKind,
    CapabilityProjectBinding, CapabilityRecord, CapabilityScopeKind, CommitMemoryNoteRequest,
    CommitMemoryNoteResponse, ConsolidationJob, ConsolidationJobId, CorrectNoteRequest,
    CorrelationId, CreateAgentRequest, CreateArtifactRequest, CreateCapabilityRequest,
    CreateClaimRequest, CreateConsolidationJobRequest, CreateDocumentRequest,
    CreatePolicyRuleRequest, CreateProjectRequest, CreateSessionRequest, CreateTaskRequest,
    DeleteCapabilityResponse, DeleteProviderConnectionResponse, DennettError, DiagnosticsResponse,
    DiscoverCapabilitiesRequest, DiscoverCapabilitiesResponse, DispatchEffectRequest,
    DispatchEffectResponse, DocumentId, EffectClaimId, EmergencyStopResponse,
    EnqueueFollowupRequest, EventStreamResponse, ExportRequest, ExportResponse,
    FlushOutboxResponse, FollowupId, ForgetRequest, ForgetResponse, ForkSessionRequest,
    GetDocumentVersionsResponse, HeadHealth, HeadMemorySearchRequest, HeadMemorySearchResponse,
    ImportRequest, ImportResponse, InboxCardId, IndexState, ListAgentMessagesResponse,
    ListAgentsResponse, ListArtifactsResponse, ListCapabilitiesResponse,
    ListCapabilityGrantsResponse, ListClaimsResponse, ListConflictsResponse,
    ListConsolidationJobsResponse, ListDocumentsResponse, ListEffectsResponse, ListInboxResponse,
    ListPermissionRequestsResponse, ListPolicyRulesResponse, ListProjectFilesResponse,
    ListProjectsResponse, ListProviderConnectionsResponse, ListSecretsResponse,
    ListSessionsResponse, ListTasksResponse, ListTombstonesResponse, LiveRunResponse, MemoryClaim,
    MemoryConflict, MemoryDocument, MemoryNoteId, MemoryOverviewResponse, MemoryScope,
    NoteHistoryResponse, NoteMutationResponse, PermissionRequestId, PolicyRule, PolicyRuleId,
    ProbeConnectionResponse, ProjectFileContentResponse, ProjectId, ProjectLifecycleResponse,
    ProjectRecord, ProviderConnectionId, ProviderConnectionRecord, RebindProjectRequest,
    ReconcileEffectRequest, ReconcileEffectResponse, ReplyTaskRequest, ResolveConflictRequest,
    ResolveInboxCardRequest, ResolveInboxCardResponse, ResolvePermissionRequest,
    ResolvePermissionResponse, ResumeFollowupsResponse, RevokeGrantRequest, RuntimeCatalogResponse,
    SchedulerTickRequest, SchedulerTickResponse, SecretMetadataRecord, SendAgentMessageRequest,
    SendAgentMessageResponse, SendMessageRequest, SendMessageResponse, SessionDetailResponse,
    SessionFollowupRecord, SessionId, SessionRecord, SetProjectCapabilityBindingRequest,
    SetSecretRequest, StateSnapshotResponse, SteerSessionRequest, SupersedeClaimRequest,
    TaskDetailResponse, TaskId, TaskRecord, TaskSignalRequest, TaskSignalResponse,
    TriggerIndexRebuildRequest, TriggerIndexRebuildResponse, UpdateAgentConfigRequest,
    UpdateCapabilityRequest, UpdateDocumentRequest, UpdateFollowupRequest, UpdateProjectRequest,
    UpdateSessionRequest, UpdateTaskRequest, UpdateUserPreferencesRequest,
    UpsertProviderConnectionRequest, UserPreferences, validate_secret_slot_key,
};
use dennett_head_client::{HEAD_TOKEN_ENV as CLIENT_HEAD_TOKEN_ENV, HeadClient};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::{Host, Url};

/// Default loopback endpoint used when `DENNETT_HEAD_URL` is unset.
pub const DEFAULT_HEAD_URL: &str = "http://127.0.0.1:41771";
/// Environment variable overriding the `dennett-head` base URL for the desktop shell.
pub const HEAD_URL_ENV: &str = "DENNETT_HEAD_URL";
/// Environment variable carrying the optional owner bearer token for `dennett-head`.
pub const HEAD_TOKEN_ENV: &str = CLIENT_HEAD_TOKEN_ENV;
/// Environment variable pointing to Dennett's data directory containing `head.token`.
pub const DATA_DIR_ENV: &str = "DENNETT_DATA_DIR";
/// Opt-in environment flag (`"1"`) allowing a non-loopback `dennett-head` URL.
pub const ALLOW_REMOTE_HEAD_ENV: &str = "DENNETT_DESKTOP_ALLOW_REMOTE_HEAD";
/// Fast probe timeout used for [`HeadBridge::get_head_health`] so offline detection stays crisp.
pub const HEALTH_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Network target policy enforced when validating a configured `dennett-head` URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadEndpointPolicy {
    /// Permit only loopback endpoints (`127.0.0.1`, `::1`, `localhost`).
    LoopbackOnly,
    /// Permit explicitly configured remote `http`/`https` endpoints (still forbidding embedded credentials).
    AllowConfiguredRemote,
}

impl HeadEndpointPolicy {
    /// Reads the endpoint policy from [`ALLOW_REMOTE_HEAD_ENV`].
    #[must_use]
    pub fn from_env() -> Self {
        match env::var(ALLOW_REMOTE_HEAD_ENV).ok().as_deref() {
            Some("1") => Self::AllowConfiguredRemote,
            _ => Self::LoopbackOnly,
        }
    }
}

/// Secret-redacted IPC error returned by [`HeadBridge`] and Tauri commands.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BridgeError {
    /// Invalid configuration or URL rejected before issuing any network call.
    #[error("{message}")]
    InvalidConfig {
        /// Safe validation reason.
        message: String,
    },
    /// Target `dennett-head` daemon could not be reached or timed out.
    #[error("{message}")]
    HeadUnreachable {
        /// Safe connectivity detail.
        message: String,
    },
    /// `dennett-head` returned a structured domain or HTTP error.
    #[error("{message}")]
    HeadApi {
        /// Machine-readable error code matching [`DennettError::code`].
        code: String,
        /// Redacted human-readable diagnostic summary.
        message: String,
    },
    /// Requested resource was not found.
    #[error("{resource} `{id}` not found")]
    NotFound {
        /// Logical resource kind.
        resource: String,
        /// Missing identifier.
        id: String,
    },
}

impl BridgeError {
    /// Returns a stable machine-readable error code for the frontend.
    #[must_use]
    pub fn code(&self) -> &str {
        match self {
            Self::InvalidConfig { .. } => "INVALID_CONFIG",
            Self::HeadUnreachable { .. } => "HEAD_UNREACHABLE",
            Self::HeadApi { code, .. } => code.as_str(),
            Self::NotFound { .. } => "NOT_FOUND",
        }
    }
}

#[derive(Serialize)]
struct SerializedBridgeError<'a> {
    code: &'a str,
    message: String,
}

impl Serialize for BridgeError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let payload = SerializedBridgeError {
            code: self.code(),
            message: redact_secrets(&self.to_string(), &[]),
        };
        payload.serialize(serializer)
    }
}

impl From<DennettError> for BridgeError {
    fn from(err: DennettError) -> Self {
        let code = err.code().to_owned();
        let safe_message = redact_secrets(&err.to_string(), &[]);
        match err {
            DennettError::DatabaseUnavailable { .. } | DennettError::Timeout { .. } => {
                Self::HeadUnreachable {
                    message: safe_message,
                }
            }
            DennettError::NotFound { resource, id } => Self::NotFound { resource, id },
            _ => Self::HeadApi {
                code,
                message: safe_message,
            },
        }
    }
}

/// Connection and health snapshot returned to the desktop UI by `get_head_health`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopHealthSnapshot {
    /// Normalized `dennett-head` endpoint URL currently targeted by the desktop bridge.
    pub head_url: String,
    /// `true` when `dennett-head` answered `/healthz` with `200 OK`.
    pub connected: bool,
    /// Canonical [`HeadHealth`] payload when connected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<HeadHealth>,
    /// Safe machine-readable error code when disconnected or degraded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    /// Redacted human-readable diagnostic summary when disconnected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// JSON-serializable immutable artifact snapshot returned to the desktop UI by `get_artifact_content`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopArtifactContentResponse {
    /// Reported MIME media type (`Content-Type`).
    pub mime_type: String,
    /// Lowercase SHA-256 hex digest (`X-Dennett-Artifact-Sha256`).
    pub sha256: String,
    /// Raw byte length of the immutable snapshot.
    pub byte_size: usize,
    /// Decoded UTF-8 text when the payload is non-binary UTF-8.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Standard RFC 4648 base64 encoding of the exact raw bytes for faithful download.
    pub base64: String,
    /// `true` when bytes contain NUL or invalid UTF-8 and cannot be previewed as plain text.
    pub is_binary: bool,
}

fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut idx = 0;
    while idx + 3 <= bytes.len() {
        let n = (u32::from(bytes[idx]) << 16)
            | (u32::from(bytes[idx + 1]) << 8)
            | u32::from(bytes[idx + 2]);
        out.push(char::from(TABLE[((n >> 18) & 0x3F) as usize]));
        out.push(char::from(TABLE[((n >> 12) & 0x3F) as usize]));
        out.push(char::from(TABLE[((n >> 6) & 0x3F) as usize]));
        out.push(char::from(TABLE[(n & 0x3F) as usize]));
        idx += 3;
    }
    match bytes.len() - idx {
        1 => {
            let n = u32::from(bytes[idx]) << 16;
            out.push(char::from(TABLE[((n >> 18) & 0x3F) as usize]));
            out.push(char::from(TABLE[((n >> 12) & 0x3F) as usize]));
            out.push('=');
            out.push('=');
        }
        2 => {
            let n = (u32::from(bytes[idx]) << 16) | (u32::from(bytes[idx + 1]) << 8);
            out.push(char::from(TABLE[((n >> 18) & 0x3F) as usize]));
            out.push(char::from(TABLE[((n >> 12) & 0x3F) as usize]));
            out.push(char::from(TABLE[((n >> 6) & 0x3F) as usize]));
            out.push('=');
        }
        _ => {}
    }
    out
}

/// Reads a trimmed non-empty owner bearer token from `<dir>/head.token`, if present.
#[must_use]
pub fn read_owner_token_from_dir(dir: &Path) -> Option<String> {
    let raw = fs::read_to_string(dir.join("head.token")).ok()?;
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

/// Resolves the optional `dennett-head` owner bearer token from `DENNETT_HEAD_TOKEN`
/// or, when unset/empty, from `<data_dir>/head.token`.
#[must_use]
pub fn resolve_owner_token() -> Option<String> {
    if let Ok(raw) = env::var(HEAD_TOKEN_ENV) {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_owned());
        }
    }

    let mut candidate_dirs: Vec<PathBuf> = Vec::new();
    if let Ok(explicit) = env::var(DATA_DIR_ENV) {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            candidate_dirs.push(PathBuf::from(trimmed));
        }
    }
    if cfg!(windows) {
        if let Ok(local) = env::var("LOCALAPPDATA")
            && !local.trim().is_empty()
        {
            candidate_dirs.push(PathBuf::from(local).join("Dennett"));
        }
        if let Ok(roaming) = env::var("APPDATA")
            && !roaming.trim().is_empty()
        {
            candidate_dirs.push(PathBuf::from(roaming).join("Dennett"));
        }
    } else {
        if let Ok(xdg) = env::var("XDG_DATA_HOME")
            && !xdg.trim().is_empty()
        {
            candidate_dirs.push(PathBuf::from(xdg).join("dennett"));
        }
        if let Ok(home) = env::var("HOME")
            && !home.trim().is_empty()
        {
            candidate_dirs.push(
                PathBuf::from(home)
                    .join(".local")
                    .join("share")
                    .join("dennett"),
            );
        }
    }

    candidate_dirs
        .into_iter()
        .find_map(|dir| read_owner_token_from_dir(&dir))
}

/// Validates and normalizes a `dennett-head` HTTP base URL under `policy`.
///
/// Rejects non-HTTP(S) schemes, embedded credentials (`user:pass@`), query strings,
/// fragments, non-root paths, and non-loopback hosts when `policy` is
/// [`HeadEndpointPolicy::LoopbackOnly`].
///
/// # Errors
///
/// Returns [`BridgeError::InvalidConfig`] if `raw` violates any URL or loopback rule.
pub fn validate_head_url(raw: &str, policy: HeadEndpointPolicy) -> Result<String, BridgeError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(BridgeError::InvalidConfig {
            message: String::from("Head URL must not be empty"),
        });
    }

    let parsed = Url::parse(trimmed).map_err(|err| BridgeError::InvalidConfig {
        message: redact_secrets(&format!("invalid Head URL: {err}"), &[]),
    })?;

    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(BridgeError::InvalidConfig {
            message: format!(
                "unsupported Head URL scheme `{}` (expected http or https)",
                parsed.scheme()
            ),
        });
    }

    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(BridgeError::InvalidConfig {
            message: String::from("Head URL must not embed credentials"),
        });
    }

    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(BridgeError::InvalidConfig {
            message: String::from("Head URL must not contain query parameters or fragments"),
        });
    }

    if !matches!(parsed.path(), "" | "/") {
        return Err(BridgeError::InvalidConfig {
            message: String::from("Head URL must target the root endpoint without a subpath"),
        });
    }

    let Some(host) = parsed.host() else {
        return Err(BridgeError::InvalidConfig {
            message: String::from("Head URL is missing a host"),
        });
    };

    if policy == HeadEndpointPolicy::LoopbackOnly && !is_loopback_host(&host) {
        return Err(BridgeError::InvalidConfig {
            message: format!(
                "refusing non-loopback Head URL host `{host}` without explicit {ALLOW_REMOTE_HEAD_ENV}=1 opt-in"
            ),
        });
    }

    let normalized = match parsed.port() {
        Some(port) => format!("{}://{host}:{port}", parsed.scheme()),
        None => format!("{}://{host}", parsed.scheme()),
    };
    Ok(normalized)
}

fn is_loopback_host(host: &Host<&str>) -> bool {
    match host {
        Host::Domain(domain) => domain.eq_ignore_ascii_case("localhost"),
        Host::Ipv4(addr) => IpAddr::V4(*addr).is_loopback(),
        Host::Ipv6(addr) => IpAddr::V6(*addr).is_loopback(),
    }
}

/// Thin async bridge forwarding desktop UI requests to an independent `dennett-head` process.
///
/// Cheap to clone (`reqwest::Client` uses internal `Arc` pooling). Dropping [`HeadBridge`]
/// only closes local HTTP client handles and NEVER signals `dennett-head` or `dennett-memoryd`
/// to terminate (V03).
#[derive(Debug, Clone)]
pub struct HeadBridge {
    client: HeadClient,
    health_client: HeadClient,
    policy: HeadEndpointPolicy,
}

impl HeadBridge {
    /// Constructs a [`HeadBridge`] after validating `base_url` against `policy` and loading any
    /// owner bearer token from `DENNETT_HEAD_TOKEN`.
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError::InvalidConfig`] if `base_url` fails validation.
    pub fn new(base_url: &str, policy: HeadEndpointPolicy) -> Result<Self, BridgeError> {
        let normalized = validate_head_url(base_url, policy)?;
        let client = HeadClient::new(&normalized)?.with_owner_token_from_env();
        let health_client = HeadClient::with_timeout(&normalized, HEALTH_PROBE_TIMEOUT)?
            .with_owner_token_from_env();
        Ok(Self {
            client,
            health_client,
            policy,
        })
    }

    /// Constructs a [`HeadBridge`] with an explicit optional owner bearer token.
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError::InvalidConfig`] if `base_url` fails validation.
    pub fn with_owner_token(
        base_url: &str,
        policy: HeadEndpointPolicy,
        owner_token: Option<String>,
    ) -> Result<Self, BridgeError> {
        let normalized = validate_head_url(base_url, policy)?;
        let client = HeadClient::new(&normalized)?.with_optional_owner_token(owner_token.clone());
        let health_client = HeadClient::with_timeout(&normalized, HEALTH_PROBE_TIMEOUT)?
            .with_optional_owner_token(owner_token);
        Ok(Self {
            client,
            health_client,
            policy,
        })
    }

    /// Constructs a [`HeadBridge`] from `DENNETT_HEAD_URL` (default [`DEFAULT_HEAD_URL`]),
    /// [`ALLOW_REMOTE_HEAD_ENV`], and [`resolve_owner_token`] (`DENNETT_HEAD_TOKEN` or
    /// `<data_dir>/head.token`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError::InvalidConfig`] if the configured URL is invalid or violates policy.
    pub fn from_env() -> Result<Self, BridgeError> {
        let policy = HeadEndpointPolicy::from_env();
        let raw_url = env::var(HEAD_URL_ENV)
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| String::from(DEFAULT_HEAD_URL));
        Self::with_owner_token(&raw_url, policy, resolve_owner_token())
    }

    /// Returns the normalized `dennett-head` base URL.
    #[must_use]
    pub fn head_url(&self) -> &str {
        self.client.base_url()
    }

    /// Returns the configured owner bearer token, when present.
    #[must_use]
    pub fn owner_token(&self) -> Option<&str> {
        self.client.owner_token()
    }

    /// Returns the active [`HeadEndpointPolicy`].
    #[must_use]
    pub const fn policy(&self) -> HeadEndpointPolicy {
        self.policy
    }

    /// Probes `GET /healthz` on `dennett-head`, returning an honest connection snapshot even when offline.
    pub async fn get_head_health(&self) -> DesktopHealthSnapshot {
        let cid = CorrelationId::generate();
        match self.health_client.healthz(Some(&cid)).await {
            Ok(health) => DesktopHealthSnapshot {
                head_url: self.head_url().to_owned(),
                connected: true,
                health: Some(health),
                error_code: None,
                error: None,
            },
            Err(err) => {
                let bridge_err = BridgeError::from(err);
                DesktopHealthSnapshot {
                    head_url: self.head_url().to_owned(),
                    connected: false,
                    health: None,
                    error_code: Some(bridge_err.code().to_owned()),
                    error: Some(redact_secrets(&bridge_err.to_string(), &[])),
                }
            }
        }
    }

    /// Fetches the dynamic runtime model, thinking-level, account, and quota catalog (`GET /v1/runtime/catalog`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if `dennett-head` is unreachable or runtime probing fails.
    pub async fn get_runtime_catalog(
        &self,
        refresh: bool,
    ) -> Result<RuntimeCatalogResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .get_runtime_catalog(refresh, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Lists registered workspace projects (`GET /v1/projects`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if `dennett-head` is unreachable or rejects the request.
    pub async fn list_projects(&self) -> Result<ListProjectsResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .list_projects(Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Creates a new project workspace (`POST /v1/projects`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure, conflict, or unreachable Head.
    pub async fn create_project(
        &self,
        req: &CreateProjectRequest,
    ) -> Result<ProjectRecord, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .create_project(req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Renames a project, attaches/rebinds its workspace folder, and/or toggles pin state (`PATCH /v1/projects/{id}`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation/boundary failure or unreachable Head.
    pub async fn update_project(
        &self,
        project_id: ProjectId,
        req: &UpdateProjectRequest,
    ) -> Result<ProjectRecord, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .update_project(project_id, req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Lists real workspace files for a project (`GET /v1/projects/{id}/files`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on access denial, missing project, or unreachable Head.
    pub async fn list_project_files(
        &self,
        project_id: ProjectId,
        subpath: Option<&str>,
    ) -> Result<ListProjectFilesResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .list_project_files(project_id, subpath, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Lists registered Dennett agents (`GET /v1/agents`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if `dennett-head` is unreachable or rejects the request.
    pub async fn list_agents(&self) -> Result<ListAgentsResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .list_agents(Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Creates a new stable Dennett agent (`POST /v1/agents`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure or unreachable Head.
    pub async fn create_agent(&self, req: &CreateAgentRequest) -> Result<AgentRecord, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .create_agent(req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Appends a new immutable `AgentConfigVersion` without changing `agent_id`.
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure, missing agent, or unreachable Head.
    pub async fn update_agent_config(
        &self,
        agent_id: AgentId,
        req: &UpdateAgentConfigRequest,
    ) -> Result<AgentRecord, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .update_agent_config(agent_id, req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Lists persisted conversation sessions (`GET /v1/sessions`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if `dennett-head` is unreachable or rejects the request.
    pub async fn list_sessions(&self) -> Result<ListSessionsResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .list_sessions(Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Creates a new conversation session (`POST /v1/sessions`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure or unreachable Head.
    pub async fn create_session(
        &self,
        req: &CreateSessionRequest,
    ) -> Result<SessionRecord, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .create_session(req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Renames a conversation session inline and/or toggles pin state (`PATCH /v1/sessions/{id}`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure, missing session, or unreachable Head.
    pub async fn update_session(
        &self,
        session_id: SessionId,
        req: &UpdateSessionRequest,
    ) -> Result<SessionRecord, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .update_session(session_id, req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Loads full canonical session state, messages, artifacts, runs, and permission audit log.
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if the session does not exist or Head is unreachable.
    pub async fn get_session_detail(
        &self,
        session_id: SessionId,
    ) -> Result<SessionDetailResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .get_session_detail(session_id, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Sends a user message to `dennett-head` for execution through the session's agent runtime.
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure, runtime error, or unreachable Head.
    pub async fn send_message(
        &self,
        session_id: SessionId,
        req: &SendMessageRequest,
    ) -> Result<SendMessageResponse, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .send_message(session_id, req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Requests cancellation of the active run for `session_id` (`POST /v1/sessions/{id}/cancel`).
    ///
    /// Never blocks on `memoryd` availability (V02).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if `dennett-head` is unreachable or session does not exist.
    pub async fn cancel_run(
        &self,
        session_id: SessionId,
    ) -> Result<CancelRunResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .cancel_run(session_id, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Lists artifacts attached to `session_id` (`GET /v1/sessions/{id}/artifacts`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if `dennett-head` is unreachable or session does not exist.
    pub async fn list_artifacts(
        &self,
        session_id: SessionId,
    ) -> Result<ListArtifactsResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .list_artifacts(session_id, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Creates an ordinary artifact attached to `session_id` (`POST /v1/sessions/{id}/artifacts`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure or unreachable Head.
    pub async fn create_artifact(
        &self,
        session_id: SessionId,
        req: &CreateArtifactRequest,
    ) -> Result<ArtifactRecord, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .create_artifact(session_id, req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Retrieves a specific artifact by `(session_id, artifact_id)` from `dennett-head`.
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError::NotFound`] if the artifact does not exist in `session_id`,
    /// or [`BridgeError::HeadUnreachable`] / [`BridgeError::HeadApi`] on transport/API failures.
    pub async fn get_artifact(
        &self,
        session_id: SessionId,
        artifact_id: ArtifactId,
    ) -> Result<ArtifactRecord, BridgeError> {
        let list = self.list_artifacts(session_id).await?;
        list.artifacts
            .into_iter()
            .find(|art| art.artifact_id == artifact_id)
            .ok_or_else(|| BridgeError::NotFound {
                resource: String::from("artifact"),
                id: artifact_id.to_string(),
            })
    }

    /// Fetches recent secret-free diagnostic events, policy decisions, and outbox rows.
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if `dennett-head` is unreachable or rejects the request.
    pub async fn list_diagnostics(
        &self,
        limit: Option<i64>,
    ) -> Result<DiagnosticsResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .get_diagnostics(limit, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Triggers an idempotent flush of pending `memory_commit_outbox` items to `memoryd`.
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if `dennett-head` is unreachable or rejects the request.
    pub async fn flush_memory_outbox(&self) -> Result<FlushOutboxResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .flush_memory_outbox(Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Queries scoped notes from standalone `dennett-memoryd` through `dennett-head` (`POST /v1/memory/search`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if `dennett-head` is unreachable or rejects the request.
    pub async fn search_memory(
        &self,
        req: &HeadMemorySearchRequest,
    ) -> Result<HeadMemorySearchResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .search_memory(req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Lists registered workspace projects with optional archived inclusion (`GET /v1/projects?include_archived=...`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if `dennett-head` is unreachable or rejects the request.
    pub async fn list_projects_with_options(
        &self,
        include_archived: bool,
    ) -> Result<ListProjectsResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .list_projects_with_options(include_archived, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Rebinds a project's canonical workspace folder (`POST /v1/projects/{id}/rebind`, Contract C).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation/boundary failure or unreachable Head.
    pub async fn rebind_project(
        &self,
        project_id: ProjectId,
        req: &RebindProjectRequest,
    ) -> Result<ProjectLifecycleResponse, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .rebind_project(project_id, req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Detaches a project's workspace folder without deleting user files (`POST /v1/projects/{id}/detach-workspace`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if the project does not exist or Head is unreachable.
    pub async fn detach_project_workspace(
        &self,
        project_id: ProjectId,
    ) -> Result<ProjectLifecycleResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .detach_project_workspace(project_id, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Reversibly archives a project and pauses its pending timers (`POST /v1/projects/{id}/archive`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if the project does not exist or Head is unreachable.
    pub async fn archive_project(
        &self,
        project_id: ProjectId,
    ) -> Result<ProjectLifecycleResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .archive_project(project_id, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Restores an archived project to active state (`POST /v1/projects/{id}/unarchive`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if the project does not exist or Head is unreachable.
    pub async fn unarchive_project(
        &self,
        project_id: ProjectId,
    ) -> Result<ProjectLifecycleResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .unarchive_project(project_id, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Removes a project entry from Dennett active lists without deleting user files (`POST /v1/projects/{id}/remove`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if the project does not exist or Head is unreachable.
    pub async fn remove_project(
        &self,
        project_id: ProjectId,
    ) -> Result<ProjectLifecycleResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .remove_project(project_id, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Deletes Dennett metadata for a project while leaving external user folders untouched (`DELETE /v1/projects/{id}?confirm=...`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if confirmation is missing, project is missing, or Head is unreachable.
    pub async fn delete_project(
        &self,
        project_id: ProjectId,
        confirm: bool,
    ) -> Result<ProjectLifecycleResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .delete_project(project_id, confirm, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Lists all revisions (`v1`, `v2`, …) of an artifact in `session_id` (`GET /v1/sessions/{id}/artifacts/{key}/revisions`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if the session/artifact is missing or Head is unreachable.
    pub async fn list_artifact_revisions(
        &self,
        session_id: SessionId,
        artifact_id_or_name: &str,
    ) -> Result<ListArtifactsResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .list_artifact_revisions(session_id, artifact_id_or_name, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Applies an explicit Contract D lifecycle transition to one artifact revision.
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on conflict, missing artifact, or unreachable Head.
    pub async fn apply_artifact_lifecycle(
        &self,
        session_id: SessionId,
        artifact_id: ArtifactId,
        req: &ArtifactLifecycleRequest,
    ) -> Result<ArtifactLifecycleResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .apply_artifact_lifecycle(session_id, artifact_id, req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Fetches a consistent `REPEATABLE READ` control-plane snapshot and its exact `snapshot_cursor` (`GET /v1/state/snapshot`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if `dennett-head` is unreachable or rejects the request.
    pub async fn get_state_snapshot(
        &self,
        project_id: Option<ProjectId>,
        include_archived: bool,
    ) -> Result<StateSnapshotResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .get_state_snapshot(project_id, include_archived, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Tails ordered control-plane journal events after `after_cursor` (`GET /v1/events`, V15/V18).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] (including `CONFLICT` on `stream_epoch` / `stream_cursor_expired` resync)
    /// or unreachable Head.
    pub async fn get_events(
        &self,
        after_cursor: i64,
        expected_epoch: Option<i64>,
        project_id: Option<ProjectId>,
        limit: Option<u32>,
    ) -> Result<EventStreamResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .get_events(after_cursor, expected_epoch, project_id, limit, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Creates a durable Dennett task or scheduled obligation (`POST /v1/tasks`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure, policy violation, or unreachable Head.
    pub async fn create_task(&self, req: &CreateTaskRequest) -> Result<TaskRecord, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .create_task(req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Lists durable tasks filtered optionally by project or status (`GET /v1/tasks`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if `dennett-head` is unreachable or rejects the request.
    pub async fn list_tasks(
        &self,
        project_id: Option<ProjectId>,
        status: Option<&str>,
    ) -> Result<ListTasksResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .list_tasks(project_id, status, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Fetches full durable task detail including runs, timers, signals, effects, and open inbox cards.
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if the task does not exist or Head is unreachable.
    pub async fn get_task_detail(
        &self,
        task_id: TaskId,
    ) -> Result<TaskDetailResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .get_task_detail(task_id, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Updates a task's title, goal, schedule, or budget (`PATCH /v1/tasks/{id}`, bumping `generation`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure, conflict, or unreachable Head.
    pub async fn update_task(
        &self,
        task_id: TaskId,
        req: &UpdateTaskRequest,
    ) -> Result<TaskRecord, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .update_task(task_id, req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Delivers an idempotent signal to a durable task (`POST /v1/tasks/{id}/signals`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure, idempotency mismatch, or unreachable Head.
    pub async fn send_task_signal(
        &self,
        task_id: TaskId,
        req: &TaskSignalRequest,
    ) -> Result<TaskSignalResponse, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .send_task_signal(task_id, req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Answers a task's clarification request and resumes execution (`POST /v1/tasks/{id}/reply`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure, conflict, or unreachable Head.
    pub async fn reply_to_task(
        &self,
        task_id: TaskId,
        req: &ReplyTaskRequest,
    ) -> Result<TaskRecord, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .reply_to_task(task_id, req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Issues targeted `Stop` for a single durable task without affecting sibling tasks (`POST /v1/tasks/{id}/cancel`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if the task does not exist or Head is unreachable.
    pub async fn cancel_task(&self, task_id: TaskId) -> Result<CancelTaskResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .cancel_task(task_id, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Advances the test-isolated scheduler clock (`POST /v1/tasks/scheduler/tick`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if Head is not in `TestIsolated` mode or unreachable.
    pub async fn scheduler_tick(
        &self,
        req: &SchedulerTickRequest,
    ) -> Result<SchedulerTickResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .scheduler_tick(req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Lists Action Inbox decision cards (`GET /v1/inbox`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if `dennett-head` is unreachable or rejects the request.
    pub async fn list_inbox_cards(
        &self,
        status: Option<&str>,
        project_id: Option<ProjectId>,
    ) -> Result<ListInboxResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .list_inbox_cards(status, project_id, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Resolves an Action Inbox card with optimistic `expected_revision` checking (`POST /v1/inbox/{id}/resolve`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on stale revision conflict, missing card, or unreachable Head.
    pub async fn resolve_inbox_card(
        &self,
        card_id: InboxCardId,
        req: &ResolveInboxCardRequest,
    ) -> Result<ResolveInboxCardResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .resolve_inbox_card(card_id, req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Lists live interactive permission requests (`GET /v1/permissions/requests`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if `dennett-head` is unreachable or rejects the request.
    pub async fn list_permission_requests(
        &self,
        status: Option<&str>,
        session_id: Option<SessionId>,
    ) -> Result<ListPermissionRequestsResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .list_permission_requests(status, session_id, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Resolves a live interactive permission request (`POST /v1/permissions/requests/{id}/resolve`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure, conflict, or unreachable Head.
    pub async fn resolve_permission_request(
        &self,
        request_id: PermissionRequestId,
        req: &ResolvePermissionRequest,
    ) -> Result<ResolvePermissionResponse, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .resolve_permission_request(request_id, req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Lists revocable capability grants (`GET /v1/grants`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if `dennett-head` is unreachable or rejects the request.
    pub async fn list_capability_grants(
        &self,
        project_id: Option<ProjectId>,
        include_revoked: bool,
    ) -> Result<ListCapabilityGrantsResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .list_capability_grants(project_id, include_revoked, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Immediately revokes an active capability grant (`POST /v1/grants/{id}/revoke`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if the grant does not exist or Head is unreachable.
    pub async fn revoke_capability_grant(
        &self,
        grant_id: CapabilityGrantId,
        req: &RevokeGrantRequest,
    ) -> Result<CapabilityGrantRecord, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .revoke_capability_grant(grant_id, req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Sends a plain-text inter-agent coordination message (`POST /v1/agents/{id}/messages`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure, access denial, or unreachable Head.
    pub async fn send_agent_message(
        &self,
        recipient_agent_id: AgentId,
        req: &SendAgentMessageRequest,
    ) -> Result<SendAgentMessageResponse, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .send_agent_message(recipient_agent_id, req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Lists incoming plain-text messages for an agent (`GET /v1/agents/{id}/messages`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if the agent does not exist or Head is unreachable.
    pub async fn list_agent_messages(
        &self,
        recipient_agent_id: AgentId,
        undelivered_only: bool,
    ) -> Result<ListAgentMessagesResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .list_agent_messages(recipient_agent_id, undelivered_only, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Dispatches an idempotent external effect through `Head`'s `ExternalEffectPort` (`POST /v1/effects/dispatch`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure, idempotency mismatch, or unreachable Head.
    pub async fn dispatch_effect(
        &self,
        req: &DispatchEffectRequest,
    ) -> Result<DispatchEffectResponse, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .dispatch_effect(req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Lists tracked external effect claims (`GET /v1/effects`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if `dennett-head` is unreachable or rejects the request.
    pub async fn list_effects(
        &self,
        task_id: Option<TaskId>,
        session_id: Option<SessionId>,
        status: Option<&str>,
    ) -> Result<ListEffectsResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .list_effects(task_id, session_id, status, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Reconciles an `unknown` external effect claim without blind re-dispatch (`POST /v1/effects/{id}/reconcile`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if the claim is missing or Head is unreachable.
    pub async fn reconcile_effect(
        &self,
        effect_id: EffectClaimId,
        req: &ReconcileEffectRequest,
    ) -> Result<ReconcileEffectResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .reconcile_effect(effect_id, req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    // ========================================================================
    // Phase D3: SecretBroker, Provider Connections, Capabilities, Memory Fabric
    // ========================================================================

    /// Lists write-only `SecretBroker` slot metadata without ever exposing plaintext values (`GET /v1/secrets`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if `dennett-head` is unreachable or rejects the request.
    pub async fn list_secrets(&self) -> Result<ListSecretsResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .list_secrets(Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Stores or rotates an encrypted write-only secret in `SecretBroker` (`PUT /v1/secrets/{slot_key}`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure or unreachable Head.
    pub async fn set_secret(
        &self,
        slot_key: &str,
        req: &SetSecretRequest,
    ) -> Result<SecretMetadataRecord, BridgeError> {
        validate_secret_slot_key(slot_key)?;
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .set_secret(slot_key, req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Deletes a stored secret from `SecretBroker` (`DELETE /v1/secrets/{slot_key}`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure or unreachable Head.
    pub async fn delete_secret(&self, slot_key: &str) -> Result<SecretMetadataRecord, BridgeError> {
        validate_secret_slot_key(slot_key)?;
        let cid = CorrelationId::generate();
        self.client
            .delete_secret(slot_key, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Lists configured runtime provider connections and read-only planned provider descriptors (`GET /v1/runtime/connections`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if `dennett-head` is unreachable or rejects the request.
    pub async fn list_provider_connections(
        &self,
    ) -> Result<ListProviderConnectionsResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .list_provider_connections(Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Creates a new runtime provider connection (`POST /v1/runtime/connections`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure or unreachable Head.
    pub async fn create_provider_connection(
        &self,
        req: &UpsertProviderConnectionRequest,
    ) -> Result<ProviderConnectionRecord, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .create_provider_connection(req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Updates an existing runtime provider connection (`PATCH /v1/runtime/connections/{id}`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure, missing connection, or unreachable Head.
    pub async fn update_provider_connection(
        &self,
        connection_id: ProviderConnectionId,
        req: &UpsertProviderConnectionRequest,
    ) -> Result<ProviderConnectionRecord, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .update_provider_connection(connection_id, req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Deletes a runtime provider connection (`DELETE /v1/runtime/connections/{id}`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if the connection is missing or Head is unreachable.
    pub async fn delete_provider_connection(
        &self,
        connection_id: ProviderConnectionId,
    ) -> Result<DeleteProviderConnectionResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .delete_provider_connection(connection_id, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Probes a runtime provider connection and loads its live model catalog (`POST /v1/runtime/connections/{id}/probe`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if the connection is missing or Head is unreachable.
    pub async fn probe_provider_connection(
        &self,
        connection_id: ProviderConnectionId,
    ) -> Result<ProbeConnectionResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .probe_provider_connection(connection_id, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Activates a runtime provider connection as default interactive/background runtime (`POST /v1/runtime/connections/{id}/activate`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if the connection is missing or Head is unreachable.
    pub async fn activate_provider_connection(
        &self,
        connection_id: ProviderConnectionId,
        req: &ActivateConnectionRequest,
    ) -> Result<ProviderConnectionRecord, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .activate_provider_connection(connection_id, req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Lists registered skills, MCP servers, and plugins (`GET /v1/capabilities`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if `dennett-head` is unreachable or rejects the request.
    pub async fn list_capabilities(
        &self,
        project_id: Option<ProjectId>,
        scope_kind: Option<CapabilityScopeKind>,
        kind: Option<CapabilityKind>,
    ) -> Result<ListCapabilitiesResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .list_capabilities(project_id, scope_kind, kind, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Registers a skill, MCP server, or plugin capability (`POST /v1/capabilities`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure or unreachable Head.
    pub async fn create_capability(
        &self,
        req: &CreateCapabilityRequest,
    ) -> Result<CapabilityRecord, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .create_capability(req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Discovers project-local and read-only personal capability candidates (`POST /v1/capabilities/discover`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if `dennett-head` is unreachable or rejects the request.
    pub async fn discover_capabilities(
        &self,
        req: &DiscoverCapabilitiesRequest,
    ) -> Result<DiscoverCapabilitiesResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .discover_capabilities(req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Updates capability metadata, activation, scope, trust, or spec (`PATCH /v1/capabilities/{id}`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure, missing capability, or unreachable Head.
    pub async fn update_capability(
        &self,
        capability_id: CapabilityId,
        req: &UpdateCapabilityRequest,
    ) -> Result<CapabilityRecord, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .update_capability(capability_id, req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Removes a capability from Dennett's registry (`DELETE /v1/capabilities/{id}`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if the capability is missing or Head is unreachable.
    pub async fn delete_capability(
        &self,
        capability_id: CapabilityId,
    ) -> Result<DeleteCapabilityResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .delete_capability(capability_id, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Rolls back a capability to its previous saved snapshot (`POST /v1/capabilities/{id}/rollback`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if rollback is unavailable or Head is unreachable.
    pub async fn rollback_capability(
        &self,
        capability_id: CapabilityId,
    ) -> Result<CapabilityRecord, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .rollback_capability(capability_id, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Sets a per-project activation override for a capability (`PUT /v1/projects/{project_id}/capabilities/{capability_id}`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if the project/capability is missing or Head is unreachable.
    pub async fn set_project_capability_binding(
        &self,
        project_id: ProjectId,
        capability_id: CapabilityId,
        req: &SetProjectCapabilityBindingRequest,
    ) -> Result<CapabilityProjectBinding, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .set_project_capability_binding(project_id, capability_id, req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Commits a memory note via `Head`'s durable outbox (`POST /v1/memory/commits`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure or unreachable Head.
    pub async fn commit_memory_note(
        &self,
        req: &CommitMemoryNoteRequest,
    ) -> Result<CommitMemoryNoteResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .commit_memory_note(req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Fetches the V02-safe aggregated Memory Fabric overview (`GET /v1/memory/overview`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if `dennett-head` is unreachable or rejects the request.
    pub async fn get_memory_overview(
        &self,
        scope: Option<&MemoryScope>,
    ) -> Result<MemoryOverviewResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .get_memory_overview(scope, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Appends an immutable correction revision to a memory note (`POST /v1/memory/notes/{id}/correct`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure, conflict, or unreachable Head.
    pub async fn correct_memory_note(
        &self,
        req: &CorrectNoteRequest,
    ) -> Result<NoteMutationResponse, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .correct_memory_note(req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Fetches the immutable revision history of a memory note (`GET /v1/memory/notes/{id}/history`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if the note is missing or Head is unreachable.
    pub async fn get_memory_note_history(
        &self,
        note_id: MemoryNoteId,
    ) -> Result<NoteHistoryResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .get_memory_note_history(note_id, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Lists structured memory claims filtered by optional `scope` and `topic_key` (`GET /v1/memory/claims`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if `dennett-head` is unreachable or rejects the request.
    pub async fn list_memory_claims(
        &self,
        scope: Option<&MemoryScope>,
        topic_key: Option<&str>,
    ) -> Result<ListClaimsResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .list_memory_claims(scope, topic_key, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Creates a structured memory claim (`POST /v1/memory/claims`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure or unreachable Head.
    pub async fn create_memory_claim(
        &self,
        req: &CreateClaimRequest,
    ) -> Result<MemoryClaim, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .create_memory_claim(req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Supersedes an existing memory claim with a newer claim (`POST /v1/memory/claims/{id}/supersede`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure, conflict, or unreachable Head.
    pub async fn supersede_memory_claim(
        &self,
        req: &SupersedeClaimRequest,
    ) -> Result<MemoryClaim, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .supersede_memory_claim(req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Lists memory conflicts for an optional scope (`GET /v1/memory/conflicts`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if `dennett-head` is unreachable or rejects the request.
    pub async fn list_memory_conflicts(
        &self,
        scope: Option<&MemoryScope>,
    ) -> Result<ListConflictsResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .list_memory_conflicts(scope, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Resolves an open memory conflict (`POST /v1/memory/conflicts/{id}/resolve`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure, missing conflict, or unreachable Head.
    pub async fn resolve_memory_conflict(
        &self,
        req: &ResolveConflictRequest,
    ) -> Result<MemoryConflict, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .resolve_memory_conflict(req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Lists versioned memory dossier documents for an optional scope (`GET /v1/memory/documents`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if `dennett-head` is unreachable or rejects the request.
    pub async fn list_memory_documents(
        &self,
        scope: Option<&MemoryScope>,
    ) -> Result<ListDocumentsResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .list_memory_documents(scope, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Creates a new version-1 memory dossier document (`POST /v1/memory/documents`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure or unreachable Head.
    pub async fn create_memory_document(
        &self,
        req: &CreateDocumentRequest,
    ) -> Result<MemoryDocument, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .create_memory_document(req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Retrieves a memory dossier document by identifier (`GET /v1/memory/documents/{id}`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if the document is missing or Head is unreachable.
    pub async fn get_memory_document(
        &self,
        document_id: DocumentId,
    ) -> Result<MemoryDocument, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .get_memory_document(document_id, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Updates a memory dossier document with optimistic `expected_version` concurrency (`PUT /v1/memory/documents/{id}`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure, version conflict, or unreachable Head.
    pub async fn update_memory_document(
        &self,
        req: &UpdateDocumentRequest,
    ) -> Result<MemoryDocument, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .update_memory_document(req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Fetches all immutable versions of a memory dossier document (`GET /v1/memory/documents/{id}/versions`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if the document is missing or Head is unreachable.
    pub async fn get_memory_document_versions(
        &self,
        document_id: DocumentId,
    ) -> Result<GetDocumentVersionsResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .get_memory_document_versions(document_id, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Forgets a memory entity with search-index purge and content-free tombstone (`POST /v1/memory/forget`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure, missing item, or unreachable Head.
    pub async fn forget_memory_item(
        &self,
        req: &ForgetRequest,
    ) -> Result<ForgetResponse, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .forget_memory_item(req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Lists content-free forget audit tombstones for an optional scope (`GET /v1/memory/forget/tombstones`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if `dennett-head` is unreachable or rejects the request.
    pub async fn list_memory_tombstones(
        &self,
        scope: Option<&MemoryScope>,
    ) -> Result<ListTombstonesResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .list_memory_tombstones(scope, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Exports a deterministic, secret-free memory package for one scope (`POST /v1/memory/export`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if `dennett-head` is unreachable or rejects the request.
    pub async fn export_memory_package(
        &self,
        req: &ExportRequest,
    ) -> Result<ExportResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .export_memory_package(req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Imports an external memory package strictly as `external_untrusted` data (`POST /v1/memory/import`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure or unreachable Head.
    pub async fn import_memory_package(
        &self,
        req: &ImportRequest,
    ) -> Result<ImportResponse, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .import_memory_package(req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Triggers a full rebuild of `memoryd`'s separable search index (`POST /v1/memory/index/rebuild`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if `dennett-head` is unreachable or rejects the request.
    pub async fn trigger_memory_index_rebuild(
        &self,
        req: &TriggerIndexRebuildRequest,
    ) -> Result<TriggerIndexRebuildResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .trigger_memory_index_rebuild(req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Lists background memory consolidation jobs for an optional scope (`GET /v1/memory/consolidation/jobs`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if `dennett-head` is unreachable or rejects the request.
    pub async fn list_memory_consolidation_jobs(
        &self,
        scope: Option<&MemoryScope>,
    ) -> Result<ListConsolidationJobsResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .list_memory_consolidation_jobs(scope, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Creates a new interruptible memory consolidation job (`POST /v1/memory/consolidation/jobs`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if `dennett-head` is unreachable or rejects the request.
    pub async fn create_memory_consolidation_job(
        &self,
        req: &CreateConsolidationJobRequest,
    ) -> Result<ConsolidationJob, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .create_memory_consolidation_job(req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Pauses an active memory consolidation job at its checkpoint (`POST /v1/memory/consolidation/jobs/{id}/pause`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if the job is missing or Head is unreachable.
    pub async fn pause_memory_consolidation_job(
        &self,
        job_id: ConsolidationJobId,
    ) -> Result<ConsolidationJob, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .pause_memory_consolidation_job(job_id, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Resumes a paused memory consolidation job from its checkpoint (`POST /v1/memory/consolidation/jobs/{id}/resume`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if the job is missing or Head is unreachable.
    pub async fn resume_memory_consolidation_job(
        &self,
        job_id: ConsolidationJobId,
    ) -> Result<ConsolidationJob, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .resume_memory_consolidation_job(job_id, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    // ========================================================================
    // Phase D3+: Live Session Runs, Durable Follow-ups & Live Steering
    // ========================================================================

    /// Fetches the active streaming run snapshot and durable follow-up queue (`GET /v1/sessions/{id}/live`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if the session is missing or Head is unreachable.
    pub async fn get_session_live(
        &self,
        session_id: SessionId,
    ) -> Result<LiveRunResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .get_session_live(session_id, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Appends a durable follow-up message to a session (`POST /v1/sessions/{id}/followups`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure, missing session, or unreachable Head.
    pub async fn enqueue_followup(
        &self,
        session_id: SessionId,
        req: &EnqueueFollowupRequest,
    ) -> Result<SessionFollowupRecord, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .enqueue_followup(session_id, req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Edits a queued or paused follow-up message (`PATCH /v1/sessions/{id}/followups/{fid}`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure, missing follow-up, or unreachable Head.
    pub async fn update_followup(
        &self,
        session_id: SessionId,
        followup_id: FollowupId,
        req: &UpdateFollowupRequest,
    ) -> Result<SessionFollowupRecord, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .update_followup(session_id, followup_id, req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Removes one follow-up message from a session's queue (`DELETE /v1/sessions/{id}/followups/{fid}`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if the session/follow-up is missing or Head is unreachable.
    pub async fn delete_followup(
        &self,
        session_id: SessionId,
        followup_id: FollowupId,
    ) -> Result<(), BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .delete_followup(session_id, followup_id, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Clears all pending follow-up messages for a session (`DELETE /v1/sessions/{id}/followups`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if the session is missing or Head is unreachable.
    pub async fn clear_followups(&self, session_id: SessionId) -> Result<(), BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .clear_followups(session_id, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Resumes paused follow-up messages for a session (`POST /v1/sessions/{id}/followups/resume`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if the session is missing or Head is unreachable.
    pub async fn resume_followups(
        &self,
        session_id: SessionId,
    ) -> Result<ResumeFollowupsResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .resume_followups(session_id, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Interrupts any active run and immediately steers the session with a new prompt (`POST /v1/sessions/{id}/steer`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure, runtime error, or unreachable Head.
    pub async fn steer_session(
        &self,
        session_id: SessionId,
        req: &SteerSessionRequest,
    ) -> Result<SendMessageResponse, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .steer_session(session_id, req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    // ========================================================================
    // Phase D3+ Audit Closure: Artifact Content, Fork, Preferences, Policies, Controls & Index Status
    // ========================================================================

    /// Fetches the immutable byte snapshot of an artifact revision (`GET /v1/sessions/{id}/artifacts/{aid}/content`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if the artifact is missing or Head is unreachable.
    pub async fn get_artifact_content(
        &self,
        session_id: SessionId,
        artifact_id: ArtifactId,
    ) -> Result<DesktopArtifactContentResponse, BridgeError> {
        let cid = CorrelationId::generate();
        let raw = self
            .client
            .get_artifact_content(session_id, artifact_id, Some(&cid))
            .await
            .map_err(BridgeError::from)?;
        let byte_size = raw.bytes.len();
        let base64 = encode_base64(&raw.bytes);
        let has_nul = raw.bytes.contains(&0);
        let text = if has_nul {
            None
        } else {
            String::from_utf8(raw.bytes).ok()
        };
        let is_binary = text.is_none();
        Ok(DesktopArtifactContentResponse {
            mime_type: raw.mime_type,
            sha256: raw.sha256.as_str().to_owned(),
            byte_size,
            text,
            base64,
            is_binary,
        })
    }

    /// Lists all immutable configuration revisions for an agent (`GET /v1/agents/{id}/config-versions`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if the agent is missing or Head is unreachable.
    pub async fn get_agent_config_history(
        &self,
        agent_id: AgentId,
    ) -> Result<AgentConfigHistoryResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .get_agent_config_history(agent_id, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Lists configured external policy rules (`GET /v1/policies`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if Head is unreachable or rejects the request.
    pub async fn list_policy_rules(&self) -> Result<ListPolicyRulesResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .list_policy_rules(Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Creates a new external policy rule (`POST /v1/policies`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure or unreachable Head.
    pub async fn create_policy_rule(
        &self,
        req: &CreatePolicyRuleRequest,
    ) -> Result<PolicyRule, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .create_policy_rule(req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Deletes an external policy rule (`DELETE /v1/policies/{id}`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if the rule is missing or Head is unreachable.
    pub async fn delete_policy_rule(&self, rule_id: PolicyRuleId) -> Result<(), BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .delete_policy_rule(rule_id, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Forks a session from an existing message (`POST /v1/sessions/{id}/fork`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure, missing session/message, or unreachable Head.
    pub async fn fork_session(
        &self,
        session_id: SessionId,
        req: &ForkSessionRequest,
    ) -> Result<SessionRecord, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .fork_session(session_id, req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Deletes a persisted session record (`DELETE /v1/sessions/{id}`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if the session is missing or Head is unreachable.
    pub async fn delete_session(&self, session_id: SessionId) -> Result<(), BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .delete_session(session_id, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Fetches persisted owner preferences (`GET /v1/preferences`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if Head is unreachable or rejects the request.
    pub async fn get_preferences(&self) -> Result<UserPreferences, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .get_preferences(Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Updates persisted owner preferences (`PATCH /v1/preferences`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on validation failure or unreachable Head.
    pub async fn update_preferences(
        &self,
        req: &UpdateUserPreferencesRequest,
    ) -> Result<UserPreferences, BridgeError> {
        req.validate()?;
        let cid = CorrelationId::generate();
        self.client
            .update_preferences(req, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Reads a bounded preview of a workspace file inside a project (`GET /v1/projects/{id}/file?path=...`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] on invalid path, boundary denial, missing file, or unreachable Head.
    pub async fn get_project_file_content(
        &self,
        project_id: ProjectId,
        relative_path: &str,
    ) -> Result<ProjectFileContentResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .get_project_file_content(project_id, relative_path, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Cancels all active runs and tasks immediately (`POST /v1/runs/emergency-stop`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if Head is unreachable or rejects the request.
    pub async fn emergency_stop_all(&self) -> Result<EmergencyStopResponse, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .emergency_stop_all(Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Triggers immediate execution of a durable task (`POST /v1/tasks/{id}/run-now`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if the task is missing or Head is unreachable.
    pub async fn run_task_now(&self, task_id: TaskId) -> Result<TaskRecord, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .run_task_now(task_id, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Pauses a scheduled or queued durable task (`POST /v1/tasks/{id}/pause`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if the task is missing or Head is unreachable.
    pub async fn pause_task(&self, task_id: TaskId) -> Result<TaskRecord, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .pause_task(task_id, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Resumes a paused durable task (`POST /v1/tasks/{id}/resume`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if the task is missing or Head is unreachable.
    pub async fn resume_task(&self, task_id: TaskId) -> Result<TaskRecord, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .resume_task(task_id, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Deletes a durable task record (`DELETE /v1/tasks/{id}`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if the task is missing or Head is unreachable.
    pub async fn delete_task(&self, task_id: TaskId) -> Result<(), BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .delete_task(task_id, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }

    /// Queries the memory search index watermark and health status (`GET /v1/memory/index/status`).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] if Head is unreachable or rejects the request.
    pub async fn get_memory_index_status(
        &self,
        scope: Option<&MemoryScope>,
    ) -> Result<IndexState, BridgeError> {
        let cid = CorrelationId::generate();
        self.client
            .get_memory_index_status(scope, Some(&cid))
            .await
            .map_err(BridgeError::from)
    }
}

const _: () = {
    const fn assert_send_sync<T: Send + Sync + 'static>() {}
    assert_send_sync::<HeadBridge>();
    assert_send_sync::<BridgeError>();
    assert_send_sync::<DesktopHealthSnapshot>();
    assert_send_sync::<DesktopArtifactContentResponse>();
};

#[cfg(test)]
mod tests {
    use tokio::net::TcpListener;

    use super::*;

    #[test]
    fn validate_head_url_accepts_loopback_and_normalizes_trailing_slash() {
        for (raw, expected) in [
            ("http://127.0.0.1:41771", "http://127.0.0.1:41771"),
            ("http://127.0.0.1:41771/", "http://127.0.0.1:41771"),
            ("http://localhost:18081/", "http://localhost:18081"),
            ("http://[::1]:41771/", "http://[::1]:41771"),
            ("https://127.0.0.1:8443", "https://127.0.0.1:8443"),
        ] {
            let got = validate_head_url(raw, HeadEndpointPolicy::LoopbackOnly)
                .unwrap_or_else(|err| panic!("expected valid loopback URL `{raw}`: {err}"));
            assert_eq!(got, expected, "raw={raw}");
        }
    }

    #[test]
    fn validate_head_url_rejects_remote_credentials_subpath_and_bad_schemes() {
        for bad in [
            "",
            "   ",
            "ftp://127.0.0.1:41771",
            "file:///etc/passwd",
            "http://user:supersecret@127.0.0.1:41771",
            "http://127.0.0.1:41771/v1",
            "http://127.0.0.1:41771?token=secret",
            "http://127.0.0.1:41771#frag",
            "http://192.168.1.50:41771",
            "https://head.example.com",
        ] {
            assert!(
                validate_head_url(bad, HeadEndpointPolicy::LoopbackOnly).is_err(),
                "expected rejection under LoopbackOnly for `{bad}`"
            );
        }

        // Explicit remote opt-in allows clean remote HTTPS host, still blocks embedded credentials.
        assert_eq!(
            validate_head_url(
                "https://head.internal:8443/",
                HeadEndpointPolicy::AllowConfiguredRemote
            )
            .expect("explicit remote allowed"),
            "https://head.internal:8443"
        );
        assert!(
            validate_head_url(
                "https://admin:pw@head.internal:8443",
                HeadEndpointPolicy::AllowConfiguredRemote
            )
            .is_err()
        );
    }

    #[test]
    fn bridge_error_redacts_secrets_when_serialized() {
        let raw = DennettError::Internal {
            detail: String::from(
                "failed talking to postgres://user:top_secret_pw@127.0.0.1:5432/db with Bearer sk-live-secret-token-12345",
            ),
        };
        let err = BridgeError::from(raw);
        let serialized = serde_json::to_string(&err).expect("serialize BridgeError");
        assert!(
            !serialized.contains("top_secret_pw"),
            "password leaked in serialized BridgeError: {serialized}"
        );
        assert!(
            !serialized.contains("sk-live-secret-token-12345"),
            "bearer token leaked in serialized BridgeError: {serialized}"
        );
        assert!(serialized.contains("\"code\":\"INTERNAL\""));
    }

    #[tokio::test]
    async fn get_head_health_reports_disconnected_honestly_on_unreachable_port() {
        // Bind and immediately drop an ephemeral loopback listener to get an unused port.
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let port = listener.local_addr().expect("local addr").port();
        drop(listener);

        let url = format!("http://127.0.0.1:{port}");
        let bridge =
            HeadBridge::new(&url, HeadEndpointPolicy::LoopbackOnly).expect("valid loopback bridge");
        let snapshot = bridge.get_head_health().await;

        assert!(!snapshot.connected);
        assert!(snapshot.health.is_none());
        assert_eq!(snapshot.head_url, url);
        assert_eq!(snapshot.error_code.as_deref(), Some("HEAD_UNREACHABLE"));
        assert!(snapshot.error.is_some());
    }

    fn json_http_response(body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    async fn spawn_mock_live_server(
        session_id: SessionId,
        followup_id: FollowupId,
    ) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock head server");
        let addr = listener.local_addr().expect("local addr");
        let now_ms = dennett_contracts::TimestampMs::now();
        let followup_json = serde_json::to_string(&SessionFollowupRecord {
            followup_id,
            session_id,
            content: String::from("Уточнить список модулей"),
            status: dennett_contracts::FollowupStatus::Queued,
            position: 1,
            idempotency_key: Some(String::from("idem-1")),
            dispatched_run_id: None,
            created_at: now_ms,
            updated_at: now_ms,
        })
        .expect("serialize followup");
        let live_json = serde_json::to_string(&LiveRunResponse {
            active: Some(dennett_contracts::LiveRunSnapshot {
                session_id,
                run_id: dennett_contracts::RunId::new(),
                streaming_text: String::from("Промежуточный текст"),
                context_used_tokens: Some(12_700),
                context_window_tokens: Some(1_000_000),
                started_at: now_ms,
                updated_at: now_ms,
            }),
            followups: vec![],
        })
        .expect("serialize live");
        let resume_json = serde_json::to_string(&ResumeFollowupsResponse {
            followups: vec![],
            dispatched: None,
        })
        .expect("serialize resume");

        let handle = tokio::spawn(async move {
            for _ in 0..6 {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let mut buf = [0_u8; 4096];
                let n = socket.read(&mut buf).await.unwrap_or(0);
                let req_str = String::from_utf8_lossy(&buf[..n]);
                let first_line = req_str.lines().next().unwrap_or("");
                let response = if first_line
                    .starts_with(&format!("GET /v1/sessions/{session_id}/live "))
                {
                    json_http_response(&live_json)
                } else if first_line
                    .starts_with(&format!("POST /v1/sessions/{session_id}/followups "))
                    || first_line.starts_with(&format!(
                        "PATCH /v1/sessions/{session_id}/followups/{followup_id} "
                    ))
                {
                    json_http_response(&followup_json)
                } else if first_line.starts_with(&format!(
                    "DELETE /v1/sessions/{session_id}/followups/{followup_id} "
                )) || first_line
                    .starts_with(&format!("DELETE /v1/sessions/{session_id}/followups "))
                {
                    String::from(
                        "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                } else if first_line
                    .starts_with(&format!("POST /v1/sessions/{session_id}/followups/resume "))
                {
                    json_http_response(&resume_json)
                } else {
                    String::from(
                        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                };
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.shutdown().await;
            }
        });
        (addr, handle)
    }

    #[tokio::test]
    async fn live_followup_and_steer_bridge_methods_validate_and_roundtrip() {
        let session_id = SessionId::new();
        let followup_id = FollowupId::new();
        let (addr, server) = spawn_mock_live_server(session_id, followup_id).await;
        let bridge = HeadBridge::new(&format!("http://{addr}"), HeadEndpointPolicy::LoopbackOnly)
            .expect("valid bridge");

        assert!(
            bridge
                .enqueue_followup(
                    session_id,
                    &EnqueueFollowupRequest {
                        content: String::from("   "),
                        idempotency_key: None,
                    }
                )
                .await
                .is_err()
        );
        assert!(
            bridge
                .update_followup(
                    session_id,
                    followup_id,
                    &UpdateFollowupRequest {
                        content: String::new(),
                    }
                )
                .await
                .is_err()
        );
        assert!(
            bridge
                .steer_session(
                    session_id,
                    &SteerSessionRequest {
                        content: String::from("  "),
                        client_message_id: None,
                        timeout_ms: None,
                    }
                )
                .await
                .is_err()
        );

        let live = bridge
            .get_session_live(session_id)
            .await
            .expect("get_session_live");
        assert_eq!(
            live.active.as_ref().map(|a| a.streaming_text.as_str()),
            Some("Промежуточный текст")
        );
        assert_eq!(
            live.active.as_ref().and_then(|a| a.context_used_tokens),
            Some(12_700)
        );

        let queued = bridge
            .enqueue_followup(
                session_id,
                &EnqueueFollowupRequest {
                    content: String::from("Уточнить список модулей"),
                    idempotency_key: Some(String::from("idem-1")),
                },
            )
            .await
            .expect("enqueue_followup");
        assert_eq!(queued.followup_id, followup_id);

        let updated = bridge
            .update_followup(
                session_id,
                followup_id,
                &UpdateFollowupRequest {
                    content: String::from("Обновлённое уточнение"),
                },
            )
            .await
            .expect("update_followup");
        assert_eq!(updated.followup_id, followup_id);

        bridge
            .delete_followup(session_id, followup_id)
            .await
            .expect("delete_followup 204");
        bridge
            .clear_followups(session_id)
            .await
            .expect("clear_followups 204");

        let resumed = bridge
            .resume_followups(session_id)
            .await
            .expect("resume_followups");
        assert_eq!(resumed.followups, Vec::<SessionFollowupRecord>::new());

        server.await.expect("mock server done");
    }

    #[test]
    fn base64_encoder_matches_rfc4648_vectors() {
        assert_eq!(encode_base64(b""), "");
        assert_eq!(encode_base64(b"f"), "Zg==");
        assert_eq!(encode_base64(b"fo"), "Zm8=");
        assert_eq!(encode_base64(b"foo"), "Zm9v");
        assert_eq!(encode_base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(encode_base64(&[0x00, 0xFF, 0x10]), "AP8Q");
    }

    #[test]
    fn read_owner_token_from_dir_reads_trimmed_token() {
        let dir = env::temp_dir().join(format!("dennett-tok-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).expect("mkdir");
        assert_eq!(read_owner_token_from_dir(&dir), None);
        fs::write(dir.join("head.token"), "  owner-tok-test-99 \r\n").expect("write token");
        assert_eq!(
            read_owner_token_from_dir(&dir).as_deref(),
            Some("owner-tok-test-99")
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn head_bridge_sends_owner_bearer_token_and_fetches_artifact_content() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let session_id = SessionId::new();
        let artifact_id = ArtifactId::new();
        let expected_sha = "a".repeat(64);
        let sha_clone = expected_sha.clone();
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock server");
        let addr = listener.local_addr().expect("local addr");

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut buf = [0_u8; 4096];
            let n = socket.read(&mut buf).await.unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            assert!(
                req.to_ascii_lowercase()
                    .contains("authorization: bearer desktop-secret-tok-42"),
                "expected Authorization Bearer header, got: {req}"
            );
            let body = "# Snapshot\nhello";
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/markdown\r\nX-Dennett-Artifact-Sha256: {sha_clone}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(resp.as_bytes()).await;
            let _ = socket.shutdown().await;
        });

        let bridge = HeadBridge::with_owner_token(
            &format!("http://{addr}"),
            HeadEndpointPolicy::LoopbackOnly,
            Some(String::from("desktop-secret-tok-42")),
        )
        .expect("bridge");
        // Debug format must never leak the raw owner token.
        let dbg = format!("{bridge:?}");
        assert!(!dbg.contains("desktop-secret-tok-42"));

        let content = bridge
            .get_artifact_content(session_id, artifact_id)
            .await
            .expect("get_artifact_content");
        assert_eq!(content.mime_type, "text/markdown");
        assert_eq!(content.sha256, expected_sha);
        assert!(!content.is_binary);
        assert_eq!(content.text.as_deref(), Some("# Snapshot\nhello"));
        assert_eq!(content.base64, encode_base64(b"# Snapshot\nhello"));

        server.await.expect("server join");
    }
}
