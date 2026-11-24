//! Narrow runtime port boundary (`AgentRuntimePort`) and normalized runtime events.
//!
//! Keeps `dennett-head` strictly decoupled from concrete harness wire protocols while
//! preserving native agent loops, external permission evaluation, cancellation, and
//! explicit separation between live runtimes (`is_simulated: false`) and CI test doubles
//! (`is_simulated: true`).

use core::future::Future;
use core::pin::Pin;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use dennett_contracts::{
    DennettError, MessageRecord, PolicyEffect, PolicyRuleId, RunId, RunStatus, RuntimeAuthProfile,
    RuntimeBinding, RuntimeModelOption, RuntimeQuotaWindow, RuntimeThinkingOption, SessionId,
    ThinkingLevel, WorkspaceWritePolicy,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{mpsc, watch};

/// Object-safe boxed async future returned by [`AgentRuntimePort`] methods.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Environment entry forwarded to a scoped MCP server launched by the runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeMcpEnvEntry {
    /// Environment variable name (e.g. `"DENNETT_MEMORY_URL"`, `"DENNETT_MEMORY_TOKEN"`).
    pub name: String,
    /// Environment variable value (redacted whenever diagnostics are emitted).
    pub value: String,
}

/// Stdio MCP server specification injected into ACP `session/new` / `session/load`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeMcpServerSpec {
    /// Logical MCP server identifier (e.g. `"dennett-memory"`).
    pub name: String,
    /// Executable command path.
    pub command: String,
    /// Positional command-line arguments.
    pub args: Vec<String>,
    /// Explicitly scoped environment variables (never PostgreSQL or Head DB credentials).
    pub env: Vec<RuntimeMcpEnvEntry>,
}

/// Active A08 skill payload materialized strictly inside an isolated per-session runtime profile
/// (never modifying the user's personal `~/.gemini/` directory).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveSkillPayload {
    /// Normalized capability slug (used to derive `<gemini_dir>/config/skills/<dir>`).
    pub slug: String,
    /// Capability version string.
    pub version: String,
    /// Lowercase SHA-256 hex digest of the canonical skill content.
    pub content_sha256: String,
    /// Full UTF-8 `SKILL.md` markdown document.
    pub skill_markdown: String,
}

/// Runtime descriptor returned by [`AgentRuntimePort::probe`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeDescriptor {
    /// Stable adapter identifier (`"antigravity-acp"` or `"simulated-ci-runtime"`).
    pub adapter_id: String,
    /// Explicitly configured authentication profile.
    pub auth_profile: RuntimeAuthProfile,
    /// `true` strictly for deterministic CI test doubles; `false` only for real `agy acp`.
    pub is_simulated: bool,
    /// Detected `agy` binary semantic version, when running against live `agy acp`.
    pub agy_version: Option<String>,
    /// Reported subscription tier (e.g. `"Google AI Ultra"`), when available.
    pub tier: Option<String>,
    /// Negotiated ACP protocol version, when available.
    pub protocol_version: Option<u64>,
    /// Whether the runtime supports explicit `session/load` / `session/resume`.
    pub supports_resume: bool,
    /// Whether the runtime supports `_meta.antigravity.systemPrompt.append`.
    pub supports_system_prompt_append: bool,
    /// Models advertised by the runtime (flat identifiers).
    pub available_models: Vec<String>,
    /// Structured model families with per-model supported thinking levels.
    pub models: Vec<RuntimeModelOption>,
    /// Global reasoning-effort choices advertised by the runtime.
    pub thinking_options: Vec<RuntimeThinkingOption>,
    /// Default model identifier selected by the runtime.
    pub default_model_id: Option<String>,
    /// Default reasoning level selected by the runtime.
    pub default_thinking_level: Option<ThinkingLevel>,
    /// Machine-readable quota windows reported by the runtime (empty when unavailable).
    pub quotas: Vec<RuntimeQuotaWindow>,
    /// Additional technical capability labels.
    pub capabilities: Vec<String>,
}

