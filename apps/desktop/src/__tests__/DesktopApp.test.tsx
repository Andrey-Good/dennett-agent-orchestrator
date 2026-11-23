import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

const desktopCss = readFileSync(resolve(__dirname, "../styles/desktop.css"), "utf8");
const tokensCss = readFileSync(resolve(__dirname, "../styles/tokens.css"), "utf8");

import type { WindowControls } from "../api/bridge";
import type { AgentRecord, ProjectRecord, SessionRecord } from "../api/contracts";
import { MockDesktopBridge } from "../api/mockBridge";
import { App } from "../App";
import { QuotaRings } from "../components/QuotaRings";

function createTestWindowControls(): WindowControls & {
  closeMock: ReturnType<typeof vi.fn>;
} {
  const closeMock = vi.fn(async () => {});
  return {
    supported: true,
    minimize: vi.fn(async () => {}),
    toggleMaximize: vi.fn(async () => {}),
    close: closeMock,
    startDragging: vi.fn(async () => {}),
    isMaximized: vi.fn(async () => false),
    closeMock,
  };
}

function makeSeedFixture() {
  const now = Date.now();
  const project: ProjectRecord = {
    project_id: "10000000-0000-4000-8000-000000000001",
    name: "dennett-core",
    workspace_path: "A:/Dev/dennett-core",
    memory_scope: "project:dennett-core",
    created_at: now - 10_000,
  };
  const agent: AgentRecord = {
    agent_id: "20000000-0000-4000-8000-000000000001",
    name: "Проектный агент",
    description: "Основной агент проекта",
    current_config_version_id: "21000000-0000-4000-8000-000000000001",
    current_config: {
      config_version_id: "21000000-0000-4000-8000-000000000001",
      agent_id: "20000000-0000-4000-8000-000000000001",
      version_number: 1,
      system_prompt: "Помогай проекту.",
      model_preference: "gemini-4-neon",
      thinking_level: "medium",
      allowed_memory_scopes: ["project:dennett-core"],
      workspace_write_policy: { mode: "ask_before_write" },
      created_at: now - 10_000,
    },
    created_at: now - 10_000,
    updated_at: now - 10_000,
  };
  const sessionA: SessionRecord = {
    session_id: "30000000-0000-4000-8000-000000000001",
    agent_id: agent.agent_id,
    project_id: project.project_id,
    title: "Сессия A",
    runtime_binding: {
      binding_id: "60000000-0000-4000-8000-000000000001",
      session_id: "30000000-0000-4000-8000-000000000001",
      adapter_id: "antigravity-acp",
      auth_profile: "oauth-personal",
      tier: "Google AI Ultra",
      agy_version: "1.5.15",
      provider_session_id: "prov-a",
      workspace_cwd: "A:/Dev/dennett-core",
      is_simulated: false,
      rehydrate_count: 0,
      created_at: now - 9_000,
      updated_at: now - 5_000,
    },
    status: "active",
    created_at: now - 9_000,
    updated_at: now - 5_000,
  };
  const sessionB: SessionRecord = {
    session_id: "30000000-0000-4000-8000-000000000002",
    agent_id: agent.agent_id,
    project_id: project.project_id,
    title: "Сессия B",
    runtime_binding: null,
    status: "active",
    created_at: now - 8_000,
    updated_at: now - 4_000,
  };
  return { project, agent, sessionA, sessionB };
}

