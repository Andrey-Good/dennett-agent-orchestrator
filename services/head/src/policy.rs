//! External [`PolicyEngine`] enforced strictly outside the model (V05 / A05).
//!
//! Guarantees:
//! - `evaluated_outside_model` is unconditionally `true`.
//! - Workspace path boundary checks reject `..` traversal, drive escapes, and symlink/junction
//!   escapes pointing outside the project's canonical workspace root.
//! - Explicit `Deny` rules ALWAYS override `Allow` rules.
//! - Dangerous or unknown tools (`run_command`, shell, network, unknown) fail closed (`Deny`)
//!   unless matched by an explicit `Allow` rule in scope.
//! - MCP memory tool invocations additionally verify target `scope` against the agent's
//!   configured `allowed_memory_scopes` (defense-in-depth alongside `dennett-memoryd`'s bearer auth).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use dennett_contracts::redact::redact_secrets;
use dennett_contracts::{
    AgentId, CorrelationId, MemoryScope, PermissionDecisionId, PermissionDecisionRecord,
    PolicyEffect, PolicyRule, ProjectId, SessionId, TimestampMs, WorkspaceWritePolicy,
};
use dennett_runtime_antigravity::{
    PolicyEvaluationOutcome, PolicyEvaluationRequest, PolicyEvaluator,
};

/// Context snapshot evaluated by [`PolicyEngine`] for one session turn.
#[derive(Debug, Clone)]
pub struct PolicyContext {
    /// Owning project identifier.
    pub project_id: ProjectId,
    /// Owning agent identifier.
    pub agent_id: AgentId,
    /// Owning session identifier.
    pub session_id: SessionId,
    /// Project workspace root directory.
    pub workspace_root: PathBuf,
    /// Agent's configured workspace write policy.
    pub workspace_write_policy: WorkspaceWritePolicy,
    /// Agent's allowed memory scopes.
    pub allowed_memory_scopes: BTreeSet<MemoryScope>,
    /// Active persisted policy rules loaded from `dennett_control`.
    pub rules: Vec<PolicyRule>,
}

/// Pure, deterministic external policy engine (reference monitor evaluated outside the model).
#[derive(Debug, Clone)]
pub struct PolicyEngine {
    ctx: PolicyContext,
}

impl PolicyEngine {
    /// Creates a new [`PolicyEngine`] bound to `ctx`.
    #[must_use]
    pub const fn new(ctx: PolicyContext) -> Self {
        Self { ctx }
    }

