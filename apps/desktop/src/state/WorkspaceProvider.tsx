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
  type AgentRecord,
  type ArtifactCompleteness,
  type ArtifactLifecycleAction,
  type ArtifactRecord,
  type BridgeErrorPayload,
  type CancelTaskResponse,
  type CapabilityGrantRecord,
  type CreateTaskRequest,
  type DesktopHealthSnapshot,
  type DiagnosticsResponse,
  type EffectClaimRecord,
  type InboxCardRecord,
  type InboxResolutionAction,
  type ListProjectFilesResponse,
  type ManualEffectResolution,
  type MessageRecord,
  type PermissionRequestRecord,
  type PermissionResolutionDecision,
  type ProjectRecord,
  type QueuedOfflineMessage,
  type RuntimeCatalogResponse,
  type SessionDetailResponse,
  type SessionRecord,
  type StreamCursor,
  type TaskRecord,
  type TaskSignalResponse,
  type ThinkingLevel,
  type WorkspaceWritePolicy,
} from "../api/contracts";

export type WorkspaceView = "chat" | "inbox" | "tasks";

export type FolderPickerIntent =
  | { mode: "create-project" }
  | { mode: "attach-project"; projectId: string }
  | { mode: "rebind-project"; projectId: string };

const DRAFTS_STORAGE_KEY = "dennett.desktop.d2.drafts.v1";

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

export interface WorkspaceState {
  initialized: boolean;
  isLoading: boolean;
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
  errorBanner: BridgeErrorPayload | null;
  isRightPanelOpen: boolean;
  isSidebarOpen: boolean;
  isSettingsOpen: boolean;
  isStatsOpen: boolean;
  isCommandOpen: boolean;
  folderPickerIntent: FolderPickerIntent | null;
  searchQuery: string;
  selectedArtifactId: string | null;
  artifactRevisions: ArtifactRecord[];
  isFlushingOutbox: boolean;
  lastFlushSummary: string | null;
  isUpdatingModel: boolean;
}

export interface CreateArtifactInput {
  name: string;
  mimeType: string;
  content: string;
  completenessStatus?: ArtifactCompleteness;
}

export interface WorkspaceActions {
  setActiveView(view: WorkspaceView): void;
  openSessionInChat(sessionId: string): Promise<void>;
  refreshAll(): Promise<void>;
  pollEventsNow(): Promise<void>;
  reconnectWithUrl(headUrl: string): Promise<void>;
  refreshRuntimeCatalog(refresh?: boolean): Promise<void>;
  selectSession(sessionId: string): Promise<void>;
  createBlankChat(projectId?: string): Promise<SessionRecord | null>;
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
  toggleRightPanel(): void;
  setRightPanelOpen(open: boolean): void;
  toggleSidebar(): void;
  setSidebarOpen(open: boolean): void;
  setSettingsOpen(open: boolean): void;
  setStatsOpen(open: boolean): void;
  setCommandOpen(open: boolean): void;
  setSearchQuery(query: string): void;
  dismissError(): void;
  dismissCancelledNotice(): void;
}

