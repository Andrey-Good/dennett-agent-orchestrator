import { useEffect, useMemo, useRef, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import {
  ArrowsClockwise,
  BracketsCurly,
  ChatCircleText,
  Check,
  Clock,
  Copy,
  Info,
  Question,
  ShieldCheck,
  ShieldWarning,
  X,
} from "@phosphor-icons/react";

import type { PermissionDecisionRecord, RunRecord } from "../api/contracts";
import { useWorkspace } from "../state/WorkspaceProvider";

function formatClock(timestampMs: number): string {
  const date = new Date(timestampMs);
  return date.toLocaleTimeString("ru-RU", { hour: "2-digit", minute: "2-digit" });
}

function cleanPlainText(text: string): string {
  return text.replace(/`([^`]+)`/g, "$1");
}

function formatApprovalReason(reason: string): string | null {
  const trimmed = cleanPlainText(reason.trim());
  if (!trimmed) {
    return null;
  }
  if (/workspace_write_policy\s*=\s*ask_before_write/i.test(trimmed)) {
    return "Включён режим «Спрашивать запись» — подтвердите действие перед изменением файлов в папке проекта.";
  }
  return trimmed;
}

const SUGGESTED_PROMPTS = [
  "Разбери структуру проекта и предложи план работы",
  "Зафиксируй ключевые архитектурные решения в памяти",
  "Проверь слабые места и риски текущего подхода",
];

export function MessageTimeline() {
  const { state, actions, meta } = useWorkspace();
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [clarifyDrafts, setClarifyDrafts] = useState<Record<string, string>>({});

  const sessionId = state.selectedSessionId;
  const persistedMessages = state.activeDetail?.messages ?? [];
  const optimisticMessages = meta.optimisticMessagesForActiveSession;
  const queuedMessages = meta.queuedMessagesForActiveSession;
  const messages = useMemo(() => {
    if (optimisticMessages.length === 0) {
      return persistedMessages;
    }
    const persistedClientIds = new Set(
      persistedMessages
        .map((m) => m.client_message_id)
        .filter((id): id is string => Boolean(id)),
    );
    const pendingOptimistic = optimisticMessages.filter((opt) => {
      if (opt.client_message_id && persistedClientIds.has(opt.client_message_id)) {
        return false;
      }
      const hasMatchingPersistedUser = persistedMessages.some(
        (m) =>
          m.role === "user" &&
          m.content === opt.content &&
          m.created_at >= opt.created_at - 5_000,
      );
      return !hasMatchingPersistedUser;
    });
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

  useEffect(() => {
    const el = scrollRef.current;
    if (el) {
      el.scrollTop = el.scrollHeight;
    }
  }, [
    messages.length,
    queuedMessages.length,
    sessionPendingPermissions.length,
    sessionClarificationCards.length,
    sessionUnknownEffects.length,
    state.selectedSessionId,
    meta.isRunningActiveSession,
  ]);

  const handleCopy = (messageId: string, content: string) => {
    if (navigator.clipboard?.writeText) {
      void navigator.clipboard.writeText(content).catch(() => {});
    }
    setCopiedId(messageId);
    setTimeout(() => {
      setCopiedId((curr) => (curr === messageId ? null : curr));
    }, 1500);
  };

  const hasTimelineContent =
    messages.length > 0 ||
    queuedMessages.length > 0 ||
    sessionPendingPermissions.length > 0 ||
    sessionClarificationCards.length > 0 ||
    sessionUnknownEffects.length > 0;

  return (
    <div className="conversation" ref={scrollRef} data-testid="message-timeline">
      <div className="conversation-inner">
        {!hasTimelineContent ? (
          <div className="empty-state" data-testid="empty-chat-state">
            <BracketsCurly size={36} weight="light" aria-hidden="true" />
            <h2>{meta.activeProject?.name ?? "Новый разговор"}</h2>
            <p>
              Напишите поручение внизу — название чата подставится автоматически по вашему первому
              сообщению.
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

                        <button
                          type="button"
                          className="message-copy-button"
                          onClick={() => handleCopy(msg.message_id, msg.content)}
                          aria-label="Скопировать сообщение"
                          title={isCopied ? "Скопировано" : "Скопировать"}
                        >
                          {isCopied ? <Check size={13} /> : <Copy size={13} />}
                        </button>
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
                              {decision.reason}
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
                            <p>{run.error_summary ?? "Агент прервал выполнение без ответа."}</p>
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
              const formattedReason = formatApprovalReason(req.reason);
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

            {/* Inline clarification question cards */}
            {sessionClarificationCards.map((card) => {
              const draft = clarifyDrafts[card.card_id] ?? "";
              return (
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
                  <form
                    className="d2-inline-reply"
                    onSubmit={(e) => {
                      e.preventDefault();
                      if (!draft.trim()) {
                        return;
                      }
                      void actions
                        .resolveInboxCard(card.card_id, card.revision, {
                          action: "clarification",
                          answer_text: draft.trim(),
                        })
                        .then(() => {
                          setClarifyDrafts((prev) => ({ ...prev, [card.card_id]: "" }));
                        });
                    }}
                  >
                    <input
                      type="text"
                      value={draft}
                      onChange={(e) =>
                        setClarifyDrafts((prev) => ({ ...prev, [card.card_id]: e.target.value }))
                      }
                      placeholder="Ваш ответ агенту…"
                      aria-label="Ответ на уточняющий вопрос агента"
                      data-testid={`inline-clarify-input-${card.card_id}`}
                    />
                    <button
                      type="submit"
                      className="popover-primary-btn"
                      disabled={!draft.trim()}
                      data-testid={`inline-clarify-submit-${card.card_id}`}
                    >
                      Ответить
                    </button>
                  </form>
                </div>
              );
            })}

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

            {/* Queued offline messages waiting for reconnect with stable client_message_id */}
            {queuedMessages.map((q) => (
              <div
                key={q.client_message_id}
                className="queued-offline-card"
                data-testid={`queued-offline-pill-${q.client_message_id}`}
              >
                <div className="queued-offline-card__body">
                  <p className="queued-offline-card__text">{q.content}</p>
                  <div className="queued-offline-card__meta">
                    <Clock size={12} aria-hidden="true" />
                    <span>Ожидает сети — отправится один раз при восстановлении</span>
                  </div>
                </div>
                <div className="queued-offline-card__actions">
                  {state.healthSnapshot?.connected && (
                    <button
                      type="button"
                      className="popover-secondary-btn"
                      disabled={state.isFlushingQueue}
                      data-testid="flush-offline-queue-btn"
                      onClick={() => void actions.flushQueuedMessages()}
                    >
                      Отправить сейчас
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
            ))}
          </>
        )}

        {meta.isRunningActiveSession && (
          <div className="loading-lines" aria-live="polite" data-testid="thinking-indicator">
            <span />
            <span />
            <span />
          </div>
        )}
      </div>
    </div>
  );
}
