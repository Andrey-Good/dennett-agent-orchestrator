use super::SqliteControlStore;
use async_trait::async_trait;
use dennett_contracts::{
    CommandId, PortableMetadataAction, PortableProjectMetadataState, ProjectId,
    ProjectInspectionId, ProjectTrustState, SessionId, WorkspaceBindingId, WorkspaceOperationId,
};
use dennett_trust_core::project_registry::{
    BindingObservationUpdate, CanonicalLocationKey, CanonicalWorkspaceLocation,
    InstructionFingerprint, InstructionFingerprintUpdate, LegacyProjectImport, ProjectAccessPolicy,
    ProjectAccessPolicyUpdate, ProjectAggregate, ProjectLifecycleEvent, ProjectLifecycleEventKind,
    ProjectLocationInspection, ProjectRecord, ProjectRegistrationCommit, ProjectRegistrationKind,
    ProjectRegistrationOperation, ProjectRegistrationPlan, ProjectRegistrationTarget,
    ProjectRegistryError, ProjectRegistryPort, ProjectWorkspaceRebindPlan,
    ProjectWorkspaceRebindReceipt, RegistrationFilesystemApplied,
    RegistrationFilesystemObservation, RegistrationOperationState, RegistrationStateUpdate,
    SensitiveAbsolutePath, SharedProjectMemoryState, TrustDecisionRef, WorkspaceAccessMode,
    WorkspaceAvailability, WorkspaceBinding, WorkspaceKind, WorkspaceSourceIdentity,
    verify_bridge_attested_project_trust_decision,
};
use sqlx::{Row, Sqlite, Transaction, sqlite::SqliteRow};
use uuid::Uuid;

fn storage_error(_: sqlx::Error) -> ProjectRegistryError {
    ProjectRegistryError::StorageUnavailable
}

fn to_i64(value: u64) -> Result<i64, ProjectRegistryError> {
    i64::try_from(value).map_err(|_| ProjectRegistryError::InvalidInput("integer overflow"))
}

fn from_i64(value: i64, field: &'static str) -> Result<u64, ProjectRegistryError> {
    u64::try_from(value).map_err(|_| ProjectRegistryError::IntegrityFailure(field))
}

fn from_i64_u32(value: i64, field: &'static str) -> Result<u32, ProjectRegistryError> {
    u32::try_from(value).map_err(|_| ProjectRegistryError::IntegrityFailure(field))
}

fn parse_uuid(value: &str, field: &'static str) -> Result<Uuid, ProjectRegistryError> {
    Uuid::parse_str(value).map_err(|_| ProjectRegistryError::IntegrityFailure(field))
}

fn blob32(value: Vec<u8>, field: &'static str) -> Result<[u8; 32], ProjectRegistryError> {
    value
        .try_into()
        .map_err(|_| ProjectRegistryError::IntegrityFailure(field))
}

fn optional_blob32(
    value: Option<Vec<u8>>,
    field: &'static str,
) -> Result<Option<[u8; 32]>, ProjectRegistryError> {
    value.map(|value| blob32(value, field)).transpose()
}

fn parse_bool(value: i64, field: &'static str) -> Result<bool, ProjectRegistryError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(ProjectRegistryError::IntegrityFailure(field)),
    }
}

fn bool_i64(value: bool) -> i64 {
    if value { 1 } else { 0 }
}

fn workspace_kind_db(value: WorkspaceKind) -> &'static str {
    match value {
        WorkspaceKind::Folder => "folder",
        WorkspaceKind::VersionedCheckout => "versioned_checkout",
        WorkspaceKind::IsolatedCheckout => "isolated_checkout",
    }
}

fn parse_workspace_kind(value: &str) -> Result<WorkspaceKind, ProjectRegistryError> {
    match value {
        "folder" => Ok(WorkspaceKind::Folder),
        "versioned_checkout" => Ok(WorkspaceKind::VersionedCheckout),
        "isolated_checkout" => Ok(WorkspaceKind::IsolatedCheckout),
        _ => Err(ProjectRegistryError::IntegrityFailure(
            "invalid workspace kind",
        )),
    }
}

fn availability_db(value: WorkspaceAvailability) -> &'static str {
    match value {
        WorkspaceAvailability::Available => "available",
        WorkspaceAvailability::Missing => "missing",
        WorkspaceAvailability::Inaccessible => "inaccessible",
        WorkspaceAvailability::Detached => "detached",
    }
}

fn parse_availability(value: &str) -> Result<WorkspaceAvailability, ProjectRegistryError> {
    match value {
        "available" => Ok(WorkspaceAvailability::Available),
        "missing" => Ok(WorkspaceAvailability::Missing),
        "inaccessible" => Ok(WorkspaceAvailability::Inaccessible),
        "detached" => Ok(WorkspaceAvailability::Detached),
        _ => Err(ProjectRegistryError::IntegrityFailure(
            "invalid workspace availability",
        )),
    }
}

fn access_mode_db(value: WorkspaceAccessMode) -> &'static str {
    match value {
        WorkspaceAccessMode::ReadOnly => "read_only",
        WorkspaceAccessMode::ReadWrite => "read_write",
    }
}

fn parse_access_mode(value: &str) -> Result<WorkspaceAccessMode, ProjectRegistryError> {
    match value {
        "read_only" => Ok(WorkspaceAccessMode::ReadOnly),
        "read_write" => Ok(WorkspaceAccessMode::ReadWrite),
        _ => Err(ProjectRegistryError::IntegrityFailure(
            "invalid workspace access mode",
        )),
    }
}

fn trust_state_db(value: ProjectTrustState) -> &'static str {
    match value {
        ProjectTrustState::Restricted => "restricted",
        ProjectTrustState::TrustedBounded => "trusted_bounded",
        ProjectTrustState::Revoked => "revoked",
    }
}

fn parse_trust_state(value: &str) -> Result<ProjectTrustState, ProjectRegistryError> {
    match value {
        "restricted" => Ok(ProjectTrustState::Restricted),
        "trusted_bounded" => Ok(ProjectTrustState::TrustedBounded),
        "revoked" => Ok(ProjectTrustState::Revoked),
        _ => Err(ProjectRegistryError::IntegrityFailure(
            "invalid project trust state",
        )),
    }
}

fn portable_state_db(value: PortableProjectMetadataState) -> &'static str {
    match value {
        PortableProjectMetadataState::Absent => "absent",
        PortableProjectMetadataState::PresentValid => "present_valid",
        PortableProjectMetadataState::Invalid => "invalid",
        PortableProjectMetadataState::IdentityConflict => "identity_conflict",
        PortableProjectMetadataState::UnsupportedVersion => "unsupported_version",
    }
}

fn parse_portable_state(value: &str) -> Result<PortableProjectMetadataState, ProjectRegistryError> {
    match value {
        "absent" => Ok(PortableProjectMetadataState::Absent),
        "present_valid" => Ok(PortableProjectMetadataState::PresentValid),
        "invalid" => Ok(PortableProjectMetadataState::Invalid),
        "identity_conflict" => Ok(PortableProjectMetadataState::IdentityConflict),
        "unsupported_version" => Ok(PortableProjectMetadataState::UnsupportedVersion),
        _ => Err(ProjectRegistryError::IntegrityFailure(
            "invalid portable metadata state",
        )),
    }
}

fn portable_action_db(value: PortableMetadataAction) -> &'static str {
    match value {
        PortableMetadataAction::LeaveAbsent => "leave_absent",
        PortableMetadataAction::UseExisting => "use_existing",
        PortableMetadataAction::CreateMinimal => "create_minimal",
        PortableMetadataAction::ForkWithNewIdentity => "fork_with_new_identity",
    }
}

fn parse_portable_action(value: &str) -> Result<PortableMetadataAction, ProjectRegistryError> {
    match value {
        "leave_absent" => Ok(PortableMetadataAction::LeaveAbsent),
        "use_existing" => Ok(PortableMetadataAction::UseExisting),
        "create_minimal" => Ok(PortableMetadataAction::CreateMinimal),
        "fork_with_new_identity" => Ok(PortableMetadataAction::ForkWithNewIdentity),
        _ => Err(ProjectRegistryError::IntegrityFailure(
            "invalid portable metadata action",
        )),
    }
}

fn shared_memory_state_db(value: SharedProjectMemoryState) -> &'static str {
    match value {
        SharedProjectMemoryState::Absent => "absent",
        SharedProjectMemoryState::Present => "present",
        SharedProjectMemoryState::Invalid => "invalid",
    }
}

fn parse_shared_memory_state(
    value: &str,
) -> Result<SharedProjectMemoryState, ProjectRegistryError> {
    match value {
        "absent" => Ok(SharedProjectMemoryState::Absent),
        "present" => Ok(SharedProjectMemoryState::Present),
        "invalid" => Ok(SharedProjectMemoryState::Invalid),
        _ => Err(ProjectRegistryError::IntegrityFailure(
            "invalid shared project memory state",
        )),
    }
}

fn registration_kind_db(value: ProjectRegistrationKind) -> &'static str {
    match value {
        ProjectRegistrationKind::CreateEmpty => "create_empty",
        ProjectRegistrationKind::AttachExisting => "attach_existing",
    }
}

fn parse_registration_kind(value: &str) -> Result<ProjectRegistrationKind, ProjectRegistryError> {
    match value {
        "create_empty" => Ok(ProjectRegistrationKind::CreateEmpty),
        "attach_existing" => Ok(ProjectRegistrationKind::AttachExisting),
        _ => Err(ProjectRegistryError::IntegrityFailure(
            "invalid project registration kind",
        )),
    }
}

fn registration_target_db(value: ProjectRegistrationTarget) -> &'static str {
    match value {
        ProjectRegistrationTarget::NewProject => "new_project",
        ProjectRegistrationTarget::ExistingProject => "existing_project",
    }
}

fn parse_registration_target(
    value: &str,
) -> Result<ProjectRegistrationTarget, ProjectRegistryError> {
    match value {
        "new_project" => Ok(ProjectRegistrationTarget::NewProject),
        "existing_project" => Ok(ProjectRegistrationTarget::ExistingProject),
        _ => Err(ProjectRegistryError::IntegrityFailure(
            "invalid registration target",
        )),
    }
}

fn operation_state_db(value: RegistrationOperationState) -> &'static str {
    match value {
        RegistrationOperationState::Prepared => "prepared",
        RegistrationOperationState::FilesystemApplied => "filesystem_applied",
        RegistrationOperationState::Committed => "committed",
        RegistrationOperationState::RecoveryRequired => "recovery_required",
    }
}

fn parse_operation_state(value: &str) -> Result<RegistrationOperationState, ProjectRegistryError> {
    match value {
        "prepared" => Ok(RegistrationOperationState::Prepared),
        "filesystem_applied" => Ok(RegistrationOperationState::FilesystemApplied),
        "committed" => Ok(RegistrationOperationState::Committed),
        "recovery_required" => Ok(RegistrationOperationState::RecoveryRequired),
        _ => Err(ProjectRegistryError::IntegrityFailure(
            "invalid registration operation state",
        )),
    }
}

fn event_kind_db(value: ProjectLifecycleEventKind) -> &'static str {
    match value {
        ProjectLifecycleEventKind::LegacyProjectImported => "legacy_project_imported",
        ProjectLifecycleEventKind::RegistrationPrepared => "registration_prepared",
        ProjectLifecycleEventKind::RegistrationFilesystemApplied => {
            "registration_filesystem_applied"
        }
        ProjectLifecycleEventKind::RegistrationCommitted => "registration_committed",
        ProjectLifecycleEventKind::RegistrationRecoveryRequired => "registration_recovery_required",
        ProjectLifecycleEventKind::TrustChanged => "trust_changed",
        ProjectLifecycleEventKind::BindingObservationUpdated => "binding_observation_updated",
        ProjectLifecycleEventKind::InstructionFingerprintChanged => {
            "instruction_fingerprint_changed"
        }
        ProjectLifecycleEventKind::WorkspaceRebound => "workspace_rebound",
    }
}

fn parse_event_kind(value: &str) -> Result<ProjectLifecycleEventKind, ProjectRegistryError> {
    match value {
        "legacy_project_imported" => Ok(ProjectLifecycleEventKind::LegacyProjectImported),
        "registration_prepared" => Ok(ProjectLifecycleEventKind::RegistrationPrepared),
        "registration_filesystem_applied" => {
            Ok(ProjectLifecycleEventKind::RegistrationFilesystemApplied)
        }
        "registration_committed" => Ok(ProjectLifecycleEventKind::RegistrationCommitted),
        "registration_recovery_required" => {
            Ok(ProjectLifecycleEventKind::RegistrationRecoveryRequired)
        }
        "trust_changed" => Ok(ProjectLifecycleEventKind::TrustChanged),
        "binding_observation_updated" => Ok(ProjectLifecycleEventKind::BindingObservationUpdated),
        "instruction_fingerprint_changed" => {
            Ok(ProjectLifecycleEventKind::InstructionFingerprintChanged)
        }
        "workspace_rebound" => Ok(ProjectLifecycleEventKind::WorkspaceRebound),
        _ => Err(ProjectRegistryError::IntegrityFailure(
            "invalid project lifecycle event kind",
        )),
    }
}

fn parse_project(row: &SqliteRow) -> Result<ProjectRecord, ProjectRegistryError> {
    Ok(ProjectRecord {
        project_id: ProjectId(parse_uuid(
            row.try_get::<String, _>("project_id")
                .map_err(storage_error)?
                .as_str(),
            "invalid project id",
        )?),
        display_name: row.try_get("display_name").map_err(storage_error)?,
        primary_binding_id: WorkspaceBindingId(parse_uuid(
            row.try_get::<String, _>("primary_binding_id")
                .map_err(storage_error)?
                .as_str(),
            "invalid primary binding id",
        )?),
        revision: from_i64(
            row.try_get("revision").map_err(storage_error)?,
            "invalid project revision",
        )?,
        created_at_unix_ms: from_i64(
            row.try_get("created_at_unix_ms").map_err(storage_error)?,
            "invalid project creation time",
        )?,
        updated_at_unix_ms: from_i64(
            row.try_get("updated_at_unix_ms").map_err(storage_error)?,
            "invalid project update time",
        )?,
    })
}