    /// Evaluates one pending tool invocation request outside the model.
    #[must_use]
    #[expect(
        clippy::too_many_lines,
        reason = "policy evaluation enforces strict 7-stage precedence order in one auditable sequence"
    )]
    pub fn evaluate_request(&self, req: &PolicyEvaluationRequest) -> PolicyEvaluationOutcome {
        let tool_lower = req.tool_name.trim().to_ascii_lowercase();
        let active_scopes = self.active_scopes();
        let applicable_rules: Vec<&PolicyRule> = self
            .ctx
            .rules
            .iter()
            .filter(|rule| active_scopes.iter().any(|s| s == &rule.scope))
            .collect();

        let is_execute = is_execute_tool(&tool_lower, &req.tool_kind, &req.raw_input);
        let is_file_write = !is_execute && is_write_tool(&tool_lower, &req.tool_kind);
        let is_file_read =
            !is_execute && !is_file_write && is_read_tool(&tool_lower, &req.tool_kind);
        let memory_mcp_op = if is_execute || is_file_write || is_file_read {
            None
        } else {
            canonical_memory_mcp_op(&tool_lower, &req.tool_kind, &req.raw_input)
        };

        // 1. Hard workspace boundary check for BOTH file-mutating AND file-reading tools
        //    (CANNOT be bypassed by Allow rules; fails CLOSED on missing target or any escaping location!).
        let mut normalized_rel_targets: Vec<String> = Vec::new();
        if is_file_write || is_file_read {
            let candidate_paths = collect_file_targets(req);
            if candidate_paths.is_empty() {
                let kind_label = if is_file_write { "write" } else { "read" };
                return PolicyEvaluationOutcome {
                    effect: PolicyEffect::Deny,
                    matched_rule_id: None,
                    reason: format!(
                        "denied {kind_label} tool `{}`: missing target path required for workspace boundary verification",
                        req.tool_name
                    ),
                    scope: format!("session:{}", self.ctx.session_id),
                };
            }

            for raw_target in &candidate_paths {
                match resolve_within_workspace(&self.ctx.workspace_root, raw_target) {
                    Ok(rel) => {
                        if !normalized_rel_targets
                            .iter()
                            .any(|existing| existing == &rel)
                        {
                            normalized_rel_targets.push(rel);
                        }
                    }
                    Err(reason) => {
                        return PolicyEvaluationOutcome {
                            effect: PolicyEffect::Deny,
                            matched_rule_id: None,
                            reason,
                            scope: format!("project:{}", self.ctx.project_id),
                        };
                    }
                }
            }
        }

        // Build full target match list (normalized workspace-relative paths FIRST, plus raw target)
        // so explicit `Deny` rules like `secrets/*` match absolute paths (`C:/ws/secrets/key.pem`)
        // AND secondary `locations[*]` entries!
        let mut rule_match_targets: Vec<&str> =
            normalized_rel_targets.iter().map(String::as_str).collect();
        if let Some(raw) = req
            .target
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            && !rule_match_targets.contains(&raw)
        {
            rule_match_targets.push(raw);
        }

        // 2. Explicit `Deny` rules ALWAYS take precedence over `Allow` rules.
        if let Some(deny_rule) = applicable_rules.iter().find(|r| {
            r.effect == PolicyEffect::Deny && rule_matches_any(r, &tool_lower, &rule_match_targets)
        }) {
            return PolicyEvaluationOutcome {
                effect: PolicyEffect::Deny,
                matched_rule_id: Some(deny_rule.rule_id),
                reason: format!("denied by explicit policy rule: {}", deny_rule.reason),
                scope: deny_rule.scope.clone(),
            };
        }

        // 3. Enforce agent `WorkspaceWritePolicy` on file-mutating tools.
        if is_file_write {
            match &self.ctx.workspace_write_policy {
                WorkspaceWritePolicy::DenyAll => {
                    return PolicyEvaluationOutcome {
                        effect: PolicyEffect::Deny,
                        matched_rule_id: None,
                        reason: String::from("denied by agent workspace_write_policy=deny_all"),
                        scope: format!("agent:{}", self.ctx.agent_id),
                    };
                }
                WorkspaceWritePolicy::AllowPaths { allowed_prefixes } => {
                    if let Some(disallowed) = normalized_rel_targets
                        .iter()
                        .find(|rel| !matches_any_prefix(rel, allowed_prefixes))
                    {
                        return PolicyEvaluationOutcome {
                            effect: PolicyEffect::Deny,
                            matched_rule_id: None,
                            reason: format!(
                                "denied by workspace_write_policy=allow_paths: `{disallowed}` is outside permitted prefixes"
                            ),
                            scope: format!("agent:{}", self.ctx.agent_id),
                        };
                    }
                    let joined_rel = normalized_rel_targets.join(", ");
                    return PolicyEvaluationOutcome {
                        effect: PolicyEffect::Allow,
                        matched_rule_id: None,
                        reason: format!(
                            "allowed by workspace_write_policy=allow_paths for `{joined_rel}`"
                        ),
                        scope: format!("agent:{}", self.ctx.agent_id),
                    };
                }
                WorkspaceWritePolicy::AllowWorkspace => {
                    let joined_rel = normalized_rel_targets.join(", ");
                    return PolicyEvaluationOutcome {
                        effect: PolicyEffect::Allow,
                        matched_rule_id: None,
                        reason: format!(
                            "allowed by workspace_write_policy=allow_workspace within project root (`{joined_rel}`)"
                        ),
                        scope: format!("agent:{}", self.ctx.agent_id),
                    };
                }
                WorkspaceWritePolicy::AskBeforeWrite => {
                    // Requires an explicit Allow rule below; otherwise fails closed!
                }
                _ => {
                    return PolicyEvaluationOutcome {
                        effect: PolicyEffect::Deny,
                        matched_rule_id: None,
                        reason: String::from("denied: unrecognized workspace_write_policy"),
                        scope: format!("agent:{}", self.ctx.agent_id),
                    };
                }
            }
        }

        // 4. Scoped `dennett-memory-mcp` tools: enforce agent's `allowed_memory_scopes` (defense-in-depth).
        if let Some(mem_op) = memory_mcp_op {
            if self.ctx.allowed_memory_scopes.is_empty() {
                return PolicyEvaluationOutcome {
                    effect: PolicyEffect::Deny,
                    matched_rule_id: None,
                    reason: String::from(
                        "denied memory MCP tool: agent has no allowed_memory_scopes configured",
                    ),
                    scope: format!("agent:{}", self.ctx.agent_id),
                };
            }

            let requires_explicit_scope = matches!(
                mem_op,
                "memory_write_note" | "memory_correct_note" | "mcp:dennett-memory"
            );

            match req.raw_input.get("scope") {
                Some(scope_val) => {
                    let Some(requested_scope) =
                        scope_val.as_str().map(str::trim).filter(|s| !s.is_empty())
                    else {
                        return PolicyEvaluationOutcome {
                            effect: PolicyEffect::Deny,
                            matched_rule_id: None,
                            reason: format!(
                                "denied memory MCP tool `{}`: `scope` must be a non-empty string",
                                req.tool_name
                            ),
                            scope: format!("agent:{}", self.ctx.agent_id),
                        };
                    };
                    let allowed = self
                        .ctx
                        .allowed_memory_scopes
                        .iter()
                        .any(|s| s.as_str() == requested_scope);
                    if !allowed {
                        return PolicyEvaluationOutcome {
                            effect: PolicyEffect::Deny,
                            matched_rule_id: None,
                            reason: format!(
                                "denied memory MCP tool `{}`: scope `{requested_scope}` is not in agent allowed_memory_scopes",
                                req.tool_name
                            ),
                            scope: format!("agent:{}", self.ctx.agent_id),
                        };
                    }
                }
                None if requires_explicit_scope => {
                    return PolicyEvaluationOutcome {
                        effect: PolicyEffect::Deny,
                        matched_rule_id: None,
                        reason: format!(
                            "denied memory MCP tool `{}`: mandatory `scope` argument is missing",
                            req.tool_name
                        ),
                        scope: format!("agent:{}", self.ctx.agent_id),
                    };
                }
                None => {}
            }

            return PolicyEvaluationOutcome {
                effect: PolicyEffect::Allow,
                matched_rule_id: None,
                reason: format!("allowed scoped Dennett memory MCP tool `{}`", req.tool_name),
                scope: format!("agent:{}", self.ctx.agent_id),
            };
        }

        // 5. Explicit `Allow` rules matching active scopes.
        if let Some(allow_rule) = applicable_rules.iter().find(|r| {
            r.effect == PolicyEffect::Allow && rule_matches_any(r, &tool_lower, &rule_match_targets)
        }) {
            return PolicyEvaluationOutcome {
                effect: PolicyEffect::Allow,
                matched_rule_id: Some(allow_rule.rule_id),
                reason: format!("allowed by explicit policy rule: {}", allow_rule.reason),
                scope: allow_rule.scope.clone(),
            };
        }

        // 6. Read-only file inspection strictly inside the project workspace root
        //    (all targets already verified non-empty & in-workspace in Step 1, and Deny-checked in Step 2).
        if is_file_read {
            let joined_rel = normalized_rel_targets.join(", ");
            return PolicyEvaluationOutcome {
                effect: PolicyEffect::Allow,
                matched_rule_id: None,
                reason: format!("allowed read-only workspace inspection of `{joined_rel}`"),
                scope: format!("project:{}", self.ctx.project_id),
            };
        }

        // 7. Default-DENY for unknown or dangerous tools (shell, run_command, network, ask_before_write without rule).
        PolicyEvaluationOutcome {
            effect: PolicyEffect::Deny,
            matched_rule_id: None,
            reason: format!(
                "default-deny outside model: tool `{}` has no matching allow rule",
                req.tool_name
            ),
            scope: format!("session:{}", self.ctx.session_id),
        }
    }

    fn active_scopes(&self) -> [String; 4] {
        [
            format!("session:{}", self.ctx.session_id),
            format!("agent:{}", self.ctx.agent_id),
            format!("project:{}", self.ctx.project_id),
            String::from("global"),
        ]
    }
}

