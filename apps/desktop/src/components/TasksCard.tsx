import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import {
  ArrowsClockwise,
  ArrowSquareOut,
  Broadcast,
  CalendarBlank,
  ChatsTeardrop,
  Clock,
  ClockCounterClockwise,
  GitBranch,
  ListChecks,
  Pause,
  PencilSimple,
  Play,
  Plus,
  Question,
  ShieldCheck,
  Stop,
  Trash,
} from "@phosphor-icons/react";

import type {
  AgentMessageRecord,
  EffectClaimRecord,
  ScheduleKind,
  TaskDetailResponse,
  TaskRecord,
  TaskStatus,
} from "../api/contracts";
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
type ScheduleMode = ScheduleKind["kind"];

export const CREATE_EVENT_DRAFT_PREFIX = "Создай событие: мне необходимо ";

const WEEKDAY_CHIPS = [
  { bit: 1, label: "Пн" },
  { bit: 2, label: "Вт" },
  { bit: 4, label: "Ср" },
  { bit: 8, label: "Чт" },
  { bit: 16, label: "Пт" },
  { bit: 32, label: "Сб" },
  { bit: 64, label: "Вс" },
] as const;

function isTaskPaused(task: TaskRecord, detail?: TaskDetailResponse): boolean {
  if (detail?.timers.some((t) => t.status === "paused")) {
    return true;
  }
  const wait = (task.wait_detail ?? "").toLowerCase();
  return wait.includes("приостановлен") || wait.includes("paused");
}

