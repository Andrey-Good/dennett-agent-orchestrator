import {
  type AgentRuntimeAdapter,
  type CancelRuntimeTurnRequest,
  OpaqueContinuation,
  RuntimeAdapterError,
  type RuntimeEvent,
  type RuntimeEventKind,
  type RuntimeTurnRequest,
} from "./runtime-contract.js";

const PROTOCOL_VERSION = 1;
const MAX_REQUEST_BYTES = 1024 * 1024;

type JsonObject = Record<string, unknown>;

export type RuntimeHostWriter = (message: JsonObject) => void | Promise<void>;

function record(value: unknown): JsonObject {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new RuntimeAdapterError("invalid_request");
  }
  return value as JsonObject;
}

function string(value: unknown): string {
  if (typeof value !== "string" || value.trim().length === 0) {
    throw new RuntimeAdapterError("invalid_request");
  }
  return value;
}

function parseContinuation(value: unknown): OpaqueContinuation | undefined {
  if (value === undefined || value === null) return undefined;
  const continuation = record(value);
  return new OpaqueContinuation(
    string(continuation.adapterId),
    string(continuation.handle),
  );
}

function parseStartRequest(value: unknown): RuntimeTurnRequest {
  const request = record(value);
  const contextHandles = request.contextHandles ?? [];
  if (
    !Array.isArray(contextHandles) ||
    contextHandles.some((handle) => typeof handle !== "string") ||
    !Number.isSafeInteger(request.timeoutMs) ||
    (request.timeoutMs as number) <= 0
  ) {
    throw new RuntimeAdapterError("invalid_request");
  }
  return {
    sessionId: string(request.sessionId),
    turnId: string(request.turnId),
    prompt: string(request.prompt),
    workspacePath: string(request.workspacePath),
    timeoutMs: request.timeoutMs as number,
    contextHandles: contextHandles as string[],
    continuation: parseContinuation(request.continuation),
  };
}

function parseCancelRequest(value: unknown): CancelRuntimeTurnRequest {
  const request = record(value);
  return {
    sessionId: string(request.sessionId),
    turnId: string(request.turnId),
  };
}

function wireContinuation(continuation: OpaqueContinuation): JsonObject {
  return {
    adapterId: continuation.adapterId,
    handle: continuation.handleFor(continuation.adapterId),
  };
}

function wireKind(kind: RuntimeEventKind): JsonObject {
  switch (kind.type) {
    case "started":
      return {
        type: kind.type,
        ...(kind.continuation
          ? { continuation: wireContinuation(kind.continuation) }
          : {}),
      };
    case "terminal":
      return {
        type: kind.type,
        outcome: kind.outcome,
        ...(kind.continuation
          ? { continuation: wireContinuation(kind.continuation) }
          : {}),
      };
    case "text_delta":
      return { type: kind.type, text: kind.text };
    case "progress":
      return {
        type: kind.type,
        ...(kind.activityId === undefined ? {} : { activityId: kind.activityId }),
        phase: kind.phase,
        ...(kind.message === undefined ? {} : { message: kind.message }),
        status: kind.status,
      };
    case "usage":
      return { type: kind.type, usage: kind.usage };
    case "warning":
      return { type: kind.type, code: kind.code };
  }
}

function wireEvent(event: RuntimeEvent): JsonObject {
  return {
    sessionId: event.sessionId,
    turnId: event.turnId,
    sequence: event.sequence,
    kind: wireKind(event.kind),
    nativeExtensions: event.nativeExtensions,
  };
}

function safeError(error: unknown): JsonObject {
  if (error instanceof RuntimeAdapterError) {
    return {
      code: error.code,
      retryable: error.retryable,
      recoverable: error.recoverable,
    };
  }
  return {
    code: "provider_failure",
    retryable: true,
    recoverable: true,
  };
}

function key(request: CancelRuntimeTurnRequest): string {
  return `${request.sessionId.length}:${request.sessionId}${request.turnId}`;
}

export class RuntimeHost {
  readonly #active = new Map<string, CancelRuntimeTurnRequest>();

  constructor(
    private readonly adapter: AgentRuntimeAdapter,
    private readonly write: RuntimeHostWriter,
  ) {}

  async handleLine(line: string): Promise<void> {
    let id: string | null = null;
    try {
      if (Buffer.byteLength(line, "utf8") > MAX_REQUEST_BYTES) {
        throw new RuntimeAdapterError("invalid_request");
      }
      const request = record(JSON.parse(line));
      if (request.v !== PROTOCOL_VERSION) {
        throw new RuntimeAdapterError("unsupported");
      }
      id = string(request.id);
      const method = string(request.method);
      switch (method) {
        case "health":
          await this.adapter.describe();
          await this.result(id, {
            status: "healthy",
            protocolVersion: PROTOCOL_VERSION,
          });
          return;
        case "describe":
          await this.result(id, await this.adapter.describe());
          return;
        case "start_turn": {
          const turnRequest = parseStartRequest(request.params);
          const turn = await this.adapter.startTurn(turnRequest);
          const cancellation = {
            sessionId: turnRequest.sessionId,
            turnId: turnRequest.turnId,
          };
          this.#active.set(key(cancellation), cancellation);
          await this.result(id, { started: true });
          queueMicrotask(() => {
            void this.forward(turn.events, cancellation);
          });
          return;
        }
        case "cancel_turn": {
          const cancellation = parseCancelRequest(request.params);
          await this.result(id, await this.adapter.cancelTurn(cancellation));
          return;
        }
        default:
          throw new RuntimeAdapterError("unsupported");
      }
    } catch (error: unknown) {
      await this.write({ v: PROTOCOL_VERSION, id, error: safeError(error) });
    }
  }

  async close(): Promise<void> {
    const active = [...this.#active.values()];
    await Promise.allSettled(active.map((request) => this.adapter.cancelTurn(request)));
  }

  private async result(id: string, result: unknown): Promise<void> {
    await this.write({ v: PROTOCOL_VERSION, id, result });
  }

  private async forward(
    events: AsyncGenerator<RuntimeEvent>,
    cancellation: CancelRuntimeTurnRequest,
  ): Promise<void> {
    try {
      for await (const event of events) {
        await this.write({
          v: PROTOCOL_VERSION,
          event: "runtime_event",
          payload: wireEvent(event),
        });
      }
    } catch (error: unknown) {
      await this.write({
        v: PROTOCOL_VERSION,
        event: "runtime_error",
        sessionId: cancellation.sessionId,
        turnId: cancellation.turnId,
        error: safeError(error),
      });
    } finally {
      this.#active.delete(key(cancellation));
    }
  }
}
