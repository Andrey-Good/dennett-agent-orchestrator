//! `axum` HTTP server router and handlers for `dennett-head`.
//!
//! Enforces loopback-by-default binding, bounded request bodies, concurrency limits,
//! correlation ID propagation, secret-redacted error envelopes, and strict test-only
//! `POST /shutdown` guards.

use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::rejection::JsonRejection;
use axum::extract::{ConnectInfo, DefaultBodyLimit, Path, Query, Request, State};
use axum::http::header::HOST;
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use dennett_contracts::redact::redact_secrets;
use dennett_contracts::{
    AgentId, ApiErrorResponse, CommitMemoryNoteRequest, CorrelationId, CreateAgentRequest,
    CreateArtifactRequest, CreatePolicyRuleRequest, CreateProjectRequest, CreateSessionRequest,
    DennettError, HeadHealth, HeadMemorySearchRequest, MemoryStatus, ProfileMode, ProjectId,
    SendMessageRequest, ServiceKind, SessionId, UpdateAgentConfigRequest,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{Semaphore, watch};
use tokio::time;

use crate::service::{HEAD_STAGE_LABEL, HeadService, d1_head_capabilities};

/// Maximum incoming HTTP request body size (1 MiB).
pub const MAX_HTTP_BODY_BYTES: usize = 1024 * 1024;
/// Maximum concurrent in-flight HTTP requests accepted by `dennett-head`.
pub const MAX_CONCURRENT_REQUESTS: usize = 64;
/// Upper bound on end-to-end HTTP request wall time (accommodates live runtime turns).
pub const HTTP_REQUEST_TIMEOUT: Duration = Duration::from_secs(180);

static HEADER_CORRELATION_ID: HeaderName = HeaderName::from_static("x-correlation-id");
static HEADER_SHUTDOWN: HeaderName = HeaderName::from_static("x-dennett-shutdown");

/// Shared `axum` application state for `dennett-head`.
#[derive(Clone)]
pub struct HeadAppState {
    /// Optional configured [`HeadService`] (`None` only if control PG URL was omitted).
    pub service: Option<HeadService>,
    /// Active execution profile mode.
    pub profile_mode: ProfileMode,
    /// Runtime adapter identifier advertised on health failure fallback.
    pub runtime_adapter_id: String,
    /// Whether the configured runtime is simulated.
    pub runtime_is_simulated: bool,
    /// Graceful shutdown trigger channel.
    pub shutdown_tx: watch::Sender<bool>,
    /// Bounded concurrency permit pool.
    pub request_limit: Arc<Semaphore>,
}

#[derive(Debug, Serialize)]
struct HealthFailureBody {
    #[serde(flatten)]
    health: HeadHealth,
    error: DennettError,
}

#[derive(Debug, Serialize)]
struct ShutdownAck {
    status: &'static str,
    correlation_id: CorrelationId,
}

#[derive(Debug, Deserialize)]
struct DiagnosticsQuery {
    #[serde(default)]
    limit: Option<i64>,
}

/// Builds the complete `axum` HTTP [`Router`] for `dennett-head`.
pub fn build_head_router(state: HeadAppState) -> Router {
    let mut router = Router::new()
        .route("/healthz", get(handle_healthz))
        .route(
            "/v1/agents",
            post(handle_create_agent).get(handle_list_agents),
        )
        .route("/v1/agents/{agent_id}", get(handle_get_agent))
        .route(
            "/v1/agents/{agent_id}/config-versions",
            post(handle_update_agent_config).get(handle_get_agent_config_history),
        )
        .route(
            "/v1/projects",
            post(handle_create_project).get(handle_list_projects),
        )
        .route("/v1/projects/{project_id}", get(handle_get_project))
        .route(
            "/v1/sessions",
            post(handle_create_session).get(handle_list_sessions),
        )
        .route("/v1/sessions/{session_id}", get(handle_get_session_detail))
        .route(
            "/v1/sessions/{session_id}/messages",
            post(handle_send_message),
        )
        .route("/v1/sessions/{session_id}/cancel", post(handle_cancel_run))
        .route(
            "/v1/sessions/{session_id}/artifacts",
            post(handle_create_artifact).get(handle_list_artifacts),
        )
        .route(
            "/v1/policies",
            post(handle_create_policy_rule).get(handle_list_policy_rules),
        )
        .route("/v1/memory/commits", post(handle_commit_memory_note))
        .route("/v1/memory/outbox", get(handle_list_outbox))
        .route("/v1/memory/outbox/flush", post(handle_flush_outbox))
        .route("/v1/memory/search", post(handle_search_memory))
        .route("/v1/diagnostics", get(handle_diagnostics));

    if state.profile_mode == ProfileMode::TestIsolated {
        router = router.route("/shutdown", post(handle_shutdown));
    }

    router
        .fallback(handle_not_found)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            enforce_limits_and_timeout,
        ))
        .layer(DefaultBodyLimit::max(MAX_HTTP_BODY_BYTES))
        .with_state(state)
}

