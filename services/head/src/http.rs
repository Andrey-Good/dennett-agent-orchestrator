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
use axum::http::header::{HOST, ORIGIN};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use dennett_contracts::redact::redact_secrets;
use dennett_contracts::{
    AgentId, ApiErrorResponse, ArtifactId, ArtifactLifecycleRequest, CapabilityGrantId,
    CommitMemoryNoteRequest, CorrelationId, CreateAgentRequest, CreateArtifactRequest,
    CreatePolicyRuleRequest, CreateProjectRequest, CreateSessionRequest, CreateTaskRequest,
    DennettError, DispatchEffectRequest, EffectClaimId, HeadHealth, HeadMemorySearchRequest,
    InboxCardId, MemoryStatus, PermissionRequestId, ProfileMode, ProjectId, RebindProjectRequest,
    ReconcileEffectRequest, ReplyTaskRequest, ResolveInboxCardRequest, ResolvePermissionRequest,
    RevokeGrantRequest, SchedulerTickRequest, SendAgentMessageRequest, SendMessageRequest,
    ServiceKind, SessionId, TaskId, TaskSignalRequest, TimestampMs, UpdateAgentConfigRequest,
    UpdateProjectRequest, UpdateSessionRequest, UpdateTaskRequest,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{Semaphore, watch};
use tokio::time;

use crate::service::{HEAD_STAGE_LABEL, HeadService, d2_head_capabilities};

/// Maximum incoming HTTP request body size (1 MiB).
pub const MAX_HTTP_BODY_BYTES: usize = 1024 * 1024;
/// Maximum concurrent in-flight HTTP requests accepted by `dennett-head`.
pub const MAX_CONCURRENT_REQUESTS: usize = 64;
/// Upper bound on end-to-end HTTP request wall time (accommodates live runtime turns).
pub const HTTP_REQUEST_TIMEOUT: Duration = Duration::from_secs(180);

static HEADER_CORRELATION_ID: HeaderName = HeaderName::from_static("x-correlation-id");
static HEADER_SHUTDOWN: HeaderName = HeaderName::from_static("x-dennett-shutdown");
static HEADER_SEC_FETCH_SITE: HeaderName = HeaderName::from_static("sec-fetch-site");

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
    /// Whether operator explicitly opted into `--allow-non-loopback` binding.
    pub allow_non_loopback: bool,
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

#[derive(Debug, Deserialize)]
struct RuntimeCatalogQuery {
    #[serde(default)]
    refresh: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ProjectFilesQuery {
    #[serde(default)]
    subpath: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListProjectsQuery {
    #[serde(default)]
    include_archived: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct DeleteProjectQuery {
    #[serde(default)]
    confirm: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct SnapshotQuery {
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    include_archived: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct EventsQuery {
    #[serde(default)]
    after_cursor: Option<i64>,
    #[serde(default)]
    epoch: Option<i64>,
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct ListTasksQuery {
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListInboxQuery {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    project_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListPermissionsQuery {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListGrantsQuery {
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    include_revoked: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ListAgentMessagesQuery {
    #[serde(default)]
    undelivered_only: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ListEffectsQuery {
    #[serde(default)]
    task_id: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    status: Option<String>,
}

/// Builds the complete `axum` HTTP [`Router`] for `dennett-head`.
#[expect(
    clippy::too_many_lines,
    reason = "central HTTP route table registers all versioned /v1/* control-plane endpoints in one transparent builder"
)]
pub fn build_head_router(state: HeadAppState) -> Router {
    let mut router = Router::new()
        .route("/healthz", get(handle_healthz))
        .route("/v1/runtime/catalog", get(handle_get_runtime_catalog))
        .route("/v1/state/snapshot", get(handle_get_state_snapshot))
        .route("/v1/events", get(handle_get_events))
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
            "/v1/agents/{agent_id}/messages",
            post(handle_send_agent_message).get(handle_list_agent_messages),
        )
        .route(
            "/v1/projects",
            post(handle_create_project).get(handle_list_projects),
        )
        .route(
            "/v1/projects/{project_id}",
            get(handle_get_project)
                .patch(handle_update_project)
                .delete(handle_delete_project),
        )
        .route(
            "/v1/projects/{project_id}/files",
            get(handle_list_project_files),
        )
        .route(
            "/v1/projects/{project_id}/rebind",
            post(handle_rebind_project),
        )
        .route(
            "/v1/projects/{project_id}/detach-workspace",
            post(handle_detach_project_workspace),
        )
        .route(
            "/v1/projects/{project_id}/archive",
            post(handle_archive_project),
        )
        .route(
            "/v1/projects/{project_id}/unarchive",
            post(handle_unarchive_project),
        )
        .route(
            "/v1/projects/{project_id}/remove",
            post(handle_remove_project),
        )
        .route(
            "/v1/sessions",
            post(handle_create_session).get(handle_list_sessions),
        )
        .route(
            "/v1/sessions/{session_id}",
            get(handle_get_session_detail).patch(handle_update_session),
        )
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
            "/v1/sessions/{session_id}/artifacts/{artifact_key}/revisions",
            get(handle_list_artifact_revisions),
        )
        .route(
            "/v1/sessions/{session_id}/artifacts/{artifact_id}/lifecycle",
            post(handle_apply_artifact_lifecycle),
        )
        .route("/v1/tasks", post(handle_create_task).get(handle_list_tasks))
        .route(
            "/v1/tasks/{task_id}",
            get(handle_get_task_detail).patch(handle_update_task),
        )
        .route("/v1/tasks/{task_id}/signals", post(handle_send_task_signal))
        .route("/v1/tasks/{task_id}/reply", post(handle_reply_to_task))
        .route("/v1/tasks/{task_id}/cancel", post(handle_cancel_task))
        .route("/v1/inbox", get(handle_list_inbox_cards))
        .route(
            "/v1/inbox/{card_id}/resolve",
            post(handle_resolve_inbox_card),
        )
        .route(
            "/v1/permissions/requests",
            get(handle_list_permission_requests),
        )
        .route(
            "/v1/permissions/requests/{request_id}/resolve",
            post(handle_resolve_permission_request),
        )
        .route(
            "/v1/approvals/{request_id}/resolve",
            post(handle_resolve_permission_request),
        )
        .route("/v1/grants", get(handle_list_capability_grants))
        .route(
            "/v1/grants/{grant_id}/revoke",
            post(handle_revoke_capability_grant),
        )
        .route("/v1/effects/dispatch", post(handle_dispatch_effect))
        .route("/v1/effects", get(handle_list_effects))
        .route(
            "/v1/effects/{effect_id}/reconcile",
            post(handle_reconcile_effect),
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
        router = router
            .route("/shutdown", post(handle_shutdown))
            .route("/v1/tasks/scheduler/tick", post(handle_scheduler_tick));
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

    if !state.allow_non_loopback
        && let Err(err) = validate_loopback_request_headers(request.headers())
    {
        return json_api_error(StatusCode::FORBIDDEN, err, &correlation_id);
    }

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

fn validate_loopback_request_headers(headers: &HeaderMap) -> Result<(), DennettError> {
    let host_ok = headers
        .get(HOST)
        .and_then(|v| v.to_str().ok())
        .is_some_and(is_loopback_host);
    if !host_ok {
        return Err(DennettError::AccessDenied {
            scope: String::from("http.host"),
            reason: String::from(
                "missing or non-loopback Host header rejected by loopback DNS-rebinding guard",
            ),
        });
    }

    if let Some(fetch_site) = headers.get(&HEADER_SEC_FETCH_SITE) {
        let is_cross_site = fetch_site
            .to_str()
            .map_or(true, |v| v.trim().eq_ignore_ascii_case("cross-site"));
        if is_cross_site {
            return Err(DennettError::AccessDenied {
                scope: String::from("http.sec_fetch_site"),
                reason: String::from("cross-site browser request rejected"),
            });
        }
    }

    if let Some(origin) = headers.get(ORIGIN) {
        let origin_ok = origin.to_str().is_ok_and(is_allowed_loopback_origin);
        if !origin_ok {
            return Err(DennettError::AccessDenied {
                scope: String::from("http.origin"),
                reason: String::from("non-loopback Origin header rejected by loopback CSRF guard"),
            });
        }
    }

    Ok(())
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
                    capabilities: d2_head_capabilities(),
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
        .is_some_and(is_loopback_host);

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

async fn handle_list_projects(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Query(query): Query<ListProjectsQuery>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let include_archived = query.include_archived.unwrap_or(false);
    match svc.list_projects_with_options(include_archived).await {
        Ok(list) => json_with_correlation(StatusCode::OK, &list, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_get_runtime_catalog(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Query(query): Query<RuntimeCatalogQuery>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let refresh = query.refresh.unwrap_or(false);
    match svc.get_runtime_catalog(refresh).await {
        Ok(catalog) => json_with_correlation(StatusCode::OK, &catalog, &cid),
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

async fn handle_update_project(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
    payload: Result<Json<UpdateProjectRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let project_id = match ProjectId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc.update_project(project_id, req, &cid).await {
        Ok(project) => json_with_correlation(StatusCode::OK, &project, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_list_project_files(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
    Query(query): Query<ProjectFilesQuery>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let project_id = match ProjectId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc.list_project_files(project_id, query.subpath).await {
        Ok(files) => json_with_correlation(StatusCode::OK, &files, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_update_session(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
    payload: Result<Json<UpdateSessionRequest>, JsonRejection>,
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
    match svc.update_session(session_id, req, &cid).await {
        Ok(session) => json_with_correlation(StatusCode::OK, &session, &cid),
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

fn parse_optional_id<T: FromStr<Err = DennettError>>(
    raw: Option<&str>,
) -> Result<Option<T>, DennettError> {
    raw.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(T::from_str)
        .transpose()
}

async fn handle_get_state_snapshot(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Query(query): Query<SnapshotQuery>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let project_id = match parse_optional_id::<ProjectId>(query.project_id.as_deref()) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let include_archived = query.include_archived.unwrap_or(false);
    match svc.get_state_snapshot(project_id, include_archived).await {
        Ok(snap) => json_with_correlation(StatusCode::OK, &snap, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_get_events(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Query(query): Query<EventsQuery>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let project_id = match parse_optional_id::<ProjectId>(query.project_id.as_deref()) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let after_cursor = query.after_cursor.unwrap_or(0);
    match svc
        .get_events(after_cursor, query.epoch, project_id, query.limit)
        .await
    {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_create_task(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    payload: Result<Json<CreateTaskRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc.create_task(req, &cid).await {
        Ok(task) => json_with_correlation(StatusCode::CREATED, &task, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_list_tasks(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Query(query): Query<ListTasksQuery>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let project_id = match parse_optional_id::<ProjectId>(query.project_id.as_deref()) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc.list_tasks(project_id, query.status.as_deref()).await {
        Ok(list) => json_with_correlation(StatusCode::OK, &list, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_get_task_detail(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let task_id = match TaskId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc.get_task_detail(task_id).await {
        Ok(detail) => json_with_correlation(StatusCode::OK, &detail, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_update_task(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
    payload: Result<Json<UpdateTaskRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let task_id = match TaskId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc.update_task(task_id, req, &cid).await {
        Ok(task) => json_with_correlation(StatusCode::OK, &task, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_send_task_signal(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
    payload: Result<Json<TaskSignalRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let task_id = match TaskId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc.send_task_signal(task_id, req, &cid).await {
        Ok(resp) => {
            let status = if resp.deduplicated {
                StatusCode::OK
            } else {
                StatusCode::CREATED
            };
            json_with_correlation(status, &resp, &cid)
        }
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_reply_to_task(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
    payload: Result<Json<ReplyTaskRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let task_id = match TaskId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc.reply_to_task(task_id, req, &cid).await {
        Ok(task) => json_with_correlation(StatusCode::OK, &task, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_cancel_task(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let task_id = match TaskId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc.cancel_task(task_id, &cid).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_scheduler_tick(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    if state.profile_mode != ProfileMode::TestIsolated {
        let err = DennettError::NotFound {
            resource: String::from("http.path"),
            id: String::from("/v1/tasks/scheduler/tick"),
        };
        return json_api_error(StatusCode::NOT_FOUND, err, &cid);
    }
    let req: SchedulerTickRequest = if body.is_empty() {
        SchedulerTickRequest::default()
    } else {
        match serde_json::from_slice(&body) {
            Ok(parsed) => parsed,
            Err(err) => {
                return domain_error_response(
                    DennettError::InvalidInput {
                        field: String::from("body"),
                        reason: redact_secrets(&err.to_string(), &[]),
                    },
                    &cid,
                );
            }
        }
    };
    let now = req
        .now_ms
        .map_or_else(TimestampMs::now, TimestampMs::from_millis);
    match svc.tick_scheduler_at(now, &cid).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_list_inbox_cards(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Query(query): Query<ListInboxQuery>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let project_id = match parse_optional_id::<ProjectId>(query.project_id.as_deref()) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc
        .list_inbox_cards(query.status.as_deref(), project_id)
        .await
    {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_resolve_inbox_card(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
    payload: Result<Json<ResolveInboxCardRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let card_id = match InboxCardId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc.resolve_inbox_card(card_id, req, &cid).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_list_permission_requests(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Query(query): Query<ListPermissionsQuery>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let session_id = match parse_optional_id::<SessionId>(query.session_id.as_deref()) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc
        .list_permission_requests(query.status.as_deref(), session_id)
        .await
    {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_resolve_permission_request(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
    payload: Result<Json<ResolvePermissionRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let request_id = match PermissionRequestId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc.resolve_permission_request(request_id, req, &cid).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_list_capability_grants(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Query(query): Query<ListGrantsQuery>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let project_id = match parse_optional_id::<ProjectId>(query.project_id.as_deref()) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let include_revoked = query.include_revoked.unwrap_or(false);
    match svc
        .list_capability_grants(project_id, include_revoked)
        .await
    {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_revoke_capability_grant(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
    body: Bytes,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let grant_id = match CapabilityGrantId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let req: RevokeGrantRequest = if body.is_empty() {
        RevokeGrantRequest::default()
    } else {
        match serde_json::from_slice(&body) {
            Ok(parsed) => parsed,
            Err(err) => {
                return domain_error_response(
                    DennettError::InvalidInput {
                        field: String::from("body"),
                        reason: redact_secrets(&err.to_string(), &[]),
                    },
                    &cid,
                );
            }
        }
    };
    match svc.revoke_capability_grant(grant_id, req, &cid).await {
        Ok(grant) => json_with_correlation(StatusCode::OK, &grant, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_send_agent_message(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
    payload: Result<Json<SendAgentMessageRequest>, JsonRejection>,
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
    match svc.send_agent_message(agent_id, req, &cid).await {
        Ok(resp) => {
            let status = if resp.deduplicated {
                StatusCode::OK
            } else {
                StatusCode::CREATED
            };
            json_with_correlation(status, &resp, &cid)
        }
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_list_agent_messages(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
    Query(query): Query<ListAgentMessagesQuery>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let agent_id = match AgentId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let undelivered_only = query.undelivered_only.unwrap_or(false);
    match svc.list_agent_messages(agent_id, undelivered_only).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_dispatch_effect(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    payload: Result<Json<DispatchEffectRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc.dispatch_external_effect(req, &cid).await {
        Ok(resp) => {
            let status = if resp.deduplicated {
                StatusCode::OK
            } else {
                StatusCode::CREATED
            };
            json_with_correlation(status, &resp, &cid)
        }
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_list_effects(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Query(query): Query<ListEffectsQuery>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let task_id = match parse_optional_id::<TaskId>(query.task_id.as_deref()) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let session_id = match parse_optional_id::<SessionId>(query.session_id.as_deref()) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc
        .list_effects(task_id, session_id, query.status.as_deref())
        .await
    {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_reconcile_effect(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
    body: Bytes,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let effect_id = match EffectClaimId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let req: ReconcileEffectRequest = if body.is_empty() {
        ReconcileEffectRequest::default()
    } else {
        match serde_json::from_slice(&body) {
            Ok(parsed) => parsed,
            Err(err) => {
                return domain_error_response(
                    DennettError::InvalidInput {
                        field: String::from("body"),
                        reason: redact_secrets(&err.to_string(), &[]),
                    },
                    &cid,
                );
            }
        }
    };
    match svc.reconcile_external_effect(effect_id, req, &cid).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_rebind_project(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
    payload: Result<Json<RebindProjectRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let project_id = match ProjectId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc.rebind_project(project_id, req, &cid).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_detach_project_workspace(
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
    match svc.detach_project_workspace(project_id, &cid).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_archive_project(
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
    match svc.archive_project(project_id, &cid).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_unarchive_project(
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
    match svc.unarchive_project(project_id, &cid).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_remove_project(
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
    match svc.remove_project(project_id, &cid).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_delete_project(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
    Query(query): Query<DeleteProjectQuery>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let project_id = match ProjectId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let confirm = query.confirm.unwrap_or(false);
    match svc.delete_project(project_id, confirm, &cid).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_list_artifact_revisions(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path((raw_session_id, artifact_key)): Path<(String, String)>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let session_id = match SessionId::from_str(&raw_session_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc.list_artifact_revisions(session_id, &artifact_key).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_apply_artifact_lifecycle(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path((raw_session_id, raw_artifact_id)): Path<(String, String)>,
    payload: Result<Json<ArtifactLifecycleRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let session_id = match SessionId::from_str(&raw_session_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let artifact_id = match ArtifactId::from_str(&raw_artifact_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc
        .apply_artifact_lifecycle(session_id, artifact_id, req, &cid)
        .await
    {
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

fn is_valid_port(port: &str) -> bool {
    !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) && port.parse::<u16>().is_ok()
}

fn is_loopback_host(raw: &str) -> bool {
    let host = raw.trim();
    if host.is_empty() {
        return false;
    }
    if let Some(rest) = host.strip_prefix('[') {
        let Some((addr, after)) = rest.split_once(']') else {
            return false;
        };
        if addr != "::1" {
            return false;
        }
        return after.is_empty() || after.strip_prefix(':').is_some_and(is_valid_port);
    }
    let (name, port_opt) = match host.split_once(':') {
        Some((n, p)) => (n, Some(p)),
        None => (host, None),
    };
    let name_ok = name.eq_ignore_ascii_case("localhost") || name == "127.0.0.1";
    name_ok && port_opt.is_none_or(is_valid_port)
}

fn is_allowed_loopback_origin(raw: &str) -> bool {
    let origin = raw.trim();
    if origin.eq_ignore_ascii_case("tauri://localhost")
        || origin.eq_ignore_ascii_case("http://tauri.localhost")
        || origin.eq_ignore_ascii_case("https://tauri.localhost")
    {
        return true;
    }
    if origin.len() <= "http://".len() || !origin[.."http://".len()].eq_ignore_ascii_case("http://")
    {
        return false;
    }
    let authority = &origin["http://".len()..];
    if authority
        .bytes()
        .any(|b| b.is_ascii_whitespace() || matches!(b, b'/' | b'?' | b'#' | b'@' | b'\\'))
    {
        return false;
    }
    is_loopback_host(authority)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_host_and_origin_guards_accept_local_and_tauri_and_reject_rebinding() {
        for good_host in [
            "127.0.0.1",
            "127.0.0.1:41771",
            "localhost",
            "LOCALHOST:1420",
            "[::1]",
            "[::1]:41771",
        ] {
            assert!(
                is_loopback_host(good_host),
                "expected loopback host `{good_host}` to be accepted"
            );
        }

        for bad_host in [
            "",
            "evil.rebind.attacker:41771",
            "127.0.0.1.evil.com",
            "localhost.evil.com:1420",
            "127.0.0.1:notaport",
            "127.0.0.1:70000",
            "127.0.0.1:80:80",
            "[::1]evil.com",
            "[::1]:80evil",
            "[::2]:41771",
            "0.0.0.0:41771",
        ] {
            assert!(
                !is_loopback_host(bad_host),
                "expected non-loopback/malformed host `{bad_host}` to be rejected"
            );
        }

        for good_origin in [
            "http://127.0.0.1:1420",
            "http://127.0.0.1",
            "http://localhost:1420",
            "http://[::1]:1420",
            "tauri://localhost",
            "http://tauri.localhost",
            "https://tauri.localhost",
        ] {
            assert!(
                is_allowed_loopback_origin(good_origin),
                "expected origin `{good_origin}` to be accepted"
            );
        }

        for bad_origin in [
            "",
            "null",
            "https://evil.example",
            "http://evil.example:1420",
            "https://127.0.0.1:1420",
            "http://127.0.0.1.evil.com",
            "http://127.0.0.1:1420/path",
            "http://localhost@evil.example",
            "http://tauri.localhost.evil.example",
        ] {
            assert!(
                !is_allowed_loopback_origin(bad_origin),
                "expected origin `{bad_origin}` to be rejected"
            );
        }

        // Missing Host fails closed.
        let empty = HeaderMap::new();
        assert!(matches!(
            validate_loopback_request_headers(&empty),
            Err(DennettError::AccessDenied { .. })
        ));

        // Cross-site Sec-Fetch-Site fails closed even with loopback Host.
        let mut cross_site = HeaderMap::new();
        cross_site.insert(HOST, HeaderValue::from_static("127.0.0.1:41771"));
        cross_site.insert(
            HEADER_SEC_FETCH_SITE.clone(),
            HeaderValue::from_static("cross-site"),
        );
        assert!(matches!(
            validate_loopback_request_headers(&cross_site),
            Err(DennettError::AccessDenied { .. })
        ));

        // Same-origin Vite dev / Tauri request passes.
        let mut ok_headers = HeaderMap::new();
        ok_headers.insert(HOST, HeaderValue::from_static("127.0.0.1:1420"));
        ok_headers.insert(ORIGIN, HeaderValue::from_static("http://127.0.0.1:1420"));
        ok_headers.insert(
            HEADER_SEC_FETCH_SITE.clone(),
            HeaderValue::from_static("same-origin"),
        );
        assert!(validate_loopback_request_headers(&ok_headers).is_ok());
    }
}
