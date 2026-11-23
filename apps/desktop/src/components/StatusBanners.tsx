import { ArrowsClockwise, CircleNotch, Stop, WarningCircle, X } from "@phosphor-icons/react";

import { useWorkspace } from "../state/WorkspaceProvider";

export function StatusBanners() {
  const { state, actions } = useWorkspace();

  const health = state.healthSnapshot?.health;
  const isMemoryDegraded =
    Boolean(state.healthSnapshot?.connected) && health?.memory_status === "degraded_unreachable";
  const pendingOutbox = health?.pending_outbox_count ?? 0;
  const isAnyRunActive = state.runningSessionId !== null;
  const isCancelling = state.cancellingSessionId !== null;

  return (
    <div className="status-banners" data-testid="status-banners">
      {isMemoryDegraded && (
        <div
          className="status-banner status-banner--warning"
          role="alert"
          data-testid="memory-outage-banner"
        >
          <div className="status-banner__copy">
            <WarningCircle size={15} aria-hidden="true" />
            <span>
              <strong>Память memoryd недоступна</strong> — разговор сохраняется, отложенные записи
              ждут в очереди, Stop доступен.
              {pendingOutbox > 0 ? ` В очереди outbox: ${pendingOutbox}.` : ""}
            </span>
          </div>
          <button
            type="button"
            className="state-action"
            onClick={() => void actions.flushMemoryOutbox()}
            disabled={state.isFlushingOutbox}
          >
            <ArrowsClockwise
              size={12}
              className={state.isFlushingOutbox ? "spin" : ""}
              aria-hidden="true"
            />
            {state.isFlushingOutbox ? "Отправка…" : "Повторить отправку очереди"}
          </button>
        </div>
      )}

      {isAnyRunActive && (
        <div
          className="status-banner status-banner--running"
          role="status"
          aria-live="polite"
          data-testid="running-control-bar"
        >
          <div className="status-banner__copy">
            <CircleNotch size={14} className="spin" aria-hidden="true" />
            <span>
              {isCancelling
                ? "Отправлена команда остановки…"
                : "Агент выполняет поручение… Вы можете остановить ход в любой момент (Esc)."}
            </span>
          </div>
          <button
            type="button"
            className="state-action state-action--danger"
            onClick={() => void actions.stopActiveRun()}
            disabled={isCancelling}
            aria-label="Остановить выполнение"
            data-testid="global-stop-button"
          >
            <Stop size={12} weight="fill" aria-hidden="true" />
            {isCancelling ? "Остановка…" : "Stop"}
          </button>
        </div>
      )}

      {state.cancelledNotice && (
        <div
          className="status-banner status-banner--neutral"
          role="status"
          data-testid="cancelled-run-notice"
        >
          <span>{state.cancelledNotice}</span>
          <button
            type="button"
            className="icon-button icon-button--xs"
            onClick={actions.dismissCancelledNotice}
            aria-label="Скрыть уведомление"
          >
            <X size={13} />
          </button>
        </div>
      )}

      {state.errorBanner && (
        <div
          className="status-banner status-banner--danger"
          role="alert"
          data-testid="error-banner"
        >
          <div className="status-banner__copy">
            <WarningCircle size={15} aria-hidden="true" />
            <span>
              <strong>{state.errorBanner.code}</strong>: {state.errorBanner.message}
            </span>
          </div>
          <button
            type="button"
            className="icon-button icon-button--xs"
            onClick={actions.dismissError}
            aria-label="Закрыть сообщение об ошибке"
          >
            <X size={13} />
          </button>
        </div>
      )}
    </div>
  );
}
