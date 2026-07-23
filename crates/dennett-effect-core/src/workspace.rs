//! Provider-neutral workspace effect decisions.
//!
//! This module owns no filesystem handles and performs no I/O. It turns a
//! complete, versioned manifest plus resolved content handles into an exact
//! before/after plan. The Node adapter remains responsible for no-follow
//! traversal, durable publication and observing the actual result.

use async_trait::async_trait;
use dennett_contracts::{
    ArtifactId, CheckpointId, CommandId, EffectId, ProjectId, ProjectRelativePath,
    WorkspaceBindingId, WorkspaceOperationId, WorkspaceRevision,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_FILE_CHANGES_PER_OPERATION: usize = 128;
pub const MAX_STAGED_FILE_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_STAGED_OPERATION_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContentSha256(pub [u8; 32]);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MetadataSha256(pub [u8; 32]);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PortableFilePermissions {
    pub read_only: bool,
    pub executable: bool,
}

impl PortableFilePermissions {
    #[must_use]
    pub fn metadata_sha256(self) -> MetadataSha256 {
        let mut hasher = Sha256::new();
        hasher.update(b"dennett.regular-file-metadata.v1\0");
        hasher.update([u8::from(self.read_only), u8::from(self.executable)]);
        MetadataSha256(hasher.finalize().into())
    }

    #[must_use]
    pub fn from_metadata_sha256(value: MetadataSha256) -> Option<Self> {
        [false, true].into_iter().find_map(|read_only| {
            [false, true].into_iter().find_map(|executable| {
                let candidate = Self {
                    read_only,
                    executable,
                };
                (candidate.metadata_sha256() == value).then_some(candidate)
            })
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspacePathState {
    Absent,
    RegularFile {
        content_sha256: ContentSha256,
        metadata_sha256: MetadataSha256,
        byte_size: u64,
    },
    Directory {
        metadata_sha256: MetadataSha256,
    },
    Link {
        metadata_sha256: MetadataSha256,
    },
    Other {
        metadata_sha256: MetadataSha256,
    },
}

impl WorkspacePathState {
    #[must_use]
    pub const fn content_sha256(&self) -> Option<ContentSha256> {
        match self {
            Self::RegularFile { content_sha256, .. } => Some(*content_sha256),
            Self::Absent | Self::Directory { .. } | Self::Link { .. } | Self::Other { .. } => None,
        }
    }

    #[must_use]
    pub const fn is_regular_file(&self) -> bool {
        matches!(self, Self::RegularFile { .. })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceManifestEntry {
    pub path: ProjectRelativePath,
    pub state: WorkspacePathState,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceManifest {
    pub revision: WorkspaceRevision,
    /// Hash of the scan and exclusion policy used to produce this manifest.
    /// Two revisions with different scope policy cannot be compared as if they
    /// observed the same workspace surface.
    pub scope_sha256: ContentSha256,
    /// False means an adapter hit a deterministic bound or could not safely
    /// classify an entry. Such a manifest is useful for diagnostics but can
    /// never authorize a write.
    pub complete: bool,
    entries: Vec<WorkspaceManifestEntry>,
}

impl WorkspaceManifest {
    pub fn new(
        revision: WorkspaceRevision,
        scope_sha256: ContentSha256,
        complete: bool,
        mut entries: Vec<WorkspaceManifestEntry>,
    ) -> Result<Self, WorkspaceManifestError> {
        entries.sort_by(|left, right| left.path.as_str().cmp(right.path.as_str()));
        for pair in entries.windows(2) {
            if pair[0].path == pair[1].path {
                return Err(WorkspaceManifestError::DuplicatePath);
            }
        }
        if entries
            .iter()
            .any(|entry| matches!(entry.state, WorkspacePathState::Absent))
        {
            return Err(WorkspaceManifestError::AbsentEntry);
        }
        Ok(Self {
            revision,
            scope_sha256,
            complete,
            entries,
        })
    }

    #[must_use]
    pub fn entries(&self) -> &[WorkspaceManifestEntry] {
        &self.entries
    }

    #[must_use]
    pub fn has_same_observation(&self, other: &Self) -> bool {
        self.scope_sha256 == other.scope_sha256
            && self.complete == other.complete
            && self.entries == other.entries
    }

    #[must_use]
    pub fn state(&self, path: &ProjectRelativePath) -> WorkspacePathState {
        self.entries
            .binary_search_by(|entry| entry.path.as_str().cmp(path.as_str()))
            .ok()
            .map(|index| self.entries[index].state.clone())
            .unwrap_or(WorkspacePathState::Absent)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum WorkspaceManifestError {
    #[error("workspace manifest contains a duplicate path")]
    DuplicatePath,
    #[error("workspace manifest stores an explicit absent entry")]
    AbsentEntry,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StagedContentRef {
    pub content_id: String,
    pub content_sha256: ContentSha256,
    pub byte_size: u64,
}

impl StagedContentRef {
    pub fn validate(&self) -> Result<(), WorkspacePlanError> {
        if self.content_id.is_empty() || self.content_id.len() > 256 {
            return Err(WorkspacePlanError::InvalidContentReference);
        }
        if self.byte_size > MAX_STAGED_FILE_BYTES {
            return Err(WorkspacePlanError::FileContentTooLarge);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileMutationKind {
    Add,
    Modify,
    Delete,
    Rename,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolvedFileChangeProposal {
    pub kind: FileMutationKind,
    pub path: ProjectRelativePath,
    pub previous_path: Option<ProjectRelativePath>,
    pub content: Option<StagedContentRef>,
    pub expected_content_sha256: Option<ContentSha256>,
    /// Portable permissions the Node will apply to a materialized file. This
    /// lets restart reconciliation predict the exact after-image without
    /// persisting provider or OS-specific metadata types.
    pub resulting_permissions: Option<PortableFilePermissions>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkspacePathTransition {
    pub path: ProjectRelativePath,
    pub before: WorkspacePathState,
    pub after: WorkspacePathState,
    /// Present only when reaching `after` requires materializing bytes.
    pub content: Option<StagedContentRef>,
    pub resulting_permissions: Option<PortableFilePermissions>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PlannedFileChange {
    pub kind: FileMutationKind,
    pub path: ProjectRelativePath,
    pub previous_path: Option<ProjectRelativePath>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceFileEffectPlan {
    pub operation_id: WorkspaceOperationId,
    pub command_id: CommandId,
    pub correlation_id: String,
    pub project_id: ProjectId,
    pub binding_id: WorkspaceBindingId,
    pub base_revision: WorkspaceRevision,
    pub scope_sha256: ContentSha256,
    pub intent_sha256: ContentSha256,
    pub safety_checkpoint_id: CheckpointId,
    pub prepared_at_unix_ms: u64,
    pub changes: Vec<PlannedFileChange>,
    pub transitions: Vec<WorkspacePathTransition>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceFileEffectRequest {
    pub operation_id: WorkspaceOperationId,
    pub command_id: CommandId,
    pub correlation_id: String,
    pub project_id: ProjectId,
    pub binding_id: WorkspaceBindingId,
    pub base_revision: WorkspaceRevision,
    pub intent_sha256: ContentSha256,
    pub safety_checkpoint_id: CheckpointId,
    pub prepared_at_unix_ms: u64,
    pub changes: Vec<ResolvedFileChangeProposal>,
}

impl WorkspaceFileEffectPlan {
    pub fn build(
        manifest: &WorkspaceManifest,
        request: WorkspaceFileEffectRequest,
    ) -> Result<Self, WorkspacePlanError> {
        validate_request_header(manifest, &request)?;

        let mut touched = BTreeSet::new();
        let mut staged_content = BTreeMap::<String, StagedContentRef>::new();
        let mut changes = Vec::with_capacity(request.changes.len());
        let mut transitions = Vec::with_capacity(request.changes.len().saturating_mul(2));

        for proposal in request.changes {
            validate_mutable_path(&proposal.path)?;
            if !touched.insert(proposal.path.as_str().to_owned()) {
                return Err(WorkspacePlanError::DuplicateTouchedPath);
            }
            if let Some(content) = &proposal.content {
                content.validate()?;
                match staged_content.get(&content.content_id) {
                    Some(existing) if existing != content => {
                        return Err(WorkspacePlanError::ContentReferenceCollision);
                    }
                    Some(_) => {}
                    None => {
                        staged_content.insert(content.content_id.clone(), content.clone());
                    }
                }
            }

            let current = manifest.state(&proposal.path);
            let previous_path = proposal.previous_path.clone();
            match proposal.kind {
                FileMutationKind::Add => {
                    require_no_previous_or_expected(&proposal)?;
                    if current != WorkspacePathState::Absent {
                        return Err(WorkspacePlanError::TargetAlreadyExists);
                    }
                    let after = proposed_regular_file(&proposal)?;
                    transitions.push(WorkspacePathTransition {
                        path: proposal.path.clone(),
                        before: current,
                        after,
                        content: proposal.content.clone(),
                        resulting_permissions: proposal.resulting_permissions,
                    });
                }
                FileMutationKind::Modify => {
                    require_no_previous(&proposal)?;
                    require_regular_source(&current)?;
                    validate_expected_content(&proposal, &current)?;
                    let after = proposed_regular_file(&proposal)?;
                    transitions.push(WorkspacePathTransition {
                        path: proposal.path.clone(),
                        before: current,
                        after,
                        content: proposal.content.clone(),
                        resulting_permissions: proposal.resulting_permissions,
                    });
                }
                FileMutationKind::Delete => {
                    require_no_previous(&proposal)?;
                    require_no_content_or_metadata(&proposal)?;
                    require_regular_source(&current)?;
                    validate_expected_content(&proposal, &current)?;
                    transitions.push(WorkspacePathTransition {
                        path: proposal.path.clone(),
                        before: current,
                        after: WorkspacePathState::Absent,
                        content: None,
                        resulting_permissions: None,
                    });
                }
                FileMutationKind::Rename => {
                    require_no_content_or_metadata(&proposal)?;
                    let source_path = previous_path
                        .as_ref()
                        .ok_or(WorkspacePlanError::MissingPreviousPath)?;
                    validate_mutable_path(source_path)?;
                    if source_path == &proposal.path {
                        return Err(WorkspacePlanError::RenamePathUnchanged);
                    }
                    if !touched.insert(source_path.as_str().to_owned()) {
                        return Err(WorkspacePlanError::DuplicateTouchedPath);
                    }
                    let source = manifest.state(source_path);
                    require_regular_source(&source)?;
                    validate_expected_content(&proposal, &source)?;
                    if current != WorkspacePathState::Absent {
                        return Err(WorkspacePlanError::TargetAlreadyExists);
                    }
                    transitions.push(WorkspacePathTransition {
                        path: source_path.clone(),
                        before: source.clone(),
                        after: WorkspacePathState::Absent,
                        content: None,
                        resulting_permissions: None,
                    });
                    transitions.push(WorkspacePathTransition {
                        path: proposal.path.clone(),
                        before: WorkspacePathState::Absent,
                        after: source,
                        content: None,
                        resulting_permissions: None,
                    });
                }
            }
            changes.push(PlannedFileChange {
                kind: proposal.kind,
                path: proposal.path,
                previous_path,
            });
        }

        let total_bytes = staged_content.values().try_fold(0_u64, |total, item| {
            total
                .checked_add(item.byte_size)
                .ok_or(WorkspacePlanError::OperationContentTooLarge)
        })?;
        if total_bytes > MAX_STAGED_OPERATION_BYTES {
            return Err(WorkspacePlanError::OperationContentTooLarge);
        }

        transitions.sort_by(|left, right| left.path.as_str().cmp(right.path.as_str()));
        Ok(Self {
            operation_id: request.operation_id,
            command_id: request.command_id,
            correlation_id: request.correlation_id,
            project_id: request.project_id,
            binding_id: request.binding_id,
            base_revision: request.base_revision,
            scope_sha256: manifest.scope_sha256,
            intent_sha256: request.intent_sha256,
            safety_checkpoint_id: request.safety_checkpoint_id,
            prepared_at_unix_ms: request.prepared_at_unix_ms,
            changes,
            transitions,
        })
    }
}

fn validate_request_header(
    manifest: &WorkspaceManifest,
    request: &WorkspaceFileEffectRequest,
) -> Result<(), WorkspacePlanError> {
    if request.correlation_id.is_empty() || request.correlation_id.len() > 256 {
        return Err(WorkspacePlanError::InvalidCorrelation);
    }
    if request.changes.is_empty() || request.changes.len() > MAX_FILE_CHANGES_PER_OPERATION {
        return Err(WorkspacePlanError::InvalidChangeCount);
    }
    if !manifest.complete {
        return Err(WorkspacePlanError::IncompleteManifest);
    }
    if manifest.revision != request.base_revision {
        return Err(WorkspacePlanError::StaleRevision);
    }
    if request.base_revision.binding_id() != request.binding_id {
        return Err(WorkspacePlanError::WrongBinding);
    }
    Ok(())
}

fn require_no_previous_or_expected(
    proposal: &ResolvedFileChangeProposal,
) -> Result<(), WorkspacePlanError> {
    require_no_previous(proposal)?;
    if proposal.expected_content_sha256.is_some() {
        return Err(WorkspacePlanError::UnexpectedExpectedContent);
    }
    Ok(())
}

fn require_no_previous(proposal: &ResolvedFileChangeProposal) -> Result<(), WorkspacePlanError> {
    if proposal.previous_path.is_some() {
        return Err(WorkspacePlanError::UnexpectedPreviousPath);
    }
    Ok(())
}

fn require_no_content_or_metadata(
    proposal: &ResolvedFileChangeProposal,
) -> Result<(), WorkspacePlanError> {
    if proposal.content.is_some() || proposal.resulting_permissions.is_some() {
        return Err(WorkspacePlanError::UnexpectedContent);
    }
    Ok(())
}

fn proposed_regular_file(
    proposal: &ResolvedFileChangeProposal,
) -> Result<WorkspacePathState, WorkspacePlanError> {
    let content = proposal
        .content
        .as_ref()
        .ok_or(WorkspacePlanError::MissingContent)?;
    let permissions = proposal
        .resulting_permissions
        .ok_or(WorkspacePlanError::MissingResultingMetadata)?;
    Ok(WorkspacePathState::RegularFile {
        content_sha256: content.content_sha256,
        metadata_sha256: permissions.metadata_sha256(),
        byte_size: content.byte_size,
    })
}

fn require_regular_source(state: &WorkspacePathState) -> Result<(), WorkspacePlanError> {
    if !state.is_regular_file() {
        return Err(match state {
            WorkspacePathState::Absent => WorkspacePlanError::SourceMissing,
            WorkspacePathState::Directory { .. }
            | WorkspacePathState::Link { .. }
            | WorkspacePathState::Other { .. }
            | WorkspacePathState::RegularFile { .. } => WorkspacePlanError::UnsupportedSourceKind,
        });
    }
    Ok(())
}

fn validate_expected_content(
    proposal: &ResolvedFileChangeProposal,
    current: &WorkspacePathState,
) -> Result<(), WorkspacePlanError> {
    if let Some(expected) = proposal.expected_content_sha256
        && current.content_sha256() != Some(expected)
    {
        return Err(WorkspacePlanError::ExpectedContentMismatch);
    }
    Ok(())
}

fn validate_mutable_path(path: &ProjectRelativePath) -> Result<(), WorkspacePlanError> {
    let segments = path.as_str().split('/').collect::<Vec<_>>();
    if segments
        .iter()
        .any(|segment| segment.eq_ignore_ascii_case(".git"))
        || (segments.len() == 2
            && segments[0].eq_ignore_ascii_case(".dennett")
            && segments[1].eq_ignore_ascii_case("project.json"))
    {
        return Err(WorkspacePlanError::ProtectedPath);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionObservation {
    Before,
    After,
    Diverged,
}

#[must_use]
pub fn classify_transition(
    transition: &WorkspacePathTransition,
    observed: &WorkspacePathState,
) -> TransitionObservation {
    if observed == &transition.after {
        TransitionObservation::After
    } else if observed == &transition.before {
        TransitionObservation::Before
    } else {
        TransitionObservation::Diverged
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceSnapshotRecord {
    pub project_id: ProjectId,
    pub manifest: WorkspaceManifest,
    pub observed_at_unix_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceSnapshotPublication {
    pub expected_head: WorkspaceRevision,
    pub snapshot: WorkspaceSnapshotRecord,
}

/// Bytes staged in the Node-owned journal before a filesystem effect starts.
/// Debug output deliberately omits the content.
#[derive(Clone, Eq, PartialEq)]
pub struct WorkspaceBlob {
    pub reference: StagedContentRef,
    pub bytes: Vec<u8>,
}

impl std::fmt::Debug for WorkspaceBlob {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WorkspaceBlob")
            .field("reference", &self.reference)
            .field("bytes", &"<redacted>")
            .finish()
    }
}

impl WorkspaceBlob {
    pub fn from_bytes(
        content_id: impl Into<String>,
        bytes: Vec<u8>,
    ) -> Result<Self, WorkspacePlanError> {
        let byte_size =
            u64::try_from(bytes.len()).map_err(|_| WorkspacePlanError::FileContentTooLarge)?;
        let reference = StagedContentRef {
            content_id: content_id.into(),
            content_sha256: ContentSha256(Sha256::digest(&bytes).into()),
            byte_size,
        };
        reference.validate()?;
        Ok(Self { reference, bytes })
    }

    pub fn validate(&self) -> Result<(), WorkspacePlanError> {
        self.reference.validate()?;
        if self.reference.byte_size
            != u64::try_from(self.bytes.len())
                .map_err(|_| WorkspacePlanError::FileContentTooLarge)?
            || self.reference.content_sha256 != ContentSha256(Sha256::digest(&self.bytes).into())
        {
            return Err(WorkspacePlanError::ContentEvidenceMismatch);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CanonicalObjectRef {
    pub kind: String,
    pub id: String,
}

impl CanonicalObjectRef {
    pub fn validate(&self) -> Result<(), WorkspacePlanError> {
        if self.kind.is_empty()
            || self.kind.len() > 128
            || self.id.is_empty()
            || self.id.len() > 512
        {
            return Err(WorkspacePlanError::InvalidCanonicalReference);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceCheckpointEntry {
    pub path: ProjectRelativePath,
    pub state: WorkspacePathState,
    pub content: Option<StagedContentRef>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DurableCheckpointState {
    Available,
    Restored,
    RecoveryRequired,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceCheckpointRecord {
    pub checkpoint_id: CheckpointId,
    pub project_id: ProjectId,
    pub binding_id: WorkspaceBindingId,
    pub base_revision: WorkspaceRevision,
    pub captured_revision: WorkspaceRevision,
    pub state: DurableCheckpointState,
    pub label: String,
    pub request_summary: String,
    pub entries: Vec<WorkspaceCheckpointEntry>,
    pub artifacts: Vec<ArtifactId>,
    pub external_effects: Vec<EffectId>,
    pub provider_continuation: Option<CanonicalObjectRef>,
    pub created_at_unix_ms: u64,
}

impl WorkspaceCheckpointRecord {
    pub fn validate(&self) -> Result<(), WorkspacePlanError> {
        if self.base_revision.binding_id() != self.binding_id
            || self.captured_revision.binding_id() != self.binding_id
            || self.label.len() > 256
            || self.request_summary.len() > 8 * 1024
            || self.entries.len() > MAX_FILE_CHANGES_PER_OPERATION * 2
        {
            return Err(WorkspacePlanError::InvalidCheckpoint);
        }
        if let Some(reference) = &self.provider_continuation {
            reference.validate()?;
        }
        let mut paths = BTreeSet::new();
        let mut content_refs = BTreeMap::<String, StagedContentRef>::new();
        for entry in &self.entries {
            validate_mutable_path(&entry.path)?;
            if !paths.insert(entry.path.as_str()) {
                return Err(WorkspacePlanError::DuplicateTouchedPath);
            }
            match (&entry.state, &entry.content) {
                (
                    WorkspacePathState::RegularFile {
                        content_sha256,
                        byte_size,
                        ..
                    },
                    Some(content),
                ) if content.content_sha256 == *content_sha256
                    && content.byte_size == *byte_size =>
                {
                    content.validate()?;
                    match content_refs.insert(content.content_id.clone(), content.clone()) {
                        Some(existing) if existing != *content => {
                            return Err(WorkspacePlanError::ContentReferenceCollision);
                        }
                        _ => {}
                    }
                }
                (WorkspacePathState::RegularFile { .. }, _) => {
                    return Err(WorkspacePlanError::ContentEvidenceMismatch);
                }
                (WorkspacePathState::Absent, None) => {}
                (
                    WorkspacePathState::Directory { .. }
                    | WorkspacePathState::Link { .. }
                    | WorkspacePathState::Other { .. },
                    None,
                ) => {}
                (_, Some(_)) => return Err(WorkspacePlanError::UnexpectedContent),
            }
        }
        let total_bytes = content_refs.values().try_fold(0_u64, |total, item| {
            total
                .checked_add(item.byte_size)
                .ok_or(WorkspacePlanError::OperationContentTooLarge)
        })?;
        if total_bytes > MAX_STAGED_OPERATION_BYTES {
            return Err(WorkspacePlanError::OperationContentTooLarge);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DurableWorkspaceOperationState {
    Prepared,
    FilesystemApplied,
    Succeeded,
    Failed,
    RecoveryRequired,
}

impl DurableWorkspaceOperationState {
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::RecoveryRequired
        )
    }

    #[must_use]
    pub fn can_transition_to(self, next: Self) -> bool {
        self == next
            || matches!(
                (self, next),
                (
                    Self::Prepared,
                    Self::FilesystemApplied | Self::Failed | Self::RecoveryRequired
                ) | (
                    Self::FilesystemApplied,
                    Self::Succeeded | Self::RecoveryRequired
                )
            )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DurableWorkspaceFailureKind {
    Conflict,
    ScopeDenied,
    AdapterFailure,
    RecoveryRequired,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DurableWorkspaceFailure {
    pub kind: DurableWorkspaceFailureKind,
    pub safe_code: String,
    pub conflicting_paths: Vec<ProjectRelativePath>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceOperationRecord {
    pub plan: WorkspaceFileEffectPlan,
    pub state: DurableWorkspaceOperationState,
    pub resulting_revision: Option<WorkspaceRevision>,
    pub failure: Option<DurableWorkspaceFailure>,
    pub completed_at_unix_ms: Option<u64>,
}

impl WorkspaceOperationRecord {
    pub fn validate(&self) -> Result<(), WorkspacePlanError> {
        let valid_terminal = match self.state {
            DurableWorkspaceOperationState::Prepared
            | DurableWorkspaceOperationState::FilesystemApplied => {
                self.resulting_revision.is_none()
                    && self.failure.is_none()
                    && self.completed_at_unix_ms.is_none()
            }
            DurableWorkspaceOperationState::Succeeded => {
                self.resulting_revision.is_some()
                    && self.failure.is_none()
                    && self.completed_at_unix_ms.is_some()
            }
            DurableWorkspaceOperationState::Failed
            | DurableWorkspaceOperationState::RecoveryRequired => {
                self.resulting_revision.is_none()
                    && self.failure.is_some()
                    && self.completed_at_unix_ms.is_some()
            }
        };
        if !valid_terminal {
            return Err(WorkspacePlanError::InvalidOperationRecord);
        }
        if let Some(revision) = self.resulting_revision
            && revision.binding_id() != self.plan.binding_id
        {
            return Err(WorkspacePlanError::WrongBinding);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotCommitOutcome {
    Inserted,
    AlreadyCurrent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum WorkspaceJournalError {
    #[error("workspace journal is unavailable")]
    Unavailable,
    #[error("workspace journal integrity check failed")]
    Integrity,
    #[error("workspace journal revision changed")]
    RevisionConflict,
    #[error("workspace journal command identity was reused for another intent")]
    IdempotencyConflict,
    #[error("workspace journal object is missing")]
    NotFound,
    #[error("workspace journal transition is invalid")]
    InvalidTransition,
}

/// Durable Node-owned state used by the workspace application. Implementations
/// must make each method atomic and preserve an existing terminal record on
/// retry.
#[async_trait]
pub trait WorkspaceJournalPort: Send + Sync {
    async fn load_head(
        &self,
        binding_id: WorkspaceBindingId,
    ) -> Result<Option<WorkspaceSnapshotRecord>, WorkspaceJournalError>;

    async fn load_snapshot(
        &self,
        revision: WorkspaceRevision,
    ) -> Result<Option<WorkspaceSnapshotRecord>, WorkspaceJournalError>;

    async fn commit_snapshot(
        &self,
        expected_head: Option<WorkspaceRevision>,
        snapshot: WorkspaceSnapshotRecord,
    ) -> Result<SnapshotCommitOutcome, WorkspaceJournalError>;

    async fn prepare_file_effect(
        &self,
        operation: WorkspaceOperationRecord,
        safety_checkpoint: WorkspaceCheckpointRecord,
        blobs: Vec<WorkspaceBlob>,
    ) -> Result<WorkspaceOperationRecord, WorkspaceJournalError>;

    async fn load_operation(
        &self,
        operation_id: WorkspaceOperationId,
    ) -> Result<Option<WorkspaceOperationRecord>, WorkspaceJournalError>;

    async fn load_operation_by_command(
        &self,
        command_id: CommandId,
    ) -> Result<Option<WorkspaceOperationRecord>, WorkspaceJournalError>;

    async fn load_operation_by_checkpoint(
        &self,
        checkpoint_id: CheckpointId,
    ) -> Result<Option<WorkspaceOperationRecord>, WorkspaceJournalError>;

    async fn load_unfinished_operations(
        &self,
    ) -> Result<Vec<WorkspaceOperationRecord>, WorkspaceJournalError>;

    async fn load_operation_blobs(
        &self,
        operation_id: WorkspaceOperationId,
    ) -> Result<Vec<WorkspaceBlob>, WorkspaceJournalError>;

    async fn transition_operation(
        &self,
        expected_state: DurableWorkspaceOperationState,
        operation: WorkspaceOperationRecord,
        resulting_snapshot: Option<WorkspaceSnapshotPublication>,
    ) -> Result<WorkspaceOperationRecord, WorkspaceJournalError>;

    async fn save_checkpoint(
        &self,
        checkpoint: WorkspaceCheckpointRecord,
        blobs: Vec<WorkspaceBlob>,
    ) -> Result<WorkspaceCheckpointRecord, WorkspaceJournalError>;

    async fn load_checkpoint(
        &self,
        checkpoint_id: CheckpointId,
    ) -> Result<Option<WorkspaceCheckpointRecord>, WorkspaceJournalError>;

    async fn load_checkpoint_blobs(
        &self,
        checkpoint_id: CheckpointId,
    ) -> Result<Vec<WorkspaceBlob>, WorkspaceJournalError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum WorkspacePlanError {
    #[error("workspace correlation is invalid")]
    InvalidCorrelation,
    #[error("workspace file change count is invalid")]
    InvalidChangeCount,
    #[error("workspace manifest is incomplete")]
    IncompleteManifest,
    #[error("workspace revision is stale")]
    StaleRevision,
    #[error("workspace revision belongs to another binding")]
    WrongBinding,
    #[error("workspace change touches the same path more than once")]
    DuplicateTouchedPath,
    #[error("workspace path belongs to a dedicated protected owner")]
    ProtectedPath,
    #[error("workspace target already exists")]
    TargetAlreadyExists,
    #[error("workspace source is missing")]
    SourceMissing,
    #[error("workspace source kind is unsupported")]
    UnsupportedSourceKind,
    #[error("workspace change content is missing")]
    MissingContent,
    #[error("workspace change resulting metadata is missing")]
    MissingResultingMetadata,
    #[error("workspace change unexpectedly includes content")]
    UnexpectedContent,
    #[error("workspace change unexpectedly includes a previous path")]
    UnexpectedPreviousPath,
    #[error("workspace change requires a previous path")]
    MissingPreviousPath,
    #[error("workspace rename source and target are equal")]
    RenamePathUnchanged,
    #[error("workspace add unexpectedly includes an expected content hash")]
    UnexpectedExpectedContent,
    #[error("workspace expected content does not match the base manifest")]
    ExpectedContentMismatch,
    #[error("workspace content reference is invalid")]
    InvalidContentReference,
    #[error("workspace content reference is reused with different evidence")]
    ContentReferenceCollision,
    #[error("workspace content bytes do not match their evidence")]
    ContentEvidenceMismatch,
    #[error("workspace staged file exceeds the bounded size")]
    FileContentTooLarge,
    #[error("workspace staged operation exceeds the bounded size")]
    OperationContentTooLarge,
    #[error("workspace canonical object reference is invalid")]
    InvalidCanonicalReference,
    #[error("workspace checkpoint is invalid")]
    InvalidCheckpoint,
    #[error("workspace durable operation record is invalid")]
    InvalidOperationRecord,
}

#[cfg(test)]
mod tests {
    use super::*;
    use dennett_contracts::WorkspaceSnapshotId;

    const SCOPE: ContentSha256 = ContentSha256([7; 32]);
    const OLD_HASH: ContentSha256 = ContentSha256([1; 32]);
    const NEW_HASH: ContentSha256 = ContentSha256([2; 32]);
    const METADATA: MetadataSha256 = MetadataSha256([3; 32]);

    fn path(value: &str) -> ProjectRelativePath {
        ProjectRelativePath::try_from(value).expect("valid project path")
    }

    fn revision(binding_id: WorkspaceBindingId, sequence: u64) -> WorkspaceRevision {
        WorkspaceRevision::new(binding_id, WorkspaceSnapshotId::new(), sequence)
            .expect("valid revision")
    }

    fn file(content_sha256: ContentSha256) -> WorkspacePathState {
        WorkspacePathState::RegularFile {
            content_sha256,
            metadata_sha256: METADATA,
            byte_size: 4,
        }
    }

    fn content() -> StagedContentRef {
        StagedContentRef {
            content_id: "staged-1".to_owned(),
            content_sha256: NEW_HASH,
            byte_size: 4,
        }
    }

    fn permissions() -> PortableFilePermissions {
        PortableFilePermissions {
            read_only: false,
            executable: false,
        }
    }

    fn request(
        binding_id: WorkspaceBindingId,
        base_revision: WorkspaceRevision,
        changes: Vec<ResolvedFileChangeProposal>,
    ) -> WorkspaceFileEffectRequest {
        WorkspaceFileEffectRequest {
            operation_id: WorkspaceOperationId::new(),
            command_id: CommandId::new(),
            correlation_id: "workspace-test".to_owned(),
            project_id: ProjectId::new(),
            binding_id,
            base_revision,
            intent_sha256: ContentSha256([9; 32]),
            safety_checkpoint_id: CheckpointId::new(),
            prepared_at_unix_ms: 1,
            changes,
        }
    }

    #[test]
    fn plan_orders_exact_add_modify_delete_and_rename_transitions() {
        let binding_id = WorkspaceBindingId::new();
        let base_revision = revision(binding_id, 4);
        let manifest = WorkspaceManifest::new(
            base_revision,
            SCOPE,
            true,
            vec![
                WorkspaceManifestEntry {
                    path: path("delete.txt"),
                    state: file(OLD_HASH),
                },
                WorkspaceManifestEntry {
                    path: path("modify.txt"),
                    state: file(OLD_HASH),
                },
                WorkspaceManifestEntry {
                    path: path("rename.txt"),
                    state: file(OLD_HASH),
                },
            ],
        )
        .unwrap();
        let plan = WorkspaceFileEffectPlan::build(
            &manifest,
            request(
                binding_id,
                base_revision,
                vec![
                    ResolvedFileChangeProposal {
                        kind: FileMutationKind::Add,
                        path: path("added.txt"),
                        previous_path: None,
                        content: Some(content()),
                        expected_content_sha256: None,
                        resulting_permissions: Some(permissions()),
                    },
                    ResolvedFileChangeProposal {
                        kind: FileMutationKind::Modify,
                        path: path("modify.txt"),
                        previous_path: None,
                        content: Some(content()),
                        expected_content_sha256: Some(OLD_HASH),
                        resulting_permissions: Some(permissions()),
                    },
                    ResolvedFileChangeProposal {
                        kind: FileMutationKind::Delete,
                        path: path("delete.txt"),
                        previous_path: None,
                        content: None,
                        expected_content_sha256: Some(OLD_HASH),
                        resulting_permissions: None,
                    },
                    ResolvedFileChangeProposal {
                        kind: FileMutationKind::Rename,
                        path: path("renamed.txt"),
                        previous_path: Some(path("rename.txt")),
                        content: None,
                        expected_content_sha256: Some(OLD_HASH),
                        resulting_permissions: None,
                    },
                ],
            ),
        )
        .unwrap();

        assert_eq!(plan.changes.len(), 4);
        assert_eq!(plan.transitions.len(), 5);
        assert_eq!(plan.transitions[0].path, path("added.txt"));
        assert_eq!(plan.transitions[1].path, path("delete.txt"));
        assert_eq!(plan.transitions[4].path, path("renamed.txt"));
    }

    #[test]
    fn plan_rejects_incomplete_stale_duplicate_and_protected_scope() {
        let binding_id = WorkspaceBindingId::new();
        let base_revision = revision(binding_id, 1);
        let incomplete = WorkspaceManifest::new(base_revision, SCOPE, false, vec![]).unwrap();
        let add = ResolvedFileChangeProposal {
            kind: FileMutationKind::Add,
            path: path("new.txt"),
            previous_path: None,
            content: Some(content()),
            expected_content_sha256: None,
            resulting_permissions: Some(permissions()),
        };
        assert_eq!(
            WorkspaceFileEffectPlan::build(
                &incomplete,
                request(binding_id, base_revision, vec![add.clone()])
            ),
            Err(WorkspacePlanError::IncompleteManifest)
        );

        let manifest = WorkspaceManifest::new(base_revision, SCOPE, true, vec![]).unwrap();
        assert_eq!(
            WorkspaceFileEffectPlan::build(
                &manifest,
                request(binding_id, revision(binding_id, 2), vec![add.clone()])
            ),
            Err(WorkspacePlanError::StaleRevision)
        );
        assert_eq!(
            WorkspaceFileEffectPlan::build(
                &manifest,
                request(binding_id, base_revision, vec![add.clone(), add])
            ),
            Err(WorkspacePlanError::DuplicateTouchedPath)
        );

        let protected = ResolvedFileChangeProposal {
            kind: FileMutationKind::Add,
            path: path(".git/config"),
            previous_path: None,
            content: Some(content()),
            expected_content_sha256: None,
            resulting_permissions: Some(permissions()),
        };
        assert_eq!(
            WorkspaceFileEffectPlan::build(
                &manifest,
                request(binding_id, base_revision, vec![protected])
            ),
            Err(WorkspacePlanError::ProtectedPath)
        );

        for protected_path in [
            ".GIT/config",
            "vendor/module/.Git/index",
            ".DENNETT/PROJECT.JSON",
        ] {
            let protected = ResolvedFileChangeProposal {
                kind: FileMutationKind::Add,
                path: path(protected_path),
                previous_path: None,
                content: Some(content()),
                expected_content_sha256: None,
                resulting_permissions: Some(permissions()),
            };
            assert_eq!(
                WorkspaceFileEffectPlan::build(
                    &manifest,
                    request(binding_id, base_revision, vec![protected])
                ),
                Err(WorkspacePlanError::ProtectedPath)
            );
        }
    }

    #[test]
    fn portable_permission_evidence_is_reversible_without_os_metadata() {
        for read_only in [false, true] {
            for executable in [false, true] {
                let permissions = PortableFilePermissions {
                    read_only,
                    executable,
                };
                assert_eq!(
                    PortableFilePermissions::from_metadata_sha256(permissions.metadata_sha256()),
                    Some(permissions)
                );
            }
        }
        assert_eq!(
            PortableFilePermissions::from_metadata_sha256(MetadataSha256([255; 32])),
            None
        );
    }

    #[test]
    fn transition_classification_never_confuses_a_third_state_with_completion() {
        let transition = WorkspacePathTransition {
            path: path("file.txt"),
            before: file(OLD_HASH),
            after: file(NEW_HASH),
            content: Some(content()),
            resulting_permissions: Some(permissions()),
        };

        assert_eq!(
            classify_transition(&transition, &transition.before),
            TransitionObservation::Before
        );
        assert_eq!(
            classify_transition(&transition, &transition.after),
            TransitionObservation::After
        );
        assert_eq!(
            classify_transition(&transition, &file(ContentSha256([5; 32]))),
            TransitionObservation::Diverged
        );
    }

    #[test]
    fn manifest_rejects_duplicate_and_explicit_absent_entries() {
        let binding_id = WorkspaceBindingId::new();
        let revision = revision(binding_id, 1);
        let duplicate = WorkspaceManifestEntry {
            path: path("same.txt"),
            state: file(OLD_HASH),
        };
        assert_eq!(
            WorkspaceManifest::new(revision, SCOPE, true, vec![duplicate.clone(), duplicate]),
            Err(WorkspaceManifestError::DuplicatePath)
        );
        assert_eq!(
            WorkspaceManifest::new(
                revision,
                SCOPE,
                true,
                vec![WorkspaceManifestEntry {
                    path: path("absent.txt"),
                    state: WorkspacePathState::Absent,
                }]
            ),
            Err(WorkspaceManifestError::AbsentEntry)
        );
    }

    #[test]
    fn staged_blob_and_checkpoint_require_matching_content_evidence() {
        let blob = WorkspaceBlob::from_bytes("blob-1", b"data".to_vec()).unwrap();
        assert!(blob.validate().is_ok());

        let mut corrupt = blob.clone();
        corrupt.bytes.push(b'!');
        assert_eq!(
            corrupt.validate(),
            Err(WorkspacePlanError::ContentEvidenceMismatch)
        );

        let binding_id = WorkspaceBindingId::new();
        let revision = revision(binding_id, 1);
        let checkpoint = WorkspaceCheckpointRecord {
            checkpoint_id: CheckpointId::new(),
            project_id: ProjectId::new(),
            binding_id,
            base_revision: revision,
            captured_revision: revision,
            state: DurableCheckpointState::Available,
            label: "Before change".to_owned(),
            request_summary: "Safety checkpoint".to_owned(),
            entries: vec![WorkspaceCheckpointEntry {
                path: path("file.txt"),
                state: WorkspacePathState::RegularFile {
                    content_sha256: blob.reference.content_sha256,
                    metadata_sha256: METADATA,
                    byte_size: blob.reference.byte_size,
                },
                content: Some(blob.reference),
            }],
            artifacts: vec![],
            external_effects: vec![],
            provider_continuation: None,
            created_at_unix_ms: 1,
        };
        assert!(checkpoint.validate().is_ok());

        let oversized_entries = (0_u8..5)
            .map(|index| {
                let reference = StagedContentRef {
                    content_id: format!("large-{index}"),
                    content_sha256: ContentSha256([index; 32]),
                    byte_size: MAX_STAGED_FILE_BYTES,
                };
                WorkspaceCheckpointEntry {
                    path: path(&format!("large-{index}.bin")),
                    state: WorkspacePathState::RegularFile {
                        content_sha256: reference.content_sha256,
                        metadata_sha256: permissions().metadata_sha256(),
                        byte_size: reference.byte_size,
                    },
                    content: Some(reference),
                }
            })
            .collect();
        let oversized = WorkspaceCheckpointRecord {
            entries: oversized_entries,
            ..checkpoint
        };
        assert_eq!(
            oversized.validate(),
            Err(WorkspacePlanError::OperationContentTooLarge)
        );
    }

    #[test]
    fn durable_operation_lifecycle_is_forward_only_and_self_consistent() {
        assert!(
            DurableWorkspaceOperationState::Prepared
                .can_transition_to(DurableWorkspaceOperationState::FilesystemApplied)
        );
        assert!(
            DurableWorkspaceOperationState::FilesystemApplied
                .can_transition_to(DurableWorkspaceOperationState::Succeeded)
        );
        assert!(
            !DurableWorkspaceOperationState::Succeeded
                .can_transition_to(DurableWorkspaceOperationState::Prepared)
        );
        assert!(DurableWorkspaceOperationState::RecoveryRequired.is_terminal());

        let binding_id = WorkspaceBindingId::new();
        let base_revision = revision(binding_id, 1);
        let manifest = WorkspaceManifest::new(base_revision, SCOPE, true, vec![]).unwrap();
        let proposal = ResolvedFileChangeProposal {
            kind: FileMutationKind::Add,
            path: path("new.txt"),
            previous_path: None,
            content: Some(content()),
            expected_content_sha256: None,
            resulting_permissions: Some(permissions()),
        };
        let plan = WorkspaceFileEffectPlan::build(
            &manifest,
            request(binding_id, base_revision, vec![proposal]),
        )
        .unwrap();
        let malformed = WorkspaceOperationRecord {
            plan,
            state: DurableWorkspaceOperationState::Succeeded,
            resulting_revision: None,
            failure: None,
            completed_at_unix_ms: Some(2),
        };
        assert_eq!(
            malformed.validate(),
            Err(WorkspacePlanError::InvalidOperationRecord)
        );
    }
}
