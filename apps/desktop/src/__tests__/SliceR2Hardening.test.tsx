import { readFileSync } from "node:fs";
import path from "node:path";

import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "../App";
import type { WindowControls } from "../api/bridge";
import type {
  AgentRecord,
  ArtifactRecord,
  EffectClaimRecord,
  InboxCardRecord,
  MessageRecord,
  PermissionRequestRecord,
  ProjectRecord,
  SessionRecord,
  TaskRecord,
} from "../api/contracts";
import { MockDesktopBridge } from "../api/mockBridge";
import { formatDecisionReasonRu, formatHumanCountdown } from "../utils/formatters";
import {
  isLoopbackHostHeader,
  isLoopbackOriginHeader,
  validateDevDirsRequest,
} from "../../devPickerGuard";

function createTestWindowControls() {
  return {
    supported: true,
    startDragging: vi.fn(async () => {}),
    minimize: vi.fn(async () => {}),
    toggleMaximize: vi.fn(async () => {}),
    close: vi.fn(async () => {}),
    isMaximized: vi.fn(async () => false),
  };
}

function makeMultiProjectFixture() {
  const now = Date.now();
  const projectA: ProjectRecord = {
    project_id: "10000000-0000-4000-8000-00000000r201",
    name: "Проект Альфа",
    workspace_path: "A:/Dev/alpha",
    memory_scope: "project:alpha",
    lifecycle_state: "active",
    archived_at: null,
    pinned: false,
    pinned_at: null,
    created_at: now - 120_000,
  };
  const projectB: ProjectRecord = {
    project_id: "10000000-0000-4000-8000-00000000r202",
    name: "Проект Бета",
    workspace_path: "A:/Dev/beta",
    memory_scope: "project:beta",
    lifecycle_state: "active",
    archived_at: null,
    pinned: false,
    pinned_at: null,
    created_at: now - 90_000,
  };
  const projectC: ProjectRecord = {
    project_id: "10000000-0000-4000-8000-00000000r203",
    name: "Проект Гамма",
    workspace_path: null,
    memory_scope: "project:gamma",
    lifecycle_state: "active",
    archived_at: null,
    pinned: false,
    pinned_at: null,
    created_at: now - 60_000,
  };

  const agentA: AgentRecord = {
    agent_id: "20000000-0000-4000-8000-00000000r201",
    name: "Агент Альфа",
    description: "Основной агент",
    current_config_version_id: "21000000-0000-4000-8000-00000000r201",
    current_config: {
      config_version_id: "21000000-0000-4000-8000-00000000r201",
      agent_id: "20000000-0000-4000-8000-00000000r201",
      version_number: 1,
      system_prompt: "Ты агент Dennett.",
      model_preference: "gemini-4-neon",
      thinking_level: "high",
      allowed_memory_scopes: ["project:alpha"],
      workspace_write_policy: { mode: "ask_before_write" },
      created_at: now - 110_000,
    },
    created_at: now - 110_000,
    updated_at: now - 110_000,
  };
  const agentB: AgentRecord = {
    ...agentA,
    agent_id: "20000000-0000-4000-8000-00000000r202",
    name: "Агент Бета",
    current_config: {
      ...agentA.current_config,
      allowed_memory_scopes: ["project:beta"],
    },
  };

  const sessionA: SessionRecord = {
    session_id: "30000000-0000-4000-8000-00000000r201",
    agent_id: agentA.agent_id,
    project_id: projectA.project_id,
    title: "Чат Альфа",
    session_kind: "user_chat",
    status: "active",
    pinned: false,
    pinned_at: null,
    created_at: now - 100_000,
    updated_at: now - 5_000,
  };
  const sessionB: SessionRecord = {
    session_id: "30000000-0000-4000-8000-00000000r202",
    agent_id: agentB.agent_id,
    project_id: projectB.project_id,
    title: "Чат Бета",
    session_kind: "user_chat",
    status: "active",
    pinned: false,
    pinned_at: null,
    created_at: now - 80_000,
    updated_at: now - 10_000,
  };

  return { now, projectA, projectB, projectC, agentA, agentB, sessionA, sessionB };
}

