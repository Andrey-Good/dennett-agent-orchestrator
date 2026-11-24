import { readFileSync } from "node:fs";
import path from "node:path";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { WindowControls } from "../api/bridge";
import type {
  AgentRecord,
  ArtifactRecord,
  EffectClaimRecord,
  InboxCardRecord,
  PermissionRequestRecord,
  ProjectRecord,
  SessionRecord,
  TaskRecord,
} from "../api/contracts";
import { MockDesktopBridge } from "../api/mockBridge";
import { App } from "../App";

function createTestWindowControls(): WindowControls {
  return {
    supported: true,
    minimize: vi.fn(async () => {}),
    toggleMaximize: vi.fn(async () => {}),
    close: vi.fn(async () => {}),
    startDragging: vi.fn(async () => {}),
    isMaximized: vi.fn(async () => false),
  };
}

function makeD2Fixture() {
  const now = Date.now();
  const project: ProjectRecord = {
    project_id: "10000000-0000-4000-8000-000000000d01",
    name: "dennett-d2",
    workspace_path: "A:/Dev/dennett-d2",
    memory_scope: "project:dennett-d2",
    lifecycle_state: "active",
    archived_at: null,
    created_at: now - 30_000,
  };
  const agent: AgentRecord = {
    agent_id: "20000000-0000-4000-8000-000000000d01",
    name: "Проектный агент",
    description: "Интерактивный агент D2",
    current_config_version_id: "21000000-0000-4000-8000-000000000d01",
    current_config: {
      config_version_id: "21000000-0000-4000-8000-000000000d01",
      agent_id: "20000000-0000-4000-8000-000000000d01",
      version_number: 1,
      system_prompt: "Помогай проекту D2.",
      model_preference: "gemini-4-neon",
      thinking_level: "medium",
      allowed_memory_scopes: ["project:dennett-d2"],
      workspace_write_policy: { mode: "ask_before_write" },
      created_at: now - 30_000,
    },
    created_at: now - 30_000,
    updated_at: now - 30_000,
  };
  const session: SessionRecord = {
    session_id: "30000000-0000-4000-8000-000000000d01",
    agent_id: agent.agent_id,
    project_id: project.project_id,
    title: "Диалог D2",
    runtime_binding: null,
    status: "active",
    created_at: now - 25_000,
    updated_at: now - 5_000,
  };
  return { now, project, agent, session };
}

