import { describe, expect, it, vi } from "vitest";
import {
  applySystemEvent,
  TauriSystemBridgeClient,
  parseOpenedSystemWatch,
  parseSystemEvent,
  type SystemBridgeDependencies,
  type SystemEvent,
} from "./systemBridge";

const snapshot = {
  revision: "18446744073709551615",
  authorityEpoch: "7",
  observedAtUnixMs: 1_750_000_000_000,
  projects: [
    {
      projectId: "project-1",
      displayName: "Dennett",
      state: "project_state_ready",
      revision: "12",
      lastActivityAtUnixMs: null,
    },
  ],
  recentSessions: [
    {
      sessionId: "session-1",
      projectId: "project-1",
      title: "Authenticated bridge",
      state: "session_state_idle",
      revision: "9",
      activeTurnId: null,
      lastActivityAtUnixMs: null,
    },
  ],
  activeProjectId: "project-1",
  activeSessionId: "session-1",
  nodeState: "health_state_ready",
  runtime: null,
};

describe("TauriSystemBridgeClient", () => {
  it("opens the typed channel, preserves u64 revisions, and closes only the watch", async () => {
    let channelHandler: ((event: unknown) => void) | undefined;
    const invoke = vi.fn(async (command: string) => {
      if (command === "open_system_watch") {
        return {
          correlationId: "correlation-1",
          subscriptionId: "subscription-1",
          snapshot,
        };
      }
      if (command === "close_system_watch") return true;
      throw new Error(`Unexpected command ${command}`);
    });
    const dependencies: SystemBridgeDependencies = {
      invoke,
      createChannel(onMessage) {
        channelHandler = onMessage;
        return { channel: "test" };
      },
      correlationId: () => "correlation-1",
    };
    const events: SystemEvent[] = [];
    const handle = await new TauriSystemBridgeClient(dependencies).openSystemWatch((event) =>
      events.push(event),
    );

    expect(handle.opened.snapshot.revision).toBe("18446744073709551615");
    expect(invoke).toHaveBeenNthCalledWith(1, "open_system_watch", {
      request: { correlationId: "correlation-1" },
      onEvent: { channel: "test" },
    });
    channelHandler?.({
      kind: "snapshot",
      subscriptionId: "subscription-1",
      cursor: { streamId: "stream-1", sequence: "1", authorityEpoch: "7" },
      snapshot,
      fingerprint: "abcd",
    });
    expect(events[0]).toMatchObject({ kind: "snapshot", fingerprint: "abcd" });

    await expect(handle.close()).resolves.toBe(true);
    await expect(handle.close()).resolves.toBe(false);
    expect(invoke).toHaveBeenNthCalledWith(2, "close_system_watch", {
      request: { subscriptionId: "subscription-1" },
    });
  });

  it("turns malformed native channel data into a safe renderer event", async () => {
    let channelHandler: ((event: unknown) => void) | undefined;
    const dependencies: SystemBridgeDependencies = {
      invoke: async () => ({
        correlationId: "correlation-2",
        subscriptionId: "subscription-2",
        snapshot,
      }),
      createChannel(onMessage) {
        channelHandler = onMessage;
        return {};
      },
      correlationId: () => "correlation-2",
    };
    const events: SystemEvent[] = [];
    await new TauriSystemBridgeClient(dependencies).openSystemWatch((event) => events.push(event));
    channelHandler?.({ kind: "delta", revision: 3 });
    expect(events).toEqual([
      {
        kind: "error",
        subscriptionId: "unbound",
        error: expect.objectContaining({
          code: "desktop_bridge_event_invalid",
          correlationId: "correlation-2",
        }),
      },
    ]);
  });
});

it("treats a protocol-v1 snapshot without the additive steering field as unsupported", () => {
  const parsed = parseOpenedSystemWatch({
    correlationId: "correlation-old-node",
    subscriptionId: "subscription-old-node",
    snapshot: {
      ...snapshot,
      runtime: {
        adapterId: "openai.codex.sdk",
        runtimeKind: "native_agent",
        streaming: true,
        continuation: true,
        scopedCancellation: true,
        deadlines: true,
        steering: "",
        nativeExtensionSchemas: [],
        controls: [],
      },
    },
  });

  expect(parsed.snapshot.runtime?.steering).toBe("unsupported");
});

describe("system bridge schema validation", () => {
  it("applies typed project, session, selection and health deltas without losing the runtime", () => {
    const delta = parseSystemEvent({
      kind: "delta",
      subscriptionId: "subscription-1",
      cursor: { streamId: "stream-1", sequence: "2", authorityEpoch: "7" },
      baseRevision: "1",
      newRevision: "2",
      mutations: [
        { kind: "removeProject", projectId: "project-1" },
        { kind: "removeSession", sessionId: "session-1" },
        { kind: "updateSelection", activeProject: { changed: true, value: null }, activeSession: { changed: true, value: null } },
        { kind: "updateHealth", nodeState: "health_state_degraded", statusCode: "offline", observedAtUnixMs: 1_750_000_001_000 },
      ],
    });
    const next = applySystemEvent({ ...snapshot, revision: "1" }, delta);
    expect(next).toMatchObject({
      revision: "2",
      projects: [],
      recentSessions: [],
      activeProjectId: null,
      activeSessionId: null,
      nodeState: "health_state_degraded",
      runtime: snapshot.runtime,
    });
    expect(applySystemEvent(next, delta)).toBe(next);
  });

  it("rejects numeric or malformed revisions before they reach the reducer", () => {
    expect(() => parseOpenedSystemWatch({
      correlationId: "correlation-1",
      subscriptionId: "subscription-1",
      snapshot: { ...snapshot, revision: Number.MAX_SAFE_INTEGER },
    })).toThrow("Invalid revision");
    expect(() => parseSystemEvent({
      kind: "heartbeat",
      subscriptionId: "subscription-1",
      cursor: { streamId: "stream-1", sequence: "2.5", authorityEpoch: "7" },
      currentRevision: "12",
      observedAtUnixMs: null,
    })).toThrow("Invalid sequence");
  });
});
