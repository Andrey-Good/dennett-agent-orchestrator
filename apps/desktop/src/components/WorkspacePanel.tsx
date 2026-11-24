import { useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import {
  Archive,
  ArrowCounterClockwise,
  ArrowsClockwise,
  CaretDown,
  Check,
  CheckCircle,
  DotsThree,
  EyeSlash,
  FileCode,
  FileText,
  FolderOpen,
  FolderSimple,
  Plus,
  Sidebar,
  Trash,
} from "@phosphor-icons/react";

import type { ArtifactRecord } from "../api/contracts";
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
  const [artifactMenuOpen, setArtifactMenuOpen] = useState(false);
  const [artifactMenuFlipUp, setArtifactMenuFlipUp] = useState(false);
  const mimeAnchorRef = useRef<HTMLDivElement | null>(null);
  const artifactMenuRef = useRef<HTMLDivElement | null>(null);
  const resourceScrollRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!mimeOpen) {
      return;
    }
    const handlePointerDown = (event: MouseEvent) => {
      if (mimeAnchorRef.current && !mimeAnchorRef.current.contains(event.target as Node)) {
        setMimeOpen(false);
      }
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        setMimeOpen(false);
      }
    };
    document.addEventListener("mousedown", handlePointerDown);
    window.addEventListener("keydown", handleKeyDown, { capture: true });
    return () => {
      document.removeEventListener("mousedown", handlePointerDown);
      window.removeEventListener("keydown", handleKeyDown, { capture: true });
    };
  }, [mimeOpen]);

  useEffect(() => {
    if (!artifactMenuOpen) {
      return;
    }
    const handlePointerDown = (event: MouseEvent) => {
      if (artifactMenuRef.current && !artifactMenuRef.current.contains(event.target as Node)) {
        setArtifactMenuOpen(false);
      }
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        setArtifactMenuOpen(false);
      }
    };
    document.addEventListener("mousedown", handlePointerDown);
    window.addEventListener("keydown", handleKeyDown, { capture: true });
    return () => {
      document.removeEventListener("mousedown", handlePointerDown);
      window.removeEventListener("keydown", handleKeyDown, { capture: true });
    };
  }, [artifactMenuOpen]);

  const rawArtifacts = state.activeDetail?.artifacts ?? [];

  // Group artifacts by logical name (excluding removed entries) so v1/v2 collapse neatly into one row + revision pills.
  const groupedArtifacts = useMemo(() => {
    const visible = rawArtifacts.filter((a) => (a.lifecycle_state ?? "active") !== "removed");
    const byName = new Map<string, ArtifactRecord[]>();
    for (const art of visible) {
      const list = byName.get(art.name) ?? [];
      list.push(art);
      byName.set(art.name, list);
    }
    return Array.from(byName.entries()).map(([name, revs]) => {
      const sorted = [...revs].sort((a, b) => a.revision - b.revision);
      const latest = sorted[sorted.length - 1]!;
      return { name, revisions: sorted, latest };
    });
  }, [rawArtifacts]);

  const selectedRevisions = useMemo(() => {
    const sel = meta.selectedArtifact;
    if (!sel) {
      return [];
    }
    if (
      state.artifactRevisions.length > 0 &&
      state.artifactRevisions[0]?.name === sel.name &&
      state.artifactRevisions[0]?.session_id === sel.session_id
    ) {
      return state.artifactRevisions;
    }
    return rawArtifacts
      .filter((a) => a.name === sel.name)
      .sort((a, b) => a.revision - b.revision);
  }, [meta.selectedArtifact, rawArtifacts, state.artifactRevisions]);

  if (!state.isRightPanelOpen) {
    return null;
  }

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

  const selectedArt = meta.selectedArtifact;
  const selectedCompleteness = selectedArt?.completeness_status ?? "final";
  const selectedLifecycle = selectedArt?.lifecycle_state ?? "active";

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

      <div className="resource-scroll" ref={resourceScrollRef}>
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
              Артефакты сессии <span>{groupedArtifacts.length}</span>
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

          {groupedArtifacts.length === 0 ? (
            <p className="resource-empty">Артефактов в этой сессии пока нет.</p>
          ) : (
            <div className="artifact-list" data-testid="artifact-list">
              {groupedArtifacts.map(({ name, revisions, latest }) => {
                const isSelected = selectedArt?.name === name;
                const completeness = latest.completeness_status ?? "final";
                const lifecycle = latest.lifecycle_state ?? "active";
                return (
                  <button
                    key={latest.artifact_id}
                    type="button"
                    className={`resource-row ${isSelected ? "is-selected" : ""}`}
                    onClick={() => void actions.selectArtifact(latest.artifact_id)}
                    data-testid={`artifact-item-${latest.artifact_id}`}
                  >
                    <FileText size={16} aria-hidden="true" />
                    <span>
                      <strong>{name}</strong>
                      <small>
                        {latest.mime_type} · v{latest.revision}
                        {revisions.length > 1 ? ` (${revisions.length} верс.)` : ""}
                        {lifecycle === "archived" ? " · архив" : ""}
                        {lifecycle === "deleted" ? " · удалено" : ""}
                      </small>
                    </span>
                    <span
                      className={`artifact-badge artifact-badge--${completeness}`}
                      data-testid={`artifact-completeness-${latest.artifact_id}`}
                    >
                      {completeness === "partial" ? "Частичный" : "Финальный"}
                    </span>
                  </button>
                );
              })}
            </div>
          )}

          {selectedArt && (
            <div className="artifact-preview" data-testid="artifact-preview">
              <div className="artifact-preview__meta">
                <div className="artifact-preview__title-group">
                  <strong>{selectedArt.name}</strong>
                  <span
                    className={`artifact-badge artifact-badge--${selectedCompleteness}`}
                    data-testid="artifact-preview-completeness"
                  >
                    {selectedCompleteness === "partial" ? "Частичный" : "Финальный"}
                  </span>
                </div>

                <div className="artifact-preview__controls" ref={artifactMenuRef}>
                  <span className="artifact-preview__rev">rev {selectedArt.revision}</span>
                  <button
                    type="button"
                    className="icon-button icon-button--xs"
                    aria-label={`Действия с артефактом ${selectedArt.name}`}
                    aria-expanded={artifactMenuOpen}
                    aria-haspopup="menu"
                    data-testid="artifact-lifecycle-trigger"
                    onClick={(e) => {
                      if (artifactMenuOpen) {
                        setArtifactMenuOpen(false);
                        return;
                      }
                      const scrollEl = resourceScrollRef.current;
                      if (scrollEl) {
                        const scrollRect = scrollEl.getBoundingClientRect();
                        const triggerRect = e.currentTarget.getBoundingClientRect();
                        if (scrollRect.height > 0) {
                          const spaceBelow = scrollRect.bottom - triggerRect.bottom;
                          const spaceAbove = triggerRect.top - scrollRect.top;
                          setArtifactMenuFlipUp(spaceBelow < 190 && spaceAbove > spaceBelow);
                        } else {
                          setArtifactMenuFlipUp(false);
                        }
                      } else {
                        setArtifactMenuFlipUp(false);
                      }
                      setArtifactMenuOpen(true);
                    }}
                  >
                    <DotsThree size={15} weight="bold" />
                  </button>

                  {artifactMenuOpen && (
                    <div
                      className={`floating-popover artifact-lifecycle-menu ${
                        artifactMenuFlipUp ? "artifact-lifecycle-menu--up" : ""
                      }`}
                      role="menu"
                      aria-label="Управление артефактом"
                      data-testid="artifact-lifecycle-menu"
                    >
                      {selectedCompleteness === "partial" && selectedLifecycle !== "deleted" && (
                        <button
                          type="button"
                          role="menuitem"
                          data-testid="artifact-action-finalize"
                          onClick={() => {
                            setArtifactMenuOpen(false);
                            void actions.applyArtifactLifecycle(
                              selectedArt.session_id,
                              selectedArt.artifact_id,
                              "finalize",
                            );
                          }}
                        >
                          <CheckCircle size={14} aria-hidden="true" />
                          <span>Отметить финальным</span>
                        </button>
                      )}

                      {selectedLifecycle === "archived" ? (
                        <button
                          type="button"
                          role="menuitem"
                          data-testid="artifact-action-unarchive"
                          onClick={() => {
                            setArtifactMenuOpen(false);
                            void actions.applyArtifactLifecycle(
                              selectedArt.session_id,
                              selectedArt.artifact_id,
                              "unarchive",
                            );
                          }}
                        >
                          <ArrowCounterClockwise size={14} aria-hidden="true" />
                          <span>Вернуть из архива</span>
                        </button>
                      ) : selectedLifecycle !== "deleted" ? (
                        <button
                          type="button"
                          role="menuitem"
                          data-testid="artifact-action-archive"
                          onClick={() => {
                            setArtifactMenuOpen(false);
                            void actions.applyArtifactLifecycle(
                              selectedArt.session_id,
                              selectedArt.artifact_id,
                              "archive",
                            );
                          }}
                        >
                          <Archive size={14} aria-hidden="true" />
                          <span>В архив</span>
                        </button>
                      ) : null}

                      {selectedLifecycle !== "deleted" && (
                        <>
                          <button
                            type="button"
                            role="menuitem"
                            data-testid="artifact-action-remove"
                            onClick={() => {
                              setArtifactMenuOpen(false);
                              void actions.applyArtifactLifecycle(
                                selectedArt.session_id,
                                selectedArt.artifact_id,
                                "remove",
                              );
                            }}
                          >
                            <EyeSlash size={14} aria-hidden="true" />
                            <span>Убрать из списка</span>
                          </button>

                          <button
                            type="button"
                            role="menuitem"
                            className="menu-item-danger"
                            data-testid="artifact-action-delete"
                            onClick={() => {
                              setArtifactMenuOpen(false);
                              void actions.applyArtifactLifecycle(
                                selectedArt.session_id,
                                selectedArt.artifact_id,
                                "delete",
                              );
                            }}
                          >
                            <Trash size={14} aria-hidden="true" />
                            <span>Удалить из Dennett</span>
                          </button>
                        </>
                      )}
                    </div>
                  )}
                </div>
              </div>

              {selectedRevisions.length > 1 && (
                <div
                  className="artifact-revisions-bar"
                  role="tablist"
                  aria-label="Версии артефакта"
                  data-testid="artifact-revisions-bar"
                >
                  {selectedRevisions.map((rev) => {
                    const isCurrent = rev.artifact_id === selectedArt.artifact_id;
                    return (
                      <button
                        key={rev.artifact_id}
                        type="button"
                        role="tab"
                        aria-selected={isCurrent}
                        className={`artifact-rev-pill ${isCurrent ? "is-active" : ""}`}
                        data-testid={`artifact-revision-pill-${rev.revision}`}
                        onClick={() => void actions.selectArtifact(rev.artifact_id)}
                      >
                        v{rev.revision}
                      </button>
                    );
                  })}
                </div>
              )}

              {/* Strictly plain text child — zero innerHTML/XSS */}
              <pre className="artifact-preview__body">
                {selectedLifecycle === "deleted"
                  ? "(Содержимое артефакта удалено в Dennett; файлы в рабочей папке не затронуты)"
                  : (selectedArt.content ??
                    (selectedArt.relative_path
                      ? `Файл рабочей области: ${selectedArt.relative_path}`
                      : "(пустой артефакт)"))}
              </pre>
            </div>
          )}
        </section>
      </div>
    </aside>
  );
}
