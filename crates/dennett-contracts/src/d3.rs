//! Pure Phase D3 domain contracts for the write-only `SecretBroker`, runtime provider
//! connections & provider-quota errors, skills/plugins/MCP capability registry (`A08`),
//! and the V02-safe `Head` Memory Fabric overview proxy.
//!
//! Depends strictly on `std`, `serde`, `thiserror`, and `uuid` (zero extra crate dependencies).

use core::fmt;
use core::str::FromStr;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::control::{
    MAX_ADAPTER_ID_LEN, MAX_WORKSPACE_PATH_LEN, MemoryStatus, ProjectId, RuntimeCatalogResponse,
};
use crate::memory::{
    ConsolidationJob, ForgetTombstone, MemoryClaim, MemoryConflict, MemoryDocument, MemoryNote,
    MemoryScope, MemoryStatsSummary, Sha256Hex, TimestampMs,
};
use crate::redact::REDACTED_PLACEHOLDER;
use crate::{DennettError, RuntimeAuthProfile};

/// Maximum permitted byte length of a `SecretBroker` slot key.
pub const MAX_SECRET_SLOT_KEY_LEN: usize = 128;
/// Maximum permitted byte length of a plaintext secret value submitted to `SecretBroker` (16 KiB).
pub const MAX_SECRET_VALUE_BYTES: usize = 16 * 1024;
/// Maximum permitted byte length of a secret slot description.
pub const MAX_SECRET_DESCRIPTION_BYTES: usize = 512;
/// Maximum permitted byte length of a provider connection display name.
pub const MAX_CONNECTION_NAME_LEN: usize = 128;
/// Maximum permitted byte length of a provider connection endpoint URL.
pub const MAX_ENDPOINT_URL_LEN: usize = 512;
/// Maximum permitted byte length of a normalized capability slug.
pub const MAX_CAPABILITY_SLUG_LEN: usize = 128;
/// Maximum permitted byte length of a capability display name.
pub const MAX_CAPABILITY_NAME_LEN: usize = 128;
/// Maximum permitted byte length of a capability description (4 KiB).
pub const MAX_CAPABILITY_DESCRIPTION_BYTES: usize = 4 * 1024;
/// Maximum permitted byte length of a capability version string.
pub const MAX_CAPABILITY_VERSION_LEN: usize = 64;
/// Maximum permitted byte length of an inline skill markdown document (256 KiB).
pub const MAX_CAPABILITY_MARKDOWN_BYTES: usize = 256 * 1024;
/// Maximum number of items allowed in capability list fields (`args`, `entry_files`, `skills`, etc.).
pub const MAX_CAPABILITY_LIST_ITEMS: usize = 128;

macro_rules! define_d3_uuid_id {
    (
        $(#[$meta:meta])*
        pub struct $name:ident;
        field = $field:literal;
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        #[repr(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// Generates a fresh random UUID v4 identifier.
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            /// Wraps an existing [`Uuid`] into this typed identifier.
            #[must_use]
            pub const fn from_uuid(id: Uuid) -> Self {
                Self(id)
            }

            /// Returns the underlying [`Uuid`].
            #[must_use]
            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl FromStr for $name {
            type Err = DennettError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(s).map(Self).map_err(|_| DennettError::InvalidInput {
                    field: String::from($field),
                    reason: String::from("must be a valid UUID"),
                })
            }
        }
    };
}

define_d3_uuid_id! {
    /// Stable UUID identifying a persisted [`ProviderConnectionRecord`].
    pub struct ProviderConnectionId;
    field = "connection_id";
}

define_d3_uuid_id! {
    /// Stable UUID identifying a registered [`CapabilityRecord`].
    pub struct CapabilityId;
    field = "capability_id";
}

// ============================================================================
// Write-Only SecretBroker Contracts (Spec 30 §17)
// ============================================================================

/// Origin of the active credential backing a `SecretBroker` slot.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SecretSource {
    /// Stored encrypted in the Head PostgreSQL `secret_entries` vault.
    Vault,
    /// Supplied via process environment variable fallback (e.g. `GEMINI_API_KEY`).
    EnvFallback,
    /// Not configured in either the encrypted vault or process environment.
    #[default]
    Unset,
}

impl SecretSource {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Vault => "vault",
            Self::EnvFallback => "env_fallback",
            Self::Unset => "unset",
        }
    }
}

impl fmt::Display for SecretSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SecretSource {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "vault" => Ok(Self::Vault),
            "env_fallback" => Ok(Self::EnvFallback),
            "unset" => Ok(Self::Unset),
            other => Err(DennettError::InvalidInput {
                field: String::from("source"),
                reason: format!("unknown secret source `{other}`"),
            }),
        }
    }
}

fn default_secret_scope() -> String {
    String::from("global")
}

const fn default_true() -> bool {
    true
}

/// Validates a `SecretBroker` slot key (`"gemini_api_key"`, `"local_runtime_token"`, `"mcp:<slug>:<ENV>"`).
///
/// # Errors
///
/// Returns [`DennettError::InvalidInput`] if `slot_key` is empty, exceeds [`MAX_SECRET_SLOT_KEY_LEN`],
/// contains traversal (`..`), or uses disallowed characters.
pub fn validate_secret_slot_key(slot_key: &str) -> Result<(), DennettError> {
    if slot_key.is_empty() || slot_key.len() > MAX_SECRET_SLOT_KEY_LEN {
        return Err(DennettError::InvalidInput {
            field: String::from("slot_key"),
            reason: format!("must be non-empty and at most {MAX_SECRET_SLOT_KEY_LEN} bytes"),
        });
    }
    if slot_key.contains("..")
        || !slot_key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b':' | b'.'))
    {
        return Err(DennettError::InvalidInput {
            field: String::from("slot_key"),
            reason: String::from(
                "must contain only ASCII alphanumeric characters, '_', '-', ':', or '.' without '..'",
            ),
        });
    }
    Ok(())
}

/// Write-only request payload for `PUT /v1/secrets/{slot_key}`.
///
/// Custom [`fmt::Debug`] strictly redacts `value` so plaintext credentials never enter logs.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetSecretRequest {
    /// Raw secret plaintext to encrypt and store in `SecretBroker` (write-only, never returned on read).
    pub value: String,
    /// Scope selector (`"global"` or `"project:<uuid>"`, defaults to `"global"`).
    #[serde(default = "default_secret_scope")]
    pub scope: String,
    /// Optional human-readable description of this secret slot.
    #[serde(default)]
    pub description: String,
}

impl fmt::Debug for SetSecretRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SetSecretRequest")
            .field("value", &REDACTED_PLACEHOLDER)
            .field("scope", &self.scope)
            .field("description", &self.description)
            .finish()
    }
}

