//! Standalone bearer credential verifier for `dennett-memoryd` (Architecture 80 §3.3, 81 §6, V06).
//!
//! Verifies either:
//! - static registry tokens (hashed to SHA-256 on load and compared in constant time), or
//! - HMAC-SHA256 signed scoped tokens (`dnt1.<base64url-json-claims>.<base64url-hmac-tag>`).
//!
//! Enforces principal identity, issuer, audience, allowed scopes, read/write capabilities,
//! expiry (`now < expires_at`), and revocation without ever trusting caller-supplied `--agent`
//! flags or request-body author labels.

use core::fmt;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::{Arc, RwLock};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use dennett_contracts::redact::{REDACTED_PLACEHOLDER, redact_secrets};
use dennett_contracts::{
    AuthPrincipal, DennettError, MemoryCapability, MemoryScope, PrincipalId, Sha256Hex, TimestampMs,
};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::fingerprint::sha256_bytes;

type HmacSha256 = Hmac<Sha256>;

/// Default audience required in signed standalone tokens.
pub const DEFAULT_AUDIENCE: &str = "dennett-memoryd";
/// Default issuer accepted by standalone memory deployments.
pub const DEFAULT_ISSUER: &str = "dennett-standalone";
/// Version prefix for HMAC-SHA256 signed scoped tokens.
pub const SIGNED_TOKEN_PREFIX: &str = "dnt1";
/// Minimum byte length required for an HMAC signing secret.
pub const MIN_HMAC_SECRET_BYTES: usize = 16;
/// Maximum permitted bearer token byte length accepted by the verifier.
pub const MAX_BEARER_TOKEN_BYTES: usize = 8 * 1024;

/// Verifies bearer credentials into a scoped [`AuthPrincipal`].
pub trait AuthVerifier: Send + Sync {
    /// Verifies `token` at wall-clock timestamp `now`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::Unauthorized`] if `token` is empty, malformed, unknown,
    /// fails cryptographic verification, is expired (`now >= expires_at`), or is revoked.
    fn verify_bearer(&self, token: &str, now: TimestampMs) -> Result<AuthPrincipal, DennettError>;
}

/// Parses an HTTP `Authorization` header and extracts the raw `Bearer` token strictly.
///
/// # Errors
///
/// Returns [`DennettError::Unauthorized`] if the scheme is not `Bearer` or the token is empty
/// or contains whitespace/control characters.
pub fn parse_bearer_header(header_value: &str) -> Result<&str, DennettError> {
    let trimmed = header_value.trim();
    let Some((scheme, rest)) = trimmed.split_once(|c: char| c.is_ascii_whitespace()) else {
        return Err(DennettError::Unauthorized {
            reason: String::from("expected `Authorization: Bearer <token>` header"),
        });
    };
    if !scheme.eq_ignore_ascii_case("bearer") {
        return Err(DennettError::Unauthorized {
            reason: String::from("unsupported authorization scheme; expected Bearer"),
        });
    }
    let token = rest.trim_start_matches([' ', '\t']);
    if token.is_empty()
        || token.len() > MAX_BEARER_TOKEN_BYTES
        || token
            .bytes()
            .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
    {
        return Err(DennettError::Unauthorized {
            reason: String::from("malformed bearer token"),
        });
    }
    Ok(token)
}

/// Claims payload embedded inside an HMAC-SHA256 signed `dnt1` scoped token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedTokenClaims {
    /// Unique non-secret identifier for audit and revocation.
    pub token_id: String,
    /// Authenticated principal identity.
    pub principal_id: PrincipalId,
    /// Token issuer (must match configured `allowed_issuers`).
    pub issuer: String,
    /// Target audience (must match configured `audience`, default `"dennett-memoryd"`).
    pub audience: String,
    /// Explicit allowlist of scopes granted to this token.
    pub allowed_scopes: BTreeSet<MemoryScope>,
    /// Capabilities granted within `allowed_scopes`.
    pub capabilities: BTreeSet<MemoryCapability>,
    /// Issue timestamp in UTC milliseconds.
    pub issued_at: TimestampMs,
    /// Expiry timestamp in UTC milliseconds (strictly required; token expires at `now >= expires_at`).
    pub expires_at: TimestampMs,
}

