use async_trait::async_trait;
use dennett_agent_core::{
    AgentRequest, AgentResponse, AgentRuntimePort, CancelDisposition, CancelRuntimeTurnRequest,
    CancellationAcknowledgement, NativeExtension, OpaqueContinuation, RuntimeActivityStatus,
    RuntimeCapabilities, RuntimeDeadline, RuntimeDescriptor, RuntimeError, RuntimeErrorCode,
    RuntimeEvent, RuntimeEventKind, RuntimeEventStream, RuntimeKind, RuntimeTerminal,
    RuntimeTerminalKind, RuntimeTerminalOutcome, RuntimeTurn, RuntimeTurnRequest, RuntimeUsage,
};
use dennett_kernel::{DennettError, DennettResult};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    ffi::OsString,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex as StdMutex, Weak},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, Command},
    sync::{Mutex, mpsc, oneshot},
};

pub const RUNTIME_HOST_SCRIPT_ENV: &str = "DENNETT_RUNTIME_HOST_SCRIPT";
pub const RUNTIME_NODE_EXECUTABLE_ENV: &str = "DENNETT_RUNTIME_NODE_EXECUTABLE";
const HOST_PROTOCOL_VERSION: u64 = 1;
const HOST_CONTROL_TIMEOUT: Duration = Duration::from_secs(10);
const HOST_STARTUP_TIMEOUT: Duration = Duration::from_secs(65);
const HOST_EVENT_CAPACITY: usize = 128;
const MAX_HOST_MESSAGE_BYTES: usize = 1024 * 1024;
const SAFE_HOST_ENVIRONMENT: &[&str] = &[
    "ALLUSERSPROFILE",
    "APPDATA",
    "COMSPEC",
    "HOME",
    "HOMEDRIVE",
    "HOMEPATH",
    "LANG",
    "LC_ALL",
    "LOCALAPPDATA",
    "LOGNAME",
    "NUMBER_OF_PROCESSORS",
    "OS",
    "PATH",
    "PATHEXT",
    "PROGRAMDATA",
    "PROGRAMFILES",
    "PROGRAMFILES(X86)",
    "PROGRAMW6432",
    "SHELL",
    "SYSTEMDRIVE",
    "SYSTEMROOT",
    "TEMP",
    "TERM",
    "TMP",
    "TMPDIR",
    "USER",
    "USERDOMAIN",
    "USERNAME",
    "USERPROFILE",
    "WINDIR",
    "XDG_CACHE_HOME",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
];

type TurnKey = (String, String);
type PendingResponse = oneshot::Sender<Result<Value, RuntimeError>>;
type TurnSender = mpsc::Sender<Result<RuntimeEvent, RuntimeError>>;

#[derive(Clone)]
pub struct HostedAgentRuntime {
    inner: Arc<RuntimeHostInner>,
}

struct RuntimeHostInner {
    writer: Mutex<ChildStdin>,
    pending: Mutex<HashMap<String, PendingResponse>>,
    turns: Mutex<HashMap<TurnKey, TurnSender>>,
    child: StdMutex<Child>,
}

impl Drop for RuntimeHostInner {
    fn drop(&mut self) {
        if let Ok(child) = self.child.get_mut() {
            let _ = child.start_kill();
        }
    }
}

