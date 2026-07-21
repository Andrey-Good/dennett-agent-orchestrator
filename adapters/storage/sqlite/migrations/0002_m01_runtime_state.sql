ALTER TABLE client_drafts
    ADD COLUMN revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0);

CREATE TABLE discarded_draft_commands (
    command_id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    discarded_at_unix_ms INTEGER NOT NULL CHECK (discarded_at_unix_ms >= 0)
);

CREATE INDEX discarded_draft_commands_by_session
    ON discarded_draft_commands(session_id);

CREATE TABLE command_admissions (
    accepted_revision INTEGER PRIMARY KEY AUTOINCREMENT,
    command_id TEXT UNIQUE NOT NULL,
    idempotency_key TEXT UNIQUE NOT NULL,
    correlation_id TEXT NOT NULL,
    operation_kind TEXT NOT NULL,
    intent_sha256 BLOB NOT NULL CHECK (length(intent_sha256) = 32),
    admitted_at_unix_ms INTEGER NOT NULL CHECK (admitted_at_unix_ms >= 0)
);

CREATE TABLE runtime_continuations (
    session_id TEXT PRIMARY KEY NOT NULL,
    adapter_id TEXT NOT NULL,
    opaque_handle TEXT NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL CHECK (updated_at_unix_ms >= 0)
);

PRAGMA user_version = 2;
