import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
  type PointerEvent as ReactPointerEvent,
} from "react";
import {
  Archive,
  ArrowCounterClockwise,
  ArrowsClockwise,
  At,
  CaretDown,
  CaretRight,
  ChatsCircle,
  Check,
  CheckCircle,
  Copy,
  DotsThree,
  DownloadSimple,
  EyeSlash,
  FileCode,
  FileText,
  FolderOpen,
  FolderSimple,
  Plus,
  Sidebar,
  Trash,
  X,
} from "@phosphor-icons/react";

import {
  normalizeBridgeError,
  type ArtifactContentResponse,
  type ArtifactRecord,
  type ProjectFileContentResponse,
  type WorkspaceFileEntry,
} from "../api/contracts";
import { useWorkspace } from "../state/WorkspaceProvider";
import { MarkdownRenderer } from "./MarkdownRenderer";
import { PortalPopover } from "./PortalPopover";

const PANEL_WIDTH_STORAGE_KEY = "dennett.desktop.workspacePanelWidth.v1";
const MIN_PANEL_WIDTH = 260;
const MAX_PANEL_WIDTH = 560;
const DEFAULT_PANEL_WIDTH = 316;

function loadSavedPanelWidth(): number {
  if (typeof window === "undefined") {
    return DEFAULT_PANEL_WIDTH;
  }
  try {
    const raw = window.localStorage.getItem(PANEL_WIDTH_STORAGE_KEY);
    const parsed = raw ? Number.parseInt(raw, 10) : NaN;
    if (Number.isFinite(parsed)) {
      return Math.max(MIN_PANEL_WIDTH, Math.min(MAX_PANEL_WIDTH, parsed));
    }
  } catch {
    // Ignore storage access errors.
  }
  return DEFAULT_PANEL_WIDTH;
}

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

