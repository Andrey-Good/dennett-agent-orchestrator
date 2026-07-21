import assert from "node:assert/strict";
import type { ChildProcessWithoutNullStreams } from "node:child_process";
import { EventEmitter } from "node:events";
import { PassThrough } from "node:stream";
import test from "node:test";

import { CodexAppServerClient } from "./codex-app-server-client.js";

type JsonObject = Record<string, unknown>;

interface AppServerHarness {
  child: ChildProcessWithoutNullStreams;
  requests: JsonObject[];
  notify(method: string, params: JsonObject): void;
  writeRaw(value: string | Buffer): void;
  exited(): boolean;
}

function appServerHarness(options: {
  rejectInitialize?: boolean;
  ignoreMethods?: string[];
  rejectMethods?: string[];
} = {}): AppServerHarness {
  const processEvents = new EventEmitter();
  const stdin = new PassThrough();
  const stdout = new PassThrough();
  const stderr = new PassThrough();
  const requests: JsonObject[] = [];
  let input = "";
  let exitCode: number | null = null;
  const send = (message: JsonObject): void => {
    queueMicrotask(() => stdout.write(`${JSON.stringify(message)}\n`));
  };
  const handle = (message: JsonObject): void => {
    requests.push(message);
    if (typeof message.id !== "number" || typeof message.method !== "string") return;
    if (options.ignoreMethods?.includes(message.method)) return;
    if (options.rejectMethods?.includes(message.method)) {
      send({ id: message.id, error: { message: "provider control rejected" } });
      return;
    }
    const result = (() => {
      switch (message.method) {
        case "initialize": return {};
        case "thread/start": return { thread: { id: "thread-1" } };
        case "turn/start": return { turn: { id: "turn-1" } };
        case "turn/steer": return { turnId: "turn-1" };
        case "turn/interrupt": return {};
        default: throw new Error(`Unexpected App Server request: ${message.method}`);
      }
    })();
    send(options.rejectInitialize && message.method === "initialize"
      ? { id: message.id, error: { message: "bad request" } }
      : { id: message.id, result });
  };
  stdin.on("data", (chunk: Buffer) => {
    input += chunk.toString("utf8");
    while (input.includes("\n")) {
      const newline = input.indexOf("\n");
      const line = input.slice(0, newline);
      input = input.slice(newline + 1);
      if (line.length > 0) handle(JSON.parse(line) as JsonObject);
    }
  });
  stdin.once("finish", () => {
    exitCode = 0;
    processEvents.emit("exit", 0, null);
  });
  const child = processEvents as ChildProcessWithoutNullStreams;
  Object.assign(child, {
    stdin,
    stdout,
    stderr,
    kill: () => {
      exitCode = 0;
      processEvents.emit("exit", 0, null);
      return true;
    },
  });
  Object.defineProperty(child, "exitCode", { get: () => exitCode });
  return {
    child,
    requests,
    notify: (method, params) => send({ method, params }),
    writeRaw: (value) => stdout.write(value),
    exited: () => exitCode !== null,
  };
}