fn parse_policy(row: &SqliteRow) -> Result<ProjectAccessPolicy, ProjectRegistryError> {
    let decision_kind: Option<String> = row.try_get("last_decision_kind").map_err(storage_error)?;
    let decision_id: Option<String> = row.try_get("last_decision_id").map_err(storage_error)?;
    let last_decision = match (decision_kind, decision_id) {
        (None, None) => None,
        (Some(kind), Some(id)) => Some(
            TrustDecisionRef::new(kind, id)
                .map_err(|_| ProjectRegistryError::IntegrityFailure("invalid trust decision"))?,
        ),
        _ => {
            return Err(ProjectRegistryError::IntegrityFailure(
                "partial trust decision reference",
            ));
        }
    };
    Ok(ProjectAccessPolicy {
        project_id: ProjectId(parse_uuid(
            row.try_get::<String, _>("project_id")
                .map_err(storage_error)?
                .as_str(),
            "invalid policy project id",
        )?),
        trust_state: parse_trust_state(
            row.try_get::<String, _>("trust_state")
                .map_err(storage_error)?
                .as_str(),
        )?,
        revision: from_i64(
            row.try_get("revision").map_err(storage_error)?,
            "invalid policy revision",
        )?,
        last_decision,
        updated_at_unix_ms: from_i64(
            row.try_get("updated_at_unix_ms").map_err(storage_error)?,
            "invalid policy update time",
        )?,
    })
}

fn parse_binding(row: &SqliteRow) -> Result<WorkspaceBinding, ProjectRegistryError> {
    let portable_project_id = row
        .try_get::<Option<String>, _>("portable_project_id")
        .map_err(storage_error)?
        .map(|value| parse_uuid(&value, "invalid portable project id").map(ProjectId))
        .transpose()?;
    Ok(WorkspaceBinding {
        binding_id: WorkspaceBindingId(parse_uuid(
            row.try_get::<String, _>("binding_id")
                .map_err(storage_error)?
                .as_str(),
            "invalid workspace binding id",
        )?),
        project_id: ProjectId(parse_uuid(
            row.try_get::<String, _>("project_id")
                .map_err(storage_error)?
                .as_str(),
            "invalid binding project id",
        )?),
        location: CanonicalWorkspaceLocation {
            path: SensitiveAbsolutePath::new(
                row.try_get::<String, _>("canonical_path")
                    .map_err(storage_error)?,
            )
            .map_err(|_| ProjectRegistryError::IntegrityFailure("invalid canonical path"))?,
            key: CanonicalLocationKey::new(blob32(
                row.try_get("canonical_location_key")
                    .map_err(storage_error)?,
                "invalid canonical location key",
            )?),
        },
        source_identity: optional_blob32(
            row.try_get("source_identity").map_err(storage_error)?,
            "invalid workspace source identity",
        )?
        .map(WorkspaceSourceIdentity::new),
        kind: parse_workspace_kind(
            row.try_get::<String, _>("workspace_kind")
                .map_err(storage_error)?
                .as_str(),
        )?,
        availability: parse_availability(
            row.try_get::<String, _>("availability")
                .map_err(storage_error)?
                .as_str(),
        )?,
        access_mode: parse_access_mode(
            row.try_get::<String, _>("access_mode")
                .map_err(storage_error)?
                .as_str(),
        )?,
        portable_metadata_state: parse_portable_state(
            row.try_get::<String, _>("portable_metadata_state")
                .map_err(storage_error)?
                .as_str(),
        )?,
        portable_project_id,
        primary: parse_bool(
            row.try_get("is_primary").map_err(storage_error)?,
            "invalid primary binding flag",
        )?,
        record_revision: from_i64(
            row.try_get("record_revision").map_err(storage_error)?,
            "invalid binding revision",
        )?,
        created_at_unix_ms: from_i64(
            row.try_get("created_at_unix_ms").map_err(storage_error)?,
            "invalid binding creation time",
        )?,
        last_verified_at_unix_ms: from_i64(
            row.try_get("last_verified_at_unix_ms")
                .map_err(storage_error)?,
            "invalid binding verification time",
        )?,
    })
}

fn parse_fingerprint(row: &SqliteRow) -> Result<InstructionFingerprint, ProjectRegistryError> {
    Ok(InstructionFingerprint {
        project_id: ProjectId(parse_uuid(
            row.try_get::<String, _>("project_id")
                .map_err(storage_error)?
                .as_str(),
            "invalid fingerprint project id",
        )?),
        binding_id: WorkspaceBindingId(parse_uuid(
            row.try_get::<String, _>("binding_id")
                .map_err(storage_error)?
                .as_str(),
            "invalid fingerprint binding id",
        )?),
        sha256: blob32(
            row.try_get("fingerprint_sha256").map_err(storage_error)?,
            "invalid instruction fingerprint",
        )?,
        source_count: from_i64_u32(
            row.try_get("source_count").map_err(storage_error)?,
            "invalid instruction source count",
        )?,
        revision: from_i64(
            row.try_get("revision").map_err(storage_error)?,
            "invalid instruction fingerprint revision",
        )?,
        observed_at_unix_ms: from_i64(
            row.try_get("observed_at_unix_ms").map_err(storage_error)?,
            "invalid instruction observation time",
        )?,
    })
}

fn parse_inspection(row: &SqliteRow) -> Result<ProjectLocationInspection, ProjectRegistryError> {
    let portable_project_id = row
        .try_get::<Option<String>, _>("portable_project_id")
        .map_err(storage_error)?
        .map(|value| parse_uuid(&value, "invalid inspection portable project id").map(ProjectId))
        .transpose()?;
    let inspection = ProjectLocationInspection {
        inspection_id: ProjectInspectionId(parse_uuid(
            row.try_get::<String, _>("inspection_id")
                .map_err(storage_error)?
                .as_str(),
            "invalid inspection id",
        )?),
        registration_kind: parse_registration_kind(
            row.try_get::<String, _>("registration_kind")
                .map_err(storage_error)?
                .as_str(),
        )?,
        location: CanonicalWorkspaceLocation {
            path: SensitiveAbsolutePath::new(
                row.try_get::<String, _>("canonical_path")
                    .map_err(storage_error)?,
            )
            .map_err(|_| ProjectRegistryError::IntegrityFailure("invalid inspection path"))?,
            key: CanonicalLocationKey::new(blob32(
                row.try_get("canonical_location_key")
                    .map_err(storage_error)?,
                "invalid inspection location key",
            )?),
        },
        suggested_display_name: row
            .try_get("suggested_display_name")
            .map_err(storage_error)?,
        location_exists: parse_bool(
            row.try_get("location_exists").map_err(storage_error)?,
            "invalid inspection location exists flag",
        )?,
        location_empty: parse_bool(
            row.try_get("location_empty").map_err(storage_error)?,
            "invalid inspection location empty flag",
        )?,
        source_identity: optional_blob32(
            row.try_get("source_identity").map_err(storage_error)?,
            "invalid inspection source identity",
        )?
        .map(WorkspaceSourceIdentity::new),
        prospective_parent_identity: optional_blob32(
            row.try_get("prospective_parent_identity")
                .map_err(storage_error)?,
            "invalid prospective parent identity",
        )?
        .map(WorkspaceSourceIdentity::new),
        workspace_kind: parse_workspace_kind(
            row.try_get::<String, _>("workspace_kind")
                .map_err(storage_error)?
                .as_str(),
        )?,
        availability: parse_availability(
            row.try_get::<String, _>("availability")
                .map_err(storage_error)?
                .as_str(),
        )?,
        access_mode: parse_access_mode(
            row.try_get::<String, _>("access_mode")
                .map_err(storage_error)?
                .as_str(),
        )?,
        portable_metadata_state: parse_portable_state(
            row.try_get::<String, _>("portable_metadata_state")
                .map_err(storage_error)?
                .as_str(),
        )?,
        portable_project_id,
        shared_memory_state: parse_shared_memory_state(
            row.try_get::<String, _>("shared_memory_state")
                .map_err(storage_error)?
                .as_str(),
        )?,
        minimal_structure_creation_available: parse_bool(
            row.try_get("minimal_structure_creation_available")
                .map_err(storage_error)?,
            "invalid minimal structure availability flag",
        )?,
        instruction_fingerprint: optional_blob32(
            row.try_get("instruction_fingerprint")
                .map_err(storage_error)?,
            "invalid inspection instruction fingerprint",
        )?,
        instruction_source_count: from_i64_u32(
            row.try_get("instruction_source_count")
                .map_err(storage_error)?,
            "invalid inspection instruction source count",
        )?,
        instruction_discovery_incomplete: parse_bool(
            row.try_get("instruction_discovery_incomplete")
                .map_err(storage_error)?,
            "invalid incomplete instruction discovery flag",
        )?,
        observed_at_unix_ms: from_i64(
            row.try_get("observed_at_unix_ms").map_err(storage_error)?,
            "invalid inspection observation time",
        )?,
        expires_at_unix_ms: from_i64(
            row.try_get("expires_at_unix_ms").map_err(storage_error)?,
            "invalid inspection expiry time",
        )?,
    };
    inspection
        .validate()
        .map_err(|_| ProjectRegistryError::IntegrityFailure("invalid stored inspection"))?;
    Ok(inspection)
}

fn parse_operation(row: &SqliteRow) -> Result<ProjectRegistrationOperation, ProjectRegistryError> {
    let command_id = CommandId(parse_uuid(
        row.try_get::<String, _>("command_id")
            .map_err(storage_error)?
            .as_str(),
        "invalid registration command id",
    )?);
    let project_id = ProjectId(parse_uuid(
        row.try_get::<String, _>("project_id")
            .map_err(storage_error)?
            .as_str(),
        "invalid registration project id",
    )?);
    let initial_trust_state = parse_trust_state(
        row.try_get::<String, _>("initial_trust_state")
            .map_err(storage_error)?
            .as_str(),
    )?;
    let decision_kind: Option<String> = row
        .try_get("initial_decision_kind")
        .map_err(storage_error)?;
    let decision_id: Option<String> = row.try_get("initial_decision_id").map_err(storage_error)?;
    let initial_trust = match (decision_kind, decision_id) {
        (None, None) if initial_trust_state == ProjectTrustState::Restricted => None,
        (Some(kind), Some(id)) => Some(
            verify_bridge_attested_project_trust_decision(
                TrustDecisionRef::new(kind, id).map_err(|_| {
                    ProjectRegistryError::IntegrityFailure("invalid initial trust decision")
                })?,
                command_id,
                project_id,
                initial_trust_state,
            )
            .map_err(|_| {
                ProjectRegistryError::IntegrityFailure("unverified initial trust decision")
            })?,
        ),
        _ => {
            return Err(ProjectRegistryError::IntegrityFailure(
                "invalid initial trust decision state",
            ));
        }
    };
    let portable_project_id = row
        .try_get::<Option<String>, _>("portable_project_id")
        .map_err(storage_error)?
        .map(|value| parse_uuid(&value, "invalid operation portable project id").map(ProjectId))
        .transpose()?;
    let plan = ProjectRegistrationPlan {
        operation_id: WorkspaceOperationId(parse_uuid(
            row.try_get::<String, _>("operation_id")
                .map_err(storage_error)?
                .as_str(),
            "invalid registration operation id",
        )?),
        command_id,
        correlation_id: row.try_get("correlation_id").map_err(storage_error)?,
        intent_sha256: blob32(
            row.try_get("intent_sha256").map_err(storage_error)?,
            "invalid registration intent hash",
        )?,
        inspection_id: ProjectInspectionId(parse_uuid(
            row.try_get::<String, _>("inspection_id")
                .map_err(storage_error)?
                .as_str(),
            "invalid registration inspection id",
        )?),
        target: parse_registration_target(
            row.try_get::<String, _>("target_kind")
                .map_err(storage_error)?
                .as_str(),
        )?,
        project_id,
        binding_id: WorkspaceBindingId(parse_uuid(
            row.try_get::<String, _>("binding_id")
                .map_err(storage_error)?
                .as_str(),
            "invalid registration binding id",
        )?),
        direct_session_id: SessionId(parse_uuid(
            row.try_get::<String, _>("direct_session_id")
                .map_err(storage_error)?
                .as_str(),
            "invalid direct session id",
        )?),
        display_name: row.try_get("display_name").map_err(storage_error)?,
        location: CanonicalWorkspaceLocation {
            path: SensitiveAbsolutePath::new(
                row.try_get::<String, _>("canonical_path")
                    .map_err(storage_error)?,
            )
            .map_err(|_| ProjectRegistryError::IntegrityFailure("invalid operation path"))?,
            key: CanonicalLocationKey::new(blob32(
                row.try_get("canonical_location_key")
                    .map_err(storage_error)?,
                "invalid operation location key",
            )?),
        },
        source_identity: optional_blob32(
            row.try_get("source_identity").map_err(storage_error)?,
            "invalid operation source identity",
        )?
        .map(WorkspaceSourceIdentity::new),
        workspace_kind: parse_workspace_kind(
            row.try_get::<String, _>("workspace_kind")
                .map_err(storage_error)?
                .as_str(),
        )?,
        availability: parse_availability(
            row.try_get::<String, _>("availability")
                .map_err(storage_error)?
                .as_str(),
        )?,
        access_mode: parse_access_mode(
            row.try_get::<String, _>("access_mode")
                .map_err(storage_error)?
                .as_str(),
        )?,
        portable_metadata_state: parse_portable_state(
            row.try_get::<String, _>("portable_metadata_state")
                .map_err(storage_error)?
                .as_str(),
        )?,
        portable_project_id,
        portable_metadata_action: parse_portable_action(
            row.try_get::<String, _>("portable_metadata_action")
                .map_err(storage_error)?
                .as_str(),
        )?,
        instruction_fingerprint: optional_blob32(
            row.try_get("instruction_fingerprint")
                .map_err(storage_error)?,
            "invalid operation instruction fingerprint",
        )?,
        instruction_source_count: from_i64_u32(
            row.try_get("instruction_source_count")
                .map_err(storage_error)?,
            "invalid operation instruction source count",
        )?,
        initial_trust,
        prepared_at_unix_ms: from_i64(
            row.try_get("created_at_unix_ms").map_err(storage_error)?,
            "invalid operation creation time",
        )?,
    };
    plan.validate()
        .map_err(|_| ProjectRegistryError::IntegrityFailure("invalid registration plan"))?;
    let final_observed_at_unix_ms = row
        .try_get::<Option<i64>, _>("final_observed_at_unix_ms")
        .map_err(storage_error)?;
    let filesystem_observation = final_observed_at_unix_ms
        .map(|observed_at_unix_ms| {
            let portable_project_id = row
                .try_get::<Option<String>, _>("final_portable_project_id")
                .map_err(storage_error)?
                .map(|value| parse_uuid(&value, "invalid final portable project id").map(ProjectId))
                .transpose()?;
            Ok(RegistrationFilesystemObservation {
                location: plan.location.clone(),
                source_identity: optional_blob32(
                    row.try_get("final_source_identity")
                        .map_err(storage_error)?,
                    "invalid final source identity",
                )?
                .map(WorkspaceSourceIdentity::new),
                workspace_kind: parse_workspace_kind(
                    row.try_get::<String, _>("final_workspace_kind")
                        .map_err(storage_error)?
                        .as_str(),
                )?,
                availability: parse_availability(
                    row.try_get::<String, _>("final_availability")
                        .map_err(storage_error)?
                        .as_str(),
                )?,
                access_mode: parse_access_mode(
                    row.try_get::<String, _>("final_access_mode")
                        .map_err(storage_error)?
                        .as_str(),
                )?,
                portable_metadata_state: parse_portable_state(
                    row.try_get::<String, _>("final_portable_metadata_state")
                        .map_err(storage_error)?
                        .as_str(),
                )?,
                portable_project_id,
                instruction_fingerprint: optional_blob32(
                    row.try_get("final_instruction_fingerprint")
                        .map_err(storage_error)?,
                    "invalid final instruction fingerprint",
                )?,
                instruction_source_count: from_i64_u32(
                    row.try_get::<i64, _>("final_instruction_source_count")
                        .map_err(storage_error)?,
                    "invalid final instruction source count",
                )?,
                observed_at_unix_ms: from_i64(
                    observed_at_unix_ms,
                    "invalid final observation time",
                )?,
            })
        })
        .transpose()?;
    let operation = ProjectRegistrationOperation {
        plan,
        filesystem_observation,
        state: parse_operation_state(
            row.try_get::<String, _>("state")
                .map_err(storage_error)?
                .as_str(),
        )?,
        safe_code: row.try_get("safe_code").map_err(storage_error)?,
        created_at_unix_ms: from_i64(
            row.try_get("created_at_unix_ms").map_err(storage_error)?,
            "invalid operation creation time",
        )?,
        updated_at_unix_ms: from_i64(
            row.try_get("updated_at_unix_ms").map_err(storage_error)?,
            "invalid operation update time",
        )?,
    };
    if matches!(
        operation.state,
        RegistrationOperationState::FilesystemApplied | RegistrationOperationState::Committed
    ) && operation.filesystem_observation.is_none()
    {
        return Err(ProjectRegistryError::IntegrityFailure(
            "registration state is missing final filesystem observation",
        ));
    }
    Ok(operation)
}