async fn enforce_limits_and_timeout(
    State(state): State<HeadAppState>,
    request: Request,
    next: Next,
) -> Response {
    let correlation_id = extract_correlation_id_lossy(request.headers());

    let Ok(_permit) = Arc::clone(&state.request_limit).try_acquire_owned() else {
        let err = DennettError::Internal {
            detail: String::from("concurrent request limit reached"),
        };
        return json_api_error(StatusCode::SERVICE_UNAVAILABLE, err, &correlation_id);
    };

    if let Ok(resp) = time::timeout(HTTP_REQUEST_TIMEOUT, next.run(request)).await {
        resp
    } else {
        let err = DennettError::Timeout {
            operation: String::from("head_http_request"),
        };
        json_api_error(StatusCode::GATEWAY_TIMEOUT, err, &correlation_id)
    }
}

async fn handle_healthz(State(state): State<HeadAppState>, headers: HeaderMap) -> Response {
    let correlation_id = match extract_correlation_id(&headers) {
        Ok(id) => id,
        Err(err) => {
            return json_api_error(StatusCode::BAD_REQUEST, err, &CorrelationId::generate());
        }
    };

    let result = match &state.service {
        Some(svc) => svc.health_check(correlation_id.clone()).await,
        None => Err(unconfigured_control_pg_error()),
    };

    match result {
        Ok(health) => json_with_correlation(StatusCode::OK, &health, &correlation_id),
        Err(err) => {
            let body = HealthFailureBody {
                health: HeadHealth {
                    service: ServiceKind::Head,
                    stage: String::from(HEAD_STAGE_LABEL),
                    profile_mode: state.profile_mode,
                    postgres_connected: false,
                    postgres_role: None,
                    postgres_database: None,
                    schema_version: None,
                    memory_status: MemoryStatus::DegradedUnreachable,
                    memory_detail: Some(String::from("control PostgreSQL unavailable")),
                    runtime_adapter_id: state.runtime_adapter_id.clone(),
                    runtime_is_simulated: state.runtime_is_simulated,
                    pending_outbox_count: 0,
                    capabilities: d1_head_capabilities(),
                    correlation_id: correlation_id.clone(),
                },
                error: err,
            };
            json_with_correlation(StatusCode::SERVICE_UNAVAILABLE, &body, &correlation_id)
        }
    }
}

async fn handle_shutdown(
    State(state): State<HeadAppState>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    _body: Bytes,
) -> Response {
    let correlation_id = extract_correlation_id_lossy(&headers);

    if state.profile_mode != ProfileMode::TestIsolated {
        let err = DennettError::NotFound {
            resource: String::from("http.path"),
            id: String::from("/shutdown"),
        };
        return json_api_error(StatusCode::NOT_FOUND, err, &correlation_id);
    }

    let shutdown_header_valid = headers
        .get(&HEADER_SHUTDOWN)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.trim() == "1");
    let host_ok = headers
        .get(HOST)
        .and_then(|v| v.to_str().ok())
        .is_none_or(is_loopback_host);

    if !peer_addr.ip().is_loopback() || !shutdown_header_valid || !host_ok {
        let err = DennettError::AccessDenied {
            scope: String::from("/shutdown"),
            reason: String::from(
                "shutdown requires test_isolated mode, loopback peer/Host, and `X-Dennett-Shutdown: 1`",
            ),
        };
        return json_api_error(StatusCode::FORBIDDEN, err, &correlation_id);
    }

    let ack = ShutdownAck {
        status: "shutting_down",
        correlation_id: correlation_id.clone(),
    };
    let _ = state.shutdown_tx.send(true);
    json_with_correlation(StatusCode::OK, &ack, &correlation_id)
}

