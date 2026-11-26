import { useState } from "react";
import {
  ArrowsClockwise,
  Brain,
  Database,
  ShieldCheck,
  Sparkle,
  WarningCircle,
} from "@phosphor-icons/react";

import type { ConsolidationJobStatus, IndexStatus } from "../api/contracts";
import { useWorkspace } from "../state/WorkspaceProvider";
import { formatContextUsage } from "../utils/formatters";

export interface StatsPopoverProps {
  onClose(): void;
}

function consolidationStatusLabel(status: ConsolidationJobStatus): string {
  switch (status) {
    case "pending":
      return "ожидает";
    case "running":
      return "выполняется";
    case "paused":
      return "приостановлено";
    case "completed":
      return "завершено";
    case "failed":
      return "ошибка";
    case "cancelled":
      return "отменено";
  }
}

function indexStatusLabel(status: IndexStatus): { text: string; className: string } {
  switch (status) {
    case "healthy":
      return { text: "Актуален", className: "text-ok" };
    case "lagging":
      return { text: "Догоняет", className: "text-warn" };
    case "rebuilding":
      return { text: "Перестроение", className: "text-warn" };
    case "degraded":
      return { text: "Деградирован", className: "text-warn" };
  }
}

export function StatsPopover({ onClose }: StatsPopoverProps) {
  const { state, actions, meta } = useWorkspace();
  const [actionFeedback, setActionFeedback] = useState<string | null>(null);

  const health = state.healthSnapshot?.health;
  const catalog = state.runtimeCatalog;
  const binding = state.activeDetail?.session.runtime_binding;
  const connected = Boolean(state.healthSnapshot?.connected);

  const isSimulated = Boolean(health?.runtime_is_simulated ?? catalog?.is_simulated);
  const agyVersion = catalog?.agy_version ?? binding?.agy_version;
  const engineTitle = isSimulated
    ? "Simulated Runtime (CI)"
    : catalog?.auth_profile === "local-endpoint"
      ? catalog.adapter_id || "Local HTTP Runtime"
      : agyVersion
        ? `Antigravity ACP ${agyVersion}`
        : "Antigravity ACP";

  const activeConnection =
    state.providerConnections.find(
      (c) => c.connection_id === meta.activeAgent?.current_config.preferred_connection_id,
    ) ??
    state.providerConnections.find((c) => c.is_default_interactive) ??
    null;

  const tierText =
    activeConnection?.display_name ??
    catalog?.tier ??
    binding?.tier ??
    (catalog?.auth_profile === "local-endpoint"
      ? "Локальный сервер"
      : catalog?.auth_profile === "gemini-api-key"
        ? "Gemini API key"
        : "Google AI Ultra");

  const quotasText =
    catalog && catalog.quotas.length > 0
      ? catalog.quotas.map((q) => `${q.label}: ${q.remaining_percent}`).join(" · ")
      : "—";

  const memoryHealthy = health?.memory_status === "healthy";
  const pendingOutbox = health?.pending_outbox_count ?? 0;
  const runs = state.activeDetail?.runs ?? [];
  const runsCount = runs.length;
  const policyCount = state.activeDetail?.permission_decisions.length ?? 0;

  let contextUsedTokens: number | null = null;
  let contextWindowTokens: number | null = null;
  if (
    meta.activeLiveRun?.context_used_tokens != null ||
    meta.activeLiveRun?.context_window_tokens != null
  ) {
    contextUsedTokens = meta.activeLiveRun.context_used_tokens ?? null;
    contextWindowTokens = meta.activeLiveRun.context_window_tokens ?? null;
  } else if (runs.length > 0) {
    let latestWithContext: (typeof runs)[number] | null = null;
    for (const run of runs) {
      if (run.context_used_tokens == null && run.context_window_tokens == null) {
        continue;
      }
      if (!latestWithContext || run.created_at >= latestWithContext.created_at) {
        latestWithContext = run;
      }
    }
    if (latestWithContext) {
      contextUsedTokens = latestWithContext.context_used_tokens ?? null;
      contextWindowTokens = latestWithContext.context_window_tokens ?? null;
    }
  }
  const contextUsageLabel = formatContextUsage(contextUsedTokens, contextWindowTokens);

  const memoryStats =
    state.memoryOverview?.memory_status === "healthy" ? (state.memoryOverview.stats ?? null) : null;
  const indexBadge = memoryStats ? indexStatusLabel(memoryStats.index_status) : null;
  const openConflictsCount = meta.openMemoryConflictsCount;
  const notesDisplayValue =
    state.activeScopeNoteCount !== null
      ? state.activeScopeNoteCount
      : memoryStats
        ? memoryStats.active_notes_count
        : "—";

  const targetScope = meta.activeProject?.memory_scope ?? "chat:standalone";

  const handleRebuildIndex = async () => {
    setActionFeedback(null);
    const resp = await actions.triggerMemoryIndexRebuild(targetScope);
    if (resp) {
      setActionFeedback(`Индекс пересобран: ${resp.entries_rebuilt} зап.`);
    }
  };

  const handleConsolidate = async () => {
    setActionFeedback(null);
    const job = await actions.createMemoryConsolidationJob(targetScope, "dossier_update");
    if (job) {
      setActionFeedback(`Консолидация: ${consolidationStatusLabel(job.status)}`);
    }
  };

  const openMemoryView = () => {
    onClose();
    actions.setActiveView("memory");
  };

  return (
    <div className="stats-popover__inner">
      <div className="popover-header">
        <strong>Состояние системы и памяти</strong>
        <button
          type="button"
          className="popover-action-link"
          onClick={() => {
            void actions.refreshAll();
            void actions.refreshMemoryStats();
            void actions.refreshMemoryOverview(null);
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
        {catalog?.quota_error && (
          <div className="stats-quota-warn" data-testid="stats-quota-error">
            <WarningCircle size={13} aria-hidden="true" />
            <span>
              Квота {catalog.quota_error.provider}: {catalog.quota_error.detail}
            </span>
          </div>
        )}
      </section>

      <section className="stats-section">
        <div className="stats-section__title">
          <Database size={14} aria-hidden="true" />
          <span>Память memoryd</span>
        </div>
        <div className="stats-kv">
          <span>Статус</span>
          <strong className={memoryHealthy ? "text-ok" : "text-warn"}>
            {!connected ? "Нет связи с Head" : memoryHealthy ? "Активна" : "Недоступна"}
          </strong>
        </div>
        {indexBadge && (
          <div className="stats-kv">
            <span>Индекс</span>
            <strong className={indexBadge.className} data-testid="stats-memory-index-status">
              {indexBadge.text}
            </strong>
          </div>
        )}
        {memoryStats && (
          <div className="stats-kv">
            <span>Источники</span>
            <strong data-testid="stats-memory-sources-count">{memoryStats.sources_count}</strong>
          </div>
        )}
        {(meta.activeProject || memoryStats) && (
          <div className="stats-kv">
            <span>{meta.activeProject ? "Заметки в проекте" : "Заметки"}</span>
            <strong data-testid="stats-memory-notes-count">{notesDisplayValue}</strong>
          </div>
        )}
        {memoryStats && (
          <>
            <div className="stats-kv">
              <span>Утверждения</span>
              <strong data-testid="stats-memory-claims-count">
                {memoryStats.active_claims_count}
              </strong>
            </div>
            <div className="stats-kv">
              <span>Досье</span>
              <strong data-testid="stats-memory-docs-count">{memoryStats.documents_count}</strong>
            </div>
            <div className="stats-kv">
              <span>Открытые конфликты</span>
              <button
                type="button"
                className={`stats-conflict-link ${openConflictsCount > 0 ? "text-warn" : ""}`}
                onClick={openMemoryView}
                data-testid="stats-memory-conflicts-link"
              >
                {openConflictsCount}
              </button>
            </div>
          </>
        )}
        <div className="stats-kv">
          <span>Очередь outbox</span>
          <strong>{pendingOutbox}</strong>
        </div>

        {(pendingOutbox > 0 || !memoryHealthy) && connected && (
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

        {memoryHealthy && connected && (
          <div className="stats-actions-row">
            <button
              type="button"
              className="popover-secondary-btn stats-mini-btn"
              onClick={() => void handleRebuildIndex()}
              disabled={state.isRebuildingIndex}
              data-testid="stats-rebuild-index-btn"
            >
              <ArrowsClockwise
                size={12}
                className={state.isRebuildingIndex ? "spin" : ""}
                aria-hidden="true"
              />
              {state.isRebuildingIndex ? "Индекс…" : "Пересобрать индекс"}
            </button>
            <button
              type="button"
              className="popover-secondary-btn stats-mini-btn"
              onClick={() => void handleConsolidate()}
              disabled={state.isConsolidatingMemory}
              data-testid="stats-consolidate-btn"
            >
              <Brain size={12} aria-hidden="true" />
              {state.isConsolidatingMemory ? "Запуск…" : "Консолидировать"}
            </button>
          </div>
        )}

        {actionFeedback && (
          <p className="stats-note" data-testid="stats-action-feedback">
            {actionFeedback}
          </p>
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
        {contextUsageLabel && (
          <div className="stats-kv" data-testid="stats-context-usage">
            <span>Контекст</span>
            <strong>{contextUsageLabel}</strong>
          </div>
        )}
        <div className="stats-kv">
          <span>Решений политики</span>
          <strong>{policyCount}</strong>
        </div>
      </section>

      <div className="stats-footer">
        <button
          type="button"
          className="popover-secondary-btn"
          onClick={openMemoryView}
          data-testid="stats-open-memory-btn"
        >
          Открыть раздел «Память»
        </button>
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
