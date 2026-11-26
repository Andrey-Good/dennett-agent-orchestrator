//! Typed Tauri 2 IPC commands delegating directly to [`HeadBridge`].
//!
//! Every command acquires the shared bridge lock only long enough to clone the cheap
//! `Arc`-backed [`HeadBridge`], then releases the lock before performing async HTTP I/O
//! so `cancel_run` (`Stop`) is never blocked behind an in-flight `send_message` turn.

use std::str::FromStr;

use dennett_contracts::{
    ActivateConnectionRequest, AgentConfigHistoryResponse, AgentId, AgentRecord, ArtifactId,
    ArtifactLifecycleRequest, ArtifactLifecycleResponse, ArtifactRecord, CancelRunResponse,
    CancelTaskResponse, CapabilityGrantId, CapabilityGrantRecord, CapabilityId, CapabilityKind,
    CapabilityProjectBinding, CapabilityRecord, CapabilityScopeKind, CommitMemoryNoteRequest,
    CommitMemoryNoteResponse, ConsolidationJob, ConsolidationJobId, CorrectNoteRequest,
    CreateAgentRequest, CreateArtifactRequest, CreateCapabilityRequest, CreateClaimRequest,
    CreateConsolidationJobRequest, CreateDocumentRequest, CreatePolicyRuleRequest,
    CreateProjectRequest, CreateSessionRequest, CreateTaskRequest, DeleteCapabilityResponse,
    DeleteProviderConnectionResponse, DiagnosticsResponse, DiscoverCapabilitiesRequest,
    DiscoverCapabilitiesResponse, DispatchEffectRequest, DispatchEffectResponse, DocumentId,
    EffectClaimId, EmergencyStopResponse, EnqueueFollowupRequest, EventStreamResponse,
    ExportRequest, ExportResponse, FlushOutboxResponse, FollowupId, ForgetRequest, ForgetResponse,
    ForkSessionRequest, GetDocumentVersionsResponse, HeadMemorySearchRequest,
    HeadMemorySearchResponse, ImportRequest, ImportResponse, InboxCardId, IndexState,
    ListAgentMessagesResponse, ListAgentsResponse, ListArtifactsResponse, ListCapabilitiesResponse,
    ListCapabilityGrantsResponse, ListClaimsResponse, ListConflictsResponse,
    ListConsolidationJobsResponse, ListDocumentsResponse, ListEffectsResponse, ListInboxResponse,
    ListPermissionRequestsResponse, ListPolicyRulesResponse, ListProjectFilesResponse,
    ListProjectsResponse, ListProviderConnectionsResponse, ListSecretsResponse,
    ListSessionsResponse, ListTasksResponse, ListTombstonesResponse, LiveRunResponse, MemoryClaim,
    MemoryConflict, MemoryDocument, MemoryNoteId, MemoryOverviewResponse, MemoryScope,
    NoteHistoryResponse, NoteMutationResponse, PermissionRequestId, PolicyRule, PolicyRuleId,
    ProbeConnectionResponse, ProjectFileContentResponse, ProjectId, ProjectLifecycleResponse,
    ProjectRecord, ProviderConnectionId, ProviderConnectionRecord, RebindProjectRequest,
    ReconcileEffectRequest, ReconcileEffectResponse, ReplyTaskRequest, ResolveConflictRequest,
    ResolveInboxCardRequest, ResolveInboxCardResponse, ResolvePermissionRequest,
    ResolvePermissionResponse, ResumeFollowupsResponse, RevokeGrantRequest, RuntimeCatalogResponse,
    SecretMetadataRecord, SendAgentMessageRequest, SendAgentMessageResponse, SendMessageRequest,
    SendMessageResponse, SessionDetailResponse, SessionFollowupRecord, SessionId, SessionRecord,
    SetProjectCapabilityBindingRequest, SetSecretRequest, StateSnapshotResponse,
    SteerSessionRequest, SupersedeClaimRequest, TaskDetailResponse, TaskId, TaskRecord,
    TaskSignalRequest, TaskSignalResponse, TriggerIndexRebuildRequest, TriggerIndexRebuildResponse,
    UpdateAgentConfigRequest, UpdateCapabilityRequest, UpdateDocumentRequest,
    UpdateFollowupRequest, UpdateProjectRequest, UpdateSessionRequest, UpdateTaskRequest,
    UpdateUserPreferencesRequest, UpsertProviderConnectionRequest, UserPreferences,
};
use tauri::State;
use tokio::sync::RwLock;