/// Recording [`PolicyEvaluator`] adapter used during a single Head turn so every external
/// policy check is captured into durable [`PermissionDecisionRecord`] entries.
pub struct RecordingPolicyEvaluator {
    engine: PolicyEngine,
    correlation_id: CorrelationId,
    secrets_to_redact: Vec<String>,
    decisions: Arc<Mutex<Vec<PermissionDecisionRecord>>>,
}

impl RecordingPolicyEvaluator {
    /// Creates a new recording evaluator sharing a decision buffer with the caller.
    #[must_use]
    pub fn new(
        engine: PolicyEngine,
        correlation_id: CorrelationId,
        secrets_to_redact: Vec<String>,
    ) -> (Self, Arc<Mutex<Vec<PermissionDecisionRecord>>>) {
        let decisions = Arc::new(Mutex::new(Vec::new()));
        let eval = Self {
            engine,
            correlation_id,
            secrets_to_redact,
            decisions: Arc::clone(&decisions),
        };
        (eval, decisions)
    }
}

impl PolicyEvaluator for RecordingPolicyEvaluator {
    fn evaluate(&self, req: &PolicyEvaluationRequest) -> PolicyEvaluationOutcome {
        let outcome = self.engine.evaluate_request(req);
        let extra: Vec<&str> = self.secrets_to_redact.iter().map(String::as_str).collect();
        let safe_target = req.target.as_deref().map(|t| redact_secrets(t, &extra));
        let safe_reason = redact_secrets(&outcome.reason, &extra);

        let record = PermissionDecisionRecord {
            decision_id: PermissionDecisionId::new(),
            session_id: req.session_id,
            run_id: Some(req.run_id),
            tool_call_id: req.tool_call_id.clone(),
            tool_name: req.tool_name.clone(),
            target_pattern: safe_target,
            scope: outcome.scope.clone(),
            effect: outcome.effect,
            matched_rule_id: outcome.matched_rule_id,
            reason: safe_reason,
            evaluated_outside_model: true,
            correlation_id: self.correlation_id.clone(),
            created_at: TimestampMs::now(),
        };
        if let Ok(mut guard) = self.decisions.lock() {
            guard.push(record);
        }
        outcome
    }
}

