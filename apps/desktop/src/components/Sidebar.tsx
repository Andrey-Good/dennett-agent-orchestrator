import { useMemo, useRef, useState } from "react";
import {
  Archive,
  ArrowCounterClockwise,
  CaretDown,
  CaretRight,
  DotsThree,
  DownloadSimple,
  EyeSlash,
  FolderDashed,
  FolderOpen,
  FolderPlus,
  FolderSimple,
  LinkBreak,
  PencilSimple,
  Plus,
  PushPin,
  Trash,
  X,
} from "@phosphor-icons/react";

import type { ProjectRecord, SessionRecord } from "../api/contracts";
import { useWorkspace } from "../state/WorkspaceProvider";
import { PortalPopover } from "./PortalPopover";
import { useModalFocusTrap } from "./useModalFocusTrap";

const COLLAPSED_PROJECTS_KEY = "dennett.desktop.collapsedProjects.v1";

function loadCollapsedProjects(): Record<string, boolean> {
  if (typeof window === "undefined") {
    return {};
  }
  try {
    const raw = window.localStorage.getItem(COLLAPSED_PROJECTS_KEY);
    if (!raw) {
      return {};
    }
    const parsed = JSON.parse(raw) as unknown;
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
      return parsed as Record<string, boolean>;
    }
  } catch {
    // Ignore corrupt localStorage entries.
  }
  return {};
}

function formatRelativeTime(timestampMs: number): string {
  const diffSec = Math.max(0, Math.floor((Date.now() - timestampMs) / 1000));
  if (diffSec < 60) {
    return "сейчас";
  }
  const diffMin = Math.floor(diffSec / 60);
  if (diffMin < 60) {
    return `${diffMin}м`;
  }
  const diffHours = Math.floor(diffMin / 60);
  if (diffHours < 24) {
    return `${diffHours}ч`;
  }
  const diffDays = Math.floor(diffHours / 24);
  if (diffDays < 7) {
    return `${diffDays}д`;
  }
  return `${Math.floor(diffDays / 7)}н`;
}

type PendingProjectConfirm = {
  project: ProjectRecord;
  mode: "remove" | "delete";
};

