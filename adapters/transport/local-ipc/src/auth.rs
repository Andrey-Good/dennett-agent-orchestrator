use crate::protocol::dennett::control::v1::{ClientHello, CompatibilityMode, ServerWelcome};
use crate::{DEFAULT_MAX_MESSAGE_BYTES, M01_PROTOCOL_VERSION, PeerIdentity};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use subtle::ConstantTimeEq;

const DEFAULT_BOOTSTRAP_CAPABILITY_TTL: Duration = Duration::from_secs(30);
const DEFAULT_UI_SESSION_TTL: Duration = Duration::from_secs(12 * 60 * 60);
const CHALLENGE_BYTES: usize = 32;
const PROOF_BYTES: usize = 32;

#[derive(Clone, Debug)]
pub struct HandshakePolicy {
    pub installation_id: String,
    pub client_component: String,
    pub node_version: String,
    pub authority_epoch: u64,
    pub supported_protocol_versions: Vec<u32>,
    pub enabled_features: Vec<String>,
    pub max_message_bytes: u64,
    pub bootstrap_capability_ttl: Duration,
    pub ui_session_ttl: Duration,
}

impl HandshakePolicy {
    #[must_use]
    pub fn m01(
        installation_id: impl Into<String>,
        node_version: impl Into<String>,
        authority_epoch: u64,
    ) -> Self {
        Self {
            installation_id: installation_id.into(),
            client_component: "dennett-desktop-shell".to_owned(),
            node_version: node_version.into(),
            authority_epoch,
            supported_protocol_versions: vec![M01_PROTOCOL_VERSION],
            enabled_features: vec!["system-watch".to_owned()],
            max_message_bytes: DEFAULT_MAX_MESSAGE_BYTES,
            bootstrap_capability_ttl: DEFAULT_BOOTSTRAP_CAPABILITY_TTL,
            ui_session_ttl: DEFAULT_UI_SESSION_TTL,
        }
    }
}

#[derive(Clone)]
pub struct SessionRegistry {
    policy: Arc<HandshakePolicy>,
    state: Arc<Mutex<RegistryState>>,
}

impl SessionRegistry {
    #[must_use]
    pub fn new(policy: HandshakePolicy) -> Self {
        Self {
            policy: Arc::new(policy),
            state: Arc::new(Mutex::new(RegistryState::default())),
        }
    }

    #[must_use]
    pub fn policy(&self) -> &HandshakePolicy {
        &self.policy
    }

    pub(crate) fn issue(
        &self,
        peer: &PeerIdentity,
        hello: ClientHello,
    ) -> Result<ServerWelcome, AuthError> {
        self.validate_hello(&hello)?;
        let protocol_version = hello
            .protocol_versions
            .iter()
            .copied()
            .filter(|candidate| self.policy.supported_protocol_versions.contains(candidate))
            .max()
            .ok_or(AuthError::ProtocolIncompatible)?;

        let challenge_digest = digest(&hello.session_challenge);
        let now = Instant::now();
        let mut state = self.state.lock().expect("session registry poisoned");
        state.purge(now);
        if state.used_challenges.contains_key(&challenge_digest) {
            return Err(AuthError::ChallengeReplay);
        }

        let proof = random_bytes::<PROOF_BYTES>()?;
        let session_id = random_hex(16)?;
        state
            .used_challenges
            .insert(challenge_digest, now + self.policy.bootstrap_capability_ttl);
        state.sessions.insert(
            session_id.clone(),
            SessionRecord {
                peer: peer.clone(),
                installation_id: hello.installation_id,
                authority_epoch: self.policy.authority_epoch,
                proof,
                bootstrap_expires_at: now + self.policy.bootstrap_capability_ttl,
                session_expires_at: now + self.policy.ui_session_ttl,
                phase: SessionPhase::AwaitingBootstrap,
            },
        );

        let enabled_features = self
            .policy
            .enabled_features
            .iter()
            .filter(|feature| hello.requested_features.contains(feature))
            .cloned()
            .collect();
        Ok(ServerWelcome {
            protocol_version,
            node_version: self.policy.node_version.clone(),
            authority_epoch_seen: self.policy.authority_epoch,
            enabled_features,
            session_proof: proof.to_vec(),
            resync_required: false,
            compatibility_mode: CompatibilityMode::Full as i32,
            max_message_bytes: self.policy.max_message_bytes,
            client_session_id: session_id,
        })
    }

