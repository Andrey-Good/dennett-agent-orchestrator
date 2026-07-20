use sha2::{Digest, Sha256};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(windows)]
mod windows;

#[derive(Clone)]
pub struct PeerIdentity {
    pub process_id: u32,
    pub user_sid: String,
    pub connection_id: String,
    connection: Arc<ConnectionState>,
}

#[derive(Debug, Default)]
struct ConnectionState {
    handshake_started: AtomicBool,
    authenticated: AtomicBool,
    closed: AtomicBool,
}

impl PeerIdentity {
    #[cfg(any(windows, test))]
    pub(crate) fn new(process_id: u32, user_sid: String, connection_id: String) -> Self {
        Self {
            process_id,
            user_sid,
            connection_id,
            connection: Arc::new(ConnectionState::default()),
        }
    }

    pub(crate) fn claim_handshake(&self) -> bool {
        self.connection
            .handshake_started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub(crate) fn mark_authenticated(&self) {
        self.connection.authenticated.store(true, Ordering::Release);
    }

    #[cfg(windows)]
    pub(crate) fn is_authenticated(&self) -> bool {
        self.connection.authenticated.load(Ordering::Acquire)
    }

    #[cfg(any(windows, test))]
    pub(crate) fn mark_closed(&self) {
        self.connection.closed.store(true, Ordering::Release);
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.connection.closed.load(Ordering::Acquire)
    }
}

impl std::fmt::Debug for PeerIdentity {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PeerIdentity")
            .field("process_id", &self.process_id)
            .field("user_sid", &self.user_sid)
            .field("connection_id", &self.connection_id)
            .finish_non_exhaustive()
    }
}

impl PartialEq for PeerIdentity {
    fn eq(&self, other: &Self) -> bool {
        self.process_id == other.process_id
            && self.user_sid == other.user_sid
            && self.connection_id == other.connection_id
    }
}

impl Eq for PeerIdentity {}

impl Hash for PeerIdentity {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.process_id.hash(state);
        self.user_sid.hash(state);
        self.connection_id.hash(state);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalEndpoint {
    installation_id: String,
    pipe_name: String,
}

impl LocalEndpoint {
    pub fn for_installation(installation_id: impl Into<String>) -> Result<Self, TransportError> {
        let installation_id = installation_id.into();
        if installation_id.is_empty()
            || installation_id.len() > 256
            || installation_id.chars().any(char::is_control)
        {
            return Err(TransportError::InvalidInstallationId);
        }
        let digest = Sha256::digest(installation_id.as_bytes());
        let suffix = hex_prefix(&digest, 20);
        Ok(Self {
            installation_id,
            pipe_name: format!(r"\\.\pipe\dennett-{suffix}"),
        })
    }

    #[must_use]
    pub fn installation_id(&self) -> &str {
        &self.installation_id
    }

    #[cfg(any(windows, test))]
    pub(crate) fn pipe_name(&self) -> &str {
        &self.pipe_name
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("installation identity is invalid")]
    InvalidInstallationId,
    #[error("local IPC is unsupported on this platform")]
    UnsupportedPlatform,
    #[error("local IPC peer identity could not be verified")]
    PeerIdentityUnavailable,
    #[error("local IPC peer belongs to a different operating-system user")]
    PeerUserMismatch,
    #[error("local IPC security descriptor is invalid")]
    InvalidSecurityDescriptor,
    #[error("local IPC operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("local IPC channel failed: {0}")]
    Channel(#[from] tonic::transport::Error),
}

fn hex_prefix(bytes: &[u8], byte_count: usize) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(byte_count * 2);
    for byte in bytes.iter().take(byte_count) {
        result.push(HEX[(byte >> 4) as usize] as char);
        result.push(HEX[(byte & 0x0f) as usize] as char);
    }
    result
}

#[cfg(all(feature = "client", windows))]
pub(crate) async fn connect_channel(
    endpoint: &LocalEndpoint,
) -> Result<tonic::transport::Channel, TransportError> {
    windows::connect_channel(endpoint).await
}

#[cfg(all(feature = "client", not(windows)))]
pub(crate) async fn connect_channel(
    _endpoint: &LocalEndpoint,
) -> Result<tonic::transport::Channel, TransportError> {
    Err(TransportError::UnsupportedPlatform)
}

#[cfg(all(feature = "server", windows))]
pub(crate) use windows::secure_incoming;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_is_stable_and_does_not_embed_installation_identity() {
        let first = LocalEndpoint::for_installation("private-installation-id").expect("endpoint");
        let second = LocalEndpoint::for_installation("private-installation-id").expect("endpoint");
        assert_eq!(first, second);
        assert!(!first.pipe_name().contains("private-installation-id"));
        assert!(first.pipe_name().starts_with(r"\\.\pipe\dennett-"));
    }

    #[test]
    fn endpoint_rejects_empty_and_control_bearing_identity() {
        assert!(matches!(
            LocalEndpoint::for_installation(""),
            Err(TransportError::InvalidInstallationId)
        ));
        assert!(matches!(
            LocalEndpoint::for_installation("bad\nidentity"),
            Err(TransportError::InvalidInstallationId)
        ));
    }
}
