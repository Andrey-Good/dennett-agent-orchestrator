import { readFileSync } from "node:fs";
import path from "node:path";

import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { WindowControls } from "../api/bridge";
import type { AgentRecord, MessageRecord, ProjectRecord, SessionRecord } from "../api/contracts";
import { MockDesktopBridge } from "../api/mockBridge";
import { App } from "../App";

const desktopCss = readFileSync(path.resolve(__dirname, "../styles/desktop.css"), "utf8");

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

function makeFixture() {
  const now = Date.now();
  const projectA: ProjectRecord = {
    project_id: "10000000-0000-4000-8000-00000000c101",
    name: "Проект Альфа",
    workspace_path: "A:/Dev/alpha",
    memory_scope: "project:alpha",
    lifecycle_state: "active",
    archived_at: null,
    pinned: false,
    pinned_at: null,
    created_at: now - 30_000,
  };
  const projectB: ProjectRecord = {
    project_id: "10000000-0000-4000-8000-00000000c102",
    name: "Проект Бета",
    workspace_path: "A:/Dev/beta",
    memory_scope: "project:beta",
    lifecycle_state: "active",
    archived_at: null,
    pinned: false,
    pinned_at: null,
    created_at: now - 20_000,
  };
  const projectC: ProjectRecord = {
    project_id: "10000000-0000-4000-8000-00000000c103",
    name: "Проект Гамма",
    workspace_path: null,
    memory_scope: "project:gamma",
    lifecycle_state: "active",
    archived_at: null,
    pinned: false,
    pinned_at: null,
    created_at: now - 40_000,
  };
  const agent: AgentRecord = {
    agent_id: "20000000-0000-4000-8000-00000000c101",
    name: "Агент Альфа",
    description: "Агент проекта",
    current_config_version_id: "21000000-0000-4000-8000-00000000c101",
    current_config: {
      config_version_id: "21000000-0000-4000-8000-00000000c101",
      agent_id: "20000000-0000-4000-8000-00000000c101",
      version_number: 1,
      system_prompt: "Помогай проекту.",
      model_preference: "gemini-4-neon",
      thinking_level: "medium",
      allowed_memory_scopes: ["project:alpha"],
      workspace_write_policy: { mode: "ask_before_write" },
      created_at: now - 30_000,
    },
    created_at: now - 30_000,
    updated_at: now - 30_000,
  };
  const projChat1: SessionRecord = {
    session_id: "30000000-0000-4000-8000-00000000c101",
    agent_id: agent.agent_id,
    project_id: projectA.project_id,
    title: "Чат проекта 1",
    session_kind: "user_chat",
    runtime_binding: null,
    status: "active",
    pinned: false,
    pinned_at: null,
    created_at: now - 25_000,
    updated_at: now - 5_000,
  };
  const projChat2: SessionRecord = {
    session_id: "30000000-0000-4000-8000-00000000c102",
    agent_id: agent.agent_id,
    project_id: projectA.project_id,
    title: "Чат проекта 2",
    session_kind: "user_chat",
    runtime_binding: null,
    status: "active",
    pinned: false,
    pinned_at: null,
    created_at: now - 24_000,
    updated_at: now - 15_000,
  };

  return { now, projectA, projectB, projectC, agent, projChat1, projChat2 };
}

