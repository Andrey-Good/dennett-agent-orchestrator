import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import {
  ArrowsClockwise,
  ArrowSquareOut,
  Broadcast,
  Clock,
  ListChecks,
  PencilSimple,
  Plus,
  Question,
  ShieldCheck,
  Stop,
} from "@phosphor-icons/react";

import type { EffectClaimRecord, TaskRecord, TaskStatus } from "../api/contracts";
import { useWorkspace } from "../state/WorkspaceProvider";
import {
  cleanPlainText,
  formatDecisionReasonRu,
  formatHumanCountdown,
  formatScheduleSummaryRu,
} from "../utils/formatters";
import { ProjectScopeFilter } from "./ProjectScopeFilter";
import { StatusBanners } from "./StatusBanners";

type TaskFilter = "all" | "active" | "done";

export const CREATE_EVENT_DRAFT_PREFIX = "Создай событие: мне необходимо ";

function statusPillLabel(task: TaskRecord, nowMs: number, hasPendingApproval: boolean): string {
  if (hasPendingApproval && task.status === "running") {
    return "Ожидает подтверждения";
  }
  switch (task.status) {
    case "queued":
      return "Ожидает запуска";
    case "running":
      return "Выполняется";
    case "waiting_timer": {
      const target =
        task.schedule?.next_fire_at ??
        (task.schedule?.kind.kind === "once_at" ? task.schedule.kind.fire_at : null);
      if (target) {
        return `Ожидает · ${formatHumanCountdown(target, nowMs)}`;
      }
      return "Ожидает";
    }
    case "waiting_signal":
      return "Ожидает сигнала";
    case "waiting_clarification":
      return "Уточнение";
    case "waiting_approval":
      return "Ожидает подтверждения";
    case "unknown_external":
      return "Неизвестно, выполнено ли";
    case "failed":
      return "Ошибка";
    case "cancelled":
      return "Остановлено";
    case "succeeded":
      return "Готово";
  }
}

function isTerminalTaskStatus(status: TaskStatus): boolean {
  return status === "succeeded" || status === "failed" || status === "cancelled";
}

