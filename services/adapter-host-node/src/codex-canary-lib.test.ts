import assert from "node:assert/strict";
import { test } from "node:test";

import type {
  Input,
  ThreadEvent,
  ThreadItem,
  ThreadOptions,
  TurnOptions,
} from "@openai/codex-sdk";

import {
  CodexCanaryError,
  type CanaryCodexClient,
  type CanaryThread,
  runSubscriptionCanary,
} from "./codex-canary-lib.js";

async function* stream(events: ThreadEvent[]): AsyncGenerator<ThreadEvent> {
  for (const event of events) {
    yield event;
  }
}

class FakeThread implements CanaryThread {
  constructor(
    private currentId: string | null,
    private readonly events: ThreadEvent[],
  ) {}

  get id(): string | null {
    return this.currentId;
  }

  async runStreamed(
    _input: Input,
    _options?: TurnOptions,
  ): Promise<{ events: AsyncGenerator<ThreadEvent> }> {
    const started = this.events.find((event) => event.type === "thread.started");
    if (started?.type === "thread.started") {
      this.currentId = started.thread_id;
    }
    return { events: stream(this.events) };
  }
}

function completedEvents(
  threadId: string,
  messageId: string,
  privateResponse: string,
): ThreadEvent[] {
  return [
    { type: "thread.started", thread_id: threadId },
    { type: "turn.started" },
    {
      type: "item.completed",
      item: { id: messageId, type: "agent_message", text: privateResponse },
    },
    {
      type: "turn.completed",
      usage: {
        input_tokens: 111,
        cached_input_tokens: 0,
        output_tokens: 222,
        reasoning_output_tokens: 0,
      },
    },
  ];
}

test("canary proves streaming and observed continuation without exposing private payloads", async () => {
  const privateThreadId = "thread-private-value";
  const privateResponse = "response-private-value";
  let resumedWith: string | null = null;
  const client: CanaryCodexClient = {
    startThread: (_options?: ThreadOptions) =>
      new FakeThread(
        null,
        completedEvents(privateThreadId, "message-1", privateResponse),
      ),
    resumeThread: (id: string, _options?: ThreadOptions) => {
      resumedWith = id;
      return new FakeThread(
        id,
        completedEvents(privateThreadId, "message-2", privateResponse),
      );
    },
  };
  const clockValues = [100, 112, 200, 209];
  const report = await runSubscriptionCanary(client, {
    workingDirectory: "C:/synthetic-git-workspace",
    firstPrompt: "prompt-private-value",
    continuationPrompt: "continuation-private-value",
    clock: () => clockValues.shift() ?? 209,
  });

  assert.equal(resumedWith, privateThreadId);
  assert.equal(report.firstTurn.terminal, "completed");
  assert.equal(report.firstTurn.latencyMs, 12);
  assert.equal(report.continuation.sameThread, true);
  assert.equal(report.continuation.latencyMs, 9);
  const serialized = JSON.stringify(report);
  assert.doesNotMatch(serialized, /private-value/);
  assert.doesNotMatch(serialized, /111|222/);
});

test("canary rejects a failed stream without copying the provider error", async () => {
  const client: CanaryCodexClient = {
    startThread: () =>
      new FakeThread(null, [
        { type: "thread.started", thread_id: "thread-id" },
        {
          type: "turn.failed",
          error: { message: "provider-private-error" },
        },
      ]),
    resumeThread: () => {
      throw new Error("not reached");
    },
  };

  await assert.rejects(
    runSubscriptionCanary(client, {
      workingDirectory: "C:/synthetic-git-workspace",
      firstPrompt: "prompt-private-value",
      continuationPrompt: "continuation-private-value",
    }),
    (error: unknown) =>
      error instanceof CodexCanaryError && error.code === "stream_failed",
  );
});

