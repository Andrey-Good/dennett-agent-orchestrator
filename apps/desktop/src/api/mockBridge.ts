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
  MemoryNote,
  MemoryStatus,
  MessageRecord,
  ProjectRecord,
  RunRecord,
  RuntimeAuthProfile,
  RuntimeBinding,
  RuntimeCatalogResponse,
  RuntimeModelOption,
  RuntimeQuotaWindow,
  SendMessageRequest,
  SendMessageResponse,
  SessionDetailResponse,
  SessionRecord,
  ThinkingLevel,
  UpdateAgentConfigRequest,
  UpdateProjectRequest,
  UpdateSessionRequest,
  WorkspaceFileEntry,
} from "./contracts";

export interface MockBridgeSeed {
  headUrl?: string;
  connected?: boolean;
  offlineError?: string;
  memoryStatus?: MemoryStatus;
  memoryDetail?: string | null;
  pendingOutboxCount?: number;
  runtimeAdapterId?: string;
  runtimeIsSimulated?: boolean;
  authProfile?: RuntimeAuthProfile;
  tier?: string | null;
  accountLabel?: string | null;
  models?: RuntimeModelOption[];
  quotas?: RuntimeQuotaWindow[];
  projects?: ProjectRecord[];
  agents?: AgentRecord[];
  sessions?: SessionRecord[];
  messagesBySession?: Record<string, MessageRecord[]>;
  artifactsBySession?: Record<string, ArtifactRecord[]>;
  runsBySession?: Record<string, RunRecord[]>;
  filesByProject?: Record<string, WorkspaceFileEntry[]>;
  notesByScope?: Record<string, MemoryNote[]>;
  supportsNativeFolderPicker?: boolean;
  mockPickedFolder?: string | null;
  sendDelayMs?: number;
  failNextSendWith?: { code: string; message: string } | null;
}

const DEFAULT_ANTIGRAVITY_MODELS: RuntimeModelOption[] = [
  {
    model_id: "gemini-4-neon",
    label: "Gemini 4 Neon",
    supported_thinking_levels: ["low", "medium", "high", "xhigh"],
    default_thinking_level: "medium",
  },
  {
    model_id: "gemini-4-carbon",
    label: "Gemini 4 Carbon",
    supported_thinking_levels: ["low", "medium", "high"],
    default_thinking_level: "medium",
  },
  {
    model_id: "claude-opus-4-6-thinking",
    label: "Claude Opus 4.6 (Thinking)",
    supported_thinking_levels: [],
    default_thinking_level: null,
  },
];

const DEFAULT_SIMULATED_MODELS: RuntimeModelOption[] = [
  {
    model_id: "sim-model-alpha",
    label: "Simulated Alpha",
    supported_thinking_levels: ["low", "medium", "high", "xhigh"],
    default_thinking_level: "medium",
  },
  {
    model_id: "sim-model-beta",
    label: "Simulated Beta",
    supported_thinking_levels: ["low", "medium", "high"],
    default_thinking_level: "medium",
  },
];

