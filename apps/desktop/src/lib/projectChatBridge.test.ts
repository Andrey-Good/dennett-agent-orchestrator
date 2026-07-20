import { describe, expect, it, vi } from "vitest";
import {
  TauriProjectChatClient,
  applyProjectChatEvent,
  parseOpenedProjectChat,
  parseProjectChatEvent,
  type ProjectChatBridgeDependencies,
  type ProjectChatEvent,
} from "./projectChatBridge";

const system = {
  revision: "2",
  authorityEpoch: "7",
  observedAtUnixMs: 1_750_000_000_000,
  projects: [{
    projectId: "project-1",
    displayName: "Dennett",
    state: "project_state_ready",
    revision: "1",
    lastActivityAtUnixMs: null,
  }],
  recentSessions: [{
    sessionId: "session-1",
    projectId: "project-1",
    title: "Conversation",
    state: "session_state_idle",
    revision: "1",
    activeTurnId: null,
    lastActivityAtUnixMs: null,
  }],
  activeProjectId: "project-1",
  activeSessionId: "session-1",
  nodeState: "health_state_ready",
  runtime: {
    adapterId: "dennett.fake",
    runtimeKind: "generic_loop",
    streaming: false,
    continuation: false,
    scopedCancellation: false,
    deadlines: false,
  },
};

const session = {
  session: system.recentSessions[0],
  fingerprint: "abcd",
  turns: [],
};

describe("TauriProjectChatClient", () => {
  it("restores a durable draft, applies ordered stream mutations, and closes only its watch", async () => {
    let channelHandler: ((event: unknown) => void) | undefined;
    const invoke = vi.fn(async (command: string) => {
      if (command === "open_project_chat") {
        return {
          correlationId: "correlation-1",
          subscriptionId: "subscription-1",
          system,
          session,
          draft: {
            commandId: "00000000-0000-7000-8000-000000000001",
            text: "restored owner draft",
            revision: "3",
            updatedAtUnixMs: 1_750_000_000_000,
          },
        };
      }
      if (command === "close_project_chat") return true;
      throw new Error(`Unexpected command ${command}`);
    });
    const dependencies: ProjectChatBridgeDependencies = {
      invoke,
      createChannel(onMessage) {
        channelHandler = onMessage;
        return { channel: "session" };
      },
      identity: () => "correlation-1",
    };
    const events: ProjectChatEvent[] = [];
    const handle = await new TauriProjectChatClient(dependencies).open((event) => events.push(event));

    expect(handle.opened.draft).toEqual(expect.objectContaining({ text: "restored owner draft" }));
    channelHandler?.({
      kind: "delta",
      subscriptionId: "subscription-1",
      cursor: { streamId: "stream-1", sequence: "2", authorityEpoch: "7" },
      baseRevision: "1",
      newRevision: "2",
      committedAtUnixMs: 1_750_000_000_000,
      mutations: [{
        kind: "upsertTurn",
        turn: {
          turnId: "turn-1",
          commandId: "command-1",
          role: "turn_role_agent",
          state: "turn_state_accepted",
          text: "",
          activities: [],
          outcome: null,
          createdAtUnixMs: 1_750_000_000_000,
          completedAtUnixMs: null,
        },
      }],
    });
    expect(events).toHaveLength(1);
    const updated = applyProjectChatEvent(handle.opened.session, events[0]);
    expect(updated.session.revision).toBe("2");
    expect(updated.turns[0].turnId).toBe("turn-1");

    channelHandler?.({
      kind: "delta",
      subscriptionId: "subscription-1",
      cursor: { streamId: "stream-1", sequence: "3", authorityEpoch: "7" },
      baseRevision: "2",
      newRevision: "3",
      committedAtUnixMs: 1_750_000_000_100,
      mutations: [{
        kind: "upsertTurnActivity",
        turnId: "turn-1",
        activity: {
          activityId: "activity-1",
          phase: "reasoning_summary",
          message: "Checked the request",
          status: "turn_activity_status_completed",
          updatedAtUnixMs: 1_750_000_000_100,
          nativeExtensions: [{
            namespace: "fixture.activity",
            schemaVersion: "1",
            jsonValue: "{}",
          }],
        },
      }],
    });
    const withActivity = applyProjectChatEvent(updated, events[1]);
    expect(withActivity.turns[0].activities[0]).toMatchObject({
      phase: "reasoning_summary",
      message: "Checked the request",
      nativeExtensions: [{
        namespace: "fixture.activity",
        schemaVersion: "1",
        jsonValue: "{}",
      }],
    });

    channelHandler?.({
      kind: "delta",
      subscriptionId: "subscription-1",
      cursor: { streamId: "stream-1", sequence: "4", authorityEpoch: "7" },
      baseRevision: "3",
      newRevision: "4",
      committedAtUnixMs: 1_750_000_000_200,
      mutations: [{
        kind: "finishTurn",
        turnId: "turn-1",
        state: "turn_state_completed",
        outcome: { kind: "result", summary: "Done", partial: false },
        completedAtUnixMs: 1_750_000_000_200,
      }],
    });
    const completed = applyProjectChatEvent(withActivity, events[2]);
    expect(completed.turns[0].completedAtUnixMs).toBe(1_750_000_000_200);

    await expect(handle.close()).resolves.toBe(true);
    await expect(handle.close()).resolves.toBe(false);
  });

  it("keeps one stable draft command through saves and SendTurn, then supports discard", async () => {
    const identities = ["operation-save", "correlation-save", "correlation-send", "operation-discard", "correlation-discard"];
    const invoke = vi.fn(async (command: string, args: Record<string, unknown>) => {
      const request = args.request as Record<string, unknown>;
      if (command === "save_composer_draft") {
        return { commandId: request.commandId, state: "composer_draft_write_state_saved" };
      }
      if (command === "send_project_turn") {
        return { commandId: request.commandId, turnId: "turn-1" };
      }
      if (command === "discard_composer_draft") return true;
      throw new Error(`Unexpected command ${command}`);
    });
    const client = new TauriProjectChatClient({
      invoke,
      createChannel: () => ({}),
      identity: () => identities.shift() ?? "identity-fallback",
    });
    const commandId = "00000000-0000-7000-8000-000000000001";

    await client.saveDraft({
      projectId: "project-1",
      sessionId: "session-1",
      commandId,
      text: "send once",
      revision: 1,
    });
    const accepted = await client.sendTurn({
      projectId: "project-1",
      sessionId: "session-1",
      revision: "1",
      commandId,
      text: "send once",
    });
    await client.discardDraft({ projectId: "project-1", sessionId: "session-1", commandId });

    expect(accepted.commandId).toBe(commandId);
    expect(invoke.mock.calls[0][1]).toMatchObject({ request: { commandId, text: "send once", revision: 1 } });
    expect(invoke.mock.calls[1][1]).toMatchObject({ request: { commandId, expectedRevision: "1" } });
    expect(invoke.mock.calls[2][1]).toMatchObject({ request: { commandId } });
  });
});

