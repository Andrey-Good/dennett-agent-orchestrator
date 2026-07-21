import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import type {
  Input,
  ThreadEvent,
  ThreadOptions,
  TurnOptions,
} from "@openai/codex-sdk";

import {
  CODEX_RUNTIME_ADAPTER_ID,
  CodexRuntimeAdapter,
  DEFAULT_CODEX_THREAD_OPTIONS,
  type CodexClientLike,
  type CodexThreadLike,
} from "./codex-runtime-adapter.js";
import { runRuntimeAdapterCanary } from "./codex-runtime-canary.js";
import {
  OpaqueContinuation,
  RuntimeAdapterError,
  type RuntimeEvent,
  type RuntimeTurn,
  type RuntimeTurnRequest,
} from "./runtime-contract.js";

interface ConformanceCase {
  id: string;
  expected_events?: string[];
  expected_error?: string;
  expected_terminal_code?: string;
  expected_retryable?: boolean;
  expected_recoverable?: boolean;
}

interface ConformanceDocument {
  version: number;
  cases: ConformanceCase[];
}

const fixturePath = new URL(
  "../../../tests/contracts/agent_runtime_conformance.json",
  import.meta.url,
);
const fixture = JSON.parse(
  await readFile(fixturePath, "utf8"),
) as ConformanceDocument;
assert.equal(fixture.version, 1);

function conformanceCase(id: string): ConformanceCase {
  const value = fixture.cases.find((candidate) => candidate.id === id);
  assert.ok(value, `missing conformance case ${id}`);
  return value;
}

function providerEvent(value: Record<string, unknown>): ThreadEvent {
  return value as unknown as ThreadEvent;
}

function usageEvent(): ThreadEvent {
  return providerEvent({
    type: "turn.completed",
    usage: {
      input_tokens: 3,
      cached_input_tokens: 1,
      output_tokens: 2,
      reasoning_output_tokens: 0,
    },
  });
}

const waitForAbort = Symbol("wait-for-abort");
type ScriptStep = ThreadEvent | typeof waitForAbort;

class ScriptedThread implements CodexThreadLike {
  readonly id: string | null = null;
  readonly inputs: Input[] = [];
  readonly steers: Array<{ input: Input; clientMessageId: string }> = [];
  closed = false;

  constructor(private readonly script: ScriptStep[]) {}

  async runStreamed(
    input: Input,
    options?: TurnOptions,
  ): Promise<{ events: AsyncGenerator<ThreadEvent> }> {
    this.inputs.push(input);
    const signal = options?.signal;
    const script = this.script;
    const self = this;
    return {
      events: (async function* () {
        try {
          for (const step of script) {
            if (step === waitForAbort) {
              await new Promise<void>((resolve) => {
                if (signal?.aborted) {
                  resolve();
                } else {
                  signal?.addEventListener("abort", () => resolve(), {
                    once: true,
                  });
                }
              });
            } else {
              yield step;
            }
          }
        } finally {
          self.closed = true;
        }
      })(),
    };
  }

  async steer(input: Input, clientMessageId: string): Promise<void> {
    this.steers.push({ input, clientMessageId });
  }
}

class TerminalizingSteerThread implements CodexThreadLike {
  readonly id: string | null = null;
  readonly steers: Array<{ input: Input; clientMessageId: string }> = [];
  readonly #releaseStream: Promise<void>;
  readonly #streamFinished: Promise<void>;
  #resolveStream!: () => void;
  #resolveFinished!: () => void;

  constructor() {
    this.#releaseStream = new Promise((resolve) => { this.#resolveStream = resolve; });
    this.#streamFinished = new Promise((resolve) => { this.#resolveFinished = resolve; });
  }

  async runStreamed(input: Input): Promise<{ events: AsyncGenerator<ThreadEvent> }> {
    const release = this.#releaseStream;
    const finish = this.#resolveFinished;
    return {
      events: (async function* () {
        yield providerEvent({ type: "thread.started", thread_id: "thread-ack-race" });
        yield providerEvent({ type: "turn.started" });
        await release;
        yield providerEvent({
          type: "item.completed",
          item: { id: "message-ack-race", type: "agent_message", text: String(input) },
        });
        yield usageEvent();
        finish();
      })(),
    };
  }

