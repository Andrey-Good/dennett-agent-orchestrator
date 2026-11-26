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
use dennett_contracts::{
    AgentId, DennettError, MemoryCapability, MemoryScope, PrincipalId, ProjectId, SessionId,
    SessionKind, TaskId, TimestampMs,
};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

/// Default audience expected by standalone `dennett-memoryd`.
pub const DEFAULT_MEMORY_AUDIENCE: &str = "dennett-memoryd";
/// Default issuer trusted by standalone `dennett-memoryd`.
pub const DEFAULT_MEMORY_ISSUER: &str = "dennett-standalone";
/// Version prefix for HMAC-SHA256 signed scoped memory tokens (`dennett-memoryd`).
pub const SIGNED_TOKEN_PREFIX: &str = "dnt1";
/// Version prefix for HMAC-SHA256 signed project-scoped control MCP tokens (`dennett-control-mcp`).
pub const CONTROL_TOKEN_PREFIX: &str = "dct1";
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

/// Verified claims embedded inside a short-lived `dct1.` session/project-scoped control token
/// issued by `dennett-head` for `dennett-control-mcp` subprocesses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlTokenClaims {
    /// Unique minted token identifier.
    pub token_id: String,
    /// Project strictly authorized by this token (`None` for standalone chats — only global session skills permitted).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
    /// Agent identity on whose behalf `dennett-control-mcp` is acting.
    pub agent_id: AgentId,
    /// Originating session identifier.
    pub session_id: SessionId,
    /// Semantic kind of the originating session (`user_chat` vs `task_run`).
    pub session_kind: SessionKind,
    /// Originating durable task identifier when minted for a background `task_run` turn (`None` for interactive chats).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    /// UTC issuance timestamp in epoch milliseconds.
    pub issued_at: TimestampMs,
    /// UTC expiry timestamp in epoch milliseconds.
    pub expires_at: TimestampMs,
}

/// Cryptographic HMAC-SHA256 signer & verifier for `dct1.` project-scoped control MCP tokens.
#[derive(Clone)]
pub struct ControlTokenSigner {
    secret: Vec<u8>,
}

impl fmt::Debug for ControlTokenSigner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ControlTokenSigner")
            .field("secret", &REDACTED_PLACEHOLDER)
            .finish()
    }
}

impl ControlTokenSigner {
    /// Creates a new [`ControlTokenSigner`] from `secret` (minimum [`MIN_HMAC_SECRET_BYTES`]).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `secret` is shorter than 16 bytes.
    pub fn new(secret: impl Into<Vec<u8>>) -> Result<Self, DennettError> {
        let bytes = secret.into();
        if bytes.len() < MIN_HMAC_SECRET_BYTES {
            return Err(DennettError::InvalidInput {
                field: String::from("control_hmac_secret"),
                reason: format!("must be at least {MIN_HMAC_SECRET_BYTES} bytes"),
            });
        }
        Ok(Self { secret: bytes })
    }

    /// Generates an ephemeral 32-byte per-daemon secret using two CSPRNG UUID v4 values.
    #[must_use]
    pub fn ephemeral() -> Self {
        let mut bytes = Vec::with_capacity(32);
        bytes.extend_from_slice(Uuid::new_v4().as_bytes());
        bytes.extend_from_slice(Uuid::new_v4().as_bytes());
        Self { secret: bytes }
    }

    /// Mints a short-lived signed `dct1.` token strictly bound to `(project_id, agent_id, session_id, session_kind)`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::Internal`] on serialization or HMAC initialization failure.
    pub fn mint_control_token(
        &self,
        project_id: Option<ProjectId>,
        agent_id: AgentId,
        session_id: SessionId,
        session_kind: SessionKind,
    ) -> Result<String, DennettError> {
        self.mint_control_token_with_task(project_id, agent_id, session_id, session_kind, None)
    }

    /// Mints a short-lived signed `dct1.` token strictly bound to
    /// `(project_id, agent_id, session_id, session_kind, task_id)`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::Internal`] on serialization or HMAC initialization failure.
    pub fn mint_control_token_with_task(
        &self,
        project_id: Option<ProjectId>,
        agent_id: AgentId,
        session_id: SessionId,
        session_kind: SessionKind,
        task_id: Option<TaskId>,
    ) -> Result<String, DennettError> {
        let issued_at = TimestampMs::now();
        let expires_at =
            TimestampMs::from_millis(issued_at.as_millis().saturating_add(DEFAULT_TOKEN_TTL_MS));
        let claims = ControlTokenClaims {
            token_id: format!("ctrl-{}", Uuid::new_v4().simple()),
            project_id,
            agent_id,
            session_id,
            session_kind,
            task_id,
            issued_at,
            expires_at,
        };
        let payload_json = serde_json::to_vec(&claims).map_err(|err| DennettError::Internal {
            detail: format!("failed serializing control token claims: {err}"),
        })?;
        let payload_b64 = URL_SAFE_NO_PAD.encode(payload_json);
        let signing_input = format!("{CONTROL_TOKEN_PREFIX}.{payload_b64}");

        let mut mac =
            HmacSha256::new_from_slice(&self.secret).map_err(|_| DennettError::Internal {
                detail: String::from("failed initializing HMAC-SHA256 control signer"),
            })?;
        mac.update(signing_input.as_bytes());
        let tag = mac.finalize().into_bytes();
        let tag_b64 = URL_SAFE_NO_PAD.encode(tag);

        Ok(format!("{signing_input}.{tag_b64}"))
    }

