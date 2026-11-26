//! Deterministic CI test-double implementation of [`AgentRuntimePort`].
//!
//! Strictly sets `adapter_id = "simulated-ci-runtime"` and `is_simulated = true` in every
//! descriptor, binding, and outcome so it can NEVER masquerade as a live `agy acp` runtime.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use dennett_contracts::redact::redact_secrets;
use dennett_contracts::{
    DennettError, PolicyEffect, RunStatus, RuntimeAuthProfile, RuntimeModelOption,
    RuntimeThinkingOption, SessionId, ThinkingLevel, WorkspaceWritePolicy,
};
use serde_json::{Map, Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{Mutex, mpsc, watch};
use tokio::time;
use uuid::Uuid;

use crate::port::{
    ActiveSkillPayload, AgentRuntimePort, BoxFuture, PolicyEvaluationRequest, PolicyEvaluator,
    RuntimeDescriptor, RuntimeEvent, RuntimeMcpServerSpec, SessionStartOutcome,
    SessionStartRequest, TurnOutcome, TurnRequest, evaluate_binding_resume_compatibility,
};
use crate::profile::normalize_workspace_path;

/// Canonical adapter identifier for the deterministic CI test double.
pub const SIMULATED_ADAPTER_ID: &str = "simulated-ci-runtime";

#[derive(Debug, Clone)]
struct SimSessionState {
    auth_profile: RuntimeAuthProfile,
    provider_session_id: String,
    workspace_cwd: String,
    system_prompt: String,
    model_preference: Option<String>,
    thinking_level: Option<ThinkingLevel>,
    write_policy: WorkspaceWritePolicy,
    mcp_servers: Vec<RuntimeMcpServerSpec>,
    active_skills: Vec<ActiveSkillPayload>,
    turn_history: Vec<String>,
}

/// Deterministic [`AgentRuntimePort`] test double for offline CI integration suites.
///
/// Supports explicit test directives inside prompts (either at prompt start or on their own line):
/// - `[SIM:TOOL:write_file:<relative_or_abs_path>|<content>]` — invokes real async [`PolicyEvaluator`],
///   writes the file once ONLY if allowed (zero double execution), and denies cleanly if blocked.
/// - `[SIM:TOOL:run_command:<command>]` — invokes real async [`PolicyEvaluator`] for shell execution.
/// - `[SIM:TOOL:mcp:<tool_name>|<target>]` — invokes real async [`PolicyEvaluator`] for MCP tool calls.
/// - `[SIM:SLOW:<millis>]` — sleeps incrementally while watching `cancel_rx` to verify live Stop
///   and strict rejection of late permission requests after cancel.
/// - `[SIM:ERROR:<message>]` — emits partial `TextDelta` then returns `Err(RuntimeProtocol)` so
///   callers prove partial output NEVER becomes `Succeeded`.
/// - `[SIM:CLARIFY:<question>]` — emits [`RuntimeEvent::ClarificationRequested`] and ends turn with
///   `stop_reason = "clarification_needed"`.
/// - `[SIM:TOKENS:<count>]` — reports `usage: { "simulated": true, "totalTokens": <count> }`.
/// - `[SIM:PARTIAL_ARTIFACT:<name>|<content>]` — emits [`RuntimeEvent::PartialArtifactEmitted`].
/// - `[SIM:EFFECT:<recipient>|<operation>|<behavior>|<idem_key>]` — emits
///   [`RuntimeEvent::ExternalEffectRequested`] for end-to-end external effect verification.
pub struct SimulatedRuntime {
    auth_profile: RuntimeAuthProfile,
    sessions: Mutex<HashMap<SessionId, SimSessionState>>,
    active_cancels: Mutex<HashMap<SessionId, watch::Sender<bool>>>,
    fail_next_resume: AtomicBool,
}

impl SimulatedRuntime {
    /// Creates a new deterministic [`SimulatedRuntime`] explicitly marked `is_simulated = true`.
    #[must_use]
    pub fn new(auth_profile: RuntimeAuthProfile) -> Self {
        Self {
            auth_profile,
            sessions: Mutex::new(HashMap::new()),
            active_cancels: Mutex::new(HashMap::new()),
            fail_next_resume: AtomicBool::new(false),
        }
    }

    /// Forces the next explicit resume attempt to report a missing/corrupt provider session
    /// so tests can deterministically exercise honest rehydration and fail-closed resume errors.
    pub fn set_fail_next_resume(&self, fail: bool) {
        self.fail_next_resume.store(fail, Ordering::SeqCst);
    }
}

impl Default for SimulatedRuntime {
    fn default() -> Self {
        Self::new(RuntimeAuthProfile::OAuthPersonal)
    }
}

impl AgentRuntimePort for SimulatedRuntime {
    fn adapter_id(&self) -> &str {
        SIMULATED_ADAPTER_ID
    }

    fn is_simulated(&self) -> bool {
        true
    }

    fn auth_profile(&self) -> RuntimeAuthProfile {
        self.auth_profile
    }

    fn probe(&self) -> BoxFuture<'_, Result<RuntimeDescriptor, DennettError>> {
        Box::pin(async move {
            Ok(RuntimeDescriptor {
                adapter_id: String::from(SIMULATED_ADAPTER_ID),
                auth_profile: self.auth_profile,
                is_simulated: true,
                agy_version: None,
                tier: Some(String::from("SIMULATED_CI_ONLY")),
                protocol_version: Some(1),
                supports_resume: true,
                supports_system_prompt_append: true,
                available_models: vec![
                    String::from("sim-model-alpha"),
                    String::from("sim-model-beta"),
                ],
                models: vec![
                    RuntimeModelOption {
                        model_id: String::from("sim-model-alpha"),
                        label: String::from("Simulated Alpha"),
                        supported_thinking_levels: vec![
                            ThinkingLevel::Low,
                            ThinkingLevel::Medium,
                            ThinkingLevel::High,
                            ThinkingLevel::XHigh,
                        ],
                        default_thinking_level: Some(ThinkingLevel::Medium),
                    },
                    RuntimeModelOption {
                        model_id: String::from("sim-model-beta"),
                        label: String::from("Simulated Beta"),
                        supported_thinking_levels: vec![
                            ThinkingLevel::Low,
                            ThinkingLevel::Medium,
                            ThinkingLevel::High,
                        ],
                        default_thinking_level: Some(ThinkingLevel::Medium),
                    },
                ],
                thinking_options: vec![
                    RuntimeThinkingOption {
                        value: ThinkingLevel::Low,
                        label: String::from("Low"),
                    },
                    RuntimeThinkingOption {
                        value: ThinkingLevel::Medium,
                        label: String::from("Medium"),
                    },
                    RuntimeThinkingOption {
                        value: ThinkingLevel::High,
                        label: String::from("High"),
                    },
                    RuntimeThinkingOption {
                        value: ThinkingLevel::XHigh,
                        label: String::from("Extra"),
                    },
                ],
                default_model_id: Some(String::from("sim-model-alpha")),
                default_thinking_level: Some(ThinkingLevel::Medium),
                quotas: Vec::new(),
                capabilities: vec![
                    String::from("simulated_ci_double"),
                    String::from("external_permissions"),
                    String::from("live_cancel"),
                ],
            })
        })
    }

    #[expect(
        clippy::too_many_lines,
        reason = "start_or_resume_session handles explicit binding checks, live resume, and honest rehydration in one state transition"
    )]
    fn start_or_resume_session(
        &self,
        req: SessionStartRequest,
    ) -> BoxFuture<'_, Result<SessionStartOutcome, DennettError>> {
        Box::pin(async move {
            let target_cwd = normalize_workspace_path(&req.workspace_cwd);
            let mut guard = self.sessions.lock().await;

            let mut rehydrated = false;
            let mut rehydrate_reason: Option<String> = None;

            if let Some(ref binding) = req.existing_binding {
                if let Err(msg) = evaluate_binding_resume_compatibility(
                    binding,
                    SIMULATED_ADAPTER_ID,
                    true,
                    self.auth_profile,
                    &target_cwd,
                    None,
                    None,
                ) {
                    if !req.allow_rehydrate {
                        return Err(DennettError::Conflict {
                            resource: String::from("runtime_binding"),
                            expected_revision: None,
                            actual_revision: None,
                            reason: msg,
                        });
                    }
                    rehydrated = true;
                    rehydrate_reason = Some(msg);
                } else if self.fail_next_resume.swap(false, Ordering::SeqCst) {
                    let msg = String::from("simulated provider session missing or expired");
                    if !req.allow_rehydrate {
                        return Err(DennettError::RuntimeProtocol {
                            code: Some(-32002),
                            detail: msg,
                        });
                    }
                    rehydrated = true;
                    rehydrate_reason = Some(msg);
                } else if let Some(ref bound_prov_id) = binding.provider_session_id {
                    if let Some(existing) = guard.get_mut(&req.dennett_session_id)
                        && existing.provider_session_id == *bound_prov_id
                    {
                        // Live session resume: apply updated model/thinking/skills without creating a new Agent!
                        existing.auth_profile = self.auth_profile;
                        existing.system_prompt = req.system_prompt;
                        existing.model_preference = req.model_preference;
                        existing.thinking_level = req.thinking_level;
                        existing.write_policy = req.workspace_write_policy;
                        existing.mcp_servers = req.mcp_servers;
                        existing.active_skills = req.active_skills;
                        return Ok(SessionStartOutcome {
                            adapter_id: String::from(SIMULATED_ADAPTER_ID),
                            auth_profile: self.auth_profile,
                            tier: Some(String::from("SIMULATED_CI_ONLY")),
                            agy_version: None,
                            provider_session_id: bound_prov_id.clone(),
                            workspace_cwd: target_cwd,
                            is_simulated: true,
                            rehydrated: false,
                            rehydrate_reason: None,
                            available_models: vec![
                                String::from("sim-model-alpha"),
                                String::from("sim-model-beta"),
                            ],
                            endpoint_fingerprint: None,
                        });
                    }
                    // Simulated daemon restarted (in-memory state gone) -> honest rehydrate from Dennett history!
                    let msg = format!(
                        "provider session `{bound_prov_id}` not present in restarted simulated runtime"
                    );
                    if !req.allow_rehydrate {
                        return Err(DennettError::RuntimeProtocol {
                            code: Some(-32002),
                            detail: msg,
                        });
                    }
                    rehydrated = true;
                    rehydrate_reason = Some(msg);
                }
            }

            let provider_session_id = format!("sim-sess-{}", Uuid::new_v4().simple());
            let mut turn_history = Vec::new();
            if rehydrated {
                for m in &req.history_for_rehydrate {
                    turn_history.push(format!("[{}] {}", m.role, m.content));
                }
            }

            guard.insert(
                req.dennett_session_id,
                SimSessionState {
                    auth_profile: self.auth_profile,
                    provider_session_id: provider_session_id.clone(),
                    workspace_cwd: target_cwd.clone(),
                    system_prompt: req.system_prompt,
                    model_preference: req.model_preference,
                    thinking_level: req.thinking_level,
                    write_policy: req.workspace_write_policy,
                    mcp_servers: req.mcp_servers,
                    active_skills: req.active_skills,
                    turn_history,
                },
            );

            Ok(SessionStartOutcome {
                adapter_id: String::from(SIMULATED_ADAPTER_ID),
                auth_profile: self.auth_profile,
                tier: Some(String::from("SIMULATED_CI_ONLY")),
                agy_version: None,
                provider_session_id,
                workspace_cwd: target_cwd,
                is_simulated: true,
                rehydrated,
                rehydrate_reason,
                available_models: vec![
                    String::from("sim-model-alpha"),
                    String::from("sim-model-beta"),
                ],
                endpoint_fingerprint: None,
            })
        })
    }

    fn send_prompt_turn(
        &self,
        req: TurnRequest,
        policy: Arc<dyn PolicyEvaluator>,
        event_tx: Option<mpsc::Sender<RuntimeEvent>>,
    ) -> BoxFuture<'_, Result<TurnOutcome, DennettError>> {
        Box::pin(async move {
            let snapshot = {
                let guard = self.sessions.lock().await;
                guard.get(&req.dennett_session_id).cloned()
            }
            .ok_or_else(|| DennettError::NotFound {
                resource: String::from("runtime_session"),
                id: req.dennett_session_id.to_string(),
            })?;

            let (cancel_tx, mut cancel_rx) = watch::channel(*req.cancel_rx.borrow());
            {
                let mut cancels = self.active_cancels.lock().await;
                cancels.insert(req.dennett_session_id, cancel_tx.clone());
            }

            let dennett_session_id = req.dennett_session_id;
            let run_id = req.run_id;
            let prompt_text = req.prompt_text;
            let mut caller_rx = req.cancel_rx;
            let forward_tx = cancel_tx.clone();
            let forwarder = tokio::spawn(async move {
                while !*caller_rx.borrow() {
                    if caller_rx.changed().await.is_err() {
                        break;
                    }
                }
                if *caller_rx.borrow() {
                    let _ = forward_tx.send(true);
                }
            });

            let result = execute_sim_turn(
                &snapshot,
                dennett_session_id,
                run_id,
                &prompt_text,
                req.timeout,
                &policy,
                event_tx.as_ref(),
                &mut cancel_rx,
            )
            .await;

            forwarder.abort();
            {
                let mut cancels = self.active_cancels.lock().await;
                cancels.remove(&dennett_session_id);
            }

            if let Ok(ref outcome) = result
                && outcome.status == RunStatus::Succeeded
            {
                let mut guard = self.sessions.lock().await;
                if let Some(state) = guard.get_mut(&dennett_session_id) {
                    state.turn_history.push(format!("[user] {prompt_text}"));
                    state
                        .turn_history
                        .push(format!("[assistant] {}", outcome.assistant_text));
                }
            }

            result
        })
    }

    fn cancel_turn(
        &self,
        dennett_session_id: SessionId,
    ) -> BoxFuture<'_, Result<bool, DennettError>> {
        Box::pin(async move {
            let cancels = self.active_cancels.lock().await;
            if let Some(tx) = cancels.get(&dennett_session_id) {
                let _ = tx.send(true);
                return Ok(true);
            }
            Ok(false)
        })
    }

    fn close_session(
        &self,
        dennett_session_id: SessionId,
    ) -> BoxFuture<'_, Result<(), DennettError>> {
        Box::pin(async move {
            if let Some(tx) = self.active_cancels.lock().await.remove(&dennett_session_id) {
                let _ = tx.send(true);
            }
            self.sessions.lock().await.remove(&dennett_session_id);
            Ok(())
        })
    }
}

