import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import {
  ArrowCounterClockwise,
  ArrowUp,
  Brain,
  CaretDown,
  ChatCircleText,
  Check,
  Clock,
  FileText,
  FolderSimple,
  Lightning,
  ListChecks,
  PauseCircle,
  PencilSimple,
  Play,
  Queue,
  ShieldCheck,
  Sparkle,
  Stop,
  Trash,
  X,
} from "@phosphor-icons/react";

import type {
  AutonomyMode,
  SessionFollowupRecord,
  ThinkingLevel,
  WorkspaceFileEntry,
} from "../api/contracts";
import { useWorkspace } from "../state/WorkspaceProvider";
import { PortalPopover } from "./PortalPopover";

type OpenComposerPopover = "access" | "model" | "thinking" | null;

const THINKING_LABELS_RU: Record<ThinkingLevel, string> = {
  low: "Низкий",
  medium: "Средний",
  high: "Высокий",
  xhigh: "Макс",
};

function localizeThinkingLabel(level: ThinkingLevel, rawLabel?: string): string {
  if (THINKING_LABELS_RU[level]) {
    return THINKING_LABELS_RU[level];
  }
  const normalized = (rawLabel ?? "").trim().toLowerCase();
  if (normalized === "low") return "Низкий";
  if (normalized === "medium") return "Средний";
  if (normalized === "high") return "Высокий";
  if (normalized === "extra" || normalized === "xhigh") return "Макс";
  return rawLabel || level;
}

const AUTONOMY_MODE_ITEMS: Array<{
  mode: AutonomyMode;
  label: string;
  description: string;
}> = [
  {
    mode: "auto",
    label: "Авто",
    description:
      "Свободная запись файлов и локальные команды проекта; запрос только на критичные внешние, платёжные и разрушительные действия",
  },
  {
    mode: "full_access",
    label: "Полный доступ",
    description:
      "Полная автономность без интерактивных подтверждений (уровень полного доступа)",
  },
];

function autonomyLabel(mode: AutonomyMode | null | undefined): string {
  return mode === "full_access" ? "Полный доступ" : "Авто";
}

interface AtMentionMatch {
  start: number;
  end: number;
  query: string;
}

function findAtMention(text: string, cursorPos: number): AtMentionMatch | null {
  const before = text.slice(0, cursorPos);
  const match = /(?:^|\s)@([^\s@]*)$/.exec(before);
  if (!match) {
    return null;
  }
  const query = match[1] ?? "";
  const start = cursorPos - query.length - 1;
  return { start, end: cursorPos, query };
}

