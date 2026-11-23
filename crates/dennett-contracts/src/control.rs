//! Pure domain contracts for Dennett Head (`dennett-head`), runtime bindings,
//! external policy enforcement, artifacts, memory outbox, and HTTP API DTOs.
//!
//! Strictly depends only on `std`, `serde`, `thiserror`, and `uuid` so `dennett-head`,
//! `dennett-head-client`, `adapters/antigravity-acp`, and future UI layers share one
//! canonical wire contract without pulling SQL drivers, HTTP stacks, or provider SDKs.

use core::fmt;
use core::str::FromStr;
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    CorrelationId, DennettError, IdempotencyKey, MemoryEventId, MemoryNote, MemoryNoteId,
    MemoryScope, ProfileMode, RuntimeAuthProfile, ServiceKind, Sha256Hex, TimestampMs,
};

/// Maximum permitted byte length of an agent display name.
pub const MAX_AGENT_NAME_LEN: usize = 128;
/// Maximum permitted byte length of an agent description.
pub const MAX_AGENT_DESCRIPTION_BYTES: usize = 2 * 1024;
/// Maximum permitted byte length of an agent system prompt (64 KiB).
pub const MAX_SYSTEM_PROMPT_BYTES: usize = 64 * 1024;
/// Maximum permitted byte length of a model identifier string.
pub const MAX_MODEL_NAME_LEN: usize = 128;
/// Maximum permitted byte length of a project display name.
pub const MAX_PROJECT_NAME_LEN: usize = 128;
/// Maximum permitted byte length of a workspace filesystem path string.
pub const MAX_WORKSPACE_PATH_LEN: usize = 1024;
/// Maximum permitted byte length of a session title.
pub const MAX_SESSION_TITLE_LEN: usize = 256;
/// Maximum permitted byte length of a single chat message body (128 KiB).
pub const MAX_MESSAGE_CONTENT_BYTES: usize = 128 * 1024;
/// Maximum permitted byte length of an artifact display name.
pub const MAX_ARTIFACT_NAME_LEN: usize = 256;
/// Maximum permitted byte length of an artifact MIME type.
pub const MAX_ARTIFACT_MIME_LEN: usize = 128;
/// Maximum permitted inline artifact payload size (256 KiB).
pub const MAX_ARTIFACT_CONTENT_BYTES: usize = 256 * 1024;
/// Maximum permitted byte length of a policy tool name or target pattern.
pub const MAX_POLICY_PATTERN_LEN: usize = 256;
/// Maximum permitted byte length of a policy scope selector.
pub const MAX_POLICY_SCOPE_LEN: usize = 128;
/// Maximum permitted byte length of a policy reason or diagnostic summary.
pub const MAX_REASON_BYTES: usize = 1024;
/// Maximum permitted number of allowed relative write prefixes on a [`WorkspaceWritePolicy`].
pub const MAX_ALLOWED_WRITE_PREFIXES: usize = 64;
/// Maximum permitted number of memory scopes assigned to one agent configuration.
pub const MAX_AGENT_MEMORY_SCOPES: usize = 32;
/// Maximum permitted byte length of a runtime adapter identifier.
pub const MAX_ADAPTER_ID_LEN: usize = 64;
/// Maximum permitted byte length of a provider session handle.
pub const MAX_PROVIDER_SESSION_ID_LEN: usize = 256;

macro_rules! define_control_uuid_id {
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

define_control_uuid_id! {
    /// Stable UUID identifying a Dennett agent across configuration and model updates.
    pub struct AgentId;
    field = "agent_id";
}

define_control_uuid_id! {
    /// Stable UUID identifying an immutable [`AgentConfigVersion`] snapshot.
    pub struct AgentConfigVersionId;
    field = "agent_config_version_id";
}

define_control_uuid_id! {
    /// Stable UUID identifying a Dennett [`ProjectRecord`].
    pub struct ProjectId;
    field = "project_id";
}

define_control_uuid_id! {
    /// Stable UUID identifying a persisted Dennett [`SessionRecord`].
    pub struct SessionId;
    field = "session_id";
}

define_control_uuid_id! {
    /// Stable UUID identifying a persisted [`MessageRecord`] in a session.
    pub struct MessageId;
    field = "message_id";
}

define_control_uuid_id! {
    /// Stable UUID identifying one concrete agent execution [`RunRecord`].
    pub struct RunId;
    field = "run_id";
}

define_control_uuid_id! {
    /// Stable UUID identifying a persisted [`ArtifactRecord`].
    pub struct ArtifactId;
    field = "artifact_id";
}

define_control_uuid_id! {
    /// Stable UUID identifying a persisted [`RuntimeBinding`].
    pub struct RuntimeBindingId;
    field = "runtime_binding_id";
}

define_control_uuid_id! {
    /// Stable UUID identifying a durable [`MemoryOutboxItem`].
    pub struct OutboxItemId;
    field = "outbox_id";
}

define_control_uuid_id! {
    /// Stable UUID identifying an external [`PolicyRule`].
    pub struct PolicyRuleId;
    field = "rule_id";
}

define_control_uuid_id! {
    /// Stable UUID identifying an audited [`PermissionDecisionRecord`].
    pub struct PermissionDecisionId;
    field = "decision_id";
}

define_control_uuid_id! {
    /// Stable UUID identifying a secret-free [`DiagnosticEvent`].
    pub struct AuditEventId;
    field = "event_id";
}

/// Reasoning effort preference mapped to `agy acp` `thought_level` configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ThinkingLevel {
    /// Low reasoning effort (`low`).
    Low,
    /// Medium reasoning effort (`medium`).
    Medium,
    /// High reasoning effort (`high`).
    High,
    /// Extra-high reasoning effort (`xhigh`).
    #[serde(rename = "xhigh", alias = "x_high")]
    XHigh,
}

impl ThinkingLevel {
    /// Returns the canonical ACP `thought_level` string identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "xhigh",
        }
    }

    /// Returns the human-readable UI display label matching `agy acp` `thought_level`.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::XHigh => "Extra",
        }
    }
}

/// One reasoning-effort choice advertised dynamically by the active runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeThinkingOption {
    /// Canonical reasoning level identifier.
    pub value: ThinkingLevel,
    /// Human-readable label returned by the runtime (e.g. `"Low"`, `"Medium"`, `"High"`, `"Extra"`).
    pub label: String,
}

/// One model family advertised dynamically by the active runtime (`GET /v1/runtime/catalog`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeModelOption {
    /// Runtime model family identifier (e.g. `"gemini-4-neon"`, `"claude-opus-4-6-thinking"`).
    pub model_id: String,
    /// Human-readable display label (e.g. `"Gemini 4 Neon"`, `"Claude Opus 4.6 (Thinking)"`).
    pub label: String,
    /// Reasoning levels supported for this specific model (`empty` when thinking is built in).
    #[serde(default)]
    pub supported_thinking_levels: Vec<ThinkingLevel>,
    /// Optional default reasoning level for this model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_thinking_level: Option<ThinkingLevel>,
}

