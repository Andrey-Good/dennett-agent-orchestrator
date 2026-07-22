#[cfg(windows)]
use crate::DEFAULT_MAX_MESSAGE_BYTES;
use crate::auth::AuthError;
use crate::protocol::dennett::common::v1::{
    CommandAccepted, CommandMetadata, ErrorEnvelope, StableRef,
};
use crate::protocol::dennett::control::v1::bootstrap_response;
use crate::protocol::dennett::control::v1::get_health_response;
use crate::protocol::dennett::control::v1::handshake_response;
use crate::protocol::dennett::control::v1::project_service_server::ProjectService;
#[cfg(windows)]
use crate::protocol::dennett::control::v1::project_service_server::ProjectServiceServer;
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
    CreateProjectAccepted, CreateProjectRequest, CreateProjectResponse, GetProjectRequest,
    GetProjectResponse, InspectProjectLocationRequest, InspectProjectLocationResponse,
    ListProjectsRequest, ListProjectsResponse, ListProjectsResult,
    PortableMetadataAction as WirePortableMetadataAction,
    PortableProjectMetadata as WirePortableProjectMetadata,
    PortableProjectMetadataState as WirePortableProjectMetadataState, Project as WireProject,
    ProjectLocationInspection as WireProjectLocationInspection,
    ProjectRegistrationKind as WireProjectRegistrationKind,
    ProjectSourceFeature as WireProjectSourceFeature, ProjectTrustState as WireProjectTrustState,
    RebindPortableMetadataAction as WireRebindAction, RebindProjectWorkspaceAccepted,
    RebindProjectWorkspaceRequest, RebindProjectWorkspaceResponse, RegisterProjectAccepted,
    RegisterProjectRequest, RegisterProjectResponse, SetProjectTrustRequest,
    SetProjectTrustResponse, SharedProjectMemoryState as WireSharedProjectMemoryState,
    WorkspaceFailure, WorkspaceFailureKind, create_project_response, get_project_response,
    inspect_project_location_response, list_projects_response, rebind_project_workspace_response,
    register_project_response, set_project_trust_response,
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
use crate::{
    LocalEndpoint, PROJECT_TRUST_DECISION_REF_KIND, PeerIdentity, SessionRegistry, TransportError,
};
use dennett_agent_core::{RuntimeControlSelection, RuntimeDescriptor, RuntimeKind};
use dennett_contracts::{
    CommandId, PortableMetadataAction, PortableProjectMetadataState, ProjectId,
    ProjectInspectionId, ProjectTrustState as DomainProjectTrustState,
    RebindPortableMetadataAction, SessionId, TurnId, WorkspaceBindingId,
};
use dennett_head::conversation::{
    ConversationApplication, ConversationError, ConversationTurnRequest, TraceContext,
    TurnDeliveryMode,
};
use dennett_head::draft::{ComposerDraftApplication, DraftApplicationError, DraftSaveOutcome};
use dennett_head::project::{
    InspectProjectLocationCommand, ProjectApplication, ProjectApplicationError,
    ProjectLocationError, RebindProjectCommand, RegisterProjectCommand, SetProjectTrustCommand,
};
use dennett_head::system::{
    ProjectState as DomainProjectState, ProjectSummary, SessionSummary, SystemDelta, SystemHealth,
    SystemMutation, SystemSnapshot, SystemStateError, SystemStatePort,
    SystemWatchFrame as DomainWatchFrame,
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
use dennett_trust_core::project_registry::{
    ProjectAggregate, ProjectLocationInspection, ProjectRegistrationKind, ProjectRegistryError,
    ProjectTrustDecisionError, SharedProjectMemoryState, TrustDecisionRef, WorkspaceAccessMode,
    WorkspaceAvailability, WorkspaceKind,
};
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

const DEFAULT_PROJECT_PAGE_SIZE: usize = 50;
const MAX_PROJECT_PAGE_SIZE: usize = 200;
const PROJECT_INSPECTION_TTL_MS: u64 = 5 * 60 * 1_000;
const PROJECT_PAGE_TOKEN_VERSION: &str = "p1";

#[derive(Clone)]
pub struct ProjectServiceAdapter {
    application: Arc<ProjectApplication>,
    sessions: SessionRegistry,
    admissions: Arc<dyn CommandAdmissionPort>,
}

impl ProjectServiceAdapter {
    #[must_use]
    pub fn new(
        application: Arc<ProjectApplication>,
        sessions: SessionRegistry,
        admissions: Arc<dyn CommandAdmissionPort>,
    ) -> Self {
        Self {
            application,
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

    fn authenticated_command<T>(
        &self,
        request: &Request<T>,
        command: Option<&CommandMetadata>,
    ) -> Result<(CommandMetadata, CommandId), ProjectTransportError> {
        let command = command_from(command)?;
        let authenticated = self.authenticate(request, &command.client_session_id)?;
        if command.authority_epoch_seen != authenticated.authority_epoch {
            return Err(AuthError::AuthorityEpochChanged.into());
        }
        let command_id = CommandId(parse_uuid(&command.command_id)?);
        Ok((command, command_id))
    }

    async fn accept(
        &self,
        command: &CommandMetadata,
        command_id: CommandId,
        operation_kind: &str,
        intent_hash: [u8; 32],
    ) -> Result<CommandAccepted, ProjectTransportError> {
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

    #[allow(deprecated)]
    async fn create_project_inner(
        &self,
        request: Request<CreateProjectRequest>,
    ) -> Result<CreateProjectAccepted, ProjectTransportError> {
        let (command, command_id) =
            self.authenticated_command(&request, request.get_ref().command.as_ref())?;
        let body = request.into_inner();
        let display_name = validated_nonempty(body.display_name)?;
        let root_uri = validated_nonempty(body.root_uri)?;
        let intent_hash = command_intent_hash(
            "create_project_legacy",
            command.expected_revision,
            &[
                display_name.as_str(),
                root_uri.as_str(),
                "leave_absent",
                "restricted",
            ],
        );
        let observed_at_unix_ms = unix_time_ms();
        let inspection = self
            .application
            .inspect_location(InspectProjectLocationCommand {
                registration_kind: ProjectRegistrationKind::CreateEmpty,
                root_uri,
                observed_at_unix_ms,
                expires_at_unix_ms: observed_at_unix_ms.saturating_add(PROJECT_INSPECTION_TTL_MS),
            })
            .await?;
        if (inspection.location_exists && !inspection.location_empty)
            || inspection.portable_metadata_state != PortableProjectMetadataState::Absent
        {
            return Err(ProjectApplicationError::InvalidRequest.into());
        }
        let mut accepted = self
            .accept(&command, command_id, "create_project_legacy", intent_hash)
            .await?;
        let registered = self
            .application
            .register_project(RegisterProjectCommand {
                command_id,
                correlation_id: command.correlation_id,
                intent_sha256: intent_hash,
                inspection_id: inspection.inspection_id,
                display_name,
                portable_metadata_action: PortableMetadataAction::LeaveAbsent,
                initial_trust_state: None,
                trust_decision: None,
                committed_at_unix_ms: unix_time_ms(),
            })
            .await?;
        accepted.operation_id = registered.operation.plan.operation_id.0.to_string();
        Ok(CreateProjectAccepted {
            command: Some(accepted),
            project_id: registered.project.project.project_id.0.to_string(),
        })
    }

    async fn list_projects_inner(
        &self,
        request: Request<ListProjectsRequest>,
    ) -> Result<ListProjectsResult, ProjectTransportError> {
        self.authenticate(&request, &request.get_ref().client_session_id)?;
        let body = request.into_inner();
        let page_size = normalized_project_page_size(body.page_size)?;
        let projects = self.application.list_projects().await?;
        let snapshot_revision = project_list_snapshot_revision(&projects);
        let offset = decode_project_page_token(&body.page_token, snapshot_revision)?;
        if offset > projects.len() {
            return Err(ProjectTransportError::InvalidRequest);
        }
        let end = offset.saturating_add(page_size).min(projects.len());
        let next_page_token = if end < projects.len() {
            encode_project_page_token(snapshot_revision, end)
        } else {
            String::new()
        };
        Ok(ListProjectsResult {
            projects: projects[offset..end]
                .iter()
                .map(project_summary_to_wire)
                .collect(),
            next_page_token,
            snapshot_revision,
        })
    }

    async fn get_project_inner(
        &self,
        request: Request<GetProjectRequest>,
    ) -> Result<WireProject, ProjectTransportError> {
        self.authenticate(&request, &request.get_ref().client_session_id)?;
        let body = request.into_inner();
        let project = self
            .application
            .get_project(parse_project_id(&body.project_id)?)
            .await?;
        Ok(project_to_project_wire(&project))
    }

    async fn inspect_project_location_inner(
        &self,
        request: Request<InspectProjectLocationRequest>,
    ) -> Result<WireProjectLocationInspection, ProjectTransportError> {
        self.authenticate(&request, &request.get_ref().client_session_id)?;
        let body = request.into_inner();
        let registration_kind = project_registration_kind_from_wire(body.registration_kind)?;
        let root_uri = validated_nonempty(body.root_uri)?;
        let observed_at_unix_ms = unix_time_ms();
        let inspection = self
            .application
            .inspect_location(InspectProjectLocationCommand {
                registration_kind,
                root_uri,
                observed_at_unix_ms,
                expires_at_unix_ms: observed_at_unix_ms.saturating_add(PROJECT_INSPECTION_TTL_MS),
            })
            .await?;
        Ok(project_inspection_to_wire(&inspection))
    }

    async fn register_project_inner(
        &self,
        request: Request<RegisterProjectRequest>,
    ) -> Result<RegisterProjectAccepted, ProjectTransportError> {
        let (command, command_id) =
            self.authenticated_command(&request, request.get_ref().command.as_ref())?;
        let body = request.into_inner();
        let inspection_id = ProjectInspectionId(parse_uuid(&body.inspection_id)?);
        let display_name = validated_nonempty(body.display_name)?;
        let portable_metadata_action =
            portable_metadata_action_from_wire(body.portable_metadata_action)?;
        let initial_trust_state = initial_project_trust_from_wire(body.initial_trust_state)?;
        let trust_decision = optional_bridge_trust_decision(body.trust_decision, command_id)?;
        validate_initial_trust_request(initial_trust_state, trust_decision.as_ref())?;

        let action = body.portable_metadata_action.to_string();
        let trust_state = body.initial_trust_state.to_string();
        let (decision_presence, decision_kind, decision_id) =
            trust_ref_intent_fields(trust_decision.as_ref());
        let intent_hash = command_intent_hash(
            "register_project",
            command.expected_revision,
            &[
                body.inspection_id.as_str(),
                display_name.as_str(),
                action.as_str(),
                trust_state.as_str(),
                decision_presence,
                decision_kind,
                decision_id,
            ],
        );
        let mut accepted = self
            .accept(&command, command_id, "register_project", intent_hash)
            .await?;
        let registered = self
            .application
            .register_project(RegisterProjectCommand {
                command_id,
                correlation_id: command.correlation_id,
                intent_sha256: intent_hash,
                inspection_id,
                display_name,
                portable_metadata_action,
                initial_trust_state,
                trust_decision,
                committed_at_unix_ms: unix_time_ms(),
            })
            .await?;
        accepted.operation_id = registered.operation.plan.operation_id.0.to_string();
        Ok(RegisterProjectAccepted {
            command: Some(accepted),
            project_id: registered.project.project.project_id.0.to_string(),
            workspace_binding_id: registered.operation.plan.binding_id.0.to_string(),
        })
    }

    async fn rebind_project_workspace_inner(
        &self,
        request: Request<RebindProjectWorkspaceRequest>,
    ) -> Result<RebindProjectWorkspaceAccepted, ProjectTransportError> {
        let (command, command_id) =
            self.authenticated_command(&request, request.get_ref().command.as_ref())?;
        let body = request.into_inner();
        let project_id = parse_project_id(&body.project_id)?;
        let current_binding_id =
            WorkspaceBindingId(parse_uuid(&body.current_workspace_binding_id)?);
        let inspection_id = ProjectInspectionId(parse_uuid(&body.inspection_id)?);
        let portable_metadata_action = rebind_action_from_wire(body.portable_metadata_action)?;
        let action = body.portable_metadata_action.to_string();
        let intent_hash = command_intent_hash(
            "rebind_project_workspace",
            command.expected_revision,
            &[
                body.project_id.as_str(),
                body.current_workspace_binding_id.as_str(),
                body.inspection_id.as_str(),
                action.as_str(),
            ],
        );
        let accepted = self
            .accept(
                &command,
                command_id,
                "rebind_project_workspace",
                intent_hash,
            )
            .await?;
        let receipt = self
            .application
            .rebind_project(RebindProjectCommand {
                command_id,
                correlation_id: command.correlation_id,
                intent_sha256: intent_hash,
                project_id,
                current_binding_id,
                inspection_id,
                portable_metadata_action,
                committed_at_unix_ms: unix_time_ms(),
            })
            .await?;
        Ok(RebindProjectWorkspaceAccepted {
            command: Some(accepted),
            project_id: receipt.project_id.0.to_string(),
            workspace_binding_id: receipt.primary_binding.binding_id.0.to_string(),
        })
    }

    async fn set_project_trust_inner(
        &self,
        request: Request<SetProjectTrustRequest>,
    ) -> Result<CommandAccepted, ProjectTransportError> {
        let (command, command_id) =
            self.authenticated_command(&request, request.get_ref().command.as_ref())?;
        let body = request.into_inner();
        let project_id = parse_project_id(&body.project_id)?;
        let target_state = explicit_project_trust_from_wire(body.trust_state)?;
        if body.expected_policy_revision == 0
            || command
                .expected_revision
                .is_some_and(|revision| revision != body.expected_policy_revision)
        {
            return Err(ProjectTransportError::InvalidRequest);
        }
        let trust_decision = bridge_trust_decision(body.trust_decision, command_id)?;
        let target = body.trust_state.to_string();
        let expected = body.expected_policy_revision.to_string();
        let intent_hash = command_intent_hash(
            "set_project_trust",
            command.expected_revision,
            &[
                body.project_id.as_str(),
                target.as_str(),
                expected.as_str(),
                trust_decision.kind.as_str(),
                trust_decision.id.as_str(),
            ],
        );
        let accepted = self
            .accept(&command, command_id, "set_project_trust", intent_hash)
            .await?;

        // Project policy keeps the bridge decision durably. This makes a retry
        // after a committed update but lost IPC response idempotent even though
        // the policy API itself is an exact compare-and-set operation.
        let project = self.application.get_project(project_id).await?;
        let already_committed = body
            .expected_policy_revision
            .checked_add(1)
            .is_some_and(|revision| project.access_policy.revision == revision)
            && project.access_policy.trust_state == target_state
            && project.access_policy.last_decision.as_ref() == Some(&trust_decision);
        if !already_committed {
            self.application
                .set_project_trust(SetProjectTrustCommand {
                    command_id,
                    correlation_id: command.correlation_id,
                    project_id,
                    target_state,
                    expected_policy_revision: body.expected_policy_revision,
                    trust_decision,
                    committed_at_unix_ms: unix_time_ms(),
                })
                .await?;
        }
        Ok(accepted)
    }
}

#[tonic::async_trait]
impl ProjectService for ProjectServiceAdapter {
    async fn create_project(
        &self,
        request: Request<CreateProjectRequest>,
    ) -> Result<Response<CreateProjectResponse>, Status> {
        let correlation = request
            .get_ref()
            .command
            .as_ref()
            .map_or("", |command| command.correlation_id.as_str())
            .to_owned();
        let outcome = match self.create_project_inner(request).await {
            Ok(accepted) => create_project_response::Outcome::Accepted(accepted),
            Err(error) => create_project_response::Outcome::Error(
                project_transport_failure(error, &correlation)
                    .error
                    .unwrap_or_else(|| internal_error("project_failure", "project.failure")),
            ),
        };
        Ok(Response::new(CreateProjectResponse {
            outcome: Some(outcome),
        }))
    }

    async fn list_projects(
        &self,
        request: Request<ListProjectsRequest>,
    ) -> Result<Response<ListProjectsResponse>, Status> {
        let outcome = match self.list_projects_inner(request).await {
            Ok(result) => list_projects_response::Outcome::Result(result),
            Err(error) => list_projects_response::Outcome::Error(
                project_transport_failure(error, "")
                    .error
                    .unwrap_or_else(|| internal_error("project_failure", "project.failure")),
            ),
        };
        Ok(Response::new(ListProjectsResponse {
            outcome: Some(outcome),
        }))
    }

    async fn get_project(
        &self,
        request: Request<GetProjectRequest>,
    ) -> Result<Response<GetProjectResponse>, Status> {
        let outcome = match self.get_project_inner(request).await {
            Ok(project) => get_project_response::Outcome::Project(project),
            Err(error) => get_project_response::Outcome::Error(
                project_transport_failure(error, "")
                    .error
                    .unwrap_or_else(|| internal_error("project_failure", "project.failure")),
            ),
        };
        Ok(Response::new(GetProjectResponse {
            outcome: Some(outcome),
        }))
    }

    async fn inspect_project_location(
        &self,
        request: Request<InspectProjectLocationRequest>,
    ) -> Result<Response<InspectProjectLocationResponse>, Status> {
        let outcome = match self.inspect_project_location_inner(request).await {
            Ok(inspection) => inspect_project_location_response::Outcome::Inspection(inspection),
            Err(error) => inspect_project_location_response::Outcome::Error(
                project_transport_failure(error, ""),
            ),
        };
        Ok(Response::new(InspectProjectLocationResponse {
            outcome: Some(outcome),
        }))
    }

    async fn register_project(
        &self,
        request: Request<RegisterProjectRequest>,
    ) -> Result<Response<RegisterProjectResponse>, Status> {
        let correlation = request
            .get_ref()
            .command
            .as_ref()
            .map_or("", |command| command.correlation_id.as_str())
            .to_owned();
        let outcome = match self.register_project_inner(request).await {
            Ok(accepted) => register_project_response::Outcome::Accepted(accepted),
            Err(error) => register_project_response::Outcome::Error(project_transport_failure(
                error,
                &correlation,
            )),
        };
        Ok(Response::new(RegisterProjectResponse {
            outcome: Some(outcome),
        }))
    }

    async fn rebind_project_workspace(
        &self,
        request: Request<RebindProjectWorkspaceRequest>,
    ) -> Result<Response<RebindProjectWorkspaceResponse>, Status> {
        let correlation = request
            .get_ref()
            .command
            .as_ref()
            .map_or("", |command| command.correlation_id.as_str())
            .to_owned();
        let outcome = match self.rebind_project_workspace_inner(request).await {
            Ok(accepted) => rebind_project_workspace_response::Outcome::Accepted(accepted),
            Err(error) => rebind_project_workspace_response::Outcome::Error(
                project_transport_failure(error, &correlation),
            ),
        };
        Ok(Response::new(RebindProjectWorkspaceResponse {
            outcome: Some(outcome),
        }))
    }

    async fn set_project_trust(
        &self,
        request: Request<SetProjectTrustRequest>,
    ) -> Result<Response<SetProjectTrustResponse>, Status> {
        let correlation = request
            .get_ref()
            .command
            .as_ref()
            .map_or("", |command| command.correlation_id.as_str())
            .to_owned();
        let outcome = match self.set_project_trust_inner(request).await {
            Ok(accepted) => set_project_trust_response::Outcome::Accepted(accepted),
            Err(error) => set_project_trust_response::Outcome::Error(project_transport_failure(
                error,
                &correlation,
            )),
        };
        Ok(Response::new(SetProjectTrustResponse {
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
    projects: ProjectServiceAdapter,
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
        .add_service(
            ProjectServiceServer::new(projects)
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
    _projects: ProjectServiceAdapter,
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
        state: match project.state {
            DomainProjectState::Ready => ProjectState::Ready,
            DomainProjectState::Missing => ProjectState::Missing,
            DomainProjectState::Detached => ProjectState::Detached,
            DomainProjectState::ReadOnly => ProjectState::ReadOnly,
        } as i32,
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

#[derive(Debug)]
enum ProjectTransportError {
    Auth(AuthError),
    Admission(CommandAdmissionError),
    Application(ProjectApplicationError),
    InvalidRequest,
    PageSnapshotChanged,
}

impl From<AuthError> for ProjectTransportError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl From<CommandAdmissionError> for ProjectTransportError {
    fn from(error: CommandAdmissionError) -> Self {
        Self::Admission(error)
    }
}

impl From<ProjectApplicationError> for ProjectTransportError {
    fn from(error: ProjectApplicationError) -> Self {
        Self::Application(error)
    }
}

impl From<ServiceError> for ProjectTransportError {
    fn from(error: ServiceError) -> Self {
        match error {
            ServiceError::Auth(error) => Self::Auth(error),
            ServiceError::Admission(error) => Self::Admission(error),
            ServiceError::InvalidRequest => Self::InvalidRequest,
        }
    }
}

fn validated_nonempty(value: String) -> Result<String, ProjectTransportError> {
    if value.trim().is_empty() || value.len() > 32 * 1_024 || value.contains('\0') {
        Err(ProjectTransportError::InvalidRequest)
    } else {
        Ok(value)
    }
}

fn project_registration_kind_from_wire(
    value: i32,
) -> Result<ProjectRegistrationKind, ProjectTransportError> {
    match WireProjectRegistrationKind::try_from(value)
        .map_err(|_| ProjectTransportError::InvalidRequest)?
    {
        WireProjectRegistrationKind::CreateEmpty => Ok(ProjectRegistrationKind::CreateEmpty),
        WireProjectRegistrationKind::AttachExisting => Ok(ProjectRegistrationKind::AttachExisting),
        WireProjectRegistrationKind::Unspecified => Err(ProjectTransportError::InvalidRequest),
    }
}

fn portable_metadata_action_from_wire(
    value: i32,
) -> Result<PortableMetadataAction, ProjectTransportError> {
    match WirePortableMetadataAction::try_from(value)
        .map_err(|_| ProjectTransportError::InvalidRequest)?
    {
        WirePortableMetadataAction::LeaveAbsent => Ok(PortableMetadataAction::LeaveAbsent),
        WirePortableMetadataAction::UseExisting => Ok(PortableMetadataAction::UseExisting),
        WirePortableMetadataAction::CreateMinimal => Ok(PortableMetadataAction::CreateMinimal),
        WirePortableMetadataAction::ForkWithNewIdentity => {
            Ok(PortableMetadataAction::ForkWithNewIdentity)
        }
        WirePortableMetadataAction::Unspecified => Err(ProjectTransportError::InvalidRequest),
    }
}

fn rebind_action_from_wire(
    value: i32,
) -> Result<RebindPortableMetadataAction, ProjectTransportError> {
    match WireRebindAction::try_from(value).map_err(|_| ProjectTransportError::InvalidRequest)? {
        WireRebindAction::LeaveAbsent => Ok(RebindPortableMetadataAction::LeaveAbsent),
        WireRebindAction::UseExisting => Ok(RebindPortableMetadataAction::UseExisting),
        WireRebindAction::CreateMinimal => Ok(RebindPortableMetadataAction::CreateMinimal),
        WireRebindAction::Unspecified => Err(ProjectTransportError::InvalidRequest),
    }
}

fn initial_project_trust_from_wire(
    value: i32,
) -> Result<Option<DomainProjectTrustState>, ProjectTransportError> {
    match WireProjectTrustState::try_from(value)
        .map_err(|_| ProjectTransportError::InvalidRequest)?
    {
        WireProjectTrustState::Unspecified => Ok(None),
        WireProjectTrustState::Restricted => Ok(Some(DomainProjectTrustState::Restricted)),
        WireProjectTrustState::TrustedBounded => Ok(Some(DomainProjectTrustState::TrustedBounded)),
        WireProjectTrustState::Revoked => Ok(Some(DomainProjectTrustState::Revoked)),
    }
}

fn explicit_project_trust_from_wire(
    value: i32,
) -> Result<DomainProjectTrustState, ProjectTransportError> {
    initial_project_trust_from_wire(value)?.ok_or(ProjectTransportError::InvalidRequest)
}

fn optional_bridge_trust_decision(
    value: Option<StableRef>,
    command_id: CommandId,
) -> Result<Option<TrustDecisionRef>, ProjectTransportError> {
    value
        .map(|value| validate_bridge_trust_decision(value, command_id))
        .transpose()
}

fn bridge_trust_decision(
    value: Option<StableRef>,
    command_id: CommandId,
) -> Result<TrustDecisionRef, ProjectTransportError> {
    validate_bridge_trust_decision(
        value.ok_or(ProjectTransportError::Application(
            ProjectApplicationError::TrustDecisionMissing,
        ))?,
        command_id,
    )
}

fn validate_bridge_trust_decision(
    value: StableRef,
    command_id: CommandId,
) -> Result<TrustDecisionRef, ProjectTransportError> {
    let decision = TrustDecisionRef::new(value.kind, value.id)
        .map_err(ProjectApplicationError::TrustDecision)?;
    if decision.kind != PROJECT_TRUST_DECISION_REF_KIND || decision.id != command_id.0.to_string() {
        let error = if decision.kind != PROJECT_TRUST_DECISION_REF_KIND {
            ProjectTrustDecisionError::InvalidKind
        } else {
            ProjectTrustDecisionError::CommandMismatch
        };
        return Err(ProjectApplicationError::TrustDecision(error).into());
    }
    Ok(decision)
}

fn validate_initial_trust_request(
    state: Option<DomainProjectTrustState>,
    decision: Option<&TrustDecisionRef>,
) -> Result<(), ProjectTransportError> {
    match (state, decision) {
        (Some(DomainProjectTrustState::TrustedBounded), Some(_))
        | (None | Some(DomainProjectTrustState::Restricted), None) => Ok(()),
        (Some(DomainProjectTrustState::Revoked), _)
        | (Some(DomainProjectTrustState::TrustedBounded), None)
        | (None | Some(DomainProjectTrustState::Restricted), Some(_)) => {
            Err(ProjectTransportError::InvalidRequest)
        }
    }
}

fn trust_ref_intent_fields(decision: Option<&TrustDecisionRef>) -> (&str, &str, &str) {
    decision.map_or(("absent", "", ""), |decision| {
        ("present", decision.kind.as_str(), decision.id.as_str())
    })
}

fn normalized_project_page_size(value: u32) -> Result<usize, ProjectTransportError> {
    if value == 0 {
        return Ok(DEFAULT_PROJECT_PAGE_SIZE);
    }
    let value = usize::try_from(value).map_err(|_| ProjectTransportError::InvalidRequest)?;
    if value > MAX_PROJECT_PAGE_SIZE {
        Err(ProjectTransportError::InvalidRequest)
    } else {
        Ok(value)
    }
}

fn project_list_snapshot_revision(projects: &[ProjectAggregate]) -> u64 {
    fn field(hasher: &mut Sha256, value: &[u8]) {
        hasher.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
        hasher.update(value);
    }

    let mut hasher = Sha256::new();
    hasher.update(b"dennett.project-list-snapshot.v1\0");
    hasher.update(
        u64::try_from(projects.len())
            .unwrap_or(u64::MAX)
            .to_be_bytes(),
    );
    for aggregate in projects {
        field(
            &mut hasher,
            aggregate.project.project_id.0.to_string().as_bytes(),
        );
        hasher.update(aggregate.project.revision.to_be_bytes());
        hasher.update(aggregate.access_policy.revision.to_be_bytes());
        for binding in &aggregate.bindings {
            field(&mut hasher, binding.binding_id.0.to_string().as_bytes());
            hasher.update(binding.record_revision.to_be_bytes());
        }
    }
    let digest = hasher.finalize();
    let mut revision = u64::from_be_bytes(digest[..8].try_into().expect("SHA-256 prefix"));
    if revision == 0 {
        revision = 1;
    }
    revision
}

fn encode_project_page_token(snapshot_revision: u64, offset: usize) -> String {
    format!("{PROJECT_PAGE_TOKEN_VERSION}:{snapshot_revision:016x}:{offset}")
}

fn decode_project_page_token(
    value: &str,
    snapshot_revision: u64,
) -> Result<usize, ProjectTransportError> {
    if value.is_empty() {
        return Ok(0);
    }
    if value.len() > 128 {
        return Err(ProjectTransportError::InvalidRequest);
    }
    let mut parts = value.split(':');
    let version = parts.next();
    let revision = parts.next();
    let offset = parts.next();
    if version != Some(PROJECT_PAGE_TOKEN_VERSION) || parts.next().is_some() {
        return Err(ProjectTransportError::InvalidRequest);
    }
    let revision = u64::from_str_radix(revision.ok_or(ProjectTransportError::InvalidRequest)?, 16)
        .map_err(|_| ProjectTransportError::InvalidRequest)?;
    if revision != snapshot_revision {
        return Err(ProjectTransportError::PageSnapshotChanged);
    }
    offset
        .ok_or(ProjectTransportError::InvalidRequest)?
        .parse::<usize>()
        .map_err(|_| ProjectTransportError::InvalidRequest)
}

fn project_summary_to_wire(project: &ProjectAggregate) -> WireProjectSummary {
    WireProjectSummary {
        project_id: project.project.project_id.0.to_string(),
        display_name: project.project.display_name.clone(),
        state: project_state_to_wire(project),
        revision: project.project.revision,
        last_activity_at: Some(timestamp(project.project.updated_at_unix_ms)),
    }
}

#[allow(deprecated)]
fn project_to_project_wire(project: &ProjectAggregate) -> WireProject {
    let primary = primary_binding(project);
    WireProject {
        project_id: project.project.project_id.0.to_string(),
        display_name: project.project.display_name.clone(),
        root_uri: primary.map_or_else(String::new, |binding| {
            binding.location.path.expose_local().to_owned()
        }),
        state: project_state_to_wire(project),
        revision: project.project.revision,
        created_at: Some(timestamp(project.project.created_at_unix_ms)),
        updated_at: Some(timestamp(project.project.updated_at_unix_ms)),
        primary_workspace_binding_id: project.project.primary_binding_id.0.to_string(),
    }
}

fn primary_binding(
    project: &ProjectAggregate,
) -> Option<&dennett_trust_core::project_registry::WorkspaceBinding> {
    project
        .bindings
        .iter()
        .find(|binding| binding.binding_id == project.project.primary_binding_id)
}

fn project_state_to_wire(project: &ProjectAggregate) -> i32 {
    let state = match primary_binding(project) {
        Some(binding) if binding.availability == WorkspaceAvailability::Missing => {
            ProjectState::Missing
        }
        Some(binding) if binding.availability == WorkspaceAvailability::Detached => {
            ProjectState::Detached
        }
        Some(binding)
            if binding.availability == WorkspaceAvailability::Inaccessible
                || binding.access_mode == WorkspaceAccessMode::ReadOnly =>
        {
            ProjectState::ReadOnly
        }
        Some(_) => ProjectState::Ready,
        None => ProjectState::Detached,
    };
    state as i32
}

fn project_inspection_to_wire(
    inspection: &ProjectLocationInspection,
) -> WireProjectLocationInspection {
    let mut detected_features = Vec::with_capacity(4);
    if matches!(
        inspection.workspace_kind,
        WorkspaceKind::VersionedCheckout | WorkspaceKind::IsolatedCheckout
    ) {
        detected_features.push(WireProjectSourceFeature::VersionedRepository as i32);
    }
    if inspection.instruction_source_count != 0 {
        detected_features.push(WireProjectSourceFeature::InstructionFiles as i32);
    }
    if inspection.portable_metadata_state != PortableProjectMetadataState::Absent {
        detected_features.push(WireProjectSourceFeature::PortableProjectMetadata as i32);
    }
    if inspection.shared_memory_state != SharedProjectMemoryState::Absent {
        detected_features.push(WireProjectSourceFeature::SharedProjectMemory as i32);
    }
    let location_identity = inspection
        .source_identity
        .map(|identity| ("workspace_source_identity", identity))
        .or_else(|| {
            inspection
                .prospective_parent_identity
                .map(|identity| ("workspace_parent_identity", identity))
        })
        .map(|(kind, identity)| StableRef {
            kind: kind.to_owned(),
            id: hex(identity.as_bytes()),
        });
    WireProjectLocationInspection {
        inspection_id: inspection.inspection_id.0.to_string(),
        registration_kind: match inspection.registration_kind {
            ProjectRegistrationKind::CreateEmpty => WireProjectRegistrationKind::CreateEmpty,
            ProjectRegistrationKind::AttachExisting => WireProjectRegistrationKind::AttachExisting,
        } as i32,
        root_uri: inspection.location.path.expose_local().to_owned(),
        suggested_display_name: inspection.suggested_display_name.clone(),
        location_exists: inspection.location_exists,
        location_empty: inspection.location_empty,
        portable_metadata: Some(WirePortableProjectMetadata {
            state: portable_metadata_state_to_wire(inspection.portable_metadata_state),
            project_id: inspection
                .portable_project_id
                .map_or_else(String::new, |project_id| project_id.0.to_string()),
            schema_version: if inspection.portable_metadata_state
                == PortableProjectMetadataState::PresentValid
            {
                "1".to_owned()
            } else {
                String::new()
            },
            shared_memory_state: shared_memory_state_to_wire(inspection.shared_memory_state),
            minimal_structure_creation_available: inspection.minimal_structure_creation_available,
        }),
        detected_features,
        location_identity,
        observed_at: Some(timestamp(inspection.observed_at_unix_ms)),
        expires_at: Some(timestamp(inspection.expires_at_unix_ms)),
    }
}

fn portable_metadata_state_to_wire(state: PortableProjectMetadataState) -> i32 {
    (match state {
        PortableProjectMetadataState::Absent => WirePortableProjectMetadataState::Absent,
        PortableProjectMetadataState::PresentValid => {
            WirePortableProjectMetadataState::PresentValid
        }
        PortableProjectMetadataState::Invalid => WirePortableProjectMetadataState::Invalid,
        PortableProjectMetadataState::IdentityConflict => {
            WirePortableProjectMetadataState::IdentityConflict
        }
        PortableProjectMetadataState::UnsupportedVersion => {
            WirePortableProjectMetadataState::UnsupportedVersion
        }
    }) as i32
}

fn shared_memory_state_to_wire(state: SharedProjectMemoryState) -> i32 {
    (match state {
        SharedProjectMemoryState::Absent => WireSharedProjectMemoryState::Absent,
        SharedProjectMemoryState::Present => WireSharedProjectMemoryState::Present,
        SharedProjectMemoryState::Invalid => WireSharedProjectMemoryState::Invalid,
    }) as i32
}

fn hex(value: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(value.len() * 2);
    for byte in value {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn project_transport_failure(
    error: ProjectTransportError,
    correlation_id: &str,
) -> WorkspaceFailure {
    match error {
        ProjectTransportError::Auth(error) => {
            let kind = if error == AuthError::AuthorityEpochChanged {
                WorkspaceFailureKind::StaleSnapshot
            } else {
                WorkspaceFailureKind::ScopeDenied
            };
            let mut envelope = auth_error(error);
            envelope.correlation_id = correlation_id.to_owned();
            workspace_failure(kind, envelope)
        }
        ProjectTransportError::Admission(error) => {
            let envelope = service_error(ServiceError::Admission(error), correlation_id);
            let kind = match error {
                CommandAdmissionError::InvalidRequest => WorkspaceFailureKind::Validation,
                CommandAdmissionError::IdempotencyConflict => WorkspaceFailureKind::Conflict,
                CommandAdmissionError::StorageUnavailable => WorkspaceFailureKind::AdapterRetryable,
                CommandAdmissionError::IntegrityFailure => WorkspaceFailureKind::RecoveryRequired,
            };
            workspace_failure(kind, envelope)
        }
        ProjectTransportError::Application(error) => {
            project_application_failure(error, correlation_id)
        }
        ProjectTransportError::InvalidRequest => workspace_failure(
            WorkspaceFailureKind::Validation,
            safe_project_error("project_request_invalid", correlation_id, false, true, None),
        ),
        ProjectTransportError::PageSnapshotChanged => workspace_failure(
            WorkspaceFailureKind::StaleSnapshot,
            safe_project_error(
                "project_list_snapshot_stale",
                correlation_id,
                true,
                false,
                None,
            ),
        ),
    }
}

fn project_application_failure(
    error: ProjectApplicationError,
    correlation_id: &str,
) -> WorkspaceFailure {
    match error {
        ProjectApplicationError::InvalidRequest => project_failure(
            WorkspaceFailureKind::Validation,
            "project_request_invalid",
            correlation_id,
            false,
            true,
            None,
        ),
        ProjectApplicationError::ProjectNotFound => project_failure(
            WorkspaceFailureKind::LocationMissing,
            "project_not_found",
            correlation_id,
            false,
            true,
            None,
        ),
        ProjectApplicationError::BindingNotFound => project_failure(
            WorkspaceFailureKind::LocationMissing,
            "project_binding_not_found",
            correlation_id,
            false,
            true,
            None,
        ),
        ProjectApplicationError::TrustDecisionMissing => project_failure(
            WorkspaceFailureKind::ScopeDenied,
            "project_trust_decision_missing",
            correlation_id,
            false,
            true,
            None,
        ),
        ProjectApplicationError::RecoveryRequired => project_failure(
            WorkspaceFailureKind::RecoveryRequired,
            "project_recovery_required",
            correlation_id,
            false,
            true,
            None,
        ),
        ProjectApplicationError::ProjectRestricted => project_failure(
            WorkspaceFailureKind::ScopeDenied,
            "project_restricted",
            correlation_id,
            false,
            true,
            None,
        ),
        ProjectApplicationError::ProjectRevoked => project_failure(
            WorkspaceFailureKind::ScopeDenied,
            "project_revoked",
            correlation_id,
            false,
            true,
            None,
        ),
        ProjectApplicationError::ProjectMissing => project_failure(
            WorkspaceFailureKind::LocationMissing,
            "project_location_missing",
            correlation_id,
            false,
            true,
            None,
        ),
        ProjectApplicationError::ProjectDetached => project_failure(
            WorkspaceFailureKind::LocationMissing,
            "project_location_detached",
            correlation_id,
            false,
            true,
            None,
        ),
        ProjectApplicationError::ProjectInaccessible => project_failure(
            WorkspaceFailureKind::AdapterTerminal,
            "project_location_inaccessible",
            correlation_id,
            false,
            true,
            None,
        ),
        ProjectApplicationError::ConcurrentChange => project_failure(
            WorkspaceFailureKind::StaleSnapshot,
            "project_concurrent_change",
            correlation_id,
            true,
            false,
            None,
        ),
        ProjectApplicationError::Location(error) => project_location_failure(error, correlation_id),
        ProjectApplicationError::Registry(error) => project_registry_failure(error, correlation_id),
        ProjectApplicationError::TrustDecision(error) => {
            project_trust_decision_failure(error, correlation_id)
        }
        ProjectApplicationError::Session(error) => project_session_failure(error, correlation_id),
    }
}

fn project_location_failure(error: ProjectLocationError, correlation_id: &str) -> WorkspaceFailure {
    let (kind, code, retryable, user_action_required) = match error {
        ProjectLocationError::InvalidRequest => (
            WorkspaceFailureKind::Validation,
            "project_location_invalid",
            false,
            true,
        ),
        ProjectLocationError::Missing => (
            WorkspaceFailureKind::LocationMissing,
            "project_location_missing",
            false,
            true,
        ),
        ProjectLocationError::Inaccessible => (
            WorkspaceFailureKind::AdapterTerminal,
            "project_location_inaccessible",
            false,
            true,
        ),
        ProjectLocationError::UnsafeLink => (
            WorkspaceFailureKind::ScopeDenied,
            "project_location_unsafe_link",
            false,
            true,
        ),
        ProjectLocationError::IdentityChanged => (
            WorkspaceFailureKind::Conflict,
            "project_location_identity_changed",
            true,
            false,
        ),
        ProjectLocationError::PortableMetadataConflict => (
            WorkspaceFailureKind::Conflict,
            "project_metadata_conflict",
            false,
            true,
        ),
        ProjectLocationError::InspectionIncomplete => (
            WorkspaceFailureKind::Validation,
            "project_inspection_incomplete",
            false,
            true,
        ),
        ProjectLocationError::RecoveryRequired => (
            WorkspaceFailureKind::RecoveryRequired,
            "project_registration_recovery_required",
            false,
            true,
        ),
        ProjectLocationError::AdapterUnavailable => (
            WorkspaceFailureKind::AdapterRetryable,
            "project_location_adapter_unavailable",
            true,
            false,
        ),
    };
    project_failure(
        kind,
        code,
        correlation_id,
        retryable,
        user_action_required,
        None,
    )
}

fn project_registry_failure(error: ProjectRegistryError, correlation_id: &str) -> WorkspaceFailure {
    let (kind, code, retryable, user_action_required, current_revision) = match error {
        ProjectRegistryError::InvalidInput(_) => (
            WorkspaceFailureKind::Validation,
            "project_registry_input_invalid",
            false,
            true,
            None,
        ),
        ProjectRegistryError::NotFound(_) => (
            WorkspaceFailureKind::LocationMissing,
            "project_registry_entity_not_found",
            false,
            true,
            None,
        ),
        ProjectRegistryError::InspectionExpired => (
            WorkspaceFailureKind::StaleSnapshot,
            "project_inspection_expired",
            true,
            false,
            None,
        ),
        ProjectRegistryError::RevisionConflict { actual, .. } => (
            WorkspaceFailureKind::Conflict,
            "project_revision_conflict",
            true,
            false,
            Some(actual),
        ),
        ProjectRegistryError::IdempotencyConflict => (
            WorkspaceFailureKind::Conflict,
            "project_idempotency_conflict",
            false,
            true,
            None,
        ),
        ProjectRegistryError::CanonicalLocationConflict { .. } => (
            WorkspaceFailureKind::Conflict,
            "project_location_already_bound",
            false,
            true,
            None,
        ),
        ProjectRegistryError::ProjectAlreadyExists => (
            WorkspaceFailureKind::Conflict,
            "project_already_exists",
            false,
            true,
            None,
        ),
        ProjectRegistryError::BindingProjectMismatch => (
            WorkspaceFailureKind::Validation,
            "project_binding_mismatch",
            false,
            true,
            None,
        ),
        ProjectRegistryError::SourceIdentityConflict => (
            WorkspaceFailureKind::Conflict,
            "project_source_identity_conflict",
            true,
            true,
            None,
        ),
        ProjectRegistryError::PortableProjectConflict => (
            WorkspaceFailureKind::Conflict,
            "project_portable_identity_conflict",
            false,
            true,
            None,
        ),
        ProjectRegistryError::InvalidStateTransition => (
            WorkspaceFailureKind::RecoveryRequired,
            "project_registration_state_invalid",
            false,
            true,
            None,
        ),
        ProjectRegistryError::TrustDecisionRejected => (
            WorkspaceFailureKind::ScopeDenied,
            "project_trust_decision_rejected",
            false,
            true,
            None,
        ),
        ProjectRegistryError::StorageUnavailable => (
            WorkspaceFailureKind::AdapterRetryable,
            "project_registry_unavailable",
            true,
            false,
            None,
        ),
        ProjectRegistryError::IntegrityFailure(_) => (
            WorkspaceFailureKind::RecoveryRequired,
            "project_registry_integrity_failure",
            false,
            true,
            None,
        ),
    };
    project_failure(
        kind,
        code,
        correlation_id,
        retryable,
        user_action_required,
        current_revision,
    )
}

fn project_trust_decision_failure(
    error: ProjectTrustDecisionError,
    correlation_id: &str,
) -> WorkspaceFailure {
    let code = match error {
        ProjectTrustDecisionError::InvalidReference => "project_trust_reference_invalid",
        ProjectTrustDecisionError::InvalidKind => "project_trust_reference_kind_invalid",
        ProjectTrustDecisionError::CommandMismatch => "project_trust_reference_command_mismatch",
    };
    project_failure(
        WorkspaceFailureKind::ScopeDenied,
        code,
        correlation_id,
        false,
        true,
        None,
    )
}

fn project_session_failure(
    error: dennett_memory_core::session::SessionJournalError,
    correlation_id: &str,
) -> WorkspaceFailure {
    use dennett_memory_core::session::SessionJournalError;
    let (kind, code, retryable, user_action_required, current_revision) = match error {
        SessionJournalError::NotFound => (
            WorkspaceFailureKind::RecoveryRequired,
            "project_session_missing",
            false,
            true,
            None,
        ),
        SessionJournalError::RevisionConflict { actual, .. } => (
            WorkspaceFailureKind::Conflict,
            "project_session_revision_conflict",
            true,
            false,
            Some(actual),
        ),
        SessionJournalError::IdempotencyConflict => (
            WorkspaceFailureKind::Conflict,
            "project_session_idempotency_conflict",
            false,
            true,
            None,
        ),
        SessionJournalError::StorageUnavailable => (
            WorkspaceFailureKind::AdapterRetryable,
            "project_session_storage_unavailable",
            true,
            false,
            None,
        ),
        SessionJournalError::InvalidTransition(_)
        | SessionJournalError::IntegrityFailure(_)
        | SessionJournalError::UnsupportedSchemaVersion { .. }
        | SessionJournalError::UnsupportedEventPayloadVersion { .. }
        | SessionJournalError::MigrationFailure => (
            WorkspaceFailureKind::RecoveryRequired,
            "project_session_recovery_required",
            false,
            true,
            None,
        ),
    };
    project_failure(
        kind,
        code,
        correlation_id,
        retryable,
        user_action_required,
        current_revision,
    )
}

fn project_failure(
    kind: WorkspaceFailureKind,
    code: &str,
    correlation_id: &str,
    retryable: bool,
    user_action_required: bool,
    current_revision: Option<u64>,
) -> WorkspaceFailure {
    workspace_failure(
        kind,
        safe_project_error(
            code,
            correlation_id,
            retryable,
            user_action_required,
            current_revision,
        ),
    )
}

fn workspace_failure(kind: WorkspaceFailureKind, error: ErrorEnvelope) -> WorkspaceFailure {
    WorkspaceFailure {
        kind: kind as i32,
        error: Some(error),
        current_revision: None,
        conflicting_paths: Vec::new(),
    }
}

fn safe_project_error(
    code: &str,
    correlation_id: &str,
    retryable: bool,
    user_action_required: bool,
    current_revision: Option<u64>,
) -> ErrorEnvelope {
    ErrorEnvelope {
        code: code.to_owned(),
        message_key: format!("project.{code}"),
        correlation_id: correlation_id.to_owned(),
        retryable,
        user_action_required,
        details_handle: String::new(),
        current_revision,
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
    if let ConversationError::Project(error) = error {
        return project_application_failure(error, correlation_id)
            .error
            .unwrap_or_else(|| internal_error("project_failure", "project.failure"));
    }
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
        ConversationError::Project(_) => unreachable!("project errors return above"),
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
    use crate::protocol::dennett::control::v1::project_service_server::ProjectService;
    use crate::protocol::dennett::control::v1::system_service_server::SystemService;
    use crate::{HandshakePolicy, SessionRegistry};
    use dennett_agent_core::FakeAgentRuntime;
    use dennett_head::conversation::LocalProject;
    use dennett_head::draft::SessionOperationLocks;
    use dennett_head::project::ProjectLocationPort;
    use dennett_head::session::SessionCoordinator;
    use dennett_head::system::{SystemMutation, SystemProjection};
    use dennett_memory_core::session::{InMemorySessionEventStore, SessionJournal};
    use dennett_storage_sqlite::SqliteControlStore;
    use dennett_sync_core::admission::InMemoryCommandAdmissionStore;
    use dennett_sync_core::draft::InMemoryDraftCache;
    use dennett_trust_core::project_registry::{
        CanonicalLocationKey, CanonicalWorkspaceLocation, RegistrationFilesystemObservation,
        SensitiveAbsolutePath, WorkspaceBinding, WorkspaceSourceIdentity,
    };
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use tempfile::TempDir;
    use tonic::codegen::tokio_stream::StreamExt;

    #[cfg(windows)]
    use crate::{AuthenticatedSystemClient, ClientConfig};
    #[cfg(windows)]
    use dennett_head::system::ProjectSummary;
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

    #[derive(Default)]
    struct RecordingProjectLocations {
        registration_actions: Mutex<Vec<PortableMetadataAction>>,
        rebind_actions: Mutex<Vec<RebindPortableMetadataAction>>,
    }

    impl RecordingProjectLocations {
        fn registration_actions(&self) -> Vec<PortableMetadataAction> {
            self.registration_actions
                .lock()
                .expect("project location actions poisoned")
                .clone()
        }

        fn canonical_location(path: &Path) -> CanonicalWorkspaceLocation {
            let display = path.to_string_lossy().into_owned();
            let digest = Sha256::digest(display.as_bytes());
            let mut key = [0_u8; 32];
            key.copy_from_slice(&digest);
            CanonicalWorkspaceLocation {
                path: SensitiveAbsolutePath::new(display).expect("absolute test project path"),
                key: CanonicalLocationKey::new(key),
            }
        }

        fn source_identity(path: &Path) -> WorkspaceSourceIdentity {
            let digest = Sha256::digest(path.to_string_lossy().as_bytes());
            let mut identity = [0_u8; 32];
            identity.copy_from_slice(&digest);
            WorkspaceSourceIdentity::new(identity)
        }

        fn observation(
            inspection: &ProjectLocationInspection,
            action: PortableMetadataAction,
            project_id: ProjectId,
        ) -> RegistrationFilesystemObservation {
            let (portable_metadata_state, portable_project_id) = match action {
                PortableMetadataAction::LeaveAbsent => (
                    inspection.portable_metadata_state,
                    inspection.portable_project_id,
                ),
                PortableMetadataAction::UseExisting
                | PortableMetadataAction::CreateMinimal
                | PortableMetadataAction::ForkWithNewIdentity => {
                    (PortableProjectMetadataState::PresentValid, Some(project_id))
                }
            };
            RegistrationFilesystemObservation {
                location: inspection.location.clone(),
                source_identity: inspection.source_identity,
                workspace_kind: inspection.workspace_kind,
                availability: WorkspaceAvailability::Available,
                access_mode: inspection.access_mode,
                portable_metadata_state,
                portable_project_id,
                instruction_fingerprint: inspection.instruction_fingerprint,
                instruction_source_count: inspection.instruction_source_count,
                observed_at_unix_ms: unix_time_ms(),
            }
        }
    }

    #[tonic::async_trait]
    impl ProjectLocationPort for RecordingProjectLocations {
        async fn inspect(
            &self,
            command: InspectProjectLocationCommand,
        ) -> Result<ProjectLocationInspection, ProjectLocationError> {
            let path = PathBuf::from(&command.root_uri);
            if !path.is_dir() {
                return Err(ProjectLocationError::Missing);
            }
            let metadata_absent = !path.join(".dennett").exists();
            Ok(ProjectLocationInspection {
                inspection_id: ProjectInspectionId::new(),
                registration_kind: command.registration_kind,
                location: Self::canonical_location(&path),
                suggested_display_name: path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("Test project")
                    .to_owned(),
                location_exists: true,
                location_empty: std::fs::read_dir(&path)
                    .map_err(|_| ProjectLocationError::Inaccessible)?
                    .next()
                    .is_none(),
                source_identity: Some(Self::source_identity(&path)),
                prospective_parent_identity: None,
                workspace_kind: WorkspaceKind::Folder,
                availability: WorkspaceAvailability::Available,
                access_mode: WorkspaceAccessMode::ReadWrite,
                portable_metadata_state: if metadata_absent {
                    PortableProjectMetadataState::Absent
                } else {
                    PortableProjectMetadataState::Invalid
                },
                portable_project_id: None,
                shared_memory_state: SharedProjectMemoryState::Absent,
                minimal_structure_creation_available: metadata_absent,
                instruction_fingerprint: None,
                instruction_source_count: 0,
                instruction_discovery_incomplete: false,
                observed_at_unix_ms: command.observed_at_unix_ms,
                expires_at_unix_ms: command.expires_at_unix_ms,
            })
        }

        async fn apply_registration_effect(
            &self,
            inspection: &ProjectLocationInspection,
            action: PortableMetadataAction,
            project_id: ProjectId,
        ) -> Result<RegistrationFilesystemObservation, ProjectLocationError> {
            self.registration_actions
                .lock()
                .expect("project location actions poisoned")
                .push(action);
            if action == PortableMetadataAction::CreateMinimal {
                std::fs::create_dir_all(
                    Path::new(inspection.location.path.expose_local()).join(".dennett"),
                )
                .map_err(|_| ProjectLocationError::AdapterUnavailable)?;
            }
            Ok(Self::observation(inspection, action, project_id))
        }

        async fn apply_rebind_effect(
            &self,
            inspection: &ProjectLocationInspection,
            action: RebindPortableMetadataAction,
            project_id: ProjectId,
        ) -> Result<RegistrationFilesystemObservation, ProjectLocationError> {
            self.rebind_actions
                .lock()
                .expect("project rebind actions poisoned")
                .push(action);
            let action = match action {
                RebindPortableMetadataAction::LeaveAbsent => PortableMetadataAction::LeaveAbsent,
                RebindPortableMetadataAction::UseExisting => PortableMetadataAction::UseExisting,
                RebindPortableMetadataAction::CreateMinimal => {
                    PortableMetadataAction::CreateMinimal
                }
            };
            Ok(Self::observation(inspection, action, project_id))
        }

        async fn observe_binding(
            &self,
            binding: &WorkspaceBinding,
            observed_at_unix_ms: u64,
        ) -> Result<RegistrationFilesystemObservation, ProjectLocationError> {
            Ok(RegistrationFilesystemObservation {
                location: binding.location.clone(),
                source_identity: binding.source_identity,
                workspace_kind: binding.kind,
                availability: binding.availability,
                access_mode: binding.access_mode,
                portable_metadata_state: binding.portable_metadata_state,
                portable_project_id: binding.portable_project_id,
                instruction_fingerprint: None,
                instruction_source_count: 0,
                observed_at_unix_ms,
            })
        }
    }

    struct ProjectServiceFixture {
        service: ProjectServiceAdapter,
        locations: Arc<RecordingProjectLocations>,
        client_session_id: String,
        workspace: PathBuf,
        _temp: TempDir,
    }

    async fn project_service_fixture() -> ProjectServiceFixture {
        let temp = tempfile::tempdir().expect("project service tempdir");
        let workspace = temp.path().join("workspace-without-dennett");
        std::fs::create_dir(&workspace).expect("create project workspace");
        let store = Arc::new(
            SqliteControlStore::open(temp.path().join("control.sqlite3"))
                .await
                .expect("open project control store"),
        );
        let locations = Arc::new(RecordingProjectLocations::default());
        let coordinator = SessionCoordinator::new(SessionJournal::new(store.clone()), 7, 16);
        let system = Arc::new(SystemProjection::new(SystemSnapshot::empty(7), 16));
        let application = Arc::new(ProjectApplication::new(
            store.clone(),
            locations.clone(),
            coordinator,
            system,
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
                    session_challenge: vec![17; 32],
                    requested_features: vec![crate::PROJECT_WORKSPACE_FEATURE.to_owned()],
                },
            )
            .expect("project handshake");
        registry
            .consume_bootstrap(&peer(), &welcome.client_session_id, &welcome.session_proof)
            .expect("project bootstrap");
        let service = ProjectServiceAdapter::new(application, registry, store);
        ProjectServiceFixture {
            service,
            locations,
            client_session_id: welcome.client_session_id,
            workspace,
            _temp: temp,
        }
    }

    async fn inspect_fixture_location(
        fixture: &ProjectServiceFixture,
    ) -> WireProjectLocationInspection {
        let response = fixture
            .service
            .inspect_project_location(request(InspectProjectLocationRequest {
                registration_kind: WireProjectRegistrationKind::AttachExisting as i32,
                root_uri: fixture.workspace.to_string_lossy().into_owned(),
                client_session_id: fixture.client_session_id.clone(),
            }))
            .await
            .expect("inspect project transport")
            .into_inner();
        match response.outcome.expect("inspection outcome") {
            inspect_project_location_response::Outcome::Inspection(inspection) => inspection,
            inspect_project_location_response::Outcome::Error(error) => {
                panic!("unexpected inspection error: {error:?}")
            }
        }
    }

    fn register_request(
        metadata: CommandMetadata,
        inspection_id: String,
        display_name: &str,
        action: WirePortableMetadataAction,
        trust_state: WireProjectTrustState,
        trust_decision: Option<StableRef>,
    ) -> RegisterProjectRequest {
        RegisterProjectRequest {
            command: Some(metadata),
            inspection_id,
            display_name: display_name.to_owned(),
            portable_metadata_action: action as i32,
            initial_trust_state: trust_state as i32,
            trust_decision,
        }
    }

    fn expect_workspace_error(
        outcome: Option<register_project_response::Outcome>,
    ) -> WorkspaceFailure {
        match outcome.expect("register project outcome") {
            register_project_response::Outcome::Error(error) => error,
            register_project_response::Outcome::Accepted(accepted) => {
                panic!("unexpected accepted project: {accepted:?}")
            }
        }
    }

    fn expect_registered(
        outcome: Option<register_project_response::Outcome>,
    ) -> RegisterProjectAccepted {
        match outcome.expect("registration outcome") {
            register_project_response::Outcome::Accepted(accepted) => accepted,
            register_project_response::Outcome::Error(error) => {
                panic!("unexpected registration error: {error:?}")
            }
        }
    }

    #[tokio::test]
    async fn folder_without_dennett_is_inspected_read_only_and_created_only_by_explicit_action() {
        let fixture = project_service_fixture().await;
        let dennett = fixture.workspace.join(".dennett");
        assert!(!dennett.exists());

        let inspection = inspect_fixture_location(&fixture).await;
        let metadata = inspection
            .portable_metadata
            .as_ref()
            .expect("portable metadata observation");
        assert_eq!(
            metadata.state,
            WirePortableProjectMetadataState::Absent as i32
        );
        assert!(metadata.minimal_structure_creation_available);
        assert!(!dennett.exists(), "inspection must not mutate the folder");
        assert!(fixture.locations.registration_actions().is_empty());

        let command_id = CommandId::new();
        let metadata = command(command_id, "explicit-minimal", &fixture.client_session_id);
        let rejected = fixture
            .service
            .register_project(request(register_request(
                metadata.clone(),
                inspection.inspection_id.clone(),
                "Explicit project",
                WirePortableMetadataAction::Unspecified,
                WireProjectTrustState::Restricted,
                None,
            )))
            .await
            .expect("unspecified action response")
            .into_inner();
        let failure = expect_workspace_error(rejected.outcome);
        assert_eq!(failure.kind, WorkspaceFailureKind::Validation as i32);
        assert_eq!(
            failure.error.expect("validation error").code,
            "project_request_invalid"
        );
        assert!(!dennett.exists());
        assert!(fixture.locations.registration_actions().is_empty());

        let accepted = fixture
            .service
            .register_project(request(register_request(
                metadata,
                inspection.inspection_id,
                "Explicit project",
                WirePortableMetadataAction::CreateMinimal,
                WireProjectTrustState::Restricted,
                None,
            )))
            .await
            .expect("explicit minimal response")
            .into_inner();
        assert!(matches!(
            accepted.outcome,
            Some(register_project_response::Outcome::Accepted(_))
        ));
        assert_eq!(
            fixture.locations.registration_actions(),
            vec![PortableMetadataAction::CreateMinimal]
        );
        assert!(
            dennett.is_dir(),
            "only the explicit action creates .dennett"
        );
    }

    #[tokio::test]
    async fn stale_authority_epoch_is_rejected_before_admission_or_folder_effect() {
        let fixture = project_service_fixture().await;
        let inspection = inspect_fixture_location(&fixture).await;
        let command_id = CommandId::new();
        let mut stale = command(command_id, "stale-epoch", &fixture.client_session_id);
        stale.authority_epoch_seen = 6;
        let rejected = fixture
            .service
            .register_project(request(register_request(
                stale,
                inspection.inspection_id.clone(),
                "Stale request",
                WirePortableMetadataAction::LeaveAbsent,
                WireProjectTrustState::Restricted,
                None,
            )))
            .await
            .expect("stale epoch response")
            .into_inner();
        let failure = expect_workspace_error(rejected.outcome);
        assert_eq!(failure.kind, WorkspaceFailureKind::StaleSnapshot as i32);
        let error = failure.error.expect("stale epoch error");
        assert_eq!(error.code, "ipc_authority_epoch_changed");
        assert!(error.retryable);
        assert!(fixture.locations.registration_actions().is_empty());

        let accepted = fixture
            .service
            .register_project(request(register_request(
                command(command_id, "stale-epoch", &fixture.client_session_id),
                inspection.inspection_id,
                "Fresh snapshot",
                WirePortableMetadataAction::LeaveAbsent,
                WireProjectTrustState::Restricted,
                None,
            )))
            .await
            .expect("fresh epoch response")
            .into_inner();
        assert!(matches!(
            accepted.outcome,
            Some(register_project_response::Outcome::Accepted(_))
        ));
        assert_eq!(
            fixture.locations.registration_actions(),
            vec![PortableMetadataAction::LeaveAbsent]
        );
    }

    #[tokio::test]
    async fn trust_reference_requires_bridge_kind_and_current_command_identity() {
        let fixture = project_service_fixture().await;
        let inspection = inspect_fixture_location(&fixture).await;
        let command_id = CommandId::new();
        let metadata = command(command_id, "trust-reference", &fixture.client_session_id);

        for (decision, expected_code) in [
            (
                StableRef {
                    kind: "project_file".to_owned(),
                    id: command_id.0.to_string(),
                },
                "project_trust_reference_kind_invalid",
            ),
            (
                StableRef {
                    kind: PROJECT_TRUST_DECISION_REF_KIND.to_owned(),
                    id: CommandId::new().0.to_string(),
                },
                "project_trust_reference_command_mismatch",
            ),
        ] {
            let rejected = fixture
                .service
                .register_project(request(register_request(
                    metadata.clone(),
                    inspection.inspection_id.clone(),
                    "Trusted project",
                    WirePortableMetadataAction::LeaveAbsent,
                    WireProjectTrustState::TrustedBounded,
                    Some(decision),
                )))
                .await
                .expect("invalid trust reference response")
                .into_inner();
            let failure = expect_workspace_error(rejected.outcome);
            assert_eq!(failure.kind, WorkspaceFailureKind::ScopeDenied as i32);
            assert_eq!(failure.error.expect("trust error").code, expected_code);
            assert!(fixture.locations.registration_actions().is_empty());
        }

        let accepted = fixture
            .service
            .register_project(request(register_request(
                metadata,
                inspection.inspection_id,
                "Trusted project",
                WirePortableMetadataAction::LeaveAbsent,
                WireProjectTrustState::TrustedBounded,
                Some(StableRef {
                    kind: PROJECT_TRUST_DECISION_REF_KIND.to_owned(),
                    id: command_id.0.to_string(),
                }),
            )))
            .await
            .expect("valid trust reference response")
            .into_inner();
        assert!(matches!(
            accepted.outcome,
            Some(register_project_response::Outcome::Accepted(_))
        ));
        assert_eq!(fixture.locations.registration_actions().len(), 1);
    }

    #[tokio::test]
    async fn registration_retry_reuses_receipts_without_reapplying_the_folder_effect() {
        let fixture = project_service_fixture().await;
        let inspection = inspect_fixture_location(&fixture).await;
        let inspection_id = inspection.inspection_id;
        let command_id = CommandId::new();
        let metadata = command(command_id, "registration-retry", &fixture.client_session_id);
        let body = register_request(
            metadata.clone(),
            inspection_id.clone(),
            "Retry project",
            WirePortableMetadataAction::LeaveAbsent,
            WireProjectTrustState::Restricted,
            None,
        );

        let first = fixture
            .service
            .register_project(request(body.clone()))
            .await
            .expect("first registration")
            .into_inner();
        let replay = fixture
            .service
            .register_project(request(body))
            .await
            .expect("registration replay")
            .into_inner();
        let first = expect_registered(first.outcome);
        let replay = expect_registered(replay.outcome);
        assert_eq!(first.project_id, replay.project_id);
        assert_eq!(first.workspace_binding_id, replay.workspace_binding_id);
        assert_eq!(first.command, replay.command);
        assert_eq!(
            fixture.locations.registration_actions(),
            vec![PortableMetadataAction::LeaveAbsent]
        );

        let conflict = fixture
            .service
            .register_project(request(register_request(
                metadata,
                inspection_id,
                "Changed intent",
                WirePortableMetadataAction::LeaveAbsent,
                WireProjectTrustState::Restricted,
                None,
            )))
            .await
            .expect("conflicting replay response")
            .into_inner();
        let failure = expect_workspace_error(conflict.outcome);
        assert_eq!(failure.kind, WorkspaceFailureKind::Conflict as i32);
        assert_eq!(
            failure.error.expect("idempotency conflict").code,
            "command_idempotency_conflict"
        );
        assert_eq!(fixture.locations.registration_actions().len(), 1);
    }

    #[tokio::test]
    async fn concurrent_registration_replay_applies_the_folder_effect_once() {
        let fixture = project_service_fixture().await;
        let inspection = inspect_fixture_location(&fixture).await;
        let command_id = CommandId::new();
        let body = register_request(
            command(
                command_id,
                "concurrent-registration-retry",
                &fixture.client_session_id,
            ),
            inspection.inspection_id,
            "Concurrent retry project",
            WirePortableMetadataAction::CreateMinimal,
            WireProjectTrustState::Restricted,
            None,
        );
        let first = fixture.service.clone();
        let second = fixture.service.clone();

        let (first, second) = tokio::join!(
            first.register_project(request(body.clone())),
            second.register_project(request(body))
        );
        let first = expect_registered(
            first
                .expect("first registration response")
                .into_inner()
                .outcome,
        );
        let second = expect_registered(
            second
                .expect("second registration response")
                .into_inner()
                .outcome,
        );

        assert_eq!(first.project_id, second.project_id);
        assert_eq!(first.workspace_binding_id, second.workspace_binding_id);
        assert_eq!(
            first
                .command
                .as_ref()
                .expect("first acceptance")
                .operation_id,
            second
                .command
                .as_ref()
                .expect("second acceptance")
                .operation_id
        );
        assert_eq!(
            fixture.locations.registration_actions(),
            vec![PortableMetadataAction::CreateMinimal]
        );
    }

    #[tokio::test]
    async fn trust_update_retry_recognizes_the_durable_decision() {
        let fixture = project_service_fixture().await;
        let inspection = inspect_fixture_location(&fixture).await;
        let registration_id = CommandId::new();
        let registration = fixture
            .service
            .register_project(request(register_request(
                command(
                    registration_id,
                    "trust-retry-registration",
                    &fixture.client_session_id,
                ),
                inspection.inspection_id,
                "Trust retry project",
                WirePortableMetadataAction::LeaveAbsent,
                WireProjectTrustState::Restricted,
                None,
            )))
            .await
            .expect("register project for trust retry")
            .into_inner();
        let registered = match registration.outcome.expect("registration outcome") {
            register_project_response::Outcome::Accepted(accepted) => accepted,
            register_project_response::Outcome::Error(error) => {
                panic!("unexpected registration error: {error:?}")
            }
        };

        let trust_command_id = CommandId::new();
        let mut trust_metadata = command(
            trust_command_id,
            "trust-update-retry",
            &fixture.client_session_id,
        );
        trust_metadata.expected_revision = Some(1);
        let trust_decision_id = trust_command_id.0.to_string();
        let body = SetProjectTrustRequest {
            command: Some(trust_metadata.clone()),
            project_id: registered.project_id.clone(),
            trust_state: WireProjectTrustState::TrustedBounded as i32,
            expected_policy_revision: 1,
            trust_decision: Some(StableRef {
                kind: PROJECT_TRUST_DECISION_REF_KIND.to_owned(),
                id: trust_decision_id.clone(),
            }),
        };
        let first = fixture.service.clone();
        let second = fixture.service.clone();
        let (first, second) = tokio::join!(
            first.set_project_trust(request(body.clone())),
            second.set_project_trust(request(body.clone()))
        );
        let replay = fixture
            .service
            .set_project_trust(request(body))
            .await
            .expect("durable trust update replay");
        for response in [
            first.expect("first trust update").into_inner(),
            second.expect("concurrent trust update replay").into_inner(),
            replay.into_inner(),
        ] {
            assert!(matches!(
                response.outcome,
                Some(set_project_trust_response::Outcome::Accepted(_))
            ));
        }
        let project = fixture
            .service
            .application
            .get_project(ProjectId(
                uuid::Uuid::parse_str(&registered.project_id).expect("registered project id"),
            ))
            .await
            .expect("load trusted project");
        assert_eq!(
            project.access_policy.trust_state,
            DomainProjectTrustState::TrustedBounded
        );
        assert_eq!(project.access_policy.revision, 2);
        assert_eq!(
            project
                .access_policy
                .last_decision
                .as_ref()
                .map(|decision| decision.id.as_str()),
            Some(trust_decision_id.as_str())
        );

        let conflict = fixture
            .service
            .set_project_trust(request(SetProjectTrustRequest {
                command: Some(trust_metadata),
                project_id: registered.project_id,
                trust_state: WireProjectTrustState::Restricted as i32,
                expected_policy_revision: 1,
                trust_decision: Some(StableRef {
                    kind: PROJECT_TRUST_DECISION_REF_KIND.to_owned(),
                    id: trust_decision_id,
                }),
            }))
            .await
            .expect("conflicting trust replay")
            .into_inner();
        let failure = match conflict.outcome.expect("trust outcome") {
            set_project_trust_response::Outcome::Error(error) => error,
            set_project_trust_response::Outcome::Accepted(accepted) => {
                panic!("unexpected trust acceptance: {accepted:?}")
            }
        };
        assert_eq!(failure.kind, WorkspaceFailureKind::Conflict as i32);
        assert_eq!(
            failure.error.expect("trust conflict").code,
            "command_idempotency_conflict"
        );
    }

    #[test]
    fn project_pagination_is_bounded_and_tokens_are_snapshot_bound() {
        assert_eq!(normalized_project_page_size(0).expect("default"), 50);
        assert_eq!(normalized_project_page_size(1).expect("minimum"), 1);
        assert_eq!(normalized_project_page_size(200).expect("maximum"), 200);
        assert!(matches!(
            normalized_project_page_size(201),
            Err(ProjectTransportError::InvalidRequest)
        ));

        let token = encode_project_page_token(0xabc, 50);
        assert_eq!(
            decode_project_page_token(&token, 0xabc).expect("matching snapshot token"),
            50
        );
        assert!(matches!(
            decode_project_page_token(&token, 0xdef),
            Err(ProjectTransportError::PageSnapshotChanged)
        ));
        assert!(matches!(
            decode_project_page_token("invalid", 0xabc),
            Err(ProjectTransportError::InvalidRequest)
        ));
    }

    #[test]
    fn project_and_conversation_errors_use_safe_retry_semantics() {
        for (error, expected_kind, expected_code, retryable, user_action) in [
            (
                ProjectApplicationError::ProjectRestricted,
                WorkspaceFailureKind::ScopeDenied,
                "project_restricted",
                false,
                true,
            ),
            (
                ProjectApplicationError::ProjectRevoked,
                WorkspaceFailureKind::ScopeDenied,
                "project_revoked",
                false,
                true,
            ),
            (
                ProjectApplicationError::ProjectMissing,
                WorkspaceFailureKind::LocationMissing,
                "project_location_missing",
                false,
                true,
            ),
            (
                ProjectApplicationError::ProjectDetached,
                WorkspaceFailureKind::LocationMissing,
                "project_location_detached",
                false,
                true,
            ),
            (
                ProjectApplicationError::ProjectInaccessible,
                WorkspaceFailureKind::AdapterTerminal,
                "project_location_inaccessible",
                false,
                true,
            ),
            (
                ProjectApplicationError::ConcurrentChange,
                WorkspaceFailureKind::StaleSnapshot,
                "project_concurrent_change",
                true,
                false,
            ),
        ] {
            let failure = project_application_failure(error, "correlation-safe");
            assert_eq!(failure.kind, expected_kind as i32);
            let envelope = failure.error.expect("safe project envelope");
            assert_eq!(envelope.code, expected_code);
            assert_eq!(envelope.correlation_id, "correlation-safe");
            assert_eq!(envelope.retryable, retryable);
            assert_eq!(envelope.user_action_required, user_action);
            assert!(envelope.details_handle.is_empty());
        }

        let conversation = conversation_error(
            ConversationError::Project(ProjectApplicationError::ConcurrentChange),
            "conversation-correlation",
        );
        assert_eq!(conversation.code, "project_concurrent_change");
        assert_eq!(conversation.correlation_id, "conversation-correlation");
        assert!(conversation.retryable);
        assert!(!conversation.user_action_required);

        let internal = project_registry_failure(
            ProjectRegistryError::IntegrityFailure("C:\\private\\owner\\workspace"),
            "safe-registry",
        );
        let rendered = format!("{internal:?}");
        assert!(!rendered.contains("private"));
        assert!(!rendered.contains("owner"));
        assert_eq!(
            internal.error.expect("registry failure").code,
            "project_registry_integrity_failure"
        );
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
        let application = Arc::new(
            ConversationApplication::new(
                coordinator,
                system.clone(),
                Arc::new(FakeAgentRuntime),
                LocalProject {
                    project_id,
                    display_name: "Test project".to_owned(),
                    workspace_path: "C:\\test-project".to_owned(),
                    standalone_workspace_path: "C:\\test-scratch".to_owned(),
                },
            )
            .with_unregistered_project_fixture_for_tests(),
        );
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
                state: DomainProjectState::Ready,
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