/// One static registry entry loaded from standalone auth configuration.
///
/// Exactly one of `token` (hashed immediately on load) or `token_sha256` must be set.
#[derive(Clone, Serialize, Deserialize)]
pub struct StaticTokenEntryConfig {
    /// Non-secret audit/revocation identifier.
    pub token_id: String,
    /// Plaintext token (optional; immediately hashed to SHA-256 on verifier load and never logged).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// Pre-hashed lowercase hex SHA-256 of the bearer token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_sha256: Option<Sha256Hex>,
    /// Verified principal identity.
    pub principal_id: PrincipalId,
    /// Issuer label (defaults to [`DEFAULT_ISSUER`]).
    #[serde(default = "default_issuer")]
    pub issuer: String,
    /// Allowed scopes for this credential.
    pub allowed_scopes: BTreeSet<MemoryScope>,
    /// Granted capabilities within `allowed_scopes`.
    pub capabilities: BTreeSet<MemoryCapability>,
    /// Optional expiry timestamp in UTC milliseconds (`now >= expires_at` rejects).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<TimestampMs>,
    /// Whether this token is statically revoked.
    #[serde(default)]
    pub revoked: bool,
}

impl fmt::Debug for StaticTokenEntryConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StaticTokenEntryConfig")
            .field("token_id", &self.token_id)
            .field("token", &self.token.as_ref().map(|_| REDACTED_PLACEHOLDER))
            .field(
                "token_sha256",
                &self.token_sha256.as_ref().map(|_| REDACTED_PLACEHOLDER),
            )
            .field("principal_id", &self.principal_id)
            .field("issuer", &self.issuer)
            .field("allowed_scopes", &self.allowed_scopes)
            .field("capabilities", &self.capabilities)
            .field("expires_at", &self.expires_at)
            .field("revoked", &self.revoked)
            .finish()
    }
}

/// Standalone authentication configuration loaded from a JSON file or environment variable.
#[derive(Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Required audience claim for signed tokens (defaults to `"dennett-memoryd"`).
    #[serde(default = "default_audience")]
    pub audience: String,
    /// Allowed issuer values for signed and static tokens.
    #[serde(default = "default_allowed_issuers")]
    pub allowed_issuers: Vec<String>,
    /// Optional HMAC-SHA256 signing secret for `dnt1` scoped tokens (minimum 16 bytes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hmac_secret: Option<String>,
    /// Static bearer token registry entries.
    #[serde(default)]
    pub tokens: Vec<StaticTokenEntryConfig>,
    /// Explicitly revoked `token_id` values (blocks both static and signed tokens).
    #[serde(default)]
    pub revoked_token_ids: BTreeSet<String>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            audience: default_audience(),
            allowed_issuers: default_allowed_issuers(),
            hmac_secret: None,
            tokens: Vec::new(),
            revoked_token_ids: BTreeSet::new(),
        }
    }
}

impl fmt::Debug for AuthConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthConfig")
            .field("audience", &self.audience)
            .field("allowed_issuers", &self.allowed_issuers)
            .field(
                "hmac_secret",
                &self.hmac_secret.as_ref().map(|_| REDACTED_PLACEHOLDER),
            )
            .field("token_count", &self.tokens.len())
            .field("revoked_token_ids_count", &self.revoked_token_ids.len())
            .finish()
    }
}

fn default_audience() -> String {
    String::from(DEFAULT_AUDIENCE)
}

fn default_issuer() -> String {
    String::from(DEFAULT_ISSUER)
}

fn default_allowed_issuers() -> Vec<String> {
    vec![default_issuer()]
}

struct CompiledStaticToken {
    token_hash: [u8; 32],
    revoked: bool,
    principal: AuthPrincipal,
}