/// Compact quota remaining window reported by a runtime when machine-readable quotas exist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeQuotaWindow {
    /// Ultra-short window label (e.g. `"5ч"`, `"нед"`).
    pub label: String,
    /// Remaining capacity percentage clamped to `0..=100`.
    pub remaining_percent: u8,
}

/// Dynamic runtime model, thinking-level, account, and quota catalog (`GET /v1/runtime/catalog`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCatalogResponse {
    /// Active runtime adapter identifier (`"antigravity-acp"` or `"simulated-ci-runtime"`).
    pub adapter_id: String,
    /// Configured authentication profile (`oauth-personal` vs `gemini-api-key`).
    pub auth_profile: RuntimeAuthProfile,
    /// `true` strictly for deterministic CI test doubles; `false` only for real `agy acp`.
    pub is_simulated: bool,
    /// Detected `agy` CLI version when live.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agy_version: Option<String>,
    /// Subscription tier reported by `agy acp` (e.g. `"Google AI Ultra"`), when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    /// Real OS/profile account display label (e.g. host username), when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_label: Option<String>,
    /// Default model identifier selected by the runtime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model_id: Option<String>,
    /// Default reasoning level selected by the runtime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_thinking_level: Option<ThinkingLevel>,
    /// Dynamically probed models currently available to this runtime/account.
    pub models: Vec<RuntimeModelOption>,
    /// Dynamically probed global thinking-level choices.
    pub thinking_options: Vec<RuntimeThinkingOption>,
    /// Optional machine-readable quota windows (empty when the runtime does not publish quotas).
    #[serde(default)]
    pub quotas: Vec<RuntimeQuotaWindow>,
    /// UTC timestamp when this catalog snapshot was probed.
    pub probed_at: TimestampMs,
}

impl fmt::Display for ThinkingLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ThinkingLevel {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            "xhigh" | "x_high" | "extra" => Ok(Self::XHigh),
            other => Err(DennettError::InvalidInput {
                field: String::from("thinking_level"),
                reason: format!(
                    "unsupported thinking level `{other}` (expected low|medium|high|xhigh)"
                ),
            }),
        }
    }
}

/// Workspace file-mutation policy enforced outside the model by Head and runtime gates.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
#[non_exhaustive]
pub enum WorkspaceWritePolicy {
    /// Deny all native file writes and edits inside and outside the workspace.
    DenyAll,
    /// Require explicit external `PolicyEngine` approval via `session/request_permission`
    /// for every native write/edit operation.
    #[default]
    AskBeforeWrite,
    /// Allow writes strictly inside the project workspace boundary (still subject to explicit
    /// `Deny` rules and symlink/traversal boundary enforcement).
    AllowWorkspace,
    /// Allow writes only under explicit workspace-relative path prefixes; all other paths
    /// are denied outside the model.
    AllowPaths {
        /// Allowed relative path prefixes inside the workspace (e.g. `["docs/", "output/"]`).
        allowed_prefixes: Vec<String>,
    },
}

impl WorkspaceWritePolicy {
    /// Validates relative prefixes and security invariants on this write policy.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `AllowPaths` is empty, exceeds
    /// [`MAX_ALLOWED_WRITE_PREFIXES`], or contains absolute paths, NUL bytes, or `..` segments.
    pub fn validate(&self) -> Result<(), DennettError> {
        if let Self::AllowPaths { allowed_prefixes } = self {
            if allowed_prefixes.is_empty() {
                return Err(DennettError::InvalidInput {
                    field: String::from("workspace_write_policy.allowed_prefixes"),
                    reason: String::from("must specify at least one allowed relative path prefix"),
                });
            }
            if allowed_prefixes.len() > MAX_ALLOWED_WRITE_PREFIXES {
                return Err(DennettError::InvalidInput {
                    field: String::from("workspace_write_policy.allowed_prefixes"),
                    reason: format!("exceeds maximum of {MAX_ALLOWED_WRITE_PREFIXES} prefixes"),
                });
            }
            for raw in allowed_prefixes {
                reject_nul(raw, "workspace_write_policy.allowed_prefixes")?;
                let trimmed = raw.trim();
                if trimmed.is_empty() || trimmed.len() > MAX_POLICY_PATTERN_LEN {
                    return Err(DennettError::InvalidInput {
                        field: String::from("workspace_write_policy.allowed_prefixes"),
                        reason: format!(
                            "each prefix must be non-empty and at most {MAX_POLICY_PATTERN_LEN} bytes"
                        ),
                    });
                }
                let normalized = trimmed.replace('\\', "/");
                if normalized.starts_with('/')
                    || normalized.contains(':')
                    || normalized.split('/').any(|seg| seg == ".." || seg == "~")
                {
                    return Err(DennettError::InvalidInput {
                        field: String::from("workspace_write_policy.allowed_prefixes"),
                        reason: format!(
                            "prefix `{trimmed}` must be a workspace-relative path without drive letters or `..`"
                        ),
                    });
                }
            }
        }
        Ok(())
    }
}

/// Immutable versioned configuration snapshot belonging to a single [`AgentRecord`].
///
/// Updating model preference, thinking level, prompt, scopes, or write policy always creates
/// a new [`AgentConfigVersion`] without altering the stable [`AgentId`] (A04 / Architecture 82 §2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentConfigVersion {
    /// Unique identifier for this immutable configuration revision.
    pub config_version_id: AgentConfigVersionId,
    /// Owning Dennett agent identifier.
    pub agent_id: AgentId,
    /// Monotonically increasing revision number (`1, 2, ...`) within this agent.
    pub version_number: i64,
    /// Explicit agent system prompt appended to the runtime (`systemPrompt.append`).
    pub system_prompt: String,
    /// Optional preferred runtime model identifier (e.g. `"gemini-3.8-flash"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_preference: Option<String>,
    /// Optional preferred reasoning effort level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<ThinkingLevel>,
    /// Explicit set of memory scopes this agent is permitted to access.
    pub allowed_memory_scopes: BTreeSet<MemoryScope>,
    /// Workspace write policy enforced outside the model.
    pub workspace_write_policy: WorkspaceWritePolicy,
    /// UTC creation timestamp in epoch milliseconds.
    pub created_at: TimestampMs,
}

/// Stable Dennett agent entity with a pointer to its active [`AgentConfigVersion`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRecord {
    /// Stable agent identity preserved across model and prompt updates.
    pub agent_id: AgentId,
    /// Human-readable agent name.
    pub name: String,
    /// Human-readable description of the agent's role.
    pub description: String,
    /// Active configuration version identifier.
    pub current_config_version_id: AgentConfigVersionId,
    /// Active configuration version payload.
    pub current_config: AgentConfigVersion,
    /// UTC creation timestamp in epoch milliseconds.
    pub created_at: TimestampMs,
    /// UTC timestamp of the latest configuration update.
    pub updated_at: TimestampMs,
}

