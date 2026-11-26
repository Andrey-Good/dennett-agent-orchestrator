//! Executable `dennett-head` control daemon for Dennett (D1+).
//!
//! Serves versioned `/v1/*` and `/healthz` endpoints over `axum`, owns `dennett_control`
//! PostgreSQL migrations, invokes `dennett-memoryd` strictly through `MemoryClient` HTTP API,
//! and binds either the production Google Antigravity (`agy acp`) runtime or the explicitly
//! marked CI `SimulatedRuntime`.

use std::env;
use std::io::{self, Write};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use dennett_contracts::redact::{redact_secrets, sanitize_cli_arg, sanitize_pg_error};
use dennett_contracts::{CorrelationId, DennettError, ProfileMode, RuntimeAuthProfile};
use dennett_head::{
    HEAD_STAGE_LABEL, HeadAppState, HeadService, HeadServiceExtras, MAX_CONCURRENT_REQUESTS,
    MemoryMcpWiring, MemoryTokenSigner, ValidatedControlPgTarget, build_head_router,
    write_owner_only_file,
};
use dennett_memory_client::MemoryClient;
use dennett_runtime_antigravity::{
    AgentRuntimePort, AntigravityAcpRuntime, AntigravityRuntimeConfig, SimulatedRuntime,
};
use tokio::net::TcpListener;
use tokio::sync::{Semaphore, watch};
use zeroize::Zeroizing;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeKind {
    Antigravity,
    Simulated,
}

#[derive(Debug, PartialEq, Eq)]
struct ServeOptions {
    addr: SocketAddr,
    pg_url: Option<String>,
    memory_url: Option<String>,
    memory_mcp_bin: Option<PathBuf>,
    control_mcp_bin: Option<PathBuf>,
    memory_hmac_secret: Option<String>,
    runtime_kind: RuntimeKind,
    auth_profile: RuntimeAuthProfile,
    profile_root: Option<PathBuf>,
    effects_ledger: Option<PathBuf>,
    enable_sandbox: bool,
    allow_non_loopback: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum Command {
    HealthOnce {
        pg_url: Option<String>,
        memory_url: Option<String>,
        runtime_kind: RuntimeKind,
        auth_profile: RuntimeAuthProfile,
        profile_root: Option<PathBuf>,
    },
    Serve(ServeOptions),
    Help,
    Version,
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
                "dennett-head {} ({HEAD_STAGE_LABEL})",
                env!("CARGO_PKG_VERSION")
            );
            ExitCode::SUCCESS
        }
        Command::HealthOnce {
            pg_url,
            memory_url,
            runtime_kind,
            auth_profile,
            profile_root,
        } => {
            run_health_once(
                pg_url,
                memory_url,
                runtime_kind,
                auth_profile,
                profile_root,
                profile_mode,
            )
            .await
        }
        Command::Serve(opts) => run_serve(opts, profile_mode).await,
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "CLI argument parser handles all Head flags in one transparent match loop"
)]
fn parse_args<I>(args: I) -> Result<Command, DennettError>
where
    I: IntoIterator<Item = String>,
{
    let mut iter = args.into_iter();
    let mut health_once = false;
    let mut serve_addr: Option<String> = None;
    let mut pg_url: Option<String> = None;
    let mut memory_url: Option<String> = None;
    let mut memory_mcp_bin: Option<PathBuf> = None;
    let mut control_mcp_bin: Option<PathBuf> = None;
    let mut memory_hmac_secret: Option<String> = None;
    let mut runtime_kind = RuntimeKind::Antigravity;
    let mut auth_profile = RuntimeAuthProfile::OAuthPersonal;
    let mut profile_root: Option<PathBuf> = None;
    let mut effects_ledger: Option<PathBuf> = None;
    let mut enable_sandbox = false;
    let mut allow_non_loopback = false;

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "-V" | "--version" => return Ok(Command::Version),
            "--health-once" => health_once = true,
            "--sandbox" => enable_sandbox = true,
            "--allow-non-loopback" => allow_non_loopback = true,
            "--serve" => {
                serve_addr = Some(next_flag_value(&mut iter, "--serve")?);
            }
            "--pg-url" => {
                pg_url = Some(next_flag_value(&mut iter, "--pg-url")?);
            }
            "--memory-url" => {
                memory_url = Some(next_flag_value(&mut iter, "--memory-url")?);
            }
            "--memory-mcp-bin" => {
                memory_mcp_bin = Some(PathBuf::from(next_flag_value(
                    &mut iter,
                    "--memory-mcp-bin",
                )?));
            }
            "--control-mcp-bin" => {
                control_mcp_bin = Some(PathBuf::from(next_flag_value(
                    &mut iter,
                    "--control-mcp-bin",
                )?));
            }
            "--memory-hmac-secret" => {
                memory_hmac_secret = Some(next_flag_value(&mut iter, "--memory-hmac-secret")?);
            }
            "--profile-root" => {
                profile_root = Some(PathBuf::from(next_flag_value(&mut iter, "--profile-root")?));
            }
            "--effects-ledger" => {
                effects_ledger = Some(PathBuf::from(next_flag_value(
                    &mut iter,
                    "--effects-ledger",
                )?));
            }
            "--runtime" => {
                let raw = next_flag_value(&mut iter, "--runtime")?;
                runtime_kind = match raw.trim().to_ascii_lowercase().as_str() {
                    "antigravity" | "antigravity-acp" => RuntimeKind::Antigravity,
                    "simulated" => RuntimeKind::Simulated,
                    other => {
                        return Err(DennettError::InvalidInput {
                            field: String::from("--runtime"),
                            reason: format!(
                                "unsupported runtime `{other}` (expected antigravity|simulated)"
                            ),
                        });
                    }
                };
            }
            "--auth-profile" => {
                let raw = next_flag_value(&mut iter, "--auth-profile")?;
                auth_profile = match raw.trim().to_ascii_lowercase().as_str() {
                    "oauth-personal" => RuntimeAuthProfile::OAuthPersonal,
                    "gemini-api-key" => RuntimeAuthProfile::GeminiApiKey,
                    other => {
                        return Err(DennettError::InvalidInput {
                            field: String::from("--auth-profile"),
                            reason: format!(
                                "unsupported auth profile `{other}` (expected oauth-personal|gemini-api-key)"
                            ),
                        });
                    }
                };
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
        (true, None) => Ok(Command::HealthOnce {
            pg_url,
            memory_url,
            runtime_kind,
            auth_profile,
            profile_root,
        }),
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
            Ok(Command::Serve(ServeOptions {
                addr,
                pg_url,
                memory_url,
                memory_mcp_bin,
                control_mcp_bin,
                memory_hmac_secret,
                runtime_kind,
                auth_profile,
                profile_root,
                effects_ledger,
                enable_sandbox,
                allow_non_loopback,
            }))
        }
        (false, None) => Err(DennettError::InvalidInput {
            field: String::from("cli"),
            reason: String::from("specify either --health-once or --serve <addr>"),
        }),
    }
}

