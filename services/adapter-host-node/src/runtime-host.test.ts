import assert from "node:assert/strict";
import { test } from "node:test";

import {
  type AgentRuntimeAdapter,
  type CancellationAcknowledgement,
  OpaqueContinuation,
  type RuntimeDescriptor,
  type RuntimeEvent,
  type RuntimeTurn,
  type RuntimeTurnRequest,
  type SteerRuntimeTurnRequest,
  type SteeringAcknowledgement,
} from "./runtime-contract.js";
import { RuntimeHost } from "./runtime-host.js";
import { RuntimeAdapterError } from "./runtime-contract.js";

class FakeAdapter implements AgentRuntimeAdapter {
  readonly continuation = new OpaqueContinuation("fake.adapter", "thread-private");

  async describe(): Promise<RuntimeDescriptor> {
    return {
      adapterId: "fake.adapter",
      runtimeKind: "native_agent",
      capabilities: {
        streaming: true,
        continuation: true,
        scopedCancellation: true,
        deadlines: true,
        steering: "native",
        nativeExtensionSchemas: [],
      },
      controls: [],
    };
  }

  async startTurn(request: RuntimeTurnRequest): Promise<RuntimeTurn> {
    const continuation = this.continuation;
    async function* events(): AsyncGenerator<RuntimeEvent> {
      yield {
        sessionId: request.sessionId,
        turnId: request.turnId,
        sequence: 1,
        kind: { type: "started", continuation },
        nativeExtensions: [],
      };
      yield {
        sessionId: request.sessionId,
        turnId: request.turnId,
        sequence: 2,
        kind: { type: "text_delta", text: "done" },
        nativeExtensions: [],
      };
      yield {
        sessionId: request.sessionId,
        turnId: request.turnId,
        sequence: 3,
        kind: {
          type: "terminal",
          outcome: { type: "completed" },
          continuation,
        },
        nativeExtensions: [],
      };
    }
    return { events: events() };
  }

  async cancelTurn(request: {
    sessionId: string;
    turnId: string;
  }): Promise<CancellationAcknowledgement> {
    return { ...request, disposition: { type: "requested" } };
  }

  async steerTurn(
    request: SteerRuntimeTurnRequest,
  ): Promise<SteeringAcknowledgement> {
    return {
      sessionId: request.sessionId,
      turnId: request.turnId,
      messageId: request.messageId,
    };
  }
}

test("runtime host acknowledges start before forwarding normalized events", async () => {
  const messages: Record<string, unknown>[] = [];
  const host = new RuntimeHost(new FakeAdapter(), (message) => {
    messages.push(message);
  });
  await host.handleLine(JSON.stringify({
    v: 1,
    id: "request-1",
    method: "start_turn",
    params: {
      sessionId: "session-1",
      turnId: "turn-1",
      prompt: "private prompt",
      workspacePath: "C:/workspace",
      timeoutMs: 1000,
      contextHandles: [],
    },
  }));
  await new Promise((resolve) => setImmediate(resolve));

  assert.deepEqual(messages[0], {
    v: 1,
    id: "request-1",
    result: { started: true },
  });
  const events = messages.slice(1).map((message) => message.payload as Record<string, unknown>);
  assert.equal(events.length, 3);
  assert.deepEqual((events[0].kind as Record<string, unknown>).continuation, {
    adapterId: "fake.adapter",
    handle: "thread-private",
  });
  assert.equal(JSON.stringify(messages).includes("private prompt"), false);
});

test("runtime host returns only safe typed failures", async () => {
  const messages: Record<string, unknown>[] = [];
  const host = new RuntimeHost(new FakeAdapter(), (message) => {
    messages.push(message);
  });
  await host.handleLine("not-json private-prompt");
  assert.deepEqual(messages, [{
    v: 1,
    id: null,
    error: {
      code: "provider_failure",
      retryable: true,
      recoverable: true,
    },
  }]);
  assert.equal(JSON.stringify(messages).includes("private-prompt"), false);
});

test("runtime host forwards native steering without starting another turn", async () => {
  const messages: Record<string, unknown>[] = [];
  const host = new RuntimeHost(new FakeAdapter(), (message) => {
    messages.push(message);
  });
  await host.handleLine(JSON.stringify({
    v: 1,
    id: "steer-1",
    method: "steer_turn",
    params: {
      sessionId: "session-1",
      turnId: "turn-1",
      messageId: "message-1",
      text: "Use the new constraint",
    },
  }));
  assert.deepEqual(messages, [{
    v: 1,
    id: "steer-1",
    result: {
      sessionId: "session-1",
      turnId: "turn-1",
      messageId: "message-1",
    },
  }]);
});

test("a slow steer does not block an independent Stop request", async () => {
  let releaseSteer: (() => void) | undefined;
  let steerEntered: (() => void) | undefined;
  const entered = new Promise<void>((resolve) => { steerEntered = resolve; });
  const gate = new Promise<void>((resolve) => { releaseSteer = resolve; });
  class SlowSteerAdapter extends FakeAdapter {
    override async steerTurn(request: SteerRuntimeTurnRequest): Promise<SteeringAcknowledgement> {
      steerEntered?.();
      await gate;
      return super.steerTurn(request);
    }
  }
  const messages: Record<string, unknown>[] = [];
  const host = new RuntimeHost(new SlowSteerAdapter(), (message) => { messages.push(message); });
  const steering = host.handleLine(JSON.stringify({
    v: 1,
    id: "steer-slow",
    method: "steer_turn",
    params: {
      sessionId: "session-1",
      turnId: "turn-1",
      messageId: "message-1",
      text: "slow clarification",
    },
  }));
  await entered;

  await host.handleLine(JSON.stringify({
    v: 1,
    id: "cancel-fast",
    method: "cancel_turn",
    params: { sessionId: "session-1", turnId: "turn-1" },
  }));
  assert.equal(messages[0]?.id, "cancel-fast");

  releaseSteer?.();
  await steering;
  assert.equal(messages[1]?.id, "steer-slow");
});

test("runtime host reports healthy only after the adapter is ready", async () => {
  class UnavailableAdapter extends FakeAdapter {
    override async describe(): Promise<RuntimeDescriptor> {
      throw new RuntimeAdapterError("provider_unavailable", false, true);
    }
  }
  const messages: Record<string, unknown>[] = [];
  const host = new RuntimeHost(
    new UnavailableAdapter(),
    (message) => {
      messages.push(message);
    },
  );
  await host.handleLine(JSON.stringify({
    v: 1,
    id: "health-1",
    method: "health",
    params: {},
  }));
  assert.deepEqual(messages, [{
    v: 1,
    id: "health-1",
    error: {
      code: "provider_unavailable",
      retryable: false,
      recoverable: true,
    },
  }]);
});

test("runtime host awaits output backpressure before accepting a start", async () => {
  const messages: Record<string, unknown>[] = [];
  let release: (() => void) | undefined;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  const host = new RuntimeHost(new FakeAdapter(), async (message) => {
    messages.push(message);
    if (message.id === "request-backpressure") await gate;
  });
  let settled = false;
  const handling = host.handleLine(JSON.stringify({
    v: 1,
    id: "request-backpressure",
    method: "start_turn",
    params: {
      sessionId: "session-1",
      turnId: "turn-1",
      prompt: "private prompt",
      workspacePath: "C:/workspace",
      timeoutMs: 1000,
      contextHandles: [],
    },
  })).then(() => {
    settled = true;
  });
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(settled, false);
  assert.equal(messages.length, 1);
  release?.();
  await handling;
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(settled, true);
  assert.equal(messages.length, 4);
});
