//! Stdio MCP client bridge for [`crate::LocalHttpRuntime`].
//!
//! Spawns configured [`RuntimeMcpServerSpec`] stdio processes inside OS-native process-tree
//! containment ([`ProcessTreeGuard`]: Win32 Job Object on Windows, dedicated process group on
//! Unix), executes MCP JSON-RPC 2.0 `initialize` + `notifications/initialized` + `tools/list`
//! discovery, bridges OpenAI-compatible function calls back to `(server_name, tool_name)` via
//! `tools/call`, and deterministically reaps every child process on turn completion, error, or
//! cancellation.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use dennett_contracts::DennettError;
use dennett_contracts::redact::redact_secrets_owned;
use dennett_runtime_antigravity::{ProcessTreeGuard, RuntimeMcpServerSpec};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::watch;
use tokio::time;

/// MCP JSON-RPC protocol version advertised during `initialize`.
const MCP_PROTOCOL_VERSION: &str = "2024-11-05";
/// Maximum permitted length of a single JSON-RPC stdout line from an MCP child (2 MiB).
const MAX_MCP_LINE_BYTES: usize = 2 * 1024 * 1024;
/// Maximum wall time for MCP `initialize` + `tools/list` handshake per server.
const MCP_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// Graceful EOF wait before terminating an MCP child process tree.
const MCP_SHUTDOWN_GRACE: Duration = Duration::from_millis(200);
/// Kernel teardown drain budget when terminating an MCP child process tree.
const MCP_TREE_DRAIN_BUDGET: Duration = Duration::from_millis(500);
/// Maximum length of an OpenAI-compatible function tool identifier.
const MAX_OPENAI_TOOL_NAME_LEN: usize = 64;

/// Built-in local workspace tool names that MCP raw-name fallback must never shadow.
const BUILTIN_LOCAL_TOOL_NAMES: [&str; 4] = ["read_file", "write_file", "list_dir", "run_command"];

/// Sensitive host environment variables scrubbed from spawned MCP child processes before
/// applying explicitly scoped `RuntimeMcpServerSpec.env` entries.
const DEFAULT_SCRUBBED_ENV_VARS: [&str; 12] = [
    "GEMINI_API_KEY",
    "GOOGLE_API_KEY",
    "DENNETT_MEMORY_PG_URL",
    "DENNETT_CONTROL_PG_URL",
    "DENNETT_TEST_MEMORY_PG_URL",
    "DENNETT_TEST_CONTROL_PG_URL",
    "PGPASSWORD",
    "DENNETT_MEMORY_TOKEN",
    "DENNETT_MEMORY_HMAC_SECRET",
    "DENNETT_MEMORY_AUTH_CONFIG",
    "DENNETT_HEAD_TOKEN",
    "DENNETT_CONTROL_TOKEN",
];

/// Metadata and `OpenAI` function descriptor for one discovered stdio MCP tool.
#[derive(Debug, Clone)]
pub(crate) struct DiscoveredMcpTool {
    /// Index into [`McpBridgePool::clients`].
    pub server_idx: usize,
    /// Configured logical server name (e.g. `"dennett-memory"`, `"dennett-control"`).
    pub server_name: String,
    /// Original tool name advertised by the MCP server's `tools/list`.
    pub tool_name: String,
    /// Policy-canonical identifier (`mcp__<server_name>__<tool_name>`) recognized by Dennett's
    /// `PolicyEngine` (`MEMORY_MCP_OPS`, `CONTROL_MCP_OPS`, and registered third-party MCP servers).
    pub policy_tool_name: String,
    /// OpenAI-compatible `{"type":"function","function":{...}}` tool schema.
    pub openai_schema: Value,
}

/// Outcome of invoking one MCP tool via `tools/call`.
#[derive(Debug, Clone)]
pub(crate) enum McpCallResult {
    /// The turn was cancelled before the MCP child responded.
    Cancelled,
    /// The MCP server returned a result payload (either normal or `isError: true`).
    Completed {
        /// `true` when the MCP server flagged `isError: true` or returned a JSON-RPC error.
        is_error: bool,
        /// Extracted text output (not yet truncated).
        text: String,
    },
}

struct RawMcpTool {
    name: String,
    description: String,
    input_schema: Value,
}

enum RpcReadOutcome {
    Cancelled,
    Response(Value),
}

