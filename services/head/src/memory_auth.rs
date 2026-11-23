//! Scoped HMAC-SHA256 `dnt1.` bearer token issuer for `dennett-head`.
//!
//! Issues short-lived, least-privilege tokens strictly matching `dennett-memoryd`'s
//! `dnt1.<base64url-json-claims>.<base64url-hmac-tag>` wire format WITHOUT linking
//! `dennett-memory-core` into `dennett-head` (Architecture 80 §3.3, 81 §6).

use core::fmt;
use std::collections::BTreeSet;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use dennett_contracts::redact::REDACTED_PLACEHOLDER;
use dennett_contracts::{DennettError, MemoryCapability, MemoryScope, PrincipalId, TimestampMs};
use hmac::{Hmac, Mac};
use serde::Serialize;
use sha2::Sha256;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

/// Default audience expected by standalone `dennett-memoryd`.
pub const DEFAULT_MEMORY_AUDIENCE: &str = "dennett-memoryd";
/// Default issuer trusted by standalone `dennett-memoryd`.
pub const DEFAULT_MEMORY_ISSUER: &str = "dennett-standalone";
/// Version prefix for HMAC-SHA256 signed scoped tokens.
pub const SIGNED_TOKEN_PREFIX: &str = "dnt1";
/// Minimum byte length required for an HMAC signing secret (matches `memoryd`).
pub const MIN_HMAC_SECRET_BYTES: usize = 16;
/// Default validity lifetime for dynamically minted per-session/per-call tokens (2 hours).
pub const DEFAULT_TOKEN_TTL_MS: i64 = 2 * 60 * 60 * 1000;

#[derive(Debug, Serialize)]
struct HeadSignedTokenClaims {
    token_id: String,
    principal_id: PrincipalId,
    issuer: String,
    audience: String,
    allowed_scopes: BTreeSet<MemoryScope>,
    capabilities: BTreeSet<MemoryCapability>,
    issued_at: TimestampMs,
    expires_at: TimestampMs,
}

/// Cryptographic signer enabling `dennett-head` to mint least-privilege `dnt1.` tokens
/// dynamically for newly created project memory scopes without static token edits.
#[derive(Clone)]
pub struct MemoryTokenSigner {
    secret: Vec<u8>,
    issuer: String,
    audience: String,
}

impl fmt::Debug for MemoryTokenSigner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemoryTokenSigner")
            .field("secret", &REDACTED_PLACEHOLDER)
            .field("issuer", &self.issuer)
            .field("audience", &self.audience)
            .finish()
    }
}

impl MemoryTokenSigner {
    /// Creates a new [`MemoryTokenSigner`] from `secret` (must be at least 16 bytes).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `secret` is shorter than [`MIN_HMAC_SECRET_BYTES`].
    pub fn new(secret: impl Into<Vec<u8>>) -> Result<Self, DennettError> {
        let bytes = secret.into();
        if bytes.len() < MIN_HMAC_SECRET_BYTES {
            return Err(DennettError::InvalidInput {
                field: String::from("memory_hmac_secret"),
                reason: format!("must be at least {MIN_HMAC_SECRET_BYTES} bytes"),
            });
        }
        Ok(Self {
            secret: bytes,
            issuer: String::from(DEFAULT_MEMORY_ISSUER),
            audience: String::from(DEFAULT_MEMORY_AUDIENCE),
        })
    }