async fn handle_create_agent(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    payload: Result<Json<CreateAgentRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc.create_agent(req, &cid).await {
        Ok(agent) => json_with_correlation(StatusCode::CREATED, &agent, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_list_agents(State(state): State<HeadAppState>, headers: HeaderMap) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    match svc.list_agents().await {
        Ok(list) => json_with_correlation(StatusCode::OK, &list, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_get_agent(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let agent_id = match AgentId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc.get_agent(agent_id).await {
        Ok(agent) => json_with_correlation(StatusCode::OK, &agent, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_update_agent_config(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
    payload: Result<Json<UpdateAgentConfigRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let agent_id = match AgentId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc.update_agent_config(agent_id, req, &cid).await {
        Ok(agent) => json_with_correlation(StatusCode::CREATED, &agent, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_get_agent_config_history(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let agent_id = match AgentId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc.get_agent_config_history(agent_id).await {
        Ok(hist) => json_with_correlation(StatusCode::OK, &hist, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_create_project(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    payload: Result<Json<CreateProjectRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc.create_project(req, &cid).await {
        Ok(project) => json_with_correlation(StatusCode::CREATED, &project, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_list_projects(State(state): State<HeadAppState>, headers: HeaderMap) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    match svc.list_projects().await {
        Ok(list) => json_with_correlation(StatusCode::OK, &list, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_get_project(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let project_id = match ProjectId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc.get_project(project_id).await {
        Ok(project) => json_with_correlation(StatusCode::OK, &project, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_create_session(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    payload: Result<Json<CreateSessionRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc.create_session(req, &cid).await {
        Ok(session) => json_with_correlation(StatusCode::CREATED, &session, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_list_sessions(State(state): State<HeadAppState>, headers: HeaderMap) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    match svc.list_sessions().await {
        Ok(list) => json_with_correlation(StatusCode::OK, &list, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_get_session_detail(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let session_id = match SessionId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc.get_session_detail(session_id).await {
        Ok(detail) => json_with_correlation(StatusCode::OK, &detail, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_send_message(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
    payload: Result<Json<SendMessageRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let session_id = match SessionId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc.send_message(session_id, req, cid.clone()).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_cancel_run(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let session_id = match SessionId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc.cancel_session_run(session_id, &cid).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_create_artifact(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
    payload: Result<Json<CreateArtifactRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let session_id = match SessionId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc.create_artifact(session_id, req, &cid).await {
        Ok(art) => json_with_correlation(StatusCode::CREATED, &art, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_list_artifacts(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let session_id = match SessionId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc.list_artifacts(session_id).await {
        Ok(list) => json_with_correlation(StatusCode::OK, &list, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_create_policy_rule(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    payload: Result<Json<CreatePolicyRuleRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc.create_policy_rule(req, &cid).await {
        Ok(rule) => json_with_correlation(StatusCode::CREATED, &rule, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_list_policy_rules(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    match svc.list_policy_rules().await {
        Ok(list) => json_with_correlation(StatusCode::OK, &list, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_commit_memory_note(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    payload: Result<Json<CommitMemoryNoteRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc.commit_memory_note(req, &cid).await {
        Ok(resp) => {
            let status = if resp.delivered_immediately {
                StatusCode::CREATED
            } else {
                StatusCode::ACCEPTED
            };
            json_with_correlation(status, &resp, &cid)
        }
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_list_outbox(State(state): State<HeadAppState>, headers: HeaderMap) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    match svc.list_memory_outbox(&cid).await {
        Ok(list) => json_with_correlation(StatusCode::OK, &list, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_flush_outbox(State(state): State<HeadAppState>, headers: HeaderMap) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    match svc.flush_memory_outbox(&cid).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_search_memory(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    payload: Result<Json<HeadMemorySearchRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc.search_memory(req, &cid).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_diagnostics(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Query(query): Query<DiagnosticsQuery>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let limit = query.limit.unwrap_or(100);
    match svc.get_diagnostics(limit).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_not_found(headers: HeaderMap) -> Response {
    let cid = extract_correlation_id_lossy(&headers);
    let err = DennettError::NotFound {
        resource: String::from("http.path"),
        id: String::from("unknown"),
    };
    json_api_error(StatusCode::NOT_FOUND, err, &cid)
}

fn get_service(
    state: &HeadAppState,
    headers: &HeaderMap,
) -> Result<(CorrelationId, HeadService), Box<Response>> {
    let cid = match extract_correlation_id(headers) {
        Ok(id) => id,
        Err(err) => {
            return Err(Box::new(json_api_error(
                StatusCode::BAD_REQUEST,
                err,
                &CorrelationId::generate(),
            )));
        }
    };
    let svc = state
        .service
        .clone()
        .ok_or_else(|| Box::new(domain_error_response(unconfigured_control_pg_error(), &cid)))?;
    Ok((cid, svc))
}

fn unconfigured_control_pg_error() -> DennettError {
    DennettError::DatabaseUnavailable {
        detail: String::from("control PostgreSQL connection URL is not configured"),
    }
}

fn extract_correlation_id(headers: &HeaderMap) -> Result<CorrelationId, DennettError> {
    let Some(raw) = headers.get(&HEADER_CORRELATION_ID) else {
        return Ok(CorrelationId::generate());
    };
    let raw_str = raw.to_str().map_err(|_| DennettError::InvalidInput {
        field: String::from("x-correlation-id"),
        reason: String::from("header must be valid ASCII"),
    })?;
    CorrelationId::try_new(raw_str.trim())
}

fn extract_correlation_id_lossy(headers: &HeaderMap) -> CorrelationId {
    extract_correlation_id(headers).unwrap_or_else(|_| CorrelationId::generate())
}

fn json_rejection_response(rej: &JsonRejection, correlation_id: &CorrelationId) -> Response {
    let status = rej.status();
    let err = DennettError::InvalidInput {
        field: String::from("body"),
        reason: redact_secrets(&rej.body_text(), &[]),
    };
    json_api_error(status, err, correlation_id)
}

fn domain_error_response(err: DennettError, correlation_id: &CorrelationId) -> Response {
    let status = match &err {
        DennettError::InvalidInput { .. } => StatusCode::BAD_REQUEST,
        DennettError::Unauthorized { .. } | DennettError::AuthRequired { .. } => {
            StatusCode::UNAUTHORIZED
        }
        DennettError::ForbiddenScope { .. }
        | DennettError::AccessDenied { .. }
        | DennettError::SafetyGuardRejected { .. } => StatusCode::FORBIDDEN,
        DennettError::NotFound { .. } => StatusCode::NOT_FOUND,
        DennettError::Conflict { .. } | DennettError::IdempotencyMismatch { .. } => {
            StatusCode::CONFLICT
        }
        DennettError::DatabaseUnavailable { .. } => StatusCode::SERVICE_UNAVAILABLE,
        DennettError::Timeout { .. } => StatusCode::GATEWAY_TIMEOUT,
        DennettError::RuntimeProtocol { .. } | DennettError::RuntimeVersionUnsupported { .. } => {
            StatusCode::BAD_GATEWAY
        }
        DennettError::Cancelled { .. } => StatusCode::REQUEST_TIMEOUT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    json_api_error(status, err, correlation_id)
}

fn json_api_error(
    status: StatusCode,
    err: DennettError,
    correlation_id: &CorrelationId,
) -> Response {
    let envelope = ApiErrorResponse::new(err, correlation_id.clone());
    json_with_correlation(status, &envelope, correlation_id)
}

fn json_with_correlation<T: Serialize>(
    status: StatusCode,
    body: &T,
    correlation_id: &CorrelationId,
) -> Response {
    let mut resp = (status, Json(body)).into_response();
    if let Ok(val) = HeaderValue::from_str(correlation_id.as_str()) {
        resp.headers_mut()
            .insert(HEADER_CORRELATION_ID.clone(), val);
    }
    resp
}

fn is_loopback_host(raw: &str) -> bool {
    let host = raw.trim();
    if host.is_empty() {
        return false;
    }
    let without_port = if let Some(rest) = host.strip_prefix('[') {
        rest.split(']').next().unwrap_or_default()
    } else {
        host.split(':').next().unwrap_or_default()
    };
    without_port.eq_ignore_ascii_case("localhost")
        || without_port == "127.0.0.1"
        || without_port == "::1"
}
