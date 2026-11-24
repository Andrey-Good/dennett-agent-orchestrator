//! Typed Tauri 2 IPC commands delegating directly to [`HeadBridge`].
//!
//! Every command acquires the shared bridge lock only long enough to clone the cheap
//! `Arc`-backed [`HeadBridge`], then releases the lock before performing async HTTP I/O
//! so `cancel_run` (`Stop`) is never blocked behind an in-flight `send_message` turn.

use std::str::FromStr;

use dennett_contracts::{
    AgentId, AgentRecord, ArtifactId, ArtifactLifecycleRequest, ArtifactLifecycleResponse,
    ArtifactRecord, CancelRunResponse, CancelTaskResponse, CapabilityGrantId,
    CapabilityGrantRecord, CreateAgentRequest, CreateArtifactRequest, CreateProjectRequest,
    CreateSessionRequest, CreateTaskRequest, DiagnosticsResponse, DispatchEffectRequest,
    DispatchEffectResponse, EffectClaimId, EventStreamResponse, FlushOutboxResponse,
    HeadMemorySearchRequest, HeadMemorySearchResponse, InboxCardId, ListAgentMessagesResponse,
    ListAgentsResponse, ListArtifactsResponse, ListCapabilityGrantsResponse, ListEffectsResponse,
    ListInboxResponse, ListPermissionRequestsResponse, ListProjectFilesResponse,
    ListProjectsResponse, ListSessionsResponse, ListTasksResponse, PermissionRequestId, ProjectId,
    ProjectLifecycleResponse, ProjectRecord, RebindProjectRequest, ReconcileEffectRequest,
    ReconcileEffectResponse, ReplyTaskRequest, ResolveInboxCardRequest, ResolveInboxCardResponse,
    ResolvePermissionRequest, ResolvePermissionResponse, RevokeGrantRequest,
    RuntimeCatalogResponse, SendAgentMessageRequest, SendAgentMessageResponse, SendMessageRequest,
    SendMessageResponse, SessionDetailResponse, SessionId, SessionRecord, StateSnapshotResponse,
    TaskDetailResponse, TaskId, TaskRecord, TaskSignalRequest, TaskSignalResponse,
    UpdateAgentConfigRequest, UpdateProjectRequest, UpdateSessionRequest, UpdateTaskRequest,
};
use tauri::State;
use tokio::sync::RwLock;

use crate::bridge::{BridgeError, DesktopHealthSnapshot, HeadBridge};

/// Shared mutable bridge state managed by Tauri across IPC command invocations.
#[derive(Debug)]
pub struct SharedBridge {
    inner: RwLock<HeadBridge>,
}

impl SharedBridge {
    /// Wraps an initialized [`HeadBridge`] for Tauri state management.
    #[must_use]
    pub fn new(bridge: HeadBridge) -> Self {
        Self {
            inner: RwLock::new(bridge),
        }
    }

    async fn snapshot_bridge(&self) -> HeadBridge {
        self.inner.read().await.clone()
    }

    async fn replace_url(&self, new_url: &str) -> Result<HeadBridge, BridgeError> {
        let policy = self.inner.read().await.policy();
        let next = HeadBridge::new(new_url, policy)?;
        let mut guard = self.inner.write().await;
        *guard = next.clone();
        Ok(next)
    }
}

fn parse_project_id(raw: &str) -> Result<ProjectId, BridgeError> {
    ProjectId::from_str(raw.trim()).map_err(BridgeError::from)
}

fn parse_agent_id(raw: &str) -> Result<AgentId, BridgeError> {
    AgentId::from_str(raw.trim()).map_err(BridgeError::from)
}

fn parse_session_id(raw: &str) -> Result<SessionId, BridgeError> {
    SessionId::from_str(raw.trim()).map_err(BridgeError::from)
}

fn parse_artifact_id(raw: &str) -> Result<ArtifactId, BridgeError> {
    ArtifactId::from_str(raw.trim()).map_err(BridgeError::from)
}

fn parse_task_id(raw: &str) -> Result<TaskId, BridgeError> {
    TaskId::from_str(raw.trim()).map_err(BridgeError::from)
}

fn parse_inbox_card_id(raw: &str) -> Result<InboxCardId, BridgeError> {
    InboxCardId::from_str(raw.trim()).map_err(BridgeError::from)
}

