import { useEffect, useMemo, useRef, useState } from "react";
import {
  FolderDashed,
  FolderOpen,
  FolderPlus,
  FolderSimple,
  PencilSimple,
  Plus,
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
  const [editingSessionId, setEditingSessionId] = useState<string | null>(null);
  const [editingSessionTitle, setEditingSessionTitle] = useState("");
  const [editingProjectId, setEditingProjectId] = useState<string | null>(null);
  const [editingProjectName, setEditingProjectName] = useState("");

  const createMenuRef = useRef<HTMLDivElement | null>(null);

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

  const query = state.searchQuery.trim().toLowerCase();

  const grouped = useMemo(() => {
    return state.projects
      .map((project) => {
        const projectMatches =
          !query ||
          project.name.toLowerCase().includes(query) ||
          (project.workspace_path?.toLowerCase().includes(query) ?? false);
        const sessions = state.sessions.filter(
          (session) =>
            session.project_id === project.project_id &&
            (!query || projectMatches || session.title.toLowerCase().includes(query)),
        );
        return { project, sessions, matches: projectMatches || sessions.length > 0 };
      })
      .filter((entry) => entry.matches);
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

      <div className="sidebar-scroll">
        {grouped.length === 0 ? (
          <div className="sidebar-empty">
            {query ? (
              <p>По запросу «{state.searchQuery}» ничего не найдено.</p>
            ) : (
              <p>Нажмите «+», чтобы открыть папку или начать проект без папки.</p>
            )}
          </div>
        ) : (
          <div className="project-groups">
            {grouped.map(({ project, sessions }) => {
              const isEditingProj = editingProjectId === project.project_id;
              const isFolderless = !project.workspace_path;

              return (
                <section
                  key={project.project_id}
                  className="project-group"
                  data-testid={`project-group-${project.project_id}`}
                >
                  <div
                    className={`project-heading ${isEditingProj ? "is-editing" : ""}`}
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
                      <FolderDashed size={16} className="project-icon project-icon--unbound" aria-hidden="true" />
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
                        const isActive = state.selectedSessionId === session.session_id;
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
          </div>
        )}
      </div>
    </aside>
  );
}
