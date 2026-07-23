CREATE TABLE workspace_snapshots (
    snapshot_id TEXT PRIMARY KEY NOT NULL,
    binding_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    scope_sha256 BLOB NOT NULL CHECK (length(scope_sha256) = 32),
    manifest_complete INTEGER NOT NULL CHECK (manifest_complete IN (0, 1)),
    record_json TEXT NOT NULL,
    record_sha256 BLOB NOT NULL CHECK (length(record_sha256) = 32),
    observed_at_unix_ms INTEGER NOT NULL CHECK (observed_at_unix_ms >= 0),
    UNIQUE (binding_id, sequence),
    UNIQUE (binding_id, snapshot_id, sequence),
    FOREIGN KEY (binding_id, project_id) REFERENCES workspace_bindings(binding_id, project_id)
);

CREATE INDEX workspace_snapshots_by_project
    ON workspace_snapshots(project_id, binding_id, sequence);

CREATE TABLE workspace_snapshot_heads (
    binding_id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL,
    snapshot_id TEXT NOT NULL,
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    FOREIGN KEY (binding_id, project_id) REFERENCES workspace_bindings(binding_id, project_id),
    FOREIGN KEY (binding_id, snapshot_id, sequence)
        REFERENCES workspace_snapshots(binding_id, snapshot_id, sequence)
);

CREATE TABLE workspace_checkpoints (
    checkpoint_id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL,
    binding_id TEXT NOT NULL,
    base_snapshot_id TEXT NOT NULL,
    base_sequence INTEGER NOT NULL CHECK (base_sequence > 0),
    captured_snapshot_id TEXT NOT NULL,
    captured_sequence INTEGER NOT NULL CHECK (captured_sequence > 0),
    state TEXT NOT NULL CHECK (state IN ('available', 'restored', 'recovery_required')),
    record_json TEXT NOT NULL,
    record_sha256 BLOB NOT NULL CHECK (length(record_sha256) = 32),
    created_at_unix_ms INTEGER NOT NULL CHECK (created_at_unix_ms >= 0),
    FOREIGN KEY (binding_id, project_id) REFERENCES workspace_bindings(binding_id, project_id),
    FOREIGN KEY (binding_id, base_snapshot_id, base_sequence)
        REFERENCES workspace_snapshots(binding_id, snapshot_id, sequence),
    FOREIGN KEY (binding_id, captured_snapshot_id, captured_sequence)
        REFERENCES workspace_snapshots(binding_id, snapshot_id, sequence)
);

CREATE INDEX workspace_checkpoints_by_project
    ON workspace_checkpoints(project_id, binding_id, created_at_unix_ms);

CREATE TABLE workspace_operations (
    operation_id TEXT PRIMARY KEY NOT NULL,
    command_id TEXT UNIQUE NOT NULL,
    project_id TEXT NOT NULL,
    binding_id TEXT NOT NULL,
    base_snapshot_id TEXT NOT NULL,
    base_sequence INTEGER NOT NULL CHECK (base_sequence > 0),
    safety_checkpoint_id TEXT UNIQUE NOT NULL,
    state TEXT NOT NULL CHECK (state IN (
        'prepared', 'filesystem_applied', 'succeeded', 'failed', 'recovery_required'
    )),
    intent_sha256 BLOB NOT NULL CHECK (length(intent_sha256) = 32),
    resulting_snapshot_id TEXT,
    resulting_sequence INTEGER CHECK (resulting_sequence IS NULL OR resulting_sequence > 0),
    failure_kind TEXT CHECK (failure_kind IS NULL OR failure_kind IN (
        'conflict', 'scope_denied', 'adapter_failure', 'recovery_required'
    )),
    failure_safe_code TEXT,
    record_json TEXT NOT NULL,
    record_sha256 BLOB NOT NULL CHECK (length(record_sha256) = 32),
    prepared_at_unix_ms INTEGER NOT NULL CHECK (prepared_at_unix_ms >= 0),
    completed_at_unix_ms INTEGER CHECK (completed_at_unix_ms IS NULL OR completed_at_unix_ms >= prepared_at_unix_ms),
    CHECK ((resulting_snapshot_id IS NULL) = (resulting_sequence IS NULL)),
    CHECK ((failure_kind IS NULL) = (failure_safe_code IS NULL)),
    CHECK (
        (state = 'succeeded' AND resulting_snapshot_id IS NOT NULL AND failure_kind IS NULL AND completed_at_unix_ms IS NOT NULL)
        OR
        (state IN ('failed', 'recovery_required') AND resulting_snapshot_id IS NULL AND failure_kind IS NOT NULL AND completed_at_unix_ms IS NOT NULL)
        OR
        (state IN ('prepared', 'filesystem_applied') AND resulting_snapshot_id IS NULL AND failure_kind IS NULL AND completed_at_unix_ms IS NULL)
    ),
    FOREIGN KEY (binding_id, project_id) REFERENCES workspace_bindings(binding_id, project_id),
    FOREIGN KEY (binding_id, base_snapshot_id, base_sequence)
        REFERENCES workspace_snapshots(binding_id, snapshot_id, sequence),
    FOREIGN KEY (safety_checkpoint_id) REFERENCES workspace_checkpoints(checkpoint_id),
    FOREIGN KEY (binding_id, resulting_snapshot_id, resulting_sequence)
        REFERENCES workspace_snapshots(binding_id, snapshot_id, sequence)
);

CREATE INDEX workspace_operations_unfinished
    ON workspace_operations(state, prepared_at_unix_ms, operation_id);

CREATE TABLE workspace_blob_data (
    content_sha256 BLOB PRIMARY KEY NOT NULL CHECK (length(content_sha256) = 32),
    byte_size INTEGER NOT NULL CHECK (byte_size >= 0 AND byte_size <= 16777216),
    bytes BLOB NOT NULL,
    UNIQUE (content_sha256, byte_size),
    CHECK (length(bytes) = byte_size)
);

CREATE TABLE workspace_operation_blobs (
    operation_id TEXT NOT NULL,
    content_id TEXT NOT NULL CHECK (length(content_id) BETWEEN 1 AND 256),
    content_sha256 BLOB NOT NULL CHECK (length(content_sha256) = 32),
    byte_size INTEGER NOT NULL CHECK (byte_size >= 0 AND byte_size <= 16777216),
    PRIMARY KEY (operation_id, content_id),
    FOREIGN KEY (operation_id) REFERENCES workspace_operations(operation_id),
    FOREIGN KEY (content_sha256, byte_size)
        REFERENCES workspace_blob_data(content_sha256, byte_size)
);

CREATE TABLE workspace_checkpoint_blobs (
    checkpoint_id TEXT NOT NULL,
    content_id TEXT NOT NULL CHECK (length(content_id) BETWEEN 1 AND 256),
    content_sha256 BLOB NOT NULL CHECK (length(content_sha256) = 32),
    byte_size INTEGER NOT NULL CHECK (byte_size >= 0 AND byte_size <= 16777216),
    PRIMARY KEY (checkpoint_id, content_id),
    FOREIGN KEY (checkpoint_id) REFERENCES workspace_checkpoints(checkpoint_id),
    FOREIGN KEY (content_sha256, byte_size)
        REFERENCES workspace_blob_data(content_sha256, byte_size)
);

PRAGMA user_version = 4;
