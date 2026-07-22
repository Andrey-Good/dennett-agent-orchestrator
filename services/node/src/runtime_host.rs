use async_trait::async_trait;
use dennett_agent_core::{
    AgentRequest, AgentResponse, AgentRuntimePort, CancelDisposition, CancelRuntimeTurnRequest,
    CancellationAcknowledgement, NativeExtension, OpaqueContinuation, RuntimeActivityStatus,
    RuntimeCapabilities, RuntimeControlChoice, RuntimeControlCondition, RuntimeControlDescriptor,
    RuntimeDeadline, RuntimeDescriptor, RuntimeError, RuntimeErrorCode, RuntimeEvent,
    RuntimeEventKind, RuntimeEventStream, RuntimeKind, RuntimeSteeringMode, RuntimeTerminal,
    RuntimeTerminalKind, RuntimeTerminalOutcome, RuntimeTurn, RuntimeTurnRequest, RuntimeUsage,
    SteerRuntimeTurnRequest, SteeringAcknowledgement,
};
use dennett_kernel::{DennettError, DennettResult};
use dennett_observability::{DiagnosticEvent, DiagnosticEventKind};
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
const MAX_HOST_DIAGNOSTIC_BYTES: usize = 4 * 1024;
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
type PendingResponseSender = oneshot::Sender<Result<Value, RuntimeError>>;
type TurnSender = mpsc::Sender<Result<RuntimeEvent, RuntimeError>>;

struct PendingRequest {
    generation: u64,
    sender: PendingResponseSender,
}

#[derive(Default)]
struct HostCoordination {
    generation: u64,
    fenced: bool,
    pending: HashMap<String, PendingRequest>,
}

enum PendingResponseDisposition {
    Deliver(PendingResponseSender),
    Reject(PendingResponseSender),
    Ignore,
    Unknown,
}

impl HostCoordination {
    fn admit(&mut self, request_id: String, sender: PendingResponseSender) -> Option<u64> {
        if self.fenced {
            return None;
        }
        let generation = self.generation;
        let replaced = self
            .pending
            .insert(request_id, PendingRequest { generation, sender });
        debug_assert!(replaced.is_none(), "runtime request IDs must be unique");
        Some(generation)
    }

    fn take_response(&mut self, request_id: &str) -> PendingResponseDisposition {
        if self.fenced {
            return self
                .pending
                .remove(request_id)
                .map_or(PendingResponseDisposition::Ignore, |pending| {
                    PendingResponseDisposition::Reject(pending.sender)
                });
        }
        match self.pending.remove(request_id) {
            Some(pending) if pending.generation == self.generation => {
                PendingResponseDisposition::Deliver(pending.sender)
            }
            Some(pending) => PendingResponseDisposition::Reject(pending.sender),
            None => PendingResponseDisposition::Unknown,
        }
    }

    fn fence(&mut self) -> Option<Vec<PendingResponseSender>> {
        if self.fenced {
            return None;
        }
        self.fenced = true;
        self.generation = self.generation.saturating_add(1);
        Some(
            self.pending
                .drain()
                .map(|(_, pending)| pending.sender)
                .collect(),
        )
    }

    const fn accepts_response(&self, generation: u64) -> bool {
        !self.fenced && self.generation == generation
    }
}

#[derive(Clone)]
pub struct HostedAgentRuntime {
    inner: Arc<RuntimeHostInner>,
}

