//! Executable stdio JSON-RPC 2.0 MCP server (`dennett-memory-mcp`).

use std::collections::BTreeSet;
use std::env;
use std::io::{self, Write};
use std::process::ExitCode;
use std::str::FromStr;

use dennett_contracts::redact::{redact_secrets, sanitize_cli_arg};
use dennett_contracts::{DennettError, MemoryCapability, MemoryScope, PrincipalId};
use dennett_memory_mcp::{
    BoundedRead, DEFAULT_MEMORY_URL, ENV_MEMORY_CAPABILITIES, ENV_MEMORY_HMAC_SECRET,
    ENV_MEMORY_PRINCIPAL_ID, ENV_MEMORY_SCOPES, ENV_MEMORY_TOKEN, ENV_MEMORY_URL, HmacTokenMinter,
    MAX_MCP_STDIN_LINE_BYTES, MemoryMcpServer, read_bounded_line,
};
use serde_json::{Value, json};

#[derive(Debug, Default, PartialEq, Eq)]
struct CliOptions {
    show_help: bool,
    memory_url: Option<String>,
    memory_hmac_secret: Option<String>,
}

fn main() -> ExitCode {
    let cli = match parse_cli_args(env::args().skip(1)) {
        Ok(opts) => opts,
        Err(err) => {
            emit_error(&err, &[]);
            return ExitCode::from(2);
        }
    };

    if cli.show_help {
        print_usage();
        return ExitCode::SUCCESS;
    }

    let server = match build_server_from_env(&cli) {
        Ok(s) => s,
        Err(code) => return code,
    };

    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(err) => {
            eprintln!("failed to initialize async runtime: {err}");
            return ExitCode::from(1);
        }
    };

    run_stdio_loop(&rt, &server);
    ExitCode::SUCCESS
}