    /// Verifies a `dct1.` token's HMAC-SHA256 tag (constant-time) and expiration against `now`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::Unauthorized`] if the token format, signature, JSON, or expiry is invalid.
    pub fn verify_control_token(
        &self,
        token: &str,
        now: TimestampMs,
    ) -> Result<ControlTokenClaims, DennettError> {
        let trimmed = token.trim();
        let mut parts = trimmed.split('.');
        let (Some(prefix), Some(payload_b64), Some(sig_b64), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(DennettError::Unauthorized {
                reason: String::from("malformed control token format"),
            });
        };
        if prefix != CONTROL_TOKEN_PREFIX {
            return Err(DennettError::Unauthorized {
                reason: String::from("unsupported control token prefix"),
            });
        }
        let sig = URL_SAFE_NO_PAD
            .decode(sig_b64)
            .map_err(|_| DennettError::Unauthorized {
                reason: String::from("invalid control token signature encoding"),
            })?;
        let signing_input = format!("{prefix}.{payload_b64}");
        let mut mac =
            HmacSha256::new_from_slice(&self.secret).map_err(|_| DennettError::Internal {
                detail: String::from("failed initializing HMAC-SHA256 control verifier"),
            })?;
        mac.update(signing_input.as_bytes());
        mac.verify_slice(&sig)
            .map_err(|_| DennettError::Unauthorized {
                reason: String::from("invalid control token signature"),
            })?;

        let payload_bytes =
            URL_SAFE_NO_PAD
                .decode(payload_b64)
                .map_err(|_| DennettError::Unauthorized {
                    reason: String::from("invalid control token payload encoding"),
                })?;
        let claims: ControlTokenClaims =
            serde_json::from_slice(&payload_bytes).map_err(|_| DennettError::Unauthorized {
                reason: String::from("malformed control token claims"),
            })?;
        if claims.expires_at.as_millis() <= now.as_millis() {
            return Err(DennettError::Unauthorized {
                reason: String::from("control token expired"),
            });
        }
        Ok(claims)
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

    #[test]
    fn control_token_signer_roundtrips_and_rejects_tamper_and_expiry() {
        let signer = ControlTokenSigner::ephemeral();
        let pid = ProjectId::new();
        let aid = AgentId::new();
        let sid = SessionId::new();
        let tok = signer
            .mint_control_token(Some(pid), aid, sid, SessionKind::UserChat)
            .expect("mint dct1");
        assert!(tok.starts_with("dct1."));

        let verified = signer
            .verify_control_token(&tok, TimestampMs::now())
            .expect("verify dct1");
        assert_eq!(verified.project_id, Some(pid));
        assert_eq!(verified.agent_id, aid);
        assert_eq!(verified.session_id, sid);
        assert_eq!(verified.session_kind, SessionKind::UserChat);

        let standalone_tok = signer
            .mint_control_token(None, aid, sid, SessionKind::UserChat)
            .expect("mint standalone dct1");
        let standalone_verified = signer
            .verify_control_token(&standalone_tok, TimestampMs::now())
            .expect("verify standalone dct1");
        assert_eq!(standalone_verified.project_id, None);
        assert_eq!(standalone_verified.session_id, sid);
        assert_eq!(standalone_verified.task_id, None);

        let tid = TaskId::new();
        let task_tok = signer
            .mint_control_token_with_task(Some(pid), aid, sid, SessionKind::TaskRun, Some(tid))
            .expect("mint task_run dct1");
        let task_verified = signer
            .verify_control_token(&task_tok, TimestampMs::now())
            .expect("verify task_run dct1");
        assert_eq!(task_verified.session_kind, SessionKind::TaskRun);
        assert_eq!(task_verified.task_id, Some(tid));

        // Tampering with signature MUST fail closed.
        let mut tampered = tok.clone();
        tampered.push('A');
        assert!(matches!(
            signer.verify_control_token(&tampered, TimestampMs::now()),
            Err(DennettError::Unauthorized { .. })
        ));

        // Expiry MUST fail closed.
        let far_future = TimestampMs::from_millis(verified.expires_at.as_millis() + 1);
        assert!(matches!(
            signer.verify_control_token(&tok, far_future),
            Err(DennettError::Unauthorized { .. })
        ));

        // Structural redaction scrubs `dct1.` tokens cleanly.
        let msg = format!("rejected X-Dennett-Control-Token: {tok}");
        let safe = dennett_contracts::redact::redact_secrets(&msg, &[]);
        assert!(!safe.contains(&tok));
        assert!(!safe.contains("dct1."));
    }
}