  async steer(input: Input, clientMessageId: string): Promise<void> {
    this.steers.push({ input, clientMessageId });
    this.#resolveStream();
    await this.#streamFinished;
  }
}

class ScriptedClient implements CodexClientLike {
  readonly startOptions: Array<ThreadOptions | undefined> = [];
  readonly resumeCalls: Array<{
    id: string;
    options: ThreadOptions | undefined;
  }> = [];

  constructor(
    private readonly starts: CodexThreadLike[],
    private readonly resumes: CodexThreadLike[] = [],
    private readonly resumeFailure?: Error,
  ) {}

  startThread(options?: ThreadOptions): CodexThreadLike {
    this.startOptions.push(options);
    const thread = this.starts.shift();
    if (!thread) {
      throw new Error("missing scripted start thread");
    }
    return thread;
  }

  resumeThread(id: string, options?: ThreadOptions): CodexThreadLike {
    this.resumeCalls.push({ id, options });
    if (this.resumeFailure) {
      throw this.resumeFailure;
    }
    const thread = this.resumes.shift();
    if (!thread) {
      throw new Error("missing scripted resume thread");
    }
    return thread;
  }
}

function request(
  sessionId: string,
  turnId: string,
  overrides: Partial<RuntimeTurnRequest> = {},
): RuntimeTurnRequest {
  return {
    sessionId,
    turnId,
    prompt: "private synthetic prompt",
    workspacePath: "C:/synthetic/project",
    timeoutMs: 5_000,
    ...overrides,
  };
}

async function collect(turn: RuntimeTurn): Promise<RuntimeEvent[]> {
  const events: RuntimeEvent[] = [];
  for await (const event of turn.events) {
    events.push(event);
  }
  return events;
}

function eventLabels(events: RuntimeEvent[]): string[] {
  return events.map((event) => {
    if (event.kind.type !== "terminal") {
      return event.kind.type;
    }
    return event.kind.outcome.type;
  });
}

function conformanceEventLabels(events: RuntimeEvent[]): string[] {
  return eventLabels(events).filter((label) => label !== "progress");
}

function completedScript(
  threadId: string,
  text: string,
  progress = false,
): ScriptStep[] {
  const events: ScriptStep[] = [
    providerEvent({ type: "thread.started", thread_id: threadId }),
    providerEvent({ type: "turn.started" }),
    providerEvent({
      type: "item.completed",
      item: { id: "message-1", type: "agent_message", text },
    }),
  ];
  if (progress) {
    events.push(
      providerEvent({
        type: "item.completed",
        item: {
          id: "command-1",
          type: "command_execution",
          status: "completed",
          command: "private command",
          aggregated_output: "private output",
        },
      }),
    );
  }
  events.push(usageEvent());
  return events;
}

test("provider controls select the bound Codex client and thread model per turn", async () => {
  const selectedThread = new ScriptedThread(completedScript("thread-controls", "configured"));
  const defaultClient = new ScriptedClient([]);
  const selectedClient = new ScriptedClient([selectedThread]);
  let observed: Array<{ controlId: string; choiceId: string }> = [];
  const controls = [{
    id: "model",
    label: "Model",
    defaultChoiceId: "gpt-default",
    choices: [{
      id: "gpt-selected",
      label: "GPT Selected",
      availableWhen: [],
    }],
  }];
  const adapter = new CodexRuntimeAdapter(defaultClient, {
    controls,
    resolveRuntimeControls(selections) {
      observed = selections.map((selection) => ({ ...selection }));
      return { client: selectedClient, threadOptions: { model: "gpt-selected" } };
    },
  });

  assert.deepEqual((await adapter.describe()).controls, controls);
  const events = await collect(await adapter.startTurn(request(
    "session-controls",
    "turn-controls",
    { runtimeControls: [{ controlId: "model", choiceId: "gpt-selected" }] },
  )));
  assert.equal(events.at(-1)?.kind.type, "terminal");
  assert.deepEqual(observed, [{ controlId: "model", choiceId: "gpt-selected" }]);
  assert.equal(defaultClient.startOptions.length, 0);
  assert.equal(selectedClient.startOptions[0]?.model, "gpt-selected");
});

