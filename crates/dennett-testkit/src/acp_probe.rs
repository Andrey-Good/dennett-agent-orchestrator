//! JSON-RPC 2.0 over stdio client for Google Antigravity (`agy acp`, ACP v1).
//!
//! Designed for D0 early compatibility probes covering:
//! - binary version checks (`>= 1.5.0`)
//! - `initialize`, `authenticate`, `session/new` (with `mcpServers`), `session/set_config_option`
//! - `session/prompt` streaming updates and `session/request_permission` callbacks
//! - `session/cancel` cancellation

use core::fmt;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant as StdInstant};

use dennett_contracts::{DennettError, RuntimeAuthProfile};
use serde::Serialize;
use serde_json::{Value, json};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::time;

use crate::redact::{redact_secrets, redact_secrets_owned};

/// Minimum supported `agy` CLI version required for ACP v1 probes.
pub const MIN_AGY_VERSION: Semver = Semver::new(1, 5, 0);

/// Maximum size of a single JSON-RPC line read from `agy acp` stdout (4 MiB).
const MAX_ACP_STDOUT_LINE_BYTES: usize = 4 * 1024 * 1024;
/// Maximum bytes retained from the child process stderr stream for redacted diagnostics (64 KiB).
const MAX_STDERR_CAPTURE_BYTES: usize = 64 * 1024;
/// Chunk size used while draining child stderr to avoid unbounded line allocation.
const STDERR_READ_CHUNK_BYTES: usize = 4 * 1024;
/// Timeout used when probing `agy --version`.
const VERSION_PROBE_TIMEOUT: Duration = Duration::from_secs(10);
/// Timeout applied to writing a JSON-RPC frame to `agy acp` stdin.
const ACP_WRITE_TIMEOUT: Duration = Duration::from_secs(10);
/// Graceful wait before forcefully killing `agy acp` during [`AcpClient::shutdown`].
const SHUTDOWN_GRACE_TIMEOUT: Duration = Duration::from_secs(3);
/// Maximum synchronous wait in [`Drop`] after `start_kill` so OS file handles close cleanly.
const DROP_REAP_BUDGET: Duration = Duration::from_millis(200);
/// Poll interval during [`Drop`] child reaping.
const DROP_REAP_POLL: Duration = Duration::from_millis(10);

/// Sensitive host environment variables scrubbed from spawned `agy acp` processes by default.
const DEFAULT_SCRUBBED_ENV_VARS: [&str; 7] = [
    "GEMINI_API_KEY",
    "GOOGLE_API_KEY",
    "DENNETT_MEMORY_PG_URL",
    "DENNETT_CONTROL_PG_URL",
    "DENNETT_TEST_MEMORY_PG_URL",
    "DENNETT_TEST_CONTROL_PG_URL",
    "PGPASSWORD",
];

/// Parsed semantic version triple (`major.minor.patch`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Semver {
    /// Major version component.
    pub major: u64,
    /// Minor version component.
    pub minor: u64,
    /// Patch version component.
    pub patch: u64,
}

