import type {
  Input,
  ThreadEvent,
  ThreadOptions,
  TurnOptions,
} from "@openai/codex-sdk";

export type CanaryErrorCode =
  | "agent_message_missing"
  | "api_key_environment_present"
  | "chatgpt_login_required"
  | "cli_command_failed"
  | "codex_binary_missing"
  | "codex_package_invalid"
  | "stream_failed"
  | "stream_protocol_violation"
  | "terminal_event_missing"
  | "thread_id_mismatch"
  | "thread_id_missing"
  | "turn_timeout"
  | "unexpected_event_type"
  | "unexpected_item_type"
  | "workspace_cleanup_failed"
  | "workspace_setup_failed";

export class CodexCanaryError extends Error {
  constructor(
    readonly code: CanaryErrorCode,
    readonly safeDetail?: {
      itemClass?:
        | "provider_error"
        | "tool_or_external_effect"
        | "unknown"
        | "unsupported";
      cleanupFailed?: true;
      errorClass?:
        | "authentication"
        | "configuration"
        | "model_access"
        | "network"
        | "rate_limit"
        | "service"
        | "tooling"
        | "unknown";
    },
  ) {
    super(code);
    this.name = "CodexCanaryError";
  }
}

export interface CanaryThread {
  readonly id: string | null;
  runStreamed(
    input: Input,
    options?: TurnOptions,
  ): Promise<{ events: AsyncGenerator<ThreadEvent> }>;
}

export interface CanaryCodexClient {
  startThread(options?: ThreadOptions): CanaryThread;
  resumeThread(id: string, options?: ThreadOptions): CanaryThread;
}

export interface CanaryTurnReport {
  terminal: "completed";
  eventKinds: string[];
  agentMessageCount: number;
  latencyMs: number;
}

export interface SubscriptionCanaryReport {
  firstTurn: CanaryTurnReport;
  continuation: CanaryTurnReport & { sameThread: true };
}

interface CollectedTurn {
  report: CanaryTurnReport;
  observedThreadId: string;
}

const DEFAULT_TURN_TIMEOUT_MS = 60_000;
const MAX_ITERATOR_CLOSE_TIMEOUT_MS = 1_000;
const PERMITTED_ITEM_TYPES = new Set(["agent_message", "reasoning"]);
const EXTERNAL_EFFECT_ITEM_TYPES = new Set([
  "command_execution",
  "file_change",
  "mcp_tool_call",
  "web_search",
]);

async function closeIterator(
  iterator: AsyncIterator<ThreadEvent>,
  turnTimeoutMs: number,
): Promise<void> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const closeDeadline = new Promise<void>((resolve) => {
    timer = setTimeout(
      resolve,
      Math.min(MAX_ITERATOR_CLOSE_TIMEOUT_MS, Math.max(1, turnTimeoutMs)),
    );
  });
  const close = Promise.resolve()
    .then(() => iterator.return?.())
    .then(
      () => undefined,
      () => undefined,
    );
  await Promise.race([close, closeDeadline]);
  if (timer !== undefined) {
    clearTimeout(timer);
  }
}

function classifyErrorItem(message: string): NonNullable<
  CodexCanaryError["safeDetail"]
>["errorClass"] {
  if (/rate.?limit|usage.?limit|quota|too many requests|credits?|limit reached/i.test(message)) {
    return "rate_limit";
  }
  if (/auth|login|credential|unauthorized|forbidden|\b401\b|\b403\b/i.test(message)) {
    return "authentication";
  }
  if (/config|toml|unknown field|invalid (field|value)|deserialize/i.test(message)) {
    return "configuration";
  }
  if (/model.*(unavailable|unsupported|not found|access)|no access.*model/i.test(message)) {
    return "model_access";
  }
  if (/network|connect|dns|tls|socket|timed? ?out/i.test(message)) {
    return "network";
  }
  if (/tool|mcp|shell/i.test(message)) {
    return "tooling";
  }
  if (/server|service|request|response|http|status/i.test(message)) {
    return "service";
  }
  return "unknown";
}

function classifyUnexpectedItem(item: unknown): NonNullable<
  CodexCanaryError["safeDetail"]
> {
  const runtimeItem =
    typeof item === "object" && item !== null
      ? (item as { type?: unknown; message?: unknown })
      : {};
  if (runtimeItem.type === "error") {
    return {
      itemClass: "provider_error",
      errorClass: classifyErrorItem(
        typeof runtimeItem.message === "string" ? runtimeItem.message : "",
      ),
    };
  }
  if (
    typeof runtimeItem.type === "string" &&
    EXTERNAL_EFFECT_ITEM_TYPES.has(runtimeItem.type)
  ) {
    return { itemClass: "tool_or_external_effect" };
  }
  if (runtimeItem.type === "todo_list") {
    return { itemClass: "unsupported" };
  }
  return { itemClass: "unknown" };
}

function isRuntimeRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function isNonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

function isValidUsage(value: unknown): boolean {
  if (!isRuntimeRecord(value)) {
    return false;
  }
  return [
    "input_tokens",
    "cached_input_tokens",
    "output_tokens",
    "reasoning_output_tokens",
  ].every((field) => {
    const count = value[field];
    return Number.isSafeInteger(count) && (count as number) >= 0;
  });
}

export const CANARY_THREAD_OPTIONS: Readonly<ThreadOptions> = {
  sandboxMode: "read-only",
  approvalPolicy: "never",
  networkAccessEnabled: false,
  webSearchMode: "disabled",
};

