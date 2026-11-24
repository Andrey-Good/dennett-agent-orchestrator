import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

import type {
  AgentRecord,
  ArtifactLifecycleRequest,
  ArtifactLifecycleResponse,
  ArtifactRecord,
  CancelRunResponse,
  CancelTaskResponse,
  CapabilityGrantRecord,
  CreateAgentRequest,
  CreateArtifactRequest,
  CreateProjectRequest,
  CreateSessionRequest,
  CreateTaskRequest,
  DesktopHealthSnapshot,
  DiagnosticsResponse,
  DispatchEffectRequest,
  DispatchEffectResponse,
  EventStreamResponse,
  FlushOutboxResponse,
  HeadMemorySearchRequest,
  HeadMemorySearchResponse,
  ListAgentMessagesResponse,
  ListAgentsResponse,
  ListArtifactsResponse,
  ListCapabilityGrantsResponse,
  ListEffectsResponse,
  ListInboxResponse,
  ListPermissionRequestsResponse,
  ListProjectFilesResponse,
  ListProjectsResponse,
  ListSessionsResponse,
  ListTasksResponse,
  ProjectLifecycleResponse,
  ProjectRecord,
  RebindProjectRequest,
  ReconcileEffectRequest,
  ReconcileEffectResponse,
  ReplyTaskRequest,
  ResolveInboxCardRequest,
  ResolveInboxCardResponse,
  ResolvePermissionRequest,
  ResolvePermissionResponse,
  RevokeGrantRequest,
  RuntimeCatalogResponse,
  SendAgentMessageRequest,
  SendAgentMessageResponse,
  SendMessageRequest,
  SendMessageResponse,
  SessionDetailResponse,
  SessionRecord,
  StateSnapshotResponse,
  TaskDetailResponse,
  TaskRecord,
  TaskSignalRequest,
  TaskSignalResponse,
  UpdateAgentConfigRequest,
  UpdateProjectRequest,
  UpdateSessionRequest,
  UpdateTaskRequest,
} from "./contracts";