describe("Dennett Desktop D1 Rework UI", () => {
  it("renders honest Head offline state and recovers cleanly on Retry", async () => {
    const user = userEvent.setup();
    const bridge = new MockDesktopBridge({
      connected: false,
      offlineError: "connection refused on 127.0.0.1:41771",
    });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);

    const offline = await screen.findByTestId("head-offline-state");
    expect(offline).toHaveAttribute("role", "alert");
    expect(screen.getByTestId("head-offline-detail")).toHaveTextContent(
      "connection refused on 127.0.0.1:41771",
    );
    expect(screen.getByTestId("runtime-status-pill")).toHaveTextContent("Head офлайн");

    bridge.connected = true;
    await user.click(screen.getByTestId("retry-head-button"));

    expect(await screen.findByTestId("empty-onboarding-state")).toBeInTheDocument();
  });

  it("supports zero-friction 1-click folderless onboarding, composer focus, and auto-title on first message", async () => {
    const user = userEvent.setup();
    const bridge = new MockDesktopBridge({ connected: true });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);

    const onboarding = await screen.findByTestId("empty-onboarding-state");
    // Zero path text inputs or memoryScope inputs in onboarding!
    expect(onboarding.querySelectorAll("input")).toHaveLength(0);

    await user.click(screen.getByTestId("onboarding-folderless-btn"));

    const composerInput = await screen.findByLabelText("Поле ввода сообщения");
    await user.type(composerInput, "## Собери релизный план D1{enter}");

    expect(await screen.findByText("## Собери релизный план D1")).toBeInTheDocument();
    expect(await screen.findByText(/Принято .* Собери релизный план D1/)).toBeInTheDocument();
    // Auto-titled session appears in sidebar and breadcrumbs without ever asking for a chat name!
    await waitFor(() => {
      expect(screen.getAllByText("Собери релизный план D1").length).toBeGreaterThanOrEqual(1);
    });
    expect(bridge.projects[0]?.workspace_path).toBeNull();
  });

  it("uses native folder picker for 'Выбрать папку…' and renders real workspace files in Workspace panel", async () => {
    const user = userEvent.setup();
    const bridge = new MockDesktopBridge({
      connected: true,
      supportsNativeFolderPicker: true,
      mockPickedFolder: "A:/Dev/my-cool-repo",
    });

    render(
      <App
        bridge={bridge}
        windowControls={createTestWindowControls()}
        initialRightPanelOpen={true}
      />,
    );

    await screen.findByTestId("empty-onboarding-state");
    await user.click(screen.getByTestId("onboarding-pick-folder-btn"));

    await waitFor(() => {
      expect(bridge.projects).toHaveLength(1);
    });
    const createdProj = bridge.projects[0]!;
    expect(createdProj.name).toBe("my-cool-repo");
    expect(createdProj.workspace_path).toBe("A:/Dev/my-cool-repo");

    bridge.filesByProject[createdProj.project_id] = [
      { relative_path: "src", name: "src", kind: "directory", size_bytes: null, depth: 1 },
      { relative_path: "README.md", name: "README.md", kind: "file", size_bytes: 512, depth: 1 },
    ];
    await user.click(screen.getByRole("button", { name: "Обновить список файлов" }));

    const fileList = await screen.findByTestId("workspace-file-list");
    expect(fileList).toHaveTextContent("src");
    expect(fileList).toHaveTextContent("README.md");
    // No fake PostgreSQL connection card in Workspace panel!
    expect(screen.getByTestId("workspace-sibling-panel")).not.toHaveTextContent("PostgreSQL");
  });

  it("renders dynamic translucent Model & Thinking popovers (zero <select>), handles Claude built-in thinking, and updates AgentConfigVersion on the SAME AgentId", async () => {
    const user = userEvent.setup();
    const { project, agent, sessionA, sessionB } = makeSeedFixture();
    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [project],
      agents: [agent],
      sessions: [sessionA, sessionB],
    });

    const { container } = render(
      <App bridge={bridge} windowControls={createTestWindowControls()} />,
    );

    const input = await screen.findByLabelText("Поле ввода сообщения");
    // Strictly ZERO native <select> elements anywhere in the DOM!
    expect(container.querySelectorAll("select")).toHaveLength(0);
    // Zero fake hardcoded models or Demo agent technical labels!
    expect(container.textContent).not.toContain("Агент Demo D1");
    expect(container.textContent).not.toContain("Gemini 3.8 Pro");
    expect(container.textContent).not.toContain("Gemini 2.5");

    // Shift+Enter inserts newline without submitting
    await user.type(input, "Строка 1{Shift>}{Enter}{/Shift}Строка 2");
    expect(input).toHaveValue("Строка 1\nСтрока 2");
    expect(bridge.calls.filter((c) => c.method === "sendMessage")).toHaveLength(0);

    // Switch to Session B and back to Session A — draft survives intact
    await user.click(screen.getByTestId(`session-item-${sessionB.session_id}`));
    await waitFor(() => {
      expect(screen.getByLabelText("Поле ввода сообщения")).toHaveValue("");
    });
    await user.click(screen.getByTestId(`session-item-${sessionA.session_id}`));
    await waitFor(() => {
      expect(screen.getByLabelText("Поле ввода сообщения")).toHaveValue("Строка 1\nСтрока 2");
    });

    // Open dynamic translucent Model popover -> switch to Gemini 4 Carbon
    await user.click(screen.getByTestId("model-selector-trigger"));
    const modelPopover = await screen.findByTestId("model-popover");
    expect(modelPopover).toHaveTextContent("Gemini 4 Neon");
    expect(modelPopover).toHaveTextContent("Gemini 4 Carbon");
    expect(modelPopover).toHaveTextContent("Claude Opus 4.6 (Thinking)");
    await user.click(screen.getByTestId("model-option-gemini-4-carbon"));

    await waitFor(() => {
      expect(bridge.agents[0]?.current_config.version_number).toBe(2);
    });
    expect(bridge.calls.filter((c) => c.method === "createAgent")).toHaveLength(0);
    expect(bridge.agents[0]?.agent_id).toBe(agent.agent_id);
    expect(bridge.agents[0]?.current_config.model_preference).toBe("gemini-4-carbon");

    // Open Thinking popover for Gemini 4 Carbon -> xhigh must NOT be offered!
    await user.click(screen.getByTestId("thinking-selector-trigger"));
    const thinkingPopover = await screen.findByTestId("thinking-popover");
    expect(within(thinkingPopover).getByTestId("thinking-option-high")).toBeInTheDocument();
    expect(within(thinkingPopover).queryByTestId("thinking-option-xhigh")).not.toBeInTheDocument();
    await user.click(within(thinkingPopover).getByTestId("thinking-option-high"));

    await waitFor(() => {
      expect(bridge.agents[0]?.current_config.thinking_level).toBe("high");
    });

    // Switch to Claude Opus 4.6 (Thinking) -> built-in thinking badge shown, no thinking trigger
    await user.click(screen.getByTestId("model-selector-trigger"));
    await user.click(await screen.findByTestId("model-option-claude-opus-4-6-thinking"));
    expect(await screen.findByTestId("thinking-builtin-badge")).toHaveTextContent("Thinking");
    expect(screen.queryByTestId("thinking-selector-trigger")).not.toBeInTheDocument();

    // Fail next send -> error alert shown AND unsent draft preserved losslessly
    bridge.failNextSendWith = { code: "RUNTIME_PROTOCOL", message: "временный сбой runtime" };
    await user.click(screen.getByTestId("send-button"));

    expect(await screen.findByTestId("error-banner")).toHaveTextContent("временный сбой runtime");
    expect(screen.getByLabelText("Поле ввода сообщения")).toHaveValue("Строка 1\nСтрока 2");
  });

  it("supports 1-click new chat, inline session rename (Enter/Escape), and inline project rename + folder attach", async () => {
    const user = userEvent.setup();
    const { project, agent, sessionA } = makeSeedFixture();
    const folderlessProject: ProjectRecord = {
      ...project,
      workspace_path: null,
    };
    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [folderlessProject],
      agents: [agent],
      sessions: [sessionA],
      supportsNativeFolderPicker: true,
      mockPickedFolder: "A:/Dev/bound-later",
    });

    render(
      <App
        bridge={bridge}
        windowControls={createTestWindowControls()}
        initialRightPanelOpen={true}
      />,
    );
    await screen.findByTestId("chat-card");

    // 1-click new chat creates blank "Новый чат" immediately without prompting for a title
    await user.click(screen.getByTestId(`new-chat-btn-${folderlessProject.project_id}`));
    await waitFor(() => {
      expect(bridge.sessions).toHaveLength(2);
    });
    expect(bridge.sessions[0]?.title).toBe("Новый чат");

    // Inline session rename -> Escape cancels, Enter commits PATCH /v1/sessions/{id}
    await user.click(screen.getByRole("button", { name: "Переименовать чат Сессия A" }));
    const sessInput = await screen.findByTestId("inline-session-rename-input");
    await user.clear(sessInput);
    await user.type(sessInput, "Отменённое имя{Escape}");
    expect(bridge.sessions.find((s) => s.session_id === sessionA.session_id)?.title).toBe(
      "Сессия A",
    );

    await user.click(screen.getByRole("button", { name: "Переименовать чат Сессия A" }));
    const sessInput2 = await screen.findByTestId("inline-session-rename-input");
    await user.clear(sessInput2);
    await user.type(sessInput2, "Переименованный чат{Enter}");
    await waitFor(() => {
      expect(bridge.sessions.find((s) => s.session_id === sessionA.session_id)?.title).toBe(
        "Переименованный чат",
      );
    });

    // Inline project rename -> Enter commits PATCH /v1/projects/{id}
    await user.click(screen.getByRole("button", { name: "Переименовать проект dennett-core" }));
    const projInput = await screen.findByTestId("inline-project-rename-input");
    await user.clear(projInput);
    await user.type(projInput, "Мой рабочий проект{Enter}");
    await waitFor(() => {
      expect(bridge.projects[0]?.name).toBe("Мой рабочий проект");
    });

    // Folderless right panel shows unbound card + "Привязать папку…" button -> attaches folder!
    expect(screen.getByTestId("workspace-unbound-notice")).toBeInTheDocument();
    await user.click(screen.getByTestId("workspace-attach-folder-btn"));
    await waitFor(() => {
      expect(bridge.projects[0]?.workspace_path).toBe("A:/Dev/bound-later");
    });
  });

  it("renders bottom-left UserSettingsBar, Settings sheet, top StatsPopover, and QuotaRings cleanly", async () => {
    const user = userEvent.setup();
    const { project, agent, sessionA } = makeSeedFixture();
    const bridge = new MockDesktopBridge({
      connected: true,
      accountLabel: "urako",
      tier: "Google AI Ultra",
      quotas: [],
      projects: [project],
      agents: [agent],
      sessions: [sessionA],
      notesByScope: {
        [project.memory_scope]: [
          {
            note_id: "n1",
            scope: project.memory_scope,
            title: "Архитектурный факт",
            body: "PG — источник истины",
            tags: [],
            revision: 1,
            created_at: Date.now(),
            updated_at: Date.now(),
          },
        ],
      },
    });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);

    expect(await screen.findByTestId("user-account-label")).toHaveTextContent("urako");
    expect(screen.getByTestId("user-profile-tier")).toHaveTextContent("Google AI Ultra");
    // Empty quotas -> QuotaRings completely hidden (zero fake 100s)
    expect(screen.queryByTestId("quota-rings")).not.toBeInTheDocument();

    // Open translucent Settings & Connections sheet from bottom-left gear
    await user.click(screen.getByTestId("settings-trigger"));
    expect(await screen.findByTestId("settings-sheet")).toBeInTheDocument();
    await user.click(screen.getByTestId("refresh-catalog-button"));
    await waitFor(() => {
      expect(
        bridge.calls.some(
          (c) =>
            c.method === "getRuntimeCatalog" &&
            typeof c.args === "object" &&
            c.args !== null &&
            (c.args as { refresh?: boolean }).refresh === true,
        ),
      ).toBe(true);
    });
    await user.click(screen.getByRole("button", { name: "Закрыть настройки" }));
    expect(screen.queryByTestId("settings-sheet")).not.toBeInTheDocument();

    // Open top-bar Stats Popover -> displays live memoryd note count in project scope
    await user.click(screen.getByTestId("system-status-pill"));
    const stats = await screen.findByTestId("stats-popover");
    expect(stats).toHaveTextContent("Состояние системы и памяти");
    await waitFor(() => {
      expect(screen.getByTestId("stats-memory-notes-count")).toHaveTextContent("1");
    });
  });

  it("renders <QuotaRings /> with short window labels and bare integers (zero '%' or verbose labels)", () => {
    const { container, rerender } = render(<QuotaRings quotas={[]} />);
    expect(container.firstChild).toBeNull();

    rerender(
      <QuotaRings
        quotas={[
          { label: "5ч", remaining_percent: 93 },
          { label: "нед", remaining_percent: 12 },
        ]}
      />,
    );
    const rings = screen.getByTestId("quota-rings");
    expect(rings).toHaveTextContent("5ч");
    expect(rings).toHaveTextContent("93");
    expect(rings).toHaveTextContent("нед");
    expect(rings).toHaveTextContent("12");
    expect(rings.textContent).not.toContain("%");
    expect(rings.textContent).not.toMatch(/процент|дневной|недельный/i);
  });

  it("exposes always-accessible Stop outside messages/artifacts and marks cancelled runs explicitly as Cancelled", async () => {
    const user = userEvent.setup();
    const { project, agent, sessionA } = makeSeedFixture();
    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [project],
      agents: [agent],
      sessions: [sessionA],
      sendDelayMs: 5_000,
    });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);

    const input = await screen.findByLabelText("Поле ввода сообщения");
    await user.type(input, "Долгое задание{enter}");

    const runningBar = await screen.findByTestId("running-control-bar");
    const stopBtn = within(runningBar).getByTestId("global-stop-button");
    expect(stopBtn).toBeEnabled();

    const timeline = screen.getByTestId("message-timeline");
    expect(timeline.contains(stopBtn)).toBe(false);

    await user.click(stopBtn);

    expect(await screen.findByTestId("cancelled-run-notice")).toHaveTextContent(/Cancelled|остановлен/i);
    expect(await screen.findByTestId("run-status-cancelled")).toHaveTextContent("Остановлено (Cancelled)");
    expect(bridge.calls.some((c) => c.method === "cancelRun")).toBe(true);
  });

  it("triggers Stop on Escape key while a run is active", async () => {
    const user = userEvent.setup();
    const { project, agent, sessionA } = makeSeedFixture();
    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [project],
      agents: [agent],
      sessions: [sessionA],
      sendDelayMs: 5_000,
    });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);

    const input = await screen.findByLabelText("Поле ввода сообщения");
    await user.type(input, "Отмена по Escape{enter}");

    await screen.findByTestId("running-control-bar");
    await user.keyboard("{Escape}");

    expect(await screen.findByTestId("cancelled-run-notice")).toBeInTheDocument();
    expect(bridge.calls.some((c) => c.method === "cancelRun")).toBe(true);
  });

  it("shows honest V02 memoryd degraded banner, keeps Stop enabled, and supports manual outbox flush", async () => {
    const user = userEvent.setup();
    const { project, agent, sessionA } = makeSeedFixture();
    const bridge = new MockDesktopBridge({
      connected: true,
      memoryStatus: "degraded_unreachable",
      memoryDetail: "memoryd connection refused",
      pendingOutboxCount: 3,
      projects: [project],
      agents: [agent],
      sessions: [sessionA],
      sendDelayMs: 5_000,
    });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);

    const banner = await screen.findByTestId("memory-outage-banner");
    expect(banner).toHaveAttribute("role", "alert");
    expect(banner).toHaveTextContent(
      "Память memoryd недоступна — разговор сохраняется, отложенные записи ждут в очереди, Stop доступен",
    );
    expect(banner).toHaveTextContent("В очереди outbox: 3");

    const input = screen.getByLabelText("Поле ввода сообщения");
    await user.type(input, "Работа при отказе memoryd{enter}");
    const stopBtn = await screen.findByTestId("global-stop-button");
    expect(stopBtn).toBeEnabled();
    await user.click(stopBtn);
    expect(await screen.findByTestId("cancelled-run-notice")).toBeInTheDocument();

    bridge.memoryStatus = "healthy";
    bridge.memoryDetail = null;
    await user.click(screen.getByRole("button", { name: /Повторить отправку очереди/ }));

    await waitFor(() => {
      expect(screen.queryByTestId("memory-outage-banner")).not.toBeInTheDocument();
    });
    expect(bridge.calls.some((c) => c.method === "flushMemoryOutbox")).toBe(true);
    expect(bridge.pendingOutboxCount).toBe(0);
  });

  it("displays honest runtime profile badges (Google AI Ultra, Gemini API key) and unmistakable SIMULATED warning", async () => {
    const { project, agent, sessionA } = makeSeedFixture();

    const ultraBridge = new MockDesktopBridge({
      connected: true,
      runtimeAdapterId: "antigravity-acp",
      runtimeIsSimulated: false,
      authProfile: "oauth-personal",
      tier: "Google AI Ultra",
      projects: [project],
      agents: [agent],
      sessions: [sessionA],
    });
    const first = render(<App bridge={ultraBridge} windowControls={createTestWindowControls()} />);
    await waitFor(() => {
      expect(screen.getByTestId("runtime-status-pill")).toHaveTextContent(
        "Antigravity ACP · Google AI Ultra",
      );
    });
    expect(screen.queryByTestId("simulated-runtime-badge")).not.toBeInTheDocument();
    first.unmount();

    const apiSession: SessionRecord = {
      ...sessionA,
      runtime_binding: {
        ...sessionA.runtime_binding!,
        auth_profile: "gemini-api-key",
        tier: null,
        is_simulated: false,
      },
    };
    const apiBridge = new MockDesktopBridge({
      connected: true,
      runtimeAdapterId: "antigravity-acp",
      runtimeIsSimulated: false,
      authProfile: "gemini-api-key",
      tier: null,
      projects: [project],
      agents: [agent],
      sessions: [apiSession],
    });
    const second = render(<App bridge={apiBridge} windowControls={createTestWindowControls()} />);
    await waitFor(() => {
      expect(screen.getByTestId("runtime-status-pill")).toHaveTextContent(
        "Antigravity ACP · Gemini API key",
      );
    });
    expect(screen.queryByTestId("simulated-runtime-badge")).not.toBeInTheDocument();
    second.unmount();

    const simSession: SessionRecord = {
      ...sessionA,
      runtime_binding: {
        ...sessionA.runtime_binding!,
        adapter_id: "simulated-ci-runtime",
        is_simulated: true,
      },
    };
    const simBridge = new MockDesktopBridge({
      connected: true,
      runtimeAdapterId: "simulated-ci-runtime",
      runtimeIsSimulated: true,
      projects: [project],
      agents: [agent],
      sessions: [simSession],
    });
    render(<App bridge={simBridge} windowControls={createTestWindowControls()} />);
    expect(await screen.findByTestId("simulated-runtime-badge")).toHaveTextContent("SIMULATED");
  });

  it("toggles right sibling Workspace card cleanly and renders adversarial content safely without XSS or <select>", async () => {
    const user = userEvent.setup();
    const { project, agent, sessionA } = makeSeedFixture();
    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [project],
      agents: [agent],
      sessions: [sessionA],
      messagesBySession: {
        [sessionA.session_id]: [
          {
            message_id: "40000000-0000-4000-8000-000000000099",
            session_id: sessionA.session_id,
            run_id: null,
            role: "assistant",
            content:
              '**Безопасный ответ** <script>window.__xss=3</script><img src=x onerror="window.__xss=4"> [Плохая ссылка](javascript:alert(1))',
            created_at: Date.now() - 1000,
            correlation_id: "corr-xss-1",
          },
        ],
      },
    });

    const { container } = render(
      <App bridge={bridge} windowControls={createTestWindowControls()} />,
    );
    await screen.findByTestId("chat-card");

    // Assistant Markdown renders bold cleanly while stripping raw HTML tags and neutralizing javascript: hrefs
    const assistantMsg = await screen.findByTestId("message-assistant");
    expect(assistantMsg.querySelector("strong")).toHaveTextContent("Безопасный ответ");
    expect(assistantMsg.querySelector("script")).toBeNull();
    expect(assistantMsg.querySelector("img")).toBeNull();
    for (const anchor of Array.from(assistantMsg.querySelectorAll("a"))) {
      const href = anchor.getAttribute("href") ?? "";
      expect(href.toLowerCase()).not.toContain("javascript:");
    }

    expect(screen.queryByTestId("workspace-sibling-panel")).not.toBeInTheDocument();

    const toggleBtn = screen.getByRole("button", {
      name: "Переключить панель рабочего пространства и артефактов",
    });
    expect(toggleBtn).toHaveAttribute("aria-expanded", "false");

    await user.click(toggleBtn);
    expect(toggleBtn).toHaveAttribute("aria-expanded", "true");

    const stage = screen.getByTestId("workspace-stage");
    const chatCard = screen.getByTestId("chat-card");
    const panel = screen.getByTestId("workspace-sibling-panel");
    expect(stage.children).toHaveLength(2);
    expect(stage.children[0]).toBe(chatCard);
    expect(stage.children[1]).toBe(panel);
    expect(chatCard.contains(panel)).toBe(false);

    // Create artifact using custom translucent MIME picker (zero <select>!)
    await user.click(screen.getByTestId("toggle-create-artifact-button"));
    expect(container.querySelectorAll("select")).toHaveLength(0);
    await user.click(screen.getByTestId("artifact-mime-trigger"));
    await user.click(screen.getByRole("menuitemradio", { name: /JSON/ }));

    await user.type(screen.getByLabelText("Имя артефакта"), "xss-check.md");
    const xssPayload = '<script>window.__xss=1</script><img src=x onerror="window.__xss=2">';
    await user.type(screen.getByLabelText("Содержимое артефакта"), xssPayload);
    await user.click(screen.getByRole("button", { name: "Сохранить" }));

    const preview = await screen.findByTestId("artifact-preview");
    expect(preview).toHaveTextContent("xss-check.md");
    expect(preview).toHaveTextContent("rev 1");
    expect(preview).toHaveTextContent(xssPayload);
    expect(preview.querySelector("script")).toBeNull();
    expect(preview.querySelector("img")).toBeNull();

    await user.click(screen.getByRole("button", { name: "Скрыть панель рабочего пространства" }));
    expect(screen.queryByTestId("workspace-sibling-panel")).not.toBeInTheDocument();
  });

  it("closes only the native UI window without issuing daemon shutdown and enforces zero CSS backdrop-filter", async () => {
    const user = userEvent.setup();
    const { project, agent, sessionA } = makeSeedFixture();
    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [project],
      agents: [agent],
      sessions: [sessionA],
    });
    const winControls = createTestWindowControls();

    render(<App bridge={bridge} windowControls={winControls} />);
    await screen.findByTestId("chat-card");

    await user.click(screen.getByRole("button", { name: "Закрыть окно" }));
    expect(winControls.closeMock).toHaveBeenCalledTimes(1);
    expect(bridge.calls.some((c) => c.method.toLowerCase().includes("shutdown"))).toBe(false);

    for (const css of [tokensCss, desktopCss]) {
      expect(css.length).toBeGreaterThan(500);
      const withoutComments = css.replace(/\/\*[\s\S]*?\*\//g, "");
      expect(withoutComments.toLowerCase()).not.toContain("backdrop-filter");
    }
  });

  it("anchors composer in flex column chat-card, auto-grows textarea without resize handle, and keeps popovers opaque", async () => {
    const user = userEvent.setup();
    const { project, agent, sessionA } = makeSeedFixture();
    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [project],
      agents: [agent],
      sessions: [sessionA],
    });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);
    const textarea = (await screen.findByLabelText("Поле ввода сообщения")) as HTMLTextAreaElement;

    // Auto-grow initializes cleanly to the 54px baseline and grows/resets without a resize handle
    expect(textarea.style.height).toBe("54px");
    Object.defineProperty(textarea, "scrollHeight", { configurable: true, value: 120 });
    await user.type(textarea, "Первая строка{Shift>}{Enter}{/Shift}Вторая строка");
    expect(textarea.style.height).toBe("120px");

    // Layout & visual regressions caught during live browser review must never regress
    expect(desktopCss).toMatch(/\.chat-card\s*\{[^}]*display:\s*flex;\s*flex-direction:\s*column;/s);
    expect(desktopCss).not.toMatch(/\.chat-card\s*\{[^}]*grid-template-rows:/s);
    expect(desktopCss).toMatch(/\.composer__input\s*\{[^}]*resize:\s*none;/s);
    expect(desktopCss).toMatch(/\.composer-toolbar\s*\{[^}]*flex-wrap:\s*wrap;/s);
    expect(desktopCss).toMatch(/\.popover-scroll-list\s*\{[^}]*max-height:\s*min\(420px,/s);
    expect(tokensCss).toMatch(/--popover-bg:\s*#1e1e20;/);
  });

  it("isolates memory scopes across multiple newly created folderless projects without binding Project 2 to Project 1's agent", async () => {
    const user = userEvent.setup();
    const bridge = new MockDesktopBridge({ connected: true });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);
    await screen.findByTestId("empty-onboarding-state");

    // Project 1 via 1-click folderless onboarding
    await user.click(screen.getByTestId("onboarding-folderless-btn"));
    await screen.findByLabelText("Поле ввода сообщения");
    expect(bridge.projects).toHaveLength(1);
    expect(bridge.agents).toHaveLength(1);

    // Project 2 via sidebar '+' translucent 2-option menu -> "Пока без папки"
    await user.click(screen.getByTestId("new-project-trigger"));
    await user.click(await screen.findByTestId("create-project-folderless"));

    await waitFor(() => {
      expect(bridge.projects).toHaveLength(2);
      expect(bridge.sessions).toHaveLength(2);
    });
    expect(bridge.agents).toHaveLength(2);

    const [p2, p1] = bridge.projects;
    expect(p1!.memory_scope).not.toBe(p2!.memory_scope);

    const s1 = bridge.sessions.find((s) => s.project_id === p1!.project_id)!;
    const s2 = bridge.sessions.find((s) => s.project_id === p2!.project_id)!;
    const a1 = bridge.agents.find((a) => a.agent_id === s1.agent_id)!;
    const a2 = bridge.agents.find((a) => a.agent_id === s2.agent_id)!;
    expect(a1.agent_id).not.toBe(a2.agent_id);
    expect(a1.current_config.allowed_memory_scopes).toEqual([p1!.memory_scope]);
    expect(a2.current_config.allowed_memory_scopes).toEqual([p2!.memory_scope]);

    // Second chat in Project 2 reuses Project 2's dedicated agent
    await user.click(screen.getByTestId(`new-chat-btn-${p2!.project_id}`));
    await waitFor(() => {
      expect(bridge.sessions).toHaveLength(3);
    });
    expect(bridge.agents).toHaveLength(2);
  });

  it("prevents an in-flight turn in Session A from clobbering Session B timeline and artifacts after switching sessions", async () => {
    const user = userEvent.setup();
    const { project, agent, sessionA, sessionB } = makeSeedFixture();
    const now = Date.now();
    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [project],
      agents: [agent],
      sessions: [sessionA, sessionB],
      sendDelayMs: 120,
      messagesBySession: {
        [sessionA.session_id]: [
          {
            message_id: "50000000-0000-4000-8000-0000000000a1",
            session_id: sessionA.session_id,
            role: "assistant",
            content: "Начальное сообщение Сессии A",
            run_id: null,
            correlation_id: "corr-a1",
            created_at: now - 5_000,
          },
        ],
        [sessionB.session_id]: [
          {
            message_id: "50000000-0000-4000-8000-0000000000b1",
            session_id: sessionB.session_id,
            role: "assistant",
            content: "Уникальное сообщение Сессии B",
            run_id: null,
            correlation_id: "corr-b1",
            created_at: now - 4_000,
          },
        ],
      },
      artifactsBySession: {
        [sessionA.session_id]: [
          {
            artifact_id: "70000000-0000-4000-8000-0000000000a1",
            session_id: sessionA.session_id,
            project_id: project.project_id,
            run_id: null,
            name: "artifact-from-a.md",
            mime_type: "text/markdown",
            revision: 1,
            content: "Контент артефакта A",
            relative_path: null,
            sha256: "a".repeat(64),
            created_at: now - 5_000,
          },
        ],
        [sessionB.session_id]: [
          {
            artifact_id: "70000000-0000-4000-8000-0000000000b1",
            session_id: sessionB.session_id,
            project_id: project.project_id,
            run_id: null,
            name: "artifact-from-b.md",
            mime_type: "text/markdown",
            revision: 1,
            content: "Контент артефакта B",
            relative_path: null,
            sha256: "b".repeat(64),
            created_at: now - 4_000,
          },
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

    expect(await screen.findByText("Начальное сообщение Сессии A")).toBeInTheDocument();

    const input = screen.getByLabelText("Поле ввода сообщения");
    await user.type(input, "Запрос в Сессию A{enter}");
    await screen.findByTestId("running-control-bar");

    await user.click(screen.getByTestId(`session-item-${sessionB.session_id}`));
    expect(await screen.findByText("Уникальное сообщение Сессии B")).toBeInTheDocument();
    expect(screen.getByTestId("artifact-preview")).toHaveTextContent("artifact-from-b.md");

    await waitFor(
      () => {
        expect(screen.queryByTestId("running-control-bar")).not.toBeInTheDocument();
      },
      { timeout: 2_000 },
    );

    expect(screen.getByText("Уникальное сообщение Сессии B")).toBeInTheDocument();
    expect(screen.queryByText("Запрос в Сессию A")).not.toBeInTheDocument();
    expect(screen.getByTestId("artifact-preview")).toHaveTextContent("artifact-from-b.md");
    expect(screen.getByTestId("artifact-preview")).not.toHaveTextContent("artifact-from-a.md");

    await user.click(screen.getByTestId(`session-item-${sessionA.session_id}`));
    expect(await screen.findByText("Запрос в Сессию A")).toBeInTheDocument();
    expect(await screen.findByText(/Принято .* Запрос в Сессию A/)).toBeInTheDocument();
  });
});
