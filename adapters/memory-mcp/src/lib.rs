//! Thin stdio JSON-RPC 2.0 MCP adapter for standalone `dennett-memoryd`.
//!
//! Architectural boundary (Architecture 80 §6, 82 §7, V06):
//! - Depends ONLY on `dennett-contracts` and `dennett-memory-client` (NEVER on
//!   `dennett-memory-core` or PostgreSQL drivers).
//! - Delegates every memory operation to `dennett-memoryd` via [`MemoryClient`] using
//!   a scoped bearer token read from environment.
//! - Enforces a strict maximum stdin line size ([`MAX_MCP_STDIN_LINE_BYTES`]) to bound memory.

use std::io::{self, BufRead};
use std::str::FromStr;

use dennett_contracts::redact::redact_secrets;
use dennett_contracts::{
    CorrectNoteRequest, CreateNoteRequest, DennettError, EvidenceLinkInput, IdempotencyKey,
    MemoryNoteId, MemoryScope, SearchNotesRequest,
};
use dennett_memory_client::MemoryClient;
use serde::Deserialize;
use serde_json::{Value, json};

/// Default MCP protocol version advertised when a client omits `protocolVersion`.
pub const DEFAULT_PROTOCOL_VERSION: &str = "2024-11-05";
/// Maximum permitted byte length of a single JSON-RPC line on stdin (256 KiB).
pub const MAX_MCP_STDIN_LINE_BYTES: usize = 256 * 1024;
/// Default `dennett-memoryd` URL used when `DENNETT_MEMORY_URL` is unset.
pub const DEFAULT_MEMORY_URL: &str = "http://127.0.0.1:18080";
/// Environment variable providing the `dennett-memoryd` base URL.
pub const ENV_MEMORY_URL: &str = "DENNETT_MEMORY_URL";
/// Environment variable providing the scoped bearer credential for `dennett-memoryd`.
pub const ENV_MEMORY_TOKEN: &str = "DENNETT_MEMORY_TOKEN";

/// Result of reading one bounded newline-delimited frame from an input stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundedRead {
    /// Clean end-of-file before any bytes of a new frame arrived.
    Eof,
    /// Complete valid UTF-8 frame within the configured byte limit.
    Line(String),
    /// Frame exceeded `max_bytes` and was safely drained up to the next newline (or EOF).
    TooLarge,
}

