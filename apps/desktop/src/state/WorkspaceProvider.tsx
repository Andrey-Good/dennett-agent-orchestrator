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
  type ListProjectFilesResponse,
  type MessageRecord,
  type ProjectRecord,
  type RuntimeCatalogResponse,
  type SessionDetailResponse,
  type SessionRecord,
  type ThinkingLevel,
  type WorkspaceWritePolicy,
} from "../api/contracts";

export type FolderPickerIntent =
  | { mode: "create-project" }
  | { mode: "attach-project"; projectId: string };

export interface WorkspaceState {
  initialized: boolean;
  isLoading: boolean;
  healthSnapshot: DesktopHealthSnapshot | null;
  runtimeCatalog: RuntimeCatalogResponse | null;
  isRefreshingCatalog: boolean;
  projects: ProjectRecord[];
  agents: AgentRecord[];
  sessions: SessionRecord[];
  selectedSessionId: string | null;
  activeDetail: SessionDetailResponse | null;
  projectFiles: ListProjectFilesResponse | null;
  isLoadingFiles: boolean;
  activeScopeNoteCount: number | null;
  diagnostics: DiagnosticsResponse | null;
  draftsBySession: Record<string, string>;
  optimisticMessagesBySession: Record<string, MessageRecord[]>;
  runningSessionIds: string[];
  cancellingSessionIds: string[];
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
  isFlushingOutbox: boolean;
  lastFlushSummary: string | null;
  isUpdatingModel: boolean;
}

export interface CreateArtifactInput {
  name: string;
  mimeType: string;
  content: string;
}

