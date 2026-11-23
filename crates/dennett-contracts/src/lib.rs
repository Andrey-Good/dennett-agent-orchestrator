//! Minimal shared domain contracts for Dennett services and verification tooling.
//!
//! This crate intentionally stays free of SQL drivers, HTTP stacks, UI layers,
//! and model-provider SDKs so that all services share stable boundary types.

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// Maximum permitted byte length of a [`CorrelationId`].
pub const MAX_CORRELATION_ID_LEN: usize = 128;

/// Execution profile mode separating production state from isolated test profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileMode {
    /// Normal operational profile targeting real user or server state.
    Production,
    /// Ephemeral isolated profile dedicated to automated verification and fault injection.
    TestIsolated,
}

impl ProfileMode {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Production => "production",
            Self::TestIsolated => "test_isolated",
        }
    }
}

impl fmt::Display for ProfileMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Explicitly separated authentication profiles for the Google Antigravity (`agy acp`) runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum RuntimeAuthProfile {
    /// Interactive Google AI Ultra subscription profile (`oauth-personal`).
    #[serde(rename = "oauth-personal")]
    OAuthPersonal,
    /// Headless background profile authenticated with `GEMINI_API_KEY` (`gemini-api-key`).
    #[serde(rename = "gemini-api-key")]
    GeminiApiKey,
}

impl RuntimeAuthProfile {
    /// Returns the ACP `authenticate` method identifier defined by `agy acp`.
    #[must_use]
    pub const fn as_method_id(self) -> &'static str {
        match self {
            Self::OAuthPersonal => "oauth-personal",
            Self::GeminiApiKey => "gemini-api-key",
        }
    }
}

impl fmt::Display for RuntimeAuthProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_method_id())
    }
}

/// Architectural service kind participating in the Dennett runtime topology.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ServiceKind {
    /// Independent memory daemon (`dennett-memoryd`) running without a Head dependency.
    Memoryd,
    /// Server-side orchestration head (future milestone).
    Head,
    /// Local device execution node (future milestone).
    Node,
}

impl ServiceKind {
    /// Returns the canonical snake-case service name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Memoryd => "memoryd",
            Self::Head => "head",
            Self::Node => "node",
        }
    }
}

impl fmt::Display for ServiceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Validated correlation identifier propagated across logs and service boundaries.
///
/// Permits non-empty ASCII identifiers up to [`MAX_CORRELATION_ID_LEN`] bytes containing
/// only alphanumeric characters, `-`, `_`, and `.` so log and header values remain safe.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct CorrelationId(String);

impl CorrelationId {
    /// Generates a fresh random UUID v4 correlation identifier.
    #[must_use]
    pub fn generate() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Validates and constructs a [`CorrelationId`] from an arbitrary string.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `value` is empty, exceeds
    /// [`MAX_CORRELATION_ID_LEN`] bytes, or contains disallowed characters.
    pub fn try_new(value: impl Into<String>) -> Result<Self, DennettError> {
        let candidate = value.into();
        validate_correlation_id(&candidate)?;
        Ok(Self(candidate))
    }

    /// Borrows the validated correlation identifier as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CorrelationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for CorrelationId {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_new(s)
    }
}

impl TryFrom<String> for CorrelationId {
    type Error = DennettError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl<'de> Deserialize<'de> for CorrelationId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::try_new(raw).map_err(serde::de::Error::custom)
    }
}

fn validate_correlation_id(candidate: &str) -> Result<(), DennettError> {
    if candidate.is_empty() {
        return Err(DennettError::InvalidInput {
            field: String::from("correlation_id"),
            reason: String::from("must not be empty"),
        });
    }
    if candidate.len() > MAX_CORRELATION_ID_LEN {
        return Err(DennettError::InvalidInput {
            field: String::from("correlation_id"),
            reason: format!("exceeds maximum length of {MAX_CORRELATION_ID_LEN} bytes"),
        });
    }
    if !candidate
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    {
        return Err(DennettError::InvalidInput {
            field: String::from("correlation_id"),
            reason: String::from(
                "must contain only ASCII alphanumeric characters, '-', '_', or '.'",
            ),
        });
    }
    Ok(())
}

/// Machine-readable health and capability snapshot exposed by Dennett services.
///
/// `memory_fabric_implemented` explicitly distinguishes early D0 connectivity scaffolds
/// from a real Memory Fabric implementation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceHealth {
    /// Service reporting this health snapshot.
    pub service: ServiceKind,
    /// Current implementation milestone label (e.g. `"D0"`).
    pub stage: String,
    /// Active execution profile mode.
    pub profile_mode: ProfileMode,
    /// Whether the Memory Fabric domain API is implemented (`false` in D0).
    pub memory_fabric_implemented: bool,
    /// Whether live PostgreSQL connectivity succeeded for this probe.
    pub postgres_connected: bool,
    /// Active PostgreSQL role reported by `current_user`, when connected.
    pub postgres_role: Option<String>,
    /// Active PostgreSQL database reported by `current_database()`, when connected.
    pub postgres_database: Option<String>,
    /// Explicitly supported capabilities for the current stage.
    pub capabilities: Vec<String>,
    /// Correlation identifier attached to this health check.
    pub correlation_id: CorrelationId,
}

