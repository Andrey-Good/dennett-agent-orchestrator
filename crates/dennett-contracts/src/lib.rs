//! Minimal shared domain contracts for Dennett services and verification tooling.
//!
//! This crate intentionally stays free of SQL drivers, HTTP stacks, UI layers,
//! and model-provider SDKs so that all services share stable boundary types.

pub mod control;
pub mod memory;
pub mod redact;

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub use control::{
    AgentConfigHistoryResponse, AgentConfigVersion, AgentConfigVersionId, AgentId, AgentRecord,
    ArtifactId, ArtifactRecord, AuditEventId, CancelRunResponse, CommitMemoryNoteRequest,
    CommitMemoryNoteResponse, CreateAgentRequest, CreateArtifactRequest, CreatePolicyRuleRequest,
    CreateProjectRequest, CreateSessionRequest, DiagnosticEvent, DiagnosticsResponse,
    FlushOutboxResponse, HeadHealth, HeadMemorySearchRequest, HeadMemorySearchResponse,
    ListAgentsResponse, ListArtifactsResponse, ListOutboxResponse, ListPolicyRulesResponse,
    ListProjectFilesResponse, ListProjectsResponse, ListSessionsResponse, MAX_ADAPTER_ID_LEN,
    MAX_AGENT_DESCRIPTION_BYTES, MAX_AGENT_MEMORY_SCOPES, MAX_AGENT_NAME_LEN,
    MAX_ALLOWED_WRITE_PREFIXES, MAX_ARTIFACT_CONTENT_BYTES, MAX_ARTIFACT_MIME_LEN,
    MAX_ARTIFACT_NAME_LEN, MAX_MESSAGE_CONTENT_BYTES, MAX_MODEL_NAME_LEN, MAX_POLICY_PATTERN_LEN,
    MAX_POLICY_SCOPE_LEN, MAX_PROJECT_NAME_LEN, MAX_PROVIDER_SESSION_ID_LEN, MAX_REASON_BYTES,
    MAX_SESSION_TITLE_LEN, MAX_SYSTEM_PROMPT_BYTES, MAX_WORKSPACE_PATH_LEN, MemoryOutboxItem,
    MemoryOutboxStatus, MemoryStatus, MessageId, MessageRecord, MessageRole, OutboxItemId,
    PermissionDecisionId, PermissionDecisionRecord, PolicyEffect, PolicyRule, PolicyRuleId,
    ProjectId, ProjectRecord, RunId, RunRecord, RunStatus, RuntimeBinding, RuntimeBindingId,
    RuntimeCatalogResponse, RuntimeModelOption, RuntimeQuotaWindow, RuntimeThinkingOption,
    SendMessageRequest, SendMessageResponse, SessionDetailResponse, SessionId, SessionRecord,
    SessionStatus, ThinkingLevel, UpdateAgentConfigRequest, UpdateProjectRequest,
    UpdateSessionRequest, WorkspaceFileEntry, WorkspaceFileKind, WorkspaceWritePolicy,
};

pub use memory::{
    AuthPrincipal, CorrectNoteRequest, CreateNoteRequest, DEFAULT_SEARCH_LIMIT, EventEvidenceLink,
    EvidenceAnchor, EvidenceId, EvidenceLinkId, EvidenceLinkInput, EvidenceObject,
    EvidenceRelationship, IdempotencyKey, IngestEvidenceRequest, IngestEvidenceResponse,
    MAX_ANCHOR_ATTRIBUTES, MAX_ANCHOR_KIND_LEN, MAX_ANCHOR_LOCATOR_LEN, MAX_AUTHOR_LABEL_BYTES,
    MAX_EVIDENCE_CONTENT_BYTES, MAX_IDEMPOTENCY_KEY_LEN, MAX_LINKS_PER_EVENT, MAX_MEDIA_TYPE_LEN,
    MAX_METADATA_ENTRIES, MAX_METADATA_KEY_LEN, MAX_METADATA_VALUE_LEN, MAX_NOTE_BODY_BYTES,
    MAX_NOTE_TITLE_BYTES, MAX_PRINCIPAL_ID_LEN, MAX_RATIONALE_BYTES, MAX_RELATIONSHIP_LEN,
    MAX_SCOPE_LEN, MAX_SEARCH_LIMIT, MAX_SEARCH_QUERY_BYTES, MAX_SOURCE_URI_LEN, MAX_TAG_LEN,
    MAX_TAGS_PER_NOTE, MAX_TIMESTAMP_MS, MIN_TIMESTAMP_MS, MemoryCapability, MemoryEvent,
    MemoryEventId, MemoryEventKind, MemoryNote, MemoryNoteId, MemoryScope, NoteHistoryResponse,
    NoteMutationResponse, NoteRevision, NoteRevisionId, PrincipalId, SHA256_HEX_LEN,
    SearchNotesRequest, SearchNotesResponse, Sha256Hex, TimestampMs,
};
pub use redact::{
    REDACTED_PLACEHOLDER, extract_connection_secrets, redact_secrets, redact_secrets_owned,
    sanitize_cli_arg, sanitize_pg_error, sanitize_pg_error_with_password,
};

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
    /// Current implementation milestone label (e.g. `"D1"`).
    pub stage: String,
    /// Active execution profile mode.
    pub profile_mode: ProfileMode,
    /// Whether the Memory Fabric domain API is implemented.
    pub memory_fabric_implemented: bool,
    /// Whether live PostgreSQL connectivity succeeded for this probe.
    pub postgres_connected: bool,
    /// Active PostgreSQL role reported by `current_user`, when connected.
    pub postgres_role: Option<String>,
    /// Active PostgreSQL database reported by `current_database()`, when connected.
    pub postgres_database: Option<String>,
    /// Highest applied schema migration version when the service schema is initialized.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_version: Option<i64>,
    /// Explicitly supported capabilities for the current stage.
    pub capabilities: Vec<String>,
    /// Correlation identifier attached to this health check.
    pub correlation_id: CorrelationId,
}