    /// Mints a short-lived signed `dnt1.` bearer token restricted strictly to `allowed_scopes`
    /// and `capabilities`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `allowed_scopes` or `capabilities` is empty,
    /// or [`DennettError::Internal`] on serialization/HMAC initialization failure.
    pub fn mint_scoped_token(
        &self,
        principal_id: PrincipalId,
        allowed_scopes: BTreeSet<MemoryScope>,
        capabilities: BTreeSet<MemoryCapability>,
    ) -> Result<String, DennettError> {
        if allowed_scopes.is_empty() {
            return Err(DennettError::InvalidInput {
                field: String::from("allowed_scopes"),
                reason: String::from("cannot mint a scoped token with empty allowed_scopes"),
            });
        }
        if capabilities.is_empty() {
            return Err(DennettError::InvalidInput {
                field: String::from("capabilities"),
                reason: String::from("cannot mint a scoped token with empty capabilities"),
            });
        }

        let issued_at = TimestampMs::now();
        let expires_at =
            TimestampMs::from_millis(issued_at.as_millis().saturating_add(DEFAULT_TOKEN_TTL_MS));
        let claims = HeadSignedTokenClaims {
            token_id: format!("head-{}", Uuid::new_v4().simple()),
            principal_id,
            issuer: self.issuer.clone(),
            audience: self.audience.clone(),
            allowed_scopes,
            capabilities,
            issued_at,
            expires_at,
        };

        let payload_json = serde_json::to_vec(&claims).map_err(|err| DennettError::Internal {
            detail: format!("failed serializing signed memory token claims: {err}"),
        })?;
        let payload_b64 = URL_SAFE_NO_PAD.encode(payload_json);
        let signing_input = format!("{SIGNED_TOKEN_PREFIX}.{payload_b64}");

        let mut mac =
            HmacSha256::new_from_slice(&self.secret).map_err(|_| DennettError::Internal {
                detail: String::from("failed initializing HMAC-SHA256 signer"),
            })?;
        mac.update(signing_input.as_bytes());
        let tag = mac.finalize().into_bytes();
        let tag_b64 = URL_SAFE_NO_PAD.encode(tag);

        Ok(format!("{signing_input}.{tag_b64}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signer_rejects_short_secret_and_mints_valid_dnt1_format() {
        assert!(MemoryTokenSigner::new(b"short".to_vec()).is_err());

        let signer =
            MemoryTokenSigner::new(b"0123456789abcdef0123456789abcdef".to_vec()).expect("signer");
        let mut scopes = BTreeSet::new();
        scopes.insert(MemoryScope::try_new("project:dynamic-1").expect("scope"));
        let mut caps = BTreeSet::new();
        caps.insert(MemoryCapability::Read);
        caps.insert(MemoryCapability::Write);

        let token = signer
            .mint_scoped_token(
                PrincipalId::try_new("agent:test-agent").expect("principal"),
                scopes,
                caps,
            )
            .expect("mint token");

        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], "dnt1");

        let decoded_payload = URL_SAFE_NO_PAD
            .decode(parts[1])
            .expect("decode payload base64url");
        let parsed: serde_json::Value =
            serde_json::from_slice(&decoded_payload).expect("valid JSON claims");
        assert_eq!(parsed["audience"], "dennett-memoryd");
        assert_eq!(parsed["issuer"], "dennett-standalone");
        assert_eq!(parsed["principal_id"], "agent:test-agent");
        assert_eq!(
            parsed["allowed_scopes"],
            serde_json::json!(["project:dynamic-1"])
        );

        // Verify HMAC tag independently.
        let sig = URL_SAFE_NO_PAD.decode(parts[2]).expect("decode sig");
        let mut mac =
            HmacSha256::new_from_slice(b"0123456789abcdef0123456789abcdef").expect("hmac");
        mac.update(format!("{}.{}", parts[0], parts[1]).as_bytes());
        assert!(mac.verify_slice(&sig).is_ok());

        // Debug redacts the secret.
        assert!(!format!("{signer:?}").contains("0123456789abcdef"));

        // Structural redaction scrubs minted `dnt1.` tokens even without caller-supplied extra_secrets.
        let diag = format!("memoryd auth failed for token {token}.");
        let redacted = dennett_contracts::redact::redact_secrets(&diag, &[]);
        assert!(!redacted.contains(&token));
        assert!(!redacted.contains(parts[1]));
        assert!(!redacted.contains(parts[2]));
        assert_eq!(redacted, "memoryd auth failed for token [REDACTED].");
        assert_eq!(
            dennett_contracts::redact::redact_secrets(&redacted, &[]),
            redacted
        );
    }
}
