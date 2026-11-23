//! Standalone `dennett-memoryd` daemon for Dennett Memory Fabric (D1+).
//!
//! Runs completely independently of any Head or UI component, owns its PostgreSQL
//! schema migrations, enforces standalone scoped bearer authentication via
//! [`StandaloneAuthVerifier`], exposes versioned `/v1/memory/*` JSON endpoints and
//! `/healthz` over `axum`, and redacts secrets from every error path.

use std::env;
use std::io::{self, Write};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::rejection::JsonRejection;
use axum::extract::{ConnectInfo, DefaultBodyLimit, Path, Request, State};
use axum::http::header::{AUTHORIZATION, HOST};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use dennett_contracts::redact::{redact_secrets, sanitize_cli_arg, sanitize_pg_error};
use dennett_contracts::{
    ApiErrorResponse, AuthPrincipal, CorrectNoteRequest, CorrelationId, CreateNoteRequest,
    DennettError, EvidenceId, IngestEvidenceRequest, MemoryEventId, MemoryNoteId, ProfileMode,
    SearchNotesRequest, ServiceHealth, ServiceKind, TimestampMs,
};
use dennett_memory_core::{
    AuthVerifier, MEMORYD_STAGE_LABEL, MemoryService, StandaloneAuthVerifier, ValidatedPgTarget,
    d1_memoryd_capabilities, parse_bearer_header,
};
use serde::Serialize;
use tokio::net::TcpListener;
use tokio::sync::{Semaphore, watch};
use tokio::time;

/// Maximum incoming HTTP request body size (512 KiB, accommodating 256 KiB evidence + JSON overhead).
const MAX_HTTP_BODY_BYTES: usize = 512 * 1024;
/// Maximum concurrent in-flight HTTP requests accepted simultaneously by `dennett-memoryd`.
const MAX_CONCURRENT_REQUESTS: usize = 64;
/// Maximum end-to-end wall time permitted for a single HTTP request handler.
const HTTP_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

static HEADER_CORRELATION_ID: HeaderName = HeaderName::from_static("x-correlation-id");
static HEADER_SHUTDOWN: HeaderName = HeaderName::from_static("x-dennett-shutdown");

#[derive(Debug, PartialEq, Eq)]
enum Command {
    HealthOnce {
        pg_url: Option<String>,
    },
    Serve {
        addr: SocketAddr,
        pg_url: Option<String>,
        auth_config_file: Option<PathBuf>,
    },
    Help,
    Version,
}

#[derive(Debug, Serialize)]
struct HealthFailureBody {
    #[serde(flatten)]
    health: ServiceHealth,
    error: DennettError,
}

#[derive(Debug, Serialize)]
struct ShutdownAck {
    status: &'static str,
    correlation_id: CorrelationId,
}

#[derive(Clone)]
struct AppState {
    service: Option<MemoryService>,
    auth: StandaloneAuthVerifier,
    profile_mode: ProfileMode,
    shutdown_tx: watch::Sender<bool>,
    request_limit: Arc<Semaphore>,
}

#[tokio::main]
async fn main() -> ExitCode {
    let command = match parse_args(env::args().skip(1)) {
        Ok(cmd) => cmd,
        Err(err) => {
            emit_stderr_error(&err);
            return ExitCode::from(2);
        }
    };

    let profile_mode = active_profile_mode();

    match command {
        Command::Help => {
            print_usage();
            ExitCode::SUCCESS
        }
        Command::Version => {
            println!(
                "dennett-memoryd {} ({MEMORYD_STAGE_LABEL})",
                env!("CARGO_PKG_VERSION")
            );
            ExitCode::SUCCESS
        }
        Command::HealthOnce { pg_url } => run_health_once(pg_url, profile_mode).await,
        Command::Serve {
            addr,
            pg_url,
            auth_config_file,
        } => run_serve(addr, pg_url, auth_config_file, profile_mode).await,
    }
}

