CREATE TABLE session_heads (
    session_id TEXT PRIMARY KEY NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0)
);

CREATE TABLE session_events (
    event_id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0),
    payload_version INTEGER NOT NULL CHECK (payload_version > 0),
    command_id TEXT UNIQUE,
    body_json TEXT NOT NULL,
    event_sha256 BLOB NOT NULL CHECK (length(event_sha256) = 32),
    committed_at_unix_ms INTEGER NOT NULL CHECK (committed_at_unix_ms >= 0),
    UNIQUE (session_id, revision),
    FOREIGN KEY (session_id) REFERENCES session_heads(session_id)
);

CREATE INDEX session_events_by_session
    ON session_events(session_id, revision);

CREATE TABLE client_drafts (
    session_id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL,
    command_id TEXT UNIQUE NOT NULL,
    text TEXT NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL CHECK (updated_at_unix_ms >= 0)
);

PRAGMA user_version = 1;