enum AsyncBoundedRead {
    Eof,
    Line(String),
    TooLarge,
}

struct McpStdioClient {
    server_name: String,
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    process_tree: ProcessTreeGuard,
    next_id: i64,
}

impl Drop for McpStdioClient {
    fn drop(&mut self) {
        self.process_tree.terminate();
        let _ = self.child.start_kill();
    }
}

impl McpStdioClient {
    async fn spawn_and_init(
        spec: &RuntimeMcpServerSpec,
        workspace_cwd: &Path,
        secrets: &[String],
        timeout: Duration,
        cancel_rx: &mut watch::Receiver<bool>,
    ) -> Result<(Self, Vec<RawMcpTool>), DennettError> {
        let mut cmd = Command::new(&spec.command);
        cmd.args(&spec.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        if workspace_cwd.is_dir() {
            cmd.current_dir(workspace_cwd);
        }
        for key in DEFAULT_SCRUBBED_ENV_VARS {
            cmd.env_remove(key);
        }
        for entry in &spec.env {
            cmd.env(&entry.name, &entry.value);
        }
        ProcessTreeGuard::configure_command(&mut cmd);

        let mut child = cmd.spawn().map_err(|err| DennettError::RuntimeProtocol {
            code: None,
            detail: redact_secrets_owned(
                &format!(
                    "failed spawning stdio MCP server `{}` (`{}`): {err}",
                    spec.name, spec.command
                ),
                secrets,
            ),
        })?;
        let process_tree = ProcessTreeGuard::attach(&child);

        let Some(stdin) = child.stdin.take() else {
            process_tree.terminate_and_drain(MCP_TREE_DRAIN_BUDGET).await;
            let _ = child.start_kill();
            let _ = time::timeout(MCP_TREE_DRAIN_BUDGET, child.wait()).await;
            return Err(DennettError::RuntimeProtocol {
                code: None,
                detail: format!("stdio MCP server `{}` missing piped stdin", spec.name),
            });
        };
        let Some(stdout) = child.stdout.take() else {
            process_tree.terminate_and_drain(MCP_TREE_DRAIN_BUDGET).await;
            let _ = child.start_kill();
            let _ = time::timeout(MCP_TREE_DRAIN_BUDGET, child.wait()).await;
            return Err(DennettError::RuntimeProtocol {
                code: None,
                detail: format!("stdio MCP server `{}` missing piped stdout", spec.name),
            });
        };

        let mut client = Self {
            server_name: spec.name.clone(),
            child,
            stdin: Some(stdin),
            stdout: BufReader::new(stdout),
            process_tree,
            next_id: 1,
        };

        let deadline = time::Instant::now() + timeout;
        let init_res = client.handshake(secrets, deadline, cancel_rx).await;
        match init_res {
            Ok(tools) => Ok((client, tools)),
            Err(err) => {
                client.shutdown().await;
                Err(err)
            }
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "executes sequential MCP initialize, notifications/initialized, and tools/list handshake with cancellation and deadline enforcement"
    )]
    async fn handshake(
        &mut self,
        secrets: &[String],
        deadline: time::Instant,
        cancel_rx: &mut watch::Receiver<bool>,
    ) -> Result<Vec<RawMcpTool>, DennettError> {
        // 1. `initialize`
        let init_id = self.alloc_id();
        let init_req = json!({
            "jsonrpc": "2.0",
            "id": init_id,
            "method": "initialize",
            "params": {
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {
                    "name": "dennett-local-http-runtime",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        });
        let init_resp = match self
            .send_and_recv(init_id, &init_req, secrets, deadline, cancel_rx)
            .await?
        {
            RpcReadOutcome::Cancelled => {
                return Err(DennettError::Cancelled {
                    operation: format!(
                        "mcp.initialize({})",
                        self.server_name
                    ),
                });
            }
            RpcReadOutcome::Response(val) => val,
        };
        if let Some(err_obj) = init_resp.get("error") {
            return Err(DennettError::RuntimeProtocol {
                code: err_obj.get("code").and_then(Value::as_i64),
                detail: redact_secrets_owned(
                    &format!(
                        "MCP server `{}` rejected initialize: {}",
                        self.server_name,
                        extract_rpc_error_message(err_obj)
                    ),
                    secrets,
                ),
            });
        }

        // 2. `notifications/initialized`
        let initialized_notif = json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        self.write_json_line(&initialized_notif, secrets, deadline)
            .await?;

        // 3. `tools/list`
        let list_id = self.alloc_id();
        let list_req = json!({
            "jsonrpc": "2.0",
            "id": list_id,
            "method": "tools/list",
            "params": {}
        });
        let list_resp = match self
            .send_and_recv(list_id, &list_req, secrets, deadline, cancel_rx)
            .await?
        {
            RpcReadOutcome::Cancelled => {
                return Err(DennettError::Cancelled {
                    operation: format!(
                        "mcp.tools_list({})",
                        self.server_name
                    ),
                });
            }
            RpcReadOutcome::Response(val) => val,
        };
        if let Some(err_obj) = list_resp.get("error") {
            return Err(DennettError::RuntimeProtocol {
                code: err_obj.get("code").and_then(Value::as_i64),
                detail: redact_secrets_owned(
                    &format!(
                        "MCP server `{}` tools/list failed: {}",
                        self.server_name,
                        extract_rpc_error_message(err_obj)
                    ),
                    secrets,
                ),
            });
        }

        let mut tools = Vec::new();
        if let Some(arr) = list_resp
            .get("result")
            .and_then(|r| r.get("tools"))
            .and_then(Value::as_array)
        {
            for item in arr {
                let Some(name) = item
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                else {
                    continue;
                };
                let description = item
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let input_schema = item
                    .get("inputSchema")
                    .or_else(|| item.get("input_schema"))
                    .filter(|v| v.is_object())
                    .cloned()
                    .unwrap_or_else(|| json!({ "type": "object", "properties": {} }));
                tools.push(RawMcpTool {
                    name: name.to_owned(),
                    description,
                    input_schema,
                });
            }
        }
        Ok(tools)
    }

    async fn call_tool(
        &mut self,
        tool_name: &str,
        arguments: &Value,
        secrets: &[String],
        timeout: Duration,
        cancel_rx: &mut watch::Receiver<bool>,
    ) -> Result<McpCallResult, DennettError> {
        let id = self.alloc_id();
        let args_obj = if arguments.is_object() {
            arguments.clone()
        } else {
            json!({})
        };
        let req = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": args_obj,
            }
        });
        let deadline = time::Instant::now() + timeout;
        let resp = match self
            .send_and_recv(id, &req, secrets, deadline, cancel_rx)
            .await?
        {
            RpcReadOutcome::Cancelled => return Ok(McpCallResult::Cancelled),
            RpcReadOutcome::Response(val) => val,
        };

        if let Some(err_obj) = resp.get("error") {
            return Ok(McpCallResult::Completed {
                is_error: true,
                text: redact_secrets_owned(
                    &format!("MCP error: {}", extract_rpc_error_message(err_obj)),
                    secrets,
                ),
            });
        }

        let result = resp.get("result").unwrap_or(&Value::Null);
        let is_error = result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let text = extract_mcp_result_text(result);
        Ok(McpCallResult::Completed {
            is_error,
            text: redact_secrets_owned(&text, secrets),
        })
    }