const SHELL_INPUT_KEYS: &[&str] = &["CommandLine", "command", "cmd", "script", "shell"];
const FILE_TARGET_INPUT_KEYS: &[&str] = &[
    "Path",
    "path",
    "TargetFile",
    "AbsolutePath",
    "file_path",
    "DirectoryPath",
    "SearchDirectory",
    "SearchPath",
    "Uri",
    "uri",
];
const MEMORY_MCP_OPS: [&str; 5] = [
    "memory_search",
    "memory_get_current",
    "memory_get_history",
    "memory_write_note",
    "memory_correct_note",
];

fn has_any_json_key(raw_input: &serde_json::Value, keys: &[&str]) -> bool {
    let Some(obj) = raw_input.as_object() else {
        return false;
    };
    keys.iter().any(|k| obj.contains_key(*k))
}

fn is_execute_tool(tool_lower: &str, tool_kind: &str, raw_input: &serde_json::Value) -> bool {
    tool_kind.eq_ignore_ascii_case("execute")
        || matches!(
            tool_lower,
            "run_command"
                | "bash"
                | "sh"
                | "zsh"
                | "fish"
                | "powershell"
                | "pwsh"
                | "cmd"
                | "shell"
                | "exec"
                | "execute"
                | "terminal"
                | "send_command_input"
        )
        || has_any_json_key(raw_input, SHELL_INPUT_KEYS)
}

fn is_write_tool(tool_lower: &str, tool_kind: &str) -> bool {
    tool_kind.eq_ignore_ascii_case("edit")
        || tool_kind.eq_ignore_ascii_case("delete")
        || tool_kind.eq_ignore_ascii_case("move")
        || matches!(
            tool_lower,
            "write_file"
                | "write_to_file"
                | "edit_file"
                | "replace_file_content"
                | "multi_replace_file_content"
                | "delete_file"
                | "create_file"
                | "move_file"
        )
        || tool_lower.starts_with("write_")
        || tool_lower.starts_with("edit_")
        || tool_lower.starts_with("replace_")
        || tool_lower.starts_with("multi_replace_")
}

fn is_read_tool(tool_lower: &str, tool_kind: &str) -> bool {
    tool_kind.eq_ignore_ascii_case("read")
        || tool_kind.eq_ignore_ascii_case("search")
        || matches!(
            tool_lower,
            "read_file"
                | "view_file"
                | "list_dir"
                | "find_by_name"
                | "grep_search"
                | "view_file_outline"
                | "view_code_item"
        )
}

fn canonical_memory_mcp_op(
    tool_lower: &str,
    tool_kind: &str,
    raw_input: &serde_json::Value,
) -> Option<&'static str> {
    if is_execute_tool(tool_lower, tool_kind, raw_input)
        || is_write_tool(tool_lower, tool_kind)
        || is_read_tool(tool_lower, tool_kind)
        || has_any_json_key(raw_input, FILE_TARGET_INPUT_KEYS)
    {
        return None;
    }

    if tool_lower == "mcp:dennett-memory" {
        return Some("mcp:dennett-memory");
    }

    for op in MEMORY_MCP_OPS {
        if tool_lower == op {
            return Some(op);
        }
        if let Some(prefix) = tool_lower.strip_suffix(op)
            && matches!(
                prefix,
                "mcp:"
                    | "mcp__"
                    | "mcp_dennett-memory_"
                    | "mcp__dennett-memory__"
                    | "mcp_dennett_memory_"
                    | "mcp__dennett_memory__"
                    | "dennett-memory:"
                    | "dennett-memory/"
                    | "dennett-memory."
                    | "dennett-memory__"
                    | "dennett_memory:"
                    | "dennett_memory/"
                    | "dennett_memory."
                    | "dennett_memory__"
            )
        {
            return Some(op);
        }
    }
    None
}

fn collect_file_targets(req: &PolicyEvaluationRequest) -> Vec<String> {
    let mut targets = Vec::new();
    let mut push_unique = |candidate: &str| {
        let trimmed = candidate.trim();
        if !trimmed.is_empty() && !targets.iter().any(|existing| existing == trimmed) {
            targets.push(trimmed.to_owned());
        }
    };

    if let Some(ref primary) = req.target {
        push_unique(primary);
    }
    for loc in &req.locations {
        push_unique(loc);
    }
    for key in FILE_TARGET_INPUT_KEYS {
        if let Some(val) = req.raw_input.get(*key).and_then(serde_json::Value::as_str) {
            push_unique(val);
        }
    }
    targets
}

fn rule_matches_any(rule: &PolicyRule, tool_lower: &str, targets: &[&str]) -> bool {
    if targets.is_empty() {
        return rule_matches(rule, tool_lower, None);
    }
    targets
        .iter()
        .any(|target| rule_matches(rule, tool_lower, Some(target)))
}

