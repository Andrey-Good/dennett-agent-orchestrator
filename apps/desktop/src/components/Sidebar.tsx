import { useEffect, useMemo, useRef, useState } from "react";
import {
  Archive,
  ArrowCounterClockwise,
  CaretDown,
  CaretRight,
  DotsThree,
  EyeSlash,
  FolderDashed,
  FolderOpen,
  FolderPlus,
  FolderSimple,
  LinkBreak,
  PencilSimple,
  Plus,
  Trash,
  X,
} from "@phosphor-icons/react";

import type { ProjectRecord, SessionRecord } from "../api/contracts";
import { useWorkspace } from "../state/WorkspaceProvider";

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

export function Sidebar() {
  const { state, actions } = useWorkspace();
  const [createMenuOpen, setCreateMenuOpen] = useState(false);
  const [openProjectMenuId, setOpenProjectMenuId] = useState<string | null>(null);
  const [openProjectMenuFlipUp, setOpenProjectMenuFlipUp] = useState(false);
  const [archiveExpanded, setArchiveExpanded] = useState(false);
  const [pendingDeleteProject, setPendingDeleteProject] = useState<ProjectRecord | null>(null);
  const [editingSessionId, setEditingSessionId] = useState<string | null>(null);
  const [editingSessionTitle, setEditingSessionTitle] = useState("");
  const [editingProjectId, setEditingProjectId] = useState<string | null>(null);
  const [editingProjectName, setEditingProjectName] = useState("");

  const createMenuRef = useRef<HTMLDivElement | null>(null);
  const projectMenuRef = useRef<HTMLDivElement | null>(null);
  const sidebarScrollRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!createMenuOpen) {
      return;
    }
    const onMouseDown = (event: MouseEvent) => {
      if (createMenuRef.current && !createMenuRef.current.contains(event.target as Node)) {
        setCreateMenuOpen(false);
      }
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        setCreateMenuOpen(false);
      }
    };
    document.addEventListener("mousedown", onMouseDown);
    window.addEventListener("keydown", onKeyDown, { capture: true });
    return () => {
      document.removeEventListener("mousedown", onMouseDown);
      window.removeEventListener("keydown", onKeyDown, { capture: true });
    };
  }, [createMenuOpen]);

  useEffect(() => {
    if (!openProjectMenuId) {
      return;
    }
    const onMouseDown = (event: MouseEvent) => {
      if (projectMenuRef.current && !projectMenuRef.current.contains(event.target as Node)) {
        setOpenProjectMenuId(null);
      }
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        setOpenProjectMenuId(null);
      }
    };
    document.addEventListener("mousedown", onMouseDown);
    window.addEventListener("keydown", onKeyDown, { capture: true });
    return () => {
      document.removeEventListener("mousedown", onMouseDown);
      window.removeEventListener("keydown", onKeyDown, { capture: true });
    };
  }, [openProjectMenuId]);

  useEffect(() => {
    if (!pendingDeleteProject) {
      return;
    }
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        setPendingDeleteProject(null);
      }
    };
    window.addEventListener("keydown", onKeyDown, { capture: true });
    return () => window.removeEventListener("keydown", onKeyDown, { capture: true });
  }, [pendingDeleteProject]);

  const query = state.searchQuery.trim().toLowerCase();

  const { activeGroups, archivedProjects } = useMemo(() => {
    const active: Array<{ project: ProjectRecord; sessions: SessionRecord[] }> = [];
    const archived: ProjectRecord[] = [];

    for (const project of state.projects) {
      const lifecycle = project.lifecycle_state ?? "active";
      if (lifecycle === "deleted_record") {
        continue;
      }
      const projectMatches =
        !query ||
        project.name.toLowerCase().includes(query) ||
        (project.workspace_path?.toLowerCase().includes(query) ?? false);

      if (lifecycle === "archived") {
        if (projectMatches) {
          archived.push(project);
        }
        continue;
      }

      const sessions = state.sessions.filter(
        (session) =>
          session.project_id === project.project_id &&
          (!query || projectMatches || session.title.toLowerCase().includes(query)),
      );
      if (projectMatches || sessions.length > 0) {
        active.push({ project, sessions });
      }
    }

    return { activeGroups: active, archivedProjects: archived };
  }, [query, state.projects, state.sessions]);

  const startSessionRename = (session: SessionRecord) => {
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

  const startProjectRename = (project: ProjectRecord) => {
    setEditingSessionId(null);
    setOpenProjectMenuId(null);
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
    projectId: string,
    isArchived: boolean,
    triggerEl: HTMLElement,
  ) => {
    if (openProjectMenuId === projectId) {
      setOpenProjectMenuId(null);
      return;
    }
    const scrollEl = sidebarScrollRef.current;
    if (scrollEl) {
      const scrollRect = scrollEl.getBoundingClientRect();
      const triggerRect = triggerEl.getBoundingClientRect();
      if (scrollRect.height > 0) {
        const spaceBelow = scrollRect.bottom - triggerRect.bottom;
        const spaceAbove = triggerRect.top - scrollRect.top;
        setOpenProjectMenuFlipUp(spaceBelow < 220 && spaceAbove > spaceBelow);
      } else {
        setOpenProjectMenuFlipUp(isArchived);
      }
    } else {
      setOpenProjectMenuFlipUp(isArchived);
    }
    setOpenProjectMenuId(projectId);
  };

  const renderProjectActionMenu = (project: ProjectRecord, isArchived: boolean) => {
    const isFolderless = !project.workspace_path;
    return (
      <div
        ref={projectMenuRef}
        className={`floating-popover project-action-menu ${
          openProjectMenuFlipUp ? "project-action-menu--up" : ""
        }`}
        role="menu"
        aria-label={`Действия с проектом ${project.name}`}
        data-testid={`project-menu-${project.project_id}`}
        onClick={(e) => e.stopPropagation()}
      >
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
            setOpenProjectMenuId(null);
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
              setOpenProjectMenuId(null);
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
              setOpenProjectMenuId(null);
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
              setOpenProjectMenuId(null);
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
            setOpenProjectMenuId(null);
            void actions.removeProject(project.project_id);
          }}
        >
          <EyeSlash size={14} aria-hidden="true" />
          <span>Убрать из списка</span>
        </button>

        <button
          type="button"
          role="menuitem"
          className="menu-item-danger"
          data-testid={`project-menu-delete-${project.project_id}`}
          onClick={() => {
            setOpenProjectMenuId(null);
            setPendingDeleteProject(project);
          }}
        >
          <Trash size={14} aria-hidden="true" />
          <span>Удалить проект…</span>
        </button>
      </div>
    );
  };

  return (
    <aside
      className="project-sidebar"
      aria-label="Проекты и чаты"
      data-testid="desktop-sidebar"
    >
      <div className="sidebar-heading" ref={createMenuRef}>
        <span className="sidebar-title">Проекты</span>

        <button
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

        {createMenuOpen && (
          <div
            className="floating-popover project-create-menu"
            role="menu"
            aria-label="Создать проект"
            data-testid="project-create-menu"
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
          </div>
        )}
      </div>

      <div className="sidebar-scroll" ref={sidebarScrollRef}>
        {activeGroups.length === 0 && archivedProjects.length === 0 ? (
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
              const isMenuOpen = openProjectMenuId === project.project_id;

              return (
                <section
                  key={project.project_id}
                  className="project-group"
                  data-testid={`project-group-${project.project_id}`}
                >
                  <div
                    className={`project-heading ${isEditingProj ? "is-editing" : ""} ${
                      isMenuOpen ? "has-menu-open" : ""
                    }`}
                    tabIndex={0}
                    onDoubleClick={() => startProjectRename(project)}
                    onKeyDown={(e) => {
                      if (e.key === "F2") {
                        e.preventDefault();
                        startProjectRename(project);
                      }
                    }}
                  >
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
                      <span
                        className="project-heading__name"
                        title={project.workspace_path ?? "Пока без папки — можно привязать позже"}
                      >
                        {project.name}
                      </span>
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
                            toggleProjectMenu(project.project_id, false, e.currentTarget);
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

                    {isMenuOpen && renderProjectActionMenu(project, false)}
                  </div>

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
                      sessions.map((session) => {
                        const isActive =
                          state.activeView === "chat" &&
                          state.selectedSessionId === session.session_id;
                        const isRunning = state.runningSessionIds.includes(session.session_id);
                        const isEditingSess = editingSessionId === session.session_id;

                        return (
                          <div
                            key={session.session_id}
                            className={`chat-row-wrap ${isActive ? "is-active" : ""} ${
                              isRunning ? "is-running" : ""
                            }`}
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
                                  </span>
                                  <small className="chat-row__meta">
                                    {isRunning
                                      ? "в работе"
                                      : formatRelativeTime(session.updated_at)}
                                  </small>
                                </button>
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
                              </>
                            )}
                          </div>
                        );
                      })
                    )}
                  </div>
                </section>
              );
            })}

            {archivedProjects.length > 0 && (
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
                  <span>Архив ({archivedProjects.length})</span>
                </button>

                {archiveExpanded && (
                  <div className="sidebar-archive-list" data-testid="sidebar-archive-list">
                    {archivedProjects.map((project) => {
                      const isMenuOpen = openProjectMenuId === project.project_id;
                      return (
                        <div
                          key={project.project_id}
                          className={`project-heading project-heading--archived ${
                            isMenuOpen ? "has-menu-open" : ""
                          }`}
                          tabIndex={0}
                          data-testid={`archived-project-${project.project_id}`}
                        >
                          <Archive size={14} className="project-icon--unbound" aria-hidden="true" />
                          <span className="project-heading__name">{project.name}</span>
                          <div className="project-heading__actions">
                            <button
                              type="button"
                              className="row-action-btn"
                              disabled={!isConnected}
                              data-testid={`unarchive-project-btn-${project.project_id}`}
                              aria-label={`Вернуть проект ${project.name} из архива`}
                              title="Вернуть из архива"
                              onClick={() => void actions.unarchiveProject(project.project_id)}
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
                                toggleProjectMenu(project.project_id, true, e.currentTarget);
                              }}
                            >
                              <DotsThree size={15} weight="bold" />
                            </button>
                          </div>
                          {isMenuOpen && renderProjectActionMenu(project, true)}
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

      {pendingDeleteProject && (
        <div
          className="dialog-backdrop"
          onMouseDown={() => setPendingDeleteProject(null)}
          data-testid="project-delete-confirm-backdrop"
        >
          <div
            className="floating-dialog confirm-dialog"
            role="dialog"
            aria-modal="true"
            aria-label="Удаление данных проекта в Dennett"
            data-testid="project-delete-confirm-dialog"
            onMouseDown={(e) => e.stopPropagation()}
          >
            <header className="settings-sheet__header">
              <div>
                <h2>Удалить проект «{pendingDeleteProject.name}» из Dennett?</h2>
                <p>
                  Служебные данные проекта в Dennett будут удалены; папка с вашими файлами на диске
                  останется полностью нетронутой.
                </p>
              </div>
              <button
                type="button"
                className="icon-button"
                onClick={() => setPendingDeleteProject(null)}
                aria-label="Закрыть подтверждение"
              >
                <X size={16} />
              </button>
            </header>
            <div className="confirm-dialog__actions">
              <button
                type="button"
                className="popover-secondary-btn"
                onClick={() => setPendingDeleteProject(null)}
              >
                Отмена
              </button>
              <button
                type="button"
                className="d2-danger-btn"
                data-testid="project-delete-confirm-btn"
                onClick={() => {
                  const id = pendingDeleteProject.project_id;
                  setPendingDeleteProject(null);
                  void actions.deleteProject(id);
                }}
              >
                <Trash size={14} aria-hidden="true" />
                Удалить запись проекта
              </button>
            </div>
          </div>
        </div>
      )}
    </aside>
  );
}
