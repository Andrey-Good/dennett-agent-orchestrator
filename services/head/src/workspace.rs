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
    CanonicalObjectRef, ContentSha256, DurableCheckpointState, DurableWorkspaceFailure,
    DurableWorkspaceFailureKind, DurableWorkspaceOperationState, FileMutationKind,
    PortableFilePermissions, ResolvedFileChangeProposal, SnapshotCommitOutcome, WorkspaceBlob,
    WorkspaceCheckpointEntry, WorkspaceCheckpointRecord, WorkspaceFileEffectPlan,
    WorkspaceFileEffectRequest, WorkspaceJournalError, WorkspaceJournalPort, WorkspaceManifest,
    WorkspaceManifestEntry, WorkspaceManifestError, WorkspaceOperationRecord, WorkspacePathState,
    WorkspacePlanError, WorkspaceSnapshotPublication, WorkspaceSnapshotRecord,
    WorkspaceStagingNonce, WorkspaceStagingReceipt, classify_transition,
};
use dennett_trust_core::project_registry::{WorkspaceAccessMode, WorkspaceSourceIdentity};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fmt, sync::Arc, time::SystemTime};

const MAX_SNAPSHOT_COMMIT_ATTEMPTS: usize = 3;

#[derive(Clone, Eq, PartialEq)]
pub struct WorkspaceFilesystemScope {
    pub project_id: ProjectId,
    pub binding_id: WorkspaceBindingId,
    pub absolute_path: String,
    pub source_identity: WorkspaceSourceIdentity,
    pub writable: bool,
}

impl fmt::Debug for WorkspaceFilesystemScope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WorkspaceFilesystemScope")
            .field("project_id", &self.project_id)
            .field("binding_id", &self.binding_id)
            .field("absolute_path", &"<redacted>")
            .field("source_identity", &"<redacted>")
            .field("writable", &self.writable)
            .finish()
    }
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
    pub resulting_permissions: Option<PortableFilePermissions>,
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
            .field("resulting_permissions", &self.resulting_permissions)
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
pub struct CapturedWorkspaceCheckpoint {
    pub observation: WorkspaceObservation,
    pub entries: Vec<WorkspaceCheckpointEntry>,
    pub blobs: Vec<WorkspaceBlob>,
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

    /// Creates only operation-private staging objects and returns durable
    /// identity receipts before any user path can be changed.
    async fn stage_file_effect(
        &self,
        scope: &WorkspaceFilesystemScope,
        plan: &WorkspaceFileEffectPlan,
        blobs: &[WorkspaceBlob],
    ) -> Result<WorkspaceStagingReceipt, WorkspaceFilesystemError>;

    /// Preflights every transition and every durable staging receipt again,
    /// then applies only recognized before states. Per-path publication is
    /// atomic; a late multi-file race is reported for reconciliation.
    async fn apply_file_effect(
        &self,
        scope: &WorkspaceFilesystemScope,
        plan: &WorkspaceFileEffectPlan,
        staging: &WorkspaceStagingReceipt,
    ) -> Result<WorkspaceObservation, WorkspaceFilesystemError>;

    async fn observe_transitions(
        &self,
        scope: &WorkspaceFilesystemScope,
        plan: &WorkspaceFileEffectPlan,
    ) -> Result<Vec<WorkspaceTransitionObservation>, WorkspaceFilesystemError>;

    /// Removes only receipt-owned staging objects after every touched path has
    /// been proven to match its durable after-image.
    async fn cleanup_file_effect(
        &self,
        scope: &WorkspaceFilesystemScope,
        plan: &WorkspaceFileEffectPlan,
        staging: Option<&WorkspaceStagingReceipt>,
    ) -> Result<(), WorkspaceFilesystemError>;

    /// Removes only staging objects whose durable identities still match,
    /// after every touched path is proven to match the durable before-image.
    async fn cleanup_unapplied_file_effect(
        &self,
        scope: &WorkspaceFilesystemScope,
        plan: &WorkspaceFileEffectPlan,
        staging: Option<&WorkspaceStagingReceipt>,
    ) -> Result<(), WorkspaceFilesystemError>;