/// Registered workspace project bound to a canonical [`MemoryScope`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRecord {
    /// Stable project identifier.
    pub project_id: ProjectId,
    /// Human-readable project name.
    pub name: String,
    /// Optional canonical filesystem directory path (`None` when created without a bound folder).
    #[serde(default)]
    pub workspace_path: Option<String>,
    /// Default memory scope associated with this project.
    pub memory_scope: MemoryScope,
    /// UTC creation timestamp in epoch milliseconds.
    pub created_at: TimestampMs,
}

/// Binding between a Dennett session and an external runtime session handle.
///
/// `is_simulated` is always explicitly serialized and checked so deterministic test doubles
/// can NEVER masquerade as a live `agy acp` runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeBinding {
    /// Unique identifier for this runtime binding record.
    pub binding_id: RuntimeBindingId,
    /// Owning Dennett session identifier.
    pub session_id: SessionId,
    /// Runtime adapter identifier (e.g. `"antigravity-acp"` or `"simulated-ci-runtime"`).
    pub adapter_id: String,
    /// Explicitly selected runtime authentication profile (`oauth-personal` vs `gemini-api-key`).
    pub auth_profile: RuntimeAuthProfile,
    /// Subscription tier reported by `agy acp` (e.g. `"Google AI Ultra"`), when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    /// Detected `agy` binary version (e.g. `"1.5.15"`), when bound to real `agy acp`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agy_version: Option<String>,
    /// Opaque provider-side session handle used for explicit `session/load` or `session/resume`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_session_id: Option<String>,
    /// Normalized workspace directory bound to this provider session.
    pub workspace_cwd: String,
    /// `true` strictly for deterministic CI test doubles; `false` only for real `agy acp`.
    pub is_simulated: bool,
    /// Number of explicit Dennett-side context rehydrations performed on this binding.
    pub rehydrate_count: i64,
    /// UTC creation timestamp in epoch milliseconds.
    pub created_at: TimestampMs,
    /// UTC last-updated timestamp in epoch milliseconds.
    pub updated_at: TimestampMs,
}

/// Lifecycle state of a persisted Dennett conversation session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SessionStatus {
    /// Session accepts new user messages and runs.
    Active,
    /// Session is read-only archived history.
    Archived,
    /// Session has been explicitly closed.
    Closed,
}

impl SessionStatus {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
            Self::Closed => "closed",
        }
    }
}

impl fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SessionStatus {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(Self::Active),
            "archived" => Ok(Self::Archived),
            "closed" => Ok(Self::Closed),
            other => Err(DennettError::InvalidInput {
                field: String::from("status"),
                reason: format!("unknown session status `{other}`"),
            }),
        }
    }
}

/// Persisted conversation session tying together an agent, project, and optional [`RuntimeBinding`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRecord {
    /// Stable session identifier.
    pub session_id: SessionId,
    /// Assigned Dennett agent identifier.
    pub agent_id: AgentId,
    /// Bound project workspace identifier.
    pub project_id: ProjectId,
    /// Human-readable session title.
    pub title: String,
    /// Active or last-known runtime binding, when initialized.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_binding: Option<RuntimeBinding>,
    /// Current session lifecycle status.
    pub status: SessionStatus,
    /// UTC creation timestamp in epoch milliseconds.
    pub created_at: TimestampMs,
    /// UTC timestamp of latest session activity.
    pub updated_at: TimestampMs,
}

/// Sender role of a persisted chat message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum MessageRole {
    /// Message authored by the human owner or external caller.
    User,
    /// Visible reply produced by the agent runtime.
    Assistant,
    /// System notice or diagnostic record persisted into the session timeline.
    System,
}

impl MessageRole {
    /// Returns the canonical snake-case role label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::System => "system",
        }
    }
}

impl fmt::Display for MessageRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for MessageRole {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "user" => Ok(Self::User),
            "assistant" => Ok(Self::Assistant),
            "system" => Ok(Self::System),
            other => Err(DennettError::InvalidInput {
                field: String::from("role"),
                reason: format!("unknown message role `{other}`"),
            }),
        }
    }
}

/// One persisted visible conversation message inside a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageRecord {
    /// Stable message identifier.
    pub message_id: MessageId,
    /// Owning session identifier.
    pub session_id: SessionId,
    /// Author role (`user`, `assistant`, or `system`).
    pub role: MessageRole,
    /// Visible text body (never contains hidden chain-of-thought reasoning).
    pub content: String,
    /// Optional execution run that produced or consumed this message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
    /// Correlation identifier linking this message to audit/policy traces.
    pub correlation_id: CorrelationId,
    /// UTC creation timestamp in epoch milliseconds.
    pub created_at: TimestampMs,
}

/// Execution outcome of a single agent turn run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RunStatus {
    /// Run is currently executing against the runtime.
    Running,
    /// Run completed cleanly without protocol or cancellation errors.
    Succeeded,
    /// Run was explicitly cancelled via Stop / `session/cancel`.
    Cancelled,
    /// Run terminated due to runtime error, timeout, nonzero exit, or policy failure.
    Failed,
    /// Run completed its control turn while a downstream dependency (e.g. `memoryd`) was degraded.
    Degraded,
}

impl RunStatus {
    /// Returns the canonical snake-case status label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
            Self::Degraded => "degraded",
        }
    }
}

impl fmt::Display for RunStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for RunStatus {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "cancelled" => Ok(Self::Cancelled),
            "failed" => Ok(Self::Failed),
            "degraded" => Ok(Self::Degraded),
            other => Err(DennettError::InvalidInput {
                field: String::from("run_status"),
                reason: format!("unknown run status `{other}`"),
            }),
        }
    }
}

/// Persisted execution attempt record linking user input, config version, and outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRecord {
    /// Stable run identifier.
    pub run_id: RunId,
    /// Owning session identifier.
    pub session_id: SessionId,
    /// Agent that executed this run.
    pub agent_id: AgentId,
    /// Exact immutable [`AgentConfigVersion`] snapshot active when this run started.
    pub agent_config_version_id: AgentConfigVersionId,
    /// Triggering user message identifier.
    pub user_message_id: MessageId,
    /// Produced assistant message identifier, when a visible reply was recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assistant_message_id: Option<MessageId>,
    /// Current or terminal run status.
    pub status: RunStatus,
    /// Provider `stopReason` when available (e.g. `"end_turn"`, `"cancelled"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    /// Safe machine-readable error code when the run failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    /// Secret-redacted failure summary when the run failed or degraded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_summary: Option<String>,
    /// Whether this turn required explicit Dennett-side context rehydration.
    pub rehydrated: bool,
    /// Correlation identifier linking run, messages, decisions, and audit events.
    pub correlation_id: CorrelationId,
    /// UTC start timestamp in epoch milliseconds.
    pub created_at: TimestampMs,
    /// UTC completion timestamp in epoch milliseconds, once terminal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<TimestampMs>,
}