/// Normalized permission evaluation request passed to Dennett's external [`PolicyEvaluator`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyEvaluationRequest {
    /// Owning Dennett session identifier.
    pub session_id: SessionId,
    /// Active execution run identifier.
    pub run_id: RunId,
    /// Provider tool call identifier from `session/request_permission`.
    pub tool_call_id: String,
    /// Normalized tool name (e.g. `"write_file"`, `"run_command"`, `"memory_search"`).
    pub tool_name: String,
    /// Provider tool kind (e.g. `"edit"`, `"execute"`, `"read"`, `"other"`).
    pub tool_kind: String,
    /// Human-readable provider tool title.
    pub title: String,
    /// Optional target path, shell command, or scope extracted from tool arguments.
    pub target: Option<String>,
    /// All candidate filesystem locations/targets extracted from `toolCall.locations[*]` and
    /// path-valued fields of `rawInput`/`args` (deduplicated, preserving order).
    pub locations: Vec<String>,
    /// Raw JSON tool arguments for deeper inspection by the policy engine.
    pub raw_input: Value,
}

/// External policy decision returned to the runtime adapter (evaluated outside the model).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyEvaluationOutcome {
    /// Effective decision (`Allow` maps strictly to `allow_once`; `Deny` maps to `reject_once`).
    pub effect: PolicyEffect,
    /// Matched policy rule identifier, if an explicit rule determined the outcome.
    pub matched_rule_id: Option<PolicyRuleId>,
    /// Human-readable, secret-free rationale recorded in audit logs.
    pub reason: String,
    /// Effective policy scope context evaluated for this check.
    pub scope: String,
}

/// External reference monitor invoked by the runtime on every `session/request_permission`.
///
/// Async (`BoxFuture`) so `WorkspaceWritePolicy::AskBeforeWrite` can persist an interactive
/// permission request + Action Inbox card and await owner approval or cancellation strictly
/// outside the model.
pub trait PolicyEvaluator: Send + Sync {
    /// Evaluates a pending tool call strictly outside the model.
    fn evaluate<'a>(
        &'a self,
        req: &'a PolicyEvaluationRequest,
    ) -> BoxFuture<'a, PolicyEvaluationOutcome>;
}

/// Normalized runtime event emitted during session setup and prompt execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum RuntimeEvent {
    /// Incremental visible assistant text chunk (never hidden reasoning).
    TextDelta {
        /// Visible text fragment.
        text: String,
    },
    /// Observation of a native or MCP tool invocation (never re-executed by Dennett).
    ToolObserved {
        /// Provider tool call identifier.
        tool_call_id: String,
        /// Normalized tool name when available.
        tool_name: String,
        /// Provider display title.
        title: String,
        /// Observed lifecycle status (`pending`, `in_progress`, `completed`, `failed`).
        status: String,
        /// Secret-redacted output excerpt.
        output_excerpt: String,
    },
    /// Runtime requested external permission for a pending tool call.
    PermissionRequested {
        /// Provider tool call identifier.
        tool_call_id: String,
        /// Normalized tool name.
        tool_name: String,
        /// Optional extracted target path or command.
        target: Option<String>,
        /// Human-readable tool title.
        title: String,
    },
    /// External policy decision returned to the runtime for a permission request.
    PermissionResolved {
        /// Provider tool call identifier.
        tool_call_id: String,
        /// Normalized tool name.
        tool_name: String,
        /// Optional extracted target path or command.
        target: Option<String>,
        /// Rendered policy effect (`allow` or `deny`).
        effect: PolicyEffect,
        /// Matching rule ID when an explicit rule fired.
        matched_rule_id: Option<PolicyRuleId>,
        /// Secret-free rationale.
        reason: String,
        /// Effective policy scope.
        scope: String,
        /// ACP option ID echoed back (`None` if cancelled/abandoned).
        chosen_option_id: Option<String>,
    },
    /// Usage metadata reported by the runtime upon turn completion.
    UsageReported {
        /// Raw JSON usage summary.
        usage: Value,
    },
    /// Explicit Dennett-side context rehydration performed when native resume was unavailable.
    Rehydrated {
        /// Safe reason explaining why fresh rehydration was required.
        reason: String,
        /// Number of persisted visible messages injected into the rehydrated context.
        history_messages: usize,
    },
    /// Agent requested an explicit clarification from the owner before continuing (`waiting_clarification`).
    ClarificationRequested {
        /// Plain-text clarifying question presented to the user.
        question: String,
    },
    /// Runtime emitted a partial intermediate artifact checkpoint (`completeness_status = partial`).
    PartialArtifactEmitted {
        /// Logical artifact name.
        name: String,
        /// Partial UTF-8 artifact body.
        content: String,
    },
    /// Runtime requested execution of an external effect through Head's `ExternalEffectPort`.
    ExternalEffectRequested {
        /// Target recipient or endpoint.
        recipient: String,
        /// Logical operation label.
        operation: String,
        /// Deterministic simulated connector behavior (`success`, `denied`, `failed_before_accept`, `lost_ack`).
        behavior: String,
        /// Deterministic idempotency key for this effect.
        idempotency_key: String,
    },
    /// Turn completed cleanly with a provider `stopReason`.
    Completed {
        /// Provider stop reason (e.g. `"end_turn"`).
        stop_reason: String,
    },
    /// Turn was cancelled via `session/cancel` or Stop control.
    Cancelled {
        /// Safe cancellation summary.
        reason: String,
    },
    /// Turn encountered a protocol, auth, exit, or timeout error.
    Error {
        /// Optional JSON-RPC error code.
        code: Option<i64>,
        /// Redacted error detail.
        detail: String,
    },
}

