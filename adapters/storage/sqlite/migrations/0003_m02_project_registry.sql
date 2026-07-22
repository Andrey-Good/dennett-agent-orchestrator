CREATE TABLE projects (
    project_id TEXT PRIMARY KEY NOT NULL,
    display_name TEXT NOT NULL,
    primary_binding_id TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0),
    created_at_unix_ms INTEGER NOT NULL CHECK (created_at_unix_ms >= 0),
    updated_at_unix_ms INTEGER NOT NULL CHECK (updated_at_unix_ms >= created_at_unix_ms)
);

CREATE TABLE project_access_policies (
    project_id TEXT PRIMARY KEY NOT NULL,
    trust_state TEXT NOT NULL CHECK (trust_state IN ('restricted', 'trusted_bounded', 'revoked')),
    revision INTEGER NOT NULL CHECK (revision > 0),
    last_decision_kind TEXT,
    last_decision_id TEXT,
    updated_at_unix_ms INTEGER NOT NULL CHECK (updated_at_unix_ms >= 0),
    CHECK ((last_decision_kind IS NULL) = (last_decision_id IS NULL)),
    FOREIGN KEY (project_id) REFERENCES projects(project_id)
);

CREATE TABLE workspace_bindings (
    binding_id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL,
    canonical_path TEXT NOT NULL,
    canonical_location_key BLOB NOT NULL CHECK (length(canonical_location_key) = 32),
    source_identity BLOB CHECK (source_identity IS NULL OR length(source_identity) = 32),
    workspace_kind TEXT NOT NULL CHECK (workspace_kind IN ('folder', 'versioned_checkout', 'isolated_checkout')),
    availability TEXT NOT NULL CHECK (availability IN ('available', 'missing', 'inaccessible', 'detached')),
    access_mode TEXT NOT NULL CHECK (access_mode IN ('read_only', 'read_write')),
    portable_metadata_state TEXT NOT NULL CHECK (portable_metadata_state IN ('absent', 'present_valid', 'invalid', 'identity_conflict', 'unsupported_version')),
    portable_project_id TEXT,
    is_primary INTEGER NOT NULL CHECK (is_primary IN (0, 1)),
    record_revision INTEGER NOT NULL CHECK (record_revision > 0),
    created_at_unix_ms INTEGER NOT NULL CHECK (created_at_unix_ms >= 0),
    last_verified_at_unix_ms INTEGER NOT NULL CHECK (last_verified_at_unix_ms >= 0),
    UNIQUE (binding_id, project_id),
    FOREIGN KEY (project_id) REFERENCES projects(project_id)
);

CREATE INDEX workspace_bindings_by_project
    ON workspace_bindings(project_id, is_primary DESC, created_at_unix_ms);

CREATE TABLE project_location_inspections (
    inspection_id TEXT PRIMARY KEY NOT NULL,
    registration_kind TEXT NOT NULL CHECK (registration_kind IN ('create_empty', 'attach_existing')),
    canonical_path TEXT NOT NULL,
    canonical_location_key BLOB NOT NULL CHECK (length(canonical_location_key) = 32),
    suggested_display_name TEXT NOT NULL,
    location_exists INTEGER NOT NULL CHECK (location_exists IN (0, 1)),
    location_empty INTEGER NOT NULL CHECK (location_empty IN (0, 1)),
    source_identity BLOB CHECK (source_identity IS NULL OR length(source_identity) = 32),
    prospective_parent_identity BLOB CHECK (prospective_parent_identity IS NULL OR length(prospective_parent_identity) = 32),
    workspace_kind TEXT NOT NULL CHECK (workspace_kind IN ('folder', 'versioned_checkout', 'isolated_checkout')),
    availability TEXT NOT NULL CHECK (availability IN ('available', 'missing', 'inaccessible', 'detached')),
    access_mode TEXT NOT NULL CHECK (access_mode IN ('read_only', 'read_write')),
    portable_metadata_state TEXT NOT NULL CHECK (portable_metadata_state IN ('absent', 'present_valid', 'invalid', 'identity_conflict', 'unsupported_version')),
    portable_project_id TEXT,
    shared_memory_state TEXT NOT NULL CHECK (shared_memory_state IN ('absent', 'present', 'invalid')),
    minimal_structure_creation_available INTEGER NOT NULL CHECK (minimal_structure_creation_available IN (0, 1)),
    instruction_fingerprint BLOB CHECK (instruction_fingerprint IS NULL OR length(instruction_fingerprint) = 32),
    instruction_source_count INTEGER NOT NULL CHECK (instruction_source_count >= 0),
    instruction_discovery_incomplete INTEGER NOT NULL CHECK (instruction_discovery_incomplete IN (0, 1)),
    observed_at_unix_ms INTEGER NOT NULL CHECK (observed_at_unix_ms >= 0),
    expires_at_unix_ms INTEGER NOT NULL CHECK (expires_at_unix_ms > observed_at_unix_ms),
    CHECK (
        (location_exists = 1 AND source_identity IS NOT NULL AND prospective_parent_identity IS NULL)
        OR
        (location_exists = 0 AND source_identity IS NULL)
    ),
    CHECK (prospective_parent_identity IS NULL OR registration_kind = 'create_empty')
);

