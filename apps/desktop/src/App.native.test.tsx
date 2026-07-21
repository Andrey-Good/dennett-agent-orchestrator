import React from "react";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
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
    systemSnapshot: null as unknown,
    systemWatchError: null as unknown,
    openSystemWatch: vi.fn(),
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
const nativeClipboard = vi.hoisted(() => ({
  writeText: vi.fn<(text: string) => Promise<void>>(async () => undefined),
}));

vi.mock("@tauri-apps/plugin-clipboard-manager", () => nativeClipboard);

vi.mock("@tauri-apps/api/core", () => ({
  Channel: tauri.TestChannel,
  invoke: (command: string, args?: Record<string, unknown>) => {
    if (command === "open_system_watch") {
      tauri.openSystemWatch();
      if (tauri.systemWatchError) return Promise.reject(tauri.systemWatchError);
      return Promise.resolve({
        correlationId: "native-system-correlation",
        subscriptionId: "native-system-subscription",
        snapshot: tauri.systemSnapshot,
      });
    }
    if (command === "close_system_watch") return Promise.resolve(true);
    return tauri.invoke(command, args);
  },
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

const runtimeControls = [
  {
    id: "dennett.access_mode",
    label: "Agent access",
    defaultChoiceId: "auto_approve",
    choices: [
      { id: "auto_approve", label: "Auto-approve", description: "Project sandbox", availableWhen: [] },
      { id: "full_access", label: "Full access", description: "Unrestricted commands", availableWhen: [] },
    ],
  },
  {
    id: "model",
    label: "Model",
    defaultChoiceId: "gpt-new",
    choices: [
      { id: "gpt-new", label: "GPT New", description: null, availableWhen: [] },
      { id: "gpt-small", label: "GPT Small", description: null, availableWhen: [] },
    ],
  },
  {
    id: "reasoning_effort",
    label: "Reasoning",
    defaultChoiceId: "provider_default",
    choices: [
      { id: "provider_default", label: "Model default", description: null, availableWhen: [] },
      { id: "low", label: "Low", description: null, availableWhen: [{ controlId: "model", choiceIds: ["gpt-new", "gpt-small"] }] },
      { id: "high", label: "High", description: null, availableWhen: [{ controlId: "model", choiceIds: ["gpt-new"] }] },
      { id: "ultra", label: "Ultra", description: null, availableWhen: [{ controlId: "model", choiceIds: ["gpt-new"] }] },
    ],
  },
  {
    id: "service_tier",
    label: "Speed",
    defaultChoiceId: "provider_default",
    choices: [
      { id: "provider_default", label: "Standard", description: null, availableWhen: [] },
      { id: "fast", label: "Fast", description: null, availableWhen: [{ controlId: "model", choiceIds: ["gpt-new"] }] },
    ],
  },
];

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
      activities: [
        {
          activityId: "reasoning-private",
          phase: "reasoning_summary",
          message: "Low-level provider reasoning must stay hidden",
          status: "turn_activity_status_completed",
          updatedAtUnixMs: 1_750_000_003_000,
        },
        {
          activityId: "commentary-1",
          phase: "commentary",
          message: "Checking the project boundary.",
          status: "turn_activity_status_completed",
          updatedAtUnixMs: 1_750_000_005_000,
        },
        {
          activityId: "activity-1",
          phase: "command",
          message: null,
          status: "turn_activity_status_completed",
          updatedAtUnixMs: 1_750_000_015_000,
        },
        {
          activityId: "commentary-final",
          phase: "commentary",
          message: "Retained partial answer",
          status: "turn_activity_status_completed",
          updatedAtUnixMs: 1_750_000_025_000,
        },
      ],
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
    steering: "native",
    nativeExtensionSchemas: [],
    controls: runtimeControls,
  },
};
const emptySystem = {
  ...system,
  revision: "1",
  projects: [],
  recentSessions: [],
  activeProjectId: null,
  activeSessionId: null,
  // Protocol-v1 peers built before steering was added decode it as an empty string.
  runtime: { ...system.runtime, steering: "" },
};

describe("native Project Chat recovery", () => {
  beforeEach(() => {
    tauri.channelHandlers.splice(0);
    tauri.closeHandlers.splice(0);
    tauri.systemSnapshot = system;
    tauri.systemWatchError = null;
    tauri.openSystemWatch.mockClear();
    tauri.invoke.mockReset();
    tauri.closeWindow.mockClear();
    tauri.minimizeWindow.mockClear();
    tauri.maximizeWindow.mockClear();
    tauri.onCloseRequested.mockClear();
    nativeClipboard.writeText.mockClear();
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

  it("copies visible user and agent content through the native system clipboard", async () => {
    const user = userEvent.setup();
    render(<App />);

    expect(await screen.findByText("Retained partial answer")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Copy user message" }));
    await user.click(screen.getByRole("button", { name: "Copy agent message" }));

    await waitFor(() => expect(nativeClipboard.writeText).toHaveBeenNthCalledWith(1, "Retry the owner request"));
    expect(nativeClipboard.writeText.mock.calls[1]?.[0]).toContain("## Result");
    expect(nativeClipboard.writeText.mock.calls[1]?.[0]).not.toContain("Low-level provider reasoning must stay hidden");
    const userMessage = screen.getByLabelText("user message");
    expect(userMessage.querySelector(".message-copy")).not.toContainElement(userMessage.querySelector(".message-time"));
  });

  it("shows retained partial output and retries the same request through a new stable command", async () => {
    const user = userEvent.setup();
    render(<App />);

    expect(await screen.findByText("Timed out")).toBeVisible();
    expect(screen.getByText("Retained partial answer")).toBeVisible();
    expect(screen.getByRole("heading", { name: "Result" })).toBeVisible();
    const commentary = screen.getByText("Checking the project boundary.");
    const commandActivity = screen.getByText("Ran command");
    const elapsed = screen.getByText("Timed out after 26s");
    expect(commentary.compareDocumentPosition(commandActivity) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(commandActivity.compareDocumentPosition(elapsed) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.queryByText("Low-level provider reasoning must stay hidden")).not.toBeInTheDocument();
    expect(screen.queryAllByText("Retained partial answer")).toHaveLength(1);
    expect(screen.queryByText("now")).not.toBeInTheDocument();
    const createdTime = new Date(1_750_000_000_001).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    const completedTime = new Date(1_750_000_026_001).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    expect(createdTime).not.toBe(completedTime);
    expect(screen.getByLabelText("agent message")).toHaveTextContent(createdTime);
    expect(screen.getByLabelText("agent message")).not.toHaveTextContent(completedTime);
    expect(screen.queryByLabelText("Workspace resources")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Plugins" })).not.toBeInTheDocument();
    const access = screen.getByRole("button", { name: "Auto-approve" });
    await user.click(access);
    const accessDialog = screen.getByRole("dialog", { name: "Agent access" });
    await user.click(within(accessDialog).getByRole("button", { name: "Full access" }));
    expect(screen.getByRole("button", { name: "Full access" })).toBeVisible();
    await user.click(screen.getByRole("button", { name: /Agent runtime: Codex/i }));
    await user.click(screen.getByRole("button", { name: "Model: GPT New" }));
    await user.click(screen.getByRole("option", { name: "GPT Small" }));
    expect(screen.getByRole("button", { name: "Reasoning: Model default" })).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Speed: Standard" }));
    expect(screen.queryByRole("option", { name: "Fast" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Model: GPT Small" }));
    await user.click(screen.getByRole("option", { name: "GPT New" }));
    await user.click(screen.getByRole("button", { name: "Reasoning: Model default" }));
    await user.click(screen.getByRole("option", { name: "Ultra" }));
    await user.click(screen.getByRole("button", { name: "Speed: Standard" }));
    await user.click(screen.getByRole("option", { name: "Fast" }));
    expect(screen.getByRole("button", { name: /Agent runtime: Codex, GPT New · Ultra · Fast/i })).toBeVisible();
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
          runtimeControls: [
            { controlId: "dennett.access_mode", choiceId: "full_access" },
            { controlId: "model", choiceId: "gpt-new" },
            { controlId: "reasoning_effort", choiceId: "ultra" },
            { controlId: "service_tier", choiceId: "fast" },
          ],
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

  it("sends an in-flight clarification to the exact active Codex turn without cancelling it", async () => {
    const activeSummary = {
      ...sessionSummary,
      state: "session_state_running",
      revision: "3",
      activeTurnId: agentTurnId,
    };
    const activeSession = {
      session: activeSummary,
      fingerprint: "active-steer-fingerprint",
      turns: [
        {
          ...timedOutSession.turns[0],
          text: "Start the long task",
        },
        {
          ...timedOutSession.turns[1],
          state: "turn_state_streaming",
          text: "",
          activities: [{
            activityId: "active-command",
            phase: "command",
            message: null,
            status: "turn_activity_status_started",
            updatedAtUnixMs: 1_750_000_005_000,
          }],
          outcome: null,
          completedAtUnixMs: null,
        },
      ],
    };
    const activeSystem = { ...system, revision: "3", recentSessions: [activeSummary] };
    tauri.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "native_mica_available") return true;
      if (command === "open_project_chat") {
        return {
          correlationId: "native-steer-correlation",
          subscriptionId: "native-steer-subscription",
          system: activeSystem,
          session: activeSession,
          draft: null,
        };
      }
      if (command === "close_project_chat") return true;
      if (command === "save_composer_draft") {
        const request = args?.request as { commandId: string };
        return { commandId: request.commandId, state: "composer_draft_write_state_saved" };
      }
      if (command === "send_project_turn") {
        const request = args?.request as Record<string, unknown>;
        return { commandId: request.commandId, turnId: agentTurnId };
      }
      throw new Error(`Unexpected native command: ${command}`);
    });
    const user = userEvent.setup();
    render(<App />);

    const composer = await screen.findByPlaceholderText("Ask the project agent…");
    expect(await screen.findByRole("button", { name: `Stop generation for session "${sessionSummary.title}"` })).toBeVisible();
    expect(screen.getByRole("button", { name: "Auto-approve" })).toBeDisabled();
    expect(screen.getByRole("button", { name: /Agent runtime: Codex/i })).toBeDisabled();
    await user.type(composer, "Use the new constraint too");
    await user.click(screen.getByRole("button", { name: "Send message" }));

    await waitFor(() => {
      const call = tauri.invoke.mock.calls.find(([command]) => command === "send_project_turn");
      expect(call?.[1]).toMatchObject({
        request: {
          projectId,
          sessionId,
          expectedRevision: "3",
          text: "Use the new constraint too",
          runtimeControls: [],
          deliveryMode: "steer_now",
          expectedActiveTurnId: agentTurnId,
        },
      });
    });
    expect(tauri.invoke.mock.calls.some(([command]) => command === "cancel_project_turn")).toBe(false);
  });

  it("keeps the next typed clarification while the previous delivery receipt is pending", async () => {
    const activeSummary = {
      ...sessionSummary,
      state: "session_state_running",
      revision: "3",
      activeTurnId: agentTurnId,
    };
    const activeSession = {
      session: activeSummary,
      fingerprint: "pending-steer-fingerprint",
      turns: [
        { ...timedOutSession.turns[0], text: "Start the long task" },
        {
          ...timedOutSession.turns[1],
          state: "turn_state_streaming",
          text: "",
          activities: [],
          outcome: null,
          completedAtUnixMs: null,
        },
      ],
    };
    let resolveFirstSend: ((value: { commandId: string; turnId: string }) => void) | undefined;
    let sendCount = 0;
    tauri.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "native_mica_available") return true;
      if (command === "open_project_chat") {
        return {
          correlationId: "native-pending-steer-correlation",
          subscriptionId: "native-pending-steer-subscription",
          system: { ...system, revision: "3", recentSessions: [activeSummary] },
          session: activeSession,
          draft: null,
        };
      }
      if (command === "close_project_chat") return true;
      if (command === "save_composer_draft") {
        const request = args?.request as { commandId: string };
        return { commandId: request.commandId, state: "composer_draft_write_state_saved" };
      }
      if (command === "send_project_turn") {
        const request = (args?.request ?? {}) as { commandId: string };
        sendCount += 1;
        if (sendCount === 1) {
          return new Promise<{ commandId: string; turnId: string }>((resolve) => {
            resolveFirstSend = resolve;
          });
        }
        return { commandId: request.commandId, turnId: agentTurnId };
      }
      throw new Error(`Unexpected native command: ${command}`);
    });
    const user = userEvent.setup();
    render(<App />);

    const composer = await screen.findByPlaceholderText("Ask the project agent…");
    await user.type(composer, "first clarification");
    await user.click(screen.getByRole("button", { name: "Send message" }));
    await waitFor(() => expect(resolveFirstSend).toBeTypeOf("function"));
    const firstSend = tauri.invoke.mock.calls.find(([command]) => command === "send_project_turn");
    const firstCommandId = (firstSend?.[1] as { request: { commandId: string } }).request.commandId;

    await user.clear(composer);
    await user.type(composer, "second clarification");
    await act(async () => resolveFirstSend?.({ commandId: firstCommandId, turnId: agentTurnId }));
    await waitFor(() => expect(composer).toHaveValue("second clarification"));

    await user.click(screen.getByRole("button", { name: "Send message" }));
    await waitFor(() => expect(
      tauri.invoke.mock.calls.filter(([command]) => command === "send_project_turn"),
    ).toHaveLength(2));
    const sends = tauri.invoke.mock.calls.filter(([command]) => command === "send_project_turn");
    expect(sends[1]?.[1]).toMatchObject({ request: { text: "second clarification" } });
    const secondCommandId = (sends[1]?.[1] as { request: { commandId: string } }).request.commandId;
    expect(secondCommandId).not.toBe(firstCommandId);
  });

  it("renders owner updates, an in-flight clarification, later work, and the final answer in causal order", async () => {
    const steerTurnId = "00000000-0000-7000-8000-000000000051";
    const orderedSession = {
      session: { ...sessionSummary, revision: "8" },
      fingerprint: "ordered-steer-fingerprint",
      turns: [
        {
          ...timedOutSession.turns[0],
          text: "Begin the task",
          createdRevision: "2",
          createdAtUnixMs: 1_750_000_000_000,
        },
        {
          ...timedOutSession.turns[1],
          state: "turn_state_completed",
          text: "## Final answer\n\n- Completed with the clarification",
          createdRevision: "2",
          activities: [
            {
              activityId: "before-steer",
              phase: "commentary",
              message: "First I will inspect the workspace.",
              status: "turn_activity_status_completed",
              createdRevision: "3",
              createdAtUnixMs: 1_750_000_010_000,
              updatedAtUnixMs: 1_750_000_010_000,
            },
            {
              activityId: "after-steer-command",
              phase: "command",
              message: null,
              status: "turn_activity_status_completed",
              createdRevision: "6",
              createdAtUnixMs: 1_750_000_010_000,
              updatedAtUnixMs: 1_750_000_020_000,
            },
            {
              activityId: "after-steer",
              phase: "commentary",
              message: "I included the new constraint.",
              status: "turn_activity_status_completed",
              createdRevision: "7",
              createdAtUnixMs: 1_750_000_010_000,
              updatedAtUnixMs: 1_750_000_022_000,
            },
          ],
          outcome: { kind: "result", summary: "Completed with the clarification", partial: false },
          createdAtUnixMs: 1_750_000_000_001,
          completedAtUnixMs: 1_750_000_030_001,
        },
        {
          turnId: steerTurnId,
          commandId: "00000000-0000-7000-8000-000000000031",
          role: "turn_role_user",
          state: "turn_state_completed",
          text: "Also include the new constraint",
          activities: [],
          outcome: null,
          createdRevision: "4",
          createdAtUnixMs: 1_750_000_010_000,
          completedAtUnixMs: 1_750_000_010_001,
        },
      ],
    };
    tauri.invoke.mockImplementation(async (command: string) => {
      if (command === "native_mica_available") return true;
      if (command === "open_project_chat") {
        return {
          correlationId: "native-order-correlation",
          subscriptionId: "native-order-subscription",
          system: { ...system, revision: "8" },
          session: orderedSession,
          draft: null,
        };
      }
      if (command === "close_project_chat") return true;
      throw new Error(`Unexpected native command: ${command}`);
    });
    render(<App />);

    const before = await screen.findByText("First I will inspect the workspace.");
    const steer = screen.getByText("Also include the new constraint");
    const command = screen.getByText("Ran command");
    const after = screen.getByText("I included the new constraint.");
    const summary = screen.getByText("Worked for 30s");
    const final = screen.getByRole("heading", { name: "Final answer" });
    expect(before.compareDocumentPosition(steer) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(steer.compareDocumentPosition(command) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(command.compareDocumentPosition(after) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(after.compareDocumentPosition(summary) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(summary.compareDocumentPosition(final) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("keeps an empty pre-steer segment in order and shows failed delivery", async () => {
    const failedSteerId = "00000000-0000-7000-8000-000000000052";
    const activeSummary = {
      ...sessionSummary,
      state: "session_state_running",
      revision: "5",
      activeTurnId: agentTurnId,
    };
    const failedSteerSession = {
      session: activeSummary,
      fingerprint: "failed-steer-order-fingerprint",
      turns: [
        {
          ...timedOutSession.turns[0],
          text: "Begin without an initial update",
          createdAtUnixMs: 1_750_000_000_000,
        },
        {
          ...timedOutSession.turns[1],
          state: "turn_state_streaming",
          text: "",
          activities: [{
            activityId: "after-failed-steer",
            phase: "commentary",
            message: "Work continued after the delivery attempt.",
            status: "turn_activity_status_completed",
            updatedAtUnixMs: 1_750_000_020_000,
          }],
          outcome: null,
          createdAtUnixMs: 1_750_000_000_001,
          completedAtUnixMs: null,
        },
        {
          turnId: failedSteerId,
          commandId: "00000000-0000-7000-8000-000000000032",
          role: "turn_role_user",
          state: "turn_state_failed",
          text: "Use a constraint that could not be delivered",
          activities: [],
          outcome: {
            kind: "error",
            error: {
              code: "provider_unavailable",
              messageKey: "session.steer_failed",
              correlationId: "",
              retryable: true,
              userActionRequired: false,
              detailsHandle: null,
              currentRevision: null,
            },
          },
          createdAtUnixMs: 1_750_000_010_000,
          completedAtUnixMs: 1_750_000_012_000,
        },
      ],
    };
    tauri.invoke.mockImplementation(async (command: string) => {
      if (command === "native_mica_available") return true;
      if (command === "open_project_chat") {
        return {
          correlationId: "failed-steer-order-correlation",
          subscriptionId: "failed-steer-order-subscription",
          system: { ...system, revision: "5", recentSessions: [activeSummary] },
          session: failedSteerSession,
          draft: null,
        };
      }
      if (command === "close_project_chat") return true;
      throw new Error(`Unexpected native command: ${command}`);
    });
    render(<App />);

    const steer = await screen.findByText("Use a constraint that could not be delivered");
    const after = screen.getByText("Work continued after the delivery attempt.");
    expect(screen.getByText("Clarification delivery could not be confirmed")).toBeVisible();
    expect(steer.compareDocumentPosition(after) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("creates a real standalone chat from Recent and opens it outside every project", async () => {
    const standaloneSessionId = "00000000-0000-7000-8000-000000000023";
    const standaloneSummary = {
      ...sessionSummary,
      sessionId: standaloneSessionId,
      projectId: null,
      title: "Untitled chat",
      revision: "1",
      activeTurnId: null,
    };
    const standaloneSystem = {
      ...system,
      revision: "5",
      recentSessions: [sessionSummary, standaloneSummary],
      activeProjectId: null,
      activeSessionId: standaloneSessionId,
    };
    const standaloneSession = {
      session: standaloneSummary,
      fingerprint: "standalone-fingerprint",
      turns: [],
    };
    tauri.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "native_mica_available") return true;
      if (command === "open_project_chat") {
        const request = args?.request as { sessionId: string | null };
        const standalone = request.sessionId === standaloneSessionId;
        return {
          correlationId: standalone ? "standalone-open" : "project-open",
          subscriptionId: standalone ? "standalone-subscription" : "project-subscription",
          system: standalone ? standaloneSystem : system,
          session: standalone ? standaloneSession : timedOutSession,
          draft: null,
        };
      }
      if (command === "close_project_chat") return true;
      if (command === "create_chat") return { sessionId: standaloneSessionId };
      throw new Error(`Unexpected native command: ${command}`);
    });
    const user = userEvent.setup();
    render(<App />);

    await screen.findByText("Timed out");
    fireEvent.click(screen.getByRole("button", { name: "New recent chat" }));

    await waitFor(() => {
      const call = tauri.invoke.mock.calls.find(([command]) => command === "create_chat");
      expect(call?.[1]).toMatchObject({ request: { projectId: null, title: "Untitled chat" } });
    });
    await screen.findByPlaceholderText("Ask the project agent…");
    expect(screen.getByRole("heading", { name: "Start a conversation" })).toBeVisible();
    expect(screen.getByRole("navigation", { name: "Current location" })).toHaveTextContent("Chats/Untitled chat");
    expect(screen.getByRole("button", { name: /Untitled chat/ })).toHaveAttribute("aria-current", "page");
  });

  it("treats a fresh profile as an empty ready system and creates its first chat", async () => {
    const firstSessionId = "00000000-0000-7000-8000-000000000024";
    const firstSummary = {
      ...sessionSummary,
      sessionId: firstSessionId,
      projectId: null,
      title: "Untitled chat",
      revision: "1",
    };
    tauri.systemSnapshot = emptySystem;
    tauri.invoke.mockImplementation(async (command: string) => {
      if (command === "native_mica_available") return true;
      if (command === "create_chat") return { sessionId: firstSessionId };
      if (command === "open_project_chat") return {
        correlationId: "first-chat-correlation",
        subscriptionId: "first-chat-subscription",
        system: { ...emptySystem, revision: "2", recentSessions: [firstSummary], activeSessionId: firstSessionId },
        session: { session: firstSummary, fingerprint: "first-chat", turns: [] },
        draft: null,
      };
      if (command === "close_project_chat") return true;
      throw new Error(`Unexpected native command: ${command}`);
    });
    const user = userEvent.setup();
    render(<App />);

    expect(await screen.findByText("The local Node and agent runtime are ready. Create or select a chat to begin.")).toBeVisible();
    expect(screen.getByRole("button", { name: /Agent runtime: Codex/i })).toBeEnabled();
    expect(screen.getByPlaceholderText("Create or select a chat to begin…")).toBeDisabled();
    expect(tauri.invoke.mock.calls.some(([command]) => command === "open_project_chat")).toBe(false);

    fireEvent.click(screen.getByRole("button", { name: "New recent chat" }));
    await screen.findByPlaceholderText("Ask the project agent…");
    expect(screen.getByRole("heading", { name: "Start a conversation" })).toBeVisible();
    expect(screen.getByRole("navigation", { name: "Current location" })).toHaveTextContent("Chats/Untitled chat");
  });

  it("returns an empty profile to Ready after the system watch recovers", async () => {
    tauri.systemSnapshot = emptySystem;
    render(<App />);

    expect(await screen.findByText("The local Node and agent runtime are ready. Create or select a chat to begin.")).toBeVisible();
    const handler = tauri.channelHandlers[0];
    expect(handler).toBeDefined();
    act(() => handler?.({
      kind: "error",
      subscriptionId: "native-system-subscription",
      error: {
        code: "ipc_watch_closed",
        messageKey: "desktop.ipc_watch_closed",
        correlationId: "native-system-correlation",
        retryable: true,
        userActionRequired: false,
        detailsHandle: null,
        currentRevision: null,
      },
    }));
    expect(screen.getByText("Unavailable")).toBeVisible();

    act(() => handler?.({
      kind: "snapshot",
      subscriptionId: "native-system-subscription",
      cursor: { streamId: "system-stream", sequence: "2", authorityEpoch: "7" },
      snapshot: { ...emptySystem, revision: "2" },
      fingerprint: "recovered-system",
    }));

    expect(await screen.findByText("The local Node and agent runtime are ready. Create or select a chat to begin.")).toBeVisible();
    expect(screen.getByLabelText("Message to project agent")).toBeDisabled();
  });

  it("shows a truthful system-watch failure instead of an endless opening state", async () => {
    tauri.systemWatchError = {
      code: "desktop_node_unavailable",
      messageKey: "desktop.node_unavailable",
      correlationId: "native-system-correlation",
      retryable: true,
      userActionRequired: false,
      detailsHandle: null,
      currentRevision: null,
    };
    render(<App />);

    expect(await screen.findByText(/desktop\.node_unavailable/)).toBeVisible();
    expect(screen.getByText("Unavailable")).toBeVisible();
    expect(tauri.invoke.mock.calls.some(([command]) => command === "open_project_chat")).toBe(false);
  });

  it("does not retry a system-watch failure that requires user action", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      tauri.systemWatchError = {
        code: "desktop_authentication_required",
        messageKey: "desktop.authentication_required",
        correlationId: "native-system-correlation",
        retryable: true,
        userActionRequired: true,
        detailsHandle: null,
        currentRevision: null,
      };
      render(<App />);

      expect(await screen.findByText(/desktop\.authentication_required/)).toBeVisible();
      await act(async () => { await vi.advanceTimersByTimeAsync(1_500); });
      expect(tauri.openSystemWatch).toHaveBeenCalledTimes(1);
    } finally {
      vi.useRealTimers();
    }
  });

  it("single-flights rapid standalone-chat creation", async () => {
    let resolveCreate: ((value: { sessionId: string }) => void) | undefined;
    tauri.invoke.mockImplementation(async (command: string) => {
      if (command === "native_mica_available") return true;
      if (command === "open_project_chat") {
        return {
          correlationId: "native-single-flight-correlation",
          subscriptionId: crypto.randomUUID(),
          system,
          session: timedOutSession,
          draft: null,
        };
      }
      if (command === "close_project_chat") return true;
      if (command === "create_chat") {
        return new Promise<{ sessionId: string }>((resolve) => { resolveCreate = resolve; });
      }
      throw new Error(`Unexpected native command: ${command}`);
    });
    render(<App />);
    await screen.findByText("Timed out");
    const create = screen.getByRole("button", { name: "New recent chat" });

    fireEvent.click(create);
    fireEvent.click(create);
    await waitFor(() => expect(
      tauri.invoke.mock.calls.filter(([command]) => command === "create_chat"),
    ).toHaveLength(1));
    await act(async () => resolveCreate?.({ sessionId: "00000000-0000-7000-8000-000000000023" }));
  });

  it("terminalizes unfinished activity after Stop instead of leaving a running spinner", async () => {
    const cancelledSession = {
      session: { ...sessionSummary, revision: "6" },
      fingerprint: "cancelled-activity-fingerprint",
      turns: [
        timedOutSession.turns[0],
        {
          ...timedOutSession.turns[1],
          state: "turn_state_cancelled",
          text: "Partial result",
          activities: [{
            activityId: "cancelled-command",
            phase: "command",
            message: null,
            status: "turn_activity_status_started",
            updatedAtUnixMs: 1_750_000_010_000,
          }],
          outcome: { kind: "result", summary: "Partial result", partial: true },
          completedAtUnixMs: 1_750_000_013_001,
        },
      ],
    };
    tauri.invoke.mockImplementation(async (command: string) => {
      if (command === "native_mica_available") return true;
      if (command === "open_project_chat") {
        return {
          correlationId: "native-cancelled-correlation",
          subscriptionId: "native-cancelled-subscription",
          system: { ...system, revision: "6" },
          session: cancelledSession,
          draft: null,
        };
      }
      if (command === "close_project_chat") return true;
      throw new Error(`Unexpected native command: ${command}`);
    });
    render(<App />);

    expect(await screen.findByText("Command stopped")).toBeVisible();
    expect(screen.queryByText("Running command")).not.toBeInTheDocument();
    expect(screen.getByLabelText("Agent work").querySelector(".spin")).toBeNull();
  });

  it("keeps project and standalone-chat actions visible without pretending unsupported storage exists", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByText("Timed out");

    expect(screen.getByRole("button", { name: "New chat in Dennett native test" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Recent" })).toBeVisible();
    expect(screen.getByText("No standalone chats yet")).toBeVisible();
    expect(screen.getByRole("button", { name: "New recent chat" })).toBeInTheDocument();

    const newProject = screen.getByRole("button", { name: "New project" });
    newProject.focus();
    await user.keyboard("{Enter}");
    const dialog = screen.getByRole("dialog", { name: "Create or add project" });
    expect(within(dialog).getByRole("button", { name: /Create empty project/i })).toBeDisabled();
    expect(within(dialog).getByRole("button", { name: /Add existing folder/i })).toBeDisabled();
    expect(within(dialog).getByText(/not connected in M01/i)).toBeVisible();
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

  it("does not leave a chat whose draft failed to save", async () => {
    const systemWithTwoChats = {
      ...system,
      recentSessions: [sessionSummary, secondSessionSummary],
    };
    tauri.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "native_mica_available") return true;
      if (command === "open_project_chat") {
        const request = args?.request as { sessionId: string | null };
        if (request.sessionId === secondSessionId) throw new Error("must not open second chat");
        return {
          correlationId: "native-switch-save-correlation",
          subscriptionId: "native-switch-save-subscription",
          system: systemWithTwoChats,
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
    await user.type(composer, "must remain in this chat");
    await user.click(screen.getByRole("button", { name: /Second chat/ }));

    expect(await screen.findByText("The current draft was not saved. This chat remains open.")).toBeVisible();
    expect(screen.getByRole("button", { name: /Timeout recovery/ })).toHaveAttribute("aria-current", "page");
    expect(tauri.invoke.mock.calls.filter(([command, args]) => (
      command === "open_project_chat"
      && (args as { request?: { sessionId?: string | null } } | undefined)?.request?.sessionId === secondSessionId
    ))).toHaveLength(0);
  });

  it("does not let a late new-chat response replace a newer navigation", async () => {
    const createdSessionId = "00000000-0000-7000-8000-000000000022";
    const systemWithTwoChats = {
      ...system,
      recentSessions: [sessionSummary, secondSessionSummary],
    };
    let resolveCreate: ((value: { sessionId: string }) => void) | undefined;
    tauri.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "native_mica_available") return true;
      if (command === "open_project_chat") {
        const request = args?.request as { sessionId: string | null };
        const openedSession = request.sessionId === secondSessionId ? secondSession : timedOutSession;
        return {
          correlationId: "native-create-race-correlation",
          subscriptionId: crypto.randomUUID(),
          system: systemWithTwoChats,
          session: openedSession,
          draft: null,
        };
      }
      if (command === "close_project_chat") return true;
      if (command === "create_chat") {
        return new Promise<{ sessionId: string }>((resolve) => { resolveCreate = resolve; });
      }
      throw new Error(`Unexpected native command: ${command}`);
    });
    const user = userEvent.setup();
    render(<App />);

    await screen.findByText("Timed out");
    await user.keyboard("{Control>}k{/Control}");
    await user.click(screen.getByRole("button", { name: "New chat in current project" }));
    await waitFor(() => expect(resolveCreate).toBeTypeOf("function"));
    await user.click(screen.getByRole("button", { name: /Second chat/ }));
    const opensBeforeCreateCompletes = tauri.invoke.mock.calls.filter(([command]) => command === "open_project_chat").length;
    await act(async () => { resolveCreate?.({ sessionId: createdSessionId }); });

    await waitFor(() => {
      expect(screen.getByRole("button", { name: /Second chat/ })).toHaveAttribute("aria-current", "page");
    });
    expect(tauri.invoke.mock.calls.some(([command, args]) => (
      command === "open_project_chat"
      && (args as { request?: { sessionId?: string | null } } | undefined)?.request?.sessionId === createdSessionId
    ))).toBe(false);
    await waitFor(() => {
      expect(tauri.invoke.mock.calls.filter(([command]) => command === "open_project_chat").length)
        .toBeGreaterThan(opensBeforeCreateCompletes);
    });
  });

  it("shows a newly active turn instead of Retry from an older timeout", async () => {
    tauri.invoke.mockImplementation(async (command: string) => {
      if (command === "native_mica_available") return true;
      if (command === "open_project_chat") {
        return {
          correlationId: "native-new-active-correlation",
          subscriptionId: "native-new-active-subscription",
          system,
          session: {
            ...timedOutSession,
            session: {
              ...timedOutSession.session,
              revision: "5",
              state: "session_state_running",
              activeTurnId: "00000000-0000-7000-8000-000000000099",
            },
          },
          draft: null,
        };
      }
      if (command === "close_project_chat") return true;
      throw new Error(`Unexpected native command: ${command}`);
    });
    render(<App />);

    expect(await screen.findByText("Working")).toBeVisible();
    expect(screen.queryByRole("button", { name: "Retry" })).not.toBeInTheDocument();
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

  it("uses heartbeats for freshness and reconnects a silently stalled watch", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      let openCount = 0;
      tauri.invoke.mockImplementation(async (command: string) => {
        if (command === "native_mica_available") return true;
        if (command === "open_project_chat") {
          openCount += 1;
          // The initial unscoped bootstrap is followed by one scoped watch for
          // the active session. Only a later freshness reconnect is stalled.
          if (openCount > 2) return new Promise(() => undefined);
          return {
            correlationId: "native-heartbeat-correlation",
            subscriptionId: "native-heartbeat-subscription",
            system,
            session: timedOutSession,
            draft: null,
          };
        }
        if (command === "close_project_chat") return true;
        throw new Error(`Unexpected native command: ${command}`);
      });
      render(<App />);
      expect(await screen.findByText("Timed out")).toBeVisible();
      await waitFor(() => expect(openCount).toBe(2));
      const handler = tauri.channelHandlers.at(-1);

      await act(async () => { await vi.advanceTimersByTimeAsync(60_000); });
      act(() => handler?.({
        kind: "heartbeat",
        subscriptionId: "native-heartbeat-subscription",
        cursor: { streamId: "stream", sequence: "2", authorityEpoch: "7" },
        currentRevision: "4",
      }));
      await act(async () => { await vi.advanceTimersByTimeAsync(60_000); });
      expect(screen.getByText("Live")).toBeVisible();

      await act(async () => { await vi.advanceTimersByTimeAsync(10_001); });
      expect(screen.getByText("Refreshing")).toBeVisible();
      expect(screen.getByLabelText("Message to project agent")).toBeDisabled();
    } finally {
      vi.useRealTimers();
    }
  });

  it("resynchronizes when a heartbeat reports an unseen revision", async () => {
    render(<App />);
    await screen.findByText("Timed out");
    await waitFor(() => {
      expect(tauri.invoke.mock.calls.filter(([command]) => command === "open_project_chat").length).toBeGreaterThanOrEqual(2);
    });
    const handler = tauri.channelHandlers.at(-1);

    act(() => handler?.({
      kind: "heartbeat",
      subscriptionId: "native-timeout-subscription",
      cursor: { streamId: "stream", sequence: "2", authorityEpoch: "7" },
      currentRevision: "5",
    }));

    expect(screen.getByText("Refreshing")).toBeVisible();
    expect(screen.getByText("Read only")).toBeVisible();
    expect(screen.getByLabelText("Message to project agent")).toBeDisabled();
  });

  it("opens provider-enforced access settings from the native Command Center", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByText("Timed out");

    await user.keyboard("{Control>}k{/Control}");
    const search = screen.getByRole("textbox", { name: "Command Center search" });
    await user.type(search, "access");

    await user.click(screen.getByRole("button", { name: "Agent access settings" }));
    const dialog = screen.getByRole("dialog", { name: "Agent access" });
    expect(within(dialog).getByRole("button", { name: "Auto-approve" })).toBeVisible();
    expect(within(dialog).getByRole("button", { name: "Full access" })).toBeVisible();
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

  it("restores the same visibly unsent draft after the renderer restarts", async () => {
    const restoredCommandId = "00000000-0000-7000-8000-000000000031";
    tauri.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "native_mica_available") return true;
      if (command === "open_project_chat") {
        return {
          correlationId: "native-restored-draft-correlation",
          subscriptionId: crypto.randomUUID(),
          system,
          session: timedOutSession,
          draft: {
            commandId: restoredCommandId,
            text: "unsent after restart",
            revision: "3",
            updatedAtUnixMs: 1_750_000_030_000,
          },
        };
      }
      if (command === "close_project_chat") return true;
      if (command === "save_composer_draft") {
        return { commandId: restoredCommandId, state: "composer_draft_write_state_saved" };
      }
      if (command === "send_project_turn") {
        const request = args?.request as Record<string, unknown>;
        return { commandId: request.commandId, turnId: "00000000-0000-7000-8000-000000000062" };
      }
      throw new Error(`Unexpected native command: ${command}`);
    });

    const first = render(<App />);
    const firstComposer = await screen.findByLabelText("Message to project agent");
    await waitFor(() => expect(firstComposer).toHaveValue("unsent after restart"));
    expect(screen.getAllByLabelText(/message$/i).some((message) => message.textContent?.includes("unsent after restart"))).toBe(false);
    first.unmount();

    const user = userEvent.setup();
    render(<App />);
    const restored = await screen.findByLabelText("Message to project agent");
    await waitFor(() => expect(restored).toHaveValue("unsent after restart"));
    await user.click(screen.getByRole("button", { name: "Send message" }));
    await waitFor(() => {
      const send = tauri.invoke.mock.calls.find(([command]) => command === "send_project_turn");
      expect(send?.[1]).toMatchObject({ request: { commandId: restoredCommandId, text: "unsent after restart" } });
    });
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
