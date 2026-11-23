import { useEffect, useState, type FormEvent } from "react";
import {
  ArrowsClockwise,
  BracketsCurly,
  FolderOpen,
  FolderPlus,
  Plugs,
} from "@phosphor-icons/react";

import { useWorkspace } from "../state/WorkspaceProvider";
import { Composer } from "./Composer";
import { MessageTimeline } from "./MessageTimeline";
import { StatusBanners } from "./StatusBanners";

export function ChatCard() {
  const { state, actions } = useWorkspace();
  const [urlInput, setUrlInput] = useState(
    state.healthSnapshot?.head_url ?? "http://127.0.0.1:41771",
  );

  useEffect(() => {
    if (state.healthSnapshot?.head_url) {
      setUrlInput(state.healthSnapshot.head_url);
    }
  }, [state.healthSnapshot?.head_url]);

  const connected = Boolean(state.healthSnapshot?.connected);
  const hasSession = Boolean(state.selectedSessionId);

  const handleReconnectSubmit = (event: FormEvent) => {
    event.preventDefault();
    if (!urlInput.trim()) {
      return;
    }
    void actions.reconnectWithUrl(urlInput);
  };

  return (
    <main className="chat-card" data-testid="chat-card">
      <StatusBanners />

      {!state.initialized && state.isLoading ? (
        <div className="center-state" data-testid="loading-state">
          <div className="loading-lines loading-lines--centered">
            <span />
            <span />
            <span />
          </div>
          <p>Подключение к dennett-head…</p>
        </div>
      ) : !connected ? (
        <section
          className="center-state offline-state"
          role="alert"
          data-testid="head-offline-state"
        >
          <div className="offline-state__icon">
            <Plugs size={26} weight="light" aria-hidden="true" />
          </div>
          <h1>Сервер Dennett Head недоступен</h1>
          <p className="offline-state__detail" data-testid="head-offline-detail">
            {state.healthSnapshot?.error ??
              "Не удалось подключиться к локальному серверу dennett-head."}
          </p>
          <p className="offline-state__hint">
            Интерфейс работает как тонкий клиент и никогда не подменяет серверные данные. Запустите{" "}
            <code>dennett-head</code> и повторите подключение.
          </p>

          <form className="offline-form" onSubmit={handleReconnectSubmit}>
            <input
              type="text"
              value={urlInput}
              onChange={(e) => setUrlInput(e.target.value)}
              aria-label="Адрес dennett-head"
              placeholder="http://127.0.0.1:41771"
            />
            <div className="offline-form__actions">
              <button
                type="button"
                className="popover-primary-btn"
                onClick={() => void actions.refreshAll()}
                data-testid="retry-head-button"
              >
                <ArrowsClockwise size={14} aria-hidden="true" />
                Повторить
              </button>
              <button type="submit" className="popover-secondary-btn">
                Подключить URL
              </button>
            </div>
          </form>
        </section>
      ) : !hasSession ? (
        <section
          className="center-state onboarding-state"
          data-testid="empty-onboarding-state"
        >
          <div className="onboarding-state__brand">
            <BracketsCurly size={32} weight="light" aria-hidden="true" />
          </div>
          <h1>Начните работу в Dennett</h1>
          <p>
            Выберите папку проекта или начните разговор сразу без папки — подключить рабочую
            директорию можно в любой момент.
          </p>

          <div className="onboarding-choices">
            <button
              type="button"
              className="onboarding-choice-card"
              data-testid="onboarding-pick-folder-btn"
              onClick={() => void actions.requestFolderPicker({ mode: "create-project" })}
            >
              <FolderOpen size={22} aria-hidden="true" />
              <span>
                <strong>Выбрать папку…</strong>
                <small>Открыть существующую папку или репозиторий</small>
              </span>
            </button>

            <button
              type="button"
              className="onboarding-choice-card"
              data-testid="onboarding-folderless-btn"
              onClick={() => void actions.createFolderlessProject()}
            >
              <FolderPlus size={22} aria-hidden="true" />
              <span>
                <strong>Пока без папки</strong>
                <small>Открыть чистый чат в 1 клик, привязать папку позже</small>
              </span>
            </button>
          </div>
        </section>
      ) : (
        <>
          <MessageTimeline />
          <Composer />
        </>
      )}
    </main>
  );
}