impl HostedAgentRuntime {
    pub async fn start(project_root: &Path) -> Result<Self, RuntimeHostStartError> {
        let script = locate_host_script(project_root)?;
        let node = std::env::var_os(RUNTIME_NODE_EXECUTABLE_ENV)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("node"));
        Self::start_process(&node, &script).await
    }

    async fn start_process(node: &Path, script: &Path) -> Result<Self, RuntimeHostStartError> {
        let mut command = Command::new(node);
        command
            .arg(script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        command.env_clear().envs(sanitized_host_environment());
        let mut child = command
            .spawn()
            .map_err(|_| RuntimeHostStartError::SpawnFailed)?;
        let writer = child
            .stdin
            .take()
            .ok_or(RuntimeHostStartError::SpawnFailed)?;
        let stdout = child
            .stdout
            .take()
            .ok_or(RuntimeHostStartError::SpawnFailed)?;
        let inner = Arc::new(RuntimeHostInner {
            writer: Mutex::new(writer),
            pending: Mutex::new(HashMap::new()),
            turns: Mutex::new(HashMap::new()),
            child: StdMutex::new(child),
        });
        tokio::spawn(read_host(BufReader::new(stdout), Arc::downgrade(&inner)));
        let runtime = Self { inner };
        let health = runtime
            .call_with_timeout("health", json!({}), HOST_STARTUP_TIMEOUT)
            .await
            .map_err(|_| RuntimeHostStartError::HandshakeFailed)?;
        if health.get("status").and_then(Value::as_str) != Some("healthy")
            || health.get("protocolVersion").and_then(Value::as_u64) != Some(HOST_PROTOCOL_VERSION)
        {
            return Err(RuntimeHostStartError::HandshakeFailed);
        }
        Ok(runtime)
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value, RuntimeError> {
        self.call_with_timeout(method, params, HOST_CONTROL_TIMEOUT)
            .await
    }

    async fn call_with_timeout(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, RuntimeError> {
        let request_id = uuid::Uuid::now_v7().to_string();
        let request = json!({
            "v": HOST_PROTOCOL_VERSION,
            "id": request_id,
            "method": method,
            "params": params,
        });
        let encoded = serde_json::to_vec(&request)
            .map_err(|_| RuntimeError::new(RuntimeErrorCode::InvalidRequest))?;
        if encoded.len().saturating_add(1) > MAX_HOST_MESSAGE_BYTES {
            return Err(RuntimeError::new(RuntimeErrorCode::InvalidRequest));
        }
        let (sender, receiver) = oneshot::channel();
        self.inner
            .pending
            .lock()
            .await
            .insert(request_id.clone(), sender);
        let write_result = async {
            let mut writer = self.inner.writer.lock().await;
            writer.write_all(&encoded).await?;
            writer.write_all(b"\n").await?;
            writer.flush().await
        }
        .await;
        if write_result.is_err() {
            self.inner.pending.lock().await.remove(&request_id);
            return Err(RuntimeError::retryable(
                RuntimeErrorCode::ProviderUnavailable,
            ));
        }
        match tokio::time::timeout(timeout, receiver).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) | Err(_) => {
                self.inner.pending.lock().await.remove(&request_id);
                Err(RuntimeError::retryable(
                    RuntimeErrorCode::ProviderUnavailable,
                ))
            }
        }
    }
}

fn sanitized_host_environment() -> Vec<(OsString, OsString)> {
    filter_host_environment(std::env::vars_os())
}

fn filter_host_environment(
    environment: impl IntoIterator<Item = (OsString, OsString)>,
) -> Vec<(OsString, OsString)> {
    environment
        .into_iter()
        .filter(|(name, _)| {
            let name = name.to_string_lossy().to_ascii_uppercase();
            SAFE_HOST_ENVIRONMENT.contains(&name.as_str())
        })
        .collect()
}