describe("Standalone Chats, Two-Zone Split Sidebar, Pinning & Message Copy", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("creates standalone chat (project_id: null) in 1 click, renders Чаты breadcrumbs, standalone empty copy, locked read-only composer with live Model/Thinking, and standalone WorkspacePanel with working artifacts (zero listProjectFiles)", async () => {
    const user = userEvent.setup();
    const { projectA, agent, projChat1 } = makeFixture();
    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [projectA],
      agents: [agent],
      sessions: [projChat1],
      filesByProject: {
        [projectA.project_id]: [
          { relative_path: "README.md", name: "README.md", kind: "file", size_bytes: 120, depth: 1 },
        ],
      },
    });

    const { container } = render(
      <App
        bridge={bridge}
        windowControls={createTestWindowControls()}
        initialRightPanelOpen={true}
      />,
    );
    await screen.findByTestId("chat-card");
    expect(screen.getByTestId("standalone-chats-empty")).toHaveTextContent(
      "Одиночных чатов пока нет",
    );

    const listFilesCallsBefore = bridge.calls.filter((c) => c.method === "listProjectFiles").length;

    // 1-click create standalone chat via '+' in Чаты header
    await user.click(screen.getByTestId("new-standalone-chat-btn"));

    await waitFor(() => {
      expect(bridge.sessions.some((s) => s.project_id === null)).toBe(true);
    });
    const standaloneSess = bridge.sessions.find((s) => s.project_id === null)!;
    expect(standaloneSess.title).toBe("Новый чат");
    expect(standaloneSess.pinned).toBe(false);

    // Standalone agent provisioned with chat:standalone scope, deny_all, and default catalog model
    const standaloneAgent = bridge.agents.find((a) => a.agent_id === standaloneSess.agent_id)!;
    expect(standaloneAgent.current_config.allowed_memory_scopes).toEqual(["chat:standalone"]);
    expect(standaloneAgent.current_config.workspace_write_policy).toEqual({ mode: "deny_all" });
    expect(standaloneAgent.current_config.model_preference).toBe("gemini-4-neon");

    // Breadcrumbs show "Чаты / Новый чат" without project name
    const breadcrumbs = screen.getByLabelText("Текущий контекст");
    expect(breadcrumbs).toHaveTextContent("Чаты");
    expect(breadcrumbs).toHaveTextContent("Новый чат");
    expect(breadcrumbs).not.toHaveTextContent("Проект Альфа");

    // Standalone empty state copy + suggestions
    const emptyState = await screen.findByTestId("empty-chat-state");
    expect(emptyState).toHaveTextContent("Новый чат");
    expect(emptyState).toHaveTextContent(
      "Задайте вопрос или обсудите идею без привязки к папке проекта",
    );

    // Composer locks write policy to "Только чтение" while keeping live Model & Thinking popovers working
    const accessTrigger = screen.getByTestId("access-selector-trigger");
    expect(accessTrigger).toBeDisabled();
    expect(accessTrigger).toHaveTextContent("Только чтение");

    await user.click(screen.getByTestId("model-selector-trigger"));
    expect(await screen.findByTestId("model-popover")).toHaveTextContent("Gemini 4 Carbon");
    await user.click(screen.getByTestId("model-option-gemini-4-carbon"));
    await waitFor(() => {
      expect(
        bridge.agents.find((a) => a.agent_id === standaloneAgent.agent_id)?.current_config
          .model_preference,
      ).toBe("gemini-4-carbon");
    });

    // WorkspacePanel hides Файлы проекта, shows standalone note, never calls listProjectFiles again,
    // and keeps Артефакты сессии 100% functional!
    expect(screen.queryByTestId("workspace-files-section")).not.toBeInTheDocument();
    expect(screen.getByTestId("workspace-standalone-note")).toHaveTextContent(
      "Одиночный чат без привязки к папке",
    );
    expect(bridge.calls.filter((c) => c.method === "listProjectFiles")).toHaveLength(
      listFilesCallsBefore,
    );

    // Create inline artifact directly in standalone session (project_id: null)
    await user.click(screen.getByTestId("toggle-create-artifact-button"));
    await user.type(screen.getByLabelText("Имя артефакта"), "standalone-idea.md");
    await user.type(screen.getByLabelText("Содержимое артефакта"), "# Черновик идеи\n\nПункт 1");
    await user.click(screen.getByRole("button", { name: "Сохранить" }));

    const artPreview = await screen.findByTestId("artifact-preview");
    expect(artPreview).toHaveTextContent("standalone-idea.md");
    expect(screen.getByTestId("artifact-preview-markdown")).toHaveTextContent("Черновик идеи");
    expect(screen.getByTestId("artifact-sha256")).toBeInTheDocument();
    expect(bridge.artifactsBySession[standaloneSess.session_id]?.[0]?.project_id).toBeNull();

    // Send live message in standalone chat -> auto-titles standalone chat & shows reply
    const composer = screen.getByLabelText("Поле ввода сообщения");
    await user.type(composer, "Объясни теорему Байеса на примере{Enter}");
    expect(await screen.findByText(/Принято .* Объясни теорему Байеса/)).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getByTestId("breadcrumb-session-title")).toHaveTextContent(
        "Объясни теорему Байеса на примере",
      );
    });

    // Clickable breadcrumb rename works on standalone chat
    await user.click(screen.getByTestId("breadcrumb-session-title"));
    const renameInput = await screen.findByTestId("breadcrumb-rename-input");
    await user.clear(renameInput);
    await user.type(renameInput, "Байес — конспект{Enter}");
    await waitFor(() => {
      expect(screen.getByTestId("breadcrumb-session-title")).toHaveTextContent("Байес — конспект");
    });

    // Zero <select> elements anywhere
    expect(container.querySelectorAll("select")).toHaveLength(0);
  });

  it("pins and unpins projects and chats (project + standalone) with deterministic pinned DESC, pinned_at DESC, timestamp DESC order", async () => {
    const user = userEvent.setup();
    const { now, projectA, projectB, projectC, agent, projChat1, projChat2 } = makeFixture();
    const standalone1: SessionRecord = {
      session_id: "30000000-0000-4000-8000-00000000s101",
      agent_id: agent.agent_id,
      project_id: null,
      title: "Одиночный новый",
      session_kind: "user_chat",
      status: "active",
      pinned: false,
      pinned_at: null,
      created_at: now - 10_000,
      updated_at: now - 2_000,
    };
    const standalone2: SessionRecord = {
      session_id: "30000000-0000-4000-8000-00000000s102",
      agent_id: agent.agent_id,
      project_id: null,
      title: "Одиночный старый",
      session_kind: "user_chat",
      status: "active",
      pinned: false,
      pinned_at: null,
      created_at: now - 30_000,
      updated_at: now - 20_000,
    };

    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [projectA, projectB, projectC],
      agents: [agent],
      sessions: [projChat1, projChat2, standalone1, standalone2],
    });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);
    await screen.findByTestId("chat-card");

    const getProjectOrder = () =>
      Array.from(
        screen.getByTestId("sidebar-projects-scroll").querySelectorAll<HTMLElement>("[data-testid^='project-group-']"),
      ).map((el) => el.getAttribute("data-testid")!.replace("project-group-", ""));

    // Initial order by created_at DESC: B (-20s), A (-30s), C (-40s)
    expect(getProjectOrder()).toEqual([
      projectB.project_id,
      projectA.project_id,
      projectC.project_id,
    ]);

    // Pin project C (oldest) via portaled '…' menu -> moves to top with filled pin indicator
    await user.click(screen.getByTestId(`project-menu-trigger-${projectC.project_id}`));
    await user.click(await screen.findByTestId(`project-menu-pin-${projectC.project_id}`));
    await waitFor(() => {
      expect(getProjectOrder()).toEqual([
        projectC.project_id,
        projectB.project_id,
        projectA.project_id,
      ]);
    });
    expect(screen.getByTestId(`project-pin-indicator-${projectC.project_id}`)).toBeInTheDocument();

    // Pin project A -> newer pinned_at moves A above C!
    await user.click(screen.getByTestId(`project-menu-trigger-${projectA.project_id}`));
    await user.click(await screen.findByTestId(`project-menu-pin-${projectA.project_id}`));
    await waitFor(() => {
      expect(getProjectOrder()).toEqual([
        projectA.project_id,
        projectC.project_id,
        projectB.project_id,
      ]);
    });

    // Unpin project A -> drops back below pinned C (and below newer unpinned B)
    await user.click(screen.getByTestId(`project-menu-trigger-${projectA.project_id}`));
    const unpinItem = await screen.findByTestId(`project-menu-pin-${projectA.project_id}`);
    expect(unpinItem).toHaveTextContent("Открепить");
    await user.click(unpinItem);
    await waitFor(() => {
      expect(getProjectOrder()).toEqual([
        projectC.project_id,
        projectB.project_id,
        projectA.project_id,
      ]);
    });
    expect(
      screen.queryByTestId(`project-pin-indicator-${projectA.project_id}`),
    ).not.toBeInTheDocument();

    // Project chat pinning: projChat2 (older) pinned -> jumps above projChat1
    const getProjectAChatOrder = () =>
      Array.from(
        screen
          .getByTestId(`project-group-${projectA.project_id}`)
          .querySelectorAll<HTMLElement>("[data-testid^='session-item-']"),
      ).map((el) => el.getAttribute("data-testid")!.replace("session-item-", ""));

    expect(getProjectAChatOrder()).toEqual([projChat1.session_id, projChat2.session_id]);
    await user.click(screen.getByTestId(`session-pin-btn-${projChat2.session_id}`));
    await waitFor(() => {
      expect(getProjectAChatOrder()).toEqual([projChat2.session_id, projChat1.session_id]);
    });
    expect(screen.getByTestId(`session-pin-btn-${projChat2.session_id}`)).toHaveAttribute(
      "aria-pressed",
      "true",
    );

    // Standalone chat pinning: standalone2 (older) pinned -> jumps above standalone1; unpin restores order
    const getStandaloneOrder = () =>
      Array.from(
        screen
          .getByTestId("standalone-chats-list")
          .querySelectorAll<HTMLElement>("[data-testid^='session-item-']"),
      ).map((el) => el.getAttribute("data-testid")!.replace("session-item-", ""));

    expect(getStandaloneOrder()).toEqual([standalone1.session_id, standalone2.session_id]);
    await user.click(screen.getByTestId(`session-pin-btn-${standalone2.session_id}`));
    await waitFor(() => {
      expect(getStandaloneOrder()).toEqual([standalone2.session_id, standalone1.session_id]);
    });
    await user.click(screen.getByTestId(`session-pin-btn-${standalone2.session_id}`));
    await waitFor(() => {
      expect(getStandaloneOrder()).toEqual([standalone1.session_id, standalone2.session_id]);
    });

    // Deleting a project leaves standalone chats completely intact
    await user.click(screen.getByTestId(`project-menu-trigger-${projectB.project_id}`));
    await user.click(await screen.findByTestId(`project-menu-delete-${projectB.project_id}`));
    await user.click(await screen.findByTestId("project-delete-confirm-btn"));
    await waitFor(() => {
      expect(screen.queryByTestId(`project-group-${projectB.project_id}`)).not.toBeInTheDocument();
    });
    expect(getStandaloneOrder()).toEqual([standalone1.session_id, standalone2.session_id]);
  });

  it("enforces exact two-zone Sidebar split-scroll CSS geometry (66.666% max / 33.334% min) and supports context-aware Ctrl+N + CommandPalette standalone search", async () => {
    const user = userEvent.setup();
    const { now, projectA, agent, projChat1 } = makeFixture();
    const standaloneChat: SessionRecord = {
      session_id: "30000000-0000-4000-8000-00000000s201",
      agent_id: agent.agent_id,
      project_id: null,
      title: "Философия науки",
      session_kind: "user_chat",
      status: "active",
      pinned: false,
      pinned_at: null,
      created_at: now - 12_000,
      updated_at: now - 12_000,
    };

    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [projectA],
      agents: [agent],
      sessions: [projChat1, standaloneChat],
    });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);
    await screen.findByTestId("chat-card");

    // Two-zone DOM + exact CSS split rules
    expect(screen.getByTestId("sidebar-projects-section")).toBeInTheDocument();
    expect(screen.getByTestId("sidebar-projects-scroll")).toBeInTheDocument();
    expect(screen.getByTestId("sidebar-chats-section")).toBeInTheDocument();
    expect(screen.getByTestId("sidebar-chats-scroll")).toBeInTheDocument();

    expect(desktopCss).toMatch(
      /\.sidebar-projects-section\s*\{[^}]*flex:\s*0\s+1\s+auto;[^}]*max-height:\s*66\.666%;[^}]*min-height:\s*0;/s,
    );
    expect(desktopCss).toMatch(
      /\.sidebar-chats-section\s*\{[^}]*flex:\s*1\s+1\s+33\.334%;[^}]*min-height:\s*33\.334%;/s,
    );
    expect(desktopCss).toMatch(
      /\.sidebar-projects-scroll\s*\{[^}]*flex:\s*1\s+1\s+auto;[^}]*min-height:\s*0;[^}]*overflow-y:\s*auto;/s,
    );
    expect(desktopCss).toMatch(
      /\.sidebar-chats-scroll\s*\{[^}]*flex:\s*1\s+1\s+auto;[^}]*min-height:\s*0;[^}]*overflow-y:\s*auto;/s,
    );

    // Ctrl+N while viewing a project chat -> creates a project chat
    await user.keyboard("{Control>}n{/Control}");
    await waitFor(() => {
      expect(bridge.sessions.filter((s) => s.project_id === projectA.project_id)).toHaveLength(2);
    });

    // Open CommandPalette -> searches standalone chat AND offers "Новый одиночный чат"
    await user.keyboard("{Control>}k{/Control}");
    const palette = await screen.findByTestId("command-palette");
    expect(within(palette).getByTestId("palette-item-action:new-standalone-chat")).toHaveTextContent(
      "Новый одиночный чат",
    );
    const paletteInput = within(palette).getByLabelText("Поиск в командной палитре");

    // CHATS-UI-02: matching a quick action ("одиночный") must NOT render the empty-state notice!
    await user.type(paletteInput, "одиночный");
    expect(
      within(palette).getByTestId("palette-item-action:new-standalone-chat"),
    ).toBeInTheDocument();
    expect(within(palette).queryByTestId("command-palette-empty")).not.toBeInTheDocument();

    // Truly unmatched query shows honest "По запросу «…» ничего не найдено."
    await user.clear(paletteInput);
    await user.type(paletteInput, "совсем-нет-совпадений-404");
    expect(await within(palette).findByTestId("command-palette-empty")).toHaveTextContent(
      "По запросу «совсем-нет-совпадений-404» ничего не найдено.",
    );

    await user.clear(paletteInput);
    await user.type(paletteInput, "Философия");
    const standalonePaletteItem = await within(palette).findByTestId(
      `palette-item-session:${standaloneChat.session_id}`,
    );
    expect(standalonePaletteItem).toHaveTextContent("Чаты");
    await user.click(standalonePaletteItem);

    await waitFor(() => {
      expect(screen.getByTestId("breadcrumb-session-title")).toHaveTextContent("Философия науки");
    });

    // Ctrl+N while viewing a standalone chat -> creates a new standalone chat!
    await user.keyboard("{Control>}n{/Control}");
    await waitFor(() => {
      expect(bridge.sessions.filter((s) => s.project_id === null)).toHaveLength(2);
    });
  });

  it("renders Copy button on BOTH user and assistant messages copying raw Markdown/code content verbatim with 'Скопировано' feedback", async () => {
    const user = userEvent.setup();
    const { now, projectA, agent, projChat1 } = makeFixture();
    const rawUserContent = "Проверь формулу `E = mc^2` и список:\n- пункт 1\n- пункт 2";
    const rawAssistantContent =
      "Вот **точный** пример кода:\n\n```ts\nconst answer = 42;\nconsole.log(answer);\n```";

    const userMsg: MessageRecord = {
      message_id: "50000000-0000-4000-8000-00000000cp01",
      session_id: projChat1.session_id,
      role: "user",
      content: rawUserContent,
      run_id: null,
      correlation_id: "corr-copy-1",
      created_at: now - 4_000,
    };
    const assistantMsg: MessageRecord = {
      message_id: "50000000-0000-4000-8000-00000000cp02",
      session_id: projChat1.session_id,
      role: "assistant",
      content: rawAssistantContent,
      run_id: null,
      correlation_id: "corr-copy-1",
      created_at: now - 3_000,
    };

    const writeTextMock = vi.fn(async () => {});
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: writeTextMock },
    });

    const bridge = new MockDesktopBridge({
      connected: true,
      projects: [projectA],
      agents: [agent],
      sessions: [projChat1],
      messagesBySession: {
        [projChat1.session_id]: [userMsg, assistantMsg],
      },
    });

    render(<App bridge={bridge} windowControls={createTestWindowControls()} />);
    await screen.findByTestId("chat-card");

    // Copy user message -> copies raw userMsg.content verbatim and shows "Скопировано"
    const userCopyBtn = await screen.findByTestId(`message-copy-btn-${userMsg.message_id}`);
    await user.click(userCopyBtn);
    expect(writeTextMock).toHaveBeenCalledWith(rawUserContent);
    expect(userCopyBtn).toHaveTextContent("Скопировано");

    // Copy assistant message -> copies raw Markdown + fenced code block verbatim and shows "Скопировано"
    const assistantCopyBtn = screen.getByTestId(`message-copy-btn-${assistantMsg.message_id}`);
    await user.click(assistantCopyBtn);
    expect(writeTextMock).toHaveBeenCalledWith(rawAssistantContent);
    expect(assistantCopyBtn).toHaveTextContent("Скопировано");

    // CHATS-UI-01: Copy pills sit strictly clear of assistant line 1, user bubble text, and top fade mask
    expect(desktopCss).toMatch(/\.message-copy-button\s*\{[^}]*top:\s*-15px;[^}]*right:\s*8px;/s);
    expect(desktopCss).toMatch(
      /\.message--assistant\s+\.message-copy-button\s*\{[^}]*top:\s*-26px;[^}]*right:\s*0;/s,
    );
    expect(desktopCss).toMatch(/\.message-turn-group:first-child\s*\{\s*margin-top:\s*10px;\s*\}/s);
  });
});
