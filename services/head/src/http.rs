//! `axum` HTTP server router and handlers for `dennett-head`.
//!
//! Enforces loopback-by-default binding, bounded request bodies, concurrency limits,
//! correlation ID propagation, secret-redacted error envelopes, and strict test-only
//! `POST /shutdown` guards.

use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::rejection::JsonRejection;
use axum::extract::{ConnectInfo, DefaultBodyLimit, Path, Query, Request, State};
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE, HOST, ORIGIN};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post, put};
use axum::{Json, Router};
use dennett_contracts::redact::redact_secrets;
use dennett_contracts::{
    ActivateConnectionRequest, AgentId, ApiErrorResponse, ArtifactId, ArtifactLifecycleRequest,
    CapabilityGrantId, CapabilityId, CapabilityKind, CapabilityOrigin, CapabilityScopeKind,
    ClaimId, CommitMemoryNoteRequest, ConflictId, ConsolidationJobId, CorrectNoteRequest,
    CorrelationId, CreateAgentRequest, CreateArtifactRequest, CreateCapabilityRequest,
    CreateClaimRequest, CreateConsolidationJobRequest, CreateDocumentRequest,
    CreatePolicyRuleRequest, CreateProjectRequest, CreateSessionRequest, CreateTaskRequest,
    DennettError, DiscoverCapabilitiesRequest, DispatchEffectRequest, DocumentId, EffectClaimId,
    EnqueueFollowupRequest, ExportRequest, FollowupId, ForgetRequest, ForkSessionRequest,
    HeadHealth, HeadMemorySearchRequest, ImportRequest, InboxCardId, MemoryNoteId, MemoryScope,
    MemoryStatus, PermissionRequestId, PolicyRuleId, ProfileMode, ProjectId, ProviderConnectionId,
    RebindProjectRequest, ReconcileEffectRequest, ReplyTaskRequest, ResolveConflictRequest,
    ResolveInboxCardRequest, ResolvePermissionRequest, RevokeGrantRequest, SchedulerTickRequest,
    SendAgentMessageRequest, SendMessageRequest, ServiceKind, SessionId, SessionKind,
    SetProjectCapabilityBindingRequest, SetSecretRequest, SteerSessionRequest,
    SupersedeClaimRequest, TaskId, TaskSignalRequest, TimestampMs, TriggerIndexRebuildRequest,
    UpdateAgentConfigRequest, UpdateCapabilityRequest, UpdateDocumentRequest,
    UpdateFollowupRequest, UpdateProjectRequest, UpdateSessionRequest, UpdateTaskRequest,
    UpdateUserPreferencesRequest, UpsertProviderConnectionRequest,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{Semaphore, watch};
use tokio::time;
use zeroize::Zeroizing;

use crate::memory_auth::ControlTokenClaims;
use crate::policy::standalone_system_project_id;
use crate::service::{HEAD_STAGE_LABEL, HeadService, d3_head_capabilities};

/// Maximum incoming HTTP request body size (1 MiB).
pub const MAX_HTTP_BODY_BYTES: usize = 1024 * 1024;
/// Maximum incoming HTTP request body size for `/v1/memory/import` packages (16 MiB + 64 KiB JSON envelope).
pub const MAX_MEMORY_IMPORT_BODY_BYTES: usize = 16 * 1024 * 1024 + 64 * 1024;
/// Maximum concurrent in-flight HTTP requests accepted by `dennett-head`.
pub const MAX_CONCURRENT_REQUESTS: usize = 64;
/// Upper bound on end-to-end HTTP request wall time (accommodates live runtime turns).
pub const HTTP_REQUEST_TIMEOUT: Duration = Duration::from_secs(180);

static HEADER_CORRELATION_ID: HeaderName = HeaderName::from_static("x-correlation-id");
static HEADER_SHUTDOWN: HeaderName = HeaderName::from_static("x-dennett-shutdown");
static HEADER_SEC_FETCH_SITE: HeaderName = HeaderName::from_static("sec-fetch-site");
static HEADER_CONTROL_TOKEN: HeaderName = HeaderName::from_static("x-dennett-control-token");
static HEADER_ARTIFACT_SHA256: HeaderName = HeaderName::from_static("x-dennett-artifact-sha256");
static HEADER_CONTENT_TYPE_OPTIONS: HeaderName = HeaderName::from_static("x-content-type-options");

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
    /// Mandatory owner Bearer token authenticating all non-`/healthz` owner HTTP routes (`A-SEC-1`).
    pub owner_token: Zeroizing<String>,
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
struct ProjectFileContentQuery {
    path: String,
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

#[derive(Debug, Deserialize)]
struct ListCapabilitiesQuery {
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    scope_kind: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MemoryScopeQuery {
    #[serde(default)]
    scope: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MemoryTombstonesQuery {
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    after_seq: Option<i64>,
    #[serde(default)]
    limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct ListMemoryClaimsQuery {
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    topic_key: Option<String>,
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
            "/v1/projects/{project_id}/file",
            get(handle_get_project_file),
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
            get(handle_get_session_detail)
                .patch(handle_update_session)
                .delete(handle_delete_session),
        )
        .route(
            "/v1/sessions/{session_id}/fork",
            post(handle_fork_session),
        )
        .route(
            "/v1/sessions/{session_id}/messages",
            post(handle_send_message),
        )
        .route("/v1/sessions/{session_id}/cancel", post(handle_cancel_run))
        .route(
            "/v1/sessions/{session_id}/live",
            get(handle_get_session_live),
        )
        .route(
            "/v1/sessions/{session_id}/followups",
            post(handle_enqueue_followup).delete(handle_clear_followups),
        )
        .route(
            "/v1/sessions/{session_id}/followups/resume",
            post(handle_resume_followups),
        )
        .route(
            "/v1/sessions/{session_id}/followups/{followup_id}",
            patch(handle_update_followup).delete(handle_delete_followup),
        )
        .route(
            "/v1/sessions/{session_id}/steer",
            post(handle_steer_session),
        )
        .route(
            "/v1/sessions/{session_id}/artifacts",
            post(handle_create_artifact).get(handle_list_artifacts),
        )
        .route(
            "/v1/sessions/{session_id}/artifacts/{artifact_key}/revisions",
            get(handle_list_artifact_revisions),
        )
        .route(
            "/v1/sessions/{session_id}/artifacts/{artifact_id}/content",
            get(handle_get_artifact_content),
        )
        .route(
            "/v1/sessions/{session_id}/artifacts/{artifact_id}/lifecycle",
            post(handle_apply_artifact_lifecycle),
        )
        .route("/v1/tasks", post(handle_create_task).get(handle_list_tasks))
        .route(
            "/v1/tasks/{task_id}",
            get(handle_get_task_detail)
                .patch(handle_update_task)
                .delete(handle_delete_task),
        )
        .route("/v1/tasks/{task_id}/run-now", post(handle_run_task_now))
        .route("/v1/tasks/{task_id}/pause", post(handle_pause_task))
        .route("/v1/tasks/{task_id}/resume", post(handle_resume_task))
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
        .route("/v1/policies/{rule_id}", delete(handle_delete_policy_rule))
        .route(
            "/v1/preferences",
            get(handle_get_preferences).patch(handle_update_preferences),
        )
        .route("/v1/runs/emergency-stop", post(handle_emergency_stop))
        .route("/v1/memory/commits", post(handle_commit_memory_note))
        .route("/v1/memory/outbox", get(handle_list_outbox))
        .route("/v1/memory/outbox/flush", post(handle_flush_outbox))
        .route("/v1/memory/search", post(handle_search_memory))
        // Phase D3: Write-only SecretBroker, Provider Connections, A08 Capabilities, Owner Memory Proxy.
        .route("/v1/secrets", get(handle_list_secrets))
        .route(
            "/v1/secrets/{slot_key}",
            put(handle_set_secret).delete(handle_delete_secret),
        )
        .route(
            "/v1/runtime/connections",
            get(handle_list_provider_connections).post(handle_create_provider_connection),
        )
        .route(
            "/v1/runtime/connections/{connection_id}",
            patch(handle_update_provider_connection).delete(handle_delete_provider_connection),
        )
        .route(
            "/v1/runtime/connections/{connection_id}/probe",
            post(handle_probe_provider_connection),
        )
        .route(
            "/v1/runtime/connections/{connection_id}/activate",
            post(handle_activate_provider_connection),
        )
        .route(
            "/v1/capabilities",
            get(handle_list_capabilities).post(handle_create_capability),
        )
        .route(
            "/v1/capabilities/discover",
            post(handle_discover_capabilities),
        )
        .route(
            "/v1/capabilities/session-skills",
            get(handle_list_session_skills),
        )
        .route(
            "/v1/capabilities/session-skills/{slug}",
            get(handle_read_session_skill),
        )
        .route(
            "/v1/capabilities/{capability_id}",
            patch(handle_update_capability).delete(handle_delete_capability),
        )
        .route(
            "/v1/capabilities/{capability_id}/rollback",
            post(handle_rollback_capability),
        )
        .route(
            "/v1/projects/{project_id}/capabilities/{capability_id}",
            put(handle_set_project_capability_binding),
        )
        .route("/v1/memory/overview", get(handle_memory_overview))
        .route(
            "/v1/memory/notes/{note_id}/correct",
            post(handle_correct_memory_note),
        )
        .route(
            "/v1/memory/notes/{note_id}/history",
            get(handle_get_memory_note_history),
        )
        .route(
            "/v1/memory/claims",
            get(handle_list_memory_claims).post(handle_create_memory_claim),
        )
        .route(
            "/v1/memory/claims/{claim_id}/supersede",
            post(handle_supersede_memory_claim),
        )
        .route("/v1/memory/conflicts", get(handle_list_memory_conflicts))
        .route(
            "/v1/memory/conflicts/{conflict_id}/resolve",
            post(handle_resolve_memory_conflict),
        )
        .route(
            "/v1/memory/documents",
            get(handle_list_memory_documents).post(handle_create_memory_document),
        )
        .route(
            "/v1/memory/documents/{document_id}",
            get(handle_get_memory_document).put(handle_update_memory_document),
        )
        .route(
            "/v1/memory/documents/{document_id}/versions",
            get(handle_get_memory_document_versions),
        )
        .route("/v1/memory/forget", post(handle_forget_memory))
        .route(
            "/v1/memory/forget/tombstones",
            get(handle_list_memory_tombstones),
        )
        .route("/v1/memory/export", post(handle_export_memory))
        .route(
            "/v1/memory/import",
            post(handle_import_memory).layer(DefaultBodyLimit::max(MAX_MEMORY_IMPORT_BODY_BYTES)),
        )
        .route(
            "/v1/memory/index/status",
            get(handle_get_memory_index_status),
        )
        .route(
            "/v1/memory/index/rebuild",
            post(handle_trigger_memory_index_rebuild),
        )
        .route(
            "/v1/memory/consolidation/jobs",
            get(handle_list_memory_consolidation_jobs).post(handle_create_memory_consolidation_job),
        )
        .route(
            "/v1/memory/consolidation/jobs/{job_id}/pause",
            post(handle_pause_memory_consolidation_job),
        )
        .route(
            "/v1/memory/consolidation/jobs/{job_id}/resume",
            post(handle_resume_memory_consolidation_job),
        )
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

    let path = request.uri().path();
    let is_healthz = request.method() == Method::GET && path == "/healthz";

    // A-SEC-1: Mandatory Owner Bearer authentication on ALL routes except `GET /healthz`, with
    // scoped `X-Dennett-Control-Token` (`dct1.`) permitted ONLY on the narrow control-MCP route surface.
    if !is_healthz {
        if request.headers().contains_key(&HEADER_CONTROL_TOKEN) {
            if !is_control_token_allowed_route(request.method(), path) {
                let err = DennettError::AccessDenied {
                    scope: String::from("http.control_token"),
                    reason: format!(
                        "control MCP token is not permitted to invoke {} {}",
                        request.method(),
                        path
                    ),
                };
                return json_api_error(StatusCode::FORBIDDEN, err, &correlation_id);
            }
            let Some(ref svc) = state.service else {
                return json_api_error(
                    StatusCode::UNAUTHORIZED,
                    DennettError::Unauthorized {
                        reason: String::from(
                            "control token verification unavailable without configured HeadService",
                        ),
                    },
                    &correlation_id,
                );
            };
            if let Err(err) = verify_optional_control_token(svc, request.headers()) {
                return domain_error_response(err, &correlation_id);
            }
        } else {
            let expected_owner_token = state
                .service
                .as_ref()
                .map_or(state.owner_token.as_str(), HeadService::owner_token);
            if let Err(err) = verify_owner_bearer_token(request.headers(), expected_owner_token) {
                return json_api_error(StatusCode::UNAUTHORIZED, err, &correlation_id);
            }
        }
    }

    let Ok(_permit) = Arc::clone(&state.request_limit).try_acquire_owned() else {
        let err = DennettError::Internal {
            detail: String::from("concurrent request limit reached"),
        };
        return json_api_error(StatusCode::SERVICE_UNAVAILABLE, err, &correlation_id);
    };

    // D2-ARCH-01 / D2-BUG-03 / D3+: Exempt session turn endpoints (`/messages`, `/steer`,
    // `/followups/resume`) from the fixed 180s HTTP layer timeout so interactive human approvals
    // (paused RPC deadline) and live turns are never spuriously aborted by a 504!
    let is_session_turn_endpoint = request.method() == Method::POST
        && path.starts_with("/v1/sessions/")
        && (path.ends_with("/messages")
            || path.ends_with("/steer")
            || path.ends_with("/followups/resume"));
    if is_session_turn_endpoint {
        return next.run(request).await;
    }

    if let Ok(resp) = time::timeout(HTTP_REQUEST_TIMEOUT, next.run(request)).await {
        resp
    } else {
        let err = DennettError::Timeout {
            operation: String::from("head_http_request"),
        };
        json_api_error(StatusCode::GATEWAY_TIMEOUT, err, &correlation_id)
    }
}

fn ct_eq_str(a: &str, b: &str) -> bool {
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    if ab.is_empty() || bb.is_empty() {
        return false;
    }
    let mut diff = ab.len() ^ bb.len();
    let max_len = ab.len().max(bb.len());
    for i in 0..max_len {
        let ca = ab.get(i).copied().unwrap_or(0);
        let cb = bb.get(i).copied().unwrap_or(0);
        diff |= usize::from(ca ^ cb);
    }
    diff == 0
}

fn verify_owner_bearer_token(
    headers: &HeaderMap,
    expected_token: &str,
) -> Result<(), DennettError> {
    let Some(raw_header) = headers.get(AUTHORIZATION) else {
        return Err(DennettError::Unauthorized {
            reason: String::from(
                "missing `Authorization: Bearer <DENNETT_HEAD_TOKEN>` header",
            ),
        });
    };
    let raw = raw_header
        .to_str()
        .map_err(|_| DennettError::Unauthorized {
            reason: String::from("Authorization header must be valid ASCII"),
        })?;
    let Some((scheme, token)) = raw.trim().split_once(' ') else {
        return Err(DennettError::Unauthorized {
            reason: String::from("Authorization header must use `Bearer <token>` scheme"),
        });
    };
    if !scheme.eq_ignore_ascii_case("bearer") || !ct_eq_str(token.trim(), expected_token) {
        return Err(DennettError::Unauthorized {
            reason: String::from("invalid owner bearer token"),
        });
    }
    Ok(())
}

fn is_control_token_allowed_route(method: &Method, path: &str) -> bool {
    if method == Method::GET && path == "/healthz" {
        return true;
    }
    if method == Method::GET && path == "/v1/capabilities/session-skills" {
        return true;
    }
    if method == Method::GET
        && let Some(slug) = path.strip_prefix("/v1/capabilities/session-skills/")
        && !slug.is_empty()
        && !slug.contains('/')
        && !slug.contains('\\')
        && !slug.contains("..")
    {
        let lower = slug.to_ascii_lowercase();
        if !lower.contains("%2f")
            && !lower.contains("%5c")
            && !lower.contains("%00")
            && !lower.contains("%2e%2e")
        {
            return true;
        }
    }
    if method == Method::GET && path == "/v1/agents" {
        return true;
    }
    if method == Method::POST && path == "/v1/sessions" {
        return true;
    }
    if let Some(rest) = path.strip_prefix("/v1/sessions/")
        && let Some(sess_seg) = rest.strip_suffix("/artifacts")
        && !sess_seg.is_empty()
        && !sess_seg.contains('/')
        && matches!(*method, Method::GET | Method::POST)
    {
        return true;
    }
    if path == "/v1/tasks" && matches!(*method, Method::GET | Method::POST) {
        return true;
    }
    if let Some(rest) = path.strip_prefix("/v1/tasks/") {
        if !rest.contains('/') && matches!(*method, Method::GET | Method::PATCH) {
            return true;
        }
        if let Some(task_seg) = rest.strip_suffix("/cancel")
            && !task_seg.is_empty()
            && !task_seg.contains('/')
            && method == Method::POST
        {
            return true;
        }
        if let Some(task_seg) = rest.strip_suffix("/signals")
            && !task_seg.is_empty()
            && !task_seg.contains('/')
            && method == Method::POST
        {
            return true;
        }
    }
    if let Some(rest) = path.strip_prefix("/v1/agents/")
        && let Some(agent_seg) = rest.strip_suffix("/messages")
        && !agent_seg.is_empty()
        && !agent_seg.contains('/')
        && matches!(*method, Method::GET | Method::POST)
    {
        return true;
    }
    false
}

fn verify_optional_control_token(
    svc: &HeadService,
    headers: &HeaderMap,
) -> Result<Option<ControlTokenClaims>, DennettError> {
    let Some(raw_header) = headers.get(&HEADER_CONTROL_TOKEN) else {
        return Ok(None);
    };
    let raw = raw_header
        .to_str()
        .map_err(|_| DennettError::Unauthorized {
            reason: String::from("X-Dennett-Control-Token header must be valid ASCII"),
        })?;
    svc.control_token_signer()
        .verify_control_token(raw, TimestampMs::now())
        .map(Some)
}

fn verify_required_control_token(
    svc: &HeadService,
    headers: &HeaderMap,
) -> Result<ControlTokenClaims, DennettError> {
    verify_optional_control_token(svc, headers)?.ok_or_else(|| DennettError::Unauthorized {
        reason: String::from(
            "X-Dennett-Control-Token header is required to access scoped session skills",
        ),
    })
}

fn require_control_token_project(
    claims: &ControlTokenClaims,
    action: &str,
) -> Result<ProjectId, DennettError> {
    claims.project_id.ok_or_else(|| DennettError::AccessDenied {
        scope: format!("session:{}", claims.session_id),
        reason: format!("standalone chat control MCP token is not permitted to {action}"),
    })
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
                    capabilities: d3_head_capabilities(),
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
    let control_claims = match verify_optional_control_token(&svc, &headers) {
        Ok(c) => c,
        Err(err) => return domain_error_response(err, &cid),
    };
    if let Some(ref claims) = control_claims
        && let Err(err) = require_control_token_project(claims, "list project agents")
    {
        return domain_error_response(err, &cid);
    }
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

async fn handle_get_project_file(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
    Query(query): Query<ProjectFileContentQuery>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let project_id = match ProjectId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc.get_project_file_content(project_id, query.path).await {
        Ok(preview) => json_with_correlation(StatusCode::OK, &preview, &cid),
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

async fn handle_fork_session(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
    payload: Result<Json<ForkSessionRequest>, JsonRejection>,
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
    match svc.fork_session(session_id, req, &cid).await {
        Ok(session) => json_with_correlation(StatusCode::CREATED, &session, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_delete_session(
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
    match svc.delete_session(session_id, &cid).await {
        Ok(()) => empty_with_correlation(StatusCode::NO_CONTENT, &cid),
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
    let control_claims = match verify_optional_control_token(&svc, &headers) {
        Ok(c) => c,
        Err(err) => return domain_error_response(err, &cid),
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    if let Some(claims) = control_claims {
        let allowed_project_id = claims
            .project_id
            .unwrap_or_else(standalone_system_project_id);
        if req.project_id != Some(allowed_project_id) {
            return domain_error_response(
                DennettError::AccessDenied {
                    scope: format!("project:{allowed_project_id}"),
                    reason: String::from(
                        "control MCP token cannot create sessions in another project",
                    ),
                },
                &cid,
            );
        }
        if req.session_kind != SessionKind::TaskRun {
            return domain_error_response(
                DennettError::AccessDenied {
                    scope: format!("project:{allowed_project_id}"),
                    reason: String::from(
                        "control MCP token may only create dedicated `task_run` sessions",
                    ),
                },
                &cid,
            );
        }
    }
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

async fn handle_get_session_live(
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
    match svc.get_session_live(session_id).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_enqueue_followup(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
    payload: Result<Json<EnqueueFollowupRequest>, JsonRejection>,
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
    match svc.enqueue_followup(session_id, req, &cid).await {
        Ok(record) => json_with_correlation(StatusCode::CREATED, &record, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_update_followup(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path((raw_session_id, raw_followup_id)): Path<(String, String)>,
    payload: Result<Json<UpdateFollowupRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let session_id = match SessionId::from_str(&raw_session_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let followup_id = match FollowupId::from_str(&raw_followup_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc
        .update_followup(session_id, followup_id, req, &cid)
        .await
    {
        Ok(record) => json_with_correlation(StatusCode::OK, &record, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_delete_followup(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path((raw_session_id, raw_followup_id)): Path<(String, String)>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let session_id = match SessionId::from_str(&raw_session_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let followup_id = match FollowupId::from_str(&raw_followup_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc.delete_followup(session_id, followup_id, &cid).await {
        Ok(()) => empty_with_correlation(StatusCode::NO_CONTENT, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_clear_followups(
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
    match svc.clear_followups(session_id, &cid).await {
        Ok(()) => empty_with_correlation(StatusCode::NO_CONTENT, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_resume_followups(
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
    match svc.resume_followups(session_id, &cid).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_steer_session(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
    payload: Result<Json<SteerSessionRequest>, JsonRejection>,
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
    match svc.steer_session(session_id, req, cid.clone()).await {
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
    let control_claims = match verify_optional_control_token(&svc, &headers) {
        Ok(c) => c,
        Err(err) => return domain_error_response(err, &cid),
    };
    let session_id = match SessionId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    if let Some(claims) = control_claims
        && claims.session_id != session_id
    {
        return domain_error_response(
            DennettError::AccessDenied {
                scope: format!("session:{}", claims.session_id),
                reason: String::from(
                    "control MCP token can only register artifacts in its own session",
                ),
            },
            &cid,
        );
    }
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
    let control_claims = match verify_optional_control_token(&svc, &headers) {
        Ok(c) => c,
        Err(err) => return domain_error_response(err, &cid),
    };
    let session_id = match SessionId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    if let Some(claims) = control_claims
        && claims.session_id != session_id
    {
        return domain_error_response(
            DennettError::AccessDenied {
                scope: format!("session:{}", claims.session_id),
                reason: String::from(
                    "control MCP token can only list artifacts in its own session",
                ),
            },
            &cid,
        );
    }
    match svc.list_artifacts(session_id).await {
        Ok(list) => json_with_correlation(StatusCode::OK, &list, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_get_artifact_content(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path((raw_session_id, raw_artifact_id)): Path<(String, String)>,
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
    match svc.get_artifact_content(session_id, artifact_id).await {
        Ok(blob) => {
            let mut resp = Response::new(Body::from(blob.bytes));
            *resp.status_mut() = StatusCode::OK;
            let content_type = HeaderValue::from_str(&blob.mime_type)
                .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));
            resp.headers_mut().insert(CONTENT_TYPE, content_type);
            if let Ok(sha_hdr) = HeaderValue::from_str(blob.sha256.as_str()) {
                resp.headers_mut()
                    .insert(HEADER_ARTIFACT_SHA256.clone(), sha_hdr);
            }
            resp.headers_mut().insert(
                HEADER_CONTENT_TYPE_OPTIONS.clone(),
                HeaderValue::from_static("nosniff"),
            );
            if let Ok(cid_hdr) = HeaderValue::from_str(cid.as_str()) {
                resp.headers_mut()
                    .insert(HEADER_CORRELATION_ID.clone(), cid_hdr);
            }
            resp
        }
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

async fn handle_delete_policy_rule(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let rule_id = match PolicyRuleId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc.delete_policy_rule(rule_id, &cid).await {
        Ok(()) => empty_with_correlation(StatusCode::NO_CONTENT, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_get_preferences(State(state): State<HeadAppState>, headers: HeaderMap) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    match svc.get_preferences().await {
        Ok(prefs) => json_with_correlation(StatusCode::OK, &prefs, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_update_preferences(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    payload: Result<Json<UpdateUserPreferencesRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc.update_preferences(req, &cid).await {
        Ok(prefs) => json_with_correlation(StatusCode::OK, &prefs, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_emergency_stop(State(state): State<HeadAppState>, headers: HeaderMap) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    match svc.emergency_stop_all(&cid).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
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
    let control_claims = match verify_optional_control_token(&svc, &headers) {
        Ok(c) => c,
        Err(err) => return domain_error_response(err, &cid),
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    if let Some(claims) = control_claims {
        let standalone_pid = standalone_system_project_id();
        if let Some(token_project_id) = claims.project_id {
            if req.project_id != token_project_id {
                return domain_error_response(
                    DennettError::AccessDenied {
                        scope: format!("project:{token_project_id}"),
                        reason: String::from(
                            "control MCP token cannot create tasks/events in another project",
                        ),
                    },
                    &cid,
                );
            }
            if claims.session_kind == SessionKind::TaskRun
                && req.parent_task_id.is_none()
                && req.schedule.is_none()
            {
                return domain_error_response(
                    DennettError::AccessDenied {
                        scope: format!("session:{}", claims.session_id),
                        reason: String::from(
                            "background task_run session cannot create unscheduled top-level tasks via control MCP",
                        ),
                    },
                    &cid,
                );
            }
            if (req.agent_id != claims.agent_id || req.connection_override_id.is_some())
                && let Err(err) = svc
                    .enforce_delegated_agent_scope_subset(
                        claims.agent_id,
                        req.agent_id,
                        req.connection_override_id,
                    )
                    .await
            {
                return domain_error_response(err, &cid);
            }
        } else {
            // Standalone chat (`project_id == None`): permit ONLY standalone session reminders
            // backed by `STANDALONE_SYSTEM_PROJECT_UUID` for this exact session & agent.
            let scoped_to_caller = req.session_id == Some(claims.session_id)
                || req.target_session_id == Some(claims.session_id)
                || req.notify_session_id == Some(claims.session_id);
            if req.project_id != standalone_pid
                || req.parent_task_id.is_some()
                || req.agent_id != claims.agent_id
                || !scoped_to_caller
                || (req.schedule.is_none() && req.trigger.is_none())
            {
                return domain_error_response(
                    DennettError::AccessDenied {
                        scope: format!("session:{}", claims.session_id),
                        reason: String::from(
                            "standalone chat control MCP token may only create scheduled reminders bound to its own session",
                        ),
                    },
                    &cid,
                );
            }
        }
    }
    match svc.create_task(req, &cid).await {
        Ok(task) => json_with_correlation(StatusCode::CREATED, &task, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

fn task_visible_to_standalone_claims(
    task: &dennett_contracts::TaskRecord,
    claims: &ControlTokenClaims,
) -> bool {
    task.project_id == standalone_system_project_id()
        && (task.session_id == claims.session_id
            || task.target_session_id == Some(claims.session_id)
            || task.notify_session_id == Some(claims.session_id))
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
    let control_claims = match verify_optional_control_token(&svc, &headers) {
        Ok(c) => c,
        Err(err) => return domain_error_response(err, &cid),
    };
    let mut project_id = match parse_optional_id::<ProjectId>(query.project_id.as_deref()) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    if let Some(ref claims) = control_claims {
        let allowed_pid = claims
            .project_id
            .unwrap_or_else(standalone_system_project_id);
        if let Some(requested_pid) = project_id
            && requested_pid != allowed_pid
        {
            return domain_error_response(
                DennettError::AccessDenied {
                    scope: format!("project:{allowed_pid}"),
                    reason: String::from(
                        "control MCP token cannot list tasks belonging to another project",
                    ),
                },
                &cid,
            );
        }
        project_id = Some(allowed_pid);
    }
    match svc.list_tasks(project_id, query.status.as_deref()).await {
        Ok(mut list) => {
            if let Some(ref claims) = control_claims
                && claims.project_id.is_none()
            {
                list.tasks
                    .retain(|t| task_visible_to_standalone_claims(t, claims));
            }
            json_with_correlation(StatusCode::OK, &list, &cid)
        }
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
    let control_claims = match verify_optional_control_token(&svc, &headers) {
        Ok(c) => c,
        Err(err) => return domain_error_response(err, &cid),
    };
    let task_id = match TaskId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc.get_task_detail(task_id).await {
        Ok(detail) => {
            if let Some(claims) = control_claims {
                let allowed = match claims.project_id {
                    Some(token_pid) => detail.task.project_id == token_pid,
                    None => task_visible_to_standalone_claims(&detail.task, &claims),
                };
                if !allowed {
                    return domain_error_response(
                        DennettError::AccessDenied {
                            scope: format!("session:{}", claims.session_id),
                            reason: String::from(
                                "control MCP token cannot inspect a task outside its scope",
                            ),
                        },
                        &cid,
                    );
                }
            }
            json_with_correlation(StatusCode::OK, &detail, &cid)
        }
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
    let control_claims = match verify_optional_control_token(&svc, &headers) {
        Ok(c) => c,
        Err(err) => return domain_error_response(err, &cid),
    };
    let task_id = match TaskId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    if let Some(claims) = control_claims {
        match svc.get_task_detail(task_id).await {
            Ok(detail) => {
                let allowed = match claims.project_id {
                    Some(token_pid) => detail.task.project_id == token_pid,
                    None => task_visible_to_standalone_claims(&detail.task, &claims),
                };
                if !allowed {
                    return domain_error_response(
                        DennettError::AccessDenied {
                            scope: format!("session:{}", claims.session_id),
                            reason: String::from(
                                "control MCP token cannot modify a task outside its scope",
                            ),
                        },
                        &cid,
                    );
                }
            }
            Err(err) => return domain_error_response(err, &cid),
        }
    }
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
    let control_claims = match verify_optional_control_token(&svc, &headers) {
        Ok(c) => c,
        Err(err) => return domain_error_response(err, &cid),
    };
    let task_id = match TaskId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    if let Some(claims) = control_claims {
        let token_project_id = match require_control_token_project(&claims, "send task signals") {
            Ok(pid) => pid,
            Err(err) => return domain_error_response(err, &cid),
        };
        match svc.get_task_detail(task_id).await {
            Ok(detail) if detail.task.project_id == token_project_id => {}
            Ok(_) => {
                return domain_error_response(
                    DennettError::AccessDenied {
                        scope: format!("project:{token_project_id}"),
                        reason: String::from(
                            "control MCP token cannot send signals to a task in another project",
                        ),
                    },
                    &cid,
                );
            }
            Err(err) => return domain_error_response(err, &cid),
        }
    }
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

async fn handle_run_task_now(
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
    match svc.run_task_now(task_id, &cid).await {
        Ok(task) => json_with_correlation(StatusCode::OK, &task, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_pause_task(
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
    match svc.pause_task(task_id, &cid).await {
        Ok(task) => json_with_correlation(StatusCode::OK, &task, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_resume_task(
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
    match svc.resume_task(task_id, &cid).await {
        Ok(task) => json_with_correlation(StatusCode::OK, &task, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_delete_task(
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
    match svc.delete_task(task_id, &cid).await {
        Ok(()) => empty_with_correlation(StatusCode::NO_CONTENT, &cid),
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
    let control_claims = match verify_optional_control_token(&svc, &headers) {
        Ok(c) => c,
        Err(err) => return domain_error_response(err, &cid),
    };
    let task_id = match TaskId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    if let Some(claims) = control_claims {
        match svc.get_task_detail(task_id).await {
            Ok(detail) => {
                let allowed = match claims.project_id {
                    Some(token_pid) => detail.task.project_id == token_pid,
                    None => task_visible_to_standalone_claims(&detail.task, &claims),
                };
                if !allowed {
                    return domain_error_response(
                        DennettError::AccessDenied {
                            scope: format!("session:{}", claims.session_id),
                            reason: String::from(
                                "control MCP token cannot cancel a task outside its scope",
                            ),
                        },
                        &cid,
                    );
                }
            }
            Err(err) => return domain_error_response(err, &cid),
        }
    }
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
    let control_claims = match verify_optional_control_token(&svc, &headers) {
        Ok(c) => c,
        Err(err) => return domain_error_response(err, &cid),
    };
    let agent_id = match AgentId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let Json(mut req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    if let Some(claims) = control_claims {
        let token_project_id =
            match require_control_token_project(&claims, "send inter-agent messages") {
                Ok(pid) => pid,
                Err(err) => return domain_error_response(err, &cid),
            };
        if req.project_id != token_project_id {
            return domain_error_response(
                DennettError::AccessDenied {
                    scope: format!("project:{token_project_id}"),
                    reason: String::from(
                        "control MCP token cannot send agent messages across projects",
                    ),
                },
                &cid,
            );
        }
        if let Some(sender) = req.sender_agent_id
            && sender != claims.agent_id
        {
            return domain_error_response(
                DennettError::AccessDenied {
                    scope: format!("agent:{}", claims.agent_id),
                    reason: String::from("control MCP token cannot spoof another sender_agent_id"),
                },
                &cid,
            );
        }
        req.sender_agent_id = Some(claims.agent_id);
    }
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
    let control_claims = match verify_optional_control_token(&svc, &headers) {
        Ok(c) => c,
        Err(err) => return domain_error_response(err, &cid),
    };
    let agent_id = match AgentId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    if let Some(ref claims) = control_claims {
        if let Err(err) = require_control_token_project(claims, "list inter-agent messages") {
            return domain_error_response(err, &cid);
        }
        if agent_id != claims.agent_id {
            return domain_error_response(
                DennettError::AccessDenied {
                    scope: format!("agent:{}", claims.agent_id),
                    reason: String::from(
                        "control MCP token can only list messages for its own agent",
                    ),
                },
                &cid,
            );
        }
    }
    let undelivered_only = query.undelivered_only.unwrap_or(false);
    match svc.list_agent_messages(agent_id, undelivered_only).await {
        Ok(mut resp) => {
            if let Some(claims) = control_claims
                && let Some(token_project_id) = claims.project_id
            {
                resp.messages.retain(|m| m.project_id == token_project_id);
            }
            json_with_correlation(StatusCode::OK, &resp, &cid)
        }
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

// ---------------------------------------------------------------------------
// Phase D3 Handlers: SecretBroker, Provider Connections, Capabilities, Memory
// ---------------------------------------------------------------------------

async fn handle_list_secrets(State(state): State<HeadAppState>, headers: HeaderMap) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    match svc.list_secrets().await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_set_secret(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(slot_key): Path<String>,
    payload: Result<Json<SetSecretRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc.set_secret(&slot_key, req, &cid).await {
        Ok(meta) => json_with_correlation(StatusCode::OK, &meta, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_delete_secret(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(slot_key): Path<String>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    match svc.delete_secret(&slot_key, &cid).await {
        Ok(meta) => json_with_correlation(StatusCode::OK, &meta, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_list_provider_connections(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    match svc.list_provider_connections().await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_create_provider_connection(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    payload: Result<Json<UpsertProviderConnectionRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc.create_provider_connection(req, &cid).await {
        Ok(conn) => json_with_correlation(StatusCode::CREATED, &conn, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_update_provider_connection(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
    payload: Result<Json<UpsertProviderConnectionRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let connection_id = match ProviderConnectionId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc
        .update_provider_connection(connection_id, req, &cid)
        .await
    {
        Ok(conn) => json_with_correlation(StatusCode::OK, &conn, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_delete_provider_connection(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let connection_id = match ProviderConnectionId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc.delete_provider_connection(connection_id, &cid).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_probe_provider_connection(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let connection_id = match ProviderConnectionId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc.probe_provider_connection(connection_id, &cid).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_activate_provider_connection(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
    payload: Result<Json<ActivateConnectionRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let connection_id = match ProviderConnectionId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc
        .activate_provider_connection(connection_id, req, &cid)
        .await
    {
        Ok(conn) => json_with_correlation(StatusCode::OK, &conn, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_list_capabilities(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Query(query): Query<ListCapabilitiesQuery>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let project_id = match parse_optional_id::<ProjectId>(query.project_id.as_deref()) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let scope_kind = match parse_optional_id::<CapabilityScopeKind>(query.scope_kind.as_deref()) {
        Ok(sk) => sk,
        Err(err) => return domain_error_response(err, &cid),
    };
    let kind = match parse_optional_id::<CapabilityKind>(query.kind.as_deref()) {
        Ok(k) => k,
        Err(err) => return domain_error_response(err, &cid),
    };
    let origin = match parse_optional_id::<CapabilityOrigin>(query.origin.as_deref()) {
        Ok(o) => o,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc
        .list_capabilities(project_id, scope_kind, kind, origin)
        .await
    {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_create_capability(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    payload: Result<Json<CreateCapabilityRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc.create_capability(req, &cid).await {
        Ok(cap) => json_with_correlation(StatusCode::CREATED, &cap, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_discover_capabilities(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    payload: Result<Json<DiscoverCapabilitiesRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc.discover_capabilities(req, &cid).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_list_session_skills(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let claims = match verify_required_control_token(&svc, &headers) {
        Ok(c) => c,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc.list_session_skills(&claims).await {
        Ok(skills) => json_with_correlation(StatusCode::OK, &skills, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_read_session_skill(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let claims = match verify_required_control_token(&svc, &headers) {
        Ok(c) => c,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc.read_session_skill(&claims, &slug).await {
        Ok(detail) => json_with_correlation(StatusCode::OK, &detail, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_update_capability(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
    payload: Result<Json<UpdateCapabilityRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let capability_id = match CapabilityId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc.update_capability(capability_id, req, &cid).await {
        Ok(cap) => json_with_correlation(StatusCode::OK, &cap, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_delete_capability(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let capability_id = match CapabilityId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc.delete_capability(capability_id, &cid).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_rollback_capability(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let capability_id = match CapabilityId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc.rollback_capability(capability_id, &cid).await {
        Ok(cap) => json_with_correlation(StatusCode::OK, &cap, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_set_project_capability_binding(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path((raw_project_id, raw_cap_id)): Path<(String, String)>,
    payload: Result<Json<SetProjectCapabilityBindingRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let project_id = match ProjectId::from_str(&raw_project_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let capability_id = match CapabilityId::from_str(&raw_cap_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc
        .set_project_capability_binding(project_id, capability_id, req, &cid)
        .await
    {
        Ok(binding) => json_with_correlation(StatusCode::OK, &binding, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

fn parse_optional_memory_scope(raw: Option<&str>) -> Result<Option<MemoryScope>, DennettError> {
    raw.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(MemoryScope::try_new)
        .transpose()
}

async fn handle_memory_overview(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Query(query): Query<MemoryScopeQuery>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let scope = match parse_optional_memory_scope(query.scope.as_deref()) {
        Ok(s) => s,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc.get_memory_overview(scope, &cid).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_correct_memory_note(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
    payload: Result<Json<CorrectNoteRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let note_id = match MemoryNoteId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    if req.note_id != note_id {
        return domain_error_response(
            DennettError::InvalidInput {
                field: String::from("note_id"),
                reason: String::from("path note_id does not match request body note_id"),
            },
            &cid,
        );
    }
    match svc.proxy_correct_memory_note(&req, &cid).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_get_memory_note_history(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let note_id = match MemoryNoteId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc.proxy_get_memory_note_history(note_id, &cid).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_list_memory_claims(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Query(query): Query<ListMemoryClaimsQuery>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let scope = match parse_optional_memory_scope(query.scope.as_deref()) {
        Ok(s) => s,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc
        .proxy_list_memory_claims(scope.as_ref(), query.topic_key.as_deref(), &cid)
        .await
    {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_create_memory_claim(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    payload: Result<Json<CreateClaimRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc.proxy_create_memory_claim(&req, &cid).await {
        Ok(claim) => json_with_correlation(StatusCode::CREATED, &claim, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_supersede_memory_claim(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
    payload: Result<Json<SupersedeClaimRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let claim_id = match ClaimId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    if req.claim_id != claim_id {
        return domain_error_response(
            DennettError::InvalidInput {
                field: String::from("claim_id"),
                reason: String::from("path claim_id does not match request body claim_id"),
            },
            &cid,
        );
    }
    match svc.proxy_supersede_memory_claim(&req, &cid).await {
        Ok(claim) => json_with_correlation(StatusCode::OK, &claim, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_list_memory_conflicts(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Query(query): Query<MemoryScopeQuery>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let scope = match parse_optional_memory_scope(query.scope.as_deref()) {
        Ok(s) => s,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc.proxy_list_memory_conflicts(scope.as_ref(), &cid).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_resolve_memory_conflict(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
    payload: Result<Json<ResolveConflictRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let conflict_id = match ConflictId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    if req.conflict_id != conflict_id {
        return domain_error_response(
            DennettError::InvalidInput {
                field: String::from("conflict_id"),
                reason: String::from("path conflict_id does not match request body conflict_id"),
            },
            &cid,
        );
    }
    match svc.proxy_resolve_memory_conflict(&req, &cid).await {
        Ok(conflict) => json_with_correlation(StatusCode::OK, &conflict, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_list_memory_documents(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Query(query): Query<MemoryScopeQuery>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let scope = match parse_optional_memory_scope(query.scope.as_deref()) {
        Ok(s) => s,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc.proxy_list_memory_documents(scope.as_ref(), &cid).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_create_memory_document(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    payload: Result<Json<CreateDocumentRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc.proxy_create_memory_document(&req, &cid).await {
        Ok(doc) => json_with_correlation(StatusCode::CREATED, &doc, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_get_memory_document(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let document_id = match DocumentId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc.proxy_get_memory_document(document_id, &cid).await {
        Ok(doc) => json_with_correlation(StatusCode::OK, &doc, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_update_memory_document(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
    payload: Result<Json<UpdateDocumentRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let document_id = match DocumentId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    if req.document_id != document_id {
        return domain_error_response(
            DennettError::InvalidInput {
                field: String::from("document_id"),
                reason: String::from("path document_id does not match request body document_id"),
            },
            &cid,
        );
    }
    match svc.proxy_update_memory_document(&req, &cid).await {
        Ok(doc) => json_with_correlation(StatusCode::OK, &doc, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_get_memory_document_versions(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let document_id = match DocumentId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc
        .proxy_get_memory_document_versions(document_id, &cid)
        .await
    {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_forget_memory(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    payload: Result<Json<ForgetRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc.proxy_forget_memory_item(&req, &cid).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_list_memory_tombstones(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Query(query): Query<MemoryTombstonesQuery>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let scope = match parse_optional_memory_scope(query.scope.as_deref()) {
        Ok(s) => s,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc
        .proxy_list_memory_tombstones_since(scope.as_ref(), query.after_seq, query.limit, &cid)
        .await
    {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_export_memory(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    payload: Result<Json<ExportRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc.proxy_export_memory_package(&req, &cid).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_import_memory(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    payload: Result<Json<ImportRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc.proxy_import_memory_package(&req, &cid).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_get_memory_index_status(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Query(query): Query<MemoryScopeQuery>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let scope = match parse_optional_memory_scope(query.scope.as_deref()) {
        Ok(s) => s,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc.proxy_get_memory_index_status(scope.as_ref(), &cid).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_trigger_memory_index_rebuild(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    payload: Result<Json<TriggerIndexRebuildRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc.proxy_trigger_memory_index_rebuild(&req, &cid).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_list_memory_consolidation_jobs(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Query(query): Query<MemoryScopeQuery>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let scope = match parse_optional_memory_scope(query.scope.as_deref()) {
        Ok(s) => s,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc
        .proxy_list_memory_consolidation_jobs(scope.as_ref(), &cid)
        .await
    {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_create_memory_consolidation_job(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    payload: Result<Json<CreateConsolidationJobRequest>, JsonRejection>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &cid),
    };
    match svc.proxy_create_memory_consolidation_job(&req, &cid).await {
        Ok(job) => json_with_correlation(StatusCode::CREATED, &job, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_pause_memory_consolidation_job(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let job_id = match ConsolidationJobId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc.proxy_pause_memory_consolidation_job(job_id, &cid).await {
        Ok(job) => json_with_correlation(StatusCode::OK, &job, &cid),
        Err(err) => domain_error_response(err, &cid),
    }
}

async fn handle_resume_memory_consolidation_job(
    State(state): State<HeadAppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
) -> Response {
    let (cid, svc) = match get_service(&state, &headers) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };
    let job_id = match ConsolidationJobId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &cid),
    };
    match svc
        .proxy_resume_memory_consolidation_job(job_id, &cid)
        .await
    {
        Ok(job) => json_with_correlation(StatusCode::OK, &job, &cid),
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
        DennettError::ProviderQuotaExceeded { .. } => StatusCode::TOO_MANY_REQUESTS,
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

fn empty_with_correlation(status: StatusCode, correlation_id: &CorrelationId) -> Response {
    let mut resp = status.into_response();
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