#[async_trait]
impl AgentRuntimePort for HostedAgentRuntime {
    async fn respond(&self, request: AgentRequest) -> DennettResult<AgentResponse> {
        let session_id = uuid::Uuid::now_v7().to_string();
        let turn_id = uuid::Uuid::now_v7().to_string();
        let deadline = RuntimeDeadline::after(Duration::from_secs(120))
            .map_err(|_| DennettError::InvalidInput("runtime deadline".to_owned()))?;
        let mut turn = self
            .start_turn(RuntimeTurnRequest {
                session_id,
                turn_id,
                prompt: request.prompt,
                workspace_path: std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .to_string_lossy()
                    .into_owned(),
                context_handles: request.context_handles,
                continuation: None,
                deadline,
            })
            .await
            .map_err(|_| DennettError::Unavailable("agent runtime".to_owned()))?;
        let mut text = String::new();
        while let Some(event) = turn.next_event().await {
            let event = event.map_err(|_| DennettError::Unavailable("agent runtime".to_owned()))?;
            match event.kind {
                RuntimeEventKind::TextDelta { text: delta } => text.push_str(&delta),
                RuntimeEventKind::Terminal(RuntimeTerminal {
                    outcome: RuntimeTerminalOutcome::Completed,
                    ..
                }) => {
                    return Ok(AgentResponse {
                        text,
                        evidence_handles: Vec::new(),
                    });
                }
                RuntimeEventKind::Terminal(_) => {
                    return Err(DennettError::Unavailable("agent runtime".to_owned()));
                }
                _ => {}
            }
        }
        Err(DennettError::Unavailable("agent runtime".to_owned()))
    }

    async fn describe(&self) -> Result<RuntimeDescriptor, RuntimeError> {
        parse_descriptor(self.call("describe", json!({})).await?)
    }

    async fn start_turn(&self, request: RuntimeTurnRequest) -> Result<RuntimeTurn, RuntimeError> {
        request.validate()?;
        let key = (request.session_id.clone(), request.turn_id.clone());
        let (sender, receiver) = mpsc::channel(HOST_EVENT_CAPACITY);
        if self
            .inner
            .turns
            .lock()
            .await
            .insert(key.clone(), sender)
            .is_some()
        {
            return Err(RuntimeError::new(RuntimeErrorCode::InvalidRequest));
        }
        let continuation = request
            .continuation
            .as_ref()
            .map(|continuation| {
                Ok(json!({
                    "adapterId": continuation.adapter_id(),
                    "handle": continuation.handle_for(continuation.adapter_id())?,
                }))
            })
            .transpose()?;
        let params = json!({
            "sessionId": request.session_id,
            "turnId": request.turn_id,
            "prompt": request.prompt,
            "workspacePath": request.workspace_path,
            "contextHandles": request.context_handles,
            "timeoutMs": u64::try_from(request.deadline.timeout().as_millis())
                .unwrap_or(u64::MAX),
            "continuation": continuation,
        });
        let started = self.call("start_turn", params).await;
        if started
            .as_ref()
            .ok()
            .and_then(|value| value.get("started"))
            .and_then(Value::as_bool)
            != Some(true)
        {
            self.inner.turns.lock().await.remove(&key);
            return Err(started
                .err()
                .unwrap_or_else(|| RuntimeError::new(RuntimeErrorCode::ProtocolViolation)));
        }
        Ok(RuntimeTurn::from_stream(
            key.0.clone(),
            key.1.clone(),
            Box::new(HostedEventStream {
                receiver,
                runtime: self.clone(),
                cancellation: CancelRuntimeTurnRequest {
                    session_id: key.0,
                    turn_id: key.1,
                },
                terminal: false,
            }),
        ))
    }

    async fn cancel_turn(
        &self,
        request: CancelRuntimeTurnRequest,
    ) -> Result<CancellationAcknowledgement, RuntimeError> {
        request.validate()?;
        let expected = request.clone();
        parse_cancellation(
            self.call(
                "cancel_turn",
                json!({ "sessionId": request.session_id, "turnId": request.turn_id }),
            )
            .await?,
            &expected,
        )
    }
}

struct HostedEventStream {
    receiver: mpsc::Receiver<Result<RuntimeEvent, RuntimeError>>,
    runtime: HostedAgentRuntime,
    cancellation: CancelRuntimeTurnRequest,
    terminal: bool,
}

#[async_trait]
impl RuntimeEventStream for HostedEventStream {
    async fn next_event(&mut self) -> Option<Result<RuntimeEvent, RuntimeError>> {
        let event = self.receiver.recv().await;
        if event.as_ref().is_some_and(|event| {
            matches!(
                event,
                Ok(RuntimeEvent {
                    kind: RuntimeEventKind::Terminal(_),
                    ..
                }) | Err(_)
            )
        }) {
            self.terminal = true;
        }
        event
    }
}

