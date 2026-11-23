import { useState, type FormEvent } from "react";

import { useWorkspace } from "../state/WorkspaceProvider";
import { CloseIcon, PlusIcon } from "./Icons";
import { formatRuntimeLabel } from "./RuntimeBadge";

export function WorkspacePanel() {
  const { state, actions, meta } = useWorkspace();
  const [showCreateForm, setShowCreateForm] = useState(false);
  const [artifactName, setArtifactName] = useState("");
  const [artifactMime, setArtifactMime] = useState("text/markdown");
  const [artifactContent, setArtifactContent] = useState("");
  const [isCreating, setIsCreating] = useState(false);

  if (!state.isRightPanelOpen) {
    return null;
  }

  const health = state.healthSnapshot?.health ?? null;
  const runtimeInfo = formatRuntimeLabel(
    state.healthSnapshot,
    meta.activeSession?.runtime_binding ?? null,
  );
  const normalizedQuery = state.searchQuery.trim().toLowerCase();
  const artifacts = (state.activeDetail?.artifacts ?? []).filter((art) => {
    if (!normalizedQuery) {
      return true;
    }
    return (
      art.name.toLowerCase().includes(normalizedQuery) ||
      art.mime_type.toLowerCase().includes(normalizedQuery)
    );
  });

  const handleCreateArtifact = async (event: FormEvent) => {
    event.preventDefault();
    if (!artifactName.trim() || !artifactContent.trim()) {
      return;
    }
    setIsCreating(true);
    try {
      const created = await actions.createArtifact({
        name: artifactName.trim(),
        mimeType: artifactMime,
        content: artifactContent,
      });
      if (created) {
        setArtifactName("");
        setArtifactContent("");
        setShowCreateForm(false);
      }
    } finally {
      setIsCreating(false);
    }
  };

  return (
    <aside
      className="surface-card workspace-panel-card"
      aria-label="Рабочее пространство и артефакты"
      data-testid="workspace-sibling-panel"
    >
      <div className="panel-header">
        <h2 className="panel-header__title">Рабочее пространство</h2>
        <button
          type="button"
          className="icon-btn"
          onClick={() => actions.setRightPanelOpen(false)}
          aria-label="Скрыть панель рабочего пространства"
        >
          <CloseIcon />
        </button>
      </div>

      <div className="panel-scroll">
        <section className="panel-section" aria-label="Состояние среды">
          <h3 className="panel-section__title">Среда и движок</h3>
          <div className="info-card">
            <div className="info-row">
              <span className="info-row__label">Движок</span>
              <span className="info-row__value">{runtimeInfo.label}</span>
            </div>
            {runtimeInfo.isSimulated && (
              <div className="info-row">
                <span className="info-row__label">Режим</span>
                <span className="info-row__value">SIMULATED (тестовый двойник)</span>
              </div>
            )}
            <div className="info-row">
              <span className="info-row__label">PostgreSQL</span>
              <span className="info-row__value">
                {health?.postgres_connected ? "подключён" : "недоступен"}
              </span>
            </div>
            <div className="info-row">
              <span className="info-row__label">Память memoryd</span>
              <span className="info-row__value">
                {!health
                  ? "неизвестно"
                  : health.memory_status === "healthy"
                    ? "доступна"
                    : `недоступна (outbox: ${health.pending_outbox_count})`}
              </span>
            </div>
            {meta.activeAgent && (
              <div className="info-row">
                <span className="info-row__label">Конфигурация агента</span>
                <span className="info-row__value">
                  v{meta.activeAgent.current_config.version_number} ·{" "}
                  {meta.activeAgent.current_config.model_preference ?? "default"}
                </span>
              </div>
            )}
          </div>
        </section>

        <section className="panel-section" aria-label="Артефакты сессии">
          <div className="panel-section__head">
            <h3 className="panel-section__title">Артефакты ({artifacts.length})</h3>
            {state.selectedSessionId && (
              <button
                type="button"
                className="sidebar__action"
                onClick={() => setShowCreateForm((v) => !v)}
                aria-label="Создать текстовый артефакт"
                data-testid="toggle-create-artifact-button"
              >
                <PlusIcon />
                Новый
              </button>
            )}
          </div>

          {showCreateForm && (
            <form
              className="inline-form"
              onSubmit={(e) => void handleCreateArtifact(e)}
              data-testid="create-artifact-form"
            >
              <span className="inline-form__title">Создать артефакт</span>
              <input
                type="text"
                className="field-input"
                placeholder="Имя файла (например, notes.md)"
                aria-label="Имя артефакта"
                value={artifactName}
                onChange={(e) => setArtifactName(e.target.value)}
                required
              />
              <select
                className="field-select"
                aria-label="MIME-тип артефакта"
                value={artifactMime}
                onChange={(e) => setArtifactMime(e.target.value)}
              >
                <option value="text/markdown">text/markdown</option>
                <option value="text/plain">text/plain</option>
                <option value="application/json">application/json</option>
              </select>
              <textarea
                className="field-input"
                rows={4}
                placeholder="Текстовое содержимое артефакта..."
                aria-label="Содержимое артефакта"
                value={artifactContent}
                onChange={(e) => setArtifactContent(e.target.value)}
                required
              />
              <div className="inline-form__actions">
                <button
                  type="button"
                  className="btn btn--ghost"
                  onClick={() => setShowCreateForm(false)}
                >
                  Отмена
                </button>
                <button type="submit" className="btn btn--primary" disabled={isCreating}>
                  {isCreating ? "Сохранение..." : "Сохранить"}
                </button>
              </div>
            </form>
          )}

          {artifacts.length === 0 ? (
            <p className="sidebar__empty">В этой сессии артефактов пока нет.</p>
          ) : (
            <ul className="artifact-list" data-testid="artifact-list">
              {artifacts.map((art) => {
                const isSelected = meta.selectedArtifact?.artifact_id === art.artifact_id;
                return (
                  <li key={art.artifact_id}>
                    <button
                      type="button"
                      className={
                        isSelected ? "artifact-item artifact-item--selected" : "artifact-item"
                      }
                      onClick={() => void actions.selectArtifact(art.artifact_id)}
                    >
                      <span className="artifact-item__name">{art.name}</span>
                      <span className="artifact-item__meta">
                        rev {art.revision} · {art.mime_type}
                      </span>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}

          {meta.selectedArtifact && (
            <div className="artifact-preview" data-testid="artifact-preview">
              <div className="artifact-preview__header">
                <span className="artifact-preview__title">{meta.selectedArtifact.name}</span>
                <span className="artifact-preview__meta">
                  rev {meta.selectedArtifact.revision} · {meta.selectedArtifact.sha256.slice(0, 12)}
                </span>
              </div>
              {meta.selectedArtifact.relative_path && (
                <span className="artifact-preview__meta">
                  Путь: {meta.selectedArtifact.relative_path}
                </span>
              )}
              {/* Pure safe React text rendering — NEVER dangerouslySetInnerHTML */}
              <pre className="artifact-preview__code">
                <code>
                  {meta.selectedArtifact.content ?? "(содержимое хранится по относительному пути)"}
                </code>
              </pre>
            </div>
          )}
        </section>

        <section className="panel-section" aria-label="Диагностика">
          <h3 className="panel-section__title">Диагностика</h3>
          {state.diagnostics && state.diagnostics.events.length > 0 ? (
            <ul className="diag-list">
              {state.diagnostics.events.slice(0, 6).map((evt) => (
                <li key={evt.event_id} className="diag-item">
                  <span className="diag-item__kind">
                    {evt.component} · {evt.event_kind}
                  </span>
                  <span className="diag-item__summary">{evt.safe_summary}</span>
                </li>
              ))}
            </ul>
          ) : (
            <p className="sidebar__empty">Событий диагностики пока нет.</p>
          )}
        </section>
      </div>
    </aside>
  );
}