impl SetSecretRequest {
    /// Validates plaintext secret bounds, scope syntax, and description bounds.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `value` is empty/oversized/contains NUL,
    /// `scope` is neither `"global"` nor `"project:<uuid>"`, or `description` is invalid.
    pub fn validate(&self) -> Result<(), DennettError> {
        reject_nul(&self.value, "value")?;
        if self.value.trim().is_empty() || self.value.len() > MAX_SECRET_VALUE_BYTES {
            return Err(DennettError::InvalidInput {
                field: String::from("value"),
                reason: format!("must be non-empty and at most {MAX_SECRET_VALUE_BYTES} bytes"),
            });
        }
        validate_secret_scope(&self.scope)?;
        reject_nul(&self.description, "description")?;
        if self.description.len() > MAX_SECRET_DESCRIPTION_BYTES {
            return Err(DennettError::InvalidInput {
                field: String::from("description"),
                reason: format!("exceeds maximum length of {MAX_SECRET_DESCRIPTION_BYTES} bytes"),
            });
        }
        Ok(())
    }
}

/// Write-only secret slot metadata returned by `GET /v1/secrets`, `PUT /v1/secrets/{slot_key}`,
/// and `DELETE /v1/secrets/{slot_key}`.
///
/// Strictly omits plaintext secret material by design.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretMetadataRecord {
    /// Stable secret slot key (e.g. `"gemini_api_key"`, `"local_runtime_token"`, `"mcp:<slug>:<ENV>"`).
    pub slot_key: String,
    /// Owning scope (`"global"` or `"project:<uuid>"`).
    pub scope: String,
    /// Human-readable description of this secret slot.
    #[serde(default)]
    pub description: String,
    /// `true` when usable secret material is configured in vault or environment.
    pub configured: bool,
    /// Monotonic rotation version number (`0` when unset/env-only, `>= 1` when persisted in vault).
    pub version: i64,
    /// Non-secret 16-hex-character fingerprint prefix enabling rotation verification without leaking value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint_short: Option<String>,
    /// Active origin of the credential (`vault`, `env_fallback`, or `unset`).
    pub source: SecretSource,
    /// UTC timestamp of the last secret rotation, when stored in vault.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<TimestampMs>,
}

/// Response payload for `GET /v1/secrets` carrying non-secret slot metadata only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListSecretsResponse {
    /// Known and configured secret slot metadata records (never plaintext values).
    pub secrets: Vec<SecretMetadataRecord>,
}

// ============================================================================
// Runtime Provider Connections & Upstream Provider Quota Distinction
// ============================================================================

/// Active runtime provider families supported in D3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ProviderKind {
    /// Production Google Antigravity (`agy acp`) runtime adapter.
    AntigravityAcp,
    /// Local loopback HTTP runtime server (`local-http-runtime`: Ollama, LM Studio, `llama.cpp`).
    LocalHttp,
}

impl ProviderKind {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AntigravityAcp => "antigravity_acp",
            Self::LocalHttp => "local_http",
        }
    }

    /// Returns the default runtime adapter identifier associated with this provider kind.
    #[must_use]
    pub const fn default_adapter_id(self) -> &'static str {
        match self {
            Self::AntigravityAcp => "antigravity-acp",
            Self::LocalHttp => "local-http-runtime",
        }
    }
}

impl fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ProviderKind {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "antigravity_acp" => Ok(Self::AntigravityAcp),
            "local_http" => Ok(Self::LocalHttp),
            other => Err(DennettError::InvalidInput {
                field: String::from("provider_kind"),
                reason: format!("unknown provider kind `{other}`"),
            }),
        }
    }
}

/// Last-observed probe health disposition for a [`ProviderConnectionRecord`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ProviderHealthStatus {
    /// Runtime probe succeeded and returned live capabilities/models.
    Healthy,
    /// Runtime requires explicit user authentication under the selected profile.
    AuthRequired,
    /// Runtime binary or loopback HTTP server could not be reached.
    Unreachable,
    /// Runtime binary version or protocol response is incompatible with Dennett.
    Incompatible,
    /// Upstream provider rejected requests due to provider-side quota or rate limits.
    QuotaLimited,
    /// Connection has not been probed yet or status is indeterminate.
    #[default]
    Unknown,
}

impl ProviderHealthStatus {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::AuthRequired => "auth_required",
            Self::Unreachable => "unreachable",
            Self::Incompatible => "incompatible",
            Self::QuotaLimited => "quota_limited",
            Self::Unknown => "unknown",
        }
    }
}

impl fmt::Display for ProviderHealthStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ProviderHealthStatus {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "healthy" => Ok(Self::Healthy),
            "auth_required" => Ok(Self::AuthRequired),
            "unreachable" => Ok(Self::Unreachable),
            "incompatible" => Ok(Self::Incompatible),
            "quota_limited" => Ok(Self::QuotaLimited),
            "unknown" => Ok(Self::Unknown),
            other => Err(DennettError::InvalidInput {
                field: String::from("last_health_status"),
                reason: format!("unknown provider health status `{other}`"),
            }),
        }
    }
}

/// Structured upstream provider quota / rate-limit error, explicitly separated from
/// Dennett's own external [`crate::TaskBudget`] / [`crate::BudgetViolation`] governance limits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderQuotaError {
    /// Provider or runtime adapter identifier that reported quota exhaustion.
    pub provider: String,
    /// Authentication profile on which the upstream quota limit occurred.
    pub auth_profile: RuntimeAuthProfile,
    /// Secret-redacted diagnostic detail returned by the upstream provider.
    pub detail: String,
    /// Optional upstream retry delay hint in seconds, when reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_secs: Option<u64>,
}

/// Persisted runtime provider connection record managed by `dennett-head`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderConnectionRecord {
    /// Stable provider connection identifier.
    pub connection_id: ProviderConnectionId,
    /// Runtime provider family (`antigravity_acp` or `local_http`).
    pub provider_kind: ProviderKind,
    /// Human-readable label shown in Settings and selectors.
    pub display_name: String,
    /// Concrete runtime adapter identifier (`"antigravity-acp"` or `"local-http-runtime"`).
    pub adapter_id: String,
    /// Authentication profile bound to this connection (`oauth-personal`, `gemini-api-key`, `local-endpoint`).
    pub auth_profile: RuntimeAuthProfile,
    /// Optional HTTP base URL for `local_http` connections (loopback by default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint_url: Option<String>,
    /// Optional `SecretBroker` slot key supplying API credentials (never plaintext secret).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_slot_key: Option<String>,
    /// Whether this connection is currently the default interactive runtime.
    #[serde(default)]
    pub is_default_interactive: bool,
    /// Whether this connection is currently the default headless background runtime.
    #[serde(default)]
    pub is_default_background: bool,
    /// Whether this connection is enabled for selection and probing.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Last-observed probe health status.
    #[serde(default)]
    pub last_health_status: ProviderHealthStatus,
    /// Optional secret-free diagnostic detail from the latest probe.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_health_detail: Option<String>,
    /// Optional UTC timestamp of the latest probe.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_probed_at: Option<TimestampMs>,
    /// UTC creation timestamp.
    pub created_at: TimestampMs,
    /// UTC last-updated timestamp.
    pub updated_at: TimestampMs,
}

