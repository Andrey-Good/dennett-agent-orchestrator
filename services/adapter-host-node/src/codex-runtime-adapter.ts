import type {
  Input,
  ThreadEvent,
  ThreadItem,
  ThreadOptions,
  TurnOptions,
} from "@openai/codex-sdk";

import {
  type AgentRuntimeAdapter,
  type CancelRuntimeTurnRequest,
  type CancellationAcknowledgement,
  OpaqueContinuation,
  RuntimeAdapterError,
  type RuntimeDescriptor,
  type RuntimeEvent,
  type RuntimeEventKind,
  type RuntimeNativeExtension,
  type RuntimeTerminalKind,
  type RuntimeTurn,
  type RuntimeTurnRequest,
  validateCancelRequest,
  validateRuntimeTurnRequest,
} from "./runtime-contract.js";

export const CODEX_RUNTIME_ADAPTER_ID = "openai.codex.sdk";
export const DEFAULT_CODEX_THREAD_OPTIONS: Readonly<
  Omit<ThreadOptions, "workingDirectory">
> = {
  approvalPolicy: "never",
  networkAccessEnabled: false,
  sandboxMode: "read-only",
  skipGitRepoCheck: true,
  webSearchMode: "disabled",
};
const CODEX_NATIVE_EXTENSION_SCHEMA = "openai.codex.item-status@0.144.5";
const MAX_TERMINAL_HISTORY = 256;
const ITERATOR_CLOSE_TIMEOUT_MS = 1_000;

export interface CodexThreadLike {
  readonly id: string | null;
  runStreamed(
    input: Input,
    options?: TurnOptions,
  ): Promise<{ events: AsyncGenerator<ThreadEvent> }>;
}

export interface CodexClientLike {
  startThread(options?: ThreadOptions): CodexThreadLike;
  resumeThread(id: string, options?: ThreadOptions): CodexThreadLike;
}

export interface CodexRuntimeAdapterOptions {
  threadOptions?: Readonly<Omit<ThreadOptions, "workingDirectory">>;
  terminalHistoryLimit?: number;
}

type StopReason = "cancelled" | "timed_out";

interface ActiveTurn {
  controller: AbortController;
  deadlineTimer?: ReturnType<typeof setTimeout>;
  stopReason?: StopReason;
}

interface ProviderItemLifecycle {
  itemType: string;
  terminal: boolean;
}

class ManagedRuntimeEventStream
  implements AsyncGenerator<RuntimeEvent, void, unknown>
{
  #closed = false;
  #started = false;

  constructor(
    private readonly source: AsyncGenerator<RuntimeEvent, void, unknown>,
    private readonly disposeUnstarted: () => void,
  ) {}

  async next(...args: [] | [unknown]): Promise<IteratorResult<RuntimeEvent, void>> {
    if (this.#closed) {
      return { done: true, value: undefined };
    }
    this.#started = true;
    const result = await this.source.next(...args);
    this.#closed = result.done ?? false;
    return result;
  }

  async return(
    value: void | PromiseLike<void>,
  ): Promise<IteratorResult<RuntimeEvent, void>> {
    if (this.#closed) {
      return { done: true, value: await value };
    }
    this.#closed = true;
    if (!this.#started) {
      this.disposeUnstarted();
      return { done: true, value: await value };
    }
    return this.source.return(value);
  }

  async throw(error: unknown): Promise<IteratorResult<RuntimeEvent, void>> {
    if (!this.#started) {
      this.#closed = true;
      this.disposeUnstarted();
      throw error;
    }
    this.#closed = true;
    return this.source.throw(error);
  }

  [Symbol.asyncIterator](): AsyncGenerator<RuntimeEvent, void, unknown> {
    return this;
  }
}

