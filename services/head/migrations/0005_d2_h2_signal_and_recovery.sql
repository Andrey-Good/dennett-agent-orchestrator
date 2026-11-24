-- Forward-only migration v5 (H2 hardening):
-- Persist structured `expected_signal_name` on durable `tasks` so `waiting_signal`
-- only wakes on a matching signal name (H2-04) and survives goal edits cleanly.

ALTER TABLE tasks
    ADD COLUMN IF NOT EXISTS expected_signal_name TEXT NULL
        CHECK (
            expected_signal_name IS NULL
            OR (octet_length(expected_signal_name) > 0 AND octet_length(expected_signal_name) <= 128)
        );
