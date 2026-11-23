import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

import type {
  AgentRecord,
  ArtifactRecord,
  CancelRunResponse,
  CreateAgentRequest,
  CreateArtifactRequest,
  CreateProjectRequest,
  CreateSessionRequest,
  DesktopHealthSnapshot,
  DiagnosticsResponse,
  FlushOutboxResponse,
  HeadMemorySearchRequest,
  HeadMemorySearchResponse,
  ListAgentsResponse,
  ListArtifactsResponse,
  ListProjectFilesResponse,
  ListProjectsResponse,
  ListSessionsResponse,
  ProjectRecord,
  RuntimeCatalogResponse,
  SendMessageRequest,
  SendMessageResponse,
  SessionDetailResponse,
  SessionRecord,
  UpdateAgentConfigRequest,
  UpdateProjectRequest,
  UpdateSessionRequest,
} from "./contracts";

export interface DesktopBridge {
  readonly isNativeTauri: boolean;
  readonly supportsNativeFolderPicker: boolean;
  getHeadHealth(): Promise<DesktopHealthSnapshot>;
  setHeadUrl(headUrl: string): Promise<DesktopHealthSnapshot>;
  getRuntimeCatalog(refresh?: boolean): Promise<RuntimeCatalogResponse>;
  pickWorkspaceFolder(): Promise<string | null>;
  listProjects(): Promise<ListProjectsResponse>;
  createProject(request: CreateProjectRequest): Promise<ProjectRecord>;
  updateProject(projectId: string, request: UpdateProjectRequest): Promise<ProjectRecord>;
  listProjectFiles(projectId: string, subpath?: string): Promise<ListProjectFilesResponse>;
  listAgents(): Promise<ListAgentsResponse>;
  createAgent(request: CreateAgentRequest): Promise<AgentRecord>;
  updateAgentConfig(agentId: string, request: UpdateAgentConfigRequest): Promise<AgentRecord>;
  listSessions(): Promise<ListSessionsResponse>;
  createSession(request: CreateSessionRequest): Promise<SessionRecord>;
  updateSession(sessionId: string, request: UpdateSessionRequest): Promise<SessionRecord>;
  getSessionDetail(sessionId: string): Promise<SessionDetailResponse>;
  sendMessage(sessionId: string, request: SendMessageRequest): Promise<SendMessageResponse>;
  cancelRun(sessionId: string): Promise<CancelRunResponse>;
  listArtifacts(sessionId: string): Promise<ListArtifactsResponse>;
  createArtifact(sessionId: string, request: CreateArtifactRequest): Promise<ArtifactRecord>;
  getArtifact(sessionId: string, artifactId: string): Promise<ArtifactRecord>;
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
    listProjects: () => invoke<ListProjectsResponse>("list_projects"),
    createProject: (request: CreateProjectRequest) =>
      invoke<ProjectRecord>("create_project", { request }),
    updateProject: (projectId: string, request: UpdateProjectRequest) =>
      invoke<ProjectRecord>("update_project", { projectId, request }),
    listProjectFiles: (projectId: string, subpath?: string) =>
      invoke<ListProjectFilesResponse>("list_project_files", {
        projectId,
        subpath: subpath ?? null,
      }),
    listAgents: () => invoke<ListAgentsResponse>("list_agents"),
    createAgent: (request: CreateAgentRequest) => invoke<AgentRecord>("create_agent", { request }),
    updateAgentConfig: (agentId: string, request: UpdateAgentConfigRequest) =>
      invoke<AgentRecord>("update_agent_config", { agentId, request }),
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