fn parse_args<I>(args: I) -> Result<Command, DennettError>
where
    I: IntoIterator<Item = String>,
{
    let mut iter = args.into_iter();
    let mut health_once = false;
    let mut serve_addr: Option<String> = None;
    let mut pg_url: Option<String> = None;
    let mut auth_config_file: Option<PathBuf> = None;
    let mut allow_non_loopback = false;

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "-V" | "--version" => return Ok(Command::Version),
            "--health-once" => health_once = true,
            "--allow-non-loopback" => allow_non_loopback = true,
            "--serve" => {
                let raw = iter.next().ok_or_else(|| DennettError::InvalidInput {
                    field: String::from("--serve"),
                    reason: String::from("expected <ip:port> address after --serve"),
                })?;
                serve_addr = Some(raw);
            }
            "--pg-url" => {
                let raw = iter.next().ok_or_else(|| DennettError::InvalidInput {
                    field: String::from("--pg-url"),
                    reason: String::from("expected PostgreSQL URL after --pg-url"),
                })?;
                pg_url = Some(raw);
            }
            "--auth-config" => {
                let raw = iter.next().ok_or_else(|| DennettError::InvalidInput {
                    field: String::from("--auth-config"),
                    reason: String::from("expected JSON file path after --auth-config"),
                })?;
                auth_config_file = Some(PathBuf::from(raw));
            }
            other => {
                let safe = sanitize_cli_arg(other);
                return Err(DennettError::InvalidInput {
                    field: String::from("cli"),
                    reason: format!("unrecognized argument `{safe}`"),
                });
            }
        }
    }

    match (health_once, serve_addr) {
        (true, Some(_)) => Err(DennettError::InvalidInput {
            field: String::from("cli"),
            reason: String::from("cannot combine --health-once and --serve"),
        }),
        (true, None) => Ok(Command::HealthOnce { pg_url }),
        (false, Some(raw_addr)) => {
            let addr: SocketAddr = raw_addr.parse().map_err(|err| DennettError::InvalidInput {
                field: String::from("--serve"),
                reason: format!(
                    "invalid socket address `{}`: {err}",
                    sanitize_pg_error(&raw_addr, None)
                ),
            })?;
            if !addr.ip().is_loopback() && !allow_non_loopback {
                return Err(DennettError::SafetyGuardRejected {
                    reason: format!(
                        "refusing non-loopback bind `{addr}` without explicit `--allow-non-loopback`"
                    ),
                });
            }
            Ok(Command::Serve {
                addr,
                pg_url,
                auth_config_file,
            })
        }
        (false, None) => Err(DennettError::InvalidInput {
            field: String::from("cli"),
            reason: String::from("specify either --health-once or --serve <addr>"),
        }),
    }
}

fn active_profile_mode() -> ProfileMode {
    match env::var("DENNETT_TEST_PROFILE").ok().as_deref() {
        Some("1") => ProfileMode::TestIsolated,
        _ => ProfileMode::Production,
    }
}

fn resolve_pg_target(
    cli_override: Option<&str>,
    profile_mode: ProfileMode,
) -> Result<Option<ValidatedPgTarget>, DennettError> {
    let prod_env = env::var("DENNETT_MEMORY_PG_URL").ok();
    let test_env = env::var("DENNETT_TEST_MEMORY_PG_URL").ok();
    resolve_pg_target_from_sources(
        cli_override,
        test_env.as_deref(),
        prod_env.as_deref(),
        profile_mode,
    )
}

fn resolve_pg_target_from_sources(
    cli_override: Option<&str>,
    test_env: Option<&str>,
    prod_env: Option<&str>,
    profile_mode: ProfileMode,
) -> Result<Option<ValidatedPgTarget>, DennettError> {
    // In `TestIsolated`, never fall back to production `DENNETT_MEMORY_PG_URL`.
    let chosen = match profile_mode {
        ProfileMode::TestIsolated => non_empty(cli_override).or_else(|| non_empty(test_env)),
        ProfileMode::Production => non_empty(cli_override).or_else(|| non_empty(prod_env)),
    };

    chosen
        .map(|url| ValidatedPgTarget::parse(url, profile_mode))
        .transpose()
}

