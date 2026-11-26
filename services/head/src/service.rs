//! Canonical Dennett Head control service (`HeadService`).
//!
//! Architectural boundaries (Architecture 80 §3.1, §6, §11; 81 §7; 82 §§1–3, 7–9, 11):
//! - Owns `dennett_control` PostgreSQL state exclusively.
//! - Communicates with standalone `dennett-memoryd` strictly over HTTP via [`MemoryClient`] —
//!   NEVER links `dennett-memory-core`.
//! - Spawns `dennett-memory-mcp` for the agent runtime passing ONLY `DENNETT_MEMORY_URL` and
//!   scoped `DENNETT_MEMORY_TOKEN` (zero PostgreSQL credentials, zero Head DB access).
//! - Survives `dennett-memoryd` outages gracefully (V02): reports honest `DegradedUnreachable`,
//!   keeps Stop/cancellation completely unblocked, queues memory commits into `memory_commit_outbox`
//!   with deterministic idempotency keys, and never fabricates confident memory answers.

use core::fmt::{self, Write as _};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex as StdMutex, RwLock as StdRwLock};
use std::time::{Duration, Instant};

use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod};
use dennett_contracts::redact::{REDACTED_PLACEHOLDER, redact_secrets_owned};
use dennett_contracts::{
    AgentConfigHistoryResponse, AgentConfigVersionId, AgentId, AgentRecord, ArtifactCompleteness,
    ArtifactId, ArtifactLifecycleState, ArtifactRecord, CancelRunResponse, CommitMemoryNoteRequest,
    CommitMemoryNoteResponse, CorrelationId, CreateAgentRequest, CreateArtifactRequest,
    CreateNoteRequest, CreatePolicyRuleRequest, CreateProjectRequest, CreateSessionRequest,
    DennettError, DiagnosticsResponse, DispatchEffectRequest, EffectClaimRecord,
    EnqueueFollowupRequest, ExternalEffectStatus, FlushOutboxResponse, FollowupId, FollowupStatus,
    ForgetTargetType, HeadHealth, HeadMemorySearchRequest, HeadMemorySearchResponse,
    IdempotencyKey, ListAgentsResponse, ListArtifactsResponse, ListOutboxResponse,
    ListPolicyRulesResponse, ListProjectFilesResponse, ListProjectsResponse, ListSessionsResponse,
    LiveRunResponse, LiveRunSnapshot, MemoryCapability, MemoryOutboxItem, MemoryOutboxStatus,
    MemoryScope, MemoryStatus, MessageId, MessageRecord, MessageRole, OrderedOutboxItem,
    OrderedOutboxStatus, OutboxItemId, PermissionDecisionRecord, PermissionRequestId, PolicyRule,
    PolicyRuleId, PrincipalId, ProfileMode, ProjectId, ProjectLifecycleState, ProjectRecord,
    ProviderConnectionId, ResumeFollowupsResponse, RunId, RunRecord, RunStatus, RuntimeAuthProfile,
    RuntimeBinding, RuntimeBindingId, RuntimeCatalogResponse, SearchNotesRequest,
    SendMessageRequest, SendMessageResponse, ServiceKind, SessionDetailResponse,
    SessionFollowupRecord, SessionId, SessionKind, SessionRecord, SessionStatus, Sha256Hex,
    SimEffectBehavior, SteerSessionRequest, TaskId, TimestampMs, UpdateAgentConfigRequest,
    UpdateFollowupRequest, UpdateProjectRequest, UpdateSessionRequest, WorkspaceWritePolicy,
};
use dennett_memory_client::MemoryClient;
use dennett_runtime_antigravity::{
    AgentRuntimePort, PolicyEvaluationRequest, RuntimeEvent, RuntimeMcpEnvEntry,
    RuntimeMcpServerSpec, SessionStartRequest, TurnRequest,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::{Mutex, mpsc, oneshot, watch};
use tokio::time;
use tokio_postgres::NoTls;

use crate::approvals::{
    ApprovalResolutionSignal, CapturedPermissionDecision, DEFAULT_APPROVAL_WAIT_TIMEOUT,
    InteractivePolicyEvaluator,
};
use crate::capabilities::format_session_skills_prompt_index;
use crate::connections::CachedDynamicRuntime;
use crate::effects::{
    ExternalEffectPort, SIMULATED_EXTERNAL_CONNECTOR_ID, SimulatedExternalConnector,
};
use crate::events::{
    ControlEventAppend, PendingControlEvent, append_control_event_in_tx,
    append_control_events_batch_in_tx, enqueue_ordered_outbox_in_tx,
};
use crate::memory_auth::{ControlTokenSigner, MemoryTokenSigner};
use crate::migrations::{query_control_schema_version, run_control_migrations, sha256_hex};
use crate::pg_target::ValidatedControlPgTarget;
use crate::policy::{
    PolicyContext, PolicyEngine, PolicyEvaluationDecision, resolve_within_workspace,
};
use crate::secrets::{SecretCipher, VaultMasterKey, default_vault_dir};
use crate::store::{
    AGENT_JOIN_SELECT, AGENT_MESSAGE_SELECT_COLS, EFFECT_CLAIM_SELECT_COLS,
    ORDERED_OUTBOX_SELECT_COLS, PROJECT_SELECT_COLS, RUN_SELECT_COLS, SESSION_FOLLOWUP_SELECT_COLS,
    SESSION_SELECT_COLS, append_audit_event, decode_agent_join_row, decode_agent_message_row,
    decode_audit_event_row, decode_config_version_row, decode_effect_claim_row, decode_message_row,
    decode_ordered_outbox_row, decode_permission_decision_row, decode_policy_rule_row,
    decode_project_row, decode_run_row, decode_runtime_binding_row, decode_session_followup_row,
    decode_session_row, fetch_active_session_followups, fetch_agent, fetch_all_runtime_bindings,
    fetch_policy_rules, fetch_project, fetch_runtime_binding, fetch_session,
    fetch_session_artifacts, fetch_session_messages, fetch_session_permission_decisions,
    fetch_session_runs, is_pg_unique_violation, map_pg_err, next_stream_seq, truncate_utf8,
};
use crate::workspace_files::{
    clean_display_path, is_symlink_or_reparse_point, list_project_workspace_files,
    validate_safe_workspace_root,
};

/// Current implementation milestone label advertised in [`HeadHealth`].
pub const HEAD_STAGE_LABEL: &str = "D3";
/// Default PostgreSQL pool size for `dennett-head`.
pub const DEFAULT_CONTROL_POOL_SIZE: usize = 16;
/// Default timeout for standard control-plane PostgreSQL queries.
pub const DEFAULT_PG_OP_TIMEOUT: Duration = Duration::from_secs(10);
/// Shorter timeout for `/healthz` and `memoryd` reachability checks.
pub const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_secs(3);
/// Default wall-clock timeout for a single runtime turn.
pub const DEFAULT_TURN_TIMEOUT: Duration = Duration::from_secs(120);
/// Cache TTL for dynamic `GET /v1/runtime/catalog` responses.
pub const RUNTIME_CATALOG_CACHE_TTL: Duration = Duration::from_secs(300);

/// Destination key used in `ordered_outbox` for unified durable memory note delivery (`D2-OVER-01`).
pub const MEMORY_OUTBOX_DESTINATION: &str = "memoryd";
/// Generous transient retry cap for durable memory outbox delivery while `memoryd` is unreachable.
const MEMORY_OUTBOX_MAX_TRANSIENT_ATTEMPTS: i32 = 1_000_000;

/// Optional extra configuration for [`HeadService::new_with_extras`].
#[derive(Clone, Default)]
pub struct HeadServiceExtras {
    /// Optional HMAC-SHA256 signer for dynamically issuing scoped `dnt1.` memory tokens per project.
    pub memory_hmac_signer: Option<MemoryTokenSigner>,
    /// Optional path to the `dennett-control-mcp` stdio MCP adapter binary.
    pub control_mcp_bin: Option<PathBuf>,
    /// Optional HMAC-SHA256 signer for issuing project-scoped `dct1.` control tokens to `dennett-control-mcp`.
    /// Defaults to a fresh process-local CSPRNG signer when omitted.
    pub control_hmac_signer: Option<ControlTokenSigner>,
    /// Root directory under which isolated scratch `cwd` folders for folderless projects are created.
    pub detached_projects_root: Option<PathBuf>,
    /// Optional dedicated runtime configured for background tasks (`gemini-api-key`, never silently
    /// substituted for `oauth-personal` or vice versa).
    pub background_runtime: Option<Arc<dyn AgentRuntimePort>>,
    /// Optional explicit [`ExternalEffectPort`] connector (defaults to [`SimulatedExternalConnector`]).
    pub effect_port: Option<Arc<dyn ExternalEffectPort>>,
    /// Optional JSON ledger path backing the default [`SimulatedExternalConnector`] across restarts.
    pub effects_ledger_path: Option<PathBuf>,
    /// Optional directory outside PostgreSQL for storing the auto-generated `SecretBroker` `master.key`.
    pub vault_dir: Option<PathBuf>,
    /// Optional explicit 32-byte hex/base64 master key override (preferred over `DENNETT_VAULT_MASTER_KEY` and file).
    pub vault_master_key: Option<String>,
    /// Optional override for the read-only personal `~/.gemini` directory scanned by A08 discovery.
    pub personal_profile_dir: Option<PathBuf>,
    /// Explicitly permits non-loopback `LocalHttpRuntime` endpoints (SSRF guard defaults to `false`).
    pub allow_non_loopback_local_runtime: bool,
    /// Optional isolated profile root for dynamically constructed `AntigravityAcpRuntime` connections.
    pub antigravity_profile_root: Option<PathBuf>,
    /// Optional override path to the `agy` CLI binary for dynamic Antigravity connections.
    pub antigravity_binary: Option<PathBuf>,
    /// Optional deterministic override for `GEMINI_API_KEY` environment fallback (`Some(None)` forces unset in tests).
    pub gemini_api_key_env_override: Option<Option<String>>,
}

impl fmt::Debug for HeadServiceExtras {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HeadServiceExtras")
            .field("memory_hmac_signer", &self.memory_hmac_signer.is_some())
            .field("control_mcp_bin", &self.control_mcp_bin)
            .field("control_hmac_signer", &self.control_hmac_signer.is_some())
            .field("detached_projects_root", &self.detached_projects_root)
            .field("background_runtime", &self.background_runtime.is_some())
            .field("effect_port", &self.effect_port.is_some())
            .field("effects_ledger_path", &self.effects_ledger_path)
            .field("vault_dir", &self.vault_dir)
            .field(
                "vault_master_key",
                &self.vault_master_key.as_ref().map(|_| REDACTED_PLACEHOLDER),
            )
            .field("personal_profile_dir", &self.personal_profile_dir)
            .field(
                "allow_non_loopback_local_runtime",
                &self.allow_non_loopback_local_runtime,
            )
            .field("antigravity_profile_root", &self.antigravity_profile_root)
            .field("antigravity_binary", &self.antigravity_binary)
            .field(
                "gemini_api_key_env_override",
                &self
                    .gemini_api_key_env_override
                    .as_ref()
                    .map(|opt| opt.as_ref().map(|_| REDACTED_PLACEHOLDER)),
            )
            .finish()
    }
}

/// Scoped configuration passed to `dennett-memory-mcp` when launched by the agent runtime.
///
/// Contains ONLY `DENNETT_MEMORY_URL` and `DENNETT_MEMORY_TOKEN` — NEVER PostgreSQL DSNs or
/// Head database credentials. `Debug` strictly redacts `memory_token`.
#[derive(Clone)]
pub struct MemoryMcpWiring {
    /// Executable path to the `dennett-memory-mcp` binary.
    pub binary_path: PathBuf,
    /// Base HTTP URL of standalone `dennett-memoryd`.
    pub memory_url: String,
    /// Scoped bearer token authorizing the agent's permitted memory scopes.
    pub memory_token: String,
}

impl fmt::Debug for MemoryMcpWiring {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemoryMcpWiring")
            .field("binary_path", &self.binary_path)
            .field("memory_url", &self.memory_url)
            .field("memory_token", &REDACTED_PLACEHOLDER)
            .finish()
    }
}

/// Returns the explicit D1 capability list advertised by `dennett-head`.
#[must_use]
pub fn d1_head_capabilities() -> Vec<String> {
    vec![
        String::from("control.agents.v1"),
        String::from("control.projects.v1"),
        String::from("control.sessions.v1"),
        String::from("control.artifacts.v1"),
        String::from("control.policies.external.v1"),
        String::from("control.memory_outbox.v1"),
        String::from("control.cancel.v1"),
        String::from("control.diagnostics.v1"),
        String::from("control.runtime_catalog.v1"),
        String::from("control.workspace_files.v1"),
    ]
}

/// Returns the complete D2 capability list advertised by `dennett-head`.
#[must_use]
pub fn d2_head_capabilities() -> Vec<String> {
    let mut caps = d1_head_capabilities();
    caps.extend([
        String::from("control.tasks.v1"),
        String::from("control.timers.pg_iana.v1"),
        String::from("control.signals.idempotent.v1"),
        String::from("control.approvals.interactive.v1"),
        String::from("control.grants.revocable.v1"),
        String::from("control.inbox.v1"),
        String::from("control.events.ordered_journal.v1"),
        String::from("control.effects.lost_ack_unknown.v1"),
        String::from("control.lifecycle.strict_separation.v1"),
        String::from("control.agent_messages.plain_text.v1"),
    ]);
    caps
}

/// Returns the complete D3 capability list advertised by `dennett-head`.
#[must_use]
pub fn d3_head_capabilities() -> Vec<String> {
    let mut caps = d2_head_capabilities();
    caps.extend([
        String::from("control.secrets.write_only.v1"),
        String::from("control.runtime_connections.v1"),
        String::from("control.runtime_local_http.v1"),
        String::from("control.capabilities_registry.v1"),
        String::from("control.memory_fabric_proxy.v1"),
        String::from("control.subagents.policy.v1"),
    ]);
    caps
}

/// Compact multi-profile runtime router (`D2-OVER-02`, expanded in D3 for dynamic connections)
/// ensuring cancellation and session teardown ALWAYS fan out across EVERY active runtime adapter
/// without silent fallback (`A09`, `Contract M`).
#[derive(Clone)]
pub(crate) struct RuntimeRouter {
    interactive: Arc<dyn AgentRuntimePort>,
    background: Option<Arc<dyn AgentRuntimePort>>,
    dynamic_runtimes: Arc<Mutex<HashMap<ProviderConnectionId, CachedDynamicRuntime>>>,
}

impl RuntimeRouter {
    pub(crate) fn for_profile(
        &self,
        override_profile: Option<RuntimeAuthProfile>,
    ) -> Result<Arc<dyn AgentRuntimePort>, DennettError> {
        let Some(requested) = override_profile else {
            return Ok(Arc::clone(&self.interactive));
        };
        if self.interactive.auth_profile() == requested {
            return Ok(Arc::clone(&self.interactive));
        }
        if let Some(ref bg) = self.background
            && bg.auth_profile() == requested
        {
            return Ok(Arc::clone(bg));
        }
        Err(DennettError::AuthRequired {
            profile: requested,
            detail: format!(
                "runtime auth profile `{}` is not configured on dennett-head; refusing silent fallback to `{}`",
                requested.as_method_id(),
                self.interactive.auth_profile().as_method_id(),
            ),
        })
    }

    pub(crate) async fn cancel_turn_all(&self, session_id: SessionId) -> bool {
        let mut cancelled = self
            .interactive
            .cancel_turn(session_id)
            .await
            .unwrap_or(false);
        if let Some(ref bg) = self.background {
            cancelled |= bg.cancel_turn(session_id).await.unwrap_or(false);
        }
        let dyn_handles: Vec<Arc<dyn AgentRuntimePort>> = {
            let guard = self.dynamic_runtimes.lock().await;
            guard.values().map(|e| Arc::clone(&e.runtime)).collect()
        };
        for rt in dyn_handles {
            cancelled |= rt.cancel_turn(session_id).await.unwrap_or(false);
        }
        cancelled
    }

    pub(crate) async fn close_session_all(&self, session_id: SessionId) {
        let _ = self.interactive.close_session(session_id).await;
        if let Some(ref bg) = self.background {
            let _ = bg.close_session(session_id).await;
        }
        let dyn_handles: Vec<Arc<dyn AgentRuntimePort>> = {
            let guard = self.dynamic_runtimes.lock().await;
            guard.values().map(|e| Arc::clone(&e.runtime)).collect()
        };
        for rt in dyn_handles {
            let _ = rt.close_session(session_id).await;
        }
    }
}

/// Typed trigger source for unified [`HeadService::execute_session_turn_core`] (`D2-OVER-02`).
#[derive(Debug, Clone, Copy)]
pub(crate) enum TurnTrigger {
    UserMessage,
    TaskGeneration { task_id: TaskId },
}

pub(crate) struct CoreTurnInput {
    pub(crate) trigger: TurnTrigger,
    pub(crate) session_id: SessionId,
    pub(crate) session_kind: SessionKind,
    pub(crate) run_id: RunId,
    pub(crate) agent: AgentRecord,
    pub(crate) project: Option<ProjectRecord>,
    pub(crate) existing_binding: Option<RuntimeBinding>,
    pub(crate) prior_history: Vec<MessageRecord>,
    pub(crate) policy_rules: Vec<PolicyRule>,
    pub(crate) prompt_text: String,
    pub(crate) runtime: Arc<dyn AgentRuntimePort>,
    pub(crate) connection_id: Option<ProviderConnectionId>,
    pub(crate) turn_timeout: Duration,
    pub(crate) approval_timeout: Duration,
    pub(crate) cancel_rx: watch::Receiver<bool>,
    pub(crate) correlation_id: CorrelationId,
}

pub(crate) struct CoreTurnOutput {
    pub(crate) start_outcome: dennett_runtime_antigravity::SessionStartOutcome,
    pub(crate) runtime_binding: RuntimeBinding,
    pub(crate) turn_outcome: dennett_runtime_antigravity::TurnOutcome,
    pub(crate) effective_elapsed_ms: u64,
    pub(crate) context_used_tokens: Option<u64>,
    pub(crate) context_window_tokens: Option<u64>,
    pub(crate) captured_decisions: Vec<CapturedPermissionDecision>,
    pub(crate) hit_unknown_external: bool,
}

pub(crate) struct CoreTurnFailure {
    pub(crate) error: DennettError,
    pub(crate) effective_elapsed_ms: u64,
    pub(crate) captured_decisions: Vec<CapturedPermissionDecision>,
}

/// Placeholder written into scrubbed `ordered_outbox` payloads after successful delivery to `memoryd`.
const OUTBOX_SCRUBBED_DELIVERED: &str = "[delivered]";
/// Placeholder written into scrubbed `ordered_outbox` payloads after the note is forgotten.
const OUTBOX_SCRUBBED_FORGOTTEN: &str = "[forgotten]";

/// Immutable content-addressed artifact payload returned by [`HeadService::get_artifact_content`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactContentBlob {
    /// Exact immutable bytes captured when the artifact revision was registered.
    pub bytes: Vec<u8>,
    /// MIME media type associated with the artifact revision.
    pub mime_type: String,
    /// Verified lowercase SHA-256 hex digest matching `bytes`.
    pub sha256: Sha256Hex,
}

/// Durable JSONB envelope stored in `ordered_outbox` (`destination = 'memoryd'`, `D2-OVER-01`).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MemoryOutboxEnvelope {
    scope: MemoryScope,
    idempotency_key: IdempotencyKey,
    operation: String,
    session_id: Option<SessionId>,
    run_id: Option<RunId>,
    note_request: CreateNoteRequest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    payload_sha256: Option<Sha256Hex>,
    #[serde(default)]
    forgotten: bool,
    #[serde(default)]
    delivered_event_id: Option<dennett_contracts::MemoryEventId>,
    #[serde(default)]
    delivered_note_id: Option<dennett_contracts::MemoryNoteId>,
}

fn note_request_fingerprint(req: &CreateNoteRequest) -> Result<Sha256Hex, DennettError> {
    let bytes = serde_json::to_vec(req).map_err(|err| DennettError::Internal {
        detail: format!("failed serializing memory note request for fingerprint: {err}"),
    })?;
    Ok(sha256_hex(&bytes))
}

fn scrub_note_request(req: &mut CreateNoteRequest, marker: &str) {
    marker.clone_into(&mut req.title);
    marker.clone_into(&mut req.body);
    req.tags.clear();
    req.observed_at = None;
    req.evidence_links.clear();
    req.author_label = None;
    req.admission_status = None;
    req.salience = None;
    req.topic_key = None;
}

fn verify_existing_outbox_envelope(
    existing_env: &MemoryOutboxEnvelope,
    create_note_req: &CreateNoteRequest,
    incoming_fp: &Sha256Hex,
) -> Result<(), DennettError> {
    if existing_env.operation != "create_note" {
        return Err(DennettError::IdempotencyMismatch {
            idempotency_key: create_note_req.idempotency_key.to_string(),
            reason: format!(
                "reused idempotency key in scope `{}` with conflicting operation",
                create_note_req.scope
            ),
        });
    }
    if existing_env.forgotten {
        return Err(DennettError::Conflict {
            resource: String::from("memory_note"),
            expected_revision: None,
            actual_revision: None,
            reason: String::from("note was previously forgotten and cannot be resurrected"),
        });
    }
    let payload_matches = match existing_env.payload_sha256.as_ref() {
        Some(stored_fp) => stored_fp == incoming_fp,
        None => existing_env.note_request == *create_note_req,
    };
    if !payload_matches {
        return Err(DennettError::IdempotencyMismatch {
            idempotency_key: create_note_req.idempotency_key.to_string(),
            reason: format!(
                "reused idempotency key in scope `{}` with conflicting note payload",
                create_note_req.scope
            ),
        });
    }
    Ok(())
}

pub(crate) const CANCEL_REASON_NONE: u8 = 0;
pub(crate) const CANCEL_REASON_EXPLICIT_STOP: u8 = 1;
pub(crate) const CANCEL_REASON_STEER: u8 = 2;

const STEER_SLOT_WAIT_TIMEOUT: Duration = Duration::from_secs(15);
const IDLE_FOLLOWUP_LOCK_WAIT_TIMEOUT: Duration = Duration::from_millis(250);

/// RAII notifier signaling `true` strictly on Drop after a session's active-run slot and
/// execution lock are released (prevents `409 Conflict` races during `POST /steer`).
pub(crate) struct SlotReleasedNotifier(watch::Sender<bool>);

impl SlotReleasedNotifier {
    pub(crate) const fn new(tx: watch::Sender<bool>) -> Self {
        Self(tx)
    }
}

impl Drop for SlotReleasedNotifier {
    fn drop(&mut self) {
        let _ = self.0.send(true);
    }
}

struct SteeringSessionGuard {
    map: Arc<StdMutex<HashMap<SessionId, usize>>>,
    session_id: SessionId,
}

impl SteeringSessionGuard {
    fn acquire(map: Arc<StdMutex<HashMap<SessionId, usize>>>, session_id: SessionId) -> Self {
        let mut guard = map
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard.entry(session_id).or_insert(0) += 1;
        drop(guard);
        Self { map, session_id }
    }
}

impl Drop for SteeringSessionGuard {
    fn drop(&mut self) {
        let mut guard = self
            .map
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(count) = guard.get_mut(&self.session_id) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                guard.remove(&self.session_id);
            }
        }
    }
}

pub(crate) struct ActiveRunHandle {
    pub(crate) run_id: RunId,
    pub(crate) client_message_id: Option<String>,
    pub(crate) cancel_tx: watch::Sender<bool>,
    pub(crate) cancel_reason: Arc<AtomicU8>,
    pub(crate) completion_rx:
        Option<watch::Receiver<Option<Result<SendMessageResponse, DennettError>>>>,
    pub(crate) slot_released_rx: watch::Receiver<bool>,
}

pub(crate) struct ActiveRunGuard {
    service: HeadService,
    session_id: SessionId,
    run_id: RunId,
    cancel_tx: watch::Sender<bool>,
    correlation_id: CorrelationId,
    completed: bool,
}

impl ActiveRunGuard {
    pub(crate) fn new(
        service: HeadService,
        session_id: SessionId,
        run_id: RunId,
        cancel_tx: watch::Sender<bool>,
        correlation_id: CorrelationId,
    ) -> Self {
        Self {
            service,
            session_id,
            run_id,
            cancel_tx,
            correlation_id,
            completed: false,
        }
    }

    pub(crate) fn mark_completed(&mut self) {
        self.completed = true;
    }
}

impl Drop for ActiveRunGuard {
    fn drop(&mut self) {
        let map = Arc::clone(&self.service.inner.active_runs);
        let session_id = self.session_id;
        let run_id = self.run_id;
        let abandoned = !self.completed;
        if abandoned {
            let _ = self.cancel_tx.send(true);
        }
        self.service.remove_live_snapshot(session_id, run_id);
        if let Ok(mut guard) = map.try_lock() {
            if guard.get(&session_id).is_some_and(|h| h.run_id == run_id) {
                guard.remove(&session_id);
            }
        } else {
            let map_async = Arc::clone(&map);
            tokio::spawn(async move {
                let mut guard = map_async.lock().await;
                if guard.get(&session_id).is_some_and(|h| h.run_id == run_id) {
                    guard.remove(&session_id);
                }
            });
        }
        if abandoned {
            let svc = self.service.clone();
            let cid = self.correlation_id.clone();
            tokio::spawn(async move {
                svc.finalize_abandoned_run(session_id, run_id, &cid).await;
            });
        }
    }
}

pub(crate) struct ActiveTaskHandle {
    pub(crate) generation: i64,
    pub(crate) session_id: SessionId,
    pub(crate) run_id: RunId,
    pub(crate) cancel_tx: watch::Sender<bool>,
}

pub(crate) struct ActiveTaskGuard {
    active_tasks: Arc<Mutex<HashMap<TaskId, ActiveTaskHandle>>>,
    task_id: TaskId,
    run_id: RunId,
}

impl ActiveTaskGuard {
    pub(crate) fn new(
        active_tasks: Arc<Mutex<HashMap<TaskId, ActiveTaskHandle>>>,
        task_id: TaskId,
        run_id: RunId,
    ) -> Self {
        Self {
            active_tasks,
            task_id,
            run_id,
        }
    }
}

impl Drop for ActiveTaskGuard {
    fn drop(&mut self) {
        let map = Arc::clone(&self.active_tasks);
        let task_id = self.task_id;
        let run_id = self.run_id;
        if let Ok(mut guard) = map.try_lock() {
            if guard.get(&task_id).is_some_and(|h| h.run_id == run_id) {
                guard.remove(&task_id);
            }
            return;
        }
        tokio::spawn(async move {
            let mut guard = map.lock().await;
            if guard.get(&task_id).is_some_and(|h| h.run_id == run_id) {
                guard.remove(&task_id);
            }
        });
    }
}

struct InFlightClientMessageEntry {
    content: String,
    completion_rx: watch::Receiver<Option<Result<SendMessageResponse, DennettError>>>,
}

type InFlightClientMessageMap =
    Arc<Mutex<HashMap<(SessionId, String), InFlightClientMessageEntry>>>;

struct InFlightClientMessageGuard {
    map: InFlightClientMessageMap,
    key: (SessionId, String),
}

struct SpawnedTurnHandle {
    completion_rx: watch::Receiver<Option<Result<SendMessageResponse, DennettError>>>,
}

struct ClaimedFollowupTurn {
    followup: SessionFollowupRecord,
    user_message: MessageRecord,
    inbox_msgs: Vec<dennett_contracts::AgentMessageRecord>,
    effective_title: String,
}

struct DetachedTurnSpawnArgs {
    session_id: SessionId,
    session_kind: SessionKind,
    run_id: RunId,
    agent: AgentRecord,
    project: Option<ProjectRecord>,
    existing_binding: Option<RuntimeBinding>,
    prior_history: Vec<MessageRecord>,
    policy_rules: Vec<PolicyRule>,
    user_message: MessageRecord,
    effective_prompt: String,
    effective_session_title: String,
    runtime: Arc<dyn AgentRuntimePort>,
    connection_id: Option<ProviderConnectionId>,
    turn_timeout: Duration,
    approval_timeout: Duration,
    cancel_rx: watch::Receiver<bool>,
    cancel_reason: Arc<AtomicU8>,
    correlation_id: CorrelationId,
    completion_tx: watch::Sender<Option<Result<SendMessageResponse, DennettError>>>,
    session_guard: tokio::sync::OwnedMutexGuard<()>,
    active_guard: ActiveRunGuard,
    slot_released: SlotReleasedNotifier,
    in_flight_cmid_guard: Option<InFlightClientMessageGuard>,
}

impl Drop for InFlightClientMessageGuard {
    fn drop(&mut self) {
        let map = Arc::clone(&self.map);
        let key = self.key.clone();
        if let Ok(mut guard) = map.try_lock() {
            guard.remove(&key);
            return;
        }
        tokio::spawn(async move {
            let mut guard = map.lock().await;
            guard.remove(&key);
        });
    }
}

const MEMORY_STATUS_DEGRADED: u8 = 0;
const MEMORY_STATUS_HEALTHY: u8 = 1;

struct HeadServiceInner {
    pg_target: ValidatedControlPgTarget,
    pool: Pool,
    migrated_version: Mutex<Option<i64>>,
    startup_recovery_done: Mutex<bool>,
    runtime: Arc<dyn AgentRuntimePort>,
    background_runtime: Option<Arc<dyn AgentRuntimePort>>,
    dynamic_runtimes: Arc<Mutex<HashMap<ProviderConnectionId, CachedDynamicRuntime>>>,
    effect_port: Arc<dyn ExternalEffectPort>,
    memory_client: Option<MemoryClient>,
    memory_mcp: Option<MemoryMcpWiring>,
    memory_hmac_signer: Option<MemoryTokenSigner>,
    control_mcp_bin: Option<PathBuf>,
    control_token_signer: ControlTokenSigner,
    self_http_url: StdRwLock<Option<String>>,
    detached_projects_root: PathBuf,
    secret_cipher: SecretCipher,
    personal_profile_dir: Option<PathBuf>,
    allow_non_loopback_local_runtime: bool,
    antigravity_profile_root: PathBuf,
    antigravity_binary: Option<PathBuf>,
    #[expect(
        clippy::option_option,
        reason = "tri-state: None reads process env, Some(None) forces unset in tests, Some(Some(v)) injects deterministic key"
    )]
    gemini_api_key_env_override: Option<Option<String>>,
    runtime_catalog_cache: Mutex<Option<(time::Instant, RuntimeCatalogResponse)>>,
    last_memory_status: AtomicU8,
    active_runs: Arc<Mutex<HashMap<SessionId, ActiveRunHandle>>>,
    live_snapshots: Arc<StdMutex<HashMap<SessionId, LiveRunSnapshot>>>,
    steering_sessions: Arc<StdMutex<HashMap<SessionId, usize>>>,
    active_tasks: Arc<Mutex<HashMap<TaskId, ActiveTaskHandle>>>,
    stopped_tasks: Arc<Mutex<HashSet<TaskId>>>,
    pending_approvals:
        Arc<Mutex<HashMap<PermissionRequestId, oneshot::Sender<ApprovalResolutionSignal>>>>,
    session_locks: Arc<Mutex<HashMap<SessionId, Arc<Mutex<()>>>>>,
    in_flight_client_messages: InFlightClientMessageMap,
    profile_mode: ProfileMode,
    secrets_to_redact: StdRwLock<Vec<String>>,
}

