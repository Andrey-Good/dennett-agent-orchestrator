import { useLayoutEffect, useMemo, useRef, useState } from "react";
import {
  ArrowsClockwise,
  ArrowSquareOut,
  ChatCircleText,
  Check,
  Clock,
  Coin,
  Key,
  Question,
  ShieldCheck,
  ShieldWarning,
  Stop,
  Tray,
  WarningCircle,
  X,
} from "@phosphor-icons/react";

import type {
  CapabilityGrantRecord,
  EffectClaimRecord,
  InboxCardRecord,
  PermissionRequestRecord,
  TaskRecord,
} from "../api/contracts";
import { useWorkspace } from "../state/WorkspaceProvider";
import { cleanPlainText, formatDecisionReasonRu } from "../utils/formatters";
import { ProjectScopeFilter } from "./ProjectScopeFilter";
import { StatusBanners } from "./StatusBanners";

function formatTime(timestampMs: number): string {
  return new Date(timestampMs).toLocaleTimeString("ru-RU", {
    hour: "2-digit",
    minute: "2-digit",
  });
}

function formatInboxDetail(card: InboxCardRecord): string | null {
  if (card.kind === "unknown_effect") {
    // Dedicated warning box below already renders the explanation and metadata without duplication.
    return null;
  }
  const formatted = formatDecisionReasonRu(card.detail);
  return formatted || null;
}

