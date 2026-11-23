import { useWorkspace } from "../state/WorkspaceProvider";
import {
  CloseIcon,
  DennettMarkIcon,
  MaximizeIcon,
  MinimizeIcon,
  PanelRightIcon,
  SearchIcon,
} from "./Icons";
import { RuntimeBadge } from "./RuntimeBadge";

export function TitleBar() {
  const { state, actions, meta } = useWorkspace();

  const handleDragStart = (event: React.MouseEvent<HTMLElement>) => {
    const target = event.target as HTMLElement;
    if (target.closest("button, input, select, textarea, a")) {
      return;
    }
    if (event.buttons === 1) {
      void meta.windowControls.startDragging();
    }
  };

  return (
    <header
      className="titlebar"
      data-tauri-drag-region=""
      onMouseDown={handleDragStart}
      aria-label="Заголовок окна Dennett"
    >
      <div className="titlebar__left">
        <button
          type="button"
          className="brand-mark"
          onClick={actions.toggleSidebar}
          aria-label="Свернуть или развернуть боковую панель"
          title="Боковая панель"
        >
          <DennettMarkIcon />
        </button>

        <nav className="breadcrumbs" aria-label="Навигация">
          <span>Проекты</span>
          {meta.activeProject && (
            <>
              <span className="breadcrumbs__sep" aria-hidden="true">
                /
              </span>
              <span>{meta.activeProject.name}</span>
            </>
          )}
          {meta.activeSession && (
            <>
              <span className="breadcrumbs__sep" aria-hidden="true">
                /
              </span>
              <span className="breadcrumbs__active">{meta.activeSession.title}</span>
            </>
          )}
        </nav>
      </div>

      <label className="titlebar__search" aria-label="Поиск чатов и артефактов">
        <SearchIcon />
        <input
          ref={meta.searchInputRef}
          type="search"
          className="titlebar__search-input"
          placeholder="Поиск чатов и артефактов..."
          value={state.searchQuery}
          onChange={(e) => actions.setSearchQuery(e.target.value)}
        />
        <span className="titlebar__kbd" aria-hidden="true">
          Ctrl K
        </span>
      </label>

      <div className="titlebar__right">
        <RuntimeBadge
          initialized={state.initialized}
          healthSnapshot={state.healthSnapshot}
          binding={meta.activeSession?.runtime_binding ?? null}
        />

        <button
          type="button"
          className={
            state.isRightPanelOpen ? "icon-btn icon-btn--active" : "icon-btn"
          }
          onClick={actions.toggleRightPanel}
          aria-label="Переключить панель рабочего пространства и артефактов"
          aria-expanded={state.isRightPanelOpen}
          title="Рабочее пространство и артефакты"
        >
          <PanelRightIcon />
        </button>

        <div className="window-controls" aria-label="Управление окном">
          <button
            type="button"
            className="window-btn"
            onClick={() => void meta.windowControls.minimize()}
            aria-label="Свернуть окно"
          >
            <MinimizeIcon />
          </button>
          <button
            type="button"
            className="window-btn"
            onClick={() => void meta.windowControls.toggleMaximize()}
            aria-label="Развернуть или восстановить окно"
          >
            <MaximizeIcon />
          </button>
          <button
            type="button"
            className="window-btn window-btn--close"
            onClick={() => void meta.windowControls.close()}
            aria-label="Закрыть окно"
            title="Закрыть окно (демоны Head и memoryd продолжат работу)"
          >
            <CloseIcon />
          </button>
        </div>
      </div>
    </header>
  );
}