/// Ordinary D1 persisted artifact record (text/file/code output attached to a session/project).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRecord {
    /// Stable artifact identifier.
    pub artifact_id: ArtifactId,
    /// Owning session identifier.
    pub session_id: SessionId,
    /// Owning project identifier.
    pub project_id: ProjectId,
    /// Optional run identifier that produced this artifact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
    /// Human-readable artifact name (e.g. `"architecture-notes.md"`).
    pub name: String,
    /// MIME media type (e.g. `"text/markdown"`, `"text/plain"`, `"application/json"`).
    pub mime_type: String,
    /// Monotonic artifact revision number (`>= 1`).
    pub revision: i64,
    /// Optional inline UTF-8 content for small artifacts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Optional workspace-relative file path for filesystem-backed artifacts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_path: Option<String>,
    /// Lowercase SHA-256 hex digest of the artifact payload.
    pub sha256: Sha256Hex,
    /// UTC creation timestamp in epoch milliseconds.
    pub created_at: TimestampMs,
}

/// Policy enforcement decision rendered outside the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyEffect {
    /// Explicitly allow the matching tool invocation once.
    Allow,
    /// Explicitly deny the matching tool invocation.
    Deny,
}

impl PolicyEffect {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
        }
    }
}

impl fmt::Display for PolicyEffect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for PolicyEffect {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "allow" => Ok(Self::Allow),
            "deny" => Ok(Self::Deny),
            other => Err(DennettError::InvalidInput {
                field: String::from("effect"),
                reason: format!("unknown policy effect `{other}` (expected allow|deny)"),
            }),
        }
    }
}

/// Persisted authorization rule evaluated by Head's external `PolicyEngine`.
///
/// `Deny` rules always take precedence over `Allow` rules; unknown or unmatched dangerous
/// tools fail closed (`Deny`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyRule {
    /// Stable rule identifier.
    pub rule_id: PolicyRuleId,
    /// Policy scope selector (`"global"`, `"project:<uuid>"`, `"agent:<uuid>"`, or `"session:<uuid>"`).
    pub scope: String,
    /// Exact tool name, prefix wildcard (e.g. `"mcp:*"`), or `"*"`.
    pub tool_name: String,
    /// Optional target pattern (path prefix, command prefix, or `"*"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_pattern: Option<String>,
    /// Whether matching invocations are allowed or denied.
    pub effect: PolicyEffect,
    /// Human-readable rationale recorded in permission audit logs.
    pub reason: String,
    /// UTC creation timestamp in epoch milliseconds.
    pub created_at: TimestampMs,
}

/// Audited record of a single permission evaluation rendered outside the model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionDecisionRecord {
    /// Stable decision identifier.
    pub decision_id: PermissionDecisionId,
    /// Owning session identifier.
    pub session_id: SessionId,
    /// Optional run identifier during which the permission check fired.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
    /// Provider tool call identifier from `session/request_permission`.
    pub tool_call_id: String,
    /// Normalized tool name evaluated by `PolicyEngine`.
    pub tool_name: String,
    /// Optional target path/command/pattern inspected by `PolicyEngine`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_pattern: Option<String>,
    /// Effective scope context evaluated for this decision.
    pub scope: String,
    /// Rendered policy effect (`allow` or `deny`).
    pub effect: PolicyEffect,
    /// Matching rule identifier when a configured [`PolicyRule`] determined the outcome.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_rule_id: Option<PolicyRuleId>,
    /// Secret-free explanation of why the decision was rendered.
    pub reason: String,
    /// Explicitly confirms this decision was rendered outside the model (always `true`).
    pub evaluated_outside_model: bool,
    /// Correlation identifier linking this decision to the triggering request/run.
    pub correlation_id: CorrelationId,
    /// UTC evaluation timestamp in epoch milliseconds.
    pub created_at: TimestampMs,
}

/// Live reachability status of standalone `dennett-memoryd` as observed by Head (V02).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    /// Standalone `dennett-memoryd` responded healthy to recent probes/calls.
    Healthy,
    /// Standalone `dennett-memoryd` is unreachable or unhealthy; Head operates in honest
    /// degraded mode with durable outbox queuing and zero fabricated memory answers.
    DegradedUnreachable,
}

impl MemoryStatus {
    /// Returns the canonical snake-case representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::DegradedUnreachable => "degraded_unreachable",
        }
    }
}

impl fmt::Display for MemoryStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Delivery lifecycle status of a durable [`MemoryOutboxItem`] queued in `dennett_control`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryOutboxStatus {
    /// Queued locally in `dennett_control` awaiting delivery to `dennett-memoryd`.
    Pending,
    /// Confirmed idempotently delivered to standalone `dennett-memoryd`.
    Delivered,
    /// Permanently rejected by `dennett-memoryd` (e.g. forbidden scope or invalid payload).
    Failed,
}

impl MemoryOutboxStatus {
    /// Returns the canonical snake-case representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Delivered => "delivered",
            Self::Failed => "failed",
        }
    }
}

impl fmt::Display for MemoryOutboxStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for MemoryOutboxStatus {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "delivered" => Ok(Self::Delivered),
            "failed" => Ok(Self::Failed),
            other => Err(DennettError::InvalidInput {
                field: String::from("outbox_status"),
                reason: format!("unknown memory outbox status `{other}`"),
            }),
        }
    }
}

/// Durable memory-write intent queued in `dennett_control` for idempotent delivery to `memoryd` (V02).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryOutboxItem {
    /// Stable outbox row identifier.
    pub outbox_id: OutboxItemId,
    /// Optional originating session identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    /// Optional originating run identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
    /// Deterministic idempotency key forwarded verbatim to `dennett-memoryd`.
    pub idempotency_key: IdempotencyKey,
    /// Target memory scope for this write.
    pub scope: MemoryScope,
    /// Operation label (`"create_note"`).
    pub operation: String,
    /// Serialized request payload for deterministic replay.
    pub payload_json: String,
    /// Delivery status (`pending`, `delivered`, or `failed`).
    pub status: MemoryOutboxStatus,
    /// Number of delivery attempts executed so far.
    pub attempts: i32,
    /// Secret-redacted error summary from the latest failed delivery attempt, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    /// Event identifier returned by `dennett-memoryd` upon confirmed delivery.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivered_event_id: Option<MemoryEventId>,
    /// Note identifier returned by `dennett-memoryd` upon confirmed delivery.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivered_note_id: Option<MemoryNoteId>,
    /// Correlation identifier linking this outbox item to its originating operation.
    pub correlation_id: CorrelationId,
    /// UTC creation timestamp in epoch milliseconds.
    pub created_at: TimestampMs,
    /// UTC timestamp of the latest status transition.
    pub updated_at: TimestampMs,
}

/// Secret-free diagnostic/audit event recorded by Head without hidden model reasoning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticEvent {
    /// Stable audit event identifier.
    pub event_id: AuditEventId,
    /// Correlation identifier connecting related actions across Head and `memoryd`.
    pub correlation_id: CorrelationId,
    /// Optional session identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    /// Optional execution run identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
    /// Component that emitted this event (e.g. `"head"`, `"policy_engine"`, `"antigravity-acp"`).
    pub component: String,
    /// Machine-readable event category (e.g. `"run_started"`, `"permission_denied"`, `"session_rehydrated"`).
    pub event_kind: String,
    /// Redacted human-readable diagnostic summary (zero secrets, zero hidden reasoning).
    pub safe_summary: String,
    /// UTC event timestamp in epoch milliseconds.
    pub created_at: TimestampMs,
}

