//! Live interactive permission approvals (`ask_before_write`), revocable `CapabilityGrants`,
//! and Action Inbox (`inbox_cards`) management (`A05`, `A06`).
//!
//! Security invariants:
//! - Every approval decision is re-verified outside the model through [`PolicyEngine`] before
//!   allowing execution (re-checking canonical workspace containment, symlink/junction blocks,
//!   and explicit `Deny` rules).
//! - The runtime (`agy acp`) ALWAYS receives strictly `allow_once` (never `allow_always`) so
//!   every subsequent tool call passes back through Head's reference monitor and observes instant
//!   grant revocation (`POST /v1/grants/{grant_id}/revoke`).
//! - Optimistic concurrency (`expected_revision`) rejects duplicate/stale resolutions with `409 Conflict`.

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use dennett_contracts::redact::redact_secrets_owned;
use dennett_contracts::{
    CapabilityGrantId, CapabilityGrantRecord, CorrelationId, DennettError, InboxCardId,
    InboxCardKind, InboxCardRecord, InboxCardStatus, InboxResolutionAction,
    ListCapabilityGrantsResponse, ListInboxResponse, ListPermissionRequestsResponse,
    PermissionDecisionId, PermissionDecisionRecord, PermissionRequestId, PermissionRequestStatus,
    PermissionResolutionDecision, PolicyEffect, ProjectId, ProjectLifecycleState,
    ReconcileEffectRequest, ReplyTaskRequest, ResolveInboxCardRequest, ResolveInboxCardResponse,
    ResolvePermissionRequest, ResolvePermissionResponse, RevokeGrantRequest, RunStatus, SessionId,
    TaskId, TaskStatus, TimestampMs, WorkspaceWritePolicy,
};
use dennett_runtime_antigravity::{
    BoxFuture, PolicyEvaluationOutcome, PolicyEvaluationRequest, PolicyEvaluator,
};
use serde_json::json;
use tokio::sync::{oneshot, watch};
use tokio::time;

use crate::events::{
    ControlEventAppend, PendingControlEvent, append_control_event_in_tx,
    append_control_events_batch_in_tx,
};
use crate::migrations::sha256_hex;
use crate::policy::{
    PolicyContext, PolicyEngine, PolicyEvaluationDecision, is_workspace_write_tool,
    resolve_within_workspace,
};
use crate::service::{DEFAULT_PG_OP_TIMEOUT, HeadService, insert_permission_decision};
use crate::store::{
    CAPABILITY_GRANT_SELECT_COLS, INBOX_CARD_SELECT_COLS, PERMISSION_REQUEST_SELECT_COLS,
    TASK_SELECT_COLS, append_audit_event, decode_capability_grant_row, decode_inbox_card_row,
    decode_permission_request_row, decode_task_row, ensure_project_active_in_tx,
    fetch_active_project_grants, fetch_agent, fetch_policy_rules, fetch_project, fetch_task,
    map_pg_err, ms_to_pg_epoch_secs, truncate_utf8,
};

/// Live PostgreSQL policy inputs refreshed on every tool invocation (`D2-BUG-02`, `D2-SEC-02`).
pub(crate) struct LivePolicyInputs {
    pub(crate) rules: Vec<dennett_contracts::PolicyRule>,
    pub(crate) grants: Vec<CapabilityGrantRecord>,
    pub(crate) project_lifecycle_state: ProjectLifecycleState,
    pub(crate) workspace_bound: bool,
    pub(crate) parent_write_ceiling: Option<WorkspaceWritePolicy>,
}

/// Default timeout for waiting on interactive owner approval before expiring fail-closed.
pub const DEFAULT_APPROVAL_WAIT_TIMEOUT: Duration = Duration::from_secs(300);

/// Decision captured during a turn, tracking whether it was already persisted by interactive resolution.
#[derive(Debug, Clone)]
pub struct CapturedPermissionDecision {
    /// Audited external decision record.
    pub record: PermissionDecisionRecord,
    /// `true` when already inserted into `permission_decisions` during interactive `resolve`.
    pub already_persisted: bool,
}

/// Waiter outcome delivered from `resolve_permission_request` to the suspended runtime callback.
#[derive(Debug, Clone)]
pub(crate) struct ApprovalResolutionSignal {
    pub(crate) outcome: PolicyEvaluationOutcome,
    pub(crate) decision_record: Option<PermissionDecisionRecord>,
}

/// Async [`PolicyEvaluator`] supporting live `WorkspaceWritePolicy::AskBeforeWrite` owner
/// approvals via PostgreSQL `permission_requests` + `inbox_cards` and revocable `capability_grants`.
pub struct InteractivePolicyEvaluator {
    service: HeadService,
    base_ctx: PolicyContext,
    task_id: Option<TaskId>,
    correlation_id: CorrelationId,
    secrets_to_redact: Vec<String>,
    cancel_rx: watch::Receiver<bool>,
    approval_timeout: Duration,
    decisions: Arc<StdMutex<Vec<CapturedPermissionDecision>>>,
}

impl InteractivePolicyEvaluator {
    /// Creates a new [`InteractivePolicyEvaluator`] sharing a decision capture buffer with the turn runner.
    #[must_use]
    pub fn new(
        service: HeadService,
        base_ctx: PolicyContext,
        task_id: Option<TaskId>,
        correlation_id: CorrelationId,
        secrets_to_redact: Vec<String>,
        cancel_rx: watch::Receiver<bool>,
        approval_timeout: Duration,
    ) -> (Self, Arc<StdMutex<Vec<CapturedPermissionDecision>>>) {
        let decisions = Arc::new(StdMutex::new(Vec::new()));
        let eval = Self {
            service,
            base_ctx,
            task_id,
            correlation_id,
            secrets_to_redact,
            cancel_rx,
            approval_timeout,
            decisions: Arc::clone(&decisions),
        };
        (eval, decisions)
    }

    fn record_decision(
        &self,
        req: &PolicyEvaluationRequest,
        outcome: &PolicyEvaluationOutcome,
        existing_record: Option<PermissionDecisionRecord>,
    ) {
        let captured = if let Some(record) = existing_record {
            CapturedPermissionDecision {
                record,
                already_persisted: true,
            }
        } else {
            let safe_target = req
                .target
                .as_deref()
                .map(|t| redact_secrets_owned(t, &self.secrets_to_redact));
            let safe_reason = redact_secrets_owned(&outcome.reason, &self.secrets_to_redact);
            CapturedPermissionDecision {
                record: PermissionDecisionRecord {
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
                },
                already_persisted: false,
            }
        };
        if let Ok(mut guard) = self.decisions.lock() {
            guard.push(captured);
        }
    }
}

impl PolicyEvaluator for InteractivePolicyEvaluator {
    fn evaluate<'a>(
        &'a self,
        req: &'a PolicyEvaluationRequest,
    ) -> BoxFuture<'a, PolicyEvaluationOutcome> {
        Box::pin(async move {
            // 1. Live lookup of policy rules, active capability grants, live project lifecycle
            //    state, and delegated parent write ceiling from PostgreSQL so revoked grants,
            //    new Deny rules, and mid-turn archive/detach take effect immediately (D2-BUG-02).
            let live = match self
                .service
                .load_live_policy_inputs(self.base_ctx.project_id, self.task_id)
                .await
            {
                Ok(inputs) => inputs,
                Err(err) => {
                    let scope = self.base_ctx.project_id.map_or_else(
                        || format!("session:{}", self.base_ctx.session_id),
                        |pid| format!("project:{pid}"),
                    );
                    let outcome = PolicyEvaluationOutcome {
                        effect: PolicyEffect::Deny,
                        matched_rule_id: None,
                        reason: format!("fail-closed: could not query live policy state ({err})"),
                        scope,
                    };
                    self.record_decision(req, &outcome, None);
                    return outcome;
                }
            };

            let mut ctx = self.base_ctx.clone();
            ctx.rules = live.rules;
            ctx.project_lifecycle_state = live.project_lifecycle_state;
            ctx.workspace_bound = live.workspace_bound;
            ctx.parent_write_ceiling = live.parent_write_ceiling;
            let engine = PolicyEngine::new(ctx);

            let decision = engine.evaluate_with_grants(req, &live.grants, TimestampMs::now());
            match decision {
                PolicyEvaluationDecision::Immediate(outcome) => {
                    self.record_decision(req, &outcome, None);
                    outcome
                }
                PolicyEvaluationDecision::AllowedByGrant { grant_id, outcome } => {
                    // Atomically verify `revoked_at IS NULL` and increment `uses_consumed` in PG!
                    if self
                        .service
                        .try_consume_capability_grant(grant_id)
                        .await
                        .unwrap_or(false)
                    {
                        self.record_decision(req, &outcome, None);
                        return outcome;
                    }
                    // Grant was revoked/expired/exhausted concurrently -> fall through to interactive prompt!
                    let fallback = engine.evaluate_with_grants(req, &[], TimestampMs::now());
                    match fallback {
                        PolicyEvaluationDecision::NeedsInteractiveApproval {
                            normalized_targets,
                            target_summary,
                            scope,
                            reason,
                        } => {
                            self.await_interactive_approval(
                                req,
                                normalized_targets,
                                target_summary,
                                scope,
                                reason,
                            )
                            .await
                        }
                        PolicyEvaluationDecision::Immediate(o)
                        | PolicyEvaluationDecision::AllowedByGrant { outcome: o, .. } => {
                            self.record_decision(req, &o, None);
                            o
                        }
                    }
                }
                PolicyEvaluationDecision::NeedsInteractiveApproval {
                    normalized_targets,
                    target_summary,
                    scope,
                    reason,
                } => {
                    self.await_interactive_approval(
                        req,
                        normalized_targets,
                        target_summary,
                        scope,
                        reason,
                    )
                    .await
                }
            }
        })
    }
}

