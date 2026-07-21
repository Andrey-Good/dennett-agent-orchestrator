import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { randomUUID } from "node:crypto";

import type {
  Input,
  ThreadEvent,
  ThreadItem,
  ThreadOptions,
  TurnOptions,
  Usage,
} from "@openai/codex-sdk";

import {
  RuntimeAdapterError,
} from "./runtime-contract.js";

const MAX_APP_SERVER_LINE_BYTES = 8 * 1024 * 1024;
const RPC_TIMEOUT_MS = 30_000;
const CONTROL_RPC_TIMEOUT_MS = 1_000;
const ACTIVE_TURN_TIMEOUT_MS = 250;
const SHUTDOWN_TIMEOUT_MS = 250;
const MAX_PENDING_NOTIFICATIONS = 4_096;
const MAX_ACTIVE_PROVIDER_ITEMS = 256;
const MAX_PROVIDER_ITEM_TEXT_BYTES = 768 * 1024;

type JsonObject = Record<string, unknown>;

interface PendingRequest {
  resolve(value: unknown): void;
  reject(error: Error): void;
  timer: ReturnType<typeof setTimeout>;
}

interface AppServerNotification {
  method: string;
  params: JsonObject;
}

interface NotificationSubscription {
  queue: AsyncQueue<AppServerNotification>;
  unsubscribe(): void;
}

export interface CodexAppServerLaunchOptions {
  executablePath: string;
  environment: Record<string, string>;
  cliArguments: string[];
  maxPendingNotifications?: number;
  controlRpcTimeoutMs?: number;
  activeTurnTimeoutMs?: number;
  shutdownTimeoutMs?: number;
  spawnProcess?: (
    executable: string,
    args: string[],
    options: { environment: Record<string, string> },
  ) => ChildProcessWithoutNullStreams;
}

interface AppServerThreadDefaults {
  reasoningEffort?: string;
  serviceTier?: string | null;
}

interface AppServerControlTimeouts {
  rpcMs: number;
  activeTurnMs: number;
}

interface NativeTurnOptions extends TurnOptions {
  clientMessageId?: string;
}

interface NativeCodexThread {
  readonly id: string | null;
  runStreamed(
    input: Input,
    options?: NativeTurnOptions,
  ): Promise<{ events: AsyncGenerator<ThreadEvent> }>;
  steer(input: Input, clientMessageId: string): Promise<void>;
  interrupt(): Promise<void>;
}