export function Composer() {
  const { state, actions, meta } = useWorkspace();
  const [openPopover, setOpenPopover] = useState<OpenComposerPopover>(null);
  const [submittingClarification, setSubmittingClarification] = useState(false);
  const [isSubmittingFollowup, setIsSubmittingFollowup] = useState(false);
  const [editingFollowupId, setEditingFollowupId] = useState<string | null>(null);
  const [editingText, setEditingText] = useState("");
  const [busyFollowupId, setBusyFollowupId] = useState<string | null>(null);
  const [isBulkFollowupBusy, setIsBulkFollowupBusy] = useState(false);
  const [atMention, setAtMention] = useState<AtMentionMatch | null>(null);
  const [atSelectedIndex, setAtSelectedIndex] = useState(0);

  const accessTriggerRef = useRef<HTMLButtonElement | null>(null);
  const modelTriggerRef = useRef<HTMLButtonElement | null>(null);
  const thinkingTriggerRef = useRef<HTMLButtonElement | null>(null);
  const composerBoxRef = useRef<HTMLDivElement | null>(null);

  const sessionId = state.selectedSessionId;
  const activeAgent = meta.activeAgent;
  const activeProject = meta.activeProject;
  const isArchivedProject = meta.isActiveProjectArchived;
  const isStandaloneSession = meta.isStandaloneActiveSession;
  const isTaskRunSession = (meta.activeSession?.session_kind ?? "user_chat") === "task_run";
  const clarificationCard = meta.activeClarificationCard;
  const isConnected = Boolean(state.healthSnapshot?.connected);
  const isRunning = meta.isRunningActiveSession;
  const isCancelling = meta.isCancellingActiveSession;
  const hasDraft = meta.currentDraft.trim().length > 0;
  const activeFollowups = meta.activeFollowups ?? [];
  const hasPausedFollowups = activeFollowups.some((item) => item.status === "paused");
  const enterToSend = state.userPreferences.enter_to_send ?? true;

  const currentAutonomy: AutonomyMode = activeAgent?.current_config.autonomy_mode ?? "auto";

  useEffect(() => {
    setEditingFollowupId(null);
    setEditingText("");
    setAtMention(null);
  }, [sessionId]);

  // Lazily load project workspace files when @-autocomplete opens and files aren't loaded yet.
  useEffect(() => {
    if (
      atMention &&
      activeProject?.workspace_path &&
      isConnected &&
      (!state.projectFiles || state.projectFiles.project_id !== activeProject.project_id) &&
      !state.isLoadingFiles &&
      !state.projectFilesError
    ) {
      void actions.refreshProjectFiles(false);
    }
  }, [
    actions,
    activeProject?.project_id,
    activeProject?.workspace_path,
    atMention,
    isConnected,
    state.isLoadingFiles,
    state.projectFiles,
    state.projectFilesError,
  ]);

  const atFileSuggestions: WorkspaceFileEntry[] = useMemo(() => {
    if (!atMention || !activeProject?.workspace_path) {
      return [];
    }
    const entries = state.projectFiles?.entries ?? [];
    const q = atMention.query.trim().toLowerCase();
    return entries
      .filter(
        (entry) =>
          !q ||
          entry.relative_path.toLowerCase().includes(q) ||
          entry.name.toLowerCase().includes(q),
      )
      .slice(0, 12);
  }, [activeProject?.workspace_path, atMention, state.projectFiles?.entries]);

  const showAtPopover = Boolean(
    atMention &&
      activeProject?.workspace_path &&
      (atFileSuggestions.length > 0 || Boolean(state.projectFilesError)),
  );

  useEffect(() => {
    setAtSelectedIndex(0);
  }, [atMention?.query]);

  // Clarification reply routes through this single composer; otherwise normal send/offline queue.
  const canSend =
    Boolean(sessionId) &&
    hasDraft &&
    (clarificationCard ? isConnected && !submittingClarification : !isRunning);

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
    const map: Record<string, string> = { ...THINKING_LABELS_RU };
    for (const opt of catalog?.thinking_options ?? []) {
      map[opt.value] = localizeThinkingLabel(opt.value, opt.label);
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

  const applyAtSelection = (entry: WorkspaceFileEntry) => {
    if (!sessionId || !atMention) {
      return;
    }
    const current = meta.currentDraft;
    const inserted = `@${entry.relative_path} `;
    const nextValue = current.slice(0, atMention.start) + inserted + current.slice(atMention.end);
    actions.updateDraft(sessionId, nextValue);
    setAtMention(null);
    const nextCursor = atMention.start + inserted.length;
    window.setTimeout(() => {
      const el = meta.composerInputRef.current;
      if (el) {
        el.focus();
        el.setSelectionRange(nextCursor, nextCursor);
      }
    }, 0);
  };

  const submitCurrentInput = async () => {
    if (!sessionId || !canSend) {
      return;
    }
    setAtMention(null);
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

  const handleEnqueueFromDraft = async () => {
    if (!sessionId || !isConnected || isSubmittingFollowup) {
      return;
    }
    const rawDraft = meta.currentDraft;
    const trimmed = rawDraft.trim();
    if (!trimmed) {
      return;
    }
    setAtMention(null);
    setIsSubmittingFollowup(true);
    actions.updateDraft(sessionId, "");
    try {
      const created = await actions.enqueueFollowup(trimmed);
      if (!created) {
        actions.updateDraft(sessionId, rawDraft);
      }
    } finally {
      setIsSubmittingFollowup(false);
    }
  };

  const handleSteerFromDraft = async () => {
    if (!sessionId || !isConnected || isSubmittingFollowup) {
      return;
    }
    const rawDraft = meta.currentDraft;
    const trimmed = rawDraft.trim();
    if (!trimmed) {
      return;
    }
    setAtMention(null);
    setIsSubmittingFollowup(true);
    actions.updateDraft(sessionId, "");
    try {
      const ok = await actions.steerSession(trimmed);
      if (!ok) {
        actions.updateDraft(sessionId, rawDraft);
      }
    } finally {
      setIsSubmittingFollowup(false);
    }
  };

  const handleSaveEditFollowup = async (followupId: string) => {
    const trimmed = editingText.trim();
    if (!trimmed || busyFollowupId) {
      return;
    }
    setBusyFollowupId(followupId);
    try {
      const updated = await actions.updateFollowup(followupId, trimmed);
      if (updated) {
        setEditingFollowupId(null);
        setEditingText("");
      }
    } finally {
      setBusyFollowupId(null);
    }
  };

  const handlePromoteFollowup = async (item: SessionFollowupRecord) => {
    if (!isConnected || busyFollowupId) {
      return;
    }
    setBusyFollowupId(item.followup_id);
    let deleted = false;
    try {
      deleted = await actions.deleteFollowup(item.followup_id);
      if (deleted && editingFollowupId === item.followup_id) {
        setEditingFollowupId(null);
        setEditingText("");
      }
    } finally {
      setBusyFollowupId(null);
    }
    if (deleted) {
      // Trigger steering immediately without keeping queue controls wedged across a long turn.
      void actions.steerSession(item.content);
    }
  };

  const handleDeleteFollowup = async (followupId: string) => {
    if (!isConnected || busyFollowupId) {
      return;
    }
    setBusyFollowupId(followupId);
    try {
      const deleted = await actions.deleteFollowup(followupId);
      if (deleted && editingFollowupId === followupId) {
        setEditingFollowupId(null);
        setEditingText("");
      }
    } finally {
      setBusyFollowupId(null);
    }
  };

  const handleResumeFollowups = async () => {
    if (!isConnected || isBulkFollowupBusy) {
      return;
    }
    setIsBulkFollowupBusy(true);
    try {
      await actions.resumeFollowups();
    } finally {
      setIsBulkFollowupBusy(false);
    }
  };

  const handleClearFollowups = async () => {
    if (!isConnected || isBulkFollowupBusy) {
      return;
    }
    setIsBulkFollowupBusy(true);
    try {
      await actions.clearFollowups();
      setEditingFollowupId(null);
      setEditingText("");
    } finally {
      setIsBulkFollowupBusy(false);
    }
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (showAtPopover) {
      if (atFileSuggestions.length > 0) {
        if (event.key === "ArrowDown") {
          event.preventDefault();
          setAtSelectedIndex((idx) => (idx + 1) % atFileSuggestions.length);
          return;
        }
        if (event.key === "ArrowUp") {
          event.preventDefault();
          setAtSelectedIndex(
            (idx) => (idx - 1 + atFileSuggestions.length) % atFileSuggestions.length,
          );
          return;
        }
        if (event.key === "Enter" || event.key === "Tab") {
          event.preventDefault();
          const chosen = atFileSuggestions[atSelectedIndex] ?? atFileSuggestions[0];
          if (chosen) {
            applyAtSelection(chosen);
          }
          return;
        }
      }
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        setAtMention(null);
        return;
      }
    }

    if (event.key === "Enter" && !event.shiftKey) {
      const isModEnter = event.ctrlKey || event.metaKey;
      if (isRunning && !clarificationCard) {
        if (!enterToSend && !isModEnter) {
          return;
        }
        event.preventDefault();
        if (!sessionId || !hasDraft || !isConnected || isSubmittingFollowup) {
          return;
        }
        if (isModEnter) {
          void handleSteerFromDraft();
        } else {
          void handleEnqueueFromDraft();
        }
        return;
      }
      if (enterToSend || isModEnter) {
        event.preventDefault();
        if (canSend) {
          void submitCurrentInput();
        }
      }
    } else if (
      event.key === "Escape" &&
      isRunning &&
      !openPopover &&
      !editingFollowupId &&
      !event.defaultPrevented
    ) {
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
    : "Размышление";

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
                Стоп
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
          isOpen={openPopover === "access" && Boolean(activeAgent)}
          anchorEl={accessTriggerRef.current}
          onClose={() => setOpenPopover(null)}
          placement="top-start"
          className="composer-popover"
          role="menu"
          ariaLabel="Режим автономности агента"
          testId="access-popover"
        >
          {activeAgent && (
            <>
              <strong>Автономность агента</strong>
              <p>Контроль выполняется внешним PolicyEngine до запуска инструментов.</p>
              {AUTONOMY_MODE_ITEMS.map((item) => {
                const isSelected = currentAutonomy === item.mode;
                return (
                  <button
                    key={item.mode}
                    type="button"
                    role="menuitemradio"
                    aria-checked={isSelected}
                    className={isSelected ? "is-selected" : ""}
                    data-testid={`autonomy-option-${item.mode}`}
                    onClick={() => {
                      setOpenPopover(null);
                      void actions.updateActiveAgentAutonomy(item.mode);
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
          isOpen={showAtPopover}
          anchorEl={composerBoxRef.current}
          onClose={() => setAtMention(null)}
          placement="top-start"
          className="composer-popover at-file-popover"
          role="listbox"
          ariaLabel="Упомянуть файл проекта"
          testId="at-file-popover"
        >
          <strong>Файлы проекта</strong>
          {state.projectFilesError ? (
            <p className="popover-empty at-file-popover__error" data-testid="at-file-error">
              Не удалось прочитать папку проекта
            </p>
          ) : (
            <div className="popover-scroll-list">
              {atFileSuggestions.map((entry, idx) => (
                <button
                  key={entry.relative_path}
                  type="button"
                  role="option"
                  aria-selected={idx === atSelectedIndex}
                  className={idx === atSelectedIndex ? "is-selected" : ""}
                  data-testid={`at-file-option-${entry.relative_path}`}
                  onMouseDown={(e) => {
                    e.preventDefault();
                    applyAtSelection(entry);
                  }}
                >
                  <span className="at-file-option-copy">
                    {entry.kind === "directory" ? (
                      <FolderSimple size={13} aria-hidden="true" />
                    ) : (
                      <FileText size={13} aria-hidden="true" />
                    )}
                    <code>{entry.relative_path}</code>
                  </span>
                </button>
              ))}
            </div>
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
                const isSelected =
                  opt.model_id === (selectedModelOption?.model_id ?? currentModelId);
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

        {activeFollowups.length > 0 && (
          <div
            className={`followup-queue-bar ${hasPausedFollowups ? "followup-queue-bar--paused" : ""}`}
            data-testid="followup-queue-bar"
          >
            <div className="followup-queue-bar__header">
              <div className="followup-queue-bar__status">
                {hasPausedFollowups ? (
                  <>
                    <PauseCircle size={14} aria-hidden="true" />
                    <span data-testid="followup-paused-status">
                      Очередь на паузе после остановки · {activeFollowups.length}
                    </span>
                  </>
                ) : (
                  <>
                    <Queue size={14} aria-hidden="true" />
                    <span data-testid="followup-queued-status">
                      В очереди · {activeFollowups.length}
                    </span>
                  </>
                )}
              </div>
              <div className="followup-queue-bar__head-actions">
                {hasPausedFollowups && (
                  <button
                    type="button"
                    className="followup-head-btn followup-head-btn--primary"
                    data-testid="followup-resume-btn"
                    disabled={!isConnected || isBulkFollowupBusy}
                    onClick={() => void handleResumeFollowups()}
                  >
                    <Play size={11} weight="fill" aria-hidden="true" />
                    Продолжить
                  </button>
                )}
                <button
                  type="button"
                  className="followup-head-btn"
                  data-testid="followup-clear-btn"
                  disabled={!isConnected || isBulkFollowupBusy}
                  onClick={() => void handleClearFollowups()}
                >
                  Очистить
                </button>
              </div>
            </div>

            <div className="followup-queue-bar__list">
              {activeFollowups.map((item) => {
                const isEditing = editingFollowupId === item.followup_id;
                const isItemBusy = busyFollowupId === item.followup_id || isBulkFollowupBusy;
                return (
                  <div
                    key={item.followup_id}
                    className={`followup-queue-item ${item.status === "paused" ? "is-paused" : ""}`}
                    data-testid={`followup-item-${item.followup_id}`}
                  >
                    {isEditing ? (
                      <div className="followup-edit-row">
                        <input
                          type="text"
                          className="followup-edit-input"
                          aria-label="Редактировать уточнение в очереди"
                          data-testid={`followup-edit-input-${item.followup_id}`}
                          value={editingText}
                          disabled={isItemBusy}
                          autoFocus
                          onChange={(e) => setEditingText(e.target.value)}
                          onKeyDown={(e) => {
                            if (e.key === "Enter") {
                              e.preventDefault();
                              e.stopPropagation();
                              void handleSaveEditFollowup(item.followup_id);
                            } else if (e.key === "Escape") {
                              e.preventDefault();
                              e.stopPropagation();
                              setEditingFollowupId(null);
                              setEditingText("");
                            }
                          }}
                        />
                        <button
                          type="button"
                          className="followup-item-btn followup-item-btn--confirm"
                          aria-label="Сохранить уточнение"
                          data-testid={`followup-edit-save-${item.followup_id}`}
                          disabled={isItemBusy || editingText.trim().length === 0}
                          onClick={() => void handleSaveEditFollowup(item.followup_id)}
                        >
                          <Check size={13} aria-hidden="true" />
                        </button>
                        <button
                          type="button"
                          className="followup-item-btn"
                          aria-label="Отменить редактирование"
                          data-testid={`followup-edit-cancel-${item.followup_id}`}
                          disabled={isItemBusy}
                          onClick={() => {
                            setEditingFollowupId(null);
                            setEditingText("");
                          }}
                        >
                          <X size={13} aria-hidden="true" />
                        </button>
                      </div>
                    ) : (
                      <>
                        <span className="followup-queue-item__text" title={item.content}>
                          {item.content}
                        </span>
                        <div className="followup-queue-item__actions">
                          <button
                            type="button"
                            className="followup-item-btn"
                            aria-label="Редактировать уточнение"
                            title="Редактировать"
                            data-testid={`followup-edit-btn-${item.followup_id}`}
                            disabled={!isConnected || isItemBusy}
                            onClick={() => {
                              setEditingFollowupId(item.followup_id);
                              setEditingText(item.content);
                            }}
                          >
                            <PencilSimple size={13} aria-hidden="true" />
                          </button>
                          <button
                            type="button"
                            className="followup-item-btn followup-item-btn--steer"
                            aria-label="Отправить немедленно"
                            title="Отправить немедленно"
                            data-testid={`followup-promote-btn-${item.followup_id}`}
                            disabled={!isConnected || isItemBusy}
                            onClick={() => void handlePromoteFollowup(item)}
                          >
                            <Lightning size={13} aria-hidden="true" />
                          </button>
                          <button
                            type="button"
                            className="followup-item-btn followup-item-btn--danger"
                            aria-label="Удалить из очереди"
                            title="Удалить"
                            data-testid={`followup-delete-btn-${item.followup_id}`}
                            disabled={!isConnected || isItemBusy}
                            onClick={() => void handleDeleteFollowup(item.followup_id)}
                          >
                            <Trash size={13} aria-hidden="true" />
                          </button>
                        </div>
                      </>
                    )}
                  </div>
                );
              })}
            </div>
          </div>
        )}

        <div className="composer" ref={composerBoxRef} data-testid="chat-composer">
          <textarea
            ref={meta.composerInputRef}
            className="composer__input"
            rows={1}
            placeholder={
              clarificationCard
                ? "Ответьте на уточняющий вопрос агента…"
                : isRunning
                  ? "Уточнить задачу — Enter в очередь, Ctrl+Enter прервать и уточнить"
                  : isConnected
                    ? isStandaloneSession
                      ? "Напишите сообщение…"
                      : "Напишите поручение агенту… (@ для выбора файла)"
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
              const nextVal = e.target.value;
              if (sessionId) {
                actions.updateDraft(sessionId, nextVal);
              }
              const cursor = e.target.selectionStart ?? nextVal.length;
              setAtMention(findAtMention(nextVal, cursor));
            }}
            onClick={(e) => {
              const cursor = e.currentTarget.selectionStart ?? meta.currentDraft.length;
              setAtMention(findAtMention(meta.currentDraft, cursor));
            }}
            onKeyDown={handleKeyDown}
          />

          <div className="composer-toolbar">
            <div className="composer-tools">
              {activeAgent && (
                <button
                  ref={accessTriggerRef}
                  type="button"
                  className="composer-setting"
                  disabled={state.isUpdatingModel || !isConnected}
                  aria-expanded={openPopover === "access"}
                  aria-haspopup="menu"
                  aria-label="Режим автономности агента"
                  data-testid="access-selector-trigger"
                  title={
                    currentAutonomy === "full_access"
                      ? "Полный доступ — без интерактивных подтверждений"
                      : "Авто — свободная работа в проекте, запрос только на критичные действия"
                  }
                  onClick={() =>
                    setOpenPopover((prev) => (prev === "access" ? null : "access"))
                  }
                >
                  <ShieldCheck size={15} aria-hidden="true" />
                  <span>{autonomyLabel(currentAutonomy)}</span>
                  <CaretDown size={11} aria-hidden="true" />
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
                      <small>Размышление</small>
                    </span>
                  )}
                </>
              )}

              {isRunning && !clarificationCard ? (
                <div className="composer-midrun-actions">
                  {hasDraft && isConnected && (
                    <>
                      <button
                        type="button"
                        className="composer-midrun-btn"
                        disabled={isSubmittingFollowup}
                        data-testid="composer-queue-followup-btn"
                        title="Добавить в очередь (Enter)"
                        onClick={() => void handleEnqueueFromDraft()}
                      >
                        <ArrowUp size={13} weight="bold" aria-hidden="true" />
                        <span>В очередь</span>
                      </button>
                      <button
                        type="button"
                        className="composer-midrun-btn composer-midrun-btn--steer"
                        disabled={isSubmittingFollowup}
                        data-testid="composer-steer-now-btn"
                        title="Прервать текущий ход и уточнить сейчас (Ctrl+Enter)"
                        onClick={() => void handleSteerFromDraft()}
                      >
                        <Lightning size={13} weight="fill" aria-hidden="true" />
                        <span>Уточнить сейчас</span>
                      </button>
                    </>
                  )}
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
                </div>
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
                      ? `Ответить агенту (${enterToSend ? "Enter" : "Ctrl+Enter"})`
                      : isConnected
                        ? `Отправить (${enterToSend ? "Enter" : "Ctrl+Enter"})`
                        : "Поставить в очередь до восстановления сети"
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
