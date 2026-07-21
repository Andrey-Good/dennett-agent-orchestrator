#[cfg(windows)]
use crate::DEFAULT_MAX_MESSAGE_BYTES;
use crate::auth::AuthError;
use crate::protocol::dennett::common::v1::{
    CommandAccepted, CommandMetadata, ErrorEnvelope, StableRef,
};
use crate::protocol::dennett::control::v1::bootstrap_response;
use crate::protocol::dennett::control::v1::get_health_response;
use crate::protocol::dennett::control::v1::handshake_response;
use crate::protocol::dennett::control::v1::session_service_server::SessionService;
#[cfg(windows)]
use crate::protocol::dennett::control::v1::session_service_server::SessionServiceServer;
use crate::protocol::dennett::control::v1::system_mutation;
use crate::protocol::dennett::control::v1::system_service_server::SystemService;
#[cfg(windows)]
use crate::protocol::dennett::control::v1::system_service_server::SystemServiceServer;
use crate::protocol::dennett::control::v1::system_watch_frame;
use crate::protocol::dennett::control::v1::{
    BootstrapRequest, BootstrapResponse, BootstrapSnapshot, GetHealthRequest, GetHealthResponse,
    GetHealthResult, HandshakeRequest, HandshakeResponse, HealthState, ProjectState,
    ProjectSummary as WireProjectSummary, RuntimeControlChoice as WireRuntimeControlChoice,
    RuntimeControlCondition as WireRuntimeControlCondition,
    RuntimeControlDescriptor as WireRuntimeControlDescriptor, RuntimeSummary as WireRuntimeSummary,
    SessionState, SessionSummary as WireSessionSummary, SystemDelta as WireSystemDelta,
    SystemHealthUpdate, SystemMutation as WireSystemMutation, SystemSelectionUpdate,
    SystemSnapshot as WireSystemSnapshot, SystemWatchFrame, WatchRequest, WatchResponse,
};
use crate::protocol::dennett::control::v1::{
    CancelTurnRequest, CancelTurnResponse, ComposerDraft, ComposerDraftDiscarded,
    ComposerDraftMissing, ComposerDraftWriteReceipt, ComposerDraftWriteState,
    CreateSessionAccepted, CreateSessionRequest, CreateSessionResponse,
    DiscardComposerDraftRequest, DiscardComposerDraftResponse, GetComposerDraftRequest,
    GetComposerDraftResponse, NativeExtensionPayload, ResultEnvelope as WireResultEnvelope,
    SaveComposerDraftRequest, SaveComposerDraftResponse, SendTurnAccepted, SendTurnRequest,
    SendTurnResponse, SessionDelta, SessionMetadataUpdate, SessionMutation, SessionSnapshot,
    SessionWatchFrame, TurnActivitySnapshot, TurnActivityStatus as WireTurnActivityStatus,
    TurnActivityUpsert, TurnDeliveryMode as WireTurnDeliveryMode, TurnRole, TurnSnapshot,
    TurnState, TurnTerminal, TurnTextAppend, WatchSessionRequest, WatchSessionResponse,
};
use crate::protocol::dennett::control::v1::{
    cancel_turn_response, create_session_response, discard_composer_draft_response,
    get_composer_draft_response, save_composer_draft_response, send_turn_response,
    session_mutation, session_watch_frame, turn_snapshot, turn_terminal,
};
use crate::protocol::dennett::sync::v1::{
    ResyncReason as WireResyncReason, ResyncRequired, WatchCursor as WireWatchCursor,
    WatchHeartbeat,
};
use crate::{LocalEndpoint, PeerIdentity, SessionRegistry, TransportError};
use dennett_agent_core::{RuntimeControlSelection, RuntimeDescriptor, RuntimeKind};
use dennett_contracts::{CommandId, ProjectId, SessionId, TurnId};
use dennett_head::conversation::{
    ConversationApplication, ConversationError, ConversationTurnRequest, TraceContext,
    TurnDeliveryMode,
};
use dennett_head::draft::{ComposerDraftApplication, DraftApplicationError, DraftSaveOutcome};
use dennett_head::system::{
    ProjectSummary, SessionSummary, SystemDelta, SystemHealth, SystemMutation, SystemSnapshot,
    SystemStateError, SystemStatePort, SystemWatchFrame as DomainWatchFrame,
};
use dennett_memory_core::session::{
    CommittedSessionEvent, ProjectSessionSnapshot, ProjectSessionState, SafeSessionError,
    SessionActivityStatus, SessionEventBody, SessionNativeExtension, SessionResult, SessionTurn,
    SessionTurnActivity, SessionTurnOutcome, SessionTurnRole, SessionTurnState,
};
use dennett_sync_core::admission::{
    CommandAdmissionError, CommandAdmissionPort, CommandAdmissionRequest,
};
use dennett_sync_core::draft::DraftRecord;
use dennett_sync_core::watch::{ResyncReason, WatchCursor, WatchFrame};
use futures_core::Stream;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tonic::{Request, Response, Status};

#[derive(Clone)]
pub struct SystemServiceAdapter<S: SystemStatePort> {
    state: Arc<S>,
    sessions: SessionRegistry,
}

impl<S: SystemStatePort> SystemServiceAdapter<S> {
    #[must_use]
    pub fn new(state: Arc<S>, sessions: SessionRegistry) -> Self {
        Self { state, sessions }
    }
}

#[tonic::async_trait]
impl<S: SystemStatePort + 'static> SystemService for SystemServiceAdapter<S> {
    async fn handshake(
        &self,
        request: Request<HandshakeRequest>,
    ) -> Result<Response<HandshakeResponse>, Status> {
        let peer = peer(&request);
        let hello = request.into_inner().hello;
        let outcome = match (peer, hello) {
            (Ok(peer), Some(hello)) => self
                .sessions
                .issue(&peer, hello)
                .map(handshake_response::Outcome::Welcome)
                .unwrap_or_else(|error| handshake_response::Outcome::Error(auth_error(error))),
            (Err(error), _) => handshake_response::Outcome::Error(auth_error(error)),
            (_, None) => {
                handshake_response::Outcome::Error(auth_error(AuthError::ClientIdentityInvalid))
            }
        };
        Ok(Response::new(HandshakeResponse {
            outcome: Some(outcome),
        }))
    }

    async fn bootstrap(
        &self,
        request: Request<BootstrapRequest>,
    ) -> Result<Response<BootstrapResponse>, Status> {
        let peer = peer(&request);
        let request = request.into_inner();
        let outcome = match peer.and_then(|peer| {
            self.sessions.consume_bootstrap(
                &peer,
                &request.client_session_id,
                &request.session_proof,
            )
        }) {
            Ok(authenticated) => match self.state.bootstrap().await {
                Ok(snapshot) if snapshot.authority_epoch == authenticated.authority_epoch => {
                    bootstrap_response::Outcome::Snapshot(snapshot_to_wire(&snapshot))
                }
                Ok(_) => {
                    bootstrap_response::Outcome::Error(auth_error(AuthError::AuthorityEpochChanged))
                }
                Err(error) => bootstrap_response::Outcome::Error(state_error(error)),
            },
            Err(error) => bootstrap_response::Outcome::Error(auth_error(error)),
        };
        Ok(Response::new(BootstrapResponse {
            outcome: Some(outcome),
        }))
    }

    type WatchStream = Pin<Box<dyn Stream<Item = Result<WatchResponse, Status>> + Send>>;

    async fn watch(
        &self,
        request: Request<WatchRequest>,
    ) -> Result<Response<Self::WatchStream>, Status> {
        let request_peer = peer(&request);
        let request = request.into_inner();
        let client_session_id = request.client_session_id;
        let authorization = request_peer.and_then(|peer| {
            self.sessions
                .authorize_active(&peer, &client_session_id)
                .map(|_| (peer, client_session_id))
        });
        let state = self.state.clone();
        let sessions = self.sessions.clone();
        let lease_interval = lease_recheck_interval(self.sessions.policy().ui_session_ttl);
        let stream = async_stream::stream! {
            let (peer, client_session_id) = match authorization {
                Ok(authorization) => authorization,
                Err(error) => {
                    yield Ok(error_watch_response(auth_error(error)));
                    return;
                }
            };
            let mut subscription = match state.subscribe().await {
                Ok(subscription) => subscription,
                Err(error) => {
                    yield Ok(error_watch_response(state_error(error)));
                    return;
                }
            };
            let Some(initial) = subscription.take_initial() else {
                yield Ok(error_watch_response(internal_error(
                    "system_watch_missing_snapshot",
                    "system.watch_missing_snapshot",
                )));
                return;
            };
            yield Ok(watch_to_wire(initial));
            let mut lease = tokio::time::interval(lease_interval);
            lease.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            lease.tick().await;
            loop {
                tokio::select! {
                    _ = lease.tick() => {
                        if let Err(error) = sessions.authorize_active(&peer, &client_session_id) {
                            yield Ok(error_watch_response(auth_error(error)));
                            return;
                        }
                        if let Some(heartbeat) = subscription.heartbeat() {
                            yield Ok(watch_to_wire(heartbeat));
                        }
                    }
                    received = subscription.recv() => {
                        match received {
                            Ok(Some(frame)) => yield Ok(watch_to_wire(frame)),
                            Ok(None) => return,
                            Err(error) => {
                                yield Ok(error_watch_response(state_error(error)));
                                return;
                            }
                        }
                    }
                }
            }
        };
        Ok(Response::new(Box::pin(stream)))
    }

    async fn get_health(
        &self,
        _request: Request<GetHealthRequest>,
    ) -> Result<Response<GetHealthResponse>, Status> {
        let outcome = match self.state.bootstrap().await {
            Ok(snapshot) => get_health_response::Outcome::Health(GetHealthResult {
                state: health_to_wire(snapshot.health),
                node_version: self.sessions.policy().node_version.clone(),
                observed_at: Some(timestamp(snapshot.observed_at_unix_ms)),
                status_code: "node_ready".to_owned(),
            }),
            Err(error) => get_health_response::Outcome::Error(state_error(error)),
        };
        Ok(Response::new(GetHealthResponse {
            outcome: Some(outcome),
        }))
    }
}

#[derive(Clone)]
pub struct SessionServiceAdapter {
    application: Arc<ConversationApplication>,
    drafts: ComposerDraftApplication,
    sessions: SessionRegistry,
    admissions: Arc<dyn CommandAdmissionPort>,
}

impl SessionServiceAdapter {
    #[must_use]
    pub fn new(
        application: Arc<ConversationApplication>,
        drafts: ComposerDraftApplication,
        sessions: SessionRegistry,
        admissions: Arc<dyn CommandAdmissionPort>,
    ) -> Self {
        Self {
            application,
            drafts,
            sessions,
            admissions,
        }
    }

    fn authenticate<T>(
        &self,
        request: &Request<T>,
        client_session_id: &str,
    ) -> Result<crate::auth::AuthenticatedSession, AuthError> {
        self.sessions
            .authorize_active(&peer(request)?, client_session_id)
    }