function statusPillLabel(
  task: TaskRecord,
  nowMs: number,
  hasPendingApproval: boolean,
  paused: boolean,
): string {
  if (hasPendingApproval && task.status === "running") {
    return "Ожидает подтверждения";
  }
  if (paused && (task.status === "waiting_timer" || task.status === "queued")) {
    return "На паузе";
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

function formatDateTimeShort(ts: number): string {
  return new Date(ts).toLocaleString("ru-RU", {
    day: "2-digit",
    month: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function TasksCard() {
  const { state, actions, meta } = useWorkspace();
  const [filter, setFilter] = useState<TaskFilter>("all");
  const [projectFilter, setProjectFilter] = useState<string | "all">("all");
  const [editingTaskId, setEditingTaskId] = useState<string | null>(null);
  const [editingGoalValue, setEditingGoalValue] = useState("");
  const [signalTaskId, setSignalTaskId] = useState<string | null>(null);
  const [signalNameInput, setSignalNameInput] = useState("");
  const [signalPayload, setSignalPayload] = useState("");
  const [replyDrafts, setReplyDrafts] = useState<Record<string, string>>({});
  const [reconcilingTaskIds, setReconcilingTaskIds] = useState<Record<string, boolean>>({});
  const [busyActionTaskIds, setBusyActionTaskIds] = useState<Record<string, boolean>>({});
  const [confirmDeleteTaskId, setConfirmDeleteTaskId] = useState<string | null>(null);

  // Schedule editor state (zero <select>!)
  const [editingScheduleTaskId, setEditingScheduleTaskId] = useState<string | null>(null);
  const [schedMode, setSchedMode] = useState<ScheduleMode>("daily_wall_time");
  const [schedIntervalMins, setSchedIntervalMins] = useState("60");
  const [schedHour, setSchedHour] = useState("9");
  const [schedMinute, setSchedMinute] = useState("0");
  const [schedWeekdaysMask, setSchedWeekdaysMask] = useState(31); // Mon-Fri
  const [schedMonth, setSchedMonth] = useState("1");
  const [schedDay, setSchedDay] = useState("1");
  const [schedOnceAtLocal, setSchedOnceAtLocal] = useState("");

  // Expandable runs history & agent messages
  const [expandedRunsTaskId, setExpandedRunsTaskId] = useState<string | null>(null);
  const [taskDetailsById, setTaskDetailsById] = useState<Record<string, TaskDetailResponse>>({});
  const [loadingRunsTaskId, setLoadingRunsTaskId] = useState<string | null>(null);

  const [expandedMsgsTaskId, setExpandedMsgsTaskId] = useState<string | null>(null);
  const [agentMessagesByTaskId, setAgentMessagesByTaskId] = useState<
    Record<string, AgentMessageRecord[]>
  >({});
  const [loadingMsgsTaskId, setLoadingMsgsTaskId] = useState<string | null>(null);

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

  const taskById = useMemo(() => {
    const map = new Map<string, TaskRecord>();
    for (const t of activeProjectTasks) {
      map.set(t.task_id, t);
    }
    return map;
  }, [activeProjectTasks]);

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

  // Build hierarchical display list so subagent child tasks nest visibly under their parent_task_id
  const hierarchicalTasks = useMemo(() => {
    const visibleIds = new Set(filteredTasks.map((t) => t.task_id));
    const childrenByParent = new Map<string, TaskRecord[]>();
    const roots: TaskRecord[] = [];

    for (const task of filteredTasks) {
      if (task.parent_task_id && visibleIds.has(task.parent_task_id)) {
        const list = childrenByParent.get(task.parent_task_id) ?? [];
        list.push(task);
        childrenByParent.set(task.parent_task_id, list);
      } else {
        roots.push(task);
      }
    }

    const ordered: Array<{ task: TaskRecord; depth: number; parentTitle: string | null }> = [];
    const visited = new Set<string>();

    const walk = (task: TaskRecord, depth: number) => {
      if (visited.has(task.task_id)) {
        return;
      }
      visited.add(task.task_id);
      const parentTitle = task.parent_task_id
        ? (taskById.get(task.parent_task_id)?.title ?? null)
        : null;
      ordered.push({ task, depth: Math.min(depth, 4), parentTitle });
      const kids = childrenByParent.get(task.task_id) ?? [];
      for (const kid of kids) {
        walk(kid, depth + 1);
      }
    };

    for (const root of roots) {
      walk(root, 0);
    }
    for (const task of filteredTasks) {
      if (!visited.has(task.task_id)) {
        walk(task, 0);
      }
    }

    return ordered;
  }, [filteredTasks, taskById]);

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
  }, [hierarchicalTasks.length]);

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

  const runBusyTaskAction = async (taskId: string, fn: () => Promise<unknown>) => {
    setBusyActionTaskIds((prev) => ({ ...prev, [taskId]: true }));
    try {
      await fn();
    } finally {
      setBusyActionTaskIds((prev) => {
        const next = { ...prev };
        delete next[taskId];
        return next;
      });
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

  const openScheduleEditor = (task: TaskRecord) => {
    setEditingScheduleTaskId(task.task_id);
    const kind = task.schedule?.kind;
    if (!kind) {
      setSchedMode("daily_wall_time");
      setSchedHour("9");
      setSchedMinute("0");
      setSchedIntervalMins("60");
      setSchedWeekdaysMask(31);
      setSchedMonth("1");
      setSchedDay("1");
      const soon = new Date(Date.now() + 3600_000);
      setSchedOnceAtLocal(soon.toISOString().slice(0, 16));
      return;
    }
    setSchedMode(kind.kind);
    if (kind.kind === "interval") {
      setSchedIntervalMins(String(Math.max(1, Math.round(kind.interval_secs / 60))));
    } else if (kind.kind === "daily_wall_time") {
      setSchedHour(String(kind.hour));
      setSchedMinute(String(kind.minute));
    } else if (kind.kind === "weekly_wall_time") {
      setSchedWeekdaysMask(kind.weekdays_mask || 31);
      setSchedHour(String(kind.hour));
      setSchedMinute(String(kind.minute));
    } else if (kind.kind === "yearly_wall_time") {
      setSchedMonth(String(kind.month));
      setSchedDay(String(kind.day));
      setSchedHour(String(kind.hour));
      setSchedMinute(String(kind.minute));
    } else if (kind.kind === "once_at") {
      setSchedOnceAtLocal(new Date(kind.fire_at).toISOString().slice(0, 16));
    }
  };

  const handleSaveSchedule = async (task: TaskRecord) => {
    const tz = task.schedule?.timezone || state.userPreferences.timezone || "UTC";
    const hour = Math.max(0, Math.min(23, Number.parseInt(schedHour, 10) || 0));
    const minute = Math.max(0, Math.min(59, Number.parseInt(schedMinute, 10) || 0));

    let kind: ScheduleKind;
    switch (schedMode) {
      case "interval": {
        const mins = Math.max(1, Number.parseInt(schedIntervalMins, 10) || 60);
        kind = { kind: "interval", interval_secs: mins * 60 };
        break;
      }
      case "daily_wall_time":
        kind = { kind: "daily_wall_time", hour, minute };
        break;
      case "weekly_wall_time":
        kind = {
          kind: "weekly_wall_time",
          weekdays_mask: schedWeekdaysMask || 1,
          hour,
          minute,
        };
        break;
      case "yearly_wall_time": {
        const month = Math.max(1, Math.min(12, Number.parseInt(schedMonth, 10) || 1));
        const day = Math.max(1, Math.min(31, Number.parseInt(schedDay, 10) || 1));
        kind = { kind: "yearly_wall_time", month, day, hour, minute };
        break;
      }
      case "once_at": {
        const parsed = Date.parse(schedOnceAtLocal);
        kind = {
          kind: "once_at",
          fire_at: Number.isFinite(parsed) ? parsed : Date.now() + 3600_000,
        };
        break;
      }
    }

    await runBusyTaskAction(task.task_id, async () => {
      await actions.updateTask(task.task_id, {
        schedule: {
          timezone: tz,
          kind,
          missed_run_policy: task.schedule?.missed_run_policy ?? "execute_once",
        },
      });
      setEditingScheduleTaskId(null);
    });
  };

  const handleToggleRunsHistory = async (taskId: string) => {
    if (expandedRunsTaskId === taskId) {
      setExpandedRunsTaskId(null);
      return;
    }
    setExpandedRunsTaskId(taskId);
    setLoadingRunsTaskId(taskId);
    try {
      const detail = await meta.bridge.getTaskDetail(taskId);
      setTaskDetailsById((prev) => ({ ...prev, [taskId]: detail }));
    } catch {
      // Non-fatal
    } finally {
      setLoadingRunsTaskId((curr) => (curr === taskId ? null : curr));
    }
  };

  const handleToggleAgentMessages = async (task: TaskRecord) => {
    if (expandedMsgsTaskId === task.task_id) {
      setExpandedMsgsTaskId(null);
      return;
    }
    setExpandedMsgsTaskId(task.task_id);
    setLoadingMsgsTaskId(task.task_id);
    try {
      const resp = await meta.bridge.listAgentMessages(task.agent_id, false);
      setAgentMessagesByTaskId((prev) => ({ ...prev, [task.task_id]: resp.messages }));
    } catch {
      setAgentMessagesByTaskId((prev) => ({ ...prev, [task.task_id]: [] }));
    } finally {
      setLoadingMsgsTaskId((curr) => (curr === task.task_id ? null : curr));
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
              Долговечные события, подагенты и расписания в PostgreSQL — управляйте напрямую или
              через чат
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
          {hierarchicalTasks.length === 0 ? (
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
              {hierarchicalTasks.map(({ task, depth, parentTitle }) => {
                const isEditing = editingTaskId === task.task_id;
                const isStopping = state.stoppingTaskIds.includes(task.task_id);
                const isBusy = Boolean(busyActionTaskIds[task.task_id]);
                const isReconciling = Boolean(reconcilingTaskIds[task.task_id]);
                const isTerminal = isTerminalTaskStatus(task.status);
                const canStop = !isTerminal;
                const goalMatchesTitle = task.title.trim() === task.goal.trim();
                const projectName = projectById.get(task.project_id) ?? "Проект";
                const replyDraft = replyDrafts[task.task_id] ?? "";
                const linkedUnknownEffect = unknownEffectByTaskId.get(task.task_id);
                const hasPendingApproval = pendingApprovalSessions.has(task.session_id);
                const detail = taskDetailsById[task.task_id];
                const paused = isTaskPaused(task, detail);
                const effectivePillStatus =
                  hasPendingApproval && task.status === "running"
                    ? "waiting_approval"
                    : task.status;
                const isRunsExpanded = expandedRunsTaskId === task.task_id;
                const isMsgsExpanded = expandedMsgsTaskId === task.task_id;
                const isEditingSchedule = editingScheduleTaskId === task.task_id;
                const agentMsgs = agentMessagesByTaskId[task.task_id] ?? [];

                return (
                  <article
                    key={task.task_id}
                    className={`d2-item-card ${
                      task.status === "unknown_external" ? "d2-item-card--unknown" : ""
                    } ${depth > 0 ? "task-subagent-child" : ""}`}
                    style={depth > 0 ? { marginLeft: `${depth * 22}px` } : undefined}
                    data-testid={`task-card-${task.task_id}`}
                    data-subagent-depth={depth}
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
                          {statusPillLabel(task, nowMs, hasPendingApproval, paused)}
                        </span>
                        <span className="d2-meta-chip">{projectName}</span>
                        {task.parent_task_id && (
                          <span
                            className="d2-meta-chip task-subagent-badge"
                            data-testid={`task-subagent-badge-${task.task_id}`}
                          >
                            <GitBranch size={11} aria-hidden="true" />
                            Подагент{parentTitle ? ` · ${parentTitle}` : ""}
                          </span>
                        )}
                        {task.generation > 1 && (
                          <span
                            className="d2-meta-chip d2-meta-chip--gen"
                            data-testid={`task-generation-${task.task_id}`}
                          >
                            цель v{task.generation}
                          </span>
                        )}
                        {task.schedule && (
                          <span
                            className="d2-meta-chip"
                            data-testid={`task-schedule-chip-${task.task_id}`}
                          >
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
                        {task.status !== "running" && (
                          <button
                            type="button"
                            className="d2-link-btn"
                            disabled={!connected || isBusy}
                            data-testid={`task-run-now-btn-${task.task_id}`}
                            title="Запустить поручение сейчас"
                            onClick={() =>
                              void runBusyTaskAction(task.task_id, () =>
                                actions.runTaskNow(task.task_id),
                              )
                            }
                          >
                            <Play size={12} weight="fill" aria-hidden="true" />
                            Запустить
                          </button>
                        )}
                        {!isTerminal &&
                          (paused ? (
                            <button
                              type="button"
                              className="d2-link-btn"
                              disabled={!connected || isBusy}
                              data-testid={`task-resume-btn-${task.task_id}`}
                              onClick={() =>
                                void runBusyTaskAction(task.task_id, () =>
                                  actions.resumeTask(task.task_id),
                                )
                              }
                            >
                              <Play size={12} aria-hidden="true" />
                              Возобновить
                            </button>
                          ) : (
                            <button
                              type="button"
                              className="d2-link-btn"
                              disabled={!connected || isBusy}
                              data-testid={`task-pause-btn-${task.task_id}`}
                              onClick={() =>
                                void runBusyTaskAction(task.task_id, () =>
                                  actions.pauseTask(task.task_id),
                                )
                              }
                            >
                              <Pause size={12} aria-hidden="true" />
                              Пауза
                            </button>
                          ))}
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
                        {!isTerminal && (
                          <button
                            type="button"
                            className="icon-button icon-button--xs"
                            data-testid={`task-schedule-edit-btn-${task.task_id}`}
                            aria-label={`Настроить расписание ${task.title}`}
                            title="Редактировать расписание"
                            onClick={() =>
                              isEditingSchedule
                                ? setEditingScheduleTaskId(null)
                                : openScheduleEditor(task)
                            }
                          >
                            <CalendarBlank size={13} />
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
                            {isStopping ? "Остановка…" : "Стоп"}
                          </button>
                        )}
                        <button
                          type="button"
                          className="icon-button icon-button--xs"
                          disabled={!connected || isBusy}
                          aria-label={`Удалить поручение ${task.title}`}
                          title="Удалить поручение"
                          data-testid={`task-delete-btn-${task.task_id}`}
                          onClick={() =>
                            setConfirmDeleteTaskId((prev) =>
                              prev === task.task_id ? null : task.task_id,
                            )
                          }
                        >
                          <Trash size={13} />
                        </button>
                      </div>
                    </div>

                    {confirmDeleteTaskId === task.task_id && (
                      <div
                        className="artifact-confirm-bar"
                        role="alertdialog"
                        aria-label="Подтверждение удаления поручения"
                        data-testid={`task-delete-confirm-${task.task_id}`}
                      >
                        <p>Удалить поручение «{task.title}» и его таймеры?</p>
                        <div className="artifact-confirm-bar__actions">
                          <button
                            type="button"
                            className="popover-secondary-btn"
                            onClick={() => setConfirmDeleteTaskId(null)}
                          >
                            Отмена
                          </button>
                          <button
                            type="button"
                            className="d2-danger-btn d2-danger-btn--sm"
                            data-testid={`task-delete-confirm-yes-${task.task_id}`}
                            onClick={() => {
                              setConfirmDeleteTaskId(null);
                              void runBusyTaskAction(task.task_id, () =>
                                actions.deleteTask(task.task_id),
                              );
                            }}
                          >
                            Удалить
                          </button>
                        </div>
                      </div>
                    )}

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

                    {/* Inline Schedule Editor (zero <select>!) */}
                    {isEditingSchedule && (
                      <div
                        className="task-schedule-editor"
                        data-testid={`task-schedule-editor-${task.task_id}`}
                      >
                        <div className="d2-segmented" role="group" aria-label="Тип расписания">
                          {(
                            [
                              { id: "interval", label: "Интервал" },
                              { id: "daily_wall_time", label: "Ежедневно" },
                              { id: "weekly_wall_time", label: "По дням" },
                              { id: "yearly_wall_time", label: "Ежегодно" },
                              { id: "once_at", label: "Один раз" },
                            ] as const
                          ).map((item) => (
                            <button
                              key={item.id}
                              type="button"
                              className={schedMode === item.id ? "is-active" : ""}
                              data-testid={`task-sched-mode-${item.id}`}
                              onClick={() => setSchedMode(item.id)}
                            >
                              {item.label}
                            </button>
                          ))}
                        </div>

                        {schedMode === "interval" && (
                          <label className="settings-field">
                            <span>Повторять каждые (минут)</span>
                            <input
                              type="number"
                              min={1}
                              value={schedIntervalMins}
                              onChange={(e) => setSchedIntervalMins(e.target.value)}
                              data-testid="task-sched-interval-input"
                            />
                          </label>
                        )}

                        {schedMode === "weekly_wall_time" && (
                          <div className="task-weekday-chips" role="group" aria-label="Дни недели">
                            {WEEKDAY_CHIPS.map((d) => {
                              const active = (schedWeekdaysMask & d.bit) !== 0;
                              return (
                                <button
                                  key={d.bit}
                                  type="button"
                                  className={`d2-link-btn ${active ? "is-selected" : ""}`}
                                  aria-pressed={active}
                                  onClick={() => setSchedWeekdaysMask((m) => m ^ d.bit)}
                                >
                                  {d.label}
                                </button>
                              );
                            })}
                          </div>
                        )}

                        {schedMode === "yearly_wall_time" && (
                          <div className="settings-mcp-secret-row">
                            <label className="settings-field">
                              <span>Месяц (1–12)</span>
                              <input
                                type="number"
                                min={1}
                                max={12}
                                value={schedMonth}
                                onChange={(e) => setSchedMonth(e.target.value)}
                              />
                            </label>
                            <label className="settings-field">
                              <span>День (1–31)</span>
                              <input
                                type="number"
                                min={1}
                                max={31}
                                value={schedDay}
                                onChange={(e) => setSchedDay(e.target.value)}
                              />
                            </label>
                          </div>
                        )}

                        {(schedMode === "daily_wall_time" ||
                          schedMode === "weekly_wall_time" ||
                          schedMode === "yearly_wall_time") && (
                          <div className="settings-mcp-secret-row">
                            <label className="settings-field">
                              <span>Час (0–23)</span>
                              <input
                                type="number"
                                min={0}
                                max={23}
                                value={schedHour}
                                onChange={(e) => setSchedHour(e.target.value)}
                                data-testid="task-sched-hour-input"
                              />
                            </label>
                            <label className="settings-field">
                              <span>Минута (0–59)</span>
                              <input
                                type="number"
                                min={0}
                                max={59}
                                value={schedMinute}
                                onChange={(e) => setSchedMinute(e.target.value)}
                                data-testid="task-sched-minute-input"
                              />
                            </label>
                          </div>
                        )}

                        {schedMode === "once_at" && (
                          <label className="settings-field">
                            <span>Дата и время запуска</span>
                            <input
                              type="datetime-local"
                              value={schedOnceAtLocal}
                              onChange={(e) => setSchedOnceAtLocal(e.target.value)}
                            />
                          </label>
                        )}

                        <div className="d2-item-card__actions">
                          <button
                            type="button"
                            className="popover-primary-btn"
                            disabled={isBusy}
                            data-testid={`task-schedule-save-${task.task_id}`}
                            onClick={() => void handleSaveSchedule(task)}
                          >
                            Сохранить расписание
                          </button>
                          {task.schedule && (
                            <button
                              type="button"
                              className="d2-danger-btn d2-danger-btn--sm"
                              disabled={isBusy}
                              data-testid={`task-schedule-clear-${task.task_id}`}
                              onClick={() =>
                                void runBusyTaskAction(task.task_id, async () => {
                                  await actions.updateTask(task.task_id, { clear_schedule: true });
                                  setEditingScheduleTaskId(null);
                                })
                              }
                            >
                              Убрать расписание
                            </button>
                          )}
                          <button
                            type="button"
                            className="popover-secondary-btn"
                            onClick={() => setEditingScheduleTaskId(null)}
                          >
                            Отмена
                          </button>
                        </div>
                      </div>
                    )}

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

                    {/* Expandable Runs History */}
                    {isRunsExpanded && (
                      <div
                        className="memory-history-panel"
                        data-testid={`task-runs-panel-${task.task_id}`}
                      >
                        <strong>История запусков и таймеров</strong>
                        {loadingRunsTaskId === task.task_id ? (
                          <span className="d2-time">Загрузка…</span>
                        ) : detail ? (
                          <>
                            {detail.runs.length === 0 ? (
                              <span className="d2-time">Запусков пока не было.</span>
                            ) : (
                              detail.runs.map((run) => (
                                <div key={run.run_id} className="memory-history-row">
                                  <span className="d2-meta-chip">{run.status}</span>
                                  <div className="memory-history-row__copy">
                                    <strong>Запуск {run.run_id.slice(0, 8)}</strong>
                                    <small>
                                      {run.tokens_used ? `${run.tokens_used} ток.` : ""}
                                      {run.wall_ms ? ` · ${Math.round(run.wall_ms / 1000)} с` : ""}
                                      {run.error_summary ? ` · ${run.error_summary}` : ""}
                                    </small>
                                  </div>
                                  <span className="d2-time">
                                    {formatDateTimeShort(run.created_at)}
                                  </span>
                                </div>
                              ))
                            )}
                            {detail.timers.length > 0 && (
                              <div className="d2-time">
                                Таймеры:{" "}
                                {detail.timers
                                  .map(
                                    (t) => `${t.status} (${formatDateTimeShort(t.fire_at)})`,
                                  )
                                  .join(", ")}
                              </div>
                            )}
                          </>
                        ) : (
                          <span className="d2-time">Не удалось загрузить историю.</span>
                        )}
                      </div>
                    )}

                    {/* Expandable Agent Messages Viewer */}
                    {isMsgsExpanded && (
                      <div
                        className="memory-history-panel"
                        data-testid={`task-agent-messages-panel-${task.task_id}`}
                      >
                        <strong>Сообщения агента ({agentMsgs.length})</strong>
                        {loadingMsgsTaskId === task.task_id ? (
                          <span className="d2-time">Загрузка…</span>
                        ) : agentMsgs.length === 0 ? (
                          <span className="d2-time">Межагентных сообщений нет.</span>
                        ) : (
                          agentMsgs.map((msg) => (
                            <div key={msg.agent_message_id} className="memory-history-row">
                              <span className="d2-meta-chip">
                                {msg.delivered_at ? "доставлено" : "в очереди"}
                              </span>
                              <div className="memory-history-row__copy">
                                <strong>{msg.sender_agent_id ?? "система"}</strong>
                                <p>{msg.body_text}</p>
                              </div>
                              <span className="d2-time">{formatDateTimeShort(msg.created_at)}</span>
                            </div>
                          ))
                        )}
                      </div>
                    )}

                    <div className="d2-task-footer">
                      <span>Ходов: {task.usage.turns_used}</span>
                      {task.budget.max_turns && <span>Лимит ходов: {task.budget.max_turns}</span>}
                      {task.usage.tokens_used > 0 && (
                        <span>Токенов: {task.usage.tokens_used}</span>
                      )}
                      <button
                        type="button"
                        className="d2-link-btn"
                        aria-expanded={isRunsExpanded}
                        data-testid={`task-runs-toggle-${task.task_id}`}
                        onClick={() => void handleToggleRunsHistory(task.task_id)}
                      >
                        <ClockCounterClockwise size={12} aria-hidden="true" />
                        История запусков
                      </button>
                      <button
                        type="button"
                        className="d2-link-btn"
                        aria-expanded={isMsgsExpanded}
                        data-testid={`task-agent-messages-btn-${task.task_id}`}
                        onClick={() => void handleToggleAgentMessages(task)}
                      >
                        <ChatsTeardrop size={12} aria-hidden="true" />
                        Сообщения
                      </button>
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
