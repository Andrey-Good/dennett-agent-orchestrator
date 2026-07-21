use crate::protocol::dennett::common::v1::ErrorEnvelope;
use crate::protocol::dennett::common::v1::{CommandAccepted, CommandMetadata, StableRef};
use crate::protocol::dennett::control::v1::bootstrap_response;
use crate::protocol::dennett::control::v1::handshake_response;
use crate::protocol::dennett::control::v1::session_service_client::SessionServiceClient;
use crate::protocol::dennett::control::v1::system_service_client::SystemServiceClient;
use crate::protocol::dennett::control::v1::system_watch_frame;
use crate::protocol::dennett::control::v1::{
    BootstrapRequest, BootstrapSnapshot, CancelTurnRequest, ClientHello, CompatibilityMode,
    ComposerDraft, ComposerDraftDiscarded, ComposerDraftWriteReceipt, ContextAttachment,
    CreateSessionAccepted, CreateSessionRequest, DiscardComposerDraftRequest,
    GetComposerDraftRequest, HandshakeRequest, RuntimeControlSelection, SaveComposerDraftRequest,
    SendTurnAccepted, SendTurnRequest, TurnDeliveryMode, WatchRequest, WatchResponse,
    WatchSessionRequest, WatchSessionResponse,
};
use crate::protocol::dennett::control::v1::{
    cancel_turn_response, create_session_response, discard_composer_draft_response,
    get_composer_draft_response, save_composer_draft_response, send_turn_response,
    session_watch_frame,
};
use crate::transport::connect_channel;
use crate::{
    COMPOSER_DRAFT_FEATURE, DEFAULT_MAX_MESSAGE_BYTES, LocalEndpoint, M01_PROTOCOL_VERSION,
    SESSION_CONVERSATION_FEATURE, SYSTEM_WATCH_FEATURE, TransportError,
};
use std::time::Duration;
use tonic::codec::Streaming;
use tonic::transport::Channel;