fn parse_lifecycle_event(row: &SqliteRow) -> Result<ProjectLifecycleEvent, ProjectRegistryError> {
    Ok(ProjectLifecycleEvent {
        sequence: from_i64(
            row.try_get("sequence").map_err(storage_error)?,
            "invalid lifecycle sequence",
        )?,
        kind: parse_event_kind(
            row.try_get::<String, _>("event_kind")
                .map_err(storage_error)?
                .as_str(),
        )?,
        project_id: ProjectId(parse_uuid(
            row.try_get::<String, _>("project_id")
                .map_err(storage_error)?
                .as_str(),
            "invalid lifecycle project id",
        )?),
        command_id: row
            .try_get::<Option<String>, _>("command_id")
            .map_err(storage_error)?
            .map(|value| parse_uuid(&value, "invalid lifecycle command id").map(CommandId))
            .transpose()?,
        correlation_id: row.try_get("correlation_id").map_err(storage_error)?,
        safe_code: row.try_get("safe_code").map_err(storage_error)?,
        project_revision: row
            .try_get::<Option<i64>, _>("project_revision")
            .map_err(storage_error)?
            .map(|value| from_i64(value, "invalid lifecycle project revision"))
            .transpose()?,
        policy_revision: row
            .try_get::<Option<i64>, _>("policy_revision")
            .map_err(storage_error)?
            .map(|value| from_i64(value, "invalid lifecycle policy revision"))
            .transpose()?,
        binding_revision: row
            .try_get::<Option<i64>, _>("binding_revision")
            .map_err(storage_error)?
            .map(|value| from_i64(value, "invalid lifecycle binding revision"))
            .transpose()?,
        occurred_at_unix_ms: from_i64(
            row.try_get("occurred_at_unix_ms").map_err(storage_error)?,
            "invalid lifecycle event time",
        )?,
    })
}

async fn load_aggregate_tx(
    transaction: &mut Transaction<'_, Sqlite>,
    project_id: ProjectId,
) -> Result<Option<ProjectAggregate>, ProjectRegistryError> {
    let project_row = sqlx::query("SELECT * FROM projects WHERE project_id = ?")
        .bind(project_id.0.to_string())
        .fetch_optional(&mut **transaction)
        .await
        .map_err(storage_error)?;
    let Some(project_row) = project_row else {
        return Ok(None);
    };
    let project = parse_project(&project_row)?;
    let policy_row = sqlx::query("SELECT * FROM project_access_policies WHERE project_id = ?")
        .bind(project_id.0.to_string())
        .fetch_optional(&mut **transaction)
        .await
        .map_err(storage_error)?
        .ok_or(ProjectRegistryError::IntegrityFailure(
            "project policy is missing",
        ))?;
    let access_policy = parse_policy(&policy_row)?;
    let bindings = sqlx::query(
        "SELECT * FROM workspace_bindings WHERE project_id = ? \
         ORDER BY is_primary DESC, created_at_unix_ms, binding_id",
    )
    .bind(project_id.0.to_string())
    .fetch_all(&mut **transaction)
    .await
    .map_err(storage_error)?
    .iter()
    .map(parse_binding)
    .collect::<Result<Vec<_>, _>>()?;
    let instruction_fingerprints = sqlx::query(
        "SELECT * FROM instruction_fingerprints WHERE project_id = ? ORDER BY binding_id",
    )
    .bind(project_id.0.to_string())
    .fetch_all(&mut **transaction)
    .await
    .map_err(storage_error)?
    .iter()
    .map(parse_fingerprint)
    .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(ProjectAggregate {
        project,
        access_policy,
        bindings,
        instruction_fingerprints,
    }))
}

async fn load_binding_tx(
    transaction: &mut Transaction<'_, Sqlite>,
    binding_id: WorkspaceBindingId,
) -> Result<Option<WorkspaceBinding>, ProjectRegistryError> {
    sqlx::query("SELECT * FROM workspace_bindings WHERE binding_id = ?")
        .bind(binding_id.0.to_string())
        .fetch_optional(&mut **transaction)
        .await
        .map_err(storage_error)?
        .as_ref()
        .map(parse_binding)
        .transpose()
}

async fn load_inspection_tx(
    transaction: &mut Transaction<'_, Sqlite>,
    inspection_id: ProjectInspectionId,
) -> Result<Option<ProjectLocationInspection>, ProjectRegistryError> {
    sqlx::query("SELECT * FROM project_location_inspections WHERE inspection_id = ?")
        .bind(inspection_id.0.to_string())
        .fetch_optional(&mut **transaction)
        .await
        .map_err(storage_error)?
        .as_ref()
        .map(parse_inspection)
        .transpose()
}

async fn load_operation_by_command_tx(
    transaction: &mut Transaction<'_, Sqlite>,
    command_id: CommandId,
) -> Result<Option<ProjectRegistrationOperation>, ProjectRegistryError> {
    sqlx::query("SELECT * FROM project_registration_operations WHERE command_id = ?")
        .bind(command_id.0.to_string())
        .fetch_optional(&mut **transaction)
        .await
        .map_err(storage_error)?
        .as_ref()
        .map(parse_operation)
        .transpose()
}

#[allow(clippy::too_many_arguments)]
async fn insert_lifecycle_event_tx(
    transaction: &mut Transaction<'_, Sqlite>,
    kind: ProjectLifecycleEventKind,
    project_id: ProjectId,
    command_id: Option<CommandId>,
    correlation_id: &str,
    safe_code: &str,
    project_revision: Option<u64>,
    policy_revision: Option<u64>,
    binding_revision: Option<u64>,
    occurred_at_unix_ms: u64,
) -> Result<(), ProjectRegistryError> {
    sqlx::query(
        "INSERT INTO project_lifecycle_events(\
            event_kind, project_id, command_id, correlation_id, safe_code, project_revision, \
            policy_revision, binding_revision, occurred_at_unix_ms\
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(event_kind_db(kind))
    .bind(project_id.0.to_string())
    .bind(command_id.map(|value| value.0.to_string()))
    .bind(correlation_id)
    .bind(safe_code)
    .bind(project_revision.map(to_i64).transpose()?)
    .bind(policy_revision.map(to_i64).transpose()?)
    .bind(binding_revision.map(to_i64).transpose()?)
    .bind(to_i64(occurred_at_unix_ms)?)
    .execute(&mut **transaction)
    .await
    .map_err(storage_error)?;
    Ok(())
}

async fn bump_project_revision_tx(
    transaction: &mut Transaction<'_, Sqlite>,
    project_id: ProjectId,
    updated_at_unix_ms: u64,
) -> Result<u64, ProjectRegistryError> {
    let current =
        sqlx::query_scalar::<_, i64>("SELECT revision FROM projects WHERE project_id = ?")
            .bind(project_id.0.to_string())
            .fetch_optional(&mut **transaction)
            .await
            .map_err(storage_error)?
            .ok_or(ProjectRegistryError::NotFound("project"))?;
    let current = from_i64(current, "invalid project revision")?;
    let next = current
        .checked_add(1)
        .ok_or(ProjectRegistryError::IntegrityFailure(
            "project revision overflow",
        ))?;
    let updated = sqlx::query(
        "UPDATE projects SET revision = ?, updated_at_unix_ms = MAX(updated_at_unix_ms, ?) \
         WHERE project_id = ? AND revision = ?",
    )
    .bind(to_i64(next)?)
    .bind(to_i64(updated_at_unix_ms)?)
    .bind(project_id.0.to_string())
    .bind(to_i64(current)?)
    .execute(&mut **transaction)
    .await
    .map_err(storage_error)?;
    if updated.rows_affected() != 1 {
        return Err(ProjectRegistryError::RevisionConflict {
            expected: current,
            actual: current,
        });
    }
    Ok(next)
}

async fn location_conflict_tx(
    transaction: &mut Transaction<'_, Sqlite>,
    location_key: CanonicalLocationKey,
) -> Result<Option<ProjectRegistryError>, ProjectRegistryError> {
    let row = sqlx::query(
        "SELECT operation_id, binding_id FROM workspace_location_claims \
         WHERE canonical_location_key = ?",
    )
    .bind(location_key.as_bytes().as_slice())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(storage_error)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let binding_id: Option<String> = row.try_get("binding_id").map_err(storage_error)?;
    if let Some(binding_id) = binding_id {
        let binding_id = WorkspaceBindingId(parse_uuid(&binding_id, "invalid claimed binding id")?);
        let binding = load_binding_tx(transaction, binding_id).await?.ok_or(
            ProjectRegistryError::IntegrityFailure("location claim references a missing binding"),
        )?;
        return Ok(Some(ProjectRegistryError::CanonicalLocationConflict {
            existing_project_id: binding.project_id,
            existing_binding_id: binding.binding_id,
        }));
    }
    let operation_id: Option<String> = row.try_get("operation_id").map_err(storage_error)?;
    let operation_id = operation_id.ok_or(ProjectRegistryError::IntegrityFailure(
        "location claim has no owner",
    ))?;
    let operation_row =
        sqlx::query("SELECT * FROM project_registration_operations WHERE operation_id = ?")
            .bind(operation_id)
            .fetch_optional(&mut **transaction)
            .await
            .map_err(storage_error)?
            .ok_or(ProjectRegistryError::IntegrityFailure(
                "location claim references a missing operation",
            ))?;
    let operation = parse_operation(&operation_row)?;
    Ok(Some(ProjectRegistryError::CanonicalLocationConflict {
        existing_project_id: operation.plan.project_id,
        existing_binding_id: operation.plan.binding_id,
    }))
}

fn inspection_matches_plan(
    inspection: &ProjectLocationInspection,
    plan: &ProjectRegistrationPlan,
) -> bool {
    inspection.location == plan.location
        && inspection.source_identity == plan.source_identity
        && inspection.workspace_kind == plan.workspace_kind
        && inspection.availability == plan.availability
        && inspection.access_mode == plan.access_mode
        && inspection.portable_metadata_state == plan.portable_metadata_state
        && inspection.portable_project_id == plan.portable_project_id
        && inspection.instruction_fingerprint == plan.instruction_fingerprint
        && inspection.instruction_source_count == plan.instruction_source_count
}

fn inspection_matches_observation(
    inspection: &ProjectLocationInspection,
    observation: &RegistrationFilesystemObservation,
) -> bool {
    inspection.location_exists
        && inspection.prospective_parent_identity.is_none()
        && inspection.location == observation.location
        && inspection.source_identity == observation.source_identity
        && inspection.workspace_kind == observation.workspace_kind
        && inspection.availability == observation.availability
        && inspection.access_mode == observation.access_mode
        && inspection.instruction_fingerprint == observation.instruction_fingerprint
        && inspection.instruction_source_count == observation.instruction_source_count
        && observation.observed_at_unix_ms >= inspection.observed_at_unix_ms
}

impl SqliteControlStore {
    pub(crate) async fn verify_project_registry_integrity(
        &self,
    ) -> Result<(), ProjectRegistryError> {
        let projects = ProjectRegistryPort::list_projects(self).await?;
        for aggregate in projects {
            let primary = aggregate
                .bindings
                .iter()
                .filter(|binding| binding.primary)
                .collect::<Vec<_>>();
            if primary.len() != 1
                || primary[0].binding_id != aggregate.project.primary_binding_id
                || aggregate.access_policy.project_id != aggregate.project.project_id
            {
                return Err(ProjectRegistryError::IntegrityFailure(
                    "project aggregate ownership is inconsistent",
                ));
            }
        }
        for row in sqlx::query("SELECT * FROM project_location_inspections")
            .fetch_all(&self.pool)
            .await
            .map_err(storage_error)?
        {
            parse_inspection(&row)?;
        }
        for row in sqlx::query("SELECT * FROM project_registration_operations")
            .fetch_all(&self.pool)
            .await
            .map_err(storage_error)?
        {
            let operation = parse_operation(&row)?;
            let claim_count = if operation.state == RegistrationOperationState::Committed {
                sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM workspace_location_claims WHERE binding_id = ?",
                )
                .bind(operation.plan.binding_id.0.to_string())
                .fetch_one(&self.pool)
                .await
                .map_err(storage_error)?
            } else {
                sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM workspace_location_claims WHERE operation_id = ?",
                )
                .bind(operation.plan.operation_id.0.to_string())
                .fetch_one(&self.pool)
                .await
                .map_err(storage_error)?
            };
            if claim_count != 1 {
                return Err(ProjectRegistryError::IntegrityFailure(
                    "registration location claim is missing",
                ));
            }
        }
        for row in sqlx::query("SELECT * FROM project_lifecycle_events ORDER BY sequence")
            .fetch_all(&self.pool)
            .await
            .map_err(storage_error)?
        {
            parse_lifecycle_event(&row)?;
        }
        Ok(())
    }
}

