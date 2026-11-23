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
  ListAgentsResponse,
  ListArtifactsResponse,
  ListProjectsResponse,
  ListSessionsResponse,
  ProjectRecord,
  SendMessageRequest,
  SendMessageResponse,
  SessionDetailResponse,
  SessionRecord,
  UpdateAgentConfigRequest,
} from "./contracts";

export interface DesktopBridge {
  readonly isNativeTauri: boolean;
  getHeadHealth(): Promise<DesktopHealthSnapshot>;
  setHeadUrl(headUrl: string): Promise<DesktopHealthSnapshot>;
  listProjects(): Promise<ListProjectsResponse>;
  createProject(request: CreateProjectRequest): Promise<ProjectRecord>;
  listAgents(): Promise<ListAgentsResponse>;
  createAgent(request: CreateAgentRequest): Promise<AgentRecord>;
  updateAgentConfig(agentId: string, request: UpdateAgentConfigRequest): Promise<AgentRecord>;
  listSessions(): Promise<ListSessionsResponse>;
  createSession(request: CreateSessionRequest): Promise<SessionRecord>;
  getSessionDetail(sessionId: string): Promise<SessionDetailResponse>;
  sendMessage(sessionId: string, request: SendMessageRequest): Promise<SendMessageResponse>;
  cancelRun(sessionId: string): Promise<CancelRunResponse>;
  listArtifacts(sessionId: string): Promise<ListArtifactsResponse>;
  createArtifact(sessionId: string, request: CreateArtifactRequest): Promise<ArtifactRecord>;
  getArtifact(sessionId: string, artifactId: string): Promise<ArtifactRecord>;
  listDiagnostics(limit?: number): Promise<DiagnosticsResponse>;
  flushMemoryOutbox(): Promise<FlushOutboxResponse>;
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
    getHeadHealth: () => invoke<DesktopHealthSnapshot>("get_head_health"),
    setHeadUrl: (headUrl: string) => invoke<DesktopHealthSnapshot>("set_head_url", { headUrl }),
    listProjects: () => invoke<ListProjectsResponse>("list_projects"),
    createProject: (request: CreateProjectRequest) =>
      invoke<ProjectRecord>("create_project", { request }),
    listAgents: () => invoke<ListAgentsResponse>("list_agents"),
    createAgent: (request: CreateAgentRequest) => invoke<AgentRecord>("create_agent", { request }),
    updateAgentConfig: (agentId: string, request: UpdateAgentConfigRequest) =>
      invoke<AgentRecord>("update_agent_config", { agentId, request }),
    listSessions: () => invoke<ListSessionsResponse>("list_sessions"),
    createSession: (request: CreateSessionRequest) =>
      invoke<SessionRecord>("create_session", { request }),
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
