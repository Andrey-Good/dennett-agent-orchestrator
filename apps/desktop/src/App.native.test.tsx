import React from "react";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";

const tauri = vi.hoisted(() => {
  const channelHandlers: Array<(event: unknown) => void> = [];
  const closeHandlers: Array<(event: { preventDefault(): void }) => void> = [];
  class TestChannel {
    constructor(onMessage: (event: unknown) => void) {
      channelHandlers.push(onMessage);
    }
  }
  return {
    channelHandlers,
    closeHandlers,
    invoke: vi.fn(),
    TestChannel,
    closeWindow: vi.fn(async () => undefined),
    minimizeWindow: vi.fn(async () => undefined),
    maximizeWindow: vi.fn(async () => undefined),
    onCloseRequested: vi.fn(async (handler: (event: { preventDefault(): void }) => void) => {
      closeHandlers.push(handler);
      return () => {
        const index = closeHandlers.indexOf(handler);
        if (index >= 0) closeHandlers.splice(index, 1);
      };
    }),
  };
});

vi.mock("@tauri-apps/api/core", () => ({
  Channel: tauri.TestChannel,
  invoke: tauri.invoke,
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    close: tauri.closeWindow,
    minimize: tauri.minimizeWindow,
    toggleMaximize: tauri.maximizeWindow,
    onCloseRequested: tauri.onCloseRequested,
  }),
}));

const projectId = "00000000-0000-7000-8000-000000000010";
const sessionId = "00000000-0000-7000-8000-000000000020";
const secondSessionId = "00000000-0000-7000-8000-000000000021";
const commandId = "00000000-0000-7000-8000-000000000030";
const userTurnId = "00000000-0000-7000-8000-000000000040";
const agentTurnId = "00000000-0000-7000-8000-000000000050";

const sessionSummary = {
  sessionId,
  projectId,
  title: "Timeout recovery",
  state: "session_state_idle",
  revision: "4",
  activeTurnId: null,
  lastActivityAtUnixMs: 1_750_000_000_000,
};

const timedOutSession = {
  session: sessionSummary,
  fingerprint: "timeout-fingerprint",
  turns: [
    {
      turnId: userTurnId,
      commandId,
      role: "turn_role_user",
      state: "turn_state_completed",
      text: "Retry the owner request",
      activities: [],
      outcome: null,
      createdAtUnixMs: 1_750_000_000_000,
      completedAtUnixMs: 1_750_000_000_000,
    },
    {
      turnId: agentTurnId,
      commandId,
      role: "turn_role_agent",
      state: "turn_state_timed_out",
      text: "## Result\n\n- Retained partial answer",
      activities: [{
        activityId: "activity-1",
        phase: "command",
        message: null,
        status: "turn_activity_status_completed",
        updatedAtUnixMs: 1_750_000_015_000,
      }],
      outcome: { kind: "result", summary: "Retained partial answer", partial: true },
      createdAtUnixMs: 1_750_000_000_001,
      completedAtUnixMs: 1_750_000_026_001,
    },
  ],
};

const secondSessionSummary = {
  ...sessionSummary,
  sessionId: secondSessionId,
  title: "Second chat",
  revision: "1",
  lastActivityAtUnixMs: 1_750_000_100_000,
};

const secondSession = {
  session: secondSessionSummary,
  fingerprint: "second-fingerprint",
  turns: [],
};

const system = {
  revision: "4",
  authorityEpoch: "7",
  observedAtUnixMs: 1_750_000_000_000,
  projects: [{
    projectId,
    displayName: "Dennett native test",
    state: "project_state_ready",
    revision: "1",
    lastActivityAtUnixMs: 1_750_000_000_000,
  }],
  recentSessions: [sessionSummary],
  activeProjectId: projectId,
  activeSessionId: sessionId,
  nodeState: "health_state_ready",
  runtime: {
    adapterId: "openai.codex.sdk",
    runtimeKind: "native_agent",
    streaming: true,
    continuation: true,
    scopedCancellation: true,
    deadlines: true,
  },
};

