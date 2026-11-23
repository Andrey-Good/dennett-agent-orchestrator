//! Minimal stdio JSON-RPC 2.0 MCP fixture server for Dennett D0 memory-tool probes.
//!
//! Exposes a single tool `memory_probe_read(key, scope?)`:
//! - Reads `DENNETT_PROBE_MEMORY_TOKEN` and `DENNETT_PROBE_ALLOWED_SCOPE` from environment.
//! - Returns the configured token when the requested scope matches the allowed scope.
//! - Returns `isError: true` (`PERMISSION_DENIED`) for any foreign scope.
//! - Returns `isError: true` (`INVALID_INPUT`) if `scope` is present but not a non-empty string.
//! - Enforces a 64 KiB maximum line length on stdin to prevent unbounded memory growth.

use std::env;
use std::io::{self, BufRead, Write};

use serde_json::{Value, json};

const DEFAULT_PROTOCOL_VERSION: &str = "2024-11-05";
const DEFAULT_ALLOWED_SCOPE: &str = "project-alpha";
const MAX_MCP_STDIN_LINE_BYTES: usize = 64 * 1024;

enum BoundedRead {
    Eof,
    Line(String),
    TooLarge,
}

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut out = io::BufWriter::new(stdout.lock());

    loop {
        let line = match read_bounded_line(&mut reader, MAX_MCP_STDIN_LINE_BYTES) {
            Ok(BoundedRead::Eof) | Err(_) => break,
            Ok(BoundedRead::TooLarge) => {
                let response = json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": {
                        "code": -32600,
                        "message": format!("request line exceeded maximum size of {MAX_MCP_STDIN_LINE_BYTES} bytes")
                    }
                });
                if write_response(&mut out, &response).is_err() {
                    break;
                }
                continue;
            }
            Ok(BoundedRead::Line(s)) => s,
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
                        "message": format!("parse error: {err}")
                    }
                });
                if write_response(&mut out, &response).is_err() {
                    break;
                }
                continue;
            }
        };

        let Some(response) = handle_message(&msg) else {
            continue;
        };
        if write_response(&mut out, &response).is_err() {
            break;
        }
    }
}

fn read_bounded_line(reader: &mut impl BufRead, max_bytes: usize) -> io::Result<BoundedRead> {
    let mut raw = Vec::new();
    let mut exceeded = false;

    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            if exceeded {
                return Ok(BoundedRead::TooLarge);
            }
            if raw.is_empty() {
                return Ok(BoundedRead::Eof);
            }
            break;
        }

        let newline_pos = available.iter().position(|&b| b == b'\n');
        let take_len = newline_pos.map_or(available.len(), |pos| pos + 1);

        if !exceeded {
            if raw.len() + take_len > max_bytes {
                exceeded = true;
                raw.clear();
            } else {
                raw.extend_from_slice(&available[..take_len]);
            }
        }
        reader.consume(take_len);

        if newline_pos.is_some() {
            if exceeded {
                return Ok(BoundedRead::TooLarge);
            }
            break;
        }
    }

    let text =
        String::from_utf8(raw).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    Ok(BoundedRead::Line(text))
}

fn handle_message(msg: &Value) -> Option<Value> {
    let method = msg.get("method").and_then(Value::as_str)?;
    let id = msg.get("id").cloned()?;
    let params = msg.get("params").unwrap_or(&Value::Null);

    match method {
        "initialize" => {
            let client_protocol = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_PROTOCOL_VERSION);
            Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": client_protocol,
                    "capabilities": {
                        "tools": {}
                    },
                    "serverInfo": {
                        "name": "dennett-d0-mcp-fixture",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }
            }))
        }
        "ping" => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {}
        })),
        "tools/list" => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "tools": [
                    {
                        "name": "memory_probe_read",
                        "description": "Read a scoped Dennett D0 test memory token by key and optional scope.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "key": {
                                    "type": "string",
                                    "description": "Memory key to look up."
                                },
                                "scope": {
                                    "type": "string",
                                    "description": "Optional memory scope. Foreign scopes are rejected with access denied."
                                }
                            },
                            "required": ["key"]
                        }
                    }
                ]
            }
        })),
        "tools/call" => Some(handle_tool_call(&id, params)),
        _ => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32601,
                "message": format!("method `{method}` not found")
            }
        })),
    }
}

fn handle_tool_call(id: &Value, params: &Value) -> Value {
    let tool_name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if tool_name != "memory_probe_read" {
        return json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32601,
                "message": format!("unknown tool `{tool_name}`")
            }
        });
    }

    let args = params.get("arguments").unwrap_or(&Value::Null);
    let Some(key) = args.get("key").and_then(Value::as_str) else {
        return tool_error(id, "INVALID_INPUT: missing required `key` argument");
    };

    let allowed_scope = env::var("DENNETT_PROBE_ALLOWED_SCOPE")
        .unwrap_or_else(|_| String::from(DEFAULT_ALLOWED_SCOPE));

    let requested_scope = match args.get("scope") {
        None => allowed_scope.as_str(),
        Some(Value::String(s)) if !s.trim().is_empty() => s.as_str(),
        Some(_) => {
            return tool_error(
                id,
                "INVALID_INPUT: `scope` must be a non-empty string when provided",
            );
        }
    };

    if requested_scope != allowed_scope {
        return tool_error(
            id,
            &format!("PERMISSION_DENIED: scope `{requested_scope}` is not permitted"),
        );
    }

    let token = env::var("DENNETT_PROBE_MEMORY_TOKEN").unwrap_or_default();
    if token.is_empty() {
        return tool_error(id, "MISCONFIGURED: DENNETT_PROBE_MEMORY_TOKEN is not set");
    }

    let payload = json!({
        "key": key,
        "scope": requested_scope,
        "token": token
    });

    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "content": [{
                "type": "text",
                "text": payload.to_string()
            }],
            "isError": false
        }
    })
}

fn tool_error(id: &Value, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "content": [{ "type": "text", "text": message }],
            "isError": true
        }
    })
}

fn write_response(out: &mut impl Write, response: &Value) -> io::Result<()> {
    let serialized = serde_json::to_string(response)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    writeln!(out, "{serialized}")?;
    out.flush()
}
