-- Dennett Memory Fabric v2 schema (Phase D3: Admission, Claims, Conflicts, Dossiers,
-- Interruptible Consolidation, Separable Multi-language FTS Index, Forgetting, Contract J).
-- Stock PostgreSQL 16 only (built-in tsvector/GIN simple + english + russian; zero extensions).
-- Forward-only and idempotent on top of 0001_memory_core_v1.sql.

-- 1. Extend evidence_objects for forgetting & provenance trust tier.
ALTER TABLE evidence_objects
    ADD COLUMN IF NOT EXISTS forgotten_at TIMESTAMPTZ NULL,
    ADD COLUMN IF NOT EXISTS trust_level TEXT NOT NULL DEFAULT 'native',
    ADD COLUMN IF NOT EXISTS instruction_active BOOLEAN NOT NULL DEFAULT true;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'evidence_objects_trust_level_check'
          AND conrelid = 'evidence_objects'::regclass
    ) THEN
        ALTER TABLE evidence_objects
            ADD CONSTRAINT evidence_objects_trust_level_check
            CHECK (trust_level IN ('native', 'external_untrusted', 'reviewed', 'trusted'));
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS evidence_objects_active_scope_sha256
    ON evidence_objects(scope, sha256)
    WHERE forgotten_at IS NULL;

-- 2. Extend memory_notes with admission, salience, topic_key, trust tier, and forgetting.
ALTER TABLE memory_notes
    ADD COLUMN IF NOT EXISTS admission_status TEXT NOT NULL DEFAULT 'admitted',
    ADD COLUMN IF NOT EXISTS salience SMALLINT NOT NULL DEFAULT 50,
    ADD COLUMN IF NOT EXISTS topic_key TEXT NULL,
    ADD COLUMN IF NOT EXISTS trust_level TEXT NOT NULL DEFAULT 'native',
    ADD COLUMN IF NOT EXISTS instruction_active BOOLEAN NOT NULL DEFAULT true,
    ADD COLUMN IF NOT EXISTS forgotten_at TIMESTAMPTZ NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'memory_notes_admission_status_check'
          AND conrelid = 'memory_notes'::regclass
    ) THEN
        ALTER TABLE memory_notes
            ADD CONSTRAINT memory_notes_admission_status_check
            CHECK (admission_status IN ('admitted', 'ephemeral', 'rejected'));
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'memory_notes_salience_check'
          AND conrelid = 'memory_notes'::regclass
    ) THEN
        ALTER TABLE memory_notes
            ADD CONSTRAINT memory_notes_salience_check
            CHECK (salience BETWEEN 0 AND 100);
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'memory_notes_topic_key_check'
          AND conrelid = 'memory_notes'::regclass
    ) THEN
        ALTER TABLE memory_notes
            ADD CONSTRAINT memory_notes_topic_key_check
            CHECK (topic_key IS NULL OR (octet_length(topic_key) > 0 AND octet_length(topic_key) <= 128));
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'memory_notes_trust_level_check'
          AND conrelid = 'memory_notes'::regclass
    ) THEN
        ALTER TABLE memory_notes
            ADD CONSTRAINT memory_notes_trust_level_check
            CHECK (trust_level IN ('native', 'external_untrusted', 'reviewed', 'trusted'));
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS memory_notes_active_scope_updated
    ON memory_notes(scope, updated_at DESC)
    WHERE forgotten_at IS NULL;

CREATE INDEX IF NOT EXISTS memory_notes_active_scope_topic
    ON memory_notes(scope, topic_key)
    WHERE topic_key IS NOT NULL AND forgotten_at IS NULL;

