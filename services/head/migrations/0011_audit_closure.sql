-- Dennett Control Schema v11 (Audit Closure):
-- 1. `agent_config_versions.autonomy_mode` (`auto` | `full_access`) for 2-mode autonomy.
-- 2. Singleton `user_preferences` table (`id = 1`) for UI/agent language, IANA timezone, enter behavior.
-- 3. `capability_records.agent_id` + `scope_kind = 'agent'` for per-agent scoped skills/MCPs.
-- 4. `tasks` subagent/trigger/session columns (`target_session_id`, `notify_session_id`,
--    `model_override`, `thinking_override`, `connection_override_id`, `skill_slugs_override_json`,
--    `trigger_json`, `paused_manually`, `deleted_at`) + `task_timers` `yearly_wall` recurrence.
-- 5. Soft-delete timestamp `sessions.deleted_at` preserving append-only audit/control_events invariants.
-- 6. Deterministic hidden system project (`00000000-0000-4000-8000-0000000000ff`) backing standalone
--    session reminders and standalone interactive approvals without relaxing project FKs.

-- ============================================================================
-- 1. Agent Autonomy Mode (`auto` | `full_access`)
-- ============================================================================

ALTER TABLE agent_config_versions
    ADD COLUMN IF NOT EXISTS autonomy_mode TEXT NOT NULL DEFAULT 'auto'
        CHECK (autonomy_mode IN ('auto', 'full_access'));

-- ============================================================================
-- 2. Singleton User Preferences (`id = 1`)
-- ============================================================================

CREATE TABLE IF NOT EXISTS user_preferences (
    id SMALLINT PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    ui_language TEXT NOT NULL DEFAULT 'ru'
        CHECK (octet_length(ui_language) > 0 AND octet_length(ui_language) <= 32),
    agent_language TEXT NOT NULL DEFAULT 'ru'
        CHECK (octet_length(agent_language) > 0 AND octet_length(agent_language) <= 32),
    timezone TEXT NOT NULL DEFAULT 'UTC'
        CHECK (octet_length(timezone) > 0 AND octet_length(timezone) <= 128),
    enter_to_send BOOLEAN NOT NULL DEFAULT TRUE,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO user_preferences (id)
VALUES (1)
ON CONFLICT (id) DO NOTHING;

-- ============================================================================
-- 3. Per-Agent Scoped Capabilities (`CapabilityScopeKind::Agent`)
-- ============================================================================

ALTER TABLE capability_records
    ADD COLUMN IF NOT EXISTS agent_id UUID NULL REFERENCES agents(agent_id) ON DELETE CASCADE;

ALTER TABLE capability_records DROP CONSTRAINT IF EXISTS capability_records_scope_kind_check;
ALTER TABLE capability_records ADD CONSTRAINT capability_records_scope_kind_check
    CHECK (scope_kind IN ('global', 'project', 'agent', 'candidate_unattached'));

DROP INDEX IF EXISTS capability_records_identity_uniq;
CREATE UNIQUE INDEX IF NOT EXISTS capability_records_identity_uniq
    ON capability_records(
        slug,
        kind,
        scope_kind,
        COALESCE(project_id, '00000000-0000-4000-8000-000000000000'::uuid),
        COALESCE(agent_id, '00000000-0000-4000-8000-000000000000'::uuid),
        origin_kind
    );

CREATE INDEX IF NOT EXISTS capability_records_by_agent_kind
    ON capability_records(agent_id, kind, activation_state)
    WHERE agent_id IS NOT NULL;

-- ============================================================================
-- 4. Durable Tasks Subagent Overrides, Wake Triggers, Pause/Delete & Yearly Timers
-- ============================================================================

ALTER TABLE tasks
    ADD COLUMN IF NOT EXISTS target_session_id UUID NULL REFERENCES sessions(session_id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS notify_session_id UUID NULL REFERENCES sessions(session_id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS model_override TEXT NULL
        CHECK (model_override IS NULL OR (octet_length(model_override) > 0 AND octet_length(model_override) <= 128)),
    ADD COLUMN IF NOT EXISTS thinking_override TEXT NULL
        CHECK (thinking_override IS NULL OR thinking_override IN ('low', 'medium', 'high', 'xhigh')),
    ADD COLUMN IF NOT EXISTS connection_override_id UUID NULL REFERENCES provider_connections(connection_id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS skill_slugs_override_json TEXT NULL,
    ADD COLUMN IF NOT EXISTS trigger_json TEXT NULL,
    ADD COLUMN IF NOT EXISTS paused_manually BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ NULL;

CREATE INDEX IF NOT EXISTS tasks_by_trigger_waiting
    ON tasks(status)
    WHERE deleted_at IS NULL AND trigger_json IS NOT NULL;

CREATE INDEX IF NOT EXISTS tasks_active_by_updated
    ON tasks(updated_at DESC)
    WHERE deleted_at IS NULL;

ALTER TABLE task_timers DROP CONSTRAINT IF EXISTS task_timers_recurrence_kind_check;
ALTER TABLE task_timers ADD CONSTRAINT task_timers_recurrence_kind_check
    CHECK (recurrence_kind IN ('once', 'interval', 'daily_wall', 'weekly_wall', 'yearly_wall'));

-- ============================================================================
-- 5. Session Soft-Delete Timestamp (preserves V15 append-only audit & journal FKs)
-- ============================================================================

ALTER TABLE sessions
    ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ NULL;

CREATE INDEX IF NOT EXISTS sessions_active_by_updated
    ON sessions(updated_at DESC)
    WHERE deleted_at IS NULL;

-- ============================================================================
-- 6. Hidden System Project Backing Standalone Session Reminders & Approvals
-- ============================================================================

INSERT INTO projects (project_id, name, workspace_path, memory_scope, lifecycle_state)
VALUES (
    '00000000-0000-4000-8000-0000000000ff',
    'Standalone',
    NULL,
    'chat:standalone',
    'active'
)
ON CONFLICT (project_id) DO NOTHING;

-- ============================================================================
-- 7. Allow Deleting Policy Rules Without Breaking Append-Only Decision History
-- ============================================================================
-- `permission_decisions` is strictly append-only (V15), so historical rows keep
-- the UUID of any deleted rule in `matched_rule_id` without blocking DELETE on
-- `policy_rules`.

ALTER TABLE permission_decisions
    DROP CONSTRAINT IF EXISTS permission_decisions_matched_rule_id_fkey;
