import {
  createContext,
  use,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
  type RefObject,
} from "react";

import type { DesktopBridge, WindowControls } from "../api/bridge";
import {
  isStreamResyncConflict,
  normalizeBridgeError,
  sortProjectsByPin,
  sortSessionsByPin,
  type AgentRecord,
  type ArtifactCompleteness,
  type ArtifactLifecycleAction,
  type ArtifactRecord,
  type BridgeErrorPayload,
  type CancelTaskResponse,
  type CapabilityActivation,
  type CapabilityGrantRecord,
  type CapabilityRecord,
  type CapabilityScopeKind,
  type ConnectionActivationRole,
  type ConsolidationJob,
  type ConsolidationJobType,
  type CreateCapabilityRequest,
  type CreateTaskRequest,
  type DesktopHealthSnapshot,
  type DiagnosticsResponse,
  type DiscoverCapabilitiesResponse,
  type EffectClaimRecord,
  type InboxCardRecord,
  type InboxResolutionAction,
  type ListProjectFilesResponse,
  type LiveRunSnapshot,
  type ManualEffectResolution,
  type MemoryOverviewResponse,
  type MessageRecord,
  type PermissionRequestRecord,
  type PermissionResolutionDecision,
  type PlannedProviderInfo,
  type ProbeConnectionResponse,
  type ProjectRecord,
  type ProviderConnectionRecord,
  type QueuedOfflineMessage,
  type RuntimeCatalogResponse,
  type SecretMetadataRecord,
  type SessionDetailResponse,
  type SessionFollowupRecord,
  type SessionRecord,
  type StreamCursor,
  type TaskRecord,
  type TaskSignalResponse,
  type ThinkingLevel,
  type TriggerIndexRebuildResponse,
  type UpdateCapabilityRequest,
  type UpsertProviderConnectionRequest,
  type WorkspaceWritePolicy,
} from "../api/contracts";
import { formatDecisionReasonRu } from "../utils/formatters";

export type WorkspaceView = "chat" | "inbox" | "tasks" | "memory";

export type FolderPickerIntent =
  | { mode: "create-project" }
  | { mode: "attach-project"; projectId: string }
  | { mode: "rebind-project"; projectId: string }
  | {
      mode: "add-capability";
      kind: "skill" | "plugin";
      scopeKind: CapabilityScopeKind;
      projectId: string | null;
    };

export interface ReconcileNotice {
  tone: "ok" | "warn" | "neutral";
  text: string;
}

const DRAFTS_STORAGE_KEY = "dennett.desktop.d2.drafts.v1";

const TERMINAL_QUEUE_ERROR_CODES = new Set([
  "NOT_FOUND",
  "HTTP_404",
  "CONFLICT",
  "HTTP_409",
  "PROJECT_ARCHIVED",
  "INVALID_INPUT",
  "HTTP_400",
  "PERMISSION_DENIED",
  "HTTP_403",
]);

interface PersistedDraftStore {
  draftsBySession: Record<string, string>;
  queuedMessages: QueuedOfflineMessage[];
}

function readPersistedDrafts(): PersistedDraftStore {
  if (typeof window === "undefined") {
    return { draftsBySession: {}, queuedMessages: [] };
  }
  try {
    const raw = window.localStorage.getItem(DRAFTS_STORAGE_KEY);
    if (!raw) {
      return { draftsBySession: {}, queuedMessages: [] };
    }
    const parsed = JSON.parse(raw) as Partial<PersistedDraftStore>;
    const draftsBySession =
      parsed.draftsBySession && typeof parsed.draftsBySession === "object"
        ? (parsed.draftsBySession as Record<string, string>)
        : {};
    const queuedMessages = Array.isArray(parsed.queuedMessages)
      ? parsed.queuedMessages.filter(
          (item): item is QueuedOfflineMessage =>
            typeof item === "object" &&
            item !== null &&
            typeof item.client_message_id === "string" &&
            typeof item.session_id === "string" &&
            typeof item.content === "string",
        )
      : [];
    return { draftsBySession, queuedMessages };
  } catch {
    return { draftsBySession: {}, queuedMessages: [] };
  }
}

function writePersistedDrafts(store: PersistedDraftStore): void {
  if (typeof window === "undefined") {
    return;
  }
  try {
    window.localStorage.setItem(DRAFTS_STORAGE_KEY, JSON.stringify(store));
  } catch {
    // Ignore storage quota or private-mode restrictions.
  }
}

function generateClientMessageId(): string {
  const rand = Math.random().toString(16).slice(2, 10);
  return `client-${Date.now()}-${rand}`;
}

function isNarrowViewport(): boolean {
  if (typeof window === "undefined") {
    return false;
  }
  if (typeof window.matchMedia === "function") {
    return window.matchMedia("(max-width: 1100px)").matches;
  }
  return window.innerWidth > 0 && window.innerWidth <= 1100;
}

function isActiveProjectRecord(project: ProjectRecord): boolean {
  const lc = project.lifecycle_state ?? "active";
  return lc !== "archived" && lc !== "deleted_record";
}

function computeOpenConflictsCount(ov: MemoryOverviewResponse | null): number {
  if (!ov || ov.memory_status !== "healthy") {
    return 0;
  }
  const fromStats = ov.stats?.open_conflicts_count ?? 0;
  const fromList = ov.conflicts.filter((c) => c.status === "open").length;
  return Math.max(fromStats, fromList);
}

function formatReconcileVerdict(
  effectStatus: string | undefined,
  summary: string | null | undefined,
): ReconcileNotice {
  const translated = formatDecisionReasonRu(summary);
  if (effectStatus === "confirmed") {
    return {
      tone: "ok",
      text:
        translated && /подтвердил|выполнен/i.test(translated)
          ? translated
          : "Сверка завершена: сервис подтвердил выполнение — повтор не отправлялся.",
    };
  }
  if (effectStatus === "failed") {
    return {
      tone: "warn",
      text:
        translated && /невыполнен|безопасен/i.test(translated)
          ? translated
          : "Сверка завершена: отмечено невыполненным — повтор безопасен.",
    };
  }
  return {
    tone: "neutral",
    text: translated || "Сверка внешнего эффекта завершена.",
  };
}

export interface WorkspaceState {
  initialized: boolean;
  isLoading: boolean;
  isSyncedAfterConnect: boolean;
  activeView: WorkspaceView;
  healthSnapshot: DesktopHealthSnapshot | null;
  runtimeCatalog: RuntimeCatalogResponse | null;
  isRefreshingCatalog: boolean;
  streamCursor: StreamCursor | null;
  projects: ProjectRecord[];
  agents: AgentRecord[];
  sessions: SessionRecord[];
  tasks: TaskRecord[];
  inboxCards: InboxCardRecord[];
  unresolvedInboxCount: number;
  pendingPermissions: PermissionRequestRecord[];
  activeGrants: CapabilityGrantRecord[];
  effects: EffectClaimRecord[];
  selectedProjectId: string | null;
  selectedSessionId: string | null;
  activeDetail: SessionDetailResponse | null;
  projectFiles: ListProjectFilesResponse | null;
  isLoadingFiles: boolean;
  activeScopeNoteCount: number | null;
  diagnostics: DiagnosticsResponse | null;
  draftsBySession: Record<string, string>;
  queuedMessages: QueuedOfflineMessage[];
  isFlushingQueue: boolean;
  optimisticMessagesBySession: Record<string, MessageRecord[]>;
  runningSessionIds: string[];
  cancellingSessionIds: string[];
  stoppingTaskIds: string[];
  cancelledNotice: string | null;
  reconcileNotice: ReconcileNotice | null;
  errorBanner: BridgeErrorPayload | null;
  isRightPanelOpen: boolean;
  isSidebarOpen: boolean;
  isSettingsOpen: boolean;
  isStatsOpen: boolean;
  isCommandOpen: boolean;
  commandInitialQuery: string;
  folderPickerIntent: FolderPickerIntent | null;
  searchQuery: string;
  selectedArtifactId: string | null;
  artifactRevisions: ArtifactRecord[];
  isFlushingOutbox: boolean;
  lastFlushSummary: string | null;
  isUpdatingModel: boolean;
  // Phase D3 state
  memoryOverview: MemoryOverviewResponse | null;
  isLoadingMemoryOverview: boolean;
  providerConnections: ProviderConnectionRecord[];
  plannedProviders: PlannedProviderInfo[];
  secrets: SecretMetadataRecord[];
  capabilities: CapabilityRecord[];
  isLoadingD3Settings: boolean;
  isRebuildingIndex: boolean;
  isConsolidatingMemory: boolean;
}

export interface CreateArtifactInput {
  name: string;
  mimeType: string;
  content: string;
  completenessStatus?: ArtifactCompleteness;
}

export interface WorkspaceActions {
  setActiveView(view: WorkspaceView): void;
  selectProject(projectId: string): void;
  openSessionInChat(sessionId: string): Promise<void>;
  refreshAll(): Promise<void>;
  pollEventsNow(): Promise<void>;
  reconnectWithUrl(headUrl: string): Promise<void>;
  refreshRuntimeCatalog(refresh?: boolean): Promise<void>;
  selectSession(sessionId: string): Promise<void>;
  createBlankChat(projectId?: string): Promise<SessionRecord | null>;
  createBlankChatWithDraft(projectId?: string, initialDraft?: string): Promise<SessionRecord | null>;
  createStandaloneChat(): Promise<SessionRecord | null>;
  toggleProjectPin(projectId: string): Promise<void>;
  toggleSessionPin(sessionId: string): Promise<void>;
  createFolderlessProject(): Promise<ProjectRecord | null>;
  createProjectFromFolder(folderPath: string): Promise<ProjectRecord | null>;
  requestFolderPicker(intent: FolderPickerIntent): Promise<void>;
  closeFolderPicker(): void;
  submitFolderPickerPath(folderPath: string): Promise<void>;
  renameProject(projectId: string, name: string): Promise<void>;
  attachProjectFolder(projectId: string, folderPath: string): Promise<void>;
  rebindProjectFolder(projectId: string, folderPath: string): Promise<void>;
  detachProjectWorkspace(projectId: string): Promise<void>;
  archiveProject(projectId: string): Promise<void>;
  unarchiveProject(projectId: string): Promise<void>;
  removeProject(projectId: string): Promise<void>;
  deleteProject(projectId: string): Promise<void>;
  renameSession(sessionId: string, title: string): Promise<void>;
  refreshProjectFiles(): Promise<void>;
  refreshMemoryStats(): Promise<void>;
  updateDraft(sessionId: string, value: string): void;
  sendMessage(): Promise<void>;
  queueOfflineMessage(sessionId: string, content: string): void;
  cancelQueuedMessage(clientMessageId: string): void;
  flushQueuedMessages(): Promise<void>;
  stopActiveRun(): Promise<void>;
  cancelSessionRun(sessionId: string): Promise<void>;
  // Phase D3+ live follow-up queue & steering actions
  enqueueFollowup(content: string): Promise<SessionFollowupRecord | null>;
  updateFollowup(followupId: string, content: string): Promise<SessionFollowupRecord | null>;
  deleteFollowup(followupId: string): Promise<boolean>;
  clearFollowups(): Promise<boolean>;
  resumeFollowups(): Promise<boolean>;
  steerSession(content: string): Promise<boolean>;
  updateActiveAgentModel(
    modelPreference: string | null,
    thinkingLevel: ThinkingLevel | null,
  ): Promise<void>;
  updateActiveAgentWritePolicy(policy: WorkspaceWritePolicy): Promise<void>;
  createArtifact(input: CreateArtifactInput): Promise<ArtifactRecord | null>;
  selectArtifact(artifactId: string): Promise<void>;
  applyArtifactLifecycle(
    sessionId: string,
    artifactId: string,
    action: ArtifactLifecycleAction,
  ): Promise<void>;
  createTask(request: CreateTaskRequest): Promise<TaskRecord | null>;
  updateTaskGoal(taskId: string, goal: string, title?: string): Promise<TaskRecord | null>;
  cancelTask(taskId: string): Promise<CancelTaskResponse | null>;
  sendTaskSignal(
    taskId: string,
    signalName: string,
    payloadText: string,
    idempotencyKey?: string,
  ): Promise<TaskSignalResponse | null>;
  replyToTask(taskId: string, answerText: string): Promise<TaskRecord | null>;
  resolvePermissionRequest(
    requestId: string,
    decision: PermissionResolutionDecision,
    expectedRevision?: number | null,
  ): Promise<void>;
  resolveInboxCard(
    cardId: string,
    expectedRevision: number,
    resolution: InboxResolutionAction,
  ): Promise<void>;
  revokeCapabilityGrant(grantId: string, reason?: string): Promise<void>;
  reconcileEffect(
    effectId: string,
    manualResolution?: ManualEffectResolution | null,
    reason?: string,
  ): Promise<void>;
  flushMemoryOutbox(): Promise<void>;
  // Phase D3 actions
  refreshMemoryOverview(scope?: string | null): Promise<MemoryOverviewResponse | null>;
  refreshD3SettingsData(): Promise<void>;
  setSecret(
    slotKey: string,
    value: string,
    description?: string,
  ): Promise<SecretMetadataRecord | null>;
  deleteSecret(slotKey: string): Promise<SecretMetadataRecord | null>;
  createProviderConnection(
    request: UpsertProviderConnectionRequest,
  ): Promise<ProviderConnectionRecord | null>;
  updateProviderConnection(
    connectionId: string,
    request: UpsertProviderConnectionRequest,
  ): Promise<ProviderConnectionRecord | null>;
  deleteProviderConnection(connectionId: string): Promise<boolean>;
  probeProviderConnection(connectionId: string): Promise<ProbeConnectionResponse | null>;
  activateProviderConnection(
    connectionId: string,
    role?: ConnectionActivationRole,
  ): Promise<ProviderConnectionRecord | null>;
  updateActiveAgentConnection(
    connectionId: string | null,
    modelPreference: string | null,
    thinkingLevel: ThinkingLevel | null,
  ): Promise<void>;
  discoverCapabilities(
    projectId?: string | null,
    scanPersonalGeminiReadonly?: boolean,
  ): Promise<DiscoverCapabilitiesResponse | null>;
  createCapability(request: CreateCapabilityRequest): Promise<CapabilityRecord | null>;
  updateCapability(
    capabilityId: string,
    request: UpdateCapabilityRequest,
  ): Promise<CapabilityRecord | null>;
  rollbackCapability(capabilityId: string): Promise<CapabilityRecord | null>;
  deleteCapability(capabilityId: string): Promise<boolean>;
  setProjectCapabilityBinding(
    projectId: string,
    capabilityId: string,
    activationState: CapabilityActivation,
  ): Promise<void>;
  triggerMemoryIndexRebuild(scope: string): Promise<TriggerIndexRebuildResponse | null>;
  createMemoryConsolidationJob(
    scope: string,
    jobType?: ConsolidationJobType,
  ): Promise<ConsolidationJob | null>;
  toggleRightPanel(): void;
  setRightPanelOpen(open: boolean): void;
  toggleSidebar(): void;
  setSidebarOpen(open: boolean): void;
  setSettingsOpen(open: boolean): void;
  setStatsOpen(open: boolean): void;
  setCommandOpen(open: boolean, initialQuery?: string): void;
  setSearchQuery(query: string): void;
  dismissError(): void;
  dismissCancelledNotice(): void;
  dismissReconcileNotice(): void;
}

export interface WorkspaceMeta {
  bridge: DesktopBridge;
  windowControls: WindowControls;
  searchInputRef: RefObject<HTMLInputElement | null>;
  composerInputRef: RefObject<HTMLTextAreaElement | null>;
  activeProject: ProjectRecord | null;
  activeSession: SessionRecord | null;
  activeAgent: AgentRecord | null;
  standaloneSessions: SessionRecord[];
  isStandaloneActiveSession: boolean;
  selectedArtifact: ArtifactRecord | null;
  currentDraft: string;
  queuedMessagesForActiveSession: QueuedOfflineMessage[];
  optimisticMessagesForActiveSession: MessageRecord[];
  isRunningActiveSession: boolean;
  isAwaitingApprovalActiveSession: boolean;
  isCancellingActiveSession: boolean;
  isSidebarEffectiveOpen: boolean;
  isActiveProjectArchived: boolean;
  activeClarificationCard: InboxCardRecord | null;
  activeTaskCount: number;
  openMemoryConflictsCount: number;
  activeLiveRun: LiveRunSnapshot | null;
  activeFollowups: SessionFollowupRecord[];
}

export interface WorkspaceContextValue {
  state: WorkspaceState;
  actions: WorkspaceActions;
  meta: WorkspaceMeta;
}

const WorkspaceContext = createContext<WorkspaceContextValue | null>(null);

export function useWorkspace(): WorkspaceContextValue {
  const ctx = use(WorkspaceContext);
  if (!ctx) {
    throw new Error("useWorkspace must be used inside WorkspaceProvider");
  }
  return ctx;
}