export interface WorkspaceMeta {
  bridge: DesktopBridge;
  windowControls: WindowControls;
  searchInputRef: RefObject<HTMLInputElement | null>;
  composerInputRef: RefObject<HTMLTextAreaElement | null>;
  activeProject: ProjectRecord | null;
  activeSession: SessionRecord | null;
  activeAgent: AgentRecord | null;
  selectedArtifact: ArtifactRecord | null;
  currentDraft: string;
  queuedMessagesForActiveSession: QueuedOfflineMessage[];
  optimisticMessagesForActiveSession: MessageRecord[];
  isRunningActiveSession: boolean;
  isCancellingActiveSession: boolean;
  activeTaskCount: number;
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
  const [activeView, setActiveView] = useState<WorkspaceView>("chat");
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
  const [runningSessionIds, setRunningSessionIds] = useState<string[]>([]);
  const [cancellingSessionIds, setCancellingSessionIds] = useState<string[]>([]);
  const [stoppingTaskIds, setStoppingTaskIds] = useState<string[]>([]);
  const [cancelledNotice, setCancelledNotice] = useState<string | null>(null);
  const [errorBanner, setErrorBanner] = useState<BridgeErrorPayload | null>(null);
  const [isRightPanelOpen, setIsRightPanelOpen] = useState(initialRightPanelOpen);
  const [isSidebarOpen, setIsSidebarOpen] = useState(true);
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [isStatsOpen, setIsStatsOpen] = useState(false);
  const [isCommandOpen, setIsCommandOpen] = useState(false);
  const [folderPickerIntent, setFolderPickerIntent] = useState<FolderPickerIntent | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedArtifactId, setSelectedArtifactId] = useState<string | null>(null);
  const [artifactRevisions, setArtifactRevisions] = useState<ArtifactRecord[]>([]);
  const [isFlushingOutbox, setIsFlushingOutbox] = useState(false);
  const [lastFlushSummary, setLastFlushSummary] = useState<string | null>(null);
  const [isUpdatingModel, setIsUpdatingModel] = useState(false);

  const searchInputRef = useRef<HTMLInputElement | null>(null);
  const composerInputRef = useRef<HTMLTextAreaElement | null>(null);
  const selectedSessionRef = useRef<string | null>(null);
  const selectedArtifactIdRef = useRef<string | null>(null);
  const projectsRef = useRef<ProjectRecord[]>([]);
  const agentsRef = useRef<AgentRecord[]>([]);
  const sessionsRef = useRef<SessionRecord[]>([]);
  const catalogRef = useRef<RuntimeCatalogResponse | null>(null);
  const streamCursorRef = useRef<StreamCursor | null>(null);
  const queuedMessagesRef = useRef<QueuedOfflineMessage[]>(initialPersisted.queuedMessages);
  const isFlushingQueueRef = useRef(false);
  const wasConnectedRef = useRef(false);
  const pollInFlightRef = useRef(false);
  const runningSessionIdsRef = useRef<Set<string>>(new Set());
  const cancellingSessionIdsRef = useRef<Set<string>>(new Set());
  const detailRequestSeqRef = useRef(0);
  const filesRequestSeqRef = useRef(0);
  const activeViewRef = useRef<WorkspaceView>("chat");

  selectedSessionRef.current = selectedSessionId;
  selectedArtifactIdRef.current = selectedArtifactId;
  projectsRef.current = projects;
  agentsRef.current = agents;
  sessionsRef.current = sessions;
  catalogRef.current = runtimeCatalog;
  queuedMessagesRef.current = queuedMessages;
  runningSessionIdsRef.current = new Set(runningSessionIds);
  cancellingSessionIdsRef.current = new Set(cancellingSessionIds);
  activeViewRef.current = activeView;

  // Persist drafts and offline queue to localStorage cleanly.
  useEffect(() => {
    writePersistedDrafts({ draftsBySession, queuedMessages });
  }, [draftsBySession, queuedMessages]);

  const focusComposerSoon = useCallback(() => {
    requestAnimationFrame(() => {
      composerInputRef.current?.focus();
    });
  }, []);

  const setSelectedSessionSync = useCallback((nextSessionId: string | null) => {
    selectedSessionRef.current = nextSessionId;
    setSelectedSessionId(nextSessionId);
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
        setSessions((prev) =>
          prev.map((s) => (s.session_id === detail.session.session_id ? detail.session : s)),
        );
        if (seq !== detailRequestSeqRef.current || selectedSessionRef.current !== sessionId) {
          return;
        }
        setActiveDetail(detail);
        const visibleArtifacts = detail.artifacts.filter(
          (a) => (a.lifecycle_state ?? "active") !== "removed",
        );
        const currentSelectedId = selectedArtifactIdRef.current;
        const stillVisible = currentSelectedId
          ? (visibleArtifacts.find((a) => a.artifact_id === currentSelectedId) ?? null)
          : null;
        const chosenArtifact = stillVisible ?? visibleArtifacts[visibleArtifacts.length - 1] ?? null;
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

  const applySnapshotCollections = useCallback(
    async (preferredSessionId?: string | null, reloadActiveDetail = false) => {
      const [snap, permResp, effResp] = await Promise.all([
        bridge.getStateSnapshot(null, true),
        bridge.listPermissionRequests("pending", null).catch(() => ({ requests: [] })),
        bridge.listEffects(null, null, null).catch(() => ({ effects: [] })),
      ]);

      streamCursorRef.current = snap.snapshot_cursor;
      setStreamCursor(snap.snapshot_cursor);
      projectsRef.current = snap.projects;
      agentsRef.current = snap.agents;
      sessionsRef.current = snap.sessions;
      const activeProjectIds = new Set(
        snap.projects
          .filter((p) => {
            const lc = p.lifecycle_state ?? "active";
            return lc !== "deleted_record" && lc !== "archived";
          })
          .map((p) => p.project_id),
      );
      const visibleSessions = snap.sessions.filter((s) => activeProjectIds.has(s.project_id));

      setProjects(snap.projects);
      setAgents(snap.agents);
      setSessions(snap.sessions);
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

      const targetSessionId =
        preferredSessionId ??
        (selectedSessionRef.current &&
        visibleSessions.some((s) => s.session_id === selectedSessionRef.current)
          ? selectedSessionRef.current
          : (visibleSessions[0]?.session_id ?? null));

      const changedSession = targetSessionId !== selectedSessionRef.current;
      setSelectedSessionSync(targetSessionId);
      if (targetSessionId && (reloadActiveDetail || changedSession)) {
        await loadSessionDetail(targetSessionId);
      } else if (!targetSessionId) {
        setActiveDetail(null);
        selectedArtifactIdRef.current = null;
        setSelectedArtifactId(null);
        setArtifactRevisions([]);
      }
    },
    [bridge, loadSessionDetail, setSelectedSessionSync],
  );

  const loadConnectedState = useCallback(
    async (preferredSessionId?: string | null) => {
      await applySnapshotCollections(preferredSessionId, true);
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
      } else if (sessionsRef.current.length === 0) {
        setActiveDetail(null);
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
    const isActiveProject = (p: ProjectRecord) => {
      const lc = p.lifecycle_state ?? "active";
      return lc !== "archived" && lc !== "deleted_record";
    };
    if (activeSession) {
      const matched = projects.find((p) => p.project_id === activeSession.project_id);
      if (matched && isActiveProject(matched)) {
        return matched;
      }
    }
    return projects.find(isActiveProject) ?? null;
  }, [activeSession, projects]);

  const activeAgent = useMemo(
    () =>
      activeSession
        ? (agents.find((a) => a.agent_id === activeSession.agent_id) ?? null)
        : (agents[0] ?? null),
    [activeSession, agents],
  );

  const pollEventsNow = useCallback(async () => {
    if (pollInFlightRef.current || !streamCursorRef.current) {
      return;
    }
    pollInFlightRef.current = true;
    try {
      let cursor = streamCursorRef.current;
      let sawEvents = false;
      let shouldReloadSessionDetail = false;
      let shouldRefreshFiles = false;
      const activeSid = selectedSessionRef.current;

      for (let page = 0; page < 5; page += 1) {
        const resp = await bridge.getEvents(cursor.committed_seq, cursor.authority_epoch, null, 100);
        if (resp.next_cursor > cursor.committed_seq) {
          cursor = {
            authority_epoch: resp.authority_epoch,
            committed_seq: resp.next_cursor,
            retained_from_seq: resp.retained_from_seq,
          };
          streamCursorRef.current = cursor;
          setStreamCursor(cursor);
        }
        if (resp.events.length > 0) {
          sawEvents = true;
          for (const ev of resp.events) {
            if (
              (activeSid && ev.session_id === activeSid) ||
              ev.event_type.startsWith("permission.") ||
              ev.event_type.startsWith("artifact.") ||
              ev.event_type.startsWith("effect.")
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
        if (shouldRefreshFiles && activeProject && isRightPanelOpen) {
          void loadFilesForProject(activeProject.project_id);
        }
      } else if (runningSessionIdsRef.current.size > 0) {
        // Also fetch pending permissions while a live turn is in flight so inline approval banners appear immediately.
        const permResp = await bridge.listPermissionRequests("pending", null).catch(() => null);
        if (permResp) {
          setPendingPermissions(permResp.requests);
        }
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
      projects
        .filter((p) => {
          const lc = p.lifecycle_state ?? "active";
          return lc !== "archived" && lc !== "deleted_record";
        })
        .map((p) => p.project_id),
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

  // Cursor-driven live sync interval (700ms when active work / approvals exist, 2500ms when idle).
  useEffect(() => {
    if (!healthSnapshot?.connected || !streamCursor) {
      return;
    }
    const isBusy =
      runningSessionIds.length > 0 ||
      activeTaskCount > 0 ||
      unresolvedInboxCount > 0 ||
      pendingPermissions.length > 0;
    const intervalMs = isBusy ? 700 : 2500;
    const handle = window.setInterval(() => {
      void pollEventsNow();
    }, intervalMs);
    return () => window.clearInterval(handle);
  }, [
    activeTaskCount,
    healthSnapshot?.connected,
    pendingPermissions.length,
    pollEventsNow,
    runningSessionIds.length,
    streamCursor,
    unresolvedInboxCount,
  ]);

  // Automatic gentle reconnect check while Head is offline after initial load.
  useEffect(() => {
    if (!initialized || healthSnapshot?.connected || sessions.length === 0) {
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
  }, [bridge, healthSnapshot?.connected, initialized, loadConnectedState, sessions.length]);

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

  const refreshMemoryStats = useCallback(async () => {
    if (!healthSnapshot?.connected || !activeProject) {
      setActiveScopeNoteCount(null);
      return;
    }
    try {
      const resp = await bridge.searchMemory({
        scope: activeProject.memory_scope,
        limit: 100,
      });
      if (resp.memory_status === "healthy") {
        setActiveScopeNoteCount(resp.notes.length);
      } else {
        setActiveScopeNoteCount(null);
      }
    } catch {
      setActiveScopeNoteCount(null);
    }
  }, [activeProject, bridge, healthSnapshot?.connected]);

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
        }
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
      } finally {
        setIsLoading(false);
      }
    },
    [bridge, loadConnectedState],
  );

  const selectSession = useCallback(
    async (sessionId: string) => {
      setActiveView("chat");
      setSelectedSessionSync(sessionId);
      setCancelledNotice(null);
      await loadSessionDetail(sessionId);
    },
    [loadSessionDetail, setSelectedSessionSync],
  );

  const openSessionInChat = useCallback(
    async (sessionId: string) => {
      await selectSession(sessionId);
    },
    [selectSession],
  );

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

      const label = projectName?.trim() || "Проект";
      const cat = catalogRef.current;
      const created = await bridge.createAgent({
        name: `Агент · ${label}`.slice(0, 120),
        description: `Агент Dennett для области памяти ${memoryScope}`,
        system_prompt: DEFAULT_SYSTEM_PROMPT,
        model_preference: cat?.default_model_id ?? null,
        thinking_level: cat?.default_thinking_level ?? "medium",
        allowed_memory_scopes: [memoryScope],
        workspace_write_policy: hasWorkspace ? { mode: "allow_workspace" } : { mode: "deny_all" },
      });
      agentsRef.current = [created, ...agentsRef.current];
      setAgents((prev) => [created, ...prev]);
      return created;
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
      projectsRef.current = [project, ...projectsRef.current];
      setProjects((prev) => [project, ...prev]);
      const agent = await ensureScopedAgent(project.memory_scope, project.name, false);
      const session = await bridge.createSession({
        agent_id: agent.agent_id,
        project_id: project.project_id,
        title: "Новый чат",
      });
      setSessions((prev) => [session, ...prev]);
      setActiveView("chat");
      setSelectedSessionSync(session.session_id);
      setCancelledNotice(null);
      await loadSessionDetail(session.session_id);
      focusComposerSoon();
      return project;
    } catch (err) {
      setErrorBanner(normalizeBridgeError(err));
      return null;
    }
  }, [bridge, ensureScopedAgent, focusComposerSoon, loadSessionDetail, setSelectedSessionSync]);

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
        projectsRef.current = [project, ...projectsRef.current];
        setProjects((prev) => [project, ...prev]);
        const agent = await ensureScopedAgent(project.memory_scope, project.name, true);
        const session = await bridge.createSession({
          agent_id: agent.agent_id,
          project_id: project.project_id,
          title: "Новый чат",
        });
        setSessions((prev) => [session, ...prev]);
        setActiveView("chat");
        setSelectedSessionSync(session.session_id);
        setCancelledNotice(null);
        await loadSessionDetail(session.session_id);
        void loadFilesForProject(project.project_id);
        focusComposerSoon();
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
      setSelectedSessionSync,
    ],
  );

  const createBlankChat = useCallback(
    async (projectId?: string): Promise<SessionRecord | null> => {
      setErrorBanner(null);
      const resolvedProjectId =
        projectId ??
        activeProject?.project_id ??
        projectsRef.current.find((p) => (p.lifecycle_state ?? "active") !== "archived")?.project_id;
      if (!resolvedProjectId) {
        await createFolderlessProject();
        return null;
      }
      try {
        const project = projectsRef.current.find((p) => p.project_id === resolvedProjectId);
        const memoryScope = project?.memory_scope ?? "project:default";
        const agent = await ensureScopedAgent(
          memoryScope,
          project?.name,
          Boolean(project?.workspace_path),
        );
        const created = await bridge.createSession({
          agent_id: agent.agent_id,
          project_id: resolvedProjectId,
          title: "Новый чат",
        });
        setSessions((prev) => [created, ...prev]);
        setActiveView("chat");
        setSelectedSessionSync(created.session_id);
        setCancelledNotice(null);
        await loadSessionDetail(created.session_id);
        focusComposerSoon();
        return created;
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
        return null;
      }
    },
    [
      activeProject?.project_id,
      bridge,
      createFolderlessProject,
      ensureScopedAgent,
      focusComposerSoon,
      loadSessionDetail,
      setSelectedSessionSync,
    ],
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
        setSessions((prev) => prev.map((s) => (s.session_id === sessionId ? updated : s)));
        setActiveDetail((prev) =>
          prev && prev.session.session_id === sessionId ? { ...prev, session: updated } : prev,
        );
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
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
    [attachProjectFolder, bridge, createProjectFromFolder, rebindProjectFolder],
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
      } else {
        await attachProjectFolder(intent.projectId, folderPath);
      }
    },
    [attachProjectFolder, createProjectFromFolder, folderPickerIntent, rebindProjectFolder],
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
      project_id: sess?.project_id ?? "",
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
      runningSessionIdsRef.current.add(sessionId);
      setRunningSessionIds((prev) => (prev.includes(sessionId) ? prev : [...prev, sessionId]));

      // Trigger rapid permission checks right after turn starts so interactive ask_before_write cards appear immediately.
      const quickPollA = window.setTimeout(() => void pollEventsNow(), 180);
      const quickPollB = window.setTimeout(() => void pollEventsNow(), 500);

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

        setSessions((prev) =>
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
          if (response.run.status === "cancelled") {
            setCancelledNotice("Запуск остановлен пользователем (Cancelled).");
          } else if (response.run.status === "failed" && response.run.error_summary) {
            setErrorBanner({
              code: response.run.error_code ?? "RUN_FAILED",
              message: response.run.error_summary,
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
        return true;
      } catch (err) {
        removeOptimistic();
        throw err;
      } finally {
        window.clearTimeout(quickPollA);
        window.clearTimeout(quickPollB);
        runningSessionIdsRef.current.delete(sessionId);
        cancellingSessionIdsRef.current.delete(sessionId);
        setRunningSessionIds((prev) => prev.filter((id) => id !== sessionId));
        setCancellingSessionIds((prev) => prev.filter((id) => id !== sessionId));
      }
    },
    [applySnapshotCollections, bridge, loadSessionDetail, pollEventsNow],
  );

  const flushQueuedMessages = useCallback(async () => {
    if (isFlushingQueueRef.current || queuedMessagesRef.current.length === 0) {
      return;
    }
    isFlushingQueueRef.current = true;
    setIsFlushingQueue(true);
    try {
      const snapshotQueue = [...queuedMessagesRef.current];
      for (const item of snapshotQueue) {
        const proj = projectsRef.current.find((p) => p.project_id === item.project_id);
        if (proj && (proj.lifecycle_state === "archived" || proj.lifecycle_state === "deleted_record")) {
          setQueuedMessages((prev) =>
            prev.filter((q) => q.client_message_id !== item.client_message_id),
          );
          setErrorBanner({
            code: "PROJECT_ARCHIVED",
            message: `Отложенное сообщение не отправлено: проект «${proj.name}» в архиве или удалён.`,
          });
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
          setErrorBanner(normalized);
          break;
        }
      }
    } finally {
      isFlushingQueueRef.current = false;
      setIsFlushingQueue(false);
    }
  }, [dispatchMessageToHead]);

  // Auto-flush queued offline messages once when Head reconnects!
  useEffect(() => {
    const isNowConnected = Boolean(healthSnapshot?.connected);
    const justReconnected = !wasConnectedRef.current && isNowConnected;
    wasConnectedRef.current = isNowConnected;
    if (justReconnected && queuedMessagesRef.current.length > 0) {
      void flushQueuedMessages();
    }
  }, [flushQueuedMessages, healthSnapshot?.connected]);

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

    // Immediately clear draft so live turns never appear stuck.
    setDraftsBySession((prev) => ({ ...prev, [sessionId]: "" }));
    const stableClientMsgId = generateClientMessageId();

    try {
      await dispatchMessageToHead(sessionId, content, stableClientMsgId);
    } catch (err) {
      const normalized = normalizeBridgeError(err);
      if (normalized.code === "HEAD_UNREACHABLE") {
        // Head dropped mid-send -> queue idempotently with SAME client_message_id so reconnect sends at most once!
        const sess = sessionsRef.current.find((s) => s.session_id === sessionId);
        setQueuedMessages((prev) => [
          ...prev,
          {
            client_message_id: stableClientMsgId,
            session_id: sessionId,
            project_id: sess?.project_id ?? "",
            content,
            created_at: Date.now(),
          },
        ]);
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
        // Restore the sent prompt losslessly if the user has not typed a new draft in the meantime.
        setDraftsBySession((prev) => {
          const existing = prev[sessionId] ?? "";
          return existing.length === 0 ? { ...prev, [sessionId]: rawDraft } : prev;
        });
        setErrorBanner(normalized);
      }
    }
  }, [dispatchMessageToHead, draftsBySession, healthSnapshot?.connected, queueOfflineMessage]);

  const cancelSessionRun = useCallback(
    async (targetSessionId: string) => {
      if (
        !targetSessionId ||
        !runningSessionIdsRef.current.has(targetSessionId) ||
        cancellingSessionIdsRef.current.has(targetSessionId)
      ) {
        return;
      }
      cancellingSessionIdsRef.current.add(targetSessionId);
      setCancellingSessionIds((prev) =>
        prev.includes(targetSessionId) ? prev : [...prev, targetSessionId],
      );
      try {
        const resp = await bridge.cancelRun(targetSessionId);
        if (selectedSessionRef.current === targetSessionId) {
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
        await applySnapshotCollections(undefined, false).catch(() => {});
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
        setIsRightPanelOpen(true);
        return created;
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
        return null;
      }
    },
    [bridge, loadRevisionsForArtifact, loadSessionDetail],
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
          idempotency_key: idempotencyKey?.trim() || `ui-sig-${Date.now()}-${Math.random().toString(16).slice(2, 8)}`,
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
        await applySnapshotCollections(undefined, false).catch(() => {});
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
        await bridge.resolveInboxCard(cardId, {
          expected_revision: expectedRevision,
          resolution,
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
        await bridge.reconcileEffect(effectId, {
          manual_resolution: manualResolution ?? null,
          reason: reason ?? null,
        });
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
      }

      if (mod && !event.altKey) {
        const lower = event.key.toLowerCase();
        if (lower === "k") {
          event.preventDefault();
          setIsCommandOpen((prev) => !prev);
          return;
        }
        if (lower === "n") {
          event.preventDefault();
          if (healthSnapshot?.connected) {
            void createBlankChat();
          }
          return;
        }
        if (lower === "b") {
          event.preventDefault();
          setIsSidebarOpen((prev) => !prev);
          return;
        }
        if (event.key === "\\") {
          event.preventDefault();
          setIsRightPanelOpen((prev) => !prev);
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
          setIsCommandOpen(false);
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
        if (target?.closest(".inline-rename-input, [role='menu'], [role='dialog']")) {
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
    folderPickerIntent,
    healthSnapshot?.connected,
    isCommandOpen,
    isSettingsOpen,
    isStatsOpen,
    stopActiveRun,
  ]);

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
  const isCancellingActiveSession =
    selectedSessionId !== null && cancellingSessionIds.includes(selectedSessionId);

  const value = useMemo<WorkspaceContextValue>(
    () => ({
      state: {
        initialized,
        isLoading,
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
        errorBanner,
        isRightPanelOpen,
        isSidebarOpen,
        isSettingsOpen,
        isStatsOpen,
        isCommandOpen,
        folderPickerIntent,
        searchQuery,
        selectedArtifactId,
        artifactRevisions,
        isFlushingOutbox,
        lastFlushSummary,
        isUpdatingModel,
      },
      actions: {
        setActiveView,
        openSessionInChat,
        refreshAll,
        pollEventsNow,
        reconnectWithUrl,
        refreshRuntimeCatalog,
        selectSession,
        createBlankChat,
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
        toggleRightPanel: () => setIsRightPanelOpen((prev) => !prev),
        setRightPanelOpen: setIsRightPanelOpen,
        toggleSidebar: () => setIsSidebarOpen((prev) => !prev),
        setSidebarOpen: setIsSidebarOpen,
        setSettingsOpen: setIsSettingsOpen,
        setStatsOpen: setIsStatsOpen,
        setCommandOpen: setIsCommandOpen,
        setSearchQuery,
        dismissError: () => setErrorBanner(null),
        dismissCancelledNotice: () => setCancelledNotice(null),
      },
      meta: {
        bridge,
        windowControls,
        searchInputRef,
        composerInputRef,
        activeProject,
        activeSession,
        activeAgent,
        selectedArtifact,
        currentDraft,
        queuedMessagesForActiveSession,
        optimisticMessagesForActiveSession,
        isRunningActiveSession,
        isCancellingActiveSession,
        activeTaskCount,
      },
    }),
    [
      activeAgent,
      activeDetail,
      activeGrants,
      activeProject,
      activeScopeNoteCount,
      activeSession,
      activeTaskCount,
      activeView,
      agents,
      applyArtifactLifecycle,
      archiveProject,
      artifactRevisions,
      attachProjectFolder,
      bridge,
      cancelQueuedMessage,
      cancelSessionRun,
      cancelTask,
      cancelledNotice,
      cancellingSessionIds,
      closeFolderPicker,
      createArtifact,
      createBlankChat,
      createFolderlessProject,
      createProjectFromFolder,
      createTask,
      currentDraft,
      deleteProject,
      detachProjectWorkspace,
      diagnostics,
      draftsBySession,
      effects,
      errorBanner,
      flushMemoryOutbox,
      flushQueuedMessages,
      folderPickerIntent,
      healthSnapshot,
      inboxCards,
      initialized,
      isCancellingActiveSession,
      isCommandOpen,
      isFlushingOutbox,
      isFlushingQueue,
      isLoading,
      isLoadingFiles,
      isRefreshingCatalog,
      isRightPanelOpen,
      isRunningActiveSession,
      isSettingsOpen,
      isSidebarOpen,
      isStatsOpen,
      isUpdatingModel,
      lastFlushSummary,
      openSessionInChat,
      optimisticMessagesBySession,
      optimisticMessagesForActiveSession,
      pendingPermissions,
      pollEventsNow,
      projectFiles,
      projects,
      queueOfflineMessage,
      queuedMessages,
      queuedMessagesForActiveSession,
      rebindProjectFolder,
      reconcileEffect,
      reconnectWithUrl,
      refreshAll,
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
      revokeCapabilityGrant,
      runningSessionIds,
      runtimeCatalog,
      searchQuery,
      selectArtifact,
      selectSession,
      selectedArtifact,
      selectedArtifactId,
      selectedSessionId,
      sendMessage,
      sendTaskSignal,
      sessions,
      stopActiveRun,
      stoppingTaskIds,
      streamCursor,
      submitFolderPickerPath,
      tasks,
      unarchiveProject,
      unresolvedInboxCount,
      updateActiveAgentModel,
      updateActiveAgentWritePolicy,
      updateDraft,
      updateTaskGoal,
      windowControls,
    ],
  );

  return <WorkspaceContext value={value}>{children}</WorkspaceContext>;
}
