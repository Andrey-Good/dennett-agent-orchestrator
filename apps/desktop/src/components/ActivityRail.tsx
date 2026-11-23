import {
  BracketsCurly,
  Browsers,
  ChatsCircle,
  ListChecks,
  Plug,
} from "@phosphor-icons/react";

import { useWorkspace } from "../state/WorkspaceProvider";
import { QuotaRings } from "./QuotaRings";

export function ActivityRail() {
  const { state, actions } = useWorkspace();

  return (
    <nav className="activity-rail" aria-label="Разделы Dennett" data-testid="activity-rail">
      <div className="rail-main">
        <span className="rail-brand" aria-hidden="true" title="Dennett">
          <BracketsCurly size={18} weight="regular" />
        </span>

        <button
          type="button"
          className={`icon-button ${state.isSidebarOpen ? "is-active" : ""}`}
          onClick={actions.toggleSidebar}
          aria-label="Чаты и проекты"
          aria-pressed={state.isSidebarOpen}
          title="Чаты и проекты (Ctrl+B)"
        >
          <ChatsCircle size={19} weight={state.isSidebarOpen ? "fill" : "regular"} />
        </button>

        <button
          type="button"
          className={`icon-button ${state.isRightPanelOpen ? "is-active" : ""}`}
          onClick={actions.toggleRightPanel}
          aria-label="Рабочее пространство"
          aria-pressed={state.isRightPanelOpen}
          title="Рабочее пространство (Ctrl+\)"
        >
          <Browsers size={19} weight={state.isRightPanelOpen ? "fill" : "regular"} />
        </button>

        <button
          type="button"
          className="icon-button rail-future"
          disabled
          aria-label="Задачи и планы (будет на D2)"
          title="Задачи и планы · этап D2"
        >
          <ListChecks size={18} />
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
