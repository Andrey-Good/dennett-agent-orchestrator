//! Bounded JSON-RPC 2.0 over stdio transport for Google Antigravity (`agy acp`, ACP v1).
//!
//! Enforces strict line/stderr bounds, secret redaction, `--dangerously-skip-permissions`
//! rejection, non-escalating `AllowOnce` permission selection, and race-free `session/cancel`
//! handling that rejects late permission approvals after cancellation.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant as StdInstant};

use dennett_contracts::redact::redact_secrets_owned;
use dennett_contracts::{
    DennettError, PolicyEffect, RunId, RunStatus, RuntimeAuthProfile, RuntimeModelOption,
    RuntimeQuotaWindow, RuntimeThinkingOption, SessionId, ThinkingLevel,
};
use serde_json::{Value, json};
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader,
};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{mpsc, watch};
use tokio::time;

use crate::port::{
    PolicyEvaluationRequest, PolicyEvaluator, RuntimeEvent, RuntimeMcpServerSpec, TurnOutcome,
};
use crate::profile::normalize_workspace_path;

/// Maximum size of a single JSON-RPC line accepted from `agy acp` stdout (4 MiB).
pub const MAX_ACP_STDOUT_LINE_BYTES: usize = 4 * 1024 * 1024;
/// Maximum bytes retained from child stderr for redacted diagnostics (64 KiB).
const MAX_STDERR_CAPTURE_BYTES: usize = 64 * 1024;
/// Chunk size for bounded stderr draining.
const STDERR_READ_CHUNK_BYTES: usize = 4 * 1024;
/// Timeout applied when writing a JSON-RPC frame to `agy acp` stdin.
const ACP_WRITE_TIMEOUT: Duration = Duration::from_secs(10);
/// Graceful wait after `session/cancel` for `agy acp` to acknowledge cancellation.
const CANCEL_GRACE_TIMEOUT: Duration = Duration::from_secs(5);
/// Graceful wait before forcefully killing `agy acp` during shutdown.
const SHUTDOWN_GRACE_TIMEOUT: Duration = Duration::from_secs(3);
/// Maximum synchronous wait in `Drop` after `start_kill` so Windows file handles close.
const DROP_REAP_BUDGET: Duration = Duration::from_millis(200);
/// Poll interval during `Drop` reaping.
const DROP_REAP_POLL: Duration = Duration::from_millis(10);

/// Sensitive host environment variables scrubbed from spawned `agy acp` processes by default.
const DEFAULT_SCRUBBED_ENV_VARS: [&str; 9] = [
    "GEMINI_API_KEY",
    "GOOGLE_API_KEY",
    "DENNETT_MEMORY_PG_URL",
    "DENNETT_CONTROL_PG_URL",
    "DENNETT_TEST_MEMORY_PG_URL",
    "DENNETT_TEST_CONTROL_PG_URL",
    "PGPASSWORD",
    "DENNETT_MEMORY_TOKEN",
    "DENNETT_HEAD_TOKEN",
];

/// Spawn configuration for a single isolated `agy acp` child process.
#[derive(Debug, Clone)]
pub struct AcpProcessConfig {
    /// Path to the `agy` binary.
    pub binary: PathBuf,
    /// Arguments passed after `acp`.
    pub extra_args: Vec<String>,
    /// Working directory for the child process.
    pub working_dir: PathBuf,
    /// Explicit environment variable overrides (applied after default scrubbing).
    pub env_overrides: Vec<(String, String)>,
    /// Extra secret values scrubbed from diagnostics and tool output excerpts.
    pub secrets_to_redact: Vec<String>,
}

/// Summary returned by ACP `initialize`.
#[derive(Debug, Clone)]
pub struct AcpInitializeInfo {
    /// Negotiated ACP protocol version.
    pub protocol_version: u64,
    /// Server name from `agentInfo.name`.
    pub agent_name: String,
    /// Server version from `agentInfo.version`.
    pub agent_version: String,
    /// Advertised `authMethods[*].id`.
    pub auth_method_ids: Vec<String>,
    /// Whether `loadSession` / `sessionCapabilities.load` is advertised.
    pub supports_load_session: bool,
    /// Whether `_meta.antigravity.systemPrompt.append` is advertised.
    pub supports_system_prompt_append: bool,
}

/// Summary returned by ACP `session/new`, `session/load`, or `session/resume`.
#[derive(Debug, Clone)]
pub struct AcpSessionInfo {
    /// Provider session identifier.
    pub session_id: String,
    /// Subscription tier from `_meta.antigravity.tier`, when present.
    pub tier: Option<String>,
    /// Available model identifiers advertised in `configOptions[id=="model"]` or `models`.
    pub available_models: Vec<String>,
    /// Structured model families with per-model `supported_thinking_levels`.
    pub models: Vec<RuntimeModelOption>,
    /// Global thinking-level options advertised in `configOptions[id=="thought_level"]`.
    pub thinking_options: Vec<RuntimeThinkingOption>,
    /// Default model family identifier (`configOptions[id=="model"].currentValue`).
    pub default_model_id: Option<String>,
    /// Default thinking level (`configOptions[id=="thought_level"].currentValue`).
    pub default_thinking_level: Option<ThinkingLevel>,
    /// Optional machine-readable quota windows from `_meta.antigravity.quotas`.
    pub quotas: Vec<RuntimeQuotaWindow>,
    /// Currently active `thought_level` values reported in this session response (`None` if `configOptions` was omitted).
    pub active_thinking_levels: Option<Vec<ThinkingLevel>>,
}

/// One permission option advertised in ACP `session/request_permission`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpPermissionOption {
    /// Opaque option identifier echoed back in `selected.optionId`.
    pub option_id: String,
    /// Option kind (`allow_once`, `allow_always`, `reject_once`, `reject_always`).
    pub kind: String,
    /// Human-readable option label.
    pub name: String,
}

/// Parsed ACP `session/request_permission` payload.
#[derive(Debug, Clone)]
pub struct AcpPermissionRequest {
    /// Tool call identifier.
    pub tool_call_id: String,
    /// Normalized tool name (`toolCall.name` with safe fallback).
    pub tool_name: String,
    /// Tool call kind (`edit`, `execute`, `read`, `other`).
    pub kind: String,
    /// Tool call display title.
    pub title: String,
    /// Extracted target path, command line, or scope when present.
    pub target: Option<String>,
    /// All extracted candidate filesystem locations from `toolCall.locations[*]` and path args.
    pub locations: Vec<String>,
    /// Raw JSON input payload.
    pub raw_input: Value,
    /// Advertised permission options.
    pub options: Vec<AcpPermissionOption>,
}

/// Active stdio JSON-RPC 2.0 connection to one isolated `agy acp` subprocess.
pub struct AcpProcess {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    stderr_buf: Arc<Mutex<String>>,
    next_id: u64,
    secrets: Vec<String>,
}

/// Bundled parameters for [`AcpProcess::run_prompt_turn`].
pub struct AcpTurnParams<'a> {
    /// Owning Dennett session identifier.
    pub dennett_session_id: SessionId,
    /// Active ACP provider session identifier.
    pub provider_session_id: &'a str,
    /// Execution run identifier.
    pub run_id: RunId,
    /// Prompt text to send to `session/prompt`.
    pub prompt_text: &'a str,
    /// Turn timeout.
    pub timeout: Duration,
    /// Live cancellation channel receiver.
    pub cancel_rx: watch::Receiver<bool>,
    /// External policy evaluator callback.
    pub policy: Arc<dyn PolicyEvaluator>,
    /// Optional event stream channel.
    pub event_tx: Option<mpsc::Sender<RuntimeEvent>>,
}