fn emit(events: &mut Vec<RuntimeEvent>, ev: RuntimeEvent, tx: Option<&mpsc::Sender<RuntimeEvent>>) {
    if let Some(sink) = tx {
        let _ = sink.try_send(ev.clone());
    }
    events.push(ev);
}

fn find_all_directives<'a>(prompt: &'a str, prefix: &str) -> Vec<&'a str> {
    let mut results = Vec::new();
    let mut rest = prompt;
    while let Some(pos) = rest.find(prefix) {
        let after_prefix = &rest[pos + prefix.len()..];
        if let Some((value, tail)) = after_prefix.split_once(']') {
            results.push(value.trim());
            rest = tail;
        } else {
            results.push(after_prefix.trim());
            break;
        }
    }
    results
}

fn find_directive<'a>(prompt: &'a str, prefix: &str) -> Option<&'a str> {
    find_all_directives(prompt, prefix).into_iter().next()
}

async fn sleep_sim_cancellable(
    dur: Duration,
    turn_deadline: time::Instant,
    cancel_rx: &mut watch::Receiver<bool>,
) -> Result<bool, DennettError> {
    if *cancel_rx.borrow() {
        return Ok(true);
    }
    let remaining = turn_deadline.saturating_duration_since(time::Instant::now());
    if remaining.is_zero() {
        return Err(DennettError::Timeout {
            operation: String::from("session/prompt"),
        });
    }
    let wait = dur.min(remaining);
    tokio::select! {
        biased;
        changed = cancel_rx.changed() => {
            Ok(changed.is_ok() && *cancel_rx.borrow())
        }
        () = time::sleep(wait) => {
            if time::Instant::now() >= turn_deadline && wait == remaining && remaining < dur {
                Err(DennettError::Timeout {
                    operation: String::from("session/prompt"),
                })
            } else {
                Ok(*cancel_rx.borrow())
            }
        }
    }
}