/// Parameters for opening or explicitly resuming a runtime session.
#[derive(Debug, Clone)]
pub struct SessionStartRequest {
    /// Owning Dennett session identifier.
    pub dennett_session_id: SessionId,
    /// Project workspace directory bound to this session.
    pub workspace_cwd: PathBuf,
    /// Agent's explicit system prompt (`systemPrompt.append`).
    pub system_prompt: String,
    /// Optional preferred model identifier (`session/set_config_option` `model`).
    pub model_preference: Option<String>,
    /// Optional preferred thinking level (`session/set_config_option` `thought_level`).
    pub thinking_level: Option<ThinkingLevel>,
    /// Workspace write policy enforced via runtime hooks/settings and external policy engine.
    pub workspace_write_policy: WorkspaceWritePolicy,
    /// Scoped MCP servers (e.g. `dennett-memory-mcp`) injected into the runtime session.
    pub mcp_servers: Vec<RuntimeMcpServerSpec>,
    /// Active A08 skills materialized strictly inside the isolated runtime profile or prompt.
    pub active_skills: Vec<ActiveSkillPayload>,
    /// Previously persisted runtime binding, when attempting explicit resume (`session/load`).
    pub existing_binding: Option<RuntimeBinding>,
    /// Persisted Dennett conversation history used ONLY if explicit rehydrate is needed.
    pub history_for_rehydrate: Vec<MessageRecord>,
    /// When `false`, missing/corrupt/cwd-mismatched sessions return an explicit error
    /// instead of silently starting fresh.
    pub allow_rehydrate: bool,
}

/// Result of starting or explicitly resuming a runtime session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionStartOutcome {
    /// Adapter identifier (`"antigravity-acp"`, `"local-http-runtime"`, or `"simulated-ci-runtime"`).
    pub adapter_id: String,
    /// Effective authentication profile.
    pub auth_profile: RuntimeAuthProfile,
    /// Reported subscription tier, if available.
    pub tier: Option<String>,
    /// Detected `agy` CLI version, if live.
    pub agy_version: Option<String>,
    /// Assigned or resumed provider session identifier.
    pub provider_session_id: String,
    /// Normalized workspace directory bound to the provider session.
    pub workspace_cwd: String,
    /// `true` strictly for deterministic CI test doubles; `false` for live runtimes.
    pub is_simulated: bool,
    /// `true` if Dennett explicitly rehydrated context into a fresh provider session.
    pub rehydrated: bool,
    /// Safe explanation when `rehydrated == true`.
    pub rehydrate_reason: Option<String>,
    /// Available model slugs reported by the runtime.
    pub available_models: Vec<String>,
    /// Optional normalized endpoint fingerprint (e.g. `http://127.0.0.1:11434`) for V29 reuse checks.
    pub endpoint_fingerprint: Option<String>,
}

/// Parameters for executing a single prompt turn on an active runtime session.
#[derive(Debug, Clone)]
pub struct TurnRequest {
    /// Owning Dennett session identifier.
    pub dennett_session_id: SessionId,
    /// Provider session identifier returned by [`AgentRuntimePort::start_or_resume_session`].
    pub provider_session_id: String,
    /// Concrete execution run identifier.
    pub run_id: RunId,
    /// User message text sent to `session/prompt`.
    pub prompt_text: String,
    /// Wall-clock timeout enforced on the turn.
    pub timeout: Duration,
    /// Live cancellation signal (`true` triggers immediate `session/cancel` and rejects late approvals).
    pub cancel_rx: watch::Receiver<bool>,
}