fn load_auth_verifier(cli_file: Option<&PathBuf>) -> Result<StandaloneAuthVerifier, DennettError> {
    if let Some(path) = cli_file {
        return StandaloneAuthVerifier::from_file(path);
    }
    if let Ok(env_path) = env::var("DENNETT_MEMORY_AUTH_CONFIG_FILE") {
        let trimmed = env_path.trim();
        if !trimmed.is_empty() {
            return StandaloneAuthVerifier::from_file(&PathBuf::from(trimmed));
        }
    }
    if let Ok(inline_json) = env::var("DENNETT_MEMORY_AUTH_CONFIG") {
        let trimmed = inline_json.trim();
        if !trimmed.is_empty() {
            return StandaloneAuthVerifier::from_json_str(trimmed);
        }
    }
    Ok(StandaloneAuthVerifier::fail_closed())
}

fn non_empty(val: Option<&str>) -> Option<&str> {
    val.map(str::trim).filter(|s| !s.is_empty())
}

async fn run_health_once(cli_pg_url: Option<String>, profile_mode: ProfileMode) -> ExitCode {
    let correlation_id = CorrelationId::generate();
    let target = match resolve_pg_target(cli_pg_url.as_deref(), profile_mode) {
        Ok(t) => t,
        Err(err) => {
            emit_stderr_error(&err);
            return ExitCode::from(1);
        }
    };

    let Some(pg_target) = target else {
        emit_stderr_error(&unconfigured_pg_error());
        return ExitCode::from(1);
    };

    let service = match MemoryService::new(pg_target) {
        Ok(s) => s,
        Err(err) => {
            emit_stderr_error(&err);
            return ExitCode::from(1);
        }
    };

    match service.health_check(profile_mode, correlation_id).await {
        Ok(health) => match serde_json::to_string(&health) {
            Ok(json) => {
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(err) => {
                emit_stderr_error(&DennettError::Internal {
                    detail: format!("failed to serialize health response: {err}"),
                });
                ExitCode::from(1)
            }
        },
        Err(err) => {
            emit_stderr_error(&err);
            ExitCode::from(1)
        }
    }
}

async fn run_serve(
    addr: SocketAddr,
    cli_pg_url: Option<String>,
    auth_config_file: Option<PathBuf>,
    profile_mode: ProfileMode,
) -> ExitCode {
    let target = match resolve_pg_target(cli_pg_url.as_deref(), profile_mode) {
        Ok(t) => t,
        Err(err) => {
            emit_stderr_error(&err);
            return ExitCode::from(1);
        }
    };

    let auth = match load_auth_verifier(auth_config_file.as_ref()) {
        Ok(v) => v,
        Err(err) => {
            emit_stderr_error(&err);
            return ExitCode::from(1);
        }
    };

    let service = match target {
        Some(pg_target) => match MemoryService::new(pg_target) {
            Ok(svc) => {
                match svc.ensure_migrated().await {
                    Ok(_) => {}
                    // Fail fast on schema corruption / incompatible newer DB version.
                    Err(err @ DennettError::SafetyGuardRejected { .. }) => {
                        emit_stderr_error(&err);
                        return ExitCode::from(1);
                    }
                    // Transient PostgreSQL outage still lets `/healthz` report honest 503.
                    Err(err) => {
                        emit_stderr_error(&err);
                    }
                }
                Some(svc)
            }
            Err(err) => {
                emit_stderr_error(&err);
                return ExitCode::from(1);
            }
        },
        None => None,
    };

    let listener = match TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(err) => {
            emit_stderr_error(&DennettError::Internal {
                detail: format!("failed to bind listener on {addr}: {err}"),
            });
            return ExitCode::from(1);
        }
    };

    let bound_addr = match listener.local_addr() {
        Ok(a) => a,
        Err(err) => {
            emit_stderr_error(&DennettError::Internal {
                detail: format!("failed to read bound address: {err}"),
            });
            return ExitCode::from(1);
        }
    };

    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
    let ctrl_c_tx = shutdown_tx.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            let _ = ctrl_c_tx.send(true);
        }
    });

    let state = AppState {
        service,
        auth,
        profile_mode,
        shutdown_tx,
        request_limit: Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS)),
    };

    let app = build_router(state);

    if writeln!(io::stdout(), "LISTENING {bound_addr}").is_err() || io::stdout().flush().is_err() {
        emit_stderr_error(&DennettError::Internal {
            detail: String::from("failed to flush listening announcement to stdout"),
        });
        return ExitCode::from(1);
    }

    let serve_result = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        while !*shutdown_rx.borrow() {
            if shutdown_rx.changed().await.is_err() {
                break;
            }
        }
    })
    .await;

    if let Err(err) = serve_result {
        emit_stderr_error(&DennettError::Internal {
            detail: format!("HTTP server error: {err}"),
        });
        return ExitCode::from(1);
    }

    ExitCode::SUCCESS
}