fn compile_static_token_entry(
    entry: StaticTokenEntryConfig,
    allowed_issuers: &BTreeSet<String>,
    seen_ids: &mut BTreeSet<String>,
    seen_hashes: &mut BTreeSet<[u8; 32]>,
) -> Result<CompiledStaticToken, DennettError> {
    if entry.token_id.trim().is_empty() {
        return Err(DennettError::InvalidInput {
            field: String::from("auth.tokens.token_id"),
            reason: String::from("must not be empty"),
        });
    }
    if !seen_ids.insert(entry.token_id.clone()) {
        return Err(DennettError::InvalidInput {
            field: String::from("auth.tokens.token_id"),
            reason: format!("duplicate token_id `{}`", entry.token_id),
        });
    }
    if !allowed_issuers.contains(&entry.issuer) {
        return Err(DennettError::InvalidInput {
            field: String::from("auth.tokens.issuer"),
            reason: format!(
                "issuer `{}` for token `{}` is not in allowed_issuers",
                entry.issuer, entry.token_id
            ),
        });
    }

    let token_hash = match (entry.token, entry.token_sha256) {
        (Some(raw), None) => {
            if raw.is_empty()
                || raw.len() > MAX_BEARER_TOKEN_BYTES
                || raw
                    .bytes()
                    .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
            {
                return Err(DennettError::InvalidInput {
                    field: String::from("auth.tokens.token"),
                    reason: format!(
                        "token `{}` must be non-empty without whitespace or control characters",
                        entry.token_id
                    ),
                });
            }
            sha256_bytes(raw.as_bytes())
        }
        (None, Some(hex_digest)) => {
            let decoded =
                hex::decode(hex_digest.as_str()).map_err(|_| DennettError::InvalidInput {
                    field: String::from("auth.tokens.token_sha256"),
                    reason: String::from("invalid hex SHA-256 digest"),
                })?;
            let mut hash = [0_u8; 32];
            if decoded.len() != 32 {
                return Err(DennettError::InvalidInput {
                    field: String::from("auth.tokens.token_sha256"),
                    reason: String::from("SHA-256 digest must decode to 32 bytes"),
                });
            }
            hash.copy_from_slice(&decoded);
            hash
        }
        (Some(_), Some(_)) | (None, None) => {
            return Err(DennettError::InvalidInput {
                field: String::from("auth.tokens"),
                reason: format!(
                    "token `{}` must specify exactly one of `token` or `token_sha256`",
                    entry.token_id
                ),
            });
        }
    };

    if !seen_hashes.insert(token_hash) {
        return Err(DennettError::InvalidInput {
            field: String::from("auth.tokens"),
            reason: format!(
                "duplicate bearer token credential detected for token_id `{}`",
                entry.token_id
            ),
        });
    }

    if let Some(exp) = entry.expires_at {
        exp.validate("auth.tokens.expires_at")?;
    }

    Ok(CompiledStaticToken {
        token_hash,
        revoked: entry.revoked,
        principal: AuthPrincipal {
            principal_id: entry.principal_id,
            issuer: entry.issuer,
            token_id: entry.token_id,
            allowed_scopes: entry.allowed_scopes,
            capabilities: entry.capabilities,
            expires_at: entry.expires_at,
        },
    })
}

struct VerifierInner {
    audience: String,
    allowed_issuers: BTreeSet<String>,
    hmac_secret: Option<Vec<u8>>,
    static_tokens: Vec<CompiledStaticToken>,
    revoked_token_ids: RwLock<BTreeSet<String>>,
}

/// Thread-safe standalone bearer verifier implementing [`AuthVerifier`].
///
/// Cheap to clone via internal `Arc`. `Debug` output strictly redacts all secrets and hashes.
#[derive(Clone)]
pub struct StandaloneAuthVerifier {
    inner: Arc<VerifierInner>,
}

impl fmt::Debug for StandaloneAuthVerifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StandaloneAuthVerifier")
            .field("audience", &self.inner.audience)
            .field("allowed_issuers", &self.inner.allowed_issuers)
            .field(
                "hmac_enabled",
                &self
                    .inner
                    .hmac_secret
                    .as_ref()
                    .map(|_| REDACTED_PLACEHOLDER),
            )
            .field("static_token_count", &self.inner.static_tokens.len())
            .finish()
    }
}