impl AcpProcess {
    /// Spawns `agy acp` with scrubbed environment and strict safety checks.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::SafetyGuardRejected`] if `--dangerously-skip-permissions`
    /// appears anywhere in `extra_args`, or [`DennettError::RuntimeProtocol`] if spawning fails.
    pub fn spawn(config: &AcpProcessConfig) -> Result<Self, DennettError> {
        if config.extra_args.iter().any(|arg| {
            arg == "--dangerously-skip-permissions"
                || arg.starts_with("--dangerously-skip-permissions=")
        }) {
            return Err(DennettError::SafetyGuardRejected {
                reason: String::from(
                    "`--dangerously-skip-permissions` is strictly forbidden in production Dennett runtime",
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

        for sensitive in DEFAULT_SCRUBBED_ENV_VARS {
            cmd.env_remove(sensitive);
        }
        for (k, v) in &config.env_overrides {
            cmd.env(k, v);
        }

        let mut child = cmd.spawn().map_err(|err| DennettError::RuntimeProtocol {
            code: None,
            detail: redact_secrets_owned(
                &format!("failed to spawn `{}`: {err}", config.binary.display()),
                &config.secrets_to_redact,
            ),
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| DennettError::RuntimeProtocol {
                code: None,
                detail: String::from("agy acp child stdin handle missing"),
            })?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| DennettError::RuntimeProtocol {
                code: None,
                detail: String::from("agy acp child stdout handle missing"),
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

    /// Appends additional secret literals (e.g. MCP bearer tokens) to be redacted from all
    /// subsequent error messages and tool output excerpts.
    pub fn add_secrets(&mut self, extra: impl IntoIterator<Item = String>) {
        for item in extra {
            if !item.is_empty() && !self.secrets.contains(&item) {
                self.secrets.push(item);
            }
        }
    }

    /// Returns `true` if the child process has already exited.
    pub fn has_exited(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)))
    }

    /// Returns a redacted snapshot of captured stderr output.
    #[must_use]
    pub fn stderr_snapshot(&self) -> String {
        snapshot_stderr_buf(&self.stderr_buf, &self.secrets)
    }

    /// Runs the ACP v1 `initialize` handshake.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::RuntimeProtocol`] or [`DennettError::Timeout`].
    pub async fn initialize(
        &mut self,
        timeout: Duration,
    ) -> Result<AcpInitializeInfo, DennettError> {
        let params = json!({
            "protocolVersion": 1,
            "clientCapabilities": {},
            "clientInfo": {
                "name": "dennett-head",
                "version": env!("CARGO_PKG_VERSION")
            }
        });
        let raw = self.request("initialize", params, timeout).await?;

        let protocol_version = raw
            .get("protocolVersion")
            .and_then(Value::as_u64)
            .ok_or_else(|| DennettError::RuntimeProtocol {
                code: None,
                detail: String::from("ACP initialize response missing numeric `protocolVersion`"),
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
        let caps = raw.get("agentCapabilities").unwrap_or(&Value::Null);
        let supports_load_session = caps
            .get("loadSession")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            || caps
                .get("sessionCapabilities")
                .and_then(|s| s.get("load"))
                .is_some();
        let supports_system_prompt_append = caps
            .get("_meta")
            .and_then(|m| m.get("antigravity"))
            .and_then(|a| a.get("systemPrompt"))
            .and_then(|sp| sp.get("append"))
            .and_then(Value::as_bool)
            .unwrap_or(false);

        Ok(AcpInitializeInfo {
            protocol_version,
            agent_name,
            agent_version,
            auth_method_ids,
            supports_load_session,
            supports_system_prompt_append,
        })
    }

    /// Calls ACP `authenticate` for the explicitly selected [`RuntimeAuthProfile`].
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::RuntimeProtocol`] (preserving JSON-RPC codes `-32602`/`-32000`)
    /// or [`DennettError::Timeout`].
    pub async fn authenticate(
        &mut self,
        profile: RuntimeAuthProfile,
        timeout: Duration,
    ) -> Result<Value, DennettError> {
        self.request(
            "authenticate",
            json!({ "methodId": profile.as_method_id() }),
            timeout,
        )
        .await
    }

    /// Creates a new ACP session (`session/new`) with explicit `cwd`, `mcpServers`, and
    /// `_meta.antigravity.systemPrompt.append`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::RuntimeProtocol`] or [`DennettError::Timeout`].
    pub async fn session_new(
        &mut self,
        cwd: &Path,
        mcp_servers: &[RuntimeMcpServerSpec],
        system_prompt_append: Option<&str>,
        timeout: Duration,
    ) -> Result<AcpSessionInfo, DennettError> {
        let cwd_str = normalize_workspace_path(cwd);
        let mut params = json!({
            "cwd": cwd_str,
            "mcpServers": mcp_servers,
        });
        if let Some(prompt) = system_prompt_append
            && !prompt.trim().is_empty()
            && let Some(obj) = params.as_object_mut()
        {
            obj.insert(
                String::from("_meta"),
                json!({
                    "antigravity": {
                        "systemPrompt": {
                            "append": prompt
                        }
                    }
                }),
            );
        }

        let raw = self.request("session/new", params, timeout).await?;
        parse_session_info(&raw, None)
    }

    /// Attempts to explicitly load an existing ACP session by exact `session_id` (`session/load`).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::RuntimeProtocol`] (e.g. code `-32002` when session not found)
    /// or [`DennettError::Timeout`].
    pub async fn session_load(
        &mut self,
        session_id: &str,
        cwd: &Path,
        mcp_servers: &[RuntimeMcpServerSpec],
        timeout: Duration,
    ) -> Result<AcpSessionInfo, DennettError> {
        let cwd_str = normalize_workspace_path(cwd);
        let params = json!({
            "sessionId": session_id,
            "cwd": cwd_str,
            "mcpServers": mcp_servers,
        });
        let raw = self.request("session/load", params, timeout).await?;
        parse_session_info(&raw, Some(session_id))
    }

    /// Sets an ACP session configuration option (`model`, `thought_level`, `mode`).
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
        self.request(
            "session/set_config_option",
            json!({
                "sessionId": session_id,
                "configId": config_id,
                "value": value
            }),
            timeout,
        )
        .await
    }

    /// Executes one `session/prompt` turn with external [`PolicyEvaluator`], optional streaming
    /// event sink, and live cancellation via `cancel_rx`.
    ///
    /// Returns `(TurnOutcome, needs_process_reset)` where `needs_process_reset == true` signals
    /// that cancellation exceeded grace timeout and the child process should be killed to
    /// guarantee no lingering background execution.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError`] on JSON-RPC error, unexpected EOF, nonzero exit, invalid UTF-8,
    /// oversized frame, or timeout — NEVER converts partial output into [`RunStatus::Succeeded`].
    pub async fn run_prompt_turn(
        &mut self,
        params: AcpTurnParams<'_>,
    ) -> Result<(TurnOutcome, bool), DennettError> {
        let req_id = self.allocate_id();
        let Some(stdin) = self.stdin.as_mut() else {
            return Err(DennettError::RuntimeProtocol {
                code: None,
                detail: String::from("cannot write session/prompt after stdin closed"),
            });
        };
        let stderr_buf = Arc::clone(&self.stderr_buf);
        let secrets = &self.secrets;

        exchange_prompt_turn_io(
            &mut self.stdout,
            stdin,
            PromptTurnIoArgs {
                req_id,
                dennett_session_id: params.dennett_session_id,
                provider_session_id: params.provider_session_id,
                run_id: params.run_id,
                prompt_text: params.prompt_text,
                timeout: params.timeout,
                secrets,
            },
            params.cancel_rx,
            params.policy,
            params.event_tx,
            move || snapshot_stderr_buf(&stderr_buf, secrets),
        )
        .await
    }

    /// Gracefully closes stdin and waits briefly before forcefully killing `agy acp`.
    pub async fn shutdown(mut self) {
        drop(self.stdin.take());
        if time::timeout(SHUTDOWN_GRACE_TIMEOUT, self.child.wait())
            .await
            .is_err()
        {
            let _ = self.child.kill().await;
        }
    }

    async fn request(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, DennettError> {
        let req_id = self.allocate_id();
        let Some(stdin) = self.stdin.as_mut() else {
            return Err(DennettError::RuntimeProtocol {
                code: None,
                detail: String::from("cannot send JSON-RPC request after stdin closed"),
            });
        };
        let stderr_buf = Arc::clone(&self.stderr_buf);
        let secrets = &self.secrets;
        exchange_request_io(
            &mut self.stdout,
            stdin,
            RpcRequestIoArgs {
                req_id,
                method,
                params,
                timeout,
                secrets,
            },
            move || snapshot_stderr_buf(&stderr_buf, secrets),
        )
        .await
    }

    const fn allocate_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

impl Drop for AcpProcess {
    fn drop(&mut self) {
        drop(self.stdin.take());
        let _ = self.child.start_kill();

        let deadline = StdInstant::now() + DROP_REAP_BUDGET;
        while StdInstant::now() < deadline {
            match self.child.try_wait() {
                Ok(Some(_)) | Err(_) => break,
                Ok(None) => std::thread::sleep(DROP_REAP_POLL),
            }
        }
    }
}

struct RpcRequestIoArgs<'a> {
    req_id: u64,
    method: &'a str,
    params: Value,
    timeout: Duration,
    secrets: &'a [String],
}

/// Internal arguments for [`exchange_prompt_turn_io`], separated for in-memory stream unit testing.
pub(crate) struct PromptTurnIoArgs<'a> {
    pub(crate) req_id: u64,
    pub(crate) dennett_session_id: SessionId,
    pub(crate) provider_session_id: &'a str,
    pub(crate) run_id: RunId,
    pub(crate) prompt_text: &'a str,
    pub(crate) timeout: Duration,
    pub(crate) secrets: &'a [String],
}

struct TurnAccumulator {
    assistant_text: String,
    events: Vec<RuntimeEvent>,
    cancelled: bool,
    cancel_sent: bool,
}

impl TurnAccumulator {
    fn new() -> Self {
        Self {
            assistant_text: String::new(),
            events: Vec::new(),
            cancelled: false,
            cancel_sent: false,
        }
    }

    fn emit(&mut self, event: RuntimeEvent, sink: Option<&mpsc::Sender<RuntimeEvent>>) {
        if let Some(tx) = sink {
            let _ = tx.try_send(event.clone());
        }
        self.events.push(event);
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "ACP prompt turn loop coordinates live cancellation, bounded reads, and terminal error classification"
)]
pub(crate) async fn exchange_prompt_turn_io<R, W>(
    reader: &mut R,
    writer: &mut W,
    args: PromptTurnIoArgs<'_>,
    mut cancel_rx: watch::Receiver<bool>,
    policy: Arc<dyn PolicyEvaluator>,
    event_tx: Option<mpsc::Sender<RuntimeEvent>>,
    stderr_hint: impl Fn() -> String,
) -> Result<(TurnOutcome, bool), DennettError>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut acc = TurnAccumulator::new();

    // If already cancelled before dispatch, return Cancelled immediately without calling provider.
    if *cancel_rx.borrow() {
        acc.emit(
            RuntimeEvent::Cancelled {
                reason: String::from("turn cancelled before dispatch"),
            },
            event_tx.as_ref(),
        );
        return Ok((
            TurnOutcome {
                status: RunStatus::Cancelled,
                stop_reason: String::from("cancelled"),
                assistant_text: String::new(),
                events: acc.events,
                usage: None,
            },
            false,
        ));
    }

    write_json_to(
        writer,
        &json!({
            "jsonrpc": "2.0",
            "id": args.req_id,
            "method": "session/prompt",
            "params": {
                "sessionId": args.provider_session_id,
                "prompt": [{ "type": "text", "text": args.prompt_text }]
            }
        }),
    )
    .await?;

    let mut deadline = time::Instant::now() + args.timeout;

    loop {
        let now = time::Instant::now();
        let remaining = deadline.saturating_duration_since(now);
        if remaining.is_zero() {
            if acc.cancelled {
                acc.emit(
                    RuntimeEvent::Cancelled {
                        reason: String::from(
                            "cancelled after grace window; resetting runtime process",
                        ),
                    },
                    event_tx.as_ref(),
                );
                return Ok((
                    TurnOutcome {
                        status: RunStatus::Cancelled,
                        stop_reason: String::from("cancelled"),
                        assistant_text: acc.assistant_text,
                        events: acc.events,
                        usage: None,
                    },
                    true,
                ));
            }
            return Err(DennettError::Timeout {
                operation: String::from("session/prompt"),
            });
        }

        let read_result = if acc.cancelled {
            read_message_from(reader, remaining, args.secrets, &stderr_hint).await
        } else {
            tokio::select! {
                biased;
                changed = cancel_rx.changed() => {
                    let is_cancelled = changed.is_ok() && *cancel_rx.borrow();
                    if is_cancelled {
                        acc.cancelled = true;
                        if !acc.cancel_sent {
                            let _ = write_json_to(
                                writer,
                                &json!({
                                    "jsonrpc": "2.0",
                                    "method": "session/cancel",
                                    "params": { "sessionId": args.provider_session_id }
                                }),
                            )
                            .await;
                            acc.cancel_sent = true;
                        }
                        let grace_deadline = time::Instant::now() + CANCEL_GRACE_TIMEOUT;
                        if grace_deadline < deadline {
                            deadline = grace_deadline;
                        }
                    }
                    continue;
                }
                msg = read_message_from(reader, remaining, args.secrets, &stderr_hint) => msg,
            }
        };

        let msg = match read_result {
            Ok(m) => m,
            Err(DennettError::Timeout { .. }) if acc.cancelled => {
                acc.emit(
                    RuntimeEvent::Cancelled {
                        reason: String::from(
                            "cancelled after grace timeout; resetting runtime process",
                        ),
                    },
                    event_tx.as_ref(),
                );
                return Ok((
                    TurnOutcome {
                        status: RunStatus::Cancelled,
                        stop_reason: String::from("cancelled"),
                        assistant_text: acc.assistant_text,
                        events: acc.events,
                        usage: None,
                    },
                    true,
                ));
            }
            Err(err) => {
                acc.emit(
                    RuntimeEvent::Error {
                        code: match &err {
                            DennettError::RuntimeProtocol { code, .. } => *code,
                            _ => None,
                        },
                        detail: redact_secrets_owned(&err.to_string(), args.secrets),
                    },
                    event_tx.as_ref(),
                );
                return Err(err);
            }
        };

        // Check for final response to our `session/prompt` request.
        if msg.get("id").and_then(Value::as_u64) == Some(args.req_id) && msg.get("method").is_none()
        {
            let result = match unwrap_rpc_result(&msg, args.secrets) {
                Ok(r) => r,
                Err(err) => {
                    if acc.cancelled {
                        acc.emit(
                            RuntimeEvent::Cancelled {
                                reason: String::from("turn cancelled"),
                            },
                            event_tx.as_ref(),
                        );
                        return Ok((
                            TurnOutcome {
                                status: RunStatus::Cancelled,
                                stop_reason: String::from("cancelled"),
                                assistant_text: acc.assistant_text,
                                events: acc.events,
                                usage: None,
                            },
                            false,
                        ));
                    }
                    return Err(err);
                }
            };

            let stop_reason = result
                .get("stopReason")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let usage = result.get("usage").cloned();
            if let Some(ref u) = usage {
                acc.emit(
                    RuntimeEvent::UsageReported { usage: u.clone() },
                    event_tx.as_ref(),
                );
            }

            if acc.cancelled || stop_reason.eq_ignore_ascii_case("cancelled") {
                acc.emit(
                    RuntimeEvent::Cancelled {
                        reason: if stop_reason.is_empty() {
                            String::from("cancelled")
                        } else {
                            stop_reason.clone()
                        },
                    },
                    event_tx.as_ref(),
                );
                return Ok((
                    TurnOutcome {
                        status: RunStatus::Cancelled,
                        stop_reason: if stop_reason.is_empty() {
                            String::from("cancelled")
                        } else {
                            stop_reason
                        },
                        assistant_text: acc.assistant_text,
                        events: acc.events,
                        usage,
                    },
                    false,
                ));
            }

            if stop_reason.eq_ignore_ascii_case("error")
                || stop_reason.eq_ignore_ascii_case("failed")
            {
                return Err(DennettError::RuntimeProtocol {
                    code: None,
                    detail: format!("agy acp turn terminated with stopReason `{stop_reason}`"),
                });
            }

            acc.emit(
                RuntimeEvent::Completed {
                    stop_reason: stop_reason.clone(),
                },
                event_tx.as_ref(),
            );
            return Ok((
                TurnOutcome {
                    status: RunStatus::Succeeded,
                    stop_reason,
                    assistant_text: acc.assistant_text,
                    events: acc.events,
                    usage,
                },
                false,
            ));
        }

        // Uncorrelated JSON-RPC error frame (`id: null` or omitted id with `"error"`) MUST fail closed!
        if is_uncorrelated_error_frame(&msg) {
            return Err(
                extract_json_rpc_error(&msg, args.secrets).unwrap_or_else(|| {
                    DennettError::RuntimeProtocol {
                        code: None,
                        detail: String::from("received uncorrelated JSON-RPC error frame"),
                    }
                }),
            );
        }

        // Handle streaming `session/update` notifications and `session/request_permission` callbacks.
        handle_turn_server_message(
            writer,
            &msg,
            &args,
            &mut acc,
            &policy,
            event_tx.as_ref(),
            &cancel_rx,
        )
        .await?;
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "server message handler keeps cancellation-race guards and non-escalating permission replies adjacent"
)]
async fn handle_turn_server_message<W: AsyncWrite + Unpin>(
    writer: &mut W,
    msg: &Value,
    args: &PromptTurnIoArgs<'_>,
    acc: &mut TurnAccumulator,
    policy: &Arc<dyn PolicyEvaluator>,
    event_tx: Option<&mpsc::Sender<RuntimeEvent>>,
    cancel_rx: &watch::Receiver<bool>,
) -> Result<(), DennettError> {
    let Some(method) = msg.get("method").and_then(Value::as_str) else {
        return Ok(());
    };

    match method {
        "session/update" => {
            record_session_update(
                msg.get("params").unwrap_or(&Value::Null),
                args.secrets,
                acc,
                event_tx,
            );
        }
        "session/request_permission" => {
            if let Some(server_req_id) = msg.get("id").filter(|id| !id.is_null()).cloned() {
                let parsed = parse_permission_request(msg.get("params").unwrap_or(&Value::Null));
                acc.emit(
                    RuntimeEvent::PermissionRequested {
                        tool_call_id: parsed.tool_call_id.clone(),
                        tool_name: parsed.tool_name.clone(),
                        target: parsed.target.clone(),
                        title: parsed.title.clone(),
                    },
                    event_tx,
                );

                // A06 invariant: if cancellation was already requested, NEVER grant approval;
                // immediately abandon pending permissions with `cancelled` without invoking PolicyEngine.
                if acc.cancelled || *cancel_rx.borrow() {
                    acc.cancelled = true;
                    write_json_to(
                        writer,
                        &json!({
                            "jsonrpc": "2.0",
                            "id": server_req_id,
                            "result": {
                                "outcome": {
                                    "outcome": "cancelled"
                                }
                            }
                        }),
                    )
                    .await?;
                    acc.emit(
                        RuntimeEvent::PermissionResolved {
                            tool_call_id: parsed.tool_call_id,
                            tool_name: parsed.tool_name,
                            target: parsed.target,
                            effect: PolicyEffect::Deny,
                            matched_rule_id: None,
                            reason: String::from("abandoned because run was cancelled"),
                            scope: format!("session:{}", args.dennett_session_id),
                            chosen_option_id: None,
                        },
                        event_tx,
                    );
                    return Ok(());
                }

                let eval_req = PolicyEvaluationRequest {
                    session_id: args.dennett_session_id,
                    run_id: args.run_id,
                    tool_call_id: parsed.tool_call_id.clone(),
                    tool_name: parsed.tool_name.clone(),
                    tool_kind: parsed.kind.clone(),
                    title: parsed.title.clone(),
                    target: parsed.target.clone(),
                    locations: parsed.locations.clone(),
                    raw_input: parsed.raw_input.clone(),
                };
                let outcome = policy.evaluate(&eval_req);

                // Re-check cancellation right after evaluation so a late approval never slips through.
                if acc.cancelled || *cancel_rx.borrow() {
                    acc.cancelled = true;
                    write_json_to(
                        writer,
                        &json!({
                            "jsonrpc": "2.0",
                            "id": server_req_id,
                            "result": {
                                "outcome": {
                                    "outcome": "cancelled"
                                }
                            }
                        }),
                    )
                    .await?;
                    acc.emit(
                        RuntimeEvent::PermissionResolved {
                            tool_call_id: parsed.tool_call_id,
                            tool_name: parsed.tool_name,
                            target: parsed.target,
                            effect: PolicyEffect::Deny,
                            matched_rule_id: outcome.matched_rule_id,
                            reason: String::from("late approval rejected after cancellation"),
                            scope: outcome.scope,
                            chosen_option_id: None,
                        },
                        event_tx,
                    );
                    return Ok(());
                }

                let chosen = select_permission_option(&parsed.options, outcome.effect);
                let result_json = match &chosen {
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
                write_json_to(
                    writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": server_req_id,
                        "result": result_json
                    }),
                )
                .await?;

                let effective_decision =
                    if outcome.effect == PolicyEffect::Allow && chosen.is_none() {
                        PolicyEffect::Deny
                    } else {
                        outcome.effect
                    };
                let effective_reason = if outcome.effect == PolicyEffect::Allow && chosen.is_none()
                {
                    format!(
                        "{} (cancelled: provider offered no non-escalating `allow_once` option)",
                        outcome.reason
                    )
                } else {
                    outcome.reason
                };

                acc.emit(
                    RuntimeEvent::PermissionResolved {
                        tool_call_id: parsed.tool_call_id,
                        tool_name: parsed.tool_name,
                        target: parsed.target,
                        effect: effective_decision,
                        matched_rule_id: outcome.matched_rule_id,
                        reason: redact_secrets_owned(&effective_reason, args.secrets),
                        scope: outcome.scope,
                        chosen_option_id: chosen,
                    },
                    event_tx,
                );
            }
        }
        _ => {
            if let Some(server_req_id) = msg.get("id").filter(|id| !id.is_null()).cloned() {
                write_json_to(
                    writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": server_req_id,
                        "error": {
                            "code": -32601,
                            "message": format!("unsupported client method `{method}`")
                        }
                    }),
                )
                .await?;
            }
        }
    }
    Ok(())
}

