import { Channel, invoke } from "@tauri-apps/api/core";
import {
  parseSystemSnapshot,
  type Revision,
  type RuntimeControlSelection,
  type SystemSnapshot,
  type UiSafeError,
  type WatchCursor,
} from "./systemBridge";

export interface SessionSummary {
  sessionId: string;
  projectId: string | null;
  title: string;
  state: string;
  revision: Revision;
  activeTurnId: string | null;
  lastActivityAtUnixMs: number | null;
}

export interface TurnOutcome {
  kind: "result" | "error";
  summary?: string;
  partial?: boolean;
  error?: UiSafeError;
}

export interface ProjectTurn {
  turnId: string;
  commandId: string;
  role: string;
  state: string;
  text: string;
  activities: TurnActivity[];
  outcome: TurnOutcome | null;
  createdRevision?: Revision | null;
  createdAtUnixMs: number | null;
  completedAtUnixMs: number | null;
}

export interface TurnActivity {
  activityId: string;
  phase: string;
  message: string | null;
  status: string;
  createdRevision?: Revision | null;
  createdAtUnixMs: number | null;
  updatedAtUnixMs: number | null;
  nativeExtensions: NativeExtension[];
}

export interface NativeExtension {
  namespace: string;
  schemaVersion: string;
  jsonValue: string;
}

export interface ProjectChatState {
  session: SessionSummary;
  fingerprint: string;
  turns: ProjectTurn[];
}

export interface ComposerDraft {
  commandId: string;
  text: string;
  revision: Revision;
  updatedAtUnixMs: number | null;
}

export type ComposerDraftWriteState =
  | "composer_draft_write_state_saved"
  | "composer_draft_write_state_already_accepted";

export type SessionMutation =
  | { kind: "upsertTurn"; turn: ProjectTurn }
  | { kind: "appendTurnText"; turnId: string; text: string }
  | { kind: "upsertTurnActivity"; turnId: string; activity: TurnActivity }
  | { kind: "finishTurn"; turnId: string; state: string; outcome: TurnOutcome | null; completedAtUnixMs: number | null }
  | { kind: "updateSession"; title: string | null; state: string | null; activeTurnId: string | null };

export type ProjectChatEvent =
  | { kind: "snapshot"; subscriptionId: string; cursor: WatchCursor; snapshot: ProjectChatState }
  | { kind: "delta"; subscriptionId: string; cursor: WatchCursor; baseRevision: Revision; newRevision: Revision; committedAtUnixMs: number | null; mutations: SessionMutation[] }
  | { kind: "heartbeat"; subscriptionId: string; cursor: WatchCursor; currentRevision: Revision }
  | { kind: "resyncRequired"; subscriptionId: string; cursor: WatchCursor; reason: string; currentRevision: Revision }
  | { kind: "error"; subscriptionId: string; error: UiSafeError };

export interface OpenedProjectChat {
  correlationId: string;
  subscriptionId: string;
  system: SystemSnapshot;
  session: ProjectChatState;
  draft: ComposerDraft | null;
}

export interface ProjectChatHandle {
  opened: OpenedProjectChat;
  close(): Promise<boolean>;
}

export interface ProjectChatBridgeDependencies {
  invoke(command: string, args: Record<string, unknown>): Promise<unknown>;
  createChannel(onMessage: (event: unknown) => void): unknown;
  identity(): string;
}

const defaults: ProjectChatBridgeDependencies = {
  invoke: (command, args) => invoke(command, args),
  createChannel: (onMessage) => new Channel(onMessage),
  identity: () => crypto.randomUUID(),
};

export class TauriProjectChatClient {
  constructor(private readonly dependencies: ProjectChatBridgeDependencies = defaults) {}