/// Structured HTTP error envelope returned by versioned Dennett service endpoints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiErrorResponse {
    /// Stable machine-readable error code matching [`DennettError::code`].
    pub code: String,
    /// Typed secret-free domain error payload.
    pub error: DennettError,
    /// Correlation identifier propagated or assigned for the failing request.
    pub correlation_id: CorrelationId,
}

impl ApiErrorResponse {
    /// Wraps a [`DennettError`] and [`CorrelationId`] into a wire error response.
    #[must_use]
    pub fn new(error: DennettError, correlation_id: CorrelationId) -> Self {
        let code = String::from(error.code());
        Self {
            code,
            error,
            correlation_id,
        }
    }
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
    /// Bearer credential was missing, malformed, expired, revoked, or failed verification.
    #[error("unauthorized: {reason}")]
    Unauthorized {
        /// Safe diagnostic reason without raw credential material.
        reason: String,
    },
    /// Authenticated principal lacks scope or capability permission for the requested resource.
    #[error("forbidden scope `{scope}`: {reason}")]
    ForbiddenScope {
        /// Scope that was denied.
        scope: String,
        /// Safe explanation of the missing scope or capability.
        reason: String,
    },
    /// Requested domain resource does not exist.
    #[error("{resource} `{id}` not found")]
    NotFound {
        /// Logical resource kind (e.g. `"memory_note"`, `"evidence"`, `"memory_event"`).
        resource: String,
        /// Identifier that was not found.
        id: String,
    },
    /// Optimistic revision check or state invariant conflict prevented the write.
    #[error("conflict on {resource}: {reason}")]
    Conflict {
        /// Logical resource kind (e.g. `"memory_note"`).
        resource: String,
        /// Caller's expected revision when applicable.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_revision: Option<i64>,
        /// Actual current revision observed in canonical storage when applicable.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        actual_revision: Option<i64>,
        /// Safe description of the conflict.
        reason: String,
    },
    /// Caller reused an idempotency key with a different operation or payload fingerprint.
    #[error("idempotency mismatch for key `{idempotency_key}`: {reason}")]
    IdempotencyMismatch {
        /// Conflicting idempotency key.
        idempotency_key: String,
        /// Safe explanation of the mismatch.
        reason: String,
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
            Self::Unauthorized { .. } => "UNAUTHORIZED",
            Self::ForbiddenScope { .. } => "FORBIDDEN_SCOPE",
            Self::NotFound { .. } => "NOT_FOUND",
            Self::Conflict { .. } => "CONFLICT",
            Self::IdempotencyMismatch { .. } => "IDEMPOTENCY_MISMATCH",
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
    assert_send_sync::<ApiErrorResponse>();
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
        let cases = [
            (
                DennettError::DatabaseUnavailable {
                    detail: String::from("connection refused"),
                },
                "DATABASE_UNAVAILABLE",
            ),
            (
                DennettError::Unauthorized {
                    reason: String::from("token expired"),
                },
                "UNAUTHORIZED",
            ),
            (
                DennettError::ForbiddenScope {
                    scope: String::from("project-beta"),
                    reason: String::from("not permitted"),
                },
                "FORBIDDEN_SCOPE",
            ),
            (
                DennettError::NotFound {
                    resource: String::from("memory_note"),
                    id: String::from("00000000-0000-0000-0000-000000000000"),
                },
                "NOT_FOUND",
            ),
            (
                DennettError::Conflict {
                    resource: String::from("memory_note"),
                    expected_revision: Some(1),
                    actual_revision: Some(2),
                    reason: String::from("stale revision"),
                },
                "CONFLICT",
            ),
            (
                DennettError::IdempotencyMismatch {
                    idempotency_key: String::from("op-1"),
                    reason: String::from("payload fingerprint mismatch"),
                },
                "IDEMPOTENCY_MISMATCH",
            ),
        ];

        for (err, expected_code) in cases {
            assert_eq!(err.code(), expected_code);
            let envelope = ApiErrorResponse::new(err.clone(), CorrelationId::generate());
            assert_eq!(envelope.code, expected_code);
            let encoded = serde_json::to_string(&envelope);
            assert!(encoded.is_ok());
            if let Ok(json) = encoded {
                let decoded: Result<ApiErrorResponse, _> = serde_json::from_str(&json);
                assert_eq!(decoded.ok(), Some(envelope));
            }
        }
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
