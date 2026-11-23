//! Typed Tauri 2 IPC commands delegating directly to [`HeadBridge`].
//!
//! Every command acquires the shared bridge lock only long enough to clone the cheap
//! `Arc`-backed [`HeadBridge`], then releases the lock before performing async HTTP I/O
//! so `cancel_run` (`Stop`) is never blocked behind an in-flight `send_message` turn.

use std::str::FromStr;

use dennett_contracts::{
    AgentId, AgentRecord, ArtifactId, ArtifactRecord, CancelRunResponse, CreateAgentRequest,
    CreateArtifactRequest, CreateProjectRequest, CreateSessionRequest, DiagnosticsResponse,
    FlushOutboxResponse, ListAgentsResponse, ListArtifactsResponse, ListProjectsResponse,
    ListSessionsResponse, ProjectRecord, SendMessageRequest, SendMessageResponse,
    SessionDetailResponse, SessionId, SessionRecord, UpdateAgentConfigRequest,
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

fn parse_agent_id(raw: &str) -> Result<AgentId, BridgeError> {
    AgentId::from_str(raw.trim()).map_err(BridgeError::from)
}

fn parse_session_id(raw: &str) -> Result<SessionId, BridgeError> {
    SessionId::from_str(raw.trim()).map_err(BridgeError::from)
}

fn parse_artifact_id(raw: &str) -> Result<ArtifactId, BridgeError> {
    ArtifactId::from_str(raw.trim()).map_err(BridgeError::from)
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

/// Lists all workspace projects registered in `dennett-head`.
#[tauri::command]
pub async fn list_projects(
    state: State<'_, SharedBridge>,
) -> Result<ListProjectsResponse, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.list_projects().await
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

/// Manually triggers an idempotent flush of pending `memory_commit_outbox` items.
#[tauri::command]
pub async fn flush_memory_outbox(
    state: State<'_, SharedBridge>,
) -> Result<FlushOutboxResponse, BridgeError> {
    let bridge = state.snapshot_bridge().await;
    bridge.flush_memory_outbox().await
}