/// Typed, secret-safe error surface shared across Dennett crates.
#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum DennettError {
    /// Caller supplied malformed or out-of-range input.
    #[error("invalid input for `{field}`: {reason}")]
    InvalidInput {
        /// Name of the offending field or parameter.
        field: String,
        /// Safe, secret-free description of the validation failure.
        reason: String,
    },
    /// Safety boundary rejected an unsafe or unisolated operation.
    #[error("safety guard rejected operation: {reason}")]
    SafetyGuardRejected {
        /// Safe explanation of which boundary precondition failed.
        reason: String,
    },
    /// PostgreSQL could not be reached or rejected the connection/query.
    #[error("database unavailable: {detail}")]
    DatabaseUnavailable {
        /// Redacted diagnostic detail with credentials stripped.
        detail: String,
    },
    /// Operation was denied by scope or permission boundaries.
    #[error("access denied for `{scope}`: {reason}")]
    AccessDenied {
        /// Scope or target resource identifier.
        scope: String,
        /// Reason for denial.
        reason: String,
    },
    /// Runtime requires explicit authentication under the selected profile.
    #[error("authentication required for profile `{profile}`: {detail}")]
    AuthRequired {
        /// Explicitly requested runtime authentication profile.
        profile: RuntimeAuthProfile,
        /// Safe diagnostic detail from the runtime.
        detail: String,
    },
    /// Runtime protocol handshake or RPC exchange failed.
    #[error("runtime protocol error (code {code:?}): {detail}")]
    RuntimeProtocol {
        /// Optional JSON-RPC or protocol error code.
        code: Option<i64>,
        /// Redacted diagnostic detail.
        detail: String,
    },
    /// Detected runtime binary is older than the supported minimum.
    #[error("unsupported runtime version `{found}`, minimum required is `{minimum}`")]
    RuntimeVersionUnsupported {
        /// Version detected on the host.
        found: String,
        /// Minimum supported version.
        minimum: String,
    },
    /// Operation exceeded its configured time limit.
    #[error("operation timed out: {operation}")]
    Timeout {
        /// Operation name that timed out.
        operation: String,
    },
    /// Operation was explicitly cancelled before completion.
    #[error("operation cancelled: {operation}")]
    Cancelled {
        /// Operation name that was cancelled.
        operation: String,
    },
    /// Unexpected internal error carrying redacted context only.
    #[error("internal error: {detail}")]
    Internal {
        /// Redacted diagnostic detail.
        detail: String,
    },
}

impl DennettError {
    /// Returns a stable machine-readable error code string.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput { .. } => "INVALID_INPUT",
            Self::SafetyGuardRejected { .. } => "SAFETY_GUARD_REJECTED",
            Self::DatabaseUnavailable { .. } => "DATABASE_UNAVAILABLE",
            Self::AccessDenied { .. } => "ACCESS_DENIED",
            Self::AuthRequired { .. } => "AUTH_REQUIRED",
            Self::RuntimeProtocol { .. } => "RUNTIME_PROTOCOL",
            Self::RuntimeVersionUnsupported { .. } => "RUNTIME_VERSION_UNSUPPORTED",
            Self::Timeout { .. } => "TIMEOUT",
            Self::Cancelled { .. } => "CANCELLED",
            Self::Internal { .. } => "INTERNAL",
        }
    }
}