impl Semver {
    /// Constructs a semantic version from explicit numeric components.
    #[must_use]
    pub const fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

impl fmt::Display for Semver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Extracts the first `MAJOR.MINOR.PATCH` semantic version from `agy --version` output.
///
/// Accepts plain triples (`"1.5.15"`), prefixed lines (`"agy 1.5.15"`), or pre-release
/// suffixes (`"1.5.15-beta.1"`).
///
/// # Errors
///
/// Returns [`DennettError::InvalidInput`] if no valid `MAJOR.MINOR.PATCH` triple is found.
pub fn parse_agy_version(raw: &str) -> Result<Semver, DennettError> {
    for token in raw.split_whitespace() {
        let trimmed = token
            .trim_start_matches('v')
            .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.' && c != '-');
        let core = trimmed.split(['-', '+']).next().unwrap_or(trimmed);
        let mut parts = core.split('.');
        if let (Some(maj), Some(min), Some(pat), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
            && let (Ok(major), Ok(minor), Ok(patch)) =
                (maj.parse::<u64>(), min.parse::<u64>(), pat.parse::<u64>())
        {
            return Ok(Semver::new(major, minor, patch));
        }
    }

    Err(DennettError::InvalidInput {
        field: String::from("agy_version"),
        reason: format!("could not parse semantic version from `{}`", raw.trim()),
    })
}

/// Parses `raw` and verifies it meets [`MIN_AGY_VERSION`].
///
/// # Errors
///
/// Returns [`DennettError::InvalidInput`] if `raw` cannot be parsed or
/// [`DennettError::RuntimeVersionUnsupported`] if the version is older than [`MIN_AGY_VERSION`].
pub fn ensure_agy_version_supported(raw: &str) -> Result<Semver, DennettError> {
    let parsed = parse_agy_version(raw)?;
    if parsed < MIN_AGY_VERSION {
        return Err(DennettError::RuntimeVersionUnsupported {
            found: parsed.to_string(),
            minimum: MIN_AGY_VERSION.to_string(),
        });
    }
    Ok(parsed)
}

/// Resolves the `agy` binary path from `DENNETT_AGY_BIN` or defaults to `"agy"` on `PATH`.
#[must_use]
pub fn resolve_agy_binary() -> PathBuf {
    env::var_os("DENNETT_AGY_BIN")
        .filter(|v| !v.is_empty())
        .map_or_else(|| PathBuf::from("agy"), PathBuf::from)
}

/// Runs `<binary> --version` and verifies the version meets [`MIN_AGY_VERSION`].
///
/// # Errors
///
/// Returns [`DennettError::RuntimeProtocol`] if executing the binary fails,
/// [`DennettError::Timeout`] if the version probe times out, or
/// [`DennettError::RuntimeVersionUnsupported`] if the binary is too old.
pub async fn detect_agy_version(binary: &Path) -> Result<Semver, DennettError> {
    let mut cmd = Command::new(binary);
    cmd.arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let output = time::timeout(VERSION_PROBE_TIMEOUT, cmd.output())
        .await
        .map_err(|_| DennettError::Timeout {
            operation: String::from("agy --version"),
        })?
        .map_err(|err| DennettError::RuntimeProtocol {
            code: None,
            detail: redact_secrets(
                &format!("failed to execute `{}`: {err}", binary.display()),
                &[],
            ),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let safe_stderr = redact_secrets(stderr.trim(), &[]);
        return Err(DennettError::RuntimeProtocol {
            code: None,
            detail: format!(
                "`agy --version` exited with {}: {safe_stderr}",
                output.status
            ),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    ensure_agy_version_supported(&stdout)
}

/// Spawn parameters for launching `agy acp`.
#[derive(Debug, Clone)]
pub struct AcpSpawnConfig {
    /// Path to the `agy` executable.
    pub binary: PathBuf,
    /// Additional flags passed after `acp` (e.g. `--gemini-dir`, `--app-data-dir`).
    pub extra_args: Vec<String>,
    /// Working directory for the spawned process.
    pub working_dir: PathBuf,
    /// Explicit environment variable overrides applied to the child process.
    pub env_overrides: Vec<(String, String)>,
    /// Extra environment variable names removed from the child process.
    pub env_removals: Vec<String>,
    /// Extra secret literals redacted from any captured diagnostics.
    pub secrets_to_redact: Vec<String>,
}

impl AcpSpawnConfig {
    /// Creates a baseline spawn configuration for `agy acp` in `working_dir`.
    #[must_use]
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            binary: resolve_agy_binary(),
            extra_args: Vec::new(),
            working_dir,
            env_overrides: Vec::new(),
            env_removals: Vec::new(),
            secrets_to_redact: Vec::new(),
        }
    }
}

/// Environment entry passed inside an ACP `mcpServers` specification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct McpEnvEntry {
    /// Variable name.
    pub name: String,
    /// Variable value.
    pub value: String,
}

/// Stdio MCP server definition passed in ACP `session/new`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct McpServerSpec {
    /// Unique MCP server name.
    pub name: String,
    /// Executable command path.
    pub command: String,
    /// Command-line arguments.
    pub args: Vec<String>,
    /// Explicit environment entries for the MCP subprocess.
    pub env: Vec<McpEnvEntry>,
}

/// Caller decision returned from a `session/request_permission` callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionDecision {
    /// Approve only this single invocation (`allow_once`; never escalates to `allow_always`).
    AllowOnce,
    /// Approve this tool invocation pattern persistently (`allow_always`).
    AllowAlways,
    /// Reject only this single invocation (`reject_once`).
    RejectOnce,
    /// Reject persistently (`reject_always`).
    RejectAlways,
}

/// Single selectable option advertised in ACP `session/request_permission`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionOption {
    /// Opaque option identifier echoed back in `selected.optionId`.
    pub option_id: String,
    /// Option kind (`allow_once`, `allow_always`, `reject_once`, `reject_always`).
    pub kind: String,
    /// Human-readable option label.
    pub name: String,
}

/// Read-only view passed into permission callbacks.
#[derive(Debug, Clone)]
pub struct PermissionRequestView {
    /// Tool call identifier.
    pub tool_call_id: String,
    /// Tool call title or summary.
    pub title: String,
    /// Tool call kind (e.g. `execute`, `read`, `other`).
    pub kind: String,
    /// Optional raw JSON input payload for inspecting tool arguments.
    pub raw_input: Value,
    /// Available selection options sent by `agy acp`.
    pub options: Vec<PermissionOption>,
}

/// Recorded permission exchange captured during a prompt turn.
#[derive(Debug, Clone)]
pub struct PermissionRequestEvent {
    /// Tool call identifier.
    pub tool_call_id: String,
    /// Tool call title.
    pub title: String,
    /// Tool call kind.
    pub kind: String,
    /// Options offered by the server.
    pub options: Vec<PermissionOption>,
    /// Selected `optionId`, if a matching option existed.
    pub chosen_option_id: Option<String>,
    /// Decision returned by the callback.
    pub decision: PermissionDecision,
}

/// Recorded tool-call update captured from `session/update`.
#[derive(Debug, Clone)]
pub struct ToolCallEvent {
    /// Tool call identifier.
    pub tool_call_id: String,
    /// Tool call title.
    pub title: String,
    /// Observed status (`pending`, `in_progress`, `completed`, `failed`, etc.).
    pub status: String,
    /// Redacted textual excerpt from tool output, when present.
    pub output_excerpt: String,
}

