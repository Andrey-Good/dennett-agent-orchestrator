import { useEffect, useRef, useState, type FormEvent } from "react";
import {
  ArrowsClockwise,
  CaretDown,
  Check,
  FileCode,
  FileText,
  FolderOpen,
  FolderSimple,
  Plus,
  Sidebar,
} from "@phosphor-icons/react";

import { useWorkspace } from "../state/WorkspaceProvider";

const MIME_OPTIONS: Array<{ value: string; label: string }> = [
  { value: "text/markdown", label: "Markdown (text/markdown)" },
  { value: "text/plain", label: "Текст (text/plain)" },
  { value: "application/json", label: "JSON (application/json)" },
];

function formatBytes(bytes: number | null | undefined): string {
  if (bytes === null || bytes === undefined) {
    return "";
  }
  if (bytes < 1024) {
    return `${bytes} Б`;
  }
  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(1)} КБ`;
  }
  return `${(bytes / (1024 * 1024)).toFixed(1)} МБ`;
}

function isCodeFile(name: string): boolean {
  return /\.(ts|tsx|js|jsx|rs|py|json|toml|sql|css|html|yml|yaml|sh|ps1)$/i.test(name);
}

export function WorkspacePanel() {
  const { state, actions, meta } = useWorkspace();
  const [creatingArtifact, setCreatingArtifact] = useState(false);
  const [artName, setArtName] = useState("");
  const [artMime, setArtMime] = useState("text/markdown");
  const [artContent, setArtContent] = useState("");
  const [mimeOpen, setMimeOpen] = useState(false);
  const mimeAnchorRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!mimeOpen) {
      return;
    }
    const handlePointerDown = (event: MouseEvent) => {
      if (mimeAnchorRef.current && !mimeAnchorRef.current.contains(event.target as Node)) {
        setMimeOpen(false);
      }
    };
    document.addEventListener("mousedown", handlePointerDown);
    return () => document.removeEventListener("mousedown", handlePointerDown);
  }, [mimeOpen]);

  if (!state.isRightPanelOpen) {
    return null;
  }

  const artifacts = state.activeDetail?.artifacts ?? [];
  const activeProject = meta.activeProject;
  const filesData = state.projectFiles;
  const isBound = Boolean(activeProject?.workspace_path);

  const handleCreateArtifact = async (event: FormEvent) => {
    event.preventDefault();
    if (!artName.trim() || !artContent.trim()) {
      return;
    }
    const created = await actions.createArtifact({
      name: artName,
      mimeType: artMime,
      content: artContent,
    });
    if (created) {
      setArtName("");
      setArtContent("");
      setCreatingArtifact(false);
    }
  };

  const currentMimeLabel =
    MIME_OPTIONS.find((item) => item.value === artMime)?.label ?? artMime;

  return (
    <aside
      className="workspace-panel"
      aria-label="Рабочее пространство и артефакты"
      data-testid="workspace-sibling-panel"
    >
      <header className="resource-header">
        <h2>Workspace</h2>
        <div className="resource-header__actions">
          {isBound && (
            <button
              type="button"
              className="icon-button"
              onClick={() => void actions.refreshProjectFiles()}
              disabled={state.isLoadingFiles}
              aria-label="Обновить список файлов"
              title="Обновить список файлов"
            >
              <ArrowsClockwise size={15} className={state.isLoadingFiles ? "spin" : ""} />
            </button>
          )}
          <button
            type="button"
            className="icon-button resource-panel-toggle is-open"
            onClick={() => actions.setRightPanelOpen(false)}
            aria-label="Скрыть панель рабочего пространства"
            title="Скрыть Workspace (Ctrl+\)"
          >
            <Sidebar size={16} />
          </button>
        </div>
      </header>

      <div className="resource-scroll">
        <section className="resource-section" data-testid="workspace-files-section">
          <div className="resource-section__head">
            <h3>
              Файлы проекта{" "}
              {filesData && filesData.workspace_bound && <span>{filesData.entries.length}</span>}
            </h3>
          </div>

          {!activeProject ? (
            <p className="resource-empty">Нет активного проекта.</p>
          ) : !isBound ? (
            <div className="workspace-unbound-card" data-testid="workspace-unbound-notice">
              <p>Папка не привязана. Проект работает в изолированном режиме чтения.</p>
              <button
                type="button"
                className="popover-secondary-btn"
                data-testid="workspace-attach-folder-btn"
                onClick={() =>
                  void actions.requestFolderPicker({
                    mode: "attach-project",
                    projectId: activeProject.project_id,
                  })
                }
              >
                <FolderOpen size={15} aria-hidden="true" />
                Привязать папку…
              </button>
            </div>
          ) : state.isLoadingFiles && !filesData ? (
            <p className="resource-empty">Сканирование файлов…</p>
          ) : (
            <>
              <div
                className="workspace-path-caption"
                title={filesData?.workspace_path ?? activeProject.workspace_path ?? undefined}
              >
                {filesData?.workspace_path ?? activeProject.workspace_path}
              </div>

              {!filesData || filesData.entries.length === 0 ? (
                <p className="resource-empty">Папка пуста.</p>
              ) : (
                <div className="workspace-file-list" data-testid="workspace-file-list">
                  {filesData.entries.map((entry) => (
                    <div
                      key={entry.relative_path}
                      className="resource-row is-static workspace-file-row"
                      style={{ paddingLeft: `${8 + (entry.depth - 1) * 12}px` }}
                      title={entry.relative_path}
                    >
                      {entry.kind === "directory" ? (
                        <FolderSimple size={15} aria-hidden="true" />
                      ) : isCodeFile(entry.name) ? (
                        <FileCode size={15} aria-hidden="true" />
                      ) : (
                        <FileText size={15} aria-hidden="true" />
                      )}
                      <span>
                        <strong>{entry.name}</strong>
                        {entry.depth > 1 && <small>{entry.relative_path}</small>}
                      </span>
                      {entry.size_bytes !== null && entry.size_bytes !== undefined && (
                        <em>{formatBytes(entry.size_bytes)}</em>
                      )}
                    </div>
                  ))}
                  {filesData.truncated && (
                    <p className="resource-truncated">
                      Показаны первые {filesData.entries.length} элементов (лимит безопасности).
                    </p>
                  )}
                </div>
              )}
            </>
          )}
        </section>

        <section className="resource-section" data-testid="workspace-artifacts-section">
          <div className="resource-section__head">
            <h3>
              Артефакты сессии <span>{artifacts.length}</span>
            </h3>
            <button
              type="button"
              className="icon-button icon-button--sm"
              onClick={() => setCreatingArtifact((prev) => !prev)}
              disabled={!state.selectedSessionId || !state.healthSnapshot?.connected}
              aria-label="Добавить артефакт"
              data-testid="toggle-create-artifact-button"
              title="Добавить артефакт"
            >
              <Plus size={14} />
            </button>
          </div>

          {creatingArtifact && (
            <form
              className="artifact-form"
              onSubmit={(e) => void handleCreateArtifact(e)}
              data-testid="create-artifact-form"
            >
              <input
                type="text"
                placeholder="Имя файла, например notes.md"
                aria-label="Имя артефакта"
                value={artName}
                onChange={(e) => setArtName(e.target.value)}
                required
              />

              <div className="mime-picker" ref={mimeAnchorRef}>
                <button
                  type="button"
                  className="mime-picker__trigger"
                  aria-label="Тип содержимого артефакта"
                  aria-expanded={mimeOpen}
                  aria-haspopup="menu"
                  data-testid="artifact-mime-trigger"
                  onClick={() => setMimeOpen((prev) => !prev)}
                >
                  <span>{currentMimeLabel}</span>
                  <CaretDown size={12} aria-hidden="true" />
                </button>
                {mimeOpen && (
                  <div
                    className="floating-popover mime-picker__popover"
                    role="menu"
                    aria-label="MIME-тип артефакта"
                  >
                    {MIME_OPTIONS.map((opt) => (
                      <button
                        key={opt.value}
                        type="button"
                        role="menuitemradio"
                        aria-checked={opt.value === artMime}
                        className={opt.value === artMime ? "is-selected" : ""}
                        onClick={() => {
                          setArtMime(opt.value);
                          setMimeOpen(false);
                        }}
                      >
                        <span>{opt.label}</span>
                        {opt.value === artMime && <Check size={13} aria-hidden="true" />}
                      </button>
                    ))}
                  </div>
                )}
              </div>

              <textarea
                rows={4}
                placeholder="Содержимое артефакта…"
                aria-label="Содержимое артефакта"
                value={artContent}
                onChange={(e) => setArtContent(e.target.value)}
                required
              />
              <div className="artifact-form__actions">
                <button type="submit" className="popover-primary-btn">
                  Сохранить
                </button>
                <button
                  type="button"
                  className="popover-secondary-btn"
                  onClick={() => setCreatingArtifact(false)}
                >
                  Отмена
                </button>
              </div>
            </form>
          )}

          {artifacts.length === 0 ? (
            <p className="resource-empty">Артефактов в этой сессии пока нет.</p>
          ) : (
            <div className="artifact-list" data-testid="artifact-list">
              {artifacts.map((art) => {
                const isSelected = meta.selectedArtifact?.artifact_id === art.artifact_id;
                return (
                  <button
                    key={art.artifact_id}
                    type="button"
                    className={`resource-row ${isSelected ? "is-selected" : ""}`}
                    onClick={() => void actions.selectArtifact(art.artifact_id)}
                    data-testid={`artifact-item-${art.artifact_id}`}
                  >
                    <FileText size={16} aria-hidden="true" />
                    <span>
                      <strong>{art.name}</strong>
                      <small>
                        {art.mime_type} · rev {art.revision}
                      </small>
                    </span>
                  </button>
                );
              })}
            </div>
          )}

          {meta.selectedArtifact && (
            <div className="artifact-preview" data-testid="artifact-preview">
              <div className="artifact-preview__meta">
                <strong>{meta.selectedArtifact.name}</strong>
                <span>rev {meta.selectedArtifact.revision}</span>
              </div>
              {/* Strictly plain text child — zero innerHTML/XSS */}
              <pre className="artifact-preview__body">
                {meta.selectedArtifact.content ??
                  (meta.selectedArtifact.relative_path
                    ? `Файл рабочей области: ${meta.selectedArtifact.relative_path}`
                    : "(пустой артефакт)")}
              </pre>
            </div>
          )}
        </section>
      </div>
    </aside>
  );
}
