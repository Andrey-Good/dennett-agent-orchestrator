import { Channel, invoke } from "@tauri-apps/api/core";

export type Revision = string;

export interface UiSafeError {
  code: string;
  messageKey: string;
  correlationId: string;
  retryable: boolean;
  userActionRequired: boolean;
  detailsHandle: string | null;
  currentRevision: Revision | null;
}

export interface ProjectSummary {
  projectId: string;
  displayName: string;
  state: string;
  revision: Revision;
  lastActivityAtUnixMs: number | null;
}

export interface SessionSummary {
  sessionId: string;
  projectId: string | null;
  title: string;
  state: string;
  revision: Revision;
  activeTurnId: string | null;
  lastActivityAtUnixMs: number | null;
}

export interface SystemSnapshot {
  revision: Revision;
  authorityEpoch: Revision;
  observedAtUnixMs: number | null;
  projects: ProjectSummary[];
  recentSessions: SessionSummary[];
  activeProjectId: string | null;
  activeSessionId: string | null;
  nodeState: string;
}

export interface WatchCursor {
  streamId: string;
  sequence: Revision;
  authorityEpoch: Revision;
}

export type BridgePhase =
  | "discovering_node"
  | "starting_node"
  | "handshaking"
  | "subscribing"
  | "watching"
  | "reconnecting";

export type SystemMutation = Record<string, unknown> & { kind: string };

export type SystemEvent =
  | { kind: "phase"; subscriptionId: string; phase: BridgePhase; attempt: number }
  | {
      kind: "snapshot";
      subscriptionId: string;
      cursor: WatchCursor;
      snapshot: SystemSnapshot;
      fingerprint: string;
    }
  | {
      kind: "delta";
      subscriptionId: string;
      cursor: WatchCursor;
      baseRevision: Revision;
      newRevision: Revision;
      mutations: SystemMutation[];
    }
  | {
      kind: "heartbeat";
      subscriptionId: string;
      cursor: WatchCursor;
      currentRevision: Revision;
      observedAtUnixMs: number | null;
    }
  | {
      kind: "resyncRequired";
      subscriptionId: string;
      cursor: WatchCursor;
      reason: string;
      currentRevision: Revision;
      snapshotRequired: boolean;
    }
  | { kind: "error"; subscriptionId: string; error: UiSafeError };

export interface OpenedSystemWatch {
  correlationId: string;
  subscriptionId: string;
  snapshot: SystemSnapshot;
}

export interface SystemWatchHandle {
  readonly opened: OpenedSystemWatch;
  close(): Promise<boolean>;
}

export interface SystemBridgeClient {
  openSystemWatch(
    onEvent: (event: SystemEvent) => void,
    correlationId?: string,
  ): Promise<SystemWatchHandle>;
}

export interface SystemBridgeDependencies {
  invoke(command: string, args: Record<string, unknown>): Promise<unknown>;
  createChannel(onMessage: (event: unknown) => void): unknown;
  correlationId(): string;
}

const defaultDependencies: SystemBridgeDependencies = {
  invoke: (command, args) => invoke(command, args),
  createChannel: (onMessage) => new Channel(onMessage),
  correlationId: () => crypto.randomUUID(),
};

export class TauriSystemBridgeClient implements SystemBridgeClient {
  constructor(private readonly dependencies: SystemBridgeDependencies = defaultDependencies) {}

  async openSystemWatch(
    onEvent: (event: SystemEvent) => void,
    correlationId = this.dependencies.correlationId(),
  ): Promise<SystemWatchHandle> {
    const channel = this.dependencies.createChannel((rawEvent) => {
      try {
        onEvent(parseSystemEvent(rawEvent));
      } catch {
        onEvent({
          kind: "error",
          subscriptionId: "unbound",
          error: {
            code: "desktop_bridge_event_invalid",
            messageKey: "desktop.bridge_event_invalid",
            correlationId,
            retryable: true,
            userActionRequired: false,
            detailsHandle: null,
            currentRevision: null,
          },
        });
      }
    });
    const rawOpened = await this.dependencies.invoke("open_system_watch", {
      request: { correlationId },
      onEvent: channel,
    });
    const opened = parseOpenedSystemWatch(rawOpened);
    let closed = false;
    return {
      opened,
      close: async () => {
        void channel;
        if (closed) return false;
        closed = true;
        const result = await this.dependencies.invoke("close_system_watch", {
          request: { subscriptionId: opened.subscriptionId },
        });
        if (typeof result !== "boolean") throw new Error("Invalid close_system_watch response");
        return result;
      },
    };
  }
}

export function parseOpenedSystemWatch(value: unknown): OpenedSystemWatch {
  const record = object(value, "open_system_watch response");
  return {
    correlationId: text(record.correlationId, "correlationId"),
    subscriptionId: text(record.subscriptionId, "subscriptionId"),
    snapshot: parseSnapshot(record.snapshot),
  };
}