impl StandaloneAuthVerifier {
    /// Creates a fail-closed verifier with zero tokens and no HMAC key.
    ///
    /// Rejects every bearer token with [`DennettError::Unauthorized`].
    #[must_use]
    pub fn fail_closed() -> Self {
        Self {
            inner: Arc::new(VerifierInner {
                audience: default_audience(),
                allowed_issuers: BTreeSet::new(),
                hmac_secret: None,
                static_tokens: Vec::new(),
                revoked_token_ids: RwLock::new(BTreeSet::new()),
            }),
        }
    }

    /// Compiles and validates an [`AuthConfig`] into a [`StandaloneAuthVerifier`].
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if configuration contains empty issuers/audience,
    /// an undersized HMAC secret, duplicate `token_id`, or entries missing token material.
    pub fn from_config(config: AuthConfig) -> Result<Self, DennettError> {
        let audience = config.audience.trim().to_owned();
        if audience.is_empty() {
            return Err(DennettError::InvalidInput {
                field: String::from("auth.audience"),
                reason: String::from("must not be empty"),
            });
        }

        let allowed_issuers: BTreeSet<String> = config
            .allowed_issuers
            .into_iter()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect();
        if allowed_issuers.is_empty() {
            return Err(DennettError::InvalidInput {
                field: String::from("auth.allowed_issuers"),
                reason: String::from("must contain at least one non-empty issuer"),
            });
        }

        let hmac_secret = match config.hmac_secret {
            None => None,
            Some(secret) => {
                if secret.len() < MIN_HMAC_SECRET_BYTES {
                    return Err(DennettError::InvalidInput {
                        field: String::from("auth.hmac_secret"),
                        reason: format!(
                            "must be at least {MIN_HMAC_SECRET_BYTES} bytes when configured"
                        ),
                    });
                }
                Some(secret.into_bytes())
            }
        };

        let mut seen_ids = BTreeSet::new();
        let mut seen_hashes: BTreeSet<[u8; 32]> = BTreeSet::new();
        let mut static_tokens = Vec::with_capacity(config.tokens.len());

        for entry in config.tokens {
            static_tokens.push(compile_static_token_entry(
                entry,
                &allowed_issuers,
                &mut seen_ids,
                &mut seen_hashes,
            )?);
        }

        Ok(Self {
            inner: Arc::new(VerifierInner {
                audience,
                allowed_issuers,
                hmac_secret,
                static_tokens,
                revoked_token_ids: RwLock::new(config.revoked_token_ids),
            }),
        })
    }

    /// Parses an [`AuthConfig`] from a JSON string, redacting secrets from any error diagnostic.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if JSON parsing or validation fails.
    pub fn from_json_str(json: &str) -> Result<Self, DennettError> {
        let config: AuthConfig =
            serde_json::from_str(json).map_err(|err| DennettError::InvalidInput {
                field: String::from("auth_config"),
                reason: format!(
                    "failed to parse standalone auth JSON: {}",
                    redact_secrets(&err.to_string(), &[])
                ),
            })?;
        Self::from_config(config)
    }

    /// Loads an [`AuthConfig`] JSON file from disk.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if reading or parsing the file fails.
    pub fn from_file(path: &Path) -> Result<Self, DennettError> {
        let raw = fs::read_to_string(path).map_err(|err| DennettError::InvalidInput {
            field: String::from("auth_config_file"),
            reason: format!(
                "failed to read auth config file `{}`: {}",
                path.display(),
                redact_secrets(&err.to_string(), &[])
            ),
        })?;
        Self::from_json_str(&raw)
    }

