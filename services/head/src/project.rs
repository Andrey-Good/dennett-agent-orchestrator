//! Project registration application boundary.
//!
//! Head coordinates durable project state with Node-owned filesystem effects.
//! Paths never become identity or permission authority at this boundary.

use async_trait::async_trait;
use dennett_contracts::{
    CommandId, PortableMetadataAction, PortableProjectMetadataState, ProjectId,
    ProjectInspectionId, ProjectTrustState, RebindPortableMetadataAction, SessionId,
    WorkspaceBindingId, WorkspaceOperationId,
};
use dennett_memory_core::session::{ProjectSessionSnapshot, SessionJournalError};
use dennett_trust_core::project_registry::{
    BindingObservationUpdate, InstructionFingerprintUpdate, ProjectAccessPolicy,
    ProjectAccessPolicyUpdate, ProjectAggregate, ProjectLocationInspection,
    ProjectRegistrationCommit, ProjectRegistrationKind, ProjectRegistrationOperation,
    ProjectRegistrationPlan, ProjectRegistrationTarget, ProjectRegistryError, ProjectRegistryPort,
    ProjectWorkspaceRebindPlan, ProjectWorkspaceRebindReceipt, RegistrationFilesystemApplied,
    RegistrationFilesystemObservation, RegistrationOperationState, RegistrationStateUpdate,
    TrustDecisionRef, WorkspaceAvailability, WorkspaceBinding,
    verify_bridge_attested_project_trust_decision,
};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    hash::Hash,
    sync::{Arc, Weak},
};
use tokio::sync::{Mutex, OwnedMutexGuard};

use crate::conversation::session_summary;
use crate::session::SessionCoordinator;
use crate::system::{ProjectState, ProjectSummary, SystemMutation, SystemProjection};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InspectProjectLocationCommand {
    pub registration_kind: ProjectRegistrationKind,
    pub root_uri: String,
    pub observed_at_unix_ms: u64,
    pub expires_at_unix_ms: u64,
}

/// Node-owned filesystem operations required by the project application.
///
/// Implementations must revalidate the selected filesystem object around every
/// mutation and return final observed facts. They must never infer trust from
/// project contents.
#[async_trait]
pub trait ProjectLocationPort: Send + Sync {
    async fn inspect(
        &self,
        command: InspectProjectLocationCommand,
    ) -> Result<ProjectLocationInspection, ProjectLocationError>;

    async fn apply_registration_effect(
        &self,
        inspection: &ProjectLocationInspection,
        action: PortableMetadataAction,
        project_id: ProjectId,
    ) -> Result<RegistrationFilesystemObservation, ProjectLocationError>;

    async fn apply_rebind_effect(
        &self,
        inspection: &ProjectLocationInspection,
        action: RebindPortableMetadataAction,
        project_id: ProjectId,
    ) -> Result<RegistrationFilesystemObservation, ProjectLocationError>;

    async fn observe_binding(
        &self,
        binding: &WorkspaceBinding,
        observed_at_unix_ms: u64,
    ) -> Result<RegistrationFilesystemObservation, ProjectLocationError>;
}

#[derive(Clone, Debug, thiserror::Error, Eq, PartialEq)]
pub enum ProjectLocationError {
    #[error("project location request is invalid")]
    InvalidRequest,
    #[error("project location is missing")]
    Missing,
    #[error("project location is inaccessible")]
    Inaccessible,
    #[error("project location contains an unsafe link or reparse point")]
    UnsafeLink,
    #[error("project location changed after inspection")]
    IdentityChanged,
    #[error("portable project metadata conflicts with the requested action")]
    PortableMetadataConflict,
    #[error("project instruction discovery exceeded its safe bounds")]
    InspectionIncomplete,
    #[error("project filesystem effect may require recovery")]
    RecoveryRequired,
    #[error("project filesystem adapter is temporarily unavailable")]
    AdapterUnavailable,
}

