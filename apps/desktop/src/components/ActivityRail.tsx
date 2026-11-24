import {
  BracketsCurly,
  Browsers,
  ChatsCircle,
  ListChecks,
  Plug,
  Tray,
} from "@phosphor-icons/react";

import { useWorkspace } from "../state/WorkspaceProvider";
import { QuotaRings } from "./QuotaRings";

export function ActivityRail() {
  const { state, actions, meta } = useWorkspace();

  const activeProjectIds = new Set(
    state.projects
      .filter((p) => {
        const lc = p.lifecycle_state ?? "active";
        return lc !== "archived" && lc !== "deleted_record";
      })
      .map((p) => p.project_id),
  );
  const openInboxCount = Math.max(
    state.unresolvedInboxCount,
    state.inboxCards.filter(
      (card) => card.status === "open" && activeProjectIds.has(card.project_id),
    ).length,
  );
  const isChatView = state.activeView === "chat";
  const isInboxView = state.activeView === "inbox";
  const isTasksView = state.activeView === "tasks";

  const handleChatNav = () => {
    if (isChatView) {
      actions.toggleSidebar();
    } else {
      actions.setActiveView("chat");
      if (!state.isSidebarOpen) {
        actions.setSidebarOpen(true);
      }
    }
  };

  return (
    <nav className="activity-rail" aria-label="Разделы Dennett" data-testid="activity-rail">
      <div className="rail-main">
        <span className="rail-brand" aria-hidden="true" title="Dennett">
          <BracketsCurly size={18} weight="regular" />
        </span>

        <button
          type="button"
          className={`icon-button ${isChatView ? "is-active" : ""}`}
          onClick={handleChatNav}
          aria-label="Чаты и проекты"
          aria-pressed={isChatView}
          data-testid="rail-nav-chat"
          title="Чаты и проекты (Alt+1 / Ctrl+B)"
        >
          <ChatsCircle size={19} weight={isChatView ? "fill" : "regular"} />
        </button>

        <button
          type="button"
          className={`icon-button rail-nav-btn ${isInboxView ? "is-active" : ""}`}
          onClick={() => actions.setActiveView("inbox")}
          aria-label={
            openInboxCount > 0 ? `Входящие (${openInboxCount})` : "Входящие"
          }
          aria-pressed={isInboxView}
          data-testid="rail-nav-inbox"
          title="Входящие и Радар (Alt+2)"
        >
          <Tray size={19} weight={isInboxView ? "fill" : "regular"} />
          {openInboxCount > 0 && (
            <span className="rail-badge" data-testid="rail-inbox-badge">
              {openInboxCount > 99 ? "99+" : openInboxCount}
            </span>
          )}
        </button>

        <button
          type="button"
          className={`icon-button rail-nav-btn ${isTasksView ? "is-active" : ""}`}
          onClick={() => actions.setActiveView("tasks")}
          aria-label={
            meta.activeTaskCount > 0
              ? `Поручения (${meta.activeTaskCount} активных)`
              : "Поручения"
          }
          aria-pressed={isTasksView}
          data-testid="rail-nav-tasks"
          title="Поручения и расписания (Alt+3)"
        >
          <ListChecks size={19} weight={isTasksView ? "fill" : "regular"} />
          {meta.activeTaskCount > 0 && (
            <span className="rail-badge rail-badge--subtle" data-testid="rail-tasks-badge">
              {meta.activeTaskCount > 99 ? "99+" : meta.activeTaskCount}
            </span>
          )}
        </button>

        <button
          type="button"
          className={`icon-button ${state.isRightPanelOpen ? "is-active" : ""}`}
          onClick={actions.toggleRightPanel}
          aria-label="Рабочее пространство"
          aria-pressed={state.isRightPanelOpen}
          data-testid="rail-toggle-workspace"
          title="Рабочее пространство (Ctrl+\)"
        >
          <Browsers size={19} weight={state.isRightPanelOpen ? "fill" : "regular"} />
        </button>

        <button
          type="button"
          className="icon-button rail-future"
          disabled
          aria-label="Интеграции и подключения (будет на D3)"
          title="Интеграции и MCP · этап D3"
        >
          <Plug size={18} />
        </button>
      </div>

      <QuotaRings quotas={state.runtimeCatalog?.quotas ?? []} />
    </nav>
  );
}