describe("native Project Chat recovery", () => {
  beforeEach(() => {
    tauri.channelHandlers.splice(0);
    tauri.closeHandlers.splice(0);
    tauri.invoke.mockReset();
    tauri.closeWindow.mockClear();
    tauri.minimizeWindow.mockClear();
    tauri.maximizeWindow.mockClear();
    tauri.onCloseRequested.mockClear();
    tauri.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "native_mica_available") return true;
      if (command === "open_project_chat") {
        return {
          correlationId: "native-timeout-correlation",
          subscriptionId: "native-timeout-subscription",
          system,
          session: timedOutSession,
          draft: null,
        };
      }
      if (command === "close_project_chat") return true;
      if (command === "send_project_turn") {
        const request = args?.request as Record<string, unknown>;
        return { commandId: request.commandId, turnId: "00000000-0000-7000-8000-000000000060" };
      }
      throw new Error(`Unexpected native command: ${command}`);
    });
    Object.defineProperty(window, "__TAURI_INTERNALS__", { configurable: true, value: {} });
  });

  afterEach(() => {
    Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
    document.documentElement.classList.remove("native-shell", "native-mica-unavailable");
  });

  it("shows retained partial output and retries the same request through a new stable command", async () => {
    const user = userEvent.setup();
    render(<App />);

    expect(await screen.findByText("Timed out")).toBeVisible();
    expect(screen.getByText("Retained partial answer")).toBeVisible();
    expect(screen.getByRole("heading", { name: "Result" })).toBeVisible();
    expect(screen.getByText("Ran command")).toBeVisible();
    expect(screen.getByText("Timed out after 26s")).toBeVisible();
    expect(screen.queryByText("now")).not.toBeInTheDocument();
    const createdTime = new Date(1_750_000_000_001).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    const completedTime = new Date(1_750_000_026_001).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    expect(createdTime).not.toBe(completedTime);
    expect(screen.getByLabelText("agent message")).toHaveTextContent(createdTime);
    expect(screen.getByLabelText("agent message")).not.toHaveTextContent(completedTime);
    expect(screen.queryByLabelText("Workspace resources")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Plugins" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Agent runtime: Codex, Native agent/i }));
    expect(screen.getByText(/did not publish selectable model, reasoning or speed options/i)).toBeVisible();
    expect(screen.queryByText("Provider default")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Medium" })).not.toBeInTheDocument();
    const retry = screen.getByRole("button", { name: "Retry" });
    await user.click(retry);

    await waitFor(() => {
      const call = tauri.invoke.mock.calls.find(([command]) => command === "send_project_turn");
      expect(call?.[1]).toMatchObject({
        request: {
          projectId,
          sessionId,
          expectedRevision: "4",
          text: "Retry the owner request",
        },
      });
      const request = (call?.[1] as { request: { commandId: string } }).request;
      expect(request.commandId).not.toBe(commandId);
      expect(request.commandId).toMatch(/^[0-9a-f-]{36}$/i);
    });
    expect(screen.getByText("Retry accepted by the local Node.")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => {
      const retries = tauri.invoke.mock.calls.filter(([command]) => command === "send_project_turn");
      expect(retries).toHaveLength(2);
      const firstCommand = (retries[0][1] as { request: { commandId: string } }).request.commandId;
      const secondCommand = (retries[1][1] as { request: { commandId: string } }).request.commandId;
      expect(secondCommand).toBe(firstCommand);
    });
  });

  it("does not let a late save receipt erase the next draft", async () => {
    let resolveFirstSave: ((value: { commandId: string; state: string }) => void) | null = null;
    let saveCount = 0;
    tauri.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "native_mica_available") return true;
      if (command === "open_project_chat") {
        return {
          correlationId: "native-draft-correlation",
          subscriptionId: "native-draft-subscription",
          system,
          session: timedOutSession,
          draft: null,
        };
      }
      if (command === "close_project_chat") return true;
      if (command === "save_composer_draft") {
        const request = args?.request as { commandId: string };
        saveCount += 1;
        if (saveCount === 1) {
          return new Promise<{ commandId: string; state: string }>((resolve) => {
            resolveFirstSave = resolve;
          });
        }
        return { commandId: request.commandId, state: "composer_draft_write_state_saved" };
      }
      if (command === "send_project_turn") {
        const request = args?.request as Record<string, unknown>;
        return { commandId: request.commandId, turnId: "00000000-0000-7000-8000-000000000060" };
      }
      throw new Error(`Unexpected native command: ${command}`);
    });
    const user = userEvent.setup();
    render(<App />);

    const composer = await screen.findByPlaceholderText("Ask the project agent…");
    await user.type(composer, "first request");
    await waitFor(() => {
      expect(tauri.invoke.mock.calls.filter(([command]) => command === "save_composer_draft")).toHaveLength(1);
    }, { timeout: 2_000 });
    const firstSave = tauri.invoke.mock.calls.find(([command]) => command === "save_composer_draft");
    const firstCommandId = ((firstSave?.[1] as { request: { commandId: string } }).request.commandId);

    await user.click(screen.getByRole("button", { name: "Send message" }));
    await waitFor(() => expect(composer).toHaveValue(""));
    await user.type(composer, "second draft");
    await act(async () => {
      resolveFirstSave?.({
        commandId: firstCommandId,
        state: "composer_draft_write_state_already_accepted",
      });
    });

    await waitFor(() => expect(composer).toHaveValue("second draft"));
  });

  it("blocks Send while another chat is opening and then targets only that chat", async () => {
    let resolveSecondOpen: ((value: unknown) => void) | null = null;
    const systemWithTwoChats = {
      ...system,
      recentSessions: [sessionSummary, secondSessionSummary],
    };
    tauri.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "native_mica_available") return true;
      if (command === "open_project_chat") {
        const request = args?.request as { sessionId: string | null };
        if (request.sessionId === secondSessionId) {
          return new Promise((resolve) => {
            resolveSecondOpen = resolve;
          });
        }
        return {
          correlationId: "native-switch-correlation",
          subscriptionId: "native-switch-subscription-a",
          system: systemWithTwoChats,
          session: timedOutSession,
          draft: null,
        };
      }
      if (command === "close_project_chat") return true;
      if (command === "send_project_turn") {
        const request = args?.request as Record<string, unknown>;
        return { commandId: request.commandId, turnId: "00000000-0000-7000-8000-000000000061" };
      }
      if (command === "save_composer_draft") {
        const request = args?.request as { commandId: string };
        return { commandId: request.commandId, state: "composer_draft_write_state_saved" };
      }
      if (command === "discard_composer_draft") return true;
      throw new Error(`Unexpected native command: ${command}`);
    });
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /Second chat/ }));
    const openingComposer = screen.getByLabelText("Message to project agent");
    expect(openingComposer).toBeDisabled();
    expect(screen.getByRole("button", { name: "Send message" })).toBeDisabled();
    expect(tauri.invoke.mock.calls.filter(([command]) => command === "send_project_turn")).toHaveLength(0);

    await act(async () => {
      resolveSecondOpen?.({
        correlationId: "native-switch-correlation-b",
        subscriptionId: "native-switch-subscription-b",
        system: systemWithTwoChats,
        session: secondSession,
        draft: null,
      });
    });
    const composer = await screen.findByPlaceholderText("Ask the project agent…");
    await user.type(composer, "Message for B");
    await user.click(screen.getByRole("button", { name: "Send message" }));
    await waitFor(() => {
      const call = tauri.invoke.mock.calls.find(([command]) => command === "send_project_turn");
      expect(call?.[1]).toMatchObject({ request: { sessionId: secondSessionId, text: "Message for B" } });
    });
  });

  it("keeps stale messages read-only while a watch gap is resynchronizing", async () => {
    let holdReconnect = false;
    tauri.invoke.mockImplementation(async (command: string) => {
      if (command === "native_mica_available") return true;
      if (command === "open_project_chat") {
        if (holdReconnect) return new Promise(() => undefined);
        return {
          correlationId: "native-resync-correlation",
          subscriptionId: "native-resync-subscription",
          system,
          session: timedOutSession,
          draft: null,
        };
      }
      if (command === "close_project_chat") return true;
      throw new Error(`Unexpected native command: ${command}`);
    });
    render(<App />);

    expect(await screen.findByText("Retained partial answer")).toBeVisible();
    holdReconnect = true;
    const handler = tauri.channelHandlers.at(-1);
    expect(handler).toBeDefined();
    act(() => {
      handler?.({
        kind: "resyncRequired",
        subscriptionId: "native-resync-subscription",
        cursor: { streamId: "stream", sequence: "2", authorityEpoch: "7" },
        reason: "revision_gap",
        currentRevision: "5",
      });
    });

    expect(screen.getByText("Refreshing")).toBeVisible();
    expect(screen.getByText("Read only")).toBeVisible();
    expect(screen.getByText("Retained partial answer")).toBeVisible();
    expect(screen.getByLabelText("Message to project agent")).toBeDisabled();
    expect(screen.queryByRole("button", { name: "Retry" })).not.toBeInTheDocument();
  });

  it("discards a possibly committed draft when its save receipt was lost", async () => {
    let attemptedCommandId = "";
    tauri.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "native_mica_available") return true;
      if (command === "open_project_chat") {
        return {
          correlationId: "native-lost-receipt-correlation",
          subscriptionId: "native-lost-receipt-subscription",
          system,
          session: timedOutSession,
          draft: null,
        };
      }
      if (command === "close_project_chat") return true;
      if (command === "save_composer_draft") {
        attemptedCommandId = ((args?.request as { commandId: string }).commandId);
        throw new Error("receipt lost");
      }
      if (command === "discard_composer_draft") return true;
      throw new Error(`Unexpected native command: ${command}`);
    });
    const user = userEvent.setup();
    render(<App />);

    const composer = await screen.findByPlaceholderText("Ask the project agent…");
    await user.type(composer, "ambiguous draft");
    await waitFor(() => expect(attemptedCommandId).not.toBe(""), { timeout: 2_000 });
    await user.clear(composer);
    await waitFor(() => {
      const discard = tauri.invoke.mock.calls.find(([command]) => command === "discard_composer_draft");
      expect(discard?.[1]).toMatchObject({ request: { commandId: attemptedCommandId } });
    }, { timeout: 2_000 });
  });

  it("keeps the window open when the final draft flush fails", async () => {
    tauri.invoke.mockImplementation(async (command: string) => {
      if (command === "native_mica_available") return true;
      if (command === "open_project_chat") {
        return {
          correlationId: "native-close-correlation",
          subscriptionId: "native-close-subscription",
          system,
          session: timedOutSession,
          draft: null,
        };
      }
      if (command === "close_project_chat") return true;
      if (command === "save_composer_draft") throw new Error("storage unavailable");
      throw new Error(`Unexpected native command: ${command}`);
    });
    const user = userEvent.setup();
    render(<App />);

    const composer = await screen.findByPlaceholderText("Ask the project agent…");
    await user.type(composer, "must survive close");
    await user.click(screen.getByRole("button", { name: "Close window" }));
    expect(await screen.findByText("The draft was not saved. The window remains open.")).toBeVisible();
    expect(tauri.closeWindow).not.toHaveBeenCalled();
  });

  it("intercepts an operating-system close request before closing", async () => {
    render(<App />);

    await screen.findByText("Timed out");
    await waitFor(() => expect(tauri.closeHandlers).toHaveLength(1));
    const preventDefault = vi.fn();
    act(() => tauri.closeHandlers[0]?.({ preventDefault }));
    expect(preventDefault).toHaveBeenCalledOnce();
    await waitFor(() => expect(tauri.closeWindow).toHaveBeenCalledOnce());
  });
});