test("App Server keeps steering inside the active provider turn and closes its child", async () => {
  const harness = appServerHarness();
  const client = await CodexAppServerClient.start({
    executablePath: "C:\\runtime\\codex.exe",
    environment: { CODEX_HOME: "C:\\profile" },
    cliArguments: ["app-server", "--stdio"],
    spawnProcess: (executable, args, options) => {
      assert.equal(executable, "C:\\runtime\\codex.exe");
      assert.deepEqual(args, ["app-server", "--stdio"]);
      assert.equal(options.environment.CODEX_HOME, "C:\\profile");
      return harness.child;
    },
  });
  const configured = client.withDefaults({ reasoningEffort: "high", serviceTier: "fast" });
  const thread = configured.startThread({
    model: "gpt-test",
    workingDirectory: "C:\\workspace",
    approvalPolicy: "never",
    sandboxMode: "workspace-write",
    networkAccessEnabled: false,
    webSearchMode: "disabled",
  });
  const streamedPromise = thread.runStreamed("initial request", {
    clientMessageId: "message-initial",
  });
  const earlySteer = thread.steer("new constraint", "message-steer");
  const streamed = await streamedPromise;
  const events = streamed.events[Symbol.asyncIterator]();
  assert.equal((await events.next()).value?.type, "thread.started");
  assert.equal((await events.next()).value?.type, "turn.started");

  await earlySteer;
  const steer = harness.requests.find((request) => request.method === "turn/steer");
  assert.deepEqual(steer?.params, {
    threadId: "thread-1",
    expectedTurnId: "turn-1",
    clientUserMessageId: "message-steer",
    input: [{ type: "text", text: "new constraint" }],
  });

  harness.notify("item/started", {
    threadId: "thread-1",
    turnId: "turn-1",
    item: { id: "message-1", type: "agentMessage", text: "" },
  });
  harness.notify("item/agentMessage/delta", {
    threadId: "thread-1",
    turnId: "turn-1",
    itemId: "message-1",
    delta: "done",
  });
  harness.notify("item/completed", {
    threadId: "thread-1",
    turnId: "turn-1",
    item: { id: "message-1", type: "agentMessage", text: "done" },
  });
  const completedFileChange = {
    id: "file-change-1",
    type: "fileChange",
    changes: [{ path: "probe.txt", kind: "add" }],
    status: "completed",
  };
  harness.notify("item/started", {
    threadId: "thread-1",
    turnId: "turn-1",
    item: completedFileChange,
  });
  harness.notify("item/completed", {
    threadId: "thread-1",
    turnId: "turn-1",
    item: completedFileChange,
  });
  harness.notify("turn/completed", {
    threadId: "thread-1",
    turn: { id: "turn-1", status: "completed", error: null },
  });

  const remaining = [];
  for (;;) {
    const event = await events.next();
    if (event.done) break;
    remaining.push({
      type: event.value.type,
      itemType: "item" in event.value ? event.value.item.type : null,
    });
  }
  assert.deepEqual(remaining, [
    { type: "item.started", itemType: "agent_message" },
    { type: "item.updated", itemType: "agent_message" },
    { type: "item.completed", itemType: "agent_message" },
    { type: "item.completed", itemType: "file_change" },
    { type: "turn.completed", itemType: null },
  ]);
  const startThread = harness.requests.find((request) => request.method === "thread/start");
  assert.deepEqual(startThread?.params, {
    model: "gpt-test",
    serviceTier: "fast",
    cwd: "C:\\workspace",
    approvalPolicy: "never",
    sandbox: "workspace-write",
    config: {
      web_search: "disabled",
      sandbox_workspace_write: { network_access: false },
    },
    ephemeral: false,
  });
  const startTurn = harness.requests.find((request) => request.method === "turn/start");
  assert.deepEqual(startTurn?.params, {
    threadId: "thread-1",
    clientUserMessageId: "message-initial",
    input: [{ type: "text", text: "initial request" }],
    effort: "high",
    serviceTier: "fast",
  });

  await client.close();
  assert.equal(harness.exited(), true);
});

test("App Server startup failure closes the spawned child", async () => {
  const harness = appServerHarness({ rejectInitialize: true });
  await assert.rejects(
    CodexAppServerClient.start({
      executablePath: "C:\\runtime\\codex.exe",
      environment: { CODEX_HOME: "C:\\profile" },
      cliArguments: ["app-server", "--stdio"],
      spawnProcess: () => harness.child,
    }),
  );
  assert.equal(harness.exited(), true);
});

