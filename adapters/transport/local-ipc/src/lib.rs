//! Authenticated, versioned local IPC between Dennett Desktop and Dennett Node.

pub mod protocol;

#[cfg(feature = "server")]
mod auth;
#[cfg(feature = "client")]
mod client;
#[cfg(feature = "server")]
mod service;
mod transport;

#[cfg(feature = "server")]
pub use auth::{HandshakePolicy, SessionRegistry};
#[cfg(feature = "client")]
pub use client::{AuthenticatedSystemClient, AuthenticatedSystemWatch, ClientConfig, ClientError};
#[cfg(feature = "server")]
pub use service::{SystemServiceAdapter, run_system_server};
pub use transport::{LocalEndpoint, PeerIdentity, TransportError};

pub const M01_PROTOCOL_VERSION: u32 = 1;
pub const DEFAULT_MAX_MESSAGE_BYTES: u64 = 4 * 1024 * 1024;