export interface WorkspaceProviderProps {
  bridge: DesktopBridge;
  windowControls: WindowControls;
  initialRightPanelOpen?: boolean;
  children: ReactNode;
}

const DEFAULT_SYSTEM_PROMPT =
  "Ты агент Dennett. Помогай владельцу вести проект, опираясь на проверяемые факты и память Dennett.";
const STANDALONE_MEMORY_SCOPE = "chat:standalone";
const STANDALONE_SYSTEM_PROMPT =
  "Ты агент Dennett. Помогай владельцу в свободном чате без привязки к папке проекта, опираясь на проверяемые факты и память Dennett.";

function extractFolderBasename(rawPath: string): string {
  const cleaned = rawPath.trim().replace(/[\\/]+$/, "");
  if (!cleaned) {
    return "Новый проект";
  }
  const segments = cleaned.split(/[\\/]/).filter(Boolean);
  const last = segments[segments.length - 1];
  return last && !last.endsWith(":") ? last.slice(0, 120) : "Новый проект";
}

function nextUntitledProjectName(existing: ProjectRecord[]): string {
  const base = "Новый проект";
  const taken = new Set(existing.map((p) => p.name.trim().toLowerCase()));
  if (!taken.has(base.toLowerCase())) {
    return base;
  }
  let idx = 2;
  while (taken.has(`${base} ${idx}`.toLowerCase())) {
    idx += 1;
  }
  return `${base} ${idx}`;
}