fn build_router(state: AppState) -> Router {
    let mut router = Router::new()
        .route("/healthz", get(handle_healthz))
        .route("/v1/memory/evidence", post(handle_ingest_evidence))
        .route(
            "/v1/memory/evidence/{evidence_id}",
            get(handle_get_evidence),
        )
        .route("/v1/memory/notes", post(handle_create_note))
        .route("/v1/memory/notes/search", post(handle_search_notes))
        .route("/v1/memory/notes/{note_id}", get(handle_get_note_current))
        .route(
            "/v1/memory/notes/{note_id}/history",
            get(handle_get_note_history),
        )
        .route(
            "/v1/memory/notes/{note_id}/corrections",
            post(handle_correct_note),
        )
        .route("/v1/memory/events/{event_id}", get(handle_get_event));

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
    State(state): State<AppState>,
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
            operation: String::from("http_request"),
        };
        json_api_error(StatusCode::GATEWAY_TIMEOUT, err, &correlation_id)
    }
}

async fn handle_healthz(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let correlation_id = match extract_correlation_id(&headers) {
        Ok(id) => id,
        Err(err) => {
            return json_api_error(StatusCode::BAD_REQUEST, err, &CorrelationId::generate());
        }
    };

    let result = match &state.service {
        Some(svc) => {
            svc.health_check(state.profile_mode, correlation_id.clone())
                .await
        }
        None => Err(unconfigured_pg_error()),
    };

    match result {
        Ok(health) => json_with_correlation(StatusCode::OK, &health, &correlation_id),
        Err(err) => {
            let body = HealthFailureBody {
                health: ServiceHealth {
                    service: ServiceKind::Memoryd,
                    stage: String::from(MEMORYD_STAGE_LABEL),
                    profile_mode: state.profile_mode,
                    memory_fabric_implemented: true,
                    postgres_connected: false,
                    postgres_role: None,
                    postgres_database: None,
                    schema_version: None,
                    capabilities: d1_memoryd_capabilities(),
                    correlation_id: correlation_id.clone(),
                },
                error: err,
            };
            json_with_correlation(StatusCode::SERVICE_UNAVAILABLE, &body, &correlation_id)
        }
    }
}

async fn handle_shutdown(
    State(state): State<AppState>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    // Drain bounded body cleanly before replying so Windows Winsock avoids TCP RST (H0-11).
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

async fn handle_ingest_evidence(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<IngestEvidenceRequest>, JsonRejection>,
) -> Response {
    let (correlation_id, principal, svc) = match authorize_and_get_service(&state, &headers) {
        Ok(tuple) => tuple,
        Err(resp) => return *resp,
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &correlation_id),
    };

    match svc.ingest_evidence(&principal, &correlation_id, req).await {
        Ok(resp) => {
            let status = if resp.idempotent_replay {
                StatusCode::OK
            } else {
                StatusCode::CREATED
            };
            json_with_correlation(status, &resp, &correlation_id)
        }
        Err(err) => domain_error_response(err, &correlation_id),
    }
}

async fn handle_get_evidence(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
) -> Response {
    let (correlation_id, principal, svc) = match authorize_and_get_service(&state, &headers) {
        Ok(tuple) => tuple,
        Err(resp) => return *resp,
    };
    let evidence_id = match EvidenceId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &correlation_id),
    };

    match svc.get_evidence(&principal, evidence_id).await {
        Ok(evidence) => json_with_correlation(StatusCode::OK, &evidence, &correlation_id),
        Err(err) => domain_error_response(err, &correlation_id),
    }
}