class AsyncQueue<T> {
  readonly #values: T[] = [];
  readonly #waiters: Array<{
    resolve(value: IteratorResult<T>): void;
    reject(error: Error): void;
  }> = [];
  #closed = false;
  #error: Error | undefined;

  constructor(private readonly maxValues: number) {}

  push(value: T): boolean {
    if (this.#closed) return false;
    const waiter = this.#waiters.shift();
    if (waiter) waiter.resolve({ done: false, value });
    else if (this.#values.length >= this.maxValues) {
      this.close(new RuntimeAdapterError("provider_failure", true, true));
      return false;
    } else this.#values.push(value);
    return true;
  }

  close(error?: Error): void {
    if (this.#closed) return;
    this.#closed = true;
    this.#error = error;
    for (const waiter of this.#waiters.splice(0)) {
      if (error) waiter.reject(error);
      else waiter.resolve({ done: true, value: undefined });
    }
  }

  next(): Promise<IteratorResult<T>> {
    const value = this.#values.shift();
    if (value !== undefined) return Promise.resolve({ done: false, value });
    if (this.#error) return Promise.reject(this.#error);
    if (this.#closed) return Promise.resolve({ done: true, value: undefined });
    return new Promise((resolve, reject) => {
      this.#waiters.push({ resolve, reject });
    });
  }
}

function record(value: unknown): JsonObject {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new RuntimeAdapterError("protocol_violation");
  }
  return value as JsonObject;
}

function requiredString(value: unknown): string {
  if (typeof value !== "string" || value.trim().length === 0) {
    throw new RuntimeAdapterError("protocol_violation");
  }
  return value;
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim().length > 0
    ? value
    : undefined;
}

function boundedProviderText(value: unknown): string {
  if (typeof value !== "string") {
    throw new RuntimeAdapterError("protocol_violation");
  }
  if (Buffer.byteLength(value, "utf8") > MAX_PROVIDER_ITEM_TEXT_BYTES) {
    throw new RuntimeAdapterError("provider_failure", false, true);
  }
  return value;
}

function providerError(value: unknown): RuntimeAdapterError {
  const error = typeof value === "object" && value !== null
    ? value as JsonObject
    : {};
  const message = typeof error.message === "string" ? error.message : "";
  if (/no active turn|does not match.*active turn|not steerable/i.test(message)) {
    return new RuntimeAdapterError("scope_mismatch", false, true);
  }
  if (/not found|unknown thread|failed to resume/i.test(message)) {
    return new RuntimeAdapterError("continuation_unavailable", false, true);
  }
  if (/invalid|bad request/i.test(message)) {
    return new RuntimeAdapterError("invalid_request");
  }
  return new RuntimeAdapterError("provider_failure", true, true);
}

function defaultSpawn(
  executable: string,
  args: string[],
  options: { environment: Record<string, string> },
): ChildProcessWithoutNullStreams {
  return spawn(executable, args, {
    env: options.environment,
    stdio: ["pipe", "pipe", "pipe"],
    windowsHide: true,
  });
}

class CodexAppServerConnection {
  readonly #pending = new Map<number, PendingRequest>();
  readonly #subscriptions = new Map<string, Set<AsyncQueue<AppServerNotification>>>();
  readonly #frameChunks: Buffer[] = [];
  #frameBytes = 0;
  #nextRequestId = 1;
  #closed = false;
  #failure: Promise<void> | undefined;
  #shutdown: Promise<void> | undefined;
  #writeTail = Promise.resolve();

  private constructor(
    private readonly child: ChildProcessWithoutNullStreams,
    private readonly maxPendingNotifications: number,
    private readonly shutdownTimeoutMs: number,
  ) {
    child.stdout.on("data", (chunk: Buffer | string) => this.onData(chunk));
    child.stdout.once("error", () => void this.fail(new RuntimeAdapterError("provider_unavailable", true, true)));
    child.once("error", () => void this.fail(new RuntimeAdapterError("provider_unavailable", true, true)));
    child.once("exit", () => void this.fail(new RuntimeAdapterError("provider_unavailable", true, true)));
    child.stderr.resume();
  }

  static async start(options: CodexAppServerLaunchOptions): Promise<CodexAppServerConnection> {
    const maxPendingNotifications = options.maxPendingNotifications ?? MAX_PENDING_NOTIFICATIONS;
    if (!Number.isSafeInteger(maxPendingNotifications) || maxPendingNotifications <= 0) {
      throw new RuntimeAdapterError("invalid_request");
    }
    const shutdownTimeoutMs = options.shutdownTimeoutMs ?? SHUTDOWN_TIMEOUT_MS;
    if (!Number.isSafeInteger(shutdownTimeoutMs) || shutdownTimeoutMs <= 0) {
      throw new RuntimeAdapterError("invalid_request");
    }
    const child = (options.spawnProcess ?? defaultSpawn)(
      options.executablePath,
      options.cliArguments,
      { environment: options.environment },
    );
    const connection = new CodexAppServerConnection(child, maxPendingNotifications, shutdownTimeoutMs);
    try {
      await connection.request("initialize", {
        clientInfo: {
          name: "dennett",
          title: "Dennett",
          version: "0.1.0",
        },
        capabilities: null,
      });
      await connection.notify("initialized", {});
      return connection;
    } catch (error: unknown) {
      await connection.close().catch(() => undefined);
      throw error;
    }
  }

  request(method: string, params: JsonObject, timeoutMs = RPC_TIMEOUT_MS): Promise<unknown> {
    if (this.#closed) {
      return Promise.reject(new RuntimeAdapterError("provider_unavailable", true, true));
    }
    const id = this.#nextRequestId++;
    const result = new Promise<unknown>((resolve, reject) => {
      const timer = setTimeout(() => {
        const error = new RuntimeAdapterError("provider_unavailable", true, true);
        void this.fail(error);
      }, timeoutMs);
      this.#pending.set(id, { resolve, reject, timer });
    });
    void this.write({ method, id, params }).catch((error: unknown) => {
      void this.fail(error instanceof Error ? error : new Error(String(error)));
    });
    return result;
  }

  notify(method: string, params: JsonObject): Promise<void> {
    return this.write({ method, params });
  }

  subscribe(threadId: string): NotificationSubscription {
    const queue = new AsyncQueue<AppServerNotification>(this.maxPendingNotifications);
    const queues = this.#subscriptions.get(threadId) ?? new Set();
    queues.add(queue);
    this.#subscriptions.set(threadId, queues);
    return {
      queue,
      unsubscribe: () => {
        const current = this.#subscriptions.get(threadId);
        current?.delete(queue);
        if (current?.size === 0) this.#subscriptions.delete(threadId);
        queue.close();
      },
    };
  }

  async close(): Promise<void> {
    await this.fail(new RuntimeAdapterError("provider_unavailable", true, true));
  }

  private write(message: JsonObject): Promise<void> {
    if (this.#closed) {
      return Promise.reject(new RuntimeAdapterError("provider_unavailable", true, true));
    }
    const encoded = `${JSON.stringify(message)}\n`;
    if (Buffer.byteLength(encoded, "utf8") > MAX_APP_SERVER_LINE_BYTES) {
      return Promise.reject(new RuntimeAdapterError("invalid_request"));
    }
    const write = this.#writeTail.then(() => new Promise<void>((resolve, reject) => {
      this.child.stdin.write(encoded, (error) => {
        if (error) reject(new RuntimeAdapterError("provider_unavailable", true, true));
        else resolve();
      });
    }));
    this.#writeTail = write.catch(() => undefined);
    return write;
  }

  private onData(rawChunk: Buffer | string): void {
    if (this.#closed) return;
    const chunk = Buffer.isBuffer(rawChunk) ? rawChunk : Buffer.from(rawChunk);
    let offset = 0;
    while (offset < chunk.length) {
      const newline = chunk.indexOf(0x0a, offset);
      const end = newline === -1 ? chunk.length : newline;
      const segment = chunk.subarray(offset, end);
      if (this.#frameBytes + segment.length > MAX_APP_SERVER_LINE_BYTES) {
        void this.fail(new RuntimeAdapterError("protocol_violation"));
        return;
      }
      if (segment.length > 0) {
        this.#frameChunks.push(Buffer.from(segment));
        this.#frameBytes += segment.length;
      }
      if (newline === -1) return;
      let frame = Buffer.concat(this.#frameChunks, this.#frameBytes);
      this.#frameChunks.length = 0;
      this.#frameBytes = 0;
      if (frame.at(-1) === 0x0d) frame = frame.subarray(0, -1);
      this.onLine(frame.toString("utf8"));
      if (this.#closed) return;
      offset = newline + 1;
    }
  }

  private onLine(line: string): void {
    let message: JsonObject;
    try {
      message = record(JSON.parse(line));
    } catch {
      void this.fail(new RuntimeAdapterError("protocol_violation"));
      return;
    }
    if (typeof message.id === "number") {
      const pending = this.#pending.get(message.id);
      if (!pending) return;
      clearTimeout(pending.timer);
      this.#pending.delete(message.id);
      if (message.error !== undefined) pending.reject(providerError(message.error));
      else pending.resolve(message.result);
      return;
    }
    if (typeof message.method !== "string") return;
    let params: JsonObject;
    try {
      params = record(message.params);
    } catch {
      return;
    }
    const threadId = optionalString(params.threadId);
    if (!threadId) return;
    for (const queue of this.#subscriptions.get(threadId) ?? []) {
      if (!queue.push({ method: message.method, params })) {
        void this.fail(new RuntimeAdapterError("provider_failure", true, true));
        return;
      }
    }
  }

  private fail(error: Error): Promise<void> {
    this.#closed = true;
    if (this.#failure) return this.#failure;
    for (const pending of this.#pending.values()) clearTimeout(pending.timer);
    this.#failure = Promise.resolve().then(async () => {
      // Requests are failed only after the dedicated provider process has been
      // fenced. This prevents a timed-out steer from being reported as failed
      // while it can still be applied later by the old App Server.
      await this.shutdownProcess();
      for (const [id, pending] of this.#pending) {
        pending.reject(error);
        this.#pending.delete(id);
      }
      for (const queues of this.#subscriptions.values()) {
        for (const queue of queues) queue.close(error);
      }
      this.#subscriptions.clear();
    });
    return this.#failure;
  }

  private shutdownProcess(): Promise<void> {
    this.#shutdown ??= (async () => {
      this.child.stdout.destroy();
      this.child.stdin.end();
      if (this.child.exitCode !== null) return;
      await Promise.race([
        new Promise<void>((resolve) => this.child.once("exit", () => resolve())),
        new Promise<void>((resolve) => setTimeout(resolve, this.shutdownTimeoutMs)),
      ]);
      if (this.child.exitCode === null) {
        this.child.kill();
        await Promise.race([
          new Promise<void>((resolve) => this.child.once("exit", () => resolve())),
          new Promise<void>((resolve) => setTimeout(resolve, this.shutdownTimeoutMs)),
        ]);
      }
    })();
    return this.#shutdown;
  }
}