test("canary rejects tool and external-effect item types", async () => {
  const client: CanaryCodexClient = {
    startThread: () =>
      new FakeThread(null, [
        { type: "thread.started", thread_id: "thread-id" },
        { type: "turn.started" },
        {
          type: "item.started",
          item: {
            id: "command-1",
            type: "command_execution",
            command: "private-command",
            aggregated_output: "private-output",
            status: "in_progress",
          },
        },
      ]),
    resumeThread: () => {
      throw new Error("not reached");
    },
  };

  await assert.rejects(
    runSubscriptionCanary(client, {
      workingDirectory: "C:/synthetic-git-workspace",
      firstPrompt: "prompt-private-value",
      continuationPrompt: "continuation-private-value",
    }),
    (error: unknown) => {
      assert.ok(error instanceof CodexCanaryError);
      assert.equal(error.code, "unexpected_item_type");
      assert.deepEqual(error.safeDetail, {
        itemClass: "tool_or_external_effect",
      });
      assert.doesNotMatch(JSON.stringify(error.safeDetail), /private/);
      return true;
    },
  );
});

test("canary classifies an error item without exposing its message", async () => {
  const client: CanaryCodexClient = {
    startThread: () =>
      new FakeThread(null, [
        { type: "thread.started", thread_id: "thread-id" },
        { type: "turn.started" },
        {
          type: "item.completed",
          item: {
            id: "error-1",
            type: "error",
            message: "Private request reached a usage limit for private-account-id",
          },
        },
      ]),
    resumeThread: () => {
      throw new Error("not reached");
    },
  };

  await assert.rejects(
    runSubscriptionCanary(client, {
      workingDirectory: "C:/synthetic-git-workspace",
      firstPrompt: "first",
      continuationPrompt: "second",
    }),
    (error: unknown) => {
      assert.ok(error instanceof CodexCanaryError);
      assert.deepEqual(error.safeDetail, {
        itemClass: "provider_error",
        errorClass: "rate_limit",
      });
      assert.doesNotMatch(JSON.stringify(error.safeDetail), /private|account/);
      return true;
    },
  );
});

test("canary normalizes an unknown runtime item type", async () => {
  const privateItem = {
    id: "future-item",
    type: "private-future-provider-item",
  } as unknown as ThreadItem;
  const client: CanaryCodexClient = {
    startThread: () =>
      new FakeThread(null, [
        { type: "thread.started", thread_id: "thread-id" },
        { type: "turn.started" },
        { type: "item.started", item: privateItem },
      ]),
    resumeThread: () => {
      throw new Error("not reached");
    },
  };

  await assert.rejects(
    runSubscriptionCanary(client, {
      workingDirectory: "C:/synthetic-git-workspace",
      firstPrompt: "first",
      continuationPrompt: "second",
    }),
    (error: unknown) => {
      assert.ok(error instanceof CodexCanaryError);
      assert.deepEqual(error.safeDetail, { itemClass: "unknown" });
      assert.doesNotMatch(JSON.stringify(error.safeDetail), /private|future/);
      return true;
    },
  );
});

test("canary rejects malformed stream ordering", async () => {
  const malformedStreams: ThreadEvent[][] = [
    [
      { type: "turn.started" },
      { type: "thread.started", thread_id: "thread-id" },
    ],
    [
      { type: "thread.started", thread_id: "thread-id" },
      {
        type: "item.completed",
        item: { id: "message", type: "agent_message", text: "ack" },
      },
    ],
    [
      { type: "thread.started", thread_id: "thread-id" },
      { type: "turn.started" },
      { type: "turn.started" },
    ],
  ];

  for (const events of malformedStreams) {
    const client: CanaryCodexClient = {
      startThread: () => new FakeThread(null, events),
      resumeThread: () => {
        throw new Error("not reached");
      },
    };
    await assert.rejects(
      runSubscriptionCanary(client, {
        workingDirectory: "C:/synthetic-git-workspace",
        firstPrompt: "first",
        continuationPrompt: "second",
      }),
      (error: unknown) =>
        error instanceof CodexCanaryError &&
        error.code === "stream_protocol_violation",
    );
  }
});

