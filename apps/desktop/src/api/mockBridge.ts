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
  ListAgentsResponse,
  ListArtifactsResponse,
  ListProjectsResponse,
  ListSessionsResponse,
  MemoryStatus,
  MessageRecord,
  ProjectRecord,
  RunRecord,
  RuntimeAuthProfile,
  RuntimeBinding,
  SendMessageRequest,
  SendMessageResponse,
  SessionDetailResponse,
  SessionRecord,
  UpdateAgentConfigRequest,
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
  projects?: ProjectRecord[];
  agents?: AgentRecord[];
  sessions?: SessionRecord[];
  messagesBySession?: Record<string, MessageRecord[]>;
  artifactsBySession?: Record<string, ArtifactRecord[]>;
  runsBySession?: Record<string, RunRecord[]>;
  sendDelayMs?: number;
  failNextSendWith?: { code: string; message: string } | null;
}

export class MockDesktopBridge implements DesktopBridge {
  public readonly isNativeTauri = false;

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
  public projects: ProjectRecord[];
  public agents: AgentRecord[];
  public sessions: SessionRecord[];
  public messagesBySession: Record<string, MessageRecord[]>;
  public artifactsBySession: Record<string, ArtifactRecord[]>;
  public runsBySession: Record<string, RunRecord[]>;
  public sendDelayMs: number;
  public failNextSendWith: { code: string; message: string } | null;

  public readonly calls: Array<{ method: string; args?: unknown }> = [];
  private idSeq = 1;
  private pendingCancelResolvers = new Map<string, () => void>();

  constructor(seed: MockBridgeSeed = {}) {
    this.headUrl = seed.headUrl ?? "http://127.0.0.1:41771";
    this.connected = seed.connected ?? true;
    this.offlineError = seed.offlineError ?? "dennett-head не отвечает по адресу 127.0.0.1:41771";
    this.memoryStatus = seed.memoryStatus ?? "healthy";
    this.memoryDetail = seed.memoryDetail ?? null;
    this.pendingOutboxCount = seed.pendingOutboxCount ?? 0;
    this.runtimeAdapterId = seed.runtimeAdapterId ?? "antigravity-acp";
    this.runtimeIsSimulated = seed.runtimeIsSimulated ?? false;
    this.authProfile = seed.authProfile ?? "oauth-personal";
    this.tier = seed.tier ?? (this.authProfile === "oauth-personal" ? "Google AI Ultra" : null);
    this.projects = [...(seed.projects ?? [])];
    this.agents = [...(seed.agents ?? [])];
    this.sessions = [...(seed.sessions ?? [])];
    this.messagesBySession = { ...(seed.messagesBySession ?? {}) };
    this.artifactsBySession = { ...(seed.artifactsBySession ?? {}) };
    this.runsBySession = { ...(seed.runsBySession ?? {}) };
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
      schema_version: 1,
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

  public async listProjects(): Promise<ListProjectsResponse> {
    this.calls.push({ method: "listProjects" });
    this.ensureConnected();
    return { projects: [...this.projects] };
  }

  public async createProject(request: CreateProjectRequest): Promise<ProjectRecord> {
    this.calls.push({ method: "createProject", args: request });
    this.ensureConnected();
    const record: ProjectRecord = {
      project_id: this.nextUuid("10000000"),
      name: request.name,
      workspace_path: request.workspace_path,
      memory_scope: request.memory_scope,
      created_at: Date.now(),
    };
    this.projects = [record, ...this.projects];
    return record;
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
      workspace_cwd: project?.workspace_path ?? "/workspace",
      is_simulated: this.runtimeIsSimulated,
      rehydrate_count: 0,
      created_at: now,
      updated_at: now,
    };

    let assistantMsg: MessageRecord | null = null;
    if (!wasCancelled) {
      const modelLabel = agent?.current_config.model_preference ?? "default";
      assistantMsg = {
        message_id: this.nextUuid("51000000"),
        session_id: sessionId,
        role: "assistant",
        content: `Принято (${modelLabel}): ${request.content}`,
        run_id: runId,
        correlation_id: "corr-turn-1",
        created_at: now + 10,
      };
    }

    const runStatus = wasCancelled
      ? "cancelled"
      : this.memoryStatus === "degraded_unreachable"
        ? "degraded"
        : "succeeded";

    const run: RunRecord = {
      run_id: runId,
      session_id: sessionId,
      agent_id: session.agent_id,
      agent_config_version_id: agent?.current_config_version_id ?? this.nextUuid("21000000"),
      user_message_id: userMsg.message_id,
      assistant_message_id: assistantMsg?.message_id ?? null,
      status: runStatus,
      stop_reason: wasCancelled ? "cancelled" : "end_turn",
      error_code: null,
      error_summary: null,
      rehydrated: false,
      correlation_id: "corr-turn-1",
      created_at: now,
      finished_at: now + 15,
    };

    const existingMsgs = this.messagesBySession[sessionId] ?? [];
    this.messagesBySession[sessionId] = assistantMsg
      ? [...existingMsgs, userMsg, assistantMsg]
      : [...existingMsgs, userMsg];
    this.runsBySession[sessionId] = [...(this.runsBySession[sessionId] ?? []), run];
    this.sessions = this.sessions.map((s) =>
      s.session_id === sessionId ? { ...s, runtime_binding: binding, updated_at: now + 15 } : s,
    );

    return {
      user_message: userMsg,
      assistant_message: assistantMsg,
      run,
      runtime_binding: binding,
      artifacts: [],
      permission_decisions: [],
      memory_status: this.memoryStatus,
    };
  }