fn parse_permission_request_id(raw: &str) -> Result<PermissionRequestId, BridgeError> {
    PermissionRequestId::from_str(raw.trim()).map_err(BridgeError::from)
}

fn parse_capability_grant_id(raw: &str) -> Result<CapabilityGrantId, BridgeError> {
    CapabilityGrantId::from_str(raw.trim()).map_err(BridgeError::from)
}

fn parse_effect_claim_id(raw: &str) -> Result<EffectClaimId, BridgeError> {
    EffectClaimId::from_str(raw.trim()).map_err(BridgeError::from)
}

fn parse_optional_project_id(raw: Option<&str>) -> Result<Option<ProjectId>, BridgeError> {
    raw.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(parse_project_id)
        .transpose()
}

fn parse_optional_session_id(raw: Option<&str>) -> Result<Option<SessionId>, BridgeError> {
    raw.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(parse_session_id)
        .transpose()
}

fn parse_optional_task_id(raw: Option<&str>) -> Result<Option<TaskId>, BridgeError> {
    raw.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(parse_task_id)
        .transpose()
}

/// Returns the current connection status, PostgreSQL/memoryd health, and runtime truth.
#[tauri::command]
pub async fn get_head_health(
    state: State<'_, SharedBridge>,
) -> Result<DesktopHealthSnapshot, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    Ok(bridge.get_head_health().await)
}

/// Re-targets the desktop bridge to another validated `dennett-head` URL and probes health.
#[tauri::command]
pub async fn set_head_url(
    state: State<'_, SharedBridge>,
    head_url: String,
) -> Result<DesktopHealthSnapshot, BridgeError> {
    let bridge = state.replace_url(&head_url).await?;
    Ok(bridge.get_head_health().await)
}

/// Lists workspace projects registered in `dennett-head` (optionally including archived projects).
#[tauri::command]
pub async fn list_projects(
    state: State<'_, SharedBridge>,
    include_archived: Option<bool>,
) -> Result<ListProjectsResponse, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge
        .list_projects_with_options(include_archived.unwrap_or(false))
        .await
}

/// Registers a new workspace project in `dennett-head`.
#[tauri::command]
pub async fn create_project(
    state: State<'_, SharedBridge>,
    request: CreateProjectRequest,
) -> Result<ProjectRecord, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.create_project(&request).await
}

/// Lists all Dennett agents with their current active configuration versions.
#[tauri::command]
pub async fn list_agents(
    state: State<'_, SharedBridge>,
) -> Result<ListAgentsResponse, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.list_agents().await
}

/// Creates a new stable Dennett agent.
#[tauri::command]
pub async fn create_agent(
    state: State<'_, SharedBridge>,
    request: CreateAgentRequest,
) -> Result<AgentRecord, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.create_agent(&request).await
}

/// Appends a new immutable configuration revision on an existing agent without changing `agent_id`.
#[tauri::command]
pub async fn update_agent_config(
    state: State<'_, SharedBridge>,
    agent_id: String,
    request: UpdateAgentConfigRequest,
) -> Result<AgentRecord, BridgeError> {
    let parsed_id = parse_agent_id(&agent_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.update_agent_config(parsed_id, &request).await
}

/// Lists persisted conversation sessions ordered newest first.
#[tauri::command]
pub async fn list_sessions(
    state: State<'_, SharedBridge>,
) -> Result<ListSessionsResponse, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.list_sessions().await
}

/// Creates a new conversation session bound to an existing project and agent.
#[tauri::command]
pub async fn create_session(
    state: State<'_, SharedBridge>,
    request: CreateSessionRequest,
) -> Result<SessionRecord, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.create_session(&request).await
}