    async fn accept(
        &self,
        command: &CommandMetadata,
        command_id: CommandId,
        operation_kind: &str,
        intent_hash: [u8; 32],
    ) -> Result<CommandAccepted, ServiceError> {
        let receipt = self
            .admissions
            .admit(CommandAdmissionRequest {
                command_id,
                idempotency_key: command.idempotency_key.clone(),
                correlation_id: command.correlation_id.clone(),
                operation_kind: operation_kind.to_owned(),
                intent_hash,
                admitted_at_unix_ms: unix_time_ms(),
            })
            .await?;
        Ok(CommandAccepted {
            command_id: receipt.command_id.0.to_string(),
            correlation_id: receipt.correlation_id,
            operation_id: receipt.operation_id.0.to_string(),
            accepted_revision: receipt.accepted_revision,
        })
    }
}

#[tonic::async_trait]
impl SessionService for SessionServiceAdapter {
    async fn create_session(
        &self,
        request: Request<CreateSessionRequest>,
    ) -> Result<Response<CreateSessionResponse>, Status> {
        let correlation = request
            .get_ref()
            .command
            .as_ref()
            .map_or("", |command| command.correlation_id.as_str())
            .to_owned();
        let outcome = match command_from(request.get_ref().command.as_ref()).and_then(|command| {
            match self.authenticate(&request, &command.client_session_id) {
                Ok(authenticated)
                    if command.authority_epoch_seen == authenticated.authority_epoch =>
                {
                    Ok(command)
                }
                Ok(_) => Err(ServiceError::Auth(AuthError::AuthorityEpochChanged)),
                Err(error) => Err(ServiceError::Auth(error)),
            }
        }) {
            Ok(command) => {
                let body = request.into_inner();
                match parse_optional_project_id(&body.project_id).and_then(|project_id| {
                    if body.title.trim().is_empty() {
                        Err(ServiceError::InvalidRequest)
                    } else {
                        Ok((project_id, body.title))
                    }
                }) {
                    Ok((project_id, title)) => match parse_uuid(&command.command_id) {
                        Ok(command_id) => {
                            let command_id = CommandId(command_id);
                            let intent_hash = command_intent_hash(
                                "create_session",
                                command.expected_revision,
                                &[body.project_id.as_str(), title.as_str()],
                            );
                            match self
                                .accept(&command, command_id, "create_session", intent_hash)
                                .await
                            {
                                Ok(accepted) => match self
                                    .application
                                    .create_session(command_id, project_id, title)
                                    .await
                                {
                                    Ok(commit) => create_session_response::Outcome::Accepted(
                                        CreateSessionAccepted {
                                            command: Some(accepted),
                                            session_id: commit
                                                .snapshot
                                                .session
                                                .session_id
                                                .0
                                                .to_string(),
                                        },
                                    ),
                                    Err(error) => create_session_response::Outcome::Error(
                                        conversation_error(error, &correlation),
                                    ),
                                },
                                Err(error) => create_session_response::Outcome::Error(
                                    service_error(error, &correlation),
                                ),
                            }
                        }
                        Err(error) => create_session_response::Outcome::Error(service_error(
                            error,
                            &correlation,
                        )),
                    },
                    Err(error) => {
                        create_session_response::Outcome::Error(service_error(error, &correlation))
                    }
                }
            }
            Err(error) => {
                create_session_response::Outcome::Error(service_error(error, &correlation))
            }
        };
        Ok(Response::new(CreateSessionResponse {
            outcome: Some(outcome),
        }))
    }

