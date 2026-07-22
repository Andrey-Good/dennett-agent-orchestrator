//! Version-bound workspace application boundary.
//!
//! Head coordinates project authority, the durable effect journal and the
//! Node-owned filesystem capability. Raw paths and bytes never enter events or
//! diagnostics at this layer.

use crate::project::{KeyedLocks, ProjectApplication, ProjectApplicationError};
use async_trait::async_trait;
use dennett_contracts::{
    CheckpointId, CommandId, ProjectId, ProjectRelativePath, WorkspaceBindingId,
    WorkspaceOperationId, WorkspaceRevision, WorkspaceSnapshotId,
};
use dennett_effect_core::workspace::{
    ContentSha256, DurableCheckpointState, DurableWorkspaceFailure, DurableWorkspaceFailureKind,
    DurableWorkspaceOperationState, FileMutationKind, ResolvedFileChangeProposal,
    SnapshotCommitOutcome, WorkspaceBlob, WorkspaceCheckpointEntry, WorkspaceCheckpointRecord,
    WorkspaceFileEffectPlan, WorkspaceFileEffectRequest, WorkspaceJournalError,
    WorkspaceJournalPort, WorkspaceManifest, WorkspaceManifestEntry, WorkspaceManifestError,
    WorkspaceOperationRecord, WorkspacePathState, WorkspacePlanError, WorkspaceSnapshotRecord,
    classify_transition,
};
use dennett_trust_core::project_registry::{WorkspaceAccessMode, WorkspaceSourceIdentity};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc, time::SystemTime};