/// Summary returned by [`AcpClient::initialize`].
#[derive(Debug, Clone)]
pub struct InitializeResult {
    /// Negotiated ACP protocol version.
    pub protocol_version: u64,
    /// Server name from `agentInfo.name`.
    pub agent_name: String,
    /// Server version from `agentInfo.version`.
    pub agent_version: String,
    /// Advertised `authMethods[*].id` entries.
    pub auth_method_ids: Vec<String>,
    /// Full JSON result object for additional assertions.
    pub raw: Value,
}

/// Summary returned by [`AcpClient::session_new`].
#[derive(Debug, Clone)]
pub struct SessionNewResult {
    /// Assigned ACP session identifier.
    pub session_id: String,
    /// Antigravity tier reported under `_meta.antigravity.tier`, when present.
    pub tier: Option<String>,
    /// Model identifiers advertised in `models` or `configOptions` (`configId == "model"`).
    pub available_models: Vec<String>,
    /// Full JSON result object for additional assertions.
    pub raw: Value,
}

/// Aggregated result of a `session/prompt` turn.
#[derive(Debug, Clone)]
pub struct PromptOutcome {
    /// Final `stopReason` (e.g. `"end_turn"`, `"cancelled"`).
    pub stop_reason: String,
    /// Concatenated assistant text streamed via `agent_message_chunk`.
    pub text: String,
    /// Tool call lifecycle events observed during the turn.
    pub tool_events: Vec<ToolCallEvent>,
    /// Permission callback exchanges observed during the turn.
    pub permission_events: Vec<PermissionRequestEvent>,
    /// Optional `usage` payload from the prompt response.
    pub usage: Option<Value>,
}

/// Active stdio JSON-RPC 2.0 client connected to a child `agy acp` process.
pub struct AcpClient {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    stderr_buf: Arc<Mutex<String>>,
    next_id: u64,
    secrets: Vec<String>,
}

