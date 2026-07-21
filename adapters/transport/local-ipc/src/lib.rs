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
pub use client::{
    AuthenticatedSessionWatch, AuthenticatedSystemClient, AuthenticatedSystemWatch, ClientCommand,
    ClientConfig, ClientError, ClientSendTurnRequest,
};
#[cfg(feature = "server")]
pub use service::{
    SessionServiceAdapter, SystemServiceAdapter, run_local_server, run_system_server,
};
pub use transport::{LocalEndpoint, PeerIdentity, TransportError};

pub const M01_PROTOCOL_VERSION: u32 = 1;
// M01 snapshots carry bounded inline conversation text. Keep enough headroom for
// multi-megabyte local chats until the object/history paging contract lands.
pub const DEFAULT_MAX_MESSAGE_BYTES: u64 = 32 * 1024 * 1024;
pub const SYSTEM_WATCH_FEATURE: &str = "system-watch";
pub const SESSION_CONVERSATION_FEATURE: &str = "session-conversation-v1";
pub const COMPOSER_DRAFT_FEATURE: &str = "composer-draft-v1";