-- 3. Replace memory_note_revisions append-only trigger to allow ONLY controlled forget-scrub.
CREATE OR REPLACE FUNCTION dennett_guard_note_revisions_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'UPDATE'
       AND current_setting('dennett.allow_forget_scrub', true) = 'on'
       AND NEW.revision_id = OLD.revision_id
       AND NEW.note_id = OLD.note_id
       AND NEW.revision = OLD.revision
       AND NEW.event_id = OLD.event_id
       AND NEW.recorded_by = OLD.recorded_by
       AND NEW.created_at = OLD.created_at
       AND NEW.title = '[forgotten]'
       AND NEW.body = '[forgotten]'
       AND NEW.rationale IS NULL
       AND NEW.tags = '[]'::jsonb
    THEN
        RETURN NEW;
    END IF;

    RAISE EXCEPTION 'table % is append-only; % is forbidden (except authorized forget-scrub)', TG_TABLE_NAME, TG_OP;
END;
$$;

DROP TRIGGER IF EXISTS trg_memory_note_revisions_append_only ON memory_note_revisions;
CREATE TRIGGER trg_memory_note_revisions_append_only
    BEFORE UPDATE OR DELETE ON memory_note_revisions
    FOR EACH ROW
    EXECUTE FUNCTION dennett_guard_note_revisions_mutation();

-- 4. Expand memory_events.event_kind CHECK constraint with all D3 MemoryEventKind variants.
DO $$
DECLARE
    con_record RECORD;
BEGIN
    FOR con_record IN
        SELECT conname
        FROM pg_constraint
        WHERE conrelid = 'memory_events'::regclass
          AND contype = 'c'
          AND pg_get_constraintdef(oid) LIKE '%event_kind%'
          AND conname <> 'memory_events_event_kind_v2_check'
    LOOP
        EXECUTE format('ALTER TABLE memory_events DROP CONSTRAINT %I', con_record.conname);
    END LOOP;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'memory_events_event_kind_v2_check'
          AND conrelid = 'memory_events'::regclass
    ) THEN
        ALTER TABLE memory_events
            ADD CONSTRAINT memory_events_event_kind_v2_check
            CHECK (event_kind IN (
                'evidence_ingested',
                'note_created',
                'note_corrected',
                'claim_created',
                'claim_superseded',
                'conflict_detected',
                'conflict_resolved',
                'document_created',
                'document_updated',
                'note_forgotten',
                'evidence_forgotten',
                'claim_forgotten',
                'document_forgotten',
                'package_exported',
                'package_imported'
            ));
    END IF;
END $$;

-- 5. Structured claims with epistemic status, supersession, temporal validity, caveats, and trust tier.
CREATE TABLE IF NOT EXISTS memory_claims (
    claim_id UUID PRIMARY KEY,
    scope TEXT NOT NULL CHECK (octet_length(scope) > 0 AND octet_length(scope) <= 128),
    topic_key TEXT NOT NULL CHECK (octet_length(topic_key) > 0 AND octet_length(topic_key) <= 128),
    claim_key TEXT NULL CHECK (claim_key IS NULL OR (octet_length(claim_key) > 0 AND octet_length(claim_key) <= 128)),
    natural_language TEXT NOT NULL CHECK (octet_length(natural_language) > 0),
    epistemic_status TEXT NOT NULL DEFAULT 'asserted'
        CHECK (epistemic_status IN ('observed', 'asserted', 'inferred', 'hypothesis', 'contested', 'superseded', 'retracted')),
    supersedes_claim_id UUID NULL REFERENCES memory_claims(claim_id),
    valid_from TIMESTAMPTZ NULL,
    valid_to TIMESTAMPTZ NULL,
    source_event_id UUID NOT NULL REFERENCES memory_events(event_id),
    source_note_id UUID NULL REFERENCES memory_notes(note_id),
    caveats JSONB NOT NULL DEFAULT '[]'::jsonb,
    provenance JSONB NOT NULL DEFAULT '{}'::jsonb,
    trust_level TEXT NOT NULL DEFAULT 'native'
        CHECK (trust_level IN ('native', 'external_untrusted', 'reviewed', 'trusted')),
    instruction_active BOOLEAN NOT NULL DEFAULT true,
    forgotten_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT memory_claims_valid_interval_check
        CHECK (valid_from IS NULL OR valid_to IS NULL OR valid_to >= valid_from)
);