    fn alloc_id(&mut self) -> i64 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        id
    }

    async fn write_json_line(
        &mut self,
        payload: &Value,
        secrets: &[String],
        deadline: time::Instant,
    ) -> Result<(), DennettError> {
        let remaining = deadline.saturating_duration_since(time::Instant::now());
        if remaining.is_zero() {
            return Err(DennettError::Timeout {
                operation: format!("mcp:{}:write", self.server_name),
            });
        }
        let mut line = serde_json::to_string(payload).map_err(|err| DennettError::Internal {
            detail: redact_secrets_owned(
                &format!("failed serializing MCP JSON-RPC frame: {err}"),
                secrets,
            ),
        })?;
        line.push('\n');

        let Some(stdin) = self.stdin.as_mut() else {
            return Err(DennettError::RuntimeProtocol {
                code: None,
                detail: format!("MCP server `{}` stdin already closed", self.server_name),
            });
        };

        let write_fut = async {
            stdin.write_all(line.as_bytes()).await?;
            stdin.flush().await
        };
        time::timeout(remaining, write_fut)
            .await
            .map_err(|_| DennettError::Timeout {
                operation: format!("mcp:{}:write", self.server_name),
            })?
            .map_err(|err| DennettError::RuntimeProtocol {
                code: None,
                detail: redact_secrets_owned(
                    &format!("failed writing to MCP server `{}`: {err}", self.server_name),
                    secrets,
                ),
            })
    }

    async fn send_and_recv(
        &mut self,
        expected_id: i64,
        payload: &Value,
        secrets: &[String],
        deadline: time::Instant,
        cancel_rx: &mut watch::Receiver<bool>,
    ) -> Result<RpcReadOutcome, DennettError> {
        if *cancel_rx.borrow() {
            return Ok(RpcReadOutcome::Cancelled);
        }
        self.write_json_line(payload, secrets, deadline).await?;

        let mut cancel_open = true;
        loop {
            if *cancel_rx.borrow() {
                return Ok(RpcReadOutcome::Cancelled);
            }
            let remaining = deadline.saturating_duration_since(time::Instant::now());
            if remaining.is_zero() {
                return Err(DennettError::Timeout {
                    operation: format!("mcp:{}:read", self.server_name),
                });
            }

            let read_fut = read_bounded_async_line(&mut self.stdout, MAX_MCP_LINE_BYTES);
            let sleep_fut = time::sleep(remaining);
            tokio::pin!(read_fut);
            tokio::pin!(sleep_fut);

            let line_outcome = loop {
                tokio::select! {
                    biased;
                    changed = cancel_rx.changed(), if cancel_open => {
                        if changed.is_ok() {
                            if *cancel_rx.borrow() {
                                return Ok(RpcReadOutcome::Cancelled);
                            }
                        } else {
                            cancel_open = false;
                        }
                    }
                    () = &mut sleep_fut => {
                        return Err(DennettError::Timeout {
                            operation: format!("mcp:{}:read", self.server_name),
                        });
                    }
                    res = &mut read_fut => break res,
                }
            };

            let raw_line = match line_outcome {
                Ok(AsyncBoundedRead::Eof) => {
                    return Err(DennettError::RuntimeProtocol {
                        code: None,
                        detail: format!(
                            "MCP server `{}` closed stdout unexpectedly before replying to id={expected_id}",
                            self.server_name
                        ),
                    });
                }
                Ok(AsyncBoundedRead::TooLarge) => {
                    return Err(DennettError::RuntimeProtocol {
                        code: None,
                        detail: format!(
                            "MCP server `{}` emitted a response exceeding {MAX_MCP_LINE_BYTES} bytes",
                            self.server_name
                        ),
                    });
                }
                Ok(AsyncBoundedRead::Line(s)) => s,
                Err(err) => {
                    return Err(DennettError::RuntimeProtocol {
                        code: None,
                        detail: redact_secrets_owned(
                            &format!(
                                "failed reading from MCP server `{}` stdout: {err}",
                                self.server_name
                            ),
                            secrets,
                        ),
                    });
                }
            };

            let trimmed = raw_line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Ok(msg) = serde_json::from_str::<Value>(trimmed) else {
                // Ignore stray non-JSON stdout banner lines safely instead of crashing discovery.
                continue;
            };
            let matches_id = msg.get("id").and_then(Value::as_i64) == Some(expected_id);
            if matches_id {
                return Ok(RpcReadOutcome::Response(msg));
            }
        }
    }

    async fn shutdown(mut self) {
        // Close stdin first so well-behaved stdio MCP servers exit cleanly on EOF.
        drop(self.stdin.take());
        let exited = matches!(
            time::timeout(MCP_SHUTDOWN_GRACE, self.child.wait()).await,
            Ok(Ok(_))
        );
        if !exited {
            self.process_tree
                .terminate_and_drain(MCP_TREE_DRAIN_BUDGET)
                .await;
            let _ = self.child.start_kill();
            let _ = time::timeout(MCP_TREE_DRAIN_BUDGET, self.child.wait()).await;
        }
        self.process_tree.terminate();
    }
}