test("TEST-AGENT-RUNTIME-STREAM-001 normalizes an ordered Codex stream", async () => {
  const contract = conformanceCase("ordered_stream");
  const thread = new ScriptedThread(completedScript("thread-1", "hello", true));
  const client = new ScriptedClient([thread]);
  const adapter = new CodexRuntimeAdapter(client);

  const descriptor = await adapter.describe();
  assert.equal(descriptor.adapterId, CODEX_RUNTIME_ADAPTER_ID);
  assert.equal(descriptor.capabilities.streaming, true);
  assert.equal(descriptor.capabilities.continuation, true);

  const events = await collect(
    await adapter.startTurn(request("session-a", "turn-a")),
  );
  assert.deepEqual(
    conformanceEventLabels(events),
    (contract.expected_events ?? []).filter((label) => label !== "progress"),
  );
  assert.deepEqual(
    events.map((event) => event.sequence),
    [1, 2, 3, 4, 5, 6],
  );
  assert.ok(
    events.every(
      (event) =>
        event.sessionId === "session-a" && event.turnId === "turn-a",
    ),
  );
  assert.equal(client.startOptions[0]?.workingDirectory, "C:/synthetic/project");
  assert.equal(
    client.startOptions[0]?.sandboxMode,
    DEFAULT_CODEX_THREAD_OPTIONS.sandboxMode,
  );
  assert.equal(client.startOptions[0]?.approvalPolicy, "never");
  assert.equal(client.startOptions[0]?.networkAccessEnabled, false);
  assert.equal(client.startOptions[0]?.skipGitRepoCheck, true);
  assert.equal(thread.closed, true);

  const serialized = JSON.stringify(events);
  assert.doesNotMatch(serialized, /private command|private output/);
  assert.match(serialized, /openai\.codex\.item-status/);
});

test("native steering appends input to the same active Codex turn", async () => {
  const thread = new ScriptedThread([
    providerEvent({ type: "thread.started", thread_id: "thread-steer" }),
    providerEvent({ type: "turn.started" }),
    waitForAbort,
  ]);
  const adapter = new CodexRuntimeAdapter(new ScriptedClient([thread]), {
    steering: "native",
  });
  assert.equal((await adapter.describe()).capabilities.steering, "native");

  const turn = await adapter.startTurn(request("session-steer", "turn-steer"));
  const started = await turn.events.next();
  assert.equal(started.value?.kind.type, "started");
  const waiting = turn.events.next();
  await new Promise((resolve) => setImmediate(resolve));

  assert.deepEqual(await adapter.steerTurn({
    sessionId: "session-steer",
    turnId: "turn-steer",
    messageId: "message-steer",
    text: "Keep the current run and add this constraint",
  }), {
    sessionId: "session-steer",
    turnId: "turn-steer",
    messageId: "message-steer",
  });
  assert.deepEqual(thread.steers, [{
    input: "Keep the current run and add this constraint",
    clientMessageId: "message-steer",
  }]);

  await adapter.cancelTurn({
    sessionId: "session-steer",
    turnId: "turn-steer",
  });
  const terminal = await waiting;
  assert.equal(terminal.value?.kind.type, "terminal");
  if (terminal.value?.kind.type === "terminal") {
    assert.equal(terminal.value.kind.outcome.type, "cancelled");
  }
});

test("a provider steer acknowledgement remains accepted when the turn completes concurrently", async () => {
  const thread = new TerminalizingSteerThread();
  const adapter = new CodexRuntimeAdapter(new ScriptedClient([thread]), {
    steering: "native",
  });
  const turn = await adapter.startTurn(request("session-ack-race", "turn-ack-race"));
  const events = collect(turn);

  const acknowledgement = await adapter.steerTurn({
    sessionId: "session-ack-race",
    turnId: "turn-ack-race",
    messageId: "message-ack-race",
    text: "accepted at the terminal boundary",
  });

  assert.deepEqual(acknowledgement, {
    sessionId: "session-ack-race",
    turnId: "turn-ack-race",
    messageId: "message-ack-race",
  });
  assert.equal(eventLabels(await events).at(-1), "completed");
});

