import { useEffect, useRef } from "react";
import {
  MagnifyingGlass,
  Minus,
  Sidebar,
  SidebarSimple,
  Square,
  X,
} from "@phosphor-icons/react";

import { useWorkspace } from "../state/WorkspaceProvider";
import { StatsPopover } from "./StatsPopover";

function resolveStatusLabel(
  connected: boolean,
  isSimulated: boolean,
  tier: string | null | undefined,
  authProfile: string | undefined,
): { text: string; tone: "ok" | "warn" | "offline" } {
  if (!connected) {
    return { text: "Head офлайн", tone: "offline" };
  }
  if (isSimulated) {
    return { text: "Simulated", tone: "warn" };
  }
  if (authProfile === "gemini-api-key") {
    return { text: "Antigravity ACP · Gemini API key", tone: "ok" };
  }
  if (tier && tier.trim().length > 0) {
    return { text: `Antigravity ACP · ${tier.trim()}`, tone: "ok" };
  }
  return { text: "Antigravity ACP · Google AI Ultra", tone: "ok" };
}

export function TitleBar() {
  const { state, actions, meta } = useWorkspace();
  const statsAnchorRef = useRef<HTMLDivElement | null>(null);

  const health = state.healthSnapshot?.health;
  const connected = Boolean(state.healthSnapshot?.connected && health);
  const catalog = state.runtimeCatalog;
  const binding = state.activeDetail?.session.runtime_binding;

  const isSimulated = Boolean(
    health?.runtime_is_simulated ?? catalog?.is_simulated ?? binding?.is_simulated,
  );
  const authProfile = binding?.auth_profile ?? catalog?.auth_profile;
  const tier = binding?.tier ?? catalog?.tier;
  const isMemoryDegraded = connected && health?.memory_status === "degraded_unreachable";

  const statusInfo = resolveStatusLabel(connected, isSimulated, tier, authProfile);
  const dotTone = !connected ? "offline" : isMemoryDegraded ? "warn" : statusInfo.tone;

  useEffect(() => {
    if (!state.isStatsOpen) {
      return;
    }
    const handlePointerDown = (event: MouseEvent) => {
      if (statsAnchorRef.current && !statsAnchorRef.current.contains(event.target as Node)) {
        actions.setStatsOpen(false);
      }
    };
    document.addEventListener("mousedown", handlePointerDown);
    return () => document.removeEventListener("mousedown", handlePointerDown);
  }, [actions, state.isStatsOpen]);

  const handleDragStart = (event: React.MouseEvent<HTMLElement>) => {
    if (event.button !== 0 || !meta.windowControls.supported) {
      return;
    }
    const target = event.target as HTMLElement | null;
    if (target?.closest("button, input, textarea, [role='dialog']")) {
      return;
    }
    void meta.windowControls.startDragging();
  };

  return (
    <header
      className="titlebar"
      onMouseDown={handleDragStart}
      onDoubleClick={(e) => {
        const target = e.target as HTMLElement | null;
        if (meta.windowControls.supported && !target?.closest("button, input")) {
          void meta.windowControls.toggleMaximize();
        }
      }}
      data-testid="desktop-titlebar"
    >
      <div className="titlebar__left">
        <button
          type="button"
          className={`icon-button ${state.isSidebarOpen ? "is-active" : ""}`}
          onClick={actions.toggleSidebar}
          aria-label="Переключить боковую панель проектов"
          aria-expanded={state.isSidebarOpen}
          title="Проекты и чаты (Ctrl+B)"
        >
          <SidebarSimple size={17} />
        </button>

        <nav className="breadcrumbs" aria-label="Текущий контекст">
          <span className={meta.activeSession ? "breadcrumb-prefix" : ""}>Проекты</span>
          {meta.activeProject && (
            <>
              <span className="breadcrumb-sep" aria-hidden="true">
                /
              </span>
              <span
                className={meta.activeSession ? "breadcrumb-prefix" : ""}
                title={meta.activeProject.workspace_path ?? "Пока без папки"}
              >
                {meta.activeProject.name}
              </span>
            </>
          )}
          {meta.activeSession && (
            <>
              <span className="breadcrumb-sep" aria-hidden="true">
                /
              </span>
              <strong>{meta.activeSession.title}</strong>
            </>
          )}
        </nav>
      </div>

      <div className="titlebar__center">
        <div className="command-search">
          <MagnifyingGlass size={14} aria-hidden="true" />
          <input
            ref={meta.searchInputRef}
            type="search"
            value={state.searchQuery}
            onChange={(e) => {
              const nextValue = e.target.value;
              actions.setSearchQuery(nextValue);
              if (!state.isSidebarOpen && nextValue.trim().length > 0) {
                actions.setSidebarOpen(true);
              }
            }}
            placeholder="Поиск по чатам и проектам…"
            aria-label="Поиск по чатам и проектам"
          />
          <button
            type="button"
            className="command-kbd-btn"
            onClick={() => actions.setCommandOpen(true)}
            aria-label="Открыть командную палитру"
            title="Командная палитра (Ctrl+K)"
          >
            <kbd>Ctrl K</kbd>
          </button>
        </div>
      </div>

      <div className="titlebar__right">
        <div className="stats-anchor" ref={statsAnchorRef}>
          <button
            type="button"
            className={`status-pill status-pill--${dotTone} ${state.isStatsOpen ? "is-open" : ""}`}
            onClick={() => actions.setStatsOpen(!state.isStatsOpen)}
            aria-expanded={state.isStatsOpen}
            aria-haspopup="dialog"
            data-testid="system-status-pill"
            title="Состояние системы, движка и памяти"
          >
            <span className={`status-dot status-dot--${dotTone}`} aria-hidden="true" />
            <span className="status-pill__text" data-testid="runtime-status-pill">
              {statusInfo.text}
            </span>
          </button>

          {connected && isSimulated && (
            <span
              className="simulated-pill"
              data-testid="simulated-runtime-badge"
              title="Тестовый двойник (SimulatedRuntime), не живой agy acp"
            >
              SIMULATED
            </span>
          )}

          {state.isStatsOpen && <StatsPopover onClose={() => actions.setStatsOpen(false)} />}
        </div>

        <button
          type="button"
          className={`icon-button ${state.isRightPanelOpen ? "is-active" : ""}`}
          onClick={actions.toggleRightPanel}
          aria-label="Переключить панель рабочего пространства и артефактов"
          aria-expanded={state.isRightPanelOpen}
          title="Рабочее пространство (Ctrl+\)"
        >
          <Sidebar size={17} />
        </button>

        {meta.windowControls.supported && (
          <div className="window-controls" aria-label="Управление окном">
            <button
              type="button"
              className="icon-button"
              onClick={() => void meta.windowControls.minimize()}
              aria-label="Свернуть окно"
            >
              <Minus size={14} />
            </button>
            <button
              type="button"
              className="icon-button"
              onClick={() => void meta.windowControls.toggleMaximize()}
              aria-label="Развернуть или восстановить окно"
            >
              <Square size={12} />
            </button>
            <button
              type="button"
              className="icon-button window-close"
              onClick={() => void meta.windowControls.close()}
              aria-label="Закрыть окно"
              title="Закрыть окно (dennett-head и memoryd продолжат работу)"
            >
              <X size={14} />
            </button>
          </div>
        )}
      </div>
    </header>
  );
}