    /// Issues an HMAC-SHA256 signed `dnt1` bearer token for `claims`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::SafetyGuardRejected`] if `hmac_secret` is not configured,
    /// or [`DennettError::InvalidInput`] if claims are invalid.
    pub fn issue_signed_token(&self, claims: &SignedTokenClaims) -> Result<String, DennettError> {
        let Some(secret) = &self.inner.hmac_secret else {
            return Err(DennettError::SafetyGuardRejected {
                reason: String::from("HMAC signing secret is not configured"),
            });
        };
        if claims.token_id.trim().is_empty() {
            return Err(DennettError::InvalidInput {
                field: String::from("claims.token_id"),
                reason: String::from("must not be empty"),
            });
        }
        claims.issued_at.validate("claims.issued_at")?;
        claims.expires_at.validate("claims.expires_at")?;
        let payload_json = serde_json::to_vec(claims).map_err(|err| DennettError::Internal {
            detail: format!("failed to serialize token claims: {err}"),
        })?;
        let payload_b64 = URL_SAFE_NO_PAD.encode(payload_json);
        let signing_input = format!("{SIGNED_TOKEN_PREFIX}.{payload_b64}");

        let mut mac = HmacSha256::new_from_slice(secret).map_err(|_| DennettError::Internal {
            detail: String::from("failed to initialize HMAC-SHA256 key"),
        })?;
        mac.update(signing_input.as_bytes());
        let tag = mac.finalize().into_bytes();
        let tag_b64 = URL_SAFE_NO_PAD.encode(tag);
        Ok(format!("{signing_input}.{tag_b64}"))
    }

    /// Dynamically revokes `token_id` in memory for subsequent verifications.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::Internal`] if the internal revocation lock was poisoned.
    pub fn revoke_token_id(&self, token_id: impl Into<String>) -> Result<(), DennettError> {
        let mut guard =
            self.inner
                .revoked_token_ids
                .write()
                .map_err(|_| DennettError::Internal {
                    detail: String::from("auth revocation table lock poisoned"),
                })?;
        guard.insert(token_id.into());
        Ok(())
    }

    fn is_revoked(&self, token_id: &str) -> Result<bool, DennettError> {
        let guard = self
            .inner
            .revoked_token_ids
            .read()
            .map_err(|_| DennettError::Internal {
                detail: String::from("auth revocation table lock poisoned"),
            })?;
        Ok(guard.contains(token_id))
    }

    fn verify_signed_token(
        &self,
        token: &str,
        now: TimestampMs,
    ) -> Result<AuthPrincipal, DennettError> {
        let Some(secret) = &self.inner.hmac_secret else {
            return Err(DennettError::Unauthorized {
                reason: String::from("signed bearer tokens are not enabled"),
            });
        };

        let mut parts = token.split('.');
        let (Some(prefix), Some(payload_b64), Some(tag_b64), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(DennettError::Unauthorized {
                reason: String::from("malformed signed bearer token"),
            });
        };
        if prefix != SIGNED_TOKEN_PREFIX || payload_b64.is_empty() || tag_b64.is_empty() {
            return Err(DennettError::Unauthorized {
                reason: String::from("malformed signed bearer token"),
            });
        }

        let sig_bytes =
            URL_SAFE_NO_PAD
                .decode(tag_b64)
                .map_err(|_| DennettError::Unauthorized {
                    reason: String::from("invalid token signature encoding"),
                })?;

        let signing_input = format!("{prefix}.{payload_b64}");
        let mut mac = HmacSha256::new_from_slice(secret).map_err(|_| DennettError::Internal {
            detail: String::from("failed to initialize HMAC-SHA256 verifier"),
        })?;
        mac.update(signing_input.as_bytes());
        mac.verify_slice(&sig_bytes)
            .map_err(|_| DennettError::Unauthorized {
                reason: String::from("invalid bearer token signature"),
            })?;

        let payload_bytes =
            URL_SAFE_NO_PAD
                .decode(payload_b64)
                .map_err(|_| DennettError::Unauthorized {
                    reason: String::from("invalid token payload encoding"),
                })?;
        let claims: SignedTokenClaims =
            serde_json::from_slice(&payload_bytes).map_err(|_| DennettError::Unauthorized {
                reason: String::from("invalid bearer token claims"),
            })?;

        if claims.token_id.trim().is_empty() {
            return Err(DennettError::Unauthorized {
                reason: String::from("bearer token missing token_id"),
            });
        }
        if claims.audience != self.inner.audience {
            return Err(DennettError::Unauthorized {
                reason: String::from("bearer token audience mismatch"),
            });
        }
        if !self.inner.allowed_issuers.contains(&claims.issuer) {
            return Err(DennettError::Unauthorized {
                reason: String::from("bearer token issuer is not trusted"),
            });
        }
        if self.is_revoked(&claims.token_id)? {
            return Err(DennettError::Unauthorized {
                reason: String::from("bearer token has been revoked"),
            });
        }
        if !claims.issued_at.is_in_range() || !claims.expires_at.is_in_range() {
            return Err(DennettError::Unauthorized {
                reason: String::from("bearer token timestamps are out of valid range"),
            });
        }
        if now >= claims.expires_at {
            return Err(DennettError::Unauthorized {
                reason: String::from("bearer token has expired"),
            });
        }
        if claims.issued_at > claims.expires_at {
            return Err(DennettError::Unauthorized {
                reason: String::from("bearer token validity window is invalid"),
            });
        }

        Ok(AuthPrincipal {
            principal_id: claims.principal_id,
            issuer: claims.issuer,
            token_id: claims.token_id,
            allowed_scopes: claims.allowed_scopes,
            capabilities: claims.capabilities,
            expires_at: Some(claims.expires_at),
        })
    }

