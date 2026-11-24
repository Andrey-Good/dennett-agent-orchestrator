import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
} from "react";
import {
  ArrowsClockwise,
  CaretDown,
  Check,
  ClockCounterClockwise,
  Cpu,
  Database,
  FolderPlus,
  Key,
  MagnifyingGlass,
  PlugsConnected,
  PuzzlePiece,
  ShieldCheck,
  ShieldWarning,
  Trash,
  WarningCircle,
  X,
} from "@phosphor-icons/react";

import type {
  CapabilityKind,
  CapabilityRecord,
  CapabilityScopeKind,
  CapabilityTrust,
  CompatibilityStatus,
  PlannedProviderInfo,
  ProviderHealthStatus,
  RuntimeModelOption,
} from "../api/contracts";
import { useWorkspace } from "../state/WorkspaceProvider";
import { formatDecisionReasonRu } from "../utils/formatters";
import { PortalPopover } from "./PortalPopover";
import { useModalFocusTrap } from "./useModalFocusTrap";

type SettingsTab = "providers" | "capabilities";
type CapabilityFilter = "all" | CapabilityScopeKind;

const LOCAL_PRESETS = [
  { id: "ollama", label: "Ollama (11434)", url: "http://127.0.0.1:11434" },
  { id: "lmstudio", label: "LM Studio (1234)", url: "http://127.0.0.1:1234" },
  { id: "llamacpp", label: "llama.cpp (8080)", url: "http://127.0.0.1:8080" },
] as const;

const FALLBACK_PLANNED_PROVIDERS: PlannedProviderInfo[] = [
  {
    id: "r-claude",
    label: "Claude Code (R-CLAUDE)",
    status: "planned",
    note: "Запланировано за пределами этапа D3 — подключение пока недоступно.",
  },
  {
    id: "r-codex",
    label: "Codex (R-CODEX)",
    status: "planned",
    note: "Запланировано за пределами этапа D3 — подключение пока недоступно.",
  },
];

function providerHealthInfo(status: ProviderHealthStatus): {
  label: string;
  tone: "ok" | "warn" | "danger" | "neutral";
} {
  switch (status) {
    case "healthy":
      return { label: "Доступен", tone: "ok" };
    case "auth_required":
      return { label: "Требуется вход", tone: "warn" };
    case "quota_limited":
      return { label: "Лимит квоты", tone: "warn" };
    case "unreachable":
      return { label: "Недоступен", tone: "danger" };
    case "incompatible":
      return { label: "Несовместим", tone: "danger" };
    case "unknown":
      return { label: "Не проверен", tone: "neutral" };
  }
}

function capabilityKindLabel(kind: CapabilityKind): string {
  switch (kind) {
    case "skill":
      return "Навык";
    case "plugin":
      return "Плагин";
    case "mcp_server":
      return "MCP";
  }
}

function capabilityScopeLabel(scope: CapabilityScopeKind): string {
  switch (scope) {
    case "global":
      return "Глобально";
    case "project":
      return "Проект";
    case "candidate_unattached":
      return "Кандидат";
  }
}

function compatibilityInfo(status: CompatibilityStatus): {
  label: string;
  className: string;
} {
  switch (status) {
    case "compatible":
      return { label: "Совместим", className: "d2-pill--status-succeeded" };
    case "warning":
      return { label: "Предупреждение", className: "d2-pill--warn" };
    case "incompatible":
      return { label: "Несовместим", className: "d2-pill--status-failed" };
  }
}

function trustLabel(trust: CapabilityTrust): string {
  switch (trust) {
    case "trusted":
      return "Доверенный";
    case "restricted":
      return "Ограниченный";
    case "unreviewed":
      return "Не проверен";
    case "revoked":
      return "Отозван";
  }
}

function bumpPatchVersion(version: string): string {
  const parts = version.trim().split(".");
  if (parts.length === 3) {
    const patch = Number.parseInt(parts[2] ?? "0", 10);
    if (Number.isFinite(patch)) {
      return `${parts[0]}.${parts[1]}.${patch + 1}`;
    }
  }
  return `${version.trim() || "1.0.0"}.1`;
}