function turnKey(sessionId: string, turnId: string): string {
  return `${sessionId.length}:${sessionId}${turnId}`;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function isNonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

function isValidUsage(value: unknown): value is {
  input_tokens: number;
  cached_input_tokens: number;
  output_tokens: number;
  reasoning_output_tokens: number;
} {
  return (
    isRecord(value) &&
    [
      "input_tokens",
      "cached_input_tokens",
      "output_tokens",
      "reasoning_output_tokens",
    ].every((field) => {
      const count = value[field];
      return Number.isSafeInteger(count) && (count as number) >= 0;
    })
  );
}

function classifyProviderFailure(value: unknown): {
  code: string;
  retryable: boolean;
  recoverable: boolean;
} {
  const message =
    isRecord(value) && typeof value.message === "string" ? value.message : "";
  if (/rate.?limit|usage.?limit|quota|too many requests|credits?/i.test(message)) {
    return { code: "rate_limit", retryable: true, recoverable: true };
  }
  if (/auth|login|credential|unauthorized|forbidden|\b401\b|\b403\b/i.test(message)) {
    return { code: "authentication", retryable: false, recoverable: true };
  }
  if (/network|connect|dns|tls|socket|timed? ?out/i.test(message)) {
    return { code: "network", retryable: true, recoverable: true };
  }
  return { code: "provider_failure", retryable: true, recoverable: true };
}

function isMissingContinuationFailure(value: unknown): boolean {
  const message =
    isRecord(value) && typeof value.message === "string" ? value.message : "";
  return /(?:thread|conversation|session).*(?:missing|not found|unknown)|(?:resume|continuation).*(?:failed|invalid)/i.test(
    message,
  );
}

function safeItemExtension(item: Record<string, unknown>): RuntimeNativeExtension {
  const payload: Record<string, unknown> = {
    itemType: typeof item.type === "string" ? item.type : "unknown",
  };
  if (typeof item.id === "string") {
    payload.providerItemId = item.id;
  }
  if (typeof item.status === "string") {
    payload.status = item.status;
  }
  return {
    namespace: "openai.codex.item-status",
    schemaVersion: "0.144.5",
    payload,
  };
}

function boundedActivityMessage(value: string): string | undefined {
  const normalized = value.trim();
  if (normalized.length === 0) return undefined;
  return normalized.length <= 2_000
    ? normalized
    : `${normalized.slice(0, 1_999)}…`;
}

function activityMessage(item: Record<string, unknown>): string | undefined {
  switch (item.type) {
    case "reasoning":
      return typeof item.text === "string"
        ? boundedActivityMessage(item.text)
        : undefined;
    case "command_execution":
    case "mcp_tool_call":
    case "web_search":
    case "file_change":
      return undefined;
    case "todo_list": {
      if (!Array.isArray(item.items)) return undefined;
      const current = item.items.flatMap((todo) =>
        isRecord(todo) && typeof todo.text === "string" && todo.completed !== true
          ? [todo.text]
          : []
      );
      return boundedActivityMessage(current.join(" · "));
    }
    default:
      return undefined;
  }
}

function activityPhase(itemType: unknown): string {
  switch (itemType) {
    case "reasoning": return "reasoning_summary";
    case "command_execution": return "command";
    case "mcp_tool_call": return "tool";
    case "web_search": return "web_search";
    case "file_change": return "workspace";
    case "todo_list": return "plan";
    default: throw new RuntimeAdapterError("protocol_violation");
  }
}

function activityStatus(
  eventType: "item.started" | "item.updated" | "item.completed",
  item: Record<string, unknown>,
): "started" | "updated" | "completed" | "failed" {
  if (item.status === "failed") return "failed";
  if (eventType === "item.started") return "started";
  if (eventType === "item.updated") return "updated";
  return "completed";
}

async function raceWithAbort<T>(operation: Promise<T>, signal: AbortSignal): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const rejectAbort = () =>
      reject(new RuntimeAdapterError("provider_failure"));
    if (signal.aborted) {
      rejectAbort();
      return;
    }
    signal.addEventListener("abort", rejectAbort, { once: true });
    operation.then(
      (value) => {
        signal.removeEventListener("abort", rejectAbort);
        if (signal.aborted) {
          rejectAbort();
        } else {
          resolve(value);
        }
      },
      (error: unknown) => {
        signal.removeEventListener("abort", rejectAbort);
        reject(error);
      },
    );
  });
}

async function closeIterator(iterator: AsyncIterator<ThreadEvent>): Promise<void> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const deadline = new Promise<void>((resolve) => {
    timer = setTimeout(resolve, ITERATOR_CLOSE_TIMEOUT_MS);
  });
  const close = Promise.resolve()
    .then(() => iterator.return?.())
    .then(
      () => undefined,
      () => undefined,
    );
  await Promise.race([close, deadline]);
  if (timer !== undefined) {
    clearTimeout(timer);
  }
}

export class CodexRuntimeAdapter implements AgentRuntimeAdapter {
  readonly #activeTurns = new Map<string, ActiveTurn>();
  readonly #terminalTurns = new Map<string, RuntimeTerminalKind>();
  readonly #terminalHistoryLimit: number;

  constructor(
    private readonly client: CodexClientLike,
    private readonly options: CodexRuntimeAdapterOptions = {},
  ) {
    const limit = options.terminalHistoryLimit ?? MAX_TERMINAL_HISTORY;
    if (!Number.isSafeInteger(limit) || limit <= 0) {
      throw new RuntimeAdapterError("invalid_request");
    }
    this.#terminalHistoryLimit = limit;
  }

