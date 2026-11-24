//! Executable stdio JSON-RPC 2.0 MCP server (`dennett-control-mcp`).

use std::env;
use std::io;
use std::process::ExitCode;
use std::str::FromStr;

use dennett_contracts::redact::redact_secrets;
use dennett_contracts::{AgentId, DennettError, ProjectId, SessionId};
use dennett_control_mcp::{
    BoundedFrame, ControlMcpServer, ENV_CONTROL_AGENT_ID, ENV_CONTROL_PROJECT_ID,
    ENV_CONTROL_SESSION_ID, ENV_CONTROL_TOKEN, ENV_HEAD_URL, FrameMode, MAX_MCP_FRAME_BYTES,
    read_mcp_frame, write_mcp_frame,
};
use dennett_head_client::HeadClient;
use serde_json::{Value, json};

fn main() -> ExitCode {
    let server = match build_server_from_env() {
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

fn build_server_from_env() -> Result<ControlMcpServer, ExitCode> {
    let raw_token = env::var(ENV_CONTROL_TOKEN)
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());
    let mut secrets = Vec::new();
    if let Some(ref tok) = raw_token {
        secrets.push(tok.clone());
    }

    let build_res = (|| -> Result<ControlMcpServer, DennettError> {
        let head_url = read_required_env(ENV_HEAD_URL)?;
        let project_id = ProjectId::from_str(&read_required_env(ENV_CONTROL_PROJECT_ID)?)?;
        let agent_id = AgentId::from_str(&read_required_env(ENV_CONTROL_AGENT_ID)?)?;
        let session_id = SessionId::from_str(&read_required_env(ENV_CONTROL_SESSION_ID)?)?;
        let token = raw_token.ok_or_else(|| DennettError::Unauthorized {
            reason: format!("missing required environment variable `{ENV_CONTROL_TOKEN}`"),
        })?;
        let client = HeadClient::new(&head_url)?.with_control_token(token);
        Ok(ControlMcpServer::new(
            client,
            project_id,
            agent_id,
            session_id,
            secrets.clone(),
        ))
    })();

    build_res.map_err(|err| {
        let secret_refs: Vec<&str> = secrets.iter().map(String::as_str).collect();
        let safe_msg = redact_secrets(&err.to_string(), &secret_refs);
        let payload = json!({
            "code": err.code(),
            "message": safe_msg,
        });
        eprintln!("{payload}");
        ExitCode::from(2)
    })
}

fn read_required_env(name: &str) -> Result<String, DennettError> {
    env::var(name)
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| DennettError::InvalidInput {
            field: name.to_owned(),
            reason: format!("missing required environment variable `{name}`"),
        })
}

fn run_stdio_loop(rt: &tokio::runtime::Runtime, server: &ControlMcpServer) {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut out = io::BufWriter::new(stdout.lock());

    loop {
        let (payload, mode) = match read_mcp_frame(&mut reader, MAX_MCP_FRAME_BYTES) {
            Ok(BoundedFrame::Eof) => break,
            Ok(BoundedFrame::TooLarge(mode)) => {
                let response = json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": {
                        "code": -32600,
                        "message": format!(
                            "request exceeded maximum size of {MAX_MCP_FRAME_BYTES} bytes"
                        )
                    }
                });
                if write_mcp_frame(&mut out, &response, mode).is_err() {
                    break;
                }
                continue;
            }
            Ok(BoundedFrame::Message(s, mode)) => (s, mode),
            Err(_) => {
                let response = json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": {
                        "code": -32700,
                        "message": "parse error: invalid UTF-8 or unreadable stdin stream"
                    }
                });
                let _ = write_mcp_frame(&mut out, &response, FrameMode::Ndjson);
                break;
            }
        };

        let msg: Value = match serde_json::from_str(&payload) {
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
                if write_mcp_frame(&mut out, &response, mode).is_err() {
                    break;
                }
                continue;
            }
        };

        let Some(response) = rt.block_on(server.handle_message(&msg)) else {
            continue;
        };
        if write_mcp_frame(&mut out, &response, mode).is_err() {
            break;
        }
    }
}
