import { useEffect, useState, type FormEvent } from "react";

import { useWorkspace } from "../state/WorkspaceProvider";
import { Composer } from "./Composer";
import { RefreshIcon, StopSquareIcon } from "./Icons";
import { MessageTimeline } from "./MessageTimeline";
import { StatusBanners } from "./StatusBanners";

export function ChatCard() {
  const { state, actions, meta } = useWorkspace();

  const [headUrlInput, setHeadUrlInput] = useState(
    state.healthSnapshot?.head_url ?? "http://127.0.0.1:41771",
  );
  const [quickProjectName, setQuickProjectName] = useState("Мой проект");
  const [quickWorkspacePath, setQuickWorkspacePath] = useState("");
  const [quickMemoryScope, setQuickMemoryScope] = useState("");
  const [quickSessionTitle, setQuickSessionTitle] = useState("Первый разговор");

  useEffect(() => {
    if (state.healthSnapshot?.head_url) {
      setHeadUrlInput(state.healthSnapshot.head_url);
    }
  }, [state.healthSnapshot?.head_url]);

  const isConnected = Boolean(state.healthSnapshot?.connected);
  const hasActiveRun = state.runningSessionId !== null;
  const isCancelling = state.cancellingSessionId !== null;

  const handleReconnect = async (event: FormEvent) => {
    event.preventDefault();
    if (headUrlInput.trim() && headUrlInput.trim() !== state.healthSnapshot?.head_url) {
      await actions.reconnectWithUrl(headUrlInput.trim());
    } else {
      await actions.refreshAll();
    }
  };

  const handleQuickStart = async (event: FormEvent) => {
    event.preventDefault();
    if (!quickProjectName.trim() || !quickWorkspacePath.trim()) {
      return;
    }
    const slug = quickProjectName
      .trim()
      .toLowerCase()
      .replace(/[^a-z0-9._-]+/g, "-")
      .replace(/^-+|-+$/g, "");
    const effectiveScope = quickMemoryScope.trim() || `project:${slug || "default"}`;
    await actions.createQuickStart({
      name: quickProjectName.trim(),
      workspacePath: quickWorkspacePath.trim(),
      memoryScope: effectiveScope,
      sessionTitle: quickSessionTitle.trim() || "Первый разговор",
    });
  };

  return (
    <main
      className="surface-card chat-card"
      aria-label="Рабочая область чата"
      data-testid="chat-card"
    >
      <StatusBanners />

      {/* Always-accessible Stop control outside message and artifact bodies */}
      {hasActiveRun && (
        <div
          className="running-bar"
          role="status"
          aria-live="polite"
          data-testid="running-control-bar"
        >
          <div className="running-bar__info">
            <span className="pulse-dot" aria-hidden="true" />
            <span>Агент выполняет поручение через dennett-head... (Esc — остановить)</span>
          </div>
          <button
            type="button"
            className="btn btn--danger"
            onClick={() => void actions.stopActiveRun()}
            disabled={isCancelling}
            aria-label="Stop"
            data-testid="global-stop-button"
          >
            <StopSquareIcon />
            {isCancelling ? "Остановка..." : "Stop"}
          </button>
        </div>
      )}

      {!state.initialized && state.isLoading ? (
        <section className="empty-state" role="status" aria-label="Подключение к dennett-head">
          <div className="empty-state__box">
            <h1 className="empty-state__title">Подключение к dennett-head...</h1>
            <p className="empty-state__desc">
              Проверяем локальный демон управления и состояние памяти memoryd.
            </p>
          </div>
        </section>
      ) : !isConnected ? (
        <section
          className="empty-state"
          role="alert"
          aria-label="dennett-head недоступен"
          data-testid="head-offline-state"
        >
          <div className="empty-state__box">
            <h1 className="empty-state__title">dennett-head недоступен</h1>
            <p className="empty-state__desc">
              Desktop-интерфейс не запускает модель напрямую и не подменяет серверный runtime.
              Запустите <code>dennett-head</code> и повторите подключение.
            </p>
            {state.healthSnapshot?.error && (
              <p className="empty-state__desc" data-testid="head-offline-detail">
                Диагностика: {state.healthSnapshot.error}
              </p>
            )}
            <form className="empty-state__form" onSubmit={(e) => void handleReconnect(e)}>
              <label className="field-label">
                Адрес локального dennett-head (loopback)
                <input
                  type="text"
                  className="field-input"
                  value={headUrlInput}
                  onChange={(e) => setHeadUrlInput(e.target.value)}
                  aria-label="Адрес dennett-head"
                />
              </label>
              <div className="inline-form__actions">
                <button
                  type="submit"
                  className="btn btn--primary"
                  disabled={state.isLoading}
                  data-testid="retry-head-button"
                >
                  <RefreshIcon />
                  {state.isLoading ? "Проверка..." : "Повторить подключение"}
                </button>
              </div>
            </form>
          </div>
        </section>
      ) : !meta.activeSession ? (
        <section
          className="empty-state"
          aria-label="Создание первого проекта"
          data-testid="empty-onboarding-state"
        >
          <div className="empty-state__box">
            <h1 className="empty-state__title">Начните работу с Dennett</h1>
            <p className="empty-state__desc">
              {state.projects.length === 0
                ? "Проектов и чатов ещё нет. Создайте первый проект — Dennett привяжет область памяти и откроет разговор с агентом."
                : "Выберите разговор в списке слева или создайте новый чат для выбранного проекта."}
            </p>
            {state.projects.length === 0 ? (
              <form className="empty-state__form" onSubmit={(e) => void handleQuickStart(e)}>
                <label className="field-label">
                  Название проекта
                  <input
                    type="text"
                    className="field-input"
                    value={quickProjectName}
                    onChange={(e) => setQuickProjectName(e.target.value)}
                    required
                  />
                </label>
                <label className="field-label">
                  Каталог рабочей области
                  <input
                    type="text"
                    className="field-input"
                    placeholder="Например, C:/Users/me/projects/demo"
                    value={quickWorkspacePath}
                    onChange={(e) => setQuickWorkspacePath(e.target.value)}
                    required
                  />
                </label>
                <label className="field-label">
                  Область памяти (необяз.)
                  <input
                    type="text"
                    className="field-input"
                    placeholder="Например, project:demo-d1"
                    value={quickMemoryScope}
                    onChange={(e) => setQuickMemoryScope(e.target.value)}
                  />
                </label>
                <label className="field-label">
                  Название первого разговора
                  <input
                    type="text"
                    className="field-input"
                    value={quickSessionTitle}
                    onChange={(e) => setQuickSessionTitle(e.target.value)}
                  />
                </label>
                <div className="inline-form__actions">
                  <button type="submit" className="btn btn--primary">
                    Создать проект и начать чат
                  </button>
                </div>
              </form>
            ) : (
              <div className="inline-form__actions">
                <button
                  type="button"
                  className="btn btn--primary"
                  onClick={() => {
                    const first = state.projects[0];
                    if (first) {
                      void actions.createSession(first.project_id, "Новый разговор");
                    }
                  }}
                >
                  Создать чат в {state.projects[0]?.name}
                </button>
              </div>
            )}
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
