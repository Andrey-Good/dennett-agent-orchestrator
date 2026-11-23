//! Standalone `dennett-memoryd` process scaffold for Dennett milestone D0.
//!
//! Runs independently of any Head or UI component, probes real PostgreSQL
//! connectivity, reports `memory_fabric_implemented: false` honestly, and strips
//! database credentials from every diagnostic response.

use std::env;
use std::io::{self, Write};
use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use dennett_contracts::redact::{
    sanitize_cli_arg, sanitize_pg_error, sanitize_pg_error_with_password,
};
use dennett_contracts::{CorrelationId, DennettError, ProfileMode, ServiceHealth, ServiceKind};
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Semaphore, watch};
use tokio::task::JoinHandle;
use tokio::time;
use tokio_postgres::{Config as PgConfig, NoTls};

/// Maximum incoming HTTP header block size accepted by the D0 health endpoint.
const MAX_HTTP_REQUEST_BYTES: usize = 16 * 1024;
/// Maximum HTTP request body bytes drained before replying so Windows Winsock avoids `TCP RST`.
const MAX_HTTP_DRAIN_BODY_BYTES: usize = 64 * 1024;
/// Maximum concurrent HTTP connections served simultaneously by the D0 scaffold.
const MAX_CONCURRENT_CONNECTIONS: usize = 32;
/// Timeout applied to reading an HTTP header block from a connected client.
const HTTP_READ_TIMEOUT: Duration = Duration::from_secs(5);
/// Short timeout applied while draining an incoming request body before replying.
const HTTP_DRAIN_TIMEOUT: Duration = Duration::from_secs(1);
/// Timeout applied to writing an HTTP response back to a client.
const HTTP_WRITE_TIMEOUT: Duration = Duration::from_secs(5);
/// Timeout applied to establishing and running a PostgreSQL health query.
const PG_PROBE_TIMEOUT: Duration = Duration::from_secs(3);
/// Current milestone identifier reported in [`ServiceHealth`].
const STAGE_LABEL: &str = "D0";