describe("Dennett Desktop Phase D2 UI", () => {
  it("surfaces live Action Inbox badge, resolves approval/clarification/UNKNOWN cards, revokes grants in SettingsSheet, and navigates 1-click to chat (zero <select>)", async () => {
    const user = userEvent.setup();
    const { now, project, agent, session } = makeD2Fixture();

    const permReq: PermissionRequestRecord = {
      request_id: "81000000-0000-4000-8000-000000000001",
      project_id: project.project_id,
      session_id: session.session_id,
      run_id: "40000000-0000-4000-8000-000000000001",
      task_id: null,
      agent_id: agent.agent_id,
      tool_call_id: "call-write-1",
      tool_name: "write_file",
      tool_kind: "edit",
      target_summary: "docs/plan.md",
      normalized_targets: ["docs/plan.md"],
      parameters_sha256: "a".repeat(64),
      reason: "Требуется подтверждение записи в режиме «Спрашивать запись».",
      status: "pending",
      revision: 1,
      expires_at: now + 120_000,
      correlation_id: "corr-perm-1",
      created_at: now - 1_000,
    };

    const clarifyTask: TaskRecord = {
      task_id: "75000000-0000-4000-8000-000000000001",
      project_id: project.project_id,
      session_id: session.session_id,
      agent_id: agent.agent_id,
      title: "Подготовка релиза",
      goal: "Подготовить заметки релиза",
      generation: 1,
      status: "waiting_clarification",
      wait_detail: "Какую ветку брать за основу?",
      budget: { exceed_policy: "ask_approval" },
      usage: { turns_used: 1, tokens_used: 150, wall_ms_used: 900, confidence: "provider_reported" },
      attached_skills: [],
      created_at: now - 10_000,
      updated_at: now - 2_000,
    };

    const unknownEffect: EffectClaimRecord = {
      effect_id: "88000000-0000-4000-8000-000000000001",
      project_id: project.project_id,
      session_id: session.session_id,
      task_id: "75000000-0000-4000-8000-000000000002",
      agent_id: agent.agent_id,
      effect_idempotency_key: "webhook-deploy-001",
      connector_id: "simulated_external",
      operation: "send_webhook",
      target_recipient: "ops@example.test",
      parameters_json: "{}",
      parameters_sha256: "b".repeat(64),
      status: "unknown",
      error_summary: "Lost acknowledgement",
      correlation_id: "corr-eff-1",
      created_at: now - 5_000,
      updated_at: now - 3_000,
    };

    const unknownTask: TaskRecord = {
      task_id: "75000000-0000-4000-8000-000000000002",
      project_id: project.project_id,
      session_id: session.session_id,
      agent_id: agent.agent_id,
      title: "Отправка уведомления",
      goal: "Уведомить ops",
      generation: 1,
      status: "unknown_external",
      wait_detail: "Ожидает сверки внешнего эффекта",
      budget: { exceed_policy: "ask_approval" },
      usage: { turns_used: 1, tokens_used: 0, wall_ms_used: 500, confidence: "unknown" },
      attached_skills: [],
      created_at: now - 8_000,
      updated_at: now - 3_000,
    };

    const inboxCards: InboxCardRecord[] = [
      {
        card_id: "89000000-0000-4000-8000-000000000001",
        project_id: project.project_id,
        session_id: session.session_id,
        agent_id: agent.agent_id,
        kind: "permission_approval",
        status: "open",
        urgency: "high",
        subject: "Запрос записи: write_file (docs/plan.md)",
        detail: "Агент хочет создать docs/plan.md в папке проекта.",
        permission_request_id: permReq.request_id,
        revision: 1,
        created_at: now - 1_000,
        updated_at: now - 1_000,
      },
      {
        card_id: "89000000-0000-4000-8000-000000000002",
        project_id: project.project_id,
        session_id: session.session_id,
        task_id: clarifyTask.task_id,
        agent_id: agent.agent_id,
        kind: "clarification",
        status: "open",
        urgency: "normal",
        subject: "Уточнение по релизу",
        detail: "Какую ветку брать за основу?",
        revision: 1,
        created_at: now - 2_000,
        updated_at: now - 2_000,
      },
      {
        card_id: "89000000-0000-4000-8000-000000000003",
        project_id: project.project_id,
        session_id: session.session_id,
        task_id: unknownTask.task_id,
        agent_id: agent.agent_id,
        kind: "unknown_effect",
        status: "open",
        urgency: "critical",
        subject: "Неизвестный исход: send_webhook → ops@example.test",
        detail: "Подтверждение внешнего сервиса потеряно.",
        effect_id: unknownEffect.effect_id,
        revision: 1,
        created_at: now - 3_000,
        updated_at: now - 3_000,
      },
    ];

    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [project],
      agents: [agent],
      sessions: [session],
      tasks: [clarifyTask, unknownTask],
      inboxCards,
      permissionRequests: [permReq],
      effects: [unknownEffect],
    });

    const { container } = render(
      <App bridge={bridge} windowControls={createTestWindowControls()} />,
    );

    // 1. Live Inbox badge shows exact open count (3) AND active chat timeline shows inline approval/clarification/UNKNOWN cards
    expect(await screen.findByTestId("rail-inbox-badge")).toHaveTextContent("3");
    expect(screen.getByTestId(`inline-permission-card-${permReq.request_id}`)).toBeInTheDocument();
    expect(
      screen.getByTestId(`inline-clarification-card-${inboxCards[1]!.card_id}`),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId(`inline-unknown-effect-card-${unknownEffect.effect_id}`),
    ).toHaveTextContent("Неизвестно, выполнено ли (UNKNOWN)");

    // 2. Switch to Входящие view via ActivityRail
    await user.click(screen.getByTestId("rail-nav-inbox"));
    const inboxCard = await screen.findByTestId("inbox-radar-card");
    expect(container.querySelectorAll("select")).toHaveLength(0);

    // 3. Approve permission for project -> creates active revocable grant & decrements badge to 2
    await user.click(within(inboxCard).getByTestId(`inbox-approve-project-${inboxCards[0]!.card_id}`));
    await waitFor(() => {
      expect(screen.getByTestId("rail-inbox-badge")).toHaveTextContent("2");
    });
    expect(bridge.capabilityGrants).toHaveLength(1);
    const issuedGrantId = bridge.capabilityGrants[0]!.grant_id;
    expect(screen.getByTestId(`grant-row-${issuedGrantId}`)).toBeInTheDocument();

    // 4. Answer clarification inline -> resumes task & decrements badge to 1
    await user.type(
      screen.getByTestId(`inbox-clarify-input-${inboxCards[1]!.card_id}`),
      "Бери main",
    );
    await user.click(screen.getByTestId(`inbox-clarify-submit-${inboxCards[1]!.card_id}`));
    await waitFor(() => {
      expect(screen.getByTestId("rail-inbox-badge")).toHaveTextContent("1");
    });

    // 5. Inspect UNKNOWN card (no blind retry!) and reconcile via Подтвердить выполнение
    const unkCardEl = screen.getByTestId(`inbox-card-${inboxCards[2]!.card_id}`);
    expect(unkCardEl).toHaveTextContent("Неизвестно, выполнено ли (UNKNOWN)");
    expect(unkCardEl).toHaveTextContent("Слепой повтор заблокирован");
    expect(within(unkCardEl).queryByRole("button", { name: /^Повторить$/i })).not.toBeInTheDocument();
    await user.click(within(unkCardEl).getByTestId(`inbox-reconcile-confirmed-${inboxCards[2]!.card_id}`));

    // Badge completely hidden at 0!
    await waitFor(() => {
      expect(screen.queryByTestId("rail-inbox-badge")).not.toBeInTheDocument();
    });
    expect(await screen.findByTestId("inbox-empty-state")).toBeInTheDocument();

    // 6. Revoke the issued project grant directly in SettingsSheet
    await user.click(screen.getByTestId("settings-trigger"));
    const settingsSheet = await screen.findByTestId("settings-sheet");
    expect(within(settingsSheet).getByTestId(`settings-grant-row-${issuedGrantId}`)).toBeInTheDocument();
    await user.click(within(settingsSheet).getByTestId(`settings-revoke-grant-${issuedGrantId}`));
    await waitFor(() => {
      expect(bridge.capabilityGrants[0]?.revoked_at).not.toBeNull();
    });
    await user.click(screen.getByRole("button", { name: "Закрыть настройки" }));

    // 7. 1-click navigation back to chat via Radar "Чат" button (clarifyTask is now running in Radar)
    await user.click(screen.getByRole("button", { name: /^Чат$/ }));
    expect(await screen.findByTestId("chat-card")).toBeInTheDocument();
  });

  it("supports Поручения zero-bureaucracy creation, distinct status pills, inline goal edit (generation bump), and targeted per-task Stop leaving sibling alive", async () => {
    const user = userEvent.setup();
    const { now, project, agent, session } = makeD2Fixture();

    const makeTask = (id: string, title: string, status: TaskRecord["status"]): TaskRecord => ({
      task_id: id,
      project_id: project.project_id,
      session_id: session.session_id,
      agent_id: agent.agent_id,
      title,
      goal: `Цель для ${title}`,
      generation: 1,
      status,
      schedule:
        status === "waiting_timer"
          ? {
              timezone: "UTC",
              original_expression: "через 5 минут",
              kind: { kind: "once_at", fire_at: now + 300_000 },
              missed_run_policy: "execute_once",
              next_fire_at: now + 300_000,
            }
          : null,
      budget: { max_turns: 5, exceed_policy: "ask_approval" },
      usage: { turns_used: 1, tokens_used: 120, wall_ms_used: 400, confidence: "provider_reported" },
      attached_skills: [],
      error_code: status === "failed" ? "BUDGET_EXCEEDED" : null,
      error_summary: status === "failed" ? "Лимит ходов исчерпан" : null,
      created_at: now - 5_000,
      updated_at: now - 1_000,
    });

    const runningA = makeTask("75000000-0000-4000-8000-000000000101", "Задача Альфа", "running");
    const runningB = makeTask("75000000-0000-4000-8000-000000000102", "Задача Бета", "running");
    const waitingT = makeTask("75000000-0000-4000-8000-000000000103", "Таймер", "waiting_timer");
    const unknownT = makeTask(
      "75000000-0000-4000-8000-000000000104",
      "Внешний вебхук",
      "unknown_external",
    );
    const failedT = makeTask("75000000-0000-4000-8000-000000000105", "Сбойная задача", "failed");
    const doneT = makeTask("75000000-0000-4000-8000-000000000106", "Готовая задача", "succeeded");

    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [project],
      agents: [agent],
      sessions: [session],
      tasks: [runningA, runningB, waitingT, unknownT, failedT, doneT],
    });

    const { container } = render(
      <App bridge={bridge} windowControls={createTestWindowControls()} />,
    );

    await user.click(await screen.findByTestId("rail-nav-tasks"));
    const tasksCard = await screen.findByTestId("tasks-card");
    expect(container.querySelectorAll("select")).toHaveLength(0);

    // Distinct status badges rendered cleanly
    expect(within(tasksCard).getAllByTestId("task-status-pill-running")).toHaveLength(2);
    expect(within(tasksCard).getByTestId("task-status-pill-waiting_timer")).toHaveTextContent(
      /Ожидает/,
    );
    expect(within(tasksCard).getByTestId("task-status-pill-unknown_external")).toHaveTextContent(
      "Неизвестно, выполнено ли",
    );
    expect(within(tasksCard).getByTestId("task-status-pill-failed")).toHaveTextContent("Ошибка");
    expect(within(tasksCard).getByTestId("task-status-pill-succeeded")).toHaveTextContent("Готово");

    // Create event via "+" button -> opens blank chat prefilled with "Создай событие: мне необходимо "
    expect(screen.queryByTestId("task-schedule-trigger")).not.toBeInTheDocument();
    expect(screen.queryByTestId("task-goal-input")).not.toBeInTheDocument();

    // Inline goal edit bumps generation to v2
    await user.click(screen.getByTestId(`task-edit-goal-btn-${waitingT.task_id}`));
    const editInput = await screen.findByTestId(`task-edit-goal-input-${waitingT.task_id}`);
    await user.clear(editInput);
    await user.type(editInput, "Обновлённая цель v2{Enter}");
    await waitFor(() => {
      expect(screen.getByTestId(`task-generation-${waitingT.task_id}`)).toHaveTextContent("цель v2");
    });

    // Targeted Stop on Задача Альфа leaves Задача Бета alive (running)!
    await user.click(screen.getByTestId(`task-stop-btn-${runningA.task_id}`));
    await waitFor(() => {
      expect(bridge.tasks.find((t) => t.task_id === runningA.task_id)?.status).toBe("cancelled");
    });
    expect(bridge.tasks.find((t) => t.task_id === runningB.task_id)?.status).toBe("running");
    expect(
      within(screen.getByTestId(`task-card-${runningA.task_id}`)).getByTestId(
        "task-status-pill-cancelled",
      ),
    ).toHaveTextContent("Остановлено");

    // Clicking "+" ("Создать событие") switches to Чаты with prefilled composer draft
    await user.click(screen.getByTestId("task-create-plus-btn"));
    const composerInput = (await screen.findByLabelText(
      "Поле ввода сообщения",
    )) as HTMLTextAreaElement;
    expect(composerInput.value).toBe("Создай событие: мне необходимо ");
  });

  it("persists drafts and queues offline messages with stable client_message_id, requiring explicit owner click to flush once upon Head reconnect", async () => {
    const user = userEvent.setup();
    const { project, agent, session } = makeD2Fixture();
    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [project],
      agents: [agent],
      sessions: [session],
    });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);
    const composer = await screen.findByLabelText("Поле ввода сообщения");

    // Simulate Head dropping offline mid-session
    bridge.connected = false;
    await user.type(composer, "Сообщение в офлайн-очередь{Enter}");

    // Honest offline banner + queued pill appear instead of losing text
    expect(await screen.findByTestId("head-offline-inline-banner")).toBeInTheDocument();
    expect(
      await screen.findByText("Ожидает сети — можно отправить после восстановления связи"),
    ).toBeInTheDocument();
    expect(screen.getByText("Сообщение в офлайн-очередь")).toBeInTheDocument();

    // Draft + queue persisted in localStorage
    const stored = window.localStorage.getItem("dennett.desktop.d2.drafts.v1");
    expect(stored).toContain("Сообщение в офлайн-очередь");

    // Reconnect Head via inline Retry button -> D2-BUG-06: does NOT blindly auto-flush!
    bridge.connected = true;
    await user.click(screen.getByTestId("inline-retry-head-button"));

    const flushBtn = await screen.findByTestId("flush-offline-queue-btn");
    await waitFor(() => {
      expect(flushBtn).not.toBeDisabled();
    });
    expect(bridge.calls.filter((c) => c.method === "sendMessage")).toHaveLength(1);

    // Explicit owner click sends queued message ONCE with the exact same stable client_message_id!
    await user.click(flushBtn);

    await waitFor(() => {
      expect(screen.queryByTestId("flush-offline-queue-btn")).not.toBeInTheDocument();
    });
    expect(await screen.findByText(/Принято .* Сообщение в офлайн-очередь/)).toBeInTheDocument();

    const sendCalls = bridge.calls.filter((c) => c.method === "sendMessage");
    // 1 failed attempt (HEAD_UNREACHABLE) + 1 owner-confirmed reconnect flush, BOTH carrying the exact same client_message_id!
    expect(sendCalls).toHaveLength(2);
    const firstId = (sendCalls[0]!.args as { request: { client_message_id?: string } }).request
      .client_message_id;
    const secondId = (sendCalls[1]!.args as { request: { client_message_id?: string } }).request
      .client_message_id;
    expect(firstId).toBeTruthy();
    expect(secondId).toBe(firstId);
  });

  it("supports Contract C project lifecycle menu (archive/unarchive/rebind/detach/remove/delete) and Contract D artifact revisions v1/v2 & lifecycle", async () => {
    const user = userEvent.setup();
    const { now, project, agent, session } = makeD2Fixture();
    const secondProject: ProjectRecord = {
      project_id: "10000000-0000-4000-8000-000000000d02",
      name: "project-to-delete",
      workspace_path: "A:/Dev/keep-my-files",
      memory_scope: "project:keep-my-files",
      lifecycle_state: "active",
      archived_at: null,
      created_at: now - 20_000,
    };
    const artV1: ArtifactRecord = {
      artifact_id: "70000000-0000-4000-8000-000000000001",
      session_id: session.session_id,
      project_id: project.project_id,
      name: "report.md",
      mime_type: "text/markdown",
      revision: 1,
      content: "# Черновик v1",
      sha256: "1".repeat(64),
      completeness_status: "partial",
      lifecycle_state: "active",
      created_at: now - 5_000,
    };
    const artV2: ArtifactRecord = {
      artifact_id: "70000000-0000-4000-8000-000000000002",
      session_id: session.session_id,
      project_id: project.project_id,
      name: "report.md",
      mime_type: "text/markdown",
      revision: 2,
      content: "# Финальная версия v2",
      sha256: "2".repeat(64),
      completeness_status: "final",
      lifecycle_state: "active",
      created_at: now - 2_000,
    };

    const bridge = new MockDesktopBridge({
      connected: true,
      supportsNativeFolderPicker: true,
      mockPickedFolder: "A:/Dev/rebound-dir",
      projects: [project, secondProject],
      agents: [agent],
      sessions: [session],
      artifactsBySession: { [session.session_id]: [artV1, artV2] },
    });

    const { container } = render(
      <App
        bridge={bridge}
        windowControls={createTestWindowControls()}
        initialRightPanelOpen={true}
      />,
    );

    await screen.findByTestId("chat-card");
    expect(container.querySelectorAll("select")).toHaveLength(0);

    // 1. Artifact revisions v1/v2 switcher + completeness badge + safe Markdown preview + lifecycle actions
    const preview = await screen.findByTestId("artifact-preview");
    expect(preview).toHaveTextContent("Финальная версия v2");
    expect(screen.getByTestId("artifact-preview-completeness")).toHaveTextContent("Финальный");

    await user.click(screen.getByTestId("artifact-mode-source"));
    expect(screen.getByTestId("artifact-preview-source")).toHaveTextContent("# Финальная версия v2");
    await user.click(screen.getByTestId("artifact-mode-rendered"));

    await user.click(screen.getByTestId("artifact-revision-pill-1"));
    await waitFor(() => {
      expect(screen.getByTestId("artifact-preview")).toHaveTextContent("Черновик v1");
    });
    expect(screen.getByTestId("artifact-preview-completeness")).toHaveTextContent("Частичный");

    await user.click(screen.getByTestId("artifact-lifecycle-trigger"));
    expect(await screen.findByTestId("artifact-lifecycle-menu")).toBeInTheDocument();
    await user.click(screen.getByTestId("artifact-action-finalize"));
    await waitFor(() => {
      expect(screen.getByTestId("artifact-preview-completeness")).toHaveTextContent("Финальный");
    });

    // 2. Project lifecycle menu: Archive -> appears under Архив (1) -> Unarchive
    await user.click(screen.getByTestId(`project-menu-trigger-${project.project_id}`));
    await user.click(await screen.findByTestId(`project-menu-archive-${project.project_id}`));
    expect(await screen.findByTestId("sidebar-archive-toggle")).toHaveTextContent("Архив (1)");

    await user.click(screen.getByTestId("sidebar-archive-toggle"));
    await user.click(await screen.findByTestId(`unarchive-project-btn-${project.project_id}`));
    await waitFor(() => {
      expect(screen.queryByTestId("sidebar-archive-section")).not.toBeInTheDocument();
    });

    // 3. Rebind folder & Detach workspace
    await user.click(screen.getByTestId(`project-menu-trigger-${project.project_id}`));
    await user.click(await screen.findByTestId(`project-menu-rebind-${project.project_id}`));
    await waitFor(() => {
      expect(bridge.projects.find((p) => p.project_id === project.project_id)?.workspace_path).toBe(
        "A:/Dev/rebound-dir",
      );
    });

    await user.click(screen.getByTestId(`project-menu-trigger-${project.project_id}`));
    await user.click(await screen.findByTestId(`project-menu-detach-${project.project_id}`));
    await waitFor(() => {
      expect(
        bridge.projects.find((p) => p.project_id === project.project_id)?.workspace_path,
      ).toBeNull();
    });

    // 4. Delete project opens explicit safety confirmation stating disk files are untouched
    await user.click(screen.getByTestId(`project-menu-trigger-${secondProject.project_id}`));
    await user.click(await screen.findByTestId(`project-menu-delete-${secondProject.project_id}`));
    const confirmDialog = await screen.findByTestId("project-delete-confirm-dialog");
    expect(confirmDialog).toHaveTextContent("останется полностью нетронутой");
    await user.click(screen.getByTestId("project-delete-confirm-btn"));
    await waitFor(() => {
      expect(
        bridge.projects.find((p) => p.project_id === secondProject.project_id)?.lifecycle_state,
      ).toBe("deleted_record");
    });
  });

  it("handles cursor-driven live sync and recovers automatically from 409 stream_epoch resync without error banner", async () => {
    const { now, project, agent, session } = makeD2Fixture();
    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [project],
      agents: [agent],
      sessions: [session],
    });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);
    await screen.findByTestId("chat-card");
    expect(screen.queryByTestId("rail-inbox-badge")).not.toBeInTheDocument();

    // Simulate backend epoch change (409 stream_epoch) + new open Inbox card arriving in background!
    bridge.authorityEpoch = 2;
    bridge.inboxCards.push({
      card_id: "89000000-0000-4000-8000-000000000999",
      project_id: project.project_id,
      session_id: session.session_id,
      kind: "clarification",
      status: "open",
      urgency: "normal",
      subject: "Фоновый вопрос после resync",
      detail: "Уточните формат отчёта.",
      revision: 1,
      created_at: now,
      updated_at: now,
    });
    bridge.emitControlEvent("task.clarification_requested", { projectId: project.project_id });

    // Background cursor poll hits 409 stream_epoch -> automatically re-snapshots -> badge updates live!
    expect(await screen.findByTestId("rail-inbox-badge", {}, { timeout: 4_500 })).toHaveTextContent(
      "1",
    );
    expect(screen.queryByTestId("error-banner")).not.toBeInTheDocument();
  });

  it("dismisses D2 project action menu via capture-phase Escape without cancelling an active run", async () => {
    const user = userEvent.setup();
    const { project, agent, session } = makeD2Fixture();
    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [project],
      agents: [agent],
      sessions: [session],
      sendDelayMs: 5_000,
    });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);
    await screen.findByTestId("chat-card");

    const textarea = await screen.findByLabelText("Поле ввода сообщения");
    await user.type(textarea, "Длинный запрос{Enter}");
    expect(await screen.findByTestId("running-control-bar")).toBeInTheDocument();

    // Open project `…` floating action menu while run is active, press Escape:
    // menu closes, active run is NOT cancelled!
    await user.click(screen.getByTestId(`project-menu-trigger-${project.project_id}`));
    expect(await screen.findByTestId(`project-menu-${project.project_id}`)).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByTestId(`project-menu-${project.project_id}`)).not.toBeInTheDocument();
    expect(bridge.calls.filter((c) => c.method === "cancelRun")).toHaveLength(0);
    expect(screen.getByTestId("running-control-bar")).toBeInTheDocument();

    // Second Escape (no open popovers/menus) cancels the active run normally.
    await user.keyboard("{Escape}");
    await waitFor(() => {
      expect(bridge.calls.filter((c) => c.method === "cancelRun")).toHaveLength(1);
    });
  });

  it("deduplicates optimistic user bubble during mid-turn waiting_approval refresh and formats Russian approval reason", async () => {
    const user = userEvent.setup();
    const { now, project, agent, session } = makeD2Fixture();
    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [project],
      agents: [agent],
      sessions: [session],
      sendDelayMs: 5_000,
    });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);
    const textarea = await screen.findByLabelText("Поле ввода сообщения");
    await user.type(textarea, "Создай файл approval-check.txt{Enter}");
    expect(await screen.findByTestId("running-control-bar")).toBeInTheDocument();

    // Simulate Head persisting the user message and pausing in waiting_approval mid-turn before sendMessage resolves!
    const sendCall = bridge.calls.find((c) => c.method === "sendMessage");
    const sentClientId =
      (sendCall?.args as { request?: { client_message_id?: string | null } } | undefined)?.request
        ?.client_message_id ?? null;
    expect(sentClientId).toBeTruthy();

    bridge.messagesBySession[session.session_id] = [
      {
        message_id: "55000000-0000-4000-8000-000000000001",
        session_id: session.session_id,
        role: "user",
        content: "Создай файл approval-check.txt",
        run_id: "44000000-0000-4000-8000-000000000001",
        client_message_id: sentClientId,
        correlation_id: "corr-persisted-1",
        created_at: Date.now(),
      },
    ];
    const permId = "88000000-0000-4000-8000-000000000201";
    bridge.permissionRequests.push({
      request_id: permId,
      project_id: project.project_id,
      session_id: session.session_id,
      run_id: "44000000-0000-4000-8000-000000000001",
      task_id: null,
      agent_id: agent.agent_id,
      tool_call_id: "tc-mid-turn-1",
      tool_name: "write_file",
      tool_kind: "write",
      target_summary: "approval-check.txt",
      normalized_targets: ["approval-check.txt"],
      parameters_sha256: "f".repeat(64),
      reason: "workspace_write_policy=ask_before_write requires owner approval to run write_file",
      status: "pending",
      revision: 1,
      expires_at: now + 300_000,
      correlation_id: "corr-perm-mid-1",
      created_at: now,
    });
    bridge.emitControlEvent("permission.requested", {
      projectId: project.project_id,
      sessionId: session.session_id,
    });

    // Mid-turn event poll reloads session detail and renders inline approval card cleanly
    const inlineCard = await screen.findByTestId(
      `inline-permission-card-${permId}`,
      {},
      { timeout: 3_000 },
    );
    expect(inlineCard).toHaveTextContent("Спрашивать запись");
    expect(inlineCard).not.toHaveTextContent("workspace_write_policy=ask_before_write");

    // User prompt bubble MUST appear exactly ONCE (no duplicate persisted + optimistic "отправка…")
    const timeline = screen.getByTestId("message-timeline");
    expect(within(timeline).getAllByText("Создай файл approval-check.txt")).toHaveLength(1);
    expect(within(timeline).queryByText("отправка…")).not.toBeInTheDocument();
  });

  it("flips archived project menu upward, excludes archived/deleted from CommandPalette, syncs TasksCard goal/title without duplicate text, and enforces CSS/favicon polish", async () => {
    const user = userEvent.setup();
    const { now, project, agent, session } = makeD2Fixture();

    const archivedProject: ProjectRecord = {
      project_id: "10000000-0000-4000-8000-000000000502",
      name: "Архивный проект",
      workspace_path: "A:/Dev/archived-proj",
      memory_scope: "project:archived-proj",
      lifecycle_state: "archived",
      archived_at: now - 10_000,
      created_at: now - 30_000,
    };
    const archivedSession: SessionRecord = {
      session_id: "30000000-0000-4000-8000-000000000502",
      agent_id: agent.agent_id,
      project_id: archivedProject.project_id,
      title: "Чат в архиве",
      status: "active",
      created_at: now - 20_000,
      updated_at: now - 10_000,
    };
    const deletedProject: ProjectRecord = {
      project_id: "10000000-0000-4000-8000-000000000503",
      name: "Удалённый проект",
      workspace_path: null,
      memory_scope: "project:deleted-proj",
      lifecycle_state: "deleted_record",
      archived_at: null,
      created_at: now - 40_000,
    };

    const sameTextTask: TaskRecord = {
      task_id: "75000000-0000-4000-8000-000000000501",
      project_id: project.project_id,
      session_id: session.session_id,
      agent_id: agent.agent_id,
      title: "Проверить отчёт по метрикам",
      goal: "Проверить отчёт по метрикам",
      generation: 1,
      status: "waiting_timer",
      schedule: {
        timezone: "UTC",
        original_expression: "через 5 минут",
        kind: { kind: "once_at", fire_at: now + 300_000 },
        missed_run_policy: "execute_once",
        next_fire_at: now + 300_000,
      },
      budget: {},
      usage: { turns_used: 0, tokens_used: 0, wall_ms_used: 0, confidence: "provider_reported" },
      attached_skills: [],
      created_at: now - 5_000,
      updated_at: now - 1_000,
    };
    const terminalTask: TaskRecord = {
      ...sameTextTask,
      task_id: "75000000-0000-4000-8000-000000000502",
      title: "Завершённое поручение",
      goal: "Завершённое поручение",
      status: "succeeded",
      schedule: null,
    };

    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [project, archivedProject, deletedProject],
      agents: [agent],
      sessions: [session, archivedSession],
      tasks: [sameTextTask, terminalTask],
    });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);
    await screen.findByTestId("chat-card");

    // 1. Archived project menu renders via PortalPopover anchored upward (data-placement="top") to avoid bottom clipping
    await user.click(await screen.findByTestId("sidebar-archive-toggle"));
    await user.click(await screen.findByTestId(`project-menu-trigger-${archivedProject.project_id}`));
    const archivedMenu = await screen.findByTestId(`project-menu-${archivedProject.project_id}`);
    expect(archivedMenu.parentElement).toBe(document.body);
    expect(archivedMenu.getAttribute("data-placement")).toBe("top");
    await user.keyboard("{Escape}");

    // 2. CommandPalette excludes archived and deleted projects (and their sessions)
    await user.keyboard("{Control>}k{/Control}");
    const palette = await screen.findByTestId("command-palette");
    expect(within(palette).getByTestId(`palette-item-project:${project.project_id}`)).toBeInTheDocument();
    expect(within(palette).getByTestId(`palette-item-session:${session.session_id}`)).toBeInTheDocument();
    expect(within(palette).queryByText("Архивный проект")).not.toBeInTheDocument();
    expect(within(palette).queryByText("Чат в архиве")).not.toBeInTheDocument();
    expect(within(palette).queryByText("Удалённый проект")).not.toBeInTheDocument();
    await user.keyboard("{Escape}");

    // 3. TasksCard: no duplicate text when title === goal, title syncs on goal edit, terminal task hides edit pencil
    await user.click(screen.getByTestId("rail-nav-tasks"));
    const sameCard = await screen.findByTestId(`task-card-${sameTextTask.task_id}`);
    expect(within(sameCard).getAllByText("Проверить отчёт по метрикам")).toHaveLength(1);
    expect(screen.queryByTestId(`task-edit-goal-btn-${terminalTask.task_id}`)).not.toBeInTheDocument();

    await user.dblClick(within(sameCard).getByText("Проверить отчёт по метрикам"));
    const editInput = await screen.findByTestId(`task-edit-goal-input-${sameTextTask.task_id}`);
    await user.clear(editInput);
    await user.type(editInput, "Собрать обновлённый отчёт v2{Enter}");

    await waitFor(() => {
      expect(screen.getByTestId(`task-generation-${sameTextTask.task_id}`)).toHaveTextContent("цель v2");
    });
    expect(within(sameCard).getAllByText("Собрать обновлённый отчёт v2")).toHaveLength(1);
    expect(within(sameCard).queryByText("Проверить отчёт по метрикам")).not.toBeInTheDocument();

    // 4. Static CSS & HTML polish guards (specificity, portals, dark stop hover, scrollbars, hover-only row actions, opaque rail badge, favicon)
    const css = readFileSync(path.resolve(__dirname, "../styles/desktop.css"), "utf-8");
    const html = readFileSync(path.resolve(__dirname, "../../index.html"), "utf-8");
    expect(css).not.toContain(".artifact-preview__meta span");
    expect(css).toContain(".artifact-preview__rev");
    expect(css).toContain(".portal-popover");
    expect(css).toContain(".send-button.stop-button:hover:not(:disabled)");
    expect(css).toContain("::-webkit-scrollbar-button");
    expect(css).toContain(".project-heading .row-action-btn:not(.is-menu-open)");
    expect(css).toContain("background: #2d3038;");
    expect(html).toContain('rel="icon"');
  });

  it("H2-03: archiving active project switches session and hides its tasks/inbox badge, and deleting project clears all orphaned state", async () => {
    const user = userEvent.setup();
    const { now, project, agent, session } = makeD2Fixture();

    const projectB: ProjectRecord = {
      project_id: "10000000-0000-4000-8000-000000000602",
      name: "Второй активный проект",
      workspace_path: "A:/Dev/second-proj",
      memory_scope: "project:second-proj",
      lifecycle_state: "active",
      archived_at: null,
      created_at: now - 50_000,
    };
    const sessionB: SessionRecord = {
      session_id: "30000000-0000-4000-8000-000000000602",
      agent_id: agent.agent_id,
      project_id: projectB.project_id,
      title: "Чат второго проекта",
      status: "active",
      created_at: now - 40_000,
      updated_at: now - 40_000,
    };
    const taskA: TaskRecord = {
      task_id: "75000000-0000-4000-8000-000000000601",
      project_id: project.project_id,
      session_id: session.session_id,
      agent_id: agent.agent_id,
      title: "Поручение первого проекта",
      goal: "Поручение первого проекта",
      generation: 1,
      status: "waiting_signal",
      expected_signal_name: "deploy_ok",
      wait_detail: "Ожидает сигнал deploy_ok",
      schedule: null,
      budget: {},
      usage: { turns_used: 0, tokens_used: 0, wall_ms_used: 0, confidence: "provider_reported" },
      attached_skills: [],
      created_at: now - 10_000,
      updated_at: now - 10_000,
    };
    const cardA: InboxCardRecord = {
      card_id: "89000000-0000-4000-8000-000000000601",
      project_id: project.project_id,
      session_id: session.session_id,
      task_id: taskA.task_id,
      agent_id: agent.agent_id,
      kind: "clarification",
      status: "open",
      urgency: "normal",
      subject: "Уточнение в первом проекте",
      detail: "Подтвердите параметры",
      revision: 1,
      created_at: now - 8_000,
      updated_at: now - 8_000,
    };
    const cardB: InboxCardRecord = {
      card_id: "89000000-0000-4000-8000-000000000602",
      project_id: projectB.project_id,
      session_id: sessionB.session_id,
      agent_id: agent.agent_id,
      kind: "clarification",
      status: "open",
      urgency: "normal",
      subject: "Уточнение во втором проекте",
      detail: "Укажите окружение",
      revision: 1,
      created_at: now - 5_000,
      updated_at: now - 5_000,
    };

    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [project, projectB],
      agents: [agent],
      sessions: [session, sessionB],
      tasks: [taskA],
      inboxCards: [cardA, cardB],
    });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);
    await screen.findByTestId("chat-card");
    expect(await screen.findByTestId("rail-inbox-badge")).toHaveTextContent("2");

    // Archive active project A -> selected session switches to project B, badge drops from 2 to 1!
    await user.click(screen.getByTestId(`project-menu-trigger-${project.project_id}`));
    await user.click(await screen.findByTestId(`project-menu-archive-${project.project_id}`));

    await waitFor(() => {
      expect(screen.getByTestId("rail-inbox-badge")).toHaveTextContent("1");
    });

    // Tasks view must hide archived project A's task cleanly
    await user.click(screen.getByTestId("rail-nav-tasks"));
    await screen.findByTestId("tasks-card");
    expect(screen.queryByTestId(`task-card-${taskA.task_id}`)).not.toBeInTheDocument();

    // Inbox view must hide archived project A's card and only show project B's card
    await user.click(screen.getByTestId("rail-nav-inbox"));
    await screen.findByTestId("inbox-radar-card");
    expect(screen.queryByTestId(`inbox-card-${cardA.card_id}`)).not.toBeInTheDocument();
    expect(screen.getByTestId(`inbox-card-${cardB.card_id}`)).toBeInTheDocument();

    // Switch back to Чаты (since Sidebar auto-hides on Inbox/Tasks) and remove project B with confirmation -> 0 orphaned inbox cards!
    await user.click(screen.getByTestId("rail-nav-chat"));
    await user.click(screen.getByTestId(`project-menu-trigger-${projectB.project_id}`));
    await user.click(await screen.findByTestId(`project-menu-remove-${projectB.project_id}`));
    await user.click(await screen.findByTestId("project-delete-confirm-btn"));
    await waitFor(() => {
      expect(screen.queryByTestId("rail-inbox-badge")).not.toBeInTheDocument();
    });
    expect(screen.queryByTestId(`inbox-card-${cardB.card_id}`)).not.toBeInTheDocument();
  });

  it("H2-04: TasksCard pre-fills expected_signal_name and sends matching signal name cleanly", async () => {
    const user = userEvent.setup();
    const { now, project, agent, session } = makeD2Fixture();

    const sigTask: TaskRecord = {
      task_id: "75000000-0000-4000-8000-000000000701",
      project_id: project.project_id,
      session_id: session.session_id,
      agent_id: agent.agent_id,
      title: "Дождаться зелёного CI",
      goal: "Выкатить релиз после сигнала ci_green",
      generation: 1,
      status: "waiting_signal",
      expected_signal_name: "ci_green",
      wait_detail: "Ожидает сигнал `ci_green`",
      schedule: null,
      budget: {},
      usage: { turns_used: 0, tokens_used: 0, wall_ms_used: 0, confidence: "provider_reported" },
      attached_skills: [],
      created_at: now - 5_000,
      updated_at: now - 5_000,
    };

    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [project],
      agents: [agent],
      sessions: [session],
      tasks: [sigTask],
    });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);
    await screen.findByTestId("chat-card");

    await user.click(screen.getByTestId("rail-nav-tasks"));
    const card = await screen.findByTestId(`task-card-${sigTask.task_id}`);
    // H2-08: raw backticks stripped from plain-text wait_detail
    expect(card).toHaveTextContent("Ожидает сигнал ci_green");
    expect(card).not.toHaveTextContent("`ci_green`");

    await user.click(screen.getByTestId(`task-signal-open-${sigTask.task_id}`));
    const nameInput = await screen.findByTestId(`task-signal-name-input-${sigTask.task_id}`);
    expect(nameInput).toHaveValue("ci_green");

    const payloadInput = screen.getByTestId(`task-signal-payload-input-${sigTask.task_id}`);
    await user.type(payloadInput, "build #42 passed");
    await user.click(screen.getByTestId(`task-signal-submit-${sigTask.task_id}`));

    await waitFor(() => {
      const sigCall = bridge.calls.find((c) => c.method === "sendTaskSignal");
      expect(sigCall).toBeTruthy();
      expect((sigCall?.args as { request: { signal_name: string } }).request.signal_name).toBe(
        "ci_green",
      );
      expect(screen.getByTestId("task-status-pill-running")).toBeInTheDocument();
    });
  });

  it("H2-07: removing a multi-revision artifact hides all revisions and clears stale preview cleanly", async () => {
    const user = userEvent.setup();
    const { now, project, agent, session } = makeD2Fixture();

    const rev1 = {
      artifact_id: "70000000-0000-4000-8000-000000000801",
      session_id: session.session_id,
      project_id: project.project_id,
      run_id: null,
      name: "release-notes.md",
      mime_type: "text/markdown",
      revision: 1,
      content: "Черновик v1",
      relative_path: null,
      sha256: "a".repeat(64),
      completeness_status: "partial" as const,
      lifecycle_state: "active" as const,
      created_at: now - 20_000,
    };
    const rev2 = {
      ...rev1,
      artifact_id: "70000000-0000-4000-8000-000000000802",
      revision: 2,
      content: "Финальная версия v2",
      completeness_status: "final" as const,
      created_at: now - 10_000,
    };

    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [project],
      agents: [agent],
      sessions: [session],
      artifactsBySession: { [session.session_id]: [rev1, rev2] },
    });

    render(
      <App
        bridge={bridge}
        windowControls={createTestWindowControls()}
        initialRightPanelOpen={true}
      />,
    );
    await screen.findByTestId("chat-card");

    // Latest revision v2 is auto-selected in WorkspacePanel
    expect(await screen.findByTestId("artifact-preview")).toHaveTextContent("Финальная версия v2");

    // Remove logical artifact via lifecycle menu -> BOTH revisions disappear and preview clears completely!
    await user.click(screen.getByTestId("artifact-lifecycle-trigger"));
    expect(await screen.findByTestId("artifact-lifecycle-menu")).toBeInTheDocument();
    await user.click(screen.getByTestId("artifact-action-remove"));
    await waitFor(() => {
      expect(screen.queryByTestId("artifact-preview")).not.toBeInTheDocument();
    });
    expect(screen.getByText("Артефактов в этой сессии пока нет.")).toBeInTheDocument();
  });

  it("H2-08: Escape stops running chat from Inbox/Tasks views, Activity Radar shows active chat runs with Stop, and UNKNOWN cards avoid duplicate text", async () => {
    const user = userEvent.setup();
    const { now, project, agent, session } = makeD2Fixture();

    const unknownEffect: EffectClaimRecord = {
      effect_id: "88000000-0000-4000-8000-000000000901",
      project_id: project.project_id,
      session_id: session.session_id,
      task_id: null,
      run_id: null,
      agent_id: agent.agent_id,
      effect_idempotency_key: "eff-h208-1",
      connector_id: "simulated_external",
      operation: "send_webhook",
      target_recipient: "ops@example.com",
      parameters_json: "{}",
      parameters_sha256: "d".repeat(64),
      status: "unknown",
      error_summary: "Lost ack",
      correlation_id: "corr-h208-1",
      created_at: now - 5_000,
      updated_at: now - 5_000,
    };
    const unknownCard: InboxCardRecord = {
      card_id: "89000000-0000-4000-8000-000000000901",
      project_id: project.project_id,
      session_id: session.session_id,
      agent_id: agent.agent_id,
      kind: "unknown_effect",
      status: "open",
      urgency: "critical",
      subject: "Неизвестный исход `send_webhook`",
      detail:
        "Запрос отправлен во внешний сервис, но подтверждение потеряно. Слепой повтор заблокирован.",
      effect_id: unknownEffect.effect_id,
      revision: 1,
      created_at: now - 5_000,
      updated_at: now - 5_000,
    };

    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [project],
      agents: [agent],
      sessions: [session],
      effects: [unknownEffect],
      inboxCards: [unknownCard],
      sendDelayMs: 5_000,
    });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);
    const textarea = await screen.findByLabelText("Поле ввода сообщения");
    await user.type(textarea, "Длительный анализ в чате{Enter}");
    expect(await screen.findByTestId("running-control-bar")).toBeInTheDocument();

    // Switch to Inbox while chat run is active:
    // 1. Unknown effect card has NO duplicate detail text and NO raw backticks!
    await user.click(screen.getByTestId("rail-nav-inbox"));
    const uCard = await screen.findByTestId(`inbox-card-${unknownCard.card_id}`);
    expect(uCard).toHaveTextContent("Неизвестный исход send_webhook");
    expect(uCard).not.toHaveTextContent("`send_webhook`");
    expect(within(uCard).getAllByText(/подтверждение потеряно/i)).toHaveLength(1);

    // 2. Activity Radar shows the active chat run with a working Stop button!
    const radarSessionRow = await screen.findByTestId(
      `radar-running-session-${session.session_id}`,
    );
    expect(radarSessionRow).toHaveTextContent("Выполняется");
    expect(
      within(radarSessionRow).getByTestId(`radar-stop-session-${session.session_id}`),
    ).toBeInTheDocument();

    // 3. Pressing Escape on Поручения view cancels the active chat run cleanly!
    await user.click(screen.getByTestId("rail-nav-tasks"));
    await screen.findByTestId("tasks-card");
    await user.keyboard("{Escape}");
    await waitFor(() => {
      expect(bridge.calls.filter((c) => c.method === "cancelRun")).toHaveLength(1);
    });
  });
});
