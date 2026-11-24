-- Dennett Control Schema v4 (Phase D2: Durable Tasks, Timers, Idempotent Signals,
-- Interactive Approvals, Revocable Capability Grants, Action Inbox, Plain-Text Inter-Agent
-- Messages, External Effect Claims, Ordered Delivery Journal & Outbox, Project/Artifact Lifecycle).
-- Strictly preserves monotonic sequence counters via `control_stream_counters` (ZERO SERIAL/BIGSERIAL).

-- 1. Retention & epoch metadata on stream counters (V15, V18)
ALTER TABLE control_stream_counters
    ADD COLUMN IF NOT EXISTS authority_epoch BIGINT NOT NULL DEFAULT 1 CHECK (authority_epoch > 0),
    ADD COLUMN IF NOT EXISTS retained_from_seq BIGINT NOT NULL DEFAULT 1 CHECK (retained_from_seq >= 1);

-- Seed canonical global control journal counter so REPEATABLE READ READ ONLY snapshots
-- always observe a consistent row even on a freshly migrated database.
INSERT INTO control_stream_counters (stream_id, committed_seq, authority_epoch, retained_from_seq)
VALUES ('control:journal', 0, 1, 1)
ON CONFLICT (stream_id) DO NOTHING;

-- 2. Agent default resource budget & attached skills (A07, Contract G)
ALTER TABLE agent_config_versions
    ADD COLUMN IF NOT EXISTS default_budget JSONB NULL,
    ADD COLUMN IF NOT EXISTS attached_skills JSONB NOT NULL DEFAULT '[]'::jsonb;

-- 3. Project lifecycle separation (Contract C: active | archived | detached | deleted_record)
ALTER TABLE projects
    ADD COLUMN IF NOT EXISTS lifecycle_state TEXT NOT NULL DEFAULT 'active'
        CHECK (lifecycle_state IN ('active', 'archived', 'detached', 'deleted_record')),
    ADD COLUMN IF NOT EXISTS archived_at TIMESTAMPTZ NULL,
    ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ NULL,
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT now();

CREATE INDEX IF NOT EXISTS projects_by_lifecycle_created
    ON projects(lifecycle_state, created_at DESC);

-- 4. Client message idempotency key on `messages` (offline reconnect dedup)
ALTER TABLE messages
    ADD COLUMN IF NOT EXISTS client_message_id TEXT NULL
        CHECK (client_message_id IS NULL OR (octet_length(client_message_id) > 0 AND octet_length(client_message_id) <= 128));

CREATE UNIQUE INDEX IF NOT EXISTS messages_session_client_msg_uniq
    ON messages(session_id, client_message_id)
    WHERE client_message_id IS NOT NULL;

-- 5. Artifact completeness (partial | final) and lifecycle separation (Contract D)
ALTER TABLE artifacts
    ADD COLUMN IF NOT EXISTS completeness_status TEXT NOT NULL DEFAULT 'final'
        CHECK (completeness_status IN ('partial', 'final')),
    ADD COLUMN IF NOT EXISTS lifecycle_state TEXT NOT NULL DEFAULT 'active'
        CHECK (lifecycle_state IN ('active', 'archived', 'removed', 'deleted')),
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT now();

ALTER TABLE artifacts DROP CONSTRAINT IF EXISTS artifacts_payload_or_path_chk;
ALTER TABLE artifacts ADD CONSTRAINT artifacts_payload_or_path_chk
    CHECK (lifecycle_state = 'deleted' OR content IS NOT NULL OR relative_path IS NOT NULL);

