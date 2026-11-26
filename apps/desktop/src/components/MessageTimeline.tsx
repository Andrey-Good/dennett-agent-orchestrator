import { useEffect, useMemo, useRef, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import {
  ArrowsClockwise,
  BracketsCurly,
  CalendarBlank,
  ChatCircleText,
  Check,
  Clock,
  Copy,
  Info,
  Play,
  Question,
  ShieldCheck,
  ShieldWarning,
  Stop,
  WarningCircle,
  X,
} from "@phosphor-icons/react";

import type { PermissionDecisionRecord, RunRecord } from "../api/contracts";
import { useWorkspace } from "../state/WorkspaceProvider";
import { cleanPlainText, formatDecisionReasonRu } from "../utils/formatters";

function formatClock(timestampMs: number): string {
  const date = new Date(timestampMs);
  return date.toLocaleTimeString("ru-RU", { hour: "2-digit", minute: "2-digit" });
}

const SUGGESTED_PROMPTS = [
  "Разбери структуру проекта и предложи план работы",
  "Зафиксируй ключевые архитектурные решения в памяти",
  "Проверь слабые места и риски текущего подхода",
];

const STANDALONE_SUGGESTED_PROMPTS = [
  "Объясни сложную тему простыми словами с примерами",
  "Помоги структурировать идею и разобрать плюсы и минусы",
  "Подготовь короткий черновик документа или заметки",
];

export function MessageTimeline() {
  const { state, actions, meta } = useWorkspace();
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const isNearBottomRef = useRef(true);
  const lastSessionIdRef = useRef<string | null>(null);
  const copyTimerRef = useRef<number | null>(null);
  const [copiedId, setCopiedId] = useState<string | null>(null);

  useEffect(() => {
    return () => {
      if (copyTimerRef.current !== null) {
        window.clearTimeout(copyTimerRef.current);
      }
    };
  }, []);

  const sessionId = state.selectedSessionId;
  const persistedMessages = state.activeDetail?.messages ?? [];
  const optimisticMessages = meta.optimisticMessagesForActiveSession;
  const queuedMessages = meta.queuedMessagesForActiveSession;

  // D2-BUG-06: Deduplicate optimistic bubbles strictly by client_message_id so
  // two intentional identical messages ("ок", "ок") never collide or hide each other.
  const messages = useMemo(() => {
    if (optimisticMessages.length === 0) {
      return persistedMessages;
    }
    const persistedClientIds = new Set(
      persistedMessages
        .map((m) => m.client_message_id)
        .filter((id): id is string => Boolean(id)),
    );
    const pendingOptimistic = optimisticMessages.filter(
      (opt) => !(opt.client_message_id && persistedClientIds.has(opt.client_message_id)),
    );
    return pendingOptimistic.length === 0
      ? persistedMessages
      : [...persistedMessages, ...pendingOptimistic];
  }, [optimisticMessages, persistedMessages]);

  const runs = state.activeDetail?.runs ?? [];
  const permissionDecisions = state.activeDetail?.permission_decisions ?? [];

  const sessionPendingPermissions = useMemo(
    () =>
      sessionId
        ? state.pendingPermissions.filter(
            (req) => req.session_id === sessionId && req.status === "pending",
          )
        : [],
    [sessionId, state.pendingPermissions],
  );

  const sessionClarificationCards = useMemo(
    () =>
      sessionId
        ? state.inboxCards.filter(
            (card) =>
              card.session_id === sessionId &&
              card.status === "open" &&
              card.kind === "clarification",
          )
        : [],
    [sessionId, state.inboxCards],
  );

  const sessionBudgetCards = useMemo(
    () =>
      sessionId
        ? state.inboxCards.filter(
            (card) =>
              card.session_id === sessionId &&
              card.status === "open" &&
              card.kind === "budget_exceeded",
          )
        : [],
    [sessionId, state.inboxCards],
  );

  const sessionMissedScheduleCards = useMemo(
    () =>
      sessionId
        ? state.inboxCards.filter(
            (card) =>
              card.session_id === sessionId &&
              card.status === "open" &&
              card.kind === "missed_schedule",
          )
        : [],
    [sessionId, state.inboxCards],
  );

  const sessionUnknownEffects = useMemo(
    () =>
      sessionId
        ? state.effects.filter((ef) => ef.session_id === sessionId && ef.status === "unknown")
        : [],
    [sessionId, state.effects],
  );

  const runsById = useMemo(() => {
    const map = new Map<string, RunRecord>();
    for (const run of runs) {
      map.set(run.run_id, run);
    }
    return map;
  }, [runs]);

  const deniedDecisionsByRun = useMemo(() => {
    const map = new Map<string, PermissionDecisionRecord[]>();
    for (const decision of permissionDecisions) {
      if (decision.effect !== "deny" || !decision.run_id) {
        continue;
      }
      const list = map.get(decision.run_id) ?? [];
      list.push(decision);
      map.set(decision.run_id, list);
    }
    return map;
  }, [permissionDecisions]);

  const runIdsWithAssistantMessage = useMemo(() => {
    const set = new Set<string>();
    for (const msg of persistedMessages) {
      if (msg.role === "assistant" && msg.run_id) {
        set.add(msg.run_id);
      }
    }
    return set;
  }, [persistedMessages]);

  const activeLiveRun = meta.activeLiveRun;
  const liveRunAlreadyPersisted = Boolean(
    activeLiveRun && runIdsWithAssistantMessage.has(activeLiveRun.run_id),
  );
  const liveStreamingText =
    activeLiveRun && !liveRunAlreadyPersisted && activeLiveRun.streaming_text.trim().length > 0
      ? activeLiveRun.streaming_text
      : "";

  // D2-UX-06: Preserve manual scroll position when user scrolled up to read history;
  // auto-scroll to bottom on session switch or when already near the bottom.
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) {
      return;
    }
    const sessionChanged = lastSessionIdRef.current !== state.selectedSessionId;
    if (sessionChanged) {
      lastSessionIdRef.current = state.selectedSessionId;
      isNearBottomRef.current = true;
      el.scrollTop = el.scrollHeight;
      return;
    }
    if (isNearBottomRef.current) {
      el.scrollTop = el.scrollHeight;
    }
  }, [
    messages.length,
    queuedMessages.length,
    sessionPendingPermissions.length,
    sessionClarificationCards.length,
    sessionBudgetCards.length,
    sessionMissedScheduleCards.length,
    sessionUnknownEffects.length,
    state.selectedSessionId,
    meta.isRunningActiveSession,
    meta.activeLiveRun?.streaming_text,
    meta.activeFollowups.length,
  ]);

  // Keep bottom-pinned chat aligned when composer/follow-up queue bar resizes the viewport.
  useEffect(() => {
    const el = scrollRef.current;
    if (!el || typeof ResizeObserver === "undefined") {
      return;
    }
    const observer = new ResizeObserver(() => {
      if (isNearBottomRef.current) {
        el.scrollTop = el.scrollHeight;
      }
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const handleTimelineScroll = () => {
    const el = scrollRef.current;
    if (!el) {
      return;
    }
    const distanceFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
    isNearBottomRef.current = distanceFromBottom < 80;
  };

  const handleCopy = (messageId: string, content: string) => {
    if (navigator.clipboard?.writeText) {
      void navigator.clipboard.writeText(content).catch(() => {});
    }
    setCopiedId(messageId);
    if (copyTimerRef.current !== null) {
      window.clearTimeout(copyTimerRef.current);
    }
    copyTimerRef.current = window.setTimeout(() => {
      setCopiedId((curr) => (curr === messageId ? null : curr));
      copyTimerRef.current = null;
    }, 1500);
  };

  const hasTimelineContent =
    messages.length > 0 ||
    queuedMessages.length > 0 ||
    sessionPendingPermissions.length > 0 ||
    sessionClarificationCards.length > 0 ||
    sessionBudgetCards.length > 0 ||
    sessionMissedScheduleCards.length > 0 ||
    sessionUnknownEffects.length > 0 ||
    (meta.isRunningActiveSession && liveStreamingText.length > 0);

  return (
    <div
      className="conversation"
      ref={scrollRef}
      onScroll={handleTimelineScroll}
      data-testid="message-timeline"
    >
      <div className="conversation-inner">
        {!hasTimelineContent ? (
          <div className="empty-state" data-testid="empty-chat-state">
            <BracketsCurly size={36} weight="light" aria-hidden="true" />
            {(meta.activeSession?.session_kind ?? "user_chat") === "task_run" ? (
              <>
                <h2>{meta.activeSession?.title ?? "Журнал фонового события"}</h2>
                <p>
                  Запусков по этому событию ещё не было — журнал выполнения появится здесь после
                  срабатывания расписания или сигнала.
                </p>
              </>
            ) : meta.isActiveProjectArchived ? (
              <>
                <h2>{meta.activeProject?.name ?? "Архивный проект"}</h2>
                <p>
                  Проект находится в архиве. В этом чате нет сообщений — чтобы продолжить диалог,
                  верните проект из архива.
                </p>
              </>
            ) : meta.isStandaloneActiveSession ? (
              <>
                <h2>{meta.activeSession?.title || "Новый чат"}</h2>
                <p>Задайте вопрос или обсудите идею без привязки к папке проекта</p>
                <div className="prompt-suggestions">
                  {STANDALONE_SUGGESTED_PROMPTS.map((prompt) => (
                    <button
                      key={prompt}
                      type="button"
                      onClick={() => {
                        if (state.selectedSessionId) {
                          actions.updateDraft(state.selectedSessionId, prompt);
                          meta.composerInputRef.current?.focus();
                        }
                      }}
                    >
                      {prompt}
                    </button>
                  ))}
                </div>
              </>
            ) : (
              <>
                <h2>{meta.activeProject?.name ?? "Новый разговор"}</h2>
                <p>
                  Напишите поручение внизу — название чата подставится автоматически по вашему
                  первому сообщению.
                </p>
                <div className="prompt-suggestions">
                  {SUGGESTED_PROMPTS.map((prompt) => (
                    <button
                      key={prompt}
                      type="button"
                      onClick={() => {
                        if (state.selectedSessionId) {
                          actions.updateDraft(state.selectedSessionId, prompt);
                          meta.composerInputRef.current?.focus();
                        }
                      }}
                    >
                      {prompt}
                    </button>
                  ))}
                </div>
              </>
            )}
          </div>
        ) : (
          <>
            {messages.map((msg) => {
              const run = msg.run_id ? runsById.get(msg.run_id) : undefined;
              const isUser = msg.role === "user";
              const isCopied = copiedId === msg.message_id;
              const isOptimistic = msg.correlation_id === "pending";
              const deniedForRun =
                isUser && run ? (deniedDecisionsByRun.get(run.run_id) ?? []) : [];
              const missingAssistantReply =
                isUser &&
                run !== undefined &&
                run.assistant_message_id === null &&
                !runIdsWithAssistantMessage.has(run.run_id) &&
                (run.status === "succeeded" || run.status === "failed");

              return (
                <div key={msg.message_id} className="message-turn-group">
                  <article
                    className={`message ${isUser ? "message--user" : "message--assistant"} ${
                      isOptimistic ? "message--optimistic" : ""
                    }`}
                    data-testid={`message-${msg.role}`}
                  >
                    <div className="message-block">
                      <button
                        type="button"
                        className={`message-copy-button ${isCopied ? "is-copied" : ""}`}
                        onClick={() => handleCopy(msg.message_id, msg.content)}
                        aria-label={isCopied ? "Скопировано" : "Скопировать сообщение"}
                        data-testid={`message-copy-btn-${msg.message_id}`}
                        title={isCopied ? "Скопировано" : "Скопировать"}
                      >
                        {isCopied ? (
                          <>
                            <Check size={13} aria-hidden="true" />
                            <span className="message-copy-feedback">Скопировано</span>
                          </>
                        ) : (
                          <Copy size={13} aria-hidden="true" />
                        )}
                      </button>

                      <div className="message-copy">
                        {isUser ? (
                          <p className="message-user-text">{msg.content}</p>
                        ) : (
                          <div className="message-markdown">
                            <ReactMarkdown remarkPlugins={[remarkGfm]} skipHtml>
                              {msg.content}
                            </ReactMarkdown>
                          </div>
                        )}
                      </div>

                      <div className="message-footer">
                        <span className="message-time">
                          {isOptimistic ? "отправка…" : formatClock(msg.created_at)}
                        </span>

                        {run && isUser && run.status === "cancelled" && (
                          <span
                            className="run-tag run-tag--cancelled"
                            data-testid="run-status-cancelled"
                          >
                            Остановлено (Cancelled)
                          </span>
                        )}
                        {run && isUser && run.status === "superseded" && (
                          <span className="run-tag" data-testid="run-status-superseded">
                            Устарело (цель обновлена)
                          </span>
                        )}
                        {run && isUser && run.status === "unknown_external" && (
                          <span
                            className="run-tag run-tag--unknown"
                            data-testid="run-status-unknown"
                          >
                            Неизвестно, выполнено ли
                          </span>
                        )}
                        {run && isUser && run.status === "failed" && (
                          <span className="run-tag run-tag--failed">
                            Ошибка{run.error_code ? ` · ${run.error_code}` : ""}
                          </span>
                        )}
                        {run && !isUser && run.rehydrated && (
                          <span className="run-tag">Контекст восстановлен</span>
                        )}
                      </div>
                    </div>
                  </article>

                  {deniedForRun.length > 0 && (
                    <div
                      className="turn-notice turn-notice--policy"
                      role="status"
                      data-testid="policy-denial-notice"
                    >
                      <ShieldWarning size={16} aria-hidden="true" />
                      <div className="turn-notice__body">
                        <strong>Политика доступа отклонила действие</strong>
                        <ul>
                          {deniedForRun.map((decision) => (
                            <li key={decision.decision_id}>
                              <code>{decision.tool_name}</code>
                              {decision.target_pattern ? ` (${decision.target_pattern})` : ""}:{" "}
                              {formatDecisionReasonRu(decision.reason)}
                            </li>
                          ))}
                        </ul>
                      </div>
                    </div>
                  )}

                  {missingAssistantReply && run && (
                    <div
                      className={`turn-notice ${
                        run.status === "failed" ? "turn-notice--danger" : "turn-notice--empty"
                      }`}
                      role="status"
                      data-testid="empty-run-notice"
                    >
                      <Info size={16} aria-hidden="true" />
                      <div className="turn-notice__body">
                        {run.status === "failed" ? (
                          <>
                            <strong>Ход завершился ошибкой</strong>
                            <p>
                              {formatDecisionReasonRu(run.error_summary) ||
                                "Агент прервал выполнение без ответа."}
                            </p>
                          </>
                        ) : deniedForRun.length > 0 ? (
                          <>
                            <strong>Агент завершил ход без текстового ответа</strong>
                            <p>
                              Инструмент был заблокирован политикой записи. Если проекту нужна
                              запись файлов, переключите доступ внизу на «Запись в папку» или
                              «Спрашивать запись».
                            </p>
                          </>
                        ) : (
                          <>
                            <strong>Агент завершил ход без текстового ответа</strong>
                            <p>Сервер сохранил статус выполнения, но модель не вернула текст.</p>
                          </>
                        )}
                      </div>
                    </div>
                  )}
                </div>
              );
            })}

            {/* Live interactive ask_before_write permission prompt directly inside active chat */}
            {sessionPendingPermissions.map((req) => {
              const formattedReason = formatDecisionReasonRu(req.reason);
              return (
                <div
                  key={req.request_id}
                  className="inline-action-card inline-action-card--approval"
                  role="alert"
                  data-testid={`inline-permission-card-${req.request_id}`}
                >
                  <div className="inline-action-card__head">
                    <ShieldCheck size={16} aria-hidden="true" />
                    <strong>Агент запрашивает разрешение на действие</strong>
                  </div>
                  <p className="inline-action-card__desc">
                    Инструмент <code>{req.tool_name}</code> →{" "}
                    <code>
                      {req.normalized_targets.length > 0
                        ? req.normalized_targets.join(", ")
                        : req.target_summary}
                    </code>
                  </p>
                  {formattedReason && (
                    <p className="inline-action-card__reason">{formattedReason}</p>
                  )}
                  {req.preview_snippet && (
                    <details
                      className="d2-preview-snippet"
                      open
                      data-testid={`inline-permission-snippet-${req.request_id}`}
                    >
                      <summary>Предпросмотр изменений</summary>
                      <pre>{req.preview_snippet}</pre>
                    </details>
                  )}
                  <div className="inline-action-card__actions">
                    <button
                      type="button"
                      className="popover-primary-btn"
                      data-testid={`inline-approve-once-${req.request_id}`}
                      onClick={() =>
                        void actions.resolvePermissionRequest(
                          req.request_id,
                          "approve_once",
                          req.revision,
                        )
                      }
                    >
                      <Check size={13} aria-hidden="true" />
                      Разрешить один раз
                    </button>
                    <button
                      type="button"
                      className="popover-secondary-btn"
                      data-testid={`inline-approve-project-${req.request_id}`}
                      onClick={() =>
                        void actions.resolvePermissionRequest(
                          req.request_id,
                          "approve_project",
                          req.revision,
                        )
                      }
                    >
                      Разрешить для проекта
                    </button>
                    <button
                      type="button"
                      className="d2-danger-btn"
                      data-testid={`inline-deny-${req.request_id}`}
                      onClick={() =>
                        void actions.resolvePermissionRequest(req.request_id, "deny", req.revision)
                      }
                    >
                      <X size={13} aria-hidden="true" />
                      Отклонить
                    </button>
                  </div>
                </div>
              );
            })}

            {/* Inline clarification question cards — D2-UX-05: single input via bottom Composer */}
            {sessionClarificationCards.map((card) => (
              <div
                key={card.card_id}
                className="inline-action-card inline-action-card--clarify"
                role="status"
                data-testid={`inline-clarification-card-${card.card_id}`}
              >
                <div className="inline-action-card__head">
                  <ChatCircleText size={16} aria-hidden="true" />
                  <strong>{cleanPlainText(card.subject)}</strong>
                </div>
                <p className="inline-action-card__reason">{cleanPlainText(card.detail)}</p>
                <div className="inline-action-card__actions">
                  <button
                    type="button"
                    className="popover-secondary-btn"
                    onClick={() => meta.composerInputRef.current?.focus()}
                  >
                    Ответить в поле ввода ниже
                  </button>
                  {card.task_id && (
                    <button
                      type="button"
                      className="d2-danger-btn"
                      data-testid={`inline-clarify-stop-${card.card_id}`}
                      onClick={() => void actions.cancelTask(card.task_id!)}
                    >
                      <Stop size={12} weight="fill" aria-hidden="true" />
                      Остановить поручение
                    </button>
                  )}
                </div>
              </div>
            ))}

            {/* D2-UX-05: Inline budget_exceeded cards */}
            {sessionBudgetCards.map((card) => (
              <div
                key={card.card_id}
                className="inline-action-card inline-action-card--approval"
                role="alert"
                data-testid={`inline-budget-card-${card.card_id}`}
              >
                <div className="inline-action-card__head">
                  <WarningCircle size={16} aria-hidden="true" />
                  <strong>{cleanPlainText(card.subject)}</strong>
                </div>
                <p className="inline-action-card__reason">
                  {formatDecisionReasonRu(card.detail)}
                </p>
                <div className="inline-action-card__actions">
                  <button
                    type="button"
                    className="popover-primary-btn"
                    data-testid={`inline-budget-continue-${card.card_id}`}
                    onClick={() =>
                      void actions.resolveInboxCard(card.card_id, card.revision, {
                        action: "increase_budget",
                        additional_turns: 4,
                        additional_wall_secs: 600,
                      })
                    }
                  >
                    <Play size={12} weight="fill" aria-hidden="true" />
                    Добавить бюджет и продолжить
                  </button>
                  <button
                    type="button"
                    className="d2-danger-btn"
                    data-testid={`inline-budget-stop-${card.card_id}`}
                    onClick={() =>
                      void actions.resolveInboxCard(card.card_id, card.revision, {
                        action: "stop_task",
                      })
                    }
                  >
                    <Stop size={12} weight="fill" aria-hidden="true" />
                    Завершить поручение
                  </button>
                </div>
              </div>
            ))}

            {/* D2-UX-05: Inline missed_schedule cards */}
            {sessionMissedScheduleCards.map((card) => (
              <div
                key={card.card_id}
                className="inline-action-card inline-action-card--clarify"
                role="status"
                data-testid={`inline-missed-schedule-card-${card.card_id}`}
              >
                <div className="inline-action-card__head">
                  <CalendarBlank size={16} aria-hidden="true" />
                  <strong>{cleanPlainText(card.subject)}</strong>
                </div>
                <p className="inline-action-card__reason">
                  {formatDecisionReasonRu(card.detail)}
                </p>
                <div className="inline-action-card__actions">
                  <button
                    type="button"
                    className="popover-primary-btn"
                    data-testid={`inline-missed-run-now-${card.card_id}`}
                    onClick={() =>
                      void actions.resolveInboxCard(card.card_id, card.revision, {
                        action: "missed_schedule",
                        run_now: true,
                      })
                    }
                  >
                    <Play size={12} weight="fill" aria-hidden="true" />
                    Запустить сейчас (1 раз)
                  </button>
                  <button
                    type="button"
                    className="popover-secondary-btn"
                    data-testid={`inline-missed-skip-${card.card_id}`}
                    onClick={() =>
                      void actions.resolveInboxCard(card.card_id, card.revision, {
                        action: "missed_schedule",
                        run_now: false,
                      })
                    }
                  >
                    Пропустить до следующего окна
                  </button>
                </div>
              </div>
            ))}

            {/* Inline UNKNOWN external effect warning cards */}
            {sessionUnknownEffects.map((ef) => (
              <div
                key={ef.effect_id}
                className="inline-action-card inline-action-card--unknown"
                role="alert"
                data-testid={`inline-unknown-effect-card-${ef.effect_id}`}
              >
                <div className="inline-action-card__head">
                  <Question size={16} weight="bold" aria-hidden="true" />
                  <strong>Неизвестно, выполнено ли (UNKNOWN)</strong>
                </div>
                <p className="inline-action-card__desc">
                  <code>{ef.operation}</code> → <code>{ef.target_recipient}</code> (ключ:{" "}
                  <code>{ef.effect_idempotency_key}</code>)
                </p>
                <p className="inline-action-card__reason">
                  Подтверждение от внешнего сервиса потеряно. Автоматический слепой повтор
                  заблокирован, чтобы исключить дубликат.
                </p>
                <div className="inline-action-card__actions">
                  <button
                    type="button"
                    className="popover-primary-btn"
                    onClick={() => void actions.reconcileEffect(ef.effect_id, null)}
                  >
                    <ArrowsClockwise size={13} aria-hidden="true" />
                    Проверить у сервиса
                  </button>
                  <button
                    type="button"
                    className="popover-secondary-btn"
                    onClick={() =>
                      void actions.reconcileEffect(
                        ef.effect_id,
                        "mark_confirmed",
                        "Подтверждено в чате",
                      )
                    }
                  >
                    Подтвердить выполнение
                  </button>
                  <button
                    type="button"
                    className="d2-danger-btn"
                    onClick={() =>
                      void actions.reconcileEffect(
                        ef.effect_id,
                        "mark_not_executed",
                        "Отмечено невыполненным в чате",
                      )
                    }
                  >
                    Отметить невыполненным
                  </button>
                </div>
              </div>
            ))}

            {/* Queued offline messages — D2-BUG-06: owner-confirmed flush after reconnect */}
            {queuedMessages.map((q) => {
              const connected = Boolean(state.healthSnapshot?.connected);
              const canFlushNow = connected && state.isSyncedAfterConnect && !state.isFlushingQueue;
              return (
                <div
                  key={q.client_message_id}
                  className="queued-offline-card"
                  data-testid={`queued-offline-pill-${q.client_message_id}`}
                >
                  <div className="queued-offline-card__body">
                    <p className="queued-offline-card__text">{q.content}</p>
                    <div className="queued-offline-card__meta">
                      <Clock size={12} aria-hidden="true" />
                      <span>
                        {connected
                          ? state.isSyncedAfterConnect
                            ? "Связь восстановлена — нажмите «Отправить сейчас», чтобы отправить один раз"
                            : "Синхронизация состояния с сервером…"
                          : "Ожидает сети — можно отправить после восстановления связи"}
                      </span>
                    </div>
                  </div>
                  <div className="queued-offline-card__actions">
                    {connected && (
                      <button
                        type="button"
                        className="popover-primary-btn"
                        disabled={!canFlushNow}
                        data-testid="flush-offline-queue-btn"
                        onClick={() => void actions.flushQueuedMessages()}
                      >
                        {state.isFlushingQueue ? "Отправка…" : "Отправить сейчас"}
                      </button>
                    )}
                    <button
                      type="button"
                      className="icon-button icon-button--xs"
                      aria-label="Отменить отложенное сообщение"
                      data-testid={`cancel-queued-${q.client_message_id}`}
                      onClick={() => actions.cancelQueuedMessage(q.client_message_id)}
                    >
                      <X size={13} />
                    </button>
                  </div>
                </div>
              );
            })}
          </>
        )}

        {meta.isRunningActiveSession &&
          !meta.isAwaitingApprovalActiveSession &&
          (liveStreamingText ? (
            <article
              className="message message--assistant message-row assistant live-streaming-bubble"
              aria-live="polite"
              data-testid="live-streaming-bubble"
            >
              <div className="message-block">
                <div className="message-copy">
                  <div className="message-markdown">
                    <ReactMarkdown remarkPlugins={[remarkGfm]} skipHtml>
                      {liveStreamingText}
                    </ReactMarkdown>
                  </div>
                </div>
                <div className="live-working-indicator" data-testid="live-working-indicator">
                  <span className="live-working-indicator__dot" aria-hidden="true" />
                  <span>Продолжает работу…</span>
                </div>
              </div>
            </article>
          ) : (
            <div className="loading-lines" aria-live="polite" data-testid="thinking-indicator">
              <span />
              <span />
              <span />
            </div>
          ))}
      </div>
    </div>
  );
}
