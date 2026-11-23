import type { KeyboardEvent } from "react";

import type { ThinkingLevel } from "../api/contracts";
import { useWorkspace } from "../state/WorkspaceProvider";
import { SendArrowIcon, StopSquareIcon } from "./Icons";

const MODEL_OPTIONS: Array<{ value: string; label: string }> = [
  { value: "", label: "Модель по умолчанию" },
  { value: "gemini-2.5-pro", label: "Gemini 2.5 Pro" },
  { value: "gemini-2.5-flash", label: "Gemini 2.5 Flash" },
  { value: "gemini-3.8-pro", label: "Gemini 3.8 Pro" },
  { value: "gemini-3.8-flash", label: "Gemini 3.8 Flash" },
];

const THINKING_OPTIONS: Array<{ value: ThinkingLevel; label: string }> = [
  { value: "low", label: "Low" },
  { value: "medium", label: "Medium" },
  { value: "high", label: "High" },
  { value: "xhigh", label: "XHigh" },
];

export function Composer() {
  const { state, actions, meta } = useWorkspace();

  const sessionId = state.selectedSessionId;
  const activeAgent = meta.activeAgent;
  const isRunning = meta.isRunningActiveSession;
  const isCancelling = sessionId !== null && state.cancellingSessionId === sessionId;
  const canSend =
    Boolean(sessionId) &&
    Boolean(state.healthSnapshot?.connected) &&
    !isRunning &&
    meta.currentDraft.trim().length > 0;

  const currentModel = activeAgent?.current_config.model_preference ?? "";
  const currentThinking = activeAgent?.current_config.thinking_level ?? "medium";

  const handleKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      if (canSend) {
        void actions.sendMessage();
      }
    } else if (event.key === "Escape" && isRunning) {
      event.preventDefault();
      void actions.stopActiveRun();
    }
  };

  return (
    <div className="composer-wrap">
      <div className="composer" data-testid="chat-composer">
        <textarea
          ref={meta.composerInputRef}
          className="composer__input"
          rows={2}
          placeholder="Напишите поручение агенту... (Enter — отправить, Shift+Enter — перенос строки)"
          aria-label="Поле ввода сообщения"
          value={meta.currentDraft}
          disabled={!sessionId || !state.healthSnapshot?.connected}
          onChange={(e) => {
            if (sessionId) {
              actions.updateDraft(sessionId, e.target.value);
            }
          }}
          onKeyDown={handleKeyDown}
        />

        <div className="composer__toolbar">
          <div className="composer__controls">
            {activeAgent && (
              <>
                <select
                  className="composer__select"
                  aria-label="Предпочитаемая модель агента"
                  value={currentModel}
                  disabled={state.isUpdatingModel || !state.healthSnapshot?.connected}
                  onChange={(e) => {
                    const nextModel = e.target.value.trim() || null;
                    void actions.updateActiveAgentModel(nextModel, currentThinking);
                  }}
                >
                  {!MODEL_OPTIONS.some((opt) => opt.value === currentModel) && currentModel && (
                    <option value={currentModel}>{currentModel}</option>
                  )}
                  {MODEL_OPTIONS.map((opt) => (
                    <option key={opt.value || "default"} value={opt.value}>
                      {opt.label}
                    </option>
                  ))}
                </select>

                <select
                  className="composer__select"
                  aria-label="Уровень рассуждений"
                  value={currentThinking}
                  disabled={state.isUpdatingModel || !state.healthSnapshot?.connected}
                  onChange={(e) => {
                    const nextLevel = e.target.value as ThinkingLevel;
                    void actions.updateActiveAgentModel(currentModel || null, nextLevel);
                  }}
                >
                  {THINKING_OPTIONS.map((opt) => (
                    <option key={opt.value} value={opt.value}>
                      {opt.label}
                    </option>
                  ))}
                </select>

                <span
                  className="composer__version-tag"
                  data-testid="agent-config-version"
                  title={`Стабильный Agent ID: ${activeAgent.agent_id}`}
                >
                  {activeAgent.name} · v{activeAgent.current_config.version_number}
                </span>
              </>
            )}
          </div>

          <div className="composer__actions">
            {isRunning ? (
              <button
                type="button"
                className="btn btn--danger"
                onClick={() => void actions.stopActiveRun()}
                disabled={isCancelling}
                aria-label="Остановить текущий запуск"
                data-testid="composer-stop-button"
              >
                <StopSquareIcon />
                {isCancelling ? "Остановка..." : "Stop"}
              </button>
            ) : (
              <button
                type="button"
                className="send-btn"
                onClick={() => void actions.sendMessage()}
                disabled={!canSend}
                aria-label="Отправить сообщение"
                data-testid="send-button"
              >
                <SendArrowIcon />
                Отправить
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