function sanitizeDownloadFilename(name: string): string {
  const cleaned = name
    .trim()
    .replace(/[\\/:*?"<>|]+/g, "-")
    .replace(/\s+/g, "-")
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "");
  return cleaned || "dennett-chat";
}

function triggerMarkdownDownload(filename: string, content: string): void {
  if (typeof URL === "undefined" || typeof URL.createObjectURL !== "function") {
    return;
  }
  const blob = new Blob([content], { type: "text/markdown;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
  URL.revokeObjectURL(url);
}

export function Sidebar() {
  const { state, actions, meta } = useWorkspace();
  const [createMenuOpen, setCreateMenuOpen] = useState(false);
  const [openProjectMenu, setOpenProjectMenu] = useState<{
    project: ProjectRecord;
    isArchived: boolean;
    anchorEl: HTMLElement;
  } | null>(null);
  const [openSessionMenu, setOpenSessionMenu] = useState<{
    session: SessionRecord;
    anchorEl: HTMLElement;
  } | null>(null);
  const [confirmDeleteSessionId, setConfirmDeleteSessionId] = useState<string | null>(null);
  const [collapsedProjects, setCollapsedProjects] =
    useState<Record<string, boolean>>(loadCollapsedProjects);
  const [showArchivedChats, setShowArchivedChats] = useState(false);
  const [archiveExpanded, setArchiveExpanded] = useState(false);
  const [pendingConfirm, setPendingConfirm] = useState<PendingProjectConfirm | null>(null);
  const [editingSessionId, setEditingSessionId] = useState<string | null>(null);
  const [editingSessionTitle, setEditingSessionTitle] = useState("");
  const [editingProjectId, setEditingProjectId] = useState<string | null>(null);
  const [editingProjectName, setEditingProjectName] = useState("");

  const createTriggerRef = useRef<HTMLButtonElement | null>(null);
  const confirmDialogRef = useRef<HTMLDivElement | null>(null);

  useModalFocusTrap(confirmDialogRef, pendingConfirm !== null, () => setPendingConfirm(null));

  const query = state.searchQuery.trim().toLowerCase();

  const toggleProjectCollapsed = (projectId: string) => {
    setCollapsedProjects((prev) => {
      const next = { ...prev, [projectId]: !prev[projectId] };
      try {
        window.localStorage.setItem(COLLAPSED_PROJECTS_KEY, JSON.stringify(next));
      } catch {
        // Non-fatal in private / restricted storage environments.
      }
      return next;
    });
  };

  const archivedUserChatsCount = useMemo(() => {
    return state.sessions.filter(
      (s) => (s.session_kind ?? "user_chat") === "user_chat" && s.status === "archived",
    ).length;
  }, [state.sessions]);

  const { activeGroups, archivedGroups } = useMemo(() => {
    const active: Array<{ project: ProjectRecord; sessions: SessionRecord[] }> = [];
    const archived: Array<{ project: ProjectRecord; sessions: SessionRecord[] }> = [];

    for (const project of state.projects) {
      const lifecycle = project.lifecycle_state ?? "active";
      if (lifecycle === "deleted_record") {
        continue;
      }
      const projectMatches =
        !query ||
        project.name.toLowerCase().includes(query) ||
        (project.workspace_path?.toLowerCase().includes(query) ?? false);

      // D2-UX-03: Hide background `task_run` sessions from user chat list unless explicitly opened!
      // Also hide archived user chats unless toggle is enabled or chat is currently opened.
      const projectSessions = state.sessions.filter((session) => {
        if (session.project_id !== project.project_id) {
          return false;
        }
        const isUserChat = (session.session_kind ?? "user_chat") === "user_chat";
        const isCurrentlyOpened = session.session_id === state.selectedSessionId;
        if (!isUserChat && !isCurrentlyOpened) {
          return false;
        }
        if (
          lifecycle !== "archived" &&
          session.status === "archived" &&
          !showArchivedChats &&
          !isCurrentlyOpened
        ) {
          return false;
        }
        return !query || projectMatches || session.title.toLowerCase().includes(query);
      });

      if (lifecycle === "archived") {
        if (projectMatches || projectSessions.length > 0) {
          archived.push({ project, sessions: projectSessions });
        }
        continue;
      }

      if (projectMatches || projectSessions.length > 0) {
        active.push({ project, sessions: projectSessions });
      }
    }

    return { activeGroups: active, archivedGroups: archived };
  }, [query, showArchivedChats, state.projects, state.selectedSessionId, state.sessions]);

  const standaloneChats = useMemo(
    () =>
      meta.standaloneSessions.filter((session) => {
        const isCurrentlyOpened = session.session_id === state.selectedSessionId;
        if (session.status === "archived" && !showArchivedChats && !isCurrentlyOpened) {
          return false;
        }
        return !query || session.title.toLowerCase().includes(query);
      }),
    [meta.standaloneSessions, query, showArchivedChats, state.selectedSessionId],
  );

  const startSessionRename = (session: SessionRecord) => {
    setOpenSessionMenu(null);
    setConfirmDeleteSessionId(null);
    setEditingProjectId(null);
    setEditingSessionId(session.session_id);
    setEditingSessionTitle(session.title);
  };

  const commitSessionRename = (session: SessionRecord) => {
    const next = editingSessionTitle.trim();
    setEditingSessionId(null);
    if (next && next !== session.title) {
      void actions.renameSession(session.session_id, next);
    }
  };

  const handleExportSessionMarkdown = async (session: SessionRecord) => {
    setOpenSessionMenu(null);
    setConfirmDeleteSessionId(null);
    try {
      const detail = await meta.bridge.getSessionDetail(session.session_id);
      const lines: string[] = [
        `# ${detail.session.title}`,
        "",
        `Экспортировано: ${new Date().toLocaleString("ru-RU")}`,
        "",
      ];
      for (const msg of detail.messages) {
        const roleLabel =
          msg.role === "user"
            ? "Пользователь"
            : msg.role === "assistant"
              ? "Агент"
              : "Система";
        lines.push(`## ${roleLabel}`);
        lines.push("");
        lines.push(msg.content);
        lines.push("");
      }
      triggerMarkdownDownload(
        `${sanitizeDownloadFilename(detail.session.title)}.md`,
        lines.join("\n"),
      );
    } catch {
      // Errors surface through bridge/UI handlers if needed.
    }
  };

  const startProjectRename = (project: ProjectRecord) => {
    setEditingSessionId(null);
    setOpenProjectMenu(null);
    setEditingProjectId(project.project_id);
    setEditingProjectName(project.name);
  };

  const commitProjectRename = (project: ProjectRecord) => {
    const next = editingProjectName.trim();
    setEditingProjectId(null);
    if (next && next !== project.name) {
      void actions.renameProject(project.project_id, next);
    }
  };

  const isConnected = Boolean(state.healthSnapshot?.connected);

  const toggleProjectMenu = (
    project: ProjectRecord,
    isArchived: boolean,
    triggerEl: HTMLElement,
  ) => {
    setOpenSessionMenu(null);
    setConfirmDeleteSessionId(null);
    setOpenProjectMenu((prev) =>
      prev && prev.project.project_id === project.project_id
        ? null
        : { project, isArchived, anchorEl: triggerEl },
    );
  };

  const toggleSessionMenu = (session: SessionRecord, triggerEl: HTMLElement) => {
    setOpenProjectMenu(null);
    setConfirmDeleteSessionId(null);
    setOpenSessionMenu((prev) =>
      prev && prev.session.session_id === session.session_id
        ? null
        : { session, anchorEl: triggerEl },
    );
  };

  const renderChatRow = (session: SessionRecord) => {
    const isActive =
      state.activeView === "chat" && state.selectedSessionId === session.session_id;
    const isRunning = state.runningSessionIds.includes(session.session_id);
    const isEditingSess = editingSessionId === session.session_id;
    const isTaskRun = (session.session_kind ?? "user_chat") === "task_run";
    const isArchivedChat = session.status === "archived";
    const isPinned = Boolean(session.pinned);
    const isMenuOpen = openSessionMenu?.session.session_id === session.session_id;

    return (
      <div
        key={session.session_id}
        className={`chat-row-wrap ${isActive ? "is-active" : ""} ${
          isRunning ? "is-running" : ""
        } ${isPinned ? "is-pinned" : ""} ${isMenuOpen ? "has-menu-open" : ""}`}
        role="listitem"
      >
        {isEditingSess ? (
          <input
            type="text"
            className="inline-rename-input inline-rename-input--chat"
            value={editingSessionTitle}
            autoFocus
            aria-label="Новое название чата"
            data-testid="inline-session-rename-input"
            onFocus={(e) => {
              e.currentTarget.scrollLeft = 0;
              e.currentTarget.select();
            }}
            onChange={(e) => setEditingSessionTitle(e.target.value)}
            onBlur={() => commitSessionRename(session)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                commitSessionRename(session);
              } else if (e.key === "Escape") {
                e.preventDefault();
                e.stopPropagation();
                setEditingSessionId(null);
              }
            }}
          />
        ) : (
          <>
            <button
              type="button"
              className={`chat-row ${isActive ? "is-active" : ""}`}
              onClick={() => void actions.selectSession(session.session_id)}
              onDoubleClick={() => startSessionRename(session)}
              onKeyDown={(e) => {
                if (e.key === "F2") {
                  e.preventDefault();
                  startSessionRename(session);
                }
              }}
              aria-current={isActive ? "page" : undefined}
              data-testid={`session-item-${session.session_id}`}
            >
              <span className="chat-row__title-wrap">
                {isRunning && (
                  <span
                    className="chat-row__running-dot"
                    aria-label="Чат выполняется"
                    data-testid={`session-running-dot-${session.session_id}`}
                  />
                )}
                <span className="chat-row__title">{session.title}</span>
                {isTaskRun && (
                  <span
                    className="chat-row__kind-badge"
                    data-testid={`session-kind-badge-${session.session_id}`}
                  >
                    Фон
                  </span>
                )}
                {isArchivedChat && (
                  <span
                    className="chat-row__kind-badge"
                    data-testid={`session-archived-badge-${session.session_id}`}
                  >
                    Архив
                  </span>
                )}
              </span>
              <small className="chat-row__meta">
                {isRunning ? "в работе" : formatRelativeTime(session.updated_at)}
              </small>
            </button>
            <div className="chat-row__actions">
              <button
                type="button"
                className="chat-row__rename-btn"
                onClick={(e) => {
                  e.stopPropagation();
                  startSessionRename(session);
                }}
                aria-label={`Переименовать чат ${session.title}`}
                title="Переименовать чат (F2)"
              >
                <PencilSimple size={12} />
              </button>
              <button
                type="button"
                className={`chat-row__pin-btn ${isPinned ? "is-pinned" : ""}`}
                disabled={!isConnected}
                onClick={(e) => {
                  e.stopPropagation();
                  void actions.toggleSessionPin(session.session_id);
                }}
                aria-label={
                  isPinned
                    ? `Открепить чат ${session.title}`
                    : `Закрепить чат ${session.title}`
                }
                aria-pressed={isPinned}
                data-testid={`session-pin-btn-${session.session_id}`}
                title={isPinned ? "Открепить чат" : "Закрепить чат"}
              >
                <PushPin size={12} weight={isPinned ? "fill" : "regular"} />
              </button>
              <button
                type="button"
                className={`chat-row__menu-btn ${isMenuOpen ? "is-menu-open" : ""}`}
                disabled={!isConnected}
                aria-expanded={isMenuOpen}
                aria-haspopup="menu"
                aria-label={`Меню чата ${session.title}`}
                data-testid={`session-menu-trigger-${session.session_id}`}
                title="Действия с чатом"
                onClick={(e) => {
                  e.stopPropagation();
                  toggleSessionMenu(session, e.currentTarget);
                }}
              >
                <DotsThree size={14} weight="bold" />
              </button>
            </div>
          </>
        )}
      </div>
    );
  };

  return (
    <aside
      className="project-sidebar"
      aria-label="Проекты и чаты"
      data-testid="desktop-sidebar"
    >
      <section
        className="sidebar-projects-section"
        aria-label="Проекты"
        data-testid="sidebar-projects-section"
      >
        <div className="sidebar-heading">
          <span className="sidebar-title">Проекты</span>

          <button
            ref={createTriggerRef}
            type="button"
            className={`icon-button new-project-trigger ${createMenuOpen ? "is-active" : ""}`}
            onClick={() => setCreateMenuOpen((prev) => !prev)}
            disabled={!isConnected}
            aria-label="Новый проект"
            aria-expanded={createMenuOpen}
            aria-haspopup="menu"
            data-testid="new-project-trigger"
            title="Новый проект"
          >
            <Plus size={16} />
          </button>

          <PortalPopover
            isOpen={createMenuOpen}
            anchorEl={createTriggerRef.current}
            onClose={() => setCreateMenuOpen(false)}
            placement="bottom-end"
            className="project-create-menu"
            role="menu"
            ariaLabel="Создать проект"
            testId="project-create-menu"
          >
            <strong>Новый проект</strong>
            <button
              type="button"
              role="menuitem"
              data-testid="create-project-pick-folder"
              onClick={() => {
                setCreateMenuOpen(false);
                void actions.requestFolderPicker({ mode: "create-project" });
              }}
            >
              <FolderOpen size={18} aria-hidden="true" />
              <span>
                <strong>Выбрать папку…</strong>
                <small>Открыть существующую папку или репозиторий</small>
              </span>
            </button>
            <button
              type="button"
              role="menuitem"
              data-testid="create-project-folderless"
              onClick={() => {
                setCreateMenuOpen(false);
                void actions.createFolderlessProject();
              }}
            >
              <FolderPlus size={18} aria-hidden="true" />
              <span>
                <strong>Пока без папки</strong>
                <small>Начать чат сразу, привязать папку позже</small>
              </span>
            </button>
          </PortalPopover>
        </div>

        <div className="sidebar-projects-scroll" data-testid="sidebar-projects-scroll">
          {activeGroups.length === 0 && archivedGroups.length === 0 ? (
            <div className="sidebar-empty">
              {query ? (
                <p>По запросу «{state.searchQuery}» ничего не найдено.</p>
              ) : (
                <p>Нажмите «+», чтобы открыть папку или начать проект без папки.</p>
              )}
            </div>
          ) : (
            <div className="project-groups">
              {activeGroups.map(({ project, sessions }) => {
                const isEditingProj = editingProjectId === project.project_id;
                const isFolderless = !project.workspace_path;
                const isMenuOpen = openProjectMenu?.project.project_id === project.project_id;
                const isSelectedProject = meta.activeProject?.project_id === project.project_id;
                const isCollapsed = !query && Boolean(collapsedProjects[project.project_id]);

                return (
                  <section
                    key={project.project_id}
                    className="project-group"
                    data-testid={`project-group-${project.project_id}`}
                  >
                    <div
                      className={`project-heading ${isEditingProj ? "is-editing" : ""} ${
                        isMenuOpen ? "has-menu-open" : ""
                      } ${isSelectedProject ? "is-selected-project" : ""}`}
                      tabIndex={0}
                      data-testid={`project-heading-${project.project_id}`}
                      onClick={() => {
                        if (!isEditingProj) {
                          actions.selectProject(project.project_id);
                        }
                      }}
                      onDoubleClick={() => startProjectRename(project)}
                      onKeyDown={(e) => {
                        if (e.key === "F2") {
                          e.preventDefault();
                          startProjectRename(project);
                        } else if (e.key === "Enter" && !isEditingProj) {
                          e.preventDefault();
                          actions.selectProject(project.project_id);
                        }
                      }}
                    >
                      <button
                        type="button"
                        className="project-collapse-btn"
                        aria-expanded={!isCollapsed}
                        aria-label={
                          isCollapsed
                            ? `Развернуть проект ${project.name}`
                            : `Свернуть проект ${project.name}`
                        }
                        data-testid={`project-collapse-btn-${project.project_id}`}
                        onClick={(e) => {
                          e.stopPropagation();
                          toggleProjectCollapsed(project.project_id);
                        }}
                      >
                        {isCollapsed ? (
                          <CaretRight size={11} aria-hidden="true" />
                        ) : (
                          <CaretDown size={11} aria-hidden="true" />
                        )}
                      </button>

                      {isFolderless ? (
                        <FolderDashed
                          size={16}
                          className="project-icon project-icon--unbound"
                          aria-hidden="true"
                        />
                      ) : (
                        <FolderSimple size={16} className="project-icon" aria-hidden="true" />
                      )}

                      {isEditingProj ? (
                        <input
                          type="text"
                          className="inline-rename-input"
                          value={editingProjectName}
                          autoFocus
                          aria-label="Новое название проекта"
                          data-testid="inline-project-rename-input"
                          onClick={(e) => e.stopPropagation()}
                          onFocus={(e) => {
                            e.currentTarget.scrollLeft = 0;
                            e.currentTarget.select();
                          }}
                          onChange={(e) => setEditingProjectName(e.target.value)}
                          onBlur={() => commitProjectRename(project)}
                          onKeyDown={(e) => {
                            if (e.key === "Enter") {
                              e.preventDefault();
                              commitProjectRename(project);
                            } else if (e.key === "Escape") {
                              e.preventDefault();
                              e.stopPropagation();
                              setEditingProjectId(null);
                            }
                          }}
                        />
                      ) : (
                        <>
                          <span
                            className="project-heading__name"
                            title={project.workspace_path ?? "Пока без папки — можно привязать позже"}
                          >
                            {project.name}
                          </span>
                          {project.pinned && (
                            <PushPin
                              size={12}
                              weight="fill"
                              className="project-pin-indicator"
                              aria-label="Закреплён"
                              data-testid={`project-pin-indicator-${project.project_id}`}
                            />
                          )}
                        </>
                      )}

                      {!isEditingProj && (
                        <div className="project-heading__actions">
                          {isFolderless && (
                            <button
                              type="button"
                              className="row-action-btn"
                              onClick={(e) => {
                                e.stopPropagation();
                                void actions.requestFolderPicker({
                                  mode: "attach-project",
                                  projectId: project.project_id,
                                });
                              }}
                              aria-label={`Привязать папку к проекту ${project.name}`}
                              title="Привязать папку…"
                            >
                              <FolderOpen size={14} />
                            </button>
                          )}
                          <button
                            type="button"
                            className="row-action-btn"
                            onClick={(e) => {
                              e.stopPropagation();
                              startProjectRename(project);
                            }}
                            aria-label={`Переименовать проект ${project.name}`}
                            title="Переименовать проект (F2)"
                          >
                            <PencilSimple size={13} />
                          </button>
                          <button
                            type="button"
                            className={`row-action-btn ${isMenuOpen ? "is-menu-open" : ""}`}
                            disabled={!isConnected}
                            aria-expanded={isMenuOpen}
                            aria-haspopup="menu"
                            aria-label={`Меню проекта ${project.name}`}
                            data-testid={`project-menu-trigger-${project.project_id}`}
                            title="Действия с проектом"
                            onClick={(e) => {
                              e.stopPropagation();
                              toggleProjectMenu(project, false, e.currentTarget);
                            }}
                          >
                            <DotsThree size={15} weight="bold" />
                          </button>
                          <button
                            type="button"
                            className="new-chat-trigger"
                            onClick={(e) => {
                              e.stopPropagation();
                              void actions.createBlankChat(project.project_id);
                            }}
                            disabled={!isConnected}
                            aria-label={`Новый чат в проекте ${project.name}`}
                            data-testid={`new-chat-btn-${project.project_id}`}
                            title="Новый чат (Ctrl+N)"
                          >
                            <Plus size={14} />
                          </button>
                        </div>
                      )}
                    </div>

                    {!isCollapsed && (
                      <div className="nested-chats" role="list">
                        {sessions.length === 0 ? (
                          <button
                            type="button"
                            className="nested-chats__empty-btn"
                            onClick={() => void actions.createBlankChat(project.project_id)}
                            disabled={!isConnected}
                          >
                            + Новый чат
                          </button>
                        ) : (
                          sessions.map((session) => renderChatRow(session))
                        )}
                      </div>
                    )}
                  </section>
                );
              })}

              {archivedGroups.length > 0 && (
                <div className="sidebar-archive-section" data-testid="sidebar-archive-section">
                  <button
                    type="button"
                    className="sidebar-archive-toggle"
                    aria-expanded={archiveExpanded}
                    data-testid="sidebar-archive-toggle"
                    onClick={() => setArchiveExpanded((prev) => !prev)}
                  >
                    {archiveExpanded ? (
                      <CaretDown size={12} aria-hidden="true" />
                    ) : (
                      <CaretRight size={12} aria-hidden="true" />
                    )}
                    <Archive size={13} aria-hidden="true" />
                    <span>Архив ({archivedGroups.length})</span>
                  </button>

                  {archiveExpanded && (
                    <div className="sidebar-archive-list" data-testid="sidebar-archive-list">
                      {archivedGroups.map(({ project, sessions: archivedSessions }) => {
                        const isMenuOpen = openProjectMenu?.project.project_id === project.project_id;
                        const isSelectedArchived =
                          meta.activeProject?.project_id === project.project_id;
                        return (
                          <div key={project.project_id} className="archived-project-block">
                            <div
                              className={`project-heading project-heading--archived ${
                                isMenuOpen ? "has-menu-open" : ""
                              } ${isSelectedArchived ? "is-selected-project" : ""}`}
                              tabIndex={0}
                              data-testid={`archived-project-${project.project_id}`}
                              onClick={() => actions.selectProject(project.project_id)}
                              onKeyDown={(e) => {
                                if (e.key === "Enter") {
                                  e.preventDefault();
                                  actions.selectProject(project.project_id);
                                }
                              }}
                            >
                              <Archive
                                size={14}
                                className="project-icon--unbound"
                                aria-hidden="true"
                              />
                              <span className="project-heading__name">{project.name}</span>
                              <div className="project-heading__actions">
                                <button
                                  type="button"
                                  className="row-action-btn"
                                  disabled={!isConnected}
                                  data-testid={`unarchive-project-btn-${project.project_id}`}
                                  aria-label={`Вернуть проект ${project.name} из архива`}
                                  title="Вернуть из архива"
                                  onClick={(e) => {
                                    e.stopPropagation();
                                    void actions.unarchiveProject(project.project_id);
                                  }}
                                >
                                  <ArrowCounterClockwise size={13} />
                                </button>
                                <button
                                  type="button"
                                  className={`row-action-btn ${isMenuOpen ? "is-menu-open" : ""}`}
                                  disabled={!isConnected}
                                  aria-expanded={isMenuOpen}
                                  aria-haspopup="menu"
                                  aria-label={`Меню архивного проекта ${project.name}`}
                                  data-testid={`project-menu-trigger-${project.project_id}`}
                                  onClick={(e) => {
                                    e.stopPropagation();
                                    toggleProjectMenu(project, true, e.currentTarget);
                                  }}
                                >
                                  <DotsThree size={15} weight="bold" />
                                </button>
                              </div>
                            </div>

                            {isSelectedArchived && archivedSessions.length > 0 && (
                              <div className="nested-chats nested-chats--archived" role="list">
                                {archivedSessions.map((sess) => {
                                  const isActive = state.selectedSessionId === sess.session_id;
                                  return (
                                    <button
                                      key={sess.session_id}
                                      type="button"
                                      className={`chat-row ${isActive ? "is-active" : ""}`}
                                      data-testid={`archived-session-item-${sess.session_id}`}
                                      onClick={() => void actions.selectSession(sess.session_id)}
                                    >
                                      <span className="chat-row__title-wrap">
                                        <span className="chat-row__title">{sess.title}</span>
                                      </span>
                                      <small className="chat-row__meta">архив</small>
                                    </button>
                                  );
                                })}
                              </div>
                            )}
                          </div>
                        );
                      })}
                    </div>
                  )}
                </div>
              )}
            </div>
          )}
        </div>
      </section>

      <section
        className="sidebar-chats-section"
        aria-label="Чаты"
        data-testid="sidebar-chats-section"
      >
        <div className="sidebar-heading sidebar-heading--chats">
          <span className="sidebar-title">Чаты</span>
          <div className="sidebar-heading__controls">
            {archivedUserChatsCount > 0 && (
              <button
                type="button"
                className={`sidebar-archived-chats-toggle ${showArchivedChats ? "is-active" : ""}`}
                onClick={() => setShowArchivedChats((prev) => !prev)}
                aria-pressed={showArchivedChats}
                data-testid="sidebar-show-archived-chats-toggle"
                title={
                  showArchivedChats ? "Скрыть архивные чаты" : "Показать архивные чаты"
                }
              >
                <Archive size={12} aria-hidden="true" />
                <span>{archivedUserChatsCount}</span>
              </button>
            )}
            <button
              type="button"
              className="icon-button new-project-trigger"
              onClick={() => void actions.createStandaloneChat()}
              disabled={!isConnected}
              title="Новый чат"
              aria-label="Новый одиночный чат"
              data-testid="new-standalone-chat-btn"
            >
              <Plus size={16} />
            </button>
          </div>
        </div>

        <div className="sidebar-chats-scroll" data-testid="sidebar-chats-scroll">
          {standaloneChats.length === 0 ? (
            <div
              className="sidebar-empty sidebar-empty--compact"
              data-testid="standalone-chats-empty"
            >
              {query ? (
                <p>По запросу «{state.searchQuery}» чаты не найдены.</p>
              ) : (
                <p>Одиночных чатов пока нет</p>
              )}
            </div>
          ) : (
            <div
              className="standalone-chats-list"
              role="list"
              data-testid="standalone-chats-list"
            >
              {standaloneChats.map((session) => renderChatRow(session))}
            </div>
          )}
        </div>
      </section>

      {/* Portaled Session Context Menu */}
      <PortalPopover
        isOpen={openSessionMenu !== null}
        anchorEl={openSessionMenu?.anchorEl ?? null}
        onClose={() => {
          setOpenSessionMenu(null);
          setConfirmDeleteSessionId(null);
        }}
        placement="bottom-end"
        className="project-action-menu"
        role="menu"
        ariaLabel={
          openSessionMenu ? `Действия с чатом ${openSessionMenu.session.title}` : "Действия с чатом"
        }
        testId={openSessionMenu ? `session-menu-${openSessionMenu.session.session_id}` : undefined}
      >
        {openSessionMenu && (() => {
          const { session } = openSessionMenu;
          const isArchivedChat = session.status === "archived";
          const isConfirmingDelete = confirmDeleteSessionId === session.session_id;
          return (
            <>
              <button
                type="button"
                role="menuitem"
                data-testid={`session-menu-rename-${session.session_id}`}
                onClick={() => startSessionRename(session)}
              >
                <PencilSimple size={14} aria-hidden="true" />
                <span>Переименовать</span>
              </button>

              <button
                type="button"
                role="menuitem"
                data-testid={`session-menu-export-${session.session_id}`}
                onClick={() => void handleExportSessionMarkdown(session)}
              >
                <DownloadSimple size={14} aria-hidden="true" />
                <span>Экспорт в Markdown</span>
              </button>

              <button
                type="button"
                role="menuitem"
                data-testid={`session-menu-archive-${session.session_id}`}
                onClick={() => {
                  setOpenSessionMenu(null);
                  setConfirmDeleteSessionId(null);
                  void actions.setSessionStatus(
                    session.session_id,
                    isArchivedChat ? "active" : "archived",
                  );
                }}
              >
                {isArchivedChat ? (
                  <>
                    <ArrowCounterClockwise size={14} aria-hidden="true" />
                    <span>Восстановить</span>
                  </>
                ) : (
                  <>
                    <Archive size={14} aria-hidden="true" />
                    <span>Архивировать</span>
                  </>
                )}
              </button>

              <div className="popover-divider" role="separator" />

              {isConfirmingDelete ? (
                <div
                  className="session-delete-inline-confirm"
                  data-testid={`session-menu-delete-confirm-${session.session_id}`}
                >
                  <p>Удалить чат «{session.title}»?</p>
                  <div className="session-delete-inline-confirm__actions">
                    <button
                      type="button"
                      className="popover-secondary-btn"
                      onClick={() => setConfirmDeleteSessionId(null)}
                    >
                      Отмена
                    </button>
                    <button
                      type="button"
                      className="d2-danger-btn d2-danger-btn--sm"
                      data-testid={`session-menu-delete-yes-${session.session_id}`}
                      onClick={() => {
                        setOpenSessionMenu(null);
                        setConfirmDeleteSessionId(null);
                        void actions.deleteSession(session.session_id);
                      }}
                    >
                      Удалить
                    </button>
                  </div>
                </div>
              ) : (
                <button
                  type="button"
                  role="menuitem"
                  className="menu-item-danger"
                  data-testid={`session-menu-delete-${session.session_id}`}
                  onClick={() => setConfirmDeleteSessionId(session.session_id)}
                >
                  <Trash size={14} aria-hidden="true" />
                  <span>Удалить</span>
                </button>
              )}
            </>
          );
        })()}
      </PortalPopover>

      {/* Portaled Project Lifecycle Menu (never clipped by sidebar scroll containers) */}
      <PortalPopover
        isOpen={openProjectMenu !== null}
        anchorEl={openProjectMenu?.anchorEl ?? null}
        onClose={() => setOpenProjectMenu(null)}
        placement={openProjectMenu?.isArchived ? "top-end" : "bottom-end"}
        className="project-action-menu"
        role="menu"
        ariaLabel={
          openProjectMenu ? `Действия с проектом ${openProjectMenu.project.name}` : "Действия с проектом"
        }
        testId={openProjectMenu ? `project-menu-${openProjectMenu.project.project_id}` : undefined}
      >
        {openProjectMenu && (() => {
          const { project, isArchived } = openProjectMenu;
          const isFolderless = !project.workspace_path;
          return (
            <>
              {!isArchived && (
                <button
                  type="button"
                  role="menuitem"
                  data-testid={`project-menu-pin-${project.project_id}`}
                  onClick={() => {
                    setOpenProjectMenu(null);
                    void actions.toggleProjectPin(project.project_id);
                  }}
                >
                  <PushPin
                    size={14}
                    weight={project.pinned ? "fill" : "regular"}
                    aria-hidden="true"
                  />
                  <span>{project.pinned ? "Открепить" : "Закрепить"}</span>
                </button>
              )}

              <button
                type="button"
                role="menuitem"
                data-testid={`project-menu-rename-${project.project_id}`}
                onClick={() => startProjectRename(project)}
              >
                <PencilSimple size={14} aria-hidden="true" />
                <span>Переименовать</span>
              </button>

              <button
                type="button"
                role="menuitem"
                data-testid={`project-menu-rebind-${project.project_id}`}
                onClick={() => {
                  setOpenProjectMenu(null);
                  void actions.requestFolderPicker({
                    mode: isFolderless ? "attach-project" : "rebind-project",
                    projectId: project.project_id,
                  });
                }}
              >
                <FolderOpen size={14} aria-hidden="true" />
                <span>{isFolderless ? "Привязать папку…" : "Перепривязать папку…"}</span>
              </button>

              {!isFolderless && (
                <button
                  type="button"
                  role="menuitem"
                  data-testid={`project-menu-detach-${project.project_id}`}
                  onClick={() => {
                    setOpenProjectMenu(null);
                    void actions.detachProjectWorkspace(project.project_id);
                  }}
                >
                  <LinkBreak size={14} aria-hidden="true" />
                  <span>Отвязать папку</span>
                </button>
              )}

              <div className="popover-divider" role="separator" />

              {isArchived ? (
                <button
                  type="button"
                  role="menuitem"
                  data-testid={`project-menu-unarchive-${project.project_id}`}
                  onClick={() => {
                    setOpenProjectMenu(null);
                    void actions.unarchiveProject(project.project_id);
                  }}
                >
                  <ArrowCounterClockwise size={14} aria-hidden="true" />
                  <span>Вернуть из архива</span>
                </button>
              ) : (
                <button
                  type="button"
                  role="menuitem"
                  data-testid={`project-menu-archive-${project.project_id}`}
                  onClick={() => {
                    setOpenProjectMenu(null);
                    void actions.archiveProject(project.project_id);
                  }}
                >
                  <Archive size={14} aria-hidden="true" />
                  <span>В архив</span>
                </button>
              )}

              <button
                type="button"
                role="menuitem"
                data-testid={`project-menu-remove-${project.project_id}`}
                onClick={() => {
                  setOpenProjectMenu(null);
                  setPendingConfirm({ project, mode: "remove" });
                }}
              >
                <EyeSlash size={14} aria-hidden="true" />
                <span>Убрать из списка…</span>
              </button>

              <button
                type="button"
                role="menuitem"
                className="menu-item-danger"
                data-testid={`project-menu-delete-${project.project_id}`}
                onClick={() => {
                  setOpenProjectMenu(null);
                  setPendingConfirm({ project, mode: "delete" });
                }}
              >
                <Trash size={14} aria-hidden="true" />
                <span>Удалить проект…</span>
              </button>
            </>
          );
        })()}
      </PortalPopover>

      {/* D2-UX-03 & D2-UX-06: Confirmation dialog with focus trap before irreversible remove/delete */}
      {pendingConfirm && (
        <div
          className="dialog-backdrop"
          onMouseDown={() => setPendingConfirm(null)}
          data-testid="project-delete-confirm-backdrop"
        >
          <div
            ref={confirmDialogRef}
            className="floating-dialog confirm-dialog"
            role="dialog"
            aria-modal="true"
            aria-label={
              pendingConfirm.mode === "remove"
                ? "Убрать проект из списка Dennett"
                : "Удаление данных проекта в Dennett"
            }
            data-testid="project-delete-confirm-dialog"
            onMouseDown={(e) => e.stopPropagation()}
          >
            <header className="settings-sheet__header">
              <div>
                <h2>
                  {pendingConfirm.mode === "remove"
                    ? `Убрать проект «${pendingConfirm.project.name}» из списка?`
                    : `Удалить проект «${pendingConfirm.project.name}» из Dennett?`}
                </h2>
                <p>
                  {pendingConfirm.mode === "remove"
                    ? "Проект будет скрыт из рабочих списков Dennett, активные таймеры остановятся, но папка с вашими файлами на диске останется полностью нетронутой."
                    : "Служебные данные проекта в Dennett будут удалены; папка с вашими файлами на диске останется полностью нетронутой."}
                </p>
              </div>
              <button
                type="button"
                className="icon-button"
                onClick={() => setPendingConfirm(null)}
                aria-label="Закрыть подтверждение"
              >
                <X size={16} />
              </button>
            </header>
            <div className="confirm-dialog__actions">
              <button
                type="button"
                className="popover-secondary-btn"
                onClick={() => setPendingConfirm(null)}
              >
                Отмена
              </button>
              <button
                type="button"
                className="d2-danger-btn"
                data-testid="project-delete-confirm-btn"
                onClick={() => {
                  const { project, mode } = pendingConfirm;
                  setPendingConfirm(null);
                  if (mode === "remove") {
                    void actions.removeProject(project.project_id);
                  } else {
                    void actions.deleteProject(project.project_id);
                  }
                }}
              >
                {pendingConfirm.mode === "remove" ? (
                  <>
                    <EyeSlash size={14} aria-hidden="true" />
                    Убрать из списка
                  </>
                ) : (
                  <>
                    <Trash size={14} aria-hidden="true" />
                    Удалить запись проекта
                  </>
                )}
              </button>
            </div>
          </div>
        </div>
      )}
    </aside>
  );
}