CREATE INDEX project_location_inspections_by_expiry
    ON project_location_inspections(expires_at_unix_ms);

CREATE TABLE project_registration_operations (
    operation_id TEXT PRIMARY KEY NOT NULL,
    command_id TEXT UNIQUE NOT NULL,
    correlation_id TEXT NOT NULL,
    intent_sha256 BLOB NOT NULL CHECK (length(intent_sha256) = 32),
    inspection_id TEXT NOT NULL,
    target_kind TEXT NOT NULL CHECK (target_kind IN ('new_project', 'existing_project')),
    project_id TEXT NOT NULL,
    binding_id TEXT NOT NULL,
    direct_session_id TEXT NOT NULL,
    display_name TEXT NOT NULL,
    canonical_path TEXT NOT NULL,
    canonical_location_key BLOB NOT NULL CHECK (length(canonical_location_key) = 32),
    source_identity BLOB CHECK (source_identity IS NULL OR length(source_identity) = 32),
    workspace_kind TEXT NOT NULL CHECK (workspace_kind IN ('folder', 'versioned_checkout', 'isolated_checkout')),
    availability TEXT NOT NULL CHECK (availability IN ('available', 'missing', 'inaccessible', 'detached')),
    access_mode TEXT NOT NULL CHECK (access_mode IN ('read_only', 'read_write')),
    portable_metadata_state TEXT NOT NULL CHECK (portable_metadata_state IN ('absent', 'present_valid', 'invalid', 'identity_conflict', 'unsupported_version')),
    portable_project_id TEXT,
    portable_metadata_action TEXT NOT NULL CHECK (portable_metadata_action IN ('leave_absent', 'use_existing', 'create_minimal', 'fork_with_new_identity')),
    instruction_fingerprint BLOB CHECK (instruction_fingerprint IS NULL OR length(instruction_fingerprint) = 32),
    instruction_source_count INTEGER NOT NULL CHECK (instruction_source_count >= 0),
    initial_trust_state TEXT NOT NULL CHECK (initial_trust_state IN ('restricted', 'trusted_bounded')),
    initial_decision_kind TEXT,
    initial_decision_id TEXT,
    final_source_identity BLOB CHECK (final_source_identity IS NULL OR length(final_source_identity) = 32),
    final_workspace_kind TEXT CHECK (final_workspace_kind IS NULL OR final_workspace_kind IN ('folder', 'versioned_checkout', 'isolated_checkout')),
    final_availability TEXT CHECK (final_availability IS NULL OR final_availability IN ('available', 'missing', 'inaccessible', 'detached')),
    final_access_mode TEXT CHECK (final_access_mode IS NULL OR final_access_mode IN ('read_only', 'read_write')),
    final_portable_metadata_state TEXT CHECK (final_portable_metadata_state IS NULL OR final_portable_metadata_state IN ('absent', 'present_valid', 'invalid', 'identity_conflict', 'unsupported_version')),
    final_portable_project_id TEXT,
    final_instruction_fingerprint BLOB CHECK (final_instruction_fingerprint IS NULL OR length(final_instruction_fingerprint) = 32),
    final_instruction_source_count INTEGER CHECK (final_instruction_source_count IS NULL OR final_instruction_source_count >= 0),
    final_observed_at_unix_ms INTEGER CHECK (final_observed_at_unix_ms IS NULL OR final_observed_at_unix_ms >= 0),
    state TEXT NOT NULL CHECK (state IN ('prepared', 'filesystem_applied', 'committed', 'recovery_required')),
    safe_code TEXT NOT NULL,
    created_at_unix_ms INTEGER NOT NULL CHECK (created_at_unix_ms >= 0),
    updated_at_unix_ms INTEGER NOT NULL CHECK (updated_at_unix_ms >= created_at_unix_ms),
    CHECK ((initial_decision_kind IS NULL) = (initial_decision_id IS NULL)),
    CHECK (initial_trust_state = 'restricted' OR initial_decision_kind IS NOT NULL),
    CHECK (
        (final_observed_at_unix_ms IS NULL AND final_workspace_kind IS NULL AND final_availability IS NULL AND final_access_mode IS NULL AND final_portable_metadata_state IS NULL AND final_instruction_source_count IS NULL)
        OR
        (final_observed_at_unix_ms IS NOT NULL AND final_workspace_kind IS NOT NULL AND final_availability IS NOT NULL AND final_access_mode IS NOT NULL AND final_portable_metadata_state IS NOT NULL AND final_instruction_source_count IS NOT NULL)
    ),
    FOREIGN KEY (inspection_id) REFERENCES project_location_inspections(inspection_id)
);