/// RAII guard that aborts a spawned background task on early return, timeout, or panic.
struct AbortOnDrop(JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Command {
    HealthOnce {
        pg_url: Option<String>,
    },
    Serve {
        addr: SocketAddr,
        pg_url: Option<String>,
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

#[derive(Debug)]
struct HttpHead<'a> {
    method: &'a str,
    path: &'a str,
    host: Option<&'a str>,
    shutdown_header_valid: bool,
    content_length: usize,
    correlation_id: CorrelationId,
}

/// Validated PostgreSQL configuration parsed once by `tokio_postgres::Config` to avoid
/// parser differentials between validation and connection establishment.
#[derive(Clone)]
struct ValidatedPgTarget {
    raw_url: String,
    config: PgConfig,
}

impl ValidatedPgTarget {
    fn parse(raw_url: &str, profile_mode: ProfileMode) -> Result<Self, DennettError> {
        let config: PgConfig = raw_url.parse().map_err(|err| {
            let safe = sanitize_pg_error(&format!("{err}"), Some(raw_url));
            if profile_mode == ProfileMode::TestIsolated {
                DennettError::SafetyGuardRejected {
                    reason: format!("invalid PostgreSQL connection string: {safe}"),
                }
            } else {
                DennettError::DatabaseUnavailable {
                    detail: format!("invalid PostgreSQL connection string: {safe}"),
                }
            }
        })?;

        if profile_mode == ProfileMode::TestIsolated {
            let Some(db_name) = config.get_dbname() else {
                return Err(DennettError::SafetyGuardRejected {
                    reason: String::from(
                        "test_isolated profile requires an explicit `*_test` PostgreSQL database",
                    ),
                });
            };
            if db_name.len() <= "_test".len() || !db_name.ends_with("_test") {
                return Err(DennettError::SafetyGuardRejected {
                    reason: format!(
                        "test_isolated profile requires a `*_test` PostgreSQL database, got `{db_name}`"
                    ),
                });
            }
        }

        Ok(Self {
            raw_url: raw_url.to_owned(),
            config,
        })
    }

    fn parsed_password(&self) -> Option<&str> {
        self.config
            .get_password()
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
    }
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
                "dennett-memoryd {} ({STAGE_LABEL})",
                env!("CARGO_PKG_VERSION")
            );
            ExitCode::SUCCESS
        }
        Command::HealthOnce { pg_url } => run_health_once(pg_url, profile_mode).await,
        Command::Serve { addr, pg_url } => run_serve(addr, pg_url, profile_mode).await,
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
            Ok(Command::Serve { addr, pg_url })
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

    match probe_postgres(target.as_ref(), profile_mode, correlation_id).await {
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
    profile_mode: ProfileMode,
) -> ExitCode {
    let target = match resolve_pg_target(cli_pg_url.as_deref(), profile_mode) {
        Ok(t) => t,
        Err(err) => {
            emit_stderr_error(&err);
            return ExitCode::from(1);
        }
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

    if writeln!(io::stdout(), "LISTENING {bound_addr}").is_err() || io::stdout().flush().is_err() {
        emit_stderr_error(&DennettError::Internal {
            detail: String::from("failed to flush listening announcement to stdout"),
        });
        return ExitCode::from(1);
    }

    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
    let conn_limit = Arc::new(Semaphore::new(MAX_CONCURRENT_CONNECTIONS));

    let ctrl_c_tx = shutdown_tx.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            let _ = ctrl_c_tx.send(true);
        }
    });

    loop {
        tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() {
                    break;
                }
            }
            accepted = listener.accept() => {
                let (mut stream, peer_addr) = match accepted {
                    Ok(pair) => pair,
                    Err(err) => {
                        emit_stderr_error(&DennettError::Internal {
                            detail: format!("accept failed: {err}"),
                        });
                        continue;
                    }
                };
                let Ok(permit) = Arc::clone(&conn_limit).try_acquire_owned() else {
                    let busy = DennettError::Internal {
                        detail: String::from("connection limit reached"),
                    };
                    let _ = write_http_json(&mut stream, "503 Service Unavailable", &busy).await;
                    continue;
                };
                let target_clone = target.clone();
                let tx_clone = shutdown_tx.clone();
                tokio::spawn(async move {
                    let _permit = permit;
                    handle_connection(
                        stream,
                        peer_addr,
                        target_clone.as_ref(),
                        profile_mode,
                        tx_clone,
                    )
                    .await;
                });
            }
        }
    }

    ExitCode::SUCCESS
}

async fn handle_connection(
    mut stream: TcpStream,
    peer_addr: SocketAddr,
    pg_target: Option<&ValidatedPgTarget>,
    profile_mode: ProfileMode,
    shutdown_tx: watch::Sender<bool>,
) {
    let Ok((header_bytes, already_read_body_len)) = read_http_header_block(&mut stream).await
    else {
        let _ = write_http_json(
            &mut stream,
            "400 Bad Request",
            &DennettError::InvalidInput {
                field: String::from("http"),
                reason: String::from("malformed or timed-out HTTP request"),
            },
        )
        .await;
        return;
    };

    let Ok(header_text) = std::str::from_utf8(&header_bytes) else {
        let _ = write_http_json(
            &mut stream,
            "400 Bad Request",
            &DennettError::InvalidInput {
                field: String::from("http"),
                reason: String::from("HTTP request header is not valid UTF-8"),
            },
        )
        .await;
        return;
    };

    let Some(head) = parse_http_head(header_text) else {
        let _ = write_http_json(
            &mut stream,
            "400 Bad Request",
            &DennettError::InvalidInput {
                field: String::from("http"),
                reason: String::from("missing HTTP method or path"),
            },
        )
        .await;
        return;
    };

    // Drain any bounded incoming request body before replying/closing to prevent Winsock TCP RST.
    let remaining_body = head
        .content_length
        .saturating_sub(already_read_body_len)
        .min(MAX_HTTP_DRAIN_BODY_BYTES);
    if remaining_body > 0 {
        drain_remaining_body(&mut stream, remaining_body).await;
    }

    dispatch_http_request(
        &mut stream,
        head,
        peer_addr,
        pg_target,
        profile_mode,
        shutdown_tx,
    )
    .await;
}