export function WorkspaceProvider({
  bridge,
  windowControls,
  initialRightPanelOpen = false,
  children,
}: WorkspaceProviderProps) {
  const initialPersisted = useMemo(() => readPersistedDrafts(), []);

  const [initialized, setInitialized] = useState(false);
  const [isLoading, setIsLoading] = useState(true);
  const [isSyncedAfterConnect, setIsSyncedAfterConnect] = useState(false);
  const [activeView, setActiveViewState] = useState<WorkspaceView>("chat");
  const [healthSnapshot, setHealthSnapshot] = useState<DesktopHealthSnapshot | null>(null);
  const [runtimeCatalog, setRuntimeCatalog] = useState<RuntimeCatalogResponse | null>(null);
  const [isRefreshingCatalog, setIsRefreshingCatalog] = useState(false);
  const [streamCursor, setStreamCursor] = useState<StreamCursor | null>(null);
  const [projects, setProjects] = useState<ProjectRecord[]>([]);
  const [agents, setAgents] = useState<AgentRecord[]>([]);
  const [sessions, setSessions] = useState<SessionRecord[]>([]);
  const [tasks, setTasks] = useState<TaskRecord[]>([]);
  const [inboxCards, setInboxCards] = useState<InboxCardRecord[]>([]);
  const [unresolvedInboxCount, setUnresolvedInboxCount] = useState(0);
  const [pendingPermissions, setPendingPermissions] = useState<PermissionRequestRecord[]>([]);
  const [activeGrants, setActiveGrants] = useState<CapabilityGrantRecord[]>([]);
  const [effects, setEffects] = useState<EffectClaimRecord[]>([]);
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null);
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);
  const [activeDetail, setActiveDetail] = useState<SessionDetailResponse | null>(null);
  const [projectFiles, setProjectFiles] = useState<ListProjectFilesResponse | null>(null);
  const [isLoadingFiles, setIsLoadingFiles] = useState(false);
  const [activeScopeNoteCount, setActiveScopeNoteCount] = useState<number | null>(null);
  const [diagnostics, setDiagnostics] = useState<DiagnosticsResponse | null>(null);
  const [draftsBySession, setDraftsBySession] = useState<Record<string, string>>(
    initialPersisted.draftsBySession,
  );
  const [queuedMessages, setQueuedMessages] = useState<QueuedOfflineMessage[]>(
    initialPersisted.queuedMessages,
  );
  const [isFlushingQueue, setIsFlushingQueue] = useState(false);
  const [optimisticMessagesBySession, setOptimisticMessagesBySession] = useState<
    Record<string, MessageRecord[]>
  >({});
  const [localInFlightCountsBySession, setLocalInFlightCountsBySession] = useState<
    Record<string, number>
  >({});
  const [serverRunningSessionIds, setServerRunningSessionIds] = useState<string[]>([]);
  const [liveRun, setLiveRun] = useState<LiveRunSnapshot | null>(null);
  const [activeFollowups, setActiveFollowups] = useState<SessionFollowupRecord[]>([]);
  const [cancellingSessionIds, setCancellingSessionIds] = useState<string[]>([]);
  const [stoppingTaskIds, setStoppingTaskIds] = useState<string[]>([]);
  const [cancelledNotice, setCancelledNotice] = useState<string | null>(null);
  const [reconcileNotice, setReconcileNotice] = useState<ReconcileNotice | null>(null);
  const [errorBanner, setErrorBanner] = useState<BridgeErrorPayload | null>(null);
  const [isRightPanelOpen, setIsRightPanelOpenState] = useState(initialRightPanelOpen);
  const [isSidebarOpen, setIsSidebarOpenState] = useState(true);
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [isStatsOpen, setIsStatsOpen] = useState(false);
  const [isCommandOpen, setIsCommandOpenState] = useState(false);
  const [commandInitialQuery, setCommandInitialQuery] = useState("");
  const [folderPickerIntent, setFolderPickerIntent] = useState<FolderPickerIntent | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedArtifactId, setSelectedArtifactId] = useState<string | null>(null);
  const [artifactRevisions, setArtifactRevisions] = useState<ArtifactRecord[]>([]);
  const [isFlushingOutbox, setIsFlushingOutbox] = useState(false);
  const [lastFlushSummary, setLastFlushSummary] = useState<string | null>(null);
  const [isUpdatingModel, setIsUpdatingModel] = useState(false);
  const [memoryOverview, setMemoryOverview] = useState<MemoryOverviewResponse | null>(null);
  const [globalOpenMemoryConflictsCount, setGlobalOpenMemoryConflictsCount] = useState(0);
  const [isLoadingMemoryOverview, setIsLoadingMemoryOverview] = useState(false);
  const [providerConnections, setProviderConnections] = useState<ProviderConnectionRecord[]>([]);
  const [plannedProviders, setPlannedProviders] = useState<PlannedProviderInfo[]>([]);
  const [secrets, setSecrets] = useState<SecretMetadataRecord[]>([]);
  const [capabilities, setCapabilities] = useState<CapabilityRecord[]>([]);
  const [isLoadingD3Settings, setIsLoadingD3Settings] = useState(false);
  const [isRebuildingIndex, setIsRebuildingIndex] = useState(false);
  const [isConsolidatingMemory, setIsConsolidatingMemory] = useState(false);
  const [isDocumentHidden, setIsDocumentHidden] = useState(
    () => typeof document !== "undefined" && document.visibilityState === "hidden",
  );

  const searchInputRef = useRef<HTMLInputElement | null>(null);
  const composerInputRef = useRef<HTMLTextAreaElement | null>(null);
  const selectedProjectRef = useRef<string | null>(null);
  const selectedSessionRef = useRef<string | null>(null);
  const selectedArtifactIdRef = useRef<string | null>(null);
  const projectsRef = useRef<ProjectRecord[]>([]);
  const agentsRef = useRef<AgentRecord[]>([]);
  const sessionsRef = useRef<SessionRecord[]>([]);
  const catalogRef = useRef<RuntimeCatalogResponse | null>(null);
  const streamCursorRef = useRef<StreamCursor | null>(null);
  const queuedMessagesRef = useRef<QueuedOfflineMessage[]>(initialPersisted.queuedMessages);
  const isFlushingQueueRef = useRef(false);
  const pollInFlightRef = useRef(false);
  const snapshotSeqRef = useRef(0);
  const snapshotInFlightRef = useRef<{
    promise: Promise<void>;
    reloadDetail: boolean;
    preferredSessionId?: string | null;
  } | null>(null);
  const inspectingArchivedProjectRef = useRef(false);
  const runningSessionIdsRef = useRef<Set<string>>(new Set());
  const cancellingSessionIdsRef = useRef<Set<string>>(new Set());
  const detailRequestSeqRef = useRef(0);
  const filesRequestSeqRef = useRef(0);
  const activeViewRef = useRef<WorkspaceView>("chat");
  const isSidebarOpenRef = useRef(true);
  const isRightPanelOpenRef = useRef(initialRightPanelOpen);
  const activeProjectScopeRef = useRef<string | null>(null);
  const livePollInFlightRef = useRef(false);
  const wasRunningActiveSessionRef = useRef(false);
  const activeSteerCountBySessionRef = useRef<Record<string, number>>({});
  const activeFollowupsRef = useRef<SessionFollowupRecord[]>([]);
  const localCompletedSessionRef = useRef<string | null>(null);

  const localInFlightSessionIds = useMemo(
    () =>
      Object.entries(localInFlightCountsBySession)
        .filter(([, count]) => count > 0)
        .map(([sid]) => sid),
    [localInFlightCountsBySession],
  );

  // D2-ARCH-01: canonical running sessions derived from server snapshot + activeDetail + local in-flight.
  const runningSessionIds = useMemo(() => {
    const set = new Set<string>(localInFlightSessionIds);
    for (const sid of serverRunningSessionIds) {
      set.add(sid);
    }
    if (activeDetail && activeDetail.runs.some((r) => r.status === "running")) {
      set.add(activeDetail.session.session_id);
    }
    return Array.from(set);
  }, [activeDetail, localInFlightSessionIds, serverRunningSessionIds]);

  const incrementLocalInFlight = useCallback((sessionId: string) => {
    runningSessionIdsRef.current.add(sessionId);
    setLocalInFlightCountsBySession((prev) => ({
      ...prev,
      [sessionId]: (prev[sessionId] ?? 0) + 1,
    }));
  }, []);

  const decrementLocalInFlight = useCallback((sessionId: string) => {
    setLocalInFlightCountsBySession((prev) => {
      const nextCount = (prev[sessionId] ?? 0) - 1;
      if (nextCount <= 0) {
        const next = { ...prev };
        delete next[sessionId];
        return next;
      }
      return { ...prev, [sessionId]: nextCount };
    });
  }, []);

  selectedProjectRef.current = selectedProjectId;
  selectedSessionRef.current = selectedSessionId;
  selectedArtifactIdRef.current = selectedArtifactId;
  projectsRef.current = projects;
  agentsRef.current = agents;
  sessionsRef.current = sessions;
  catalogRef.current = runtimeCatalog;
  queuedMessagesRef.current = queuedMessages;
  runningSessionIdsRef.current = new Set(runningSessionIds);
  cancellingSessionIdsRef.current = new Set(cancellingSessionIds);
  activeFollowupsRef.current = activeFollowups;
  activeViewRef.current = activeView;
  isSidebarOpenRef.current = isSidebarOpen;
  isRightPanelOpenRef.current = isRightPanelOpen;

  // Persist drafts and offline queue to localStorage cleanly.
  useEffect(() => {
    writePersistedDrafts({ draftsBySession, queuedMessages });
  }, [draftsBySession, queuedMessages]);

  // Track document visibility so background tabs poll calmly at 5000ms.
  useEffect(() => {
    if (typeof document === "undefined") {
      return;
    }
    const onVisibility = () => {
      setIsDocumentHidden(document.visibilityState === "hidden");
    };
    document.addEventListener("visibilitychange", onVisibility);
    return () => document.removeEventListener("visibilitychange", onVisibility);
  }, []);

  // D2-UX-02: Auto-collapse a side panel below 1100px so 960x620 never overflows or breaks Composer.
  useEffect(() => {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
      return;
    }
    const mql = window.matchMedia("(max-width: 1100px)");
    const enforceNarrowBudget = (narrow: boolean) => {
      if (
        narrow &&
        isSidebarOpenRef.current &&
        activeViewRef.current === "chat" &&
        isRightPanelOpenRef.current
      ) {
        setIsRightPanelOpenState(false);
      }
    };
    enforceNarrowBudget(mql.matches);
    const listener = (event: MediaQueryListEvent) => enforceNarrowBudget(event.matches);
    if (typeof mql.addEventListener === "function") {
      mql.addEventListener("change", listener);
      return () => mql.removeEventListener("change", listener);
    }
    return undefined;
  }, []);

  const focusComposerSoon = useCallback((placeCursorAtEnd = false) => {
    requestAnimationFrame(() => {
      const el = composerInputRef.current;
      if (!el) {
        return;
      }
      el.focus();
      if (placeCursorAtEnd) {
        const len = el.value.length;
        try {
          el.setSelectionRange(len, len);
        } catch {
          // Ignore environments without selection support.
        }
      }
    });
  }, []);

  const setSelectedSessionSync = useCallback((nextSessionId: string | null) => {
    if (selectedSessionRef.current !== nextSessionId) {
      setLiveRun(null);
      setActiveFollowups([]);
      wasRunningActiveSessionRef.current = false;
      localCompletedSessionRef.current = null;
    }
    selectedSessionRef.current = nextSessionId;
    setSelectedSessionId(nextSessionId);
  }, []);

  const setSelectedProjectSync = useCallback((nextProjectId: string | null) => {
    selectedProjectRef.current = nextProjectId;
    setSelectedProjectId(nextProjectId);
  }, []);

  const setActiveView = useCallback((nextView: WorkspaceView) => {
    setActiveViewState(nextView);
    // D2-UX-05: clear stale cancelledNotice when navigating between top-level views.
    setCancelledNotice(null);
  }, []);

  const setSidebarOpen = useCallback((open: boolean) => {
    setIsSidebarOpenState(open);
    if (open && activeViewRef.current === "chat" && isNarrowViewport()) {
      setIsRightPanelOpenState(false);
    }
  }, []);

  const toggleSidebar = useCallback(() => {
    if (activeViewRef.current !== "chat") {
      setActiveViewState("chat");
      setCancelledNotice(null);
      setIsSidebarOpenState(true);
      if (isNarrowViewport()) {
        setIsRightPanelOpenState(false);
      }
      return;
    }
    setIsSidebarOpenState((prev) => {
      const next = !prev;
      if (next && isNarrowViewport()) {
        setIsRightPanelOpenState(false);
      }
      return next;
    });
  }, []);

  const setRightPanelOpen = useCallback((open: boolean) => {
    setIsRightPanelOpenState(open);
    if (open && activeViewRef.current === "chat" && isNarrowViewport()) {
      setIsSidebarOpenState(false);
    }
  }, []);

  const toggleRightPanel = useCallback(() => {
    setIsRightPanelOpenState((prev) => {
      const next = !prev;
      if (next && activeViewRef.current === "chat" && isNarrowViewport()) {
        setIsSidebarOpenState(false);
      }
      return next;
    });
  }, []);

  const setCommandOpen = useCallback((open: boolean, initialQuery?: string) => {
    if (open) {
      setCommandInitialQuery(initialQuery ?? "");
    } else {
      setCommandInitialQuery("");
    }
    setIsCommandOpenState(open);
  }, []);

  const loadFilesForProject = useCallback(
    async (projectId: string | null) => {
      const seq = ++filesRequestSeqRef.current;
      if (!projectId) {
        setProjectFiles(null);
        setIsLoadingFiles(false);
        return;
      }
      setIsLoadingFiles(true);
      try {
        const resp = await bridge.listProjectFiles(projectId);
        if (seq === filesRequestSeqRef.current) {
          setProjectFiles(resp);
        }
      } catch {
        if (seq === filesRequestSeqRef.current) {
          setProjectFiles(null);
        }
      } finally {
        if (seq === filesRequestSeqRef.current) {
          setIsLoadingFiles(false);
        }
      }
    },
    [bridge],
  );

  const loadRevisionsForArtifact = useCallback(
    async (sessionId: string, artifactName: string) => {
      try {
        const resp = await bridge.listArtifactRevisions(sessionId, artifactName);
        if (selectedSessionRef.current === sessionId) {
          setArtifactRevisions(
            resp.artifacts
              .filter((a) => (a.lifecycle_state ?? "active") !== "removed")
              .sort((a, b) => a.revision - b.revision),
          );
        }
      } catch {
        // Fallback to revisions already present in activeDetail.
      }
    },
    [bridge],
  );

  const loadSessionDetail = useCallback(
    async (sessionId: string) => {
      const seq = ++detailRequestSeqRef.current;
      try {
        const detail = await bridge.getSessionDetail(sessionId);
        setSessions((prev) => {
          const next = sortSessionsByPin(
            prev.map((s) => (s.session_id === detail.session.session_id ? detail.session : s)),
          );
          sessionsRef.current = next;
          return next;
        });
        if (seq !== detailRequestSeqRef.current || selectedSessionRef.current !== sessionId) {
          return;
        }
        setActiveDetail(detail);
        setActiveFollowups(
          (detail.followups ?? [])
            .filter((f) => f.status === "queued" || f.status === "paused")
            .sort((a, b) => a.position - b.position),
        );
        const visibleArtifacts = detail.artifacts.filter((a) => {
          const lc = a.lifecycle_state ?? "active";
          return lc !== "removed";
        });
        const activeOnlyArtifacts = visibleArtifacts.filter(
          (a) => (a.lifecycle_state ?? "active") === "active",
        );
        const currentSelectedId = selectedArtifactIdRef.current;
        const stillVisible = currentSelectedId
          ? (visibleArtifacts.find((a) => a.artifact_id === currentSelectedId) ?? null)
          : null;
        const chosenArtifact =
          stillVisible ??
          activeOnlyArtifacts[activeOnlyArtifacts.length - 1] ??
          null;
        const nextArtifactId = chosenArtifact?.artifact_id ?? null;
        selectedArtifactIdRef.current = nextArtifactId;
        setSelectedArtifactId(nextArtifactId);
        if (chosenArtifact) {
          setArtifactRevisions(
            visibleArtifacts
              .filter((a) => a.name === chosenArtifact.name)
              .sort((a, b) => a.revision - b.revision),
          );
        } else {
          setArtifactRevisions([]);
        }
      } catch (err) {
        if (seq === detailRequestSeqRef.current && selectedSessionRef.current === sessionId) {
          setErrorBanner(normalizeBridgeError(err));
        }
      }
    },
    [bridge],
  );

  const refreshRuntimeCatalog = useCallback(
    async (refresh = true) => {
      setIsRefreshingCatalog(true);
      try {
        const cat = await bridge.getRuntimeCatalog(refresh);
        catalogRef.current = cat;
        setRuntimeCatalog(cat);
      } catch (err) {
        if (refresh) {
          setErrorBanner(normalizeBridgeError(err));
        }
      } finally {
        setIsRefreshingCatalog(false);
      }
    },
    [bridge],
  );

  // D2-OVER-03: Single-flight coalesced snapshot fetch with monotonic snapshotSeqRef!
  const applySnapshotCollections = useCallback(
    async (preferredSessionId?: string | null, reloadActiveDetail = false) => {
      const existing = snapshotInFlightRef.current;
      if (
        existing &&
        preferredSessionId === undefined &&
        (!reloadActiveDetail || existing.reloadDetail)
      ) {
        return existing.promise;
      }

      const mySeq = ++snapshotSeqRef.current;
      const run = (async () => {
        const [snap, permResp, effResp] = await Promise.all([
          bridge.getStateSnapshot(null, true),
          bridge.listPermissionRequests("pending", null).catch(() => ({ requests: [] })),
          bridge.listEffects(null, null, null).catch(() => ({ effects: [] })),
        ]);

        // Reject stale responses superseded by a newer snapshot call.
        if (mySeq !== snapshotSeqRef.current) {
          return;
        }

        const sortedProjects = sortProjectsByPin(snap.projects);
        const sortedSessions = sortSessionsByPin(snap.sessions);

        streamCursorRef.current = snap.snapshot_cursor;
        setStreamCursor(snap.snapshot_cursor);
        projectsRef.current = sortedProjects;
        agentsRef.current = snap.agents;
        sessionsRef.current = sortedSessions;

        const nonDeletedProjects = sortedProjects.filter(
          (p) => (p.lifecycle_state ?? "active") !== "deleted_record",
        );
        const activeProjectIds = new Set(
          nonDeletedProjects.filter(isActiveProjectRecord).map((p) => p.project_id),
        );
        const visibleActiveSessions = sortedSessions.filter(
          (s) => s.project_id === null || activeProjectIds.has(s.project_id),
        );
        const visibleUserChats = visibleActiveSessions.filter(
          (s) => (s.session_kind ?? "user_chat") === "user_chat",
        );

        setProjects(sortedProjects);
        setAgents(snap.agents);
        setSessions(sortedSessions);
        setTasks(snap.tasks);
        setInboxCards(snap.inbox_cards);
        setUnresolvedInboxCount(
          snap.inbox_cards.filter(
            (c) => c.status === "open" && activeProjectIds.has(c.project_id),
          ).length,
        );
        setActiveGrants(snap.active_grants);
        setPendingPermissions(permResp.requests);
        setEffects(effResp.effects);
        setServerRunningSessionIds(snap.running_session_ids ?? []);

        // Determine selected project/session respecting archived read-only inspection only when explicitly chosen.
        const currentProj = selectedProjectRef.current
          ? nonDeletedProjects.find((p) => p.project_id === selectedProjectRef.current) ?? null
          : null;
        const keepArchivedInspection =
          inspectingArchivedProjectRef.current &&
          currentProj !== null &&
          (currentProj.lifecycle_state ?? "active") === "archived";

        let targetSessionId: string | null = null;
        if (preferredSessionId !== undefined) {
          targetSessionId = preferredSessionId;
        } else if (keepArchivedInspection && currentProj) {
          const archivedSessions = sortedSessions.filter(
            (s) => s.project_id === currentProj.project_id,
          );
          targetSessionId =
            selectedSessionRef.current &&
            archivedSessions.some((s) => s.session_id === selectedSessionRef.current)
              ? selectedSessionRef.current
              : (archivedSessions[0]?.session_id ?? null);
        } else if (
          selectedSessionRef.current &&
          visibleActiveSessions.some((s) => s.session_id === selectedSessionRef.current)
        ) {
          targetSessionId = selectedSessionRef.current;
        } else if (
          currentProj &&
          activeProjectIds.has(currentProj.project_id) &&
          selectedSessionRef.current === null
        ) {
          // User explicitly selected an active project that has no sessions yet.
          const projChats = visibleActiveSessions.filter(
            (s) => s.project_id === currentProj.project_id,
          );
          targetSessionId = projChats[0]?.session_id ?? null;
        } else {
          targetSessionId =
            visibleUserChats[0]?.session_id ?? visibleActiveSessions[0]?.session_id ?? null;
        }

        const resolvedSession = targetSessionId
          ? sortedSessions.find((s) => s.session_id === targetSessionId) ?? null
          : null;
        const nextProjectId = resolvedSession
          ? resolvedSession.project_id
          : ((keepArchivedInspection && currentProj ? currentProj.project_id : null) ??
            (currentProj && activeProjectIds.has(currentProj.project_id)
              ? currentProj.project_id
              : null) ??
            nonDeletedProjects.find(isActiveProjectRecord)?.project_id ??
            null);

        const changedSession = targetSessionId !== selectedSessionRef.current;
        setSelectedProjectSync(nextProjectId);
        setSelectedSessionSync(targetSessionId);
        if (targetSessionId && (reloadActiveDetail || changedSession)) {
          await loadSessionDetail(targetSessionId);
        } else if (!targetSessionId) {
          setActiveDetail(null);
          setLiveRun(null);
          setActiveFollowups([]);
          selectedArtifactIdRef.current = null;
          setSelectedArtifactId(null);
          setArtifactRevisions([]);
        }
      })();

      const entry = { promise: run, reloadDetail: reloadActiveDetail, preferredSessionId };
      snapshotInFlightRef.current = entry;
      try {
        await run;
      } finally {
        if (snapshotInFlightRef.current === entry) {
          snapshotInFlightRef.current = null;
        }
      }
    },
    [bridge, loadSessionDetail, setSelectedProjectSync, setSelectedSessionSync],
  );

  const loadConnectedState = useCallback(
    async (preferredSessionId?: string | null) => {
      await applySnapshotCollections(preferredSessionId, true);
      setIsSyncedAfterConnect(true);
      void bridge
        .listDiagnostics(20)
        .then((diagResp) => {
          setDiagnostics(diagResp);
        })
        .catch(() => {});
      void bridge
        .getRuntimeCatalog(false)
        .then((catResp) => {
          catalogRef.current = catResp;
          setRuntimeCatalog(catResp);
        })
        .catch(() => {});
      void bridge
        .getMemoryOverview(null)
        .then((ov) => {
          setMemoryOverview(ov);
          setGlobalOpenMemoryConflictsCount(computeOpenConflictsCount(ov));
        })
        .catch(() => {});
      void bridge
        .listProviderConnections()
        .then((resp) => {
          setProviderConnections(resp.connections);
          setPlannedProviders(resp.planned_providers);
        })
        .catch(() => {});
    },
    [applySnapshotCollections, bridge],
  );

  const refreshAll = useCallback(async () => {
    setIsLoading(true);
    try {
      const snap = await bridge.getHeadHealth();
      setHealthSnapshot(snap);
      if (snap.connected) {
        setErrorBanner(null);
        await loadConnectedState();
      } else {
        setIsSyncedAfterConnect(false);
        if (sessionsRef.current.length === 0) {
          setActiveDetail(null);
        }
      }
    } catch (err) {
      setErrorBanner(normalizeBridgeError(err));
    } finally {
      setInitialized(true);
      setIsLoading(false);
    }
  }, [bridge, loadConnectedState]);

  useEffect(() => {
    void refreshAll();
  }, [refreshAll]);

  const activeSession = useMemo(
    () => sessions.find((s) => s.session_id === selectedSessionId) ?? activeDetail?.session ?? null,
    [activeDetail?.session, selectedSessionId, sessions],
  );

  const activeProject = useMemo(() => {
    if (activeSession && activeSession.project_id === null) {
      return null;
    }
    if (selectedProjectId) {
      const explicit = projects.find(
        (p) =>
          p.project_id === selectedProjectId &&
          (p.lifecycle_state ?? "active") !== "deleted_record",
      );
      if (explicit) {
        return explicit;
      }
    }
    if (activeSession && activeSession.project_id !== null) {
      const matched = projects.find(
        (p) =>
          p.project_id === activeSession.project_id &&
          (p.lifecycle_state ?? "active") !== "deleted_record",
      );
      if (matched) {
        return matched;
      }
    }
    return projects.find(isActiveProjectRecord) ?? null;
  }, [activeSession, projects, selectedProjectId]);

  const activeAgent = useMemo(() => {
    if (activeSession) {
      return agents.find((a) => a.agent_id === activeSession.agent_id) ?? null;
    }
    if (activeProject) {
      const scoped = agents.find(
        (a) =>
          a.current_config.allowed_memory_scopes.length === 1 &&
          a.current_config.allowed_memory_scopes[0] === activeProject.memory_scope,
      );
      if (scoped) {
        return scoped;
      }
    }
    return agents[0] ?? null;
  }, [activeProject, activeSession, agents]);

  // D2-OVER-03: Cursor advances strictly AFTER snapshot/detail commit; zero redundant permission polls when sawEvents===false.
  const pollEventsNow = useCallback(async () => {
    if (pollInFlightRef.current || !streamCursorRef.current) {
      return;
    }
    pollInFlightRef.current = true;
    try {
      const startCursor = streamCursorRef.current;
      let nextCursor = startCursor;
      let sawEvents = false;
      let shouldReloadSessionDetail = false;
      let shouldRefreshFiles = false;
      const activeSid = selectedSessionRef.current;

      for (let page = 0; page < 5; page += 1) {
        const resp = await bridge.getEvents(
          nextCursor.committed_seq,
          nextCursor.authority_epoch,
          null,
          100,
        );
        if (resp.next_cursor > nextCursor.committed_seq) {
          nextCursor = {
            authority_epoch: resp.authority_epoch,
            committed_seq: resp.next_cursor,
            retained_from_seq: resp.retained_from_seq,
          };
        }
        if (resp.events.length > 0) {
          sawEvents = true;
          for (const ev of resp.events) {
            if (
              (activeSid && ev.session_id === activeSid) ||
              ev.event_type.startsWith("permission.") ||
              ev.event_type.startsWith("artifact.") ||
              ev.event_type.startsWith("effect.") ||
              ev.event_type.startsWith("run.") ||
              ev.event_type.startsWith("inbox.")
            ) {
              shouldReloadSessionDetail = true;
            }
            if (
              ev.event_type === "permission.resolved" ||
              ev.event_type === "session.message_completed"
            ) {
              shouldRefreshFiles = true;
            }
          }
        }
        if (!resp.has_more) {
          break;
        }
      }

      if (sawEvents) {
        await applySnapshotCollections(undefined, shouldReloadSessionDetail);
        // Only advance cursor past snapshot if the event scan moved even further and snapshot succeeded.
        if (
          streamCursorRef.current &&
          streamCursorRef.current.authority_epoch === nextCursor.authority_epoch &&
          nextCursor.committed_seq > streamCursorRef.current.committed_seq
        ) {
          streamCursorRef.current = nextCursor;
          setStreamCursor(nextCursor);
        }
        if (shouldRefreshFiles && activeProject && isRightPanelOpen) {
          void loadFilesForProject(activeProject.project_id);
        }
      } else if (nextCursor.committed_seq > startCursor.committed_seq) {
        streamCursorRef.current = nextCursor;
        setStreamCursor(nextCursor);
      }
    } catch (err) {
      const normalized = normalizeBridgeError(err);
      if (isStreamResyncConflict(normalized)) {
        // V18: clean automatic snapshot resync on epoch bump or retention advance!
        try {
          await applySnapshotCollections(undefined, true);
        } catch {
          // Will retry on next tick.
        }
      } else if (normalized.code === "HEAD_UNREACHABLE") {
        setIsSyncedAfterConnect(false);
        setHealthSnapshot((prev) =>
          prev
            ? {
                ...prev,
                connected: false,
                error_code: normalized.code,
                error: normalized.message,
              }
            : {
                head_url: "http://127.0.0.1:41771",
                connected: false,
                health: null,
                error_code: normalized.code,
                error: normalized.message,
              },
        );
      }
    } finally {
      pollInFlightRef.current = false;
    }
  }, [activeProject, applySnapshotCollections, bridge, isRightPanelOpen, loadFilesForProject]);

  const activeTaskCount = useMemo(() => {
    const activeProjectIds = new Set(
      projects.filter(isActiveProjectRecord).map((p) => p.project_id),
    );
    return tasks.filter(
      (t) =>
        activeProjectIds.has(t.project_id) &&
        [
          "queued",
          "running",
          "waiting_timer",
          "waiting_signal",
          "waiting_clarification",
          "waiting_approval",
          "unknown_external",
        ].includes(t.status),
    ).length;
  }, [projects, tasks]);

  // D2-OVER-03: Calmer adaptive cursor sync (1000ms active, 2500ms idle, 5000ms hidden tab).
  useEffect(() => {
    if (!healthSnapshot?.connected || !streamCursor) {
      return;
    }
    const hasActiveWork =
      runningSessionIds.length > 0 ||
      activeTaskCount > 0 ||
      pendingPermissions.length > 0;
    const intervalMs = isDocumentHidden ? 5000 : hasActiveWork ? 1000 : 2500;
    const handle = window.setInterval(() => {
      void pollEventsNow();
    }, intervalMs);
    return () => window.clearInterval(handle);
  }, [
    activeTaskCount,
    healthSnapshot?.connected,
    isDocumentHidden,
    pendingPermissions.length,
    pollEventsNow,
    runningSessionIds.length,
    streamCursor,
  ]);

  // D2-OVER-03: Automatic reconnect check while Head is offline (including cold-start when sessions.length === 0!).
  useEffect(() => {
    if (!initialized || healthSnapshot?.connected) {
      return;
    }
    const handle = window.setInterval(() => {
      void (async () => {
        const snap = await bridge.getHeadHealth();
        if (snap.connected) {
          setHealthSnapshot(snap);
          setErrorBanner(null);
          await loadConnectedState();
        }
      })();
    }, 3000);
    return () => window.clearInterval(handle);
  }, [bridge, healthSnapshot?.connected, initialized, loadConnectedState]);

  // Keep right-panel workspace files in sync when active project changes or panel opens.
  useEffect(() => {
    if (!healthSnapshot?.connected || !activeProject) {
      setProjectFiles(null);
      return;
    }
    if (isRightPanelOpen) {
      void loadFilesForProject(activeProject.project_id);
    }
  }, [activeProject, healthSnapshot?.connected, isRightPanelOpen, loadFilesForProject]);

  const refreshProjectFiles = useCallback(async () => {
    if (!activeProject) {
      return;
    }
    await loadFilesForProject(activeProject.project_id);
  }, [activeProject, loadFilesForProject]);

  activeProjectScopeRef.current = activeProject?.memory_scope ?? null;

  const refreshMemoryOverview = useCallback(
    async (scope?: string | null): Promise<MemoryOverviewResponse | null> => {
      if (!healthSnapshot?.connected) {
        return null;
      }
      const normalizedScope = scope ?? null;
      setIsLoadingMemoryOverview(true);
      try {
        const overview = await bridge.getMemoryOverview(normalizedScope);
        if (normalizedScope === null) {
          setMemoryOverview(overview);
          setGlobalOpenMemoryConflictsCount(computeOpenConflictsCount(overview));
        }
        if (overview.memory_status === "healthy" && overview.stats) {
          const currentProjectScope = activeProjectScopeRef.current;
          if (
            currentProjectScope &&
            (!normalizedScope || normalizedScope === currentProjectScope)
          ) {
            const scopedNotes = overview.notes.filter((n) => n.scope === currentProjectScope);
            setActiveScopeNoteCount(scopedNotes.length);
          }
        }
        return overview;
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
        return null;
      } finally {
        setIsLoadingMemoryOverview(false);
      }
    },
    [bridge, healthSnapshot?.connected],
  );

  const refreshD3SettingsData = useCallback(async () => {
    if (!healthSnapshot?.connected) {
      return;
    }
    setIsLoadingD3Settings(true);
    try {
      const [secResp, connResp, capResp] = await Promise.all([
        bridge.listSecrets().catch(() => null),
        bridge.listProviderConnections().catch(() => null),
        bridge.listCapabilities(null, null, null).catch(() => null),
      ]);
      if (secResp) {
        setSecrets(secResp.secrets);
      }
      if (connResp) {
        setProviderConnections(connResp.connections);
        setPlannedProviders(connResp.planned_providers);
      }
      if (capResp) {
        setCapabilities(capResp.capabilities);
      }
    } finally {
      setIsLoadingD3Settings(false);
    }
  }, [bridge, healthSnapshot?.connected]);

  useEffect(() => {
    if (isSettingsOpen && healthSnapshot?.connected) {
      void refreshD3SettingsData();
    }
  }, [healthSnapshot?.connected, isSettingsOpen, refreshD3SettingsData]);

  const refreshMemoryStats = useCallback(async () => {
    if (!healthSnapshot?.connected) {
      setActiveScopeNoteCount(null);
      return;
    }
    try {
      const targetScope = activeProjectScopeRef.current;
      const [searchResp, overviewResp] = await Promise.all([
        targetScope
          ? bridge.searchMemory({ scope: targetScope, limit: 100 }).catch(() => null)
          : Promise.resolve(null),
        bridge.getMemoryOverview(null).catch(() => null),
      ]);
      if (overviewResp) {
        setMemoryOverview(overviewResp);
        setGlobalOpenMemoryConflictsCount(computeOpenConflictsCount(overviewResp));
      }
      if (searchResp && searchResp.memory_status === "healthy") {
        setActiveScopeNoteCount(searchResp.notes.length);
      } else if (overviewResp && overviewResp.memory_status === "healthy" && overviewResp.stats) {
        setActiveScopeNoteCount(
          targetScope
            ? overviewResp.notes.filter((n) => n.scope === targetScope).length
            : overviewResp.stats.active_notes_count,
        );
      } else {
        setActiveScopeNoteCount(null);
      }
    } catch {
      setActiveScopeNoteCount(null);
    }
  }, [bridge, healthSnapshot?.connected]);

  useEffect(() => {
    if (isStatsOpen) {
      void refreshMemoryStats();
    }
  }, [isStatsOpen, refreshMemoryStats]);

  const reconnectWithUrl = useCallback(
    async (headUrl: string) => {
      setIsLoading(true);
      setErrorBanner(null);
      try {
        const snap = await bridge.setHeadUrl(headUrl);
        setHealthSnapshot(snap);
        if (snap.connected) {
          await loadConnectedState();
        } else {
          setIsSyncedAfterConnect(false);
        }
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
      } finally {
        setIsLoading(false);
      }
    },
    [bridge, loadConnectedState],
  );

  // D2-UX-03: Independent project selection (never forces view switch; supports read-only archived inspection).
  const selectProject = useCallback(
    (projectId: string) => {
      const target = projectsRef.current.find((p) => p.project_id === projectId);
      if (!target || (target.lifecycle_state ?? "active") === "deleted_record") {
        return;
      }
      const isArchived = (target.lifecycle_state ?? "active") === "archived";
      inspectingArchivedProjectRef.current = isArchived;
      setSelectedProjectSync(projectId);

      const projectSessions = sessionsRef.current.filter((s) => s.project_id === projectId);
      const userChats = projectSessions.filter((s) => (s.session_kind ?? "user_chat") === "user_chat");
      const currentBelongsToProject =
        selectedSessionRef.current &&
        projectSessions.some((s) => s.session_id === selectedSessionRef.current);

      if (!currentBelongsToProject) {
        const nextSess = userChats[0] ?? projectSessions[0] ?? null;
        setSelectedSessionSync(nextSess?.session_id ?? null);
        if (nextSess) {
          void loadSessionDetail(nextSess.session_id);
        } else {
          setActiveDetail(null);
          setLiveRun(null);
          setActiveFollowups([]);
          selectedArtifactIdRef.current = null;
          setSelectedArtifactId(null);
          setArtifactRevisions([]);
        }
      }
    },
    [loadSessionDetail, setSelectedProjectSync, setSelectedSessionSync],
  );

  const selectSession = useCallback(
    async (sessionId: string) => {
      const targetSess = sessionsRef.current.find((s) => s.session_id === sessionId);
      if (targetSess) {
        if (targetSess.project_id === null) {
          inspectingArchivedProjectRef.current = false;
          setSelectedProjectSync(null);
          setProjectFiles(null);
        } else {
          const ownerProj = projectsRef.current.find(
            (p) => p.project_id === targetSess.project_id,
          );
          inspectingArchivedProjectRef.current =
            (ownerProj?.lifecycle_state ?? "active") === "archived";
          setSelectedProjectSync(targetSess.project_id);
        }
      }
      setActiveViewState("chat");
      setSelectedSessionSync(sessionId);
      setCancelledNotice(null);
      await loadSessionDetail(sessionId);
    },
    [loadSessionDetail, setSelectedProjectSync, setSelectedSessionSync],
  );

  const openSessionInChat = useCallback(
    async (sessionId: string) => {
      await selectSession(sessionId);
    },
    [selectSession],
  );

  // D2-UX-05: Always sync starter/newly created agent default model with runtime catalog default_model_id!
  const ensureScopedAgent = useCallback(
    async (
      memoryScope: string,
      projectName?: string,
      hasWorkspace = false,
    ): Promise<AgentRecord> => {
      const exactMatch = agentsRef.current.find(
        (agent) =>
          agent.current_config.allowed_memory_scopes.length === 1 &&
          agent.current_config.allowed_memory_scopes[0] === memoryScope,
      );
      if (exactMatch) {
        return exactMatch;
      }

      let cat = catalogRef.current;
      if (!cat) {
        cat = await bridge.getRuntimeCatalog(false).catch(() => null);
        if (cat) {
          catalogRef.current = cat;
          setRuntimeCatalog(cat);
        }
      }

      const defaultModelId = cat?.default_model_id ?? cat?.models[0]?.model_id ?? null;
      const defaultThinking = cat?.default_thinking_level ?? "medium";
      const label = projectName?.trim() || "Проект";
      const created = await bridge.createAgent({
        name: `Агент · ${label}`.slice(0, 120),
        description: `Агент Dennett для области памяти ${memoryScope}`,
        system_prompt: DEFAULT_SYSTEM_PROMPT,
        model_preference: defaultModelId,
        thinking_level: defaultThinking,
        allowed_memory_scopes: [memoryScope],
        workspace_write_policy: hasWorkspace ? { mode: "allow_workspace" } : { mode: "deny_all" },
      });
      agentsRef.current = [created, ...agentsRef.current];
      setAgents((prev) => [created, ...prev]);
      return created;
    },
    [bridge],
  );

  const ensureStandaloneAgent = useCallback(async (): Promise<AgentRecord> => {
    const exactMatch = agentsRef.current.find(
      (agent) =>
        agent.current_config.allowed_memory_scopes.length === 1 &&
        agent.current_config.allowed_memory_scopes[0] === STANDALONE_MEMORY_SCOPE,
    );
    if (exactMatch) {
      return exactMatch;
    }

    let cat = catalogRef.current;
    if (!cat) {
      cat = await bridge.getRuntimeCatalog(false).catch(() => null);
      if (cat) {
        catalogRef.current = cat;
        setRuntimeCatalog(cat);
      }
    }

    const defaultModelId = cat?.default_model_id ?? cat?.models[0]?.model_id ?? null;
    const defaultThinking = cat?.default_thinking_level ?? "medium";
    const created = await bridge.createAgent({
      name: "Агент · Чаты",
      description: "Агент Dennett для одиночных чатов без привязки к папке проекта",
      system_prompt: STANDALONE_SYSTEM_PROMPT,
      model_preference: defaultModelId,
      thinking_level: defaultThinking,
      allowed_memory_scopes: [STANDALONE_MEMORY_SCOPE],
      workspace_write_policy: { mode: "deny_all" },
    });
    agentsRef.current = [created, ...agentsRef.current];
    setAgents((prev) => [created, ...prev]);
    return created;
  }, [bridge]);

  const createStandaloneChat = useCallback(async (): Promise<SessionRecord | null> => {
    setErrorBanner(null);
    try {
      inspectingArchivedProjectRef.current = false;
      const agent = await ensureStandaloneAgent();
      const created = await bridge.createSession({
        agent_id: agent.agent_id,
        project_id: null,
        title: "Новый чат",
        session_kind: "user_chat",
      });
      const nextSessions = sortSessionsByPin([created, ...sessionsRef.current]);
      sessionsRef.current = nextSessions;
      setSessions(nextSessions);
      setActiveViewState("chat");
      setSelectedProjectSync(null);
      setSelectedSessionSync(created.session_id);
      setProjectFiles(null);
      setCancelledNotice(null);
      await loadSessionDetail(created.session_id);
      focusComposerSoon(false);
      return created;
    } catch (err) {
      setErrorBanner(normalizeBridgeError(err));
      return null;
    }
  }, [
    bridge,
    ensureStandaloneAgent,
    focusComposerSoon,
    loadSessionDetail,
    setSelectedProjectSync,
    setSelectedSessionSync,
  ]);

  const toggleProjectPin = useCallback(
    async (projectId: string) => {
      const existing = projectsRef.current.find((p) => p.project_id === projectId);
      if (!existing) {
        return;
      }
      setErrorBanner(null);
      const previous = projectsRef.current;
      const nextPinned = !existing.pinned;
      const optimistic: ProjectRecord = {
        ...existing,
        pinned: nextPinned,
        pinned_at: nextPinned ? Date.now() : null,
      };
      const optimisticList = sortProjectsByPin(
        previous.map((p) => (p.project_id === projectId ? optimistic : p)),
      );
      projectsRef.current = optimisticList;
      setProjects(optimisticList);
      try {
        const updated = await bridge.updateProject(projectId, { pinned: nextPinned });
        const committed = sortProjectsByPin(
          projectsRef.current.map((p) => (p.project_id === projectId ? updated : p)),
        );
        projectsRef.current = committed;
        setProjects(committed);
      } catch (err) {
        projectsRef.current = previous;
        setProjects(previous);
        setErrorBanner(normalizeBridgeError(err));
      }
    },
    [bridge],
  );

  const toggleSessionPin = useCallback(
    async (sessionId: string) => {
      const existing = sessionsRef.current.find((s) => s.session_id === sessionId);
      if (!existing) {
        return;
      }
      setErrorBanner(null);
      const previous = sessionsRef.current;
      const nextPinned = !existing.pinned;
      const optimistic: SessionRecord = {
        ...existing,
        pinned: nextPinned,
        pinned_at: nextPinned ? Date.now() : null,
      };
      const optimisticList = sortSessionsByPin(
        previous.map((s) => (s.session_id === sessionId ? optimistic : s)),
      );
      sessionsRef.current = optimisticList;
      setSessions(optimisticList);
      setActiveDetail((prev) =>
        prev && prev.session.session_id === sessionId ? { ...prev, session: optimistic } : prev,
      );
      try {
        const updated = await bridge.updateSession(sessionId, { pinned: nextPinned });
        const committed = sortSessionsByPin(
          sessionsRef.current.map((s) => (s.session_id === sessionId ? updated : s)),
        );
        sessionsRef.current = committed;
        setSessions(committed);
        setActiveDetail((prev) =>
          prev && prev.session.session_id === sessionId ? { ...prev, session: updated } : prev,
        );
      } catch (err) {
        sessionsRef.current = previous;
        setSessions(previous);
        setActiveDetail((prev) =>
          prev && prev.session.session_id === sessionId ? { ...prev, session: existing } : prev,
        );
        setErrorBanner(normalizeBridgeError(err));
      }
    },
    [bridge],
  );

  const createFolderlessProject = useCallback(async (): Promise<ProjectRecord | null> => {
    setErrorBanner(null);
    try {
      const name = nextUntitledProjectName(projectsRef.current);
      const project = await bridge.createProject({
        name,
        workspace_path: null,
        memory_scope: null,
      });
      inspectingArchivedProjectRef.current = false;
      const nextProjects = sortProjectsByPin([project, ...projectsRef.current]);
      projectsRef.current = nextProjects;
      setProjects(nextProjects);
      const agent = await ensureScopedAgent(project.memory_scope, project.name, false);
      const session = await bridge.createSession({
        agent_id: agent.agent_id,
        project_id: project.project_id,
        title: "Новый чат",
        session_kind: "user_chat",
      });
      const nextSessions = sortSessionsByPin([session, ...sessionsRef.current]);
      sessionsRef.current = nextSessions;
      setSessions(nextSessions);
      setActiveViewState("chat");
      setSelectedProjectSync(project.project_id);
      setSelectedSessionSync(session.session_id);
      setCancelledNotice(null);
      await loadSessionDetail(session.session_id);
      focusComposerSoon(false);
      return project;
    } catch (err) {
      setErrorBanner(normalizeBridgeError(err));
      return null;
    }
  }, [
    bridge,
    ensureScopedAgent,
    focusComposerSoon,
    loadSessionDetail,
    setSelectedProjectSync,
    setSelectedSessionSync,
  ]);

  const createProjectFromFolder = useCallback(
    async (folderPath: string): Promise<ProjectRecord | null> => {
      const trimmed = folderPath.trim();
      if (!trimmed) {
        return null;
      }
      setErrorBanner(null);
      try {
        const name = extractFolderBasename(trimmed);
        const project = await bridge.createProject({
          name,
          workspace_path: trimmed,
          memory_scope: null,
        });
        inspectingArchivedProjectRef.current = false;
        const nextProjects = sortProjectsByPin([project, ...projectsRef.current]);
        projectsRef.current = nextProjects;
        setProjects(nextProjects);
        const agent = await ensureScopedAgent(project.memory_scope, project.name, true);
        const session = await bridge.createSession({
          agent_id: agent.agent_id,
          project_id: project.project_id,
          title: "Новый чат",
          session_kind: "user_chat",
        });
        const nextSessions = sortSessionsByPin([session, ...sessionsRef.current]);
        sessionsRef.current = nextSessions;
        setSessions(nextSessions);
        setActiveViewState("chat");
        setSelectedProjectSync(project.project_id);
        setSelectedSessionSync(session.session_id);
        setCancelledNotice(null);
        await loadSessionDetail(session.session_id);
        void loadFilesForProject(project.project_id);
        focusComposerSoon(false);
        return project;
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
        return null;
      }
    },
    [
      bridge,
      ensureScopedAgent,
      focusComposerSoon,
      loadFilesForProject,
      loadSessionDetail,
      setSelectedProjectSync,
      setSelectedSessionSync,
    ],
  );

  // Owner Directive 1: Create blank user_chat with optional prefilled draft and cursor at end!
  const createBlankChatWithDraft = useCallback(
    async (projectId?: string, initialDraft?: string): Promise<SessionRecord | null> => {
      setErrorBanner(null);
      const activeProjectsList = projectsRef.current.filter(isActiveProjectRecord);
      const explicitValid = projectId
        ? activeProjectsList.find((p) => p.project_id === projectId)
        : undefined;
      const currentValid =
        activeProject && isActiveProjectRecord(activeProject) ? activeProject : undefined;
      const targetProject = explicitValid ?? currentValid ?? activeProjectsList[0];

      if (!targetProject) {
        const createdProj = await createFolderlessProject();
        const createdSid = selectedSessionRef.current;
        if (createdProj && createdSid && initialDraft !== undefined) {
          setDraftsBySession((prev) => ({ ...prev, [createdSid]: initialDraft }));
          focusComposerSoon(true);
          return sessionsRef.current.find((s) => s.session_id === createdSid) ?? null;
        }
        return null;
      }

      try {
        inspectingArchivedProjectRef.current = false;
        const agent = await ensureScopedAgent(
          targetProject.memory_scope,
          targetProject.name,
          Boolean(targetProject.workspace_path),
        );
        const created = await bridge.createSession({
          agent_id: agent.agent_id,
          project_id: targetProject.project_id,
          title: "Новый чат",
          session_kind: "user_chat",
        });
        const nextSessions = sortSessionsByPin([created, ...sessionsRef.current]);
        sessionsRef.current = nextSessions;
        setSessions(nextSessions);
        if (initialDraft !== undefined) {
          setDraftsBySession((prev) => ({ ...prev, [created.session_id]: initialDraft }));
        }
        setActiveViewState("chat");
        setSelectedProjectSync(targetProject.project_id);
        setSelectedSessionSync(created.session_id);
        setCancelledNotice(null);
        await loadSessionDetail(created.session_id);
        focusComposerSoon(initialDraft !== undefined && initialDraft.length > 0);
        return created;
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
        return null;
      }
    },
    [
      activeProject,
      bridge,
      createFolderlessProject,
      ensureScopedAgent,
      focusComposerSoon,
      loadSessionDetail,
      setSelectedProjectSync,
      setSelectedSessionSync,
    ],
  );

  const createBlankChat = useCallback(
    async (projectId?: string): Promise<SessionRecord | null> =>
      createBlankChatWithDraft(projectId, undefined),
    [createBlankChatWithDraft],
  );

  const upgradeFolderlessAgentIfNeeded = useCallback(
    async (project: ProjectRecord, wasFolderless: boolean) => {
      if (!wasFolderless || !project.workspace_path) {
        return;
      }
      const scopedAgent = agentsRef.current.find(
        (a) =>
          a.current_config.allowed_memory_scopes.length === 1 &&
          a.current_config.allowed_memory_scopes[0] === project.memory_scope,
      );
      if (
        scopedAgent &&
        scopedAgent.current_config.workspace_write_policy.mode === "deny_all"
      ) {
        const upgraded = await bridge.updateAgentConfig(scopedAgent.agent_id, {
          system_prompt: scopedAgent.current_config.system_prompt,
          model_preference: scopedAgent.current_config.model_preference ?? null,
          thinking_level: scopedAgent.current_config.thinking_level ?? null,
          allowed_memory_scopes: scopedAgent.current_config.allowed_memory_scopes,
          workspace_write_policy: { mode: "allow_workspace" },
        });
        agentsRef.current = agentsRef.current.map((a) =>
          a.agent_id === upgraded.agent_id ? upgraded : a,
        );
        setAgents((prev) => prev.map((a) => (a.agent_id === upgraded.agent_id ? upgraded : a)));
      }
    },
    [bridge],
  );

  const attachProjectFolder = useCallback(
    async (projectId: string, folderPath: string) => {
      const trimmed = folderPath.trim();
      if (!trimmed) {
        return;
      }
      setErrorBanner(null);
      const wasFolderless = !projectsRef.current.find((p) => p.project_id === projectId)
        ?.workspace_path;
      try {
        const updated = await bridge.updateProject(projectId, { workspace_path: trimmed });
        projectsRef.current = projectsRef.current.map((p) =>
          p.project_id === projectId ? updated : p,
        );
        setProjects((prev) => prev.map((p) => (p.project_id === projectId ? updated : p)));
        await upgradeFolderlessAgentIfNeeded(updated, wasFolderless);
        if (activeProject?.project_id === projectId) {
          await loadFilesForProject(projectId);
        }
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
      }
    },
    [activeProject?.project_id, bridge, loadFilesForProject, upgradeFolderlessAgentIfNeeded],
  );

  const rebindProjectFolder = useCallback(
    async (projectId: string, folderPath: string) => {
      const trimmed = folderPath.trim();
      if (!trimmed) {
        return;
      }
      setErrorBanner(null);
      const wasFolderless = !projectsRef.current.find((p) => p.project_id === projectId)
        ?.workspace_path;
      try {
        const resp = await bridge.rebindProject(projectId, { workspace_path: trimmed });
        projectsRef.current = projectsRef.current.map((p) =>
          p.project_id === projectId ? resp.project : p,
        );
        setProjects((prev) => prev.map((p) => (p.project_id === projectId ? resp.project : p)));
        await upgradeFolderlessAgentIfNeeded(resp.project, wasFolderless);
        if (activeProject?.project_id === projectId) {
          await loadFilesForProject(projectId);
        }
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
      }
    },
    [activeProject?.project_id, bridge, loadFilesForProject, upgradeFolderlessAgentIfNeeded],
  );

  const detachProjectWorkspace = useCallback(
    async (projectId: string) => {
      setErrorBanner(null);
      try {
        const resp = await bridge.detachProjectWorkspace(projectId);
        projectsRef.current = projectsRef.current.map((p) =>
          p.project_id === projectId ? resp.project : p,
        );
        setProjects((prev) => prev.map((p) => (p.project_id === projectId ? resp.project : p)));
        if (activeProject?.project_id === projectId) {
          setProjectFiles(null);
        }
        setCancelledNotice("Папка отвязана от проекта. Ваши файлы на диске не затронуты.");
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
      }
    },
    [activeProject?.project_id, bridge],
  );

  const archiveProject = useCallback(
    async (projectId: string) => {
      setErrorBanner(null);
      try {
        inspectingArchivedProjectRef.current = false;
        const resp = await bridge.archiveProject(projectId);
        await applySnapshotCollections(undefined, true);
        setCancelledNotice(
          resp.affected_timers > 0
            ? `Проект архивирован. Приостановлено таймеров: ${resp.affected_timers}. Файлы на диске не тронуты.`
            : "Проект перемещён в архив. Ваши файлы на диске не затронуты.",
        );
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
      }
    },
    [applySnapshotCollections, bridge],
  );

  const unarchiveProject = useCallback(
    async (projectId: string) => {
      setErrorBanner(null);
      try {
        inspectingArchivedProjectRef.current = false;
        await bridge.unarchiveProject(projectId);
        await applySnapshotCollections(undefined, true);
        setCancelledNotice("Проект возвращён из архива.");
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
      }
    },
    [applySnapshotCollections, bridge],
  );

  const removeProject = useCallback(
    async (projectId: string) => {
      setErrorBanner(null);
      try {
        inspectingArchivedProjectRef.current = false;
        await bridge.removeProject(projectId);
        await applySnapshotCollections(undefined, true);
        setCancelledNotice("Проект убран из списка Dennett. Ваши локальные файлы не удалялись.");
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
      }
    },
    [applySnapshotCollections, bridge],
  );

  const deleteProject = useCallback(
    async (projectId: string) => {
      setErrorBanner(null);
      try {
        inspectingArchivedProjectRef.current = false;
        await bridge.deleteProject(projectId, true);
        await applySnapshotCollections(undefined, true);
        setCancelledNotice(
          "Служебная запись проекта в Dennett удалена. Папка с вашими файлами на диске осталась нетронутой.",
        );
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
      }
    },
    [applySnapshotCollections, bridge],
  );

  const renameProject = useCallback(
    async (projectId: string, name: string) => {
      const trimmed = name.trim();
      if (!trimmed) {
        return;
      }
      setErrorBanner(null);
      try {
        const updated = await bridge.updateProject(projectId, { name: trimmed });
        projectsRef.current = projectsRef.current.map((p) =>
          p.project_id === projectId ? updated : p,
        );
        setProjects((prev) => prev.map((p) => (p.project_id === projectId ? updated : p)));
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
      }
    },
    [bridge],
  );

  const renameSession = useCallback(
    async (sessionId: string, title: string) => {
      const trimmed = title.trim();
      if (!trimmed) {
        return;
      }
      setErrorBanner(null);
      try {
        const updated = await bridge.updateSession(sessionId, { title: trimmed });
        setSessions((prev) => {
          const next = sortSessionsByPin(
            prev.map((s) => (s.session_id === sessionId ? updated : s)),
          );
          sessionsRef.current = next;
          return next;
        });
        setActiveDetail((prev) =>
          prev && prev.session.session_id === sessionId ? { ...prev, session: updated } : prev,
        );
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
      }
    },
    [bridge],
  );

  const addCapabilityFromFolder = useCallback(
    async (
      folderPath: string,
      kind: "skill" | "plugin",
      scopeKind: CapabilityScopeKind,
      projectId: string | null,
    ): Promise<CapabilityRecord | null> => {
      const trimmed = folderPath.trim();
      if (!trimmed) {
        return null;
      }
      const displayName = extractFolderBasename(trimmed);
      const slug =
        displayName
          .toLowerCase()
          .replace(/[^a-z0-9._:-]+/g, "-")
          .replace(/^-+|-+$/g, "")
          .slice(0, 80) || `${kind}-${Date.now().toString(16)}`;
      setErrorBanner(null);
      try {
        // Send real `source_path` with minimal contract-compliant placeholders;
        // dennett-head inspects SKILL.md / plugin manifest on disk read-only and rejects invalid folders.
        const created = await bridge.createCapability({
          slug,
          display_name: displayName,
          description: "",
          version: "1.0.0",
          origin_kind: "user_manual",
          ownership: scopeKind === "project" ? "project" : "user",
          scope_kind: scopeKind,
          project_id: scopeKind === "project" ? projectId : null,
          trust_state: "unreviewed",
          activation_state: "disabled",
          source_path: trimmed,
          spec:
            kind === "plugin"
              ? {
                  kind: "plugin",
                  manifest_version: "1.0",
                  skills: [],
                  mcp_servers: [],
                  has_hooks: false,
                  enabled_components: [],
                }
              : {
                  kind: "skill",
                  skill_markdown: "pending-source-path-inspection",
                  has_scripts: false,
                  entry_files: [],
                },
        });
        setCapabilities((prev) => [
          created,
          ...prev.filter((c) => c.capability_id !== created.capability_id),
        ]);
        return created;
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
        return null;
      }
    },
    [bridge],
  );

  const requestFolderPicker = useCallback(
    async (intent: FolderPickerIntent) => {
      if (bridge.supportsNativeFolderPicker) {
        try {
          const picked = await bridge.pickWorkspaceFolder();
          if (!picked) {
            return;
          }
          if (intent.mode === "create-project") {
            await createProjectFromFolder(picked);
          } else if (intent.mode === "rebind-project") {
            await rebindProjectFolder(intent.projectId, picked);
          } else if (intent.mode === "add-capability") {
            await addCapabilityFromFolder(picked, intent.kind, intent.scopeKind, intent.projectId);
          } else {
            await attachProjectFolder(intent.projectId, picked);
          }
        } catch (err) {
          setErrorBanner(normalizeBridgeError(err));
        }
        return;
      }
      setFolderPickerIntent(intent);
    },
    [
      addCapabilityFromFolder,
      attachProjectFolder,
      bridge,
      createProjectFromFolder,
      rebindProjectFolder,
    ],
  );

  const closeFolderPicker = useCallback(() => {
    setFolderPickerIntent(null);
  }, []);

  const submitFolderPickerPath = useCallback(
    async (folderPath: string) => {
      const intent = folderPickerIntent;
      setFolderPickerIntent(null);
      if (!intent) {
        return;
      }
      if (intent.mode === "create-project") {
        await createProjectFromFolder(folderPath);
      } else if (intent.mode === "rebind-project") {
        await rebindProjectFolder(intent.projectId, folderPath);
      } else if (intent.mode === "add-capability") {
        await addCapabilityFromFolder(
          folderPath,
          intent.kind,
          intent.scopeKind,
          intent.projectId,
        );
      } else {
        await attachProjectFolder(intent.projectId, folderPath);
      }
    },
    [
      addCapabilityFromFolder,
      attachProjectFolder,
      createProjectFromFolder,
      folderPickerIntent,
      rebindProjectFolder,
    ],
  );

  const updateDraft = useCallback((sessionId: string, value: string) => {
    setDraftsBySession((prev) => ({ ...prev, [sessionId]: value }));
  }, []);

  const queueOfflineMessage = useCallback((sessionId: string, content: string) => {
    const trimmed = content.trim();
    if (!trimmed) {
      return;
    }
    const sess = sessionsRef.current.find((s) => s.session_id === sessionId);
    const item: QueuedOfflineMessage = {
      client_message_id: generateClientMessageId(),
      session_id: sessionId,
      project_id: sess?.project_id ?? null,
      content: trimmed,
      created_at: Date.now(),
    };
    setQueuedMessages((prev) => [...prev, item]);
    setDraftsBySession((prev) => ({ ...prev, [sessionId]: "" }));
  }, []);

  const cancelQueuedMessage = useCallback((clientMessageId: string) => {
    setQueuedMessages((prev) => prev.filter((item) => item.client_message_id !== clientMessageId));
  }, []);

  const dispatchMessageToHead = useCallback(
    async (sessionId: string, content: string, clientMessageId?: string | null) => {
      const effectiveClientMessageId = clientMessageId ?? generateClientMessageId();
      const optimisticId = `optimistic-${sessionId}-${Date.now()}`;
      const optimisticMessage: MessageRecord = {
        message_id: optimisticId,
        session_id: sessionId,
        role: "user",
        content,
        run_id: null,
        client_message_id: effectiveClientMessageId,
        correlation_id: "pending",
        created_at: Date.now(),
      };

      setErrorBanner(null);
      setCancelledNotice(null);
      setOptimisticMessagesBySession((prev) => ({
        ...prev,
        [sessionId]: [...(prev[sessionId] ?? []), optimisticMessage],
      }));
      incrementLocalInFlight(sessionId);

      const removeOptimistic = () => {
        setOptimisticMessagesBySession((prev) => {
          const list = prev[sessionId];
          if (!list || list.length === 0) {
            return prev;
          }
          const remaining = list.filter((m) => m.message_id !== optimisticId);
          if (remaining.length === 0) {
            const next = { ...prev };
            delete next[sessionId];
            return next;
          }
          return { ...prev, [sessionId]: remaining };
        });
      };

      try {
        const response = await bridge.sendMessage(sessionId, {
          content,
          client_message_id: effectiveClientMessageId,
        });

        setSessions((prev) => {
          const next = sortSessionsByPin(
            prev.map((s) =>
              s.session_id === sessionId
                ? {
                    ...s,
                    title:
                      response.session_title && response.session_title.trim().length > 0
                        ? response.session_title
                        : s.title,
                    runtime_binding: response.runtime_binding,
                    updated_at: response.run.finished_at ?? response.run.created_at,
                  }
                : s,
            ),
          );
          sessionsRef.current = next;
          return next;
        });

        setHealthSnapshot((prev) =>
          prev && prev.health
            ? {
                ...prev,
                health: {
                  ...prev.health,
                  memory_status: response.memory_status,
                },
              }
            : prev,
        );

        const steerActiveOnSession = (activeSteerCountBySessionRef.current[sessionId] ?? 0) > 0;
        if (selectedSessionRef.current === sessionId) {
          if (response.run.status === "cancelled" && !steerActiveOnSession) {
            setCancelledNotice("Запуск остановлен пользователем (Cancelled).");
          } else if (response.run.status === "failed" && response.run.error_summary) {
            setErrorBanner({
              code: response.run.error_code ?? "RUN_FAILED",
              message: formatDecisionReasonRu(response.run.error_summary),
            });
          }
          if (!steerActiveOnSession) {
            setLiveRun((prev) => (prev && prev.session_id === sessionId ? null : prev));
          }
          await loadSessionDetail(sessionId);
        }
        removeOptimistic();

        await applySnapshotCollections(undefined, false).catch(() => {});
        const diag = await bridge.listDiagnostics(20).catch(() => null);
        if (diag) {
          setDiagnostics(diag);
        }
        if (!steerActiveOnSession) {
          localCompletedSessionRef.current = sessionId;
        }
        return true;
      } catch (err) {
        removeOptimistic();
        throw err;
      } finally {
        decrementLocalInFlight(sessionId);
        cancellingSessionIdsRef.current.delete(sessionId);
        setCancellingSessionIds((prev) => prev.filter((id) => id !== sessionId));
      }
    },
    [
      applySnapshotCollections,
      bridge,
      decrementLocalInFlight,
      incrementLocalInFlight,
      loadSessionDetail,
    ],
  );

  // D2-BUG-06: Owner-confirmed offline queue flush (NEVER blind auto-flush!).
  // Terminal 404/409 errors remove & skip the broken item so the queue never wedges.
  const flushQueuedMessages = useCallback(async () => {
    if (isFlushingQueueRef.current || queuedMessagesRef.current.length === 0) {
      return;
    }
    if (!healthSnapshot?.connected || !isSyncedAfterConnect) {
      return;
    }
    isFlushingQueueRef.current = true;
    setIsFlushingQueue(true);
    try {
      const snapshotQueue = [...queuedMessagesRef.current];
      let lastSkippedError: BridgeErrorPayload | null = null;
      for (const item of snapshotQueue) {
        const proj = item.project_id
          ? projectsRef.current.find((p) => p.project_id === item.project_id)
          : undefined;
        if (
          proj &&
          (proj.lifecycle_state === "archived" || proj.lifecycle_state === "deleted_record")
        ) {
          setQueuedMessages((prev) =>
            prev.filter((q) => q.client_message_id !== item.client_message_id),
          );
          lastSkippedError = {
            code: "PROJECT_ARCHIVED",
            message: `Отложенное сообщение пропущено: проект «${proj.name}» в архиве или удалён.`,
          };
          setErrorBanner(lastSkippedError);
          continue;
        }
        try {
          await dispatchMessageToHead(item.session_id, item.content, item.client_message_id);
          setQueuedMessages((prev) =>
            prev.filter((q) => q.client_message_id !== item.client_message_id),
          );
        } catch (err) {
          const normalized = normalizeBridgeError(err);
          if (normalized.code === "HEAD_UNREACHABLE") {
            setIsSyncedAfterConnect(false);
            setHealthSnapshot((prev) =>
              prev
                ? {
                    ...prev,
                    connected: false,
                    error_code: normalized.code,
                    error: normalized.message,
                  }
                : prev,
            );
            break;
          }
          if (TERMINAL_QUEUE_ERROR_CODES.has(normalized.code.toUpperCase())) {
            // Terminal 404/409/400/403 -> drop broken item cleanly and continue flushing remaining items!
            setQueuedMessages((prev) =>
              prev.filter((q) => q.client_message_id !== item.client_message_id),
            );
            lastSkippedError = {
              code: normalized.code,
              message: `Отложенное сообщение снято с очереди (${normalized.code}): ${formatDecisionReasonRu(normalized.message)}`,
            };
            setErrorBanner(lastSkippedError);
            continue;
          }
          setErrorBanner(normalized);
          break;
        }
      }
      if (lastSkippedError) {
        setErrorBanner(lastSkippedError);
      }
    } finally {
      isFlushingQueueRef.current = false;
      setIsFlushingQueue(false);
    }
  }, [dispatchMessageToHead, healthSnapshot?.connected, isSyncedAfterConnect]);

  const sendMessage = useCallback(async () => {
    const sessionId = selectedSessionRef.current;
    if (!sessionId || runningSessionIdsRef.current.has(sessionId)) {
      return;
    }
    const rawDraft = draftsBySession[sessionId] ?? "";
    const content = rawDraft.trim();
    if (!content) {
      return;
    }

    if (!healthSnapshot?.connected) {
      queueOfflineMessage(sessionId, content);
      return;
    }

    setDraftsBySession((prev) => ({ ...prev, [sessionId]: "" }));
    const stableClientMsgId = generateClientMessageId();

    try {
      await dispatchMessageToHead(sessionId, content, stableClientMsgId);
    } catch (err) {
      const normalized = normalizeBridgeError(err);
      if (normalized.code === "HEAD_UNREACHABLE") {
        // Head dropped mid-send -> queue idempotently with SAME client_message_id!
        const sess = sessionsRef.current.find((s) => s.session_id === sessionId);
        setQueuedMessages((prev) => [
          ...prev,
          {
            client_message_id: stableClientMsgId,
            session_id: sessionId,
            project_id: sess?.project_id ?? null,
            content,
            created_at: Date.now(),
          },
        ]);
        setIsSyncedAfterConnect(false);
        setHealthSnapshot((prev) =>
          prev
            ? {
                ...prev,
                connected: false,
                error_code: normalized.code,
                error: normalized.message,
              }
            : prev,
        );
      } else {
        setDraftsBySession((prev) => {
          const existing = prev[sessionId] ?? "";
          return existing.length === 0 ? { ...prev, [sessionId]: rawDraft } : prev;
        });
        setErrorBanner(normalized);
      }
    }
  }, [dispatchMessageToHead, draftsBySession, healthSnapshot?.connected, queueOfflineMessage]);

  // D2-ARCH-01: NEVER block cancelSessionRun behind stale local running sets!
  const cancelSessionRun = useCallback(
    async (targetSessionId: string) => {
      if (!targetSessionId || cancellingSessionIdsRef.current.has(targetSessionId)) {
        return;
      }
      cancellingSessionIdsRef.current.add(targetSessionId);
      setCancellingSessionIds((prev) =>
        prev.includes(targetSessionId) ? prev : [...prev, targetSessionId],
      );
      try {
        const resp = await bridge.cancelRun(targetSessionId);
        if (selectedSessionRef.current === targetSessionId) {
          setLiveRun(null);
          setCancelledNotice(
            resp.external_outcome_unknown
              ? "Остановлено локально, но внешний исход неизвестен (UNKNOWN) — проверьте во Входящих."
              : resp.cancelled
                ? "Запуск остановлен (Cancelled). Новые действия отменены."
                : "Команда остановки отправлена в dennett-head.",
          );
        }
        setHealthSnapshot((prev) =>
          prev && prev.health
            ? {
                ...prev,
                health: {
                  ...prev.health,
                  memory_status: resp.memory_status,
                },
              }
            : prev,
        );
        await applySnapshotCollections(undefined, true).catch(() => {});
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
      } finally {
        cancellingSessionIdsRef.current.delete(targetSessionId);
        setCancellingSessionIds((prev) => prev.filter((id) => id !== targetSessionId));
      }
    },
    [applySnapshotCollections, bridge],
  );

  const stopActiveRun = useCallback(async () => {
    const targetSessionId = selectedSessionRef.current;
    if (!targetSessionId) {
      return;
    }
    await cancelSessionRun(targetSessionId);
  }, [cancelSessionRun]);

  // Phase D3+ Durable Follow-Up Queue & Live Steering Actions
  const enqueueFollowup = useCallback(
    async (content: string): Promise<SessionFollowupRecord | null> => {
      const sessionId = selectedSessionRef.current;
      const trimmed = content.trim();
      if (!sessionId || !trimmed || !healthSnapshot?.connected) {
        return null;
      }
      setErrorBanner(null);
      try {
        const created = await bridge.enqueueFollowup(sessionId, {
          content: trimmed,
          idempotency_key: generateClientMessageId(),
        });
        if (selectedSessionRef.current === sessionId) {
          setActiveFollowups((prev) =>
            [...prev.filter((f) => f.followup_id !== created.followup_id), created]
              .filter((f) => f.status === "queued" || f.status === "paused")
              .sort((a, b) => a.position - b.position),
          );
        }
        return created;
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
        return null;
      }
    },
    [bridge, healthSnapshot?.connected],
  );

  const updateFollowup = useCallback(
    async (followupId: string, content: string): Promise<SessionFollowupRecord | null> => {
      const sessionId = selectedSessionRef.current;
      const trimmed = content.trim();
      if (!sessionId || !trimmed || !healthSnapshot?.connected) {
        return null;
      }
      setErrorBanner(null);
      try {
        const updated = await bridge.updateFollowup(sessionId, followupId, { content: trimmed });
        if (selectedSessionRef.current === sessionId) {
          setActiveFollowups((prev) =>
            prev
              .map((f) => (f.followup_id === followupId ? updated : f))
              .filter((f) => f.status === "queued" || f.status === "paused")
              .sort((a, b) => a.position - b.position),
          );
        }
        return updated;
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
        return null;
      }
    },
    [bridge, healthSnapshot?.connected],
  );

  const deleteFollowup = useCallback(
    async (followupId: string): Promise<boolean> => {
      const sessionId = selectedSessionRef.current;
      if (!sessionId || !healthSnapshot?.connected) {
        return false;
      }
      setErrorBanner(null);
      try {
        await bridge.deleteFollowup(sessionId, followupId);
        if (selectedSessionRef.current === sessionId) {
          setActiveFollowups((prev) => prev.filter((f) => f.followup_id !== followupId));
        }
        return true;
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
        return false;
      }
    },
    [bridge, healthSnapshot?.connected],
  );

  const clearFollowups = useCallback(async (): Promise<boolean> => {
    const sessionId = selectedSessionRef.current;
    if (!sessionId || !healthSnapshot?.connected) {
      return false;
    }
    setErrorBanner(null);
    try {
      await bridge.clearFollowups(sessionId);
      if (selectedSessionRef.current === sessionId) {
        setActiveFollowups([]);
      }
      return true;
    } catch (err) {
      setErrorBanner(normalizeBridgeError(err));
      return false;
    }
  }, [bridge, healthSnapshot?.connected]);

  const resumeFollowups = useCallback(async (): Promise<boolean> => {
    const sessionId = selectedSessionRef.current;
    if (!sessionId || !healthSnapshot?.connected) {
      return false;
    }
    setErrorBanner(null);
    try {
      const resp = await bridge.resumeFollowups(sessionId);
      if (selectedSessionRef.current === sessionId) {
        setActiveFollowups(
          (resp.followups ?? [])
            .filter((f) => f.status === "queued" || f.status === "paused")
            .sort((a, b) => a.position - b.position),
        );
        if (resp.dispatched) {
          setCancelledNotice(null);
          await loadSessionDetail(sessionId);
        }
      }
      await applySnapshotCollections(undefined, false).catch(() => {});
      return true;
    } catch (err) {
      setErrorBanner(normalizeBridgeError(err));
      return false;
    }
  }, [applySnapshotCollections, bridge, healthSnapshot?.connected, loadSessionDetail]);

  const steerSession = useCallback(
    async (content: string): Promise<boolean> => {
      const sessionId = selectedSessionRef.current;
      const trimmed = content.trim();
      if (!sessionId || !trimmed || !healthSnapshot?.connected) {
        return false;
      }

      const stableClientMsgId = generateClientMessageId();
      const optimisticId = `optimistic-steer-${sessionId}-${Date.now()}`;
      const optimisticMessage: MessageRecord = {
        message_id: optimisticId,
        session_id: sessionId,
        role: "user",
        content: trimmed,
        run_id: null,
        client_message_id: stableClientMsgId,
        correlation_id: "pending",
        created_at: Date.now(),
      };

      setErrorBanner(null);
      setCancelledNotice(null);
      setLiveRun((prev) => (prev && prev.session_id === sessionId ? null : prev));
      setOptimisticMessagesBySession((prev) => ({
        ...prev,
        [sessionId]: [...(prev[sessionId] ?? []), optimisticMessage],
      }));
      activeSteerCountBySessionRef.current[sessionId] =
        (activeSteerCountBySessionRef.current[sessionId] ?? 0) + 1;
      incrementLocalInFlight(sessionId);

      const removeOptimistic = () => {
        setOptimisticMessagesBySession((prev) => {
          const list = prev[sessionId];
          if (!list || list.length === 0) {
            return prev;
          }
          const remaining = list.filter((m) => m.message_id !== optimisticId);
          if (remaining.length === 0) {
            const next = { ...prev };
            delete next[sessionId];
            return next;
          }
          return { ...prev, [sessionId]: remaining };
        });
      };

      try {
        const response = await bridge.steerSession(sessionId, {
          content: trimmed,
          client_message_id: stableClientMsgId,
        });

        setSessions((prev) => {
          const next = sortSessionsByPin(
            prev.map((s) =>
              s.session_id === sessionId
                ? {
                    ...s,
                    title:
                      response.session_title && response.session_title.trim().length > 0
                        ? response.session_title
                        : s.title,
                    runtime_binding: response.runtime_binding,
                    updated_at: response.run.finished_at ?? response.run.created_at,
                  }
                : s,
            ),
          );
          sessionsRef.current = next;
          return next;
        });

        setHealthSnapshot((prev) =>
          prev && prev.health
            ? {
                ...prev,
                health: {
                  ...prev.health,
                  memory_status: response.memory_status,
                },
              }
            : prev,
        );

        if (selectedSessionRef.current === sessionId) {
          setLiveRun((prev) => (prev && prev.session_id === sessionId ? null : prev));
          if (response.run.status === "cancelled") {
            setCancelledNotice("Запуск остановлен пользователем (Cancelled).");
          } else if (response.run.status === "failed" && response.run.error_summary) {
            setErrorBanner({
              code: response.run.error_code ?? "RUN_FAILED",
              message: formatDecisionReasonRu(response.run.error_summary),
            });
          }
          await loadSessionDetail(sessionId);
        }
        removeOptimistic();

        await applySnapshotCollections(undefined, false).catch(() => {});
        const diag = await bridge.listDiagnostics(20).catch(() => null);
        if (diag) {
          setDiagnostics(diag);
        }
        localCompletedSessionRef.current = sessionId;
        return true;
      } catch (err) {
        removeOptimistic();
        setErrorBanner(normalizeBridgeError(err));
        return false;
      } finally {
        const remainingSteers = (activeSteerCountBySessionRef.current[sessionId] ?? 1) - 1;
        if (remainingSteers <= 0) {
          delete activeSteerCountBySessionRef.current[sessionId];
        } else {
          activeSteerCountBySessionRef.current[sessionId] = remainingSteers;
        }
        decrementLocalInFlight(sessionId);
        cancellingSessionIdsRef.current.delete(sessionId);
        setCancellingSessionIds((prev) => prev.filter((id) => id !== sessionId));
      }
    },
    [
      applySnapshotCollections,
      bridge,
      decrementLocalInFlight,
      healthSnapshot?.connected,
      incrementLocalInFlight,
      loadSessionDetail,
    ],
  );

  const updateActiveAgentModel = useCallback(
    async (modelPreference: string | null, thinkingLevel: ThinkingLevel | null) => {
      if (!activeAgent) {
        return;
      }
      setIsUpdatingModel(true);
      setErrorBanner(null);
      try {
        const updated = await bridge.updateAgentConfig(activeAgent.agent_id, {
          system_prompt: activeAgent.current_config.system_prompt,
          model_preference: modelPreference,
          thinking_level: thinkingLevel,
          allowed_memory_scopes: activeAgent.current_config.allowed_memory_scopes,
          workspace_write_policy: activeAgent.current_config.workspace_write_policy,
        });
        agentsRef.current = agentsRef.current.map((a) =>
          a.agent_id === updated.agent_id ? updated : a,
        );
        setAgents((prev) => prev.map((a) => (a.agent_id === updated.agent_id ? updated : a)));
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
      } finally {
        setIsUpdatingModel(false);
      }
    },
    [activeAgent, bridge],
  );

  const updateActiveAgentWritePolicy = useCallback(
    async (policy: WorkspaceWritePolicy) => {
      if (!activeAgent) {
        return;
      }
      setIsUpdatingModel(true);
      setErrorBanner(null);
      try {
        const updated = await bridge.updateAgentConfig(activeAgent.agent_id, {
          system_prompt: activeAgent.current_config.system_prompt,
          model_preference: activeAgent.current_config.model_preference ?? null,
          thinking_level: activeAgent.current_config.thinking_level ?? null,
          allowed_memory_scopes: activeAgent.current_config.allowed_memory_scopes,
          workspace_write_policy: policy,
        });
        agentsRef.current = agentsRef.current.map((a) =>
          a.agent_id === updated.agent_id ? updated : a,
        );
        setAgents((prev) => prev.map((a) => (a.agent_id === updated.agent_id ? updated : a)));
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
      } finally {
        setIsUpdatingModel(false);
      }
    },
    [activeAgent, bridge],
  );

  const createArtifact = useCallback(
    async (input: CreateArtifactInput): Promise<ArtifactRecord | null> => {
      const sessionId = selectedSessionRef.current;
      if (!sessionId) {
        return null;
      }
      setErrorBanner(null);
      try {
        const created = await bridge.createArtifact(sessionId, {
          name: input.name.trim(),
          mime_type: input.mimeType.trim() || "text/plain",
          content: input.content,
          completeness_status: input.completenessStatus ?? "final",
        });
        await loadSessionDetail(sessionId);
        selectedArtifactIdRef.current = created.artifact_id;
        setSelectedArtifactId(created.artifact_id);
        void loadRevisionsForArtifact(sessionId, created.name);
        setRightPanelOpen(true);
        return created;
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
        return null;
      }
    },
    [bridge, loadRevisionsForArtifact, loadSessionDetail, setRightPanelOpen],
  );

  const selectArtifact = useCallback(
    async (artifactId: string) => {
      selectedArtifactIdRef.current = artifactId;
      setSelectedArtifactId(artifactId);
      const sessionId = selectedSessionRef.current;
      if (!sessionId) {
        return;
      }
      try {
        const fetched = await bridge.getArtifact(sessionId, artifactId);
        setActiveDetail((prev) => {
          if (!prev || prev.session.session_id !== sessionId) {
            return prev;
          }
          const exists = prev.artifacts.some((a) => a.artifact_id === fetched.artifact_id);
          return {
            ...prev,
            artifacts: exists
              ? prev.artifacts.map((a) => (a.artifact_id === fetched.artifact_id ? fetched : a))
              : [...prev.artifacts, fetched],
          };
        });
        void loadRevisionsForArtifact(sessionId, fetched.name);
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
      }
    },
    [bridge, loadRevisionsForArtifact],
  );

  const applyArtifactLifecycle = useCallback(
    async (sessionId: string, artifactId: string, action: ArtifactLifecycleAction) => {
      setErrorBanner(null);
      try {
        await bridge.applyArtifactLifecycle(sessionId, artifactId, { action });
        if (action === "delete" || action === "remove") {
          selectedArtifactIdRef.current = null;
          setSelectedArtifactId(null);
        }
        await loadSessionDetail(sessionId);
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
      }
    },
    [bridge, loadSessionDetail],
  );

  // ============================================================================
  // Phase D2 Actions: Tasks, Inbox, Permissions, Grants, External Effects
  // ============================================================================

  const createTask = useCallback(
    async (request: CreateTaskRequest): Promise<TaskRecord | null> => {
      setErrorBanner(null);
      try {
        const created = await bridge.createTask(request);
        await applySnapshotCollections(undefined, false);
        return created;
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
        return null;
      }
    },
    [applySnapshotCollections, bridge],
  );

  const updateTaskGoal = useCallback(
    async (taskId: string, goal: string, title?: string): Promise<TaskRecord | null> => {
      const trimmedGoal = goal.trim();
      if (!trimmedGoal) {
        return null;
      }
      setErrorBanner(null);
      try {
        const updated = await bridge.updateTask(taskId, {
          goal: trimmedGoal,
          title: title?.trim() || null,
        });
        setTasks((prev) => prev.map((t) => (t.task_id === taskId ? updated : t)));
        return updated;
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
        return null;
      }
    },
    [bridge],
  );

  const cancelTask = useCallback(
    async (taskId: string): Promise<CancelTaskResponse | null> => {
      setStoppingTaskIds((prev) => (prev.includes(taskId) ? prev : [...prev, taskId]));
      setErrorBanner(null);
      try {
        const resp = await bridge.cancelTask(taskId);
        setTasks((prev) => prev.map((t) => (t.task_id === taskId ? resp.task : t)));
        if (resp.external_outcome_unknown) {
          setCancelledNotice(
            "Поручение остановлено локально, но внешний исход неизвестен (UNKNOWN) — требуется сверка.",
          );
        }
        await applySnapshotCollections(undefined, false).catch(() => {});
        return resp;
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
        return null;
      } finally {
        setStoppingTaskIds((prev) => prev.filter((id) => id !== taskId));
      }
    },
    [applySnapshotCollections, bridge],
  );

  const sendTaskSignal = useCallback(
    async (
      taskId: string,
      signalName: string,
      payloadText: string,
      idempotencyKey?: string,
    ): Promise<TaskSignalResponse | null> => {
      setErrorBanner(null);
      try {
        const resp = await bridge.sendTaskSignal(taskId, {
          signal_name: signalName.trim(),
          payload_text: payloadText,
          idempotency_key:
            idempotencyKey?.trim() ||
            `ui-sig-${Date.now()}-${Math.random().toString(16).slice(2, 8)}`,
        });
        setTasks((prev) => prev.map((t) => (t.task_id === taskId ? resp.task : t)));
        await applySnapshotCollections(undefined, false).catch(() => {});
        return resp;
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
        return null;
      }
    },
    [applySnapshotCollections, bridge],
  );

  const replyToTask = useCallback(
    async (taskId: string, answerText: string): Promise<TaskRecord | null> => {
      const trimmed = answerText.trim();
      if (!trimmed) {
        return null;
      }
      setErrorBanner(null);
      try {
        const updated = await bridge.replyToTask(taskId, { answer_text: trimmed });
        setTasks((prev) => prev.map((t) => (t.task_id === taskId ? updated : t)));
        await applySnapshotCollections(undefined, true).catch(() => {});
        return updated;
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
        return null;
      }
    },
    [applySnapshotCollections, bridge],
  );

  const resolvePermissionRequest = useCallback(
    async (
      requestId: string,
      decision: PermissionResolutionDecision,
      expectedRevision?: number | null,
    ) => {
      setErrorBanner(null);
      try {
        await bridge.resolvePermissionRequest(requestId, {
          expected_revision: expectedRevision ?? null,
          decision,
        });
        await applySnapshotCollections(undefined, true);
        if (activeProject && isRightPanelOpen) {
          void loadFilesForProject(activeProject.project_id);
        }
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
      }
    },
    [activeProject, applySnapshotCollections, bridge, isRightPanelOpen, loadFilesForProject],
  );

  const resolveInboxCard = useCallback(
    async (cardId: string, expectedRevision: number, resolution: InboxResolutionAction) => {
      setErrorBanner(null);
      try {
        const resp = await bridge.resolveInboxCard(cardId, {
          expected_revision: expectedRevision,
          resolution,
        });
        if (resolution.action === "reconcile_effect") {
          setReconcileNotice(
            formatReconcileVerdict(resp.effect?.status, resp.card.resolution_summary),
          );
        }
        await applySnapshotCollections(undefined, true);
        if (activeProject && isRightPanelOpen) {
          void loadFilesForProject(activeProject.project_id);
        }
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
      }
    },
    [activeProject, applySnapshotCollections, bridge, isRightPanelOpen, loadFilesForProject],
  );

  const revokeCapabilityGrant = useCallback(
    async (grantId: string, reason?: string) => {
      setErrorBanner(null);
      try {
        await bridge.revokeCapabilityGrant(grantId, {
          reason: reason ?? "Отозвано владельцем в интерфейсе Dennett",
        });
        setActiveGrants((prev) => prev.filter((g) => g.grant_id !== grantId));
        await applySnapshotCollections(undefined, false).catch(() => {});
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
      }
    },
    [applySnapshotCollections, bridge],
  );

  const reconcileEffect = useCallback(
    async (
      effectId: string,
      manualResolution?: ManualEffectResolution | null,
      reason?: string,
    ) => {
      setErrorBanner(null);
      try {
        const resp = await bridge.reconcileEffect(effectId, {
          manual_resolution: manualResolution ?? null,
          reason: reason ?? null,
        });
        setReconcileNotice(
          formatReconcileVerdict(resp.effect.status, resp.resolution_summary),
        );
        await applySnapshotCollections(undefined, true);
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
      }
    },
    [applySnapshotCollections, bridge],
  );

  const flushMemoryOutbox = useCallback(async () => {
    setIsFlushingOutbox(true);
    setLastFlushSummary(null);
    try {
      const resp = await bridge.flushMemoryOutbox();
      setLastFlushSummary(
        `Доставлено: ${resp.delivered}, в очереди: ${resp.pending}, ошибок: ${resp.failed}`,
      );
      const snap = await bridge.getHeadHealth();
      setHealthSnapshot(snap);
      const diag = await bridge.listDiagnostics(20).catch(() => null);
      if (diag) {
        setDiagnostics(diag);
      }
      void refreshMemoryStats();
    } catch (err) {
      setErrorBanner(normalizeBridgeError(err));
    } finally {
      setIsFlushingOutbox(false);
    }
  }, [bridge, refreshMemoryStats]);

  // ============================================================================
  // Phase D3 Actions: Secrets, Providers, Capabilities, Memory Maintenance
  // ============================================================================

  const setSecret = useCallback(
    async (
      slotKey: string,
      value: string,
      description?: string,
    ): Promise<SecretMetadataRecord | null> => {
      setErrorBanner(null);
      try {
        const updated = await bridge.setSecret(slotKey, {
          value,
          scope: "global",
          description: description ?? "",
        });
        setSecrets((prev) => {
          const exists = prev.some((s) => s.slot_key === updated.slot_key);
          return exists
            ? prev.map((s) => (s.slot_key === updated.slot_key ? updated : s))
            : [...prev, updated];
        });
        const connResp = await bridge.listProviderConnections().catch(() => null);
        if (connResp) {
          setProviderConnections(connResp.connections);
          setPlannedProviders(connResp.planned_providers);
        }
        return updated;
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
        return null;
      }
    },
    [bridge],
  );

  const deleteSecret = useCallback(
    async (slotKey: string): Promise<SecretMetadataRecord | null> => {
      setErrorBanner(null);
      try {
        const cleared = await bridge.deleteSecret(slotKey);
        setSecrets((prev) =>
          prev.map((s) => (s.slot_key === cleared.slot_key ? cleared : s)),
        );
        const connResp = await bridge.listProviderConnections().catch(() => null);
        if (connResp) {
          setProviderConnections(connResp.connections);
          setPlannedProviders(connResp.planned_providers);
        }
        return cleared;
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
        return null;
      }
    },
    [bridge],
  );

  const createProviderConnection = useCallback(
    async (
      request: UpsertProviderConnectionRequest,
    ): Promise<ProviderConnectionRecord | null> => {
      setErrorBanner(null);
      try {
        const created = await bridge.createProviderConnection(request);
        setProviderConnections((prev) => [...prev, created]);
        return created;
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
        return null;
      }
    },
    [bridge],
  );

  const updateProviderConnection = useCallback(
    async (
      connectionId: string,
      request: UpsertProviderConnectionRequest,
    ): Promise<ProviderConnectionRecord | null> => {
      setErrorBanner(null);
      try {
        const updated = await bridge.updateProviderConnection(connectionId, request);
        setProviderConnections((prev) =>
          prev.map((c) => (c.connection_id === connectionId ? updated : c)),
        );
        return updated;
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
        return null;
      }
    },
    [bridge],
  );

  const deleteProviderConnection = useCallback(
    async (connectionId: string): Promise<boolean> => {
      setErrorBanner(null);
      try {
        await bridge.deleteProviderConnection(connectionId);
        setProviderConnections((prev) => prev.filter((c) => c.connection_id !== connectionId));
        return true;
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
        return false;
      }
    },
    [bridge],
  );

  const probeProviderConnection = useCallback(
    async (connectionId: string): Promise<ProbeConnectionResponse | null> => {
      setErrorBanner(null);
      try {
        const resp = await bridge.probeProviderConnection(connectionId);
        setProviderConnections((prev) =>
          prev.map((c) => (c.connection_id === connectionId ? resp.connection : c)),
        );
        if (resp.catalog && resp.connection.is_default_interactive) {
          catalogRef.current = resp.catalog;
          setRuntimeCatalog(resp.catalog);
        }
        return resp;
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
        return null;
      }
    },
    [bridge],
  );

  const activateProviderConnection = useCallback(
    async (
      connectionId: string,
      role: ConnectionActivationRole = "interactive",
    ): Promise<ProviderConnectionRecord | null> => {
      setErrorBanner(null);
      try {
        const activated = await bridge.activateProviderConnection(connectionId, { role });
        const listResp = await bridge.listProviderConnections().catch(() => null);
        if (listResp) {
          setProviderConnections(listResp.connections);
          setPlannedProviders(listResp.planned_providers);
        } else {
          setProviderConnections((prev) =>
            prev.map((c) =>
              c.connection_id === connectionId
                ? activated
                : {
                    ...c,
                    is_default_interactive:
                      role === "interactive" || role === "both" ? false : c.is_default_interactive,
                    is_default_background:
                      role === "background" || role === "both" ? false : c.is_default_background,
                  },
            ),
          );
        }
        await refreshRuntimeCatalog(true);
        return activated;
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
        return null;
      }
    },
    [bridge, refreshRuntimeCatalog],
  );

  const updateActiveAgentConnection = useCallback(
    async (
      connectionId: string | null,
      modelPreference: string | null,
      thinkingLevel: ThinkingLevel | null,
    ) => {
      if (!activeAgent) {
        return;
      }
      setIsUpdatingModel(true);
      setErrorBanner(null);
      try {
        const updated = await bridge.updateAgentConfig(activeAgent.agent_id, {
          system_prompt: activeAgent.current_config.system_prompt,
          model_preference: modelPreference,
          thinking_level: thinkingLevel,
          allowed_memory_scopes: activeAgent.current_config.allowed_memory_scopes,
          workspace_write_policy: activeAgent.current_config.workspace_write_policy,
          default_budget: activeAgent.current_config.default_budget ?? null,
          attached_skills: activeAgent.current_config.attached_skills ?? [],
          preferred_connection_id: connectionId,
        });
        agentsRef.current = agentsRef.current.map((a) =>
          a.agent_id === updated.agent_id ? updated : a,
        );
        setAgents((prev) => prev.map((a) => (a.agent_id === updated.agent_id ? updated : a)));
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
      } finally {
        setIsUpdatingModel(false);
      }
    },
    [activeAgent, bridge],
  );

  const discoverCapabilities = useCallback(
    async (
      projectId?: string | null,
      scanPersonalGeminiReadonly = false,
    ): Promise<DiscoverCapabilitiesResponse | null> => {
      setErrorBanner(null);
      try {
        const resp = await bridge.discoverCapabilities({
          project_id: projectId ?? null,
          scan_personal_gemini_readonly: scanPersonalGeminiReadonly,
        });
        const listResp = await bridge.listCapabilities(null, null, null).catch(() => null);
        if (listResp) {
          setCapabilities(listResp.capabilities);
        }
        return resp;
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
        return null;
      }
    },
    [bridge],
  );

  const createCapability = useCallback(
    async (request: CreateCapabilityRequest): Promise<CapabilityRecord | null> => {
      setErrorBanner(null);
      try {
        const created = await bridge.createCapability(request);
        setCapabilities((prev) => [
          created,
          ...prev.filter((c) => c.capability_id !== created.capability_id),
        ]);
        return created;
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
        return null;
      }
    },
    [bridge],
  );

  const updateCapability = useCallback(
    async (
      capabilityId: string,
      request: UpdateCapabilityRequest,
    ): Promise<CapabilityRecord | null> => {
      setErrorBanner(null);
      try {
        const updated = await bridge.updateCapability(capabilityId, request);
        setCapabilities((prev) =>
          prev.map((c) => (c.capability_id === capabilityId ? updated : c)),
        );
        return updated;
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
        return null;
      }
    },
    [bridge],
  );

  const rollbackCapability = useCallback(
    async (capabilityId: string): Promise<CapabilityRecord | null> => {
      setErrorBanner(null);
      try {
        const restored = await bridge.rollbackCapability(capabilityId);
        setCapabilities((prev) =>
          prev.map((c) => (c.capability_id === capabilityId ? restored : c)),
        );
        return restored;
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
        return null;
      }
    },
    [bridge],
  );

  const deleteCapability = useCallback(
    async (capabilityId: string): Promise<boolean> => {
      setErrorBanner(null);
      try {
        await bridge.deleteCapability(capabilityId);
        setCapabilities((prev) => prev.filter((c) => c.capability_id !== capabilityId));
        return true;
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
        return false;
      }
    },
    [bridge],
  );

  const setProjectCapabilityBinding = useCallback(
    async (
      projectId: string,
      capabilityId: string,
      activationState: CapabilityActivation,
    ) => {
      setErrorBanner(null);
      try {
        await bridge.setProjectCapabilityBinding(projectId, capabilityId, {
          activation_state: activationState,
        });
        const listResp = await bridge.listCapabilities(null, null, null).catch(() => null);
        if (listResp) {
          setCapabilities(listResp.capabilities);
        }
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
      }
    },
    [bridge],
  );

  const triggerMemoryIndexRebuild = useCallback(
    async (scope: string): Promise<TriggerIndexRebuildResponse | null> => {
      setIsRebuildingIndex(true);
      setErrorBanner(null);
      try {
        const resp = await bridge.triggerMemoryIndexRebuild({ scope });
        await refreshMemoryOverview(null);
        return resp;
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
        return null;
      } finally {
        setIsRebuildingIndex(false);
      }
    },
    [bridge, refreshMemoryOverview],
  );

  const createMemoryConsolidationJob = useCallback(
    async (
      scope: string,
      jobType: ConsolidationJobType = "dossier_update",
    ): Promise<ConsolidationJob | null> => {
      setIsConsolidatingMemory(true);
      setErrorBanner(null);
      try {
        const job = await bridge.createMemoryConsolidationJob({
          scope,
          job_type: jobType,
          idempotency_key: `ui-cons-${Date.now()}-${Math.random().toString(16).slice(2, 8)}`,
        });
        await refreshMemoryOverview(null);
        return job;
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
        return null;
      } finally {
        setIsConsolidatingMemory(false);
      }
    },
    [bridge, refreshMemoryOverview],
  );

  // Global keyboard shortcuts (Alt+1/2/3, Ctrl+N, Ctrl+B, Ctrl+\, Ctrl+K, Escape for Stop)
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const mod = event.ctrlKey || event.metaKey;
      if (event.altKey && !mod) {
        if (event.key === "1") {
          event.preventDefault();
          setActiveView("chat");
          return;
        }
        if (event.key === "2") {
          event.preventDefault();
          setActiveView("inbox");
          return;
        }
        if (event.key === "3") {
          event.preventDefault();
          setActiveView("tasks");
          return;
        }
        if (event.key === "4") {
          event.preventDefault();
          setActiveView("memory");
          return;
        }
      }

      if (mod && !event.altKey) {
        const lower = event.key.toLowerCase();
        if (lower === "k") {
          event.preventDefault();
          setCommandOpen(!isCommandOpen);
          return;
        }
        if (lower === "n") {
          event.preventDefault();
          if (healthSnapshot?.connected) {
            const currentSess = selectedSessionRef.current
              ? (sessionsRef.current.find((s) => s.session_id === selectedSessionRef.current) ??
                null)
              : null;
            const isViewingStandalone = currentSess !== null && currentSess.project_id === null;
            const hasSelectedActiveProject =
              selectedProjectRef.current !== null &&
              projectsRef.current.some(
                (p) => p.project_id === selectedProjectRef.current && isActiveProjectRecord(p),
              );
            if (isViewingStandalone || !hasSelectedActiveProject) {
              void createStandaloneChat();
            } else {
              void createBlankChat(selectedProjectRef.current ?? undefined);
            }
          }
          return;
        }
        if (lower === "b") {
          event.preventDefault();
          toggleSidebar();
          return;
        }
        if (event.key === "\\") {
          event.preventDefault();
          toggleRightPanel();
          return;
        }
      }

      if (event.key === "Escape") {
        if (event.defaultPrevented) {
          return;
        }
        if (folderPickerIntent) {
          event.preventDefault();
          setFolderPickerIntent(null);
          return;
        }
        if (isCommandOpen) {
          event.preventDefault();
          setCommandOpen(false);
          return;
        }
        if (isSettingsOpen) {
          event.preventDefault();
          setIsSettingsOpen(false);
          return;
        }
        if (isStatsOpen) {
          event.preventDefault();
          setIsStatsOpen(false);
          return;
        }
        const target = event.target as HTMLElement | null;
        if (
          target?.closest(".inline-rename-input, .followup-edit-input, [role='menu'], [role='dialog']")
        ) {
          return;
        }
        const activeSid = selectedSessionRef.current;
        if (
          activeSid &&
          runningSessionIdsRef.current.has(activeSid) &&
          !cancellingSessionIdsRef.current.has(activeSid)
        ) {
          event.preventDefault();
          void stopActiveRun();
        }
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [
    createBlankChat,
    createStandaloneChat,
    folderPickerIntent,
    healthSnapshot?.connected,
    isCommandOpen,
    isSettingsOpen,
    isStatsOpen,
    setActiveView,
    setCommandOpen,
    stopActiveRun,
    toggleRightPanel,
    toggleSidebar,
  ]);

  const standaloneSessions = useMemo(
    () =>
      sessions.filter(
        (s) => s.project_id === null && (s.session_kind ?? "user_chat") !== "task_run",
      ),
    [sessions],
  );
  const isStandaloneActiveSession = Boolean(activeSession && activeSession.project_id === null);

  const selectedArtifact = useMemo(() => {
    if (!activeDetail || !selectedArtifactId) {
      return null;
    }
    const found = activeDetail.artifacts.find((a) => a.artifact_id === selectedArtifactId) ?? null;
    if (!found || (found.lifecycle_state ?? "active") === "removed") {
      return null;
    }
    return found;
  }, [activeDetail, selectedArtifactId]);

  const currentDraft = selectedSessionId ? (draftsBySession[selectedSessionId] ?? "") : "";
  const queuedMessagesForActiveSession = useMemo(
    () =>
      selectedSessionId
        ? queuedMessages.filter((q) => q.session_id === selectedSessionId)
        : [],
    [queuedMessages, selectedSessionId],
  );
  const optimisticMessagesForActiveSession = useMemo(
    () => (selectedSessionId ? (optimisticMessagesBySession[selectedSessionId] ?? []) : []),
    [optimisticMessagesBySession, selectedSessionId],
  );
  const isRunningActiveSession =
    selectedSessionId !== null && runningSessionIds.includes(selectedSessionId);

  // Phase D3+: Poll live streaming text & durable followups (~350ms) ONLY while active session is running and visible.
  useEffect(() => {
    const sessionId = selectedSessionId;
    if (!healthSnapshot?.connected || !sessionId || !isRunningActiveSession || isDocumentHidden) {
      return;
    }

    let cancelled = false;
    let sawActiveLiveRun = false;

    const pollLiveOnce = async () => {
      if (cancelled || livePollInFlightRef.current || selectedSessionRef.current !== sessionId) {
        return;
      }
      livePollInFlightRef.current = true;
      try {
        const resp = await bridge.getSessionLive(sessionId);
        if (cancelled || selectedSessionRef.current !== sessionId) {
          return;
        }
        const nextActive = resp.active ?? null;
        setLiveRun(nextActive);
        setActiveFollowups(
          (resp.followups ?? [])
            .filter((f) => f.status === "queued" || f.status === "paused")
            .sort((a, b) => a.position - b.position),
        );
        if (nextActive) {
          sawActiveLiveRun = true;
        } else if (sawActiveLiveRun) {
          sawActiveLiveRun = false;
          void loadSessionDetail(sessionId);
          void applySnapshotCollections(undefined, false).catch(() => {});
        }
      } catch {
        // Transient live-poll errors are ignored; main cursor poller handles connection truth.
      } finally {
        livePollInFlightRef.current = false;
      }
    };

    void pollLiveOnce();
    const handle = window.setInterval(() => {
      void pollLiveOnce();
    }, 350);

    return () => {
      cancelled = true;
      window.clearInterval(handle);
    };
  }, [
    applySnapshotCollections,
    bridge,
    healthSnapshot?.connected,
    isDocumentHidden,
    isRunningActiveSession,
    loadSessionDetail,
    selectedSessionId,
  ]);

  // Phase D3+: When an active running turn transitions to idle, clear liveRun and refresh once so
  // persisted assistant messages and any auto-dispatched follow-up runs appear with zero duplicate flash.
  useEffect(() => {
    const wasRunning = wasRunningActiveSessionRef.current;
    wasRunningActiveSessionRef.current = isRunningActiveSession;
    if (wasRunning && !isRunningActiveSession) {
      setLiveRun(null);
      const sid = selectedSessionRef.current;
      const alreadyRefreshedLocally = localCompletedSessionRef.current === sid;
      localCompletedSessionRef.current = null;
      const hasQueuedFollowups = activeFollowupsRef.current.some((f) => f.status === "queued");
      if (sid && healthSnapshot?.connected && (!alreadyRefreshedLocally || hasQueuedFollowups)) {
        void loadSessionDetail(sid);
        void applySnapshotCollections(undefined, false).catch(() => {});
      }
    }
  }, [
    applySnapshotCollections,
    healthSnapshot?.connected,
    isRunningActiveSession,
    loadSessionDetail,
  ]);

  const activeLiveRun = useMemo(
    () =>
      liveRun && selectedSessionId && liveRun.session_id === selectedSessionId ? liveRun : null,
    [liveRun, selectedSessionId],
  );
  const activeFollowupsForSession = useMemo(
    () =>
      selectedSessionId
        ? activeFollowups.filter(
            (f) =>
              f.session_id === selectedSessionId &&
              (f.status === "queued" || f.status === "paused"),
          )
        : [],
    [activeFollowups, selectedSessionId],
  );
  const isAwaitingApprovalActiveSession = useMemo(
    () =>
      Boolean(
        selectedSessionId &&
          pendingPermissions.some(
            (req) => req.session_id === selectedSessionId && req.status === "pending",
          ),
      ),
    [pendingPermissions, selectedSessionId],
  );
  const isCancellingActiveSession =
    selectedSessionId !== null && cancellingSessionIds.includes(selectedSessionId);
  const isSidebarEffectiveOpen = isSidebarOpen && activeView === "chat";
  const isActiveProjectArchived = (activeProject?.lifecycle_state ?? "active") === "archived";
  const activeClarificationCard = useMemo(
    () =>
      selectedSessionId
        ? (inboxCards.find(
            (card) =>
              card.session_id === selectedSessionId &&
              card.status === "open" &&
              card.kind === "clarification",
          ) ?? null)
        : null,
    [inboxCards, selectedSessionId],
  );

  const dismissError = useCallback(() => setErrorBanner(null), []);
  const dismissCancelledNotice = useCallback(() => setCancelledNotice(null), []);
  const dismissReconcileNotice = useCallback(() => setReconcileNotice(null), []);

  const openMemoryConflictsCount = globalOpenMemoryConflictsCount;

  const actionsValue = useMemo<WorkspaceActions>(
    () => ({
      setActiveView,
      selectProject,
      openSessionInChat,
      refreshAll,
      pollEventsNow,
      reconnectWithUrl,
      refreshRuntimeCatalog,
      selectSession,
      createBlankChat,
      createBlankChatWithDraft,
      createStandaloneChat,
      toggleProjectPin,
      toggleSessionPin,
      createFolderlessProject,
      createProjectFromFolder,
      requestFolderPicker,
      closeFolderPicker,
      submitFolderPickerPath,
      renameProject,
      attachProjectFolder,
      rebindProjectFolder,
      detachProjectWorkspace,
      archiveProject,
      unarchiveProject,
      removeProject,
      deleteProject,
      renameSession,
      refreshProjectFiles,
      refreshMemoryStats,
      updateDraft,
      sendMessage,
      queueOfflineMessage,
      cancelQueuedMessage,
      flushQueuedMessages,
      stopActiveRun,
      cancelSessionRun,
      enqueueFollowup,
      updateFollowup,
      deleteFollowup,
      clearFollowups,
      resumeFollowups,
      steerSession,
      updateActiveAgentModel,
      updateActiveAgentWritePolicy,
      createArtifact,
      selectArtifact,
      applyArtifactLifecycle,
      createTask,
      updateTaskGoal,
      cancelTask,
      sendTaskSignal,
      replyToTask,
      resolvePermissionRequest,
      resolveInboxCard,
      revokeCapabilityGrant,
      reconcileEffect,
      flushMemoryOutbox,
      refreshMemoryOverview,
      refreshD3SettingsData,
      setSecret,
      deleteSecret,
      createProviderConnection,
      updateProviderConnection,
      deleteProviderConnection,
      probeProviderConnection,
      activateProviderConnection,
      updateActiveAgentConnection,
      discoverCapabilities,
      createCapability,
      updateCapability,
      rollbackCapability,
      deleteCapability,
      setProjectCapabilityBinding,
      triggerMemoryIndexRebuild,
      createMemoryConsolidationJob,
      toggleRightPanel,
      setRightPanelOpen,
      toggleSidebar,
      setSidebarOpen,
      setSettingsOpen: setIsSettingsOpen,
      setStatsOpen: setIsStatsOpen,
      setCommandOpen,
      setSearchQuery,
      dismissError,
      dismissCancelledNotice,
      dismissReconcileNotice,
    }),
    [
      activateProviderConnection,
      applyArtifactLifecycle,
      archiveProject,
      attachProjectFolder,
      cancelQueuedMessage,
      cancelSessionRun,
      cancelTask,
      clearFollowups,
      closeFolderPicker,
      createArtifact,
      createBlankChat,
      createBlankChatWithDraft,
      createCapability,
      createFolderlessProject,
      createMemoryConsolidationJob,
      createProjectFromFolder,
      createProviderConnection,
      createStandaloneChat,
      createTask,
      deleteCapability,
      deleteFollowup,
      deleteProject,
      deleteProviderConnection,
      deleteSecret,
      detachProjectWorkspace,
      discoverCapabilities,
      dismissCancelledNotice,
      dismissError,
      dismissReconcileNotice,
      enqueueFollowup,
      flushMemoryOutbox,
      flushQueuedMessages,
      openSessionInChat,
      pollEventsNow,
      probeProviderConnection,
      queueOfflineMessage,
      rebindProjectFolder,
      reconcileEffect,
      reconnectWithUrl,
      refreshAll,
      refreshD3SettingsData,
      refreshMemoryOverview,
      refreshMemoryStats,
      refreshProjectFiles,
      refreshRuntimeCatalog,
      removeProject,
      renameProject,
      renameSession,
      replyToTask,
      requestFolderPicker,
      resolveInboxCard,
      resolvePermissionRequest,
      resumeFollowups,
      revokeCapabilityGrant,
      rollbackCapability,
      selectArtifact,
      selectProject,
      selectSession,
      sendMessage,
      sendTaskSignal,
      setActiveView,
      setCommandOpen,
      setProjectCapabilityBinding,
      setRightPanelOpen,
      setSecret,
      setSidebarOpen,
      steerSession,
      stopActiveRun,
      submitFolderPickerPath,
      toggleProjectPin,
      toggleRightPanel,
      toggleSessionPin,
      toggleSidebar,
      triggerMemoryIndexRebuild,
      unarchiveProject,
      updateActiveAgentConnection,
      updateActiveAgentModel,
      updateActiveAgentWritePolicy,
      updateCapability,
      updateDraft,
      updateFollowup,
      updateProviderConnection,
      updateTaskGoal,
    ],
  );

  const stateValue = useMemo<WorkspaceState>(
    () => ({
      initialized,
      isLoading,
      isSyncedAfterConnect,
      activeView,
      healthSnapshot,
      runtimeCatalog,
      isRefreshingCatalog,
      streamCursor,
      projects,
      agents,
      sessions,
      tasks,
      inboxCards,
      unresolvedInboxCount,
      pendingPermissions,
      activeGrants,
      effects,
      selectedProjectId,
      selectedSessionId,
      activeDetail,
      projectFiles,
      isLoadingFiles,
      activeScopeNoteCount,
      diagnostics,
      draftsBySession,
      queuedMessages,
      isFlushingQueue,
      optimisticMessagesBySession,
      runningSessionIds,
      cancellingSessionIds,
      stoppingTaskIds,
      cancelledNotice,
      reconcileNotice,
      errorBanner,
      isRightPanelOpen,
      isSidebarOpen,
      isSettingsOpen,
      isStatsOpen,
      isCommandOpen,
      commandInitialQuery,
      folderPickerIntent,
      searchQuery,
      selectedArtifactId,
      artifactRevisions,
      isFlushingOutbox,
      lastFlushSummary,
      isUpdatingModel,
      memoryOverview,
      isLoadingMemoryOverview,
      providerConnections,
      plannedProviders,
      secrets,
      capabilities,
      isLoadingD3Settings,
      isRebuildingIndex,
      isConsolidatingMemory,
    }),
    [
      activeDetail,
      activeGrants,
      activeScopeNoteCount,
      activeView,
      agents,
      artifactRevisions,
      cancelledNotice,
      cancellingSessionIds,
      capabilities,
      commandInitialQuery,
      diagnostics,
      draftsBySession,
      effects,
      errorBanner,
      folderPickerIntent,
      healthSnapshot,
      inboxCards,
      initialized,
      isCommandOpen,
      isConsolidatingMemory,
      isFlushingOutbox,
      isFlushingQueue,
      isLoading,
      isLoadingD3Settings,
      isLoadingFiles,
      isLoadingMemoryOverview,
      isRebuildingIndex,
      isRefreshingCatalog,
      isRightPanelOpen,
      isSettingsOpen,
      isSidebarOpen,
      isStatsOpen,
      isSyncedAfterConnect,
      isUpdatingModel,
      lastFlushSummary,
      memoryOverview,
      optimisticMessagesBySession,
      pendingPermissions,
      plannedProviders,
      projectFiles,
      projects,
      providerConnections,
      queuedMessages,
      reconcileNotice,
      runningSessionIds,
      runtimeCatalog,
      searchQuery,
      secrets,
      selectedArtifactId,
      selectedProjectId,
      selectedSessionId,
      sessions,
      stoppingTaskIds,
      streamCursor,
      tasks,
      unresolvedInboxCount,
    ],
  );

  const metaValue = useMemo<WorkspaceMeta>(
    () => ({
      bridge,
      windowControls,
      searchInputRef,
      composerInputRef,
      activeProject,
      activeSession,
      activeAgent,
      standaloneSessions,
      isStandaloneActiveSession,
      selectedArtifact,
      currentDraft,
      queuedMessagesForActiveSession,
      optimisticMessagesForActiveSession,
      isRunningActiveSession,
      isAwaitingApprovalActiveSession,
      isCancellingActiveSession,
      isSidebarEffectiveOpen,
      isActiveProjectArchived,
      activeClarificationCard,
      activeTaskCount,
      openMemoryConflictsCount,
      activeLiveRun,
      activeFollowups: activeFollowupsForSession,
    }),
    [
      activeAgent,
      activeClarificationCard,
      activeFollowupsForSession,
      activeLiveRun,
      activeProject,
      activeSession,
      activeTaskCount,
      bridge,
      currentDraft,
      isActiveProjectArchived,
      isAwaitingApprovalActiveSession,
      isCancellingActiveSession,
      isRunningActiveSession,
      isSidebarEffectiveOpen,
      isStandaloneActiveSession,
      openMemoryConflictsCount,
      optimisticMessagesForActiveSession,
      queuedMessagesForActiveSession,
      selectedArtifact,
      standaloneSessions,
      windowControls,
    ],
  );

  const value = useMemo<WorkspaceContextValue>(
    () => ({
      state: stateValue,
      actions: actionsValue,
      meta: metaValue,
    }),
    [actionsValue, metaValue, stateValue],
  );

  return <WorkspaceContext value={value}>{children}</WorkspaceContext>;
}