/// Reads one newline-delimited UTF-8 line without allocating beyond `max_bytes`.
///
/// When a line exceeds `max_bytes`, all remaining bytes up to and including `\n` are
/// discarded so subsequent lines remain aligned.
///
/// # Errors
///
/// Returns `io::Error` on underlying I/O failure or invalid UTF-8 bytes within the limit.
pub fn read_bounded_line(reader: &mut impl BufRead, max_bytes: usize) -> io::Result<BoundedRead> {
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

/// Parsed and validated MCP tool invocation ready to dispatch via [`MemoryClient`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedToolCall {
    /// Scoped search over current memory notes (`memory_search`).
    Search(SearchNotesRequest),
    /// Fetch current state of a memory note (`memory_get_current`).
    GetCurrent(MemoryNoteId),
    /// Fetch complete immutable revision history of a note (`memory_get_history`).
    GetHistory(MemoryNoteId),
    /// Create a revision-1 memory note (`memory_write_note`).
    WriteNote(CreateNoteRequest),
    /// Append an immutable correction revision to a note (`memory_correct_note`).
    CorrectNote(CorrectNoteRequest),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchArgs {
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NoteIdArgs {
    note_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteNoteArgs {
    scope: String,
    title: String,
    body: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    evidence_links: Vec<EvidenceLinkInput>,
    #[serde(default)]
    author_label: Option<String>,
    #[serde(default)]
    idempotency_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CorrectNoteArgs {
    note_id: String,
    scope: String,
    expected_revision: i64,
    title: String,
    body: String,
    rationale: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    evidence_links: Vec<EvidenceLinkInput>,
    #[serde(default)]
    author_label: Option<String>,
    #[serde(default)]
    idempotency_key: Option<String>,
}

/// Parses and validates an MCP tool name + JSON arguments object into a typed [`ParsedToolCall`].
///
/// # Errors
///
/// Returns `Ok(None)` if `tool_name` is unknown (so caller can emit JSON-RPC `-32601`),
/// or `Err(DennettError::InvalidInput)` if arguments fail validation.
pub fn parse_tool_call(
    tool_name: &str,
    raw_args: &Value,
) -> Result<Option<ParsedToolCall>, DennettError> {
    let args_obj = match raw_args {
        Value::Object(_) => raw_args.clone(),
        Value::Null if tool_name == "memory_search" => json!({}),
        _ => {
            return Err(DennettError::InvalidInput {
                field: String::from("arguments"),
                reason: String::from("tool arguments must be a JSON object"),
            });
        }
    };

    match tool_name {
        "memory_search" => {
            let parsed: SearchArgs = deserialize_args(args_obj)?;
            let scope = parsed.scope.map(MemoryScope::try_new).transpose()?;
            let req = SearchNotesRequest {
                scope,
                query: parsed.query,
                limit: parsed.limit,
            };
            let _ = req.validated_limit()?;
            Ok(Some(ParsedToolCall::Search(req)))
        }
        "memory_get_current" => {
            let parsed: NoteIdArgs = deserialize_args(args_obj)?;
            let note_id = MemoryNoteId::from_str(&parsed.note_id)?;
            Ok(Some(ParsedToolCall::GetCurrent(note_id)))
        }
        "memory_get_history" => {
            let parsed: NoteIdArgs = deserialize_args(args_obj)?;
            let note_id = MemoryNoteId::from_str(&parsed.note_id)?;
            Ok(Some(ParsedToolCall::GetHistory(note_id)))
        }
        "memory_write_note" => {
            let parsed: WriteNoteArgs = deserialize_args(args_obj)?;
            let scope = MemoryScope::try_new(parsed.scope)?;
            let idempotency_key = match parsed.idempotency_key {
                Some(raw) => IdempotencyKey::try_new(raw)?,
                None => IdempotencyKey::generate(),
            };
            let req = CreateNoteRequest {
                scope,
                title: parsed.title,
                body: parsed.body,
                tags: parsed.tags,
                observed_at: None,
                evidence_links: parsed.evidence_links,
                author_label: parsed.author_label,
                idempotency_key,
            };
            req.validate()?;
            Ok(Some(ParsedToolCall::WriteNote(req)))
        }
        "memory_correct_note" => {
            let parsed: CorrectNoteArgs = deserialize_args(args_obj)?;
            let note_id = MemoryNoteId::from_str(&parsed.note_id)?;
            let scope = MemoryScope::try_new(parsed.scope)?;
            let idempotency_key = match parsed.idempotency_key {
                Some(raw) => IdempotencyKey::try_new(raw)?,
                None => IdempotencyKey::generate(),
            };
            let req = CorrectNoteRequest {
                note_id,
                scope,
                expected_revision: parsed.expected_revision,
                title: parsed.title,
                body: parsed.body,
                tags: parsed.tags,
                rationale: parsed.rationale,
                evidence_links: parsed.evidence_links,
                author_label: parsed.author_label,
                idempotency_key,
            };
            req.validate()?;
            Ok(Some(ParsedToolCall::CorrectNote(req)))
        }
        _ => Ok(None),
    }
}

fn deserialize_args<T: for<'de> Deserialize<'de>>(value: Value) -> Result<T, DennettError> {
    serde_json::from_value(value).map_err(|err| DennettError::InvalidInput {
        field: String::from("arguments"),
        reason: redact_secrets(&err.to_string(), &[]),
    })
}

/// Thin MCP JSON-RPC dispatcher backed by [`MemoryClient`].
#[derive(Debug, Clone)]
pub struct MemoryMcpServer {
    client: MemoryClient,
}

impl MemoryMcpServer {
    /// Creates a new [`MemoryMcpServer`] wrapping a configured [`MemoryClient`].
    #[must_use]
    pub const fn new(client: MemoryClient) -> Self {
        Self { client }
    }

    /// Redacts generic secrets and the configured bearer credential from `text`.
    #[must_use]
    pub fn redact_text(&self, text: &str) -> String {
        self.client.redact_text(text)
    }

    /// Dispatches one parsed JSON-RPC 2.0 message and returns an optional response payload.
    ///
    /// Only a JSON object carrying a string `"method"` and omitting `"id"` is treated as a
    /// notification (`None`). Non-object frames or objects missing a valid string `"method"`
    /// return JSON-RPC `-32600 Invalid Request`.
    pub async fn handle_message(&self, msg: &Value) -> Option<Value> {
        let Some(obj) = msg.as_object() else {
            return Some(invalid_request(
                &Value::Null,
                "invalid JSON-RPC request: frame must be a JSON object",
            ));
        };

        let has_id = obj.contains_key("id");
        let id = obj.get("id").cloned().unwrap_or(Value::Null);
        let Some(method) = obj.get("method").and_then(Value::as_str) else {
            return Some(invalid_request(
                &id,
                "invalid JSON-RPC request: missing string `method`",
            ));
        };

        if !has_id {
            return None;
        }

        let params = obj.get("params").unwrap_or(&Value::Null);

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
                            "name": "dennett-memory-mcp",
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
                    "tools": tool_descriptors()
                }
            })),
            "tools/call" => Some(self.handle_tool_call(&id, params).await),
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

    async fn handle_tool_call(&self, id: &Value, params: &Value) -> Value {
        let tool_name = params
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let raw_args = params.get("arguments").unwrap_or(&Value::Null);

        let parsed = match parse_tool_call(tool_name, raw_args) {
            Ok(Some(call)) => call,
            Ok(None) => {
                return json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32601,
                        "message": format!("unknown tool `{tool_name}`")
                    }
                });
            }
            Err(err) => return self.tool_error_redacted(id, &err),
        };

        let result_json = match parsed {
            ParsedToolCall::Search(req) => self
                .client
                .search_notes(&req, None)
                .await
                .and_then(|resp| to_json_string(&resp)),
            ParsedToolCall::GetCurrent(note_id) => self
                .client
                .get_note_current(note_id, None)
                .await
                .and_then(|resp| to_json_string(&resp)),
            ParsedToolCall::GetHistory(note_id) => self
                .client
                .get_note_history(note_id, None)
                .await
                .and_then(|resp| to_json_string(&resp)),
            ParsedToolCall::WriteNote(req) => self
                .client
                .create_note(&req, None)
                .await
                .and_then(|resp| to_json_string(&resp)),
            ParsedToolCall::CorrectNote(req) => self
                .client
                .correct_note(&req, None)
                .await
                .and_then(|resp| to_json_string(&resp)),
        };

        match result_json {
            Ok(text) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [{
                        "type": "text",
                        "text": text
                    }],
                    "isError": false
                }
            }),
            Err(err) => self.tool_error_redacted(id, &err),
        }
    }

    fn tool_error_redacted(&self, id: &Value, err: &DennettError) -> Value {
        let safe_message = self.client.redact_text(&err.to_string());
        let payload = json!({
            "code": err.code(),
            "error": err,
            "message": safe_message,
        });
        let safe_text = self.client.redact_text(&payload.to_string());
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "content": [{
                    "type": "text",
                    "text": safe_text
                }],
                "isError": true
            }
        })
    }
}

