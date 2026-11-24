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
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod};
use dennett_contracts::redact::{REDACTED_PLACEHOLDER, redact_secrets_owned};
use dennett_contracts::{
    AgentConfigHistoryResponse, AgentConfigVersionId, AgentId, AgentRecord, ArtifactCompleteness,
    ArtifactId, ArtifactLifecycleState, ArtifactRecord, CancelRunResponse, CommitMemoryNoteRequest,
    CommitMemoryNoteResponse, CorrelationId, CreateAgentRequest, CreateArtifactRequest,
    CreateNoteRequest, CreatePolicyRuleRequest, CreateProjectRequest, CreateSessionRequest,
    DennettError, DiagnosticsResponse, DispatchEffectRequest, EffectClaimRecord,
    ExternalEffectStatus, FlushOutboxResponse, HeadHealth, HeadMemorySearchRequest,
    HeadMemorySearchResponse, IdempotencyKey, ListAgentsResponse, ListArtifactsResponse,
    ListOutboxResponse, ListPolicyRulesResponse, ListProjectFilesResponse, ListProjectsResponse,
    ListSessionsResponse, MemoryCapability, MemoryOutboxItem, MemoryOutboxStatus, MemoryScope,
    MemoryStatus, MessageId, MessageRecord, MessageRole, OutboxItemId, PermissionDecisionRecord,
    PermissionRequestId, PolicyRule, PolicyRuleId, PrincipalId, ProfileMode, ProjectId,
    ProjectLifecycleState, ProjectRecord, RunId, RunRecord, RunStatus, RuntimeAuthProfile,
    RuntimeBinding, RuntimeBindingId, RuntimeCatalogResponse, SearchNotesRequest,
    SendMessageRequest, SendMessageResponse, ServiceKind, SessionDetailResponse, SessionId,
    SessionRecord, SessionStatus, Sha256Hex, SimEffectBehavior, TaskId, TimestampMs,
    UpdateAgentConfigRequest, UpdateProjectRequest, UpdateSessionRequest, WorkspaceWritePolicy,
};
use dennett_memory_client::MemoryClient;
use dennett_runtime_antigravity::{
    AgentRuntimePort, RuntimeEvent, RuntimeMcpEnvEntry, RuntimeMcpServerSpec, SessionStartRequest,
    TurnRequest,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, Notify, oneshot, watch};
use tokio::time;
use tokio_postgres::NoTls;

use crate::approvals::{
    ApprovalResolutionSignal, CapturedPermissionDecision, DEFAULT_APPROVAL_WAIT_TIMEOUT,
    InteractivePolicyEvaluator,
};
use crate::effects::{
    ExternalEffectPort, SIMULATED_EXTERNAL_CONNECTOR_ID, SimulatedExternalConnector,
};
use crate::events::{
    ControlEventAppend, PendingControlEvent, append_control_event_in_tx,
    append_control_events_batch_in_tx,
};
use crate::memory_auth::MemoryTokenSigner;
use crate::migrations::{query_control_schema_version, run_control_migrations, sha256_hex};
use crate::pg_target::ValidatedControlPgTarget;
use crate::policy::{PolicyContext, resolve_within_workspace};
use crate::store::{
    AGENT_JOIN_SELECT, AGENT_MESSAGE_SELECT_COLS, EFFECT_CLAIM_SELECT_COLS, PROJECT_SELECT_COLS,
    RUN_SELECT_COLS, append_audit_event, decode_agent_join_row, decode_agent_message_row,
    decode_audit_event_row, decode_config_version_row, decode_effect_claim_row, decode_message_row,
    decode_outbox_row, decode_permission_decision_row, decode_policy_rule_row, decode_project_row,
    decode_run_row, decode_runtime_binding_row, decode_session_row, fetch_agent,
    fetch_all_runtime_bindings, fetch_policy_rules, fetch_project, fetch_runtime_binding,
    fetch_session, fetch_session_artifacts, fetch_session_messages,
    fetch_session_permission_decisions, fetch_session_runs, is_pg_unique_violation, map_pg_err,
    next_stream_seq, truncate_utf8,
};
use crate::workspace_files::{
    clean_display_path, is_symlink_or_reparse_point, list_project_workspace_files,
    validate_safe_workspace_root,
};

/// Current implementation milestone label advertised in [`HeadHealth`].
pub const HEAD_STAGE_LABEL: &str = "D2";
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

/// Optional extra configuration for [`HeadService::new_with_extras`].
#[derive(Clone, Default)]
pub struct HeadServiceExtras {
    /// Optional HMAC-SHA256 signer for dynamically issuing scoped `dnt1.` memory tokens per project.
    pub memory_hmac_signer: Option<MemoryTokenSigner>,
    /// Root directory under which isolated scratch `cwd` folders for folderless projects are created.
    pub detached_projects_root: Option<PathBuf>,
    /// Optional dedicated runtime configured for background tasks (`gemini-api-key`, never silently
    /// substituted for `oauth-personal` or vice versa).
    pub background_runtime: Option<Arc<dyn AgentRuntimePort>>,
    /// Optional explicit [`ExternalEffectPort`] connector (defaults to [`SimulatedExternalConnector`]).
    pub effect_port: Option<Arc<dyn ExternalEffectPort>>,
    /// Optional JSON ledger path backing the default [`SimulatedExternalConnector`] across restarts.
    pub effects_ledger_path: Option<PathBuf>,
}