/// Fetches full canonical detail for one conversation session.
#[tauri::command]
pub async fn get_session_detail(
    state: State<'_, SharedBridge>,
    session_id: String,
) -> Result<SessionDetailResponse, BridgeError> {
    let parsed_id = parse_session_id(&session_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.get_session_detail(parsed_id).await
}

/// Sends a user message through `dennett-head` and returns the persisted turn result.
#[tauri::command]
pub async fn send_message(
    state: State<'_, SharedBridge>,
    session_id: String,
    request: SendMessageRequest,
) -> Result<SendMessageResponse, BridgeError> {
    let parsed_id = parse_session_id(&session_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.send_message(parsed_id, &request).await
}

/// Cancels any active execution run for `session_id` (`Stop` button).
#[tauri::command]
pub async fn cancel_run(
    state: State<'_, SharedBridge>,
    session_id: String,
) -> Result<CancelRunResponse, BridgeError> {
    let parsed_id = parse_session_id(&session_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.cancel_run(parsed_id).await
}

/// Lists ordinary artifacts attached to `session_id`.
#[tauri::command]
pub async fn list_artifacts(
    state: State<'_, SharedBridge>,
    session_id: String,
) -> Result<ListArtifactsResponse, BridgeError> {
    let parsed_id = parse_session_id(&session_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.list_artifacts(parsed_id).await
}

/// Creates a new ordinary artifact attached to `session_id`.
#[tauri::command]
pub async fn create_artifact(
    state: State<'_, SharedBridge>,
    session_id: String,
    request: CreateArtifactRequest,
) -> Result<ArtifactRecord, BridgeError> {
    let parsed_id = parse_session_id(&session_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.create_artifact(parsed_id, &request).await
}

/// Retrieves a single persisted artifact by `(session_id, artifact_id)`.
#[tauri::command]
pub async fn get_artifact(
    state: State<'_, SharedBridge>,
    session_id: String,
    artifact_id: String,
) -> Result<ArtifactRecord, BridgeError> {
    let parsed_session = parse_session_id(&session_id)?;
    let parsed_artifact = parse_artifact_id(&artifact_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.get_artifact(parsed_session, parsed_artifact).await
}

/// Lists secret-free audit events, permission decisions, and memory outbox items.
#[tauri::command]
pub async fn list_diagnostics(
    state: State<'_, SharedBridge>,
    limit: Option<i64>,
) -> Result<DiagnosticsResponse, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.list_diagnostics(limit).await
}

/// Opens the native OS folder chooser without widening `WebView` filesystem or shell capabilities.
#[tauri::command]
pub async fn pick_workspace_folder() -> Result<Option<String>, BridgeError> {
    tokio::task::spawn_blocking(|| {
        rfd::FileDialog::new()
            .set_title("Выберите рабочую папку проекта")
            .pick_folder()
            .map(|path| path.to_string_lossy().into_owned())
    })
    .await
    .map_err(|err| BridgeError::HeadApi {
        code: String::from("INTERNAL"),
        message: format!("native folder dialog task failed: {err}"),
    })
}

/// Fetches the dynamic runtime model, thinking-level, account, and quota catalog from `dennett-head`.
#[tauri::command]
pub async fn get_runtime_catalog(
    state: State<'_, SharedBridge>,
    refresh: Option<bool>,
) -> Result<RuntimeCatalogResponse, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.get_runtime_catalog(refresh.unwrap_or(false)).await
}

/// Renames a project or attaches/rebinds its workspace folder (`PATCH /v1/projects/{project_id}`).
#[tauri::command]
pub async fn update_project(
    state: State<'_, SharedBridge>,
    project_id: String,
    request: UpdateProjectRequest,
) -> Result<ProjectRecord, BridgeError> {
    let parsed_id = parse_project_id(&project_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.update_project(parsed_id, &request).await
}

/// Lists real files inside a project's bound workspace folder (`GET /v1/projects/{project_id}/files`).
#[tauri::command]
pub async fn list_project_files(
    state: State<'_, SharedBridge>,
    project_id: String,
    subpath: Option<String>,
) -> Result<ListProjectFilesResponse, BridgeError> {
    let parsed_id = parse_project_id(&project_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge
        .list_project_files(parsed_id, subpath.as_deref())
        .await
}

/// Renames an existing conversation session inline (`PATCH /v1/sessions/{session_id}`).
#[tauri::command]
pub async fn update_session(
    state: State<'_, SharedBridge>,
    session_id: String,
    request: UpdateSessionRequest,
) -> Result<SessionRecord, BridgeError> {
    let parsed_id = parse_session_id(&session_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.update_session(parsed_id, &request).await
}

/// Queries memory notes in a scope via `dennett-head` (`POST /v1/memory/search`).
#[tauri::command]
pub async fn search_memory(
    state: State<'_, SharedBridge>,
    request: HeadMemorySearchRequest,
) -> Result<HeadMemorySearchResponse, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.search_memory(&request).await
}

/// Manually triggers an idempotent flush of pending `memory_commit_outbox` items.
#[tauri::command]
pub async fn flush_memory_outbox(
    state: State<'_, SharedBridge>,
) -> Result<FlushOutboxResponse, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.flush_memory_outbox().await
}

/// Rebinds a project's canonical workspace folder (`POST /v1/projects/{id}/rebind`).
#[tauri::command]
pub async fn rebind_project(
    state: State<'_, SharedBridge>,
    project_id: String,
    request: RebindProjectRequest,
) -> Result<ProjectLifecycleResponse, BridgeError> {
    let parsed_id = parse_project_id(&project_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.rebind_project(parsed_id, &request).await
}

/// Detaches a project's workspace folder without deleting user files (`POST /v1/projects/{id}/detach-workspace`).
#[tauri::command]
pub async fn detach_project_workspace(
    state: State<'_, SharedBridge>,
    project_id: String,
) -> Result<ProjectLifecycleResponse, BridgeError> {
    let parsed_id = parse_project_id(&project_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.detach_project_workspace(parsed_id).await
}

/// Reversibly archives a project and pauses its timers (`POST /v1/projects/{id}/archive`).
#[tauri::command]
pub async fn archive_project(
    state: State<'_, SharedBridge>,
    project_id: String,
) -> Result<ProjectLifecycleResponse, BridgeError> {
    let parsed_id = parse_project_id(&project_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.archive_project(parsed_id).await
}

/// Restores an archived project to active state (`POST /v1/projects/{id}/unarchive`).
#[tauri::command]
pub async fn unarchive_project(
    state: State<'_, SharedBridge>,
    project_id: String,
) -> Result<ProjectLifecycleResponse, BridgeError> {
    let parsed_id = parse_project_id(&project_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.unarchive_project(parsed_id).await
}

/// Removes a project entry from Dennett active lists without deleting user files (`POST /v1/projects/{id}/remove`).
#[tauri::command]
pub async fn remove_project(
    state: State<'_, SharedBridge>,
    project_id: String,
) -> Result<ProjectLifecycleResponse, BridgeError> {
    let parsed_id = parse_project_id(&project_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.remove_project(parsed_id).await
}

/// Deletes Dennett metadata for a project while leaving external user folders untouched (`DELETE /v1/projects/{id}?confirm=...`).
#[tauri::command]
pub async fn delete_project(
    state: State<'_, SharedBridge>,
    project_id: String,
    confirm: bool,
) -> Result<ProjectLifecycleResponse, BridgeError> {
    let parsed_id = parse_project_id(&project_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.delete_project(parsed_id, confirm).await
}

/// Lists all persisted revisions (`v1`, `v2`, …) of an artifact within `session_id`.
#[tauri::command]
pub async fn list_artifact_revisions(
    state: State<'_, SharedBridge>,
    session_id: String,
    artifact_id_or_name: String,
) -> Result<ListArtifactsResponse, BridgeError> {
    let parsed_session = parse_session_id(&session_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge
        .list_artifact_revisions(parsed_session, &artifact_id_or_name)
        .await
}

/// Applies an explicit Contract D lifecycle transition to one artifact revision.
#[tauri::command]
pub async fn apply_artifact_lifecycle(
    state: State<'_, SharedBridge>,
    session_id: String,
    artifact_id: String,
    request: ArtifactLifecycleRequest,
) -> Result<ArtifactLifecycleResponse, BridgeError> {
    let parsed_session = parse_session_id(&session_id)?;
    let parsed_artifact = parse_artifact_id(&artifact_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge
        .apply_artifact_lifecycle(parsed_session, parsed_artifact, &request)
        .await
}

/// Fetches a consistent `REPEATABLE READ` state snapshot with `snapshot_cursor` (`GET /v1/state/snapshot`).
#[tauri::command]
pub async fn get_state_snapshot(
    state: State<'_, SharedBridge>,
    project_id: Option<String>,
    include_archived: Option<bool>,
) -> Result<StateSnapshotResponse, BridgeError> {
    let parsed_project = parse_optional_project_id(project_id.as_deref())?;
    let bridge = state.snapshot_bridge().await;
    bridge
        .get_state_snapshot(parsed_project, include_archived.unwrap_or(false))
        .await
}

/// Tails gapless ordered control-plane journal events after `after_cursor` (`GET /v1/events`).
#[tauri::command]
pub async fn get_events(
    state: State<'_, SharedBridge>,
    after_cursor: i64,
    expected_epoch: Option<i64>,
    project_id: Option<String>,
    limit: Option<u32>,
) -> Result<EventStreamResponse, BridgeError> {
    let parsed_project = parse_optional_project_id(project_id.as_deref())?;
    let bridge = state.snapshot_bridge().await;
    bridge
        .get_events(after_cursor, expected_epoch, parsed_project, limit)
        .await
}

/// Creates a durable Dennett task or scheduled obligation (`POST /v1/tasks`).
#[tauri::command]
pub async fn create_task(
    state: State<'_, SharedBridge>,
    request: CreateTaskRequest,
) -> Result<TaskRecord, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.create_task(&request).await
}

/// Lists durable tasks filtered optionally by project or status (`GET /v1/tasks`).
#[tauri::command]
pub async fn list_tasks(
    state: State<'_, SharedBridge>,
    project_id: Option<String>,
    status: Option<String>,
) -> Result<ListTasksResponse, BridgeError> {
    let parsed_project = parse_optional_project_id(project_id.as_deref())?;
    let bridge = state.snapshot_bridge().await;
    bridge.list_tasks(parsed_project, status.as_deref()).await
}

/// Fetches full detail for a durable task including runs, timers, signals, effects, and open inbox cards.
#[tauri::command]
pub async fn get_task_detail(
    state: State<'_, SharedBridge>,
    task_id: String,
) -> Result<TaskDetailResponse, BridgeError> {
    let parsed_id = parse_task_id(&task_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.get_task_detail(parsed_id).await
}

/// Updates a task's title, goal, schedule, or budget (`PATCH /v1/tasks/{id}`, bumping `generation`).
#[tauri::command]
pub async fn update_task(
    state: State<'_, SharedBridge>,
    task_id: String,
    request: UpdateTaskRequest,
) -> Result<TaskRecord, BridgeError> {
    let parsed_id = parse_task_id(&task_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.update_task(parsed_id, &request).await
}

/// Delivers an idempotent signal to a durable task (`POST /v1/tasks/{id}/signals`).
#[tauri::command]
pub async fn send_task_signal(
    state: State<'_, SharedBridge>,
    task_id: String,
    request: TaskSignalRequest,
) -> Result<TaskSignalResponse, BridgeError> {
    let parsed_id = parse_task_id(&task_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.send_task_signal(parsed_id, &request).await
}

/// Answers a task's clarification request and resumes execution (`POST /v1/tasks/{id}/reply`).
#[tauri::command]
pub async fn reply_to_task(
    state: State<'_, SharedBridge>,
    task_id: String,
    request: ReplyTaskRequest,
) -> Result<TaskRecord, BridgeError> {
    let parsed_id = parse_task_id(&task_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.reply_to_task(parsed_id, &request).await
}

/// Issues targeted `Stop` for a single durable task without affecting sibling tasks (`POST /v1/tasks/{id}/cancel`).
#[tauri::command]
pub async fn cancel_task(
    state: State<'_, SharedBridge>,
    task_id: String,
) -> Result<CancelTaskResponse, BridgeError> {
    let parsed_id = parse_task_id(&task_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.cancel_task(parsed_id).await
}

/// Lists Action Inbox decision cards (`GET /v1/inbox`).
#[tauri::command]
pub async fn list_inbox_cards(
    state: State<'_, SharedBridge>,
    status: Option<String>,
    project_id: Option<String>,
) -> Result<ListInboxResponse, BridgeError> {
    let parsed_project = parse_optional_project_id(project_id.as_deref())?;
    let bridge = state.snapshot_bridge().await;
    bridge
        .list_inbox_cards(status.as_deref(), parsed_project)
        .await
}

/// Resolves an Action Inbox card with optimistic `expected_revision` checking (`POST /v1/inbox/{id}/resolve`).
#[tauri::command]
pub async fn resolve_inbox_card(
    state: State<'_, SharedBridge>,
    card_id: String,
    request: ResolveInboxCardRequest,
) -> Result<ResolveInboxCardResponse, BridgeError> {
    let parsed_id = parse_inbox_card_id(&card_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.resolve_inbox_card(parsed_id, &request).await
}

/// Lists live interactive permission requests (`GET /v1/permissions/requests`).
#[tauri::command]
pub async fn list_permission_requests(
    state: State<'_, SharedBridge>,
    status: Option<String>,
    session_id: Option<String>,
) -> Result<ListPermissionRequestsResponse, BridgeError> {
    let parsed_session = parse_optional_session_id(session_id.as_deref())?;
    let bridge = state.snapshot_bridge().await;
    bridge
        .list_permission_requests(status.as_deref(), parsed_session)
        .await
}

/// Resolves a live interactive permission request (`POST /v1/permissions/requests/{id}/resolve`).
#[tauri::command]
pub async fn resolve_permission_request(
    state: State<'_, SharedBridge>,
    request_id: String,
    request: ResolvePermissionRequest,
) -> Result<ResolvePermissionResponse, BridgeError> {
    let parsed_id = parse_permission_request_id(&request_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.resolve_permission_request(parsed_id, &request).await
}

/// Lists revocable capability grants (`GET /v1/grants`).
#[tauri::command]
pub async fn list_capability_grants(
    state: State<'_, SharedBridge>,
    project_id: Option<String>,
    include_revoked: Option<bool>,
) -> Result<ListCapabilityGrantsResponse, BridgeError> {
    let parsed_project = parse_optional_project_id(project_id.as_deref())?;
    let bridge = state.snapshot_bridge().await;
    bridge
        .list_capability_grants(parsed_project, include_revoked.unwrap_or(false))
        .await
}

/// Immediately revokes an active capability grant (`POST /v1/grants/{id}/revoke`).
#[tauri::command]
pub async fn revoke_capability_grant(
    state: State<'_, SharedBridge>,
    grant_id: String,
    request: Option<RevokeGrantRequest>,
) -> Result<CapabilityGrantRecord, BridgeError> {
    let parsed_id = parse_capability_grant_id(&grant_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge
        .revoke_capability_grant(parsed_id, &request.unwrap_or_default())
        .await
}

/// Sends a plain-text inter-agent coordination message (`POST /v1/agents/{id}/messages`).
#[tauri::command]
pub async fn send_agent_message(
    state: State<'_, SharedBridge>,
    recipient_agent_id: String,
    request: SendAgentMessageRequest,
) -> Result<SendAgentMessageResponse, BridgeError> {
    let parsed_id = parse_agent_id(&recipient_agent_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.send_agent_message(parsed_id, &request).await
}

/// Lists incoming plain-text messages for an agent (`GET /v1/agents/{id}/messages`).
#[tauri::command]
pub async fn list_agent_messages(
    state: State<'_, SharedBridge>,
    recipient_agent_id: String,
    undelivered_only: Option<bool>,
) -> Result<ListAgentMessagesResponse, BridgeError> {
    let parsed_id = parse_agent_id(&recipient_agent_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge
        .list_agent_messages(parsed_id, undelivered_only.unwrap_or(false))
        .await
}

/// Dispatches an idempotent external effect through Head (`POST /v1/effects/dispatch`).
#[tauri::command]
pub async fn dispatch_effect(
    state: State<'_, SharedBridge>,
    request: DispatchEffectRequest,
) -> Result<DispatchEffectResponse, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.dispatch_effect(&request).await
}

/// Lists tracked external effect claims (`GET /v1/effects`).
#[tauri::command]
pub async fn list_effects(
    state: State<'_, SharedBridge>,
    task_id: Option<String>,
    session_id: Option<String>,
    status: Option<String>,
) -> Result<ListEffectsResponse, BridgeError> {
    let parsed_task = parse_optional_task_id(task_id.as_deref())?;
    let parsed_session = parse_optional_session_id(session_id.as_deref())?;
    let bridge = state.snapshot_bridge().await;
    bridge
        .list_effects(parsed_task, parsed_session, status.as_deref())
        .await
}

/// Reconciles an `unknown` external effect claim without blind re-dispatch (`POST /v1/effects/{id}/reconcile`).
#[tauri::command]
pub async fn reconcile_effect(
    state: State<'_, SharedBridge>,
    effect_id: String,
    request: Option<ReconcileEffectRequest>,
) -> Result<ReconcileEffectResponse, BridgeError> {
    let parsed_id = parse_effect_claim_id(&effect_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge
        .reconcile_effect(parsed_id, &request.unwrap_or_default())
        .await
}
