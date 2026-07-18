import React from "react";
import type { IconProps } from "@phosphor-icons/react";
import {
  ArrowUp,
  At,
  Bell,
  Books,
  BracketsCurly,
  CaretDown,
  CaretLeft,
  CaretRight,
  ChatCircleDots,
  CheckCircle,
  CircleNotch,
  Code,
  Command,
  DotsThree,
  FileCode,
  FolderSimple,
  GearSix,
  GitBranch,
  HardDrives,
  House,
  Info,
  MagnifyingGlass,
  Microphone,
  Paperclip,
  Plus,
  Pulse,
  Robot,
  SidebarSimple,
  SquaresFour,
  Stop,
  TerminalWindow,
  Tray,
  WarningCircle,
  X,
} from "@phosphor-icons/react";
import {
  fixtureClient,
  fixtureIds,
  fixtureLabels,
  type ChatMessage,
  type FixtureId,
  type FixtureTone,
  type ProjectChatSnapshot,
} from "./fixtures/projectChat";
import "./styles.css";

type Icon = React.ComponentType<IconProps>;

const railItems: Array<{ label: string; icon: Icon; enabled?: boolean; badge?: number }> = [
  { label: "Home", icon: House },
  { label: "Orchestrator", icon: Robot },
  { label: "Projects", icon: FolderSimple, enabled: true },
  { label: "Action Inbox", icon: Tray, badge: 2 },
  { label: "Agent Radar", icon: Pulse },
  { label: "Library", icon: Books },
];

const sessions = [
  { id: "screen", title: "First Project Chat screen", time: "now", active: true },
  { id: "protocol", title: "M01 protocol epoch", time: "2h", active: false },
  { id: "runtime", title: "Codex runtime canary", time: "1d", active: false },
];

function IconButton({
  label,
  icon: IconComponent,
  onClick,
  active = false,
  disabled = false,
}: {
  label: string;
  icon: Icon;
  onClick?: () => void;
  active?: boolean;
  disabled?: boolean;
}): React.JSX.Element {
  return (
    <button
      type="button"
      className={`icon-button${active ? " is-active" : ""}`}
      aria-label={label}
      title={label}
      onClick={onClick}
      disabled={disabled}
    >
      <IconComponent size={18} weight={active ? "fill" : "regular"} aria-hidden="true" />
    </button>
  );
}

function StateIcon({ tone }: { tone: FixtureTone }): React.JSX.Element {
  if (tone === "danger" || tone === "warning") {
    return <WarningCircle size={16} weight="fill" aria-hidden="true" />;
  }
  if (tone === "active") {
    return <CircleNotch size={16} className="spin" aria-hidden="true" />;
  }
  if (tone === "good") {
    return <CheckCircle size={16} weight="fill" aria-hidden="true" />;
  }
  return <Info size={16} weight="fill" aria-hidden="true" />;
}

function Message({ message }: { message: ChatMessage }): React.JSX.Element {
  return (
    <article className={`message message--${message.author}`} aria-label={`${message.author} message`}>
      {message.author === "agent" && (
        <div className="agent-mark" aria-hidden="true">
          <BracketsCurly size={14} weight="bold" />
        </div>
      )}
      <div className="message-copy">
        {message.paragraphs.map((paragraph) => (
          <p key={paragraph}>{paragraph}</p>
        ))}
        {message.bullets && (
          <ul>
            {message.bullets.map((item) => (
              <li key={item}>{item}</li>
            ))}
          </ul>
        )}
        <span className="message-time">{message.timestamp}</span>
      </div>
    </article>
  );
}

function EmptyConversation(): React.JSX.Element {
  return (
    <div className="empty-state">
      <div className="empty-state__icon"><ChatCircleDots size={26} aria-hidden="true" /></div>
      <h2>Start with the project itself</h2>
      <p>Ask the agent to inspect, explain or change this workspace. Project context stays attached.</p>
      <div className="prompt-suggestions" aria-label="Prompt suggestions">
        <button type="button">Summarize the current milestone</button>
        <button type="button">Find the next eligible package</button>
      </div>
    </div>
  );
}