fn invalid_request(id: &Value, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": -32600,
            "message": message
        }
    })
}

fn to_json_string<T: serde::Serialize>(value: &T) -> Result<String, DennettError> {
    serde_json::to_string(value).map_err(|err| DennettError::Internal {
        detail: format!("failed to serialize MCP tool response: {err}"),
    })
}

/// Formats a [`DennettError`] into a structured, secret-redacted MCP tool error payload.
#[must_use]
pub fn tool_error(id: &Value, err: &DennettError) -> Value {
    let safe_message = redact_secrets(&err.to_string(), &[]);
    let payload = json!({
        "code": err.code(),
        "error": err,
        "message": safe_message,
    });
    let safe_text = redact_secrets(&payload.to_string(), &[]);
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "content": [{
                "type": "text",
                "text": safe_text
            }],
            "isError": true
        }
    })
}

/// Returns the JSON Schema descriptors for all tools exposed by `dennett-memory-mcp`.
#[must_use]
pub fn tool_descriptors() -> Value {
    let evidence_link_schema = json!({
        "type": "object",
        "properties": {
            "evidence_id": { "type": "string", "description": "UUID of an existing evidence object in the same scope." },
            "relationship": { "type": "string", "description": "Link relationship (e.g. supports, excerpt, correction_basis)." },
            "ordinal": { "type": ["integer", "null"], "minimum": 0, "description": "Optional non-negative position hint." },
            "anchor": {
                "type": ["object", "null"],
                "properties": {
                    "kind": { "type": "string" },
                    "start": { "type": "integer", "minimum": 0 },
                    "end": { "type": "integer", "minimum": 0 },
                    "locator": { "type": "string" },
                    "attributes": { "type": "object", "additionalProperties": { "type": "string" } }
                },
                "required": ["kind"]
            }
        },
        "required": ["evidence_id", "relationship"]
    });

    json!([
        {
            "name": "memory_search",
            "description": "Search current memory notes within authorized scopes.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "scope": { "type": "string", "description": "Optional scope filter (must be allowed by the active credential)." },
                    "query": { "type": "string", "description": "Optional text search query over note titles, bodies, and tags." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 100 }
                }
            }
        },
        {
            "name": "memory_get_current",
            "description": "Fetch the current revision and evidence links of a memory note.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "note_id": { "type": "string", "description": "UUID of the memory note." }
                },
                "required": ["note_id"]
            }
        },
        {
            "name": "memory_get_history",
            "description": "Fetch the complete immutable revision history of a memory note.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "note_id": { "type": "string", "description": "UUID of the memory note." }
                },
                "required": ["note_id"]
            }
        },
        {
            "name": "memory_write_note",
            "description": "Create a new revision-1 memory note with optional evidence links and idempotency key.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "scope": { "type": "string" },
                    "title": { "type": "string" },
                    "body": { "type": "string" },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "evidence_links": { "type": "array", "items": evidence_link_schema.clone() },
                    "author_label": { "type": "string", "description": "Optional display label; never used for authorization." },
                    "idempotency_key": { "type": "string" }
                },
                "required": ["scope", "title", "body"]
            }
        },
        {
            "name": "memory_correct_note",
            "description": "Append an immutable correction revision to an existing memory note using optimistic revision locking.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "note_id": { "type": "string" },
                    "scope": { "type": "string" },
                    "expected_revision": { "type": "integer", "minimum": 1 },
                    "title": { "type": "string" },
                    "body": { "type": "string" },
                    "rationale": { "type": "string" },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "evidence_links": { "type": "array", "items": evidence_link_schema },
                    "author_label": { "type": "string", "description": "Optional display label; never used for authorization." },
                    "idempotency_key": { "type": "string" }
                },
                "required": ["note_id", "scope", "expected_revision", "title", "body", "rationale"]
            }
        }
    ])
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use proptest::prelude::*;

    use super::*;

    #[test]
    fn bounded_line_reader_enforces_exact_limit_and_recovers_on_next_line() {
        let input = b"12345\n1234567890\nok\n";
        let mut cursor = Cursor::new(&input[..]);

        assert_eq!(
            read_bounded_line(&mut cursor, 6).expect("line 1"),
            BoundedRead::Line(String::from("12345\n"))
        );
        assert_eq!(
            read_bounded_line(&mut cursor, 6).expect("line 2 oversized"),
            BoundedRead::TooLarge
        );
        assert_eq!(
            read_bounded_line(&mut cursor, 6).expect("line 3 recovered"),
            BoundedRead::Line(String::from("ok\n"))
        );
        assert_eq!(
            read_bounded_line(&mut cursor, 6).expect("eof"),
            BoundedRead::Eof
        );
    }

    #[test]
    fn parse_tool_call_validates_all_five_tools_and_rejects_bad_inputs() {
        let search = parse_tool_call(
            "memory_search",
            &json!({"scope": "project-alpha", "limit": 5}),
        );
        assert!(matches!(search, Ok(Some(ParsedToolCall::Search(_)))));

        let bad_scope = parse_tool_call("memory_search", &json!({"scope": "../escape"}));
        assert!(matches!(bad_scope, Err(DennettError::InvalidInput { .. })));

        let bad_limit = parse_tool_call("memory_search", &json!({"limit": 0}));
        assert!(matches!(bad_limit, Err(DennettError::InvalidInput { .. })));

        let bad_uuid = parse_tool_call("memory_get_current", &json!({"note_id": "not-a-uuid"}));
        assert!(matches!(bad_uuid, Err(DennettError::InvalidInput { .. })));

        let bad_rev = parse_tool_call(
            "memory_correct_note",
            &json!({
                "note_id": MemoryNoteId::new().to_string(),
                "scope": "project-alpha",
                "expected_revision": 0,
                "title": "t",
                "body": "b",
                "rationale": "r"
            }),
        );
        assert!(matches!(bad_rev, Err(DennettError::InvalidInput { .. })));

        let unknown = parse_tool_call("memory_drop_database", &json!({}));
        assert_eq!(unknown, Ok(None));
    }

    #[tokio::test]
    async fn mcp_protocol_handles_initialize_ping_list_and_notifications() {
        let client = MemoryClient::unauthenticated("http://127.0.0.1:1").expect("valid client");
        let server = MemoryMcpServer::new(client);

        // Notification without `id` produces no response.
        assert!(
            server
                .handle_message(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
                .await
                .is_none()
        );

        let init = server
            .handle_message(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {"protocolVersion": "2024-11-05"}
            }))
            .await
            .expect("initialize response");
        assert_eq!(init["result"]["serverInfo"]["name"], "dennett-memory-mcp");

        let list = server
            .handle_message(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}))
            .await
            .expect("tools/list response");
        let tools = list["result"]["tools"].as_array().expect("tools array");
        assert_eq!(tools.len(), 5);

        let unknown = server
            .handle_message(&json!({"jsonrpc": "2.0", "id": 3, "method": "does/not/exist"}))
            .await
            .expect("unknown method error");
        assert_eq!(unknown["error"]["code"], -32601);
    }

    #[tokio::test]
    async fn mcp_rejects_non_object_and_missing_method_frames_with_32600_and_redacts_token() {
        let secret = "dnt_static_secret_token_for_redaction_check_9911";
        let client = MemoryClient::new("http://127.0.0.1:1", secret).expect("valid client");
        let server = MemoryMcpServer::new(client);

        for invalid in [
            json!(123),
            json!([]),
            json!("raw-string"),
            json!({}),
            json!({"method": 42}),
        ] {
            let resp = server.handle_message(&invalid).await.unwrap_or_else(|| {
                panic!("invalid frame `{invalid}` must return -32600, not None")
            });
            assert_eq!(resp["error"]["code"], -32600);
            assert_eq!(resp["id"], Value::Null);
        }

        let missing_method_with_id = server
            .handle_message(&json!({"jsonrpc": "2.0", "id": 9}))
            .await
            .expect("missing method with id must return -32600");
        assert_eq!(missing_method_with_id["error"]["code"], -32600);
        assert_eq!(missing_method_with_id["id"], 9);

        // Pre-HTTP tool validation error echoing the static token string MUST redact it.
        let leak_attempt = server
            .handle_message(&json!({
                "jsonrpc": "2.0",
                "id": 10,
                "method": "tools/call",
                "params": {
                    "name": "memory_search",
                    "arguments": {
                        secret: "unexpected_field_with_secret_name"
                    }
                }
            }))
            .await
            .expect("tool error response");
        assert_eq!(leak_attempt["result"]["isError"], true);
        let text = leak_attempt["result"]["content"][0]["text"]
            .as_str()
            .expect("text content");
        assert!(
            !text.contains(secret),
            "configured bearer token leaked in pre-HTTP MCP tool error: {text}"
        );
    }

    proptest! {
        #[test]
        fn prop_bounded_reader_never_exceeds_max_bytes_on_arbitrary_ascii(
            payload in "[A-Za-z0-9 ]{0,200}",
            limit in 1_usize..64_usize,
        ) {
            let line = format!("{payload}\n");
            let mut cursor = Cursor::new(line.as_bytes());
            let res = read_bounded_line(&mut cursor, limit).expect("valid ascii");
            let eof = read_bounded_line(&mut cursor, limit).expect("eof");
            prop_assert_eq!(eof, BoundedRead::Eof);
            if line.len() <= limit {
                prop_assert_eq!(res, BoundedRead::Line(line));
            } else {
                prop_assert_eq!(res, BoundedRead::TooLarge);
            }
        }
    }
}