// ============================================================================
// Head HTTP API Request & Response Envelopes
// ============================================================================

/// Request payload for `POST /v1/agents`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateAgentRequest {
    /// Human-readable agent display name.
    pub name: String,
    /// Optional description of the agent's responsibilities.
    #[serde(default)]
    pub description: String,
    /// Explicit system prompt passed via `systemPrompt.append`.
    pub system_prompt: String,
    /// Optional preferred runtime model identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_preference: Option<String>,
    /// Optional preferred reasoning effort level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<ThinkingLevel>,
    /// Memory scopes this agent is permitted to access.
    #[serde(default)]
    pub allowed_memory_scopes: BTreeSet<MemoryScope>,
    /// Workspace write policy enforced outside the model.
    #[serde(default)]
    pub workspace_write_policy: WorkspaceWritePolicy,
}

impl CreateAgentRequest {
    /// Validates agent creation input bounds and policy invariants.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] on empty/oversized fields or invalid policy.
    pub fn validate(&self) -> Result<(), DennettError> {
        validate_non_empty_text(&self.name, "name", MAX_AGENT_NAME_LEN)?;
        validate_optional_text(
            &self.description,
            "description",
            MAX_AGENT_DESCRIPTION_BYTES,
        )?;
        validate_config_fields(
            &self.system_prompt,
            self.model_preference.as_deref(),
            &self.allowed_memory_scopes,
            &self.workspace_write_policy,
        )
    }
}

/// Request payload for `POST /v1/agents/{agent_id}/config-versions`.
///
/// Always appends a new [`AgentConfigVersion`] without creating a new [`AgentId`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateAgentConfigRequest {
    /// Explicit system prompt for the new configuration version.
    pub system_prompt: String,
    /// Optional preferred runtime model identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_preference: Option<String>,
    /// Optional preferred reasoning effort level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<ThinkingLevel>,
    /// Memory scopes permitted under the new configuration version.
    #[serde(default)]
    pub allowed_memory_scopes: BTreeSet<MemoryScope>,
    /// Workspace write policy under the new configuration version.
    #[serde(default)]
    pub workspace_write_policy: WorkspaceWritePolicy,
}

impl UpdateAgentConfigRequest {
    /// Validates configuration version input bounds and policy invariants.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] on invalid fields or write policy.
    pub fn validate(&self) -> Result<(), DennettError> {
        validate_config_fields(
            &self.system_prompt,
            self.model_preference.as_deref(),
            &self.allowed_memory_scopes,
            &self.workspace_write_policy,
        )
    }
}

/// Response payload for `GET /v1/agents`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListAgentsResponse {
    /// All registered agents with their current active configuration versions.
    pub agents: Vec<AgentRecord>,
}

/// Response payload for `GET /v1/agents/{agent_id}/config-versions`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentConfigHistoryResponse {
    /// Agent identifier.
    pub agent_id: AgentId,
    /// Active configuration version identifier.
    pub current_config_version_id: AgentConfigVersionId,
    /// All immutable configuration versions ordered ascending by `version_number`.
    pub versions: Vec<AgentConfigVersion>,
}

/// Request payload for `POST /v1/projects`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateProjectRequest {
    /// Human-readable project name.
    pub name: String,
    /// Optional filesystem path to the project's workspace directory (`None` for folderless start).
    #[serde(default)]
    pub workspace_path: Option<String>,
    /// Optional explicit memory scope; auto-generated as `project:<slug>-<short8>` when omitted.
    #[serde(default)]
    pub memory_scope: Option<MemoryScope>,
}

impl CreateProjectRequest {
    /// Validates project creation fields.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `name` or provided `workspace_path` is empty/oversized/contains NUL.
    pub fn validate(&self) -> Result<(), DennettError> {
        validate_non_empty_text(&self.name, "name", MAX_PROJECT_NAME_LEN)?;
        if let Some(ref path) = self.workspace_path {
            validate_non_empty_text(path, "workspace_path", MAX_WORKSPACE_PATH_LEN)?;
        }
        Ok(())
    }
}

/// Request payload for `PATCH /v1/projects/{project_id}` (inline rename and attach/rebind folder).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateProjectRequest {
    /// Optional updated human-readable project name.
    #[serde(default)]
    pub name: Option<String>,
    /// Optional bound workspace folder path (attach or rebind folder).
    #[serde(default)]
    pub workspace_path: Option<String>,
}

impl UpdateProjectRequest {
    /// Validates project update input fields.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if neither field is set or any provided value is invalid.
    pub fn validate(&self) -> Result<(), DennettError> {
        if self.name.is_none() && self.workspace_path.is_none() {
            return Err(DennettError::InvalidInput {
                field: String::from("body"),
                reason: String::from("must specify at least one of `name` or `workspace_path`"),
            });
        }
        if let Some(ref name) = self.name {
            validate_non_empty_text(name, "name", MAX_PROJECT_NAME_LEN)?;
        }
        if let Some(ref path) = self.workspace_path {
            validate_non_empty_text(path, "workspace_path", MAX_WORKSPACE_PATH_LEN)?;
        }
        Ok(())
    }
}

/// Entry kind returned by `GET /v1/projects/{project_id}/files`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceFileKind {
    /// Regular file inside the project workspace.
    File,
    /// Subdirectory inside the project workspace.
    Directory,
}

/// One real, boundary-verified filesystem entry returned by `GET /v1/projects/{project_id}/files`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceFileEntry {
    /// Workspace-relative POSIX path (`/`-separated).
    pub relative_path: String,
    /// Entry basename.
    pub name: String,
    /// Regular file or directory.
    pub kind: WorkspaceFileKind,
    /// Byte length for regular files (`None` for directories).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    /// Traversal depth relative to the listed root (`1..=MAX_DEPTH`).
    pub depth: u32,
}

/// Response payload for `GET /v1/projects/{project_id}/files`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListProjectFilesResponse {
    /// Project identifier.
    pub project_id: ProjectId,
    /// `true` when the project currently has a bound workspace directory.
    pub workspace_bound: bool,
    /// Bound workspace directory path when `workspace_bound == true`.
    #[serde(default)]
    pub workspace_path: Option<String>,
    /// Bounded, symlink-safe list of real entries inside the workspace.
    pub entries: Vec<WorkspaceFileEntry>,
    /// `true` when bounded entry count or node-visit cap truncated the listing.
    pub truncated: bool,
}

/// Response payload for `GET /v1/projects`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListProjectsResponse {
    /// All registered projects ordered newest first.
    pub projects: Vec<ProjectRecord>,
}

