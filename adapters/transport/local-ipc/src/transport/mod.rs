use sha2::{Digest, Sha256};

#[cfg(windows)]
mod windows;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PeerIdentity {
    pub process_id: u32,
    pub user_sid: String,
    pub connection_id: String,
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