    #[tracing::instrument(
        name = "local_ipc_send_turn",
        skip_all,
        fields(
            dennett.component = "dennett-node",
            dennett.protocol.version = 1_u64,
            dennett.project.id = tracing::field::Empty,
            dennett.session.id = tracing::field::Empty,
            dennett.command.id = tracing::field::Empty,
            correlation_id = tracing::field::Empty,
        )
    )]
    async fn send_turn(
        &self,
        request: Request<SendTurnRequest>,
    ) -> Result<Response<SendTurnResponse>, Status> {
        let correlation = request
            .get_ref()
            .command
            .as_ref()
            .map_or("", |command| command.correlation_id.as_str())
            .to_owned();
        let command = match command_from(request.get_ref().command.as_ref()) {
            Ok(command) => command,
            Err(error) => {
                return Ok(Response::new(SendTurnResponse {
                    outcome: Some(send_turn_response::Outcome::Error(service_error(
                        error,
                        &correlation,
                    ))),
                }));
            }
        };
        let authenticated = match self.authenticate(&request, &command.client_session_id) {
            Ok(authenticated) if command.authority_epoch_seen == authenticated.authority_epoch => {
                authenticated
            }
            Ok(_) => {
                return Ok(Response::new(SendTurnResponse {
                    outcome: Some(send_turn_response::Outcome::Error(auth_error(
                        AuthError::AuthorityEpochChanged,
                    ))),
                }));
            }
            Err(error) => {
                return Ok(Response::new(SendTurnResponse {
                    outcome: Some(send_turn_response::Outcome::Error(auth_error(error))),
                }));
            }
        };
        let body = request.into_inner();
        let span = tracing::Span::current();
        span.record("correlation_id", command.correlation_id.as_str());
        span.record("dennett.command.id", command.command_id.as_str());
        span.record("dennett.project.id", body.project_id.as_str());
        span.record("dennett.session.id", body.session_id.as_str());
        let parsed = parse_optional_project_id(&body.project_id).and_then(|project_id| {
            let expected_active_turn_id = if body.expected_active_turn_id.trim().is_empty() {
                None
            } else {
                Some(TurnId(parse_uuid(&body.expected_active_turn_id)?))
            };
            Ok((
                project_id,
                SessionId(parse_uuid(&body.session_id)?),
                CommandId(parse_uuid(&command.command_id)?),
                expected_active_turn_id,
            ))
        });
        let outcome = match parsed {
            Ok((project_id, session_id, command_id, expected_active_turn_id)) => {
                let intent_hash = match send_turn_intent_hash(&body, command.expected_revision) {
                    Ok(intent_hash) => intent_hash,
                    Err(error) => {
                        return Ok(Response::new(SendTurnResponse {
                            outcome: Some(send_turn_response::Outcome::Error(service_error(
                                error,
                                &correlation,
                            ))),
                        }));
                    }
                };
                let handles = body
                    .attachments
                    .iter()
                    .map(|attachment| {
                        let source = attachment
                            .source
                            .as_ref()
                            .ok_or(ServiceError::InvalidRequest)?;
                        Ok(format!("{}:{}", source.kind, source.id))
                    })
                    .collect::<Result<Vec<_>, ServiceError>>();
                let handles = match handles {
                    Ok(handles) => handles,
                    Err(error) => {
                        return Ok(Response::new(SendTurnResponse {
                            outcome: Some(send_turn_response::Outcome::Error(service_error(
                                error,
                                &correlation,
                            ))),
                        }));
                    }
                };
                let runtime_controls = match parse_runtime_controls(&body) {
                    Ok(runtime_controls) => runtime_controls,
                    Err(error) => {
                        return Ok(Response::new(SendTurnResponse {
                            outcome: Some(send_turn_response::Outcome::Error(service_error(
                                error,
                                &correlation,
                            ))),
                        }));
                    }
                };
                let delivery_mode = match WireTurnDeliveryMode::try_from(body.delivery_mode)
                    .unwrap_or(WireTurnDeliveryMode::Unspecified)
                {
                    WireTurnDeliveryMode::Unspecified | WireTurnDeliveryMode::NewTurn => {
                        TurnDeliveryMode::NewTurn
                    }
                    WireTurnDeliveryMode::SteerNow => TurnDeliveryMode::SteerNow,
                };
                let trace = TraceContext {
                    installation_id: self.sessions.policy().installation_id.clone(),
                    device_id: authenticated.device_id,
                    correlation_id: command.correlation_id.clone(),
                    authority_epoch: authenticated.authority_epoch,
                };
                match self
                    .accept(&command, command_id, "send_turn", intent_hash)
                    .await
                {
                    Ok(accepted_command) => match self
                        .application
                        .send_turn(ConversationTurnRequest {
                            trace,
                            command_id,
                            project_id,
                            session_id,
                            expected_revision: command.expected_revision,
                            text: body.text,
                            context_handles: handles,
                            runtime_controls,
                            delivery_mode,
                            expected_active_turn_id,
                        })
                        .await
                    {
                        Ok(accepted) => send_turn_response::Outcome::Accepted(SendTurnAccepted {
                            command: Some(accepted_command),
                            turn_id: accepted.agent_turn_id.0.to_string(),
                        }),
                        Err(error) => send_turn_response::Outcome::Error(conversation_error(
                            error,
                            &correlation,
                        )),
                    },
                    Err(error) => {
                        send_turn_response::Outcome::Error(service_error(error, &correlation))
                    }
                }
            }
            Err(error) => send_turn_response::Outcome::Error(service_error(error, &correlation)),
        };
        Ok(Response::new(SendTurnResponse {
            outcome: Some(outcome),
        }))
    }

    async fn cancel_turn(
        &self,
        request: Request<CancelTurnRequest>,
    ) -> Result<Response<CancelTurnResponse>, Status> {
        let correlation = request
            .get_ref()
            .command
            .as_ref()
            .map_or("", |command| command.correlation_id.as_str())
            .to_owned();
        let command = match command_from(request.get_ref().command.as_ref()) {
            Ok(command) => command,
            Err(error) => {
                return Ok(Response::new(CancelTurnResponse {
                    outcome: Some(cancel_turn_response::Outcome::Error(service_error(
                        error,
                        &correlation,
                    ))),
                }));
            }
        };
        let outcome = match self.authenticate(&request, &command.client_session_id) {
            Ok(authenticated) if command.authority_epoch_seen == authenticated.authority_epoch => {
                let body = request.into_inner();
                let parsed = parse_optional_project_id(&body.project_id).and_then(|project_id| {
                    Ok((
                        project_id,
                        SessionId(parse_uuid(&body.session_id)?),
                        TurnId(parse_uuid(&body.turn_id)?),
                        CommandId(parse_uuid(&command.command_id)?),
                    ))
                });
                match parsed {
                    Ok((project_id, session_id, turn_id, command_id)) => {
                        let intent_hash = command_intent_hash(
                            "cancel_turn",
                            command.expected_revision,
                            &[
                                body.project_id.as_str(),
                                body.session_id.as_str(),
                                body.turn_id.as_str(),
                            ],
                        );
                        match self
                            .accept(&command, command_id, "cancel_turn", intent_hash)
                            .await
                        {
                            Ok(accepted) => {
                                match self
                                    .application
                                    .cancel_turn(project_id, session_id, turn_id)
                                    .await
                                {
                                    Ok(_) => cancel_turn_response::Outcome::Accepted(accepted),
                                    Err(error) => cancel_turn_response::Outcome::Error(
                                        conversation_error(error, &correlation),
                                    ),
                                }
                            }
                            Err(error) => cancel_turn_response::Outcome::Error(service_error(
                                error,
                                &correlation,
                            )),
                        }
                    }
                    Err(error) => {
                        cancel_turn_response::Outcome::Error(service_error(error, &correlation))
                    }
                }
            }
            Ok(_) => {
                cancel_turn_response::Outcome::Error(auth_error(AuthError::AuthorityEpochChanged))
            }
            Err(error) => cancel_turn_response::Outcome::Error(auth_error(error)),
        };
        Ok(Response::new(CancelTurnResponse {
            outcome: Some(outcome),
        }))
    }

    async fn get_composer_draft(
        &self,
        request: Request<GetComposerDraftRequest>,
    ) -> Result<Response<GetComposerDraftResponse>, Status> {
        let outcome = match self.authenticate(&request, &request.get_ref().client_session_id) {
            Ok(_) => {
                let body = request.into_inner();
                let parsed = parse_optional_project_id(&body.project_id).and_then(|project_id| {
                    Ok((project_id, SessionId(parse_uuid(&body.session_id)?)))
                });
                match parsed {
                    Ok((project_id, session_id)) => {
                        match self.drafts.load(project_id, session_id).await {
                            Ok(Some(draft)) => {
                                get_composer_draft_response::Outcome::Draft(draft_to_wire(&draft))
                            }
                            Ok(None) => get_composer_draft_response::Outcome::Missing(
                                ComposerDraftMissing {
                                    session_id: session_id.0.to_string(),
                                },
                            ),
                            Err(error) => {
                                get_composer_draft_response::Outcome::Error(draft_error(error, ""))
                            }
                        }
                    }
                    Err(error) => {
                        get_composer_draft_response::Outcome::Error(service_error(error, ""))
                    }
                }
            }
            Err(error) => get_composer_draft_response::Outcome::Error(auth_error(error)),
        };
        Ok(Response::new(GetComposerDraftResponse {
            outcome: Some(outcome),
        }))
    }

    async fn save_composer_draft(
        &self,
        request: Request<SaveComposerDraftRequest>,
    ) -> Result<Response<SaveComposerDraftResponse>, Status> {
        let correlation = request
            .get_ref()
            .operation
            .as_ref()
            .map_or("", |command| command.correlation_id.as_str())
            .to_owned();
        let command = match command_from(request.get_ref().operation.as_ref()) {
            Ok(command) => command,
            Err(error) => {
                return Ok(Response::new(SaveComposerDraftResponse {
                    outcome: Some(save_composer_draft_response::Outcome::Error(service_error(
                        error,
                        &correlation,
                    ))),
                }));
            }
        };
        let outcome = match self.authenticate(&request, &command.client_session_id) {
            Ok(authenticated) if command.authority_epoch_seen == authenticated.authority_epoch => {
                let body = request.into_inner();
                let parsed = body
                    .draft
                    .ok_or(ServiceError::InvalidRequest)
                    .and_then(|draft| {
                        Ok(DraftRecord {
                            project_id: parse_optional_project_id(&draft.project_id)?,
                            session_id: SessionId(parse_uuid(&draft.session_id)?),
                            command_id: CommandId(parse_uuid(&draft.command_id)?),
                            text: draft.text,
                            revision: draft.revision,
                            updated_at_unix_ms: timestamp_to_unix_ms(draft.updated_at.as_ref())?,
                        })
                    });
                match parsed {
                    Ok(draft) => {
                        let session_id = draft.session_id;
                        let command_id = draft.command_id;
                        match self.drafts.save(draft).await {
                            Ok(outcome) => save_composer_draft_response::Outcome::Saved(
                                ComposerDraftWriteReceipt {
                                    session_id: session_id.0.to_string(),
                                    command_id: command_id.0.to_string(),
                                    persisted_at: Some(timestamp(unix_time_ms())),
                                    state: match outcome {
                                        DraftSaveOutcome::Saved => {
                                            ComposerDraftWriteState::Saved as i32
                                        }
                                        DraftSaveOutcome::AlreadyAccepted => {
                                            ComposerDraftWriteState::AlreadyAccepted as i32
                                        }
                                    },
                                },
                            ),
                            Err(error) => save_composer_draft_response::Outcome::Error(
                                draft_error(error, &correlation),
                            ),
                        }
                    }
                    Err(error) => save_composer_draft_response::Outcome::Error(service_error(
                        error,
                        &correlation,
                    )),
                }
            }
            Ok(_) => save_composer_draft_response::Outcome::Error(auth_error(
                AuthError::AuthorityEpochChanged,
            )),
            Err(error) => save_composer_draft_response::Outcome::Error(auth_error(error)),
        };
        Ok(Response::new(SaveComposerDraftResponse {
            outcome: Some(outcome),
        }))
    }

    async fn discard_composer_draft(
        &self,
        request: Request<DiscardComposerDraftRequest>,
    ) -> Result<Response<DiscardComposerDraftResponse>, Status> {
        let correlation = request
            .get_ref()
            .operation
            .as_ref()
            .map_or("", |command| command.correlation_id.as_str())
            .to_owned();
        let command = match command_from(request.get_ref().operation.as_ref()) {
            Ok(command) => command,
            Err(error) => {
                return Ok(Response::new(DiscardComposerDraftResponse {
                    outcome: Some(discard_composer_draft_response::Outcome::Error(
                        service_error(error, &correlation),
                    )),
                }));
            }
        };
        let outcome = match self.authenticate(&request, &command.client_session_id) {
            Ok(authenticated) if command.authority_epoch_seen == authenticated.authority_epoch => {
                let body = request.into_inner();
                let parsed = parse_optional_project_id(&body.project_id).and_then(|project_id| {
                    Ok((
                        project_id,
                        SessionId(parse_uuid(&body.session_id)?),
                        CommandId(parse_uuid(&body.draft_command_id)?),
                    ))
                });
                match parsed {
                    Ok((project_id, session_id, draft_command_id)) => {
                        match self
                            .drafts
                            .discard(project_id, session_id, draft_command_id)
                            .await
                        {
                            Ok(existed) => discard_composer_draft_response::Outcome::Discarded(
                                ComposerDraftDiscarded {
                                    session_id: session_id.0.to_string(),
                                    existed,
                                },
                            ),
                            Err(error) => discard_composer_draft_response::Outcome::Error(
                                draft_error(error, &correlation),
                            ),
                        }
                    }
                    Err(error) => discard_composer_draft_response::Outcome::Error(service_error(
                        error,
                        &correlation,
                    )),
                }
            }
            Ok(_) => discard_composer_draft_response::Outcome::Error(auth_error(
                AuthError::AuthorityEpochChanged,
            )),
            Err(error) => discard_composer_draft_response::Outcome::Error(auth_error(error)),
        };
        Ok(Response::new(DiscardComposerDraftResponse {
            outcome: Some(outcome),
        }))
    }

    type WatchSessionStream =
        Pin<Box<dyn Stream<Item = Result<WatchSessionResponse, Status>> + Send>>;

    async fn watch_session(
        &self,
        request: Request<WatchSessionRequest>,
    ) -> Result<Response<Self::WatchSessionStream>, Status> {
        let request_peer = peer(&request);
        let client_session_id = request.get_ref().client_session_id.clone();
        let authorization = request_peer.and_then(|peer| {
            self.sessions
                .authorize_active(&peer, &client_session_id)
                .map(|_| (peer, client_session_id))
        });
        let session_id = parse_uuid(&request.get_ref().session_id).map(SessionId);
        let application = self.application.clone();
        let sessions = self.sessions.clone();
        let lease_interval = lease_recheck_interval(self.sessions.policy().ui_session_ttl);
        let stream = async_stream::stream! {
            let (peer, client_session_id) = match authorization {
                Ok(authorization) => authorization,
                Err(error) => {
                    yield Ok(session_error_response(auth_error(error)));
                    return;
                }
            };
            let session_id = match session_id {
                Ok(session_id) => session_id,
                Err(error) => {
                    yield Ok(session_error_response(service_error(error, "")));
                    return;
                }
            };
            let mut subscription = match application.subscribe(session_id).await {
                Ok(subscription) => subscription,
                Err(error) => {
                    yield Ok(session_error_response(conversation_error(error, "")));
                    return;
                }
            };
            let mut commands = HashMap::new();
            let mut activity_created = HashMap::new();
            let Some(initial) = subscription.take_initial() else {
                yield Ok(session_error_response(internal_error("session_watch_missing_snapshot", "session.watch_missing_snapshot")));
                return;
            };
            if let WatchFrame::Snapshot { value, .. } = &initial {
                for turn in &value.turns {
                    commands.insert(turn.turn_id, turn.command_id);
                    for activity in &turn.activities {
                        activity_created.insert(
                            (turn.turn_id, activity.activity_id.clone()),
                            (
                                activity.created_at_unix_ms,
                                activity.created_revision,
                            ),
                        );
                    }
                }
            }
            yield Ok(session_watch_to_wire(initial, &mut commands, &mut activity_created));
            let mut lease = tokio::time::interval(lease_interval);
            lease.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            lease.tick().await;
            loop {
                tokio::select! {
                    _ = lease.tick() => {
                        if let Err(error) = sessions.authorize_active(&peer, &client_session_id) {
                            yield Ok(session_error_response(auth_error(error)));
                            return;
                        }
                        if let Some(heartbeat) = subscription.heartbeat() {
                            yield Ok(session_watch_to_wire(heartbeat, &mut commands, &mut activity_created));
                        }
                    }
                    received = subscription.recv() => {
                        match received {
                            Ok(Some(frame)) => yield Ok(session_watch_to_wire(frame, &mut commands, &mut activity_created)),
                            Ok(None) => return,
                            Err(error) => {
                                yield Ok(session_error_response(conversation_error(error.into(), "")));
                                return;
                            }
                        }
                    }
                }
            }
        };
        Ok(Response::new(Box::pin(stream)))
    }
}

#[cfg(windows)]
pub async fn run_system_server<S, F>(
    endpoint: LocalEndpoint,
    service: SystemServiceAdapter<S>,
    shutdown: F,
) -> Result<(), TransportError>
where
    S: SystemStatePort + 'static,
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let incoming = crate::transport::secure_incoming(endpoint)?;
    tonic::transport::Server::builder()
        .add_service(
            SystemServiceServer::new(service)
                .max_decoding_message_size(DEFAULT_MAX_MESSAGE_BYTES as usize)
                .max_encoding_message_size(DEFAULT_MAX_MESSAGE_BYTES as usize),
        )
        .serve_with_incoming_shutdown(incoming, shutdown)
        .await
        .map_err(TransportError::from)
}

#[cfg(windows)]
pub async fn run_local_server<S, F>(
    endpoint: LocalEndpoint,
    system: SystemServiceAdapter<S>,
    sessions: SessionServiceAdapter,
    shutdown: F,
) -> Result<(), TransportError>
where
    S: SystemStatePort + 'static,
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let incoming = crate::transport::secure_incoming(endpoint)?;
    tonic::transport::Server::builder()
        .add_service(
            SystemServiceServer::new(system)
                .max_decoding_message_size(DEFAULT_MAX_MESSAGE_BYTES as usize)
                .max_encoding_message_size(DEFAULT_MAX_MESSAGE_BYTES as usize),
        )
        .add_service(
            SessionServiceServer::new(sessions)
                .max_decoding_message_size(DEFAULT_MAX_MESSAGE_BYTES as usize)
                .max_encoding_message_size(DEFAULT_MAX_MESSAGE_BYTES as usize),
        )
        .serve_with_incoming_shutdown(incoming, shutdown)
        .await
        .map_err(TransportError::from)
}