describe("project chat validation", () => {
  it("rejects malformed draft payloads and revision jumps before reducer state changes", () => {
    expect(() => parseOpenedProjectChat({
      correlationId: "correlation-1",
      subscriptionId: "subscription-1",
      system,
      session,
      draft: { commandId: "command-1", text: 42, updatedAtUnixMs: null },
    })).toThrow("Invalid text");

    const jump = parseProjectChatEvent({
      kind: "delta",
      subscriptionId: "subscription-1",
      cursor: { streamId: "stream-1", sequence: "2", authorityEpoch: "7" },
      baseRevision: "1",
      newRevision: "3",
      committedAtUnixMs: 1_750_000_000_000,
      mutations: [],
    });
    expect(() => applyProjectChatEvent(session, jump)).toThrow("Session revision gap");
  });

  it("keeps an authoritative timeout terminal when a late provider mutation arrives", () => {
    const timedOut = {
      session: { ...session.session, revision: "2", state: "session_state_idle", activeTurnId: null },
      fingerprint: "timed-out",
      turns: [{
        turnId: "turn-1",
        commandId: "command-1",
        role: "turn_role_agent",
        state: "turn_state_timed_out",
        text: "retained partial response",
        activities: [],
        outcome: { kind: "result" as const, summary: "retained partial response", partial: true },
        createdAtUnixMs: null,
        completedAtUnixMs: 1_750_000_000_000,
      }],
    };
    const late = parseProjectChatEvent({
      kind: "delta",
      subscriptionId: "subscription-1",
      cursor: { streamId: "stream-1", sequence: "3", authorityEpoch: "7" },
      baseRevision: "2",
      newRevision: "3",
      committedAtUnixMs: 1_750_000_000_100,
      mutations: [{ kind: "appendTurnText", turnId: "turn-1", text: "late success" }],
    });

    expect(() => applyProjectChatEvent(timedOut, late)).toThrow("Terminal turn is immutable");
    expect(timedOut.turns[0]).toMatchObject({
      state: "turn_state_timed_out",
      text: "retained partial response",
    });
  });
});
