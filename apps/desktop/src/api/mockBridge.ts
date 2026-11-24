import type { DesktopBridge } from "./bridge";
import {
  sortProjectsByPin,
  sortSessionsByPin,
  type ActivateConnectionRequest,
  type AgentMessageRecord,
  type AgentRecord,
  type ArtifactLifecycleRequest,
  type ArtifactLifecycleResponse,
  type ArtifactRecord,
  type CancelRunResponse,
  type CancelTaskResponse,
  type CapabilityGrantRecord,
  type CapabilityKind,
  type CapabilityProjectBinding,
  type CapabilityRecord,
  type CapabilityScopeKind,
  type CommitMemoryNoteRequest,
  type CommitMemoryNoteResponse,
  type ConsolidationJob,
  type ControlEventRecord,
  type CorrectNoteRequest,
  type CreateAgentRequest,
  type CreateArtifactRequest,
  type CreateCapabilityRequest,
  type CreateClaimRequest,
  type CreateConsolidationJobRequest,
  type CreateDocumentRequest,
  type CreateProjectRequest,
  type CreateSessionRequest,
  type CreateTaskRequest,
  type DeleteCapabilityResponse,
  type DeleteProviderConnectionResponse,
  type DesktopHealthSnapshot,
  type DiagnosticsResponse,
  type DiscoverCapabilitiesRequest,
  type DiscoverCapabilitiesResponse,
  type DispatchEffectRequest,
  type DispatchEffectResponse,
  type DocumentVersion,
  type EffectClaimRecord,
  type EventStreamResponse,
  type ExportRequest,
  type ExportResponse,
  type FlushOutboxResponse,
  type ForgetRequest,
  type ForgetResponse,
  type ForgetTombstone,
  type GetDocumentVersionsResponse,
  type HeadHealth,
  type HeadMemorySearchRequest,
  type HeadMemorySearchResponse,
  type ImportRequest,
  type ImportResponse,
  type InboxCardRecord,
  type IndexStatus,
  type ListAgentMessagesResponse,
  type ListAgentsResponse,
  type ListArtifactsResponse,
  type ListCapabilitiesResponse,
  type ListCapabilityGrantsResponse,
  type ListClaimsResponse,
  type ListConflictsResponse,
  type ListConsolidationJobsResponse,
  type ListDocumentsResponse,
  type ListEffectsResponse,
  type ListInboxResponse,
  type ListPermissionRequestsResponse,
  type ListProjectFilesResponse,
  type ListProjectsResponse,
  type ListProviderConnectionsResponse,
  type ListSecretsResponse,
  type ListSessionsResponse,
  type ListTasksResponse,
  type ListTombstonesResponse,
  type MemoryClaim,
  type MemoryConflict,
  type MemoryDocument,
  type MemoryNote,
  type MemoryOverviewResponse,
  type MemoryStatus,
  type MessageRecord,
  type NoteHistoryResponse,
  type NoteMutationResponse,
  type NoteRevision,
  type PermissionDecisionRecord,
  type PermissionRequestRecord,
  type PlannedProviderInfo,
  type ProbeConnectionResponse,
  type ProjectLifecycleResponse,
  type ProjectRecord,
  type ProviderConnectionRecord,
  type ProviderQuotaError,
  type RebindProjectRequest,
  type ReconcileEffectRequest,
  type ReconcileEffectResponse,
  type ReplyTaskRequest,
  type ResolveConflictRequest,
  type ResolveInboxCardRequest,
  type ResolveInboxCardResponse,
  type ResolvePermissionRequest,
  type ResolvePermissionResponse,
  type RevokeGrantRequest,
  type RunRecord,
  type RuntimeAuthProfile,
  type RuntimeBinding,
  type RuntimeCatalogResponse,
  type RuntimeModelOption,
  type RuntimeQuotaWindow,
  type SecretMetadataRecord,
  type SendAgentMessageRequest,
  type SendAgentMessageResponse,
  type SendMessageRequest,
  type SendMessageResponse,
  type SessionDetailResponse,
  type SessionRecord,
  type SetProjectCapabilityBindingRequest,
  type SetSecretRequest,
  type StateSnapshotResponse,
  type SupersedeClaimRequest,
  type TaskDetailResponse,
  type TaskRecord,
  type TaskSignalRecord,
  type TaskSignalRequest,
  type TaskSignalResponse,
  type TaskTimerRecord,
  type ThinkingLevel,
  type TriggerIndexRebuildRequest,
  type TriggerIndexRebuildResponse,
  type UpdateAgentConfigRequest,
  type UpdateCapabilityRequest,
  type UpdateDocumentRequest,
  type UpdateProjectRequest,
  type UpdateSessionRequest,
  type UpdateTaskRequest,
  type UpsertProviderConnectionRequest,
  type WorkspaceFileEntry,
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
  tasks?: TaskRecord[];
  inboxCards?: InboxCardRecord[];
  permissionRequests?: PermissionRequestRecord[];
  capabilityGrants?: CapabilityGrantRecord[];
  effects?: EffectClaimRecord[];
  agentMessages?: AgentMessageRecord[];
  messagesBySession?: Record<string, MessageRecord[]>;
  artifactsBySession?: Record<string, ArtifactRecord[]>;
  runsBySession?: Record<string, RunRecord[]>;
  permissionDecisionsBySession?: Record<string, PermissionDecisionRecord[]>;
  filesByProject?: Record<string, WorkspaceFileEntry[]>;
  notesByScope?: Record<string, MemoryNote[]>;
  supportsNativeFolderPicker?: boolean;
  mockPickedFolder?: string | null;
  sendDelayMs?: number;
  failNextSendWith?: { code: string; message: string } | null;
  runningSessionIds?: string[];
  // Phase D3 seeds
  secrets?: SecretMetadataRecord[];
  providerConnections?: ProviderConnectionRecord[];
  plannedProviders?: PlannedProviderInfo[];
  capabilities?: CapabilityRecord[];
  memoryClaims?: MemoryClaim[];
  memoryConflicts?: MemoryConflict[];
  memoryDocuments?: MemoryDocument[];
  documentVersionsById?: Record<string, DocumentVersion[]>;
  noteRevisionsById?: Record<string, NoteRevision[]>;
  consolidationJobs?: ConsolidationJob[];
  tombstones?: ForgetTombstone[];
  indexStatus?: IndexStatus;
  staleWarning?: string | null;
  quotaError?: ProviderQuotaError | null;
  localProbeModels?: RuntimeModelOption[];
  memorySourcesCount?: number;
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

const DEFAULT_LOCAL_HTTP_MODELS: RuntimeModelOption[] = [
  {
    model_id: "qwen2.5-coder:14b",
    label: "Qwen 2.5 Coder 14B (Local)",
    supported_thinking_levels: [],
    default_thinking_level: null,
  },
  {
    model_id: "llama3.2:latest",
    label: "Llama 3.2 (Local)",
    supported_thinking_levels: [],
    default_thinking_level: null,
  },
];

const DEFAULT_PLANNED_PROVIDERS: PlannedProviderInfo[] = [
  {
    id: "r-claude",
    label: "Claude Code (R-CLAUDE)",
    status: "planned",
    note: "Запланировано за пределами D0–D9",
  },
  {
    id: "r-codex",
    label: "Codex (R-CODEX)",
    status: "planned",
    note: "Запланировано за пределами D0–D9",
  },
];

function deterministicHex(input: string, len: number): string {
  let h1 = 0xdeadbeef ^ input.length;
  let h2 = 0x41c6ce57 ^ input.length;
  for (let i = 0; i < input.length; i += 1) {
    const ch = input.charCodeAt(i);
    h1 = Math.imul(h1 ^ ch, 2654435761);
    h2 = Math.imul(h2 ^ ch, 1597334677);
  }
  let out = "";
  let seed = (h1 >>> 0).toString(16).padStart(8, "0") + (h2 >>> 0).toString(16).padStart(8, "0");
  while (out.length < len) {
    h1 = Math.imul(h1 ^ 0x9e3779b1, 2246822519);
    h2 = Math.imul(h2 ^ 0x85ebca77, 3266489917);
    out += seed;
    seed = (h1 >>> 0).toString(16).padStart(8, "0") + (h2 >>> 0).toString(16).padStart(8, "0");
  }
  return out.slice(0, len);
}

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
  public tasks: TaskRecord[];
  public inboxCards: InboxCardRecord[];
  public permissionRequests: PermissionRequestRecord[];
  public capabilityGrants: CapabilityGrantRecord[];
  public effects: EffectClaimRecord[];
  public agentMessages: AgentMessageRecord[];
  public timersByTask: Record<string, TaskTimerRecord[]> = {};
  public signalsByTask: Record<string, TaskSignalRecord[]> = {};
  public messagesBySession: Record<string, MessageRecord[]>;
  public artifactsBySession: Record<string, ArtifactRecord[]>;
  public runsBySession: Record<string, RunRecord[]>;
  public permissionDecisionsBySession: Record<string, PermissionDecisionRecord[]>;
  public filesByProject: Record<string, WorkspaceFileEntry[]>;
  public notesByScope: Record<string, MemoryNote[]>;
  public mockPickedFolder: string | null;
  public sendDelayMs: number;
  public failNextSendWith: { code: string; message: string } | null;
  public seededRunningSessionIds: Set<string>;

  // Phase D3 in-memory state
  public secrets: SecretMetadataRecord[];
  public providerConnections: ProviderConnectionRecord[];
  public plannedProviders: PlannedProviderInfo[];
  public capabilities: CapabilityRecord[];
  public memoryClaims: MemoryClaim[];
  public memoryConflicts: MemoryConflict[];
  public memoryDocuments: MemoryDocument[];
  public documentVersionsById: Record<string, DocumentVersion[]>;
  public noteRevisionsById: Record<string, NoteRevision[]>;
  public consolidationJobs: ConsolidationJob[];
  public tombstones: ForgetTombstone[];
  public indexStatus: IndexStatus;
  public staleWarning: string | null;
  public quotaError: ProviderQuotaError | null;
  public localProbeModels: RuntimeModelOption[];
  public memorySourcesCount: number;
  private previousCapabilitySnapshots = new Map<string, CapabilityRecord>();
  private lastRebuildAt: number | null = null;

  public authorityEpoch = 1;
  public retainedFromSeq = 1;
  public events: ControlEventRecord[] = [];
  private cursorSeq = 0;
  private clientMessageIndex = new Map<string, SendMessageResponse>();

  public readonly calls: Array<{ method: string; args?: unknown }> = [];
  private idSeq = 10000;
  private lastMutationTs = 0;
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
    this.projects = sortProjectsByPin(
      (seed.projects ?? []).map((p) => ({
        ...p,
        lifecycle_state: p.lifecycle_state ?? "active",
        pinned: Boolean(p.pinned),
        pinned_at: p.pinned ? (p.pinned_at ?? null) : null,
      })),
    );
    this.agents = [...(seed.agents ?? [])];
    this.sessions = sortSessionsByPin(
      (seed.sessions ?? []).map((s) => ({
        ...s,
        project_id: s.project_id ?? null,
        pinned: Boolean(s.pinned),
        pinned_at: s.pinned ? (s.pinned_at ?? null) : null,
      })),
    );
    this.tasks = [...(seed.tasks ?? [])];
    this.inboxCards = [...(seed.inboxCards ?? [])];
    this.permissionRequests = [...(seed.permissionRequests ?? [])];
    this.capabilityGrants = [...(seed.capabilityGrants ?? [])];
    this.effects = [...(seed.effects ?? [])];
    this.agentMessages = [...(seed.agentMessages ?? [])];
    this.messagesBySession = { ...(seed.messagesBySession ?? {}) };
    this.artifactsBySession = { ...(seed.artifactsBySession ?? {}) };
    this.runsBySession = { ...(seed.runsBySession ?? {}) };
    this.permissionDecisionsBySession = { ...(seed.permissionDecisionsBySession ?? {}) };
    this.filesByProject = { ...(seed.filesByProject ?? {}) };
    this.notesByScope = { ...(seed.notesByScope ?? {}) };
    this.mockPickedFolder = seed.mockPickedFolder ?? null;
    this.sendDelayMs = seed.sendDelayMs ?? 0;
    this.failNextSendWith = seed.failNextSendWith ?? null;
    this.seededRunningSessionIds = new Set(seed.runningSessionIds ?? []);

    const initNow = Date.now();
    this.secrets = seed.secrets
      ? seed.secrets.map((s) => ({ ...s }))
      : [
          {
            slot_key: "gemini_api_key",
            scope: "global",
            description: "Gemini API key для фоновых профилей Antigravity",
            configured: false,
            version: 0,
            fingerprint_short: null,
            source: "unset",
            updated_at: null,
          },
          {
            slot_key: "local_runtime_token",
            scope: "global",
            description: "Опциональный Bearer-токен локального сервера",
            configured: false,
            version: 0,
            fingerprint_short: null,
            source: "unset",
            updated_at: null,
          },
        ];
    this.providerConnections = seed.providerConnections
      ? seed.providerConnections.map((c) => ({ ...c }))
      : [
          {
            connection_id: "91000000-0000-4000-8000-000000000001",
            provider_kind: "antigravity_acp",
            display_name: "Google Antigravity · Google AI Ultra",
            adapter_id: "antigravity-acp",
            auth_profile: "oauth-personal",
            endpoint_url: null,
            secret_slot_key: null,
            is_default_interactive: this.authProfile === "oauth-personal",
            is_default_background: false,
            enabled: true,
            last_health_status: "healthy",
            last_health_detail: null,
            last_probed_at: initNow,
            created_at: initNow - 60_000,
            updated_at: initNow,
          },
          {
            connection_id: "91000000-0000-4000-8000-000000000002",
            provider_kind: "antigravity_acp",
            display_name: "Google Antigravity · Gemini API key",
            adapter_id: "antigravity-acp",
            auth_profile: "gemini-api-key",
            endpoint_url: null,
            secret_slot_key: "gemini_api_key",
            is_default_interactive: this.authProfile === "gemini-api-key",
            is_default_background: true,
            enabled: true,
            last_health_status: this.authProfile === "gemini-api-key" ? "healthy" : "auth_required",
            last_health_detail: null,
            last_probed_at: initNow,
            created_at: initNow - 60_000,
            updated_at: initNow,
          },
        ];
    this.plannedProviders = seed.plannedProviders
      ? seed.plannedProviders.map((p) => ({ ...p }))
      : DEFAULT_PLANNED_PROVIDERS.map((p) => ({ ...p }));
    this.capabilities = seed.capabilities ? seed.capabilities.map((c) => ({ ...c })) : [];
    this.memoryClaims = seed.memoryClaims ? seed.memoryClaims.map((c) => ({ ...c })) : [];
    this.memoryConflicts = seed.memoryConflicts ? seed.memoryConflicts.map((c) => ({ ...c })) : [];
    this.memoryDocuments = seed.memoryDocuments ? seed.memoryDocuments.map((d) => ({ ...d })) : [];
    this.documentVersionsById = { ...(seed.documentVersionsById ?? {}) };
    for (const doc of this.memoryDocuments) {
      if (!this.documentVersionsById[doc.document_id]) {
        this.documentVersionsById[doc.document_id] = [
          {
            version_id: this.nextUuid("96000000"),
            document_id: doc.document_id,
            version: doc.current_version,
            body: doc.current_body,
            provenance_claim_ids: [...(doc.provenance_claim_ids ?? [])],
            diff_summary: "Начальная версия",
            event_id: doc.source_event_id ?? this.nextUuid("97000000"),
            created_at: doc.created_at,
          },
        ];
      }
    }
    this.noteRevisionsById = { ...(seed.noteRevisionsById ?? {}) };
    for (const notes of Object.values(this.notesByScope)) {
      for (const note of notes) {
        if (!this.noteRevisionsById[note.note_id]) {
          this.noteRevisionsById[note.note_id] = [
            {
              revision_id: note.current_revision_id ?? this.nextUuid("95000000"),
              note_id: note.note_id,
              revision: note.current_revision ?? note.revision ?? 1,
              title: note.title,
              body: note.body,
              tags: [...note.tags],
              rationale: note.rationale ?? null,
              event_id: note.last_event_id ?? this.nextUuid("97000000"),
              recorded_by: note.recorded_by ?? "owner",
              author_label: note.author_label ?? null,
              created_at: note.updated_at,
            },
          ];
        }
      }
    }
    this.consolidationJobs = seed.consolidationJobs
      ? seed.consolidationJobs.map((j) => ({ ...j }))
      : [];
    this.tombstones = seed.tombstones ? seed.tombstones.map((t) => ({ ...t })) : [];
    this.indexStatus = seed.indexStatus ?? "healthy";
    this.staleWarning = seed.staleWarning ?? null;
    this.quotaError = seed.quotaError ?? null;
    this.localProbeModels = seed.localProbeModels
      ? [...seed.localProbeModels]
      : [...DEFAULT_LOCAL_HTTP_MODELS];
    this.memorySourcesCount = seed.memorySourcesCount ?? 0;
  }

  public emitControlEvent(
    eventType: string,
    opts: {
      entityKey?: string;
      projectId?: string | null;
      sessionId?: string | null;
      taskId?: string | null;
      payload?: Record<string, unknown>;
    } = {},
  ): ControlEventRecord {
    this.cursorSeq += 1;
    const ev: ControlEventRecord = {
      cursor_seq: this.cursorSeq,
      event_id: this.nextUuid("99000000"),
      authority_epoch: this.authorityEpoch,
      entity_key: opts.entityKey ?? "control:mock",
      entity_seq: this.cursorSeq,
      event_type: eventType,
      project_id: opts.projectId ?? null,
      session_id: opts.sessionId ?? null,
      task_id: opts.taskId ?? null,
      correlation_id: `corr-ev-${this.cursorSeq}`,
      payload_json: JSON.stringify(opts.payload ?? {}),
      created_at: Date.now(),
    };
    this.events.push(ev);
    return ev;
  }

  private nextUuid(prefix: string): string {
    const n = String(this.idSeq++).padStart(12, "0");
    return `${prefix}-0000-4000-8000-${n}`;
  }

  private nextTimestamp(): number {
    const now = Date.now();
    this.lastMutationTs = Math.max(now, this.lastMutationTs + 1);
    return this.lastMutationTs;
  }

  private ensureConnected(): void {
    if (!this.connected) {
      throw { code: "HEAD_UNREACHABLE", message: this.offlineError };
    }
  }

  private isProjectDeleted(projectId: string | null | undefined): boolean {
    if (!projectId) {
      return false;
    }
    const project = this.projects.find((p) => p.project_id === projectId);
    return project !== undefined && (project.lifecycle_state ?? "active") === "deleted_record";
  }

  private terminateProjectWork(projectId: string, reason: string): void {
    const now = Date.now();
    this.tasks = this.tasks.map((t) =>
      t.project_id === projectId &&
      !["succeeded", "failed", "cancelled", "unknown_external"].includes(t.status)
        ? {
            ...t,
            status: "cancelled",
            wait_detail: null,
            active_run_id: null,
            finished_at: t.finished_at ?? now,
            updated_at: now,
          }
        : t,
    );
    this.inboxCards = this.inboxCards.map((c) =>
      c.project_id === projectId && c.status === "open" && c.kind !== "unknown_effect"
        ? {
            ...c,
            status: "withdrawn",
            resolution_summary: reason,
            revision: c.revision + 1,
            updated_at: now,
            resolved_at: now,
          }
        : c,
    );
    this.permissionRequests = this.permissionRequests.map((r) =>
      r.project_id === projectId && r.status === "pending"
        ? { ...r, status: "expired", revision: r.revision + 1, resolved_at: now }
        : r,
    );
    this.capabilityGrants = this.capabilityGrants.map((g) =>
      g.project_id === projectId && !g.revoked_at
        ? { ...g, revoked_at: now, revoked_reason: reason }
        : g,
    );
    this.sessions = this.sessions.map((s) =>
      s.project_id === projectId ? { ...s, status: "closed", updated_at: now } : s,
    );
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
      stage: "D2",
      profile_mode: this.runtimeIsSimulated ? "test_isolated" : "production",
      postgres_connected: true,
      postgres_role: "dennett_control_role",
      postgres_database: "dennett_control_test",
      schema_version: 7,
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
        "control.tasks.v1",
        "control.events.v1",
        "control.inbox.v1",
        "control.approvals.v1",
        "control.effects.v1",
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
    const activeConn =
      this.providerConnections.find((c) => c.is_default_interactive) ??
      this.providerConnections[0] ??
      null;
    return {
      adapter_id: this.runtimeAdapterId,
      auth_profile: this.authProfile,
      is_simulated: this.runtimeIsSimulated,
      agy_version:
        this.runtimeIsSimulated || this.runtimeAdapterId === "local-http-runtime"
          ? null
          : "1.5.15",
      tier: this.tier,
      account_label: this.accountLabel,
      default_model_id: this.models[0]?.model_id ?? null,
      default_thinking_level:
        this.models[0]?.default_thinking_level ??
        (this.runtimeAdapterId === "local-http-runtime" ? null : "medium"),
      models: [...this.models],
      thinking_options:
        this.runtimeAdapterId === "local-http-runtime"
          ? []
          : [
              { value: "low", label: "Low" },
              { value: "medium", label: "Medium" },
              { value: "high", label: "High" },
              { value: "xhigh", label: "Extra" },
            ],
      quotas: this.runtimeAdapterId === "local-http-runtime" ? [] : [...this.quotas],
      connection_id: activeConn?.connection_id ?? null,
      quota_error: this.quotaError,
      probed_at: Date.now(),
    };
  }

  public async pickWorkspaceFolder(): Promise<string | null> {
    this.calls.push({ method: "pickWorkspaceFolder" });
    return this.mockPickedFolder;
  }

  public async listProjects(includeArchived = false): Promise<ListProjectsResponse> {
    this.calls.push({ method: "listProjects", args: { includeArchived } });
    this.ensureConnected();
    const visible = this.projects.filter((p) => {
      const lc = p.lifecycle_state ?? "active";
      if (lc === "deleted_record") {
        return false;
      }
      if (!includeArchived && lc === "archived") {
        return false;
      }
      return true;
    });
    return { projects: sortProjectsByPin(visible) };
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
      lifecycle_state: "active",
      archived_at: null,
      pinned: false,
      pinned_at: null,
      created_at: this.nextTimestamp(),
    };
    this.projects = sortProjectsByPin([record, ...this.projects]);
    this.emitControlEvent("project.created", { projectId });
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
    const nextPinned = request.pinned !== undefined ? request.pinned : existing.pinned;
    const nextPinnedAt =
      request.pinned === true
        ? (existing.pinned_at ?? this.nextTimestamp())
        : request.pinned === false
          ? null
          : (existing.pinned_at ?? null);
    const updated: ProjectRecord = {
      ...existing,
      name: request.name?.trim() || existing.name,
      workspace_path:
        request.workspace_path !== undefined ? request.workspace_path : existing.workspace_path,
      lifecycle_state:
        request.workspace_path && existing.lifecycle_state === "detached"
          ? "active"
          : (existing.lifecycle_state ?? "active"),
      pinned: nextPinned,
      pinned_at: nextPinnedAt,
    };
    this.projects = sortProjectsByPin(
      this.projects.map((p) => (p.project_id === projectId ? updated : p)),
    );
    this.emitControlEvent("project.updated", { projectId });
    return updated;
  }

  public async rebindProject(
    projectId: string,
    request: RebindProjectRequest,
  ): Promise<ProjectLifecycleResponse> {
    this.calls.push({ method: "rebindProject", args: { projectId, request } });
    this.ensureConnected();
    const existing = this.projects.find((p) => p.project_id === projectId);
    if (!existing) {
      throw { code: "NOT_FOUND", message: `project ${projectId} not found` };
    }
    const updated: ProjectRecord = {
      ...existing,
      workspace_path: request.workspace_path.trim(),
      lifecycle_state: existing.lifecycle_state === "archived" ? "archived" : "active",
    };
    this.projects = this.projects.map((p) => (p.project_id === projectId ? updated : p));
    this.emitControlEvent("project.rebound", { projectId });
    return { project: updated, affected_timers: 0, user_workspace_files_deleted: false };
  }

  public async detachProjectWorkspace(projectId: string): Promise<ProjectLifecycleResponse> {
    this.calls.push({ method: "detachProjectWorkspace", args: projectId });
    this.ensureConnected();
    const existing = this.projects.find((p) => p.project_id === projectId);
    if (!existing) {
      throw { code: "NOT_FOUND", message: `project ${projectId} not found` };
    }
    const updated: ProjectRecord = {
      ...existing,
      workspace_path: null,
      lifecycle_state: "detached",
    };
    this.projects = this.projects.map((p) => (p.project_id === projectId ? updated : p));
    this.emitControlEvent("project.workspace_detached", { projectId });
    return { project: updated, affected_timers: 0, user_workspace_files_deleted: false };
  }

  public async archiveProject(projectId: string): Promise<ProjectLifecycleResponse> {
    this.calls.push({ method: "archiveProject", args: projectId });
    this.ensureConnected();
    const existing = this.projects.find((p) => p.project_id === projectId);
    if (!existing) {
      throw { code: "NOT_FOUND", message: `project ${projectId} not found` };
    }
    const updated: ProjectRecord = {
      ...existing,
      lifecycle_state: "archived",
      archived_at: Date.now(),
    };
    this.projects = this.projects.map((p) => (p.project_id === projectId ? updated : p));
    this.emitControlEvent("project.archived", { projectId });
    return { project: updated, affected_timers: 0, user_workspace_files_deleted: false };
  }

  public async unarchiveProject(projectId: string): Promise<ProjectLifecycleResponse> {
    this.calls.push({ method: "unarchiveProject", args: projectId });
    this.ensureConnected();
    const existing = this.projects.find((p) => p.project_id === projectId);
    if (!existing) {
      throw { code: "NOT_FOUND", message: `project ${projectId} not found` };
    }
    const updated: ProjectRecord = {
      ...existing,
      lifecycle_state: "active",
      archived_at: null,
    };
    this.projects = this.projects.map((p) => (p.project_id === projectId ? updated : p));
    this.emitControlEvent("project.unarchived", { projectId });
    return { project: updated, affected_timers: 0, user_workspace_files_deleted: false };
  }

  public async removeProject(projectId: string): Promise<ProjectLifecycleResponse> {
    this.calls.push({ method: "removeProject", args: projectId });
    this.ensureConnected();
    const existing = this.projects.find((p) => p.project_id === projectId);
    if (!existing) {
      throw { code: "NOT_FOUND", message: `project ${projectId} not found` };
    }
    this.terminateProjectWork(projectId, "Проект убран из списка");
    const updated: ProjectRecord = {
      ...existing,
      lifecycle_state: "deleted_record",
    };
    this.projects = this.projects.map((p) => (p.project_id === projectId ? updated : p));
    this.emitControlEvent("project.removed", { projectId });
    return { project: updated, affected_timers: 0, user_workspace_files_deleted: false };
  }

  public async deleteProject(
    projectId: string,
    confirm: boolean,
  ): Promise<ProjectLifecycleResponse> {
    this.calls.push({ method: "deleteProject", args: { projectId, confirm } });
    this.ensureConnected();
    if (!confirm) {
      throw { code: "INVALID_INPUT", message: "confirm=true is required to delete project" };
    }
    const existing = this.projects.find((p) => p.project_id === projectId);
    if (!existing) {
      throw { code: "NOT_FOUND", message: `project ${projectId} not found` };
    }
    this.terminateProjectWork(projectId, "Проект удалён из Dennett");
    const updated: ProjectRecord = {
      ...existing,
      workspace_path: null,
      lifecycle_state: "deleted_record",
    };
    this.projects = this.projects.map((p) => (p.project_id === projectId ? updated : p));
    this.emitControlEvent("project.deleted", { projectId });
    return { project: updated, affected_timers: 0, user_workspace_files_deleted: false };
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
    const allEntries = this.filesByProject[projectId] ?? [];
    if (subpath && subpath.trim().length > 0) {
      const normalized = subpath.trim().replace(/\\/g, "/").replace(/^\/+|\/+$/g, "");
      const prefix = `${normalized}/`;
      const children = allEntries.filter(
        (entry) =>
          entry.relative_path.startsWith(prefix) &&
          !entry.relative_path.slice(prefix.length).includes("/"),
      );
      return {
        project_id: projectId,
        workspace_bound: true,
        workspace_path: project.workspace_path,
        entries: children,
        truncated: false,
      };
    }
    return {
      project_id: projectId,
      workspace_bound: true,
      workspace_path: project.workspace_path,
      entries: [...allEntries],
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
        default_budget: request.default_budget ?? null,
        attached_skills: [...(request.attached_skills ?? [])],
        preferred_connection_id: request.preferred_connection_id ?? null,
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
        default_budget: request.default_budget ?? existing.current_config.default_budget ?? null,
        attached_skills: request.attached_skills ?? existing.current_config.attached_skills ?? [],
        preferred_connection_id:
          request.preferred_connection_id !== undefined
            ? request.preferred_connection_id
            : (existing.current_config.preferred_connection_id ?? null),
        created_at: now,
      },
    };
    this.agents = this.agents.map((a) => (a.agent_id === agentId ? updated : a));
    return updated;
  }

  public async sendAgentMessage(
    recipientAgentId: string,
    request: SendAgentMessageRequest,
  ): Promise<SendAgentMessageResponse> {
    this.calls.push({ method: "sendAgentMessage", args: { recipientAgentId, request } });
    this.ensureConnected();
    const existing = this.agentMessages.find(
      (m) =>
        m.recipient_agent_id === recipientAgentId && m.idempotency_key === request.idempotency_key,
    );
    if (existing) {
      return { message: existing, deduplicated: true };
    }
    const msg: AgentMessageRecord = {
      agent_message_id: this.nextUuid("85000000"),
      sender_agent_id: request.sender_agent_id ?? null,
      recipient_agent_id: recipientAgentId,
      project_id: request.project_id,
      related_task_id: request.related_task_id ?? null,
      idempotency_key: request.idempotency_key,
      body_text: request.body_text,
      delivered_in_run_id: null,
      correlation_id: "corr-agent-msg",
      created_at: Date.now(),
      delivered_at: null,
    };
    this.agentMessages = [...this.agentMessages, msg];
    return { message: msg, deduplicated: false };
  }

  public async listAgentMessages(
    recipientAgentId: string,
    undeliveredOnly = false,
  ): Promise<ListAgentMessagesResponse> {
    this.calls.push({ method: "listAgentMessages", args: { recipientAgentId, undeliveredOnly } });
    this.ensureConnected();
    const list = this.agentMessages.filter(
      (m) =>
        m.recipient_agent_id === recipientAgentId &&
        (!undeliveredOnly || m.delivered_in_run_id === null),
    );
    return { messages: list };
  }

  public async listSessions(): Promise<ListSessionsResponse> {
    this.calls.push({ method: "listSessions" });
    this.ensureConnected();
    return { sessions: sortSessionsByPin(this.sessions) };
  }

  public async createSession(request: CreateSessionRequest): Promise<SessionRecord> {
    this.calls.push({ method: "createSession", args: request });
    this.ensureConnected();
    const now = this.nextTimestamp();
    const sessionId = this.nextUuid("30000000");
    const session: SessionRecord = {
      session_id: sessionId,
      agent_id: request.agent_id,
      project_id: request.project_id ?? null,
      title: request.title,
      session_kind: request.session_kind ?? "user_chat",
      runtime_binding: null,
      status: "active",
      pinned: false,
      pinned_at: null,
      created_at: now,
      updated_at: now,
    };
    this.sessions = sortSessionsByPin([session, ...this.sessions]);
    this.messagesBySession[sessionId] = [];
    this.artifactsBySession[sessionId] = [];
    this.runsBySession[sessionId] = [];
    this.emitControlEvent("session.created", { projectId: session.project_id, sessionId });
    return session;
  }

  public async updateSession(
    sessionId: string,
    request: UpdateSessionRequest,
  ): Promise<SessionRecord> {
    this.calls.push({ method: "updateSession", args: { sessionId, request } });
    this.ensureConnected();
    if (request.title === undefined && request.pinned === undefined) {
      throw { code: "INVALID_INPUT", message: "updateSession requires title or pinned" };
    }
    const existing = this.sessions.find((s) => s.session_id === sessionId);
    if (!existing) {
      throw { code: "NOT_FOUND", message: `session ${sessionId} not found` };
    }
    const nextTitle = request.title !== undefined ? request.title.trim() : existing.title;
    if (request.title !== undefined && nextTitle.length === 0) {
      throw { code: "INVALID_INPUT", message: "session title cannot be empty" };
    }
    const nextPinned = request.pinned !== undefined ? request.pinned : existing.pinned;
    const nextPinnedAt =
      request.pinned === true
        ? (existing.pinned_at ?? this.nextTimestamp())
        : request.pinned === false
          ? null
          : (existing.pinned_at ?? null);
    const updated: SessionRecord = {
      ...existing,
      title: nextTitle,
      pinned: nextPinned,
      pinned_at: nextPinnedAt,
      updated_at: request.title !== undefined ? this.nextTimestamp() : existing.updated_at,
    };
    this.sessions = sortSessionsByPin(
      this.sessions.map((s) => (s.session_id === sessionId ? updated : s)),
    );
    this.emitControlEvent("session.updated", { projectId: updated.project_id, sessionId });
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
      permission_decisions: [...(this.permissionDecisionsBySession[sessionId] ?? [])],
    };
  }

  public async sendMessage(
    sessionId: string,
    request: SendMessageRequest,
  ): Promise<SendMessageResponse> {
    this.calls.push({ method: "sendMessage", args: { sessionId, request } });
    this.ensureConnected();

    if (request.client_message_id) {
      const dedupKey = `${sessionId}:${request.client_message_id}`;
      const existing = this.clientMessageIndex.get(dedupKey);
      if (existing) {
        return { ...existing, deduplicated: true };
      }
    }

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
    const project = session.project_id
      ? this.projects.find((p) => p.project_id === session.project_id)
      : undefined;

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
      client_message_id: request.client_message_id ?? null,
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
      workspace_cwd:
        project?.workspace_path ??
        (session.project_id ? "/detached" : `/standalone-sessions/${sessionId}`),
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
    this.sessions = sortSessionsByPin(
      this.sessions.map((s) =>
        s.session_id === sessionId
          ? { ...s, title: effectiveTitle, runtime_binding: binding, updated_at: now + 10 }
          : s,
      ),
    );

    const response: SendMessageResponse = {
      user_message: userMsg,
      assistant_message: assistantMsg,
      run,
      runtime_binding: binding,
      artifacts: [...(this.artifactsBySession[sessionId] ?? [])],
      permission_decisions: [],
      memory_status: this.memoryStatus,
      session_title: effectiveTitle,
      deduplicated: false,
    };

    if (request.client_message_id) {
      this.clientMessageIndex.set(`${sessionId}:${request.client_message_id}`, response);
    }
    this.emitControlEvent("session.message_completed", {
      projectId: session.project_id,
      sessionId,
    });
    return response;
  }

  public async cancelRun(sessionId: string): Promise<CancelRunResponse> {
    this.calls.push({ method: "cancelRun", args: sessionId });
    this.ensureConnected();
    const resolver = this.pendingCancelResolvers.get(sessionId);
    if (resolver) {
      resolver();
    }
    const hadSeededRunning = this.seededRunningSessionIds.delete(sessionId);
    const sessionRuns = this.runsBySession[sessionId] ?? [];
    let cancelledExistingRunId: string | null = null;
    if (sessionRuns.some((r) => r.status === "running")) {
      this.runsBySession[sessionId] = sessionRuns.map((r) => {
        if (r.status === "running") {
          cancelledExistingRunId = r.run_id;
          return { ...r, status: "cancelled", stop_reason: "cancelled", finished_at: Date.now() };
        }
        return r;
      });
    }
    const unknownEffects = this.effects.filter(
      (ef) => ef.session_id === sessionId && ef.status === "unknown",
    );
    const didCancel = Boolean(resolver) || hadSeededRunning || cancelledExistingRunId !== null;
    return {
      session_id: sessionId,
      run_id: cancelledExistingRunId ?? (didCancel ? this.nextUuid("40000000") : null),
      cancelled: didCancel,
      status: unknownEffects.length > 0 ? "unknown_external" : didCancel ? "cancelled" : null,
      memory_status: this.memoryStatus,
      external_outcome_unknown: unknownEffects.length > 0,
      committed_effects: this.effects.filter(
        (ef) => ef.session_id === sessionId && ef.status === "confirmed",
      ),
      unknown_effects: unknownEffects,
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
    if (session.project_id === null && request.relative_path) {
      throw {
        code: "INVALID_INPUT",
        message: "standalone sessions do not support workspace relative_path artifacts",
      };
    }
    const existing = this.artifactsBySession[sessionId] ?? [];
    const sameNameRevs = existing.filter((a) => a.name === request.name).length;
    const artifact: ArtifactRecord = {
      artifact_id: this.nextUuid("70000000"),
      session_id: sessionId,
      project_id: session.project_id ?? null,
      run_id: request.run_id ?? null,
      name: request.name,
      mime_type: request.mime_type,
      revision: sameNameRevs + 1,
      content: request.content ?? null,
      relative_path: request.relative_path ?? null,
      sha256: "a".repeat(64),
      completeness_status: request.completeness_status ?? "final",
      lifecycle_state: "active",
      created_at: Date.now(),
    };
    this.artifactsBySession[sessionId] = [...existing, artifact];
    this.emitControlEvent("artifact.created", { projectId: session.project_id, sessionId });
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

  public async listArtifactRevisions(
    sessionId: string,
    artifactIdOrName: string,
  ): Promise<ListArtifactsResponse> {
    this.calls.push({ method: "listArtifactRevisions", args: { sessionId, artifactIdOrName } });
    this.ensureConnected();
    const all = this.artifactsBySession[sessionId] ?? [];
    const byId = all.find((a) => a.artifact_id === artifactIdOrName);
    const logicalName = byId ? byId.name : artifactIdOrName;
    const matched = all
      .filter((a) => a.name === logicalName)
      .sort((a, b) => a.revision - b.revision);
    return { artifacts: matched };
  }

  public async applyArtifactLifecycle(
    sessionId: string,
    artifactId: string,
    request: ArtifactLifecycleRequest,
  ): Promise<ArtifactLifecycleResponse> {
    this.calls.push({
      method: "applyArtifactLifecycle",
      args: { sessionId, artifactId, request },
    });
    this.ensureConnected();
    const list = this.artifactsBySession[sessionId] ?? [];
    const existing = list.find((a) => a.artifact_id === artifactId);
    if (!existing) {
      throw { code: "NOT_FOUND", message: `artifact ${artifactId} not found` };
    }
    let updated: ArtifactRecord = { ...existing };
    switch (request.action) {
      case "finalize":
        updated = { ...updated, completeness_status: "final" };
        break;
      case "archive":
        updated = { ...updated, lifecycle_state: "archived" };
        break;
      case "unarchive":
        updated = { ...updated, lifecycle_state: "active" };
        break;
      case "remove":
        updated = { ...updated, lifecycle_state: "removed" };
        break;
      case "delete":
        updated = { ...updated, lifecycle_state: "deleted", content: null };
        break;
    }
    if (request.action === "remove") {
      this.artifactsBySession[sessionId] = list.map((a) =>
        a.name === existing.name && (a.lifecycle_state ?? "active") !== "deleted"
          ? { ...a, lifecycle_state: "removed" }
          : a,
      );
    } else {
      this.artifactsBySession[sessionId] = list.map((a) =>
        a.artifact_id === artifactId ? updated : a,
      );
    }
    this.emitControlEvent(`artifact.${request.action}`, { sessionId });
    return { artifact: updated, user_workspace_files_deleted: false };
  }

  public async getStateSnapshot(
    projectId?: string | null,
    includeArchived = false,
  ): Promise<StateSnapshotResponse> {
    this.calls.push({ method: "getStateSnapshot", args: { projectId, includeArchived } });
    this.ensureConnected();
    const projects = sortProjectsByPin(
      this.projects.filter((p) => {
        const lc = p.lifecycle_state ?? "active";
        if (lc === "deleted_record") {
          return false;
        }
        if (projectId && p.project_id !== projectId) {
          return false;
        }
        if (!includeArchived && !projectId && lc === "archived") {
          return false;
        }
        return true;
      }),
    );
    const sessions = sortSessionsByPin(
      this.sessions.filter((s) => {
        if (s.project_id === null) {
          return !projectId;
        }
        return !this.isProjectDeleted(s.project_id) && (!projectId || s.project_id === projectId);
      }),
    );
    const tasks = this.tasks.filter(
      (t) => !this.isProjectDeleted(t.project_id) && (!projectId || t.project_id === projectId),
    );
    const openInbox = this.inboxCards.filter(
      (c) =>
        c.status === "open" &&
        !this.isProjectDeleted(c.project_id) &&
        (!projectId || c.project_id === projectId),
    );
    const activeGrants = this.capabilityGrants.filter(
      (g) =>
        !g.revoked_at &&
        !this.isProjectDeleted(g.project_id) &&
        (!projectId || g.project_id === projectId),
    );
    const runningSessionIds = new Set<string>(this.seededRunningSessionIds);
    for (const sid of this.pendingCancelResolvers.keys()) {
      runningSessionIds.add(sid);
    }
    for (const [sid, runs] of Object.entries(this.runsBySession)) {
      if (runs.some((r) => r.status === "running")) {
        runningSessionIds.add(sid);
      }
    }
    const visibleRunningSessionIds = Array.from(runningSessionIds).filter((sid) => {
      const s = this.sessions.find((item) => item.session_id === sid);
      if (!s) {
        return true;
      }
      if (s.project_id === null) {
        return !projectId;
      }
      return !this.isProjectDeleted(s.project_id) && (!projectId || s.project_id === projectId);
    });
    return {
      snapshot_cursor: {
        authority_epoch: this.authorityEpoch,
        committed_seq: this.cursorSeq,
        retained_from_seq: this.retainedFromSeq,
      },
      projects,
      agents: [...this.agents],
      sessions,
      tasks,
      inbox_cards: openInbox,
      active_grants: activeGrants,
      unresolved_inbox_count: openInbox.length,
      running_session_ids: visibleRunningSessionIds,
    };
  }

  public async getEvents(
    afterCursor: number,
    expectedEpoch?: number | null,
    projectId?: string | null,
    limit?: number | null,
  ): Promise<EventStreamResponse> {
    this.calls.push({
      method: "getEvents",
      args: { afterCursor, expectedEpoch, projectId, limit },
    });
    this.ensureConnected();
    if (expectedEpoch !== undefined && expectedEpoch !== null && expectedEpoch !== this.authorityEpoch) {
      throw {
        code: "CONFLICT",
        message: `conflict on stream_epoch: expected epoch ${expectedEpoch}, current is ${this.authorityEpoch}`,
      };
    }
    if (afterCursor + 1 < this.retainedFromSeq) {
      throw {
        code: "CONFLICT",
        message: `conflict on stream_cursor_expired: cursor ${afterCursor} predates retained_from_seq ${this.retainedFromSeq}`,
      };
    }
    const maxItems = limit ?? 100;
    const scanned = this.events.filter((ev) => ev.cursor_seq > afterCursor).slice(0, maxItems);
    const scannedThrough =
      scanned.length > 0 ? scanned[scanned.length - 1]!.cursor_seq : afterCursor;
    const filtered = projectId ? scanned.filter((ev) => ev.project_id === projectId) : scanned;
    const hasMore = this.events.some((ev) => ev.cursor_seq > scannedThrough);
    return {
      authority_epoch: this.authorityEpoch,
      retained_from_seq: this.retainedFromSeq,
      after_cursor: afterCursor,
      scanned_through_seq: scannedThrough,
      next_cursor: scannedThrough,
      has_more: hasMore,
      events: filtered,
    };
  }

  public async createTask(request: CreateTaskRequest): Promise<TaskRecord> {
    this.calls.push({ method: "createTask", args: request });
    this.ensureConnected();
    const now = Date.now();
    let sessionId = request.session_id ?? null;
    if (!sessionId) {
      const createdSession = await this.createSession({
        agent_id: request.agent_id,
        project_id: request.project_id,
        title: request.title?.trim() || deriveAutoTitle(request.goal) || "Поручение",
        session_kind: "task_run",
      });
      sessionId = createdSession.session_id;
    }
    const taskId = this.nextUuid("75000000");
    const status = request.schedule
      ? "waiting_timer"
      : request.wait_for_signal
        ? "waiting_signal"
        : request.execute_immediately === false
          ? "queued"
          : "running";

    const nextFireAt =
      request.schedule?.kind.kind === "once_at"
        ? request.schedule.kind.fire_at
        : request.schedule?.kind.kind === "interval"
          ? now + request.schedule.kind.interval_secs * 1000
          : request.schedule?.kind.kind === "daily_wall_time"
            ? now + 3_600_000
            : null;

    const task: TaskRecord = {
      task_id: taskId,
      project_id: request.project_id,
      session_id: sessionId,
      agent_id: request.agent_id,
      parent_task_id: request.parent_task_id ?? null,
      title: request.title?.trim() || deriveAutoTitle(request.goal) || "Новое поручение",
      goal: request.goal.trim(),
      generation: 1,
      status,
      expected_signal_name: request.wait_for_signal?.trim() || null,
      wait_detail: request.wait_for_signal
        ? `Ожидает сигнал ${request.wait_for_signal.trim()}`
        : request.schedule
          ? request.schedule.original_expression ?? "Запланировано по таймеру"
          : null,
      partial_result_summary: null,
      error_code: null,
      error_summary: null,
      active_run_id: null,
      schedule: request.schedule
        ? {
            ...request.schedule,
            missed_run_policy: request.schedule.missed_run_policy ?? "execute_once",
            next_fire_at: request.schedule.next_fire_at ?? nextFireAt,
          }
        : null,
      budget: request.budget ?? { exceed_policy: "ask_approval" },
      usage: { turns_used: 0, tokens_used: 0, wall_ms_used: 0, confidence: "unknown" },
      attached_skills: [...(request.attached_skills ?? [])],
      auth_profile_override: request.auth_profile_override ?? null,
      created_at: now,
      updated_at: now,
      finished_at: null,
    };
    this.tasks = [task, ...this.tasks];
    this.emitControlEvent("task.created", { projectId: task.project_id, taskId: task.task_id });
    return task;
  }

  public async listTasks(
    projectId?: string | null,
    status?: string | null,
  ): Promise<ListTasksResponse> {
    this.calls.push({ method: "listTasks", args: { projectId, status } });
    this.ensureConnected();
    const list = this.tasks.filter(
      (t) =>
        !this.isProjectDeleted(t.project_id) &&
        (!projectId || t.project_id === projectId) &&
        (!status || t.status === status),
    );
    return { tasks: list };
  }

  public async getTaskDetail(taskId: string): Promise<TaskDetailResponse> {
    this.calls.push({ method: "getTaskDetail", args: taskId });
    this.ensureConnected();
    const task = this.tasks.find((t) => t.task_id === taskId);
    if (!task) {
      throw { code: "NOT_FOUND", message: `task ${taskId} not found` };
    }
    return {
      task,
      runs: (this.runsBySession[task.session_id] ?? []).filter((r) => r.task_id === taskId),
      timers: this.timersByTask[taskId] ?? [],
      signals: this.signalsByTask[taskId] ?? [],
      effects: this.effects.filter((ef) => ef.task_id === taskId),
      open_inbox_cards: this.inboxCards.filter(
        (c) => c.task_id === taskId && c.status === "open",
      ),
    };
  }

  public async updateTask(taskId: string, request: UpdateTaskRequest): Promise<TaskRecord> {
    this.calls.push({ method: "updateTask", args: { taskId, request } });
    this.ensureConnected();
    const existing = this.tasks.find((t) => t.task_id === taskId);
    if (!existing) {
      throw { code: "NOT_FOUND", message: `task ${taskId} not found` };
    }
    const bumpsGen = Boolean(request.goal || request.schedule || request.clear_schedule);
    const now = Date.now();
    const nextSchedule = request.clear_schedule
      ? null
      : request.schedule !== undefined
        ? request.schedule
        : existing.schedule;
    const nextStatus =
      request.schedule !== undefined && request.schedule !== null
        ? "waiting_timer"
        : existing.status === "waiting_signal" && !request.clear_schedule
          ? "waiting_signal"
          : existing.status;
    const nextExpectedSignal =
      nextStatus === "waiting_signal" ? (existing.expected_signal_name ?? null) : null;
    const updated: TaskRecord = {
      ...existing,
      title: request.title?.trim() || existing.title,
      goal: request.goal?.trim() || existing.goal,
      generation: bumpsGen ? existing.generation + 1 : existing.generation,
      status: nextStatus,
      expected_signal_name: nextExpectedSignal,
      schedule: nextSchedule,
      budget: request.budget ?? existing.budget,
      updated_at: now,
    };
    this.tasks = this.tasks.map((t) => (t.task_id === taskId ? updated : t));
    if (bumpsGen) {
      this.permissionRequests = this.permissionRequests.map((r) =>
        r.task_id === taskId && r.status === "pending"
          ? { ...r, status: "expired", revision: r.revision + 1, resolved_at: now }
          : r,
      );
      this.inboxCards = this.inboxCards.map((c) =>
        c.task_id === taskId &&
        c.status === "open" &&
        ["clarification", "missed_schedule", "budget_exceeded", "permission_approval"].includes(
          c.kind,
        )
          ? {
              ...c,
              status: "superseded",
              resolution_summary: "Заменено обновлением цели",
              revision: c.revision + 1,
              updated_at: now,
              resolved_at: now,
            }
          : c,
      );
    }
    this.emitControlEvent("task.updated", { projectId: updated.project_id, taskId });
    return updated;
  }

  public async sendTaskSignal(
    taskId: string,
    request: TaskSignalRequest,
  ): Promise<TaskSignalResponse> {
    this.calls.push({ method: "sendTaskSignal", args: { taskId, request } });
    this.ensureConnected();
    const existing = this.tasks.find((t) => t.task_id === taskId);
    if (!existing) {
      throw { code: "NOT_FOUND", message: `task ${taskId} not found` };
    }
    const signals = this.signalsByTask[taskId] ?? [];
    const dup = signals.find((s) => s.idempotency_key === request.idempotency_key);
    if (dup) {
      return { signal: dup, task: existing, deduplicated: true };
    }
    const now = Date.now();
    const trimmedName = request.signal_name.trim();
    const signalMatches =
      !existing.expected_signal_name || existing.expected_signal_name === trimmedName;
    const woke = existing.status === "waiting_signal" && signalMatches;
    const signal: TaskSignalRecord = {
      signal_id: this.nextUuid("76000000"),
      task_id: taskId,
      idempotency_key: request.idempotency_key,
      signal_name: trimmedName,
      payload_text: request.payload_text ?? "",
      payload_sha256: "b".repeat(64),
      applied_generation: existing.generation,
      outcome_status: existing.status === "cancelled" ? "rejected_stopped" : woke ? "woke_task" : "queued_on_task",
      correlation_id: "corr-signal-1",
      created_at: now,
    };
    this.signalsByTask[taskId] = [...signals, signal];
    const updated: TaskRecord = woke
      ? {
          ...existing,
          status: "running",
          expected_signal_name: null,
          wait_detail: null,
          updated_at: now,
        }
      : existing;
    this.tasks = this.tasks.map((t) => (t.task_id === taskId ? updated : t));
    this.emitControlEvent("task.signal_received", { projectId: updated.project_id, taskId });
    return { signal, task: updated, deduplicated: false };
  }

  public async replyToTask(taskId: string, request: ReplyTaskRequest): Promise<TaskRecord> {
    this.calls.push({ method: "replyToTask", args: { taskId, request } });
    this.ensureConnected();
    const existing = this.tasks.find((t) => t.task_id === taskId);
    if (!existing) {
      throw { code: "NOT_FOUND", message: `task ${taskId} not found` };
    }
    const now = Date.now();
    const updated: TaskRecord = {
      ...existing,
      status: "running",
      wait_detail: null,
      partial_result_summary: `Ответ получен: ${request.answer_text.trim()}`,
      updated_at: now,
    };
    this.tasks = this.tasks.map((t) => (t.task_id === taskId ? updated : t));
    this.inboxCards = this.inboxCards.map((c) =>
      c.task_id === taskId && c.kind === "clarification" && c.status === "open"
        ? { ...c, status: "resolved", resolution_summary: request.answer_text.trim(), resolved_at: now, revision: c.revision + 1 }
        : c,
    );
    this.emitControlEvent("task.clarification_answered", { projectId: updated.project_id, taskId });
    return updated;
  }

  public async cancelTask(taskId: string): Promise<CancelTaskResponse> {
    this.calls.push({ method: "cancelTask", args: taskId });
    this.ensureConnected();
    const existing = this.tasks.find((t) => t.task_id === taskId);
    if (!existing) {
      throw { code: "NOT_FOUND", message: `task ${taskId} not found` };
    }
    const unknownEffects = this.effects.filter(
      (ef) => ef.task_id === taskId && ef.status === "unknown",
    );
    const committedEffects = this.effects.filter(
      (ef) => ef.task_id === taskId && ef.status === "confirmed",
    );
    const hasUnknown = unknownEffects.length > 0 || existing.status === "unknown_external";
    const now = Date.now();
    const updated: TaskRecord = {
      ...existing,
      status: hasUnknown ? "unknown_external" : "cancelled",
      wait_detail: hasUnknown
        ? "Остановлено локально, но внешний исход остаётся неизвестным (UNKNOWN)."
        : null,
      finished_at: hasUnknown ? existing.finished_at ?? null : now,
      updated_at: now,
    };
    this.tasks = this.tasks.map((t) => (t.task_id === taskId ? updated : t));
    this.emitControlEvent(
      hasUnknown ? "task.stopped_with_unknown_external" : "task.cancelled",
      { projectId: updated.project_id, taskId },
    );
    return {
      task: updated,
      cancelled_run_id: existing.active_run_id ?? null,
      external_outcome_unknown: hasUnknown,
      committed_effects: committedEffects,
      unknown_effects: unknownEffects,
      memory_status: this.memoryStatus,
    };
  }

  public async listInboxCards(
    status?: string | null,
    projectId?: string | null,
  ): Promise<ListInboxResponse> {
    this.calls.push({ method: "listInboxCards", args: { status, projectId } });
    this.ensureConnected();
    const filtered = this.inboxCards.filter((c) => {
      if (this.isProjectDeleted(c.project_id)) {
        return false;
      }
      if (projectId && c.project_id !== projectId) {
        return false;
      }
      if (status && status !== "all" && c.status !== status) {
        return false;
      }
      return true;
    });
    const unresolvedCount = this.inboxCards.filter(
      (c) =>
        c.status === "open" &&
        !this.isProjectDeleted(c.project_id) &&
        (!projectId || c.project_id === projectId),
    ).length;
    return { cards: filtered, unresolved_count: unresolvedCount };
  }

  public async resolveInboxCard(
    cardId: string,
    request: ResolveInboxCardRequest,
  ): Promise<ResolveInboxCardResponse> {
    this.calls.push({ method: "resolveInboxCard", args: { cardId, request } });
    this.ensureConnected();
    const existing = this.inboxCards.find((c) => c.card_id === cardId);
    if (!existing) {
      throw { code: "NOT_FOUND", message: `inbox_card ${cardId} not found` };
    }
    if (existing.revision !== request.expected_revision || existing.status !== "open") {
      throw {
        code: "CONFLICT",
        message: "conflict on inbox_card: inbox card revision mismatch (already_resolved)",
      };
    }

    const now = Date.now();
    let updatedTask: TaskRecord | null = null;
    let updatedPerm: PermissionRequestRecord | null = null;
    let issuedGrant: CapabilityGrantRecord | null = null;
    let updatedEffect: EffectClaimRecord | null = null;
    let summary = "Решено владельцем";

    const res = request.resolution;
    if (res.action === "permission" && existing.permission_request_id) {
      const permResp = await this.resolvePermissionRequest(existing.permission_request_id, {
        decision: res.decision,
        reason: res.reason ?? null,
        max_uses: res.max_uses ?? null,
        expires_in_secs: res.expires_in_secs ?? null,
      });
      updatedPerm = permResp.request;
      issuedGrant = permResp.issued_grant ?? null;
      summary = `Решение: ${res.decision}`;
    } else if (res.action === "clarification" && existing.task_id) {
      updatedTask = await this.replyToTask(existing.task_id, { answer_text: res.answer_text });
      summary = `Ответ: ${res.answer_text}`;
    } else if (res.action === "reconcile_effect" && existing.effect_id) {
      const recResp = await this.reconcileEffect(existing.effect_id, {
        manual_resolution: res.manual_resolution ?? null,
        reason: res.reason ?? null,
      });
      updatedEffect = recResp.effect;
      updatedTask = recResp.task ?? null;
      summary = `Сверка эффекта: ${recResp.effect.status}`;
    } else if (res.action === "stop_task" && existing.task_id) {
      if (existing.kind === "unknown_effect") {
        throw {
          code: "CONFLICT",
          message: "Карточку неизвестного внешнего эффекта нельзя закрыть остановкой — выполните сверку.",
        };
      }
      const stopResp = await this.cancelTask(existing.task_id);
      updatedTask = stopResp.task;
      summary = "Поручение остановлено";
    } else if (res.action === "increase_budget" && existing.task_id) {
      const t = this.tasks.find((item) => item.task_id === existing.task_id);
      if (t) {
        updatedTask = {
          ...t,
          status: "queued",
          budget: {
            ...t.budget,
            max_turns:
              res.additional_turns && t.budget.max_turns
                ? t.budget.max_turns + res.additional_turns
                : t.budget.max_turns,
            max_tokens:
              res.additional_tokens && t.budget.max_tokens
                ? t.budget.max_tokens + res.additional_tokens
                : t.budget.max_tokens,
            max_wall_secs:
              res.additional_wall_secs && t.budget.max_wall_secs
                ? t.budget.max_wall_secs + res.additional_wall_secs
                : t.budget.max_wall_secs,
          },
          updated_at: now,
        };
        this.tasks = this.tasks.map((item) =>
          item.task_id === updatedTask!.task_id ? updatedTask! : item,
        );
      }
      summary = "Бюджет увеличен";
    } else if (res.action === "missed_schedule" && existing.task_id) {
      const t = this.tasks.find((item) => item.task_id === existing.task_id);
      if (t) {
        updatedTask = {
          ...t,
          status: res.run_now ? "queued" : "waiting_timer",
          updated_at: now,
        };
        this.tasks = this.tasks.map((item) =>
          item.task_id === updatedTask!.task_id ? updatedTask! : item,
        );
      }
      summary = res.run_now ? "Запущено один раз" : "Пропущенный запуск отложен";
    }

    const updatedCard: InboxCardRecord = {
      ...existing,
      status: "resolved",
      resolution_summary: summary,
      revision: existing.revision + 1,
      updated_at: now,
      resolved_at: now,
    };
    this.inboxCards = this.inboxCards.map((c) => (c.card_id === cardId ? updatedCard : c));
    this.emitControlEvent("inbox.resolved", { projectId: existing.project_id });
    return {
      card: updatedCard,
      task: updatedTask,
      permission_request: updatedPerm,
      issued_grant: issuedGrant,
      effect: updatedEffect,
    };
  }

  public async listPermissionRequests(
    status?: string | null,
    sessionId?: string | null,
  ): Promise<ListPermissionRequestsResponse> {
    this.calls.push({ method: "listPermissionRequests", args: { status, sessionId } });
    this.ensureConnected();
    const list = this.permissionRequests.filter(
      (r) =>
        !this.isProjectDeleted(r.project_id) &&
        (!status || status === "all" || r.status === status) &&
        (!sessionId || r.session_id === sessionId),
    );
    return { requests: list };
  }

  public async resolvePermissionRequest(
    requestId: string,
    request: ResolvePermissionRequest,
  ): Promise<ResolvePermissionResponse> {
    this.calls.push({ method: "resolvePermissionRequest", args: { requestId, request } });
    this.ensureConnected();
    const existing = this.permissionRequests.find((r) => r.request_id === requestId);
    if (!existing) {
      throw { code: "NOT_FOUND", message: `permission_request ${requestId} not found` };
    }
    if (
      request.expected_revision !== undefined &&
      request.expected_revision !== null &&
      existing.revision !== request.expected_revision
    ) {
      throw { code: "CONFLICT", message: "conflict on permission_request: stale revision" };
    }
    const now = Date.now();
    let issuedGrant: CapabilityGrantRecord | null = null;
    if (request.decision === "approve_project" || request.decision === "approve_task") {
      issuedGrant = {
        grant_id: this.nextUuid("82000000"),
        scope:
          request.decision === "approve_task" && existing.task_id
            ? `task:${existing.task_id}`
            : `project:${existing.project_id}`,
        project_id: existing.project_id,
        agent_id: existing.agent_id,
        session_id: existing.session_id,
        task_id: request.decision === "approve_task" ? (existing.task_id ?? null) : null,
        tool_pattern: "workspace_write",
        target_prefixes: [],
        max_uses: request.max_uses ?? null,
        uses_consumed: 1,
        reason: request.reason ?? "Одобрено владельцем для проекта",
        expires_at: request.expires_in_secs ? now + request.expires_in_secs * 1000 : null,
        revoked_at: null,
        revoked_reason: null,
        created_at: now,
      };
      this.capabilityGrants = [issuedGrant, ...this.capabilityGrants];
    }

    const nextStatus =
      request.decision === "deny"
        ? "denied"
        : request.decision === "approve_once"
          ? "approved_once"
          : "approved_scope";

    const decision: PermissionDecisionRecord = {
      decision_id: this.nextUuid("83000000"),
      session_id: existing.session_id,
      run_id: existing.run_id,
      tool_call_id: existing.tool_call_id,
      tool_name: existing.tool_name,
      target_pattern: existing.target_summary,
      scope: `project:${existing.project_id}`,
      effect: request.decision === "deny" ? "deny" : "allow",
      matched_rule_id: null,
      reason: request.reason ?? `Owner decision: ${request.decision}`,
      evaluated_outside_model: true,
      correlation_id: existing.correlation_id,
      created_at: now,
    };

    const updatedReq: PermissionRequestRecord = {
      ...existing,
      status: nextStatus,
      issued_grant_id: issuedGrant?.grant_id ?? null,
      decision_id: decision.decision_id,
      revision: existing.revision + 1,
      resolved_at: now,
    };
    this.permissionRequests = this.permissionRequests.map((r) =>
      r.request_id === requestId ? updatedReq : r,
    );
    this.inboxCards = this.inboxCards.map((c) =>
      c.permission_request_id === requestId && c.status === "open"
        ? { ...c, status: "resolved", resolved_at: now, revision: c.revision + 1 }
        : c,
    );
    this.emitControlEvent("permission.resolved", {
      projectId: existing.project_id,
      sessionId: existing.session_id,
    });
    return { request: updatedReq, decision, issued_grant: issuedGrant };
  }

  public async listCapabilityGrants(
    projectId?: string | null,
    includeRevoked = false,
  ): Promise<ListCapabilityGrantsResponse> {
    this.calls.push({ method: "listCapabilityGrants", args: { projectId, includeRevoked } });
    this.ensureConnected();
    const list = this.capabilityGrants.filter(
      (g) =>
        !this.isProjectDeleted(g.project_id) &&
        (!projectId || g.project_id === projectId) &&
        (includeRevoked || !g.revoked_at),
    );
    return { grants: list };
  }

  public async revokeCapabilityGrant(
    grantId: string,
    request?: RevokeGrantRequest,
  ): Promise<CapabilityGrantRecord> {
    this.calls.push({ method: "revokeCapabilityGrant", args: { grantId, request } });
    this.ensureConnected();
    const existing = this.capabilityGrants.find((g) => g.grant_id === grantId);
    if (!existing) {
      throw { code: "NOT_FOUND", message: `capability_grant ${grantId} not found` };
    }
    const updated: CapabilityGrantRecord = {
      ...existing,
      revoked_at: Date.now(),
      revoked_reason: request?.reason ?? "Отозвано пользователем",
    };
    this.capabilityGrants = this.capabilityGrants.map((g) =>
      g.grant_id === grantId ? updated : g,
    );
    this.emitControlEvent("grant.revoked", { projectId: existing.project_id });
    return updated;
  }

  public async dispatchEffect(request: DispatchEffectRequest): Promise<DispatchEffectResponse> {
    this.calls.push({ method: "dispatchEffect", args: request });
    this.ensureConnected();
    const existing = this.effects.find(
      (e) => e.effect_idempotency_key === request.effect_idempotency_key,
    );
    if (existing) {
      return {
        effect: existing,
        deduplicated: true,
        external_outcome_unknown: existing.status === "unknown",
      };
    }
    const now = Date.now();
    const isLostAck = request.simulated_behavior === "lost_ack";
    const status = isLostAck
      ? "unknown"
      : request.simulated_behavior === "denied"
        ? "denied"
        : request.simulated_behavior === "failed_before_accept"
          ? "failed"
          : "confirmed";
    const claim: EffectClaimRecord = {
      effect_id: this.nextUuid("88000000"),
      project_id: request.project_id,
      session_id: request.session_id,
      task_id: request.task_id ?? null,
      run_id: request.run_id ?? null,
      agent_id: request.agent_id,
      effect_idempotency_key: request.effect_idempotency_key,
      connector_id: request.connector_id ?? "simulated_external",
      operation: request.operation,
      target_recipient: request.target_recipient,
      parameters_json: request.parameters_json ?? "{}",
      parameters_sha256: "c".repeat(64),
      status,
      provider_operation_id: status === "confirmed" ? "prov-op-1" : null,
      receipt_json: status === "confirmed" ? "{\"ok\":true}" : null,
      error_summary: isLostAck ? "Lost acknowledgement from external service" : null,
      reconciled_at: null,
      correlation_id: "corr-effect-1",
      created_at: now,
      updated_at: now,
    };
    this.effects = [claim, ...this.effects];
    if (isLostAck) {
      this.inboxCards = [
        {
          card_id: this.nextUuid("89000000"),
          project_id: request.project_id,
          session_id: request.session_id,
          task_id: request.task_id ?? null,
          run_id: request.run_id ?? null,
          agent_id: request.agent_id,
          kind: "unknown_effect",
          status: "open",
          urgency: "critical",
          subject: `Неизвестный исход: ${request.operation} → ${request.target_recipient}`,
          detail:
            "Запрос отправлен во внешний сервис, но подтверждение потеряно. Слепой повтор заблокирован.",
          effect_id: claim.effect_id,
          revision: 1,
          created_at: now,
          updated_at: now,
        },
        ...this.inboxCards,
      ];
    }
    this.emitControlEvent(isLostAck ? "effect.unknown" : "effect.confirmed", {
      projectId: request.project_id,
      sessionId: request.session_id,
      taskId: request.task_id ?? null,
    });
    return { effect: claim, deduplicated: false, external_outcome_unknown: isLostAck };
  }

  public async listEffects(
    taskId?: string | null,
    sessionId?: string | null,
    status?: string | null,
  ): Promise<ListEffectsResponse> {
    this.calls.push({ method: "listEffects", args: { taskId, sessionId, status } });
    this.ensureConnected();
    const list = this.effects.filter(
      (ef) =>
        !this.isProjectDeleted(ef.project_id) &&
        (!taskId || ef.task_id === taskId) &&
        (!sessionId || ef.session_id === sessionId) &&
        (!status || ef.status === status),
    );
    return { effects: list };
  }

  public async reconcileEffect(
    effectId: string,
    request?: ReconcileEffectRequest,
  ): Promise<ReconcileEffectResponse> {
    this.calls.push({ method: "reconcileEffect", args: { effectId, request } });
    this.ensureConnected();
    const existing = this.effects.find((ef) => ef.effect_id === effectId);
    if (!existing) {
      throw { code: "NOT_FOUND", message: `effect_claim ${effectId} not found` };
    }
    const now = Date.now();
    const nextStatus =
      request?.manual_resolution === "mark_not_executed" ? "failed" : "confirmed";
    const updated: EffectClaimRecord = {
      ...existing,
      status: nextStatus,
      provider_operation_id:
        nextStatus === "confirmed" ? (existing.provider_operation_id ?? "prov-reconciled-1") : null,
      reconciled_at: now,
      updated_at: now,
    };
    this.effects = this.effects.map((ef) => (ef.effect_id === effectId ? updated : ef));
    this.inboxCards = this.inboxCards.map((c) =>
      c.effect_id === effectId && c.status === "open"
        ? {
            ...c,
            status: "resolved",
            resolution_summary:
              nextStatus === "confirmed" ? "Подтверждено выполнение" : "Отмечено невыполненным",
            resolved_at: now,
            revision: c.revision + 1,
          }
        : c,
    );
    let updatedTask: TaskRecord | null = null;
    if (existing.task_id) {
      const t = this.tasks.find((item) => item.task_id === existing.task_id);
      if (t && t.status === "unknown_external") {
        updatedTask = {
          ...t,
          status: nextStatus === "confirmed" ? "succeeded" : "failed",
          wait_detail: null,
          finished_at: now,
          updated_at: now,
        };
        this.tasks = this.tasks.map((item) =>
          item.task_id === updatedTask!.task_id ? updatedTask! : item,
        );
      }
    }
    this.emitControlEvent("effect.reconciled", {
      projectId: existing.project_id,
      sessionId: existing.session_id,
      taskId: existing.task_id ?? null,
    });
    return {
      effect: updated,
      task: updatedTask,
      provider_found: nextStatus === "confirmed",
      provider_execution_count: nextStatus === "confirmed" ? 1 : 0,
      resolution_summary:
        nextStatus === "confirmed"
          ? "Сверка завершена: сервис подтвердил выполнение (1 вызов, дубль не отправлялся)."
          : "Сверка завершена: отмечено невыполненным — повтор безопасен.",
    };
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
          safe_summary: "dennett-head готов к работе (D2)",
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
        index_status: "degraded",
        stale_warning: "Память memoryd недоступна.",
      };
    }
    const baseNotes = request.scope
      ? [...(this.notesByScope[request.scope] ?? [])]
      : Object.values(this.notesByScope).flat();
    const q = request.query?.trim().toLowerCase() ?? "";
    const matchedNotes: MemoryNote[] = [];
    for (const note of baseNotes) {
      const currentMatches =
        !q ||
        note.title.toLowerCase().includes(q) ||
        note.body.toLowerCase().includes(q) ||
        note.tags.some((t) => t.toLowerCase().includes(q)) ||
        (note.topic_key?.toLowerCase().includes(q) ?? false);
      if (currentMatches) {
        matchedNotes.push(note);
        continue;
      }
      if (request.include_history) {
        const revs = this.noteRevisionsById[note.note_id] ?? [];
        const histMatch = revs.some(
          (r) =>
            r.title.toLowerCase().includes(q) ||
            r.body.toLowerCase().includes(q) ||
            (r.rationale?.toLowerCase().includes(q) ?? false),
        );
        if (histMatch) {
          matchedNotes.push(note);
        }
      }
    }
    const limited = request.limit ? matchedNotes.slice(0, request.limit) : matchedNotes;
    const staleWarning =
      this.staleWarning ??
      (this.indexStatus === "degraded"
        ? "Индекс деградирован — свежий хвост активен, результаты возвращаются из живого потока."
        : this.indexStatus === "rebuilding"
          ? "Идёт перестроение поискового индекса — свежие записи доступны через хвост."
          : this.indexStatus === "lagging"
            ? "Индекс догоняет канонический журнал — свежий хвост активен."
            : null);
    return {
      memory_status: "healthy",
      degraded_reason: null,
      notes: limited,
      index_status: this.indexStatus,
      stale_warning: staleWarning,
    };
  }

  // ============================================================================
  // Phase D3 Mock Methods: SecretBroker, Providers, Capabilities, Memory Fabric
  // ============================================================================

  public async listSecrets(): Promise<ListSecretsResponse> {
    this.calls.push({ method: "listSecrets" });
    this.ensureConnected();
    return { secrets: this.secrets.map((s) => ({ ...s })) };
  }

  public async setSecret(
    slotKey: string,
    request: SetSecretRequest,
  ): Promise<SecretMetadataRecord> {
    // Never record plaintext secret value in calls log!
    this.calls.push({
      method: "setSecret",
      args: { slotKey, scope: request.scope ?? "global", configured: true },
    });
    this.ensureConnected();
    const trimmed = request.value.trim();
    if (!trimmed) {
      throw { code: "INVALID_INPUT", message: "Секрет не может быть пустым." };
    }
    const now = this.nextTimestamp();
    const existing = this.secrets.find((s) => s.slot_key === slotKey);
    const updated: SecretMetadataRecord = {
      slot_key: slotKey,
      scope: request.scope ?? existing?.scope ?? "global",
      description: request.description ?? existing?.description ?? "",
      configured: true,
      version: (existing?.version ?? 0) + 1,
      fingerprint_short: deterministicHex(`${slotKey}:${trimmed}:${now}`, 16),
      source: "vault",
      updated_at: now,
    };
    if (existing) {
      this.secrets = this.secrets.map((s) => (s.slot_key === slotKey ? updated : s));
    } else {
      this.secrets = [...this.secrets, updated];
    }
    if (slotKey === "gemini_api_key") {
      this.providerConnections = this.providerConnections.map((c) =>
        c.auth_profile === "gemini-api-key"
          ? { ...c, last_health_status: "healthy", updated_at: now }
          : c,
      );
    }
    return updated;
  }

  public async deleteSecret(slotKey: string): Promise<SecretMetadataRecord> {
    this.calls.push({ method: "deleteSecret", args: { slotKey } });
    this.ensureConnected();
    const now = this.nextTimestamp();
    const existing = this.secrets.find((s) => s.slot_key === slotKey);
    const cleared: SecretMetadataRecord = {
      slot_key: slotKey,
      scope: existing?.scope ?? "global",
      description: existing?.description ?? "",
      configured: false,
      version: existing?.version ?? 0,
      fingerprint_short: null,
      source: "unset",
      updated_at: now,
    };
    if (existing) {
      this.secrets = this.secrets.map((s) => (s.slot_key === slotKey ? cleared : s));
    }
    if (slotKey === "gemini_api_key") {
      this.providerConnections = this.providerConnections.map((c) =>
        c.auth_profile === "gemini-api-key"
          ? { ...c, last_health_status: "auth_required", updated_at: now }
          : c,
      );
    }
    return cleared;
  }

  public async listProviderConnections(): Promise<ListProviderConnectionsResponse> {
    this.calls.push({ method: "listProviderConnections" });
    this.ensureConnected();
    return {
      connections: this.providerConnections.map((c) => ({ ...c })),
      planned_providers: this.plannedProviders.map((p) => ({ ...p })),
    };
  }

  public async createProviderConnection(
    request: UpsertProviderConnectionRequest,
  ): Promise<ProviderConnectionRecord> {
    this.calls.push({ method: "createProviderConnection", args: request });
    this.ensureConnected();
    const now = this.nextTimestamp();
    const conn: ProviderConnectionRecord = {
      connection_id: this.nextUuid("91000000"),
      provider_kind: request.provider_kind,
      display_name: request.display_name.trim(),
      adapter_id:
        request.adapter_id ??
        (request.provider_kind === "local_http" ? "local-http-runtime" : "antigravity-acp"),
      auth_profile: request.auth_profile,
      endpoint_url: request.endpoint_url ?? null,
      secret_slot_key: request.secret_slot_key ?? null,
      is_default_interactive: Boolean(request.is_default_interactive),
      is_default_background: Boolean(request.is_default_background),
      enabled: request.enabled ?? true,
      last_health_status: "unknown",
      last_health_detail: null,
      last_probed_at: null,
      created_at: now,
      updated_at: now,
    };
    this.providerConnections = [...this.providerConnections, conn];
    return conn;
  }

  public async updateProviderConnection(
    connectionId: string,
    request: UpsertProviderConnectionRequest,
  ): Promise<ProviderConnectionRecord> {
    this.calls.push({ method: "updateProviderConnection", args: { connectionId, request } });
    this.ensureConnected();
    const existing = this.providerConnections.find((c) => c.connection_id === connectionId);
    if (!existing) {
      throw { code: "NOT_FOUND", message: `provider_connection ${connectionId} not found` };
    }
    const now = this.nextTimestamp();
    const updated: ProviderConnectionRecord = {
      ...existing,
      provider_kind: request.provider_kind,
      display_name: request.display_name.trim(),
      adapter_id: request.adapter_id ?? existing.adapter_id,
      auth_profile: request.auth_profile,
      endpoint_url: request.endpoint_url ?? null,
      secret_slot_key: request.secret_slot_key ?? null,
      is_default_interactive:
        request.is_default_interactive ?? existing.is_default_interactive,
      is_default_background: request.is_default_background ?? existing.is_default_background,
      enabled: request.enabled ?? existing.enabled,
      updated_at: now,
    };
    this.providerConnections = this.providerConnections.map((c) =>
      c.connection_id === connectionId ? updated : c,
    );
    return updated;
  }

  public async deleteProviderConnection(
    connectionId: string,
  ): Promise<DeleteProviderConnectionResponse> {
    this.calls.push({ method: "deleteProviderConnection", args: { connectionId } });
    this.ensureConnected();
    this.providerConnections = this.providerConnections.filter(
      (c) => c.connection_id !== connectionId,
    );
    return { connection_id: connectionId, deleted: true };
  }

  public async probeProviderConnection(connectionId: string): Promise<ProbeConnectionResponse> {
    this.calls.push({ method: "probeProviderConnection", args: { connectionId } });
    this.ensureConnected();
    const existing = this.providerConnections.find((c) => c.connection_id === connectionId);
    if (!existing) {
      throw { code: "NOT_FOUND", message: `provider_connection ${connectionId} not found` };
    }
    const now = this.nextTimestamp();
    const isLocal = existing.provider_kind === "local_http";
    const probedModels = isLocal ? [...this.localProbeModels] : [...this.models];
    const updated: ProviderConnectionRecord = {
      ...existing,
      last_health_status: "healthy",
      last_health_detail: isLocal
        ? `Локальный сервер отвечает (${existing.endpoint_url ?? "loopback"}), моделей: ${probedModels.length}`
        : "Живой каталог моделей получен",
      last_probed_at: now,
      updated_at: now,
    };
    this.providerConnections = this.providerConnections.map((c) =>
      c.connection_id === connectionId ? updated : c,
    );
    const catalog: RuntimeCatalogResponse = {
      adapter_id: updated.adapter_id,
      auth_profile: updated.auth_profile,
      is_simulated: !isLocal && this.runtimeIsSimulated,
      agy_version: isLocal ? null : "1.5.15",
      tier: isLocal ? updated.display_name : this.tier,
      account_label: isLocal ? null : this.accountLabel,
      default_model_id: probedModels[0]?.model_id ?? null,
      default_thinking_level: isLocal ? null : "medium",
      models: probedModels,
      thinking_options: isLocal
        ? []
        : [
            { value: "low", label: "Low" },
            { value: "medium", label: "Medium" },
            { value: "high", label: "High" },
            { value: "xhigh", label: "Extra" },
          ],
      quotas: isLocal ? [] : [...this.quotas],
      connection_id: updated.connection_id,
      quota_error: isLocal ? null : this.quotaError,
      probed_at: now,
    };
    return { connection: updated, catalog };
  }

  public async activateProviderConnection(
    connectionId: string,
    request?: ActivateConnectionRequest,
  ): Promise<ProviderConnectionRecord> {
    this.calls.push({ method: "activateProviderConnection", args: { connectionId, request } });
    this.ensureConnected();
    const existing = this.providerConnections.find((c) => c.connection_id === connectionId);
    if (!existing) {
      throw { code: "NOT_FOUND", message: `provider_connection ${connectionId} not found` };
    }
    const role = request?.role ?? "interactive";
    const now = this.nextTimestamp();
    this.providerConnections = this.providerConnections.map((c) => {
      const match = c.connection_id === connectionId;
      return {
        ...c,
        is_default_interactive:
          role === "interactive" || role === "both" ? match : c.is_default_interactive,
        is_default_background:
          role === "background" || role === "both" ? match : c.is_default_background,
        updated_at: match ? now : c.updated_at,
      };
    });
    if (role === "interactive" || role === "both") {
      this.runtimeAdapterId = existing.adapter_id;
      this.authProfile = existing.auth_profile;
      if (existing.provider_kind === "local_http") {
        this.models = [...this.localProbeModels];
        this.tier = existing.display_name;
      } else {
        this.models = this.runtimeIsSimulated
          ? [...DEFAULT_SIMULATED_MODELS]
          : [...DEFAULT_ANTIGRAVITY_MODELS];
        this.tier = existing.auth_profile === "oauth-personal" ? "Google AI Ultra" : null;
      }
    }
    return this.providerConnections.find((c) => c.connection_id === connectionId)!;
  }

  public async listCapabilities(
    projectId?: string | null,
    scopeKind?: CapabilityScopeKind | null,
    kind?: CapabilityKind | null,
  ): Promise<ListCapabilitiesResponse> {
    this.calls.push({ method: "listCapabilities", args: { projectId, scopeKind, kind } });
    this.ensureConnected();
    const filtered = this.capabilities.filter((c) => {
      if (projectId && c.scope_kind === "project" && c.project_id !== projectId) {
        return false;
      }
      if (scopeKind && c.scope_kind !== scopeKind) {
        return false;
      }
      if (kind && c.kind !== kind) {
        return false;
      }
      return true;
    });
    return { capabilities: filtered.map((c) => ({ ...c })) };
  }

  public async createCapability(request: CreateCapabilityRequest): Promise<CapabilityRecord> {
    this.calls.push({ method: "createCapability", args: request });
    this.ensureConnected();
    const now = this.nextTimestamp();
    const record: CapabilityRecord = {
      capability_id: this.nextUuid("92000000"),
      slug: request.slug.trim(),
      kind: request.spec.kind,
      display_name: request.display_name.trim(),
      description: request.description ?? "",
      version: request.version.trim(),
      origin_kind: request.origin_kind ?? "user_manual",
      ownership: request.ownership ?? "user",
      scope_kind: request.scope_kind,
      project_id: request.project_id ?? null,
      possession_state: "installed",
      trust_state: request.trust_state ?? "unreviewed",
      activation_state: request.activation_state ?? "disabled",
      compatibility_status: "compatible",
      compatibility_detail: null,
      source_path: request.source_path ?? null,
      content_sha256: deterministicHex(JSON.stringify(request.spec), 64),
      spec: request.spec,
      can_rollback: false,
      revision: 1,
      project_bindings: [],
      created_at: now,
      updated_at: now,
    };
    this.capabilities = [record, ...this.capabilities];
    return record;
  }

  public async discoverCapabilities(
    request: DiscoverCapabilitiesRequest,
  ): Promise<DiscoverCapabilitiesResponse> {
    this.calls.push({ method: "discoverCapabilities", args: request });
    this.ensureConnected();
    const now = this.nextTimestamp();
    const discovered: CapabilityRecord[] = [];
    let newlyRegistered = 0;

    if (request.project_id) {
      const slug = "project-rust-checklist";
      const existing = this.capabilities.find(
        (c) => c.slug === slug && c.project_id === request.project_id,
      );
      if (!existing) {
        const cap: CapabilityRecord = {
          capability_id: this.nextUuid("92000000"),
          slug,
          kind: "skill",
          display_name: "Rust Checklist (проект)",
          description: "Обнаружен в .agents/skills/project-rust-checklist/SKILL.md",
          version: "1.0.0",
          origin_kind: "project_local",
          ownership: "project",
          scope_kind: "project",
          project_id: request.project_id,
          possession_state: "discovered",
          trust_state: "unreviewed",
          activation_state: "disabled",
          compatibility_status: "compatible",
          compatibility_detail: null,
          source_path: ".agents/skills/project-rust-checklist",
          content_sha256: deterministicHex(slug, 64),
          spec: {
            kind: "skill",
            skill_markdown: "# Rust Checklist\nПроверяй clippy и тесты.",
            has_scripts: false,
            entry_files: ["SKILL.md"],
          },
          can_rollback: false,
          revision: 1,
          project_bindings: [],
          created_at: now,
          updated_at: now,
        };
        this.capabilities = [cap, ...this.capabilities];
        discovered.push(cap);
        newlyRegistered += 1;
      } else {
        discovered.push(existing);
      }
    }

    if (request.scan_personal_gemini_readonly) {
      const slug = "personal-gemini-notes";
      const existing = this.capabilities.find(
        (c) => c.slug === slug && c.origin_kind === "personal_discovered",
      );
      if (!existing) {
        const cap: CapabilityRecord = {
          capability_id: this.nextUuid("92000000"),
          slug,
          kind: "skill",
          display_name: "Личный навык (~/.gemini/)",
          description:
            "Найден в режиме только чтения в ~/.gemini/ — кандидат, не подключён к проектам.",
          version: "1.0.0",
          origin_kind: "personal_discovered",
          ownership: "user",
          scope_kind: "candidate_unattached",
          project_id: null,
          possession_state: "discovered",
          trust_state: "unreviewed",
          activation_state: "disabled",
          compatibility_status: "compatible",
          compatibility_detail: "Только чтение · без автоподключения",
          source_path: "~/.gemini/skills/personal-gemini-notes",
          content_sha256: deterministicHex(slug, 64),
          spec: {
            kind: "skill",
            skill_markdown: "# Personal skill\nRead-only candidate.",
            has_scripts: false,
            entry_files: ["SKILL.md"],
          },
          can_rollback: false,
          revision: 1,
          project_bindings: [],
          created_at: now,
          updated_at: now,
        };
        this.capabilities = [cap, ...this.capabilities];
        discovered.push(cap);
        newlyRegistered += 1;
      } else {
        discovered.push(existing);
      }
    }

    return {
      discovered,
      newly_registered: newlyRegistered,
      updated_existing: discovered.length - newlyRegistered,
      warnings: [],
    };
  }

  public async updateCapability(
    capabilityId: string,
    request: UpdateCapabilityRequest,
  ): Promise<CapabilityRecord> {
    this.calls.push({ method: "updateCapability", args: { capabilityId, request } });
    this.ensureConnected();
    const existing = this.capabilities.find((c) => c.capability_id === capabilityId);
    if (!existing) {
      throw { code: "NOT_FOUND", message: `capability ${capabilityId} not found` };
    }
    const now = this.nextTimestamp();
    const versionOrSpecChanged =
      (request.version !== undefined && request.version !== null && request.version !== existing.version) ||
      (request.spec !== undefined && request.spec !== null);
    if (versionOrSpecChanged) {
      this.previousCapabilitySnapshots.set(capabilityId, { ...existing });
    }
    const nextSpec = request.spec ?? existing.spec;
    let nextTrust = request.trust_state ?? existing.trust_state;
    if (
      request.spec &&
      ((request.spec.kind === "skill" &&
        request.spec.has_scripts &&
        existing.spec.kind === "skill" &&
        !existing.spec.has_scripts) ||
        (request.spec.kind === "plugin" &&
          request.spec.has_hooks &&
          existing.spec.kind === "plugin" &&
          !existing.spec.has_hooks))
    ) {
      nextTrust = "restricted";
    }
    const updated: CapabilityRecord = {
      ...existing,
      display_name: request.display_name?.trim() || existing.display_name,
      description: request.description ?? existing.description,
      version: request.version?.trim() || existing.version,
      scope_kind: request.scope_kind ?? existing.scope_kind,
      project_id: request.project_id !== undefined ? request.project_id : existing.project_id,
      trust_state: nextTrust,
      activation_state: request.activation_state ?? existing.activation_state,
      possession_state:
        (request.activation_state ?? existing.activation_state) === "active"
          ? "connected"
          : existing.possession_state,
      spec: nextSpec,
      can_rollback: versionOrSpecChanged || existing.can_rollback,
      revision: existing.revision + 1,
      updated_at: now,
    };
    this.capabilities = this.capabilities.map((c) =>
      c.capability_id === capabilityId ? updated : c,
    );
    return updated;
  }

  public async deleteCapability(capabilityId: string): Promise<DeleteCapabilityResponse> {
    this.calls.push({ method: "deleteCapability", args: { capabilityId } });
    this.ensureConnected();
    this.capabilities = this.capabilities.filter((c) => c.capability_id !== capabilityId);
    this.previousCapabilitySnapshots.delete(capabilityId);
    return { capability_id: capabilityId, deleted: true };
  }

  public async rollbackCapability(capabilityId: string): Promise<CapabilityRecord> {
    this.calls.push({ method: "rollbackCapability", args: { capabilityId } });
    this.ensureConnected();
    const prev = this.previousCapabilitySnapshots.get(capabilityId);
    const existing = this.capabilities.find((c) => c.capability_id === capabilityId);
    if (!existing) {
      throw { code: "NOT_FOUND", message: `capability ${capabilityId} not found` };
    }
    const now = this.nextTimestamp();
    const restored: CapabilityRecord = prev
      ? {
          ...prev,
          can_rollback: false,
          revision: existing.revision + 1,
          updated_at: now,
        }
      : {
          ...existing,
          version: "1.0.0",
          can_rollback: false,
          revision: existing.revision + 1,
          updated_at: now,
        };
    this.previousCapabilitySnapshots.delete(capabilityId);
    this.capabilities = this.capabilities.map((c) =>
      c.capability_id === capabilityId ? restored : c,
    );
    return restored;
  }

  public async setProjectCapabilityBinding(
    projectId: string,
    capabilityId: string,
    request: SetProjectCapabilityBindingRequest,
  ): Promise<CapabilityProjectBinding> {
    this.calls.push({
      method: "setProjectCapabilityBinding",
      args: { projectId, capabilityId, request },
    });
    this.ensureConnected();
    const now = this.nextTimestamp();
    const binding: CapabilityProjectBinding = {
      capability_id: capabilityId,
      project_id: projectId,
      activation_state: request.activation_state,
      updated_at: now,
    };
    this.capabilities = this.capabilities.map((c) => {
      if (c.capability_id !== capabilityId) {
        return c;
      }
      const prev = c.project_bindings ?? [];
      const exists = prev.some((b) => b.project_id === projectId);
      return {
        ...c,
        project_bindings: exists
          ? prev.map((b) => (b.project_id === projectId ? binding : b))
          : [...prev, binding],
      };
    });
    return binding;
  }

  public async commitMemoryNote(
    request: CommitMemoryNoteRequest,
  ): Promise<CommitMemoryNoteResponse> {
    this.calls.push({ method: "commitMemoryNote", args: request });
    this.ensureConnected();
    const now = this.nextTimestamp();
    const eventId = this.nextUuid("97000000");
    const noteId = this.nextUuid("94000000");
    const revId = this.nextUuid("95000000");
    const outboxId = this.nextUuid("98000000");
    const delivered = this.memoryStatus === "healthy";
    if (delivered) {
      const note: MemoryNote = {
        note_id: noteId,
        scope: request.scope,
        current_revision: 1,
        revision: 1,
        current_revision_id: revId,
        title: request.title.trim(),
        body: request.body.trim(),
        tags: [...(request.tags ?? [])],
        rationale: null,
        last_event_id: eventId,
        recorded_by: "owner",
        author_label: "owner",
        created_at: now,
        updated_at: now,
        admission_status: "admitted",
        salience: 60,
        trust_level: "native",
        instruction_active: false,
      };
      this.notesByScope[request.scope] = [note, ...(this.notesByScope[request.scope] ?? [])];
      this.noteRevisionsById[noteId] = [
        {
          revision_id: revId,
          note_id: noteId,
          revision: 1,
          title: note.title,
          body: note.body,
          tags: [...note.tags],
          rationale: null,
          event_id: eventId,
          recorded_by: "owner",
          author_label: "owner",
          created_at: now,
        },
      ];
    } else {
      this.pendingOutboxCount += 1;
    }
    return {
      outbox_item: {
        outbox_id: outboxId,
        session_id: request.session_id ?? null,
        run_id: request.run_id ?? null,
        idempotency_key: request.idempotency_key ?? `note-${now}`,
        scope: request.scope,
        operation: "create_note",
        payload_json: JSON.stringify(request),
        status: delivered ? "delivered" : "pending",
        attempts: 1,
        last_error: delivered ? null : (this.memoryDetail ?? "memoryd недоступна"),
        delivered_event_id: delivered ? eventId : null,
        delivered_note_id: delivered ? noteId : null,
        correlation_id: `corr-note-${now}`,
        created_at: now,
        updated_at: now,
      },
      delivered_immediately: delivered,
      memory_status: this.memoryStatus,
    };
  }

  public async getMemoryOverview(scope?: string | null): Promise<MemoryOverviewResponse> {
    this.calls.push({ method: "getMemoryOverview", args: { scope: scope ?? null } });
    this.ensureConnected();
    if (this.memoryStatus === "degraded_unreachable") {
      return {
        memory_status: "degraded_unreachable",
        degraded_reason: this.memoryDetail ?? "memoryd недоступна",
        scope: scope ?? null,
        stats: null,
        notes: [],
        claims: [],
        conflicts: [],
        documents: [],
        consolidation_jobs: [],
        tombstones: [],
      };
    }
    const notes = scope
      ? [...(this.notesByScope[scope] ?? [])]
      : Object.values(this.notesByScope).flat();
    const claims = scope
      ? this.memoryClaims.filter((c) => c.scope === scope)
      : [...this.memoryClaims];
    const conflicts = scope
      ? this.memoryConflicts.filter((c) => c.scope === scope)
      : [...this.memoryConflicts];
    const documents = scope
      ? this.memoryDocuments.filter((d) => d.scope === scope)
      : [...this.memoryDocuments];
    const jobs = scope
      ? this.consolidationJobs.filter((j) => j.scope === scope)
      : [...this.consolidationJobs];
    const tombstones = scope
      ? this.tombstones.filter((t) => t.scope === scope)
      : [...this.tombstones];

    const activeNotesCount = notes.filter((n) => (n.admission_status ?? "admitted") !== "rejected").length;
    const activeClaimsCount = claims.filter(
      (c) => c.epistemic_status !== "superseded" && c.epistemic_status !== "retracted",
    ).length;
    const openConflictsCount = conflicts.filter((c) => c.status === "open").length;
    const importedUntrustedCount =
      notes.filter((n) => n.trust_level === "external_untrusted").length +
      claims.filter((c) => c.trust_level === "external_untrusted").length +
      documents.filter((d) => d.trust_level === "external_untrusted").length;

    return {
      memory_status: "healthy",
      degraded_reason: null,
      scope: scope ?? null,
      stats: {
        scope: scope ?? null,
        sources_count: this.memorySourcesCount,
        active_notes_count: activeNotesCount,
        active_claims_count: activeClaimsCount,
        documents_count: documents.length,
        open_conflicts_count: openConflictsCount,
        imported_untrusted_count: importedUntrustedCount,
        index_status: this.indexStatus,
        watermark_seq: notes.length + claims.length + documents.length,
        last_rebuild_at: this.lastRebuildAt,
      },
      notes,
      claims,
      conflicts,
      documents,
      consolidation_jobs: jobs,
      tombstones,
    };
  }

  public async correctMemoryNote(request: CorrectNoteRequest): Promise<NoteMutationResponse> {
    this.calls.push({ method: "correctMemoryNote", args: request });
    this.ensureConnected();
    const list = this.notesByScope[request.scope] ?? [];
    const existing = list.find((n) => n.note_id === request.note_id);
    if (!existing) {
      throw { code: "NOT_FOUND", message: `note ${request.note_id} not found` };
    }
    const curRev = existing.current_revision ?? existing.revision ?? 1;
    if (curRev !== request.expected_revision) {
      throw {
        code: "CONFLICT",
        message: `conflict on memory_note: expected revision ${request.expected_revision}, got ${curRev}`,
      };
    }
    const now = this.nextTimestamp();
    const nextRev = curRev + 1;
    const revId = this.nextUuid("95000000");
    const eventId = this.nextUuid("97000000");
    const updated: MemoryNote = {
      ...existing,
      current_revision: nextRev,
      revision: nextRev,
      current_revision_id: revId,
      title: request.title.trim(),
      body: request.body.trim(),
      tags: [...(request.tags ?? existing.tags)],
      rationale: request.rationale.trim(),
      last_event_id: eventId,
      updated_at: now,
    };
    this.notesByScope[request.scope] = list.map((n) =>
      n.note_id === request.note_id ? updated : n,
    );
    const prevRevs = this.noteRevisionsById[request.note_id] ?? [];
    this.noteRevisionsById[request.note_id] = [
      ...prevRevs,
      {
        revision_id: revId,
        note_id: request.note_id,
        revision: nextRev,
        title: updated.title,
        body: updated.body,
        tags: [...updated.tags],
        rationale: updated.rationale ?? null,
        event_id: eventId,
        recorded_by: "owner",
        author_label: request.author_label ?? "owner",
        created_at: now,
      },
    ];
    return { note: updated, event_id: eventId, idempotent_replay: false };
  }

  public async getMemoryNoteHistory(noteId: string): Promise<NoteHistoryResponse> {
    this.calls.push({ method: "getMemoryNoteHistory", args: { noteId } });
    this.ensureConnected();
    const note = Object.values(this.notesByScope)
      .flat()
      .find((n) => n.note_id === noteId);
    if (!note) {
      throw { code: "NOT_FOUND", message: `note ${noteId} not found` };
    }
    const revisions = this.noteRevisionsById[noteId] ?? [
      {
        revision_id: note.current_revision_id ?? this.nextUuid("95000000"),
        note_id: note.note_id,
        revision: note.current_revision ?? note.revision ?? 1,
        title: note.title,
        body: note.body,
        tags: [...note.tags],
        rationale: note.rationale ?? null,
        event_id: note.last_event_id ?? this.nextUuid("97000000"),
        recorded_by: note.recorded_by ?? "owner",
        author_label: note.author_label ?? null,
        created_at: note.updated_at,
      },
    ];
    return {
      note_id: noteId,
      scope: note.scope,
      current_revision: note.current_revision ?? note.revision ?? 1,
      revisions,
    };
  }

  public async listMemoryClaims(
    scope?: string | null,
    topicKey?: string | null,
  ): Promise<ListClaimsResponse> {
    this.calls.push({ method: "listMemoryClaims", args: { scope, topicKey } });
    this.ensureConnected();
    const claims = this.memoryClaims.filter(
      (c) => (!scope || c.scope === scope) && (!topicKey || c.topic_key === topicKey),
    );
    return { claims };
  }

  public async createMemoryClaim(request: CreateClaimRequest): Promise<MemoryClaim> {
    this.calls.push({ method: "createMemoryClaim", args: request });
    this.ensureConnected();
    const now = this.nextTimestamp();
    const claim: MemoryClaim = {
      claim_id: this.nextUuid("93000000"),
      scope: request.scope,
      topic_key: request.topic_key.trim(),
      claim_key: request.claim_key ?? null,
      natural_language: request.natural_language.trim(),
      epistemic_status: request.epistemic_status ?? "asserted",
      supersedes_claim_id: request.supersedes_claim_id ?? null,
      valid_from: request.valid_from ?? now,
      valid_to: request.valid_to ?? null,
      source_event_id: this.nextUuid("97000000"),
      source_note_id: request.source_note_id ?? null,
      caveats: [...(request.caveats ?? [])],
      provenance: { ...(request.provenance ?? {}) },
      trust_level: "native",
      instruction_active: false,
      created_at: now,
    };
    this.memoryClaims = [claim, ...this.memoryClaims];
    return claim;
  }

  public async supersedeMemoryClaim(request: SupersedeClaimRequest): Promise<MemoryClaim> {
    this.calls.push({ method: "supersedeMemoryClaim", args: request });
    this.ensureConnected();
    const oldClaim = this.memoryClaims.find((c) => c.claim_id === request.claim_id);
    if (!oldClaim) {
      throw { code: "NOT_FOUND", message: `claim ${request.claim_id} not found` };
    }
    this.memoryClaims = this.memoryClaims.map((c) =>
      c.claim_id === request.claim_id ? { ...c, epistemic_status: "superseded" } : c,
    );
    return this.createMemoryClaim({
      ...request.new_claim,
      supersedes_claim_id: request.claim_id,
    });
  }

  public async listMemoryConflicts(scope?: string | null): Promise<ListConflictsResponse> {
    this.calls.push({ method: "listMemoryConflicts", args: { scope } });
    this.ensureConnected();
    return {
      conflicts: scope
        ? this.memoryConflicts.filter((c) => c.scope === scope)
        : [...this.memoryConflicts],
    };
  }

  public async resolveMemoryConflict(request: ResolveConflictRequest): Promise<MemoryConflict> {
    this.calls.push({ method: "resolveMemoryConflict", args: request });
    this.ensureConnected();
    const existing = this.memoryConflicts.find((c) => c.conflict_id === request.conflict_id);
    if (!existing) {
      throw { code: "NOT_FOUND", message: `conflict ${request.conflict_id} not found` };
    }
    const now = this.nextTimestamp();
    const resolutionEventId = this.nextUuid("97000000");
    const resolved: MemoryConflict = {
      ...existing,
      status: request.target_status ?? "resolved",
      resolution_event_id: resolutionEventId,
      resolution_note: request.resolution_note.trim(),
      resolved_at: now,
    };
    this.memoryConflicts = this.memoryConflicts.map((c) =>
      c.conflict_id === request.conflict_id ? resolved : c,
    );
    if (request.winning_claim_id) {
      this.memoryClaims = this.memoryClaims.map((cl) => {
        if (!existing.candidate_claim_ids.includes(cl.claim_id)) {
          return cl;
        }
        return cl.claim_id === request.winning_claim_id
          ? { ...cl, epistemic_status: "asserted" }
          : { ...cl, epistemic_status: "superseded" };
      });
    }
    return resolved;
  }

  public async listMemoryDocuments(scope?: string | null): Promise<ListDocumentsResponse> {
    this.calls.push({ method: "listMemoryDocuments", args: { scope } });
    this.ensureConnected();
    return {
      documents: scope
        ? this.memoryDocuments.filter((d) => d.scope === scope)
        : [...this.memoryDocuments],
    };
  }

  public async createMemoryDocument(request: CreateDocumentRequest): Promise<MemoryDocument> {
    this.calls.push({ method: "createMemoryDocument", args: request });
    this.ensureConnected();
    const now = this.nextTimestamp();
    const docId = this.nextUuid("96000000");
    const eventId = this.nextUuid("97000000");
    const doc: MemoryDocument = {
      document_id: docId,
      scope: request.scope,
      document_key: request.document_key.trim(),
      title: request.title.trim(),
      current_version: 1,
      current_body: request.body,
      provenance_claim_ids: [...(request.provenance_claim_ids ?? [])],
      provenance_note_ids: [...(request.provenance_note_ids ?? [])],
      source_event_id: eventId,
      principal_id: "owner",
      trust_level: "native",
      instruction_active: false,
      created_at: now,
      updated_at: now,
    };
    this.memoryDocuments = [doc, ...this.memoryDocuments];
    this.documentVersionsById[docId] = [
      {
        version_id: this.nextUuid("96100000"),
        document_id: docId,
        version: 1,
        body: doc.current_body,
        provenance_claim_ids: [...(doc.provenance_claim_ids ?? [])],
        diff_summary: "Создание досье",
        event_id: eventId,
        created_at: now,
      },
    ];
    return doc;
  }

  public async getMemoryDocument(documentId: string): Promise<MemoryDocument> {
    this.calls.push({ method: "getMemoryDocument", args: { documentId } });
    this.ensureConnected();
    const doc = this.memoryDocuments.find((d) => d.document_id === documentId);
    if (!doc) {
      throw { code: "NOT_FOUND", message: `document ${documentId} not found` };
    }
    return doc;
  }

  public async updateMemoryDocument(request: UpdateDocumentRequest): Promise<MemoryDocument> {
    this.calls.push({ method: "updateMemoryDocument", args: request });
    this.ensureConnected();
    const existing = this.memoryDocuments.find((d) => d.document_id === request.document_id);
    if (!existing) {
      throw { code: "NOT_FOUND", message: `document ${request.document_id} not found` };
    }
    if (existing.current_version !== request.expected_version) {
      throw {
        code: "CONFLICT",
        message: `conflict on memory_document: expected version ${request.expected_version}, got ${existing.current_version}`,
      };
    }
    const now = this.nextTimestamp();
    const nextVersion = existing.current_version + 1;
    const eventId = this.nextUuid("97000000");
    const updated: MemoryDocument = {
      ...existing,
      title: request.title.trim(),
      current_version: nextVersion,
      current_body: request.body,
      provenance_claim_ids: [...(request.provenance_claim_ids ?? existing.provenance_claim_ids ?? [])],
      provenance_note_ids: [...(request.provenance_note_ids ?? existing.provenance_note_ids ?? [])],
      source_event_id: eventId,
      updated_at: now,
    };
    this.memoryDocuments = this.memoryDocuments.map((d) =>
      d.document_id === request.document_id ? updated : d,
    );
    const prevVersions = this.documentVersionsById[request.document_id] ?? [];
    this.documentVersionsById[request.document_id] = [
      ...prevVersions,
      {
        version_id: this.nextUuid("96100000"),
        document_id: request.document_id,
        version: nextVersion,
        body: updated.current_body,
        provenance_claim_ids: [...(updated.provenance_claim_ids ?? [])],
        diff_summary: request.diff_summary ?? `Обновление v${nextVersion}`,
        event_id: eventId,
        created_at: now,
      },
    ];
    return updated;
  }

  public async getMemoryDocumentVersions(
    documentId: string,
  ): Promise<GetDocumentVersionsResponse> {
    this.calls.push({ method: "getMemoryDocumentVersions", args: { documentId } });
    this.ensureConnected();
    const doc = this.memoryDocuments.find((d) => d.document_id === documentId);
    if (!doc) {
      throw { code: "NOT_FOUND", message: `document ${documentId} not found` };
    }
    return {
      document_id: documentId,
      scope: doc.scope,
      current_version: doc.current_version,
      versions: [...(this.documentVersionsById[documentId] ?? [])],
    };
  }

  public async forgetMemoryItem(request: ForgetRequest): Promise<ForgetResponse> {
    this.calls.push({ method: "forgetMemoryItem", args: request });
    this.ensureConnected();
    const now = this.nextTimestamp();
    const tombstoneId = this.nextUuid("98100000");
    const forgetEventId = this.nextUuid("97000000");
    if (request.target_type === "note") {
      this.notesByScope[request.scope] = (this.notesByScope[request.scope] ?? []).filter(
        (n) => n.note_id !== request.target_id,
      );
    } else if (request.target_type === "claim") {
      this.memoryClaims = this.memoryClaims.filter((c) => c.claim_id !== request.target_id);
    } else if (request.target_type === "document") {
      this.memoryDocuments = this.memoryDocuments.filter(
        (d) => d.document_id !== request.target_id,
      );
    }
    const notice =
      "Запись удалена из активных проекций и поискового индекса; содержимое может сохраняться в офлайн-резервных копиях до их плановой ротации.";
    const tombstone: ForgetTombstone = {
      tombstone_id: tombstoneId,
      scope: request.scope,
      target_type: request.target_type,
      target_id: request.target_id,
      forget_event_id: forgetEventId,
      principal_id: "owner",
      reason: request.reason.trim(),
      residual_backup_notice: notice,
      propagation_status: "completed",
      created_at: now,
    };
    this.tombstones = [tombstone, ...this.tombstones];
    return {
      tombstone_id: tombstoneId,
      forget_event_id: forgetEventId,
      residual_backup_notice: notice,
      propagation_status: "completed",
      idempotent_replay: false,
    };
  }

  public async listMemoryTombstones(scope?: string | null): Promise<ListTombstonesResponse> {
    this.calls.push({ method: "listMemoryTombstones", args: { scope } });
    this.ensureConnected();
    return {
      tombstones: scope
        ? this.tombstones.filter((t) => t.scope === scope)
        : [...this.tombstones],
    };
  }

  public async exportMemoryPackage(request: ExportRequest): Promise<ExportResponse> {
    this.calls.push({ method: "exportMemoryPackage", args: request });
    this.ensureConnected();
    const notes = [...(this.notesByScope[request.scope] ?? [])];
    const claims = this.memoryClaims.filter((c) => c.scope === request.scope);
    const documents = this.memoryDocuments.filter((d) => d.scope === request.scope);
    const pkg = {
      format_version: request.format_version ?? "1.0",
      scope: request.scope,
      secrets_stripped: true,
      notes,
      claims,
      documents,
    };
    const packageJson = JSON.stringify(pkg, null, 2);
    return {
      package_id: this.nextUuid("98200000"),
      scope: request.scope,
      format_version: pkg.format_version,
      manifest_sha256: deterministicHex(packageJson, 64),
      item_count: notes.length + claims.length + documents.length,
      byte_size: packageJson.length,
      secrets_stripped: true,
      package_json: packageJson,
    };
  }

  public async importMemoryPackage(request: ImportRequest): Promise<ImportResponse> {
    this.calls.push({ method: "importMemoryPackage", args: { scope: request.scope } });
    this.ensureConnected();
    const now = this.nextTimestamp();
    const sha = deterministicHex(request.package_json, 64);
    let parsed: {
      notes?: Array<{ title?: string; body?: string; tags?: string[] }>;
      claims?: Array<{ topic_key?: string; natural_language?: string; caveats?: string[] }>;
      documents?: Array<{ document_key?: string; title?: string; body?: string; current_body?: string }>;
    };
    try {
      parsed = JSON.parse(request.package_json) as typeof parsed;
      if (!parsed || typeof parsed !== "object") {
        throw new Error("Ожидается JSON-объект пакета памяти");
      }
    } catch (err) {
      return {
        import_id: this.nextUuid("98300000"),
        scope: request.scope,
        source_package_sha256: sha,
        trust_level: "external_untrusted",
        instruction_active: false,
        items_imported: 0,
        items_rejected: 1,
        validation_errors: [err instanceof Error ? err.message : "Невалидный JSON"],
      };
    }

    let imported = 0;
    let rejected = 0;
    const errors: string[] = [];

    for (const rawNote of parsed.notes ?? []) {
      if (!rawNote.title?.trim() || !rawNote.body?.trim()) {
        rejected += 1;
        errors.push("Пропущена заметка с пустым заголовком или текстом");
        continue;
      }
      const noteId = this.nextUuid("94000000");
      const revId = this.nextUuid("95000000");
      const evId = this.nextUuid("97000000");
      const note: MemoryNote = {
        note_id: noteId,
        scope: request.scope,
        current_revision: 1,
        revision: 1,
        current_revision_id: revId,
        title: rawNote.title.trim(),
        body: rawNote.body.trim(),
        tags: [...(rawNote.tags ?? []), "imported"],
        last_event_id: evId,
        recorded_by: "import",
        created_at: now,
        updated_at: now,
        admission_status: "admitted",
        salience: 40,
        trust_level: "external_untrusted",
        instruction_active: false,
      };
      this.notesByScope[request.scope] = [note, ...(this.notesByScope[request.scope] ?? [])];
      this.noteRevisionsById[noteId] = [
        {
          revision_id: revId,
          note_id: noteId,
          revision: 1,
          title: note.title,
          body: note.body,
          tags: [...note.tags],
          rationale: "Внешний недоверенный импорт",
          event_id: evId,
          recorded_by: "import",
          created_at: now,
        },
      ];
      imported += 1;
    }

    for (const rawClaim of parsed.claims ?? []) {
      if (!rawClaim.topic_key?.trim() || !rawClaim.natural_language?.trim()) {
        rejected += 1;
        errors.push("Пропущено утверждение без topic_key или текста");
        continue;
      }
      const claim: MemoryClaim = {
        claim_id: this.nextUuid("93000000"),
        scope: request.scope,
        topic_key: rawClaim.topic_key.trim(),
        natural_language: rawClaim.natural_language.trim(),
        epistemic_status: "inferred",
        source_event_id: this.nextUuid("97000000"),
        caveats: [...(rawClaim.caveats ?? [])],
        trust_level: "external_untrusted",
        instruction_active: false,
        created_at: now,
      };
      this.memoryClaims = [claim, ...this.memoryClaims];
      imported += 1;
    }

    for (const rawDoc of parsed.documents ?? []) {
      const body = rawDoc.body ?? rawDoc.current_body ?? "";
      if (!rawDoc.title?.trim() || !body.trim()) {
        rejected += 1;
        errors.push("Пропущено досье без заголовка или содержимого");
        continue;
      }
      const docId = this.nextUuid("96000000");
      const evId = this.nextUuid("97000000");
      const doc: MemoryDocument = {
        document_id: docId,
        scope: request.scope,
        document_key: rawDoc.document_key?.trim() || `dossier:imported-${docId.slice(-6)}`,
        title: rawDoc.title.trim(),
        current_version: 1,
        current_body: body,
        provenance_claim_ids: [],
        provenance_note_ids: [],
        source_event_id: evId,
        principal_id: "import",
        trust_level: "external_untrusted",
        instruction_active: false,
        created_at: now,
        updated_at: now,
      };
      this.memoryDocuments = [doc, ...this.memoryDocuments];
      this.documentVersionsById[docId] = [
        {
          version_id: this.nextUuid("96100000"),
          document_id: docId,
          version: 1,
          body,
          provenance_claim_ids: [],
          diff_summary: "Внешний недоверенный импорт",
          event_id: evId,
          created_at: now,
        },
      ];
      imported += 1;
    }

    return {
      import_id: this.nextUuid("98300000"),
      scope: request.scope,
      source_package_sha256: sha,
      trust_level: "external_untrusted",
      instruction_active: false,
      items_imported: imported,
      items_rejected: rejected,
      validation_errors: errors,
    };
  }

  public async triggerMemoryIndexRebuild(
    request: TriggerIndexRebuildRequest,
  ): Promise<TriggerIndexRebuildResponse> {
    this.calls.push({ method: "triggerMemoryIndexRebuild", args: request });
    this.ensureConnected();
    const now = this.nextTimestamp();
    this.indexStatus = "healthy";
    this.staleWarning = null;
    this.lastRebuildAt = now;
    const notesCount = (this.notesByScope[request.scope] ?? []).length;
    const claimsCount = this.memoryClaims.filter((c) => c.scope === request.scope).length;
    const docsCount = this.memoryDocuments.filter((d) => d.scope === request.scope).length;
    const rebuilt = notesCount + claimsCount + docsCount;
    return {
      index_state: {
        scope: request.scope,
        watermark_event_id: this.nextUuid("97000000"),
        watermark_seq: rebuilt,
        model_version: "fts-v1",
        status: "healthy",
        last_rebuild_at: now,
        updated_at: now,
      },
      entries_rebuilt: rebuilt,
    };
  }

  public async listMemoryConsolidationJobs(
    scope?: string | null,
  ): Promise<ListConsolidationJobsResponse> {
    this.calls.push({ method: "listMemoryConsolidationJobs", args: { scope } });
    this.ensureConnected();
    return {
      jobs: scope
        ? this.consolidationJobs.filter((j) => j.scope === scope)
        : [...this.consolidationJobs],
    };
  }

  public async createMemoryConsolidationJob(
    request: CreateConsolidationJobRequest,
  ): Promise<ConsolidationJob> {
    this.calls.push({ method: "createMemoryConsolidationJob", args: request });
    this.ensureConnected();
    const now = this.nextTimestamp();
    const job: ConsolidationJob = {
      job_id: this.nextUuid("98400000"),
      scope: request.scope,
      job_type: request.job_type,
      status: "completed",
      checkpoint_json: JSON.stringify({ processed: true }),
      watermark_event_id: this.nextUuid("97000000"),
      caveats_preserved: true,
      error_detail: null,
      created_at: now,
      updated_at: now,
      started_at: now,
      completed_at: now,
    };
    this.consolidationJobs = [job, ...this.consolidationJobs];
    return job;
  }

  public async pauseMemoryConsolidationJob(jobId: string): Promise<ConsolidationJob> {
    this.calls.push({ method: "pauseMemoryConsolidationJob", args: { jobId } });
    this.ensureConnected();
    const existing = this.consolidationJobs.find((j) => j.job_id === jobId);
    if (!existing) {
      throw { code: "NOT_FOUND", message: `consolidation_job ${jobId} not found` };
    }
    const updated: ConsolidationJob = {
      ...existing,
      status: "paused",
      updated_at: this.nextTimestamp(),
    };
    this.consolidationJobs = this.consolidationJobs.map((j) =>
      j.job_id === jobId ? updated : j,
    );
    return updated;
  }

  public async resumeMemoryConsolidationJob(jobId: string): Promise<ConsolidationJob> {
    this.calls.push({ method: "resumeMemoryConsolidationJob", args: { jobId } });
    this.ensureConnected();
    const existing = this.consolidationJobs.find((j) => j.job_id === jobId);
    if (!existing) {
      throw { code: "NOT_FOUND", message: `consolidation_job ${jobId} not found` };
    }
    const now = this.nextTimestamp();
    const updated: ConsolidationJob = {
      ...existing,
      status: "completed",
      updated_at: now,
      completed_at: now,
    };
    this.consolidationJobs = this.consolidationJobs.map((j) =>
      j.job_id === jobId ? updated : j,
    );
    return updated;
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
        lifecycle_state: "active",
        archived_at: null,
        pinned: false,
        pinned_at: null,
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
        title: "Обзор архитектуры D2",
        runtime_binding: {
          binding_id: "60000000-0000-4000-8000-000000000001",
          session_id: sessionId,
          adapter_id: "antigravity-acp",
          auth_profile: "oauth-personal",
          tier: "Google AI Ultra",
          agy_version: "1.5.15",
          provider_session_id: "prov-preview-1",
          workspace_cwd: "A:/Dev/dennett-agent-orchestrator",
          is_simulated: false,
          rehydrate_count: 0,
          created_at: now - 250_000,
          updated_at: now - 120_000,
        },
        status: "active",
        pinned: false,
        pinned_at: null,
        created_at: now - 250_000,
        updated_at: now - 120_000,
      },
    ],
    filesByProject: {
      [projectId]: [
        { relative_path: "apps", name: "apps", kind: "directory", size_bytes: null, depth: 1 },
        {
          relative_path: "crates",
          name: "crates",
          kind: "directory",
          size_bytes: null,
          depth: 1,
        },
        {
          relative_path: "README.md",
          name: "README.md",
          kind: "file",
          size_bytes: 2840,
          depth: 1,
        },
      ],
    },
    messagesBySession: {
      [sessionId]: [
        {
          message_id: "50000000-0000-4000-8000-000000000001",
          session_id: sessionId,
          role: "user",
          content: "Проверь статус поручений, таймеров и интерактивных разрешений D2.",
          run_id: null,
          correlation_id: "corr-preview-1",
          created_at: now - 180_000,
        },
        {
          message_id: "50000000-0000-4000-8000-000000000002",
          session_id: sessionId,
          role: "assistant",
          content:
            "Демон `dennett-head` активен на схеме `v5`. Поручения, таймеры, Action Inbox и интерактивный режим «Спрашивать запись» подключены по курсору `/v1/events`.",
          run_id: null,
          correlation_id: "corr-preview-1",
          created_at: now - 175_000,
        },
      ],
    },
  });
}
