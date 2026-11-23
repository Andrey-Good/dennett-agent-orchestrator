import type { DesktopBridge } from "./bridge";
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
  HeadHealth,
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

async function parseErrorResponse(resp: Response): Promise<{ code: string; message: string }> {
  try {
    const data = (await resp.json()) as {
      error?: { code?: string; message?: string; detail?: string; reason?: string };
      code?: string;
      message?: string;
    };
    if (data.error) {
      return {
        code: data.error.code ?? `HTTP_${resp.status}`,
        message:
          data.error.message ??
          data.error.detail ??
          data.error.reason ??
          `dennett-head returned HTTP ${resp.status}`,
      };
    }
    if (typeof data.message === "string") {
      return { code: data.code ?? `HTTP_${resp.status}`, message: data.message };
    }
  } catch {
    // Ignore non-JSON error body.
  }
  return {
    code: resp.status >= 500 ? "HEAD_UNREACHABLE" : `HTTP_${resp.status}`,
    message: `dennett-head returned HTTP ${resp.status} ${resp.statusText}`.trim(),
  };
}

/**
 * Dev-only HTTP bridge talking to a live local `dennett-head` through the Vite dev server proxy
 * (`/__dennett_dev_head`). Strictly instantiated ONLY when `import.meta.env.DEV && !isTauriRuntime()`.
 */
export function createDevHttpBridge(proxyPrefix = "/__dennett_dev_head"): DesktopBridge {
  const base = proxyPrefix.replace(/\/+$/, "");
  let displayedUrl = "http://127.0.0.1:41771 (Vite dev proxy)";

  async function requestJson<T>(
    method: string,
    path: string,
    body?: unknown,
  ): Promise<T> {
    let resp: Response;
    try {
      resp = await fetch(`${base}${path}`, {
        method,
        headers: body !== undefined ? { "Content-Type": "application/json" } : undefined,
        body: body !== undefined ? JSON.stringify(body) : undefined,
      });
    } catch (err) {
      throw {
        code: "HEAD_UNREACHABLE",
        message:
          err instanceof Error
            ? `Не удалось подключиться к dennett-head: ${err.message}`
            : "Не удалось подключиться к dennett-head",
      };
    }

    if (!resp.ok) {
      throw await parseErrorResponse(resp);
    }
    return (await resp.json()) as T;
  }

  return {
    isNativeTauri: false,
    supportsNativeFolderPicker: false,

    async getHeadHealth(): Promise<DesktopHealthSnapshot> {
      try {
        const resp = await fetch(`${base}/healthz`, { method: "GET" });
        if (!resp.ok) {
          const parsed = await parseErrorResponse(resp);
          return {
            head_url: displayedUrl,
            connected: false,
            health: null,
            error_code: parsed.code,
            error: parsed.message,
          };
        }
        const health = (await resp.json()) as HeadHealth;
        return {
          head_url: displayedUrl,
          connected: true,
          health,
          error_code: null,
          error: null,
        };
      } catch (err) {
        return {
          head_url: displayedUrl,
          connected: false,
          health: null,
          error_code: "HEAD_UNREACHABLE",
          error:
            err instanceof Error
              ? `dennett-head недоступен: ${err.message}`
              : "dennett-head недоступен",
        };
      }
    },

    async setHeadUrl(headUrl: string): Promise<DesktopHealthSnapshot> {
      displayedUrl = headUrl.trim() || displayedUrl;
      return this.getHeadHealth();
    },

    getRuntimeCatalog: (refresh?: boolean) =>
      requestJson<RuntimeCatalogResponse>(
        "GET",
        refresh ? "/v1/runtime/catalog?refresh=true" : "/v1/runtime/catalog",
      ),

    pickWorkspaceFolder: async () => null,

    listProjects: () => requestJson<ListProjectsResponse>("GET", "/v1/projects"),

    createProject: (request: CreateProjectRequest) =>
      requestJson<ProjectRecord>("POST", "/v1/projects", request),

    updateProject: (projectId: string, request: UpdateProjectRequest) =>
      requestJson<ProjectRecord>("PATCH", `/v1/projects/${projectId}`, request),

    listProjectFiles: (projectId: string, subpath?: string) => {
      const qs = subpath ? `?subpath=${encodeURIComponent(subpath)}` : "";
      return requestJson<ListProjectFilesResponse>("GET", `/v1/projects/${projectId}/files${qs}`);
    },

    listAgents: () => requestJson<ListAgentsResponse>("GET", "/v1/agents"),

    createAgent: (request: CreateAgentRequest) =>
      requestJson<AgentRecord>("POST", "/v1/agents", request),

    updateAgentConfig: (agentId: string, request: UpdateAgentConfigRequest) =>
      requestJson<AgentRecord>("POST", `/v1/agents/${agentId}/config-versions`, request),

    listSessions: () => requestJson<ListSessionsResponse>("GET", "/v1/sessions"),

    createSession: (request: CreateSessionRequest) =>
      requestJson<SessionRecord>("POST", "/v1/sessions", request),

    updateSession: (sessionId: string, request: UpdateSessionRequest) =>
      requestJson<SessionRecord>("PATCH", `/v1/sessions/${sessionId}`, request),

    getSessionDetail: (sessionId: string) =>
      requestJson<SessionDetailResponse>("GET", `/v1/sessions/${sessionId}`),

    sendMessage: (sessionId: string, request: SendMessageRequest) =>
      requestJson<SendMessageResponse>("POST", `/v1/sessions/${sessionId}/messages`, request),

    cancelRun: (sessionId: string) =>
      requestJson<CancelRunResponse>("POST", `/v1/sessions/${sessionId}/cancel`),

    listArtifacts: (sessionId: string) =>
      requestJson<ListArtifactsResponse>("GET", `/v1/sessions/${sessionId}/artifacts`),

    createArtifact: (sessionId: string, request: CreateArtifactRequest) =>
      requestJson<ArtifactRecord>("POST", `/v1/sessions/${sessionId}/artifacts`, request),

    async getArtifact(sessionId: string, artifactId: string): Promise<ArtifactRecord> {
      const list = await requestJson<ListArtifactsResponse>(
        "GET",
        `/v1/sessions/${sessionId}/artifacts`,
      );
      const found = list.artifacts.find((item) => item.artifact_id === artifactId);
      if (!found) {
        throw { code: "NOT_FOUND", message: `artifact ${artifactId} not found` };
      }
      return found;
    },

    listDiagnostics: (limit?: number) => {
      const qs = limit !== undefined ? `?limit=${limit}` : "";
      return requestJson<DiagnosticsResponse>("GET", `/v1/diagnostics${qs}`);
    },

    flushMemoryOutbox: () =>
      requestJson<FlushOutboxResponse>("POST", "/v1/memory/outbox/flush"),

    searchMemory: (request: HeadMemorySearchRequest) =>
      requestJson<HeadMemorySearchResponse>("POST", "/v1/memory/search", request),
  };
}
