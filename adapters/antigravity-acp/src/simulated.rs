//! Deterministic CI test-double implementation of [`AgentRuntimePort`].
//!
//! Strictly sets `adapter_id = "simulated-ci-runtime"` and `is_simulated = true` in every
//! descriptor, binding, and outcome so it can NEVER masquerade as a live `agy acp` runtime.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use dennett_contracts::{
    DennettError, PolicyEffect, RunStatus, RuntimeAuthProfile, SessionId, ThinkingLevel,
    WorkspaceWritePolicy,
};
use serde_json::json;
use tokio::sync::{Mutex, mpsc, watch};
use tokio::time;
use uuid::Uuid;

use crate::port::{
    AgentRuntimePort, BoxFuture, PolicyEvaluationRequest, PolicyEvaluator, RuntimeDescriptor,
    RuntimeEvent, SessionStartOutcome, SessionStartRequest, TurnOutcome, TurnRequest,
};
use crate::profile::{normalize_workspace_path, workspace_paths_match};

/// Canonical adapter identifier for the deterministic CI test double.
pub const SIMULATED_ADAPTER_ID: &str = "simulated-ci-runtime";

#[derive(Debug, Clone)]
struct SimSessionState {
    provider_session_id: String,
    workspace_cwd: String,
    system_prompt: String,
    model_preference: Option<String>,
    thinking_level: Option<ThinkingLevel>,
    write_policy: WorkspaceWritePolicy,
    turn_history: Vec<String>,
}

/// Deterministic [`AgentRuntimePort`] test double for offline CI integration suites.
///
/// Supports explicit test directives inside prompts:
/// - `[SIM:TOOL:write_file:<relative_or_abs_path>|<content>]` — invokes real [`PolicyEvaluator`],
///   writes the file once ONLY if allowed (zero double execution), and denies cleanly if blocked.
/// - `[SIM:TOOL:run_command:<command>]` — invokes real [`PolicyEvaluator`] for shell execution.
/// - `[SIM:TOOL:mcp:<tool_name>|<target>]` — invokes real [`PolicyEvaluator`] for MCP tool calls.
/// - `[SIM:SLOW:<millis>]` — sleeps incrementally while watching `cancel_rx` to verify live Stop
///   and strict rejection of late permission requests after cancel.
/// - `[SIM:ERROR:<message>]` — emits partial `TextDelta` then returns `Err(RuntimeProtocol)` so
///   callers prove partial output NEVER becomes `Succeeded`.
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
                if !binding.is_simulated {
                    let msg = String::from(
                        "cannot resume live antigravity-acp binding inside simulated runtime",
                    );
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
                } else if !workspace_paths_match(&binding.workspace_cwd, &target_cwd) {
                    let msg = format!(
                        "workspace cwd mismatch (bound `{}`, requested `{target_cwd}`)",
                        binding.workspace_cwd
                    );
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
                        // Live session resume: apply updated model/thinking without creating a new Agent!
                        existing.system_prompt = req.system_prompt;
                        existing.model_preference = req.model_preference;
                        existing.thinking_level = req.thinking_level;
                        existing.write_policy = req.workspace_write_policy;
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
                    provider_session_id: provider_session_id.clone(),
                    workspace_cwd: target_cwd.clone(),
                    system_prompt: req.system_prompt,
                    model_preference: req.model_preference,
                    thinking_level: req.thinking_level,
                    write_policy: req.workspace_write_policy,
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

#[expect(
    clippy::too_many_lines,
    reason = "simulated directive dispatcher keeps all deterministic CI paths in one clear function"
)]
async fn execute_sim_turn(
    session: &SimSessionState,
    dennett_session_id: SessionId,
    run_id: dennett_contracts::RunId,
    prompt_text: &str,
    policy: &Arc<dyn PolicyEvaluator>,
    event_tx: Option<&mpsc::Sender<RuntimeEvent>>,
    cancel_rx: &mut watch::Receiver<bool>,
) -> Result<TurnOutcome, DennettError> {
    let mut events = Vec::new();
    let prompt = prompt_text.trim();

    if let Some(rest) = prompt.strip_prefix("[SIM:ERROR:") {
        let detail = rest.trim_end_matches(']').trim();
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

    if let Some(rest) = prompt.strip_prefix("[SIM:SLOW:") {
        let millis: u64 = rest.trim_end_matches(']').trim().parse().unwrap_or(5_000);
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
                    });
                }
            }
            () = time::sleep(sleep_dur) => {}
        }
    }

    if let Some(rest) = prompt.strip_prefix("[SIM:TOOL:") {
        let body = rest.trim_end_matches(']');
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
            "edit" | "read" if !target_str.is_empty() => json!({ "Path": target_str }),
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

        let decision = policy.evaluate(&eval_req);
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
            usage: Some(json!({ "simulated": true })),
        });
    }

    let model_label = session.model_preference.as_deref().unwrap_or("sim-default");
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
    let reply = format!(
        "[simulated provider_session={} model={} thinking={} write_policy={} history_entries={} last_history={}] prompt_ack: {} | system_prompt_len={}",
        session.provider_session_id,
        model_label,
        thinking_label,
        write_mode,
        history_count,
        history_hint,
        prompt,
        session.system_prompt.len()
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
        usage: Some(json!({ "simulated": true })),
    })
}

const _: () = {
    const fn assert_send_sync<T: Send + Sync + 'static>() {}
    assert_send_sync::<SimulatedRuntime>();
};
