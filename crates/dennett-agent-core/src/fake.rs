use crate::runtime::{
    AgentRequest, AgentResponse, AgentRuntimePort, CancelDisposition, CancelRuntimeTurnRequest,
    CancellationAcknowledgement, NativeExtension, OpaqueContinuation, RuntimeActivityStatus,
    RuntimeCapabilities, RuntimeDescriptor, RuntimeError, RuntimeErrorCode, RuntimeEvent,
    RuntimeEventKind, RuntimeEventStream, RuntimeKind, RuntimeTerminal, RuntimeTerminalKind,
    RuntimeTerminalOutcome, RuntimeTurn, RuntimeTurnRequest, RuntimeUsage,
};
use async_trait::async_trait;
use dennett_kernel::DennettResult;
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

const FAKE_ADAPTER_ID: &str = "dennett.fake";

#[derive(Default)]
pub struct FakeAgentRuntime;

#[async_trait]
impl AgentRuntimePort for FakeAgentRuntime {
    async fn respond(&self, request: AgentRequest) -> DennettResult<AgentResponse> {
        Ok(AgentResponse {
            text: format!("Dennett skeleton received: {}", request.prompt),
            evidence_handles: request.context_handles,
        })
    }

    async fn describe(&self) -> Result<RuntimeDescriptor, RuntimeError> {
        Ok(RuntimeDescriptor {
            adapter_id: FAKE_ADAPTER_ID.to_owned(),
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FakeRuntimeStep {
    TextDelta(String),
    Progress {
        phase: String,
        message: Option<String>,
    },
    ProgressWithNativeExtension {
        phase: String,
        message: Option<String>,
        extension: NativeExtension,
    },
    Usage(RuntimeUsage),
    Advance(Duration),
    Complete,
    Fail {
        code: String,
        retryable: bool,
        recoverable: bool,
    },
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct TurnKey {
    session_id: String,
    turn_id: String,
}

impl TurnKey {
    fn new(session_id: impl Into<String>, turn_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            turn_id: turn_id.into(),
        }
    }
}

#[derive(Clone, Debug)]
enum FakeTurnStatus {
    Active { cancel_requested: bool },
    Terminal(RuntimeTerminalKind),
}

#[derive(Default)]
struct FakeRuntimeState {
    scripts: VecDeque<VecDeque<FakeRuntimeStep>>,
    turns: HashMap<TurnKey, FakeTurnStatus>,
    continuation_counter: u64,
}

#[derive(Clone, Default)]
pub struct ScriptedFakeAgentRuntime {
    state: Arc<Mutex<FakeRuntimeState>>,
}

impl ScriptedFakeAgentRuntime {
    pub fn push_script(
        &self,
        steps: impl IntoIterator<Item = FakeRuntimeStep>,
    ) -> Result<(), RuntimeError> {
        let mut state = self.lock_state()?;
        state.scripts.push_back(steps.into_iter().collect());
        Ok(())
    }

    fn lock_state(&self) -> Result<MutexGuard<'_, FakeRuntimeState>, RuntimeError> {
        self.state
            .lock()
            .map_err(|_| RuntimeError::new(RuntimeErrorCode::ProviderFailure))
    }
}

#[async_trait]
impl AgentRuntimePort for ScriptedFakeAgentRuntime {
    async fn respond(&self, request: AgentRequest) -> DennettResult<AgentResponse> {
        Ok(AgentResponse {
            text: format!("Dennett scripted fake received: {}", request.prompt),
            evidence_handles: request.context_handles,
        })
    }

    async fn describe(&self) -> Result<RuntimeDescriptor, RuntimeError> {
        Ok(RuntimeDescriptor {
            adapter_id: FAKE_ADAPTER_ID.to_owned(),
            runtime_kind: RuntimeKind::GenericLoop,
            capabilities: RuntimeCapabilities {
                streaming: true,
                continuation: true,
                scoped_cancellation: true,
                deadlines: true,
                native_extension_schemas: Vec::new(),
            },
        })
    }

    async fn start_turn(&self, request: RuntimeTurnRequest) -> Result<RuntimeTurn, RuntimeError> {
        request.validate()?;
        if let Some(continuation) = &request.continuation {
            continuation.handle_for(FAKE_ADAPTER_ID)?;
        }
        let key = TurnKey::new(request.session_id.clone(), request.turn_id.clone());
        let (steps, continuation) = {
            let mut state = self.lock_state()?;
            if state.turns.contains_key(&key) {
                return Err(RuntimeError::new(RuntimeErrorCode::InvalidRequest));
            }
            let steps = state
                .scripts
                .pop_front()
                .ok_or_else(|| RuntimeError::new(RuntimeErrorCode::ProviderUnavailable))?;
            state.continuation_counter += 1;
            let continuation = match request.continuation {
                Some(continuation) => continuation,
                None => OpaqueContinuation::new(
                    FAKE_ADAPTER_ID,
                    format!("fake-continuation-{}", state.continuation_counter),
                )?,
            };
            state.turns.insert(
                key.clone(),
                FakeTurnStatus::Active {
                    cancel_requested: false,
                },
            );
            (steps, continuation)
        };

        let stream = ScriptedFakeEventStream {
            state: Arc::clone(&self.state),
            key: key.clone(),
            steps,
            continuation,
            next_sequence: 1,
            started: false,
            terminal: false,
            emitted_text: false,
            elapsed: Duration::ZERO,
            deadline: request.deadline.timeout(),
        };
        Ok(RuntimeTurn::from_stream(
            key.session_id,
            key.turn_id,
            Box::new(stream),
        ))
    }

    async fn cancel_turn(
        &self,
        request: CancelRuntimeTurnRequest,
    ) -> Result<CancellationAcknowledgement, RuntimeError> {
        request.validate()?;
        let key = TurnKey::new(request.session_id.clone(), request.turn_id.clone());
        let disposition = match self.lock_state()?.turns.get_mut(&key) {
            Some(FakeTurnStatus::Active { cancel_requested }) if *cancel_requested => {
                CancelDisposition::AlreadyRequested
            }
            Some(FakeTurnStatus::Active { cancel_requested }) => {
                *cancel_requested = true;
                CancelDisposition::Requested
            }
            Some(FakeTurnStatus::Terminal(kind)) => CancelDisposition::AlreadyTerminal(*kind),
            None => CancelDisposition::NotFound,
        };
        Ok(CancellationAcknowledgement {
            session_id: request.session_id,
            turn_id: request.turn_id,
            disposition,
        })
    }
}

struct ScriptedFakeEventStream {
    state: Arc<Mutex<FakeRuntimeState>>,
    key: TurnKey,
    steps: VecDeque<FakeRuntimeStep>,
    continuation: OpaqueContinuation,
    next_sequence: u64,
    started: bool,
    terminal: bool,
    emitted_text: bool,
    elapsed: Duration,
    deadline: Duration,
}

impl ScriptedFakeEventStream {
    fn event(&mut self, kind: RuntimeEventKind) -> RuntimeEvent {
        self.event_with_extensions(kind, Vec::new())
    }

    fn event_with_extensions(
        &mut self,
        kind: RuntimeEventKind,
        native_extensions: Vec<NativeExtension>,
    ) -> RuntimeEvent {
        let event = RuntimeEvent {
            session_id: self.key.session_id.clone(),
            turn_id: self.key.turn_id.clone(),
            sequence: self.next_sequence,
            kind,
            native_extensions,
        };
        self.next_sequence += 1;
        event
    }

    fn mark_terminal(
        &mut self,
        preferred: RuntimeTerminalKind,
    ) -> Result<RuntimeTerminalKind, RuntimeError> {
        self.terminal = true;
        self.steps.clear();
        let mut state = self
            .state
            .lock()
            .map_err(|_| RuntimeError::new(RuntimeErrorCode::ProviderFailure))?;
        let resolved = match state.turns.get(&self.key) {
            Some(FakeTurnStatus::Active {
                cancel_requested: true,
            }) => RuntimeTerminalKind::Cancelled,
            Some(FakeTurnStatus::Active {
                cancel_requested: false,
            }) => preferred,
            Some(FakeTurnStatus::Terminal(_)) | None => {
                return Err(RuntimeError::new(RuntimeErrorCode::ProtocolViolation));
            }
        };
        state
            .turns
            .insert(self.key.clone(), FakeTurnStatus::Terminal(resolved));
        Ok(resolved)
    }

    fn cancellation_requested(&self) -> Result<bool, RuntimeError> {
        let state = self
            .state
            .lock()
            .map_err(|_| RuntimeError::new(RuntimeErrorCode::ProviderFailure))?;
        Ok(matches!(
            state.turns.get(&self.key),
            Some(FakeTurnStatus::Active {
                cancel_requested: true
            })
        ))
    }
}

#[async_trait]
impl RuntimeEventStream for ScriptedFakeEventStream {
    async fn next_event(&mut self) -> Option<Result<RuntimeEvent, RuntimeError>> {
        if self.terminal {
            return None;
        }
        if !self.started {
            self.started = true;
            let continuation = self.continuation.clone();
            return Some(Ok(self.event(RuntimeEventKind::Started {
                continuation: Some(continuation),
            })));
        }
        match self.cancellation_requested() {
            Ok(true) => {
                let resolved = match self.mark_terminal(RuntimeTerminalKind::Cancelled) {
                    Ok(resolved) => resolved,
                    Err(error) => return Some(Err(error)),
                };
                debug_assert_eq!(resolved, RuntimeTerminalKind::Cancelled);
                let continuation = self.continuation.clone();
                let partial = self.emitted_text;
                return Some(Ok(self.event(RuntimeEventKind::Terminal(
                    RuntimeTerminal {
                        outcome: RuntimeTerminalOutcome::Cancelled { partial },
                        continuation: Some(continuation),
                    },
                ))));
            }
            Err(error) => return Some(Err(error)),
            Ok(false) => {}
        }

        loop {
            let step = self.steps.pop_front()?;
            let kind = match step {
                FakeRuntimeStep::TextDelta(text) => {
                    self.emitted_text |= !text.is_empty();
                    RuntimeEventKind::TextDelta { text }
                }
                FakeRuntimeStep::Progress { phase, message } => RuntimeEventKind::Progress {
                    activity_id: None,
                    phase,
                    message,
                    status: RuntimeActivityStatus::Completed,
                },
                FakeRuntimeStep::ProgressWithNativeExtension {
                    phase,
                    message,
                    extension,
                } => {
                    return Some(Ok(self.event_with_extensions(
                        RuntimeEventKind::Progress {
                            activity_id: None,
                            phase,
                            message,
                            status: RuntimeActivityStatus::Completed,
                        },
                        vec![extension],
                    )));
                }
                FakeRuntimeStep::Usage(usage) => RuntimeEventKind::Usage(usage),
                FakeRuntimeStep::Advance(duration) => {
                    self.elapsed = self.elapsed.saturating_add(duration);
                    if self.elapsed < self.deadline {
                        continue;
                    }
                    let resolved = match self.mark_terminal(RuntimeTerminalKind::TimedOut) {
                        Ok(resolved) => resolved,
                        Err(error) => return Some(Err(error)),
                    };
                    RuntimeEventKind::Terminal(RuntimeTerminal {
                        outcome: match resolved {
                            RuntimeTerminalKind::TimedOut => RuntimeTerminalOutcome::TimedOut {
                                partial: self.emitted_text,
                            },
                            RuntimeTerminalKind::Cancelled => RuntimeTerminalOutcome::Cancelled {
                                partial: self.emitted_text,
                            },
                            _ => {
                                return Some(Err(RuntimeError::new(
                                    RuntimeErrorCode::ProtocolViolation,
                                )));
                            }
                        },
                        continuation: Some(self.continuation.clone()),
                    })
                }
                FakeRuntimeStep::Complete => {
                    let resolved = match self.mark_terminal(RuntimeTerminalKind::Completed) {
                        Ok(resolved) => resolved,
                        Err(error) => return Some(Err(error)),
                    };
                    RuntimeEventKind::Terminal(RuntimeTerminal {
                        outcome: match resolved {
                            RuntimeTerminalKind::Completed => RuntimeTerminalOutcome::Completed,
                            RuntimeTerminalKind::Cancelled => RuntimeTerminalOutcome::Cancelled {
                                partial: self.emitted_text,
                            },
                            _ => {
                                return Some(Err(RuntimeError::new(
                                    RuntimeErrorCode::ProtocolViolation,
                                )));
                            }
                        },
                        continuation: Some(self.continuation.clone()),
                    })
                }
                FakeRuntimeStep::Fail {
                    code,
                    retryable,
                    recoverable,
                } => {
                    let resolved = match self.mark_terminal(RuntimeTerminalKind::Failed) {
                        Ok(resolved) => resolved,
                        Err(error) => return Some(Err(error)),
                    };
                    RuntimeEventKind::Terminal(RuntimeTerminal {
                        outcome: match resolved {
                            RuntimeTerminalKind::Failed => RuntimeTerminalOutcome::Failed {
                                code,
                                retryable,
                                recoverable,
                                partial: self.emitted_text,
                            },
                            RuntimeTerminalKind::Cancelled => RuntimeTerminalOutcome::Cancelled {
                                partial: self.emitted_text,
                            },
                            _ => {
                                return Some(Err(RuntimeError::new(
                                    RuntimeErrorCode::ProtocolViolation,
                                )));
                            }
                        },
                        continuation: Some(self.continuation.clone()),
                    })
                }
            };
            return Some(Ok(self.event(kind)));
        }
    }
}