fn next_flag_value<I>(iter: &mut I, flag: &str) -> Result<String, DennettError>
where
    I: Iterator<Item = String>,
{
    iter.next().ok_or_else(|| DennettError::InvalidInput {
        field: flag.to_owned(),
        reason: format!("expected value after {flag}"),
    })
}

fn active_profile_mode() -> ProfileMode {
    match env::var("DENNETT_TEST_PROFILE").ok().as_deref() {
        Some("1") => ProfileMode::TestIsolated,
        _ => ProfileMode::Production,
    }
}

fn resolve_control_pg_target(
    cli_override: Option<&str>,
    profile_mode: ProfileMode,
) -> Result<Option<ValidatedControlPgTarget>, DennettError> {
    let prod_env = env::var("DENNETT_CONTROL_PG_URL").ok();
    let test_env = env::var("DENNETT_TEST_CONTROL_PG_URL").ok();
    ValidatedControlPgTarget::resolve_from_sources(
        cli_override,
        test_env.as_deref(),
        prod_env.as_deref(),
        profile_mode,
    )
}

struct MemoryComponents {
    client: Option<MemoryClient>,
    mcp: Option<MemoryMcpWiring>,
    hmac_signer: Option<MemoryTokenSigner>,
    secrets: Vec<String>,
}