test("steering rejects inactive or non-native turns without cancelling them", async () => {
  const adapter = new CodexRuntimeAdapter(new ScriptedClient([]));
  await assert.rejects(
    adapter.steerTurn({
      sessionId: "missing-session",
      turnId: "missing-turn",
      messageId: "message",
      text: "clarification",
    }),
    (error: unknown) => error instanceof RuntimeAdapterError && error.code === "scope_mismatch",
  );
});

test("streams text deltas and preserves the safe provider activity lifecycle", async () => {
  const thread = new ScriptedThread([
    providerEvent({ type: "thread.started", thread_id: "thread-activity" }),
    providerEvent({ type: "turn.started" }),
    providerEvent({
      type: "item.started",
      item: { id: "reasoning-1", type: "reasoning", text: "Checking" },
    }),
    providerEvent({
      type: "item.updated",
      item: { id: "reasoning-1", type: "reasoning", text: "Checking the request" },
    }),
    providerEvent({
      type: "item.completed",
      item: { id: "reasoning-1", type: "reasoning", text: "Request checked" },
    }),
    providerEvent({
      type: "item.started",
      item: { id: "message-1", type: "agent_message", text: "" },
    }),
    providerEvent({
      type: "item.updated",
      item: { id: "message-1", type: "agent_message", text: "Hello" },
    }),
    providerEvent({
      type: "item.completed",
      item: { id: "message-1", type: "agent_message", text: "Hello owner" },
    }),
    providerEvent({
      type: "item.completed",
      item: {
        id: "command-1",
        type: "command_execution",
        status: "completed",
        command: "private command",
        aggregated_output: "private output",
      },
    }),
    usageEvent(),
  ]);
  const events = await collect(
    await new CodexRuntimeAdapter(new ScriptedClient([thread])).startTurn(
      request("session-activity", "turn-activity"),
    ),
  );

  assert.deepEqual(
    events.flatMap((event) =>
      event.kind.type === "text_delta" ? [event.kind.text] : [],
    ),
    ["Hello owner"],
  );
  assert.deepEqual(
    events.flatMap((event) =>
      event.kind.type === "progress"
        ? [{
            activityId: event.kind.activityId,
            phase: event.kind.phase,
            message: event.kind.message,
            status: event.kind.status,
          }]
        : [],
    ),
    [
      { activityId: "message-1", phase: "commentary", message: undefined, status: "started" },
      { activityId: "message-1", phase: "commentary", message: "Hello", status: "updated" },
      { activityId: "message-1", phase: "commentary", message: "Hello owner", status: "completed" },
      { activityId: "command-1", phase: "command", message: undefined, status: "completed" },
    ],
  );
  assert.doesNotMatch(JSON.stringify(events), /private command|private output/);
});

test("interleaves concise commentary with effects and keeps only the final agent message as output", async () => {
  const thread = new ScriptedThread([
    providerEvent({ type: "thread.started", thread_id: "thread-owner-timeline" }),
    providerEvent({ type: "turn.started" }),
    providerEvent({
      type: "item.completed",
      item: { id: "reasoning-private", type: "reasoning", text: "private low-level chain" },
    }),
    providerEvent({
      type: "item.completed",
      item: { id: "commentary-1", type: "agent_message", text: "I will inspect the project." },
    }),
    providerEvent({
      type: "item.completed",
      item: { id: "command-1", type: "command_execution", status: "completed" },
    }),
    providerEvent({
      type: "item.completed",
      item: { id: "commentary-2", type: "agent_message", text: "The first check passed." },
    }),
    providerEvent({
      type: "item.completed",
      item: { id: "command-2", type: "command_execution", status: "completed" },
    }),
    providerEvent({
      type: "item.completed",
      item: { id: "answer", type: "agent_message", text: "## Result\n\nEverything is ready." },
    }),
    usageEvent(),
  ]);
  const events = await collect(
    await new CodexRuntimeAdapter(new ScriptedClient([thread])).startTurn(
      request("session-owner-timeline", "turn-owner-timeline"),
    ),
  );

  assert.deepEqual(
    events.flatMap((event) => event.kind.type === "progress"
      ? [[event.kind.phase, event.kind.message]]
      : []),
    [
      ["commentary", "I will inspect the project."],
      ["command", undefined],
      ["commentary", "The first check passed."],
      ["command", undefined],
      ["commentary", "## Result\n\nEverything is ready."],
    ],
  );
  assert.deepEqual(
    events.flatMap((event) => event.kind.type === "text_delta" ? [event.kind.text] : []),
    ["## Result\n\nEverything is ready."],
  );
  assert.doesNotMatch(JSON.stringify(events), /private low-level chain/);
});