test("App Server bounds unread provider notifications instead of growing memory without limit", async () => {
  const harness = appServerHarness();
  const client = await CodexAppServerClient.start({
    executablePath: "C:\\runtime\\codex.exe",
    environment: { CODEX_HOME: "C:\\profile" },
    cliArguments: ["app-server", "--stdio"],
    maxPendingNotifications: 2,
    spawnProcess: () => harness.child,
  });
  const thread = client.startThread();
  const streamed = await thread.runStreamed("bounded queue");
  const events = streamed.events[Symbol.asyncIterator]();
  for (let index = 0; index < 3; index += 1) {
    harness.notify("item/started", {
      threadId: "thread-1",
      turnId: "turn-1",
      item: {
        id: `command-${index}`,
        type: "commandExecution",
        command: `command ${index}`,
        aggregatedOutput: "",
        status: "in_progress",
      },
    });
  }
  await new Promise<void>((resolve) => setImmediate(resolve));

  assert.equal((await events.next()).value?.type, "thread.started");
  assert.equal((await events.next()).value?.type, "turn.started");
  assert.equal((await events.next()).value?.type, "item.started");
  assert.equal((await events.next()).value?.type, "item.started");
  await assert.rejects(events.next(), { code: "provider_failure" });

  await client.close();
});

test("App Server fences the provider before an unacknowledged steer is reported failed", async () => {
  const harness = appServerHarness({ ignoreMethods: ["turn/steer"] });
  const client = await CodexAppServerClient.start({
    executablePath: "C:\\runtime\\codex.exe",
    environment: { CODEX_HOME: "C:\\profile" },
    cliArguments: ["app-server", "--stdio"],
    controlRpcTimeoutMs: 15,
    shutdownTimeoutMs: 15,
    spawnProcess: () => harness.child,
  });
  const thread = client.startThread();
  await thread.runStreamed("active turn");

  await assert.rejects(thread.steer("late steer", "message-late"), {
    code: "provider_unavailable",
  });
  assert.equal(harness.exited(), true);
});

test("App Server fences a rejected interrupt before acknowledging local Stop", async () => {
  const harness = appServerHarness({ rejectMethods: ["turn/interrupt"] });
  const client = await CodexAppServerClient.start({
    executablePath: "C:\\runtime\\codex.exe",
    environment: { CODEX_HOME: "C:\\profile" },
    cliArguments: ["app-server", "--stdio"],
    shutdownTimeoutMs: 15,
    spawnProcess: () => harness.child,
  });
  const thread = client.startThread();
  await thread.runStreamed("active turn");

  await thread.interrupt();
  assert.equal(harness.exited(), true);
});

test("App Server preserves owner-facing agent-message phases", async () => {
  const harness = appServerHarness();
  const client = await CodexAppServerClient.start({
    executablePath: "C:\\runtime\\codex.exe",
    environment: { CODEX_HOME: "C:\\profile" },
    cliArguments: ["app-server", "--stdio"],
    spawnProcess: () => harness.child,
  });
  const thread = client.startThread();
  const events = (await thread.runStreamed("phases")).events[Symbol.asyncIterator]();
  await events.next();
  await events.next();
  harness.notify("item/started", {
    threadId: "thread-1",
    turnId: "turn-1",
    item: { id: "commentary", type: "agentMessage", text: "Checking", phase: "commentary" },
  });
  harness.notify("item/completed", {
    threadId: "thread-1",
    turnId: "turn-1",
    item: { id: "answer", type: "agentMessage", text: "Done", phase: "final_answer" },
  });

  const commentary = await events.next();
  const answer = await events.next();
  assert.equal((commentary.value as { item?: { phase?: string } }).item?.phase, "commentary");
  assert.equal((answer.value as { item?: { phase?: string } }).item?.phase, "final_answer");
  await client.close();
});

test("App Server rejects an oversized unterminated frame and closes its child", async () => {
  const harness = appServerHarness();
  const client = await CodexAppServerClient.start({
    executablePath: "C:\\runtime\\codex.exe",
    environment: { CODEX_HOME: "C:\\profile" },
    cliArguments: ["app-server", "--stdio"],
    shutdownTimeoutMs: 15,
    spawnProcess: () => harness.child,
  });

  harness.writeRaw(Buffer.alloc(8 * 1024 * 1024 + 1, 0x61));
  await new Promise<void>((resolve) => setTimeout(resolve, 25));
  assert.equal(harness.exited(), true);
  await client.close();
});