    pub(crate) fn consume_bootstrap(
        &self,
        peer: &PeerIdentity,
        client_session_id: &str,
        session_proof: &[u8],
    ) -> Result<AuthenticatedSession, AuthError> {
        let now = Instant::now();
        let mut state = self.state.lock().expect("session registry poisoned");
        state.purge(now);
        let record = state
            .sessions
            .get_mut(client_session_id)
            .ok_or(AuthError::SessionUnknown)?;
        record.validate_binding(
            peer,
            &self.policy.installation_id,
            self.policy.authority_epoch,
        )?;
        if record.phase != SessionPhase::AwaitingBootstrap {
            return Err(AuthError::ProofAlreadyConsumed);
        }
        if now >= record.bootstrap_expires_at {
            return Err(AuthError::ProofExpired);
        }
        if session_proof.len() != PROOF_BYTES || record.proof.ct_eq(session_proof).unwrap_u8() != 1
        {
            return Err(AuthError::ProofInvalid);
        }
        record.proof.fill(0);
        record.phase = SessionPhase::Active;
        Ok(record.authenticated(client_session_id))
    }

    pub(crate) fn authorize_active(
        &self,
        peer: &PeerIdentity,
        client_session_id: &str,
    ) -> Result<AuthenticatedSession, AuthError> {
        let now = Instant::now();
        let mut state = self.state.lock().expect("session registry poisoned");
        state.purge(now);
        let record = state
            .sessions
            .get(client_session_id)
            .ok_or(AuthError::SessionUnknown)?;
        record.validate_binding(
            peer,
            &self.policy.installation_id,
            self.policy.authority_epoch,
        )?;
        if record.phase != SessionPhase::Active {
            return Err(AuthError::BootstrapRequired);
        }
        Ok(record.authenticated(client_session_id))
    }