struct RuntimeHostInner {
    writer: Mutex<ChildStdin>,
    coordination: Mutex<HostCoordination>,
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
    pub async fn start() -> Result<Self, RuntimeHostStartError> {
        let script = locate_host_script()?;
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
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        command.env_clear().envs(sanitized_host_environment());
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(_) => {
                dennett_observability::record(
                    DiagnosticEvent::new(DiagnosticEventKind::RuntimeHostSpawnFailed)
                        .error_code("runtime_host.spawn_failed")
                        .retryable(true),
                );
                return Err(RuntimeHostStartError::SpawnFailed);
            }
        };
        let writer = child
            .stdin
            .take()
            .ok_or(RuntimeHostStartError::SpawnFailed)?;
        let stdout = child
            .stdout
            .take()
            .ok_or(RuntimeHostStartError::SpawnFailed)?;
        let stderr = child
            .stderr
            .take()
            .ok_or(RuntimeHostStartError::SpawnFailed)?;
        let inner = Arc::new(RuntimeHostInner {
            writer: Mutex::new(writer),
            coordination: Mutex::new(HostCoordination::default()),
            turns: Mutex::new(HashMap::new()),
            child: StdMutex::new(child),
        });
        tokio::spawn(read_host(BufReader::new(stdout), Arc::downgrade(&inner)));
        tokio::spawn(read_host_diagnostics(BufReader::new(stderr)));
        let runtime = Self { inner };
        let health = match runtime
            .call_with_timeout("health", json!({}), HOST_STARTUP_TIMEOUT)
            .await
        {
            Ok(health) => health,
            Err(error) => {
                fail_host(&runtime.inner).await;
                dennett_observability::record(
                    DiagnosticEvent::new(DiagnosticEventKind::RuntimeHostHandshakeFailed)
                        .error_code(error.code.as_str())
                        .retryable(error.retryable),
                );
                return Err(RuntimeHostStartError::HandshakeFailed);
            }
        };
        if health.get("status").and_then(Value::as_str) != Some("healthy")
            || health.get("protocolVersion").and_then(Value::as_u64) != Some(HOST_PROTOCOL_VERSION)
        {
            fail_host(&runtime.inner).await;
            dennett_observability::record(
                DiagnosticEvent::new(DiagnosticEventKind::RuntimeHostProtocolMismatch)
                    .error_code("runtime_host.protocol_mismatch"),
            );
            return Err(RuntimeHostStartError::HandshakeFailed);
        }
        dennett_observability::record(
            DiagnosticEvent::new(DiagnosticEventKind::RuntimeHostReady).status("ready"),
        );
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
        let generation = self
            .inner
            .coordination
            .lock()
            .await
            .admit(request_id.clone(), sender)
            .ok_or_else(provider_unavailable)?;
        enum ControlIoFailure {
            Write,
            ResponseClosed,
        }
        let operation = async {
            {
                let mut writer = self.inner.writer.lock().await;
                writer
                    .write_all(&encoded)
                    .await
                    .map_err(|_| ControlIoFailure::Write)?;
                writer
                    .write_all(b"\n")
                    .await
                    .map_err(|_| ControlIoFailure::Write)?;
                writer.flush().await.map_err(|_| ControlIoFailure::Write)?;
            }
            let result = receiver
                .await
                .map_err(|_| ControlIoFailure::ResponseClosed)?;
            Ok(validate_control_result(&self.inner.coordination, generation, result).await)
        };
        match tokio::time::timeout(timeout, operation).await {
            Ok(Ok(result)) => result,
            Ok(Err(ControlIoFailure::Write)) => {
                fail_host(&self.inner).await;
                dennett_observability::record(
                    DiagnosticEvent::new(DiagnosticEventKind::RuntimeHostWriteFailed)
                        .error_code("provider_unavailable")
                        .retryable(true),
                );
                Err(provider_unavailable())
            }
            Ok(Err(ControlIoFailure::ResponseClosed)) => {
                fail_host(&self.inner).await;
                dennett_observability::record(
                    DiagnosticEvent::new(DiagnosticEventKind::RuntimeHostResponseChannelClosed)
                        .error_code("provider_unavailable")
                        .retryable(true),
                );
                Err(provider_unavailable())
            }
            Err(_) => {
                // A timed-out control request has an unknown external result.
                // Fence the dedicated host before callers may persist failure
                // or retry against a replacement runtime.
                fail_host(&self.inner).await;
                dennett_observability::record(
                    DiagnosticEvent::new(DiagnosticEventKind::RuntimeHostControlTimeout)
                        .error_code("provider_unavailable")
                        .retryable(true),
                );
                Err(provider_unavailable())
            }
        }
    }
}

fn provider_unavailable() -> RuntimeError {
    RuntimeError::retryable(RuntimeErrorCode::ProviderUnavailable)
}

