import { GearSix } from "@phosphor-icons/react";

import { useWorkspace } from "../state/WorkspaceProvider";

function resolveProfileSubtitle(
  isSimulated: boolean,
  authProfile: string | undefined,
  tier: string | null | undefined,
): string {
  if (isSimulated) {
    return "Тестовый контур";
  }
  if (tier && tier.trim().length > 0) {
    return tier.trim();
  }
  if (authProfile === "gemini-api-key") {
    return "Gemini API key";
  }
  return "Google AI Ultra";
}

export function UserSettingsBar() {
  const { state, actions } = useWorkspace();

  const catalog = state.runtimeCatalog;
  const binding = state.activeDetail?.session.runtime_binding;
  const health = state.healthSnapshot?.health;

  const isSimulated = Boolean(
    catalog?.is_simulated ?? binding?.is_simulated ?? health?.runtime_is_simulated,
  );
  const accountName = catalog?.account_label?.trim() || "Владелец";
  const initial = accountName.charAt(0).toUpperCase() || "Д";
  const subtitle = resolveProfileSubtitle(
    isSimulated,
    catalog?.auth_profile ?? binding?.auth_profile,
    catalog?.tier ?? binding?.tier,
  );

  return (
    <footer
      className="user-settings-bar"
      data-testid="user-settings-bar"
      aria-label="Профиль и настройки"
    >
      <div className="user-settings-bar__identity" title={`${accountName} · ${subtitle}`}>
        <span className="user-avatar" aria-hidden="true">
          {initial}
        </span>
        <div className="user-meta">
          <strong data-testid="user-account-label">{accountName}</strong>
          <small data-testid="user-profile-tier">{subtitle}</small>
        </div>
      </div>

      <button
        type="button"
        className={`icon-button user-settings-bar__gear ${state.isSettingsOpen ? "is-active" : ""}`}
        onClick={() => actions.setSettingsOpen(!state.isSettingsOpen)}
        aria-label="Настройки и подключения"
        aria-expanded={state.isSettingsOpen}
        aria-haspopup="dialog"
        data-testid="settings-trigger"
        title="Настройки и подключения"
      >
        <GearSix size={17} />
      </button>
    </footer>
  );
}