#[async_trait]
impl ProjectRegistryPort for SqliteControlStore {
    async fn get_project(
        &self,
        project_id: ProjectId,
    ) -> Result<Option<ProjectAggregate>, ProjectRegistryError> {
        let mut transaction = self.pool.begin().await.map_err(storage_error)?;
        let result = load_aggregate_tx(&mut transaction, project_id).await?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(result)
    }

    async fn list_projects(&self) -> Result<Vec<ProjectAggregate>, ProjectRegistryError> {
        let mut transaction = self.pool.begin().await.map_err(storage_error)?;
        let ids = sqlx::query_scalar::<_, String>(
            "SELECT project_id FROM projects ORDER BY created_at_unix_ms, project_id",
        )
        .fetch_all(&mut *transaction)
        .await
        .map_err(storage_error)?;
        let mut projects = Vec::with_capacity(ids.len());
        for id in ids {
            let project_id = ProjectId(parse_uuid(&id, "invalid project id")?);
            projects.push(
                load_aggregate_tx(&mut transaction, project_id)
                    .await?
                    .ok_or(ProjectRegistryError::IntegrityFailure(
                        "listed project disappeared inside transaction",
                    ))?,
            );
        }
        transaction.commit().await.map_err(storage_error)?;
        Ok(projects)
    }

    async fn find_binding_by_location(
        &self,
        location_key: CanonicalLocationKey,
    ) -> Result<Option<WorkspaceBinding>, ProjectRegistryError> {
        sqlx::query(
            "SELECT b.* FROM workspace_location_claims c \
             JOIN workspace_bindings b ON b.binding_id = c.binding_id \
             WHERE c.canonical_location_key = ? AND c.binding_id IS NOT NULL",
        )
        .bind(location_key.as_bytes().as_slice())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?
        .as_ref()
        .map(parse_binding)
        .transpose()
    }