function taskStatusLabel(status: TaskRecord["status"], hasPendingApproval: boolean): string {
  if (hasPendingApproval && status === "running") {
    return "Ожидает подтверждения";
  }
  switch (status) {
    case "queued":
      return "В очереди";
    case "running":
      return "Выполняется";
    case "waiting_timer":
      return "Ожидает";
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

function isSnippetLong(snippet: string): boolean {
  return snippet.length > 320 || snippet.split(/\r?\n/).length > 6;
}

function truncateSnippet(snippet: string): string {
  const lines = snippet.split(/\r?\n/);
  const sliced = lines.slice(0, 6).join("\n");
  return sliced.length > 320 ? `${sliced.slice(0, 320)}…` : sliced;
}

export function InboxRadarCard() {
  const { state, actions } = useWorkspace();
  const [projectFilter, setProjectFilter] = useState<string | "all">("all");
  const [replyDrafts, setReplyDrafts] = useState<Record<string, string>>({});
  const [expandedSnippets, setExpandedSnippets] = useState<Record<string, boolean>>({});
  const [busyIds, setBusyIds] = useState<Record<string, boolean>>({});

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

  const agentById = useMemo(() => {
    const map = new Map<string, string>();
    for (const a of state.agents) {
      map.set(a.agent_id, a.name);
    }
    return map;
  }, [state.agents]);

  const permissionById = useMemo(() => {
    const map = new Map<string, PermissionRequestRecord>();
    for (const req of state.pendingPermissions) {
      map.set(req.request_id, req);
    }
    return map;
  }, [state.pendingPermissions]);

  const pendingApprovalSessions = useMemo(
    () =>
      new Set(
        state.pendingPermissions
          .filter((req) => req.status === "pending")
          .map((req) => req.session_id),
      ),
    [state.pendingPermissions],
  );

  const effectById = useMemo(() => {
    const map = new Map<string, EffectClaimRecord>();
    for (const ef of state.effects) {
      map.set(ef.effect_id, ef);
    }
    return map;
  }, [state.effects]);

  const allOpenCards = useMemo(
    () =>
      state.inboxCards.filter(
        (card) => card.status === "open" && activeProjectIds.has(card.project_id),
      ),
    [activeProjectIds, state.inboxCards],
  );

  const countByProjectId = useMemo(() => {
    const map = new Map<string, number>();
    for (const card of allOpenCards) {
      map.set(card.project_id, (map.get(card.project_id) ?? 0) + 1);
    }
    return map;
  }, [allOpenCards]);

  const query = state.searchQuery.trim().toLowerCase();

  const matchesScopeAndQuery = (projectId: string | null, haystack: string): boolean => {
    if (projectFilter !== "all" && projectId !== projectFilter) {
      return false;
    }
    if (!query) {
      return true;
    }
    const projName = (projectId ? (projectById.get(projectId) ?? "") : "Чаты").toLowerCase();
    return haystack.toLowerCase().includes(query) || projName.includes(query);
  };

  const openCards = useMemo(
    () =>
      allOpenCards.filter((card) =>
        matchesScopeAndQuery(card.project_id, `${card.subject} ${card.detail}`),
      ),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [allOpenCards, projectById, projectFilter, query],
  );

  const failedTasks = useMemo(
    () =>
      state.tasks
        .filter((t) => t.status === "failed" && activeProjectIds.has(t.project_id))
        .filter((t) =>
          matchesScopeAndQuery(t.project_id, `${t.title} ${t.error_summary ?? ""}`),
        ),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [activeProjectIds, projectById, projectFilter, query, state.tasks],
  );

  const radarTasks = useMemo(
    () =>
      state.tasks
        .filter(
          (t) =>
            activeProjectIds.has(t.project_id) &&
            [
              "queued",
              "running",
              "waiting_timer",
              "waiting_signal",
              "waiting_clarification",
              "waiting_approval",
              "unknown_external",
            ].includes(t.status),
        )
        .filter((t) => matchesScopeAndQuery(t.project_id, `${t.title} ${t.wait_detail ?? ""}`)),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [activeProjectIds, projectById, projectFilter, query, state.tasks],
  );

  const radarRunningSessions = useMemo(() => {
    const taskRunningSessionIds = new Set(
      radarTasks.filter((t) => t.status === "running").map((t) => t.session_id),
    );
    const runningSet = new Set(state.runningSessionIds);
    return state.sessions
      .filter(
        (s) =>
          runningSet.has(s.session_id) &&
          (s.project_id === null || activeProjectIds.has(s.project_id)),
      )
      .filter((s) => !taskRunningSessionIds.has(s.session_id))
      .filter((s) => matchesScopeAndQuery(s.project_id, s.title));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    activeProjectIds,
    projectById,
    projectFilter,
    query,
    radarTasks,
    state.runningSessionIds,
    state.sessions,
  ]);

  const visibleGrants = useMemo(
    () =>
      state.activeGrants
        .filter((g) => !g.revoked_at && activeProjectIds.has(g.project_id))
        .filter((g) => matchesScopeAndQuery(g.project_id, `${g.tool_pattern} ${g.reason}`)),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [activeProjectIds, projectById, projectFilter, query, state.activeGrants],
  );

  // Preserve scroll position on live snapshot refreshes.
  useLayoutEffect(() => {
    if (scrollRef.current && savedScrollTopRef.current > 0) {
      scrollRef.current.scrollTop = savedScrollTopRef.current;
    }
  }, [openCards.length, radarTasks.length, radarRunningSessions.length]);

  const runBusy = async (id: string, fn: () => Promise<void>) => {
    setBusyIds((prev) => ({ ...prev, [id]: true }));
    try {
      await fn();
    } finally {
      setBusyIds((prev) => {
        const next = { ...prev };
        delete next[id];
        return next;
      });
    }
  };

  const renderCard = (card: InboxCardRecord) => {
    const isBusy = Boolean(busyIds[card.card_id]);
    const projectName = projectById.get(card.project_id) ?? "Проект";
    const agentName = card.agent_id ? (agentById.get(card.agent_id) ?? "Агент") : "Агент";
    const linkedPerm = card.permission_request_id
      ? permissionById.get(card.permission_request_id)
      : undefined;
    const linkedEffect = card.effect_id ? effectById.get(card.effect_id) : undefined;
    const draft = replyDrafts[card.card_id] ?? "";
    const isUnknown = card.kind === "unknown_effect";

    const snippet = linkedPerm?.preview_snippet?.trim() ?? "";
    const snippetExpandable = snippet.length > 0 && isSnippetLong(snippet);
    const snippetExpanded = Boolean(expandedSnippets[card.card_id]);

    return (
      <article
        key={card.card_id}
        className={`d2-item-card ${isUnknown ? "d2-item-card--unknown" : ""} ${
          card.kind === "permission_approval" ? "d2-item-card--approval" : ""
        }`}
        data-testid={`inbox-card-${card.card_id}`}
      >
        <div className="d2-item-card__top">
          <div className="d2-item-card__badges">
            {card.kind === "permission_approval" && (
              <span className="d2-pill d2-pill--approval">
                <ShieldCheck size={12} aria-hidden="true" />
                Подтверждение записи
              </span>
            )}
            {card.kind === "clarification" && (
              <span className="d2-pill d2-pill--clarify">
                <ChatCircleText size={12} aria-hidden="true" />
                Уточнение
              </span>
            )}
            {card.kind === "unknown_effect" && (
              <span
                className="d2-pill d2-pill--unknown"
                data-testid={`inbox-unknown-badge-${card.card_id}`}
              >
                <Question size={12} weight="bold" aria-hidden="true" />
                Неизвестно, выполнено ли (UNKNOWN)
              </span>
            )}
            {card.kind === "budget_exceeded" && (
              <span className="d2-pill d2-pill--warn">
                <Coin size={12} aria-hidden="true" />
                Лимит бюджета
              </span>
            )}
            {card.kind === "missed_schedule" && (
              <span className="d2-pill d2-pill--waiting">
                <Clock size={12} aria-hidden="true" />
                Пропущенный запуск
              </span>
            )}
            <span className="d2-meta-chip">{projectName}</span>
            <span className="d2-meta-chip">{agentName}</span>
          </div>

          <div className="d2-item-card__meta-right">
            <span className="d2-time">{formatTime(card.created_at)}</span>
            {card.session_id && (
              <button
                type="button"
                className="d2-link-btn"
                data-testid={`inbox-open-chat-${card.card_id}`}
                onClick={() => void actions.openSessionInChat(card.session_id!)}
              >
                <ArrowSquareOut size={13} aria-hidden="true" />
                Открыть чат
              </button>
            )}
          </div>
        </div>

        <h3 className="d2-item-card__title">{cleanPlainText(card.subject)}</h3>
        {(() => {
          const detailText = formatInboxDetail(card);
          return detailText ? <p className="d2-item-card__detail">{detailText}</p> : null;
        })()}

        {card.kind === "permission_approval" && linkedPerm && (
          <div className="d2-param-box">
            <div className="d2-param-row">
              <span>Инструмент</span>
              <code>{linkedPerm.tool_name}</code>
            </div>
            <div className="d2-param-row">
              <span>Цель</span>
              <code>
                {linkedPerm.normalized_targets.length > 0
                  ? linkedPerm.normalized_targets.join(", ")
                  : linkedPerm.target_summary}
              </code>
            </div>
            {snippet && (
              <div
                className="d2-preview-snippet"
                data-testid={`permission-preview-snippet-${linkedPerm.request_id}`}
              >
                <div className="d2-preview-snippet__head">
                  <span>Предпросмотр изменений</span>
                  {snippetExpandable && (
                    <button
                      type="button"
                      className="popover-action-link"
                      onClick={() =>
                        setExpandedSnippets((prev) => ({
                          ...prev,
                          [card.card_id]: !prev[card.card_id],
                        }))
                      }
                    >
                      {snippetExpanded ? "Свернуть" : "Показать полностью"}
                    </button>
                  )}
                </div>
                <pre>{snippetExpanded || !snippetExpandable ? snippet : truncateSnippet(snippet)}</pre>
              </div>
            )}
          </div>
        )}

        {card.kind === "unknown_effect" && (
          <div className="d2-param-box d2-param-box--unknown">
            <p className="d2-unknown-warning">
              <ShieldWarning size={14} aria-hidden="true" />
              Запрос отправлен во внешний сервис, но подтверждение потеряно. Слепой повтор
              заблокирован, чтобы не создать дубль.
            </p>
            {linkedEffect && (
              <>
                <div className="d2-param-row">
                  <span>Операция / Получатель</span>
                  <code>
                    {linkedEffect.operation} → {linkedEffect.target_recipient}
                  </code>
                </div>
                <div className="d2-param-row">
                  <span>Ключ идемпотентности</span>
                  <code>{linkedEffect.effect_idempotency_key}</code>
                </div>
              </>
            )}
          </div>
        )}

        {card.kind === "permission_approval" && (
          <div className="d2-item-card__actions">
            <button
              type="button"
              className="popover-primary-btn"
              disabled={isBusy}
              data-testid={`inbox-approve-once-${card.card_id}`}
              onClick={() =>
                void runBusy(card.card_id, () =>
                  actions.resolveInboxCard(card.card_id, card.revision, {
                    action: "permission",
                    decision: "approve_once",
                  }),
                )
              }
            >
              <Check size={13} aria-hidden="true" />
              Разрешить один раз
            </button>
            <button
              type="button"
              className="popover-secondary-btn"
              disabled={isBusy}
              data-testid={`inbox-approve-project-${card.card_id}`}
              onClick={() =>
                void runBusy(card.card_id, () =>
                  actions.resolveInboxCard(card.card_id, card.revision, {
                    action: "permission",
                    decision: "approve_project",
                  }),
                )
              }
            >
              Разрешить для проекта
            </button>
            <button
              type="button"
              className="d2-danger-btn"
              disabled={isBusy}
              data-testid={`inbox-deny-${card.card_id}`}
              onClick={() =>
                void runBusy(card.card_id, () =>
                  actions.resolveInboxCard(card.card_id, card.revision, {
                    action: "permission",
                    decision: "deny",
                  }),
                )
              }
            >
              <X size={13} aria-hidden="true" />
              Отклонить
            </button>
          </div>
        )}

        {card.kind === "clarification" && (
          <form
            className="d2-inline-reply"
            onSubmit={(e) => {
              e.preventDefault();
              if (!draft.trim() || isBusy) {
                return;
              }
              void runBusy(card.card_id, async () => {
                await actions.resolveInboxCard(card.card_id, card.revision, {
                  action: "clarification",
                  answer_text: draft.trim(),
                });
                setReplyDrafts((prev) => ({ ...prev, [card.card_id]: "" }));
              });
            }}
          >
            <input
              type="text"
              value={draft}
              onChange={(e) =>
                setReplyDrafts((prev) => ({ ...prev, [card.card_id]: e.target.value }))
              }
              placeholder="Напишите ответ агенту…"
              aria-label={`Ответ на уточнение: ${card.subject}`}
              data-testid={`inbox-clarify-input-${card.card_id}`}
            />
            <button
              type="submit"
              className="popover-primary-btn"
              disabled={isBusy || !draft.trim()}
              data-testid={`inbox-clarify-submit-${card.card_id}`}
            >
              Ответить
            </button>
            {card.task_id && (
              <button
                type="button"
                className="d2-danger-btn"
                disabled={isBusy}
                onClick={() =>
                  void runBusy(card.card_id, () =>
                    actions.resolveInboxCard(card.card_id, card.revision, {
                      action: "stop_task",
                    }),
                  )
                }
              >
                Остановить
              </button>
            )}
          </form>
        )}

        {card.kind === "unknown_effect" && (
          <div className="d2-item-card__actions">
            <button
              type="button"
              className="popover-primary-btn"
              disabled={isBusy}
              data-testid={`inbox-reconcile-auto-${card.card_id}`}
              onClick={() =>
                void runBusy(card.card_id, () =>
                  actions.resolveInboxCard(card.card_id, card.revision, {
                    action: "reconcile_effect",
                    manual_resolution: null,
                  }),
                )
              }
            >
              <ArrowsClockwise size={13} aria-hidden="true" />
              Проверить у сервиса
            </button>
            <button
              type="button"
              className="popover-secondary-btn"
              disabled={isBusy}
              data-testid={`inbox-reconcile-confirmed-${card.card_id}`}
              onClick={() =>
                void runBusy(card.card_id, () =>
                  actions.resolveInboxCard(card.card_id, card.revision, {
                    action: "reconcile_effect",
                    manual_resolution: "mark_confirmed",
                    reason: "Подтверждено вручную владельцем",
                  }),
                )
              }
            >
              Подтвердить выполнение
            </button>
            <button
              type="button"
              className="d2-danger-btn"
              disabled={isBusy}
              data-testid={`inbox-reconcile-not-executed-${card.card_id}`}
              onClick={() =>
                void runBusy(card.card_id, () =>
                  actions.resolveInboxCard(card.card_id, card.revision, {
                    action: "reconcile_effect",
                    manual_resolution: "mark_not_executed",
                    reason: "Отмечено невыполненным владельцем",
                  }),
                )
              }
            >
              Отметить невыполненным
            </button>
          </div>
        )}

        {card.kind === "budget_exceeded" && (
          <div className="d2-item-card__actions">
            <button
              type="button"
              className="popover-primary-btn"
              disabled={isBusy}
              data-testid={`inbox-budget-increase-${card.card_id}`}
              onClick={() =>
                void runBusy(card.card_id, () =>
                  actions.resolveInboxCard(card.card_id, card.revision, {
                    action: "increase_budget",
                    additional_turns: 3,
                    additional_tokens: 10000,
                    additional_wall_secs: 120,
                  }),
                )
              }
            >
              Добавить бюджет и продолжить
            </button>
            <button
              type="button"
              className="d2-danger-btn"
              disabled={isBusy}
              onClick={() =>
                void runBusy(card.card_id, () =>
                  actions.resolveInboxCard(card.card_id, card.revision, {
                    action: "stop_task",
                  }),
                )
              }
            >
              Завершить
            </button>
          </div>
        )}

        {card.kind === "missed_schedule" && (
          <div className="d2-item-card__actions">
            <button
              type="button"
              className="popover-primary-btn"
              disabled={isBusy}
              data-testid={`inbox-missed-run-now-${card.card_id}`}
              onClick={() =>
                void runBusy(card.card_id, () =>
                  actions.resolveInboxCard(card.card_id, card.revision, {
                    action: "missed_schedule",
                    run_now: true,
                  }),
                )
              }
            >
              Запустить сейчас (1 раз)
            </button>
            <button
              type="button"
              className="popover-secondary-btn"
              disabled={isBusy}
              data-testid={`inbox-missed-skip-${card.card_id}`}
              onClick={() =>
                void runBusy(card.card_id, () =>
                  actions.resolveInboxCard(card.card_id, card.revision, {
                    action: "missed_schedule",
                    run_now: false,
                  }),
                )
              }
            >
              Пропустить до следующего окна
            </button>
          </div>
        )}
      </article>
    );
  };

  const renderGrant = (grant: CapabilityGrantRecord) => {
    const projectName = projectById.get(grant.project_id) ?? "Проект";
    return (
      <div
        key={grant.grant_id}
        className="d2-grant-row"
        data-testid={`grant-row-${grant.grant_id}`}
      >
        <div className="d2-grant-row__info">
          <strong>
            <Key size={13} aria-hidden="true" />
            {grant.tool_pattern === "workspace_write"
              ? "Запись в папку проекта"
              : grant.tool_pattern}
          </strong>
          <small>
            {projectName} · использовано: {grant.uses_consumed}
            {grant.max_uses ? ` / ${grant.max_uses}` : ""} · {formatDecisionReasonRu(grant.reason)}
          </small>
        </div>
        <button
          type="button"
          className="d2-danger-btn d2-danger-btn--sm"
          data-testid={`revoke-grant-${grant.grant_id}`}
          onClick={() => void actions.revokeCapabilityGrant(grant.grant_id)}
        >
          Отозвать
        </button>
      </div>
    );
  };

  return (
    <main className="chat-card d2-surface-card" data-testid="inbox-radar-card">
      <StatusBanners />

      <header className="d2-card-header">
        <div className="d2-card-header__title">
          <Tray size={18} aria-hidden="true" />
          <div>
            <h1>Входящие и Радар</h1>
            <p>Подтверждения записи, уточнения агентов, неизвестные исходы и активные разрешения</p>
          </div>
        </div>

        <div className="d2-card-header__controls">
          <ProjectScopeFilter
            projects={activeProjects}
            selectedProjectId={projectFilter}
            onChange={setProjectFilter}
            totalCount={allOpenCards.length}
            countByProjectId={countByProjectId}
            ariaLabel="Фильтр Входящих по проектам"
          />
          <button
            type="button"
            className="icon-button"
            onClick={() => void actions.refreshAll()}
            aria-label="Обновить входящие"
            title="Обновить"
          >
            <ArrowsClockwise size={15} />
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
          <section className="d2-section" aria-label="Требуют решения">
            <div className="d2-section__head">
              <h2>
                Требуют решения <span>{openCards.length}</span>
              </h2>
            </div>

            {openCards.length === 0 ? (
              <div className="d2-empty-box" data-testid="inbox-empty-state">
                <Tray size={24} weight="light" aria-hidden="true" />
                <p>Срочных запросов нет — все подтверждения и уточнения обработаны.</p>
              </div>
            ) : (
              <div className="d2-card-list">{openCards.map(renderCard)}</div>
            )}
          </section>

          {failedTasks.length > 0 && (
            <section className="d2-section" aria-label="Поручения с ошибкой">
              <div className="d2-section__head">
                <h2>
                  Ошибки поручений <span>{failedTasks.length}</span>
                </h2>
              </div>
              <div className="d2-card-list">
                {failedTasks.map((task) => (
                  <div
                    key={task.task_id}
                    className="d2-radar-row d2-radar-row--failed"
                    data-testid={`inbox-failed-task-${task.task_id}`}
                  >
                    <WarningCircle size={15} aria-hidden="true" />
                    <div className="d2-radar-row__copy">
                      <strong>{task.title}</strong>
                      <small>
                        {formatDecisionReasonRu(task.error_summary) ||
                          task.error_code ||
                          "Выполнение прервано"}
                      </small>
                    </div>
                    <button
                      type="button"
                      className="d2-link-btn"
                      onClick={() => void actions.openSessionInChat(task.session_id)}
                    >
                      Открыть чат
                    </button>
                  </div>
                ))}
              </div>
            </section>
          )}

          <section className="d2-section" aria-label="Действующие разрешения">
            <div className="d2-section__head">
              <h2>
                Действующие разрешения <span>{visibleGrants.length}</span>
              </h2>
            </div>
            {visibleGrants.length === 0 ? (
              <p className="d2-section-empty">
                Постоянных выданных разрешений нет. При выборе «Разрешить для проекта» разрешение
                появится здесь с возможностью мгновенного отзыва.
              </p>
            ) : (
              <div className="d2-grant-list">{visibleGrants.map(renderGrant)}</div>
            )}
          </section>

          <section className="d2-section" aria-label="Радар активности">
            <div className="d2-section__head">
              <h2>
                Радар активности <span>{radarTasks.length + radarRunningSessions.length}</span>
              </h2>
            </div>
            {radarTasks.length === 0 && radarRunningSessions.length === 0 ? (
              <p className="d2-section-empty">
                Сейчас нет активных чатов или ожидающих поручений.
              </p>
            ) : (
              <div className="d2-radar-list">
                {radarRunningSessions.map((session) => {
                  const awaitingApproval = pendingApprovalSessions.has(session.session_id);
                  return (
                    <div
                      key={session.session_id}
                      className="d2-radar-row"
                      data-testid={`radar-running-session-${session.session_id}`}
                    >
                      <span
                        className={`d2-pill ${
                          awaitingApproval
                            ? "d2-pill--status-waiting_approval"
                            : "d2-pill--status-running"
                        }`}
                      >
                        {awaitingApproval ? "Ожидает подтверждения" : "Выполняется"}
                      </span>
                      <div className="d2-radar-row__copy">
                        <strong title={session.title}>{session.title}</strong>
                        <small>
                          {session.project_id
                            ? (projectById.get(session.project_id) ?? "Проект")
                            : "Чаты"}{" "}
                          ·{" "}
                          {awaitingApproval ? "ожидает вашего решения" : "активный ход в чате"}
                        </small>
                      </div>
                      <div className="d2-radar-row__actions">
                        <button
                          type="button"
                          className="d2-link-btn"
                          onClick={() => void actions.openSessionInChat(session.session_id)}
                        >
                          Чат
                        </button>
                        <button
                          type="button"
                          className="d2-danger-btn d2-danger-btn--sm"
                          data-testid={`radar-stop-session-${session.session_id}`}
                          disabled={state.cancellingSessionIds.includes(session.session_id)}
                          onClick={() => void actions.cancelSessionRun(session.session_id)}
                        >
                          <Stop size={11} weight="fill" aria-hidden="true" />
                          Stop
                        </button>
                      </div>
                    </div>
                  );
                })}
                {radarTasks.map((task) => {
                  const awaitingApproval = pendingApprovalSessions.has(task.session_id);
                  const effectiveStatus =
                    awaitingApproval && task.status === "running"
                      ? "waiting_approval"
                      : task.status;
                  return (
                    <div key={task.task_id} className="d2-radar-row">
                      <span className={`d2-pill d2-pill--status-${effectiveStatus}`}>
                        {taskStatusLabel(task.status, awaitingApproval)}
                      </span>
                      <div className="d2-radar-row__copy">
                        <strong title={task.title}>{task.title}</strong>
                        <small>
                          {projectById.get(task.project_id) ?? "Проект"}
                          {task.wait_detail ? ` · ${cleanPlainText(task.wait_detail)}` : ""}
                        </small>
                      </div>
                      <div className="d2-radar-row__actions">
                        <button
                          type="button"
                          className="d2-link-btn"
                          onClick={() => void actions.openSessionInChat(task.session_id)}
                        >
                          Чат
                        </button>
                        <button
                          type="button"
                          className="d2-danger-btn d2-danger-btn--sm"
                          disabled={state.stoppingTaskIds.includes(task.task_id)}
                          onClick={() => void actions.cancelTask(task.task_id)}
                        >
                          <Stop size={11} weight="fill" aria-hidden="true" />
                          Stop
                        </button>
                      </div>
                    </div>
                  );
                })}
              </div>
            )}
          </section>
        </div>
      </div>
    </main>
  );
}