/// Active per-turn pool of spawned stdio MCP servers and their discovered `OpenAI` tool schemas.
pub(crate) struct McpBridgePool {
    clients: Vec<McpStdioClient>,
    tools: Vec<DiscoveredMcpTool>,
    by_wire_name: HashMap<String, usize>,
    by_unique_raw_name: HashMap<String, usize>,
}

impl McpBridgePool {
    /// Returns an empty bridge pool with zero spawned child processes.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            clients: Vec::new(),
            tools: Vec::new(),
            by_wire_name: HashMap::new(),
            by_unique_raw_name: HashMap::new(),
        }
    }

    /// Spawns and initializes every configured stdio MCP server, discovering all advertised tools.
    ///
    /// If any server fails to start or handshake, every previously spawned server is deterministically
    /// shut down before returning the error.
    pub async fn start_all(
        specs: &[RuntimeMcpServerSpec],
        workspace_cwd: &Path,
        secrets: &[String],
        cancel_rx: &mut watch::Receiver<bool>,
    ) -> Result<Self, DennettError> {
        if specs.is_empty() {
            return Ok(Self::empty());
        }

        let mut pool = Self::empty();
        let mut used_wire_names: HashSet<String> = BUILTIN_LOCAL_TOOL_NAMES
            .iter()
            .map(|&s| s.to_owned())
            .collect();
        let mut raw_name_counts: HashMap<String, usize> = HashMap::new();

        for spec in specs {
            if *cancel_rx.borrow() {
                pool.shutdown().await;
                return Err(DennettError::Cancelled {
                    operation: String::from("mcp.startup"),
                });
            }
            let (client, raw_tools) = match McpStdioClient::spawn_and_init(
                spec,
                workspace_cwd,
                secrets,
                MCP_HANDSHAKE_TIMEOUT,
                cancel_rx,
            )
            .await
            {
                Ok(pair) => pair,
                Err(err) => {
                    pool.shutdown().await;
                    return Err(err);
                }
            };

            let server_idx = pool.clients.len();
            pool.clients.push(client);

            for raw_tool in raw_tools {
                let wire_name = make_unique_wire_name(&spec.name, &raw_tool.name, &used_wire_names);
                used_wire_names.insert(wire_name.clone());
                let policy_tool_name = format!("mcp__{}__{}", spec.name, raw_tool.name);
                let description = if raw_tool.description.trim().is_empty() {
                    format!("[MCP:{}] {}", spec.name, raw_tool.name)
                } else {
                    format!("[MCP:{}] {}", spec.name, raw_tool.description.trim())
                };
                let openai_schema = json!({
                    "type": "function",
                    "function": {
                        "name": wire_name,
                        "description": description,
                        "parameters": raw_tool.input_schema,
                    }
                });

                let tool_idx = pool.tools.len();
                *raw_name_counts.entry(raw_tool.name.clone()).or_insert(0) += 1;
                pool.by_wire_name.insert(wire_name, tool_idx);
                pool.tools.push(DiscoveredMcpTool {
                    server_idx,
                    server_name: spec.name.clone(),
                    tool_name: raw_tool.name,
                    policy_tool_name,
                    openai_schema,
                });
            }
        }

        // Allow unambiguous unprefixed MCP tool names (excluding built-in workspace tools)
        // for smaller local models that occasionally drop the `mcp__<server>__` prefix.
        for (idx, tool) in pool.tools.iter().enumerate() {
            if !BUILTIN_LOCAL_TOOL_NAMES.contains(&tool.tool_name.as_str())
                && raw_name_counts.get(&tool.tool_name).copied() == Some(1)
            {
                pool.by_unique_raw_name.insert(tool.tool_name.clone(), idx);
            }
        }

        Ok(pool)
    }

    /// Returns OpenAI-compatible function schemas for every discovered MCP tool.
    #[must_use]
    pub fn openai_tool_definitions(&self) -> Vec<Value> {
        self.tools.iter().map(|t| t.openai_schema.clone()).collect()
    }

    /// Resolves a model-emitted function name to a discovered MCP tool `(tool_idx, &DiscoveredMcpTool)`.
    #[must_use]
    pub fn resolve_tool(&self, called_name: &str) -> Option<(usize, &DiscoveredMcpTool)> {
        let idx = self
            .by_wire_name
            .get(called_name)
            .or_else(|| self.by_unique_raw_name.get(called_name))
            .copied()?;
        self.tools.get(idx).map(|t| (idx, t))
    }

    /// Returns `true` if a discovered MCP server exposes `dennett_skill_read`.
    #[must_use]
    pub fn has_control_skill_tool(&self) -> bool {
        self.tools
            .iter()
            .any(|t| t.tool_name == "dennett_skill_read")
    }

    /// Invokes a discovered MCP tool by index after `PolicyEvaluator` approval.
    pub async fn call_tool(
        &mut self,
        tool_idx: usize,
        arguments: &Value,
        secrets: &[String],
        timeout: Duration,
        cancel_rx: &mut watch::Receiver<bool>,
    ) -> Result<McpCallResult, DennettError> {
        let Some(meta) = self.tools.get(tool_idx) else {
            return Err(DennettError::InvalidInput {
                field: String::from("tool_name"),
                reason: String::from("unknown MCP tool index"),
            });
        };
        let server_idx = meta.server_idx;
        let orig_name = meta.tool_name.clone();
        let Some(client) = self.clients.get_mut(server_idx) else {
            return Err(DennettError::Internal {
                detail: String::from("MCP server client missing for discovered tool"),
            });
        };
        client
            .call_tool(&orig_name, arguments, secrets, timeout, cancel_rx)
            .await
    }

    /// Deterministically terminates and reaps every spawned MCP child process.
    pub async fn shutdown(self) {
        for client in self.clients {
            client.shutdown().await;
        }
    }
}