async fn validate_control_result(
    coordination: &Mutex<HostCoordination>,
    generation: u64,
    result: Result<Value, RuntimeError>,
) -> Result<Value, RuntimeError> {
    if coordination.lock().await.accepts_response(generation) {
        result
    } else {
        Err(provider_unavailable())
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
                runtime_controls: Vec::new(),
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
            "runtimeControls": request.runtime_controls.iter().map(|selection| json!({
                "controlId": selection.control_id,
                "choiceId": selection.choice_id,
            })).collect::<Vec<_>>(),
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

    async fn steer_turn(
        &self,
        request: SteerRuntimeTurnRequest,
    ) -> Result<SteeringAcknowledgement, RuntimeError> {
        request.validate()?;
        let expected = request.clone();
        parse_steering(
            self.call(
                "steer_turn",
                json!({
                    "sessionId": request.session_id,
                    "turnId": request.turn_id,
                    "messageId": request.message_id,
                    "text": request.text,
                }),
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
            Err(error) => {
                if let Some(inner) = inner.upgrade() {
                    let (kind, error_code) = if error.kind() == std::io::ErrorKind::InvalidData {
                        (
                            DiagnosticEventKind::RuntimeHostFrameTooLarge,
                            "runtime_host.frame_too_large",
                        )
                    } else {
                        (
                            DiagnosticEventKind::RuntimeHostReadFailed,
                            "runtime_host.read_failed",
                        )
                    };
                    fail_host(&inner).await;
                    record_host_failure(kind, error_code);
                }
                return;
            }
        };
        let Ok(line) = String::from_utf8(line) else {
            if let Some(inner) = inner.upgrade() {
                fail_host(&inner).await;
                record_host_failure(
                    DiagnosticEventKind::RuntimeHostInvalidUtf8,
                    "runtime_host.invalid_utf8",
                );
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
        record_host_failure(
            DiagnosticEventKind::RuntimeHostStdoutEof,
            "runtime_host.stdout_eof",
        );
    }
}

async fn read_host_diagnostics<R>(mut reader: BufReader<R>)
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut unclassified_reported = false;
    loop {
        let line = match read_bounded_frame_with_limit(&mut reader, MAX_HOST_DIAGNOSTIC_BYTES).await
        {
            Ok(Some(line)) => line,
            Ok(None) => return,
            Err(error) => {
                let (kind, error_code) = if error.kind() == std::io::ErrorKind::InvalidData {
                    (
                        DiagnosticEventKind::RuntimeHostStderrFrameTooLarge,
                        "runtime_host.stderr_frame_too_large",
                    )
                } else {
                    (
                        DiagnosticEventKind::RuntimeHostStderrReadFailed,
                        "runtime_host.stderr_read_failed",
                    )
                };
                record_host_failure(kind, error_code);
                return;
            }
        };
        let diagnostic = String::from_utf8(line)
            .ok()
            .as_deref()
            .map_or(HostDiagnostic::Unclassified, classify_host_diagnostic);
        match diagnostic {
            HostDiagnostic::UnhandledFailure => record_host_failure(
                DiagnosticEventKind::RuntimeHostUnhandledFailure,
                "runtime_host.unhandled_failure",
            ),
            HostDiagnostic::Unclassified if !unclassified_reported => {
                unclassified_reported = true;
                record_host_failure(
                    DiagnosticEventKind::RuntimeHostStderrUnclassified,
                    "runtime_host.stderr_unclassified",
                );
            }
            HostDiagnostic::Unclassified => {}
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HostDiagnostic {
    UnhandledFailure,
    Unclassified,
}

fn classify_host_diagnostic(line: &str) -> HostDiagnostic {
    let Ok(Value::Object(message)) = serde_json::from_str::<Value>(line) else {
        return HostDiagnostic::Unclassified;
    };
    if message.len() == 2
        && message.get("v").and_then(Value::as_u64) == Some(HOST_PROTOCOL_VERSION)
        && message.get("diagnosticCode").and_then(Value::as_str)
            == Some("runtime_host.unhandled_failure")
    {
        HostDiagnostic::UnhandledFailure
    } else {
        HostDiagnostic::Unclassified
    }
}

async fn read_bounded_frame<R>(reader: &mut R) -> std::io::Result<Option<Vec<u8>>>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    read_bounded_frame_with_limit(reader, MAX_HOST_MESSAGE_BYTES).await
}

async fn read_bounded_frame_with_limit<R>(
    reader: &mut R,
    max_bytes: usize,
) -> std::io::Result<Option<Vec<u8>>>
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
            if line.len().saturating_add(payload_len) > max_bytes {
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

async fn dispatch_host_message(inner: &Arc<RuntimeHostInner>, line: &str) {
    let Ok(message) = serde_json::from_str::<Value>(line) else {
        fail_host(inner).await;
        record_host_failure(
            DiagnosticEventKind::RuntimeHostInvalidJson,
            "runtime_host.invalid_json",
        );
        return;
    };
    if message.get("v").and_then(Value::as_u64) != Some(HOST_PROTOCOL_VERSION) {
        fail_host(inner).await;
        record_host_failure(
            DiagnosticEventKind::RuntimeHostProtocolVersionInvalid,
            "runtime_host.protocol_version_invalid",
        );
        return;
    }
    if let Some(id) = message.get("id").and_then(Value::as_str) {
        let disposition = {
            let mut coordination = inner.coordination.lock().await;
            coordination.take_response(id)
        };
        let sender = match disposition {
            PendingResponseDisposition::Deliver(sender) => sender,
            PendingResponseDisposition::Reject(sender) => {
                let _ = sender.send(Err(provider_unavailable()));
                fail_host(inner).await;
                return;
            }
            PendingResponseDisposition::Ignore => return,
            PendingResponseDisposition::Unknown => {
                fail_host(inner).await;
                record_host_failure(
                    DiagnosticEventKind::RuntimeHostUnknownResponse,
                    "runtime_host.unknown_response",
                );
                return;
            }
        };
        let result = match (message.get("error"), message.get("result")) {
            (Some(error), None) => parse_error(error).map_or_else(
                || Err(RuntimeError::new(RuntimeErrorCode::ProtocolViolation)),
                Err,
            ),
            (None, Some(result)) => Ok(result.clone()),
            _ => Err(RuntimeError::new(RuntimeErrorCode::ProtocolViolation)),
        };
        let protocol_violation = result
            .as_ref()
            .is_err_and(|error| error.code == RuntimeErrorCode::ProtocolViolation);
        if protocol_violation {
            fail_host(inner).await;
            record_host_failure(
                DiagnosticEventKind::RuntimeHostInvalidResponse,
                "runtime_host.invalid_response",
            );
        }
        let _ = sender.send(result);
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
                    record_host_failure(
                        DiagnosticEventKind::RuntimeHostInvalidEvent,
                        "runtime_host.invalid_event",
                    );
                    return;
                }
            };
            let key = (event.session_id.clone(), event.turn_id.clone());
            let terminal = matches!(event.kind, RuntimeEventKind::Terminal(_));
            try_deliver_turn_message(inner, &key, Ok(event), terminal).await;
        }
        Some("runtime_error") => {
            let Some(session_id) = message.get("sessionId").and_then(Value::as_str) else {
                fail_host(inner).await;
                record_host_failure(
                    DiagnosticEventKind::RuntimeHostInvalidScope,
                    "runtime_host.invalid_scope",
                );
                return;
            };
            let Some(turn_id) = message.get("turnId").and_then(Value::as_str) else {
                fail_host(inner).await;
                record_host_failure(
                    DiagnosticEventKind::RuntimeHostInvalidScope,
                    "runtime_host.invalid_scope",
                );
                return;
            };
            let Some(error) = message.get("error").and_then(parse_error) else {
                fail_host(inner).await;
                record_host_failure(
                    DiagnosticEventKind::RuntimeHostInvalidError,
                    "runtime_host.invalid_error",
                );
                return;
            };
            let key = (session_id.to_owned(), turn_id.to_owned());
            try_deliver_turn_message(inner, &key, Err(error), true).await;
        }
        _ => {
            fail_host(inner).await;
            record_host_failure(
                DiagnosticEventKind::RuntimeHostUnknownEvent,
                "runtime_host.unknown_event",
            );
        }
    }
}

async fn fail_host(inner: &RuntimeHostInner) {
    let Some(pending) = inner.coordination.lock().await.fence() else {
        return;
    };
    if let Ok(mut child) = inner.child.lock() {
        let _ = child.start_kill();
    }
    let error = provider_unavailable();
    for sender in pending {
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
        let _ = sender.try_send(Err(error.clone()));
    }
    dennett_observability::record(
        DiagnosticEvent::new(DiagnosticEventKind::RuntimeHostFenced)
            .error_code("provider_unavailable")
            .retryable(true),
    );
}

fn record_host_failure(kind: DiagnosticEventKind, error_code: &'static str) {
    dennett_observability::record(
        DiagnosticEvent::new(kind)
            .error_code(error_code)
            .retryable(true),
    );
}

async fn try_deliver_turn_message(
    inner: &Arc<RuntimeHostInner>,
    key: &TurnKey,
    message: Result<RuntimeEvent, RuntimeError>,
    terminal: bool,
) {
    use tokio::sync::mpsc::error::TrySendError;

    let sender = inner.turns.lock().await.get(key).cloned();
    let Some(sender) = sender else {
        return;
    };
    match sender.try_send(message) {
        Ok(()) if terminal => {
            inner.turns.lock().await.remove(key);
        }
        Ok(()) => {}
        Err(TrySendError::Full(_)) => {
            let sender = inner.turns.lock().await.remove(key);
            if let Some(sender) = sender {
                let runtime = HostedAgentRuntime {
                    inner: inner.clone(),
                };
                let cancellation = CancelRuntimeTurnRequest {
                    session_id: key.0.clone(),
                    turn_id: key.1.clone(),
                };
                let inner = inner.clone();
                tokio::spawn(async move {
                    if runtime.cancel_turn(cancellation).await.is_err() {
                        let _ = sender.try_send(Err(RuntimeError::retryable(
                            RuntimeErrorCode::ProviderFailure,
                        )));
                        fail_host(&inner).await;
                        return;
                    }
                    let _ = sender
                        .send(Err(RuntimeError::retryable(
                            RuntimeErrorCode::ProviderFailure,
                        )))
                        .await;
                });
            }
        }
        Err(TrySendError::Closed(_)) => {
            inner.turns.lock().await.remove(key);
            let runtime = HostedAgentRuntime {
                inner: inner.clone(),
            };
            let cancellation = CancelRuntimeTurnRequest {
                session_id: key.0.clone(),
                turn_id: key.1.clone(),
            };
            let inner = inner.clone();
            tokio::spawn(async move {
                if runtime.cancel_turn(cancellation).await.is_err() {
                    fail_host(&inner).await;
                }
            });
        }
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
    let controls = value
        .get("controls")
        .and_then(Value::as_array)
        .map(|controls| {
            controls
                .iter()
                .map(parse_control_descriptor)
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    Ok(RuntimeDescriptor {
        adapter_id,
        runtime_kind,
        capabilities: RuntimeCapabilities {
            streaming: required_bool(capabilities, "streaming")?,
            continuation: required_bool(capabilities, "continuation")?,
            scoped_cancellation: required_bool(capabilities, "scopedCancellation")?,
            deadlines: required_bool(capabilities, "deadlines")?,
            steering: match required_string(capabilities, "steering")?.as_str() {
                "unsupported" => RuntimeSteeringMode::Unsupported,
                "native" => RuntimeSteeringMode::Native,
                "interrupt_and_resume" => RuntimeSteeringMode::InterruptAndResume,
                _ => return Err(RuntimeError::new(RuntimeErrorCode::ProtocolViolation)),
            },
            native_extension_schemas: schemas,
        },
        controls,
    })
}

fn parse_control_descriptor(value: &Value) -> Result<RuntimeControlDescriptor, RuntimeError> {
    let choices = value
        .get("choices")
        .and_then(Value::as_array)
        .ok_or_else(|| RuntimeError::new(RuntimeErrorCode::ProtocolViolation))?
        .iter()
        .map(|choice| {
            let available_when = choice
                .get("availableWhen")
                .and_then(Value::as_array)
                .ok_or_else(|| RuntimeError::new(RuntimeErrorCode::ProtocolViolation))?
                .iter()
                .map(|condition| {
                    let choice_ids = condition
                        .get("choiceIds")
                        .and_then(Value::as_array)
                        .ok_or_else(|| RuntimeError::new(RuntimeErrorCode::ProtocolViolation))?
                        .iter()
                        .map(|choice_id| {
                            choice_id
                                .as_str()
                                .filter(|choice_id| !choice_id.trim().is_empty())
                                .map(str::to_owned)
                                .ok_or_else(|| {
                                    RuntimeError::new(RuntimeErrorCode::ProtocolViolation)
                                })
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok(RuntimeControlCondition {
                        control_id: required_string(condition, "controlId")?,
                        choice_ids,
                    })
                })
                .collect::<Result<Vec<_>, RuntimeError>>()?;
            Ok(RuntimeControlChoice {
                id: required_string(choice, "id")?,
                label: required_string(choice, "label")?,
                description: choice
                    .get("description")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                available_when,
            })
        })
        .collect::<Result<Vec<_>, RuntimeError>>()?;
    Ok(RuntimeControlDescriptor {
        id: required_string(value, "id")?,
        label: required_string(value, "label")?,
        default_choice_id: required_string(value, "defaultChoiceId")?,
        choices,
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

fn parse_steering(
    value: Value,
    expected: &SteerRuntimeTurnRequest,
) -> Result<SteeringAcknowledgement, RuntimeError> {
    let acknowledgement = SteeringAcknowledgement {
        session_id: required_string(&value, "sessionId")?,
        turn_id: required_string(&value, "turnId")?,
        message_id: required_string(&value, "messageId")?,
    };
    if acknowledgement.session_id != expected.session_id
        || acknowledgement.turn_id != expected.turn_id
        || acknowledgement.message_id != expected.message_id
    {
        return Err(RuntimeError::new(RuntimeErrorCode::ScopeMismatch));
    }
    Ok(acknowledgement)
}

fn parse_error(value: &Value) -> Option<RuntimeError> {
    let code = match value.get("code").and_then(Value::as_str) {
        Some("invalid_request") => RuntimeErrorCode::InvalidRequest,
        Some("unsupported") => RuntimeErrorCode::Unsupported,
        Some("protocol_violation") => RuntimeErrorCode::ProtocolViolation,
        Some("scope_mismatch") => RuntimeErrorCode::ScopeMismatch,
        Some("continuation_unavailable") => RuntimeErrorCode::ContinuationUnavailable,
        Some("provider_unavailable") => RuntimeErrorCode::ProviderUnavailable,
        Some("provider_failure") => RuntimeErrorCode::ProviderFailure,
        _ => return None,
    };
    Some(RuntimeError {
        code,
        retryable: value.get("retryable").and_then(Value::as_bool)?,
        recoverable: value.get("recoverable").and_then(Value::as_bool)?,
    })
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

fn locate_host_script() -> Result<PathBuf, RuntimeHostStartError> {
    configured_host_script(std::env::var_os(RUNTIME_HOST_SCRIPT_ENV))
}

fn configured_host_script(path: Option<OsString>) -> Result<PathBuf, RuntimeHostStartError> {
    let path = path
        .map(PathBuf::from)
        .ok_or(RuntimeHostStartError::HostMissing)?;
    if !path.is_absolute() || !path.is_file() {
        return Err(RuntimeHostStartError::HostMissing);
    }
    let canonical = path
        .canonicalize()
        .map_err(|_| RuntimeHostStartError::HostMissing)?;
    Ok(subprocess_compatible_path(canonical))
}

#[cfg(windows)]
fn subprocess_compatible_path(path: PathBuf) -> PathBuf {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    const VERBATIM_PREFIX: &[u16] = &[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16];
    const UNC_PREFIX: &[u16] = &[b'U' as u16, b'N' as u16, b'C' as u16, b'\\' as u16];
    let encoded = path.as_os_str().encode_wide().collect::<Vec<_>>();
    let Some(remainder) = encoded.strip_prefix(VERBATIM_PREFIX) else {
        return path;
    };
    let normalized = if let Some(unc) = remainder.strip_prefix(UNC_PREFIX) {
        let mut value = vec![b'\\' as u16, b'\\' as u16];
        value.extend_from_slice(unc);
        value
    } else {
        remainder.to_vec()
    };
    PathBuf::from(OsString::from_wide(&normalized))
}

#[cfg(not(windows))]
fn subprocess_compatible_path(path: PathBuf) -> PathBuf {
    path
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

impl RuntimeHostStartError {
    #[must_use]
    pub const fn diagnostic_code(self) -> &'static str {
        match self {
            Self::HostMissing => "runtime_host.missing",
            Self::SpawnFailed => "runtime_host.spawn_failed",
            Self::HandshakeFailed => "runtime_host.handshake_failed",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_host_requires_an_explicit_absolute_trusted_path() {
        assert_eq!(
            configured_host_script(None),
            Err(RuntimeHostStartError::HostMissing)
        );
        assert_eq!(
            configured_host_script(Some(OsString::from(
                "services/adapter-host-node/dist/index.js"
            ))),
            Err(RuntimeHostStartError::HostMissing)
        );
    }

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

    #[tokio::test]
    async fn request_admission_cannot_cross_a_concurrent_fence() {
        let coordination = Arc::new(Mutex::new(HostCoordination::default()));
        let mut held = coordination.lock().await;
        let (admitted_sender, admitted_receiver) = oneshot::channel();
        assert_eq!(
            held.admit("already-admitted".to_owned(), admitted_sender),
            Some(0)
        );

        let race_barrier = Arc::new(tokio::sync::Barrier::new(2));
        let racing_coordination = coordination.clone();
        let racing_barrier = race_barrier.clone();
        let racing_admission = tokio::spawn(async move {
            let (sender, _receiver) = oneshot::channel();
            racing_barrier.wait().await;
            racing_coordination
                .lock()
                .await
                .admit("racing-admission".to_owned(), sender)
        });

        race_barrier.wait().await;
        tokio::task::yield_now().await;
        let drained = held.fence().expect("first fence transition");
        assert_eq!(held.generation, 1);
        assert!(held.fenced);
        assert!(held.pending.is_empty());
        drop(held);

        let error = provider_unavailable();
        for sender in drained {
            let _ = sender.send(Err(error.clone()));
        }
        let admitted_result = admitted_receiver
            .await
            .expect("fence must resolve every admitted request")
            .expect_err("fence must reject the admitted request");
        assert_eq!(admitted_result.code, RuntimeErrorCode::ProviderUnavailable);
        assert_eq!(
            racing_admission.await.expect("racing admission task"),
            None,
            "an admission queued at the fence boundary must observe the fence"
        );
        assert!(coordination.lock().await.pending.is_empty());
    }

    #[tokio::test]
    async fn buffered_response_is_rejected_after_its_generation_is_fenced() {
        let coordination = Arc::new(Mutex::new(HostCoordination::default()));
        let (sender, receiver) = oneshot::channel();
        let generation = coordination
            .lock()
            .await
            .admit("buffered-response".to_owned(), sender)
            .expect("request admission");
        let sender = match coordination.lock().await.take_response("buffered-response") {
            PendingResponseDisposition::Deliver(sender) => sender,
            _ => panic!("current-generation response must be deliverable"),
        };
        sender
            .send(Ok(json!({ "status": "buffered-success" })))
            .expect("buffer response before validation");

        let buffered_barrier = Arc::new(tokio::sync::Barrier::new(2));
        let validation_barrier = Arc::new(tokio::sync::Barrier::new(2));
        let validating_coordination = coordination.clone();
        let validating_buffered_barrier = buffered_barrier.clone();
        let validating_validation_barrier = validation_barrier.clone();
        let validation = tokio::spawn(async move {
            let buffered = receiver.await.expect("buffered host response");
            validating_buffered_barrier.wait().await;
            validating_validation_barrier.wait().await;
            validate_control_result(&validating_coordination, generation, buffered).await
        });

        buffered_barrier.wait().await;
        let drained = coordination
            .lock()
            .await
            .fence()
            .expect("first fence transition");
        assert!(drained.is_empty(), "response sender was already detached");
        validation_barrier.wait().await;

        let error = validation
            .await
            .expect("response validation task")
            .expect_err("buffered success must not cross a later fence");
        assert_eq!(error.code, RuntimeErrorCode::ProviderUnavailable);
    }

    #[tokio::test]
    async fn control_timeout_covers_a_blocked_stdin_write_and_fences_the_host() {
        let temp = tempfile::tempdir().expect("temporary runtime host");
        let script = temp.path().join("blocked-stdin-runtime-host.mjs");
        std::fs::write(
            &script,
            r#"
import readline from "node:readline";
const write = value => process.stdout.write(JSON.stringify(value) + "\n");
const input = readline.createInterface({ input: process.stdin });
input.on("line", line => {
  const request = JSON.parse(line);
  if (request.method === "health") {
    write({ v: 1, id: request.id, result: { status: "healthy", protocolVersion: 1 } });
    input.pause();
    process.stdin.pause();
  }
});
"#,
        )
        .expect("write blocked-stdin fixture");
        let script = configured_host_script(Some(script.into_os_string()))
            .expect("canonical blocked-stdin fixture");
        let runtime = HostedAgentRuntime::start_process(Path::new("node"), &script)
            .await
            .expect("start blocked-stdin fixture");

        let error = runtime
            .call_with_timeout(
                "blocked",
                json!({ "payload": "x".repeat(512 * 1024) }),
                Duration::from_millis(100),
            )
            .await
            .expect_err("blocked write must time out");
        assert_eq!(error.code, RuntimeErrorCode::ProviderUnavailable);
        let coordination = runtime.inner.coordination.lock().await;
        assert!(coordination.fenced);
        assert_eq!(coordination.generation, 1);
        assert!(coordination.pending.is_empty());
    }

    #[tokio::test]
    async fn unknown_response_fences_the_host_without_self_deadlock() {
        let temp = tempfile::tempdir().expect("temporary runtime host");
        let script = temp.path().join("unknown-response-runtime-host.mjs");
        std::fs::write(
            &script,
            r#"
import readline from "node:readline";
const write = value => process.stdout.write(JSON.stringify(value) + "\n");
readline.createInterface({ input: process.stdin }).on("line", line => {
  const request = JSON.parse(line);
  if (request.method === "health") {
    write({ v: 1, id: request.id, result: { status: "healthy", protocolVersion: 1 } });
    setTimeout(() => write({ v: 1, id: "unknown-response", result: {} }), 10);
  }
});
"#,
        )
        .expect("write unknown-response fixture");
        let script = configured_host_script(Some(script.into_os_string()))
            .expect("canonical unknown-response fixture");
        let runtime = HostedAgentRuntime::start_process(Path::new("node"), &script)
            .await
            .expect("start unknown-response fixture");

        let error = tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                match runtime.describe().await {
                    Ok(_) => tokio::task::yield_now().await,
                    Err(error) => break error,
                }
            }
        })
        .await
        .expect("unknown response handling must remain bounded");
        assert_eq!(error.code, RuntimeErrorCode::ProviderUnavailable);
        let coordination = runtime.inner.coordination.lock().await;
        assert!(coordination.fenced);
        assert!(coordination.pending.is_empty());
    }

    #[tokio::test]
    async fn a_full_turn_queue_never_blocks_the_shared_host_reader() {
        let temp = tempfile::tempdir().expect("temporary runtime host");
        let script = temp.path().join("overflow-runtime-host.mjs");
        let cancel_marker = temp.path().join("cancel-observed");
        let cancel_marker_json = serde_json::to_string(&cancel_marker.to_string_lossy())
            .expect("encode cancellation marker");
        std::fs::write(
            &script,
            r#"
import fs from "node:fs";
import readline from "node:readline";
const cancelMarker = __CANCEL_MARKER__;
const write = value => process.stdout.write(JSON.stringify(value) + "\n");
readline.createInterface({ input: process.stdin }).on("line", line => {
  const request = JSON.parse(line);
  if (request.method === "health") {
    write({ v: 1, id: request.id, result: { status: "healthy", protocolVersion: 1 } });
  } else if (request.method === "cancel_turn") {
    fs.writeFileSync(cancelMarker, "cancelled");
    write({ v: 1, id: request.id, result: {
      sessionId: request.params.sessionId,
      turnId: request.params.turnId,
      disposition: { type: "requested" }
    }});
  }
});
"#
            .replace("__CANCEL_MARKER__", &cancel_marker_json),
        )
        .expect("write overflow fixture");
        let script = configured_host_script(Some(script.into_os_string()))
            .expect("canonical overflow fixture");
        let runtime = HostedAgentRuntime::start_process(Path::new("node"), &script)
            .await
            .expect("start overflow fixture");
        let inner = runtime.inner.clone();
        let key = ("session-a".to_owned(), "turn-a".to_owned());
        let (sender, mut receiver) = mpsc::channel(1);
        sender
            .send(Ok(test_warning_event(&key, 1)))
            .await
            .expect("prime bounded turn queue");
        inner.turns.lock().await.insert(key.clone(), sender);

        tokio::time::timeout(
            Duration::from_millis(50),
            try_deliver_turn_message(&inner, &key, Ok(test_warning_event(&key, 2)), false),
        )
        .await
        .expect("full turn queue must be detached without blocking");
        assert!(!inner.turns.lock().await.contains_key(&key));
        assert!(receiver.recv().await.is_some());
        let overflow = tokio::time::timeout(Duration::from_secs(1), receiver.recv())
            .await
            .expect("overflow failure delivery timeout")
            .expect("explicit overflow failure");
        assert!(matches!(
            overflow,
            Err(RuntimeError {
                code: RuntimeErrorCode::ProviderFailure,
                retryable: true,
                recoverable: true,
            })
        ));
        tokio::time::timeout(Duration::from_secs(1), async {
            while !cancel_marker.is_file() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("overflow must cancel the provider turn");
    }

    #[tokio::test]
    async fn a_failed_overflow_cancellation_fences_the_runtime_host() {
        let temp = tempfile::tempdir().expect("temporary runtime host");
        let script = temp.path().join("overflow-cancel-failure-runtime-host.mjs");
        std::fs::write(
            &script,
            r#"
import readline from "node:readline";
const write = value => process.stdout.write(JSON.stringify(value) + "\n");
readline.createInterface({ input: process.stdin }).on("line", line => {
  const request = JSON.parse(line);
  if (request.method === "health") {
    write({ v: 1, id: request.id, result: { status: "healthy", protocolVersion: 1 } });
  } else if (request.method === "cancel_turn") {
    write({ v: 1, id: request.id, error: {
      code: "provider_failure", retryable: true, recoverable: true
    }});
  }
});
"#,
        )
        .expect("write failed-cancel fixture");
        let script = configured_host_script(Some(script.into_os_string()))
            .expect("canonical failed-cancel fixture");
        let runtime = HostedAgentRuntime::start_process(Path::new("node"), &script)
            .await
            .expect("start failed-cancel fixture");
        let inner = runtime.inner.clone();
        let key = ("session-fenced".to_owned(), "turn-fenced".to_owned());
        let (sender, mut receiver) = mpsc::channel(1);
        sender
            .send(Ok(test_warning_event(&key, 1)))
            .await
            .expect("prime bounded turn queue");
        inner.turns.lock().await.insert(key.clone(), sender);

        try_deliver_turn_message(&inner, &key, Ok(test_warning_event(&key, 2)), false).await;
        assert!(receiver.recv().await.is_some());
        let _ = tokio::time::timeout(Duration::from_secs(1), receiver.recv()).await;

        let error = tokio::time::timeout(Duration::from_secs(1), runtime.describe())
            .await
            .expect("fenced host must fail promptly")
            .expect_err("failed overflow cancellation must fence the host");
        assert_eq!(error.code, RuntimeErrorCode::ProviderUnavailable);
    }

    fn test_warning_event(key: &TurnKey, sequence: u64) -> RuntimeEvent {
        RuntimeEvent {
            session_id: key.0.clone(),
            turn_id: key.1.clone(),
            sequence,
            kind: RuntimeEventKind::Warning {
                code: "test".to_owned(),
            },
            native_extensions: Vec::new(),
        }
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

    #[test]
    fn stderr_diagnostics_accept_only_the_fixed_safe_schema() {
        assert_eq!(
            classify_host_diagnostic(
                r#"{"v":1,"diagnosticCode":"runtime_host.unhandled_failure"}"#
            ),
            HostDiagnostic::UnhandledFailure
        );
        assert_eq!(
            classify_host_diagnostic("sk-proj-private-provider-secret"),
            HostDiagnostic::Unclassified
        );
        assert_eq!(
            classify_host_diagnostic(
                r#"{"v":1,"diagnosticCode":"runtime_host.unhandled_failure","detail":"private"}"#
            ),
            HostDiagnostic::Unclassified
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
      capabilities: { streaming: true, continuation: true, scopedCancellation: true, deadlines: true, steering: "native", nativeExtensionSchemas: [] },
      controls: [{ id: "model", label: "Model", defaultChoiceId: "fixture-model", choices: [
        { id: "fixture-model", label: "Fixture model", availableWhen: [] }
      ] }]
    }});
  } else if (request.method === "start_turn") {
    const p = request.params;
    if (p.runtimeControls?.[0]?.controlId !== "model" || p.runtimeControls?.[0]?.choiceId !== "fixture-model") {
      write({ v: 1, id: request.id, error: { code: "invalid_request", retryable: false, recoverable: false } });
      return;
    }
    write({ v: 1, id: request.id, result: { started: true } });
    write({ v: 1, event: "runtime_event", payload: { sessionId: p.sessionId, turnId: p.turnId, sequence: 1, kind: { type: "started", continuation: { adapterId: "fixture.runtime", handle: "thread-a" } }, nativeExtensions: [] }});
    write({ v: 1, event: "runtime_event", payload: { sessionId: p.sessionId, turnId: p.turnId, sequence: 2, kind: { type: "text_delta", text: "fixture answer" }, nativeExtensions: [] }});
    write({ v: 1, event: "runtime_event", payload: { sessionId: p.sessionId, turnId: p.turnId, sequence: 3, kind: { type: "terminal", outcome: { type: "completed" }, continuation: { adapterId: "fixture.runtime", handle: "thread-a" } }, nativeExtensions: [] }});
  } else if (request.method === "cancel_turn") {
    write({ v: 1, id: request.id, result: { sessionId: request.params.sessionId, turnId: request.params.turnId, disposition: { type: "requested" } } });
  } else if (request.method === "steer_turn") {
    write({ v: 1, id: request.id, result: {
      sessionId: request.params.sessionId,
      turnId: request.params.turnId,
      messageId: request.params.messageId
    } });
  }
});
"#,
        )
        .expect("write runtime fixture");
        let script = configured_host_script(Some(script.into_os_string()))
            .expect("canonical runtime fixture");
        let runtime = HostedAgentRuntime::start_process(Path::new("node"), &script)
            .await
            .expect("start fixture runtime");
        let descriptor = runtime.describe().await.expect("describe runtime");
        assert_eq!(descriptor.adapter_id, "fixture.runtime");
        assert_eq!(descriptor.controls[0].label, "Model");
        assert_eq!(descriptor.controls[0].choices[0].label, "Fixture model");
        let mut turn = runtime
            .start_turn(RuntimeTurnRequest {
                session_id: "session-a".to_owned(),
                turn_id: "turn-a".to_owned(),
                prompt: "private prompt".to_owned(),
                workspace_path: temp.path().to_string_lossy().into_owned(),
                context_handles: Vec::new(),
                runtime_controls: vec![dennett_agent_core::RuntimeControlSelection {
                    control_id: "model".to_owned(),
                    choice_id: "fixture-model".to_owned(),
                }],
                continuation: None,
                deadline: RuntimeDeadline::after(Duration::from_secs(1)).expect("deadline"),
            })
            .await
            .expect("start turn");
        assert_eq!(
            runtime
                .steer_turn(SteerRuntimeTurnRequest {
                    session_id: "session-a".to_owned(),
                    turn_id: "turn-a".to_owned(),
                    message_id: "message-a".to_owned(),
                    text: "new constraint".to_owned(),
                })
                .await
                .expect("steer active turn"),
            SteeringAcknowledgement {
                session_id: "session-a".to_owned(),
                turn_id: "turn-a".to_owned(),
                message_id: "message-a".to_owned(),
            }
        );
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