/// Read-only descriptor of a deferred/planned runtime shown as non-interactive status in Settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedProviderInfo {
    /// Stable roadmap identifier (e.g. `"r-claude"`, `"r-codex"`).
    pub id: String,
    /// Human-readable provider display label.
    pub label: String,
    /// Non-interactive lifecycle status marker (always `"planned"` in D3).
    pub status: String,
    /// Optional concise note explaining deferred rollout status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Response payload for `GET /v1/runtime/connections`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListProviderConnectionsResponse {
    /// Persisted active and disabled provider connections.
    pub connections: Vec<ProviderConnectionRecord>,
    /// Read-only planned provider descriptors (no login buttons or deferred dependencies).
    #[serde(default)]
    pub planned_providers: Vec<PlannedProviderInfo>,
}

/// Request payload for `POST /v1/runtime/connections` and `PATCH /v1/runtime/connections/{id}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpsertProviderConnectionRequest {
    /// Runtime provider family (`antigravity_acp` or `local_http`).
    pub provider_kind: ProviderKind,
    /// Human-readable display name.
    pub display_name: String,
    /// Optional explicit adapter identifier (inferred from `provider_kind` when omitted).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter_id: Option<String>,
    /// Authentication profile bound to this connection.
    pub auth_profile: RuntimeAuthProfile,
    /// Optional HTTP base URL (required for `local_http`, loopback by default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint_url: Option<String>,
    /// Optional `SecretBroker` slot key supplying an API key or bearer token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_slot_key: Option<String>,
    /// Optional flag marking this connection as default interactive runtime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_default_interactive: Option<bool>,
    /// Optional flag marking this connection as default headless background runtime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_default_background: Option<bool>,
    /// Optional enabled toggle (defaults to `true` on create).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

impl UpsertProviderConnectionRequest {
    /// Validates connection name, adapter id, endpoint URL, secret slot key, and kind/profile compatibility.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] on malformed fields or mismatched `provider_kind` / `auth_profile`.
    pub fn validate(&self) -> Result<(), DennettError> {
        validate_non_empty_text(&self.display_name, "display_name", MAX_CONNECTION_NAME_LEN)?;
        if let Some(ref adapter) = self.adapter_id {
            validate_non_empty_text(adapter, "adapter_id", MAX_ADAPTER_ID_LEN)?;
        }
        if let Some(ref slot) = self.secret_slot_key {
            validate_secret_slot_key(slot)?;
        }
        if let Some(ref url) = self.endpoint_url {
            validate_http_url(url, "endpoint_url")?;
        }
        match self.provider_kind {
            ProviderKind::AntigravityAcp => {
                if self.auth_profile == RuntimeAuthProfile::LocalEndpoint {
                    return Err(DennettError::InvalidInput {
                        field: String::from("auth_profile"),
                        reason: String::from(
                            "antigravity_acp requires `oauth-personal` or `gemini-api-key`",
                        ),
                    });
                }
            }
            ProviderKind::LocalHttp => {
                if self.auth_profile != RuntimeAuthProfile::LocalEndpoint {
                    return Err(DennettError::InvalidInput {
                        field: String::from("auth_profile"),
                        reason: String::from("local_http requires `local-endpoint` auth profile"),
                    });
                }
                if self.endpoint_url.is_none() {
                    return Err(DennettError::InvalidInput {
                        field: String::from("endpoint_url"),
                        reason: String::from("local_http connections require `endpoint_url`"),
                    });
                }
            }
        }
        Ok(())
    }
}

/// Target default role when activating a provider connection (`POST /v1/runtime/connections/{id}/activate`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ConnectionActivationRole {
    /// Activate as the default interactive runtime.
    #[default]
    Interactive,
    /// Activate as the default headless background runtime.
    Background,
    /// Activate as both default interactive and default background runtime.
    Both,
}

impl ConnectionActivationRole {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Interactive => "interactive",
            Self::Background => "background",
            Self::Both => "both",
        }
    }
}

impl fmt::Display for ConnectionActivationRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Request payload for `POST /v1/runtime/connections/{id}/activate`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivateConnectionRequest {
    /// Default role to assign to this connection (defaults to [`ConnectionActivationRole::Interactive`]).
    #[serde(default)]
    pub role: ConnectionActivationRole,
}

/// Response payload for `POST /v1/runtime/connections/{id}/probe`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeConnectionResponse {
    /// Updated provider connection record carrying fresh health status and probe timestamp.
    pub connection: ProviderConnectionRecord,
    /// Live probed model/thinking/quota catalog when the probe succeeded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog: Option<RuntimeCatalogResponse>,
}

/// Response payload for `DELETE /v1/runtime/connections/{id}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteProviderConnectionResponse {
    /// Identifier of the deleted connection.
    pub connection_id: ProviderConnectionId,
    /// Confirms the connection record was removed.
    pub deleted: bool,
}

// ============================================================================
// Capabilities Registry: Skills, MCP Servers, and Plugins (Spec 41, A08)
// ============================================================================

/// Category of capability managed by `dennett-head`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CapabilityKind {
    /// Markdown instruction skill bundle materialized into isolated runtime skill folders.
    Skill,
    /// Model Context Protocol server injected via `SessionStartRequest.mcp_servers`.
    McpServer,
    /// Composite plugin bundle grouping skills, MCP servers, and optional hooks.
    Plugin,
}

impl CapabilityKind {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Skill => "skill",
            Self::McpServer => "mcp_server",
            Self::Plugin => "plugin",
        }
    }
}

impl fmt::Display for CapabilityKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CapabilityKind {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "skill" => Ok(Self::Skill),
            "mcp_server" => Ok(Self::McpServer),
            "plugin" => Ok(Self::Plugin),
            other => Err(DennettError::InvalidInput {
                field: String::from("kind"),
                reason: format!("unknown capability kind `{other}`"),
            }),
        }
    }
}

/// Discovery or registration provenance of a [`CapabilityRecord`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CapabilityOrigin {
    /// Explicitly added or configured by the user.
    #[default]
    UserManual,
    /// Discovered inside a project's `.agents/skills` or `.dennett/` directory.
    ProjectLocal,
    /// Discovered read-only from the user's personal profile (e.g. `~/.gemini/`); always candidate/disabled by default.
    PersonalDiscovered,
    /// Shipped built-in capability provided by Dennett.
    Builtin,
}

impl CapabilityOrigin {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UserManual => "user_manual",
            Self::ProjectLocal => "project_local",
            Self::PersonalDiscovered => "personal_discovered",
            Self::Builtin => "builtin",
        }
    }
}

