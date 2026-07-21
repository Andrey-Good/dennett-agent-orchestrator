import React from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { IconProps } from "@phosphor-icons/react";
import {
  ArrowUp,
  Brain,
  BracketsCurly,
  Browsers,
  CaretDown,
  ChatCircleDots,
  ChatsCircle,
  CheckCircle,
  CircleNotch,
  Command,
  Copy,
  DownloadSimple,
  FileCode,
  FileText,
  FolderOpen,
  FolderPlus,
  FolderSimple,
  Gauge,
  GitBranch,
  GithubLogo,
  Globe,
  Info,
  LinkSimple,
  ListChecks,
  MagnifyingGlass,
  Microphone,
  Minus,
  Plug,
  Plus,
  Robot,
  ShieldCheck,
  Sidebar,
  SidebarSimple,
  Square,
  Stop,
  WarningCircle,
  X,
} from "@phosphor-icons/react";
import {
  createFixtureDennettClient,
  fixtureIds,
  fixtureLabels,
  type ChatMessage,
  type FixtureId,
  type FixtureTone,
  type ProjectChatSnapshot,
} from "./fixtures/projectChat";
import {
  TauriProjectChatClient,
  applyProjectChatEvent,
  type ProjectChatEvent,
  type ProjectChatState,
  type ProjectTurn,
} from "./lib/projectChatBridge";
import type {
  RuntimeControlChoice,
  RuntimeControlDescriptor,
  RuntimeControlSelection,
  SystemEvent,
  SystemSnapshot,
} from "./lib/systemBridge";
import { applySystemEvent, TauriSystemBridgeClient } from "./lib/systemBridge";
import "./styles.css";

type Icon = React.ComponentType<IconProps>;
type ComposerPopover = "context" | "plugins" | "access" | "runtime" | null;
type ProjectCreationKind = "empty" | "existing";
type AccessMode = "full" | "auto";
type ReasoningLevel = "medium" | "high";
type LiveConnectionPhase = "opening" | "empty" | "live" | "resyncing" | "error";
type SystemConnectionPhase = "opening" | "live" | "error";
interface LiveConnectionState {
  phase: LiveConnectionPhase;
  message: string | null;
  targetSessionId: string | null;
  lastEventAtUnixMs: number | null;
}

interface SystemConnectionState {
  phase: SystemConnectionPhase;
  message: string | null;
}

function bridgeFailure(error: unknown, fallback: string): { message: string; retryable: boolean } {
  if (error && typeof error === "object" && "messageKey" in error) {
    const candidate = error as {
      messageKey?: unknown;
      retryable?: unknown;
      userActionRequired?: unknown;
    };
    if (typeof candidate.messageKey === "string" && candidate.messageKey.length > 0) {
      return {
        message: candidate.messageKey,
        retryable: candidate.retryable === true && candidate.userActionRequired !== true,
      };
    }
  }
  return { message: fallback, retryable: false };
}
type WorkspaceSurface =
  | { kind: "chat" }
  | { kind: "browser"; title: string; subtitle: string }
  | { kind: "report"; title: string; subtitle: string }
  | { kind: "source"; title: string; subtitle: string };

interface SessionItem {
  id: string;
  title: string;
  meta: string;
}

interface ProjectGroup {
  id: string;
  title: string;
  sessions: SessionItem[];
}

const projectGroups: ProjectGroup[] = [
  {
    id: "dennett-agent-orchestrator",
    title: "dennett-agent-orchestrator",
    sessions: [
      { id: "screen", title: "Project Chat owner checkpoint", meta: "active now" },
      { id: "protocol", title: "M01 protocol epoch", meta: "2h" },
    ],
  },
  {
    id: "practice",
    title: "Practice",
    sessions: [{ id: "runtime", title: "Codex runtime canary", meta: "1d" }],
  },
];

const recentChats: SessionItem[] = [
  { id: "recent-provider", title: "Provider adapter notes", meta: "3d" },
  { id: "recent-ux", title: "Desktop UX review", meta: "5d" },
  { id: "recent-voice", title: "Voice interaction sketch", meta: "1w" },
];

const updateAvailable = false;
const LIVE_WATCH_STALE_AFTER_MS = 70_000;
const ACCESS_CONTROL_ID = "dennett.access_mode";

function runtimeChoiceAvailable(
  choice: RuntimeControlChoice,
  selections: Readonly<Record<string, string>>,
): boolean {
  return choice.availableWhen.every((condition) => {
    const selected = selections[condition.controlId];
    return selected !== undefined && condition.choiceIds.includes(selected);
  });
}

function reconcileRuntimeSelections(
  controls: readonly RuntimeControlDescriptor[],
  requested: Readonly<Record<string, string>>,
): Record<string, string> {
  const next = Object.fromEntries(controls.flatMap((control) => {
    const initial = control.choices.some((choice) => choice.id === requested[control.id])
      ? requested[control.id]
      : control.defaultChoiceId;
    return initial ? [[control.id, initial]] : [];
  }));
  for (let pass = 0; pass <= controls.length; pass += 1) {
    let changed = false;
    for (const control of controls) {
      const available = control.choices.filter((choice) => runtimeChoiceAvailable(choice, next));
      const selected = available.find((choice) => choice.id === next[control.id])
        ?? available.find((choice) => choice.id === control.defaultChoiceId)
        ?? available[0];
      if (!selected) {
        if (control.id in next) {
          delete next[control.id];
          changed = true;
        }
      } else if (next[control.id] !== selected.id) {
        next[control.id] = selected.id;
        changed = true;
      }
    }
    if (!changed) break;
  }
  return next;
}

const browserPreviewDocument = `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <style>
      :root { color-scheme: dark; font-family: Inter, "Segoe UI", sans-serif; }
      * { box-sizing: border-box; }
      body { margin: 0; min-height: 100vh; display: grid; place-items: center; color: #eeeeee; background: #151515; }
      main { width: min(620px, calc(100% - 48px)); padding: 34px; border: 1px solid #333333; border-radius: 24px; background: #1c1c1c; }
      small { color: #8d8d8d; letter-spacing: .12em; text-transform: uppercase; }
      h1 { margin: 12px 0 10px; font-size: 30px; font-weight: 620; }
      p { margin: 0; color: #bdbdbd; line-height: 1.6; }
      ul { display: grid; grid-template-columns: repeat(3, 1fr); gap: 10px; margin: 26px 0 0; padding: 0; list-style: none; }
      li { padding: 14px; border: 1px solid #303030; border-radius: 14px; color: #a8a8a8; background: #202020; }
      strong { display: block; margin-bottom: 5px; color: #f0f0f0; font-size: 14px; }
    </style>
  </head>
  <body>
    <main>
      <small>Dennett local preview</small>
      <h1>Project Chat renderer</h1>
      <p>This embedded surface demonstrates how a browser, document or artifact can replace the central chat without leaving the desktop workbench.</p>
      <ul>
        <li><strong>Renderer</strong>Available</li>
        <li><strong>Authority</strong>Fixture only</li>
        <li><strong>Effects</strong>Read only</li>
      </ul>
    </main>
  </body>
</html>`;

function IconButton({
  label,
  icon: IconComponent,
  onClick,
  active = false,
  disabled = false,
  className = "",
  buttonRef,
  ariaControls,
  ariaExpanded,
  ariaHasPopup,
}: {
  label: string;
  icon: Icon;
  onClick?: () => void;
  active?: boolean;
  disabled?: boolean;
  className?: string;
  buttonRef?: React.Ref<HTMLButtonElement>;
  ariaControls?: string;
  ariaExpanded?: boolean;
  ariaHasPopup?: "dialog";
}): React.JSX.Element {
  return (
    <button
      ref={buttonRef}
      type="button"
      className={`icon-button${active ? " is-active" : ""}${className ? ` ${className}` : ""}`}
      aria-label={label}
      aria-controls={ariaControls}
      aria-expanded={ariaExpanded}
      aria-haspopup={ariaHasPopup}
      title={label}
      onClick={onClick}
      disabled={disabled || !onClick}
    >
      <IconComponent size={18} weight={active ? "fill" : "regular"} aria-hidden="true" />
    </button>
  );
}

function StateIcon({ tone }: { tone: FixtureTone }): React.JSX.Element {
  if (tone === "danger" || tone === "warning") {
    return <WarningCircle size={15} weight="fill" aria-hidden="true" />;
  }
  if (tone === "active") {
    return <CircleNotch size={15} className="spin" aria-hidden="true" />;
  }
  if (tone === "good") {
    return <CheckCircle size={15} weight="fill" aria-hidden="true" />;
  }
  return <Info size={15} weight="fill" aria-hidden="true" />;
}