/// Aggregated result of a completed or cancelled prompt turn.
///
/// Protocol errors, nonzero child exits, EOFs, and timeouts return `Err(DennettError)` rather
/// than a [`TurnOutcome`] so partial output can NEVER masquerade as [`RunStatus::Succeeded`].
#[derive(Debug, Clone)]
pub struct TurnOutcome {
    /// Terminal status ([`RunStatus::Succeeded`] or [`RunStatus::Cancelled`]).
    pub status: RunStatus,
    /// Provider `stopReason` (e.g. `"end_turn"`, `"cancelled"`).
    pub stop_reason: String,
    /// Concatenated visible assistant text.
    pub assistant_text: String,
    /// Normalized observable events captured during the turn.
    pub events: Vec<RuntimeEvent>,
    /// Optional provider usage payload.
    pub usage: Option<Value>,
    /// Cumulative milliseconds spent waiting on interactive owner approvals during this turn
    /// (excluded from task execution `wall_ms_used` budget accounting; D2-BUG-03).
    pub human_wait_ms: u64,
}

impl TurnOutcome {
    /// Extracts total token count from provider `usage` metadata when available.
    #[must_use]
    pub fn total_tokens(&self) -> Option<u64> {
        let usage = self.usage.as_ref()?;
        if let Some(total) = usage
            .get("totalTokens")
            .or_else(|| usage.get("total_tokens"))
            .and_then(Value::as_u64)
        {
            return Some(total);
        }
        let input = usage
            .get("inputTokens")
            .or_else(|| usage.get("input_tokens"))
            .and_then(Value::as_u64);
        let output = usage
            .get("outputTokens")
            .or_else(|| usage.get("output_tokens"))
            .and_then(Value::as_u64);
        match (input, output) {
            (Some(i), Some(o)) => Some(i.saturating_add(o)),
            (Some(i), None) => Some(i),
            (None, Some(o)) => Some(o),
            (None, None) => None,
        }
    }

    /// Returns the clarifying question if the runtime requested user clarification during this turn.
    #[must_use]
    pub fn clarification_question(&self) -> Option<&str> {
        self.events.iter().find_map(|ev| match ev {
            RuntimeEvent::ClarificationRequested { question } => Some(question.as_str()),
            _ => None,
        })
    }
}

/// Narrow port implemented by concrete agent runtimes (live `agy acp` and CI test doubles).
pub trait AgentRuntimePort: Send + Sync {
    /// Stable adapter identifier (e.g. `"antigravity-acp"` or `"simulated-ci-runtime"`).
    fn adapter_id(&self) -> &str;

    /// Returns `true` strictly for test doubles; `false` for production `agy acp`.
    fn is_simulated(&self) -> bool;

    /// Returns the explicitly configured authentication profile.
    fn auth_profile(&self) -> RuntimeAuthProfile;

    /// Probes binary availability, version, and advertised capabilities.
    fn probe(&self) -> BoxFuture<'_, Result<RuntimeDescriptor, DennettError>>;

    /// Starts a new runtime session or explicitly resumes an existing binding by exact ID.
    fn start_or_resume_session(
        &self,
        req: SessionStartRequest,
    ) -> BoxFuture<'_, Result<SessionStartOutcome, DennettError>>;

    /// Executes one prompt turn with external policy callbacks, optional event sink, and live cancel.
    fn send_prompt_turn(
        &self,
        req: TurnRequest,
        policy: Arc<dyn PolicyEvaluator>,
        event_tx: Option<mpsc::Sender<RuntimeEvent>>,
    ) -> BoxFuture<'_, Result<TurnOutcome, DennettError>>;

    /// Triggers live cancellation for an in-flight turn on `dennett_session_id`.
    fn cancel_turn(
        &self,
        dennett_session_id: SessionId,
    ) -> BoxFuture<'_, Result<bool, DennettError>>;

    /// Cleanly closes any live runtime process/session associated with `dennett_session_id`.
    fn close_session(
        &self,
        dennett_session_id: SessionId,
    ) -> BoxFuture<'_, Result<(), DennettError>>;
}

/// Classifies upstream provider quota / rate-limit errors into [`DennettError::ProviderQuotaExceeded`],
/// keeping them strictly distinct from Dennett's own external `TaskBudget` governance caps (A09).
#[must_use]
pub fn classify_provider_quota_error(
    provider: &str,
    code: Option<i64>,
    detail: &str,
    retry_after_secs: Option<u64>,
) -> Option<DennettError> {
    let lower = detail.to_ascii_lowercase();
    let is_quota_code = matches!(code, Some(429 | -32029));
    let is_quota_message = lower.contains("resource_exhausted")
        || lower.contains("quota exceeded")
        || lower.contains("quota exhausted")
        || lower.contains("exceeded your current quota")
        || lower.contains("insufficient_quota")
        || lower.contains("rate limit")
        || lower.contains("rate_limit")
        || lower.contains("too many requests");

    if !is_quota_code && !is_quota_message {
        return None;
    }

    let parsed_retry = retry_after_secs.or_else(|| parse_retry_after_hint(&lower));
    Some(DennettError::ProviderQuotaExceeded {
        provider: provider.to_owned(),
        detail: detail.to_owned(),
        retry_after_secs: parsed_retry,
    })
}