test("uses native agent-message phases instead of guessing the final answer", async () => {
  const thread = new ScriptedThread([
    providerEvent({ type: "thread.started", thread_id: "thread-native-phases" }),
    providerEvent({ type: "turn.started" }),
    providerEvent({
      type: "item.completed",
      item: {
        id: "owner-update",
        type: "agent_message",
        text: "I checked the repository.",
        phase: "commentary",
      },
    }),
    providerEvent({
      type: "item.completed",
      item: {
        id: "final-answer",
        type: "agent_message",
        text: "## Result\n\nReady.",
        phase: "final_answer",
      },
    }),
    usageEvent(),
  ]);
  const events = await collect(
    await new CodexRuntimeAdapter(new ScriptedClient([thread])).startTurn(
      request("session-native-phases", "turn-native-phases"),
    ),
  );

  assert.deepEqual(
    events.flatMap((event) => event.kind.type === "progress"
      ? [[event.kind.phase, event.kind.message]]
      : []),
    [["commentary", "I checked the repository."]],
  );
  assert.deepEqual(
    events.flatMap((event) => event.kind.type === "text_delta" ? [event.kind.text] : []),
    ["## Result\n\nReady."],
  );
});

for (const malformed of [
  {
    name: "an update before item start",
    events: [providerEvent({
      type: "item.updated",
      item: { id: "item-1", type: "agent_message", text: "late" },
    })],
  },
  {
    name: "an item type change",
    events: [
      providerEvent({
        type: "item.started",
        item: { id: "item-1", type: "agent_message", text: "" },
      }),
      providerEvent({
        type: "item.updated",
        item: { id: "item-1", type: "reasoning", text: "changed" },
      }),
    ],
  },
  {
    name: "an update after item completion",
    events: [
      providerEvent({
        type: "item.completed",
        item: { id: "item-1", type: "agent_message", text: "done" },
      }),
      providerEvent({
        type: "item.updated",
        item: { id: "item-1", type: "agent_message", text: "done late" },
      }),
    ],
  },
  {
    name: "duplicate item completion",
    events: [
      providerEvent({
        type: "item.completed",
        item: { id: "item-1", type: "agent_message", text: "done" },
      }),
      providerEvent({
        type: "item.completed",
        item: { id: "item-1", type: "agent_message", text: "done" },
      }),
    ],
  },
]) {
  test(`rejects ${malformed.name}`, async () => {
    const thread = new ScriptedThread([
      providerEvent({ type: "thread.started", thread_id: "thread-malformed-item" }),
      providerEvent({ type: "turn.started" }),
      ...malformed.events,
    ]);
    const turn = await new CodexRuntimeAdapter(new ScriptedClient([thread]))
      .startTurn(request("session-malformed-item", "turn-malformed-item"));

    await assert.rejects(
      collect(turn),
      (error: unknown) =>
        error instanceof RuntimeAdapterError
        && error.code === "protocol_violation",
    );
  });
}

