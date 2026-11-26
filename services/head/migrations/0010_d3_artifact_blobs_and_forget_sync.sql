-- Dennett Control Plane Schema v10:
-- 1. Content-addressed immutable artifact blobs (`artifact_blobs`) + `artifacts.content_byte_size`
--    so workspace-backed artifact snapshots survive overwrites, branch switches, and worktree removal.
-- 2. Singleton watermark (`memory_tombstone_sync_state`) tracking the highest processed
--    `memory_forget_tombstones.tombstone_seq` synced from standalone `dennett-memoryd`.

CREATE TABLE IF NOT EXISTS artifact_blobs (
    sha256 TEXT PRIMARY KEY CHECK (sha256 ~ '^[0-9a-f]{64}$'),
    bytes BYTEA NOT NULL,
    byte_size BIGINT NOT NULL CHECK (byte_size >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE artifacts
    ADD COLUMN IF NOT EXISTS content_byte_size BIGINT NULL
        CHECK (content_byte_size IS NULL OR content_byte_size >= 0);

CREATE TABLE IF NOT EXISTS memory_tombstone_sync_state (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    last_tombstone_seq BIGINT NOT NULL DEFAULT 0 CHECK (last_tombstone_seq >= 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO memory_tombstone_sync_state (singleton, last_tombstone_seq)
VALUES (TRUE, 0)
ON CONFLICT (singleton) DO NOTHING;