impl Drop for HostedEventStream {
    fn drop(&mut self) {
        if self.terminal {
            return;
        }
        let runtime = self.runtime.clone();
        let request = self.cancellation.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let _ = runtime.cancel_turn(request).await;
            });
        }
    }
}

async fn read_host<R>(mut reader: BufReader<R>, inner: Weak<RuntimeHostInner>)
where
    R: tokio::io::AsyncRead + Unpin,
{
    loop {
        let line = match read_bounded_frame(&mut reader).await {
            Ok(Some(line)) => line,
            Ok(None) => break,
            Err(_) => {
                if let Some(inner) = inner.upgrade() {
                    fail_host(&inner).await;
                }
                return;
            }
        };
        let Ok(line) = String::from_utf8(line) else {
            if let Some(inner) = inner.upgrade() {
                fail_host(&inner).await;
            }
            return;
        };
        let Some(inner) = inner.upgrade() else {
            return;
        };
        dispatch_host_message(&inner, &line).await;
    }
    if let Some(inner) = inner.upgrade() {
        fail_host(&inner).await;
    }
}

async fn read_bounded_frame<R>(reader: &mut R) -> std::io::Result<Option<Vec<u8>>>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    let mut line = Vec::new();
    loop {
        let (consumed, newline) = {
            let available = reader.fill_buf().await?;
            if available.is_empty() {
                return if line.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(line))
                };
            }
            let newline = available.iter().position(|byte| *byte == b'\n');
            let payload_len = newline.unwrap_or(available.len());
            if line.len().saturating_add(payload_len) > MAX_HOST_MESSAGE_BYTES {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "runtime host frame exceeds the protocol limit",
                ));
            }
            line.extend_from_slice(&available[..payload_len]);
            (
                payload_len + usize::from(newline.is_some()),
                newline.is_some(),
            )
        };
        reader.consume(consumed);
        if newline {
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            return Ok(Some(line));
        }
    }
}

async fn dispatch_host_message(inner: &RuntimeHostInner, line: &str) {
    let Ok(message) = serde_json::from_str::<Value>(line) else {
        fail_host(inner).await;
        return;
    };
    if message.get("v").and_then(Value::as_u64) != Some(HOST_PROTOCOL_VERSION) {
        fail_host(inner).await;
        return;
    }
    if let Some(id) = message.get("id").and_then(Value::as_str) {
        if let Some(sender) = inner.pending.lock().await.remove(id) {
            let result = if let Some(error) = message.get("error") {
                Err(parse_error(error))
            } else {
                message
                    .get("result")
                    .cloned()
                    .ok_or_else(|| RuntimeError::new(RuntimeErrorCode::ProtocolViolation))
            };
            let _ = sender.send(result);
        }
        return;
    }
    match message.get("event").and_then(Value::as_str) {
        Some("runtime_event") => {
            let event = match message
                .get("payload")
                .ok_or(())
                .and_then(|value| parse_runtime_event(value).map_err(|_| ()))
            {
                Ok(event) => event,
                Err(()) => {
                    fail_host(inner).await;
                    return;
                }
            };
            let key = (event.session_id.clone(), event.turn_id.clone());
            let terminal = matches!(event.kind, RuntimeEventKind::Terminal(_));
            let sender = inner.turns.lock().await.get(&key).cloned();
            if let Some(sender) = sender {
                let _ = sender.send(Ok(event)).await;
            }
            if terminal {
                inner.turns.lock().await.remove(&key);
            }
        }
        Some("runtime_error") => {
            let Some(session_id) = message.get("sessionId").and_then(Value::as_str) else {
                fail_host(inner).await;
                return;
            };
            let Some(turn_id) = message.get("turnId").and_then(Value::as_str) else {
                fail_host(inner).await;
                return;
            };
            let error = message.get("error").map_or_else(
                || RuntimeError::new(RuntimeErrorCode::ProtocolViolation),
                parse_error,
            );
            if let Some(sender) = inner
                .turns
                .lock()
                .await
                .remove(&(session_id.to_owned(), turn_id.to_owned()))
            {
                let _ = sender.send(Err(error)).await;
            }
        }
        _ => fail_host(inner).await,
    }
}