impl fmt::Display for CapabilityOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CapabilityOrigin {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "user_manual" => Ok(Self::UserManual),
            "project_local" => Ok(Self::ProjectLocal),
            "personal_discovered" => Ok(Self::PersonalDiscovered),
            "builtin" => Ok(Self::Builtin),
            other => Err(DennettError::InvalidInput {
                field: String::from("origin_kind"),
                reason: format!("unknown capability origin `{other}`"),
            }),
        }
    }
}

/// Administrative ownership boundary of a [`CapabilityRecord`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CapabilityOwnership {
    /// Owned and curated by the human user.
    #[default]
    User,
    /// Owned by a specific project repository.
    Project,
    /// Supplied by a runtime provider.
    Provider,
    /// Supplied by Dennett itself.
    Dennett,
}

impl CapabilityOwnership {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Project => "project",
            Self::Provider => "provider",
            Self::Dennett => "dennett",
        }
    }
}

impl fmt::Display for CapabilityOwnership {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CapabilityOwnership {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "user" => Ok(Self::User),
            "project" => Ok(Self::Project),
            "provider" => Ok(Self::Provider),
            "dennett" => Ok(Self::Dennett),
            other => Err(DennettError::InvalidInput {
                field: String::from("ownership"),
                reason: format!("unknown capability ownership `{other}`"),
            }),
        }
    }
}

/// Scope visibility of a [`CapabilityRecord`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CapabilityScopeKind {
    /// Available globally across projects unless overridden per project.
    Global,
    /// Scoped strictly to a single [`ProjectId`].
    Project,
    /// Discovered candidate not yet attached to any project or global pool (e.g. `~/.gemini/` read-only scan).
    #[default]
    CandidateUnattached,
}

impl CapabilityScopeKind {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Project => "project",
            Self::CandidateUnattached => "candidate_unattached",
        }
    }
}

impl fmt::Display for CapabilityScopeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CapabilityScopeKind {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "global" => Ok(Self::Global),
            "project" => Ok(Self::Project),
            "candidate_unattached" => Ok(Self::CandidateUnattached),
            other => Err(DennettError::InvalidInput {
                field: String::from("scope_kind"),
                reason: format!("unknown capability scope kind `{other}`"),
            }),
        }
    }
}

/// Possession lifecycle state of a [`CapabilityRecord`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CapabilityPossession {
    /// Found by discovery scan but not yet explicitly installed/adopted.
    #[default]
    Discovered,
    /// Installed into Dennett's registry.
    Installed,
    /// Actively wired/connected into runtime execution.
    Connected,
}

impl CapabilityPossession {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Discovered => "discovered",
            Self::Installed => "installed",
            Self::Connected => "connected",
        }
    }
}

impl fmt::Display for CapabilityPossession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CapabilityPossession {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "discovered" => Ok(Self::Discovered),
            "installed" => Ok(Self::Installed),
            "connected" => Ok(Self::Connected),
            other => Err(DennettError::InvalidInput {
                field: String::from("possession_state"),
                reason: format!("unknown capability possession state `{other}`"),
            }),
        }
    }
}

/// Security trust disposition of a [`CapabilityRecord`].
///
/// Updates that expand executable surfaces (`has_hooks`, `has_scripts`, or new `secret_slots`)
/// automatically downgrade trust to [`CapabilityTrust::Restricted`] (rug-pull protection, Spec 30 §16.5).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CapabilityTrust {
    /// Newly discovered or registered; not yet reviewed by the owner.
    #[default]
    Unreviewed,
    /// Restricted pending explicit owner re-approval (e.g. after executable-surface expansion).
    Restricted,
    /// Explicitly reviewed and trusted by the owner.
    Trusted,
    /// Revoked and blocked from activation.
    Revoked,
}

impl CapabilityTrust {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unreviewed => "unreviewed",
            Self::Restricted => "restricted",
            Self::Trusted => "trusted",
            Self::Revoked => "revoked",
        }
    }
}

impl fmt::Display for CapabilityTrust {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CapabilityTrust {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "unreviewed" => Ok(Self::Unreviewed),
            "restricted" => Ok(Self::Restricted),
            "trusted" => Ok(Self::Trusted),
            "revoked" => Ok(Self::Revoked),
            other => Err(DennettError::InvalidInput {
                field: String::from("trust_state"),
                reason: format!("unknown capability trust state `{other}`"),
            }),
        }
    }
}

/// Activation disposition for a capability globally or inside a specific project binding.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CapabilityActivation {
    /// Disabled and excluded from runtime sessions.
    #[default]
    Disabled,
    /// Available on demand when explicitly referenced.
    OnDemand,
    /// Active and materialized for eligible runtime sessions.
    Active,
    /// Pinned active at high priority.
    Pinned,
    /// Explicitly forbidden in a specific project override (`capability_project_bindings`).
    Forbidden,
}

impl CapabilityActivation {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::OnDemand => "on_demand",
            Self::Active => "active",
            Self::Pinned => "pinned",
            Self::Forbidden => "forbidden",
        }
    }
}

impl fmt::Display for CapabilityActivation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CapabilityActivation {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "disabled" => Ok(Self::Disabled),
            "on_demand" => Ok(Self::OnDemand),
            "active" => Ok(Self::Active),
            "pinned" => Ok(Self::Pinned),
            "forbidden" => Ok(Self::Forbidden),
            other => Err(DennettError::InvalidInput {
                field: String::from("activation_state"),
                reason: format!("unknown capability activation state `{other}`"),
            }),
        }
    }
}

/// Compatibility assessment of a capability against the current Dennett runtime.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CompatibilityStatus {
    /// Fully compatible with the active runtime.
    #[default]
    Compatible,
    /// Usable with non-fatal caveats or degraded optional features.
    Warning,
    /// Incompatible and cannot be materialized safely.
    Incompatible,
}

impl CompatibilityStatus {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Compatible => "compatible",
            Self::Warning => "warning",
            Self::Incompatible => "incompatible",
        }
    }
}

impl fmt::Display for CompatibilityStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CompatibilityStatus {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "compatible" => Ok(Self::Compatible),
            "warning" => Ok(Self::Warning),
            "incompatible" => Ok(Self::Incompatible),
            other => Err(DennettError::InvalidInput {
                field: String::from("compatibility_status"),
                reason: format!("unknown compatibility status `{other}`"),
            }),
        }
    }
}

/// Declarative MCP server entry bundled inside a [`CapabilitySpec::Plugin`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginMcpServerEntry {
    /// Logical server name within the plugin bundle.
    pub name: String,
    /// Executable command used to launch the MCP server.
    pub command: String,
    /// Command-line arguments passed to `command`.
    #[serde(default)]
    pub args: Vec<String>,
    /// Non-secret environment variables (`ENV_NAME -> VALUE`).
    #[serde(default)]
    pub env_vars: BTreeMap<String, String>,
    /// Mapping of environment variable name -> `SecretBroker` slot key (never raw secrets).
    #[serde(default)]
    pub secret_slots: BTreeMap<String, String>,
}