  public async cancelRun(sessionId: string): Promise<CancelRunResponse> {
    this.calls.push({ method: "cancelRun", args: sessionId });
    this.ensureConnected();
    const resolver = this.pendingCancelResolvers.get(sessionId);
    if (resolver) {
      resolver();
      return {
        session_id: sessionId,
        run_id: this.nextUuid("49000000"),
        cancelled: true,
        status: "cancelled",
        memory_status: this.memoryStatus,
      };
    }
    return {
      session_id: sessionId,
      run_id: null,
      cancelled: false,
      status: null,
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
    const sameName = existing.filter((a) => a.name === request.name);
    const artifact: ArtifactRecord = {
      artifact_id: this.nextUuid("70000000"),
      session_id: sessionId,
      project_id: session.project_id,
      run_id: request.run_id ?? null,
      name: request.name,
      mime_type: request.mime_type,
      revision: sameName.length + 1,
      content: request.content ?? null,
      relative_path: request.relative_path ?? null,
      sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
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
          event_id: this.nextUuid("80000000"),
          correlation_id: "corr-diag-1",
          session_id: this.sessions[0]?.session_id ?? null,
          run_id: null,
          component: "head",
          event_kind: "health_checked",
          safe_summary:
            this.memoryStatus === "degraded_unreachable"
              ? "memoryd unreachable; outbox active"
              : "control + memoryd healthy",
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
    const attempted = this.pendingOutboxCount;
    const delivered = this.memoryStatus === "healthy" ? attempted : 0;
    const pending = this.memoryStatus === "healthy" ? 0 : attempted;
    this.pendingOutboxCount = pending;
    return {
      attempted,
      delivered,
      pending,
      failed: 0,
      memory_status: this.memoryStatus,
      items: [],
    };
  }
}

export function createBrowserPreviewBridge(): MockDesktopBridge {
  const now = Date.now();
  const projectId = "10000000-0000-4000-8000-000000000001";
  const agentId = "20000000-0000-4000-8000-000000000001";
  const cfgId = "21000000-0000-4000-8000-000000000001";
  const sessionId = "30000000-0000-4000-8000-000000000001";

  return new MockDesktopBridge({
    connected: true,
    memoryStatus: "healthy",
    runtimeAdapterId: "antigravity-acp",
    runtimeIsSimulated: true,
    authProfile: "oauth-personal",
    tier: "Google AI Ultra",
    projects: [
      {
        project_id: projectId,
        name: "dennett-agent-orchestrator",
        workspace_path: "A:/Dev/dennett-agent-orchestrator",
        memory_scope: "project:dennett",
        created_at: now - 3_600_000,
      },
    ],
    agents: [
      {
        agent_id: agentId,
        name: "Агент проекта",
        description: "Основной агент D1 через dennett-head",
        current_config_version_id: cfgId,
        current_config: {
          config_version_id: cfgId,
          agent_id: agentId,
          version_number: 1,
          system_prompt: "Помогай владельцу вести проект, фиксируя факты через память Dennett.",
          model_preference: "gemini-2.5-pro",
          thinking_level: "high",
          allowed_memory_scopes: ["project:dennett"],
          workspace_write_policy: { mode: "ask_before_write" },
          created_at: now - 3_600_000,
        },
        created_at: now - 3_600_000,
        updated_at: now - 3_600_000,
      },
    ],
    sessions: [
      {
        session_id: sessionId,
        agent_id: agentId,
        project_id: projectId,
        title: "Рабочий чат D1",
        runtime_binding: {
          binding_id: "60000000-0000-4000-8000-000000000001",
          session_id: sessionId,
          adapter_id: "simulated-preview",
          auth_profile: "oauth-personal",
          tier: "Google AI Ultra",
          agy_version: null,
          provider_session_id: "preview-session",
          workspace_cwd: "A:/Dev/dennett-agent-orchestrator",
          is_simulated: true,
          rehydrate_count: 0,
          created_at: now - 1_200_000,
          updated_at: now - 600_000,
        },
        status: "active",
        created_at: now - 1_200_000,
        updated_at: now - 600_000,
      },
    ],
    messagesBySession: {
      [sessionId]: [
        {
          message_id: "50000000-0000-4000-8000-000000000001",
          session_id: sessionId,
          role: "assistant",
          content:
            "Демонстрационный режим браузера (без Tauri). В нативном окне Dennett подключается к живому dennett-head по loopback IPC.",
          run_id: null,
          correlation_id: "corr-preview-1",
          created_at: now - 600_000,
        },
      ],
    },
    artifactsBySession: {
      [sessionId]: [
        {
          artifact_id: "70000000-0000-4000-8000-000000000001",
          session_id: sessionId,
          project_id: projectId,
          run_id: null,
          name: "project-summary.md",
          mime_type: "text/markdown",
          revision: 1,
          content: "# Dennett D1\n\n- Независимый memoryd\n- Тонкий Tauri 2 клиент\n- Двухслойная Mica-композиция",
          relative_path: null,
          sha256: "8f434346648f6b96df89dda901c5176b10a6d83961dd3c1ac88b59b2dc327aa4",
          created_at: now - 500_000,
        },
      ],
    },
  });
}