  async describe(): Promise<RuntimeDescriptor> {
    return {
      adapterId: CODEX_RUNTIME_ADAPTER_ID,
      runtimeKind: "native_agent",
      capabilities: {
        streaming: true,
        continuation: true,
        scopedCancellation: true,
        deadlines: true,
        nativeExtensionSchemas: [CODEX_NATIVE_EXTENSION_SCHEMA],
      },
    };
  }

  async startTurn(request: RuntimeTurnRequest): Promise<RuntimeTurn> {
    validateRuntimeTurnRequest(request);
    const key = turnKey(request.sessionId, request.turnId);
    if (this.#activeTurns.has(key) || this.#terminalTurns.has(key)) {
      throw new RuntimeAdapterError("invalid_request");
    }

    const threadOptions: ThreadOptions = {
      ...DEFAULT_CODEX_THREAD_OPTIONS,
      ...this.options.threadOptions,
      workingDirectory: request.workspacePath,
    };
    let thread: CodexThreadLike;
    try {
      thread = request.continuation
        ? this.client.resumeThread(
            request.continuation.handleFor(CODEX_RUNTIME_ADAPTER_ID),
            threadOptions,
          )
        : this.client.startThread(threadOptions);
    } catch {
      throw new RuntimeAdapterError(
        request.continuation
          ? "continuation_unavailable"
          : "provider_unavailable",
        false,
        true,
      );
    }

    const active: ActiveTurn = { controller: new AbortController() };
    this.#activeTurns.set(key, active);
    active.deadlineTimer = setTimeout(() => {
      if (this.#activeTurns.get(key) !== active) {
        return;
      }
      active.stopReason ??= "timed_out";
      active.controller.abort();
      this.rememberTerminal(key, active.stopReason, active);
    }, request.timeoutMs);
    const events = new ManagedRuntimeEventStream(
      this.streamTurn(key, request, thread, active),
      () => this.disposeUnstarted(key, active),
    );
    return { events };
  }

  async cancelTurn(
    request: CancelRuntimeTurnRequest,
  ): Promise<CancellationAcknowledgement> {
    validateCancelRequest(request);
    const key = turnKey(request.sessionId, request.turnId);
    const active = this.#activeTurns.get(key);
    let disposition: CancellationAcknowledgement["disposition"];
    if (active?.stopReason === "cancelled") {
      disposition = { type: "already_requested" };
    } else if (active?.stopReason === "timed_out") {
      disposition = { type: "already_terminal", terminal: "timed_out" };
    } else if (active) {
      active.stopReason = "cancelled";
      active.controller.abort();
      disposition = { type: "requested" };
    } else {
      const terminal = this.#terminalTurns.get(key);
      disposition = terminal
        ? { type: "already_terminal", terminal }
        : { type: "not_found" };
    }
    return {
      sessionId: request.sessionId,
      turnId: request.turnId,
      disposition,
    };
  }

  private rememberTerminal(
    key: string,
    terminal: RuntimeTerminalKind,
    owner: ActiveTurn,
  ): void {
    if (this.#activeTurns.get(key) !== owner) {
      return;
    }
    if (owner.deadlineTimer !== undefined) {
      clearTimeout(owner.deadlineTimer);
      owner.deadlineTimer = undefined;
    }
    this.#activeTurns.delete(key);
    this.#terminalTurns.set(key, terminal);
    while (this.#terminalTurns.size > this.#terminalHistoryLimit) {
      const oldest = this.#terminalTurns.keys().next().value as string | undefined;
      if (oldest === undefined) {
        break;
      }
      this.#terminalTurns.delete(oldest);
    }
  }

  private disposeUnstarted(key: string, active: ActiveTurn): void {
    if (this.#activeTurns.get(key) !== active) {
      return;
    }
    active.controller.abort();
    if (active.stopReason) {
      this.rememberTerminal(key, active.stopReason, active);
      return;
    }
    if (active.deadlineTimer !== undefined) {
      clearTimeout(active.deadlineTimer);
      active.deadlineTimer = undefined;
    }
    this.#activeTurns.delete(key);
  }