export function parseSystemEvent(value: unknown): SystemEvent {
  const record = object(value, "system event");
  const kind = text(record.kind, "kind");
  const subscriptionId = text(record.subscriptionId, "subscriptionId");
  switch (kind) {
    case "phase": {
      const phase = text(record.phase, "phase");
      if (!bridgePhases.has(phase as BridgePhase)) throw new Error("Invalid bridge phase");
      return {
        kind,
        subscriptionId,
        phase: phase as BridgePhase,
        attempt: integer(record.attempt, "attempt"),
      };
    }
    case "snapshot":
      return {
        kind,
        subscriptionId,
        cursor: parseCursor(record.cursor),
        snapshot: parseSnapshot(record.snapshot),
        fingerprint: text(record.fingerprint, "fingerprint"),
      };
    case "delta":
      return {
        kind,
        subscriptionId,
        cursor: parseCursor(record.cursor),
        baseRevision: revision(record.baseRevision, "baseRevision"),
        newRevision: revision(record.newRevision, "newRevision"),
        mutations: array(record.mutations, "mutations").map((mutation) => {
          const parsed = object(mutation, "mutation");
          text(parsed.kind, "mutation.kind");
          return parsed as SystemMutation;
        }),
      };
    case "heartbeat":
      return {
        kind,
        subscriptionId,
        cursor: parseCursor(record.cursor),
        currentRevision: revision(record.currentRevision, "currentRevision"),
        observedAtUnixMs: optionalInteger(record.observedAtUnixMs, "observedAtUnixMs"),
      };
    case "resyncRequired":
      return {
        kind,
        subscriptionId,
        cursor: parseCursor(record.cursor),
        reason: text(record.reason, "reason"),
        currentRevision: revision(record.currentRevision, "currentRevision"),
        snapshotRequired: flag(record.snapshotRequired, "snapshotRequired"),
      };
    case "error":
      return { kind, subscriptionId, error: parseError(record.error) };
    default:
      throw new Error("Unknown system event kind");
  }
}

function parseSnapshot(value: unknown): SystemSnapshot {
  const record = object(value, "snapshot");
  return {
    revision: revision(record.revision, "revision"),
    authorityEpoch: revision(record.authorityEpoch, "authorityEpoch"),
    observedAtUnixMs: optionalInteger(record.observedAtUnixMs, "observedAtUnixMs"),
    projects: array(record.projects, "projects").map(parseProject),
    recentSessions: array(record.recentSessions, "recentSessions").map(parseSession),
    activeProjectId: optionalText(record.activeProjectId, "activeProjectId"),
    activeSessionId: optionalText(record.activeSessionId, "activeSessionId"),
    nodeState: text(record.nodeState, "nodeState"),
  };
}

function parseProject(value: unknown): ProjectSummary {
  const record = object(value, "project");
  return {
    projectId: text(record.projectId, "projectId"),
    displayName: text(record.displayName, "displayName"),
    state: text(record.state, "state"),
    revision: revision(record.revision, "revision"),
    lastActivityAtUnixMs: optionalInteger(record.lastActivityAtUnixMs, "lastActivityAtUnixMs"),
  };
}

function parseSession(value: unknown): SessionSummary {
  const record = object(value, "session");
  return {
    sessionId: text(record.sessionId, "sessionId"),
    projectId: optionalText(record.projectId, "projectId"),
    title: text(record.title, "title"),
    state: text(record.state, "state"),
    revision: revision(record.revision, "revision"),
    activeTurnId: optionalText(record.activeTurnId, "activeTurnId"),
    lastActivityAtUnixMs: optionalInteger(record.lastActivityAtUnixMs, "lastActivityAtUnixMs"),
  };
}

function parseCursor(value: unknown): WatchCursor {
  const record = object(value, "cursor");
  return {
    streamId: text(record.streamId, "streamId"),
    sequence: revision(record.sequence, "sequence"),
    authorityEpoch: revision(record.authorityEpoch, "authorityEpoch"),
  };
}

function parseError(value: unknown): UiSafeError {
  const record = object(value, "error");
  return {
    code: text(record.code, "error.code"),
    messageKey: text(record.messageKey, "error.messageKey"),
    correlationId: text(record.correlationId, "error.correlationId", true),
    retryable: flag(record.retryable, "error.retryable"),
    userActionRequired: flag(record.userActionRequired, "error.userActionRequired"),
    detailsHandle: optionalText(record.detailsHandle, "error.detailsHandle"),
    currentRevision:
      record.currentRevision === null
        ? null
        : revision(record.currentRevision, "error.currentRevision"),
  };
}

const bridgePhases = new Set<BridgePhase>([
  "discovering_node",
  "starting_node",
  "handshaking",
  "subscribing",
  "watching",
  "reconnecting",
]);

function object(value: unknown, label: string): Record<string, unknown> {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`Invalid ${label}`);
  }
  return value as Record<string, unknown>;
}

function array(value: unknown, label: string): unknown[] {
  if (!Array.isArray(value)) throw new Error(`Invalid ${label}`);
  return value;
}

function text(value: unknown, label: string, emptyAllowed = false): string {
  if (typeof value !== "string" || (!emptyAllowed && value.length === 0)) {
    throw new Error(`Invalid ${label}`);
  }
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

function integer(value: unknown, label: string): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value)) throw new Error(`Invalid ${label}`);
  return value;
}

function optionalInteger(value: unknown, label: string): number | null {
  return value === null ? null : integer(value, label);
}

function flag(value: unknown, label: string): boolean {
  if (typeof value !== "boolean") throw new Error(`Invalid ${label}`);
  return value;
}