async fn fail_host(inner: &RuntimeHostInner) {
    if let Ok(mut child) = inner.child.lock() {
        let _ = child.start_kill();
    }
    let error = RuntimeError::retryable(RuntimeErrorCode::ProviderUnavailable);
    for (_, sender) in inner.pending.lock().await.drain() {
        let _ = sender.send(Err(error.clone()));
    }
    let senders = inner
        .turns
        .lock()
        .await
        .drain()
        .map(|(_, sender)| sender)
        .collect::<Vec<_>>();
    for sender in senders {
        let _ = sender.send(Err(error.clone())).await;
    }
}

fn parse_descriptor(value: Value) -> Result<RuntimeDescriptor, RuntimeError> {
    let adapter_id = required_string(&value, "adapterId")?;
    let runtime_kind = match required_string(&value, "runtimeKind")?.as_str() {
        "native_agent" => RuntimeKind::NativeAgent,
        "generic_loop" => RuntimeKind::GenericLoop,
        _ => return Err(RuntimeError::new(RuntimeErrorCode::ProtocolViolation)),
    };
    let capabilities = value
        .get("capabilities")
        .ok_or_else(|| RuntimeError::new(RuntimeErrorCode::ProtocolViolation))?;
    let schemas = capabilities
        .get("nativeExtensionSchemas")
        .and_then(Value::as_array)
        .ok_or_else(|| RuntimeError::new(RuntimeErrorCode::ProtocolViolation))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| RuntimeError::new(RuntimeErrorCode::ProtocolViolation))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RuntimeDescriptor {
        adapter_id,
        runtime_kind,
        capabilities: RuntimeCapabilities {
            streaming: required_bool(capabilities, "streaming")?,
            continuation: required_bool(capabilities, "continuation")?,
            scoped_cancellation: required_bool(capabilities, "scopedCancellation")?,
            deadlines: required_bool(capabilities, "deadlines")?,
            native_extension_schemas: schemas,
        },
    })
}

