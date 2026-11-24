import type { DesktopBridge } from "./bridge";
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
  HeadHealth,
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

async function parseErrorResponse(resp: Response): Promise<{ code: string; message: string }> {
  try {
    const data = (await resp.json()) as {
      error?: {
        code?: string;
        kind?: string;
        resource?: string;
        message?: string;
        detail?: string;
        reason?: string;
      };
      code?: string;
      message?: string;
    };
    const resolvedCode = data.code ?? data.error?.code ?? `HTTP_${resp.status}`;
    if (data.error) {
      const resourcePrefix =
        data.error.resource && data.error.resource.length > 0
          ? `conflict on ${data.error.resource}: `
          : "";
      const detailText =
        data.error.message ??
        (data.error.reason ? `${resourcePrefix}${data.error.reason}` : null) ??
        data.error.detail ??
        `dennett-head returned HTTP ${resp.status}`;
      return {
        code: resolvedCode,
        message: detailText,
      };
    }
    if (typeof data.message === "string") {
      return { code: resolvedCode, message: data.message };
    }
  } catch {
    // Ignore non-JSON error body.
  }
  return {
    code: resp.status >= 500 ? "HEAD_UNREACHABLE" : `HTTP_${resp.status}`,
    message: `dennett-head returned HTTP ${resp.status} ${resp.statusText}`.trim(),
  };
}

