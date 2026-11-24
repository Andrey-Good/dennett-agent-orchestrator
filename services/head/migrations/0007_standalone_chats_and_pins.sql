-- Dennett Head Control Plane Schema v7 (Standalone Chats & Project/Session Pinning).
--
-- Purely additive / forward-compatible migration over schema v6:
--   1. Allow `sessions.project_id` to be NULL for standalone user chats (no bound project).
--   2. Allow `artifacts.project_id` to be NULL so standalone sessions can persist inline artifacts.
--   3. Add `pinned` and `pinned_at` columns + descending pin-order indexes on `projects` and `sessions`.

ALTER TABLE sessions
    ALTER COLUMN project_id DROP NOT NULL;

ALTER TABLE sessions DROP CONSTRAINT IF EXISTS sessions_standalone_kind_check;
ALTER TABLE sessions ADD CONSTRAINT sessions_standalone_kind_check
    CHECK (project_id IS NOT NULL OR session_kind = 'user_chat');

ALTER TABLE artifacts
    ALTER COLUMN project_id DROP NOT NULL;

ALTER TABLE projects
    ADD COLUMN IF NOT EXISTS pinned BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS pinned_at TIMESTAMPTZ NULL;

ALTER TABLE sessions
    ADD COLUMN IF NOT EXISTS pinned BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS pinned_at TIMESTAMPTZ NULL;

CREATE INDEX IF NOT EXISTS idx_projects_pinned
    ON projects (pinned DESC, pinned_at DESC NULLS LAST);

CREATE INDEX IF NOT EXISTS idx_sessions_pinned
    ON sessions (pinned DESC, pinned_at DESC NULLS LAST);