impl PluginMcpServerEntry {
    /// Validates nested plugin MCP server command, arguments, and secret slot references.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] on empty/oversized/NUL-containing fields or invalid slot keys.
    pub fn validate(&self) -> Result<(), DennettError> {
        validate_non_empty_text(&self.name, "mcp_servers.name", MAX_CAPABILITY_NAME_LEN)?;
        validate_mcp_command_spec(
            &self.command,
            &self.args,
            &self.env_vars,
            &self.secret_slots,
        )
    }
}

/// Kind-specific declarative specification payload persisted on a [`CapabilityRecord`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum CapabilitySpec {
    /// Markdown instruction skill bundle materialized into isolated runtime skill folders.
    Skill {
        /// Full UTF-8 `SKILL.md` body.
        skill_markdown: String,
        /// `true` when the skill bundle contains executable helper scripts.
        #[serde(default)]
        has_scripts: bool,
        /// Relative file names discovered inside the skill bundle.
        #[serde(default)]
        entry_files: Vec<String>,
    },
    /// Model Context Protocol server injected into runtime sessions.
    McpServer {
        /// Executable command used to launch the MCP server.
        command: String,
        /// Command-line arguments passed to `command`.
        #[serde(default)]
        args: Vec<String>,
        /// Non-secret environment variables (`ENV_NAME -> VALUE`).
        #[serde(default)]
        env_vars: BTreeMap<String, String>,
        /// Mapping of environment variable name -> `SecretBroker` slot key (never raw secrets).
        #[serde(default)]
        secret_slots: BTreeMap<String, String>,
    },
    /// Composite plugin bundle grouping skills, MCP servers, and optional runtime hooks.
    Plugin {
        /// Plugin manifest version string (e.g. `"1.0"`).
        manifest_version: String,
        /// Skill slugs bundled by this plugin.
        #[serde(default)]
        skills: Vec<String>,
        /// MCP server declarations bundled by this plugin.
        #[serde(default)]
        mcp_servers: Vec<PluginMcpServerEntry>,
        /// `true` when the plugin declares executable runtime hooks.
        #[serde(default)]
        has_hooks: bool,
        /// Component identifiers currently enabled within this plugin bundle.
        #[serde(default)]
        enabled_components: Vec<String>,
    },
}

impl CapabilitySpec {
    /// Returns the corresponding [`CapabilityKind`] discriminator.
    #[must_use]
    pub const fn kind(&self) -> CapabilityKind {
        match self {
            Self::Skill { .. } => CapabilityKind::Skill,
            Self::McpServer { .. } => CapabilityKind::McpServer,
            Self::Plugin { .. } => CapabilityKind::Plugin,
        }
    }

    /// Validates specification bounds, NUL-safety, and secret slot key references.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] on empty/oversized/malformed specification fields.
    pub fn validate(&self) -> Result<(), DennettError> {
        match self {
            Self::Skill {
                skill_markdown,
                entry_files,
                ..
            } => {
                validate_non_empty_text(
                    skill_markdown,
                    "spec.skill_markdown",
                    MAX_CAPABILITY_MARKDOWN_BYTES,
                )?;
                validate_bounded_list(entry_files, "spec.entry_files", MAX_WORKSPACE_PATH_LEN)
            }
            Self::McpServer {
                command,
                args,
                env_vars,
                secret_slots,
            } => validate_mcp_command_spec(command, args, env_vars, secret_slots),
            Self::Plugin {
                manifest_version,
                skills,
                mcp_servers,
                enabled_components,
                ..
            } => {
                validate_non_empty_text(
                    manifest_version,
                    "spec.manifest_version",
                    MAX_CAPABILITY_VERSION_LEN,
                )?;
                validate_bounded_list(skills, "spec.skills", MAX_CAPABILITY_SLUG_LEN)?;
                if mcp_servers.len() > MAX_CAPABILITY_LIST_ITEMS {
                    return Err(DennettError::InvalidInput {
                        field: String::from("spec.mcp_servers"),
                        reason: format!("exceeds maximum of {MAX_CAPABILITY_LIST_ITEMS} entries"),
                    });
                }
                for server in mcp_servers {
                    server.validate()?;
                }
                validate_bounded_list(
                    enabled_components,
                    "spec.enabled_components",
                    MAX_CAPABILITY_NAME_LEN,
                )
            }
        }
    }
}

/// Per-project activation override for a registered capability (`capability_project_bindings`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityProjectBinding {
    /// Target capability identifier.
    pub capability_id: CapabilityId,
    /// Target project identifier.
    pub project_id: ProjectId,
    /// Effective project-level activation state (`active`, `on_demand`, `disabled`, or `forbidden`).
    pub activation_state: CapabilityActivation,
    /// UTC timestamp of the latest binding update.
    pub updated_at: TimestampMs,
}

/// Registered skill, MCP server, or plugin capability managed by `dennett-head` (A08).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityRecord {
    /// Stable capability identifier.
    pub capability_id: CapabilityId,
    /// Normalized capability slug (e.g. `"rust-audit"`, `"local-postgres-mcp"`).
    pub slug: String,
    /// Capability category (`skill`, `mcp_server`, or `plugin`).
    pub kind: CapabilityKind,
    /// Human-readable display name.
    pub display_name: String,
    /// Human-readable description of what this capability provides.
    #[serde(default)]
    pub description: String,
    /// Semantic or snapshot version string (e.g. `"1.0.0"`).
    pub version: String,
    /// Discovery or registration provenance (`user_manual`, `project_local`, `personal_discovered`, `builtin`).
    pub origin_kind: CapabilityOrigin,
    /// Ownership boundary (`user`, `project`, `provider`, `dennett`).
    pub ownership: CapabilityOwnership,
    /// Scope visibility (`global`, `project`, or `candidate_unattached`).
    pub scope_kind: CapabilityScopeKind,
    /// Owning project identifier when `scope_kind == CapabilityScopeKind::Project`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
    /// Possession lifecycle state (`discovered`, `installed`, `connected`).
    pub possession_state: CapabilityPossession,
    /// Security trust disposition (`unreviewed`, `restricted`, `trusted`, `revoked`).
    pub trust_state: CapabilityTrust,
    /// Default activation state (`disabled`, `on_demand`, `active`, `pinned`).
    pub activation_state: CapabilityActivation,
    /// Runtime compatibility status (`compatible`, `warning`, `incompatible`).
    pub compatibility_status: CompatibilityStatus,
    /// Optional human-readable compatibility detail or warning.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatibility_detail: Option<String>,
    /// Optional read-only source path from which this capability was discovered or imported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    /// Lowercase SHA-256 hex digest of the canonical capability content.
    pub content_sha256: Sha256Hex,
    /// Kind-specific declarative specification payload.
    pub spec: CapabilitySpec,
    /// `true` when a previous snapshot is available for 1-click rollback.
    #[serde(default)]
    pub can_rollback: bool,
    /// Monotonic revision counter (`>= 1`).
    pub revision: i64,
    /// Per-project activation overrides associated with this capability.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub project_bindings: Vec<CapabilityProjectBinding>,
    /// UTC creation timestamp.
    pub created_at: TimestampMs,
    /// UTC timestamp of the latest revision.
    pub updated_at: TimestampMs,
}