fn parse_cli_args<I>(args: I) -> Result<CliOptions, DennettError>
where
    I: IntoIterator<Item = String>,
{
    let mut opts = CliOptions::default();
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        if let Some(val) = arg.strip_prefix("--memory-hmac-secret=") {
            opts.memory_hmac_secret = Some(val.to_owned());
            continue;
        }
        if let Some(val) = arg.strip_prefix("--memory-url=") {
            opts.memory_url = Some(val.to_owned());
            continue;
        }
        match arg.as_str() {
            "-h" | "--help" => {
                opts.show_help = true;
                return Ok(opts);
            }
            "--memory-hmac-secret" => {
                let raw = iter.next().ok_or_else(|| DennettError::InvalidInput {
                    field: String::from("--memory-hmac-secret"),
                    reason: String::from("expected HMAC secret value after --memory-hmac-secret"),
                })?;
                opts.memory_hmac_secret = Some(raw);
            }
            "--memory-url" => {
                let raw = iter.next().ok_or_else(|| DennettError::InvalidInput {
                    field: String::from("--memory-url"),
                    reason: String::from("expected memoryd URL after --memory-url"),
                })?;
                opts.memory_url = Some(raw);
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

    Ok(opts)
}

fn build_server_from_env(cli: &CliOptions) -> Result<MemoryMcpServer, ExitCode> {
    let env_url = env::var(ENV_MEMORY_URL).ok();
    let env_token = env::var(ENV_MEMORY_TOKEN).ok();
    let env_hmac = env::var(ENV_MEMORY_HMAC_SECRET).ok();
    let env_scopes = env::var(ENV_MEMORY_SCOPES).ok();
    let env_principal = env::var(ENV_MEMORY_PRINCIPAL_ID).ok();
    let env_caps = env::var(ENV_MEMORY_CAPABILITIES).ok();

    let mut secrets: Vec<String> = Vec::new();
    for raw in [
        env_token.as_deref(),
        env_hmac.as_deref(),
        cli.memory_hmac_secret.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if !raw.is_empty() {
            secrets.push(raw.to_owned());
        }
        let trimmed = raw.trim();
        if !trimmed.is_empty() && trimmed != raw {
            secrets.push(trimmed.to_owned());
        }
    }

    build_server_from_sources(
        cli.memory_url.as_deref(),
        env_url.as_deref(),
        env_token.as_deref(),
        cli.memory_hmac_secret.as_deref(),
        env_hmac.as_deref(),
        env_scopes.as_deref(),
        env_principal.as_deref(),
        env_caps.as_deref(),
        secrets.clone(),
    )
    .map_err(|err| {
        let refs: Vec<&str> = secrets.iter().map(String::as_str).collect();
        emit_error(&err, &refs);
        ExitCode::from(2)
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "pure configuration resolver takes explicit CLI and env slices for deterministic unit testing"
)]
fn build_server_from_sources(
    cli_url: Option<&str>,
    env_url: Option<&str>,
    env_token: Option<&str>,
    cli_hmac_secret: Option<&str>,
    env_hmac_secret: Option<&str>,
    env_scopes: Option<&str>,
    env_principal: Option<&str>,
    env_caps: Option<&str>,
    extra_secrets: Vec<String>,
) -> Result<MemoryMcpServer, DennettError> {
    let url = non_empty(cli_url)
        .or_else(|| non_empty(env_url))
        .unwrap_or(DEFAULT_MEMORY_URL);

    let token = non_empty(env_token).map(ToOwned::to_owned);
    // Support `DENNETT_MEMORY_HMAC_SECRET` env var cleanly alongside `--memory-hmac-secret` CLI flag.
    let hmac_secret = non_empty(cli_hmac_secret)
        .or_else(|| non_empty(env_hmac_secret))
        .map(ToOwned::to_owned);

    let hmac_minter = match hmac_secret {
        Some(secret) => {
            let scopes = parse_csv_scopes(env_scopes)?;
            let principal = non_empty(env_principal)
                .map(PrincipalId::try_new)
                .transpose()?;
            let caps = parse_csv_capabilities(env_caps)?;
            Some(HmacTokenMinter::new(secret, scopes, principal, caps)?)
        }
        None => None,
    };

    MemoryMcpServer::with_auth(url, token, hmac_minter, extra_secrets)
}

fn non_empty(val: Option<&str>) -> Option<&str> {
    val.map(str::trim).filter(|s| !s.is_empty())
}

fn parse_csv_scopes(raw: Option<&str>) -> Result<BTreeSet<MemoryScope>, DennettError> {
    let mut scopes = BTreeSet::new();
    let Some(csv) = non_empty(raw) else {
        return Ok(scopes);
    };
    for part in csv.split(',') {
        let trimmed = part.trim();
        if !trimmed.is_empty() {
            scopes.insert(MemoryScope::try_new(trimmed)?);
        }
    }
    Ok(scopes)
}

fn parse_csv_capabilities(
    raw: Option<&str>,
) -> Result<Option<BTreeSet<MemoryCapability>>, DennettError> {
    let Some(csv) = non_empty(raw) else {
        return Ok(None);
    };
    let mut caps = BTreeSet::new();
    for part in csv.split(',') {
        let trimmed = part.trim();
        if !trimmed.is_empty() {
            caps.insert(MemoryCapability::from_str(trimmed)?);
        }
    }
    Ok(Some(caps))
}

fn emit_error(err: &DennettError, secrets: &[&str]) {
    let safe_msg = redact_secrets(&err.to_string(), secrets);
    let payload = json!({
        "code": err.code(),
        "message": safe_msg,
    });
    eprintln!("{payload}");
}

fn print_usage() {
    println!(
        "dennett-memory-mcp (stdio JSON-RPC 2.0 MCP adapter for standalone dennett-memoryd)\n\
         Usage:\n\
         \x20 dennett-memory-mcp [--memory-url <URL>] [--memory-hmac-secret <SECRET>]\n\
         Env:\n\
         \x20 {ENV_MEMORY_URL} (default {DEFAULT_MEMORY_URL})\n\
         \x20 {ENV_MEMORY_TOKEN} (scoped bearer token)\n\
         \x20 {ENV_MEMORY_HMAC_SECRET} (preferred over --memory-hmac-secret to keep secrets off argv)\n\
         \x20 {ENV_MEMORY_SCOPES} (optional comma-separated scope allowlist for HMAC tokens)\n\
         \x20 {ENV_MEMORY_PRINCIPAL_ID} (optional principal ID for HMAC tokens)\n\
         \x20 {ENV_MEMORY_CAPABILITIES} (optional comma-separated capabilities for HMAC tokens)"
    );
}

fn run_stdio_loop(rt: &tokio::runtime::Runtime, server: &MemoryMcpServer) {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut out = io::BufWriter::new(stdout.lock());

    loop {
        let line = match read_bounded_line(&mut reader, MAX_MCP_STDIN_LINE_BYTES) {
            Ok(BoundedRead::Eof) => break,
            Ok(BoundedRead::TooLarge) => {
                let response = json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": {
                        "code": -32600,
                        "message": format!(
                            "request line exceeded maximum size of {MAX_MCP_STDIN_LINE_BYTES} bytes"
                        )
                    }
                });
                if write_response(&mut out, &response).is_err() {
                    break;
                }
                continue;
            }
            Ok(BoundedRead::Line(s)) => s,
            Err(_) => {
                let response = json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": {
                        "code": -32700,
                        "message": "parse error: invalid UTF-8 or unreadable stdin stream"
                    }
                });
                let _ = write_response(&mut out, &response);
                break;
            }
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let msg: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(err) => {
                let response = json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": {
                        "code": -32700,
                        "message": format!("parse error: {}", server.redact_text(&err.to_string()))
                    }
                });
                if write_response(&mut out, &response).is_err() {
                    break;
                }
                continue;
            }
        };

        let Some(response) = rt.block_on(server.handle_message(&msg)) else {
            continue;
        };
        if write_response(&mut out, &response).is_err() {
            break;
        }
    }
}