export interface DesktopBridge {
  readonly isNativeTauri: boolean;
  readonly supportsNativeFolderPicker: boolean;
  getHeadHealth(): Promise<DesktopHealthSnapshot>;
  setHeadUrl(headUrl: string): Promise<DesktopHealthSnapshot>;
  getRuntimeCatalog(refresh?: boolean): Promise<RuntimeCatalogResponse>;
  pickWorkspaceFolder(): Promise<string | null>;
  listProjects(includeArchived?: boolean): Promise<ListProjectsResponse>;
  createProject(request: CreateProjectRequest): Promise<ProjectRecord>;
  updateProject(projectId: string, request: UpdateProjectRequest): Promise<ProjectRecord>;
  rebindProject(projectId: string, request: RebindProjectRequest): Promise<ProjectLifecycleResponse>;
  detachProjectWorkspace(projectId: string): Promise<ProjectLifecycleResponse>;
  archiveProject(projectId: string): Promise<ProjectLifecycleResponse>;
  unarchiveProject(projectId: string): Promise<ProjectLifecycleResponse>;
  removeProject(projectId: string): Promise<ProjectLifecycleResponse>;
  deleteProject(projectId: string, confirm: boolean): Promise<ProjectLifecycleResponse>;
  listProjectFiles(projectId: string, subpath?: string): Promise<ListProjectFilesResponse>;
  listAgents(): Promise<ListAgentsResponse>;
  createAgent(request: CreateAgentRequest): Promise<AgentRecord>;
  updateAgentConfig(agentId: string, request: UpdateAgentConfigRequest): Promise<AgentRecord>;
  sendAgentMessage(
    recipientAgentId: string,
    request: SendAgentMessageRequest,
  ): Promise<SendAgentMessageResponse>;
  listAgentMessages(
    recipientAgentId: string,
    undeliveredOnly?: boolean,
  ): Promise<ListAgentMessagesResponse>;
  listSessions(): Promise<ListSessionsResponse>;
  createSession(request: CreateSessionRequest): Promise<SessionRecord>;
  updateSession(sessionId: string, request: UpdateSessionRequest): Promise<SessionRecord>;
  getSessionDetail(sessionId: string): Promise<SessionDetailResponse>;
  sendMessage(sessionId: string, request: SendMessageRequest): Promise<SendMessageResponse>;
  cancelRun(sessionId: string): Promise<CancelRunResponse>;
  listArtifacts(sessionId: string): Promise<ListArtifactsResponse>;
  createArtifact(sessionId: string, request: CreateArtifactRequest): Promise<ArtifactRecord>;
  getArtifact(sessionId: string, artifactId: string): Promise<ArtifactRecord>;
  listArtifactRevisions(
    sessionId: string,
    artifactIdOrName: string,
  ): Promise<ListArtifactsResponse>;
  applyArtifactLifecycle(
    sessionId: string,
    artifactId: string,
    request: ArtifactLifecycleRequest,
  ): Promise<ArtifactLifecycleResponse>;
  getStateSnapshot(
    projectId?: string | null,
    includeArchived?: boolean,
  ): Promise<StateSnapshotResponse>;
  getEvents(
    afterCursor: number,
    expectedEpoch?: number | null,
    projectId?: string | null,
    limit?: number | null,
  ): Promise<EventStreamResponse>;
  createTask(request: CreateTaskRequest): Promise<TaskRecord>;
  listTasks(projectId?: string | null, status?: string | null): Promise<ListTasksResponse>;
  getTaskDetail(taskId: string): Promise<TaskDetailResponse>;
  updateTask(taskId: string, request: UpdateTaskRequest): Promise<TaskRecord>;
  sendTaskSignal(taskId: string, request: TaskSignalRequest): Promise<TaskSignalResponse>;
  replyToTask(taskId: string, request: ReplyTaskRequest): Promise<TaskRecord>;
  cancelTask(taskId: string): Promise<CancelTaskResponse>;
  listInboxCards(status?: string | null, projectId?: string | null): Promise<ListInboxResponse>;
  resolveInboxCard(
    cardId: string,
    request: ResolveInboxCardRequest,
  ): Promise<ResolveInboxCardResponse>;
  listPermissionRequests(
    status?: string | null,
    sessionId?: string | null,
  ): Promise<ListPermissionRequestsResponse>;
  resolvePermissionRequest(
    requestId: string,
    request: ResolvePermissionRequest,
  ): Promise<ResolvePermissionResponse>;
  listCapabilityGrants(
    projectId?: string | null,
    includeRevoked?: boolean,
  ): Promise<ListCapabilityGrantsResponse>;
  revokeCapabilityGrant(
    grantId: string,
    request?: RevokeGrantRequest,
  ): Promise<CapabilityGrantRecord>;
  dispatchEffect(request: DispatchEffectRequest): Promise<DispatchEffectResponse>;
  listEffects(
    taskId?: string | null,
    sessionId?: string | null,
    status?: string | null,
  ): Promise<ListEffectsResponse>;
  reconcileEffect(
    effectId: string,
    request?: ReconcileEffectRequest,
  ): Promise<ReconcileEffectResponse>;
  listDiagnostics(limit?: number): Promise<DiagnosticsResponse>;
  flushMemoryOutbox(): Promise<FlushOutboxResponse>;
  searchMemory(request: HeadMemorySearchRequest): Promise<HeadMemorySearchResponse>;
}

export interface WindowControls {
  readonly supported: boolean;
  minimize(): Promise<void>;
  toggleMaximize(): Promise<void>;
  close(): Promise<void>;
  startDragging(): Promise<void>;
  isMaximized(): Promise<boolean>;
}