fn parse_runtime_event(value: &Value) -> Result<RuntimeEvent, RuntimeError> {
    let kind = value
        .get("kind")
        .ok_or_else(|| RuntimeError::new(RuntimeErrorCode::ProtocolViolation))?;
    let event_kind = match required_string(kind, "type")?.as_str() {
        "started" => RuntimeEventKind::Started {
            continuation: kind
                .get("continuation")
                .map(parse_continuation)
                .transpose()?,
        },
        "text_delta" => RuntimeEventKind::TextDelta {
            text: required_string(kind, "text")?,
        },
        "progress" => RuntimeEventKind::Progress {
            activity_id: kind
                .get("activityId")
                .map(|value| {
                    value
                        .as_str()
                        .filter(|value| !value.is_empty())
                        .map(str::to_owned)
                        .ok_or_else(|| RuntimeError::new(RuntimeErrorCode::ProtocolViolation))
                })
                .transpose()?,
            phase: required_string(kind, "phase")?,
            message: kind
                .get("message")
                .map(|value| {
                    value
                        .as_str()
                        .filter(|value| !value.is_empty())
                        .map(str::to_owned)
                        .ok_or_else(|| RuntimeError::new(RuntimeErrorCode::ProtocolViolation))
                })
                .transpose()?,
            status: match required_string(kind, "status")?.as_str() {
                "started" => RuntimeActivityStatus::Started,
                "updated" => RuntimeActivityStatus::Updated,
                "completed" => RuntimeActivityStatus::Completed,
                "failed" => RuntimeActivityStatus::Failed,
                _ => return Err(RuntimeError::new(RuntimeErrorCode::ProtocolViolation)),
            },
        },
        "usage" => {
            let usage = kind
                .get("usage")
                .ok_or_else(|| RuntimeError::new(RuntimeErrorCode::ProtocolViolation))?;
            RuntimeEventKind::Usage(RuntimeUsage {
                input_tokens: required_u64(usage, "inputTokens")?,
                cached_input_tokens: required_u64(usage, "cachedInputTokens")?,
                output_tokens: required_u64(usage, "outputTokens")?,
                reasoning_output_tokens: required_u64(usage, "reasoningOutputTokens")?,
            })
        }
        "warning" => RuntimeEventKind::Warning {
            code: required_string(kind, "code")?,
        },
        "terminal" => RuntimeEventKind::Terminal(RuntimeTerminal {
            outcome: parse_terminal(
                kind.get("outcome")
                    .ok_or_else(|| RuntimeError::new(RuntimeErrorCode::ProtocolViolation))?,
            )?,
            continuation: kind
                .get("continuation")
                .map(parse_continuation)
                .transpose()?,
        }),
        _ => return Err(RuntimeError::new(RuntimeErrorCode::ProtocolViolation)),
    };
    let native_extensions = value
        .get("nativeExtensions")
        .and_then(Value::as_array)
        .ok_or_else(|| RuntimeError::new(RuntimeErrorCode::ProtocolViolation))?
        .iter()
        .map(|extension| {
            let payload = extension
                .get("payload")
                .ok_or_else(|| RuntimeError::new(RuntimeErrorCode::ProtocolViolation))?;
            Ok(NativeExtension {
                namespace: required_string(extension, "namespace")?,
                schema_version: required_string(extension, "schemaVersion")?,
                payload: serde_json::to_vec(payload)
                    .map_err(|_| RuntimeError::new(RuntimeErrorCode::ProtocolViolation))?,
            })
        })
        .collect::<Result<Vec<_>, RuntimeError>>()?;
    Ok(RuntimeEvent {
        session_id: required_string(value, "sessionId")?,
        turn_id: required_string(value, "turnId")?,
        sequence: required_u64(value, "sequence")?,
        kind: event_kind,
        native_extensions,
    })
}

fn parse_terminal(value: &Value) -> Result<RuntimeTerminalOutcome, RuntimeError> {
    match required_string(value, "type")?.as_str() {
        "completed" => Ok(RuntimeTerminalOutcome::Completed),
        "cancelled" => Ok(RuntimeTerminalOutcome::Cancelled {
            partial: required_bool(value, "partial")?,
        }),
        "timed_out" => Ok(RuntimeTerminalOutcome::TimedOut {
            partial: required_bool(value, "partial")?,
        }),
        "failed" => Ok(RuntimeTerminalOutcome::Failed {
            code: required_string(value, "code")?,
            retryable: required_bool(value, "retryable")?,
            recoverable: required_bool(value, "recoverable")?,
            partial: required_bool(value, "partial")?,
        }),
        _ => Err(RuntimeError::new(RuntimeErrorCode::ProtocolViolation)),
    }
}

fn parse_continuation(value: &Value) -> Result<OpaqueContinuation, RuntimeError> {
    OpaqueContinuation::new(
        required_string(value, "adapterId")?,
        required_string(value, "handle")?,
    )
}

