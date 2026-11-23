import { useWorkspace } from "../state/WorkspaceProvider";
import { RefreshIcon } from "./Icons";

export function StatusBanners() {
  const { state, actions } = useWorkspace();

  const health = state.healthSnapshot?.health ?? null;
  const isMemoryDegraded =
    Boolean(state.healthSnapshot?.connected) &&
    health?.memory_status === "degraded_unreachable";

  if (!isMemoryDegraded && !state.errorBanner && !state.cancelledNotice) {
    return null;
  }

  return (
    <div className="banners-stack">
      {isMemoryDegraded && (
        <div
          className="status-banner status-banner--warn"
          role="alert"
          data-testid="memory-outage-banner"
        >
          <div className="status-banner__body">
            <span className="status-banner__title">
              Память memoryd недоступна — разговор сохраняется, отложенные записи ждут в очереди, Stop доступен
            </span>
            <span className="status-banner__detail">
              В очереди outbox: {health?.pending_outbox_count ?? 0}
              {health?.memory_detail ? ` · ${health.memory_detail}` : ""}
              {state.lastFlushSummary ? ` · ${state.lastFlushSummary}` : ""}
            </span>
          </div>
          <div className="status-banner__actions">
            <button
              type="button"
              className="btn btn--warn"
              onClick={() => void actions.flushMemoryOutbox()}
              disabled={state.isFlushingOutbox}
            >
              <RefreshIcon />
              {state.isFlushingOutbox ? "Повтор..." : "Повторить отправку очереди"}
            </button>
          </div>
        </div>
      )}

      {state.cancelledNotice && (
        <div
          className="status-banner status-banner--info"
          role="status"
          data-testid="cancelled-run-notice"
        >
          <div className="status-banner__body">
            <span className="status-banner__title">{state.cancelledNotice}</span>
          </div>
          <div className="status-banner__actions">
            <button
              type="button"
              className="btn btn--ghost"
              onClick={actions.dismissCancelledNotice}
            >
              Скрыть
            </button>
          </div>
        </div>
      )}

      {state.errorBanner && (
        <div
          className="status-banner status-banner--danger"
          role="alert"
          data-testid="error-banner"
        >
          <div className="status-banner__body">
            <span className="status-banner__title">
              Ошибка ({state.errorBanner.code})
            </span>
            <span className="status-banner__detail">{state.errorBanner.message}</span>
          </div>
          <div className="status-banner__actions">
            <button
              type="button"
              className="btn btn--ghost"
              onClick={actions.dismissError}
            >
              Закрыть
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