#[cfg(not(windows))]
pub async fn run_local_server<S, F>(
    _endpoint: LocalEndpoint,
    _system: SystemServiceAdapter<S>,
    _sessions: SessionServiceAdapter,
    _shutdown: F,
) -> Result<(), TransportError>
where
    S: SystemStatePort + 'static,
    F: std::future::Future<Output = ()> + Send + 'static,
{
    Err(TransportError::UnsupportedPlatform)
}

#[cfg(not(windows))]
pub async fn run_system_server<S, F>(
    _endpoint: LocalEndpoint,
    _service: SystemServiceAdapter<S>,
    _shutdown: F,
) -> Result<(), TransportError>
where
    S: SystemStatePort + 'static,
    F: std::future::Future<Output = ()> + Send + 'static,
{
    Err(TransportError::UnsupportedPlatform)
}

fn peer<T>(request: &Request<T>) -> Result<PeerIdentity, AuthError> {
    request
        .extensions()
        .get::<PeerIdentity>()
        .cloned()
        .ok_or(AuthError::ConnectionMismatch)
}

fn snapshot_to_wire(snapshot: &SystemSnapshot) -> BootstrapSnapshot {
    BootstrapSnapshot {
        revision: snapshot.revision,
        authority_epoch: snapshot.authority_epoch,
        observed_at: Some(timestamp(snapshot.observed_at_unix_ms)),
        projects: snapshot.projects.iter().map(project_to_wire).collect(),
        recent_sessions: snapshot
            .recent_sessions
            .iter()
            .map(session_to_wire)
            .collect(),
        active_project_id: snapshot.active_project_id.clone().unwrap_or_default(),
        active_session_id: snapshot.active_session_id.clone().unwrap_or_default(),
        node_state: health_to_wire(snapshot.health),
        runtime: snapshot.runtime.as_ref().map(runtime_to_wire),
    }
}