fn build_memory_components(
    cli_memory_url: Option<&str>,
    cli_mcp_bin: Option<&PathBuf>,
    cli_hmac_secret: Option<&str>,
) -> Result<MemoryComponents, DennettError> {
    let env_url = env::var("DENNETT_MEMORY_URL").ok();
    let memory_url = cli_memory_url
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            env_url
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToOwned::to_owned)
        });

    let memory_token = env::var("DENNETT_MEMORY_TOKEN")
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());

    let env_hmac = env::var("DENNETT_MEMORY_HMAC_SECRET").ok();
    let hmac_secret = cli_hmac_secret
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            env_hmac
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToOwned::to_owned)
        });

    let mut secrets = Vec::new();
    if let Some(ref tok) = memory_token {
        secrets.push(tok.clone());
    }
    let hmac_signer = match hmac_secret {
        Some(secret) => {
            secrets.push(secret.clone());
            Some(MemoryTokenSigner::new(secret.into_bytes())?)
        }
        None => None,
    };

    let client = match (&memory_url, &memory_token) {
        (Some(url), Some(tok)) => Some(MemoryClient::new(url, tok)?),
        (Some(url), None) => Some(MemoryClient::unauthenticated(url)?),
        (None, _) => None,
    };

    let mcp_bin = cli_mcp_bin
        .cloned()
        .or_else(|| {
            env::var_os("DENNETT_MEMORY_MCP_BIN")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        })
        .or_else(find_sibling_memory_mcp_bin);

    let mcp_wiring = match (mcp_bin, memory_url, memory_token, hmac_signer.is_some()) {
        (Some(binary_path), Some(url), Some(token), _) => Some(MemoryMcpWiring {
            binary_path,
            memory_url: url,
            memory_token: token,
        }),
        (Some(binary_path), Some(url), None, true) => Some(MemoryMcpWiring {
            binary_path,
            memory_url: url,
            memory_token: String::new(),
        }),
        _ => None,
    };

    Ok(MemoryComponents {
        client,
        mcp: mcp_wiring,
        hmac_signer,
        secrets,
    })
}

fn find_sibling_memory_mcp_bin() -> Option<PathBuf> {
    find_sibling_bin("dennett-memory-mcp")
}

fn find_sibling_control_mcp_bin() -> Option<PathBuf> {
    find_sibling_bin("dennett-control-mcp")
}

fn find_sibling_bin(base_name: &str) -> Option<PathBuf> {
    let exe_name = if cfg!(windows) {
        format!("{base_name}.exe")
    } else {
        base_name.to_owned()
    };
    let current = env::current_exe().ok()?;
    let candidate = current.parent()?.join(exe_name);
    candidate.is_file().then_some(candidate)
}

fn build_runtime(
    runtime_kind: RuntimeKind,
    auth_profile: RuntimeAuthProfile,
    profile_root: Option<PathBuf>,
    enable_sandbox: bool,
) -> (Arc<dyn AgentRuntimePort>, Vec<String>) {
    match runtime_kind {
        RuntimeKind::Simulated => (Arc::new(SimulatedRuntime::new(auth_profile)), Vec::new()),
        RuntimeKind::Antigravity => {
            let root = profile_root.unwrap_or_else(|| {
                env::temp_dir().join(format!("dennett-head-acp-{}", std::process::id()))
            });
            let mut secrets = Vec::new();
            let mut cfg = match auth_profile {
                RuntimeAuthProfile::GeminiApiKey => {
                    let api_key = env::var("DENNETT_GEMINI_API_KEY")
                        .ok()
                        .or_else(|| env::var("GEMINI_API_KEY").ok())
                        .map(|s| s.trim().to_owned())
                        .filter(|s| !s.is_empty());
                    if let Some(ref k) = api_key {
                        secrets.push(k.clone());
                    }
                    AntigravityRuntimeConfig::gemini_api_key(root, api_key)
                }
                _ => AntigravityRuntimeConfig::oauth_personal(root),
            };
            cfg.enable_sandbox = enable_sandbox;
            (Arc::new(AntigravityAcpRuntime::new(cfg)), secrets)
        }
    }
}