async fn handle_create_note(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<CreateNoteRequest>, JsonRejection>,
) -> Response {
    let (correlation_id, principal, svc) = match authorize_and_get_service(&state, &headers) {
        Ok(tuple) => tuple,
        Err(resp) => return *resp,
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &correlation_id),
    };

    match svc.create_note(&principal, &correlation_id, req).await {
        Ok(resp) => {
            let status = if resp.idempotent_replay {
                StatusCode::OK
            } else {
                StatusCode::CREATED
            };
            json_with_correlation(status, &resp, &correlation_id)
        }
        Err(err) => domain_error_response(err, &correlation_id),
    }
}

async fn handle_get_note_current(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
) -> Response {
    let (correlation_id, principal, svc) = match authorize_and_get_service(&state, &headers) {
        Ok(tuple) => tuple,
        Err(resp) => return *resp,
    };
    let note_id = match MemoryNoteId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &correlation_id),
    };

    match svc.get_note_current(&principal, note_id).await {
        Ok(note) => json_with_correlation(StatusCode::OK, &note, &correlation_id),
        Err(err) => domain_error_response(err, &correlation_id),
    }
}

async fn handle_get_note_history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
) -> Response {
    let (correlation_id, principal, svc) = match authorize_and_get_service(&state, &headers) {
        Ok(tuple) => tuple,
        Err(resp) => return *resp,
    };
    let note_id = match MemoryNoteId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &correlation_id),
    };

    match svc.get_note_history(&principal, note_id).await {
        Ok(history) => json_with_correlation(StatusCode::OK, &history, &correlation_id),
        Err(err) => domain_error_response(err, &correlation_id),
    }
}

async fn handle_correct_note(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
    payload: Result<Json<CorrectNoteRequest>, JsonRejection>,
) -> Response {
    let (correlation_id, principal, svc) = match authorize_and_get_service(&state, &headers) {
        Ok(tuple) => tuple,
        Err(resp) => return *resp,
    };
    let path_note_id = match MemoryNoteId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &correlation_id),
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &correlation_id),
    };
    if req.note_id != path_note_id {
        let err = DennettError::InvalidInput {
            field: String::from("note_id"),
            reason: String::from("path note_id does not match request body note_id"),
        };
        return domain_error_response(err, &correlation_id);
    }

    match svc.correct_note(&principal, &correlation_id, req).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &correlation_id),
        Err(err) => domain_error_response(err, &correlation_id),
    }
}

async fn handle_search_notes(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<SearchNotesRequest>, JsonRejection>,
) -> Response {
    let (correlation_id, principal, svc) = match authorize_and_get_service(&state, &headers) {
        Ok(tuple) => tuple,
        Err(resp) => return *resp,
    };
    let Json(req) = match payload {
        Ok(j) => j,
        Err(rej) => return json_rejection_response(&rej, &correlation_id),
    };

    match svc.search_notes(&principal, req).await {
        Ok(resp) => json_with_correlation(StatusCode::OK, &resp, &correlation_id),
        Err(err) => domain_error_response(err, &correlation_id),
    }
}

async fn handle_get_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
) -> Response {
    let (correlation_id, principal, svc) = match authorize_and_get_service(&state, &headers) {
        Ok(tuple) => tuple,
        Err(resp) => return *resp,
    };
    let event_id = match MemoryEventId::from_str(&raw_id) {
        Ok(id) => id,
        Err(err) => return domain_error_response(err, &correlation_id),
    };

    match svc.get_event(&principal, event_id).await {
        Ok(event) => json_with_correlation(StatusCode::OK, &event, &correlation_id),
        Err(err) => domain_error_response(err, &correlation_id),
    }
}

async fn handle_not_found(headers: HeaderMap) -> Response {
    let correlation_id = extract_correlation_id_lossy(&headers);
    let err = DennettError::NotFound {
        resource: String::from("http.path"),
        id: String::from("unknown"),
    };
    json_api_error(StatusCode::NOT_FOUND, err, &correlation_id)
}