use crate::MicaAvailability;
use crate::bridge::{
    BridgeError, DesktopArtifactContentResponse, DesktopHealthSnapshot, HeadBridge,
};

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
        let (policy, owner_token) = {
            let guard = self.inner.read().await;
            (guard.policy(), guard.owner_token().map(str::to_owned))
        };
        let next = HeadBridge::with_owner_token(new_url, policy, owner_token)?;
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

fn parse_provider_connection_id(raw: &str) -> Result<ProviderConnectionId, BridgeError> {
    ProviderConnectionId::from_str(raw.trim()).map_err(BridgeError::from)
}

fn parse_capability_id(raw: &str) -> Result<CapabilityId, BridgeError> {
    CapabilityId::from_str(raw.trim()).map_err(BridgeError::from)
}

fn parse_memory_note_id(raw: &str) -> Result<MemoryNoteId, BridgeError> {
    MemoryNoteId::from_str(raw.trim()).map_err(BridgeError::from)
}

fn parse_document_id(raw: &str) -> Result<DocumentId, BridgeError> {
    DocumentId::from_str(raw.trim()).map_err(BridgeError::from)
}

fn parse_consolidation_job_id(raw: &str) -> Result<ConsolidationJobId, BridgeError> {
    ConsolidationJobId::from_str(raw.trim()).map_err(BridgeError::from)
}

fn parse_followup_id(raw: &str) -> Result<FollowupId, BridgeError> {
    FollowupId::from_str(raw.trim()).map_err(BridgeError::from)
}

fn parse_policy_rule_id(raw: &str) -> Result<PolicyRuleId, BridgeError> {
    PolicyRuleId::from_str(raw.trim()).map_err(BridgeError::from)
}

fn parse_optional_memory_scope(raw: Option<&str>) -> Result<Option<MemoryScope>, BridgeError> {
    raw.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| MemoryScope::try_new(s).map_err(BridgeError::from))
        .transpose()
}

fn parse_optional_capability_scope_kind(
    raw: Option<&str>,
) -> Result<Option<CapabilityScopeKind>, BridgeError> {
    raw.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| CapabilityScopeKind::from_str(s).map_err(BridgeError::from))
        .transpose()
}

fn parse_optional_capability_kind(
    raw: Option<&str>,
) -> Result<Option<CapabilityKind>, BridgeError> {
    raw.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| CapabilityKind::from_str(s).map_err(BridgeError::from))
        .transpose()
}

/// Returns whether native Windows 11 Mica backdrop is available on this host.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri::command extracts managed State by value"
)]
pub fn is_mica_available(mica: State<'_, MicaAvailability>) -> bool {
    mica.is_available()
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

/// Creates a new conversation session (project-bound or standalone when `project_id` is omitted/null).
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

/// Renames a project, attaches/rebinds its workspace folder, and/or toggles pin state (`PATCH /v1/projects/{project_id}`).
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

/// Renames an existing conversation session inline and/or toggles pin state (`PATCH /v1/sessions/{session_id}`).
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

// ============================================================================
// Phase D3 IPC Commands: SecretBroker, Providers, Capabilities, Memory Fabric
// ============================================================================

/// Lists write-only `SecretBroker` slot metadata without exposing secret values (`GET /v1/secrets`).
#[tauri::command]
pub async fn list_secrets(
    state: State<'_, SharedBridge>,
) -> Result<ListSecretsResponse, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.list_secrets().await
}

