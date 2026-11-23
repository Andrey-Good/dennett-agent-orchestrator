import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import desktopCss from "../styles/desktop.css?raw";
import tokensCss from "../styles/tokens.css?raw";

import type { WindowControls } from "../api/bridge";
import type { AgentRecord, ProjectRecord, SessionRecord } from "../api/contracts";
import { MockDesktopBridge } from "../api/mockBridge";
import { App } from "../App";

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
    name: "Главный агент",
    description: "Основной агент проекта",
    current_config_version_id: "21000000-0000-4000-8000-000000000001",
    current_config: {
      config_version_id: "21000000-0000-4000-8000-000000000001",
      agent_id: "20000000-0000-4000-8000-000000000001",
      version_number: 1,
      system_prompt: "Помогай проекту.",
      model_preference: "gemini-2.5-pro",
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
      workspace_cwd: project.workspace_path,
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

describe("Dennett Desktop D1 UI", () => {
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

    // Bring Head online and click Retry
    bridge.connected = true;
    await user.click(screen.getByTestId("retry-head-button"));

    expect(await screen.findByTestId("empty-onboarding-state")).toBeInTheDocument();
  });

  it("supports empty onboarding flow to create first project and start chatting", async () => {
    const user = userEvent.setup();
    const bridge = new MockDesktopBridge({ connected: true });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);

    expect(await screen.findByTestId("empty-onboarding-state")).toBeInTheDocument();

    const workspaceInput = screen.getByLabelText("Каталог рабочей области");
    await user.type(workspaceInput, "C:/work/first-project");
    await user.click(screen.getByRole("button", { name: "Создать проект и начать чат" }));

    const composerInput = await screen.findByLabelText("Поле ввода сообщения");
    await user.type(composerInput, "Привет, Dennett!{enter}");

    expect(await screen.findByText("Привет, Dennett!")).toBeInTheDocument();
    expect(await screen.findByText(/Принято .* Привет, Dennett!/)).toBeInTheDocument();
  });

  it("handles Shift+Enter newline, switches model via new AgentConfigVersion without creating a new Agent, and preserves unsent drafts across sessions and errors", async () => {
    const user = userEvent.setup();
    const { project, agent, sessionA, sessionB } = makeSeedFixture();
    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [project],
      agents: [agent],
      sessions: [sessionA, sessionB],
    });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);

    const input = await screen.findByLabelText("Поле ввода сообщения");
    expect(screen.getByTestId("agent-config-version")).toHaveTextContent("Главный агент · v1");

    // Shift+Enter inserts newline without submitting
    await user.type(input, "Строка 1{Shift>}{Enter}{/Shift}Строка 2");
    expect(input).toHaveValue("Строка 1\nСтрока 2");
    expect(bridge.calls.filter((c) => c.method === "sendMessage")).toHaveLength(0);

    // Switch to Session B and back to Session A — draft must survive intact
    await user.click(screen.getByRole("button", { name: /Сессия B/ }));
    await waitFor(() => {
      expect(screen.getByLabelText("Поле ввода сообщения")).toHaveValue("");
    });
    await user.click(screen.getByRole("button", { name: /Сессия A/ }));
    await waitFor(() => {
      expect(screen.getByLabelText("Поле ввода сообщения")).toHaveValue("Строка 1\nСтрока 2");
    });

    // Switch model preference -> creates new AgentConfigVersion on the SAME AgentId
    await user.selectOptions(
      screen.getByLabelText("Предпочитаемая модель агента"),
      "gemini-3.8-pro",
    );
    await waitFor(() => {
      expect(screen.getByTestId("agent-config-version")).toHaveTextContent("Главный агент · v2");
    });
    expect(bridge.calls.filter((c) => c.method === "createAgent")).toHaveLength(0);
    expect(bridge.agents).toHaveLength(1);
    expect(bridge.agents[0]?.agent_id).toBe(agent.agent_id);
    expect(bridge.agents[0]?.current_config.version_number).toBe(2);

    // Fail next send -> error alert shown AND unsent draft preserved losslessly
    bridge.failNextSendWith = { code: "RUNTIME_PROTOCOL", message: "временный сбой runtime" };
    await user.click(screen.getByTestId("send-button"));

    expect(await screen.findByTestId("error-banner")).toHaveTextContent("временный сбой runtime");
    expect(screen.getByLabelText("Поле ввода сообщения")).toHaveValue("Строка 1\nСтрока 2");
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

    // Stop control MUST live outside message timeline and workspace panel
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

    // Start a run during memoryd outage — Stop must remain active and functional
    const input = screen.getByLabelText("Поле ввода сообщения");
    await user.type(input, "Работа при отказе memoryd{enter}");
    const stopBtn = await screen.findByTestId("global-stop-button");
    expect(stopBtn).toBeEnabled();
    await user.click(stopBtn);
    expect(await screen.findByTestId("cancelled-run-notice")).toBeInTheDocument();

    // Recover memoryd and flush outbox manually
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

    // 1. Live Google AI Ultra (oauth-personal) -> zero SIMULATED badge
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

    // 2. Live Gemini API key profile -> explicit Gemini API key label, zero SIMULATED badge
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

    // 3. Simulated runtime -> unmistakable SIMULATED warning pill
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

  it("toggles right sibling Artifacts card cleanly and renders adversarial content safely without XSS", async () => {
    const user = userEvent.setup();
    const { project, agent, sessionA } = makeSeedFixture();
    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [project],
      agents: [agent],
      sessions: [sessionA],
    });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);
    await screen.findByTestId("chat-card");

    // Closed by default
    expect(screen.queryByTestId("workspace-sibling-panel")).not.toBeInTheDocument();

    const toggleBtn = screen.getByRole("button", {
      name: "Переключить панель рабочего пространства и артефактов",
    });
    expect(toggleBtn).toHaveAttribute("aria-expanded", "false");

    await user.click(toggleBtn);
    expect(toggleBtn).toHaveAttribute("aria-expanded", "true");

    // Verify sibling rounded card architecture: panel is a sibling of chat-card inside workspace-stage
    const stage = screen.getByTestId("workspace-stage");
    const chatCard = screen.getByTestId("chat-card");
    const panel = screen.getByTestId("workspace-sibling-panel");
    expect(stage.children).toHaveLength(2);
    expect(stage.children[0]).toBe(chatCard);
    expect(stage.children[1]).toBe(panel);
    expect(chatCard.contains(panel)).toBe(false);

    // Create artifact with XSS payload and prove pure safe text rendering
    await user.click(screen.getByTestId("toggle-create-artifact-button"));
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

    // Collapse right panel again
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

    // Verify owner-direction-v5 rule: zero CSS backdrop-filter hacks in desktop stylesheets
    for (const css of [tokensCss, desktopCss]) {
      const withoutComments = css.replace(/\/\*[\s\S]*?\*\//g, "");
      expect(withoutComments.toLowerCase()).not.toContain("backdrop-filter");
    }
  });

  it("isolates memory scopes across multiple projects without binding Project 2 to Project 1's agent", async () => {
    const user = userEvent.setup();
    const bridge = new MockDesktopBridge({ connected: true });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);
    expect(await screen.findByTestId("empty-onboarding-state")).toBeInTheDocument();

    // Create Project 1 via onboarding with explicit scope `project:alpha`
    const nameInput = screen.getByLabelText("Название проекта");
    await user.clear(nameInput);
    await user.type(nameInput, "Alpha");
    await user.type(screen.getByLabelText("Каталог рабочей области"), "C:/work/alpha");
    await user.type(screen.getByLabelText("Область памяти (необяз.)"), "project:alpha");
    await user.click(screen.getByRole("button", { name: "Создать проект и начать чат" }));

    await screen.findByLabelText("Поле ввода сообщения");
    expect(bridge.projects).toHaveLength(1);
    expect(bridge.agents).toHaveLength(1);
    expect(bridge.agents[0]?.current_config.allowed_memory_scopes).toEqual(["project:alpha"]);

    // Create Project 2 via sidebar with explicit scope `project:beta`
    await user.click(screen.getByRole("button", { name: "Создать проект" }));
    await user.type(screen.getByLabelText("Название нового проекта"), "Beta");
    await user.type(screen.getByLabelText("Путь к рабочей папке"), "C:/work/beta");
    await user.type(screen.getByLabelText("Область памяти проекта"), "project:beta");
    await user.click(screen.getByRole("button", { name: "Создать" }));

    await waitFor(() => {
      expect(bridge.projects).toHaveLength(2);
      expect(bridge.sessions).toHaveLength(2);
    });

    // Must create a dedicated agent for `project:beta` (zero cross-project scope lockout/bleed)
    expect(bridge.agents).toHaveLength(2);
    const alphaProject = bridge.projects.find((p) => p.memory_scope === "project:alpha")!;
    const betaProject = bridge.projects.find((p) => p.memory_scope === "project:beta")!;
    const alphaSession = bridge.sessions.find((s) => s.project_id === alphaProject.project_id)!;
    const betaSession = bridge.sessions.find((s) => s.project_id === betaProject.project_id)!;
    const alphaAgent = bridge.agents.find((a) => a.agent_id === alphaSession.agent_id)!;
    const betaAgent = bridge.agents.find((a) => a.agent_id === betaSession.agent_id)!;

    expect(alphaAgent.agent_id).not.toBe(betaAgent.agent_id);
    expect(alphaAgent.current_config.allowed_memory_scopes).toEqual(["project:alpha"]);
    expect(betaAgent.current_config.allowed_memory_scopes).toEqual(["project:beta"]);

    // Creating another chat inside Beta reuses Beta's dedicated agent rather than spawning a third
    await user.click(screen.getByRole("button", { name: "Новый чат в проекте Beta" }));
    await user.type(screen.getByLabelText("Название нового чата"), "Beta Chat 2");
    await user.click(screen.getByRole("button", { name: "Открыть" }));

    await waitFor(() => {
      expect(bridge.sessions).toHaveLength(3);
    });
    expect(bridge.agents).toHaveLength(2);
    const betaChat2 = bridge.sessions.find((s) => s.title === "Beta Chat 2")!;
    expect(betaChat2.agent_id).toBe(betaAgent.agent_id);
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
            sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
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
            sha256: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
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

    // Start an in-flight turn in Session A
    const input = screen.getByLabelText("Поле ввода сообщения");
    await user.type(input, "Запрос в Сессию A{enter}");
    await screen.findByTestId("running-control-bar");

    // Immediately switch to Session B while Session A's turn is still running
    await user.click(screen.getByRole("button", { name: /Сессия B/ }));
    expect(await screen.findByText("Уникальное сообщение Сессии B")).toBeInTheDocument();
    expect(screen.getByTestId("artifact-preview")).toHaveTextContent("artifact-from-b.md");

    // Wait for Session A's turn to resolve in the background
    await waitFor(
      () => {
        expect(screen.queryByTestId("running-control-bar")).not.toBeInTheDocument();
      },
      { timeout: 2_000 },
    );

    // Session B MUST remain cleanly displayed (zero clobbering from Session A's late response)
    expect(screen.getByText("Уникальное сообщение Сессии B")).toBeInTheDocument();
    expect(screen.queryByText("Запрос в Сессию A")).not.toBeInTheDocument();
    expect(screen.getByTestId("artifact-preview")).toHaveTextContent("artifact-from-b.md");
    expect(screen.getByTestId("artifact-preview")).not.toHaveTextContent("artifact-from-a.md");

    // Switching back to Session A loads its completed turn intact
    await user.click(screen.getByRole("button", { name: /Сессия A/ }));
    expect(await screen.findByText("Запрос в Сессию A")).toBeInTheDocument();
    expect(await screen.findByText(/Принято .* Запрос в Сессию A/)).toBeInTheDocument();
  });
});