impl InteractivePolicyEvaluator {
    #[expect(
        clippy::too_many_lines,
        reason = "handles atomic approval card creation, oneshot registration, cancellation-safe select, and fail-closed cleanup"
    )]
    async fn await_interactive_approval(
        &self,
        req: &PolicyEvaluationRequest,
        normalized_targets: Vec<String>,
        target_summary: String,
        scope: String,
        reason: String,
    ) -> PolicyEvaluationOutcome {
        let mut cancel_rx = self.cancel_rx.clone();
        if *cancel_rx.borrow() {
            let outcome = PolicyEvaluationOutcome {
                effect: PolicyEffect::Deny,
                matched_rule_id: None,
                reason: String::from("abandoned because run was cancelled"),
                scope,
            };
            self.record_decision(req, &outcome, None);
            return outcome;
        }

        let params_fingerprint_src = format!(
            "{}\0{}\0{}\0{}",
            req.tool_name,
            req.tool_kind,
            normalized_targets.join(","),
            req.raw_input
        );
        let params_sha = sha256_hex(params_fingerprint_src.as_bytes());
        let safe_target_summary = redact_secrets_owned(&target_summary, &self.secrets_to_redact);
        let safe_reason = redact_secrets_owned(&reason, &self.secrets_to_redact);
        let preview_snippet =
            extract_redacted_preview_snippet(&req.raw_input, &self.secrets_to_redact);

        let Some(project_id) = self.base_ctx.project_id else {
            let outcome = PolicyEvaluationOutcome {
                effect: PolicyEffect::Deny,
                matched_rule_id: None,
                reason: String::from(
                    "denied: standalone chat sessions enforce deny_all and do not support workspace approvals",
                ),
                scope,
            };
            self.record_decision(req, &outcome, None);
            return outcome;
        };

        let (tx, rx) = oneshot::channel::<ApprovalResolutionSignal>();
        let request_id = match self
            .service
            .create_pending_permission_request(
                project_id,
                self.base_ctx.session_id,
                req.run_id,
                self.task_id,
                self.base_ctx.agent_id,
                &req.tool_call_id,
                &req.tool_name,
                &req.tool_kind,
                &safe_target_summary,
                &normalized_targets,
                &params_sha,
                preview_snippet.as_deref(),
                &safe_reason,
                self.approval_timeout,
                &self.correlation_id,
                tx,
            )
            .await
        {
            Ok(id) => id,
            Err(err) => {
                let outcome = PolicyEvaluationOutcome {
                    effect: PolicyEffect::Deny,
                    matched_rule_id: None,
                    reason: format!(
                        "fail-closed: failed persisting interactive permission request ({err})"
                    ),
                    scope,
                };
                self.record_decision(req, &outcome, None);
                return outcome;
            }
        };

        let sleep = time::sleep(self.approval_timeout);
        tokio::pin!(sleep);
        let mut rx = rx;
        let mut cancel_open = true;

        let resolved: Option<ApprovalResolutionSignal> = loop {
            tokio::select! {
                biased;
                res = &mut rx => break res.ok(),
                changed = cancel_rx.changed(), if cancel_open => {
                    if changed.is_ok() {
                        if *cancel_rx.borrow() {
                            let _ = self.service.expire_pending_permission_request(
                                request_id,
                                "withdrawn",
                                "run cancelled while awaiting approval",
                                &self.correlation_id,
                            ).await;
                            break None;
                        }
                    } else {
                        cancel_open = false;
                    }
                }
                () = &mut sleep => {
                    let _ = self.service.expire_pending_permission_request(
                        request_id,
                        "expired",
                        "interactive approval wait timed out",
                        &self.correlation_id,
                    ).await;
                    break None;
                }
            }
        };

        self.service.remove_approval_waiter(request_id).await;

        if let Some(signal) = resolved {
            self.record_decision(req, &signal.outcome, signal.decision_record);
            signal.outcome
        } else {
            let deny_reason = if *cancel_rx.borrow() {
                String::from("abandoned because run was cancelled during approval wait")
            } else {
                String::from("denied: interactive approval request expired before owner decision")
            };
            let outcome = PolicyEvaluationOutcome {
                effect: PolicyEffect::Deny,
                matched_rule_id: None,
                reason: deny_reason,
                scope,
            };
            self.record_decision(req, &outcome, None);
            outcome
        }
    }
}

fn format_diff_pair(old_text: Option<&str>, new_text: &str) -> String {
    let trimmed_new = new_text.trim();
    if let Some(old) = old_text.map(str::trim).filter(|s| !s.is_empty()) {
        format!("--- old\n{old}\n+++ new\n{trimmed_new}")
    } else {
        trimmed_new.to_owned()
    }
}

fn extract_acp_content_array_preview(items: &[serde_json::Value]) -> Option<String> {
    let mut parts = Vec::new();
    for item in items {
        let Some(obj) = item.as_object() else {
            continue;
        };
        let item_type = obj
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if item_type == "diff" || obj.contains_key("newText") {
            if let Some(new_text) = obj
                .get("newText")
                .and_then(serde_json::Value::as_str)
                .filter(|s| !s.trim().is_empty())
            {
                let old_text = obj.get("oldText").and_then(serde_json::Value::as_str);
                parts.push(format_diff_pair(old_text, new_text));
            }
            continue;
        }
        if let Some(nested_text) = obj
            .get("content")
            .and_then(|c| c.get("text"))
            .and_then(serde_json::Value::as_str)
            .or_else(|| obj.get("text").and_then(serde_json::Value::as_str))
            .filter(|s| !s.trim().is_empty())
        {
            parts.push(nested_text.trim().to_owned());
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n---\n"))
    }
}

fn extract_redacted_preview_snippet(
    raw_input: &serde_json::Value,
    secrets_to_redact: &[String],
) -> Option<String> {
    let obj = raw_input.as_object()?;
    let finalize = |raw: &str| -> Option<String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        let redacted = redact_secrets_owned(trimmed, secrets_to_redact);
        Some(truncate_utf8(&redacted, 2000).to_owned())
    };

    // 1. Direct write / patch keys (covering PascalCase agy acp & camelCase/snake_case tools)
    for key in [
        "Content",
        "content",
        "CodeContent",
        "FileContent",
        "new_content",
        "newContent",
        "newText",
        "text",
        "patch",
        "diff",
    ] {
        if let Some(val) = obj.get(key).and_then(serde_json::Value::as_str)
            && let Some(out) = finalize(val)
        {
            return Some(out);
        }
    }

    // 2. Paired edit keys (Target/Replacement, TargetContent/ReplacementContent, old_string/new_string)
    for (old_key, new_key) in [
        ("Target", "Replacement"),
        ("TargetContent", "ReplacementContent"),
        ("old_string", "new_string"),
        ("oldString", "newString"),
        ("oldText", "newText"),
    ] {
        if let Some(new_val) = obj
            .get(new_key)
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.trim().is_empty())
        {
            let old_val = obj.get(old_key).and_then(serde_json::Value::as_str);
            let combined = format_diff_pair(old_val, new_val);
            if let Some(out) = finalize(&combined) {
                return Some(out);
            }
        }
    }

    // 3. Multi-chunk replacements
    if let Some(chunks) = obj
        .get("ReplacementChunks")
        .and_then(serde_json::Value::as_array)
    {
        let mut buf = String::new();
        for chunk in chunks {
            if let Some(repl) = chunk
                .get("ReplacementContent")
                .or_else(|| chunk.get("Replacement"))
                .and_then(serde_json::Value::as_str)
                .filter(|s| !s.trim().is_empty())
            {
                let old_val = chunk
                    .get("TargetContent")
                    .or_else(|| chunk.get("Target"))
                    .and_then(serde_json::Value::as_str);
                if !buf.is_empty() {
                    buf.push_str("\n---\n");
                }
                buf.push_str(&format_diff_pair(old_val, repl));
            }
        }
        if let Some(out) = finalize(&buf) {
            return Some(out);
        }
    }

    // 4. ACP toolCall.content diff/text array fallback
    for arr_key in ["_acp_content", "content"] {
        if let Some(items) = obj.get(arr_key).and_then(serde_json::Value::as_array)
            && let Some(preview) = extract_acp_content_array_preview(items)
            && let Some(out) = finalize(&preview)
        {
            return Some(out);
        }
    }

    None
}