  private async *streamTurn(
    key: string,
    request: RuntimeTurnRequest,
    thread: CodexThreadLike,
    active: ActiveTurn,
  ): AsyncGenerator<RuntimeEvent> {
    let sequence = 0;
    const lifecycle: {
      phase: "awaiting_thread" | "awaiting_turn" | "streaming" | "terminal";
    } = { phase: "awaiting_thread" };
    let continuation = request.continuation;
    let emittedText = false;
    let terminalKind: RuntimeTerminalKind | undefined;
    let iterator: AsyncIterator<ThreadEvent> | undefined;
    let exhausted = false;
    const itemText = new Map<string, string>();
    const itemLifecycles = new Map<string, ProviderItemLifecycle>();

    const event = (
      kind: RuntimeEventKind,
      nativeExtensions: RuntimeNativeExtension[] = [],
    ): RuntimeEvent => ({
      sessionId: request.sessionId,
      turnId: request.turnId,
      sequence: ++sequence,
      kind,
      nativeExtensions,
    });
    const started = (): RuntimeEvent =>
      event({ type: "started", ...(continuation ? { continuation } : {}) });
    const claimTerminal = (kind: RuntimeTerminalKind): void => {
      terminalKind = kind;
      lifecycle.phase = "terminal";
      this.rememberTerminal(key, kind, active);
    };
    const stopped = (reason: StopReason): RuntimeEvent => {
      claimTerminal(reason);
      return event({
        type: "terminal",
        outcome: { type: reason, partial: emittedText },
        ...(continuation ? { continuation } : {}),
      });
    };
    const failed = (
      code: string,
      retryable: boolean,
      recoverable: boolean,
    ): RuntimeEvent => {
      claimTerminal("failed");
      return event({
        type: "terminal",
        outcome: {
          type: "failed",
          code,
          retryable,
          recoverable,
          partial: emittedText,
        },
        ...(continuation ? { continuation } : {}),
      });
    };

    try {
      if (active.stopReason) {
        const startEvent = started();
        const terminalEvent = stopped(active.stopReason);
        yield startEvent;
        yield terminalEvent;
        return;
      }

      const streamed = await raceWithAbort(
        thread.runStreamed(request.prompt, { signal: active.controller.signal }),
        active.controller.signal,
      );
      iterator = streamed.events[Symbol.asyncIterator]();

      while (true) {
        const next = await raceWithAbort(iterator.next(), active.controller.signal);
        if (next.done) {
          exhausted = true;
          break;
        }
        const raw: unknown = next.value;
        if (
          !isRecord(raw) ||
          typeof raw.type !== "string" ||
          lifecycle.phase === "terminal"
        ) {
          throw new RuntimeAdapterError("protocol_violation");
        }

        switch (raw.type) {
          case "thread.started": {
            if (
              lifecycle.phase !== "awaiting_thread" ||
              !isNonEmptyString(raw.thread_id)
            ) {
              throw new RuntimeAdapterError("protocol_violation");
            }
            const observed = new OpaqueContinuation(
              CODEX_RUNTIME_ADAPTER_ID,
              raw.thread_id,
            );
            if (continuation && !continuation.equals(observed)) {
              throw new RuntimeAdapterError(
                "continuation_unavailable",
                false,
                true,
              );
            }
            continuation = observed;
            lifecycle.phase = "awaiting_turn";
            yield started();
            break;
          }
          case "turn.started":
            if (lifecycle.phase !== "awaiting_turn") {
              throw new RuntimeAdapterError("protocol_violation");
            }
            lifecycle.phase = "streaming";
            break;
          case "item.started":
          case "item.updated":
          case "item.completed": {
            if (lifecycle.phase !== "streaming" || !isRecord(raw.item)) {
              throw new RuntimeAdapterError("protocol_violation");
            }
            const item = raw.item as unknown as ThreadItem;
            if (!isNonEmptyString((item as { id?: unknown }).id)) {
              throw new RuntimeAdapterError("protocol_violation");
            }
            const runtimeItem = item as unknown as Record<string, unknown>;
            const itemType = runtimeItem.type;
            if (!isNonEmptyString(itemType)) {
              throw new RuntimeAdapterError("protocol_violation");
            }
            const itemId = runtimeItem.id as string;
            const priorLifecycle = itemLifecycles.get(itemId);
            if (raw.type === "item.started") {
              if (priorLifecycle || runtimeItem.status === "completed" || runtimeItem.status === "failed") {
                throw new RuntimeAdapterError("protocol_violation");
              }
              itemLifecycles.set(itemId, { itemType, terminal: false });
            } else if (raw.type === "item.updated") {
              if (
                !priorLifecycle
                || priorLifecycle.terminal
                || priorLifecycle.itemType !== itemType
                || runtimeItem.status === "completed"
                || runtimeItem.status === "failed"
              ) {
                throw new RuntimeAdapterError("protocol_violation");
              }
            } else {
              if (
                priorLifecycle?.terminal
                || (priorLifecycle && priorLifecycle.itemType !== itemType)
                || runtimeItem.status === "in_progress"
                || runtimeItem.status === "running"
              ) {
                throw new RuntimeAdapterError("protocol_violation");
              }
              itemLifecycles.set(itemId, { itemType, terminal: true });
            }
            if (itemType === "agent_message") {
              if (typeof runtimeItem.text !== "string") {
                throw new RuntimeAdapterError("protocol_violation");
              }
              const previous = itemText.get(itemId) ?? "";
              if (!runtimeItem.text.startsWith(previous)) {
                throw new RuntimeAdapterError("protocol_violation");
              }
              const delta = runtimeItem.text.slice(previous.length);
              itemText.set(itemId, runtimeItem.text);
              if (delta.length > 0) {
                emittedText = true;
                yield event({ type: "text_delta", text: delta });
              }
              break;
            }
            if (itemType === "error") {
              yield event({ type: "warning", code: "provider_item_error" });
              break;
            }
            if (
              itemType === "reasoning" ||
              itemType === "command_execution" ||
              itemType === "mcp_tool_call" ||
              itemType === "web_search" ||
              itemType === "file_change" ||
              itemType === "todo_list"
            ) {
              if (itemType === "reasoning" && typeof runtimeItem.text !== "string") {
                throw new RuntimeAdapterError("protocol_violation");
              }
              const message = activityMessage(runtimeItem);
              yield event(
                {
                  type: "progress",
                  activityId: runtimeItem.id as string,
                  phase: activityPhase(itemType),
                  ...(message === undefined ? {} : { message }),
                  status: activityStatus(raw.type, runtimeItem),
                },
                [safeItemExtension(runtimeItem)],
              );
              break;
            }
            throw new RuntimeAdapterError("protocol_violation");
          }
          case "turn.completed":
            if (lifecycle.phase !== "streaming" || !isValidUsage(raw.usage)) {
              throw new RuntimeAdapterError("protocol_violation");
            }
            const usage = event({
              type: "usage",
              usage: {
                inputTokens: raw.usage.input_tokens,
                cachedInputTokens: raw.usage.cached_input_tokens,
                outputTokens: raw.usage.output_tokens,
                reasoningOutputTokens: raw.usage.reasoning_output_tokens,
              },
            });
            claimTerminal("completed");
            const completion = event({
              type: "terminal",
              outcome: { type: "completed" },
              ...(continuation ? { continuation } : {}),
            });
            yield usage;
            yield completion;
            break;
          case "turn.failed": {
            if (lifecycle.phase !== "streaming") {
              throw new RuntimeAdapterError("protocol_violation");
            }
            const classified = classifyProviderFailure(raw.error);
            yield failed(
              classified.code,
              classified.retryable,
              classified.recoverable,
            );
            break;
          }
          case "error": {
            const classified = classifyProviderFailure(raw);
            const startEvent =
              lifecycle.phase === "awaiting_thread" ? started() : undefined;
            const terminalEvent = failed(
              request.continuation && isMissingContinuationFailure(raw)
                ? "continuation_unavailable"
                : classified.code,
              classified.retryable,
              true,
            );
            if (startEvent) {
              yield startEvent;
            }
            yield terminalEvent;
            break;
          }
          default:
            throw new RuntimeAdapterError("protocol_violation");
        }
      }

      if (lifecycle.phase !== "terminal") {
        throw new RuntimeAdapterError("protocol_violation");
      }
    } catch (error: unknown) {
      if (error instanceof RuntimeAdapterError && error.code === "protocol_violation") {
        throw error;
      }
      if (lifecycle.phase !== "terminal") {
        const startEvent =
          lifecycle.phase === "awaiting_thread" ? started() : undefined;
        let terminalEvent: RuntimeEvent;
        if (active.stopReason) {
          terminalEvent = stopped(active.stopReason);
        } else if (error instanceof RuntimeAdapterError) {
          terminalEvent = failed(
            error.code,
            error.retryable,
            error.recoverable,
          );
        } else {
          const classified = classifyProviderFailure(error);
          terminalEvent = failed(
            request.continuation && isMissingContinuationFailure(error)
              ? "continuation_unavailable"
              : classified.code,
            classified.retryable,
            true,
          );
        }
        if (startEvent) {
          yield startEvent;
        }
        yield terminalEvent;
      }
    } finally {
      if (!exhausted && iterator !== undefined) {
        await closeIterator(iterator);
      }
      if (terminalKind === undefined) {
        active.controller.abort();
        terminalKind = active.stopReason ?? "failed";
      }
      this.rememberTerminal(key, terminalKind, active);
    }
  }
}
