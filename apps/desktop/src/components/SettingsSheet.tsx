import { useEffect, useRef, useState, type FormEvent } from "react";
import { ArrowsClockwise, Database, Key, PlugsConnected, X } from "@phosphor-icons/react";

import { useWorkspace } from "../state/WorkspaceProvider";

export function SettingsSheet() {
  const { state, actions } = useWorkspace();
  const [urlDraft, setUrlDraft] = useState(
    state.healthSnapshot?.head_url ?? "http://127.0.0.1:41771",
  );
  const dialogRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (state.healthSnapshot?.head_url) {
      setUrlDraft(state.healthSnapshot.head_url);
    }
  }, [state.healthSnapshot?.head_url]);

  if (!state.isSettingsOpen) {
    return null;
  }

  const health = state.healthSnapshot?.health;
  const catalog = state.runtimeCatalog;
  const isSimulated = Boolean(health?.runtime_is_simulated ?? catalog?.is_simulated);
  const authLabel = isSimulated
    ? "Simulated CI Runtime"
    : catalog?.auth_profile === "gemini-api-key"
      ? "Gemini API key · headless"
      : `${catalog?.tier ?? "Google AI Ultra"} · oauth-personal`;
  const engineLabel = isSimulated
    ? "SimulatedRuntime"
    : `Google Antigravity · agy acp ${catalog?.agy_version ?? "1.5+"}`;

  const handleReconnect = (event: FormEvent) => {
    event.preventDefault();
    const cleaned = urlDraft.trim();
    if (!cleaned) {
      return;
    }
    void actions.reconnectWithUrl(cleaned);
  };

  return (
    <div
      className="dialog-backdrop"
      onMouseDown={() => actions.setSettingsOpen(false)}
      data-testid="settings-backdrop"
    >
      <div
        ref={dialogRef}
        className="floating-dialog settings-sheet"
        role="dialog"
        aria-modal="true"
        aria-label="Настройки и подключения"
        data-testid="settings-sheet"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <header className="settings-sheet__header">
          <div>
            <h2>Настройки и подключения</h2>
            <p>Активный движок, сервер управления Dennett Head и память</p>
          </div>
          <button
            type="button"
            className="icon-button"
            onClick={() => actions.setSettingsOpen(false)}
            aria-label="Закрыть настройки"
          >
            <X size={16} />
          </button>
        </header>

        <div className="settings-sheet__body">
          <section className="settings-card">
            <div className="settings-card__head">
              <PlugsConnected size={16} aria-hidden="true" />
              <strong>Движок и авторизация</strong>
            </div>
            <div className="stats-kv">
              <span>Активный адаптер</span>
              <strong>{engineLabel}</strong>
            </div>
            <div className="stats-kv">
              <span>Профиль входа</span>
              <strong>{authLabel}</strong>
            </div>
            <div className="stats-kv">
              <span>Доступно моделей</span>
              <strong>{catalog ? catalog.models.length : "—"}</strong>
            </div>

            <div className="settings-actions">
              <button
                type="button"
                className="popover-primary-btn"
                onClick={() => void actions.refreshRuntimeCatalog(true)}
                disabled={state.isRefreshingCatalog || !state.healthSnapshot?.connected}
                data-testid="refresh-catalog-button"
              >
                <ArrowsClockwise
                  size={14}
                  className={state.isRefreshingCatalog ? "spin" : ""}
                  aria-hidden="true"
                />
                {state.isRefreshingCatalog
                  ? "Опрос движка…"
                  : "Обновить список моделей"}
              </button>
            </div>
          </section>

          <section className="settings-card">
            <div className="settings-card__head">
              <Database size={16} aria-hidden="true" />
              <strong>Сервер управления и память</strong>
            </div>

            <form className="settings-url-form" onSubmit={handleReconnect}>
              <label className="settings-field">
                <span>Адрес dennett-head (loopback)</span>
                <div className="settings-url-row">
                  <input
                    type="text"
                    value={urlDraft}
                    onChange={(e) => setUrlDraft(e.target.value)}
                    placeholder="http://127.0.0.1:41771"
                    aria-label="Адрес dennett-head"
                  />
                  <button type="submit" className="popover-secondary-btn">
                    Подключить
                  </button>
                </div>
              </label>
            </form>

            <div className="stats-kv">
              <span>PostgreSQL (dennett_control)</span>
              <strong>
                {health?.postgres_connected
                  ? `Подключён (схема v${health.schema_version ?? "?"})`
                  : "Нет соединения"}
              </strong>
            </div>
            <div className="stats-kv">
              <span>Независимый memoryd</span>
              <strong>
                {health?.memory_status === "healthy"
                  ? "Активен"
                  : health
                    ? "Недоступен (очередь outbox)"
                    : "—"}
              </strong>
            </div>
            <div className="stats-kv">
              <span>Записей в очереди outbox</span>
              <strong>{health?.pending_outbox_count ?? 0}</strong>
            </div>
          </section>

          <section className="settings-card settings-card--muted">
            <div className="settings-card__head">
              <Key size={16} aria-hidden="true" />
              <strong>Хранилище секретов (Secret Broker)</strong>
            </div>
            <p className="settings-note">
              Интерактивная подписка OAuth изолирована в системном keyring CLI{" "}
              <code>agy</code>. API-ключи и секреты никогда не хранятся в открытом состоянии
              интерфейса и не попадают в контекст модели. Полноценное управление ключами сторонних
              провайдеров через системный Secret Broker запланировано на этапах D3–D5.
            </p>
          </section>
        </div>
      </div>
    </div>
  );
}
