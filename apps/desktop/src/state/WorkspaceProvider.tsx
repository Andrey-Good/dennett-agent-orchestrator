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
  normalizeBridgeError,
  type AgentRecord,
  type ArtifactRecord,
  type BridgeErrorPayload,
  type DesktopHealthSnapshot,
  type DiagnosticsResponse,
  type ProjectRecord,
  type SessionDetailResponse,
  type SessionRecord,
  type ThinkingLevel,
} from "../api/contracts";

export interface WorkspaceState {
  initialized: boolean;
  isLoading: boolean;
  healthSnapshot: DesktopHealthSnapshot | null;
  projects: ProjectRecord[];
  agents: AgentRecord[];
  sessions: SessionRecord[];
  selectedSessionId: string | null;
  activeDetail: SessionDetailResponse | null;
  diagnostics: DiagnosticsResponse | null;
  draftsBySession: Record<string, string>;
  runningSessionId: string | null;
  cancellingSessionId: string | null;
  cancelledNotice: string | null;
  errorBanner: BridgeErrorPayload | null;
  isRightPanelOpen: boolean;
  isSidebarOpen: boolean;
  searchQuery: string;
  selectedArtifactId: string | null;
  isFlushingOutbox: boolean;
  lastFlushSummary: string | null;
  isUpdatingModel: boolean;
}

export interface CreateProjectInput {
  name: string;
  workspacePath: string;
  memoryScope: string;
}

export interface QuickStartInput extends CreateProjectInput {
  sessionTitle: string;
}

export interface CreateArtifactInput {
  name: string;
  mimeType: string;
  content: string;
}

