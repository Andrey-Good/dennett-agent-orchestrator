import {
  ArrowsClockwise,
  CheckCircle,
  CircleNotch,
  ShieldCheck,
  Stop,
  WarningCircle,
  X,
} from "@phosphor-icons/react";

import { useWorkspace } from "../state/WorkspaceProvider";

export function StatusBanners() {
  const { state, actions, meta } = useWorkspace();

  const health = state.healthSnapshot?.health;
  const isMemoryDegraded =
    Boolean(state.healthSnapshot?.connected) && health?.memory_status === "degraded_unreachable";
  const pendingOutbox = health?.pending_outbox_count ?? 0;
  const isActiveRunRunning = meta.isRunningActiveSession;
  const isAwaitingApproval = meta.isAwaitingApprovalActiveSession;
  const isCancelling = meta.isCancellingActiveSession;

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
              ждут в очереди, Стоп доступен.
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

      {isActiveRunRunning && (
        <div
          className={`status-banner ${
            isAwaitingApproval && !isCancelling
              ? "status-banner--approval"
              : "status-banner--running"
          }`}
          role="status"
          aria-live="polite"
          data-testid="running-control-bar"
          data-state={
            isCancelling
              ? "cancelling"
              : isAwaitingApproval
                ? "awaiting-approval"
                : "running"
          }
        >
          <div className="status-banner__copy">
            {isAwaitingApproval && !isCancelling ? (
              <ShieldCheck size={15} aria-hidden="true" />
            ) : (
              <CircleNotch size={14} className="spin" aria-hidden="true" />
            )}
            <span>
              {isCancelling
                ? "Отправлена команда остановки…"
                : isAwaitingApproval
                  ? "Ожидает подтверждения — агент приостановлен до вашего решения."
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
            {isCancelling ? "Остановка…" : "Стоп"}
          </button>
        </div>
      )}

      {state.reconcileNotice && (
        <div
          className={`status-banner ${
            state.reconcileNotice.tone === "ok"
              ? "status-banner--ok"
              : state.reconcileNotice.tone === "warn"
                ? "status-banner--warning"
                : "status-banner--neutral"
          }`}
          role="status"
          data-testid="reconcile-verdict-banner"
        >
          <div className="status-banner__copy">
            {state.reconcileNotice.tone === "ok" ? (
              <CheckCircle size={15} aria-hidden="true" />
            ) : (
              <WarningCircle size={15} aria-hidden="true" />
            )}
            <span>{state.reconcileNotice.text}</span>
          </div>
          <button
            type="button"
            className="icon-button icon-button--xs"
            onClick={actions.dismissReconcileNotice}
            aria-label="Скрыть вердикт сверки"
          >
            <X size={13} />
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
