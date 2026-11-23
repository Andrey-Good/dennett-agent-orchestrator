//! Typed async HTTP client for standalone `dennett-memoryd` (`/v1/memory/*` and `/healthz`).
//!
//! Architectural boundary (Architecture 80 §6): depends ONLY on `dennett-contracts` and
//! HTTP/JSON transport crates — NEVER on `dennett-memory-core` or PostgreSQL drivers.

use core::fmt;
use std::time::Duration;

use dennett_contracts::redact::{REDACTED_PLACEHOLDER, redact_secrets};
use dennett_contracts::{
    ApiErrorResponse, CorrectNoteRequest, CorrelationId, CreateNoteRequest, DennettError,
    EvidenceId, EvidenceObject, IngestEvidenceRequest, IngestEvidenceResponse, MemoryEvent,
    MemoryEventId, MemoryNote, MemoryNoteId, NoteHistoryResponse, NoteMutationResponse,
    SearchNotesRequest, SearchNotesResponse, ServiceHealth,
};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Method, StatusCode};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Default total request timeout for [`MemoryClient`] HTTP calls.
pub const DEFAULT_CLIENT_TIMEOUT: Duration = Duration::from_secs(15);
/// Default TCP connect timeout for [`MemoryClient`].
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

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
        let status = resp.status();
        let body_bytes = resp
            .bytes()
            .await
            .map_err(|err| self.map_transport_error(&err))?;

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

const _: () = {
    const fn assert_send_sync<T: Send + Sync + 'static>() {}
    assert_send_sync::<MemoryClient>();
};

#[cfg(test)]
mod tests {
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
}
