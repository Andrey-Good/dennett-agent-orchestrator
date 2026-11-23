import { useEffect, useRef } from "react";

import type { MessageRecord, RunRecord } from "../api/contracts";
import { useWorkspace } from "../state/WorkspaceProvider";

function formatTime(tsMs: number): string {
  try {
    return new Intl.DateTimeFormat("ru-RU", {
      hour: "2-digit",
      minute: "2-digit",
    }).format(new Date(tsMs));
  } catch {
    return "";
  }
}

function findRunForMessage(message: MessageRecord, runs: RunRecord[]): RunRecord | undefined {
  if (!message.run_id) {
    return undefined;
  }
  return runs.find((run) => run.run_id === message.run_id);
}

export function MessageTimeline() {
  const { state } = useWorkspace();
  const bottomRef = useRef<HTMLDivElement | null>(null);

  const messages = state.activeDetail?.messages ?? [];
  const runs = state.activeDetail?.runs ?? [];

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "auto", block: "end" });
  }, [messages.length, state.selectedSessionId]);

  return (
    <div
      className="timeline-viewport"
      role="log"
      aria-label="История разговора"
      aria-live="polite"
      data-testid="message-timeline"
    >
      <div className="timeline-inner">
        {messages.length === 0 ? (
          <div className="message-row message-row--system">
            <div className="message-bubble--system">
              Сообщений пока нет. Опишите задачу для агента — история и память сохраняются в dennett-head.
            </div>
          </div>
        ) : (
          messages.map((message) => {
            const run = findRunForMessage(message, runs);
            const showCancelledBadge =
              run?.status === "cancelled" &&
              (message.role === "user" || message.role === "assistant");
            const showFailedBadge =
              run?.status === "failed" && message.role === "user";
            const showDegradedBadge =
              run?.status === "degraded" && message.role === "assistant";

            return (
              <article
                key={message.message_id}
                className={`message-row message-row--${message.role}`}
                data-role={message.role}
              >
                <div className={`message-bubble--${message.role}`}>
                  {message.content}
                </div>
                <div className="message-meta">
                  <time dateTime={new Date(message.created_at).toISOString()}>
                    {formatTime(message.created_at)}
                  </time>
                  {showCancelledBadge && (
                    <span className="run-chip run-chip--cancelled" data-testid="run-status-cancelled">
                      Остановлено (Cancelled)
                    </span>
                  )}
                  {showFailedBadge && (
                    <span className="run-chip run-chip--failed">
                      Ошибка запуска
                    </span>
                  )}
                  {showDegradedBadge && (
                    <span className="run-chip run-chip--degraded">
                      Без memoryd
                    </span>
                  )}
                </div>
              </article>
            );
          })
        )}
        <div ref={bottomRef} />
      </div>
    </div>
  );
}