    async fn save_inspection(
        &self,
        inspection: ProjectLocationInspection,
    ) -> Result<ProjectLocationInspection, ProjectRegistryError> {
        inspection.validate()?;
        let mut transaction = self.pool.begin().await.map_err(storage_error)?;
        if let Some(row) =
            sqlx::query("SELECT * FROM project_location_inspections WHERE inspection_id = ?")
                .bind(inspection.inspection_id.0.to_string())
                .fetch_optional(&mut *transaction)
                .await
                .map_err(storage_error)?
        {
            let existing = parse_inspection(&row)?;
            return if existing == inspection {
                Ok(existing)
            } else {
                Err(ProjectRegistryError::IdempotencyConflict)
            };
        }
        sqlx::query(
            "INSERT INTO project_location_inspections(\
                inspection_id, registration_kind, canonical_path, canonical_location_key, \
                suggested_display_name, location_exists, location_empty, source_identity, \
                prospective_parent_identity, workspace_kind, availability, access_mode, portable_metadata_state, \
                portable_project_id, shared_memory_state, minimal_structure_creation_available, \
                instruction_fingerprint, instruction_source_count, instruction_discovery_incomplete, \
                observed_at_unix_ms, expires_at_unix_ms\
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(inspection.inspection_id.0.to_string())
        .bind(registration_kind_db(inspection.registration_kind))
        .bind(inspection.location.path.expose_local())
        .bind(inspection.location.key.as_bytes().as_slice())
        .bind(&inspection.suggested_display_name)
        .bind(bool_i64(inspection.location_exists))
        .bind(bool_i64(inspection.location_empty))
        .bind(
            inspection
                .source_identity
                .map(|identity| identity.as_bytes().to_vec()),
        )
        .bind(
            inspection
                .prospective_parent_identity
                .map(|identity| identity.as_bytes().to_vec()),
        )
        .bind(workspace_kind_db(inspection.workspace_kind))
        .bind(availability_db(inspection.availability))
        .bind(access_mode_db(inspection.access_mode))
        .bind(portable_state_db(inspection.portable_metadata_state))
        .bind(
            inspection
                .portable_project_id
                .map(|project_id| project_id.0.to_string()),
        )
        .bind(shared_memory_state_db(inspection.shared_memory_state))
        .bind(bool_i64(
            inspection.minimal_structure_creation_available,
        ))
        .bind(inspection.instruction_fingerprint.map(|value| value.to_vec()))
        .bind(i64::from(inspection.instruction_source_count))
        .bind(bool_i64(inspection.instruction_discovery_incomplete))
        .bind(to_i64(inspection.observed_at_unix_ms)?)
        .bind(to_i64(inspection.expires_at_unix_ms)?)
        .execute(&mut *transaction)
        .await
        .map_err(storage_error)?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(inspection)
    }

    async fn load_inspection(
        &self,
        inspection_id: ProjectInspectionId,
        now_unix_ms: u64,
    ) -> Result<ProjectLocationInspection, ProjectRegistryError> {
        let row = sqlx::query("SELECT * FROM project_location_inspections WHERE inspection_id = ?")
            .bind(inspection_id.0.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(storage_error)?
            .ok_or(ProjectRegistryError::NotFound("project inspection"))?;
        let inspection = parse_inspection(&row)?;
        if inspection.is_expired_at(now_unix_ms) {
            return Err(ProjectRegistryError::InspectionExpired);
        }
        Ok(inspection)
    }

    async fn prepare_registration(
        &self,
        plan: ProjectRegistrationPlan,
    ) -> Result<ProjectRegistrationOperation, ProjectRegistryError> {
        plan.validate()?;
        let mut transaction = self.pool.begin().await.map_err(storage_error)?;
        if let Some(existing) =
            load_operation_by_command_tx(&mut transaction, plan.command_id).await?
        {
            return if existing.matches_plan(&plan) {
                Ok(existing)
            } else {
                Err(ProjectRegistryError::IdempotencyConflict)
            };
        }
        if sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM project_registration_operations WHERE operation_id = ?",
        )
        .bind(plan.operation_id.0.to_string())
        .fetch_one(&mut *transaction)
        .await
        .map_err(storage_error)?
            != 0
        {
            return Err(ProjectRegistryError::IdempotencyConflict);
        }
        let inspection = load_inspection_tx(&mut transaction, plan.inspection_id)
            .await?
            .ok_or(ProjectRegistryError::NotFound("project inspection"))?;
        if inspection.is_expired_at(plan.prepared_at_unix_ms) {
            return Err(ProjectRegistryError::InspectionExpired);
        }
        if !inspection_matches_plan(&inspection, &plan) {
            return Err(ProjectRegistryError::InvalidInput(
                "registration plan does not match its inspection",
            ));
        }
        if let Some(conflict) = location_conflict_tx(&mut transaction, plan.location.key).await? {
            return Err(conflict);
        }
        match plan.target {
            ProjectRegistrationTarget::NewProject => {
                if load_aggregate_tx(&mut transaction, plan.project_id)
                    .await?
                    .is_some()
                    || sqlx::query_scalar::<_, i64>(
                        "SELECT COUNT(*) FROM project_registration_operations \
                         WHERE project_id = ? AND target_kind = 'new_project'",
                    )
                    .bind(plan.project_id.0.to_string())
                    .fetch_one(&mut *transaction)
                    .await
                    .map_err(storage_error)?
                        != 0
                {
                    return Err(ProjectRegistryError::ProjectAlreadyExists);
                }
            }
            ProjectRegistrationTarget::ExistingProject => {
                if load_aggregate_tx(&mut transaction, plan.project_id)
                    .await?
                    .is_none()
                {
                    return Err(ProjectRegistryError::NotFound("project"));
                }
                if plan.portable_project_id.is_some()
                    && plan.portable_project_id != Some(plan.project_id)
                {
                    return Err(ProjectRegistryError::InvalidInput(
                        "portable identity does not match existing project",
                    ));
                }
            }
        }
        let decision = plan.initial_trust.as_ref().map(|grant| grant.decision());
        sqlx::query(
            "INSERT INTO project_registration_operations(\
                operation_id, command_id, correlation_id, intent_sha256, inspection_id, target_kind, \
                project_id, binding_id, direct_session_id, display_name, canonical_path, \
                canonical_location_key, source_identity, workspace_kind, availability, access_mode, \
                portable_metadata_state, portable_project_id, instruction_fingerprint, \
                portable_metadata_action, instruction_source_count, initial_trust_state, initial_decision_kind, \
                initial_decision_id, state, safe_code, created_at_unix_ms, updated_at_unix_ms\
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(plan.operation_id.0.to_string())
        .bind(plan.command_id.0.to_string())
        .bind(&plan.correlation_id)
        .bind(plan.intent_sha256.as_slice())
        .bind(plan.inspection_id.0.to_string())
        .bind(registration_target_db(plan.target))
        .bind(plan.project_id.0.to_string())
        .bind(plan.binding_id.0.to_string())
        .bind(plan.direct_session_id.0.to_string())
        .bind(&plan.display_name)
        .bind(plan.location.path.expose_local())
        .bind(plan.location.key.as_bytes().as_slice())
        .bind(
            plan.source_identity
                .map(|identity| identity.as_bytes().to_vec()),
        )
        .bind(workspace_kind_db(plan.workspace_kind))
        .bind(availability_db(plan.availability))
        .bind(access_mode_db(plan.access_mode))
        .bind(portable_state_db(plan.portable_metadata_state))
        .bind(
            plan.portable_project_id
                .map(|project_id| project_id.0.to_string()),
        )
        .bind(plan.instruction_fingerprint.map(|value| value.to_vec()))
        .bind(portable_action_db(plan.portable_metadata_action))
        .bind(i64::from(plan.instruction_source_count))
        .bind(trust_state_db(plan.initial_trust_state()))
        .bind(decision.map(|decision| decision.kind.as_str()))
        .bind(decision.map(|decision| decision.id.as_str()))
        .bind(operation_state_db(RegistrationOperationState::Prepared))
        .bind("project.registration.prepared")
        .bind(to_i64(plan.prepared_at_unix_ms)?)
        .bind(to_i64(plan.prepared_at_unix_ms)?)
        .execute(&mut *transaction)
        .await
        .map_err(storage_error)?;
        sqlx::query(
            "INSERT INTO workspace_location_claims(\
                canonical_location_key, canonical_path, operation_id, binding_id\
             ) VALUES (?, ?, ?, NULL)",
        )
        .bind(plan.location.key.as_bytes().as_slice())
        .bind(plan.location.path.expose_local())
        .bind(plan.operation_id.0.to_string())
        .execute(&mut *transaction)
        .await
        .map_err(storage_error)?;
        insert_lifecycle_event_tx(
            &mut transaction,
            ProjectLifecycleEventKind::RegistrationPrepared,
            plan.project_id,
            Some(plan.command_id),
            &plan.correlation_id,
            "project.registration.prepared",
            None,
            None,
            None,
            plan.prepared_at_unix_ms,
        )
        .await?;
        // Stored timestamps are authoritative; reconstruct once before commit
        // so callers receive exactly what restart recovery will observe.
        let operation = load_operation_by_command_tx(&mut transaction, plan.command_id)
            .await?
            .ok_or(ProjectRegistryError::IntegrityFailure(
                "prepared registration was not stored",
            ))?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(operation)
    }

    async fn record_registration_filesystem_applied(
        &self,
        update: RegistrationFilesystemApplied,
    ) -> Result<ProjectRegistrationOperation, ProjectRegistryError> {
        let mut transaction = self.pool.begin().await.map_err(storage_error)?;
        let current = load_operation_by_command_tx(&mut transaction, update.command_id)
            .await?
            .ok_or(ProjectRegistryError::NotFound("registration operation"))?;
        update.validate_for(&current)?;
        if matches!(
            current.state,
            RegistrationOperationState::FilesystemApplied | RegistrationOperationState::Committed
        ) {
            return if current
                .filesystem_observation
                .as_ref()
                .is_some_and(|stored| stored.matches_effect_facts(&update.observation))
            {
                Ok(current)
            } else {
                Err(ProjectRegistryError::IdempotencyConflict)
            };
        }
        if !matches!(
            current.state,
            RegistrationOperationState::Prepared | RegistrationOperationState::RecoveryRequired
        ) {
            return Err(ProjectRegistryError::InvalidStateTransition);
        }
        let observation = &update.observation;
        let changed = sqlx::query(
            "UPDATE project_registration_operations SET \
                final_source_identity = ?, final_workspace_kind = ?, final_availability = ?, \
                final_access_mode = ?, final_portable_metadata_state = ?, final_portable_project_id = ?, \
                final_instruction_fingerprint = ?, final_instruction_source_count = ?, \
                final_observed_at_unix_ms = ?, state = 'filesystem_applied', safe_code = ?, \
                updated_at_unix_ms = ? \
             WHERE command_id = ? AND state IN ('prepared', 'recovery_required')",
        )
        .bind(
            observation
                .source_identity
                .map(|identity| identity.as_bytes().to_vec()),
        )
        .bind(workspace_kind_db(observation.workspace_kind))
        .bind(availability_db(observation.availability))
        .bind(access_mode_db(observation.access_mode))
        .bind(portable_state_db(observation.portable_metadata_state))
        .bind(
            observation
                .portable_project_id
                .map(|project_id| project_id.0.to_string()),
        )
        .bind(
            observation
                .instruction_fingerprint
                .map(|value| value.to_vec()),
        )
        .bind(i64::from(observation.instruction_source_count))
        .bind(to_i64(observation.observed_at_unix_ms)?)
        .bind(&update.safe_code)
        .bind(to_i64(observation.observed_at_unix_ms)?)
        .bind(update.command_id.0.to_string())
        .execute(&mut *transaction)
        .await
        .map_err(storage_error)?;
        if changed.rows_affected() != 1 {
            return Err(ProjectRegistryError::InvalidStateTransition);
        }
        insert_lifecycle_event_tx(
            &mut transaction,
            ProjectLifecycleEventKind::RegistrationFilesystemApplied,
            current.plan.project_id,
            Some(current.plan.command_id),
            &current.plan.correlation_id,
            &update.safe_code,
            None,
            None,
            None,
            observation.observed_at_unix_ms,
        )
        .await?;
        let operation = load_operation_by_command_tx(&mut transaction, update.command_id)
            .await?
            .ok_or(ProjectRegistryError::IntegrityFailure(
                "filesystem-applied registration disappeared",
            ))?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(operation)
    }

    async fn load_registration(
        &self,
        command_id: CommandId,
    ) -> Result<Option<ProjectRegistrationOperation>, ProjectRegistryError> {
        sqlx::query("SELECT * FROM project_registration_operations WHERE command_id = ?")
            .bind(command_id.0.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(storage_error)?
            .as_ref()
            .map(parse_operation)
            .transpose()
    }

    async fn list_reconcilable_registrations(
        &self,
    ) -> Result<Vec<ProjectRegistrationOperation>, ProjectRegistryError> {
        sqlx::query(
            "SELECT * FROM project_registration_operations \
             WHERE state != 'committed' ORDER BY created_at_unix_ms, operation_id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?
        .iter()
        .map(parse_operation)
        .collect()
    }

    async fn update_registration_state(
        &self,
        update: RegistrationStateUpdate,
    ) -> Result<ProjectRegistrationOperation, ProjectRegistryError> {
        update.validate()?;
        let mut transaction = self.pool.begin().await.map_err(storage_error)?;
        let current = load_operation_by_command_tx(&mut transaction, update.command_id)
            .await?
            .ok_or(ProjectRegistryError::NotFound("registration operation"))?;
        if current.state == update.target_state {
            return if current.safe_code == update.safe_code {
                Ok(current)
            } else {
                Err(ProjectRegistryError::IdempotencyConflict)
            };
        }
        if current.state != update.expected_state
            || !current.state.can_transition_to(update.target_state)
        {
            return Err(ProjectRegistryError::InvalidStateTransition);
        }
        let changed = sqlx::query(
            "UPDATE project_registration_operations SET state = ?, safe_code = ?, \
             updated_at_unix_ms = ? WHERE command_id = ? AND state = ?",
        )
        .bind(operation_state_db(update.target_state))
        .bind(&update.safe_code)
        .bind(to_i64(update.updated_at_unix_ms)?)
        .bind(update.command_id.0.to_string())
        .bind(operation_state_db(update.expected_state))
        .execute(&mut *transaction)
        .await
        .map_err(storage_error)?;
        if changed.rows_affected() != 1 {
            return Err(ProjectRegistryError::InvalidStateTransition);
        }
        let event_kind = match update.target_state {
            RegistrationOperationState::Prepared => ProjectLifecycleEventKind::RegistrationPrepared,
            RegistrationOperationState::FilesystemApplied => {
                ProjectLifecycleEventKind::RegistrationFilesystemApplied
            }
            RegistrationOperationState::RecoveryRequired => {
                ProjectLifecycleEventKind::RegistrationRecoveryRequired
            }
            RegistrationOperationState::Committed => unreachable!("validated above"),
        };
        insert_lifecycle_event_tx(
            &mut transaction,
            event_kind,
            current.plan.project_id,
            Some(current.plan.command_id),
            &current.plan.correlation_id,
            &update.safe_code,
            None,
            None,
            None,
            update.updated_at_unix_ms,
        )
        .await?;
        let operation = load_operation_by_command_tx(&mut transaction, update.command_id)
            .await?
            .ok_or(ProjectRegistryError::IntegrityFailure(
                "updated registration disappeared",
            ))?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(operation)
    }

    async fn commit_registration(
        &self,
        command_id: CommandId,
        committed_at_unix_ms: u64,
    ) -> Result<ProjectRegistrationCommit, ProjectRegistryError> {
        let mut transaction = self.pool.begin().await.map_err(storage_error)?;
        let operation = load_operation_by_command_tx(&mut transaction, command_id)
            .await?
            .ok_or(ProjectRegistryError::NotFound("registration operation"))?;
        if operation.state == RegistrationOperationState::Committed {
            let project = load_aggregate_tx(&mut transaction, operation.plan.project_id)
                .await?
                .ok_or(ProjectRegistryError::IntegrityFailure(
                    "committed registration has no project",
                ))?;
            transaction.commit().await.map_err(storage_error)?;
            return Ok(ProjectRegistrationCommit { operation, project });
        }
        if operation.state != RegistrationOperationState::FilesystemApplied {
            return Err(ProjectRegistryError::InvalidStateTransition);
        }
        let plan = &operation.plan;
        let observation = operation.filesystem_observation.as_ref().ok_or(
            ProjectRegistryError::IntegrityFailure(
                "filesystem-applied registration has no final observation",
            ),
        )?;
        let owns_claim = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM workspace_location_claims \
             WHERE canonical_location_key = ? AND operation_id = ? AND binding_id IS NULL",
        )
        .bind(plan.location.key.as_bytes().as_slice())
        .bind(plan.operation_id.0.to_string())
        .fetch_one(&mut *transaction)
        .await
        .map_err(storage_error)?;
        if owns_claim != 1 {
            return Err(ProjectRegistryError::IntegrityFailure(
                "registration no longer owns its location claim",
            ));
        }
        let (project_revision, policy_revision, primary) = match plan.target {
            ProjectRegistrationTarget::NewProject => {
                if load_aggregate_tx(&mut transaction, plan.project_id)
                    .await?
                    .is_some()
                {
                    return Err(ProjectRegistryError::ProjectAlreadyExists);
                }
                sqlx::query(
                    "INSERT INTO projects(\
                        project_id, display_name, primary_binding_id, revision, \
                        created_at_unix_ms, updated_at_unix_ms\
                     ) VALUES (?, ?, ?, 1, ?, ?)",
                )
                .bind(plan.project_id.0.to_string())
                .bind(&plan.display_name)
                .bind(plan.binding_id.0.to_string())
                .bind(to_i64(committed_at_unix_ms)?)
                .bind(to_i64(committed_at_unix_ms)?)
                .execute(&mut *transaction)
                .await
                .map_err(storage_error)?;
                let decision = plan.initial_trust.as_ref().map(|grant| grant.decision());
                sqlx::query(
                    "INSERT INTO project_access_policies(\
                        project_id, trust_state, revision, last_decision_kind, \
                        last_decision_id, updated_at_unix_ms\
                     ) VALUES (?, ?, 1, ?, ?, ?)",
                )
                .bind(plan.project_id.0.to_string())
                .bind(trust_state_db(plan.initial_trust_state()))
                .bind(decision.map(|decision| decision.kind.as_str()))
                .bind(decision.map(|decision| decision.id.as_str()))
                .bind(to_i64(committed_at_unix_ms)?)
                .execute(&mut *transaction)
                .await
                .map_err(storage_error)?;
                (1, 1, true)
            }
            ProjectRegistrationTarget::ExistingProject => {
                let existing = load_aggregate_tx(&mut transaction, plan.project_id)
                    .await?
                    .ok_or(ProjectRegistryError::NotFound("project"))?;
                if plan.initial_trust.is_some() {
                    return Err(ProjectRegistryError::TrustDecisionRejected);
                }
                let revision = bump_project_revision_tx(
                    &mut transaction,
                    plan.project_id,
                    committed_at_unix_ms,
                )
                .await?;
                (revision, existing.access_policy.revision, false)
            }
        };
        if load_binding_tx(&mut transaction, plan.binding_id)
            .await?
            .is_some()
        {
            return Err(ProjectRegistryError::IdempotencyConflict);
        }
        sqlx::query(
            "INSERT INTO workspace_bindings(\
                binding_id, project_id, canonical_path, canonical_location_key, source_identity, \
                workspace_kind, availability, access_mode, portable_metadata_state, \
                portable_project_id, is_primary, record_revision, created_at_unix_ms, \
                last_verified_at_unix_ms\
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?)",
        )
        .bind(plan.binding_id.0.to_string())
        .bind(plan.project_id.0.to_string())
        .bind(plan.location.path.expose_local())
        .bind(plan.location.key.as_bytes().as_slice())
        .bind(
            observation
                .source_identity
                .map(|identity| identity.as_bytes().to_vec()),
        )
        .bind(workspace_kind_db(observation.workspace_kind))
        .bind(availability_db(observation.availability))
        .bind(access_mode_db(observation.access_mode))
        .bind(portable_state_db(observation.portable_metadata_state))
        .bind(
            observation
                .portable_project_id
                .map(|project_id| project_id.0.to_string()),
        )
        .bind(bool_i64(primary))
        .bind(to_i64(committed_at_unix_ms)?)
        .bind(to_i64(committed_at_unix_ms)?)
        .execute(&mut *transaction)
        .await
        .map_err(storage_error)?;
        if let Some(fingerprint) = observation.instruction_fingerprint {
            sqlx::query(
                "INSERT INTO instruction_fingerprints(\
                    binding_id, project_id, fingerprint_sha256, source_count, revision, \
                    observed_at_unix_ms\
                 ) VALUES (?, ?, ?, ?, 1, ?)",
            )
            .bind(plan.binding_id.0.to_string())
            .bind(plan.project_id.0.to_string())
            .bind(fingerprint.as_slice())
            .bind(i64::from(observation.instruction_source_count))
            .bind(to_i64(observation.observed_at_unix_ms)?)
            .execute(&mut *transaction)
            .await
            .map_err(storage_error)?;
        }
        let moved_claim = sqlx::query(
            "UPDATE workspace_location_claims SET operation_id = NULL, binding_id = ? \
             WHERE canonical_location_key = ? AND operation_id = ? AND binding_id IS NULL",
        )
        .bind(plan.binding_id.0.to_string())
        .bind(plan.location.key.as_bytes().as_slice())
        .bind(plan.operation_id.0.to_string())
        .execute(&mut *transaction)
        .await
        .map_err(storage_error)?;
        if moved_claim.rows_affected() != 1 {
            return Err(ProjectRegistryError::IntegrityFailure(
                "registration location claim could not be committed",
            ));
        }
        let completed = sqlx::query(
            "UPDATE project_registration_operations SET state = 'committed', \
             safe_code = 'project.registration.committed', updated_at_unix_ms = ? \
             WHERE command_id = ? AND state = 'filesystem_applied'",
        )
        .bind(to_i64(committed_at_unix_ms)?)
        .bind(command_id.0.to_string())
        .execute(&mut *transaction)
        .await
        .map_err(storage_error)?;
        if completed.rows_affected() != 1 {
            return Err(ProjectRegistryError::InvalidStateTransition);
        }
        insert_lifecycle_event_tx(
            &mut transaction,
            ProjectLifecycleEventKind::RegistrationCommitted,
            plan.project_id,
            Some(command_id),
            &plan.correlation_id,
            "project.registration.committed",
            Some(project_revision),
            Some(policy_revision),
            Some(1),
            committed_at_unix_ms,
        )
        .await?;
        if observation.instruction_fingerprint.is_some() {
            insert_lifecycle_event_tx(
                &mut transaction,
                ProjectLifecycleEventKind::InstructionFingerprintChanged,
                plan.project_id,
                Some(command_id),
                &plan.correlation_id,
                "project.instructions.initial",
                Some(project_revision),
                Some(policy_revision),
                Some(1),
                committed_at_unix_ms,
            )
            .await?;
        }
        let operation = load_operation_by_command_tx(&mut transaction, command_id)
            .await?
            .ok_or(ProjectRegistryError::IntegrityFailure(
                "committed registration disappeared",
            ))?;
        let project = load_aggregate_tx(&mut transaction, plan.project_id)
            .await?
            .ok_or(ProjectRegistryError::IntegrityFailure(
                "registration commit did not publish project",
            ))?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(ProjectRegistrationCommit { operation, project })
    }

    async fn import_legacy_project(
        &self,
        import: LegacyProjectImport,
    ) -> Result<ProjectAggregate, ProjectRegistryError> {
        import.validate()?;
        let mut transaction = self.pool.begin().await.map_err(storage_error)?;
        if let Some(binding) = load_binding_tx(&mut transaction, import.binding_id).await? {
            if binding.project_id == import.project_id && binding.location == import.location {
                // This is a one-time migration identity, not a fresh binding
                // observation. A folder may disappear or be replaced between
                // restarts; startup reconciliation records that state without
                // rewriting the original M01 project/session identity.
                let project = load_aggregate_tx(&mut transaction, import.project_id)
                    .await?
                    .ok_or(ProjectRegistryError::IntegrityFailure(
                        "legacy binding has no project",
                    ))?;
                transaction.commit().await.map_err(storage_error)?;
                return Ok(project);
            }
            return Err(ProjectRegistryError::IdempotencyConflict);
        }
        if let Some(conflict) = location_conflict_tx(&mut transaction, import.location.key).await? {
            return Err(conflict);
        }
        let existing = load_aggregate_tx(&mut transaction, import.project_id).await?;
        let (project_revision, policy_revision, primary) = if let Some(existing) = existing {
            let revision = bump_project_revision_tx(
                &mut transaction,
                import.project_id,
                import.imported_at_unix_ms,
            )
            .await?;
            (revision, existing.access_policy.revision, false)
        } else {
            sqlx::query(
                "INSERT INTO projects(\
                    project_id, display_name, primary_binding_id, revision, \
                    created_at_unix_ms, updated_at_unix_ms\
                 ) VALUES (?, ?, ?, 1, ?, ?)",
            )
            .bind(import.project_id.0.to_string())
            .bind(&import.display_name)
            .bind(import.binding_id.0.to_string())
            .bind(to_i64(import.imported_at_unix_ms)?)
            .bind(to_i64(import.imported_at_unix_ms)?)
            .execute(&mut *transaction)
            .await
            .map_err(storage_error)?;
            sqlx::query(
                "INSERT INTO project_access_policies(\
                    project_id, trust_state, revision, last_decision_kind, \
                    last_decision_id, updated_at_unix_ms\
                 ) VALUES (?, 'restricted', 1, NULL, NULL, ?)",
            )
            .bind(import.project_id.0.to_string())
            .bind(to_i64(import.imported_at_unix_ms)?)
            .execute(&mut *transaction)
            .await
            .map_err(storage_error)?;
            (1, 1, true)
        };
        sqlx::query(
            "INSERT INTO workspace_bindings(\
                binding_id, project_id, canonical_path, canonical_location_key, source_identity, \
                workspace_kind, availability, access_mode, portable_metadata_state, \
                portable_project_id, is_primary, record_revision, created_at_unix_ms, \
                last_verified_at_unix_ms\
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?)",
        )
        .bind(import.binding_id.0.to_string())
        .bind(import.project_id.0.to_string())
        .bind(import.location.path.expose_local())
        .bind(import.location.key.as_bytes().as_slice())
        .bind(
            import
                .source_identity
                .map(|identity| identity.as_bytes().to_vec()),
        )
        .bind(workspace_kind_db(import.workspace_kind))
        .bind(availability_db(import.availability))
        .bind(access_mode_db(import.access_mode))
        .bind(portable_state_db(import.portable_metadata_state))
        .bind(
            import
                .portable_project_id
                .map(|project_id| project_id.0.to_string()),
        )
        .bind(bool_i64(primary))
        .bind(to_i64(import.imported_at_unix_ms)?)
        .bind(to_i64(import.imported_at_unix_ms)?)
        .execute(&mut *transaction)
        .await
        .map_err(storage_error)?;
        if let Some(fingerprint) = import.instruction_fingerprint {
            sqlx::query(
                "INSERT INTO instruction_fingerprints(\
                    binding_id, project_id, fingerprint_sha256, source_count, revision, \
                    observed_at_unix_ms\
                 ) VALUES (?, ?, ?, ?, 1, ?)",
            )
            .bind(import.binding_id.0.to_string())
            .bind(import.project_id.0.to_string())
            .bind(fingerprint.as_slice())
            .bind(i64::from(import.instruction_source_count))
            .bind(to_i64(import.imported_at_unix_ms)?)
            .execute(&mut *transaction)
            .await
            .map_err(storage_error)?;
        }
        if import.claim_location {
            sqlx::query(
                "INSERT INTO workspace_location_claims(\
                    canonical_location_key, canonical_path, operation_id, binding_id\
                 ) VALUES (?, ?, NULL, ?)",
            )
            .bind(import.location.key.as_bytes().as_slice())
            .bind(import.location.path.expose_local())
            .bind(import.binding_id.0.to_string())
            .execute(&mut *transaction)
            .await
            .map_err(storage_error)?;
        }
        insert_lifecycle_event_tx(
            &mut transaction,
            ProjectLifecycleEventKind::LegacyProjectImported,
            import.project_id,
            None,
            &import.correlation_id,
            "project.legacy_imported",
            Some(project_revision),
            Some(policy_revision),
            Some(1),
            import.imported_at_unix_ms,
        )
        .await?;
        let project = load_aggregate_tx(&mut transaction, import.project_id)
            .await?
            .ok_or(ProjectRegistryError::IntegrityFailure(
                "legacy import did not publish project",
            ))?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(project)
    }

    async fn compare_and_set_access_policy(
        &self,
        update: ProjectAccessPolicyUpdate,
    ) -> Result<ProjectAccessPolicy, ProjectRegistryError> {
        update.validate()?;
        let mut transaction = self.pool.begin().await.map_err(storage_error)?;
        let row = sqlx::query("SELECT * FROM project_access_policies WHERE project_id = ?")
            .bind(update.project_id.0.to_string())
            .fetch_optional(&mut *transaction)
            .await
            .map_err(storage_error)?
            .ok_or(ProjectRegistryError::NotFound("project access policy"))?;
        let current = parse_policy(&row)?;
        if current.revision != update.expected_revision {
            return Err(ProjectRegistryError::RevisionConflict {
                expected: update.expected_revision,
                actual: current.revision,
            });
        }
        let next =
            current
                .revision
                .checked_add(1)
                .ok_or(ProjectRegistryError::IntegrityFailure(
                    "policy revision overflow",
                ))?;
        let decision = update.grant.decision();
        let changed = sqlx::query(
            "UPDATE project_access_policies SET trust_state = ?, revision = ?, \
             last_decision_kind = ?, last_decision_id = ?, updated_at_unix_ms = ? \
             WHERE project_id = ? AND revision = ?",
        )
        .bind(trust_state_db(update.grant.target_state()))
        .bind(to_i64(next)?)
        .bind(&decision.kind)
        .bind(&decision.id)
        .bind(to_i64(update.updated_at_unix_ms)?)
        .bind(update.project_id.0.to_string())
        .bind(to_i64(update.expected_revision)?)
        .execute(&mut *transaction)
        .await
        .map_err(storage_error)?;
        if changed.rows_affected() != 1 {
            return Err(ProjectRegistryError::RevisionConflict {
                expected: update.expected_revision,
                actual: current.revision,
            });
        }
        let project_revision = bump_project_revision_tx(
            &mut transaction,
            update.project_id,
            update.updated_at_unix_ms,
        )
        .await?;
        insert_lifecycle_event_tx(
            &mut transaction,
            ProjectLifecycleEventKind::TrustChanged,
            update.project_id,
            Some(update.command_id),
            &update.correlation_id,
            "project.trust.changed",
            Some(project_revision),
            Some(next),
            None,
            update.updated_at_unix_ms,
        )
        .await?;
        let row = sqlx::query("SELECT * FROM project_access_policies WHERE project_id = ?")
            .bind(update.project_id.0.to_string())
            .fetch_one(&mut *transaction)
            .await
            .map_err(storage_error)?;
        let policy = parse_policy(&row)?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(policy)
    }

    async fn compare_and_set_binding_observation(
        &self,
        update: BindingObservationUpdate,
    ) -> Result<WorkspaceBinding, ProjectRegistryError> {
        update.validate()?;
        let mut transaction = self.pool.begin().await.map_err(storage_error)?;
        let current = load_binding_tx(&mut transaction, update.binding_id)
            .await?
            .ok_or(ProjectRegistryError::NotFound("workspace binding"))?;
        if current.record_revision != update.expected_revision {
            return Err(ProjectRegistryError::RevisionConflict {
                expected: update.expected_revision,
                actual: current.record_revision,
            });
        }
        if let (Some(existing), Some(observed)) =
            (current.source_identity, update.observed_source_identity)
            && existing != observed
        {
            return Err(ProjectRegistryError::SourceIdentityConflict);
        }
        if current.availability != WorkspaceAvailability::Available
            && update.availability == WorkspaceAvailability::Available
            && current.source_identity.is_some()
            && update.observed_source_identity != current.source_identity
        {
            return Err(ProjectRegistryError::SourceIdentityConflict);
        }
        let source_identity = update.observed_source_identity.or(current.source_identity);
        let next = current.record_revision.checked_add(1).ok_or(
            ProjectRegistryError::IntegrityFailure("binding revision overflow"),
        )?;
        let changed = sqlx::query(
            "UPDATE workspace_bindings SET source_identity = ?, availability = ?, access_mode = ?, \
             portable_metadata_state = ?, portable_project_id = ?, record_revision = ?, \
             last_verified_at_unix_ms = ? \
             WHERE binding_id = ? AND record_revision = ?",
        )
        .bind(source_identity.map(|identity| identity.as_bytes().to_vec()))
        .bind(availability_db(update.availability))
        .bind(access_mode_db(update.access_mode))
        .bind(portable_state_db(update.portable_metadata_state))
        .bind(
            update
                .portable_project_id
                .map(|project_id| project_id.0.to_string()),
        )
        .bind(to_i64(next)?)
        .bind(to_i64(update.verified_at_unix_ms)?)
        .bind(update.binding_id.0.to_string())
        .bind(to_i64(update.expected_revision)?)
        .execute(&mut *transaction)
        .await
        .map_err(storage_error)?;
        if changed.rows_affected() != 1 {
            return Err(ProjectRegistryError::RevisionConflict {
                expected: update.expected_revision,
                actual: current.record_revision,
            });
        }
        let project_revision = bump_project_revision_tx(
            &mut transaction,
            current.project_id,
            update.verified_at_unix_ms,
        )
        .await?;
        insert_lifecycle_event_tx(
            &mut transaction,
            ProjectLifecycleEventKind::BindingObservationUpdated,
            current.project_id,
            update.command_id,
            &update.correlation_id,
            &update.safe_code,
            Some(project_revision),
            None,
            Some(next),
            update.verified_at_unix_ms,
        )
        .await?;
        let binding = load_binding_tx(&mut transaction, update.binding_id)
            .await?
            .ok_or(ProjectRegistryError::IntegrityFailure(
                "updated workspace binding disappeared",
            ))?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(binding)
    }

    async fn rebind_project_workspace(
        &self,
        plan: ProjectWorkspaceRebindPlan,
    ) -> Result<ProjectWorkspaceRebindReceipt, ProjectRegistryError> {
        plan.validate()?;
        let mut transaction = self.pool.begin().await.map_err(storage_error)?;

        if let Some(row) = sqlx::query("SELECT * FROM project_rebind_receipts WHERE command_id = ?")
            .bind(plan.command_id.0.to_string())
            .fetch_optional(&mut *transaction)
            .await
            .map_err(storage_error)?
        {
            let stored_project_id = ProjectId(parse_uuid(
                row.try_get::<String, _>("project_id")
                    .map_err(storage_error)?
                    .as_str(),
                "invalid rebind receipt project id",
            )?);
            let stored_intent = blob32(
                row.try_get("intent_sha256").map_err(storage_error)?,
                "invalid rebind receipt intent",
            )?;
            let stored_correlation: String =
                row.try_get("correlation_id").map_err(storage_error)?;
            if stored_project_id != plan.project_id
                || stored_intent != plan.intent_sha256
                || stored_correlation != plan.correlation_id
            {
                return Err(ProjectRegistryError::IdempotencyConflict);
            }
            let primary_binding_id = WorkspaceBindingId(parse_uuid(
                row.try_get::<String, _>("primary_binding_id")
                    .map_err(storage_error)?
                    .as_str(),
                "invalid rebind receipt binding id",
            )?);
            let primary_binding = load_binding_tx(&mut transaction, primary_binding_id)
                .await?
                .ok_or(ProjectRegistryError::IntegrityFailure(
                    "rebind receipt references a missing binding",
                ))?;
            let receipt = ProjectWorkspaceRebindReceipt {
                command_id: plan.command_id,
                correlation_id: stored_correlation,
                intent_sha256: stored_intent,
                project_id: stored_project_id,
                previous_binding_id: WorkspaceBindingId(parse_uuid(
                    row.try_get::<String, _>("previous_binding_id")
                        .map_err(storage_error)?
                        .as_str(),
                    "invalid previous rebind binding id",
                )?),
                primary_binding,
                project_revision: from_i64(
                    row.try_get("project_revision").map_err(storage_error)?,
                    "invalid rebind project revision",
                )?,
                rebound_at_unix_ms: from_i64(
                    row.try_get("rebound_at_unix_ms").map_err(storage_error)?,
                    "invalid rebind time",
                )?,
            };
            transaction.commit().await.map_err(storage_error)?;
            return Ok(receipt);
        }

        let aggregate = load_aggregate_tx(&mut transaction, plan.project_id)
            .await?
            .ok_or(ProjectRegistryError::NotFound("project"))?;
        if aggregate.project.primary_binding_id != plan.current_binding_id {
            return Err(ProjectRegistryError::BindingProjectMismatch);
        }
        let current = load_binding_tx(&mut transaction, plan.current_binding_id)
            .await?
            .ok_or(ProjectRegistryError::NotFound("workspace binding"))?;
        if current.project_id != plan.project_id || !current.primary {
            return Err(ProjectRegistryError::BindingProjectMismatch);
        }
        if current.record_revision != plan.expected_current_binding_revision {
            return Err(ProjectRegistryError::RevisionConflict {
                expected: plan.expected_current_binding_revision,
                actual: current.record_revision,
            });
        }
        if current.source_identity != plan.expected_current_source_identity {
            return Err(ProjectRegistryError::SourceIdentityConflict);
        }
        let inspection = load_inspection_tx(&mut transaction, plan.inspection_id)
            .await?
            .ok_or(ProjectRegistryError::NotFound("project inspection"))?;
        if inspection.is_expired_at(plan.rebound_at_unix_ms) {
            return Err(ProjectRegistryError::InspectionExpired);
        }
        if !inspection_matches_observation(&inspection, &plan.final_observation) {
            return Err(ProjectRegistryError::InvalidInput(
                "rebind final observation does not match its inspection",
            ));
        }
        if let Some(conflict) =
            location_conflict_tx(&mut transaction, plan.final_observation.location.key).await?
        {
            match conflict {
                ProjectRegistryError::CanonicalLocationConflict {
                    existing_binding_id,
                    ..
                } if existing_binding_id == current.binding_id => {}
                other => return Err(other),
            }
        }
        if plan.replacement_binding_id != current.binding_id
            && load_binding_tx(&mut transaction, plan.replacement_binding_id)
                .await?
                .is_some()
        {
            return Err(ProjectRegistryError::IdempotencyConflict);
        }

        let observation = &plan.final_observation;
        let binding_revision = if plan.replacement_binding_id == current.binding_id {
            let next = current.record_revision.checked_add(1).ok_or(
                ProjectRegistryError::IntegrityFailure("binding revision overflow"),
            )?;
            sqlx::query("DELETE FROM workspace_location_claims WHERE binding_id = ?")
                .bind(current.binding_id.0.to_string())
                .execute(&mut *transaction)
                .await
                .map_err(storage_error)?;
            let changed = sqlx::query(
                "UPDATE workspace_bindings SET canonical_path = ?, canonical_location_key = ?, \
                    source_identity = ?, workspace_kind = ?, availability = ?, access_mode = ?, \
                    portable_metadata_state = ?, portable_project_id = ?, is_primary = 1, \
                    record_revision = ?, last_verified_at_unix_ms = ? \
                 WHERE binding_id = ? AND project_id = ? AND record_revision = ?",
            )
            .bind(observation.location.path.expose_local())
            .bind(observation.location.key.as_bytes().as_slice())
            .bind(
                observation
                    .source_identity
                    .map(|identity| identity.as_bytes().to_vec()),
            )
            .bind(workspace_kind_db(observation.workspace_kind))
            .bind(availability_db(observation.availability))
            .bind(access_mode_db(observation.access_mode))
            .bind(portable_state_db(observation.portable_metadata_state))
            .bind(
                observation
                    .portable_project_id
                    .map(|project_id| project_id.0.to_string()),
            )
            .bind(to_i64(next)?)
            .bind(to_i64(observation.observed_at_unix_ms)?)
            .bind(current.binding_id.0.to_string())
            .bind(plan.project_id.0.to_string())
            .bind(to_i64(current.record_revision)?)
            .execute(&mut *transaction)
            .await
            .map_err(storage_error)?;
            if changed.rows_affected() != 1 {
                return Err(ProjectRegistryError::RevisionConflict {
                    expected: current.record_revision,
                    actual: current.record_revision,
                });
            }
            next
        } else {
            let previous_revision = current.record_revision.checked_add(1).ok_or(
                ProjectRegistryError::IntegrityFailure("binding revision overflow"),
            )?;
            sqlx::query(
                "UPDATE workspace_bindings SET is_primary = 0, record_revision = ? \
                 WHERE binding_id = ? AND project_id = ? AND record_revision = ?",
            )
            .bind(to_i64(previous_revision)?)
            .bind(current.binding_id.0.to_string())
            .bind(plan.project_id.0.to_string())
            .bind(to_i64(current.record_revision)?)
            .execute(&mut *transaction)
            .await
            .map_err(storage_error)?;
            sqlx::query(
                "INSERT INTO workspace_bindings(\
                    binding_id, project_id, canonical_path, canonical_location_key, source_identity, \
                    workspace_kind, availability, access_mode, portable_metadata_state, \
                    portable_project_id, is_primary, record_revision, created_at_unix_ms, \
                    last_verified_at_unix_ms\
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, 1, ?, ?)",
            )
            .bind(plan.replacement_binding_id.0.to_string())
            .bind(plan.project_id.0.to_string())
            .bind(observation.location.path.expose_local())
            .bind(observation.location.key.as_bytes().as_slice())
            .bind(
                observation
                    .source_identity
                    .map(|identity| identity.as_bytes().to_vec()),
            )
            .bind(workspace_kind_db(observation.workspace_kind))
            .bind(availability_db(observation.availability))
            .bind(access_mode_db(observation.access_mode))
            .bind(portable_state_db(observation.portable_metadata_state))
            .bind(
                observation
                    .portable_project_id
                    .map(|project_id| project_id.0.to_string()),
            )
            .bind(to_i64(plan.rebound_at_unix_ms)?)
            .bind(to_i64(observation.observed_at_unix_ms)?)
            .execute(&mut *transaction)
            .await
            .map_err(storage_error)?;
            1
        };

        sqlx::query(
            "INSERT INTO workspace_location_claims(\
                canonical_location_key, canonical_path, operation_id, binding_id\
             ) VALUES (?, ?, NULL, ?)",
        )
        .bind(observation.location.key.as_bytes().as_slice())
        .bind(observation.location.path.expose_local())
        .bind(plan.replacement_binding_id.0.to_string())
        .execute(&mut *transaction)
        .await
        .map_err(storage_error)?;

        sqlx::query("DELETE FROM instruction_fingerprints WHERE binding_id = ?")
            .bind(plan.replacement_binding_id.0.to_string())
            .execute(&mut *transaction)
            .await
            .map_err(storage_error)?;
        if let Some(fingerprint) = observation.instruction_fingerprint {
            sqlx::query(
                "INSERT INTO instruction_fingerprints(\
                    binding_id, project_id, fingerprint_sha256, source_count, revision, observed_at_unix_ms\
                 ) VALUES (?, ?, ?, ?, 1, ?)",
            )
            .bind(plan.replacement_binding_id.0.to_string())
            .bind(plan.project_id.0.to_string())
            .bind(fingerprint.as_slice())
            .bind(i64::from(observation.instruction_source_count))
            .bind(to_i64(observation.observed_at_unix_ms)?)
            .execute(&mut *transaction)
            .await
            .map_err(storage_error)?;
        }

        let project_revision = aggregate.project.revision.checked_add(1).ok_or(
            ProjectRegistryError::IntegrityFailure("project revision overflow"),
        )?;
        let changed = sqlx::query(
            "UPDATE projects SET primary_binding_id = ?, revision = ?, updated_at_unix_ms = ? \
             WHERE project_id = ? AND revision = ?",
        )
        .bind(plan.replacement_binding_id.0.to_string())
        .bind(to_i64(project_revision)?)
        .bind(to_i64(plan.rebound_at_unix_ms)?)
        .bind(plan.project_id.0.to_string())
        .bind(to_i64(aggregate.project.revision)?)
        .execute(&mut *transaction)
        .await
        .map_err(storage_error)?;
        if changed.rows_affected() != 1 {
            return Err(ProjectRegistryError::RevisionConflict {
                expected: aggregate.project.revision,
                actual: aggregate.project.revision,
            });
        }

        sqlx::query(
            "INSERT INTO project_rebind_receipts(\
                command_id, correlation_id, intent_sha256, project_id, previous_binding_id, \
                primary_binding_id, project_revision, rebound_at_unix_ms\
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(plan.command_id.0.to_string())
        .bind(&plan.correlation_id)
        .bind(plan.intent_sha256.as_slice())
        .bind(plan.project_id.0.to_string())
        .bind(current.binding_id.0.to_string())
        .bind(plan.replacement_binding_id.0.to_string())
        .bind(to_i64(project_revision)?)
        .bind(to_i64(plan.rebound_at_unix_ms)?)
        .execute(&mut *transaction)
        .await
        .map_err(storage_error)?;
        insert_lifecycle_event_tx(
            &mut transaction,
            ProjectLifecycleEventKind::WorkspaceRebound,
            plan.project_id,
            Some(plan.command_id),
            &plan.correlation_id,
            &plan.safe_code,
            Some(project_revision),
            Some(aggregate.access_policy.revision),
            Some(binding_revision),
            plan.rebound_at_unix_ms,
        )
        .await?;
        let primary_binding = load_binding_tx(&mut transaction, plan.replacement_binding_id)
            .await?
            .ok_or(ProjectRegistryError::IntegrityFailure(
                "rebind did not publish its primary binding",
            ))?;
        let receipt = ProjectWorkspaceRebindReceipt {
            command_id: plan.command_id,
            correlation_id: plan.correlation_id,
            intent_sha256: plan.intent_sha256,
            project_id: plan.project_id,
            previous_binding_id: current.binding_id,
            primary_binding,
            project_revision,
            rebound_at_unix_ms: plan.rebound_at_unix_ms,
        };
        transaction.commit().await.map_err(storage_error)?;
        Ok(receipt)
    }

    async fn compare_and_set_instruction_fingerprint(
        &self,
        update: InstructionFingerprintUpdate,
    ) -> Result<InstructionFingerprint, ProjectRegistryError> {
        update.validate()?;
        let mut transaction = self.pool.begin().await.map_err(storage_error)?;
        let binding = load_binding_tx(&mut transaction, update.binding_id)
            .await?
            .ok_or(ProjectRegistryError::NotFound("workspace binding"))?;
        if binding.project_id != update.project_id {
            return Err(ProjectRegistryError::BindingProjectMismatch);
        }
        let current_row =
            sqlx::query("SELECT * FROM instruction_fingerprints WHERE binding_id = ?")
                .bind(update.binding_id.0.to_string())
                .fetch_optional(&mut *transaction)
                .await
                .map_err(storage_error)?;
        let current = current_row.as_ref().map(parse_fingerprint).transpose()?;
        let actual_revision = current.as_ref().map_or(0, |value| value.revision);
        if actual_revision != update.expected_revision {
            return Err(ProjectRegistryError::RevisionConflict {
                expected: update.expected_revision,
                actual: actual_revision,
            });
        }
        let next_revision =
            actual_revision
                .checked_add(1)
                .ok_or(ProjectRegistryError::IntegrityFailure(
                    "fingerprint revision overflow",
                ))?;
        sqlx::query(
            "INSERT INTO instruction_fingerprints(\
                binding_id, project_id, fingerprint_sha256, source_count, revision, observed_at_unix_ms\
             ) VALUES (?, ?, ?, ?, ?, ?) \
             ON CONFLICT(binding_id) DO UPDATE SET fingerprint_sha256 = excluded.fingerprint_sha256, \
                source_count = excluded.source_count, revision = excluded.revision, \
                observed_at_unix_ms = excluded.observed_at_unix_ms",
        )
        .bind(update.binding_id.0.to_string())
        .bind(update.project_id.0.to_string())
        .bind(update.sha256.as_slice())
        .bind(i64::from(update.source_count))
        .bind(to_i64(next_revision)?)
        .bind(to_i64(update.observed_at_unix_ms)?)
        .execute(&mut *transaction)
        .await
        .map_err(storage_error)?;
        let project_revision = bump_project_revision_tx(
            &mut transaction,
            update.project_id,
            update.observed_at_unix_ms,
        )
        .await?;
        insert_lifecycle_event_tx(
            &mut transaction,
            ProjectLifecycleEventKind::InstructionFingerprintChanged,
            update.project_id,
            update.command_id,
            &update.correlation_id,
            &update.safe_code,
            Some(project_revision),
            None,
            Some(binding.record_revision),
            update.observed_at_unix_ms,
        )
        .await?;
        let row = sqlx::query("SELECT * FROM instruction_fingerprints WHERE binding_id = ?")
            .bind(update.binding_id.0.to_string())
            .fetch_one(&mut *transaction)
            .await
            .map_err(storage_error)?;
        let fingerprint = parse_fingerprint(&row)?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(fingerprint)
    }

    async fn list_lifecycle_events(
        &self,
        project_id: ProjectId,
    ) -> Result<Vec<ProjectLifecycleEvent>, ProjectRegistryError> {
        sqlx::query("SELECT * FROM project_lifecycle_events WHERE project_id = ? ORDER BY sequence")
            .bind(project_id.0.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(storage_error)?
            .iter()
            .map(parse_lifecycle_event)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dennett_contracts::RebindPortableMetadataAction;
    use std::path::Path;
    use tempfile::tempdir;

    fn location(path: &Path, marker: u8) -> CanonicalWorkspaceLocation {
        CanonicalWorkspaceLocation {
            path: SensitiveAbsolutePath::new(path.to_string_lossy().into_owned()).unwrap(),
            key: CanonicalLocationKey::new([marker; 32]),
        }
    }

    fn absent_create_inspection(
        root: &Path,
        inspection_id: ProjectInspectionId,
    ) -> ProjectLocationInspection {
        ProjectLocationInspection {
            inspection_id,
            registration_kind: ProjectRegistrationKind::CreateEmpty,
            location: location(root, 1),
            suggested_display_name: "new project".to_owned(),
            location_exists: false,
            location_empty: true,
            source_identity: None,
            prospective_parent_identity: Some(WorkspaceSourceIdentity::new([9; 32])),
            workspace_kind: WorkspaceKind::Folder,
            availability: WorkspaceAvailability::Missing,
            access_mode: WorkspaceAccessMode::ReadWrite,
            portable_metadata_state: PortableProjectMetadataState::Absent,
            portable_project_id: None,
            shared_memory_state: SharedProjectMemoryState::Absent,
            minimal_structure_creation_available: true,
            instruction_fingerprint: None,
            instruction_source_count: 0,
            instruction_discovery_incomplete: false,
            observed_at_unix_ms: 10,
            expires_at_unix_ms: 1_000,
        }
    }

    async fn commit_create_minimal(
        store: &SqliteControlStore,
        root: &Path,
    ) -> (ProjectRegistrationCommit, ProjectRegistrationPlan) {
        let project_id = ProjectId::new();
        let inspection_id = ProjectInspectionId::new();
        let inspection = absent_create_inspection(root, inspection_id);
        store.save_inspection(inspection).await.unwrap();
        let plan = ProjectRegistrationPlan {
            operation_id: WorkspaceOperationId::new(),
            command_id: CommandId::new(),
            correlation_id: "test.registration".to_owned(),
            intent_sha256: [2; 32],
            inspection_id,
            target: ProjectRegistrationTarget::NewProject,
            project_id,
            binding_id: WorkspaceBindingId::new(),
            direct_session_id: SessionId::new(),
            display_name: "new project".to_owned(),
            location: location(root, 1),
            source_identity: None,
            workspace_kind: WorkspaceKind::Folder,
            availability: WorkspaceAvailability::Missing,
            access_mode: WorkspaceAccessMode::ReadWrite,
            portable_metadata_state: PortableProjectMetadataState::Absent,
            portable_project_id: None,
            portable_metadata_action: PortableMetadataAction::CreateMinimal,
            instruction_fingerprint: None,
            instruction_source_count: 0,
            initial_trust: None,
            prepared_at_unix_ms: 20,
        };
        let prepared = store.prepare_registration(plan.clone()).await.unwrap();
        assert_eq!(prepared.state, RegistrationOperationState::Prepared);
        assert!(store.get_project(project_id).await.unwrap().is_none());

        std::fs::create_dir_all(root).unwrap();
        let observation = RegistrationFilesystemObservation {
            location: location(root, 1),
            source_identity: Some(WorkspaceSourceIdentity::new([3; 32])),
            workspace_kind: WorkspaceKind::Folder,
            availability: WorkspaceAvailability::Available,
            access_mode: WorkspaceAccessMode::ReadWrite,
            portable_metadata_state: PortableProjectMetadataState::PresentValid,
            portable_project_id: Some(project_id),
            instruction_fingerprint: Some([4; 32]),
            instruction_source_count: 1,
            observed_at_unix_ms: 30,
        };
        store
            .record_registration_filesystem_applied(RegistrationFilesystemApplied {
                command_id: plan.command_id,
                expected_action: PortableMetadataAction::CreateMinimal,
                observation: observation.clone(),
                safe_code: "project.registration.filesystem_applied".to_owned(),
            })
            .await
            .unwrap();
        let commit = store
            .commit_registration(plan.command_id, 40)
            .await
            .unwrap();
        assert_eq!(
            commit.project.bindings[0].source_identity,
            observation.source_identity
        );
        assert_eq!(
            commit.project.bindings[0].portable_metadata_state,
            PortableProjectMetadataState::PresentValid
        );
        (commit, plan)
    }

    #[tokio::test]
    async fn absent_create_parent_and_final_facts_survive_restart() {
        let temp = tempdir().unwrap();
        let database = temp.path().join("control.sqlite");
        let root = temp.path().join("project");
        let store = SqliteControlStore::open(&database).await.unwrap();
        let inspection_id = ProjectInspectionId::new();
        let inspection = absent_create_inspection(&root, inspection_id);
        store.save_inspection(inspection.clone()).await.unwrap();
        store.close().await;

        let store = SqliteControlStore::open(&database).await.unwrap();
        let restored = store.load_inspection(inspection_id, 20).await.unwrap();
        assert_eq!(
            restored.prospective_parent_identity,
            inspection.prospective_parent_identity
        );
        assert!(restored.source_identity.is_none());
        let (commit, plan) = commit_create_minimal(&store, &root).await;
        let restored_operation = store
            .load_registration(plan.command_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            restored_operation.state,
            RegistrationOperationState::Committed
        );
        assert!(restored_operation.filesystem_observation.is_some());
        assert_eq!(
            commit.project.access_policy.trust_state,
            ProjectTrustState::Restricted
        );
        store.close().await;
    }

    #[tokio::test]
    async fn rebind_commits_final_observation_and_retries_from_receipt() {
        let temp = tempdir().unwrap();
        let database = temp.path().join("control.sqlite");
        let first_root = temp.path().join("first");
        let store = SqliteControlStore::open(&database).await.unwrap();
        let (created, _) = commit_create_minimal(&store, &first_root).await;
        let project_id = created.project.project.project_id;
        let current = created.project.bindings[0].clone();
        let policy_revision = created.project.access_policy.revision;

        let rebound_root = temp.path().join("rebound");
        std::fs::create_dir_all(&rebound_root).unwrap();
        let inspection_id = ProjectInspectionId::new();
        let final_observation = RegistrationFilesystemObservation {
            location: location(&rebound_root, 7),
            source_identity: Some(WorkspaceSourceIdentity::new([8; 32])),
            workspace_kind: WorkspaceKind::Folder,
            availability: WorkspaceAvailability::Available,
            access_mode: WorkspaceAccessMode::ReadWrite,
            portable_metadata_state: PortableProjectMetadataState::PresentValid,
            portable_project_id: Some(project_id),
            instruction_fingerprint: Some([6; 32]),
            instruction_source_count: 2,
            observed_at_unix_ms: 50,
        };
        store
            .save_inspection(ProjectLocationInspection {
                inspection_id,
                registration_kind: ProjectRegistrationKind::AttachExisting,
                location: final_observation.location.clone(),
                suggested_display_name: "rebound".to_owned(),
                location_exists: true,
                location_empty: false,
                source_identity: final_observation.source_identity,
                prospective_parent_identity: None,
                workspace_kind: final_observation.workspace_kind,
                availability: final_observation.availability,
                access_mode: final_observation.access_mode,
                portable_metadata_state: final_observation.portable_metadata_state,
                portable_project_id: final_observation.portable_project_id,
                shared_memory_state: SharedProjectMemoryState::Present,
                minimal_structure_creation_available: false,
                instruction_fingerprint: final_observation.instruction_fingerprint,
                instruction_source_count: final_observation.instruction_source_count,
                instruction_discovery_incomplete: false,
                observed_at_unix_ms: final_observation.observed_at_unix_ms,
                expires_at_unix_ms: 1_000,
            })
            .await
            .unwrap();
        let plan = ProjectWorkspaceRebindPlan {
            command_id: CommandId::new(),
            correlation_id: "test.rebind".to_owned(),
            intent_sha256: [5; 32],
            project_id,
            current_binding_id: current.binding_id,
            expected_current_binding_revision: current.record_revision,
            expected_current_source_identity: current.source_identity,
            replacement_binding_id: WorkspaceBindingId::new(),
            inspection_id,
            portable_metadata_action: RebindPortableMetadataAction::CreateMinimal,
            final_observation: final_observation.clone(),
            safe_code: "project.workspace.rebound".to_owned(),
            rebound_at_unix_ms: 60,
        };
        let receipt = store.rebind_project_workspace(plan.clone()).await.unwrap();
        let replay = store.rebind_project_workspace(plan).await.unwrap();
        assert_eq!(receipt, replay);
        assert_eq!(receipt.primary_binding.location, final_observation.location);
        assert_eq!(
            receipt.primary_binding.source_identity,
            final_observation.source_identity
        );
        let aggregate = store.get_project(project_id).await.unwrap().unwrap();
        assert_eq!(aggregate.access_policy.revision, policy_revision);
        assert_eq!(
            aggregate.project.primary_binding_id,
            receipt.primary_binding.binding_id
        );
        store.close().await;
    }

    #[tokio::test]
    async fn legacy_import_is_stable_and_does_not_rebind_on_restart_observation() {
        let directory = tempdir().unwrap();
        let store = SqliteControlStore::open(directory.path().join("control.sqlite3"))
            .await
            .unwrap();
        let project_id = ProjectId::new();
        let binding_id = WorkspaceBindingId::new();
        let root = directory.path().join("legacy-project");
        std::fs::create_dir(&root).unwrap();
        let import = LegacyProjectImport {
            project_id,
            binding_id,
            display_name: "legacy project".to_owned(),
            location: location(&root, 33),
            source_identity: Some(WorkspaceSourceIdentity::new([7; 32])),
            workspace_kind: WorkspaceKind::VersionedCheckout,
            availability: WorkspaceAvailability::Available,
            access_mode: WorkspaceAccessMode::ReadWrite,
            portable_metadata_state: PortableProjectMetadataState::PresentValid,
            portable_project_id: Some(project_id),
            instruction_fingerprint: Some([8; 32]),
            instruction_source_count: 2,
            claim_location: true,
            correlation_id: "test.legacy_import".to_owned(),
            imported_at_unix_ms: 100,
        };
        let first = store.import_legacy_project(import.clone()).await.unwrap();
        assert_eq!(
            first.access_policy.trust_state,
            ProjectTrustState::Restricted
        );
        assert_eq!(first.bindings.len(), 1);
        assert_eq!(first.bindings[0].portable_project_id, Some(project_id));
        assert_eq!(first.instruction_fingerprints.len(), 1);
        assert_eq!(first.instruction_fingerprints[0].sha256, [8; 32]);

        let replay = store
            .import_legacy_project(LegacyProjectImport {
                source_identity: Some(WorkspaceSourceIdentity::new([9; 32])),
                imported_at_unix_ms: 200,
                ..import
            })
            .await
            .unwrap();
        assert_eq!(
            replay.bindings[0].source_identity,
            Some(WorkspaceSourceIdentity::new([7; 32]))
        );
        assert_eq!(replay.project.revision, first.project.revision);
    }

    #[tokio::test]
    async fn detached_legacy_placeholder_does_not_claim_another_portable_project_location() {
        let directory = tempdir().unwrap();
        let store = SqliteControlStore::open(directory.path().join("control.sqlite3"))
            .await
            .unwrap();
        let root = directory.path().join("portable-project");
        std::fs::create_dir(&root).unwrap();
        let legacy_id = ProjectId::new();
        let detached_location = location(&root, 44);
        store
            .import_legacy_project(LegacyProjectImport {
                project_id: legacy_id,
                binding_id: WorkspaceBindingId::new(),
                display_name: "detached legacy history".to_owned(),
                location: detached_location.clone(),
                source_identity: None,
                workspace_kind: WorkspaceKind::VersionedCheckout,
                availability: WorkspaceAvailability::Detached,
                access_mode: WorkspaceAccessMode::ReadWrite,
                portable_metadata_state: PortableProjectMetadataState::IdentityConflict,
                portable_project_id: None,
                instruction_fingerprint: Some([0; 32]),
                instruction_source_count: 0,
                claim_location: false,
                correlation_id: "test.detached_legacy".to_owned(),
                imported_at_unix_ms: 100,
            })
            .await
            .unwrap();
        assert!(
            store
                .find_binding_by_location(detached_location.key)
                .await
                .unwrap()
                .is_none()
        );

        let inspection_id = ProjectInspectionId::new();
        let portable_id = ProjectId::new();
        store
            .save_inspection(ProjectLocationInspection {
                inspection_id,
                registration_kind: ProjectRegistrationKind::AttachExisting,
                location: detached_location.clone(),
                suggested_display_name: "portable project".to_owned(),
                location_exists: true,
                location_empty: false,
                source_identity: Some(WorkspaceSourceIdentity::new([45; 32])),
                prospective_parent_identity: None,
                workspace_kind: WorkspaceKind::VersionedCheckout,
                availability: WorkspaceAvailability::Available,
                access_mode: WorkspaceAccessMode::ReadWrite,
                portable_metadata_state: PortableProjectMetadataState::PresentValid,
                portable_project_id: Some(portable_id),
                shared_memory_state: SharedProjectMemoryState::Absent,
                minimal_structure_creation_available: false,
                instruction_fingerprint: Some([0; 32]),
                instruction_source_count: 0,
                instruction_discovery_incomplete: false,
                observed_at_unix_ms: 110,
                expires_at_unix_ms: 1_000,
            })
            .await
            .unwrap();
        let plan = ProjectRegistrationPlan {
            operation_id: WorkspaceOperationId::new(),
            command_id: CommandId::new(),
            correlation_id: "test.portable_after_legacy".to_owned(),
            intent_sha256: [46; 32],
            inspection_id,
            target: ProjectRegistrationTarget::NewProject,
            project_id: portable_id,
            binding_id: WorkspaceBindingId::new(),
            direct_session_id: SessionId::new(),
            display_name: "portable project".to_owned(),
            location: detached_location,
            source_identity: Some(WorkspaceSourceIdentity::new([45; 32])),
            workspace_kind: WorkspaceKind::VersionedCheckout,
            availability: WorkspaceAvailability::Available,
            access_mode: WorkspaceAccessMode::ReadWrite,
            portable_metadata_state: PortableProjectMetadataState::PresentValid,
            portable_project_id: Some(portable_id),
            portable_metadata_action: PortableMetadataAction::UseExisting,
            instruction_fingerprint: Some([0; 32]),
            instruction_source_count: 0,
            initial_trust: None,
            prepared_at_unix_ms: 120,
        };
        assert_eq!(
            store.prepare_registration(plan).await.unwrap().state,
            RegistrationOperationState::Prepared
        );
    }
}