fn parse_http_head(header_block: &str) -> Option<HttpHead<'_>> {
    // Defense-in-depth: never inspect anything after the header/body delimiter.
    let headers_only = header_block
        .split_once("\r\n\r\n")
        .map_or(header_block, |(h, _)| h);

    let mut lines = headers_only.lines();
    let request_line = lines.next()?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?;
    let raw_target = parts.next()?;
    let path = raw_target.split('?').next().unwrap_or(raw_target);

    let mut correlation_id: Option<CorrelationId> = None;
    let mut host: Option<&str> = None;
    let mut shutdown_header_valid = false;
    let mut content_length: usize = 0;

    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let key = name.trim();
        let val = value.trim();
        if key.eq_ignore_ascii_case("x-correlation-id") && correlation_id.is_none() {
            correlation_id = CorrelationId::try_new(val).ok();
        } else if key.eq_ignore_ascii_case("host") && host.is_none() {
            host = Some(val);
        } else if key.eq_ignore_ascii_case("x-dennett-shutdown") && val == "1" {
            shutdown_header_valid = true;
        } else if key.eq_ignore_ascii_case("content-length")
            && let Ok(parsed) = val.parse::<usize>()
        {
            content_length = parsed;
        }
    }

    Some(HttpHead {
        method,
        path,
        host,
        shutdown_header_valid,
        content_length,
        correlation_id: correlation_id.unwrap_or_else(CorrelationId::generate),
    })
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

async fn dispatch_http_request(
    stream: &mut TcpStream,
    head: HttpHead<'_>,
    peer_addr: SocketAddr,
    pg_target: Option<&ValidatedPgTarget>,
    profile_mode: ProfileMode,
    shutdown_tx: watch::Sender<bool>,
) {
    let HttpHead {
        method,
        path,
        host,
        shutdown_header_valid,
        content_length: _,
        correlation_id,
    } = head;

    match (method, path) {
        ("GET", "/healthz") => {
            match probe_postgres(pg_target, profile_mode, correlation_id.clone()).await {
                Ok(health) => {
                    let _ = write_http_json(stream, "200 OK", &health).await;
                }
                Err(err) => {
                    let body = HealthFailureBody {
                        health: ServiceHealth {
                            service: ServiceKind::Memoryd,
                            stage: String::from(STAGE_LABEL),
                            profile_mode,
                            memory_fabric_implemented: false,
                            postgres_connected: false,
                            postgres_role: None,
                            postgres_database: None,
                            capabilities: d0_capabilities(),
                            correlation_id,
                        },
                        error: err,
                    };
                    let _ = write_http_json(stream, "503 Service Unavailable", &body).await;
                }
            }
        }
        ("POST", "/shutdown") => {
            if profile_mode != ProfileMode::TestIsolated {
                let err = DennettError::InvalidInput {
                    field: String::from("http.path"),
                    reason: String::from("unknown path `/shutdown`"),
                };
                let _ = write_http_json(stream, "404 Not Found", &err).await;
                return;
            }
            if !peer_addr.ip().is_loopback()
                || !shutdown_header_valid
                || host.is_some_and(|h| !is_loopback_host(h))
            {
                let err = DennettError::AccessDenied {
                    scope: String::from("/shutdown"),
                    reason: String::from(
                        "shutdown requires test_isolated mode, loopback peer/Host, and `X-Dennett-Shutdown: 1`",
                    ),
                };
                let _ = write_http_json(stream, "403 Forbidden", &err).await;
                return;
            }
            let ack = ShutdownAck {
                status: "shutting_down",
                correlation_id,
            };
            let _ = write_http_json(stream, "200 OK", &ack).await;
            let _ = shutdown_tx.send(true);
        }
        (_, "/healthz") => {
            let err = DennettError::InvalidInput {
                field: String::from("http.method"),
                reason: format!("method `{method}` is not allowed for `/healthz`"),
            };
            let _ = write_http_json(stream, "405 Method Not Allowed", &err).await;
        }
        (_, "/shutdown") if profile_mode == ProfileMode::TestIsolated => {
            let err = DennettError::InvalidInput {
                field: String::from("http.method"),
                reason: format!("method `{method}` is not allowed for `/shutdown`"),
            };
            let _ = write_http_json(stream, "405 Method Not Allowed", &err).await;
        }
        _ => {
            let err = DennettError::InvalidInput {
                field: String::from("http.path"),
                reason: format!("unknown path `{path}`"),
            };
            let _ = write_http_json(stream, "404 Not Found", &err).await;
        }
    }
}

