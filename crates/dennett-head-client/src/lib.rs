//! Typed async HTTP client for `dennett-head` (`/v1/*` and `/healthz`).
//!
//! Architectural boundary (Architecture 80 §6): depends ONLY on `dennett-contracts` and
//! HTTP/JSON transport crates — NEVER on `dennett-head`, `dennett-memory-core`, or PostgreSQL
//! drivers, so `dennettctl` and future `apps/desktop` UI reuse one canonical typed client.

use std::time::Duration;

use dennett_contracts::redact::redact_secrets;
use dennett_contracts::{
    AgentConfigHistoryResponse, AgentId, AgentRecord, ApiErrorResponse, ArtifactRecord,
    CancelRunResponse, CommitMemoryNoteRequest, CommitMemoryNoteResponse, CorrelationId,
    CreateAgentRequest, CreateArtifactRequest, CreatePolicyRuleRequest, CreateProjectRequest,
    CreateSessionRequest, DennettError, DiagnosticsResponse, FlushOutboxResponse, HeadHealth,
    HeadMemorySearchRequest, HeadMemorySearchResponse, ListAgentsResponse, ListArtifactsResponse,
    ListOutboxResponse, ListPolicyRulesResponse, ListProjectsResponse, ListSessionsResponse,
    PolicyRule, ProjectId, ProjectRecord, SendMessageRequest, SendMessageResponse,
    SessionDetailResponse, SessionId, SessionRecord, UpdateAgentConfigRequest,
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

/// Typed asynchronous HTTP client communicating with a `dennett-head` daemon.
#[derive(Debug, Clone)]
pub struct HeadClient {
    base_url: String,
    http: Client,
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
        })
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

    async fn request_no_body<Resp: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<Resp, DennettError> {
        let url = format!("{}{path}", self.base_url);
        let headers = build_headers(correlation_id)?;
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
        let headers = build_headers(correlation_id)?;
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

fn build_headers(correlation_id: Option<&CorrelationId>) -> Result<HeaderMap, DennettError> {
    let mut headers = HeaderMap::new();
    let cid = correlation_id
        .cloned()
        .unwrap_or_else(CorrelationId::generate);
    let val = HeaderValue::from_str(cid.as_str()).map_err(|_| DennettError::InvalidInput {
        field: String::from("correlation_id"),
        reason: String::from("invalid HTTP header value"),
    })?;
    headers.insert(HEADER_CORRELATION_ID.clone(), val);
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