/// Request payload for `POST /v1/sessions`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateSessionRequest {
    /// Existing agent assigned to this session.
    pub agent_id: AgentId,
    /// Existing project workspace bound to this session.
    pub project_id: ProjectId,
    /// Human-readable session title.
    pub title: String,
}

impl CreateSessionRequest {
    /// Validates session creation input.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `title` is empty, oversized, or contains NUL.
    pub fn validate(&self) -> Result<(), DennettError> {
        validate_non_empty_text(&self.title, "title", MAX_SESSION_TITLE_LEN)
    }
}

/// Request payload for `PATCH /v1/sessions/{session_id}` (inline session rename).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateSessionRequest {
    /// Updated human-readable session title.
    pub title: String,
}

impl UpdateSessionRequest {
    /// Validates session rename input.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `title` is empty, oversized, or contains NUL.
    pub fn validate(&self) -> Result<(), DennettError> {
        validate_non_empty_text(&self.title, "title", MAX_SESSION_TITLE_LEN)
    }
}

/// Response payload for `GET /v1/sessions`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListSessionsResponse {
    /// Persisted sessions ordered by latest update descending.
    pub sessions: Vec<SessionRecord>,
}

/// Full session detail payload returned by `GET /v1/sessions/{session_id}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionDetailResponse {
    /// Session metadata and runtime binding.
    pub session: SessionRecord,
    /// Persisted conversation messages ordered chronologically.
    pub messages: Vec<MessageRecord>,
    /// Persisted artifacts attached to this session.
    pub artifacts: Vec<ArtifactRecord>,
    /// Execution runs recorded in this session.
    pub runs: Vec<RunRecord>,
    /// Audited permission decisions evaluated in this session.
    pub permission_decisions: Vec<PermissionDecisionRecord>,
}

/// Request payload for `POST /v1/sessions/{session_id}/messages`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendMessageRequest {
    /// User prompt text to persist and execute through the runtime.
    pub content: String,
    /// Optional explicit turn timeout override in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

impl SendMessageRequest {
    /// Validates message body bounds.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `content` is empty, oversized, or contains NUL.
    pub fn validate(&self) -> Result<(), DennettError> {
        validate_non_empty_text(&self.content, "content", MAX_MESSAGE_CONTENT_BYTES)?;
        if let Some(ms) = self.timeout_ms
            && ms == 0
        {
            return Err(DennettError::InvalidInput {
                field: String::from("timeout_ms"),
                reason: String::from("must be greater than zero when provided"),
            });
        }
        Ok(())
    }
}

/// Response payload for `POST /v1/sessions/{session_id}/messages`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendMessageResponse {
    /// Persisted user message record (saved before runtime invocation).
    pub user_message: MessageRecord,
    /// Persisted assistant reply when the run produced visible output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assistant_message: Option<MessageRecord>,
    /// Persisted run record describing terminal status and correlation.
    pub run: RunRecord,
    /// Updated runtime binding after the turn.
    pub runtime_binding: RuntimeBinding,
    /// Any artifacts created or registered during this turn.
    pub artifacts: Vec<ArtifactRecord>,
    /// Permission decisions rendered outside the model during this turn.
    pub permission_decisions: Vec<PermissionDecisionRecord>,
    /// Observed reachability of standalone `dennett-memoryd`.
    pub memory_status: MemoryStatus,
    /// Effective session title after any first-message auto-titling.
    #[serde(default)]
    pub session_title: String,
}

/// Response payload for `POST /v1/sessions/{session_id}/cancel`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelRunResponse {
    /// Target session identifier.
    pub session_id: SessionId,
    /// Run identifier that was targeted for cancellation, if an active run existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
    /// `true` if an in-flight run received a cancellation signal.
    pub cancelled: bool,
    /// Resulting run status, when a run was active or recently terminal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<RunStatus>,
    /// Current `memoryd` reachability (Stop NEVER blocks on `memoryd`).
    pub memory_status: MemoryStatus,
}

/// Request payload for `POST /v1/sessions/{session_id}/artifacts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateArtifactRequest {
    /// Optional run identifier associated with this artifact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
    /// Human-readable artifact name.
    pub name: String,
    /// MIME media type (e.g. `"text/markdown"`).
    pub mime_type: String,
    /// Optional inline UTF-8 content (at least one of `content` or `relative_path` must be set).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Optional workspace-relative path to the produced file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_path: Option<String>,
}

impl CreateArtifactRequest {
    /// Validates artifact creation fields and path safety.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] on empty/oversized fields, missing payload/path,
    /// or traversal in `relative_path`.
    pub fn validate(&self) -> Result<(), DennettError> {
        validate_non_empty_text(&self.name, "name", MAX_ARTIFACT_NAME_LEN)?;
        validate_non_empty_text(&self.mime_type, "mime_type", MAX_ARTIFACT_MIME_LEN)?;
        if self.content.is_none() && self.relative_path.is_none() {
            return Err(DennettError::InvalidInput {
                field: String::from("content"),
                reason: String::from("either `content` or `relative_path` must be provided"),
            });
        }
        if let Some(ref content) = self.content {
            reject_nul(content, "content")?;
            if content.len() > MAX_ARTIFACT_CONTENT_BYTES {
                return Err(DennettError::InvalidInput {
                    field: String::from("content"),
                    reason: format!("exceeds maximum length of {MAX_ARTIFACT_CONTENT_BYTES} bytes"),
                });
            }
        }
        if let Some(ref rel) = self.relative_path {
            validate_non_empty_text(rel, "relative_path", MAX_WORKSPACE_PATH_LEN)?;
            let normalized = rel.replace('\\', "/");
            if normalized.starts_with('/')
                || normalized.contains(':')
                || normalized.split('/').any(|seg| seg == "..")
            {
                return Err(DennettError::InvalidInput {
                    field: String::from("relative_path"),
                    reason: String::from(
                        "must be a workspace-relative path without drive letters or `..`",
                    ),
                });
            }
        }
        Ok(())
    }
}

/// Response payload for `GET /v1/sessions/{session_id}/artifacts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListArtifactsResponse {
    /// Artifacts registered in the session ordered ascending by creation time.
    pub artifacts: Vec<ArtifactRecord>,
}

/// Request payload for `POST /v1/policies`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatePolicyRuleRequest {
    /// Policy scope selector (`"global"`, `"project:<uuid>"`, `"agent:<uuid>"`, or `"session:<uuid>"`).
    pub scope: String,
    /// Exact tool name, prefix wildcard (e.g. `"mcp:*"`), or `"*"`.
    pub tool_name: String,
    /// Optional target pattern (path prefix, command prefix, or `"*"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_pattern: Option<String>,
    /// Policy effect (`allow` or `deny`).
    pub effect: PolicyEffect,
    /// Human-readable reason for this rule.
    pub reason: String,
}