async fn read_http_header_block(stream: &mut TcpStream) -> io::Result<(Vec<u8>, usize)> {
    let mut buf = Vec::with_capacity(512);
    let mut chunk = [0_u8; 1024];

    let read_fut = async {
        loop {
            let n = stream.read(&mut chunk).await?;
            if n == 0 {
                return Ok((buf, 0));
            }
            if buf.len() + n > MAX_HTTP_REQUEST_BYTES {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "HTTP request exceeds maximum size",
                ));
            }
            buf.extend_from_slice(&chunk[..n]);
            if let Some(header_end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                let body_start = header_end + 4;
                let already_read_body = buf.len().saturating_sub(body_start);
                buf.truncate(header_end);
                return Ok((buf, already_read_body));
            }
        }
    };

    time::timeout(HTTP_READ_TIMEOUT, read_fut)
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "HTTP request read timed out"))?
}

async fn drain_remaining_body(stream: &mut TcpStream, mut remaining: usize) {
    let mut sink = [0_u8; 2048];
    let _ = time::timeout(HTTP_DRAIN_TIMEOUT, async {
        while remaining > 0 {
            let to_read = remaining.min(sink.len());
            match stream.read(&mut sink[..to_read]).await {
                Ok(0) | Err(_) => break,
                Ok(n) => remaining = remaining.saturating_sub(n),
            }
        }
    })
    .await;
}

async fn write_http_json<T: Serialize>(
    stream: &mut TcpStream,
    status: &str,
    payload: &T,
) -> io::Result<()> {
    let body = serde_json::to_vec(payload)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let write_fut = async {
        stream.write_all(header.as_bytes()).await?;
        stream.write_all(&body).await?;
        stream.flush().await?;
        let _ = stream.shutdown().await;
        Ok(())
    };
    time::timeout(HTTP_WRITE_TIMEOUT, write_fut)
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "HTTP response write timed out"))?
}

async fn probe_postgres(
    pg_target: Option<&ValidatedPgTarget>,
    profile_mode: ProfileMode,
    correlation_id: CorrelationId,
) -> Result<ServiceHealth, DennettError> {
    let Some(target) = pg_target else {
        return Err(DennettError::DatabaseUnavailable {
            detail: String::from(
                "PostgreSQL URL is not configured (set --pg-url, DENNETT_TEST_MEMORY_PG_URL in test_isolated, or DENNETT_MEMORY_PG_URL in production)",
            ),
        });
    };

    let raw_url = target.raw_url.as_str();
    let parsed_pw = target.parsed_password();

    let probe_fut = async {
        let (client, connection) = target.config.connect(NoTls).await.map_err(|err| {
            DennettError::DatabaseUnavailable {
                detail: sanitize_pg_error_with_password(&err.to_string(), Some(raw_url), parsed_pw),
            }
        })?;

        let _conn_guard = AbortOnDrop(tokio::spawn(async move {
            let _ = connection.await;
        }));

        let row = client
            .query_one(
                "SELECT 1::int4, current_user::text, current_database()::text",
                &[],
            )
            .await
            .map_err(|err| DennettError::DatabaseUnavailable {
                detail: sanitize_pg_error_with_password(&err.to_string(), Some(raw_url), parsed_pw),
            })?;

        let check: i32 = row.get(0);
        let role: String = row.get(1);
        let database: String = row.get(2);
        drop(client);

        if check != 1 {
            return Err(DennettError::DatabaseUnavailable {
                detail: String::from("unexpected SELECT 1 result from PostgreSQL"),
            });
        }

        Ok(ServiceHealth {
            service: ServiceKind::Memoryd,
            stage: String::from(STAGE_LABEL),
            profile_mode,
            memory_fabric_implemented: false,
            postgres_connected: true,
            postgres_role: Some(role),
            postgres_database: Some(database),
            capabilities: d0_capabilities(),
            correlation_id,
        })
    };

    time::timeout(PG_PROBE_TIMEOUT, probe_fut)
        .await
        .map_err(|_| DennettError::DatabaseUnavailable {
            detail: String::from("PostgreSQL health probe timed out"),
        })?
}