const MAX_SNAPSHOT_COMMIT_ATTEMPTS: usize = 3;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceFilesystemScope {
    pub project_id: ProjectId,
    pub binding_id: WorkspaceBindingId,
    pub absolute_path: String,
    pub source_identity: WorkspaceSourceIdentity,
    pub writable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceObservation {
    pub scope_sha256: ContentSha256,
    pub complete: bool,
    pub entries: Vec<WorkspaceManifestEntry>,
}

impl WorkspaceObservation {
    fn into_manifest(
        self,
        revision: WorkspaceRevision,
    ) -> Result<WorkspaceManifest, WorkspaceManifestError> {
        WorkspaceManifest::new(revision, self.scope_sha256, self.complete, self.entries)
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct WorkspaceFileChangeInput {
    pub kind: FileMutationKind,
    pub path: ProjectRelativePath,
    pub previous_path: Option<ProjectRelativePath>,
    pub content: Option<Vec<u8>>,
    pub expected_content_sha256: Option<ContentSha256>,
}

impl std::fmt::Debug for WorkspaceFileChangeInput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WorkspaceFileChangeInput")
            .field("kind", &self.kind)
            .field("path", &self.path)
            .field("previous_path", &self.previous_path)
            .field("content", &self.content.as_ref().map(|bytes| bytes.len()))
            .field("expected_content_sha256", &self.expected_content_sha256)
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedWorkspaceFileEffect {
    pub observation: WorkspaceObservation,
    pub proposals: Vec<ResolvedFileChangeProposal>,
    pub proposed_blobs: Vec<WorkspaceBlob>,
    pub checkpoint_entries: Vec<WorkspaceCheckpointEntry>,
    pub checkpoint_blobs: Vec<WorkspaceBlob>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceTransitionObservation {
    pub path: ProjectRelativePath,
    pub state: WorkspacePathState,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum WorkspaceFilesystemError {
    #[error("workspace location is missing")]
    LocationMissing,
    #[error("workspace filesystem scope is denied")]
    ScopeDenied,
    #[error("workspace filesystem state conflicts with the requested effect")]
    Conflict,
    #[error("workspace filesystem object is unsupported")]
    UnsupportedObject,
    #[error("workspace filesystem bound was exceeded")]
    BoundExceeded,
    #[error("workspace filesystem effect requires recovery")]
    RecoveryRequired,
    #[error("workspace filesystem adapter is unavailable")]
    AdapterUnavailable,
}

impl WorkspaceFilesystemError {
    #[must_use]
    pub const fn safe_code(&self) -> &'static str {
        match self {
            Self::LocationMissing => "workspace.location.missing",
            Self::ScopeDenied => "workspace.scope.denied",
            Self::Conflict => "workspace.path.conflict",
            Self::UnsupportedObject => "workspace.object.unsupported",
            Self::BoundExceeded => "workspace.snapshot.bound_exceeded",
            Self::RecoveryRequired => "workspace.effect.recovery_required",
            Self::AdapterUnavailable => "workspace.adapter.unavailable",
        }
    }
}

#[async_trait]
pub trait WorkspaceFilesystemPort: Send + Sync {
    async fn observe_workspace(
        &self,
        scope: &WorkspaceFilesystemScope,
    ) -> Result<WorkspaceObservation, WorkspaceFilesystemError>;

    /// Resolves metadata and captures every reversible before-image without
    /// changing the project directory.
    async fn prepare_file_effect(
        &self,
        scope: &WorkspaceFilesystemScope,
        changes: Vec<WorkspaceFileChangeInput>,
    ) -> Result<PreparedWorkspaceFileEffect, WorkspaceFilesystemError>;

    /// Preflights every transition again and applies only recognized before
    /// states. Per-path publication is atomic; a late multi-file race is
    /// reported for reconciliation rather than called success.
    async fn apply_file_effect(
        &self,
        scope: &WorkspaceFilesystemScope,
        plan: &WorkspaceFileEffectPlan,
        blobs: &[WorkspaceBlob],
    ) -> Result<WorkspaceObservation, WorkspaceFilesystemError>;

    async fn observe_transitions(
        &self,
        scope: &WorkspaceFilesystemScope,
        plan: &WorkspaceFileEffectPlan,
    ) -> Result<Vec<WorkspaceTransitionObservation>, WorkspaceFilesystemError>;

    /// Removes only operation-owned publication sidecars after every touched
    /// path has been proven to match its durable after-image.
    async fn cleanup_file_effect(
        &self,
        scope: &WorkspaceFilesystemScope,
        plan: &WorkspaceFileEffectPlan,
    ) -> Result<(), WorkspaceFilesystemError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplyWorkspaceFileChangesCommand {
    pub operation_id: WorkspaceOperationId,
    pub command_id: CommandId,
    pub correlation_id: String,
    pub project_id: ProjectId,
    pub binding_id: WorkspaceBindingId,
    pub base_revision: WorkspaceRevision,
    pub changes: Vec<WorkspaceFileChangeInput>,
    pub prepared_at_unix_ms: u64,
}

#[derive(Clone)]
pub struct WorkspaceApplication {
    projects: Arc<ProjectApplication>,
    journal: Arc<dyn WorkspaceJournalPort>,
    filesystem: Arc<dyn WorkspaceFilesystemPort>,
    writers: KeyedLocks<WorkspaceBindingId>,
}

impl WorkspaceApplication {
    #[must_use]
    pub fn new(
        projects: Arc<ProjectApplication>,
        journal: Arc<dyn WorkspaceJournalPort>,
        filesystem: Arc<dyn WorkspaceFilesystemPort>,
    ) -> Self {
        Self {
            projects,
            journal,
            filesystem,
            writers: KeyedLocks::default(),
        }
    }

    pub async fn observe(
        &self,
        project_id: ProjectId,
        binding_id: WorkspaceBindingId,
        correlation_id: String,
    ) -> Result<WorkspaceSnapshotRecord, WorkspaceApplicationError> {
        let _writer = self.writers.acquire(binding_id).await;
        let authority = self
            .projects
            .prepare_agent_workspace(project_id, correlation_id)
            .await?;
        let scope = scope_from_authority(&authority, binding_id, false)?;
        let observation = self.filesystem.observe_workspace(&scope).await?;
        self.commit_observation(project_id, binding_id, observation)
            .await
    }

    pub async fn apply_file_changes(
        &self,
        command: ApplyWorkspaceFileChangesCommand,
    ) -> Result<WorkspaceOperationRecord, WorkspaceApplicationError> {
        validate_apply_command(&command)?;
        let intent_sha256 = hash_file_change_intent(&command.changes)?;
        if let Some(existing) = self
            .journal
            .load_operation_by_command(command.command_id)
            .await?
        {
            return if operation_matches_command(&existing, &command, intent_sha256) {
                Ok(existing)
            } else {
                Err(WorkspaceJournalError::IdempotencyConflict.into())
            };
        }

        let _writer = self.writers.acquire(command.binding_id).await;
        if let Some(existing) = self
            .journal
            .load_operation_by_command(command.command_id)
            .await?
        {
            return if operation_matches_command(&existing, &command, intent_sha256) {
                Ok(existing)
            } else {
                Err(WorkspaceJournalError::IdempotencyConflict.into())
            };
        }

        let authority = self
            .projects
            .prepare_agent_workspace(command.project_id, command.correlation_id.clone())
            .await?;
        let scope = scope_from_authority(&authority, command.binding_id, true)?;
        let base = self
            .journal
            .load_snapshot(command.base_revision)
            .await?
            .ok_or(WorkspaceApplicationError::SnapshotNotFound)?;
        if base.project_id != command.project_id
            || base.manifest.revision.binding_id() != command.binding_id
        {
            return Err(WorkspaceApplicationError::BindingMismatch);
        }

        let prepared = self
            .filesystem
            .prepare_file_effect(&scope, command.changes)
            .await?;
        let safety_checkpoint_id = CheckpointId::new();
        let plan = WorkspaceFileEffectPlan::build(
            &base.manifest,
            WorkspaceFileEffectRequest {
                operation_id: command.operation_id,
                command_id: command.command_id,
                correlation_id: command.correlation_id,
                project_id: command.project_id,
                binding_id: command.binding_id,
                base_revision: command.base_revision,
                intent_sha256,
                safety_checkpoint_id,
                prepared_at_unix_ms: command.prepared_at_unix_ms,
                changes: prepared.proposals,
            },
        )?;

        let observed_manifest = prepared
            .observation
            .clone()
            .into_manifest(command.base_revision)?;
        require_recognized_before_states(&plan, &observed_manifest)?;
        let captured = self
            .commit_observation(command.project_id, command.binding_id, prepared.observation)
            .await?;
        validate_checkpoint_capture(&plan, &captured.manifest, &prepared.checkpoint_entries)?;

        let checkpoint = WorkspaceCheckpointRecord {
            checkpoint_id: safety_checkpoint_id,
            project_id: command.project_id,
            binding_id: command.binding_id,
            base_revision: command.base_revision,
            captured_revision: captured.manifest.revision,
            state: DurableCheckpointState::Available,
            label: "Automatic safety checkpoint".to_owned(),
            request_summary: "Before version-bound file changes".to_owned(),
            entries: prepared.checkpoint_entries,
            artifacts: vec![],
            external_effects: vec![],
            provider_continuation: None,
            created_at_unix_ms: command.prepared_at_unix_ms,
        };
        checkpoint.validate()?;
        let mut blobs = prepared.proposed_blobs;
        blobs.extend(prepared.checkpoint_blobs);
        validate_blob_set(&blobs)?;
        let operation = WorkspaceOperationRecord {
            plan,
            state: DurableWorkspaceOperationState::Prepared,
            resulting_revision: None,
            failure: None,
            completed_at_unix_ms: None,
        };
        operation.validate()?;
        let operation = self
            .journal
            .prepare_file_effect(operation, checkpoint, blobs.clone())
            .await?;

        match self
            .filesystem
            .apply_file_effect(&scope, &operation.plan, &blobs)
            .await
        {
            Ok(after) => {
                self.finish_applied_operation(operation, captured.manifest.revision, after)
                    .await
            }
            Err(error) => {
                self.classify_failed_application(operation, &scope, error)
                    .await
            }
        }
    }

    /// Reconciles only unfinished effects. Terminal receipts remain immutable;
    /// a later human edit becomes a newer snapshot, never retroactive failure.
    pub async fn reconcile_unfinished(
        &self,
    ) -> Result<Vec<WorkspaceOperationRecord>, WorkspaceApplicationError> {
        let operations = self.journal.load_unfinished_operations().await?;
        let mut reconciled = Vec::with_capacity(operations.len());
        for operation in operations {
            let _writer = self.writers.acquire(operation.plan.binding_id).await;
            let authority = match self
                .projects
                .prepare_workspace_recovery(operation.plan.project_id, operation.plan.binding_id)
                .await
            {
                Ok(authority) => authority,
                Err(_) => {
                    reconciled.push(
                        self.mark_recovery_required(
                            operation,
                            "workspace.recovery.location_unavailable",
                            vec![],
                        )
                        .await?,
                    );
                    continue;
                }
            };
            let scope = scope_from_authority(&authority, operation.plan.binding_id, false)?;
            let observations = match self
                .filesystem
                .observe_transitions(&scope, &operation.plan)
                .await
            {
                Ok(observations) => observations,
                Err(_) => {
                    reconciled.push(
                        self.mark_recovery_required(
                            operation,
                            "workspace.recovery.observation_failed",
                            vec![],
                        )
                        .await?,
                    );
                    continue;
                }
            };
            let classification = classify_operation(&operation.plan, &observations)?;
            let result = match classification {
                OperationObservation::Before
                    if operation.state == DurableWorkspaceOperationState::Prepared =>
                {
                    self.mark_failed(
                        operation,
                        "workspace.effect.unapplied_after_restart",
                        vec![],
                    )
                    .await?
                }
                OperationObservation::After => {
                    if self
                        .filesystem
                        .cleanup_file_effect(&scope, &operation.plan)
                        .await
                        .is_err()
                    {
                        self.mark_recovery_required(
                            operation,
                            "workspace.recovery.cleanup_failed",
                            vec![],
                        )
                        .await?
                    } else {
                        let after = self.filesystem.observe_workspace(&scope).await?;
                        let head = self.journal.load_head(operation.plan.binding_id).await?;
                        let expected = head.map(|snapshot| snapshot.manifest.revision);
                        let expected = expected.unwrap_or(operation.plan.base_revision);
                        self.finish_applied_operation(operation, expected, after)
                            .await?
                    }
                }
                OperationObservation::Before
                | OperationObservation::Partial
                | OperationObservation::Diverged => {
                    let conflicts = divergent_paths(&operation.plan, &observations);
                    self.mark_recovery_required(
                        operation,
                        "workspace.effect.partial_or_diverged_after_restart",
                        conflicts,
                    )
                    .await?
                }
            };
            reconciled.push(result);
        }
        Ok(reconciled)
    }

    async fn commit_observation(
        &self,
        project_id: ProjectId,
        binding_id: WorkspaceBindingId,
        observation: WorkspaceObservation,
    ) -> Result<WorkspaceSnapshotRecord, WorkspaceApplicationError> {
        if !observation.complete {
            return Err(WorkspaceFilesystemError::BoundExceeded.into());
        }
        for _ in 0..MAX_SNAPSHOT_COMMIT_ATTEMPTS {
            let head = self.journal.load_head(binding_id).await?;
            let expected = head.as_ref().map(|record| record.manifest.revision);
            let sequence = expected.map_or(Ok(1), |revision| {
                revision
                    .sequence()
                    .checked_add(1)
                    .ok_or(WorkspaceApplicationError::RevisionExhausted)
            })?;
            let revision = WorkspaceRevision::new(binding_id, WorkspaceSnapshotId::new(), sequence)
                .map_err(|_| WorkspaceApplicationError::RevisionExhausted)?;
            let candidate = WorkspaceSnapshotRecord {
                project_id,
                manifest: observation.clone().into_manifest(revision)?,
                observed_at_unix_ms: unix_time_ms(),
            };
            if let Some(head) = head
                && head.manifest.has_same_observation(&candidate.manifest)
            {
                return Ok(head);
            }
            match self
                .journal
                .commit_snapshot(expected, candidate.clone())
                .await
            {
                Ok(SnapshotCommitOutcome::Inserted | SnapshotCommitOutcome::AlreadyCurrent) => {
                    return Ok(candidate);
                }
                Err(WorkspaceJournalError::RevisionConflict) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(WorkspaceApplicationError::ConcurrentChange)
    }

    async fn finish_applied_operation(
        &self,
        mut operation: WorkspaceOperationRecord,
        expected_head: WorkspaceRevision,
        after: WorkspaceObservation,
    ) -> Result<WorkspaceOperationRecord, WorkspaceApplicationError> {
        if operation.state == DurableWorkspaceOperationState::Prepared {
            let mut applied = operation.clone();
            applied.state = DurableWorkspaceOperationState::FilesystemApplied;
            operation = self
                .journal
                .transition_operation(operation.state, applied, None)
                .await?;
        }
        let resulting = self
            .commit_observation_with_expected(
                operation.plan.project_id,
                operation.plan.binding_id,
                expected_head,
                after,
            )
            .await?;
        let mut succeeded = operation.clone();
        succeeded.state = DurableWorkspaceOperationState::Succeeded;
        succeeded.resulting_revision = Some(resulting.manifest.revision);
        succeeded.completed_at_unix_ms = Some(unix_time_ms());
        succeeded.validate()?;
        self.journal
            .transition_operation(operation.state, succeeded, None)
            .await
            .map_err(Into::into)
    }

    async fn commit_observation_with_expected(
        &self,
        project_id: ProjectId,
        binding_id: WorkspaceBindingId,
        expected_head: WorkspaceRevision,
        observation: WorkspaceObservation,
    ) -> Result<WorkspaceSnapshotRecord, WorkspaceApplicationError> {
        let head = self
            .journal
            .load_head(binding_id)
            .await?
            .ok_or(WorkspaceApplicationError::SnapshotNotFound)?;
        if head.manifest.revision != expected_head {
            // The only local writer is serialized. A newer durable head here
            // means recovery or another authority boundary intervened.
            return Err(WorkspaceApplicationError::ConcurrentChange);
        }
        if head
            .manifest
            .has_same_observation(&observation.clone().into_manifest(head.manifest.revision)?)
        {
            return Ok(head);
        }
        let sequence = expected_head
            .sequence()
            .checked_add(1)
            .ok_or(WorkspaceApplicationError::RevisionExhausted)?;
        let revision = WorkspaceRevision::new(binding_id, WorkspaceSnapshotId::new(), sequence)
            .map_err(|_| WorkspaceApplicationError::RevisionExhausted)?;
        let snapshot = WorkspaceSnapshotRecord {
            project_id,
            manifest: observation.into_manifest(revision)?,
            observed_at_unix_ms: unix_time_ms(),
        };
        self.journal
            .commit_snapshot(Some(expected_head), snapshot.clone())
            .await?;
        Ok(snapshot)
    }

    async fn classify_failed_application(
        &self,
        operation: WorkspaceOperationRecord,
        scope: &WorkspaceFilesystemScope,
        error: WorkspaceFilesystemError,
    ) -> Result<WorkspaceOperationRecord, WorkspaceApplicationError> {
        let observations = match self
            .filesystem
            .observe_transitions(scope, &operation.plan)
            .await
        {
            Ok(observations) => observations,
            Err(_) => {
                return self
                    .mark_recovery_required(
                        operation,
                        "workspace.effect.observation_failed",
                        vec![],
                    )
                    .await;
            }
        };
        match classify_operation(&operation.plan, &observations)? {
            OperationObservation::Before => {
                self.mark_failed(operation, error.safe_code(), vec![]).await
            }
            OperationObservation::After => {
                if self
                    .filesystem
                    .cleanup_file_effect(scope, &operation.plan)
                    .await
                    .is_err()
                {
                    return self
                        .mark_recovery_required(
                            operation,
                            "workspace.effect.cleanup_failed",
                            vec![],
                        )
                        .await;
                }
                let after = self.filesystem.observe_workspace(scope).await?;
                let head = self
                    .journal
                    .load_head(operation.plan.binding_id)
                    .await?
                    .ok_or(WorkspaceApplicationError::SnapshotNotFound)?;
                self.finish_applied_operation(operation, head.manifest.revision, after)
                    .await
            }
            OperationObservation::Partial | OperationObservation::Diverged => {
                let conflicts = divergent_paths(&operation.plan, &observations);
                self.mark_recovery_required(operation, error.safe_code(), conflicts)
                    .await
            }
        }
    }

    async fn mark_failed(
        &self,
        operation: WorkspaceOperationRecord,
        safe_code: &str,
        conflicting_paths: Vec<ProjectRelativePath>,
    ) -> Result<WorkspaceOperationRecord, WorkspaceApplicationError> {
        self.mark_terminal_failure(
            operation,
            DurableWorkspaceOperationState::Failed,
            DurableWorkspaceFailureKind::AdapterFailure,
            safe_code,
            conflicting_paths,
        )
        .await
    }

    async fn mark_recovery_required(
        &self,
        operation: WorkspaceOperationRecord,
        safe_code: &str,
        conflicting_paths: Vec<ProjectRelativePath>,
    ) -> Result<WorkspaceOperationRecord, WorkspaceApplicationError> {
        self.mark_terminal_failure(
            operation,
            DurableWorkspaceOperationState::RecoveryRequired,
            DurableWorkspaceFailureKind::RecoveryRequired,
            safe_code,
            conflicting_paths,
        )
        .await
    }

    async fn mark_terminal_failure(
        &self,
        operation: WorkspaceOperationRecord,
        state: DurableWorkspaceOperationState,
        kind: DurableWorkspaceFailureKind,
        safe_code: &str,
        conflicting_paths: Vec<ProjectRelativePath>,
    ) -> Result<WorkspaceOperationRecord, WorkspaceApplicationError> {
        let mut terminal = operation.clone();
        terminal.state = state;
        terminal.failure = Some(DurableWorkspaceFailure {
            kind,
            safe_code: safe_code.to_owned(),
            conflicting_paths,
        });
        terminal.completed_at_unix_ms = Some(unix_time_ms());
        terminal.validate()?;
        self.journal
            .transition_operation(operation.state, terminal, None)
            .await
            .map_err(Into::into)
    }
}

fn scope_from_authority(
    authority: &crate::project::AgentProjectWorkspace,
    requested_binding_id: WorkspaceBindingId,
    require_write: bool,
) -> Result<WorkspaceFilesystemScope, WorkspaceApplicationError> {
    if authority.binding_id != requested_binding_id {
        return Err(WorkspaceApplicationError::BindingMismatch);
    }
    let writable = authority.access_mode == WorkspaceAccessMode::ReadWrite;
    if require_write && !writable {
        return Err(WorkspaceApplicationError::ReadOnly);
    }
    Ok(WorkspaceFilesystemScope {
        project_id: authority.project_id,
        binding_id: authority.binding_id,
        absolute_path: authority.absolute_path.clone(),
        source_identity: authority.source_identity,
        writable,
    })
}

fn validate_apply_command(
    command: &ApplyWorkspaceFileChangesCommand,
) -> Result<(), WorkspaceApplicationError> {
    if command.correlation_id.is_empty()
        || command.correlation_id.len() > 256
        || command.base_revision.binding_id() != command.binding_id
    {
        return Err(WorkspaceApplicationError::InvalidRequest);
    }
    Ok(())
}

fn hash_file_change_intent(
    changes: &[WorkspaceFileChangeInput],
) -> Result<ContentSha256, WorkspaceApplicationError> {
    let mut hasher = Sha256::new();
    hasher.update(b"dennett.workspace-file-intent.v1\0");
    hash_length(&mut hasher, changes.len())?;
    for change in changes {
        hasher.update([match change.kind {
            FileMutationKind::Add => 1,
            FileMutationKind::Modify => 2,
            FileMutationKind::Delete => 3,
            FileMutationKind::Rename => 4,
        }]);
        hash_field(&mut hasher, change.path.as_str().as_bytes())?;
        hash_optional_field(
            &mut hasher,
            change
                .previous_path
                .as_ref()
                .map(|path| path.as_str().as_bytes()),
        )?;
        hash_optional_field(&mut hasher, change.content.as_deref())?;
        match change.expected_content_sha256 {
            Some(hash) => {
                hasher.update([1]);
                hasher.update(hash.0);
            }
            None => hasher.update([0]),
        }
    }
    Ok(ContentSha256(hasher.finalize().into()))
}

fn hash_length(hasher: &mut Sha256, length: usize) -> Result<(), WorkspaceApplicationError> {
    let length = u64::try_from(length).map_err(|_| WorkspaceApplicationError::InvalidRequest)?;
    hasher.update(length.to_be_bytes());
    Ok(())
}

fn hash_field(hasher: &mut Sha256, value: &[u8]) -> Result<(), WorkspaceApplicationError> {
    hash_length(hasher, value.len())?;
    hasher.update(value);
    Ok(())
}

fn hash_optional_field(
    hasher: &mut Sha256,
    value: Option<&[u8]>,
) -> Result<(), WorkspaceApplicationError> {
    if let Some(value) = value {
        hasher.update([1]);
        hash_field(hasher, value)?;
    } else {
        hasher.update([0]);
    }
    Ok(())
}

fn operation_matches_command(
    operation: &WorkspaceOperationRecord,
    command: &ApplyWorkspaceFileChangesCommand,
    intent_sha256: ContentSha256,
) -> bool {
    operation.plan.operation_id == command.operation_id
        && operation.plan.command_id == command.command_id
        && operation.plan.project_id == command.project_id
        && operation.plan.binding_id == command.binding_id
        && operation.plan.base_revision == command.base_revision
        && operation.plan.intent_sha256 == intent_sha256
}

fn require_recognized_before_states(
    plan: &WorkspaceFileEffectPlan,
    observed: &WorkspaceManifest,
) -> Result<(), WorkspaceApplicationError> {
    if !observed.complete || observed.scope_sha256 != plan.scope_sha256 {
        return Err(WorkspaceFilesystemError::BoundExceeded.into());
    }
    let conflicts = plan
        .transitions
        .iter()
        .filter(|transition| observed.state(&transition.path) != transition.before)
        .map(|transition| transition.path.clone())
        .collect::<Vec<_>>();
    if conflicts.is_empty() {
        Ok(())
    } else {
        Err(WorkspaceApplicationError::Conflict(conflicts))
    }
}

fn validate_checkpoint_capture(
    plan: &WorkspaceFileEffectPlan,
    captured: &WorkspaceManifest,
    entries: &[WorkspaceCheckpointEntry],
) -> Result<(), WorkspaceApplicationError> {
    let by_path = entries
        .iter()
        .map(|entry| (entry.path.as_str(), entry))
        .collect::<BTreeMap<_, _>>();
    if by_path.len() != plan.transitions.len() {
        return Err(WorkspaceApplicationError::InvalidCheckpointCapture);
    }
    for transition in &plan.transitions {
        let entry = by_path
            .get(transition.path.as_str())
            .ok_or(WorkspaceApplicationError::InvalidCheckpointCapture)?;
        if entry.state != captured.state(&transition.path)
            || entry.state != transition.before
            || matches!(
                entry.state,
                WorkspacePathState::Directory { .. }
                    | WorkspacePathState::Link { .. }
                    | WorkspacePathState::Other { .. }
            )
        {
            return Err(WorkspaceApplicationError::InvalidCheckpointCapture);
        }
    }
    Ok(())
}

fn validate_blob_set(blobs: &[WorkspaceBlob]) -> Result<(), WorkspaceApplicationError> {
    let mut by_id = BTreeMap::new();
    for blob in blobs {
        blob.validate()?;
        match by_id.insert(blob.reference.content_id.as_str(), &blob.reference) {
            Some(existing) if existing != &blob.reference => {
                return Err(WorkspacePlanError::ContentReferenceCollision.into());
            }
            _ => {}
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OperationObservation {
    Before,
    After,
    Partial,
    Diverged,
}

fn classify_operation(
    plan: &WorkspaceFileEffectPlan,
    observations: &[WorkspaceTransitionObservation],
) -> Result<OperationObservation, WorkspaceApplicationError> {
    let observed = observations
        .iter()
        .map(|item| (item.path.as_str(), &item.state))
        .collect::<BTreeMap<_, _>>();
    if observed.len() != plan.transitions.len() {
        return Err(WorkspaceApplicationError::InvalidTransitionObservation);
    }
    let mut before = 0;
    let mut after = 0;
    for transition in &plan.transitions {
        let state = observed
            .get(transition.path.as_str())
            .ok_or(WorkspaceApplicationError::InvalidTransitionObservation)?;
        match classify_transition(transition, state) {
            dennett_effect_core::workspace::TransitionObservation::Before => before += 1,
            dennett_effect_core::workspace::TransitionObservation::After => after += 1,
            dennett_effect_core::workspace::TransitionObservation::Diverged => {
                return Ok(OperationObservation::Diverged);
            }
        }
    }
    Ok(if before == plan.transitions.len() {
        OperationObservation::Before
    } else if after == plan.transitions.len() {
        OperationObservation::After
    } else {
        OperationObservation::Partial
    })
}

fn divergent_paths(
    plan: &WorkspaceFileEffectPlan,
    observations: &[WorkspaceTransitionObservation],
) -> Vec<ProjectRelativePath> {
    let observed = observations
        .iter()
        .map(|item| (item.path.as_str(), &item.state))
        .collect::<BTreeMap<_, _>>();
    plan.transitions
        .iter()
        .filter(|transition| {
            observed.get(transition.path.as_str()).is_none_or(|state| {
                matches!(
                    classify_transition(transition, state),
                    dennett_effect_core::workspace::TransitionObservation::Diverged
                )
            })
        })
        .map(|transition| transition.path.clone())
        .collect()
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceApplicationError {
    #[error("workspace request is invalid")]
    InvalidRequest,
    #[error("workspace binding does not match the request")]
    BindingMismatch,
    #[error("workspace is read-only")]
    ReadOnly,
    #[error("workspace base snapshot was not found")]
    SnapshotNotFound,
    #[error("workspace revision sequence is exhausted")]
    RevisionExhausted,
    #[error("workspace changed concurrently")]
    ConcurrentChange,
    #[error("workspace touched paths changed after the base snapshot")]
    Conflict(Vec<ProjectRelativePath>),
    #[error("workspace checkpoint capture is inconsistent")]
    InvalidCheckpointCapture,
    #[error("workspace transition observation is inconsistent")]
    InvalidTransitionObservation,
    #[error(transparent)]
    Project(#[from] ProjectApplicationError),
    #[error(transparent)]
    Filesystem(#[from] WorkspaceFilesystemError),
    #[error(transparent)]
    Journal(#[from] WorkspaceJournalError),
    #[error(transparent)]
    Plan(#[from] WorkspacePlanError),
    #[error(transparent)]
    Manifest(#[from] WorkspaceManifestError),
}