async fn exchange_request_io<R: AsyncBufRead + Unpin, W: AsyncWrite + Unpin>(
    reader: &mut R,
    writer: &mut W,
    req: RpcRequestIoArgs<'_>,
    stderr_hint: impl Fn() -> String,
) -> Result<Value, DennettError> {
    write_json_to(
        writer,
        &json!({
            "jsonrpc": "2.0",
            "id": req.req_id,
            "method": req.method,
            "params": req.params
        }),
    )
    .await?;

    let deadline = time::Instant::now() + req.timeout;
    loop {
        let remaining = deadline.saturating_duration_since(time::Instant::now());
        if remaining.is_zero() {
            return Err(DennettError::Timeout {
                operation: req.method.to_owned(),
            });
        }
        let msg = read_message_from(reader, remaining, req.secrets, &stderr_hint).await?;

        if msg.get("id").and_then(Value::as_u64) == Some(req.req_id) && msg.get("method").is_none()
        {
            return unwrap_rpc_result(&msg, req.secrets);
        }

        if is_uncorrelated_error_frame(&msg) {
            return Err(
                extract_json_rpc_error(&msg, req.secrets).unwrap_or_else(|| {
                    DennettError::RuntimeProtocol {
                        code: None,
                        detail: String::from("received uncorrelated JSON-RPC error frame"),
                    }
                }),
            );
        }

        if let Some(server_method) = msg.get("method").and_then(Value::as_str)
            && let Some(server_req_id) = msg.get("id").filter(|id| !id.is_null()).cloned()
        {
            write_json_to(
                writer,
                &json!({
                    "jsonrpc": "2.0",
                    "id": server_req_id,
                    "error": {
                        "code": -32601,
                        "message": format!("unsupported client method `{server_method}`")
                    }
                }),
            )
            .await?;
        }
    }
}