function inputItems(input: Input): Array<JsonObject> {
  if (typeof input === "string") return [{ type: "text", text: input }];
  return input.map((item) => item.type === "text"
    ? { type: "text", text: item.text }
    : { type: "localImage", path: item.path });
}

function mapStatus(value: unknown): "in_progress" | "completed" | "failed" {
  if (value === "completed") return "completed";
  if (value === "failed" || value === "declined") return "failed";
  return "in_progress";
}

function normalizeItem(value: unknown): ThreadItem | undefined {
  const item = record(value);
  const id = requiredString(item.id);
  switch (item.type) {
    case "agentMessage":
      return {
        id,
        type: "agent_message",
        text: boundedProviderText(item.text ?? ""),
        phase: item.phase === "commentary" || item.phase === "final_answer"
          ? item.phase
          : null,
      } as ThreadItem;
    case "reasoning": {
      return { id, type: "reasoning", text: "" };
    }
    case "commandExecution":
      return {
        id,
        type: "command_execution",
        command: "",
        // Command output is not owner-facing M01 state. Keeping every output
        // delta would duplicate an unbounded provider transcript in memory.
        aggregated_output: "",
        ...(typeof item.exitCode === "number" ? { exit_code: item.exitCode } : {}),
        status: mapStatus(item.status),
      };
    case "fileChange":
      return {
        id,
        type: "file_change",
        changes: [],
        status: item.status === "failed" ? "failed" : "completed",
      };
    case "mcpToolCall":
      return {
        id,
        type: "mcp_tool_call",
        server: typeof item.server === "string" ? item.server : "unknown",
        tool: typeof item.tool === "string" ? item.tool : "unknown",
        arguments: {},
        status: mapStatus(item.status),
      };
    case "webSearch":
      return {
        id,
        type: "web_search",
        query: "",
      };
    case "plan":
      return {
        id,
        type: "todo_list",
        items: typeof item.text === "string"
          ? [{ text: boundedProviderText(item.text), completed: false }]
          : [],
      };
    default:
      return undefined;
  }
}