CREATE INDEX IF NOT EXISTS idx_claims_scope_topic
    ON memory_claims(scope, topic_key)
    WHERE forgotten_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_claims_supersedes
    ON memory_claims(supersedes_claim_id)
    WHERE supersedes_claim_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_claims_active_status
    ON memory_claims(scope, epistemic_status)
    WHERE forgotten_at IS NULL AND epistemic_status NOT IN ('superseded', 'retracted');

CREATE INDEX IF NOT EXISTS idx_claims_scope_topic_key
    ON memory_claims(scope, topic_key, claim_key)
    WHERE forgotten_at IS NULL;

-- 6. Explicit contradictions & resolutions between competing claims.
CREATE TABLE IF NOT EXISTS memory_conflicts (
    conflict_id UUID PRIMARY KEY,
    scope TEXT NOT NULL CHECK (octet_length(scope) > 0 AND octet_length(scope) <= 128),
    topic_key TEXT NULL CHECK (topic_key IS NULL OR (octet_length(topic_key) > 0 AND octet_length(topic_key) <= 128)),
    conflict_type TEXT NOT NULL
        CHECK (conflict_type IN ('temporal', 'source', 'concurrent_edit', 'semantic', 'policy')),
    candidate_claim_ids UUID[] NOT NULL CHECK (cardinality(candidate_claim_ids) >= 2),
    blocking BOOLEAN NOT NULL DEFAULT false,
    status TEXT NOT NULL DEFAULT 'open'
        CHECK (status IN ('open', 'resolved', 'accepted_ambiguity', 'invalidated')),
    resolution_event_id UUID NULL REFERENCES memory_events(event_id),
    resolution_note TEXT NULL,
    detected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    resolved_at TIMESTAMPTZ NULL
);

CREATE INDEX IF NOT EXISTS idx_conflicts_scope_open
    ON memory_conflicts(scope, detected_at DESC)
    WHERE status = 'open';

CREATE INDEX IF NOT EXISTS idx_conflicts_scope_topic
    ON memory_conflicts(scope, topic_key)
    WHERE topic_key IS NOT NULL;