/// Response payload for `GET /v1/capabilities`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListCapabilitiesResponse {
    /// Matching capability records.
    pub capabilities: Vec<CapabilityRecord>,
}

/// Request payload for `POST /v1/capabilities/discover`.
///
/// Scanning personal profile directories (`~/.gemini/`) is strictly read-only and never writes
/// back to the user's personal profile.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoverCapabilitiesRequest {
    /// Optional project whose workspace (`.agents/skills`, `.dennett/*`) should be scanned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
    /// When `true`, performs a strictly read-only discovery scan of personal `~/.gemini/` candidates.
    #[serde(default)]
    pub scan_personal_gemini_readonly: bool,
}

/// Response payload for `POST /v1/capabilities/discover`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoverCapabilitiesResponse {
    /// Discovered or refreshed capability records.
    pub discovered: Vec<CapabilityRecord>,
    /// Number of newly registered candidate capabilities.
    pub newly_registered: u64,
    /// Number of previously known capabilities refreshed during this scan.
    pub updated_existing: u64,
    /// Non-fatal diagnostic warnings encountered during read-only discovery.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// Request payload for `POST /v1/capabilities`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateCapabilityRequest {
    /// Normalized capability slug.
    pub slug: String,
    /// Human-readable display name.
    pub display_name: String,
    /// Optional human-readable description.
    #[serde(default)]
    pub description: String,
    /// Version label (e.g. `"1.0.0"`).
    pub version: String,
    /// Discovery or registration provenance (defaults to [`CapabilityOrigin::UserManual`]).
    #[serde(default)]
    pub origin_kind: CapabilityOrigin,
    /// Administrative ownership (defaults to [`CapabilityOwnership::User`]).
    #[serde(default)]
    pub ownership: CapabilityOwnership,
    /// Scope visibility (`global`, `project`, or `candidate_unattached`).
    pub scope_kind: CapabilityScopeKind,
    /// Owning project identifier (required when `scope_kind == CapabilityScopeKind::Project`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
    /// Initial trust disposition (defaults to [`CapabilityTrust::Unreviewed`]).
    #[serde(default)]
    pub trust_state: CapabilityTrust,
    /// Initial activation disposition (defaults to [`CapabilityActivation::Disabled`]).
    #[serde(default)]
    pub activation_state: CapabilityActivation,
    /// Optional read-only source path selected via native folder picker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    /// Kind-specific declarative specification payload.
    pub spec: CapabilitySpec,
}

impl CreateCapabilityRequest {
    /// Validates slug, metadata, scope/project consistency, source path, and specification payload.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] on invalid fields or missing `project_id` for project scope.
    pub fn validate(&self) -> Result<(), DennettError> {
        validate_slug(&self.slug)?;
        validate_non_empty_text(&self.display_name, "display_name", MAX_CAPABILITY_NAME_LEN)?;
        reject_nul(&self.description, "description")?;
        if self.description.len() > MAX_CAPABILITY_DESCRIPTION_BYTES {
            return Err(DennettError::InvalidInput {
                field: String::from("description"),
                reason: format!(
                    "exceeds maximum length of {MAX_CAPABILITY_DESCRIPTION_BYTES} bytes"
                ),
            });
        }
        validate_non_empty_text(&self.version, "version", MAX_CAPABILITY_VERSION_LEN)?;
        if self.scope_kind == CapabilityScopeKind::Project && self.project_id.is_none() {
            return Err(DennettError::InvalidInput {
                field: String::from("project_id"),
                reason: String::from("project_id is required when `scope_kind` is `project`"),
            });
        }
        if self.activation_state == CapabilityActivation::Forbidden {
            return Err(DennettError::InvalidInput {
                field: String::from("activation_state"),
                reason: String::from(
                    "`forbidden` is only valid on per-project capability bindings",
                ),
            });
        }
        if let Some(ref src) = self.source_path {
            validate_non_empty_text(src, "source_path", MAX_WORKSPACE_PATH_LEN)?;
        }
        self.spec.validate()
    }
}

/// Request payload for `PATCH /v1/capabilities/{capability_id}`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateCapabilityRequest {
    /// Optional updated display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Optional updated description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional updated version label (triggers snapshot preservation for rollback).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Optional updated scope visibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_kind: Option<CapabilityScopeKind>,
    /// Optional owning project identifier (required when moving into `scope_kind == Project`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
    /// Optional updated trust disposition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trust_state: Option<CapabilityTrust>,
    /// Optional updated activation disposition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activation_state: Option<CapabilityActivation>,
    /// Optional updated specification payload (must match the capability's existing `kind`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<CapabilitySpec>,
}

impl UpdateCapabilityRequest {
    /// Validates that at least one field is updated and all provided fields meet invariants.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if no field is provided or any field fails validation.
    pub fn validate(&self) -> Result<(), DennettError> {
        if self.display_name.is_none()
            && self.description.is_none()
            && self.version.is_none()
            && self.scope_kind.is_none()
            && self.project_id.is_none()
            && self.trust_state.is_none()
            && self.activation_state.is_none()
            && self.spec.is_none()
        {
            return Err(DennettError::InvalidInput {
                field: String::from("body"),
                reason: String::from("must specify at least one capability field to update"),
            });
        }
        if let Some(ref name) = self.display_name {
            validate_non_empty_text(name, "display_name", MAX_CAPABILITY_NAME_LEN)?;
        }
        if let Some(ref desc) = self.description {
            reject_nul(desc, "description")?;
            if desc.len() > MAX_CAPABILITY_DESCRIPTION_BYTES {
                return Err(DennettError::InvalidInput {
                    field: String::from("description"),
                    reason: format!(
                        "exceeds maximum length of {MAX_CAPABILITY_DESCRIPTION_BYTES} bytes"
                    ),
                });
            }
        }
        if let Some(ref version) = self.version {
            validate_non_empty_text(version, "version", MAX_CAPABILITY_VERSION_LEN)?;
        }
        if self.scope_kind == Some(CapabilityScopeKind::Project) && self.project_id.is_none() {
            return Err(DennettError::InvalidInput {
                field: String::from("project_id"),
                reason: String::from(
                    "project_id is required when transitioning `scope_kind` to `project`",
                ),
            });
        }
        if self.activation_state == Some(CapabilityActivation::Forbidden) {
            return Err(DennettError::InvalidInput {
                field: String::from("activation_state"),
                reason: String::from(
                    "`forbidden` is only valid on per-project capability bindings",
                ),
            });
        }
        if let Some(ref spec) = self.spec {
            spec.validate()?;
        }
        Ok(())
    }
}