function formatClock(unixMs: number | null | undefined): string {
  if (unixMs == null) return "—";
  return new Date(unixMs).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

function formatElapsed(durationMs: number): string {
  const seconds = Math.max(0, Math.floor(durationMs / 1_000));
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  const remainder = seconds % 60;
  return remainder === 0 ? `${minutes}m` : `${minutes}m ${remainder}s`;
}

function activityLabel(phase: string, status: string): string {
  const completed = status.endsWith("_completed");
  const failed = status.endsWith("_failed");
  const stopped = status.endsWith("_cancelled");
  const timedOut = status.endsWith("_timed_out");
  switch (phase) {
    case "commentary": return "Agent update";
    case "command": return stopped ? "Command stopped" : timedOut ? "Command timed out" : failed ? "Command failed" : completed ? "Ran command" : "Running command";
    case "workspace": return stopped ? "File update stopped" : timedOut ? "File update timed out" : failed ? "File update failed" : completed ? "Updated files" : "Updating files";
    case "web_search": return stopped ? "Search stopped" : timedOut ? "Search timed out" : completed ? "Searched the web" : "Searching the web";
    case "plan": return stopped ? "Plan update stopped" : timedOut ? "Plan update timed out" : completed ? "Updated the plan" : "Updating the plan";
    case "tool": return stopped ? "Tool stopped" : timedOut ? "Tool timed out" : failed ? "Tool failed" : completed ? "Used tool" : "Using tool";
    default: return completed ? "Completed activity" : "Working";
  }
}

function ActivityIcon({ phase, status }: { phase: string; status: string }): React.JSX.Element {
  if (status.endsWith("_cancelled")) return <Stop size={15} weight="fill" aria-hidden="true" />;
  if (status.endsWith("_timed_out")) return <WarningCircle size={15} aria-hidden="true" />;
  if (status.endsWith("_failed")) return <WarningCircle size={15} aria-hidden="true" />;
  if (!status.endsWith("_completed")) return <CircleNotch size={15} className="spin" aria-hidden="true" />;
  if (phase === "commentary") return <ChatCircleDots size={15} aria-hidden="true" />;
  if (phase === "workspace") return <FileCode size={15} aria-hidden="true" />;
  if (phase === "web_search") return <Globe size={15} aria-hidden="true" />;
  if (phase === "plan") return <ListChecks size={15} aria-hidden="true" />;
  if (phase === "tool") return <Plug size={15} aria-hidden="true" />;
  return <Command size={15} aria-hidden="true" />;
}

function effectiveActivityStatus(message: ChatMessage, status: string): string {
  if (status.endsWith("_completed") || status.endsWith("_failed") || message.active) return status;
  const terminal = message.terminalState ?? "";
  if (terminal.endsWith("_cancelled")) return "turn_activity_status_cancelled";
  if (terminal.endsWith("_timed_out")) return "turn_activity_status_timed_out";
  if (terminal.endsWith("_failed")) return "turn_activity_status_failed";
  return "turn_activity_status_completed";
}

function ActivityTimeline({ message }: { message: ChatMessage }): React.JSX.Element | null {
  const [, refresh] = React.useReducer((value: number) => value + 1, 0);
  React.useEffect(() => {
    if (!message.active) return;
    const timer = window.setInterval(refresh, 1_000);
    return () => window.clearInterval(timer);
  }, [message.active]);
  if (message.author !== "agent" || message.startedAtUnixMs == null) return null;
  const elapsedUntil = message.completedAtUnixMs ?? (message.active ? Date.now() : null);
  const elapsed = elapsedUntil === null
    ? null
    : formatElapsed(elapsedUntil - message.startedAtUnixMs);
  const activities = message.activities ?? [];
  const showSummary = message.showActivitySummary !== false;
  if (activities.length === 0 && !showSummary) return null;
  const terminal = message.terminalState ?? "";
  const summary = message.active
    ? { icon: <CircleNotch size={15} className="spin" aria-hidden="true" />, text: `Working for ${elapsed ?? "0s"}` }
    : terminal.endsWith("_cancelled")
      ? { icon: <Stop size={15} weight="fill" aria-hidden="true" />, text: elapsed ? `Stopped after ${elapsed}` : "Stopped" }
      : terminal.endsWith("_timed_out")
        ? { icon: <WarningCircle size={15} aria-hidden="true" />, text: elapsed ? `Timed out after ${elapsed}` : "Timed out" }
        : terminal.endsWith("_failed")
          ? { icon: <WarningCircle size={15} aria-hidden="true" />, text: elapsed ? `Failed after ${elapsed}` : "Failed" }
          : { icon: <CheckCircle size={15} aria-hidden="true" />, text: elapsed ? `Worked for ${elapsed}` : "Completed" };
  return (
    <section className={`turn-activity${message.active ? " is-active" : ""}`} aria-label="Agent work">
      {activities.length > 0 ? (
        <div className="turn-activity__items">
          {activities.map((activity) => activity.phase === "commentary" && activity.message ? (
            <div className="turn-activity__commentary" key={activity.id}>
              <ReactMarkdown remarkPlugins={[remarkGfm]} skipHtml>{activity.message}</ReactMarkdown>
            </div>
          ) : (() => {
            const status = effectiveActivityStatus(message, activity.status);
            return (
              <div className="turn-activity__item" key={activity.id}>
                <ActivityIcon phase={activity.phase} status={status} />
                <span>
                  <strong>{activityLabel(activity.phase, status)}</strong>
                  {activity.message && <small>{activity.message}</small>}
                </span>
              </div>
            );
          })())}
        </div>
      ) : message.active ? <p className="turn-activity__waiting">Starting the agent…</p> : null}
      {showSummary && (
        <div className="turn-activity__summary">
          {summary.icon}
          <strong>{summary.text}</strong>
        </div>
      )}
    </section>
  );
}

function messageCopyText(message: ChatMessage): string {
  const visibleBullets = message.author === "agent" ? undefined : message.bullets;
  return [message.paragraphs.join("\n\n"), visibleBullets?.map((item) => `- ${item}`).join("\n")]
    .filter((part) => part?.trim())
    .join("\n\n");
}

function Message({ message, onCopy }: { message: ChatMessage; onCopy: (author: ChatMessage["author"], text: string) => void }): React.JSX.Element {
  const copyText = messageCopyText(message);
  return (
    <article className={`message message--${message.author}`} aria-label={`${message.author} message`}>
      <div className="message-block">
        <div className="message-copy">
          <ActivityTimeline message={message} />
          {message.author === "agent" ? (
            message.paragraphs.join("\n\n").length > 0 && (
              <div className="message-markdown">
                <ReactMarkdown remarkPlugins={[remarkGfm]} skipHtml>
                  {message.paragraphs.join("\n\n")}
                </ReactMarkdown>
              </div>
            )
          ) : message.paragraphs.map((paragraph) => <p key={paragraph}>{paragraph}</p>)}
          {message.author !== "agent" && message.bullets && (
            <ul>{message.bullets.map((item) => <li key={item}>{item}</li>)}</ul>
          )}
          {message.deliveryError && (
            <span className="message-delivery-error"><WarningCircle size={13} aria-hidden="true" />{message.deliveryError}</span>
          )}
        </div>
        <div className="message-footer">
          <span className="message-time">{message.timestamp}</span>
          {copyText && (
            <button type="button" className="message-copy-button" aria-label={`Copy ${message.author} message`} title="Copy message" onClick={() => onCopy(message.author, copyText)}>
              <Copy size={14} aria-hidden="true" />
            </button>
          )}
        </div>
      </div>
    </article>
  );
}

function EmptyConversation({
  onSuggestion,
  projectScoped,
  chatAvailable = true,
}: {
  onSuggestion: (prompt: string) => void;
  projectScoped: boolean;
  chatAvailable?: boolean;
}): React.JSX.Element {
  return (
    <div className="empty-state">
      <ChatCircleDots size={30} aria-hidden="true" />
      <h2>{projectScoped ? "Start with the project" : "Start a conversation"}</h2>
      <p>{!chatAvailable
        ? "Create a standalone chat from Recent, or select a project chat."
        : projectScoped
        ? "Ask the direct agent to inspect, explain or change the selected workspace."
        : "Ask the agent to explore an idea, answer a question or begin a task."}</p>
      {chatAvailable && <div className="prompt-suggestions" role="group" aria-label="Prompt suggestions">
        <button type="button" onClick={() => onSuggestion("Задай мне вопросы, чтобы лучше понять мою идею проекта")}>Задай мне вопросы о моей идее</button>
        <button type="button" onClick={() => onSuggestion("Изучи этот репозиторий и расскажи, что это")}>Изучи этот репозиторий</button>
      </div>}
    </div>
  );
}

function isTerminalTurn(state: string): boolean {
  return /_(completed|cancelled|timed_out|failed)$/.test(state);
}

function visibleTurn(turn: ProjectTurn): boolean {
  return turn.text.length > 0 || turn.activities.length > 0 || !isTerminalTurn(turn.state);
}

function userMessage(turn: ProjectTurn): ChatMessage {
  return {
    id: turn.turnId,
    author: "user",
    paragraphs: turn.text.split(/\n\s*\n/).filter(Boolean),
    timestamp: formatClock(turn.createdAtUnixMs),
    startedAtUnixMs: turn.createdAtUnixMs,
    completedAtUnixMs: turn.completedAtUnixMs,
    active: !isTerminalTurn(turn.state),
    terminalState: turn.state,
    deliveryError: turn.state.endsWith("_failed")
      ? "Clarification delivery could not be confirmed"
      : undefined,
  };
}

function finalCommentaryId(turn: ProjectTurn): string | undefined {
  const finalTexts = new Set([
    turn.text.trim(),
    turn.outcome?.kind === "result" ? turn.outcome.summary?.trim() ?? "" : "",
  ].filter(Boolean));
  if (finalTexts.size === 0) return undefined;
  return [...turn.activities].reverse().find((activity) => (
    activity.phase === "commentary"
    && activity.message !== null
    && finalTexts.has(activity.message.trim())
  ))?.activityId;
}

function activitySegment(activity: ProjectTurn["activities"][number], steers: readonly ProjectTurn[]): number {
  if (
    activity.createdRevision != null
    && steers.every((steer) => steer.createdRevision != null)
  ) {
    const activityRevision = BigInt(activity.createdRevision);
    return steers.filter((steer) => activityRevision >= BigInt(steer.createdRevision!)).length;
  }
  const causalAt = activity.createdAtUnixMs ?? activity.updatedAtUnixMs;
  if (causalAt === null) return steers.length;
  return steers.filter((steer) => causalAt >= (steer.createdAtUnixMs ?? Number.NEGATIVE_INFINITY)).length;
}

function agentMessages(turn: ProjectTurn, steers: readonly ProjectTurn[]): Array<ChatMessage | null> {
  const duplicateFinalCommentary = finalCommentaryId(turn);
  const activities = turn.activities.filter((activity) => (
    activity.phase !== "reasoning_summary" && activity.activityId !== duplicateFinalCommentary
  ));
  const groupedActivities = Array.from({ length: steers.length + 1 }, () => [] as typeof activities);
  for (const activity of activities) {
    const segment = activitySegment(activity, steers);
    groupedActivities[Math.min(segment, steers.length)].push(activity);
  }
  const active = !isTerminalTurn(turn.state);
  return groupedActivities.map((segmentActivities, index) => {
    const last = index === groupedActivities.length - 1;
    const paragraphs = last ? turn.text.split(/\n\s*\n/).filter(Boolean) : [];
    if (segmentActivities.length === 0 && paragraphs.length === 0 && !(last && (active || turn.createdAtUnixMs !== null))) {
      return null;
    }
    return {
      id: `${turn.turnId}:segment:${index}`,
      author: "agent" as const,
      paragraphs,
      timestamp: formatClock(index === 0 ? turn.createdAtUnixMs : steers[index - 1]?.createdAtUnixMs),
      activities: segmentActivities.map((activity) => ({
        id: activity.activityId,
        phase: activity.phase,
        message: activity.message,
        status: activity.status,
        createdRevision: activity.createdRevision,
        createdAtUnixMs: activity.createdAtUnixMs,
        updatedAtUnixMs: activity.updatedAtUnixMs,
      })),
      startedAtUnixMs: turn.createdAtUnixMs,
      completedAtUnixMs: last ? turn.completedAtUnixMs : null,
      active,
      terminalState: turn.state,
      showActivitySummary: last,
    };
  });
}

function liveMessages(turns: readonly ProjectTurn[]): ChatMessage[] {
  const agentEntries = turns.flatMap((turn, index) => turn.role.endsWith("_agent") ? [{ turn, index }] : []);
  const pairedUsers = new Set(agentEntries.flatMap(({ turn: agent }) => {
    const user = turns.find((candidate) => (
      candidate.role.endsWith("_user") && candidate.commandId === agent.commandId
    ));
    return user ? [user.turnId] : [];
  }));
  const consumedUsers = new Set<string>();
  const blocks = agentEntries.map(({ turn: agent, index: agentIndex }, agentPosition) => {
    const nextAgentIndex = agentEntries[agentPosition + 1]?.index ?? turns.length;
    const pairedUser = turns.find((candidate) => (
      candidate.role.endsWith("_user") && candidate.commandId === agent.commandId
    ));
    const steers = turns.slice(agentIndex + 1, nextAgentIndex).filter((candidate) => (
      candidate.role.endsWith("_user") && !pairedUsers.has(candidate.turnId)
    ));
    if (pairedUser) consumedUsers.add(pairedUser.turnId);
    for (const steer of steers) consumedUsers.add(steer.turnId);
    const segments = agentMessages(agent, steers);
    const messages: ChatMessage[] = [];
    if (pairedUser && visibleTurn(pairedUser)) messages.push(userMessage(pairedUser));
    for (let segment = 0; segment < segments.length; segment += 1) {
      const agentSegment = segments[segment];
      if (agentSegment) messages.push(agentSegment);
      const steer = steers[segment];
      if (steer && visibleTurn(steer)) messages.push(userMessage(steer));
    }
    return {
      index: pairedUser ? turns.indexOf(pairedUser) : agentIndex,
      messages,
    };
  });
  const orphanUsers = turns.flatMap((turn, index) => (
    turn.role.endsWith("_user") && !consumedUsers.has(turn.turnId) && visibleTurn(turn)
      ? [{ index, messages: [userMessage(turn)] }]
      : []
  ));
  return [...blocks, ...orphanUsers]
    .sort((left, right) => left.index - right.index)
    .flatMap((block) => block.messages);
}

function liveSnapshot(state: ProjectChatState | null, connection: LiveConnectionState): ProjectChatSnapshot | null {
  if (!state) {
    if (connection.phase === "empty") return {
      state: "restored",
      stateLabel: "Ready",
      stateTone: "good",
      notice: "The local Node and agent runtime are ready. Create or select a chat to begin.",
      phase: "No chat selected",
      freshness: "Live",
      canStop: false,
      messages: [],
    };
    return connection.phase !== "opening" ? {
      state: "stale",
      stateLabel: connection.phase === "resyncing" ? "Refreshing" : "Unavailable",
      stateTone: connection.phase === "resyncing" ? "active" : "warning",
      notice: connection.phase === "resyncing"
        ? "The local conversation is refreshing after an update gap."
        : `The local conversation is unavailable${connection.message ? ` (${connection.message})` : ""}.`,
      phase: connection.message ?? "Opening the local conversation",
      freshness: "Retrying",
      canStop: false,
      messages: [],
    } : null;
  }
  const active = state.session.activeTurnId !== null;
  const lastAgent = [...state.turns].reverse().find((turn) => turn.role.endsWith("_agent"));
  const terminal = lastAgent?.state ?? "";
  const view = active
    ? ["streaming", "Working", "active", "The project agent is responding. You can stop this turn."] as const
    : terminal.endsWith("_timed_out")
    ? ["timed-out", "Timed out", "danger", "The runtime exceeded its deadline. The partial response is preserved."] as const
    : terminal.endsWith("_cancelled")
      ? ["stopped", "Stopped", "warning", "Generation stopped. The partial response is preserved."] as const
      : terminal.endsWith("_failed")
        ? ["stale", "Failed", "danger", "The runtime could not complete this turn."] as const
        : ["restored", "Ready", "good", "Conversation is synchronized with the local Node."] as const;
  const messages = liveMessages(state.turns);
  if (connection.phase !== "live") {
    return {
      state: connection.phase === "opening" ? "loading" : "resyncing",
      stateLabel: connection.phase === "opening" ? "Opening" : connection.phase === "resyncing" ? "Refreshing" : "Unavailable",
      stateTone: connection.phase === "error" ? "warning" : "active",
      notice: connection.phase === "opening"
        ? "Opening the selected local conversation."
        : connection.phase === "resyncing"
          ? "Messages are read-only while Dennett refreshes an update gap."
          : "Messages are read-only until the local conversation reconnects.",
      phase: connection.message ?? "Waiting for a fresh snapshot",
      freshness: state.turns.length > 0 ? "Read only" : "Retrying",
      canStop: false,
      messages,
    };
  }
  return {
    state: view[0],
    stateLabel: view[1],
    stateTone: view[2],
    notice: view[3],
    phase: active ? "Streaming response" : "Ready",
    freshness: "Live",
    canStop: active,
    messages,
  };
}

function ArtifactViewer({ surface, onClose }: { surface: WorkspaceSurface; onClose: () => void }): React.JSX.Element | null {
  if (surface.kind === "chat") return null;

  return (
    <section className="artifact-surface" aria-label={`${surface.title} viewer`}>
      <header className="artifact-header">
        <div>
          {surface.kind === "browser" ? <Globe size={17} aria-hidden="true" /> : <FileText size={17} aria-hidden="true" />}
          <span><strong>{surface.title}</strong><small>{surface.subtitle}</small></span>
        </div>
        <IconButton label="Close viewer and return to chat" icon={X} onClick={onClose} />
      </header>
      {surface.kind === "browser" ? (
        <div className="browser-viewer">
          <div className="browser-address"><Globe size={14} aria-hidden="true" /><span>127.0.0.1:5173</span><span>Local fixture</span></div>
          <iframe title="Dennett local preview" srcDoc={browserPreviewDocument} sandbox="" />
        </div>
      ) : (
        <article className="document-viewer">
          <span className="document-kicker">{surface.kind === "report" ? "RESULT" : "SOURCE"}</span>
          <h2>{surface.title}</h2>
          <p>{surface.kind === "report"
            ? "The owner checkpoint now uses one monochrome glass shell, a project-first conversation list, compact runtime controls and a resource workspace that can open artifacts in the center."
            : "This source is attached as read-only context. Later milestones can open the canonical file with provenance, revision and permission metadata."}</p>
          <div className="document-block">
            <span>{surface.subtitle}</span>
            <pre>{surface.kind === "report"
              ? "state: owner-review\npalette: grayscale\nauthority: fixture\nexternal_effects: none"
              : "mode: read-only\nsource: repository\nrevision: current checkpoint\nprovider_types: adapter-only"}</pre>
          </div>
        </article>
      )}
    </section>
  );
}

export function App(): React.JSX.Element {
  const nativeShell = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
  const [fixturesBySession, setFixturesBySession] = React.useState<Record<string, FixtureId>>({ screen: "streaming" });
  const [snapshot, setSnapshot] = React.useState<ProjectChatSnapshot | null>(null);
  const [sidebarOpen, setSidebarOpen] = React.useState(true);
  const [resourcesOpen, setResourcesOpen] = React.useState(!nativeShell);
  const [selectedSession, setSelectedSession] = React.useState("screen");
  const [localProjects, setLocalProjects] = React.useState<ProjectGroup[]>([]);
  const [localSessions, setLocalSessions] = React.useState<Array<SessionItem & { projectId?: string }>>([]);
  const [newProjectMenuOpen, setNewProjectMenuOpen] = React.useState(false);
  const [surface, setSurface] = React.useState<WorkspaceSurface>({ kind: "chat" });
  const [draftsBySession, setDraftsBySession] = React.useState<Record<string, string>>({});
  const [draftCommandsBySession, setDraftCommandsBySession] = React.useState<Record<string, string>>({});
  const [localMessages, setLocalMessages] = React.useState<Record<string, ChatMessage[]>>({});
  const [announcement, setAnnouncement] = React.useState("Project Chat opened");
  const [commandOpen, setCommandOpen] = React.useState(false);
  const [commandQuery, setCommandQuery] = React.useState("");
  const [composerPopover, setComposerPopover] = React.useState<ComposerPopover>(null);
  const [expandedRuntimeControl, setExpandedRuntimeControl] = React.useState<string | null>(null);
  const [accessMode, setAccessMode] = React.useState<AccessMode>("full");
  const [reasoning, setReasoning] = React.useState<ReasoningLevel>("high");
  const [runtimeSelections, setRuntimeSelections] = React.useState<Record<string, string>>({});
  const [planPinned, setPlanPinned] = React.useState(false);
  const [planHovered, setPlanHovered] = React.useState(false);
  const [liveSystem, setLiveSystem] = React.useState<SystemSnapshot | null>(null);
  const [liveChat, setLiveChat] = React.useState<ProjectChatState | null>(null);
  const [liveConnection, setLiveConnection] = React.useState<LiveConnectionState>({
    phase: nativeShell ? "opening" : "live",
    message: null,
    targetSessionId: null,
    lastEventAtUnixMs: nativeShell ? null : Date.now(),
  });
  const [systemConnection, setSystemConnection] = React.useState<SystemConnectionState>({
    phase: nativeShell ? "opening" : "live",
    message: null,
  });
  const liveConnectionRef = React.useRef(liveConnection);
  liveConnectionRef.current = liveConnection;
  const [retryingTurnId, setRetryingTurnId] = React.useState<string | null>(null);
  const [creatingChat, setCreatingChat] = React.useState(false);
  const [sendingSessions, setSendingSessions] = React.useState<ReadonlySet<string>>(new Set());
  const [reconnectAttempt, setReconnectAttempt] = React.useState(0);
  const [systemReconnectAttempt, setSystemReconnectAttempt] = React.useState(0);
  const commandRef = React.useRef<HTMLInputElement>(null);
  const commandDialogRef = React.useRef<HTMLDivElement>(null);
  const returnFocusRef = React.useRef<HTMLElement | null>(null);
  const commandWasOpenRef = React.useRef(false);
  const composerRef = React.useRef<HTMLTextAreaElement>(null);
  const composerShellRef = React.useRef<HTMLDivElement>(null);
  const conversationRef = React.useRef<HTMLElement>(null);
  const accessTriggerRef = React.useRef<HTMLButtonElement>(null);
  const accessFirstRef = React.useRef<HTMLButtonElement>(null);
  const runtimeTriggerRef = React.useRef<HTMLButtonElement>(null);
  const runtimeFirstRef = React.useRef<HTMLElement | null>(null);
  const runtimeControlTriggerRefs = React.useRef<Record<string, HTMLButtonElement | null>>({});
  const contextTriggerRef = React.useRef<HTMLButtonElement>(null);
  const contextDialogRef = React.useRef<HTMLDivElement>(null);
  const pluginsTriggerRef = React.useRef<HTMLButtonElement>(null);
  const pluginsDialogRef = React.useRef<HTMLDivElement>(null);
  const projectMenuRef = React.useRef<HTMLDivElement>(null);
  const projectMenuTriggerRef = React.useRef<HTMLButtonElement>(null);
  const projectMenuFirstRef = React.useRef<HTMLButtonElement>(null);
  const nextLocalProjectIdRef = React.useRef(1);
  const nextLocalSessionIdRef = React.useRef(1);
  const nextLocalMessageIdRef = React.useRef(1);
  const liveClientRef = React.useRef<TauriProjectChatClient | null>(null);
  const systemClientRef = React.useRef<TauriSystemBridgeClient | null>(null);
  const draftRevisionsRef = React.useRef<Record<string, number>>({});
  const currentDraftCommandsRef = React.useRef<Record<string, string>>({});
  const persistedDraftCommandsRef = React.useRef<Record<string, string>>({});
  const draftSaveQueuesRef = React.useRef<Record<string, Promise<boolean>>>({});
  const pendingSentDraftsRef = React.useRef<Record<string, { text: string; nextCommandId: string }>>({});
  const sendingSessionsRef = React.useRef(new Set<string>());
  const creatingChatRef = React.useRef(false);
  const navigationSequenceRef = React.useRef(0);
  const retryCommandsRef = React.useRef<Record<string, string>>({});
  const activityAnnouncementRef = React.useRef<string | null>(null);
  const allowWindowCloseRef = React.useRef(false);
  const closingWindowRef = React.useRef(false);
  const persistCurrentDraftRef = React.useRef<() => Promise<boolean>>(async () => true);
  const draftsBySessionRef = React.useRef(draftsBySession);
  draftsBySessionRef.current = draftsBySession;
  if (nativeShell && !liveClientRef.current) liveClientRef.current = new TauriProjectChatClient();
  if (nativeShell && !systemClientRef.current) systemClientRef.current = new TauriSystemBridgeClient();
  const authoritativeProjectGroups: ProjectGroup[] = liveSystem?.projects.map((project) => ({
    id: project.projectId,
    title: project.displayName,
    sessions: liveSystem.recentSessions
      .filter((session) => session.projectId === project.projectId)
      .map((session) => ({ id: session.sessionId, title: session.title, meta: session.activeTurnId ? "Working" : formatClock(session.lastActivityAtUnixMs) })),
  })) ?? [];
  const visibleProjectGroups = (nativeShell ? authoritativeProjectGroups : [...projectGroups, ...localProjects]).map((project) => ({
    ...project,
    sessions: nativeShell ? project.sessions : [...project.sessions, ...localSessions.filter((session) => session.projectId === project.id)],
  }));
  const standaloneSessions = nativeShell
    ? liveSystem?.recentSessions
      .filter((session) => !session.projectId)
      .map((session) => ({
        id: session.sessionId,
        title: session.title,
        meta: session.activeTurnId ? "Working" : formatClock(session.lastActivityAtUnixMs),
      })) ?? []
    : [...localSessions.filter((session) => !session.projectId), ...recentChats];
  const allSessions = [...visibleProjectGroups.flatMap((project) => project.sessions), ...standaloneSessions];
  const selectedTitle = allSessions.find((session) => session.id === selectedSession)?.title
    ?? allSessions[0]?.title
    ?? (nativeShell && liveSystem ? "No chat selected" : "Opening chat");
  const selectedProject = visibleProjectGroups.find((project) => project.sessions.some((session) => session.id === selectedSession));
  const selectedLocalMessages = localMessages[selectedSession] ?? [];
  const fixture = fixturesBySession[selectedSession] ?? "streaming";
  const draft = draftsBySession[selectedSession] ?? "";
  const draftCommand = draftCommandsBySession[selectedSession];
  const resourcesAvailable = !nativeShell;
  const runtimeAdapterId = liveSystem?.runtime?.adapterId ?? null;
  const steeringMode = liveSystem?.runtime?.steering ?? "unsupported";
  const advertisedRuntimeControls = nativeShell ? liveSystem?.runtime?.controls ?? [] : [];
  const effectiveRuntimeSelections = reconcileRuntimeSelections(advertisedRuntimeControls, runtimeSelections);
  const accessControl = advertisedRuntimeControls.find((control) => control.id === ACCESS_CONTROL_ID) ?? null;
  const runtimeControls = advertisedRuntimeControls.filter((control) => control.id !== ACCESS_CONTROL_ID);
  const selectedRuntimeControls: RuntimeControlSelection[] = advertisedRuntimeControls.flatMap((control) => {
    const choiceId = effectiveRuntimeSelections[control.id];
    return choiceId ? [{ controlId: control.id, choiceId }] : [];
  });
  const selectedAccessChoice = accessControl?.choices.find(
    (choice) => choice.id === effectiveRuntimeSelections[accessControl.id],
  ) ?? null;
  const runtimeSelectionSummary = runtimeControls
    .map((control) => control.choices.find((choice) => choice.id === effectiveRuntimeSelections[control.id])?.label)
    .filter((label): label is string => Boolean(label))
    .join(" · ");
  const runtimeName = !nativeShell
    ? "Codex"
    : runtimeAdapterId === "openai.codex.sdk"
      ? "Codex"
      : runtimeAdapterId === "dennett.fake"
        ? "Test runtime"
        : runtimeAdapterId ?? "Runtime";
  const runtimeModeLabel = nativeShell
    ? liveSystem?.runtime?.runtimeKind === "native_agent"
      ? runtimeSelectionSummary || "Native agent"
      : liveSystem?.runtime?.runtimeKind === "generic_loop"
        ? "Agent loop"
        : null
    : reasoning === "high" ? "High" : "Medium";
  const runtimeSourceLabel = nativeShell ? (runtimeAdapterId ? runtimeName : "Unavailable") : "Codex SDK";
  const nativeConversationReady = !nativeShell || (
    liveConnection.phase === "live"
    && liveChat?.session.sessionId === selectedSession
  );
  const systemConnected = liveSystem !== null;
  const bootstrapSessionId = liveSystem?.activeSessionId ?? null;
  const runtimeControlsLocked = nativeShell && Boolean(liveChat?.session.activeTurnId);

  React.useEffect(() => {
    if (!nativeShell) return;
    setRuntimeSelections((selections) => reconcileRuntimeSelections(advertisedRuntimeControls, selections));
  }, [liveSystem?.runtime, nativeShell]);

  React.useEffect(() => {
    if (composerPopover !== "runtime") setExpandedRuntimeControl(null);
  }, [composerPopover]);

  React.useEffect(() => {
    if (runtimeControlsLocked && (composerPopover === "runtime" || composerPopover === "access")) {
      setComposerPopover(null);
    }
  }, [composerPopover, runtimeControlsLocked]);

  const setSelectedFixture = React.useCallback((nextFixture: FixtureId) => {
    setFixturesBySession((fixtures) => ({ ...fixtures, [selectedSession]: nextFixture }));
  }, [selectedSession]);

  const setDraft = React.useCallback((nextDraft: string) => {
    const pending = pendingSentDraftsRef.current[selectedSession];
    if (pending && pending.text !== nextDraft) delete pendingSentDraftsRef.current[selectedSession];
    setDraftsBySession((drafts) => ({ ...drafts, [selectedSession]: nextDraft }));
  }, [selectedSession]);

  const openCommandCenter = React.useCallback(() => {
    setComposerPopover(null);
    setCommandQuery("");
    setCommandOpen((open) => {
      if (!open && document.activeElement instanceof HTMLElement) returnFocusRef.current = document.activeElement;
      return true;
    });
  }, []);
  const closeCommandCenter = React.useCallback(() => {
    setCommandOpen(false);
    setCommandQuery("");
  }, []);

  React.useEffect(() => {
    document.documentElement.classList.toggle("native-shell", nativeShell);
    document.documentElement.classList.toggle("native-mica-unavailable", nativeShell);
    let current = true;
    if (nativeShell) {
      void import("@tauri-apps/api/core")
        .then(({ invoke }) => invoke<boolean>("native_mica_available"))
        .then((available) => {
          if (current && available) document.documentElement.classList.remove("native-mica-unavailable");
        })
        .catch(() => undefined);
    }
    return () => {
      current = false;
      document.documentElement.classList.remove("native-shell", "native-mica-unavailable");
    };
  }, [nativeShell]);

  React.useEffect(() => {
    if (!nativeShell) return;
    let current = true;
    let handle: Awaited<ReturnType<TauriSystemBridgeClient["openSystemWatch"]>> | null = null;
    let retryTimer: ReturnType<typeof setTimeout> | undefined;
    let bootstrapped = false;
    let currentSystem: SystemSnapshot | null = null;
    const pendingEvents: SystemEvent[] = [];
    const clearScheduledReconnect = () => {
      if (!retryTimer) return;
      clearTimeout(retryTimer);
      retryTimer = undefined;
    };
    const scheduleReconnect = (delay: number) => {
      if (retryTimer) return;
      retryTimer = setTimeout(() => setSystemReconnectAttempt((attempt) => attempt + 1), delay);
    };
    setSystemConnection({ phase: "opening", message: null });
    const applyEvent = (event: SystemEvent) => {
      if (!current) return;
      if (event.kind === "phase") {
        if (event.phase === "reconnecting") {
          setSystemConnection({ phase: "opening", message: "desktop.system_watch_reconnecting" });
        }
        return;
      }
      if (event.kind === "error") {
        setSystemConnection({
          phase: "error",
          message: event.error.messageKey,
        });
        if (event.error.retryable && !event.error.userActionRequired) scheduleReconnect(750);
        return;
      }
      if (event.kind === "resyncRequired") {
        setSystemConnection({ phase: "opening", message: "desktop.system_watch_resyncing" });
        return;
      }
      if (event.kind === "heartbeat") {
        if (currentSystem && currentSystem.revision === event.currentRevision) {
          clearScheduledReconnect();
          setSystemConnection({ phase: "live", message: null });
        }
        return;
      }
      try {
        if (!currentSystem) {
          if (event.kind !== "snapshot") throw new Error("System delta before snapshot");
          currentSystem = event.snapshot;
        } else {
          currentSystem = applySystemEvent(currentSystem, event);
        }
        setLiveSystem(currentSystem);
        clearScheduledReconnect();
        setSystemConnection({ phase: "live", message: null });
      } catch {
        currentSystem = null;
        setLiveSystem(null);
        setSystemConnection({ phase: "opening", message: "desktop.system_watch_resyncing" });
        scheduleReconnect(80);
      }
    };
    const onEvent = (event: SystemEvent) => bootstrapped ? applyEvent(event) : pendingEvents.push(event);
    void systemClientRef.current?.openSystemWatch(onEvent).then((opened) => {
      if (!current) {
        void opened.close();
        return;
      }
      handle = opened;
      currentSystem = opened.opened.snapshot;
      setLiveSystem(currentSystem);
      setSystemConnection({ phase: "live", message: null });
      for (const event of pendingEvents.splice(0)) applyEvent(event);
      bootstrapped = true;
    }).catch((error: unknown) => {
      if (!current) return;
      const failure = bridgeFailure(error, "desktop.system_watch_unavailable");
      setLiveSystem(null);
      setSystemConnection({
        phase: "error",
        message: failure.message,
      });
      if (failure.retryable) scheduleReconnect(750);
    });
    return () => {
      current = false;
      if (retryTimer) clearTimeout(retryTimer);
      if (handle) void handle.close();
    };
  }, [nativeShell, systemReconnectAttempt]);

  React.useEffect(() => {
    if (!nativeShell || !systemConnected) return;
    let current = true;
    let handle: Awaited<ReturnType<TauriProjectChatClient["open"]>> | null = null;
    let retryTimer: ReturnType<typeof setTimeout> | undefined;
    const client = liveClientRef.current;
    if (!client) return;
    const requestedSession = /^[0-9a-f]{8}-[0-9a-f-]{27}$/i.test(selectedSession) ? selectedSession : bootstrapSessionId;
    if (!requestedSession) {
      setLiveChat(null);
      setLiveConnection({
        phase: "empty",
        message: "Create or select a chat",
        targetSessionId: null,
        lastEventAtUnixMs: Date.now(),
      });
      setAnnouncement("Local Node ready. Create or select a chat to begin.");
      return;
    }
    setLiveChat((snapshot) => requestedSession && snapshot?.session.sessionId !== requestedSession ? null : snapshot);
    setLiveConnection((connection) => ({
      phase: connection.phase === "resyncing" && connection.targetSessionId === requestedSession
        ? "resyncing"
        : "opening",
      message: connection.phase === "resyncing" && connection.targetSessionId === requestedSession
        ? connection.message
        : "Opening the selected chat",
      targetSessionId: requestedSession,
      lastEventAtUnixMs: null,
    }));
    let bootstrapped = false;
    let currentSession: ProjectChatState | null = null;
    const pendingEvents: ProjectChatEvent[] = [];
    const applyLiveEvent = (event: ProjectChatEvent) => {
      if (!current) return;
      if (event.kind === "delta" || event.kind === "snapshot") {
        try {
          if (event.kind === "snapshot") currentSession = event.snapshot;
          else if (currentSession) currentSession = applyProjectChatEvent(currentSession, event);
          else throw new Error("delta before snapshot");
          setLiveChat(currentSession);
          setLiveConnection({
            phase: "live",
            message: null,
            targetSessionId: requestedSession,
            lastEventAtUnixMs: Date.now(),
          });
        } catch {
          setLiveConnection({
            phase: "resyncing",
            message: "Session state changed; refreshing the snapshot",
            targetSessionId: requestedSession,
            lastEventAtUnixMs: null,
          });
          retryTimer = setTimeout(() => setReconnectAttempt((attempt) => attempt + 1), 80);
        }
      }
      if (event.kind === "heartbeat") {
        if (!currentSession || currentSession.session.revision !== event.currentRevision) {
          setLiveConnection({
            phase: "resyncing",
            message: "The local conversation has a newer revision; refreshing the snapshot",
            targetSessionId: requestedSession,
            lastEventAtUnixMs: null,
          });
          retryTimer = setTimeout(() => setReconnectAttempt((attempt) => attempt + 1), 80);
          return;
        }
        setLiveConnection({
          phase: "live",
          message: null,
          targetSessionId: requestedSession,
          lastEventAtUnixMs: Date.now(),
        });
      }
      if (event.kind === "resyncRequired") {
        setLiveConnection({
          phase: "resyncing",
          message: "Refreshing after an update gap",
          targetSessionId: requestedSession,
          lastEventAtUnixMs: null,
        });
        retryTimer = setTimeout(() => setReconnectAttempt((attempt) => attempt + 1), 80);
      }
      if (event.kind === "error") {
        setLiveConnection({
          phase: "error",
          message: event.error.messageKey,
          targetSessionId: requestedSession,
          lastEventAtUnixMs: null,
        });
        if (event.error.retryable) retryTimer = setTimeout(() => setReconnectAttempt((attempt) => attempt + 1), 500);
      }
    };
    const onEvent = (event: ProjectChatEvent) => {
      if (!bootstrapped) {
        pendingEvents.push(event);
        return;
      }
      applyLiveEvent(event);
    };
    void client.open(onEvent, requestedSession).then((opened) => {
      if (!current) {
        void opened.close();
        return;
      }
      handle = opened;
      setLiveSystem((system) => !system || BigInt(opened.opened.system.revision) >= BigInt(system.revision)
        ? opened.opened.system
        : system);
      let initialSession = opened.opened.session;
      const deferredEvents: ProjectChatEvent[] = [];
      for (const event of pendingEvents.splice(0)) {
        if (event.kind === "snapshot") {
          initialSession = event.snapshot;
        } else if (event.kind === "delta") {
          try {
            initialSession = applyProjectChatEvent(initialSession, event);
          } catch {
            deferredEvents.push({
              kind: "resyncRequired",
              subscriptionId: event.subscriptionId,
              cursor: event.cursor,
              reason: "bootstrap_revision_gap",
              currentRevision: event.newRevision,
            });
          }
        } else {
          deferredEvents.push(event);
        }
      }
      currentSession = initialSession;
      setLiveChat(currentSession);
      setLiveConnection({
        phase: "live",
        message: null,
        targetSessionId: initialSession.session.sessionId,
        lastEventAtUnixMs: Date.now(),
      });
      bootstrapped = true;
      for (const event of deferredEvents) applyLiveEvent(event);
      const openedSessionId = opened.opened.session.session.sessionId;
      setDraftsBySession((drafts) => Object.hasOwn(drafts, openedSessionId)
        ? drafts
        : { ...drafts, [openedSessionId]: opened.opened.draft?.text ?? "" });
      const restoredDraftCommand = opened.opened.draft?.commandId ?? crypto.randomUUID();
      if (!Object.hasOwn(currentDraftCommandsRef.current, openedSessionId)) {
        currentDraftCommandsRef.current[openedSessionId] = restoredDraftCommand;
      }
      setDraftCommandsBySession((commands) => Object.hasOwn(commands, openedSessionId)
        ? commands
        : { ...commands, [openedSessionId]: restoredDraftCommand });
      if (!Object.hasOwn(draftRevisionsRef.current, openedSessionId)) {
        draftRevisionsRef.current[openedSessionId] = Number(opened.opened.draft?.revision ?? "0");
      }
      if (opened.opened.draft) {
        persistedDraftCommandsRef.current[openedSessionId] = opened.opened.draft.commandId;
      }
      setSelectedSession(openedSessionId);
      setAnnouncement("Project Chat connected to the local Node.");
    }).catch((error: unknown) => {
      if (!current) return;
      setLiveConnection({
        phase: "error",
        message: error instanceof Error ? error.message : "Local Node unavailable",
        targetSessionId: requestedSession,
        lastEventAtUnixMs: null,
      });
      retryTimer = setTimeout(() => setReconnectAttempt((attempt) => attempt + 1), 750);
    });
    return () => {
      current = false;
      if (retryTimer) clearTimeout(retryTimer);
      if (handle) void handle.close();
    };
  }, [bootstrapSessionId, nativeShell, reconnectAttempt, selectedSession, systemConnected]);

  React.useEffect(() => {
    if (!nativeShell || !liveChat) return;
    const turn = [...liveChat.turns].reverse().find((candidate) => candidate.role.endsWith("_agent"));
    if (!turn) return;
    const terminal = turn.state.match(/_(completed|cancelled|timed_out|failed)$/)?.[1] ?? null;
    const activity = turn.activities.at(-1);
    const key = terminal
      ? `${turn.turnId}:terminal:${terminal}`
      : activity
        ? `${turn.turnId}:${activity.activityId}:${activity.status}`
        : `${turn.turnId}:${turn.state}`;
    if (activityAnnouncementRef.current === key) return;
    activityAnnouncementRef.current = key;
    if (terminal === "completed") setAnnouncement("Agent response completed.");
    else if (terminal === "cancelled") setAnnouncement("Agent response stopped. Partial output was preserved.");
    else if (terminal === "timed_out") setAnnouncement("Agent response timed out. Partial output was preserved.");
    else if (terminal === "failed") setAnnouncement("Agent response failed.");
    else if (activity) setAnnouncement(activityLabel(activity.phase, activity.status));
    else setAnnouncement("Agent started working.");
  }, [liveChat, nativeShell]);

  React.useEffect(() => {
    if (
      !nativeShell
      || liveConnection.phase !== "live"
      || liveConnection.lastEventAtUnixMs === null
    ) return;
    const expectedFreshness = liveConnection.lastEventAtUnixMs;
    const remaining = Math.max(
      0,
      LIVE_WATCH_STALE_AFTER_MS - (Date.now() - expectedFreshness),
    );
    const timer = window.setTimeout(() => {
      const current = liveConnectionRef.current;
      if (
        current.phase !== "live"
        || current.lastEventAtUnixMs !== expectedFreshness
      ) return;
      setLiveConnection({
        phase: "resyncing",
        message: "The local conversation stopped reporting freshness; reconnecting",
        targetSessionId: current.targetSessionId,
        lastEventAtUnixMs: null,
      });
      setReconnectAttempt((attempt) => attempt + 1);
    }, remaining);
    return () => window.clearTimeout(timer);
  }, [liveConnection.lastEventAtUnixMs, liveConnection.phase, nativeShell]);

  const persistCurrentDraft = React.useCallback(async (): Promise<boolean> => {
    const client = liveClientRef.current;
    if (!nativeShell || !client || !liveChat || !draftCommand) return true;
    const projectId = liveChat.session.projectId;
    const sessionId = liveChat.session.sessionId;
    const text = draft;
    const pendingSentDraft = pendingSentDraftsRef.current[sessionId];
    if (
      pendingSentDraft?.nextCommandId === draftCommand
      && pendingSentDraft.text === text
    ) return true;
    const revision = (draftRevisionsRef.current[sessionId] ?? 0) + 1;
    draftRevisionsRef.current[sessionId] = revision;
    const operation = async (): Promise<boolean> => {
      try {
        if (currentDraftCommandsRef.current[sessionId] !== draftCommand) return true;
        if (text.length === 0) {
          if (persistedDraftCommandsRef.current[sessionId] !== draftCommand) return true;
          await client.discardDraft({ projectId, sessionId, commandId: draftCommand });
          if (currentDraftCommandsRef.current[sessionId] !== draftCommand) return true;
          delete persistedDraftCommandsRef.current[sessionId];
          draftRevisionsRef.current[sessionId] = 0;
          const nextCommand = crypto.randomUUID();
          currentDraftCommandsRef.current[sessionId] = nextCommand;
          setDraftCommandsBySession((commands) => commands[sessionId] === draftCommand
            ? { ...commands, [sessionId]: nextCommand }
            : commands);
          return true;
        }
        persistedDraftCommandsRef.current[sessionId] = draftCommand;
        const receipt = await client.saveDraft({
          projectId,
          sessionId,
          commandId: draftCommand,
          text,
          revision,
        });
        if (currentDraftCommandsRef.current[sessionId] !== draftCommand) return true;
        if (receipt.state === "composer_draft_write_state_already_accepted") {
          delete persistedDraftCommandsRef.current[sessionId];
          draftRevisionsRef.current[sessionId] = 0;
          const nextCommand = crypto.randomUUID();
          currentDraftCommandsRef.current[sessionId] = nextCommand;
          setDraftsBySession((drafts) => ({ ...drafts, [sessionId]: "" }));
          setDraftCommandsBySession((commands) => ({
            ...commands,
            [sessionId]: nextCommand,
          }));
        } else if (receipt.state !== "composer_draft_write_state_saved") return false;
        return true;
      } catch (error) {
        setAnnouncement(error instanceof Error ? error.message : "The draft could not be saved.");
        return false;
      }
    };
    const previous = draftSaveQueuesRef.current[sessionId] ?? Promise.resolve(true);
    const queued = previous.catch(() => false).then(operation);
    draftSaveQueuesRef.current[sessionId] = queued;
    const succeeded = await queued;
    if (draftSaveQueuesRef.current[sessionId] === queued) {
      delete draftSaveQueuesRef.current[sessionId];
    }
    return succeeded;
  }, [draft, draftCommand, liveChat, nativeShell]);
  persistCurrentDraftRef.current = persistCurrentDraft;

  const requestNativeWindowClose = React.useCallback(async () => {
    if (!nativeShell || closingWindowRef.current) return;
    closingWindowRef.current = true;
    const saved = await Promise.race([
      persistCurrentDraftRef.current(),
      new Promise<boolean>((resolve) => window.setTimeout(() => resolve(false), 3_000)),
    ]);
    if (!saved) {
      closingWindowRef.current = false;
      setAnnouncement("The draft was not saved. The window remains open.");
      return;
    }
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      allowWindowCloseRef.current = true;
      await getCurrentWindow().close();
    } catch (error) {
      allowWindowCloseRef.current = false;
      closingWindowRef.current = false;
      setAnnouncement(error instanceof Error ? error.message : "The window could not be closed.");
    }
  }, [nativeShell]);

  React.useEffect(() => {
    if (!nativeShell) return;
    let current = true;
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/window")
      .then(({ getCurrentWindow }) => getCurrentWindow().onCloseRequested((event) => {
        if (allowWindowCloseRef.current) return;
        event.preventDefault();
        void requestNativeWindowClose();
      }))
      .then((stopListening) => {
        if (current) unlisten = stopListening;
        else stopListening();
      })
      .catch(() => undefined);
    return () => {
      current = false;
      unlisten?.();
    };
  }, [nativeShell, requestNativeWindowClose]);

  React.useEffect(() => {
    if (!nativeShell || !liveChat || liveChat.session.sessionId !== selectedSession) return;
    if (!draftCommand) {
      draftRevisionsRef.current[selectedSession] = 0;
      const nextCommand = crypto.randomUUID();
      currentDraftCommandsRef.current[selectedSession] = nextCommand;
      setDraftCommandsBySession((commands) => ({
        ...commands,
        [selectedSession]: nextCommand,
      }));
      return;
    }
    const timer = setTimeout(() => void persistCurrentDraft(), 350);
    return () => clearTimeout(timer);
  }, [draft, draftCommand, liveChat, nativeShell, persistCurrentDraft, selectedSession]);

  React.useEffect(() => {
    if (nativeShell) return;
    let current = true;
    setSnapshot(null);
    const client = createFixtureDennettClient(fixture);
    client.readProjectChat({ projectId: selectedProject?.id ?? "standalone", sessionId: selectedSession }).then((next) => {
      if (!current) return;
      setSnapshot(next);
      setAnnouncement(`${next.stateLabel}. ${next.phase}.`);
    });
    return () => { current = false; };
  }, [fixture, nativeShell, selectedProject?.id, selectedSession]);

  React.useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        openCommandCenter();
      }
      if (event.key === "Escape") {
        closeCommandCenter();
        const projectMenuWasOpen = newProjectMenuOpen;
        const returnTarget = composerPopover === "access"
          ? accessTriggerRef.current
          : composerPopover === "runtime"
            ? runtimeTriggerRef.current
            : composerPopover === "context"
              ? contextTriggerRef.current
              : composerPopover === "plugins"
                ? pluginsTriggerRef.current
                : null;
        setComposerPopover(null);
        setNewProjectMenuOpen(false);
        requestAnimationFrame(() => {
          if (returnTarget) returnTarget.focus();
          else if (projectMenuWasOpen) projectMenuTriggerRef.current?.focus();
        });
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [closeCommandCenter, composerPopover, newProjectMenuOpen, openCommandCenter]);

  React.useEffect(() => {
    if (!composerPopover) return;
    const closeOnOutsidePointer = (event: PointerEvent) => {
      if (!composerShellRef.current?.contains(event.target as Node)) setComposerPopover(null);
    };
    document.addEventListener("pointerdown", closeOnOutsidePointer);
    return () => document.removeEventListener("pointerdown", closeOnOutsidePointer);
  }, [composerPopover]);

  React.useEffect(() => {
    if (composerPopover === "context") contextDialogRef.current?.focus();
    if (composerPopover === "plugins") pluginsDialogRef.current?.focus();
    if (composerPopover === "access") accessFirstRef.current?.focus();
    if (composerPopover === "runtime") runtimeFirstRef.current?.focus();
  }, [composerPopover]);

  React.useEffect(() => {
    if (!newProjectMenuOpen) return;
    projectMenuFirstRef.current?.focus();
    const closeOnOutsidePointer = (event: PointerEvent) => {
      if (!projectMenuRef.current?.contains(event.target as Node)) setNewProjectMenuOpen(false);
    };
    document.addEventListener("pointerdown", closeOnOutsidePointer);
    return () => document.removeEventListener("pointerdown", closeOnOutsidePointer);
  }, [newProjectMenuOpen]);

  React.useEffect(() => {
    if (commandOpen) commandRef.current?.focus();
    else if (commandWasOpenRef.current) returnFocusRef.current?.focus();
    commandWasOpenRef.current = commandOpen;
  }, [commandOpen]);

  React.useEffect(() => {
    const conversation = conversationRef.current;
    if (conversation && surface.kind === "chat") conversation.scrollTop = conversation.scrollHeight;
  }, [liveChat, snapshot, selectedLocalMessages.length, surface.kind]);

  const selectSession = async (sessionId: string) => {
    if (sessionId === selectedSession) return;
    const navigationSequence = ++navigationSequenceRef.current;
    const saved = await persistCurrentDraft();
    if (navigationSequence !== navigationSequenceRef.current) return;
    if (!saved) {
      setAnnouncement("The current draft was not saved. This chat remains open.");
      return;
    }
    if (nativeShell && liveChat?.session.sessionId !== sessionId) {
      setLiveChat(null);
      setLiveConnection({
        phase: "opening",
        message: "Opening the selected chat",
        targetSessionId: sessionId,
        lastEventAtUnixMs: null,
      });
    }
    setSelectedSession(sessionId);
    setSurface({ kind: "chat" });
    setAnnouncement(`Opened ${allSessions.find((item) => item.id === sessionId)?.title ?? "chat"}.`);
  };

  const createProjectPreview = (kind: ProjectCreationKind) => {
    if (nativeShell) {
      setAnnouncement("Project folders arrive in M02.");
      return;
    }
    const projectNumber = nextLocalProjectIdRef.current++;
    const project: ProjectGroup = {
      id: `local-project-${projectNumber}`,
      title: kind === "empty" ? `Untitled project ${projectNumber}` : `Existing folder ${projectNumber}`,
      sessions: [],
    };
    setLocalProjects((projects) => [...projects, project]);
    setNewProjectMenuOpen(false);
    setAnnouncement(kind === "empty"
      ? "Empty project added to this local preview. No folder was created."
      : "Existing-folder project added to this local preview. No folder picker was opened.");
    requestAnimationFrame(() => projectMenuTriggerRef.current?.focus());
  };

  const startNewChat = async (projectId?: string) => {
    if (creatingChatRef.current) return;
    creatingChatRef.current = true;
    setCreatingChat(true);
    try {
      if (nativeShell && liveClientRef.current) {
        const navigationSequence = ++navigationSequenceRef.current;
        try {
          if (!(await persistCurrentDraft())) {
            setAnnouncement("The current draft was not saved. A new chat was not created.");
            return;
          }
          if (navigationSequence !== navigationSequenceRef.current) return;
          const created = await liveClientRef.current.createChat(projectId ?? null);
          if (navigationSequence !== navigationSequenceRef.current) {
            setReconnectAttempt((attempt) => attempt + 1);
            return;
          }
          setLiveChat(null);
          setLiveConnection({
            phase: "opening",
            message: "Opening the new chat",
            targetSessionId: created.sessionId,
            lastEventAtUnixMs: null,
          });
          setSelectedSession(created.sessionId);
          setAnnouncement(projectId ? "New project chat created." : "New standalone chat created.");
          requestAnimationFrame(() => composerRef.current?.focus());
        } catch (error) {
          setAnnouncement(error instanceof Error ? error.message : "Could not create the chat.");
        }
        return;
      }
      const sessionId = `local-chat-${nextLocalSessionIdRef.current++}`;
      setLocalSessions((sessions) => [...sessions, { id: sessionId, title: "Untitled chat", meta: formatClock(Date.now()), projectId }]);
      setFixturesBySession((fixtures) => ({ ...fixtures, [sessionId]: "empty" }));
      setDraftsBySession((drafts) => ({ ...drafts, [sessionId]: "" }));
      setSelectedSession(sessionId);
      setSurface({ kind: "chat" });
      setComposerPopover(null);
      setAnnouncement(projectId ? "New project chat preview opened." : "New standalone chat preview opened.");
      requestAnimationFrame(() => composerRef.current?.focus());
    } finally {
      creatingChatRef.current = false;
      setCreatingChat(false);
    }
  };

  const sendDraft = async () => {
    const originalDraft = draft;
    const content = originalDraft.trim();
    if (!content) return;
    if (nativeShell) {
      if (!nativeConversationReady || !liveChat || !liveClientRef.current) {
        setAnnouncement("Wait until the selected chat finishes opening.");
        return;
      }
      const sessionId = liveChat.session.sessionId;
      if (sendingSessionsRef.current.has(sessionId)) return;
      const steering = liveChat.session.activeTurnId !== null;
      if (steering && steeringMode !== "native") {
        setAnnouncement("This agent cannot accept a clarification while it is working.");
        return;
      }
      const sentCommandId = draftCommand ?? crypto.randomUUID();
      const nextCommandId = crypto.randomUUID();
      const priorRevision = draftRevisionsRef.current[sessionId] ?? 0;
      pendingSentDraftsRef.current[sessionId] = { text: originalDraft, nextCommandId };
      currentDraftCommandsRef.current[sessionId] = nextCommandId;
      draftRevisionsRef.current[sessionId] = 0;
      setDraftCommandsBySession((commands) => ({ ...commands, [sessionId]: nextCommandId }));
      sendingSessionsRef.current.add(sessionId);
      setSendingSessions(new Set(sendingSessionsRef.current));
      try {
        await liveClientRef.current.sendTurn({
          projectId: liveChat.session.projectId,
          sessionId,
          revision: liveChat.session.revision,
          text: content,
          // Provider controls configure a new turn. A native clarification is
          // delivered inside the already-running provider turn and therefore
          // cannot truthfully change its model, speed or permission envelope.
          runtimeControls: steering ? [] : selectedRuntimeControls,
          commandId: sentCommandId,
          deliveryMode: steering ? "steer_now" : "new_turn",
          activeTurnId: liveChat.session.activeTurnId,
        });
        if (persistedDraftCommandsRef.current[sessionId] === sentCommandId) {
          delete persistedDraftCommandsRef.current[sessionId];
        }
        if (pendingSentDraftsRef.current[sessionId]?.nextCommandId === nextCommandId) {
          delete pendingSentDraftsRef.current[sessionId];
        }
        if (
          currentDraftCommandsRef.current[sessionId] === nextCommandId
          && draftsBySessionRef.current[sessionId] === originalDraft
        ) {
          setDraftsBySession((drafts) => drafts[sessionId] === originalDraft
            ? { ...drafts, [sessionId]: "" }
            : drafts);
        }
        setAnnouncement(steering
          ? "Clarification accepted; the current run will continue with it."
          : "Message accepted by the local Node.");
      } catch (error) {
        if (
          currentDraftCommandsRef.current[sessionId] === nextCommandId
          && draftsBySessionRef.current[sessionId] === originalDraft
        ) {
          currentDraftCommandsRef.current[sessionId] = sentCommandId;
          draftRevisionsRef.current[sessionId] = priorRevision;
          setDraftCommandsBySession((commands) => commands[sessionId] === nextCommandId
            ? { ...commands, [sessionId]: sentCommandId }
            : commands);
        }
        if (pendingSentDraftsRef.current[sessionId]?.nextCommandId === nextCommandId) {
          delete pendingSentDraftsRef.current[sessionId];
        }
        setAnnouncement(error instanceof Error ? error.message : "The message was not accepted.");
      } finally {
        sendingSessionsRef.current.delete(sessionId);
        setSendingSessions(new Set(sendingSessionsRef.current));
      }
      return;
    }
    setLocalMessages((messagesBySession) => ({
      ...messagesBySession,
      [selectedSession]: [
        ...(messagesBySession[selectedSession] ?? []),
        { id: `local-message-${nextLocalMessageIdRef.current++}`, author: "user", paragraphs: [content], timestamp: formatClock(Date.now()) },
      ],
    }));
    setDraft("");
    setAnnouncement("Draft added to this local preview. No runtime command was sent.");
  };

  const useSuggestion = (prompt: string) => {
    setDraft(prompt);
    composerRef.current?.focus();
    setAnnouncement("Prompt suggestion added to the local draft.");
  };

  const stopGeneration = async () => {
    if (nativeShell) {
      if (!nativeConversationReady || !liveChat || !liveChat.session.activeTurnId || !liveClientRef.current) {
        setAnnouncement("Stop is unavailable until the selected chat is live.");
        return;
      }
      try {
        await liveClientRef.current.cancelTurn({
          projectId: liveChat.session.projectId,
          sessionId: liveChat.session.sessionId,
          turnId: liveChat.session.activeTurnId,
        });
        setAnnouncement(`Stop requested for session "${selectedTitle}".`);
      } catch (error) {
        setAnnouncement(error instanceof Error ? error.message : "Stop was not accepted.");
      }
      return;
    }
    setSelectedFixture("stopped");
    setAnnouncement(`Stop requested for session "${selectedTitle}".`);
  };

  const retryTimedOutTurn = async () => {
    if (!nativeShell || !nativeConversationReady || !liveChat || !liveClientRef.current) return;
    const timedOutTurn = [...liveChat.turns].reverse().find((turn) => turn.role.endsWith("_agent") && turn.state.endsWith("_timed_out")) ?? null;
    const terminalIndex = timedOutTurn ? liveChat.turns.findIndex((turn) => turn.turnId === timedOutTurn.turnId) : -1;
    const userTurn = terminalIndex > 0
      ? [...liveChat.turns.slice(0, terminalIndex)].reverse().find((turn) => turn.role.endsWith("_user"))
      : null;
    if (!timedOutTurn || !userTurn?.text.trim()) {
      setAnnouncement("The timed-out request is unavailable for retry.");
      return;
    }
    const commandId = retryCommandsRef.current[timedOutTurn.turnId] ?? crypto.randomUUID();
    retryCommandsRef.current[timedOutTurn.turnId] = commandId;
    setRetryingTurnId(timedOutTurn.turnId);
    try {
      await liveClientRef.current.sendTurn({
        projectId: liveChat.session.projectId,
        sessionId: liveChat.session.sessionId,
        revision: liveChat.session.revision,
        text: userTurn.text,
        runtimeControls: selectedRuntimeControls,
        commandId,
      });
      setAnnouncement("Retry accepted by the local Node.");
    } catch (error) {
      setAnnouncement(error instanceof Error ? error.message : "The retry was not accepted.");
    } finally {
      setRetryingTurnId(null);
    }
  };

  const openSurface = (next: WorkspaceSurface) => {
    setSurface(next);
    setAnnouncement(`Opened ${next.kind === "chat" ? "chat" : next.title}.`);
  };

  const copyMessage = async (author: ChatMessage["author"], text: string) => {
    try {
      if (nativeShell) {
        const { writeText } = await import("@tauri-apps/plugin-clipboard-manager");
        await writeText(text);
      } else {
        await navigator.clipboard.writeText(text);
      }
      setAnnouncement(`${author === "agent" ? "Agent" : "User"} message copied.`);
    } catch {
      setAnnouncement("Clipboard access is unavailable.");
    }
  };

  const runWindowAction = async (action: "minimize" | "maximize" | "close") => {
    if (!nativeShell) return;
    if (action === "close") {
      await requestNativeWindowClose();
      return;
    }
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    const appWindow = getCurrentWindow();
    if (action === "minimize") await appWindow.minimize();
    if (action === "maximize") await appWindow.toggleMaximize();
  };

  const trapCommandFocus = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      closeCommandCenter();
      return;
    }
    if (event.key !== "Tab") return;
    const controls = Array.from(commandDialogRef.current?.querySelectorAll<HTMLElement>("input, button:not([disabled])") ?? []);
    if (!controls.length) return;
    const first = controls[0];
    const last = controls[controls.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  };

  const displayConnection: LiveConnectionState = nativeShell && !liveChat && systemConnection.phase !== "live"
    ? {
        phase: systemConnection.phase,
        message: systemConnection.message,
        targetSessionId: null,
        lastEventAtUnixMs: null,
      }
    : liveConnection;
  const displaySnapshot = nativeShell ? liveSnapshot(liveChat, displayConnection) : snapshot;
  const messages = [...(displaySnapshot?.messages ?? []), ...(nativeShell ? [] : selectedLocalMessages)];
  const planExpanded = planPinned || planHovered;
  const commandNeedle = commandQuery.trim().toLocaleLowerCase();
  const commandMatches = (label: string) => label.toLocaleLowerCase().includes(commandNeedle);
  const commandChats = allSessions.filter((session) => commandMatches(session.title));
  const showNewChatCommand = (!nativeShell || selectedProject)
    && commandMatches(selectedProject ? "New chat in current project" : "New standalone chat");
  const showAccessCommand = (!nativeShell || accessControl !== null)
    && commandMatches("Agent access settings permissions full access auto approve");
  const showRuntimeCommand = commandMatches("Runtime settings provider model reasoning speed Codex");
  const showResourcesCommand = resourcesAvailable && commandMatches("Open workspace resources");
  const showPreviewCommand = resourcesAvailable && commandMatches("Open local preview browser");
  const commandHasResults = commandChats.length > 0
    || showNewChatCommand
    || showAccessCommand
    || showRuntimeCommand
    || showResourcesCommand
    || showPreviewCommand;

  return (
    <div className={`workbench${sidebarOpen ? "" : " sidebar-collapsed"}${resourcesOpen ? "" : " resources-collapsed"}${resourcesAvailable ? "" : " resources-unavailable"}`}>
      <header className="titlebar" data-tauri-drag-region>
        <div className="titlebar__left" data-tauri-drag-region>
          <IconButton
            label={sidebarOpen ? "Hide project navigation" : "Show project navigation"}
            icon={SidebarSimple}
            active={sidebarOpen}
            onClick={() => setSidebarOpen((open) => !open)}
          />
          <nav className="breadcrumbs" aria-label="Current location">
            <span>{selectedProject ? "Projects" : "Chats"}</span>
            {selectedProject && <><span>/</span><span>{selectedProject.title}</span></>}
            <span>/</span><strong>{selectedTitle}</strong>
          </nav>
        </div>

        <button type="button" className="command-button" onClick={openCommandCenter}>
          <MagnifyingGlass size={15} aria-hidden="true" />
          <span>Search chats, settings or commands</span>
          <kbd>Ctrl K</kbd>
        </button>

        <div className="titlebar__right">
          <div className="window-controls" role="group" aria-label="Window controls">
            <IconButton label={nativeShell ? "Minimize window" : "Minimize — available in desktop shell"} icon={Minus} onClick={() => void runWindowAction("minimize")} disabled={!nativeShell} />
            <IconButton label={nativeShell ? "Maximize or restore window" : "Maximize — available in desktop shell"} icon={Square} onClick={() => void runWindowAction("maximize")} disabled={!nativeShell} />
            <IconButton label={nativeShell ? "Close window" : "Close — available in desktop shell"} icon={X} onClick={() => void runWindowAction("close")} disabled={!nativeShell} className="window-close" />
          </div>
        </div>
      </header>

      <nav className="activity-rail" aria-label="Primary navigation">
        <div className="rail-main">
          <div className="rail-brand" role="img" aria-label="Dennett"><BracketsCurly size={19} weight="bold" aria-hidden="true" /></div>
          <IconButton label="Chats" icon={ChatsCircle} active onClick={() => setSidebarOpen(true)} />
          <IconButton label="Tasks — available in a later milestone" icon={ListChecks} disabled />
          <IconButton label="Plugins — available in a later milestone" icon={Plug} disabled />
        </div>
      </nav>

      {sidebarOpen && (
        <aside className="project-sidebar" aria-label="Project and chat navigation">
          <div ref={projectMenuRef} className="sidebar-heading">
            <button type="button" className="sidebar-title" aria-label="Projects list"><span>Projects</span><CaretDown size={14} aria-hidden="true" /></button>
            <IconButton
              buttonRef={projectMenuTriggerRef}
              label="New project"
              icon={Plus}
              className="new-project-trigger"
              active={newProjectMenuOpen}
              onClick={() => {
                setComposerPopover(null);
                setNewProjectMenuOpen((open) => !open);
              }}
            />
            {newProjectMenuOpen && (
              <div className="project-create-menu" role="dialog" aria-label="Create or add project">
                <strong>New project</strong>
                <button ref={projectMenuFirstRef} type="button" disabled={nativeShell} onClick={() => createProjectPreview("empty")}>
                  <FolderPlus size={17} aria-hidden="true" />
                  <span><strong>Create empty project</strong><small>{nativeShell ? "Available with workspace management" : "Use the default projects folder"}</small></span>
                </button>
                <button type="button" disabled={nativeShell} onClick={() => createProjectPreview("existing")}>
                  <FolderOpen size={17} aria-hidden="true" />
                  <span><strong>Add existing folder</strong><small>{nativeShell ? "Available with workspace management" : "Choose a folder as the project"}</small></span>
                </button>
                <p>{nativeShell
                  ? "Project folders are not connected in M01 yet. Nothing will be created silently."
                  : "Folder changes arrive with the real workspace in M02. This checkpoint previews the flow locally."}</p>
              </div>
            )}
          </div>

          <div className="sidebar-scroll">
            <div className="project-groups">
              {visibleProjectGroups.map((project) => (
                <section key={project.id} className="project-group" aria-label={`${project.title} project`}>
                  <div className="project-heading">
                    <FolderSimple size={17} aria-hidden="true" />
                    <span>{project.title}</span>
                    <button
                      type="button"
                      className="new-chat-trigger"
                      aria-label={`New chat in ${project.title}`}
                      title={`New chat in ${project.title}`}
                      disabled={creatingChat}
                      onClick={(event) => {
                        event.preventDefault();
                        event.stopPropagation();
                        startNewChat(project.id);
                      }}
                    >
                      <Plus size={15} aria-hidden="true" />
                    </button>
                  </div>
                  <div className="nested-chats">
                    {project.sessions.map((session) => (
                      <button
                        type="button"
                        key={session.id}
                        className={selectedSession === session.id ? "chat-row is-active" : "chat-row"}
                        aria-current={selectedSession === session.id ? "page" : undefined}
                        onClick={() => void selectSession(session.id)}
                      >
                        <span>{session.title}</span><small>{session.meta}</small>
                      </button>
                    ))}
                  </div>
                </section>
              ))}
            </div>

            <section className="recent-chats" aria-labelledby="recent-chats-heading">
              <div className="recent-heading">
                <h2 id="recent-chats-heading">Recent</h2>
                <button type="button" className="new-chat-trigger" aria-label="New recent chat" title="New recent chat" disabled={creatingChat} onClick={() => startNewChat()}>
                  <Plus size={15} aria-hidden="true" />
                </button>
              </div>
              {standaloneSessions.map((session) => (
                <button
                  type="button"
                  key={session.id}
                  className={selectedSession === session.id ? "recent-row is-active" : "recent-row"}
                  aria-current={selectedSession === session.id ? "page" : undefined}
                  onClick={() => void selectSession(session.id)}
                >
                  <span>{session.title}</span><small>{session.meta}</small>
                </button>
              ))}
              {standaloneSessions.length === 0 && <p className="recent-empty">No standalone chats yet</p>}
            </section>
          </div>
        </aside>
      )}

      <div className="account-dock" role="group" aria-label="Account and device controls">
        <div className="account-identity" role="group" aria-label="Account: User"><span>U</span><strong>User</strong></div>
        <div>
          {updateAvailable && <IconButton label="Install available update" icon={DownloadSimple} onClick={() => setAnnouncement("Update flow is not connected in this checkpoint.")} />}
          <IconButton label="Voice mode — available in a later milestone" icon={Microphone} disabled />
        </div>
      </div>

      <main className="main-workspace">
        {surface.kind === "chat" ? (
          <>
            <section ref={conversationRef} className="conversation" aria-label="Conversation">
              <div className="conversation-inner">
                {displaySnapshot ? (
                  <>
                    <div className={`state-line tone-${displaySnapshot.stateTone}`} role="status">
                      <StateIcon tone={displaySnapshot.stateTone} />
                      <strong>{displaySnapshot.stateLabel}</strong>
                      <span>{displaySnapshot.notice}</span>
                      {nativeShell && displaySnapshot.state === "timed-out" && (
                        <button
                          type="button"
                          className="state-action"
                          disabled={retryingTurnId !== null}
                          onClick={retryTimedOutTurn}
                        >{retryingTurnId ? "Retrying..." : "Retry"}</button>
                      )}
                      <small>{displaySnapshot.freshness}</small>
                    </div>
                    {displaySnapshot.state === "loading" ? (
                      <div className="loading-lines" role="status" aria-label="Loading conversation content"><span /><span /><span /></div>
                    ) : messages.length ? (
                      messages.map((message) => <Message key={message.id} message={message} onCopy={(author, text) => void copyMessage(author, text)} />)
                    ) : (
                      <EmptyConversation onSuggestion={useSuggestion} projectScoped={Boolean(selectedProject)} chatAvailable={!nativeShell || liveChat !== null} />
                    )}
                  </>
                ) : (
                  <div className="loading-lines" role="status" aria-label="Loading Project Chat"><span /><span /><span /></div>
                )}
              </div>
            </section>

            <section className="composer-region" aria-label="Message composer">
              <div ref={composerShellRef} className="composer-shell">
                <div className="composer">
                  <textarea
                    ref={composerRef}
                    value={draft}
                    onChange={(event) => setDraft(event.target.value)}
                    onFocus={() => setComposerPopover(null)}
                    onBlur={() => void persistCurrentDraft()}
                    onKeyDown={(event) => {
                      if ((event.ctrlKey || event.metaKey) && event.key === "Enter") sendDraft();
                    }}
                    rows={2}
                    placeholder={nativeShell && liveConnection.phase === "empty"
                      ? "Create or select a chat to begin…"
                      : nativeShell && !nativeConversationReady ? "Opening the selected chat…" : "Ask the project agent…"}
                    aria-label="Message to project agent"
                    disabled={nativeShell && !nativeConversationReady}
                  />
                  <div className="composer-toolbar">
                    <div className="composer-tools">
                      {!nativeShell && <IconButton
                        label="Add context"
                        icon={Plus}
                        active={composerPopover === "context"}
                        buttonRef={contextTriggerRef}
                        ariaControls="composer-context-popover"
                        ariaExpanded={composerPopover === "context"}
                        ariaHasPopup="dialog"
                        onClick={() => setComposerPopover((open) => open === "context" ? null : "context")}
                      />}
                      {!nativeShell && <IconButton
                        label="Plugins"
                        icon={Plug}
                        active={composerPopover === "plugins"}
                        buttonRef={pluginsTriggerRef}
                        ariaControls="composer-plugins-popover"
                        ariaExpanded={composerPopover === "plugins"}
                        ariaHasPopup="dialog"
                        onClick={() => setComposerPopover((open) => open === "plugins" ? null : "plugins")}
                      />}
                      {nativeShell && accessControl === null ? (
                        <span className="composer-setting is-static"><ShieldCheck size={14} aria-hidden="true" />Access unavailable</span>
                      ) : <button
                        ref={accessTriggerRef}
                        type="button"
                        className="composer-setting"
                        aria-expanded={composerPopover === "access"}
                        aria-controls="composer-access-popover"
                        aria-haspopup="dialog"
                        disabled={runtimeControlsLocked}
                        title={runtimeControlsLocked ? "Available after the current run finishes" : undefined}
                        onClick={() => setComposerPopover((open) => open === "access" ? null : "access")}
                      >
                        <ShieldCheck size={14} aria-hidden="true" />{nativeShell
                          ? selectedAccessChoice?.label ?? "Agent access"
                          : accessMode === "full" ? "Full access" : "Auto-approve"}<CaretDown size={11} aria-hidden="true" />
                      </button>}
                    </div>
                    <div className="composer-send">
                      <button
                        ref={runtimeTriggerRef}
                        type="button"
                        className="runtime-setting"
                        aria-expanded={composerPopover === "runtime"}
                        aria-controls="composer-runtime-popover"
                        aria-haspopup="dialog"
                        aria-label={`Agent runtime: ${runtimeName}${runtimeModeLabel ? `, ${runtimeModeLabel}` : ""}`}
                        disabled={runtimeControlsLocked}
                        title={runtimeControlsLocked ? "Available after the current run finishes" : undefined}
                        onClick={() => setComposerPopover((open) => open === "runtime" ? null : "runtime")}
                      >
                        <span>{runtimeName}</span>{runtimeModeLabel && <small>{runtimeModeLabel}</small>}<CaretDown size={11} aria-hidden="true" />
                      </button>
                      <IconButton label="Voice input — available in a later milestone" icon={Microphone} disabled />
                      {(!displaySnapshot?.canStop || steeringMode === "native") && (
                        <button type="button" className="send-button" onClick={sendDraft} disabled={!draft.trim() || (nativeShell && (!nativeConversationReady || sendingSessions.has(selectedSession)))} aria-label="Send message" title="Send with Ctrl Enter"><ArrowUp size={17} weight="bold" /></button>
                      )}
                      {displaySnapshot?.canStop && (
                        <button type="button" className="send-button stop-button" onClick={stopGeneration} aria-label={`Stop generation for session "${selectedTitle}"`} title="Stop generation"><Stop size={15} weight="fill" /></button>
                      )}
                    </div>
                  </div>
                </div>

                {!nativeShell && composerPopover === "context" && (
                  <div ref={contextDialogRef} id="composer-context-popover" className="composer-popover popover-left" role="dialog" aria-label="Add context" tabIndex={-1}>
                    <strong>Add context</strong><p>Context effects arrive with typed local IPC.</p>
                    <button type="button" disabled><Plus size={14} />Files or folders<span>Later</span></button>
                    <button type="button" disabled><LinkSimple size={14} />URL or artifact<span>Later</span></button>
                  </div>
                )}
                {!nativeShell && composerPopover === "plugins" && (
                  <div ref={pluginsDialogRef} id="composer-plugins-popover" className="composer-popover popover-left popover-plugins" role="dialog" aria-label="Plugins" tabIndex={-1}>
                    <strong>Plugins</strong><p>No plugins are attached to this session.</p>
                    <button type="button" disabled><Plug size={14} />Browse plugins<span>Later</span></button>
                  </div>
                )}
                {composerPopover === "access" && (!nativeShell || accessControl !== null) && (
                  <div id="composer-access-popover" className="composer-popover popover-left popover-access" role="dialog" aria-label="Agent access">
                    <strong>Agent access</strong>
                    {nativeShell ? accessControl?.choices.map((choice, index) => {
                      const selected = choice.id === selectedAccessChoice?.id;
                      return (
                        <button
                          ref={index === 0 ? accessFirstRef : undefined}
                          type="button"
                          className={selected ? "is-selected" : ""}
                          key={choice.id}
                          title={choice.description ?? undefined}
                          onClick={() => {
                            setRuntimeSelections((selections) => reconcileRuntimeSelections(
                              advertisedRuntimeControls,
                              { ...selections, [accessControl.id]: choice.id },
                            ));
                            setComposerPopover(null);
                            accessTriggerRef.current?.focus();
                          }}
                        >
                          {choice.id === "full_access" ? <ShieldCheck size={14} /> : <Command size={14} />}
                          {choice.label}{selected && <CheckCircle size={14} />}
                        </button>
                      );
                    }) : <>
                      <button ref={accessFirstRef} type="button" className={accessMode === "full" ? "is-selected" : ""} onClick={() => { setAccessMode("full"); setComposerPopover(null); accessTriggerRef.current?.focus(); }}><ShieldCheck size={14} />Full access{accessMode === "full" && <CheckCircle size={14} />}</button>
                      <button type="button" className={accessMode === "auto" ? "is-selected" : ""} onClick={() => { setAccessMode("auto"); setComposerPopover(null); accessTriggerRef.current?.focus(); }}><Command size={14} />Auto-approve{accessMode === "auto" && <CheckCircle size={14} />}</button>
                    </>}
                  </div>
                )}
                {composerPopover === "runtime" && (
                  <div id="composer-runtime-popover" className="composer-popover popover-right runtime-popover" role="dialog" aria-label="Agent runtime">
                    <strong>Agent runtime</strong>
                    <div className="runtime-row"><span><Robot size={14} />Source</span><b>{runtimeSourceLabel}</b></div>
                    {!nativeShell && <div className="runtime-row"><span><Brain size={14} />Model</span><b>Provider default</b></div>}
                    {!nativeShell && <div className="runtime-choice"><span><Gauge size={14} />Reasoning</span><div><button ref={(node) => { runtimeFirstRef.current = node; }} type="button" className={reasoning === "medium" ? "is-selected" : ""} onClick={() => setReasoning("medium")}>Medium</button><button type="button" className={reasoning === "high" ? "is-selected" : ""} onClick={() => setReasoning("high")}>High</button></div></div>}
                    {nativeShell && runtimeControls.map((control, index) => {
                      const availableChoices = control.choices.filter((choice) => runtimeChoiceAvailable(choice, effectiveRuntimeSelections));
                      const selectedChoice = availableChoices.find(
                        (choice) => choice.id === effectiveRuntimeSelections[control.id],
                      ) ?? null;
                      const expanded = expandedRuntimeControl === control.id;
                      const optionListId = `runtime-options-${control.id.replace(/[^a-z0-9_-]/gi, "-")}`;
                      return (
                        <div className={`runtime-control${expanded ? " is-expanded" : ""}`} key={control.id}>
                          <button
                            ref={(node) => {
                              runtimeControlTriggerRefs.current[control.id] = node;
                              if (index === 0) runtimeFirstRef.current = node;
                            }}
                            type="button"
                            className="runtime-control__trigger"
                            aria-label={`${control.label}: ${selectedChoice?.label ?? "Unavailable"}`}
                            aria-haspopup="listbox"
                            aria-expanded={expanded}
                            aria-controls={optionListId}
                            onClick={() => setExpandedRuntimeControl((current) => current === control.id ? null : control.id)}
                          >
                            <span><Gauge size={14} />{control.label}</span>
                            <b>{selectedChoice?.label ?? "Unavailable"}<CaretDown size={11} aria-hidden="true" /></b>
                          </button>
                          {expanded && (
                            <div id={optionListId} className="runtime-option-list" role="listbox" aria-label={`${control.label} options`}>
                              {availableChoices.map((choice) => {
                                const selected = choice.id === selectedChoice?.id;
                                return (
                                  <button
                                    type="button"
                                    role="option"
                                    aria-selected={selected}
                                    className={selected ? "is-selected" : ""}
                                    key={choice.id}
                                    title={choice.description ?? undefined}
                                    onClick={() => {
                                      setRuntimeSelections((selections) => reconcileRuntimeSelections(
                                        advertisedRuntimeControls,
                                        { ...selections, [control.id]: choice.id },
                                      ));
                                      setExpandedRuntimeControl(null);
                                      requestAnimationFrame(() => runtimeControlTriggerRefs.current[control.id]?.focus());
                                    }}
                                  >
                                    <span>{choice.label}</span>{selected && <CheckCircle size={13} aria-hidden="true" />}
                                  </button>
                                );
                              })}
                            </div>
                          )}
                        </div>
                      );
                    })}
                    {nativeShell && runtimeControls.length === 0 && <p className="runtime-capability-note">{runtimeAdapterId
                      ? "This runtime did not publish selectable model, reasoning or speed options. Dennett will show them here only when the active provider reports real choices."
                      : "No runtime descriptor is available yet."}</p>}
                  </div>
                )}
              </div>
            </section>
          </>
        ) : (
          <ArtifactViewer surface={surface} onClose={() => openSurface({ kind: "chat" })} />
        )}
      </main>

      {resourcesAvailable && <aside className="resource-area" aria-label="Workspace resources">
        {resourcesOpen ? (
          <div className="resource-panel">
            <header className="resource-header"><h2>Workspace</h2><IconButton label="Collapse workspace resources" icon={SidebarSimple} className="resource-panel-toggle is-open" onClick={() => setResourcesOpen(false)} /></header>

            <section
              className={`plan-card${planExpanded ? " is-expanded" : ""}`}
              onMouseEnter={() => setPlanHovered(true)}
              onMouseLeave={() => setPlanHovered(false)}
              onFocus={() => setPlanHovered(true)}
              onBlur={(event) => { if (!event.currentTarget.contains(event.relatedTarget)) setPlanHovered(false); }}
            >
              <button type="button" aria-expanded={planExpanded} onClick={() => setPlanPinned((pinned) => !pinned)}>
                <span className="plan-index">2</span>
                <span><small>Current plan step</small><strong>Rebuild the owner checkpoint</strong></span>
                <CaretDown size={13} aria-hidden="true" />
              </button>
              {planExpanded && (
                <ol className="plan-details">
                  <li className="is-done">Capture owner corrections</li>
                  <li className="is-current">Rebuild the interface</li>
                  <li>Run visual and interaction QA</li>
                  <li>Owner review before merge</li>
                </ol>
              )}
            </section>

            <div className="resource-scroll">
              <section className="resource-section" aria-labelledby="results-heading">
                <h3 id="results-heading">Results</h3>
                <button type="button" className="resource-row" onClick={() => openSurface({ kind: "report", title: "Project Chat checkpoint", subtitle: "WP-M01-003 · owner review" })}>
                  <FileText size={17} aria-hidden="true" /><span><strong>Project Chat checkpoint</strong><small>Owner review</small></span>
                </button>
              </section>

              <section className="resource-section" aria-labelledby="subagents-heading">
                <h3 id="subagents-heading">Subagents <span>1</span></h3>
                <div className="resource-row is-static"><Robot size={17} aria-hidden="true" /><span><strong>Detached design review</strong><small>Queued after implementation</small></span></div>
              </section>

              <section className="resource-section" aria-labelledby="browser-heading">
                <h3 id="browser-heading">Browser</h3>
                <button type="button" className="resource-row" onClick={() => openSurface({ kind: "browser", title: "Dennett", subtitle: "127.0.0.1:5173" })}>
                  <Globe size={17} aria-hidden="true" /><span><strong>Dennett</strong><small>Local preview</small></span><em>127.0.0.1</em>
                </button>
              </section>

              <section className="resource-section" aria-labelledby="sources-heading">
                <h3 id="sources-heading">Sources <span>3</span></h3>
                <button type="button" className="resource-row" onClick={() => openSurface({ kind: "source", title: "Desktop specification", subtitle: "docs/specifications/60_…" })}><FileCode size={17} aria-hidden="true" /><span><strong>Desktop specification</strong><small>Project Workspace and Chat</small></span></button>
                <button type="button" className="resource-row" onClick={() => openSurface({ kind: "source", title: "Owner direction v5", subtitle: "docs/design/WP-M01-003/…" })}><LinkSimple size={17} aria-hidden="true" /><span><strong>Owner direction v5</strong><small>Current visual contract</small></span></button>
                <button type="button" className="resource-row" onClick={() => openSurface({ kind: "source", title: "Draft pull request", subtitle: "github.com/Andrey-Good/…/pull/12" })}><GithubLogo size={17} aria-hidden="true" /><span><strong>Draft pull request</strong><small>PR #12</small></span></button>
              </section>
            </div>

            <footer className="resource-footer">
              <label className="fixture-select"><span>Preview</span><select value={fixture} onChange={(event) => setSelectedFixture(event.target.value as FixtureId)} aria-label="Preview state">{fixtureIds.map((id) => <option key={id} value={id}>{fixtureLabels[id]}</option>)}</select><CaretDown size={12} aria-hidden="true" /></label>
              <span className="git-status"><GitBranch size={13} aria-hidden="true" />codex/wp-m01-003 · clean</span>
            </footer>
          </div>
        ) : (
          <nav className="resource-dock" aria-label="Collapsed workspace resources">
            <IconButton label="Show workspace resources" icon={Sidebar} className="resource-panel-toggle is-closed" onClick={() => setResourcesOpen(true)} />
            <IconButton label="Open results" icon={FileText} onClick={() => { setResourcesOpen(true); openSurface({ kind: "report", title: "Project Chat checkpoint", subtitle: "WP-M01-003 · owner review" }); }} />
            <IconButton label="Open subagents" icon={Robot} onClick={() => setResourcesOpen(true)} />
            <IconButton label="Open browser" icon={Globe} onClick={() => { setResourcesOpen(true); openSurface({ kind: "browser", title: "Dennett", subtitle: "127.0.0.1:5173" }); }} />
            <IconButton label="Open sources" icon={LinkSimple} onClick={() => setResourcesOpen(true)} />
          </nav>
        )}
      </aside>}

      <div className="sr-only" role="status" aria-live="polite" aria-atomic="true">{announcement}</div>

      {commandOpen && (
        <div className="dialog-backdrop" onMouseDown={closeCommandCenter}>
          <div ref={commandDialogRef} className="command-dialog" role="dialog" aria-modal="true" aria-label="Command Center" onKeyDown={trapCommandFocus} onMouseDown={(event) => event.stopPropagation()}>
            <label><MagnifyingGlass size={18} aria-hidden="true" /><input ref={commandRef} value={commandQuery} onChange={(event) => setCommandQuery(event.target.value)} placeholder="Search chats, settings or commands…" aria-label="Command Center search" /></label>
            <div className="command-results">
              {commandChats.length > 0 && <><span>CHATS</span>{commandChats.map((session) => (
                <button
                  type="button"
                  key={session.id}
                  aria-current={selectedSession === session.id ? "page" : undefined}
                  onClick={() => { closeCommandCenter(); void selectSession(session.id); }}
                >
                  <ChatsCircle size={16} />{session.title}
                </button>
              ))}</>}
              {(showNewChatCommand || showAccessCommand || showRuntimeCommand || showResourcesCommand || showPreviewCommand) && <span>QUICK ACTIONS</span>}
              {showNewChatCommand && <button
                type="button"
                aria-label={selectedProject ? "New chat in current project" : "New standalone chat"}
                disabled={creatingChat}
                onClick={() => { closeCommandCenter(); startNewChat(selectedProject?.id); }}
              >
                <Plus size={16} />{selectedProject ? "New chat in current project" : "New standalone chat"}<kbd>Enter</kbd>
              </button>}
              {showAccessCommand && <button type="button" onClick={() => { closeCommandCenter(); setComposerPopover("access"); }}><ShieldCheck size={16} />Agent access settings</button>}
              {showRuntimeCommand && <button type="button" onClick={() => { closeCommandCenter(); setComposerPopover("runtime"); }}><Brain size={16} />Runtime settings</button>}
              {showResourcesCommand && <button type="button" onClick={() => { closeCommandCenter(); setResourcesOpen(true); }}><Browsers size={16} />Open workspace resources</button>}
              {showPreviewCommand && <button type="button" onClick={() => { closeCommandCenter(); setResourcesOpen(true); openSurface({ kind: "browser", title: "Dennett", subtitle: "127.0.0.1:5173" }); }}><Globe size={16} />Open local preview</button>}
              {!commandHasResults && <p role="status">No matching chats or actions.</p>}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
