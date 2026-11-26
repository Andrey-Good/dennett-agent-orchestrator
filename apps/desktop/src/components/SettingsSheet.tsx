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
  CaretRight,
  Check,
  ClockCounterClockwise,
  Code,
  Cpu,
  Database,
  FolderPlus,
  GearSix,
  Globe,
  Key,
  Keyboard,
  MagnifyingGlass,
  PlugsConnected,
  PuzzlePiece,
  Robot,
  ShieldCheck,
  ShieldWarning,
  Trash,
  WarningCircle,
  X,
} from "@phosphor-icons/react";

import type {
  AgentConfigVersion,
  AutonomyMode,
  CapabilityActivation,
  CapabilityKind,
  CapabilityRecord,
  CapabilityScopeKind,
  CapabilityTrust,
  CompatibilityStatus,
  IndexStatus,
  PlannedProviderInfo,
  PolicyEffect,
  ProviderHealthStatus,
  RuntimeModelOption,
} from "../api/contracts";
import { useWorkspace } from "../state/WorkspaceProvider";
import { formatDecisionReasonRu } from "../utils/formatters";
import { MarkdownRenderer } from "./MarkdownRenderer";
import { PortalPopover } from "./PortalPopover";
import { useModalFocusTrap } from "./useModalFocusTrap";

type SettingsTab = "general" | "providers" | "agent" | "capabilities" | "diagnostics";
type CapabilityFilter = "all" | CapabilityScopeKind;

const LOCAL_PRESETS = [
  { id: "ollama", label: "Ollama (11434)", url: "http://127.0.0.1:11434" },
  { id: "lmstudio", label: "LM Studio (1234)", url: "http://127.0.0.1:1234" },
  { id: "llamacpp", label: "llama.cpp (8080)", url: "http://127.0.0.1:8080" },
] as const;

const COMMON_TIMEZONES = [
  "UTC",
  "Europe/Moscow",
  "Asia/Yekaterinburg",
  "Europe/Kaliningrad",
  "Asia/Novosibirsk",
  "Asia/Vladivostok",
  "Europe/London",
  "Europe/Berlin",
  "Europe/Paris",
  "America/New_York",
  "America/Chicago",
  "America/Los_Angeles",
  "Asia/Dubai",
  "Asia/Singapore",
  "Asia/Tokyo",
  "Australia/Sydney",
] as const;

const ACTIVATION_MODES: Array<{ value: CapabilityActivation; label: string }> = [
  { value: "pinned", label: "Закреплён" },
  { value: "active", label: "Активен" },
  { value: "on_demand", label: "По запросу" },
  { value: "disabled", label: "Отключён" },
];

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

/**
 * Splits a shell-style argument string while respecting single/double quotes and backslash escapes.
 * E.g. `--flag "hello world" 'a b'` -> ["--flag", "hello world", "a b"]
 */
export function parseShellArgs(input: string): string[] {
  const result: string[] = [];
  let current = "";
  let inQuote: "'" | '"' | null = null;
  let hasToken = false;

  for (let i = 0; i < input.length; i += 1) {
    const ch = input[i]!;
    if (inQuote) {
      if (ch === "\\" && inQuote === '"' && i + 1 < input.length) {
        const next = input[i + 1]!;
        if (next === '"' || next === "\\" || next === "$" || next === "`") {
          current += next;
          i += 1;
          continue;
        }
      }
      if (ch === inQuote) {
        inQuote = null;
        continue;
      }
      current += ch;
      hasToken = true;
      continue;
    }

    if (ch === "'" || ch === '"') {
      inQuote = ch;
      hasToken = true;
      continue;
    }
    if (ch === "\\" && i + 1 < input.length) {
      current += input[i + 1]!;
      hasToken = true;
      i += 1;
      continue;
    }
    if (/\s/.test(ch)) {
      if (hasToken) {
        result.push(current);
        current = "";
        hasToken = false;
      }
      continue;
    }
    current += ch;
    hasToken = true;
  }

  if (hasToken) {
    result.push(current);
  }
  return result;
}