const DEFAULT_RPC_DEADLINE: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientConfig {
    pub installation_id: String,
    pub device_id: String,
    pub component_version: String,
    pub requested_features: Vec<String>,
    pub rpc_deadline: Duration,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientSendTurnRequest {
    pub command: ClientCommand,
    pub project_id: String,
    pub session_id: String,
    pub text: String,
    pub attachments: Vec<(String, String, String)>,
    pub runtime_controls: Vec<(String, String)>,
    pub delivery_mode: TurnDeliveryMode,
    pub expected_active_turn_id: Option<String>,
}

impl ClientConfig {
    #[must_use]
    pub fn m01(
        installation_id: impl Into<String>,
        device_id: impl Into<String>,
        component_version: impl Into<String>,
    ) -> Self {
        Self {
            installation_id: installation_id.into(),
            device_id: device_id.into(),
            component_version: component_version.into(),
            requested_features: vec![
                SYSTEM_WATCH_FEATURE.to_owned(),
                SESSION_CONVERSATION_FEATURE.to_owned(),
                COMPOSER_DRAFT_FEATURE.to_owned(),
            ],
            rpc_deadline: DEFAULT_RPC_DEADLINE,
        }
    }
}

pub struct AuthenticatedSystemClient {
    inner: SystemServiceClient<Channel>,
    sessions: SessionServiceClient<Channel>,
    client_session_id: String,
    bootstrap: BootstrapSnapshot,
    rpc_deadline: Duration,
}

impl AuthenticatedSystemClient {
    pub async fn connect(config: ClientConfig) -> Result<Self, ClientError> {
        if config.device_id.is_empty()
            || config.component_version.is_empty()
            || config.rpc_deadline.is_zero()
        {
            return Err(ClientError::InvalidConfiguration);
        }
        let rpc_deadline = config.rpc_deadline;
        let requested_features = config.requested_features.clone();
        let endpoint = LocalEndpoint::for_installation(config.installation_id.clone())?;
        let channel = connect_channel(&endpoint).await?;
        let mut inner = SystemServiceClient::new(channel.clone())
            .max_decoding_message_size(DEFAULT_MAX_MESSAGE_BYTES as usize)
            .max_encoding_message_size(DEFAULT_MAX_MESSAGE_BYTES as usize);
        let mut challenge = vec![0_u8; 32];
        getrandom::fill(&mut challenge).map_err(|_| ClientError::RandomUnavailable)?;
        let response = tokio::time::timeout(
            rpc_deadline,
            inner.handshake(HandshakeRequest {
                hello: Some(ClientHello {
                    client_component: "dennett-desktop-shell".to_owned(),
                    component_version: config.component_version,
                    protocol_versions: vec![M01_PROTOCOL_VERSION],
                    installation_id: config.installation_id,
                    device_id: config.device_id,
                    session_challenge: challenge,
                    requested_features: config.requested_features,
                }),
            }),
        )
        .await
        .map_err(|_| ClientError::DeadlineExceeded("handshake"))??
        .into_inner();
        let welcome = match response.outcome {
            Some(handshake_response::Outcome::Welcome(welcome)) => welcome,
            Some(handshake_response::Outcome::Error(error)) => {
                return Err(ClientError::Remote(error));
            }
            None => return Err(ClientError::MalformedResponse("handshake outcome")),
        };
        if welcome.protocol_version != M01_PROTOCOL_VERSION
            || welcome.compatibility_mode != CompatibilityMode::Full as i32
            || welcome.client_session_id.is_empty()
            || welcome.session_proof.is_empty()
            || requested_features
                .iter()
                .any(|feature| !welcome.enabled_features.contains(feature))
        {
            return Err(ClientError::ProtocolIncompatible);
        }

        let client_session_id = welcome.client_session_id;
        let response = tokio::time::timeout(
            rpc_deadline,
            inner.bootstrap(BootstrapRequest {
                session_proof: welcome.session_proof,
                known_revision: None,
                client_session_id: client_session_id.clone(),
            }),
        )
        .await
        .map_err(|_| ClientError::DeadlineExceeded("bootstrap"))??
        .into_inner();
        let bootstrap = match response.outcome {
            Some(bootstrap_response::Outcome::Snapshot(snapshot)) => snapshot,
            Some(bootstrap_response::Outcome::Error(error)) => {
                return Err(ClientError::Remote(error));
            }
            None => return Err(ClientError::MalformedResponse("bootstrap outcome")),
        };
        if bootstrap.authority_epoch != welcome.authority_epoch_seen {
            return Err(ClientError::AuthorityEpochMismatch);
        }
        Ok(Self {
            inner,
            sessions: SessionServiceClient::new(channel)
                .max_decoding_message_size(DEFAULT_MAX_MESSAGE_BYTES as usize)
                .max_encoding_message_size(DEFAULT_MAX_MESSAGE_BYTES as usize),
            client_session_id,
            bootstrap,
            rpc_deadline,
        })
    }

    #[must_use]
    pub fn bootstrap(&self) -> &BootstrapSnapshot {
        &self.bootstrap
    }

    pub async fn watch(&mut self) -> Result<AuthenticatedSystemWatch, ClientError> {
        let inner = tokio::time::timeout(
            self.rpc_deadline,
            self.inner.watch(WatchRequest {
                client_session_id: self.client_session_id.clone(),
                known_revision: Some(self.bootstrap.revision),
            }),
        )
        .await
        .map_err(|_| ClientError::DeadlineExceeded("watch"))??
        .into_inner();
        Ok(AuthenticatedSystemWatch {
            inner,
            state: WatchState::new(&self.bootstrap),
        })
    }

    pub async fn create_session(
        &mut self,
        command: ClientCommand,
        project_id: String,
        title: String,
    ) -> Result<CreateSessionAccepted, ClientError> {
        let expected_command_id = command.command_id.clone();
        let expected_correlation_id = command.correlation_id.clone();
        let response = tokio::time::timeout(
            self.rpc_deadline,
            self.sessions.create_session(CreateSessionRequest {
                command: Some(self.command_metadata(command)),
                project_id,
                title,
            }),
        )
        .await
        .map_err(|_| ClientError::DeadlineExceeded("create_session"))??
        .into_inner();
        match response.outcome {
            Some(create_session_response::Outcome::Accepted(accepted)) => {
                validate_command_accepted(
                    accepted.command.as_ref(),
                    &expected_command_id,
                    &expected_correlation_id,
                )?;
                Ok(accepted)
            }
            Some(create_session_response::Outcome::Error(error)) => Err(ClientError::Remote(error)),
            None => Err(ClientError::MalformedResponse("create session outcome")),
        }
    }

    pub async fn send_turn(
        &mut self,
        request: ClientSendTurnRequest,
    ) -> Result<SendTurnAccepted, ClientError> {
        let ClientSendTurnRequest {
            command,
            project_id,
            session_id,
            text,
            attachments,
            runtime_controls,
            delivery_mode,
            expected_active_turn_id,
        } = request;
        let expected_command_id = command.command_id.clone();
        let expected_correlation_id = command.correlation_id.clone();
        let response = tokio::time::timeout(
            self.rpc_deadline,
            self.sessions.send_turn(SendTurnRequest {
                command: Some(self.command_metadata(command)),
                project_id,
                session_id,
                text,
                attachments: attachments
                    .into_iter()
                    .map(|(kind, id, label)| ContextAttachment {
                        source: Some(StableRef { kind, id }),
                        label,
                    })
                    .collect(),
                runtime_controls: runtime_controls
                    .into_iter()
                    .map(|(control_id, choice_id)| RuntimeControlSelection {
                        control_id,
                        choice_id,
                    })
                    .collect(),
                delivery_mode: delivery_mode as i32,
                expected_active_turn_id: expected_active_turn_id.unwrap_or_default(),
            }),
        )
        .await
        .map_err(|_| ClientError::DeadlineExceeded("send_turn"))??
        .into_inner();
        match response.outcome {
            Some(send_turn_response::Outcome::Accepted(accepted)) => {
                validate_command_accepted(
                    accepted.command.as_ref(),
                    &expected_command_id,
                    &expected_correlation_id,
                )?;
                Ok(accepted)
            }
            Some(send_turn_response::Outcome::Error(error)) => Err(ClientError::Remote(error)),
            None => Err(ClientError::MalformedResponse("send turn outcome")),
        }
    }

    pub async fn cancel_turn(
        &mut self,
        command: ClientCommand,
        project_id: String,
        session_id: String,
        turn_id: String,
    ) -> Result<(), ClientError> {
        let expected_command_id = command.command_id.clone();
        let expected_correlation_id = command.correlation_id.clone();
        let response = tokio::time::timeout(
            self.rpc_deadline,
            self.sessions.cancel_turn(CancelTurnRequest {
                command: Some(self.command_metadata(command)),
                project_id,
                session_id,
                turn_id,
            }),
        )
        .await
        .map_err(|_| ClientError::DeadlineExceeded("cancel_turn"))??
        .into_inner();
        match response.outcome {
            Some(cancel_turn_response::Outcome::Accepted(accepted)) => {
                validate_command_accepted(
                    Some(&accepted),
                    &expected_command_id,
                    &expected_correlation_id,
                )?;
                Ok(())
            }
            Some(cancel_turn_response::Outcome::Error(error)) => Err(ClientError::Remote(error)),
            None => Err(ClientError::MalformedResponse("cancel turn outcome")),
        }
    }

    pub async fn watch_session(
        &mut self,
        session_id: String,
        known_revision: Option<u64>,
    ) -> Result<AuthenticatedSessionWatch, ClientError> {
        let inner = tokio::time::timeout(
            self.rpc_deadline,
            self.sessions.watch_session(WatchSessionRequest {
                session_id,
                known_revision,
                client_session_id: self.client_session_id.clone(),
            }),
        )
        .await
        .map_err(|_| ClientError::DeadlineExceeded("watch_session"))??
        .into_inner();
        Ok(AuthenticatedSessionWatch {
            inner,
            state: SessionWatchState::new(
                self.bootstrap.authority_epoch,
                known_revision.unwrap_or(0),
            ),
        })
    }

    pub async fn get_composer_draft(
        &mut self,
        project_id: String,
        session_id: String,
    ) -> Result<Option<ComposerDraft>, ClientError> {
        let response = tokio::time::timeout(
            self.rpc_deadline,
            self.sessions.get_composer_draft(GetComposerDraftRequest {
                project_id,
                session_id,
                client_session_id: self.client_session_id.clone(),
            }),
        )
        .await
        .map_err(|_| ClientError::DeadlineExceeded("get_composer_draft"))??
        .into_inner();
        match response.outcome {
            Some(get_composer_draft_response::Outcome::Draft(draft)) => Ok(Some(draft)),
            Some(get_composer_draft_response::Outcome::Missing(_)) => Ok(None),
            Some(get_composer_draft_response::Outcome::Error(error)) => {
                Err(ClientError::Remote(error))
            }
            None => Err(ClientError::MalformedResponse("get composer draft outcome")),
        }
    }

    pub async fn save_composer_draft(
        &mut self,
        operation: ClientCommand,
        draft: ComposerDraft,
    ) -> Result<ComposerDraftWriteReceipt, ClientError> {
        let response = tokio::time::timeout(
            self.rpc_deadline,
            self.sessions.save_composer_draft(SaveComposerDraftRequest {
                operation: Some(self.command_metadata(operation)),
                draft: Some(draft),
            }),
        )
        .await
        .map_err(|_| ClientError::DeadlineExceeded("save_composer_draft"))??
        .into_inner();
        match response.outcome {
            Some(save_composer_draft_response::Outcome::Saved(receipt)) => Ok(receipt),
            Some(save_composer_draft_response::Outcome::Error(error)) => {
                Err(ClientError::Remote(error))
            }
            None => Err(ClientError::MalformedResponse(
                "save composer draft outcome",
            )),
        }
    }

    pub async fn discard_composer_draft(
        &mut self,
        operation: ClientCommand,
        project_id: String,
        session_id: String,
        draft_command_id: String,
    ) -> Result<ComposerDraftDiscarded, ClientError> {
        let response = tokio::time::timeout(
            self.rpc_deadline,
            self.sessions
                .discard_composer_draft(DiscardComposerDraftRequest {
                    operation: Some(self.command_metadata(operation)),
                    project_id,
                    session_id,
                    draft_command_id,
                }),
        )
        .await
        .map_err(|_| ClientError::DeadlineExceeded("discard_composer_draft"))??
        .into_inner();
        match response.outcome {
            Some(discard_composer_draft_response::Outcome::Discarded(discarded)) => Ok(discarded),
            Some(discard_composer_draft_response::Outcome::Error(error)) => {
                Err(ClientError::Remote(error))
            }
            None => Err(ClientError::MalformedResponse(
                "discard composer draft outcome",
            )),
        }
    }

    fn command_metadata(&self, command: ClientCommand) -> CommandMetadata {
        CommandMetadata {
            idempotency_key: command.command_id.clone(),
            command_id: command.command_id,
            correlation_id: command.correlation_id,
            authority_epoch_seen: self.bootstrap.authority_epoch,
            created_at: Some(timestamp(command.created_at_unix_ms)),
            expected_revision: command.expected_revision,
            client_session_id: self.client_session_id.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientCommand {
    pub command_id: String,
    pub correlation_id: String,
    pub created_at_unix_ms: u64,
    pub expected_revision: Option<u64>,
}

impl ClientCommand {
    #[must_use]
    pub fn new(correlation_id: impl Into<String>, expected_revision: Option<u64>) -> Self {
        Self {
            command_id: uuid::Uuid::now_v7().to_string(),
            correlation_id: correlation_id.into(),
            created_at_unix_ms: unix_time_ms(),
            expected_revision,
        }
    }
}

pub struct AuthenticatedSystemWatch {
    inner: Streaming<WatchResponse>,
    state: WatchState,
}

pub struct AuthenticatedSessionWatch {
    inner: Streaming<WatchSessionResponse>,
    state: SessionWatchState,
}

impl AuthenticatedSessionWatch {
    pub async fn message(&mut self) -> Result<Option<WatchSessionResponse>, ClientError> {
        let Some(response) = self.inner.message().await? else {
            return Ok(None);
        };
        self.state.validate(&response)?;
        Ok(Some(response))
    }
}

impl AuthenticatedSystemWatch {
    pub async fn message(&mut self) -> Result<Option<WatchResponse>, ClientError> {
        let Some(response) = self.inner.message().await? else {
            return Ok(None);
        };
        self.state.validate(&response)?;
        Ok(Some(response))
    }
}

#[derive(Debug)]
struct WatchState {
    authority_epoch: u64,
    stream_id: Option<String>,
    sequence: u64,
    revision: u64,
    first_frame: bool,
    stale: bool,
}

impl WatchState {
    fn new(bootstrap: &BootstrapSnapshot) -> Self {
        Self {
            authority_epoch: bootstrap.authority_epoch,
            stream_id: None,
            sequence: 0,
            revision: bootstrap.revision,
            first_frame: true,
            stale: false,
        }
    }

    fn validate(&mut self, response: &WatchResponse) -> Result<(), ClientError> {
        if self.stale {
            return Err(ClientError::WatchInvariant("frame_after_resync"));
        }
        let frame = response
            .frame
            .as_ref()
            .ok_or(ClientError::WatchInvariant("missing_frame"))?;
        let payload = frame
            .frame
            .as_ref()
            .ok_or(ClientError::WatchInvariant("missing_payload"))?;
        if matches!(payload, system_watch_frame::Frame::Error(_)) {
            self.stale = true;
            self.first_frame = false;
            return Ok(());
        }
        let cursor = frame
            .cursor
            .as_ref()
            .ok_or(ClientError::WatchInvariant("missing_cursor"))?;
        if cursor.authority_epoch != self.authority_epoch {
            return Err(ClientError::WatchInvariant("authority_epoch_changed"));
        }
        match &self.stream_id {
            Some(stream_id) if stream_id != &cursor.stream_id => {
                return Err(ClientError::WatchInvariant("stream_changed"));
            }
            None if cursor.stream_id.is_empty() => {
                return Err(ClientError::WatchInvariant("missing_stream_id"));
            }
            None => self.stream_id = Some(cursor.stream_id.clone()),
            Some(_) => {}
        }
        let expected_sequence = self
            .sequence
            .checked_add(1)
            .ok_or(ClientError::WatchInvariant("sequence_overflow"))?;
        if cursor.sequence != expected_sequence {
            return Err(ClientError::WatchInvariant("sequence_gap"));
        }
        match payload {
            system_watch_frame::Frame::Snapshot(snapshot) => {
                if !self.first_frame {
                    return Err(ClientError::WatchInvariant("unexpected_snapshot"));
                }
                let bootstrap = snapshot
                    .bootstrap
                    .as_ref()
                    .ok_or(ClientError::WatchInvariant("missing_snapshot"))?;
                if bootstrap.authority_epoch != self.authority_epoch {
                    return Err(ClientError::WatchInvariant("snapshot_epoch_mismatch"));
                }
                if snapshot.snapshot_fingerprint.is_empty() {
                    return Err(ClientError::WatchInvariant("snapshot_fingerprint_missing"));
                }
                self.revision = bootstrap.revision;
            }
            system_watch_frame::Frame::Delta(delta) => {
                self.require_snapshot()?;
                if delta.base_revision != self.revision
                    || delta.new_revision != delta.base_revision.saturating_add(1)
                {
                    return Err(ClientError::WatchInvariant("revision_gap"));
                }
                self.revision = delta.new_revision;
            }
            system_watch_frame::Frame::Heartbeat(heartbeat) => {
                self.require_snapshot()?;
                if heartbeat.current_revision != self.revision {
                    return Err(ClientError::WatchInvariant("heartbeat_revision_mismatch"));
                }
            }
            system_watch_frame::Frame::ResyncRequired(_) => {
                self.require_snapshot()?;
                self.stale = true;
            }
            system_watch_frame::Frame::Error(_) => unreachable!("handled before cursor validation"),
        }
        self.sequence = cursor.sequence;
        self.first_frame = false;
        Ok(())
    }

    fn require_snapshot(&self) -> Result<(), ClientError> {
        if self.first_frame {
            Err(ClientError::WatchInvariant("first_frame_not_snapshot"))
        } else {
            Ok(())
        }
    }
}

#[derive(Debug)]
struct SessionWatchState {
    authority_epoch: u64,
    stream_id: Option<String>,
    sequence: u64,
    revision: u64,
    first_frame: bool,
    stale: bool,
}

impl SessionWatchState {
    fn new(authority_epoch: u64, revision: u64) -> Self {
        Self {
            authority_epoch,
            stream_id: None,
            sequence: 0,
            revision,
            first_frame: true,
            stale: false,
        }
    }

    fn validate(&mut self, response: &WatchSessionResponse) -> Result<(), ClientError> {
        if self.stale {
            return Err(ClientError::WatchInvariant("frame_after_resync"));
        }
        let frame = response
            .frame
            .as_ref()
            .ok_or(ClientError::WatchInvariant("missing_frame"))?;
        let payload = frame
            .frame
            .as_ref()
            .ok_or(ClientError::WatchInvariant("missing_payload"))?;
        if matches!(payload, session_watch_frame::Frame::Error(_)) {
            self.stale = true;
            self.first_frame = false;
            return Ok(());
        }
        let cursor = frame
            .cursor
            .as_ref()
            .ok_or(ClientError::WatchInvariant("missing_cursor"))?;
        if cursor.authority_epoch != self.authority_epoch {
            return Err(ClientError::WatchInvariant("authority_epoch_changed"));
        }
        match &self.stream_id {
            Some(stream_id) if stream_id != &cursor.stream_id => {
                return Err(ClientError::WatchInvariant("stream_changed"));
            }
            None if cursor.stream_id.is_empty() => {
                return Err(ClientError::WatchInvariant("missing_stream_id"));
            }
            None => self.stream_id = Some(cursor.stream_id.clone()),
            Some(_) => {}
        }
        if cursor.sequence != self.sequence.saturating_add(1) {
            return Err(ClientError::WatchInvariant("sequence_gap"));
        }
        match payload {
            session_watch_frame::Frame::Snapshot(snapshot) => {
                if !self.first_frame || snapshot.snapshot_fingerprint.is_empty() {
                    return Err(ClientError::WatchInvariant("unexpected_snapshot"));
                }
                self.revision = snapshot
                    .session
                    .as_ref()
                    .ok_or(ClientError::WatchInvariant("missing_snapshot"))?
                    .revision;
            }
            session_watch_frame::Frame::Delta(delta) => {
                self.require_snapshot()?;
                if delta.base_revision != self.revision
                    || delta.new_revision != delta.base_revision.saturating_add(1)
                {
                    return Err(ClientError::WatchInvariant("revision_gap"));
                }
                self.revision = delta.new_revision;
            }
            session_watch_frame::Frame::Heartbeat(heartbeat) => {
                self.require_snapshot()?;
                if heartbeat.current_revision != self.revision {
                    return Err(ClientError::WatchInvariant("heartbeat_revision_mismatch"));
                }
            }
            session_watch_frame::Frame::ResyncRequired(_) => {
                self.require_snapshot()?;
                self.stale = true;
            }
            session_watch_frame::Frame::Error(_) => {
                unreachable!("handled before cursor validation")
            }
        }
        self.sequence = cursor.sequence;
        self.first_frame = false;
        Ok(())
    }

    fn require_snapshot(&self) -> Result<(), ClientError> {
        if self.first_frame {
            Err(ClientError::WatchInvariant("first_frame_not_snapshot"))
        } else {
            Ok(())
        }
    }
}

fn timestamp(unix_ms: u64) -> prost_types::Timestamp {
    prost_types::Timestamp {
        seconds: (unix_ms / 1_000).try_into().unwrap_or(i64::MAX),
        nanos: ((unix_ms % 1_000) * 1_000_000) as i32,
    }
}

fn validate_command_accepted(
    accepted: Option<&CommandAccepted>,
    expected_command_id: &str,
    expected_correlation_id: &str,
) -> Result<(), ClientError> {
    let accepted = accepted.ok_or(ClientError::MalformedResponse("command acknowledgement"))?;
    if accepted.command_id != expected_command_id
        || accepted.correlation_id != expected_correlation_id
        || accepted.operation_id.trim().is_empty()
        || accepted.accepted_revision == 0
    {
        return Err(ClientError::MalformedResponse("command acknowledgement"));
    }
    Ok(())
}

fn unix_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("local IPC client configuration is invalid")]
    InvalidConfiguration,
    #[error("secure random generation failed")]
    RandomUnavailable,
    #[error("Node returned an incomplete {0}")]
    MalformedResponse(&'static str),
    #[error("Desktop and Node do not share a compatible protocol")]
    ProtocolIncompatible,
    #[error("Node authority changed during bootstrap")]
    AuthorityEpochMismatch,
    #[error("Node watch stream violated the local IPC contract: {0}")]
    WatchInvariant(&'static str),
    #[error("local IPC {0} request exceeded its deadline")]
    DeadlineExceeded(&'static str),
    #[error("Node rejected local IPC request: {0:?}")]
    Remote(ErrorEnvelope),
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error("local IPC request failed: {0}")]
    Grpc(#[from] tonic::Status),
}

impl ClientError {
    #[must_use]
    pub fn code(&self) -> &str {
        match self {
            Self::InvalidConfiguration => "ipc_client_configuration_invalid",
            Self::RandomUnavailable => "ipc_random_unavailable",
            Self::MalformedResponse(_) => "ipc_response_malformed",
            Self::ProtocolIncompatible => "ipc_protocol_incompatible",
            Self::AuthorityEpochMismatch => "ipc_authority_epoch_changed",
            Self::WatchInvariant(code) => watch_invariant_code(code),
            Self::DeadlineExceeded(_) => "ipc_request_deadline_exceeded",
            Self::Remote(error) => &error.code,
            Self::Transport(TransportError::UnsupportedPlatform) => "ipc_platform_unsupported",
            Self::Transport(TransportError::InvalidInstallationId) => {
                "ipc_installation_identity_invalid"
            }
            Self::Transport(TransportError::PeerIdentityUnavailable) => {
                "ipc_peer_identity_unavailable"
            }
            Self::Transport(TransportError::PeerUserMismatch) => "ipc_peer_user_mismatch",
            Self::Transport(TransportError::InvalidSecurityDescriptor) => {
                "ipc_security_descriptor_invalid"
            }
            Self::Transport(TransportError::Io(_)) => "ipc_transport_io",
            Self::Transport(TransportError::Channel(_)) => "ipc_node_unavailable",
            Self::Grpc(status) => grpc_code(status.code()),
        }
    }

    #[must_use]
    pub fn retryable(&self) -> bool {
        match self {
            Self::RandomUnavailable
            | Self::AuthorityEpochMismatch
            | Self::WatchInvariant(_)
            | Self::DeadlineExceeded(_) => true,
            Self::Remote(error) => error.retryable,
            Self::Transport(TransportError::Io(_) | TransportError::Channel(_)) => true,
            Self::Grpc(status) => matches!(
                status.code(),
                tonic::Code::Cancelled
                    | tonic::Code::Unknown
                    | tonic::Code::DeadlineExceeded
                    | tonic::Code::Aborted
                    | tonic::Code::Unavailable
            ),
            _ => false,
        }
    }

    #[must_use]
    pub fn user_action_required(&self) -> bool {
        match self {
            Self::Remote(error) => error.user_action_required,
            Self::InvalidConfiguration
            | Self::MalformedResponse(_)
            | Self::ProtocolIncompatible
            | Self::Transport(
                TransportError::UnsupportedPlatform
                | TransportError::InvalidInstallationId
                | TransportError::PeerUserMismatch
                | TransportError::InvalidSecurityDescriptor,
            ) => true,
            _ => false,
        }
    }

    #[must_use]
    pub fn node_start_candidate(&self) -> bool {
        matches!(self, Self::Transport(TransportError::Channel(_)))
    }
}

fn grpc_code(code: tonic::Code) -> &'static str {
    match code {
        tonic::Code::Cancelled => "ipc_request_cancelled",
        tonic::Code::DeadlineExceeded => "ipc_request_deadline_exceeded",
        tonic::Code::Unavailable => "ipc_node_unavailable",
        tonic::Code::Unauthenticated => "ipc_unauthenticated",
        tonic::Code::PermissionDenied => "ipc_permission_denied",
        _ => "ipc_grpc_failure",
    }
}

fn watch_invariant_code(code: &str) -> &'static str {
    match code {
        "missing_frame" => "ipc_watch_frame_missing",
        "missing_payload" => "ipc_watch_payload_missing",
        "missing_cursor" => "ipc_watch_cursor_missing",
        "authority_epoch_changed" => "ipc_watch_authority_epoch_changed",
        "stream_changed" => "ipc_watch_stream_changed",
        "missing_stream_id" => "ipc_watch_stream_id_missing",
        "sequence_overflow" => "ipc_watch_sequence_overflow",
        "sequence_gap" => "ipc_watch_sequence_gap",
        "unexpected_snapshot" => "ipc_watch_snapshot_unexpected",
        "missing_snapshot" => "ipc_watch_snapshot_missing",
        "snapshot_epoch_mismatch" => "ipc_watch_snapshot_epoch_mismatch",
        "snapshot_fingerprint_missing" => "ipc_watch_snapshot_fingerprint_missing",
        "revision_gap" => "ipc_watch_revision_gap",
        "heartbeat_revision_mismatch" => "ipc_watch_heartbeat_revision_mismatch",
        "first_frame_not_snapshot" => "ipc_watch_first_frame_not_snapshot",
        "frame_after_resync" => "ipc_watch_frame_after_resync",
        _ => "ipc_watch_invariant_failed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::dennett::control::v1::{
        SystemDelta, SystemSnapshot as WireSystemSnapshot, SystemWatchFrame,
    };
    use crate::protocol::dennett::sync::v1::{
        ResyncReason, ResyncRequired, WatchCursor, WatchHeartbeat,
    };

    fn bootstrap(revision: u64) -> BootstrapSnapshot {
        BootstrapSnapshot {
            revision,
            authority_epoch: 7,
            ..Default::default()
        }
    }

    fn response(sequence: u64, frame: system_watch_frame::Frame) -> WatchResponse {
        WatchResponse {
            frame: Some(SystemWatchFrame {
                cursor: Some(WatchCursor {
                    stream_id: "stream-1".to_owned(),
                    sequence,
                    authority_epoch: 7,
                }),
                frame: Some(frame),
            }),
        }
    }

    fn snapshot(sequence: u64, revision: u64) -> WatchResponse {
        response(
            sequence,
            system_watch_frame::Frame::Snapshot(WireSystemSnapshot {
                bootstrap: Some(bootstrap(revision)),
                snapshot_fingerprint: vec![1, 2, 3],
            }),
        )
    }

    #[tokio::test]
    async fn zero_deadline_is_rejected_and_deadline_errors_are_retryable() {
        let mut config = ClientConfig::m01("installation", "device", "test");
        config.rpc_deadline = Duration::ZERO;
        assert!(matches!(
            AuthenticatedSystemClient::connect(config).await,
            Err(ClientError::InvalidConfiguration)
        ));

        let error = ClientError::DeadlineExceeded("handshake");
        assert_eq!(error.code(), "ipc_request_deadline_exceeded");
        assert!(error.retryable());
        assert!(!error.user_action_required());
        assert!(!error.node_start_candidate());
    }

    #[test]
    fn command_acknowledgement_must_match_the_request_and_be_durable() {
        let valid = CommandAccepted {
            command_id: "command-1".to_owned(),
            correlation_id: "correlation-1".to_owned(),
            operation_id: "operation-1".to_owned(),
            accepted_revision: 1,
        };
        validate_command_accepted(Some(&valid), "command-1", "correlation-1")
            .expect("matching durable acknowledgement");

        for invalid in [
            CommandAccepted {
                command_id: "different-command".to_owned(),
                ..valid.clone()
            },
            CommandAccepted {
                correlation_id: "different-correlation".to_owned(),
                ..valid.clone()
            },
            CommandAccepted {
                operation_id: "  ".to_owned(),
                ..valid.clone()
            },
            CommandAccepted {
                accepted_revision: 0,
                ..valid.clone()
            },
        ] {
            assert!(matches!(
                validate_command_accepted(Some(&invalid), "command-1", "correlation-1"),
                Err(ClientError::MalformedResponse("command acknowledgement"))
            ));
        }
        assert!(matches!(
            validate_command_accepted(None, "command-1", "correlation-1"),
            Err(ClientError::MalformedResponse("command acknowledgement"))
        ));
    }

    #[test]
    fn validator_accepts_snapshot_delta_heartbeat_and_resync() {
        let mut state = WatchState::new(&bootstrap(3));
        state.validate(&snapshot(1, 3)).expect("snapshot");
        state
            .validate(&response(
                2,
                system_watch_frame::Frame::Delta(SystemDelta {
                    base_revision: 3,
                    new_revision: 4,
                    mutations: Vec::new(),
                }),
            ))
            .expect("delta");
        state
            .validate(&response(
                3,
                system_watch_frame::Frame::Heartbeat(WatchHeartbeat {
                    observed_at: None,
                    current_revision: 4,
                }),
            ))
            .expect("heartbeat");
        state
            .validate(&response(
                4,
                system_watch_frame::Frame::ResyncRequired(ResyncRequired {
                    reason: ResyncReason::SequenceGap as i32,
                    current_revision: 5,
                    snapshot_required: true,
                }),
            ))
            .expect("resync signal");
        assert!(matches!(
            state.validate(&snapshot(5, 5)),
            Err(ClientError::WatchInvariant("frame_after_resync"))
        ));
    }

    #[test]
    fn validator_rejects_delta_before_snapshot_and_sequence_gap() {
        let mut state = WatchState::new(&bootstrap(3));
        assert!(matches!(
            state.validate(&response(
                1,
                system_watch_frame::Frame::Delta(SystemDelta {
                    base_revision: 3,
                    new_revision: 4,
                    mutations: Vec::new(),
                }),
            )),
            Err(ClientError::WatchInvariant("first_frame_not_snapshot"))
        ));

        let mut state = WatchState::new(&bootstrap(3));
        state.validate(&snapshot(1, 3)).expect("snapshot");
        assert!(matches!(
            state.validate(&response(
                3,
                system_watch_frame::Frame::Heartbeat(WatchHeartbeat {
                    observed_at: None,
                    current_revision: 3,
                }),
            )),
            Err(ClientError::WatchInvariant("sequence_gap"))
        ));
    }

    #[test]
    fn validator_rejects_revision_and_authority_gaps() {
        let mut state = WatchState::new(&bootstrap(3));
        state.validate(&snapshot(1, 3)).expect("snapshot");
        assert!(matches!(
            state.validate(&response(
                2,
                system_watch_frame::Frame::Delta(SystemDelta {
                    base_revision: 2,
                    new_revision: 4,
                    mutations: Vec::new(),
                }),
            )),
            Err(ClientError::WatchInvariant("revision_gap"))
        ));

        let mut changed_epoch = snapshot(1, 3);
        changed_epoch
            .frame
            .as_mut()
            .expect("frame")
            .cursor
            .as_mut()
            .expect("cursor")
            .authority_epoch = 8;
        let mut state = WatchState::new(&bootstrap(3));
        assert!(matches!(
            state.validate(&changed_epoch),
            Err(ClientError::WatchInvariant("authority_epoch_changed"))
        ));
    }

    #[test]
    fn validator_preserves_terminal_error_without_a_cursor() {
        let response = WatchResponse {
            frame: Some(SystemWatchFrame {
                cursor: None,
                frame: Some(system_watch_frame::Frame::Error(ErrorEnvelope {
                    code: "watch_unavailable".to_owned(),
                    retryable: false,
                    ..Default::default()
                })),
            }),
        };
        let mut state = WatchState::new(&bootstrap(3));
        state.validate(&response).expect("terminal error frame");
        assert!(matches!(
            state.validate(&snapshot(1, 3)),
            Err(ClientError::WatchInvariant("frame_after_resync"))
        ));
    }
}