impl AcpClient {
    /// Spawns `agy acp` over stdio with the provided configuration.
    ///
    /// Rejects `--dangerously-skip-permissions` unconditionally in accordance with A05 and
    /// scrubs sensitive host credential environment variables before applying explicit
    /// `env_overrides`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::SafetyGuardRejected`] if forbidden flags are supplied, or
    /// [`DennettError::RuntimeProtocol`] if spawning the child process fails.
    pub fn spawn(config: &AcpSpawnConfig) -> Result<Self, DennettError> {
        if config.extra_args.iter().any(|arg| {
            arg == "--dangerously-skip-permissions"
                || arg.starts_with("--dangerously-skip-permissions=")
        }) {
            return Err(DennettError::SafetyGuardRejected {
                reason: String::from(
                    "`--dangerously-skip-permissions` is forbidden in Dennett ACP probes",
                ),
            });
        }

        let mut cmd = Command::new(&config.binary);
        cmd.arg("acp")
            .args(&config.extra_args)
            .current_dir(&config.working_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        // Scrub sensitive credentials by default before applying explicit overrides.
        for sensitive in DEFAULT_SCRUBBED_ENV_VARS {
            cmd.env_remove(sensitive);
        }
        for removed in &config.env_removals {
            cmd.env_remove(removed);
        }
        for (k, v) in &config.env_overrides {
            cmd.env(k, v);
        }

        let mut child = cmd.spawn().map_err(|err| DennettError::RuntimeProtocol {
            code: None,
            detail: format!("failed to spawn `{}`: {err}", config.binary.display()),
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| DennettError::RuntimeProtocol {
                code: None,
                detail: String::from("child process stdin handle was not captured"),
            })?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| DennettError::RuntimeProtocol {
                code: None,
                detail: String::from("child process stdout handle was not captured"),
            })?;
        let stderr = child.stderr.take();

        let stderr_buf = Arc::new(Mutex::new(String::new()));
        if let Some(stderr_pipe) = stderr {
            let capture = Arc::clone(&stderr_buf);
            tokio::spawn(async move {
                drain_stderr_bounded(stderr_pipe, capture).await;
            });
        }

        Ok(Self {
            child,
            stdin: Some(stdin),
            stdout: BufReader::new(stdout),
            stderr_buf,
            next_id: 1,
            secrets: config.secrets_to_redact.clone(),
        })
    }

    /// Returns a redacted snapshot of the child process's captured stderr output.
    #[must_use]
    pub fn stderr_snapshot(&self) -> String {
        let raw = self.stderr_buf.lock().map_or_default(|guard| guard.clone());
        redact_secrets_owned(&raw, &self.secrets)
    }

    /// Performs the ACP v1 `initialize` handshake.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::RuntimeProtocol`] or [`DennettError::Timeout`] if the
    /// handshake fails or returns an invalid payload.
    pub async fn initialize(
        &mut self,
        timeout: Duration,
    ) -> Result<InitializeResult, DennettError> {
        let params = json!({
            "protocolVersion": 1,
            "clientCapabilities": {},
            "clientInfo": {
                "name": "dennett-d0-acp-probe",
                "version": env!("CARGO_PKG_VERSION")
            }
        });
        let raw = self.request("initialize", params, timeout).await?;

        let protocol_version = raw
            .get("protocolVersion")
            .and_then(Value::as_u64)
            .ok_or_else(|| DennettError::RuntimeProtocol {
                code: None,
                detail: String::from("initialize response missing numeric `protocolVersion`"),
            })?;
        let agent_info = raw.get("agentInfo").unwrap_or(&Value::Null);
        let agent_name = agent_info
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let agent_version = agent_info
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let auth_method_ids = raw
            .get("authMethods")
            .and_then(Value::as_array)
            .map_or_default(|arr| {
                arr.iter()
                    .filter_map(|item| item.get("id").and_then(Value::as_str).map(str::to_owned))
                    .collect()
            });

        Ok(InitializeResult {
            protocol_version,
            agent_name,
            agent_version,
            auth_method_ids,
            raw,
        })
    }

    /// Calls ACP `authenticate` for the explicitly requested [`RuntimeAuthProfile`].
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::RuntimeProtocol`] (preserving the JSON-RPC error code such
    /// as `-32602` or `-32000`) or [`DennettError::Timeout`].
    pub async fn authenticate(
        &mut self,
        profile: RuntimeAuthProfile,
        timeout: Duration,
    ) -> Result<Value, DennettError> {
        let params = json!({
            "methodId": profile.as_method_id()
        });
        self.request("authenticate", params, timeout).await
    }

    /// Opens a new ACP session (`session/new`) bound to `cwd` and optional `mcp_servers`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::RuntimeProtocol`] (preserving JSON-RPC codes such as `-32000`
    /// when authentication is required) or [`DennettError::Timeout`].
    pub async fn session_new(
        &mut self,
        cwd: &Path,
        mcp_servers: &[McpServerSpec],
        timeout: Duration,
    ) -> Result<SessionNewResult, DennettError> {
        let cwd_str = clean_path_for_acp(cwd);
        let params = json!({
            "cwd": cwd_str,
            "mcpServers": mcp_servers
        });
        let raw = self.request("session/new", params, timeout).await?;
        parse_session_new_result(raw)
    }

    /// Sets an ACP session configuration option (e.g. `configId: "model"`).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::RuntimeProtocol`] or [`DennettError::Timeout`].
    pub async fn set_config_option(
        &mut self,
        session_id: &str,
        config_id: &str,
        value: &str,
        timeout: Duration,
    ) -> Result<Value, DennettError> {
        let params = json!({
            "sessionId": session_id,
            "configId": config_id,
            "value": value
        });
        self.request("session/set_config_option", params, timeout)
            .await
    }

    /// Sends a `session/prompt` turn and handles streaming `session/update` notifications
    /// and `session/request_permission` callbacks until the turn completes.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::RuntimeProtocol`] or [`DennettError::Timeout`].
    pub async fn prompt<F>(
        &mut self,
        session_id: &str,
        text: &str,
        timeout: Duration,
        on_permission: F,
    ) -> Result<PromptOutcome, DennettError>
    where
        F: FnMut(&PermissionRequestView) -> PermissionDecision,
    {
        self.run_prompt_turn(
            session_id,
            text,
            timeout,
            CancelMode::RunToCompletion,
            on_permission,
        )
        .await
    }

    /// Sends a `session/prompt` turn and immediately transmits a `session/cancel`
    /// notification as soon as the first `session/update` arrives.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::RuntimeProtocol`] or [`DennettError::Timeout`].
    pub async fn prompt_and_cancel_on_first_update<F>(
        &mut self,
        session_id: &str,
        text: &str,
        timeout: Duration,
        on_permission: F,
    ) -> Result<PromptOutcome, DennettError>
    where
        F: FnMut(&PermissionRequestView) -> PermissionDecision,
    {
        self.run_prompt_turn(
            session_id,
            text,
            timeout,
            CancelMode::CancelOnFirstUpdate,
            on_permission,
        )
        .await
    }

    /// Sends a `session/cancel` JSON-RPC notification for `session_id`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::RuntimeProtocol`] or [`DennettError::Timeout`] if writing
    /// to child stdin fails or stalls.
    pub async fn send_cancel(&mut self, session_id: &str) -> Result<(), DennettError> {
        self.write_json(&json!({
            "jsonrpc": "2.0",
            "method": "session/cancel",
            "params": { "sessionId": session_id }
        }))
        .await
    }

    /// Gracefully closes stdin and waits briefly before forcefully terminating `agy acp`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::RuntimeProtocol`] if forcefully killing the child fails.
    pub async fn shutdown(mut self) -> Result<(), DennettError> {
        drop(self.stdin.take());
        if time::timeout(SHUTDOWN_GRACE_TIMEOUT, self.child.wait())
            .await
            .is_err()
        {
            self.child
                .kill()
                .await
                .map_err(|err| DennettError::RuntimeProtocol {
                    code: None,
                    detail: format!("failed to kill agy acp child: {err}"),
                })?;
        }
        Ok(())
    }

    async fn run_prompt_turn<F>(
        &mut self,
        session_id: &str,
        text: &str,
        timeout: Duration,
        cancel_mode: CancelMode,
        mut on_permission: F,
    ) -> Result<PromptOutcome, DennettError>
    where
        F: FnMut(&PermissionRequestView) -> PermissionDecision,
    {
        let req_id = self.allocate_id();
        self.write_json(&json!({
            "jsonrpc": "2.0",
            "id": req_id,
            "method": "session/prompt",
            "params": {
                "sessionId": session_id,
                "prompt": [{ "type": "text", "text": text }]
            }
        }))
        .await?;

        let deadline = time::Instant::now() + timeout;
        let mut collected_text = String::new();
        let mut tool_events = Vec::new();
        let mut permission_events = Vec::new();
        let mut cancel_sent = false;

        loop {
            let remaining = deadline.saturating_duration_since(time::Instant::now());
            if remaining.is_zero() {
                return Err(DennettError::Timeout {
                    operation: String::from("session/prompt"),
                });
            }

            let msg = self.read_message(remaining).await?;

            if msg.get("id").and_then(Value::as_u64) == Some(req_id) && msg.get("method").is_none()
            {
                let result = self.unwrap_rpc_result(&msg)?;
                let stop_reason = result
                    .get("stopReason")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let usage = result.get("usage").cloned();
                return Ok(PromptOutcome {
                    stop_reason,
                    text: collected_text,
                    tool_events,
                    permission_events,
                    usage,
                });
            }

            let Some(method) = msg.get("method").and_then(Value::as_str) else {
                continue;
            };

            match method {
                "session/update" => {
                    self.record_session_update(
                        msg.get("params").unwrap_or(&Value::Null),
                        &mut collected_text,
                        &mut tool_events,
                    );
                    if cancel_mode == CancelMode::CancelOnFirstUpdate && !cancel_sent {
                        self.send_cancel(session_id).await?;
                        cancel_sent = true;
                    }
                }
                "session/request_permission" => {
                    if let Some(server_req_id) = msg.get("id").cloned() {
                        let event = self
                            .handle_permission_request(
                                server_req_id,
                                msg.get("params").unwrap_or(&Value::Null),
                                &mut on_permission,
                            )
                            .await?;
                        permission_events.push(event);
                    }
                }
                _ => {
                    if let Some(server_req_id) = msg.get("id").cloned() {
                        self.write_json(&json!({
                            "jsonrpc": "2.0",
                            "id": server_req_id,
                            "error": {
                                "code": -32601,
                                "message": format!("unsupported client method `{method}`")
                            }
                        }))
                        .await?;
                    }
                }
            }
        }
    }

    async fn handle_permission_request<F>(
        &mut self,
        server_req_id: Value,
        params: &Value,
        on_permission: &mut F,
    ) -> Result<PermissionRequestEvent, DennettError>
    where
        F: FnMut(&PermissionRequestView) -> PermissionDecision,
    {
        let view = parse_permission_request(params);
        let decision = on_permission(&view);
        let chosen = select_permission_option(&view.options, decision);
        let outcome_json = match &chosen {
            Some(option_id) => json!({
                "outcome": {
                    "outcome": "selected",
                    "optionId": option_id
                }
            }),
            None => json!({
                "outcome": {
                    "outcome": "cancelled"
                }
            }),
        };
        self.write_json(&json!({
            "jsonrpc": "2.0",
            "id": server_req_id,
            "result": outcome_json
        }))
        .await?;

        Ok(PermissionRequestEvent {
            tool_call_id: view.tool_call_id,
            title: view.title,
            kind: view.kind,
            options: view.options,
            chosen_option_id: chosen,
            decision,
        })
    }

    async fn request(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, DennettError> {
        let req_id = self.allocate_id();
        self.write_json(&json!({
            "jsonrpc": "2.0",
            "id": req_id,
            "method": method,
            "params": params
        }))
        .await?;

        let deadline = time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(time::Instant::now());
            if remaining.is_zero() {
                return Err(DennettError::Timeout {
                    operation: method.to_owned(),
                });
            }
            let msg = self.read_message(remaining).await?;
            if msg.get("id").and_then(Value::as_u64) == Some(req_id) && msg.get("method").is_none()
            {
                return self.unwrap_rpc_result(&msg);
            }
        }
    }

    fn unwrap_rpc_result(&self, msg: &Value) -> Result<Value, DennettError> {
        if let Some(err) = extract_json_rpc_error(msg, &self.secrets) {
            return Err(err);
        }
        Ok(msg.get("result").cloned().unwrap_or(Value::Null))
    }

    fn record_session_update(
        &self,
        params: &Value,
        collected_text: &mut String,
        tool_events: &mut Vec<ToolCallEvent>,
    ) {
        let update = params.get("update").unwrap_or(params);
        let kind = update
            .get("sessionUpdate")
            .and_then(Value::as_str)
            .unwrap_or_default();

        match kind {
            "agent_message_chunk" => {
                if let Some(content) = update.get("content") {
                    extract_text_chunks(content, collected_text);
                }
            }
            "tool_call" | "tool_call_update" => {
                let tool_call_id = update
                    .get("toolCallId")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let title = update
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let status = update
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or(kind)
                    .to_owned();
                let mut excerpt = String::new();
                if let Some(content) = update.get("content") {
                    extract_text_chunks(content, &mut excerpt);
                }
                if excerpt.is_empty()
                    && let Some(raw_out) = update.get("rawOutput")
                {
                    excerpt = raw_out.to_string();
                }
                tool_events.push(ToolCallEvent {
                    tool_call_id,
                    title,
                    status,
                    output_excerpt: redact_secrets_owned(&excerpt, &self.secrets),
                });
            }
            _ => {}
        }
    }

    async fn write_json(&mut self, value: &Value) -> Result<(), DennettError> {
        let Some(stdin) = self.stdin.as_mut() else {
            return Err(DennettError::RuntimeProtocol {
                code: None,
                detail: String::from("cannot write JSON-RPC message after stdin closed"),
            });
        };
        let mut line = serde_json::to_string(value).map_err(|err| DennettError::Internal {
            detail: format!("failed to serialize JSON-RPC message: {err}"),
        })?;
        line.push('\n');

        let write_fut = async {
            stdin.write_all(line.as_bytes()).await?;
            stdin.flush().await
        };

        time::timeout(ACP_WRITE_TIMEOUT, write_fut)
            .await
            .map_err(|_| DennettError::Timeout {
                operation: String::from("agy acp stdin write"),
            })?
            .map_err(|err| DennettError::RuntimeProtocol {
                code: None,
                detail: format!("failed to write to agy acp stdin: {err}"),
            })?;
        Ok(())
    }

    async fn read_message(&mut self, timeout: Duration) -> Result<Value, DennettError> {
        let deadline = time::Instant::now() + timeout;

        loop {
            let remaining = deadline.saturating_duration_since(time::Instant::now());
            if remaining.is_zero() {
                return Err(DennettError::Timeout {
                    operation: String::from("agy acp stdout read"),
                });
            }

            let maybe_line = time::timeout(
                remaining,
                read_bounded_line(&mut self.stdout, MAX_ACP_STDOUT_LINE_BYTES),
            )
            .await
            .map_err(|_| DennettError::Timeout {
                operation: String::from("agy acp stdout read"),
            })??;

            let Some(line) = maybe_line else {
                let stderr = self.stderr_snapshot();
                return Err(DennettError::RuntimeProtocol {
                    code: None,
                    detail: format!("agy acp closed stdout unexpectedly; stderr: {stderr}"),
                });
            };

            let trimmed = line.trim();
            if trimmed.is_empty() || !trimmed.starts_with('{') {
                continue;
            }

            return serde_json::from_str(trimmed).map_err(|err| DennettError::RuntimeProtocol {
                code: None,
                detail: redact_secrets_owned(
                    &format!("invalid JSON-RPC frame from agy acp: {err}"),
                    &self.secrets,
                ),
            });
        }
    }

    const fn allocate_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

impl Drop for AcpClient {
    fn drop(&mut self) {
        drop(self.stdin.take());
        let _ = self.child.start_kill();

        // Briefly poll `try_wait` so Windows releases open log file handles (e.g. `acp.log`)
        // before sibling RAII `TempProfileDir` destructors remove the scratch directory.
        let deadline = StdInstant::now() + DROP_REAP_BUDGET;
        while StdInstant::now() < deadline {
            match self.child.try_wait() {
                Ok(Some(_)) | Err(_) => break,
                Ok(None) => std::thread::sleep(DROP_REAP_POLL),
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CancelMode {
    RunToCompletion,
    CancelOnFirstUpdate,
}

async fn drain_stderr_bounded<R: AsyncRead + Unpin>(
    mut stderr_pipe: R,
    capture: Arc<Mutex<String>>,
) {
    let mut chunk = [0_u8; STDERR_READ_CHUNK_BYTES];
    loop {
        match stderr_pipe.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if let Ok(mut buf) = capture.lock()
                    && buf.len() < MAX_STDERR_CAPTURE_BYTES
                {
                    let remaining = MAX_STDERR_CAPTURE_BYTES - buf.len();
                    let text = String::from_utf8_lossy(&chunk[..n]);
                    if text.len() <= remaining {
                        buf.push_str(&text);
                    } else {
                        let mut cut = remaining;
                        while cut > 0 && !text.is_char_boundary(cut) {
                            cut -= 1;
                        }
                        buf.push_str(&text[..cut]);
                    }
                }
            }
        }
    }
}

async fn read_bounded_line<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    max_bytes: usize,
) -> Result<Option<String>, DennettError> {
    let mut raw = Vec::new();
    loop {
        let available = reader
            .fill_buf()
            .await
            .map_err(|err| DennettError::RuntimeProtocol {
                code: None,
                detail: format!("failed reading ACP stream: {err}"),
            })?;
        if available.is_empty() {
            if raw.is_empty() {
                return Ok(None);
            }
            break;
        }

        let newline_pos = available.iter().position(|&b| b == b'\n');
        let take_len = newline_pos.map_or(available.len(), |pos| pos + 1);
        if raw.len() + take_len > max_bytes {
            return Err(DennettError::RuntimeProtocol {
                code: None,
                detail: format!("ACP stream line exceeded maximum limit of {max_bytes} bytes"),
            });
        }
        raw.extend_from_slice(&available[..take_len]);
        reader.consume(take_len);
        if newline_pos.is_some() {
            break;
        }
    }

    String::from_utf8(raw)
        .map(Some)
        .map_err(|err| DennettError::RuntimeProtocol {
            code: None,
            detail: format!("ACP stream emitted invalid UTF-8: {err}"),
        })
}

fn extract_json_rpc_error(msg: &Value, secrets: &[String]) -> Option<DennettError> {
    let err_obj = msg.get("error")?;
    let code = err_obj.get("code").and_then(Value::as_i64);
    let message = err_obj
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("unknown JSON-RPC error");
    let data = err_obj.get("data").map_or_default(Value::to_string);
    let combined = if data.is_empty() {
        message.to_owned()
    } else {
        format!("{message} ({data})")
    };
    Some(DennettError::RuntimeProtocol {
        code,
        detail: redact_secrets_owned(&combined, secrets),
    })
}

fn parse_session_new_result(raw: Value) -> Result<SessionNewResult, DennettError> {
    let session_id = raw
        .get("sessionId")
        .and_then(Value::as_str)
        .ok_or_else(|| DennettError::RuntimeProtocol {
            code: None,
            detail: String::from("session/new response missing `sessionId`"),
        })?
        .to_owned();

    let tier = raw
        .get("_meta")
        .and_then(|m| m.get("antigravity"))
        .and_then(|a| a.get("tier"))
        .and_then(|t| match t {
            Value::String(s) if !s.trim().is_empty() => Some(s.clone()),
            Value::Object(map) => map
                .get("name")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .or_else(|| {
                    map.get("id")
                        .and_then(Value::as_str)
                        .filter(|s| !s.trim().is_empty())
                })
                .map(str::to_owned),
            _ => None,
        });

    let mut available_models = Vec::new();
    let mut push_unique = |candidate: &str| {
        if !candidate.is_empty() && !available_models.iter().any(|m| m == candidate) {
            available_models.push(candidate.to_owned());
        }
    };

    // Prefer `configOptions[id=="model"].options` first because `session/set_config_option`
    // expects those exact option values (e.g. `gemini-3.8-flash`), whereas
    // `models.availableModels` may contain effort-suffixed slugs (e.g. `gemini-3.8-flash-high`).
    if let Some(config_options) = raw.get("configOptions").and_then(Value::as_array) {
        for opt in config_options {
            if opt.get("id").and_then(Value::as_str) == Some("model")
                && let Some(values) = opt.get("options").and_then(Value::as_array)
            {
                for item in values {
                    if let Some(val) = item.get("value").and_then(Value::as_str) {
                        push_unique(val);
                    }
                }
            }
        }
    }

    if let Some(models_arr) = raw
        .get("models")
        .and_then(|m| m.get("availableModels"))
        .and_then(Value::as_array)
    {
        for entry in models_arr {
            if let Some(id) = entry
                .get("modelId")
                .or_else(|| entry.get("id"))
                .and_then(Value::as_str)
            {
                push_unique(id);
            }
        }
    }

    Ok(SessionNewResult {
        session_id,
        tier,
        available_models,
        raw,
    })
}

fn parse_permission_request(params: &Value) -> PermissionRequestView {
    let tool_call = params.get("toolCall").unwrap_or(&Value::Null);
    let tool_call_id = tool_call
        .get("toolCallId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let title = tool_call
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let kind = tool_call
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let raw_input = tool_call.get("rawInput").cloned().unwrap_or(Value::Null);

    let options = params
        .get("options")
        .and_then(Value::as_array)
        .map_or_default(|arr| {
            arr.iter()
                .filter_map(|opt| {
                    let option_id = opt.get("optionId").and_then(Value::as_str)?.to_owned();
                    let opt_kind = opt
                        .get("kind")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned();
                    let name = opt
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned();
                    Some(PermissionOption {
                        option_id,
                        kind: opt_kind,
                        name,
                    })
                })
                .collect()
        });

    PermissionRequestView {
        tool_call_id,
        title,
        kind,
        raw_input,
        options,
    }
}

/// Selects a matching ACP permission `optionId` without ever escalating privilege.
///
/// `AllowOnce` matches strictly `kind == "allow_once"` and returns `None` (cancelled) rather
/// than escalating to `allow_always`. Reject decisions may safely fall back across `reject_*`.
fn select_permission_option(
    options: &[PermissionOption],
    decision: PermissionDecision,
) -> Option<String> {
    match decision {
        PermissionDecision::AllowOnce => options
            .iter()
            .find(|opt| opt.kind == "allow_once")
            .map(|opt| opt.option_id.clone()),
        PermissionDecision::AllowAlways => options
            .iter()
            .find(|opt| opt.kind == "allow_always")
            .map(|opt| opt.option_id.clone()),
        PermissionDecision::RejectOnce => options
            .iter()
            .find(|opt| opt.kind == "reject_once")
            .or_else(|| options.iter().find(|opt| opt.kind == "reject_always"))
            .map(|opt| opt.option_id.clone()),
        PermissionDecision::RejectAlways => options
            .iter()
            .find(|opt| opt.kind == "reject_always")
            .or_else(|| options.iter().find(|opt| opt.kind == "reject_once"))
            .map(|opt| opt.option_id.clone()),
    }
}

fn extract_text_chunks(content: &Value, dst: &mut String) {
    match content {
        Value::Object(map) => {
            if let Some(text) = map.get("text").and_then(Value::as_str) {
                dst.push_str(text);
            } else if let Some(inner) = map.get("content") {
                extract_text_chunks(inner, dst);
            }
        }
        Value::Array(items) => {
            for item in items {
                extract_text_chunks(item, dst);
            }
        }
        Value::String(s) => dst.push_str(s),
        _ => {}
    }
}

fn clean_path_for_acp(path: &Path) -> String {
    let raw = path.to_string_lossy();
    raw.strip_prefix(r"\\?\").unwrap_or(&raw).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opt(id: &str, kind: &str) -> PermissionOption {
        PermissionOption {
            option_id: id.to_owned(),
            kind: kind.to_owned(),
            name: kind.to_owned(),
        }
    }

    #[test]
    fn select_permission_option_never_escalates_allow_once_to_allow_always() {
        let only_always = vec![
            opt("opt-always", "allow_always"),
            opt("opt-rej", "reject_once"),
        ];
        assert_eq!(
            select_permission_option(&only_always, PermissionDecision::AllowOnce),
            None,
            "AllowOnce must return None (cancel) rather than escalating to allow_always"
        );

        let full = vec![
            opt("opt-once", "allow_once"),
            opt("opt-always", "allow_always"),
            opt("opt-rej-always", "reject_always"),
        ];
        assert_eq!(
            select_permission_option(&full, PermissionDecision::AllowOnce).as_deref(),
            Some("opt-once")
        );
        assert_eq!(
            select_permission_option(&full, PermissionDecision::RejectOnce).as_deref(),
            Some("opt-rej-always"),
            "RejectOnce may safely fall back to stricter reject_always"
        );
    }

    #[tokio::test]
    async fn read_bounded_line_reads_normal_and_rejects_oversized() {
        let mut ok_reader = BufReader::new(&b"{\"ok\":true}\nsecond\n"[..]);
        let first = match read_bounded_line(&mut ok_reader, 64).await {
            Ok(v) => v,
            Err(err) => panic!("expected first line: {err}"),
        };
        assert_eq!(first.as_deref(), Some("{\"ok\":true}\n"));

        let oversized = vec![b'x'; 256];
        let mut bad_reader = BufReader::new(&oversized[..]);
        let err = read_bounded_line(&mut bad_reader, 64).await;
        assert!(
            matches!(err, Err(DennettError::RuntimeProtocol { .. })),
            "oversized line must fail closed without allocating unbounded memory, got: {err:?}"
        );
    }

    #[test]
    fn parse_session_new_result_extracts_tier_and_available_models() {
        let payload = json!({
            "sessionId": "sess-123",
            "_meta": { "antigravity": { "tier": "Google AI Ultra" } },
            "models": {
                "availableModels": [
                    { "modelId": "gemini-3.8-pro" },
                    { "modelId": "gemini-3.8-flash" }
                ]
            },
            "configOptions": [
                {
                    "id": "model",
                    "options": [
                        { "value": "gemini-3.8-flash" },
                        { "value": "gemini-3.9-flash-lite" }
                    ]
                }
            ]
        });

        let parsed = match parse_session_new_result(payload) {
            Ok(p) => p,
            Err(err) => panic!("parse_session_new_result failed: {err}"),
        };
        assert_eq!(parsed.session_id, "sess-123");
        assert_eq!(parsed.tier.as_deref(), Some("Google AI Ultra"));
        assert_eq!(
            parsed.available_models,
            vec![
                String::from("gemini-3.8-flash"),
                String::from("gemini-3.9-flash-lite"),
                String::from("gemini-3.8-pro"),
            ]
        );
    }

    #[test]
    fn parse_session_new_result_handles_object_tier_from_live_agy() {
        let payload = json!({
            "sessionId": "sess-live-1515",
            "_meta": {
                "antigravity": {
                    "tier": {
                        "name": "Google AI Ultra",
                        "id": "g1-ultra-tier",
                        "upgradeUri": ""
                    }
                }
            },
            "models": {
                "availableModels": [
                    { "modelId": "gemini-3.8-flash-high" }
                ]
            },
            "configOptions": [
                {
                    "id": "model",
                    "options": [
                        { "value": "gemini-3.8-flash" }
                    ]
                }
            ]
        });

        let parsed = match parse_session_new_result(payload) {
            Ok(p) => p,
            Err(err) => panic!("parse_session_new_result failed: {err}"),
        };
        assert_eq!(parsed.tier.as_deref(), Some("Google AI Ultra"));
        assert_eq!(
            parsed.available_models.first().map(String::as_str),
            Some("gemini-3.8-flash")
        );
    }

    #[test]
    fn extract_json_rpc_error_redacts_secrets() {
        let msg = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "error": {
                "code": -32000,
                "message": "Authentication required",
                "data": "Authorization: Bearer ya29.secret_oauth_token_123456 GEMINI_API_KEY=AIzaSySecretKeyValue1234567890abcd"
            }
        });
        let Some(DennettError::RuntimeProtocol { code, detail }) =
            extract_json_rpc_error(&msg, &[String::from("explicit-tok")])
        else {
            panic!("expected RuntimeProtocol error");
        };
        assert_eq!(code, Some(-32000));
        assert!(!detail.contains("ya29.secret_oauth_token_123456"));
        assert!(!detail.contains("AIzaSySecretKeyValue1234567890abcd"));
        assert!(detail.contains("Authentication required"));
    }
}