async function collectTurn(
  thread: CanaryThread,
  prompt: string,
  clock: () => number,
  timeoutMs: number,
): Promise<CollectedTurn> {
  const startedAt = clock();
  const controller = new AbortController();
  let timer: ReturnType<typeof setTimeout> | undefined;
  const deadline = new Promise<never>((_resolve, reject) => {
    timer = setTimeout(() => {
      controller.abort();
      reject(new CodexCanaryError("turn_timeout"));
    }, timeoutMs);
  });

  const eventKinds = new Set<string>();
  let agentMessageCount = 0;
  let observedThreadId: string | null = null;
  let phase: "awaiting_thread" | "awaiting_turn" | "streaming" | "terminal" =
    "awaiting_thread";
  let exhausted = false;
  let iterator: AsyncIterator<ThreadEvent> | undefined;

  try {
    const { events } = await Promise.race([
      thread.runStreamed(prompt, { signal: controller.signal }),
      deadline,
    ]);
    iterator = events[Symbol.asyncIterator]();

    while (true) {
      const next = await Promise.race([iterator.next(), deadline]);
      if (next.done) {
        exhausted = true;
        break;
      }

      const event: unknown = next.value;
      if (!isRuntimeRecord(event) || typeof event.type !== "string") {
        throw new CodexCanaryError("stream_protocol_violation");
      }
      const eventType = event.type;
      if (phase === "terminal") {
        throw new CodexCanaryError("stream_protocol_violation");
      }
      eventKinds.add(eventType);

      switch (event.type) {
        case "thread.started":
          if (phase !== "awaiting_thread") {
            throw new CodexCanaryError("stream_protocol_violation");
          }
          if (!isNonEmptyString(event.thread_id)) {
            throw new CodexCanaryError("stream_protocol_violation");
          }
          observedThreadId = event.thread_id;
          phase = "awaiting_turn";
          break;
        case "turn.started":
          if (phase !== "awaiting_turn") {
            throw new CodexCanaryError("stream_protocol_violation");
          }
          phase = "streaming";
          break;
        case "item.started":
        case "item.updated":
        case "item.completed":
          if (phase !== "streaming") {
            throw new CodexCanaryError("stream_protocol_violation");
          }
          if (!isRuntimeRecord(event.item)) {
            throw new CodexCanaryError("stream_protocol_violation");
          }
          const itemType = event.item.type;
          if (typeof itemType !== "string" || !PERMITTED_ITEM_TYPES.has(itemType)) {
            throw new CodexCanaryError("unexpected_item_type", {
              ...classifyUnexpectedItem(event.item),
            });
          }
          if (
            !isNonEmptyString(event.item.id) ||
            typeof event.item.text !== "string"
          ) {
            throw new CodexCanaryError("stream_protocol_violation");
          }
          if (
            eventType === "item.completed" &&
            itemType === "agent_message"
          ) {
            if (!isNonEmptyString(event.item.text)) {
              throw new CodexCanaryError("stream_protocol_violation");
            }
            agentMessageCount += 1;
          }
          break;
        case "turn.completed":
          if (phase !== "streaming") {
            throw new CodexCanaryError("stream_protocol_violation");
          }
          if (!isValidUsage(event.usage)) {
            throw new CodexCanaryError("stream_protocol_violation");
          }
          phase = "terminal";
          break;
        case "turn.failed":
        case "error":
          throw new CodexCanaryError("stream_failed");
        default:
          throw new CodexCanaryError("unexpected_event_type");
      }
    }
  } catch (error: unknown) {
    controller.abort();
    if (error instanceof CodexCanaryError) {
      throw error;
    }
    if (controller.signal.aborted) {
      throw new CodexCanaryError("turn_timeout");
    }
    throw new CodexCanaryError("stream_failed");
  } finally {
    if (timer !== undefined) {
      clearTimeout(timer);
    }
    if (!exhausted && iterator?.return) {
      await closeIterator(iterator, timeoutMs);
    }
  }

  if (phase !== "terminal") {
    throw new CodexCanaryError("terminal_event_missing");
  }
  if (agentMessageCount === 0) {
    throw new CodexCanaryError("agent_message_missing");
  }
  if (!observedThreadId) {
    throw new CodexCanaryError("thread_id_missing");
  }

  return {
    report: {
      terminal: "completed",
      eventKinds: [...eventKinds],
      agentMessageCount,
      latencyMs: Math.max(0, Math.round(clock() - startedAt)),
    },
    observedThreadId,
  };
}

export async function runSubscriptionCanary(
  client: CanaryCodexClient,
  options: {
    workingDirectory: string;
    firstPrompt: string;
    continuationPrompt: string;
    clock?: () => number;
    timeoutMs?: number;
  },
): Promise<SubscriptionCanaryReport> {
  const clock = options.clock ?? performance.now.bind(performance);
  const timeoutMs = options.timeoutMs ?? DEFAULT_TURN_TIMEOUT_MS;
  const threadOptions: ThreadOptions = {
    ...CANARY_THREAD_OPTIONS,
    workingDirectory: options.workingDirectory,
  };
  const firstThread = client.startThread(threadOptions);
  const first = await collectTurn(
    firstThread,
    options.firstPrompt,
    clock,
    timeoutMs,
  );

  const resumedThread = client.resumeThread(
    first.observedThreadId,
    threadOptions,
  );
  const continuation = await collectTurn(
    resumedThread,
    options.continuationPrompt,
    clock,
    timeoutMs,
  );

  if (continuation.observedThreadId !== first.observedThreadId) {
    throw new CodexCanaryError("thread_id_mismatch");
  }

  return {
    firstTurn: first.report,
    continuation: {
      ...continuation.report,
      sameThread: true,
    },
  };
}
