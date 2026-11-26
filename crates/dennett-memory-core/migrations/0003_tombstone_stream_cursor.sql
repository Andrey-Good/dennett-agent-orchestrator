-- Dennett Memory Fabric Schema v3:
-- Monotonic `tombstone_seq` identity sequence and V15 commit-visibility `commit_xid` barrier
-- on `memory_forget_tombstones` so downstream consumers (`dennett-head` outbox scrubber) can
-- stream forget tombstones monotonically without wall-clock skew or skipped-transaction holes.

ALTER TABLE memory_forget_tombstones
    ADD COLUMN IF NOT EXISTS tombstone_seq BIGINT GENERATED ALWAYS AS IDENTITY NOT NULL,
    ADD COLUMN IF NOT EXISTS commit_xid XID8 NOT NULL DEFAULT pg_current_xact_id();

CREATE UNIQUE INDEX IF NOT EXISTS idx_tombstones_seq
    ON memory_forget_tombstones(tombstone_seq);
