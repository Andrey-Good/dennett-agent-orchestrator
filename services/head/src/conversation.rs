use crate::draft::{ComposerDraftApplication, SessionOperationLocks};
use crate::session::{
    AcceptedTurn, AgentActivityUpdate, SessionCoordinator, SessionSubscription,
    UserSteerCompletion, UserSteerRequest,
};
use crate::system::{
    ProjectSummary, SessionSummary, SystemHealth, SystemMutation, SystemProjection,
};
use dennett_agent_core::{
    AgentRuntimePort, CancelDisposition, CancelRuntimeTurnRequest, CancellationAcknowledgement,
    InMemoryRuntimeContinuationStore, NativeExtension, RuntimeActivityStatus,
    RuntimeContinuationError, RuntimeContinuationPort, RuntimeControlSelection, RuntimeDeadline,
    RuntimeError, RuntimeErrorCode, RuntimeEvent, RuntimeEventKind, RuntimeSteeringMode,
    RuntimeTerminalKind, RuntimeTerminalOutcome, RuntimeTurnRequest, SteerRuntimeTurnRequest,
};
use dennett_contracts::{CommandId, ProjectId, SessionEventId, SessionId, TurnId};
use dennett_memory_core::session::{
    ProjectSessionSnapshot, SafeSessionError, SessionActivityStatus, SessionCommit,
    SessionJournalError, SessionNativeExtension, SessionResult, SessionTurnOutcome,
    SessionTurnRole, SessionTurnState,
};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Mutex, watch};
use tracing::Instrument;

const DEFAULT_TURN_TIMEOUT: Duration = Duration::from_secs(120);
const PROVIDER_CONTROL_TIMEOUT: Duration = Duration::from_secs(2);
const COMMIT_RETRY_DELAY: Duration = Duration::from_millis(100);
const MAX_TURN_OUTPUT_BYTES: usize = 768 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ActiveTurnPhase {
    Preparing,
    Starting,
    Running,
    Finishing,
}

#[derive(Clone, Debug)]
struct ActiveTurnControl {
    phase: watch::Sender<ActiveTurnPhase>,
    cancel_requested: bool,
    cancel_signal: watch::Sender<bool>,
    cancel_gate: Arc<Mutex<()>>,
}