export interface WorkspaceActions {
  refreshAll(): Promise<void>;
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
  renameSession(sessionId: string, title: string): Promise<void>;
  refreshProjectFiles(): Promise<void>;
  refreshMemoryStats(): Promise<void>;
  updateDraft(sessionId: string, value: string): void;
  sendMessage(): Promise<void>;
  stopActiveRun(): Promise<void>;
  updateActiveAgentModel(
    modelPreference: string | null,
    thinkingLevel: ThinkingLevel | null,
  ): Promise<void>;
  updateActiveAgentWritePolicy(policy: WorkspaceWritePolicy): Promise<void>;
  createArtifact(input: CreateArtifactInput): Promise<ArtifactRecord | null>;
  selectArtifact(artifactId: string): Promise<void>;
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
  optimisticMessagesForActiveSession: MessageRecord[];
  isRunningActiveSession: boolean;
  isCancellingActiveSession: boolean;
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
  const [initialized, setInitialized] = useState(false);
  const [isLoading, setIsLoading] = useState(true);
  const [healthSnapshot, setHealthSnapshot] = useState<DesktopHealthSnapshot | null>(null);
  const [runtimeCatalog, setRuntimeCatalog] = useState<RuntimeCatalogResponse | null>(null);
  const [isRefreshingCatalog, setIsRefreshingCatalog] = useState(false);
  const [projects, setProjects] = useState<ProjectRecord[]>([]);
  const [agents, setAgents] = useState<AgentRecord[]>([]);
  const [sessions, setSessions] = useState<SessionRecord[]>([]);
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);
  const [activeDetail, setActiveDetail] = useState<SessionDetailResponse | null>(null);
  const [projectFiles, setProjectFiles] = useState<ListProjectFilesResponse | null>(null);
  const [isLoadingFiles, setIsLoadingFiles] = useState(false);
  const [activeScopeNoteCount, setActiveScopeNoteCount] = useState<number | null>(null);
  const [diagnostics, setDiagnostics] = useState<DiagnosticsResponse | null>(null);
  const [draftsBySession, setDraftsBySession] = useState<Record<string, string>>({});
  const [optimisticMessagesBySession, setOptimisticMessagesBySession] = useState<
    Record<string, MessageRecord[]>
  >({});
  const [runningSessionIds, setRunningSessionIds] = useState<string[]>([]);
  const [cancellingSessionIds, setCancellingSessionIds] = useState<string[]>([]);
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
  const [isFlushingOutbox, setIsFlushingOutbox] = useState(false);
  const [lastFlushSummary, setLastFlushSummary] = useState<string | null>(null);
  const [isUpdatingModel, setIsUpdatingModel] = useState(false);

  const searchInputRef = useRef<HTMLInputElement | null>(null);
  const composerInputRef = useRef<HTMLTextAreaElement | null>(null);
  const selectedSessionRef = useRef<string | null>(null);
  const projectsRef = useRef<ProjectRecord[]>([]);
  const agentsRef = useRef<AgentRecord[]>([]);
  const catalogRef = useRef<RuntimeCatalogResponse | null>(null);
  const runningSessionIdsRef = useRef<Set<string>>(new Set());
  const cancellingSessionIdsRef = useRef<Set<string>>(new Set());
  const detailRequestSeqRef = useRef(0);
  const filesRequestSeqRef = useRef(0);

  selectedSessionRef.current = selectedSessionId;
  projectsRef.current = projects;
  agentsRef.current = agents;
  catalogRef.current = runtimeCatalog;
  runningSessionIdsRef.current = new Set(runningSessionIds);
  cancellingSessionIdsRef.current = new Set(cancellingSessionIds);

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

  const loadConnectedState = useCallback(
    async (preferredSessionId?: string | null) => {
      const [projResp, agentResp, sessResp, diagResp, catResp] = await Promise.all([
        bridge.listProjects(),
        bridge.listAgents(),
        bridge.listSessions(),
        bridge.listDiagnostics(20).catch(() => null),
        bridge.getRuntimeCatalog(false).catch(() => null),
      ]);

      projectsRef.current = projResp.projects;
      agentsRef.current = agentResp.agents;
      setProjects(projResp.projects);
      setAgents(agentResp.agents);
      setSessions(sessResp.sessions);
      if (diagResp) {
        setDiagnostics(diagResp);
      }
      if (catResp) {
        catalogRef.current = catResp;
        setRuntimeCatalog(catResp);
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

  const activeSession = useMemo(
    () => sessions.find((s) => s.session_id === selectedSessionId) ?? activeDetail?.session ?? null,
    [activeDetail?.session, selectedSessionId, sessions],
  );

  const activeProject = useMemo(
    () =>
      activeSession
        ? (projects.find((p) => p.project_id === activeSession.project_id) ?? null)
        : (projects[0] ?? null),
    [activeSession, projects],
  );

  const activeAgent = useMemo(
    () =>
      activeSession
        ? (agents.find((a) => a.agent_id === activeSession.agent_id) ?? null)
        : (agents[0] ?? null),
    [activeSession, agents],
  );

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
      setSelectedSessionSync(sessionId);
      setCancelledNotice(null);
      await loadSessionDetail(sessionId);
    },
    [loadSessionDetail, setSelectedSessionSync],
  );

  const ensureScopedAgent = useCallback(
    async (
      memoryScope: string,
      projectName?: string,
      hasWorkspace = false,
    ): Promise<AgentRecord> => {
      // Strictly match an existing agent dedicated solely to `[memoryScope]` to preserve
      // cross-project memory isolation.
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
        projectId ?? activeProject?.project_id ?? projectsRef.current[0]?.project_id;
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

        // Upgrade the dedicated folderless `deny_all` agent to `allow_workspace` upon first folder bind
        // so the user can actually work with workspace files without silent policy denials in D1.
        if (wasFolderless && updated.workspace_path) {
          const scopedAgent = agentsRef.current.find(
            (a) =>
              a.current_config.allowed_memory_scopes.length === 1 &&
              a.current_config.allowed_memory_scopes[0] === updated.memory_scope,
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
        }

        if (activeProject?.project_id === projectId) {
          await loadFilesForProject(projectId);
        }
      } catch (err) {
        setErrorBanner(normalizeBridgeError(err));
      }
    },
    [activeProject?.project_id, bridge, loadFilesForProject],
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
    [attachProjectFolder, bridge, createProjectFromFolder],
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
      } else {
        await attachProjectFolder(intent.projectId, folderPath);
      }
    },
    [attachProjectFolder, createProjectFromFolder, folderPickerIntent],
  );

  const updateDraft = useCallback((sessionId: string, value: string) => {
    setDraftsBySession((prev) => ({ ...prev, [sessionId]: value }));
  }, []);

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

    const optimisticId = `optimistic-${sessionId}-${Date.now()}`;
    const optimisticMessage: MessageRecord = {
      message_id: optimisticId,
      session_id: sessionId,
      role: "user",
      content,
      run_id: null,
      correlation_id: "pending",
      created_at: Date.now(),
    };

    setErrorBanner(null);
    setCancelledNotice(null);
    // Immediately clear draft & show optimistic user bubble so live turns never appear stuck.
    setDraftsBySession((prev) => ({ ...prev, [sessionId]: "" }));
    setOptimisticMessagesBySession((prev) => ({
      ...prev,
      [sessionId]: [...(prev[sessionId] ?? []), optimisticMessage],
    }));
    runningSessionIdsRef.current.add(sessionId);
    setRunningSessionIds((prev) => (prev.includes(sessionId) ? prev : [...prev, sessionId]));

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
      const response = await bridge.sendMessage(sessionId, { content });

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

      const diag = await bridge.listDiagnostics(20).catch(() => null);
      if (diag) {
        setDiagnostics(diag);
      }
    } catch (err) {
      removeOptimistic();
      // Restore the sent prompt losslessly if the user has not typed a new draft in the meantime.
      setDraftsBySession((prev) => {
        const existing = prev[sessionId] ?? "";
        return existing.length === 0 ? { ...prev, [sessionId]: rawDraft } : prev;
      });
      setErrorBanner(normalizeBridgeError(err));
    } finally {
      runningSessionIdsRef.current.delete(sessionId);
      cancellingSessionIdsRef.current.delete(sessionId);
      setRunningSessionIds((prev) => prev.filter((id) => id !== sessionId));
      setCancellingSessionIds((prev) => prev.filter((id) => id !== sessionId));
    }
  }, [bridge, draftsBySession, loadSessionDetail]);

  const stopActiveRun = useCallback(async () => {
    const targetSessionId = selectedSessionRef.current;
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
          resp.cancelled
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
    } catch (err) {
      setErrorBanner(normalizeBridgeError(err));
    } finally {
      cancellingSessionIdsRef.current.delete(targetSessionId);
      setCancellingSessionIds((prev) => prev.filter((id) => id !== targetSessionId));
    }
  }, [bridge]);

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

  // Global keyboard shortcuts (Ctrl+N, Ctrl+B, Ctrl+\, Ctrl+K, Escape for Stop)
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const mod = event.ctrlKey || event.metaKey;
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
    return activeDetail.artifacts.find((a) => a.artifact_id === selectedArtifactId) ?? null;
  }, [activeDetail, selectedArtifactId]);

  const currentDraft = selectedSessionId ? (draftsBySession[selectedSessionId] ?? "") : "";
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
        healthSnapshot,
        runtimeCatalog,
        isRefreshingCatalog,
        projects,
        agents,
        sessions,
        selectedSessionId,
        activeDetail,
        projectFiles,
        isLoadingFiles,
        activeScopeNoteCount,
        diagnostics,
        draftsBySession,
        optimisticMessagesBySession,
        runningSessionIds,
        cancellingSessionIds,
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
        isFlushingOutbox,
        lastFlushSummary,
        isUpdatingModel,
      },
      actions: {
        refreshAll,
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
        renameSession,
        refreshProjectFiles,
        refreshMemoryStats,
        updateDraft,
        sendMessage,
        stopActiveRun,
        updateActiveAgentModel,
        updateActiveAgentWritePolicy,
        createArtifact,
        selectArtifact,
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
        optimisticMessagesForActiveSession,
        isRunningActiveSession,
        isCancellingActiveSession,
      },
    }),
    [
      activeAgent,
      activeDetail,
      activeProject,
      activeScopeNoteCount,
      activeSession,
      agents,
      attachProjectFolder,
      bridge,
      cancelledNotice,
      cancellingSessionIds,
      closeFolderPicker,
      createArtifact,
      createBlankChat,
      createFolderlessProject,
      createProjectFromFolder,
      currentDraft,
      diagnostics,
      draftsBySession,
      errorBanner,
      flushMemoryOutbox,
      folderPickerIntent,
      healthSnapshot,
      initialized,
      isCancellingActiveSession,
      isCommandOpen,
      isFlushingOutbox,
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
      optimisticMessagesBySession,
      optimisticMessagesForActiveSession,
      projectFiles,
      projects,
      reconnectWithUrl,
      refreshAll,
      refreshMemoryStats,
      refreshProjectFiles,
      refreshRuntimeCatalog,
      renameProject,
      renameSession,
      requestFolderPicker,
      runningSessionIds,
      runtimeCatalog,
      searchQuery,
      selectArtifact,
      selectSession,
      selectedArtifact,
      selectedArtifactId,
      selectedSessionId,
      sendMessage,
      sessions,
      stopActiveRun,
      submitFolderPickerPath,
      updateActiveAgentModel,
      updateActiveAgentWritePolicy,
      updateDraft,
      windowControls,
    ],
  );

  return <WorkspaceContext value={value}>{children}</WorkspaceContext>;
}
