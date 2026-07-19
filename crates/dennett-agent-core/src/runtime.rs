use async_trait::async_trait;
use dennett_kernel::{DennettError, DennettResult};
use std::{collections::VecDeque, fmt, time::Duration};

#[derive(Clone, Debug)]
pub struct AgentRequest {
    pub prompt: String,
    pub context_handles: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct AgentResponse {
    pub text: String,
    pub evidence_handles: Vec<String>,
}

#[derive(Clone, Eq, PartialEq)]
pub struct OpaqueContinuation {
    adapter_id: String,
    handle: String,
}

impl OpaqueContinuation {
    pub fn new(
        adapter_id: impl Into<String>,
        handle: impl Into<String>,
    ) -> Result<Self, RuntimeError> {
        let continuation = Self {
            adapter_id: adapter_id.into(),
            handle: handle.into(),
        };
        if continuation.adapter_id.trim().is_empty() || continuation.handle.trim().is_empty() {
            return Err(RuntimeError::new(RuntimeErrorCode::InvalidRequest));
        }
        Ok(continuation)
    }

    #[must_use]
    pub fn adapter_id(&self) -> &str {
        &self.adapter_id
    }

    pub fn handle_for(&self, adapter_id: &str) -> Result<&str, RuntimeError> {
        if self.adapter_id != adapter_id {
            return Err(RuntimeError::recoverable(
                RuntimeErrorCode::ContinuationUnavailable,
            ));
        }
        Ok(&self.handle)
    }
}

impl fmt::Debug for OpaqueContinuation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpaqueContinuation")
            .field("adapter_id", &self.adapter_id)
            .field("handle", &"[opaque]")
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct NativeExtension {
    pub namespace: String,
    pub schema_version: String,
    pub payload: Vec<u8>,
}

impl fmt::Debug for NativeExtension {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeExtension")
            .field("namespace", &self.namespace)
            .field("schema_version", &self.schema_version)
            .field("payload", &"[opaque]")
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    pub streaming: bool,
    pub continuation: bool,
    pub scoped_cancellation: bool,
    pub deadlines: bool,
    pub native_extension_schemas: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeDescriptor {
    pub adapter_id: String,
    pub runtime_kind: RuntimeKind,
    pub capabilities: RuntimeCapabilities,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeKind {
    NativeAgent,
    GenericLoop,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeDeadline {
    timeout: Duration,
}

impl RuntimeDeadline {
    pub fn after(timeout: Duration) -> Result<Self, RuntimeError> {
        if timeout.is_zero() {
            return Err(RuntimeError::new(RuntimeErrorCode::InvalidRequest));
        }
        Ok(Self { timeout })
    }

    #[must_use]
    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeTurnRequest {
    pub session_id: String,
    pub turn_id: String,
    pub prompt: String,
    pub workspace_path: String,
    pub context_handles: Vec<String>,
    pub continuation: Option<OpaqueContinuation>,
    pub deadline: RuntimeDeadline,
}

impl RuntimeTurnRequest {
    pub fn validate(&self) -> Result<(), RuntimeError> {
        if self.session_id.trim().is_empty()
            || self.turn_id.trim().is_empty()
            || self.prompt.trim().is_empty()
            || self.workspace_path.trim().is_empty()
        {
            return Err(RuntimeError::new(RuntimeErrorCode::InvalidRequest));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeUsage {
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_output_tokens: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeTerminalKind {
    Completed,
    Cancelled,
    TimedOut,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeTerminalOutcome {
    Completed,
    Cancelled {
        partial: bool,
    },
    TimedOut {
        partial: bool,
    },
    Failed {
        code: String,
        retryable: bool,
        recoverable: bool,
        partial: bool,
    },
}

impl RuntimeTerminalOutcome {
    #[must_use]
    pub fn kind(&self) -> RuntimeTerminalKind {
        match self {
            Self::Completed => RuntimeTerminalKind::Completed,
            Self::Cancelled { .. } => RuntimeTerminalKind::Cancelled,
            Self::TimedOut { .. } => RuntimeTerminalKind::TimedOut,
            Self::Failed { .. } => RuntimeTerminalKind::Failed,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeTerminal {
    pub outcome: RuntimeTerminalOutcome,
    pub continuation: Option<OpaqueContinuation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeEventKind {
    Started {
        continuation: Option<OpaqueContinuation>,
    },
    TextDelta {
        text: String,
    },
    Progress {
        phase: String,
        message: Option<String>,
    },
    Usage(RuntimeUsage),
    Warning {
        code: String,
    },
    Terminal(RuntimeTerminal),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeEvent {
    pub session_id: String,
    pub turn_id: String,
    pub sequence: u64,
    pub kind: RuntimeEventKind,
    pub native_extensions: Vec<NativeExtension>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeErrorCode {
    InvalidRequest,
    Unsupported,
    ProtocolViolation,
    ScopeMismatch,
    ContinuationUnavailable,
    ProviderUnavailable,
    ProviderFailure,
}

impl RuntimeErrorCode {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::Unsupported => "unsupported",
            Self::ProtocolViolation => "protocol_violation",
            Self::ScopeMismatch => "scope_mismatch",
            Self::ContinuationUnavailable => "continuation_unavailable",
            Self::ProviderUnavailable => "provider_unavailable",
            Self::ProviderFailure => "provider_failure",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeError {
    pub code: RuntimeErrorCode,
    pub retryable: bool,
    pub recoverable: bool,
}

impl RuntimeError {
    #[must_use]
    pub fn new(code: RuntimeErrorCode) -> Self {
        Self {
            code,
            retryable: false,
            recoverable: false,
        }
    }

    #[must_use]
    pub fn recoverable(code: RuntimeErrorCode) -> Self {
        Self {
            code,
            retryable: false,
            recoverable: true,
        }
    }

    #[must_use]
    pub fn retryable(code: RuntimeErrorCode) -> Self {
        Self {
            code,
            retryable: true,
            recoverable: true,
        }
    }

    fn from_dennett(error: DennettError) -> Self {
        match error {
            DennettError::Cancelled => Self::recoverable(RuntimeErrorCode::ProviderFailure),
            DennettError::Unavailable(_) => Self::retryable(RuntimeErrorCode::ProviderUnavailable),
            DennettError::InvalidInput(_) => Self::new(RuntimeErrorCode::InvalidRequest),
            DennettError::Internal(_) => Self::new(RuntimeErrorCode::ProviderFailure),
        }
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code.as_str())
    }
}

impl std::error::Error for RuntimeError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancelRuntimeTurnRequest {
    pub session_id: String,
    pub turn_id: String,
}

impl CancelRuntimeTurnRequest {
    pub fn validate(&self) -> Result<(), RuntimeError> {
        if self.session_id.trim().is_empty() || self.turn_id.trim().is_empty() {
            return Err(RuntimeError::new(RuntimeErrorCode::InvalidRequest));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancelDisposition {
    Requested,
    AlreadyRequested,
    AlreadyTerminal(RuntimeTerminalKind),
    NotFound,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancellationAcknowledgement {
    pub session_id: String,
    pub turn_id: String,
    pub disposition: CancelDisposition,
}

pub struct RuntimeEventValidator {
    session_id: String,
    turn_id: String,
    next_sequence: u64,
    started: bool,
    terminal: bool,
    continuation: Option<OpaqueContinuation>,
}

impl RuntimeEventValidator {
    #[must_use]
    pub fn new(session_id: impl Into<String>, turn_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            turn_id: turn_id.into(),
            next_sequence: 1,
            started: false,
            terminal: false,
            continuation: None,
        }
    }

    pub fn observe(&mut self, event: &RuntimeEvent) -> Result<(), RuntimeError> {
        if self.terminal
            || event.session_id != self.session_id
            || event.turn_id != self.turn_id
            || event.sequence != self.next_sequence
        {
            return Err(RuntimeError::new(RuntimeErrorCode::ProtocolViolation));
        }
        if event.native_extensions.iter().any(|extension| {
            extension.namespace.trim().is_empty() || extension.schema_version.trim().is_empty()
        }) {
            return Err(RuntimeError::new(RuntimeErrorCode::ProtocolViolation));
        }

        match &event.kind {
            RuntimeEventKind::Started { continuation } if !self.started && event.sequence == 1 => {
                self.started = true;
                self.continuation.clone_from(continuation);
            }
            RuntimeEventKind::Started { .. } => {
                return Err(RuntimeError::new(RuntimeErrorCode::ProtocolViolation));
            }
            RuntimeEventKind::TextDelta { text } if text.is_empty() => {
                return Err(RuntimeError::new(RuntimeErrorCode::ProtocolViolation));
            }
            RuntimeEventKind::Progress { phase, message }
                if phase.trim().is_empty()
                    || message
                        .as_ref()
                        .is_some_and(|value| value.trim().is_empty()) =>
            {
                return Err(RuntimeError::new(RuntimeErrorCode::ProtocolViolation));
            }
            RuntimeEventKind::Warning { code } if code.trim().is_empty() => {
                return Err(RuntimeError::new(RuntimeErrorCode::ProtocolViolation));
            }
            RuntimeEventKind::Terminal(terminal)
                if matches!(
                    &terminal.outcome,
                    RuntimeTerminalOutcome::Failed { code, .. } if code.trim().is_empty()
                ) =>
            {
                return Err(RuntimeError::new(RuntimeErrorCode::ProtocolViolation));
            }
            RuntimeEventKind::Terminal(terminal) if self.started => {
                if self.continuation.is_some()
                    && terminal.continuation.as_ref() != self.continuation.as_ref()
                {
                    return Err(RuntimeError::new(RuntimeErrorCode::ProtocolViolation));
                }
                self.terminal = true;
            }
            _ if !self.started => {
                return Err(RuntimeError::new(RuntimeErrorCode::ProtocolViolation));
            }
            _ => {}
        }
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or_else(|| RuntimeError::new(RuntimeErrorCode::ProtocolViolation))?;
        Ok(())
    }

    pub fn finish(&self) -> Result<(), RuntimeError> {
        if self.terminal {
            Ok(())
        } else {
            Err(RuntimeError::new(RuntimeErrorCode::ProtocolViolation))
        }
    }
}

#[async_trait]
pub trait RuntimeEventStream: Send {
    async fn next_event(&mut self) -> Option<Result<RuntimeEvent, RuntimeError>>;
}

pub struct RuntimeTurn {
    events: Box<dyn RuntimeEventStream>,
}

impl RuntimeTurn {
    #[must_use]
    pub fn from_stream(
        session_id: impl Into<String>,
        turn_id: impl Into<String>,
        stream: Box<dyn RuntimeEventStream>,
    ) -> Self {
        Self {
            events: Box::new(ValidatedRuntimeEventStream::new(
                session_id.into(),
                turn_id.into(),
                stream,
            )),
        }
    }

    pub async fn next_event(&mut self) -> Option<Result<RuntimeEvent, RuntimeError>> {
        self.events.next_event().await
    }
}

struct BufferedRuntimeEventStream {
    events: VecDeque<RuntimeEvent>,
}

impl BufferedRuntimeEventStream {
    fn new(events: Vec<RuntimeEvent>) -> Self {
        Self {
            events: events.into(),
        }
    }
}

#[async_trait]
impl RuntimeEventStream for BufferedRuntimeEventStream {
    async fn next_event(&mut self) -> Option<Result<RuntimeEvent, RuntimeError>> {
        self.events.pop_front().map(Ok)
    }
}

struct ValidatedRuntimeEventStream {
    source: Box<dyn RuntimeEventStream>,
    validator: RuntimeEventValidator,
    finished: bool,
}

impl ValidatedRuntimeEventStream {
    fn new(session_id: String, turn_id: String, source: Box<dyn RuntimeEventStream>) -> Self {
        Self {
            source,
            validator: RuntimeEventValidator::new(session_id, turn_id),
            finished: false,
        }
    }
}

#[async_trait]
impl RuntimeEventStream for ValidatedRuntimeEventStream {
    async fn next_event(&mut self) -> Option<Result<RuntimeEvent, RuntimeError>> {
        if self.finished {
            return None;
        }
        match self.source.next_event().await {
            Some(Ok(event)) => match self.validator.observe(&event) {
                Ok(()) => Some(Ok(event)),
                Err(error) => {
                    self.finished = true;
                    Some(Err(error))
                }
            },
            Some(Err(error)) => {
                self.finished = true;
                Some(Err(error))
            }
            None => {
                self.finished = true;
                self.validator.finish().err().map(Err)
            }
        }
    }
}

#[async_trait]
pub trait AgentRuntimePort: Send + Sync {
    async fn respond(&self, request: AgentRequest) -> DennettResult<AgentResponse>;

    async fn describe(&self) -> Result<RuntimeDescriptor, RuntimeError> {
        Ok(RuntimeDescriptor {
            adapter_id: "legacy.unary".to_owned(),
            runtime_kind: RuntimeKind::GenericLoop,
            capabilities: RuntimeCapabilities {
                streaming: false,
                continuation: false,
                scoped_cancellation: false,
                deadlines: false,
                native_extension_schemas: Vec::new(),
            },
        })
    }

    async fn start_turn(&self, request: RuntimeTurnRequest) -> Result<RuntimeTurn, RuntimeError> {
        request.validate()?;
        if request.continuation.is_some() {
            return Err(RuntimeError::recoverable(
                RuntimeErrorCode::ContinuationUnavailable,
            ));
        }
        let response = self
            .respond(AgentRequest {
                prompt: request.prompt.clone(),
                context_handles: request.context_handles.clone(),
            })
            .await
            .map_err(RuntimeError::from_dennett)?;
        let mut sequence = 1;
        let mut events = vec![RuntimeEvent {
            session_id: request.session_id.clone(),
            turn_id: request.turn_id.clone(),
            sequence,
            kind: RuntimeEventKind::Started { continuation: None },
            native_extensions: Vec::new(),
        }];
        if !response.text.is_empty() {
            sequence += 1;
            events.push(RuntimeEvent {
                session_id: request.session_id.clone(),
                turn_id: request.turn_id.clone(),
                sequence,
                kind: RuntimeEventKind::TextDelta {
                    text: response.text,
                },
                native_extensions: Vec::new(),
            });
        }
        sequence += 1;
        events.push(RuntimeEvent {
            session_id: request.session_id.clone(),
            turn_id: request.turn_id.clone(),
            sequence,
            kind: RuntimeEventKind::Terminal(RuntimeTerminal {
                outcome: RuntimeTerminalOutcome::Completed,
                continuation: None,
            }),
            native_extensions: Vec::new(),
        });
        Ok(RuntimeTurn::from_stream(
            request.session_id,
            request.turn_id,
            Box::new(BufferedRuntimeEventStream::new(events)),
        ))
    }

    async fn cancel_turn(
        &self,
        request: CancelRuntimeTurnRequest,
    ) -> Result<CancellationAcknowledgement, RuntimeError> {
        request.validate()?;
        Err(RuntimeError::new(RuntimeErrorCode::Unsupported))
    }
}