fn parse_retry_after_hint(lower: &str) -> Option<u64> {
    for needle in ["retry-after", "retry_after", "retry after"] {
        if let Some(pos) = lower.find(needle) {
            let tail = &lower[pos + needle.len()..];
            let digits: String = tail
                .chars()
                .skip_while(|c| !c.is_ascii_digit())
                .take_while(char::is_ascii_digit)
                .collect();
            if let Ok(secs) = digits.parse::<u64>() {
                return Some(secs);
            }
        }
    }
    None
}

/// Evaluates V29 resume compatibility between a persisted [`RuntimeBinding`] and the target runtime.
///
/// Returns `Ok(())` only when the binding matches the target simulation mode, `adapter_id`,
/// `auth_profile`, normalized `workspace_cwd`, compatible major `agy` CLI version (when applicable),
/// and `endpoint_fingerprint` (when applicable).
///
/// # Errors
///
/// Returns a safe mismatch explanation whenever native `session/load` MUST NOT be attempted so
/// callers either perform honest rehydration (`allow_rehydrate = true`) or return `Conflict`
/// (`allow_rehydrate = false`).
pub fn evaluate_binding_resume_compatibility(
    binding: &RuntimeBinding,
    target_adapter_id: &str,
    target_is_simulated: bool,
    target_auth_profile: RuntimeAuthProfile,
    target_cwd: &str,
    target_agy_version: Option<crate::profile::Semver>,
    target_endpoint_fingerprint: Option<&str>,
) -> Result<(), String> {
    if binding.is_simulated != target_is_simulated {
        return Err(format!(
            "existing runtime binding simulation mode mismatch (bound is_simulated={}, target is_simulated={target_is_simulated}); refusing silent cross-runtime resume",
            binding.is_simulated
        ));
    }
    if binding.adapter_id != target_adapter_id {
        return Err(format!(
            "runtime adapter mismatch (bound `{}`, requested `{target_adapter_id}`); refusing silent cross-adapter resume",
            binding.adapter_id
        ));
    }
    if binding.auth_profile != target_auth_profile {
        return Err(format!(
            "runtime auth profile mismatch (bound `{}`, requested `{target_auth_profile}`); refusing silent cross-profile resume",
            binding.auth_profile
        ));
    }
    if !crate::profile::workspace_paths_match(&binding.workspace_cwd, target_cwd) {
        return Err(format!(
            "workspace cwd mismatch (bound `{}`, requested `{target_cwd}`)",
            binding.workspace_cwd
        ));
    }
    if let (Some(bound_raw), Some(running_ver)) =
        (binding.agy_version.as_deref(), target_agy_version)
    {
        match crate::profile::parse_agy_version(bound_raw) {
            Ok(bound_ver) if bound_ver.major == running_ver.major => {}
            Ok(bound_ver) => {
                return Err(format!(
                    "agy major version mismatch (bound `{bound_ver}`, running `{running_ver}`); refusing stale session/load"
                ));
            }
            Err(_) => {
                return Err(format!(
                    "unparseable bound agy version `{bound_raw}` against running `{running_ver}`; refusing stale session/load"
                ));
            }
        }
    }
    if binding.endpoint_fingerprint.as_deref() != target_endpoint_fingerprint
        && (binding.endpoint_fingerprint.is_some() || target_endpoint_fingerprint.is_some())
    {
        return Err(format!(
            "runtime endpoint fingerprint mismatch (bound `{}`, requested `{}`)",
            binding.endpoint_fingerprint.as_deref().unwrap_or("none"),
            target_endpoint_fingerprint.unwrap_or("none"),
        ));
    }
    Ok(())
}

const _: () = {
    const fn assert_send_sync<T: Send + Sync + 'static>() {}
    assert_send_sync::<ActiveSkillPayload>();
    assert_send_sync::<RuntimeDescriptor>();
    assert_send_sync::<RuntimeEvent>();
    assert_send_sync::<SessionStartOutcome>();
    assert_send_sync::<TurnOutcome>();
};
