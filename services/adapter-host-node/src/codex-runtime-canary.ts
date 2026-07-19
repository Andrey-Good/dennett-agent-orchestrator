import {
  CodexCanaryError,
  type CanaryTurnReport,
  type SubscriptionCanaryReport,
} from "./codex-canary-lib.js";
import type {
  AgentRuntimeAdapter,
  OpaqueContinuation,
  RuntimeEvent,
  RuntimeTurnRequest,
} from "./runtime-contract.js";

const DEFAULT_TURN_TIMEOUT_MS = 60_000;

interface CollectedRuntimeTurn {
  report: CanaryTurnReport;
  continuation: OpaqueContinuation;
}

function eventLabel(event: RuntimeEvent): string {
  return event.kind.type === "terminal"
    ? `terminal.${event.kind.outcome.type}`
    : event.kind.type;
}

async function collectRuntimeTurn(
  adapter: AgentRuntimeAdapter,
  request: RuntimeTurnRequest,
  clock: () => number,
): Promise<CollectedRuntimeTurn> {
  const startedAt = clock();
  const eventKinds = new Set<string>();
  let textDeltaCount = 0;
  let continuation = request.continuation;
  let terminal: RuntimeEvent | undefined;

  try {
    const turn = await adapter.startTurn(request);
    for await (const event of turn.events) {
      eventKinds.add(eventLabel(event));
      if (event.kind.type === "text_delta" && event.kind.text.length > 0) {
        textDeltaCount += 1;
      }
      if (event.kind.type === "started" && event.kind.continuation) {
        continuation = event.kind.continuation;
      }
      if (event.kind.type === "terminal") {
        terminal = event;
        if (event.kind.continuation) {
          if (continuation && !continuation.equals(event.kind.continuation)) {
            throw new CodexCanaryError("thread_id_mismatch");
          }
          continuation = event.kind.continuation;
        }
      }
    }
  } catch (error: unknown) {
    if (error instanceof CodexCanaryError) {
      throw error;
    }
    throw new CodexCanaryError("stream_failed");
  }

  if (!terminal || terminal.kind.type !== "terminal") {
    throw new CodexCanaryError("terminal_event_missing");
  }
  if (terminal.kind.outcome.type === "timed_out") {
    throw new CodexCanaryError("turn_timeout");
  }
  if (terminal.kind.outcome.type !== "completed") {
    throw new CodexCanaryError("stream_failed");
  }
  if (textDeltaCount === 0) {
    throw new CodexCanaryError("agent_message_missing");
  }
  if (!continuation) {
    throw new CodexCanaryError("thread_id_missing");
  }

  return {
    report: {
      terminal: "completed",
      eventKinds: [...eventKinds],
      agentMessageCount: textDeltaCount,
      latencyMs: Math.max(0, Math.round(clock() - startedAt)),
    },
    continuation,
  };
}

export async function runRuntimeAdapterCanary(
  adapter: AgentRuntimeAdapter,
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
  const sessionId = "codex-subscription-canary";
  const common = {
    sessionId,
    workspacePath: options.workingDirectory,
    timeoutMs,
  };
  const first = await collectRuntimeTurn(
    adapter,
    {
      ...common,
      turnId: "connectivity",
      prompt: options.firstPrompt,
    },
    clock,
  );
  const resumed = await collectRuntimeTurn(
    adapter,
    {
      ...common,
      turnId: "continuation",
      prompt: options.continuationPrompt,
      continuation: first.continuation,
    },
    clock,
  );

  if (!first.continuation.equals(resumed.continuation)) {
    throw new CodexCanaryError("thread_id_mismatch");
  }
  return {
    firstTurn: first.report,
    continuation: { ...resumed.report, sameThread: true },
  };
}