  async open(
    onEvent: (event: ProjectChatEvent) => void,
    sessionId: string | null = null,
    correlationId = this.dependencies.identity(),
  ): Promise<ProjectChatHandle> {
    const channel = this.dependencies.createChannel((raw) => {
      try {
        onEvent(parseProjectChatEvent(raw));
      } catch {
        onEvent({
          kind: "error",
          subscriptionId: "unbound",
          error: bridgeError("desktop_bridge_event_invalid", correlationId),
        });
      }
    });
    const raw = await this.dependencies.invoke("open_project_chat", {
      request: { correlationId, sessionId },
      onEvent: channel,
    });
    const opened = parseOpenedProjectChat(raw);
    let closed = false;
    return {
      opened,
      close: async () => {
        void channel;
        if (closed) return false;
        closed = true;
        const result = await this.dependencies.invoke("close_project_chat", {
          request: { subscriptionId: opened.subscriptionId },
        });
        if (typeof result !== "boolean") throw new Error("Invalid close_project_chat response");
        return result;
      },
    };
  }

  async createChat(projectId: string | null, title = "Untitled chat"): Promise<{ sessionId: string }> {
    const result = record(await this.dependencies.invoke("create_chat", {
      request: {
        correlationId: this.dependencies.identity(),
        commandId: this.dependencies.identity(),
        projectId,
        title,
      },
    }), "create_chat response");
    return { sessionId: text(result.sessionId, "sessionId") };
  }

  async sendTurn(request: {
    projectId: string | null;
    sessionId: string;
    revision: Revision;
    text: string;
    runtimeControls?: RuntimeControlSelection[];
    commandId?: string;
    deliveryMode?: "new_turn" | "steer_now";
    activeTurnId?: string | null;
  }): Promise<{ commandId: string; turnId: string }> {
    const commandId = request.commandId ?? this.dependencies.identity();
    const result = record(await this.dependencies.invoke("send_project_turn", {
      request: {
        correlationId: this.dependencies.identity(),
        commandId,
        projectId: request.projectId,
        sessionId: request.sessionId,
        expectedRevision: request.revision,
        text: request.text,
        runtimeControls: request.runtimeControls ?? [],
        deliveryMode: request.deliveryMode ?? "new_turn",
        expectedActiveTurnId: request.activeTurnId ?? null,
      },
    }), "send_project_turn response");
    return {
      commandId: text(result.commandId, "commandId"),
      turnId: text(result.turnId, "turnId"),
    };
  }

  async cancelTurn(request: { projectId: string | null; sessionId: string; turnId: string }): Promise<void> {
    await this.dependencies.invoke("cancel_project_turn", {
      request: {
        correlationId: this.dependencies.identity(),
        commandId: this.dependencies.identity(),
        projectId: request.projectId,
        sessionId: request.sessionId,
        turnId: request.turnId,
      },
    });
  }

  async saveDraft(request: {
    projectId: string | null;
    sessionId: string;
    commandId: string;
    text: string;
    revision: number;
  }): Promise<{ commandId: string; state: ComposerDraftWriteState }> {
    const result = record(await this.dependencies.invoke("save_composer_draft", {
      request: {
        correlationId: this.dependencies.identity(),
        operationId: this.dependencies.identity(),
        projectId: request.projectId,
        sessionId: request.sessionId,
        commandId: request.commandId,
        text: request.text,
        revision: request.revision,
        updatedAtUnixMs: Date.now(),
      },
    }), "save_composer_draft response");
    const state = text(result.state, "state");
    if (
      state !== "composer_draft_write_state_saved"
      && state !== "composer_draft_write_state_already_accepted"
    ) throw new Error("Invalid save_composer_draft state");
    return { commandId: text(result.commandId, "commandId"), state };
  }

  async discardDraft(request: {
    projectId: string | null;
    sessionId: string;
    commandId: string;
  }): Promise<boolean> {
    const result = await this.dependencies.invoke("discard_composer_draft", {
      request: {
        correlationId: this.dependencies.identity(),
        operationId: this.dependencies.identity(),
        projectId: request.projectId,
        sessionId: request.sessionId,
        commandId: request.commandId,
      },
    });
    if (typeof result !== "boolean") throw new Error("Invalid discard_composer_draft response");
    return result;
  }
}

