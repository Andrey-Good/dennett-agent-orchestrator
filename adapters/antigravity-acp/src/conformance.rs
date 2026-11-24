//! Reusable conformance test suite for [`AgentRuntimePort`] implementations (Contract M §4 / D3).
//!
//! Verifies identical behavioral invariants across every concrete runtime adapter
//! (`SimulatedRuntime`, `LocalHttpRuntime`, and live `AntigravityAcpRuntime`):
//! 1. `probe()` descriptor honesty (`adapter_id`, `is_simulated`, `auth_profile`, models).
//! 2. Fresh session start + basic turn execution (`RunStatus::Succeeded`, visible text + events).
//! 3. Model switching within the same Dennett `SessionId`.
//! 4. V29 resume safety: adapter mismatch, auth profile mismatch, and workspace cwd mismatch
//!    MUST fail with `Conflict` when `allow_rehydrate == false` and rehydrate honestly
//!    (`rehydrated == true`) when `allow_rehydrate == true`.
//! 5. External [`PolicyEvaluator`] enforcement on both `Allow` and `Deny` decisions (zero bypass).
//! 6. Pre-cancelled turn and live in-flight cancellation (`RunStatus::Cancelled`).
//! 7. Clean `close_session`.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use dennett_contracts::{
    CorrelationId, DennettError, MessageId, MessageRecord, MessageRole, PolicyEffect, RunId,
    RunStatus, RuntimeAuthProfile, RuntimeBinding, RuntimeBindingId, SessionId, TimestampMs,
    WorkspaceWritePolicy,
};
use tokio::sync::{mpsc, watch};

use crate::port::{
    AgentRuntimePort, BoxFuture, PolicyEvaluationOutcome, PolicyEvaluationRequest, PolicyEvaluator,
    RuntimeEvent, SessionStartRequest, TurnRequest,
};

/// Adapter-specific prompt triggers fed into [`run_runtime_conformance_suite`].
#[derive(Debug, Clone)]
pub struct ConformancePrompts {
    /// Simple user prompt expecting a normal `RunStatus::Succeeded` text reply.
    pub basic_prompt: String,
    /// Prompt triggering a tool call expected to pass through [`PolicyEvaluator`] with `Allow`.
    pub tool_allow_prompt: String,
    /// Prompt triggering a tool call expected to pass through [`PolicyEvaluator`] with `Deny`.
    pub tool_deny_prompt: String,
    /// Prompt triggering a slow turn so live cancellation (`cancel_turn` / `cancel_rx`) can interrupt it.
    pub slow_cancel_prompt: String,
    /// Optional alternate model identifier supported by the runtime to verify live model switching.
    pub alt_model: Option<String>,
}

struct RecordingPolicy {
    effect: PolicyEffect,
    calls: Arc<AtomicUsize>,
}

impl PolicyEvaluator for RecordingPolicy {
    fn evaluate<'a>(
        &'a self,
        req: &'a PolicyEvaluationRequest,
    ) -> BoxFuture<'a, PolicyEvaluationOutcome> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            PolicyEvaluationOutcome {
                effect: self.effect,
                matched_rule_id: None,
                reason: format!(
                    "conformance policy {:?} for `{}`",
                    self.effect, req.tool_name
                ),
                scope: String::from("conformance"),
            }
        })
    }
}

fn alternate_auth_profile(current: RuntimeAuthProfile) -> RuntimeAuthProfile {
    match current {
        RuntimeAuthProfile::OAuthPersonal => RuntimeAuthProfile::GeminiApiKey,
        RuntimeAuthProfile::GeminiApiKey => RuntimeAuthProfile::LocalEndpoint,
        _ => RuntimeAuthProfile::OAuthPersonal,
    }
}