function buildQuery(entries: Array<[string, string | number | boolean | null | undefined]>): string {
  const parts: string[] = [];
  for (const [key, val] of entries) {
    if (val === undefined || val === null || val === "") {
      continue;
    }
    parts.push(`${encodeURIComponent(key)}=${encodeURIComponent(String(val))}`);
  }
  return parts.length > 0 ? `?${parts.join("&")}` : "";
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

    listProjects: (includeArchived?: boolean) =>
      requestJson<ListProjectsResponse>(
        "GET",
        includeArchived ? "/v1/projects?include_archived=true" : "/v1/projects",
      ),

    createProject: (request: CreateProjectRequest) =>
      requestJson<ProjectRecord>("POST", "/v1/projects", request),

    updateProject: (projectId: string, request: UpdateProjectRequest) =>
      requestJson<ProjectRecord>("PATCH", `/v1/projects/${projectId}`, request),

    rebindProject: (projectId: string, request: RebindProjectRequest) =>
      requestJson<ProjectLifecycleResponse>("POST", `/v1/projects/${projectId}/rebind`, request),

    detachProjectWorkspace: (projectId: string) =>
      requestJson<ProjectLifecycleResponse>("POST", `/v1/projects/${projectId}/detach-workspace`),

    archiveProject: (projectId: string) =>
      requestJson<ProjectLifecycleResponse>("POST", `/v1/projects/${projectId}/archive`),

    unarchiveProject: (projectId: string) =>
      requestJson<ProjectLifecycleResponse>("POST", `/v1/projects/${projectId}/unarchive`),

    removeProject: (projectId: string) =>
      requestJson<ProjectLifecycleResponse>("POST", `/v1/projects/${projectId}/remove`),

    deleteProject: (projectId: string, confirm: boolean) =>
      requestJson<ProjectLifecycleResponse>(
        "DELETE",
        `/v1/projects/${projectId}?confirm=${confirm ? "true" : "false"}`,
      ),

    listProjectFiles: (projectId: string, subpath?: string) => {
      const qs = subpath ? `?subpath=${encodeURIComponent(subpath)}` : "";
      return requestJson<ListProjectFilesResponse>("GET", `/v1/projects/${projectId}/files${qs}`);
    },

    listAgents: () => requestJson<ListAgentsResponse>("GET", "/v1/agents"),

    createAgent: (request: CreateAgentRequest) =>
      requestJson<AgentRecord>("POST", "/v1/agents", request),

    updateAgentConfig: (agentId: string, request: UpdateAgentConfigRequest) =>
      requestJson<AgentRecord>("POST", `/v1/agents/${agentId}/config-versions`, request),

    sendAgentMessage: (recipientAgentId: string, request: SendAgentMessageRequest) =>
      requestJson<SendAgentMessageResponse>(
        "POST",
        `/v1/agents/${recipientAgentId}/messages`,
        request,
      ),

    listAgentMessages: (recipientAgentId: string, undeliveredOnly?: boolean) =>
      requestJson<ListAgentMessagesResponse>(
        "GET",
        `/v1/agents/${recipientAgentId}/messages${buildQuery([
          ["undelivered_only", undeliveredOnly ? true : null],
        ])}`,
      ),

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

    listArtifactRevisions: (sessionId: string, artifactIdOrName: string) =>
      requestJson<ListArtifactsResponse>(
        "GET",
        `/v1/sessions/${sessionId}/artifacts/${encodeURIComponent(artifactIdOrName)}/revisions`,
      ),

    applyArtifactLifecycle: (
      sessionId: string,
      artifactId: string,
      request: ArtifactLifecycleRequest,
    ) =>
      requestJson<ArtifactLifecycleResponse>(
        "POST",
        `/v1/sessions/${sessionId}/artifacts/${artifactId}/lifecycle`,
        request,
      ),

    getStateSnapshot: (projectId?: string | null, includeArchived?: boolean) =>
      requestJson<StateSnapshotResponse>(
        "GET",
        `/v1/state/snapshot${buildQuery([
          ["project_id", projectId],
          ["include_archived", includeArchived ? true : null],
        ])}`,
      ),

    getEvents: (
      afterCursor: number,
      expectedEpoch?: number | null,
      projectId?: string | null,
      limit?: number | null,
    ) =>
      requestJson<EventStreamResponse>(
        "GET",
        `/v1/events${buildQuery([
          ["after_cursor", afterCursor],
          ["epoch", expectedEpoch],
          ["project_id", projectId],
          ["limit", limit],
        ])}`,
      ),

    createTask: (request: CreateTaskRequest) =>
      requestJson<TaskRecord>("POST", "/v1/tasks", request),

    listTasks: (projectId?: string | null, status?: string | null) =>
      requestJson<ListTasksResponse>(
        "GET",
        `/v1/tasks${buildQuery([
          ["project_id", projectId],
          ["status", status],
        ])}`,
      ),

    getTaskDetail: (taskId: string) =>
      requestJson<TaskDetailResponse>("GET", `/v1/tasks/${taskId}`),

    updateTask: (taskId: string, request: UpdateTaskRequest) =>
      requestJson<TaskRecord>("PATCH", `/v1/tasks/${taskId}`, request),

    sendTaskSignal: (taskId: string, request: TaskSignalRequest) =>
      requestJson<TaskSignalResponse>("POST", `/v1/tasks/${taskId}/signals`, request),

    replyToTask: (taskId: string, request: ReplyTaskRequest) =>
      requestJson<TaskRecord>("POST", `/v1/tasks/${taskId}/reply`, request),

    cancelTask: (taskId: string) =>
      requestJson<CancelTaskResponse>("POST", `/v1/tasks/${taskId}/cancel`),

    listInboxCards: (status?: string | null, projectId?: string | null) =>
      requestJson<ListInboxResponse>(
        "GET",
        `/v1/inbox${buildQuery([
          ["status", status],
          ["project_id", projectId],
        ])}`,
      ),

    resolveInboxCard: (cardId: string, request: ResolveInboxCardRequest) =>
      requestJson<ResolveInboxCardResponse>("POST", `/v1/inbox/${cardId}/resolve`, request),

    listPermissionRequests: (status?: string | null, sessionId?: string | null) =>
      requestJson<ListPermissionRequestsResponse>(
        "GET",
        `/v1/permissions/requests${buildQuery([
          ["status", status],
          ["session_id", sessionId],
        ])}`,
      ),

    resolvePermissionRequest: (requestId: string, request: ResolvePermissionRequest) =>
      requestJson<ResolvePermissionResponse>(
        "POST",
        `/v1/permissions/requests/${requestId}/resolve`,
        request,
      ),

    listCapabilityGrants: (projectId?: string | null, includeRevoked?: boolean) =>
      requestJson<ListCapabilityGrantsResponse>(
        "GET",
        `/v1/grants${buildQuery([
          ["project_id", projectId],
          ["include_revoked", includeRevoked ? true : null],
        ])}`,
      ),

    revokeCapabilityGrant: (grantId: string, request?: RevokeGrantRequest) =>
      requestJson<CapabilityGrantRecord>(
        "POST",
        `/v1/grants/${grantId}/revoke`,
        request ?? {},
      ),

    dispatchEffect: (request: DispatchEffectRequest) =>
      requestJson<DispatchEffectResponse>("POST", "/v1/effects/dispatch", request),

    listEffects: (taskId?: string | null, sessionId?: string | null, status?: string | null) =>
      requestJson<ListEffectsResponse>(
        "GET",
        `/v1/effects${buildQuery([
          ["task_id", taskId],
          ["session_id", sessionId],
          ["status", status],
        ])}`,
      ),

    reconcileEffect: (effectId: string, request?: ReconcileEffectRequest) =>
      requestJson<ReconcileEffectResponse>(
        "POST",
        `/v1/effects/${effectId}/reconcile`,
        request ?? {},
      ),

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