async fn run_health_once(
    pg_url: Option<String>,
    memory_url: Option<String>,
    runtime_kind: RuntimeKind,
    auth_profile: RuntimeAuthProfile,
    profile_root: Option<PathBuf>,
    profile_mode: ProfileMode,
) -> ExitCode {
    let correlation_id = CorrelationId::generate();
    let target = match resolve_control_pg_target(pg_url.as_deref(), profile_mode) {
        Ok(Some(t)) => t,
        Ok(None) => {
            emit_stderr_error(&DennettError::DatabaseUnavailable {
                detail: String::from("control PostgreSQL connection URL is not configured"),
            });
            return ExitCode::from(1);
        }
        Err(err) => {
            emit_stderr_error(&err);
            return ExitCode::from(1);
        }
    };

    let MemoryComponents {
        client: memory_client,
        mcp: memory_mcp,
        hmac_signer: memory_hmac_signer,
        mut secrets,
    } = match build_memory_components(memory_url.as_deref(), None, None) {
        Ok(parts) => parts,
        Err(err) => {
            emit_stderr_error(&err);
            return ExitCode::from(1);
        }
    };
    let detached_projects_root = profile_root.clone();
    let (runtime, rt_secrets) = build_runtime(runtime_kind, auth_profile, profile_root, false);
    secrets.extend(rt_secrets);

    let service = match HeadService::new_with_extras(
        target,
        runtime,
        memory_client,
        memory_mcp,
        profile_mode,
        secrets,
        HeadServiceExtras {
            memory_hmac_signer,
            detached_projects_root,
            ..HeadServiceExtras::default()
        },
    ) {
        Ok(s) => s,
        Err(err) => {
            emit_stderr_error(&err);
            return ExitCode::from(1);
        }
    };

    match service.health_check(correlation_id).await {
        Ok(health) => match serde_json::to_string(&health) {
            Ok(json) => {
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(err) => {
                emit_stderr_error(&DennettError::Internal {
                    detail: format!("failed serializing head health: {err}"),
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

#[expect(
    clippy::too_many_lines,
    reason = "server bootstrap wires PostgreSQL, memoryd, runtime, listener, and graceful shutdown cleanly in one flow"
)]
async fn run_serve(opts: ServeOptions, profile_mode: ProfileMode) -> ExitCode {
    let target = match resolve_control_pg_target(opts.pg_url.as_deref(), profile_mode) {
        Ok(t) => t,
        Err(err) => {
            emit_stderr_error(&err);
            return ExitCode::from(1);
        }
    };

    let MemoryComponents {
        client: memory_client,
        mcp: memory_mcp,
        hmac_signer: memory_hmac_signer,
        mut secrets,
    } = match build_memory_components(
        opts.memory_url.as_deref(),
        opts.memory_mcp_bin.as_ref(),
        opts.memory_hmac_secret.as_deref(),
    ) {
        Ok(parts) => parts,
        Err(err) => {
            emit_stderr_error(&err);
            return ExitCode::from(1);
        }
    };

    let detached_projects_root = opts.profile_root.clone();
    let effects_ledger_path = opts
        .effects_ledger
        .or_else(|| {
            env::var_os("DENNETT_EFFECTS_LEDGER_PATH")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        })
        .or_else(|| {
            opts.profile_root
                .as_ref()
                .map(|root| root.join("sim-effects-ledger.json"))
        });
    // H2-06 / A10 / Contract M: when serving interactive `oauth-personal` with Antigravity,
    // also wire an isolated `gemini-api-key` background runtime in a separate profile subdirectory
    // (strictly fail-closed with `AUTH_REQUIRED` if no API key is configured; zero fallback!).
    let background_runtime = if opts.runtime_kind == RuntimeKind::Antigravity
        && opts.auth_profile == RuntimeAuthProfile::OAuthPersonal
    {
        let bg_profile_root = opts
            .profile_root
            .as_ref()
            .map(|root| root.join("bg-gemini-api-key"))
            .or_else(|| {
                Some(env::temp_dir().join(format!("dennett-head-acp-bg-{}", std::process::id())))
            });
        let (bg_rt, bg_secrets) = build_runtime(
            RuntimeKind::Antigravity,
            RuntimeAuthProfile::GeminiApiKey,
            bg_profile_root,
            opts.enable_sandbox,
        );
        secrets.extend(bg_secrets);
        Some(bg_rt)
    } else {
        None
    };
    let antigravity_profile_root = opts.profile_root.clone();
    let token_data_dir = opts.profile_root.clone().unwrap_or_else(default_head_data_dir);
    let env_head_token = env::var("DENNETT_HEAD_TOKEN")
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());
    let (runtime, rt_secrets) = build_runtime(
        opts.runtime_kind,
        opts.auth_profile,
        opts.profile_root,
        opts.enable_sandbox,
    );
    secrets.extend(rt_secrets);
    let runtime_adapter_id = runtime.adapter_id().to_owned();
    let runtime_is_simulated = runtime.is_simulated();

    let control_mcp_bin = opts
        .control_mcp_bin
        .or_else(|| {
            env::var_os("DENNETT_CONTROL_MCP_BIN")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        })
        .or_else(find_sibling_control_mcp_bin);

    let service = match target {
        Some(pg_target) => match HeadService::new_with_extras(
            pg_target,
            runtime,
            memory_client,
            memory_mcp,
            profile_mode,
            secrets,
            HeadServiceExtras {
                memory_hmac_signer,
                control_mcp_bin,
                control_hmac_signer: None,
                detached_projects_root,
                background_runtime,
                effect_port: None,
                effects_ledger_path,
                antigravity_profile_root,
                antigravity_enable_sandbox: opts.enable_sandbox,
                owner_token_override: env_head_token.clone(),
                allow_non_loopback_local_runtime: opts.allow_non_loopback,
                ..HeadServiceExtras::default()
            },
        ) {
            Ok(svc) => {
                match svc.ensure_migrated().await {
                    Ok(_) => {
                        let startup_cid = CorrelationId::generate();
                        if let Err(err) = svc.recover_after_startup_once(&startup_cid).await {
                            emit_stderr_error(&err);
                        }
                    }
                    Err(err @ DennettError::SafetyGuardRejected { .. }) => {
                        emit_stderr_error(&err);
                        return ExitCode::from(1);
                    }
                    Err(err) => emit_stderr_error(&err),
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

    // A-SEC-1: Resolve mandatory owner bearer token; if DENNETT_HEAD_TOKEN was not supplied via
    // environment, persist the generated 256-bit token into `<data_dir>/head.token` with strict
    // owner-only filesystem permissions (never logging the plaintext token!).
    let owner_token = if let Some(ref svc) = service {
        Zeroizing::new(svc.owner_token().to_owned())
    } else if let Some(ref tok) = env_head_token {
        Zeroizing::new(tok.clone())
    } else {
        use chacha20poly1305::aead::OsRng;
        use chacha20poly1305::{ChaCha20Poly1305, KeyInit};
        use core::fmt::Write as _;
        let key = ChaCha20Poly1305::generate_key(&mut OsRng);
        let mut hex = String::with_capacity(64);
        for b in key {
            let _ = write!(&mut hex, "{b:02x}");
        }
        Zeroizing::new(hex)
    };
    if env_head_token.is_none() {
        let token_path = token_data_dir.join("head.token");
        if let Err(err) = write_owner_only_file(&token_path, owner_token.as_bytes()) {
            emit_stderr_error(&err);
            return ExitCode::from(1);
        }
    }

    let listener = match TcpListener::bind(opts.addr).await {
        Ok(l) => l,
        Err(err) => {
            emit_stderr_error(&DennettError::Internal {
                detail: format!(
                    "failed binding dennett-head listener on {}: {err}",
                    opts.addr
                ),
            });
            return ExitCode::from(1);
        }
    };
    let bound_addr = match listener.local_addr() {
        Ok(a) => a,
        Err(err) => {
            emit_stderr_error(&DennettError::Internal {
                detail: format!("failed reading bound listener address: {err}"),
            });
            return ExitCode::from(1);
        }
    };
    if let Some(ref svc) = service {
        svc.set_self_http_url(format!("http://{bound_addr}"));
    }

    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
    let ctrl_c_tx = shutdown_tx.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            let _ = ctrl_c_tx.send(true);
        }
    });

    let scheduler_handle = service
        .as_ref()
        .map(|svc| svc.spawn_background_scheduler(shutdown_rx.clone()));

    let state = HeadAppState {
        service,
        profile_mode,
        runtime_adapter_id,
        runtime_is_simulated,
        allow_non_loopback: opts.allow_non_loopback,
        owner_token,
        shutdown_tx,
        request_limit: Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS)),
    };
    let app = build_head_router(state);

    if writeln!(io::stdout(), "LISTENING {bound_addr}").is_err() || io::stdout().flush().is_err() {
        emit_stderr_error(&DennettError::Internal {
            detail: String::from("failed flushing LISTENING startup line"),
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

    if let Some(handle) = scheduler_handle {
        let _ = handle.await;
    }

    if let Err(err) = serve_result {
        emit_stderr_error(&DennettError::Internal {
            detail: format!("Head HTTP server error: {err}"),
        });
        return ExitCode::from(1);
    }

    ExitCode::SUCCESS
}

fn default_head_data_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(local) = env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty()) {
            return PathBuf::from(local).join("dennett");
        }
    }
    if let Some(xdg) = env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(xdg).join("dennett");
    }
    if let Some(home) = env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .filter(|v| !v.is_empty())
    {
        return PathBuf::from(home).join(".local").join("share").join("dennett");
    }
    env::temp_dir().join("dennett-head")
}

fn emit_stderr_error(err: &DennettError) {
    let safe_msg = redact_secrets(&err.to_string(), &[]);
    let payload = serde_json::json!({
        "code": err.code(),
        "message": safe_msg,
    });
    eprintln!("{payload}");
}

fn print_usage() {
    println!(
        "dennett-head {} ({HEAD_STAGE_LABEL})\n\
         \n\
         USAGE:\n\
           dennett-head --serve <ip:port> [OPTIONS]\n\
           dennett-head --health-once [OPTIONS]\n\
         \n\
         OPTIONS:\n\
           --serve <ip:port>                   Bind HTTP server (loopback only by default)\n\
           --allow-non-loopback                Explicitly permit non-loopback listener address\n\
           --health-once                       Run one PostgreSQL + memoryd health probe and exit\n\
           --pg-url <dsn>                      Override control PostgreSQL DSN\n\
           --memory-url <url>                  Base URL of standalone dennett-memoryd\n\
           --memory-mcp-bin <path>             Path to dennett-memory-mcp binary\n\
           --control-mcp-bin <path>            Path to dennett-control-mcp binary\n\
           --memory-hmac-secret <secret>       HMAC-SHA256 secret for minting per-project scoped memoryd tokens\n\
           --runtime <antigravity|simulated>   Runtime adapter (default: antigravity)\n\
           --auth-profile <oauth-personal|gemini-api-key>\n\
                                               Runtime auth profile (default: oauth-personal)\n\
           --profile-root <dir>                Isolated directory for agy acp profiles\n\
           --effects-ledger <path>             JSON ledger path for simulated external connector persistence\n\
           --sandbox                           Enable agy acp --sandbox terminal restrictions\n\
           -h, --help                          Show this help text\n\
           -V, --version                       Show version\n\
         \n\
         ENVIRONMENT:\n\
           DENNETT_CONTROL_PG_URL              Production control PostgreSQL DSN\n\
           DENNETT_TEST_CONTROL_PG_URL         Isolated *_test control PostgreSQL DSN\n\
           DENNETT_TEST_PROFILE=1              Enable test_isolated mode (blocks prod PG fallback)\n\
           DENNETT_MEMORY_URL                  Standalone dennett-memoryd HTTP URL\n\
           DENNETT_MEMORY_TOKEN                Scoped bearer token for memoryd and dennett-memory-mcp\n\
           DENNETT_MEMORY_HMAC_SECRET          Shared HMAC-SHA256 secret for minting dynamic project tokens\n\
           DENNETT_EFFECTS_LEDGER_PATH         Optional JSON ledger file path for simulated external connector\n\
           DENNETT_GEMINI_API_KEY              API key used ONLY when --auth-profile gemini-api-key",
        env!("CARGO_PKG_VERSION")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_rejects_non_loopback_without_opt_in_and_parses_runtime_modes() {
        let non_loopback = parse_args(["--serve", "0.0.0.0:18081"].map(String::from));
        assert!(matches!(
            non_loopback,
            Err(DennettError::SafetyGuardRejected { .. })
        ));

        let sim = parse_args(
            [
                "--serve",
                "127.0.0.1:0",
                "--runtime",
                "simulated",
                "--auth-profile",
                "gemini-api-key",
            ]
            .map(String::from),
        )
        .expect("valid serve args");
        match sim {
            Command::Serve(opts) => {
                assert_eq!(opts.runtime_kind, RuntimeKind::Simulated);
                assert_eq!(opts.auth_profile, RuntimeAuthProfile::GeminiApiKey);
            }
            other => panic!("expected Serve, got {other:?}"),
        }
    }
}
