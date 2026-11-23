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

use core::fmt;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod};
use dennett_contracts::redact::{REDACTED_PLACEHOLDER, redact_secrets_owned};
use dennett_contracts::{
    AgentConfigHistoryResponse, AgentConfigVersionId, AgentId, AgentRecord, ArtifactId,
    ArtifactRecord, CancelRunResponse, CommitMemoryNoteRequest, CommitMemoryNoteResponse,
    CorrelationId, CreateAgentRequest, CreateArtifactRequest, CreateNoteRequest,
    CreatePolicyRuleRequest, CreateProjectRequest, CreateSessionRequest, DennettError,
    DiagnosticsResponse, FlushOutboxResponse, HeadHealth, HeadMemorySearchRequest,
    HeadMemorySearchResponse, IdempotencyKey, ListAgentsResponse, ListArtifactsResponse,
    ListOutboxResponse, ListPolicyRulesResponse, ListProjectsResponse, ListSessionsResponse,
    MemoryOutboxItem, MemoryOutboxStatus, MemoryStatus, MessageId, MessageRecord, MessageRole,
    OutboxItemId, PermissionDecisionRecord, PolicyRule, PolicyRuleId, ProfileMode, ProjectId,
    ProjectRecord, RunId, RunRecord, RunStatus, RuntimeBinding, RuntimeBindingId,
    SearchNotesRequest, SendMessageRequest, SendMessageResponse, ServiceKind,
    SessionDetailResponse, SessionId, SessionRecord, SessionStatus, TimestampMs,
    UpdateAgentConfigRequest,
};
use dennett_memory_client::MemoryClient;
use dennett_runtime_antigravity::{
    AgentRuntimePort, RuntimeEvent, RuntimeMcpEnvEntry, RuntimeMcpServerSpec, SessionStartRequest,
    TurnRequest,
};
use tokio::sync::{Mutex, watch};
use tokio::time;
use tokio_postgres::NoTls;

use crate::migrations::{query_control_schema_version, run_control_migrations, sha256_hex};
use crate::pg_target::ValidatedControlPgTarget;
use crate::policy::{
    PolicyContext, PolicyEngine, RecordingPolicyEvaluator, resolve_within_workspace,
};
use crate::store::{
    append_audit_event, decode_agent_join_row, decode_audit_event_row, decode_config_version_row,
    decode_outbox_row, decode_permission_decision_row, decode_policy_rule_row, decode_project_row,
    decode_runtime_binding_row, decode_session_row, fetch_agent, fetch_all_runtime_bindings,
    fetch_policy_rules, fetch_project, fetch_runtime_binding, fetch_session,
    fetch_session_artifacts, fetch_session_messages, fetch_session_permission_decisions,
    fetch_session_runs, map_pg_err, next_stream_seq, truncate_utf8,
};

/// Current implementation milestone label advertised in [`HeadHealth`].
pub const HEAD_STAGE_LABEL: &str = "D1";
/// Default PostgreSQL pool size for `dennett-head`.
pub const DEFAULT_CONTROL_POOL_SIZE: usize = 16;
/// Default timeout for standard control-plane PostgreSQL queries.
pub const DEFAULT_PG_OP_TIMEOUT: Duration = Duration::from_secs(10);
/// Shorter timeout for `/healthz` and `memoryd` reachability checks.
pub const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_secs(3);
/// Default wall-clock timeout for a single runtime turn.
pub const DEFAULT_TURN_TIMEOUT: Duration = Duration::from_secs(120);

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
    ]
}

struct ActiveRunHandle {
    run_id: RunId,
    cancel_tx: watch::Sender<bool>,
}

struct ActiveRunGuard {
    active_runs: Arc<Mutex<HashMap<SessionId, ActiveRunHandle>>>,
    session_id: SessionId,
    run_id: RunId,
}