test("TEST-AGENT-RUNTIME-CANCEL-001 scopes and acknowledges Stop", async () => {
  const contract = conformanceCase("scoped_cancellation");
  const threadA = new ScriptedThread([
    providerEvent({ type: "thread.started", thread_id: "thread-a" }),
    providerEvent({ type: "turn.started" }),
    waitForAbort,
    providerEvent({
      type: "item.completed",
      item: { id: "late", type: "agent_message", text: "late" },
    }),
    usageEvent(),
  ]);
  const threadB = new ScriptedThread(completedScript("thread-b", "kept"));
  const adapter = new CodexRuntimeAdapter(
    new ScriptedClient([threadA, threadB]),
  );
  const turnA = await adapter.startTurn(request("session-a", "turn-a"));
  const turnB = await adapter.startTurn(request("session-b", "turn-b"));

  const started = await turnA.events.next();
  assert.equal(started.value?.kind.type, "started");
  assert.deepEqual(
    (
      await adapter.cancelTurn({
        sessionId: "session-b",
        turnId: "turn-a",
      })
    ).disposition,
    { type: "not_found" },
  );
  assert.deepEqual(
    (
      await adapter.cancelTurn({
        sessionId: "session-a",
        turnId: "turn-a",
      })
    ).disposition,
    { type: "requested" },
  );
  assert.deepEqual(
    (
      await adapter.cancelTurn({
        sessionId: "session-a",
        turnId: "turn-a",
      })
    ).disposition,
    { type: "already_requested" },
  );

  const cancelled = await turnA.events.next();
  assert.equal(cancelled.value?.kind.type, "terminal");
  const labels = [started.value, cancelled.value]
    .filter((event): event is RuntimeEvent => event !== undefined)
    .map((event) =>
      event.kind.type === "terminal"
        ? event.kind.outcome.type
        : event.kind.type,
    );
  assert.deepEqual(labels, contract.expected_events);
  assert.equal((await turnA.events.next()).done, true);
  assert.deepEqual(eventLabels(await collect(turnB)), [
    "started",
    "progress",
    "text_delta",
    "usage",
    "completed",
  ]);
  assert.deepEqual(
    (
      await adapter.cancelTurn({
        sessionId: "session-a",
        turnId: "turn-a",
      })
    ).disposition,
    { type: "already_terminal", terminal: "cancelled" },
  );
  assert.equal(threadA.closed, true);
});

test("an unconsumed stream can be closed or stopped without leaking active scope", async () => {
  const abandoned = new ScriptedThread(completedScript("thread-unused", "unused"));
  const replacement = new ScriptedThread(
    completedScript("thread-replacement", "replacement"),
  );
  const stopped = new ScriptedThread(completedScript("thread-stopped", "late"));
  const dropped = new ScriptedThread(completedScript("thread-dropped", "late"));
  const adapter = new CodexRuntimeAdapter(
    new ScriptedClient([abandoned, replacement, stopped, dropped]),
  );
  const scope = request("session-dispose", "turn-dispose");

  const abandonedTurn = await adapter.startTurn(scope);
  await abandonedTurn.events.return(undefined);
  assert.equal(abandoned.inputs.length, 0);
  assert.deepEqual(
    (
      await adapter.cancelTurn({
        sessionId: scope.sessionId,
        turnId: scope.turnId,
      })
    ).disposition,
    { type: "not_found" },
  );
  assert.deepEqual(eventLabels(await collect(await adapter.startTurn(scope))), [
    "started",
    "progress",
    "text_delta",
    "usage",
    "completed",
  ]);

  const stoppedScope = request("session-dispose", "turn-stopped");
  const stoppedTurn = await adapter.startTurn(stoppedScope);
  assert.deepEqual(
    (
      await adapter.cancelTurn({
        sessionId: stoppedScope.sessionId,
        turnId: stoppedScope.turnId,
      })
    ).disposition,
    { type: "requested" },
  );
  await stoppedTurn.events.return(undefined);
  assert.deepEqual(
    (
      await adapter.cancelTurn({
        sessionId: stoppedScope.sessionId,
        turnId: stoppedScope.turnId,
      })
    ).disposition,
    { type: "already_terminal", terminal: "cancelled" },
  );
  assert.equal(stopped.inputs.length, 0);

  const droppedScope = request("session-dispose", "turn-dropped", {
    timeoutMs: 20,
  });
  await adapter.startTurn(droppedScope);
  await new Promise((resolve) => setTimeout(resolve, 30));
  assert.deepEqual(
    (
      await adapter.cancelTurn({
        sessionId: droppedScope.sessionId,
        turnId: droppedScope.turnId,
      })
    ).disposition,
    { type: "already_terminal", terminal: "timed_out" },
  );
  assert.equal(dropped.inputs.length, 0);
});