test("canary rejects malformed runtime event fields", async () => {
  const validUsage = {
    input_tokens: 1,
    cached_input_tokens: 0,
    output_tokens: 1,
    reasoning_output_tokens: 0,
  };
  const malformedStreams = [
    [
      { type: "thread.started", thread_id: 42 },
      { type: "turn.started" },
    ],
    [
      { type: "thread.started", thread_id: "thread-id" },
      { type: "turn.started" },
      {
        type: "item.completed",
        item: { id: "", type: "agent_message", text: 42 },
      },
      { type: "turn.completed", usage: validUsage },
    ],
    [
      { type: "thread.started", thread_id: "thread-id" },
      { type: "turn.started" },
      {
        type: "item.completed",
        item: { id: "message", type: "agent_message", text: "ack" },
      },
      {
        type: "turn.completed",
        usage: { ...validUsage, output_tokens: -1 },
      },
    ],
  ] as unknown as ThreadEvent[][];

  for (const events of malformedStreams) {
    const client: CanaryCodexClient = {
      startThread: () => new FakeThread(null, events),
      resumeThread: () => {
        throw new Error("not reached");
      },
    };
    await assert.rejects(
      runSubscriptionCanary(client, {
        workingDirectory: "C:/synthetic-git-workspace",
        firstPrompt: "first",
        continuationPrompt: "second",
      }),
      (error: unknown) =>
        error instanceof CodexCanaryError &&
        error.code === "stream_protocol_violation",
    );
  }
});

test("canary rejects a continuation whose streamed thread identity changes", async () => {
  const client: CanaryCodexClient = {
    startThread: () =>
      new FakeThread(
        null,
        completedEvents("thread-original", "message-1", "ack"),
      ),
    resumeThread: () =>
      new FakeThread(
        "thread-original",
        completedEvents("thread-different", "message-2", "ack"),
      ),
  };

  await assert.rejects(
    runSubscriptionCanary(client, {
      workingDirectory: "C:/synthetic-git-workspace",
      firstPrompt: "first",
      continuationPrompt: "second",
    }),
    (error: unknown) =>
      error instanceof CodexCanaryError &&
      error.code === "thread_id_mismatch",
  );
});

test("canary aborts and normalizes a turn that exceeds its deadline", async () => {
  let observedAbort = false;
  const waitingThread: CanaryThread = {
    id: null,
    runStreamed: async (_input: Input, options?: TurnOptions) =>
      new Promise((_resolve, reject) => {
        options?.signal?.addEventListener(
          "abort",
          () => {
            observedAbort = true;
            reject(new DOMException("private-timeout-detail", "AbortError"));
          },
          { once: true },
        );
      }),
  };
  const client: CanaryCodexClient = {
    startThread: () => waitingThread,
    resumeThread: () => {
      throw new Error("not reached");
    },
  };

  await assert.rejects(
    runSubscriptionCanary(client, {
      workingDirectory: "C:/synthetic-git-workspace",
      firstPrompt: "first",
      continuationPrompt: "second",
      timeoutMs: 20,
    }),
    (error: unknown) =>
      error instanceof CodexCanaryError && error.code === "turn_timeout",
  );
  assert.equal(observedAbort, true);
});

test("canary deadline remains bounded when iterator cleanup stalls", async () => {
  let returnCalled = false;
  const events = {
    [Symbol.asyncIterator]() {
      return this;
    },
    next: () => new Promise<IteratorResult<ThreadEvent>>(() => undefined),
    return: () => {
      returnCalled = true;
      return new Promise<IteratorResult<ThreadEvent>>(() => undefined);
    },
    throw: () => new Promise<IteratorResult<ThreadEvent>>(() => undefined),
  } as AsyncGenerator<ThreadEvent>;
  const client: CanaryCodexClient = {
    startThread: () => ({
      id: null,
      runStreamed: async () => ({ events }),
    }),
    resumeThread: () => {
      throw new Error("not reached");
    },
  };

  const startedAt = performance.now();
  await assert.rejects(
    runSubscriptionCanary(client, {
      workingDirectory: "C:/synthetic-git-workspace",
      firstPrompt: "first",
      continuationPrompt: "second",
      timeoutMs: 20,
    }),
    (error: unknown) =>
      error instanceof CodexCanaryError && error.code === "turn_timeout",
  );

  assert.equal(returnCalled, true);
  assert.ok(performance.now() - startedAt < 500);
});