    fn verify_static_token(
        &self,
        token: &str,
        now: TimestampMs,
    ) -> Result<AuthPrincipal, DennettError> {
        let presented_hash = sha256_bytes(token.as_bytes());

        // Scan every static token entry in constant time to avoid leaking match position via timing.
        let mut matched_idx: Option<usize> = None;
        for (idx, entry) in self.inner.static_tokens.iter().enumerate() {
            if bool::from(entry.token_hash.ct_eq(&presented_hash)) {
                matched_idx = Some(idx);
            }
        }

        let Some(idx) = matched_idx else {
            return Err(DennettError::Unauthorized {
                reason: String::from("invalid bearer token"),
            });
        };

        let entry = &self.inner.static_tokens[idx];
        if entry.revoked || self.is_revoked(&entry.principal.token_id)? {
            return Err(DennettError::Unauthorized {
                reason: String::from("bearer token has been revoked"),
            });
        }
        if entry.principal.is_expired_at(now) {
            return Err(DennettError::Unauthorized {
                reason: String::from("bearer token has expired"),
            });
        }

        Ok(entry.principal.clone())
    }
}

impl AuthVerifier for StandaloneAuthVerifier {
    fn verify_bearer(&self, token: &str, now: TimestampMs) -> Result<AuthPrincipal, DennettError> {
        if token.is_empty()
            || token.len() > MAX_BEARER_TOKEN_BYTES
            || token
                .bytes()
                .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
        {
            return Err(DennettError::Unauthorized {
                reason: String::from("malformed bearer token"),
            });
        }

        if token.starts_with("dnt1.") {
            self.verify_signed_token(token, now)
        } else {
            self.verify_static_token(token, now)
        }
    }
}