test("a stale stream cannot terminalize a reused scope after history eviction", async () => {
  const oldThread = new ScriptedThread(completedScript("thread-old", "old"));
  const evictionThread = new ScriptedThread(
    completedScript("thread-eviction", "eviction"),
  );
  const replacementThread = new ScriptedThread(
    completedScript("thread-new", "new"),
  );
  const adapter = new CodexRuntimeAdapter(
    new ScriptedClient([oldThread, evictionThread, replacementThread]),
    { terminalHistoryLimit: 1 },
  );
  const reusedScope = request("session-reused", "turn-reused", {
    timeoutMs: 20,
  });
  const oldTurn = await adapter.startTurn(reusedScope);
  await new Promise((resolve) => setTimeout(resolve, 30));
  await collect(
    await adapter.startTurn(request("session-eviction", "turn-eviction")),
  );

  const replacement = await adapter.startTurn({
    ...reusedScope,
    timeoutMs: 5_000,
  });
  assert.deepEqual(eventLabels(await collect(oldTurn)), [
    "started",
    "timed_out",
  ]);
  assert.deepEqual(
    (
      await adapter.cancelTurn({
        sessionId: reusedScope.sessionId,
        turnId: reusedScope.turnId,
      })
    ).disposition,
    { type: "requested" },
  );
  assert.deepEqual(eventLabels(await collect(replacement)), [
    "started",
    "cancelled",
  ]);
  assert.equal(oldThread.inputs.length, 0);
});

test("TEST-AGENT-RUNTIME-TIMEOUT-001 preserves partial output and drops late completion", async () => {
  const contract = conformanceCase("partial_timeout");
  const thread = new ScriptedThread([
    providerEvent({ type: "thread.started", thread_id: "thread-timeout" }),
    providerEvent({ type: "turn.started" }),
    providerEvent({
      type: "item.completed",
      item: { id: "partial", type: "agent_message", text: "partial" },
    }),
    waitForAbort,
    usageEvent(),
  ]);
  const adapter = new CodexRuntimeAdapter(new ScriptedClient([thread]));
  const events = await collect(
    await adapter.startTurn(
      request("session-timeout", "turn-timeout", { timeoutMs: 20 }),
    ),
  );

  assert.deepEqual(conformanceEventLabels(events), contract.expected_events);
  const terminal = events.at(-1);
  assert.equal(terminal?.kind.type, "terminal");
  if (terminal?.kind.type === "terminal") {
    assert.deepEqual(terminal.kind.outcome, {
      type: "timed_out",
      partial: true,
    });
  }
  assert.deepEqual(
    (
      await adapter.cancelTurn({
        sessionId: "session-timeout",
        turnId: "turn-timeout",
      })
    ).disposition,
    { type: "already_terminal", terminal: "timed_out" },
  );
  assert.equal(thread.closed, true);
});

test("completion becomes authoritative before usage is exposed", async () => {
  const adapter = new CodexRuntimeAdapter(
    new ScriptedClient([
      new ScriptedThread(completedScript("thread-complete", "done")),
    ]),
  );
  const turn = await adapter.startTurn(
    request("session-complete", "turn-complete"),
  );

  let event = await turn.events.next();
  while (!event.done && event.value.kind.type !== "usage") {
    event = await turn.events.next();
  }
  assert.equal(event.value?.kind.type, "usage");
  assert.deepEqual(
    (
      await adapter.cancelTurn({
        sessionId: "session-complete",
        turnId: "turn-complete",
      })
    ).disposition,
    { type: "already_terminal", terminal: "completed" },
  );
  const terminal = await turn.events.next();
  assert.equal(
    terminal.value?.kind.type === "terminal"
      ? terminal.value.kind.outcome.type
      : undefined,
    "completed",
  );
  assert.equal((await turn.events.next()).done, true);
});

