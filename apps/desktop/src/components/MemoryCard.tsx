import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ChangeEvent,
  type FormEvent,
} from "react";
import {
  ArrowsClockwise,
  BookOpenText,
  Brain,
  CaretDown,
  Check,
  ClockCounterClockwise,
  DownloadSimple,
  FileText,
  GitDiff,
  MagnifyingGlass,
  PencilSimple,
  Plus,
  ShieldWarning,
  Trash,
  UploadSimple,
  WarningCircle,
} from "@phosphor-icons/react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

import {
  normalizeBridgeError,
  type DocumentVersion,
  type EpistemicStatus,
  type ExportResponse,
  type ForgetResponse,
  type ForgetTargetType,
  type HeadMemorySearchResponse,
  type ImportResponse,
  type IndexStatus,
  type MemoryClaim,
  type MemoryConflict,
  type MemoryDocument,
  type MemoryNote,
  type NoteHistoryResponse,
} from "../api/contracts";
import { useWorkspace } from "../state/WorkspaceProvider";
import { PortalPopover } from "./PortalPopover";
import { ProjectScopeFilter, STANDALONE_SCOPE_VALUE } from "./ProjectScopeFilter";
import { StatusBanners } from "./StatusBanners";

type MemoryTab = "knowledge" | "conflicts" | "dossiers";

interface ForgetTargetState {
  targetType: ForgetTargetType;
  targetId: string;
  scope: string;
  label: string;
}

function readFileAsText(file: File): Promise<string> {
  if (typeof file.text === "function") {
    return file.text();
  }
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result ?? ""));
    reader.onerror = () => reject(new Error("Не удалось прочитать выбранный JSON-файл"));
    reader.readAsText(file);
  });
}

