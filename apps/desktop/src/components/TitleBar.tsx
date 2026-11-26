import { useRef, useState } from "react";
import {
  MagnifyingGlass,
  Minus,
  Octagon,
  Sidebar,
  SidebarSimple,
  Square,
  X,
} from "@phosphor-icons/react";

import { useWorkspace } from "../state/WorkspaceProvider";
import { PortalPopover } from "./PortalPopover";
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
    return { text: "Симуляция", tone: "warn" };
  }
  if (authProfile === "local-endpoint") {
    return {
      text: tier && tier.trim().length > 0 ? `Локальный · ${tier.trim()}` : "Локальный сервер",
      tone: "ok",
    };
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
  const statsTriggerRef = useRef<HTMLButtonElement | null>(null);
  const emergencyTriggerRef = useRef<HTMLButtonElement | null>(null);
  const [editingBreadcrumb, setEditingBreadcrumb] = useState(false);
  const [breadcrumbTitleDraft, setBreadcrumbTitleDraft] = useState("");
  const [emergencyConfirmOpen, setEmergencyConfirmOpen] = useState(false);

  const health = state.healthSnapshot?.health;
  const connected = Boolean(state.healthSnapshot?.connected && health);
  const hasAnyActiveWork = state.runningSessionIds.length > 0 || meta.activeTaskCount > 0;
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

  // D2-UX-01: Exclude interactive controls & search capsule; handle double-click via event.detail >= 2!
  const handleDragStart = (event: React.MouseEvent<HTMLElement>) => {
    if (event.button !== 0 || !meta.windowControls.supported) {
      return;
    }
    const target = event.target as HTMLElement | null;
    if (
      target?.closest(
        "button, input, textarea, .command-search, .stats-anchor, .window-controls, .breadcrumb-title-btn, [role='dialog'], [role='menu']",
      )
    ) {
      return;
    }
    if (event.detail >= 2) {
      event.preventDefault();
      void meta.windowControls.toggleMaximize();
      return;
    }
    void meta.windowControls.startDragging();
  };

  const startBreadcrumbRename = () => {
    if (!meta.activeSession || !connected || meta.isActiveProjectArchived) {
      return;
    }
    setBreadcrumbTitleDraft(meta.activeSession.title);
    setEditingBreadcrumb(true);
  };

  const commitBreadcrumbRename = () => {
    const sess = meta.activeSession;
    const next = breadcrumbTitleDraft.trim();
    setEditingBreadcrumb(false);
    if (sess && next && next !== sess.title) {
      void actions.renameSession(sess.session_id, next);
    }
  };

  const searchPlaceholder =
    state.activeView === "inbox"
      ? "Фильтр по Входящим и Радару…"
      : state.activeView === "tasks"
        ? "Фильтр по поручениям…"
        : state.activeView === "memory"
          ? "Поиск по памяти и заметкам…"
          : "Поиск по чатам и проектам…";

  return (
    <header
      className="titlebar"
      onMouseDown={handleDragStart}
      data-testid="desktop-titlebar"
    >
      <div className="titlebar__left">
        <button
          type="button"
          className={`icon-button ${meta.isSidebarEffectiveOpen ? "is-active" : ""}`}
          onClick={actions.toggleSidebar}
          aria-label="Переключить боковую панель проектов"
          aria-expanded={meta.isSidebarEffectiveOpen}
          title="Проекты и чаты (Ctrl+B)"
        >
          <SidebarSimple size={17} />
        </button>

        <nav className="breadcrumbs" aria-label="Текущий контекст">
          {state.activeView === "inbox" ? (
            <>
              <span className="breadcrumb-prefix">Dennett</span>
              <span className="breadcrumb-sep" aria-hidden="true">
                /
              </span>
              <strong>Входящие и Радар</strong>
            </>
          ) : state.activeView === "tasks" ? (
            <>
              <span className="breadcrumb-prefix">Dennett</span>
              <span className="breadcrumb-sep" aria-hidden="true">
                /
              </span>
              <strong>Поручения и расписания</strong>
            </>
          ) : state.activeView === "memory" ? (
            <>
              <span className="breadcrumb-prefix">Dennett</span>
              <span className="breadcrumb-sep" aria-hidden="true">
                /
              </span>
              <strong>Память</strong>
            </>
          ) : meta.isStandaloneActiveSession && meta.activeSession ? (
            <>
              <span className="breadcrumb-prefix">Чаты</span>
              <span className="breadcrumb-sep" aria-hidden="true">
                /
              </span>
              {editingBreadcrumb ? (
                <input
                  type="text"
                  className="inline-rename-input breadcrumb-rename-input"
                  value={breadcrumbTitleDraft}
                  autoFocus
                  aria-label="Переименовать текущий чат"
                  data-testid="breadcrumb-rename-input"
                  onFocus={(e) => e.currentTarget.select()}
                  onChange={(e) => setBreadcrumbTitleDraft(e.target.value)}
                  onBlur={commitBreadcrumbRename}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      commitBreadcrumbRename();
                    } else if (e.key === "Escape") {
                      e.preventDefault();
                      e.stopPropagation();
                      setEditingBreadcrumb(false);
                    }
                  }}
                />
              ) : (
                <button
                  type="button"
                  className="breadcrumb-title-btn"
                  onClick={startBreadcrumbRename}
                  disabled={!connected}
                  data-testid="breadcrumb-session-title"
                  title="Нажмите, чтобы переименовать чат"
                >
                  <strong>{meta.activeSession.title}</strong>
                </button>
              )}
            </>
          ) : (
            <>
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
                    {meta.isActiveProjectArchived ? " (архив)" : ""}
                  </span>
                </>
              )}
              {meta.activeSession && (
                <>
                  <span className="breadcrumb-sep" aria-hidden="true">
                    /
                  </span>
                  {editingBreadcrumb ? (
                    <input
                      type="text"
                      className="inline-rename-input breadcrumb-rename-input"
                      value={breadcrumbTitleDraft}
                      autoFocus
                      aria-label="Переименовать текущий чат"
                      data-testid="breadcrumb-rename-input"
                      onFocus={(e) => e.currentTarget.select()}
                      onChange={(e) => setBreadcrumbTitleDraft(e.target.value)}
                      onBlur={commitBreadcrumbRename}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") {
                          e.preventDefault();
                          commitBreadcrumbRename();
                        } else if (e.key === "Escape") {
                          e.preventDefault();
                          e.stopPropagation();
                          setEditingBreadcrumb(false);
                        }
                      }}
                    />
                  ) : (
                    <button
                      type="button"
                      className="breadcrumb-title-btn"
                      onClick={startBreadcrumbRename}
                      disabled={!connected || meta.isActiveProjectArchived}
                      data-testid="breadcrumb-session-title"
                      title={
                        meta.isActiveProjectArchived
                          ? "Архивный проект (режим чтения)"
                          : "Нажмите, чтобы переименовать чат"
                      }
                    >
                      <strong>{meta.activeSession.title}</strong>
                    </button>
                  )}
                </>
              )}
            </>
          )}
        </nav>
      </div>

      <div className="titlebar__center">
        <div
          className="command-search"
          onMouseDown={(e) => e.stopPropagation()}
          onClick={(e) => {
            if (!(e.target as HTMLElement).closest("button")) {
              meta.searchInputRef.current?.focus();
            }
          }}
        >
          <MagnifyingGlass size={14} aria-hidden="true" />
          <input
            ref={meta.searchInputRef}
            type="search"
            value={state.searchQuery}
            onChange={(e) => {
              const nextValue = e.target.value;
              actions.setSearchQuery(nextValue);
              if (
                state.activeView === "chat" &&
                !state.isSidebarOpen &&
                nextValue.trim().length > 0
              ) {
                actions.setSidebarOpen(true);
              }
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                actions.setCommandOpen(true, state.searchQuery);
              }
            }}
            placeholder={searchPlaceholder}
            aria-label="Поиск по чатам и проектам"
          />
          <button
            type="button"
            className="command-kbd-btn"
            onClick={() => actions.setCommandOpen(true, state.searchQuery)}
            aria-label="Открыть командную палитру"
            title="Командная палитра (Ctrl+K)"
          >
            <kbd>Ctrl K</kbd>
          </button>
        </div>
      </div>

      <div className="titlebar__right">
        {connected && hasAnyActiveWork && (
          <div className="stats-anchor">
            <button
              ref={emergencyTriggerRef}
              type="button"
              className="emergency-stop-pill"
              data-testid="emergency-stop-trigger"
              disabled={state.isEmergencyStopping}
              aria-expanded={emergencyConfirmOpen}
              aria-haspopup="dialog"
              title="Экстренно остановить все активные чаты и поручения"
              onClick={() => setEmergencyConfirmOpen((prev) => !prev)}
            >
              <Octagon size={13} weight="fill" aria-hidden="true" />
              <span>Стоп всё</span>
            </button>
            <PortalPopover
              isOpen={emergencyConfirmOpen}
              anchorEl={emergencyTriggerRef.current}
              onClose={() => setEmergencyConfirmOpen(false)}
              placement="bottom-end"
              className="confirm-popover"
              role="dialog"
              ariaLabel="Подтверждение экстренной остановки"
              testId="emergency-stop-popover"
            >
              <strong>Остановить все активные запуски?</strong>
              <p>
                Будет отправлен сигнал отмены во все запущенные чаты и активные фоновые поручения.
              </p>
              <div className="confirm-popover__actions">
                <button
                  type="button"
                  className="d2-danger-btn d2-danger-btn--sm"
                  data-testid="emergency-stop-confirm-btn"
                  disabled={state.isEmergencyStopping}
                  onClick={() => {
                    setEmergencyConfirmOpen(false);
                    void actions.emergencyStopAll();
                  }}
                >
                  Остановить всё
                </button>
                <button
                  type="button"
                  className="popover-secondary-btn"
                  onClick={() => setEmergencyConfirmOpen(false)}
                >
                  Отмена
                </button>
              </div>
            </PortalPopover>
          </div>
        )}

        <div className="stats-anchor">
          <button
            ref={statsTriggerRef}
            type="button"
            className={`status-pill status-pill--${dotTone} ${
              connected && isSimulated ? "status-pill--simulated" : ""
            } ${state.isStatsOpen ? "is-open" : ""}`}
            onClick={() => actions.setStatsOpen(!state.isStatsOpen)}
            aria-expanded={state.isStatsOpen}
            aria-haspopup="dialog"
            data-testid="system-status-pill"
            title={
              connected && isSimulated
                ? "Тестовый двойник (SimulatedRuntime), не живой agy acp — нажмите для деталей"
                : "Состояние системы, движка и памяти"
            }
          >
            <span className={`status-dot status-dot--${dotTone}`} aria-hidden="true" />
            <span className="status-pill__text" data-testid="runtime-status-pill">
              {connected && isSimulated ? (
                <span data-testid="simulated-runtime-badge">Симуляция</span>
              ) : (
                statusInfo.text
              )}
            </span>
          </button>

          <PortalPopover
            isOpen={state.isStatsOpen}
            anchorEl={statsTriggerRef.current}
            onClose={() => actions.setStatsOpen(false)}
            placement="bottom-end"
            className="stats-popover"
            role="dialog"
            ariaLabel="Состояние системы и памяти"
            testId="stats-popover"
          >
            <StatsPopover onClose={() => actions.setStatsOpen(false)} />
          </PortalPopover>
        </div>

        <button
          type="button"
          className={`icon-button ${state.isRightPanelOpen ? "is-active" : ""}`}
          onClick={actions.toggleRightPanel}
          aria-label="Переключить панель рабочего пространства и артефактов"
          aria-expanded={state.isRightPanelOpen}
          data-testid="titlebar-toggle-workspace"
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