    fn validate_hello(&self, hello: &ClientHello) -> Result<(), AuthError> {
        if hello.client_component != self.policy.client_component
            || hello.component_version.is_empty()
            || hello.device_id.is_empty()
        {
            return Err(AuthError::ClientIdentityInvalid);
        }
        if hello.installation_id != self.policy.installation_id {
            return Err(AuthError::InstallationMismatch);
        }
        if hello.session_challenge.len() != CHALLENGE_BYTES {
            return Err(AuthError::ChallengeInvalid);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AuthenticatedSession {
    pub client_session_id: String,
    pub authority_epoch: u64,
}

#[derive(Default)]
struct RegistryState {
    sessions: HashMap<String, SessionRecord>,
    used_challenges: HashMap<[u8; 32], Instant>,
}

impl RegistryState {
    fn purge(&mut self, now: Instant) {
        self.sessions
            .retain(|_, record| now < record.session_expires_at);
        self.used_challenges.retain(|_, expiry| now < *expiry);
    }
}

struct SessionRecord {
    peer: PeerIdentity,
    installation_id: String,
    authority_epoch: u64,
    proof: [u8; PROOF_BYTES],
    bootstrap_expires_at: Instant,
    session_expires_at: Instant,
    phase: SessionPhase,
}

impl SessionRecord {
    fn validate_binding(
        &self,
        peer: &PeerIdentity,
        installation_id: &str,
        authority_epoch: u64,
    ) -> Result<(), AuthError> {
        if &self.peer != peer {
            return Err(AuthError::ConnectionMismatch);
        }
        if self.installation_id != installation_id {
            return Err(AuthError::InstallationMismatch);
        }
        if self.authority_epoch != authority_epoch {
            return Err(AuthError::AuthorityEpochChanged);
        }
        Ok(())
    }

    fn authenticated(&self, client_session_id: &str) -> AuthenticatedSession {
        AuthenticatedSession {
            client_session_id: client_session_id.to_owned(),
            authority_epoch: self.authority_epoch,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SessionPhase {
    AwaitingBootstrap,
    Active,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum AuthError {
    #[error("client identity is invalid")]
    ClientIdentityInvalid,
    #[error("installation identity does not match this Node")]
    InstallationMismatch,
    #[error("no compatible protocol version exists")]
    ProtocolIncompatible,
    #[error("session challenge is invalid")]
    ChallengeInvalid,
    #[error("session challenge has already been used")]
    ChallengeReplay,
    #[error("secure random generation failed")]
    RandomUnavailable,
    #[error("authenticated UI session is unknown or expired")]
    SessionUnknown,
    #[error("bootstrap capability has expired")]
    ProofExpired,
    #[error("bootstrap capability is invalid")]
    ProofInvalid,
    #[error("bootstrap capability has already been consumed")]
    ProofAlreadyConsumed,
    #[error("bootstrap must succeed before watch")]
    BootstrapRequired,
    #[error("authenticated session is bound to another connection")]
    ConnectionMismatch,
    #[error("authority epoch changed")]
    AuthorityEpochChanged,
}

impl AuthError {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::ClientIdentityInvalid => "ipc_client_identity_invalid",
            Self::InstallationMismatch => "ipc_installation_mismatch",
            Self::ProtocolIncompatible => "ipc_protocol_incompatible",
            Self::ChallengeInvalid => "ipc_challenge_invalid",
            Self::ChallengeReplay => "ipc_challenge_replay",
            Self::RandomUnavailable => "ipc_random_unavailable",
            Self::SessionUnknown => "ipc_session_unknown",
            Self::ProofExpired => "ipc_proof_expired",
            Self::ProofInvalid => "ipc_proof_invalid",
            Self::ProofAlreadyConsumed => "ipc_proof_consumed",
            Self::BootstrapRequired => "ipc_bootstrap_required",
            Self::ConnectionMismatch => "ipc_connection_mismatch",
            Self::AuthorityEpochChanged => "ipc_authority_epoch_changed",
        }
    }

    pub(crate) fn retryable(self) -> bool {
        matches!(
            self,
            Self::ChallengeReplay
                | Self::SessionUnknown
                | Self::ProofExpired
                | Self::ProofAlreadyConsumed
                | Self::BootstrapRequired
                | Self::ConnectionMismatch
                | Self::AuthorityEpochChanged
        )
    }
}

fn random_bytes<const N: usize>() -> Result<[u8; N], AuthError> {
    let mut bytes = [0_u8; N];
    getrandom::fill(&mut bytes).map_err(|_| AuthError::RandomUnavailable)?;
    Ok(bytes)
}

fn random_hex(byte_count: usize) -> Result<String, AuthError> {
    let mut bytes = vec![0_u8; byte_count];
    getrandom::fill(&mut bytes).map_err(|_| AuthError::RandomUnavailable)?;
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(byte_count * 2);
    for byte in bytes {
        value.push(HEX[(byte >> 4) as usize] as char);
        value.push(HEX[(byte & 0x0f) as usize] as char);
    }
    Ok(value)
}

fn digest(value: &[u8]) -> [u8; 32] {
    Sha256::digest(value).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer(connection: &str) -> PeerIdentity {
        PeerIdentity {
            process_id: 42,
            user_sid: "S-1-5-21-test".to_owned(),
            connection_id: connection.to_owned(),
        }
    }

    fn hello(challenge: u8) -> ClientHello {
        ClientHello {
            client_component: "dennett-desktop-shell".to_owned(),
            component_version: "0.1.0".to_owned(),
            protocol_versions: vec![1],
            installation_id: "installation-1".to_owned(),
            device_id: "device-1".to_owned(),
            session_challenge: vec![challenge; CHALLENGE_BYTES],
            requested_features: vec!["system-watch".to_owned(), "future".to_owned()],
        }
    }

    #[test]
    fn proof_is_single_use_and_connection_bound() {
        let registry = SessionRegistry::new(HandshakePolicy::m01("installation-1", "node", 7));
        let welcome = registry.issue(&peer("a"), hello(1)).expect("handshake");
        assert_eq!(welcome.enabled_features, vec!["system-watch"]);
        assert!(matches!(
            registry.consume_bootstrap(
                &peer("b"),
                &welcome.client_session_id,
                &welcome.session_proof
            ),
            Err(AuthError::ConnectionMismatch)
        ));
        registry
            .consume_bootstrap(
                &peer("a"),
                &welcome.client_session_id,
                &welcome.session_proof,
            )
            .expect("bootstrap");
        assert!(matches!(
            registry.consume_bootstrap(
                &peer("a"),
                &welcome.client_session_id,
                &welcome.session_proof
            ),
            Err(AuthError::ProofAlreadyConsumed)
        ));
        registry
            .authorize_active(&peer("a"), &welcome.client_session_id)
            .expect("active session");
    }

    #[test]
    fn incompatible_installation_protocol_and_replayed_challenge_are_rejected() {
        let registry = SessionRegistry::new(HandshakePolicy::m01("installation-1", "node", 7));
        let mut wrong_installation = hello(2);
        wrong_installation.installation_id = "other".to_owned();
        assert!(matches!(
            registry.issue(&peer("a"), wrong_installation),
            Err(AuthError::InstallationMismatch)
        ));

        let mut wrong_protocol = hello(3);
        wrong_protocol.protocol_versions = vec![99];
        assert!(matches!(
            registry.issue(&peer("a"), wrong_protocol),
            Err(AuthError::ProtocolIncompatible)
        ));

        registry.issue(&peer("a"), hello(4)).expect("first use");
        assert!(matches!(
            registry.issue(&peer("a"), hello(4)),
            Err(AuthError::ChallengeReplay)
        ));
    }
}
