-- Dennett Head Control Plane Schema v6 (D2 Hardening Pass & Agent-Native Events).
--
-- Purely additive / forward-compatible migration over schema v5:
--   1. `sessions.session_kind` (`'user_chat' | 'task_run'`) separating interactive user chats from
--      dedicated task execution sessions (D2-UX-01 / Agent-Native Events).
--   2. `task_timers.pause_reason` (`'archived_project' | 'misfire_ask'`) & `weekdays_mask`
--      (`'weekly_wall'` recurrence support; D2-ARCH-02 / Agent-Native Events).
--   3. `tasks.pending_continuation_input` & `task_signals.consumed_in_generation` for durable
--      crash-safe reply and signal delivery across Head restarts (D2-BUG-01).
--   4. `permission_requests.preview_snippet` for secret-redacted write-preview inspection (D2-UX-05).
--   5. Forward consolidation of `memory_commit_outbox` into `ordered_outbox` (`destination = 'memoryd'`,
--      `ordering_key = scope`) with per-(destination, ordering_key, idempotency_key) uniqueness (D2-OVER-01).

-- ============================================================================
-- 1. Sessions: explicit `session_kind` (`user_chat` vs `task_run`)
-- ============================================================================

ALTER TABLE sessions
    ADD COLUMN IF NOT EXISTS session_kind TEXT NOT NULL DEFAULT 'user_chat'
        CHECK (session_kind IN ('user_chat', 'task_run'));

-- Backfill existing dedicated task-only sessions that have never hosted a non-task user chat run.
UPDATE sessions s
SET session_kind = 'task_run'
WHERE s.session_kind = 'user_chat'
  AND EXISTS (SELECT 1 FROM tasks t WHERE t.session_id = s.session_id)
  AND NOT EXISTS (
      SELECT 1 FROM runs r
      WHERE r.session_id = s.session_id AND r.task_id IS NULL
  );

CREATE INDEX IF NOT EXISTS idx_sessions_project_kind_updated
    ON sessions (project_id, session_kind, updated_at DESC);

-- ============================================================================
-- 2. Task Timers: `pause_reason`, `weekdays_mask`, and `weekly_wall` recurrence
-- ============================================================================

ALTER TABLE task_timers
    ADD COLUMN IF NOT EXISTS pause_reason TEXT NULL
        CHECK (pause_reason IS NULL OR pause_reason IN ('archived_project', 'misfire_ask')),
    ADD COLUMN IF NOT EXISTS weekdays_mask SMALLINT NULL
        CHECK (weekdays_mask IS NULL OR weekdays_mask BETWEEN 1 AND 127);

ALTER TABLE task_timers DROP CONSTRAINT IF EXISTS task_timers_recurrence_kind_check;
ALTER TABLE task_timers ADD CONSTRAINT task_timers_recurrence_kind_check
    CHECK (recurrence_kind IN ('once', 'interval', 'daily_wall', 'weekly_wall'));

-- Backfill pause_reason for any pre-existing paused timers.
UPDATE task_timers tt
SET pause_reason = 'archived_project'
FROM tasks t
JOIN projects p ON p.project_id = t.project_id
WHERE tt.task_id = t.task_id
  AND tt.status = 'paused'
  AND tt.pause_reason IS NULL
  AND p.lifecycle_state = 'archived';

UPDATE task_timers tt
SET pause_reason = 'misfire_ask'
WHERE tt.status = 'paused'
  AND tt.pause_reason IS NULL
  AND EXISTS (
      SELECT 1 FROM inbox_cards c
      WHERE c.task_id = tt.task_id
        AND c.kind = 'missed_schedule'
        AND c.status = 'open'
  );

-- ============================================================================
-- 3. Durable Task Continuation Input & Signal Consumption Tracking (D2-BUG-01)
-- ============================================================================

ALTER TABLE tasks
    ADD COLUMN IF NOT EXISTS pending_continuation_input TEXT NULL;

ALTER TABLE task_signals
    ADD COLUMN IF NOT EXISTS consumed_in_generation BIGINT NULL;

-- ============================================================================
-- 4. Permission Request Secret-Redacted Preview Snippet (D2-UX-05)
-- ============================================================================