fn rule_matches(rule: &PolicyRule, tool_lower: &str, target: Option<&str>) -> bool {
    let pattern = rule.tool_name.trim().to_ascii_lowercase();
    let tool_ok = if pattern == "*" {
        true
    } else if let Some(prefix) = pattern.strip_suffix('*') {
        tool_lower.starts_with(prefix)
    } else {
        tool_lower == pattern
    };
    if !tool_ok {
        return false;
    }

    let Some(target_pat) = rule
        .target_pattern
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        return true;
    };
    if target_pat == "*" {
        return true;
    }
    let Some(actual) = target.map(str::trim).filter(|s| !s.is_empty()) else {
        return false;
    };

    let norm_pat = target_pat.replace('\\', "/").to_ascii_lowercase();
    let norm_actual = actual.replace('\\', "/").to_ascii_lowercase();
    if let Some(prefix) = norm_pat.strip_suffix('*') {
        norm_actual.starts_with(prefix)
    } else {
        norm_actual == norm_pat || norm_actual.starts_with(&format!("{norm_pat}/"))
    }
}

fn matches_any_prefix(rel_path: &str, allowed_prefixes: &[String]) -> bool {
    let norm_rel = rel_path.replace('\\', "/");
    for raw in allowed_prefixes {
        let trimmed = raw.trim().replace('\\', "/");
        let clean = trimmed.trim_start_matches("./").trim_end_matches('/');
        if clean.is_empty() {
            continue;
        }
        let matches = if cfg!(windows) {
            norm_rel.eq_ignore_ascii_case(clean)
                || norm_rel
                    .to_ascii_lowercase()
                    .starts_with(&format!("{}/", clean.to_ascii_lowercase()))
        } else {
            norm_rel == clean || norm_rel.starts_with(&format!("{clean}/"))
        };
        if matches {
            return true;
        }
    }
    false
}

/// Resolves `raw_target` strictly inside `workspace_root`, rejecting `..` traversal,
/// absolute escapes, and symlink/NTFS-junction escapes outside the canonical workspace root.
///
/// Returns the normalized workspace-relative path (`/`-separated) when safe.
///
/// # Errors
///
/// Returns a safe explanation string when the path escapes or cannot be verified within workspace.
pub fn resolve_within_workspace(workspace_root: &Path, raw_target: &str) -> Result<String, String> {
    if raw_target.as_bytes().contains(&0) {
        return Err(String::from("denied path containing NUL byte"));
    }

    let canonical_root = fs::canonicalize(workspace_root).map_err(|err| {
        format!(
            "denied: workspace root `{}` could not be canonicalized: {err}",
            workspace_root.display()
        )
    })?;

    let joined = normalize_candidate_path(&canonical_root, raw_target)?;

    // Walk ancestors up to the deepest existing node and canonicalize it to detect symlink/junction escapes!
    let mut existing = joined;
    let mut non_existing_suffix: Vec<std::ffi::OsString> = Vec::new();
    while !existing.exists() {
        // Even if the path doesn't resolve via `exists()`, check `symlink_metadata` in case
        // it is a broken symlink/reparse point pointing outside the workspace!
        if fs::symlink_metadata(&existing).is_ok() {
            return Err(format!(
                "denied path `{raw_target}`: dangling symlink/reparse point is forbidden"
            ));
        }
        let Some(name) = existing.file_name().map(ToOwned::to_owned) else {
            return Err(format!(
                "denied path `{raw_target}`: cannot resolve within workspace boundary"
            ));
        };
        non_existing_suffix.push(name);
        if !existing.pop() {
            return Err(format!(
                "denied path `{raw_target}`: escaped filesystem root"
            ));
        }
    }

    let canonical_existing = fs::canonicalize(&existing).map_err(|err| {
        format!("denied path `{raw_target}`: failed canonicalizing ancestor: {err}")
    })?;

    if !canonical_existing.starts_with(&canonical_root) {
        return Err(format!(
            "denied path `{raw_target}`: target escapes project workspace boundary (including symlink/junction check)"
        ));
    }

    // Reassemble the canonical target path with any non-existing trailing segments.
    let mut full_canonical = canonical_existing;
    for seg in non_existing_suffix.into_iter().rev() {
        full_canonical.push(seg);
    }

    let rel = full_canonical
        .strip_prefix(&canonical_root)
        .map_err(|_| format!("denied path `{raw_target}`: target is outside workspace root"))?;
    let rel_str = rel
        .to_string_lossy()
        .replace('\\', "/")
        .trim_start_matches('/')
        .to_owned();
    Ok(rel_str)
}

