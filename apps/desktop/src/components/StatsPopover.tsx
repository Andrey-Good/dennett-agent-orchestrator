import { ArrowsClockwise, Database, ShieldCheck, Sparkle } from "@phosphor-icons/react";

import { useWorkspace } from "../state/WorkspaceProvider";

export interface StatsPopoverProps {
  onClose(): void;
}

export function StatsPopover({ onClose }: StatsPopoverProps) {
  const { state, actions, meta } = useWorkspace();
  const health = state.healthSnapshot?.health;
  const catalog = state.runtimeCatalog;
  const binding = state.activeDetail?.session.runtime_binding;

  const isSimulated = Boolean(health?.runtime_is_simulated ?? catalog?.is_simulated);
  const agyVersion = catalog?.agy_version ?? binding?.agy_version;
  const engineTitle = isSimulated
    ? "Simulated Runtime (CI)"
    : agyVersion
      ? `Antigravity ACP ${agyVersion}`
      : "Antigravity ACP";

  const tierText =
    catalog?.tier ??
    binding?.tier ??
    (catalog?.auth_profile === "gemini-api-key" ? "Gemini API key" : "Google AI Ultra");

  const quotasText =
    catalog && catalog.quotas.length > 0
      ? catalog.quotas.map((q) => `${q.label}: ${q.remaining_percent}`).join(" · ")
      : "—";

  const memoryHealthy = health?.memory_status === "healthy";
  const pendingOutbox = health?.pending_outbox_count ?? 0;
  const runsCount = state.activeDetail?.runs.length ?? 0;
  const policyCount = state.activeDetail?.permission_decisions.length ?? 0;

  return (
    <div
      className="floating-popover stats-popover"
      role="dialog"
      aria-label="Состояние системы и памяти"
      data-testid="stats-popover"
    >
      <div className="popover-header">
        <strong>Состояние системы и памяти</strong>
        <button
          type="button"
          className="popover-action-link"
          onClick={() => {
            void actions.refreshAll();
            void actions.refreshMemoryStats();
          }}
        >
          Обновить
        </button>
      </div>

      <section className="stats-section">
        <div className="stats-section__title">
          <Sparkle size={14} aria-hidden="true" />
          <span>Движок</span>
        </div>
        <div className="stats-kv">
          <span>Адаптер</span>
          <strong>{engineTitle}</strong>
        </div>
        <div className="stats-kv">
          <span>Профиль / тариф</span>
          <strong>{tierText}</strong>
        </div>
        <div className="stats-kv">
          <span>Квоты</span>
          <strong>{quotasText}</strong>
        </div>
      </section>

      <section className="stats-section">
        <div className="stats-section__title">
          <Database size={14} aria-hidden="true" />
          <span>Память memoryd</span>
        </div>
        <div className="stats-kv">
          <span>Статус</span>
          <strong className={memoryHealthy ? "text-ok" : "text-warn"}>
            {!state.healthSnapshot?.connected
              ? "Нет связи с Head"
              : memoryHealthy
                ? "Активна"
                : "Недоступна"}
          </strong>
        </div>
        {meta.activeProject && (
          <div className="stats-kv">
            <span>Заметки в проекте</span>
            <strong data-testid="stats-memory-notes-count">
              {state.activeScopeNoteCount !== null ? state.activeScopeNoteCount : "—"}
            </strong>
          </div>
        )}
        <div className="stats-kv">
          <span>Очередь outbox</span>
          <strong>{pendingOutbox}</strong>
        </div>

        {(pendingOutbox > 0 || !memoryHealthy) && state.healthSnapshot?.connected && (
          <button
            type="button"
            className="popover-primary-btn"
            onClick={() => void actions.flushMemoryOutbox()}
            disabled={state.isFlushingOutbox}
            data-testid="stats-flush-outbox-btn"
          >
            <ArrowsClockwise
              size={14}
              className={state.isFlushingOutbox ? "spin" : ""}
              aria-hidden="true"
            />
            {state.isFlushingOutbox ? "Отправка очереди…" : "Отправить очередь"}
          </button>
        )}
        {state.lastFlushSummary && <p className="stats-note">{state.lastFlushSummary}</p>}
      </section>

      <section className="stats-section">
        <div className="stats-section__title">
          <ShieldCheck size={14} aria-hidden="true" />
          <span>Сессия и безопасность</span>
        </div>
        <div className="stats-kv">
          <span>Ходов в сессии</span>
          <strong>{runsCount}</strong>
        </div>
        <div className="stats-kv">
          <span>Решений политики</span>
          <strong>{policyCount}</strong>
        </div>
      </section>

      <div className="stats-footer">
        <button
          type="button"
          className="popover-secondary-btn"
          onClick={() => {
            onClose();
            actions.setSettingsOpen(true);
          }}
        >
          Открыть настройки и подключения…
        </button>
      </div>
    </div>
  );
}