function updatedItem(item: ThreadItem, notification: AppServerNotification): ThreadItem {
  if (notification.method === "item/agentMessage/delta" && item.type === "agent_message") {
    return {
      ...item,
      text: boundedProviderText(item.text + boundedProviderText(notification.params.delta ?? "")),
    };
  }
  return item;
}

function usageFrom(value: unknown): Usage | undefined {
  const usage = typeof value === "object" && value !== null ? value as JsonObject : {};
  const fields = ["inputTokens", "cachedInputTokens", "outputTokens", "reasoningOutputTokens"] as const;
  if (!fields.every((field) => Number.isSafeInteger(usage[field]) && (usage[field] as number) >= 0)) {
    return undefined;
  }
  return {
    input_tokens: usage.inputTokens as number,
    cached_input_tokens: usage.cachedInputTokens as number,
    output_tokens: usage.outputTokens as number,
    reasoning_output_tokens: usage.reasoningOutputTokens as number,
  };
}

class CodexAppServerThread implements NativeCodexThread {
  #threadId: string | null;
  #threadLoaded = false;
  #activeTurnId: string | null = null;
  #activeTurnReady: Promise<string | null> | null = null;
  #resolveActiveTurn: ((turnId: string | null) => void) | null = null;
  #interruptPromise: Promise<void> | null = null;

