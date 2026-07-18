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

export interface ChatMessage {
  id: string;
  author: "user" | "agent";
  paragraphs: string[];
  bullets?: string[];
  timestamp: string;
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
      "Build the first Project Chat slice from the approved workbench direction. Keep the provider boundary replaceable.",
    ],
    timestamp: "10:32",
  },
  {
    id: "message-2",
    author: "agent",
    paragraphs: [
      "I mapped the screen to the M01 contract and kept the renderer on a transport-neutral client.",
      "The first slice concentrates on one direct conversation and makes uncertain runtime state visible without exposing technical logs by default.",
    ],
    bullets: [
      "Project and session context stay visible",
      "The auxiliary pane is read-only until M02",
      "Composer actions affect presentation state only",
    ],
    timestamp: "10:34",
  },
  {
    id: "message-3",
    author: "user",
    paragraphs: [
      "Good. Keep the controls under the prompt compact and make the sidebar feel like one surface with the title row.",
    ],
    timestamp: "10:35",
  },
];

const activeMessage: ChatMessage = {
  id: "message-4",
  author: "agent",
  paragraphs: [
    "The workbench shell is now assembled. I am checking keyboard paths, bounded status announcements and the responsive inspector before the owner checkpoint.",
  ],
  bullets: ["Renderer fixture connected", "Visual tokens applied", "Accessibility pass running"],
  timestamp: "now",
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
