import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import { ArrowUp, Check, FolderOpen, FolderSimple, House, X } from "@phosphor-icons/react";

import { useWorkspace } from "../state/WorkspaceProvider";
import { useModalFocusTrap } from "./useModalFocusTrap";

interface DevDirsResponse {
  current: string;
  parent: string | null;
  home?: string;
  temp?: string;
  dirs: Array<{ name: string; path: string }>;
  error?: string;
}

export function DevFolderPickerModal() {
  const { state, actions } = useWorkspace();
  const [listing, setListing] = useState<DevDirsResponse | null>(null);
  const [pathDraft, setPathDraft] = useState("");
  const [loading, setLoading] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const dialogRef = useRef<HTMLDivElement | null>(null);
  const requestSeqRef = useRef(0);
  const userEditedDraftRef = useRef(false);

  useModalFocusTrap(dialogRef, Boolean(state.folderPickerIntent), actions.closeFolderPicker);

  const loadDirectory = useCallback(async (targetPath?: string): Promise<DevDirsResponse | null> => {
    const seq = ++requestSeqRef.current;
    const isInitialScan = targetPath === undefined;
    if (!isInitialScan) {
      userEditedDraftRef.current = false;
    }
    setLoading(true);
    setError(null);
    try {
      const qs = targetPath ? `?path=${encodeURIComponent(targetPath)}` : "";
      const resp = await fetch(`/__dennett_dev_dirs${qs}`, {
        headers: {
          "X-Dennett-Dev-Picker": "1",
        },
      });
      const data = (await resp.json()) as DevDirsResponse;
      if (seq !== requestSeqRef.current) {
        return null;
      }
      if (!resp.ok || data.error) {
        setError(data.error ?? `Не удалось открыть каталог (HTTP ${resp.status})`);
        return null;
      }
      setListing(data);
      if (!isInitialScan || !userEditedDraftRef.current) {
        setPathDraft(data.current);
      }
      return data;
    } catch {
      if (seq !== requestSeqRef.current) {
        return null;
      }
      setError("Локальный обозреватель папок доступен только в dev-режиме Vite.");
      return null;
    } finally {
      if (seq === requestSeqRef.current) {
        setLoading(false);
      }
    }
  }, []);

  useEffect(() => {
    if (state.folderPickerIntent) {
      userEditedDraftRef.current = false;
      setPathDraft("");
      setError(null);
      setConfirming(false);
      void loadDirectory();
    } else {
      requestSeqRef.current += 1;
    }
  }, [loadDirectory, state.folderPickerIntent]);

  if (!state.folderPickerIntent) {
    return null;
  }

  const title =
    state.folderPickerIntent.mode === "attach-project"
      ? "Привязать рабочую папку"
      : state.folderPickerIntent.mode === "rebind-project"
        ? "Перепривязать рабочую папку"
        : state.folderPickerIntent.mode === "add-capability"
          ? state.folderPickerIntent.kind === "plugin"
            ? "Выбрать папку плагина"
            : "Выбрать папку навыка"
          : "Выбрать папку проекта";

  const subtitle =
    state.folderPickerIntent.mode === "add-capability"
      ? "Выберите локальный каталог навыка или плагина (только чтение)"
      : "Выберите локальный каталог для привязки к проекту";

  const handleNavigateSubmit = (event: FormEvent) => {
    event.preventDefault();
    if (pathDraft.trim()) {
      void loadDirectory(pathDraft.trim());
    }
  };

  const trimmedDraft = pathDraft.trim();
  const selectedPath = trimmedDraft || listing?.current || "";

  const handleConfirm = async () => {
    if (!selectedPath || confirming) {
      return;
    }
    if (!listing || trimmedDraft !== listing.current) {
      setConfirming(true);
      const resolved = await loadDirectory(selectedPath);
      setConfirming(false);
      if (!resolved) {
        return;
      }
      await actions.submitFolderPickerPath(resolved.current);
      return;
    }
    await actions.submitFolderPickerPath(listing.current);
  };

  return (
    <div
      className="dialog-backdrop"
      onMouseDown={actions.closeFolderPicker}
      data-testid="dev-folder-picker-backdrop"
    >
      <div
        ref={dialogRef}
        className="floating-dialog folder-picker-dialog"
        role="dialog"
        aria-modal="true"
        aria-label={title}
        data-testid="dev-folder-picker-modal"
        tabIndex={-1}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <header className="settings-sheet__header">
          <div>
            <h2>{title}</h2>
            <p>{subtitle}</p>
          </div>
          <button
            type="button"
            className="icon-button"
            onClick={actions.closeFolderPicker}
            aria-label="Закрыть выбор папки"
          >
            <X size={16} />
          </button>
        </header>

        <div className="folder-picker__toolbar">
          <div className="folder-picker__quick">
            {listing?.parent && (
              <button
                type="button"
                className="popover-secondary-btn"
                onClick={() => void loadDirectory(listing.parent ?? undefined)}
                title="На уровень выше"
              >
                <ArrowUp size={14} aria-hidden="true" />
                Вверх
              </button>
            )}
            {listing?.home && (
              <button
                type="button"
                className="popover-secondary-btn"
                onClick={() => void loadDirectory(listing.home)}
                title="Домашняя папка"
              >
                <House size={14} aria-hidden="true" />
                Дом
              </button>
            )}
            {listing?.temp && (
              <button
                type="button"
                className="popover-secondary-btn"
                onClick={() => void loadDirectory(listing.temp)}
                title="Временная папка"
              >
                <FolderOpen size={14} aria-hidden="true" />
                Temp
              </button>
            )}
          </div>

          <form className="folder-picker__jump" onSubmit={handleNavigateSubmit}>
            <input
              type="text"
              value={pathDraft}
              onChange={(e) => {
                userEditedDraftRef.current = true;
                setPathDraft(e.target.value);
              }}
              aria-label="Путь к каталогу"
              placeholder="Путь к локальному каталогу…"
              data-testid="dev-folder-picker-path-input"
            />
            <button type="submit" className="popover-secondary-btn">
              Перейти
            </button>
          </form>
        </div>

        {error && <p className="folder-picker__error">{error}</p>}

        <div className="folder-picker__list" data-testid="dev-folder-picker-list">
          {loading ? (
            <p className="resource-empty">Загрузка каталогов…</p>
          ) : !listing || listing.dirs.length === 0 ? (
            <p className="resource-empty">В этом каталоге нет подпапок.</p>
          ) : (
            listing.dirs.map((dir) => (
              <button
                key={dir.path}
                type="button"
                className="folder-picker__row"
                onClick={() => void loadDirectory(dir.path)}
              >
                <FolderSimple size={15} aria-hidden="true" />
                <span>{dir.name}</span>
              </button>
            ))
          )}
        </div>

        <footer className="folder-picker__footer">
          <span className="folder-picker__selected" title={selectedPath}>
            {selectedPath || "Каталог не выбран"}
          </span>
          <div className="folder-picker__actions">
            <button
              type="button"
              className="popover-secondary-btn"
              onClick={actions.closeFolderPicker}
            >
              Отмена
            </button>
            <button
              type="button"
              className="popover-primary-btn"
              disabled={!selectedPath || confirming}
              data-testid="dev-folder-picker-confirm"
              onClick={() => void handleConfirm()}
            >
              <Check size={14} aria-hidden="true" />
              {confirming ? "Проверка…" : "Выбрать эту папку"}
            </button>
          </div>
        </footer>
      </div>
    </div>
  );
}