test("TEST-CODEX-SDK-CONTINUATION-001 keeps Codex thread state opaque and resumable", async () => {
  const contract = conformanceCase("opaque_continuation");
  const firstThread = new ScriptedThread(completedScript("thread-private", "one"));
  const resumedThread = new ScriptedThread(
    completedScript("thread-private", "continued"),
  );
  const client = new ScriptedClient([firstThread], [resumedThread]);
  const adapter = new CodexRuntimeAdapter(client);
  const first = await collect(
    await adapter.startTurn(request("project-session", "turn-1")),
  );
  const firstTerminal = first.at(-1);
  assert.equal(firstTerminal?.kind.type, "terminal");
  assert.ok(
    firstTerminal?.kind.type === "terminal" && firstTerminal.kind.continuation,
  );
  const continuation = firstTerminal.kind.continuation;

  const resumed = await collect(
    await adapter.startTurn(
      request("project-session", "turn-2", { continuation }),
    ),
  );
  assert.deepEqual(conformanceEventLabels(resumed), contract.expected_events);
  assert.deepEqual(client.resumeCalls.map((call) => call.id), ["thread-private"]);
  assert.ok(resumed.every((event) => event.sessionId === "project-session"));
  assert.doesNotMatch(JSON.stringify(continuation), /thread-private/);

  const foreign = new OpaqueContinuation("other.adapter", "foreign-secret");
  await assert.rejects(
    adapter.startTurn(
      request("project-session", "turn-3", { continuation: foreign }),
    ),
    (error: unknown) =>
      error instanceof RuntimeAdapterError &&
      error.code === "continuation_unavailable" &&
      error.recoverable,
  );

  const unavailable = new CodexRuntimeAdapter(
    new ScriptedClient([], [], new Error("missing provider thread")),
  );
  await assert.rejects(
    unavailable.startTurn(
      request("project-session", "turn-4", { continuation }),
    ),
    (error: unknown) =>
      error instanceof RuntimeAdapterError &&
      error.code === "continuation_unavailable" &&
      error.recoverable,
  );
});

test("the subscription canary exercises the normalized Codex adapter", async () => {
  const firstThread = new ScriptedThread(completedScript("canary-thread", "one"));
  const resumedThread = new ScriptedThread(
    completedScript("canary-thread", "two"),
  );
  const client = new ScriptedClient([firstThread], [resumedThread]);
  const adapter = new CodexRuntimeAdapter(client);
  const ticks = [0, 12, 12, 31];

  const report = await runRuntimeAdapterCanary(adapter, {
    workingDirectory: "C:/synthetic/canary",
    firstPrompt: "first",
    continuationPrompt: "second",
    clock: () => ticks.shift() ?? 31,
  });

  assert.equal(report.firstTurn.terminal, "completed");
  assert.equal(report.firstTurn.latencyMs, 12);
  assert.equal(report.continuation.latencyMs, 19);
  assert.equal(report.continuation.sameThread, true);
  assert.deepEqual(client.resumeCalls.map((call) => call.id), ["canary-thread"]);
});

test("TEST-AGENT-RUNTIME-STREAM-001 normalizes provider failure without leaking its message", async () => {
  const contract = conformanceCase("provider_failure");
  const thread = new ScriptedThread([
    providerEvent({ type: "thread.started", thread_id: "thread-failure" }),
    providerEvent({ type: "turn.started" }),
    providerEvent({
      type: "turn.failed",
      error: { message: "rate limit reached: private provider detail" },
    }),
  ]);
  const adapter = new CodexRuntimeAdapter(new ScriptedClient([thread]));
  const events = await collect(
    await adapter.startTurn(request("session-failure", "turn-failure")),
  );

  assert.deepEqual(conformanceEventLabels(events), contract.expected_events);
  const terminal = events.at(-1);
  assert.equal(terminal?.kind.type, "terminal");
  if (terminal?.kind.type === "terminal") {
    assert.deepEqual(terminal.kind.outcome, {
      type: "failed",
      code: contract.expected_terminal_code,
      retryable: contract.expected_retryable,
      recoverable: contract.expected_recoverable,
      partial: false,
    });
  }
  assert.doesNotMatch(JSON.stringify(events), /private provider detail/);
});

test("TEST-AGENT-RUNTIME-STREAM-001 rejects provider events after terminal", async () => {
  const contract = conformanceCase("malformed_late_event");
  const thread = new ScriptedThread([
    ...completedScript("thread-late", "complete"),
    providerEvent({
      type: "item.completed",
      item: { id: "late", type: "agent_message", text: "late" },
    }),
  ]);
  const adapter = new CodexRuntimeAdapter(new ScriptedClient([thread]));
  const turn = await adapter.startTurn(request("session-late", "turn-late"));

  const received: RuntimeEvent[] = [];
  await assert.rejects(
    async () => {
      for await (const event of turn.events) {
        received.push(event);
      }
    },
    (error: unknown) =>
      error instanceof RuntimeAdapterError &&
      error.code === contract.expected_error,
  );
  assert.equal(eventLabels(received).at(-1), "completed");
  assert.equal(received.filter((event) => event.kind.type === "terminal").length, 1);
});