function formatDateTime(ts: number): string {
  return new Date(ts).toLocaleString("ru-RU", {
    day: "2-digit",
    month: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function indexStatusInfo(status: IndexStatus): {
  label: string;
  tone: "ok" | "warn" | "danger";
} {
  switch (status) {
    case "healthy":
      return { label: "Актуален", tone: "ok" };
    case "lagging":
      return { label: "Догоняет", tone: "warn" };
    case "rebuilding":
      return { label: "Перестроение", tone: "warn" };
    case "degraded":
      return { label: "Деградирован — свежий хвост активен", tone: "danger" };
  }
}

function epistemicStatusLabel(status: EpistemicStatus): string {
  switch (status) {
    case "observed":
      return "Наблюдение";
    case "asserted":
      return "Утверждено";
    case "inferred":
      return "Вывод";
    case "hypothesis":
      return "Гипотеза";
    case "contested":
      return "Оспорено";
    case "superseded":
      return "Заменено";
    case "retracted":
      return "Отозвано";
  }
}

function conflictTypeLabel(type: MemoryConflict["conflict_type"]): string {
  switch (type) {
    case "temporal":
      return "Временной";
    case "source":
      return "Источники";
    case "concurrent_edit":
      return "Параллельная правка";
    case "semantic":
      return "Смысловой";
    case "policy":
      return "Политика";
  }
}

function targetTypeLabel(type: ForgetTargetType): string {
  switch (type) {
    case "note":
      return "Заметка";
    case "claim":
      return "Утверждение";
    case "document":
      return "Досье";
    case "evidence":
      return "Источник";
  }
}

function makeIdempotencyKey(prefix: string): string {
  return `${prefix}-${Date.now()}-${Math.random().toString(16).slice(2, 8)}`;
}

export function MemoryCard() {
  const { state, actions, meta } = useWorkspace();
  const [activeTab, setActiveTab] = useState<MemoryTab>("knowledge");
  const [scopeSelection, setScopeSelection] = useState<string | "all">("all");
  const [queryInput, setQueryInput] = useState("");
  const [includeHistory, setIncludeHistory] = useState(false);
  const [searchResult, setSearchResult] = useState<HeadMemorySearchResponse | null>(null);
  const [isSearching, setIsSearching] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);

  // Create Note / Claim / Dossier inline forms
  const [showCreateNote, setShowCreateNote] = useState(false);
  const [noteTitle, setNoteTitle] = useState("");
  const [noteBody, setNoteBody] = useState("");
  const [noteTags, setNoteTags] = useState("");

  const [showCreateClaim, setShowCreateClaim] = useState(false);
  const [claimTopic, setClaimTopic] = useState("");
  const [claimText, setClaimText] = useState("");
  const [claimCaveats, setClaimCaveats] = useState("");

  const [showCreateDoc, setShowCreateDoc] = useState(false);
  const [docKey, setDocKey] = useState("");
  const [docTitle, setDocTitle] = useState("");
  const [docBody, setDocBody] = useState("");

  // Note history & correction
  const [historyByNoteId, setHistoryByNoteId] = useState<Record<string, NoteHistoryResponse>>({});
  const [expandedHistoryNoteId, setExpandedHistoryNoteId] = useState<string | null>(null);
  const [correctingNoteId, setCorrectingNoteId] = useState<string | null>(null);
  const [correctTitle, setCorrectTitle] = useState("");
  const [correctBody, setCorrectBody] = useState("");
  const [correctRationale, setCorrectRationale] = useState("");

  // Claim supersede
  const [supersedingClaimId, setSupersedingClaimId] = useState<string | null>(null);
  const [supersedeText, setSupersedeText] = useState("");
  const [supersedeCaveats, setSupersedeCaveats] = useState("");

  // Conflict resolution
  const [selectedWinnerByConflict, setSelectedWinnerByConflict] = useState<Record<string, string>>(
    {},
  );
  const [resolutionNoteByConflict, setResolutionNoteByConflict] = useState<Record<string, string>>(
    {},
  );
  const [resolvingConflictId, setResolvingConflictId] = useState<string | null>(null);

  // Dossiers viewer, versions PortalPopover, and optimistic editor
  const [selectedDocId, setSelectedDocId] = useState<string | null>(null);
  const [docVersions, setDocVersions] = useState<DocumentVersion[]>([]);
  const [viewedDocVersion, setViewedDocVersion] = useState<number | null>(null);
  const [versionMenuOpen, setVersionMenuOpen] = useState(false);
  const versionTriggerRef = useRef<HTMLButtonElement | null>(null);
  const [editingDoc, setEditingDoc] = useState(false);
  const [editDocTitle, setEditDocTitle] = useState("");
  const [editDocBody, setEditDocBody] = useState("");
  const [editDocSummary, setEditDocSummary] = useState("");

  // Provenance jump highlight
  const [highlightedId, setHighlightedId] = useState<string | null>(null);

  // Inline Forget confirmation (zero window.confirm!) & honest residual_backup_notice
  const [forgetTarget, setForgetTarget] = useState<ForgetTargetState | null>(null);
  const [forgetReason, setForgetReason] = useState("Удалено владельцем из памяти Dennett");
  const [isForgetting, setIsForgetting] = useState(false);
  const [lastForgetResult, setLastForgetResult] = useState<ForgetResponse | null>(null);

  // Export & Untrusted Import (Contract J)
  const [lastExport, setLastExport] = useState<ExportResponse | null>(null);
  const [lastImport, setLastImport] = useState<ImportResponse | null>(null);
  const [isExporting, setIsExporting] = useState(false);
  const [isImporting, setIsImporting] = useState(false);
  const importFileRef = useRef<HTMLInputElement | null>(null);

  const activeProjects = useMemo(
    () =>
      state.projects.filter((p) => {
        const lc = p.lifecycle_state ?? "active";
        return lc !== "archived" && lc !== "deleted_record";
      }),
    [state.projects],
  );

  const selectedMemoryScope = useMemo<string | null>(() => {
    if (scopeSelection === "all") {
      return null;
    }
    if (scopeSelection === STANDALONE_SCOPE_VALUE) {
      return STANDALONE_SCOPE_VALUE;
    }
    const found = activeProjects.find((p) => p.project_id === scopeSelection);
    return found ? found.memory_scope : null;
  }, [activeProjects, scopeSelection]);

  const defaultWriteScope = useMemo(() => {
    if (selectedMemoryScope) {
      return selectedMemoryScope;
    }
    if (meta.activeProject) {
      return meta.activeProject.memory_scope;
    }
    return activeProjects[0]?.memory_scope ?? STANDALONE_SCOPE_VALUE;
  }, [activeProjects, meta.activeProject, selectedMemoryScope]);

  const scopeLabelByScope = useMemo(() => {
    const map = new Map<string, string>();
    map.set(STANDALONE_SCOPE_VALUE, "Одиночные чаты");
    for (const proj of state.projects) {
      map.set(proj.memory_scope, proj.name);
    }
    return map;
  }, [state.projects]);

  const connected = Boolean(state.healthSnapshot?.connected);
  const overview = state.memoryOverview;
  const isDegradedUnreachable =
    overview?.memory_status === "degraded_unreachable" ||
    state.healthSnapshot?.health?.memory_status === "degraded_unreachable";

  const reloadOverview = useCallback(async () => {
    if (!connected) {
      return;
    }
    await actions.refreshMemoryOverview(selectedMemoryScope);
  }, [actions, connected, selectedMemoryScope]);

  useEffect(() => {
    if (connected) {
      void reloadOverview();
    }
  }, [connected, reloadOverview]);

  const runSearch = useCallback(
    async (q: string, historyFlag: boolean, scope: string | null) => {
      if (!connected) {
        return;
      }
      setIsSearching(true);
      setLocalError(null);
      try {
        const resp = await meta.bridge.searchMemory({
          scope,
          query: q.trim() || null,
          limit: 100,
          include_history: historyFlag,
        });
        setSearchResult(resp);
      } catch (err) {
        setLocalError(normalizeBridgeError(err).message);
      } finally {
        setIsSearching(false);
      }
    },
    [connected, meta.bridge],
  );

  // Debounced search whenever query, includeHistory, or scope changes
  useEffect(() => {
    if (!connected) {
      return;
    }
    const trimmed = queryInput.trim();
    if (!trimmed && !includeHistory) {
      setSearchResult(null);
      return;
    }
    const timer = window.setTimeout(() => {
      void runSearch(trimmed, includeHistory, selectedMemoryScope);
    }, 180);
    return () => window.clearTimeout(timer);
  }, [connected, includeHistory, queryInput, runSearch, selectedMemoryScope]);

  const effectiveIndexStatus: IndexStatus =
    searchResult?.index_status ?? overview?.stats?.index_status ?? "healthy";
  const indexBadge = indexStatusInfo(effectiveIndexStatus);
  const effectiveStaleWarning =
    searchResult?.stale_warning ??
    (effectiveIndexStatus === "degraded"
      ? "Поисковый индекс деградирован — свежий хвост активен, новые и актуальные записи доступны."
      : effectiveIndexStatus === "rebuilding"
        ? "Идёт перестроение поискового индекса — свежий хвост активен."
        : effectiveIndexStatus === "lagging"
          ? "Поисковый индекс догоняет канонический журнал — свежий хвост активен."
          : null);

  const visibleNotes = useMemo<MemoryNote[]>(() => {
    if (searchResult && searchResult.memory_status === "healthy") {
      return searchResult.notes;
    }
    const all = overview?.notes ?? [];
    if (!selectedMemoryScope) {
      return all;
    }
    return all.filter((n) => n.scope === selectedMemoryScope);
  }, [overview?.notes, searchResult, selectedMemoryScope]);

  const allClaims = useMemo<MemoryClaim[]>(() => {
    const raw = overview?.claims ?? [];
    return selectedMemoryScope ? raw.filter((c) => c.scope === selectedMemoryScope) : raw;
  }, [overview?.claims, selectedMemoryScope]);

  const claimById = useMemo(() => {
    const map = new Map<string, MemoryClaim>();
    for (const c of overview?.claims ?? []) {
      map.set(c.claim_id, c);
    }
    return map;
  }, [overview?.claims]);

  const visibleClaims = useMemo<MemoryClaim[]>(() => {
    const q = queryInput.trim().toLowerCase();
    return allClaims.filter((c) => {
      if (!includeHistory && (c.epistemic_status === "superseded" || c.epistemic_status === "retracted")) {
        return false;
      }
      if (!q) {
        return true;
      }
      return (
        c.topic_key.toLowerCase().includes(q) ||
        c.natural_language.toLowerCase().includes(q) ||
        (c.caveats ?? []).some((cav) => cav.toLowerCase().includes(q))
      );
    });
  }, [allClaims, includeHistory, queryInput]);

  const visibleConflicts = useMemo<MemoryConflict[]>(() => {
    const raw = overview?.conflicts ?? [];
    const scoped = selectedMemoryScope ? raw.filter((c) => c.scope === selectedMemoryScope) : raw;
    const q = queryInput.trim().toLowerCase();
    if (!q) {
      return scoped;
    }
    return scoped.filter((c) => {
      const topicMatch = c.topic_key?.toLowerCase().includes(q) ?? false;
      const candMatch = c.candidate_claim_ids.some((id) =>
        claimById.get(id)?.natural_language.toLowerCase().includes(q),
      );
      return topicMatch || candMatch;
    });
  }, [claimById, overview?.conflicts, queryInput, selectedMemoryScope]);

  const openConflicts = useMemo(
    () => visibleConflicts.filter((c) => c.status === "open"),
    [visibleConflicts],
  );
  const resolvedConflicts = useMemo(
    () => visibleConflicts.filter((c) => c.status !== "open"),
    [visibleConflicts],
  );

  const visibleDocuments = useMemo<MemoryDocument[]>(() => {
    const raw = overview?.documents ?? [];
    const scoped = selectedMemoryScope ? raw.filter((d) => d.scope === selectedMemoryScope) : raw;
    const q = queryInput.trim().toLowerCase();
    if (!q) {
      return scoped;
    }
    return scoped.filter(
      (d) =>
        d.title.toLowerCase().includes(q) ||
        d.document_key.toLowerCase().includes(q) ||
        d.current_body.toLowerCase().includes(q),
    );
  }, [overview?.documents, queryInput, selectedMemoryScope]);

  const visibleTombstones = useMemo(() => {
    const raw = overview?.tombstones ?? [];
    return selectedMemoryScope ? raw.filter((t) => t.scope === selectedMemoryScope) : raw;
  }, [overview?.tombstones, selectedMemoryScope]);

  // Select first dossier automatically when switching to dossiers tab
  useEffect(() => {
    if (visibleDocuments.length === 0) {
      setSelectedDocId(null);
      setDocVersions([]);
      setViewedDocVersion(null);
      return;
    }
    if (!selectedDocId || !visibleDocuments.some((d) => d.document_id === selectedDocId)) {
      const first = visibleDocuments[0]!;
      setSelectedDocId(first.document_id);
      setViewedDocVersion(first.current_version);
      setEditingDoc(false);
    }
  }, [selectedDocId, visibleDocuments]);

  const selectedDoc = useMemo(
    () => visibleDocuments.find((d) => d.document_id === selectedDocId) ?? null,
    [selectedDocId, visibleDocuments],
  );

  const loadDocVersions = useCallback(
    async (docId: string) => {
      if (!connected) {
        return;
      }
      try {
        const resp = await meta.bridge.getMemoryDocumentVersions(docId);
        setDocVersions(resp.versions);
      } catch {
        setDocVersions([]);
      }
    },
    [connected, meta.bridge],
  );

  useEffect(() => {
    if (activeTab === "dossiers" && selectedDoc) {
      void loadDocVersions(selectedDoc.document_id);
    }
  }, [activeTab, loadDocVersions, selectedDoc]);

  const activeDocVersionObj = useMemo(() => {
    if (!selectedDoc) {
      return null;
    }
    const targetVer = viewedDocVersion ?? selectedDoc.current_version;
    return docVersions.find((v) => v.version === targetVer) ?? null;
  }, [docVersions, selectedDoc, viewedDocVersion]);

  const countByProjectId = useMemo(() => {
    const map = new Map<string, number>();
    const notes = overview?.notes ?? [];
    const claims = overview?.claims ?? [];
    const docs = overview?.documents ?? [];
    for (const proj of activeProjects) {
      const c =
        notes.filter((n) => n.scope === proj.memory_scope).length +
        claims.filter((cl) => cl.scope === proj.memory_scope).length +
        docs.filter((d) => d.scope === proj.memory_scope).length;
      map.set(proj.project_id, c);
    }
    return map;
  }, [activeProjects, overview?.claims, overview?.documents, overview?.notes]);

  const standaloneItemsCount = useMemo(() => {
    const notes = (overview?.notes ?? []).filter((n) => n.scope === STANDALONE_SCOPE_VALUE).length;
    const claims = (overview?.claims ?? []).filter((c) => c.scope === STANDALONE_SCOPE_VALUE).length;
    const docs = (overview?.documents ?? []).filter((d) => d.scope === STANDALONE_SCOPE_VALUE).length;
    return notes + claims + docs;
  }, [overview?.claims, overview?.documents, overview?.notes]);

  const totalItemsCount =
    (overview?.notes.length ?? 0) +
    (overview?.claims.length ?? 0) +
    (overview?.documents.length ?? 0);

  // Handlers
  const handleSearchSubmit = (event: FormEvent) => {
    event.preventDefault();
    void runSearch(queryInput, includeHistory, selectedMemoryScope);
  };

  const handleCreateNote = async (event: FormEvent) => {
    event.preventDefault();
    if (!noteTitle.trim() || !noteBody.trim()) {
      return;
    }
    setLocalError(null);
    try {
      const tags = noteTags
        .split(",")
        .map((t) => t.trim())
        .filter(Boolean);
      await meta.bridge.commitMemoryNote({
        scope: defaultWriteScope,
        title: noteTitle.trim(),
        body: noteBody.trim(),
        tags,
        idempotency_key: makeIdempotencyKey("ui-note"),
      });
      setNoteTitle("");
      setNoteBody("");
      setNoteTags("");
      setShowCreateNote(false);
      await reloadOverview();
    } catch (err) {
      setLocalError(normalizeBridgeError(err).message);
    }
  };

  const handleCreateClaim = async (event: FormEvent) => {
    event.preventDefault();
    if (!claimTopic.trim() || !claimText.trim()) {
      return;
    }
    setLocalError(null);
    try {
      const caveats = claimCaveats
        .split(/\r?\n/)
        .map((line) => line.trim())
        .filter(Boolean);
      await meta.bridge.createMemoryClaim({
        scope: defaultWriteScope,
        topic_key: claimTopic.trim(),
        natural_language: claimText.trim(),
        epistemic_status: "asserted",
        caveats,
        idempotency_key: makeIdempotencyKey("ui-claim"),
      });
      setClaimTopic("");
      setClaimText("");
      setClaimCaveats("");
      setShowCreateClaim(false);
      await reloadOverview();
    } catch (err) {
      setLocalError(normalizeBridgeError(err).message);
    }
  };

  const handleToggleNoteHistory = async (noteId: string) => {
    if (expandedHistoryNoteId === noteId) {
      setExpandedHistoryNoteId(null);
      return;
    }
    setLocalError(null);
    try {
      const hist = await meta.bridge.getMemoryNoteHistory(noteId);
      setHistoryByNoteId((prev) => ({ ...prev, [noteId]: hist }));
      setExpandedHistoryNoteId(noteId);
    } catch (err) {
      setLocalError(normalizeBridgeError(err).message);
    }
  };

  const startCorrectNote = (note: MemoryNote) => {
    setCorrectingNoteId(note.note_id);
    setCorrectTitle(note.title);
    setCorrectBody(note.body);
    setCorrectRationale("");
  };

  const handleCorrectNoteSubmit = async (note: MemoryNote, event: FormEvent) => {
    event.preventDefault();
    if (!correctTitle.trim() || !correctBody.trim() || !correctRationale.trim()) {
      return;
    }
    setLocalError(null);
    try {
      await meta.bridge.correctMemoryNote({
        note_id: note.note_id,
        scope: note.scope,
        expected_revision: note.current_revision ?? note.revision ?? 1,
        title: correctTitle.trim(),
        body: correctBody.trim(),
        tags: note.tags,
        rationale: correctRationale.trim(),
        idempotency_key: makeIdempotencyKey("ui-correct"),
      });
      setCorrectingNoteId(null);
      await reloadOverview();
      if (expandedHistoryNoteId === note.note_id) {
        const hist = await meta.bridge.getMemoryNoteHistory(note.note_id);
        setHistoryByNoteId((prev) => ({ ...prev, [note.note_id]: hist }));
      }
    } catch (err) {
      setLocalError(normalizeBridgeError(err).message);
    }
  };

  const handleSupersedeClaim = async (claim: MemoryClaim, event: FormEvent) => {
    event.preventDefault();
    if (!supersedeText.trim()) {
      return;
    }
    setLocalError(null);
    try {
      const caveats = supersedeCaveats
        .split(/\r?\n/)
        .map((l) => l.trim())
        .filter(Boolean);
      await meta.bridge.supersedeMemoryClaim({
        claim_id: claim.claim_id,
        scope: claim.scope,
        new_claim: {
          scope: claim.scope,
          topic_key: claim.topic_key,
          claim_key: claim.claim_key ?? null,
          natural_language: supersedeText.trim(),
          epistemic_status: "asserted",
          caveats,
          idempotency_key: makeIdempotencyKey("ui-supersede"),
        },
      });
      setSupersedingClaimId(null);
      setSupersedeText("");
      setSupersedeCaveats("");
      await reloadOverview();
    } catch (err) {
      setLocalError(normalizeBridgeError(err).message);
    }
  };

  const handleResolveConflict = async (conflict: MemoryConflict, explicitWinnerId?: string) => {
    const winningClaimId =
      explicitWinnerId ??
      selectedWinnerByConflict[conflict.conflict_id] ??
      conflict.candidate_claim_ids[0] ??
      null;
    const note =
      resolutionNoteByConflict[conflict.conflict_id]?.trim() ||
      "Разрешено владельцем в разделе Память";
    setResolvingConflictId(conflict.conflict_id);
    setLocalError(null);
    try {
      await meta.bridge.resolveMemoryConflict({
        conflict_id: conflict.conflict_id,
        scope: conflict.scope,
        resolution_note: note,
        winning_claim_id: winningClaimId,
        target_status: "resolved",
        idempotency_key: makeIdempotencyKey("ui-conflict"),
      });
      await reloadOverview();
    } catch (err) {
      setLocalError(normalizeBridgeError(err).message);
    } finally {
      setResolvingConflictId(null);
    }
  };

  const handleCreateDocument = async (event: FormEvent) => {
    event.preventDefault();
    if (!docTitle.trim() || !docBody.trim()) {
      return;
    }
    const normalizedKey =
      docKey.trim() ||
      `dossier:${docTitle
        .trim()
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, "-")
        .replace(/^-+|-+$/g, "") || Date.now().toString(16)}`;
    setLocalError(null);
    try {
      const created = await meta.bridge.createMemoryDocument({
        scope: defaultWriteScope,
        document_key: normalizedKey,
        title: docTitle.trim(),
        body: docBody.trim(),
        provenance_claim_ids: [],
        provenance_note_ids: [],
        idempotency_key: makeIdempotencyKey("ui-doc"),
      });
      setDocKey("");
      setDocTitle("");
      setDocBody("");
      setShowCreateDoc(false);
      await reloadOverview();
      setSelectedDocId(created.document_id);
      setViewedDocVersion(created.current_version);
      await loadDocVersions(created.document_id);
    } catch (err) {
      setLocalError(normalizeBridgeError(err).message);
    }
  };

  const startEditDoc = (doc: MemoryDocument) => {
    setEditDocTitle(doc.title);
    setEditDocBody(doc.current_body);
    setEditDocSummary("");
    setEditingDoc(true);
  };

  const handleUpdateDocument = async (doc: MemoryDocument, event: FormEvent) => {
    event.preventDefault();
    if (!editDocTitle.trim() || !editDocBody.trim()) {
      return;
    }
    setLocalError(null);
    try {
      const updated = await meta.bridge.updateMemoryDocument({
        document_id: doc.document_id,
        scope: doc.scope,
        expected_version: doc.current_version,
        title: editDocTitle.trim(),
        body: editDocBody,
        provenance_claim_ids: doc.provenance_claim_ids ?? [],
        provenance_note_ids: doc.provenance_note_ids ?? [],
        diff_summary: editDocSummary.trim() || `Правка v${doc.current_version + 1}`,
        idempotency_key: makeIdempotencyKey("ui-doc-upd"),
      });
      setEditingDoc(false);
      setViewedDocVersion(updated.current_version);
      await reloadOverview();
      await loadDocVersions(updated.document_id);
    } catch (err) {
      setLocalError(normalizeBridgeError(err).message);
    }
  };

  const handleConfirmForget = async () => {
    if (!forgetTarget || !forgetReason.trim()) {
      return;
    }
    setIsForgetting(true);
    setLocalError(null);
    try {
      const resp = await meta.bridge.forgetMemoryItem({
        scope: forgetTarget.scope,
        target_type: forgetTarget.targetType,
        target_id: forgetTarget.targetId,
        reason: forgetReason.trim(),
        idempotency_key: makeIdempotencyKey("ui-forget"),
      });
      setLastForgetResult(resp);
      setForgetTarget(null);
      await reloadOverview();
      if (queryInput.trim() || includeHistory) {
        await runSearch(queryInput, includeHistory, selectedMemoryScope);
      }
    } catch (err) {
      setLocalError(normalizeBridgeError(err).message);
    } finally {
      setIsForgetting(false);
    }
  };

  const handleExport = async () => {
    setIsExporting(true);
    setLocalError(null);
    try {
      const exported = await meta.bridge.exportMemoryPackage({
        scope: defaultWriteScope,
        format_version: "1.0",
      });
      setLastExport(exported);
      // Trigger browser/WebView JSON download without secrets
      if (typeof URL !== "undefined" && typeof URL.createObjectURL === "function") {
        const blob = new Blob([exported.package_json], { type: "application/json" });
        const url = URL.createObjectURL(blob);
        const anchor = document.createElement("a");
        anchor.href = url;
        anchor.download = `dennett-memory-${exported.scope.replace(/[^a-z0-9_-]+/gi, "-")}.json`;
        document.body.appendChild(anchor);
        anchor.click();
        anchor.remove();
        URL.revokeObjectURL(url);
      }
    } catch (err) {
      setLocalError(normalizeBridgeError(err).message);
    } finally {
      setIsExporting(false);
    }
  };

  const handleImportFileChange = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    event.target.value = "";
    if (!file) {
      return;
    }
    setIsImporting(true);
    setLocalError(null);
    try {
      const text = await readFileAsText(file);
      const imported = await meta.bridge.importMemoryPackage({
        scope: defaultWriteScope,
        package_json: text,
      });
      setLastImport(imported);
      await reloadOverview();
    } catch (err) {
      setLocalError(normalizeBridgeError(err).message);
    } finally {
      setIsImporting(false);
    }
  };

  const jumpToProvenance = (entityId: string) => {
    setActiveTab("knowledge");
    setHighlightedId(entityId);
    window.setTimeout(() => {
      const el = document.getElementById(`memory-entity-${entityId}`);
      if (el && typeof el.scrollIntoView === "function") {
        el.scrollIntoView({ behavior: "smooth", block: "center" });
      }
    }, 50);
  };

  return (
    <main className="chat-card d2-surface-card memory-card" data-testid="memory-card">
      <StatusBanners />

      <header className="d2-card-header memory-card__header">
        <div className="d2-card-header__title">
          <Brain size={18} aria-hidden="true" />
          <div>
            <h1>Память</h1>
            <p>
              Знания, утверждения, разрешение конфликтов и версионированные досье независимого
              memoryd
            </p>
          </div>
        </div>

        <div className="d2-card-header__controls memory-header-controls">
          <ProjectScopeFilter
            projects={activeProjects}
            selectedProjectId={scopeSelection}
            onChange={setScopeSelection}
            totalCount={totalItemsCount}
            countByProjectId={countByProjectId}
            standaloneCount={standaloneItemsCount}
            includeStandalone={true}
            allLabel="Все области"
            ariaLabel="Фильтр области памяти"
          />

          {!isDegradedUnreachable && (
            <span
              className={`d2-pill memory-index-pill memory-index-pill--${indexBadge.tone}`}
              data-testid="memory-index-status-badge"
              title="Состояние перестраиваемого поискового индекса memoryd"
            >
              Индекс: {indexBadge.label}
            </span>
          )}

          <button
            type="button"
            className="popover-secondary-btn"
            onClick={() => void handleExport()}
            disabled={!connected || isDegradedUnreachable || isExporting}
            data-testid="memory-export-btn"
            title="Экспортировать детерминированный JSON без секретов"
          >
            <DownloadSimple size={14} aria-hidden="true" />
            Экспорт
          </button>

          <button
            type="button"
            className="popover-secondary-btn"
            onClick={() => importFileRef.current?.click()}
            disabled={!connected || isDegradedUnreachable || isImporting}
            data-testid="memory-import-btn"
            title="Импортировать JSON-пакет как недоверенные внешние данные"
          >
            <UploadSimple size={14} aria-hidden="true" />
            Импорт
          </button>
          <input
            ref={importFileRef}
            type="file"
            accept=".json,application/json"
            className="sr-only-file-input"
            data-testid="memory-import-file-input"
            aria-label="Выбрать JSON-файл для импорта памяти"
            onChange={(e) => void handleImportFileChange(e)}
          />

          <button
            type="button"
            className="icon-button"
            onClick={() => void reloadOverview()}
            disabled={!connected || state.isLoadingMemoryOverview}
            aria-label="Обновить данные памяти"
            title="Обновить"
          >
            <ArrowsClockwise
              size={16}
              className={state.isLoadingMemoryOverview ? "spin" : ""}
            />
          </button>
        </div>
      </header>

      <div className="memory-toolbar">
        <form className="memory-search-form" onSubmit={handleSearchSubmit}>
          <MagnifyingGlass size={14} aria-hidden="true" />
          <input
            type="search"
            value={queryInput}
            onChange={(e) => setQueryInput(e.target.value)}
            placeholder="Поиск по заметкам, утверждениям и темам…"
            aria-label="Поиск по памяти"
            data-testid="memory-search-input"
            disabled={!connected || isDegradedUnreachable}
          />
        </form>

        <button
          type="button"
          role="switch"
          aria-checked={includeHistory}
          className={`memory-history-switch ${includeHistory ? "is-checked" : ""}`}
          onClick={() => setIncludeHistory((prev) => !prev)}
          disabled={!connected || isDegradedUnreachable}
          data-testid="memory-include-history-toggle"
          title="Включать исторические ревизии и заменённые утверждения в выдачу"
        >
          <ClockCounterClockwise size={14} aria-hidden="true" />
          <span>История</span>
          <span className="memory-switch-track" aria-hidden="true">
            <span className="memory-switch-thumb" />
          </span>
        </button>

        <div className="d2-segmented" role="tablist" aria-label="Разделы памяти">
          <button
            type="button"
            role="tab"
            aria-selected={activeTab === "knowledge"}
            className={activeTab === "knowledge" ? "is-active" : ""}
            onClick={() => setActiveTab("knowledge")}
            data-testid="memory-tab-knowledge"
          >
            Знания и заметки ({visibleNotes.length + visibleClaims.length})
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={activeTab === "conflicts"}
            className={activeTab === "conflicts" ? "is-active" : ""}
            onClick={() => setActiveTab("conflicts")}
            data-testid="memory-tab-conflicts"
          >
            Конфликты ({openConflicts.length})
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={activeTab === "dossiers"}
            className={activeTab === "dossiers" ? "is-active" : ""}
            onClick={() => setActiveTab("dossiers")}
            data-testid="memory-tab-dossiers"
          >
            Досье ({visibleDocuments.length})
          </button>
        </div>
      </div>

      <div className="d2-card-scroll">
        <div className="d2-card-inner memory-inner">
          {localError && (
            <div className="memory-alert memory-alert--danger" role="alert">
              <WarningCircle size={15} aria-hidden="true" />
              <span>{localError}</span>
              <button
                type="button"
                className="popover-action-link"
                onClick={() => setLocalError(null)}
              >
                Скрыть
              </button>
            </div>
          )}

          {isDegradedUnreachable ? (
            <section
              className="memory-degraded-panel"
              role="alert"
              data-testid="memory-degraded-state"
            >
              <ShieldWarning size={26} aria-hidden="true" />
              <h2>Память memoryd недоступна (DegradedUnreachable)</h2>
              <p>
                {overview?.degraded_reason ??
                  state.healthSnapshot?.health?.memory_detail ??
                  "Сервис dennett-memoryd не отвечает. Записи не подменяются и не выдумываются — после восстановления связи данные снова станут доступны."}
              </p>
              <button
                type="button"
                className="popover-primary-btn"
                onClick={() => void reloadOverview()}
                disabled={!connected}
                data-testid="memory-degraded-retry-btn"
              >
                Повторить проверку memoryd
              </button>
            </section>
          ) : (
            <>
              {effectiveIndexStatus !== "healthy" && effectiveStaleWarning && (
                <div
                  className="memory-alert memory-alert--warn"
                  role="status"
                  data-testid="memory-index-degraded-banner"
                >
                  <WarningCircle size={15} aria-hidden="true" />
                  <span>{effectiveStaleWarning}</span>
                  <button
                    type="button"
                    className="popover-secondary-btn"
                    onClick={() => void actions.triggerMemoryIndexRebuild(defaultWriteScope)}
                    disabled={state.isRebuildingIndex}
                    data-testid="memory-rebuild-index-btn"
                  >
                    {state.isRebuildingIndex ? "Перестроение…" : "Пересобрать индекс"}
                  </button>
                </div>
              )}

              {lastExport && (
                <div className="memory-alert memory-alert--ok" data-testid="memory-export-banner">
                  <DownloadSimple size={15} aria-hidden="true" />
                  <span>
                    Экспортировано элементов: {lastExport.item_count} (секреты удалены) · SHA-256:{" "}
                    <code>{lastExport.manifest_sha256.slice(0, 16)}…</code>
                  </span>
                  <button
                    type="button"
                    className="popover-action-link"
                    onClick={() => setLastExport(null)}
                  >
                    Скрыть
                  </button>
                </div>
              )}

              {lastImport && (
                <div className="memory-alert memory-alert--warn" data-testid="memory-import-banner">
                  <ShieldWarning size={15} aria-hidden="true" />
                  <div className="memory-alert__body">
                    <strong>Недоверенный импорт · инструкции отключены</strong>
                    <span>
                      Импортировано: {lastImport.items_imported}, отклонено:{" "}
                      {lastImport.items_rejected} (уровень доверия: {lastImport.trust_level})
                    </span>
                    {lastImport.validation_errors && lastImport.validation_errors.length > 0 && (
                      <small>{lastImport.validation_errors.join("; ")}</small>
                    )}
                  </div>
                  <button
                    type="button"
                    className="popover-action-link"
                    onClick={() => setLastImport(null)}
                  >
                    Скрыть
                  </button>
                </div>
              )}

              {forgetTarget && (
                <section
                  className="memory-forget-confirm"
                  role="alertdialog"
                  aria-label="Подтверждение забывания записи"
                  data-testid="memory-forget-confirm"
                >
                  <div className="memory-forget-confirm__head">
                    <Trash size={16} aria-hidden="true" />
                    <strong>
                      Забыть ({targetTypeLabel(forgetTarget.targetType)}): «{forgetTarget.label}»?
                    </strong>
                  </div>
                  <p>
                    Запись будет немедленно удалена из активных проекций и поискового индекса с
                    фиксацией content-free tombstone в журнале аудита.
                  </p>
                  <div className="d2-inline-reply">
                    <input
                      type="text"
                      value={forgetReason}
                      onChange={(e) => setForgetReason(e.target.value)}
                      placeholder="Причина забывания (без секретов и содержимого)…"
                      aria-label="Причина забывания"
                      data-testid="memory-forget-reason-input"
                    />
                    <button
                      type="button"
                      className="d2-danger-btn"
                      onClick={() => void handleConfirmForget()}
                      disabled={isForgetting || !forgetReason.trim()}
                      data-testid="memory-forget-confirm-btn"
                    >
                      {isForgetting ? "Удаление…" : "Подтвердить забывание"}
                    </button>
                    <button
                      type="button"
                      className="popover-secondary-btn"
                      onClick={() => setForgetTarget(null)}
                    >
                      Отмена
                    </button>
                  </div>
                </section>
              )}

              {lastForgetResult && (
                <div
                  className="memory-alert memory-alert--warn"
                  role="status"
                  data-testid="memory-forget-result-banner"
                >
                  <Trash size={15} aria-hidden="true" />
                  <div className="memory-alert__body">
                    <strong>
                      Запись забыта (статус: {lastForgetResult.propagation_status})
                    </strong>
                    <span>{lastForgetResult.residual_backup_notice}</span>
                  </div>
                  <button
                    type="button"
                    className="popover-action-link"
                    onClick={() => setLastForgetResult(null)}
                  >
                    Скрыть
                  </button>
                </div>
              )}

              {/* TAB 1: Knowledge & Notes */}
              {activeTab === "knowledge" && (
                <>
                  <div className="memory-create-bar">
                    <button
                      type="button"
                      className="popover-secondary-btn"
                      onClick={() => {
                        setShowCreateNote((v) => !v);
                        setShowCreateClaim(false);
                      }}
                      data-testid="memory-toggle-create-note"
                    >
                      <Plus size={14} aria-hidden="true" />
                      Новая заметка
                    </button>
                    <button
                      type="button"
                      className="popover-secondary-btn"
                      onClick={() => {
                        setShowCreateClaim((v) => !v);
                        setShowCreateNote(false);
                      }}
                      data-testid="memory-toggle-create-claim"
                    >
                      <Plus size={14} aria-hidden="true" />
                      Новое утверждение
                    </button>
                    {isSearching && <span className="d2-time">Поиск…</span>}
                  </div>

                  {showCreateNote && (
                    <form
                      className="d2-item-card memory-inline-form"
                      onSubmit={(e) => void handleCreateNote(e)}
                      data-testid="memory-create-note-form"
                    >
                      <strong>
                        Новая заметка ({scopeLabelByScope.get(defaultWriteScope) ?? defaultWriteScope})
                      </strong>
                      <input
                        type="text"
                        value={noteTitle}
                        onChange={(e) => setNoteTitle(e.target.value)}
                        placeholder="Заголовок заметки"
                        aria-label="Заголовок заметки"
                        required
                      />
                      <textarea
                        rows={3}
                        value={noteBody}
                        onChange={(e) => setNoteBody(e.target.value)}
                        placeholder="Факты, решения и контекст…"
                        aria-label="Текст заметки"
                        required
                      />
                      <input
                        type="text"
                        value={noteTags}
                        onChange={(e) => setNoteTags(e.target.value)}
                        placeholder="Теги через запятую (необязательно)"
                        aria-label="Теги заметки"
                      />
                      <div className="d2-item-card__actions">
                        <button type="submit" className="popover-primary-btn">
                          Сохранить заметку
                        </button>
                        <button
                          type="button"
                          className="popover-secondary-btn"
                          onClick={() => setShowCreateNote(false)}
                        >
                          Отмена
                        </button>
                      </div>
                    </form>
                  )}

                  {showCreateClaim && (
                    <form
                      className="d2-item-card memory-inline-form"
                      onSubmit={(e) => void handleCreateClaim(e)}
                      data-testid="memory-create-claim-form"
                    >
                      <strong>
                        Новое утверждение (
                        {scopeLabelByScope.get(defaultWriteScope) ?? defaultWriteScope})
                      </strong>
                      <input
                        type="text"
                        value={claimTopic}
                        onChange={(e) => setClaimTopic(e.target.value)}
                        placeholder="Ключ темы (например architecture:storage)"
                        aria-label="Ключ темы утверждения"
                        required
                      />
                      <textarea
                        rows={2}
                        value={claimText}
                        onChange={(e) => setClaimText(e.target.value)}
                        placeholder="Формулировка утверждения…"
                        aria-label="Текст утверждения"
                        required
                      />
                      <textarea
                        rows={2}
                        value={claimCaveats}
                        onChange={(e) => setClaimCaveats(e.target.value)}
                        placeholder="Оговорки и исключения (по одной на строку, необязательно)"
                        aria-label="Оговорки утверждения"
                      />
                      <div className="d2-item-card__actions">
                        <button type="submit" className="popover-primary-btn">
                          Сохранить утверждение
                        </button>
                        <button
                          type="button"
                          className="popover-secondary-btn"
                          onClick={() => setShowCreateClaim(false)}
                        >
                          Отмена
                        </button>
                      </div>
                    </form>
                  )}

                  {/* Claims section */}
                  <section className="d2-section" data-testid="memory-claims-section">
                    <div className="d2-section__head">
                      <h2>
                        Утверждения <span>{visibleClaims.length}</span>
                      </h2>
                    </div>

                    {visibleClaims.length === 0 ? (
                      <p className="d2-section-empty">
                        Актуальных утверждений в выбранной области пока нет.
                      </p>
                    ) : (
                      <div className="d2-card-list">
                        {visibleClaims.map((claim) => {
                          const isUntrusted = claim.trust_level === "external_untrusted";
                          const isHighlighted = highlightedId === claim.claim_id;
                          return (
                            <article
                              key={claim.claim_id}
                              id={`memory-entity-${claim.claim_id}`}
                              className={`d2-item-card ${isHighlighted ? "memory-entity--highlighted" : ""}`}
                              data-testid={`memory-claim-${claim.claim_id}`}
                              data-highlighted={isHighlighted ? "true" : "false"}
                            >
                              <div className="d2-item-card__top">
                                <div className="d2-item-card__badges">
                                  <span className="d2-meta-chip">
                                    <code>{claim.topic_key}</code>
                                  </span>
                                  <span
                                    className={`d2-pill memory-epistemic--${claim.epistemic_status}`}
                                  >
                                    {epistemicStatusLabel(claim.epistemic_status)}
                                  </span>
                                  {isUntrusted && (
                                    <span
                                      className="d2-pill d2-pill--warn"
                                      data-testid={`memory-untrusted-badge-${claim.claim_id}`}
                                    >
                                      Недоверенный импорт · инструкции отключены
                                    </span>
                                  )}
                                </div>
                                <div className="d2-item-card__meta-right">
                                  <span className="d2-meta-chip">
                                    {scopeLabelByScope.get(claim.scope) ?? claim.scope}
                                  </span>
                                  <span className="d2-time">
                                    {formatDateTime(claim.created_at)}
                                  </span>
                                </div>
                              </div>

                              <p className="d2-item-card__detail memory-body-text">
                                {claim.natural_language}
                              </p>

                              {claim.caveats && claim.caveats.length > 0 && (
                                <div
                                  className="memory-caveats-box"
                                  data-testid={`memory-claim-caveats-${claim.claim_id}`}
                                >
                                  <strong>Оговорки:</strong>
                                  <ul>
                                    {claim.caveats.map((cav, idx) => (
                                      <li key={`${claim.claim_id}-cav-${idx}`}>{cav}</li>
                                    ))}
                                  </ul>
                                </div>
                              )}

                              {supersedingClaimId === claim.claim_id ? (
                                <form
                                  className="memory-inline-form"
                                  onSubmit={(e) => void handleSupersedeClaim(claim, e)}
                                >
                                  <textarea
                                    rows={2}
                                    value={supersedeText}
                                    onChange={(e) => setSupersedeText(e.target.value)}
                                    placeholder="Новое уточнённое утверждение…"
                                    aria-label="Текст заменяющего утверждения"
                                    required
                                  />
                                  <input
                                    type="text"
                                    value={supersedeCaveats}
                                    onChange={(e) => setSupersedeCaveats(e.target.value)}
                                    placeholder="Оговорки (необязательно)"
                                    aria-label="Оговорки заменяющего утверждения"
                                  />
                                  <div className="d2-item-card__actions">
                                    <button type="submit" className="popover-primary-btn">
                                      Заменить
                                    </button>
                                    <button
                                      type="button"
                                      className="popover-secondary-btn"
                                      onClick={() => setSupersedingClaimId(null)}
                                    >
                                      Отмена
                                    </button>
                                  </div>
                                </form>
                              ) : (
                                <div className="d2-item-card__actions">
                                  {claim.epistemic_status !== "superseded" &&
                                    claim.epistemic_status !== "retracted" && (
                                      <button
                                        type="button"
                                        className="d2-link-btn"
                                        onClick={() => {
                                          setSupersedingClaimId(claim.claim_id);
                                          setSupersedeText(claim.natural_language);
                                          setSupersedeCaveats((claim.caveats ?? []).join("\n"));
                                        }}
                                        data-testid={`memory-claim-supersede-btn-${claim.claim_id}`}
                                      >
                                        <PencilSimple size={13} aria-hidden="true" />
                                        Уточнить / заменить
                                      </button>
                                    )}
                                  <button
                                    type="button"
                                    className="d2-danger-btn d2-danger-btn--sm"
                                    onClick={() =>
                                      setForgetTarget({
                                        targetType: "claim",
                                        targetId: claim.claim_id,
                                        scope: claim.scope,
                                        label: claim.topic_key,
                                      })
                                    }
                                    data-testid={`memory-claim-forget-btn-${claim.claim_id}`}
                                  >
                                    <Trash size={13} aria-hidden="true" />
                                    Забыть
                                  </button>
                                </div>
                              )}
                            </article>
                          );
                        })}
                      </div>
                    )}
                  </section>

                  {/* Notes section */}
                  <section className="d2-section" data-testid="memory-notes-section">
                    <div className="d2-section__head">
                      <h2>
                        Заметки <span>{visibleNotes.length}</span>
                      </h2>
                    </div>

                    {visibleNotes.length === 0 ? (
                      <div className="d2-empty-box" data-testid="memory-notes-empty">
                        <FileText size={22} aria-hidden="true" />
                        <p>В выбранной области пока нет заметок.</p>
                      </div>
                    ) : (
                      <div className="d2-card-list">
                        {visibleNotes.map((note) => {
                          const rev = note.current_revision ?? note.revision ?? 1;
                          const isUntrusted = note.trust_level === "external_untrusted";
                          const isHighlighted = highlightedId === note.note_id;
                          const history = historyByNoteId[note.note_id];
                          return (
                            <article
                              key={note.note_id}
                              id={`memory-entity-${note.note_id}`}
                              className={`d2-item-card ${isHighlighted ? "memory-entity--highlighted" : ""}`}
                              data-testid={`memory-note-${note.note_id}`}
                              data-highlighted={isHighlighted ? "true" : "false"}
                            >
                              <div className="d2-item-card__top">
                                <div className="d2-item-card__badges">
                                  <span className="d2-meta-chip d2-meta-chip--gen">rev {rev}</span>
                                  {note.tags.map((tag) => (
                                    <span key={tag} className="d2-meta-chip">
                                      #{tag}
                                    </span>
                                  ))}
                                  {isUntrusted && (
                                    <span
                                      className="d2-pill d2-pill--warn"
                                      data-testid={`memory-untrusted-badge-${note.note_id}`}
                                    >
                                      Недоверенный импорт · инструкции отключены
                                    </span>
                                  )}
                                </div>
                                <div className="d2-item-card__meta-right">
                                  <span className="d2-meta-chip">
                                    {scopeLabelByScope.get(note.scope) ?? note.scope}
                                  </span>
                                  <span className="d2-time">{formatDateTime(note.updated_at)}</span>
                                </div>
                              </div>

                              <h3 className="d2-item-card__title">{note.title}</h3>
                              <p className="d2-item-card__detail memory-body-text">{note.body}</p>

                              {correctingNoteId === note.note_id ? (
                                <form
                                  className="memory-inline-form"
                                  onSubmit={(e) => void handleCorrectNoteSubmit(note, e)}
                                  data-testid={`memory-note-correct-form-${note.note_id}`}
                                >
                                  <input
                                    type="text"
                                    value={correctTitle}
                                    onChange={(e) => setCorrectTitle(e.target.value)}
                                    aria-label="Новый заголовок заметки"
                                    required
                                  />
                                  <textarea
                                    rows={3}
                                    value={correctBody}
                                    onChange={(e) => setCorrectBody(e.target.value)}
                                    aria-label="Новый текст заметки"
                                    required
                                  />
                                  <input
                                    type="text"
                                    value={correctRationale}
                                    onChange={(e) => setCorrectRationale(e.target.value)}
                                    placeholder="Причина коррекции (обязательно)"
                                    aria-label="Причина коррекции"
                                    required
                                  />
                                  <div className="d2-item-card__actions">
                                    <button type="submit" className="popover-primary-btn">
                                      Сохранить ревизию {rev + 1}
                                    </button>
                                    <button
                                      type="button"
                                      className="popover-secondary-btn"
                                      onClick={() => setCorrectingNoteId(null)}
                                    >
                                      Отмена
                                    </button>
                                  </div>
                                </form>
                              ) : (
                                <div className="d2-item-card__actions">
                                  <button
                                    type="button"
                                    className="d2-link-btn"
                                    onClick={() => void handleToggleNoteHistory(note.note_id)}
                                    data-testid={`memory-note-history-btn-${note.note_id}`}
                                  >
                                    <ClockCounterClockwise size={13} aria-hidden="true" />
                                    {expandedHistoryNoteId === note.note_id
                                      ? "Скрыть историю"
                                      : "История ревизий"}
                                  </button>
                                  <button
                                    type="button"
                                    className="d2-link-btn"
                                    onClick={() => startCorrectNote(note)}
                                    data-testid={`memory-note-correct-btn-${note.note_id}`}
                                  >
                                    <PencilSimple size={13} aria-hidden="true" />
                                    Исправить
                                  </button>
                                  <button
                                    type="button"
                                    className="d2-danger-btn d2-danger-btn--sm"
                                    onClick={() =>
                                      setForgetTarget({
                                        targetType: "note",
                                        targetId: note.note_id,
                                        scope: note.scope,
                                        label: note.title,
                                      })
                                    }
                                    data-testid={`memory-note-forget-btn-${note.note_id}`}
                                  >
                                    <Trash size={13} aria-hidden="true" />
                                    Забыть
                                  </button>
                                </div>
                              )}

                              {expandedHistoryNoteId === note.note_id && history && (
                                <div
                                  className="memory-history-panel"
                                  data-testid={`memory-note-history-${note.note_id}`}
                                >
                                  <strong>История ревизий ({history.revisions.length})</strong>
                                  {history.revisions.map((r) => (
                                    <div key={r.revision_id} className="memory-history-row">
                                      <span className="d2-meta-chip">rev {r.revision}</span>
                                      <div className="memory-history-row__copy">
                                        <strong>{r.title}</strong>
                                        <p>{r.body}</p>
                                        {r.rationale && <small>Причина: {r.rationale}</small>}
                                      </div>
                                      <span className="d2-time">{formatDateTime(r.created_at)}</span>
                                    </div>
                                  ))}
                                </div>
                              )}
                            </article>
                          );
                        })}
                      </div>
                    )}
                  </section>
                </>
              )}

              {/* TAB 2: Conflicts */}
              {activeTab === "conflicts" && (
                <section className="d2-section" data-testid="memory-conflicts-section">
                  <div className="d2-section__head">
                    <h2>
                      Открытые конфликты <span>{openConflicts.length}</span>
                    </h2>
                  </div>

                  {openConflicts.length === 0 ? (
                    <div className="d2-empty-box" data-testid="memory-conflicts-empty">
                      <GitDiff size={22} aria-hidden="true" />
                      <p>Открытых противоречий между утверждениями нет.</p>
                    </div>
                  ) : (
                    <div className="d2-card-list">
                      {openConflicts.map((conflict) => {
                        const selectedWinner =
                          selectedWinnerByConflict[conflict.conflict_id] ??
                          conflict.candidate_claim_ids[0] ??
                          "";
                        const noteDraft = resolutionNoteByConflict[conflict.conflict_id] ?? "";
                        const isBusy = resolvingConflictId === conflict.conflict_id;

                        return (
                          <article
                            key={conflict.conflict_id}
                            className="d2-item-card d2-item-card--unknown"
                            data-testid={`memory-conflict-${conflict.conflict_id}`}
                          >
                            <div className="d2-item-card__top">
                              <div className="d2-item-card__badges">
                                <span className="d2-pill d2-pill--unknown">
                                  Конфликт · {conflictTypeLabel(conflict.conflict_type)}
                                </span>
                                {conflict.blocking && (
                                  <span className="d2-pill d2-pill--status-failed">
                                    Блокирующий
                                  </span>
                                )}
                                {conflict.topic_key && (
                                  <span className="d2-meta-chip">
                                    <code>{conflict.topic_key}</code>
                                  </span>
                                )}
                              </div>
                              <span className="d2-time">
                                {formatDateTime(conflict.detected_at)}
                              </span>
                            </div>

                            <div className="memory-conflict-candidates">
                              {conflict.candidate_claim_ids.map((cid) => {
                                const claim = claimById.get(cid);
                                const isPicked = selectedWinner === cid;
                                return (
                                  <div
                                    key={cid}
                                    className={`memory-conflict-candidate ${isPicked ? "is-selected" : ""}`}
                                  >
                                    <button
                                      type="button"
                                      className="memory-conflict-candidate__pick"
                                      onClick={() =>
                                        setSelectedWinnerByConflict((prev) => ({
                                          ...prev,
                                          [conflict.conflict_id]: cid,
                                        }))
                                      }
                                      data-testid={`conflict-pick-claim-${cid}`}
                                      aria-pressed={isPicked}
                                    >
                                      <span className="memory-radio-dot" aria-hidden="true">
                                        {isPicked && <Check size={11} />}
                                      </span>
                                      <div className="memory-conflict-candidate__copy">
                                        <strong>
                                          {claim?.natural_language ?? `Утверждение ${cid.slice(0, 8)}`}
                                        </strong>
                                        {claim && (
                                          <small>
                                            Статус: {epistemicStatusLabel(claim.epistemic_status)} ·{" "}
                                            {formatDateTime(claim.created_at)}
                                          </small>
                                        )}
                                      </div>
                                    </button>
                                  </div>
                                );
                              })}
                            </div>

                            <div className="d2-inline-reply">
                              <input
                                type="text"
                                value={noteDraft}
                                onChange={(e) =>
                                  setResolutionNoteByConflict((prev) => ({
                                    ...prev,
                                    [conflict.conflict_id]: e.target.value,
                                  }))
                                }
                                placeholder="Комментарий к решению (необязательно)…"
                                aria-label="Комментарий к разрешению конфликта"
                                data-testid={`conflict-note-input-${conflict.conflict_id}`}
                              />
                              <button
                                type="button"
                                className="popover-primary-btn"
                                disabled={isBusy}
                                onClick={() => void handleResolveConflict(conflict)}
                                data-testid={`conflict-resolve-btn-${conflict.conflict_id}`}
                              >
                                {isBusy ? "Сохранение…" : "Разрешить конфликт"}
                              </button>
                            </div>
                          </article>
                        );
                      })}
                    </div>
                  )}

                  {resolvedConflicts.length > 0 && (
                    <div className="d2-section" data-testid="memory-resolved-conflicts-section">
                      <div className="d2-section__head">
                        <h2>
                          Разрешённые <span>{resolvedConflicts.length}</span>
                        </h2>
                      </div>
                      <div className="d2-card-list">
                        {resolvedConflicts.map((c) => (
                          <div
                            key={c.conflict_id}
                            className="d2-grant-row"
                            data-testid={`memory-conflict-resolved-${c.conflict_id}`}
                          >
                            <div className="d2-grant-row__info">
                              <strong>
                                {c.topic_key ?? conflictTypeLabel(c.conflict_type)} — разрешён
                              </strong>
                              <small>{c.resolution_note ?? "Решение зафиксировано"}</small>
                            </div>
                            <span className="d2-pill d2-pill--status-succeeded">Разрешён</span>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}
                </section>
              )}

              {/* TAB 3: Dossiers */}
              {activeTab === "dossiers" && (
                <section className="d2-section" data-testid="memory-dossiers-section">
                  <div className="memory-create-bar">
                    <button
                      type="button"
                      className="popover-secondary-btn"
                      onClick={() => setShowCreateDoc((v) => !v)}
                      data-testid="memory-toggle-create-doc"
                    >
                      <Plus size={14} aria-hidden="true" />
                      Новое досье
                    </button>
                  </div>

                  {showCreateDoc && (
                    <form
                      className="d2-item-card memory-inline-form"
                      onSubmit={(e) => void handleCreateDocument(e)}
                      data-testid="memory-create-doc-form"
                    >
                      <strong>Новое досье</strong>
                      <input
                        type="text"
                        value={docTitle}
                        onChange={(e) => setDocTitle(e.target.value)}
                        placeholder="Название досье"
                        aria-label="Название досье"
                        required
                      />
                      <input
                        type="text"
                        value={docKey}
                        onChange={(e) => setDocKey(e.target.value)}
                        placeholder="Ключ досье (например dossier:architecture, необязательно)"
                        aria-label="Ключ досье"
                      />
                      <textarea
                        rows={4}
                        value={docBody}
                        onChange={(e) => setDocBody(e.target.value)}
                        placeholder="Markdown-содержимое досье…"
                        aria-label="Содержимое досье"
                        required
                      />
                      <div className="d2-item-card__actions">
                        <button type="submit" className="popover-primary-btn">
                          Создать досье
                        </button>
                        <button
                          type="button"
                          className="popover-secondary-btn"
                          onClick={() => setShowCreateDoc(false)}
                        >
                          Отмена
                        </button>
                      </div>
                    </form>
                  )}

                  {visibleDocuments.length === 0 ? (
                    <div className="d2-empty-box" data-testid="memory-dossiers-empty">
                      <BookOpenText size={22} aria-hidden="true" />
                      <p>В выбранной области пока нет досье.</p>
                    </div>
                  ) : (
                    <div className="memory-dossiers-layout">
                      <div className="memory-dossiers-list" role="listbox" aria-label="Список досье">
                        {visibleDocuments.map((doc) => {
                          const isSelected = selectedDoc?.document_id === doc.document_id;
                          return (
                            <button
                              key={doc.document_id}
                              type="button"
                              role="option"
                              aria-selected={isSelected}
                              className={`memory-dossier-item ${isSelected ? "is-selected" : ""}`}
                              onClick={() => {
                                setSelectedDocId(doc.document_id);
                                setViewedDocVersion(doc.current_version);
                                setEditingDoc(false);
                              }}
                              data-testid={`dossier-list-item-${doc.document_id}`}
                            >
                              <strong>{doc.title}</strong>
                              <small>
                                v{doc.current_version} · {doc.document_key}
                              </small>
                            </button>
                          );
                        })}
                      </div>

                      {selectedDoc && (
                        <article
                          className="d2-item-card memory-dossier-viewer"
                          data-testid={`dossier-viewer-${selectedDoc.document_id}`}
                        >
                          <div className="d2-item-card__top">
                            <div>
                              <h3 className="d2-item-card__title">{selectedDoc.title}</h3>
                              <small className="d2-time">
                                {selectedDoc.document_key} ·{" "}
                                {scopeLabelByScope.get(selectedDoc.scope) ?? selectedDoc.scope}
                              </small>
                            </div>

                            <div className="d2-item-card__meta-right">
                              <button
                                ref={versionTriggerRef}
                                type="button"
                                className="project-scope-filter__trigger"
                                onClick={() => setVersionMenuOpen((v) => !v)}
                                aria-expanded={versionMenuOpen}
                                aria-haspopup="menu"
                                data-testid="dossier-version-trigger"
                              >
                                <span>
                                  Версия v{viewedDocVersion ?? selectedDoc.current_version}
                                </span>
                                <CaretDown size={11} aria-hidden="true" />
                              </button>

                              <PortalPopover
                                isOpen={versionMenuOpen}
                                anchorEl={versionTriggerRef.current}
                                onClose={() => setVersionMenuOpen(false)}
                                placement="bottom-end"
                                className="project-scope-filter__menu"
                                role="menu"
                                ariaLabel="Выбор версии досье"
                                testId="dossier-version-menu"
                              >
                                {(docVersions.length > 0
                                  ? docVersions
                                  : [
                                      {
                                        version_id: selectedDoc.document_id,
                                        document_id: selectedDoc.document_id,
                                        version: selectedDoc.current_version,
                                        body: selectedDoc.current_body,
                                        diff_summary: "Текущая версия",
                                        event_id: selectedDoc.source_event_id ?? "",
                                        created_at: selectedDoc.updated_at,
                                      },
                                    ]
                                )
                                  .slice()
                                  .sort((a, b) => b.version - a.version)
                                  .map((ver) => {
                                    const active =
                                      (viewedDocVersion ?? selectedDoc.current_version) ===
                                      ver.version;
                                    return (
                                      <button
                                        key={ver.version_id}
                                        type="button"
                                        role="menuitemradio"
                                        aria-checked={active}
                                        className={active ? "is-selected" : ""}
                                        data-testid={`dossier-version-option-${ver.version}`}
                                        onClick={() => {
                                          setViewedDocVersion(ver.version);
                                          setVersionMenuOpen(false);
                                          setEditingDoc(false);
                                        }}
                                      >
                                        <span>
                                          v{ver.version}
                                          {ver.diff_summary ? ` · ${ver.diff_summary}` : ""}
                                        </span>
                                        {active && <Check size={13} aria-hidden="true" />}
                                      </button>
                                    );
                                  })}
                              </PortalPopover>

                              {!editingDoc && (
                                <button
                                  type="button"
                                  className="d2-link-btn"
                                  onClick={() => startEditDoc(selectedDoc)}
                                  data-testid="dossier-edit-btn"
                                >
                                  <PencilSimple size={13} aria-hidden="true" />
                                  Редактировать
                                </button>
                              )}

                              <button
                                type="button"
                                className="d2-danger-btn d2-danger-btn--sm"
                                onClick={() =>
                                  setForgetTarget({
                                    targetType: "document",
                                    targetId: selectedDoc.document_id,
                                    scope: selectedDoc.scope,
                                    label: selectedDoc.title,
                                  })
                                }
                                data-testid={`dossier-forget-btn-${selectedDoc.document_id}`}
                              >
                                <Trash size={13} aria-hidden="true" />
                                Забыть
                              </button>
                            </div>
                          </div>

                          {selectedDoc.trust_level === "external_untrusted" && (
                            <span className="d2-pill d2-pill--warn">
                              Недоверенный импорт · инструкции отключены
                            </span>
                          )}

                          {editingDoc ? (
                            <form
                              className="memory-inline-form"
                              onSubmit={(e) => void handleUpdateDocument(selectedDoc, e)}
                              data-testid="dossier-edit-form"
                            >
                              <input
                                type="text"
                                value={editDocTitle}
                                onChange={(e) => setEditDocTitle(e.target.value)}
                                aria-label="Редактировать название досье"
                                required
                              />
                              <textarea
                                rows={7}
                                value={editDocBody}
                                onChange={(e) => setEditDocBody(e.target.value)}
                                aria-label="Редактировать содержимое досье"
                                required
                              />
                              <input
                                type="text"
                                value={editDocSummary}
                                onChange={(e) => setEditDocSummary(e.target.value)}
                                placeholder="Краткое описание изменений (необязательно)"
                                aria-label="Описание изменений досье"
                              />
                              <div className="d2-item-card__actions">
                                <button
                                  type="submit"
                                  className="popover-primary-btn"
                                  data-testid="dossier-save-btn"
                                >
                                  Сохранить версию v{selectedDoc.current_version + 1}
                                </button>
                                <button
                                  type="button"
                                  className="popover-secondary-btn"
                                  onClick={() => setEditingDoc(false)}
                                >
                                  Отмена
                                </button>
                              </div>
                            </form>
                          ) : (
                            <div
                              className="artifact-preview__markdown memory-dossier-markdown"
                              data-testid="dossier-markdown-body"
                            >
                              <ReactMarkdown remarkPlugins={[remarkGfm]} skipHtml>
                                {activeDocVersionObj?.body ?? selectedDoc.current_body}
                              </ReactMarkdown>
                            </div>
                          )}

                          {((selectedDoc.provenance_claim_ids?.length ?? 0) > 0 ||
                            (selectedDoc.provenance_note_ids?.length ?? 0) > 0) && (
                            <div
                              className="memory-provenance-row"
                              data-testid="dossier-provenance-chips"
                            >
                              <span className="d2-time">Источники:</span>
                              {(selectedDoc.provenance_claim_ids ?? []).map((cid) => (
                                <button
                                  key={cid}
                                  type="button"
                                  className="d2-link-btn"
                                  onClick={() => jumpToProvenance(cid)}
                                  data-testid={`dossier-prov-claim-${cid}`}
                                >
                                  claim:{cid.slice(0, 8)}
                                </button>
                              ))}
                              {(selectedDoc.provenance_note_ids ?? []).map((nid) => (
                                <button
                                  key={nid}
                                  type="button"
                                  className="d2-link-btn"
                                  onClick={() => jumpToProvenance(nid)}
                                  data-testid={`dossier-prov-note-${nid}`}
                                >
                                  note:{nid.slice(0, 8)}
                                </button>
                              ))}
                            </div>
                          )}
                        </article>
                      )}
                    </div>
                  )}
                </section>
              )}

              {/* Recent content-free forget tombstones */}
              {visibleTombstones.length > 0 && (
                <section className="d2-section" data-testid="memory-tombstones-section">
                  <div className="d2-section__head">
                    <h2>
                      Журнал забывания (Tombstones) <span>{visibleTombstones.length}</span>
                    </h2>
                  </div>
                  <div className="d2-grant-list">
                    {visibleTombstones.slice(0, 5).map((t) => (
                      <div
                        key={t.tombstone_id}
                        className="d2-grant-row"
                        data-testid={`memory-tombstone-${t.tombstone_id}`}
                      >
                        <div className="d2-grant-row__info">
                          <strong>
                            {targetTypeLabel(t.target_type)} {t.target_id.slice(0, 8)} ·{" "}
                            {t.reason}
                          </strong>
                          <small>{t.residual_backup_notice}</small>
                        </div>
                        <span className="d2-meta-chip">{t.propagation_status}</span>
                      </div>
                    ))}
                  </div>
                </section>
              )}
            </>
          )}
        </div>
      </div>
    </main>
  );
}