  constructor(
    private readonly connection: CodexAppServerConnection,
    threadId: string | null,
    private readonly options: ThreadOptions,
    private readonly defaults: AppServerThreadDefaults,
    private readonly controlTimeouts: AppServerControlTimeouts,
  ) {
    this.#threadId = threadId;
  }

  get id(): string | null {
    return this.#threadId;
  }

  async runStreamed(
    input: Input,
    options: NativeTurnOptions = {},
  ): Promise<{ events: AsyncGenerator<ThreadEvent> }> {
    if (this.#activeTurnReady !== null) {
      throw new RuntimeAdapterError("invalid_request");
    }
    if (options.signal?.aborted) {
      throw new RuntimeAdapterError("provider_failure", false, true);
    }
    this.#activeTurnReady = new Promise((resolve) => {
      this.#resolveActiveTurn = resolve;
    });
    this.#interruptPromise = null;
    let subscription: NotificationSubscription | undefined;
    let threadId: string;
    let turnId: string;
    try {
      threadId = await this.ensureThread();
      if (options.signal?.aborted) {
        throw new RuntimeAdapterError("provider_failure", false, true);
      }
      subscription = this.connection.subscribe(threadId);
      const response = record(await this.connection.request("turn/start", {
        threadId,
        clientUserMessageId: options.clientMessageId ?? randomUUID(),
        input: inputItems(input),
        ...(this.defaults.reasoningEffort ? { effort: this.defaults.reasoningEffort } : {}),
        ...(this.defaults.serviceTier ? { serviceTier: this.defaults.serviceTier } : {}),
        ...(options.outputSchema === undefined ? {} : { outputSchema: options.outputSchema }),
      }));
      const turn = record(response.turn);
      turnId = requiredString(turn.id);
      this.#activeTurnId = turnId;
      this.#resolveActiveTurn?.(turnId);
      this.#resolveActiveTurn = null;
    } catch (error: unknown) {
      subscription?.unsubscribe();
      this.#resolveActiveTurn?.(null);
      this.#activeTurnReady = null;
      this.#resolveActiveTurn = null;
      throw error;
    }
    const interrupt = (): void => {
      void this.interrupt();
      subscription.unsubscribe();
    };
    options.signal?.addEventListener("abort", interrupt, { once: true });
    if (options.signal?.aborted) {
      interrupt();
      this.#activeTurnId = null;
      this.#activeTurnReady = null;
      throw new RuntimeAdapterError("provider_failure", false, true);
    }
    return {
      events: this.events(threadId, turnId, subscription, options.signal, interrupt),
    };
  }

  async steer(input: Input, clientMessageId: string): Promise<void> {
    const active = this.#activeTurnId ?? await this.waitForActiveTurn();
    const threadId = this.#threadId;
    if (!threadId || !active) {
      throw new RuntimeAdapterError("scope_mismatch", false, true);
    }
    const response = record(await this.connection.request("turn/steer", {
      threadId,
      expectedTurnId: active,
      clientUserMessageId: clientMessageId,
      input: inputItems(input),
    }, this.controlTimeouts.rpcMs));
    if (requiredString(response.turnId) !== active) {
      throw new RuntimeAdapterError("protocol_violation");
    }
  }

  interrupt(): Promise<void> {
    this.#interruptPromise ??= this.interruptActiveTurn();
    return this.#interruptPromise;
  }

  private async interruptActiveTurn(): Promise<void> {
    const active = this.#activeTurnId ?? await this.waitForActiveTurn();
    const threadId = this.#threadId;
    if (!threadId || !active) {
      await this.connection.close();
      return;
    }
    try {
      await this.connection.request(
        "turn/interrupt",
        { threadId, turnId: active },
        this.controlTimeouts.rpcMs,
      );
    } catch {
      // An unacknowledged interrupt is an uncertain external effect. Closing
      // the dedicated App Server process fences the provider before Dennett
      // is allowed to publish a terminal Stop/timeout state.
      await this.connection.close();
    }
  }

  private async ensureThread(): Promise<string> {
    if (this.#threadId && !this.#threadLoaded) {
      const response = record(await this.connection.request("thread/resume", {
        threadId: this.#threadId,
        ...this.threadConfiguration(),
      }));
      const thread = record(response.thread);
      const resumed = requiredString(thread.id);
      if (resumed !== this.#threadId) {
        throw new RuntimeAdapterError("continuation_unavailable", false, true);
      }
      this.#threadLoaded = true;
      return resumed;
    }
    if (this.#threadId) return this.#threadId;
    const response = record(await this.connection.request("thread/start", {
      ...this.threadConfiguration(),
      ephemeral: false,
    }));
    const thread = record(response.thread);
    this.#threadId = requiredString(thread.id);
    this.#threadLoaded = true;
    return this.#threadId;
  }

  private threadConfiguration(): JsonObject {
    const config: JsonObject = {
      web_search: this.options.webSearchMode ?? "disabled",
      ...(this.options.sandboxMode === "workspace-write"
        ? {
            sandbox_workspace_write: {
              network_access: this.options.networkAccessEnabled ?? false,
            },
          }
        : {}),
    };
    return {
      ...(this.options.model ? { model: this.options.model } : {}),
      ...(this.defaults.serviceTier ? { serviceTier: this.defaults.serviceTier } : {}),
      ...(this.options.workingDirectory ? { cwd: this.options.workingDirectory } : {}),
      approvalPolicy: this.options.approvalPolicy ?? "never",
      sandbox: this.options.sandboxMode ?? "read-only",
      config,
    };
  }

  private async waitForActiveTurn(): Promise<string | null> {
    const pending = this.#activeTurnReady;
    if (!pending) return null;
    return Promise.race([
      pending,
      new Promise<null>((resolve) => setTimeout(() => resolve(null), this.controlTimeouts.activeTurnMs)),
    ]);
  }

  private async *events(
    threadId: string,
    turnId: string,
    subscription: NotificationSubscription,
    signal: AbortSignal | undefined,
    interrupt: () => void,
  ): AsyncGenerator<ThreadEvent> {
    const items = new Map<string, ThreadItem>();
    let latestUsage: Usage | undefined;
    try {
      yield { type: "thread.started", thread_id: threadId };
      yield { type: "turn.started" };
      while (true) {
        const next = await subscription.queue.next();
        if (next.done) throw new RuntimeAdapterError("provider_unavailable", true, true);
        const notification = next.value;
        const notificationTurnId = optionalString(notification.params.turnId)
          ?? (typeof notification.params.turn === "object" && notification.params.turn !== null
            ? optionalString((notification.params.turn as JsonObject).id)
            : undefined);
        if (notificationTurnId && notificationTurnId !== turnId) continue;
        if (notification.method === "item/started" || notification.method === "item/completed") {
          const item = normalizeItem(notification.params.item);
          if (!item) continue;
          // App Server announces file changes twice with an already-terminal
          // status. The public Codex SDK contract exposes them only after the
          // patch succeeds or fails, so do not invent a running lifecycle.
          if (notification.method === "item/started" && item.type === "file_change") continue;
          if (notification.method === "item/started") {
            if (!items.has(item.id) && items.size >= MAX_ACTIVE_PROVIDER_ITEMS) {
              throw new RuntimeAdapterError("provider_failure", false, true);
            }
            items.set(item.id, item);
          }
          yield {
            type: notification.method === "item/started" ? "item.started" : "item.completed",
            item,
          };
          if (notification.method === "item/completed") items.delete(item.id);
          continue;
        }
        if (notification.method === "item/commandExecution/outputDelta") {
          // The completed command item carries the safe status needed by the
          // work log; raw stdout/stderr remains provider-private.
          continue;
        }
        if (notification.method === "item/agentMessage/delta") {
          const itemId = requiredString(notification.params.itemId);
          const existing = items.get(itemId);
          if (!existing) continue;
          const item = updatedItem(existing, notification);
          items.set(itemId, item);
          yield { type: "item.updated", item };
          continue;
        }
        if (notification.method === "thread/tokenUsage/updated") {
          const tokenUsage = record(notification.params.tokenUsage);
          latestUsage = usageFrom(tokenUsage.last) ?? latestUsage;
          continue;
        }
        if (notification.method === "turn/completed") {
          const completed = record(notification.params.turn);
          const status = completed.status;
          if (status === "completed") {
            yield latestUsage
              ? { type: "turn.completed", usage: latestUsage }
              : providerThreadEvent({ type: "turn.completed" });
            return;
          }
          const error = typeof completed.error === "object" && completed.error !== null
            ? completed.error as JsonObject
            : {};
          yield {
            type: "turn.failed",
            error: { message: typeof error.message === "string" ? error.message : String(status) },
          };
          return;
        }
      }
    } finally {
      signal?.removeEventListener("abort", interrupt);
      subscription.unsubscribe();
      this.#activeTurnId = null;
      this.#activeTurnReady = null;
      this.#resolveActiveTurn = null;
      this.#interruptPromise = null;
    }
  }
}