export interface WorkspaceActions {
  refreshAll(): Promise<void>;
  reconnectWithUrl(headUrl: string): Promise<void>;
  selectSession(sessionId: string): Promise<void>;
  createProject(input: CreateProjectInput): Promise<ProjectRecord | null>;
  createQuickStart(input: QuickStartInput): Promise<void>;
  createSession(projectId: string, title: string): Promise<SessionRecord | null>;
  updateDraft(sessionId: string, value: string): void;
  sendMessage(): Promise<void>;
  stopActiveRun(): Promise<void>;
  updateActiveAgentModel(
    modelPreference: string | null,
    thinkingLevel: ThinkingLevel | null,
  ): Promise<void>;
  createArtifact(input: CreateArtifactInput): Promise<ArtifactRecord | null>;
  selectArtifact(artifactId: string): Promise<void>;
  flushMemoryOutbox(): Promise<void>;
  toggleRightPanel(): void;
  setRightPanelOpen(open: boolean): void;
  toggleSidebar(): void;
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
  isRunningActiveSession: boolean;
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

export function WorkspaceProvider({
  bridge,
  windowControls,
  initialRightPanelOpen = false,
  children,
}: WorkspaceProviderProps) {
  const [initialized, setInitialized] = useState(false);
  const [isLoading, setIsLoading] = useState(true);
  const [healthSnapshot, setHealthSnapshot] = useState<DesktopHealthSnapshot | null>(null);
  const [projects, setProjects] = useState<ProjectRecord[]>([]);
  const [agents, setAgents] = useState<AgentRecord[]>([]);
  const [sessions, setSessions] = useState<SessionRecord[]>([]);
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);
  const [activeDetail, setActiveDetail] = useState<SessionDetailResponse | null>(null);
  const [diagnostics, setDiagnostics] = useState<DiagnosticsResponse | null>(null);
  const [draftsBySession, setDraftsBySession] = useState<Record<string, string>>({});
  const [runningSessionId, setRunningSessionId] = useState<string | null>(null);
  const [cancellingSessionId, setCancellingSessionId] = useState<string | null>(null);
  const [cancelledNotice, setCancelledNotice] = useState<string | null>(null);
  const [errorBanner, setErrorBanner] = useState<BridgeErrorPayload | null>(null);
  const [isRightPanelOpen, setIsRightPanelOpen] = useState(initialRightPanelOpen);
  const [isSidebarOpen, setIsSidebarOpen] = useState(true);
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedArtifactId, setSelectedArtifactId] = useState<string | null>(null);
  const [isFlushingOutbox, setIsFlushingOutbox] = useState(false);
  const [lastFlushSummary, setLastFlushSummary] = useState<string | null>(null);
  const [isUpdatingModel, setIsUpdatingModel] = useState(false);

  const searchInputRef = useRef<HTMLInputElement | null>(null);
  const composerInputRef = useRef<HTMLTextAreaElement | null>(null);
  const selectedSessionRef = useRef<string | null>(null);
  const projectsRef = useRef<ProjectRecord[]>([]);
  const agentsRef = useRef<AgentRecord[]>([]);
  const detailRequestSeqRef = useRef(0);
  selectedSessionRef.current = selectedSessionId;
  projectsRef.current = projects;
  agentsRef.current = agents;

  const setSelectedSessionSync = useCallback((nextSessionId: string | null) => {
    selectedSessionRef.current = nextSessionId;
    setSelectedSessionId(nextSessionId);
  }, []);

  const loadSessionDetail = useCallback(
    async (sessionId: string) => {
      const seq = ++detailRequestSeqRef.current;
      try {
        const detail = await bridge.getSessionDetail(sessionId);
        setSessions((prev) =>
          prev.map((s) => (s.session_id === detail.session.session_id ? detail.session : s)),
        );
        // Guard against stale async resolution or cross-session switch race (Architecture 83 §8):
        // never overwrite the active timeline/artifacts if another session is now selected.
        if (seq !== detailRequestSeqRef.current || selectedSessionRef.current !== sessionId) {
          return;
        }
        setActiveDetail(detail);
        setSelectedArtifactId((prev) => {
          if (prev && detail.artifacts.some((a) => a.artifact_id === prev)) {
            return prev;
          }
          return detail.artifacts[detail.artifacts.length - 1]?.artifact_id ?? null;
        });
      } catch (err) {
        if (seq === detailRequestSeqRef.current && selectedSessionRef.current === sessionId) {
          setErrorBanner(normalizeBridgeError(err));
        }
      }
    },
    [bridge],
  );

  const loadConnectedState = useCallback(
    async (preferredSessionId?: string | null) => {
      const [projResp, agentResp, sessResp, diagResp] = await Promise.all([
        bridge.listProjects(),
        bridge.listAgents(),
        bridge.listSessions(),
        bridge.listDiagnostics(20).catch(() => null),
      ]);

      projectsRef.current = projResp.projects;
      agentsRef.current = agentResp.agents;
      setProjects(projResp.projects);
      setAgents(agentResp.agents);
      setSessions(sessResp.sessions);
      if (diagResp) {
        setDiagnostics(diagResp);
      }

      const targetSessionId =
        preferredSessionId ??
        (selectedSessionRef.current &&
        sessResp.sessions.some((s) => s.session_id === selectedSessionRef.current)
          ? selectedSessionRef.current
          : (sessResp.sessions[0]?.session_id ?? null));

      setSelectedSessionSync(targetSessionId);
      if (targetSessionId) {
        await loadSessionDetail(targetSessionId);
      } else {
        setActiveDetail(null);
        setSelectedArtifactId(null);
      }
    },
    [bridge, loadSessionDetail, setSelectedSessionSync],
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
      setSelectedSessionSync(sessionId);
      setCancelledNotice(null);
      await loadSessionDetail(sessionId);
    },
    [loadSessionDetail, setSelectedSessionSync],
  );

  const ensureScopedAgent = useCallback(
    async (memoryScope: string, projectName?: string): Promise<AgentRecord> => {
      // Strictly match an existing agent dedicated solely to `[memoryScope]` to prevent
      // cross-project memory lockout and foreign scope bleed (TEST_CATALOGUE §3.7).
      const exactMatch = agentsRef.current.find(
        (agent) =>
          agent.current_config.allowed_memory_scopes.length === 1 &&
          agent.current_config.allowed_memory_scopes[0] === memoryScope,
      );
      if (exactMatch) {
        return exactMatch;
      }

      const label = projectName?.trim() || memoryScope;
      const created = await bridge.createAgent({
        name: `Агент · ${label}`.slice(0, 120),
        description: `Агент Dennett D1 для области памяти ${memoryScope}`,
        system_prompt: DEFAULT_SYSTEM_PROMPT,
        model_preference: null,
        thinking_level: "medium",
        allowed_memory_scopes: [memoryScope],
        workspace_write_policy: { mode: "ask_before_write" },
      });
      agentsRef.current = [created, ...agentsRef.current];
      setAgents((prev) => [created, ...prev]);
      return created;
    },
    [bridge],
  );

  const createProject = useCallback(
    async (input: CreateProjectInput): Promise<ProjectRecord | null> => {
      setErrorBanner(null);
      try {
        const created = await bridge.createProject({
          name: input.name.trim(),
          workspace_path: input.workspacePath.trim(),
          memory_scope: input.memoryScope.trim(),
        });
        projectsRef.current = [created, ...projectsRef.current];
        setProjects((prev) => [created, ...prev]);
        return created;
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
        return null;
      }
    },
    [bridge],
  );

  const createSession = useCallback(
    async (projectId: string, title: string): Promise<SessionRecord | null> => {
      setErrorBanner(null);
      try {
        const project = projectsRef.current.find((p) => p.project_id === projectId);
        const memoryScope = project?.memory_scope ?? "project:default";
        const agent = await ensureScopedAgent(memoryScope, project?.name);
        const created = await bridge.createSession({
          agent_id: agent.agent_id,
          project_id: projectId,
          title: title.trim() || "Новый разговор",
        });
        setSessions((prev) => [created, ...prev]);
        setSelectedSessionSync(created.session_id);
        setCancelledNotice(null);
        await loadSessionDetail(created.session_id);
        return created;
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
        return null;
      }
    },
    [bridge, ensureScopedAgent, loadSessionDetail, setSelectedSessionSync],
  );

  const createQuickStart = useCallback(
    async (input: QuickStartInput) => {
      setErrorBanner(null);
      try {
        const project = await bridge.createProject({
          name: input.name.trim(),
          workspace_path: input.workspacePath.trim(),
          memory_scope: input.memoryScope.trim(),
        });
        projectsRef.current = [project, ...projectsRef.current];
        setProjects((prev) => [project, ...prev]);
        const agent = await ensureScopedAgent(project.memory_scope, project.name);
        const session = await bridge.createSession({
          agent_id: agent.agent_id,
          project_id: project.project_id,
          title: input.sessionTitle.trim() || "Первый разговор",
        });
        setSessions((prev) => [session, ...prev]);
        setSelectedSessionSync(session.session_id);
        setCancelledNotice(null);
        await loadSessionDetail(session.session_id);
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
      }
    },
    [bridge, ensureScopedAgent, loadSessionDetail, setSelectedSessionSync],
  );

  const updateDraft = useCallback((sessionId: string, value: string) => {
    setDraftsBySession((prev) => ({ ...prev, [sessionId]: value }));
  }, []);

  const sendMessage = useCallback(async () => {
    const sessionId = selectedSessionRef.current;
    if (!sessionId || runningSessionId === sessionId) {
      return;
    }
    const rawDraft = draftsBySession[sessionId] ?? "";
    const content = rawDraft.trim();
    if (!content) {
      return;
    }

    setErrorBanner(null);
    setCancelledNotice(null);
    setRunningSessionId(sessionId);

    try {
      const response = await bridge.sendMessage(sessionId, { content });
      // Clear draft ONLY after Head has persisted/accepted the turn.
      setDraftsBySession((prev) => ({ ...prev, [sessionId]: "" }));

      // Update session list metadata (binding & updated_at) without touching another active view.
      setSessions((prev) =>
        prev.map((s) =>
          s.session_id === sessionId
            ? {
                ...s,
                runtime_binding: response.runtime_binding,
                updated_at: response.run.finished_at ?? response.run.created_at,
              }
            : s,
        ),
      );

      // Sync health snapshot memory_status honestly with latest turn observation.
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

      // Only update session-scoped notices and active detail if the user is still viewing `sessionId`.
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

      const diag = await bridge.listDiagnostics(20).catch(() => null);
      if (diag) {
        setDiagnostics(diag);
      }
    } catch (err) {
      // Lossless draft invariant: unsent draft stays intact in draftsBySession!
      setErrorBanner(normalizeBridgeError(err));
    } finally {
      setRunningSessionId((curr) => (curr === sessionId ? null : curr));
      setCancellingSessionId((curr) => (curr === sessionId ? null : curr));
    }
  }, [bridge, draftsBySession, loadSessionDetail, runningSessionId]);

  const stopActiveRun = useCallback(async () => {
    const targetSessionId = runningSessionId ?? selectedSessionRef.current;
    if (!targetSessionId) {
      return;
    }
    setCancellingSessionId(targetSessionId);
    try {
      const resp = await bridge.cancelRun(targetSessionId);
      setCancelledNotice(
        resp.cancelled
          ? "Запуск остановлен (Cancelled). Новые действия отменены."
          : "Команда остановки отправлена в dennett-head.",
      );
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
    } catch (err) {
      setErrorBanner(normalizeBridgeError(err));
    } finally {
      setCancellingSessionId((curr) => (curr === targetSessionId ? null : curr));
    }
  }, [bridge, runningSessionId]);

  const activeSession = useMemo(
    () => sessions.find((s) => s.session_id === selectedSessionId) ?? activeDetail?.session ?? null,
    [activeDetail?.session, selectedSessionId, sessions],
  );

  const activeProject = useMemo(
    () =>
      activeSession
        ? (projects.find((p) => p.project_id === activeSession.project_id) ?? null)
        : null,
    [activeSession, projects],
  );

  const activeAgent = useMemo(
    () =>
      activeSession
        ? (agents.find((a) => a.agent_id === activeSession.agent_id) ?? null)
        : (agents[0] ?? null),
    [activeSession, agents],
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
        });
        await loadSessionDetail(sessionId);
        setSelectedArtifactId(created.artifact_id);
        setIsRightPanelOpen(true);
        return created;
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
        return null;
      }
    },
    [bridge, loadSessionDetail],
  );

  const selectArtifact = useCallback(
    async (artifactId: string) => {
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
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
      }
    },
    [bridge],
  );

  const flushMemoryOutbox = useCallback(async () => {
    setIsFlushingOutbox(true);
    setErrorBanner(null);
    try {
      const result = await bridge.flushMemoryOutbox();
      setLastFlushSummary(
        `Попытка доставки: отправлено ${result.delivered}, в очереди ${result.pending}`,
      );
      const snap = await bridge.getHeadHealth();
      setHealthSnapshot(snap);
      const diag = await bridge.listDiagnostics(20).catch(() => null);
      if (diag) {
        setDiagnostics(diag);
      }
    } catch (err) {
      setErrorBanner(normalizeBridgeError(err));
    } finally {
      setIsFlushingOutbox(false);
    }
  }, [bridge]);

  // Global keyboard shortcuts: Ctrl+K focuses search, Escape triggers Stop when running
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        searchInputRef.current?.focus();
      } else if (event.key === "Escape" && runningSessionId !== null) {
        event.preventDefault();
        void stopActiveRun();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [runningSessionId, stopActiveRun]);

  const selectedArtifact = useMemo(
    () =>
      activeDetail?.artifacts.find((a) => a.artifact_id === selectedArtifactId) ??
      activeDetail?.artifacts[activeDetail.artifacts.length - 1] ??
      null,
    [activeDetail?.artifacts, selectedArtifactId],
  );

  const currentDraft = selectedSessionId ? (draftsBySession[selectedSessionId] ?? "") : "";
  const isRunningActiveSession =
    selectedSessionId !== null && runningSessionId === selectedSessionId;

  const value = useMemo<WorkspaceContextValue>(
    () => ({
      state: {
        initialized,
        isLoading,
        healthSnapshot,
        projects,
        agents,
        sessions,
        selectedSessionId,
        activeDetail,
        diagnostics,
        draftsBySession,
        runningSessionId,
        cancellingSessionId,
        cancelledNotice,
        errorBanner,
        isRightPanelOpen,
        isSidebarOpen,
        searchQuery,
        selectedArtifactId,
        isFlushingOutbox,
        lastFlushSummary,
        isUpdatingModel,
      },
      actions: {
        refreshAll,
        reconnectWithUrl,
        selectSession,
        createProject,
        createQuickStart,
        createSession,
        updateDraft,
        sendMessage,
        stopActiveRun,
        updateActiveAgentModel,
        createArtifact,
        selectArtifact,
        flushMemoryOutbox,
        toggleRightPanel: () => setIsRightPanelOpen((v) => !v),
        setRightPanelOpen: setIsRightPanelOpen,
        toggleSidebar: () => setIsSidebarOpen((v) => !v),
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
        isRunningActiveSession,
      },
    }),
    [
      initialized,
      isLoading,
      healthSnapshot,
      projects,
      agents,
      sessions,
      selectedSessionId,
      activeDetail,
      diagnostics,
      draftsBySession,
      runningSessionId,
      cancellingSessionId,
      cancelledNotice,
      errorBanner,
      isRightPanelOpen,
      isSidebarOpen,
      searchQuery,
      selectedArtifactId,
      isFlushingOutbox,
      lastFlushSummary,
      isUpdatingModel,
      refreshAll,
      reconnectWithUrl,
      selectSession,
      createProject,
      createQuickStart,
      createSession,
      updateDraft,
      sendMessage,
      stopActiveRun,
      updateActiveAgentModel,
      createArtifact,
      selectArtifact,
      flushMemoryOutbox,
      bridge,
      windowControls,
      activeProject,
      activeSession,
      activeAgent,
      selectedArtifact,
      currentDraft,
      isRunningActiveSession,
    ],
  );

  return <WorkspaceContext value={value}>{children}</WorkspaceContext>;
}