-- 7. Human-readable versioned topic dossiers / documents.
CREATE TABLE IF NOT EXISTS memory_documents (
    document_id UUID PRIMARY KEY,
    scope TEXT NOT NULL CHECK (octet_length(scope) > 0 AND octet_length(scope) <= 128),
    document_key TEXT NOT NULL CHECK (octet_length(document_key) > 0 AND octet_length(document_key) <= 128),
    title TEXT NOT NULL CHECK (octet_length(title) > 0 AND octet_length(title) <= 256),
    current_version INTEGER NOT NULL DEFAULT 1 CHECK (current_version > 0),
    current_body TEXT NOT NULL CHECK (octet_length(current_body) > 0),
    provenance_claim_ids UUID[] NOT NULL DEFAULT '{}'::uuid[],
    provenance_note_ids UUID[] NOT NULL DEFAULT '{}'::uuid[],
    source_event_id UUID NOT NULL REFERENCES memory_events(event_id),
    principal_id TEXT NOT NULL CHECK (octet_length(principal_id) > 0 AND octet_length(principal_id) <= 128),
    trust_level TEXT NOT NULL DEFAULT 'native'
        CHECK (trust_level IN ('native', 'external_untrusted', 'reviewed', 'trusted')),
    instruction_active BOOLEAN NOT NULL DEFAULT true,
    forgotten_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_documents_active_scope_key
    ON memory_documents(scope, document_key)
    WHERE forgotten_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_documents_active_scope_updated
    ON memory_documents(scope, updated_at DESC)
    WHERE forgotten_at IS NULL;

CREATE TABLE IF NOT EXISTS memory_document_versions (
    version_id UUID PRIMARY KEY,
    document_id UUID NOT NULL REFERENCES memory_documents(document_id),
    version INTEGER NOT NULL CHECK (version > 0),
    body TEXT NOT NULL CHECK (octet_length(body) > 0),
    provenance_claim_ids UUID[] NOT NULL DEFAULT '{}'::uuid[],
    diff_summary TEXT NULL,
    event_id UUID NOT NULL REFERENCES memory_events(event_id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT memory_document_versions_doc_ver_uniq UNIQUE (document_id, version)
);

CREATE OR REPLACE FUNCTION dennett_guard_document_versions_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'UPDATE'
       AND current_setting('dennett.allow_forget_scrub', true) = 'on'
       AND NEW.version_id = OLD.version_id
       AND NEW.document_id = OLD.document_id
       AND NEW.version = OLD.version
       AND NEW.event_id = OLD.event_id
       AND NEW.created_at = OLD.created_at
       AND NEW.body = '[forgotten]'
       AND NEW.diff_summary IS NULL
       AND NEW.provenance_claim_ids = '{}'::uuid[]
    THEN
        RETURN NEW;
    END IF;

    RAISE EXCEPTION 'table % is append-only; % is forbidden (except authorized forget-scrub)', TG_TABLE_NAME, TG_OP;
END;
$$;

DROP TRIGGER IF EXISTS trg_memory_document_versions_append_only ON memory_document_versions;
CREATE TRIGGER trg_memory_document_versions_append_only
    BEFORE UPDATE OR DELETE ON memory_document_versions
    FOR EACH ROW
    EXECUTE FUNCTION dennett_guard_document_versions_mutation();

-- 8. Interruptible background consolidation jobs with durable checkpoint/resume state.
CREATE TABLE IF NOT EXISTS memory_consolidation_jobs (
    job_id UUID PRIMARY KEY,
    scope TEXT NOT NULL CHECK (octet_length(scope) > 0 AND octet_length(scope) <= 128),
    job_type TEXT NOT NULL
        CHECK (job_type IN ('episode_summary', 'dossier_update', 'claim_extraction', 'index_rebuild', 'conflict_scan', 'retention_sweep')),
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'running', 'paused', 'completed', 'failed', 'cancelled')),
    checkpoint_data JSONB NULL,
    watermark_event_id UUID NULL REFERENCES memory_events(event_id),
    caveats_preserved BOOLEAN NOT NULL DEFAULT true,
    error_detail TEXT NULL,
    idempotency_key TEXT NULL CHECK (idempotency_key IS NULL OR (octet_length(idempotency_key) > 0 AND octet_length(idempotency_key) <= 128)),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at TIMESTAMPTZ NULL,
    completed_at TIMESTAMPTZ NULL
);

CREATE INDEX IF NOT EXISTS idx_consolidation_active
    ON memory_consolidation_jobs(scope, status)
    WHERE status IN ('pending', 'running', 'paused');

CREATE INDEX IF NOT EXISTS idx_consolidation_scope_created
    ON memory_consolidation_jobs(scope, created_at DESC);

CREATE UNIQUE INDEX IF NOT EXISTS idx_consolidation_scope_idempotency
    ON memory_consolidation_jobs(scope, idempotency_key)
    WHERE idempotency_key IS NOT NULL;