fn intersect_write_ceilings(
    a: WorkspaceWritePolicy,
    b: WorkspaceWritePolicy,
) -> WorkspaceWritePolicy {
    match (a, b) {
        (WorkspaceWritePolicy::DenyAll, _) | (_, WorkspaceWritePolicy::DenyAll) => {
            WorkspaceWritePolicy::DenyAll
        }
        (WorkspaceWritePolicy::AllowWorkspace, other)
        | (other, WorkspaceWritePolicy::AllowWorkspace) => other,
        (
            WorkspaceWritePolicy::AllowPaths {
                allowed_prefixes: p1,
            },
            WorkspaceWritePolicy::AllowPaths {
                allowed_prefixes: p2,
            },
        ) => {
            let mut narrowed = Vec::new();
            for candidate in &p1 {
                let norm_c = candidate
                    .trim()
                    .replace('\\', "/")
                    .trim_start_matches("./")
                    .trim_end_matches('/')
                    .to_ascii_lowercase();
                if p2.iter().any(|p| {
                    let norm_p = p
                        .trim()
                        .replace('\\', "/")
                        .trim_start_matches("./")
                        .trim_end_matches('/')
                        .to_ascii_lowercase();
                    norm_c == norm_p || norm_c.starts_with(&format!("{norm_p}/"))
                }) {
                    narrowed.push(candidate.clone());
                }
            }
            for candidate in &p2 {
                let norm_c = candidate
                    .trim()
                    .replace('\\', "/")
                    .trim_start_matches("./")
                    .trim_end_matches('/')
                    .to_ascii_lowercase();
                if p1.iter().any(|p| {
                    let norm_p = p
                        .trim()
                        .replace('\\', "/")
                        .trim_start_matches("./")
                        .trim_end_matches('/')
                        .to_ascii_lowercase();
                    norm_c == norm_p || norm_c.starts_with(&format!("{norm_p}/"))
                }) && !narrowed.iter().any(|existing| existing == candidate)
                {
                    narrowed.push(candidate.clone());
                }
            }
            if narrowed.is_empty() {
                WorkspaceWritePolicy::DenyAll
            } else {
                WorkspaceWritePolicy::AllowPaths {
                    allowed_prefixes: narrowed,
                }
            }
        }
        (WorkspaceWritePolicy::AllowPaths { allowed_prefixes }, _)
        | (_, WorkspaceWritePolicy::AllowPaths { allowed_prefixes }) => {
            WorkspaceWritePolicy::AllowPaths { allowed_prefixes }
        }
        (WorkspaceWritePolicy::AskBeforeWrite, WorkspaceWritePolicy::AskBeforeWrite) => {
            WorkspaceWritePolicy::AskBeforeWrite
        }
        _ => WorkspaceWritePolicy::DenyAll,
    }
}

impl HeadService {
    pub(crate) async fn compute_parent_write_ceiling_in_tx<C: deadpool_postgres::GenericClient>(
        &self,
        client: &C,
        task_id: Option<TaskId>,
    ) -> Result<Option<WorkspaceWritePolicy>, DennettError> {
        let Some(start_tid) = task_id else {
            return Ok(None);
        };
        let mut ceiling: Option<WorkspaceWritePolicy> = None;
        let mut cursor_parent = fetch_task(client, start_tid, self.pg_target())
            .await
            .ok()
            .and_then(|t| t.parent_task_id);
        let mut depth = 0_u8;
        while let Some(parent_tid) = cursor_parent {
            depth += 1;
            if depth > 16 {
                return Ok(Some(WorkspaceWritePolicy::DenyAll));
            }
            let parent_task = fetch_task(client, parent_tid, self.pg_target()).await?;
            let parent_agent = fetch_agent(client, parent_task.agent_id, self.pg_target()).await?;
            let parent_policy = parent_agent.current_config.workspace_write_policy;
            ceiling = Some(match ceiling {
                Some(existing) => intersect_write_ceilings(existing, parent_policy),
                None => parent_policy,
            });
            cursor_parent = parent_task.parent_task_id;
        }
        Ok(ceiling)
    }

    pub(crate) async fn load_live_policy_inputs(
        &self,
        project_id: Option<ProjectId>,
        task_id: Option<TaskId>,
    ) -> Result<LivePolicyInputs, DennettError> {
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let rules = fetch_policy_rules(&conn, self.pg_target()).await?;
        let Some(pid) = project_id else {
            return Ok(LivePolicyInputs {
                rules,
                grants: Vec::new(),
                project_lifecycle_state: ProjectLifecycleState::Detached,
                workspace_bound: false,
                parent_write_ceiling: Some(WorkspaceWritePolicy::DenyAll),
            });
        };
        let project = fetch_project(&conn, pid, self.pg_target()).await?;
        let grants = fetch_active_project_grants(&conn, pid, self.pg_target()).await?;
        let parent_write_ceiling = self
            .compute_parent_write_ceiling_in_tx(&conn, task_id)
            .await?;
        Ok(LivePolicyInputs {
            rules,
            grants,
            project_lifecycle_state: project.lifecycle_state,
            workspace_bound: project.workspace_path.is_some(),
            parent_write_ceiling,
        })
    }

