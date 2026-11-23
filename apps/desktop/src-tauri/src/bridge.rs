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
use std::net::IpAddr;
use std::time::Duration;

use dennett_contracts::redact::redact_secrets;
use dennett_contracts::{
    AgentId, AgentRecord, ArtifactId, ArtifactRecord, CancelRunResponse, CorrelationId,
    CreateAgentRequest, CreateArtifactRequest, CreateProjectRequest, CreateSessionRequest,
    DennettError, DiagnosticsResponse, FlushOutboxResponse, HeadHealth, HeadMemorySearchRequest,
    HeadMemorySearchResponse, ListAgentsResponse, ListArtifactsResponse, ListProjectFilesResponse,
    ListProjectsResponse, ListSessionsResponse, ProjectId, ProjectRecord, RuntimeCatalogResponse,
    SendMessageRequest, SendMessageResponse, SessionDetailResponse, SessionId, SessionRecord,
    UpdateAgentConfigRequest, UpdateProjectRequest, UpdateSessionRequest,
};
use dennett_head_client::HeadClient;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::{Host, Url};

/// Default loopback endpoint used when `DENNETT_HEAD_URL` is unset.
pub const DEFAULT_HEAD_URL: &str = "http://127.0.0.1:41771";
/// Environment variable overriding the `dennett-head` base URL for the desktop shell.
pub const HEAD_URL_ENV: &str = "DENNETT_HEAD_URL";
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
    /// Constructs a [`HeadBridge`] after validating `base_url` against `policy`.
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError::InvalidConfig`] if `base_url` fails validation.
    pub fn new(base_url: &str, policy: HeadEndpointPolicy) -> Result<Self, BridgeError> {
        let normalized = validate_head_url(base_url, policy)?;
        let client = HeadClient::new(&normalized)?;
        let health_client = HeadClient::with_timeout(&normalized, HEALTH_PROBE_TIMEOUT)?;
        Ok(Self {
            client,
            health_client,
            policy,
        })
    }

    /// Constructs a [`HeadBridge`] from `DENNETT_HEAD_URL` (default [`DEFAULT_HEAD_URL`])
    /// and [`ALLOW_REMOTE_HEAD_ENV`].
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
        Self::new(&raw_url, policy)
    }

    /// Returns the normalized `dennett-head` base URL.
    #[must_use]
    pub fn head_url(&self) -> &str {
        self.client.base_url()
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

    /// Renames a project and/or attaches/rebinds its workspace folder (`PATCH /v1/projects/{id}`).
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

    /// Renames a conversation session inline (`PATCH /v1/sessions/{id}`).
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
}

const _: () = {
    const fn assert_send_sync<T: Send + Sync + 'static>() {}
    assert_send_sync::<HeadBridge>();
    assert_send_sync::<BridgeError>();
    assert_send_sync::<DesktopHealthSnapshot>();
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
}