export function App(): React.JSX.Element {
  const [fixture, setFixture] = React.useState<FixtureId>("streaming");
  const [snapshot, setSnapshot] = React.useState<ProjectChatSnapshot | null>(null);
  const [sidebarOpen, setSidebarOpen] = React.useState(true);
  const [inspectorOpen, setInspectorOpen] = React.useState(true);
  const [sidebarTab, setSidebarTab] = React.useState<"sessions" | "project">("sessions");
  const [inspectorTab, setInspectorTab] = React.useState<"result" | "context">("result");
  const [selectedSession, setSelectedSession] = React.useState("screen");
  const [draft, setDraft] = React.useState("");
  const [localMessages, setLocalMessages] = React.useState<ChatMessage[]>([]);
  const [announcement, setAnnouncement] = React.useState("Project Chat opened");
  const [commandOpen, setCommandOpen] = React.useState(false);
  const searchRef = React.useRef<HTMLInputElement>(null);
  const commandRef = React.useRef<HTMLInputElement>(null);

  React.useEffect(() => {
    let current = true;
    setSnapshot(null);
    fixtureClient.readProjectChat(fixture).then((next) => {
      if (!current) return;
      setSnapshot(next);
      setAnnouncement(`${next.stateLabel}. ${next.phase}.`);
    });
    return () => {
      current = false;
    };
  }, [fixture]);

  React.useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setCommandOpen(true);
      }
      if (event.key === "Escape") setCommandOpen(false);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  React.useEffect(() => {
    if (commandOpen) commandRef.current?.focus();
  }, [commandOpen]);

  const sendDraft = () => {
    const content = draft.trim();
    if (!content) return;
    setLocalMessages((items) => [
      ...items,
      { id: `local-${items.length}`, author: "user", paragraphs: [content], timestamp: "now" },
    ]);
    setDraft("");
    setAnnouncement("Draft added to this local preview. No runtime command was sent.");
  };

  const stopGeneration = () => {
    setFixture("stopped");
    setAnnouncement('Stop requested for session "First Project Chat screen".');
  };

  const messages = [...(snapshot?.messages ?? []), ...localMessages];
  const selectedTitle = sessions.find((session) => session.id === selectedSession)?.title ?? sessions[0].title;

  return (
    <div className={`workbench${sidebarOpen ? "" : " sidebar-collapsed"}${inspectorOpen ? "" : " inspector-collapsed"}`}>
      <header className="titlebar">
        <div className="titlebar__brand" aria-label="Dennett">
          <span className="brand-mark"><BracketsCurly size={17} weight="bold" aria-hidden="true" /></span>
          <span>Dennett</span>
        </div>
        <div className="titlebar__nav">
          <IconButton label="Back" icon={CaretLeft} disabled />
          <IconButton label="Forward" icon={CaretRight} disabled />
          <button type="button" className="breadcrumbs" onClick={() => searchRef.current?.focus()}>
            <span>Projects</span><span>/</span><strong>dennett-agent-orchestrator</strong>
          </button>
        </div>
        <button type="button" className="command-button" onClick={() => setCommandOpen(true)}>
          <MagnifyingGlass size={15} aria-hidden="true" />
          <span>Search or run a command</span>
          <kbd>Ctrl K</kbd>
        </button>
        <div className="titlebar__status">
          <span className="node-status"><span className="status-dot" />Local node</span>
          <IconButton label="Voice capture" icon={Microphone} />
          <IconButton label="Notifications, 2 unread" icon={Bell} />
          <button type="button" className="profile-button" aria-label="Open profile">AG</button>
        </div>
      </header>

      <nav className="activity-rail" aria-label="Primary navigation">
        <div className="rail-main">
          <IconButton label="Application menu" icon={SquaresFour} />
          <div className="rail-rule" />
          {railItems.map((item) => (
            <div className="rail-item" key={item.label}>
              <IconButton
                label={item.enabled ? item.label : `${item.label} — available in a later milestone`}
                icon={item.icon}
                active={item.label === "Projects"}
                disabled={!item.enabled}
              />
              {!!item.badge && <span className="rail-badge" aria-hidden="true">{item.badge}</span>}
            </div>
          ))}
        </div>
        <div className="rail-bottom">
          <IconButton label="System status" icon={HardDrives} />
          <IconButton label="Settings" icon={GearSix} disabled />
        </div>
      </nav>

      <aside className="project-sidebar" aria-label="Project navigation">
        <div className="sidebar-heading">
          <div><span className="eyebrow">PROJECT</span><strong>dennett-agent-orchestrator</strong></div>
          <IconButton label="Collapse project sidebar" icon={SidebarSimple} onClick={() => setSidebarOpen(false)} />
        </div>
        <div className="segmented-control" role="group" aria-label="Project sidebar view">
          <button type="button" className={sidebarTab === "sessions" ? "is-active" : ""} onClick={() => setSidebarTab("sessions")}>Sessions</button>
          <button type="button" className={sidebarTab === "project" ? "is-active" : ""} onClick={() => setSidebarTab("project")}>Project</button>
        </div>
        <label className="sidebar-search">
          <MagnifyingGlass size={15} aria-hidden="true" />
          <span className="sr-only">Search project</span>
          <input ref={searchRef} type="search" placeholder="Search" />
        </label>
        {sidebarTab === "sessions" ? (
          <div className="session-list">
            <div className="list-heading"><span>RECENT</span><button type="button" aria-label="Create new session"><Plus size={15} /></button></div>
            {sessions.map((session) => (
              <button
                type="button"
                key={session.id}
                className={selectedSession === session.id ? "session-row is-active" : "session-row"}
                onClick={() => setSelectedSession(session.id)}
              >
                <span className="session-presence" aria-hidden="true" />
                <span><strong>{session.title}</strong><small>{session.id === "screen" ? "Direct project agent" : "Completed session"}</small></span>
                <time>{session.time}</time>
              </button>
            ))}
          </div>
        ) : (
          <div className="project-sections">
            <button type="button"><FileCode size={16} /><span><strong>Files</strong><small>Workspace context</small></span></button>
            <button type="button"><Robot size={16} /><span><strong>Agent</strong><small>Direct · Codex</small></span></button>
            <button type="button"><GitBranch size={16} /><span><strong>Branch</strong><small>codex/wp-m01-003</small></span></button>
          </div>
        )}
        <div className="sidebar-footer">
          <span className="workspace-health"><CheckCircle size={15} weight="fill" />Workspace healthy</span>
          <span>main · clean</span>
        </div>
      </aside>

      {!sidebarOpen && (
        <button type="button" className="restore-sidebar" onClick={() => setSidebarOpen(true)} aria-label="Open project sidebar">
          <SidebarSimple size={18} />
        </button>
      )}

      <main className="chat-workspace">
        <header className="chat-header">
          <div className="chat-title">
            <span className="eyebrow">DIRECT PROJECT CHAT</span>
            <div><h1>{selectedTitle}</h1><span className="header-chip"><Robot size={13} />Codex</span></div>
          </div>
          <div className="chat-actions">
            <label className="fixture-select">
              <span className="sr-only">Preview state</span>
              <select value={fixture} onChange={(event) => setFixture(event.target.value as FixtureId)}>
                {fixtureIds.map((id) => <option key={id} value={id}>{fixtureLabels[id]}</option>)}
              </select>
              <CaretDown size={13} aria-hidden="true" />
            </label>
            <IconButton label={inspectorOpen ? "Hide context inspector" : "Show context inspector"} icon={Code} active={inspectorOpen} onClick={() => setInspectorOpen((open) => !open)} />
            <IconButton label="More session actions" icon={DotsThree} />
          </div>
        </header>

        <section className="conversation" aria-label="Conversation">
          <div className="conversation-inner">
            {snapshot ? (
              <>
                <div className={`state-notice tone-${snapshot.stateTone}`} role="status">
                  <StateIcon tone={snapshot.stateTone} />
                  <div><strong>{snapshot.stateLabel}</strong><span>{snapshot.notice}</span></div>
                  <span className="state-freshness">{snapshot.freshness}</span>
                </div>
                {messages.length ? messages.map((message) => <Message key={message.id} message={message} />) : <EmptyConversation />}
                {snapshot.fixture === "loading" && <div className="loading-lines" aria-hidden="true"><span /><span /><span /></div>}
              </>
            ) : (
              <div className="loading-lines" aria-label="Loading Project Chat"><span /><span /><span /></div>
            )}
          </div>
        </section>

        <section className="composer-region" aria-label="Message composer">
          <div className="composer">
            <textarea
              value={draft}
              onChange={(event) => setDraft(event.target.value)}
              onKeyDown={(event) => {
                if ((event.ctrlKey || event.metaKey) && event.key === "Enter") sendDraft();
              }}
              rows={2}
              placeholder="Ask the project agent…"
              aria-label="Message to project agent"
            />
            <div className="composer-toolbar">
              <div className="composer-tools">
                <IconButton label="Add context" icon={Paperclip} />
                <IconButton label="Mention project context" icon={At} />
                <button type="button" className="compact-setting"><FolderSimple size={14} />Project<CaretDown size={11} /></button>
              </div>
              <div className="composer-send">
                <button type="button" className="model-setting">Codex<CaretDown size={11} /></button>
                <IconButton label="Voice input" icon={Microphone} />
                {snapshot?.canStop ? (
                  <button type="button" className="send-button stop-button" onClick={stopGeneration} aria-label={`Stop generation for session "${selectedTitle}"`} title="Stop generation">
                    <Stop size={15} weight="fill" />
                  </button>
                ) : (
                  <button type="button" className="send-button" onClick={sendDraft} disabled={!draft.trim()} aria-label="Send message" title="Send with Ctrl Enter">
                    <ArrowUp size={17} weight="bold" />
                  </button>
                )}
              </div>
            </div>
          </div>
          <div className="composer-meta">
            <span>{snapshot?.phase ?? "Opening session"}</span>
            <span><GitBranch size={12} />codex/wp-m01-003-project-chat-screen</span>
          </div>
        </section>
      </main>

      {inspectorOpen && (
        <aside className="inspector" aria-label="Read-only context inspector">
          <header className="inspector-header">
            <div><span className="eyebrow">AUXILIARY PANE</span><h2>Work result</h2></div>
            <IconButton label="Close context inspector" icon={X} onClick={() => setInspectorOpen(false)} />
          </header>
          <div className="inspector-tabs" role="tablist" aria-label="Inspector content">
            <button type="button" role="tab" aria-selected={inspectorTab === "result"} onClick={() => setInspectorTab("result")}>Result</button>
            <button type="button" role="tab" aria-selected={inspectorTab === "context"} onClick={() => setInspectorTab("context")}>Context</button>
            <button type="button" role="tab" aria-selected="false" disabled title="Working changes arrive in M02">Changes</button>
          </div>
          {inspectorTab === "result" ? (
            <div className="inspector-content">
              <section className="result-card">
                <div className="result-card__heading"><span className="file-icon"><BracketsCurly size={16} /></span><div><strong>Project Chat screen</strong><small>WP-M01-003 · preview</small></div></div>
                <p>One owner-reviewable renderer surface with deterministic runtime states.</p>
                <div className="result-metrics"><span><strong>9</strong>states</span><span><strong>0</strong>effects</span><span><strong>1</strong>screen</span></div>
              </section>
              <section className="inspector-section">
                <h3>Visible contract</h3>
                <ul className="check-list"><li><CheckCircle size={15} />Direct project context</li><li><CheckCircle size={15} />Bounded state updates</li><li><CheckCircle size={15} />Read-only auxiliary view</li></ul>
              </section>
              <section className="inspector-section">
                <h3>Artifact</h3>
                <div className="code-preview"><div><TerminalWindow size={14} /><span>renderer.fixture</span><span>read only</span></div><pre>{`session: project-chat\nstate: ${fixture}\nprovider: codex\nauthority: fixture`}</pre></div>
              </section>
            </div>
          ) : (
            <div className="inspector-content">
              <section className="inspector-section"><h3>Attached context</h3><div className="context-row"><FileCode size={16} /><span><strong>Specification 60</strong><small>Project Workspace and Chat</small></span></div><div className="context-row"><Command size={16} /><span><strong>M01 execution protocol</strong><small>Owner UX gate</small></span></div></section>
              <section className="inspector-section"><h3>Runtime boundary</h3><p className="muted-copy">The renderer reads this snapshot through a provider-neutral DennettClient fake. Codex-specific types do not cross into presentation state.</p></section>
            </div>
          )}
          <footer className="inspector-footer"><Info size={14} />Git and diff actions remain disabled until M02.</footer>
        </aside>
      )}

      <div className="sr-only" role="status" aria-live="polite" aria-atomic="true">{announcement}</div>

      {commandOpen && (
        <div className="dialog-backdrop" onMouseDown={() => setCommandOpen(false)}>
          <div className="command-dialog" role="dialog" aria-modal="true" aria-label="Command Center" onMouseDown={(event) => event.stopPropagation()}>
            <label><MagnifyingGlass size={18} /><input ref={commandRef} placeholder="Type a command or search…" aria-label="Command Center search" /></label>
            <div className="command-results"><span>QUICK ACTIONS</span><button type="button" onClick={() => { setCommandOpen(false); setFixture("empty"); }}><Plus size={16} />New project chat<kbd>Enter</kbd></button><button type="button" onClick={() => { setCommandOpen(false); setInspectorOpen(true); }}><Code size={16} />Open context inspector</button></div>
          </div>
        </div>
      )}
    </div>
  );
}