    /// Removes receipt-owned recovery objects only while every touched path
    /// still matches either its durable before-image or after-image. The
    /// checkpoint journal remains the recovery source after cleanup.
    async fn cleanup_recovery_file_effect(
        &self,
        scope: &WorkspaceFilesystemScope,
        plan: &WorkspaceFileEffectPlan,
        staging: Option<&WorkspaceStagingReceipt>,
    ) -> Result<(), WorkspaceFilesystemError>;

    async fn capture_checkpoint(
        &self,
        scope: &WorkspaceFilesystemScope,
        paths: Vec<ProjectRelativePath>,
    ) -> Result<CapturedWorkspaceCheckpoint, WorkspaceFilesystemError>;
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
    /// Optional outer-command identity (for example checkpoint restore) that
    /// remains stable even when its compiled file deltas become a no-op.
    pub request_intent_sha256: Option<ContentSha256>,
    pub prepared_at_unix_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateWorkspaceCheckpointCommand {
    pub checkpoint_id: CheckpointId,
    pub project_id: ProjectId,
    pub binding_id: WorkspaceBindingId,
    pub base_revision: WorkspaceRevision,
    pub correlation_id: String,
    pub label: String,
    pub request_summary: String,
    pub touched_paths: Vec<ProjectRelativePath>,
    pub artifacts: Vec<dennett_contracts::ArtifactId>,
    pub external_effects: Vec<dennett_contracts::EffectId>,
    pub provider_continuation: Option<CanonicalObjectRef>,
    pub created_at_unix_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestoreWorkspaceCheckpointCommand {
    pub operation_id: WorkspaceOperationId,
    pub command_id: CommandId,
    pub correlation_id: String,
    pub project_id: ProjectId,
    pub binding_id: WorkspaceBindingId,
    pub checkpoint_id: CheckpointId,
    pub expected_current_revision: WorkspaceRevision,
    pub prepared_at_unix_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CheckpointRestoreOutcome {
    Applied(Box<WorkspaceOperationRecord>),
    AlreadyMatches(WorkspaceSnapshotRecord),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckpointComparison {
    pub checkpoint: WorkspaceCheckpointRecord,
    pub current: WorkspaceSnapshotRecord,
    pub matching_paths: Vec<ProjectRelativePath>,
    pub changed_paths: Vec<ProjectRelativePath>,
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

    pub async fn create_checkpoint(
        &self,
        command: CreateWorkspaceCheckpointCommand,
    ) -> Result<WorkspaceCheckpointRecord, WorkspaceApplicationError> {
        validate_create_checkpoint_command(&command)?;
        if let Some(existing) = self.journal.load_checkpoint(command.checkpoint_id).await? {
            return if checkpoint_matches_create_command(&existing, &command) {
                Ok(existing)
            } else {
                Err(WorkspaceJournalError::IdempotencyConflict.into())
            };
        }

        let _writer = self.writers.acquire(command.binding_id).await;
        if let Some(existing) = self.journal.load_checkpoint(command.checkpoint_id).await? {
            return if checkpoint_matches_create_command(&existing, &command) {
                Ok(existing)
            } else {
                Err(WorkspaceJournalError::IdempotencyConflict.into())
            };
        }
        let authority = self
            .projects
            .prepare_agent_workspace(command.project_id, command.correlation_id.clone())
            .await?;
        let scope = scope_from_authority(&authority, command.binding_id, false)?;
        let base = self
            .journal
            .load_snapshot(command.base_revision)
            .await?
            .ok_or(WorkspaceApplicationError::SnapshotNotFound)?;
        validate_snapshot_owner(&base, command.project_id, command.binding_id)?;

        let captured = self
            .filesystem
            .capture_checkpoint(&scope, command.touched_paths.clone())
            .await?;
        let observed_manifest = captured
            .observation
            .clone()
            .into_manifest(command.base_revision)?;
        require_checkpoint_paths_match(&base.manifest, &captured.entries)?;
        require_checkpoint_paths_match(&observed_manifest, &captured.entries)?;
        let current = self
            .commit_observation(command.project_id, command.binding_id, captured.observation)
            .await?;
        require_checkpoint_paths_match(&current.manifest, &captured.entries)?;

        let checkpoint = WorkspaceCheckpointRecord {
            checkpoint_id: command.checkpoint_id,
            project_id: command.project_id,
            binding_id: command.binding_id,
            base_revision: command.base_revision,
            captured_revision: current.manifest.revision,
            state: DurableCheckpointState::Available,
            label: command.label,
            request_summary: command.request_summary,
            entries: captured.entries,
            artifacts: command.artifacts,
            external_effects: command.external_effects,
            provider_continuation: command.provider_continuation,
            created_at_unix_ms: command.created_at_unix_ms,
        };
        checkpoint.validate()?;
        self.journal
            .save_checkpoint(checkpoint, captured.blobs)
            .await
            .map_err(Into::into)
    }

    pub async fn compare_checkpoint(
        &self,
        project_id: ProjectId,
        binding_id: WorkspaceBindingId,
        checkpoint_id: CheckpointId,
        correlation_id: String,
    ) -> Result<CheckpointComparison, WorkspaceApplicationError> {
        validate_correlation(&correlation_id)?;
        let _writer = self.writers.acquire(binding_id).await;
        let authority = self
            .projects
            .prepare_agent_workspace(project_id, correlation_id)
            .await?;
        let scope = scope_from_authority(&authority, binding_id, false)?;
        let checkpoint = self
            .journal
            .load_checkpoint(checkpoint_id)
            .await?
            .ok_or(WorkspaceApplicationError::CheckpointNotFound)?;
        validate_checkpoint_owner(&checkpoint, project_id, binding_id)?;
        let observation = self.filesystem.observe_workspace(&scope).await?;
        let current = self
            .commit_observation(project_id, binding_id, observation)
            .await?;
        let mut matching_paths = Vec::new();
        let mut changed_paths = Vec::new();
        for entry in &checkpoint.entries {
            if current.manifest.state(&entry.path) == entry.state {
                matching_paths.push(entry.path.clone());
            } else {
                changed_paths.push(entry.path.clone());
            }
        }
        Ok(CheckpointComparison {
            checkpoint,
            current,
            matching_paths,
            changed_paths,
        })
    }

    pub async fn restore_checkpoint(
        &self,
        command: RestoreWorkspaceCheckpointCommand,
    ) -> Result<CheckpointRestoreOutcome, WorkspaceApplicationError> {
        validate_restore_checkpoint_command(&command)?;
        let restore_intent = hash_restore_intent(&command);
        if let Some(existing) = self
            .journal
            .load_operation_by_command(command.command_id)
            .await?
        {
            return if existing.plan.operation_id == command.operation_id
                && existing.plan.project_id == command.project_id
                && existing.plan.binding_id == command.binding_id
                && existing.plan.intent_sha256 == restore_intent
            {
                Ok(CheckpointRestoreOutcome::Applied(Box::new(existing)))
            } else {
                Err(WorkspaceJournalError::IdempotencyConflict.into())
            };
        }

        let writer = self.writers.acquire(command.binding_id).await;
        let authority = self
            .projects
            .prepare_agent_workspace(command.project_id, command.correlation_id.clone())
            .await?;
        let scope = scope_from_authority(&authority, command.binding_id, true)?;
        let checkpoint = self
            .journal
            .load_checkpoint(command.checkpoint_id)
            .await?
            .ok_or(WorkspaceApplicationError::CheckpointNotFound)?;
        validate_checkpoint_owner(&checkpoint, command.project_id, command.binding_id)?;
        if checkpoint.state == DurableCheckpointState::RecoveryRequired {
            return Err(WorkspaceApplicationError::CheckpointUnavailable);
        }
        let expected = self
            .journal
            .load_snapshot(command.expected_current_revision)
            .await?
            .ok_or(WorkspaceApplicationError::SnapshotNotFound)?;
        validate_snapshot_owner(&expected, command.project_id, command.binding_id)?;
        let mut observation = self.filesystem.observe_workspace(&scope).await?;
        let mut observed_manifest = observation
            .clone()
            .into_manifest(command.expected_current_revision)?;
        let conflicts = manifest_path_conflicts(
            &expected.manifest,
            &observed_manifest,
            checkpoint.entries.iter().map(|entry| &entry.path),
        )?;
        if !conflicts.is_empty() {
            let recovery_operation = self
                .journal
                .load_operation_by_checkpoint(command.checkpoint_id)
                .await?;
            let Some(recovery_operation) = recovery_operation else {
                return Err(WorkspaceApplicationError::Conflict(conflicts));
            };
            self.filesystem
                .cleanup_recovery_file_effect(
                    &scope,
                    &recovery_operation.plan,
                    recovery_operation.staging.as_ref(),
                )
                .await?;
            observation = self.filesystem.observe_workspace(&scope).await?;
            observed_manifest = observation
                .clone()
                .into_manifest(command.expected_current_revision)?;
            if !recovery_operation_recognizes_checkpoint(
                &recovery_operation,
                &checkpoint,
                &observed_manifest,
            ) {
                return Err(WorkspaceApplicationError::Conflict(
                    recovery_divergent_paths(&recovery_operation.plan, &observed_manifest),
                ));
            }
        }
        let current = self
            .commit_observation(command.project_id, command.binding_id, observation)
            .await?;
        let blobs = self
            .journal
            .load_checkpoint_blobs(command.checkpoint_id)
            .await?;
        let changes = restore_changes(&checkpoint, &current.manifest, &blobs)?;
        drop(scope);
        drop(authority);
        drop(writer);

        if changes.is_empty() {
            return Ok(CheckpointRestoreOutcome::AlreadyMatches(current));
        }
        let operation = self
            .apply_file_changes(ApplyWorkspaceFileChangesCommand {
                operation_id: command.operation_id,
                command_id: command.command_id,
                correlation_id: command.correlation_id,
                project_id: command.project_id,
                binding_id: command.binding_id,
                base_revision: current.manifest.revision,
                changes,
                request_intent_sha256: Some(restore_intent),
                prepared_at_unix_ms: command.prepared_at_unix_ms,
            })
            .await?;
        Ok(CheckpointRestoreOutcome::Applied(Box::new(operation)))
    }

    pub async fn apply_file_changes(
        &self,
        command: ApplyWorkspaceFileChangesCommand,
    ) -> Result<WorkspaceOperationRecord, WorkspaceApplicationError> {
        validate_apply_command(&command)?;
        let intent_sha256 = command
            .request_intent_sha256
            .map_or_else(|| hash_file_change_intent(&command.changes), Ok)?;
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
        let staging_nonce = generate_staging_nonce()?;
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
                staging_nonce,
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
            staging: None,
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

        let staging = match self
            .filesystem
            .stage_file_effect(&scope, &operation.plan, &blobs)
            .await
        {
            Ok(staging) => staging,
            Err(error) => {
                return self
                    .classify_failed_application(operation, &scope, error)
                    .await;
            }
        };
        let mut staged_operation = operation.clone();
        staged_operation.staging = Some(staging.clone());
        staged_operation.validate()?;
        let operation = match self
            .journal
            .transition_operation(
                DurableWorkspaceOperationState::Prepared,
                staged_operation,
                None,
            )
            .await
        {
            Ok(operation) => operation,
            Err(error) => {
                let _ = self
                    .filesystem
                    .cleanup_unapplied_file_effect(&scope, &operation.plan, Some(&staging))
                    .await;
                return Err(error.into());
            }
        };

        match self
            .filesystem
            .apply_file_effect(
                &scope,
                &operation.plan,
                operation
                    .staging
                    .as_ref()
                    .ok_or(WorkspacePlanError::InvalidOperationRecord)?,
            )
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
                    if self
                        .filesystem
                        .cleanup_unapplied_file_effect(
                            &scope,
                            &operation.plan,
                            operation.staging.as_ref(),
                        )
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
                        self.mark_failed(
                            operation,
                            DurableWorkspaceFailureKind::AdapterFailure,
                            "workspace.effect.unapplied_after_restart",
                            vec![],
                        )
                        .await?
                    }
                }
                OperationObservation::After => {
                    if self
                        .filesystem
                        .cleanup_file_effect(&scope, &operation.plan, operation.staging.as_ref())
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
        let (resulting, publication) = self
            .prepare_observation_publication(
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
            .transition_operation(operation.state, succeeded, publication)
            .await
            .map_err(Into::into)
    }

    async fn prepare_observation_publication(
        &self,
        project_id: ProjectId,
        binding_id: WorkspaceBindingId,
        expected_head: WorkspaceRevision,
        observation: WorkspaceObservation,
    ) -> Result<
        (
            WorkspaceSnapshotRecord,
            Option<WorkspaceSnapshotPublication>,
        ),
        WorkspaceApplicationError,
    > {
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
            return Ok((head, None));
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
        Ok((
            snapshot.clone(),
            Some(WorkspaceSnapshotPublication {
                expected_head,
                snapshot,
            }),
        ))
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
                if self
                    .filesystem
                    .cleanup_unapplied_file_effect(
                        scope,
                        &operation.plan,
                        operation.staging.as_ref(),
                    )
                    .await
                    .is_err()
                {
                    self.mark_recovery_required(
                        operation,
                        "workspace.effect.cleanup_failed",
                        vec![],
                    )
                    .await
                } else {
                    self.mark_failed(
                        operation,
                        failure_kind_for_filesystem_error(&error),
                        error.safe_code(),
                        vec![],
                    )
                    .await
                }
            }
            OperationObservation::After => {
                if self
                    .filesystem
                    .cleanup_file_effect(scope, &operation.plan, operation.staging.as_ref())
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
        kind: DurableWorkspaceFailureKind,
        safe_code: &str,
        conflicting_paths: Vec<ProjectRelativePath>,
    ) -> Result<WorkspaceOperationRecord, WorkspaceApplicationError> {
        self.mark_terminal_failure(
            operation,
            DurableWorkspaceOperationState::Failed,
            kind,
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
    validate_correlation(&command.correlation_id)?;
    if command.base_revision.binding_id() != command.binding_id {
        return Err(WorkspaceApplicationError::InvalidRequest);
    }
    Ok(())
}

fn validate_correlation(value: &str) -> Result<(), WorkspaceApplicationError> {
    if value.is_empty() || value.len() > 256 {
        Err(WorkspaceApplicationError::InvalidRequest)
    } else {
        Ok(())
    }
}

fn validate_create_checkpoint_command(
    command: &CreateWorkspaceCheckpointCommand,
) -> Result<(), WorkspaceApplicationError> {
    validate_correlation(&command.correlation_id)?;
    if command.base_revision.binding_id() != command.binding_id
        || command.label.len() > 256
        || command.request_summary.len() > 8 * 1024
        || command.touched_paths.is_empty()
        || command.touched_paths.len() > 256
    {
        return Err(WorkspaceApplicationError::InvalidRequest);
    }
    if let Some(reference) = &command.provider_continuation {
        reference.validate()?;
    }
    let unique = command
        .touched_paths
        .iter()
        .map(ProjectRelativePath::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    if unique.len() != command.touched_paths.len() {
        return Err(WorkspaceApplicationError::InvalidRequest);
    }
    Ok(())
}

fn validate_restore_checkpoint_command(
    command: &RestoreWorkspaceCheckpointCommand,
) -> Result<(), WorkspaceApplicationError> {
    validate_correlation(&command.correlation_id)?;
    if command.expected_current_revision.binding_id() != command.binding_id {
        return Err(WorkspaceApplicationError::InvalidRequest);
    }
    Ok(())
}

fn validate_snapshot_owner(
    snapshot: &WorkspaceSnapshotRecord,
    project_id: ProjectId,
    binding_id: WorkspaceBindingId,
) -> Result<(), WorkspaceApplicationError> {
    if snapshot.project_id != project_id || snapshot.manifest.revision.binding_id() != binding_id {
        Err(WorkspaceApplicationError::BindingMismatch)
    } else {
        Ok(())
    }
}

fn validate_checkpoint_owner(
    checkpoint: &WorkspaceCheckpointRecord,
    project_id: ProjectId,
    binding_id: WorkspaceBindingId,
) -> Result<(), WorkspaceApplicationError> {
    if checkpoint.project_id != project_id || checkpoint.binding_id != binding_id {
        Err(WorkspaceApplicationError::BindingMismatch)
    } else {
        Ok(())
    }
}

fn checkpoint_matches_create_command(
    checkpoint: &WorkspaceCheckpointRecord,
    command: &CreateWorkspaceCheckpointCommand,
) -> bool {
    checkpoint.project_id == command.project_id
        && checkpoint.binding_id == command.binding_id
        && checkpoint.base_revision == command.base_revision
        && checkpoint.label == command.label
        && checkpoint.request_summary == command.request_summary
        && checkpoint
            .entries
            .iter()
            .map(|entry| entry.path.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            == command
                .touched_paths
                .iter()
                .map(ProjectRelativePath::as_str)
                .collect::<std::collections::BTreeSet<_>>()
        && checkpoint.artifacts == command.artifacts
        && checkpoint.external_effects == command.external_effects
        && checkpoint.provider_continuation == command.provider_continuation
        && checkpoint.created_at_unix_ms == command.created_at_unix_ms
}

fn require_checkpoint_paths_match(
    manifest: &WorkspaceManifest,
    entries: &[WorkspaceCheckpointEntry],
) -> Result<(), WorkspaceApplicationError> {
    let conflicts = entries
        .iter()
        .filter(|entry| manifest.state(&entry.path) != entry.state)
        .map(|entry| entry.path.clone())
        .collect::<Vec<_>>();
    if conflicts.is_empty() {
        Ok(())
    } else {
        Err(WorkspaceApplicationError::Conflict(conflicts))
    }
}

fn manifest_path_conflicts<'a>(
    expected: &WorkspaceManifest,
    observed: &WorkspaceManifest,
    paths: impl IntoIterator<Item = &'a ProjectRelativePath>,
) -> Result<Vec<ProjectRelativePath>, WorkspaceApplicationError> {
    if !expected.complete || !observed.complete || expected.scope_sha256 != observed.scope_sha256 {
        return Err(WorkspaceFilesystemError::BoundExceeded.into());
    }
    Ok(paths
        .into_iter()
        .filter(|path| expected.state(path) != observed.state(path))
        .cloned()
        .collect())
}

fn recovery_operation_recognizes_checkpoint(
    operation: &WorkspaceOperationRecord,
    checkpoint: &WorkspaceCheckpointRecord,
    observed: &WorkspaceManifest,
) -> bool {
    if operation.state != DurableWorkspaceOperationState::RecoveryRequired
        || operation.plan.safety_checkpoint_id != checkpoint.checkpoint_id
        || operation.plan.project_id != checkpoint.project_id
        || operation.plan.binding_id != checkpoint.binding_id
        || !observed.complete
        || observed.scope_sha256 != operation.plan.scope_sha256
        || operation.plan.transitions.len() != checkpoint.entries.len()
    {
        return false;
    }
    let checkpoint_by_path = checkpoint
        .entries
        .iter()
        .map(|entry| (entry.path.as_str(), &entry.state))
        .collect::<BTreeMap<_, _>>();
    operation.plan.transitions.iter().all(|transition| {
        checkpoint_by_path.get(transition.path.as_str()).copied() == Some(&transition.before)
            && matches!(
                classify_transition(transition, &observed.state(&transition.path)),
                dennett_effect_core::workspace::TransitionObservation::Before
                    | dennett_effect_core::workspace::TransitionObservation::After
            )
    })
}

fn recovery_divergent_paths(
    plan: &WorkspaceFileEffectPlan,
    observed: &WorkspaceManifest,
) -> Vec<ProjectRelativePath> {
    plan.transitions
        .iter()
        .filter(|transition| {
            matches!(
                classify_transition(transition, &observed.state(&transition.path)),
                dennett_effect_core::workspace::TransitionObservation::Diverged
            )
        })
        .map(|transition| transition.path.clone())
        .collect()
}

fn restore_changes(
    checkpoint: &WorkspaceCheckpointRecord,
    current: &WorkspaceManifest,
    blobs: &[WorkspaceBlob],
) -> Result<Vec<WorkspaceFileChangeInput>, WorkspaceApplicationError> {
    let blob_by_id = blobs
        .iter()
        .map(|blob| (blob.reference.content_id.as_str(), blob))
        .collect::<BTreeMap<_, _>>();
    let mut changes = Vec::new();
    for entry in &checkpoint.entries {
        let observed = current.state(&entry.path);
        if observed == entry.state {
            continue;
        }
        let (kind, content, expected_content_sha256, resulting_permissions) = match &entry.state {
            WorkspacePathState::Absent => {
                let expected = observed
                    .content_sha256()
                    .ok_or_else(|| WorkspaceApplicationError::Conflict(vec![entry.path.clone()]))?;
                (FileMutationKind::Delete, None, Some(expected), None)
            }
            WorkspacePathState::RegularFile { .. } => {
                let reference = entry
                    .content
                    .as_ref()
                    .ok_or(WorkspaceApplicationError::InvalidCheckpointCapture)?;
                let blob = blob_by_id
                    .get(reference.content_id.as_str())
                    .ok_or(WorkspaceJournalError::Integrity)?;
                blob.validate()?;
                if &blob.reference != reference {
                    return Err(WorkspaceJournalError::Integrity.into());
                }
                let permissions = entry
                    .permissions
                    .ok_or(WorkspaceApplicationError::InvalidCheckpointCapture)?;
                match observed {
                    WorkspacePathState::Absent => (
                        FileMutationKind::Add,
                        Some(blob.bytes.clone()),
                        None,
                        Some(permissions),
                    ),
                    WorkspacePathState::RegularFile { content_sha256, .. } => (
                        FileMutationKind::Modify,
                        Some(blob.bytes.clone()),
                        Some(content_sha256),
                        Some(permissions),
                    ),
                    WorkspacePathState::Directory { .. }
                    | WorkspacePathState::Link { .. }
                    | WorkspacePathState::Other { .. } => {
                        return Err(WorkspaceApplicationError::Conflict(vec![
                            entry.path.clone(),
                        ]));
                    }
                }
            }
            WorkspacePathState::Directory { .. }
            | WorkspacePathState::Link { .. }
            | WorkspacePathState::Other { .. } => {
                return Err(WorkspaceApplicationError::CheckpointUnavailable);
            }
        };
        changes.push(WorkspaceFileChangeInput {
            kind,
            path: entry.path.clone(),
            previous_path: None,
            content,
            expected_content_sha256,
            resulting_permissions,
        });
    }
    Ok(changes)
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
        match change.resulting_permissions {
            Some(permissions) => {
                hasher.update([
                    1,
                    u8::from(permissions.read_only),
                    u8::from(permissions.executable),
                ]);
                match permissions.unix_mode {
                    Some(mode) => {
                        hasher.update([1]);
                        hasher.update(mode.to_be_bytes());
                    }
                    None => hasher.update([0]),
                }
            }
            None => hasher.update([0]),
        }
    }
    Ok(ContentSha256(hasher.finalize().into()))
}

fn generate_staging_nonce() -> Result<WorkspaceStagingNonce, WorkspaceApplicationError> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| WorkspaceFilesystemError::AdapterUnavailable)?;
    let nonce = WorkspaceStagingNonce(bytes);
    nonce.validate()?;
    Ok(nonce)
}

fn hash_restore_intent(command: &RestoreWorkspaceCheckpointCommand) -> ContentSha256 {
    let mut hasher = Sha256::new();
    hasher.update(b"dennett.workspace-checkpoint-restore.v1\0");
    hasher.update(command.project_id.0.as_bytes());
    hasher.update(command.binding_id.0.as_bytes());
    hasher.update(command.checkpoint_id.0.as_bytes());
    hasher.update(command.expected_current_revision.binding_id().0.as_bytes());
    hasher.update(command.expected_current_revision.snapshot_id().0.as_bytes());
    hasher.update(command.expected_current_revision.sequence().to_be_bytes());
    ContentSha256(hasher.finalize().into())
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
    let mut staged_bytes = 0_u64;
    for blob in blobs {
        blob.validate()?;
        match by_id.insert(blob.reference.content_id.as_str(), &blob.reference) {
            Some(existing) if existing != &blob.reference => {
                return Err(WorkspacePlanError::ContentReferenceCollision.into());
            }
            Some(_) => return Err(WorkspacePlanError::ContentReferenceCollision.into()),
            None => {
                staged_bytes = staged_bytes
                    .checked_add(blob.reference.byte_size)
                    .filter(|value| {
                        *value <= dennett_effect_core::workspace::MAX_STAGED_OPERATION_BYTES
                    })
                    .ok_or(WorkspacePlanError::OperationContentTooLarge)?;
            }
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

fn failure_kind_for_filesystem_error(
    error: &WorkspaceFilesystemError,
) -> DurableWorkspaceFailureKind {
    match error {
        WorkspaceFilesystemError::Conflict => DurableWorkspaceFailureKind::Conflict,
        WorkspaceFilesystemError::ScopeDenied
        | WorkspaceFilesystemError::UnsupportedObject
        | WorkspaceFilesystemError::BoundExceeded => DurableWorkspaceFailureKind::ScopeDenied,
        WorkspaceFilesystemError::RecoveryRequired => DurableWorkspaceFailureKind::RecoveryRequired,
        WorkspaceFilesystemError::LocationMissing
        | WorkspaceFilesystemError::AdapterUnavailable => {
            DurableWorkspaceFailureKind::AdapterFailure
        }
    }
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
    #[error("workspace checkpoint was not found")]
    CheckpointNotFound,
    #[error("workspace checkpoint cannot be restored safely")]
    CheckpointUnavailable,
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

#[cfg(test)]
mod privacy_tests {
    use super::*;

    #[test]
    fn filesystem_scope_debug_output_redacts_machine_specific_authority() {
        let source_identity = WorkspaceSourceIdentity::new([0x5a; 32]);
        let leaked_identity = format!("{source_identity:?}");
        let scope = WorkspaceFilesystemScope {
            project_id: ProjectId::new(),
            binding_id: WorkspaceBindingId::new(),
            absolute_path: r"C:\Users\owner\secret-project".to_owned(),
            source_identity,
            writable: true,
        };

        let debug = format!("{scope:?}");
        assert!(!debug.contains("secret-project"));
        assert!(!debug.contains(&leaked_identity));
        assert!(debug.contains("<redacted>"));
    }

    #[test]
    fn workspace_file_intent_hash_includes_exact_unix_mode() {
        let change = |unix_mode| WorkspaceFileChangeInput {
            kind: FileMutationKind::Modify,
            path: ProjectRelativePath::try_from("src/main.rs").expect("valid project path"),
            previous_path: None,
            content: Some(b"fn main() {}\n".to_vec()),
            expected_content_sha256: None,
            resulting_permissions: Some(PortableFilePermissions {
                read_only: false,
                executable: false,
                unix_mode: Some(unix_mode),
            }),
        };

        let owner_only =
            hash_file_change_intent(&[change(0o600)]).expect("owner-only intent hashes");
        let owner_and_group =
            hash_file_change_intent(&[change(0o640)]).expect("owner-and-group intent hashes");

        assert_ne!(owner_only, owner_and_group);
    }
}