fn normalize_candidate_path(canonical_root: &Path, raw_target: &str) -> Result<PathBuf, String> {
    // Handle `file://` URIs cleanly and reject non-file URI schemes / remote file hosts.
    let trimmed_raw = raw_target.trim();
    let without_scheme = if let Some(rest) = trimmed_raw.strip_prefix("file://localhost/") {
        format!("/{rest}")
    } else if let Some(rest) = trimmed_raw.strip_prefix("file:///") {
        format!("/{rest}")
    } else if trimmed_raw.contains("://") {
        return Err(format!(
            "denied path `{raw_target}`: unsupported or remote URI scheme is forbidden"
        ));
    } else {
        trimmed_raw.to_owned()
    };

    // Strip leading `/C:/...` style URL paths emitted occasionally by ACP file tools on Windows.
    let cleaned_raw = strip_leading_slash_before_drive(&without_scheme);
    let candidate_path = PathBuf::from(cleaned_raw);
    let is_abs = candidate_path.is_absolute();

    // Reject explicit parent directory traversal (`..`), UNC/device prefixes, and
    // non-absolute Windows drive-relative (`C:foo.txt`) or root-relative (`\foo`) paths!
    let mut joined = if is_abs {
        PathBuf::new()
    } else {
        canonical_root.to_path_buf()
    };
    for comp in candidate_path.components() {
        match comp {
            Component::ParentDir => {
                return Err(format!(
                    "denied path `{raw_target}`: parent-directory traversal (`..`) is forbidden"
                ));
            }
            Component::Prefix(prefix) => {
                if !is_abs {
                    return Err(format!(
                        "denied path `{raw_target}`: drive-relative path is forbidden"
                    ));
                }
                if matches!(
                    prefix.kind(),
                    std::path::Prefix::UNC(..)
                        | std::path::Prefix::VerbatimUNC(..)
                        | std::path::Prefix::DeviceNS(..)
                ) {
                    return Err(format!(
                        "denied path `{raw_target}`: UNC or device namespace path is forbidden"
                    ));
                }
                joined.push(prefix.as_os_str());
            }
            Component::RootDir => {
                if !is_abs {
                    return Err(format!(
                        "denied path `{raw_target}`: drive-root-relative path is forbidden"
                    ));
                }
                joined.push(comp.as_os_str());
            }
            Component::Normal(seg) => joined.push(seg),
            Component::CurDir => {}
        }
    }
    Ok(joined)
}