fn record_session_update(
    params: &Value,
    secrets: &[String],
    acc: &mut TurnAccumulator,
    event_tx: Option<&mpsc::Sender<RuntimeEvent>>,
) {
    let update = params.get("update").unwrap_or(params);
    let kind = update
        .get("sessionUpdate")
        .and_then(Value::as_str)
        .unwrap_or_default();

    match kind {
        "agent_message_chunk" => {
            let mut chunk = String::new();
            if let Some(content) = update.get("content") {
                extract_text_chunks(content, &mut chunk);
            }
            if !chunk.is_empty() {
                let safe_chunk = redact_secrets_owned(&chunk, secrets);
                acc.assistant_text.push_str(&safe_chunk);
                acc.emit(RuntimeEvent::TextDelta { text: safe_chunk }, event_tx);
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
            let tool_name = update
                .get("name")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map_or_else(|| title.clone(), str::to_owned);
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
            let safe_excerpt = redact_secrets_owned(&excerpt, secrets);
            acc.emit(
                RuntimeEvent::ToolObserved {
                    tool_call_id,
                    tool_name,
                    title: redact_secrets_owned(&title, secrets),
                    status,
                    output_excerpt: safe_excerpt,
                },
                event_tx,
            );
        }
        // Explicitly ignore hidden thought/reasoning chunks so private reasoning never leaks.
        _ => {}
    }
}

/// Selects an ACP permission `optionId` strictly without privilege escalation (A05).
///
/// - [`PolicyEffect::Allow`] matches ONLY `kind == "allow_once"`. It NEVER escalates to
///   `allow_always`; if `allow_once` is unavailable, returns `None` (`cancelled`).
/// - [`PolicyEffect::Deny`] matches `reject_once`, falling back safely to `reject_always`.
#[must_use]
pub fn select_permission_option(
    options: &[AcpPermissionOption],
    effect: PolicyEffect,
) -> Option<String> {
    match effect {
        PolicyEffect::Allow => options
            .iter()
            .find(|opt| opt.kind == "allow_once")
            .map(|opt| opt.option_id.clone()),
        PolicyEffect::Deny => options
            .iter()
            .find(|opt| opt.kind == "reject_once")
            .or_else(|| options.iter().find(|opt| opt.kind == "reject_always"))
            .map(|opt| opt.option_id.clone()),
    }
}

/// Parses ACP `session/request_permission` params into a structured [`AcpPermissionRequest`].
#[must_use]
pub fn parse_permission_request(params: &Value) -> AcpPermissionRequest {
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
    let raw_input = tool_call
        .get("rawInput")
        .or_else(|| tool_call.get("args"))
        .cloned()
        .unwrap_or(Value::Null);

    let tool_name = tool_call
        .get("name")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map_or_else(|| infer_tool_name(&kind, &title, &raw_input), str::to_owned);

    let locations = extract_permission_locations(tool_call, &raw_input);
    let target = extract_permission_target(tool_call, &raw_input, &locations);

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
                    Some(AcpPermissionOption {
                        option_id,
                        kind: opt_kind,
                        name,
                    })
                })
                .collect()
        });

    AcpPermissionRequest {
        tool_call_id,
        tool_name,
        kind,
        title,
        target,
        locations,
        raw_input,
        options,
    }
}

