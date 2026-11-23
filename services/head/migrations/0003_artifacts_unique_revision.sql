-- Dennett Control Schema v3 (D1)
-- Enforces unique artifact revisions per (session_id, name) to prevent duplicate revisions under concurrency.

ALTER TABLE artifacts
    ADD CONSTRAINT artifacts_session_name_revision_unique
    UNIQUE (session_id, name, revision);
