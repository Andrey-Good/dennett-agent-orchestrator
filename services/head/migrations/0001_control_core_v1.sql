-- Dennett Head Control v1 schema (Architecture 80 §3.1, 81 §§2, 3.1, 7, 8.1; V02, V04, V05, V06, V29).
-- Monotonic event counters are strictly stored in `control_stream_counters(stream_id, committed_seq)`;
-- PostgreSQL SERIAL / GENERATED AS IDENTITY are intentionally forbidden.

CREATE TABLE IF NOT EXISTS control_stream_counters (
    stream_id TEXT PRIMARY KEY CHECK (octet_length(stream_id) > 0 AND octet_length(stream_id) <= 256),
    committed_seq BIGINT NOT NULL DEFAULT 0 CHECK (committed_seq >= 0)
);

CREATE TABLE IF NOT EXISTS agents (
    agent_id UUID PRIMARY KEY,
    name TEXT NOT NULL CHECK (octet_length(name) > 0 AND octet_length(name) <= 128),
    description TEXT NOT NULL DEFAULT '' CHECK (octet_length(description) <= 2048),
    current_config_version_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS agent_config_versions (
    config_version_id UUID PRIMARY KEY,
    agent_id UUID NOT NULL REFERENCES agents(agent_id),
    version_number BIGINT NOT NULL CHECK (version_number > 0),
    system_prompt TEXT NOT NULL CHECK (octet_length(system_prompt) > 0),
    model_preference TEXT NULL CHECK (model_preference IS NULL OR octet_length(model_preference) <= 128),
    thinking_level TEXT NULL CHECK (thinking_level IS NULL OR thinking_level IN ('low', 'medium', 'high', 'xhigh')),
    allowed_memory_scopes JSONB NOT NULL DEFAULT '[]'::jsonb,
    workspace_write_policy JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT agent_config_versions_agent_ver_uniq UNIQUE (agent_id, version_number),
    CONSTRAINT agent_config_versions_agent_id_uniq UNIQUE (agent_id, config_version_id)
);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'agents_current_config_fk'
          AND conrelid = 'agents'::regclass
    ) THEN
        ALTER TABLE agents
            ADD CONSTRAINT agents_current_config_fk
            FOREIGN KEY (agent_id, current_config_version_id)
            REFERENCES agent_config_versions(agent_id, config_version_id)
            DEFERRABLE INITIALLY DEFERRED;
    END IF;
END $$;

