use crate::protocol::dennett::common::v1::ErrorEnvelope;
use crate::protocol::dennett::control::v1::bootstrap_response;
use crate::protocol::dennett::control::v1::handshake_response;
use crate::protocol::dennett::control::v1::system_service_client::SystemServiceClient;
use crate::protocol::dennett::control::v1::{
    BootstrapRequest, BootstrapSnapshot, ClientHello, CompatibilityMode, HandshakeRequest,
    WatchRequest, WatchResponse,
};
use crate::transport::connect_channel;
use crate::{LocalEndpoint, M01_PROTOCOL_VERSION, TransportError};
use tonic::codec::Streaming;
use tonic::transport::Channel;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientConfig {
    pub installation_id: String,
    pub device_id: String,
    pub component_version: String,
    pub requested_features: Vec<String>,
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
            requested_features: vec!["system-watch".to_owned()],
        }
    }
}

pub struct AuthenticatedSystemClient {
    inner: SystemServiceClient<Channel>,
    client_session_id: String,
    bootstrap: BootstrapSnapshot,
}

impl AuthenticatedSystemClient {
    pub async fn connect(config: ClientConfig) -> Result<Self, ClientError> {
        if config.device_id.is_empty() || config.component_version.is_empty() {
            return Err(ClientError::InvalidConfiguration);
        }
        let endpoint = LocalEndpoint::for_installation(config.installation_id.clone())?;
        let channel = connect_channel(&endpoint).await?;
        let mut inner = SystemServiceClient::new(channel);
        let mut challenge = vec![0_u8; 32];
        getrandom::fill(&mut challenge).map_err(|_| ClientError::RandomUnavailable)?;
        let response = inner
            .handshake(HandshakeRequest {
                hello: Some(ClientHello {
                    client_component: "dennett-desktop-shell".to_owned(),
                    component_version: config.component_version,
                    protocol_versions: vec![M01_PROTOCOL_VERSION],
                    installation_id: config.installation_id,
                    device_id: config.device_id,
                    session_challenge: challenge,
                    requested_features: config.requested_features,
                }),
            })
            .await?
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
        {
            return Err(ClientError::ProtocolIncompatible);
        }

        let client_session_id = welcome.client_session_id;
        let response = inner
            .bootstrap(BootstrapRequest {
                session_proof: welcome.session_proof,
                known_revision: None,
                client_session_id: client_session_id.clone(),
            })
            .await?
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
            client_session_id,
            bootstrap,
        })
    }

    #[must_use]
    pub fn bootstrap(&self) -> &BootstrapSnapshot {
        &self.bootstrap
    }

    pub async fn watch(&mut self) -> Result<Streaming<WatchResponse>, ClientError> {
        Ok(self
            .inner
            .watch(WatchRequest {
                client_session_id: self.client_session_id.clone(),
                known_revision: Some(self.bootstrap.revision),
            })
            .await?
            .into_inner())
    }
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
    #[error("Node rejected local IPC request: {0:?}")]
    Remote(ErrorEnvelope),
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error("local IPC request failed: {0}")]
    Grpc(#[from] tonic::Status),
}