CREATE INDEX project_registration_operations_by_project
    ON project_registration_operations(project_id, created_at_unix_ms);

-- One claim reserves a canonical local location either for an unfinished
-- registration or for its committed binding. Paths remain local-sensitive;
-- callers and events use only the opaque key.
CREATE TABLE workspace_location_claims (
    canonical_location_key BLOB PRIMARY KEY NOT NULL CHECK (length(canonical_location_key) = 32),
    canonical_path TEXT NOT NULL,
    operation_id TEXT UNIQUE,
    binding_id TEXT UNIQUE,
    CHECK ((operation_id IS NULL) <> (binding_id IS NULL))
);

CREATE TABLE instruction_fingerprints (
    binding_id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL,
    fingerprint_sha256 BLOB NOT NULL CHECK (length(fingerprint_sha256) = 32),
    source_count INTEGER NOT NULL CHECK (source_count >= 0),
    revision INTEGER NOT NULL CHECK (revision > 0),
    observed_at_unix_ms INTEGER NOT NULL CHECK (observed_at_unix_ms >= 0),
    FOREIGN KEY (binding_id, project_id) REFERENCES workspace_bindings(binding_id, project_id)
);

CREATE TABLE project_lifecycle_events (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    event_kind TEXT NOT NULL CHECK (event_kind IN (
        'legacy_project_imported',
        'registration_prepared',
        'registration_filesystem_applied',
        'registration_committed',
        'registration_recovery_required',
        'trust_changed',
        'binding_observation_updated',
        'instruction_fingerprint_changed',
        'workspace_rebound'
    )),
    project_id TEXT NOT NULL,
    command_id TEXT,
    correlation_id TEXT NOT NULL,
    safe_code TEXT NOT NULL,
    project_revision INTEGER CHECK (project_revision IS NULL OR project_revision > 0),
    policy_revision INTEGER CHECK (policy_revision IS NULL OR policy_revision > 0),
    binding_revision INTEGER CHECK (binding_revision IS NULL OR binding_revision > 0),
    occurred_at_unix_ms INTEGER NOT NULL CHECK (occurred_at_unix_ms >= 0)
);

CREATE INDEX project_lifecycle_events_by_project
    ON project_lifecycle_events(project_id, sequence);

CREATE TABLE project_rebind_receipts (
    command_id TEXT PRIMARY KEY NOT NULL,
    correlation_id TEXT NOT NULL,
    intent_sha256 BLOB NOT NULL CHECK (length(intent_sha256) = 32),
    project_id TEXT NOT NULL,
    previous_binding_id TEXT NOT NULL,
    primary_binding_id TEXT NOT NULL,
    project_revision INTEGER NOT NULL CHECK (project_revision > 0),
    rebound_at_unix_ms INTEGER NOT NULL CHECK (rebound_at_unix_ms >= 0),
    FOREIGN KEY (project_id) REFERENCES projects(project_id),
    FOREIGN KEY (primary_binding_id, project_id) REFERENCES workspace_bindings(binding_id, project_id)
);

PRAGMA user_version = 3;