function base64ToUint8Array(base64: string): Uint8Array {
  const binary = window.atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

interface ArtifactGroup {
  name: string;
  revisions: ArtifactRecord[];
  latest: ArtifactRecord;
}

export function WorkspacePanel() {
  const { state, actions, meta } = useWorkspace();
  const [panelWidth, setPanelWidth] = useState<number>(loadSavedPanelWidth);
  const [filesExpanded, setFilesExpanded] = useState(true);
  const [artifactsExpanded, setArtifactsExpanded] = useState(true);
  const [archivedArtifactsOpen, setArchivedArtifactsOpen] = useState(false);
  const [deletedArtifactsOpen, setDeletedArtifactsOpen] = useState(false);

  const [expandedDirs, setExpandedDirs] = useState<Record<string, boolean>>({});
  const [loadedSubdirs, setLoadedSubdirs] = useState<Record<string, WorkspaceFileEntry[]>>({});
  const [loadingSubdirs, setLoadingSubdirs] = useState<Record<string, boolean>>({});

  // Clickable workspace file preview state
  const [selectedFilePath, setSelectedFilePath] = useState<string | null>(null);
  const [filePreview, setFilePreview] = useState<ProjectFileContentResponse | null>(null);
  const [isLoadingFilePreview, setIsLoadingFilePreview] = useState(false);
  const [filePreviewError, setFilePreviewError] = useState<string | null>(null);

  // Artifact immutable snapshot content preview (A-UI-P0-3)
  const [artifactContent, setArtifactContent] = useState<ArtifactContentResponse | null>(null);
  const [isLoadingArtifactContent, setIsLoadingArtifactContent] = useState(false);
  const [artifactContentError, setArtifactContentError] = useState<string | null>(null);

  const [creatingArtifact, setCreatingArtifact] = useState(false);
  const [artName, setArtName] = useState("");
  const [artMime, setArtMime] = useState("text/markdown");
  const [artContent, setArtContent] = useState("");
  const [mimeOpen, setMimeOpen] = useState(false);
  const [artifactMenuOpen, setArtifactMenuOpen] = useState(false);
  const [confirmDeleteArtifactId, setConfirmDeleteArtifactId] = useState<string | null>(null);
  const [markdownViewMode, setMarkdownViewMode] = useState<"rendered" | "source">("rendered");
  const [copiedField, setCopiedField] = useState<"content" | "sha256" | "file_path" | null>(null);

  const mimeTriggerRef = useRef<HTMLButtonElement | null>(null);
  const artifactMenuTriggerRef = useRef<HTMLButtonElement | null>(null);
  const resourceScrollRef = useRef<HTMLDivElement | null>(null);
  const resizeStateRef = useRef<{ startX: number; startWidth: number } | null>(null);

  const activeProject = meta.activeProject;
  const filesData = state.projectFiles;
  const selectedArt = meta.selectedArtifact;
  const selectedLifecycle = selectedArt?.lifecycle_state ?? "active";

  // Reset per-project folder tree cache & file preview when project or root files reload.
  useEffect(() => {
    setExpandedDirs({});
    setLoadedSubdirs({});
    setLoadingSubdirs({});
    setSelectedFilePath(null);
    setFilePreview(null);
    setFilePreviewError(null);
  }, [activeProject?.project_id, filesData]);

  // Reset inline delete confirmation and copy badge when switching artifacts.
  useEffect(() => {
    setConfirmDeleteArtifactId(null);
    setArtifactMenuOpen(false);
    setCopiedField(null);
  }, [selectedArt?.artifact_id]);

  // A-UI-P0-3: Fetch immutable snapshot content via /v1/sessions/{id}/artifacts/{aid}/content
  useEffect(() => {
    if (!selectedArt || selectedLifecycle === "deleted" || !state.healthSnapshot?.connected) {
      setArtifactContent(null);
      setArtifactContentError(null);
      setIsLoadingArtifactContent(false);
      return;
    }

    let cancelled = false;
    setIsLoadingArtifactContent(true);
    setArtifactContentError(null);

    void meta.bridge
      .getArtifactContent(selectedArt.session_id, selectedArt.artifact_id)
      .then((resp) => {
        if (!cancelled) {
          setArtifactContent(resp);
        }
      })
      .catch((err) => {
        if (!cancelled) {
          setArtifactContent(null);
          setArtifactContentError(normalizeBridgeError(err).message);
        }
      })
      .finally(() => {
        if (!cancelled) {
          setIsLoadingArtifactContent(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [
    meta.bridge,
    selectedArt?.artifact_id,
    selectedArt?.session_id,
    selectedLifecycle,
    state.healthSnapshot?.connected,
  ]);

  // Drag resize handlers
  const handleResizePointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    e.preventDefault();
    resizeStateRef.current = { startX: e.clientX, startWidth: panelWidth };

    const handleMove = (ev: PointerEvent) => {
      const origin = resizeStateRef.current;
      if (!origin) {
        return;
      }
      const delta = origin.startX - ev.clientX;
      const clamped = Math.max(
        MIN_PANEL_WIDTH,
        Math.min(MAX_PANEL_WIDTH, Math.round(origin.startWidth + delta)),
      );
      setPanelWidth(clamped);
    };

    const handleUp = () => {
      resizeStateRef.current = null;
      window.removeEventListener("pointermove", handleMove);
      window.removeEventListener("pointerup", handleUp);
      setPanelWidth((curr) => {
        try {
          window.localStorage.setItem(PANEL_WIDTH_STORAGE_KEY, String(curr));
        } catch {
          // Ignore storage errors.
        }
        return curr;
      });
    };

    window.addEventListener("pointermove", handleMove);
    window.addEventListener("pointerup", handleUp);
  };

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

  const handleSelectWorkspaceFile = async (relativePath: string) => {
    if (!activeProject) {
      return;
    }
    if (selectedFilePath === relativePath && filePreview) {
      setSelectedFilePath(null);
      setFilePreview(null);
      setFilePreviewError(null);
      return;
    }
    setSelectedFilePath(relativePath);
    setFilePreview(null);
    setFilePreviewError(null);
    setIsLoadingFilePreview(true);
    try {
      const resp = await meta.bridge.getProjectFileContent(activeProject.project_id, relativePath);
      setFilePreview(resp);
    } catch (err) {
      setFilePreviewError(normalizeBridgeError(err).message);
    } finally {
      setIsLoadingFilePreview(false);
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

  const copyText = (field: "content" | "sha256" | "file_path", value: string) => {
    if (navigator.clipboard?.writeText) {
      void navigator.clipboard.writeText(value).catch(() => {});
    }
    setCopiedField(field);
    window.setTimeout(() => {
      setCopiedField((curr) => (curr === field ? null : curr));
    }, 1500);
  };

  const handleDownloadArtifact = () => {
    if (!selectedArt || selectedLifecycle === "deleted") {
      return;
    }
    if (typeof URL === "undefined" || typeof URL.createObjectURL !== "function") {
      return;
    }
    let blob: Blob;
    if (artifactContent?.base64) {
      const bytes = base64ToUint8Array(artifactContent.base64);
      blob = new Blob([bytes.buffer as ArrayBuffer], {
        type: artifactContent.mime_type || selectedArt.mime_type || "application/octet-stream",
      });
    } else {
      const fallbackText = artifactContent?.text ?? selectedArt.content ?? "";
      blob = new Blob([fallbackText], {
        type: selectedArt.mime_type || "text/plain;charset=utf-8",
      });
    }
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = selectedArt.name;
    document.body.appendChild(anchor);
    anchor.click();
    anchor.remove();
    URL.revokeObjectURL(url);
  };

  const currentMimeLabel =
    MIME_OPTIONS.find((item) => item.value === artMime)?.label ?? artMime;

  const selectedCompleteness = selectedArt?.completeness_status ?? "final";
  const canFinalize = selectedCompleteness === "partial" && selectedLifecycle !== "deleted";
  const canUnarchive = selectedLifecycle === "archived";
  const canArchive = selectedLifecycle !== "archived" && selectedLifecycle !== "deleted";
  const canRemove = selectedLifecycle !== "deleted";
  const canDelete = selectedLifecycle !== "deleted";
  const hasLifecycleActions =
    Boolean(selectedArt) &&
    (canFinalize || canUnarchive || canArchive || canRemove || canDelete);

  const effectiveArtifactText =
    selectedLifecycle === "deleted"
      ? null
      : (artifactContent?.text ?? selectedArt?.content ?? null);
  const effectiveArtifactIsBinary = Boolean(artifactContent?.is_binary);
  const effectiveArtifactSha256 = artifactContent?.sha256 || selectedArt?.sha256 || "";
  const effectiveArtifactByteSize = artifactContent?.byte_size ?? null;

  const isMarkdownPreviewable = Boolean(
    selectedArt &&
      selectedLifecycle !== "deleted" &&
      !effectiveArtifactIsBinary &&
      effectiveArtifactText &&
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
      style={{ width: `${panelWidth}px`, flex: `0 0 ${panelWidth}px` }}
      aria-label="Проект, файлы и артефакты"
      data-testid="workspace-sibling-panel"
    >
      <div
        className="workspace-resize-handle"
        role="separator"
        aria-orientation="vertical"
        aria-label="Изменить ширину панели проекта"
        data-testid="workspace-resize-handle"
        onPointerDown={handleResizePointerDown}
      />

      <header className="resource-header">
        <h2>Проект и файлы</h2>
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
            title="Скрыть панель проекта (Ctrl+\)"
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
                  <p>Папка не привязана. Проект работает без локальной рабочей директории.</p>
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
                        const isSelectedFile = selectedFilePath === entry.relative_path;
                        return (
                          <button
                            key={entry.relative_path}
                            type="button"
                            className={`resource-row workspace-file-row ${
                              isSelectedFile ? "is-selected" : ""
                            }`}
                            style={{ paddingLeft: `${22 + (depth - 1) * 14}px` }}
                            title={entry.relative_path}
                            data-testid={`workspace-file-${entry.relative_path}`}
                            onClick={() => void handleSelectWorkspaceFile(entry.relative_path)}
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
                          </button>
                        );
                      })}
                      {filesData.truncated && (
                        <p className="resource-truncated">
                          Показаны первые {filesData.entries.length} элементов (лимит безопасности).
                        </p>
                      )}
                    </div>
                  )}

                  {selectedFilePath && (
                    <div className="workspace-file-preview" data-testid="workspace-file-preview">
                      <div className="workspace-file-preview__head">
                        <strong title={selectedFilePath}>{selectedFilePath}</strong>
                        <div className="workspace-file-preview__actions">
                          <button
                            type="button"
                            className="icon-button icon-button--xs"
                            title={copiedField === "file_path" ? "Путь скопирован" : "Копировать путь"}
                            aria-label="Копировать путь файла"
                            data-testid="workspace-file-copy-path"
                            onClick={() => copyText("file_path", selectedFilePath)}
                          >
                            {copiedField === "file_path" ? <Check size={12} /> : <Copy size={12} />}
                          </button>
                          <button
                            type="button"
                            className="icon-button icon-button--xs"
                            title="Вставить @путь в чат"
                            aria-label="Вставить файл в чат"
                            data-testid="workspace-file-insert-chat"
                            onClick={() => actions.insertIntoDraft(`@${selectedFilePath}`)}
                          >
                            <At size={12} />
                          </button>
                          <button
                            type="button"
                            className="icon-button icon-button--xs"
                            title="Закрыть просмотр файла"
                            aria-label="Закрыть просмотр файла"
                            onClick={() => {
                              setSelectedFilePath(null);
                              setFilePreview(null);
                              setFilePreviewError(null);
                            }}
                          >
                            <X size={12} />
                          </button>
                        </div>
                      </div>

                      {isLoadingFilePreview ? (
                        <p className="resource-empty">Загрузка файла…</p>
                      ) : filePreviewError ? (
                        <p className="resource-empty">{filePreviewError}</p>
                      ) : filePreview ? (
                        <>
                          <div className="workspace-file-preview__meta">
                            <span>{filePreview.mime_type}</span>
                            <span>{formatBytes(filePreview.byte_size)}</span>
                            {filePreview.truncated && <span>· усечён</span>}
                          </div>
                          {filePreview.is_binary ? (
                            <p className="resource-empty" data-testid="workspace-file-binary">
                              Бинарный файл ({formatBytes(filePreview.byte_size)}) — текстовый
                              просмотр недоступен.
                            </p>
                          ) : (
                            <pre
                              className="artifact-preview__body"
                              data-testid="workspace-file-content"
                            >
                              {filePreview.content ?? "(пустой файл)"}
                            </pre>
                          )}
                        </>
                      ) : null}
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
                      {selectedLifecycle !== "deleted" && (
                        <button
                          type="button"
                          className="icon-button icon-button--xs"
                          aria-label={`Скачать артефакт ${selectedArt.name}`}
                          title="Скачать артефакт"
                          data-testid="artifact-download-btn"
                          onClick={handleDownloadArtifact}
                        >
                          <DownloadSimple size={13} />
                        </button>
                      )}
                      {selectedLifecycle !== "deleted" &&
                        !effectiveArtifactIsBinary &&
                        effectiveArtifactText && (
                          <button
                            type="button"
                            className="icon-button icon-button--xs"
                            aria-label="Скопировать содержимое артефакта"
                            title={
                              copiedField === "content" ? "Скопировано" : "Копировать содержимое"
                            }
                            data-testid="artifact-copy-content"
                            onClick={() => copyText("content", effectiveArtifactText)}
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

                  {(effectiveArtifactSha256 || effectiveArtifactByteSize !== null) && (
                    <div className="artifact-sha-bar">
                      <code
                        className="artifact-sha-bar__hash"
                        title={effectiveArtifactSha256 || undefined}
                        data-testid="artifact-sha256"
                      >
                        {effectiveArtifactSha256
                          ? `SHA-256: ${effectiveArtifactSha256.slice(0, 12)}…`
                          : ""}
                        {effectiveArtifactByteSize !== null
                          ? `${effectiveArtifactSha256 ? " · " : ""}${formatBytes(
                              effectiveArtifactByteSize,
                            )}`
                          : ""}
                      </code>
                      {effectiveArtifactSha256 && (
                        <button
                          type="button"
                          className="icon-button icon-button--xs"
                          aria-label="Скопировать SHA-256"
                          title={
                            copiedField === "sha256" ? "Хеш скопирован" : "Скопировать SHA-256"
                          }
                          data-testid="artifact-copy-sha256"
                          onClick={() => copyText("sha256", effectiveArtifactSha256)}
                        >
                          {copiedField === "sha256" ? <Check size={12} /> : <Copy size={12} />}
                        </button>
                      )}
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

                  {isLoadingArtifactContent && !effectiveArtifactText ? (
                    <p className="resource-empty">Загрузка снимка артефакта…</p>
                  ) : effectiveArtifactIsBinary && selectedLifecycle !== "deleted" ? (
                    <div className="artifact-binary-box" data-testid="artifact-binary-indicator">
                      <p className="resource-empty">
                        Бинарный артефакт ({artifactContent?.mime_type ?? selectedArt.mime_type}
                        {effectiveArtifactByteSize !== null
                          ? `, ${formatBytes(effectiveArtifactByteSize)}`
                          : ""}
                        ). Используйте кнопку «Скачать» выше.
                      </p>
                    </div>
                  ) : isMarkdownPreviewable && markdownViewMode === "rendered" ? (
                    <div
                      className="artifact-preview__markdown message-markdown"
                      data-testid="artifact-preview-markdown"
                    >
                      <MarkdownRenderer content={effectiveArtifactText ?? ""} />
                    </div>
                  ) : (
                    /* Strictly plain text child — zero innerHTML/XSS */
                    <pre className="artifact-preview__body" data-testid="artifact-preview-source">
                      {selectedLifecycle === "deleted"
                        ? "(Содержимое артефакта удалено в Dennett; файлы в рабочей папке не затронуты)"
                        : (effectiveArtifactText ??
                          artifactContentError ??
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