export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function createTauriBridge(): DesktopBridge {
  return {
    isNativeTauri: true,
    supportsNativeFolderPicker: true,
    getHeadHealth: () => invoke<DesktopHealthSnapshot>("get_head_health"),
    setHeadUrl: (headUrl: string) => invoke<DesktopHealthSnapshot>("set_head_url", { headUrl }),
    getRuntimeCatalog: (refresh?: boolean) =>
      invoke<RuntimeCatalogResponse>("get_runtime_catalog", { refresh: refresh ?? false }),
    pickWorkspaceFolder: () => invoke<string | null>("pick_workspace_folder"),
    listProjects: (includeArchived?: boolean) =>
      invoke<ListProjectsResponse>("list_projects", {
        includeArchived: includeArchived ?? false,
      }),
    createProject: (request: CreateProjectRequest) =>
      invoke<ProjectRecord>("create_project", { request }),
    updateProject: (projectId: string, request: UpdateProjectRequest) =>
      invoke<ProjectRecord>("update_project", { projectId, request }),
    rebindProject: (projectId: string, request: RebindProjectRequest) =>
      invoke<ProjectLifecycleResponse>("rebind_project", { projectId, request }),
    detachProjectWorkspace: (projectId: string) =>
      invoke<ProjectLifecycleResponse>("detach_project_workspace", { projectId }),
    archiveProject: (projectId: string) =>
      invoke<ProjectLifecycleResponse>("archive_project", { projectId }),
    unarchiveProject: (projectId: string) =>
      invoke<ProjectLifecycleResponse>("unarchive_project", { projectId }),
    removeProject: (projectId: string) =>
      invoke<ProjectLifecycleResponse>("remove_project", { projectId }),
    deleteProject: (projectId: string, confirm: boolean) =>
      invoke<ProjectLifecycleResponse>("delete_project", { projectId, confirm }),
    listProjectFiles: (projectId: string, subpath?: string) =>
      invoke<ListProjectFilesResponse>("list_project_files", {
        projectId,
        subpath: subpath ?? null,
      }),
    listAgents: () => invoke<ListAgentsResponse>("list_agents"),
    createAgent: (request: CreateAgentRequest) => invoke<AgentRecord>("create_agent", { request }),
    updateAgentConfig: (agentId: string, request: UpdateAgentConfigRequest) =>
      invoke<AgentRecord>("update_agent_config", { agentId, request }),
    sendAgentMessage: (recipientAgentId: string, request: SendAgentMessageRequest) =>
      invoke<SendAgentMessageResponse>("send_agent_message", { recipientAgentId, request }),
    listAgentMessages: (recipientAgentId: string, undeliveredOnly?: boolean) =>
      invoke<ListAgentMessagesResponse>("list_agent_messages", {
        recipientAgentId,
        undeliveredOnly: undeliveredOnly ?? false,
      }),
    listSessions: () => invoke<ListSessionsResponse>("list_sessions"),
    createSession: (request: CreateSessionRequest) =>
      invoke<SessionRecord>("create_session", { request }),
    updateSession: (sessionId: string, request: UpdateSessionRequest) =>
      invoke<SessionRecord>("update_session", { sessionId, request }),
    getSessionDetail: (sessionId: string) =>
      invoke<SessionDetailResponse>("get_session_detail", { sessionId }),
    sendMessage: (sessionId: string, request: SendMessageRequest) =>
      invoke<SendMessageResponse>("send_message", { sessionId, request }),
    cancelRun: (sessionId: string) => invoke<CancelRunResponse>("cancel_run", { sessionId }),
    listArtifacts: (sessionId: string) =>
      invoke<ListArtifactsResponse>("list_artifacts", { sessionId }),
    createArtifact: (sessionId: string, request: CreateArtifactRequest) =>
      invoke<ArtifactRecord>("create_artifact", { sessionId, request }),
    getArtifact: (sessionId: string, artifactId: string) =>
      invoke<ArtifactRecord>("get_artifact", { sessionId, artifactId }),
    listArtifactRevisions: (sessionId: string, artifactIdOrName: string) =>
      invoke<ListArtifactsResponse>("list_artifact_revisions", { sessionId, artifactIdOrName }),
    applyArtifactLifecycle: (
      sessionId: string,
      artifactId: string,
      request: ArtifactLifecycleRequest,
    ) =>
      invoke<ArtifactLifecycleResponse>("apply_artifact_lifecycle", {
        sessionId,
        artifactId,
        request,
      }),
    getStateSnapshot: (projectId?: string | null, includeArchived?: boolean) =>
      invoke<StateSnapshotResponse>("get_state_snapshot", {
        projectId: projectId ?? null,
        includeArchived: includeArchived ?? false,
      }),
    getEvents: (
      afterCursor: number,
      expectedEpoch?: number | null,
      projectId?: string | null,
      limit?: number | null,
    ) =>
      invoke<EventStreamResponse>("get_events", {
        afterCursor,
        expectedEpoch: expectedEpoch ?? null,
        projectId: projectId ?? null,
        limit: limit ?? null,
      }),
    createTask: (request: CreateTaskRequest) => invoke<TaskRecord>("create_task", { request }),
    listTasks: (projectId?: string | null, status?: string | null) =>
      invoke<ListTasksResponse>("list_tasks", {
        projectId: projectId ?? null,
        status: status ?? null,
      }),
    getTaskDetail: (taskId: string) => invoke<TaskDetailResponse>("get_task_detail", { taskId }),
    updateTask: (taskId: string, request: UpdateTaskRequest) =>
      invoke<TaskRecord>("update_task", { taskId, request }),
    sendTaskSignal: (taskId: string, request: TaskSignalRequest) =>
      invoke<TaskSignalResponse>("send_task_signal", { taskId, request }),
    replyToTask: (taskId: string, request: ReplyTaskRequest) =>
      invoke<TaskRecord>("reply_to_task", { taskId, request }),
    cancelTask: (taskId: string) => invoke<CancelTaskResponse>("cancel_task", { taskId }),
    listInboxCards: (status?: string | null, projectId?: string | null) =>
      invoke<ListInboxResponse>("list_inbox_cards", {
        status: status ?? null,
        projectId: projectId ?? null,
      }),
    resolveInboxCard: (cardId: string, request: ResolveInboxCardRequest) =>
      invoke<ResolveInboxCardResponse>("resolve_inbox_card", { cardId, request }),
    listPermissionRequests: (status?: string | null, sessionId?: string | null) =>
      invoke<ListPermissionRequestsResponse>("list_permission_requests", {
        status: status ?? null,
        sessionId: sessionId ?? null,
      }),
    resolvePermissionRequest: (requestId: string, request: ResolvePermissionRequest) =>
      invoke<ResolvePermissionResponse>("resolve_permission_request", { requestId, request }),
    listCapabilityGrants: (projectId?: string | null, includeRevoked?: boolean) =>
      invoke<ListCapabilityGrantsResponse>("list_capability_grants", {
        projectId: projectId ?? null,
        includeRevoked: includeRevoked ?? false,
      }),
    revokeCapabilityGrant: (grantId: string, request?: RevokeGrantRequest) =>
      invoke<CapabilityGrantRecord>("revoke_capability_grant", {
        grantId,
        request: request ?? null,
      }),
    dispatchEffect: (request: DispatchEffectRequest) =>
      invoke<DispatchEffectResponse>("dispatch_effect", { request }),
    listEffects: (taskId?: string | null, sessionId?: string | null, status?: string | null) =>
      invoke<ListEffectsResponse>("list_effects", {
        taskId: taskId ?? null,
        sessionId: sessionId ?? null,
        status: status ?? null,
      }),
    reconcileEffect: (effectId: string, request?: ReconcileEffectRequest) =>
      invoke<ReconcileEffectResponse>("reconcile_effect", {
        effectId,
        request: request ?? null,
      }),
    listDiagnostics: (limit?: number) =>
      invoke<DiagnosticsResponse>("list_diagnostics", { limit: limit ?? null }),
    flushMemoryOutbox: () => invoke<FlushOutboxResponse>("flush_memory_outbox"),
    searchMemory: (request: HeadMemorySearchRequest) =>
      invoke<HeadMemorySearchResponse>("search_memory", { request }),
  };
}

export function createWindowControls(): WindowControls {
  if (!isTauriRuntime()) {
    return {
      supported: false,
      minimize: async () => {},
      toggleMaximize: async () => {},
      close: async () => {},
      startDragging: async () => {},
      isMaximized: async () => false,
    };
  }

  return {
    supported: true,
    minimize: async () => {
      await getCurrentWindow().minimize();
    },
    toggleMaximize: async () => {
      await getCurrentWindow().toggleMaximize();
    },
    close: async () => {
      // V03: closes ONLY the native UI window; never signals dennett-head or memoryd.
      await getCurrentWindow().close();
    },
    startDragging: async () => {
      await getCurrentWindow().startDragging();
    },
    isMaximized: async () => getCurrentWindow().isMaximized(),
  };
}
