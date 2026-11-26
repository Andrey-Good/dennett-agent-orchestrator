-- D3+ Live Agent Polish: run context token tracking & durable session follow-up queue (V03).

ALTER TABLE runs
    ADD COLUMN context_used_tokens BIGINT NULL,
    ADD COLUMN context_window_tokens BIGINT NULL;

CREATE TABLE session_followups (
    followup_id UUID PRIMARY KEY,
    session_id UUID NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('queued', 'paused', 'dispatched', 'cancelled')),
    position BIGINT NOT NULL,
    idempotency_key TEXT NULL,
    dispatched_run_id UUID NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_followups_session_status
    ON session_followups(session_id, status, position);

CREATE UNIQUE INDEX idx_followups_idempotency
    ON session_followups(session_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;