export function applyProjectChatEvent(
  current: ProjectChatState,
  event: ProjectChatEvent,
): ProjectChatState {
  if (event.kind === "snapshot") return structuredClone(event.snapshot);
  if (event.kind !== "delta") return current;
  const currentRevision = BigInt(current.session.revision);
  const baseRevision = BigInt(event.baseRevision);
  const newRevision = BigInt(event.newRevision);
  if (newRevision !== baseRevision + 1n) throw new Error("Session revision gap");
  if (newRevision <= currentRevision) return current;
  if (baseRevision !== currentRevision) throw new Error("Session revision gap");
  const next = structuredClone(current);
  for (const mutation of event.mutations) {
    switch (mutation.kind) {
      case "upsertTurn": {
        const index = next.turns.findIndex((turn) => turn.turnId === mutation.turn.turnId);
        if (index === -1) next.turns.push(structuredClone(mutation.turn));
        else {
          if (isTerminalTurnState(next.turns[index].state)) {
            throw new Error("Terminal turn is immutable");
          }
          next.turns[index] = structuredClone(mutation.turn);
        }
        break;
      }
      case "appendTurnText": {
        const turn = next.turns.find((item) => item.turnId === mutation.turnId);
        if (!turn) throw new Error("Missing streamed turn");
        if (isTerminalTurnState(turn.state)) throw new Error("Terminal turn is immutable");
        turn.text += mutation.text;
        turn.state = "turn_state_streaming";
        break;
      }
      case "upsertTurnActivity": {
        const turn = next.turns.find((item) => item.turnId === mutation.turnId);
        if (!turn) throw new Error("Missing activity turn");
        if (isTerminalTurnState(turn.state)) throw new Error("Terminal turn is immutable");
        const index = turn.activities.findIndex(
          (activity) => activity.activityId === mutation.activity.activityId,
        );
        if (index === -1) turn.activities.push(structuredClone(mutation.activity));
        else turn.activities[index] = structuredClone(mutation.activity);
        turn.state = "turn_state_streaming";
        break;
      }
      case "finishTurn": {
        const turn = next.turns.find((item) => item.turnId === mutation.turnId);
        if (!turn) throw new Error("Missing terminal turn");
        if (isTerminalTurnState(turn.state)) throw new Error("Terminal turn is immutable");
        turn.state = mutation.state;
        turn.outcome = structuredClone(mutation.outcome);
        turn.completedAtUnixMs = mutation.completedAtUnixMs;
        break;
      }
      case "updateSession":
        if (mutation.title !== null) next.session.title = mutation.title;
        if (mutation.state !== null) next.session.state = mutation.state;
        next.session.activeTurnId = mutation.activeTurnId;
        break;
    }
  }
  next.session.revision = event.newRevision;
  return next;
}

function isTerminalTurnState(state: string): boolean {
  return state.endsWith("_completed")
    || state.endsWith("_cancelled")
    || state.endsWith("_timed_out")
    || state.endsWith("_failed");
}

export function parseOpenedProjectChat(value: unknown): OpenedProjectChat {
  const valueRecord = record(value, "open_project_chat response");
  return {
    correlationId: text(valueRecord.correlationId, "correlationId"),
    subscriptionId: text(valueRecord.subscriptionId, "subscriptionId"),
    system: parseSystemSnapshot(valueRecord.system),
    session: parseChatState(valueRecord.session),
    draft: valueRecord.draft == null ? null : parseDraft(valueRecord.draft),
  };
}

function parseDraft(value: unknown): ComposerDraft {
  const valueRecord = record(value, "composer draft");
  return {
    commandId: text(valueRecord.commandId, "commandId"),
    text: text(valueRecord.text, "text", true),
    revision: revision(valueRecord.revision, "revision"),
    updatedAtUnixMs: optionalInteger(valueRecord.updatedAtUnixMs, "updatedAtUnixMs"),
  };
}

function parseActivity(value: unknown): TurnActivity {
  const valueRecord = record(value, "turn activity");
  const updatedAtUnixMs = optionalInteger(valueRecord.updatedAtUnixMs, "updatedAtUnixMs");
  return {
    activityId: text(valueRecord.activityId, "activityId"),
    phase: text(valueRecord.phase, "phase"),
    message: optionalText(valueRecord.message, "message"),
    status: text(valueRecord.status, "status"),
    createdRevision: optionalRevision(valueRecord.createdRevision, "createdRevision"),
    createdAtUnixMs: valueRecord.createdAtUnixMs == null
      ? updatedAtUnixMs
      : optionalInteger(valueRecord.createdAtUnixMs, "createdAtUnixMs"),
    updatedAtUnixMs,
    nativeExtensions: array(
      valueRecord.nativeExtensions ?? [],
      "nativeExtensions",
    ).map(parseNativeExtension),
  };
}

