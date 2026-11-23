import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import {
  ArrowUp,
  Brain,
  CaretDown,
  Check,
  ShieldCheck,
  Sparkle,
  Stop,
} from "@phosphor-icons/react";

import type { ThinkingLevel, WorkspaceWritePolicy } from "../api/contracts";
import { useWorkspace } from "../state/WorkspaceProvider";

type OpenComposerPopover = "access" | "model" | "thinking" | null;

const THINKING_LABELS: Record<ThinkingLevel, string> = {
  low: "Low",
  medium: "Medium",
  high: "High",
  xhigh: "Extra",
};

const WRITE_POLICY_ITEMS: Array<{
  mode: "ask_before_write" | "allow_workspace" | "deny_all";
  label: string;
  description: string;
}> = [
  {
    mode: "ask_before_write",
    label: "Спрашивать запись",
    description: "Внешний PolicyEngine проверяет каждое изменение файла",
  },
  {
    mode: "allow_workspace",
    label: "Запись в папку",
    description: "Разрешить запись строго внутри корневой папки проекта",
  },
  {
    mode: "deny_all",
    label: "Только чтение",
    description: "Запретить любые файловые изменения в этом проекте",
  },
];

function policyLabel(policy: WorkspaceWritePolicy | undefined, isFolderless: boolean): string {
  if (isFolderless) {
    return "Только чтение";
  }
  switch (policy?.mode) {
    case "allow_workspace":
      return "Запись в папку";
    case "deny_all":
      return "Только чтение";
    case "allow_paths":
      return "Ограниченная запись";
    default:
      return "Спрашивать запись";
  }
}