impl CreatePolicyRuleRequest {
    /// Validates policy rule syntax and length limits.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if scope, tool name, pattern, or reason is invalid.
    pub fn validate(&self) -> Result<(), DennettError> {
        validate_policy_scope(&self.scope)?;
        validate_non_empty_text(&self.tool_name, "tool_name", MAX_POLICY_PATTERN_LEN)?;
        if let Some(ref target) = self.target_pattern {
            validate_non_empty_text(target, "target_pattern", MAX_POLICY_PATTERN_LEN)?;
        }
        validate_non_empty_text(&self.reason, "reason", MAX_REASON_BYTES)?;
        Ok(())
    }
}

/// Response payload for `GET /v1/policies`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListPolicyRulesResponse {
    /// Configured external policy rules ordered by creation time ascending.
    pub rules: Vec<PolicyRule>,
}

/// Request payload for `POST /v1/memory/commits` (durable outbox-backed note write to `memoryd`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitMemoryNoteRequest {
    /// Optional originating session identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    /// Optional originating run identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
    /// Target memory scope.
    pub scope: MemoryScope,
    /// Note title.
    pub title: String,
    /// Note body text.
    pub body: String,
    /// Optional note tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Optional caller-supplied deterministic idempotency key; generated if omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<IdempotencyKey>,
}

/// Response payload for `POST /v1/memory/commits`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitMemoryNoteResponse {
    /// Durable outbox item recorded in `dennett_control`.
    pub outbox_item: MemoryOutboxItem,
    /// `true` if `memoryd` accepted the commit immediately; `false` if queued pending recovery.
    pub delivered_immediately: bool,
    /// Current `memoryd` reachability status.
    pub memory_status: MemoryStatus,
}

/// Response payload for `POST /v1/memory/outbox/flush`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlushOutboxResponse {
    /// Number of pending outbox rows attempted during this flush pass.
    pub attempted: usize,
    /// Number of outbox rows confirmed delivered to `memoryd`.
    pub delivered: usize,
    /// Number of outbox rows still pending after this pass.
    pub pending: usize,
    /// Number of outbox rows permanently failed (non-transient rejection).
    pub failed: usize,
    /// Observed reachability of standalone `dennett-memoryd`.
    pub memory_status: MemoryStatus,
    /// Detailed status of items touched during this flush pass.
    pub items: Vec<MemoryOutboxItem>,
}

/// Response payload for `GET /v1/memory/outbox`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListOutboxResponse {
    /// Observed reachability of standalone `dennett-memoryd`.
    pub memory_status: MemoryStatus,
    /// All durable memory outbox items ordered newest first.
    pub items: Vec<MemoryOutboxItem>,
}

/// Request payload for `POST /v1/memory/search` via Head.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeadMemorySearchRequest {
    /// Optional memory scope filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<MemoryScope>,
    /// Optional text query.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    /// Optional maximum result count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// Honest response payload for `POST /v1/memory/search` via Head (V02).
///
/// When `dennett-memoryd` is unreachable, `memory_status` is [`MemoryStatus::DegradedUnreachable`]
/// and `degraded_reason` explains the outage rather than fabricating confident memory results.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeadMemorySearchResponse {
    /// Whether `memoryd` answered normally or was unreachable.
    pub memory_status: MemoryStatus,
    /// Safe explanation when `memory_status == MemoryStatus::DegradedUnreachable`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degraded_reason: Option<String>,
    /// Matching notes returned by `memoryd` (always empty when degraded).
    pub notes: Vec<MemoryNote>,
}

/// Machine-readable health snapshot exposed by `dennett-head` (`GET /healthz`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeadHealth {
    /// Service kind (always [`ServiceKind::Head`]).
    pub service: ServiceKind,
    /// Current implementation milestone label (`"D1"`).
    pub stage: String,
    /// Active execution profile mode.
    pub profile_mode: ProfileMode,
    /// Whether live PostgreSQL connectivity to `dennett_control` succeeded.
    pub postgres_connected: bool,
    /// Active PostgreSQL role reported by `current_user`, when connected.
    pub postgres_role: Option<String>,
    /// Active PostgreSQL database reported by `current_database()`, when connected.
    pub postgres_database: Option<String>,
    /// Highest applied `control_schema_migrations` version when initialized.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_version: Option<i64>,
    /// Live reachability status of standalone `dennett-memoryd` (V02).
    pub memory_status: MemoryStatus,
    /// Safe diagnostic detail when `memory_status` is degraded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_detail: Option<String>,
    /// Configured runtime adapter identifier (e.g. `"antigravity-acp"` or `"simulated-ci-runtime"`).
    pub runtime_adapter_id: String,
    /// `true` only when Head is wired to the deterministic CI test double.
    pub runtime_is_simulated: bool,
    /// Count of pending rows in `memory_commit_outbox`.
    pub pending_outbox_count: i64,
    /// Explicitly supported Head capabilities for the current milestone.
    pub capabilities: Vec<String>,
    /// Correlation identifier attached to this health check.
    pub correlation_id: CorrelationId,
}

/// Secret-free diagnostic snapshot exposed by `GET /v1/diagnostics`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticsResponse {
    /// Recent audit/diagnostic events ordered newest first.
    pub events: Vec<DiagnosticEvent>,
    /// Recent external policy decisions ordered newest first.
    pub permission_decisions: Vec<PermissionDecisionRecord>,
    /// Recent memory outbox items ordered newest first.
    pub outbox_items: Vec<MemoryOutboxItem>,
}

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

fn validate_optional_text(value: &str, field: &str, max_bytes: usize) -> Result<(), DennettError> {
    reject_nul(value, field)?;
    if value.len() > max_bytes {
        return Err(DennettError::InvalidInput {
            field: field.to_owned(),
            reason: format!("exceeds maximum length of {max_bytes} bytes"),
        });
    }
    Ok(())
}

fn validate_config_fields(
    system_prompt: &str,
    model_preference: Option<&str>,
    allowed_memory_scopes: &BTreeSet<MemoryScope>,
    workspace_write_policy: &WorkspaceWritePolicy,
) -> Result<(), DennettError> {
    validate_non_empty_text(system_prompt, "system_prompt", MAX_SYSTEM_PROMPT_BYTES)?;
    if let Some(model) = model_preference {
        validate_non_empty_text(model, "model_preference", MAX_MODEL_NAME_LEN)?;
        if model.bytes().any(|b| b.is_ascii_control()) {
            return Err(DennettError::InvalidInput {
                field: String::from("model_preference"),
                reason: String::from("must not contain ASCII control characters"),
            });
        }
    }
    if allowed_memory_scopes.len() > MAX_AGENT_MEMORY_SCOPES {
        return Err(DennettError::InvalidInput {
            field: String::from("allowed_memory_scopes"),
            reason: format!("exceeds maximum of {MAX_AGENT_MEMORY_SCOPES} memory scopes"),
        });
    }
    workspace_write_policy.validate()
}

