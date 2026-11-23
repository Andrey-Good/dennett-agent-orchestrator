//! Executable stdio JSON-RPC 2.0 MCP server (`dennett-memory-mcp`).

use std::env;
use std::io::{self, Write};
use std::process::ExitCode;

use dennett_contracts::redact::redact_secrets;
use dennett_memory_client::MemoryClient;
use dennett_memory_mcp::{
    BoundedRead, DEFAULT_MEMORY_URL, ENV_MEMORY_TOKEN, ENV_MEMORY_URL, MAX_MCP_STDIN_LINE_BYTES,
    MemoryMcpServer, read_bounded_line,
};
use serde_json::{Value, json};

fn main() -> ExitCode {
    let client = match build_client_from_env() {
        Ok(c) => c,
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

    let server = MemoryMcpServer::new(client);
    run_stdio_loop(&rt, &server);
    ExitCode::SUCCESS
}

fn build_client_from_env() -> Result<MemoryClient, ExitCode> {
    let url = env::var(ENV_MEMORY_URL)
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| String::from(DEFAULT_MEMORY_URL));

    let raw_token = env::var(ENV_MEMORY_TOKEN).ok();
    let token = raw_token
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned);

    let client = match token.as_deref() {
        Some(tok) => MemoryClient::new(&url, tok),
        None => MemoryClient::unauthenticated(&url),
    };

    client.map_err(|err| {
        let mut secrets: Vec<&str> = Vec::new();
        if let Some(ref raw) = raw_token
            && !raw.is_empty()
        {
            secrets.push(raw.as_str());
        }
        if let Some(ref trimmed) = token {
            secrets.push(trimmed.as_str());
        }
        let safe_msg = redact_secrets(&err.to_string(), &secrets);
        let payload = json!({
            "code": err.code(),
            "message": safe_msg,
        });
        eprintln!("{payload}");
        ExitCode::from(2)
    })
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