fn write_response(out: &mut impl Write, response: &Value) -> io::Result<()> {
    let serialized = serde_json::to_string(response)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    writeln!(out, "{serialized}")?;
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_and_env_resolution_supports_hmac_secret_without_argv_and_redacts_secrets() {
        let opts = parse_cli_args(
            [
                "--memory-url",
                "http://127.0.0.1:18081",
                "--memory-hmac-secret",
                "cli-secret-0123456789abcdef",
            ]
            .into_iter()
            .map(String::from),
        )
        .expect("parse valid cli args");
        assert_eq!(opts.memory_url.as_deref(), Some("http://127.0.0.1:18081"));
        assert_eq!(
            opts.memory_hmac_secret.as_deref(),
            Some("cli-secret-0123456789abcdef")
        );

        let eq_opts = parse_cli_args(
            ["--memory-hmac-secret=eq-secret-0123456789abcdef"]
                .into_iter()
                .map(String::from),
        )
        .expect("parse = form");
        assert_eq!(
            eq_opts.memory_hmac_secret.as_deref(),
            Some("eq-secret-0123456789abcdef")
        );

        // Env-only DENNETT_MEMORY_HMAC_SECRET works cleanly without CLI flag.
        let env_secret = "env-hmac-secret-0123456789abcdef";
        let server = build_server_from_sources(
            None,
            Some("http://127.0.0.1:18080"),
            None,
            None,
            Some(env_secret),
            Some("scope-alpha,scope-beta"),
            Some("agent:mcp-env"),
            Some("read,write"),
            vec![env_secret.to_owned()],
        )
        .expect("build server from DENNETT_MEMORY_HMAC_SECRET env");
        let redacted = server.redact_text(&format!("secret leaked: {env_secret}"));
        assert!(!redacted.contains(env_secret));

        // Short HMAC secret fails closed.
        let too_short = build_server_from_sources(
            None,
            Some("http://127.0.0.1:18080"),
            None,
            None,
            Some("short"),
            None,
            None,
            None,
            vec![String::from("short")],
        );
        assert!(matches!(too_short, Err(DennettError::InvalidInput { .. })));
    }
}