/// Request payload for `PUT /v1/projects/{project_id}/capabilities/{capability_id}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetProjectCapabilityBindingRequest {
    /// Effective project-level activation override (`active`, `on_demand`, `disabled`, or `forbidden`).
    pub activation_state: CapabilityActivation,
}

/// Response payload for `DELETE /v1/capabilities/{capability_id}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteCapabilityResponse {
    /// Identifier of the deleted capability.
    pub capability_id: CapabilityId,
    /// Confirms the registry record was deleted (user files on disk remain untouched).
    pub deleted: bool,
}

// ============================================================================
// V02-Safe Head Memory Fabric Overview Proxy
// ============================================================================

/// V02-safe aggregated Memory Fabric snapshot exposed by `dennett-head` (`GET /v1/memory/overview`).
///
/// When standalone `dennett-memoryd` is unreachable, `memory_status` is
/// [`MemoryStatus::DegradedUnreachable`], `degraded_reason` explains the outage, and all
/// collections remain empty rather than fabricating memory answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryOverviewResponse {
    /// Live reachability status of standalone `dennett-memoryd`.
    pub memory_status: MemoryStatus,
    /// Optional safe explanation when `memory_status == MemoryStatus::DegradedUnreachable`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degraded_reason: Option<String>,
    /// Optional scope filter applied to this overview snapshot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<MemoryScope>,
    /// Scoped statistics summary when `memoryd` is reachable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stats: Option<MemoryStatsSummary>,
    /// Active notes visible in the queried scope(s).
    #[serde(default)]
    pub notes: Vec<MemoryNote>,
    /// Structured claims visible in the queried scope(s).
    #[serde(default)]
    pub claims: Vec<MemoryClaim>,
    /// Tracked conflicts visible in the queried scope(s).
    #[serde(default)]
    pub conflicts: Vec<MemoryConflict>,
    /// Versioned dossiers/documents visible in the queried scope(s).
    #[serde(default)]
    pub documents: Vec<MemoryDocument>,
    /// Consolidation jobs in the queried scope(s).
    #[serde(default)]
    pub consolidation_jobs: Vec<ConsolidationJob>,
    /// Content-free forget audit tombstones in the queried scope(s).
    #[serde(default)]
    pub tombstones: Vec<ForgetTombstone>,
}

// ============================================================================
// Internal Validation Helpers
// ============================================================================

fn reject_nul(value: &str, field: &str) -> Result<(), DennettError> {
    if value.as_bytes().contains(&0) {
        return Err(DennettError::InvalidInput {
            field: field.to_owned(),
            reason: String::from("must not contain NUL bytes"),
        });
    }
    Ok(())
}

fn validate_non_empty_text(value: &str, field: &str, max_bytes: usize) -> Result<(), DennettError> {
    reject_nul(value, field)?;
    if value.trim().is_empty() || value.len() > max_bytes {
        return Err(DennettError::InvalidInput {
            field: field.to_owned(),
            reason: format!("must be non-empty and at most {max_bytes} bytes"),
        });
    }
    Ok(())
}

fn validate_secret_scope(scope: &str) -> Result<(), DennettError> {
    if scope == "global" {
        return Ok(());
    }
    if let Some(rest) = scope.strip_prefix("project:") {
        Uuid::parse_str(rest).map_err(|_| DennettError::InvalidInput {
            field: String::from("scope"),
            reason: String::from("project secret scope must carry a valid UUID (`project:<uuid>`)"),
        })?;
        return Ok(());
    }
    Err(DennettError::InvalidInput {
        field: String::from("scope"),
        reason: String::from("must be `global` or `project:<uuid>`"),
    })
}

fn validate_http_url(url: &str, field: &str) -> Result<(), DennettError> {
    reject_nul(url, field)?;
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_ENDPOINT_URL_LEN {
        return Err(DennettError::InvalidInput {
            field: field.to_owned(),
            reason: format!("must be non-empty and at most {MAX_ENDPOINT_URL_LEN} bytes"),
        });
    }
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://"))
        || trimmed.len() <= "http://".len()
        || trimmed
            .bytes()
            .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
    {
        return Err(DennettError::InvalidInput {
            field: field.to_owned(),
            reason: String::from("must be a valid http:// or https:// URL without whitespace"),
        });
    }
    Ok(())
}

fn validate_slug(slug: &str) -> Result<(), DennettError> {
    if slug.is_empty() || slug.len() > MAX_CAPABILITY_SLUG_LEN {
        return Err(DennettError::InvalidInput {
            field: String::from("slug"),
            reason: format!("must be non-empty and at most {MAX_CAPABILITY_SLUG_LEN} bytes"),
        });
    }
    if slug.contains("..")
        || !slug
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
    {
        return Err(DennettError::InvalidInput {
            field: String::from("slug"),
            reason: String::from(
                "must contain only ASCII alphanumeric characters, '-', '_', '.', or ':' without '..'",
            ),
        });
    }
    Ok(())
}

fn validate_bounded_list(
    items: &[String],
    field: &str,
    max_item_bytes: usize,
) -> Result<(), DennettError> {
    if items.len() > MAX_CAPABILITY_LIST_ITEMS {
        return Err(DennettError::InvalidInput {
            field: field.to_owned(),
            reason: format!("exceeds maximum of {MAX_CAPABILITY_LIST_ITEMS} items"),
        });
    }
    for item in items {
        validate_non_empty_text(item, field, max_item_bytes)?;
    }
    Ok(())
}