const SHELL_INPUT_KEYS: &[&str] = &["CommandLine", "command", "cmd", "script", "shell"];
const WRITE_PATH_INPUT_KEYS: &[&str] = &[
    "TargetFile",
    "Path",
    "path",
    "file_path",
    "CodeContent",
    "ReplacementContent",
    "ReplacementChunks",
    "FileContent",
];
const READ_PATH_INPUT_KEYS: &[&str] = &[
    "AbsolutePath",
    "DirectoryPath",
    "SearchDirectory",
    "SearchPath",
    "Uri",
    "uri",
];
const ALL_PATH_TARGET_KEYS: &[&str] = &[
    "Path",
    "path",
    "TargetFile",
    "AbsolutePath",
    "file_path",
    "DirectoryPath",
    "SearchDirectory",
    "SearchPath",
    "Uri",
    "uri",
];

fn has_any_key(val: &Value, keys: &[&str]) -> bool {
    let Some(obj) = val.as_object() else {
        return false;
    };
    keys.iter().any(|k| obj.contains_key(*k))
}

fn has_identifier_token(haystack: &str, token: &str) -> bool {
    let bytes = haystack.as_bytes();
    let mut start = 0;
    while let Some(rel) = haystack[start..].find(token) {
        let idx = start + rel;
        let end = idx + token.len();
        let before_ok = idx == 0 || !is_ident_byte(bytes[idx - 1]);
        let after_ok = end == bytes.len() || !is_ident_byte(bytes[end]);
        if before_ok && after_ok {
            return true;
        }
        start = idx + 1;
    }
    false
}

const fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn infer_tool_name(kind: &str, title: &str, raw_input: &Value) -> String {
    // 1. Always classify native provider tool kinds FIRST so untrusted titles/arguments
    //    (e.g. `rm -rf * # memory_search`) can NEVER spoof an MCP memory tool.
    match kind.trim().to_ascii_lowercase().as_str() {
        "execute" => return String::from("run_command"),
        "edit" => return String::from("write_file"),
        "delete" => return String::from("delete_file"),
        "move" => return String::from("move_file"),
        "read" | "search" => return String::from("read_file"),
        _ => {}
    }

    // 2. Inspect structured native shell/filesystem input keys BEFORE ever consulting `title`.
    if has_any_key(raw_input, SHELL_INPUT_KEYS) {
        return String::from("run_command");
    }
    if has_any_key(raw_input, WRITE_PATH_INPUT_KEYS) {
        return String::from("write_file");
    }
    if has_any_key(raw_input, READ_PATH_INPUT_KEYS) {
        return String::from("read_file");
    }

    // 3. Only non-native, non-shell/file calls may resolve to Dennett memory MCP tools from title.
    let lower_title = title.to_ascii_lowercase();
    for mcp_tool in [
        "memory_search",
        "memory_get_current",
        "memory_get_history",
        "memory_write_note",
        "memory_correct_note",
    ] {
        if has_identifier_token(&lower_title, mcp_tool) {
            return mcp_tool.to_owned();
        }
    }
    if lower_title == "dennett-memory"
        || lower_title.starts_with("dennett-memory:")
        || lower_title.starts_with("dennett-memory/")
        || lower_title.starts_with("dennett-memory__")
        || lower_title.starts_with("mcp:dennett-memory")
        || lower_title.contains("(dennett-memory")
    {
        return String::from("mcp:dennett-memory");
    }

    if title.trim().is_empty() {
        String::from("unknown")
    } else {
        title.trim().to_owned()
    }
}

fn push_unique_trimmed(out: &mut Vec<String>, candidate: &str) {
    let trimmed = candidate.trim();
    if !trimmed.is_empty() && !out.iter().any(|existing| existing == trimmed) {
        out.push(trimmed.to_owned());
    }
}

fn extract_permission_locations(tool_call: &Value, raw_input: &Value) -> Vec<String> {
    let mut locations = Vec::new();
    for source in [Some(raw_input), tool_call.get("args")]
        .into_iter()
        .flatten()
    {
        for key in ALL_PATH_TARGET_KEYS {
            if let Some(val) = source.get(*key).and_then(Value::as_str) {
                push_unique_trimmed(&mut locations, val);
            }
        }
    }
    if let Some(arr) = tool_call.get("locations").and_then(Value::as_array) {
        for loc in arr {
            for field in ["path", "uri", "Uri"] {
                if let Some(val) = loc.get(field).and_then(Value::as_str) {
                    push_unique_trimmed(&mut locations, val);
                }
            }
        }
    }
    locations
}

