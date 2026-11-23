import { useEffect, useMemo, useRef, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { BracketsCurly, Check, Copy, Info, ShieldWarning } from "@phosphor-icons/react";

import type { PermissionDecisionRecord, RunRecord } from "../api/contracts";
import { useWorkspace } from "../state/WorkspaceProvider";

function formatClock(timestampMs: number): string {
  const date = new Date(timestampMs);
  return date.toLocaleTimeString("ru-RU", { hour: "2-digit", minute: "2-digit" });
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

  const persistedMessages = state.activeDetail?.messages ?? [];
  const optimisticMessages = meta.optimisticMessagesForActiveSession;
  const messages = useMemo(
    () =>
      optimisticMessages.length === 0
        ? persistedMessages
        : [...persistedMessages, ...optimisticMessages],
    [optimisticMessages, persistedMessages],
  );
  const runs = state.activeDetail?.runs ?? [];
  const permissionDecisions = state.activeDetail?.permission_decisions ?? [];

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
  }, [messages.length, state.selectedSessionId, meta.isRunningActiveSession]);

  const handleCopy = (messageId: string, content: string) => {
    if (navigator.clipboard?.writeText) {
      void navigator.clipboard.writeText(content).catch(() => {});
    }
    setCopiedId(messageId);
    setTimeout(() => {
      setCopiedId((curr) => (curr === messageId ? null : curr));
    }, 1500);
  };

  return (
    <div className="conversation" ref={scrollRef} data-testid="message-timeline">
      <div className="conversation-inner">
        {messages.length === 0 ? (
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
          messages.map((msg) => {
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
                            Инструмент был заблокирован политикой записи. Если проекту нужна запись
                            файлов, переключите доступ внизу на «Запись в папку».
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
          })
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
