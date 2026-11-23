import { useEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import {
  Browsers,
  ChatsCircle,
  FolderDashed,
  FolderOpen,
  FolderPlus,
  FolderSimple,
  GearSix,
  MagnifyingGlass,
  Plus,
} from "@phosphor-icons/react";

import { useWorkspace } from "../state/WorkspaceProvider";

interface PaletteItem {
  id: string;
  section: "projects" | "sessions" | "actions";
  label: string;
  meta?: string;
  kbd?: string;
  disabled?: boolean;
  icon: ReactNode;
  run: () => void;
}

export function CommandPalette() {
  const { state, actions } = useWorkspace();
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (state.isCommandOpen) {
      setQuery("");
      setActiveIndex(0);
      requestAnimationFrame(() => inputRef.current?.focus());
    }
  }, [state.isCommandOpen]);

  const close = () => actions.setCommandOpen(false);
  const connected = Boolean(state.healthSnapshot?.connected);
  const trimmed = query.trim().toLowerCase();

  const projectById = useMemo(() => {
    const map = new Map<string, (typeof state.projects)[number]>();
    for (const p of state.projects) {
      map.set(p.project_id, p);
    }
    return map;
  }, [state.projects]);

  const matchingProjects = useMemo(() => {
    if (!trimmed) {
      return state.projects.slice(0, 4);
    }
    return state.projects
      .filter(
        (p) =>
          p.name.toLowerCase().includes(trimmed) ||
          (p.workspace_path?.toLowerCase().includes(trimmed) ?? false),
      )
      .slice(0, 6);
  }, [state.projects, trimmed]);

  const matchingSessions = useMemo(() => {
    if (!trimmed) {
      return state.sessions.slice(0, 6);
    }
    return state.sessions
      .filter((s) => {
        const proj = projectById.get(s.project_id);
        return (
          s.title.toLowerCase().includes(trimmed) ||
          (proj?.name.toLowerCase().includes(trimmed) ?? false)
        );
      })
      .slice(0, 8);
  }, [projectById, state.sessions, trimmed]);

  const items = useMemo<PaletteItem[]>(() => {
    const list: PaletteItem[] = [];

    for (const project of matchingProjects) {
      const firstSession = state.sessions.find((s) => s.project_id === project.project_id);
      list.push({
        id: `project:${project.project_id}`,
        section: "projects",
        label: project.name,
        meta: project.workspace_path ? "папка" : "без папки",
        icon: project.workspace_path ? (
          <FolderSimple size={15} aria-hidden="true" />
        ) : (
          <FolderDashed size={15} aria-hidden="true" />
        ),
        run: () => {
          close();
          if (firstSession) {
            void actions.selectSession(firstSession.session_id);
          } else {
            void actions.createBlankChat(project.project_id);
          }
        },
      });
    }

    for (const session of matchingSessions) {
      const proj = projectById.get(session.project_id);
      list.push({
        id: `session:${session.session_id}`,
        section: "sessions",
        label: session.title,
        meta: proj?.name,
        icon: <ChatsCircle size={15} aria-hidden="true" />,
        run: () => {
          close();
          void actions.selectSession(session.session_id);
        },
      });
    }

    const quickActions: PaletteItem[] = [
      {
        id: "action:new-chat",
        section: "actions",
        label: "Новый чат",
        kbd: "Ctrl N",
        disabled: !connected,
        icon: <Plus size={15} aria-hidden="true" />,
        run: () => {
          close();
          void actions.createBlankChat();
        },
      },
      {
        id: "action:pick-folder",
        section: "actions",
        label: "Новый проект: выбрать папку…",
        disabled: !connected,
        icon: <FolderOpen size={15} aria-hidden="true" />,
        run: () => {
          close();
          void actions.requestFolderPicker({ mode: "create-project" });
        },
      },
      {
        id: "action:folderless",
        section: "actions",
        label: "Новый проект: пока без папки",
        disabled: !connected,
        icon: <FolderPlus size={15} aria-hidden="true" />,
        run: () => {
          close();
          void actions.createFolderlessProject();
        },
      },
      {
        id: "action:toggle-workspace",
        section: "actions",
        label: "Переключить панель Workspace",
        kbd: "Ctrl \\",
        icon: <Browsers size={15} aria-hidden="true" />,
        run: () => {
          close();
          actions.toggleRightPanel();
        },
      },
      {
        id: "action:settings",
        section: "actions",
        label: "Настройки и подключения",
        icon: <GearSix size={15} aria-hidden="true" />,
        run: () => {
          close();
          actions.setSettingsOpen(true);
        },
      },
    ];

    for (const act of quickActions) {
      if (!trimmed || act.label.toLowerCase().includes(trimmed)) {
        list.push(act);
      }
    }

    return list;
  }, [actions, connected, matchingProjects, matchingSessions, projectById, state.sessions, trimmed]);

  useEffect(() => {
    setActiveIndex(0);
  }, [trimmed, items.length]);

  if (!state.isCommandOpen) {
    return null;
  }

  const enabledIndices = items
    .map((item, idx) => (item.disabled ? -1 : idx))
    .filter((idx) => idx >= 0);

  const clampedIndex =
    items[activeIndex] && !items[activeIndex].disabled
      ? activeIndex
      : (enabledIndices[0] ?? -1);

  const moveSelection = (delta: 1 | -1) => {
    if (enabledIndices.length === 0) {
      return;
    }
    const pos = enabledIndices.indexOf(clampedIndex);
    const nextPos =
      pos === -1
        ? 0
        : (pos + delta + enabledIndices.length) % enabledIndices.length;
    setActiveIndex(enabledIndices[nextPos] ?? 0);
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      moveSelection(1);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      moveSelection(-1);
    } else if (event.key === "Enter") {
      if (clampedIndex >= 0 && items[clampedIndex] && !items[clampedIndex].disabled) {
        event.preventDefault();
        items[clampedIndex].run();
      }
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      close();
    }
  };

  const projectItems = items
    .map((item, idx) => ({ item, idx }))
    .filter(({ item }) => item.section === "projects");
  const sessionItems = items
    .map((item, idx) => ({ item, idx }))
    .filter(({ item }) => item.section === "sessions");
  const actionItems = items
    .map((item, idx) => ({ item, idx }))
    .filter(({ item }) => item.section === "actions");

  const noDataMatches =
    trimmed.length > 0 && projectItems.length === 0 && sessionItems.length === 0;

  return (
    <div
      className="dialog-backdrop"
      onMouseDown={close}
      data-testid="command-palette-backdrop"
    >
      <div
        className="floating-dialog command-dialog"
        role="dialog"
        aria-modal="true"
        aria-label="Командная палитра"
        data-testid="command-palette"
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={handleKeyDown}
      >
        <label className="command-dialog__search">
          <MagnifyingGlass size={17} aria-hidden="true" />
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Поиск по проектам, чатам или действиям…"
            aria-label="Поиск в командной палитре"
          />
        </label>

        <div className="command-results" role="listbox" aria-label="Результаты поиска">
          {noDataMatches && (
            <p className="command-empty" data-testid="command-palette-empty">
              По запросу «{query.trim()}» проекты и чаты не найдены.
            </p>
          )}

          {projectItems.length > 0 && (
            <>
              <span className="command-section-label">ПРОЕКТЫ</span>
              {projectItems.map(({ item, idx }) => {
                const isHighlighted = idx === clampedIndex;
                return (
                  <button
                    key={item.id}
                    type="button"
                    role="option"
                    aria-selected={isHighlighted}
                    className={isHighlighted ? "is-highlighted" : ""}
                    data-testid={`palette-item-${item.id}`}
                    onMouseEnter={() => setActiveIndex(idx)}
                    onClick={item.run}
                  >
                    {item.icon}
                    <span className="command-item-label">{item.label}</span>
                    {item.meta && <small className="command-item-meta">{item.meta}</small>}
                  </button>
                );
              })}
            </>
          )}

          {sessionItems.length > 0 && (
            <>
              <span className="command-section-label">ЧАТЫ</span>
              {sessionItems.map(({ item, idx }) => {
                const isHighlighted = idx === clampedIndex;
                return (
                  <button
                    key={item.id}
                    type="button"
                    role="option"
                    aria-selected={isHighlighted}
                    className={isHighlighted ? "is-highlighted" : ""}
                    data-testid={`palette-item-${item.id}`}
                    onMouseEnter={() => setActiveIndex(idx)}
                    onClick={item.run}
                  >
                    {item.icon}
                    <span className="command-item-label">{item.label}</span>
                    {item.meta && <small className="command-item-meta">{item.meta}</small>}
                  </button>
                );
              })}
            </>
          )}

          {actionItems.length > 0 && (
            <>
              <span className="command-section-label">БЫСТРЫЕ ДЕЙСТВИЯ</span>
              {actionItems.map(({ item, idx }) => {
                const isHighlighted = idx === clampedIndex;
                return (
                  <button
                    key={item.id}
                    type="button"
                    role="option"
                    aria-selected={isHighlighted}
                    disabled={item.disabled}
                    className={isHighlighted ? "is-highlighted" : ""}
                    data-testid={`palette-item-${item.id}`}
                    onMouseEnter={() => !item.disabled && setActiveIndex(idx)}
                    onClick={item.run}
                  >
                    {item.icon}
                    <span className="command-item-label">{item.label}</span>
                    {item.kbd && <kbd>{item.kbd}</kbd>}
                  </button>
                );
              })}
            </>
          )}
        </div>
      </div>
    </div>
  );
}