fn extract_permission_target(
    tool_call: &Value,
    raw_input: &Value,
    locations: &[String],
) -> Option<String> {
    if let Some(first_path) = locations.first() {
        return Some(first_path.clone());
    }
    for source in [Some(raw_input), tool_call.get("args")]
        .into_iter()
        .flatten()
    {
        for key in ["CommandLine", "command", "cmd", "scope"] {
            if let Some(val) = source.get(key).and_then(Value::as_str)
                && !val.trim().is_empty()
            {
                return Some(val.trim().to_owned());
            }
        }
    }
    None
}

/// Extracts `thought_level` options and current value from an ACP response containing `configOptions`.
///
/// Returns `None` if the JSON payload does not include a `configOptions` array at all.
/// Returns `Some((vec![], None))` when `configOptions` IS present but omits `thought_level`
/// (which `agy acp 1.5.15` does for models with built-in reasoning such as Claude Opus 4.6 Thinking).
#[must_use]
pub fn extract_thinking_options_from_config(
    raw: &Value,
) -> Option<(Vec<RuntimeThinkingOption>, Option<ThinkingLevel>)> {
    let config_options = raw.get("configOptions").and_then(Value::as_array)?;
    for opt in config_options {
        if opt.get("id").and_then(Value::as_str) == Some("thought_level") {
            let current = opt
                .get("currentValue")
                .and_then(Value::as_str)
                .and_then(|s| ThinkingLevel::from_str(s).ok());
            let mut parsed = Vec::new();
            if let Some(values) = opt.get("options").and_then(Value::as_array) {
                for item in values {
                    let Some(val_str) = item.get("value").and_then(Value::as_str) else {
                        continue;
                    };
                    let Ok(level) = ThinkingLevel::from_str(val_str) else {
                        continue;
                    };
                    if parsed
                        .iter()
                        .any(|existing: &RuntimeThinkingOption| existing.value == level)
                    {
                        continue;
                    }
                    let label = item
                        .get("name")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map_or_else(|| level.label().to_owned(), str::to_owned);
                    parsed.push(RuntimeThinkingOption {
                        value: level,
                        label,
                    });
                }
            }
            return Some((parsed, current));
        }
    }
    Some((Vec::new(), None))
}