fn authorize_and_get_service(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(CorrelationId, AuthPrincipal, MemoryService), Box<Response>> {
    let correlation_id = match extract_correlation_id(headers) {
        Ok(id) => id,
        Err(err) => {
            return Err(Box::new(json_api_error(
                StatusCode::BAD_REQUEST,
                err,
                &CorrelationId::generate(),
            )));
        }
    };

    let raw_auth = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            Box::new(json_api_error(
                StatusCode::UNAUTHORIZED,
                DennettError::Unauthorized {
                    reason: String::from("missing `Authorization: Bearer <token>` header"),
                },
                &correlation_id,
            ))
        })?;

    let token = parse_bearer_header(raw_auth)
        .map_err(|err| Box::new(domain_error_response(err, &correlation_id)))?;

    let principal = state
        .auth
        .verify_bearer(token, TimestampMs::now())
        .map_err(|err| Box::new(domain_error_response(err, &correlation_id)))?;

    let svc = state.service.clone().ok_or_else(|| {
        Box::new(domain_error_response(
            unconfigured_pg_error(),
            &correlation_id,
        ))
    })?;

    Ok((correlation_id, principal, svc))
}

fn extract_correlation_id(headers: &HeaderMap) -> Result<CorrelationId, DennettError> {
    let Some(raw) = headers.get(&HEADER_CORRELATION_ID) else {
        return Ok(CorrelationId::generate());
    };
    let text = raw.to_str().map_err(|_| DennettError::InvalidInput {
        field: String::from("X-Correlation-Id"),
        reason: String::from("header must be valid ASCII"),
    })?;
    CorrelationId::try_new(text)
}

fn extract_correlation_id_lossy(headers: &HeaderMap) -> CorrelationId {
    extract_correlation_id(headers).unwrap_or_else(|_| CorrelationId::generate())
}