-- 9. Separable, rebuildable multi-language PostgreSQL FTS search index + index watermark state.
CREATE TABLE IF NOT EXISTS memory_search_index (
    entry_id UUID PRIMARY KEY,
    source_type TEXT NOT NULL CHECK (source_type IN ('note', 'claim', 'document')),
    source_id UUID NOT NULL,
    scope TEXT NOT NULL CHECK (octet_length(scope) > 0 AND octet_length(scope) <= 128),
    tsv_simple TSVECTOR NOT NULL,
    tsv_english TSVECTOR NULL,
    tsv_russian TSVECTOR NULL,
    indexed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    source_updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_search_tsv_simple
    ON memory_search_index USING GIN(tsv_simple);
CREATE INDEX IF NOT EXISTS idx_search_tsv_english
    ON memory_search_index USING GIN(tsv_english)
    WHERE tsv_english IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_search_tsv_russian
    ON memory_search_index USING GIN(tsv_russian)
    WHERE tsv_russian IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_search_scope
    ON memory_search_index(scope);
CREATE UNIQUE INDEX IF NOT EXISTS idx_search_source_uniq
    ON memory_search_index(source_type, source_id);

CREATE TABLE IF NOT EXISTS memory_index_state (
    scope TEXT PRIMARY KEY CHECK (octet_length(scope) > 0 AND octet_length(scope) <= 128),
    watermark_event_id UUID NULL REFERENCES memory_events(event_id),
    watermark_seq BIGINT NOT NULL DEFAULT 0 CHECK (watermark_seq >= 0),
    model_version TEXT NOT NULL DEFAULT 'fts-v1' CHECK (octet_length(model_version) > 0),
    status TEXT NOT NULL DEFAULT 'healthy'
        CHECK (status IN ('healthy', 'lagging', 'rebuilding', 'degraded')),
    last_rebuild_at TIMESTAMPTZ NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 10. Content-free forget audit tombstones with honest residual backup disclosure.
CREATE TABLE IF NOT EXISTS memory_forget_tombstones (
    tombstone_id UUID PRIMARY KEY,
    scope TEXT NOT NULL CHECK (octet_length(scope) > 0 AND octet_length(scope) <= 128),
    target_type TEXT NOT NULL CHECK (target_type IN ('note', 'evidence', 'claim', 'document', 'event')),
    target_id UUID NOT NULL,
    forget_event_id UUID NOT NULL REFERENCES memory_events(event_id),
    principal_id TEXT NOT NULL CHECK (octet_length(principal_id) > 0 AND octet_length(principal_id) <= 128),
    reason TEXT NOT NULL CHECK (octet_length(reason) > 0),
    residual_backup_notice TEXT NOT NULL,
    propagation_status TEXT NOT NULL DEFAULT 'pending'
        CHECK (propagation_status IN ('pending', 'index_cleared', 'projections_cleared', 'completed', 'failed')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_tombstones_target
    ON memory_forget_tombstones(target_type, target_id);
CREATE INDEX IF NOT EXISTS idx_tombstones_scope_created
    ON memory_forget_tombstones(scope, created_at DESC);

-- 11. Contract J portable export/import audit tables.
CREATE TABLE IF NOT EXISTS memory_export_packages (
    package_id UUID PRIMARY KEY,
    scope TEXT NOT NULL CHECK (octet_length(scope) > 0 AND octet_length(scope) <= 128),
    manifest_sha256 TEXT NOT NULL CHECK (manifest_sha256 ~ '^[0-9a-f]{64}$'),
    format_version TEXT NOT NULL DEFAULT '1.0' CHECK (octet_length(format_version) > 0),
    exported_by TEXT NOT NULL CHECK (octet_length(exported_by) > 0 AND octet_length(exported_by) <= 128),
    item_count INTEGER NOT NULL CHECK (item_count >= 0),
    byte_size BIGINT NOT NULL CHECK (byte_size >= 0),
    secrets_stripped BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_export_packages_scope_created
    ON memory_export_packages(scope, created_at DESC);

CREATE TABLE IF NOT EXISTS memory_import_records (
    import_id UUID PRIMARY KEY,
    scope TEXT NOT NULL CHECK (octet_length(scope) > 0 AND octet_length(scope) <= 128),
    source_package_sha256 TEXT NOT NULL CHECK (source_package_sha256 ~ '^[0-9a-f]{64}$'),
    trust_level TEXT NOT NULL DEFAULT 'external_untrusted'
        CHECK (trust_level IN ('external_untrusted', 'reviewed', 'trusted')),
    instruction_active BOOLEAN NOT NULL DEFAULT false,
    items_imported INTEGER NOT NULL CHECK (items_imported >= 0),
    items_rejected INTEGER NOT NULL DEFAULT 0 CHECK (items_rejected >= 0),
    validation_errors JSONB NOT NULL DEFAULT '[]'::jsonb,
    imported_by TEXT NOT NULL CHECK (octet_length(imported_by) > 0 AND octet_length(imported_by) <= 128),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_import_records_scope_created
    ON memory_import_records(scope, created_at DESC);
