//! Stable identifiers and cross-process contracts for the Dennett skeleton.

use serde::{Deserialize, Serialize};
use std::{fmt, num::NonZeroU64};
use uuid::Uuid;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);
        impl $name {
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}

id_type!(ProjectId);
id_type!(ProjectInspectionId);
id_type!(WorkspaceBindingId);
id_type!(WorkspaceSnapshotId);
id_type!(WorkspaceOperationId);
id_type!(SessionId);
id_type!(TurnId);
id_type!(SessionEventId);
id_type!(TaskId);
id_type!(RunId);
id_type!(CommandId);
id_type!(CommandReceiptId);
id_type!(TestReceiptId);
id_type!(ArtifactId);
id_type!(CheckpointId);
id_type!(ReviewId);
id_type!(ReviewCommentId);
id_type!(MemoryEventId);
id_type!(DeviceId);
id_type!(EffectId);

/// A slash-separated path relative to one authoritative workspace binding.
///
/// This type rejects lexical escape forms at the contract boundary. The
/// Workspace Manager must still perform handle-relative canonicalization and
/// symlink/junction checks before filesystem access.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ProjectRelativePath(String);

impl ProjectRelativePath {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ProjectRelativePath {
    type Error = ProjectRelativePathError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_from(value.as_str()).map(|_| Self(value))
    }
}

impl TryFrom<&str> for ProjectRelativePath {
    type Error = ProjectRelativePathError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        validate_project_relative_path(value)?;
        Ok(Self(value.to_owned()))
    }
}