fn json_rejection_response(rej: &JsonRejection, correlation_id: &CorrelationId) -> Response {
    let status = match rej {
        JsonRejection::BytesRejection(_) => StatusCode::PAYLOAD_TOO_LARGE,
        _ => StatusCode::BAD_REQUEST,
    };
    let safe_reason = redact_secrets(&rej.body_text(), &[]);
    let err = DennettError::InvalidInput {
        field: String::from("body"),
        reason: safe_reason,
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
        DennettError::DatabaseUnavailable { .. } | DennettError::Cancelled { .. } => {
            StatusCode::SERVICE_UNAVAILABLE
        }
        DennettError::Timeout { .. } => StatusCode::GATEWAY_TIMEOUT,
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
    payload: &T,
    correlation_id: &CorrelationId,
) -> Response {
    let mut resp = (status, Json(payload)).into_response();
    if let Ok(val) = HeaderValue::from_str(correlation_id.as_str()) {
        resp.headers_mut()
            .insert(HEADER_CORRELATION_ID.clone(), val);
    }
    resp
}

fn is_loopback_host(host_header: &str) -> bool {
    let trimmed = host_header.trim();
    if trimmed.is_empty() {
        return false;
    }
    let host_only = if let Some(after_bracket) = trimmed.strip_prefix('[') {
        after_bracket.split(']').next().unwrap_or_default()
    } else {
        trimmed.split(':').next().unwrap_or_default()
    };
    host_only.eq_ignore_ascii_case("localhost") || host_only == "127.0.0.1" || host_only == "::1"
}

fn unconfigured_pg_error() -> DennettError {
    DennettError::DatabaseUnavailable {
        detail: String::from(
            "PostgreSQL URL is not configured (set --pg-url, DENNETT_TEST_MEMORY_PG_URL in test_isolated, or DENNETT_MEMORY_PG_URL in production)",
        ),
    }
}

fn emit_stderr_error(err: &DennettError) {
    match serde_json::to_string(err) {
        Ok(json) => eprintln!("{json}"),
        Err(_) => eprintln!("{{\"kind\":\"internal\",\"code\":\"{}\"}}", err.code()),
    }
}

fn print_usage() {
    println!(
        "dennett-memoryd ({MEMORYD_STAGE_LABEL} — standalone Memory Fabric daemon on PostgreSQL)\n\
         Usage:\n\
         \x20 dennett-memoryd --health-once [--pg-url <URL>]\n\
         \x20 dennett-memoryd --serve <IP:PORT> [--allow-non-loopback] [--pg-url <URL>] [--auth-config <FILE>]\n\
         Env:\n\
         \x20 DENNETT_TEST_MEMORY_PG_URL (required in test_isolated unless --pg-url given; requires *_test DB)\n\
         \x20 DENNETT_MEMORY_PG_URL (production profile only)\n\
         \x20 DENNETT_MEMORY_AUTH_CONFIG_FILE (optional path to standalone auth JSON)\n\
         \x20 DENNETT_MEMORY_AUTH_CONFIG (optional inline standalone auth JSON; fail-closed if omitted)"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_args_rejects_non_loopback_unless_explicitly_allowed() {
        let denied = parse_args(["--serve", "0.0.0.0:18080"].into_iter().map(String::from));
        assert!(matches!(
            denied,
            Err(DennettError::SafetyGuardRejected { .. })
        ));

        let allowed = parse_args(
            ["--serve", "0.0.0.0:18080", "--allow-non-loopback"]
                .into_iter()
                .map(String::from),
        );
        assert!(allowed.is_ok(), "explicit --allow-non-loopback should work");
    }

    #[test]
    fn parse_args_redacts_secret_and_handles_multibyte_escape_without_panic() {
        let err = parse_args(
            ["--pg-url=postgres://u:SECRET_ARG_99@127.0.0.1/db"]
                .into_iter()
                .map(String::from),
        );
        let Err(DennettError::InvalidInput { reason, .. }) = err else {
            panic!("expected InvalidInput error");
        };
        assert!(
            !reason.contains("SECRET_ARG_99"),
            "CLI arg secret leaked into error: {reason}"
        );

        // H0-01 regression: `--serve "password='\\я'"` must return clean InvalidInput, never panic.
        let utf8_err = parse_args(["--serve", "password='\\я'"].into_iter().map(String::from));
        assert!(matches!(utf8_err, Err(DennettError::InvalidInput { .. })));
    }

    #[test]
    fn h0_02_test_isolated_forbids_prod_fallback_and_blocks_dsn_override_exploits() {
        let no_fallback = resolve_pg_target_from_sources(
            None,
            None,
            Some("postgres://u:p@127.0.0.1:1/dennett_memory_test"),
            ProfileMode::TestIsolated,
        );
        assert!(
            matches!(no_fallback, Ok(None)),
            "TestIsolated must not fall back to DENNETT_MEMORY_PG_URL"
        );

        let uri_override = ValidatedPgTarget::parse(
            "postgres://u:p@127.0.0.1:1/fake_test?dbname=production_db",
            ProfileMode::TestIsolated,
        );
        assert!(matches!(
            uri_override,
            Err(DennettError::SafetyGuardRejected { .. })
        ));

        let kv_override = ValidatedPgTarget::parse(
            "host=127.0.0.1 port=1 user=u password=p dbname=fake_test dbname=production_db",
            ProfileMode::TestIsolated,
        );
        assert!(matches!(
            kv_override,
            Err(DennettError::SafetyGuardRejected { .. })
        ));

        let valid = resolve_pg_target_from_sources(
            None,
            Some("postgres://u:p@127.0.0.1:55432/dennett_memory_test"),
            Some("postgres://u:p@127.0.0.1:5432/production_db"),
            ProfileMode::TestIsolated,
        );
        assert!(matches!(valid, Ok(Some(_))));
    }

    #[test]
    fn loopback_host_validation_matches_exact_loopback_hosts() {
        assert!(is_loopback_host("127.0.0.1:8080"));
        assert!(is_loopback_host("[::1]:8080"));
        assert!(is_loopback_host("localhost:8080"));
        assert!(!is_loopback_host("evil.example:8080"));
        assert!(!is_loopback_host(""));
    }
}
