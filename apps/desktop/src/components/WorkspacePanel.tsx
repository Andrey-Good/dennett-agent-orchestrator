import { useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import {
  Archive,
  ArrowCounterClockwise,
  ArrowsClockwise,
  CaretDown,
  CaretRight,
  ChatsCircle,
  Check,
  CheckCircle,
  Copy,
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

import type { ArtifactRecord, WorkspaceFileEntry } from "../api/contracts";
import { useWorkspace } from "../state/WorkspaceProvider";
import { PortalPopover } from "./PortalPopover";

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

function parentDirOf(relativePath: string): string {
  const normalized = relativePath.replace(/\\/g, "/");
  const idx = normalized.lastIndexOf("/");
  return idx <= 0 ? "" : normalized.slice(0, idx);
}

function compareEntries(a: WorkspaceFileEntry, b: WorkspaceFileEntry): number {
  if (a.kind !== b.kind) {
    return a.kind === "directory" ? -1 : 1;
  }
  return a.name.localeCompare(b.name, "ru");
}

interface ArtifactGroup {
  name: string;
  revisions: ArtifactRecord[];
  latest: ArtifactRecord;
}

export function WorkspacePanel() {
  const { state, actions, meta } = useWorkspace();
  const [filesExpanded, setFilesExpanded] = useState(true);
  const [artifactsExpanded, setArtifactsExpanded] = useState(true);
  const [archivedArtifactsOpen, setArchivedArtifactsOpen] = useState(false);
  const [deletedArtifactsOpen, setDeletedArtifactsOpen] = useState(false);

  const [expandedDirs, setExpandedDirs] = useState<Record<string, boolean>>({});
  const [loadedSubdirs, setLoadedSubdirs] = useState<Record<string, WorkspaceFileEntry[]>>({});
  const [loadingSubdirs, setLoadingSubdirs] = useState<Record<string, boolean>>({});

  const [creatingArtifact, setCreatingArtifact] = useState(false);
  const [artName, setArtName] = useState("");
  const [artMime, setArtMime] = useState("text/markdown");
  const [artContent, setArtContent] = useState("");
  const [mimeOpen, setMimeOpen] = useState(false);
  const [artifactMenuOpen, setArtifactMenuOpen] = useState(false);
  const [confirmDeleteArtifactId, setConfirmDeleteArtifactId] = useState<string | null>(null);
  const [markdownViewMode, setMarkdownViewMode] = useState<"rendered" | "source">("rendered");
  const [copiedField, setCopiedField] = useState<"content" | "sha256" | null>(null);

  const mimeTriggerRef = useRef<HTMLButtonElement | null>(null);
  const artifactMenuTriggerRef = useRef<HTMLButtonElement | null>(null);
  const resourceScrollRef = useRef<HTMLDivElement | null>(null);

  const activeProject = meta.activeProject;
  const filesData = state.projectFiles;
  const selectedArt = meta.selectedArtifact;

  // Reset per-project folder tree cache when project or root files reload.
  useEffect(() => {
    setExpandedDirs({});
    setLoadedSubdirs({});
    setLoadingSubdirs({});
  }, [activeProject?.project_id, filesData]);

  // Reset inline delete confirmation and copy badge when switching artifacts.
  useEffect(() => {
    setConfirmDeleteArtifactId(null);
    setArtifactMenuOpen(false);
    setCopiedField(null);
  }, [selectedArt?.artifact_id]);

  // Build visible flat tree from root filesData + lazily loaded subdirectories.
  const visibleTreeRows = useMemo(() => {
    const entriesMap = new Map<string, WorkspaceFileEntry>();
    for (const entry of filesData?.entries ?? []) {
      entriesMap.set(entry.relative_path, entry);
    }
    for (const subEntries of Object.values(loadedSubdirs)) {
      for (const entry of subEntries) {
        if (!entriesMap.has(entry.relative_path)) {
          entriesMap.set(entry.relative_path, entry);
        }
      }
    }

    const allEntries = Array.from(entriesMap.values());
    const knownDirPaths = new Set(
      allEntries.filter((e) => e.kind === "directory").map((e) => e.relative_path),
    );
    const childrenByParent = new Map<string, WorkspaceFileEntry[]>();

    for (const entry of allEntries) {
      const parent = parentDirOf(entry.relative_path);
      const bucketKey = parent && knownDirPaths.has(parent) ? parent : "";
      const list = childrenByParent.get(bucketKey) ?? [];
      list.push(entry);
      childrenByParent.set(bucketKey, list);
    }

    for (const list of childrenByParent.values()) {
      list.sort(compareEntries);
    }

    const rows: Array<
      | { kind: "entry"; entry: WorkspaceFileEntry; depth: number }
      | { kind: "loading"; parentPath: string; depth: number }
      | { kind: "empty"; parentPath: string; depth: number }
    > = [];

    const walk = (parentPath: string, depth: number) => {
      const children = childrenByParent.get(parentPath) ?? [];
      for (const entry of children) {
        rows.push({ kind: "entry", entry, depth });
        if (entry.kind === "directory" && expandedDirs[entry.relative_path]) {
          const nested = childrenByParent.get(entry.relative_path);
          if (loadingSubdirs[entry.relative_path]) {
            rows.push({ kind: "loading", parentPath: entry.relative_path, depth: depth + 1 });
          } else if (nested && nested.length > 0) {
            walk(entry.relative_path, depth + 1);
          } else if (entry.relative_path in loadedSubdirs) {
            rows.push({ kind: "empty", parentPath: entry.relative_path, depth: depth + 1 });
          }
        }
      }
    };

    walk("", 1);
    return { rows, childrenByParent };
  }, [expandedDirs, filesData?.entries, loadedSubdirs, loadingSubdirs]);

  const rawArtifacts = state.activeDetail?.artifacts ?? [];

  // Group non-removed artifacts by logical name, partitioned by lifecycle state.
  const { activeArtifacts, archivedArtifacts, deletedArtifacts } = useMemo(() => {
    const visible = rawArtifacts.filter((a) => (a.lifecycle_state ?? "active") !== "removed");
    const byName = new Map<string, ArtifactRecord[]>();
    for (const art of visible) {
      const list = byName.get(art.name) ?? [];
      list.push(art);
      byName.set(art.name, list);
    }
    const active: ArtifactGroup[] = [];
    const archived: ArtifactGroup[] = [];
    const deleted: ArtifactGroup[] = [];

    for (const [name, revs] of byName.entries()) {
      const sorted = [...revs].sort((a, b) => a.revision - b.revision);
      const latest = sorted[sorted.length - 1]!;
      const group: ArtifactGroup = { name, revisions: sorted, latest };
      const lc = latest.lifecycle_state ?? "active";
      if (lc === "archived") {
        archived.push(group);
      } else if (lc === "deleted") {
        deleted.push(group);
      } else {
        active.push(group);
      }
    }
    return { activeArtifacts: active, archivedArtifacts: archived, deletedArtifacts: deleted };
  }, [rawArtifacts]);

  const totalVisibleArtifacts =
    activeArtifacts.length + archivedArtifacts.length + deletedArtifacts.length;

  const selectedRevisions = useMemo(() => {
    const sel = meta.selectedArtifact;
    if (!sel || (sel.lifecycle_state ?? "active") === "removed") {
      return [];
    }
    if (
      state.artifactRevisions.length > 0 &&
      state.artifactRevisions[0]?.name === sel.name &&
      state.artifactRevisions[0]?.session_id === sel.session_id
    ) {
      return state.artifactRevisions.filter((a) => (a.lifecycle_state ?? "active") !== "removed");
    }
    return rawArtifacts
      .filter((a) => a.name === sel.name && (a.lifecycle_state ?? "active") !== "removed")
      .sort((a, b) => a.revision - b.revision);
  }, [meta.selectedArtifact, rawArtifacts, state.artifactRevisions]);

  if (!state.isRightPanelOpen) {
    return null;
  }

  const isBound = Boolean(activeProject?.workspace_path);

  const handleToggleDirectory = async (dirPath: string) => {
    const nextOpen = !expandedDirs[dirPath];
    setExpandedDirs((prev) => ({ ...prev, [dirPath]: nextOpen }));
    if (!nextOpen || !activeProject) {
      return;
    }
    const alreadyHasChildren = (visibleTreeRows.childrenByParent.get(dirPath)?.length ?? 0) > 0;
    if (alreadyHasChildren || dirPath in loadedSubdirs || loadingSubdirs[dirPath]) {
      return;
    }
    setLoadingSubdirs((prev) => ({ ...prev, [dirPath]: true }));
    try {
      const resp = await meta.bridge.listProjectFiles(activeProject.project_id, dirPath);
      setLoadedSubdirs((prev) => ({ ...prev, [dirPath]: resp.entries }));
    } catch {
      setLoadedSubdirs((prev) => ({ ...prev, [dirPath]: [] }));
    } finally {
      setLoadingSubdirs((prev) => ({ ...prev, [dirPath]: false }));
    }
  };

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

  const copyText = (field: "content" | "sha256", value: string) => {
    if (navigator.clipboard?.writeText) {
      void navigator.clipboard.writeText(value).catch(() => {});
    }
    setCopiedField(field);
    window.setTimeout(() => {
      setCopiedField((curr) => (curr === field ? null : curr));
    }, 1500);
  };

  const currentMimeLabel =
    MIME_OPTIONS.find((item) => item.value === artMime)?.label ?? artMime;

  const selectedCompleteness = selectedArt?.completeness_status ?? "final";
  const selectedLifecycle = selectedArt?.lifecycle_state ?? "active";
  const canFinalize = selectedCompleteness === "partial" && selectedLifecycle !== "deleted";
  const canUnarchive = selectedLifecycle === "archived";
  const canArchive = selectedLifecycle !== "archived" && selectedLifecycle !== "deleted";
  const canRemove = selectedLifecycle !== "deleted";
  const canDelete = selectedLifecycle !== "deleted";
  const hasLifecycleActions =
    Boolean(selectedArt) &&
    (canFinalize || canUnarchive || canArchive || canRemove || canDelete);

  const isMarkdownPreviewable = Boolean(
    selectedArt &&
      selectedLifecycle !== "deleted" &&
      selectedArt.content &&
      (selectedArt.mime_type.toLowerCase().includes("markdown") ||
        (/\.(md|markdown)$/i.test(selectedArt.name) &&
          !selectedArt.mime_type.toLowerCase().includes("json"))),
  );

  const renderArtifactRow = ({ name, revisions, latest }: ArtifactGroup) => {
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
  };

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
        {meta.isStandaloneActiveSession ? (
          <div className="workspace-standalone-note" data-testid="workspace-standalone-note">
            <ChatsCircle size={15} aria-hidden="true" />
            <span>Одиночный чат без привязки к папке</span>
          </div>
        ) : (
        <section className="resource-section" data-testid="workspace-files-section">
          <div className="resource-section__head">
            <button
              type="button"
              className="resource-section__toggle"
              aria-expanded={filesExpanded}
              data-testid="workspace-files-toggle"
              onClick={() => setFilesExpanded((prev) => !prev)}
            >
              {filesExpanded ? (
                <CaretDown size={12} aria-hidden="true" />
              ) : (
                <CaretRight size={12} aria-hidden="true" />
              )}
              <h3>
                Файлы проекта{" "}
                {filesData && filesData.workspace_bound && <span>{filesData.entries.length}</span>}
              </h3>
            </button>
          </div>

          {filesExpanded && (
            <>
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
                      {visibleTreeRows.rows.map((row) => {
                        if (row.kind === "loading") {
                          return (
                            <div
                              key={`loading:${row.parentPath}`}
                              className="workspace-tree-hint"
                              style={{ paddingLeft: `${8 + (row.depth - 1) * 14}px` }}
                            >
                              Загрузка…
                            </div>
                          );
                        }
                        if (row.kind === "empty") {
                          return (
                            <div
                              key={`empty:${row.parentPath}`}
                              className="workspace-tree-hint"
                              style={{ paddingLeft: `${8 + (row.depth - 1) * 14}px` }}
                            >
                              (пусто)
                            </div>
                          );
                        }
                        const { entry, depth } = row;
                        if (entry.kind === "directory") {
                          const isOpen = Boolean(expandedDirs[entry.relative_path]);
                          return (
                            <button
                              key={entry.relative_path}
                              type="button"
                              className="resource-row workspace-file-row workspace-tree-dir-btn"
                              style={{ paddingLeft: `${8 + (depth - 1) * 14}px` }}
                              title={entry.relative_path}
                              aria-expanded={isOpen}
                              data-testid={`workspace-dir-${entry.relative_path}`}
                              onClick={() => void handleToggleDirectory(entry.relative_path)}
                            >
                              {isOpen ? (
                                <CaretDown size={11} aria-hidden="true" />
                              ) : (
                                <CaretRight size={11} aria-hidden="true" />
                              )}
                              <FolderSimple size={15} aria-hidden="true" />
                              <span>
                                <strong>{entry.name}</strong>
                              </span>
                            </button>
                          );
                        }
                        return (
                          <div
                            key={entry.relative_path}
                            className="resource-row is-static workspace-file-row"
                            style={{ paddingLeft: `${22 + (depth - 1) * 14}px` }}
                            title={entry.relative_path}
                          >
                            {isCodeFile(entry.name) ? (
                              <FileCode size={15} aria-hidden="true" />
                            ) : (
                              <FileText size={15} aria-hidden="true" />
                            )}
                            <span>
                              <strong>{entry.name}</strong>
                            </span>
                            {entry.size_bytes !== null && entry.size_bytes !== undefined && (
                              <em>{formatBytes(entry.size_bytes)}</em>
                            )}
                          </div>
                        );
                      })}
                      {filesData.truncated && (
                        <p className="resource-truncated">
                          Показаны первые {filesData.entries.length} элементов (лимит безопасности).
                        </p>
                      )}
                    </div>
                  )}
                </>
              )}
            </>
          )}
        </section>
        )}

        <section className="resource-section" data-testid="workspace-artifacts-section">
          <div className="resource-section__head">
            <button
              type="button"
              className="resource-section__toggle"
              aria-expanded={artifactsExpanded}
              data-testid="workspace-artifacts-toggle"
              onClick={() => setArtifactsExpanded((prev) => !prev)}
            >
              {artifactsExpanded ? (
                <CaretDown size={12} aria-hidden="true" />
              ) : (
                <CaretRight size={12} aria-hidden="true" />
              )}
              <h3>
                Артефакты сессии <span>{activeArtifacts.length}</span>
              </h3>
            </button>
            <button
              type="button"
              className="icon-button icon-button--sm"
              onClick={() => {
                setArtifactsExpanded(true);
                setCreatingArtifact((prev) => !prev);
              }}
              disabled={
                !state.selectedSessionId ||
                !state.healthSnapshot?.connected ||
                meta.isActiveProjectArchived
              }
              aria-label="Добавить артефакт"
              data-testid="toggle-create-artifact-button"
              title="Добавить артефакт"
            >
              <Plus size={14} />
            </button>
          </div>

          {artifactsExpanded && (
            <>
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

                  <div className="mime-picker">
                    <button
                      ref={mimeTriggerRef}
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
                    <PortalPopover
                      isOpen={mimeOpen}
                      anchorEl={mimeTriggerRef.current}
                      onClose={() => setMimeOpen(false)}
                      placement="bottom-start"
                      className="mime-picker__popover"
                      role="menu"
                      ariaLabel="MIME-тип артефакта"
                      matchAnchorWidth
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
                    </PortalPopover>
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

              {totalVisibleArtifacts === 0 ? (
                <p className="resource-empty">Артефактов в этой сессии пока нет.</p>
              ) : (
                <>
                  {activeArtifacts.length === 0 ? (
                    <p className="resource-empty">Активных артефактов нет.</p>
                  ) : (
                    <div className="artifact-list" data-testid="artifact-list">
                      {activeArtifacts.map(renderArtifactRow)}
                    </div>
                  )}

                  {archivedArtifacts.length > 0 && (
                    <div className="workspace-archive-block" data-testid="workspace-archived-artifacts">
                      <button
                        type="button"
                        className="workspace-archive-toggle"
                        aria-expanded={archivedArtifactsOpen}
                        data-testid="workspace-archived-artifacts-toggle"
                        onClick={() => setArchivedArtifactsOpen((prev) => !prev)}
                      >
                        {archivedArtifactsOpen ? (
                          <CaretDown size={11} aria-hidden="true" />
                        ) : (
                          <CaretRight size={11} aria-hidden="true" />
                        )}
                        <Archive size={12} aria-hidden="true" />
                        <span>Архив ({archivedArtifacts.length})</span>
                      </button>
                      {archivedArtifactsOpen && (
                        <div className="artifact-list artifact-list--archived">
                          {archivedArtifacts.map(renderArtifactRow)}
                        </div>
                      )}
                    </div>
                  )}

                  {deletedArtifacts.length > 0 && (
                    <div className="workspace-archive-block" data-testid="workspace-deleted-artifacts">
                      <button
                        type="button"
                        className="workspace-archive-toggle"
                        aria-expanded={deletedArtifactsOpen}
                        data-testid="workspace-deleted-artifacts-toggle"
                        onClick={() => setDeletedArtifactsOpen((prev) => !prev)}
                      >
                        {deletedArtifactsOpen ? (
                          <CaretDown size={11} aria-hidden="true" />
                        ) : (
                          <CaretRight size={11} aria-hidden="true" />
                        )}
                        <Trash size={12} aria-hidden="true" />
                        <span>Удалённые ({deletedArtifacts.length})</span>
                      </button>
                      {deletedArtifactsOpen && (
                        <div className="artifact-list artifact-list--archived">
                          {deletedArtifacts.map(renderArtifactRow)}
                        </div>
                      )}
                    </div>
                  )}
                </>
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

                    <div className="artifact-preview__controls">
                      <span className="artifact-preview__rev">rev {selectedArt.revision}</span>
                      {selectedLifecycle !== "deleted" && selectedArt.content && (
                        <button
                          type="button"
                          className="icon-button icon-button--xs"
                          aria-label="Скопировать содержимое артефакта"
                          title={copiedField === "content" ? "Скопировано" : "Копировать содержимое"}
                          data-testid="artifact-copy-content"
                          onClick={() => copyText("content", selectedArt.content ?? "")}
                        >
                          {copiedField === "content" ? <Check size={13} /> : <Copy size={13} />}
                        </button>
                      )}

                      {hasLifecycleActions && (
                        <>
                          <button
                            ref={artifactMenuTriggerRef}
                            type="button"
                            className="icon-button icon-button--xs"
                            aria-label={`Действия с артефактом ${selectedArt.name}`}
                            aria-expanded={artifactMenuOpen}
                            aria-haspopup="menu"
                            data-testid="artifact-lifecycle-trigger"
                            onClick={() => setArtifactMenuOpen((prev) => !prev)}
                          >
                            <DotsThree size={15} weight="bold" />
                          </button>

                          <PortalPopover
                            isOpen={artifactMenuOpen}
                            anchorEl={artifactMenuTriggerRef.current}
                            onClose={() => setArtifactMenuOpen(false)}
                            placement="bottom-end"
                            className="artifact-lifecycle-menu"
                            role="menu"
                            ariaLabel="Управление артефактом"
                            testId="artifact-lifecycle-menu"
                          >
                            {canFinalize && (
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

                            {canUnarchive && (
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
                            )}

                            {canArchive && (
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
                            )}

                            {canRemove && (
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
                            )}

                            {canDelete && (
                              <button
                                type="button"
                                role="menuitem"
                                className="menu-item-danger"
                                data-testid="artifact-action-delete"
                                onClick={() => {
                                  setArtifactMenuOpen(false);
                                  setConfirmDeleteArtifactId(selectedArt.artifact_id);
                                }}
                              >
                                <Trash size={14} aria-hidden="true" />
                                <span>Удалить из Dennett</span>
                              </button>
                            )}
                          </PortalPopover>
                        </>
                      )}
                    </div>
                  </div>

                  {confirmDeleteArtifactId === selectedArt.artifact_id && (
                    <div
                      className="artifact-confirm-bar"
                      role="alertdialog"
                      aria-label="Подтверждение удаления артефакта"
                      data-testid="artifact-confirm-bar"
                    >
                      <p>
                        Удалить содержимое «{selectedArt.name}» из базы Dennett? Файлы в рабочей
                        папке на диске не будут затронуты.
                      </p>
                      <div className="artifact-confirm-bar__actions">
                        <button
                          type="button"
                          className="popover-secondary-btn"
                          onClick={() => setConfirmDeleteArtifactId(null)}
                        >
                          Отмена
                        </button>
                        <button
                          type="button"
                          className="d2-danger-btn d2-danger-btn--sm"
                          data-testid="artifact-confirm-yes"
                          onClick={() => {
                            setConfirmDeleteArtifactId(null);
                            void actions.applyArtifactLifecycle(
                              selectedArt.session_id,
                              selectedArt.artifact_id,
                              "delete",
                            );
                          }}
                        >
                          Удалить из Dennett
                        </button>
                      </div>
                    </div>
                  )}

                  {selectedArt.sha256 && (
                    <div className="artifact-sha-bar">
                      <code
                        className="artifact-sha-bar__hash"
                        title={selectedArt.sha256}
                        data-testid="artifact-sha256"
                      >
                        SHA-256: {selectedArt.sha256.slice(0, 12)}…
                      </code>
                      <button
                        type="button"
                        className="icon-button icon-button--xs"
                        aria-label="Скопировать SHA-256"
                        title={copiedField === "sha256" ? "Хеш скопирован" : "Скопировать SHA-256"}
                        data-testid="artifact-copy-sha256"
                        onClick={() => copyText("sha256", selectedArt.sha256 ?? "")}
                      >
                        {copiedField === "sha256" ? <Check size={12} /> : <Copy size={12} />}
                      </button>
                    </div>
                  )}

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

                  {isMarkdownPreviewable && (
                    <div
                      className="artifact-mode-tabs"
                      role="tablist"
                      aria-label="Режим просмотра Markdown"
                    >
                      <button
                        type="button"
                        role="tab"
                        aria-selected={markdownViewMode === "rendered"}
                        className={markdownViewMode === "rendered" ? "is-active" : ""}
                        data-testid="artifact-mode-rendered"
                        onClick={() => setMarkdownViewMode("rendered")}
                      >
                        Просмотр
                      </button>
                      <button
                        type="button"
                        role="tab"
                        aria-selected={markdownViewMode === "source"}
                        className={markdownViewMode === "source" ? "is-active" : ""}
                        data-testid="artifact-mode-source"
                        onClick={() => setMarkdownViewMode("source")}
                      >
                        Исходник
                      </button>
                    </div>
                  )}

                  {isMarkdownPreviewable && markdownViewMode === "rendered" ? (
                    <div
                      className="artifact-preview__markdown message-markdown"
                      data-testid="artifact-preview-markdown"
                    >
                      <ReactMarkdown remarkPlugins={[remarkGfm]} skipHtml>
                        {selectedArt.content ?? ""}
                      </ReactMarkdown>
                    </div>
                  ) : (
                    /* Strictly plain text child — zero innerHTML/XSS */
                    <pre className="artifact-preview__body" data-testid="artifact-preview-source">
                      {selectedLifecycle === "deleted"
                        ? "(Содержимое артефакта удалено в Dennett; файлы в рабочей папке не затронуты)"
                        : (selectedArt.content ??
                          (selectedArt.relative_path
                            ? `Файл рабочей области: ${selectedArt.relative_path}`
                            : "(пустой артефакт)"))}
                    </pre>
                  )}
                </div>
              )}
            </>
          )}
        </section>
      </div>
    </aside>
  );
}
