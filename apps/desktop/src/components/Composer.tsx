import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import {
  ArrowCounterClockwise,
  ArrowUp,
  Brain,
  CaretDown,
  ChatCircleText,
  Check,
  Clock,
  ListChecks,
  ShieldCheck,
  Sparkle,
  Stop,
} from "@phosphor-icons/react";

import type { ThinkingLevel, WorkspaceWritePolicy } from "../api/contracts";
import { useWorkspace } from "../state/WorkspaceProvider";
import { PortalPopover } from "./PortalPopover";

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
    description: "Запрашивать подтверждение в чате и Входящих перед изменением файла",
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
    case "ask_before_write":
      return "Спрашивать запись";
    case "deny_all":
      return "Только чтение";
    case "allow_paths":
      return "Ограниченная запись (CLI)";
    default:
      return "Запись в папку";
  }
}

export function Composer() {
  const { state, actions, meta } = useWorkspace();
  const [openPopover, setOpenPopover] = useState<OpenComposerPopover>(null);
  const [submittingClarification, setSubmittingClarification] = useState(false);

  const accessTriggerRef = useRef<HTMLButtonElement | null>(null);
  const modelTriggerRef = useRef<HTMLButtonElement | null>(null);
  const thinkingTriggerRef = useRef<HTMLButtonElement | null>(null);

  const sessionId = state.selectedSessionId;
  const activeAgent = meta.activeAgent;
  const activeProject = meta.activeProject;
  const isArchivedProject = meta.isActiveProjectArchived;
  const isTaskRunSession = (meta.activeSession?.session_kind ?? "user_chat") === "task_run";
  const clarificationCard = meta.activeClarificationCard;
  const isFolderless = activeProject ? !activeProject.workspace_path : false;
  const isConnected = Boolean(state.healthSnapshot?.connected);
  const isRunning = meta.isRunningActiveSession;
  const isCancelling = meta.isCancellingActiveSession;
  const hasDraft = meta.currentDraft.trim().length > 0;

  // Clarification reply routes through this single composer; otherwise normal send/offline queue.
  const canSend = Boolean(sessionId) && hasDraft && (
    clarificationCard ? isConnected && !submittingClarification : !isRunning
  );

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

  const submitCurrentInput = async () => {
    if (!sessionId || !canSend) {
      return;
    }
    if (clarificationCard) {
      const answerText = meta.currentDraft.trim();
      if (!answerText) {
        return;
      }
      setSubmittingClarification(true);
      try {
        await actions.resolveInboxCard(clarificationCard.card_id, clarificationCard.revision, {
          action: "clarification",
          answer_text: answerText,
        });
        actions.updateDraft(sessionId, "");
      } finally {
        setSubmittingClarification(false);
      }
      return;
    }
    await actions.sendMessage();
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      if (canSend) {
        void submitCurrentInput();
      }
    } else if (event.key === "Escape" && isRunning && !openPopover && !event.defaultPrevented) {
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

  // D2-UX-03: Archived project is strictly read-only until unarchived.
  if (isArchivedProject && activeProject) {
    return (
      <div className="composer-region">
        <div className="composer-readonly-bar" data-testid="composer-archived-banner">
          <span>Проект «{activeProject.name}» в архиве — просмотр истории в режиме чтения.</span>
          <button
            type="button"
            className="popover-secondary-btn"
            data-testid="composer-unarchive-project-btn"
            onClick={() => void actions.unarchiveProject(activeProject.project_id)}
          >
            <ArrowCounterClockwise size={14} aria-hidden="true" />
            Вернуть из архива
          </button>
        </div>
      </div>
    );
  }

  // D2-UX-03 / D2-UX-05: Background task_run logs stay clean & read-only unless awaiting clarification.
  if (isTaskRunSession && !clarificationCard) {
    return (
      <div className="composer-region">
        <div className="composer-readonly-bar" data-testid="composer-task-run-banner">
          <span>
            Это журнал фонового события — управление расписанием и сигналами доступно в разделе
            «Поручения».
          </span>
          <div className="composer-readonly-bar__actions">
            {isRunning && (
              <button
                type="button"
                className="d2-danger-btn d2-danger-btn--sm"
                onClick={() => void actions.stopActiveRun()}
                disabled={isCancelling}
                data-testid="composer-stop-button"
              >
                <Stop size={12} weight="fill" aria-hidden="true" />
                Stop
              </button>
            )}
            <button
              type="button"
              className="popover-secondary-btn"
              onClick={() => actions.setActiveView("tasks")}
            >
              <ListChecks size={14} aria-hidden="true" />
              Открыть Поручения
            </button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="composer-region">
      <div className="composer-shell">
        <PortalPopover
          isOpen={openPopover === "access" && Boolean(activeAgent) && !isFolderless}
          anchorEl={accessTriggerRef.current}
          onClose={() => setOpenPopover(null)}
          placement="top-start"
          className="composer-popover"
          role="menu"
          ariaLabel="Доступ к файлам проекта"
          testId="access-popover"
        >
          {activeAgent && (
            <>
              <strong>Доступ к файлам</strong>
              <p>Политика проверяется внешним PolicyEngine до выполнения инструмента.</p>
              {activeAgent.current_config.workspace_write_policy.mode === "allow_paths" && (
                <p className="popover-readonly-note" data-testid="cli-write-policy-note">
                  Активна политика из CLI: <strong>Ограниченная запись (CLI)</strong>. Выберите режим
                  ниже, если хотите изменить доступ.
                </p>
              )}
              {WRITE_POLICY_ITEMS.map((item) => {
                const isSelected =
                  activeAgent.current_config.workspace_write_policy.mode === item.mode;
                return (
                  <button
                    key={item.mode}
                    type="button"
                    role="menuitemradio"
                    aria-checked={isSelected}
                    className={isSelected ? "is-selected" : ""}
                    data-testid={`write-policy-option-${item.mode}`}
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
            </>
          )}
        </PortalPopover>

        <PortalPopover
          isOpen={openPopover === "model"}
          anchorEl={modelTriggerRef.current}
          onClose={() => setOpenPopover(null)}
          placement="top-end"
          className="composer-popover popover-models"
          role="menu"
          ariaLabel="Выбор модели агента"
          testId="model-popover"
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
        </PortalPopover>

        <PortalPopover
          isOpen={openPopover === "thinking" && supportedThinkingLevels.length > 0}
          anchorEl={thinkingTriggerRef.current}
          onClose={() => setOpenPopover(null)}
          placement="top-end"
          className="composer-popover popover-thinking"
          role="menu"
          ariaLabel="Уровень размышления"
          testId="thinking-popover"
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
        </PortalPopover>

        {clarificationCard && (
          <div className="composer-clarification-hint" data-testid="composer-clarification-hint">
            <ChatCircleText size={14} aria-hidden="true" />
            <span>Ответ на уточняющий вопрос агента будет передан в активное поручение</span>
          </div>
        )}

        <div className="composer" data-testid="chat-composer">
          <textarea
            ref={meta.composerInputRef}
            className="composer__input"
            rows={1}
            placeholder={
              clarificationCard
                ? "Ответьте на уточняющий вопрос агента…"
                : isConnected
                  ? "Напишите поручение агенту…"
                  : "Офлайн-черновик — нажмите Enter, чтобы поставить в очередь отправки…"
            }
            aria-label={
              clarificationCard
                ? "Ответ на уточняющий вопрос агента"
                : "Поле ввода сообщения"
            }
            value={meta.currentDraft}
            disabled={!sessionId}
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
                  ref={accessTriggerRef}
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
                  <span>
                    {policyLabel(activeAgent.current_config.workspace_write_policy, isFolderless)}
                  </span>
                  {!isFolderless && <CaretDown size={11} aria-hidden="true" />}
                </button>
              )}
            </div>

            <div className="composer-send">
              {activeAgent && (
                <>
                  <button
                    ref={modelTriggerRef}
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
                      ref={thinkingTriggerRef}
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

              {isRunning && !clarificationCard ? (
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
                  className={`send-button ${!isConnected && !clarificationCard ? "send-button--queue" : ""}`}
                  onClick={() => void submitCurrentInput()}
                  disabled={!canSend}
                  aria-label={
                    clarificationCard
                      ? "Отправить ответ на уточнение"
                      : isConnected
                        ? "Отправить сообщение"
                        : "Поставить сообщение в офлайн-очередь"
                  }
                  data-testid="send-button"
                  title={
                    clarificationCard
                      ? "Ответить агенту (Enter)"
                      : isConnected
                        ? "Отправить (Enter)"
                        : "Поставить в очередь до восстановления сети (Enter)"
                  }
                >
                  {isConnected || clarificationCard ? (
                    <ArrowUp size={15} weight="bold" />
                  ) : (
                    <Clock size={15} weight="bold" />
                  )}
                </button>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
