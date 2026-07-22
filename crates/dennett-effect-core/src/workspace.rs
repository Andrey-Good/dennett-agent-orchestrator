//! Provider-neutral workspace effect decisions.
//!
//! This module owns no filesystem handles and performs no I/O. It turns a
//! complete, versioned manifest plus resolved content handles into an exact
//! before/after plan. The Node adapter remains responsible for no-follow
//! traversal, durable publication and observing the actual result.

use dennett_contracts::{
    CommandId, ProjectId, ProjectRelativePath, WorkspaceBindingId, WorkspaceOperationId,
    WorkspaceRevision,
};
use serde::{Deserialize, Serialize};
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
    /// Trusted adapter prediction for the post-publication metadata. A modify
    /// normally preserves the existing mode; an add uses the adapter's bounded
    /// creation policy. This lets restart reconciliation distinguish the exact
    /// after-image from a third state.
    pub resulting_metadata_sha256: Option<MetadataSha256>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkspacePathTransition {
    pub path: ProjectRelativePath,
    pub before: WorkspacePathState,
    pub after: WorkspacePathState,
    /// Present only when reaching `after` requires materializing bytes.
    pub content: Option<StagedContentRef>,
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
                    });
                    transitions.push(WorkspacePathTransition {
                        path: proposal.path.clone(),
                        before: WorkspacePathState::Absent,
                        after: source,
                        content: None,
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
    if proposal.content.is_some() || proposal.resulting_metadata_sha256.is_some() {
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
    let metadata_sha256 = proposal
        .resulting_metadata_sha256
        .ok_or(WorkspacePlanError::MissingResultingMetadata)?;
    Ok(WorkspacePathState::RegularFile {
        content_sha256: content.content_sha256,
        metadata_sha256,
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
    let value = path.as_str();
    if value == ".git" || value.starts_with(".git/") || value == ".dennett/project.json" {
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
    #[error("workspace staged file exceeds the bounded size")]
    FileContentTooLarge,
    #[error("workspace staged operation exceeds the bounded size")]
    OperationContentTooLarge,
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
                        resulting_metadata_sha256: Some(METADATA),
                    },
                    ResolvedFileChangeProposal {
                        kind: FileMutationKind::Modify,
                        path: path("modify.txt"),
                        previous_path: None,
                        content: Some(content()),
                        expected_content_sha256: Some(OLD_HASH),
                        resulting_metadata_sha256: Some(METADATA),
                    },
                    ResolvedFileChangeProposal {
                        kind: FileMutationKind::Delete,
                        path: path("delete.txt"),
                        previous_path: None,
                        content: None,
                        expected_content_sha256: Some(OLD_HASH),
                        resulting_metadata_sha256: None,
                    },
                    ResolvedFileChangeProposal {
                        kind: FileMutationKind::Rename,
                        path: path("renamed.txt"),
                        previous_path: Some(path("rename.txt")),
                        content: None,
                        expected_content_sha256: Some(OLD_HASH),
                        resulting_metadata_sha256: None,
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
            resulting_metadata_sha256: Some(METADATA),
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
            resulting_metadata_sha256: Some(METADATA),
        };
        assert_eq!(
            WorkspaceFileEffectPlan::build(
                &manifest,
                request(binding_id, base_revision, vec![protected])
            ),
            Err(WorkspacePlanError::ProtectedPath)
        );
    }

    #[test]
    fn transition_classification_never_confuses_a_third_state_with_completion() {
        let transition = WorkspacePathTransition {
            path: path("file.txt"),
            before: file(OLD_HASH),
            after: file(NEW_HASH),
            content: Some(content()),
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
}
