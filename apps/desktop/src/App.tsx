import React from "react";
import type { IconProps } from "@phosphor-icons/react";
import {
  ArrowUp,
  Brain,
  BracketsCurly,
  Browsers,
  CaretDown,
  ChatCircleDots,
  CheckCircle,
  CircleNotch,
  Command,
  DotsThree,
  DownloadSimple,
  FileCode,
  FileText,
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
  PencilSimpleLine,
  Plug,
  Plus,
  Robot,
  ShieldCheck,
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
import "./styles.css";

type Icon = React.ComponentType<IconProps>;
type ComposerPopover = "context" | "plugins" | "access" | "runtime" | null;
type AccessMode = "full" | "auto";
type ReasoningLevel = "medium" | "high";
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
}: {
  label: string;
  icon: Icon;
  onClick?: () => void;
  active?: boolean;
  disabled?: boolean;
  className?: string;
}): React.JSX.Element {
  return (
    <button
      type="button"
      className={`icon-button${active ? " is-active" : ""}${className ? ` ${className}` : ""}`}
      aria-label={label}
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

function Message({ message }: { message: ChatMessage }): React.JSX.Element {
  return (
    <article className={`message message--${message.author}`} aria-label={`${message.author} message`}>
      <div className="message-copy">
        {message.paragraphs.map((paragraph) => <p key={paragraph}>{paragraph}</p>)}
        {message.bullets && (
          <ul>{message.bullets.map((item) => <li key={item}>{item}</li>)}</ul>
        )}
        <span className="message-time">{message.timestamp}</span>
      </div>
    </article>
  );
}

function EmptyConversation({ onSuggestion }: { onSuggestion: (prompt: string) => void }): React.JSX.Element {
  return (
    <div className="empty-state">
      <ChatCircleDots size={30} aria-hidden="true" />
      <h2>Start with the project</h2>
      <p>Ask the direct agent to inspect, explain or change the selected workspace.</p>
      <div className="prompt-suggestions" role="group" aria-label="Prompt suggestions">
        <button type="button" onClick={() => onSuggestion("Summarize the current milestone")}>Summarize the milestone</button>
        <button type="button" onClick={() => onSuggestion("Find the next eligible package")}>Find the next package</button>
      </div>
    </div>
  );
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
  const [fixture, setFixture] = React.useState<FixtureId>("streaming");
  const [snapshot, setSnapshot] = React.useState<ProjectChatSnapshot | null>(null);
  const [sidebarOpen, setSidebarOpen] = React.useState(true);
  const [resourcesOpen, setResourcesOpen] = React.useState(true);
  const [selectedSession, setSelectedSession] = React.useState("screen");
  const [localSessions, setLocalSessions] = React.useState<Array<SessionItem & { projectId?: string }>>([]);
  const [surface, setSurface] = React.useState<WorkspaceSurface>({ kind: "chat" });
  const [draft, setDraft] = React.useState("");
  const [localMessages, setLocalMessages] = React.useState<Record<string, ChatMessage[]>>({});
  const [announcement, setAnnouncement] = React.useState("Project Chat opened");
  const [commandOpen, setCommandOpen] = React.useState(false);
  const [composerPopover, setComposerPopover] = React.useState<ComposerPopover>(null);
  const [accessMode, setAccessMode] = React.useState<AccessMode>("full");
  const [reasoning, setReasoning] = React.useState<ReasoningLevel>("high");
  const [planPinned, setPlanPinned] = React.useState(false);
  const [planHovered, setPlanHovered] = React.useState(false);
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
  const runtimeFirstRef = React.useRef<HTMLButtonElement>(null);
  const nextLocalSessionIdRef = React.useRef(1);
  const nextLocalMessageIdRef = React.useRef(1);
  const nativeShell = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

  const visibleProjectGroups = projectGroups.map((project) => ({
    ...project,
    sessions: [...project.sessions, ...localSessions.filter((session) => session.projectId === project.id)],
  }));
  const standaloneSessions = [...localSessions.filter((session) => !session.projectId), ...recentChats];
  const allSessions = [...visibleProjectGroups.flatMap((project) => project.sessions), ...standaloneSessions];
  const selectedTitle = allSessions.find((session) => session.id === selectedSession)?.title ?? allSessions[0].title;
  const selectedProject = visibleProjectGroups.find((project) => project.sessions.some((session) => session.id === selectedSession));
  const selectedLocalMessages = localMessages[selectedSession] ?? [];

  const openCommandCenter = React.useCallback(() => {
    setComposerPopover(null);
    setCommandOpen((open) => {
      if (!open && document.activeElement instanceof HTMLElement) returnFocusRef.current = document.activeElement;
      return true;
    });
  }, []);
  const closeCommandCenter = React.useCallback(() => setCommandOpen(false), []);

  React.useEffect(() => {
    document.documentElement.classList.toggle("native-shell", nativeShell);
    return () => document.documentElement.classList.remove("native-shell");
  }, [nativeShell]);

  React.useEffect(() => {
    let current = true;
    setSnapshot(null);
    const client = createFixtureDennettClient(fixture);
    client.readProjectChat({ projectId: selectedProject?.id ?? "standalone", sessionId: selectedSession }).then((next) => {
      if (!current) return;
      setSnapshot(next);
      setAnnouncement(`${next.stateLabel}. ${next.phase}.`);
    });
    return () => { current = false; };
  }, [fixture, selectedProject?.id, selectedSession]);

  React.useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        openCommandCenter();
      }
      if (event.key === "Escape") {
        closeCommandCenter();
        const returnTarget = composerPopover === "access"
          ? accessTriggerRef.current
          : composerPopover === "runtime"
            ? runtimeTriggerRef.current
            : null;
        setComposerPopover(null);
        requestAnimationFrame(() => returnTarget?.focus());
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [closeCommandCenter, composerPopover, openCommandCenter]);

  React.useEffect(() => {
    if (!composerPopover) return;
    const closeOnOutsidePointer = (event: PointerEvent) => {
      if (!composerShellRef.current?.contains(event.target as Node)) setComposerPopover(null);
    };
    document.addEventListener("pointerdown", closeOnOutsidePointer);
    return () => document.removeEventListener("pointerdown", closeOnOutsidePointer);
  }, [composerPopover]);

  React.useEffect(() => {
    if (composerPopover === "access") accessFirstRef.current?.focus();
    if (composerPopover === "runtime") runtimeFirstRef.current?.focus();
  }, [composerPopover]);

  React.useEffect(() => {
    if (commandOpen) commandRef.current?.focus();
    else if (commandWasOpenRef.current) returnFocusRef.current?.focus();
    commandWasOpenRef.current = commandOpen;
  }, [commandOpen]);

  React.useEffect(() => {
    const conversation = conversationRef.current;
    if (conversation && surface.kind === "chat") conversation.scrollTop = conversation.scrollHeight;
  }, [snapshot, selectedLocalMessages.length, surface.kind]);

  const selectSession = (sessionId: string) => {
    setSelectedSession(sessionId);
    setSurface({ kind: "chat" });
    setAnnouncement(`Opened ${allSessions.find((item) => item.id === sessionId)?.title ?? "chat"}.`);
  };

  const startNewChat = (projectId?: string) => {
    const sessionId = `local-chat-${nextLocalSessionIdRef.current++}`;
    setLocalSessions((sessions) => [...sessions, { id: sessionId, title: "Untitled chat", meta: "now", projectId }]);
    setSelectedSession(sessionId);
    setFixture("empty");
    setSurface({ kind: "chat" });
    setComposerPopover(null);
    setAnnouncement(projectId ? "New project chat preview opened." : "New standalone chat preview opened.");
    requestAnimationFrame(() => composerRef.current?.focus());
  };

  const sendDraft = () => {
    const content = draft.trim();
    if (!content) return;
    setLocalMessages((messagesBySession) => ({
      ...messagesBySession,
      [selectedSession]: [
        ...(messagesBySession[selectedSession] ?? []),
        { id: `local-message-${nextLocalMessageIdRef.current++}`, author: "user", paragraphs: [content], timestamp: "now" },
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

  const stopGeneration = () => {
    setFixture("stopped");
    setAnnouncement(`Stop requested for session "${selectedTitle}".`);
  };

  const openSurface = (next: WorkspaceSurface) => {
    setSurface(next);
    setAnnouncement(`Opened ${next.kind === "chat" ? "chat" : next.title}.`);
  };

  const runWindowAction = async (action: "minimize" | "maximize" | "close") => {
    if (!nativeShell) return;
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    const appWindow = getCurrentWindow();
    if (action === "minimize") await appWindow.minimize();
    if (action === "maximize") await appWindow.toggleMaximize();
    if (action === "close") await appWindow.close();
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

  const messages = [...(snapshot?.messages ?? []), ...selectedLocalMessages];
  const planExpanded = planPinned || planHovered;

  return (
    <div className={`workbench${sidebarOpen ? "" : " sidebar-collapsed"}${resourcesOpen ? "" : " resources-collapsed"}`}>
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
          <IconButton
            label={resourcesOpen ? "Hide workspace resources" : "Show workspace resources"}
            icon={Browsers}
            active={resourcesOpen}
            onClick={() => setResourcesOpen((open) => !open)}
          />
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
          <IconButton label="New chat" icon={PencilSimpleLine} onClick={() => startNewChat()} />
          <IconButton label="Projects" icon={FolderSimple} active onClick={() => setSidebarOpen(true)} />
          <IconButton label="Tasks — available in a later milestone" icon={ListChecks} disabled />
          <IconButton label="Plugins — available in a later milestone" icon={Plug} disabled />
        </div>
      </nav>

      {sidebarOpen && (
        <aside className="project-sidebar" aria-label="Project and chat navigation">
          <div className="sidebar-heading">
            <button type="button" className="sidebar-title" aria-label="Projects list"><span>Projects</span><CaretDown size={14} aria-hidden="true" /></button>
            <div>
              <IconButton label="Project options — available in a later milestone" icon={DotsThree} disabled />
              <IconButton label="New project chat" icon={Plus} onClick={() => startNewChat(selectedProject?.id ?? projectGroups[0].id)} />
            </div>
          </div>

          <div className="sidebar-scroll">
            <div className="project-groups">
              {visibleProjectGroups.map((project) => (
                <details key={project.id} open>
                  <summary><FolderSimple size={17} aria-hidden="true" /><span>{project.title}</span></summary>
                  <div className="nested-chats">
                    {project.sessions.map((session) => (
                      <button
                        type="button"
                        key={session.id}
                        className={selectedSession === session.id ? "chat-row is-active" : "chat-row"}
                        onClick={() => selectSession(session.id)}
                      >
                        <span>{session.title}</span><small>{session.meta}</small>
                      </button>
                    ))}
                  </div>
                </details>
              ))}
            </div>

            <section className="recent-chats" aria-labelledby="recent-chats-heading">
              <h2 id="recent-chats-heading">Recent</h2>
              {standaloneSessions.map((session) => (
                <button
                  type="button"
                  key={session.id}
                  className={selectedSession === session.id ? "recent-row is-active" : "recent-row"}
                  onClick={() => selectSession(session.id)}
                >
                  <span>{session.title}</span><small>{session.meta}</small>
                </button>
              ))}
            </section>
          </div>
        </aside>
      )}

      <div className="account-dock" role="group" aria-label="Account and device controls">
        <div className="account-identity" role="group" aria-label="Account: User"><span>U</span><strong>User</strong></div>
        <div>
          <IconButton label="Updates — none available" icon={DownloadSimple} disabled />
          <IconButton label="Voice mode — available in a later milestone" icon={Microphone} disabled />
        </div>
      </div>

      <main className="main-workspace">
        {surface.kind === "chat" ? (
          <>
            <section ref={conversationRef} className="conversation" aria-label="Conversation">
              <div className="conversation-inner">
                {snapshot ? (
                  <>
                    <div className={`state-line tone-${snapshot.stateTone}`} role="status">
                      <StateIcon tone={snapshot.stateTone} />
                      <strong>{snapshot.stateLabel}</strong>
                      <span>{snapshot.notice}</span>
                      <small>{snapshot.freshness}</small>
                    </div>
                    {snapshot.state === "loading" ? (
                      <div className="loading-lines" role="status" aria-label="Loading conversation content"><span /><span /><span /></div>
                    ) : messages.length ? (
                      messages.map((message) => <Message key={message.id} message={message} />)
                    ) : (
                      <EmptyConversation onSuggestion={useSuggestion} />
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
                    onKeyDown={(event) => {
                      if ((event.ctrlKey || event.metaKey) && event.key === "Enter") sendDraft();
                    }}
                    rows={2}
                    placeholder="Ask the project agent…"
                    aria-label="Message to project agent"
                  />
                  <div className="composer-toolbar">
                    <div className="composer-tools">
                      <IconButton label="Add context" icon={Plus} active={composerPopover === "context"} onClick={() => setComposerPopover((open) => open === "context" ? null : "context")} />
                      <IconButton label="Plugins" icon={Plug} active={composerPopover === "plugins"} onClick={() => setComposerPopover((open) => open === "plugins" ? null : "plugins")} />
                      <button
                        ref={accessTriggerRef}
                        type="button"
                        className="composer-setting"
                        aria-expanded={composerPopover === "access"}
                        aria-controls="composer-access-popover"
                        aria-haspopup="dialog"
                        onClick={() => setComposerPopover((open) => open === "access" ? null : "access")}
                      >
                        <ShieldCheck size={14} aria-hidden="true" />{accessMode === "full" ? "Full access" : "Auto-approve"}<CaretDown size={11} aria-hidden="true" />
                      </button>
                    </div>
                    <div className="composer-send">
                      <button
                        ref={runtimeTriggerRef}
                        type="button"
                        className="runtime-setting"
                        aria-expanded={composerPopover === "runtime"}
                        aria-controls="composer-runtime-popover"
                        aria-haspopup="dialog"
                        onClick={() => setComposerPopover((open) => open === "runtime" ? null : "runtime")}
                      >
                        <span>Codex</span><small>{reasoning === "high" ? "High" : "Medium"}</small><CaretDown size={11} aria-hidden="true" />
                      </button>
                      <IconButton label="Voice input — available in a later milestone" icon={Microphone} disabled />
                      {snapshot?.canStop ? (
                        <button type="button" className="send-button stop-button" onClick={stopGeneration} aria-label={`Stop generation for session "${selectedTitle}"`} title="Stop generation"><Stop size={15} weight="fill" /></button>
                      ) : (
                        <button type="button" className="send-button" onClick={sendDraft} disabled={!draft.trim()} aria-label="Send message" title="Send with Ctrl Enter"><ArrowUp size={17} weight="bold" /></button>
                      )}
                    </div>
                  </div>
                </div>

                {composerPopover === "context" && (
                  <div className="composer-popover popover-left" role="dialog" aria-label="Add context">
                    <strong>Add context</strong><p>Context effects arrive with typed local IPC.</p>
                    <button type="button" disabled><Plus size={14} />Files or folders<span>Later</span></button>
                    <button type="button" disabled><LinkSimple size={14} />URL or artifact<span>Later</span></button>
                  </div>
                )}
                {composerPopover === "plugins" && (
                  <div className="composer-popover popover-left popover-plugins" role="dialog" aria-label="Plugins">
                    <strong>Plugins</strong><p>No plugins are attached to this session.</p>
                    <button type="button" disabled><Plug size={14} />Browse plugins<span>Later</span></button>
                  </div>
                )}
                {composerPopover === "access" && (
                  <div id="composer-access-popover" className="composer-popover popover-left popover-access" role="dialog" aria-label="Agent access">
                    <strong>Agent access</strong>
                    <button ref={accessFirstRef} type="button" className={accessMode === "full" ? "is-selected" : ""} onClick={() => { setAccessMode("full"); setComposerPopover(null); accessTriggerRef.current?.focus(); }}><ShieldCheck size={14} />Full access{accessMode === "full" && <CheckCircle size={14} />}</button>
                    <button type="button" className={accessMode === "auto" ? "is-selected" : ""} onClick={() => { setAccessMode("auto"); setComposerPopover(null); accessTriggerRef.current?.focus(); }}><Command size={14} />Auto-approve{accessMode === "auto" && <CheckCircle size={14} />}</button>
                  </div>
                )}
                {composerPopover === "runtime" && (
                  <div id="composer-runtime-popover" className="composer-popover popover-right runtime-popover" role="dialog" aria-label="Agent runtime">
                    <strong>Agent runtime</strong>
                    <div className="runtime-row"><span><Robot size={14} />Source</span><b>Codex SDK</b></div>
                    <div className="runtime-row"><span><Brain size={14} />Model</span><b>Provider default</b></div>
                    <div className="runtime-choice"><span><Gauge size={14} />Reasoning</span><div><button ref={runtimeFirstRef} type="button" className={reasoning === "medium" ? "is-selected" : ""} onClick={() => setReasoning("medium")}>Medium</button><button type="button" className={reasoning === "high" ? "is-selected" : ""} onClick={() => setReasoning("high")}>High</button></div></div>
                    <div className="runtime-row"><span>Speed</span><b>Provider managed</b></div>
                  </div>
                )}
              </div>
            </section>
          </>
        ) : (
          <ArtifactViewer surface={surface} onClose={() => openSurface({ kind: "chat" })} />
        )}
      </main>

      <aside className="resource-area" aria-label="Workspace resources">
        {resourcesOpen ? (
          <div className="resource-panel">
            <header className="resource-header"><h2>Workspace</h2><IconButton label="Collapse workspace resources" icon={X} onClick={() => setResourcesOpen(false)} /></header>

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
                <button type="button" className="resource-row" onClick={() => openSurface({ kind: "source", title: "Owner direction v2", subtitle: "docs/design/WP-M01-003/…" })}><LinkSimple size={17} aria-hidden="true" /><span><strong>Owner direction v2</strong><small>Current visual contract</small></span></button>
                <button type="button" className="resource-row" onClick={() => openSurface({ kind: "source", title: "Draft pull request", subtitle: "github.com/Andrey-Good/…/pull/12" })}><GithubLogo size={17} aria-hidden="true" /><span><strong>Draft pull request</strong><small>PR #12</small></span></button>
              </section>
            </div>

            <footer className="resource-footer">
              <label className="fixture-select"><span>Preview</span><select value={fixture} onChange={(event) => setFixture(event.target.value as FixtureId)} aria-label="Preview state">{fixtureIds.map((id) => <option key={id} value={id}>{fixtureLabels[id]}</option>)}</select><CaretDown size={12} aria-hidden="true" /></label>
              <span className="git-status"><GitBranch size={13} aria-hidden="true" />codex/wp-m01-003 · clean</span>
            </footer>
          </div>
        ) : (
          <nav className="resource-dock" aria-label="Collapsed workspace resources">
            <IconButton label="Open results" icon={FileText} onClick={() => { setResourcesOpen(true); openSurface({ kind: "report", title: "Project Chat checkpoint", subtitle: "WP-M01-003 · owner review" }); }} />
            <IconButton label="Open subagents" icon={Robot} onClick={() => setResourcesOpen(true)} />
            <IconButton label="Open browser" icon={Globe} onClick={() => { setResourcesOpen(true); openSurface({ kind: "browser", title: "Dennett", subtitle: "127.0.0.1:5173" }); }} />
            <IconButton label="Open sources" icon={LinkSimple} onClick={() => setResourcesOpen(true)} />
          </nav>
        )}
      </aside>

      <div className="sr-only" role="status" aria-live="polite" aria-atomic="true">{announcement}</div>

      {commandOpen && (
        <div className="dialog-backdrop" onMouseDown={closeCommandCenter}>
          <div ref={commandDialogRef} className="command-dialog" role="dialog" aria-modal="true" aria-label="Command Center" onKeyDown={trapCommandFocus} onMouseDown={(event) => event.stopPropagation()}>
            <label><MagnifyingGlass size={18} aria-hidden="true" /><input ref={commandRef} placeholder="Search chats, settings or commands…" aria-label="Command Center search" /></label>
            <div className="command-results">
              <span>QUICK ACTIONS</span>
              <button type="button" onClick={() => { closeCommandCenter(); startNewChat(); }}><Plus size={16} />New project chat<kbd>Enter</kbd></button>
              <button type="button" onClick={() => { closeCommandCenter(); setResourcesOpen(true); }}><Browsers size={16} />Open workspace resources</button>
              <button type="button" onClick={() => { closeCommandCenter(); setResourcesOpen(true); openSurface({ kind: "browser", title: "Dennett", subtitle: "127.0.0.1:5173" }); }}><Globe size={16} />Open local preview</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