impl ProjectLocationError {
    #[must_use]
    pub const fn safe_code(&self) -> &'static str {
        match self {
            Self::InvalidRequest => "project.location.invalid",
            Self::Missing => "project.location.missing",
            Self::Inaccessible => "project.location.inaccessible",
            Self::UnsafeLink => "project.location.unsafe_link",
            Self::IdentityChanged => "project.location.identity_changed",
            Self::PortableMetadataConflict => "project.metadata.conflict",
            Self::InspectionIncomplete => "project.inspection.incomplete",
            Self::RecoveryRequired => "project.registration.recovery_required",
            Self::AdapterUnavailable => "project.location.adapter_unavailable",
        }
    }

    #[must_use]
    pub const fn retryable(&self) -> bool {
        matches!(self, Self::AdapterUnavailable)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegisterProjectCommand {
    pub command_id: CommandId,
    pub correlation_id: String,
    pub intent_sha256: [u8; 32],
    pub inspection_id: ProjectInspectionId,
    pub display_name: String,
    pub portable_metadata_action: PortableMetadataAction,
    pub initial_trust_state: Option<ProjectTrustState>,
    pub trust_decision: Option<TrustDecisionRef>,
    pub committed_at_unix_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegisteredProject {
    pub project: ProjectAggregate,
    pub direct_session: ProjectSessionSnapshot,
    pub operation: ProjectRegistrationOperation,
}

pub struct AgentProjectWorkspace {
    pub project_id: ProjectId,
    pub binding_id: WorkspaceBindingId,
    pub absolute_path: String,
    pub access_mode: dennett_trust_core::project_registry::WorkspaceAccessMode,
    pub policy_revision: u64,
    pub binding_revision: u64,
    authority: ProjectAuthorityPermit,
}

impl AgentProjectWorkspace {
    pub(crate) fn into_runtime_parts(self) -> (String, ProjectAuthorityPermit) {
        (self.absolute_path, self.authority)
    }
}

/// Keeps one project authority snapshot linearizable until the caller has
/// admitted its provider or workspace effect. Trust revocation and rebinding
/// take the exclusive side of the same gate.
pub(crate) struct ProjectAuthorityPermit {
    _guard: OwnedMutexGuard<()>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetProjectTrustCommand {
    pub command_id: CommandId,
    pub correlation_id: String,
    pub project_id: ProjectId,
    pub target_state: ProjectTrustState,
    pub expected_policy_revision: u64,
    pub trust_decision: TrustDecisionRef,
    pub committed_at_unix_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RebindProjectCommand {
    pub command_id: CommandId,
    pub correlation_id: String,
    pub intent_sha256: [u8; 32],
    pub project_id: ProjectId,
    pub current_binding_id: WorkspaceBindingId,
    pub inspection_id: ProjectInspectionId,
    pub portable_metadata_action: RebindPortableMetadataAction,
    pub committed_at_unix_ms: u64,
}

#[derive(Clone)]
pub struct ProjectApplication {
    registry: Arc<dyn ProjectRegistryPort>,
    locations: Arc<dyn ProjectLocationPort>,
    sessions: SessionCoordinator,
    system: Arc<SystemProjection>,
    command_locks: KeyedLocks<CommandId>,
    authority_locks: KeyedLocks<ProjectId>,
}

#[derive(Clone)]
struct KeyedLocks<K> {
    locks: Arc<Mutex<HashMap<K, Weak<Mutex<()>>>>>,
}

impl<K> Default for KeyedLocks<K> {
    fn default() -> Self {
        Self {
            locks: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl<K> KeyedLocks<K>
where
    K: Copy + Eq + Hash,
{
    async fn acquire(&self, key: K) -> OwnedMutexGuard<()> {
        let lock = {
            let mut locks = self.locks.lock().await;
            locks.retain(|_, lock| lock.strong_count() > 0);
            if let Some(lock) = locks.get(&key).and_then(Weak::upgrade) {
                lock
            } else {
                let lock = Arc::new(Mutex::new(()));
                locks.insert(key, Arc::downgrade(&lock));
                lock
            }
        };
        lock.lock_owned().await
    }
}

impl ProjectApplication {
    #[must_use]
    pub fn new(
        registry: Arc<dyn ProjectRegistryPort>,
        locations: Arc<dyn ProjectLocationPort>,
        sessions: SessionCoordinator,
        system: Arc<SystemProjection>,
    ) -> Self {
        Self {
            registry,
            locations,
            sessions,
            system,
            command_locks: KeyedLocks::default(),
            authority_locks: KeyedLocks::default(),
        }
    }

    pub async fn inspect_location(
        &self,
        command: InspectProjectLocationCommand,
    ) -> Result<ProjectLocationInspection, ProjectApplicationError> {
        let inspection = self.locations.inspect(command).await?;
        if inspection.instruction_discovery_incomplete {
            return Err(ProjectLocationError::InspectionIncomplete.into());
        }
        Ok(self.registry.save_inspection(inspection).await?)
    }

    pub async fn list_projects(&self) -> Result<Vec<ProjectAggregate>, ProjectApplicationError> {
        self.registry.list_projects().await.map_err(Into::into)
    }

    pub async fn get_project(
        &self,
        project_id: ProjectId,
    ) -> Result<ProjectAggregate, ProjectApplicationError> {
        self.registry
            .get_project(project_id)
            .await?
            .ok_or(ProjectApplicationError::ProjectNotFound)
    }

    /// Revalidates the primary binding immediately before an agent turn.
    ///
    /// Stored trust is necessary but never sufficient: an unavailable,
    /// replaced or identity-conflicting folder is denied and projected before
    /// the provider can receive a workspace path.
    pub async fn prepare_agent_workspace(
        &self,
        project_id: ProjectId,
        correlation_id: String,
    ) -> Result<AgentProjectWorkspace, ProjectApplicationError> {
        for _ in 0..3 {
            let project = self.get_project(project_id).await?;
            require_trusted_policy(&project.access_policy)?;
            match self
                .refresh_primary_binding(&project, correlation_id.clone())
                .await
            {
                Ok((binding, observation)) => {
                    // Filesystem inspection is intentionally outside the
                    // authority gate so a slow bounded scan cannot delay a
                    // user revocation. Acquire after inspection, reload the
                    // policy and binding, and keep this permit until the
                    // provider accepts the effect.
                    let authority_guard = self.authority_locks.acquire(project_id).await;
                    let current = self.get_project(project_id).await?;
                    require_trusted_policy(&current.access_policy)?;
                    let Some(current_binding) = current.bindings.iter().find(|candidate| {
                        candidate.binding_id == current.project.primary_binding_id
                    }) else {
                        continue;
                    };
                    if current_binding.binding_id != binding.binding_id
                        || current_binding.record_revision != binding.record_revision
                        || current_binding.location != binding.location
                        || current_binding.source_identity != observation.source_identity
                        || current_binding.availability != observation.availability
                        || current_binding.access_mode != observation.access_mode
                    {
                        continue;
                    }
                    return Ok(AgentProjectWorkspace {
                        project_id,
                        binding_id: current_binding.binding_id,
                        absolute_path: current_binding.location.path.expose_local().to_owned(),
                        access_mode: current_binding.access_mode,
                        policy_revision: current.access_policy.revision,
                        binding_revision: current_binding.record_revision,
                        authority: ProjectAuthorityPermit {
                            _guard: authority_guard,
                        },
                    });
                }
                Err(ProjectApplicationError::Registry(
                    ProjectRegistryError::RevisionConflict { .. },
                )) => continue,
                Err(error) => return Err(error),
            }
        }
        Err(ProjectApplicationError::ConcurrentChange)
    }

    /// Refreshes all persisted bindings after restart without granting trust
    /// or starting providers. Failures stay visible per project and do not
    /// prevent unrelated projects from loading.
    pub async fn reconcile_project_locations(
        &self,
        correlation_id: &str,
    ) -> Result<Vec<(ProjectId, Result<(), ProjectApplicationError>)>, ProjectApplicationError>
    {
        let projects = self.list_projects().await?;
        let mut results = Vec::with_capacity(projects.len());
        for project in projects {
            let project_id = project.project.project_id;
            let _authority_guard = self.authority_locks.acquire(project_id).await;
            let result = match self.get_project(project_id).await {
                Ok(current) => self
                    .refresh_primary_binding(&current, correlation_id.to_owned())
                    .await
                    .map(|_| ()),
                Err(error) => Err(error),
            };
            results.push((project_id, result));
        }
        Ok(results)
    }

    pub async fn register_project(
        &self,
        command: RegisterProjectCommand,
    ) -> Result<RegisteredProject, ProjectApplicationError> {
        let _command_guard = self.command_locks.acquire(command.command_id).await;
        if let Some(operation) = self.registry.load_registration(command.command_id).await? {
            if operation.plan.intent_sha256 != command.intent_sha256
                || operation.plan.correlation_id != command.correlation_id
            {
                return Err(ProjectRegistryError::IdempotencyConflict.into());
            }
            let _authority_guard =
                if operation.plan.target == ProjectRegistrationTarget::ExistingProject {
                    Some(
                        self.authority_locks
                            .acquire(operation.plan.project_id)
                            .await,
                    )
                } else {
                    None
                };
            return self.finish_registration(operation).await;
        }

        let inspection = self
            .registry
            .load_inspection(command.inspection_id, command.committed_at_unix_ms)
            .await?;
        validate_registration_action(&inspection, command.portable_metadata_action)?;
        let (project_id, target) = self
            .registration_target(&inspection, command.portable_metadata_action)
            .await?;
        let _authority_guard = if target == ProjectRegistrationTarget::ExistingProject {
            Some(self.authority_locks.acquire(project_id).await)
        } else {
            None
        };
        let initial_trust = initial_trust_grant(
            command.command_id,
            project_id,
            target,
            command.initial_trust_state,
            command.trust_decision,
        )?;
        let direct_session_id = self
            .direct_session_id_for_registration(target, project_id, command.committed_at_unix_ms)
            .await?;
        let plan = ProjectRegistrationPlan {
            // Local command admission uses the command UUID as its operation
            // UUID. Reuse it here so IPC acceptance, durable recovery and
            // lifecycle evidence never expose two identities for one action.
            operation_id: WorkspaceOperationId(command.command_id.0),
            command_id: command.command_id,
            correlation_id: command.correlation_id,
            intent_sha256: command.intent_sha256,
            inspection_id: inspection.inspection_id,
            target,
            project_id,
            binding_id: WorkspaceBindingId::new(),
            direct_session_id,
            display_name: command.display_name,
            location: inspection.location.clone(),
            source_identity: inspection.source_identity,
            workspace_kind: inspection.workspace_kind,
            availability: inspection.availability,
            access_mode: inspection.access_mode,
            portable_metadata_state: inspection.portable_metadata_state,
            portable_project_id: inspection.portable_project_id,
            portable_metadata_action: command.portable_metadata_action,
            instruction_fingerprint: inspection.instruction_fingerprint,
            instruction_source_count: inspection.instruction_source_count,
            initial_trust,
            prepared_at_unix_ms: command.committed_at_unix_ms,
        };
        let operation = self.registry.prepare_registration(plan).await?;
        self.finish_registration(operation).await
    }

    pub async fn reconcile_registrations(
        &self,
    ) -> Vec<Result<RegisteredProject, ProjectApplicationError>> {
        let operations = match self.registry.list_reconcilable_registrations().await {
            Ok(operations) => operations,
            Err(error) => return vec![Err(error.into())],
        };
        let mut results = Vec::with_capacity(operations.len());
        for operation in operations {
            let _authority_guard =
                if operation.plan.target == ProjectRegistrationTarget::ExistingProject {
                    Some(
                        self.authority_locks
                            .acquire(operation.plan.project_id)
                            .await,
                    )
                } else {
                    None
                };
            results.push(self.finish_registration(operation).await);
        }
        results
    }

    pub async fn set_project_trust(
        &self,
        command: SetProjectTrustCommand,
    ) -> Result<ProjectAccessPolicy, ProjectApplicationError> {
        let _command_guard = self.command_locks.acquire(command.command_id).await;
        let _authority_guard = self.authority_locks.acquire(command.project_id).await;
        let grant = verify_bridge_attested_project_trust_decision(
            command.trust_decision.clone(),
            command.command_id,
            command.project_id,
            command.target_state,
        )?;
        let current = self.get_project(command.project_id).await?;
        if command
            .expected_policy_revision
            .checked_add(1)
            .is_some_and(|revision| current.access_policy.revision == revision)
            && current.access_policy.trust_state == command.target_state
            && current.access_policy.last_decision.as_ref() == Some(&command.trust_decision)
        {
            return Ok(current.access_policy);
        }
        let policy = self
            .registry
            .compare_and_set_access_policy(ProjectAccessPolicyUpdate {
                project_id: command.project_id,
                expected_revision: command.expected_policy_revision,
                grant,
                command_id: command.command_id,
                correlation_id: command.correlation_id,
                updated_at_unix_ms: command.committed_at_unix_ms,
            })
            .await?;
        let refreshed = self.get_project(command.project_id).await?;
        self.publish_project(&refreshed).await;
        Ok(policy)
    }

    pub async fn rebind_project(
        &self,
        command: RebindProjectCommand,
    ) -> Result<ProjectWorkspaceRebindReceipt, ProjectApplicationError> {
        let _command_guard = self.command_locks.acquire(command.command_id).await;
        let _authority_guard = self.authority_locks.acquire(command.project_id).await;
        let project = self.get_project(command.project_id).await?;
        let current = project
            .bindings
            .iter()
            .find(|binding| binding.binding_id == command.current_binding_id)
            .cloned()
            .ok_or(ProjectApplicationError::BindingNotFound)?;
        let inspection = self
            .registry
            .load_inspection(command.inspection_id, command.committed_at_unix_ms)
            .await?;
        if inspection.registration_kind != ProjectRegistrationKind::AttachExisting {
            return Err(ProjectApplicationError::InvalidRequest);
        }
        let final_observation = self
            .locations
            .apply_rebind_effect(
                &inspection,
                command.portable_metadata_action,
                command.project_id,
            )
            .await?;
        let replacement_binding_id = if final_observation.location.key == current.location.key {
            current.binding_id
        } else {
            WorkspaceBindingId::new()
        };
        let receipt = self
            .registry
            .rebind_project_workspace(ProjectWorkspaceRebindPlan {
                command_id: command.command_id,
                correlation_id: command.correlation_id,
                intent_sha256: command.intent_sha256,
                project_id: command.project_id,
                current_binding_id: command.current_binding_id,
                expected_current_binding_revision: current.record_revision,
                expected_current_source_identity: current.source_identity,
                replacement_binding_id,
                inspection_id: inspection.inspection_id,
                portable_metadata_action: command.portable_metadata_action,
                final_observation,
                safe_code: "project.workspace.rebound".to_owned(),
                rebound_at_unix_ms: command.committed_at_unix_ms,
            })
            .await?;
        let refreshed = self.get_project(command.project_id).await?;
        self.publish_project(&refreshed).await;
        Ok(receipt)
    }

    async fn registration_target(
        &self,
        inspection: &ProjectLocationInspection,
        action: PortableMetadataAction,
    ) -> Result<(ProjectId, ProjectRegistrationTarget), ProjectApplicationError> {
        match action {
            PortableMetadataAction::UseExisting => {
                let project_id = inspection
                    .portable_project_id
                    .ok_or(ProjectApplicationError::InvalidRequest)?;
                let target = if self.registry.get_project(project_id).await?.is_some() {
                    ProjectRegistrationTarget::ExistingProject
                } else {
                    ProjectRegistrationTarget::NewProject
                };
                Ok((project_id, target))
            }
            PortableMetadataAction::LeaveAbsent
            | PortableMetadataAction::CreateMinimal
            | PortableMetadataAction::ForkWithNewIdentity => {
                Ok((ProjectId::new(), ProjectRegistrationTarget::NewProject))
            }
        }
    }

    async fn direct_session_id_for_registration(
        &self,
        target: ProjectRegistrationTarget,
        project_id: ProjectId,
        committed_at_unix_ms: u64,
    ) -> Result<SessionId, ProjectApplicationError> {
        if target == ProjectRegistrationTarget::NewProject {
            return Ok(SessionId::new());
        }
        if let Some(snapshot) = self
            .sessions
            .restore_all()
            .await?
            .into_iter()
            .filter(|snapshot| snapshot.session.project_id == Some(project_id))
            .max_by_key(|snapshot| snapshot.session.last_activity_unix_ms)
        {
            return Ok(snapshot.session.session_id);
        }
        let command_id = deterministic_id("existing-project-direct-session-command", project_id);
        let session_id =
            SessionId(deterministic_id("existing-project-direct-session", project_id).0);
        Ok(self
            .sessions
            .create_session_with_id(
                command_id,
                session_id,
                Some(project_id),
                "Untitled chat".to_owned(),
                committed_at_unix_ms,
            )
            .await?
            .snapshot
            .session
            .session_id)
    }

    async fn refresh_primary_binding(
        &self,
        project: &ProjectAggregate,
        correlation_id: String,
    ) -> Result<(WorkspaceBinding, RegistrationFilesystemObservation), ProjectApplicationError>
    {
        let binding = project
            .bindings
            .iter()
            .find(|binding| binding.binding_id == project.project.primary_binding_id)
            .cloned()
            .ok_or(ProjectApplicationError::BindingNotFound)?;
        let observed_at = unix_time_ms();
        let observation = match self.locations.observe_binding(&binding, observed_at).await {
            Ok(observation) => observation,
            Err(ProjectLocationError::Missing) => {
                self.record_unavailable_binding(
                    project,
                    &binding,
                    WorkspaceAvailability::Missing,
                    "project.location.missing",
                    correlation_id,
                    observed_at,
                )
                .await?;
                return Err(ProjectApplicationError::ProjectMissing);
            }
            Err(ProjectLocationError::IdentityChanged | ProjectLocationError::UnsafeLink) => {
                self.record_unavailable_binding(
                    project,
                    &binding,
                    WorkspaceAvailability::Detached,
                    "project.location.identity_changed",
                    correlation_id,
                    observed_at,
                )
                .await?;
                return Err(ProjectApplicationError::ProjectDetached);
            }
            Err(ProjectLocationError::Inaccessible) => {
                self.record_unavailable_binding(
                    project,
                    &binding,
                    WorkspaceAvailability::Inaccessible,
                    "project.location.inaccessible",
                    correlation_id,
                    observed_at,
                )
                .await?;
                return Err(ProjectApplicationError::ProjectInaccessible);
            }
            Err(error) => return Err(error.into()),
        };

        let portable_identity_conflict = observation
            .portable_project_id
            .is_some_and(|id| id != project.project.project_id)
            || (binding.portable_metadata_state == PortableProjectMetadataState::PresentValid
                && (observation.portable_metadata_state
                    != PortableProjectMetadataState::PresentValid
                    || observation.portable_project_id != Some(project.project.project_id)));
        if portable_identity_conflict {
            self.record_unavailable_binding(
                project,
                &binding,
                WorkspaceAvailability::Detached,
                "project.metadata.identity_changed",
                correlation_id,
                observed_at,
            )
            .await?;
            return Err(ProjectApplicationError::ProjectDetached);
        }

        let binding_changed = binding.availability != observation.availability
            || binding.access_mode != observation.access_mode
            || binding.source_identity != observation.source_identity
            || binding.portable_metadata_state != observation.portable_metadata_state
            || binding.portable_project_id != observation.portable_project_id;
        let binding = if binding_changed {
            self.registry
                .compare_and_set_binding_observation(BindingObservationUpdate {
                    binding_id: binding.binding_id,
                    expected_revision: binding.record_revision,
                    availability: observation.availability,
                    access_mode: observation.access_mode,
                    observed_source_identity: observation.source_identity,
                    portable_metadata_state: observation.portable_metadata_state,
                    portable_project_id: observation.portable_project_id,
                    command_id: None,
                    correlation_id: correlation_id.clone(),
                    safe_code: "project.location.verified".to_owned(),
                    verified_at_unix_ms: observation.observed_at_unix_ms,
                })
                .await?
        } else {
            binding
        };

        let mut project_changed = binding_changed;
        if let Some(sha256) = observation.instruction_fingerprint {
            let current = project
                .instruction_fingerprints
                .iter()
                .find(|fingerprint| fingerprint.binding_id == binding.binding_id);
            if current.is_none_or(|current| {
                current.sha256 != sha256
                    || current.source_count != observation.instruction_source_count
            }) {
                self.registry
                    .compare_and_set_instruction_fingerprint(InstructionFingerprintUpdate {
                        project_id: project.project.project_id,
                        binding_id: binding.binding_id,
                        expected_revision: current.map_or(0, |current| current.revision),
                        sha256,
                        source_count: observation.instruction_source_count,
                        command_id: None,
                        correlation_id,
                        safe_code: "project.instructions.changed".to_owned(),
                        observed_at_unix_ms: observation.observed_at_unix_ms,
                    })
                    .await?;
                project_changed = true;
            }
        }
        if project_changed {
            let refreshed = self.get_project(project.project.project_id).await?;
            self.publish_project(&refreshed).await;
        }
        Ok((binding, observation))
    }

    async fn record_unavailable_binding(
        &self,
        project: &ProjectAggregate,
        binding: &WorkspaceBinding,
        availability: WorkspaceAvailability,
        safe_code: &str,
        correlation_id: String,
        observed_at_unix_ms: u64,
    ) -> Result<(), ProjectApplicationError> {
        if binding.availability == availability {
            self.publish_project(project).await;
            return Ok(());
        }
        self.registry
            .compare_and_set_binding_observation(BindingObservationUpdate {
                binding_id: binding.binding_id,
                expected_revision: binding.record_revision,
                availability,
                access_mode: binding.access_mode,
                observed_source_identity: binding.source_identity,
                portable_metadata_state: binding.portable_metadata_state,
                portable_project_id: binding.portable_project_id,
                command_id: None,
                correlation_id,
                safe_code: safe_code.to_owned(),
                verified_at_unix_ms: observed_at_unix_ms,
            })
            .await?;
        let refreshed = self.get_project(project.project.project_id).await?;
        self.publish_project(&refreshed).await;
        Ok(())
    }

    async fn finish_registration(
        &self,
        mut operation: ProjectRegistrationOperation,
    ) -> Result<RegisteredProject, ProjectApplicationError> {
        if operation.state == RegistrationOperationState::RecoveryRequired {
            operation = self
                .registry
                .update_registration_state(RegistrationStateUpdate {
                    command_id: operation.plan.command_id,
                    expected_state: RegistrationOperationState::RecoveryRequired,
                    target_state: RegistrationOperationState::Prepared,
                    safe_code: "project.registration.retrying".to_owned(),
                    updated_at_unix_ms: unix_time_ms(),
                })
                .await?;
        }
        if operation.state == RegistrationOperationState::Prepared {
            let observation = match self
                .locations
                .apply_registration_effect(
                    &self
                        .registry
                        // Once an operation is durably prepared, its original
                        // observation remains recovery evidence even after the
                        // UI preview TTL. The filesystem adapter still checks
                        // the live object identity before every effect.
                        .load_inspection(
                            operation.plan.inspection_id,
                            operation.plan.prepared_at_unix_ms,
                        )
                        .await?,
                    operation.plan.portable_metadata_action,
                    operation.plan.project_id,
                )
                .await
            {
                Ok(observation) => observation,
                Err(error) => {
                    let _ = self
                        .registry
                        .update_registration_state(RegistrationStateUpdate {
                            command_id: operation.plan.command_id,
                            expected_state: RegistrationOperationState::Prepared,
                            target_state: RegistrationOperationState::RecoveryRequired,
                            safe_code: error.safe_code().to_owned(),
                            updated_at_unix_ms: unix_time_ms(),
                        })
                        .await;
                    return Err(error.into());
                }
            };
            operation = self
                .registry
                .record_registration_filesystem_applied(RegistrationFilesystemApplied {
                    command_id: operation.plan.command_id,
                    expected_action: operation.plan.portable_metadata_action,
                    observation,
                    safe_code: "project.registration.filesystem_applied".to_owned(),
                })
                .await?;
        }
        if operation.state == RegistrationOperationState::Committed {
            let session = self
                .sessions
                .restore(operation.plan.direct_session_id)
                .await?;
            let project = self.get_project(operation.plan.project_id).await?;
            self.publish_registration(&project, &session).await;
            return Ok(RegisteredProject {
                project,
                direct_session: session,
                operation,
            });
        }
        if operation.state != RegistrationOperationState::FilesystemApplied {
            return Err(ProjectApplicationError::RecoveryRequired);
        }
        let session = if operation.plan.target == ProjectRegistrationTarget::ExistingProject {
            self.sessions
                .restore(operation.plan.direct_session_id)
                .await?
        } else {
            self.sessions
                .create_session_with_id(
                    operation.plan.command_id,
                    operation.plan.direct_session_id,
                    Some(operation.plan.project_id),
                    "Untitled chat".to_owned(),
                    unix_time_ms(),
                )
                .await?
                .snapshot
        };
        let ProjectRegistrationCommit { operation, project } = self
            .registry
            .commit_registration(operation.plan.command_id, unix_time_ms())
            .await?;
        self.publish_registration(&project, &session).await;
        Ok(RegisteredProject {
            project,
            direct_session: session,
            operation,
        })
    }

    async fn publish_registration(
        &self,
        project: &ProjectAggregate,
        session: &ProjectSessionSnapshot,
    ) {
        self.system
            .apply(vec![
                SystemMutation::UpsertProject(project_summary(project)),
                SystemMutation::UpsertSession(session_summary(session)),
                SystemMutation::Select {
                    project_id: Some(project.project.project_id.0.to_string()),
                    session_id: Some(session.session.session_id.0.to_string()),
                },
            ])
            .await;
    }

    async fn publish_project(&self, project: &ProjectAggregate) {
        self.system
            .apply(vec![SystemMutation::UpsertProject(project_summary(
                project,
            ))])
            .await;
    }
}

fn validate_registration_action(
    inspection: &ProjectLocationInspection,
    action: PortableMetadataAction,
) -> Result<(), ProjectApplicationError> {
    if inspection.instruction_discovery_incomplete {
        return Err(ProjectLocationError::InspectionIncomplete.into());
    }
    let valid = match action {
        PortableMetadataAction::LeaveAbsent => matches!(
            inspection.portable_metadata_state,
            PortableProjectMetadataState::Absent
                | PortableProjectMetadataState::Invalid
                | PortableProjectMetadataState::UnsupportedVersion
        ),
        PortableMetadataAction::UseExisting | PortableMetadataAction::ForkWithNewIdentity => {
            inspection.portable_metadata_state == PortableProjectMetadataState::PresentValid
                && inspection.portable_project_id.is_some()
        }
        PortableMetadataAction::CreateMinimal => {
            inspection.portable_metadata_state == PortableProjectMetadataState::Absent
                && inspection.minimal_structure_creation_available
        }
    };
    if valid {
        Ok(())
    } else {
        Err(ProjectApplicationError::InvalidRequest)
    }
}

fn require_trusted_policy(policy: &ProjectAccessPolicy) -> Result<(), ProjectApplicationError> {
    match policy.trust_state {
        ProjectTrustState::Restricted => Err(ProjectApplicationError::ProjectRestricted),
        ProjectTrustState::Revoked => Err(ProjectApplicationError::ProjectRevoked),
        ProjectTrustState::TrustedBounded => Ok(()),
    }
}

fn initial_trust_grant(
    command_id: CommandId,
    project_id: ProjectId,
    target: ProjectRegistrationTarget,
    requested: Option<ProjectTrustState>,
    decision: Option<TrustDecisionRef>,
) -> Result<
    Option<dennett_trust_core::project_registry::VerifiedProjectTrustGrant>,
    ProjectApplicationError,
> {
    if target == ProjectRegistrationTarget::ExistingProject {
        return if requested.is_none() && decision.is_none() {
            Ok(None)
        } else {
            Err(ProjectApplicationError::InvalidRequest)
        };
    }
    match requested.unwrap_or(ProjectTrustState::Restricted) {
        ProjectTrustState::Restricted if decision.is_none() => Ok(None),
        ProjectTrustState::TrustedBounded => {
            let decision = decision.ok_or(ProjectApplicationError::TrustDecisionMissing)?;
            Ok(Some(verify_bridge_attested_project_trust_decision(
                decision,
                command_id,
                project_id,
                ProjectTrustState::TrustedBounded,
            )?))
        }
        ProjectTrustState::Restricted | ProjectTrustState::Revoked => {
            Err(ProjectApplicationError::InvalidRequest)
        }
    }
}

pub(crate) fn project_summary(project: &ProjectAggregate) -> ProjectSummary {
    let primary = project
        .bindings
        .iter()
        .find(|binding| binding.binding_id == project.project.primary_binding_id);
    let state = match primary {
        Some(binding) if binding.availability == WorkspaceAvailability::Missing => {
            ProjectState::Missing
        }
        Some(binding) if binding.availability == WorkspaceAvailability::Detached => {
            ProjectState::Detached
        }
        Some(binding)
            if binding.availability == WorkspaceAvailability::Inaccessible
                || binding.access_mode
                    == dennett_trust_core::project_registry::WorkspaceAccessMode::ReadOnly =>
        {
            ProjectState::ReadOnly
        }
        Some(_) => ProjectState::Ready,
        None => ProjectState::Detached,
    };
    ProjectSummary {
        project_id: project.project.project_id.0.to_string(),
        display_name: project.project.display_name.clone(),
        state,
        revision: project.project.revision,
    }
}

fn unix_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn deterministic_id(label: &str, project_id: ProjectId) -> CommandId {
    let digest = Sha256::digest(format!("dennett:{label}:{}", project_id.0).as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    CommandId(uuid::Uuid::from_bytes(bytes))
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectApplicationError {
    #[error("project request is invalid")]
    InvalidRequest,
    #[error("project was not found")]
    ProjectNotFound,
    #[error("workspace binding was not found")]
    BindingNotFound,
    #[error("project trust decision is required")]
    TrustDecisionMissing,
    #[error("project registration requires recovery")]
    RecoveryRequired,
    #[error("project is restricted")]
    ProjectRestricted,
    #[error("project trust was revoked")]
    ProjectRevoked,
    #[error("project workspace is missing")]
    ProjectMissing,
    #[error("project workspace is detached")]
    ProjectDetached,
    #[error("project workspace is inaccessible")]
    ProjectInaccessible,
    #[error("project changed concurrently; retry from a fresh snapshot")]
    ConcurrentChange,
    #[error(transparent)]
    Location(#[from] ProjectLocationError),
    #[error(transparent)]
    Registry(#[from] ProjectRegistryError),
    #[error(transparent)]
    TrustDecision(#[from] dennett_trust_core::project_registry::ProjectTrustDecisionError),
    #[error(transparent)]
    Session(#[from] SessionJournalError),
}