fn validate_mcp_command_spec(
    command: &str,
    args: &[String],
    env_vars: &BTreeMap<String, String>,
    secret_slots: &BTreeMap<String, String>,
) -> Result<(), DennettError> {
    validate_non_empty_text(command, "spec.command", MAX_WORKSPACE_PATH_LEN)?;
    if args.len() > MAX_CAPABILITY_LIST_ITEMS {
        return Err(DennettError::InvalidInput {
            field: String::from("spec.args"),
            reason: format!("exceeds maximum of {MAX_CAPABILITY_LIST_ITEMS} arguments"),
        });
    }
    for arg in args {
        reject_nul(arg, "spec.args")?;
        if arg.len() > MAX_WORKSPACE_PATH_LEN {
            return Err(DennettError::InvalidInput {
                field: String::from("spec.args"),
                reason: format!("each argument must be at most {MAX_WORKSPACE_PATH_LEN} bytes"),
            });
        }
    }
    if env_vars.len() > MAX_CAPABILITY_LIST_ITEMS {
        return Err(DennettError::InvalidInput {
            field: String::from("spec.env_vars"),
            reason: format!("exceeds maximum of {MAX_CAPABILITY_LIST_ITEMS} entries"),
        });
    }
    for (k, v) in env_vars {
        validate_non_empty_text(k, "spec.env_vars", MAX_SECRET_SLOT_KEY_LEN)?;
        reject_nul(v, "spec.env_vars")?;
        if v.len() > MAX_CAPABILITY_DESCRIPTION_BYTES {
            return Err(DennettError::InvalidInput {
                field: String::from("spec.env_vars"),
                reason: format!(
                    "value exceeds maximum length of {MAX_CAPABILITY_DESCRIPTION_BYTES} bytes"
                ),
            });
        }
    }
    if secret_slots.len() > MAX_CAPABILITY_LIST_ITEMS {
        return Err(DennettError::InvalidInput {
            field: String::from("spec.secret_slots"),
            reason: format!("exceeds maximum of {MAX_CAPABILITY_LIST_ITEMS} entries"),
        });
    }
    for (env_name, slot_key) in secret_slots {
        validate_non_empty_text(env_name, "spec.secret_slots", MAX_SECRET_SLOT_KEY_LEN)?;
        validate_secret_slot_key(slot_key)?;
    }
    Ok(())
}

const _: () = {
    const fn assert_send_sync<T: Send + Sync + 'static>() {}
    assert_send_sync::<ProviderConnectionId>();
    assert_send_sync::<CapabilityId>();
    assert_send_sync::<SecretSource>();
    assert_send_sync::<SetSecretRequest>();
    assert_send_sync::<SecretMetadataRecord>();
    assert_send_sync::<ListSecretsResponse>();
    assert_send_sync::<ProviderKind>();
    assert_send_sync::<ProviderHealthStatus>();
    assert_send_sync::<ProviderQuotaError>();
    assert_send_sync::<ProviderConnectionRecord>();
    assert_send_sync::<PlannedProviderInfo>();
    assert_send_sync::<ListProviderConnectionsResponse>();
    assert_send_sync::<UpsertProviderConnectionRequest>();
    assert_send_sync::<ConnectionActivationRole>();
    assert_send_sync::<ActivateConnectionRequest>();
    assert_send_sync::<ProbeConnectionResponse>();
    assert_send_sync::<DeleteProviderConnectionResponse>();
    assert_send_sync::<CapabilityKind>();
    assert_send_sync::<CapabilityOrigin>();
    assert_send_sync::<CapabilityOwnership>();
    assert_send_sync::<CapabilityScopeKind>();
    assert_send_sync::<CapabilityPossession>();
    assert_send_sync::<CapabilityTrust>();
    assert_send_sync::<CapabilityActivation>();
    assert_send_sync::<CompatibilityStatus>();
    assert_send_sync::<PluginMcpServerEntry>();
    assert_send_sync::<CapabilitySpec>();
    assert_send_sync::<CapabilityProjectBinding>();
    assert_send_sync::<CapabilityRecord>();
    assert_send_sync::<ListCapabilitiesResponse>();
    assert_send_sync::<DiscoverCapabilitiesRequest>();
    assert_send_sync::<DiscoverCapabilitiesResponse>();
    assert_send_sync::<CreateCapabilityRequest>();
    assert_send_sync::<UpdateCapabilityRequest>();
    assert_send_sync::<SetProjectCapabilityBindingRequest>();
    assert_send_sync::<DeleteCapabilityResponse>();
    assert_send_sync::<MemoryOverviewResponse>();
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_secret_request_debug_strictly_redacts_plaintext_value() {
        let secret = "super-sensitive-api-key-987654321";
        let req = SetSecretRequest {
            value: String::from(secret),
            scope: String::from("global"),
            description: String::from("Gemini API key"),
        };
        assert!(req.validate().is_ok());
        let dbg = format!("{req:?}");
        assert!(
            !dbg.contains(secret),
            "Debug leaked plaintext secret: {dbg}"
        );
        assert!(dbg.contains(REDACTED_PLACEHOLDER));

        assert!(validate_secret_slot_key("gemini_api_key").is_ok());
        assert!(validate_secret_slot_key("mcp:docs:API_TOKEN").is_ok());
        assert!(validate_secret_slot_key("../escape").is_err());
        assert!(validate_secret_slot_key("has space").is_err());
    }

    #[test]
    fn provider_connection_validation_enforces_profile_and_endpoint_invariants() {
        let local_ok = UpsertProviderConnectionRequest {
            provider_kind: ProviderKind::LocalHttp,
            display_name: String::from("Local Ollama"),
            adapter_id: None,
            auth_profile: RuntimeAuthProfile::LocalEndpoint,
            endpoint_url: Some(String::from("http://127.0.0.1:11434")),
            secret_slot_key: Some(String::from("local_runtime_token")),
            is_default_interactive: Some(true),
            is_default_background: None,
            enabled: Some(true),
        };
        assert!(local_ok.validate().is_ok());

        let missing_url = UpsertProviderConnectionRequest {
            endpoint_url: None,
            ..local_ok.clone()
        };
        assert!(missing_url.validate().is_err());

        let mismatched_profile = UpsertProviderConnectionRequest {
            auth_profile: RuntimeAuthProfile::OAuthPersonal,
            ..local_ok
        };
        assert!(mismatched_profile.validate().is_err());
    }

    #[test]
    fn capability_requests_validate_specs_and_roundtrip_json() {
        let create_req = CreateCapabilityRequest {
            slug: String::from("rust-audit"),
            display_name: String::from("Rust Audit"),
            description: String::from("Static safety checklist"),
            version: String::from("1.0.0"),
            origin_kind: CapabilityOrigin::ProjectLocal,
            ownership: CapabilityOwnership::Project,
            scope_kind: CapabilityScopeKind::Project,
            project_id: Some(ProjectId::new()),
            trust_state: CapabilityTrust::Unreviewed,
            activation_state: CapabilityActivation::Disabled,
            source_path: None,
            spec: CapabilitySpec::Skill {
                skill_markdown: String::from("# Rust Audit\nCheck invariants."),
                has_scripts: false,
                entry_files: vec![String::from("SKILL.md")],
            },
        };
        assert!(create_req.validate().is_ok());
        assert_eq!(create_req.spec.kind(), CapabilityKind::Skill);

        let encoded = serde_json::to_string(&create_req).expect("serialize capability");
        let decoded: CreateCapabilityRequest =
            serde_json::from_str(&encoded).expect("deserialize capability");
        assert_eq!(decoded, create_req);

        let missing_project = CreateCapabilityRequest {
            project_id: None,
            ..create_req
        };
        assert!(missing_project.validate().is_err());

        let empty_update = UpdateCapabilityRequest::default();
        assert!(empty_update.validate().is_err());
    }
}