export function SettingsSheet() {
  const { state, actions, meta } = useWorkspace();
  const [activeTab, setActiveTab] = useState<SettingsTab>("providers");
  const [urlDraft, setUrlDraft] = useState(
    state.healthSnapshot?.head_url ?? "http://127.0.0.1:41771",
  );

  // Write-only SecretBroker state for Gemini API key (never prefilled!)
  const [geminiKeyDraft, setGeminiKeyDraft] = useState("");
  const [isSavingGeminiKey, setIsSavingGeminiKey] = useState(false);

  // Local HTTP server state
  const [localUrlDraft, setLocalUrlDraft] = useState("http://127.0.0.1:11434");
  const [localNameDraft, setLocalNameDraft] = useState("Локальный сервер");
  const [localTokenDraft, setLocalTokenDraft] = useState("");
  const [isProbingLocal, setIsProbingLocal] = useState(false);
  const [localProbeFeedback, setLocalProbeFeedback] = useState<string | null>(null);
  const [probedModelsByConn, setProbedModelsByConn] = useState<
    Record<string, RuntimeModelOption[]>
  >({});

  // Custom PortalPopover selectors (zero <select>!)
  const [connMenuOpen, setConnMenuOpen] = useState(false);
  const connTriggerRef = useRef<HTMLButtonElement | null>(null);
  const [modelMenuOpen, setModelMenuOpen] = useState(false);
  const modelTriggerRef = useRef<HTMLButtonElement | null>(null);

  // Capabilities tab state
  const [capFilter, setCapFilter] = useState<CapabilityFilter>("all");
  const [isDiscovering, setIsDiscovering] = useState(false);
  const [discoveryFeedback, setDiscoveryFeedback] = useState<string | null>(null);
  const [showMcpForm, setShowMcpForm] = useState(false);
  const [mcpName, setMcpName] = useState("");
  const [mcpCommand, setMcpCommand] = useState("");
  const [mcpArgs, setMcpArgs] = useState("");
  const [mcpSecretSlot, setMcpSecretSlot] = useState("");
  const [mcpSecretValue, setMcpSecretValue] = useState("");
  const [editingCapId, setEditingCapId] = useState<string | null>(null);
  const [editCapVersion, setEditCapVersion] = useState("");
  const [editCapDesc, setEditCapDesc] = useState("");

  const dialogRef = useRef<HTMLDivElement | null>(null);

  const closeSettings = useCallback(() => {
    setConnMenuOpen(false);
    setModelMenuOpen(false);
    actions.setSettingsOpen(false);
  }, [actions]);

  useModalFocusTrap(dialogRef, state.isSettingsOpen, closeSettings);

  useEffect(() => {
    if (state.healthSnapshot?.head_url) {
      setUrlDraft(state.healthSnapshot.head_url);
    }
  }, [state.healthSnapshot?.head_url]);

  const existingLocalConn = useMemo(
    () => state.providerConnections.find((c) => c.provider_kind === "local_http") ?? null,
    [state.providerConnections],
  );

  useEffect(() => {
    if (existingLocalConn?.endpoint_url) {
      setLocalUrlDraft(existingLocalConn.endpoint_url);
      setLocalNameDraft(existingLocalConn.display_name);
    }
  }, [existingLocalConn?.connection_id, existingLocalConn?.display_name, existingLocalConn?.endpoint_url]);

  const projectNames = useMemo(() => {
    const map = new Map<string, string>();
    for (const p of state.projects) {
      map.set(p.project_id, p.name);
    }
    return map;
  }, [state.projects]);

  const secretBySlot = useMemo(() => {
    const map = new Map<string, (typeof state.secrets)[number]>();
    for (const s of state.secrets) {
      map.set(s.slot_key, s);
    }
    return map;
  }, [state.secrets]);

  const filteredCapabilities = useMemo(() => {
    const activeProjectId = meta.activeProject?.project_id ?? null;
    return state.capabilities.filter((cap) => {
      if (capFilter === "all") {
        return true;
      }
      if (capFilter === "project") {
        return (
          cap.scope_kind === "project" &&
          (!activeProjectId || cap.project_id === activeProjectId)
        );
      }
      return cap.scope_kind === capFilter;
    });
  }, [capFilter, meta.activeProject?.project_id, state.capabilities]);

  if (!state.isSettingsOpen) {
    return null;
  }

  const connected = Boolean(state.healthSnapshot?.connected);
  const health = state.healthSnapshot?.health;
  const catalog = state.runtimeCatalog;
  const isSimulated = Boolean(health?.runtime_is_simulated ?? catalog?.is_simulated);
  const authLabel = isSimulated
    ? "Simulated CI Runtime"
    : catalog?.auth_profile === "local-endpoint"
      ? "Локальный OpenAI-совместимый сервер"
      : catalog?.auth_profile === "gemini-api-key"
        ? "Gemini API key · headless"
        : `${catalog?.tier ?? "Google AI Ultra"} · oauth-personal`;
  const engineLabel = isSimulated
    ? "SimulatedRuntime"
    : catalog?.auth_profile === "local-endpoint"
      ? catalog.adapter_id || "local-openai-http"
      : `Google Antigravity · agy acp ${catalog?.agy_version ?? "1.5+"}`;

  const geminiSecret = secretBySlot.get("gemini_api_key") ?? null;
  const localTokenSecret = secretBySlot.get("local_runtime_token") ?? null;

  const activeConnection =
    state.providerConnections.find(
      (c) => c.connection_id === meta.activeAgent?.current_config.preferred_connection_id,
    ) ??
    state.providerConnections.find((c) => c.is_default_interactive) ??
    state.providerConnections[0] ??
    null;

  const selectableModels: RuntimeModelOption[] = (() => {
    if (activeConnection && probedModelsByConn[activeConnection.connection_id]?.length) {
      return probedModelsByConn[activeConnection.connection_id]!;
    }
    return catalog?.models ?? [];
  })();

  const currentModelId =
    meta.activeAgent?.current_config.model_preference ??
    catalog?.default_model_id ??
    selectableModels[0]?.model_id ??
    null;

  const currentModelLabel =
    selectableModels.find((m) => m.model_id === currentModelId)?.label ??
    currentModelId ??
    "По умолчанию движка";

  const plannedProviders =
    state.plannedProviders.length > 0 ? state.plannedProviders : FALLBACK_PLANNED_PROVIDERS;

  const activeProjectIds = new Set(
    state.projects
      .filter((p) => (p.lifecycle_state ?? "active") !== "deleted_record")
      .map((p) => p.project_id),
  );
  const activeGrants = state.activeGrants.filter(
    (g) => !g.revoked_at && activeProjectIds.has(g.project_id),
  );

  const handleReconnect = (event: FormEvent) => {
    event.preventDefault();
    const cleaned = urlDraft.trim();
    if (!cleaned) {
      return;
    }
    void actions.reconnectWithUrl(cleaned);
  };

  const handleSaveGeminiKey = async (event: FormEvent) => {
    event.preventDefault();
    const secretVal = geminiKeyDraft.trim();
    if (!secretVal) {
      return;
    }
    setIsSavingGeminiKey(true);
    setGeminiKeyDraft("");
    try {
      await actions.setSecret(
        "gemini_api_key",
        secretVal,
        "Фоновый Gemini API ключ для headless-задач Antigravity",
      );
    } finally {
      setIsSavingGeminiKey(false);
    }
  };

  const handleDeleteGeminiKey = async () => {
    setIsSavingGeminiKey(true);
    setGeminiKeyDraft("");
    try {
      await actions.deleteSecret("gemini_api_key");
    } finally {
      setIsSavingGeminiKey(false);
    }
  };

  const handleProbeLocalServer = async (event: FormEvent) => {
    event.preventDefault();
    const endpoint = localUrlDraft.trim();
    if (!endpoint) {
      return;
    }
    setIsProbingLocal(true);
    setLocalProbeFeedback(null);

    const tokenVal = localTokenDraft.trim();
    setLocalTokenDraft("");
    try {
      let secretSlotKey: string | null = existingLocalConn?.secret_slot_key ?? null;
      if (tokenVal) {
        await actions.setSecret(
          "local_runtime_token",
          tokenVal,
          "Write-only Bearer токен локального OpenAI-совместимого сервера",
        );
        secretSlotKey = "local_runtime_token";
      } else if (localTokenSecret?.configured) {
        secretSlotKey = "local_runtime_token";
      }

      let targetId = existingLocalConn?.connection_id ?? null;
      if (targetId) {
        await actions.updateProviderConnection(targetId, {
          provider_kind: "local_http",
          display_name: localNameDraft.trim() || "Локальный сервер",
          adapter_id: "local-openai-http",
          auth_profile: "local-endpoint",
          endpoint_url: endpoint,
          secret_slot_key: secretSlotKey,
          enabled: true,
        });
      } else {
        const created = await actions.createProviderConnection({
          provider_kind: "local_http",
          display_name: localNameDraft.trim() || "Локальный сервер",
          adapter_id: "local-openai-http",
          auth_profile: "local-endpoint",
          endpoint_url: endpoint,
          secret_slot_key: secretSlotKey,
          enabled: true,
        });
        targetId = created?.connection_id ?? null;
      }

      if (!targetId) {
        return;
      }

      const probeResp = await actions.probeProviderConnection(targetId);
      if (probeResp) {
        const models = probeResp.catalog?.models ?? [];
        setProbedModelsByConn((prev) => ({
          ...prev,
          [targetId!]: models,
        }));
        const statusInfo = providerHealthInfo(probeResp.connection.last_health_status);
        setLocalProbeFeedback(
          `${statusInfo.label}: найдено моделей — ${models.length}${
            probeResp.connection.last_health_detail
              ? ` (${probeResp.connection.last_health_detail})`
              : ""
          }`,
        );
      }
    } finally {
      setIsProbingLocal(false);
    }
  };

  const handleSelectConnection = async (connectionId: string) => {
    setConnMenuOpen(false);
    await actions.activateProviderConnection(connectionId, "interactive");
    const probed = probedModelsByConn[connectionId];
    const nextModel = probed?.[0]?.model_id ?? null;
    if (meta.activeAgent) {
      await actions.updateActiveAgentConnection(
        connectionId,
        nextModel,
        meta.activeAgent.current_config.thinking_level ?? null,
      );
    }
  };

  const handleSelectModel = async (modelId: string) => {
    setModelMenuOpen(false);
    if (!meta.activeAgent) {
      return;
    }
    await actions.updateActiveAgentConnection(
      activeConnection?.connection_id ??
        meta.activeAgent.current_config.preferred_connection_id ??
        null,
      modelId,
      meta.activeAgent.current_config.thinking_level ?? null,
    );
  };

  const handleDiscover = async (scanPersonalGeminiReadonly: boolean) => {
    setIsDiscovering(true);
    setDiscoveryFeedback(null);
    try {
      const resp = await actions.discoverCapabilities(
        meta.activeProject?.project_id ?? null,
        scanPersonalGeminiReadonly,
      );
      if (resp) {
        setDiscoveryFeedback(
          `${
            scanPersonalGeminiReadonly
              ? "Сканирование ~/.gemini/ (только чтение)"
              : "Сканирование проекта"
          }: найдено ${resp.discovered.length}, новых ${resp.newly_registered}, обновлено ${resp.updated_existing}.${
            resp.warnings && resp.warnings.length > 0 ? ` ${resp.warnings.join(" ")}` : ""
          }`,
        );
      }
    } finally {
      setIsDiscovering(false);
    }
  };

  const handleAddLocalFolderCapability = (kind: "skill" | "plugin") => {
    void actions.requestFolderPicker({
      mode: "add-capability",
      kind,
      scopeKind: meta.activeProject ? "project" : "global",
      projectId: meta.activeProject?.project_id ?? null,
    });
  };

  const handleCreateMcp = async (event: FormEvent) => {
    event.preventDefault();
    if (!mcpName.trim() || !mcpCommand.trim()) {
      return;
    }
    const slug =
      mcpName
        .trim()
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, "-")
        .replace(/^-+|-+$/g, "") || `mcp-${Date.now().toString(16)}`;
    const args = mcpArgs
      .trim()
      .split(/\s+/)
      .filter(Boolean);
    const slotKey = mcpSecretSlot.trim();
    const secretVal = mcpSecretValue.trim();
    setMcpSecretValue("");

    const secretSlots: Record<string, string> = {};
    if (slotKey) {
      if (secretVal) {
        await actions.setSecret(slotKey, secretVal, `Секрет MCP-сервера ${mcpName.trim()}`);
      }
      secretSlots[slotKey.toUpperCase()] = slotKey;
    }

    const created = await actions.createCapability({
      slug,
      display_name: mcpName.trim(),
      description: `MCP-сервер: ${mcpCommand.trim()}`,
      version: "1.0.0",
      origin_kind: "user_manual",
      ownership: meta.activeProject ? "project" : "user",
      scope_kind: meta.activeProject ? "project" : "global",
      project_id: meta.activeProject?.project_id ?? null,
      trust_state: "restricted",
      activation_state: "active",
      spec: {
        kind: "mcp_server",
        command: mcpCommand.trim(),
        args,
        env_vars: {},
        secret_slots: secretSlots,
      },
    });
    if (created) {
      setMcpName("");
      setMcpCommand("");
      setMcpArgs("");
      setMcpSecretSlot("");
      setShowMcpForm(false);
    }
  };

  const handleToggleCapability = async (cap: CapabilityRecord) => {
    const isActive = cap.activation_state === "active" || cap.activation_state === "pinned";
    const nextState = isActive ? "disabled" : "active";
    const promoteScope =
      !isActive && cap.scope_kind === "candidate_unattached"
        ? {
            scope_kind: (meta.activeProject ? "project" : "global") as CapabilityScopeKind,
            project_id: meta.activeProject?.project_id ?? null,
          }
        : {};
    await actions.updateCapability(cap.capability_id, {
      activation_state: nextState,
      ...promoteScope,
    });
  };

  const startEditCapability = (cap: CapabilityRecord) => {
    setEditingCapId(cap.capability_id);
    setEditCapVersion(bumpPatchVersion(cap.version));
    setEditCapDesc(cap.description);
  };

  const handleApplyCapabilityUpdate = async (cap: CapabilityRecord, event: FormEvent) => {
    event.preventDefault();
    const updated = await actions.updateCapability(cap.capability_id, {
      version: editCapVersion.trim() || bumpPatchVersion(cap.version),
      description: editCapDesc.trim() || cap.description,
    });
    if (updated) {
      setEditingCapId(null);
    }
  };

  return (
    <div
      className="dialog-backdrop"
      onMouseDown={closeSettings}
      data-testid="settings-backdrop"
    >
      <div
        ref={dialogRef}
        className="floating-dialog settings-sheet"
        role="dialog"
        aria-modal="true"
        aria-label="Настройки и подключения"
        data-testid="settings-sheet"
        tabIndex={-1}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <header className="settings-sheet__header">
          <div>
            <h2>Настройки и подключения</h2>
            <p>
              Провайдеры, write-only секреты, локальные модели, навыки, плагины, MCP и разрешения
            </p>
          </div>
          <button
            type="button"
            className="icon-button"
            onClick={closeSettings}
            aria-label="Закрыть настройки"
          >
            <X size={16} />
          </button>
        </header>

        <div className="settings-tabs-bar">
          <div className="d2-segmented" role="tablist" aria-label="Разделы настроек">
            <button
              type="button"
              role="tab"
              aria-selected={activeTab === "providers"}
              className={activeTab === "providers" ? "is-active" : ""}
              onClick={() => setActiveTab("providers")}
              data-testid="settings-tab-providers"
            >
              Провайдеры и ключи
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={activeTab === "capabilities"}
              className={activeTab === "capabilities" ? "is-active" : ""}
              onClick={() => setActiveTab("capabilities")}
              data-testid="settings-tab-capabilities"
            >
              Навыки, плагины и MCP ({state.capabilities.length})
            </button>
          </div>
          {state.isLoadingD3Settings && <span className="d2-time">Обновление…</span>}
        </div>

        <div className="settings-sheet__body">
          {activeTab === "providers" ? (
            <>
              {/* 1. Google Antigravity + SecretBroker (write-only) */}
              <section className="settings-card" data-testid="settings-antigravity-card">
                <div className="settings-card__head">
                  <PlugsConnected size={16} aria-hidden="true" />
                  <strong>Google Antigravity (основной runtime)</strong>
                </div>

                <div className="stats-kv">
                  <span>Активный адаптер</span>
                  <strong>{engineLabel}</strong>
                </div>
                <div className="stats-kv">
                  <span>Активный профиль</span>
                  <strong>{authLabel}</strong>
                </div>
                <div className="stats-kv">
                  <span>Доступно моделей</span>
                  <strong>{catalog ? catalog.models.length : "—"}</strong>
                </div>

                <div className="settings-profiles-grid">
                  <div className="settings-profile-tile">
                    <strong>Google AI Ultra · oauth-personal</strong>
                    <small>
                      Интерактивные сессии через системный keyring CLI <code>agy</code> (секреты не
                      хранятся в UI).
                    </small>
                  </div>
                  <div className="settings-profile-tile">
                    <strong>Gemini API key · headless</strong>
                    <small>
                      Фоновые задачи и расписание. Ключ передаётся через write-only Secret Broker.
                    </small>
                  </div>
                </div>

                <div className="settings-secret-box" data-testid="settings-secret-gemini">
                  <div className="settings-secret-box__head">
                    <div className="settings-card__head">
                      <Key size={14} aria-hidden="true" />
                      <strong>Secret Broker · gemini_api_key</strong>
                    </div>
                    {geminiSecret?.configured ? (
                      <span
                        className="d2-pill d2-pill--status-succeeded"
                        data-testid="secret-status-gemini_api_key"
                      >
                        Задан · v{geminiSecret.version}
                        {geminiSecret.fingerprint_short
                          ? ` · ${geminiSecret.fingerprint_short}`
                          : ""}
                      </span>
                    ) : (
                      <span
                        className="d2-meta-chip"
                        data-testid="secret-status-gemini_api_key"
                      >
                        Не задан
                      </span>
                    )}
                  </div>
                  <form
                    className="settings-secret-form"
                    onSubmit={(e) => void handleSaveGeminiKey(e)}
                  >
                    <input
                      type="password"
                      autoComplete="new-password"
                      value={geminiKeyDraft}
                      onChange={(e) => setGeminiKeyDraft(e.target.value)}
                      placeholder={
                        geminiSecret?.configured
                          ? "Введите новый ключ для замены (write-only)…"
                          : "Введите Gemini API key (write-only)…"
                      }
                      aria-label="Gemini API ключ (только запись)"
                      data-testid="secret-input-gemini_api_key"
                      disabled={!connected || isSavingGeminiKey}
                    />
                    <button
                      type="submit"
                      className="popover-primary-btn"
                      disabled={!connected || isSavingGeminiKey || !geminiKeyDraft.trim()}
                      data-testid="secret-save-gemini_api_key"
                    >
                      {geminiSecret?.configured ? "Заменить" : "Сохранить ключ"}
                    </button>
                    {geminiSecret?.configured && (
                      <button
                        type="button"
                        className="d2-danger-btn"
                        disabled={!connected || isSavingGeminiKey}
                        onClick={() => void handleDeleteGeminiKey()}
                        data-testid="secret-delete-gemini_api_key"
                      >
                        Удалить
                      </button>
                    )}
                  </form>
                </div>

                <div className="settings-actions">
                  <button
                    type="button"
                    className="popover-primary-btn"
                    onClick={() => void actions.refreshRuntimeCatalog(true)}
                    disabled={state.isRefreshingCatalog || !connected}
                    data-testid="refresh-catalog-button"
                  >
                    <ArrowsClockwise
                      size={14}
                      className={state.isRefreshingCatalog ? "spin" : ""}
                      aria-hidden="true"
                    />
                    {state.isRefreshingCatalog
                      ? "Опрос движка…"
                      : "Обновить каталог"}
                  </button>
                </div>
              </section>

              {/* 2. Local OpenAI-compatible HTTP server + Active connection/model PortalPopover */}
              <section className="settings-card" data-testid="settings-local-server-card">
                <div className="settings-card__head">
                  <Cpu size={16} aria-hidden="true" />
                  <strong>Локальный сервер (Ollama / LM Studio / llama.cpp)</strong>
                </div>

                <div className="settings-presets-row">
                  {LOCAL_PRESETS.map((preset) => (
                    <button
                      key={preset.id}
                      type="button"
                      className={`d2-link-btn ${localUrlDraft === preset.url ? "is-selected" : ""}`}
                      onClick={() => {
                        setLocalUrlDraft(preset.url);
                        setLocalNameDraft(preset.label.split(" (")[0] ?? "Локальный сервер");
                      }}
                      data-testid={`local-preset-${preset.id}`}
                    >
                      {preset.label}
                    </button>
                  ))}
                </div>

                <form
                  className="settings-local-form"
                  onSubmit={(e) => void handleProbeLocalServer(e)}
                >
                  <label className="settings-field">
                    <span>Адрес локального сервера (только loopback)</span>
                    <input
                      type="text"
                      value={localUrlDraft}
                      onChange={(e) => setLocalUrlDraft(e.target.value)}
                      placeholder="http://127.0.0.1:11434"
                      aria-label="URL локального сервера"
                      data-testid="local-endpoint-input"
                      disabled={!connected || isProbingLocal}
                    />
                  </label>
                  <label className="settings-field">
                    <span>
                      Опциональный Bearer-токен (write-only)
                      {localTokenSecret?.configured
                        ? ` · задан v${localTokenSecret.version} (${localTokenSecret.fingerprint_short ?? ""})`
                        : ""}
                    </span>
                    <input
                      type="password"
                      autoComplete="new-password"
                      value={localTokenDraft}
                      onChange={(e) => setLocalTokenDraft(e.target.value)}
                      placeholder="Оставьте пустым, если сервер без авторизации"
                      aria-label="Bearer токен локального сервера"
                      data-testid="local-token-input"
                      disabled={!connected || isProbingLocal}
                    />
                  </label>
                  <div className="settings-actions">
                    <button
                      type="submit"
                      className="popover-secondary-btn"
                      disabled={!connected || isProbingLocal || !localUrlDraft.trim()}
                      data-testid="local-probe-btn"
                    >
                      <ArrowsClockwise
                        size={14}
                        className={isProbingLocal ? "spin" : ""}
                        aria-hidden="true"
                      />
                      {isProbingLocal
                        ? "Проверка сервера…"
                        : "Проверить и загрузить модели"}
                    </button>
                  </div>
                </form>

                {localProbeFeedback && (
                  <p className="settings-probe-feedback" data-testid="local-probe-feedback">
                    {localProbeFeedback}
                  </p>
                )}

                {state.providerConnections.length > 0 && (
                  <div className="settings-binding-row">
                    <div className="settings-binding-col">
                      <span className="settings-binding-label">Активное подключение</span>
                      <button
                        ref={connTriggerRef}
                        type="button"
                        className="project-scope-filter__trigger settings-popover-trigger"
                        onClick={() => setConnMenuOpen((v) => !v)}
                        aria-expanded={connMenuOpen}
                        aria-haspopup="menu"
                        disabled={!connected}
                        data-testid="settings-connection-trigger"
                      >
                        <span className="project-scope-filter__label">
                          {activeConnection?.display_name ?? "Выбрать подключение"}
                        </span>
                        <CaretDown size={11} aria-hidden="true" />
                      </button>
                      <PortalPopover
                        isOpen={connMenuOpen}
                        anchorEl={connTriggerRef.current}
                        onClose={() => setConnMenuOpen(false)}
                        placement="bottom-start"
                        className="project-scope-filter__menu"
                        role="menu"
                        ariaLabel="Выбор активного подключения провайдера"
                        testId="settings-connection-menu"
                      >
                        {state.providerConnections.map((conn) => {
                          const selected = conn.connection_id === activeConnection?.connection_id;
                          const healthInfo = providerHealthInfo(conn.last_health_status);
                          return (
                            <button
                              key={conn.connection_id}
                              type="button"
                              role="menuitemradio"
                              aria-checked={selected}
                              className={selected ? "is-selected" : ""}
                              onClick={() => void handleSelectConnection(conn.connection_id)}
                              data-testid={`connection-option-${conn.connection_id}`}
                            >
                              <span className="popover-option-copy">
                                <strong>{conn.display_name}</strong>
                                <small>
                                  {conn.auth_profile} · {healthInfo.label}
                                </small>
                              </span>
                              {selected && <Check size={13} aria-hidden="true" />}
                            </button>
                          );
                        })}
                      </PortalPopover>
                    </div>

                    <div className="settings-binding-col">
                      <span className="settings-binding-label">Модель агента</span>
                      <button
                        ref={modelTriggerRef}
                        type="button"
                        className="project-scope-filter__trigger settings-popover-trigger"
                        onClick={() => setModelMenuOpen((v) => !v)}
                        aria-expanded={modelMenuOpen}
                        aria-haspopup="menu"
                        disabled={!connected || selectableModels.length === 0}
                        data-testid="settings-model-trigger"
                      >
                        <span className="project-scope-filter__label">{currentModelLabel}</span>
                        <CaretDown size={11} aria-hidden="true" />
                      </button>
                      <PortalPopover
                        isOpen={modelMenuOpen}
                        anchorEl={modelTriggerRef.current}
                        onClose={() => setModelMenuOpen(false)}
                        placement="bottom-end"
                        className="project-scope-filter__menu"
                        role="menu"
                        ariaLabel="Выбор модели для активного агента"
                        testId="settings-model-menu"
                      >
                        {selectableModels.map((model) => {
                          const selected = model.model_id === currentModelId;
                          return (
                            <button
                              key={model.model_id}
                              type="button"
                              role="menuitemradio"
                              aria-checked={selected}
                              className={selected ? "is-selected" : ""}
                              onClick={() => void handleSelectModel(model.model_id)}
                              data-testid={`settings-model-option-${model.model_id}`}
                            >
                              <span className="popover-option-copy">
                                <strong>{model.label}</strong>
                                <small>{model.model_id}</small>
                              </span>
                              {selected && <Check size={13} aria-hidden="true" />}
                            </button>
                          );
                        })}
                      </PortalPopover>
                    </div>
                  </div>
                )}

                {meta.activeAgent && (
                  <p className="settings-note" data-testid="settings-agent-identity-note">
                    Агент <strong>{meta.activeAgent.name}</strong> (
                    <code>{meta.activeAgent.agent_id}</code> · config v
                    {meta.activeAgent.current_config.version_number}): смена подключения и модели не
                    меняет личность и память агента.
                  </p>
                )}
              </section>

              {/* 3. Clear visual split: Provider Quotas vs Dennett Budgets */}
              <section
                className="settings-quota-split"
                data-testid="settings-quota-budget-split"
              >
                <div className="settings-card" data-testid="settings-provider-quotas-box">
                  <div className="settings-card__head">
                    <WarningCircle size={15} aria-hidden="true" />
                    <strong>Квоты провайдера</strong>
                  </div>
                  {catalog?.quota_error ? (
                    <div
                      className="memory-alert memory-alert--warn"
                      role="status"
                      data-testid="settings-quota-error-banner"
                    >
                      <ShieldWarning size={15} aria-hidden="true" />
                      <div className="memory-alert__body">
                        <strong>
                          Исчерпана квота ({catalog.quota_error.provider} ·{" "}
                          {catalog.quota_error.auth_profile})
                        </strong>
                        <span>{catalog.quota_error.detail}</span>
                        {catalog.quota_error.retry_after_secs != null && (
                          <small>
                            Повтор через {catalog.quota_error.retry_after_secs} с. Автоматического
                            переключения на платный API нет.
                          </small>
                        )}
                      </div>
                    </div>
                  ) : catalog && catalog.quotas.length > 0 ? (
                    catalog.quotas.map((q) => (
                      <div key={q.label} className="stats-kv">
                        <span>{q.label}</span>
                        <strong>{q.remaining_percent}</strong>
                      </div>
                    ))
                  ) : (
                    <p className="settings-note">
                      Внешние лимиты подписки и API-тарифа отображаются по данным живого провайдера.
                    </p>
                  )}
                </div>

                <div className="settings-card" data-testid="settings-dennett-budgets-box">
                  <div className="settings-card__head">
                    <ShieldCheck size={15} aria-hidden="true" />
                    <strong>Бюджеты Dennett</strong>
                  </div>
                  {meta.activeAgent?.current_config.default_budget ? (
                    <>
                      <div className="stats-kv">
                        <span>Макс. ходов</span>
                        <strong>
                          {meta.activeAgent.current_config.default_budget.max_turns ?? "—"}
                        </strong>
                      </div>
                      <div className="stats-kv">
                        <span>Лимит времени</span>
                        <strong>
                          {meta.activeAgent.current_config.default_budget.max_wall_secs
                            ? `${meta.activeAgent.current_config.default_budget.max_wall_secs} с`
                            : "—"}
                        </strong>
                      </div>
                    </>
                  ) : (
                    <p className="settings-note">
                      Внутренние лимиты Dennett на ходы, время и автономные поручения задаются
                      отдельно от квот провайдера и действуют до вызова модели.
                    </p>
                  )}
                </div>
              </section>

              {/* 4. Planned providers — strictly non-interactive static cards (ZERO login buttons!) */}
              <section
                className="settings-card settings-card--muted"
                data-testid="settings-planned-providers"
              >
                <div className="settings-card__head">
                  <PlugsConnected size={15} aria-hidden="true" />
                  <strong>Запланированные движки (за пределами D3)</strong>
                </div>
                <div className="settings-planned-list">
                  {plannedProviders.map((provider) => (
                    <div
                      key={provider.id}
                      className="settings-planned-card"
                      data-testid={`planned-provider-${provider.id}`}
                    >
                      <div className="settings-planned-card__copy">
                        <strong>{provider.label}</strong>
                        {provider.note && <small>{provider.note}</small>}
                      </div>
                      <span className="d2-meta-chip">Запланировано</span>
                    </div>
                  ))}
                </div>
              </section>

              {/* 5. Preserved D2 Grants section */}
              <section className="settings-card" data-testid="settings-grants-section">
                <div className="settings-card__head">
                  <ShieldCheck size={16} aria-hidden="true" />
                  <strong>Действующие разрешения ({activeGrants.length})</strong>
                </div>
                {activeGrants.length === 0 ? (
                  <p className="settings-note">
                    Активных постоянных разрешений записи нет. Выданные через «Разрешить для
                    проекта» права можно мгновенно отозвать здесь или во Входящих.
                  </p>
                ) : (
                  <div className="d2-grant-list">
                    {activeGrants.map((grant) => (
                      <div
                        key={grant.grant_id}
                        className="d2-grant-row"
                        data-testid={`settings-grant-row-${grant.grant_id}`}
                      >
                        <div className="d2-grant-row__info">
                          <strong>
                            {grant.tool_pattern === "workspace_write"
                              ? "Запись в папку проекта"
                              : grant.tool_pattern}
                          </strong>
                          <small>
                            {projectNames.get(grant.project_id) ?? grant.scope} · вызовов:{" "}
                            {grant.uses_consumed}
                            {grant.max_uses ? ` / ${grant.max_uses}` : ""}
                            {grant.reason ? ` · ${formatDecisionReasonRu(grant.reason)}` : ""}
                          </small>
                        </div>
                        <button
                          type="button"
                          className="d2-danger-btn d2-danger-btn--sm"
                          data-testid={`settings-revoke-grant-${grant.grant_id}`}
                          onClick={() => void actions.revokeCapabilityGrant(grant.grant_id)}
                        >
                          Отозвать
                        </button>
                      </div>
                    ))}
                  </div>
                )}
              </section>

              {/* 6. Preserved Head & memoryd connection card */}
              <section className="settings-card">
                <div className="settings-card__head">
                  <Database size={16} aria-hidden="true" />
                  <strong>Сервер управления и память</strong>
                </div>

                <form className="settings-url-form" onSubmit={handleReconnect}>
                  <label className="settings-field">
                    <span>Адрес dennett-head (loopback)</span>
                    <div className="settings-url-row">
                      <input
                        type="text"
                        value={urlDraft}
                        onChange={(e) => setUrlDraft(e.target.value)}
                        placeholder="http://127.0.0.1:41771"
                        aria-label="Адрес dennett-head"
                      />
                      <button type="submit" className="popover-secondary-btn">
                        Подключить
                      </button>
                    </div>
                  </label>
                </form>

                <div className="stats-kv">
                  <span>PostgreSQL (dennett_control)</span>
                  <strong>
                    {health?.postgres_connected
                      ? `Подключён (схема v${health.schema_version ?? "?"})`
                      : "Нет соединения"}
                  </strong>
                </div>
                <div className="stats-kv">
                  <span>Независимый memoryd</span>
                  <strong>
                    {health?.memory_status === "healthy"
                      ? "Активен"
                      : health
                        ? "Недоступен (очередь outbox)"
                        : "—"}
                  </strong>
                </div>
                <div className="stats-kv">
                  <span>Записей в очереди outbox</span>
                  <strong>{health?.pending_outbox_count ?? 0}</strong>
                </div>
              </section>
            </>
          ) : (
            /* TAB 2: Capabilities (Skills, Plugins, MCP) */
            <div className="cap-tab-body" data-testid="settings-capabilities-panel">
              <div className="cap-filter-row" data-testid="capabilities-filter-bar">
                {(
                  [
                    { id: "all", label: "Все" },
                    { id: "global", label: "Глобальные" },
                    { id: "project", label: "Текущий проект" },
                    { id: "candidate_unattached", label: "Кандидаты" },
                  ] as const
                ).map((item) => (
                  <button
                    key={item.id}
                    type="button"
                    className={`d2-link-btn ${capFilter === item.id ? "is-selected" : ""}`}
                    onClick={() => setCapFilter(item.id)}
                    data-testid={`cap-filter-${item.id}`}
                  >
                    {item.label}
                  </button>
                ))}
              </div>

              <div className="cap-toolbar">
                <button
                  type="button"
                  className="popover-secondary-btn"
                  onClick={() => void handleDiscover(false)}
                  disabled={!connected || isDiscovering}
                  data-testid="cap-discover-project-btn"
                >
                  <MagnifyingGlass size={14} aria-hidden="true" />
                  Сканировать проект
                </button>
                <button
                  type="button"
                  className="popover-secondary-btn"
                  onClick={() => void handleDiscover(true)}
                  disabled={!connected || isDiscovering}
                  data-testid="cap-discover-gemini-btn"
                  title="Только чтение ~/.gemini/, найденные кандидаты остаются выключенными до вашего включения"
                >
                  <MagnifyingGlass size={14} aria-hidden="true" />
                  Сканировать ~/.gemini/ (только чтение)
                </button>
                <button
                  type="button"
                  className="popover-secondary-btn"
                  onClick={() => handleAddLocalFolderCapability("skill")}
                  disabled={!connected}
                  data-testid="cap-add-skill-btn"
                >
                  <FolderPlus size={14} aria-hidden="true" />
                  Добавить навык
                </button>
                <button
                  type="button"
                  className="popover-secondary-btn"
                  onClick={() => handleAddLocalFolderCapability("plugin")}
                  disabled={!connected}
                  data-testid="cap-add-plugin-btn"
                >
                  <FolderPlus size={14} aria-hidden="true" />
                  Добавить плагин
                </button>
                <button
                  type="button"
                  className="popover-primary-btn"
                  onClick={() => setShowMcpForm((v) => !v)}
                  disabled={!connected}
                  data-testid="cap-toggle-mcp-btn"
                >
                  <PuzzlePiece size={14} aria-hidden="true" />
                  Подключить MCP
                </button>
              </div>

              <p className="settings-note">
                Сканирование <code>~/.gemini/</code> выполняется строго в режиме{" "}
                <strong>только чтение</strong>: кандидаты не модифицируют файлы и остаются
                отключёнными до явного включения. Локальные навыки и плагины добавляются через
                системный выбор папки.
              </p>

              {discoveryFeedback && (
                <div className="memory-alert memory-alert--ok" data-testid="cap-discovery-feedback">
                  <Check size={14} aria-hidden="true" />
                  <span>{discoveryFeedback}</span>
                </div>
              )}

              {showMcpForm && (
                <form
                  className="settings-card memory-inline-form"
                  onSubmit={(e) => void handleCreateMcp(e)}
                  data-testid="cap-mcp-form"
                >
                  <strong>Подключить MCP-сервер</strong>
                  <input
                    type="text"
                    value={mcpName}
                    onChange={(e) => setMcpName(e.target.value)}
                    placeholder="Название сервера (например GitHub MCP)"
                    aria-label="Название MCP-сервера"
                    data-testid="cap-mcp-name-input"
                    required
                  />
                  <input
                    type="text"
                    value={mcpCommand}
                    onChange={(e) => setMcpCommand(e.target.value)}
                    placeholder="Команда запуска (например npx)"
                    aria-label="Команда запуска MCP"
                    data-testid="cap-mcp-command-input"
                    required
                  />
                  <input
                    type="text"
                    value={mcpArgs}
                    onChange={(e) => setMcpArgs(e.target.value)}
                    placeholder="Аргументы через пробел (например -y @modelcontextprotocol/server)"
                    aria-label="Аргументы MCP"
                    data-testid="cap-mcp-args-input"
                  />
                  <div className="settings-mcp-secret-row">
                    <input
                      type="text"
                      value={mcpSecretSlot}
                      onChange={(e) => setMcpSecretSlot(e.target.value)}
                      placeholder="Слот секрета (необязательно, напр. mcp_token)"
                      aria-label="Имя слота секрета MCP"
                      data-testid="cap-mcp-secret-slot-input"
                    />
                    <input
                      type="password"
                      autoComplete="new-password"
                      value={mcpSecretValue}
                      onChange={(e) => setMcpSecretValue(e.target.value)}
                      placeholder="Значение секрета (write-only)"
                      aria-label="Значение секрета MCP"
                      data-testid="cap-mcp-secret-value-input"
                    />
                  </div>
                  <div className="d2-item-card__actions">
                    <button
                      type="submit"
                      className="popover-primary-btn"
                      data-testid="cap-mcp-submit-btn"
                    >
                      Подключить
                    </button>
                    <button
                      type="button"
                      className="popover-secondary-btn"
                      onClick={() => setShowMcpForm(false)}
                    >
                      Отмена
                    </button>
                  </div>
                </form>
              )}

              {filteredCapabilities.length === 0 ? (
                <div className="d2-empty-box" data-testid="cap-empty-state">
                  <PuzzlePiece size={22} aria-hidden="true" />
                  <p>В выбранном фильтре пока нет навыков, плагинов или MCP-серверов.</p>
                </div>
              ) : (
                <div className="cap-list">
                  {filteredCapabilities.map((cap) => {
                    const isActive =
                      cap.activation_state === "active" || cap.activation_state === "pinned";
                    const compat = compatibilityInfo(cap.compatibility_status);
                    const isEditing = editingCapId === cap.capability_id;

                    return (
                      <article
                        key={cap.capability_id}
                        className="settings-card cap-card"
                        data-testid={`capability-card-${cap.capability_id}`}
                      >
                        <div className="cap-card__top">
                          <div className="cap-card__title">
                            <strong>{cap.display_name}</strong>
                            <span className="d2-meta-chip">v{cap.version}</span>
                            <span className="d2-meta-chip">{capabilityKindLabel(cap.kind)}</span>
                            <span className="d2-meta-chip">
                              {capabilityScopeLabel(cap.scope_kind)}
                            </span>
                            <span className={`d2-pill ${compat.className}`}>{compat.label}</span>
                            <span className="d2-meta-chip">{trustLabel(cap.trust_state)}</span>
                            {cap.origin_kind === "personal_discovered" && (
                              <span className="d2-pill d2-pill--warn">
                                ~/.gemini/ · только чтение
                              </span>
                            )}
                          </div>

                          <button
                            type="button"
                            role="switch"
                            aria-checked={isActive}
                            aria-label={`Включить или отключить ${cap.display_name}`}
                            className={`memory-history-switch cap-switch ${isActive ? "is-checked" : ""}`}
                            onClick={() => void handleToggleCapability(cap)}
                            data-testid={`cap-toggle-${cap.capability_id}`}
                          >
                            <span>{isActive ? "Вкл" : "Выкл"}</span>
                            <span className="memory-switch-track" aria-hidden="true">
                              <span className="memory-switch-thumb" />
                            </span>
                          </button>
                        </div>

                        {cap.description && <p className="settings-note">{cap.description}</p>}
                        {cap.compatibility_detail && (
                          <p className="cap-card__compat-detail">{cap.compatibility_detail}</p>
                        )}

                        {isEditing ? (
                          <form
                            className="memory-inline-form"
                            onSubmit={(e) => void handleApplyCapabilityUpdate(cap, e)}
                            data-testid={`cap-edit-form-${cap.capability_id}`}
                          >
                            <input
                              type="text"
                              value={editCapVersion}
                              onChange={(e) => setEditCapVersion(e.target.value)}
                              aria-label="Новая версия возможности"
                              data-testid={`cap-edit-version-${cap.capability_id}`}
                              required
                            />
                            <input
                              type="text"
                              value={editCapDesc}
                              onChange={(e) => setEditCapDesc(e.target.value)}
                              aria-label="Новое описание возможности"
                            />
                            <div className="d2-item-card__actions">
                              <button
                                type="submit"
                                className="popover-primary-btn"
                                data-testid={`cap-apply-update-${cap.capability_id}`}
                              >
                                Применить обновление
                              </button>
                              <button
                                type="button"
                                className="popover-secondary-btn"
                                onClick={() => setEditingCapId(null)}
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
                              onClick={() => startEditCapability(cap)}
                              data-testid={`cap-update-${cap.capability_id}`}
                            >
                              Обновить
                            </button>
                            {cap.can_rollback && (
                              <button
                                type="button"
                                className="d2-link-btn"
                                onClick={() => void actions.rollbackCapability(cap.capability_id)}
                                data-testid={`cap-rollback-${cap.capability_id}`}
                              >
                                <ClockCounterClockwise size={13} aria-hidden="true" />
                                Откатить
                              </button>
                            )}
                            <button
                              type="button"
                              className="d2-danger-btn d2-danger-btn--sm"
                              onClick={() => void actions.deleteCapability(cap.capability_id)}
                              data-testid={`cap-delete-${cap.capability_id}`}
                            >
                              <Trash size={13} aria-hidden="true" />
                              Удалить
                            </button>
                          </div>
                        )}
                      </article>
                    );
                  })}
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