/// Stores or rotates a write-only secret in `SecretBroker` (`PUT /v1/secrets/{slot_key}`).
#[tauri::command]
pub async fn set_secret(
    state: State<'_, SharedBridge>,
    slot_key: String,
    request: SetSecretRequest,
) -> Result<SecretMetadataRecord, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.set_secret(&slot_key, &request).await
}

/// Deletes a secret slot from `SecretBroker` (`DELETE /v1/secrets/{slot_key}`).
#[tauri::command]
pub async fn delete_secret(
    state: State<'_, SharedBridge>,
    slot_key: String,
) -> Result<SecretMetadataRecord, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.delete_secret(&slot_key).await
}

/// Lists configured provider connections and read-only planned provider descriptors (`GET /v1/runtime/connections`).
#[tauri::command]
pub async fn list_provider_connections(
    state: State<'_, SharedBridge>,
) -> Result<ListProviderConnectionsResponse, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.list_provider_connections().await
}

/// Creates a runtime provider connection (`POST /v1/runtime/connections`).
#[tauri::command]
pub async fn create_provider_connection(
    state: State<'_, SharedBridge>,
    request: UpsertProviderConnectionRequest,
) -> Result<ProviderConnectionRecord, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.create_provider_connection(&request).await
}

/// Updates an existing runtime provider connection (`PATCH /v1/runtime/connections/{id}`).
#[tauri::command]
pub async fn update_provider_connection(
    state: State<'_, SharedBridge>,
    connection_id: String,
    request: UpsertProviderConnectionRequest,
) -> Result<ProviderConnectionRecord, BridgeError> {
    let parsed_id = parse_provider_connection_id(&connection_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.update_provider_connection(parsed_id, &request).await
}

/// Deletes a runtime provider connection (`DELETE /v1/runtime/connections/{id}`).
#[tauri::command]
pub async fn delete_provider_connection(
    state: State<'_, SharedBridge>,
    connection_id: String,
) -> Result<DeleteProviderConnectionResponse, BridgeError> {
    let parsed_id = parse_provider_connection_id(&connection_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.delete_provider_connection(parsed_id).await
}

/// Probes a runtime provider connection and loads live models (`POST /v1/runtime/connections/{id}/probe`).
#[tauri::command]
pub async fn probe_provider_connection(
    state: State<'_, SharedBridge>,
    connection_id: String,
) -> Result<ProbeConnectionResponse, BridgeError> {
    let parsed_id = parse_provider_connection_id(&connection_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.probe_provider_connection(parsed_id).await
}

/// Activates a runtime provider connection as active default (`POST /v1/runtime/connections/{id}/activate`).
#[tauri::command]
pub async fn activate_provider_connection(
    state: State<'_, SharedBridge>,
    connection_id: String,
    request: Option<ActivateConnectionRequest>,
) -> Result<ProviderConnectionRecord, BridgeError> {
    let parsed_id = parse_provider_connection_id(&connection_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge
        .activate_provider_connection(parsed_id, &request.unwrap_or_default())
        .await
}

/// Lists registered skills, plugins, and MCP servers (`GET /v1/capabilities`).
#[tauri::command]
pub async fn list_capabilities(
    state: State<'_, SharedBridge>,
    project_id: Option<String>,
    scope_kind: Option<String>,
    kind: Option<String>,
) -> Result<ListCapabilitiesResponse, BridgeError> {
    let parsed_project = parse_optional_project_id(project_id.as_deref())?;
    let parsed_scope_kind = parse_optional_capability_scope_kind(scope_kind.as_deref())?;
    let parsed_kind = parse_optional_capability_kind(kind.as_deref())?;
    let bridge = state.snapshot_bridge().await;
    bridge
        .list_capabilities(parsed_project, parsed_scope_kind, parsed_kind)
        .await
}

/// Registers a new skill, MCP server, or plugin capability (`POST /v1/capabilities`).
#[tauri::command]
pub async fn create_capability(
    state: State<'_, SharedBridge>,
    request: CreateCapabilityRequest,
) -> Result<CapabilityRecord, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.create_capability(&request).await
}

/// Scans project directories and read-only personal profile candidates (`POST /v1/capabilities/discover`).
#[tauri::command]
pub async fn discover_capabilities(
    state: State<'_, SharedBridge>,
    request: DiscoverCapabilitiesRequest,
) -> Result<DiscoverCapabilitiesResponse, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.discover_capabilities(&request).await
}

/// Updates a registered capability (`PATCH /v1/capabilities/{id}`).
#[tauri::command]
pub async fn update_capability(
    state: State<'_, SharedBridge>,
    capability_id: String,
    request: UpdateCapabilityRequest,
) -> Result<CapabilityRecord, BridgeError> {
    let parsed_id = parse_capability_id(&capability_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.update_capability(parsed_id, &request).await
}

/// Removes a capability from Dennett's registry (`DELETE /v1/capabilities/{id}`).
#[tauri::command]
pub async fn delete_capability(
    state: State<'_, SharedBridge>,
    capability_id: String,
) -> Result<DeleteCapabilityResponse, BridgeError> {
    let parsed_id = parse_capability_id(&capability_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.delete_capability(parsed_id).await
}

/// Rolls back a capability to its previous snapshot (`POST /v1/capabilities/{id}/rollback`).
#[tauri::command]
pub async fn rollback_capability(
    state: State<'_, SharedBridge>,
    capability_id: String,
) -> Result<CapabilityRecord, BridgeError> {
    let parsed_id = parse_capability_id(&capability_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.rollback_capability(parsed_id).await
}

/// Sets a per-project capability activation binding (`PUT /v1/projects/{project_id}/capabilities/{capability_id}`).
#[tauri::command]
pub async fn set_project_capability_binding(
    state: State<'_, SharedBridge>,
    project_id: String,
    capability_id: String,
    request: SetProjectCapabilityBindingRequest,
) -> Result<CapabilityProjectBinding, BridgeError> {
    let parsed_project = parse_project_id(&project_id)?;
    let parsed_cap = parse_capability_id(&capability_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge
        .set_project_capability_binding(parsed_project, parsed_cap, &request)
        .await
}

/// Commits a memory note via `Head`'s durable outbox (`POST /v1/memory/commits`).
#[tauri::command]
pub async fn commit_memory_note(
    state: State<'_, SharedBridge>,
    request: CommitMemoryNoteRequest,
) -> Result<CommitMemoryNoteResponse, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.commit_memory_note(&request).await
}

/// Fetches the V02-safe aggregated Memory Fabric overview (`GET /v1/memory/overview`).
#[tauri::command]
pub async fn get_memory_overview(
    state: State<'_, SharedBridge>,
    scope: Option<String>,
) -> Result<MemoryOverviewResponse, BridgeError> {
    let parsed_scope = parse_optional_memory_scope(scope.as_deref())?;
    let bridge = state.snapshot_bridge().await;
    bridge.get_memory_overview(parsed_scope.as_ref()).await
}

/// Appends a correction revision to a memory note (`POST /v1/memory/notes/{id}/correct`).
#[tauri::command]
pub async fn correct_memory_note(
    state: State<'_, SharedBridge>,
    request: CorrectNoteRequest,
) -> Result<NoteMutationResponse, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.correct_memory_note(&request).await
}

/// Fetches immutable revision history for a memory note (`GET /v1/memory/notes/{id}/history`).
#[tauri::command]
pub async fn get_memory_note_history(
    state: State<'_, SharedBridge>,
    note_id: String,
) -> Result<NoteHistoryResponse, BridgeError> {
    let parsed_id = parse_memory_note_id(&note_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.get_memory_note_history(parsed_id).await
}

/// Lists memory claims filtered optionally by scope and topic (`GET /v1/memory/claims`).
#[tauri::command]
pub async fn list_memory_claims(
    state: State<'_, SharedBridge>,
    scope: Option<String>,
    topic_key: Option<String>,
) -> Result<ListClaimsResponse, BridgeError> {
    let parsed_scope = parse_optional_memory_scope(scope.as_deref())?;
    let bridge = state.snapshot_bridge().await;
    bridge
        .list_memory_claims(parsed_scope.as_ref(), topic_key.as_deref())
        .await
}

/// Creates a structured memory claim (`POST /v1/memory/claims`).
#[tauri::command]
pub async fn create_memory_claim(
    state: State<'_, SharedBridge>,
    request: CreateClaimRequest,
) -> Result<MemoryClaim, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.create_memory_claim(&request).await
}

/// Supersedes an existing memory claim (`POST /v1/memory/claims/{id}/supersede`).
#[tauri::command]
pub async fn supersede_memory_claim(
    state: State<'_, SharedBridge>,
    request: SupersedeClaimRequest,
) -> Result<MemoryClaim, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.supersede_memory_claim(&request).await
}

/// Lists memory conflicts (`GET /v1/memory/conflicts`).
#[tauri::command]
pub async fn list_memory_conflicts(
    state: State<'_, SharedBridge>,
    scope: Option<String>,
) -> Result<ListConflictsResponse, BridgeError> {
    let parsed_scope = parse_optional_memory_scope(scope.as_deref())?;
    let bridge = state.snapshot_bridge().await;
    bridge.list_memory_conflicts(parsed_scope.as_ref()).await
}

/// Resolves an open memory conflict (`POST /v1/memory/conflicts/{id}/resolve`).
#[tauri::command]
pub async fn resolve_memory_conflict(
    state: State<'_, SharedBridge>,
    request: ResolveConflictRequest,
) -> Result<MemoryConflict, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.resolve_memory_conflict(&request).await
}

/// Lists memory dossier documents (`GET /v1/memory/documents`).
#[tauri::command]
pub async fn list_memory_documents(
    state: State<'_, SharedBridge>,
    scope: Option<String>,
) -> Result<ListDocumentsResponse, BridgeError> {
    let parsed_scope = parse_optional_memory_scope(scope.as_deref())?;
    let bridge = state.snapshot_bridge().await;
    bridge.list_memory_documents(parsed_scope.as_ref()).await
}

/// Creates a version-1 memory dossier document (`POST /v1/memory/documents`).
#[tauri::command]
pub async fn create_memory_document(
    state: State<'_, SharedBridge>,
    request: CreateDocumentRequest,
) -> Result<MemoryDocument, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.create_memory_document(&request).await
}

/// Retrieves a single memory dossier document (`GET /v1/memory/documents/{id}`).
#[tauri::command]
pub async fn get_memory_document(
    state: State<'_, SharedBridge>,
    document_id: String,
) -> Result<MemoryDocument, BridgeError> {
    let parsed_id = parse_document_id(&document_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.get_memory_document(parsed_id).await
}

/// Updates a memory dossier document with optimistic `expected_version` concurrency (`PUT /v1/memory/documents/{id}`).
#[tauri::command]
pub async fn update_memory_document(
    state: State<'_, SharedBridge>,
    request: UpdateDocumentRequest,
) -> Result<MemoryDocument, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.update_memory_document(&request).await
}

/// Fetches immutable version history for a memory dossier document (`GET /v1/memory/documents/{id}/versions`).
#[tauri::command]
pub async fn get_memory_document_versions(
    state: State<'_, SharedBridge>,
    document_id: String,
) -> Result<GetDocumentVersionsResponse, BridgeError> {
    let parsed_id = parse_document_id(&document_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.get_memory_document_versions(parsed_id).await
}

/// Forgets a memory entity with search-index purge and tombstone (`POST /v1/memory/forget`).
#[tauri::command]
pub async fn forget_memory_item(
    state: State<'_, SharedBridge>,
    request: ForgetRequest,
) -> Result<ForgetResponse, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.forget_memory_item(&request).await
}

/// Lists content-free forget audit tombstones (`GET /v1/memory/forget/tombstones`).
#[tauri::command]
pub async fn list_memory_tombstones(
    state: State<'_, SharedBridge>,
    scope: Option<String>,
) -> Result<ListTombstonesResponse, BridgeError> {
    let parsed_scope = parse_optional_memory_scope(scope.as_deref())?;
    let bridge = state.snapshot_bridge().await;
    bridge.list_memory_tombstones(parsed_scope.as_ref()).await
}

/// Exports a deterministic, secret-free memory package (`POST /v1/memory/export`).
#[tauri::command]
pub async fn export_memory_package(
    state: State<'_, SharedBridge>,
    request: ExportRequest,
) -> Result<ExportResponse, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.export_memory_package(&request).await
}