export function TasksCard() {
  const { state, actions } = useWorkspace();
  const [filter, setFilter] = useState<TaskFilter>("all");
  const [projectFilter, setProjectFilter] = useState<string | "all">("all");
  const [editingTaskId, setEditingTaskId] = useState<string | null>(null);
  const [editingGoalValue, setEditingGoalValue] = useState("");
  const [signalTaskId, setSignalTaskId] = useState<string | null>(null);
  const [signalNameInput, setSignalNameInput] = useState("");
  const [signalPayload, setSignalPayload] = useState("");
  const [replyDrafts, setReplyDrafts] = useState<Record<string, string>>({});
  const [reconcilingTaskIds, setReconcilingTaskIds] = useState<Record<string, boolean>>({});
  const [nowMs, setNowMs] = useState(() => Date.now());

  const scrollRef = useRef<HTMLDivElement | null>(null);
  const savedScrollTopRef = useRef(0);

  const activeProjects = useMemo(
    () =>
      state.projects.filter((p) => {
        const lc = p.lifecycle_state ?? "active";
        return lc !== "archived" && lc !== "deleted_record";
      }),
    [state.projects],
  );

  const activeProjectIds = useMemo(
    () => new Set(activeProjects.map((p) => p.project_id)),
    [activeProjects],
  );

  const projectById = useMemo(() => {
    const map = new Map<string, string>();
    for (const p of state.projects) {
      map.set(p.project_id, p.name);
    }
    return map;
  }, [state.projects]);

  const activeProjectTasks = useMemo(
    () => state.tasks.filter((task) => activeProjectIds.has(task.project_id)),
    [activeProjectIds, state.tasks],
  );

  const countByProjectId = useMemo(() => {
    const map = new Map<string, number>();
    for (const task of activeProjectTasks) {
      map.set(task.project_id, (map.get(task.project_id) ?? 0) + 1);
    }
    return map;
  }, [activeProjectTasks]);

  const scopedTasks = useMemo(
    () =>
      projectFilter === "all"
        ? activeProjectTasks
        : activeProjectTasks.filter((t) => t.project_id === projectFilter),
    [activeProjectTasks, projectFilter],
  );

  const activeCountInScope = useMemo(
    () => scopedTasks.filter((t) => !isTerminalTaskStatus(t.status)).length,
    [scopedTasks],
  );

  const query = state.searchQuery.trim().toLowerCase();

  const filteredTasks = useMemo(() => {
    return scopedTasks.filter((task) => {
      if (filter === "active" && isTerminalTaskStatus(task.status)) {
        return false;
      }
      if (filter === "done" && !isTerminalTaskStatus(task.status)) {
        return false;
      }
      if (query) {
        const projName = (projectById.get(task.project_id) ?? "").toLowerCase();
        return (
          task.title.toLowerCase().includes(query) ||
          task.goal.toLowerCase().includes(query) ||
          projName.includes(query)
        );
      }
      return true;
    });
  }, [filter, projectById, query, scopedTasks]);

  const unknownEffectByTaskId = useMemo(() => {
    const map = new Map<string, EffectClaimRecord>();
    for (const ef of state.effects) {
      if (ef.status === "unknown" && ef.task_id && !map.has(ef.task_id)) {
        map.set(ef.task_id, ef);
      }
    }
    return map;
  }, [state.effects]);

  const pendingApprovalSessions = useMemo(
    () =>
      new Set(
        state.pendingPermissions
          .filter((req) => req.status === "pending")
          .map((req) => req.session_id),
      ),
    [state.pendingPermissions],
  );

  // Live 1s countdown tick whenever any waiting_timer task exists.
  const hasWaitingTimer = useMemo(
    () => activeProjectTasks.some((t) => t.status === "waiting_timer"),
    [activeProjectTasks],
  );
  useEffect(() => {
    if (!hasWaitingTimer) {
      return;
    }
    const id = window.setInterval(() => setNowMs(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, [hasWaitingTimer]);

  // Preserve scroll position across live refreshes.
  useLayoutEffect(() => {
    if (scrollRef.current && savedScrollTopRef.current > 0) {
      scrollRef.current.scrollTop = savedScrollTopRef.current;
    }
  }, [filteredTasks.length]);

  const connected = Boolean(state.healthSnapshot?.connected);

  const handleCreateEventInChat = () => {
    const targetProjectId = projectFilter === "all" ? undefined : projectFilter;
    void actions.createBlankChatWithDraft(targetProjectId, CREATE_EVENT_DRAFT_PREFIX);
  };

  const startGoalEdit = (task: TaskRecord) => {
    setEditingTaskId(task.task_id);
    setEditingGoalValue(task.goal);
  };

  const commitGoalEdit = async (task: TaskRecord) => {
    const next = editingGoalValue.trim();
    setEditingTaskId(null);
    if (next && next !== task.goal) {
      const oldTitle = task.title.trim();
      const oldGoal = task.goal.trim();
      const titleDerivedFromGoal =
        oldTitle === oldGoal ||
        (oldTitle.length > 0 && oldGoal.startsWith(oldTitle.replace(/…$/, "").trim()));
      await actions.updateTaskGoal(
        task.task_id,
        next,
        titleDerivedFromGoal ? next.slice(0, 120) : undefined,
      );
    }
  };

  const runReconcile = async (taskId: string, fn: () => Promise<void>) => {
    setReconcilingTaskIds((prev) => ({ ...prev, [taskId]: true }));
    try {
      await fn();
    } finally {
      setReconcilingTaskIds((prev) => {
        const next = { ...prev };
        delete next[taskId];
        return next;
      });
    }
  };

  return (
    <main className="chat-card d2-surface-card" data-testid="tasks-card">
      <StatusBanners />

      <header className="d2-card-header">
        <div className="d2-card-header__title">
          <ListChecks size={18} aria-hidden="true" />
          <div>
            <h1>Поручения и расписания</h1>
            <p>
              Долговечные события и расписания в PostgreSQL — создавайте через диалог с агентом
            </p>
          </div>
        </div>

        <div className="d2-card-header__controls">
          <ProjectScopeFilter
            projects={activeProjects}
            selectedProjectId={projectFilter}
            onChange={setProjectFilter}
            totalCount={activeProjectTasks.length}
            countByProjectId={countByProjectId}
            ariaLabel="Фильтр поручений по проектам"
          />

          <div className="d2-segmented" role="group" aria-label="Фильтр поручений">
            <button
              type="button"
              className={filter === "all" ? "is-active" : ""}
              onClick={() => setFilter("all")}
            >
              Все ({scopedTasks.length})
            </button>
            <button
              type="button"
              className={filter === "active" ? "is-active" : ""}
              onClick={() => setFilter("active")}
            >
              Активные ({activeCountInScope})
            </button>
            <button
              type="button"
              className={filter === "done" ? "is-active" : ""}
              onClick={() => setFilter("done")}
            >
              Завершённые
            </button>
          </div>

          <button
            type="button"
            className="icon-button task-create-plus-btn"
            disabled={!connected}
            onClick={handleCreateEventInChat}
            title="Создать событие"
            aria-label="Создать событие"
            data-testid="task-create-plus-btn"
          >
            <Plus size={16} weight="bold" />
          </button>
        </div>
      </header>

      <div
        className="d2-card-scroll"
        ref={scrollRef}
        onScroll={(e) => {
          savedScrollTopRef.current = e.currentTarget.scrollTop;
        }}
      >
        <div className="d2-card-inner">
          {filteredTasks.length === 0 ? (
            <div className="d2-empty-box" data-testid="tasks-empty-state">
              <ListChecks size={26} weight="light" aria-hidden="true" />
              <p>
                {query
                  ? `По запросу «${state.searchQuery}» поручений не найдено.`
                  : "Поручений пока нет. Нажмите «+» или опишите расписание агенту в чате — он настроит событие автоматически."}
              </p>
              {!query && (
                <button
                  type="button"
                  className="popover-secondary-btn"
                  disabled={!connected}
                  data-testid="task-empty-create-btn"
                  onClick={handleCreateEventInChat}
                >
                  <Plus size={14} weight="bold" aria-hidden="true" />
                  Создать событие в чате
                </button>
              )}
            </div>
          ) : (
            <div className="d2-card-list" data-testid="tasks-list">
              {filteredTasks.map((task) => {
                const isEditing = editingTaskId === task.task_id;
                const isStopping = state.stoppingTaskIds.includes(task.task_id);
                const isReconciling = Boolean(reconcilingTaskIds[task.task_id]);
                const isTerminal = isTerminalTaskStatus(task.status);
                const canStop = !isTerminal;
                const goalMatchesTitle = task.title.trim() === task.goal.trim();
                const projectName = projectById.get(task.project_id) ?? "Проект";
                const replyDraft = replyDrafts[task.task_id] ?? "";
                const linkedUnknownEffect = unknownEffectByTaskId.get(task.task_id);
                const hasPendingApproval = pendingApprovalSessions.has(task.session_id);
                const effectivePillStatus =
                  hasPendingApproval && task.status === "running"
                    ? "waiting_approval"
                    : task.status;

                return (
                  <article
                    key={task.task_id}
                    className={`d2-item-card ${
                      task.status === "unknown_external" ? "d2-item-card--unknown" : ""
                    }`}
                    data-testid={`task-card-${task.task_id}`}
                  >
                    <div className="d2-item-card__top">
                      <div className="d2-item-card__badges">
                        <span
                          className={`d2-pill d2-pill--status-${effectivePillStatus}`}
                          data-testid={`task-status-pill-${effectivePillStatus}`}
                        >
                          {task.status === "unknown_external" && (
                            <Question size={12} weight="bold" aria-hidden="true" />
                          )}
                          {effectivePillStatus === "waiting_approval" && (
                            <ShieldCheck size={12} aria-hidden="true" />
                          )}
                          {statusPillLabel(task, nowMs, hasPendingApproval)}
                        </span>
                        <span className="d2-meta-chip">{projectName}</span>
                        {task.generation > 1 && (
                          <span
                            className="d2-meta-chip d2-meta-chip--gen"
                            data-testid={`task-generation-${task.task_id}`}
                          >
                            цель v{task.generation}
                          </span>
                        )}
                        {task.schedule && (
                          <span className="d2-meta-chip" data-testid={`task-schedule-chip-${task.task_id}`}>
                            <Clock size={11} aria-hidden="true" />
                            {task.schedule.original_expression &&
                            /[А-Яа-яЁё]/.test(task.schedule.original_expression)
                              ? cleanPlainText(task.schedule.original_expression)
                              : formatScheduleSummaryRu(task.schedule.kind, task.schedule.timezone)}
                          </span>
                        )}
                      </div>

                      <div className="d2-item-card__meta-right">
                        <button
                          type="button"
                          className="d2-link-btn"
                          data-testid={`task-open-chat-${task.task_id}`}
                          onClick={() => void actions.openSessionInChat(task.session_id)}
                        >
                          <ArrowSquareOut size={13} aria-hidden="true" />
                          Открыть чат
                        </button>
                        {!isEditing && !isTerminal && (
                          <button
                            type="button"
                            className="icon-button icon-button--xs"
                            data-testid={`task-edit-goal-btn-${task.task_id}`}
                            aria-label={`Изменить цель поручения ${task.title}`}
                            title="Изменить цель (новое поколение)"
                            onClick={() => startGoalEdit(task)}
                          >
                            <PencilSimple size={13} />
                          </button>
                        )}
                        {canStop && (
                          <button
                            type="button"
                            className="d2-danger-btn d2-danger-btn--sm"
                            disabled={isStopping}
                            data-testid={`task-stop-btn-${task.task_id}`}
                            onClick={() => void actions.cancelTask(task.task_id)}
                          >
                            <Stop size={11} weight="fill" aria-hidden="true" />
                            {isStopping ? "Остановка…" : "Stop"}
                          </button>
                        )}
                      </div>
                    </div>

                    {(!isEditing || !goalMatchesTitle) && (
                      <h3
                        className="d2-item-card__title"
                        onDoubleClick={!isTerminal ? () => startGoalEdit(task) : undefined}
                        title={!isTerminal ? "Двойной клик — изменить цель" : undefined}
                      >
                        {task.title}
                      </h3>
                    )}

                    {isEditing ? (
                      <div className="d2-inline-goal-editor">
                        <input
                          type="text"
                          className="inline-rename-input"
                          value={editingGoalValue}
                          autoFocus
                          aria-label="Новая цель поручения"
                          data-testid={`task-edit-goal-input-${task.task_id}`}
                          onChange={(e) => setEditingGoalValue(e.target.value)}
                          onKeyDown={(e) => {
                            if (e.key === "Enter") {
                              e.preventDefault();
                              void commitGoalEdit(task);
                            } else if (e.key === "Escape") {
                              e.preventDefault();
                              e.stopPropagation();
                              setEditingTaskId(null);
                            }
                          }}
                        />
                        <button
                          type="button"
                          className="popover-primary-btn"
                          data-testid={`task-edit-goal-save-${task.task_id}`}
                          onClick={() => void commitGoalEdit(task)}
                        >
                          Сохранить
                        </button>
                        <button
                          type="button"
                          className="popover-secondary-btn"
                          onClick={() => setEditingTaskId(null)}
                        >
                          Отмена
                        </button>
                      </div>
                    ) : !goalMatchesTitle ? (
                      <p
                        className="d2-item-card__detail d2-item-card__goal"
                        onDoubleClick={!isTerminal ? () => startGoalEdit(task) : undefined}
                        title={!isTerminal ? "Двойной клик — изменить цель" : undefined}
                      >
                        {task.goal}
                      </p>
                    ) : null}

                    {task.wait_detail && (
                      <p className="d2-task-note">{cleanPlainText(task.wait_detail)}</p>
                    )}

                    {task.partial_result_summary && (
                      <div className="d2-task-summary">
                        <strong>Итог / прогресс:</strong> {task.partial_result_summary}
                      </div>
                    )}

                    {task.status === "failed" && (task.error_summary || task.error_code) && (
                      <div className="d2-task-error">
                        <strong>{task.error_code ?? "ERROR"}:</strong>{" "}
                        {formatDecisionReasonRu(task.error_summary) ||
                          "Поручение завершилось с ошибкой."}
                      </div>
                    )}

                    {/* D2-UX-04 & D2-UX-05: Direct UNKNOWN reconcile actions right on task card */}
                    {task.status === "unknown_external" && (
                      <div className="d2-item-card__actions">
                        {linkedUnknownEffect && (
                          <>
                            <button
                              type="button"
                              className="popover-primary-btn"
                              disabled={isReconciling}
                              data-testid={`task-reconcile-auto-${task.task_id}`}
                              onClick={() =>
                                void runReconcile(task.task_id, () =>
                                  actions.reconcileEffect(linkedUnknownEffect.effect_id, null),
                                )
                              }
                            >
                              <ArrowsClockwise size={13} aria-hidden="true" />
                              Проверить у сервиса
                            </button>
                            <button
                              type="button"
                              className="popover-secondary-btn"
                              disabled={isReconciling}
                              data-testid={`task-reconcile-confirmed-${task.task_id}`}
                              onClick={() =>
                                void runReconcile(task.task_id, () =>
                                  actions.reconcileEffect(
                                    linkedUnknownEffect.effect_id,
                                    "mark_confirmed",
                                    "Подтверждено в разделе Поручения",
                                  ),
                                )
                              }
                            >
                              Подтвердить выполнение
                            </button>
                            <button
                              type="button"
                              className="d2-danger-btn"
                              disabled={isReconciling}
                              data-testid={`task-reconcile-not-executed-${task.task_id}`}
                              onClick={() =>
                                void runReconcile(task.task_id, () =>
                                  actions.reconcileEffect(
                                    linkedUnknownEffect.effect_id,
                                    "mark_not_executed",
                                    "Отмечено невыполненным в разделе Поручения",
                                  ),
                                )
                              }
                            >
                              Отметить невыполненным
                            </button>
                          </>
                        )}
                        <button
                          type="button"
                          className="popover-secondary-btn"
                          onClick={() => actions.setActiveView("inbox")}
                        >
                          Открыть во Входящих
                        </button>
                      </div>
                    )}

                    {task.status === "waiting_clarification" && (
                      <form
                        className="d2-inline-reply"
                        onSubmit={(e) => {
                          e.preventDefault();
                          if (!replyDraft.trim()) {
                            return;
                          }
                          void actions.replyToTask(task.task_id, replyDraft.trim()).then(() => {
                            setReplyDrafts((prev) => ({ ...prev, [task.task_id]: "" }));
                          });
                        }}
                      >
                        <input
                          type="text"
                          value={replyDraft}
                          onChange={(e) =>
                            setReplyDrafts((prev) => ({ ...prev, [task.task_id]: e.target.value }))
                          }
                          placeholder="Ответить на вопрос агента…"
                          aria-label={`Ответ для поручения ${task.title}`}
                        />
                        <button
                          type="submit"
                          className="popover-primary-btn"
                          disabled={!replyDraft.trim()}
                        >
                          Ответить
                        </button>
                      </form>
                    )}

                    {task.status === "waiting_signal" && (
                      <div className="d2-signal-bar">
                        {signalTaskId === task.task_id ? (
                          <form
                            className="d2-inline-reply"
                            onSubmit={(e) => {
                              e.preventDefault();
                              const resolvedSignalName =
                                signalNameInput.trim() || task.expected_signal_name || "ready";
                              void actions
                                .sendTaskSignal(
                                  task.task_id,
                                  resolvedSignalName,
                                  signalPayload.trim(),
                                )
                                .then(() => {
                                  setSignalTaskId(null);
                                  setSignalNameInput("");
                                  setSignalPayload("");
                                });
                            }}
                          >
                            <input
                              type="text"
                              className="d2-signal-name-input"
                              value={signalNameInput}
                              onChange={(e) => setSignalNameInput(e.target.value)}
                              onKeyDown={(e) => {
                                if (e.key === "Escape") {
                                  e.preventDefault();
                                  e.stopPropagation();
                                  setSignalTaskId(null);
                                }
                              }}
                              placeholder="Имя сигнала…"
                              aria-label="Имя сигнала"
                              data-testid={`task-signal-name-input-${task.task_id}`}
                            />
                            <input
                              type="text"
                              value={signalPayload}
                              onChange={(e) => setSignalPayload(e.target.value)}
                              onKeyDown={(e) => {
                                if (e.key === "Escape") {
                                  e.preventDefault();
                                  e.stopPropagation();
                                  setSignalTaskId(null);
                                }
                              }}
                              placeholder="Текст сигнала (необязательно)…"
                              aria-label="Данные сигнала"
                              data-testid={`task-signal-payload-input-${task.task_id}`}
                            />
                            <button
                              type="submit"
                              className="popover-primary-btn"
                              data-testid={`task-signal-submit-${task.task_id}`}
                            >
                              Отправить сигнал
                            </button>
                            <button
                              type="button"
                              className="popover-secondary-btn"
                              onClick={() => setSignalTaskId(null)}
                            >
                              Отмена
                            </button>
                          </form>
                        ) : (
                          <div className="d2-item-card__actions">
                            {task.expected_signal_name && (
                              <button
                                type="button"
                                className="popover-primary-btn"
                                data-testid={`task-signal-quick-send-${task.task_id}`}
                                onClick={() =>
                                  void actions.sendTaskSignal(
                                    task.task_id,
                                    task.expected_signal_name!,
                                    "",
                                  )
                                }
                              >
                                <Broadcast size={13} aria-hidden="true" />
                                Отправить сигнал «{task.expected_signal_name}»
                              </button>
                            )}
                            <button
                              type="button"
                              className="popover-secondary-btn"
                              data-testid={`task-signal-open-${task.task_id}`}
                              onClick={() => {
                                setSignalTaskId(task.task_id);
                                setSignalNameInput(task.expected_signal_name ?? "ready");
                                setSignalPayload("");
                              }}
                            >
                              <Broadcast size={13} aria-hidden="true" />
                              {task.expected_signal_name ? "+ данные" : "Подать сигнал…"}
                            </button>
                          </div>
                        )}
                      </div>
                    )}

                    <div className="d2-task-footer">
                      <span>Ходов: {task.usage.turns_used}</span>
                      {task.budget.max_turns && <span>Лимит ходов: {task.budget.max_turns}</span>}
                      {task.usage.tokens_used > 0 && (
                        <span>Токенов: {task.usage.tokens_used}</span>
                      )}
                    </div>
                  </article>
                );
              })}
            </div>
          )}
        </div>
      </div>
    </main>
  );
}