fn parse_cancellation(
    value: Value,
    expected: &CancelRuntimeTurnRequest,
) -> Result<CancellationAcknowledgement, RuntimeError> {
    let session_id = required_string(&value, "sessionId")?;
    let turn_id = required_string(&value, "turnId")?;
    if session_id != expected.session_id || turn_id != expected.turn_id {
        return Err(RuntimeError::new(RuntimeErrorCode::ScopeMismatch));
    }
    let disposition = value
        .get("disposition")
        .ok_or_else(|| RuntimeError::new(RuntimeErrorCode::ProtocolViolation))?;
    let disposition = match required_string(disposition, "type")?.as_str() {
        "requested" => CancelDisposition::Requested,
        "already_requested" => CancelDisposition::AlreadyRequested,
        "not_found" => CancelDisposition::NotFound,
        "already_terminal" => CancelDisposition::AlreadyTerminal(
            match required_string(disposition, "terminal")?.as_str() {
                "completed" => RuntimeTerminalKind::Completed,
                "cancelled" => RuntimeTerminalKind::Cancelled,
                "timed_out" => RuntimeTerminalKind::TimedOut,
                "failed" => RuntimeTerminalKind::Failed,
                _ => return Err(RuntimeError::new(RuntimeErrorCode::ProtocolViolation)),
            },
        ),
        _ => return Err(RuntimeError::new(RuntimeErrorCode::ProtocolViolation)),
    };
    Ok(CancellationAcknowledgement {
        session_id,
        turn_id,
        disposition,
    })
}

fn parse_error(value: &Value) -> RuntimeError {
    let code = match value.get("code").and_then(Value::as_str) {
        Some("invalid_request") => RuntimeErrorCode::InvalidRequest,
        Some("unsupported") => RuntimeErrorCode::Unsupported,
        Some("protocol_violation") => RuntimeErrorCode::ProtocolViolation,
        Some("scope_mismatch") => RuntimeErrorCode::ScopeMismatch,
        Some("continuation_unavailable") => RuntimeErrorCode::ContinuationUnavailable,
        Some("provider_unavailable") => RuntimeErrorCode::ProviderUnavailable,
        _ => RuntimeErrorCode::ProviderFailure,
    };
    RuntimeError {
        code,
        retryable: value
            .get("retryable")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        recoverable: value
            .get("recoverable")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

fn required_string(value: &Value, field: &str) -> Result<String, RuntimeError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| RuntimeError::new(RuntimeErrorCode::ProtocolViolation))
}

fn required_bool(value: &Value, field: &str) -> Result<bool, RuntimeError> {
    value
        .get(field)
        .and_then(Value::as_bool)
        .ok_or_else(|| RuntimeError::new(RuntimeErrorCode::ProtocolViolation))
}

fn required_u64(value: &Value, field: &str) -> Result<u64, RuntimeError> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| RuntimeError::new(RuntimeErrorCode::ProtocolViolation))
}