function parseNativeExtension(value: unknown): NativeExtension {
  const valueRecord = record(value, "native extension");
  return {
    namespace: text(valueRecord.namespace, "nativeExtension.namespace"),
    schemaVersion: text(valueRecord.schemaVersion, "nativeExtension.schemaVersion"),
    jsonValue: jsonObjectText(valueRecord.jsonValue, "nativeExtension.jsonValue"),
  };
}

function jsonObjectText(value: unknown, label: string): string {
  const encoded = text(value, label);
  try {
    const parsed: unknown = JSON.parse(encoded);
    if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) {
      throw new Error(`Invalid ${label}`);
    }
  } catch {
    throw new Error(`Invalid ${label}`);
  }
  return encoded;
}

export function parseProjectChatEvent(value: unknown): ProjectChatEvent {
  const valueRecord = record(value, "project chat event");
  const kind = text(valueRecord.kind, "kind");
  const subscriptionId = text(valueRecord.subscriptionId, "subscriptionId");
  if (kind === "error") {
    return { kind, subscriptionId, error: parseError(valueRecord.error) };
  }
  const cursor = parseCursor(valueRecord.cursor);
  switch (kind) {
    case "snapshot":
      return { kind, subscriptionId, cursor, snapshot: parseChatState(valueRecord.snapshot) };
    case "delta":
      return {
        kind,
        subscriptionId,
        cursor,
        baseRevision: revision(valueRecord.baseRevision, "baseRevision"),
        newRevision: revision(valueRecord.newRevision, "newRevision"),
        committedAtUnixMs: optionalInteger(valueRecord.committedAtUnixMs, "committedAtUnixMs"),
        mutations: array(valueRecord.mutations, "mutations").map(parseMutation),
      };
    case "heartbeat":
      return { kind, subscriptionId, cursor, currentRevision: revision(valueRecord.currentRevision, "currentRevision") };
    case "resyncRequired":
      return {
        kind,
        subscriptionId,
        cursor,
        reason: text(valueRecord.reason, "reason"),
        currentRevision: revision(valueRecord.currentRevision, "currentRevision"),
      };
    default:
      throw new Error("Unknown project chat event");
  }
}

function parseChatState(value: unknown): ProjectChatState {
  const valueRecord = record(value, "project chat snapshot");
  return {
    session: parseSession(valueRecord.session),
    fingerprint: text(valueRecord.fingerprint, "fingerprint"),
    turns: array(valueRecord.turns, "turns").map(parseTurn),
  };
}

function parseSession(value: unknown): SessionSummary {
  const valueRecord = record(value, "session");
  return {
    sessionId: text(valueRecord.sessionId, "sessionId"),
    projectId: optionalText(valueRecord.projectId, "projectId"),
    title: text(valueRecord.title, "title"),
    state: text(valueRecord.state, "state"),
    revision: revision(valueRecord.revision, "revision"),
    activeTurnId: optionalText(valueRecord.activeTurnId, "activeTurnId"),
    lastActivityAtUnixMs: optionalInteger(valueRecord.lastActivityAtUnixMs, "lastActivityAtUnixMs"),
  };
}

function parseTurn(value: unknown): ProjectTurn {
  const valueRecord = record(value, "turn");
  return {
    turnId: text(valueRecord.turnId, "turnId"),
    commandId: text(valueRecord.commandId, "commandId"),
    role: text(valueRecord.role, "role"),
    state: text(valueRecord.state, "state"),
    text: text(valueRecord.text, "text", true),
    activities: array(valueRecord.activities, "activities").map(parseActivity),
    outcome: valueRecord.outcome === null ? null : parseOutcome(valueRecord.outcome),
    createdRevision: optionalRevision(valueRecord.createdRevision, "createdRevision"),
    createdAtUnixMs: optionalInteger(valueRecord.createdAtUnixMs, "createdAtUnixMs"),
    completedAtUnixMs: optionalInteger(valueRecord.completedAtUnixMs, "completedAtUnixMs"),
  };
}

