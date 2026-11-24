import { useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import {
  ArrowSquareOut,
  Broadcast,
  CaretDown,
  Check,
  Clock,
  Coin,
  ListChecks,
  PencilSimple,
  PaperPlaneTilt,
  Question,
  Stop,
} from "@phosphor-icons/react";

import type {
  BudgetExceedPolicy,
  TaskBudget,
  TaskRecord,
  TaskSchedule,
  TaskStatus,
} from "../api/contracts";
import { useWorkspace } from "../state/WorkspaceProvider";
import { StatusBanners } from "./StatusBanners";

type SchedulePreset = "now" | "in_10s" | "in_5m" | "in_1h" | "daily" | "signal";
type BudgetPreset = "standard" | "turns_3" | "turns_10" | "wall_60";
type TaskPopover = "schedule" | "budget" | null;
type TaskFilter = "all" | "active" | "done";

function detectLocalTimezone(): string {
  try {
    const tz = Intl.DateTimeFormat().resolvedOptions().timeZone;
    if (tz && /^[A-Za-z0-9/_+-]+$/.test(tz)) {
      return tz;
    }
  } catch {
    // Fallback to UTC.
  }
  return "UTC";
}

function formatCountdown(targetMs: number, nowMs: number): string {
  const diffSec = Math.max(0, Math.ceil((targetMs - nowMs) / 1000));
  const hrs = Math.floor(diffSec / 3600);
  const mins = Math.floor((diffSec % 3600) / 60);
  const secs = diffSec % 60;
  if (hrs > 0) {
    return `${String(hrs).padStart(2, "0")}:${String(mins).padStart(2, "0")}:${String(secs).padStart(2, "0")}`;
  }
  return `${String(mins).padStart(2, "0")}:${String(secs).padStart(2, "0")}`;
}

function statusPillLabel(task: TaskRecord, nowMs: number): string {
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
        return `Ожидает · ${formatCountdown(target, nowMs)}`;
      }
      return "Ожидает";
    }
    case "waiting_signal":
      return "Ожидает сигнала";
    case "waiting_clarification":
      return "Уточнение";
    case "waiting_approval":
      return "Ждёт подтверждения";
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
  const { state, actions, meta } = useWorkspace();
  const [goalDraft, setGoalDraft] = useState("");
  const [schedulePreset, setSchedulePreset] = useState<SchedulePreset>("now");
  const [dailyHour, setDailyHour] = useState(9);
  const [dailyMinute, setDailyMinute] = useState(0);
  const [signalNameDraft, setSignalNameDraft] = useState("ready");
  const [budgetPreset, setBudgetPreset] = useState<BudgetPreset>("standard");
  const [exceedPolicy, setExceedPolicy] = useState<BudgetExceedPolicy>("ask_approval");
  const [openPopover, setOpenPopover] = useState<TaskPopover>(null);
  const [filter, setFilter] = useState<TaskFilter>("all");
  const [editingTaskId, setEditingTaskId] = useState<string | null>(null);
  const [editingGoalValue, setEditingGoalValue] = useState("");
  const [signalTaskId, setSignalTaskId] = useState<string | null>(null);
  const [signalPayload, setSignalPayload] = useState("");
  const [replyDrafts, setReplyDrafts] = useState<Record<string, string>>({});
  const [isCreating, setIsCreating] = useState(false);
  const [nowMs, setNowMs] = useState(() => Date.now());

  const composerRef = useRef<HTMLDivElement | null>(null);
  const localTz = useMemo(() => detectLocalTimezone(), []);

  // Tick every second when any waiting_timer task exists so countdowns stay live.
  const hasWaitingTimer = useMemo(
    () => state.tasks.some((t) => t.status === "waiting_timer"),
    [state.tasks],
  );
  useEffect(() => {
    if (!hasWaitingTimer) {
      return;
    }
    const id = window.setInterval(() => setNowMs(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, [hasWaitingTimer]);

  // Capture-phase Escape dismisses schedule/budget popovers without side effects.
  useEffect(() => {
    if (!openPopover) {
      return;
    }
    const onMouseDown = (event: MouseEvent) => {
      if (composerRef.current && !composerRef.current.contains(event.target as Node)) {
        setOpenPopover(null);
      }
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        setOpenPopover(null);
      }
    };
    document.addEventListener("mousedown", onMouseDown);
    window.addEventListener("keydown", onKeyDown, { capture: true });
    return () => {
      document.removeEventListener("mousedown", onMouseDown);
      window.removeEventListener("keydown", onKeyDown, { capture: true });
    };
  }, [openPopover]);

  const projectById = useMemo(() => {
    const map = new Map<string, string>();
    for (const p of state.projects) {
      map.set(p.project_id, p.name);
    }
    return map;
  }, [state.projects]);

  const filteredTasks = useMemo(() => {
    return state.tasks.filter((task) => {
      if (filter === "active") {
        return !isTerminalTaskStatus(task.status);
      }
      if (filter === "done") {
        return isTerminalTaskStatus(task.status);
      }
      return true;
    });
  }, [filter, state.tasks]);

  const connected = Boolean(state.healthSnapshot?.connected);
  const targetProject = meta.activeProject;
  const targetAgent = meta.activeAgent;

  const scheduleTriggerLabel = (() => {
    switch (schedulePreset) {
      case "now":
        return "Сейчас";
      case "in_10s":
        return "Через 10 сек";
      case "in_5m":
        return "Через 5 мин";
      case "in_1h":
        return "Через 1 час";
      case "daily":
        return `Ежедневно ${String(dailyHour).padStart(2, "0")}:${String(dailyMinute).padStart(2, "0")}`;
      case "signal":
        return `Сигнал: ${signalNameDraft.trim() || "ready"}`;
    }
  })();

  const budgetTriggerLabel = (() => {
    switch (budgetPreset) {
      case "standard":
        return "Бюджет: стандартный";
      case "turns_3":
        return "До 3 ходов";
      case "turns_10":
        return "До 10 ходов";
      case "wall_60":
        return "До 60 сек";
    }
  })();

  const handleCreateTask = async (event: FormEvent) => {
    event.preventDefault();
    const trimmed = goalDraft.trim();
    if (!trimmed || !targetProject || !targetAgent || !connected || isCreating) {
      return;
    }

    const now = Date.now();
    let schedule: TaskSchedule | null = null;
    let waitForSignal: string | null = null;

    if (schedulePreset === "in_10s") {
      schedule = {
        timezone: localTz,
        original_expression: "через 10 секунд",
        kind: { kind: "once_at", fire_at: now + 10_000 },
        missed_run_policy: "execute_once",
      };
    } else if (schedulePreset === "in_5m") {
      schedule = {
        timezone: localTz,
        original_expression: "через 5 минут",
        kind: { kind: "once_at", fire_at: now + 5 * 60_000 },
        missed_run_policy: "execute_once",
      };
    } else if (schedulePreset === "in_1h") {
      schedule = {
        timezone: localTz,
        original_expression: "через 1 час",
        kind: { kind: "once_at", fire_at: now + 60 * 60_000 },
        missed_run_policy: "execute_once",
      };
    } else if (schedulePreset === "daily") {
      const hh = String(dailyHour).padStart(2, "0");
      const mm = String(dailyMinute).padStart(2, "0");
      schedule = {
        timezone: localTz,
        original_expression: `ежедневно в ${hh}:${mm} (${localTz})`,
        kind: { kind: "daily_wall_time", hour: dailyHour, minute: dailyMinute },
        missed_run_policy: "execute_once",
      };
    } else if (schedulePreset === "signal") {
      waitForSignal = signalNameDraft.trim() || "ready";
    }

    let budget: TaskBudget | null = null;
    if (budgetPreset === "turns_3") {
      budget = { max_turns: 3, exceed_policy: exceedPolicy };
    } else if (budgetPreset === "turns_10") {
      budget = { max_turns: 10, exceed_policy: exceedPolicy };
    } else if (budgetPreset === "wall_60") {
      budget = { max_wall_secs: 60, exceed_policy: exceedPolicy };
    }

    setIsCreating(true);
    setOpenPopover(null);
    try {
      const created = await actions.createTask({
        project_id: targetProject.project_id,
        agent_id: targetAgent.agent_id,
        goal: trimmed,
        schedule,
        wait_for_signal: waitForSignal,
        budget,
        execute_immediately: schedule === null && waitForSignal === null ? true : null,
      });
      if (created) {
        setGoalDraft("");
      }
    } finally {
      setIsCreating(false);
    }
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

  return (
    <main className="chat-card d2-surface-card" data-testid="tasks-card">
      <StatusBanners />

      <header className="d2-card-header">
        <div className="d2-card-header__title">
          <ListChecks size={18} aria-hidden="true" />
          <div>
            <h1>Поручения и расписания</h1>
            <p>
              Долговечные обязательства в PostgreSQL — переживают закрытие окна и перезапуск Head
            </p>
          </div>
        </div>

        <div className="d2-card-header__controls">
          <div className="d2-segmented" role="group" aria-label="Фильтр поручений">
            <button
              type="button"
              className={filter === "all" ? "is-active" : ""}
              onClick={() => setFilter("all")}
            >
              Все ({state.tasks.length})
            </button>
            <button
              type="button"
              className={filter === "active" ? "is-active" : ""}
              onClick={() => setFilter("active")}
            >
              Активные ({meta.activeTaskCount})
            </button>
            <button
              type="button"
              className={filter === "done" ? "is-active" : ""}
              onClick={() => setFilter("done")}
            >
              Завершённые
            </button>
          </div>
        </div>
      </header>

      <div className="d2-card-scroll">
        <div className="d2-card-inner">
          {/* Zero-bureaucracy quick composer matching WP-M01-003 composer style */}
          <div className="d2-task-composer-shell" ref={composerRef}>
            {openPopover === "schedule" && (
              <div
                className="floating-popover d2-task-popover d2-task-popover--left"
                role="menu"
                aria-label="Время запуска поручения"
                data-testid="task-schedule-popover"
              >
                <strong>Когда запустить</strong>
                <p>Часовой пояс IANA: {localTz}</p>

                {(
                  [
                    { id: "now", label: "Сейчас", desc: "Запустить в фоне немедленно" },
                    { id: "in_10s", label: "Через 10 сек", desc: "Отложенный таймер в PostgreSQL" },
                    { id: "in_5m", label: "Через 5 мин", desc: "Сработает даже после закрытия окна" },
                    { id: "in_1h", label: "Через 1 час", desc: "Однократный долговечный таймер" },
                    { id: "daily", label: "По расписанию (ежедневно)", desc: `С учётом DST (${localTz})` },
                    { id: "signal", label: "Ждать сигнал", desc: "Проснётся по идемпотентному сигналу" },
                  ] as Array<{ id: SchedulePreset; label: string; desc: string }>
                ).map((opt) => (
                  <button
                    key={opt.id}
                    type="button"
                    role="menuitemradio"
                    aria-checked={schedulePreset === opt.id}
                    className={schedulePreset === opt.id ? "is-selected" : ""}
                    data-testid={`task-schedule-opt-${opt.id}`}
                    onClick={() => {
                      setSchedulePreset(opt.id);
                      if (opt.id !== "daily" && opt.id !== "signal") {
                        setOpenPopover(null);
                      }
                    }}
                  >
                    <span className="popover-option-copy">
                      <strong>{opt.label}</strong>
                      <small>{opt.desc}</small>
                    </span>
                    {schedulePreset === opt.id && <Check size={14} aria-hidden="true" />}
                  </button>
                ))}

                {schedulePreset === "daily" && (
                  <div className="d2-schedule-extra" data-testid="task-daily-config">
                    <div className="d2-time-stepper">
                      <span>Время ({localTz})</span>
                      <div className="d2-time-stepper__controls">
                        <button
                          type="button"
                          className="d2-step-btn"
                          onClick={() => setDailyHour((h) => (h + 23) % 24)}
                          aria-label="Уменьшить час"
                        >
                          −
                        </button>
                        <code>{String(dailyHour).padStart(2, "0")}</code>
                        <button
                          type="button"
                          className="d2-step-btn"
                          onClick={() => setDailyHour((h) => (h + 1) % 24)}
                          aria-label="Увеличить час"
                        >
                          +
                        </button>
                        <span>:</span>
                        <button
                          type="button"
                          className="d2-step-btn"
                          onClick={() => setDailyMinute((m) => (m + 55) % 60)}
                          aria-label="Уменьшить минуты"
                        >
                          −
                        </button>
                        <code>{String(dailyMinute).padStart(2, "0")}</code>
                        <button
                          type="button"
                          className="d2-step-btn"
                          onClick={() => setDailyMinute((m) => (m + 5) % 60)}
                          aria-label="Увеличить минуты"
                        >
                          +
                        </button>
                      </div>
                    </div>
                  </div>
                )}

                {schedulePreset === "signal" && (
                  <div className="d2-schedule-extra" data-testid="task-signal-config">
                    <input
                      type="text"
                      value={signalNameDraft}
                      onChange={(e) => setSignalNameDraft(e.target.value)}
                      placeholder="Имя сигнала, например ci_passed"
                      aria-label="Имя ожидаемого сигнала"
                    />
                  </div>
                )}
              </div>
            )}

            {openPopover === "budget" && (
              <div
                className="floating-popover d2-task-popover d2-task-popover--budget"
                role="menu"
                aria-label="Лимит бюджета поручения"
                data-testid="task-budget-popover"
              >
                <strong>Внешний бюджет</strong>
                <p>Проверяется в Head вне контекста модели.</p>
                {(
                  [
                    { id: "standard", label: "Стандартный", desc: "Без жёсткого отсечения ходов" },
                    { id: "turns_3", label: "До 3 ходов", desc: "Короткое ограниченное поручение" },
                    { id: "turns_10", label: "До 10 ходов", desc: "Среднее автономное поручение" },
                    { id: "wall_60", label: "До 60 сек", desc: "Ограничение по времени работы" },
                  ] as Array<{ id: BudgetPreset; label: string; desc: string }>
                ).map((opt) => (
                  <button
                    key={opt.id}
                    type="button"
                    role="menuitemradio"
                    aria-checked={budgetPreset === opt.id}
                    className={budgetPreset === opt.id ? "is-selected" : ""}
                    data-testid={`task-budget-opt-${opt.id}`}
                    onClick={() => setBudgetPreset(opt.id)}
                  >
                    <span className="popover-option-copy">
                      <strong>{opt.label}</strong>
                      <small>{opt.desc}</small>
                    </span>
                    {budgetPreset === opt.id && <Check size={14} aria-hidden="true" />}
                  </button>
                ))}

                {budgetPreset !== "standard" && (
                  <div className="d2-schedule-extra">
                    <div className="d2-segmented d2-segmented--full">
                      <button
                        type="button"
                        className={exceedPolicy === "ask_approval" ? "is-active" : ""}
                        onClick={() => setExceedPolicy("ask_approval")}
                      >
                        Спросить меня
                      </button>
                      <button
                        type="button"
                        className={exceedPolicy === "fail" ? "is-active" : ""}
                        onClick={() => setExceedPolicy("fail")}
                      >
                        Остановить
                      </button>
                    </div>
                  </div>
                )}
              </div>
            )}

            <form
              className="composer d2-task-composer"
              onSubmit={(e) => void handleCreateTask(e)}
              data-testid="task-quick-composer"
            >
              <input
                type="text"
                className="d2-task-composer__input"
                value={goalDraft}
                onChange={(e) => setGoalDraft(e.target.value)}
                placeholder={
                  targetProject
                    ? `Поставить поручение в проекте «${targetProject.name}»…`
                    : "Сначала выберите или создайте проект…"
                }
                aria-label="Цель нового поручения"
                data-testid="task-goal-input"
                disabled={!connected || !targetProject || !targetAgent}
              />

              <div className="composer-toolbar">
                <div className="composer-tools">
                  <button
                    type="button"
                    className="composer-setting"
                    aria-expanded={openPopover === "schedule"}
                    aria-haspopup="menu"
                    data-testid="task-schedule-trigger"
                    onClick={() =>
                      setOpenPopover((prev) => (prev === "schedule" ? null : "schedule"))
                    }
                  >
                    <Clock size={14} aria-hidden="true" />
                    <span>{scheduleTriggerLabel}</span>
                    <CaretDown size={11} aria-hidden="true" />
                  </button>

                  <button
                    type="button"
                    className="composer-setting"
                    aria-expanded={openPopover === "budget"}
                    aria-haspopup="menu"
                    data-testid="task-budget-trigger"
                    onClick={() =>
                      setOpenPopover((prev) => (prev === "budget" ? null : "budget"))
                    }
                  >
                    <Coin size={14} aria-hidden="true" />
                    <span>{budgetTriggerLabel}</span>
                    <CaretDown size={11} aria-hidden="true" />
                  </button>
                </div>

                <div className="composer-send">
                  <button
                    type="submit"
                    className="popover-primary-btn"
                    disabled={
                      !connected ||
                      !targetProject ||
                      !targetAgent ||
                      !goalDraft.trim() ||
                      isCreating
                    }
                    data-testid="task-create-submit"
                  >
                    <PaperPlaneTilt size={13} aria-hidden="true" />
                    {isCreating ? "Создание…" : "Поставить поручение"}
                  </button>
                </div>
              </div>
            </form>
          </div>

          {filteredTasks.length === 0 ? (
            <div className="d2-empty-box" data-testid="tasks-empty-state">
              <ListChecks size={26} weight="light" aria-hidden="true" />
              <p>
                Поручений пока нет. Поставьте задачу выше — мгновенную, по таймеру или по ожиданию
                сигнала.
              </p>
            </div>
          ) : (
            <div className="d2-card-list" data-testid="tasks-list">
              {filteredTasks.map((task) => {
                const isEditing = editingTaskId === task.task_id;
                const isStopping = state.stoppingTaskIds.includes(task.task_id);
                const isTerminal = isTerminalTaskStatus(task.status);
                const canStop = !isTerminal;
                const goalMatchesTitle = task.title.trim() === task.goal.trim();
                const projectName = projectById.get(task.project_id) ?? "Проект";
                const replyDraft = replyDrafts[task.task_id] ?? "";

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
                          className={`d2-pill d2-pill--status-${task.status}`}
                          data-testid={`task-status-pill-${task.status}`}
                        >
                          {task.status === "unknown_external" && (
                            <Question size={12} weight="bold" aria-hidden="true" />
                          )}
                          {statusPillLabel(task, nowMs)}
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
                          <span className="d2-meta-chip">
                            <Clock size={11} aria-hidden="true" />
                            {task.schedule.original_expression ?? task.schedule.timezone}
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
                      <p className="d2-task-note">{task.wait_detail}</p>
                    )}

                    {task.partial_result_summary && (
                      <div className="d2-task-summary">
                        <strong>Итог / прогресс:</strong> {task.partial_result_summary}
                      </div>
                    )}

                    {task.status === "failed" && (task.error_summary || task.error_code) && (
                      <div className="d2-task-error">
                        <strong>{task.error_code ?? "ERROR"}:</strong>{" "}
                        {task.error_summary ?? "Поручение завершилось с ошибкой."}
                      </div>
                    )}

                    {task.status === "unknown_external" && (
                      <div className="d2-item-card__actions">
                        <button
                          type="button"
                          className="popover-secondary-btn"
                          onClick={() => actions.setActiveView("inbox")}
                        >
                          Перейти к сверке во Входящих
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
                              void actions
                                .sendTaskSignal(task.task_id, "ready", signalPayload.trim())
                                .then(() => {
                                  setSignalTaskId(null);
                                  setSignalPayload("");
                                });
                            }}
                          >
                            <input
                              type="text"
                              value={signalPayload}
                              onChange={(e) => setSignalPayload(e.target.value)}
                              placeholder="Текст сигнала (необязательно)…"
                              aria-label="Данные сигнала"
                            />
                            <button type="submit" className="popover-primary-btn">
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
                          <button
                            type="button"
                            className="popover-secondary-btn"
                            onClick={() => setSignalTaskId(task.task_id)}
                          >
                            <Broadcast size={13} aria-hidden="true" />
                            Подать сигнал…
                          </button>
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