#[derive(Clone, Debug)]
struct RuntimeTurnInput {
    prompt: String,
    context_handles: Vec<String>,
    runtime_controls: Vec<RuntimeControlSelection>,
    workspace_path: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalProject {
    pub project_id: ProjectId,
    pub display_name: String,
    pub workspace_path: String,
    pub standalone_workspace_path: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TurnDeliveryMode {
    NewTurn,
    SteerNow,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceContext {
    pub installation_id: String,
    pub device_id: String,
    pub correlation_id: String,
    pub authority_epoch: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversationTurnRequest {
    pub trace: TraceContext,
    pub command_id: CommandId,
    pub project_id: Option<ProjectId>,
    pub session_id: SessionId,
    pub expected_revision: Option<u64>,
    pub text: String,
    pub context_handles: Vec<String>,
    pub runtime_controls: Vec<RuntimeControlSelection>,
    pub delivery_mode: TurnDeliveryMode,
    pub expected_active_turn_id: Option<TurnId>,
}

#[derive(Clone)]
pub struct ConversationApplication {
    sessions: SessionCoordinator,
    system: Arc<SystemProjection>,
    runtime: Arc<dyn AgentRuntimePort>,
    project: LocalProject,
    continuations: Arc<dyn RuntimeContinuationPort>,
    active_turns: Arc<Mutex<HashMap<(SessionId, TurnId), ActiveTurnControl>>>,
    drafts: Option<ComposerDraftApplication>,
    turn_gates: SessionOperationLocks,
    turn_timeout: Duration,
}

impl ConversationApplication {
    #[must_use]
    pub fn new(
        sessions: SessionCoordinator,
        system: Arc<SystemProjection>,
        runtime: Arc<dyn AgentRuntimePort>,
        project: LocalProject,
    ) -> Self {
        Self {
            sessions,
            system,
            runtime,
            project,
            continuations: Arc::new(InMemoryRuntimeContinuationStore::default()),
            active_turns: Arc::new(Mutex::new(HashMap::new())),
            drafts: None,
            turn_gates: SessionOperationLocks::default(),
            turn_timeout: DEFAULT_TURN_TIMEOUT,
        }
    }

    #[must_use]
    pub fn with_turn_timeout(mut self, turn_timeout: Duration) -> Self {
        self.turn_timeout = turn_timeout;
        self
    }

    #[must_use]
    pub fn with_drafts(mut self, drafts: ComposerDraftApplication) -> Self {
        self.drafts = Some(drafts);
        self
    }

    #[must_use]
    pub fn with_continuations(mut self, continuations: Arc<dyn RuntimeContinuationPort>) -> Self {
        self.continuations = continuations;
        self
    }

    #[must_use]
    pub fn project(&self) -> &LocalProject {
        &self.project
    }

    pub async fn initialize(
        &self,
        create_command_id: CommandId,
        default_title: String,
    ) -> Result<ProjectSessionSnapshot, ConversationError> {
        let mut restored = self
            .sessions
            .restore_all()
            .await?
            .into_iter()
            .filter(|snapshot| self.scope_is_local(snapshot.session.project_id))
            .collect::<Vec<_>>();
        if !restored
            .iter()
            .any(|snapshot| snapshot.session.project_id == Some(self.project.project_id))
        {
            restored.push(
                self.sessions
                    .create_session(
                        create_command_id,
                        Some(self.project.project_id),
                        default_title,
                        unix_time_ms(),
                    )
                    .await?
                    .snapshot,
            );
        }

        for snapshot in &mut restored {
            if let Some(turn_id) = snapshot.session.active_turn_id {
                let pending_steers = snapshot
                    .turns
                    .iter()
                    .filter(|turn| {
                        turn.role == SessionTurnRole::User
                            && turn.state == SessionTurnState::Accepted
                    })
                    .map(|turn| turn.turn_id)
                    .collect::<Vec<_>>();
                for user_turn_id in pending_steers {
                    *snapshot = self
                        .sessions
                        .finish_steer(UserSteerCompletion {
                            event_id: SessionEventId::new(),
                            session_id: snapshot.session.session_id,
                            user_turn_id,
                            agent_turn_id: turn_id,
                            state: SessionTurnState::Failed,
                            error: Some(SafeSessionError {
                                code: "runtime_interrupted".to_owned(),
                                message_key: "session.steer_interrupted".to_owned(),
                                details_handle: None,
                            }),
                            committed_at_unix_ms: unix_time_ms(),
                        })
                        .await?
                        .snapshot;
                }
                *snapshot = self
                    .sessions
                    .finish_turn(
                        SessionEventId::new(),
                        snapshot.session.session_id,
                        turn_id,
                        SessionTurnState::Failed,
                        Some(SessionTurnOutcome::Error(SafeSessionError {
                            code: "runtime_interrupted".to_owned(),
                            message_key: "session.runtime_interrupted".to_owned(),
                            details_handle: None,
                        })),
                        unix_time_ms(),
                    )
                    .await?
                    .snapshot;
            }
        }

        let active = restored
            .iter()
            .max_by_key(|snapshot| snapshot.session.last_activity_unix_ms)
            .cloned()
            .ok_or(ConversationError::SessionUnavailable)?;
        let mut mutations = vec![SystemMutation::UpsertProject(ProjectSummary {
            project_id: self.project.project_id.0.to_string(),
            display_name: self.project.display_name.clone(),
            revision: 1,
        })];
        mutations.extend(
            restored
                .iter()
                .map(|snapshot| SystemMutation::UpsertSession(session_summary(snapshot))),
        );
        mutations.push(SystemMutation::Select {
            project_id: active.session.project_id.map(|id| id.0.to_string()),
            session_id: Some(active.session.session_id.0.to_string()),
        });
        mutations.push(SystemMutation::SetHealth(SystemHealth::Ready));
        self.system.apply(mutations).await;
        Ok(active)
    }

    pub async fn create_session(
        &self,
        command_id: CommandId,
        project_id: Option<ProjectId>,
        title: String,
    ) -> Result<SessionCommit, ConversationError> {
        self.require_scope(project_id)?;
        let commit = self
            .sessions
            .create_session(command_id, project_id, title, unix_time_ms())
            .await?;
        self.system
            .apply(vec![
                SystemMutation::UpsertSession(session_summary(&commit.snapshot)),
                SystemMutation::Select {
                    project_id: project_id.map(|id| id.0.to_string()),
                    session_id: Some(commit.snapshot.session.session_id.0.to_string()),
                },
            ])
            .await;
        Ok(commit)
    }

    pub async fn send_turn(
        &self,
        request: ConversationTurnRequest,
    ) -> Result<AcceptedTurn, ConversationError> {
        let ConversationTurnRequest {
            trace,
            command_id,
            project_id,
            session_id,
            expected_revision,
            text,
            context_handles,
            runtime_controls,
            delivery_mode,
            expected_active_turn_id,
        } = request;
        self.require_scope(project_id)?;
        if text.trim().is_empty() {
            return Err(ConversationError::InvalidRequest);
        }
        let _turn_guard = self.turn_gates.acquire(session_id).await;
        let draft_guard = match &self.drafts {
            Some(drafts) => Some(drafts.acquire(session_id).await),
            None => None,
        };
        let existing = self.sessions.event_for_command(command_id).await?;
        let snapshot = self.sessions.restore(session_id).await?;
        if snapshot.session.project_id != project_id {
            return Err(ConversationError::ScopeMismatch);
        }
        if let Some(existing) = existing.as_ref()
            && let dennett_memory_core::session::SessionEventBody::UserSteerRequested {
                agent_turn_id,
                ..
            } = &existing.body
        {
            if delivery_mode != TurnDeliveryMode::SteerNow
                || expected_active_turn_id != Some(*agent_turn_id)
                || !context_handles.is_empty()
                || !runtime_controls.is_empty()
            {
                return Err(ConversationError::ScopeMismatch);
            }
            self.require_native_steering().await?;
            let accepted = self
                .deliver_native_steer(
                    trace,
                    command_id,
                    project_id,
                    session_id,
                    *agent_turn_id,
                    text,
                )
                .await?;
            if let Some(drafts) = &self.drafts
                && let Err(error) = drafts.discard_accepted(session_id, command_id).await
            {
                tracing::warn!(code = ?error, "accepted steer draft cleanup will be retried");
            }
            drop(draft_guard);
            return Ok(accepted);
        }
        if existing.is_none() {
            if let Some(active_turn_id) = snapshot.session.active_turn_id {
                if delivery_mode != TurnDeliveryMode::SteerNow {
                    return Err(ConversationError::TurnAlreadyActive);
                }
                if expected_active_turn_id != Some(active_turn_id)
                    || !context_handles.is_empty()
                    || !runtime_controls.is_empty()
                {
                    return Err(ConversationError::ScopeMismatch);
                }
                self.require_native_steering().await?;
                let accepted = self
                    .deliver_native_steer(
                        trace,
                        command_id,
                        project_id,
                        session_id,
                        active_turn_id,
                        text,
                    )
                    .await?;
                if let Some(drafts) = &self.drafts
                    && let Err(error) = drafts.discard_accepted(session_id, command_id).await
                {
                    tracing::warn!(code = ?error, "accepted steer draft cleanup will be retried");
                }
                drop(draft_guard);
                return Ok(accepted);
            }
            if delivery_mode == TurnDeliveryMode::SteerNow || expected_active_turn_id.is_some() {
                return Err(ConversationError::ScopeMismatch);
            }
            if let Some(expected) = expected_revision
                && expected != snapshot.session.revision
            {
                return Err(SessionJournalError::RevisionConflict {
                    expected,
                    actual: snapshot.session.revision,
                }
                .into());
            }
        } else if expected_active_turn_id.is_some() {
            return Err(ConversationError::ScopeMismatch);
        }
        let admission_revision = self.sessions.restore(session_id).await?.session.revision;
        let accepted = self
            .sessions
            .accept_turn(
                command_id,
                project_id,
                session_id,
                text.clone(),
                Some(admission_revision),
                unix_time_ms(),
            )
            .await?;
        if let Some(drafts) = &self.drafts
            && let Err(error) = drafts.discard_accepted(session_id, command_id).await
        {
            tracing::warn!(code = ?error, "accepted turn draft cleanup will be retried");
        }
        drop(draft_guard);
        let turn_id = accepted.agent_turn_id;
        if !accepted.replayed {
            let (cancel_signal, cancel_rx) = watch::channel(false);
            let (phase, _) = watch::channel(ActiveTurnPhase::Preparing);
            self.active_turns.lock().await.insert(
                (session_id, turn_id),
                ActiveTurnControl {
                    phase,
                    cancel_requested: false,
                    cancel_signal,
                    cancel_gate: Arc::new(Mutex::new(())),
                },
            );
            self.system
                .apply(vec![SystemMutation::UpsertSession(session_summary(
                    &accepted.commit.snapshot,
                ))])
                .await;
            let application = self.clone();
            let workspace_path = self.workspace_for(project_id).to_owned();
            let project_span_id = project_id.map(|id| id.0.to_string()).unwrap_or_default();
            let span = tracing::info_span!(
                "project_chat_turn",
                dennett.installation.id = %trace.installation_id,
                dennett.device.id = %trace.device_id,
                dennett.component = "dennett-head",
                dennett.project.id = %project_span_id,
                dennett.session.id = %session_id.0,
                dennett.command.id = %command_id.0,
                dennett.runtime.turn.id = %turn_id.0,
                dennett.provider.id = tracing::field::Empty,
                dennett.memory.event.id = tracing::field::Empty,
                dennett.turn.terminal_state = tracing::field::Empty,
                dennett.protocol.version = 1_u64,
                dennett.authority.epoch = trace.authority_epoch,
                correlation_id = %trace.correlation_id,
            );
            let instrument_span = span.clone();
            tokio::spawn(
                async move {
                    application
                        .run_turn(
                            session_id,
                            turn_id,
                            RuntimeTurnInput {
                                prompt: text,
                                context_handles,
                                runtime_controls,
                                workspace_path,
                            },
                            cancel_rx,
                            span,
                        )
                        .await;
                }
                .instrument(instrument_span),
            );
        }
        Ok(accepted)
    }

    async fn require_native_steering(&self) -> Result<(), ConversationError> {
        let descriptor = tokio::time::timeout(PROVIDER_CONTROL_TIMEOUT, self.runtime.describe())
            .await
            .map_err(|_| RuntimeError::retryable(RuntimeErrorCode::ProviderUnavailable))??;
        if descriptor.capabilities.steering != RuntimeSteeringMode::Native {
            return Err(RuntimeError::new(RuntimeErrorCode::Unsupported).into());
        }
        Ok(())
    }

    async fn deliver_native_steer(
        &self,
        trace: TraceContext,
        command_id: CommandId,
        project_id: Option<ProjectId>,
        session_id: SessionId,
        agent_turn_id: TurnId,
        text: String,
    ) -> Result<AcceptedTurn, ConversationError> {
        let mut accepted = self
            .sessions
            .request_steer(UserSteerRequest {
                command_id,
                project_id,
                session_id,
                agent_turn_id,
                text: text.clone(),
                expected_revision: None,
                committed_at_unix_ms: unix_time_ms(),
            })
            .await?;
        self.system
            .apply(vec![SystemMutation::UpsertSession(session_summary(
                &accepted.commit.snapshot,
            ))])
            .await;
        let user_state = accepted
            .commit
            .snapshot
            .turns
            .iter()
            .find(|turn| turn.turn_id == accepted.user_turn_id)
            .map(|turn| turn.state)
            .ok_or(ConversationError::SessionUnavailable)?;
        match user_state {
            SessionTurnState::Completed => return Ok(accepted),
            SessionTurnState::Failed => {
                return Err(RuntimeError::recoverable(RuntimeErrorCode::ProviderFailure).into());
            }
            SessionTurnState::Accepted => {}
            _ => return Err(RuntimeError::new(RuntimeErrorCode::ProtocolViolation).into()),
        }

        let span = tracing::info_span!(
            "project_chat_steer",
            dennett.installation.id = %trace.installation_id,
            dennett.device.id = %trace.device_id,
            dennett.component = "dennett-head",
            dennett.project.id = %project_id.map(|id| id.0.to_string()).unwrap_or_default(),
            dennett.session.id = %session_id.0,
            dennett.command.id = %command_id.0,
            dennett.runtime.turn.id = %agent_turn_id.0,
            dennett.provider.id = tracing::field::Empty,
            dennett.memory.event.id = tracing::field::Empty,
            dennett.protocol.version = 1_u64,
            dennett.authority.epoch = trace.authority_epoch,
            correlation_id = %trace.correlation_id,
        );
        let control_deadline = tokio::time::Instant::now() + PROVIDER_CONTROL_TIMEOUT;
        let steering = match self
            .wait_until_runtime_steerable(session_id, agent_turn_id, control_deadline)
            .await
        {
            Ok(()) => match tokio::time::timeout_at(
                control_deadline,
                self.runtime.steer_turn(SteerRuntimeTurnRequest {
                    session_id: session_id.0.to_string(),
                    turn_id: agent_turn_id.0.to_string(),
                    message_id: command_id.0.to_string(),
                    text,
                }),
            )
            .instrument(span.clone())
            .await
            {
                Ok(result) => result,
                Err(_) => Err(RuntimeError::retryable(
                    RuntimeErrorCode::ProviderUnavailable,
                )),
            },
            Err(error) => Err(error),
        };

        let (state, safe_error, runtime_error) = match steering {
            Ok(_) => (SessionTurnState::Completed, None, None),
            Err(error) => (
                SessionTurnState::Failed,
                Some(SafeSessionError {
                    code: error.code.as_str().to_owned(),
                    message_key: "session.steer_failed".to_owned(),
                    details_handle: None,
                }),
                Some(error),
            ),
        };
        let commit = self
            .finish_steer_reliably(
                session_id,
                accepted.user_turn_id,
                agent_turn_id,
                state,
                safe_error,
            )
            .await?;
        record_span_text(
            &span,
            "dennett.memory.event.id",
            &commit.event.event_id.0.to_string(),
        );
        self.system
            .apply(vec![SystemMutation::UpsertSession(session_summary(
                &commit.snapshot,
            ))])
            .await;
        accepted.commit = commit;
        if let Some(error) = runtime_error {
            return Err(error.into());
        }
        Ok(accepted)
    }

    async fn wait_until_runtime_steerable(
        &self,
        session_id: SessionId,
        turn_id: TurnId,
        deadline: tokio::time::Instant,
    ) -> Result<(), RuntimeError> {
        let mut phase_changes = {
            let active = self.active_turns.lock().await;
            active
                .get(&(session_id, turn_id))
                .ok_or_else(|| RuntimeError::recoverable(RuntimeErrorCode::ScopeMismatch))?
                .phase
                .subscribe()
        };
        loop {
            {
                let active = self.active_turns.lock().await;
                let control = active
                    .get(&(session_id, turn_id))
                    .ok_or_else(|| RuntimeError::recoverable(RuntimeErrorCode::ScopeMismatch))?;
                let phase = *control.phase.borrow();
                if control.cancel_requested || phase == ActiveTurnPhase::Finishing {
                    return Err(RuntimeError::recoverable(RuntimeErrorCode::ScopeMismatch));
                }
                if phase == ActiveTurnPhase::Running {
                    return Ok(());
                }
            }
            tokio::time::timeout_at(deadline, phase_changes.changed())
                .await
                .map_err(|_| RuntimeError::retryable(RuntimeErrorCode::ProviderUnavailable))?
                .map_err(|_| RuntimeError::recoverable(RuntimeErrorCode::ScopeMismatch))?;
        }
    }

    async fn finish_steer_reliably(
        &self,
        session_id: SessionId,
        user_turn_id: TurnId,
        agent_turn_id: TurnId,
        state: SessionTurnState,
        error: Option<SafeSessionError>,
    ) -> Result<SessionCommit, ConversationError> {
        let event_id = SessionEventId::new();
        let committed_at = unix_time_ms();
        loop {
            match self
                .sessions
                .finish_steer(UserSteerCompletion {
                    event_id,
                    session_id,
                    user_turn_id,
                    agent_turn_id,
                    state,
                    error: error.clone(),
                    committed_at_unix_ms: committed_at,
                })
                .await
            {
                Ok(commit) => return Ok(commit),
                Err(
                    SessionJournalError::StorageUnavailable
                    | SessionJournalError::RevisionConflict { .. },
                ) => tokio::time::sleep(COMMIT_RETRY_DELAY).await,
                Err(error) => return Err(error.into()),
            }
        }
    }

    pub async fn cancel_turn(
        &self,
        project_id: Option<ProjectId>,
        session_id: SessionId,
        turn_id: TurnId,
    ) -> Result<CancellationAcknowledgement, ConversationError> {
        let snapshot = self.sessions.restore(session_id).await?;
        self.require_scope(project_id)?;
        if snapshot.session.project_id != project_id {
            return Err(ConversationError::ScopeMismatch);
        }
        let turn = snapshot
            .turns
            .iter()
            .find(|candidate| {
                candidate.turn_id == turn_id && candidate.role == SessionTurnRole::Agent
            })
            .ok_or(ConversationError::ScopeMismatch)?;
        if turn.state.is_terminal() {
            return Ok(CancellationAcknowledgement {
                session_id: session_id.0.to_string(),
                turn_id: turn_id.0.to_string(),
                disposition: CancelDisposition::AlreadyTerminal(terminal_kind(turn.state)),
            });
        }
        if snapshot.session.active_turn_id != Some(turn_id) {
            return Err(ConversationError::ScopeMismatch);
        }
        let cancel_gate = {
            let active = self.active_turns.lock().await;
            active
                .get(&(session_id, turn_id))
                .map(|control| Arc::clone(&control.cancel_gate))
                .ok_or(ConversationError::SessionUnavailable)?
        };
        let _cancel_guard = cancel_gate.lock().await;

        // A concurrent cancellation may have completed while this caller waited.
        // Re-read canonical state before deciding whether another provider request is needed.
        let snapshot = self.sessions.restore(session_id).await?;
        let turn = snapshot
            .turns
            .iter()
            .find(|candidate| {
                candidate.turn_id == turn_id && candidate.role == SessionTurnRole::Agent
            })
            .ok_or(ConversationError::ScopeMismatch)?;
        if turn.state.is_terminal() {
            return Ok(CancellationAcknowledgement {
                session_id: session_id.0.to_string(),
                turn_id: turn_id.0.to_string(),
                disposition: CancelDisposition::AlreadyTerminal(terminal_kind(turn.state)),
            });
        }
        if snapshot.session.active_turn_id != Some(turn_id) {
            return Err(ConversationError::ScopeMismatch);
        }

        let (cancel_signal, phase) = {
            let mut active = self.active_turns.lock().await;
            let control = active
                .get_mut(&(session_id, turn_id))
                .ok_or(ConversationError::SessionUnavailable)?;
            if control.cancel_requested {
                return Ok(cancellation_ack(
                    session_id,
                    turn_id,
                    CancelDisposition::AlreadyRequested,
                ));
            }
            control.cancel_requested = true;
            (control.cancel_signal.clone(), *control.phase.borrow())
        };
        if phase == ActiveTurnPhase::Preparing {
            let _ = cancel_signal.send(true);
            return Ok(cancellation_ack(
                session_id,
                turn_id,
                CancelDisposition::Requested,
            ));
        }
        let acknowledgement = match tokio::time::timeout(
            PROVIDER_CONTROL_TIMEOUT,
            self.runtime.cancel_turn(CancelRuntimeTurnRequest {
                session_id: session_id.0.to_string(),
                turn_id: turn_id.0.to_string(),
            }),
        )
        .await
        {
            Ok(Ok(acknowledgement)) => acknowledgement,
            Ok(Err(error)) => {
                self.reset_cancel_request(session_id, turn_id).await;
                return Err(ConversationError::Runtime(error));
            }
            Err(_) => {
                self.reset_cancel_request(session_id, turn_id).await;
                return Err(ConversationError::Runtime(RuntimeError::retryable(
                    RuntimeErrorCode::ProviderUnavailable,
                )));
            }
        };
        if acknowledgement.disposition == CancelDisposition::NotFound {
            if phase == ActiveTurnPhase::Starting {
                let _ = cancel_signal.send(true);
                return Ok(cancellation_ack(
                    session_id,
                    turn_id,
                    CancelDisposition::Requested,
                ));
            }
            self.reset_cancel_request(session_id, turn_id).await;
            return Err(ConversationError::Runtime(RuntimeError::recoverable(
                RuntimeErrorCode::ScopeMismatch,
            )));
        }
        if matches!(
            acknowledgement.disposition,
            CancelDisposition::Requested
                | CancelDisposition::AlreadyRequested
                | CancelDisposition::AlreadyTerminal(RuntimeTerminalKind::Cancelled)
        ) {
            let _ = cancel_signal.send(true);
        }
        Ok(acknowledgement)
    }

    async fn reset_cancel_request(&self, session_id: SessionId, turn_id: TurnId) {
        if let Some(control) = self
            .active_turns
            .lock()
            .await
            .get_mut(&(session_id, turn_id))
        {
            control.cancel_requested = false;
        }
    }

    async fn cancel_runtime_before_terminal(
        &self,
        session_id: SessionId,
        turn_id: TurnId,
    ) -> Result<(), RuntimeError> {
        let acknowledgement = tokio::time::timeout(
            PROVIDER_CONTROL_TIMEOUT,
            self.runtime.cancel_turn(CancelRuntimeTurnRequest {
                session_id: session_id.0.to_string(),
                turn_id: turn_id.0.to_string(),
            }),
        )
        .await
        .map_err(|_| RuntimeError::retryable(RuntimeErrorCode::ProviderUnavailable))??;
        match acknowledgement.disposition {
            CancelDisposition::Requested
            | CancelDisposition::AlreadyRequested
            | CancelDisposition::AlreadyTerminal(RuntimeTerminalKind::Cancelled)
            | CancelDisposition::AlreadyTerminal(RuntimeTerminalKind::TimedOut) => Ok(()),
            CancelDisposition::AlreadyTerminal(
                RuntimeTerminalKind::Completed | RuntimeTerminalKind::Failed,
            ) => Err(RuntimeError::recoverable(RuntimeErrorCode::ProviderFailure)),
            CancelDisposition::NotFound => {
                Err(RuntimeError::recoverable(RuntimeErrorCode::ScopeMismatch))
            }
        }
    }

    pub async fn subscribe(
        &self,
        session_id: SessionId,
    ) -> Result<SessionSubscription, ConversationError> {
        self.require_session_scope(session_id).await?;
        self.sessions.subscribe(session_id).await.map_err(|error| {
            tracing::error!(
                dennett.session.id = %session_id.0,
                code = ?error,
                "failed to open authoritative session watch"
            );
            error.into()
        })
    }

    pub async fn restore(
        &self,
        session_id: SessionId,
    ) -> Result<ProjectSessionSnapshot, ConversationError> {
        let snapshot = self.sessions.restore(session_id).await?;
        if !self.scope_is_local(snapshot.session.project_id) {
            return Err(ConversationError::ScopeMismatch);
        }
        Ok(snapshot)
    }

    async fn require_session_scope(&self, session_id: SessionId) -> Result<(), ConversationError> {
        let snapshot = self.sessions.restore(session_id).await.map_err(|error| {
            tracing::error!(
                dennett.session.id = %session_id.0,
                code = ?error,
                "failed to restore authoritative session"
            );
            error
        })?;
        if !self.scope_is_local(snapshot.session.project_id) {
            return Err(ConversationError::ScopeMismatch);
        }
        Ok(())
    }

    async fn run_turn(
        &self,
        session_id: SessionId,
        turn_id: TurnId,
        input: RuntimeTurnInput,
        mut cancel_rx: watch::Receiver<bool>,
        trace_span: tracing::Span,
    ) {
        let RuntimeTurnInput {
            prompt,
            context_handles,
            runtime_controls,
            workspace_path,
        } = input;
        let deadline_at = tokio::time::Instant::now() + self.turn_timeout;
        if *cancel_rx.borrow() {
            self.finish_cancelled(session_id, turn_id, String::new(), &trace_span)
                .await;
            return;
        }
        let descriptor = tokio::select! {
            biased;
            () = wait_for_cancel_signal(&mut cancel_rx) => {
                self.finish_cancelled(session_id, turn_id, String::new(), &trace_span).await;
                return;
            }
            descriptor = tokio::time::timeout_at(deadline_at, self.runtime.describe()) => match descriptor {
                Ok(descriptor) => descriptor,
                Err(_) => {
                    self.finish_timeout(session_id, turn_id, String::new(), &trace_span).await;
                    return;
                }
            },
        };
        let provider_id = descriptor
            .as_ref()
            .map_or("runtime-unavailable", |value| value.adapter_id.as_str());
        record_span_text(&trace_span, "dennett.provider.id", provider_id);
        if let Err(error) = descriptor {
            self.finish_runtime_error(session_id, turn_id, error, &trace_span)
                .await;
            return;
        }
        if *cancel_rx.borrow() {
            self.finish_cancelled(session_id, turn_id, String::new(), &trace_span)
                .await;
            return;
        }
        let session_key = session_id.0.to_string();
        let continuation = tokio::select! {
            biased;
            () = wait_for_cancel_signal(&mut cancel_rx) => {
                self.finish_cancelled(session_id, turn_id, String::new(), &trace_span).await;
                return;
            }
            continuation = tokio::time::timeout_at(deadline_at, self.continuations.load(&session_key)) => match continuation {
                Ok(continuation) => continuation,
                Err(_) => {
                self.finish_timeout(session_id, turn_id, String::new(), &trace_span)
                    .await;
                return;
                }
            },
        };
        let continuation = match continuation {
            Ok(continuation) => continuation,
            Err(error) => {
                self.finish_runtime_error(
                    session_id,
                    turn_id,
                    runtime_continuation_error(error),
                    &trace_span,
                )
                .await;
                return;
            }
        };
        let deadline = match RuntimeDeadline::after(self.turn_timeout) {
            Ok(deadline) => deadline,
            Err(error) => {
                self.finish_runtime_error(session_id, turn_id, error, &trace_span)
                    .await;
                return;
            }
        };
        let request = RuntimeTurnRequest {
            session_id: session_id.0.to_string(),
            turn_id: turn_id.0.to_string(),
            prompt,
            workspace_path,
            context_handles,
            runtime_controls,
            continuation,
            deadline,
        };
        let start_cancel_requested = {
            let mut active = self.active_turns.lock().await;
            active.get_mut(&(session_id, turn_id)).map(|control| {
                control.phase.send_replace(ActiveTurnPhase::Starting);
                control.cancel_requested
            })
        };
        let Some(start_cancel_requested) = start_cancel_requested else {
            self.finish_runtime_error(
                session_id,
                turn_id,
                RuntimeError::new(RuntimeErrorCode::ProtocolViolation),
                &trace_span,
            )
            .await;
            return;
        };
        if start_cancel_requested {
            self.finish_cancelled(session_id, turn_id, String::new(), &trace_span)
                .await;
            return;
        }
        let mut turn = tokio::select! {
            biased;
            () = wait_for_cancel_signal(&mut cancel_rx) => {
                self.finish_cancelled(session_id, turn_id, String::new(), &trace_span).await;
                return;
            }
            turn = tokio::time::timeout_at(deadline_at, self.runtime.start_turn(request)) => match turn {
                Ok(Ok(turn)) => turn,
                Ok(Err(error)) => {
                    self.finish_runtime_error(session_id, turn_id, error, &trace_span)
                        .await;
                    return;
                }
                Err(_) => {
                    match self.cancel_runtime_before_terminal(session_id, turn_id).await {
                        Ok(()) => {
                            self.finish_timeout(session_id, turn_id, String::new(), &trace_span)
                                .await;
                        }
                        Err(error) => {
                            self.finish_runtime_error(session_id, turn_id, error, &trace_span)
                                .await;
                        }
                    }
                    return;
                }
            },
        };
        let mut output = String::new();
        loop {
            let next = match tokio::time::timeout_at(deadline_at, turn.next_event()).await {
                Ok(next) => next,
                Err(_) => {
                    match self
                        .cancel_runtime_before_terminal(session_id, turn_id)
                        .await
                    {
                        Ok(()) => {
                            self.finish_timeout(session_id, turn_id, output, &trace_span)
                                .await;
                        }
                        Err(error) => {
                            self.finish_runtime_error(session_id, turn_id, error, &trace_span)
                                .await;
                        }
                    }
                    return;
                }
            };
            let Some(event) = next else {
                self.finish_runtime_error(
                    session_id,
                    turn_id,
                    RuntimeError::new(RuntimeErrorCode::ProtocolViolation),
                    &trace_span,
                )
                .await;
                return;
            };
            let event = match event {
                Ok(event) => event,
                Err(error) => {
                    self.finish_runtime_error(session_id, turn_id, error, &trace_span)
                        .await;
                    return;
                }
            };
            let RuntimeEvent {
                sequence: event_sequence,
                kind,
                native_extensions,
                ..
            } = event;
            match kind {
                RuntimeEventKind::Started { continuation } => {
                    let transitioned = {
                        let mut active = self.active_turns.lock().await;
                        active.get_mut(&(session_id, turn_id)).map(|control| {
                            control.phase.send_replace(ActiveTurnPhase::Running);
                        })
                    };
                    if transitioned.is_none() {
                        self.finish_runtime_error(
                            session_id,
                            turn_id,
                            RuntimeError::new(RuntimeErrorCode::ProtocolViolation),
                            &trace_span,
                        )
                        .await;
                        return;
                    }
                    if let Some(continuation) = continuation
                        && let Err(error) =
                            self.continuations.save(&session_key, &continuation).await
                    {
                        self.finish_runtime_error(
                            session_id,
                            turn_id,
                            runtime_continuation_error(error),
                            &trace_span,
                        )
                        .await;
                        return;
                    }
                }
                RuntimeEventKind::TextDelta { text } => {
                    if output.len().saturating_add(text.len()) > MAX_TURN_OUTPUT_BYTES {
                        let error = self
                            .cancel_runtime_before_terminal(session_id, turn_id)
                            .await
                            .err()
                            .unwrap_or_else(|| {
                                RuntimeError::recoverable(RuntimeErrorCode::ProviderFailure)
                            });
                        self.finish_runtime_error(session_id, turn_id, error, &trace_span)
                            .await;
                        return;
                    }
                    output.push_str(&text);
                    if let Err(error) = self
                        .append_agent_text_reliably(session_id, turn_id, text)
                        .await
                    {
                        self.finish_runtime_error(
                            session_id,
                            turn_id,
                            RuntimeError::new(error),
                            &trace_span,
                        )
                        .await;
                        return;
                    }
                }
                RuntimeEventKind::Terminal(terminal) => {
                    if let Some(continuation) = terminal.continuation
                        && let Err(error) =
                            self.continuations.save(&session_key, &continuation).await
                    {
                        self.finish_runtime_error(
                            session_id,
                            turn_id,
                            runtime_continuation_error(error),
                            &trace_span,
                        )
                        .await;
                        return;
                    }
                    self.finish_terminal(
                        session_id,
                        turn_id,
                        output,
                        terminal.outcome,
                        &trace_span,
                    )
                    .await;
                    return;
                }
                RuntimeEventKind::Progress {
                    activity_id,
                    phase,
                    message,
                    status,
                } => {
                    let activity_id =
                        activity_id.unwrap_or_else(|| format!("runtime-progress-{event_sequence}"));
                    let native_extensions = match native_extensions
                        .into_iter()
                        .map(session_native_extension)
                        .collect::<Result<Vec<_>, _>>()
                    {
                        Ok(native_extensions) => native_extensions,
                        Err(error) => {
                            self.finish_runtime_error(
                                session_id,
                                turn_id,
                                RuntimeError::new(error),
                                &trace_span,
                            )
                            .await;
                            return;
                        }
                    };
                    if let Err(error) = self
                        .upsert_agent_activity_reliably(AgentActivityUpdate {
                            event_id: SessionEventId::new(),
                            session_id,
                            turn_id,
                            activity_id,
                            phase: phase.clone(),
                            message,
                            status: session_activity_status(status),
                            native_extensions,
                            committed_at_unix_ms: unix_time_ms(),
                        })
                        .await
                    {
                        self.finish_runtime_error(
                            session_id,
                            turn_id,
                            RuntimeError::new(error),
                            &trace_span,
                        )
                        .await;
                        return;
                    }
                    tracing::info!(runtime_phase = phase, "runtime progress");
                }
                RuntimeEventKind::Usage(_) | RuntimeEventKind::Warning { .. } => {}
            }
        }
    }

    async fn finish_terminal(
        &self,
        session_id: SessionId,
        turn_id: TurnId,
        output: String,
        terminal: RuntimeTerminalOutcome,
        trace_span: &tracing::Span,
    ) {
        let (state, outcome) = match terminal {
            RuntimeTerminalOutcome::Completed => (
                SessionTurnState::Completed,
                Some(SessionTurnOutcome::Result(SessionResult {
                    summary: output,
                    partial: false,
                    artifact_handles: Vec::new(),
                    evidence_handles: Vec::new(),
                })),
            ),
            RuntimeTerminalOutcome::Cancelled { partial } => (
                SessionTurnState::Cancelled,
                Some(SessionTurnOutcome::Result(SessionResult {
                    summary: output,
                    partial,
                    artifact_handles: Vec::new(),
                    evidence_handles: Vec::new(),
                })),
            ),
            RuntimeTerminalOutcome::TimedOut { partial } => (
                SessionTurnState::TimedOut,
                Some(SessionTurnOutcome::Result(SessionResult {
                    summary: output,
                    partial,
                    artifact_handles: Vec::new(),
                    evidence_handles: Vec::new(),
                })),
            ),
            RuntimeTerminalOutcome::Failed { code, .. } => (
                SessionTurnState::Failed,
                Some(SessionTurnOutcome::Error(SafeSessionError {
                    message_key: format!("runtime.{code}"),
                    code,
                    details_handle: None,
                })),
            ),
        };
        self.commit_terminal(session_id, turn_id, state, outcome, trace_span)
            .await;
    }

    async fn finish_runtime_error(
        &self,
        session_id: SessionId,
        turn_id: TurnId,
        error: RuntimeError,
        trace_span: &tracing::Span,
    ) {
        self.commit_terminal(
            session_id,
            turn_id,
            SessionTurnState::Failed,
            Some(SessionTurnOutcome::Error(SafeSessionError {
                code: error.code.as_str().to_owned(),
                message_key: format!("runtime.{}", error.code.as_str()),
                details_handle: None,
            })),
            trace_span,
        )
        .await;
    }

    async fn finish_cancelled(
        &self,
        session_id: SessionId,
        turn_id: TurnId,
        output: String,
        trace_span: &tracing::Span,
    ) {
        self.finish_terminal(
            session_id,
            turn_id,
            output.clone(),
            RuntimeTerminalOutcome::Cancelled {
                partial: !output.is_empty(),
            },
            trace_span,
        )
        .await;
    }

    async fn finish_timeout(
        &self,
        session_id: SessionId,
        turn_id: TurnId,
        output: String,
        trace_span: &tracing::Span,
    ) {
        self.commit_terminal(
            session_id,
            turn_id,
            SessionTurnState::TimedOut,
            Some(SessionTurnOutcome::Result(SessionResult {
                partial: !output.is_empty(),
                summary: output,
                artifact_handles: Vec::new(),
                evidence_handles: Vec::new(),
            })),
            trace_span,
        )
        .await;
    }

    async fn commit_terminal(
        &self,
        session_id: SessionId,
        turn_id: TurnId,
        state: SessionTurnState,
        outcome: Option<SessionTurnOutcome>,
        trace_span: &tracing::Span,
    ) {
        if let Some(control) = self
            .active_turns
            .lock()
            .await
            .get_mut(&(session_id, turn_id))
        {
            control.phase.send_replace(ActiveTurnPhase::Finishing);
        }
        let event_id = SessionEventId::new();
        let committed_at = unix_time_ms();
        loop {
            match self
                .sessions
                .finish_turn(
                    event_id,
                    session_id,
                    turn_id,
                    state,
                    outcome.clone(),
                    committed_at,
                )
                .await
            {
                Ok(commit) => {
                    let memory_event_id = event_id.0.to_string();
                    record_span_text(
                        trace_span,
                        "dennett.turn.terminal_state",
                        terminal_state_label(state),
                    );
                    record_span_text(
                        trace_span,
                        "dennett.memory.event.id",
                        memory_event_id.as_str(),
                    );
                    tracing::info!(dennett.memory.event.id = %event_id.0, "runtime turn committed");
                    self.system
                        .apply(vec![SystemMutation::UpsertSession(session_summary(
                            &commit.snapshot,
                        ))])
                        .await;
                    break;
                }
                Err(
                    SessionJournalError::StorageUnavailable
                    | SessionJournalError::RevisionConflict { .. },
                ) => {
                    tracing::warn!("terminal commit unavailable; retrying");
                    tokio::time::sleep(COMMIT_RETRY_DELAY).await;
                }
                Err(error) => {
                    tracing::error!(code = ?error, "failed to commit runtime terminal");
                    break;
                }
            }
        }
        self.active_turns
            .lock()
            .await
            .remove(&(session_id, turn_id));
    }

    async fn append_agent_text_reliably(
        &self,
        session_id: SessionId,
        turn_id: TurnId,
        text: String,
    ) -> Result<(), RuntimeErrorCode> {
        let event_id = SessionEventId::new();
        let committed_at = unix_time_ms();
        loop {
            match self
                .sessions
                .append_agent_text(event_id, session_id, turn_id, text.clone(), committed_at)
                .await
            {
                Ok(_) => return Ok(()),
                Err(
                    SessionJournalError::StorageUnavailable
                    | SessionJournalError::RevisionConflict { .. },
                ) => {
                    tokio::time::sleep(COMMIT_RETRY_DELAY).await;
                }
                Err(error) => {
                    tracing::error!(code = ?error, "failed to commit runtime text");
                    return Err(RuntimeErrorCode::ProtocolViolation);
                }
            }
        }
    }

    async fn upsert_agent_activity_reliably(
        &self,
        update: AgentActivityUpdate,
    ) -> Result<(), RuntimeErrorCode> {
        loop {
            match self
                .sessions
                .upsert_agent_activity(AgentActivityUpdate {
                    event_id: update.event_id,
                    session_id: update.session_id,
                    turn_id: update.turn_id,
                    activity_id: update.activity_id.clone(),
                    phase: update.phase.clone(),
                    message: update.message.clone(),
                    status: update.status,
                    native_extensions: update.native_extensions.clone(),
                    committed_at_unix_ms: update.committed_at_unix_ms,
                })
                .await
            {
                Ok(_) => return Ok(()),
                Err(
                    SessionJournalError::StorageUnavailable
                    | SessionJournalError::RevisionConflict { .. },
                ) => tokio::time::sleep(COMMIT_RETRY_DELAY).await,
                Err(error) => {
                    tracing::error!(code = ?error, "failed to commit runtime activity");
                    return Err(RuntimeErrorCode::ProtocolViolation);
                }
            }
        }
    }

    fn scope_is_local(&self, project_id: Option<ProjectId>) -> bool {
        project_id.is_none() || project_id == Some(self.project.project_id)
    }

    fn require_scope(&self, project_id: Option<ProjectId>) -> Result<(), ConversationError> {
        if self.scope_is_local(project_id) {
            Ok(())
        } else {
            Err(ConversationError::ScopeMismatch)
        }
    }

    fn workspace_for(&self, project_id: Option<ProjectId>) -> &str {
        if project_id.is_some() {
            &self.project.workspace_path
        } else {
            &self.project.standalone_workspace_path
        }
    }
}

fn record_span_text(span: &tracing::Span, field_name: &str, value: &str) {
    if let Some(field) = span
        .metadata()
        .and_then(|metadata| metadata.fields().field(field_name))
    {
        span.record(&field, value);
    }
}

fn terminal_state_label(state: SessionTurnState) -> &'static str {
    match state {
        SessionTurnState::Completed => "completed",
        SessionTurnState::Cancelled => "cancelled",
        SessionTurnState::TimedOut => "timed_out",
        SessionTurnState::Failed => "failed",
        SessionTurnState::Accepted | SessionTurnState::Streaming => "non_terminal",
    }
}

fn session_activity_status(status: RuntimeActivityStatus) -> SessionActivityStatus {
    match status {
        RuntimeActivityStatus::Started => SessionActivityStatus::Started,
        RuntimeActivityStatus::Updated => SessionActivityStatus::Updated,
        RuntimeActivityStatus::Completed => SessionActivityStatus::Completed,
        RuntimeActivityStatus::Failed => SessionActivityStatus::Failed,
    }
}

async fn wait_for_cancel_signal(cancel_rx: &mut watch::Receiver<bool>) {
    if *cancel_rx.borrow() {
        return;
    }
    loop {
        if cancel_rx.changed().await.is_err() || *cancel_rx.borrow() {
            return;
        }
    }
}

fn session_native_extension(
    extension: NativeExtension,
) -> Result<SessionNativeExtension, RuntimeErrorCode> {
    Ok(SessionNativeExtension {
        namespace: extension.namespace,
        schema_version: extension.schema_version,
        json_value: String::from_utf8(extension.payload)
            .map_err(|_| RuntimeErrorCode::ProtocolViolation)?,
    })
}

fn terminal_kind(state: SessionTurnState) -> RuntimeTerminalKind {
    match state {
        SessionTurnState::Completed => RuntimeTerminalKind::Completed,
        SessionTurnState::Cancelled => RuntimeTerminalKind::Cancelled,
        SessionTurnState::TimedOut => RuntimeTerminalKind::TimedOut,
        SessionTurnState::Failed => RuntimeTerminalKind::Failed,
        SessionTurnState::Accepted | SessionTurnState::Streaming => {
            unreachable!("terminal_kind requires terminal state")
        }
    }
}

fn cancellation_ack(
    session_id: SessionId,
    turn_id: TurnId,
    disposition: CancelDisposition,
) -> CancellationAcknowledgement {
    CancellationAcknowledgement {
        session_id: session_id.0.to_string(),
        turn_id: turn_id.0.to_string(),
        disposition,
    }
}

fn runtime_continuation_error(error: RuntimeContinuationError) -> RuntimeError {
    match error {
        RuntimeContinuationError::StorageUnavailable => {
            RuntimeError::retryable(RuntimeErrorCode::ProviderUnavailable)
        }
        RuntimeContinuationError::InvalidRequest | RuntimeContinuationError::IntegrityFailure => {
            RuntimeError::new(RuntimeErrorCode::ProtocolViolation)
        }
    }
}

fn session_summary(snapshot: &ProjectSessionSnapshot) -> SessionSummary {
    SessionSummary {
        session_id: snapshot.session.session_id.0.to_string(),
        project_id: snapshot
            .session
            .project_id
            .map_or_else(String::new, |id| id.0.to_string()),
        title: snapshot.session.title.clone(),
        state: snapshot.session.state,
        revision: snapshot.session.revision,
        active_turn_id: snapshot.session.active_turn_id.map(|id| id.0.to_string()),
        last_activity_unix_ms: snapshot.session.last_activity_unix_ms,
    }
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[derive(Debug, thiserror::Error)]
pub enum ConversationError {
    #[error("conversation request is invalid")]
    InvalidRequest,
    #[error("another conversation turn is already active")]
    TurnAlreadyActive,
    #[error("conversation scope does not match the local project")]
    ScopeMismatch,
    #[error("conversation session is unavailable")]
    SessionUnavailable,
    #[error(transparent)]
    Session(#[from] SessionJournalError),
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
}