function deriveAutoTitle(content: string): string | null {
  const firstLine = content
    .split(/\r?\n/)
    .map((line) => line.trim())
    .find((line) => line.length > 0);
  if (!firstLine) {
    return null;
  }
  const cleaned = firstLine.replace(/^[#*\-`>•\s]+/, "").replace(/\s+/g, " ").trim();
  if (!cleaned) {
    return null;
  }
  if (cleaned.length <= 52) {
    return cleaned;
  }
  const prefix = cleaned.slice(0, 52);
  const lastSpace = prefix.lastIndexOf(" ");
  const cut = lastSpace >= 16 ? prefix.slice(0, lastSpace) : prefix;
  return `${cut.trimEnd()}…`;
}

function isDefaultTitle(title: string): boolean {
  return ["новый чат", "новый разговор", "первый разговор", "untitled", "new chat"].includes(
    title.trim().toLowerCase(),
  );
}

export class MockDesktopBridge implements DesktopBridge {
  public readonly isNativeTauri = false;
  public readonly supportsNativeFolderPicker: boolean;

  public headUrl: string;
  public connected: boolean;
  public offlineError: string;
  public memoryStatus: MemoryStatus;
  public memoryDetail: string | null;
  public pendingOutboxCount: number;
  public runtimeAdapterId: string;
  public runtimeIsSimulated: boolean;
  public authProfile: RuntimeAuthProfile;
  public tier: string | null;
  public accountLabel: string | null;
  public models: RuntimeModelOption[];
  public quotas: RuntimeQuotaWindow[];
  public projects: ProjectRecord[];
  public agents: AgentRecord[];
  public sessions: SessionRecord[];
  public messagesBySession: Record<string, MessageRecord[]>;
  public artifactsBySession: Record<string, ArtifactRecord[]>;
  public runsBySession: Record<string, RunRecord[]>;
  public filesByProject: Record<string, WorkspaceFileEntry[]>;
  public notesByScope: Record<string, MemoryNote[]>;
  public mockPickedFolder: string | null;
  public sendDelayMs: number;
  public failNextSendWith: { code: string; message: string } | null;

  public readonly calls: Array<{ method: string; args?: unknown }> = [];
  private idSeq = 100;
  private pendingCancelResolvers = new Map<string, () => void>();

  constructor(seed: MockBridgeSeed = {}) {
    this.supportsNativeFolderPicker = seed.supportsNativeFolderPicker ?? true;
    this.headUrl = seed.headUrl ?? "http://127.0.0.1:41771";
    this.connected = seed.connected ?? true;
    this.offlineError = seed.offlineError ?? "dennett-head не отвечает по адресу 127.0.0.1:41771";
    this.memoryStatus = seed.memoryStatus ?? "healthy";
    this.memoryDetail = seed.memoryDetail ?? null;
    this.pendingOutboxCount = seed.pendingOutboxCount ?? 0;
    this.runtimeAdapterId = seed.runtimeAdapterId ?? "antigravity-acp";
    this.runtimeIsSimulated = seed.runtimeIsSimulated ?? false;
    this.authProfile = seed.authProfile ?? "oauth-personal";
    this.tier =
      seed.tier !== undefined
        ? seed.tier
        : this.runtimeIsSimulated
          ? "SIMULATED_CI_ONLY"
          : this.authProfile === "oauth-personal"
            ? "Google AI Ultra"
            : null;
    this.accountLabel = seed.accountLabel !== undefined ? seed.accountLabel : "urako";
    this.models =
      seed.models ??
      (this.runtimeIsSimulated ? [...DEFAULT_SIMULATED_MODELS] : [...DEFAULT_ANTIGRAVITY_MODELS]);
    this.quotas = [...(seed.quotas ?? [])];
    this.projects = [...(seed.projects ?? [])];
    this.agents = [...(seed.agents ?? [])];
    this.sessions = [...(seed.sessions ?? [])];
    this.messagesBySession = { ...(seed.messagesBySession ?? {}) };
    this.artifactsBySession = { ...(seed.artifactsBySession ?? {}) };
    this.runsBySession = { ...(seed.runsBySession ?? {}) };
    this.filesByProject = { ...(seed.filesByProject ?? {}) };
    this.notesByScope = { ...(seed.notesByScope ?? {}) };
    this.mockPickedFolder = seed.mockPickedFolder ?? null;
    this.sendDelayMs = seed.sendDelayMs ?? 0;
    this.failNextSendWith = seed.failNextSendWith ?? null;
  }

  private nextUuid(prefix: string): string {
    const n = String(this.idSeq++).padStart(12, "0");
    return `${prefix}-0000-4000-8000-${n}`;
  }

  private ensureConnected(): void {
    if (!this.connected) {
      throw { code: "HEAD_UNREACHABLE", message: this.offlineError };
    }
  }

  public async getHeadHealth(): Promise<DesktopHealthSnapshot> {
    this.calls.push({ method: "getHeadHealth" });
    if (!this.connected) {
      return {
        head_url: this.headUrl,
        connected: false,
        health: null,
        error_code: "HEAD_UNREACHABLE",
        error: this.offlineError,
      };
    }

    const health: HeadHealth = {
      service: "head",
      stage: "D1",
      profile_mode: this.runtimeIsSimulated ? "test_isolated" : "production",
      postgres_connected: true,
      postgres_role: "dennett_control_role",
      postgres_database: "dennett_control_test",
      schema_version: 2,
      memory_status: this.memoryStatus,
      memory_detail: this.memoryDetail,
      runtime_adapter_id: this.runtimeAdapterId,
      runtime_is_simulated: this.runtimeIsSimulated,
      pending_outbox_count: this.pendingOutboxCount,
      capabilities: [
        "control.agents.v1",
        "control.projects.v1",
        "control.sessions.v1",
        "control.artifacts.v1",
        "control.cancel.v1",
        "control.runtime_catalog.v1",
        "control.workspace_files.v1",
      ],
      correlation_id: "corr-health-1",
    };

    return {
      head_url: this.headUrl,
      connected: true,
      health,
      error_code: null,
      error: null,
    };
  }

  public async setHeadUrl(headUrl: string): Promise<DesktopHealthSnapshot> {
    this.calls.push({ method: "setHeadUrl", args: headUrl });
    this.headUrl = headUrl.trim().replace(/\/+$/, "");
    return this.getHeadHealth();
  }

  public async getRuntimeCatalog(refresh?: boolean): Promise<RuntimeCatalogResponse> {
    this.calls.push({ method: "getRuntimeCatalog", args: { refresh: Boolean(refresh) } });
    this.ensureConnected();
    return {
      adapter_id: this.runtimeAdapterId,
      auth_profile: this.authProfile,
      is_simulated: this.runtimeIsSimulated,
      agy_version: this.runtimeIsSimulated ? null : "1.5.15",
      tier: this.tier,
      account_label: this.accountLabel,
      default_model_id: this.models[0]?.model_id ?? null,
      default_thinking_level: "medium",
      models: [...this.models],
      thinking_options: [
        { value: "low", label: "Low" },
        { value: "medium", label: "Medium" },
        { value: "high", label: "High" },
        { value: "xhigh", label: "Extra" },
      ],
      quotas: [...this.quotas],
      probed_at: Date.now(),
    };
  }

  public async pickWorkspaceFolder(): Promise<string | null> {
    this.calls.push({ method: "pickWorkspaceFolder" });
    return this.mockPickedFolder;
  }

  public async listProjects(): Promise<ListProjectsResponse> {
    this.calls.push({ method: "listProjects" });
    this.ensureConnected();
    return { projects: [...this.projects] };
  }

  public async createProject(request: CreateProjectRequest): Promise<ProjectRecord> {
    this.calls.push({ method: "createProject", args: request });
    this.ensureConnected();
    const projectId = this.nextUuid("10000000");
    const slug =
      request.name
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, "-")
        .replace(/^-+|-+$/g, "")
        .slice(0, 24) || "proj";
    const scope = request.memory_scope ?? `project:${slug}-${projectId.slice(-8)}`;
    const record: ProjectRecord = {
      project_id: projectId,
      name: request.name,
      workspace_path: request.workspace_path ?? null,
      memory_scope: scope,
      created_at: Date.now(),
    };
    this.projects = [record, ...this.projects];
    return record;
  }

  public async updateProject(
    projectId: string,
    request: UpdateProjectRequest,
  ): Promise<ProjectRecord> {
    this.calls.push({ method: "updateProject", args: { projectId, request } });
    this.ensureConnected();
    const existing = this.projects.find((p) => p.project_id === projectId);
    if (!existing) {
      throw { code: "NOT_FOUND", message: `project ${projectId} not found` };
    }
    const updated: ProjectRecord = {
      ...existing,
      name: request.name?.trim() || existing.name,
      workspace_path:
        request.workspace_path !== undefined ? request.workspace_path : existing.workspace_path,
    };
    this.projects = this.projects.map((p) => (p.project_id === projectId ? updated : p));
    return updated;
  }

  public async listProjectFiles(
    projectId: string,
    subpath?: string,
  ): Promise<ListProjectFilesResponse> {
    this.calls.push({ method: "listProjectFiles", args: { projectId, subpath } });
    this.ensureConnected();
    const project = this.projects.find((p) => p.project_id === projectId);
    if (!project) {
      throw { code: "NOT_FOUND", message: `project ${projectId} not found` };
    }
    if (!project.workspace_path) {
      return {
        project_id: projectId,
        workspace_bound: false,
        workspace_path: null,
        entries: [],
        truncated: false,
      };
    }
    return {
      project_id: projectId,
      workspace_bound: true,
      workspace_path: project.workspace_path,
      entries: [...(this.filesByProject[projectId] ?? [])],
      truncated: false,
    };
  }

  public async listAgents(): Promise<ListAgentsResponse> {
    this.calls.push({ method: "listAgents" });
    this.ensureConnected();
    return { agents: [...this.agents] };
  }

  public async createAgent(request: CreateAgentRequest): Promise<AgentRecord> {
    this.calls.push({ method: "createAgent", args: request });
    this.ensureConnected();
    const now = Date.now();
    const agentId = this.nextUuid("20000000");
    const cfgId = this.nextUuid("21000000");
    const agent: AgentRecord = {
      agent_id: agentId,
      name: request.name,
      description: request.description,
      current_config_version_id: cfgId,
      current_config: {
        config_version_id: cfgId,
        agent_id: agentId,
        version_number: 1,
        system_prompt: request.system_prompt,
        model_preference: request.model_preference ?? null,
        thinking_level: request.thinking_level ?? null,
        allowed_memory_scopes: [...request.allowed_memory_scopes],
        workspace_write_policy: request.workspace_write_policy,
        created_at: now,
      },
      created_at: now,
      updated_at: now,
    };
    this.agents = [agent, ...this.agents];
    return agent;
  }

  public async updateAgentConfig(
    agentId: string,
    request: UpdateAgentConfigRequest,
  ): Promise<AgentRecord> {
    this.calls.push({ method: "updateAgentConfig", args: { agentId, request } });
    this.ensureConnected();
    const existing = this.agents.find((a) => a.agent_id === agentId);
    if (!existing) {
      throw { code: "NOT_FOUND", message: `agent ${agentId} not found` };
    }
    const now = Date.now();
    const nextCfgId = this.nextUuid("22000000");
    const updated: AgentRecord = {
      ...existing,
      current_config_version_id: nextCfgId,
      updated_at: now,
      current_config: {
        config_version_id: nextCfgId,
        agent_id: existing.agent_id,
        version_number: existing.current_config.version_number + 1,
        system_prompt: request.system_prompt,
        model_preference: request.model_preference ?? null,
        thinking_level: request.thinking_level ?? null,
        allowed_memory_scopes: [...request.allowed_memory_scopes],
        workspace_write_policy: request.workspace_write_policy,
        created_at: now,
      },
    };
    this.agents = this.agents.map((a) => (a.agent_id === agentId ? updated : a));
    return updated;
  }

  public async listSessions(): Promise<ListSessionsResponse> {
    this.calls.push({ method: "listSessions" });
    this.ensureConnected();
    return { sessions: [...this.sessions] };
  }

  public async createSession(request: CreateSessionRequest): Promise<SessionRecord> {
    this.calls.push({ method: "createSession", args: request });
    this.ensureConnected();
    const now = Date.now();
    const sessionId = this.nextUuid("30000000");
    const session: SessionRecord = {
      session_id: sessionId,
      agent_id: request.agent_id,
      project_id: request.project_id,
      title: request.title,
      runtime_binding: null,
      status: "active",
      created_at: now,
      updated_at: now,
    };
    this.sessions = [session, ...this.sessions];
    this.messagesBySession[sessionId] = [];
    this.artifactsBySession[sessionId] = [];
    this.runsBySession[sessionId] = [];
    return session;
  }

  public async updateSession(
    sessionId: string,
    request: UpdateSessionRequest,
  ): Promise<SessionRecord> {
    this.calls.push({ method: "updateSession", args: { sessionId, request } });
    this.ensureConnected();
    const existing = this.sessions.find((s) => s.session_id === sessionId);
    if (!existing) {
      throw { code: "NOT_FOUND", message: `session ${sessionId} not found` };
    }
    const updated: SessionRecord = {
      ...existing,
      title: request.title.trim(),
      updated_at: Date.now(),
    };
    this.sessions = this.sessions.map((s) => (s.session_id === sessionId ? updated : s));
    return updated;
  }

  public async getSessionDetail(sessionId: string): Promise<SessionDetailResponse> {
    this.calls.push({ method: "getSessionDetail", args: sessionId });
    this.ensureConnected();
    const session = this.sessions.find((s) => s.session_id === sessionId);
    if (!session) {
      throw { code: "NOT_FOUND", message: `session ${sessionId} not found` };
    }
    return {
      session,
      messages: [...(this.messagesBySession[sessionId] ?? [])],
      artifacts: [...(this.artifactsBySession[sessionId] ?? [])],
      runs: [...(this.runsBySession[sessionId] ?? [])],
      permission_decisions: [],
    };
  }

  public async sendMessage(
    sessionId: string,
    request: SendMessageRequest,
  ): Promise<SendMessageResponse> {
    this.calls.push({ method: "sendMessage", args: { sessionId, request } });
    this.ensureConnected();

    if (this.failNextSendWith) {
      const err = this.failNextSendWith;
      this.failNextSendWith = null;
      throw err;
    }

    const session = this.sessions.find((s) => s.session_id === sessionId);
    if (!session) {
      throw { code: "NOT_FOUND", message: `session ${sessionId} not found` };
    }
    const agent = this.agents.find((a) => a.agent_id === session.agent_id);
    const project = this.projects.find((p) => p.project_id === session.project_id);

    const prior = this.messagesBySession[sessionId] ?? [];
    let effectiveTitle = session.title;
    if (!prior.some((m) => m.role === "user") && isDefaultTitle(session.title)) {
      const auto = deriveAutoTitle(request.content);
      if (auto) {
        effectiveTitle = auto;
      }
    }

    let wasCancelled = false;
    if (this.sendDelayMs > 0) {
      await new Promise<void>((resolve) => {
        const timer = setTimeout(() => {
          this.pendingCancelResolvers.delete(sessionId);
          resolve();
        }, this.sendDelayMs);
        this.pendingCancelResolvers.set(sessionId, () => {
          wasCancelled = true;
          clearTimeout(timer);
          this.pendingCancelResolvers.delete(sessionId);
          resolve();
        });
      });
    }

    const now = Date.now();
    const runId = this.nextUuid("40000000");
    const userMsg: MessageRecord = {
      message_id: this.nextUuid("50000000"),
      session_id: sessionId,
      role: "user",
      content: request.content,
      run_id: runId,
      correlation_id: "corr-turn-1",
      created_at: now,
    };

    const binding: RuntimeBinding = session.runtime_binding ?? {
      binding_id: this.nextUuid("60000000"),
      session_id: sessionId,
      adapter_id: this.runtimeAdapterId,
      auth_profile: this.authProfile,
      tier: this.tier,
      agy_version: this.runtimeIsSimulated ? null : "1.5.15",
      provider_session_id: `prov-${sessionId.slice(0, 8)}`,
      workspace_cwd: project?.workspace_path ?? "/detached",
      is_simulated: this.runtimeIsSimulated,
      rehydrate_count: 0,
      created_at: now,
      updated_at: now,
    };

    const modelLabel =
      agent?.current_config.model_preference ?? this.models[0]?.model_id ?? "default";
    const assistantMsg: MessageRecord | null = wasCancelled
      ? null
      : {
          message_id: this.nextUuid("51000000"),
          session_id: sessionId,
          role: "assistant",
          content: `Принято (${modelLabel}): ${request.content}`,
          run_id: runId,
          correlation_id: "corr-turn-1",
          created_at: now + 10,
        };

    const run: RunRecord = {
      run_id: runId,
      session_id: sessionId,
      agent_id: session.agent_id,
      agent_config_version_id: agent?.current_config_version_id ?? this.nextUuid("21000000"),
      user_message_id: userMsg.message_id,
      assistant_message_id: assistantMsg?.message_id ?? null,
      status: wasCancelled ? "cancelled" : "succeeded",
      stop_reason: wasCancelled ? "cancelled" : "end_turn",
      error_code: null,
      error_summary: null,
      rehydrated: false,
      correlation_id: "corr-turn-1",
      created_at: now,
      finished_at: now + 10,
    };

    this.messagesBySession[sessionId] = [
      ...prior,
      userMsg,
      ...(assistantMsg ? [assistantMsg] : []),
    ];
    this.runsBySession[sessionId] = [...(this.runsBySession[sessionId] ?? []), run];
    this.sessions = this.sessions.map((s) =>
      s.session_id === sessionId
        ? { ...s, title: effectiveTitle, runtime_binding: binding, updated_at: now + 10 }
        : s,
    );

    return {
      user_message: userMsg,
      assistant_message: assistantMsg,
      run,
      runtime_binding: binding,
      artifacts: [...(this.artifactsBySession[sessionId] ?? [])],
      permission_decisions: [],
      memory_status: this.memoryStatus,
      session_title: effectiveTitle,
    };
  }

  public async cancelRun(sessionId: string): Promise<CancelRunResponse> {
    this.calls.push({ method: "cancelRun", args: sessionId });
    this.ensureConnected();
    const resolver = this.pendingCancelResolvers.get(sessionId);
    if (resolver) {
      resolver();
    }
    return {
      session_id: sessionId,
      run_id: resolver ? this.nextUuid("40000000") : null,
      cancelled: Boolean(resolver),
      status: resolver ? "cancelled" : null,
      memory_status: this.memoryStatus,
    };
  }

  public async listArtifacts(sessionId: string): Promise<ListArtifactsResponse> {
    this.calls.push({ method: "listArtifacts", args: sessionId });
    this.ensureConnected();
    return { artifacts: [...(this.artifactsBySession[sessionId] ?? [])] };
  }

  public async createArtifact(
    sessionId: string,
    request: CreateArtifactRequest,
  ): Promise<ArtifactRecord> {
    this.calls.push({ method: "createArtifact", args: { sessionId, request } });
    this.ensureConnected();
    const session = this.sessions.find((s) => s.session_id === sessionId);
    if (!session) {
      throw { code: "NOT_FOUND", message: `session ${sessionId} not found` };
    }
    const existing = this.artifactsBySession[sessionId] ?? [];
    const sameNameRevs = existing.filter((a) => a.name === request.name).length;
    const artifact: ArtifactRecord = {
      artifact_id: this.nextUuid("70000000"),
      session_id: sessionId,
      project_id: session.project_id,
      run_id: request.run_id ?? null,
      name: request.name,
      mime_type: request.mime_type,
      revision: sameNameRevs + 1,
      content: request.content ?? null,
      relative_path: request.relative_path ?? null,
      sha256: "a".repeat(64),
      created_at: Date.now(),
    };
    this.artifactsBySession[sessionId] = [...existing, artifact];
    return artifact;
  }

  public async getArtifact(sessionId: string, artifactId: string): Promise<ArtifactRecord> {
    this.calls.push({ method: "getArtifact", args: { sessionId, artifactId } });
    this.ensureConnected();
    const found = (this.artifactsBySession[sessionId] ?? []).find(
      (a) => a.artifact_id === artifactId,
    );
    if (!found) {
      throw { code: "NOT_FOUND", message: `artifact ${artifactId} not found` };
    }
    return found;
  }

  public async listDiagnostics(limit?: number): Promise<DiagnosticsResponse> {
    this.calls.push({ method: "listDiagnostics", args: limit });
    this.ensureConnected();
    return {
      events: [
        {
          event_id: "80000000-0000-4000-8000-000000000001",
          correlation_id: "corr-diag-1",
          session_id: this.sessions[0]?.session_id ?? null,
          run_id: null,
          component: "head",
          event_kind: "head_ready",
          safe_summary: "dennett-head готов к работе (D1)",
          created_at: Date.now(),
        },
      ],
      permission_decisions: [],
      outbox_items: [],
    };
  }

  public async flushMemoryOutbox(): Promise<FlushOutboxResponse> {
    this.calls.push({ method: "flushMemoryOutbox" });
    this.ensureConnected();
    if (this.memoryStatus === "degraded_unreachable") {
      return {
        attempted: this.pendingOutboxCount,
        delivered: 0,
        pending: this.pendingOutboxCount,
        failed: 0,
        memory_status: "degraded_unreachable",
        items: [],
      };
    }
    const delivered = this.pendingOutboxCount;
    this.pendingOutboxCount = 0;
    return {
      attempted: delivered,
      delivered,
      pending: 0,
      failed: 0,
      memory_status: "healthy",
      items: [],
    };
  }

  public async searchMemory(request: HeadMemorySearchRequest): Promise<HeadMemorySearchResponse> {
    this.calls.push({ method: "searchMemory", args: request });
    this.ensureConnected();
    if (this.memoryStatus === "degraded_unreachable") {
      return {
        memory_status: "degraded_unreachable",
        degraded_reason: this.memoryDetail ?? "memoryd недоступна",
        notes: [],
      };
    }
    const notes = request.scope
      ? [...(this.notesByScope[request.scope] ?? [])]
      : Object.values(this.notesByScope).flat();
    return {
      memory_status: "healthy",
      degraded_reason: null,
      notes,
    };
  }
}

export function createBrowserPreviewBridge(): DesktopBridge {
  const now = Date.now();
  const projectId = "10000000-0000-4000-8000-000000000001";
  const agentId = "20000000-0000-4000-8000-000000000001";
  const cfgId = "21000000-0000-4000-8000-000000000001";
  const sessionId = "30000000-0000-4000-8000-000000000001";
  const defaultThinking: ThinkingLevel = "medium";

  return new MockDesktopBridge({
    connected: true,
    runtimeAdapterId: "antigravity-acp",
    runtimeIsSimulated: false,
    authProfile: "oauth-personal",
    tier: "Google AI Ultra",
    accountLabel: "urako",
    supportsNativeFolderPicker: false,
    projects: [
      {
        project_id: projectId,
        name: "dennett-agent-orchestrator",
        workspace_path: "A:/Dev/dennett-agent-orchestrator",
        memory_scope: "project:dennett-preview",
        created_at: now - 300_000,
      },
    ],
    agents: [
      {
        agent_id: agentId,
        name: "Проектный агент",
        description: "Интерактивный агент проекта",
        current_config_version_id: cfgId,
        current_config: {
          config_version_id: cfgId,
          agent_id: agentId,
          version_number: 1,
          system_prompt: "Помогай владельцу вести проект.",
          model_preference: "gemini-4-neon",
          thinking_level: defaultThinking,
          allowed_memory_scopes: ["project:dennett-preview"],
          workspace_write_policy: { mode: "ask_before_write" },
          created_at: now - 300_000,
        },
        created_at: now - 300_000,
        updated_at: now - 300_000,
      },
    ],
    sessions: [
      {
        session_id: sessionId,
        agent_id: agentId,
        project_id: projectId,
        title: "Рабочий диалог D1",
        runtime_binding: {
          binding_id: "60000000-0000-4000-8000-000000000001",
          session_id: sessionId,
          adapter_id: "antigravity-acp",
          auth_profile: "oauth-personal",
          tier: "Google AI Ultra",
          agy_version: "1.5.15",
          provider_session_id: "preview-sess-1",
          workspace_cwd: "A:/Dev/dennett-agent-orchestrator",
          is_simulated: false,
          rehydrate_count: 0,
          created_at: now - 120_000,
          updated_at: now - 60_000,
        },
        status: "active",
        created_at: now - 120_000,
        updated_at: now - 60_000,
      },
    ],
    messagesBySession: {
      [sessionId]: [
        {
          message_id: "50000000-0000-4000-8000-000000000001",
          session_id: sessionId,
          role: "assistant",
          content:
            "Демо-предпросмотр интерфейса Dennett (`?mock=1`). Запустите стек `Head + memoryd` для живой работы с `agy acp`.",
          run_id: null,
          correlation_id: "corr-preview-1",
          created_at: now - 60_000,
        },
      ],
    },
    filesByProject: {
      [projectId]: [
        {
          relative_path: "src",
          name: "src",
          kind: "directory",
          size_bytes: null,
          depth: 1,
        },
        {
          relative_path: "README.md",
          name: "README.md",
          kind: "file",
          size_bytes: 2048,
          depth: 1,
        },
      ],
    },
  });
}