/// Runs the shared [`AgentRuntimePort`] conformance suite against `runtime` in `workspace`.
///
/// # Errors
///
/// Returns a descriptive failure string on the first violated runtime invariant.
#[expect(
    clippy::too_many_lines,
    reason = "conformance suite executes sequential lifecycle assertions (probe, turn, model switch, V29, policy, cancel, close) in one readable flow"
)]
pub async fn run_runtime_conformance_suite<R: AgentRuntimePort>(
    runtime: &R,
    workspace: &Path,
    prompts: &ConformancePrompts,
) -> Result<(), String> {
    // 1. Probe descriptor honesty.
    let descriptor = runtime
        .probe()
        .await
        .map_err(|err| format!("probe() failed: {err}"))?;
    if descriptor.adapter_id != runtime.adapter_id() {
        return Err(format!(
            "probe adapter_id mismatch: `{}` != `{}`",
            descriptor.adapter_id,
            runtime.adapter_id()
        ));
    }
    if descriptor.is_simulated != runtime.is_simulated() {
        return Err(format!(
            "probe is_simulated mismatch: {} != {}",
            descriptor.is_simulated,
            runtime.is_simulated()
        ));
    }
    if descriptor.auth_profile != runtime.auth_profile() {
        return Err(format!(
            "probe auth_profile mismatch: {} != {}",
            descriptor.auth_profile,
            runtime.auth_profile()
        ));
    }
    if descriptor.available_models.is_empty() {
        return Err(String::from(
            "probe() returned empty available_models catalog",
        ));
    }

    // 2. Fresh session start + basic turn execution.
    let session_id = SessionId::new();
    let start = runtime
        .start_or_resume_session(SessionStartRequest {
            dennett_session_id: session_id,
            workspace_cwd: workspace.to_path_buf(),
            system_prompt: String::from("You are a Dennett conformance test assistant."),
            model_preference: descriptor.default_model_id.clone(),
            thinking_level: None,
            workspace_write_policy: WorkspaceWritePolicy::AskBeforeWrite,
            mcp_servers: Vec::new(),
            active_skills: Vec::new(),
            existing_binding: None,
            history_for_rehydrate: Vec::new(),
            allow_rehydrate: true,
        })
        .await
        .map_err(|err| format!("fresh start_or_resume_session failed: {err}"))?;
    if start.provider_session_id.trim().is_empty() {
        return Err(String::from(
            "fresh session returned empty provider_session_id",
        ));
    }
    if start.rehydrated {
        return Err(String::from(
            "fresh session unexpectedly set rehydrated=true",
        ));
    }
    if start.adapter_id != runtime.adapter_id() || start.auth_profile != runtime.auth_profile() {
        return Err(String::from(
            "fresh session outcome adapter/profile mismatch",
        ));
    }

    let allow_calls = Arc::new(AtomicUsize::new(0));
    let allow_policy: Arc<dyn PolicyEvaluator> = Arc::new(RecordingPolicy {
        effect: PolicyEffect::Allow,
        calls: Arc::clone(&allow_calls),
    });

    let (ev_tx, mut ev_rx) = mpsc::channel(64);
    let (_cancel_tx, cancel_rx) = watch::channel(false);
    let basic_turn = runtime
        .send_prompt_turn(
            TurnRequest {
                dennett_session_id: session_id,
                provider_session_id: start.provider_session_id.clone(),
                run_id: RunId::new(),
                prompt_text: prompts.basic_prompt.clone(),
                timeout: Duration::from_secs(15),
                cancel_rx,
            },
            Arc::clone(&allow_policy),
            Some(ev_tx),
        )
        .await
        .map_err(|err| format!("basic send_prompt_turn failed: {err}"))?;
    if basic_turn.status != RunStatus::Succeeded {
        return Err(format!("expected Succeeded, got {:?}", basic_turn.status));
    }
    if basic_turn.assistant_text.trim().is_empty() {
        return Err(String::from("basic turn produced empty assistant_text"));
    }
    let mut streamed_text_delta = false;
    while let Ok(ev) = ev_rx.try_recv() {
        if matches!(ev, RuntimeEvent::TextDelta { .. }) {
            streamed_text_delta = true;
        }
    }
    if !streamed_text_delta {
        return Err(String::from(
            "basic turn did not stream any TextDelta event",
        ));
    }

    // 3. Model switch within the same Dennett session.
    if let Some(ref alt_model) = prompts.alt_model {
        let now = TimestampMs::now();
        let existing_binding = RuntimeBinding {
            binding_id: RuntimeBindingId::new(),
            session_id,
            adapter_id: start.adapter_id.clone(),
            auth_profile: start.auth_profile,
            tier: start.tier.clone(),
            agy_version: start.agy_version.clone(),
            provider_session_id: Some(start.provider_session_id.clone()),
            workspace_cwd: start.workspace_cwd.clone(),
            is_simulated: start.is_simulated,
            rehydrate_count: 0,
            connection_id: None,
            endpoint_fingerprint: start.endpoint_fingerprint.clone(),
            created_at: now,
            updated_at: now,
        };
        let switched = runtime
            .start_or_resume_session(SessionStartRequest {
                dennett_session_id: session_id,
                workspace_cwd: workspace.to_path_buf(),
                system_prompt: String::from("You are a Dennett conformance test assistant."),
                model_preference: Some(alt_model.clone()),
                thinking_level: None,
                workspace_write_policy: WorkspaceWritePolicy::AskBeforeWrite,
                mcp_servers: Vec::new(),
                active_skills: Vec::new(),
                existing_binding: Some(existing_binding),
                history_for_rehydrate: Vec::new(),
                allow_rehydrate: true,
            })
            .await
            .map_err(|err| format!("model switch start_or_resume_session failed: {err}"))?;
        let (_c_tx, c_rx) = watch::channel(false);
        let switch_turn = runtime
            .send_prompt_turn(
                TurnRequest {
                    dennett_session_id: session_id,
                    provider_session_id: switched.provider_session_id,
                    run_id: RunId::new(),
                    prompt_text: prompts.basic_prompt.clone(),
                    timeout: Duration::from_secs(15),
                    cancel_rx: c_rx,
                },
                Arc::clone(&allow_policy),
                None,
            )
            .await
            .map_err(|err| format!("post-model-switch turn failed: {err}"))?;
        if switch_turn.status != RunStatus::Succeeded {
            return Err(format!(
                "post-model-switch turn did not succeed: {:?}",
                switch_turn.status
            ));
        }
    }

    // 4. V29 resume mismatch safety checks (adapter, auth_profile, cwd).
    let now = TimestampMs::now();
    let v29_session_id = SessionId::new();
    let foreign_adapter_binding = RuntimeBinding {
        binding_id: RuntimeBindingId::new(),
        session_id: v29_session_id,
        adapter_id: String::from("foreign-other-adapter"),
        auth_profile: runtime.auth_profile(),
        tier: None,
        agy_version: None,
        provider_session_id: Some(String::from("foreign-prov-1")),
        workspace_cwd: start.workspace_cwd.clone(),
        is_simulated: runtime.is_simulated(),
        rehydrate_count: 0,
        connection_id: None,
        endpoint_fingerprint: start.endpoint_fingerprint.clone(),
        created_at: now,
        updated_at: now,
    };
    let foreign_res = runtime
        .start_or_resume_session(SessionStartRequest {
            dennett_session_id: v29_session_id,
            workspace_cwd: workspace.to_path_buf(),
            system_prompt: String::from("sys"),
            model_preference: None,
            thinking_level: None,
            workspace_write_policy: WorkspaceWritePolicy::DenyAll,
            mcp_servers: Vec::new(),
            active_skills: Vec::new(),
            existing_binding: Some(foreign_adapter_binding.clone()),
            history_for_rehydrate: Vec::new(),
            allow_rehydrate: false,
        })
        .await;
    if !matches!(foreign_res, Err(DennettError::Conflict { .. })) {
        return Err(format!(
            "V29 adapter mismatch with allow_rehydrate=false must return Conflict, got {foreign_res:?}"
        ));
    }

    let foreign_profile_binding = RuntimeBinding {
        adapter_id: runtime.adapter_id().to_owned(),
        auth_profile: alternate_auth_profile(runtime.auth_profile()),
        ..foreign_adapter_binding.clone()
    };
    let profile_res = runtime
        .start_or_resume_session(SessionStartRequest {
            dennett_session_id: v29_session_id,
            workspace_cwd: workspace.to_path_buf(),
            system_prompt: String::from("sys"),
            model_preference: None,
            thinking_level: None,
            workspace_write_policy: WorkspaceWritePolicy::DenyAll,
            mcp_servers: Vec::new(),
            active_skills: Vec::new(),
            existing_binding: Some(foreign_profile_binding),
            history_for_rehydrate: Vec::new(),
            allow_rehydrate: false,
        })
        .await;
    if !matches!(profile_res, Err(DennettError::Conflict { .. })) {
        return Err(format!(
            "V29 auth_profile mismatch with allow_rehydrate=false must return Conflict, got {profile_res:?}"
        ));
    }

    let wrong_cwd_binding = RuntimeBinding {
        adapter_id: runtime.adapter_id().to_owned(),
        auth_profile: runtime.auth_profile(),
        workspace_cwd: String::from("/nonexistent-other-workspace-v29"),
        ..foreign_adapter_binding.clone()
    };
    let cwd_res = runtime
        .start_or_resume_session(SessionStartRequest {
            dennett_session_id: v29_session_id,
            workspace_cwd: workspace.to_path_buf(),
            system_prompt: String::from("sys"),
            model_preference: None,
            thinking_level: None,
            workspace_write_policy: WorkspaceWritePolicy::DenyAll,
            mcp_servers: Vec::new(),
            active_skills: Vec::new(),
            existing_binding: Some(wrong_cwd_binding),
            history_for_rehydrate: Vec::new(),
            allow_rehydrate: false,
        })
        .await;
    if !matches!(cwd_res, Err(DennettError::Conflict { .. })) {
        return Err(format!(
            "V29 cwd mismatch with allow_rehydrate=false must return Conflict, got {cwd_res:?}"
        ));
    }

    // With allow_rehydrate = true, cross-adapter binding must rehydrate honestly!
    let rehydrated_start = runtime
        .start_or_resume_session(SessionStartRequest {
            dennett_session_id: v29_session_id,
            workspace_cwd: workspace.to_path_buf(),
            system_prompt: String::from("sys"),
            model_preference: descriptor.default_model_id.clone(),
            thinking_level: None,
            workspace_write_policy: WorkspaceWritePolicy::AskBeforeWrite,
            mcp_servers: Vec::new(),
            active_skills: Vec::new(),
            existing_binding: Some(foreign_adapter_binding),
            history_for_rehydrate: vec![MessageRecord {
                message_id: MessageId::new(),
                session_id: v29_session_id,
                role: MessageRole::User,
                content: String::from("Remember constraint: PG16 only."),
                run_id: None,
                client_message_id: None,
                correlation_id: CorrelationId::generate(),
                created_at: now,
            }],
            allow_rehydrate: true,
        })
        .await
        .map_err(|err| format!("V29 honest rehydrate failed: {err}"))?;
    if !rehydrated_start.rehydrated || rehydrated_start.rehydrate_reason.is_none() {
        return Err(format!(
            "V29 cross-adapter start with allow_rehydrate=true must set rehydrated=true and reason, got {rehydrated_start:?}"
        ));
    }
    runtime
        .close_session(v29_session_id)
        .await
        .map_err(|err| format!("close_session(v29) failed: {err}"))?;

    // 5. External PolicyEvaluator enforcement (Allow + Deny).
    let allow_before = allow_calls.load(Ordering::SeqCst);
    let (_c1_tx, c1_rx) = watch::channel(false);
    let tool_allow = runtime
        .send_prompt_turn(
            TurnRequest {
                dennett_session_id: session_id,
                provider_session_id: start.provider_session_id.clone(),
                run_id: RunId::new(),
                prompt_text: prompts.tool_allow_prompt.clone(),
                timeout: Duration::from_secs(15),
                cancel_rx: c1_rx,
            },
            Arc::clone(&allow_policy),
            None,
        )
        .await
        .map_err(|err| format!("tool_allow turn failed: {err}"))?;
    if allow_calls.load(Ordering::SeqCst) <= allow_before {
        return Err(String::from(
            "tool_allow turn bypassed PolicyEvaluator completely",
        ));
    }
    if !tool_allow.events.iter().any(|ev| {
        matches!(
            ev,
            RuntimeEvent::PermissionResolved {
                effect: PolicyEffect::Allow,
                ..
            }
        )
    }) {
        return Err(String::from(
            "tool_allow turn did not emit PermissionResolved(Allow)",
        ));
    }

    let deny_calls = Arc::new(AtomicUsize::new(0));
    let deny_policy: Arc<dyn PolicyEvaluator> = Arc::new(RecordingPolicy {
        effect: PolicyEffect::Deny,
        calls: Arc::clone(&deny_calls),
    });
    let (_c2_tx, c2_rx) = watch::channel(false);
    let tool_deny = runtime
        .send_prompt_turn(
            TurnRequest {
                dennett_session_id: session_id,
                provider_session_id: start.provider_session_id.clone(),
                run_id: RunId::new(),
                prompt_text: prompts.tool_deny_prompt.clone(),
                timeout: Duration::from_secs(15),
                cancel_rx: c2_rx,
            },
            deny_policy,
            None,
        )
        .await
        .map_err(|err| format!("tool_deny turn failed: {err}"))?;
    if deny_calls.load(Ordering::SeqCst) == 0 {
        return Err(String::from(
            "tool_deny turn bypassed PolicyEvaluator completely",
        ));
    }
    if !tool_deny.events.iter().any(|ev| {
        matches!(
            ev,
            RuntimeEvent::PermissionResolved {
                effect: PolicyEffect::Deny,
                ..
            }
        )
    }) {
        return Err(String::from(
            "tool_deny turn did not emit PermissionResolved(Deny)",
        ));
    }

    // 6a. Pre-cancelled turn returns Cancelled immediately.
    let (_pre_tx, pre_rx) = watch::channel(true);
    let pre_cancelled = runtime
        .send_prompt_turn(
            TurnRequest {
                dennett_session_id: session_id,
                provider_session_id: start.provider_session_id.clone(),
                run_id: RunId::new(),
                prompt_text: prompts.basic_prompt.clone(),
                timeout: Duration::from_secs(10),
                cancel_rx: pre_rx,
            },
            Arc::clone(&allow_policy),
            None,
        )
        .await
        .map_err(|err| format!("pre-cancelled turn returned error instead of Cancelled: {err}"))?;
    if pre_cancelled.status != RunStatus::Cancelled {
        return Err(format!(
            "pre-cancelled turn expected RunStatus::Cancelled, got {:?}",
            pre_cancelled.status
        ));
    }

    // 6b. Live cancellation via watch channel during slow turn.
    let (live_tx, live_rx) = watch::channel(false);
    let canceller = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(80)).await;
        let _ = live_tx.send(true);
    });
    let live_cancelled = runtime
        .send_prompt_turn(
            TurnRequest {
                dennett_session_id: session_id,
                provider_session_id: start.provider_session_id.clone(),
                run_id: RunId::new(),
                prompt_text: prompts.slow_cancel_prompt.clone(),
                timeout: Duration::from_secs(10),
                cancel_rx: live_rx,
            },
            allow_policy,
            None,
        )
        .await
        .map_err(|err| format!("live-cancelled turn failed: {err}"))?;
    let _ = canceller.await;
    if live_cancelled.status != RunStatus::Cancelled {
        return Err(format!(
            "live-cancelled turn expected RunStatus::Cancelled, got {:?}",
            live_cancelled.status
        ));
    }

    // 7. Close session cleanly.
    runtime
        .close_session(session_id)
        .await
        .map_err(|err| format!("close_session failed: {err}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use dennett_contracts::RuntimeAuthProfile;

    use super::{ConformancePrompts, run_runtime_conformance_suite};
    use crate::SimulatedRuntime;

    #[tokio::test]
    async fn simulated_runtime_passes_shared_conformance_suite() {
        let ws = std::env::temp_dir().join(format!(
            "dennett-sim-conformance-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&ws).expect("create conformance workspace");

        let runtime = SimulatedRuntime::new(RuntimeAuthProfile::OAuthPersonal);
        let prompts = ConformancePrompts {
            basic_prompt: String::from("Say hello from conformance"),
            tool_allow_prompt: String::from("[SIM:TOOL:read_file:README.md]"),
            tool_deny_prompt: String::from("[SIM:TOOL:run_command:rm -rf /tmp/fake]"),
            slow_cancel_prompt: String::from("[SIM:SLOW:5000]"),
            alt_model: Some(String::from("sim-model-beta")),
        };

        let result = run_runtime_conformance_suite(&runtime, &ws, &prompts).await;
        let _ = std::fs::remove_dir_all(&ws);
        assert!(
            result.is_ok(),
            "SimulatedRuntime failed conformance suite: {:?}",
            result.err()
        );
    }
}