fn validate_policy_scope(scope: &str) -> Result<(), DennettError> {
    validate_non_empty_text(scope, "scope", MAX_POLICY_SCOPE_LEN)?;
    if scope == "global" {
        return Ok(());
    }
    for prefix in ["project:", "agent:", "session:"] {
        if let Some(rest) = scope.strip_prefix(prefix) {
            Uuid::parse_str(rest).map_err(|_| DennettError::InvalidInput {
                field: String::from("scope"),
                reason: format!("scoped policy `{prefix}<uuid>` must carry a valid UUID"),
            })?;
            return Ok(());
        }
    }
    Err(DennettError::InvalidInput {
        field: String::from("scope"),
        reason: String::from(
            "must be `global`, `project:<uuid>`, `agent:<uuid>`, or `session:<uuid>`",
        ),
    })
}

const _: () = {
    const fn assert_send_sync<T: Send + Sync + 'static>() {}
    assert_send_sync::<AgentId>();
    assert_send_sync::<AgentConfigVersionId>();
    assert_send_sync::<ProjectId>();
    assert_send_sync::<SessionId>();
    assert_send_sync::<MessageId>();
    assert_send_sync::<RunId>();
    assert_send_sync::<ArtifactId>();
    assert_send_sync::<RuntimeBindingId>();
    assert_send_sync::<OutboxItemId>();
    assert_send_sync::<PolicyRuleId>();
    assert_send_sync::<PermissionDecisionId>();
    assert_send_sync::<AuditEventId>();
    assert_send_sync::<AgentRecord>();
    assert_send_sync::<ProjectRecord>();
    assert_send_sync::<SessionRecord>();
    assert_send_sync::<MessageRecord>();
    assert_send_sync::<RunRecord>();
    assert_send_sync::<ArtifactRecord>();
    assert_send_sync::<RuntimeBinding>();
    assert_send_sync::<PolicyRule>();
    assert_send_sync::<PermissionDecisionRecord>();
    assert_send_sync::<MemoryOutboxItem>();
    assert_send_sync::<DiagnosticEvent>();
    assert_send_sync::<HeadHealth>();
    assert_send_sync::<RuntimeThinkingOption>();
    assert_send_sync::<RuntimeModelOption>();
    assert_send_sync::<RuntimeQuotaWindow>();
    assert_send_sync::<RuntimeCatalogResponse>();
    assert_send_sync::<UpdateProjectRequest>();
    assert_send_sync::<WorkspaceFileKind>();
    assert_send_sync::<WorkspaceFileEntry>();
    assert_send_sync::<ListProjectFilesResponse>();
    assert_send_sync::<UpdateSessionRequest>();
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_write_policy_rejects_traversal_and_absolute_prefixes() {
        for bad in [
            WorkspaceWritePolicy::AllowPaths {
                allowed_prefixes: vec![],
            },
            WorkspaceWritePolicy::AllowPaths {
                allowed_prefixes: vec![String::from("../escape")],
            },
            WorkspaceWritePolicy::AllowPaths {
                allowed_prefixes: vec![String::from("/etc/passwd")],
            },
            WorkspaceWritePolicy::AllowPaths {
                allowed_prefixes: vec![String::from("C:\\Windows")],
            },
            WorkspaceWritePolicy::AllowPaths {
                allowed_prefixes: vec![String::from("docs/../../outside")],
            },
        ] {
            assert!(
                bad.validate().is_err(),
                "policy must fail validation: {bad:?}"
            );
        }

        let good = WorkspaceWritePolicy::AllowPaths {
            allowed_prefixes: vec![String::from("docs/"), String::from("output/reports")],
        };
        assert!(good.validate().is_ok());
    }

    #[test]
    fn policy_scope_validation_enforces_exact_prefixes_and_uuids() {
        assert!(validate_policy_scope("global").is_ok());
        assert!(validate_policy_scope(&format!("project:{}", ProjectId::new())).is_ok());
        assert!(validate_policy_scope(&format!("agent:{}", AgentId::new())).is_ok());
        assert!(validate_policy_scope(&format!("session:{}", SessionId::new())).is_ok());

        for bad in ["", "project:not-a-uuid", "unknown:123", "global:extra"] {
            assert!(
                validate_policy_scope(bad).is_err(),
                "invalid scope `{bad}` must fail validation"
            );
        }
    }

    #[test]
    fn thinking_level_and_runtime_binding_roundtrip() {
        for (raw, expected) in [
            ("low", ThinkingLevel::Low),
            ("medium", ThinkingLevel::Medium),
            ("high", ThinkingLevel::High),
            ("xhigh", ThinkingLevel::XHigh),
        ] {
            let parsed = ThinkingLevel::from_str(raw).expect("valid thinking level");
            assert_eq!(parsed, expected);
            assert_eq!(parsed.as_str(), raw);
            let encoded = serde_json::to_string(&expected).expect("serialize thinking level");
            assert_eq!(encoded, format!("\"{raw}\""));
            let decoded: ThinkingLevel =
                serde_json::from_str(&encoded).expect("deserialize thinking level");
            assert_eq!(decoded, expected);
        }
        assert_eq!(
            ThinkingLevel::from_str("x_high").expect("accept legacy x_high alias"),
            ThinkingLevel::XHigh
        );
        assert_eq!(
            serde_json::from_str::<ThinkingLevel>("\"x_high\"")
                .expect("deserialize legacy x_high alias"),
            ThinkingLevel::XHigh
        );
        assert!(ThinkingLevel::from_str("turbo").is_err());

        let binding = RuntimeBinding {
            binding_id: RuntimeBindingId::new(),
            session_id: SessionId::new(),
            adapter_id: String::from("antigravity-acp"),
            auth_profile: RuntimeAuthProfile::OAuthPersonal,
            tier: Some(String::from("Google AI Ultra")),
            agy_version: Some(String::from("1.5.15")),
            provider_session_id: Some(String::from("prov-sess-1")),
            workspace_cwd: String::from("C:/tmp/project"),
            is_simulated: false,
            rehydrate_count: 0,
            created_at: TimestampMs::now(),
            updated_at: TimestampMs::now(),
        };
        let encoded = serde_json::to_string(&binding).expect("serialize");
        let decoded: RuntimeBinding = serde_json::from_str(&encoded).expect("deserialize");
        assert_eq!(decoded, binding);
        assert!(!decoded.is_simulated);
    }

    #[test]
    fn create_artifact_request_rejects_missing_payload_and_traversal() {
        let missing = CreateArtifactRequest {
            run_id: None,
            name: String::from("empty.txt"),
            mime_type: String::from("text/plain"),
            content: None,
            relative_path: None,
        };
        assert!(missing.validate().is_err());

        let traversal = CreateArtifactRequest {
            run_id: None,
            name: String::from("evil.txt"),
            mime_type: String::from("text/plain"),
            content: Some(String::from("hi")),
            relative_path: Some(String::from("../outside.txt")),
        };
        assert!(traversal.validate().is_err());

        let ok = CreateArtifactRequest {
            run_id: None,
            name: String::from("report.md"),
            mime_type: String::from("text/markdown"),
            content: Some(String::from("# Report")),
            relative_path: Some(String::from("out/report.md")),
        };
        assert!(ok.validate().is_ok());
    }
}