/// Imports an external memory package strictly as `external_untrusted` (`POST /v1/memory/import`).
#[tauri::command]
pub async fn import_memory_package(
    state: State<'_, SharedBridge>,
    request: ImportRequest,
) -> Result<ImportResponse, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.import_memory_package(&request).await
}

/// Triggers a full search index rebuild for a memory scope (`POST /v1/memory/index/rebuild`).
#[tauri::command]
pub async fn trigger_memory_index_rebuild(
    state: State<'_, SharedBridge>,
    request: TriggerIndexRebuildRequest,
) -> Result<TriggerIndexRebuildResponse, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.trigger_memory_index_rebuild(&request).await
}

/// Lists memory consolidation jobs (`GET /v1/memory/consolidation/jobs`).
#[tauri::command]
pub async fn list_memory_consolidation_jobs(
    state: State<'_, SharedBridge>,
    scope: Option<String>,
) -> Result<ListConsolidationJobsResponse, BridgeError> {
    let parsed_scope = parse_optional_memory_scope(scope.as_deref())?;
    let bridge = state.snapshot_bridge().await;
    bridge
        .list_memory_consolidation_jobs(parsed_scope.as_ref())
        .await
}

/// Creates a background memory consolidation job (`POST /v1/memory/consolidation/jobs`).
#[tauri::command]
pub async fn create_memory_consolidation_job(
    state: State<'_, SharedBridge>,
    request: CreateConsolidationJobRequest,
) -> Result<ConsolidationJob, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.create_memory_consolidation_job(&request).await
}