describe("Slice R2 Hardening & UX Refinement", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("D2-SEC-03: enforces loopback Host, same-origin Sec-Fetch-Site, and X-Dennett-Dev-Picker header on /__dennett_dev_dirs and sends header from DevFolderPickerModal", async () => {
    // 1. Direct guard unit checks on Vite middleware validators
    expect(isLoopbackHostHeader("127.0.0.1:1420")).toBe(true);
    expect(isLoopbackHostHeader("localhost:1420")).toBe(true);
    expect(isLoopbackHostHeader("[::1]:1420")).toBe(true);
    expect(isLoopbackHostHeader("evil.example:1420")).toBe(false);
    expect(isLoopbackHostHeader("127.0.0.1.evil.example:1420")).toBe(false);
    expect(isLoopbackHostHeader("192.168.1.20:1420")).toBe(false);
    expect(isLoopbackHostHeader(undefined)).toBe(false);

    expect(isLoopbackOriginHeader("http://127.0.0.1:1420")).toBe(true);
    expect(isLoopbackOriginHeader("http://localhost:1420")).toBe(true);
    expect(isLoopbackOriginHeader("https://evil.example")).toBe(false);

    expect(
      validateDevDirsRequest("GET", {
        host: "127.0.0.1:1420",
        "sec-fetch-site": "same-origin",
        "x-dennett-dev-picker": "1",
        origin: "http://127.0.0.1:1420",
      }),
    ).toEqual({ ok: true });
    expect(
      validateDevDirsRequest("POST", {
        host: "127.0.0.1:1420",
        "sec-fetch-site": "same-origin",
        "x-dennett-dev-picker": "1",
      }),
    ).toMatchObject({ ok: false, status: 405 });
    expect(
      validateDevDirsRequest("GET", {
        host: "evil.example",
        "sec-fetch-site": "same-origin",
        "x-dennett-dev-picker": "1",
      }),
    ).toMatchObject({ ok: false, status: 403 });
    expect(
      validateDevDirsRequest("GET", {
        host: "127.0.0.1:1420",
        "sec-fetch-site": "cross-site",
        "x-dennett-dev-picker": "1",
      }),
    ).toMatchObject({ ok: false, status: 403 });
    expect(
      validateDevDirsRequest("GET", {
        host: "127.0.0.1:1420",
        "sec-fetch-site": "same-origin",
      }),
    ).toMatchObject({ ok: false, status: 403 });
    expect(
      validateDevDirsRequest("GET", {
        host: "127.0.0.1:1420",
        "sec-fetch-site": "same-origin",
        "x-dennett-dev-picker": "1",
        origin: "https://attacker.tld",
      }),
    ).toMatchObject({ ok: false, status: 403 });

    // 2. Verify DevFolderPickerModal sends X-Dennett-Dev-Picker: 1 header
    const fetchMock = vi.fn(async () => ({
      ok: true,
      status: 200,
      json: async () => ({
        current: "A:/Dev/workspace",
        parent: "A:/Dev",
        dirs: [{ name: "sub", path: "A:/Dev/workspace/sub" }],
      }),
    }));
    vi.stubGlobal("fetch", fetchMock);

    const user = userEvent.setup();
    const bridge = new MockDesktopBridge({
      connected: true,
      supportsNativeFolderPicker: false,
    });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);
    await user.click(await screen.findByTestId("onboarding-pick-folder-btn"));
    expect(await screen.findByTestId("dev-folder-picker-modal")).toBeInTheDocument();

    await waitFor(() => {
      expect(fetchMock).toHaveBeenCalled();
    });
    const firstCall = fetchMock.mock.calls[0] as unknown as [string, RequestInit | undefined];
    expect(firstCall[0]).toContain("/__dennett_dev_dirs");
    const headers = firstCall[1]?.headers as Record<string, string> | undefined;
    expect(headers?.["X-Dennett-Dev-Picker"]).toBe("1");
  });

  it("Owner Directives 1-3: removes manual TasksCard form in favor of '+' chat draft, auto-hides Sidebar on Inbox/Tasks with full ProjectScopeFilter, and cleans ActivityRail", async () => {
    const user = userEvent.setup();
    const { now, projectA, projectB, projectC, agentA, agentB, sessionA, sessionB } =
      makeMultiProjectFixture();

    const taskA: TaskRecord = {
      task_id: "75000000-0000-4000-8000-00000000r201",
      project_id: projectA.project_id,
      session_id: sessionA.session_id,
      agent_id: agentA.agent_id,
      title: "Отчёт Альфа",
      goal: "Отчёт Альфа",
      generation: 1,
      status: "waiting_timer",
      schedule: {
        timezone: "UTC",
        original_expression: "через 2 часа 10 минут",
        kind: { kind: "once_at", fire_at: now + 7_800_000 },
        missed_run_policy: "execute_once",
        next_fire_at: now + 7_800_000,
      },
      budget: {},
      usage: { turns_used: 0, tokens_used: 0, wall_ms_used: 0, confidence: "provider_reported" },
      attached_skills: [],
      created_at: now - 5_000,
      updated_at: now - 1_000,
    };
    const taskB: TaskRecord = {
      ...taskA,
      task_id: "75000000-0000-4000-8000-00000000r202",
      project_id: projectB.project_id,
      session_id: sessionB.session_id,
      agent_id: agentB.agent_id,
      title: "Деплой Бета",
      goal: "Деплой Бета",
      status: "running",
      schedule: null,
    };
    const cardA: InboxCardRecord = {
      card_id: "89000000-0000-4000-8000-00000000r201",
      project_id: projectA.project_id,
      session_id: sessionA.session_id,
      agent_id: agentA.agent_id,
      kind: "clarification",
      status: "open",
      urgency: "normal",
      subject: "Вопрос по Альфа",
      detail: "Уточните формат",
      revision: 1,
      created_at: now - 3_000,
      updated_at: now - 3_000,
    };
    const cardB: InboxCardRecord = {
      ...cardA,
      card_id: "89000000-0000-4000-8000-00000000r202",
      project_id: projectB.project_id,
      session_id: sessionB.session_id,
      agent_id: agentB.agent_id,
      subject: "Вопрос по Бета",
    };

    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [projectA, projectB, projectC],
      agents: [agentA, agentB],
      sessions: [sessionA, sessionB],
      tasks: [taskA, taskB],
      inboxCards: [cardA, cardB],
    });

    const { container } = render(
      <App bridge={bridge} windowControls={createTestWindowControls()} />,
    );
    await screen.findByTestId("chat-card");

    // Directive 3: ActivityRail has ZERO duplicate Workspace or dead Plug buttons; top-right Workspace toggle works
    const rail = screen.getByTestId("activity-rail");
    expect(within(rail).queryByTestId("rail-toggle-workspace")).not.toBeInTheDocument();
    expect(within(rail).queryByLabelText(/Интеграции/i)).not.toBeInTheDocument();
    expect(screen.getByTestId("titlebar-toggle-workspace")).toBeInTheDocument();

    const shell = screen.getByTestId("desktop-app-shell");
    expect(shell.className).not.toContain("sidebar-collapsed");

    // Directive 2: Switching to Входящие auto-hides Sidebar and exposes multi-project ProjectScopeFilter
    await user.click(screen.getByTestId("rail-nav-inbox"));
    await screen.findByTestId("inbox-radar-card");
    expect(shell.className).toContain("sidebar-collapsed");
    expect(screen.getByTestId("user-settings-bar")).toBeInTheDocument();
    expect(container.querySelectorAll("select")).toHaveLength(0);

    await user.click(screen.getByTestId("project-scope-filter-trigger"));
    const inboxFilterMenu = await screen.findByTestId("project-scope-filter-menu");
    expect(inboxFilterMenu).toHaveTextContent("Все проекты");
    expect(inboxFilterMenu).toHaveTextContent("Проект Альфа");
    expect(inboxFilterMenu).toHaveTextContent("Проект Бета");
    expect(inboxFilterMenu).toHaveTextContent("Проект Гамма");
    await user.click(within(inboxFilterMenu).getByTestId(`project-scope-option-${projectB.project_id}`));
    expect(screen.queryByTestId(`inbox-card-${cardA.card_id}`)).not.toBeInTheDocument();
    expect(screen.getByTestId(`inbox-card-${cardB.card_id}`)).toBeInTheDocument();

    // Directive 1 & 2: Поручения keeps Sidebar auto-hidden, shows multi-project filter, NO manual creation form, human countdown, and "+" opens prefilled chat
    await user.click(screen.getByTestId("rail-nav-tasks"));
    const tasksCard = await screen.findByTestId("tasks-card");
    expect(shell.className).toContain("sidebar-collapsed");
    expect(screen.queryByTestId("task-goal-input")).not.toBeInTheDocument();
    expect(screen.queryByTestId("task-schedule-trigger")).not.toBeInTheDocument();
    expect(within(tasksCard).getByTestId("task-status-pill-waiting_timer")).toHaveTextContent(
      /через 2ч (9|10)м/,
    );

    await user.click(screen.getByTestId("project-scope-filter-trigger"));
    const tasksFilterMenu = await screen.findByTestId("project-scope-filter-menu");
    await user.click(within(tasksFilterMenu).getByTestId(`project-scope-option-${projectA.project_id}`));
    expect(screen.getByTestId(`task-card-${taskA.task_id}`)).toBeInTheDocument();
    expect(screen.queryByTestId(`task-card-${taskB.task_id}`)).not.toBeInTheDocument();

    const plusBtn = screen.getByTestId("task-create-plus-btn");
    expect(plusBtn).toHaveAttribute("title", "Создать событие");
    await user.click(plusBtn);

    // Returned to Чаты -> Sidebar restored, blank user_chat created, composer prefilled with cursor at end!
    const composer = (await screen.findByLabelText("Поле ввода сообщения")) as HTMLTextAreaElement;
    expect(shell.className).not.toContain("sidebar-collapsed");
    expect(composer.value).toBe("Создай событие: мне необходимо ");
    await waitFor(() => {
      expect(composer.selectionStart).toBe("Создай событие: мне необходимо ".length);
      expect(composer.selectionEnd).toBe("Создай событие: мне необходимо ".length);
    });
  });

  it("D2-ARCH-01: derives active session running state from server running_session_ids and allows Stop without local lockout", async () => {
    const user = userEvent.setup();
    const { projectA, agentA, sessionA } = makeMultiProjectFixture();
    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [projectA],
      agents: [agentA],
      sessions: [sessionA],
      runningSessionIds: [sessionA.session_id],
    });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);
    expect(await screen.findByTestId("running-control-bar")).toBeInTheDocument();
    expect(screen.getByTestId("composer-stop-button")).toBeInTheDocument();

    await user.click(screen.getByTestId("composer-stop-button"));
    await waitFor(() => {
      expect(bridge.calls.filter((c) => c.method === "cancelRun")).toHaveLength(1);
      expect(screen.queryByTestId("running-control-bar")).not.toBeInTheDocument();
    });
  });

  it("D2-OVER-03 + D2-BUG-06: recovers from cold-start offline with 0 sessions, skips terminal 404 queued item without wedging queue, and deduplicates strictly by client_message_id", async () => {
    const user = userEvent.setup();
    const { now, projectA, agentA, sessionA } = makeMultiProjectFixture();

    // 1. Cold-start offline (connected: false, sessions: []) -> retry reconnect succeeds!
    const coldBridge = new MockDesktopBridge({
      connected: false,
      projects: [projectA],
      agents: [agentA],
      sessions: [sessionA],
    });
    render(<App bridge={coldBridge} windowControls={createTestWindowControls()} />);
    expect(await screen.findByTestId("head-offline-state")).toBeInTheDocument();
    coldBridge.connected = true;
    await user.click(screen.getByTestId("retry-head-button"));
    expect(await screen.findByTestId("chat-composer")).toBeInTheDocument();
    cleanup();

    // 2. Offline queue with a dead session (404/NOT_FOUND) + a valid session -> skips dead item without wedging
    window.localStorage.setItem(
      "dennett.desktop.d2.drafts.v1",
      JSON.stringify({
        draftsBySession: {},
        queuedMessages: [
          {
            client_message_id: "cid-dead-session",
            session_id: "30000000-0000-4000-8000-00000000dead",
            content: "В удалённую сессию",
            created_at: now - 2_000,
          },
          {
            client_message_id: "cid-live-session",
            session_id: sessionA.session_id,
            content: "В живую сессию",
            created_at: now - 1_000,
          },
        ],
      }),
    );

    const queueBridge = new MockDesktopBridge({
      connected: true,
      projects: [projectA],
      agents: [agentA],
      sessions: [sessionA],
    });

    render(<App bridge={queueBridge} windowControls={createTestWindowControls()} />);
    // Must NOT auto-flush on mount!
    const flushBtn = await screen.findByTestId("flush-offline-queue-btn");
    expect(queueBridge.calls.filter((c) => c.method === "sendMessage")).toHaveLength(0);

    await user.click(flushBtn);
    await waitFor(() => {
      expect(screen.queryByTestId("flush-offline-queue-btn")).not.toBeInTheDocument();
    });
    expect(await screen.findByText(/Принято .* В живую сессию/)).toBeInTheDocument();
    expect(screen.getByTestId("error-banner")).toHaveTextContent(/пропущено|NOT_FOUND/i);
    cleanup();

    // 3. Strict client_message_id dedup allows two distinct user messages with identical text ("ок")
    const dupMsg1: MessageRecord = {
      message_id: "50000000-0000-4000-8000-00000000r201",
      session_id: sessionA.session_id,
      run_id: null,
      role: "user",
      content: "ок",
      client_message_id: "cid-ok-1",
      correlation_id: "corr-1",
      created_at: now - 500,
    };
    const dedupBridge = new MockDesktopBridge({
      connected: true,
      projects: [projectA],
      agents: [agentA],
      sessions: [sessionA],
      messagesBySession: { [sessionA.session_id]: [dupMsg1] },
      sendDelayMs: 4_000,
    });
    render(<App bridge={dedupBridge} windowControls={createTestWindowControls()} />);
    const composer = await screen.findByLabelText("Поле ввода сообщения");
    await user.type(composer, "ок{Enter}");
    await waitFor(() => {
      expect(screen.getAllByTestId("message-user")).toHaveLength(2);
    });
  });

  it("D2-UX-01 + D2-UX-02: TitleBar double-click maximizes, search click never drags, single SIMULATED pill, PortalPopover mutual exclusion + capture Escape, and CSS hardening guards", async () => {
    const user = userEvent.setup();
    const { projectA, agentA, sessionA } = makeMultiProjectFixture();
    const windowControls: WindowControls = createTestWindowControls();
    const bridge = new MockDesktopBridge({
      connected: true,
      runtimeIsSimulated: true,
      projects: [projectA],
      agents: [agentA],
      sessions: [sessionA],
      runningSessionIds: [sessionA.session_id],
    });

    render(<App bridge={bridge} windowControls={windowControls} />);
    const titlebar = await screen.findByTestId("desktop-titlebar");

    // 1. Single SIMULATED pill (no separate duplicate .simulated-pill)
    expect(screen.getAllByTestId("simulated-runtime-badge")).toHaveLength(1);
    expect(document.querySelectorAll(".simulated-pill")).toHaveLength(0);

    // 2. Clicking command search focuses input and NEVER calls startDragging
    const searchInput = screen.getByLabelText("Поиск по чатам и проектам");
    fireEvent.mouseDown(searchInput, { button: 0, detail: 1 });
    expect(windowControls.startDragging).not.toHaveBeenCalled();

    // 3. Double-clicking titlebar empty area calls toggleMaximize (not startDragging)
    fireEvent.mouseDown(titlebar, { button: 0, detail: 2 });
    expect(windowControls.toggleMaximize).toHaveBeenCalledTimes(1);
    expect(windowControls.startDragging).not.toHaveBeenCalled();

    // 4. PortalPopover mounts in document.body, mutually excludes sibling popovers, and capture-phase Escape closes popover without cancelling active run
    await user.click(screen.getByTestId("model-selector-trigger"));
    const modelPopover = await screen.findByTestId("model-popover");
    expect(modelPopover.parentElement).toBe(document.body);

    await user.click(screen.getByTestId("thinking-selector-trigger"));
    expect(await screen.findByTestId("thinking-popover")).toBeInTheDocument();
    expect(screen.queryByTestId("model-popover")).not.toBeInTheDocument();

    await user.keyboard("{Escape}");
    expect(screen.queryByTestId("thinking-popover")).not.toBeInTheDocument();
    expect(bridge.calls.filter((c) => c.method === "cancelRun")).toHaveLength(0);

    // 5. CSS hardening guards (Mica fallback, reduced transparency, workspace density, dark stop hover, dark scrollbars, radar ellipsis, 0 backdrop-filter)
    const tokensCss = readFileSync(path.resolve(__dirname, "../styles/tokens.css"), "utf-8");
    const desktopCss = readFileSync(path.resolve(__dirname, "../styles/desktop.css"), "utf-8");
    expect(tokensCss).toContain("html.native-shell.native-mica-unavailable");
    expect(tokensCss).toContain("prefers-reduced-transparency: reduce");
    expect(tokensCss).toContain("rgba(38, 38, 38, 0.50)");
    expect(desktopCss).toContain(".send-button.stop-button:hover:not(:disabled)");
    expect(desktopCss).toContain("::-webkit-scrollbar-button");
    expect(desktopCss).toContain(".d2-radar-row__copy strong");
    expect(desktopCss).not.toMatch(/backdrop-filter\s*:/i);
  });

  it("D2-UX-03: supports independent project selection, hides task_run sessions in Sidebar until opened, read-only archived inspection, and clickable breadcrumb rename", async () => {
    const user = userEvent.setup();
    const { now, projectA, projectB, agentA, agentB, sessionA, sessionB } =
      makeMultiProjectFixture();

    const archivedProject: ProjectRecord = {
      project_id: "10000000-0000-4000-8000-00000000r299",
      name: "Архив Легаси",
      workspace_path: null,
      memory_scope: "project:legacy",
      lifecycle_state: "archived",
      archived_at: now - 50_000,
      pinned: false,
      pinned_at: null,
      created_at: now - 200_000,
    };
    const archivedSession: SessionRecord = {
      session_id: "30000000-0000-4000-8000-00000000r299",
      agent_id: agentA.agent_id,
      project_id: archivedProject.project_id,
      title: "Старый чат",
      session_kind: "user_chat",
      status: "active",
      pinned: false,
      pinned_at: null,
      created_at: now - 150_000,
      updated_at: now - 60_000,
    };
    const taskRunSession: SessionRecord = {
      session_id: "30000000-0000-4000-8000-00000000r288",
      agent_id: agentA.agent_id,
      project_id: projectA.project_id,
      title: "Фоновый запуск таймера",
      session_kind: "task_run",
      status: "active",
      pinned: false,
      pinned_at: null,
      created_at: now - 20_000,
      updated_at: now - 2_000,
    };
    const bgTask: TaskRecord = {
      task_id: "75000000-0000-4000-8000-00000000r288",
      project_id: projectA.project_id,
      session_id: taskRunSession.session_id,
      agent_id: agentA.agent_id,
      title: "Ночной бэкап",
      goal: "Сделать ночной бэкап",
      generation: 1,
      status: "succeeded",
      schedule: null,
      budget: {},
      usage: { turns_used: 1, tokens_used: 40, wall_ms_used: 300, confidence: "provider_reported" },
      attached_skills: [],
      created_at: now - 20_000,
      updated_at: now - 2_000,
    };

    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [projectA, projectB, archivedProject],
      agents: [agentA, agentB],
      sessions: [sessionA, sessionB, archivedSession, taskRunSession],
      tasks: [bgTask],
    });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);
    await screen.findByTestId("chat-card");

    // 1. Background task_run session is hidden from user chat list by default
    expect(screen.queryByTestId(`session-item-${taskRunSession.session_id}`)).not.toBeInTheDocument();

    // 2. Clicking project heading selects Project B independently
    await user.click(screen.getByTestId(`project-heading-${projectB.project_id}`));
    expect(screen.getByTestId(`project-heading-${projectB.project_id}`).className).toContain(
      "is-selected-project",
    );

    // 3. Breadcrumb inline rename updates session title cleanly
    await user.click(screen.getByTestId("breadcrumb-session-title"));
    const renameInput = await screen.findByTestId("breadcrumb-rename-input");
    await user.clear(renameInput);
    await user.type(renameInput, "Новое имя из шапки{Enter}");
    await waitFor(() => {
      expect(screen.getByTestId("breadcrumb-session-title")).toHaveTextContent(
        "Новое имя из шапки",
      );
    });

    // 4. Opening task_run session from Поручения renders read-only banner + Фон badge in sidebar
    await user.click(screen.getByTestId("rail-nav-tasks"));
    await user.click(await screen.findByTestId(`task-open-chat-${bgTask.task_id}`));
    expect(await screen.findByTestId("composer-task-run-banner")).toBeInTheDocument();
    expect(screen.getByTestId(`session-item-${taskRunSession.session_id}`)).toHaveTextContent("Фон");

    // 5. Inspecting archived project opens read-only archived banner
    await user.click(screen.getByTestId("sidebar-archive-toggle"));
    await user.click(screen.getByTestId(`archived-project-${archivedProject.project_id}`));
    expect(await screen.findByTestId("composer-archived-banner")).toBeInTheDocument();
  });

  it("D2-UX-04: Right Workspace panel has collapsible files & folder tree, archived/deleted buckets, delete confirmation, zero empty ⋯ menu on tombstone, SHA-256 + Copy + safe Markdown preview", async () => {
    const user = userEvent.setup();
    const { now, projectA, agentA, sessionA } = makeMultiProjectFixture();

    const activeArt: ArtifactRecord = {
      artifact_id: "70000000-0000-4000-8000-00000000r201",
      session_id: sessionA.session_id,
      project_id: projectA.project_id,
      run_id: null,
      name: "summary.md",
      mime_type: "text/markdown",
      revision: 1,
      content: "# Итоги релиза\n\n**Готово** <script>window.__xss=1</script>",
      relative_path: null,
      sha256: "abcdef1234567890".repeat(4),
      completeness_status: "final",
      lifecycle_state: "active",
      created_at: now - 10_000,
    };
    const archivedArt: ArtifactRecord = {
      ...activeArt,
      artifact_id: "70000000-0000-4000-8000-00000000r202",
      name: "old-notes.md",
      content: "Старый архив",
      lifecycle_state: "archived",
    };

    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [projectA],
      agents: [agentA],
      sessions: [sessionA],
      artifactsBySession: { [sessionA.session_id]: [activeArt, archivedArt] },
      filesByProject: {
        [projectA.project_id]: [
          { relative_path: "src", name: "src", kind: "directory", size_bytes: null, depth: 1 },
          { relative_path: "src/main.rs", name: "main.rs", kind: "file", size_bytes: 1024, depth: 2 },
          { relative_path: "README.md", name: "README.md", kind: "file", size_bytes: 512, depth: 1 },
        ],
      },
    });

    render(
      <App
        bridge={bridge}
        windowControls={createTestWindowControls()}
        initialRightPanelOpen={true}
      />,
    );

    // 1. Folder tree collapses nested children by default and expands on click
    const fileList = await screen.findByTestId("workspace-file-list");
    expect(fileList).toHaveTextContent("src");
    expect(fileList).not.toHaveTextContent("main.rs");
    await user.click(screen.getByTestId("workspace-dir-src"));
    expect(fileList).toHaveTextContent("main.rs");

    // Collapsible Файлы проекта section toggle hides file tree cleanly
    await user.click(screen.getByTestId("workspace-files-toggle"));
    expect(screen.queryByTestId("workspace-file-list")).not.toBeInTheDocument();

    // 2. Archived artifact lives inside collapsible Архив (1) block
    expect(screen.getByTestId("workspace-archived-artifacts-toggle")).toHaveTextContent("Архив (1)");
    expect(screen.queryByTestId(`artifact-item-${archivedArt.artifact_id}`)).not.toBeInTheDocument();
    await user.click(screen.getByTestId("workspace-archived-artifacts-toggle"));
    expect(screen.getByTestId(`artifact-item-${archivedArt.artifact_id}`)).toBeInTheDocument();

    // 3. Preview shows SHA-256, Copy buttons, and safe rendered Markdown (0 <script> DOM elements)
    expect(await screen.findByTestId("artifact-sha256")).toHaveTextContent("abcdef123456");
    expect(screen.getByTestId("artifact-copy-sha256")).toBeInTheDocument();
    expect(screen.getByTestId("artifact-copy-content")).toBeInTheDocument();
    const mdBox = screen.getByTestId("artifact-preview-markdown");
    expect(mdBox.querySelector("h1")?.textContent).toBe("Итоги релиза");
    expect(mdBox.querySelector("script")).toBeNull();
    expect((window as unknown as { __xss?: number }).__xss).toBeUndefined();

    await user.click(screen.getByTestId("artifact-mode-source"));
    expect(screen.getByTestId("artifact-preview-source")).toHaveTextContent("# Итоги релиза");

    // 4. Delete requires inline confirmation -> after delete tombstone hides empty ⋯ trigger!
    await user.click(screen.getByTestId("artifact-lifecycle-trigger"));
    await user.click(await screen.findByTestId("artifact-action-delete"));
    expect(await screen.findByTestId("artifact-confirm-bar")).toBeInTheDocument();
    await user.click(screen.getByTestId("artifact-confirm-yes"));

    await waitFor(() => {
      expect(screen.getByTestId("workspace-deleted-artifacts-toggle")).toHaveTextContent("Удалённые (1)");
    });
    await user.click(screen.getByTestId("workspace-deleted-artifacts-toggle"));
    await user.click(screen.getByTestId(`artifact-item-${activeArt.artifact_id}`));
    await waitFor(() => {
      expect(screen.queryByTestId("artifact-lifecycle-trigger")).not.toBeInTheDocument();
    });
  });

  it("D2-UX-05 + D2-UX-06: honest awaiting-approval status with preview_snippet, direct UNKNOWN reconcile verdict banner, inline budget/missed cards, single clarification input, and modal focus trap", async () => {
    const user = userEvent.setup();
    const { now, projectA, agentA, sessionA } = makeMultiProjectFixture();

    // Unit checks on countdown and Russianized reason formatters
    expect(formatHumanCountdown(now + 7_800_000, now)).toBe("через 2ч 10м");
    expect(formatHumanCountdown(now + 45_000, now)).toBe("через 45с");
    expect(formatDecisionReasonRu("workspace_write_policy=ask_before_write")).toContain(
      "Спрашивать запись",
    );

    const pendingPerm: PermissionRequestRecord = {
      request_id: "85000000-0000-4000-8000-00000000r201",
      project_id: projectA.project_id,
      session_id: sessionA.session_id,
      run_id: "40000000-0000-4000-8000-00000000r201",
      agent_id: agentA.agent_id,
      tool_call_id: "tc-r2-01",
      tool_name: "write_file",
      tool_kind: "workspace_write",
      target_summary: "src/config.toml",
      normalized_targets: ["src/config.toml"],
      parameters_sha256: "e".repeat(64),
      preview_snippet: "+ port = 41771\n+ token = [REDACTED]",
      reason: "workspace_write_policy=ask_before_write",
      status: "pending",
      revision: 1,
      expires_at: now + 300_000,
      correlation_id: "corr-perm-r2",
      created_at: now - 1_500,
    };
    const clarifyCard: InboxCardRecord = {
      card_id: "89000000-0000-4000-8000-00000000r210",
      project_id: projectA.project_id,
      session_id: sessionA.session_id,
      agent_id: agentA.agent_id,
      kind: "clarification",
      status: "open",
      urgency: "normal",
      subject: "Уточнение перед запуском",
      detail: "Какую ветку использовать?",
      revision: 1,
      created_at: now - 1_200,
      updated_at: now - 1_200,
    };
    const budgetCard: InboxCardRecord = {
      ...clarifyCard,
      card_id: "89000000-0000-4000-8000-00000000r211",
      kind: "budget_exceeded",
      subject: "Бюджет исчерпан",
      detail: "Budget exceeded: max turns reached",
    };
    const missedCard: InboxCardRecord = {
      ...clarifyCard,
      card_id: "89000000-0000-4000-8000-00000000r212",
      kind: "missed_schedule",
      subject: "Пропущенное окно расписания",
      detail: "Утренний отчёт пропущен",
    };
    const unknownEffect: EffectClaimRecord = {
      effect_id: "88000000-0000-4000-8000-00000000r201",
      project_id: projectA.project_id,
      session_id: sessionA.session_id,
      task_id: "75000000-0000-4000-8000-00000000r210",
      run_id: null,
      agent_id: agentA.agent_id,
      effect_idempotency_key: "eff-r2-01",
      connector_id: "simulated_external",
      operation: "send_webhook",
      target_recipient: "ops@example.com",
      parameters_json: "{}",
      parameters_sha256: "b".repeat(64),
      status: "unknown",
      error_summary: "Lost ack",
      correlation_id: "corr-r2",
      created_at: now - 2_000,
      updated_at: now - 2_000,
    };
    const unknownTask: TaskRecord = {
      task_id: unknownEffect.task_id!,
      project_id: projectA.project_id,
      session_id: sessionA.session_id,
      agent_id: agentA.agent_id,
      title: "Внешняя отправка",
      goal: "Отправить webhook",
      generation: 1,
      status: "unknown_external",
      schedule: null,
      budget: {},
      usage: { turns_used: 1, tokens_used: 30, wall_ms_used: 200, confidence: "provider_reported" },
      attached_skills: [],
      created_at: now - 2_000,
      updated_at: now - 2_000,
    };

    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [projectA],
      agents: [agentA],
      sessions: [sessionA],
      tasks: [unknownTask],
      effects: [unknownEffect],
      permissionRequests: [pendingPerm],
      inboxCards: [clarifyCard, budgetCard, missedCard],
      runningSessionIds: [sessionA.session_id],
    });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);

    // 1. Honest "Ожидает подтверждения" banner (no spinner, no thinking-indicator) + redacted preview_snippet
    const runningBar = await screen.findByTestId("running-control-bar");
    expect(runningBar).toHaveAttribute("data-state", "awaiting-approval");
    expect(runningBar).toHaveTextContent("Ожидает подтверждения");
    expect(runningBar.querySelector(".spin")).toBeNull();
    expect(screen.queryByTestId("thinking-indicator")).not.toBeInTheDocument();
    expect(
      screen.getByTestId(`inline-permission-snippet-${pendingPerm.request_id}`),
    ).toHaveTextContent("[REDACTED]");

    // 2. Inline budget_exceeded and missed_schedule cards render cleanly
    expect(screen.getByTestId(`inline-budget-card-${budgetCard.card_id}`)).toBeInTheDocument();
    expect(
      screen.getByTestId(`inline-missed-schedule-card-${missedCard.card_id}`),
    ).toBeInTheDocument();

    // 3. Single input during waiting_clarification (NO duplicate input inside inline card!)
    const inlineClarify = screen.getByTestId(`inline-clarification-card-${clarifyCard.card_id}`);
    expect(inlineClarify.querySelectorAll("input, textarea")).toHaveLength(0);
    expect(screen.getByTestId("composer-clarification-hint")).toBeInTheDocument();

    const composer = screen.getByLabelText("Ответ на уточняющий вопрос агента");
    await user.type(composer, "Ветка release-v2{Enter}");
    await waitFor(() => {
      const resolveCall = bridge.calls.find(
        (c) =>
          c.method === "resolveInboxCard" &&
          (c.args as { cardId: string }).cardId === clarifyCard.card_id,
      );
      expect(resolveCall).toBeTruthy();
    });

    // 4. Direct UNKNOWN reconcile in TasksCard displays explicit service verdict banner
    await user.click(screen.getByTestId("rail-nav-tasks"));
    await user.click(await screen.findByTestId(`task-reconcile-auto-${unknownTask.task_id}`));
    expect(await screen.findByTestId("reconcile-verdict-banner")).toHaveTextContent(
      /подтвердил|выполнен|Сверка/i,
    );

    // 5. useModalFocusTrap: opening SettingsSheet traps Tab and restores focus to trigger on Escape
    const settingsBtn = screen.getByTestId("settings-trigger");
    settingsBtn.focus();
    await user.click(settingsBtn);
    const settingsSheet = await screen.findByTestId("settings-sheet");
    expect(settingsSheet.contains(document.activeElement)).toBe(true);

    await user.tab();
    expect(settingsSheet.contains(document.activeElement)).toBe(true);

    await user.keyboard("{Escape}");
    await waitFor(() => {
      expect(screen.queryByTestId("settings-sheet")).not.toBeInTheDocument();
    });
    expect(document.activeElement).toBe(settingsBtn);
  });

  it("R2-UI-01..07 regressions: styles snippet head & folder tree override, matches ProjectScopeFilter classes, localizes English MCP schedule chips, fixes task_run empty copy, previews *.md text/plain artifacts, and guards DevFolderPickerModal draft against stale initial load", async () => {
    const user = userEvent.setup();
    const { now, projectA, projectB, agentA, agentB, sessionA, sessionB } =
      makeMultiProjectFixture();

    // R2-UI-01 & R2-UI-02 CSS checks
    const desktopCss = readFileSync(path.resolve(__dirname, "../styles/desktop.css"), "utf-8");
    expect(desktopCss).toContain(".d2-preview-snippet__head");
    const resourceRowIdx = desktopCss.indexOf(".resource-row {");
    const dirOverrideIdx = desktopCss.indexOf(".resource-row.workspace-tree-dir-btn {");
    expect(resourceRowIdx).toBeGreaterThan(-1);
    expect(dirOverrideIdx).toBeGreaterThan(resourceRowIdx);

    // R2-UI-04 + R2-UI-05 + R2-UI-06 fixtures
    const mcpTaskRunSession: SessionRecord = {
      session_id: "30000000-0000-4000-8000-00000000r277",
      agent_id: agentA.agent_id,
      project_id: projectA.project_id,
      title: "Проверка сборки каждые 2 часа",
      session_kind: "task_run",
      status: "active",
      pinned: false,
      pinned_at: null,
      created_at: now - 5_000,
      updated_at: now - 5_000,
    };
    const mcpIntervalTask: TaskRecord = {
      task_id: "75000000-0000-4000-8000-00000000r277",
      project_id: projectA.project_id,
      session_id: mcpTaskRunSession.session_id,
      agent_id: agentA.agent_id,
      title: "Проверка сборки каждые 2 часа",
      goal: "Проверять статус CI",
      generation: 1,
      status: "waiting_timer",
      schedule: {
        timezone: "UTC",
        original_expression: "every 7200s",
        kind: { kind: "interval", interval_secs: 7200 },
        missed_run_policy: "execute_once",
        next_fire_at: now + 7_140_000,
      },
      budget: {},
      usage: { turns_used: 0, tokens_used: 0, wall_ms_used: 0, confidence: "provider_reported" },
      attached_skills: [],
      created_at: now - 5_000,
      updated_at: now - 5_000,
    };
    const plainMdArtifact: ArtifactRecord = {
      artifact_id: "70000000-0000-4000-8000-00000000r277",
      session_id: sessionA.session_id,
      project_id: projectA.project_id,
      run_id: null,
      name: "README-notes.md",
      mime_type: "text/plain",
      revision: 1,
      content: "# Заметки по релизу\n\n- Пункт 1",
      relative_path: null,
      sha256: "f".repeat(64),
      completeness_status: "final",
      lifecycle_state: "active",
      created_at: now - 4_000,
    };

    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [projectA, projectB],
      agents: [agentA, agentB],
      sessions: [sessionA, sessionB, mcpTaskRunSession],
      tasks: [mcpIntervalTask],
      artifactsBySession: { [sessionA.session_id]: [plainMdArtifact] },
    });

    render(
      <App
        bridge={bridge}
        windowControls={createTestWindowControls()}
        initialRightPanelOpen={true}
      />,
    );

    // R2-UI-06: README-notes.md with mime_type="text/plain" renders Markdown preview tabs
    expect(await screen.findByTestId("artifact-mode-rendered")).toBeInTheDocument();
    expect(screen.getByTestId("artifact-mode-source")).toBeInTheDocument();
    expect(
      screen.getByTestId("artifact-preview-markdown").querySelector("h1")?.textContent,
    ).toBe("Заметки по релизу");

    // R2-UI-03 & R2-UI-04: ProjectScopeFilter aligned classes & Russian schedule chip (not "every 7200s")
    await user.click(screen.getByTestId("rail-nav-tasks"));
    const scheduleChip = await screen.findByTestId(`task-schedule-chip-${mcpIntervalTask.task_id}`);
    expect(scheduleChip).toHaveTextContent("каждые 2ч");
    expect(scheduleChip).not.toHaveTextContent("every 7200s");

    await user.click(screen.getByTestId("project-scope-filter-trigger"));
    const scopeMenu = await screen.findByTestId("project-scope-filter-menu");
    expect(scopeMenu.querySelectorAll(".project-scope-filter__opt-copy").length).toBeGreaterThan(0);
    expect(scopeMenu.querySelectorAll(".project-scope-filter__opt-meta").length).toBeGreaterThan(0);
    expect(scopeMenu.querySelectorAll(".project-scope-filter__count").length).toBeGreaterThan(0);
    await user.keyboard("{Escape}");

    // R2-UI-05: Empty task_run session shows honest background-event copy (not "Напишите поручение внизу")
    await user.click(screen.getByTestId(`task-open-chat-${mcpIntervalTask.task_id}`));
    const emptyState = await screen.findByTestId("empty-chat-state");
    expect(emptyState).toHaveTextContent("Запусков по этому событию ещё не было");
    expect(emptyState).not.toHaveTextContent("Напишите поручение внизу");
    cleanup();

    // R2-UI-07: Typing into DevFolderPickerModal before initial load resolves is never clobbered
    let resolveInitialFetch!: (value: unknown) => void;
    const delayedFetch = vi.fn(
      () =>
        new Promise((resolve) => {
          resolveInitialFetch = resolve;
        }),
    );
    vi.stubGlobal("fetch", delayedFetch);

    const pickerBridge = new MockDesktopBridge({
      connected: true,
      supportsNativeFolderPicker: false,
    });
    render(<App bridge={pickerBridge} windowControls={createTestWindowControls()} />);
    await user.click(await screen.findByTestId("onboarding-pick-folder-btn"));
    const pathInput = (await screen.findByTestId(
      "dev-folder-picker-path-input",
    )) as HTMLInputElement;
    await user.type(pathInput, "A:/Dev/my-custom-folder");
    expect(pathInput.value).toBe("A:/Dev/my-custom-folder");

    // Resolve stale initial home scan after user typed -> pathDraft must remain intact!
    resolveInitialFetch({
      ok: true,
      status: 200,
      json: async () => ({
        current: "C:/Users/tester",
        parent: "C:/Users",
        dirs: [{ name: "Documents", path: "C:/Users/tester/Documents" }],
      }),
    });
    await screen.findByText("Documents");
    expect(pathInput.value).toBe("A:/Dev/my-custom-folder");
  });
});
