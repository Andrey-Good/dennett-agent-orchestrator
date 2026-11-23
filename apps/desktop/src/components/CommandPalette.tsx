import { useEffect, useMemo, useRef, useState } from "react";
import {
  Browsers,
  ChatsCircle,
  FolderOpen,
  FolderPlus,
  GearSix,
  MagnifyingGlass,
  Plus,
} from "@phosphor-icons/react";

import { useWorkspace } from "../state/WorkspaceProvider";

export function CommandPalette() {
  const { state, actions } = useWorkspace();
  const [query, setQuery] = useState("");
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (state.isCommandOpen) {
      setQuery("");
      requestAnimationFrame(() => inputRef.current?.focus());
    }
  }, [state.isCommandOpen]);

  const trimmed = query.trim().toLowerCase();

  const matchingSessions = useMemo(() => {
    if (!trimmed) {
      return state.sessions.slice(0, 6);
    }
    return state.sessions
      .filter((s) => s.title.toLowerCase().includes(trimmed))
      .slice(0, 8);
  }, [state.sessions, trimmed]);

  if (!state.isCommandOpen) {
    return null;
  }

  const close = () => actions.setCommandOpen(false);
  const connected = Boolean(state.healthSnapshot?.connected);

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
      >
        <label className="command-dialog__search">
          <MagnifyingGlass size={17} aria-hidden="true" />
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Поиск по чатам или быстрые действия…"
            aria-label="Поиск в командной палитре"
          />
        </label>

        <div className="command-results">
          {matchingSessions.length > 0 && (
            <>
              <span className="command-section-label">ЧАТЫ</span>
              {matchingSessions.map((session) => (
                <button
                  key={session.session_id}
                  type="button"
                  onClick={() => {
                    close();
                    void actions.selectSession(session.session_id);
                  }}
                >
                  <ChatsCircle size={15} aria-hidden="true" />
                  <span>{session.title}</span>
                </button>
              ))}
            </>
          )}

          <span className="command-section-label">БЫСТРЫЕ ДЕЙСТВИЯ</span>

          <button
            type="button"
            disabled={!connected}
            onClick={() => {
              close();
              void actions.createBlankChat();
            }}
          >
            <Plus size={15} aria-hidden="true" />
            <span>Новый чат</span>
            <kbd>Ctrl N</kbd>
          </button>

          <button
            type="button"
            disabled={!connected}
            onClick={() => {
              close();
              void actions.requestFolderPicker({ mode: "create-project" });
            }}
          >
            <FolderOpen size={15} aria-hidden="true" />
            <span>Новый проект: выбрать папку…</span>
          </button>

          <button
            type="button"
            disabled={!connected}
            onClick={() => {
              close();
              void actions.createFolderlessProject();
            }}
          >
            <FolderPlus size={15} aria-hidden="true" />
            <span>Новый проект: пока без папки</span>
          </button>

          <button
            type="button"
            onClick={() => {
              close();
              actions.toggleRightPanel();
            }}
          >
            <Browsers size={15} aria-hidden="true" />
            <span>Переключить панель Workspace</span>
            <kbd>Ctrl \</kbd>
          </button>

          <button
            type="button"
            onClick={() => {
              close();
              actions.setSettingsOpen(true);
            }}
          >
            <GearSix size={15} aria-hidden="true" />
            <span>Настройки и подключения</span>
          </button>
        </div>
      </div>
    </div>
  );
}