/// Pauses a memory consolidation job at its durable checkpoint (`POST /v1/memory/consolidation/jobs/{id}/pause`).
#[tauri::command]
pub async fn pause_memory_consolidation_job(
    state: State<'_, SharedBridge>,
    job_id: String,
) -> Result<ConsolidationJob, BridgeError> {
    let parsed_id = parse_consolidation_job_id(&job_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.pause_memory_consolidation_job(parsed_id).await
}

/// Resumes a paused memory consolidation job (`POST /v1/memory/consolidation/jobs/{id}/resume`).
#[tauri::command]
pub async fn resume_memory_consolidation_job(
    state: State<'_, SharedBridge>,
    job_id: String,
) -> Result<ConsolidationJob, BridgeError> {
    let parsed_id = parse_consolidation_job_id(&job_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.resume_memory_consolidation_job(parsed_id).await
}

// ============================================================================
// Phase D3+ IPC Commands: Live Session Runs, Follow-Up Queue & Live Steering
// ============================================================================

/// Fetches the active streaming run snapshot and durable follow-up queue (`GET /v1/sessions/{id}/live`).
#[tauri::command]
pub async fn get_session_live(
    state: State<'_, SharedBridge>,
    session_id: String,
) -> Result<LiveRunResponse, BridgeError> {
    let parsed_session = parse_session_id(&session_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.get_session_live(parsed_session).await
}

/// Appends a durable follow-up message for a session (`POST /v1/sessions/{id}/followups`).
#[tauri::command]
pub async fn enqueue_followup(
    state: State<'_, SharedBridge>,
    session_id: String,
    content: String,
    idempotency_key: Option<String>,
) -> Result<SessionFollowupRecord, BridgeError> {
    let parsed_session = parse_session_id(&session_id)?;
    let bridge = state.snapshot_bridge().await;
    let req = EnqueueFollowupRequest {
        content,
        idempotency_key,
    };
    bridge.enqueue_followup(parsed_session, &req).await
}

/// Updates a queued or paused follow-up message (`PATCH /v1/sessions/{id}/followups/{fid}`).
#[tauri::command]
pub async fn update_followup(
    state: State<'_, SharedBridge>,
    session_id: String,
    followup_id: String,
    content: String,
) -> Result<SessionFollowupRecord, BridgeError> {
    let parsed_session = parse_session_id(&session_id)?;
    let parsed_followup = parse_followup_id(&followup_id)?;
    let bridge = state.snapshot_bridge().await;
    let req = UpdateFollowupRequest { content };
    bridge
        .update_followup(parsed_session, parsed_followup, &req)
        .await
}

/// Removes one follow-up message from a session's queue (`DELETE /v1/sessions/{id}/followups/{fid}`).
#[tauri::command]
pub async fn delete_followup(
    state: State<'_, SharedBridge>,
    session_id: String,
    followup_id: String,
) -> Result<(), BridgeError> {
    let parsed_session = parse_session_id(&session_id)?;
    let parsed_followup = parse_followup_id(&followup_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge
        .delete_followup(parsed_session, parsed_followup)
        .await
}

/// Clears all pending follow-up messages for a session (`DELETE /v1/sessions/{id}/followups`).
#[tauri::command]
pub async fn clear_followups(
    state: State<'_, SharedBridge>,
    session_id: String,
) -> Result<(), BridgeError> {
    let parsed_session = parse_session_id(&session_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.clear_followups(parsed_session).await
}

/// Resumes paused follow-up messages for a session (`POST /v1/sessions/{id}/followups/resume`).
#[tauri::command]
pub async fn resume_followups(
    state: State<'_, SharedBridge>,
    session_id: String,
) -> Result<ResumeFollowupsResponse, BridgeError> {
    let parsed_session = parse_session_id(&session_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.resume_followups(parsed_session).await
}

/// Interrupts any active run and immediately steers the session (`POST /v1/sessions/{id}/steer`).
#[tauri::command]
pub async fn steer_session(
    state: State<'_, SharedBridge>,
    session_id: String,
    content: String,
    client_message_id: Option<String>,
    timeout_ms: Option<u64>,
) -> Result<SendMessageResponse, BridgeError> {
    let parsed_session = parse_session_id(&session_id)?;
    let bridge = state.snapshot_bridge().await;
    let req = SteerSessionRequest {
        content,
        client_message_id,
        timeout_ms,
    };
    bridge.steer_session(parsed_session, &req).await
}

// ============================================================================
// Phase D3+ Audit Closure IPC Commands
// ============================================================================

/// Fetches immutable raw bytes (text + base64) for an artifact revision (`GET /v1/sessions/{id}/artifacts/{aid}/content`).
#[tauri::command]
pub async fn get_artifact_content(
    state: State<'_, SharedBridge>,
    session_id: String,
    artifact_id: String,
) -> Result<DesktopArtifactContentResponse, BridgeError> {
    let parsed_session = parse_session_id(&session_id)?;
    let parsed_artifact = parse_artifact_id(&artifact_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge
        .get_artifact_content(parsed_session, parsed_artifact)
        .await
}

/// Lists immutable configuration revisions for an agent (`GET /v1/agents/{id}/config-versions`).
#[tauri::command]
pub async fn get_agent_config_history(
    state: State<'_, SharedBridge>,
    agent_id: String,
) -> Result<AgentConfigHistoryResponse, BridgeError> {
    let parsed_agent = parse_agent_id(&agent_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.get_agent_config_history(parsed_agent).await
}

/// Lists configured external policy rules (`GET /v1/policies`).
#[tauri::command]
pub async fn list_policy_rules(
    state: State<'_, SharedBridge>,
) -> Result<ListPolicyRulesResponse, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.list_policy_rules().await
}

/// Creates a new external policy rule (`POST /v1/policies`).
#[tauri::command]
pub async fn create_policy_rule(
    state: State<'_, SharedBridge>,
    request: CreatePolicyRuleRequest,
) -> Result<PolicyRule, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.create_policy_rule(&request).await
}

/// Deletes an external policy rule (`DELETE /v1/policies/{id}`).
#[tauri::command]
pub async fn delete_policy_rule(
    state: State<'_, SharedBridge>,
    rule_id: String,
) -> Result<(), BridgeError> {
    let parsed_rule = parse_policy_rule_id(&rule_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.delete_policy_rule(parsed_rule).await
}

/// Forks an existing session up to a chosen message (`POST /v1/sessions/{id}/fork`).
#[tauri::command]
pub async fn fork_session(
    state: State<'_, SharedBridge>,
    session_id: String,
    request: ForkSessionRequest,
) -> Result<SessionRecord, BridgeError> {
    let parsed_session = parse_session_id(&session_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.fork_session(parsed_session, &request).await
}

/// Deletes a persisted session record (`DELETE /v1/sessions/{id}`).
#[tauri::command]
pub async fn delete_session(
    state: State<'_, SharedBridge>,
    session_id: String,
) -> Result<(), BridgeError> {
    let parsed_session = parse_session_id(&session_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.delete_session(parsed_session).await
}

/// Retrieves persisted owner preferences (`GET /v1/preferences`).
#[tauri::command]
pub async fn get_preferences(
    state: State<'_, SharedBridge>,
) -> Result<UserPreferences, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.get_preferences().await
}

/// Updates persisted owner preferences (`PATCH /v1/preferences`).
#[tauri::command]
pub async fn update_preferences(
    state: State<'_, SharedBridge>,
    request: UpdateUserPreferencesRequest,
) -> Result<UserPreferences, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.update_preferences(&request).await
}

/// Reads a bounded workspace file preview (`GET /v1/projects/{id}/file?path=...`).
#[tauri::command]
pub async fn get_project_file_content(
    state: State<'_, SharedBridge>,
    project_id: String,
    relative_path: String,
) -> Result<ProjectFileContentResponse, BridgeError> {
    let parsed_project = parse_project_id(&project_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge
        .get_project_file_content(parsed_project, &relative_path)
        .await
}

/// Cancels all active runs and tasks immediately (`POST /v1/runs/emergency-stop`).
#[tauri::command]
pub async fn emergency_stop_all(
    state: State<'_, SharedBridge>,
) -> Result<EmergencyStopResponse, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.emergency_stop_all().await
}

/// Triggers immediate execution of a durable task (`POST /v1/tasks/{id}/run-now`).
#[tauri::command]
pub async fn run_task_now(
    state: State<'_, SharedBridge>,
    task_id: String,
) -> Result<TaskRecord, BridgeError> {
    let parsed_task = parse_task_id(&task_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.run_task_now(parsed_task).await
}

/// Pauses a scheduled or queued durable task (`POST /v1/tasks/{id}/pause`).
#[tauri::command]
pub async fn pause_task(
    state: State<'_, SharedBridge>,
    task_id: String,
) -> Result<TaskRecord, BridgeError> {
    let parsed_task = parse_task_id(&task_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.pause_task(parsed_task).await
}

/// Resumes a paused durable task (`POST /v1/tasks/{id}/resume`).
#[tauri::command]
pub async fn resume_task(
    state: State<'_, SharedBridge>,
    task_id: String,
) -> Result<TaskRecord, BridgeError> {
    let parsed_task = parse_task_id(&task_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.resume_task(parsed_task).await
}

/// Deletes a durable task record (`DELETE /v1/tasks/{id}`).
#[tauri::command]
pub async fn delete_task(
    state: State<'_, SharedBridge>,
    task_id: String,
) -> Result<(), BridgeError> {
    let parsed_task = parse_task_id(&task_id)?;
    let bridge = state.snapshot_bridge().await;
    bridge.delete_task(parsed_task).await
}

/// Queries the memory search index watermark and health status (`GET /v1/memory/index/status`).
#[tauri::command]
pub async fn get_memory_index_status(
    state: State<'_, SharedBridge>,
    scope: Option<String>,
) -> Result<IndexState, BridgeError> {
    let parsed_scope = parse_optional_memory_scope(scope.as_deref())?;
    let bridge = state.snapshot_bridge().await;
    bridge.get_memory_index_status(parsed_scope.as_ref()).await
}