fn runtime_to_wire(runtime: &RuntimeDescriptor) -> WireRuntimeSummary {
    WireRuntimeSummary {
        adapter_id: runtime.adapter_id.clone(),
        runtime_kind: match runtime.runtime_kind {
            RuntimeKind::NativeAgent => "native_agent",
            RuntimeKind::GenericLoop => "generic_loop",
        }
        .to_owned(),
        streaming: runtime.capabilities.streaming,
        continuation: runtime.capabilities.continuation,
        scoped_cancellation: runtime.capabilities.scoped_cancellation,
        deadlines: runtime.capabilities.deadlines,
        steering: runtime.capabilities.steering.as_str().to_owned(),
        native_extension_schemas: runtime.capabilities.native_extension_schemas.clone(),
        controls: runtime
            .controls
            .iter()
            .map(|control| WireRuntimeControlDescriptor {
                id: control.id.clone(),
                label: control.label.clone(),
                default_choice_id: control.default_choice_id.clone(),
                choices: control
                    .choices
                    .iter()
                    .map(|choice| WireRuntimeControlChoice {
                        id: choice.id.clone(),
                        label: choice.label.clone(),
                        description: choice.description.clone(),
                        available_when: choice
                            .available_when
                            .iter()
                            .map(|condition| WireRuntimeControlCondition {
                                control_id: condition.control_id.clone(),
                                choice_ids: condition.choice_ids.clone(),
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn project_to_wire(project: &ProjectSummary) -> WireProjectSummary {
    WireProjectSummary {
        project_id: project.project_id.clone(),
        display_name: project.display_name.clone(),
        state: ProjectState::Ready as i32,
        revision: project.revision,
        last_activity_at: None,
    }
}

fn session_to_wire(session: &SessionSummary) -> WireSessionSummary {
    WireSessionSummary {
        session_id: session.session_id.clone(),
        project_id: session.project_id.clone(),
        title: session.title.clone(),
        state: session_state_to_wire(session.state),
        revision: session.revision,
        active_turn_id: session.active_turn_id.clone().unwrap_or_default(),
        last_activity_at: Some(timestamp(session.last_activity_unix_ms)),
    }
}

fn delta_to_wire(base_revision: u64, new_revision: u64, delta: SystemDelta) -> WireSystemDelta {
    WireSystemDelta {
        base_revision,
        new_revision,
        mutations: delta
            .mutations
            .into_iter()
            .map(|mutation| WireSystemMutation {
                mutation: Some(match mutation {
                    SystemMutation::UpsertProject(project) => {
                        system_mutation::Mutation::UpsertProject(project_to_wire(&project))
                    }
                    SystemMutation::RemoveProject(project_id) => {
                        system_mutation::Mutation::RemoveProjectId(project_id)
                    }
                    SystemMutation::UpsertSession(session) => {
                        system_mutation::Mutation::UpsertSession(session_to_wire(&session))
                    }
                    SystemMutation::RemoveSession(session_id) => {
                        system_mutation::Mutation::RemoveSessionId(session_id)
                    }
                    SystemMutation::Select {
                        project_id,
                        session_id,
                    } => system_mutation::Mutation::UpdateSelection(SystemSelectionUpdate {
                        active_project_id: Some(project_id.unwrap_or_default()),
                        active_session_id: Some(session_id.unwrap_or_default()),
                    }),
                    SystemMutation::SetHealth(health) => {
                        system_mutation::Mutation::UpdateHealth(SystemHealthUpdate {
                            node_state: health_to_wire(health),
                            status_code: "node_health_changed".to_owned(),
                            observed_at: None,
                        })
                    }
                }),
            })
            .collect(),
    }
}

fn watch_to_wire(frame: DomainWatchFrame) -> WatchResponse {
    let frame = match frame {
        WatchFrame::Snapshot {
            cursor,
            fingerprint,
            value,
            ..
        } => SystemWatchFrame {
            cursor: Some(cursor_to_wire(cursor)),
            frame: Some(system_watch_frame::Frame::Snapshot(WireSystemSnapshot {
                bootstrap: Some(snapshot_to_wire(&value)),
                snapshot_fingerprint: fingerprint,
            })),
        },
        WatchFrame::Delta {
            cursor,
            base_revision,
            new_revision,
            delta,
        } => SystemWatchFrame {
            cursor: Some(cursor_to_wire(cursor)),
            frame: Some(system_watch_frame::Frame::Delta(delta_to_wire(
                base_revision,
                new_revision,
                delta,
            ))),
        },
        WatchFrame::Heartbeat {
            cursor,
            current_revision,
        } => SystemWatchFrame {
            cursor: Some(cursor_to_wire(cursor)),
            frame: Some(system_watch_frame::Frame::Heartbeat(WatchHeartbeat {
                observed_at: Some(timestamp(unix_time_ms())),
                current_revision,
            })),
        },
        WatchFrame::ResyncRequired {
            cursor,
            current_revision,
            reason,
        } => SystemWatchFrame {
            cursor: Some(cursor_to_wire(cursor)),
            frame: Some(system_watch_frame::Frame::ResyncRequired(ResyncRequired {
                reason: resync_reason_to_wire(reason),
                current_revision,
                snapshot_required: true,
            })),
        },
        WatchFrame::Unavailable { error } => SystemWatchFrame {
            cursor: None,
            frame: Some(system_watch_frame::Frame::Error(internal_error(
                &error.code,
                &error.message_key,
            ))),
        },
        WatchFrame::AccessRevoked => SystemWatchFrame {
            cursor: None,
            frame: Some(system_watch_frame::Frame::Error(internal_error(
                "access_revoked",
                "system.access_revoked",
            ))),
        },
    };
    WatchResponse { frame: Some(frame) }
}

fn session_watch_to_wire(
    frame: dennett_head::session::SessionWatchFrame,
    commands: &mut HashMap<TurnId, CommandId>,
    activity_created: &mut HashMap<(TurnId, String), (u64, u64)>,
) -> WatchSessionResponse {
    let frame = match frame {
        WatchFrame::Snapshot { cursor, value, .. } => {
            for turn in &value.turns {
                commands.insert(turn.turn_id, turn.command_id);
                for activity in &turn.activities {
                    activity_created.insert(
                        (turn.turn_id, activity.activity_id.clone()),
                        (activity.created_at_unix_ms, activity.created_revision),
                    );
                }
            }
            SessionWatchFrame {
                cursor: Some(cursor_to_wire(cursor)),
                frame: Some(session_watch_frame::Frame::Snapshot(
                    session_snapshot_to_wire(&value),
                )),
            }
        }
        WatchFrame::Delta {
            cursor,
            base_revision,
            new_revision,
            delta,
        } => SessionWatchFrame {
            cursor: Some(cursor_to_wire(cursor)),
            frame: Some(session_watch_frame::Frame::Delta(session_delta_to_wire(
                base_revision,
                new_revision,
                delta,
                commands,
                activity_created,
            ))),
        },
        WatchFrame::Heartbeat {
            cursor,
            current_revision,
        } => SessionWatchFrame {
            cursor: Some(cursor_to_wire(cursor)),
            frame: Some(session_watch_frame::Frame::Heartbeat(WatchHeartbeat {
                observed_at: Some(timestamp(unix_time_ms())),
                current_revision,
            })),
        },
        WatchFrame::ResyncRequired {
            cursor,
            current_revision,
            reason,
        } => SessionWatchFrame {
            cursor: Some(cursor_to_wire(cursor)),
            frame: Some(session_watch_frame::Frame::ResyncRequired(ResyncRequired {
                reason: resync_reason_to_wire(reason),
                current_revision,
                snapshot_required: true,
            })),
        },
        WatchFrame::Unavailable { error } => SessionWatchFrame {
            cursor: None,
            frame: Some(session_watch_frame::Frame::Error(internal_error(
                &error.code,
                &error.message_key,
            ))),
        },
        WatchFrame::AccessRevoked => SessionWatchFrame {
            cursor: None,
            frame: Some(session_watch_frame::Frame::Error(internal_error(
                "access_revoked",
                "session.access_revoked",
            ))),
        },
    };
    WatchSessionResponse { frame: Some(frame) }
}

fn session_snapshot_to_wire(snapshot: &ProjectSessionSnapshot) -> SessionSnapshot {
    SessionSnapshot {
        session: Some(session_summary_snapshot_to_wire(snapshot)),
        snapshot_fingerprint: snapshot.fingerprint.to_vec(),
        turns: snapshot.turns.iter().map(turn_to_wire).collect(),
    }
}

fn session_summary_snapshot_to_wire(snapshot: &ProjectSessionSnapshot) -> WireSessionSummary {
    WireSessionSummary {
        session_id: snapshot.session.session_id.0.to_string(),
        project_id: snapshot
            .session
            .project_id
            .map_or_else(String::new, |project_id| project_id.0.to_string()),
        title: snapshot.session.title.clone(),
        state: session_state_to_wire(snapshot.session.state),
        revision: snapshot.session.revision,
        active_turn_id: snapshot
            .session
            .active_turn_id
            .map_or_else(String::new, |turn_id| turn_id.0.to_string()),
        last_activity_at: Some(timestamp(snapshot.session.last_activity_unix_ms)),
    }
}

fn turn_to_wire(turn: &SessionTurn) -> TurnSnapshot {
    TurnSnapshot {
        turn_id: turn.turn_id.0.to_string(),
        command_id: turn.command_id.0.to_string(),
        role: turn_role_to_wire(turn.role),
        state: turn_state_to_wire(turn.state),
        text: turn.text.clone(),
        outcome: turn.outcome.as_ref().map(|outcome| {
            turn_snapshot::Outcome::from(outcome_to_wire(outcome, turn.command_id, turn.turn_id))
        }),
        created_at: Some(timestamp(turn.created_at_unix_ms)),
        completed_at: turn.completed_at_unix_ms.map(timestamp),
        activities: turn.activities.iter().map(activity_to_wire).collect(),
        created_revision: turn.created_revision,
    }
}

fn activity_to_wire(activity: &SessionTurnActivity) -> TurnActivitySnapshot {
    TurnActivitySnapshot {
        activity_id: activity.activity_id.clone(),
        phase: activity.phase.clone(),
        message: activity.message.clone(),
        status: activity_status_to_wire(activity.status),
        updated_at: Some(timestamp(activity.updated_at_unix_ms)),
        created_at: Some(timestamp(if activity.created_at_unix_ms == 0 {
            activity.updated_at_unix_ms
        } else {
            activity.created_at_unix_ms
        })),
        created_revision: activity.created_revision,
        native_extensions: activity
            .native_extensions
            .iter()
            .map(native_extension_to_wire)
            .collect(),
    }
}

fn native_extension_to_wire(extension: &SessionNativeExtension) -> NativeExtensionPayload {
    NativeExtensionPayload {
        namespace: extension.namespace.clone(),
        schema_version: extension.schema_version.clone(),
        json_value: extension.json_value.clone(),
    }
}

impl From<WireTurnOutcome> for turn_snapshot::Outcome {
    fn from(value: WireTurnOutcome) -> Self {
        match value {
            WireTurnOutcome::Result(result) => Self::Result(result),
            WireTurnOutcome::Error(error) => Self::Error(error),
        }
    }
}

enum WireTurnOutcome {
    Result(WireResultEnvelope),
    Error(ErrorEnvelope),
}

fn outcome_to_wire(
    outcome: &SessionTurnOutcome,
    command_id: CommandId,
    turn_id: TurnId,
) -> WireTurnOutcome {
    match outcome {
        SessionTurnOutcome::Result(result) => {
            WireTurnOutcome::Result(result_to_wire(result, command_id, turn_id))
        }
        SessionTurnOutcome::Error(error) => WireTurnOutcome::Error(session_error_to_wire(error)),
    }
}

fn result_to_wire(
    result: &SessionResult,
    command_id: CommandId,
    turn_id: TurnId,
) -> WireResultEnvelope {
    WireResultEnvelope {
        command_id: command_id.0.to_string(),
        turn_id: turn_id.0.to_string(),
        summary: result.summary.clone(),
        partial: result.partial,
        artifacts: result
            .artifact_handles
            .iter()
            .map(|id| StableRef {
                kind: "artifact".to_owned(),
                id: id.clone(),
            })
            .collect(),
        evidence: result
            .evidence_handles
            .iter()
            .map(|id| StableRef {
                kind: "evidence".to_owned(),
                id: id.clone(),
            })
            .collect(),
    }
}

fn session_error_to_wire(error: &SafeSessionError) -> ErrorEnvelope {
    ErrorEnvelope {
        code: error.code.clone(),
        message_key: error.message_key.clone(),
        correlation_id: String::new(),
        retryable: false,
        user_action_required: false,
        details_handle: error.details_handle.clone().unwrap_or_default(),
        current_revision: None,
    }
}

fn session_delta_to_wire(
    base_revision: u64,
    new_revision: u64,
    event: CommittedSessionEvent,
    commands: &mut HashMap<TurnId, CommandId>,
    activity_created: &mut HashMap<(TurnId, String), (u64, u64)>,
) -> SessionDelta {
    let committed_at_unix_ms = event.committed_at_unix_ms;
    let committed_revision = event.revision;
    let mutations = match event.body {
        SessionEventBody::SessionCreated { title, .. } => vec![SessionMutation {
            mutation: Some(session_mutation::Mutation::UpdateSession(
                SessionMetadataUpdate {
                    title: Some(title),
                    state: Some(SessionState::Idle as i32),
                    active_turn_id: Some(String::new()),
                },
            )),
        }],
        SessionEventBody::TurnAccepted {
            user_turn_id,
            agent_turn_id,
            command_id,
            text,
        } => {
            commands.insert(user_turn_id, command_id);
            commands.insert(agent_turn_id, command_id);
            vec![
                SessionMutation {
                    mutation: Some(session_mutation::Mutation::UpsertTurn(TurnSnapshot {
                        turn_id: user_turn_id.0.to_string(),
                        command_id: command_id.0.to_string(),
                        role: TurnRole::User as i32,
                        state: TurnState::Completed as i32,
                        text,
                        outcome: None,
                        created_at: Some(timestamp(committed_at_unix_ms)),
                        completed_at: Some(timestamp(committed_at_unix_ms)),
                        activities: Vec::new(),
                        created_revision: committed_revision,
                    })),
                },
                SessionMutation {
                    mutation: Some(session_mutation::Mutation::UpsertTurn(TurnSnapshot {
                        turn_id: agent_turn_id.0.to_string(),
                        command_id: command_id.0.to_string(),
                        role: TurnRole::Agent as i32,
                        state: TurnState::Accepted as i32,
                        text: String::new(),
                        outcome: None,
                        created_at: Some(timestamp(committed_at_unix_ms)),
                        completed_at: None,
                        activities: Vec::new(),
                        created_revision: committed_revision,
                    })),
                },
                SessionMutation {
                    mutation: Some(session_mutation::Mutation::UpdateSession(
                        SessionMetadataUpdate {
                            title: None,
                            state: Some(SessionState::Running as i32),
                            active_turn_id: Some(agent_turn_id.0.to_string()),
                        },
                    )),
                },
            ]
        }
        SessionEventBody::UserSteerRequested {
            user_turn_id,
            command_id,
            text,
            ..
        } => {
            commands.insert(user_turn_id, command_id);
            vec![SessionMutation {
                mutation: Some(session_mutation::Mutation::UpsertTurn(TurnSnapshot {
                    turn_id: user_turn_id.0.to_string(),
                    command_id: command_id.0.to_string(),
                    role: TurnRole::User as i32,
                    state: TurnState::Accepted as i32,
                    text,
                    outcome: None,
                    created_at: Some(timestamp(committed_at_unix_ms)),
                    completed_at: None,
                    activities: Vec::new(),
                    created_revision: committed_revision,
                })),
            }]
        }
        SessionEventBody::UserSteerFinished {
            user_turn_id,
            state,
            error,
            ..
        } => {
            vec![SessionMutation {
                mutation: Some(session_mutation::Mutation::FinishTurn(TurnTerminal {
                    turn_id: user_turn_id.0.to_string(),
                    state: turn_state_to_wire(state),
                    outcome: error
                        .map(|error| turn_terminal::Outcome::Error(session_error_to_wire(&error))),
                })),
            }]
        }
        SessionEventBody::AgentTextAppended { turn_id, text } => vec![SessionMutation {
            mutation: Some(session_mutation::Mutation::AppendTurnText(TurnTextAppend {
                turn_id: turn_id.0.to_string(),
                text,
            })),
        }],
        SessionEventBody::AgentActivityUpserted {
            turn_id,
            activity_id,
            phase,
            message,
            status,
            native_extensions,
        } => {
            let (created_at_unix_ms, created_revision) = *activity_created
                .entry((turn_id, activity_id.clone()))
                .or_insert((committed_at_unix_ms, committed_revision));
            vec![SessionMutation {
                mutation: Some(session_mutation::Mutation::UpsertTurnActivity(
                    TurnActivityUpsert {
                        turn_id: turn_id.0.to_string(),
                        activity: Some(TurnActivitySnapshot {
                            activity_id,
                            phase,
                            message,
                            status: activity_status_to_wire(status),
                            updated_at: Some(timestamp(committed_at_unix_ms)),
                            created_at: Some(timestamp(created_at_unix_ms)),
                            created_revision,
                            native_extensions: native_extensions
                                .iter()
                                .map(native_extension_to_wire)
                                .collect(),
                        }),
                    },
                )),
            }]
        }
        SessionEventBody::TurnFinished {
            turn_id,
            state,
            outcome,
        } => {
            let command_id = commands
                .get(&turn_id)
                .copied()
                .unwrap_or(CommandId(turn_id.0));
            let terminal_outcome = outcome.as_ref().map(|outcome| {
                match outcome_to_wire(outcome, command_id, turn_id) {
                    WireTurnOutcome::Result(result) => turn_terminal::Outcome::Result(result),
                    WireTurnOutcome::Error(error) => turn_terminal::Outcome::Error(error),
                }
            });
            vec![
                SessionMutation {
                    mutation: Some(session_mutation::Mutation::FinishTurn(TurnTerminal {
                        turn_id: turn_id.0.to_string(),
                        state: turn_state_to_wire(state),
                        outcome: terminal_outcome,
                    })),
                },
                SessionMutation {
                    mutation: Some(session_mutation::Mutation::UpdateSession(
                        SessionMetadataUpdate {
                            title: None,
                            state: Some(if state == SessionTurnState::Failed {
                                SessionState::Failed as i32
                            } else {
                                SessionState::Idle as i32
                            }),
                            active_turn_id: Some(String::new()),
                        },
                    )),
                },
            ]
        }
    };
    SessionDelta {
        base_revision,
        new_revision,
        mutations,
        committed_at: Some(timestamp(committed_at_unix_ms)),
    }
}

fn session_error_response(error: ErrorEnvelope) -> WatchSessionResponse {
    WatchSessionResponse {
        frame: Some(SessionWatchFrame {
            cursor: None,
            frame: Some(session_watch_frame::Frame::Error(error)),
        }),
    }
}

fn session_state_to_wire(state: ProjectSessionState) -> i32 {
    (match state {
        ProjectSessionState::Idle => SessionState::Idle,
        ProjectSessionState::Running => SessionState::Running,
        ProjectSessionState::Waiting => SessionState::Waiting,
        ProjectSessionState::Failed => SessionState::Failed,
        ProjectSessionState::Archived => SessionState::Archived,
    }) as i32
}

fn turn_role_to_wire(role: SessionTurnRole) -> i32 {
    (match role {
        SessionTurnRole::User => TurnRole::User,
        SessionTurnRole::Agent => TurnRole::Agent,
        SessionTurnRole::System => TurnRole::System,
    }) as i32
}

fn activity_status_to_wire(status: SessionActivityStatus) -> i32 {
    (match status {
        SessionActivityStatus::Started => WireTurnActivityStatus::Started,
        SessionActivityStatus::Updated => WireTurnActivityStatus::Updated,
        SessionActivityStatus::Completed => WireTurnActivityStatus::Completed,
        SessionActivityStatus::Failed => WireTurnActivityStatus::Failed,
    }) as i32
}

fn turn_state_to_wire(state: SessionTurnState) -> i32 {
    (match state {
        SessionTurnState::Accepted => TurnState::Accepted,
        SessionTurnState::Streaming => TurnState::Streaming,
        SessionTurnState::Completed => TurnState::Completed,
        SessionTurnState::Cancelled => TurnState::Cancelled,
        SessionTurnState::TimedOut => TurnState::TimedOut,
        SessionTurnState::Failed => TurnState::Failed,
    }) as i32
}

fn error_watch_response(error: ErrorEnvelope) -> WatchResponse {
    WatchResponse {
        frame: Some(SystemWatchFrame {
            cursor: None,
            frame: Some(system_watch_frame::Frame::Error(error)),
        }),
    }
}

fn cursor_to_wire(cursor: WatchCursor) -> WireWatchCursor {
    WireWatchCursor {
        stream_id: cursor.stream_id,
        sequence: cursor.sequence,
        authority_epoch: cursor.authority_epoch,
    }
}

fn health_to_wire(health: SystemHealth) -> i32 {
    (match health {
        SystemHealth::Starting => HealthState::Starting,
        SystemHealth::Ready => HealthState::Ready,
        SystemHealth::Degraded => HealthState::Degraded,
        SystemHealth::RecoveryRequired => HealthState::RecoveryRequired,
    }) as i32
}

fn resync_reason_to_wire(reason: ResyncReason) -> i32 {
    (match reason {
        ResyncReason::SequenceGap => WireResyncReason::SequenceGap,
        ResyncReason::RevisionGap => WireResyncReason::RevisionGap,
        ResyncReason::AuthorityEpochChanged => WireResyncReason::AuthorityEpochChanged,
        ResyncReason::StreamReplaced => WireResyncReason::StreamReplaced,
        ResyncReason::SnapshotInvalid => WireResyncReason::SnapshotInvalid,
    }) as i32
}

fn timestamp(unix_ms: u64) -> prost_types::Timestamp {
    prost_types::Timestamp {
        seconds: (unix_ms / 1_000).try_into().unwrap_or(i64::MAX),
        nanos: ((unix_ms % 1_000) * 1_000_000) as i32,
    }
}

fn timestamp_to_unix_ms(value: Option<&prost_types::Timestamp>) -> Result<u64, ServiceError> {
    let value = value.ok_or(ServiceError::InvalidRequest)?;
    if value.seconds < 0 || !(0..1_000_000_000).contains(&value.nanos) {
        return Err(ServiceError::InvalidRequest);
    }
    let seconds = u64::try_from(value.seconds).map_err(|_| ServiceError::InvalidRequest)?;
    seconds
        .checked_mul(1_000)
        .and_then(|millis| millis.checked_add((value.nanos as u64) / 1_000_000))
        .ok_or(ServiceError::InvalidRequest)
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn lease_recheck_interval(ttl: std::time::Duration) -> std::time::Duration {
    (ttl / 4)
        .min(std::time::Duration::from_secs(30))
        .max(std::time::Duration::from_millis(10))
}

fn command_intent_hash(
    operation_kind: &str,
    expected_revision: Option<u64>,
    fields: &[&str],
) -> [u8; 32] {
    fn field(hasher: &mut Sha256, value: &[u8]) {
        let length = u64::try_from(value.len()).unwrap_or(u64::MAX);
        hasher.update(length.to_be_bytes());
        hasher.update(value);
    }

    let mut hasher = Sha256::new();
    hasher.update(b"dennett.command-intent.v1\0");
    field(&mut hasher, operation_kind.as_bytes());
    match expected_revision {
        Some(revision) => {
            hasher.update([1]);
            hasher.update(revision.to_be_bytes());
        }
        None => hasher.update([0]),
    }
    for value in fields {
        field(&mut hasher, value.as_bytes());
    }
    hasher.finalize().into()
}

fn send_turn_intent_hash(
    request: &SendTurnRequest,
    expected_revision: Option<u64>,
) -> Result<[u8; 32], ServiceError> {
    let delivery_mode = request.delivery_mode.to_string();
    let mut fields = vec![
        request.project_id.as_str(),
        request.session_id.as_str(),
        request.text.as_str(),
        delivery_mode.as_str(),
        request.expected_active_turn_id.as_str(),
    ];
    for attachment in &request.attachments {
        let source = attachment
            .source
            .as_ref()
            .ok_or(ServiceError::InvalidRequest)?;
        if source.kind.trim().is_empty() || source.id.trim().is_empty() {
            return Err(ServiceError::InvalidRequest);
        }
        fields.extend([
            source.kind.as_str(),
            source.id.as_str(),
            attachment.label.as_str(),
        ]);
    }
    for selection in &request.runtime_controls {
        fields.extend([selection.control_id.as_str(), selection.choice_id.as_str()]);
    }
    Ok(command_intent_hash("send_turn", expected_revision, &fields))
}

fn parse_runtime_controls(
    request: &SendTurnRequest,
) -> Result<Vec<RuntimeControlSelection>, ServiceError> {
    if request.runtime_controls.len() > 32 {
        return Err(ServiceError::InvalidRequest);
    }
    let mut seen = std::collections::HashSet::new();
    request
        .runtime_controls
        .iter()
        .map(|selection| {
            if selection.control_id.trim().is_empty()
                || selection.choice_id.trim().is_empty()
                || selection.control_id.len() > 128
                || selection.choice_id.len() > 128
                || !seen.insert(selection.control_id.as_str())
            {
                return Err(ServiceError::InvalidRequest);
            }
            Ok(RuntimeControlSelection {
                control_id: selection.control_id.clone(),
                choice_id: selection.choice_id.clone(),
            })
        })
        .collect()
}

fn draft_to_wire(draft: &DraftRecord) -> ComposerDraft {
    ComposerDraft {
        project_id: draft
            .project_id
            .map_or_else(String::new, |id| id.0.to_string()),
        session_id: draft.session_id.0.to_string(),
        command_id: draft.command_id.0.to_string(),
        text: draft.text.clone(),
        updated_at: Some(timestamp(draft.updated_at_unix_ms)),
        revision: draft.revision,
    }
}

fn auth_error(error: AuthError) -> ErrorEnvelope {
    ErrorEnvelope {
        code: error.code().to_owned(),
        message_key: format!("local_ipc.{}", error.code()),
        correlation_id: String::new(),
        retryable: error.retryable(),
        user_action_required: matches!(
            error,
            AuthError::InstallationMismatch | AuthError::ProtocolIncompatible
        ),
        details_handle: String::new(),
        current_revision: None,
    }
}

fn state_error(_error: SystemStateError) -> ErrorEnvelope {
    internal_error("system_unavailable", "system.unavailable")
}

#[derive(Debug)]
enum ServiceError {
    Auth(AuthError),
    Admission(CommandAdmissionError),
    InvalidRequest,
}

impl From<CommandAdmissionError> for ServiceError {
    fn from(error: CommandAdmissionError) -> Self {
        Self::Admission(error)
    }
}

fn command_from(command: Option<&CommandMetadata>) -> Result<CommandMetadata, ServiceError> {
    let command = command.cloned().ok_or(ServiceError::InvalidRequest)?;
    if command.command_id.is_empty()
        || command.idempotency_key.is_empty()
        || command.correlation_id.is_empty()
        || command.client_session_id.is_empty()
        || command.authority_epoch_seen == 0
        || command.created_at.is_none()
        || parse_uuid(&command.command_id).is_err()
    {
        return Err(ServiceError::InvalidRequest);
    }
    Ok(command)
}

fn parse_uuid(value: &str) -> Result<uuid::Uuid, ServiceError> {
    uuid::Uuid::parse_str(value).map_err(|_| ServiceError::InvalidRequest)
}

fn parse_project_id(value: &str) -> Result<ProjectId, ServiceError> {
    parse_uuid(value).map(ProjectId)
}

fn parse_optional_project_id(value: &str) -> Result<Option<ProjectId>, ServiceError> {
    if value.is_empty() {
        Ok(None)
    } else {
        parse_project_id(value).map(Some)
    }
}

fn conversation_error(error: ConversationError, correlation_id: &str) -> ErrorEnvelope {
    let (code, retryable, user_action_required, current_revision) = match error {
        ConversationError::InvalidRequest => ("conversation_request_invalid", false, true, None),
        ConversationError::TurnAlreadyActive => {
            ("conversation_turn_already_active", true, false, None)
        }
        ConversationError::ScopeMismatch => ("conversation_scope_mismatch", false, true, None),
        ConversationError::SessionUnavailable => {
            ("conversation_session_unavailable", true, false, None)
        }
        ConversationError::Session(dennett_memory_core::session::SessionJournalError::NotFound) => {
            ("session_not_found", false, true, None)
        }
        ConversationError::Session(
            dennett_memory_core::session::SessionJournalError::RevisionConflict { actual, .. },
        ) => ("session_revision_stale", true, false, Some(actual)),
        ConversationError::Session(_) => ("session_journal_unavailable", true, false, None),
        ConversationError::Runtime(error) => (error.code.as_str(), error.retryable, false, None),
    };
    ErrorEnvelope {
        code: code.to_owned(),
        message_key: format!("conversation.{code}"),
        correlation_id: correlation_id.to_owned(),
        retryable,
        user_action_required,
        details_handle: String::new(),
        current_revision,
    }
}

fn draft_error(error: DraftApplicationError, correlation_id: &str) -> ErrorEnvelope {
    let (code, retryable, user_action_required) = match error {
        DraftApplicationError::ScopeMismatch => ("draft_scope_mismatch", false, true),
        DraftApplicationError::StableCommandMismatch => ("draft_command_mismatch", false, true),
        DraftApplicationError::Session(
            dennett_memory_core::session::SessionJournalError::NotFound,
        ) => ("draft_session_not_found", false, true),
        DraftApplicationError::Session(_) => ("draft_session_unavailable", true, false),
        DraftApplicationError::Cache(
            dennett_sync_core::draft::DraftCacheError::MigrationFailure,
        ) => ("draft_migration_required", false, true),
        DraftApplicationError::Cache(_) => ("draft_storage_unavailable", true, false),
    };
    ErrorEnvelope {
        code: code.to_owned(),
        message_key: format!("draft.{code}"),
        correlation_id: correlation_id.to_owned(),
        retryable,
        user_action_required,
        details_handle: String::new(),
        current_revision: None,
    }
}

fn service_error(error: ServiceError, correlation_id: &str) -> ErrorEnvelope {
    match error {
        ServiceError::Auth(error) => auth_error(error),
        ServiceError::Admission(error) => {
            let (code, retryable, user_action_required) = match error {
                CommandAdmissionError::InvalidRequest => ("command_admission_invalid", false, true),
                CommandAdmissionError::IdempotencyConflict => {
                    ("command_idempotency_conflict", false, true)
                }
                CommandAdmissionError::StorageUnavailable => {
                    ("command_admission_unavailable", true, false)
                }
                CommandAdmissionError::IntegrityFailure => {
                    ("command_admission_integrity", false, true)
                }
            };
            ErrorEnvelope {
                code: code.to_owned(),
                message_key: format!("local_ipc.{code}"),
                correlation_id: correlation_id.to_owned(),
                retryable,
                user_action_required,
                details_handle: String::new(),
                current_revision: None,
            }
        }
        ServiceError::InvalidRequest => ErrorEnvelope {
            code: "request_invalid".to_owned(),
            message_key: "local_ipc.request_invalid".to_owned(),
            correlation_id: correlation_id.to_owned(),
            retryable: false,
            user_action_required: true,
            details_handle: String::new(),
            current_revision: None,
        },
    }
}

fn internal_error(code: &str, message_key: &str) -> ErrorEnvelope {
    ErrorEnvelope {
        code: code.to_owned(),
        message_key: message_key.to_owned(),
        correlation_id: String::new(),
        retryable: true,
        user_action_required: false,
        details_handle: String::new(),
        current_revision: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::dennett::control::v1::system_service_server::SystemService;
    use crate::{HandshakePolicy, SessionRegistry};
    use dennett_agent_core::FakeAgentRuntime;
    use dennett_head::conversation::LocalProject;
    use dennett_head::draft::SessionOperationLocks;
    use dennett_head::session::SessionCoordinator;
    use dennett_head::system::{ProjectSummary, SystemMutation, SystemProjection};
    use dennett_memory_core::session::{InMemorySessionEventStore, SessionJournal};
    use dennett_sync_core::admission::InMemoryCommandAdmissionStore;
    use dennett_sync_core::draft::InMemoryDraftCache;
    use tonic::codegen::tokio_stream::StreamExt;

    #[cfg(windows)]
    use crate::{AuthenticatedSystemClient, ClientConfig};
    #[cfg(windows)]
    use std::time::Duration;
    #[cfg(windows)]
    use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};
    #[cfg(windows)]
    use tokio::sync::oneshot;

    fn peer() -> PeerIdentity {
        PeerIdentity::new(7, "S-1-test".to_owned(), "connection-1".to_owned())
    }

    fn request<T>(value: T) -> Request<T> {
        let mut request = Request::new(value);
        request.extensions_mut().insert(peer());
        request
    }

    #[test]
    fn send_turn_identity_and_validation_include_provider_controls() {
        let base = SendTurnRequest {
            command: None,
            project_id: "project".to_owned(),
            session_id: "session".to_owned(),
            text: "prompt".to_owned(),
            attachments: Vec::new(),
            runtime_controls: vec![
                crate::protocol::dennett::control::v1::RuntimeControlSelection {
                    control_id: "model".to_owned(),
                    choice_id: "gpt-new".to_owned(),
                },
            ],
            delivery_mode: WireTurnDeliveryMode::NewTurn as i32,
            expected_active_turn_id: String::new(),
        };
        assert_eq!(
            parse_runtime_controls(&base).expect("valid runtime controls"),
            vec![RuntimeControlSelection {
                control_id: "model".to_owned(),
                choice_id: "gpt-new".to_owned(),
            }]
        );
        let mut changed = base.clone();
        changed.runtime_controls[0].choice_id = "gpt-small".to_owned();
        assert_ne!(
            send_turn_intent_hash(&base, Some(3)).expect("base intent"),
            send_turn_intent_hash(&changed, Some(3)).expect("changed intent"),
        );
        let mut steered = base.clone();
        steered.delivery_mode = WireTurnDeliveryMode::SteerNow as i32;
        steered.expected_active_turn_id = "00000000-0000-7000-8000-000000000099".to_owned();
        assert_ne!(
            send_turn_intent_hash(&base, Some(3)).expect("new-turn intent"),
            send_turn_intent_hash(&steered, Some(3)).expect("steer intent"),
        );

        let mut duplicate = base;
        duplicate.runtime_controls.push(
            crate::protocol::dennett::control::v1::RuntimeControlSelection {
                control_id: "model".to_owned(),
                choice_id: "gpt-small".to_owned(),
            },
        );
        assert!(matches!(
            parse_runtime_controls(&duplicate),
            Err(ServiceError::InvalidRequest)
        ));
    }

    #[test]
    fn session_deltas_carry_the_canonical_event_time() {
        let session_id = SessionId::new();
        let command_id = CommandId::new();
        let user_turn_id = TurnId::new();
        let agent_turn_id = TurnId::new();
        let committed_at_unix_ms = 1_234_567_890;
        let mut commands = HashMap::new();
        let mut activity_created = HashMap::new();
        let delta = session_delta_to_wire(
            1,
            2,
            CommittedSessionEvent {
                event_id: dennett_contracts::SessionEventId::new(),
                session_id,
                revision: 2,
                payload_version: dennett_memory_core::session::SESSION_EVENT_PAYLOAD_VERSION,
                command_id: Some(command_id),
                body: SessionEventBody::TurnAccepted {
                    user_turn_id,
                    agent_turn_id,
                    command_id,
                    text: "hello".to_owned(),
                },
                committed_at_unix_ms,
            },
            &mut commands,
            &mut activity_created,
        );

        assert_eq!(
            delta.committed_at.as_ref().map(timestamp_unix_ms),
            Some(committed_at_unix_ms as i64)
        );
        let turn_times = delta
            .mutations
            .iter()
            .filter_map(|mutation| match mutation.mutation.as_ref() {
                Some(session_mutation::Mutation::UpsertTurn(turn)) => {
                    turn.created_at.as_ref().map(timestamp_unix_ms)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            turn_times,
            vec![committed_at_unix_ms as i64, committed_at_unix_ms as i64]
        );
        let turn_revisions = delta
            .mutations
            .iter()
            .filter_map(|mutation| match mutation.mutation.as_ref() {
                Some(session_mutation::Mutation::UpsertTurn(turn)) => Some(turn.created_revision),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(turn_revisions, vec![2, 2]);
    }

    #[test]
    fn activity_delta_keeps_its_first_causal_timestamp_across_updates() {
        let session_id = SessionId::new();
        let turn_id = TurnId::new();
        let mut commands = HashMap::new();
        let mut activity_created = HashMap::new();
        let event = |revision, status, committed_at_unix_ms| CommittedSessionEvent {
            event_id: dennett_contracts::SessionEventId::new(),
            session_id,
            revision,
            payload_version: dennett_memory_core::session::SESSION_EVENT_PAYLOAD_VERSION,
            command_id: None,
            body: SessionEventBody::AgentActivityUpserted {
                turn_id,
                activity_id: "commentary-1".to_owned(),
                phase: "commentary".to_owned(),
                message: Some("Working".to_owned()),
                status,
                native_extensions: Vec::new(),
            },
            committed_at_unix_ms,
        };
        let first = session_delta_to_wire(
            1,
            2,
            event(2, SessionActivityStatus::Started, 10),
            &mut commands,
            &mut activity_created,
        );
        let updated = session_delta_to_wire(
            2,
            3,
            event(3, SessionActivityStatus::Completed, 20),
            &mut commands,
            &mut activity_created,
        );
        fn activity(delta: &SessionDelta) -> &TurnActivitySnapshot {
            match delta.mutations[0].mutation.as_ref() {
                Some(session_mutation::Mutation::UpsertTurnActivity(update)) => {
                    update.activity.as_ref().expect("activity")
                }
                _ => panic!("activity mutation"),
            }
        }

        assert_eq!(
            activity(&first).created_at.as_ref().map(timestamp_unix_ms),
            Some(10)
        );
        assert_eq!(
            activity(&updated)
                .created_at
                .as_ref()
                .map(timestamp_unix_ms),
            Some(10)
        );
        assert_eq!(
            activity(&updated)
                .updated_at
                .as_ref()
                .map(timestamp_unix_ms),
            Some(20)
        );
        assert_eq!(activity(&first).created_revision, 2);
        assert_eq!(activity(&updated).created_revision, 2);
    }

    fn timestamp_unix_ms(value: &prost_types::Timestamp) -> i64 {
        value.seconds * 1_000 + i64::from(value.nanos) / 1_000_000
    }

    async fn session_service() -> (
        SessionServiceAdapter,
        Arc<SystemProjection>,
        ProjectId,
        String,
    ) {
        let project_id = ProjectId::new();
        let coordinator = SessionCoordinator::new(
            SessionJournal::new(Arc::new(InMemorySessionEventStore::default())),
            7,
            16,
        );
        let system = Arc::new(SystemProjection::new(SystemSnapshot::empty(7), 16));
        let drafts = ComposerDraftApplication::new(
            coordinator.clone(),
            Arc::new(InMemoryDraftCache::default()),
            SessionOperationLocks::default(),
        );
        let application = Arc::new(ConversationApplication::new(
            coordinator,
            system.clone(),
            Arc::new(FakeAgentRuntime),
            LocalProject {
                project_id,
                display_name: "Test project".to_owned(),
                workspace_path: "C:\\test-project".to_owned(),
                standalone_workspace_path: "C:\\test-scratch".to_owned(),
            },
        ));
        let registry = SessionRegistry::new(HandshakePolicy::m01("install", "node", 7));
        let welcome = registry
            .issue(
                &peer(),
                crate::protocol::dennett::control::v1::ClientHello {
                    client_component: "dennett-desktop-shell".to_owned(),
                    component_version: "0.1.0".to_owned(),
                    protocol_versions: vec![1],
                    installation_id: "install".to_owned(),
                    device_id: "device".to_owned(),
                    session_challenge: vec![7; 32],
                    requested_features: vec!["system-watch".to_owned()],
                },
            )
            .expect("handshake");
        registry
            .consume_bootstrap(&peer(), &welcome.client_session_id, &welcome.session_proof)
            .expect("bootstrap");
        (
            SessionServiceAdapter::new(
                application,
                drafts,
                registry,
                Arc::new(InMemoryCommandAdmissionStore::default()),
            ),
            system,
            project_id,
            welcome.client_session_id,
        )
    }

    fn command(
        command_id: CommandId,
        idempotency_key: &str,
        client_session_id: &str,
    ) -> CommandMetadata {
        CommandMetadata {
            idempotency_key: idempotency_key.to_owned(),
            command_id: command_id.0.to_string(),
            correlation_id: format!("correlation-{}", command_id.0),
            authority_epoch_seen: 7,
            created_at: Some(timestamp(1)),
            expected_revision: None,
            client_session_id: client_session_id.to_owned(),
        }
    }

    #[tokio::test]
    async fn concurrent_idempotency_collision_cannot_create_a_hidden_session() {
        let (service, system, project_id, client_session_id) = session_service().await;
        let first = service.clone();
        let second = service;
        let shared_key = "shared-idempotency-key";
        let first_request = request(CreateSessionRequest {
            command: Some(command(CommandId::new(), shared_key, &client_session_id)),
            project_id: project_id.0.to_string(),
            title: "First".to_owned(),
        });
        let second_request = request(CreateSessionRequest {
            command: Some(command(CommandId::new(), shared_key, &client_session_id)),
            project_id: project_id.0.to_string(),
            title: "Second".to_owned(),
        });

        let (first_response, second_response) = tokio::join!(
            first.create_session(first_request),
            second.create_session(second_request)
        );
        let outcomes = [
            first_response.expect("first response").into_inner().outcome,
            second_response
                .expect("second response")
                .into_inner()
                .outcome,
        ];
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| matches!(
                    outcome,
                    Some(create_session_response::Outcome::Accepted(_))
                ))
                .count(),
            1
        );
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| matches!(
                    outcome,
                    Some(create_session_response::Outcome::Error(ErrorEnvelope { code, .. }))
                        if code == "command_idempotency_conflict"
                ))
                .count(),
            1
        );
        assert_eq!(
            system
                .bootstrap()
                .await
                .expect("system snapshot")
                .recent_sessions
                .len(),
            1,
            "the rejected command must not create a hidden session"
        );
    }

    async fn authenticated(service: &SystemServiceAdapter<SystemProjection>) -> (String, Vec<u8>) {
        let response = service
            .handshake(request(HandshakeRequest {
                hello: Some(crate::protocol::dennett::control::v1::ClientHello {
                    client_component: "dennett-desktop-shell".to_owned(),
                    component_version: "0.1.0".to_owned(),
                    protocol_versions: vec![1],
                    installation_id: "install".to_owned(),
                    device_id: "device".to_owned(),
                    session_challenge: vec![9; 32],
                    requested_features: vec!["system-watch".to_owned()],
                }),
            }))
            .await
            .expect("handshake")
            .into_inner();
        match response.outcome.expect("outcome") {
            handshake_response::Outcome::Welcome(welcome) => {
                (welcome.client_session_id, welcome.session_proof)
            }
            handshake_response::Outcome::Error(error) => panic!("unexpected {error:?}"),
        }
    }

    #[tokio::test]
    async fn bootstrap_consumes_proof_and_watch_starts_with_snapshot_then_delta() {
        let projection = Arc::new(SystemProjection::new(SystemSnapshot::empty(7), 8));
        let service = SystemServiceAdapter::new(
            projection.clone(),
            SessionRegistry::new(HandshakePolicy::m01("install", "node", 7)),
        );
        let (session_id, proof) = authenticated(&service).await;
        let bootstrap = service
            .bootstrap(request(BootstrapRequest {
                session_proof: proof.clone(),
                known_revision: None,
                client_session_id: session_id.clone(),
            }))
            .await
            .expect("bootstrap")
            .into_inner();
        assert!(matches!(
            bootstrap.outcome,
            Some(bootstrap_response::Outcome::Snapshot(BootstrapSnapshot {
                revision: 1,
                authority_epoch: 7,
                ..
            }))
        ));
        let replay = service
            .bootstrap(request(BootstrapRequest {
                session_proof: proof,
                known_revision: None,
                client_session_id: session_id.clone(),
            }))
            .await
            .expect("replay response")
            .into_inner();
        assert!(matches!(
            replay.outcome,
            Some(bootstrap_response::Outcome::Error(ErrorEnvelope { ref code, .. }))
                if code == "ipc_proof_consumed"
        ));

        let mut stream = service
            .watch(request(WatchRequest {
                client_session_id: session_id,
                known_revision: Some(1),
            }))
            .await
            .expect("watch")
            .into_inner();
        let first = stream.as_mut().next().await.expect("snapshot").expect("ok");
        assert!(matches!(
            first.frame.and_then(|frame| frame.frame),
            Some(system_watch_frame::Frame::Snapshot(_))
        ));
        projection
            .apply(vec![SystemMutation::SetHealth(SystemHealth::Degraded)])
            .await;
        let second = stream.as_mut().next().await.expect("delta").expect("ok");
        assert!(matches!(
            second.frame.and_then(|frame| frame.frame),
            Some(system_watch_frame::Frame::Delta(WireSystemDelta {
                base_revision: 1,
                new_revision: 2,
                ..
            }))
        ));
    }

    #[cfg(windows)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn authenticated_named_pipe_round_trip_bootstraps_and_streams_a_delta() {
        let installation_id = format!("ipc-integration-{}", uuid::Uuid::now_v7());
        let endpoint = LocalEndpoint::for_installation(installation_id.clone()).expect("endpoint");
        let projection = Arc::new(SystemProjection::new(SystemSnapshot::empty(11), 8));
        let service = SystemServiceAdapter::new(
            projection.clone(),
            SessionRegistry::new(HandshakePolicy::m01(
                installation_id.clone(),
                "node-test",
                11,
            )),
        );
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let server = tokio::spawn(async move {
            run_system_server(endpoint, service, async move {
                let _ = shutdown_rx.await;
            })
            .await
        });

        let mut client = tokio::time::timeout(
            Duration::from_secs(5),
            AuthenticatedSystemClient::connect(ClientConfig::m01(
                installation_id,
                "desktop-test-device",
                "0.1.0-test",
            )),
        )
        .await
        .expect("client connection timed out")
        .expect("authenticated client");
        assert_eq!(client.bootstrap().revision, 1);
        assert_eq!(client.bootstrap().authority_epoch, 11);

        let mut watch = client.watch().await.expect("watch");
        let initial = tokio::time::timeout(Duration::from_secs(5), watch.message())
            .await
            .expect("initial watch frame timed out")
            .expect("initial watch status")
            .expect("initial watch frame");
        assert!(matches!(
            initial.frame.and_then(|frame| frame.frame),
            Some(system_watch_frame::Frame::Snapshot(WireSystemSnapshot {
                bootstrap: Some(BootstrapSnapshot {
                    revision: 1,
                    authority_epoch: 11,
                    ..
                }),
                ..
            }))
        ));

        projection
            .apply(vec![SystemMutation::SetHealth(SystemHealth::Degraded)])
            .await;
        let update = tokio::time::timeout(Duration::from_secs(5), watch.message())
            .await
            .expect("delta watch frame timed out")
            .expect("delta watch status")
            .expect("delta watch frame");
        assert!(matches!(
            update.frame.and_then(|frame| frame.frame),
            Some(system_watch_frame::Frame::Delta(WireSystemDelta {
                base_revision: 1,
                new_revision: 2,
                ..
            }))
        ));

        drop(watch);
        drop(client);
        shutdown_tx.send(()).expect("shutdown receiver");
        tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .expect("server shutdown timed out")
            .expect("server task")
            .expect("server result");
    }

    #[cfg(windows)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn snapshot_above_the_legacy_four_mib_limit_round_trips() {
        let installation_id = format!("ipc-large-snapshot-{}", uuid::Uuid::now_v7());
        let endpoint = LocalEndpoint::for_installation(installation_id.clone()).expect("endpoint");
        let projection = Arc::new(SystemProjection::new(SystemSnapshot::empty(19), 8));
        let large_display_name = "x".repeat(5 * 1024 * 1024);
        projection
            .apply(vec![SystemMutation::UpsertProject(ProjectSummary {
                project_id: "large-project".to_owned(),
                display_name: large_display_name.clone(),
                revision: 1,
            })])
            .await;
        let service = SystemServiceAdapter::new(
            projection,
            SessionRegistry::new(HandshakePolicy::m01(
                installation_id.clone(),
                "node-test",
                19,
            )),
        );
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let server = tokio::spawn(async move {
            run_system_server(endpoint, service, async move {
                let _ = shutdown_rx.await;
            })
            .await
        });

        let mut client = tokio::time::timeout(
            Duration::from_secs(10),
            AuthenticatedSystemClient::connect(ClientConfig::m01(
                installation_id,
                "desktop-large-snapshot",
                "0.1.0-test",
            )),
        )
        .await
        .expect("large bootstrap timed out")
        .expect("large bootstrap");
        assert_eq!(client.bootstrap().projects.len(), 1);
        assert_eq!(
            client.bootstrap().projects[0].display_name.len(),
            large_display_name.len()
        );

        let mut watch = client.watch().await.expect("large watch");
        let initial = tokio::time::timeout(Duration::from_secs(10), watch.message())
            .await
            .expect("large watch frame timed out")
            .expect("large watch status")
            .expect("large watch frame");
        let Some(system_watch_frame::Frame::Snapshot(snapshot)) =
            initial.frame.and_then(|frame| frame.frame)
        else {
            panic!("large watch must start with a snapshot");
        };
        assert_eq!(snapshot.bootstrap.expect("bootstrap").projects.len(), 1);

        drop(watch);
        drop(client);
        shutdown_tx.send(()).expect("shutdown receiver");
        tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .expect("server shutdown timed out")
            .expect("server task")
            .expect("server result");
    }

    #[cfg(windows)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn unauthenticated_pipe_clients_expire_without_ending_the_accept_loop() {
        let installation_id = format!("raw-pipe-timeout-{}", uuid::Uuid::now_v7());
        let endpoint = LocalEndpoint::for_installation(&installation_id).expect("endpoint");
        let projection = Arc::new(SystemProjection::new(SystemSnapshot::empty(7), 8));
        let sessions = SessionRegistry::new(HandshakePolicy::m01(&installation_id, "node", 7));
        let service = SystemServiceAdapter::new(projection, sessions);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let server_endpoint = endpoint.clone();
        let server = tokio::spawn(run_system_server(server_endpoint, service, async move {
            let _ = shutdown_rx.await;
        }));

        let mut raw_clients = Vec::new();
        for _ in 0..8 {
            raw_clients.push(open_raw_pipe(&endpoint).await);
        }
        tokio::time::sleep(Duration::from_millis(3_500)).await;

        let client = tokio::time::timeout(
            Duration::from_secs(10),
            AuthenticatedSystemClient::connect(ClientConfig::m01(
                &installation_id,
                "desktop-after-raw-clients",
                "test",
            )),
        )
        .await
        .expect("authenticated reconnect deadline")
        .expect("server accepts after pre-authentication timeouts");
        assert_eq!(client.bootstrap().authority_epoch, 7);
        drop(client);
        drop(raw_clients);
        shutdown_tx.send(()).expect("shutdown server");
        tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .expect("server shutdown timed out")
            .expect("server task")
            .expect("server result");
    }

    #[cfg(windows)]
    async fn open_raw_pipe(endpoint: &LocalEndpoint) -> NamedPipeClient {
        for _ in 0..400 {
            match ClientOptions::new().open(endpoint.pipe_name()) {
                Ok(pipe) => return pipe,
                Err(error)
                    if error.kind() == std::io::ErrorKind::NotFound
                        || error.raw_os_error()
                            == Some(windows_sys::Win32::Foundation::ERROR_PIPE_BUSY as i32) =>
                {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                Err(error) => panic!("open raw pipe: {error}"),
            }
        }
        panic!("raw pipe listener did not become available")
    }
}
