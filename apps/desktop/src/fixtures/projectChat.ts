export const fixtureIds = [
  "streaming",
  "restored",
  "cached",
  "stopped",
  "timed-out",
  "stale",
  "resyncing",
  "loading",
  "empty",
] as const;

export type FixtureId = (typeof fixtureIds)[number];
export type FixtureTone = "active" | "good" | "quiet" | "warning" | "danger";

export interface ChatActivity {
  id: string;
  phase: string;
  message: string | null;
  status: string;
}

export interface ChatMessage {
  id: string;
  author: "user" | "agent";
  paragraphs: string[];
  bullets?: string[];
  timestamp: string;
  activities?: ChatActivity[];
  startedAtUnixMs?: number | null;
  completedAtUnixMs?: number | null;
  active?: boolean;
  terminalState?: string | null;
}

export interface ProjectChatSnapshot {
  state: FixtureId;
  stateLabel: string;
  stateTone: FixtureTone;
  notice: string;
  phase: string;
  freshness: string;
  canStop: boolean;
  messages: ChatMessage[];
}

export interface DennettClient {
  readProjectChat(request: { projectId: string; sessionId: string }): Promise<ProjectChatSnapshot>;
}

export const fixtureLabels: Record<FixtureId, string> = {
  streaming: "Streaming",
  restored: "Restored",
  cached: "Cached",
  stopped: "Stopped",
  "timed-out": "Timed out",
  stale: "Stale",
  resyncing: "Resyncing",
  loading: "Loading",
  empty: "Empty",
};

const baseMessages: ChatMessage[] = [
  {
    id: "message-1",
    author: "user",
    paragraphs: [
      "Rework the checkpoint into a monochrome glass workbench. Keep projects and their chats together, then show standalone chats below them.",
    ],
    timestamp: "10:32",
  },
  {
    id: "message-2",
    author: "agent",
    paragraphs: [
      "I mapped the corrections to the M01 presentation boundary and kept the renderer on a transport-neutral client.",
      "The shell now removes browser-like navigation, redundant project controls and persistent infrastructure noise.",
    ],
    bullets: [
      "Breadcrumbs include the selected chat",
      "The right workspace opens fixtures without external effects",
      "Provider and access controls stay truthful to the Codex-only constraint",
    ],
    timestamp: "10:34",
  },
  {
    id: "message-3",
    author: "user",
    paragraphs: [
      "Keep it minimal: gray user messages, unboxed model replies, rounder surfaces and no color accents.",
    ],
    timestamp: "10:35",
  },
];

const activeMessage: ChatMessage = {
  id: "message-4",
  author: "agent",
  paragraphs: [
    "The second workbench pass is assembled. I am checking keyboard paths, the collapsible resource workspace and the compact composer before the owner checkpoint.",
  ],
  bullets: ["Monochrome tokens applied", "Resource viewer connected", "Accessibility pass running"],
  timestamp: "10:36",
};

const stateData: Record<
  FixtureId,
  Omit<ProjectChatSnapshot, "state" | "messages"> & { messages?: ChatMessage[] }
> = {
  streaming: {
    stateLabel: "Working",
    stateTone: "active",
    notice: "Codex is checking the renderer. You can steer or stop this session.",
    phase: "Accessibility pass",
    freshness: "Live",
    canStop: true,
  },
  restored: {
    stateLabel: "Restored",
    stateTone: "good",
    notice: "The session was restored from the authoritative local snapshot.",
    phase: "Ready for review",
    freshness: "Synced now",
    canStop: false,
  },
  cached: {
    stateLabel: "Cached",
    stateTone: "quiet",
    notice: "Showing the last local snapshot while the node reconnects.",
    phase: "Read-only snapshot",
    freshness: "Cached 2 min ago",
    canStop: false,
  },
  stopped: {
    stateLabel: "Stopped",
    stateTone: "warning",
    notice: "Generation stopped for this session. The partial response is preserved.",
    phase: "Stopped by user",
    freshness: "Updated now",
    canStop: false,
  },
  "timed-out": {
    stateLabel: "Timed out",
    stateTone: "danger",
    notice: "The runtime did not acknowledge completion. Retry when the connection is healthy.",
    phase: "Attention required",
    freshness: "No final receipt",
    canStop: false,
  },
  stale: {
    stateLabel: "Stale",
    stateTone: "warning",
    notice: "This view is behind the authoritative revision. Mutating actions are unavailable.",
    phase: "Waiting for revision 48",
    freshness: "Last synced 11 min ago",
    canStop: false,
  },
  resyncing: {
    stateLabel: "Resyncing",
    stateTone: "active",
    notice: "Refreshing the session snapshot after a revision gap.",
    phase: "Fetching authoritative state",
    freshness: "Revision check in progress",
    canStop: false,
  },
  loading: {
    stateLabel: "Loading",
    stateTone: "quiet",
    notice: "Opening the local Project Chat snapshot.",
    phase: "Loading conversation",
    freshness: "Not available yet",
    canStop: false,
    messages: [],
  },
  empty: {
    stateLabel: "Ready",
    stateTone: "good",
    notice: "Start a direct conversation with the project agent.",
    phase: "New session",
    freshness: "No messages yet",
    canStop: false,
    messages: [],
  },
};

export function createFixtureDennettClient(fixture: FixtureId): DennettClient {
  return {
    async readProjectChat(_request) {
      const fixtureState = stateData[fixture];
      const messages = fixtureState.messages ?? [
        ...baseMessages,
        ...(fixture === "streaming" || fixture === "stopped" || fixture === "timed-out"
          ? [activeMessage]
          : []),
      ];
      return structuredClone({ state: fixture, ...fixtureState, messages });
    },
  };
}