const _: () = {
    const fn assert_send_sync<T: Send + Sync + 'static>() {}
    assert_send_sync::<AuthConfig>();
    assert_send_sync::<StandaloneAuthVerifier>();
};

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn sample_scope(name: &str) -> MemoryScope {
        MemoryScope::try_new(name).expect("valid test scope")
    }

    fn sample_principal(name: &str) -> PrincipalId {
        PrincipalId::try_new(name).expect("valid test principal")
    }

    #[test]
    fn static_token_enforces_match_expiry_boundary_revocation_and_debug_redaction() {
        let secret_token = "tok_secret_alpha_999888";
        let config = AuthConfig {
            audience: default_audience(),
            allowed_issuers: default_allowed_issuers(),
            hmac_secret: Some(String::from("super-secret-hmac-key-0123456789")),
            tokens: vec![
                StaticTokenEntryConfig {
                    token_id: String::from("static-alpha"),
                    token: Some(String::from(secret_token)),
                    token_sha256: None,
                    principal_id: sample_principal("agent:alpha"),
                    issuer: default_issuer(),
                    allowed_scopes: BTreeSet::from([sample_scope("project-alpha")]),
                    capabilities: BTreeSet::from([MemoryCapability::Read, MemoryCapability::Write]),
                    expires_at: Some(TimestampMs::from_millis(5_000)),
                    revoked: false,
                },
                StaticTokenEntryConfig {
                    token_id: String::from("static-revoked"),
                    token: Some(String::from("tok_revoked_111")),
                    token_sha256: None,
                    principal_id: sample_principal("agent:revoked"),
                    issuer: default_issuer(),
                    allowed_scopes: BTreeSet::from([sample_scope("project-alpha")]),
                    capabilities: BTreeSet::from([MemoryCapability::Read]),
                    expires_at: None,
                    revoked: true,
                },
            ],
            revoked_token_ids: BTreeSet::new(),
        };

        let cfg_debug = format!("{config:?}");
        assert!(!cfg_debug.contains(secret_token));
        assert!(!cfg_debug.contains("super-secret-hmac-key"));

        let verifier = StandaloneAuthVerifier::from_config(config).expect("valid config");
        let ver_debug = format!("{verifier:?}");
        assert!(!ver_debug.contains(secret_token));
        assert!(!ver_debug.contains("super-secret-hmac-key"));

        // 1 ms before expiry -> allowed.
        let ok = verifier.verify_bearer(secret_token, TimestampMs::from_millis(4_999));
        let Ok(principal) = ok else {
            panic!("token before expiry must verify: {ok:?}");
        };
        assert_eq!(principal.principal_id.as_str(), "agent:alpha");
        assert!(principal.can_access(&sample_scope("project-alpha"), MemoryCapability::Write));
        assert!(!principal.can_access(&sample_scope("project-beta"), MemoryCapability::Read));

        // Exact expiry instant -> denied.
        assert!(matches!(
            verifier.verify_bearer(secret_token, TimestampMs::from_millis(5_000)),
            Err(DennettError::Unauthorized { .. })
        ));
        // After expiry -> denied.
        assert!(matches!(
            verifier.verify_bearer(secret_token, TimestampMs::from_millis(5_001)),
            Err(DennettError::Unauthorized { .. })
        ));

        // Statically revoked -> denied.
        assert!(matches!(
            verifier.verify_bearer("tok_revoked_111", TimestampMs::from_millis(1_000)),
            Err(DennettError::Unauthorized { .. })
        ));

        // Dynamic revocation -> denied immediately.
        assert!(verifier.revoke_token_id("static-alpha").is_ok());
        assert!(matches!(
            verifier.verify_bearer(secret_token, TimestampMs::from_millis(4_000)),
            Err(DennettError::Unauthorized { .. })
        ));
    }

    #[test]
    fn signed_hmac_token_validates_signature_audience_issuer_expiry_and_revocation() {
        let config = AuthConfig {
            audience: String::from(DEFAULT_AUDIENCE),
            allowed_issuers: vec![String::from(DEFAULT_ISSUER)],
            hmac_secret: Some(String::from("hmac-signing-secret-must-be-long-enough")),
            tokens: vec![],
            revoked_token_ids: BTreeSet::new(),
        };
        let verifier = StandaloneAuthVerifier::from_config(config).expect("valid verifier");

        let claims = SignedTokenClaims {
            token_id: String::from("jti-100"),
            principal_id: sample_principal("agent:signed"),
            issuer: String::from(DEFAULT_ISSUER),
            audience: String::from(DEFAULT_AUDIENCE),
            allowed_scopes: BTreeSet::from([sample_scope("project-alpha")]),
            capabilities: BTreeSet::from([MemoryCapability::Read, MemoryCapability::Write]),
            issued_at: TimestampMs::from_millis(1_000),
            expires_at: TimestampMs::from_millis(2_000),
        };
        let token = verifier
            .issue_signed_token(&claims)
            .expect("issue signed token");

        let principal = verifier
            .verify_bearer(&token, TimestampMs::from_millis(1_999))
            .expect("valid signed token");
        assert_eq!(principal.principal_id.as_str(), "agent:signed");

        // Expired at exact boundary.
        assert!(matches!(
            verifier.verify_bearer(&token, TimestampMs::from_millis(2_000)),
            Err(DennettError::Unauthorized { .. })
        ));

        // Tampered token (flip last character).
        let mut tampered = token.clone();
        let replacement = if tampered.ends_with('A') { 'B' } else { 'A' };
        tampered.pop();
        tampered.push(replacement);
        assert!(matches!(
            verifier.verify_bearer(&tampered, TimestampMs::from_millis(1_500)),
            Err(DennettError::Unauthorized { .. })
        ));

        // Wrong audience.
        let wrong_aud_token = verifier
            .issue_signed_token(&SignedTokenClaims {
                audience: String::from("wrong-service"),
                ..claims.clone()
            })
            .expect("issue wrong aud");
        assert!(matches!(
            verifier.verify_bearer(&wrong_aud_token, TimestampMs::from_millis(1_500)),
            Err(DennettError::Unauthorized { .. })
        ));

        // Revoked jti.
        assert!(verifier.revoke_token_id("jti-100").is_ok());
        assert!(matches!(
            verifier.verify_bearer(&token, TimestampMs::from_millis(1_500)),
            Err(DennettError::Unauthorized { .. })
        ));
    }

    #[test]
    fn parse_bearer_header_rejects_malformed_inputs() {
        assert_eq!(parse_bearer_header("Bearer abc.def").ok(), Some("abc.def"));
        assert_eq!(
            parse_bearer_header("bearer   xyz_123").ok(),
            Some("xyz_123")
        );
        for bad in [
            "",
            "Bearer",
            "Bearer   ",
            "Basic dXNlcjpwYXNz",
            "Bearer two words",
            "Bearer bad\ntoken",
        ] {
            assert!(
                matches!(
                    parse_bearer_header(bad),
                    Err(DennettError::Unauthorized { .. })
                ),
                "expected Unauthorized for `{bad:?}`"
            );
        }
    }

    proptest! {
        #[test]
        fn prop_fail_closed_verifier_rejects_arbitrary_tokens(raw in any::<String>()) {
            let verifier = StandaloneAuthVerifier::fail_closed();
            let res = verifier.verify_bearer(&raw, TimestampMs::from_millis(1_000));
            prop_assert!(
                matches!(res, Err(DennettError::Unauthorized { .. })),
                "expected Unauthorized, got: {res:?}"
            );
        }

        #[test]
        fn prop_signed_token_expiry_boundary_is_exact(
            issued in 1_000_i64..100_000_i64,
            ttl in 1_i64..100_000_i64,
            delta in -500_i64..500_i64,
        ) {
            let expires = issued + ttl;
            let now = expires.saturating_add(delta);
            let verifier = StandaloneAuthVerifier::from_config(AuthConfig {
                hmac_secret: Some(String::from("prop-test-hmac-secret-1234567890")),
                ..AuthConfig::default()
            })
            .expect("valid verifier");
            let claims = SignedTokenClaims {
                token_id: String::from("prop-jti"),
                principal_id: sample_principal("agent:prop"),
                issuer: default_issuer(),
                audience: default_audience(),
                allowed_scopes: BTreeSet::from([sample_scope("project-prop")]),
                capabilities: BTreeSet::from([MemoryCapability::Read]),
                issued_at: TimestampMs::from_millis(issued),
                expires_at: TimestampMs::from_millis(expires),
            };
            let token = verifier.issue_signed_token(&claims).expect("issue");
            let res = verifier.verify_bearer(&token, TimestampMs::from_millis(now));
            prop_assert_eq!(res.is_ok(), now < expires);
        }
    }
}