#[expect(
    clippy::too_many_lines,
    clippy::too_many_arguments,
    reason = "simulated directive dispatcher keeps all deterministic CI paths in one clear function"
)]
async fn execute_sim_turn(
    session: &SimSessionState,
    dennett_session_id: SessionId,
    run_id: dennett_contracts::RunId,
    prompt_text: &str,
    turn_timeout: Duration,
    policy: &Arc<dyn PolicyEvaluator>,
    event_tx: Option<&mpsc::Sender<RuntimeEvent>>,
    cancel_rx: &mut watch::Receiver<bool>,
) -> Result<TurnOutcome, DennettError> {
    let mut events = Vec::new();
    let mut human_wait_ms: u64 = 0;
    let prompt = prompt_text.trim();

    if *cancel_rx.borrow() {
        emit(
            &mut events,
            RuntimeEvent::Cancelled {
                reason: String::from("cancelled before execution"),
            },
            event_tx,
        );
        return Ok(TurnOutcome {
            status: RunStatus::Cancelled,
            stop_reason: String::from("cancelled"),
            assistant_text: String::new(),
            events,
            usage: None,
            human_wait_ms,
        });
    }

    if let Some(detail) = find_directive(prompt, "[SIM:ERROR:") {
        emit(
            &mut events,
            RuntimeEvent::TextDelta {
                text: String::from("partial output before failure"),
            },
            event_tx,
        );
        return Err(DennettError::RuntimeProtocol {
            code: Some(-32000),
            detail: detail.to_owned(),
        });
    }

    if let Some(raw_quota) = find_directive(prompt, "[SIM:QUOTA:") {
        let (detail, retry_after_secs) = match raw_quota.split_once('|') {
            Some((d, secs)) => (d.trim().to_owned(), secs.trim().parse::<u64>().ok()),
            None => (raw_quota.trim().to_owned(), None),
        };
        return Err(DennettError::ProviderQuotaExceeded {
            provider: String::from(SIMULATED_ADAPTER_ID),
            detail: if detail.is_empty() {
                String::from("simulated upstream provider quota exceeded")
            } else {
                detail
            },
            retry_after_secs,
        });
    }

    if let Some(detail) = find_directive(prompt, "[SIM:AUTH_REQUIRED:") {
        return Err(DennettError::AuthRequired {
            profile: session.auth_profile,
            detail: if detail.is_empty() {
                String::from("simulated auth required")
            } else {
                detail.to_owned()
            },
        });
    }

    if let Some(millis_str) = find_directive(prompt, "[SIM:SLOW:") {
        let millis: u64 = millis_str.parse().unwrap_or(5_000);
        emit(
            &mut events,
            RuntimeEvent::TextDelta {
                text: String::from("starting slow simulated turn..."),
            },
            event_tx,
        );
        let sleep_dur = Duration::from_millis(millis.min(30_000));
        tokio::select! {
            biased;
            changed = cancel_rx.changed() => {
                if changed.is_ok() && *cancel_rx.borrow() {
                    // Simulate a late permission request arriving after cancel: must be abandoned!
                    emit(
                        &mut events,
                        RuntimeEvent::PermissionRequested {
                            tool_call_id: String::from("sim-late-perm"),
                            tool_name: String::from("run_command"),
                            target: Some(String::from("echo late")),
                            title: String::from("Late tool after cancel"),
                        },
                        event_tx,
                    );
                    emit(
                        &mut events,
                        RuntimeEvent::PermissionResolved {
                            tool_call_id: String::from("sim-late-perm"),
                            tool_name: String::from("run_command"),
                            target: Some(String::from("echo late")),
                            effect: PolicyEffect::Deny,
                            matched_rule_id: None,
                            reason: String::from("abandoned because run was cancelled"),
                            scope: format!("session:{dennett_session_id}"),
                            chosen_option_id: None,
                        },
                        event_tx,
                    );
                    emit(
                        &mut events,
                        RuntimeEvent::Cancelled {
                            reason: String::from("cancelled"),
                        },
                        event_tx,
                    );
                    return Ok(TurnOutcome {
                        status: RunStatus::Cancelled,
                        stop_reason: String::from("cancelled"),
                        assistant_text: String::from("starting slow simulated turn..."),
                        events,
                        usage: None,
                        human_wait_ms,
                    });
                }
            }
            () = time::sleep(sleep_dur) => {}
            () = time::sleep(turn_timeout) => {
                return Err(DennettError::Timeout {
                    operation: String::from("session/prompt"),
                });
            }
        }
    }

    let explicit_tokens =
        find_directive(prompt, "[SIM:TOKENS:").and_then(|raw| raw.parse::<u64>().ok());
    let usage_payload = match explicit_tokens {
        Some(tokens) => {
            let u = json!({ "simulated": true, "totalTokens": tokens });
            emit(
                &mut events,
                RuntimeEvent::UsageReported { usage: u.clone() },
                event_tx,
            );
            Some(u)
        }
        None => Some(json!({ "simulated": true })),
    };

    for raw_partial in find_all_directives(prompt, "[SIM:PARTIAL_ARTIFACT:") {
        let (name, content) = match raw_partial.split_once('|') {
            Some((n, c)) => (n.trim().to_owned(), c.to_owned()),
            None => (String::from("partial-draft.md"), raw_partial.to_owned()),
        };
        if !name.is_empty() {
            emit(
                &mut events,
                RuntimeEvent::PartialArtifactEmitted { name, content },
                event_tx,
            );
        }
    }

    if let Some(question) = find_directive(prompt, "[SIM:CLARIFY:") {
        let question_owned = if question.is_empty() {
            String::from("Please clarify the expected parameters.")
        } else {
            question.to_owned()
        };
        emit(
            &mut events,
            RuntimeEvent::ClarificationRequested {
                question: question_owned.clone(),
            },
            event_tx,
        );
        emit(
            &mut events,
            RuntimeEvent::TextDelta {
                text: question_owned.clone(),
            },
            event_tx,
        );
        emit(
            &mut events,
            RuntimeEvent::Completed {
                stop_reason: String::from("clarification_needed"),
            },
            event_tx,
        );
        return Ok(TurnOutcome {
            status: RunStatus::Succeeded,
            stop_reason: String::from("clarification_needed"),
            assistant_text: question_owned,
            events,
            usage: usage_payload,
            human_wait_ms,
        });
    }

    let mut effect_summaries = Vec::new();
    for raw_effect in find_all_directives(prompt, "[SIM:EFFECT:") {
        let mut parts = raw_effect.splitn(4, '|');
        let recipient = parts.next().unwrap_or("sim-recipient").trim().to_owned();
        let operation = parts.next().unwrap_or("send_notice").trim().to_owned();
        let behavior = parts.next().unwrap_or("success").trim().to_owned();
        let idempotency_key = parts
            .next()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map_or_else(
                || format!("sim-eff-{}", &Uuid::new_v4().simple().to_string()[..12]),
                ToOwned::to_owned,
            );
        effect_summaries.push(format!(
            "effect `{operation}` -> `{recipient}` ({behavior}, key={idempotency_key})"
        ));
        emit(
            &mut events,
            RuntimeEvent::ExternalEffectRequested {
                recipient,
                operation,
                behavior,
                idempotency_key,
            },
            event_tx,
        );
    }

    if prompt.contains("[SIM:ECHO_SYSTEM_PROMPT]") {
        let reply = session.system_prompt.clone();
        emit(
            &mut events,
            RuntimeEvent::TextDelta {
                text: reply.clone(),
            },
            event_tx,
        );
        emit(
            &mut events,
            RuntimeEvent::Completed {
                stop_reason: String::from("end_turn"),
            },
            event_tx,
        );
        return Ok(TurnOutcome {
            status: RunStatus::Succeeded,
            stop_reason: String::from("end_turn"),
            assistant_text: reply,
            events,
            usage: usage_payload,
            human_wait_ms,
        });
    }

    if let Some(raw_stream_tool) = find_directive(prompt, "[SIM:STREAM_TOOL:") {
        let chunks: Vec<&str> = raw_stream_tool.split('|').collect();
        let lead_chunk = chunks.first().copied().unwrap_or_default();
        let rest_chunks = if chunks.len() > 1 { &chunks[1..] } else { &[] };
        let deadline = time::Instant::now() + turn_timeout;
        let mut assistant_text = String::new();

        if !lead_chunk.is_empty() {
            assistant_text.push_str(lead_chunk);
            emit(
                &mut events,
                RuntimeEvent::TextDelta {
                    text: lead_chunk.to_owned(),
                },
                event_tx,
            );
        }

        if sleep_sim_cancellable(Duration::from_millis(120), deadline, cancel_rx).await? {
            emit(
                &mut events,
                RuntimeEvent::Cancelled {
                    reason: String::from("cancelled"),
                },
                event_tx,
            );
            return Ok(TurnOutcome {
                status: RunStatus::Cancelled,
                stop_reason: String::from("cancelled"),
                assistant_text,
                events,
                usage: None,
                human_wait_ms,
            });
        }

        emit(
            &mut events,
            RuntimeEvent::ToolObserved {
                tool_call_id: format!("sim-st-{}", &Uuid::new_v4().simple().to_string()[..8]),
                tool_name: String::from("read_file"),
                title: String::from("read_file src/lib.rs"),
                status: String::from("completed"),
                output_excerpt: String::from("simulated silent tool call"),
            },
            event_tx,
        );

        if sleep_sim_cancellable(Duration::from_millis(120), deadline, cancel_rx).await? {
            emit(
                &mut events,
                RuntimeEvent::Cancelled {
                    reason: String::from("cancelled"),
                },
                event_tx,
            );
            return Ok(TurnOutcome {
                status: RunStatus::Cancelled,
                stop_reason: String::from("cancelled"),
                assistant_text,
                events,
                usage: None,
                human_wait_ms,
            });
        }

        let mut need_tool_break = true;
        for (idx, chunk) in rest_chunks.iter().enumerate() {
            if idx > 0
                && sleep_sim_cancellable(Duration::from_millis(120), deadline, cancel_rx).await?
            {
                emit(
                    &mut events,
                    RuntimeEvent::Cancelled {
                        reason: String::from("cancelled"),
                    },
                    event_tx,
                );
                return Ok(TurnOutcome {
                    status: RunStatus::Cancelled,
                    stop_reason: String::from("cancelled"),
                    assistant_text,
                    events,
                    usage: None,
                    human_wait_ms,
                });
            }
            if !chunk.is_empty() {
                if need_tool_break
                    && !assistant_text.is_empty()
                    && !assistant_text.ends_with('\n')
                    && !chunk.starts_with('\n')
                {
                    assistant_text.push_str("\n\n");
                }
                need_tool_break = false;
                assistant_text.push_str(chunk);
                emit(
                    &mut events,
                    RuntimeEvent::TextDelta {
                        text: (*chunk).to_owned(),
                    },
                    event_tx,
                );
            }
        }

        emit(
            &mut events,
            RuntimeEvent::ContextUsageUpdated {
                used_tokens: Some(4096),
                window_tokens: Some(1_048_576),
            },
            event_tx,
        );
        emit(
            &mut events,
            RuntimeEvent::Completed {
                stop_reason: String::from("end_turn"),
            },
            event_tx,
        );
        return Ok(TurnOutcome {
            status: RunStatus::Succeeded,
            stop_reason: String::from("end_turn"),
            assistant_text,
            events,
            usage: usage_payload,
            human_wait_ms,
        });
    }

    if let Some(raw_stream) = find_directive(prompt, "[SIM:STREAM:") {
        let parts: Vec<&str> = raw_stream.split('|').collect();
        let deadline = time::Instant::now() + turn_timeout;
        let mut assistant_text = String::new();

        for (idx, part) in parts.iter().enumerate() {
            if idx > 0
                && sleep_sim_cancellable(Duration::from_millis(120), deadline, cancel_rx).await?
            {
                emit(
                    &mut events,
                    RuntimeEvent::Cancelled {
                        reason: String::from("cancelled"),
                    },
                    event_tx,
                );
                return Ok(TurnOutcome {
                    status: RunStatus::Cancelled,
                    stop_reason: String::from("cancelled"),
                    assistant_text,
                    events,
                    usage: None,
                    human_wait_ms,
                });
            }
            if !part.is_empty() {
                assistant_text.push_str(part);
                emit(
                    &mut events,
                    RuntimeEvent::TextDelta {
                        text: (*part).to_owned(),
                    },
                    event_tx,
                );
            }
        }

        emit(
            &mut events,
            RuntimeEvent::ContextUsageUpdated {
                used_tokens: Some(4096),
                window_tokens: Some(1_048_576),
            },
            event_tx,
        );
        emit(
            &mut events,
            RuntimeEvent::Completed {
                stop_reason: String::from("end_turn"),
            },
            event_tx,
        );
        return Ok(TurnOutcome {
            status: RunStatus::Succeeded,
            stop_reason: String::from("end_turn"),
            assistant_text,
            events,
            usage: usage_payload,
            human_wait_ms,
        });
    }

    if prompt.contains("[SIM:INSPECT_CAPABILITIES]") {
        let skills_summary: Vec<String> = session
            .active_skills
            .iter()
            .map(|s| format!("{}@{}", s.slug, s.version))
            .collect();
        let mcp_summary: Vec<String> = session
            .mcp_servers
            .iter()
            .map(|srv| {
                let env_keys: Vec<String> = srv
                    .env
                    .iter()
                    .map(|e| format!("{}[set:{}]", e.name, !e.value.is_empty()))
                    .collect();
                format!("{}({})", srv.name, env_keys.join(","))
            })
            .collect();
        let reply = format!(
            "[simulated capabilities] skills=[{}] mcp=[{}]",
            skills_summary.join(","),
            mcp_summary.join(";")
        );
        emit(
            &mut events,
            RuntimeEvent::TextDelta {
                text: reply.clone(),
            },
            event_tx,
        );
        emit(
            &mut events,
            RuntimeEvent::Completed {
                stop_reason: String::from("end_turn"),
            },
            event_tx,
        );
        return Ok(TurnOutcome {
            status: RunStatus::Succeeded,
            stop_reason: String::from("end_turn"),
            assistant_text: reply,
            events,
            usage: usage_payload,
            human_wait_ms,
        });
    }

    if let Some(raw_mcp) = find_directive(prompt, "[SIM:MCP_CALL:") {
        let mut parts = raw_mcp.splitn(3, '|');
        let server_name = parts.next().unwrap_or_default().trim();
        let tool_name = parts.next().unwrap_or_default().trim();
        let raw_args = parts.next().unwrap_or("{}").trim();
        let args_json: Value = serde_json::from_str(raw_args).unwrap_or_else(|_| json!({}));
        let tool_call_id = format!("sim-mcp-{}", &Uuid::new_v4().simple().to_string()[..8]);
        let eval_req = PolicyEvaluationRequest {
            session_id: dennett_session_id,
            run_id,
            tool_call_id: tool_call_id.clone(),
            tool_name: tool_name.to_owned(),
            tool_kind: String::from("other"),
            title: format!("{server_name}:{tool_name}"),
            target: args_json
                .get("project_id")
                .or_else(|| args_json.get("scope"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            locations: Vec::new(),
            raw_input: args_json.clone(),
        };
        emit(
            &mut events,
            RuntimeEvent::PermissionRequested {
                tool_call_id: tool_call_id.clone(),
                tool_name: tool_name.to_owned(),
                target: eval_req.target.clone(),
                title: eval_req.title.clone(),
            },
            event_tx,
        );
        let wait_started = time::Instant::now();
        let decision = policy.evaluate(&eval_req).await;
        human_wait_ms = human_wait_ms
            .saturating_add(u64::try_from(wait_started.elapsed().as_millis()).unwrap_or(u64::MAX));
        emit(
            &mut events,
            RuntimeEvent::PermissionResolved {
                tool_call_id: tool_call_id.clone(),
                tool_name: tool_name.to_owned(),
                target: eval_req.target.clone(),
                effect: decision.effect,
                matched_rule_id: decision.matched_rule_id,
                reason: decision.reason.clone(),
                scope: decision.scope.clone(),
                chosen_option_id: Some(if decision.effect == PolicyEffect::Allow {
                    String::from("allow_once")
                } else {
                    String::from("reject_once")
                }),
            },
            event_tx,
        );

        let reply = if decision.effect == PolicyEffect::Allow {
            let spec = session
                .mcp_servers
                .iter()
                .find(|s| s.name == server_name)
                .ok_or_else(|| DennettError::NotFound {
                    resource: String::from("mcp_server"),
                    id: server_name.to_owned(),
                })?;
            let mcp_output = invoke_mcp_tool_stdio(spec, tool_name, &args_json).await?;
            emit(
                &mut events,
                RuntimeEvent::ToolObserved {
                    tool_call_id,
                    tool_name: tool_name.to_owned(),
                    title: eval_req.title,
                    status: String::from("completed"),
                    output_excerpt: mcp_output.clone(),
                },
                event_tx,
            );
            format!("[simulated] mcp `{server_name}:{tool_name}` ALLOWED: {mcp_output}")
        } else {
            emit(
                &mut events,
                RuntimeEvent::ToolObserved {
                    tool_call_id,
                    tool_name: tool_name.to_owned(),
                    title: eval_req.title,
                    status: String::from("denied"),
                    output_excerpt: format!("denied: {}", decision.reason),
                },
                event_tx,
            );
            format!(
                "[simulated] mcp `{server_name}:{tool_name}` DENIED (policy: {})",
                decision.reason
            )
        };
        emit(
            &mut events,
            RuntimeEvent::TextDelta {
                text: reply.clone(),
            },
            event_tx,
        );
        emit(
            &mut events,
            RuntimeEvent::Completed {
                stop_reason: String::from("end_turn"),
            },
            event_tx,
        );
        return Ok(TurnOutcome {
            status: RunStatus::Succeeded,
            stop_reason: String::from("end_turn"),
            assistant_text: reply,
            events,
            usage: usage_payload,
            human_wait_ms,
        });
    }

    if let Some(raw_sub) = find_directive(prompt, "[SIM:SUBAGENT:") {
        let mut parts = raw_sub.splitn(4, '|');
        let subagent_name = parts.next().unwrap_or("helper").trim();
        let child_tool = parts.next().unwrap_or("read_file").trim();
        let child_target = parts.next().unwrap_or_default().trim();
        let child_content = parts.next().map(ToOwned::to_owned);

        let spawn_call_id = format!("sim-sub-{}", &Uuid::new_v4().simple().to_string()[..8]);
        let spawn_req = PolicyEvaluationRequest {
            session_id: dennett_session_id,
            run_id,
            tool_call_id: spawn_call_id.clone(),
            tool_name: String::from("invoke_subagent"),
            tool_kind: String::from("other"),
            title: format!("invoke_subagent {subagent_name}"),
            target: Some(subagent_name.to_owned()),
            locations: Vec::new(),
            raw_input: json!({
                "subagent_name": subagent_name,
                "child_tool": child_tool,
            }),
        };
        emit(
            &mut events,
            RuntimeEvent::PermissionRequested {
                tool_call_id: spawn_call_id.clone(),
                tool_name: String::from("invoke_subagent"),
                target: spawn_req.target.clone(),
                title: spawn_req.title.clone(),
            },
            event_tx,
        );
        let wait_started = time::Instant::now();
        let spawn_decision = policy.evaluate(&spawn_req).await;
        human_wait_ms = human_wait_ms
            .saturating_add(u64::try_from(wait_started.elapsed().as_millis()).unwrap_or(u64::MAX));
        emit(
            &mut events,
            RuntimeEvent::PermissionResolved {
                tool_call_id: spawn_call_id.clone(),
                tool_name: String::from("invoke_subagent"),
                target: spawn_req.target.clone(),
                effect: spawn_decision.effect,
                matched_rule_id: spawn_decision.matched_rule_id,
                reason: spawn_decision.reason.clone(),
                scope: spawn_decision.scope.clone(),
                chosen_option_id: Some(if spawn_decision.effect == PolicyEffect::Allow {
                    String::from("allow_once")
                } else {
                    String::from("reject_once")
                }),
            },
            event_tx,
        );

        let reply = if spawn_decision.effect != PolicyEffect::Allow {
            emit(
                &mut events,
                RuntimeEvent::ToolObserved {
                    tool_call_id: spawn_call_id,
                    tool_name: String::from("invoke_subagent"),
                    title: spawn_req.title,
                    status: String::from("denied"),
                    output_excerpt: format!("denied: {}", spawn_decision.reason),
                },
                event_tx,
            );
            format!(
                "[simulated] subagent `{subagent_name}` DENIED (policy: {})",
                spawn_decision.reason
            )
        } else if *cancel_rx.borrow() {
            emit(
                &mut events,
                RuntimeEvent::Cancelled {
                    reason: String::from("cancelled before subagent child tool"),
                },
                event_tx,
            );
            return Ok(TurnOutcome {
                status: RunStatus::Cancelled,
                stop_reason: String::from("cancelled"),
                assistant_text: String::new(),
                events,
                usage: usage_payload,
                human_wait_ms,
            });
        } else {
            let child_call_id = format!("sim-subc-{}", &Uuid::new_v4().simple().to_string()[..8]);
            let child_kind = match child_tool {
                "write_file" | "edit_file" => "edit",
                "run_command" => "execute",
                "read_file" | "list_dir" => "read",
                _ => "other",
            };
            let child_locations =
                if matches!(child_kind, "edit" | "read") && !child_target.is_empty() {
                    vec![child_target.to_owned()]
                } else {
                    Vec::new()
                };
            let child_req = PolicyEvaluationRequest {
                session_id: dennett_session_id,
                run_id,
                tool_call_id: child_call_id.clone(),
                tool_name: child_tool.to_owned(),
                tool_kind: String::from(child_kind),
                title: format!("subagent({subagent_name}) {child_tool} {child_target}"),
                target: if child_target.is_empty() {
                    None
                } else {
                    Some(child_target.to_owned())
                },
                locations: child_locations,
                raw_input: match child_kind {
                    "execute" => json!({ "CommandLine": child_target }),
                    "edit" | "read" if !child_target.is_empty() => json!({ "Path": child_target }),
                    _ => json!({ "target": child_target }),
                },
            };
            emit(
                &mut events,
                RuntimeEvent::PermissionRequested {
                    tool_call_id: child_call_id.clone(),
                    tool_name: child_tool.to_owned(),
                    target: child_req.target.clone(),
                    title: child_req.title.clone(),
                },
                event_tx,
            );
            let child_wait_started = time::Instant::now();
            let child_decision = policy.evaluate(&child_req).await;
            human_wait_ms = human_wait_ms.saturating_add(
                u64::try_from(child_wait_started.elapsed().as_millis()).unwrap_or(u64::MAX),
            );
            emit(
                &mut events,
                RuntimeEvent::PermissionResolved {
                    tool_call_id: child_call_id.clone(),
                    tool_name: child_tool.to_owned(),
                    target: child_req.target.clone(),
                    effect: child_decision.effect,
                    matched_rule_id: child_decision.matched_rule_id,
                    reason: child_decision.reason.clone(),
                    scope: child_decision.scope.clone(),
                    chosen_option_id: Some(if child_decision.effect == PolicyEffect::Allow {
                        String::from("allow_once")
                    } else {
                        String::from("reject_once")
                    }),
                },
                event_tx,
            );
            if child_decision.effect == PolicyEffect::Allow {
                if child_tool == "write_file" && !child_target.is_empty() {
                    let p = PathBuf::from(child_target);
                    let resolved = if p.is_absolute() {
                        p
                    } else {
                        PathBuf::from(&session.workspace_cwd).join(p)
                    };
                    if let Some(parent) = resolved.parent() {
                        let _ = fs::create_dir_all(parent);
                    }
                    let body = child_content.as_deref().unwrap_or("subagent output");
                    fs::write(&resolved, body).map_err(|err| DennettError::Internal {
                        detail: format!(
                            "simulated subagent write_file failed for `{}`: {err}",
                            resolved.display()
                        ),
                    })?;
                }
                emit(
                    &mut events,
                    RuntimeEvent::ToolObserved {
                        tool_call_id: child_call_id,
                        tool_name: child_tool.to_owned(),
                        title: child_req.title,
                        status: String::from("completed"),
                        output_excerpt: format!("allowed: {}", child_decision.reason),
                    },
                    event_tx,
                );
                format!(
                    "[simulated] subagent `{subagent_name}` executed `{child_tool}` ALLOWED for `{child_target}`"
                )
            } else {
                emit(
                    &mut events,
                    RuntimeEvent::ToolObserved {
                        tool_call_id: child_call_id,
                        tool_name: child_tool.to_owned(),
                        title: child_req.title,
                        status: String::from("denied"),
                        output_excerpt: format!("denied: {}", child_decision.reason),
                    },
                    event_tx,
                );
                format!(
                    "[simulated] subagent `{subagent_name}` child `{child_tool}` DENIED (policy: {})",
                    child_decision.reason
                )
            }
        };
        emit(
            &mut events,
            RuntimeEvent::TextDelta {
                text: reply.clone(),
            },
            event_tx,
        );
        emit(
            &mut events,
            RuntimeEvent::Completed {
                stop_reason: String::from("end_turn"),
            },
            event_tx,
        );
        return Ok(TurnOutcome {
            status: RunStatus::Succeeded,
            stop_reason: String::from("end_turn"),
            assistant_text: reply,
            events,
            usage: usage_payload,
            human_wait_ms,
        });
    }

    if let Some(raw_ctrl) = find_directive(prompt, "[SIM:CONTROL_MCP:") {
        let (tool_name, args_json) = match raw_ctrl.split_once('|') {
            Some((name, raw_json)) => {
                let parsed = serde_json::from_str::<Value>(raw_json.trim())
                    .unwrap_or_else(|_| json!({}));
                (name.trim().to_owned(), parsed)
            }
            None => (raw_ctrl.trim().to_owned(), json!({})),
        };
        let tool_call_id = format!("sim-ctrl-{}", &Uuid::new_v4().simple().to_string()[..8]);
        let eval_req = PolicyEvaluationRequest {
            session_id: dennett_session_id,
            run_id,
            tool_call_id: tool_call_id.clone(),
            tool_name: tool_name.clone(),
            tool_kind: String::from("other"),
            title: format!("dennett-control:{tool_name}"),
            target: args_json
                .get("slug")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            locations: Vec::new(),
            raw_input: args_json.clone(),
        };
        emit(
            &mut events,
            RuntimeEvent::PermissionRequested {
                tool_call_id: tool_call_id.clone(),
                tool_name: tool_name.clone(),
                target: eval_req.target.clone(),
                title: eval_req.title.clone(),
            },
            event_tx,
        );
        let wait_started = time::Instant::now();
        let decision = policy.evaluate(&eval_req).await;
        human_wait_ms = human_wait_ms
            .saturating_add(u64::try_from(wait_started.elapsed().as_millis()).unwrap_or(u64::MAX));
        emit(
            &mut events,
            RuntimeEvent::PermissionResolved {
                tool_call_id: tool_call_id.clone(),
                tool_name: tool_name.clone(),
                target: eval_req.target.clone(),
                effect: decision.effect,
                matched_rule_id: decision.matched_rule_id,
                reason: decision.reason.clone(),
                scope: decision.scope.clone(),
                chosen_option_id: Some(if decision.effect == PolicyEffect::Allow {
                    String::from("allow_once")
                } else {
                    String::from("reject_once")
                }),
            },
            event_tx,
        );

        let reply = if decision.effect == PolicyEffect::Allow {
            let control_spec = session
                .mcp_servers
                .iter()
                .find(|s| s.name == "dennett-control")
                .ok_or_else(|| DennettError::Internal {
                    detail: String::from("dennett-control MCP server spec not attached to session"),
                })?;
            let mcp_output = invoke_mcp_tool_stdio(control_spec, &tool_name, &args_json).await?;
            emit(
                &mut events,
                RuntimeEvent::ToolObserved {
                    tool_call_id,
                    tool_name: tool_name.clone(),
                    title: eval_req.title,
                    status: String::from("completed"),
                    output_excerpt: mcp_output.clone(),
                },
                event_tx,
            );
            format!("[simulated] {tool_name} ALLOWED: {mcp_output}")
        } else {
            emit(
                &mut events,
                RuntimeEvent::ToolObserved {
                    tool_call_id,
                    tool_name: tool_name.clone(),
                    title: eval_req.title,
                    status: String::from("denied"),
                    output_excerpt: format!("denied: {}", decision.reason),
                },
                event_tx,
            );
            format!(
                "[simulated] tool `{tool_name}` DENIED (policy: {})",
                decision.reason
            )
        };

        emit(
            &mut events,
            RuntimeEvent::TextDelta {
                text: reply.clone(),
            },
            event_tx,
        );
        emit(
            &mut events,
            RuntimeEvent::Completed {
                stop_reason: String::from("end_turn"),
            },
            event_tx,
        );
        return Ok(TurnOutcome {
            status: RunStatus::Succeeded,
            stop_reason: String::from("end_turn"),
            assistant_text: reply,
            events,
            usage: usage_payload,
            human_wait_ms,
        });
    }

    if let Some(raw_event_args) = find_directive(prompt, "[SIM:EVENT:CREATE ") {
        let args_json = parse_sim_event_create_args(raw_event_args);
        let tool_call_id = format!("sim-evt-{}", &Uuid::new_v4().simple().to_string()[..8]);
        let title_hint = args_json
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("event");
        let eval_req = PolicyEvaluationRequest {
            session_id: dennett_session_id,
            run_id,
            tool_call_id: tool_call_id.clone(),
            tool_name: String::from("dennett_event_create"),
            tool_kind: String::from("other"),
            title: format!("dennett_event_create {title_hint}"),
            target: args_json
                .get("project_id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            locations: Vec::new(),
            raw_input: args_json.clone(),
        };
        emit(
            &mut events,
            RuntimeEvent::PermissionRequested {
                tool_call_id: tool_call_id.clone(),
                tool_name: String::from("dennett_event_create"),
                target: eval_req.target.clone(),
                title: eval_req.title.clone(),
            },
            event_tx,
        );
        let wait_started = time::Instant::now();
        let decision = policy.evaluate(&eval_req).await;
        human_wait_ms = human_wait_ms
            .saturating_add(u64::try_from(wait_started.elapsed().as_millis()).unwrap_or(u64::MAX));
        emit(
            &mut events,
            RuntimeEvent::PermissionResolved {
                tool_call_id: tool_call_id.clone(),
                tool_name: String::from("dennett_event_create"),
                target: eval_req.target.clone(),
                effect: decision.effect,
                matched_rule_id: decision.matched_rule_id,
                reason: decision.reason.clone(),
                scope: decision.scope.clone(),
                chosen_option_id: Some(if decision.effect == PolicyEffect::Allow {
                    String::from("allow_once")
                } else {
                    String::from("reject_once")
                }),
            },
            event_tx,
        );

        let reply = if decision.effect == PolicyEffect::Allow {
            let control_spec = session
                .mcp_servers
                .iter()
                .find(|s| s.name == "dennett-control")
                .ok_or_else(|| DennettError::Internal {
                    detail: String::from("dennett-control MCP server spec not attached to session"),
                })?;
            let mcp_output =
                invoke_mcp_tool_stdio(control_spec, "dennett_event_create", &args_json).await?;
            emit(
                &mut events,
                RuntimeEvent::ToolObserved {
                    tool_call_id,
                    tool_name: String::from("dennett_event_create"),
                    title: eval_req.title,
                    status: String::from("completed"),
                    output_excerpt: mcp_output.clone(),
                },
                event_tx,
            );
            format!("[simulated] dennett_event_create ALLOWED: {mcp_output}")
        } else {
            emit(
                &mut events,
                RuntimeEvent::ToolObserved {
                    tool_call_id,
                    tool_name: String::from("dennett_event_create"),
                    title: eval_req.title,
                    status: String::from("denied"),
                    output_excerpt: format!("denied: {}", decision.reason),
                },
                event_tx,
            );
            format!(
                "[simulated] tool `dennett_event_create` DENIED (policy: {})",
                decision.reason
            )
        };

        emit(
            &mut events,
            RuntimeEvent::TextDelta {
                text: reply.clone(),
            },
            event_tx,
        );
        emit(
            &mut events,
            RuntimeEvent::Completed {
                stop_reason: String::from("end_turn"),
            },
            event_tx,
        );
        return Ok(TurnOutcome {
            status: RunStatus::Succeeded,
            stop_reason: String::from("end_turn"),
            assistant_text: reply,
            events,
            usage: usage_payload,
            human_wait_ms,
        });
    }

    if let Some(body) = find_directive(prompt, "[SIM:TOOL:") {
        let mut parts = body.splitn(3, ':');
        let tool_name = parts.next().unwrap_or("unknown").trim().to_owned();
        let arg_tail = parts.next().unwrap_or_default();
        let extra_colon = parts.next();
        let combined_arg = match extra_colon {
            Some(c) => format!("{arg_tail}:{c}"),
            None => arg_tail.to_owned(),
        };

        let (target_str, file_content) = match combined_arg.split_once('|') {
            Some((t, c)) => (t.trim().to_owned(), Some(c.to_owned())),
            None => (combined_arg.trim().to_owned(), None),
        };

        let tool_call_id = format!("sim-call-{}", &Uuid::new_v4().simple().to_string()[..8]);
        let tool_kind = match tool_name.as_str() {
            "write_file" | "edit_file" => "edit",
            "run_command" => "execute",
            "read_file" | "list_dir" | "find_by_name" | "grep_search" => "read",
            _ => "other",
        };
        let locations = if matches!(tool_kind, "edit" | "read") && !target_str.is_empty() {
            vec![target_str.clone()]
        } else {
            Vec::new()
        };
        let raw_input = match tool_kind {
            "execute" => json!({ "CommandLine": target_str }),
            "edit" if !target_str.is_empty() => match file_content {
                Some(ref content) => json!({ "Path": target_str, "content": content }),
                None => json!({ "Path": target_str }),
            },
            "read" if !target_str.is_empty() => json!({ "Path": target_str }),
            _ if tool_name.starts_with("memory_") && !target_str.is_empty() => {
                json!({ "scope": target_str })
            }
            _ => json!({
                "tool": tool_name,
                "target": target_str,
            }),
        };
        let eval_req = PolicyEvaluationRequest {
            session_id: dennett_session_id,
            run_id,
            tool_call_id: tool_call_id.clone(),
            tool_name: tool_name.clone(),
            tool_kind: String::from(tool_kind),
            title: format!("{tool_name} {target_str}"),
            target: if target_str.is_empty() {
                None
            } else {
                Some(target_str.clone())
            },
            locations,
            raw_input,
        };

        emit(
            &mut events,
            RuntimeEvent::PermissionRequested {
                tool_call_id: tool_call_id.clone(),
                tool_name: tool_name.clone(),
                target: eval_req.target.clone(),
                title: eval_req.title.clone(),
            },
            event_tx,
        );

        let wait_started = time::Instant::now();
        let maybe_decision = {
            let mut eval_fut = policy.evaluate(&eval_req);
            let mut cancel_open = true;
            loop {
                tokio::select! {
                    biased;
                    res = &mut eval_fut => break Some(res),
                    changed = cancel_rx.changed(), if cancel_open => {
                        if changed.is_ok() {
                            if *cancel_rx.borrow() {
                                break None;
                            }
                        } else {
                            cancel_open = false;
                        }
                    }
                }
            }
        };
        human_wait_ms = human_wait_ms
            .saturating_add(u64::try_from(wait_started.elapsed().as_millis()).unwrap_or(u64::MAX));

        let Some(decision) = maybe_decision else {
            emit(
                &mut events,
                RuntimeEvent::PermissionResolved {
                    tool_call_id,
                    tool_name,
                    target: eval_req.target,
                    effect: PolicyEffect::Deny,
                    matched_rule_id: None,
                    reason: String::from(
                        "abandoned because run was cancelled during approval wait",
                    ),
                    scope: format!("session:{dennett_session_id}"),
                    chosen_option_id: None,
                },
                event_tx,
            );
            emit(
                &mut events,
                RuntimeEvent::Cancelled {
                    reason: String::from("cancelled"),
                },
                event_tx,
            );
            return Ok(TurnOutcome {
                status: RunStatus::Cancelled,
                stop_reason: String::from("cancelled"),
                assistant_text: String::new(),
                events,
                usage: None,
                human_wait_ms,
            });
        };

        if *cancel_rx.borrow() {
            emit(
                &mut events,
                RuntimeEvent::PermissionResolved {
                    tool_call_id,
                    tool_name,
                    target: eval_req.target,
                    effect: PolicyEffect::Deny,
                    matched_rule_id: decision.matched_rule_id,
                    reason: String::from("late approval rejected after cancellation"),
                    scope: decision.scope,
                    chosen_option_id: None,
                },
                event_tx,
            );
            emit(
                &mut events,
                RuntimeEvent::Cancelled {
                    reason: String::from("cancelled"),
                },
                event_tx,
            );
            return Ok(TurnOutcome {
                status: RunStatus::Cancelled,
                stop_reason: String::from("cancelled"),
                assistant_text: String::new(),
                events,
                usage: None,
                human_wait_ms,
            });
        }

        let chosen_option = match decision.effect {
            PolicyEffect::Allow => Some(String::from("allow_once")),
            PolicyEffect::Deny => Some(String::from("reject_once")),
        };
        emit(
            &mut events,
            RuntimeEvent::PermissionResolved {
                tool_call_id: tool_call_id.clone(),
                tool_name: tool_name.clone(),
                target: eval_req.target.clone(),
                effect: decision.effect,
                matched_rule_id: decision.matched_rule_id,
                reason: decision.reason.clone(),
                scope: decision.scope.clone(),
                chosen_option_id: chosen_option,
            },
            event_tx,
        );

        let reply = if decision.effect == PolicyEffect::Allow {
            if tool_name == "write_file" {
                let resolved_target = {
                    let p = PathBuf::from(&target_str);
                    if p.is_absolute() {
                        p
                    } else {
                        PathBuf::from(&session.workspace_cwd).join(p)
                    }
                };
                if let Some(parent) = resolved_target.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let data = file_content.as_deref().unwrap_or("simulated write");
                fs::write(&resolved_target, data).map_err(|err| DennettError::Internal {
                    detail: format!(
                        "simulated write_file failed for `{}`: {err}",
                        resolved_target.display()
                    ),
                })?;
            }
            emit(
                &mut events,
                RuntimeEvent::ToolObserved {
                    tool_call_id,
                    tool_name: tool_name.clone(),
                    title: eval_req.title,
                    status: String::from("completed"),
                    output_excerpt: format!("allowed: {}", decision.reason),
                },
                event_tx,
            );
            format!(
                "[simulated] tool `{tool_name}` ALLOWED for `{target_str}` (policy: {})",
                decision.reason
            )
        } else {
            emit(
                &mut events,
                RuntimeEvent::ToolObserved {
                    tool_call_id,
                    tool_name: tool_name.clone(),
                    title: eval_req.title,
                    status: String::from("denied"),
                    output_excerpt: format!("denied: {}", decision.reason),
                },
                event_tx,
            );
            format!(
                "[simulated] tool `{tool_name}` DENIED for `{target_str}` (policy: {})",
                decision.reason
            )
        };

        emit(
            &mut events,
            RuntimeEvent::TextDelta {
                text: reply.clone(),
            },
            event_tx,
        );
        emit(
            &mut events,
            RuntimeEvent::Completed {
                stop_reason: String::from("end_turn"),
            },
            event_tx,
        );
        return Ok(TurnOutcome {
            status: RunStatus::Succeeded,
            stop_reason: String::from("end_turn"),
            assistant_text: reply,
            events,
            usage: usage_payload,
            human_wait_ms,
        });
    }

    let model_label = session
        .model_preference
        .as_deref()
        .unwrap_or("sim-model-alpha");
    let thinking_label = session
        .thinking_level
        .map_or("default", ThinkingLevel::as_str);
    let history_count = session.turn_history.len();
    let history_hint = session
        .turn_history
        .last()
        .cloned()
        .unwrap_or_else(|| String::from("none"));
    let write_mode = format!("{:?}", session.write_policy);
    let effects_note = if effect_summaries.is_empty() {
        String::new()
    } else {
        format!(" | effects=[{}]", effect_summaries.join("; "))
    };
    let reply = format!(
        "[simulated provider_session={} model={} thinking={} write_policy={} history_entries={} last_history={}] prompt_ack: {} | system_prompt_len={}{}",
        session.provider_session_id,
        model_label,
        thinking_label,
        write_mode,
        history_count,
        history_hint,
        prompt,
        session.system_prompt.len(),
        effects_note
    );

    emit(
        &mut events,
        RuntimeEvent::TextDelta {
            text: reply.clone(),
        },
        event_tx,
    );
    emit(
        &mut events,
        RuntimeEvent::Completed {
            stop_reason: String::from("end_turn"),
        },
        event_tx,
    );

    Ok(TurnOutcome {
        status: RunStatus::Succeeded,
        stop_reason: String::from("end_turn"),
        assistant_text: reply,
        events,
        usage: usage_payload,
        human_wait_ms,
    })
}

fn parse_sim_event_create_args(raw: &str) -> Value {
    let mut map = Map::new();
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b'|') {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let key_start = i;
        while i < bytes.len() && bytes[i] != b'=' && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let key = raw[key_start..i].trim();
        if key.is_empty() || i >= bytes.len() || bytes[i] != b'=' {
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            continue;
        }
        i += 1; // skip '='
        let val = if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
            let quote = bytes[i];
            i += 1;
            let val_start = i;
            while i < bytes.len() && bytes[i] != quote {
                i += 1;
            }
            let s = &raw[val_start..i];
            if i < bytes.len() && bytes[i] == quote {
                i += 1;
            }
            s.to_owned()
        } else {
            let val_start = i;
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'|' {
                i += 1;
            }
            raw[val_start..i].to_owned()
        };

        match key {
            "interval_secs" | "hour" | "minute" | "max_turns" | "max_tokens" | "max_wall_secs" => {
                if let Ok(num) = val.parse::<u64>() {
                    map.insert(key.to_owned(), Value::from(num));
                }
            }
            "weekdays" => {
                let items: Vec<Value> = val
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| Value::from(s.to_owned()))
                    .collect();
                map.insert(String::from("weekdays"), Value::Array(items));
            }
            _ => {
                map.insert(key.to_owned(), Value::from(val));
            }
        }
    }

    if !map.contains_key("trigger_kind") {
        let inferred = if map.contains_key("interval_secs") {
            "interval"
        } else if map.contains_key("weekdays") {
            "weekly_local_time"
        } else if map.contains_key("hour") {
            "daily_local_time"
        } else if map.contains_key("fire_at") {
            "once_at"
        } else if map.contains_key("expected_signal_name") {
            "wait_for_signal"
        } else {
            "immediate"
        };
        map.insert(String::from("trigger_kind"), Value::from(inferred));
    }

    Value::Object(map)
}