impl fmt::Debug for HeadServiceExtras {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HeadServiceExtras")
            .field("memory_hmac_signer", &self.memory_hmac_signer.is_some())
            .field("detached_projects_root", &self.detached_projects_root)
            .field("background_runtime", &self.background_runtime.is_some())
            .field("effect_port", &self.effect_port.is_some())
            .field("effects_ledger_path", &self.effects_ledger_path)
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

pub(crate) struct ActiveRunHandle {
    pub(crate) run_id: RunId,
    pub(crate) cancel_tx: watch::Sender<bool>,
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

type InFlightClientMessageMap = Arc<Mutex<HashMap<(SessionId, String), Arc<Notify>>>>;

struct InFlightClientMessageGuard {
    map: InFlightClientMessageMap,
    key: (SessionId, String),
    notify: Arc<Notify>,
}

impl Drop for InFlightClientMessageGuard {
    fn drop(&mut self) {
        let map = Arc::clone(&self.map);
        let key = self.key.clone();
        let notify = Arc::clone(&self.notify);
        if let Ok(mut guard) = map.try_lock() {
            guard.remove(&key);
            notify.notify_waiters();
            return;
        }
        tokio::spawn(async move {
            let mut guard = map.lock().await;
            guard.remove(&key);
            notify.notify_waiters();
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
    effect_port: Arc<dyn ExternalEffectPort>,
    memory_client: Option<MemoryClient>,
    memory_mcp: Option<MemoryMcpWiring>,
    memory_hmac_signer: Option<MemoryTokenSigner>,
    detached_projects_root: PathBuf,
    runtime_catalog_cache: Mutex<Option<(time::Instant, RuntimeCatalogResponse)>>,
    last_memory_status: AtomicU8,
    active_runs: Arc<Mutex<HashMap<SessionId, ActiveRunHandle>>>,
    active_tasks: Arc<Mutex<HashMap<TaskId, ActiveTaskHandle>>>,
    stopped_tasks: Arc<Mutex<HashSet<TaskId>>>,
    pending_approvals:
        Arc<Mutex<HashMap<PermissionRequestId, oneshot::Sender<ApprovalResolutionSignal>>>>,
    session_locks: Arc<Mutex<HashMap<SessionId, Arc<Mutex<()>>>>>,
    in_flight_client_messages: InFlightClientMessageMap,
    profile_mode: ProfileMode,
    secrets_to_redact: Vec<String>,
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
        let effect_port: Arc<dyn ExternalEffectPort> = if let Some(custom_port) = extras.effect_port
        {
            custom_port
        } else if let Some(ledger_path) = extras.effects_ledger_path {
            Arc::new(SimulatedExternalConnector::with_ledger_file(ledger_path))
        } else {
            Arc::new(SimulatedExternalConnector::in_memory())
        };

        Ok(Self {
            inner: Arc::new(HeadServiceInner {
                pg_target,
                pool,
                migrated_version: Mutex::new(None),
                startup_recovery_done: Mutex::new(false),
                runtime,
                background_runtime: extras.background_runtime,
                effect_port,
                memory_client,
                memory_mcp,
                memory_hmac_signer: extras.memory_hmac_signer,
                detached_projects_root,
                runtime_catalog_cache: Mutex::new(None),
                last_memory_status: AtomicU8::new(initial_memory_status),
                active_runs: Arc::new(Mutex::new(HashMap::new())),
                active_tasks: Arc::new(Mutex::new(HashMap::new())),
                stopped_tasks: Arc::new(Mutex::new(HashSet::new())),
                pending_approvals: Arc::new(Mutex::new(HashMap::new())),
                session_locks: Arc::new(Mutex::new(HashMap::new())),
                in_flight_client_messages: Arc::new(Mutex::new(HashMap::new())),
                profile_mode,
                secrets_to_redact: secrets,
            }),
        })
    }

    /// Probes the active runtime dynamically (`agy acp` or `SimulatedRuntime`) and returns the
    /// cached or freshly refreshed [`RuntimeCatalogResponse`] (`GET /v1/runtime/catalog`).
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

        let descriptor = self.inner.runtime.probe().await?;
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

    /// Redacts configured secrets and generic credential patterns from `text`.
    #[must_use]
    pub fn redact(&self, text: &str) -> String {
        redact_secrets_owned(text, &self.inner.secrets_to_redact)
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

            let pending_outbox_count: i64 = if schema_version.is_some() {
                conn.query_one(
                    "SELECT COUNT(*)::bigint FROM memory_commit_outbox WHERE status = 'pending'",
                    &[],
                )
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?
                .get(0)
            } else {
                0
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
            capabilities: d2_head_capabilities(),
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
                workspace_write_policy, default_budget, attached_skills
             ) VALUES ($1, $2, 1, $3, $4, $5, $6, $7, $8, $9)",
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

        tx.execute(
            "INSERT INTO agent_config_versions (
                config_version_id, agent_id, version_number, system_prompt,
                model_preference, thinking_level, allowed_memory_scopes,
                workspace_write_policy, default_budget, attached_skills
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
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
                        TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint
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
        if existing.lifecycle_state == ProjectLifecycleState::DeletedRecord {
            return Err(DennettError::Conflict {
                resource: String::from("project"),
                expected_revision: None,
                actual_revision: None,
                reason: String::from("cannot update a deleted project record"),
            });
        }
        let next_name = req
            .name
            .as_deref()
            .map_or(existing.name.as_str(), str::trim)
            .to_owned();
        let next_workspace = validated_workspace.or(existing.workspace_path);

        let update_sql = format!(
            "UPDATE projects
             SET name = $2,
                 workspace_path = $3,
                 updated_at = now()
             WHERE project_id = $1
             RETURNING {PROJECT_SELECT_COLS}"
        );
        let row = tx
            .query_one(
                &update_sql,
                &[&project_id.as_uuid(), &next_name, &next_workspace],
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
            "project_updated",
            &format!(
                "updated project `{next_name}` ({project_id}) bound={}",
                next_workspace.is_some()
            ),
        )
        .await?;

        let record = decode_project_row(&row)?;
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
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
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
                 ORDER BY created_at DESC"
            )
        } else {
            format!(
                "SELECT {PROJECT_SELECT_COLS}
                 FROM projects
                 WHERE lifecycle_state NOT IN ('archived', 'deleted_record')
                 ORDER BY created_at DESC"
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
        let project = fetch_project(&tx, req.project_id, &self.inner.pg_target).await?;
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

        let session_id = SessionId::new();
        let row = tx
            .query_one(
                "INSERT INTO sessions (session_id, agent_id, project_id, title, status)
                 VALUES ($1, $2, $3, $4, 'active')
                 RETURNING session_id, agent_id, project_id, title, status,
                           TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
                           TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint",
                &[
                    &session_id.as_uuid(),
                    &req.agent_id.as_uuid(),
                    &req.project_id.as_uuid(),
                    &req.title.trim(),
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
            &format!("created session `{}` ({session_id})", req.title.trim()),
        )
        .await?;

        let record = decode_session_row(&row, None)?;
        let _ = append_control_event_in_tx(
            &tx,
            &self.inner.pg_target,
            ControlEventAppend {
                entity_key: &format!("session:{session_id}"),
                event_type: "session.created",
                project_id: Some(req.project_id),
                session_id: Some(session_id),
                task_id: None,
                correlation_id,
                payload: json!({
                    "session_id": session_id,
                    "project_id": req.project_id,
                    "agent_id": req.agent_id,
                    "title": record.title,
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
        Ok(record)
    }

    /// Renames an existing conversation session (`PATCH /v1/sessions/{session_id}`).
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

        let trimmed_title = req.title.trim();
        let row = tx
            .query_opt(
                "UPDATE sessions
                 SET title = $2,
                     updated_at = now()
                 WHERE session_id = $1
                 RETURNING session_id, agent_id, project_id, title, status,
                           TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
                           TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint",
                &[&session_id.as_uuid(), &trimmed_title],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let Some(row) = row else {
            return Err(DennettError::NotFound {
                resource: String::from("session"),
                id: session_id.to_string(),
            });
        };

        let _ = append_audit_event(
            &tx,
            &self.inner.pg_target,
            correlation_id,
            Some(session_id),
            None,
            "head",
            "session_renamed",
            &format!("renamed session {session_id} to `{trimmed_title}`"),
        )
        .await?;

        let binding = fetch_runtime_binding(&tx, session_id, &self.inner.pg_target).await?;
        let record = decode_session_row(&row, binding)?;
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
        let rows = conn
            .query(
                "SELECT session_id, agent_id, project_id, title, status,
                        TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
                        TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint
                 FROM sessions
                 ORDER BY updated_at DESC",
                &[],
            )
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

        Ok(SessionDetailResponse {
            session,
            messages,
            artifacts,
            runs,
            permission_decisions,
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

        // Fetch session + project metadata first and release the pooled connection BEFORE any
        // blocking filesystem validation or streaming SHA-256 disk I/O.
        let (project_id, workspace_path) = {
            let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
            let session = fetch_session(&conn, session_id, &self.inner.pg_target).await?;
            let project = fetch_project(&conn, session.project_id, &self.inner.pg_target).await?;
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
            (project.project_id, project.workspace_path)
        };

        let (normalized_rel_path, digest) = if let Some(ref rel) = req.relative_path {
            let Some(ws_path) = workspace_path else {
                return Err(DennettError::InvalidInput {
                    field: String::from("relative_path"),
                    reason: String::from(
                        "cannot attach workspace-relative file artifact to a project without a bound workspace folder",
                    ),
                });
            };
            let rel_owned = rel.clone();
            let inline_owned = req.content.clone();
            let scope_label = format!("project:{project_id}");
            let (safe_rel, file_digest) = tokio::task::spawn_blocking(move || {
                let ws_root = PathBuf::from(&ws_path);
                let safe_rel =
                    resolve_within_workspace(&ws_root, &rel_owned).map_err(|reason| {
                        DennettError::AccessDenied {
                            scope: scope_label.clone(),
                            reason,
                        }
                    })?;
                let computed = if let Some(inline) = inline_owned {
                    sha256_hex(inline.as_bytes())
                } else {
                    let full_path = ws_root.join(&safe_rel);
                    stream_sha256_regular_file(
                        &full_path,
                        MAX_WORKSPACE_ARTIFACT_HASH_BYTES,
                        &scope_label,
                        &safe_rel,
                    )?
                };
                Ok::<_, DennettError>((safe_rel, computed))
            })
            .await
            .map_err(|err| DennettError::Internal {
                detail: format!("artifact hashing task failed: {err}"),
            })??;
            (Some(safe_rel), file_digest)
        } else {
            let computed = sha256_hex(req.content.as_deref().unwrap_or_default().as_bytes());
            (None, computed)
        };

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

        let row = tx
            .query_one(
                "INSERT INTO artifacts (
                    artifact_id, session_id, project_id, run_id, name, mime_type,
                    revision, content, relative_path, sha256,
                    completeness_status, lifecycle_state
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 'active')
                 RETURNING TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint",
                &[
                    &artifact_id.as_uuid(),
                    &session_id.as_uuid(),
                    &project_id.as_uuid(),
                    &req.run_id.map(RunId::as_uuid),
                    &req.name.trim(),
                    &req.mime_type.trim(),
                    &revision,
                    &req.content,
                    &normalized_rel_path,
                    &digest.as_str(),
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
                project_id: Some(project_id),
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
            content: req.content,
            relative_path: normalized_rel_path,
            sha256: digest,
            completeness_status: req.completeness_status,
            lifecycle_state: ArtifactLifecycleState::Active,
            created_at: TimestampMs::from_millis(row.get(0)),
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

    /// Persists a user message BEFORE invoking the runtime, starts or resumes the runtime session
    /// with scoped `dennett-memory-mcp`, executes one turn with external [`crate::policy::PolicyEngine`] callbacks
    /// (including live interactive `AskBeforeWrite` approvals) and live cancellation support, and
    /// persists all decisions, audit traces, and outcomes.
    ///
    /// Supports optional `client_message_id` idempotency (`UNIQUE (session_id, client_message_id)`):
    /// - Replaying the same `client_message_id` with identical content returns the persisted
    ///   [`SendMessageResponse`] with `deduplicated = true` without invoking the runtime again.
    /// - Reusing `client_message_id` with different content fails closed with `409 IdempotencyMismatch`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError`] on validation, missing session, idempotency conflict, concurrent
    /// active run conflict, auth failure, runtime error, or database unavailability.
    #[expect(
        clippy::too_many_lines,
        reason = "send_message orchestrates client idempotency deduplication, pre-run persistence, interactive policy evaluation, runtime turn execution, and post-run atomic recording without holding SQL locks across runtime I/O"
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

        // 0. Client-side retry idempotency (`client_message_id`): deduplicate both completed
        //    and concurrently in-flight retries before touching active_runs or runtime!
        let mut _in_flight_cmid_guard: Option<InFlightClientMessageGuard> = None;
        if let Some(ref cmid) = client_message_id {
            loop {
                let existing_notify = {
                    let mut map = self.inner.in_flight_client_messages.lock().await;
                    let key = (session_id, cmid.clone());
                    if let Some(notify) = map.get(&key) {
                        Some(Arc::clone(notify))
                    } else {
                        let notify = Arc::new(Notify::new());
                        map.insert(key.clone(), Arc::clone(&notify));
                        _in_flight_cmid_guard = Some(InFlightClientMessageGuard {
                            map: Arc::clone(&self.inner.in_flight_client_messages),
                            key,
                            notify,
                        });
                        None
                    }
                };

                if let Some(notify) = existing_notify {
                    let _ = time::timeout(turn_timeout, notify.notified()).await;
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
            let project = fetch_project(&conn, session.project_id, &self.inner.pg_target).await?;
            if matches!(
                project.lifecycle_state,
                ProjectLifecycleState::Archived | ProjectLifecycleState::DeletedRecord
            ) {
                return Err(DennettError::Conflict {
                    resource: String::from("project"),
                    expected_revision: None,
                    actual_revision: None,
                    reason: format!(
                        "project `{}` is `{}` and does not accept new messages",
                        project.project_id, project.lifecycle_state
                    ),
                });
            }
            let binding = fetch_runtime_binding(&conn, session_id, &self.inner.pg_target).await?;
            let history = fetch_session_messages(&conn, session_id, &self.inner.pg_target).await?;
            let rules = fetch_policy_rules(&conn, &self.inner.pg_target).await?;
            (session, agent, project, binding, history, rules)
        };

        // 2. Acquire unified per-session execution lock + active run slot (H2-05: serializes
        //    `send_message` and `execute_task_generation` so neither can clobber `active_runs`).
        let session_lock = self.session_exec_lock(session_id).await;
        let _session_guard = session_lock
            .try_lock_owned()
            .map_err(|_| DennettError::Conflict {
                resource: String::from("session_run"),
                expected_revision: None,
                actual_revision: None,
                reason: String::from("another run is already active in this session"),
            })?;

        let run_id = RunId::new();
        let (cancel_tx, cancel_rx) = watch::channel(false);
        self.try_register_active_session_run(
            session_id,
            ActiveRunHandle {
                run_id,
                cancel_tx: cancel_tx.clone(),
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

        // 3. Persist user message + running RunRecord + claim undelivered inter-agent messages
        //    BEFORE touching runtime (Architecture 80 §11).
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
                    let agent_msg_sql = format!(
                        "SELECT {AGENT_MESSAGE_SELECT_COLS}
                         FROM agent_messages
                         WHERE recipient_agent_id = $1
                           AND project_id = $2
                           AND delivered_in_run_id IS NULL
                         ORDER BY created_at ASC
                         FOR UPDATE"
                    );
                    let agent_msg_rows = tx
                        .query(
                            &agent_msg_sql,
                            &[&agent.agent_id.as_uuid(), &project.project_id.as_uuid()],
                        )
                        .await
                        .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
                    let mut inbox_msgs = Vec::with_capacity(agent_msg_rows.len());
                    for row in &agent_msg_rows {
                        inbox_msgs.push(decode_agent_message_row(row)?);
                    }
                    if !inbox_msgs.is_empty() {
                        let ids: Vec<uuid::Uuid> = inbox_msgs
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
                            self.inner.runtime.adapter_id()
                        ),
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
                Err(pg_err) => return Err(map_pg_err(&self.inner.pg_target, &pg_err)),
            }
        };

        let (user_message, inbox_agent_messages) = match pre_run_res {
            Ok(pair) => pair,
            Err(replay_res) => {
                active_guard.mark_completed();
                self.unregister_active_session_run(session_id, run_id).await;
                return replay_res;
            }
        };

        // 4. Probe memoryd status (never blocks or aborts the control turn when memoryd is down!).
        let (memory_status, _mem_detail) = self.probe_memory_status(&correlation_id).await;

        // Resolve effective workspace directory & write policy: folderless projects get an
        // isolated scratch directory and mandatory `DenyAll` write policy until a real folder is bound.
        let (effective_workspace_cwd, effective_write_policy) = match project.workspace_path {
            Some(ref ws_path) => (
                PathBuf::from(ws_path),
                agent.current_config.workspace_write_policy.clone(),
            ),
            None => (
                self.ensure_detached_project_dir(project.project_id)?,
                WorkspaceWritePolicy::DenyAll,
            ),
        };

        // 5. Start or explicitly resume runtime session (with dynamically scoped `dennett-memory-mcp`).
        let mcp_servers = self.build_memory_mcp_specs(&agent);
        let turn_secrets = self.collect_turn_secrets(&mcp_servers);
        let started_instant = Instant::now();
        let start_outcome = match self
            .inner
            .runtime
            .start_or_resume_session(SessionStartRequest {
                dennett_session_id: session_id,
                workspace_cwd: effective_workspace_cwd.clone(),
                system_prompt: agent.current_config.system_prompt.clone(),
                model_preference: agent.current_config.model_preference.clone(),
                thinking_level: agent.current_config.thinking_level,
                workspace_write_policy: effective_write_policy.clone(),
                mcp_servers,
                existing_binding,
                history_for_rehydrate: prior_history,
                allow_rehydrate: true,
            })
            .await
        {
            Ok(outcome) => outcome,
            Err(err) => {
                self.record_failed_run(session_id, run_id, &correlation_id, &err, &[])
                    .await;
                active_guard.mark_completed();
                self.unregister_active_session_run(session_id, run_id).await;
                return Err(err);
            }
        };

        let runtime_binding = self
            .upsert_runtime_binding(session_id, &start_outcome, &correlation_id, Some(run_id))
            .await?;

        // 6. Execute turn with external InteractivePolicyEvaluator + live cancellation handle.
        let policy_ctx = PolicyContext {
            project_id: project.project_id,
            agent_id: agent.agent_id,
            session_id: session.session_id,
            task_id: None,
            workspace_root: effective_workspace_cwd,
            workspace_write_policy: effective_write_policy,
            allowed_memory_scopes: agent.current_config.allowed_memory_scopes.clone(),
            rules: policy_rules,
        };
        // Align interactive approval wait with turn_timeout so `await_interactive_approval`
        // cleanly expires before `transport.rs`'s turn deadline fires (H2-01).
        let approval_timeout = turn_timeout
            .saturating_sub(Duration::from_secs(2))
            .max(Duration::from_millis(250))
            .min(DEFAULT_APPROVAL_WAIT_TIMEOUT);
        let (interactive_policy, decisions_buf) = InteractivePolicyEvaluator::new(
            self.clone(),
            policy_ctx,
            None,
            correlation_id.clone(),
            turn_secrets,
            cancel_rx.clone(),
            approval_timeout,
        );

        let effective_prompt = if inbox_agent_messages.is_empty() {
            req.content.clone()
        } else {
            let mut combined = String::from("[Messages from other agents]\n");
            for msg in &inbox_agent_messages {
                let sender_label = msg
                    .sender_agent_id
                    .map_or_else(|| String::from("user"), |id| id.to_string());
                let _ = writeln!(
                    combined,
                    "- from {sender_label}: {}",
                    self.redact(&msg.body_text)
                );
            }
            combined.push('\n');
            combined.push_str(&req.content);
            combined
        };

        let turn_result = self
            .inner
            .runtime
            .send_prompt_turn(
                TurnRequest {
                    dennett_session_id: session_id,
                    provider_session_id: start_outcome.provider_session_id.clone(),
                    run_id,
                    prompt_text: effective_prompt,
                    timeout: turn_timeout,
                    cancel_rx: cancel_rx.clone(),
                },
                Arc::new(interactive_policy),
                None,
            )
            .await;

        let elapsed_ms = u64::try_from(started_instant.elapsed().as_millis()).unwrap_or(u64::MAX);
        let captured_decisions: Vec<CapturedPermissionDecision> =
            decisions_buf.lock().map_or_default(|g| g.clone());

        let turn_outcome = match turn_result {
            Ok(outcome) => outcome,
            Err(err) => {
                self.record_failed_run(
                    session_id,
                    run_id,
                    &correlation_id,
                    &err,
                    &captured_decisions,
                )
                .await;
                active_guard.mark_completed();
                self.unregister_active_session_run(session_id, run_id).await;
                return Err(err);
            }
        };

        // Process any partial artifacts or external effects requested during the turn.
        for ev in &turn_outcome.events {
            match ev {
                RuntimeEvent::PartialArtifactEmitted { name, content } => {
                    let _ = self
                        .create_artifact(
                            session_id,
                            CreateArtifactRequest {
                                name: name.clone(),
                                mime_type: String::from("text/plain"),
                                content: Some(content.clone()),
                                relative_path: None,
                                run_id: Some(run_id),
                                completeness_status: ArtifactCompleteness::Partial,
                            },
                            &correlation_id,
                        )
                        .await;
                }
                RuntimeEvent::ExternalEffectRequested {
                    recipient,
                    operation,
                    behavior,
                    idempotency_key,
                } => {
                    if *cancel_rx.borrow() {
                        break;
                    }
                    if let Ok(idem) = IdempotencyKey::try_new(idempotency_key.clone()) {
                        let sim_behavior = SimEffectBehavior::from_str(behavior)
                            .unwrap_or(SimEffectBehavior::Success);
                        let _ = self
                            .dispatch_external_effect(
                                DispatchEffectRequest {
                                    project_id: project.project_id,
                                    session_id,
                                    task_id: None,
                                    run_id: Some(run_id),
                                    agent_id: agent.agent_id,
                                    connector_id: Some(String::from(
                                        SIMULATED_EXTERNAL_CONNECTOR_ID,
                                    )),
                                    operation: operation.clone(),
                                    target_recipient: recipient.clone(),
                                    parameters_json: json!({
                                        "session_id": session_id,
                                        "run_id": run_id,
                                    })
                                    .to_string(),
                                    effect_idempotency_key: idem,
                                    simulated_behavior: Some(sim_behavior),
                                },
                                &correlation_id,
                            )
                            .await;
                    }
                }
                _ => {}
            }
        }

        // 7. Atomically persist decisions, tool audit trace, assistant reply, and terminal RunRecord.
        let (assistant_message, run_record) = self
            .finalize_turn(
                session_id,
                project.project_id,
                run_id,
                &agent,
                user_message.message_id,
                start_outcome.rehydrated,
                &turn_outcome,
                elapsed_ms,
                &captured_decisions,
                &correlation_id,
            )
            .await?;
        active_guard.mark_completed();
        self.unregister_active_session_run(session_id, run_id).await;

        let artifacts = {
            let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
            fetch_session_artifacts(&conn, session_id, &self.inner.pg_target).await?
        };

        Ok(SendMessageResponse {
            user_message,
            assistant_message,
            run: run_record,
            runtime_binding,
            artifacts,
            permission_decisions: captured_decisions.into_iter().map(|c| c.record).collect(),
            memory_status,
            session_title: effective_session_title,
            deduplicated: false,
        })
    }

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
        reason = "signals live cancellation, withdraws pending approvals, transitions dispatching effects to unknown, and reports honest external outcome uncertainty without calling memoryd"
    )]
    pub async fn cancel_session_run(
        &self,
        session_id: SessionId,
        correlation_id: &CorrelationId,
    ) -> Result<CancelRunResponse, DennettError> {
        let _ = self.ensure_migrated().await?;

        // Signal cancellation immediately BEFORE touching PostgreSQL or anything else!
        let active_run_id = {
            let active = self.inner.active_runs.lock().await;
            active.get(&session_id).map(|h| {
                let _ = h.cancel_tx.send(true);
                h.run_id
            })
        };
        let runtime_cancelled = self
            .inner
            .runtime
            .cancel_turn(session_id)
            .await
            .unwrap_or(false);

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

        let status = if cancelled {
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
            project_id: Some(session.project_id),
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

    /// Durable memory commit path (V02): records intent into `memory_commit_outbox` inside
    /// `dennett_control` FIRST, then attempts immediate delivery to `dennett-memoryd`.
    ///
    /// If `dennett-memoryd` is unreachable, the item stays durably `pending` in `dennett_control`
    /// with its deterministic `idempotency_key` until [`Self::flush_memory_outbox`] succeeds!
    /// Reusing `(scope, idempotency_key)` with a conflicting payload fails closed with
    /// [`DennettError::IdempotencyMismatch`] (`409 Conflict`).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::IdempotencyMismatch`],
    /// [`DennettError::DatabaseUnavailable`] (control PG), or permanent non-transient memoryd
    /// rejections (`ForbiddenScope`, `Unauthorized`, etc.).
    pub async fn commit_memory_note(
        &self,
        req: CommitMemoryNoteRequest,
        correlation_id: &CorrelationId,
    ) -> Result<CommitMemoryNoteResponse, DennettError> {
        let _ = self.ensure_migrated().await?;
        let idempotency_key = req.idempotency_key.unwrap_or_else(IdempotencyKey::generate);

        let create_note_req = CreateNoteRequest {
            scope: req.scope.clone(),
            title: req.title,
            body: req.body,
            tags: req.tags,
            observed_at: None,
            evidence_links: Vec::new(),
            author_label: Some(String::from("dennett-head")),
            idempotency_key: idempotency_key.clone(),
        };
        create_note_req.validate()?;
        let payload_json =
            serde_json::to_string(&create_note_req).map_err(|err| DennettError::Internal {
                detail: format!("failed serializing memory outbox payload: {err}"),
            })?;

        // 1. Race-safe durable insert into `memory_commit_outbox` on `(scope, idempotency_key)`.
        //    If a row already exists, verify payload equality or return `IdempotencyMismatch` (409)!
        let outbox_item = self
            .insert_or_verify_outbox_note(
                req.session_id,
                req.run_id,
                &create_note_req,
                &payload_json,
                correlation_id,
            )
            .await?;

        if outbox_item.status == MemoryOutboxStatus::Delivered {
            return Ok(CommitMemoryNoteResponse {
                outbox_item,
                delivered_immediately: true,
                memory_status: self.cached_memory_status(),
            });
        }

        // 2. Attempt immediate delivery to standalone `dennett-memoryd`.
        let (updated, memory_status) = self
            .deliver_single_outbox_item(outbox_item, correlation_id)
            .await?;

        Ok(CommitMemoryNoteResponse {
            delivered_immediately: updated.status == MemoryOutboxStatus::Delivered,
            outbox_item: updated,
            memory_status,
        })
    }

    async fn insert_or_verify_outbox_note(
        &self,
        session_id: Option<SessionId>,
        run_id: Option<RunId>,
        create_note_req: &CreateNoteRequest,
        payload_json: &str,
        correlation_id: &CorrelationId,
    ) -> Result<MemoryOutboxItem, DennettError> {
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let outbox_id = OutboxItemId::new();
        let inserted = tx
            .query_opt(
                "INSERT INTO memory_commit_outbox (
                    outbox_id, session_id, run_id, scope, idempotency_key,
                    operation, payload_json, status, correlation_id
                 ) VALUES ($1, $2, $3, $4, $5, 'create_note', $6, 'pending', $7)
                 ON CONFLICT (scope, idempotency_key) DO NOTHING
                 RETURNING outbox_id, session_id, run_id, scope, idempotency_key, operation,
                           payload_json, status, attempts, last_error,
                           delivered_event_id, delivered_note_id, correlation_id,
                           TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
                           TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint",
                &[
                    &outbox_id.as_uuid(),
                    &session_id.map(SessionId::as_uuid),
                    &run_id.map(RunId::as_uuid),
                    &create_note_req.scope.as_str(),
                    &create_note_req.idempotency_key.as_str(),
                    &payload_json,
                    &correlation_id.as_str(),
                ],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let item = if let Some(row) = inserted {
            decode_outbox_row(&row)?
        } else {
            let existing_row = tx
                .query_one(
                    "SELECT outbox_id, session_id, run_id, scope, idempotency_key, operation,
                            payload_json, status, attempts, last_error,
                            delivered_event_id, delivered_note_id, correlation_id,
                            TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
                            TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint
                     FROM memory_commit_outbox
                     WHERE scope = $1 AND idempotency_key = $2",
                    &[
                        &create_note_req.scope.as_str(),
                        &create_note_req.idempotency_key.as_str(),
                    ],
                )
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
            let existing = decode_outbox_row(&existing_row)?;
            let existing_payload: CreateNoteRequest = serde_json::from_str(&existing.payload_json)
                .map_err(|err| DennettError::Internal {
                    detail: format!("corrupt existing outbox payload JSON: {err}"),
                })?;
            if existing.operation != "create_note" || existing_payload != *create_note_req {
                return Err(DennettError::IdempotencyMismatch {
                    idempotency_key: create_note_req.idempotency_key.to_string(),
                    reason: format!(
                        "reused idempotency key in scope `{}` with conflicting note payload",
                        create_note_req.scope
                    ),
                });
            }
            existing
        };

        tx.commit()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
        Ok(item)
    }

    /// Flushes all pending `memory_commit_outbox` rows to standalone `dennett-memoryd` idempotently.
    ///
    /// Safe to invoke repeatedly: `memoryd` deduplicates on `(scope, idempotency_key)` with zero
    /// duplicate notes.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::DatabaseUnavailable`] only if `dennett_control` PG fails.
    pub async fn flush_memory_outbox(
        &self,
        correlation_id: &CorrelationId,
    ) -> Result<FlushOutboxResponse, DennettError> {
        let _ = self.ensure_migrated().await?;
        let pending_rows = {
            let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
            let rows = conn
                .query(
                    "SELECT outbox_id, session_id, run_id, scope, idempotency_key, operation,
                            payload_json, status, attempts, last_error,
                            delivered_event_id, delivered_note_id, correlation_id,
                            TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
                            TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint
                     FROM memory_commit_outbox
                     WHERE status = 'pending'
                     ORDER BY created_at ASC",
                    &[],
                )
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
            rows.iter()
                .map(decode_outbox_row)
                .collect::<Result<Vec<_>, _>>()?
        };

        let attempted = pending_rows.len();
        let mut delivered = 0;
        let mut pending = 0;
        let mut failed = 0;
        let mut last_mem_status = self.probe_memory_status(correlation_id).await.0;
        let mut touched = Vec::with_capacity(attempted);

        for item in pending_rows {
            let (updated, mem_status) = self
                .deliver_single_outbox_item(item, correlation_id)
                .await?;
            last_mem_status = mem_status;
            match updated.status {
                MemoryOutboxStatus::Delivered => delivered += 1,
                MemoryOutboxStatus::Pending => pending += 1,
                MemoryOutboxStatus::Failed => failed += 1,
            }
            touched.push(updated);
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

    /// Lists all durable memory outbox items ordered newest first.
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
        let rows = conn
            .query(
                "SELECT outbox_id, session_id, run_id, scope, idempotency_key, operation,
                        payload_json, status, attempts, last_error,
                        delivered_event_id, delivered_note_id, correlation_id,
                        TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
                        TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint
                 FROM memory_commit_outbox
                 ORDER BY created_at DESC",
                &[],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
        let items = rows
            .iter()
            .map(decode_outbox_row)
            .collect::<Result<Vec<_>, _>>()?;
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
            });
        };

        let search_req = SearchNotesRequest {
            scope: req.scope,
            query: req.query,
            limit: req.limit,
        };
        match client.search_notes(&search_req, Some(correlation_id)).await {
            Ok(resp) => {
                self.record_memory_status(MemoryStatus::Healthy);
                Ok(HeadMemorySearchResponse {
                    memory_status: MemoryStatus::Healthy,
                    degraded_reason: None,
                    notes: resp.notes,
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

        let outbox_rows = conn
            .query(
                "SELECT outbox_id, session_id, run_id, scope, idempotency_key, operation,
                        payload_json, status, attempts, last_error,
                        delivered_event_id, delivered_note_id, correlation_id,
                        TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
                        TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint
                 FROM memory_commit_outbox
                 ORDER BY created_at DESC
                 LIMIT $1",
                &[&capped],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        Ok(DiagnosticsResponse {
            events: event_rows
                .iter()
                .map(decode_audit_event_row)
                .collect::<Result<Vec<_>, _>>()?,
            permission_decisions: decision_rows
                .iter()
                .map(decode_permission_decision_row)
                .collect::<Result<Vec<_>, _>>()?,
            outbox_items: outbox_rows
                .iter()
                .map(decode_outbox_row)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }

    async fn deliver_single_outbox_item(
        &self,
        item: MemoryOutboxItem,
        correlation_id: &CorrelationId,
    ) -> Result<(MemoryOutboxItem, MemoryStatus), DennettError> {
        let mut scopes = BTreeSet::new();
        scopes.insert(item.scope.clone());
        let client_opt = self.memory_client_for_scopes(scopes, true)?;
        let Some(client) = client_opt else {
            self.record_memory_status(MemoryStatus::DegradedUnreachable);
            let safe_err = String::from("memoryd client is not configured on dennett-head");
            let updated = self
                .update_outbox_attempt(
                    item.outbox_id,
                    MemoryOutboxStatus::Pending,
                    Some(&safe_err),
                    None,
                    None,
                    correlation_id,
                    item.session_id,
                    item.run_id,
                )
                .await?;
            return Ok((updated, MemoryStatus::DegradedUnreachable));
        };

        let create_req: CreateNoteRequest =
            serde_json::from_str(&item.payload_json).map_err(|err| DennettError::Internal {
                detail: format!("corrupt outbox payload JSON: {err}"),
            })?;

        match client.create_note(&create_req, Some(correlation_id)).await {
            Ok(resp) => {
                self.record_memory_status(MemoryStatus::Healthy);
                let updated = self
                    .update_outbox_attempt(
                        item.outbox_id,
                        MemoryOutboxStatus::Delivered,
                        None,
                        Some(resp.event_id),
                        Some(resp.note.note_id),
                        correlation_id,
                        item.session_id,
                        item.run_id,
                    )
                    .await?;
                Ok((updated, MemoryStatus::Healthy))
            }
            Err(
                DennettError::DatabaseUnavailable { detail }
                | DennettError::Timeout { operation: detail },
            ) => {
                self.record_memory_status(MemoryStatus::DegradedUnreachable);
                let safe_err = self.redact(&detail);
                let updated = self
                    .update_outbox_attempt(
                        item.outbox_id,
                        MemoryOutboxStatus::Pending,
                        Some(&safe_err),
                        None,
                        None,
                        correlation_id,
                        item.session_id,
                        item.run_id,
                    )
                    .await?;
                Ok((updated, MemoryStatus::DegradedUnreachable))
            }
            Err(permanent_err) => {
                self.record_memory_status(MemoryStatus::Healthy);
                let safe_err = self.redact(&permanent_err.to_string());
                let updated = self
                    .update_outbox_attempt(
                        item.outbox_id,
                        MemoryOutboxStatus::Failed,
                        Some(&safe_err),
                        None,
                        None,
                        correlation_id,
                        item.session_id,
                        item.run_id,
                    )
                    .await?;
                Ok((updated, MemoryStatus::Healthy))
            }
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "internal helper persists all outbox delivery columns atomically in one UPDATE"
    )]
    async fn update_outbox_attempt(
        &self,
        outbox_id: OutboxItemId,
        status: MemoryOutboxStatus,
        last_error: Option<&str>,
        delivered_event_id: Option<dennett_contracts::MemoryEventId>,
        delivered_note_id: Option<dennett_contracts::MemoryNoteId>,
        correlation_id: &CorrelationId,
        session_id: Option<SessionId>,
        run_id: Option<RunId>,
    ) -> Result<MemoryOutboxItem, DennettError> {
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let truncated_err = last_error.map(|e| truncate_utf8(e, 2000).to_owned());
        let row = tx
            .query_one(
                "UPDATE memory_commit_outbox
                 SET status = $2,
                     attempts = attempts + 1,
                     last_error = $3,
                     delivered_event_id = COALESCE($4, delivered_event_id),
                     delivered_note_id = COALESCE($5, delivered_note_id),
                     updated_at = now()
                 WHERE outbox_id = $1
                 RETURNING outbox_id, session_id, run_id, scope, idempotency_key, operation,
                           payload_json, status, attempts, last_error,
                           delivered_event_id, delivered_note_id, correlation_id,
                           TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
                           TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint",
                &[
                    &outbox_id.as_uuid(),
                    &status.as_str(),
                    &truncated_err,
                    &delivered_event_id.map(dennett_contracts::MemoryEventId::as_uuid),
                    &delivered_note_id.map(dennett_contracts::MemoryNoteId::as_uuid),
                ],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

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
            &format!("outbox item {outbox_id} transitioned to `{status}`"),
        )
        .await?;

        let item = decode_outbox_row(&row)?;
        tx.commit()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
        Ok(item)
    }

    pub(crate) async fn upsert_runtime_binding(
        &self,
        session_id: SessionId,
        outcome: &dennett_runtime_antigravity::SessionStartOutcome,
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
                    provider_session_id, workspace_cwd, is_simulated, rehydrate_count
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
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
                    updated_at = now()
                 RETURNING binding_id, session_id, adapter_id, auth_profile, tier, agy_version,
                           provider_session_id, workspace_cwd, is_simulated, rehydrate_count,
                           TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
                           TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint",
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
        reason = "atomic turn finalization persists decisions, tool observations, assistant message, terminal run status, and ordered journal event together"
    )]
    async fn finalize_turn(
        &self,
        session_id: SessionId,
        project_id: ProjectId,
        run_id: RunId,
        agent: &AgentRecord,
        user_message_id: MessageId,
        rehydrated: bool,
        outcome: &dennett_runtime_antigravity::TurnOutcome,
        elapsed_ms: u64,
        decisions: &[CapturedPermissionDecision],
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

        let stop_reason = truncate_utf8(&outcome.stop_reason, 128).to_owned();
        let tokens_used = outcome.total_tokens();
        let tokens_i64 = tokens_used.and_then(|v| i64::try_from(v).ok());
        let wall_i64 = i64::try_from(elapsed_ms).ok();
        let run_row = tx
            .query_one(
                "UPDATE runs
                 SET assistant_message_id = $2,
                     status = $3,
                     stop_reason = $4,
                     rehydrated = $5,
                     tokens_used = $6,
                     wall_ms = $7,
                     finished_at = now()
                 WHERE run_id = $1
                 RETURNING TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
                           TRUNC(EXTRACT(EPOCH FROM finished_at) * 1000)::bigint",
                &[
                    &run_id.as_uuid(),
                    &assistant_message.as_ref().map(|m| m.message_id.as_uuid()),
                    &outcome.status.as_str(),
                    &Some(stop_reason.as_str()),
                    &rehydrated,
                    &tokens_i64,
                    &wall_i64,
                ],
            )
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

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
                if outcome.status == RunStatus::Cancelled {
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
            if outcome.status == RunStatus::Cancelled {
                "run_cancelled"
            } else {
                "run_succeeded"
            },
            &format!(
                "run {run_id} finished status={} stop_reason={} decisions={}",
                outcome.status,
                stop_reason,
                decisions.len()
            ),
        )
        .await?;

        pending_events.push(PendingControlEvent {
            entity_key: format!("session:{session_id}"),
            event_type: "session.message_completed",
            project_id: Some(project_id),
            session_id: Some(session_id),
            task_id: None,
            payload: json!({
                "session_id": session_id,
                "run_id": run_id,
                "status": outcome.status,
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
            status: outcome.status,
            stop_reason: Some(stop_reason),
            error_code: None,
            error_summary: None,
            rehydrated,
            task_id: None,
            task_generation: None,
            tokens_used,
            wall_ms: Some(elapsed_ms),
            correlation_id: correlation_id.clone(),
            created_at: TimestampMs::from_millis(run_row.get(0)),
            finished_at: Some(TimestampMs::from_millis(run_row.get(1))),
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
        let safe_summary = self.redact(&err.to_string());
        let truncated = truncate_utf8(&safe_summary, 2000).to_owned();
        let _ = tx
            .execute(
                "UPDATE runs
                 SET status = 'failed',
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

    /// Finalizes a run whose enclosing future was dropped (e.g. HTTP timeout or client disconnect),
    /// ensuring PostgreSQL `runs.status = 'running'` and pending approvals are NEVER stranded (H2-01).
    pub(crate) async fn finalize_abandoned_run(
        &self,
        session_id: SessionId,
        run_id: RunId,
        correlation_id: &CorrelationId,
    ) {
        let _ = self.inner.runtime.cancel_turn(session_id).await;
        let Ok(mut conn) = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await else {
            return;
        };
        let Ok(tx) = conn.transaction().await else {
            return;
        };
        let updated = tx
            .execute(
                "UPDATE runs
                 SET status = 'cancelled',
                     stop_reason = 'client_disconnected_or_timeout',
                     finished_at = now()
                 WHERE run_id = $1 AND status = 'running'",
                &[&run_id.as_uuid()],
            )
            .await
            .unwrap_or(0);
        let mut pending_events = Vec::new();
        let expired_perms = self
            .expire_pending_approvals_for_run_in_tx(
                &tx,
                run_id,
                "withdrawn",
                "Запрос был прерван или истёк тайм-аут ожидания ответа",
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
                &format!("run {run_id} abandoned (HTTP disconnect or timeout)"),
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
                "abandoned due to HTTP disconnect or timeout",
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

    fn memory_client_for_scopes(
        &self,
        scopes: BTreeSet<MemoryScope>,
        allow_write: bool,
    ) -> Result<Option<MemoryClient>, DennettError> {
        if let Some(ref signer) = self.inner.memory_hmac_signer
            && let Some(base_url) = self.memory_base_url()
            && !scopes.is_empty()
        {
            let mut caps = BTreeSet::new();
            caps.insert(MemoryCapability::Read);
            if allow_write {
                caps.insert(MemoryCapability::Write);
            }
            let principal = PrincipalId::try_new("dennett-head")?;
            let token = signer.mint_scoped_token(principal, scopes, caps)?;
            return Ok(Some(MemoryClient::new(base_url, token)?));
        }
        Ok(self.inner.memory_client.clone())
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

    pub(crate) fn pg_target(&self) -> &ValidatedControlPgTarget {
        &self.inner.pg_target
    }

    pub(crate) fn runtime(&self) -> &Arc<dyn AgentRuntimePort> {
        &self.inner.runtime
    }

    pub(crate) fn runtime_for_profile(
        &self,
        override_profile: Option<RuntimeAuthProfile>,
    ) -> Result<Arc<dyn AgentRuntimePort>, DennettError> {
        let Some(requested) = override_profile else {
            return Ok(Arc::clone(&self.inner.runtime));
        };
        if self.inner.runtime.auth_profile() == requested {
            return Ok(Arc::clone(&self.inner.runtime));
        }
        if let Some(ref bg) = self.inner.background_runtime
            && bg.auth_profile() == requested
        {
            return Ok(Arc::clone(bg));
        }
        Err(DennettError::AuthRequired {
            profile: requested,
            detail: format!(
                "runtime auth profile `{}` is not configured on dennett-head; refusing silent fallback to `{}`",
                requested.as_method_id(),
                self.inner.runtime.auth_profile().as_method_id(),
            ),
        })
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

    pub(crate) fn build_memory_mcp_specs(&self, agent: &AgentRecord) -> Vec<RuntimeMcpServerSpec> {
        let Some(ref mcp) = self.inner.memory_mcp else {
            return Vec::new();
        };

        // When dynamic HMAC signing is configured, mint a least-privilege `dnt1.` token bound
        // strictly to THIS agent's `allowed_memory_scopes` (preserving cross-project isolation
        // while eliminating static-token `403 ForbiddenScope` on user-created projects).
        let effective_token = if let Some(ref signer) = self.inner.memory_hmac_signer
            && !agent.current_config.allowed_memory_scopes.is_empty()
        {
            let mut caps = BTreeSet::new();
            caps.insert(MemoryCapability::Read);
            caps.insert(MemoryCapability::Write);
            let principal_res = PrincipalId::try_new(format!("agent:{}", agent.agent_id));
            match principal_res.and_then(|principal| {
                signer.mint_scoped_token(
                    principal,
                    agent.current_config.allowed_memory_scopes.clone(),
                    caps,
                )
            }) {
                Ok(tok) => tok,
                Err(_) => mcp.memory_token.clone(),
            }
        } else {
            mcp.memory_token.clone()
        };

        // CRITICAL SECURITY BOUNDARY: pass ONLY `DENNETT_MEMORY_URL` and scoped
        // `DENNETT_MEMORY_TOKEN` — NEVER PostgreSQL credentials or Head DB URLs!
        vec![RuntimeMcpServerSpec {
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
        }]
    }

    pub(crate) fn collect_turn_secrets(&self, mcp_servers: &[RuntimeMcpServerSpec]) -> Vec<String> {
        let mut turn_secrets = self.inner.secrets_to_redact.clone();
        for server in mcp_servers {
            for entry in &server.env {
                if entry.name == "DENNETT_MEMORY_TOKEN" && !entry.value.trim().is_empty() {
                    turn_secrets.push(entry.value.clone());
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
        active.insert(session_id, handle);
        Ok(())
    }

    pub(crate) async fn unregister_active_session_run(&self, session_id: SessionId, run_id: RunId) {
        let mut active = self.inner.active_runs.lock().await;
        if active.get(&session_id).is_some_and(|h| h.run_id == run_id) {
            active.remove(&session_id);
        }
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

    pub(crate) fn background_runtime(&self) -> Option<&Arc<dyn AgentRuntimePort>> {
        self.inner.background_runtime.as_ref()
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
            let _ = self.inner.runtime.cancel_turn(session_id).await;
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

/// Maximum workspace-backed file size hashed on-demand during `create_artifact` (128 MiB).
const MAX_WORKSPACE_ARTIFACT_HASH_BYTES: u64 = 128 * 1024 * 1024;

/// Streams a regular (non-symlink, non-reparse-point) workspace file in O(1) RAM through SHA-256.
fn stream_sha256_regular_file(
    full_path: &Path,
    max_bytes: u64,
    scope_label: &str,
    display_rel: &str,
) -> Result<Sha256Hex, DennettError> {
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
    if meta.len() > max_bytes {
        return Err(DennettError::InvalidInput {
            field: String::from("relative_path"),
            reason: format!(
                "workspace artifact file exceeds maximum hashable size ({max_bytes} bytes)"
            ),
        });
    }

    let file = fs::File::open(full_path).map_err(|err| DennettError::NotFound {
        resource: String::from("workspace_file"),
        id: format!("{display_rel} ({err})"),
    })?;
    let mut reader = BufReader::with_capacity(64 * 1024, file);
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut total_read: u64 = 0;

    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|err| DennettError::Internal {
                detail: format!("failed reading workspace artifact `{display_rel}`: {err}"),
            })?;
        if n == 0 {
            break;
        }
        total_read = total_read.saturating_add(n as u64);
        if total_read > max_bytes {
            return Err(DennettError::InvalidInput {
                field: String::from("relative_path"),
                reason: format!(
                    "workspace artifact file grew beyond maximum hashable size ({max_bytes} bytes)"
                ),
            });
        }
        hasher.update(&buf[..n]);
    }

    Ok(Sha256Hex::from_digest_bytes(hasher.finalize().into()))
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
}