function providerHealthInfo(status: ProviderHealthStatus): {
  label: string;
  tone: "ok" | "warn" | "danger" | "neutral";
} {
  switch (status) {
    case "healthy":
      return { label: "Работает", tone: "ok" };
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

function localizeIndexStatus(status: IndexStatus | string): string {
  switch (status) {
    case "healthy":
      return "Работает";
    case "degraded":
      return "Деградация";
    case "lagging":
      return "Отстаёт";
    case "rebuilding":
      return "Переиндексация";
    case "unreachable":
      return "Недоступен";
    default:
      return localizeDiagnosticText(status);
  }
}

function localizeDiagnosticComponent(component: string): string {
  switch (component.trim()) {
    case "SimulatedRuntime":
    case "simulated_runtime":
      return "Симуляция";
    case "head":
      return "Сервер управления";
    case "memoryd":
    case "memory":
      return "Память";
    case "policy":
    case "PolicyEngine":
      return "Политика";
    case "scheduler":
      return "Планировщик";
    case "runtime":
      return "Движок";
    default:
      return localizeDiagnosticText(component);
  }
}

function localizeDiagnosticEventKind(kind: string): string {
  switch (kind.trim()) {
    case "run_succeeded":
      return "Запуск завершён";
    case "run_failed":
      return "Ошибка запуска";
    case "run_cancelled":
      return "Запуск остановлен";
    case "run_started":
      return "Запуск начат";
    case "healthy":
      return "Работает";
    case "degraded":
      return "Деградация";
    case "unreachable":
      return "Недоступен";
    case "permission_denied":
      return "Доступ отклонён";
    case "permission_requested":
      return "Запрос разрешения";
    case "permission_resolved":
      return "Решение по доступу";
    case "memory_outbox_flushed":
      return "Очередь памяти отправлена";
    default:
      return localizeDiagnosticText(kind);
  }
}

function localizeDiagnosticText(raw: string): string {
  return raw
    .replace(/\bSimulatedRuntime\b/g, "Симуляция")
    .replace(/\brun_succeeded\b/g, "Запуск завершён")
    .replace(/\brun_failed\b/g, "Ошибка запуска")
    .replace(/\bhealthy\b/gi, "Работает")
    .replace(/\bdegraded\b/gi, "Деградация")
    .replace(/\bunreachable\b/gi, "Недоступен");
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
    case "agent":
      return "Агент";
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

function formatAgentIdentityHeading(name: string): string {
  const trimmed = name.trim();
  if (!trimmed) {
    return "Агент";
  }
  return /^агент(?:\b|[\s·:-])/i.test(trimmed) ? trimmed : `Агент ${trimmed}`;
}

function localizePlannedProviderNote(note?: string | null): string {
  const trimmed = note?.trim() ?? "";
  if (!trimmed) {
    return "Отложено за пределы этапов D0–D9 — подключение пока недоступно.";
  }
  if (/[а-яА-ЯёЁ]/.test(trimmed)) {
    return trimmed;
  }
  return "Отложено за пределы этапов D0–D9 — подключение пока недоступно.";
}

export function SettingsSheet() {
  const { state, actions, meta } = useWorkspace();
  const [activeTab, setActiveTab] = useState<SettingsTab>("providers");
  const [urlDraft, setUrlDraft] = useState(
    state.healthSnapshot?.head_url ?? "http://127.0.0.1:41771",
  );

  // General tab: timezone popover + filter
  const [tzMenuOpen, setTzMenuOpen] = useState(false);
  const [tzFilter, setTzFilter] = useState("");
  const tzTriggerRef = useRef<HTMLButtonElement | null>(null);

  // Write-only SecretBroker state for Gemini API key & generic secrets (never prefilled!)
  const [geminiKeyDraft, setGeminiKeyDraft] = useState("");
  const [isSavingGeminiKey, setIsSavingGeminiKey] = useState(false);
  const [rotatingSecretSlot, setRotatingSecretSlot] = useState<string | null>(null);
  const [rotateSecretDraft, setRotateSecretDraft] = useState("");
  const [busySecretSlot, setBusySecretSlot] = useState<string | null>(null);

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

  // Agent Profile & Policy Rules tab state
  const [selectedAgentId, setSelectedAgentId] = useState<string | null>(null);
  const [agentMenuOpen, setAgentMenuOpen] = useState(false);
  const agentTriggerRef = useRef<HTMLButtonElement | null>(null);
  const [promptDraft, setPromptDraft] = useState("");
  const [maxTurnsDraft, setMaxTurnsDraft] = useState("");
  const [maxWallDraft, setMaxWallDraft] = useState("");
  const [autonomyDraft, setAutonomyDraft] = useState<AutonomyMode>("auto");
  const [isSavingAgent, setIsSavingAgent] = useState(false);
  const [agentSavedFeedback, setAgentSavedFeedback] = useState<string | null>(null);
  const [agentHistory, setAgentHistory] = useState<AgentConfigVersion[]>([]);
  const [isLoadingHistory, setIsLoadingHistory] = useState(false);

  const [policyScopeDraft, setPolicyScopeDraft] = useState("global");
  const [policyToolDraft, setPolicyToolDraft] = useState("");
  const [policyTargetDraft, setPolicyTargetDraft] = useState("");
  const [policyEffectDraft, setPolicyEffectDraft] = useState<PolicyEffect>("allow");
  const [policyReasonDraft, setPolicyReasonDraft] = useState("");
  const [isCreatingPolicy, setIsCreatingPolicy] = useState(false);

  // Capabilities tab state
  const [capFilter, setCapFilter] = useState<CapabilityFilter>("all");
  const [isDiscovering, setIsDiscovering] = useState(false);
  const [discoveryFeedback, setDiscoveryFeedback] = useState<string | null>(null);
  const [showMcpForm, setShowMcpForm] = useState(false);
  const [mcpName, setMcpName] = useState("");
  const [mcpCommand, setMcpCommand] = useState("");
  const [mcpArgs, setMcpArgs] = useState("");
  const [mcpSecretEnv, setMcpSecretEnv] = useState("");
  const [mcpSecretSlot, setMcpSecretSlot] = useState("");
  const [mcpSecretValue, setMcpSecretValue] = useState("");
  const [editingCapId, setEditingCapId] = useState<string | null>(null);
  const [editCapVersion, setEditCapVersion] = useState("");
  const [editCapDesc, setEditCapDesc] = useState("");
  const [inspectedCapIds, setInspectedCapIds] = useState<Record<string, boolean>>({});

  const dialogRef = useRef<HTMLDivElement | null>(null);

  const closeSettings = useCallback(() => {
    setConnMenuOpen(false);
    setModelMenuOpen(false);
    setTzMenuOpen(false);
    setAgentMenuOpen(false);
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
  }, [
    existingLocalConn?.connection_id,
    existingLocalConn?.display_name,
    existingLocalConn?.endpoint_url,
  ]);

  const editedAgent = useMemo(() => {
    if (selectedAgentId) {
      const found = state.agents.find((a) => a.agent_id === selectedAgentId);
      if (found) {
        return found;
      }
    }
    return meta.activeAgent ?? state.agents[0] ?? null;
  }, [meta.activeAgent, selectedAgentId, state.agents]);

  // Sync editor form fields when switching edited agent or version
  useEffect(() => {
    if (!editedAgent) {
      return;
    }
    setPromptDraft(editedAgent.current_config.system_prompt);
    setMaxTurnsDraft(
      editedAgent.current_config.default_budget?.max_turns != null
        ? String(editedAgent.current_config.default_budget.max_turns)
        : "",
    );
    setMaxWallDraft(
      editedAgent.current_config.default_budget?.max_wall_secs != null
        ? String(editedAgent.current_config.default_budget.max_wall_secs)
        : "",
    );
    setAutonomyDraft(
      editedAgent.current_config.autonomy_mode === "full_access" ? "full_access" : "auto",
    );
  }, [
    editedAgent?.agent_id,
    editedAgent?.current_config.version_number,
    editedAgent?.current_config.system_prompt,
    editedAgent?.current_config.autonomy_mode,
    editedAgent?.current_config.default_budget?.max_turns,
    editedAgent?.current_config.default_budget?.max_wall_secs,
  ]);

  // Load agent config history & policies when viewing Agent tab
  useEffect(() => {
    if (!state.isSettingsOpen || activeTab !== "agent" || !state.healthSnapshot?.connected) {
      return;
    }
    void actions.refreshPolicyRules();
    if (!editedAgent) {
      setAgentHistory([]);
      return;
    }
    let cancelled = false;
    setIsLoadingHistory(true);
    void actions
      .getAgentConfigHistory(editedAgent.agent_id)
      .then((resp) => {
        if (!cancelled) {
          setAgentHistory(resp?.versions ?? []);
        }
      })
      .finally(() => {
        if (!cancelled) {
          setIsLoadingHistory(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [
    actions,
    activeTab,
    editedAgent?.agent_id,
    editedAgent?.current_config.version_number,
    state.healthSnapshot?.connected,
    state.isSettingsOpen,
  ]);

  // Refresh diagnostics & memory index status when opening Diagnostics tab
  useEffect(() => {
    if (!state.isSettingsOpen || activeTab !== "diagnostics" || !state.healthSnapshot?.connected) {
      return;
    }
    void actions.refreshMemoryIndexStatus(meta.activeProject?.memory_scope ?? null);
  }, [
    actions,
    activeTab,
    meta.activeProject?.memory_scope,
    state.healthSnapshot?.connected,
    state.isSettingsOpen,
  ]);

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

  const filteredTimezones = useMemo(() => {
    const q = tzFilter.trim().toLowerCase();
    const currentTz = state.userPreferences.timezone;
    const set = new Set<string>(COMMON_TIMEZONES);
    if (currentTz) {
      set.add(currentTz);
    }
    const list = Array.from(set);
    if (!q) {
      return list;
    }
    return list.filter((tz) => tz.toLowerCase().includes(q));
  }, [state.userPreferences.timezone, tzFilter]);

  if (!state.isSettingsOpen) {
    return null;
  }

  const connected = Boolean(state.healthSnapshot?.connected);
  const health = state.healthSnapshot?.health;
  const catalog = state.runtimeCatalog;
  const isSimulated = Boolean(health?.runtime_is_simulated ?? catalog?.is_simulated);
  const authLabel = isSimulated
    ? "Тестовый CI-движок"
    : catalog?.auth_profile === "local-endpoint"
      ? "Локальный OpenAI-совместимый сервер"
      : catalog?.auth_profile === "gemini-api-key"
        ? "Gemini API key · фоновый"
        : `${catalog?.tier ?? "Google AI Ultra"} · oauth-personal`;
  const engineLabel = isSimulated
    ? "Симуляция"
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
        "Фоновый Gemini API ключ для фоновых задач Antigravity",
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

  const handleRotateGenericSecret = async (slotKey: string, description: string, e: FormEvent) => {
    e.preventDefault();
    const nextVal = rotateSecretDraft.trim();
    if (!nextVal) {
      return;
    }
    setBusySecretSlot(slotKey);
    setRotateSecretDraft("");
    try {
      await actions.setSecret(slotKey, nextVal, description || `Секрет ${slotKey}`);
      setRotatingSecretSlot(null);
    } finally {
      setBusySecretSlot(null);
    }
  };

  const handleDeleteGenericSecret = async (slotKey: string) => {
    setBusySecretSlot(slotKey);
    try {
      await actions.deleteSecret(slotKey);
      if (rotatingSecretSlot === slotKey) {
        setRotatingSecretSlot(null);
      }
    } finally {
      setBusySecretSlot(null);
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
              ? ` (${localizeDiagnosticText(probeResp.connection.last_health_detail)})`
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

  const handleSaveAgentProfile = async (event: FormEvent) => {
    event.preventDefault();
    if (!editedAgent) {
      return;
    }
    setIsSavingAgent(true);
    setAgentSavedFeedback(null);
    try {
      const parsedTurns = maxTurnsDraft.trim() ? Number.parseInt(maxTurnsDraft.trim(), 10) : null;
      const parsedWall = maxWallDraft.trim() ? Number.parseInt(maxWallDraft.trim(), 10) : null;
      const nextBudget =
        parsedTurns || parsedWall || editedAgent.current_config.default_budget
          ? {
              ...(editedAgent.current_config.default_budget ?? {}),
              max_turns: Number.isFinite(parsedTurns) ? parsedTurns : null,
              max_wall_secs: Number.isFinite(parsedWall) ? parsedWall : null,
            }
          : null;
      const updated = await actions.updateAgentProfile(editedAgent.agent_id, {
        system_prompt: promptDraft,
        autonomy_mode: autonomyDraft,
        default_budget: nextBudget,
      });
      if (updated) {
        setAgentSavedFeedback(`Сохранена версия v${updated.current_config.version_number}`);
      }
    } finally {
      setIsSavingAgent(false);
    }
  };

  const handleRollbackAgentVersion = async (version: AgentConfigVersion) => {
    if (!editedAgent) {
      return;
    }
    setIsSavingAgent(true);
    setAgentSavedFeedback(null);
    try {
      const updated = await actions.updateAgentProfile(editedAgent.agent_id, {
        system_prompt: version.system_prompt,
        model_preference: version.model_preference ?? null,
        thinking_level: version.thinking_level ?? null,
        allowed_memory_scopes: version.allowed_memory_scopes,
        workspace_write_policy: version.workspace_write_policy,
        autonomy_mode: version.autonomy_mode ?? "auto",
        default_budget: version.default_budget ?? null,
        attached_skills: version.attached_skills ?? [],
        preferred_connection_id: version.preferred_connection_id ?? null,
      });
      if (updated) {
        setAgentSavedFeedback(
          `Откат к v${version.version_number} -> создана v${updated.current_config.version_number}`,
        );
      }
    } finally {
      setIsSavingAgent(false);
    }
  };

  const handleCreatePolicyRule = async (event: FormEvent) => {
    event.preventDefault();
    if (!policyScopeDraft.trim() || !policyToolDraft.trim() || !policyReasonDraft.trim()) {
      return;
    }
    setIsCreatingPolicy(true);
    try {
      const created = await actions.createPolicyRule({
        scope: policyScopeDraft.trim(),
        tool_name: policyToolDraft.trim(),
        target_pattern: policyTargetDraft.trim() || null,
        effect: policyEffectDraft,
        reason: policyReasonDraft.trim(),
      });
      if (created) {
        setPolicyToolDraft("");
        setPolicyTargetDraft("");
        setPolicyReasonDraft("");
      }
    } finally {
      setIsCreatingPolicy(false);
    }
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
    const args = parseShellArgs(mcpArgs);
    const slotKey = mcpSecretSlot.trim();
    const envVarName = (mcpSecretEnv.trim() || slotKey.toUpperCase()).trim();
    const secretVal = mcpSecretValue.trim();
    setMcpSecretValue("");

    const secretSlots: Record<string, string> = {};
    if (slotKey && envVarName) {
      if (secretVal) {
        await actions.setSecret(slotKey, secretVal, `Секрет MCP-сервера ${mcpName.trim()}`);
      }
      secretSlots[envVarName] = slotKey;
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
      setMcpSecretEnv("");
      setMcpSecretSlot("");
      setShowMcpForm(false);
    }
  };

  // A-UI-P0-2: Promoting candidate_unattached explicitly attaches to active project or global
  const handleSetCapabilityActivation = async (
    cap: CapabilityRecord,
    nextState: CapabilityActivation,
  ) => {
    const attachingCandidate =
      nextState !== "disabled" && cap.scope_kind === "candidate_unattached";
    const promoteScope = attachingCandidate
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

  const handleToggleCapability = async (cap: CapabilityRecord) => {
    const isActive = cap.activation_state === "active" || cap.activation_state === "pinned";
    await handleSetCapabilityActivation(cap, isActive ? "disabled" : "active");
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

  const browserTimezone =
    typeof Intl !== "undefined"
      ? Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC"
      : "UTC";

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
              Язык и часовой пояс, провайдеры, write-only секреты, профиль агента, навыки и
              диагностика
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
          <div className="d2-segmented settings-tabs-segmented" role="tablist" aria-label="Разделы настроек">
            <button
              type="button"
              role="tab"
              aria-selected={activeTab === "general"}
              className={activeTab === "general" ? "is-active" : ""}
              onClick={() => setActiveTab("general")}
              data-testid="settings-tab-general"
            >
              Общие
            </button>
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
              aria-selected={activeTab === "agent"}
              className={activeTab === "agent" ? "is-active" : ""}
              onClick={() => setActiveTab("agent")}
              data-testid="settings-tab-agent"
            >
              Агент и правила
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
            <button
              type="button"
              role="tab"
              aria-selected={activeTab === "diagnostics"}
              className={activeTab === "diagnostics" ? "is-active" : ""}
              onClick={() => setActiveTab("diagnostics")}
              data-testid="settings-tab-diagnostics"
            >
              Диагностика
            </button>
          </div>
          {state.isLoadingD3Settings && <span className="d2-time">Обновление…</span>}
        </div>

        <div className="settings-sheet__body">
          {state.errorBanner && (
            <div
              className="memory-alert memory-alert--danger"
              role="alert"
              data-testid="settings-error-banner"
            >
              <WarningCircle size={15} aria-hidden="true" />
              <span>{state.errorBanner.message}</span>
              <button
                type="button"
                className="popover-action-link"
                onClick={actions.dismissError}
              >
                Скрыть
              </button>
            </div>
          )}

          {/* TAB: GENERAL */}
          {activeTab === "general" && (
            <div className="cap-tab-body" data-testid="settings-general-panel">
              <section className="settings-card">
                <div className="settings-card__head">
                  <Globe size={16} aria-hidden="true" />
                  <strong>Язык и часовой пояс</strong>
                </div>

                <div className="settings-pref-row">
                  <div>
                    <strong>Язык интерфейса</strong>
                    <small>Применяется к системным подписям и элементам управления</small>
                  </div>
                  <div className="d2-segmented" role="group" aria-label="Язык интерфейса">
                    <button
                      type="button"
                      className={state.userPreferences.ui_language === "ru" ? "is-active" : ""}
                      data-testid="settings-pref-ui-ru"
                      disabled={!connected}
                      onClick={() => void actions.updatePreferences({ ui_language: "ru" })}
                    >
                      Русский
                    </button>
                    <button
                      type="button"
                      className={state.userPreferences.ui_language === "en" ? "is-active" : ""}
                      data-testid="settings-pref-ui-en"
                      disabled={!connected}
                      onClick={() => void actions.updatePreferences({ ui_language: "en" })}
                    >
                      English
                    </button>
                  </div>
                </div>

                <div className="settings-pref-row">
                  <div>
                    <strong>Язык ответов агента</strong>
                    <small>Предпочитаемый язык рассуждений и итогов агента</small>
                  </div>
                  <div className="d2-segmented" role="group" aria-label="Язык агента">
                    <button
                      type="button"
                      className={state.userPreferences.agent_language === "ru" ? "is-active" : ""}
                      data-testid="settings-pref-agent-ru"
                      disabled={!connected}
                      onClick={() => void actions.updatePreferences({ agent_language: "ru" })}
                    >
                      Русский
                    </button>
                    <button
                      type="button"
                      className={state.userPreferences.agent_language === "en" ? "is-active" : ""}
                      data-testid="settings-pref-agent-en"
                      disabled={!connected}
                      onClick={() => void actions.updatePreferences({ agent_language: "en" })}
                    >
                      English
                    </button>
                  </div>
                </div>

                <div className="settings-pref-row">
                  <div>
                    <strong>Часовой пояс (IANA)</strong>
                    <small>Используется для расписаний, напоминаний и временных меток</small>
                  </div>
                  <div className="settings-tz-controls">
                    <button
                      ref={tzTriggerRef}
                      type="button"
                      className="project-scope-filter__trigger"
                      aria-expanded={tzMenuOpen}
                      aria-haspopup="menu"
                      disabled={!connected}
                      data-testid="settings-timezone-trigger"
                      onClick={() => setTzMenuOpen((v) => !v)}
                    >
                      <span>{state.userPreferences.timezone}</span>
                      <CaretDown size={11} aria-hidden="true" />
                    </button>
                    <PortalPopover
                      isOpen={tzMenuOpen}
                      anchorEl={tzTriggerRef.current}
                      onClose={() => setTzMenuOpen(false)}
                      placement="bottom-end"
                      className="project-scope-filter__menu settings-tz-popover"
                      role="menu"
                      ariaLabel="Выбор часового пояса IANA"
                      testId="settings-timezone-menu"
                    >
                      <input
                        type="search"
                        className="settings-tz-search"
                        value={tzFilter}
                        onChange={(e) => setTzFilter(e.target.value)}
                        placeholder="Найти IANA-зону (напр. Europe/Moscow)…"
                        aria-label="Поиск часового пояса"
                        data-testid="settings-timezone-search"
                      />
                      <div className="popover-scroll-list">
                        {filteredTimezones.map((tz) => {
                          const selected = tz === state.userPreferences.timezone;
                          return (
                            <button
                              key={tz}
                              type="button"
                              role="menuitemradio"
                              aria-checked={selected}
                              className={selected ? "is-selected" : ""}
                              data-testid={`settings-tz-option-${tz}`}
                              onClick={() => {
                                setTzMenuOpen(false);
                                setTzFilter("");
                                void actions.updatePreferences({ timezone: tz });
                              }}
                            >
                              <span>{tz}</span>
                              {selected && <Check size={13} aria-hidden="true" />}
                            </button>
                          );
                        })}
                        {tzFilter.trim() &&
                          !filteredTimezones.includes(tzFilter.trim()) && (
                            <button
                              type="button"
                              role="menuitem"
                              data-testid="settings-tz-custom-apply"
                              onClick={() => {
                                const custom = tzFilter.trim();
                                setTzMenuOpen(false);
                                setTzFilter("");
                                void actions.updatePreferences({ timezone: custom });
                              }}
                            >
                              <span>Задать «{tzFilter.trim()}»</span>
                            </button>
                          )}
                      </div>
                    </PortalPopover>
                    <button
                      type="button"
                      className="popover-secondary-btn"
                      disabled={!connected}
                      data-testid="settings-timezone-auto-btn"
                      onClick={() =>
                        void actions.updatePreferences({ timezone: browserTimezone })
                      }
                    >
                      Определить ({browserTimezone})
                    </button>
                  </div>
                </div>
              </section>

              <section className="settings-card">
                <div className="settings-card__head">
                  <GearSix size={16} aria-hidden="true" />
                  <strong>Поведение ввода в чате</strong>
                </div>

                <div className="settings-pref-row">
                  <div>
                    <strong>Отправка сообщения</strong>
                    <small>Выберите, какая клавиша отправляет запрос в поле ввода</small>
                  </div>
                  <div className="d2-segmented" role="group" aria-label="Клавиша отправки">
                    <button
                      type="button"
                      className={state.userPreferences.enter_to_send ? "is-active" : ""}
                      data-testid="settings-enter-send-true"
                      disabled={!connected}
                      onClick={() => void actions.updatePreferences({ enter_to_send: true })}
                    >
                      Enter — отправить
                    </button>
                    <button
                      type="button"
                      className={!state.userPreferences.enter_to_send ? "is-active" : ""}
                      data-testid="settings-enter-send-false"
                      disabled={!connected}
                      onClick={() => void actions.updatePreferences({ enter_to_send: false })}
                    >
                      Ctrl+Enter — отправить
                    </button>
                  </div>
                </div>
              </section>

              <section className="settings-card">
                <div className="settings-card__head">
                  <Keyboard size={16} aria-hidden="true" />
                  <strong>Горячие клавиши</strong>
                </div>
                <div className="stats-kv">
                  <span>Поиск по текущему чату</span>
                  <strong>Ctrl + F</strong>
                </div>
                <div className="stats-kv">
                  <span>Палитра команд и проектов</span>
                  <strong>Ctrl + K</strong>
                </div>
                <div className="stats-kv">
                  <span>Открыть настройки</span>
                  <strong>Ctrl + ,</strong>
                </div>
                <div className="stats-kv">
                  <span>Новый чат / Панель файлов</span>
                  <strong>Ctrl + N / Ctrl + \</strong>
                </div>
                <div className="settings-actions">
                  <button
                    type="button"
                    className="popover-secondary-btn"
                    data-testid="settings-open-shortcuts-btn"
                    onClick={() => {
                      closeSettings();
                      actions.setShortcutsOpen(true);
                    }}
                  >
                    <Keyboard size={14} aria-hidden="true" />
                    Все горячие клавиши…
                  </button>
                </div>
              </section>
            </div>
          )}

          {/* TAB: PROVIDERS & SECRETS (Default) */}
          {activeTab === "providers" && (
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
                    <strong>Gemini API key · фоновый</strong>
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
                  <>
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

                    <div className="d2-grant-list" data-testid="settings-connections-list">
                      {state.providerConnections.map((conn) => {
                        const healthInfo = providerHealthInfo(conn.last_health_status);
                        return (
                          <div
                            key={conn.connection_id}
                            className="d2-grant-row"
                            data-testid={`settings-conn-row-${conn.connection_id}`}
                          >
                            <div className="d2-grant-row__info">
                              <strong>
                                {conn.display_name}{" "}
                                {conn.is_default_interactive && (
                                  <span className="d2-meta-chip">Интерактивный</span>
                                )}{" "}
                                {conn.is_default_background && (
                                  <span className="d2-meta-chip">Фоновый</span>
                                )}
                              </strong>
                              <small>
                                {conn.auth_profile}
                                {conn.endpoint_url ? ` · ${conn.endpoint_url}` : ""} ·{" "}
                                {healthInfo.label}
                              </small>
                            </div>
                            <div className="d2-item-card__actions">
                              {!conn.is_default_background && (
                                <button
                                  type="button"
                                  className="d2-link-btn"
                                  disabled={!connected}
                                  data-testid={`connection-set-bg-${conn.connection_id}`}
                                  onClick={() =>
                                    void actions.activateProviderConnection(
                                      conn.connection_id,
                                      "background",
                                    )
                                  }
                                >
                                  На фон
                                </button>
                              )}
                              {conn.provider_kind === "local_http" && (
                                <button
                                  type="button"
                                  className="d2-danger-btn d2-danger-btn--sm"
                                  disabled={!connected}
                                  data-testid={`connection-delete-${conn.connection_id}`}
                                  onClick={() =>
                                    void actions.deleteProviderConnection(conn.connection_id)
                                  }
                                >
                                  Удалить
                                </button>
                              )}
                            </div>
                          </div>
                        );
                      })}
                    </div>
                  </>
                )}

                {meta.activeAgent && (
                  <p className="settings-note" data-testid="settings-agent-identity-note">
                    <strong>{formatAgentIdentityHeading(meta.activeAgent.name)}</strong> (
                    <code>{meta.activeAgent.agent_id}</code> · config v
                    {meta.activeAgent.current_config.version_number}): смена подключения и модели не
                    меняет личность и память агента.
                  </p>
                )}
              </section>

              {/* 2b. All configured secrets (including MCP custom slots) */}
              <section className="settings-card" data-testid="settings-all-secrets-card">
                <div className="settings-card__head">
                  <Key size={16} aria-hidden="true" />
                  <strong>Все секреты Secret Broker ({state.secrets.length})</strong>
                </div>
                {state.secrets.length === 0 ? (
                  <p className="settings-note">
                    Секреты пока не зарегистрированы. Значения хранятся строго в режиме write-only и
                    никогда не возвращаются в UI.
                  </p>
                ) : (
                  <div className="d2-grant-list">
                    {state.secrets.map((sec) => {
                      const isRotating = rotatingSecretSlot === sec.slot_key;
                      const isBusy = busySecretSlot === sec.slot_key;
                      return (
                        <div
                          key={sec.slot_key}
                          className="settings-secret-row-wrap"
                          data-testid={`secret-row-${sec.slot_key}`}
                        >
                          <div className="d2-grant-row">
                            <div className="d2-grant-row__info">
                              <strong>
                                <code>{sec.slot_key}</code>{" "}
                                {sec.configured ? (
                                  <span className="d2-pill d2-pill--status-succeeded">
                                    v{sec.version}
                                    {sec.fingerprint_short ? ` · ${sec.fingerprint_short}` : ""}
                                  </span>
                                ) : (
                                  <span className="d2-meta-chip">Не задан</span>
                                )}
                              </strong>
                              <small>
                                {sec.description || sec.scope} · источник: {sec.source}
                              </small>
                            </div>
                            <div className="d2-item-card__actions">
                              <button
                                type="button"
                                className="d2-link-btn"
                                disabled={!connected || isBusy}
                                data-testid={`secret-rotate-toggle-${sec.slot_key}`}
                                onClick={() => {
                                  setRotatingSecretSlot(isRotating ? null : sec.slot_key);
                                  setRotateSecretDraft("");
                                }}
                              >
                                {sec.configured ? "Обновить" : "Задать"}
                              </button>
                              {sec.configured && (
                                <button
                                  type="button"
                                  className="d2-danger-btn d2-danger-btn--sm"
                                  disabled={!connected || isBusy}
                                  data-testid={`secret-delete-row-${sec.slot_key}`}
                                  onClick={() => void handleDeleteGenericSecret(sec.slot_key)}
                                >
                                  Удалить
                                </button>
                              )}
                            </div>
                          </div>
                          {isRotating && (
                            <form
                              className="settings-secret-form"
                              onSubmit={(e) =>
                                void handleRotateGenericSecret(sec.slot_key, sec.description, e)
                              }
                            >
                              <input
                                type="password"
                                autoComplete="new-password"
                                value={rotateSecretDraft}
                                onChange={(e) => setRotateSecretDraft(e.target.value)}
                                placeholder={`Новое значение для ${sec.slot_key} (write-only)…`}
                                aria-label={`Новое значение секрета ${sec.slot_key}`}
                                data-testid={`secret-rotate-input-${sec.slot_key}`}
                                disabled={!connected || isBusy}
                              />
                              <button
                                type="submit"
                                className="popover-primary-btn"
                                disabled={!connected || isBusy || !rotateSecretDraft.trim()}
                                data-testid={`secret-rotate-save-${sec.slot_key}`}
                              >
                                Сохранить
                              </button>
                            </form>
                          )}
                        </div>
                      );
                    })}
                  </div>
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
                        <small>{localizePlannedProviderNote(provider.note)}</small>
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
                    Активных постоянных разрешений записи нет. Выданные через «Разрешить всегда»
                    права можно мгновенно отозвать здесь или во Входящих.
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
            </>
          )}

          {/* TAB: AGENT PROFILE, HISTORY & POLICY RULES */}
          {activeTab === "agent" && (
            <div className="cap-tab-body" data-testid="settings-agent-panel">
              <section className="settings-card" data-testid="settings-agent-editor">
                <div className="settings-card__head">
                  <Robot size={16} aria-hidden="true" />
                  <strong>Профиль агента</strong>
                </div>

                {!editedAgent ? (
                  <p className="settings-note">Нет доступного агента для редактирования.</p>
                ) : (
                  <>
                    {state.agents.length > 1 && (
                      <div className="settings-pref-row">
                        <span>Выбранный агент</span>
                        <div>
                          <button
                            ref={agentTriggerRef}
                            type="button"
                            className="project-scope-filter__trigger"
                            aria-expanded={agentMenuOpen}
                            aria-haspopup="menu"
                            data-testid="settings-agent-select-trigger"
                            onClick={() => setAgentMenuOpen((v) => !v)}
                          >
                            <span>{editedAgent.name}</span>
                            <CaretDown size={11} aria-hidden="true" />
                          </button>
                          <PortalPopover
                            isOpen={agentMenuOpen}
                            anchorEl={agentTriggerRef.current}
                            onClose={() => setAgentMenuOpen(false)}
                            placement="bottom-end"
                            className="project-scope-filter__menu"
                            role="menu"
                            ariaLabel="Выбор агента"
                          >
                            {state.agents.map((ag) => (
                              <button
                                key={ag.agent_id}
                                type="button"
                                role="menuitemradio"
                                aria-checked={ag.agent_id === editedAgent.agent_id}
                                className={ag.agent_id === editedAgent.agent_id ? "is-selected" : ""}
                                onClick={() => {
                                  setSelectedAgentId(ag.agent_id);
                                  setAgentMenuOpen(false);
                                }}
                              >
                                <span>{ag.name}</span>
                                {ag.agent_id === editedAgent.agent_id && (
                                  <Check size={13} aria-hidden="true" />
                                )}
                              </button>
                            ))}
                          </PortalPopover>
                        </div>
                      </div>
                    )}

                    <form
                      className="memory-inline-form"
                      onSubmit={(e) => void handleSaveAgentProfile(e)}
                    >
                      <label className="settings-field">
                        <span>Системная инструкция (system_prompt)</span>
                        <textarea
                          rows={4}
                          value={promptDraft}
                          onChange={(e) => setPromptDraft(e.target.value)}
                          aria-label="Системная инструкция агента"
                          data-testid="settings-agent-prompt-input"
                          disabled={!connected || isSavingAgent}
                        />
                      </label>

                      <div className="settings-pref-row">
                        <div>
                          <strong>Режим автономности</strong>
                          <small>
                            «Авто» разрешает файлы и shell в рабочей папке, спрашивая только на
                            критических действиях
                          </small>
                        </div>
                        <div className="d2-segmented" role="group" aria-label="Режим автономности">
                          <button
                            type="button"
                            className={autonomyDraft === "auto" ? "is-active" : ""}
                            data-testid="settings-agent-autonomy-auto"
                            onClick={() => setAutonomyDraft("auto")}
                          >
                            Авто
                          </button>
                          <button
                            type="button"
                            className={autonomyDraft === "full_access" ? "is-active" : ""}
                            data-testid="settings-agent-autonomy-full_access"
                            onClick={() => setAutonomyDraft("full_access")}
                          >
                            Полный доступ
                          </button>
                        </div>
                      </div>

                      <div className="settings-mcp-secret-row">
                        <label className="settings-field">
                          <span>Лимит ходов по умолчанию</span>
                          <input
                            type="number"
                            min={1}
                            max={500}
                            value={maxTurnsDraft}
                            onChange={(e) => setMaxTurnsDraft(e.target.value)}
                            placeholder="например 16"
                            aria-label="Лимит ходов по умолчанию"
                            data-testid="settings-agent-max-turns-input"
                            disabled={!connected || isSavingAgent}
                          />
                        </label>
                        <label className="settings-field">
                          <span>Лимит времени (секунд)</span>
                          <input
                            type="number"
                            min={10}
                            max={86400}
                            value={maxWallDraft}
                            onChange={(e) => setMaxWallDraft(e.target.value)}
                            placeholder="например 900"
                            aria-label="Лимит времени в секундах"
                            data-testid="settings-agent-max-wall-input"
                            disabled={!connected || isSavingAgent}
                          />
                        </label>
                      </div>

                      <div className="d2-item-card__actions">
                        <button
                          type="submit"
                          className="popover-primary-btn"
                          disabled={!connected || isSavingAgent}
                          data-testid="settings-agent-save-btn"
                        >
                          {isSavingAgent ? "Сохранение…" : "Сохранить профиль"}
                        </button>
                        {agentSavedFeedback && (
                          <span className="settings-probe-feedback">{agentSavedFeedback}</span>
                        )}
                      </div>
                    </form>
                  </>
                )}
              </section>

              {/* Agent Config Version History & Rollback */}
              <section className="settings-card" data-testid="settings-agent-history">
                <div className="settings-card__head">
                  <ClockCounterClockwise size={16} aria-hidden="true" />
                  <strong>История версий конфигурации ({agentHistory.length})</strong>
                </div>
                {isLoadingHistory ? (
                  <p className="settings-note">Загрузка истории…</p>
                ) : agentHistory.length === 0 ? (
                  <p className="settings-note">История изменений конфигурации пуста.</p>
                ) : (
                  <div className="d2-grant-list">
                    {agentHistory
                      .slice()
                      .sort((a, b) => b.version_number - a.version_number)
                      .map((ver) => {
                        const isCurrent =
                          editedAgent?.current_config_version_id === ver.config_version_id;
                        return (
                          <div
                            key={ver.config_version_id}
                            className="d2-grant-row"
                            data-testid={`agent-history-v-${ver.version_number}`}
                          >
                            <div className="d2-grant-row__info">
                              <strong>
                                Версия v{ver.version_number}{" "}
                                {isCurrent && (
                                  <span className="d2-pill d2-pill--status-succeeded">Текущая</span>
                                )}
                              </strong>
                              <small>
                                Модель: {ver.model_preference ?? "по умолчанию"} ·{" "}
                                {ver.autonomy_mode === "full_access" ? "Полный доступ" : "Авто"} ·{" "}
                                {ver.system_prompt.slice(0, 72)}
                                {ver.system_prompt.length > 72 ? "…" : ""}
                              </small>
                            </div>
                            {!isCurrent && (
                              <button
                                type="button"
                                className="d2-link-btn"
                                disabled={!connected || isSavingAgent}
                                data-testid={`agent-rollback-v-${ver.version_number}`}
                                onClick={() => void handleRollbackAgentVersion(ver)}
                              >
                                <ClockCounterClockwise size={13} aria-hidden="true" />
                                Откатить к v{ver.version_number}
                              </button>
                            )}
                          </div>
                        );
                      })}
                  </div>
                )}
              </section>

              {/* Policy Rules Management (GET/POST/DELETE /v1/policies) */}
              <section className="settings-card" data-testid="settings-policies-card">
                <div className="settings-card__head">
                  <ShieldCheck size={16} aria-hidden="true" />
                  <strong>Правила политики ({state.policyRules.length})</strong>
                </div>

                {state.policyRules.length === 0 ? (
                  <p className="settings-note">
                    Пользовательских правил политики пока нет. Можно задать явные разрешения или
                    запреты по имени инструмента и шаблону цели.
                  </p>
                ) : (
                  <div className="d2-grant-list">
                    {state.policyRules.map((rule) => (
                      <div
                        key={rule.rule_id}
                        className="d2-grant-row"
                        data-testid={`policy-rule-row-${rule.rule_id}`}
                      >
                        <div className="d2-grant-row__info">
                          <strong>
                            <code>{rule.tool_name}</code>{" "}
                            <span
                              className={`d2-pill ${
                                rule.effect === "allow"
                                  ? "d2-pill--status-succeeded"
                                  : "d2-pill--status-failed"
                              }`}
                            >
                              {rule.effect === "allow" ? "Разрешить" : "Запретить"}
                            </span>
                          </strong>
                          <small>
                            область: {rule.scope}
                            {rule.target_pattern ? ` · цель: ${rule.target_pattern}` : ""} ·{" "}
                            {rule.reason}
                          </small>
                        </div>
                        <button
                          type="button"
                          className="d2-danger-btn d2-danger-btn--sm"
                          disabled={!connected}
                          data-testid={`policy-delete-${rule.rule_id}`}
                          onClick={() => void actions.deletePolicyRule(rule.rule_id)}
                        >
                          <Trash size={12} aria-hidden="true" />
                          Удалить
                        </button>
                      </div>
                    ))}
                  </div>
                )}

                <form
                  className="memory-inline-form"
                  data-testid="settings-policy-create-form"
                  onSubmit={(e) => void handleCreatePolicyRule(e)}
                >
                  <strong>Добавить правило политики</strong>
                  <div className="settings-mcp-secret-row">
                    <input
                      type="text"
                      value={policyScopeDraft}
                      onChange={(e) => setPolicyScopeDraft(e.target.value)}
                      placeholder="Область (например global или project:...)"
                      aria-label="Область правила политики"
                      data-testid="policy-scope-input"
                      required
                    />
                    <input
                      type="text"
                      value={policyToolDraft}
                      onChange={(e) => setPolicyToolDraft(e.target.value)}
                      placeholder="Инструмент (например shell_exec или workspace_write)"
                      aria-label="Имя инструмента политики"
                      data-testid="policy-tool-input"
                      required
                    />
                  </div>
                  <div className="settings-mcp-secret-row">
                    <input
                      type="text"
                      value={policyTargetDraft}
                      onChange={(e) => setPolicyTargetDraft(e.target.value)}
                      placeholder="Шаблон цели (необязательно, напр. src/*)"
                      aria-label="Шаблон цели политики"
                      data-testid="policy-target-input"
                    />
                    <div className="d2-segmented" role="group" aria-label="Эффект правила">
                      <button
                        type="button"
                        className={policyEffectDraft === "allow" ? "is-active" : ""}
                        data-testid="policy-effect-allow"
                        onClick={() => setPolicyEffectDraft("allow")}
                      >
                        Разрешить
                      </button>
                      <button
                        type="button"
                        className={policyEffectDraft === "deny" ? "is-active" : ""}
                        data-testid="policy-effect-deny"
                        onClick={() => setPolicyEffectDraft("deny")}
                      >
                        Запретить
                      </button>
                    </div>
                  </div>
                  <input
                    type="text"
                    value={policyReasonDraft}
                    onChange={(e) => setPolicyReasonDraft(e.target.value)}
                    placeholder="Причина правила (например: Разрешить тесты в песочнице)"
                    aria-label="Причина правила политики"
                    data-testid="policy-reason-input"
                    required
                  />
                  <div className="d2-item-card__actions">
                    <button
                      type="submit"
                      className="popover-primary-btn"
                      disabled={!connected || isCreatingPolicy}
                      data-testid="policy-create-submit"
                    >
                      Добавить правило
                    </button>
                  </div>
                </form>
              </section>
            </div>
          )}

          {/* TAB: CAPABILITIES (Skills, Plugins, MCP) — A-UI-P0-2 + MCP fixes */}
          {activeTab === "capabilities" && (
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
                  className={`popover-secondary-btn cap-toolbar__mcp-btn ${showMcpForm ? "is-active" : ""}`}
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
                <strong>только чтение</strong>: кандидаты не модифицируют файлы и при включении
                явно привязываются к проекту или глобальной области.
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
                    placeholder='Аргументы (поддерживаются кавычки, напр. -y --path "My Folder")'
                    aria-label="Аргументы MCP"
                    data-testid="cap-mcp-args-input"
                  />
                  <div className="settings-mcp-secret-grid">
                    <input
                      type="text"
                      value={mcpSecretEnv}
                      onChange={(e) => setMcpSecretEnv(e.target.value)}
                      placeholder="Переменная среды (напр. GITHUB_TOKEN)"
                      aria-label="Имя переменной среды секрета MCP"
                      data-testid="cap-mcp-secret-env-input"
                    />
                    <input
                      type="text"
                      value={mcpSecretSlot}
                      onChange={(e) => setMcpSecretSlot(e.target.value)}
                      placeholder="Слот секрета (напр. mcp_github_token)"
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
                    const isInspected = Boolean(inspectedCapIds[cap.capability_id]);
                    const activeProjectBinding = meta.activeProject
                      ? (cap.project_bindings ?? []).find(
                          (b) => b.project_id === meta.activeProject!.project_id,
                        )
                      : null;

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
                            <span
                              className="d2-meta-chip"
                              data-testid={`cap-scope-${cap.capability_id}`}
                            >
                              {capabilityScopeLabel(cap.scope_kind)}
                            </span>
                            <span className={`d2-pill ${compat.className}`}>{compat.label}</span>
                            <span
                              className={`d2-meta-chip ${
                                cap.trust_state === "trusted"
                                  ? "cap-trust--trusted"
                                  : cap.trust_state === "revoked"
                                    ? "cap-trust--revoked"
                                    : ""
                              }`}
                              data-testid={`cap-trust-badge-${cap.capability_id}`}
                            >
                              {trustLabel(cap.trust_state)}
                            </span>
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

                        {/* 4-mode activation selector (pinned / active / on_demand / disabled) */}
                        <div className="cap-controls-row">
                          <div
                            className="d2-segmented cap-activation-group"
                            role="group"
                            aria-label={`Режим активации ${cap.display_name}`}
                            data-testid={`cap-activation-group-${cap.capability_id}`}
                          >
                            {ACTIVATION_MODES.map((mode) => (
                              <button
                                key={mode.value}
                                type="button"
                                className={cap.activation_state === mode.value ? "is-active" : ""}
                                disabled={!connected}
                                data-testid={`cap-act-${cap.capability_id}-${mode.value}`}
                                onClick={() => void handleSetCapabilityActivation(cap, mode.value)}
                              >
                                {mode.label}
                              </button>
                            ))}
                          </div>

                          {/* Explicit Trust / Revoke controls (no dead-end on restricted/unreviewed!) */}
                          <div className="d2-item-card__actions">
                            {(cap.trust_state === "restricted" ||
                              cap.trust_state === "unreviewed" ||
                              cap.trust_state === "revoked") && (
                              <button
                                type="button"
                                className="popover-secondary-btn"
                                disabled={!connected}
                                data-testid={`cap-trust-btn-${cap.capability_id}`}
                                onClick={() =>
                                  void actions.updateCapability(cap.capability_id, {
                                    trust_state: "trusted",
                                  })
                                }
                              >
                                <ShieldCheck size={13} aria-hidden="true" />
                                Доверять
                              </button>
                            )}
                            {cap.trust_state !== "revoked" && (
                              <button
                                type="button"
                                className="d2-danger-btn d2-danger-btn--sm"
                                disabled={!connected}
                                data-testid={`cap-revoke-btn-${cap.capability_id}`}
                                onClick={() =>
                                  void actions.updateCapability(cap.capability_id, {
                                    trust_state: "revoked",
                                  })
                                }
                              >
                                <ShieldWarning size={13} aria-hidden="true" />
                                Отозвать
                              </button>
                            )}
                          </div>
                        </div>

                        {/* Per-project override binding when an active project exists */}
                        {meta.activeProject &&
                          (cap.scope_kind === "global" ||
                            cap.scope_kind === "candidate_unattached") && (
                            <div
                              className="cap-project-binding-row"
                              data-testid={`cap-project-binding-${cap.capability_id}`}
                            >
                              <span className="d2-time">
                                Для проекта «{meta.activeProject.name}»
                                {activeProjectBinding
                                  ? ` (сейчас: ${activeProjectBinding.activation_state})`
                                  : ""}
                                :
                              </span>
                              <div className="d2-item-card__actions">
                                {(
                                  [
                                    { value: "active", label: "Вкл в проекте" },
                                    { value: "on_demand", label: "По запросу" },
                                    { value: "disabled", label: "Откл в проекте" },
                                  ] as const
                                ).map((opt) => (
                                  <button
                                    key={opt.value}
                                    type="button"
                                    className={`d2-link-btn ${
                                      activeProjectBinding?.activation_state === opt.value
                                        ? "is-selected"
                                        : ""
                                    }`}
                                    disabled={!connected}
                                    data-testid={`cap-project-bind-${cap.capability_id}-${opt.value}`}
                                    onClick={() =>
                                      void actions.setProjectCapabilityBinding(
                                        meta.activeProject!.project_id,
                                        cap.capability_id,
                                        opt.value,
                                      )
                                    }
                                  >
                                    {opt.label}
                                  </button>
                                ))}
                              </div>
                            </div>
                          )}

                        {/* Inline expandable SKILL.md / manifest viewer */}
                        {isInspected && (
                          <div
                            className="cap-manifest-viewer"
                            data-testid={`cap-manifest-viewer-${cap.capability_id}`}
                          >
                            {cap.source_path && (
                              <div className="cap-manifest-meta">
                                <span>Путь:</span> <code>{cap.source_path}</code>
                              </div>
                            )}
                            <div className="cap-manifest-meta">
                              <span>SHA-256:</span> <code>{cap.content_sha256.slice(0, 16)}…</code>
                            </div>

                            {cap.spec.kind === "skill" && (
                              <div className="cap-manifest-skill-body">
                                <MarkdownRenderer content={cap.spec.skill_markdown || "(SKILL.md пуст)"} />
                                {cap.spec.has_scripts && (
                                  <p className="cap-card__compat-detail">
                                    Содержит скрипты — перед доверием проверьте исходники.
                                  </p>
                                )}
                              </div>
                            )}

                            {cap.spec.kind === "mcp_server" && (
                              <div className="cap-manifest-code">
                                <div>
                                  <strong>Команда:</strong> <code>{cap.spec.command}</code>
                                </div>
                                <div>
                                  <strong>Аргументы:</strong>{" "}
                                  <code>
                                    {(cap.spec.args ?? []).length > 0
                                      ? JSON.stringify(cap.spec.args)
                                      : "[]"}
                                  </code>
                                </div>
                                {cap.spec.secret_slots &&
                                  Object.keys(cap.spec.secret_slots).length > 0 && (
                                    <div>
                                      <strong>Слоты секретов:</strong>{" "}
                                      <code>
                                        {Object.entries(cap.spec.secret_slots)
                                          .map(([envName, slot]) => `${envName} -> ${slot}`)
                                          .join(", ")}
                                      </code>
                                    </div>
                                  )}
                              </div>
                            )}

                            {cap.spec.kind === "plugin" && (
                              <div className="cap-manifest-code">
                                <div>
                                  <strong>Manifest:</strong> <code>v{cap.spec.manifest_version}</code>
                                </div>
                                {cap.spec.skills && cap.spec.skills.length > 0 && (
                                  <div>
                                    <strong>Навыки:</strong> <code>{cap.spec.skills.join(", ")}</code>
                                  </div>
                                )}
                                {cap.spec.mcp_servers && cap.spec.mcp_servers.length > 0 && (
                                  <div>
                                    <strong>MCP-серверы:</strong>{" "}
                                    <code>
                                      {cap.spec.mcp_servers.map((m) => m.name).join(", ")}
                                    </code>
                                  </div>
                                )}
                                {cap.spec.has_hooks && (
                                  <p className="cap-card__compat-detail">
                                    Плагин содержит hooks — проверьте перед выдачей доверия.
                                  </p>
                                )}
                              </div>
                            )}
                          </div>
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
                              aria-expanded={isInspected}
                              data-testid={`cap-inspect-btn-${cap.capability_id}`}
                              onClick={() =>
                                setInspectedCapIds((prev) => ({
                                  ...prev,
                                  [cap.capability_id]: !prev[cap.capability_id],
                                }))
                              }
                            >
                              {isInspected ? (
                                <CaretDown size={12} aria-hidden="true" />
                              ) : (
                                <CaretRight size={12} aria-hidden="true" />
                              )}
                              <Code size={13} aria-hidden="true" />
                              {isInspected ? "Скрыть манифест" : "Инспектировать"}
                            </button>
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

          {/* TAB: DIAGNOSTICS (PostgreSQL, memoryd outbox, index status, events) */}
          {activeTab === "diagnostics" && (
            <div className="cap-tab-body" data-testid="settings-diagnostics-panel">
              <section className="settings-card" data-testid="settings-diagnostics-head-card">
                <div className="settings-card__head">
                  <Database size={16} aria-hidden="true" />
                  <strong>Сервер управления (dennett-head) и PostgreSQL</strong>
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
                      ? "Работает"
                      : health
                        ? "Недоступен (очередь outbox)"
                        : "—"}
                  </strong>
                </div>
                <div className="stats-kv">
                  <span>Записей в очереди outbox</span>
                  <strong>{health?.pending_outbox_count ?? 0}</strong>
                </div>
                {(health?.pending_outbox_count ?? 0) > 0 && connected && (
                  <div className="settings-actions">
                    <button
                      type="button"
                      className="popover-secondary-btn"
                      disabled={state.isFlushingOutbox}
                      onClick={() => void actions.flushMemoryOutbox()}
                    >
                      <ArrowsClockwise
                        size={13}
                        className={state.isFlushingOutbox ? "spin" : ""}
                        aria-hidden="true"
                      />
                      Отправить очередь outbox
                    </button>
                  </div>
                )}
              </section>

              <section className="settings-card" data-testid="diagnostics-memory-index-card">
                <div className="settings-card__head">
                  <Database size={16} aria-hidden="true" />
                  <strong>Индекс памяти (состояние и позиция журнала)</strong>
                </div>
                {state.memoryIndexStatus ? (
                  <>
                    <div className="stats-kv">
                      <span>Область</span>
                      <strong>{state.memoryIndexStatus.scope}</strong>
                    </div>
                    <div className="stats-kv">
                      <span>Статус индекса</span>
                      <strong>{localizeIndexStatus(state.memoryIndexStatus.status)}</strong>
                    </div>
                    <div className="stats-kv">
                      <span>Позиция журнала</span>
                      <strong>{state.memoryIndexStatus.watermark_seq}</strong>
                    </div>
                    <div className="stats-kv">
                      <span>Версия модели индекса</span>
                      <strong>{state.memoryIndexStatus.model_version}</strong>
                    </div>
                  </>
                ) : (
                  <p className="settings-note">Нет данных о состоянии индекса памяти.</p>
                )}
                <div className="settings-actions">
                  <button
                    type="button"
                    className="popover-secondary-btn"
                    disabled={!connected}
                    data-testid="diagnostics-refresh-index-btn"
                    onClick={() =>
                      void actions.refreshMemoryIndexStatus(
                        meta.activeProject?.memory_scope ?? null,
                      )
                    }
                  >
                    <ArrowsClockwise size={13} aria-hidden="true" />
                    Обновить статус индекса
                  </button>
                </div>
              </section>

              <section className="settings-card" data-testid="diagnostics-events-card">
                <div className="settings-card__head">
                  <ClockCounterClockwise size={16} aria-hidden="true" />
                  <strong>
                    Последние диагностические события ({state.diagnostics?.events.length ?? 0})
                  </strong>
                </div>
                {!state.diagnostics || state.diagnostics.events.length === 0 ? (
                  <p className="settings-note">Диагностических событий пока нет.</p>
                ) : (
                  <div className="d2-grant-list" data-testid="diagnostics-events-list">
                    {state.diagnostics.events.slice(0, 15).map((ev) => (
                      <div key={ev.event_id} className="d2-grant-row">
                        <div className="d2-grant-row__info">
                          <strong>
                            {localizeDiagnosticComponent(ev.component)} ·{" "}
                            {localizeDiagnosticEventKind(ev.event_kind)}
                          </strong>
                          <small>{localizeDiagnosticText(ev.safe_summary)}</small>
                        </div>
                        <span className="d2-time">
                          {new Date(ev.created_at).toLocaleTimeString("ru-RU", {
                            hour: "2-digit",
                            minute: "2-digit",
                            second: "2-digit",
                          })}
                        </span>
                      </div>
                    ))}
                  </div>
                )}
              </section>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