function providerThreadEvent(value: JsonObject): ThreadEvent {
  return value as unknown as ThreadEvent;
}

export class CodexAppServerClient {
  private constructor(
    private readonly connection: CodexAppServerConnection,
    private readonly defaults: AppServerThreadDefaults = {},
    private readonly controlTimeouts: AppServerControlTimeouts = {
      rpcMs: CONTROL_RPC_TIMEOUT_MS,
      activeTurnMs: ACTIVE_TURN_TIMEOUT_MS,
    },
  ) {}

  static async start(options: CodexAppServerLaunchOptions): Promise<CodexAppServerClient> {
    const rpcMs = options.controlRpcTimeoutMs ?? CONTROL_RPC_TIMEOUT_MS;
    const activeTurnMs = options.activeTurnTimeoutMs ?? ACTIVE_TURN_TIMEOUT_MS;
    if (
      !Number.isSafeInteger(rpcMs) || rpcMs <= 0
      || !Number.isSafeInteger(activeTurnMs) || activeTurnMs <= 0
    ) {
      throw new RuntimeAdapterError("invalid_request");
    }
    return new CodexAppServerClient(
      await CodexAppServerConnection.start(options),
      {},
      { rpcMs, activeTurnMs },
    );
  }

  withDefaults(defaults: AppServerThreadDefaults): CodexAppServerClient {
    return new CodexAppServerClient(this.connection, defaults, this.controlTimeouts);
  }

  startThread(options: ThreadOptions = {}): NativeCodexThread {
    return new CodexAppServerThread(
      this.connection,
      null,
      options,
      this.defaults,
      this.controlTimeouts,
    );
  }

  resumeThread(id: string, options: ThreadOptions = {}): NativeCodexThread {
    return new CodexAppServerThread(
      this.connection,
      id,
      options,
      this.defaults,
      this.controlTimeouts,
    );
  }

  close(): Promise<void> {
    return this.connection.close();
  }
}

export type { NativeCodexThread };