ALTER TABLE permission_requests
    ADD COLUMN IF NOT EXISTS preview_snippet TEXT NULL;

-- ============================================================================
-- 5. Unify `memory_commit_outbox` onto `ordered_outbox` (D2-OVER-01)
-- ============================================================================

-- Scope idempotency uniqueness per `(destination, ordering_key, idempotency_key)` so distinct
-- memory scopes can reuse the same caller idempotency key identically to `memory_commit_outbox`.
ALTER TABLE ordered_outbox DROP CONSTRAINT IF EXISTS ordered_outbox_dest_idem_uniq;
ALTER TABLE ordered_outbox
    ADD CONSTRAINT ordered_outbox_dest_idem_uniq
        UNIQUE (destination, ordering_key, idempotency_key);

-- Forward-migrate any existing `memory_commit_outbox` rows into `ordered_outbox` (`destination = 'memoryd'`).
WITH base_seq AS (
    SELECT ordering_key, COALESCE(MAX(key_seq), 0) AS max_seq
    FROM ordered_outbox
    WHERE destination = 'memoryd'
    GROUP BY ordering_key
),
migrated AS (
    SELECT
        m.outbox_id,
        m.scope AS ordering_key,
        COALESCE(b.max_seq, 0)
            + ROW_NUMBER() OVER (PARTITION BY m.scope ORDER BY m.created_at ASC, m.outbox_id ASC) AS key_seq,
        GREATEST(
            1,
            COALESCE(
                (SELECT committed_seq FROM control_stream_counters WHERE stream_id = 'control:journal'),
                1
            )
        ) AS cursor_seq,
        m.idempotency_key,
        jsonb_build_object(
            'scope', m.scope,
            'idempotency_key', m.idempotency_key,
            'operation', m.operation,
            'session_id', m.session_id,
            'run_id', m.run_id,
            'note_request', m.payload_json::jsonb,
            'delivered_event_id', m.delivered_event_id,
            'delivered_note_id', m.delivered_note_id
        ) AS payload_jsonb,
        CASE m.status
            WHEN 'delivered' THEN 'delivered'
            WHEN 'failed' THEN 'parked'
            ELSE 'pending'
        END AS mapped_status,
        m.attempts,
        m.last_error,
        m.correlation_id,
        m.created_at,
        m.updated_at
    FROM memory_commit_outbox m
    LEFT JOIN base_seq b ON b.ordering_key = m.scope
)
INSERT INTO ordered_outbox (
    item_id, destination, ordering_key, key_seq, cursor_seq,
    idempotency_key, payload_json, status, attempts, available_at,
    last_error, correlation_id, created_at, updated_at
)
SELECT
    outbox_id, 'memoryd', ordering_key, key_seq, cursor_seq,
    idempotency_key, payload_jsonb, mapped_status, attempts, created_at,
    last_error, correlation_id, created_at, updated_at
FROM migrated
ON CONFLICT (destination, ordering_key, idempotency_key) DO NOTHING;

-- Synchronize `ordered_outbox_heads` and `control_stream_counters` for migrated `memoryd` rows.
INSERT INTO ordered_outbox_heads (destination, ordering_key, next_deliver_seq)
SELECT
    'memoryd',
    ordering_key,
    COALESCE(MIN(key_seq) FILTER (WHERE status <> 'delivered'), MAX(key_seq) + 1)
FROM ordered_outbox
WHERE destination = 'memoryd'
GROUP BY ordering_key
ON CONFLICT (destination, ordering_key) DO UPDATE
SET next_deliver_seq = LEAST(ordered_outbox_heads.next_deliver_seq, EXCLUDED.next_deliver_seq);

INSERT INTO control_stream_counters (stream_id, committed_seq)
SELECT
    'outbox:memoryd:' || ordering_key,
    MAX(key_seq)
FROM ordered_outbox
WHERE destination = 'memoryd'
GROUP BY ordering_key
ON CONFLICT (stream_id) DO UPDATE
SET committed_seq = GREATEST(control_stream_counters.committed_seq, EXCLUDED.committed_seq);
