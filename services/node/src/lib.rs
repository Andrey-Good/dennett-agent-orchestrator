use dennett_head::system::{SystemProjection, SystemSnapshot};
use dennett_local_ipc::{
    HandshakePolicy, LocalEndpoint, SessionRegistry, SystemServiceAdapter, TransportError,
    run_system_server,
};
use std::sync::Arc;

pub const INSTALLATION_ID_ENV: &str = "DENNETT_INSTALLATION_ID";
pub const AUTHORITY_EPOCH_ENV: &str = "DENNETT_AUTHORITY_EPOCH";
const SYSTEM_WATCH_CAPACITY: usize = 128;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeConfig {
    pub installation_id: String,
    pub authority_epoch: u64,
    pub node_version: String,
}

impl NodeConfig {
    pub fn from_environment() -> Result<Self, NodeConfigError> {
        let installation_id = std::env::var(INSTALLATION_ID_ENV)
            .map_err(|_| NodeConfigError::MissingInstallationIdentity)?;
        let authority_epoch = std::env::var(AUTHORITY_EPOCH_ENV)
            .ok()
            .map(|value| value.parse::<u64>())
            .transpose()
            .map_err(|_| NodeConfigError::InvalidAuthorityEpoch)?
            .unwrap_or(1);
        Self::new(installation_id, authority_epoch, env!("CARGO_PKG_VERSION"))
    }

    pub fn new(
        installation_id: impl Into<String>,
        authority_epoch: u64,
        node_version: impl Into<String>,
    ) -> Result<Self, NodeConfigError> {
        let installation_id = installation_id.into();
        LocalEndpoint::for_installation(installation_id.clone())
            .map_err(|_| NodeConfigError::InvalidInstallationIdentity)?;
        let node_version = node_version.into();
        if authority_epoch == 0 {
            return Err(NodeConfigError::InvalidAuthorityEpoch);
        }
        if node_version.is_empty() {
            return Err(NodeConfigError::InvalidNodeVersion);
        }
        Ok(Self {
            installation_id,
            authority_epoch,
            node_version,
        })
    }
}

pub async fn run<F>(config: NodeConfig, shutdown: F) -> Result<(), TransportError>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    tracing::info!(
        phase = "local_ipc_configuration",
        authority_epoch = config.authority_epoch,
        "validated Node local IPC configuration"
    );
    let endpoint = LocalEndpoint::for_installation(config.installation_id.clone())?;
    let projection = Arc::new(SystemProjection::new(
        SystemSnapshot::empty(config.authority_epoch),
        SYSTEM_WATCH_CAPACITY,
    ));
    let sessions = SessionRegistry::new(HandshakePolicy::m01(
        config.installation_id,
        config.node_version,
        config.authority_epoch,
    ));
    let service = SystemServiceAdapter::new(projection, sessions);
    tracing::info!(
        phase = "local_ipc_listen",
        "starting authenticated Node IPC"
    );
    run_system_server(endpoint, service, shutdown).await
}

#[derive(Debug, thiserror::Error)]
pub enum NodeConfigError {
    #[error("DENNETT_INSTALLATION_ID is required")]
    MissingInstallationIdentity,
    #[error("installation identity is invalid")]
    InvalidInstallationIdentity,
    #[error("authority epoch must be a positive integer")]
    InvalidAuthorityEpoch,
    #[error("Node version is invalid")]
    InvalidNodeVersion,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_rejects_invalid_identity_epoch_and_version() {
        assert!(matches!(
            NodeConfig::new("", 1, "0.1.0"),
            Err(NodeConfigError::InvalidInstallationIdentity)
        ));
        assert!(matches!(
            NodeConfig::new("install", 0, "0.1.0"),
            Err(NodeConfigError::InvalidAuthorityEpoch)
        ));
        assert!(matches!(
            NodeConfig::new("install", 1, ""),
            Err(NodeConfigError::InvalidNodeVersion)
        ));
    }
}