impl Drop for ActiveRunGuard {
    fn drop(&mut self) {
        let map = Arc::clone(&self.active_runs);
        let session_id = self.session_id;
        let run_id = self.run_id;
        if let Ok(mut guard) = map.try_lock() {
            if guard.get(&session_id).is_some_and(|h| h.run_id == run_id) {
                guard.remove(&session_id);
            }
            return;
        }
        tokio::spawn(async move {
            let mut guard = map.lock().await;
            if guard.get(&session_id).is_some_and(|h| h.run_id == run_id) {
                guard.remove(&session_id);
            }
        });
    }
}

const MEMORY_STATUS_DEGRADED: u8 = 0;
const MEMORY_STATUS_HEALTHY: u8 = 1;

struct HeadServiceInner {
    pg_target: ValidatedControlPgTarget,
    pool: Pool,
    migrated_version: Mutex<Option<i64>>,
    runtime: Arc<dyn AgentRuntimePort>,
    memory_client: Option<MemoryClient>,
    memory_mcp: Option<MemoryMcpWiring>,
    last_memory_status: AtomicU8,
    active_runs: Arc<Mutex<HashMap<SessionId, ActiveRunHandle>>>,
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

        Ok(Self {
            inner: Arc::new(HeadServiceInner {
                pg_target,
                pool,
                migrated_version: Mutex::new(None),
                runtime,
                memory_client,
                memory_mcp,
                last_memory_status: AtomicU8::new(initial_memory_status),
                active_runs: Arc::new(Mutex::new(HashMap::new())),
                profile_mode,
                secrets_to_redact: secrets,
            }),
        })
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
            capabilities: d1_head_capabilities(),
            correlation_id,
        })
    }

    /// Creates a new stable [`AgentRecord`] with its initial revision-1 [`dennett_contracts::AgentConfigVersion`].
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] or [`DennettError::DatabaseUnavailable`].
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
                model_preference, thinking_level, allowed_memory_scopes, workspace_write_policy
             ) VALUES ($1, $2, 1, $3, $4, $5, $6, $7)",
            &[
                &config_version_id.as_uuid(),
                &agent_id.as_uuid(),
                &req.system_prompt,
                &req.model_preference,
                &thinking_str,
                &scopes_json,
                &write_policy_json,
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
        let rows = conn
            .query(
                "SELECT a.agent_id, a.name, a.description, a.current_config_version_id,
                        TRUNC(EXTRACT(EPOCH FROM a.created_at) * 1000)::bigint,
                        TRUNC(EXTRACT(EPOCH FROM a.updated_at) * 1000)::bigint,
                        v.config_version_id, v.version_number, v.system_prompt,
                        v.model_preference, v.thinking_level,
                        v.allowed_memory_scopes, v.workspace_write_policy,
                        TRUNC(EXTRACT(EPOCH FROM v.created_at) * 1000)::bigint
                 FROM agents a
                 JOIN agent_config_versions v ON v.config_version_id = a.current_config_version_id
                 ORDER BY a.created_at DESC",
                &[],
            )
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
        let thinking_str = req.thinking_level.map(|t| t.as_str().to_owned());

        tx.execute(
            "INSERT INTO agent_config_versions (
                config_version_id, agent_id, version_number, system_prompt,
                model_preference, thinking_level, allowed_memory_scopes, workspace_write_policy
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            &[
                &new_version_id.as_uuid(),
                &agent_id.as_uuid(),
                &next_ver,
                &req.system_prompt,
                &req.model_preference,
                &thinking_str,
                &scopes_json,
                &write_policy_json,
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
                        workspace_write_policy,
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
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        let project_id = ProjectId::new();
        let workspace_path = req.workspace_path.trim().to_owned();
        let row = tx
            .query_one(
                "INSERT INTO projects (project_id, name, workspace_path, memory_scope)
                 VALUES ($1, $2, $3, $4)
                 RETURNING project_id, name, workspace_path, memory_scope,
                           TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint",
                &[
                    &project_id.as_uuid(),
                    &req.name.trim(),
                    &workspace_path,
                    &req.memory_scope.as_str(),
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
                "registered project `{}` ({project_id}) scope={}",
                req.name.trim(),
                req.memory_scope
            ),
        )
        .await?;

        let record = decode_project_row(&row)?;
        tx.commit()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;
        Ok(record)
    }

    /// Lists all registered projects.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::DatabaseUnavailable`] on PG failure.
    pub async fn list_projects(&self) -> Result<ListProjectsResponse, DennettError> {
        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let rows = conn
            .query(
                "SELECT project_id, name, workspace_path, memory_scope,
                        TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint
                 FROM projects
                 ORDER BY created_at DESC",
                &[],
            )
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
        let _ = fetch_project(&tx, req.project_id, &self.inner.pg_target).await?;

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
    pub async fn create_artifact(
        &self,
        session_id: SessionId,
        req: CreateArtifactRequest,
        correlation_id: &CorrelationId,
    ) -> Result<ArtifactRecord, DennettError> {
        req.validate()?;
        let _ = self.ensure_migrated().await?;
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let session = fetch_session(&conn, session_id, &self.inner.pg_target).await?;
        let project = fetch_project(&conn, session.project_id, &self.inner.pg_target).await?;

        let (normalized_rel_path, hash_bytes) = if let Some(ref rel) = req.relative_path {
            let safe_rel = resolve_within_workspace(Path::new(&project.workspace_path), rel)
                .map_err(|reason| DennettError::AccessDenied {
                    scope: format!("project:{}", project.project_id),
                    reason,
                })?;
            let bytes = if let Some(ref inline) = req.content {
                inline.as_bytes().to_vec()
            } else {
                let full_path = Path::new(&project.workspace_path).join(&safe_rel);
                fs::read(&full_path).map_err(|err| DennettError::NotFound {
                    resource: String::from("workspace_file"),
                    id: format!("{safe_rel} ({err})"),
                })?
            };
            (Some(safe_rel), bytes)
        } else {
            let bytes = req
                .content
                .as_deref()
                .unwrap_or_default()
                .as_bytes()
                .to_vec();
            (None, bytes)
        };

        let digest = sha256_hex(&hash_bytes);
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

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
                    revision, content, relative_path, sha256
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                 RETURNING TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint",
                &[
                    &artifact_id.as_uuid(),
                    &session_id.as_uuid(),
                    &project.project_id.as_uuid(),
                    &req.run_id.map(RunId::as_uuid),
                    &req.name.trim(),
                    &req.mime_type.trim(),
                    &revision,
                    &req.content,
                    &normalized_rel_path,
                    &digest.as_str(),
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
                "registered artifact `{}` rev {revision} ({artifact_id})",
                req.name.trim()
            ),
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        Ok(ArtifactRecord {
            artifact_id,
            session_id,
            project_id: project.project_id,
            run_id: req.run_id,
            name: req.name.trim().to_owned(),
            mime_type: req.mime_type.trim().to_owned(),
            revision,
            content: req.content,
            relative_path: normalized_rel_path,
            sha256: digest,
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
    /// with scoped `dennett-memory-mcp`, executes one turn with external [`PolicyEngine`] callbacks
    /// and live cancellation support, and persists all decisions, audit traces, and outcomes.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError`] on validation, missing session, concurrent active run conflict,
    /// auth failure, runtime error, or database unavailability.
    #[expect(
        clippy::too_many_lines,
        reason = "send_message orchestrates pre-run persistence, runtime turn execution, and post-run atomic recording without holding SQL locks across runtime I/O"
    )]
    pub async fn send_message(
        &self,
        session_id: SessionId,
        req: SendMessageRequest,
        correlation_id: CorrelationId,
    ) -> Result<SendMessageResponse, DennettError> {
        req.validate()?;
        let _ = self.ensure_migrated().await?;

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
            let binding = fetch_runtime_binding(&conn, session_id, &self.inner.pg_target).await?;
            let history = fetch_session_messages(&conn, session_id, &self.inner.pg_target).await?;
            let rules = fetch_policy_rules(&conn, &self.inner.pg_target).await?;
            (session, agent, project, binding, history, rules)
        };

        // 2. Acquire per-session active run slot (prevents two concurrent writers on one session).
        let run_id = RunId::new();
        let (cancel_tx, cancel_rx) = watch::channel(false);
        {
            let mut active = self.inner.active_runs.lock().await;
            if active.contains_key(&session_id) {
                return Err(DennettError::Conflict {
                    resource: String::from("session_run"),
                    expected_revision: None,
                    actual_revision: None,
                    reason: String::from("another run is already active in this session"),
                });
            }
            active.insert(session_id, ActiveRunHandle { run_id, cancel_tx });
        }
        let _active_guard = ActiveRunGuard {
            active_runs: Arc::clone(&self.inner.active_runs),
            session_id,
            run_id,
        };

        // 3. Persist user message + running RunRecord BEFORE touching runtime (Architecture 80 §11).
        let user_message = {
            let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
            let tx = conn
                .transaction()
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

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
                        &req.content,
                        &run_id.as_uuid(),
                        &correlation_id.as_str(),
                    ],
                )
                .await
                .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

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

            MessageRecord {
                message_id: msg_id,
                session_id,
                role: MessageRole::User,
                content: req.content.clone(),
                run_id: Some(run_id),
                correlation_id: correlation_id.clone(),
                created_at: TimestampMs::from_millis(msg_row.get(0)),
            }
        };

        // 4. Probe memoryd status (never blocks or aborts the control turn when memoryd is down!).
        let (memory_status, _mem_detail) = self.probe_memory_status(&correlation_id).await;

        // 5. Start or explicitly resume runtime session (with scoped `dennett-memory-mcp`).
        let mcp_servers = self.build_memory_mcp_specs();
        let start_outcome = match self
            .inner
            .runtime
            .start_or_resume_session(SessionStartRequest {
                dennett_session_id: session_id,
                workspace_cwd: PathBuf::from(&project.workspace_path),
                system_prompt: agent.current_config.system_prompt.clone(),
                model_preference: agent.current_config.model_preference.clone(),
                thinking_level: agent.current_config.thinking_level,
                workspace_write_policy: agent.current_config.workspace_write_policy.clone(),
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
                return Err(err);
            }
        };

        let runtime_binding = self
            .upsert_runtime_binding(session_id, &start_outcome, &correlation_id, Some(run_id))
            .await?;

        // 6. Execute turn with external PolicyEngine + live cancellation handle.
        let policy_engine = PolicyEngine::new(PolicyContext {
            project_id: project.project_id,
            agent_id: agent.agent_id,
            session_id: session.session_id,
            workspace_root: PathBuf::from(&project.workspace_path),
            workspace_write_policy: agent.current_config.workspace_write_policy.clone(),
            allowed_memory_scopes: agent.current_config.allowed_memory_scopes.clone(),
            rules: policy_rules,
        });
        let (recording_policy, decisions_buf) = RecordingPolicyEvaluator::new(
            policy_engine,
            correlation_id.clone(),
            self.inner.secrets_to_redact.clone(),
        );
        let turn_timeout = req
            .timeout_ms
            .map_or(DEFAULT_TURN_TIMEOUT, Duration::from_millis);

        let turn_result = self
            .inner
            .runtime
            .send_prompt_turn(
                TurnRequest {
                    dennett_session_id: session_id,
                    provider_session_id: start_outcome.provider_session_id.clone(),
                    run_id,
                    prompt_text: req.content,
                    timeout: turn_timeout,
                    cancel_rx,
                },
                Arc::new(recording_policy),
                None,
            )
            .await;

        let captured_decisions: Vec<PermissionDecisionRecord> =
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
                return Err(err);
            }
        };

        // 7. Atomically persist decisions, tool audit trace, assistant reply, and terminal RunRecord.
        let (assistant_message, run_record) = self
            .finalize_turn(
                session_id,
                run_id,
                &agent,
                user_message.message_id,
                start_outcome.rehydrated,
                &turn_outcome,
                &captured_decisions,
                &correlation_id,
            )
            .await?;

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
            permission_decisions: captured_decisions,
            memory_status,
        })
    }

    /// Triggers immediate live cancellation (`Stop`) on any active run in `session_id`.
    ///
    /// CRITICAL V02 INVARIANT: NEVER blocks on or calls `dennett-memoryd`!
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`] if `session_id` does not exist or
    /// [`DennettError::DatabaseUnavailable`] if control PG fails.
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

        let cancelled = active_run_id.is_some() || runtime_cancelled;

        // Verify session exists and record cancel audit in control PG (never calls memoryd!).
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let _ = fetch_session(&conn, session_id, &self.inner.pg_target).await?;
        let _ = append_audit_event(
            &conn,
            &self.inner.pg_target,
            correlation_id,
            Some(session_id),
            active_run_id,
            "head",
            "run_cancel_requested",
            &format!(
                "cancel requested for session {session_id} (active_run={active_run_id:?}, signalled={cancelled})"
            ),
        )
        .await?;

        let status = if cancelled {
            Some(RunStatus::Cancelled)
        } else {
            fetch_session_runs(&conn, session_id, &self.inner.pg_target)
                .await?
                .last()
                .map(|r| r.status)
        };

        Ok(CancelRunResponse {
            session_id,
            run_id: active_run_id,
            cancelled,
            status,
            // Non-blocking V02 report: returns the latest observed memoryd status from an
            // atomic cache without performing any synchronous network probe during Stop!
            memory_status: self.cached_memory_status(),
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
        let Some(ref client) = self.inner.memory_client else {
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
        let Some(ref client) = self.inner.memory_client else {
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

    async fn upsert_runtime_binding(
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
        reason = "atomic turn finalization persists decisions, tool observations, assistant message, and terminal run status together"
    )]
    async fn finalize_turn(
        &self,
        session_id: SessionId,
        run_id: RunId,
        agent: &AgentRecord,
        user_message_id: MessageId,
        rehydrated: bool,
        outcome: &dennett_runtime_antigravity::TurnOutcome,
        decisions: &[PermissionDecisionRecord],
        correlation_id: &CorrelationId,
    ) -> Result<(Option<MessageRecord>, RunRecord), DennettError> {
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

        for dec in decisions {
            insert_permission_decision(&tx, &self.inner.pg_target, dec).await?;
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
                correlation_id: correlation_id.clone(),
                created_at: TimestampMs::from_millis(msg_row.get(0)),
            })
        };

        let stop_reason = truncate_utf8(&outcome.stop_reason, 128).to_owned();
        let run_row = tx
            .query_one(
                "UPDATE runs
                 SET assistant_message_id = $2,
                     status = $3,
                     stop_reason = $4,
                     rehydrated = $5,
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

        tx.commit()
            .await
            .map_err(|err| map_pg_err(&self.inner.pg_target, &err))?;

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
        decisions: &[PermissionDecisionRecord],
    ) {
        let Ok(mut conn) = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await else {
            return;
        };
        let Ok(tx) = conn.transaction().await else {
            return;
        };
        for dec in decisions {
            let _ = insert_permission_decision(&tx, &self.inner.pg_target, dec).await;
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
        let _ = tx.commit().await;
    }

    fn build_memory_mcp_specs(&self) -> Vec<RuntimeMcpServerSpec> {
        let Some(ref mcp) = self.inner.memory_mcp else {
            return Vec::new();
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
                    value: mcp.memory_token.clone(),
                },
            ],
        }]
    }

    fn cached_memory_status(&self) -> MemoryStatus {
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

    async fn get_conn(&self, timeout: Duration) -> Result<deadpool_postgres::Object, DennettError> {
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

async fn insert_permission_decision<C: deadpool_postgres::GenericClient>(
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
    assert_send_sync::<MemoryMcpWiring>();
};