/// Canonical orchestration control service backing `dennett-head`.
///
/// Cheap to clone via internal `Arc`.
#[derive(Clone)]
pub struct HeadService {
    inner: Arc<HeadServiceInner>,
}

impl HeadService {
    /// Constructs a new [`HeadService`] wired to `dennett_control` PostgreSQL, an
    /// [`AgentRuntimePort`], optional [`MemoryClient`], and optional [`MemoryMcpWiring`].
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::Internal`] if PostgreSQL pool construction fails.
    pub fn new(
        pg_target: ValidatedControlPgTarget,
        runtime: Arc<dyn AgentRuntimePort>,
        memory_client: Option<MemoryClient>,
        memory_mcp: Option<MemoryMcpWiring>,
        profile_mode: ProfileMode,
        extra_secrets: Vec<String>,
    ) -> Result<Self, DennettError> {
        Self::new_with_extras(
            pg_target,
            runtime,
            memory_client,
            memory_mcp,
            profile_mode,
            extra_secrets,
            HeadServiceExtras::default(),
        )
    }

    /// Constructs a new [`HeadService`] with optional dynamic HMAC token signing and detached
    /// project workspace root configuration.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::Internal`] if PostgreSQL pool construction fails.
    #[expect(
        clippy::too_many_lines,
        reason = "initializes PG pool, SecretBroker vault key, control/memory signers, and HeadServiceInner atomically"
    )]
    pub fn new_with_extras(
        pg_target: ValidatedControlPgTarget,
        runtime: Arc<dyn AgentRuntimePort>,
        memory_client: Option<MemoryClient>,
        memory_mcp: Option<MemoryMcpWiring>,
        profile_mode: ProfileMode,
        extra_secrets: Vec<String>,
        extras: HeadServiceExtras,
    ) -> Result<Self, DennettError> {
        let mgr_cfg = ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        };
        let manager = Manager::from_config(pg_target.config().clone(), NoTls, mgr_cfg);
        let pool = Pool::builder(manager)
            .max_size(DEFAULT_CONTROL_POOL_SIZE)
            .build()
            .map_err(|err| DennettError::Internal {
                detail: format!("failed constructing control PostgreSQL pool: {err}"),
            })?;

        let mut secrets = extra_secrets;
        if let Some(ref mcp) = memory_mcp
            && !mcp.memory_token.is_empty()
        {
            secrets.push(mcp.memory_token.clone());
        }
        if let Some(pw) = pg_target.parsed_password()
            && pw.len() >= 3
        {
            secrets.push(pw.to_owned());
        }

        let initial_memory_status = if memory_client.is_some() {
            MEMORY_STATUS_HEALTHY
        } else {
            MEMORY_STATUS_DEGRADED
        };

        let detached_projects_root = extras.detached_projects_root.unwrap_or_else(|| {
            std::env::temp_dir().join(format!("dennett-head-detached-{}", std::process::id()))
        });
        let vault_dir = extras
            .vault_dir
            .unwrap_or_else(|| default_vault_dir(&detached_projects_root));
        if let Some(ref mk) = extras.vault_master_key
            && !mk.trim().is_empty()
        {
            secrets.push(mk.clone());
        }
        let vault_master_key =
            VaultMasterKey::load_or_create(&vault_dir, extras.vault_master_key.as_deref())?;
        let secret_cipher = SecretCipher::new(vault_master_key);

        if let Some(Some(ref env_key)) = extras.gemini_api_key_env_override
            && !env_key.trim().is_empty()
        {
            secrets.push(env_key.clone());
        } else if extras.gemini_api_key_env_override.is_none()
            && let Some(env_key) = std::env::var("DENNETT_GEMINI_API_KEY")
                .ok()
                .or_else(|| std::env::var("GEMINI_API_KEY").ok())
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
        {
            secrets.push(env_key);
        }

        let antigravity_profile_root = extras
            .antigravity_profile_root
            .unwrap_or_else(|| detached_projects_root.join("agy-profiles"));

        let effect_port: Arc<dyn ExternalEffectPort> = if let Some(custom_port) = extras.effect_port
        {
            custom_port
        } else if let Some(ledger_path) = extras.effects_ledger_path {
            Arc::new(SimulatedExternalConnector::with_ledger_file(ledger_path))
        } else {
            Arc::new(SimulatedExternalConnector::in_memory())
        };

        let control_token_signer = extras
            .control_hmac_signer
            .unwrap_or_else(ControlTokenSigner::ephemeral);

        Ok(Self {
            inner: Arc::new(HeadServiceInner {
                pg_target,
                pool,
                migrated_version: Mutex::new(None),
                startup_recovery_done: Mutex::new(false),
                runtime,
                background_runtime: extras.background_runtime,
                dynamic_runtimes: Arc::new(Mutex::new(HashMap::new())),
                effect_port,
                memory_client,
                memory_mcp,
                memory_hmac_signer: extras.memory_hmac_signer,
                control_mcp_bin: extras.control_mcp_bin,
                control_token_signer,
                self_http_url: StdRwLock::new(None),
                detached_projects_root,
                secret_cipher,
                personal_profile_dir: extras.personal_profile_dir,
                allow_non_loopback_local_runtime: extras.allow_non_loopback_local_runtime,
                antigravity_profile_root,
                antigravity_binary: extras.antigravity_binary,
                gemini_api_key_env_override: extras.gemini_api_key_env_override,
                runtime_catalog_cache: Mutex::new(None),
                last_memory_status: AtomicU8::new(initial_memory_status),
                active_runs: Arc::new(Mutex::new(HashMap::new())),
                live_snapshots: Arc::new(StdMutex::new(HashMap::new())),
                steering_sessions: Arc::new(StdMutex::new(HashMap::new())),
                active_tasks: Arc::new(Mutex::new(HashMap::new())),
                stopped_tasks: Arc::new(Mutex::new(HashSet::new())),
                pending_approvals: Arc::new(Mutex::new(HashMap::new())),
                session_locks: Arc::new(Mutex::new(HashMap::new())),
                in_flight_client_messages: Arc::new(Mutex::new(HashMap::new())),
                profile_mode,
                secrets_to_redact: StdRwLock::new(secrets),
            }),
        })
    }

    /// Registers the bound loopback HTTP base URL of this `dennett-head` instance so
    /// `dennett-control-mcp` child adapters can call back into `Head` via `HeadClient`.
    pub fn set_self_http_url(&self, url: impl Into<String>) {
        let trimmed = url.into().trim().trim_end_matches('/').to_owned();
        if let Ok(mut guard) = self.inner.self_http_url.write() {
            *guard = (!trimmed.is_empty()).then_some(trimmed);
        }
    }

    /// Returns the registered loopback HTTP base URL of this `dennett-head` instance, if bound.
    #[must_use]
    pub fn self_http_url(&self) -> Option<String> {
        self.inner.self_http_url.read().ok().and_then(|g| g.clone())
    }

    /// Returns the HMAC-SHA256 signer verifying `X-Dennett-Control-Token` (`dct1.`) headers.
    #[must_use]
    pub fn control_token_signer(&self) -> &ControlTokenSigner {
        &self.inner.control_token_signer
    }

    /// Returns a [`RuntimeRouter`] spanning interactive, background, and cached dynamic runtimes.
    pub(crate) fn runtime_router(&self) -> RuntimeRouter {
        RuntimeRouter {
            interactive: Arc::clone(&self.inner.runtime),
            background: self.inner.background_runtime.clone(),
            dynamic_runtimes: Arc::clone(&self.inner.dynamic_runtimes),
        }
    }

    /// Returns the isolated fallback workspace directory path for a folderless/detached project.
    pub(crate) fn fallback_workspace_root(&self, project_id: ProjectId) -> PathBuf {
        self.detached_project_scratch_dir(project_id)
    }

    /// Probes the active runtime dynamically (`agy acp`, `LocalHttpRuntime`, or `SimulatedRuntime`)
    /// and returns the cached or freshly refreshed [`RuntimeCatalogResponse`] (`GET /v1/runtime/catalog`).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError`] if probing the active runtime fails.
    pub async fn get_runtime_catalog(
        &self,
        refresh: bool,
    ) -> Result<RuntimeCatalogResponse, DennettError> {
        let mut cache_guard = self.inner.runtime_catalog_cache.lock().await;
        if !refresh
            && let Some((probed_instant, ref cached)) = *cache_guard
            && probed_instant.elapsed() < RUNTIME_CATALOG_CACHE_TTL
        {
            return Ok(cached.clone());
        }

        let resolved = self.resolve_runtime_for_turn(None, None).await?;
        let descriptor = resolved.runtime.probe().await?;
        let catalog = RuntimeCatalogResponse {
            adapter_id: descriptor.adapter_id,
            auth_profile: descriptor.auth_profile,
            is_simulated: descriptor.is_simulated,
            agy_version: descriptor.agy_version,
            tier: descriptor.tier,
            account_label: detect_os_account_label(),
            default_model_id: descriptor.default_model_id,
            default_thinking_level: descriptor.default_thinking_level,
            models: descriptor.models,
            thinking_options: descriptor.thinking_options,
            quotas: descriptor.quotas,
            connection_id: resolved.connection_id,
            quota_error: None,
            probed_at: TimestampMs::now(),
        };
        *cache_guard = Some((time::Instant::now(), catalog.clone()));
        Ok(catalog)
    }

    /// Returns the configured runtime adapter identifier.
    #[must_use]
    pub fn runtime_adapter_id(&self) -> &str {
        self.inner.runtime.adapter_id()
    }

    /// Returns `true` strictly when Head is wired to the deterministic CI test double.
    #[must_use]
    pub fn runtime_is_simulated(&self) -> bool {
        self.inner.runtime.is_simulated()
    }

    /// Returns the active [`ProfileMode`].
    #[must_use]
    pub fn profile_mode(&self) -> ProfileMode {
        self.inner.profile_mode
    }

    /// Registers a secret string into Head's live redactor so logs, errors, and diagnostics
    /// never leak it.
    pub(crate) fn register_secret_for_redaction(&self, secret: &str) {
        let trimmed = secret.trim();
        if trimmed.len() < 3 {
            return;
        }
        if let Ok(mut guard) = self.inner.secrets_to_redact.write()
            && !guard.iter().any(|existing| existing == trimmed)
        {
            guard.push(trimmed.to_owned());
        }
    }

    /// Redacts configured secrets and generic credential patterns from `text`.
    #[must_use]
    pub fn redact(&self, text: &str) -> String {
        let snapshot: Vec<String> = self
            .inner
            .secrets_to_redact
            .read()
            .map_or_default(|g| g.clone());
        redact_secrets_owned(text, &snapshot)
    }

    /// Ensures `dennett_control` PostgreSQL migrations are applied idempotently.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::SafetyGuardRejected`] on schema mismatch or
    /// [`DennettError::DatabaseUnavailable`] when PostgreSQL is unreachable.
    pub async fn ensure_migrated(&self) -> Result<i64, DennettError> {
        let mut guard = self.inner.migrated_version.lock().await;
        if let Some(v) = *guard {
            return Ok(v);
        }

        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        if self.inner.profile_mode == ProfileMode::TestIsolated {
            let sp_row = conn
                .query_one("SHOW search_path", &[])
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
            let search_path: String = sp_row.get(0);
            let trimmed = search_path.trim().trim_matches('"');
            if trimmed.starts_with("iso_")
                && trimmed.len() <= 63
                && trimmed
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            {
                conn.batch_execute(&format!(
                    "CREATE SCHEMA IF NOT EXISTS {trimmed}; SET search_path TO {trimmed};"
                ))
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
            }
        }
        let version = time::timeout(
            DEFAULT_PG_OP_TIMEOUT,
            run_control_migrations(
                &mut conn,
                Some(self.inner.pg_target.raw_url()),
                self.inner.pg_target.parsed_password(),
            ),
        )
        .await
        .map_err(|_| DennettError::DatabaseUnavailable {
            detail: String::from("control PostgreSQL migration timed out"),
        })??;

        *guard = Some(version);
        Ok(version)
    }

    /// Probes both `dennett_control` PostgreSQL and standalone `dennett-memoryd` reachability.
    ///
    /// Per V02: when `dennett-memoryd` is down, this still succeeds with `postgres_connected: true`
    /// and `memory_status: MemoryStatus::DegradedUnreachable`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::DatabaseUnavailable`] only if `dennett_control` PostgreSQL itself is down.
    pub async fn health_check(
        &self,
        correlation_id: CorrelationId,
    ) -> Result<HeadHealth, DennettError> {
        let pg_fut = async {
            let conn = self.get_conn(DEFAULT_PROBE_TIMEOUT).await?;
            let row = conn
                .query_one(
                    "SELECT 1::int4, current_user::text, current_database()::text",
                    &[],
                )
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

            let check: i32 = row.get(0);
            let role: String = row.get(1);
            let database: String = row.get(2);
            if check != 1 {
                return Err(DennettError::DatabaseUnavailable {
                    detail: String::from("unexpected SELECT 1 result from control PostgreSQL"),
                });
            }

            let schema_version = query_control_schema_version(
                &conn,
                Some(self.inner.pg_target.raw_url()),
                self.inner.pg_target.parsed_password(),
            )
            .await?;

            let pending_outbox_count: i64 = match schema_version {
                Some(v) if v >= 6 => conn
                    .query_one(
                        "SELECT COUNT(*)::bigint
                         FROM ordered_outbox
                         WHERE destination = 'memoryd' AND status IN ('pending', 'leased')",
                        &[],
                    )
                    .await
                    .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?
                    .get(0),
                Some(_) => conn
                    .query_one(
                        "SELECT COUNT(*)::bigint FROM memory_commit_outbox WHERE status = 'pending'",
                        &[],
                    )
                    .await
                    .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?
                    .get(0),
                None => 0,
            };

            Ok::<_, DennettError>((role, database, schema_version, pending_outbox_count))
        };

        let (role, database, schema_version, pending_outbox_count) =
            time::timeout(DEFAULT_PROBE_TIMEOUT, pg_fut)
                .await
                .map_err(|_| DennettError::DatabaseUnavailable {
                    detail: String::from("control PostgreSQL health probe timed out"),
                })??;

        let (memory_status, memory_detail) = self.probe_memory_status(&correlation_id).await;

        Ok(HeadHealth {
            service: ServiceKind::Head,
            stage: String::from(HEAD_STAGE_LABEL),
            profile_mode: self.inner.profile_mode,
            postgres_connected: true,
            postgres_role: Some(role),
            postgres_database: Some(database),
            schema_version,
            memory_status,
            memory_detail,
            runtime_adapter_id: self.inner.runtime.adapter_id().to_owned(),
            runtime_is_simulated: self.inner.runtime.is_simulated(),
            pending_outbox_count,
            capabilities: d3_head_capabilities(),
            correlation_id,
        })
    }

    /// Creates a new stable [`AgentRecord`] with its initial revision-1 [`dennett_contracts::AgentConfigVersion`].
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] or [`DennettError::DatabaseUnavailable`].
    #[expect(
        clippy::too_many_lines,
        reason = "persists agent row, v1 config version, audit event, and control journal event atomically"
    )]
    pub async fn create_agent(
        &self,
        req: CreateAgentRequest,
        correlation_id: &CorrelationId,
    ) -> Result<AgentRecord, DennettError> {
        req.validate()?;
        let _ = self.ensure_migrated().await?;
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let agent_id = AgentId::new();
        let config_version_id = AgentConfigVersionId::new();
        let scopes_json = serde_json::to_value(&req.allowed_memory_scopes).map_err(|err| {
            DennettError::Internal {
                detail: format!("failed serializing allowed_memory_scopes: {err}"),
            }
        })?;
        let write_policy_json =
            serde_json::to_value(&req.workspace_write_policy).map_err(|err| {
                DennettError::Internal {
                    detail: format!("failed serializing workspace_write_policy: {err}"),
                }
            })?;
        let default_budget_json = req
            .default_budget
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .map_err(|err| DennettError::Internal {
                detail: format!("failed serializing default_budget: {err}"),
            })?;
        let attached_skills_json =
            serde_json::to_value(&req.attached_skills).map_err(|err| DennettError::Internal {
                detail: format!("failed serializing attached_skills: {err}"),
            })?;
        let thinking_str = req.thinking_level.map(|t| t.as_str().to_owned());
        if let Some(conn_id) = req.preferred_connection_id {
            let exists: bool = tx
                .query_one(
                    "SELECT EXISTS(SELECT 1 FROM provider_connections WHERE connection_id = $1)",
                    &[&conn_id.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?
                .get(0);
            if !exists {
                return Err(DennettError::NotFound {
                    resource: String::from("provider_connection"),
                    id: conn_id.to_string(),
                });
            }
        }

        tx.execute(
            "INSERT INTO agents (agent_id, name, description, current_config_version_id)
             VALUES ($1, $2, $3, $4)",
            &[
                &agent_id.as_uuid(),
                &req.name.trim(),
                &req.description.trim(),
                &config_version_id.as_uuid(),
            ],
        )
        .await
        .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        tx.execute(
            "INSERT INTO agent_config_versions (
                config_version_id, agent_id, version_number, system_prompt,
                model_preference, thinking_level, allowed_memory_scopes,
                workspace_write_policy, default_budget, attached_skills,
                preferred_connection_id
             ) VALUES ($1, $2, 1, $3, $4, $5, $6, $7, $8, $9, $10)",
            &[
                &config_version_id.as_uuid(),
                &agent_id.as_uuid(),
                &req.system_prompt,
                &req.model_preference,
                &thinking_str,
                &scopes_json,
                &write_policy_json,
                &default_budget_json,
                &attached_skills_json,
                &req.preferred_connection_id
                    .map(ProviderConnectionId::as_uuid),
            ],
        )
        .await
        .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let _ = append_audit_event(
            &tx,
            &self.inner.pg_target,
            correlation_id,
            None,
            None,
            "head",
            "agent_created",
            &format!("created agent `{}` ({agent_id}) v1", req.name.trim()),
        )
        .await?;

        let record = fetch_agent(&tx, agent_id, &self.inner.pg_target).await?;
        let _ = append_control_event_in_tx(
            &tx,
            &self.inner.pg_target,
            ControlEventAppend {
                entity_key: &format!("agent:{agent_id}"),
                event_type: "agent.created",
                project_id: None,
                session_id: None,
                task_id: None,
                correlation_id,
                payload: json!({
                    "agent_id": agent_id,
                    "name": record.name,
                    "version_number": 1,
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
        Ok(record)
    }

    /// Lists all registered agents with their current configuration versions.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::DatabaseUnavailable`] if PostgreSQL queries fail.
    pub async fn list_agents(&self) -> Result<ListAgentsResponse, DennettError> {
        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let sql = format!("{AGENT_JOIN_SELECT} ORDER BY a.created_at DESC");
        let rows = conn
            .query(&sql, &[])
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let agents = rows
            .iter()
            .map(decode_agent_join_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ListAgentsResponse { agents })
    }

    /// Fetches a single [`AgentRecord`] by stable [`AgentId`].
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`] or [`DennettError::DatabaseUnavailable`].
    pub async fn get_agent(&self, agent_id: AgentId) -> Result<AgentRecord, DennettError> {
        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        fetch_agent(&conn, agent_id, &self.inner.pg_target).await
    }

    /// Appends a new immutable [`dennett_contracts::AgentConfigVersion`] onto an existing agent WITHOUT changing its
    /// stable [`AgentId`] (A04 / Architecture 82 §2).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::NotFound`], or
    /// [`DennettError::DatabaseUnavailable`].
    #[expect(
        clippy::too_many_lines,
        reason = "locks agent row, inserts next immutable config version, updates pointer, and emits audit + journal events atomically"
    )]
    pub async fn update_agent_config(
        &self,
        agent_id: AgentId,
        req: UpdateAgentConfigRequest,
        correlation_id: &CorrelationId,
    ) -> Result<AgentRecord, DennettError> {
        req.validate()?;
        let _ = self.ensure_migrated().await?;
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let locked = tx
            .query_opt(
                "SELECT agent_id FROM agents WHERE agent_id = $1 FOR UPDATE",
                &[&agent_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
        if locked.is_none() {
            return Err(DennettError::NotFound {
                resource: String::from("agent"),
                id: agent_id.to_string(),
            });
        }

        let max_ver: i64 = tx
            .query_one(
                "SELECT COALESCE(MAX(version_number), 0)::bigint
                 FROM agent_config_versions WHERE agent_id = $1",
                &[&agent_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?
            .get(0);
        let next_ver = max_ver + 1;
        let new_version_id = AgentConfigVersionId::new();
        let scopes_json = serde_json::to_value(&req.allowed_memory_scopes).map_err(|err| {
            DennettError::Internal {
                detail: format!("failed serializing allowed_memory_scopes: {err}"),
            }
        })?;
        let write_policy_json =
            serde_json::to_value(&req.workspace_write_policy).map_err(|err| {
                DennettError::Internal {
                    detail: format!("failed serializing workspace_write_policy: {err}"),
                }
            })?;
        let default_budget_json = req
            .default_budget
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .map_err(|err| DennettError::Internal {
                detail: format!("failed serializing default_budget: {err}"),
            })?;
        let attached_skills_json =
            serde_json::to_value(&req.attached_skills).map_err(|err| DennettError::Internal {
                detail: format!("failed serializing attached_skills: {err}"),
            })?;
        let thinking_str = req.thinking_level.map(|t| t.as_str().to_owned());
        if let Some(conn_id) = req.preferred_connection_id {
            let exists: bool = tx
                .query_one(
                    "SELECT EXISTS(SELECT 1 FROM provider_connections WHERE connection_id = $1)",
                    &[&conn_id.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?
                .get(0);
            if !exists {
                return Err(DennettError::NotFound {
                    resource: String::from("provider_connection"),
                    id: conn_id.to_string(),
                });
            }
        }

        tx.execute(
            "INSERT INTO agent_config_versions (
                config_version_id, agent_id, version_number, system_prompt,
                model_preference, thinking_level, allowed_memory_scopes,
                workspace_write_policy, default_budget, attached_skills,
                preferred_connection_id
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
            &[
                &new_version_id.as_uuid(),
                &agent_id.as_uuid(),
                &next_ver,
                &req.system_prompt,
                &req.model_preference,
                &thinking_str,
                &scopes_json,
                &write_policy_json,
                &default_budget_json,
                &attached_skills_json,
                &req.preferred_connection_id
                    .map(ProviderConnectionId::as_uuid),
            ],
        )
        .await
        .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        tx.execute(
            "UPDATE agents SET current_config_version_id = $2, updated_at = now()
             WHERE agent_id = $1",
            &[&agent_id.as_uuid(), &new_version_id.as_uuid()],
        )
        .await
        .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let _ = append_audit_event(
            &tx,
            &self.inner.pg_target,
            correlation_id,
            None,
            None,
            "head",
            "agent_config_version_created",
            &format!(
                "updated agent {agent_id} to config version v{next_ver} (model={:?}, thinking={:?})",
                req.model_preference, thinking_str
            ),
        )
        .await?;

        let record = fetch_agent(&tx, agent_id, &self.inner.pg_target).await?;
        let _ = append_control_event_in_tx(
            &tx,
            &self.inner.pg_target,
            ControlEventAppend {
                entity_key: &format!("agent:{agent_id}"),
                event_type: "agent.config_updated",
                project_id: None,
                session_id: None,
                task_id: None,
                correlation_id,
                payload: json!({
                    "agent_id": agent_id,
                    "version_number": next_ver,
                    "config_version_id": new_version_id,
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
        Ok(record)
    }

    /// Returns all immutable configuration versions for `agent_id`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`] or [`DennettError::DatabaseUnavailable`].
    pub async fn get_agent_config_history(
        &self,
        agent_id: AgentId,
    ) -> Result<AgentConfigHistoryResponse, DennettError> {
        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let agent = fetch_agent(&conn, agent_id, &self.inner.pg_target).await?;

        let rows = conn
            .query(
                "SELECT config_version_id, agent_id, version_number, system_prompt,
                        model_preference, thinking_level, allowed_memory_scopes,
                        workspace_write_policy, default_budget, attached_skills,
                        TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
                        preferred_connection_id
                 FROM agent_config_versions
                 WHERE agent_id = $1
                 ORDER BY version_number ASC",
                &[&agent_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let versions = rows
            .iter()
            .map(decode_config_version_row)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(AgentConfigHistoryResponse {
            agent_id,
            current_config_version_id: agent.current_config_version_id,
            versions,
        })
    }

    /// Registers a workspace project bound to a canonical [`dennett_contracts::MemoryScope`].
    ///
    /// Supports both folder-bound projects (`workspace_path = Some(...)`) and folderless projects
    /// (`workspace_path = None`, "Пока без папки"). When `memory_scope` is omitted, generates a
    /// unique canonical scope `project:<slug>-<short8>`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] or [`DennettError::DatabaseUnavailable`].
    pub async fn create_project(
        &self,
        req: CreateProjectRequest,
        correlation_id: &CorrelationId,
    ) -> Result<ProjectRecord, DennettError> {
        req.validate()?;
        let _ = self.ensure_migrated().await?;

        let project_id = ProjectId::new();
        let raw_workspace_path = req
            .workspace_path
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned);
        let workspace_path = if let Some(raw_path) = raw_workspace_path {
            let scope_label = format!("project:{project_id}");
            let canonical = tokio::task::spawn_blocking(move || {
                validate_safe_workspace_root(&raw_path, &scope_label)
            })
            .await
            .map_err(|err| DennettError::Internal {
                detail: format!("workspace path validation task failed: {err}"),
            })??;
            Some(clean_display_path(&canonical))
        } else {
            None
        };

        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let memory_scope = match req.memory_scope {
            Some(scope) => scope,
            None => generate_project_memory_scope(&req.name, project_id)?,
        };
        let insert_sql = format!(
            "INSERT INTO projects (project_id, name, workspace_path, memory_scope, lifecycle_state)
             VALUES ($1, $2, $3, $4, 'active')
             RETURNING {PROJECT_SELECT_COLS}"
        );
        let row = tx
            .query_one(
                &insert_sql,
                &[
                    &project_id.as_uuid(),
                    &req.name.trim(),
                    &workspace_path,
                    &memory_scope.as_str(),
                ],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let _ = append_audit_event(
            &tx,
            &self.inner.pg_target,
            correlation_id,
            None,
            None,
            "head",
            "project_created",
            &format!(
                "registered project `{}` ({project_id}) scope={} bound={}",
                req.name.trim(),
                memory_scope,
                workspace_path.is_some()
            ),
        )
        .await?;

        let record = decode_project_row(&row)?;
        let _ = append_control_event_in_tx(
            &tx,
            &self.inner.pg_target,
            ControlEventAppend {
                entity_key: &format!("project:{project_id}"),
                event_type: "project.created",
                project_id: Some(project_id),
                session_id: None,
                task_id: None,
                correlation_id,
                payload: json!({
                    "project_id": project_id,
                    "name": record.name,
                    "memory_scope": record.memory_scope,
                    "lifecycle_state": record.lifecycle_state,
                    "bound": record.workspace_path.is_some(),
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
        Ok(record)
    }

    /// Updates a project's display name and/or attaches/rebinds its workspace folder
    /// (`PATCH /v1/projects/{project_id}`).
    ///
    /// When `workspace_path` is provided, strictly validates and canonicalizes the directory
    /// outside the model (rejecting UNC/device paths, `..` traversal, symlinks, and Windows
    /// NTFS junctions/reparse points).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::AccessDenied`],
    /// [`DennettError::NotFound`], or [`DennettError::DatabaseUnavailable`].
    #[expect(
        clippy::too_many_lines,
        reason = "project update atomically validates workspace rebind, transitions Detached->Active, emits journal events, and resets runtime sessions"
    )]
    pub async fn update_project(
        &self,
        project_id: ProjectId,
        req: UpdateProjectRequest,
        correlation_id: &CorrelationId,
    ) -> Result<ProjectRecord, DennettError> {
        req.validate()?;
        let _ = self.ensure_migrated().await?;

        let validated_workspace = if let Some(raw_path) = req.workspace_path {
            let scope_label = format!("project:{project_id}");
            let canonical = tokio::task::spawn_blocking(move || {
                validate_safe_workspace_root(&raw_path, &scope_label)
            })
            .await
            .map_err(|err| DennettError::Internal {
                detail: format!("workspace path validation task failed: {err}"),
            })??;
            Some(clean_display_path(&canonical))
        } else {
            None
        };

        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let existing = fetch_project(&tx, project_id, &self.inner.pg_target).await?;
        if matches!(
            existing.lifecycle_state,
            ProjectLifecycleState::Archived | ProjectLifecycleState::DeletedRecord
        ) {
            return Err(DennettError::Conflict {
                resource: String::from("project"),
                expected_revision: None,
                actual_revision: None,
                reason: format!(
                    "cannot update project `{project_id}` in lifecycle state `{}`",
                    existing.lifecycle_state
                ),
            });
        }
        let bump_updated_at = req.name.is_some() || validated_workspace.is_some();
        let next_name = req
            .name
            .as_deref()
            .map_or(existing.name.as_str(), str::trim)
            .to_owned();
        let workspace_changed = validated_workspace.is_some()
            && validated_workspace.as_deref() != existing.workspace_path.as_deref();
        let next_workspace = validated_workspace.or(existing.workspace_path.clone());
        let next_lifecycle = if next_workspace.is_some()
            && existing.lifecycle_state == ProjectLifecycleState::Detached
        {
            ProjectLifecycleState::Active
        } else {
            existing.lifecycle_state
        };

        let update_sql = format!(
            "UPDATE projects
             SET name = $2,
                 workspace_path = $3,
                 lifecycle_state = $4,
                 pinned = COALESCE($5, pinned),
                 pinned_at = CASE
                     WHEN $5 IS TRUE THEN COALESCE(pinned_at, now())
                     WHEN $5 IS FALSE THEN NULL
                     ELSE pinned_at
                 END,
                 updated_at = CASE WHEN $6 THEN now() ELSE updated_at END
             WHERE project_id = $1
             RETURNING {PROJECT_SELECT_COLS}"
        );
        let row = tx
            .query_one(
                &update_sql,
                &[
                    &project_id.as_uuid(),
                    &next_name,
                    &next_workspace,
                    &next_lifecycle.as_str(),
                    &req.pinned,
                    &bump_updated_at,
                ],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let sessions_to_close: Vec<SessionId> = if workspace_changed {
            tx.query(
                "SELECT session_id FROM sessions WHERE project_id = $1",
                &[&project_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?
            .iter()
            .map(|r| SessionId::from_uuid(r.get(0)))
            .collect()
        } else {
            Vec::new()
        };

        let record = decode_project_row(&row)?;
        let _ = append_audit_event(
            &tx,
            &self.inner.pg_target,
            correlation_id,
            None,
            None,
            "head",
            "project_updated",
            &format!(
                "updated project `{next_name}` ({project_id}) state={} bound={} pinned={}",
                next_lifecycle,
                next_workspace.is_some(),
                record.pinned
            ),
        )
        .await?;

        let _ = append_control_event_in_tx(
            &tx,
            &self.inner.pg_target,
            ControlEventAppend {
                entity_key: &format!("project:{project_id}"),
                event_type: "project.updated",
                project_id: Some(project_id),
                session_id: None,
                task_id: None,
                correlation_id,
                payload: json!({
                    "project_id": project_id,
                    "name": record.name,
                    "lifecycle_state": record.lifecycle_state,
                    "bound": record.workspace_path.is_some(),
                    "pinned": record.pinned,
                    "pinned_at": record.pinned_at,
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        if !sessions_to_close.is_empty() {
            let router = self.runtime_router();
            for sid in sessions_to_close {
                router.close_session_all(sid).await;
            }
        }
        Ok(record)
    }

    /// Lists real files inside a project's bound workspace folder (`GET /v1/projects/{id}/files`).
    ///
    /// Executes filesystem traversal inside `tokio::task::spawn_blocking` with strict canonical
    /// containment, symlink/junction rejection, and depth/count anti-DoS caps.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`], [`DennettError::AccessDenied`], or
    /// [`DennettError::DatabaseUnavailable`].
    pub async fn list_project_files(
        &self,
        project_id: ProjectId,
        subpath: Option<String>,
    ) -> Result<ListProjectFilesResponse, DennettError> {
        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let project = fetch_project(&conn, project_id, &self.inner.pg_target).await?;
        let ws = project.workspace_path;

        tokio::task::spawn_blocking(move || {
            list_project_workspace_files(project_id, ws.as_deref(), subpath.as_deref())
        })
        .await
        .map_err(|err| DennettError::Internal {
            detail: format!("workspace file listing task failed: {err}"),
        })?
    }

    /// Lists active/detached registered projects (excluding `archived` and `deleted_record`).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::DatabaseUnavailable`] on PG failure.
    pub async fn list_projects(&self) -> Result<ListProjectsResponse, DennettError> {
        self.list_projects_with_options(false).await
    }

    /// Lists registered projects with explicit `include_archived` toggle (`deleted_record` rows
    /// are never returned in active lists per Contract C).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::DatabaseUnavailable`] on PG failure.
    pub async fn list_projects_with_options(
        &self,
        include_archived: bool,
    ) -> Result<ListProjectsResponse, DennettError> {
        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let sql = if include_archived {
            format!(
                "SELECT {PROJECT_SELECT_COLS}
                 FROM projects
                 WHERE lifecycle_state <> 'deleted_record'
                 ORDER BY pinned DESC, pinned_at DESC NULLS LAST, updated_at DESC, created_at DESC"
            )
        } else {
            format!(
                "SELECT {PROJECT_SELECT_COLS}
                 FROM projects
                 WHERE lifecycle_state NOT IN ('archived', 'deleted_record')
                 ORDER BY pinned DESC, pinned_at DESC NULLS LAST, updated_at DESC, created_at DESC"
            )
        };
        let rows = conn
            .query(&sql, &[])
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let projects = rows
            .iter()
            .map(decode_project_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ListProjectsResponse { projects })
    }

    /// Fetches a single registered project.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`] or [`DennettError::DatabaseUnavailable`].
    pub async fn get_project(&self, project_id: ProjectId) -> Result<ProjectRecord, DennettError> {
        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        fetch_project(&conn, project_id, &self.inner.pg_target).await
    }

    /// Creates a new persisted conversation session for an existing agent and project.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::NotFound`], or
    /// [`DennettError::DatabaseUnavailable`].
    pub async fn create_session(
        &self,
        req: CreateSessionRequest,
        correlation_id: &CorrelationId,
    ) -> Result<SessionRecord, DennettError> {
        req.validate()?;
        let _ = self.ensure_migrated().await?;
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let _ = fetch_agent(&tx, req.agent_id, &self.inner.pg_target).await?;
        if let Some(pid) = req.project_id {
            let project = fetch_project(&tx, pid, &self.inner.pg_target).await?;
            if matches!(
                project.lifecycle_state,
                ProjectLifecycleState::Archived | ProjectLifecycleState::DeletedRecord
            ) {
                return Err(DennettError::Conflict {
                    resource: String::from("project"),
                    expected_revision: None,
                    actual_revision: None,
                    reason: format!(
                        "project `{}` is `{}` and does not accept new sessions",
                        project.project_id, project.lifecycle_state
                    ),
                });
            }
        }

        let session_id = SessionId::new();
        let insert_sql = format!(
            "INSERT INTO sessions (session_id, agent_id, project_id, title, status, session_kind)
             VALUES ($1, $2, $3, $4, 'active', $5)
             RETURNING {SESSION_SELECT_COLS}"
        );
        let row = tx
            .query_one(
                &insert_sql,
                &[
                    &session_id.as_uuid(),
                    &req.agent_id.as_uuid(),
                    &req.project_id.map(ProjectId::as_uuid),
                    &req.title.trim(),
                    &req.session_kind.as_str(),
                ],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let _ = append_audit_event(
            &tx,
            &self.inner.pg_target,
            correlation_id,
            Some(session_id),
            None,
            "head",
            "session_created",
            &format!(
                "created session `{}` ({session_id}) kind={} standalone={}",
                req.title.trim(),
                req.session_kind,
                req.project_id.is_none()
            ),
        )
        .await?;

        let record = decode_session_row(&row, None)?;
        let _ = append_control_event_in_tx(
            &tx,
            &self.inner.pg_target,
            ControlEventAppend {
                entity_key: &format!("session:{session_id}"),
                event_type: "session.created",
                project_id: req.project_id,
                session_id: Some(session_id),
                task_id: None,
                correlation_id,
                payload: json!({
                    "session_id": session_id,
                    "project_id": req.project_id,
                    "agent_id": req.agent_id,
                    "title": record.title,
                    "session_kind": record.session_kind,
                    "pinned": record.pinned,
                    "pinned_at": record.pinned_at,
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
        Ok(record)
    }

    /// Updates an existing conversation session's title and/or pin state
    /// (`PATCH /v1/sessions/{session_id}`).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::NotFound`], or
    /// [`DennettError::DatabaseUnavailable`].
    pub async fn update_session(
        &self,
        session_id: SessionId,
        req: UpdateSessionRequest,
        correlation_id: &CorrelationId,
    ) -> Result<SessionRecord, DennettError> {
        req.validate()?;
        let _ = self.ensure_migrated().await?;
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let trimmed_title = req.title.as_deref().map(str::trim);
        let bump_updated_at = trimmed_title.is_some();
        let update_sql = format!(
            "UPDATE sessions
             SET title = COALESCE($2, title),
                 pinned = COALESCE($3, pinned),
                 pinned_at = CASE
                     WHEN $3 IS TRUE THEN COALESCE(pinned_at, now())
                     WHEN $3 IS FALSE THEN NULL
                     ELSE pinned_at
                 END,
                 updated_at = CASE WHEN $4 THEN now() ELSE updated_at END
             WHERE session_id = $1
             RETURNING {SESSION_SELECT_COLS}"
        );
        let row = tx
            .query_opt(
                &update_sql,
                &[
                    &session_id.as_uuid(),
                    &trimmed_title,
                    &req.pinned,
                    &bump_updated_at,
                ],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let Some(row) = row else {
            return Err(DennettError::NotFound {
                resource: String::from("session"),
                id: session_id.to_string(),
            });
        };

        let binding = fetch_runtime_binding(&tx, session_id, &self.inner.pg_target).await?;
        let record = decode_session_row(&row, binding)?;
        let audit_kind = if trimmed_title.is_some() && req.pinned.is_none() {
            "session_renamed"
        } else {
            "session_updated"
        };
        let _ = append_audit_event(
            &tx,
            &self.inner.pg_target,
            correlation_id,
            Some(session_id),
            None,
            "head",
            audit_kind,
            &format!(
                "updated session {session_id} title=`{}` pinned={}",
                record.title, record.pinned
            ),
        )
        .await?;

        let _ = append_control_event_in_tx(
            &tx,
            &self.inner.pg_target,
            ControlEventAppend {
                entity_key: &format!("session:{session_id}"),
                event_type: "session.updated",
                project_id: record.project_id,
                session_id: Some(session_id),
                task_id: None,
                correlation_id,
                payload: json!({
                    "session_id": session_id,
                    "project_id": record.project_id,
                    "title": record.title,
                    "session_kind": record.session_kind,
                    "pinned": record.pinned,
                    "pinned_at": record.pinned_at,
                }),
            },
        )
        .await?;
        tx.commit()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
        Ok(record)
    }

    /// Lists all persisted sessions with their latest runtime bindings.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::DatabaseUnavailable`] on PG failure.
    pub async fn list_sessions(&self) -> Result<ListSessionsResponse, DennettError> {
        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let mut bindings = fetch_all_runtime_bindings(&conn, &self.inner.pg_target).await?;
        let sql = format!(
            "SELECT {SESSION_SELECT_COLS}
             FROM sessions
             ORDER BY pinned DESC, pinned_at DESC NULLS LAST, updated_at DESC"
        );
        let rows = conn
            .query(&sql, &[])
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let mut sessions = Vec::with_capacity(rows.len());
        for row in &rows {
            let sid = SessionId::from_uuid(row.get(0));
            sessions.push(decode_session_row(row, bindings.remove(&sid))?);
        }
        Ok(ListSessionsResponse { sessions })
    }

    /// Returns complete persisted session detail (session, binding, messages, artifacts, runs, decisions).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`] or [`DennettError::DatabaseUnavailable`].
    pub async fn get_session_detail(
        &self,
        session_id: SessionId,
    ) -> Result<SessionDetailResponse, DennettError> {
        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let session = fetch_session(&conn, session_id, &self.inner.pg_target).await?;
        let messages = fetch_session_messages(&conn, session_id, &self.inner.pg_target).await?;
        let artifacts = fetch_session_artifacts(&conn, session_id, &self.inner.pg_target).await?;
        let runs = fetch_session_runs(&conn, session_id, &self.inner.pg_target).await?;
        let permission_decisions =
            fetch_session_permission_decisions(&conn, session_id, &self.inner.pg_target).await?;
        let followups =
            fetch_active_session_followups(&conn, session_id, &self.inner.pg_target).await?;

        Ok(SessionDetailResponse {
            session,
            messages,
            artifacts,
            runs,
            permission_decisions,
            followups,
        })
    }

    /// Registers an external [`PolicyRule`] enforced outside the model.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] or [`DennettError::DatabaseUnavailable`].
    pub async fn create_policy_rule(
        &self,
        req: CreatePolicyRuleRequest,
        correlation_id: &CorrelationId,
    ) -> Result<PolicyRule, DennettError> {
        req.validate()?;
        let _ = self.ensure_migrated().await?;
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let rule_id = PolicyRuleId::new();
        let row = tx
            .query_one(
                "INSERT INTO policy_rules (rule_id, scope, tool_name, target_pattern, effect, reason)
                 VALUES ($1, $2, $3, $4, $5, $6)
                 RETURNING rule_id, scope, tool_name, target_pattern, effect, reason,
                           TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint",
                &[
                    &rule_id.as_uuid(),
                    &req.scope.trim(),
                    &req.tool_name.trim(),
                    &req.target_pattern.as_deref().map(str::trim),
                    &req.effect.as_str(),
                    &req.reason.trim(),
                ],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let _ = append_audit_event(
            &tx,
            &self.inner.pg_target,
            correlation_id,
            None,
            None,
            "policy_engine",
            "policy_rule_created",
            &format!(
                "added {} rule for tool `{}` in scope `{}`",
                req.effect,
                req.tool_name.trim(),
                req.scope.trim()
            ),
        )
        .await?;

        let rule = decode_policy_rule_row(&row)?;
        tx.commit()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
        Ok(rule)
    }

    /// Lists all configured external policy rules.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::DatabaseUnavailable`] on PG failure.
    pub async fn list_policy_rules(&self) -> Result<ListPolicyRulesResponse, DennettError> {
        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let rules = fetch_policy_rules(&conn, &self.inner.pg_target).await?;
        Ok(ListPolicyRulesResponse { rules })
    }

    /// Persists an ordinary D1 artifact linked to `session_id` and its project workspace.
    ///
    /// When `relative_path` is supplied without inline `content`, verifies the path stays strictly
    /// inside the project workspace root (symlink/traversal-safe) and hashes the real file bytes.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::AccessDenied`],
    /// [`DennettError::NotFound`], or [`DennettError::DatabaseUnavailable`].
    #[expect(
        clippy::too_many_lines,
        reason = "artifact creation validates workspace containment, streams SHA-256 off the async executor without holding a DB connection, locks the owning session row, and persists metadata + audit event atomically"
    )]
    pub async fn create_artifact(
        &self,
        session_id: SessionId,
        req: CreateArtifactRequest,
        correlation_id: &CorrelationId,
    ) -> Result<ArtifactRecord, DennettError> {
        req.validate()?;
        let _ = self.ensure_migrated().await?;

        // Fetch session + optional project metadata first and release the pooled connection BEFORE
        // any blocking filesystem validation or streaming SHA-256 disk I/O.
        let (project_id, workspace_path) = {
            let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
            let session = fetch_session(&conn, session_id, &self.inner.pg_target).await?;
            if let Some(pid) = session.project_id {
                let project = fetch_project(&conn, pid, &self.inner.pg_target).await?;
                if matches!(
                    project.lifecycle_state,
                    ProjectLifecycleState::Archived | ProjectLifecycleState::DeletedRecord
                ) {
                    return Err(DennettError::Conflict {
                        resource: String::from("project"),
                        expected_revision: None,
                        actual_revision: None,
                        reason: format!(
                            "project `{}` is `{}` and does not accept new artifacts",
                            project.project_id, project.lifecycle_state
                        ),
                    });
                }
                (Some(project.project_id), project.workspace_path)
            } else {
                (None, None)
            }
        };

        let (normalized_rel_path, blob_bytes, digest, stored_inline_content) = if let Some(
            ref rel,
        ) =
            req.relative_path
        {
            let Some(ws_path) = workspace_path else {
                let reason = if project_id.is_none() {
                    String::from(
                        "cannot attach workspace-relative file artifact to a standalone chat session without a bound project folder",
                    )
                } else {
                    String::from(
                        "cannot attach workspace-relative file artifact to a project without a bound workspace folder",
                    )
                };
                return Err(DennettError::InvalidInput {
                    field: String::from("relative_path"),
                    reason,
                });
            };
            let rel_owned = rel.clone();
            let inline_owned = req.content.clone();
            let scope_label = project_id.map_or_else(
                || format!("session:{session_id}"),
                |pid| format!("project:{pid}"),
            );
            let (safe_rel, snap_bytes, snap_digest, snap_inline) =
                tokio::task::spawn_blocking(move || {
                    let ws_root = PathBuf::from(&ws_path);
                    let safe_rel =
                        resolve_within_workspace(&ws_root, &rel_owned).map_err(|reason| {
                            DennettError::AccessDenied {
                                scope: scope_label.clone(),
                                reason,
                            }
                        })?;
                    if let Some(inline) = inline_owned {
                        let bytes = inline.as_bytes().to_vec();
                        let computed = sha256_hex(&bytes);
                        Ok::<_, DennettError>((safe_rel, bytes, computed, Some(inline)))
                    } else {
                        let full_path = ws_root.join(&safe_rel);
                        let (bytes, computed) = read_regular_file_snapshot(
                            &full_path,
                            MAX_ARTIFACT_SNAPSHOT_BYTES,
                            &scope_label,
                            &safe_rel,
                        )?;
                        let inline_preview = if bytes.len() <= MAX_ARTIFACT_INLINE_PREVIEW_BYTES {
                            String::from_utf8(bytes.clone()).ok()
                        } else {
                            None
                        };
                        Ok::<_, DennettError>((safe_rel, bytes, computed, inline_preview))
                    }
                })
                .await
                .map_err(|err| DennettError::Internal {
                    detail: format!("artifact snapshot task failed: {err}"),
                })??;
            (Some(safe_rel), snap_bytes, snap_digest, snap_inline)
        } else {
            let bytes = req
                .content
                .as_deref()
                .unwrap_or_default()
                .as_bytes()
                .to_vec();
            let computed = sha256_hex(&bytes);
            (None, bytes, computed, req.content)
        };

        let content_byte_size =
            i64::try_from(blob_bytes.len()).map_err(|_| DennettError::InvalidInput {
                field: String::from("content"),
                reason: String::from("artifact snapshot size exceeds representable range"),
            })?;

        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        // Serialize concurrent revision allocations for the same session cleanly under READ COMMITTED.
        let locked_session = tx
            .query_opt(
                "SELECT session_id FROM sessions WHERE session_id = $1 FOR UPDATE",
                &[&session_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
        if locked_session.is_none() {
            return Err(DennettError::NotFound {
                resource: String::from("session"),
                id: session_id.to_string(),
            });
        }

        let prev_rev: i64 = tx
            .query_one(
                "SELECT COALESCE(MAX(revision), 0)::bigint
                 FROM artifacts WHERE session_id = $1 AND name = $2",
                &[&session_id.as_uuid(), &req.name.trim()],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?
            .get(0);
        let revision = prev_rev + 1;
        let artifact_id = ArtifactId::new();

        tx.execute(
            "INSERT INTO artifact_blobs (sha256, bytes, byte_size)
             VALUES ($1, $2, $3)
             ON CONFLICT (sha256) DO NOTHING",
            &[&digest.as_str(), &blob_bytes, &content_byte_size],
        )
        .await
        .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let row = tx
            .query_one(
                "INSERT INTO artifacts (
                    artifact_id, session_id, project_id, run_id, name, mime_type,
                    revision, content, relative_path, sha256, content_byte_size,
                    completeness_status, lifecycle_state
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, 'active')
                 RETURNING TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint",
                &[
                    &artifact_id.as_uuid(),
                    &session_id.as_uuid(),
                    &project_id.map(ProjectId::as_uuid),
                    &req.run_id.map(RunId::as_uuid),
                    &req.name.trim(),
                    &req.mime_type.trim(),
                    &revision,
                    &stored_inline_content,
                    &normalized_rel_path,
                    &digest.as_str(),
                    &content_byte_size,
                    &req.completeness_status.as_str(),
                ],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let _ = append_audit_event(
            &tx,
            &self.inner.pg_target,
            correlation_id,
            Some(session_id),
            req.run_id,
            "head",
            "artifact_created",
            &format!(
                "registered artifact `{}` rev {revision} ({artifact_id}) status={}",
                req.name.trim(),
                req.completeness_status
            ),
        )
        .await?;

        let _ = append_control_event_in_tx(
            &tx,
            &self.inner.pg_target,
            ControlEventAppend {
                entity_key: &format!("artifact:{artifact_id}"),
                event_type: "artifact.created",
                project_id,
                session_id: Some(session_id),
                task_id: None,
                correlation_id,
                payload: json!({
                    "artifact_id": artifact_id,
                    "session_id": session_id,
                    "name": req.name.trim(),
                    "revision": revision,
                    "completeness_status": req.completeness_status,
                    "lifecycle_state": ArtifactLifecycleState::Active,
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        Ok(ArtifactRecord {
            artifact_id,
            session_id,
            project_id,
            run_id: req.run_id,
            name: req.name.trim().to_owned(),
            mime_type: req.mime_type.trim().to_owned(),
            revision,
            content: stored_inline_content,
            relative_path: normalized_rel_path,
            sha256: digest,
            completeness_status: req.completeness_status,
            lifecycle_state: ArtifactLifecycleState::Active,
            created_at: TimestampMs::from_millis(row.get(0)),
        })
    }

    /// Retrieves the exact immutable content-addressed bytes for one persisted artifact revision,
    /// verifying SHA-256 integrity against `artifacts.sha256` (Contract D §D.10).
    ///
    /// Falls back to legacy inline `artifacts.content` when `artifact_blobs` has no pre-v10 row,
    /// and rejects `deleted` tombstones with [`DennettError::NotFound`].
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`], [`DennettError::Internal`] (on digest mismatch), or
    /// [`DennettError::DatabaseUnavailable`].
    pub async fn get_artifact_content(
        &self,
        session_id: SessionId,
        artifact_id: ArtifactId,
    ) -> Result<ArtifactContentBlob, DennettError> {
        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let row = conn
            .query_opt(
                "SELECT a.mime_type, a.sha256, a.lifecycle_state, a.content, b.bytes
                 FROM artifacts a
                 LEFT JOIN artifact_blobs b ON b.sha256 = a.sha256
                 WHERE a.session_id = $1 AND a.artifact_id = $2",
                &[&session_id.as_uuid(), &artifact_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let Some(row) = row else {
            return Err(DennettError::NotFound {
                resource: String::from("artifact"),
                id: artifact_id.to_string(),
            });
        };

        let mime_type: String = row.get(0);
        let expected_sha256 = Sha256Hex::try_new(row.get::<_, String>(1))?;
        let lifecycle_state = ArtifactLifecycleState::from_str(&row.get::<_, String>(2))?;
        if lifecycle_state == ArtifactLifecycleState::Deleted {
            return Err(DennettError::NotFound {
                resource: String::from("artifact_content"),
                id: artifact_id.to_string(),
            });
        }

        let legacy_content: Option<String> = row.get(3);
        let blob_bytes: Option<Vec<u8>> = row.get(4);
        let bytes = blob_bytes
            .or_else(|| legacy_content.map(String::into_bytes))
            .ok_or_else(|| DennettError::NotFound {
                resource: String::from("artifact_content"),
                id: artifact_id.to_string(),
            })?;

        let actual_sha256 = sha256_hex(&bytes);
        if actual_sha256 != expected_sha256 {
            return Err(DennettError::Internal {
                detail: format!(
                    "artifact {artifact_id} immutable blob integrity check failed: expected {expected_sha256}, computed {actual_sha256}"
                ),
            });
        }

        Ok(ArtifactContentBlob {
            bytes,
            mime_type,
            sha256: expected_sha256,
        })
    }

    /// Lists all persisted artifacts for `session_id`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`] or [`DennettError::DatabaseUnavailable`].
    pub async fn list_artifacts(
        &self,
        session_id: SessionId,
    ) -> Result<ListArtifactsResponse, DennettError> {
        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let _ = fetch_session(&conn, session_id, &self.inner.pg_target).await?;
        let artifacts = fetch_session_artifacts(&conn, session_id, &self.inner.pg_target).await?;
        Ok(ListArtifactsResponse { artifacts })
    }

    /// Formats incoming plain-text inter-agent coordination messages consistently for both
    /// user chats and durable task generations (`D2-OVER-02`).
    pub(crate) fn format_inter_agent_messages_block(
        &self,
        messages: &[dennett_contracts::AgentMessageRecord],
    ) -> String {
        let mut block = String::from("[Messages from other agents]\n");
        for msg in messages {
            let sender_label = msg
                .sender_agent_id
                .map_or_else(|| String::from("user"), |id| format!("agent:{id}"));
            let _ = writeln!(
                block,
                "- from {sender_label}: {}",
                self.redact(&msg.body_text)
            );
        }
        block
    }

    /// Unified session turn execution core shared between `send_message` and `execute_task_generation`
    /// (`D2-OVER-02`): builds MCP specs (`dennett-memory` + `dennett-control`), starts/resumes the
    /// runtime session, enforces live external `PolicyEngine` + `InteractivePolicyEvaluator`, tracks
    /// `human_wait_ms` exclusion (`D2-BUG-03`), and dispatches emitted partial artifacts and external effects.
    #[expect(
        clippy::too_many_lines,
        reason = "shared turn core coordinates workspace policy, MCP specs, runtime turn I/O, live policy evaluation, and post-turn artifact/effect processing"
    )]
    pub(crate) async fn execute_session_turn_core(
        &self,
        input: CoreTurnInput,
    ) -> Result<CoreTurnOutput, CoreTurnFailure> {
        let task_id = match input.trigger {
            TurnTrigger::UserMessage => None,
            TurnTrigger::TaskGeneration { task_id } => Some(task_id),
        };
        let started_instant = Instant::now();

        // Resolve effective workspace directory & write policy:
        // - Folder-bound projects use their canonical workspace + agent write policy.
        // - Folderless projects use an isolated detached scratch directory + mandatory `DenyAll`.
        // - Standalone chats (`project = None`) use `<managed_root>/standalone-sessions/<session_id>`
        //   and mandatory `DenyAll`, with NO `dennett-control-mcp` mounted.
        let project_id_opt = input.project.as_ref().map(|p| p.project_id);
        let (effective_workspace_cwd, effective_write_policy) = match input.project {
            Some(ref proj) => match proj.workspace_path {
                Some(ref ws_path) => (
                    PathBuf::from(ws_path),
                    input.agent.current_config.workspace_write_policy.clone(),
                ),
                None => match self.ensure_detached_project_dir(proj.project_id) {
                    Ok(dir) => (dir, WorkspaceWritePolicy::DenyAll),
                    Err(error) => {
                        return Err(CoreTurnFailure {
                            error,
                            effective_elapsed_ms: 0,
                            captured_decisions: Vec::new(),
                        });
                    }
                },
            },
            None => match self.ensure_standalone_session_dir(input.session_id) {
                Ok(dir) => (dir, WorkspaceWritePolicy::DenyAll),
                Err(error) => {
                    return Err(CoreTurnFailure {
                        error,
                        effective_elapsed_ms: 0,
                        captured_decisions: Vec::new(),
                    });
                }
            },
        };

        // Build dynamically scoped MCP server specs:
        // - Always preserve `build_memory_mcp_specs` using the agent's own `allowed_memory_scopes`
        //   (e.g. `chat:standalone` for standalone chats — never leaking project scopes!).
        // - Mount `dennett-control-mcp` for BOTH project-bound sessions and standalone chats
        //   (`project_id = None` exposes ONLY `dennett_skill_list`/`dennett_skill_read` while
        //   failing closed on all project events/tasks/delegation tools!).
        let mut mcp_servers = match self.build_memory_mcp_specs(&input.agent) {
            Ok(specs) => specs,
            Err(error) => {
                return Err(CoreTurnFailure {
                    error,
                    effective_elapsed_ms: 0,
                    captured_decisions: Vec::new(),
                });
            }
        };
        match self.build_control_mcp_spec(
            project_id_opt,
            input.agent.agent_id,
            input.session_id,
            input.session_kind,
        ) {
            Ok(Some(ctrl_spec)) => mcp_servers.push(ctrl_spec),
            Ok(None) => {}
            Err(error) => {
                return Err(CoreTurnFailure {
                    error,
                    effective_elapsed_ms: 0,
                    captured_decisions: Vec::new(),
                });
            }
        }
        // Resolve active A08 capabilities (trusted & enabled skills + MCP servers with SecretBroker env).
        let resolved_caps = match self
            .resolve_active_capabilities_for_session(
                project_id_opt,
                &input.agent.current_config.attached_skills,
            )
            .await
        {
            Ok(caps) => caps,
            Err(error) => {
                return Err(CoreTurnFailure {
                    error,
                    effective_elapsed_ms: 0,
                    captured_decisions: Vec::new(),
                });
            }
        };
        mcp_servers.extend(resolved_caps.extra_mcp_servers);
        let turn_secrets = self.collect_turn_secrets(&mcp_servers);

        let skills_index = format_session_skills_prompt_index(&resolved_caps.session_skills);
        let effective_system_prompt = if skills_index.is_empty() {
            input.agent.current_config.system_prompt.clone()
        } else {
            format!(
                "{}{}",
                input.agent.current_config.system_prompt, skills_index
            )
        };

        let start_outcome = match input
            .runtime
            .start_or_resume_session(SessionStartRequest {
                dennett_session_id: input.session_id,
                workspace_cwd: effective_workspace_cwd.clone(),
                system_prompt: effective_system_prompt,
                model_preference: input.agent.current_config.model_preference.clone(),
                thinking_level: input.agent.current_config.thinking_level,
                workspace_write_policy: effective_write_policy.clone(),
                mcp_servers,
                active_skills: resolved_caps.active_skills,
                existing_binding: input.existing_binding,
                history_for_rehydrate: input.prior_history,
                allow_rehydrate: true,
            })
            .await
        {
            Ok(outcome) => outcome,
            Err(error) => {
                let elapsed = u64::try_from(started_instant.elapsed().as_millis()).unwrap_or(0);
                return Err(CoreTurnFailure {
                    error,
                    effective_elapsed_ms: elapsed,
                    captured_decisions: Vec::new(),
                });
            }
        };

        let runtime_binding = match self
            .upsert_runtime_binding(
                input.session_id,
                &start_outcome,
                input.connection_id,
                &input.correlation_id,
                Some(input.run_id),
            )
            .await
        {
            Ok(binding) => binding,
            Err(error) => {
                let elapsed = u64::try_from(started_instant.elapsed().as_millis()).unwrap_or(0);
                return Err(CoreTurnFailure {
                    error,
                    effective_elapsed_ms: elapsed,
                    captured_decisions: Vec::new(),
                });
            }
        };

        // Load live lifecycle state + delegated parent ceiling + budget exhaustion right before turn starts.
        let (live_rules, live_lifecycle, live_ws_bound, live_parent_ceiling, live_budget_exhausted) =
            match self.load_live_policy_inputs(project_id_opt, task_id).await {
                Ok(live) => (
                    live.rules,
                    live.project_lifecycle_state,
                    live.workspace_bound,
                    live.parent_write_ceiling,
                    live.task_budget_exhausted,
                ),
                Err(_) => (
                    input.policy_rules,
                    input
                        .project
                        .as_ref()
                        .map_or(ProjectLifecycleState::Detached, |p| p.lifecycle_state),
                    input
                        .project
                        .as_ref()
                        .is_some_and(|p| p.workspace_path.is_some()),
                    project_id_opt
                        .is_none()
                        .then_some(WorkspaceWritePolicy::DenyAll),
                    false,
                ),
            };

        let policy_ctx = PolicyContext {
            project_id: project_id_opt,
            agent_id: input.agent.agent_id,
            session_id: input.session_id,
            task_id,
            project_lifecycle_state: live_lifecycle,
            workspace_bound: live_ws_bound,
            workspace_root: effective_workspace_cwd,
            workspace_write_policy: effective_write_policy,
            parent_write_ceiling: live_parent_ceiling,
            allowed_memory_scopes: input.agent.current_config.allowed_memory_scopes.clone(),
            registered_mcp_servers: resolved_caps.registered_mcp_server_names,
            task_budget_exhausted: live_budget_exhausted,
            rules: live_rules,
        };
        let (interactive_policy, decisions_buf) = InteractivePolicyEvaluator::new(
            self.clone(),
            policy_ctx,
            task_id,
            input.correlation_id.clone(),
            turn_secrets.clone(),
            input.cancel_rx.clone(),
            input.approval_timeout,
        );

        let (event_tx, mut event_rx) = mpsc::channel::<RuntimeEvent>(256);
        let live_svc = self.clone();
        let live_session_id = input.session_id;
        let live_run_id = input.run_id;
        let live_turn_secrets = turn_secrets;
        let live_consumer = tokio::spawn(async move {
            let mut had_tool_since_last_text = false;
            let mut last_ctx_used: Option<u64> = None;
            let mut last_ctx_window: Option<u64> = None;
            while let Some(ev) = event_rx.recv().await {
                match ev {
                    RuntimeEvent::TextDelta { text } => {
                        if text.is_empty() {
                            continue;
                        }
                        let safe_delta = redact_secrets_owned(&text, &live_turn_secrets);
                        live_svc.mutate_live_snapshot(live_session_id, live_run_id, |snap| {
                            if had_tool_since_last_text
                                && !snap.streaming_text.is_empty()
                                && !snap.streaming_text.ends_with('\n')
                                && !safe_delta.starts_with('\n')
                            {
                                snap.streaming_text.push_str("\n\n");
                            }
                            snap.streaming_text.push_str(&safe_delta);
                            snap.updated_at = TimestampMs::now();
                        });
                        had_tool_since_last_text = false;
                    }
                    RuntimeEvent::ToolObserved { .. } => {
                        // CRITICAL OWNER RULE (#158): ZERO tool calls in LiveRunSnapshot;
                        // only track boundary so subsequent assistant prose separates cleanly with \n\n.
                        had_tool_since_last_text = true;
                    }
                    RuntimeEvent::ContextUsageUpdated {
                        used_tokens,
                        window_tokens,
                    } => {
                        if used_tokens.is_some() {
                            last_ctx_used = used_tokens;
                        }
                        if window_tokens.is_some() {
                            last_ctx_window = window_tokens;
                        }
                        live_svc.mutate_live_snapshot(live_session_id, live_run_id, |snap| {
                            if let Some(used) = used_tokens {
                                snap.context_used_tokens = Some(used);
                            }
                            if let Some(window) = window_tokens {
                                snap.context_window_tokens = Some(window);
                            }
                            snap.updated_at = TimestampMs::now();
                        });
                    }
                    _ => {}
                }
            }
            (last_ctx_used, last_ctx_window)
        });

        let turn_result = input
            .runtime
            .send_prompt_turn(
                TurnRequest {
                    dennett_session_id: input.session_id,
                    provider_session_id: start_outcome.provider_session_id.clone(),
                    run_id: input.run_id,
                    prompt_text: input.prompt_text,
                    timeout: input.turn_timeout,
                    cancel_rx: input.cancel_rx.clone(),
                },
                Arc::new(interactive_policy),
                Some(event_tx),
            )
            .await;

        let (streamed_ctx_used, streamed_ctx_window) =
            match time::timeout(Duration::from_secs(2), live_consumer).await {
                Ok(Ok(pair)) => pair,
                _ => (None, None),
            };

        let raw_elapsed_ms =
            u64::try_from(started_instant.elapsed().as_millis()).unwrap_or(u64::MAX);
        let captured_decisions: Vec<CapturedPermissionDecision> =
            decisions_buf.lock().map_or_default(|g| g.clone());

        let turn_outcome = match turn_result {
            Ok(outcome) => outcome,
            Err(error) => {
                return Err(CoreTurnFailure {
                    error,
                    effective_elapsed_ms: raw_elapsed_ms,
                    captured_decisions,
                });
            }
        };

        let (outcome_ctx_used, outcome_ctx_window) = turn_outcome.context_usage();
        let context_used_tokens = outcome_ctx_used.or(streamed_ctx_used);
        let context_window_tokens = outcome_ctx_window.or(streamed_ctx_window);

        // D2-BUG-03: Exclude human approval wait time from run/task machine wall-time accounting!
        let effective_elapsed_ms = raw_elapsed_ms.saturating_sub(turn_outcome.human_wait_ms);

        // Process emitted partial artifacts and external effects (strictly checking cancellation first!).
        let mut hit_unknown_external = false;
        for ev in &turn_outcome.events {
            match ev {
                RuntimeEvent::PartialArtifactEmitted { name, content } => {
                    let mime_type = infer_artifact_mime_type(name).to_owned();
                    let _ = self
                        .create_artifact(
                            input.session_id,
                            CreateArtifactRequest {
                                name: name.clone(),
                                mime_type,
                                content: Some(content.clone()),
                                relative_path: None,
                                run_id: Some(input.run_id),
                                completeness_status: ArtifactCompleteness::Partial,
                            },
                            &input.correlation_id,
                        )
                        .await;
                }
                RuntimeEvent::ExternalEffectRequested {
                    recipient,
                    operation,
                    behavior,
                    idempotency_key,
                } => {
                    let cancelled = *input.cancel_rx.borrow()
                        || match task_id {
                            Some(tid) => self.is_task_cancel_signalled(tid).await,
                            None => false,
                        };
                    if cancelled {
                        break;
                    }
                    if let Some(pid) = project_id_opt
                        && let Ok(idem) = IdempotencyKey::try_new(idempotency_key.clone())
                    {
                        let sim_behavior = SimEffectBehavior::from_str(behavior)
                            .unwrap_or(SimEffectBehavior::Success);
                        if let Ok(eff_resp) = self
                            .dispatch_external_effect(
                                DispatchEffectRequest {
                                    project_id: pid,
                                    session_id: input.session_id,
                                    task_id,
                                    run_id: Some(input.run_id),
                                    agent_id: input.agent.agent_id,
                                    connector_id: Some(String::from(
                                        SIMULATED_EXTERNAL_CONNECTOR_ID,
                                    )),
                                    operation: operation.clone(),
                                    target_recipient: recipient.clone(),
                                    parameters_json: json!({
                                        "session_id": input.session_id,
                                        "run_id": input.run_id,
                                        "task_id": task_id,
                                    })
                                    .to_string(),
                                    effect_idempotency_key: idem,
                                    simulated_behavior: Some(sim_behavior),
                                },
                                &input.correlation_id,
                            )
                            .await
                            && eff_resp.external_outcome_unknown
                        {
                            hit_unknown_external = true;
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(CoreTurnOutput {
            start_outcome,
            runtime_binding,
            turn_outcome,
            effective_elapsed_ms,
            context_used_tokens,
            context_window_tokens,
            captured_decisions,
            hit_unknown_external,
        })
    }

    /// Persists a user message BEFORE invoking the runtime, spawns the turn in a **detached**
    /// background worker owned by `HeadService` (`D2-ARCH-01` / `V03`: HTTP disconnects or window
    /// reloads NEVER abort the running agent turn!), and awaits completion for the caller.
    ///
    /// Supports optional `client_message_id` idempotency (`UNIQUE (session_id, client_message_id)`):
    /// - Replaying the same `client_message_id` with identical content while the run is STILL
    ///   in-flight joins the active run's completion channel and returns its result with
    ///   `deduplicated = true` (zero double execution, no spurious `409 Conflict`!).
    /// - Replaying after completion returns the persisted [`SendMessageResponse`] (`deduplicated = true`).
    /// - Reusing `client_message_id` with different content fails closed with `409 IdempotencyMismatch`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError`] on validation, missing session, idempotency conflict, concurrent
    /// active run conflict, auth failure, runtime error, or database unavailability.
    #[expect(
        clippy::too_many_lines,
        reason = "send_message orchestrates detached run execution, client_message_id join/replay, pre-run persistence, and clean awaiting"
    )]
    pub async fn send_message(
        &self,
        session_id: SessionId,
        req: SendMessageRequest,
        correlation_id: CorrelationId,
    ) -> Result<SendMessageResponse, DennettError> {
        req.validate()?;
        let _ = self.ensure_migrated().await?;

        let client_message_id = req
            .client_message_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned);
        let turn_timeout = req
            .timeout_ms
            .map_or(DEFAULT_TURN_TIMEOUT, Duration::from_millis);
        // When an explicit short `timeout_ms` is requested (e.g. automated tests), align approval
        // expiry inside it; for normal interactive chats (`timeout_ms = None`), allow the full
        // `DEFAULT_APPROVAL_WAIT_TIMEOUT` while pausing the RPC deadline (`D2-BUG-03`).
        let approval_timeout = match req.timeout_ms {
            Some(explicit_ms) => Duration::from_millis(explicit_ms)
                .saturating_sub(Duration::from_millis(200))
                .max(Duration::from_millis(250))
                .min(DEFAULT_APPROVAL_WAIT_TIMEOUT),
            None => DEFAULT_APPROVAL_WAIT_TIMEOUT,
        };

        let (completion_tx, mut completion_rx) =
            watch::channel::<Option<Result<SendMessageResponse, DennettError>>>(None);

        // 0. Client-side retry idempotency (`client_message_id`): join active in-flight run OR
        //    replay persisted completed run without ever returning spurious 409 (`D2-ARCH-01`).
        let mut in_flight_cmid_guard: Option<InFlightClientMessageGuard> = None;
        if let Some(ref cmid) = client_message_id {
            loop {
                let existing_rx = {
                    let mut map = self.inner.in_flight_client_messages.lock().await;
                    let key = (session_id, cmid.clone());
                    if let Some(entry) = map.get(&key) {
                        if entry.content != req.content {
                            return Err(DennettError::IdempotencyMismatch {
                                idempotency_key: cmid.clone(),
                                reason: String::from(
                                    "reused client_message_id on this session with different message content",
                                ),
                            });
                        }
                        Some(entry.completion_rx.clone())
                    } else {
                        map.insert(
                            key.clone(),
                            InFlightClientMessageEntry {
                                content: req.content.clone(),
                                completion_rx: completion_rx.clone(),
                            },
                        );
                        in_flight_cmid_guard = Some(InFlightClientMessageGuard {
                            map: Arc::clone(&self.inner.in_flight_client_messages),
                            key,
                        });
                        None
                    }
                };

                if let Some(mut rx) = existing_rx {
                    if let Ok(guard) = rx.wait_for(Option::is_some).await
                        && let Some(res) = guard.as_ref()
                    {
                        return match res {
                            Ok(resp) => {
                                let mut dedup = resp.clone();
                                dedup.deduplicated = true;
                                Ok(dedup)
                            }
                            Err(err) => Err(err.clone()),
                        };
                    }
                    // Leading attempt exited before broadcasting; check PG replay or retry.
                    if let Some(replayed) = self
                        .try_replay_client_message(session_id, cmid, &req.content, &correlation_id)
                        .await?
                    {
                        return Ok(replayed);
                    }
                    continue;
                }

                if let Some(replayed) = self
                    .try_replay_client_message(session_id, cmid, &req.content, &correlation_id)
                    .await?
                {
                    return Ok(replayed);
                }
                break;
            }
        }

        // 1. Load session, agent+config, project, prior history, and policy rules.
        let (session, agent, project, existing_binding, prior_history, policy_rules) = {
            let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
            let session = fetch_session(&conn, session_id, &self.inner.pg_target).await?;
            if session.status != SessionStatus::Active {
                return Err(DennettError::Conflict {
                    resource: String::from("session"),
                    expected_revision: None,
                    actual_revision: None,
                    reason: format!(
                        "session is `{}` and does not accept new messages",
                        session.status
                    ),
                });
            }
            let agent = fetch_agent(&conn, session.agent_id, &self.inner.pg_target).await?;
            let project = if let Some(pid) = session.project_id {
                let proj = fetch_project(&conn, pid, &self.inner.pg_target).await?;
                if matches!(
                    proj.lifecycle_state,
                    ProjectLifecycleState::Archived | ProjectLifecycleState::DeletedRecord
                ) {
                    return Err(DennettError::Conflict {
                        resource: String::from("project"),
                        expected_revision: None,
                        actual_revision: None,
                        reason: format!(
                            "project `{}` is `{}` and does not accept new messages",
                            proj.project_id, proj.lifecycle_state
                        ),
                    });
                }
                Some(proj)
            } else {
                None
            };
            let binding = fetch_runtime_binding(&conn, session_id, &self.inner.pg_target).await?;
            let history = fetch_session_messages(&conn, session_id, &self.inner.pg_target).await?;
            let rules = fetch_policy_rules(&conn, &self.inner.pg_target).await?;
            (session, agent, project, binding, history, rules)
        };

        // Resolve target runtime strictly without silent fallback; if resolution fails (e.g.
        // missing GEMINI_API_KEY), we still record a durable `RunStatus::Failed` row below (A09).
        let resolved_runtime_res = self
            .resolve_runtime_for_turn(agent.current_config.preferred_connection_id, None)
            .await;
        let runtime_adapter_for_audit = resolved_runtime_res.as_ref().map_or_else(
            |_| String::from("unresolved"),
            |r| r.runtime.adapter_id().to_owned(),
        );

        // 2. Acquire unified per-session execution lock + active run slot (`H2-05`).
        let session_lock = self.session_exec_lock(session_id).await;
        let Ok(session_guard) = session_lock.try_lock_owned() else {
            // Belt-and-suspenders: if an active run with the SAME client_message_id holds the
            // slot, join it instead of returning 409 Conflict (`D2-ARCH-01`).
            if let Some(ref cmid) = client_message_id {
                let existing_rx = {
                    let active = self.inner.active_runs.lock().await;
                    active
                        .get(&session_id)
                        .filter(|h| h.client_message_id.as_deref() == Some(cmid.as_str()))
                        .and_then(|h| h.completion_rx.clone())
                };
                if let Some(mut rx) = existing_rx {
                    drop(in_flight_cmid_guard);
                    if let Ok(guard) = rx.wait_for(Option::is_some).await
                        && let Some(res) = guard.as_ref()
                    {
                        return match res {
                            Ok(resp) => {
                                let mut dedup = resp.clone();
                                dedup.deduplicated = true;
                                Ok(dedup)
                            }
                            Err(err) => Err(err.clone()),
                        };
                    }
                }
            }
            return Err(DennettError::Conflict {
                resource: String::from("session_run"),
                expected_revision: None,
                actual_revision: None,
                reason: String::from("another run is already active in this session"),
            });
        };

        let run_id = RunId::new();
        let (cancel_tx, cancel_rx) = watch::channel(false);
        let cancel_reason = Arc::new(AtomicU8::new(CANCEL_REASON_NONE));
        let (slot_released_tx, slot_released_rx) = watch::channel(false);
        let slot_released = SlotReleasedNotifier::new(slot_released_tx);
        self.try_register_active_session_run(
            session_id,
            ActiveRunHandle {
                run_id,
                client_message_id: client_message_id.clone(),
                cancel_tx: cancel_tx.clone(),
                cancel_reason: Arc::clone(&cancel_reason),
                completion_rx: Some(completion_rx.clone()),
                slot_released_rx,
            },
        )
        .await?;
        let mut active_guard = ActiveRunGuard::new(
            self.clone(),
            session_id,
            run_id,
            cancel_tx,
            correlation_id.clone(),
        );

        // Determine automatic title if this is the very first user message on an untitled chat.
        let auto_title = if !prior_history.iter().any(|m| m.role == MessageRole::User)
            && is_default_session_title(&session.title)
        {
            derive_auto_session_title(&req.content)
        } else {
            None
        };
        let effective_session_title = auto_title.clone().unwrap_or_else(|| session.title.clone());

        // 3. Persist user message + running RunRecord + claim undelivered general inter-agent
        //    messages (`related_task_id IS NULL`, `D2-BUG-04`) BEFORE spawning runtime work.
        let pre_run_res: Result<
            (MessageRecord, Vec<dennett_contracts::AgentMessageRecord>),
            Result<SendMessageResponse, DennettError>,
        > = {
            let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
            let tx = conn
                .transaction()
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

            let msg_id = MessageId::new();
            let stream_id = format!("session:{session_id}:messages");
            let msg_seq = next_stream_seq(&tx, &stream_id, &self.inner.pg_target).await?;

            if let Some(ref new_title) = auto_title {
                tx.execute(
                    "UPDATE sessions SET title = $2, updated_at = now() WHERE session_id = $1",
                    &[&session_id.as_uuid(), &new_title.as_str()],
                )
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
            }

            tx.execute(
                "INSERT INTO runs (
                    run_id, session_id, agent_id, agent_config_version_id,
                    user_message_id, status, correlation_id
                 ) VALUES ($1, $2, $3, $4, $5, 'running', $6)",
                &[
                    &run_id.as_uuid(),
                    &session_id.as_uuid(),
                    &agent.agent_id.as_uuid(),
                    &agent.current_config_version_id.as_uuid(),
                    &msg_id.as_uuid(),
                    &correlation_id.as_str(),
                ],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

            let msg_insert = tx
                .query_one(
                    "INSERT INTO messages (
                        message_id, session_id, stream_seq, role, content, run_id,
                        client_message_id, correlation_id
                     ) VALUES ($1, $2, $3, 'user', $4, $5, $6, $7)
                     RETURNING TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint",
                    &[
                        &msg_id.as_uuid(),
                        &session_id.as_uuid(),
                        &msg_seq,
                        &req.content,
                        &run_id.as_uuid(),
                        &client_message_id,
                        &correlation_id.as_str(),
                    ],
                )
                .await;

            match msg_insert {
                Ok(msg_row) => {
                    let project_id_opt = project.as_ref().map(|p| p.project_id);
                    let inbox_msgs = if let Some(pid) = project_id_opt {
                        let agent_msg_sql = format!(
                            "SELECT {AGENT_MESSAGE_SELECT_COLS}
                             FROM agent_messages
                             WHERE recipient_agent_id = $1
                               AND project_id = $2
                               AND related_task_id IS NULL
                               AND delivered_in_run_id IS NULL
                             ORDER BY created_at ASC
                             FOR UPDATE"
                        );
                        let agent_msg_rows = tx
                            .query(&agent_msg_sql, &[&agent.agent_id.as_uuid(), &pid.as_uuid()])
                            .await
                            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
                        let mut claimed = Vec::with_capacity(agent_msg_rows.len());
                        for row in &agent_msg_rows {
                            claimed.push(decode_agent_message_row(row)?);
                        }
                        if !claimed.is_empty() {
                            let ids: Vec<uuid::Uuid> = claimed
                                .iter()
                                .map(|m| m.agent_message_id.as_uuid())
                                .collect();
                            tx.execute(
                                "UPDATE agent_messages
                                 SET delivered_in_run_id = $2, delivered_at = now()
                                 WHERE agent_message_id = ANY($1)",
                                &[&ids, &run_id.as_uuid()],
                            )
                            .await
                            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
                        }
                        claimed
                    } else {
                        Vec::new()
                    };

                    let _ = append_audit_event(
                        &tx,
                        &self.inner.pg_target,
                        &correlation_id,
                        Some(session_id),
                        Some(run_id),
                        "head",
                        "run_started",
                        &format!(
                            "started run {run_id} for agent `{}` config v{} adapter={}",
                            agent.name,
                            agent.current_config.version_number,
                            runtime_adapter_for_audit
                        ),
                    )
                    .await?;

                    // D2-BUG-04: Emit `session.run_started` so UI/event subscribers observe active runs live.
                    let _ = append_control_event_in_tx(
                        &tx,
                        &self.inner.pg_target,
                        ControlEventAppend {
                            entity_key: &format!("session:{session_id}"),
                            event_type: "session.run_started",
                            project_id: project_id_opt,
                            session_id: Some(session_id),
                            task_id: None,
                            correlation_id: &correlation_id,
                            payload: json!({
                                "session_id": session_id,
                                "run_id": run_id,
                                "agent_id": agent.agent_id,
                                "title": effective_session_title,
                            }),
                        },
                    )
                    .await?;

                    tx.commit()
                        .await
                        .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

                    Ok((
                        MessageRecord {
                            message_id: msg_id,
                            session_id,
                            role: MessageRole::User,
                            content: req.content.clone(),
                            run_id: Some(run_id),
                            client_message_id: client_message_id.clone(),
                            correlation_id: correlation_id.clone(),
                            created_at: TimestampMs::from_millis(msg_row.get(0)),
                        },
                        inbox_msgs,
                    ))
                }
                Err(pg_err) if is_pg_unique_violation(&pg_err) && client_message_id.is_some() => {
                    drop(tx);
                    drop(conn);
                    let cmid = client_message_id.as_deref().unwrap_or_default();
                    let replay = self
                        .try_replay_client_message(session_id, cmid, &req.content, &correlation_id)
                        .await
                        .and_then(|opt| {
                            opt.ok_or_else(|| DennettError::Conflict {
                                resource: String::from("message"),
                                expected_revision: None,
                                actual_revision: None,
                                reason: String::from(
                                    "duplicate client_message_id raced during insertion",
                                ),
                            })
                        });
                    Err(replay)
                }
                Err(pg_err) => {
                    active_guard.mark_completed();
                    self.unregister_active_session_run(session_id, run_id).await;
                    drop(in_flight_cmid_guard);
                    drop(session_guard);
                    drop(slot_released);
                    return Err(map_pg_err(&self.inner.pg_target, &pg_err));
                }
            }
        };

        let (user_message, inbox_agent_messages) = match pre_run_res {
            Ok(pair) => pair,
            Err(replay_res) => {
                active_guard.mark_completed();
                self.unregister_active_session_run(session_id, run_id).await;
                drop(in_flight_cmid_guard);
                drop(session_guard);
                drop(slot_released);
                return replay_res;
            }
        };

        let resolved_runtime = match resolved_runtime_res {
            Ok(rt) => rt,
            Err(err) => {
                self.record_failed_run(session_id, run_id, &correlation_id, &err, &[])
                    .await;
                self.pause_queued_followups_for_session(session_id, &correlation_id)
                    .await;
                active_guard.mark_completed();
                self.unregister_active_session_run(session_id, run_id).await;
                let _ = completion_tx.send(Some(Err(err.clone())));
                drop(in_flight_cmid_guard);
                drop(session_guard);
                drop(slot_released);
                return Err(err);
            }
        };

        let effective_prompt = if inbox_agent_messages.is_empty() {
            req.content.clone()
        } else {
            let mut combined = self.format_inter_agent_messages_block(&inbox_agent_messages);
            combined.push('\n');
            combined.push_str(&req.content);
            combined
        };

        // 4. Spawn the turn in a DETACHED task owned by `HeadService` (`D2-ARCH-01` / `V03`).
        //    HTTP disconnects drop ONLY the awaiter below — the detached worker completes cleanly!
        self.spawn_detached_session_turn_worker(DetachedTurnSpawnArgs {
            session_id,
            session_kind: session.session_kind,
            run_id,
            agent,
            project,
            existing_binding,
            prior_history,
            policy_rules,
            user_message,
            effective_prompt,
            effective_session_title,
            runtime: resolved_runtime.runtime,
            connection_id: resolved_runtime.connection_id,
            turn_timeout,
            approval_timeout,
            cancel_rx,
            cancel_reason,
            correlation_id,
            completion_tx,
            session_guard,
            active_guard,
            slot_released,
            in_flight_cmid_guard,
        });

        // 5. Caller awaits the detached worker's completion channel cleanly.
        match completion_rx.wait_for(Option::is_some).await {
            Ok(guard) => match guard.as_ref() {
                Some(res) => res.clone(),
                None => Err(DennettError::Internal {
                    detail: String::from("detached session turn completed without result"),
                }),
            },
            Err(_) => Err(DennettError::Internal {
                detail: String::from("detached session turn terminated unexpectedly"),
            }),
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "detached session worker owns turn execution, finalization, safe follow-up pause, slot release, and V03 auto-dispatch in strict drop order"
    )]
    fn spawn_detached_session_turn_worker(&self, args: DetachedTurnSpawnArgs) {
        let svc = self.clone();
        tokio::spawn(async move {
            let DetachedTurnSpawnArgs {
                session_id,
                session_kind,
                run_id,
                agent,
                project,
                existing_binding,
                prior_history,
                policy_rules,
                user_message,
                effective_prompt,
                effective_session_title,
                runtime,
                connection_id,
                turn_timeout,
                approval_timeout,
                cancel_rx,
                cancel_reason,
                correlation_id,
                completion_tx,
                session_guard,
                mut active_guard,
                slot_released,
                in_flight_cmid_guard,
            } = args;

            let (memory_status, _mem_detail) = svc.probe_memory_status(&correlation_id).await;

            let core_res = svc
                .execute_session_turn_core(CoreTurnInput {
                    trigger: TurnTrigger::UserMessage,
                    session_id,
                    session_kind,
                    run_id,
                    agent: agent.clone(),
                    project: project.clone(),
                    existing_binding,
                    prior_history,
                    policy_rules,
                    prompt_text: effective_prompt,
                    runtime,
                    connection_id,
                    turn_timeout,
                    approval_timeout,
                    cancel_rx,
                    correlation_id: correlation_id.clone(),
                })
                .await;

            let final_result: Result<SendMessageResponse, DennettError> = match core_res {
                Err(failure) => {
                    svc.record_failed_run(
                        session_id,
                        run_id,
                        &correlation_id,
                        &failure.error,
                        &failure.captured_decisions,
                    )
                    .await;
                    active_guard.mark_completed();
                    svc.unregister_active_session_run(session_id, run_id).await;
                    Err(failure.error)
                }
                Ok(core) => {
                    match svc
                        .finalize_turn(
                            session_id,
                            project.as_ref().map(|p| p.project_id),
                            run_id,
                            &agent,
                            user_message.message_id,
                            core.start_outcome.rehydrated,
                            &core.turn_outcome,
                            core.effective_elapsed_ms,
                            core.context_used_tokens,
                            core.context_window_tokens,
                            &core.captured_decisions,
                            core.hit_unknown_external,
                            &correlation_id,
                        )
                        .await
                    {
                        Ok((assistant_message, run_record)) => {
                            active_guard.mark_completed();
                            svc.unregister_active_session_run(session_id, run_id).await;
                            let artifacts = match svc.get_conn(DEFAULT_PG_OP_TIMEOUT).await {
                                Ok(conn) => {
                                    fetch_session_artifacts(&conn, session_id, &svc.inner.pg_target)
                                        .await
                                        .unwrap_or_default()
                                }
                                Err(_) => Vec::new(),
                            };
                            Ok(SendMessageResponse {
                                user_message,
                                assistant_message,
                                run: run_record,
                                runtime_binding: core.runtime_binding,
                                artifacts,
                                permission_decisions: core
                                    .captured_decisions
                                    .into_iter()
                                    .map(|c| c.record)
                                    .collect(),
                                memory_status,
                                session_title: effective_session_title,
                                deduplicated: false,
                            })
                        }
                        Err(err) => {
                            svc.record_failed_run(
                                session_id,
                                run_id,
                                &correlation_id,
                                &err,
                                &core.captured_decisions,
                            )
                            .await;
                            active_guard.mark_completed();
                            svc.unregister_active_session_run(session_id, run_id).await;
                            Err(err)
                        }
                    }
                }
            };

            let final_run_status = final_result.as_ref().ok().map(|resp| resp.run.status);
            let was_steer_cancel = cancel_reason.load(Ordering::Acquire) == CANCEL_REASON_STEER;
            let should_pause_followups = match final_run_status {
                Some(RunStatus::Cancelled | RunStatus::Failed | RunStatus::UnknownExternal)
                | None => !was_steer_cancel,
                Some(_) => false,
            };
            if should_pause_followups {
                svc.pause_queued_followups_for_session(session_id, &correlation_id)
                    .await;
            }

            // Notify awaiter and release execution lock BEFORE signaling slot_released /
            // kicking off the next queued follow-up (zero 409 Conflict races!).
            let _ = completion_tx.send(Some(final_result));
            drop(in_flight_cmid_guard);
            drop(session_guard);
            drop(slot_released);

            // V03-proof server-side auto-dispatch: survives closed Desktop UI.
            if final_run_status == Some(RunStatus::Succeeded)
                && !svc.is_session_steering(session_id)
            {
                let next_cid = CorrelationId::generate();
                let _ = svc
                    .try_dispatch_next_queued_followup_inner(session_id, None, &next_cid)
                    .await;
            }

            svc.prune_session_lock(session_id).await;
        });
    }

    /// Returns the ephemeral live snapshot of any active run in `session_id` together with its
    /// durable active (`queued` / `paused`) follow-up queue (`GET /v1/sessions/{session_id}/live`).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`] if `session_id` does not exist or
    /// [`DennettError::DatabaseUnavailable`] on PostgreSQL error.
    pub async fn get_session_live(
        &self,
        session_id: SessionId,
    ) -> Result<LiveRunResponse, DennettError> {
        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let _ = fetch_session(&conn, session_id, &self.inner.pg_target).await?;
        let followups =
            fetch_active_session_followups(&conn, session_id, &self.inner.pg_target).await?;
        drop(conn);
        let active = self.get_live_snapshot(session_id);
        Ok(LiveRunResponse { active, followups })
    }

    /// Enqueues a durable follow-up message behind any active run (`POST /v1/sessions/{id}/followups`).
    ///
    /// Deduplicates by `(session_id, idempotency_key)` (`409 IdempotencyMismatch` on conflicting
    /// content). If the session is idle and has no `paused` follow-ups right after commit,
    /// auto-dispatches immediately so a race right after run completion never strands a message.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::NotFound`],
    /// [`DennettError::Conflict`], [`DennettError::IdempotencyMismatch`], or
    /// [`DennettError::DatabaseUnavailable`].
    #[expect(
        clippy::too_many_lines,
        reason = "validates session/project state, handles idempotency dedup/conflict, inserts durable row, and triggers idle auto-dispatch"
    )]
    pub async fn enqueue_followup(
        &self,
        session_id: SessionId,
        req: EnqueueFollowupRequest,
        correlation_id: &CorrelationId,
    ) -> Result<SessionFollowupRecord, DennettError> {
        req.validate()?;
        let _ = self.ensure_migrated().await?;

        let idempotency_key = req
            .idempotency_key
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned);

        let inserted = {
            let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
            let tx = conn
                .transaction()
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

            let session = fetch_session(&tx, session_id, &self.inner.pg_target).await?;
            if session.status != SessionStatus::Active {
                return Err(DennettError::Conflict {
                    resource: String::from("session"),
                    expected_revision: None,
                    actual_revision: None,
                    reason: format!(
                        "session is `{}` and does not accept follow-up messages",
                        session.status
                    ),
                });
            }
            if let Some(pid) = session.project_id {
                let proj = fetch_project(&tx, pid, &self.inner.pg_target).await?;
                if matches!(
                    proj.lifecycle_state,
                    ProjectLifecycleState::Archived | ProjectLifecycleState::DeletedRecord
                ) {
                    return Err(DennettError::Conflict {
                        resource: String::from("project"),
                        expected_revision: None,
                        actual_revision: None,
                        reason: format!(
                            "project `{}` is `{}` and does not accept follow-up messages",
                            proj.project_id, proj.lifecycle_state
                        ),
                    });
                }
            }

            if let Some(ref key) = idempotency_key {
                let existing_sql = format!(
                    "SELECT {SESSION_FOLLOWUP_SELECT_COLS}
                     FROM session_followups
                     WHERE session_id = $1 AND idempotency_key = $2"
                );
                if let Some(row) = tx
                    .query_opt(&existing_sql, &[&session_id.as_uuid(), &key.as_str()])
                    .await
                    .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?
                {
                    let existing = decode_session_followup_row(&row)?;
                    if existing.content != req.content {
                        return Err(DennettError::IdempotencyMismatch {
                            idempotency_key: key.clone(),
                            reason: String::from(
                                "reused follow-up idempotency_key on this session with different content",
                            ),
                        });
                    }
                    tx.commit()
                        .await
                        .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
                    return Ok(existing);
                }
            }

            let followup_id = FollowupId::new();
            let stream_id = format!("session:{session_id}:followups");
            let position = next_stream_seq(&tx, &stream_id, &self.inner.pg_target).await?;
            let insert_sql = format!(
                "INSERT INTO session_followups (
                    followup_id, session_id, content, status, position, idempotency_key
                 ) VALUES ($1, $2, $3, 'queued', $4, $5)
                 RETURNING {SESSION_FOLLOWUP_SELECT_COLS}"
            );
            let insert_res = tx
                .query_one(
                    &insert_sql,
                    &[
                        &followup_id.as_uuid(),
                        &session_id.as_uuid(),
                        &req.content,
                        &position,
                        &idempotency_key,
                    ],
                )
                .await;

            let record = match insert_res {
                Ok(row) => decode_session_followup_row(&row)?,
                Err(pg_err) if is_pg_unique_violation(&pg_err) && idempotency_key.is_some() => {
                    drop(tx);
                    let key = idempotency_key.as_deref().unwrap_or_default();
                    let lookup_sql = format!(
                        "SELECT {SESSION_FOLLOWUP_SELECT_COLS}
                         FROM session_followups
                         WHERE session_id = $1 AND idempotency_key = $2"
                    );
                    let existing_row = conn
                        .query_opt(&lookup_sql, &[&session_id.as_uuid(), &key])
                        .await
                        .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
                    let Some(row) = existing_row else {
                        return Err(map_pg_err(&self.inner.pg_target, &pg_err));
                    };
                    let existing = decode_session_followup_row(&row)?;
                    if existing.content != req.content {
                        return Err(DennettError::IdempotencyMismatch {
                            idempotency_key: key.to_owned(),
                            reason: String::from(
                                "reused follow-up idempotency_key on this session with different content",
                            ),
                        });
                    }
                    return Ok(existing);
                }
                Err(pg_err) => return Err(map_pg_err(&self.inner.pg_target, &pg_err)),
            };

            let _ = append_control_event_in_tx(
                &tx,
                &self.inner.pg_target,
                ControlEventAppend {
                    entity_key: &format!("session:{session_id}"),
                    event_type: "session.followup_enqueued",
                    project_id: session.project_id,
                    session_id: Some(session_id),
                    task_id: None,
                    correlation_id,
                    payload: json!({
                        "session_id": session_id,
                        "followup_id": record.followup_id,
                        "position": record.position,
                    }),
                },
            )
            .await?;

            tx.commit()
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
            record
        };

        // If the session is idle (or finishing within milliseconds) and has no paused items,
        // auto-dispatch immediately so a race right after run completion never strands a message!
        if !self.is_session_active_or_steering(session_id).await
            && let Ok(Some((dispatched_fid, _handle))) = self
                .try_dispatch_next_queued_followup_inner(
                    session_id,
                    Some(IDLE_FOLLOWUP_LOCK_WAIT_TIMEOUT),
                    correlation_id,
                )
                .await
            && dispatched_fid == inserted.followup_id
            && let Ok(conn) = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await
        {
            let refresh_sql = format!(
                "SELECT {SESSION_FOLLOWUP_SELECT_COLS}
                 FROM session_followups
                 WHERE followup_id = $1"
            );
            if let Ok(Some(row)) = conn
                .query_opt(&refresh_sql, &[&inserted.followup_id.as_uuid()])
                .await
                && let Ok(refreshed) = decode_session_followup_row(&row)
            {
                return Ok(refreshed);
            }
        }

        Ok(inserted)
    }

    /// Updates a queued or paused follow-up message (`PATCH /v1/sessions/{id}/followups/{fid}`).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::NotFound`],
    /// [`DennettError::Conflict`] (if already dispatched/cancelled), or
    /// [`DennettError::DatabaseUnavailable`].
    pub async fn update_followup(
        &self,
        session_id: SessionId,
        followup_id: FollowupId,
        req: UpdateFollowupRequest,
        correlation_id: &CorrelationId,
    ) -> Result<SessionFollowupRecord, DennettError> {
        req.validate()?;
        let _ = self.ensure_migrated().await?;
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let session = fetch_session(&tx, session_id, &self.inner.pg_target).await?;
        let lock_sql = format!(
            "SELECT {SESSION_FOLLOWUP_SELECT_COLS}
             FROM session_followups
             WHERE session_id = $1 AND followup_id = $2
             FOR UPDATE"
        );
        let Some(row) = tx
            .query_opt(&lock_sql, &[&session_id.as_uuid(), &followup_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?
        else {
            return Err(DennettError::NotFound {
                resource: String::from("session_followup"),
                id: followup_id.to_string(),
            });
        };
        let existing = decode_session_followup_row(&row)?;
        if !matches!(
            existing.status,
            FollowupStatus::Queued | FollowupStatus::Paused
        ) {
            return Err(DennettError::Conflict {
                resource: String::from("session_followup"),
                expected_revision: None,
                actual_revision: None,
                reason: format!("cannot edit follow-up in status `{}`", existing.status),
            });
        }

        let update_sql = format!(
            "UPDATE session_followups
             SET content = $3, updated_at = now()
             WHERE session_id = $1 AND followup_id = $2
             RETURNING {SESSION_FOLLOWUP_SELECT_COLS}"
        );
        let updated_row = tx
            .query_one(
                &update_sql,
                &[&session_id.as_uuid(), &followup_id.as_uuid(), &req.content],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
        let updated = decode_session_followup_row(&updated_row)?;

        let _ = append_control_event_in_tx(
            &tx,
            &self.inner.pg_target,
            ControlEventAppend {
                entity_key: &format!("session:{session_id}"),
                event_type: "session.followup_updated",
                project_id: session.project_id,
                session_id: Some(session_id),
                task_id: None,
                correlation_id,
                payload: json!({
                    "session_id": session_id,
                    "followup_id": followup_id,
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
        Ok(updated)
    }

    /// Cancels a single queued or paused follow-up (`DELETE /v1/sessions/{id}/followups/{fid}`).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`], [`DennettError::Conflict`], or
    /// [`DennettError::DatabaseUnavailable`].
    pub async fn delete_followup(
        &self,
        session_id: SessionId,
        followup_id: FollowupId,
        correlation_id: &CorrelationId,
    ) -> Result<(), DennettError> {
        let _ = self.ensure_migrated().await?;
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let session = fetch_session(&tx, session_id, &self.inner.pg_target).await?;
        let lock_sql = format!(
            "SELECT {SESSION_FOLLOWUP_SELECT_COLS}
             FROM session_followups
             WHERE session_id = $1 AND followup_id = $2
             FOR UPDATE"
        );
        let Some(row) = tx
            .query_opt(&lock_sql, &[&session_id.as_uuid(), &followup_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?
        else {
            return Err(DennettError::NotFound {
                resource: String::from("session_followup"),
                id: followup_id.to_string(),
            });
        };
        let existing = decode_session_followup_row(&row)?;
        if !matches!(
            existing.status,
            FollowupStatus::Queued | FollowupStatus::Paused
        ) {
            return Err(DennettError::Conflict {
                resource: String::from("session_followup"),
                expected_revision: None,
                actual_revision: None,
                reason: format!("cannot delete follow-up in status `{}`", existing.status),
            });
        }

        tx.execute(
            "UPDATE session_followups
             SET status = 'cancelled', updated_at = now()
             WHERE session_id = $1 AND followup_id = $2",
            &[&session_id.as_uuid(), &followup_id.as_uuid()],
        )
        .await
        .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let _ = append_control_event_in_tx(
            &tx,
            &self.inner.pg_target,
            ControlEventAppend {
                entity_key: &format!("session:{session_id}"),
                event_type: "session.followup_cancelled",
                project_id: session.project_id,
                session_id: Some(session_id),
                task_id: None,
                correlation_id,
                payload: json!({
                    "session_id": session_id,
                    "followup_id": followup_id,
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
        Ok(())
    }

    /// Cancels all queued or paused follow-ups for `session_id` (`DELETE /v1/sessions/{id}/followups`).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`] or [`DennettError::DatabaseUnavailable`].
    pub async fn clear_followups(
        &self,
        session_id: SessionId,
        correlation_id: &CorrelationId,
    ) -> Result<(), DennettError> {
        let _ = self.ensure_migrated().await?;
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let session = fetch_session(&tx, session_id, &self.inner.pg_target).await?;
        let cleared = tx
            .execute(
                "UPDATE session_followups
                 SET status = 'cancelled', updated_at = now()
                 WHERE session_id = $1 AND status IN ('queued', 'paused')",
                &[&session_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        if cleared > 0 {
            let _ = append_control_event_in_tx(
                &tx,
                &self.inner.pg_target,
                ControlEventAppend {
                    entity_key: &format!("session:{session_id}"),
                    event_type: "session.followups_cleared",
                    project_id: session.project_id,
                    session_id: Some(session_id),
                    task_id: None,
                    correlation_id,
                    payload: json!({
                        "session_id": session_id,
                        "cleared_count": cleared,
                    }),
                },
            )
            .await?;
        }

        tx.commit()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
        Ok(())
    }

    /// Resumes paused follow-ups (`POST /v1/sessions/{id}/followups/resume`): transitions all
    /// `paused` items to `queued`, and if the session is idle atomically pops and executes the
    /// head `queued` item returning `dispatched: Some(SendMessageResponse)` + remaining active items.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`], [`DennettError::Conflict`], runtime errors from the
    /// dispatched turn, or [`DennettError::DatabaseUnavailable`].
    pub async fn resume_followups(
        &self,
        session_id: SessionId,
        correlation_id: &CorrelationId,
    ) -> Result<ResumeFollowupsResponse, DennettError> {
        let _ = self.ensure_migrated().await?;

        {
            let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
            let tx = conn
                .transaction()
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
            let session = fetch_session(&tx, session_id, &self.inner.pg_target).await?;
            if session.status != SessionStatus::Active {
                return Err(DennettError::Conflict {
                    resource: String::from("session"),
                    expected_revision: None,
                    actual_revision: None,
                    reason: format!(
                        "session is `{}` and cannot resume follow-ups",
                        session.status
                    ),
                });
            }
            if let Some(pid) = session.project_id {
                let proj = fetch_project(&tx, pid, &self.inner.pg_target).await?;
                if matches!(
                    proj.lifecycle_state,
                    ProjectLifecycleState::Archived | ProjectLifecycleState::DeletedRecord
                ) {
                    return Err(DennettError::Conflict {
                        resource: String::from("project"),
                        expected_revision: None,
                        actual_revision: None,
                        reason: format!(
                            "project `{}` is `{}` and cannot resume follow-ups",
                            proj.project_id, proj.lifecycle_state
                        ),
                    });
                }
            }

            let resumed = tx
                .execute(
                    "UPDATE session_followups
                     SET status = 'queued', updated_at = now()
                     WHERE session_id = $1 AND status = 'paused'",
                    &[&session_id.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

            if resumed > 0 {
                let _ = append_control_event_in_tx(
                    &tx,
                    &self.inner.pg_target,
                    ControlEventAppend {
                        entity_key: &format!("session:{session_id}"),
                        event_type: "session.followups_resumed",
                        project_id: session.project_id,
                        session_id: Some(session_id),
                        task_id: None,
                        correlation_id,
                        payload: json!({
                            "session_id": session_id,
                            "resumed_count": resumed,
                        }),
                    },
                )
                .await?;
            }

            tx.commit()
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
        }

        let dispatched = if self.is_session_active_or_steering(session_id).await {
            None
        } else if let Some((_fid, mut handle)) = self
            .try_dispatch_next_queued_followup_inner(
                session_id,
                Some(IDLE_FOLLOWUP_LOCK_WAIT_TIMEOUT),
                correlation_id,
            )
            .await?
        {
            match handle.completion_rx.wait_for(Option::is_some).await {
                Ok(guard) => match guard.as_ref() {
                    Some(Ok(resp)) => Some(resp.clone()),
                    Some(Err(err)) => return Err(err.clone()),
                    None => None,
                },
                Err(_) => None,
            }
        } else {
            None
        };

        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let followups =
            fetch_active_session_followups(&conn, session_id, &self.inner.pg_target).await?;
        Ok(ResumeFollowupsResponse {
            followups,
            dispatched,
        })
    }

    /// Live mid-run steer (`POST /v1/sessions/{id}/steer`): cancels any active run cleanly with
    /// `CANCEL_REASON_STEER` (without pausing queued follow-ups!), awaits clean release of the
    /// active-run slot with bounded timeout (ZERO `409 Conflict`!), and executes the steered turn.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::NotFound`],
    /// [`DennettError::Conflict`], [`DennettError::Timeout`], runtime errors, or
    /// [`DennettError::DatabaseUnavailable`].
    #[expect(
        clippy::too_many_lines,
        reason = "steer coordinates SteeringSessionGuard, clean active-run cancellation, bounded slot release wait, pre-run persistence, and detached execution"
    )]
    pub async fn steer_session(
        &self,
        session_id: SessionId,
        req: SteerSessionRequest,
        correlation_id: CorrelationId,
    ) -> Result<SendMessageResponse, DennettError> {
        req.validate()?;
        let _ = self.ensure_migrated().await?;

        let steering_guard =
            SteeringSessionGuard::acquire(Arc::clone(&self.inner.steering_sessions), session_id);

        // Fail fast if session does not exist or is not active before cancelling anything.
        {
            let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
            let session = fetch_session(&conn, session_id, &self.inner.pg_target).await?;
            if session.status != SessionStatus::Active {
                return Err(DennettError::Conflict {
                    resource: String::from("session"),
                    expected_revision: None,
                    actual_revision: None,
                    reason: format!(
                        "session is `{}` and does not accept steering messages",
                        session.status
                    ),
                });
            }
            if let Some(pid) = session.project_id {
                let proj = fetch_project(&conn, pid, &self.inner.pg_target).await?;
                if matches!(
                    proj.lifecycle_state,
                    ProjectLifecycleState::Archived | ProjectLifecycleState::DeletedRecord
                ) {
                    return Err(DennettError::Conflict {
                        resource: String::from("project"),
                        expected_revision: None,
                        actual_revision: None,
                        reason: format!(
                            "project `{}` is `{}` and does not accept steering messages",
                            proj.project_id, proj.lifecycle_state
                        ),
                    });
                }
            }
        }

        // If a run is active, mark it cancelled by Steer (never pausing queued follow-ups!)
        // and await clean release of its slot + session_lock.
        let active_wait = {
            let active = self.inner.active_runs.lock().await;
            active.get(&session_id).map(|h| {
                h.cancel_reason
                    .store(CANCEL_REASON_STEER, Ordering::Release);
                let _ = h.cancel_tx.send(true);
                (h.run_id, h.slot_released_rx.clone())
            })
        };
        if let Some((_prev_run_id, mut slot_rx)) = active_wait {
            let _ = self.runtime_router().cancel_turn_all(session_id).await;
            let _ = self
                .cancel_pending_permissions_for_scope(
                    Some(session_id),
                    None,
                    "cancelled by live steer on session",
                    &correlation_id,
                )
                .await;
            match time::timeout(
                STEER_SLOT_WAIT_TIMEOUT,
                slot_rx.wait_for(|released| *released),
            )
            .await
            {
                Ok(Ok(_) | Err(_)) => {}
                Err(_) => {
                    return Err(DennettError::Timeout {
                        operation: String::from("steer_wait_active_run_release"),
                    });
                }
            }
        }

        let session_lock = self.session_exec_lock(session_id).await;
        let Ok(session_guard) =
            time::timeout(STEER_SLOT_WAIT_TIMEOUT, session_lock.lock_owned()).await
        else {
            return Err(DennettError::Timeout {
                operation: String::from("steer_acquire_session_lock"),
            });
        };

        let client_message_id = req
            .client_message_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned);
        if let Some(ref cmid) = client_message_id
            && let Some(replayed) = self
                .try_replay_client_message(session_id, cmid, &req.content, &correlation_id)
                .await?
        {
            return Ok(replayed);
        }

        let turn_timeout = req
            .timeout_ms
            .map_or(DEFAULT_TURN_TIMEOUT, Duration::from_millis);
        let approval_timeout = match req.timeout_ms {
            Some(explicit_ms) => Duration::from_millis(explicit_ms)
                .saturating_sub(Duration::from_millis(200))
                .max(Duration::from_millis(250))
                .min(DEFAULT_APPROVAL_WAIT_TIMEOUT),
            None => DEFAULT_APPROVAL_WAIT_TIMEOUT,
        };

        // Reload fresh post-cancel session state now that we hold `session_guard`.
        let (session, agent, project, existing_binding, prior_history, policy_rules) = {
            let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
            let session = fetch_session(&conn, session_id, &self.inner.pg_target).await?;
            let agent = fetch_agent(&conn, session.agent_id, &self.inner.pg_target).await?;
            let project = match session.project_id {
                Some(pid) => Some(fetch_project(&conn, pid, &self.inner.pg_target).await?),
                None => None,
            };
            let binding = fetch_runtime_binding(&conn, session_id, &self.inner.pg_target).await?;
            let history = fetch_session_messages(&conn, session_id, &self.inner.pg_target).await?;
            let rules = fetch_policy_rules(&conn, &self.inner.pg_target).await?;
            (session, agent, project, binding, history, rules)
        };

        let resolved_runtime_res = self
            .resolve_runtime_for_turn(agent.current_config.preferred_connection_id, None)
            .await;
        let runtime_adapter_for_audit = resolved_runtime_res.as_ref().map_or_else(
            |_| String::from("unresolved"),
            |r| r.runtime.adapter_id().to_owned(),
        );

        let (completion_tx, mut completion_rx) =
            watch::channel::<Option<Result<SendMessageResponse, DennettError>>>(None);
        let run_id = RunId::new();
        let (cancel_tx, cancel_rx) = watch::channel(false);
        let cancel_reason = Arc::new(AtomicU8::new(CANCEL_REASON_NONE));
        let (slot_released_tx, slot_released_rx) = watch::channel(false);
        let slot_released = SlotReleasedNotifier::new(slot_released_tx);

        self.try_register_active_session_run(
            session_id,
            ActiveRunHandle {
                run_id,
                client_message_id: client_message_id.clone(),
                cancel_tx: cancel_tx.clone(),
                cancel_reason: Arc::clone(&cancel_reason),
                completion_rx: Some(completion_rx.clone()),
                slot_released_rx,
            },
        )
        .await?;
        let mut active_guard = ActiveRunGuard::new(
            self.clone(),
            session_id,
            run_id,
            cancel_tx,
            correlation_id.clone(),
        );

        // We now hold `session_guard` AND `active_runs` slot; drop `steering_guard` so queued
        // follow-ups auto-dispatch normally once this steered turn finishes with `Succeeded`!
        drop(steering_guard);

        let auto_title = if !prior_history.iter().any(|m| m.role == MessageRole::User)
            && is_default_session_title(&session.title)
        {
            derive_auto_session_title(&req.content)
        } else {
            None
        };
        let effective_session_title = auto_title.clone().unwrap_or_else(|| session.title.clone());

        let pre_run_res: Result<
            (MessageRecord, Vec<dennett_contracts::AgentMessageRecord>),
            DennettError,
        > = async {
            let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
            let tx = conn
                .transaction()
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

            let msg_id = MessageId::new();
            let stream_id = format!("session:{session_id}:messages");
            let msg_seq = next_stream_seq(&tx, &stream_id, &self.inner.pg_target).await?;

            if let Some(ref new_title) = auto_title {
                tx.execute(
                    "UPDATE sessions SET title = $2, updated_at = now() WHERE session_id = $1",
                    &[&session_id.as_uuid(), &new_title.as_str()],
                )
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
            }

            tx.execute(
                "INSERT INTO runs (
                    run_id, session_id, agent_id, agent_config_version_id,
                    user_message_id, status, correlation_id
                 ) VALUES ($1, $2, $3, $4, $5, 'running', $6)",
                &[
                    &run_id.as_uuid(),
                    &session_id.as_uuid(),
                    &agent.agent_id.as_uuid(),
                    &agent.current_config_version_id.as_uuid(),
                    &msg_id.as_uuid(),
                    &correlation_id.as_str(),
                ],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

            let msg_row = tx
                .query_one(
                    "INSERT INTO messages (
                        message_id, session_id, stream_seq, role, content, run_id,
                        client_message_id, correlation_id
                     ) VALUES ($1, $2, $3, 'user', $4, $5, $6, $7)
                     RETURNING TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint",
                    &[
                        &msg_id.as_uuid(),
                        &session_id.as_uuid(),
                        &msg_seq,
                        &req.content,
                        &run_id.as_uuid(),
                        &client_message_id,
                        &correlation_id.as_str(),
                    ],
                )
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

            let project_id_opt = project.as_ref().map(|p| p.project_id);
            let inbox_msgs = if let Some(pid) = project_id_opt {
                let agent_msg_sql = format!(
                    "SELECT {AGENT_MESSAGE_SELECT_COLS}
                     FROM agent_messages
                     WHERE recipient_agent_id = $1
                       AND project_id = $2
                       AND related_task_id IS NULL
                       AND delivered_in_run_id IS NULL
                     ORDER BY created_at ASC
                     FOR UPDATE"
                );
                let agent_msg_rows = tx
                    .query(&agent_msg_sql, &[&agent.agent_id.as_uuid(), &pid.as_uuid()])
                    .await
                    .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
                let mut claimed = Vec::with_capacity(agent_msg_rows.len());
                for row in &agent_msg_rows {
                    claimed.push(decode_agent_message_row(row)?);
                }
                if !claimed.is_empty() {
                    let ids: Vec<uuid::Uuid> = claimed
                        .iter()
                        .map(|m| m.agent_message_id.as_uuid())
                        .collect();
                    tx.execute(
                        "UPDATE agent_messages
                         SET delivered_in_run_id = $2, delivered_at = now()
                         WHERE agent_message_id = ANY($1)",
                        &[&ids, &run_id.as_uuid()],
                    )
                    .await
                    .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
                }
                claimed
            } else {
                Vec::new()
            };

            let _ = append_audit_event(
                &tx,
                &self.inner.pg_target,
                &correlation_id,
                Some(session_id),
                Some(run_id),
                "head",
                "run_steered",
                &format!(
                    "steered session {session_id} into run {run_id} adapter={runtime_adapter_for_audit}"
                ),
            )
            .await?;

            let _ = append_control_event_in_tx(
                &tx,
                &self.inner.pg_target,
                ControlEventAppend {
                    entity_key: &format!("session:{session_id}"),
                    event_type: "session.run_started",
                    project_id: project_id_opt,
                    session_id: Some(session_id),
                    task_id: None,
                    correlation_id: &correlation_id,
                    payload: json!({
                        "session_id": session_id,
                        "run_id": run_id,
                        "agent_id": agent.agent_id,
                        "title": effective_session_title,
                        "steered": true,
                    }),
                },
            )
            .await?;

            tx.commit()
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

            Ok((
                MessageRecord {
                    message_id: msg_id,
                    session_id,
                    role: MessageRole::User,
                    content: req.content.clone(),
                    run_id: Some(run_id),
                    client_message_id: client_message_id.clone(),
                    correlation_id: correlation_id.clone(),
                    created_at: TimestampMs::from_millis(msg_row.get(0)),
                },
                inbox_msgs,
            ))
        }
        .await;

        let (user_message, inbox_agent_messages) = match pre_run_res {
            Ok(pair) => pair,
            Err(err) => {
                active_guard.mark_completed();
                self.unregister_active_session_run(session_id, run_id).await;
                drop(session_guard);
                drop(slot_released);
                return Err(err);
            }
        };

        let resolved_runtime = match resolved_runtime_res {
            Ok(rt) => rt,
            Err(err) => {
                self.record_failed_run(session_id, run_id, &correlation_id, &err, &[])
                    .await;
                self.pause_queued_followups_for_session(session_id, &correlation_id)
                    .await;
                active_guard.mark_completed();
                self.unregister_active_session_run(session_id, run_id).await;
                let _ = completion_tx.send(Some(Err(err.clone())));
                drop(session_guard);
                drop(slot_released);
                return Err(err);
            }
        };

        let effective_prompt = if inbox_agent_messages.is_empty() {
            req.content
        } else {
            let mut combined = self.format_inter_agent_messages_block(&inbox_agent_messages);
            combined.push('\n');
            combined.push_str(&req.content);
            combined
        };

        self.spawn_detached_session_turn_worker(DetachedTurnSpawnArgs {
            session_id,
            session_kind: session.session_kind,
            run_id,
            agent,
            project,
            existing_binding,
            prior_history,
            policy_rules,
            user_message,
            effective_prompt,
            effective_session_title,
            runtime: resolved_runtime.runtime,
            connection_id: resolved_runtime.connection_id,
            turn_timeout,
            approval_timeout,
            cancel_rx,
            cancel_reason,
            correlation_id,
            completion_tx,
            session_guard,
            active_guard,
            slot_released,
            in_flight_cmid_guard: None,
        });

        match completion_rx.wait_for(Option::is_some).await {
            Ok(guard) => match guard.as_ref() {
                Some(res) => res.clone(),
                None => Err(DennettError::Internal {
                    detail: String::from("detached steered turn completed without result"),
                }),
            },
            Err(_) => Err(DennettError::Internal {
                detail: String::from("detached steered turn terminated unexpectedly"),
            }),
        }
    }

    async fn pause_queued_followups_for_session(
        &self,
        session_id: SessionId,
        correlation_id: &CorrelationId,
    ) {
        let Ok(mut conn) = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await else {
            return;
        };
        let Ok(tx) = conn.transaction().await else {
            return;
        };
        let paused = tx
            .execute(
                "UPDATE session_followups
                 SET status = 'paused', updated_at = now()
                 WHERE session_id = $1 AND status = 'queued'",
                &[&session_id.as_uuid()],
            )
            .await
            .unwrap_or(0);
        if paused > 0 {
            let project_id = fetch_session(&tx, session_id, &self.inner.pg_target)
                .await
                .ok()
                .and_then(|s| s.project_id);
            let _ = append_control_event_in_tx(
                &tx,
                &self.inner.pg_target,
                ControlEventAppend {
                    entity_key: &format!("session:{session_id}"),
                    event_type: "session.followups_paused",
                    project_id,
                    session_id: Some(session_id),
                    task_id: None,
                    correlation_id,
                    payload: json!({
                        "session_id": session_id,
                        "paused_count": paused,
                    }),
                },
            )
            .await;
        }
        let _ = tx.commit().await;
    }

    #[expect(
        clippy::too_many_lines,
        reason = "atomically claims next queued follow-up with SKIP LOCKED, persists user message and run, and spawns detached worker"
    )]
    async fn try_dispatch_next_queued_followup_inner(
        &self,
        session_id: SessionId,
        lock_wait: Option<Duration>,
        correlation_id: &CorrelationId,
    ) -> Result<Option<(FollowupId, SpawnedTurnHandle)>, DennettError> {
        if self.is_session_steering(session_id) {
            return Ok(None);
        }

        let session_lock = self.session_exec_lock(session_id).await;
        let session_guard = match lock_wait {
            Some(dur) => match time::timeout(dur, session_lock.lock_owned()).await {
                Ok(g) => g,
                Err(_) => return Ok(None),
            },
            None => match session_lock.try_lock_owned() {
                Ok(g) => g,
                Err(_) => return Ok(None),
            },
        };

        if self.is_session_active_or_steering(session_id).await {
            return Ok(None);
        }

        // Read metadata and check whether a queued follow-up is eligible BEFORE resolving runtime
        // so we never hold a PG connection across runtime/network probes.
        let (session, agent, project, existing_binding, prior_history, policy_rules) = {
            let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
            let has_paused: bool = conn
                .query_one(
                    "SELECT EXISTS(
                        SELECT 1 FROM session_followups
                        WHERE session_id = $1 AND status = 'paused'
                    )",
                    &[&session_id.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?
                .get(0);
            if has_paused {
                return Ok(None);
            }
            let has_queued: bool = conn
                .query_one(
                    "SELECT EXISTS(
                        SELECT 1 FROM session_followups
                        WHERE session_id = $1 AND status = 'queued'
                    )",
                    &[&session_id.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?
                .get(0);
            if !has_queued {
                return Ok(None);
            }

            let session = fetch_session(&conn, session_id, &self.inner.pg_target).await?;
            if session.status != SessionStatus::Active {
                return Ok(None);
            }
            let agent = fetch_agent(&conn, session.agent_id, &self.inner.pg_target).await?;
            let project = if let Some(pid) = session.project_id {
                let proj = fetch_project(&conn, pid, &self.inner.pg_target).await?;
                if matches!(
                    proj.lifecycle_state,
                    ProjectLifecycleState::Archived | ProjectLifecycleState::DeletedRecord
                ) {
                    return Ok(None);
                }
                Some(proj)
            } else {
                None
            };
            let binding = fetch_runtime_binding(&conn, session_id, &self.inner.pg_target).await?;
            let history = fetch_session_messages(&conn, session_id, &self.inner.pg_target).await?;
            let rules = fetch_policy_rules(&conn, &self.inner.pg_target).await?;
            (session, agent, project, binding, history, rules)
        };

        let resolved_runtime_res = self
            .resolve_runtime_for_turn(agent.current_config.preferred_connection_id, None)
            .await;
        let runtime_adapter_for_audit = resolved_runtime_res.as_ref().map_or_else(
            |_| String::from("unresolved"),
            |r| r.runtime.adapter_id().to_owned(),
        );

        let (completion_tx, completion_rx) =
            watch::channel::<Option<Result<SendMessageResponse, DennettError>>>(None);
        let run_id = RunId::new();
        let (cancel_tx, cancel_rx) = watch::channel(false);
        let cancel_reason = Arc::new(AtomicU8::new(CANCEL_REASON_NONE));
        let (slot_released_tx, slot_released_rx) = watch::channel(false);
        let slot_released = SlotReleasedNotifier::new(slot_released_tx);

        self.try_register_active_session_run(
            session_id,
            ActiveRunHandle {
                run_id,
                client_message_id: None,
                cancel_tx: cancel_tx.clone(),
                cancel_reason: Arc::clone(&cancel_reason),
                completion_rx: Some(completion_rx.clone()),
                slot_released_rx,
            },
        )
        .await?;
        let mut active_guard = ActiveRunGuard::new(
            self.clone(),
            session_id,
            run_id,
            cancel_tx,
            correlation_id.clone(),
        );

        let claim_res: Result<Option<ClaimedFollowupTurn>, DennettError> = async {
            let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
            let tx = conn
                .transaction()
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

            let has_paused: bool = tx
                .query_one(
                    "SELECT EXISTS(
                        SELECT 1 FROM session_followups
                        WHERE session_id = $1 AND status = 'paused'
                    )",
                    &[&session_id.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?
                .get(0);
            if has_paused {
                return Ok(None);
            }

            let claim_sql = format!(
                "SELECT {SESSION_FOLLOWUP_SELECT_COLS}
                 FROM session_followups
                 WHERE session_id = $1 AND status = 'queued'
                 ORDER BY position ASC
                 LIMIT 1
                 FOR UPDATE SKIP LOCKED"
            );
            let Some(followup_row) = tx
                .query_opt(&claim_sql, &[&session_id.as_uuid()])
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?
            else {
                return Ok(None);
            };
            let followup = decode_session_followup_row(&followup_row)?;

            tx.execute(
                "UPDATE session_followups
                 SET status = 'dispatched',
                     dispatched_run_id = $2,
                     updated_at = now()
                 WHERE followup_id = $1",
                &[&followup.followup_id.as_uuid(), &run_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

            let auto_title = if !prior_history.iter().any(|m| m.role == MessageRole::User)
                && is_default_session_title(&session.title)
            {
                derive_auto_session_title(&followup.content)
            } else {
                None
            };
            let effective_title = auto_title.clone().unwrap_or_else(|| session.title.clone());
            if let Some(ref new_title) = auto_title {
                tx.execute(
                    "UPDATE sessions SET title = $2, updated_at = now() WHERE session_id = $1",
                    &[&session_id.as_uuid(), &new_title.as_str()],
                )
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
            }

            let msg_id = MessageId::new();
            let stream_id = format!("session:{session_id}:messages");
            let msg_seq = next_stream_seq(&tx, &stream_id, &self.inner.pg_target).await?;

            tx.execute(
                "INSERT INTO runs (
                    run_id, session_id, agent_id, agent_config_version_id,
                    user_message_id, status, correlation_id
                 ) VALUES ($1, $2, $3, $4, $5, 'running', $6)",
                &[
                    &run_id.as_uuid(),
                    &session_id.as_uuid(),
                    &agent.agent_id.as_uuid(),
                    &agent.current_config_version_id.as_uuid(),
                    &msg_id.as_uuid(),
                    &correlation_id.as_str(),
                ],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

            let msg_row = tx
                .query_one(
                    "INSERT INTO messages (
                        message_id, session_id, stream_seq, role, content, run_id, correlation_id
                     ) VALUES ($1, $2, $3, 'user', $4, $5, $6)
                     RETURNING TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint",
                    &[
                        &msg_id.as_uuid(),
                        &session_id.as_uuid(),
                        &msg_seq,
                        &followup.content,
                        &run_id.as_uuid(),
                        &correlation_id.as_str(),
                    ],
                )
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

            let project_id_opt = project.as_ref().map(|p| p.project_id);
            let inbox_msgs = if let Some(pid) = project_id_opt {
                let agent_msg_sql = format!(
                    "SELECT {AGENT_MESSAGE_SELECT_COLS}
                     FROM agent_messages
                     WHERE recipient_agent_id = $1
                       AND project_id = $2
                       AND related_task_id IS NULL
                       AND delivered_in_run_id IS NULL
                     ORDER BY created_at ASC
                     FOR UPDATE"
                );
                let agent_msg_rows = tx
                    .query(&agent_msg_sql, &[&agent.agent_id.as_uuid(), &pid.as_uuid()])
                    .await
                    .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
                let mut claimed = Vec::with_capacity(agent_msg_rows.len());
                for row in &agent_msg_rows {
                    claimed.push(decode_agent_message_row(row)?);
                }
                if !claimed.is_empty() {
                    let ids: Vec<uuid::Uuid> = claimed
                        .iter()
                        .map(|m| m.agent_message_id.as_uuid())
                        .collect();
                    tx.execute(
                        "UPDATE agent_messages
                         SET delivered_in_run_id = $2, delivered_at = now()
                         WHERE agent_message_id = ANY($1)",
                        &[&ids, &run_id.as_uuid()],
                    )
                    .await
                    .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
                }
                claimed
            } else {
                Vec::new()
            };

            let _ = append_audit_event(
                &tx,
                &self.inner.pg_target,
                correlation_id,
                Some(session_id),
                Some(run_id),
                "head",
                "followup_dispatched",
                &format!(
                    "dispatched follow-up {} into run {run_id} adapter={runtime_adapter_for_audit}",
                    followup.followup_id
                ),
            )
            .await?;

            let pending_events = vec![
                PendingControlEvent {
                    entity_key: format!("session:{session_id}"),
                    event_type: "session.followup_dispatched",
                    project_id: project_id_opt,
                    session_id: Some(session_id),
                    task_id: None,
                    payload: json!({
                        "session_id": session_id,
                        "followup_id": followup.followup_id,
                        "run_id": run_id,
                    }),
                },
                PendingControlEvent {
                    entity_key: format!("session:{session_id}"),
                    event_type: "session.run_started",
                    project_id: project_id_opt,
                    session_id: Some(session_id),
                    task_id: None,
                    payload: json!({
                        "session_id": session_id,
                        "run_id": run_id,
                        "agent_id": agent.agent_id,
                        "title": effective_title,
                        "followup_id": followup.followup_id,
                    }),
                },
            ];
            let _ = append_control_events_batch_in_tx(
                &tx,
                &self.inner.pg_target,
                correlation_id,
                &pending_events,
            )
            .await?;

            tx.commit()
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

            let user_message = MessageRecord {
                message_id: msg_id,
                session_id,
                role: MessageRole::User,
                content: followup.content.clone(),
                run_id: Some(run_id),
                client_message_id: None,
                correlation_id: correlation_id.clone(),
                created_at: TimestampMs::from_millis(msg_row.get(0)),
            };
            Ok(Some(ClaimedFollowupTurn {
                followup,
                user_message,
                inbox_msgs,
                effective_title,
            }))
        }
        .await;

        let ClaimedFollowupTurn {
            followup,
            user_message,
            inbox_msgs: inbox_agent_messages,
            effective_title: effective_session_title,
        } = match claim_res {
            Ok(Some(claimed)) => claimed,
            Ok(None) => {
                active_guard.mark_completed();
                self.unregister_active_session_run(session_id, run_id).await;
                drop(session_guard);
                drop(slot_released);
                return Ok(None);
            }
            Err(err) => {
                active_guard.mark_completed();
                self.unregister_active_session_run(session_id, run_id).await;
                drop(session_guard);
                drop(slot_released);
                return Err(err);
            }
        };

        let resolved_runtime = match resolved_runtime_res {
            Ok(rt) => rt,
            Err(err) => {
                self.record_failed_run(session_id, run_id, correlation_id, &err, &[])
                    .await;
                self.pause_queued_followups_for_session(session_id, correlation_id)
                    .await;
                active_guard.mark_completed();
                self.unregister_active_session_run(session_id, run_id).await;
                let _ = completion_tx.send(Some(Err(err.clone())));
                drop(session_guard);
                drop(slot_released);
                return Err(err);
            }
        };

        let effective_prompt = if inbox_agent_messages.is_empty() {
            followup.content
        } else {
            let mut combined = self.format_inter_agent_messages_block(&inbox_agent_messages);
            combined.push('\n');
            combined.push_str(&followup.content);
            combined
        };

        self.spawn_detached_session_turn_worker(DetachedTurnSpawnArgs {
            session_id,
            session_kind: session.session_kind,
            run_id,
            agent,
            project,
            existing_binding,
            prior_history,
            policy_rules,
            user_message,
            effective_prompt,
            effective_session_title,
            runtime: resolved_runtime.runtime,
            connection_id: resolved_runtime.connection_id,
            turn_timeout: DEFAULT_TURN_TIMEOUT,
            approval_timeout: DEFAULT_APPROVAL_WAIT_TIMEOUT,
            cancel_rx,
            cancel_reason,
            correlation_id: correlation_id.clone(),
            completion_tx,
            session_guard,
            active_guard,
            slot_released,
            in_flight_cmid_guard: None,
        });

        Ok(Some((
            followup.followup_id,
            SpawnedTurnHandle { completion_rx },
        )))
    }

    #[expect(
        clippy::too_many_lines,
        reason = "idempotent client_message_id replay reconstructs complete SendMessageResponse from PG rows"
    )]
    async fn try_replay_client_message(
        &self,
        session_id: SessionId,
        client_message_id: &str,
        expected_content: &str,
        _correlation_id: &CorrelationId,
    ) -> Result<Option<SendMessageResponse>, DennettError> {
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let existing_msg_row = conn
            .query_opt(
                "SELECT message_id, session_id, role, content, run_id, client_message_id,
                        correlation_id, TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint
                 FROM messages
                 WHERE session_id = $1 AND client_message_id = $2",
                &[&session_id.as_uuid(), &client_message_id],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let Some(msg_row) = existing_msg_row else {
            return Ok(None);
        };
        let user_message = decode_message_row(&msg_row)?;
        if user_message.content != expected_content {
            return Err(DennettError::IdempotencyMismatch {
                idempotency_key: client_message_id.to_owned(),
                reason: String::from(
                    "reused client_message_id on this session with different message content",
                ),
            });
        }

        let Some(run_id) = user_message.run_id else {
            return Err(DennettError::Conflict {
                resource: String::from("message"),
                expected_revision: None,
                actual_revision: None,
                reason: String::from("existing deduplicated user message has no associated run"),
            });
        };

        let run_sql = format!("SELECT {RUN_SELECT_COLS} FROM runs WHERE run_id = $1");
        let run_row = conn
            .query_one(&run_sql, &[&run_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
        let run = decode_run_row(&run_row)?;
        if run.status == RunStatus::Running {
            drop(conn);
            let active_rx = {
                let active = self.inner.active_runs.lock().await;
                active
                    .get(&session_id)
                    .filter(|h| h.run_id == run_id)
                    .and_then(|h| h.completion_rx.clone())
            };
            if let Some(mut rx) = active_rx
                && let Ok(guard) = rx.wait_for(Option::is_some).await
                && let Some(res) = guard.as_ref()
            {
                return match res {
                    Ok(resp) => {
                        let mut dedup = resp.clone();
                        dedup.deduplicated = true;
                        Ok(Some(dedup))
                    }
                    Err(err) => Err(err.clone()),
                };
            }
            return Err(DennettError::Conflict {
                resource: String::from("session_run"),
                expected_revision: None,
                actual_revision: None,
                reason: String::from(
                    "original message with this client_message_id is still running",
                ),
            });
        }

        let assistant_message = if let Some(amid) = run.assistant_message_id {
            let am_row = conn
                .query_opt(
                    "SELECT message_id, session_id, role, content, run_id, client_message_id,
                            correlation_id, TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint
                     FROM messages
                     WHERE message_id = $1",
                    &[&amid.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
            am_row.as_ref().map(decode_message_row).transpose()?
        } else {
            None
        };

        let session = fetch_session(&conn, session_id, &self.inner.pg_target).await?;
        let Some(runtime_binding) = session.runtime_binding else {
            return Err(DennettError::Conflict {
                resource: String::from("session_run"),
                expected_revision: None,
                actual_revision: None,
                reason: format!(
                    "previous attempt for client_message_id `{client_message_id}` failed before binding runtime"
                ),
            });
        };
        let artifacts = fetch_session_artifacts(&conn, session_id, &self.inner.pg_target).await?;
        let all_decisions =
            fetch_session_permission_decisions(&conn, session_id, &self.inner.pg_target).await?;
        let run_decisions = all_decisions
            .into_iter()
            .filter(|d| d.run_id == Some(run_id))
            .collect();

        Ok(Some(SendMessageResponse {
            user_message,
            assistant_message,
            run,
            runtime_binding,
            artifacts,
            permission_decisions: run_decisions,
            memory_status: self.cached_memory_status(),
            session_title: session.title,
            deduplicated: true,
        }))
    }

    /// Triggers immediate live cancellation (`Stop`) on any active run in `session_id`, withdraws
    /// pending interactive approvals for the session, marks any in-flight `dispatching` external
    /// effects as `unknown`, and reports honest external-outcome uncertainty (`external_outcome_unknown`).
    ///
    /// CRITICAL V02 INVARIANT: NEVER blocks on or calls `dennett-memoryd`!
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`] if `session_id` does not exist or
    /// [`DennettError::DatabaseUnavailable`] if control PG fails.
    #[expect(
        clippy::too_many_lines,
        reason = "signals live cancellation across all runtime profiles, withdraws pending approvals, transitions dispatching effects to unknown, and reports honest external outcome uncertainty without calling memoryd"
    )]
    pub async fn cancel_session_run(
        &self,
        session_id: SessionId,
        correlation_id: &CorrelationId,
    ) -> Result<CancelRunResponse, DennettError> {
        let _ = self.ensure_migrated().await?;

        // Signal cancellation immediately across ALL runtime profiles BEFORE touching PostgreSQL!
        let active_run_id = {
            let active = self.inner.active_runs.lock().await;
            active.get(&session_id).map(|h| {
                h.cancel_reason
                    .store(CANCEL_REASON_EXPLICIT_STOP, Ordering::Release);
                let _ = h.cancel_tx.send(true);
                h.run_id
            })
        };
        let runtime_cancelled = self.runtime_router().cancel_turn_all(session_id).await;

        let withdrawn_perms = self
            .cancel_pending_permissions_for_scope(
                Some(session_id),
                None,
                "cancelled by Stop on session",
                correlation_id,
            )
            .await
            .unwrap_or(0);

        let cancelled = active_run_id.is_some() || runtime_cancelled || withdrawn_perms > 0;

        // Verify session exists, mark any `dispatching` external effects as `unknown`, and record
        // cancel audit in control PG (never calls memoryd!).
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let session = fetch_session(&tx, session_id, &self.inner.pg_target).await?;

        let dispatching_sql = format!(
            "UPDATE effect_claims
             SET status = 'unknown',
                 error_summary = 'external effect outcome unknown after Stop/cancellation; manual reconciliation required',
                 updated_at = now()
             WHERE session_id = $1 AND status = 'dispatching'
             RETURNING {EFFECT_CLAIM_SELECT_COLS}"
        );
        let dispatching_rows = tx
            .query(&dispatching_sql, &[&session_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
        let mut pending_events = Vec::with_capacity(dispatching_rows.len() + 1);
        for row in &dispatching_rows {
            let eff = decode_effect_claim_row(row)?;
            self.mark_unknown_effect_side_effects_in_tx(&tx, &eff, &mut pending_events)
                .await?;
        }

        let effects_sql = format!(
            "SELECT {EFFECT_CLAIM_SELECT_COLS}
             FROM effect_claims
             WHERE session_id = $1
             ORDER BY created_at ASC"
        );
        let effect_rows = tx
            .query(&effects_sql, &[&session_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
        let mut committed_effects: Vec<EffectClaimRecord> = Vec::new();
        let mut unknown_effects: Vec<EffectClaimRecord> = Vec::new();
        for row in &effect_rows {
            let eff = decode_effect_claim_row(row)?;
            match eff.status {
                ExternalEffectStatus::Confirmed => committed_effects.push(eff),
                ExternalEffectStatus::Unknown => unknown_effects.push(eff),
                _ => {}
            }
        }
        let external_outcome_unknown = !unknown_effects.is_empty();

        // D2-ARCH-03: Preserve `unknown_external` status on the run when external outcome is uncertain.
        if external_outcome_unknown && let Some(rid) = active_run_id {
            tx.execute(
                "UPDATE runs SET status = 'unknown_external' WHERE run_id = $1",
                &[&rid.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
        }

        // Safe pause: explicit Stop ALWAYS pauses any queued follow-ups on this session.
        tx.execute(
            "UPDATE session_followups
             SET status = 'paused', updated_at = now()
             WHERE session_id = $1 AND status = 'queued'",
            &[&session_id.as_uuid()],
        )
        .await
        .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let _ = append_audit_event(
            &tx,
            &self.inner.pg_target,
            correlation_id,
            Some(session_id),
            active_run_id,
            "head",
            "run_cancel_requested",
            &format!(
                "cancel requested for session {session_id} (active_run={active_run_id:?}, signalled={cancelled}, unknown_effects={})",
                unknown_effects.len()
            ),
        )
        .await?;

        let status = if external_outcome_unknown {
            Some(RunStatus::UnknownExternal)
        } else if cancelled {
            Some(RunStatus::Cancelled)
        } else {
            fetch_session_runs(&tx, session_id, &self.inner.pg_target)
                .await?
                .last()
                .map(|r| r.status)
        };

        pending_events.push(PendingControlEvent {
            entity_key: format!("session:{session_id}"),
            event_type: "session.run_cancel_requested",
            project_id: session.project_id,
            session_id: Some(session_id),
            task_id: None,
            payload: json!({
                "session_id": session_id,
                "run_id": active_run_id,
                "cancelled": cancelled,
                "external_outcome_unknown": external_outcome_unknown,
            }),
        });
        let _ = append_control_events_batch_in_tx(
            &tx,
            &self.inner.pg_target,
            correlation_id,
            &pending_events,
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        Ok(CancelRunResponse {
            session_id,
            run_id: active_run_id,
            cancelled,
            status,
            // Non-blocking V02 report: returns the latest observed memoryd status from an
            // atomic cache without performing any synchronous network probe during Stop!
            memory_status: self.cached_memory_status(),
            external_outcome_unknown,
            committed_effects,
            unknown_effects,
        })
    }

    /// Durable memory commit path (`V02` / `D2-OVER-01` / `D2-SEC-01`):
    /// - When `req.session_id` is supplied, validates active project lifecycle, agent
    ///   `allowed_memory_scopes`, and external `PolicyEngine` rules BEFORE writing outbox.
    /// - Enqueues into unified `ordered_outbox` (`destination = 'memoryd'`, `ordering_key = scope`),
    ///   strictly preserving FIFO ordering per scope and deterministic `(scope, idempotency_key)` dedup.
    /// - Attempts immediate delivery to standalone `dennett-memoryd`; if unreachable, stays durably
    ///   `pending` for automatic background drain in `HeadScheduler` or manual `/v1/memory/outbox/flush`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::AccessDenied`],
    /// [`DennettError::ForbiddenScope`], [`DennettError::IdempotencyMismatch`],
    /// [`DennettError::DatabaseUnavailable`] (control PG), or permanent non-transient memoryd rejections.
    pub async fn commit_memory_note(
        &self,
        req: CommitMemoryNoteRequest,
        correlation_id: &CorrelationId,
    ) -> Result<CommitMemoryNoteResponse, DennettError> {
        let _ = self.ensure_migrated().await?;
        let idempotency_key = req.idempotency_key.unwrap_or_else(IdempotencyKey::generate);

        // D2-SEC-01: When bound to a session, route through external `PolicyEngine` + agent scopes!
        if let Some(session_id) = req.session_id {
            self.authorize_session_memory_operation(
                session_id,
                "memory_write_note",
                &req.scope,
                correlation_id,
            )
            .await?;
        }

        let create_note_req = CreateNoteRequest {
            scope: req.scope.clone(),
            title: req.title,
            body: req.body,
            tags: req.tags,
            observed_at: None,
            evidence_links: Vec::new(),
            author_label: Some(String::from("dennett-head")),
            admission_status: None,
            salience: None,
            topic_key: None,
            idempotency_key: idempotency_key.clone(),
        };
        create_note_req.validate()?;

        // 1. Race-safe durable insert into unified `ordered_outbox` (`destination = 'memoryd'`,
        //    `ordering_key = scope`). Verifies payload equality on duplicate key -> 409 on conflict.
        let ordered_item = self
            .insert_or_verify_outbox_note(
                req.session_id,
                req.run_id,
                &create_note_req,
                correlation_id,
            )
            .await?;

        if ordered_item.status == OrderedOutboxStatus::Delivered {
            let outbox_item = ordered_item_to_memory_outbox_item(&ordered_item)?;
            return Ok(CommitMemoryNoteResponse {
                outbox_item,
                delivered_immediately: true,
                memory_status: self.cached_memory_status(),
            });
        }

        // 2. Attempt immediate ordered delivery for `memoryd` (preserving FIFO per scope).
        let _ = self.flush_memory_outbox(correlation_id).await?;
        let refreshed = self.fetch_memory_outbox_item(ordered_item.item_id).await?;
        let delivered_immediately = refreshed.status == MemoryOutboxStatus::Delivered;

        Ok(CommitMemoryNoteResponse {
            delivered_immediately,
            outbox_item: refreshed,
            memory_status: self.cached_memory_status(),
        })
    }

    async fn authorize_session_memory_operation(
        &self,
        session_id: SessionId,
        tool_name: &str,
        scope: &MemoryScope,
        _correlation_id: &CorrelationId,
    ) -> Result<(), DennettError> {
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let session = fetch_session(&conn, session_id, &self.inner.pg_target).await?;
        let agent = fetch_agent(&conn, session.agent_id, &self.inner.pg_target).await?;
        let project = match session.project_id {
            Some(pid) => Some(fetch_project(&conn, pid, &self.inner.pg_target).await?),
            None => None,
        };
        drop(conn);

        let project_id = project.as_ref().map(|p| p.project_id);
        let live = self.load_live_policy_inputs(project_id, None).await?;
        let workspace_root = match project {
            Some(ref proj) => proj.workspace_path.as_deref().map_or_else(
                || self.fallback_workspace_root(proj.project_id),
                PathBuf::from,
            ),
            None => self.standalone_session_scratch_dir(session_id),
        };
        let workspace_write_policy = if project_id.is_some() {
            agent.current_config.workspace_write_policy.clone()
        } else {
            WorkspaceWritePolicy::DenyAll
        };
        let ctx = PolicyContext {
            project_id,
            agent_id: agent.agent_id,
            session_id,
            task_id: None,
            project_lifecycle_state: live.project_lifecycle_state,
            workspace_bound: live.workspace_bound,
            workspace_root,
            workspace_write_policy,
            parent_write_ceiling: live.parent_write_ceiling,
            allowed_memory_scopes: agent.current_config.allowed_memory_scopes.clone(),
            registered_mcp_servers: BTreeSet::new(),
            task_budget_exhausted: live.task_budget_exhausted,
            rules: live.rules,
        };
        let engine = PolicyEngine::new(ctx);
        let eval_req = PolicyEvaluationRequest {
            session_id,
            run_id: RunId::new(),
            tool_call_id: format!("head-memory-{}", OutboxItemId::new()),
            tool_name: tool_name.to_owned(),
            tool_kind: String::from("mcp"),
            title: format!("{tool_name} ({scope})"),
            target: Some(scope.as_str().to_owned()),
            locations: Vec::new(),
            raw_input: json!({ "scope": scope.as_str() }),
        };
        let decision = engine.evaluate_with_grants(&eval_req, &live.grants, TimestampMs::now());
        match decision {
            PolicyEvaluationDecision::Immediate(out)
                if out.effect == dennett_contracts::PolicyEffect::Allow =>
            {
                Ok(())
            }
            PolicyEvaluationDecision::AllowedByGrant { .. } => Ok(()),
            PolicyEvaluationDecision::Immediate(out) => Err(DennettError::AccessDenied {
                scope: out.scope,
                reason: out.reason,
            }),
            PolicyEvaluationDecision::NeedsInteractiveApproval { scope, reason, .. } => {
                Err(DennettError::AccessDenied { scope, reason })
            }
        }
    }

    async fn insert_or_verify_outbox_note(
        &self,
        session_id: Option<SessionId>,
        run_id: Option<RunId>,
        create_note_req: &CreateNoteRequest,
        correlation_id: &CorrelationId,
    ) -> Result<OrderedOutboxItem, DennettError> {
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let existing_sql = format!(
            "SELECT {ORDERED_OUTBOX_SELECT_COLS}
             FROM ordered_outbox
             WHERE destination = $1 AND ordering_key = $2 AND idempotency_key = $3"
        );
        let incoming_fp = note_request_fingerprint(create_note_req)?;
        if let Some(existing_row) = tx
            .query_opt(
                &existing_sql,
                &[
                    &MEMORY_OUTBOX_DESTINATION,
                    &create_note_req.scope.as_str(),
                    &create_note_req.idempotency_key.as_str(),
                ],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?
        {
            let existing = decode_ordered_outbox_row(&existing_row)?;
            let existing_env: MemoryOutboxEnvelope = serde_json::from_str(&existing.payload_json)
                .map_err(|err| DennettError::Internal {
                detail: format!("corrupt ordered_outbox memory envelope JSON: {err}"),
            })?;
            verify_existing_outbox_envelope(&existing_env, create_note_req, &incoming_fp)?;
            tx.commit()
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
            return Ok(existing);
        }

        let envelope = MemoryOutboxEnvelope {
            scope: create_note_req.scope.clone(),
            idempotency_key: create_note_req.idempotency_key.clone(),
            operation: String::from("create_note"),
            session_id,
            run_id,
            note_request: create_note_req.clone(),
            payload_sha256: Some(incoming_fp),
            forgotten: false,
            delivered_event_id: None,
            delivered_note_id: None,
        };
        let payload_val =
            serde_json::to_value(&envelope).map_err(|err| DennettError::Internal {
                detail: format!("failed serializing memory outbox envelope: {err}"),
            })?;

        let cursor_seq: i64 = tx
            .query_opt(
                "SELECT GREATEST(1, committed_seq)::bigint
                 FROM control_stream_counters
                 WHERE stream_id = 'control:journal'",
                &[],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?
            .map_or(1, |row| row.get(0));

        let item = enqueue_ordered_outbox_in_tx(
            &tx,
            &self.inner.pg_target,
            MEMORY_OUTBOX_DESTINATION,
            create_note_req.scope.as_str(),
            cursor_seq,
            &create_note_req.idempotency_key,
            &payload_val,
            correlation_id,
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
        Ok(item)
    }

    async fn fetch_memory_outbox_item(
        &self,
        item_id: dennett_contracts::OrderedOutboxItemId,
    ) -> Result<MemoryOutboxItem, DennettError> {
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let sql =
            format!("SELECT {ORDERED_OUTBOX_SELECT_COLS} FROM ordered_outbox WHERE item_id = $1");
        let row = conn
            .query_one(&sql, &[&item_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
        let ordered = decode_ordered_outbox_row(&row)?;
        ordered_item_to_memory_outbox_item(&ordered)
    }

    /// Lightweight check used by `HeadScheduler` before invoking automatic background memory outbox
    /// draining (`D2-OVER-01`).
    pub(crate) async fn has_pending_memory_outbox(&self) -> bool {
        let Ok(conn) = self.get_conn(DEFAULT_PROBE_TIMEOUT).await else {
            return false;
        };
        conn.query_opt(
            "SELECT 1
             FROM ordered_outbox
             WHERE destination = 'memoryd'
               AND status IN ('pending', 'leased')
               AND available_at <= now()
             LIMIT 1",
            &[],
        )
        .await
        .ok()
        .flatten()
        .is_some()
    }

    /// Reads the PostgreSQL clock and returns `max(host_now, pg_now + 1ms)` so freshly enqueued or
    /// migrated `ordered_outbox` rows stamped with PG `now()` are immediately claimable even when
    /// Docker/WSL2 PostgreSQL clocks sit slightly ahead of the host clock.
    async fn effective_outbox_now(&self) -> Result<TimestampMs, DennettError> {
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let pg_now_ms: i64 = conn
            .query_one(
                "SELECT TRUNC(EXTRACT(EPOCH FROM now()) * 1000)::bigint",
                &[],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?
            .get(0);
        let host_now_ms = TimestampMs::now().as_millis();
        Ok(TimestampMs::from_millis(
            host_now_ms.max(pg_now_ms.saturating_add(1)),
        ))
    }

    /// Scrubs plaintext `note_request` fields to `[forgotten]` in any `ordered_outbox` rows
    /// (`destination = 'memoryd'`) whose `delivered_note_id` matches one of `note_ids`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::DatabaseUnavailable`] on control PG failure.
    pub async fn scrub_forgotten_memory_outbox(
        &self,
        note_ids: &[uuid::Uuid],
    ) -> Result<u64, DennettError> {
        if note_ids.is_empty() {
            return Ok(0);
        }
        let _ = self.ensure_migrated().await?;
        let id_strings: Vec<String> = note_ids.iter().map(ToString::to_string).collect();
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let rows = tx
            .query(
                "SELECT item_id, payload_json::text
                 FROM ordered_outbox
                 WHERE destination = $1
                   AND (payload_json->>'delivered_note_id') = ANY($2::text[])
                 FOR UPDATE",
                &[&MEMORY_OUTBOX_DESTINATION, &id_strings],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let mut scrubbed: u64 = 0;
        for row in rows {
            let item_id: uuid::Uuid = row.get(0);
            let raw_payload: String = row.get(1);
            let Ok(mut env) = serde_json::from_str::<MemoryOutboxEnvelope>(&raw_payload) else {
                continue;
            };
            scrub_note_request(&mut env.note_request, OUTBOX_SCRUBBED_FORGOTTEN);
            env.payload_sha256 = None;
            env.forgotten = true;
            let updated_payload =
                serde_json::to_value(&env).map_err(|err| DennettError::Internal {
                    detail: format!("failed serializing forgotten outbox envelope: {err}"),
                })?;
            tx.execute(
                "UPDATE ordered_outbox
                 SET payload_json = $2,
                     updated_at = now()
                 WHERE item_id = $1",
                &[&item_id, &updated_payload],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
            scrubbed = scrubbed.saturating_add(1);
        }

        tx.commit()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
        Ok(scrubbed)
    }

    /// Streams monotonic forget tombstones from standalone `dennett-memoryd` starting strictly
    /// after `memory_tombstone_sync_state.last_tombstone_seq`, scrubs matching `ordered_outbox`
    /// rows to `[forgotten]`, and persists the advanced watermark.
    ///
    /// Best-effort on `memoryd` outage (`DatabaseUnavailable`/`Timeout`): marks degraded status
    /// and returns `Ok(scrubbed)` without failing or blocking `Stop`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError`] on control PG failures or unexpected non-transient `memoryd` errors.
    pub async fn sync_memory_tombstones(
        &self,
        correlation_id: &CorrelationId,
    ) -> Result<u64, DennettError> {
        const PAGE_LIMIT: u32 = 200;
        const MAX_PAGES_PER_SYNC: usize = 50;

        let _ = self.ensure_migrated().await?;
        let scopes = self.collect_all_known_memory_scopes().await?;
        let mut caps = BTreeSet::new();
        caps.insert(MemoryCapability::Read);
        let Some(client) = self.memory_client_with_capabilities(scopes, caps)? else {
            return Ok(0);
        };

        let mut cursor: i64 = {
            let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
            conn.query_opt(
                "SELECT last_tombstone_seq
                 FROM memory_tombstone_sync_state
                 WHERE singleton = TRUE",
                &[],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?
            .map_or(0, |row| row.get(0))
        };

        let mut total_scrubbed: u64 = 0;
        for _ in 0..MAX_PAGES_PER_SYNC {
            let page = match client
                .list_tombstones_since(None, Some(cursor), Some(PAGE_LIMIT), Some(correlation_id))
                .await
            {
                Ok(resp) => {
                    self.record_memory_status(MemoryStatus::Healthy);
                    resp
                }
                Err(DennettError::DatabaseUnavailable { .. } | DennettError::Timeout { .. }) => {
                    self.record_memory_status(MemoryStatus::DegradedUnreachable);
                    break;
                }
                Err(other) => return Err(other),
            };

            let note_ids: Vec<uuid::Uuid> = page
                .tombstones
                .iter()
                .filter(|t| t.target_type == ForgetTargetType::Note)
                .map(|t| t.target_id)
                .collect();
            if !note_ids.is_empty() {
                total_scrubbed = total_scrubbed
                    .saturating_add(self.scrub_forgotten_memory_outbox(&note_ids).await?);
            }

            let next_cursor = page.next_after_seq.unwrap_or(cursor).max(cursor);
            if next_cursor > cursor {
                let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
                conn.execute(
                    "INSERT INTO memory_tombstone_sync_state (singleton, last_tombstone_seq, updated_at)
                     VALUES (TRUE, $1, now())
                     ON CONFLICT (singleton) DO UPDATE
                     SET last_tombstone_seq = GREATEST(
                             memory_tombstone_sync_state.last_tombstone_seq,
                             EXCLUDED.last_tombstone_seq
                         ),
                         updated_at = now()",
                    &[&next_cursor],
                )
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
                cursor = next_cursor;
            }

            if page.tombstones.len() < usize::try_from(PAGE_LIMIT).unwrap_or(200) {
                break;
            }
        }

        Ok(total_scrubbed)
    }

    /// Flushes all pending `ordered_outbox (destination = 'memoryd')` items to standalone
    /// `dennett-memoryd` in strict per-scope FIFO order (`D2-OVER-01`).
    ///
    /// Safe to invoke repeatedly or concurrently: `ordered_outbox` leases each scope's head with
    /// `FOR UPDATE SKIP LOCKED` and `memoryd` deduplicates on `(scope, idempotency_key)`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::DatabaseUnavailable`] only if `dennett_control` PG fails.
    pub async fn flush_memory_outbox(
        &self,
        correlation_id: &CorrelationId,
    ) -> Result<FlushOutboxResponse, DennettError> {
        let _ = self.ensure_migrated().await?;

        // Best-effort monotonic tombstone sync before draining outbox so forgotten notes are
        // scrubbed promptly without blocking flush or `Stop` when `memoryd` is degraded.
        let _ = self.sync_memory_tombstones(correlation_id).await;

        let worker_id = format!("head-mem-flush-{}", std::process::id());
        let mut attempted = 0_usize;
        let mut delivered = 0_usize;
        let mut pending = 0_usize;
        let mut failed = 0_usize;
        let mut last_mem_status = self.probe_memory_status(correlation_id).await.0;
        let mut touched: Vec<MemoryOutboxItem> = Vec::new();
        let mut visited: HashSet<dennett_contracts::OrderedOutboxItemId> = HashSet::new();

        loop {
            let effective_now = self.effective_outbox_now().await?;
            let batch = self
                .claim_ordered_outbox_batch(
                    MEMORY_OUTBOX_DESTINATION,
                    &worker_id,
                    32,
                    30_000,
                    effective_now,
                )
                .await?;
            if batch.is_empty() {
                break;
            }

            let mut progressed_any = false;
            for leased in batch {
                if !visited.insert(leased.item_id) {
                    // Already attempted and nacked back to `pending` during this flush call
                    // (e.g. `memoryd` is still unreachable) — release lease without looping.
                    let _ = self
                        .nack_ordered_outbox_item(
                            leased.item_id,
                            leased.lease_generation,
                            leased
                                .last_error
                                .as_deref()
                                .unwrap_or("memoryd unreachable"),
                            MEMORY_OUTBOX_MAX_TRANSIENT_ATTEMPTS,
                            0,
                            effective_now,
                        )
                        .await;
                    continue;
                }

                progressed_any = true;
                attempted += 1;
                let (updated, mem_status) = self
                    .deliver_leased_memory_outbox_item(leased, effective_now, correlation_id)
                    .await?;
                last_mem_status = mem_status;
                match updated.status {
                    MemoryOutboxStatus::Delivered => delivered += 1,
                    MemoryOutboxStatus::Pending => pending += 1,
                    MemoryOutboxStatus::Failed => failed += 1,
                }
                touched.push(updated);
            }

            if !progressed_any {
                break;
            }
        }

        Ok(FlushOutboxResponse {
            attempted,
            delivered,
            pending,
            failed,
            memory_status: last_mem_status,
            items: touched,
        })
    }

    /// Lists all durable memory outbox items (`ordered_outbox WHERE destination = 'memoryd'`)
    /// ordered newest first.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::DatabaseUnavailable`] on control PG failure.
    pub async fn list_memory_outbox(
        &self,
        correlation_id: &CorrelationId,
    ) -> Result<ListOutboxResponse, DennettError> {
        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let sql = format!(
            "SELECT {ORDERED_OUTBOX_SELECT_COLS}
             FROM ordered_outbox
             WHERE destination = $1
             ORDER BY created_at DESC"
        );
        let rows = conn
            .query(&sql, &[&MEMORY_OUTBOX_DESTINATION])
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
        let mut items = Vec::with_capacity(rows.len());
        for row in &rows {
            let ordered = decode_ordered_outbox_row(row)?;
            items.push(ordered_item_to_memory_outbox_item(&ordered)?);
        }
        let (memory_status, _) = self.probe_memory_status(correlation_id).await;
        Ok(ListOutboxResponse {
            memory_status,
            items,
        })
    }

    /// Searches standalone `dennett-memoryd` honestly (V02): if `memoryd` is unreachable, returns
    /// `MemoryStatus::DegradedUnreachable` with an explicit reason and empty notes rather than
    /// fabricating confident memory answers.
    ///
    /// # Errors
    ///
    /// Returns authorization/validation errors reported by `memoryd` when reachable.
    pub async fn search_memory(
        &self,
        req: HeadMemorySearchRequest,
        correlation_id: &CorrelationId,
    ) -> Result<HeadMemorySearchResponse, DennettError> {
        let client_opt = if self.inner.memory_hmac_signer.is_some() {
            let scopes = if let Some(ref scope) = req.scope {
                let mut set = BTreeSet::new();
                set.insert(scope.clone());
                set
            } else {
                self.collect_registered_project_scopes().await?
            };
            self.memory_client_for_scopes(scopes, false)?
        } else {
            self.inner.memory_client.clone()
        };

        let Some(client) = client_opt else {
            self.record_memory_status(MemoryStatus::DegradedUnreachable);
            return Ok(HeadMemorySearchResponse {
                memory_status: MemoryStatus::DegradedUnreachable,
                degraded_reason: Some(String::from(
                    "memoryd client is not configured on dennett-head",
                )),
                notes: Vec::new(),
                index_status: dennett_contracts::IndexStatus::Degraded,
                stale_warning: None,
            });
        };

        let search_req = SearchNotesRequest {
            scope: req.scope,
            query: req.query,
            limit: req.limit,
            include_history: req.include_history,
        };
        match client.search_notes(&search_req, Some(correlation_id)).await {
            Ok(resp) => {
                self.record_memory_status(MemoryStatus::Healthy);
                Ok(HeadMemorySearchResponse {
                    memory_status: MemoryStatus::Healthy,
                    degraded_reason: None,
                    notes: resp.notes,
                    index_status: resp.index_status,
                    stale_warning: resp.stale_warning,
                })
            }
            Err(
                DennettError::DatabaseUnavailable { detail }
                | DennettError::Timeout { operation: detail },
            ) => {
                self.record_memory_status(MemoryStatus::DegradedUnreachable);
                Ok(HeadMemorySearchResponse {
                    memory_status: MemoryStatus::DegradedUnreachable,
                    degraded_reason: Some(self.redact(&format!("memoryd unavailable: {detail}"))),
                    notes: Vec::new(),
                    index_status: dennett_contracts::IndexStatus::Degraded,
                    stale_warning: None,
                })
            }
            Err(other) => Err(other),
        }
    }

    /// Returns secret-free audit events, permission decisions, and outbox items.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::DatabaseUnavailable`] on control PG failure.
    pub async fn get_diagnostics(&self, limit: i64) -> Result<DiagnosticsResponse, DennettError> {
        let _ = self.ensure_migrated().await?;
        let capped = limit.clamp(1, 200);
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;

        let event_rows = conn
            .query(
                "SELECT event_id, correlation_id, session_id, run_id, component,
                        event_kind, safe_summary,
                        TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint
                 FROM audit_events
                 ORDER BY stream_seq DESC
                 LIMIT $1",
                &[&capped],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let decision_rows = conn
            .query(
                "SELECT decision_id, session_id, run_id, tool_call_id, tool_name,
                        target_pattern, scope, effect, matched_rule_id, reason,
                        evaluated_outside_model, correlation_id,
                        TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint
                 FROM permission_decisions
                 ORDER BY created_at DESC
                 LIMIT $1",
                &[&capped],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let outbox_sql = format!(
            "SELECT {ORDERED_OUTBOX_SELECT_COLS}
             FROM ordered_outbox
             WHERE destination = $1
             ORDER BY created_at DESC
             LIMIT $2"
        );
        let outbox_rows = conn
            .query(&outbox_sql, &[&MEMORY_OUTBOX_DESTINATION, &capped])
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let mut outbox_items = Vec::with_capacity(outbox_rows.len());
        for row in &outbox_rows {
            let ordered = decode_ordered_outbox_row(row)?;
            outbox_items.push(ordered_item_to_memory_outbox_item(&ordered)?);
        }

        Ok(DiagnosticsResponse {
            events: event_rows
                .iter()
                .map(decode_audit_event_row)
                .collect::<Result<Vec<_>, _>>()?,
            permission_decisions: decision_rows
                .iter()
                .map(decode_permission_decision_row)
                .collect::<Result<Vec<_>, _>>()?,
            outbox_items,
        })
    }

    #[expect(
        clippy::too_many_lines,
        reason = "delivers one leased ordered_outbox memory item with short-lived token minting, ack, or retry backoff"
    )]
    async fn deliver_leased_memory_outbox_item(
        &self,
        leased: OrderedOutboxItem,
        effective_now: TimestampMs,
        correlation_id: &CorrelationId,
    ) -> Result<(MemoryOutboxItem, MemoryStatus), DennettError> {
        let mut envelope: MemoryOutboxEnvelope = serde_json::from_str(&leased.payload_json)
            .map_err(|err| DennettError::Internal {
                detail: format!("corrupt ordered_outbox memory envelope JSON: {err}"),
            })?;

        let mut scopes = BTreeSet::new();
        scopes.insert(envelope.scope.clone());
        let client_opt = self.memory_client_for_scopes(scopes, true)?;
        let Some(client) = client_opt else {
            self.record_memory_status(MemoryStatus::DegradedUnreachable);
            let safe_err = String::from("memoryd client is not configured on dennett-head");
            let updated_ordered = self
                .nack_ordered_outbox_item(
                    leased.item_id,
                    leased.lease_generation,
                    &safe_err,
                    MEMORY_OUTBOX_MAX_TRANSIENT_ATTEMPTS,
                    0,
                    effective_now,
                )
                .await?;
            self.record_memory_outbox_audit(
                leased.item_id,
                MemoryOutboxStatus::Pending,
                envelope.session_id,
                envelope.run_id,
                correlation_id,
            )
            .await;
            return Ok((
                ordered_item_to_memory_outbox_item(&updated_ordered)?,
                MemoryStatus::DegradedUnreachable,
            ));
        };

        if envelope.forgotten {
            scrub_note_request(&mut envelope.note_request, OUTBOX_SCRUBBED_FORGOTTEN);
            let updated_payload =
                serde_json::to_value(&envelope).map_err(|err| DennettError::Internal {
                    detail: format!("failed serializing forgotten outbox envelope: {err}"),
                })?;
            let updated_ordered = self
                .nack_ordered_outbox_item_with_payload(
                    leased.item_id,
                    leased.lease_generation,
                    "note was previously forgotten; delivery aborted and scrubbed",
                    1,
                    0,
                    TimestampMs::now(),
                    Some(&updated_payload),
                )
                .await?;
            self.record_memory_outbox_audit(
                leased.item_id,
                MemoryOutboxStatus::Failed,
                envelope.session_id,
                envelope.run_id,
                correlation_id,
            )
            .await;
            return Ok((
                ordered_item_to_memory_outbox_item(&updated_ordered)?,
                MemoryStatus::Healthy,
            ));
        }

        // Ensure fingerprint is recorded before delivery scrubbing so idempotent replays still validate.
        if envelope.payload_sha256.is_none() {
            envelope.payload_sha256 = Some(note_request_fingerprint(&envelope.note_request)?);
        }

        match client
            .create_note(&envelope.note_request, Some(correlation_id))
            .await
        {
            Ok(resp) => {
                self.record_memory_status(MemoryStatus::Healthy);
                envelope.delivered_event_id = Some(resp.event_id);
                envelope.delivered_note_id = Some(resp.note.note_id);
                // Immediately scrub plaintext note fields so ZERO secret/user prose lingers in
                // `ordered_outbox.payload_json` after delivery to `memoryd`.
                scrub_note_request(&mut envelope.note_request, OUTBOX_SCRUBBED_DELIVERED);
                let updated_payload =
                    serde_json::to_value(&envelope).map_err(|err| DennettError::Internal {
                        detail: format!("failed serializing delivered outbox envelope: {err}"),
                    })?;
                let updated_ordered = self
                    .ack_ordered_outbox_item_with_payload(
                        leased.item_id,
                        leased.lease_generation,
                        Some(&updated_payload),
                    )
                    .await?;
                self.record_memory_outbox_audit(
                    leased.item_id,
                    MemoryOutboxStatus::Delivered,
                    envelope.session_id,
                    envelope.run_id,
                    correlation_id,
                )
                .await;
                Ok((
                    ordered_item_to_memory_outbox_item(&updated_ordered)?,
                    MemoryStatus::Healthy,
                ))
            }
            Err(
                DennettError::DatabaseUnavailable { detail }
                | DennettError::Timeout { operation: detail },
            ) => {
                self.record_memory_status(MemoryStatus::DegradedUnreachable);
                let safe_err = self.redact(&detail);
                let updated_ordered = self
                    .nack_ordered_outbox_item(
                        leased.item_id,
                        leased.lease_generation,
                        &safe_err,
                        MEMORY_OUTBOX_MAX_TRANSIENT_ATTEMPTS,
                        0,
                        effective_now,
                    )
                    .await?;
                self.record_memory_outbox_audit(
                    leased.item_id,
                    MemoryOutboxStatus::Pending,
                    envelope.session_id,
                    envelope.run_id,
                    correlation_id,
                )
                .await;
                Ok((
                    ordered_item_to_memory_outbox_item(&updated_ordered)?,
                    MemoryStatus::DegradedUnreachable,
                ))
            }
            Err(permanent_err) => {
                self.record_memory_status(MemoryStatus::Healthy);
                let safe_err = self.redact(&permanent_err.to_string());
                let is_previously_forgotten = matches!(
                    &permanent_err,
                    DennettError::Conflict { reason, .. }
                        if reason.to_ascii_lowercase().contains("forgotten")
                );
                let scrubbed_payload = if is_previously_forgotten {
                    scrub_note_request(&mut envelope.note_request, OUTBOX_SCRUBBED_FORGOTTEN);
                    envelope.payload_sha256 = None;
                    envelope.forgotten = true;
                    Some(
                        serde_json::to_value(&envelope).map_err(|err| DennettError::Internal {
                            detail: format!("failed serializing forgotten outbox envelope: {err}"),
                        })?,
                    )
                } else {
                    None
                };
                let updated_ordered = self
                    .nack_ordered_outbox_item_with_payload(
                        leased.item_id,
                        leased.lease_generation,
                        &safe_err,
                        1,
                        0,
                        effective_now,
                        scrubbed_payload.as_ref(),
                    )
                    .await?;
                self.record_memory_outbox_audit(
                    leased.item_id,
                    MemoryOutboxStatus::Failed,
                    envelope.session_id,
                    envelope.run_id,
                    correlation_id,
                )
                .await;
                Ok((
                    ordered_item_to_memory_outbox_item(&updated_ordered)?,
                    MemoryStatus::Healthy,
                ))
            }
        }
    }

    async fn record_memory_outbox_audit(
        &self,
        item_id: dennett_contracts::OrderedOutboxItemId,
        status: MemoryOutboxStatus,
        session_id: Option<SessionId>,
        run_id: Option<RunId>,
        correlation_id: &CorrelationId,
    ) {
        let Ok(mut conn) = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await else {
            return;
        };
        let Ok(tx) = conn.transaction().await else {
            return;
        };
        let event_kind = match status {
            MemoryOutboxStatus::Delivered => "memory_outbox_delivered",
            MemoryOutboxStatus::Pending => "memory_outbox_queued",
            MemoryOutboxStatus::Failed => "memory_outbox_failed",
        };
        let _ = append_audit_event(
            &tx,
            &self.inner.pg_target,
            correlation_id,
            session_id,
            run_id,
            "head",
            event_kind,
            &format!("outbox item {item_id} transitioned to `{status}`"),
        )
        .await;
        let _ = tx.commit().await;
    }

    pub(crate) async fn upsert_runtime_binding(
        &self,
        session_id: SessionId,
        outcome: &dennett_runtime_antigravity::SessionStartOutcome,
        connection_id: Option<ProviderConnectionId>,
        correlation_id: &CorrelationId,
        run_id: Option<RunId>,
    ) -> Result<RuntimeBinding, DennettError> {
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let binding_id = RuntimeBindingId::new();
        let rehydrate_inc: i64 = i64::from(outcome.rehydrated);
        let row = tx
            .query_one(
                "INSERT INTO runtime_bindings (
                    binding_id, session_id, adapter_id, auth_profile, tier, agy_version,
                    provider_session_id, workspace_cwd, is_simulated, rehydrate_count,
                    connection_id, endpoint_fingerprint
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                 ON CONFLICT (session_id)
                 DO UPDATE SET
                    adapter_id = EXCLUDED.adapter_id,
                    auth_profile = EXCLUDED.auth_profile,
                    tier = EXCLUDED.tier,
                    agy_version = EXCLUDED.agy_version,
                    provider_session_id = EXCLUDED.provider_session_id,
                    workspace_cwd = EXCLUDED.workspace_cwd,
                    is_simulated = EXCLUDED.is_simulated,
                    rehydrate_count = runtime_bindings.rehydrate_count + EXCLUDED.rehydrate_count,
                    connection_id = EXCLUDED.connection_id,
                    endpoint_fingerprint = EXCLUDED.endpoint_fingerprint,
                    updated_at = now()
                 RETURNING binding_id, session_id, adapter_id, auth_profile, tier, agy_version,
                           provider_session_id, workspace_cwd, is_simulated, rehydrate_count,
                           TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
                           TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint,
                           connection_id, endpoint_fingerprint",
                &[
                    &binding_id.as_uuid(),
                    &session_id.as_uuid(),
                    &outcome.adapter_id,
                    &outcome.auth_profile.as_method_id(),
                    &outcome.tier,
                    &outcome.agy_version,
                    &Some(outcome.provider_session_id.as_str()),
                    &outcome.workspace_cwd,
                    &outcome.is_simulated,
                    &rehydrate_inc,
                    &connection_id.map(ProviderConnectionId::as_uuid),
                    &outcome.endpoint_fingerprint.as_deref(),
                ],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        if outcome.rehydrated {
            let reason = outcome
                .rehydrate_reason
                .as_deref()
                .unwrap_or("explicit rehydrate");
            let safe = self.redact(&format!(
                "rehydrated session {session_id} into fresh provider_session `{}`: {reason}",
                outcome.provider_session_id
            ));
            let _ = append_audit_event(
                &tx,
                &self.inner.pg_target,
                correlation_id,
                Some(session_id),
                run_id,
                "head",
                "session_rehydrated",
                &safe,
            )
            .await?;
        }

        let binding = decode_runtime_binding_row(&row)?;
        tx.commit()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
        Ok(binding)
    }

    #[expect(
        clippy::too_many_arguments,
        clippy::too_many_lines,
        reason = "atomic turn finalization persists decisions, tool observations, assistant message, honest run status, and ordered journal event together"
    )]
    async fn finalize_turn(
        &self,
        session_id: SessionId,
        project_id: Option<ProjectId>,
        run_id: RunId,
        agent: &AgentRecord,
        user_message_id: MessageId,
        rehydrated: bool,
        outcome: &dennett_runtime_antigravity::TurnOutcome,
        elapsed_ms: u64,
        context_used_tokens: Option<u64>,
        context_window_tokens: Option<u64>,
        decisions: &[CapturedPermissionDecision],
        hit_unknown_external: bool,
        correlation_id: &CorrelationId,
    ) -> Result<(Option<MessageRecord>, RunRecord), DennettError> {
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let assistant_message = if outcome.assistant_text.trim().is_empty() {
            None
        } else {
            let msg_id = MessageId::new();
            let stream_id = format!("session:{session_id}:messages");
            let msg_seq = next_stream_seq(&tx, &stream_id, &self.inner.pg_target).await?;
            let safe_text = self.redact(&outcome.assistant_text);
            let msg_row = tx
                .query_one(
                    "INSERT INTO messages (
                        message_id, session_id, stream_seq, role, content, run_id, correlation_id
                     ) VALUES ($1, $2, $3, 'assistant', $4, $5, $6)
                     RETURNING TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint",
                    &[
                        &msg_id.as_uuid(),
                        &session_id.as_uuid(),
                        &msg_seq,
                        &safe_text,
                        &run_id.as_uuid(),
                        &correlation_id.as_str(),
                    ],
                )
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
            Some(MessageRecord {
                message_id: msg_id,
                session_id,
                role: MessageRole::Assistant,
                content: safe_text,
                run_id: Some(run_id),
                client_message_id: None,
                correlation_id: correlation_id.clone(),
                created_at: TimestampMs::from_millis(msg_row.get(0)),
            })
        };

        for cap in decisions {
            let dec = &cap.record;
            if !cap.already_persisted {
                insert_permission_decision(&tx, &self.inner.pg_target, dec).await?;
            }
            let _ = append_audit_event(
                &tx,
                &self.inner.pg_target,
                correlation_id,
                Some(session_id),
                Some(run_id),
                "policy_engine",
                if dec.effect == dennett_contracts::PolicyEffect::Allow {
                    "permission_allowed"
                } else {
                    "permission_denied"
                },
                &self.redact(&format!(
                    "tool `{}` target={:?} -> {} ({})",
                    dec.tool_name, dec.target_pattern, dec.effect, dec.reason
                )),
            )
            .await?;
        }

        for ev in &outcome.events {
            if let RuntimeEvent::ToolObserved {
                tool_name,
                title,
                status,
                ..
            } = ev
            {
                let _ = append_audit_event(
                    &tx,
                    &self.inner.pg_target,
                    correlation_id,
                    Some(session_id),
                    Some(run_id),
                    self.inner.runtime.adapter_id(),
                    "tool_observed",
                    &self.redact(&format!("tool `{tool_name}` ({title}) status={status}")),
                )
                .await?;
            }
        }

        // D2-ARCH-03: Never overwrite `unknown_external` or hide unknown/dispatching external effects!
        let has_unknown_effect: bool = hit_unknown_external
            || tx
                .query_one(
                    "SELECT EXISTS(
                        SELECT 1 FROM effect_claims
                        WHERE run_id = $1 AND status IN ('unknown', 'dispatching')
                    )",
                    &[&run_id.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?
                .get(0);
        let requested_status = if has_unknown_effect {
            RunStatus::UnknownExternal
        } else {
            outcome.status
        };

        // D2-BUG-04: If run was cancelled before producing an assistant response, release any
        // stamped inter-agent messages so they are not silently lost!
        if requested_status == RunStatus::Cancelled && assistant_message.is_none() {
            tx.execute(
                "UPDATE agent_messages
                 SET delivered_in_run_id = NULL, delivered_at = NULL
                 WHERE delivered_in_run_id = $1",
                &[&run_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
        }

        let stop_reason = truncate_utf8(&outcome.stop_reason, 128).to_owned();
        let tokens_used = outcome.total_tokens();
        let tokens_i64 = tokens_used.and_then(|v| i64::try_from(v).ok());
        let wall_i64 = i64::try_from(elapsed_ms).ok();
        let ctx_used_i64 = context_used_tokens.and_then(|v| i64::try_from(v).ok());
        let ctx_window_i64 = context_window_tokens.and_then(|v| i64::try_from(v).ok());
        let run_row = tx
            .query_one(
                "UPDATE runs
                 SET assistant_message_id = $2,
                     status = CASE WHEN status = 'unknown_external' THEN 'unknown_external' ELSE $3 END,
                     stop_reason = $4,
                     rehydrated = $5,
                     tokens_used = $6,
                     wall_ms = $7,
                     context_used_tokens = $8,
                     context_window_tokens = $9,
                     finished_at = now()
                 WHERE run_id = $1
                 RETURNING status,
                           TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
                           TRUNC(EXTRACT(EPOCH FROM finished_at) * 1000)::bigint",
                &[
                    &run_id.as_uuid(),
                    &assistant_message.as_ref().map(|m| m.message_id.as_uuid()),
                    &requested_status.as_str(),
                    &Some(stop_reason.as_str()),
                    &rehydrated,
                    &tokens_i64,
                    &wall_i64,
                    &ctx_used_i64,
                    &ctx_window_i64,
                ],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
        let persisted_status = RunStatus::from_str(run_row.get(0))?;

        tx.execute(
            "UPDATE sessions SET updated_at = now() WHERE session_id = $1",
            &[&session_id.as_uuid()],
        )
        .await
        .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let mut pending_events = Vec::new();
        let expired_perms = self
            .expire_pending_approvals_for_run_in_tx(
                &tx,
                run_id,
                if persisted_status == RunStatus::Cancelled {
                    "withdrawn"
                } else {
                    "expired"
                },
                "Ход завершён",
                &mut pending_events,
            )
            .await?;

        let _ = append_audit_event(
            &tx,
            &self.inner.pg_target,
            correlation_id,
            Some(session_id),
            Some(run_id),
            "head",
            if persisted_status == RunStatus::Cancelled {
                "run_cancelled"
            } else if persisted_status == RunStatus::UnknownExternal {
                "run_unknown_external"
            } else {
                "run_succeeded"
            },
            &format!(
                "run {run_id} finished status={} stop_reason={} decisions={}",
                persisted_status,
                stop_reason,
                decisions.len()
            ),
        )
        .await?;

        pending_events.push(PendingControlEvent {
            entity_key: format!("session:{session_id}"),
            event_type: "session.message_completed",
            project_id,
            session_id: Some(session_id),
            task_id: None,
            payload: json!({
                "session_id": session_id,
                "run_id": run_id,
                "status": persisted_status,
                "tokens_used": tokens_used,
                "wall_ms": elapsed_ms,
            }),
        });
        let _ = append_control_events_batch_in_tx(
            &tx,
            &self.inner.pg_target,
            correlation_id,
            &pending_events,
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        if !expired_perms.is_empty() {
            self.wake_expired_approval_waiters(&expired_perms, session_id, "run finalized")
                .await;
        }

        let run_record = RunRecord {
            run_id,
            session_id,
            agent_id: agent.agent_id,
            agent_config_version_id: agent.current_config_version_id,
            user_message_id,
            assistant_message_id: assistant_message.as_ref().map(|m| m.message_id),
            status: persisted_status,
            stop_reason: Some(stop_reason),
            error_code: None,
            error_summary: None,
            rehydrated,
            task_id: None,
            task_generation: None,
            tokens_used,
            wall_ms: Some(elapsed_ms),
            context_used_tokens,
            context_window_tokens,
            correlation_id: correlation_id.clone(),
            created_at: TimestampMs::from_millis(run_row.get(1)),
            finished_at: Some(TimestampMs::from_millis(run_row.get(2))),
        };

        Ok((assistant_message, run_record))
    }

    async fn record_failed_run(
        &self,
        session_id: SessionId,
        run_id: RunId,
        correlation_id: &CorrelationId,
        err: &DennettError,
        decisions: &[CapturedPermissionDecision],
    ) {
        let Ok(mut conn) = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await else {
            return;
        };
        let Ok(tx) = conn.transaction().await else {
            return;
        };
        for cap in decisions {
            if !cap.already_persisted {
                let _ = insert_permission_decision(&tx, &self.inner.pg_target, &cap.record).await;
            }
        }
        // D2-BUG-04: Return undelivered inter-agent messages back to queue on failed turn!
        let _ = tx
            .execute(
                "UPDATE agent_messages
                 SET delivered_in_run_id = NULL, delivered_at = NULL
                 WHERE delivered_in_run_id = $1",
                &[&run_id.as_uuid()],
            )
            .await;
        let safe_summary = self.redact(&err.to_string());
        let truncated = truncate_utf8(&safe_summary, 2000).to_owned();
        let _ = tx
            .execute(
                "UPDATE runs
                 SET status = CASE WHEN status = 'unknown_external' THEN 'unknown_external' ELSE 'failed' END,
                     error_code = $2,
                     error_summary = $3,
                     finished_at = now()
                 WHERE run_id = $1",
                &[&run_id.as_uuid(), &err.code(), &truncated],
            )
            .await;
        let mut pending_events = Vec::new();
        let expired_perms = self
            .expire_pending_approvals_for_run_in_tx(
                &tx,
                run_id,
                "expired",
                &format!("Ход завершился ошибкой ({}): {truncated}", err.code()),
                &mut pending_events,
            )
            .await
            .unwrap_or_default();
        let _ = append_audit_event(
            &tx,
            &self.inner.pg_target,
            correlation_id,
            Some(session_id),
            Some(run_id),
            "head",
            "run_failed",
            &format!("run {run_id} failed [{}]: {truncated}", err.code()),
        )
        .await;
        let _ = append_control_events_batch_in_tx(
            &tx,
            &self.inner.pg_target,
            correlation_id,
            &pending_events,
        )
        .await;
        let _ = tx.commit().await;
        if !expired_perms.is_empty() {
            self.wake_expired_approval_waiters(&expired_perms, session_id, &truncated)
                .await;
        }
    }

    /// Finalizes a run whose detached worker panicked or was aborted on daemon shutdown,
    /// ensuring PostgreSQL `runs.status = 'running'`, pending approvals, and stamped
    /// `agent_messages` are NEVER stranded.
    pub(crate) async fn finalize_abandoned_run(
        &self,
        session_id: SessionId,
        run_id: RunId,
        correlation_id: &CorrelationId,
    ) {
        let _ = self.runtime_router().cancel_turn_all(session_id).await;
        let Ok(mut conn) = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await else {
            return;
        };
        let Ok(tx) = conn.transaction().await else {
            return;
        };
        let _ = tx
            .execute(
                "UPDATE agent_messages
                 SET delivered_in_run_id = NULL, delivered_at = NULL
                 WHERE delivered_in_run_id = $1",
                &[&run_id.as_uuid()],
            )
            .await;
        let updated = tx
            .execute(
                "UPDATE runs
                 SET status = 'cancelled',
                     stop_reason = 'worker_aborted_or_shutdown',
                     finished_at = now()
                 WHERE run_id = $1 AND status = 'running'",
                &[&run_id.as_uuid()],
            )
            .await
            .unwrap_or(0);
        let _ = tx
            .execute(
                "UPDATE session_followups
                 SET status = 'paused', updated_at = now()
                 WHERE session_id = $1 AND status = 'queued'",
                &[&session_id.as_uuid()],
            )
            .await;
        let mut pending_events = Vec::new();
        let expired_perms = self
            .expire_pending_approvals_for_run_in_tx(
                &tx,
                run_id,
                "withdrawn",
                "Выполнение было прервано при остановке воркера",
                &mut pending_events,
            )
            .await
            .unwrap_or_default();
        if updated > 0 {
            let _ = append_audit_event(
                &tx,
                &self.inner.pg_target,
                correlation_id,
                Some(session_id),
                Some(run_id),
                "head",
                "run_abandoned",
                &format!("run {run_id} abandoned (worker panic or daemon shutdown)"),
            )
            .await;
        }
        let _ = append_control_events_batch_in_tx(
            &tx,
            &self.inner.pg_target,
            correlation_id,
            &pending_events,
        )
        .await;
        let _ = tx.commit().await;
        if !expired_perms.is_empty() {
            self.wake_expired_approval_waiters(
                &expired_perms,
                session_id,
                "abandoned due to worker abort or shutdown",
            )
            .await;
        }
    }

    fn memory_base_url(&self) -> Option<&str> {
        self.inner
            .memory_client
            .as_ref()
            .map(MemoryClient::base_url)
            .or_else(|| {
                self.inner
                    .memory_mcp
                    .as_ref()
                    .map(|m| m.memory_url.as_str())
            })
    }

    pub(crate) fn memory_client_with_capabilities(
        &self,
        scopes: BTreeSet<MemoryScope>,
        caps: BTreeSet<MemoryCapability>,
    ) -> Result<Option<MemoryClient>, DennettError> {
        if let Some(ref signer) = self.inner.memory_hmac_signer
            && let Some(base_url) = self.memory_base_url()
            && !scopes.is_empty()
        {
            let principal = PrincipalId::try_new("dennett-head")?;
            let token = signer.mint_scoped_token(principal, scopes, caps)?;
            return Ok(Some(MemoryClient::new(base_url, token)?));
        }
        Ok(self.inner.memory_client.clone())
    }

    fn memory_client_for_scopes(
        &self,
        scopes: BTreeSet<MemoryScope>,
        allow_write: bool,
    ) -> Result<Option<MemoryClient>, DennettError> {
        let mut caps = BTreeSet::new();
        caps.insert(MemoryCapability::Read);
        if allow_write {
            caps.insert(MemoryCapability::Write);
        }
        self.memory_client_with_capabilities(scopes, caps)
    }

    async fn collect_registered_project_scopes(
        &self,
    ) -> Result<BTreeSet<MemoryScope>, DennettError> {
        let mut scopes = BTreeSet::new();
        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let rows = conn
            .query("SELECT DISTINCT memory_scope FROM projects", &[])
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
        for row in rows {
            let raw: String = row.get(0);
            if let Ok(scope) = MemoryScope::try_new(raw) {
                scopes.insert(scope);
            }
        }
        if scopes.is_empty() {
            scopes.insert(MemoryScope::try_new("project:default")?);
        }
        Ok(scopes)
    }

    pub(crate) async fn collect_all_known_memory_scopes(
        &self,
    ) -> Result<BTreeSet<MemoryScope>, DennettError> {
        let mut scopes = self.collect_registered_project_scopes().await?;
        if let Ok(standalone) = MemoryScope::try_new("chat:standalone") {
            scopes.insert(standalone);
        }
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let agent_rows = conn
            .query(
                "SELECT v.allowed_memory_scopes
                 FROM agents a
                 JOIN agent_config_versions v ON v.config_version_id = a.current_config_version_id",
                &[],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
        for row in agent_rows {
            if let Ok(vec_scopes) =
                serde_json::from_value::<Vec<MemoryScope>>(row.get::<_, serde_json::Value>(0))
            {
                scopes.extend(vec_scopes);
            }
        }
        let outbox_rows = conn
            .query(
                "SELECT DISTINCT ordering_key FROM ordered_outbox WHERE destination = $1",
                &[&MEMORY_OUTBOX_DESTINATION],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
        for row in outbox_rows {
            let raw: String = row.get(0);
            if let Ok(scope) = MemoryScope::try_new(raw) {
                scopes.insert(scope);
            }
        }
        Ok(scopes)
    }

    pub(crate) fn secret_cipher(&self) -> &SecretCipher {
        &self.inner.secret_cipher
    }

    pub(crate) fn env_gemini_api_key(&self) -> Option<String> {
        if let Some(ref forced) = self.inner.gemini_api_key_env_override {
            return forced
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToOwned::to_owned);
        }
        std::env::var("DENNETT_GEMINI_API_KEY")
            .ok()
            .or_else(|| std::env::var("GEMINI_API_KEY").ok())
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
    }

    pub(crate) fn dynamic_runtimes_lock(
        &self,
    ) -> &Arc<Mutex<HashMap<ProviderConnectionId, CachedDynamicRuntime>>> {
        &self.inner.dynamic_runtimes
    }

    pub(crate) fn background_runtime(&self) -> Option<&Arc<dyn AgentRuntimePort>> {
        self.inner.background_runtime.as_ref()
    }

    pub(crate) fn allow_non_loopback_local_runtime(&self) -> bool {
        self.inner.allow_non_loopback_local_runtime
    }

    pub(crate) fn antigravity_profile_root(&self) -> PathBuf {
        self.inner.antigravity_profile_root.clone()
    }

    pub(crate) fn antigravity_binary(&self) -> Option<PathBuf> {
        self.inner.antigravity_binary.clone()
    }

    pub(crate) fn resolve_personal_profile_dir(&self) -> Option<PathBuf> {
        if let Some(ref explicit) = self.inner.personal_profile_dir {
            return Some(explicit.clone());
        }
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .filter(|v| !v.is_empty())?;
        Some(PathBuf::from(home).join(".gemini"))
    }

    pub(crate) async fn invalidate_runtime_catalog_cache(&self) {
        let mut cache_guard = self.inner.runtime_catalog_cache.lock().await;
        *cache_guard = None;
    }

    pub(crate) fn record_memory_status_pub(&self, status: MemoryStatus) {
        self.record_memory_status(status);
    }

    pub(crate) fn pg_target(&self) -> &ValidatedControlPgTarget {
        &self.inner.pg_target
    }

    pub(crate) fn runtime(&self) -> &Arc<dyn AgentRuntimePort> {
        &self.inner.runtime
    }

    pub(crate) fn effect_port(&self) -> &Arc<dyn ExternalEffectPort> {
        &self.inner.effect_port
    }

    pub(crate) fn detached_project_scratch_dir(&self, project_id: ProjectId) -> PathBuf {
        self.inner
            .detached_projects_root
            .join("detached-projects")
            .join(project_id.to_string())
    }

    pub(crate) fn ensure_detached_project_dir(
        &self,
        project_id: ProjectId,
    ) -> Result<PathBuf, DennettError> {
        let dir = self.detached_project_scratch_dir(project_id);
        fs::create_dir_all(&dir).map_err(|err| DennettError::Internal {
            detail: format!("failed creating isolated detached project workspace: {err}"),
        })?;
        Ok(dir)
    }

    pub(crate) fn standalone_session_scratch_dir(&self, session_id: SessionId) -> PathBuf {
        self.inner
            .detached_projects_root
            .join("standalone-sessions")
            .join(session_id.to_string())
    }

    pub(crate) fn ensure_standalone_session_dir(
        &self,
        session_id: SessionId,
    ) -> Result<PathBuf, DennettError> {
        let dir = self.standalone_session_scratch_dir(session_id);
        fs::create_dir_all(&dir).map_err(|err| DennettError::Internal {
            detail: format!("failed creating isolated standalone session workspace: {err}"),
        })?;
        Ok(dir)
    }

    /// Builds least-privilege `dennett-memory` MCP server spec (`D2-SEC-01`).
    ///
    /// Security invariants:
    /// - Empty `agent.current_config.allowed_memory_scopes` ALWAYS returns `Ok(Vec::new())` (never
    ///   mounts memory MCP and never falls back to a static token!).
    /// - When `memory_hmac_signer` is configured, token mint failure fails CLOSED (`Err(...)`)
    ///   rather than silently substituting Head's static token!
    ///
    /// # Errors
    ///
    /// Returns [`DennettError`] if scoped token minting fails or no token is available.
    pub(crate) fn build_memory_mcp_specs(
        &self,
        agent: &AgentRecord,
    ) -> Result<Vec<RuntimeMcpServerSpec>, DennettError> {
        let Some(ref mcp) = self.inner.memory_mcp else {
            return Ok(Vec::new());
        };
        if agent.current_config.allowed_memory_scopes.is_empty() {
            return Ok(Vec::new());
        }

        let effective_token = if let Some(ref signer) = self.inner.memory_hmac_signer {
            let mut caps = BTreeSet::new();
            caps.insert(MemoryCapability::Read);
            caps.insert(MemoryCapability::Write);
            let principal = PrincipalId::try_new(format!("agent:{}", agent.agent_id))?;
            signer.mint_scoped_token(
                principal,
                agent.current_config.allowed_memory_scopes.clone(),
                caps,
            )?
        } else if !mcp.memory_token.trim().is_empty() {
            mcp.memory_token.clone()
        } else {
            return Err(DennettError::Internal {
                detail: String::from(
                    "memory MCP configured without HMAC signer or static memory token",
                ),
            });
        };

        // CRITICAL SECURITY BOUNDARY: pass ONLY `DENNETT_MEMORY_URL` and scoped
        // `DENNETT_MEMORY_TOKEN` — NEVER PostgreSQL credentials or Head DB URLs!
        Ok(vec![RuntimeMcpServerSpec {
            name: String::from("dennett-memory"),
            command: mcp.binary_path.to_string_lossy().into_owned(),
            args: Vec::new(),
            env: vec![
                RuntimeMcpEnvEntry {
                    name: String::from("DENNETT_MEMORY_URL"),
                    value: mcp.memory_url.clone(),
                },
                RuntimeMcpEnvEntry {
                    name: String::from("DENNETT_MEMORY_TOKEN"),
                    value: effective_token,
                },
            ],
        }])
    }

    /// Builds session-scoped `dennett-control` MCP server spec (`dennett-control-mcp`) when both
    /// the binary path and `dennett-head`'s bound loopback HTTP URL are configured.
    ///
    /// Works for BOTH project-bound sessions (`project_id = Some(...)`) and standalone chats
    /// (`project_id = None`, exposing only `dennett_skill_list`/`dennett_skill_read` while failing
    /// closed on all project event/task/delegation tools).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError`] if `dct1.` token minting fails.
    pub(crate) fn build_control_mcp_spec(
        &self,
        project_id: Option<ProjectId>,
        agent_id: AgentId,
        session_id: SessionId,
        session_kind: SessionKind,
    ) -> Result<Option<RuntimeMcpServerSpec>, DennettError> {
        let Some(ref bin) = self.inner.control_mcp_bin else {
            return Ok(None);
        };
        let Some(head_url) = self.self_http_url() else {
            return Ok(None);
        };
        let token = self.inner.control_token_signer.mint_control_token(
            project_id,
            agent_id,
            session_id,
            session_kind,
        )?;
        let mut env = vec![
            RuntimeMcpEnvEntry {
                name: String::from("DENNETT_HEAD_URL"),
                value: head_url,
            },
            RuntimeMcpEnvEntry {
                name: String::from("DENNETT_CONTROL_AGENT_ID"),
                value: agent_id.to_string(),
            },
            RuntimeMcpEnvEntry {
                name: String::from("DENNETT_CONTROL_SESSION_ID"),
                value: session_id.to_string(),
            },
            RuntimeMcpEnvEntry {
                name: String::from("DENNETT_CONTROL_TOKEN"),
                value: token,
            },
        ];
        if let Some(pid) = project_id {
            env.push(RuntimeMcpEnvEntry {
                name: String::from("DENNETT_CONTROL_PROJECT_ID"),
                value: pid.to_string(),
            });
        }
        Ok(Some(RuntimeMcpServerSpec {
            name: String::from("dennett-control"),
            command: bin.to_string_lossy().into_owned(),
            args: Vec::new(),
            env,
        }))
    }

    pub(crate) fn collect_turn_secrets(&self, mcp_servers: &[RuntimeMcpServerSpec]) -> Vec<String> {
        let mut turn_secrets: Vec<String> = self
            .inner
            .secrets_to_redact
            .read()
            .map_or_default(|g| g.clone());
        for server in mcp_servers {
            for entry in &server.env {
                let name_upper = entry.name.to_ascii_uppercase();
                let is_sensitive = matches!(
                    entry.name.as_str(),
                    "DENNETT_MEMORY_TOKEN" | "DENNETT_CONTROL_TOKEN"
                ) || name_upper.contains("TOKEN")
                    || name_upper.contains("SECRET")
                    || name_upper.contains("KEY")
                    || name_upper.contains("PASS")
                    || name_upper.contains("AUTH")
                    || name_upper.contains("CREDENTIAL");
                let trimmed = entry.value.trim();
                if is_sensitive && trimmed.len() >= 3 && !turn_secrets.iter().any(|s| s == trimmed)
                {
                    turn_secrets.push(trimmed.to_owned());
                }
            }
        }
        turn_secrets
    }

    pub(crate) async fn register_approval_waiter(
        &self,
        request_id: PermissionRequestId,
        tx: oneshot::Sender<ApprovalResolutionSignal>,
    ) {
        self.inner
            .pending_approvals
            .lock()
            .await
            .insert(request_id, tx);
    }

    pub(crate) async fn remove_approval_waiter(&self, request_id: PermissionRequestId) {
        self.inner
            .pending_approvals
            .lock()
            .await
            .remove(&request_id);
    }

    pub(crate) async fn wake_approval_waiter(
        &self,
        request_id: PermissionRequestId,
        signal: ApprovalResolutionSignal,
    ) {
        if let Some(tx) = self
            .inner
            .pending_approvals
            .lock()
            .await
            .remove(&request_id)
        {
            let _ = tx.send(signal);
        }
    }

    pub(crate) fn startup_recovery_lock(&self) -> &Mutex<bool> {
        &self.inner.startup_recovery_done
    }

    pub(crate) fn active_tasks_map(&self) -> Arc<Mutex<HashMap<TaskId, ActiveTaskHandle>>> {
        Arc::clone(&self.inner.active_tasks)
    }

    pub(crate) async fn register_active_task(&self, task_id: TaskId, handle: ActiveTaskHandle) {
        self.inner.active_tasks.lock().await.insert(task_id, handle);
    }

    pub(crate) async fn try_register_active_session_run(
        &self,
        session_id: SessionId,
        handle: ActiveRunHandle,
    ) -> Result<(), DennettError> {
        let mut active = self.inner.active_runs.lock().await;
        if active.contains_key(&session_id) {
            return Err(DennettError::Conflict {
                resource: String::from("session_run"),
                expected_revision: None,
                actual_revision: None,
                reason: String::from("another run is already active in this session"),
            });
        }
        let now = TimestampMs::now();
        let initial_snapshot = LiveRunSnapshot {
            session_id,
            run_id: handle.run_id,
            streaming_text: String::new(),
            context_used_tokens: None,
            context_window_tokens: None,
            started_at: now,
            updated_at: now,
        };
        active.insert(session_id, handle);
        let mut snaps = self
            .inner
            .live_snapshots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        snaps.insert(session_id, initial_snapshot);
        Ok(())
    }

    pub(crate) async fn unregister_active_session_run(&self, session_id: SessionId, run_id: RunId) {
        self.remove_live_snapshot(session_id, run_id);
        let mut active = self.inner.active_runs.lock().await;
        if active.get(&session_id).is_some_and(|h| h.run_id == run_id) {
            active.remove(&session_id);
        }
    }

    pub(crate) fn get_live_snapshot(&self, session_id: SessionId) -> Option<LiveRunSnapshot> {
        let snaps = self
            .inner
            .live_snapshots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        snaps.get(&session_id).cloned()
    }

    pub(crate) fn mutate_live_snapshot(
        &self,
        session_id: SessionId,
        run_id: RunId,
        f: impl FnOnce(&mut LiveRunSnapshot),
    ) {
        let mut snaps = self
            .inner
            .live_snapshots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(snap) = snaps.get_mut(&session_id)
            && snap.run_id == run_id
        {
            f(snap);
        }
    }

    pub(crate) fn remove_live_snapshot(&self, session_id: SessionId, run_id: RunId) {
        let mut snaps = self
            .inner
            .live_snapshots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if snaps.get(&session_id).is_some_and(|s| s.run_id == run_id) {
            snaps.remove(&session_id);
        }
    }

    pub(crate) fn is_session_steering(&self, session_id: SessionId) -> bool {
        let map = self
            .inner
            .steering_sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        map.get(&session_id).copied().unwrap_or(0) > 0
    }

    pub(crate) async fn is_session_active_or_steering(&self, session_id: SessionId) -> bool {
        if self.is_session_steering(session_id) {
            return true;
        }
        self.inner
            .active_runs
            .lock()
            .await
            .contains_key(&session_id)
    }

    pub(crate) async fn cancel_active_session_run(&self, session_id: SessionId) -> Option<RunId> {
        let active = self.inner.active_runs.lock().await;
        active.get(&session_id).map(|h| {
            let _ = h.cancel_tx.send(true);
            h.run_id
        })
    }

    pub(crate) async fn active_run_and_task_ids(&self) -> (Vec<RunId>, Vec<TaskId>) {
        let run_ids: Vec<RunId> = self
            .inner
            .active_runs
            .lock()
            .await
            .values()
            .map(|h| h.run_id)
            .collect();
        let task_ids: Vec<TaskId> = self
            .inner
            .active_tasks
            .lock()
            .await
            .keys()
            .copied()
            .collect();
        (run_ids, task_ids)
    }

    pub(crate) async fn is_task_cancel_signalled(&self, task_id: TaskId) -> bool {
        if self.inner.stopped_tasks.lock().await.contains(&task_id) {
            return true;
        }
        self.inner
            .active_tasks
            .lock()
            .await
            .get(&task_id)
            .is_some_and(|h| *h.cancel_tx.borrow())
    }

    pub(crate) async fn signal_task_cancel_only(
        &self,
        task_id: TaskId,
    ) -> Option<(SessionId, RunId)> {
        self.inner.stopped_tasks.lock().await.insert(task_id);
        let active = self.inner.active_tasks.lock().await;
        active.get(&task_id).map(|h| {
            let _ = h.cancel_tx.send(true);
            (h.session_id, h.run_id)
        })
    }

    pub(crate) async fn clear_stopped_task(&self, task_id: TaskId) {
        self.inner.stopped_tasks.lock().await.remove(&task_id);
    }

    pub(crate) async fn signal_old_task_generation_preempt(
        &self,
        task_id: TaskId,
        new_generation: i64,
    ) {
        let maybe_session = {
            let active = self.inner.active_tasks.lock().await;
            active
                .get(&task_id)
                .filter(|h| h.generation < new_generation)
                .map(|h| {
                    let _ = h.cancel_tx.send(true);
                    (h.session_id, h.run_id)
                })
        };
        if let Some((session_id, run_id)) = maybe_session {
            let _ = self
                .signal_session_run_if_matching(session_id, run_id)
                .await;
            let _ = self.runtime_router().cancel_turn_all(session_id).await;
        }
    }

    pub(crate) async fn signal_session_run_if_matching(
        &self,
        session_id: SessionId,
        expected_run_id: RunId,
    ) -> bool {
        let active = self.inner.active_runs.lock().await;
        if let Some(h) = active.get(&session_id)
            && h.run_id == expected_run_id
        {
            let _ = h.cancel_tx.send(true);
            return true;
        }
        false
    }

    pub(crate) async fn session_exec_lock(&self, session_id: SessionId) -> Arc<Mutex<()>> {
        let mut locks = self.inner.session_locks.lock().await;
        Arc::clone(
            locks
                .entry(session_id)
                .or_insert_with(|| Arc::new(Mutex::new(()))),
        )
    }

    /// Prunes an idle per-session execution lock entry when no other caller holds a handle
    /// (`D2-BUG-05` memory hygiene).
    pub(crate) async fn prune_session_lock(&self, session_id: SessionId) {
        let mut locks = self.inner.session_locks.lock().await;
        if locks
            .get(&session_id)
            .is_some_and(|lock| Arc::strong_count(lock) == 1)
        {
            locks.remove(&session_id);
        }
    }

    pub(crate) fn cached_memory_status(&self) -> MemoryStatus {
        if self.inner.last_memory_status.load(Ordering::Acquire) == MEMORY_STATUS_HEALTHY {
            MemoryStatus::Healthy
        } else {
            MemoryStatus::DegradedUnreachable
        }
    }

    fn record_memory_status(&self, status: MemoryStatus) {
        let encoded = if status == MemoryStatus::Healthy {
            MEMORY_STATUS_HEALTHY
        } else {
            MEMORY_STATUS_DEGRADED
        };
        self.inner
            .last_memory_status
            .store(encoded, Ordering::Release);
    }

    async fn probe_memory_status(
        &self,
        correlation_id: &CorrelationId,
    ) -> (MemoryStatus, Option<String>) {
        let Some(ref client) = self.inner.memory_client else {
            self.record_memory_status(MemoryStatus::DegradedUnreachable);
            return (
                MemoryStatus::DegradedUnreachable,
                Some(String::from("memoryd client not configured")),
            );
        };
        let result = match time::timeout(
            DEFAULT_PROBE_TIMEOUT,
            client.healthz(Some(correlation_id)),
        )
        .await
        {
            Ok(Ok(health)) if health.postgres_connected => (MemoryStatus::Healthy, None),
            Ok(Ok(_)) => (
                MemoryStatus::DegradedUnreachable,
                Some(String::from("memoryd reported postgres_connected=false")),
            ),
            Ok(Err(err)) => (
                MemoryStatus::DegradedUnreachable,
                Some(self.redact(&err.to_string())),
            ),
            Err(_) => (
                MemoryStatus::DegradedUnreachable,
                Some(String::from("memoryd health probe timed out")),
            ),
        };
        self.record_memory_status(result.0);
        result
    }

    pub(crate) async fn get_conn(
        &self,
        timeout: Duration,
    ) -> Result<deadpool_postgres::Object, DennettError> {
        time::timeout(timeout, self.inner.pool.get())
            .await
            .map_err(|_| DennettError::DatabaseUnavailable {
                detail: String::from("timed out acquiring control PostgreSQL connection"),
            })?
            .map_err(|err| DennettError::DatabaseUnavailable {
                detail: self.inner.pg_target.sanitize_error(&err.to_string()),
            })
    }
}

pub(crate) async fn insert_permission_decision<C: deadpool_postgres::GenericClient>(
    client: &C,
    target: &ValidatedControlPgTarget,
    dec: &PermissionDecisionRecord,
) -> Result<(), DennettError> {
    client
        .execute(
            "INSERT INTO permission_decisions (
                decision_id, session_id, run_id, tool_call_id, tool_name,
                target_pattern, scope, effect, matched_rule_id, reason,
                evaluated_outside_model, correlation_id
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, TRUE, $11)",
            &[
                &dec.decision_id.as_uuid(),
                &dec.session_id.as_uuid(),
                &dec.run_id.map(RunId::as_uuid),
                &truncate_utf8(&dec.tool_call_id, 256),
                &truncate_utf8(&dec.tool_name, 256),
                &dec.target_pattern
                    .as_deref()
                    .map(|t| truncate_utf8(t, 1024)),
                &truncate_utf8(&dec.scope, 128),
                &dec.effect.as_str(),
                &dec.matched_rule_id.map(PolicyRuleId::as_uuid),
                &truncate_utf8(&dec.reason, 1024),
                &dec.correlation_id.as_str(),
            ],
        )
        .await
        .map_err(|err| map_pg_err(target, &err))?;
    Ok(())
}

const _: () = {
    const fn assert_send_sync<T: Send + Sync + 'static>() {}
    assert_send_sync::<HeadService>();
    assert_send_sync::<HeadServiceExtras>();
    assert_send_sync::<MemoryMcpWiring>();
};

fn ordered_item_to_memory_outbox_item(
    ordered: &OrderedOutboxItem,
) -> Result<MemoryOutboxItem, DennettError> {
    let env: MemoryOutboxEnvelope =
        serde_json::from_str(&ordered.payload_json).map_err(|err| DennettError::Internal {
            detail: format!("corrupt ordered_outbox memory envelope JSON: {err}"),
        })?;
    let payload_json =
        serde_json::to_string(&env.note_request).map_err(|err| DennettError::Internal {
            detail: format!("failed serializing memory outbox note_request: {err}"),
        })?;
    let status = match ordered.status {
        OrderedOutboxStatus::Delivered => MemoryOutboxStatus::Delivered,
        OrderedOutboxStatus::Parked => MemoryOutboxStatus::Failed,
        _ => MemoryOutboxStatus::Pending,
    };
    Ok(MemoryOutboxItem {
        outbox_id: OutboxItemId::from_uuid(ordered.item_id.as_uuid()),
        session_id: env.session_id,
        run_id: env.run_id,
        scope: env.scope,
        idempotency_key: env.idempotency_key,
        operation: env.operation,
        payload_json,
        status,
        attempts: ordered.attempts,
        last_error: ordered.last_error.clone(),
        delivered_event_id: env.delivered_event_id,
        delivered_note_id: env.delivered_note_id,
        correlation_id: ordered.correlation_id.clone(),
        created_at: ordered.created_at,
        updated_at: ordered.updated_at,
    })
}

fn infer_artifact_mime_type(name: &str) -> &'static str {
    let ext = Path::new(name.trim())
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    match ext.as_deref() {
        Some("json") => "application/json",
        Some("txt" | "log") => "text/plain",
        Some("csv") => "text/csv",
        Some("html" | "htm") => "text/html",
        _ => "text/markdown",
    }
}

pub(crate) fn detect_os_account_label_pub() -> Option<String> {
    detect_os_account_label()
}

fn detect_os_account_label() -> Option<String> {
    std::env::var("USERNAME")
        .ok()
        .or_else(|| std::env::var("USER").ok())
        .map(|raw| raw.trim().to_owned())
        .filter(|s| !s.is_empty() && s.len() <= 64)
}

fn generate_project_memory_scope(
    name: &str,
    project_id: ProjectId,
) -> Result<MemoryScope, DennettError> {
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !slug.is_empty() {
            slug.push('-');
            last_dash = true;
        }
        if slug.len() >= 32 {
            break;
        }
    }
    let clean_slug = slug.trim_matches('-');
    let base = if clean_slug.is_empty() {
        "proj"
    } else {
        clean_slug
    };
    let uuid_hex = project_id.as_uuid().simple().to_string();
    let short_id = &uuid_hex[..8];
    MemoryScope::try_new(format!("project:{base}-{short_id}"))
}

/// Maximum workspace-backed file size snapshotted into `artifact_blobs` during `create_artifact` (16 MiB).
const MAX_ARTIFACT_SNAPSHOT_BYTES: usize = 16 * 1024 * 1024;
/// Maximum UTF-8 workspace file size also mirrored into `artifacts.content` for instant UI preview (256 KiB).
const MAX_ARTIFACT_INLINE_PREVIEW_BYTES: usize = 256 * 1024;

/// Reads a regular (non-symlink, non-reparse-point) workspace file in a single pass (zero TOCTOU)
/// up to `max_bytes` and returns `(exact_bytes, sha256_of_exact_bytes)`.
fn read_regular_file_snapshot(
    full_path: &Path,
    max_bytes: usize,
    scope_label: &str,
    display_rel: &str,
) -> Result<(Vec<u8>, Sha256Hex), DennettError> {
    let meta = fs::symlink_metadata(full_path).map_err(|err| DennettError::NotFound {
        resource: String::from("workspace_file"),
        id: format!("{display_rel} ({err})"),
    })?;
    if is_symlink_or_reparse_point(&meta) {
        return Err(DennettError::AccessDenied {
            scope: scope_label.to_owned(),
            reason: String::from(
                "workspace artifact target cannot be a symbolic link or NTFS reparse point",
            ),
        });
    }
    if !meta.file_type().is_file() {
        return Err(DennettError::InvalidInput {
            field: String::from("relative_path"),
            reason: String::from("workspace artifact target must be a regular file"),
        });
    }
    let max_bytes_u64 = u64::try_from(max_bytes).unwrap_or(u64::MAX);
    if meta.len() > max_bytes_u64 {
        return Err(DennettError::InvalidInput {
            field: String::from("relative_path"),
            reason: format!(
                "workspace artifact file exceeds maximum snapshot size ({max_bytes} bytes)"
            ),
        });
    }

    let file = fs::File::open(full_path).map_err(|err| DennettError::NotFound {
        resource: String::from("workspace_file"),
        id: format!("{display_rel} ({err})"),
    })?;
    let opened_meta = file.metadata().map_err(|err| DennettError::Internal {
        detail: format!("failed inspecting workspace artifact `{display_rel}`: {err}"),
    })?;
    if !opened_meta.file_type().is_file() {
        return Err(DennettError::InvalidInput {
            field: String::from("relative_path"),
            reason: String::from("workspace artifact target must be a regular file"),
        });
    }

    let initial_cap = usize::try_from(opened_meta.len().min(max_bytes_u64)).unwrap_or(0);
    let mut bytes = Vec::with_capacity(initial_cap);
    let mut limited = file.take(max_bytes_u64.saturating_add(1));
    limited
        .read_to_end(&mut bytes)
        .map_err(|err| DennettError::Internal {
            detail: format!("failed reading workspace artifact `{display_rel}`: {err}"),
        })?;
    if bytes.len() > max_bytes {
        return Err(DennettError::InvalidInput {
            field: String::from("relative_path"),
            reason: format!(
                "workspace artifact file grew beyond maximum snapshot size ({max_bytes} bytes)"
            ),
        });
    }

    let digest = sha256_hex(&bytes);
    Ok((bytes, digest))
}

fn is_default_session_title(title: &str) -> bool {
    let trimmed = title.trim();
    matches!(
        trimmed.to_lowercase().as_str(),
        "новый чат" | "новый разговор" | "первый разговор" | "untitled" | "new chat"
    )
}

pub(crate) fn derive_auto_session_title(content: &str) -> Option<String> {
    const MAX_CHARS: usize = 52;

    let first_line = content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    let stripped = first_line.trim_start_matches(['#', '*', '-', '`', '>', '•', ' ']);
    let collapsed = stripped.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }

    let char_count = collapsed.chars().count();
    if char_count <= MAX_CHARS {
        return Some(collapsed);
    }

    let prefix: String = collapsed.chars().take(MAX_CHARS).collect();
    let cut = prefix
        .rfind(' ')
        .filter(|&idx| idx >= 16)
        .map_or(prefix.as_str(), |idx| &prefix[..idx]);
    Some(format!("{}…", cut.trim_end()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_title_cleans_markdown_and_truncates_utf8_cleanly() {
        assert!(is_default_session_title("Новый чат"));
        assert!(is_default_session_title("  новый разговор  "));
        assert!(!is_default_session_title("Моё кастомное название"));

        assert_eq!(
            derive_auto_session_title("\n\n  ##   План   релизной  проверки\nдетали ниже"),
            Some(String::from("План релизной проверки"))
        );

        let long_msg = "Спроектируй новую архитектуру кэширования запросов к PostgreSQL с автоматическим сбросом устаревших записей";
        let title = derive_auto_session_title(long_msg).expect("derived title");
        assert!(title.ends_with('…'));
        assert!(title.chars().count() <= 53);
    }

    #[test]
    fn generate_project_memory_scope_handles_cyrillic_and_ascii_names() {
        let pid = ProjectId::new();
        let ascii_scope = generate_project_memory_scope("Dennett Core!", pid).expect("ascii scope");
        assert!(ascii_scope.as_str().starts_with("project:dennett-core-"));

        let cyr_scope = generate_project_memory_scope("Новый проект", pid).expect("cyr scope");
        assert!(cyr_scope.as_str().starts_with("project:proj-"));
    }

    #[test]
    fn read_regular_file_snapshot_rejects_oversized_and_hashes_exact_bytes() {
        let dir = std::env::temp_dir().join(format!(
            "dennett-art-snap-unit-{}",
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        let file_path = dir.join("sample.bin");
        let payload = vec![0x00, 0xFF, 0xFE, b'O', b'K', 0xAB];
        fs::write(&file_path, &payload).expect("write payload");

        let (bytes, digest) =
            read_regular_file_snapshot(&file_path, 16, "project:test", "sample.bin")
                .expect("read within limit");
        assert_eq!(bytes, payload);
        assert_eq!(digest, sha256_hex(&payload));

        let err = read_regular_file_snapshot(&file_path, 4, "project:test", "sample.bin");
        assert!(
            matches!(err, Err(DennettError::InvalidInput { ref field, .. }) if field == "relative_path"),
            "oversized snapshot must fail closed with InvalidInput, got: {err:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