#[expect(
    clippy::too_many_lines,
    reason = "spawns stdio MCP subprocess and executes initialize + tools/call JSON-RPC roundtrip with timeout"
)]
async fn invoke_mcp_tool_stdio(
    spec: &RuntimeMcpServerSpec,
    tool_name: &str,
    arguments: &Value,
) -> Result<String, DennettError> {
    let mut cmd = Command::new(&spec.command);
    cmd.args(&spec.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    for entry in &spec.env {
        cmd.env(&entry.name, &entry.value);
    }

    let mut child = cmd.spawn().map_err(|err| DennettError::RuntimeProtocol {
        code: None,
        detail: redact_secrets(
            &format!("failed spawning MCP server `{}`: {err}", spec.name),
            &[],
        ),
    })?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| DennettError::RuntimeProtocol {
            code: None,
            detail: String::from("MCP child stdin missing"),
        })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| DennettError::RuntimeProtocol {
            code: None,
            detail: String::from("MCP child stdout missing"),
        })?;
    let mut reader = BufReader::new(stdout);

    let init_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "dennett-simulated-runtime", "version": "0.1.0" }
        }
    });
    let call_req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": tool_name,
            "arguments": arguments,
        }
    });

    let rpc_res = time::timeout(Duration::from_secs(15), async {
        let mut line = serde_json::to_string(&init_req).unwrap_or_default();
        line.push('\n');
        stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|err| DennettError::RuntimeProtocol {
                code: None,
                detail: format!("MCP write initialize failed: {err}"),
            })?;
        stdin
            .flush()
            .await
            .map_err(|err| DennettError::RuntimeProtocol {
                code: None,
                detail: format!("MCP flush initialize failed: {err}"),
            })?;

        let mut init_resp = String::new();
        let n = reader.read_line(&mut init_resp).await.map_err(|err| {
            DennettError::RuntimeProtocol {
                code: None,
                detail: format!("MCP read initialize response failed: {err}"),
            }
        })?;
        if n == 0 {
            return Err(DennettError::RuntimeProtocol {
                code: None,
                detail: String::from("MCP server closed stdout during initialize"),
            });
        }

        let mut call_line = serde_json::to_string(&call_req).unwrap_or_default();
        call_line.push('\n');
        stdin.write_all(call_line.as_bytes()).await.map_err(|err| {
            DennettError::RuntimeProtocol {
                code: None,
                detail: format!("MCP write tools/call failed: {err}"),
            }
        })?;
        stdin
            .flush()
            .await
            .map_err(|err| DennettError::RuntimeProtocol {
                code: None,
                detail: format!("MCP flush tools/call failed: {err}"),
            })?;

        let mut call_resp = String::new();
        let n2 = reader.read_line(&mut call_resp).await.map_err(|err| {
            DennettError::RuntimeProtocol {
                code: None,
                detail: format!("MCP read tools/call response failed: {err}"),
            }
        })?;
        if n2 == 0 {
            return Err(DennettError::RuntimeProtocol {
                code: None,
                detail: String::from("MCP server closed stdout during tools/call"),
            });
        }
        let parsed: Value = serde_json::from_str(call_resp.trim()).map_err(|err| {
            DennettError::RuntimeProtocol {
                code: None,
                detail: format!("MCP server returned malformed JSON: {err}"),
            }
        })?;
        if let Some(err_obj) = parsed.get("error") {
            return Err(DennettError::RuntimeProtocol {
                code: err_obj.get("code").and_then(Value::as_i64),
                detail: redact_secrets(&err_obj.to_string(), &[]),
            });
        }
        let result = parsed.get("result").cloned().unwrap_or(Value::Null);
        let text = result
            .get("content")
            .and_then(Value::as_array)
            .and_then(|arr| arr.first())
            .and_then(|item| item.get("text"))
            .and_then(Value::as_str)
            .map_or_else(|| result.to_string(), ToOwned::to_owned);
        Ok(redact_secrets(&text, &[]))
    })
    .await
    .map_err(|_| DennettError::Timeout {
        operation: format!("mcp:{tool_name}"),
    })?;

    drop(stdin);
    let _ = child.kill().await;
    let _ = child.wait().await;
    rpc_res
}

const _: () = {
    const fn assert_send_sync<T: Send + Sync + 'static>() {}
    assert_send_sync::<SimulatedRuntime>();
};