fn d0_capabilities() -> Vec<String> {
    vec![
        String::from("healthz"),
        String::from("postgres-connectivity"),
    ]
}

fn emit_stderr_error(err: &DennettError) {
    match serde_json::to_string(err) {
        Ok(json) => eprintln!("{json}"),
        Err(_) => eprintln!("{{\"kind\":\"internal\",\"code\":\"{}\"}}", err.code()),
    }
}

fn print_usage() {
    println!(
        "dennett-memoryd (D0 scaffold — Memory Fabric not yet implemented)\n\
         Usage:\n\
         \x20 dennett-memoryd --health-once [--pg-url <URL>]\n\
         \x20 dennett-memoryd --serve <IP:PORT> [--allow-non-loopback] [--pg-url <URL>]\n\
         Env:\n\
         \x20 DENNETT_TEST_MEMORY_PG_URL (required in test_isolated unless --pg-url given; requires *_test DB)\n\
         \x20 DENNETT_MEMORY_PG_URL (production profile only)"
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
        // TestIsolated must NEVER fall back to DENNETT_MEMORY_PG_URL.
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

        // Exploit 1: URI path looks like `_test`, query `dbname` overrides to production_db.
        let uri_override = ValidatedPgTarget::parse(
            "postgres://u:p@127.0.0.1:1/fake_test?dbname=production_db",
            ProfileMode::TestIsolated,
        );
        assert!(
            matches!(uri_override, Err(DennettError::SafetyGuardRejected { .. })),
            "URI query dbname override must be rejected with SafetyGuardRejected"
        );

        // Exploit 2: duplicate libpq `dbname` keys where second points to production_db.
        let kv_override = ValidatedPgTarget::parse(
            "host=127.0.0.1 port=1 user=u password=p dbname=fake_test dbname=production_db",
            ProfileMode::TestIsolated,
        );
        assert!(
            matches!(kv_override, Err(DennettError::SafetyGuardRejected { .. })),
            "libpq duplicate dbname override must be rejected with SafetyGuardRejected"
        );

        // Valid isolated `_test` DSN passes cleanly.
        let valid = resolve_pg_target_from_sources(
            None,
            Some("postgres://u:p@127.0.0.1:55432/dennett_memory_test"),
            Some("postgres://u:p@127.0.0.1:5432/production_db"),
            ProfileMode::TestIsolated,
        );
        assert!(matches!(valid, Ok(Some(_))));
    }

    #[test]
    fn parse_http_head_ignores_body_headers_and_parses_content_length() {
        let raw = "POST /shutdown HTTP/1.1\r\nHost: 127.0.0.1:8080\r\nContent-Length: 4096\r\n\r\nX-Dennett-Shutdown: 1\r\nX-Correlation-Id: from-body";
        let Some(head) = parse_http_head(raw) else {
            panic!("expected valid HTTP head");
        };
        assert_eq!(head.content_length, 4096);
        assert!(
            !head.shutdown_header_valid,
            "shutdown header inside body must never be honored"
        );
        assert_ne!(
            head.correlation_id.as_str(),
            "from-body",
            "correlation id inside body must never be honored"
        );
        assert!(is_loopback_host("127.0.0.1:8080"));
        assert!(is_loopback_host("[::1]:8080"));
        assert!(is_loopback_host("localhost:8080"));
        assert!(!is_loopback_host("evil.example:8080"));
    }
}