fn strip_leading_slash_before_drive(raw: &str) -> &str {
    let bytes = raw.as_bytes();
    if cfg!(windows)
        && bytes.len() >= 3
        && bytes[0] == b'/'
        && bytes[1].is_ascii_alphabetic()
        && bytes[2] == b':'
    {
        &raw[1..]
    } else {
        raw
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use serde_json::json;

    use super::*;
    use dennett_contracts::{PolicyRuleId, RunId};

    fn make_req(tool_name: &str, tool_kind: &str, target: Option<&str>) -> PolicyEvaluationRequest {
        PolicyEvaluationRequest {
            session_id: SessionId::new(),
            run_id: RunId::new(),
            tool_call_id: String::from("call-1"),
            tool_name: tool_name.to_owned(),
            tool_kind: tool_kind.to_owned(),
            title: format!("{tool_name} {}", target.unwrap_or_default()),
            target: target.map(str::to_owned),
            locations: Vec::new(),
            raw_input: json!({}),
        }
    }

    fn temp_workspace(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "dennett-policy-test-{label}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&dir).expect("create temp workspace");
        dir
    }

    #[test]
    fn policy_engine_enforces_workspace_boundary_deny_override_and_default_deny() {
        let ws = temp_workspace("core");
        let outside = temp_workspace("outside");
        let scope_alpha = MemoryScope::try_new("project-alpha").expect("scope");

        let allow_all_rule = PolicyRule {
            rule_id: PolicyRuleId::new(),
            scope: String::from("global"),
            tool_name: String::from("*"),
            target_pattern: None,
            effect: PolicyEffect::Allow,
            reason: String::from("overly broad allow"),
            created_at: TimestampMs::now(),
        };
        let deny_shell_rule = PolicyRule {
            rule_id: PolicyRuleId::new(),
            scope: String::from("global"),
            tool_name: String::from("run_command"),
            target_pattern: None,
            effect: PolicyEffect::Deny,
            reason: String::from("shell commands denied"),
            created_at: TimestampMs::now(),
        };

        let engine = PolicyEngine::new(PolicyContext {
            project_id: ProjectId::new(),
            agent_id: AgentId::new(),
            session_id: SessionId::new(),
            workspace_root: ws.clone(),
            workspace_write_policy: WorkspaceWritePolicy::AllowPaths {
                allowed_prefixes: vec![String::from("docs/")],
            },
            allowed_memory_scopes: BTreeSet::from([scope_alpha]),
            rules: vec![allow_all_rule, deny_shell_rule],
        });

        // 1. Traversal escape is ALWAYS denied even with wildcard Allow rule!
        let escape = engine.evaluate_request(&make_req("write_file", "edit", Some("../evil.txt")));
        assert_eq!(escape.effect, PolicyEffect::Deny);

        // 2. Absolute outside path is ALWAYS denied!
        let outside_file = outside.join("steal.txt");
        let abs_escape = engine.evaluate_request(&make_req(
            "write_file",
            "edit",
            Some(&outside_file.to_string_lossy()),
        ));
        assert_eq!(abs_escape.effect, PolicyEffect::Deny);

        // 3. Path inside workspace but outside `docs/` prefix is denied!
        let wrong_subdir =
            engine.evaluate_request(&make_req("write_file", "edit", Some("src/main.rs")));
        assert_eq!(wrong_subdir.effect, PolicyEffect::Deny);

        // 4. Path inside `docs/` prefix is allowed!
        let ok_write =
            engine.evaluate_request(&make_req("write_file", "edit", Some("docs/report.md")));
        assert_eq!(ok_write.effect, PolicyEffect::Allow);

        // 5. Explicit Deny beats Allow!
        let denied_shell =
            engine.evaluate_request(&make_req("run_command", "execute", Some("echo hi")));
        assert_eq!(denied_shell.effect, PolicyEffect::Deny);
        assert!(denied_shell.matched_rule_id.is_some());

        // 6. Memory MCP tool enforces allowed_memory_scopes!
        let mut bad_mem = make_req("memory_write_note", "other", Some("project-beta"));
        bad_mem.raw_input = json!({"scope": "project-beta"});
        assert_eq!(engine.evaluate_request(&bad_mem).effect, PolicyEffect::Deny);

        let mut good_mem = make_req("memory_write_note", "other", Some("project-alpha"));
        good_mem.raw_input = json!({"scope": "project-alpha"});
        assert_eq!(
            engine.evaluate_request(&good_mem).effect,
            PolicyEffect::Allow
        );

        let _ = fs::remove_dir_all(&ws);
        let _ = fs::remove_dir_all(&outside);
    }

    #[test]
    fn policy_engine_default_denies_unknown_dangerous_tools() {
        let ws = temp_workspace("default-deny");
        let engine = PolicyEngine::new(PolicyContext {
            project_id: ProjectId::new(),
            agent_id: AgentId::new(),
            session_id: SessionId::new(),
            workspace_root: ws.clone(),
            workspace_write_policy: WorkspaceWritePolicy::DenyAll,
            allowed_memory_scopes: BTreeSet::new(),
            rules: vec![],
        });

        assert_eq!(
            engine
                .evaluate_request(&make_req("run_command", "execute", Some("dir")))
                .effect,
            PolicyEffect::Deny
        );
        assert_eq!(
            engine
                .evaluate_request(&make_req("write_file", "edit", Some("a.txt")))
                .effect,
            PolicyEffect::Deny
        );
        assert_eq!(
            engine
                .evaluate_request(&make_req("unknown_network_tool", "other", Some("http://x")))
                .effect,
            PolicyEffect::Deny
        );

        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn defect1_shell_and_non_mcp_tools_cannot_bypass_policy_via_memory_substrings() {
        let ws = temp_workspace("defect1");
        let scope_alpha = MemoryScope::try_new("project-alpha").expect("scope");
        let engine = PolicyEngine::new(PolicyContext {
            project_id: ProjectId::new(),
            agent_id: AgentId::new(),
            session_id: SessionId::new(),
            workspace_root: ws.clone(),
            workspace_write_policy: WorkspaceWritePolicy::DenyAll,
            allowed_memory_scopes: BTreeSet::from([scope_alpha]),
            rules: vec![],
        });

        // (a) Native `run_command` containing `dennett-memory` in CommandLine and title MUST be denied!
        let mut spoofed_shell = make_req(
            "run_command",
            "execute",
            Some("powershell -c whoami # dennett-memory"),
        );
        spoofed_shell.title = String::from("powershell -c whoami # dennett-memory");
        spoofed_shell.raw_input = json!({ "CommandLine": "powershell -c whoami # dennett-memory" });
        assert_eq!(
            engine.evaluate_request(&spoofed_shell).effect,
            PolicyEffect::Deny
        );

        // (b) Even if `kind` is `"other"` and `tool_name` claims `"memory_search"`, shell `CommandLine` MUST deny!
        let mut shell_in_mcp_clothing = make_req("memory_search", "other", Some("whoami"));
        shell_in_mcp_clothing.title = String::from("dennett-memory: memory_search");
        shell_in_mcp_clothing.raw_input = json!({ "CommandLine": "whoami" });
        assert_eq!(
            engine.evaluate_request(&shell_in_mcp_clothing).effect,
            PolicyEffect::Deny
        );

        // Unrecognized tool containing substring `memory_search` MUST NOT match MCP allow!
        let mut evil_sub = make_req("evil_shell_memory_search", "other", None);
        evil_sub.raw_input = json!({ "scope": "project-alpha" });
        assert_eq!(
            engine.evaluate_request(&evil_sub).effect,
            PolicyEffect::Deny
        );

        // (c) `memory_write_note` and `memory_correct_note` MUST require non-empty string `scope`!
        let missing_scope_write = make_req("memory_write_note", "other", None);
        assert_eq!(
            engine.evaluate_request(&missing_scope_write).effect,
            PolicyEffect::Deny
        );

        let mut non_string_scope_write = make_req("memory_write_note", "other", None);
        non_string_scope_write.raw_input = json!({ "scope": 123, "title": "x", "body": "y" });
        assert_eq!(
            engine.evaluate_request(&non_string_scope_write).effect,
            PolicyEffect::Deny
        );

        let missing_scope_correct = make_req("memory_correct_note", "other", None);
        assert_eq!(
            engine.evaluate_request(&missing_scope_correct).effect,
            PolicyEffect::Deny
        );

        // Genuine scoped MCP calls succeed.
        let mut ok_search = make_req("mcp__dennett-memory__memory_search", "other", None);
        ok_search.raw_input = json!({ "scope": "project-alpha", "query": "test" });
        assert_eq!(
            engine.evaluate_request(&ok_search).effect,
            PolicyEffect::Allow
        );

        let mut ok_write = make_req("dennett-memory:memory_write_note", "other", None);
        ok_write.raw_input = json!({ "scope": "project-alpha", "title": "t", "body": "b" });
        assert_eq!(
            engine.evaluate_request(&ok_write).effect,
            PolicyEffect::Allow
        );

        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn defect2_file_reads_fail_closed_check_all_locations_and_enforce_relative_deny_rules_on_abs_paths()
     {
        let ws = temp_workspace("defect2-ws");
        let outside = temp_workspace("defect2-out");
        fs::create_dir_all(ws.join("docs")).expect("create docs");
        fs::create_dir_all(ws.join("secrets")).expect("create secrets");
        fs::write(ws.join("docs").join("readme.md"), "public").expect("write readme");
        fs::write(ws.join("secrets").join("key.pem"), "SECRET").expect("write key");

        let deny_secrets = PolicyRule {
            rule_id: PolicyRuleId::new(),
            scope: String::from("global"),
            tool_name: String::from("*"),
            target_pattern: Some(String::from("secrets/*")),
            effect: PolicyEffect::Deny,
            reason: String::from("never read or write secrets directory"),
            created_at: TimestampMs::now(),
        };
        let engine = PolicyEngine::new(PolicyContext {
            project_id: ProjectId::new(),
            agent_id: AgentId::new(),
            session_id: SessionId::new(),
            workspace_root: ws.clone(),
            workspace_write_policy: WorkspaceWritePolicy::DenyAll,
            allowed_memory_scopes: BTreeSet::new(),
            rules: vec![deny_secrets],
        });

        // (a) Missing target on read tool MUST fail CLOSED (`Deny`)!
        let missing_read = make_req("read_file", "read", None);
        assert_eq!(
            engine.evaluate_request(&missing_read).effect,
            PolicyEffect::Deny
        );

        // Outside target via `DirectoryPath` / `SearchDirectory` / `SearchPath` MUST be denied!
        let mut outside_dir_req = make_req("list_dir", "read", None);
        outside_dir_req.raw_input = json!({ "DirectoryPath": outside.to_string_lossy() });
        assert_eq!(
            engine.evaluate_request(&outside_dir_req).effect,
            PolicyEffect::Deny
        );

        // Secondary escaping location in `locations[1]` MUST be denied even when `target` is inside ws!
        let mut multi_loc_escape = make_req("read_file", "read", Some("docs/readme.md"));
        multi_loc_escape.locations = vec![
            String::from("docs/readme.md"),
            outside.to_string_lossy().into_owned(),
        ];
        assert_eq!(
            engine.evaluate_request(&multi_loc_escape).effect,
            PolicyEffect::Deny
        );

        // (b) Explicit relative `Deny` rule (`secrets/*`) MUST block ABSOLUTE path `<ws>/secrets/key.pem`!
        let abs_secret = ws.join("secrets").join("key.pem");
        let abs_secret_req = make_req("read_file", "read", Some(&abs_secret.to_string_lossy()));
        let abs_res = engine.evaluate_request(&abs_secret_req);
        assert_eq!(abs_res.effect, PolicyEffect::Deny);
        assert!(abs_res.matched_rule_id.is_some());

        // Safe relative path with `./` inside workspace resolves cleanly and is allowed.
        let ok_rel = make_req("read_file", "read", Some("./docs/./readme.md"));
        assert_eq!(engine.evaluate_request(&ok_rel).effect, PolicyEffect::Allow);

        // (c) Windows drive-relative (`C:foo.txt`), remote URI, and UNC paths MUST be rejected!
        if cfg!(windows) {
            assert!(resolve_within_workspace(&ws, "C:foo.txt").is_err());
            assert!(resolve_within_workspace(&ws, r"\Windows\System32").is_err());
            assert!(resolve_within_workspace(&ws, r"\\attacker\share\file.txt").is_err());
        }
        assert!(resolve_within_workspace(&ws, "https://evil.example/x").is_err());
        assert_eq!(
            resolve_within_workspace(&ws, "./docs/./readme.md").expect("clean rel"),
            "docs/readme.md"
        );

        let _ = fs::remove_dir_all(&ws);
        let _ = fs::remove_dir_all(&outside);
    }

    proptest! {
        #[test]
        fn prop_traversal_paths_never_pass_workspace_boundary(
            prefix in "[a-z0-9_]{0,8}",
            suffix in "[a-z0-9_.]{1,12}",
        ) {
            let ws = temp_workspace("prop-trav");
            let bad = if prefix.is_empty() {
                format!("../{suffix}")
            } else {
                format!("{prefix}/../../{suffix}")
            };
            let res = resolve_within_workspace(&ws, &bad);
            let _ = fs::remove_dir_all(&ws);
            prop_assert!(res.is_err(), "traversal path `{bad}` must always be rejected");
        }
    }
}
