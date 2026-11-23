import { useState, type FormEvent } from "react";

import { useWorkspace } from "../state/WorkspaceProvider";
import { ActivityIcon, ChatIcon, FolderIcon, PanelRightIcon, PlusIcon } from "./Icons";

function formatRelativeAge(tsMs: number): string {
  const diffMin = Math.max(0, Math.floor((Date.now() - tsMs) / 60_000));
  if (diffMin < 1) {
    return "сейчас";
  }
  if (diffMin < 60) {
    return `${diffMin}м`;
  }
  const diffHours = Math.floor(diffMin / 60);
  if (diffHours < 24) {
    return `${diffHours}ч`;
  }
  return `${Math.floor(diffHours / 24)}д`;
}

export function Sidebar() {
  const { state, actions, meta } = useWorkspace();
  const [showNewProjectForm, setShowNewProjectForm] = useState(false);
  const [projectName, setProjectName] = useState("");
  const [workspacePath, setWorkspacePath] = useState("");
  const [memoryScope, setMemoryScope] = useState("");
  const [newChatProjectId, setNewChatProjectId] = useState<string | null>(null);
  const [newChatTitle, setNewChatTitle] = useState("");

  const normalizedQuery = state.searchQuery.trim().toLowerCase();
  const isConnected = Boolean(state.healthSnapshot?.connected);
  const isMemoryDegraded =
    isConnected && state.healthSnapshot?.health?.memory_status === "degraded_unreachable";

  const handleCreateProject = async (event: FormEvent) => {
    event.preventDefault();
    if (!projectName.trim() || !workspacePath.trim()) {
      return;
    }
    const slug = projectName
      .trim()
      .toLowerCase()
      .replace(/[^a-z0-9._-]+/g, "-")
      .replace(/^-+|-+$/g, "");
    const effectiveScope = memoryScope.trim() || `project:${slug || "default"}`;
      // After creating a new project, immediately open an isolated scoped session for it.
      const created = await actions.createProject({
        name: projectName.trim(),
        workspacePath: workspacePath.trim(),
        memoryScope: effectiveScope,
      });
      if (created) {
        setProjectName("");
        setWorkspacePath("");
        setMemoryScope("");
        setShowNewProjectForm(false);
        await actions.createSession(created.project_id, "Новый разговор");
      }
  };

  const handleCreateSession = async (event: FormEvent, projectId: string) => {
    event.preventDefault();
    const created = await actions.createSession(projectId, newChatTitle.trim() || "Новый разговор");
    if (created) {
      setNewChatTitle("");
      setNewChatProjectId(null);
    }
  };

  return (
    <>
      <aside className="activity-rail" aria-label="Панель разделов">
        <div className="activity-rail__group">
          <button
            type="button"
            className={state.isSidebarOpen ? "rail-btn rail-btn--active" : "rail-btn"}
            onClick={actions.toggleSidebar}
            aria-label="Проекты и разговоры"
            aria-pressed={state.isSidebarOpen}
            title="Проекты и разговоры"
          >
            <ChatIcon />
          </button>
          <button
            type="button"
            className={state.isRightPanelOpen ? "rail-btn rail-btn--active" : "rail-btn"}
            onClick={actions.toggleRightPanel}
            aria-label="Артефакты и рабочее пространство"
            aria-pressed={state.isRightPanelOpen}
            title="Артефакты и рабочее пространство"
          >
            <PanelRightIcon />
          </button>
          <button
            type="button"
            className="rail-btn"
            onClick={() => void actions.refreshAll()}
            aria-label="Обновить состояние Head и памяти"
            title="Обновить состояние"
          >
            <ActivityIcon />
          </button>
        </div>

        <div className="activity-rail__group">
          <span
            className={
              !isConnected
                ? "status-dot status-dot--offline"
                : isMemoryDegraded
                  ? "status-dot status-dot--degraded"
                  : "status-dot"
            }
            role="status"
            aria-label={
              !isConnected
                ? "Head недоступен"
                : isMemoryDegraded
                  ? "memoryd недоступна"
                  : "Head и memoryd работают"
            }
          />
        </div>
      </aside>

      {state.isSidebarOpen && (
        <nav className="sidebar" aria-label="Проекты и чаты">
          <div className="sidebar__header">
            <h2 className="sidebar__title">Проекты</h2>
            <button
              type="button"
              className="sidebar__action"
              onClick={() => setShowNewProjectForm((v) => !v)}
              disabled={!isConnected}
              aria-label="Создать проект"
            >
              <PlusIcon />
              Проект
            </button>
          </div>

          {showNewProjectForm && (
            <form className="inline-form" onSubmit={(e) => void handleCreateProject(e)}>
              <span className="inline-form__title">Новый проект</span>
              <input
                type="text"
                className="field-input"
                placeholder="Название проекта"
                aria-label="Название нового проекта"
                value={projectName}
                onChange={(e) => setProjectName(e.target.value)}
                required
              />
              <input
                type="text"
                className="field-input"
                placeholder="Путь к рабочей папке (например, C:/work/demo)"
                aria-label="Путь к рабочей папке"
                value={workspacePath}
                onChange={(e) => setWorkspacePath(e.target.value)}
                required
              />
              <input
                type="text"
                className="field-input"
                placeholder="Область памяти (необяз., project:slug)"
                aria-label="Область памяти проекта"
                value={memoryScope}
                onChange={(e) => setMemoryScope(e.target.value)}
              />
              <div className="inline-form__actions">
                <button
                  type="button"
                  className="btn btn--ghost"
                  onClick={() => setShowNewProjectForm(false)}
                >
                  Отмена
                </button>
                <button type="submit" className="btn btn--primary">
                  Создать
                </button>
              </div>
            </form>
          )}

          <div className="sidebar__scroll">
            {!isConnected ? (
              <p className="sidebar__empty">
                Подключение к dennett-head отсутствует.
              </p>
            ) : state.projects.length === 0 ? (
              <p className="sidebar__empty">
                Проектов пока нет. Создайте первый проект, чтобы начать разговор.
              </p>
            ) : (
              state.projects.map((project) => {
                const projectSessions = state.sessions.filter((session) => {
                  if (session.project_id !== project.project_id) {
                    return false;
                  }
                  if (!normalizedQuery) {
                    return true;
                  }
                  return (
                    session.title.toLowerCase().includes(normalizedQuery) ||
                    project.name.toLowerCase().includes(normalizedQuery)
                  );
                });

                if (
                  normalizedQuery &&
                  projectSessions.length === 0 &&
                  !project.name.toLowerCase().includes(normalizedQuery)
                ) {
                  return null;
                }

                return (
                  <div key={project.project_id} className="project-group">
                    <div className="project-group__head">
                      <span className="project-group__name" title={project.workspace_path}>
                        <FolderIcon />
                        <span>{project.name}</span>
                      </span>
                      <button
                        type="button"
                        className="project-group__add-chat"
                        onClick={() =>
                          setNewChatProjectId((curr) =>
                            curr === project.project_id ? null : project.project_id,
                          )
                        }
                        aria-label={`Новый чат в проекте ${project.name}`}
                        title="Новый чат"
                      >
                        <PlusIcon />
                      </button>
                    </div>

                    {newChatProjectId === project.project_id && (
                      <form
                        className="inline-form"
                        onSubmit={(e) => void handleCreateSession(e, project.project_id)}
                      >
                        <input
                          type="text"
                          className="field-input"
                          placeholder="Название чата"
                          aria-label="Название нового чата"
                          value={newChatTitle}
                          onChange={(e) => setNewChatTitle(e.target.value)}
                          autoFocus
                        />
                        <div className="inline-form__actions">
                          <button
                            type="button"
                            className="btn btn--ghost"
                            onClick={() => setNewChatProjectId(null)}
                          >
                            Отмена
                          </button>
                          <button type="submit" className="btn btn--primary">
                            Открыть
                          </button>
                        </div>
                      </form>
                    )}

                    <ul className="session-list">
                      {projectSessions.map((session) => {
                        const isSelected = session.session_id === state.selectedSessionId;
                        const isRunning = session.session_id === state.runningSessionId;
                        return (
                          <li key={session.session_id}>
                            <button
                              type="button"
                              className={
                                isSelected
                                  ? "session-item session-item--selected"
                                  : "session-item"
                              }
                              aria-current={isSelected ? "page" : undefined}
                              onClick={() => void actions.selectSession(session.session_id)}
                            >
                              <span className="session-item__title">{session.title}</span>
                              <span className="session-item__meta">
                                {isRunning ? "идёт..." : formatRelativeAge(session.updated_at)}
                              </span>
                            </button>
                          </li>
                        );
                      })}
                    </ul>
                  </div>
                );
              })
            )}
          </div>

          <div className="sidebar__footer">
            <div className="sidebar__account">
              <span className="avatar-pill" aria-hidden="true">
                D
              </span>
              <span className="sidebar__account-name">
                {meta.activeAgent
                  ? `${meta.activeAgent.name} · v${meta.activeAgent.current_config.version_number}`
                  : "Владелец"}
              </span>
            </div>
          </div>
        </nav>
      )}
    </>
  );
}