function parseMutation(value: unknown): SessionMutation {
  const valueRecord = record(value, "session mutation");
  const kind = text(valueRecord.kind, "mutation.kind");
  switch (kind) {
    case "upsertTurn": return { kind, turn: parseTurn(valueRecord.turn) };
    case "appendTurnText": return { kind, turnId: text(valueRecord.turnId, "turnId"), text: text(valueRecord.text, "text") };
    case "upsertTurnActivity": return {
      kind,
      turnId: text(valueRecord.turnId, "turnId"),
      activity: parseActivity(valueRecord.activity),
    };
    case "finishTurn": return {
      kind,
      turnId: text(valueRecord.turnId, "turnId"),
      state: text(valueRecord.state, "state"),
      outcome: valueRecord.outcome === null ? null : parseOutcome(valueRecord.outcome),
      completedAtUnixMs: optionalInteger(valueRecord.completedAtUnixMs, "completedAtUnixMs"),
    };
    case "updateSession": return {
      kind,
      title: optionalText(valueRecord.title, "title"),
      state: optionalText(valueRecord.state, "state"),
      activeTurnId: optionalText(valueRecord.activeTurnId, "activeTurnId"),
    };
    default: throw new Error("Unknown session mutation");
  }
}

function parseOutcome(value: unknown): TurnOutcome {
  const valueRecord = record(value, "turn outcome");
  const kind = text(valueRecord.kind, "outcome.kind");
  if (kind === "result") {
    return { kind, summary: text(valueRecord.summary, "summary", true), partial: flag(valueRecord.partial, "partial") };
  }
  if (kind === "error") return { kind, error: parseError(valueRecord.error) };
  throw new Error("Unknown turn outcome");
}

function parseCursor(value: unknown): WatchCursor {
  const valueRecord = record(value, "cursor");
  return {
    streamId: text(valueRecord.streamId, "streamId"),
    sequence: revision(valueRecord.sequence, "sequence"),
    authorityEpoch: revision(valueRecord.authorityEpoch, "authorityEpoch"),
  };
}

function parseError(value: unknown): UiSafeError {
  const valueRecord = record(value, "error");
  return {
    code: text(valueRecord.code, "code"),
    messageKey: text(valueRecord.messageKey, "messageKey"),
    correlationId: text(valueRecord.correlationId, "correlationId", true),
    retryable: flag(valueRecord.retryable, "retryable"),
    userActionRequired: flag(valueRecord.userActionRequired, "userActionRequired"),
    detailsHandle: optionalText(valueRecord.detailsHandle, "detailsHandle"),
    currentRevision: valueRecord.currentRevision === null ? null : revision(valueRecord.currentRevision, "currentRevision"),
  };
}

function bridgeError(code: string, correlationId: string): UiSafeError {
  return { code, messageKey: `desktop.${code}`, correlationId, retryable: true, userActionRequired: false, detailsHandle: null, currentRevision: null };
}

function record(value: unknown, label: string): Record<string, unknown> {
  if (value === null || typeof value !== "object" || Array.isArray(value)) throw new Error(`Invalid ${label}`);
  return value as Record<string, unknown>;
}

function array(value: unknown, label: string): unknown[] {
  if (!Array.isArray(value)) throw new Error(`Invalid ${label}`);
  return value;
}

function text(value: unknown, label: string, empty = false): string {
  if (typeof value !== "string" || (!empty && value.length === 0)) throw new Error(`Invalid ${label}`);
  return value;
}

function optionalText(value: unknown, label: string): string | null {
  return value === null ? null : text(value, label);
}

function revision(value: unknown, label: string): Revision {
  const parsed = text(value, label);
  if (!/^(0|[1-9]\d*)$/.test(parsed)) throw new Error(`Invalid ${label}`);
  return parsed;
}

function optionalRevision(value: unknown, label: string): Revision | null {
  return value == null ? null : revision(value, label);
}

function optionalInteger(value: unknown, label: string): number | null {
  if (value === null) return null;
  if (typeof value !== "number" || !Number.isSafeInteger(value)) throw new Error(`Invalid ${label}`);
  return value;
}

function flag(value: unknown, label: string): boolean {
  if (typeof value !== "boolean") throw new Error(`Invalid ${label}`);
  return value;
}
