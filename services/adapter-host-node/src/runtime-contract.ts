export type RuntimeErrorCode =
  | "continuation_unavailable"
  | "invalid_request"
  | "protocol_violation"
  | "provider_failure"
  | "provider_unavailable"
  | "scope_mismatch"
  | "unsupported";

export class RuntimeAdapterError extends Error {
  constructor(
    readonly code: RuntimeErrorCode,
    readonly retryable = false,
    readonly recoverable = false,
  ) {
    super(code);
    this.name = "RuntimeAdapterError";
  }
}

export class OpaqueContinuation {
  readonly adapterId: string;
  readonly #handle: string;

  constructor(adapterId: string, handle: string) {
    if (adapterId.trim().length === 0 || handle.trim().length === 0) {
      throw new RuntimeAdapterError("invalid_request");
    }
    this.adapterId = adapterId;
    this.#handle = handle;
  }

  handleFor(adapterId: string): string {
    if (adapterId !== this.adapterId) {
      throw new RuntimeAdapterError(
        "continuation_unavailable",
        false,
        true,
      );
    }
    return this.#handle;
  }

  equals(other: OpaqueContinuation): boolean {
    return this.adapterId === other.adapterId && this.#handle === other.#handle;
  }

  toJSON(): { adapterId: string; handle: "[opaque]" } {
    return { adapterId: this.adapterId, handle: "[opaque]" };
  }
}

export interface RuntimeNativeExtension {
  namespace: string;
  schemaVersion: string;
  payload: Readonly<Record<string, unknown>>;
}

export interface RuntimeCapabilities {
  streaming: boolean;
  continuation: boolean;
  scopedCancellation: boolean;
  deadlines: boolean;
  nativeExtensionSchemas: string[];
}

export interface RuntimeDescriptor {
  adapterId: string;
  runtimeKind: "native_agent" | "generic_loop";
  capabilities: RuntimeCapabilities;
}

export interface RuntimeTurnRequest {
  sessionId: string;
  turnId: string;
  prompt: string;
  workspacePath: string;
  timeoutMs: number;
  continuation?: OpaqueContinuation;
}

export interface CancelRuntimeTurnRequest {
  sessionId: string;
  turnId: string;
}

export type RuntimeTerminalKind =
  | "completed"
  | "cancelled"
  | "timed_out"
  | "failed";

export type CancelDisposition =
  | { type: "requested" }
  | { type: "already_requested" }
  | { type: "already_terminal"; terminal: RuntimeTerminalKind }
  | { type: "not_found" };

export interface CancellationAcknowledgement {
  sessionId: string;
  turnId: string;
  disposition: CancelDisposition;
}

export interface RuntimeUsage {
  inputTokens: number;
  cachedInputTokens: number;
  outputTokens: number;
  reasoningOutputTokens: number;
}

export type RuntimeTerminalOutcome =
  | { type: "completed" }
  | { type: "cancelled"; partial: boolean }
  | { type: "timed_out"; partial: boolean }
  | {
      type: "failed";
      code: string;
      retryable: boolean;
      recoverable: boolean;
      partial: boolean;
    };

export type RuntimeEventKind =
  | { type: "started"; continuation?: OpaqueContinuation }
  | { type: "text_delta"; text: string }
  | { type: "progress"; phase: string; message?: string }
  | { type: "usage"; usage: RuntimeUsage }
  | { type: "warning"; code: string }
  | {
      type: "terminal";
      outcome: RuntimeTerminalOutcome;
      continuation?: OpaqueContinuation;
    };

export interface RuntimeEvent {
  sessionId: string;
  turnId: string;
  sequence: number;
  kind: RuntimeEventKind;
  nativeExtensions: RuntimeNativeExtension[];
}

export interface RuntimeTurn {
  events: AsyncGenerator<RuntimeEvent>;
}

export interface AgentRuntimeAdapter {
  describe(): Promise<RuntimeDescriptor>;
  startTurn(request: RuntimeTurnRequest): Promise<RuntimeTurn>;
  cancelTurn(
    request: CancelRuntimeTurnRequest,
  ): Promise<CancellationAcknowledgement>;
}

export function validateRuntimeTurnRequest(request: RuntimeTurnRequest): void {
  if (
    request.sessionId.trim().length === 0 ||
    request.turnId.trim().length === 0 ||
    request.prompt.trim().length === 0 ||
    request.workspacePath.trim().length === 0 ||
    !Number.isSafeInteger(request.timeoutMs) ||
    request.timeoutMs <= 0
  ) {
    throw new RuntimeAdapterError("invalid_request");
  }
}

export function validateCancelRequest(request: CancelRuntimeTurnRequest): void {
  if (
    request.sessionId.trim().length === 0 ||
    request.turnId.trim().length === 0
  ) {
    throw new RuntimeAdapterError("invalid_request");
  }
}
