//! Provider-neutral project registry contracts and trust invariants.

use async_trait::async_trait;
use dennett_contracts::{
    CommandId, PortableMetadataAction, PortableProjectMetadataState, ProjectId,
    ProjectInspectionId, ProjectTrustState, RebindPortableMetadataAction, SessionId,
    WorkspaceBindingId, WorkspaceOperationId,
};
use std::{fmt, path::Path};
use thiserror::Error;

const MAX_DISPLAY_NAME_BYTES: usize = 512;
const MAX_REFERENCE_BYTES: usize = 512;
const MAX_CORRELATION_BYTES: usize = 512;
const MAX_SAFE_CODE_BYTES: usize = 128;

/// StableRef kind accepted from the authenticated ProjectService bridge.
pub const BRIDGE_ATTESTED_PROJECT_TRUST_DECISION_KIND: &str = "project_trust_decision";

/// An absolute path stored only in the local profile.
///
/// `Debug` is intentionally redacted so routine diagnostics cannot leak the
/// owner's filesystem layout. The Node remains responsible for resolving the
/// path without following unsafe links before constructing this value.
#[derive(Clone, Eq, Hash, PartialEq)]
pub struct SensitiveAbsolutePath(String);

impl SensitiveAbsolutePath {
    pub fn new(value: impl Into<String>) -> Result<Self, ProjectRegistryError> {
        let value = value.into();
        if value.is_empty() || value.contains('\0') || !Path::new(&value).is_absolute() {
            return Err(ProjectRegistryError::InvalidInput(
                "workspace path must be an absolute local path",
            ));
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn expose_local(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SensitiveAbsolutePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SensitiveAbsolutePath(<redacted>)")
    }
}

/// OS-aware canonical location key produced by the Node.
///
/// The key is separate from the displayable path so SQLite can enforce one
/// binding per canonical location without placing raw paths in diagnostics or
/// uniqueness errors.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct CanonicalLocationKey([u8; 32]);

impl CanonicalLocationKey {
    #[must_use]
    pub const fn new(value: [u8; 32]) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for CanonicalLocationKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CanonicalLocationKey(<redacted>)")
    }
}

/// Opaque identity of the filesystem object at a workspace location.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WorkspaceSourceIdentity([u8; 32]);

impl WorkspaceSourceIdentity {
    #[must_use]
    pub const fn new(value: [u8; 32]) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalWorkspaceLocation {
    pub path: SensitiveAbsolutePath,
    pub key: CanonicalLocationKey,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceKind {
    Folder,
    VersionedCheckout,
    IsolatedCheckout,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceAvailability {
    Available,
    Missing,
    Inaccessible,
    Detached,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceAccessMode {
    ReadOnly,
    ReadWrite,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectRegistrationKind {
    CreateEmpty,
    AttachExisting,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SharedProjectMemoryState {
    Absent,
    Present,
    Invalid,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectRegistrationTarget {
    NewProject,
    ExistingProject,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistrationOperationState {
    Prepared,
    FilesystemApplied,
    Committed,
    RecoveryRequired,
}

impl RegistrationOperationState {
    #[must_use]
    pub fn can_transition_to(self, next: Self) -> bool {
        self == next
            || matches!(
                (self, next),
                (
                    Self::Prepared,
                    Self::FilesystemApplied | Self::RecoveryRequired
                ) | (
                    Self::FilesystemApplied,
                    Self::Committed | Self::RecoveryRequired
                ) | (
                    Self::RecoveryRequired,
                    Self::Prepared | Self::FilesystemApplied
                )
            )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectRecord {
    pub project_id: ProjectId,
    pub display_name: String,
    pub primary_binding_id: WorkspaceBindingId,
    pub revision: u64,
    pub created_at_unix_ms: u64,
    pub updated_at_unix_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceBinding {
    pub binding_id: WorkspaceBindingId,
    pub project_id: ProjectId,
    pub location: CanonicalWorkspaceLocation,
    pub source_identity: Option<WorkspaceSourceIdentity>,
    pub kind: WorkspaceKind,
    pub availability: WorkspaceAvailability,
    pub access_mode: WorkspaceAccessMode,
    pub portable_metadata_state: PortableProjectMetadataState,
    pub portable_project_id: Option<ProjectId>,
    pub primary: bool,
    pub record_revision: u64,
    pub created_at_unix_ms: u64,
    pub last_verified_at_unix_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectAccessPolicy {
    pub project_id: ProjectId,
    pub trust_state: ProjectTrustState,
    pub revision: u64,
    pub last_decision: Option<TrustDecisionRef>,
    pub updated_at_unix_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstructionFingerprint {
    pub project_id: ProjectId,
    pub binding_id: WorkspaceBindingId,
    pub sha256: [u8; 32],
    pub source_count: u32,
    pub revision: u64,
    pub observed_at_unix_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectAggregate {
    pub project: ProjectRecord,
    pub access_policy: ProjectAccessPolicy,
    pub bindings: Vec<WorkspaceBinding>,
    pub instruction_fingerprints: Vec<InstructionFingerprint>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectLocationInspection {
    pub inspection_id: ProjectInspectionId,
    pub registration_kind: ProjectRegistrationKind,
    pub location: CanonicalWorkspaceLocation,
    pub suggested_display_name: String,
    pub location_exists: bool,
    pub location_empty: bool,
    pub source_identity: Option<WorkspaceSourceIdentity>,
    pub prospective_parent_identity: Option<WorkspaceSourceIdentity>,
    pub workspace_kind: WorkspaceKind,
    pub availability: WorkspaceAvailability,
    pub access_mode: WorkspaceAccessMode,
    pub portable_metadata_state: PortableProjectMetadataState,
    pub portable_project_id: Option<ProjectId>,
    pub shared_memory_state: SharedProjectMemoryState,
    pub minimal_structure_creation_available: bool,
    pub instruction_fingerprint: Option<[u8; 32]>,
    pub instruction_source_count: u32,
    pub instruction_discovery_incomplete: bool,
    pub observed_at_unix_ms: u64,
    pub expires_at_unix_ms: u64,
}

impl ProjectLocationInspection {
    pub fn validate(&self) -> Result<(), ProjectRegistryError> {
        validate_display_name(&self.suggested_display_name)?;
        if self.expires_at_unix_ms <= self.observed_at_unix_ms {
            return Err(ProjectRegistryError::InvalidInput(
                "inspection expiry must follow its observation time",
            ));
        }
        if self.portable_project_id.is_some()
            && self.portable_metadata_state != PortableProjectMetadataState::PresentValid
        {
            return Err(ProjectRegistryError::InvalidInput(
                "portable project identity requires valid metadata",
            ));
        }
        if self.minimal_structure_creation_available
            && self.portable_metadata_state != PortableProjectMetadataState::Absent
        {
            return Err(ProjectRegistryError::InvalidInput(
                "minimal project structure is only available when metadata is absent",
            ));
        }
        if self.instruction_fingerprint.is_none() && self.instruction_source_count != 0 {
            return Err(ProjectRegistryError::InvalidInput(
                "instruction source count requires a fingerprint",
            ));
        }
        if self.location_exists {
            if self.source_identity.is_none() || self.prospective_parent_identity.is_some() {
                return Err(ProjectRegistryError::InvalidInput(
                    "an existing location requires root identity and no prospective parent identity",
                ));
            }
        } else {
            if self.source_identity.is_some() {
                return Err(ProjectRegistryError::InvalidInput(
                    "an absent location cannot have a root source identity",
                ));
            }
            if self.prospective_parent_identity.is_some()
                && self.registration_kind != ProjectRegistrationKind::CreateEmpty
            {
                return Err(ProjectRegistryError::InvalidInput(
                    "prospective parent identity is only valid for absent create",
                ));
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn is_expired_at(&self, now_unix_ms: u64) -> bool {
        now_unix_ms >= self.expires_at_unix_ms
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrustDecisionRef {
    pub kind: String,
    pub id: String,
}

impl TrustDecisionRef {
    pub fn new(
        kind: impl Into<String>,
        id: impl Into<String>,
    ) -> Result<Self, ProjectTrustDecisionError> {
        let value = Self {
            kind: kind.into(),
            id: id.into(),
        };
        value.validate()?;
        Ok(value)
    }

    fn validate(&self) -> Result<(), ProjectTrustDecisionError> {
        if !valid_reference_part(&self.kind) || !valid_reference_part(&self.id) {
            return Err(ProjectTrustDecisionError::InvalidReference);
        }
        Ok(())
    }
}

/// Verifies the narrow attestation produced by authenticated ProjectService
/// ingress. Authentication and authority-epoch checks happen before this pure
/// bridge. The decision cannot be copied from a project file because its ID
/// must equal the mutating command identity.
pub fn verify_bridge_attested_project_trust_decision(
    decision: TrustDecisionRef,
    command_id: CommandId,
    project_id: ProjectId,
    target_state: ProjectTrustState,
) -> Result<VerifiedProjectTrustGrant, ProjectTrustDecisionError> {
    decision.validate()?;
    if decision.kind != BRIDGE_ATTESTED_PROJECT_TRUST_DECISION_KIND {
        return Err(ProjectTrustDecisionError::InvalidKind);
    }
    if decision.id != command_id.0.to_string() {
        return Err(ProjectTrustDecisionError::CommandMismatch);
    }
    Ok(VerifiedProjectTrustGrant {
        decision,
        command_id,
        project_id,
        target_state,
    })
}

/// A trust grant produced only after authenticated bridge attestation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedProjectTrustGrant {
    decision: TrustDecisionRef,
    command_id: CommandId,
    project_id: ProjectId,
    target_state: ProjectTrustState,
}

impl VerifiedProjectTrustGrant {
    #[must_use]
    pub fn decision(&self) -> &TrustDecisionRef {
        &self.decision
    }

    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }

    #[must_use]
    pub const fn project_id(&self) -> ProjectId {
        self.project_id
    }

    #[must_use]
    pub const fn target_state(&self) -> ProjectTrustState {
        self.target_state
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectRegistrationPlan {
    pub operation_id: WorkspaceOperationId,
    pub command_id: CommandId,
    pub correlation_id: String,
    pub intent_sha256: [u8; 32],
    pub inspection_id: ProjectInspectionId,
    pub target: ProjectRegistrationTarget,
    pub project_id: ProjectId,
    pub binding_id: WorkspaceBindingId,
    pub direct_session_id: SessionId,
    pub display_name: String,
    pub location: CanonicalWorkspaceLocation,
    pub source_identity: Option<WorkspaceSourceIdentity>,
    pub workspace_kind: WorkspaceKind,
    pub availability: WorkspaceAvailability,
    pub access_mode: WorkspaceAccessMode,
    pub portable_metadata_state: PortableProjectMetadataState,
    pub portable_project_id: Option<ProjectId>,
    pub portable_metadata_action: PortableMetadataAction,
    pub instruction_fingerprint: Option<[u8; 32]>,
    pub instruction_source_count: u32,
    pub initial_trust: Option<VerifiedProjectTrustGrant>,
    pub prepared_at_unix_ms: u64,
}

impl ProjectRegistrationPlan {
    pub fn validate(&self) -> Result<(), ProjectRegistryError> {
        validate_display_name(&self.display_name)?;
        validate_correlation_id(&self.correlation_id)?;
        if self.portable_project_id.is_some()
            && self.portable_metadata_state != PortableProjectMetadataState::PresentValid
        {
            return Err(ProjectRegistryError::InvalidInput(
                "portable project identity requires valid metadata",
            ));
        }
        if self.instruction_fingerprint.is_none() && self.instruction_source_count != 0 {
            return Err(ProjectRegistryError::InvalidInput(
                "instruction source count requires a fingerprint",
            ));
        }
        match self.portable_metadata_action {
            PortableMetadataAction::LeaveAbsent
                if self.portable_metadata_state
                    == PortableProjectMetadataState::IdentityConflict =>
            {
                return Err(ProjectRegistryError::InvalidInput(
                    "conflicting portable identity cannot be ignored",
                ));
            }
            PortableMetadataAction::UseExisting
                if self.portable_metadata_state != PortableProjectMetadataState::PresentValid =>
            {
                return Err(ProjectRegistryError::InvalidInput(
                    "using portable metadata requires a valid identity",
                ));
            }
            PortableMetadataAction::CreateMinimal
                if self.portable_metadata_state != PortableProjectMetadataState::Absent =>
            {
                return Err(ProjectRegistryError::InvalidInput(
                    "minimal metadata can only be created when absent",
                ));
            }
            PortableMetadataAction::ForkWithNewIdentity
                if self.target != ProjectRegistrationTarget::NewProject =>
            {
                return Err(ProjectRegistryError::InvalidInput(
                    "fork registration must create a new logical project",
                ));
            }
            _ => {}
        }
        match (self.target, &self.initial_trust) {
            (ProjectRegistrationTarget::ExistingProject, Some(_)) => {
                return Err(ProjectRegistryError::InvalidInput(
                    "an existing project policy must be preserved during registration",
                ));
            }
            (ProjectRegistrationTarget::NewProject, Some(grant))
                if grant.project_id() != self.project_id
                    || grant.command_id() != self.command_id
                    || grant.target_state() == ProjectTrustState::Revoked =>
            {
                return Err(ProjectRegistryError::InvalidInput(
                    "initial trust grant does not match the new project",
                ));
            }
            _ => {}
        }
        Ok(())
    }

    #[must_use]
    pub fn initial_trust_state(&self) -> ProjectTrustState {
        self.initial_trust
            .as_ref()
            .map_or(ProjectTrustState::Restricted, |grant| grant.target_state())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectRegistrationOperation {
    pub plan: ProjectRegistrationPlan,
    pub filesystem_observation: Option<RegistrationFilesystemObservation>,
    pub state: RegistrationOperationState,
    pub safe_code: String,
    pub created_at_unix_ms: u64,
    pub updated_at_unix_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistrationFilesystemObservation {
    pub location: CanonicalWorkspaceLocation,
    pub source_identity: Option<WorkspaceSourceIdentity>,
    pub workspace_kind: WorkspaceKind,
    pub availability: WorkspaceAvailability,
    pub access_mode: WorkspaceAccessMode,
    pub portable_metadata_state: PortableProjectMetadataState,
    pub portable_project_id: Option<ProjectId>,
    pub instruction_fingerprint: Option<[u8; 32]>,
    pub instruction_source_count: u32,
    pub observed_at_unix_ms: u64,
}

impl RegistrationFilesystemObservation {
    /// Compares durable effect facts while allowing an idempotent retry to
    /// observe the same filesystem state at a later instant.
    #[must_use]
    pub fn matches_effect_facts(&self, other: &Self) -> bool {
        self.location == other.location
            && self.source_identity == other.source_identity
            && self.workspace_kind == other.workspace_kind
            && self.availability == other.availability
            && self.access_mode == other.access_mode
            && self.portable_metadata_state == other.portable_metadata_state
            && self.portable_project_id == other.portable_project_id
            && self.instruction_fingerprint == other.instruction_fingerprint
            && self.instruction_source_count == other.instruction_source_count
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistrationFilesystemApplied {
    pub command_id: CommandId,
    pub expected_action: PortableMetadataAction,
    pub observation: RegistrationFilesystemObservation,
    pub safe_code: String,
}

impl RegistrationFilesystemApplied {
    pub fn validate_for(
        &self,
        operation: &ProjectRegistrationOperation,
    ) -> Result<(), ProjectRegistryError> {
        validate_safe_code(&self.safe_code)?;
        if self.command_id != operation.plan.command_id {
            return Err(ProjectRegistryError::IdempotencyConflict);
        }
        if self.expected_action != operation.plan.portable_metadata_action {
            return Err(ProjectRegistryError::IdempotencyConflict);
        }
        if self.observation.location != operation.plan.location {
            return Err(ProjectRegistryError::InvalidInput(
                "filesystem result belongs to another canonical location",
            ));
        }
        if self.observation.availability != WorkspaceAvailability::Available {
            return Err(ProjectRegistryError::InvalidInput(
                "filesystem-applied registration must observe an available location",
            ));
        }
        if self.observation.source_identity.is_none() {
            return Err(ProjectRegistryError::InvalidInput(
                "filesystem-applied registration requires a root source identity",
            ));
        }
        if self.observation.instruction_fingerprint.is_none()
            && self.observation.instruction_source_count != 0
        {
            return Err(ProjectRegistryError::InvalidInput(
                "instruction source count requires a final fingerprint",
            ));
        }
        let final_identity = (
            self.observation.portable_metadata_state,
            self.observation.portable_project_id,
        );
        match operation.plan.portable_metadata_action {
            PortableMetadataAction::LeaveAbsent => {
                if final_identity
                    != (
                        operation.plan.portable_metadata_state,
                        operation.plan.portable_project_id,
                    )
                {
                    return Err(ProjectRegistryError::InvalidInput(
                        "leave-absent effect changed portable metadata",
                    ));
                }
            }
            PortableMetadataAction::UseExisting => {
                if final_identity
                    != (
                        PortableProjectMetadataState::PresentValid,
                        Some(operation.plan.project_id),
                    )
                {
                    return Err(ProjectRegistryError::InvalidInput(
                        "existing portable identity does not match the project",
                    ));
                }
            }
            PortableMetadataAction::CreateMinimal | PortableMetadataAction::ForkWithNewIdentity => {
                if final_identity
                    != (
                        PortableProjectMetadataState::PresentValid,
                        Some(operation.plan.project_id),
                    )
                {
                    return Err(ProjectRegistryError::InvalidInput(
                        "portable metadata effect did not create the expected project identity",
                    ));
                }
            }
        }
        Ok(())
    }
}

impl ProjectRegistrationOperation {
    #[must_use]
    pub fn matches_plan(&self, plan: &ProjectRegistrationPlan) -> bool {
        self.plan == *plan
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectRegistrationCommit {
    pub operation: ProjectRegistrationOperation,
    pub project: ProjectAggregate,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistrationStateUpdate {
    pub command_id: CommandId,
    pub expected_state: RegistrationOperationState,
    pub target_state: RegistrationOperationState,
    pub safe_code: String,
    pub updated_at_unix_ms: u64,
}

impl RegistrationStateUpdate {
    pub fn validate(&self) -> Result<(), ProjectRegistryError> {
        validate_safe_code(&self.safe_code)?;
        if matches!(
            self.target_state,
            RegistrationOperationState::FilesystemApplied | RegistrationOperationState::Committed
        ) {
            return Err(ProjectRegistryError::InvalidInput(
                "registration filesystem/commit transitions require typed atomic APIs",
            ));
        }
        if !self.expected_state.can_transition_to(self.target_state) {
            return Err(ProjectRegistryError::InvalidStateTransition);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectAccessPolicyUpdate {
    pub project_id: ProjectId,
    pub expected_revision: u64,
    pub grant: VerifiedProjectTrustGrant,
    pub command_id: CommandId,
    pub correlation_id: String,
    pub updated_at_unix_ms: u64,
}

impl ProjectAccessPolicyUpdate {
    pub fn validate(&self) -> Result<(), ProjectRegistryError> {
        validate_correlation_id(&self.correlation_id)?;
        if self.expected_revision == 0 {
            return Err(ProjectRegistryError::InvalidInput(
                "policy update requires a non-zero expected revision",
            ));
        }
        if self.grant.project_id() != self.project_id {
            return Err(ProjectRegistryError::TrustDecisionRejected);
        }
        if self.grant.command_id() != self.command_id {
            return Err(ProjectRegistryError::TrustDecisionRejected);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectWorkspaceRebindPlan {
    pub command_id: CommandId,
    pub correlation_id: String,
    pub intent_sha256: [u8; 32],
    pub project_id: ProjectId,
    pub current_binding_id: WorkspaceBindingId,
    pub expected_current_binding_revision: u64,
    pub expected_current_source_identity: Option<WorkspaceSourceIdentity>,
    /// Equal to current_binding_id to update it, or a fresh ID to replace it.
    pub replacement_binding_id: WorkspaceBindingId,
    pub inspection_id: ProjectInspectionId,
    pub portable_metadata_action: RebindPortableMetadataAction,
    pub final_observation: RegistrationFilesystemObservation,
    pub safe_code: String,
    pub rebound_at_unix_ms: u64,
}

impl ProjectWorkspaceRebindPlan {
    pub fn validate(&self) -> Result<(), ProjectRegistryError> {
        validate_correlation_id(&self.correlation_id)?;
        validate_safe_code(&self.safe_code)?;
        if self.expected_current_binding_revision == 0 {
            return Err(ProjectRegistryError::InvalidInput(
                "rebind requires a non-zero current binding revision",
            ));
        }
        if self.final_observation.availability != WorkspaceAvailability::Available
            || self.final_observation.source_identity.is_none()
        {
            return Err(ProjectRegistryError::InvalidInput(
                "rebind requires an available final location with a root source identity",
            ));
        }
        if self.final_observation.instruction_fingerprint.is_none()
            && self.final_observation.instruction_source_count != 0
        {
            return Err(ProjectRegistryError::InvalidInput(
                "instruction source count requires a final fingerprint",
            ));
        }
        match self.portable_metadata_action {
            RebindPortableMetadataAction::LeaveAbsent => {
                if self.final_observation.portable_metadata_state
                    != PortableProjectMetadataState::Absent
                    || self.final_observation.portable_project_id.is_some()
                    || self.final_observation.source_identity
                        != self.expected_current_source_identity
                {
                    return Err(ProjectRegistryError::SourceIdentityConflict);
                }
            }
            RebindPortableMetadataAction::UseExisting
            | RebindPortableMetadataAction::CreateMinimal => {
                if self.final_observation.portable_metadata_state
                    != PortableProjectMetadataState::PresentValid
                    || self.final_observation.portable_project_id != Some(self.project_id)
                {
                    return Err(ProjectRegistryError::PortableProjectConflict);
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectWorkspaceRebindReceipt {
    pub command_id: CommandId,
    pub correlation_id: String,
    pub intent_sha256: [u8; 32],
    pub project_id: ProjectId,
    pub previous_binding_id: WorkspaceBindingId,
    pub primary_binding: WorkspaceBinding,
    pub project_revision: u64,
    pub rebound_at_unix_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BindingObservationUpdate {
    pub binding_id: WorkspaceBindingId,
    pub expected_revision: u64,
    pub availability: WorkspaceAvailability,
    pub access_mode: WorkspaceAccessMode,
    pub observed_source_identity: Option<WorkspaceSourceIdentity>,
    pub portable_metadata_state: PortableProjectMetadataState,
    pub portable_project_id: Option<ProjectId>,
    pub command_id: Option<CommandId>,
    pub correlation_id: String,
    pub safe_code: String,
    pub verified_at_unix_ms: u64,
}

impl BindingObservationUpdate {
    pub fn validate(&self) -> Result<(), ProjectRegistryError> {
        if self.expected_revision == 0 {
            return Err(ProjectRegistryError::InvalidInput(
                "binding update requires a non-zero expected revision",
            ));
        }
        validate_correlation_id(&self.correlation_id)?;
        validate_safe_code(&self.safe_code)?;
        if self.portable_project_id.is_some()
            && self.portable_metadata_state != PortableProjectMetadataState::PresentValid
        {
            return Err(ProjectRegistryError::InvalidInput(
                "portable project identity requires valid metadata",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstructionFingerprintUpdate {
    pub project_id: ProjectId,
    pub binding_id: WorkspaceBindingId,
    /// Zero means no fingerprint exists yet; otherwise this is exact CAS.
    pub expected_revision: u64,
    pub sha256: [u8; 32],
    pub source_count: u32,
    pub command_id: Option<CommandId>,
    pub correlation_id: String,
    pub safe_code: String,
    pub observed_at_unix_ms: u64,
}

impl InstructionFingerprintUpdate {
    pub fn validate(&self) -> Result<(), ProjectRegistryError> {
        validate_correlation_id(&self.correlation_id)?;
        validate_safe_code(&self.safe_code)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyProjectImport {
    pub project_id: ProjectId,
    pub binding_id: WorkspaceBindingId,
    pub display_name: String,
    pub location: CanonicalWorkspaceLocation,
    pub source_identity: Option<WorkspaceSourceIdentity>,
    pub workspace_kind: WorkspaceKind,
    pub availability: WorkspaceAvailability,
    pub access_mode: WorkspaceAccessMode,
    pub portable_metadata_state: PortableProjectMetadataState,
    pub portable_project_id: Option<ProjectId>,
    pub instruction_fingerprint: Option<[u8; 32]>,
    pub instruction_source_count: u32,
    /// False only for the detached M01 compatibility placeholder when the
    /// configured path already proves it belongs to another portable Project
    /// ID. The placeholder preserves old session scope without reserving that
    /// other project's canonical location.
    pub claim_location: bool,
    pub correlation_id: String,
    pub imported_at_unix_ms: u64,
}

impl LegacyProjectImport {
    pub fn validate(&self) -> Result<(), ProjectRegistryError> {
        validate_display_name(&self.display_name)?;
        validate_correlation_id(&self.correlation_id)?;
        if self.portable_project_id.is_some()
            && self.portable_metadata_state != PortableProjectMetadataState::PresentValid
        {
            return Err(ProjectRegistryError::InvalidInput(
                "portable project identity requires valid metadata",
            ));
        }
        if self.portable_metadata_state == PortableProjectMetadataState::PresentValid
            && self.portable_project_id != Some(self.project_id)
        {
            return Err(ProjectRegistryError::PortableProjectConflict);
        }
        if self.instruction_fingerprint.is_none() && self.instruction_source_count != 0 {
            return Err(ProjectRegistryError::InvalidInput(
                "instruction source count requires a fingerprint",
            ));
        }
        if !self.claim_location
            && (self.availability != WorkspaceAvailability::Detached
                || self.source_identity.is_some())
        {
            return Err(ProjectRegistryError::InvalidInput(
                "an unclaimed legacy binding must be detached and source-less",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectLifecycleEventKind {
    LegacyProjectImported,
    RegistrationPrepared,
    RegistrationFilesystemApplied,
    RegistrationCommitted,
    RegistrationRecoveryRequired,
    TrustChanged,
    BindingObservationUpdated,
    InstructionFingerprintChanged,
    WorkspaceRebound,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectLifecycleEvent {
    pub sequence: u64,
    pub kind: ProjectLifecycleEventKind,
    pub project_id: ProjectId,
    pub command_id: Option<CommandId>,
    pub correlation_id: String,
    pub safe_code: String,
    pub project_revision: Option<u64>,
    pub policy_revision: Option<u64>,
    pub binding_revision: Option<u64>,
    pub occurred_at_unix_ms: u64,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ProjectTrustDecisionError {
    #[error("trust decision reference is invalid")]
    InvalidReference,
    #[error("trust decision kind is not bridge-attested project trust")]
    InvalidKind,
    #[error("trust decision identity does not match the mutating command")]
    CommandMismatch,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ProjectRegistryError {
    #[error("project registry input is invalid: {0}")]
    InvalidInput(&'static str),
    #[error("project registry entity was not found: {0}")]
    NotFound(&'static str),
    #[error("project location inspection has expired")]
    InspectionExpired,
    #[error("project registry revision conflict: expected {expected}, actual {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
    #[error("project registration identity was reused for different intent")]
    IdempotencyConflict,
    #[error("canonical project location is already bound")]
    CanonicalLocationConflict {
        existing_project_id: ProjectId,
        existing_binding_id: WorkspaceBindingId,
    },
    #[error("project already exists")]
    ProjectAlreadyExists,
    #[error("workspace binding does not belong to the requested project")]
    BindingProjectMismatch,
    #[error("workspace source identity changed")]
    SourceIdentityConflict,
    #[error("portable project identity conflicts with the requested project")]
    PortableProjectConflict,
    #[error("project registration state transition is invalid")]
    InvalidStateTransition,
    #[error("project trust decision is missing or does not match the update")]
    TrustDecisionRejected,
    #[error("project registry storage is unavailable")]
    StorageUnavailable,
    #[error("project registry failed an integrity check: {0}")]
    IntegrityFailure(&'static str),
}

#[async_trait]
pub trait ProjectRegistryPort: Send + Sync {
    async fn get_project(
        &self,
        project_id: ProjectId,
    ) -> Result<Option<ProjectAggregate>, ProjectRegistryError>;

    async fn list_projects(&self) -> Result<Vec<ProjectAggregate>, ProjectRegistryError>;

    async fn find_binding_by_location(
        &self,
        location_key: CanonicalLocationKey,
    ) -> Result<Option<WorkspaceBinding>, ProjectRegistryError>;

    async fn save_inspection(
        &self,
        inspection: ProjectLocationInspection,
    ) -> Result<ProjectLocationInspection, ProjectRegistryError>;

    async fn load_inspection(
        &self,
        inspection_id: ProjectInspectionId,
        now_unix_ms: u64,
    ) -> Result<ProjectLocationInspection, ProjectRegistryError>;

    async fn prepare_registration(
        &self,
        plan: ProjectRegistrationPlan,
    ) -> Result<ProjectRegistrationOperation, ProjectRegistryError>;

    async fn load_registration(
        &self,
        command_id: CommandId,
    ) -> Result<Option<ProjectRegistrationOperation>, ProjectRegistryError>;

    async fn list_reconcilable_registrations(
        &self,
    ) -> Result<Vec<ProjectRegistrationOperation>, ProjectRegistryError>;

    async fn update_registration_state(
        &self,
        update: RegistrationStateUpdate,
    ) -> Result<ProjectRegistrationOperation, ProjectRegistryError>;

    async fn record_registration_filesystem_applied(
        &self,
        update: RegistrationFilesystemApplied,
    ) -> Result<ProjectRegistrationOperation, ProjectRegistryError>;

    /// Atomically publishes project/binding/policy/fingerprint after the
    /// external filesystem step has durably reached FilesystemApplied.
    async fn commit_registration(
        &self,
        command_id: CommandId,
        committed_at_unix_ms: u64,
    ) -> Result<ProjectRegistrationCommit, ProjectRegistryError>;

    async fn import_legacy_project(
        &self,
        import: LegacyProjectImport,
    ) -> Result<ProjectAggregate, ProjectRegistryError>;

    async fn compare_and_set_access_policy(
        &self,
        update: ProjectAccessPolicyUpdate,
    ) -> Result<ProjectAccessPolicy, ProjectRegistryError>;

    async fn compare_and_set_binding_observation(
        &self,
        update: BindingObservationUpdate,
    ) -> Result<WorkspaceBinding, ProjectRegistryError>;

    async fn rebind_project_workspace(
        &self,
        plan: ProjectWorkspaceRebindPlan,
    ) -> Result<ProjectWorkspaceRebindReceipt, ProjectRegistryError>;

    async fn compare_and_set_instruction_fingerprint(
        &self,
        update: InstructionFingerprintUpdate,
    ) -> Result<InstructionFingerprint, ProjectRegistryError>;

    async fn list_lifecycle_events(
        &self,
        project_id: ProjectId,
    ) -> Result<Vec<ProjectLifecycleEvent>, ProjectRegistryError>;
}

fn validate_display_name(value: &str) -> Result<(), ProjectRegistryError> {
    if value.trim().is_empty() || value.len() > MAX_DISPLAY_NAME_BYTES || value.contains('\0') {
        return Err(ProjectRegistryError::InvalidInput(
            "project display name is empty or too large",
        ));
    }
    Ok(())
}

fn validate_correlation_id(value: &str) -> Result<(), ProjectRegistryError> {
    if value.trim().is_empty() || value.len() > MAX_CORRELATION_BYTES || value.contains('\0') {
        return Err(ProjectRegistryError::InvalidInput(
            "correlation identity is empty or too large",
        ));
    }
    Ok(())
}

fn validate_safe_code(value: &str) -> Result<(), ProjectRegistryError> {
    if value.is_empty()
        || value.len() > MAX_SAFE_CODE_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(ProjectRegistryError::InvalidInput(
            "lifecycle safe code contains unsupported characters",
        ));
    }
    Ok(())
}

fn valid_reference_part(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= MAX_REFERENCE_BYTES
        && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_path_debug_is_redacted() {
        let path = SensitiveAbsolutePath::new(
            std::env::current_dir()
                .expect("current directory")
                .to_string_lossy()
                .into_owned(),
        )
        .expect("absolute path");
        let rendered = format!("{path:?}");
        assert!(!rendered.contains(path.expose_local()));
        assert!(rendered.contains("redacted"));
    }

    #[test]
    fn bridge_attested_trust_is_bound_to_the_mutating_command() {
        let project_id = ProjectId::new();
        let command_id = CommandId::new();
        let grant = verify_bridge_attested_project_trust_decision(
            TrustDecisionRef::new(
                BRIDGE_ATTESTED_PROJECT_TRUST_DECISION_KIND,
                command_id.0.to_string(),
            )
            .expect("decision"),
            command_id,
            project_id,
            ProjectTrustState::TrustedBounded,
        )
        .expect("verified grant");
        assert_eq!(grant.project_id(), project_id);
        assert_eq!(grant.command_id(), command_id);
        assert_eq!(grant.target_state(), ProjectTrustState::TrustedBounded);
    }

    #[test]
    fn bridge_attested_trust_rejects_wrong_kind_and_command() {
        let command_id = CommandId::new();
        assert_eq!(
            verify_bridge_attested_project_trust_decision(
                TrustDecisionRef::new("project_file", command_id.0.to_string()).expect("reference"),
                command_id,
                ProjectId::new(),
                ProjectTrustState::TrustedBounded,
            ),
            Err(ProjectTrustDecisionError::InvalidKind)
        );
        assert_eq!(
            verify_bridge_attested_project_trust_decision(
                TrustDecisionRef::new(
                    BRIDGE_ATTESTED_PROJECT_TRUST_DECISION_KIND,
                    CommandId::new().0.to_string(),
                )
                .expect("reference"),
                command_id,
                ProjectId::new(),
                ProjectTrustState::TrustedBounded,
            ),
            Err(ProjectTrustDecisionError::CommandMismatch)
        );
    }

    #[test]
    fn registration_state_machine_is_bounded_and_recoverable() {
        assert!(
            RegistrationOperationState::Prepared
                .can_transition_to(RegistrationOperationState::FilesystemApplied)
        );
        assert!(
            RegistrationOperationState::FilesystemApplied
                .can_transition_to(RegistrationOperationState::RecoveryRequired)
        );
        assert!(
            RegistrationOperationState::RecoveryRequired
                .can_transition_to(RegistrationOperationState::FilesystemApplied)
        );
        assert!(
            !RegistrationOperationState::Committed
                .can_transition_to(RegistrationOperationState::Prepared)
        );
    }
}