impl From<ProjectRelativePath> for String {
    fn from(value: ProjectRelativePath) -> Self {
        value.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectRelativePathError {
    Empty,
    Absolute,
    NonCanonicalSeparator,
    TooLong,
    EmptySegment,
    SegmentTooLong,
    DotSegment,
    WindowsDrivePrefix,
    WindowsAlternateDataStream,
    WindowsShortNameAlias,
    WindowsReservedName,
    WindowsAmbiguousSuffix,
    ControlCharacter,
}

impl fmt::Display for ProjectRelativePathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Empty => "project-relative path is empty",
            Self::Absolute => "project-relative path is absolute",
            Self::NonCanonicalSeparator => "project-relative path must use forward slashes",
            Self::TooLong => "project-relative path exceeds the portable length bound",
            Self::EmptySegment => "project-relative path contains an empty segment",
            Self::SegmentTooLong => {
                "project-relative path contains a segment above the portable length bound"
            }
            Self::DotSegment => "project-relative path contains a dot segment",
            Self::WindowsDrivePrefix => "project-relative path contains a Windows drive prefix",
            Self::WindowsAlternateDataStream => {
                "project-relative path contains a Windows alternate data stream separator"
            }
            Self::WindowsShortNameAlias => {
                "project-relative path may resolve through a Windows short-name alias"
            }
            Self::WindowsReservedName => {
                "project-relative path contains a Windows reserved device name"
            }
            Self::WindowsAmbiguousSuffix => {
                "project-relative path contains a Windows-ambiguous trailing dot or space"
            }
            Self::ControlCharacter => "project-relative path contains a control character",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ProjectRelativePathError {}

fn validate_project_relative_path(value: &str) -> Result<(), ProjectRelativePathError> {
    if value.is_empty() {
        return Err(ProjectRelativePathError::Empty);
    }
    if value.len() > 4096 {
        return Err(ProjectRelativePathError::TooLong);
    }
    if value.starts_with('/') {
        return Err(ProjectRelativePathError::Absolute);
    }
    if value.contains('\\') {
        return Err(ProjectRelativePathError::NonCanonicalSeparator);
    }
    if value.chars().any(char::is_control) {
        return Err(ProjectRelativePathError::ControlCharacter);
    }

    let mut segments = value.split('/');
    let Some(first) = segments.next() else {
        return Err(ProjectRelativePathError::Empty);
    };
    if is_windows_drive_prefix(first) {
        return Err(ProjectRelativePathError::WindowsDrivePrefix);
    }
    for segment in std::iter::once(first).chain(segments) {
        if segment.is_empty() {
            return Err(ProjectRelativePathError::EmptySegment);
        }
        if segment.len() > 255 {
            return Err(ProjectRelativePathError::SegmentTooLong);
        }
        if matches!(segment, "." | "..") {
            return Err(ProjectRelativePathError::DotSegment);
        }
        if segment.contains(':') {
            return Err(ProjectRelativePathError::WindowsAlternateDataStream);
        }
        if contains_windows_short_name_alias(segment) {
            return Err(ProjectRelativePathError::WindowsShortNameAlias);
        }
        if segment.ends_with(['.', ' ']) {
            return Err(ProjectRelativePathError::WindowsAmbiguousSuffix);
        }
        if is_windows_reserved_name(segment) {
            return Err(ProjectRelativePathError::WindowsReservedName);
        }
    }
    Ok(())
}

fn contains_windows_short_name_alias(segment: &str) -> bool {
    segment
        .as_bytes()
        .windows(2)
        .any(|pair| pair[0] == b'~' && matches!(pair[1], b'1'..=b'9'))
}

fn is_windows_drive_prefix(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn is_windows_reserved_name(segment: &str) -> bool {
    let stem = segment.split('.').next().unwrap_or(segment);
    let upper = stem.to_ascii_uppercase();
    matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || matches!(
            upper.strip_prefix("COM"),
            Some("1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
        )
        || matches!(
            upper.strip_prefix("LPT"),
            Some("1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
        )
}

/// Exact, monotonic identity of one observed workspace state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceRevision {
    binding_id: WorkspaceBindingId,
    snapshot_id: WorkspaceSnapshotId,
    sequence: NonZeroU64,
}

impl WorkspaceRevision {
    pub fn new(
        binding_id: WorkspaceBindingId,
        snapshot_id: WorkspaceSnapshotId,
        sequence: u64,
    ) -> Result<Self, WorkspaceRevisionError> {
        let sequence = NonZeroU64::new(sequence).ok_or(WorkspaceRevisionError::ZeroSequence)?;
        Ok(Self {
            binding_id,
            snapshot_id,
            sequence,
        })
    }

    #[must_use]
    pub fn binding_id(self) -> WorkspaceBindingId {
        self.binding_id
    }

    #[must_use]
    pub fn snapshot_id(self) -> WorkspaceSnapshotId {
        self.snapshot_id
    }

    #[must_use]
    pub fn sequence(self) -> u64 {
        self.sequence.get()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceRevisionError {
    ZeroSequence,
}

impl fmt::Display for WorkspaceRevisionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("workspace revision sequence must be non-zero")
    }
}

impl std::error::Error for WorkspaceRevisionError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceBindingRef {
    pub project_id: ProjectId,
    pub binding_id: WorkspaceBindingId,
    pub device_id: DeviceId,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectTrustState {
    #[default]
    Restricted,
    TrustedBounded,
    Revoked,
}

impl ProjectTrustState {
    /// Fail-closed normalization for initial registration. Wire-level
    /// `UNSPECIFIED` is mapped to `None` by the ingress adapter.
    #[must_use]
    pub fn initial_or_restricted(requested: Option<Self>) -> Self {
        requested.unwrap_or(Self::Restricted)
    }

    /// Trust updates must name a real target state; wire-level `UNSPECIFIED`
    /// can never expand authority by falling through a default.
    pub fn require_explicit_update(
        requested: Option<Self>,
    ) -> Result<Self, ProjectTrustStateError> {
        requested.ok_or(ProjectTrustStateError::UnspecifiedUpdate)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectTrustStateError {
    UnspecifiedUpdate,
    MissingTrustDecision,
    MissingPolicyRevision,
    ExistingPolicyMustBePreserved,
    InvalidInitialState,
}

impl fmt::Display for ProjectTrustStateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::UnspecifiedUpdate => "project trust update requires an explicit target state",
            Self::MissingTrustDecision => "project trust change requires a trust decision",
            Self::MissingPolicyRevision => {
                "project trust update requires a non-zero policy revision"
            }
            Self::ExistingPolicyMustBePreserved => {
                "registration cannot initialize an existing local project policy"
            }
            Self::InvalidInitialState => "project registration has an invalid initial trust state",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ProjectTrustStateError {}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortableProjectMetadataState {
    #[default]
    Absent,
    PresentValid,
    Invalid,
    IdentityConflict,
    UnsupportedVersion,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortableMetadataAction {
    LeaveAbsent,
    UseExisting,
    CreateMinimal,
    ForkWithNewIdentity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RebindPortableMetadataAction {
    LeaveAbsent,
    UseExisting,
    CreateMinimal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectRegistrationIdentity {
    NewProject,
    ExistingLocalProject,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectRegistrationTrust {
    PreserveExisting,
    Initialize(ProjectTrustState),
}

/// Resolves initial policy without allowing a portable identity to overwrite
/// an existing local `project_id -> policy` record.
pub fn validate_project_registration_trust(
    identity: ProjectRegistrationIdentity,
    requested: Option<ProjectTrustState>,
    trust_decision_present: bool,
) -> Result<ProjectRegistrationTrust, ProjectTrustStateError> {
    if matches!(identity, ProjectRegistrationIdentity::ExistingLocalProject) {
        return if requested.is_none() && !trust_decision_present {
            Ok(ProjectRegistrationTrust::PreserveExisting)
        } else {
            Err(ProjectTrustStateError::ExistingPolicyMustBePreserved)
        };
    }

    let state = ProjectTrustState::initial_or_restricted(requested);
    if matches!(state, ProjectTrustState::Revoked) {
        return Err(ProjectTrustStateError::InvalidInitialState);
    }
    if matches!(state, ProjectTrustState::TrustedBounded) && !trust_decision_present {
        return Err(ProjectTrustStateError::MissingTrustDecision);
    }
    Ok(ProjectRegistrationTrust::Initialize(state))
}

/// Validates the compare-and-set preconditions for every local trust change.
pub fn validate_project_trust_update(
    requested: Option<ProjectTrustState>,
    expected_policy_revision: u64,
    trust_decision_present: bool,
) -> Result<ProjectTrustState, ProjectTrustStateError> {
    let state = ProjectTrustState::require_explicit_update(requested)?;
    if expected_policy_revision == 0 {
        return Err(ProjectTrustStateError::MissingPolicyRevision);
    }
    if !trust_decision_present {
        return Err(ProjectTrustStateError::MissingTrustDecision);
    }
    Ok(state)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceFailureKind {
    StaleSnapshot,
    ScopeDenied,
    Conflict,
    Cancelled,
    LocationMissing,
    AdapterRetryable,
    AdapterTerminal,
    Validation,
    RecoveryRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceOperationState {
    Accepted,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
    RecoveryRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancellationDisposition {
    RequestTermination,
    AlreadyTerminal,
}

/// Terminal workspace receipts are immutable even when cancellation races
/// with completion. The cancellation command reports a no-op instead of
/// rewriting success or failure history.
#[must_use]
pub fn cancellation_disposition(state: WorkspaceOperationState) -> CancellationDisposition {
    match state {
        WorkspaceOperationState::Accepted | WorkspaceOperationState::Running => {
            CancellationDisposition::RequestTermination
        }
        WorkspaceOperationState::Succeeded
        | WorkspaceOperationState::Failed
        | WorkspaceOperationState::Cancelled
        | WorkspaceOperationState::TimedOut
        | WorkspaceOperationState::RecoveryRequired => CancellationDisposition::AlreadyTerminal,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptTerminalArm {
    None,
    Success,
    Failure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionTerminalKind {
    Succeeded,
    Failed,
    TimedOut,
    Cancelled,
    SpawnFailed,
    RecoveryRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestOutcome {
    Passed,
    Failed,
    TimedOut,
    Cancelled,
    SpawnFailed,
    RecoveryRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReceiptValidationError {
    WorkspaceOperationState,
    CommandTerminal,
    TestTerminal,
}

impl fmt::Display for ReceiptValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::WorkspaceOperationState => {
                "workspace operation state contradicts its terminal arm"
            }
            Self::CommandTerminal => "command terminal kind contradicts failure presence",
            Self::TestTerminal => "test outcome contradicts failure presence",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ReceiptValidationError {}

/// Enforces the lifecycle matrix for the wire-level `WorkspaceOperationReceipt`.
pub fn validate_workspace_operation_receipt(
    state: WorkspaceOperationState,
    terminal: ReceiptTerminalArm,
) -> Result<(), ReceiptValidationError> {
    let valid = matches!(
        (state, terminal),
        (
            WorkspaceOperationState::Accepted | WorkspaceOperationState::Running,
            ReceiptTerminalArm::None
        ) | (
            WorkspaceOperationState::Succeeded,
            ReceiptTerminalArm::Success
        ) | (
            WorkspaceOperationState::Failed
                | WorkspaceOperationState::Cancelled
                | WorkspaceOperationState::TimedOut
                | WorkspaceOperationState::RecoveryRequired,
            ReceiptTerminalArm::Failure
        )
    );
    valid
        .then_some(())
        .ok_or(ReceiptValidationError::WorkspaceOperationState)
}

/// Enforces failure presence for the wire-level `CommandReceipt`.
pub fn validate_command_receipt(
    terminal: ExecutionTerminalKind,
    failure_present: bool,
) -> Result<(), ReceiptValidationError> {
    let valid = matches!(terminal, ExecutionTerminalKind::Succeeded) != failure_present;
    valid
        .then_some(())
        .ok_or(ReceiptValidationError::CommandTerminal)
}

/// Enforces failure presence for the wire-level `TestReceipt`. Assertion
/// failures are valid test results; transport/execution failures carry error
/// evidence. Staleness is checked independently against workspace revision.
pub fn validate_test_receipt(
    outcome: TestOutcome,
    failure_present: bool,
) -> Result<(), ReceiptValidationError> {
    let valid_result = matches!(outcome, TestOutcome::Passed | TestOutcome::Failed);
    (valid_result != failure_present)
        .then_some(())
        .ok_or(ReceiptValidationError::TestTerminal)
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeadEligibility {
    #[default]
    None,
    Emergency,
    Full,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryDeploymentMode {
    ClientCache,
    EmbeddedSingleDeviceCanonical,
    CanonicalService,
    FullReplicaCandidate,
    EmergencySubset,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceManifest {
    pub device_id: DeviceId,
    pub display_name: String,
    pub head_eligibility: HeadEligibility,
    pub memory_mode: MemoryDeploymentMode,
    pub authority_epoch_seen: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectChatCommand {
    pub command_id: CommandId,
    pub project_id: ProjectId,
    pub session_id: SessionId,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResultEnvelope {
    pub command_id: CommandId,
    pub summary: String,
    pub partial: bool,
    pub artifact_handles: Vec<String>,
    pub evidence_handles: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_relative_path_accepts_canonical_unicode_paths() {
        let path = ProjectRelativePath::try_from("src/модель/.rules.json").expect("valid path");

        assert_eq!(path.as_str(), "src/модель/.rules.json");
        let encoded = serde_json::to_string(&path).expect("serialize path");
        assert_eq!(encoded, r#""src/модель/.rules.json""#);
        assert_eq!(
            serde_json::from_str::<ProjectRelativePath>(&encoded).expect("deserialize path"),
            path
        );
    }

    #[test]
    fn project_relative_path_rejects_lexical_escape_forms() {
        let cases = [
            "",
            "/absolute",
            r"C:\\workspace\\file",
            "C:/workspace/file",
            r"folder\\file",
            "folder//file",
            "folder/./file",
            "folder/../file",
            "folder/file/",
            "folder/\0file",
        ];

        for value in cases {
            assert!(
                ProjectRelativePath::try_from(value).is_err(),
                "accepted unsafe path {value:?}"
            );
        }
    }

    #[test]
    fn project_relative_path_rejects_windows_aliases_on_every_host() {
        let cases = [
            "file.txt:secret",
            "CON",
            "con.txt",
            "folder/AUX.log",
            "COM1",
            "folder/lpt9.output",
            "GIT~1/hooks/pre-commit",
            "folder/DENNET~2/project.json",
            "trailing.",
            "folder/trailing ",
        ];

        for value in cases {
            assert!(
                ProjectRelativePath::try_from(value).is_err(),
                "accepted Windows-ambiguous path {value:?}"
            );
        }

        for value in [
            "console.txt",
            "COM10",
            "lpt0",
            "ordinary.name",
            "draft~backup.txt",
            "version~0.txt",
        ] {
            assert!(
                ProjectRelativePath::try_from(value).is_ok(),
                "rejected ordinary path {value:?}"
            );
        }
    }

    #[test]
    fn project_relative_path_enforces_portable_length_bounds() {
        let oversized_segment = "a".repeat(256);
        assert_eq!(
            ProjectRelativePath::try_from(oversized_segment.as_str()),
            Err(ProjectRelativePathError::SegmentTooLong)
        );

        let segment = "a".repeat(255);
        let oversized_path = std::iter::repeat_n(segment.as_str(), 17)
            .collect::<Vec<_>>()
            .join("/");
        assert!(oversized_path.len() > 4096);
        assert_eq!(
            ProjectRelativePath::try_from(oversized_path.as_str()),
            Err(ProjectRelativePathError::TooLong)
        );
    }

    #[test]
    fn workspace_revision_requires_a_committed_sequence() {
        let binding_id = WorkspaceBindingId::new();
        let snapshot_id = WorkspaceSnapshotId::new();

        assert_eq!(
            WorkspaceRevision::new(binding_id, snapshot_id, 0),
            Err(WorkspaceRevisionError::ZeroSequence)
        );

        let revision = WorkspaceRevision::new(binding_id, snapshot_id, 7).expect("revision");
        assert_eq!(revision.binding_id(), binding_id);
        assert_eq!(revision.snapshot_id(), snapshot_id);
        assert_eq!(revision.sequence(), 7);
        let encoded = serde_json::to_string(&revision).expect("serialize revision");
        assert_eq!(
            serde_json::from_str::<WorkspaceRevision>(&encoded).expect("deserialize revision"),
            revision
        );
    }

    #[test]
    fn portable_and_trust_states_have_stable_wire_names() {
        assert_eq!(
            serde_json::to_string(&PortableMetadataAction::CreateMinimal)
                .expect("serialize metadata action"),
            r#""create_minimal""#
        );
        assert_eq!(
            serde_json::to_string(&ProjectTrustState::TrustedBounded)
                .expect("serialize trust state"),
            r#""trusted_bounded""#
        );
        assert_eq!(
            serde_json::to_string(&WorkspaceFailureKind::AdapterRetryable)
                .expect("serialize failure kind"),
            r#""adapter_retryable""#
        );
        assert_eq!(
            serde_json::to_string(&PortableMetadataAction::ForkWithNewIdentity)
                .expect("serialize fork action"),
            r#""fork_with_new_identity""#
        );
        assert_eq!(
            serde_json::to_string(&RebindPortableMetadataAction::CreateMinimal)
                .expect("serialize rebind action"),
            r#""create_minimal""#
        );
    }

    #[test]
    fn unspecified_trust_is_fail_closed_and_not_a_valid_update() {
        assert_eq!(
            ProjectTrustState::initial_or_restricted(None),
            ProjectTrustState::Restricted
        );
        assert_eq!(
            ProjectTrustState::initial_or_restricted(Some(ProjectTrustState::TrustedBounded)),
            ProjectTrustState::TrustedBounded
        );
        assert_eq!(
            ProjectTrustState::require_explicit_update(None),
            Err(ProjectTrustStateError::UnspecifiedUpdate)
        );

        assert_eq!(
            validate_project_registration_trust(
                ProjectRegistrationIdentity::ExistingLocalProject,
                None,
                false,
            ),
            Ok(ProjectRegistrationTrust::PreserveExisting)
        );
        assert_eq!(
            validate_project_registration_trust(
                ProjectRegistrationIdentity::ExistingLocalProject,
                Some(ProjectTrustState::Restricted),
                false,
            ),
            Err(ProjectTrustStateError::ExistingPolicyMustBePreserved)
        );
        assert_eq!(
            validate_project_registration_trust(
                ProjectRegistrationIdentity::NewProject,
                Some(ProjectTrustState::TrustedBounded),
                false,
            ),
            Err(ProjectTrustStateError::MissingTrustDecision)
        );
        assert_eq!(
            validate_project_registration_trust(
                ProjectRegistrationIdentity::NewProject,
                Some(ProjectTrustState::TrustedBounded),
                true,
            ),
            Ok(ProjectRegistrationTrust::Initialize(
                ProjectTrustState::TrustedBounded
            ))
        );
        assert_eq!(
            validate_project_registration_trust(
                ProjectRegistrationIdentity::NewProject,
                Some(ProjectTrustState::Revoked),
                true,
            ),
            Err(ProjectTrustStateError::InvalidInitialState)
        );

        assert_eq!(
            validate_project_trust_update(Some(ProjectTrustState::Restricted), 0, true),
            Err(ProjectTrustStateError::MissingPolicyRevision)
        );
        assert_eq!(
            validate_project_trust_update(Some(ProjectTrustState::Restricted), 4, false),
            Err(ProjectTrustStateError::MissingTrustDecision)
        );
        assert_eq!(
            validate_project_trust_update(Some(ProjectTrustState::Restricted), 4, true),
            Ok(ProjectTrustState::Restricted)
        );
    }

    #[test]
    fn cancellation_never_rewrites_an_existing_terminal_receipt() {
        assert_eq!(
            cancellation_disposition(WorkspaceOperationState::Running),
            CancellationDisposition::RequestTermination
        );
        for state in [
            WorkspaceOperationState::Succeeded,
            WorkspaceOperationState::Failed,
            WorkspaceOperationState::Cancelled,
            WorkspaceOperationState::TimedOut,
            WorkspaceOperationState::RecoveryRequired,
        ] {
            assert_eq!(
                cancellation_disposition(state),
                CancellationDisposition::AlreadyTerminal,
                "terminal state was not preserved: {state:?}"
            );
        }
    }

    #[test]
    fn receipt_state_matrices_reject_contradictory_terminals() {
        let operation_states = [
            WorkspaceOperationState::Accepted,
            WorkspaceOperationState::Running,
            WorkspaceOperationState::Succeeded,
            WorkspaceOperationState::Failed,
            WorkspaceOperationState::Cancelled,
            WorkspaceOperationState::TimedOut,
            WorkspaceOperationState::RecoveryRequired,
        ];
        let terminal_arms = [
            ReceiptTerminalArm::None,
            ReceiptTerminalArm::Success,
            ReceiptTerminalArm::Failure,
        ];
        for state in operation_states {
            for terminal in terminal_arms {
                let expected = matches!(
                    (state, terminal),
                    (
                        WorkspaceOperationState::Accepted | WorkspaceOperationState::Running,
                        ReceiptTerminalArm::None
                    ) | (
                        WorkspaceOperationState::Succeeded,
                        ReceiptTerminalArm::Success
                    ) | (
                        WorkspaceOperationState::Failed
                            | WorkspaceOperationState::Cancelled
                            | WorkspaceOperationState::TimedOut
                            | WorkspaceOperationState::RecoveryRequired,
                        ReceiptTerminalArm::Failure
                    )
                );
                assert_eq!(
                    validate_workspace_operation_receipt(state, terminal).is_ok(),
                    expected,
                    "unexpected workspace receipt result for {state:?}/{terminal:?}"
                );
            }
        }

        let command_kinds = [
            ExecutionTerminalKind::Succeeded,
            ExecutionTerminalKind::Failed,
            ExecutionTerminalKind::TimedOut,
            ExecutionTerminalKind::Cancelled,
            ExecutionTerminalKind::SpawnFailed,
            ExecutionTerminalKind::RecoveryRequired,
        ];
        for terminal in command_kinds {
            for failure_present in [false, true] {
                let expected =
                    matches!(terminal, ExecutionTerminalKind::Succeeded) != failure_present;
                assert_eq!(
                    validate_command_receipt(terminal, failure_present).is_ok(),
                    expected,
                    "unexpected command receipt result for {terminal:?}/{failure_present}"
                );
            }
        }

        let test_outcomes = [
            TestOutcome::Passed,
            TestOutcome::Failed,
            TestOutcome::TimedOut,
            TestOutcome::Cancelled,
            TestOutcome::SpawnFailed,
            TestOutcome::RecoveryRequired,
        ];
        for outcome in test_outcomes {
            for failure_present in [false, true] {
                let expected =
                    matches!(outcome, TestOutcome::Passed | TestOutcome::Failed) != failure_present;
                assert_eq!(
                    validate_test_receipt(outcome, failure_present).is_ok(),
                    expected,
                    "unexpected test receipt result for {outcome:?}/{failure_present}"
                );
            }
        }
    }
}