const _: () = {
    const fn assert_send_sync<T: Send + Sync + 'static>() {}
    assert_send_sync::<ProfileMode>();
    assert_send_sync::<RuntimeAuthProfile>();
    assert_send_sync::<ServiceKind>();
    assert_send_sync::<CorrelationId>();
    assert_send_sync::<ServiceHealth>();
    assert_send_sync::<DennettError>();
};

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn correlation_id_accepts_uuid_and_safe_ascii() {
        let generated = CorrelationId::generate();
        assert!(
            Uuid::parse_str(generated.as_str()).is_ok(),
            "generated id should parse as UUID: {generated}"
        );

        let custom = CorrelationId::try_new("d0.probe_run-01");
        assert!(custom.is_ok(), "safe ASCII id should succeed: {custom:?}");
    }

    #[test]
    fn correlation_id_rejects_empty_oversized_and_unsafe_chars() {
        assert!(CorrelationId::try_new("").is_err());
        assert!(CorrelationId::try_new("a".repeat(MAX_CORRELATION_ID_LEN + 1)).is_err());
        assert!(CorrelationId::try_new("bad id with spaces").is_err());
        assert!(CorrelationId::try_new("bad\ninjection").is_err());
        assert!(CorrelationId::try_new("bad/path").is_err());
    }

    #[test]
    fn correlation_id_serde_enforces_invariants() {
        let bad_json = "\"invalid\\r\\nheader\"";
        let parsed: Result<CorrelationId, _> = serde_json::from_str(bad_json);
        assert!(
            parsed.is_err(),
            "deserializing unsafe correlation id must fail"
        );
    }

    #[test]
    fn runtime_auth_profile_matches_acp_method_ids() {
        assert_eq!(
            RuntimeAuthProfile::OAuthPersonal.as_method_id(),
            "oauth-personal"
        );
        assert_eq!(
            RuntimeAuthProfile::GeminiApiKey.as_method_id(),
            "gemini-api-key"
        );

        let serialized = serde_json::to_string(&RuntimeAuthProfile::OAuthPersonal);
        assert_eq!(serialized.ok().as_deref(), Some("\"oauth-personal\""));
        let serialized_api = serde_json::to_string(&RuntimeAuthProfile::GeminiApiKey);
        assert_eq!(serialized_api.ok().as_deref(), Some("\"gemini-api-key\""));
    }

    #[test]
    fn error_codes_are_stable() {
        let err = DennettError::DatabaseUnavailable {
            detail: String::from("connection refused"),
        };
        assert_eq!(err.code(), "DATABASE_UNAVAILABLE");
    }

    fn control_char_strategy() -> impl Strategy<Value = char> {
        prop_oneof![
            (0x00_u8..=0x1F_u8).prop_map(char::from),
            Just('\u{007F}'),
            (0x80_u32..=0x9F_u32).prop_filter_map("valid char", char::from_u32),
        ]
    }

    proptest! {
        #[test]
        fn prop_correlation_id_valid_roundtrips_across_constructors_and_serde(
            raw in "[A-Za-z0-9._-]{1,128}"
        ) {
            let from_try = CorrelationId::try_new(raw.clone());
            prop_assert!(from_try.is_ok(), "valid ASCII id must succeed: {raw:?}");
            let Ok(id) = from_try else {
                unreachable!();
            };
            prop_assert_eq!(id.as_str(), raw.as_str());
            prop_assert_eq!(id.to_string(), raw.clone());
            prop_assert_eq!(raw.parse::<CorrelationId>(), Ok(id.clone()));
            prop_assert_eq!(CorrelationId::try_from(raw), Ok(id.clone()));

            let encoded = serde_json::to_string(&id);
            prop_assert!(encoded.is_ok());
            if let Ok(json) = encoded {
                let decoded: Result<CorrelationId, _> = serde_json::from_str(&json);
                prop_assert_eq!(decoded.ok(), Some(id));
            }
        }

        #[test]
        fn prop_correlation_id_arbitrary_unicode_matches_spec_oracle(raw in any::<String>()) {
            let expected_valid = !raw.is_empty()
                && raw.len() <= MAX_CORRELATION_ID_LEN
                && raw
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));

            let try_res = CorrelationId::try_new(raw.clone());
            prop_assert_eq!(try_res.is_ok(), expected_valid);
            prop_assert_eq!(raw.parse::<CorrelationId>().is_ok(), expected_valid);

            if let Ok(json) = serde_json::to_string(&raw) {
                let de_res: Result<CorrelationId, _> = serde_json::from_str(&json);
                prop_assert_eq!(de_res.is_ok(), expected_valid);
            }
        }

        #[test]
        fn prop_correlation_id_rejects_control_characters(
            prefix in "[A-Za-z0-9._-]{0,24}",
            ctrl in control_char_strategy(),
            suffix in "[A-Za-z0-9._-]{0,24}",
        ) {
            let candidate = format!("{prefix}{ctrl}{suffix}");
            let res = CorrelationId::try_new(candidate.clone());
            prop_assert!(
                matches!(
                    res,
                    Err(DennettError::InvalidInput { ref field, .. }) if field == "correlation_id"
                ),
                "expected InvalidInput(correlation_id), got: {res:?}"
            );
            if let Ok(json) = serde_json::to_string(&candidate) {
                let de_res: Result<CorrelationId, _> = serde_json::from_str(&json);
                prop_assert!(de_res.is_err());
            }
        }

        #[test]
        fn prop_correlation_id_rejects_oversized_inputs(raw in "[A-Za-z0-9._-]{129,256}") {
            let res = CorrelationId::try_new(raw);
            prop_assert!(
                matches!(
                    res,
                    Err(DennettError::InvalidInput { ref field, .. }) if field == "correlation_id"
                ),
                "expected InvalidInput(correlation_id), got: {res:?}"
            );
        }
    }
}
