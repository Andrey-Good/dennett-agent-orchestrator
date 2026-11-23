import type { DesktopHealthSnapshot, RuntimeBinding } from "../api/contracts";

export interface RuntimeBadgeProps {
  initialized?: boolean;
  healthSnapshot: DesktopHealthSnapshot | null;
  binding?: RuntimeBinding | null;
}

export function formatRuntimeLabel(
  healthSnapshot: DesktopHealthSnapshot | null,
  binding?: RuntimeBinding | null,
  initialized = true,
): {
  label: string;
  isSimulated: boolean;
  isOffline: boolean;
  isConnecting: boolean;
  isMemoryDegraded: boolean;
} {
  if (!initialized && !healthSnapshot) {
    return {
      label: "Подключение...",
      isSimulated: false,
      isOffline: false,
      isConnecting: true,
      isMemoryDegraded: false,
    };
  }

  if (!healthSnapshot || !healthSnapshot.connected || !healthSnapshot.health) {
    return {
      label: "Head офлайн",
      isSimulated: false,
      isOffline: true,
      isConnecting: false,
      isMemoryDegraded: false,
    };
  }

  const health = healthSnapshot.health;
  const isSimulated = Boolean(health.runtime_is_simulated || binding?.is_simulated);
  const isMemoryDegraded = health.memory_status === "degraded_unreachable";

  const engineName =
    (binding?.adapter_id ?? health.runtime_adapter_id) === "antigravity-acp"
      ? "Antigravity ACP"
      : (binding?.adapter_id ?? health.runtime_adapter_id);

  let profileLabel: string;
  if (binding) {
    if (binding.auth_profile === "oauth-personal") {
      profileLabel = binding.tier?.trim() || "Google AI Ultra";
    } else {
      profileLabel = "Gemini API key";
    }
  } else {
    profileLabel = isSimulated ? "Тестовый двойник" : "ожидает сессию";
  }

  return {
    label: `${engineName} · ${profileLabel}`,
    isSimulated,
    isOffline: false,
    isConnecting: false,
    isMemoryDegraded,
  };
}

export function RuntimeBadge({
  initialized = true,
  healthSnapshot,
  binding,
}: RuntimeBadgeProps) {
  const info = formatRuntimeLabel(healthSnapshot, binding, initialized);

  if (info.isConnecting) {
    return (
      <div className="runtime-badge-group" role="status" aria-label="Статус движка">
        <span className="runtime-pill" data-testid="runtime-status-pill">
          {info.label}
        </span>
      </div>
    );
  }

  if (info.isOffline) {
    return (
      <div className="runtime-badge-group" role="status" aria-label="Статус движка">
        <span className="runtime-pill runtime-pill--offline" data-testid="runtime-status-pill">
          <span className="status-dot status-dot--offline" aria-hidden="true" />
          {info.label}
        </span>
      </div>
    );
  }

  return (
    <div className="runtime-badge-group" role="status" aria-label="Статус движка">
      {info.isSimulated && (
        <span
          className="runtime-pill runtime-pill--simulated"
          data-testid="simulated-runtime-badge"
          title="Внимание: активен тестовый двойник runtime, не настоящий agy acp"
        >
          SIMULATED
        </span>
      )}
      <span className="runtime-pill" data-testid="runtime-status-pill">
        <span
          className={
            info.isMemoryDegraded ? "status-dot status-dot--degraded" : "status-dot"
          }
          aria-hidden="true"
        />
        {info.label}
      </span>
    </div>
  );
}