-- 6. Durable Tasks, Timers, Signals & Runs linkage
CREATE TABLE IF NOT EXISTS tasks (
    task_id UUID PRIMARY KEY,
    project_id UUID NOT NULL REFERENCES projects(project_id),
    session_id UUID NOT NULL REFERENCES sessions(session_id),
    agent_id UUID NOT NULL REFERENCES agents(agent_id),
    parent_task_id UUID NULL REFERENCES tasks(task_id),
    title TEXT NOT NULL CHECK (octet_length(title) > 0 AND octet_length(title) <= 256),
    goal TEXT NOT NULL CHECK (octet_length(goal) > 0 AND octet_length(goal) <= 32768),
    generation BIGINT NOT NULL DEFAULT 1 CHECK (generation > 0),
    status TEXT NOT NULL CHECK (status IN (
        'queued',
        'running',
        'waiting_timer',
        'waiting_signal',
        'waiting_clarification',
        'waiting_approval',
        'succeeded',
        'failed',
        'cancelled',
        'unknown_external'
    )),
    wait_detail TEXT NULL CHECK (wait_detail IS NULL OR octet_length(wait_detail) <= 2048),
    partial_result_summary TEXT NULL CHECK (partial_result_summary IS NULL OR octet_length(partial_result_summary) <= 16384),
    error_code TEXT NULL CHECK (error_code IS NULL OR octet_length(error_code) <= 64),
    error_summary TEXT NULL CHECK (error_summary IS NULL OR octet_length(error_summary) <= 2048),
    active_run_id UUID NULL REFERENCES runs(run_id),
    schedule_json JSONB NULL,
    budget_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    turns_used INTEGER NOT NULL DEFAULT 0 CHECK (turns_used >= 0),
    tokens_used BIGINT NOT NULL DEFAULT 0 CHECK (tokens_used >= 0),
    wall_ms_used BIGINT NOT NULL DEFAULT 0 CHECK (wall_ms_used >= 0),
    usage_confidence TEXT NOT NULL DEFAULT 'unknown'
        CHECK (usage_confidence IN ('provider_reported', 'estimated', 'unknown')),
    attached_skills JSONB NOT NULL DEFAULT '[]'::jsonb,
    auth_profile_override TEXT NULL
        CHECK (auth_profile_override IS NULL OR auth_profile_override IN ('oauth-personal', 'gemini-api-key')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ NULL
);

CREATE INDEX IF NOT EXISTS tasks_by_project_updated
    ON tasks(project_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS tasks_by_status_updated
    ON tasks(status, updated_at ASC);

ALTER TABLE runs
    ADD COLUMN IF NOT EXISTS task_id UUID NULL REFERENCES tasks(task_id),
    ADD COLUMN IF NOT EXISTS task_generation BIGINT NULL
        CHECK (task_generation IS NULL OR task_generation > 0),
    ADD COLUMN IF NOT EXISTS tokens_used BIGINT NULL CHECK (tokens_used IS NULL OR tokens_used >= 0),
    ADD COLUMN IF NOT EXISTS wall_ms BIGINT NULL CHECK (wall_ms IS NULL OR wall_ms >= 0);

ALTER TABLE runs DROP CONSTRAINT IF EXISTS runs_status_check;
ALTER TABLE runs ADD CONSTRAINT runs_status_check
    CHECK (status IN (
        'running',
        'succeeded',
        'cancelled',
        'failed',
        'degraded',
        'superseded',
        'unknown_external'
    ));

CREATE INDEX IF NOT EXISTS runs_by_task_created
    ON runs(task_id, created_at ASC)
    WHERE task_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS task_timers (
    timer_id UUID PRIMARY KEY,
    task_id UUID NOT NULL REFERENCES tasks(task_id),
    task_generation BIGINT NOT NULL CHECK (task_generation > 0),
    idempotency_key TEXT NOT NULL CHECK (octet_length(idempotency_key) > 0 AND octet_length(idempotency_key) <= 128),
    fire_at TIMESTAMPTZ NOT NULL,
    timezone TEXT NOT NULL DEFAULT 'UTC' CHECK (octet_length(timezone) > 0 AND octet_length(timezone) <= 128),
    original_expression TEXT NULL CHECK (original_expression IS NULL OR octet_length(original_expression) <= 256),
    recurrence_kind TEXT NOT NULL CHECK (recurrence_kind IN ('once', 'interval', 'daily_wall')),
    recurrence_spec JSONB NULL,
    missed_run_policy TEXT NOT NULL DEFAULT 'execute_once'
        CHECK (missed_run_policy IN ('execute_once', 'skip', 'ask')),
    status TEXT NOT NULL CHECK (status IN ('pending', 'claimed', 'fired', 'paused', 'skipped', 'cancelled')),
    lease_owner TEXT NULL CHECK (lease_owner IS NULL OR octet_length(lease_owner) <= 128),
    lease_generation BIGINT NOT NULL DEFAULT 0 CHECK (lease_generation >= 0),
    lease_expires_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    fired_at TIMESTAMPTZ NULL,
    CONSTRAINT task_timers_task_idem_uniq UNIQUE (task_id, idempotency_key)
);

CREATE INDEX IF NOT EXISTS task_timers_due_idx
    ON task_timers(status, fire_at ASC)
    WHERE status IN ('pending', 'claimed');

CREATE TABLE IF NOT EXISTS task_signals (
    signal_id UUID PRIMARY KEY,
    task_id UUID NOT NULL REFERENCES tasks(task_id),
    idempotency_key TEXT NOT NULL CHECK (octet_length(idempotency_key) > 0 AND octet_length(idempotency_key) <= 128),
    signal_name TEXT NOT NULL CHECK (octet_length(signal_name) > 0 AND octet_length(signal_name) <= 128),
    payload_text TEXT NOT NULL DEFAULT '' CHECK (octet_length(payload_text) <= 32768),
    payload_sha256 TEXT NOT NULL CHECK (payload_sha256 ~ '^[0-9a-f]{64}$'),
    applied_generation BIGINT NULL,
    outcome_status TEXT NOT NULL CHECK (outcome_status IN ('woke_task', 'queued_on_task', 'rejected_stopped')),
    correlation_id TEXT NOT NULL CHECK (octet_length(correlation_id) > 0 AND octet_length(correlation_id) <= 128),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT task_signals_task_idem_uniq UNIQUE (task_id, idempotency_key)
);

-- 7. Interactive Permissions, Revocable Capability Grants, External Effects, Inbox & Agent Messages
CREATE TABLE IF NOT EXISTS capability_grants (
    grant_id UUID PRIMARY KEY,
    scope TEXT NOT NULL CHECK (octet_length(scope) > 0 AND octet_length(scope) <= 128),
    project_id UUID NOT NULL REFERENCES projects(project_id),
    agent_id UUID NULL REFERENCES agents(agent_id),
    session_id UUID NULL REFERENCES sessions(session_id),
    task_id UUID NULL REFERENCES tasks(task_id),
    tool_pattern TEXT NOT NULL CHECK (octet_length(tool_pattern) > 0 AND octet_length(tool_pattern) <= 256),
    target_prefixes JSONB NOT NULL DEFAULT '[]'::jsonb,
    max_uses INTEGER NULL CHECK (max_uses IS NULL OR max_uses > 0),
    uses_consumed INTEGER NOT NULL DEFAULT 0 CHECK (uses_consumed >= 0),
    reason TEXT NOT NULL CHECK (octet_length(reason) > 0 AND octet_length(reason) <= 1024),
    expires_at TIMESTAMPTZ NULL,
    revoked_at TIMESTAMPTZ NULL,
    revoked_reason TEXT NULL CHECK (revoked_reason IS NULL OR octet_length(revoked_reason) <= 512),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS capability_grants_active_by_project
    ON capability_grants(project_id, created_at DESC)
    WHERE revoked_at IS NULL;

CREATE TABLE IF NOT EXISTS permission_requests (
    request_id UUID PRIMARY KEY,
    project_id UUID NOT NULL REFERENCES projects(project_id),
    session_id UUID NOT NULL REFERENCES sessions(session_id),
    run_id UUID NOT NULL REFERENCES runs(run_id),
    task_id UUID NULL REFERENCES tasks(task_id),
    agent_id UUID NOT NULL REFERENCES agents(agent_id),
    tool_call_id TEXT NOT NULL CHECK (octet_length(tool_call_id) <= 256),
    tool_name TEXT NOT NULL CHECK (octet_length(tool_name) > 0 AND octet_length(tool_name) <= 256),
    tool_kind TEXT NOT NULL CHECK (octet_length(tool_kind) <= 64),
    target_summary TEXT NOT NULL CHECK (octet_length(target_summary) <= 1024),
    normalized_targets JSONB NOT NULL DEFAULT '[]'::jsonb,
    parameters_sha256 TEXT NOT NULL CHECK (parameters_sha256 ~ '^[0-9a-f]{64}$'),
    reason TEXT NOT NULL CHECK (octet_length(reason) > 0 AND octet_length(reason) <= 1024),
    status TEXT NOT NULL CHECK (status IN (
        'pending',
        'approved_once',
        'approved_scope',
        'denied',
        'expired'
    )),
    issued_grant_id UUID NULL REFERENCES capability_grants(grant_id),
    decision_id UUID NULL REFERENCES permission_decisions(decision_id),
    revision BIGINT NOT NULL DEFAULT 1 CHECK (revision > 0),
    expires_at TIMESTAMPTZ NOT NULL,
    resolved_at TIMESTAMPTZ NULL,
    correlation_id TEXT NOT NULL CHECK (octet_length(correlation_id) > 0 AND octet_length(correlation_id) <= 128),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS permission_requests_pending_idx
    ON permission_requests(status, created_at ASC)
    WHERE status = 'pending';

CREATE TABLE IF NOT EXISTS effect_claims (
    effect_id UUID PRIMARY KEY,
    project_id UUID NOT NULL REFERENCES projects(project_id),
    session_id UUID NOT NULL REFERENCES sessions(session_id),
    task_id UUID NULL REFERENCES tasks(task_id),
    run_id UUID NULL REFERENCES runs(run_id),
    agent_id UUID NOT NULL REFERENCES agents(agent_id),
    effect_idempotency_key TEXT NOT NULL CHECK (octet_length(effect_idempotency_key) > 0 AND octet_length(effect_idempotency_key) <= 128),
    connector_id TEXT NOT NULL CHECK (octet_length(connector_id) > 0 AND octet_length(connector_id) <= 64),
    operation TEXT NOT NULL CHECK (octet_length(operation) > 0 AND octet_length(operation) <= 128),
    target_recipient TEXT NOT NULL CHECK (octet_length(target_recipient) > 0 AND octet_length(target_recipient) <= 512),
    parameters_json JSONB NOT NULL,
    parameters_sha256 TEXT NOT NULL CHECK (parameters_sha256 ~ '^[0-9a-f]{64}$'),
    status TEXT NOT NULL CHECK (status IN (
        'prepared',
        'dispatching',
        'confirmed',
        'denied',
        'failed',
        'unknown'
    )),
    provider_operation_id TEXT NULL CHECK (provider_operation_id IS NULL OR octet_length(provider_operation_id) <= 256),
    receipt_json JSONB NULL,
    error_summary TEXT NULL CHECK (error_summary IS NULL OR octet_length(error_summary) <= 2048),
    reconciled_at TIMESTAMPTZ NULL,
    correlation_id TEXT NOT NULL CHECK (octet_length(correlation_id) > 0 AND octet_length(correlation_id) <= 128),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT effect_claims_idem_uniq UNIQUE (effect_idempotency_key)
);

CREATE INDEX IF NOT EXISTS effect_claims_by_task_status
    ON effect_claims(task_id, status)
    WHERE task_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS inbox_cards (
    card_id UUID PRIMARY KEY,
    project_id UUID NOT NULL REFERENCES projects(project_id),
    session_id UUID NULL REFERENCES sessions(session_id),
    task_id UUID NULL REFERENCES tasks(task_id),
    run_id UUID NULL REFERENCES runs(run_id),
    agent_id UUID NULL REFERENCES agents(agent_id),
    kind TEXT NOT NULL CHECK (kind IN (
        'permission_approval',
        'clarification',
        'unknown_effect',
        'budget_exceeded',
        'missed_schedule'
    )),
    status TEXT NOT NULL CHECK (status IN ('open', 'resolved', 'expired', 'superseded', 'withdrawn')),
    urgency TEXT NOT NULL DEFAULT 'normal' CHECK (urgency IN ('low', 'normal', 'high', 'critical')),
    subject TEXT NOT NULL CHECK (octet_length(subject) > 0 AND octet_length(subject) <= 512),
    detail TEXT NOT NULL CHECK (octet_length(detail) > 0 AND octet_length(detail) <= 4096),
    permission_request_id UUID NULL REFERENCES permission_requests(request_id),
    effect_id UUID NULL REFERENCES effect_claims(effect_id),
    resolution_summary TEXT NULL CHECK (resolution_summary IS NULL OR octet_length(resolution_summary) <= 2048),
    revision BIGINT NOT NULL DEFAULT 1 CHECK (revision > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    resolved_at TIMESTAMPTZ NULL
);

CREATE INDEX IF NOT EXISTS inbox_cards_open_idx
    ON inbox_cards(status, created_at DESC)
    WHERE status = 'open';

CREATE TABLE IF NOT EXISTS agent_messages (
    agent_message_id UUID PRIMARY KEY,
    sender_agent_id UUID NULL REFERENCES agents(agent_id),
    recipient_agent_id UUID NOT NULL REFERENCES agents(agent_id),
    project_id UUID NOT NULL REFERENCES projects(project_id),
    related_task_id UUID NULL REFERENCES tasks(task_id),
    idempotency_key TEXT NOT NULL CHECK (octet_length(idempotency_key) > 0 AND octet_length(idempotency_key) <= 128),
    body_text TEXT NOT NULL CHECK (octet_length(body_text) > 0 AND octet_length(body_text) <= 65536),
    delivered_in_run_id UUID NULL REFERENCES runs(run_id),
    correlation_id TEXT NOT NULL CHECK (octet_length(correlation_id) > 0 AND octet_length(correlation_id) <= 128),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    delivered_at TIMESTAMPTZ NULL,
    CONSTRAINT agent_messages_recipient_idem_uniq UNIQUE (recipient_agent_id, idempotency_key)
);

CREATE INDEX IF NOT EXISTS agent_messages_undelivered_idx
    ON agent_messages(recipient_agent_id, project_id, created_at ASC)
    WHERE delivered_in_run_id IS NULL;

-- 8. Ordered Delivery Journal (`control_events`) & Per-Key Ordered Outbox (`ordered_outbox`) (V15-V18)
CREATE TABLE IF NOT EXISTS control_events (
    cursor_seq BIGINT PRIMARY KEY CHECK (cursor_seq > 0),
    event_id UUID NOT NULL UNIQUE,
    authority_epoch BIGINT NOT NULL DEFAULT 1 CHECK (authority_epoch > 0),
    entity_key TEXT NOT NULL CHECK (octet_length(entity_key) > 0 AND octet_length(entity_key) <= 160),
    entity_seq BIGINT NOT NULL CHECK (entity_seq > 0),
    event_type TEXT NOT NULL CHECK (octet_length(event_type) > 0 AND octet_length(event_type) <= 128),
    project_id UUID NULL REFERENCES projects(project_id),
    session_id UUID NULL REFERENCES sessions(session_id),
    task_id UUID NULL REFERENCES tasks(task_id),
    correlation_id TEXT NOT NULL CHECK (octet_length(correlation_id) > 0 AND octet_length(correlation_id) <= 128),
    payload_json JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT control_events_entity_seq_uniq UNIQUE (entity_key, entity_seq)
);

CREATE INDEX IF NOT EXISTS control_events_by_project_cursor
    ON control_events(project_id, cursor_seq ASC)
    WHERE project_id IS NOT NULL;

DROP TRIGGER IF EXISTS trg_control_events_append_only ON control_events;
CREATE TRIGGER trg_control_events_append_only
    BEFORE UPDATE OR DELETE ON control_events
    FOR EACH ROW
    EXECUTE FUNCTION dennett_control_reject_append_only_mutation();

CREATE TABLE IF NOT EXISTS ordered_outbox_heads (
    destination TEXT NOT NULL CHECK (octet_length(destination) > 0 AND octet_length(destination) <= 64),
    ordering_key TEXT NOT NULL CHECK (octet_length(ordering_key) > 0 AND octet_length(ordering_key) <= 160),
    next_deliver_seq BIGINT NOT NULL DEFAULT 1 CHECK (next_deliver_seq > 0),
    PRIMARY KEY (destination, ordering_key)
);

CREATE TABLE IF NOT EXISTS ordered_outbox (
    item_id UUID PRIMARY KEY,
    destination TEXT NOT NULL CHECK (octet_length(destination) > 0 AND octet_length(destination) <= 64),
    ordering_key TEXT NOT NULL CHECK (octet_length(ordering_key) > 0 AND octet_length(ordering_key) <= 160),
    key_seq BIGINT NOT NULL CHECK (key_seq > 0),
    cursor_seq BIGINT NOT NULL CHECK (cursor_seq > 0),
    idempotency_key TEXT NOT NULL CHECK (octet_length(idempotency_key) > 0 AND octet_length(idempotency_key) <= 128),
    payload_json JSONB NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'leased', 'delivered', 'parked')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    lease_owner TEXT NULL CHECK (lease_owner IS NULL OR octet_length(lease_owner) <= 128),
    lease_generation BIGINT NOT NULL DEFAULT 0 CHECK (lease_generation >= 0),
    lease_expires_at TIMESTAMPTZ NULL,
    last_error TEXT NULL CHECK (last_error IS NULL OR octet_length(last_error) <= 2048),
    correlation_id TEXT NOT NULL CHECK (octet_length(correlation_id) > 0 AND octet_length(correlation_id) <= 128),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT ordered_outbox_dest_key_seq_uniq UNIQUE (destination, ordering_key, key_seq),
    CONSTRAINT ordered_outbox_dest_idem_uniq UNIQUE (destination, idempotency_key)
);

CREATE INDEX IF NOT EXISTS ordered_outbox_claim_idx
    ON ordered_outbox(destination, ordering_key, key_seq)
    WHERE status IN ('pending', 'leased');