    pub(crate) async fn try_consume_capability_grant(
        &self,
        grant_id: CapabilityGrantId,
    ) -> Result<bool, DennettError> {
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let row = conn
            .query_opt(
                "UPDATE capability_grants
                 SET uses_consumed = uses_consumed + 1
                 WHERE grant_id = $1
                   AND revoked_at IS NULL
                   AND (expires_at IS NULL OR expires_at > now())
                   AND (max_uses IS NULL OR uses_consumed < max_uses)
                 RETURNING grant_id",
                &[&grant_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        Ok(row.is_some())
    }

    #[expect(
        clippy::too_many_arguments,
        clippy::too_many_lines,
        reason = "persists permission_requests + inbox_cards + task waiting_approval + control_events atomically before registering waiter"
    )]
    pub(crate) async fn create_pending_permission_request(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
        run_id: dennett_contracts::RunId,
        task_id: Option<TaskId>,
        agent_id: dennett_contracts::AgentId,
        tool_call_id: &str,
        tool_name: &str,
        tool_kind: &str,
        target_summary: &str,
        normalized_targets: &[String],
        parameters_sha256: &dennett_contracts::Sha256Hex,
        preview_snippet: Option<&str>,
        reason: &str,
        timeout: Duration,
        correlation_id: &CorrelationId,
        waiter_tx: oneshot::Sender<ApprovalResolutionSignal>,
    ) -> Result<PermissionRequestId, DennettError> {
        let request_id = PermissionRequestId::new();
        let card_id = InboxCardId::new();
        let expires_at_ms = TimestampMs::now()
            .as_millis()
            .saturating_add(i64::try_from(timeout.as_millis()).unwrap_or(300_000));
        let expires_at_secs = ms_to_pg_epoch_secs(expires_at_ms);
        let targets_json =
            serde_json::to_value(normalized_targets).map_err(|err| DennettError::Internal {
                detail: format!("failed serializing normalized_targets: {err}"),
            })?;

        // Register waiter BEFORE committing to PostgreSQL so an immediate concurrent resolver
        // observing the committed row ALWAYS finds the live waiter channel!
        self.register_approval_waiter(request_id, waiter_tx).await;

        let mut conn = match self.get_conn(DEFAULT_PG_OP_TIMEOUT).await {
            Ok(c) => c,
            Err(err) => {
                self.remove_approval_waiter(request_id).await;
                return Err(err);
            }
        };
        let tx = match conn.transaction().await {
            Ok(t) => t,
            Err(err) => {
                self.remove_approval_waiter(request_id).await;
                return Err(map_pg_err(self.pg_target(), &err));
            }
        };

        let insert_res: Result<(), DennettError> = async {
            if let Some(tid) = task_id {
                let wait_msg = truncate_utf8(
                    &format!("Ожидает разрешения на {tool_name} ({target_summary})"),
                    2000,
                )
                .to_owned();
                tx.execute(
                    "UPDATE tasks
                     SET status = 'waiting_approval',
                         wait_detail = $2,
                         updated_at = now()
                     WHERE task_id = $1 AND status NOT IN ('cancelled', 'unknown_external')",
                    &[&tid.as_uuid(), &wait_msg],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;
            }

            tx.execute(
                "INSERT INTO permission_requests (
                    request_id, project_id, session_id, run_id, task_id, agent_id,
                    tool_call_id, tool_name, tool_kind, target_summary, normalized_targets,
                    parameters_sha256, preview_snippet, reason, status, expires_at, correlation_id
                 ) VALUES (
                    $1, $2, $3, $4, $5, $6,
                    $7, $8, $9, $10, $11,
                    $12, $13, $14, 'pending', to_timestamp($15), $16
                 )",
                &[
                    &request_id.as_uuid(),
                    &project_id.as_uuid(),
                    &session_id.as_uuid(),
                    &run_id.as_uuid(),
                    &task_id.map(TaskId::as_uuid),
                    &agent_id.as_uuid(),
                    &truncate_utf8(tool_call_id, 256),
                    &truncate_utf8(tool_name, 256),
                    &truncate_utf8(tool_kind, 64),
                    &truncate_utf8(target_summary, 1024),
                    &targets_json,
                    &parameters_sha256.as_str(),
                    &preview_snippet.map(|s| truncate_utf8(s, 2000)),
                    &truncate_utf8(reason, 1024),
                    &expires_at_secs,
                    &correlation_id.as_str(),
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

            let subject =
                truncate_utf8(&format!("Разрешить {tool_name} ({target_summary})"), 500).to_owned();
            let detail = truncate_utf8(reason, 4000).to_owned();
            tx.execute(
                "INSERT INTO inbox_cards (
                    card_id, project_id, session_id, task_id, run_id, agent_id,
                    kind, status, urgency, subject, detail, permission_request_id
                 ) VALUES (
                    $1, $2, $3, $4, $5, $6,
                    'permission_approval', 'open', 'high', $7, $8, $9
                 )",
                &[
                    &card_id.as_uuid(),
                    &project_id.as_uuid(),
                    &session_id.as_uuid(),
                    &task_id.map(TaskId::as_uuid),
                    &run_id.as_uuid(),
                    &agent_id.as_uuid(),
                    &subject,
                    &detail,
                    &request_id.as_uuid(),
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

            let entity_key = task_id.map_or_else(
                || format!("session:{session_id}"),
                |tid| format!("task:{tid}"),
            );
            let _ = append_control_event_in_tx(
                &tx,
                self.pg_target(),
                ControlEventAppend {
                    entity_key: &entity_key,
                    event_type: "permission.requested",
                    project_id: Some(project_id),
                    session_id: Some(session_id),
                    task_id,
                    correlation_id,
                    payload: json!({
                        "request_id": request_id,
                        "card_id": card_id,
                        "tool_name": tool_name,
                        "target_summary": target_summary,
                        "normalized_targets": normalized_targets,
                    }),
                },
            )
            .await?;

            tx.commit()
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;
            Ok(())
        }
        .await;

        if let Err(err) = insert_res {
            self.remove_approval_waiter(request_id).await;
            return Err(err);
        }

        Ok(request_id)
    }

    pub(crate) async fn expire_pending_permission_request(
        &self,
        request_id: PermissionRequestId,
        inbox_status: &str,
        summary: &str,
        correlation_id: &CorrelationId,
    ) -> Result<(), DennettError> {
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let sql = format!(
            "UPDATE permission_requests
             SET status = 'expired',
                 revision = revision + 1,
                 resolved_at = now()
             WHERE request_id = $1 AND status = 'pending'
             RETURNING {PERMISSION_REQUEST_SELECT_COLS}"
        );
        let Some(row) = tx
            .query_opt(&sql, &[&request_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?
        else {
            return Ok(());
        };
        let perm = decode_permission_request_row(&row)?;

        let safe_summary = truncate_utf8(&self.redact(summary), 2000).to_owned();
        tx.execute(
            "UPDATE inbox_cards
             SET status = $2,
                 resolution_summary = $3,
                 revision = revision + 1,
                 resolved_at = now(),
                 updated_at = now()
             WHERE permission_request_id = $1 AND status = 'open'",
            &[&request_id.as_uuid(), &inbox_status, &safe_summary],
        )
        .await
        .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let entity_key = perm.task_id.map_or_else(
            || format!("session:{}", perm.session_id),
            |tid| format!("task:{tid}"),
        );
        let _ = append_control_event_in_tx(
            &tx,
            self.pg_target(),
            ControlEventAppend {
                entity_key: &entity_key,
                event_type: "permission.expired",
                project_id: Some(perm.project_id),
                session_id: Some(perm.session_id),
                task_id: perm.task_id,
                correlation_id,
                payload: json!({
                    "request_id": request_id,
                    "reason": safe_summary,
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        Ok(())
    }

    /// Expires/withdraws every still-`pending` permission request and linked `open` approval inbox
    /// card belonging to `run_id` inside the caller's PostgreSQL transaction (H2-01).
    ///
    /// Strictly buffers journal events into `pending_events` instead of locking `control:journal`
    /// mid-transaction, preserving the V15 deadlock-free lock hierarchy.
    pub(crate) async fn expire_pending_approvals_for_run_in_tx<
        C: deadpool_postgres::GenericClient,
    >(
        &self,
        tx: &C,
        run_id: dennett_contracts::RunId,
        inbox_status: &str,
        summary: &str,
        pending_events: &mut Vec<PendingControlEvent>,
    ) -> Result<Vec<PermissionRequestId>, DennettError> {
        let safe_summary = truncate_utf8(&self.redact(summary), 2000).to_owned();
        let sql = format!(
            "UPDATE permission_requests
             SET status = 'expired',
                 revision = revision + 1,
                 resolved_at = now()
             WHERE run_id = $1 AND status = 'pending'
             RETURNING {PERMISSION_REQUEST_SELECT_COLS}"
        );
        let rows = tx
            .query(&sql, &[&run_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let mut expired_ids = Vec::with_capacity(rows.len());
        let event_type = if inbox_status == "withdrawn" {
            "permission.withdrawn"
        } else {
            "permission.expired"
        };
        for row in &rows {
            let perm = decode_permission_request_row(row)?;
            tx.execute(
                "UPDATE inbox_cards
                 SET status = $2,
                     resolution_summary = $3,
                     revision = revision + 1,
                     resolved_at = now(),
                     updated_at = now()
                 WHERE permission_request_id = $1 AND status = 'open'",
                &[&perm.request_id.as_uuid(), &inbox_status, &safe_summary],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

            let entity_key = perm.task_id.map_or_else(
                || format!("session:{}", perm.session_id),
                |tid| format!("task:{tid}"),
            );
            pending_events.push(PendingControlEvent {
                entity_key,
                event_type,
                project_id: Some(perm.project_id),
                session_id: Some(perm.session_id),
                task_id: perm.task_id,
                payload: json!({
                    "request_id": perm.request_id,
                    "run_id": run_id,
                    "reason": safe_summary,
                }),
            });
            expired_ids.push(perm.request_id);
        }

        // Belt-and-suspenders: close any open `permission_approval` inbox card still tied to this run.
        tx.execute(
            "UPDATE inbox_cards
             SET status = $2,
                 resolution_summary = $3,
                 revision = revision + 1,
                 resolved_at = now(),
                 updated_at = now()
             WHERE run_id = $1
               AND kind = 'permission_approval'
               AND status = 'open'",
            &[&run_id.as_uuid(), &inbox_status, &safe_summary],
        )
        .await
        .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        Ok(expired_ids)
    }

    pub(crate) async fn wake_expired_approval_waiters(
        &self,
        request_ids: &[PermissionRequestId],
        session_id: SessionId,
        reason: &str,
    ) {
        let safe_reason = truncate_utf8(&self.redact(reason), 1000).to_owned();
        for &request_id in request_ids {
            self.wake_approval_waiter(
                request_id,
                ApprovalResolutionSignal {
                    outcome: PolicyEvaluationOutcome {
                        effect: PolicyEffect::Deny,
                        matched_rule_id: None,
                        reason: safe_reason.clone(),
                        scope: format!("session:{session_id}"),
                    },
                    decision_record: None,
                },
            )
            .await;
        }
    }

    /// Cancels all pending interactive permission requests for a session or task during `Stop`,
    /// immediately waking any waiting runtime turn with `Deny`.
    pub(crate) async fn cancel_pending_permissions_for_scope(
        &self,
        session_id: Option<SessionId>,
        task_id: Option<TaskId>,
        reason: &str,
        correlation_id: &CorrelationId,
    ) -> Result<usize, DennettError> {
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let sql = format!(
            "UPDATE permission_requests
             SET status = 'expired',
                 revision = revision + 1,
                 resolved_at = now()
             WHERE status = 'pending'
               AND (($1::uuid IS NOT NULL AND task_id = $1)
                 OR ($1::uuid IS NULL AND $2::uuid IS NOT NULL AND session_id = $2))
             RETURNING {PERMISSION_REQUEST_SELECT_COLS}"
        );
        let rows = tx
            .query(
                &sql,
                &[
                    &task_id.map(TaskId::as_uuid),
                    &session_id.map(SessionId::as_uuid),
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let safe_reason = truncate_utf8(&self.redact(reason), 2000).to_owned();
        let mut expired = Vec::with_capacity(rows.len());
        let mut pending_events = Vec::with_capacity(rows.len());
        for row in &rows {
            let perm = decode_permission_request_row(row)?;
            tx.execute(
                "UPDATE inbox_cards
                 SET status = 'withdrawn',
                     resolution_summary = $2,
                     revision = revision + 1,
                     resolved_at = now(),
                     updated_at = now()
                 WHERE permission_request_id = $1 AND status = 'open'",
                &[&perm.request_id.as_uuid(), &safe_reason],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

            let entity_key = perm.task_id.map_or_else(
                || format!("session:{}", perm.session_id),
                |tid| format!("task:{tid}"),
            );
            pending_events.push(PendingControlEvent {
                entity_key,
                event_type: "permission.withdrawn",
                project_id: Some(perm.project_id),
                session_id: Some(perm.session_id),
                task_id: perm.task_id,
                payload: json!({
                    "request_id": perm.request_id,
                    "reason": safe_reason,
                }),
            });
            expired.push(perm);
        }

        let _ = append_control_events_batch_in_tx(
            &tx,
            self.pg_target(),
            correlation_id,
            &pending_events,
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        for perm in &expired {
            self.wake_approval_waiter(
                perm.request_id,
                ApprovalResolutionSignal {
                    outcome: PolicyEvaluationOutcome {
                        effect: PolicyEffect::Deny,
                        matched_rule_id: None,
                        reason: safe_reason.clone(),
                        scope: format!("session:{}", perm.session_id),
                    },
                    decision_record: None,
                },
            )
            .await;
        }
        Ok(expired.len())
    }

    /// Lists interactive permission requests (`GET /v1/permissions/requests`).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] or [`DennettError::DatabaseUnavailable`].
    pub async fn list_permission_requests(
        &self,
        status_filter: Option<&str>,
        session_id: Option<SessionId>,
    ) -> Result<ListPermissionRequestsResponse, DennettError> {
        let parsed_status = status_filter
            .map(str::trim)
            .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("all"))
            .map(PermissionRequestStatus::from_str)
            .transpose()?;
        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let sql = format!(
            "SELECT {PERMISSION_REQUEST_SELECT_COLS}
             FROM permission_requests
             WHERE EXISTS (
                 SELECT 1 FROM projects p
                 WHERE p.project_id = permission_requests.project_id
                   AND p.lifecycle_state <> 'deleted_record'
             )
               AND ($1::text IS NULL OR status = $1)
               AND ($2::uuid IS NULL OR session_id = $2)
             ORDER BY created_at ASC"
        );
        let status_str = parsed_status.map(|s| s.as_str().to_owned());
        let rows = conn
            .query(&sql, &[&status_str, &session_id.map(SessionId::as_uuid)])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let requests = rows
            .iter()
            .map(decode_permission_request_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ListPermissionRequestsResponse { requests })
    }

    /// Resolves a pending interactive permission request (`POST /v1/permissions/requests/{id}/resolve`
    /// and `/v1/approvals/{id}/resolve`), re-verifying every granted write through [`PolicyEngine`]
    /// outside the model, optionally issuing a revocable [`CapabilityGrantRecord`], and waking the
    /// suspended runtime callback with strictly non-escalating `allow_once` or `reject_once`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::NotFound`],
    /// [`DennettError::Conflict`] (already resolved, stale revision, or cancelled run/task),
    /// or [`DennettError::DatabaseUnavailable`].
    pub async fn resolve_permission_request(
        &self,
        request_id: PermissionRequestId,
        req: ResolvePermissionRequest,
        correlation_id: &CorrelationId,
    ) -> Result<ResolvePermissionResponse, DennettError> {
        self.resolve_permission_request_with_card_guard(request_id, req, None, correlation_id)
            .await
    }

    #[expect(
        clippy::too_many_lines,
        reason = "permission resolution performs optimistic locking, PolicyEngine re-verification, grant issuance, atomic inbox card update, audit decision persistence, and waiter wakeup in one atomic unit"
    )]
    async fn resolve_permission_request_with_card_guard(
        &self,
        request_id: PermissionRequestId,
        req: ResolvePermissionRequest,
        card_revision_guard: Option<(InboxCardId, i64)>,
        correlation_id: &CorrelationId,
    ) -> Result<ResolvePermissionResponse, DennettError> {
        req.validate()?;
        let _ = self.ensure_migrated().await?;

        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let lock_sql = format!(
            "SELECT {PERMISSION_REQUEST_SELECT_COLS}
             FROM permission_requests
             WHERE request_id = $1
             FOR UPDATE"
        );
        let Some(row) = tx
            .query_opt(&lock_sql, &[&request_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?
        else {
            return Err(DennettError::NotFound {
                resource: String::from("permission_request"),
                id: request_id.to_string(),
            });
        };
        let perm = decode_permission_request_row(&row)?;

        if let Some(expected_rev) = req.expected_revision
            && perm.revision != expected_rev
        {
            return Err(DennettError::Conflict {
                resource: String::from("permission_request"),
                expected_revision: Some(expected_rev),
                actual_revision: Some(perm.revision),
                reason: String::from("permission request revision mismatch (already_resolved)"),
            });
        }
        if perm.status != PermissionRequestStatus::Pending {
            return Err(DennettError::Conflict {
                resource: String::from("permission_request"),
                expected_revision: req.expected_revision,
                actual_revision: Some(perm.revision),
                reason: format!(
                    "permission request already resolved with status `{}`",
                    perm.status
                ),
            });
        }

        // Verify the owning run/task has NOT been cancelled!
        // For explicit `Deny`, allow idempotent dismissal even if the run already terminated (H2-01)
        // so the user is NEVER trapped by an uncloseable approval card.
        let wants_allow = !matches!(req.decision, PermissionResolutionDecision::Deny);
        if wants_allow {
            let _ = ensure_project_active_in_tx(
                &tx,
                perm.project_id,
                "approve permission request",
                self.pg_target(),
            )
            .await?;
        }
        let run_status_raw: String = tx
            .query_one(
                "SELECT status FROM runs WHERE run_id = $1",
                &[&perm.run_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?
            .get(0);
        let run_status = RunStatus::from_str(&run_status_raw)?;
        if run_status != RunStatus::Running && wants_allow {
            return Err(DennettError::Conflict {
                resource: String::from("permission_request"),
                expected_revision: req.expected_revision,
                actual_revision: Some(perm.revision),
                reason: format!(
                    "cannot approve permission request for non-running run (status={run_status})"
                ),
            });
        }
        if let Some(tid) = perm.task_id {
            let task = fetch_task(&tx, tid, self.pg_target()).await?;
            if wants_allow
                && (task.status == TaskStatus::Cancelled
                    || self.is_task_cancel_signalled(tid).await)
            {
                return Err(DennettError::Conflict {
                    resource: String::from("permission_request"),
                    expected_revision: req.expected_revision,
                    actual_revision: Some(perm.revision),
                    reason: String::from(
                        "cannot approve permission request after task Stop/cancellation",
                    ),
                });
            }
        }

        let project = fetch_project(&tx, perm.project_id, self.pg_target()).await?;
        let agent = fetch_agent(&tx, perm.agent_id, self.pg_target()).await?;
        let rules = fetch_policy_rules(&tx, self.pg_target()).await?;
        let parent_write_ceiling = self
            .compute_parent_write_ceiling_in_tx(&tx, perm.task_id)
            .await?;

        // Re-verify through PolicyEngine before allowing execution!
        let (effective_effect, final_status, effective_reason, effective_scope) = if wants_allow {
            let reverify_err: Option<String> = if project.lifecycle_state
                != ProjectLifecycleState::Active
            {
                Some(format!(
                    "denied on re-verification: project lifecycle_state is `{}`",
                    project.lifecycle_state
                ))
            } else if let Some(ref ws_path) = project.workspace_path {
                let ws_root = PathBuf::from(ws_path);
                let mut err_msg = None;
                for rel in &perm.normalized_targets {
                    if let Err(boundary_err) = resolve_within_workspace(&ws_root, rel) {
                        err_msg = Some(format!(
                            "denied on re-verification (workspace containment): {boundary_err}"
                        ));
                        break;
                    }
                }
                if err_msg.is_none() {
                    let reverify_engine = PolicyEngine::new(PolicyContext {
                        project_id: Some(project.project_id),
                        project_lifecycle_state: project.lifecycle_state,
                        workspace_bound: true,
                        agent_id: agent.agent_id,
                        session_id: perm.session_id,
                        task_id: perm.task_id,
                        workspace_root: ws_root,
                        workspace_write_policy: agent.current_config.workspace_write_policy.clone(),
                        parent_write_ceiling,
                        allowed_memory_scopes: agent.current_config.allowed_memory_scopes.clone(),
                        rules,
                    });
                    let synthetic_req = PolicyEvaluationRequest {
                        session_id: perm.session_id,
                        run_id: perm.run_id,
                        tool_call_id: perm.tool_call_id.clone(),
                        tool_name: perm.tool_name.clone(),
                        tool_kind: perm.tool_kind.clone(),
                        title: format!("{} {}", perm.tool_name, perm.target_summary),
                        target: perm.normalized_targets.first().cloned(),
                        locations: perm.normalized_targets.clone(),
                        raw_input: json!({}),
                    };
                    if let PolicyEvaluationDecision::Immediate(o) = reverify_engine
                        .evaluate_with_grants(&synthetic_req, &[], TimestampMs::now())
                        && o.effect == PolicyEffect::Deny
                    {
                        err_msg = Some(format!(
                            "denied on PolicyEngine re-verification: {}",
                            o.reason
                        ));
                    }
                }
                err_msg
            } else {
                Some(String::from(
                    "denied on re-verification: project workspace folder was detached",
                ))
            };

            if let Some(fail_reason) = reverify_err {
                (
                    PolicyEffect::Deny,
                    PermissionRequestStatus::Denied,
                    fail_reason,
                    format!("project:{}", perm.project_id),
                )
            } else {
                let status = match req.decision {
                    PermissionResolutionDecision::ApproveOnce => {
                        PermissionRequestStatus::ApprovedOnce
                    }
                    PermissionResolutionDecision::ApproveProject
                    | PermissionResolutionDecision::ApproveTask => {
                        PermissionRequestStatus::ApprovedScope
                    }
                    PermissionResolutionDecision::Deny | _ => PermissionRequestStatus::Denied,
                };
                let scope_label = match req.decision {
                    PermissionResolutionDecision::ApproveTask => perm.task_id.map_or_else(
                        || format!("project:{}", perm.project_id),
                        |tid| format!("task:{tid}"),
                    ),
                    _ => format!("project:{}", perm.project_id),
                };
                let user_reason = req
                    .reason
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                let rationale = user_reason.map_or_else(
                    || format!("owner approved `{}` via `{}`", perm.tool_name, req.decision),
                    |r| format!("owner approved via `{}`: {r}", req.decision),
                );
                (PolicyEffect::Allow, status, rationale, scope_label)
            }
        } else {
            let user_reason = req
                .reason
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let rationale = user_reason.map_or_else(
                || {
                    format!(
                        "owner denied `{}` ({})",
                        perm.tool_name, perm.target_summary
                    )
                },
                |r| format!("owner denied: {r}"),
            );
            (
                PolicyEffect::Deny,
                PermissionRequestStatus::Denied,
                rationale,
                format!("project:{}", perm.project_id),
            )
        };

        // Issue revocable CapabilityGrant when ApproveProject / ApproveTask succeeded re-verification!
        let mut issued_grant: Option<CapabilityGrantRecord> = None;
        if effective_effect == PolicyEffect::Allow
            && matches!(
                req.decision,
                PermissionResolutionDecision::ApproveProject
                    | PermissionResolutionDecision::ApproveTask
            )
        {
            let grant_id = CapabilityGrantId::new();
            let grant_task_id = if req.decision == PermissionResolutionDecision::ApproveTask {
                perm.task_id
            } else {
                None
            };
            let grant_scope = match grant_task_id {
                Some(tid) => format!("task:{tid}"),
                None => format!("project:{}", perm.project_id),
            };
            let tool_pattern = if is_workspace_write_tool(&perm.tool_name, &perm.tool_kind) {
                String::from("workspace_write")
            } else {
                perm.tool_name.clone()
            };
            let expires_at_secs: Option<f64> = req.expires_in_secs.map(|secs| {
                let target_ms = TimestampMs::now()
                    .as_millis()
                    .saturating_add(i64::try_from(secs.saturating_mul(1_000)).unwrap_or(i64::MAX));
                ms_to_pg_epoch_secs(target_ms)
            });
            let safe_grant_reason = truncate_utf8(&self.redact(&effective_reason), 1000).to_owned();
            let empty_prefixes = json!([]);
            let grant_sql = format!(
                "INSERT INTO capability_grants (
                    grant_id, scope, project_id, agent_id, session_id, task_id,
                    tool_pattern, target_prefixes, max_uses, uses_consumed, reason, expires_at
                 ) VALUES (
                    $1, $2, $3, NULL, NULL, $4,
                    $5, $6, $7, 1, $8,
                    CASE WHEN $9::double precision IS NULL THEN NULL ELSE to_timestamp($9) END
                 )
                 RETURNING {CAPABILITY_GRANT_SELECT_COLS}"
            );
            let grant_row = tx
                .query_one(
                    &grant_sql,
                    &[
                        &grant_id.as_uuid(),
                        &grant_scope,
                        &perm.project_id.as_uuid(),
                        &grant_task_id.map(TaskId::as_uuid),
                        &tool_pattern,
                        &empty_prefixes,
                        &req.max_uses,
                        &safe_grant_reason,
                        &expires_at_secs,
                    ],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;
            issued_grant = Some(decode_capability_grant_row(&grant_row)?);
        }

        let safe_reason = truncate_utf8(&self.redact(&effective_reason), 1000).to_owned();
        let decision_record = PermissionDecisionRecord {
            decision_id: PermissionDecisionId::new(),
            session_id: perm.session_id,
            run_id: Some(perm.run_id),
            tool_call_id: perm.tool_call_id.clone(),
            tool_name: perm.tool_name.clone(),
            target_pattern: Some(self.redact(&perm.target_summary)),
            scope: effective_scope.clone(),
            effect: effective_effect,
            matched_rule_id: None,
            reason: safe_reason.clone(),
            evaluated_outside_model: true,
            correlation_id: correlation_id.clone(),
            created_at: TimestampMs::now(),
        };
        insert_permission_decision(&tx, self.pg_target(), &decision_record).await?;

        let update_perm_sql = format!(
            "UPDATE permission_requests
             SET status = $2,
                 issued_grant_id = $3,
                 decision_id = $4,
                 revision = revision + 1,
                 resolved_at = now()
             WHERE request_id = $1
             RETURNING {PERMISSION_REQUEST_SELECT_COLS}"
        );
        let updated_perm_row = tx
            .query_one(
                &update_perm_sql,
                &[
                    &request_id.as_uuid(),
                    &final_status.as_str(),
                    &issued_grant.as_ref().map(|g| g.grant_id.as_uuid()),
                    &decision_record.decision_id.as_uuid(),
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let updated_perm = decode_permission_request_row(&updated_perm_row)?;

        if let Some((expected_card_id, expected_card_rev)) = card_revision_guard {
            let card_rows = tx
                .execute(
                    "UPDATE inbox_cards
                     SET status = 'resolved',
                         resolution_summary = $3,
                         revision = revision + 1,
                         resolved_at = now(),
                         updated_at = now()
                     WHERE card_id = $1
                       AND permission_request_id = $2
                       AND status = 'open'
                       AND revision = $4",
                    &[
                        &expected_card_id.as_uuid(),
                        &request_id.as_uuid(),
                        &safe_reason,
                        &expected_card_rev,
                    ],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;
            if card_rows == 0 {
                return Err(DennettError::Conflict {
                    resource: String::from("inbox_card"),
                    expected_revision: Some(expected_card_rev),
                    actual_revision: None,
                    reason: String::from("inbox card revision mismatch (already_resolved)"),
                });
            }
        } else {
            tx.execute(
                "UPDATE inbox_cards
                 SET status = 'resolved',
                     resolution_summary = $2,
                     revision = revision + 1,
                     resolved_at = now(),
                     updated_at = now()
                 WHERE permission_request_id = $1 AND status = 'open'",
                &[&request_id.as_uuid(), &safe_reason],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        }

        if let Some(tid) = perm.task_id {
            if run_status == RunStatus::Running {
                tx.execute(
                    "UPDATE tasks
                     SET status = 'running',
                         wait_detail = NULL,
                         updated_at = now()
                     WHERE task_id = $1 AND status = 'waiting_approval'",
                    &[&tid.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;
            } else {
                tx.execute(
                    "UPDATE tasks
                     SET status = 'failed',
                         wait_detail = NULL,
                         active_run_id = NULL,
                         finished_at = COALESCE(finished_at, now()),
                         updated_at = now()
                     WHERE task_id = $1 AND status = 'waiting_approval'",
                    &[&tid.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;
            }
        }

        let _ = append_audit_event(
            &tx,
            self.pg_target(),
            correlation_id,
            Some(perm.session_id),
            Some(perm.run_id),
            "policy_engine",
            if effective_effect == PolicyEffect::Allow {
                "permission_approved_interactively"
            } else {
                "permission_denied_interactively"
            },
            &format!(
                "permission request {request_id} for `{}` -> {} ({safe_reason})",
                perm.tool_name, effective_effect
            ),
        )
        .await?;

        let entity_key = perm.task_id.map_or_else(
            || format!("session:{}", perm.session_id),
            |tid| format!("task:{tid}"),
        );
        let _ = append_control_event_in_tx(
            &tx,
            self.pg_target(),
            ControlEventAppend {
                entity_key: &entity_key,
                event_type: "permission.resolved",
                project_id: Some(perm.project_id),
                session_id: Some(perm.session_id),
                task_id: perm.task_id,
                correlation_id,
                payload: json!({
                    "request_id": request_id,
                    "status": updated_perm.status,
                    "effect": effective_effect,
                    "issued_grant_id": issued_grant.as_ref().map(|g| g.grant_id),
                    "reason": safe_reason,
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        // Wake the suspended runtime `PolicyEvaluator` callback with strictly `Allow`/`Deny`
        // (runtime transport translates `Allow` strictly to `allow_once`, NEVER `allow_always`!).
        self.wake_approval_waiter(
            request_id,
            ApprovalResolutionSignal {
                outcome: PolicyEvaluationOutcome {
                    effect: effective_effect,
                    matched_rule_id: None,
                    reason: safe_reason,
                    scope: effective_scope,
                },
                decision_record: Some(decision_record.clone()),
            },
        )
        .await;

        Ok(ResolvePermissionResponse {
            request: updated_perm,
            decision: decision_record,
            issued_grant,
        })
    }

    /// Lists revocable capability grants (`GET /v1/grants`).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::DatabaseUnavailable`] on PG failure.
    pub async fn list_capability_grants(
        &self,
        project_id: Option<ProjectId>,
        include_revoked: bool,
    ) -> Result<ListCapabilityGrantsResponse, DennettError> {
        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let sql = format!(
            "SELECT {CAPABILITY_GRANT_SELECT_COLS}
             FROM capability_grants
             WHERE EXISTS (
                 SELECT 1 FROM projects p
                 WHERE p.project_id = capability_grants.project_id
                   AND p.lifecycle_state <> 'deleted_record'
             )
               AND ($1::uuid IS NULL OR project_id = $1)
               AND ($2::boolean OR revoked_at IS NULL)
             ORDER BY created_at DESC"
        );
        let rows = conn
            .query(
                &sql,
                &[&project_id.map(ProjectId::as_uuid), &include_revoked],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let grants = rows
            .iter()
            .map(decode_capability_grant_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ListCapabilityGrantsResponse { grants })
    }

    /// Immediately revokes an active [`CapabilityGrantRecord`] (`POST /v1/grants/{grant_id}/revoke`).
    ///
    /// Because `agy acp` never receives `allow_always`, the very next tool call re-queries
    /// `capability_grants` and immediately requires fresh owner approval!
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`] or [`DennettError::DatabaseUnavailable`].
    pub async fn revoke_capability_grant(
        &self,
        grant_id: CapabilityGrantId,
        req: RevokeGrantRequest,
        correlation_id: &CorrelationId,
    ) -> Result<CapabilityGrantRecord, DennettError> {
        let _ = self.ensure_migrated().await?;
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let reason = req
            .reason
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("revoked by owner");
        let safe_reason = truncate_utf8(&self.redact(reason), 500).to_owned();

        let sql = format!(
            "UPDATE capability_grants
             SET revoked_at = COALESCE(revoked_at, now()),
                 revoked_reason = COALESCE(revoked_reason, $2)
             WHERE grant_id = $1
             RETURNING {CAPABILITY_GRANT_SELECT_COLS}"
        );
        let Some(row) = tx
            .query_opt(&sql, &[&grant_id.as_uuid(), &safe_reason])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?
        else {
            return Err(DennettError::NotFound {
                resource: String::from("capability_grant"),
                id: grant_id.to_string(),
            });
        };
        let grant = decode_capability_grant_row(&row)?;

        let _ = append_audit_event(
            &tx,
            self.pg_target(),
            correlation_id,
            grant.session_id,
            None,
            "policy_engine",
            "capability_grant_revoked",
            &format!("revoked capability grant {grant_id} ({safe_reason})"),
        )
        .await?;

        let entity_key = format!("project:{}", grant.project_id);
        let _ = append_control_event_in_tx(
            &tx,
            self.pg_target(),
            ControlEventAppend {
                entity_key: &entity_key,
                event_type: "grant.revoked",
                project_id: Some(grant.project_id),
                session_id: grant.session_id,
                task_id: grant.task_id,
                correlation_id,
                payload: json!({
                    "grant_id": grant.grant_id,
                    "scope": grant.scope,
                    "tool_pattern": grant.tool_pattern,
                    "revoked_reason": safe_reason,
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        Ok(grant)
    }

    /// Lists Action Inbox cards (`GET /v1/inbox`) plus exact open unresolved counter.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] or [`DennettError::DatabaseUnavailable`].
    pub async fn list_inbox_cards(
        &self,
        status_filter: Option<&str>,
        project_id: Option<ProjectId>,
    ) -> Result<ListInboxResponse, DennettError> {
        let trimmed_filter = status_filter.map(str::trim).filter(|s| !s.is_empty());
        let parsed_status = match trimmed_filter {
            None | Some("open") => Some(InboxCardStatus::Open),
            Some("all") => None,
            Some(other) => Some(InboxCardStatus::from_str(other)?),
        };
        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let sql = format!(
            "SELECT {INBOX_CARD_SELECT_COLS}
             FROM inbox_cards
             WHERE (
                 (kind = 'unknown_effect' AND status = 'open')
                 OR EXISTS (
                     SELECT 1 FROM projects p
                     WHERE p.project_id = inbox_cards.project_id
                       AND p.lifecycle_state <> 'deleted_record'
                 )
             )
               AND ($1::text IS NULL OR status = $1)
               AND ($2::uuid IS NULL OR project_id = $2)
             ORDER BY created_at DESC"
        );
        let status_str = parsed_status.map(|s| s.as_str().to_owned());
        let rows = conn
            .query(&sql, &[&status_str, &project_id.map(ProjectId::as_uuid)])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let cards = rows
            .iter()
            .map(decode_inbox_card_row)
            .collect::<Result<Vec<_>, _>>()?;

        let open_count: i64 = conn
            .query_one(
                "SELECT COUNT(*)::bigint FROM inbox_cards
                 WHERE status = 'open'
                   AND (
                       kind = 'unknown_effect'
                       OR EXISTS (
                           SELECT 1 FROM projects p
                           WHERE p.project_id = inbox_cards.project_id
                             AND p.lifecycle_state <> 'deleted_record'
                       )
                   )
                   AND ($1::uuid IS NULL OR project_id = $1)",
                &[&project_id.map(ProjectId::as_uuid)],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?
            .get(0);

        Ok(ListInboxResponse {
            cards,
            unresolved_count: usize::try_from(open_count).unwrap_or(0),
        })
    }

    /// Resolves one Action Inbox card with optimistic `expected_revision` checking
    /// (`POST /v1/inbox/{card_id}/resolve`).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`], [`DennettError::InvalidInput`],
    /// [`DennettError::Conflict`], or [`DennettError::DatabaseUnavailable`].
    #[expect(
        clippy::too_many_lines,
        reason = "dispatches all five InboxCardKind resolutions cleanly with optimistic revision verification"
    )]
    pub async fn resolve_inbox_card(
        &self,
        card_id: InboxCardId,
        req: ResolveInboxCardRequest,
        correlation_id: &CorrelationId,
    ) -> Result<ResolveInboxCardResponse, DennettError> {
        let _ = self.ensure_migrated().await?;
        let card = {
            let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
            let sql =
                format!("SELECT {INBOX_CARD_SELECT_COLS} FROM inbox_cards WHERE card_id = $1");
            let Some(row) = conn
                .query_opt(&sql, &[&card_id.as_uuid()])
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?
            else {
                return Err(DennettError::NotFound {
                    resource: String::from("inbox_card"),
                    id: card_id.to_string(),
                });
            };
            decode_inbox_card_row(&row)?
        };

        if card.revision != req.expected_revision {
            return Err(DennettError::Conflict {
                resource: String::from("inbox_card"),
                expected_revision: Some(req.expected_revision),
                actual_revision: Some(card.revision),
                reason: String::from("inbox card revision mismatch (already_resolved)"),
            });
        }
        if card.status != InboxCardStatus::Open {
            return Err(DennettError::Conflict {
                resource: String::from("inbox_card"),
                expected_revision: Some(req.expected_revision),
                actual_revision: Some(card.revision),
                reason: format!("inbox card is already `{}`", card.status),
            });
        }

        match (card.kind, req.resolution) {
            // D2-ARCH-03: Unknown external effect cards MUST NEVER be dismissed via StopTask —
            // the operator must explicitly reconcile them (`reconcile_effect`).
            (InboxCardKind::UnknownEffect, InboxResolutionAction::StopTask { .. }) => {
                Err(DennettError::Conflict {
                    resource: String::from("inbox_card"),
                    expected_revision: Some(req.expected_revision),
                    actual_revision: Some(card.revision),
                    reason: String::from(
                        "cannot close unknown_effect inbox card via StopTask; use explicit ReconcileEffect instead",
                    ),
                })
            }
            (
                InboxCardKind::PermissionApproval,
                InboxResolutionAction::Permission {
                    decision,
                    reason,
                    max_uses,
                    expires_in_secs,
                },
            ) => {
                let perm_id = card
                    .permission_request_id
                    .ok_or_else(|| DennettError::Internal {
                        detail: String::from(
                            "permission_approval inbox card missing permission_request_id",
                        ),
                    })?;
                let res = self
                    .resolve_permission_request_with_card_guard(
                        perm_id,
                        ResolvePermissionRequest {
                            expected_revision: None,
                            decision,
                            reason,
                            max_uses,
                            expires_in_secs,
                        },
                        Some((card_id, req.expected_revision)),
                        correlation_id,
                    )
                    .await?;
                let updated_card = self.fetch_inbox_card(card_id).await?;
                let updated_task = match card.task_id {
                    Some(tid) => Some(self.get_task_record(tid).await?),
                    None => None,
                };
                Ok(ResolveInboxCardResponse {
                    card: updated_card,
                    task: updated_task,
                    permission_request: Some(res.request),
                    issued_grant: res.issued_grant,
                    effect: None,
                })
            }
            (
                InboxCardKind::Clarification,
                InboxResolutionAction::Clarification { answer_text },
            ) => {
                let task_id = card.task_id.ok_or_else(|| DennettError::Internal {
                    detail: String::from("clarification inbox card missing task_id"),
                })?;
                let task = self
                    .reply_to_task_with_card_guard(
                        task_id,
                        ReplyTaskRequest { answer_text },
                        Some((card_id, req.expected_revision)),
                        correlation_id,
                    )
                    .await?;
                let updated_card = self.fetch_inbox_card(card_id).await?;
                Ok(ResolveInboxCardResponse {
                    card: updated_card,
                    task: Some(task),
                    permission_request: None,
                    issued_grant: None,
                    effect: None,
                })
            }
            (
                InboxCardKind::UnknownEffect,
                InboxResolutionAction::ReconcileEffect {
                    manual_resolution,
                    reason,
                },
            ) => {
                let eff_id = card.effect_id.ok_or_else(|| DennettError::Internal {
                    detail: String::from("unknown_effect inbox card missing effect_id"),
                })?;
                let rec = self
                    .reconcile_external_effect(
                        eff_id,
                        ReconcileEffectRequest {
                            manual_resolution,
                            reason,
                        },
                        correlation_id,
                    )
                    .await?;
                let updated_card = self.fetch_inbox_card(card_id).await?;
                Ok(ResolveInboxCardResponse {
                    card: updated_card,
                    task: rec.task,
                    permission_request: None,
                    issued_grant: None,
                    effect: Some(rec.effect),
                })
            }
            (
                InboxCardKind::BudgetExceeded,
                InboxResolutionAction::IncreaseBudget {
                    additional_turns,
                    additional_tokens,
                    additional_wall_secs,
                },
            ) => {
                let task_id = card.task_id.ok_or_else(|| DennettError::Internal {
                    detail: String::from("budget_exceeded inbox card missing task_id"),
                })?;
                let updated_task = self
                    .increase_task_budget_and_resume(
                        card_id,
                        req.expected_revision,
                        task_id,
                        additional_turns,
                        additional_tokens,
                        additional_wall_secs,
                        correlation_id,
                    )
                    .await?;
                let updated_card = self.fetch_inbox_card(card_id).await?;
                Ok(ResolveInboxCardResponse {
                    card: updated_card,
                    task: Some(updated_task),
                    permission_request: None,
                    issued_grant: None,
                    effect: None,
                })
            }
            (InboxCardKind::MissedSchedule, InboxResolutionAction::MissedSchedule { run_now }) => {
                let task_id = card.task_id.ok_or_else(|| DennettError::Internal {
                    detail: String::from("missed_schedule inbox card missing task_id"),
                })?;
                let updated_task = self
                    .resolve_missed_schedule_card(
                        card_id,
                        req.expected_revision,
                        task_id,
                        run_now,
                        correlation_id,
                    )
                    .await?;
                let updated_card = self.fetch_inbox_card(card_id).await?;
                Ok(ResolveInboxCardResponse {
                    card: updated_card,
                    task: Some(updated_task),
                    permission_request: None,
                    issued_grant: None,
                    effect: None,
                })
            }
            (_, InboxResolutionAction::StopTask { reason }) => {
                let task_id = card.task_id.ok_or_else(|| DennettError::InvalidInput {
                    field: String::from("resolution"),
                    reason: String::from("cannot StopTask on an inbox card without a task_id"),
                })?;
                let safe_reason = truncate_utf8(
                    &self.redact(reason.as_deref().unwrap_or("owner stopped task from inbox")),
                    2000,
                )
                .to_owned();
                // D2-BUG-04: Atomically claim the inbox card revision FIRST before stopping task
                // so concurrent resolvers strictly receive 409 Conflict!
                {
                    let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
                    let tx = conn
                        .transaction()
                        .await
                        .map_err(|err| map_pg_err(self.pg_target(), &err))?;
                    let _ = ensure_project_active_in_tx(
                        &tx,
                        card.project_id,
                        "resolve inbox card",
                        self.pg_target(),
                    )
                    .await?;
                    let updated = tx
                        .execute(
                            "UPDATE inbox_cards
                             SET status = 'resolved',
                                 resolution_summary = $2,
                                 revision = revision + 1,
                                 resolved_at = now(),
                                 updated_at = now()
                             WHERE card_id = $1 AND status = 'open' AND revision = $3",
                            &[&card_id.as_uuid(), &safe_reason, &req.expected_revision],
                        )
                        .await
                        .map_err(|err| map_pg_err(self.pg_target(), &err))?;
                    if updated == 0 {
                        return Err(DennettError::Conflict {
                            resource: String::from("inbox_card"),
                            expected_revision: Some(req.expected_revision),
                            actual_revision: None,
                            reason: String::from("inbox card revision mismatch (already_resolved)"),
                        });
                    }
                    tx.commit()
                        .await
                        .map_err(|err| map_pg_err(self.pg_target(), &err))?;
                }
                let cancel_resp = self.cancel_task(task_id, correlation_id).await?;
                let updated_card = self.fetch_inbox_card(card_id).await?;
                Ok(ResolveInboxCardResponse {
                    card: updated_card,
                    task: Some(cancel_resp.task),
                    permission_request: None,
                    issued_grant: None,
                    effect: None,
                })
            }
            (kind, action) => Err(DennettError::InvalidInput {
                field: String::from("resolution"),
                reason: format!(
                    "incompatible resolution action `{action:?}` for inbox card kind `{kind}`"
                ),
            }),
        }
    }

    async fn fetch_inbox_card(
        &self,
        card_id: InboxCardId,
    ) -> Result<InboxCardRecord, DennettError> {
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let sql = format!("SELECT {INBOX_CARD_SELECT_COLS} FROM inbox_cards WHERE card_id = $1");
        let row = conn
            .query_one(&sql, &[&card_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        decode_inbox_card_row(&row)
    }

    async fn get_task_record(
        &self,
        task_id: TaskId,
    ) -> Result<dennett_contracts::TaskRecord, DennettError> {
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        fetch_task(&conn, task_id, self.pg_target()).await
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "atomically bumps task budget caps, resolves budget_exceeded card, and requeues execution"
    )]
    #[expect(
        clippy::too_many_lines,
        reason = "atomically resolves budget_exceeded inbox card with optimistic concurrency, updates task budget, emits journal event, and re-enqueues execution"
    )]
    async fn increase_task_budget_and_resume(
        &self,
        card_id: InboxCardId,
        expected_revision: i64,
        task_id: TaskId,
        additional_turns: Option<u32>,
        additional_tokens: Option<u64>,
        additional_wall_secs: Option<u64>,
        correlation_id: &CorrelationId,
    ) -> Result<dennett_contracts::TaskRecord, DennettError> {
        if additional_turns.is_none()
            && additional_tokens.is_none()
            && additional_wall_secs.is_none()
        {
            return Err(DennettError::InvalidInput {
                field: String::from("resolution"),
                reason: String::from(
                    "must supply at least one of additional_turns, additional_tokens, or additional_wall_secs",
                ),
            });
        }

        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let lock_task_sql =
            format!("SELECT {TASK_SELECT_COLS} FROM tasks WHERE task_id = $1 FOR UPDATE");
        let task_row = tx
            .query_one(&lock_task_sql, &[&task_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let task = decode_task_row(&task_row)?;
        let _ = ensure_project_active_in_tx(
            &tx,
            task.project_id,
            "increase task budget",
            self.pg_target(),
        )
        .await?;

        let mut next_budget = task.budget.clone();
        if let Some(add) = additional_turns {
            next_budget.max_turns = Some(
                next_budget
                    .max_turns
                    .unwrap_or(task.usage.turns_used)
                    .saturating_add(add),
            );
        }
        if let Some(add) = additional_tokens {
            next_budget.max_tokens = Some(
                next_budget
                    .max_tokens
                    .unwrap_or(task.usage.tokens_used)
                    .saturating_add(add),
            );
        }
        if let Some(add) = additional_wall_secs {
            next_budget.max_wall_secs = Some(
                next_budget
                    .max_wall_secs
                    .unwrap_or(task.usage.wall_ms_used.div_ceil(1_000))
                    .saturating_add(add),
            );
        }
        // D2-BUG-05: Refuse budget adjustments that fail to relieve the actually exceeded dimension
        // (e.g. adding turns when tokens or wall_ms is still exceeded).
        if let Some(still_exceeded) = next_budget.check_before_next_turn(&task.usage) {
            return Err(DennettError::InvalidInput {
                field: String::from("resolution"),
                reason: format!(
                    "budget increase does not relieve exceeded dimension `{}` (used={}, new_limit={})",
                    still_exceeded.dimension, still_exceeded.used, still_exceeded.limit
                ),
            });
        }
        let budget_val =
            serde_json::to_value(&next_budget).map_err(|err| DennettError::Internal {
                detail: format!("failed serializing updated task budget: {err}"),
            })?;

        let card_updated = tx
            .execute(
                "UPDATE inbox_cards
                 SET status = 'resolved',
                     resolution_summary = 'Бюджет увеличен владельцем, поручение продолжено',
                     revision = revision + 1,
                     resolved_at = now(),
                     updated_at = now()
                 WHERE card_id = $1 AND status = 'open' AND revision = $2",
                &[&card_id.as_uuid(), &expected_revision],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        if card_updated == 0 {
            return Err(DennettError::Conflict {
                resource: String::from("inbox_card"),
                expected_revision: Some(expected_revision),
                actual_revision: None,
                reason: String::from("inbox card was already modified concurrently"),
            });
        }

        let update_task_sql = format!(
            "UPDATE tasks
             SET budget_json = $2,
                 status = 'queued',
                 wait_detail = NULL,
                 error_code = NULL,
                 error_summary = NULL,
                 updated_at = now()
             WHERE task_id = $1
             RETURNING {TASK_SELECT_COLS}"
        );
        let updated_row = tx
            .query_one(&update_task_sql, &[&task_id.as_uuid(), &budget_val])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let updated_task = decode_task_row(&updated_row)?;

        let _ = append_control_event_in_tx(
            &tx,
            self.pg_target(),
            ControlEventAppend {
                entity_key: &format!("task:{task_id}"),
                event_type: "task.budget_increased",
                project_id: Some(updated_task.project_id),
                session_id: Some(updated_task.session_id),
                task_id: Some(task_id),
                correlation_id,
                payload: json!({
                    "task_id": task_id,
                    "budget": next_budget,
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        self.spawn_task_execution(task_id, updated_task.generation, correlation_id.clone());
        Ok(updated_task)
    }
}

#[cfg(test)]
mod preview_snippet_tests {
    use super::extract_redacted_preview_snippet;
    use serde_json::json;

    #[test]
    fn extracts_agy_acp_pascal_case_content_and_redacts_secrets() {
        let raw = json!({
            "Path": "notes-r2.md",
            "Content": "# R2 Notes\napi_key=super-secret-token-value\nline 3"
        });
        let snippet =
            extract_redacted_preview_snippet(&raw, &[String::from("super-secret-token-value")])
                .expect("PascalCase Content must yield preview snippet");
        assert!(snippet.contains("# R2 Notes"));
        assert!(!snippet.contains("super-secret-token-value"));
        assert!(snippet.contains("[REDACTED]"));
    }

    #[test]
    fn extracts_agy_acp_edit_target_and_replacement_pair() {
        let raw = json!({
            "Path": "src/lib.rs",
            "Target": "let port = 8080;",
            "Replacement": "let port = 41771;"
        });
        let snippet = extract_redacted_preview_snippet(&raw, &[])
            .expect("Target/Replacement must yield diff snippet");
        assert!(snippet.contains("--- old\nlet port = 8080;"));
        assert!(snippet.contains("+++ new\nlet port = 41771;"));
    }

    #[test]
    fn extracts_file_content_and_acp_tool_call_diff_array() {
        let file_content = json!({ "FileContent": "hello from FileContent" });
        assert_eq!(
            extract_redacted_preview_snippet(&file_content, &[]).as_deref(),
            Some("hello from FileContent")
        );

        let acp_diff = json!({
            "Path": "notes-r2.md",
            "_acp_content": [
                {
                    "type": "diff",
                    "path": "notes-r2.md",
                    "oldText": "before",
                    "newText": "after secret-xyz"
                }
            ]
        });
        let snippet = extract_redacted_preview_snippet(&acp_diff, &[String::from("secret-xyz")])
            .expect("ACP diff array must yield preview snippet");
        assert!(snippet.contains("--- old\nbefore"));
        assert!(snippet.contains("+++ new\nafter [REDACTED]"));
    }
}
