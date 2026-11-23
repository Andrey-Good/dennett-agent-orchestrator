-- Dennett Memory Fabric v1 schema (Architecture 81 §§2-4, §3.3, §8.1; V01, V06, V19).
-- Monotonic stream counters are strictly stored in `stream_counters(stream_id, committed_seq)`;
-- PostgreSQL SERIAL / GENERATED AS IDENTITY are intentionally forbidden.

CREATE TABLE IF NOT EXISTS stream_counters (
    stream_id TEXT PRIMARY KEY CHECK (octet_length(stream_id) > 0 AND octet_length(stream_id) <= 256),
    committed_seq BIGINT NOT NULL DEFAULT 0 CHECK (committed_seq >= 0)
);

CREATE TABLE IF NOT EXISTS evidence_objects (
    evidence_id UUID PRIMARY KEY,
    scope TEXT NOT NULL CHECK (octet_length(scope) > 0 AND octet_length(scope) <= 128),
    sha256 TEXT NOT NULL CHECK (sha256 ~ '^[0-9a-f]{64}$'),
    media_type TEXT NOT NULL CHECK (octet_length(media_type) > 0 AND octet_length(media_type) <= 128),
    byte_len BIGINT NOT NULL CHECK (byte_len >= 0),
    content TEXT NOT NULL CHECK (octet_length(content) > 0),
    source_uri TEXT NULL CHECK (source_uri IS NULL OR octet_length(source_uri) <= 1024),
    observed_at TIMESTAMPTZ NULL,
    ingested_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    ingested_by TEXT NOT NULL CHECK (octet_length(ingested_by) > 0 AND octet_length(ingested_by) <= 128),
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX IF NOT EXISTS evidence_objects_by_scope_sha256
    ON evidence_objects(scope, sha256);

CREATE TABLE IF NOT EXISTS memory_events (
    event_id UUID PRIMARY KEY,
    stream_id TEXT NOT NULL CHECK (octet_length(stream_id) > 0 AND octet_length(stream_id) <= 256),
    stream_seq BIGINT NOT NULL CHECK (stream_seq > 0),
    scope TEXT NOT NULL CHECK (octet_length(scope) > 0 AND octet_length(scope) <= 128),
    event_kind TEXT NOT NULL CHECK (event_kind IN ('evidence_ingested', 'note_created', 'note_corrected')),
    principal_id TEXT NOT NULL CHECK (octet_length(principal_id) > 0 AND octet_length(principal_id) <= 128),
    correlation_id TEXT NOT NULL CHECK (octet_length(correlation_id) > 0 AND octet_length(correlation_id) <= 128),
    idempotency_key TEXT NOT NULL CHECK (octet_length(idempotency_key) > 0 AND octet_length(idempotency_key) <= 128),
    payload_fingerprint TEXT NOT NULL CHECK (payload_fingerprint ~ '^[0-9a-f]{64}$'),
    target_entity_id UUID NOT NULL,
    target_revision BIGINT NULL CHECK (target_revision IS NULL OR target_revision > 0),
    observed_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT memory_events_stream_seq_uniq UNIQUE (stream_id, stream_seq),
    CONSTRAINT memory_events_scope_idempotency_uniq UNIQUE (scope, idempotency_key)
);

CREATE INDEX IF NOT EXISTS memory_events_by_scope_created
    ON memory_events(scope, created_at DESC);

-- Exact Architecture 81 §3.3 / V19 schema: link_id primary key, nullable ordinal outside PK,
-- non-unique index on (event_id, evidence_id, relationship) so one evidence object can carry
-- multiple distinct anchors on the same event.
CREATE TABLE IF NOT EXISTS event_evidence_links (
    link_id UUID PRIMARY KEY,
    event_id UUID NOT NULL REFERENCES memory_events(event_id),
    evidence_id UUID NOT NULL REFERENCES evidence_objects(evidence_id),
    relationship TEXT NOT NULL CHECK (octet_length(relationship) > 0 AND octet_length(relationship) <= 64),
    anchor JSONB NULL,
    ordinal INTEGER NULL CHECK (ordinal IS NULL OR ordinal >= 0),
    revision BIGINT NOT NULL DEFAULT 1 CHECK (revision > 0)
);

CREATE INDEX IF NOT EXISTS event_evidence_by_event
    ON event_evidence_links(event_id, evidence_id, relationship);

CREATE INDEX IF NOT EXISTS event_evidence_by_evidence
    ON event_evidence_links(evidence_id);

CREATE TABLE IF NOT EXISTS memory_notes (
    note_id UUID PRIMARY KEY,
    scope TEXT NOT NULL CHECK (octet_length(scope) > 0 AND octet_length(scope) <= 128),
    current_revision BIGINT NOT NULL CHECK (current_revision > 0),
    current_revision_id UUID NOT NULL,
    title TEXT NOT NULL CHECK (octet_length(title) > 0 AND octet_length(title) <= 256),
    body TEXT NOT NULL CHECK (octet_length(body) > 0),
    tags JSONB NOT NULL DEFAULT '[]'::jsonb,
    rationale TEXT NULL,
    last_event_id UUID NOT NULL REFERENCES memory_events(event_id),
    recorded_by TEXT NOT NULL CHECK (octet_length(recorded_by) > 0 AND octet_length(recorded_by) <= 128),
    author_label TEXT NULL CHECK (author_label IS NULL OR octet_length(author_label) <= 128),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS memory_note_revisions (
    revision_id UUID PRIMARY KEY,
    note_id UUID NOT NULL REFERENCES memory_notes(note_id),
    revision BIGINT NOT NULL CHECK (revision > 0),
    title TEXT NOT NULL CHECK (octet_length(title) > 0 AND octet_length(title) <= 256),
    body TEXT NOT NULL CHECK (octet_length(body) > 0),
    tags JSONB NOT NULL DEFAULT '[]'::jsonb,
    rationale TEXT NULL,
    event_id UUID NOT NULL REFERENCES memory_events(event_id),
    recorded_by TEXT NOT NULL CHECK (octet_length(recorded_by) > 0 AND octet_length(recorded_by) <= 128),
    author_label TEXT NULL CHECK (author_label IS NULL OR octet_length(author_label) <= 128),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT memory_note_revisions_note_rev_uniq UNIQUE (note_id, revision),
    CONSTRAINT memory_note_revisions_note_rev_id_uniq UNIQUE (note_id, revision_id)
);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'memory_notes_current_rev_fk'
          AND conrelid = 'memory_notes'::regclass
    ) THEN
        ALTER TABLE memory_notes
            ADD CONSTRAINT memory_notes_current_rev_fk
            FOREIGN KEY (note_id, current_revision_id)
            REFERENCES memory_note_revisions(note_id, revision_id)
            DEFERRABLE INITIALLY DEFERRED;
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS memory_notes_by_scope_updated
    ON memory_notes(scope, updated_at DESC);

CREATE INDEX IF NOT EXISTS memory_notes_fts_idx
    ON memory_notes USING GIN (to_tsvector('simple', title || ' ' || body || ' ' || COALESCE(rationale, '')));

CREATE OR REPLACE FUNCTION dennett_reject_append_only_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'table % is append-only; % is forbidden', TG_TABLE_NAME, TG_OP;
END;
$$;

DROP TRIGGER IF EXISTS trg_memory_events_append_only ON memory_events;
CREATE TRIGGER trg_memory_events_append_only
    BEFORE UPDATE OR DELETE ON memory_events
    FOR EACH ROW
    EXECUTE FUNCTION dennett_reject_append_only_mutation();

DROP TRIGGER IF EXISTS trg_memory_note_revisions_append_only ON memory_note_revisions;
CREATE TRIGGER trg_memory_note_revisions_append_only
    BEFORE UPDATE OR DELETE ON memory_note_revisions
    FOR EACH ROW
    EXECUTE FUNCTION dennett_reject_append_only_mutation();