fn locate_host_script(project_root: &Path) -> Result<PathBuf, RuntimeHostStartError> {
    if let Some(path) = std::env::var_os(RUNTIME_HOST_SCRIPT_ENV).map(PathBuf::from) {
        return path
            .is_file()
            .then_some(path)
            .ok_or(RuntimeHostStartError::HostMissing);
    }
    let mut roots = project_root
        .ancestors()
        .map(Path::to_path_buf)
        .collect::<Vec<_>>();
    if let Ok(current) = std::env::current_dir() {
        roots.extend(current.ancestors().map(Path::to_path_buf));
    }
    roots
        .into_iter()
        .map(|root| root.join("services/adapter-host-node/dist/index.js"))
        .find(|candidate| candidate.is_file())
        .ok_or(RuntimeHostStartError::HostMissing)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RuntimeHostStartError {
    #[error("the built Node adapter host is missing")]
    HostMissing,
    #[error("the Node adapter host could not be started")]
    SpawnFailed,
    #[error("the Node adapter host protocol handshake failed")]
    HandshakeFailed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn host_frames_are_bounded_before_allocation_can_grow_without_limit() {
        let oversized = vec![b'x'; MAX_HOST_MESSAGE_BYTES + 1];
        let mut reader = BufReader::new(oversized.as_slice());
        let error = read_bounded_frame(&mut reader)
            .await
            .expect_err("oversized host frame must be rejected");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);

        let mut reader = BufReader::new(b"{\"v\":1}\r\n".as_slice());
        assert_eq!(
            read_bounded_frame(&mut reader)
                .await
                .expect("bounded frame")
                .expect("frame"),
            b"{\"v\":1}".to_vec()
        );
    }

    #[test]
    fn runtime_host_environment_excludes_api_and_unrelated_service_secrets() {
        let filtered = filter_host_environment([
            (OsString::from("PATH"), OsString::from("safe")),
            (OsString::from("OPENAI_API_KEY"), OsString::from("secret")),
            (OsString::from("GITHUB_TOKEN"), OsString::from("secret")),
            (OsString::from("LOCALAPPDATA"), OsString::from("profile")),
        ]);
        let names = filtered
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![OsString::from("PATH"), OsString::from("LOCALAPPDATA")]
        );
    }

    #[tokio::test]
    async fn process_bridge_streams_provider_neutral_events() {
        let temp = tempfile::tempdir().expect("temporary runtime host");
        let script = temp.path().join("fake-runtime-host.mjs");
        std::fs::write(
            &script,
            r#"
import readline from "node:readline";
const write = value => process.stdout.write(JSON.stringify(value) + "\n");
readline.createInterface({ input: process.stdin }).on("line", line => {
  const request = JSON.parse(line);
  if (request.method === "health") {
    write({ v: 1, id: request.id, result: { status: "healthy", protocolVersion: 1 } });
  } else if (request.method === "describe") {
    write({ v: 1, id: request.id, result: {
      adapterId: "fixture.runtime", runtimeKind: "native_agent",
      capabilities: { streaming: true, continuation: true, scopedCancellation: true, deadlines: true, nativeExtensionSchemas: [] }
    }});
  } else if (request.method === "start_turn") {
    const p = request.params;
    write({ v: 1, id: request.id, result: { started: true } });
    write({ v: 1, event: "runtime_event", payload: { sessionId: p.sessionId, turnId: p.turnId, sequence: 1, kind: { type: "started", continuation: { adapterId: "fixture.runtime", handle: "thread-a" } }, nativeExtensions: [] }});
    write({ v: 1, event: "runtime_event", payload: { sessionId: p.sessionId, turnId: p.turnId, sequence: 2, kind: { type: "text_delta", text: "fixture answer" }, nativeExtensions: [] }});
    write({ v: 1, event: "runtime_event", payload: { sessionId: p.sessionId, turnId: p.turnId, sequence: 3, kind: { type: "terminal", outcome: { type: "completed" }, continuation: { adapterId: "fixture.runtime", handle: "thread-a" } }, nativeExtensions: [] }});
  } else if (request.method === "cancel_turn") {
    write({ v: 1, id: request.id, result: { sessionId: request.params.sessionId, turnId: request.params.turnId, disposition: { type: "requested" } } });
  }
});
"#,
        )
        .expect("write runtime fixture");
        let runtime = HostedAgentRuntime::start_process(Path::new("node"), &script)
            .await
            .expect("start fixture runtime");
        let descriptor = runtime.describe().await.expect("describe runtime");
        assert_eq!(descriptor.adapter_id, "fixture.runtime");
        let mut turn = runtime
            .start_turn(RuntimeTurnRequest {
                session_id: "session-a".to_owned(),
                turn_id: "turn-a".to_owned(),
                prompt: "private prompt".to_owned(),
                workspace_path: temp.path().to_string_lossy().into_owned(),
                context_handles: Vec::new(),
                continuation: None,
                deadline: RuntimeDeadline::after(Duration::from_secs(1)).expect("deadline"),
            })
            .await
            .expect("start turn");
        let mut kinds = Vec::new();
        while let Some(event) = turn.next_event().await {
            kinds.push(event.expect("runtime event").kind);
        }
        assert!(matches!(kinds.as_slice(), [
            RuntimeEventKind::Started { .. },
            RuntimeEventKind::TextDelta { text },
            RuntimeEventKind::Terminal(RuntimeTerminal { outcome: RuntimeTerminalOutcome::Completed, .. }),
        ] if text == "fixture answer"));
    }
}