fn parse_quotas_meta(raw: &Value) -> Vec<RuntimeQuotaWindow> {
    let Some(arr) = raw
        .get("_meta")
        .and_then(|m| m.get("antigravity"))
        .and_then(|a| a.get("quotas"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    let mut quotas = Vec::new();
    for item in arr {
        let Some(label) = item
            .get("label")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let Some(raw_pct) = item
            .get("remainingPercent")
            .or_else(|| item.get("remaining_percent"))
            .and_then(Value::as_u64)
        else {
            continue;
        };
        let pct = u8::try_from(raw_pct.min(100)).unwrap_or(100);
        quotas.push(RuntimeQuotaWindow {
            label: label.to_owned(),
            remaining_percent: pct,
        });
    }
    quotas
}

#[expect(
    clippy::too_many_lines,
    reason = "parses ACP session/new and session/load payload into unified AcpSessionInfo including models, thinking levels, and optional quotas"
)]
fn parse_session_info(
    raw: &Value,
    fallback_session_id: Option<&str>,
) -> Result<AcpSessionInfo, DennettError> {
    let session_id = raw
        .get("sessionId")
        .and_then(Value::as_str)
        .or(fallback_session_id)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| DennettError::RuntimeProtocol {
            code: None,
            detail: String::from("ACP session response missing `sessionId`"),
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

    let quotas = parse_quotas_meta(raw);

    let extracted_thinking = extract_thinking_options_from_config(raw);
    let (thinking_options, default_thinking_level) = match &extracted_thinking {
        Some((opts, cur)) => (opts.clone(), *cur),
        None => (Vec::new(), None),
    };
    let active_thinking_levels = extracted_thinking
        .map(|(opts, _)| opts.into_iter().map(|opt| opt.value).collect::<Vec<_>>());

    let mut available_models = Vec::new();
    let mut raw_available_ids: BTreeSet<String> = BTreeSet::new();
    let mut push_unique = |candidate: &str| {
        if !candidate.is_empty() && !available_models.iter().any(|m| m == candidate) {
            available_models.push(candidate.to_owned());
        }
    };

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
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                raw_available_ids.insert(id.to_owned());
            }
        }
    }

    let mut default_model_id: Option<String> = None;
    let mut models: Vec<RuntimeModelOption> = Vec::new();

    if let Some(config_options) = raw.get("configOptions").and_then(Value::as_array) {
        for opt in config_options {
            if opt.get("id").and_then(Value::as_str) == Some("model") {
                default_model_id = opt
                    .get("currentValue")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned);
                if let Some(values) = opt.get("options").and_then(Value::as_array) {
                    for item in values {
                        let Some(val) = item
                            .get("value")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                        else {
                            continue;
                        };
                        push_unique(val);
                        if models.iter().any(|m| m.model_id == val) {
                            continue;
                        }
                        let label = item
                            .get("name")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .unwrap_or(val)
                            .to_owned();
                        let mut supported: Vec<ThinkingLevel> = [
                            ThinkingLevel::Low,
                            ThinkingLevel::Medium,
                            ThinkingLevel::High,
                            ThinkingLevel::XHigh,
                        ]
                        .into_iter()
                        .filter(|lvl| {
                            raw_available_ids.contains(&format!("{val}-{}", lvl.as_str()))
                        })
                        .collect();
                        if supported.is_empty()
                            && raw_available_ids.is_empty()
                            && default_model_id.as_deref() == Some(val)
                            && let Some(ref active_lvls) = active_thinking_levels
                        {
                            supported.clone_from(active_lvls);
                        }
                        let model_default_thinking = default_thinking_level
                            .filter(|lvl| supported.contains(lvl))
                            .or_else(|| {
                                if supported.contains(&ThinkingLevel::Medium) {
                                    Some(ThinkingLevel::Medium)
                                } else {
                                    supported.first().copied()
                                }
                            });
                        models.push(RuntimeModelOption {
                            model_id: val.to_owned(),
                            label,
                            supported_thinking_levels: supported,
                            default_thinking_level: model_default_thinking,
                        });
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

    if models.is_empty() {
        for model_id in &available_models {
            models.push(RuntimeModelOption {
                model_id: model_id.clone(),
                label: model_id.clone(),
                supported_thinking_levels: active_thinking_levels.clone().unwrap_or_default(),
                default_thinking_level,
            });
        }
        if default_model_id.is_none() {
            default_model_id = models.first().map(|m| m.model_id.clone());
        }
    }

    Ok(AcpSessionInfo {
        session_id,
        tier,
        available_models,
        models,
        thinking_options,
        default_model_id,
        default_thinking_level,
        quotas,
        active_thinking_levels,
    })
}

fn snapshot_stderr_buf(stderr_buf: &Mutex<String>, secrets: &[String]) -> String {
    let raw = stderr_buf.lock().map_or_default(|guard| guard.clone());
    redact_secrets_owned(&raw, secrets)
}

fn is_uncorrelated_error_frame(msg: &Value) -> bool {
    msg.get("method").is_none()
        && msg.get("error").is_some()
        && matches!(msg.get("id"), None | Some(Value::Null))
}

fn unwrap_rpc_result(msg: &Value, secrets: &[String]) -> Result<Value, DennettError> {
    if let Some(err) = extract_json_rpc_error(msg, secrets) {
        return Err(err);
    }
    Ok(msg.get("result").cloned().unwrap_or(Value::Null))
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

async fn write_json_to<W: AsyncWrite + Unpin>(
    writer: &mut W,
    value: &Value,
) -> Result<(), DennettError> {
    let mut line = serde_json::to_string(value).map_err(|err| DennettError::Internal {
        detail: format!("failed to serialize JSON-RPC message: {err}"),
    })?;
    line.push('\n');

    let write_fut = async {
        writer.write_all(line.as_bytes()).await?;
        writer.flush().await
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

async fn read_message_from<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    timeout: Duration,
    secrets: &[String],
    stderr_hint: impl FnOnce() -> String,
) -> Result<Value, DennettError> {
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
            read_bounded_line(reader, MAX_ACP_STDOUT_LINE_BYTES),
        )
        .await
        .map_err(|_| DennettError::Timeout {
            operation: String::from("agy acp stdout read"),
        })??;

        let Some(line) = maybe_line else {
            let stderr = stderr_hint();
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
                secrets,
            ),
        });
    }
}

pub(crate) async fn read_bounded_line<R: AsyncBufRead + Unpin>(
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

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::port::PolicyEvaluationOutcome;

    use super::*;

    struct CountingAllowPolicy {
        calls: Arc<AtomicUsize>,
    }

    impl PolicyEvaluator for CountingAllowPolicy {
        fn evaluate(&self, _req: &PolicyEvaluationRequest) -> PolicyEvaluationOutcome {
            self.calls.fetch_add(1, Ordering::SeqCst);
            PolicyEvaluationOutcome {
                effect: PolicyEffect::Allow,
                matched_rule_id: None,
                reason: String::from("allowed for test"),
                scope: String::from("global"),
            }
        }
    }

    #[test]
    fn select_permission_option_never_escalates_allow_to_allow_always() {
        let only_always = vec![
            AcpPermissionOption {
                option_id: String::from("opt-always"),
                kind: String::from("allow_always"),
                name: String::from("Always Allow"),
            },
            AcpPermissionOption {
                option_id: String::from("opt-rej"),
                kind: String::from("reject_once"),
                name: String::from("Reject"),
            },
        ];
        assert_eq!(
            select_permission_option(&only_always, PolicyEffect::Allow),
            None
        );
        assert_eq!(
            select_permission_option(&only_always, PolicyEffect::Deny).as_deref(),
            Some("opt-rej")
        );
    }

    #[test]
    fn spawn_rejects_dangerously_skip_permissions() {
        let cfg = AcpProcessConfig {
            binary: PathBuf::from("agy"),
            extra_args: vec![String::from("--dangerously-skip-permissions")],
            working_dir: PathBuf::from("."),
            env_overrides: vec![],
            secrets_to_redact: vec![],
        };
        let res = AcpProcess::spawn(&cfg);
        assert!(matches!(res, Err(DennettError::SafetyGuardRejected { .. })));
    }

    #[tokio::test]
    async fn partial_text_followed_by_uncorrelated_error_or_eof_never_becomes_succeeded() {
        let sid = SessionId::new();
        let rid = RunId::new();
        let policy: Arc<dyn PolicyEvaluator> = Arc::new(CountingAllowPolicy {
            calls: Arc::new(AtomicUsize::new(0)),
        });

        // 1. Partial text + id:null JSON-RPC error frame -> Err(RuntimeProtocol), NOT Succeeded!
        let server_stream = concat!(
            "{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"update\":{\"sessionUpdate\":\"agent_message_chunk\",\"content\":{\"type\":\"text\",\"text\":\"partial answer\"}}}}\n",
            "{\"jsonrpc\":\"2.0\",\"id\":null,\"error\":{\"code\":-32000,\"message\":\"quota exhausted GEMINI_API_KEY=AIzaSySecretKey1234567890123456\"}}\n"
        );
        let mut reader = BufReader::new(server_stream.as_bytes());
        let mut writer = Vec::new();
        let (_tx, rx) = watch::channel(false);
        let res = exchange_prompt_turn_io(
            &mut reader,
            &mut writer,
            PromptTurnIoArgs {
                req_id: 1,
                dennett_session_id: sid,
                provider_session_id: "prov-1",
                run_id: rid,
                prompt_text: "hello",
                timeout: Duration::from_secs(5),
                secrets: &[String::from("AIzaSySecretKey1234567890123456")],
            },
            rx,
            Arc::clone(&policy),
            None,
            String::new,
        )
        .await;
        let Err(DennettError::RuntimeProtocol { code, detail }) = res else {
            panic!("expected RuntimeProtocol error, got {res:?}");
        };
        assert_eq!(code, Some(-32000));
        assert!(
            !detail.contains("AIzaSySecretKey"),
            "secret leaked in error: {detail}"
        );

        // 2. Partial text + abrupt EOF -> Err(RuntimeProtocol), NOT Succeeded!
        let eof_stream = "{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"update\":{\"sessionUpdate\":\"agent_message_chunk\",\"content\":{\"type\":\"text\",\"text\":\"truncated\"}}}}\n";
        let mut reader2 = BufReader::new(eof_stream.as_bytes());
        let mut writer2 = Vec::new();
        let (_tx2, rx2) = watch::channel(false);
        let res2 = exchange_prompt_turn_io(
            &mut reader2,
            &mut writer2,
            PromptTurnIoArgs {
                req_id: 2,
                dennett_session_id: sid,
                provider_session_id: "prov-1",
                run_id: rid,
                prompt_text: "hello",
                timeout: Duration::from_secs(5),
                secrets: &[],
            },
            rx2,
            policy,
            None,
            || String::from("fatal crash"),
        )
        .await;
        assert!(matches!(res2, Err(DennettError::RuntimeProtocol { .. })));
    }

    #[tokio::test]
    async fn live_cancel_sends_session_cancel_and_rejects_late_permission_request() {
        let (mut client_to_server_w, server_from_client_r) = tokio::io::duplex(8192);
        let (mut server_to_client_w, client_from_server_r) = tokio::io::duplex(8192);

        let policy_calls = Arc::new(AtomicUsize::new(0));
        let policy: Arc<dyn PolicyEvaluator> = Arc::new(CountingAllowPolicy {
            calls: Arc::clone(&policy_calls),
        });
        let (cancel_tx, cancel_rx) = watch::channel(false);

        let server_task = tokio::spawn(async move {
            let mut srv_in = BufReader::new(server_from_client_r);
            let mut line = String::new();
            // 1. Read session/prompt
            srv_in.read_line(&mut line).await.expect("read prompt");
            assert!(line.contains("session/prompt"));

            // 2. Send first streaming update
            server_to_client_w
                .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"update\":{\"sessionUpdate\":\"agent_message_chunk\",\"content\":{\"type\":\"text\",\"text\":\"working...\"}}}}\n")
                .await
                .expect("write update");

            // 3. Wait for session/cancel from client
            line.clear();
            srv_in.read_line(&mut line).await.expect("read cancel");
            assert!(
                line.contains("session/cancel"),
                "expected session/cancel, got: {line}"
            );

            // 4. Send a late session/request_permission AFTER cancel!
            server_to_client_w
                .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":99,\"method\":\"session/request_permission\",\"params\":{\"toolCall\":{\"toolCallId\":\"late-call\",\"name\":\"run_command\",\"kind\":\"execute\",\"title\":\"rm -rf /\"},\"options\":[{\"optionId\":\"allow-1\",\"kind\":\"allow_once\",\"name\":\"Allow\"}]}}\n")
                .await
                .expect("write late perm");

            // 5. Verify client responds with `cancelled` (NEVER `allow-1`!)
            line.clear();
            srv_in
                .read_line(&mut line)
                .await
                .expect("read late perm response");
            let perm_resp: Value = serde_json::from_str(line.trim()).expect("parse perm resp");
            assert_eq!(perm_resp["id"], 99);
            assert_eq!(perm_resp["result"]["outcome"]["outcome"], "cancelled");

            // 6. Send final cancelled prompt response
            server_to_client_w
                .write_all(
                    b"{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"stopReason\":\"cancelled\"}}\n",
                )
                .await
                .expect("write final cancelled");
        });

        let (ev_tx, mut ev_rx) = mpsc::channel(16);
        let trigger = tokio::spawn(async move {
            if let Some(RuntimeEvent::TextDelta { .. }) = ev_rx.recv().await {
                let _ = cancel_tx.send(true);
            }
        });

        let mut reader = BufReader::new(client_from_server_r);
        let (outcome, needs_reset) = exchange_prompt_turn_io(
            &mut reader,
            &mut client_to_server_w,
            PromptTurnIoArgs {
                req_id: 1,
                dennett_session_id: SessionId::new(),
                provider_session_id: "prov-cancel",
                run_id: RunId::new(),
                prompt_text: "long task",
                timeout: Duration::from_secs(10),
                secrets: &[],
            },
            cancel_rx,
            policy,
            Some(ev_tx),
            String::new,
        )
        .await
        .expect("cancelled turn outcome");

        server_task.await.expect("server assertions");
        trigger.await.expect("trigger task");

        assert!(!needs_reset);
        assert_eq!(outcome.status, RunStatus::Cancelled);
        assert_eq!(outcome.stop_reason, "cancelled");
        assert_eq!(
            policy_calls.load(Ordering::SeqCst),
            0,
            "late permission after cancel must NEVER reach PolicyEvaluator"
        );
    }

    #[test]
    fn parse_permission_request_never_misclassifies_shell_or_file_with_memory_substrings() {
        // Native execute tool with omitted `name` and spoofed memory keywords in title + CommandLine.
        let exec_params = json!({
            "toolCall": {
                "toolCallId": "call-exec-spoof",
                "kind": "execute",
                "title": "rm -rf * # memory_search dennett-memory",
                "rawInput": { "CommandLine": "rm -rf * # memory_search dennett-memory" }
            },
            "options": []
        });
        let parsed_exec = parse_permission_request(&exec_params);
        assert_eq!(parsed_exec.tool_name, "run_command");
        assert_eq!(
            parsed_exec.target.as_deref(),
            Some("rm -rf * # memory_search dennett-memory")
        );

        // Unknown kind with CommandLine + spoofed title MUST STILL classify as `run_command`!
        let cmd_params = json!({
            "toolCall": {
                "toolCallId": "call-cmd-spoof",
                "kind": "other",
                "title": "dennett-memory: memory_search",
                "rawInput": { "CommandLine": "powershell -c whoami # dennett-memory" }
            },
            "options": []
        });
        let parsed_cmd = parse_permission_request(&cmd_params);
        assert_eq!(parsed_cmd.tool_name, "run_command");

        // Genuine MCP memory call (kind=other, no shell/file keys) classifies as `memory_search`.
        let mcp_params = json!({
            "toolCall": {
                "toolCallId": "call-mcp-ok",
                "kind": "other",
                "title": "dennett-memory: memory_search",
                "rawInput": { "scope": "proj-1", "query": "constraint" }
            },
            "options": []
        });
        let parsed_mcp = parse_permission_request(&mcp_params);
        assert_eq!(parsed_mcp.tool_name, "memory_search");
        assert_eq!(parsed_mcp.target.as_deref(), Some("proj-1"));
        assert_eq!(parsed_mcp.locations, Vec::<String>::new());
    }

    #[test]
    fn parse_permission_request_extracts_all_read_keys_and_multiple_locations() {
        for (key, expected) in [
            ("DirectoryPath", "/home/user/.ssh"),
            ("SearchDirectory", "/etc/shadow"),
            ("SearchPath", "/var/secrets"),
            ("Uri", "file:///etc/passwd"),
            ("uri", "file:///C:/Users/me/.gemini"),
        ] {
            let params = json!({
                "toolCall": {
                    "toolCallId": "call-read",
                    "kind": "read",
                    "title": "read outside cwd",
                    "rawInput": { key: expected }
                },
                "options": []
            });
            let parsed = parse_permission_request(&params);
            assert_eq!(parsed.tool_name, "read_file");
            assert_eq!(parsed.target.as_deref(), Some(expected));
            assert_eq!(parsed.locations, vec![expected.to_owned()]);
        }

        // Multiple locations including a second escaping entry must ALL be captured.
        let multi_params = json!({
            "toolCall": {
                "toolCallId": "call-multi",
                "kind": "read",
                "title": "multi-location read",
                "rawInput": { "Path": "docs/ok.txt" },
                "locations": [
                    { "path": "docs/ok.txt" },
                    { "path": "/etc/passwd" },
                    { "uri": "file:///root/.ssh/id_rsa" }
                ]
            },
            "options": []
        });
        let parsed_multi = parse_permission_request(&multi_params);
        assert_eq!(parsed_multi.target.as_deref(), Some("docs/ok.txt"));
        assert_eq!(
            parsed_multi.locations,
            vec![
                String::from("docs/ok.txt"),
                String::from("/etc/passwd"),
                String::from("file:///root/.ssh/id_rsa"),
            ]
        );
    }

    #[test]
    fn parse_session_info_derives_per_model_thinking_levels_and_optional_quotas() {
        let raw = json!({
            "sessionId": "sess-live-1",
            "_meta": {
                "antigravity": {
                    "tier": { "name": "Google AI Ultra", "id": "g1-ultra-tier" }
                }
            },
            "configOptions": [
                {
                    "id": "model",
                    "currentValue": "gemini-4-neon",
                    "options": [
                        { "value": "gemini-4-neon", "name": "Gemini 4 Neon" },
                        { "value": "gemini-4-carbon", "name": "Gemini 4 Carbon" },
                        { "value": "claude-opus-4-6-thinking", "name": "Claude Opus 4.6 (Thinking)" }
                    ]
                },
                {
                    "id": "thought_level",
                    "currentValue": "medium",
                    "options": [
                        { "value": "low", "name": "Low" },
                        { "value": "medium", "name": "Medium" },
                        { "value": "high", "name": "High" },
                        { "value": "xhigh", "name": "Extra" }
                    ]
                }
            ],
            "models": {
                "availableModels": [
                    { "modelId": "gemini-4-neon-low" },
                    { "modelId": "gemini-4-neon-medium" },
                    { "modelId": "gemini-4-neon-high" },
                    { "modelId": "gemini-4-neon-xhigh" },
                    { "modelId": "gemini-4-carbon-low" },
                    { "modelId": "gemini-4-carbon-medium" },
                    { "modelId": "gemini-4-carbon-high" },
                    { "modelId": "claude-opus-4-6-thinking" }
                ]
            }
        });

        let info = parse_session_info(&raw, None).expect("parse session info");
        assert_eq!(info.tier.as_deref(), Some("Google AI Ultra"));
        assert_eq!(info.default_model_id.as_deref(), Some("gemini-4-neon"));
        assert_eq!(info.default_thinking_level, Some(ThinkingLevel::Medium));
        assert!(info.quotas.is_empty(), "no quotas reported -> empty vec");
        assert_eq!(info.models.len(), 3);
        assert_eq!(
            info.models[0].supported_thinking_levels,
            vec![
                ThinkingLevel::Low,
                ThinkingLevel::Medium,
                ThinkingLevel::High,
                ThinkingLevel::XHigh
            ]
        );
        assert_eq!(
            info.models[1].supported_thinking_levels,
            vec![
                ThinkingLevel::Low,
                ThinkingLevel::Medium,
                ThinkingLevel::High
            ]
        );
        assert_eq!(info.models[2].supported_thinking_levels, Vec::new());

        // Switching to Claude drops `thought_level` from configOptions entirely.
        let claude_set_resp = json!({
            "configOptions": [
                { "id": "model", "currentValue": "claude-opus-4-6-thinking", "options": [] }
            ]
        });
        let (claude_opts, claude_cur) =
            extract_thinking_options_from_config(&claude_set_resp).expect("configOptions present");
        assert_eq!(claude_opts, Vec::new());
        assert_eq!(claude_cur, None);

        // Optional future `_meta.antigravity.quotas` parses and clamps cleanly.
        let with_quotas = json!({
            "sessionId": "sess-q",
            "_meta": {
                "antigravity": {
                    "quotas": [
                        { "label": "5ч", "remainingPercent": 87 },
                        { "label": "нед", "remaining_percent": 140 }
                    ]
                }
            }
        });
        let q_info = parse_session_info(&with_quotas, None).expect("parse quotas");
        assert_eq!(q_info.quotas.len(), 2);
        assert_eq!(q_info.quotas[0].label, "5ч");
        assert_eq!(q_info.quotas[0].remaining_percent, 87);
        assert_eq!(q_info.quotas[1].remaining_percent, 100);
    }
}