export function Composer() {
  const { state, actions, meta } = useWorkspace();
  const [openPopover, setOpenPopover] = useState<OpenComposerPopover>(null);
  const shellRef = useRef<HTMLDivElement | null>(null);

  const sessionId = state.selectedSessionId;
  const activeAgent = meta.activeAgent;
  const activeProject = meta.activeProject;
  const isFolderless = activeProject ? !activeProject.workspace_path : false;
  const isConnected = Boolean(state.healthSnapshot?.connected);
  const isRunning = meta.isRunningActiveSession;
  const isCancelling = sessionId !== null && state.cancellingSessionId === sessionId;
  const canSend =
    Boolean(sessionId) && isConnected && !isRunning && meta.currentDraft.trim().length > 0;

  const catalog = state.runtimeCatalog;
  const models = catalog?.models ?? [];

  const currentModelId =
    activeAgent?.current_config.model_preference ?? catalog?.default_model_id ?? null;

  const selectedModelOption = useMemo(
    () => models.find((m) => m.model_id === currentModelId) ?? models[0] ?? null,
    [currentModelId, models],
  );

  const supportedThinkingLevels: ThinkingLevel[] = useMemo(() => {
    if (!selectedModelOption) {
      return (catalog?.thinking_options ?? []).map((t) => t.value);
    }
    return selectedModelOption.supported_thinking_levels;
  }, [catalog?.thinking_options, selectedModelOption]);

  const currentThinking: ThinkingLevel | null = useMemo(() => {
    if (supportedThinkingLevels.length === 0) {
      return null;
    }
    const raw = activeAgent?.current_config.thinking_level ?? catalog?.default_thinking_level;
    if (raw && supportedThinkingLevels.includes(raw)) {
      return raw;
    }
    return (
      selectedModelOption?.default_thinking_level ??
      supportedThinkingLevels[supportedThinkingLevels.length - 1] ??
      null
    );
  }, [
    activeAgent?.current_config.thinking_level,
    catalog?.default_thinking_level,
    selectedModelOption?.default_thinking_level,
    supportedThinkingLevels,
  ]);

  const thinkingLabelMap = useMemo(() => {
    const map: Record<string, string> = { ...THINKING_LABELS };
    for (const opt of catalog?.thinking_options ?? []) {
      map[opt.value] = opt.label;
    }
    return map;
  }, [catalog?.thinking_options]);

  useEffect(() => {
    const el = meta.composerInputRef.current;
    if (!el) {
      return;
    }
    el.style.height = "auto";
    const nextHeight = Math.min(Math.max(el.scrollHeight, 54), 170);
    el.style.height = `${nextHeight}px`;
  }, [meta.composerInputRef, meta.currentDraft, sessionId]);

  useEffect(() => {
    if (!openPopover) {
      return;
    }
    const handlePointerDown = (event: MouseEvent) => {
      if (shellRef.current && !shellRef.current.contains(event.target as Node)) {
        setOpenPopover(null);
      }
    };
    const handleKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpenPopover(null);
      }
    };
    document.addEventListener("mousedown", handlePointerDown);
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("mousedown", handlePointerDown);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [openPopover]);

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

  const handleSelectModel = (modelId: string) => {
    setOpenPopover(null);
    const chosen = models.find((m) => m.model_id === modelId);
    const nextSupported = chosen?.supported_thinking_levels ?? [];
    let nextThinking: ThinkingLevel | null = null;
    if (nextSupported.length > 0) {
      if (currentThinking && nextSupported.includes(currentThinking)) {
        nextThinking = currentThinking;
      } else {
        nextThinking =
          chosen?.default_thinking_level ?? nextSupported[nextSupported.length - 1] ?? null;
      }
    }
    void actions.updateActiveAgentModel(modelId, nextThinking);
  };

  const handleSelectThinking = (level: ThinkingLevel) => {
    setOpenPopover(null);
    void actions.updateActiveAgentModel(currentModelId, level);
  };

  const modelButtonLabel =
    selectedModelOption?.label ?? currentModelId ?? (isConnected ? "Модель движка" : "Офлайн");
  const thinkingButtonLabel = currentThinking
    ? (thinkingLabelMap[currentThinking] ?? currentThinking)
    : "Thinking";

  return (
    <div className="composer-region">
      <div className="composer-shell" ref={shellRef}>
        {openPopover === "access" && activeAgent && !isFolderless && (
          <div
            className="floating-popover composer-popover popover-left"
            role="menu"
            aria-label="Доступ к файлам проекта"
            data-testid="access-popover"
          >
            <strong>Доступ к файлам</strong>
            <p>Политика проверяется внешним PolicyEngine до выполнения инструмента.</p>
            {WRITE_POLICY_ITEMS.map((item) => {
              const isSelected = activeAgent.current_config.workspace_write_policy.mode === item.mode;
              return (
                <button
                  key={item.mode}
                  type="button"
                  role="menuitemradio"
                  aria-checked={isSelected}
                  className={isSelected ? "is-selected" : ""}
                  onClick={() => {
                    setOpenPopover(null);
                    void actions.updateActiveAgentWritePolicy({ mode: item.mode });
                  }}
                >
                  <span className="popover-option-copy">
                    <strong>{item.label}</strong>
                    <small>{item.description}</small>
                  </span>
                  {isSelected && <Check size={14} aria-hidden="true" />}
                </button>
              );
            })}
          </div>
        )}

        {openPopover === "model" && (
          <div
            className="floating-popover composer-popover popover-right popover-models"
            role="menu"
            aria-label="Выбор модели агента"
            data-testid="model-popover"
          >
            <strong>Модель</strong>
            <p>
              {catalog?.is_simulated
                ? "Динамический список моделей тестового контура."
                : `Живой список моделей ${catalog?.tier ?? "Antigravity ACP"}.`}
            </p>
            <div className="popover-scroll-list">
              {models.length === 0 ? (
                <p className="popover-empty">Список моделей загружается…</p>
              ) : (
                models.map((opt) => {
                  const isSelected = opt.model_id === (selectedModelOption?.model_id ?? currentModelId);
                  return (
                    <button
                      key={opt.model_id}
                      type="button"
                      role="menuitemradio"
                      aria-checked={isSelected}
                      className={isSelected ? "is-selected" : ""}
                      data-testid={`model-option-${opt.model_id}`}
                      onClick={() => handleSelectModel(opt.model_id)}
                    >
                      <span className="popover-model-name">{opt.label}</span>
                      {isSelected && <Check size={14} aria-hidden="true" />}
                    </button>
                  );
                })
              )}
            </div>
          </div>
        )}

        {openPopover === "thinking" && supportedThinkingLevels.length > 0 && (
          <div
            className="floating-popover composer-popover popover-right popover-thinking"
            role="menu"
            aria-label="Уровень размышления"
            data-testid="thinking-popover"
          >
            <strong>Уровень размышления</strong>
            <p>Глубина рассуждений для {selectedModelOption?.label ?? "выбранной модели"}.</p>
            {supportedThinkingLevels.map((lvl) => {
              const isSelected = lvl === currentThinking;
              return (
                <button
                  key={lvl}
                  type="button"
                  role="menuitemradio"
                  aria-checked={isSelected}
                  className={isSelected ? "is-selected" : ""}
                  data-testid={`thinking-option-${lvl}`}
                  onClick={() => handleSelectThinking(lvl)}
                >
                  <span>{thinkingLabelMap[lvl] ?? lvl}</span>
                  {isSelected && <Check size={14} aria-hidden="true" />}
                </button>
              );
            })}
          </div>
        )}

        <div className="composer" data-testid="chat-composer">
          <textarea
            ref={meta.composerInputRef}
            className="composer__input"
            rows={1}
            placeholder="Напишите поручение агенту…"
            aria-label="Поле ввода сообщения"
            value={meta.currentDraft}
            disabled={!sessionId || !isConnected}
            onChange={(e) => {
              if (sessionId) {
                actions.updateDraft(sessionId, e.target.value);
              }
            }}
            onKeyDown={handleKeyDown}
          />

          <div className="composer-toolbar">
            <div className="composer-tools">
              {activeAgent && (
                <button
                  type="button"
                  className={`composer-setting ${isFolderless ? "is-static" : ""}`}
                  disabled={isFolderless || state.isUpdatingModel || !isConnected}
                  aria-expanded={openPopover === "access"}
                  aria-haspopup="menu"
                  aria-label="Доступ к файлам проекта"
                  data-testid="access-selector-trigger"
                  title={
                    isFolderless
                      ? "Проект без папки работает в режиме только чтение до привязки папки"
                      : "Политика записи в рабочую папку"
                  }
                  onClick={() =>
                    setOpenPopover((prev) => (prev === "access" ? null : "access"))
                  }
                >
                  <ShieldCheck size={15} aria-hidden="true" />
                  <span>{policyLabel(activeAgent.current_config.workspace_write_policy, isFolderless)}</span>
                  {!isFolderless && <CaretDown size={11} aria-hidden="true" />}
                </button>
              )}
            </div>

            <div className="composer-send">
              {activeAgent && (
                <>
                  <button
                    type="button"
                    className="runtime-setting"
                    disabled={state.isUpdatingModel || !isConnected || models.length === 0}
                    aria-expanded={openPopover === "model"}
                    aria-haspopup="menu"
                    aria-label="Модель агента"
                    data-testid="model-selector-trigger"
                    onClick={() =>
                      setOpenPopover((prev) => (prev === "model" ? null : "model"))
                    }
                  >
                    <Sparkle size={14} aria-hidden="true" />
                    <span>{modelButtonLabel}</span>
                    <CaretDown size={11} aria-hidden="true" />
                  </button>

                  {supportedThinkingLevels.length > 0 ? (
                    <button
                      type="button"
                      className="runtime-setting runtime-setting--thinking"
                      disabled={state.isUpdatingModel || !isConnected}
                      aria-expanded={openPopover === "thinking"}
                      aria-haspopup="menu"
                      aria-label="Уровень размышления"
                      data-testid="thinking-selector-trigger"
                      onClick={() =>
                        setOpenPopover((prev) => (prev === "thinking" ? null : "thinking"))
                      }
                    >
                      <Brain size={14} aria-hidden="true" />
                      <small>{thinkingButtonLabel}</small>
                      <CaretDown size={11} aria-hidden="true" />
                    </button>
                  ) : (
                    <span
                      className="runtime-setting runtime-setting--fixed"
                      data-testid="thinking-builtin-badge"
                      title="Уровень размышления встроен в эту модель"
                    >
                      <Brain size={14} aria-hidden="true" />
                      <small>Thinking</small>
                    </span>
                  )}
                </>
              )}

              {isRunning ? (
                <button
                  type="button"
                  className="send-button stop-button"
                  onClick={() => void actions.stopActiveRun()}
                  disabled={isCancelling}
                  aria-label="Остановить текущий запуск"
                  data-testid="composer-stop-button"
                  title="Остановить (Esc)"
                >
                  <Stop size={14} weight="fill" />
                </button>
              ) : (
                <button
                  type="button"
                  className="send-button"
                  onClick={() => void actions.sendMessage()}
                  disabled={!canSend}
                  aria-label="Отправить сообщение"
                  data-testid="send-button"
                  title="Отправить (Enter)"
                >
                  <ArrowUp size={15} weight="bold" />
                </button>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