fn sanitize_ident_segment(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        String::from("mcp")
    } else {
        out
    }
}

fn fnv1a_32(input: &str) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for b in input.as_bytes() {
        hash ^= u32::from(*b);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

fn make_unique_wire_name(
    server_name: &str,
    tool_name: &str,
    used: &HashSet<String>,
) -> String {
    let srv = sanitize_ident_segment(server_name);
    let tool = sanitize_ident_segment(tool_name);
    let direct = format!("mcp__{srv}__{tool}");
    if direct.len() <= MAX_OPENAI_TOOL_NAME_LEN && !used.contains(&direct) {
        return direct;
    }

    let hash = fnv1a_32(&format!("{server_name}\0{tool_name}"));
    let mut suffix_counter: u32 = 0;
    loop {
        let suffix = if suffix_counter == 0 {
            format!("_{hash:08x}")
        } else {
            format!("_{hash:08x}_{suffix_counter}")
        };
        let max_prefix = MAX_OPENAI_TOOL_NAME_LEN.saturating_sub(suffix.len());
        let mut prefix = direct.clone();
        if prefix.len() > max_prefix {
            prefix.truncate(max_prefix);
        }
        let candidate = format!("{prefix}{suffix}");
        if !used.contains(&candidate) {
            return candidate;
        }
        suffix_counter = suffix_counter.saturating_add(1);
    }
}

fn extract_rpc_error_message(err_obj: &Value) -> String {
    err_obj
        .get("message")
        .and_then(Value::as_str)
        .map_or_else(|| err_obj.to_string(), str::to_owned)
}

fn extract_mcp_result_text(result: &Value) -> String {
    if let Some(arr) = result.get("content").and_then(Value::as_array) {
        let mut parts = Vec::with_capacity(arr.len());
        for item in arr {
            if let Some(txt) = item.get("text").and_then(Value::as_str) {
                parts.push(txt.to_owned());
            } else {
                parts.push(item.to_string());
            }
        }
        if !parts.is_empty() {
            return parts.join("\n");
        }
    }
    if let Some(s) = result.as_str() {
        return s.to_owned();
    }
    result.to_string()
}

async fn read_bounded_async_line(
    reader: &mut BufReader<ChildStdout>,
    max_bytes: usize,
) -> std::io::Result<AsyncBoundedRead> {
    let mut raw = Vec::new();
    let mut exceeded = false;

    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            if exceeded {
                return Ok(AsyncBoundedRead::TooLarge);
            }
            if raw.is_empty() {
                return Ok(AsyncBoundedRead::Eof);
            }
            break;
        }

        let newline_pos = available.iter().position(|&b| b == b'\n');
        let take_len = newline_pos.map_or(available.len(), |pos| pos + 1);

        if !exceeded {
            if raw.len().saturating_add(take_len) > max_bytes {
                exceeded = true;
                raw.clear();
            } else {
                raw.extend_from_slice(&available[..take_len]);
            }
        }
        reader.consume(take_len);

        if newline_pos.is_some() {
            if exceeded {
                return Ok(AsyncBoundedRead::TooLarge);
            }
            break;
        }
    }

    let text = String::from_utf8(raw)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
    Ok(AsyncBoundedRead::Line(text))
}