CREATE TABLE IF NOT EXISTS projects (
    project_id UUID PRIMARY KEY,
    name TEXT NOT NULL CHECK (octet_length(name) > 0 AND octet_length(name) <= 128),
    workspace_path TEXT NOT NULL CHECK (octet_length(workspace_path) > 0 AND octet_length(workspace_path) <= 1024),
    memory_scope TEXT NOT NULL CHECK (octet_length(memory_scope) > 0 AND octet_length(memory_scope) <= 128),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS sessions (
    session_id UUID PRIMARY KEY,
    agent_id UUID NOT NULL REFERENCES agents(agent_id),
    project_id UUID NOT NULL REFERENCES projects(project_id),
    title TEXT NOT NULL CHECK (octet_length(title) > 0 AND octet_length(title) <= 256),
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'archived', 'closed')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS sessions_by_updated
    ON sessions(updated_at DESC);

CREATE TABLE IF NOT EXISTS runtime_bindings (
    binding_id UUID PRIMARY KEY,
    session_id UUID NOT NULL UNIQUE REFERENCES sessions(session_id),
    adapter_id TEXT NOT NULL CHECK (octet_length(adapter_id) > 0 AND octet_length(adapter_id) <= 64),
    auth_profile TEXT NOT NULL CHECK (auth_profile IN ('oauth-personal', 'gemini-api-key')),
    tier TEXT NULL CHECK (tier IS NULL OR octet_length(tier) <= 256),
    agy_version TEXT NULL CHECK (agy_version IS NULL OR octet_length(agy_version) <= 64),
    provider_session_id TEXT NULL CHECK (provider_session_id IS NULL OR octet_length(provider_session_id) <= 256),
    workspace_cwd TEXT NOT NULL CHECK (octet_length(workspace_cwd) > 0 AND octet_length(workspace_cwd) <= 1024),
    is_simulated BOOLEAN NOT NULL,
    rehydrate_count BIGINT NOT NULL DEFAULT 0 CHECK (rehydrate_count >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS runs (
    run_id UUID PRIMARY KEY,
    session_id UUID NOT NULL REFERENCES sessions(session_id),
    agent_id UUID NOT NULL REFERENCES agents(agent_id),
    agent_config_version_id UUID NOT NULL REFERENCES agent_config_versions(config_version_id),
    user_message_id UUID NOT NULL,
    assistant_message_id UUID NULL,
    status TEXT NOT NULL CHECK (status IN ('running', 'succeeded', 'cancelled', 'failed', 'degraded')),
    stop_reason TEXT NULL CHECK (stop_reason IS NULL OR octet_length(stop_reason) <= 128),
    error_code TEXT NULL CHECK (error_code IS NULL OR octet_length(error_code) <= 64),
    error_summary TEXT NULL CHECK (error_summary IS NULL OR octet_length(error_summary) <= 2048),
    rehydrated BOOLEAN NOT NULL DEFAULT FALSE,
    correlation_id TEXT NOT NULL CHECK (octet_length(correlation_id) > 0 AND octet_length(correlation_id) <= 128),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ NULL
);

CREATE INDEX IF NOT EXISTS runs_by_session_created
    ON runs(session_id, created_at ASC);

CREATE TABLE IF NOT EXISTS messages (
    message_id UUID PRIMARY KEY,
    session_id UUID NOT NULL REFERENCES sessions(session_id),
    stream_seq BIGINT NOT NULL CHECK (stream_seq > 0),
    role TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system')),
    content TEXT NOT NULL CHECK (octet_length(content) > 0),
    run_id UUID NULL REFERENCES runs(run_id),
    correlation_id TEXT NOT NULL CHECK (octet_length(correlation_id) > 0 AND octet_length(correlation_id) <= 128),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT messages_session_seq_uniq UNIQUE (session_id, stream_seq)
);

CREATE INDEX IF NOT EXISTS messages_by_session_seq
    ON messages(session_id, stream_seq ASC);

CREATE TABLE IF NOT EXISTS artifacts (
    artifact_id UUID PRIMARY KEY,
    session_id UUID NOT NULL REFERENCES sessions(session_id),
    project_id UUID NOT NULL REFERENCES projects(project_id),
    run_id UUID NULL REFERENCES runs(run_id),
    name TEXT NOT NULL CHECK (octet_length(name) > 0 AND octet_length(name) <= 256),
    mime_type TEXT NOT NULL CHECK (octet_length(mime_type) > 0 AND octet_length(mime_type) <= 128),
    revision BIGINT NOT NULL DEFAULT 1 CHECK (revision > 0),
    content TEXT NULL,
    relative_path TEXT NULL CHECK (relative_path IS NULL OR octet_length(relative_path) <= 1024),
    sha256 TEXT NOT NULL CHECK (sha256 ~ '^[0-9a-f]{64}$'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT artifacts_payload_or_path_chk CHECK (content IS NOT NULL OR relative_path IS NOT NULL)
);

CREATE INDEX IF NOT EXISTS artifacts_by_session_created
    ON artifacts(session_id, created_at ASC);

CREATE TABLE IF NOT EXISTS policy_rules (
    rule_id UUID PRIMARY KEY,
    scope TEXT NOT NULL CHECK (octet_length(scope) > 0 AND octet_length(scope) <= 128),
    tool_name TEXT NOT NULL CHECK (octet_length(tool_name) > 0 AND octet_length(tool_name) <= 256),
    target_pattern TEXT NULL CHECK (target_pattern IS NULL OR octet_length(target_pattern) <= 256),
    effect TEXT NOT NULL CHECK (effect IN ('allow', 'deny')),
    reason TEXT NOT NULL CHECK (octet_length(reason) > 0 AND octet_length(reason) <= 1024),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS policy_rules_by_scope
    ON policy_rules(scope, created_at ASC);

CREATE TABLE IF NOT EXISTS permission_decisions (
    decision_id UUID PRIMARY KEY,
    session_id UUID NOT NULL REFERENCES sessions(session_id),
    run_id UUID NULL REFERENCES runs(run_id),
    tool_call_id TEXT NOT NULL CHECK (octet_length(tool_call_id) <= 256),
    tool_name TEXT NOT NULL CHECK (octet_length(tool_name) > 0 AND octet_length(tool_name) <= 256),
    target_pattern TEXT NULL CHECK (target_pattern IS NULL OR octet_length(target_pattern) <= 1024),
    scope TEXT NOT NULL CHECK (octet_length(scope) > 0 AND octet_length(scope) <= 128),
    effect TEXT NOT NULL CHECK (effect IN ('allow', 'deny')),
    matched_rule_id UUID NULL REFERENCES policy_rules(rule_id),
    reason TEXT NOT NULL CHECK (octet_length(reason) > 0 AND octet_length(reason) <= 1024),
    evaluated_outside_model BOOLEAN NOT NULL CHECK (evaluated_outside_model = TRUE),
    correlation_id TEXT NOT NULL CHECK (octet_length(correlation_id) > 0 AND octet_length(correlation_id) <= 128),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS permission_decisions_by_session
    ON permission_decisions(session_id, created_at ASC);

CREATE TABLE IF NOT EXISTS memory_commit_outbox (
    outbox_id UUID PRIMARY KEY,
    session_id UUID NULL REFERENCES sessions(session_id),
    run_id UUID NULL REFERENCES runs(run_id),
    scope TEXT NOT NULL CHECK (octet_length(scope) > 0 AND octet_length(scope) <= 128),
    idempotency_key TEXT NOT NULL CHECK (octet_length(idempotency_key) > 0 AND octet_length(idempotency_key) <= 128),
    operation TEXT NOT NULL CHECK (operation IN ('create_note')),
    payload_json TEXT NOT NULL CHECK (octet_length(payload_json) > 0),
    status TEXT NOT NULL CHECK (status IN ('pending', 'delivered', 'failed')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    last_error TEXT NULL CHECK (last_error IS NULL OR octet_length(last_error) <= 2048),
    delivered_event_id UUID NULL,
    delivered_note_id UUID NULL,
    correlation_id TEXT NOT NULL CHECK (octet_length(correlation_id) > 0 AND octet_length(correlation_id) <= 128),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT memory_commit_outbox_scope_idem_uniq UNIQUE (scope, idempotency_key)
);

CREATE INDEX IF NOT EXISTS memory_commit_outbox_by_status
    ON memory_commit_outbox(status, created_at ASC);

CREATE TABLE IF NOT EXISTS audit_events (
    event_id UUID PRIMARY KEY,
    stream_seq BIGINT NOT NULL CHECK (stream_seq > 0),
    correlation_id TEXT NOT NULL CHECK (octet_length(correlation_id) > 0 AND octet_length(correlation_id) <= 128),
    session_id UUID NULL REFERENCES sessions(session_id),
    run_id UUID NULL REFERENCES runs(run_id),
    component TEXT NOT NULL CHECK (octet_length(component) > 0 AND octet_length(component) <= 64),
    event_kind TEXT NOT NULL CHECK (octet_length(event_kind) > 0 AND octet_length(event_kind) <= 128),
    safe_summary TEXT NOT NULL CHECK (octet_length(safe_summary) > 0 AND octet_length(safe_summary) <= 2048),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT audit_events_stream_seq_uniq UNIQUE (stream_seq)
);

CREATE INDEX IF NOT EXISTS audit_events_by_correlation
    ON audit_events(correlation_id, stream_seq ASC);

CREATE OR REPLACE FUNCTION dennett_control_reject_append_only_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'control table % is append-only; % is forbidden', TG_TABLE_NAME, TG_OP;
END;
$$;

DROP TRIGGER IF EXISTS trg_agent_config_versions_append_only ON agent_config_versions;
CREATE TRIGGER trg_agent_config_versions_append_only
    BEFORE UPDATE OR DELETE ON agent_config_versions
    FOR EACH ROW
    EXECUTE FUNCTION dennett_control_reject_append_only_mutation();

DROP TRIGGER IF EXISTS trg_messages_append_only ON messages;
CREATE TRIGGER trg_messages_append_only
    BEFORE UPDATE OR DELETE ON messages
    FOR EACH ROW
    EXECUTE FUNCTION dennett_control_reject_append_only_mutation();

DROP TRIGGER IF EXISTS trg_permission_decisions_append_only ON permission_decisions;
CREATE TRIGGER trg_permission_decisions_append_only
    BEFORE UPDATE OR DELETE ON permission_decisions
    FOR EACH ROW
    EXECUTE FUNCTION dennett_control_reject_append_only_mutation();

DROP TRIGGER IF EXISTS trg_audit_events_append_only ON audit_events;
CREATE TRIGGER trg_audit_events_append_only
    BEFORE UPDATE OR DELETE ON audit_events
    FOR EACH ROW
    EXECUTE FUNCTION dennett_control_reject_append_only_mutation();
