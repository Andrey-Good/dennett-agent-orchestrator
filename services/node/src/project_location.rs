use crate::project_fs::{
    CreateEmptyPrecondition, GitLayout, InspectionCompleteness, PortableProjectMetadata,
    ProjectFolderAdapter, ProjectFolderError, ProjectFolderInspection, ProjectFolderIntent,
    SharedMemoryPack, SourceIdentity,
};
use async_trait::async_trait;
use dennett_contracts::{
    PortableMetadataAction, PortableProjectMetadataState, ProjectId, ProjectInspectionId,
    RebindPortableMetadataAction, WorkspaceBindingId,
};
use dennett_head::project::{
    InspectProjectLocationCommand, ProjectLocationError, ProjectLocationPort,
};
use dennett_trust_core::project_registry::{
    CanonicalLocationKey, CanonicalWorkspaceLocation, LegacyProjectImport,
    ProjectLocationInspection, ProjectRegistrationKind, RegistrationFilesystemObservation,
    SensitiveAbsolutePath, SharedProjectMemoryState, WorkspaceAccessMode, WorkspaceAvailability,
    WorkspaceBinding, WorkspaceKind, WorkspaceSourceIdentity,
};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Default)]
pub(crate) struct NodeProjectLocationAdapter {
    folders: ProjectFolderAdapter,
}

impl NodeProjectLocationAdapter {
    pub(crate) async fn inspect_legacy_import(
        &self,
        project_id: ProjectId,
        binding_id: WorkspaceBindingId,
        display_name: String,
        root: PathBuf,
        imported_at_unix_ms: u64,
    ) -> Result<LegacyProjectImport, ProjectLocationError> {
        if !root.is_absolute() {
            return Err(ProjectLocationError::InvalidRequest);
        }
        let inspection = match self
            .inspect(InspectProjectLocationCommand {
                registration_kind: ProjectRegistrationKind::AttachExisting,
                root_uri: root.to_string_lossy().into_owned(),
                observed_at_unix_ms: imported_at_unix_ms,
                expires_at_unix_ms: imported_at_unix_ms.saturating_add(60_000),
            })
            .await
        {
            Ok(inspection) => inspection,
            Err(ProjectLocationError::Missing) => {
                return Ok(LegacyProjectImport {
                    project_id,
                    binding_id,
                    display_name,
                    location: canonical_location(&root.to_string_lossy())?,
                    source_identity: None,
                    workspace_kind: WorkspaceKind::Folder,
                    availability: WorkspaceAvailability::Missing,
                    access_mode: WorkspaceAccessMode::ReadWrite,
                    portable_metadata_state: PortableProjectMetadataState::Absent,
                    portable_project_id: None,
                    instruction_fingerprint: Some(Sha256::digest([]).into()),
                    instruction_source_count: 0,
                    claim_location: true,
                    correlation_id: "m02.legacy_project_import".to_owned(),
                    imported_at_unix_ms,
                });
            }
            Err(error) => return Err(error),
        };
        let portable_identity_conflict = inspection.portable_metadata_state
            == PortableProjectMetadataState::PresentValid
            && inspection.portable_project_id != Some(project_id);
        Ok(LegacyProjectImport {
            project_id,
            binding_id,
            display_name,
            location: inspection.location,
            source_identity: if portable_identity_conflict {
                None
            } else {
                inspection.source_identity
            },
            workspace_kind: inspection.workspace_kind,
            availability: if portable_identity_conflict {
                WorkspaceAvailability::Detached
            } else {
                inspection.availability
            },
            access_mode: inspection.access_mode,
            portable_metadata_state: if portable_identity_conflict {
                PortableProjectMetadataState::IdentityConflict
            } else {
                inspection.portable_metadata_state
            },
            portable_project_id: if portable_identity_conflict {
                None
            } else {
                inspection.portable_project_id
            },
            instruction_fingerprint: if portable_identity_conflict {
                Some(Sha256::digest([]).into())
            } else {
                inspection.instruction_fingerprint
            },
            instruction_source_count: if portable_identity_conflict {
                0
            } else {
                inspection.instruction_source_count
            },
            claim_location: !portable_identity_conflict,
            correlation_id: "m02.legacy_project_import".to_owned(),
            imported_at_unix_ms,
        })
    }
}

#[async_trait]
impl ProjectLocationPort for NodeProjectLocationAdapter {
    async fn inspect(
        &self,
        command: InspectProjectLocationCommand,
    ) -> Result<ProjectLocationInspection, ProjectLocationError> {
        if command.root_uri.trim().is_empty()
            || command.expires_at_unix_ms <= command.observed_at_unix_ms
        {
            return Err(ProjectLocationError::InvalidRequest);
        }
        let folders = self.folders.clone();
        let path = PathBuf::from(command.root_uri);
        let intent = folder_intent(command.registration_kind);
        let folder = tokio::task::spawn_blocking(move || folders.inspect(&path, intent))
            .await
            .map_err(|_| ProjectLocationError::AdapterUnavailable)?
            .map_err(folder_error)?;
        folder_to_inspection(
            folder,
            command.registration_kind,
            command.observed_at_unix_ms,
            command.expires_at_unix_ms,
        )
    }

    async fn apply_registration_effect(
        &self,
        inspection: &ProjectLocationInspection,
        action: PortableMetadataAction,
        project_id: ProjectId,
    ) -> Result<RegistrationFilesystemObservation, ProjectLocationError> {
        let folders = self.folders.clone();
        let inspection = inspection.clone();
        tokio::task::spawn_blocking(move || {
            apply_registration_blocking(&folders, &inspection, action, project_id)
        })
        .await
        .map_err(|_| ProjectLocationError::AdapterUnavailable)?
    }

    async fn apply_rebind_effect(
        &self,
        inspection: &ProjectLocationInspection,
        action: RebindPortableMetadataAction,
        project_id: ProjectId,
    ) -> Result<RegistrationFilesystemObservation, ProjectLocationError> {
        let folders = self.folders.clone();
        let inspection = inspection.clone();
        tokio::task::spawn_blocking(move || {
            let action = match action {
                RebindPortableMetadataAction::LeaveAbsent => PortableMetadataAction::LeaveAbsent,
                RebindPortableMetadataAction::UseExisting => PortableMetadataAction::UseExisting,
                RebindPortableMetadataAction::CreateMinimal => {
                    PortableMetadataAction::CreateMinimal
                }
            };
            apply_existing_root_action(&folders, &inspection, action, project_id)
        })
        .await
        .map_err(|_| ProjectLocationError::AdapterUnavailable)?
    }

    async fn observe_binding(
        &self,
        binding: &WorkspaceBinding,
        observed_at_unix_ms: u64,
    ) -> Result<RegistrationFilesystemObservation, ProjectLocationError> {
        let folders = self.folders.clone();
        let binding = binding.clone();
        tokio::task::spawn_blocking(move || {
            let folder = folders
                .inspect(
                    Path::new(binding.location.path.expose_local()),
                    ProjectFolderIntent::AttachExisting,
                )
                .map_err(folder_error)?;
            let observed = folder_to_observation(folder, observed_at_unix_ms)?;
            if observed.location != binding.location
                || binding.source_identity.is_some()
                    && observed.source_identity != binding.source_identity
            {
                return Err(ProjectLocationError::IdentityChanged);
            }
            Ok(observed)
        })
        .await
        .map_err(|_| ProjectLocationError::AdapterUnavailable)?
    }
}

fn apply_registration_blocking(
    folders: &ProjectFolderAdapter,
    inspection: &ProjectLocationInspection,
    action: PortableMetadataAction,
    project_id: ProjectId,
) -> Result<RegistrationFilesystemObservation, ProjectLocationError> {
    if inspection.registration_kind == ProjectRegistrationKind::CreateEmpty {
        let precondition = if inspection.location_exists {
            CreateEmptyPrecondition::ExistingEmpty {
                source_identity: decode_source_identity(
                    inspection
                        .source_identity
                        .ok_or(ProjectLocationError::IdentityChanged)?,
                )?,
            }
        } else {
            CreateEmptyPrecondition::Absent {
                parent_source_identity: decode_source_identity(
                    inspection
                        .prospective_parent_identity
                        .ok_or(ProjectLocationError::IdentityChanged)?,
                )?,
            }
        };
        folders
            .create_empty_root(
                Path::new(inspection.location.path.expose_local()),
                precondition,
            )
            .map_err(folder_error)?;
    }
    apply_existing_root_action(folders, inspection, action, project_id)
}

fn apply_existing_root_action(
    folders: &ProjectFolderAdapter,
    inspection: &ProjectLocationInspection,
    action: PortableMetadataAction,
    project_id: ProjectId,
) -> Result<RegistrationFilesystemObservation, ProjectLocationError> {
    let root = Path::new(inspection.location.path.expose_local());
    let before = folders
        .inspect(root, ProjectFolderIntent::AttachExisting)
        .map_err(folder_error)?;
    if inspection.location_exists {
        let expected = inspection
            .source_identity
            .ok_or(ProjectLocationError::IdentityChanged)?;
        if before.source_identity.map(encode_source_identity) != Some(expected) {
            return Err(ProjectLocationError::IdentityChanged);
        }
    }
    let source = before
        .source_identity
        .ok_or(ProjectLocationError::IdentityChanged)?;
    match action {
        PortableMetadataAction::LeaveAbsent => {}
        PortableMetadataAction::UseExisting => match before.portable_metadata {
            PortableProjectMetadata::Valid {
                project_id: observed,
            } if observed == project_id.0 => {}
            _ => return Err(ProjectLocationError::PortableMetadataConflict),
        },
        PortableMetadataAction::CreateMinimal => {
            folders
                .create_minimal(root, source, project_id.0)
                .map_err(folder_error)?;
        }
        PortableMetadataAction::ForkWithNewIdentity => {
            let PortableProjectMetadata::Valid {
                project_id: previous,
            } = before.portable_metadata
            else {
                return Err(ProjectLocationError::PortableMetadataConflict);
            };
            folders
                .rewrite_project_identity(root, source, previous, project_id.0)
                .map_err(folder_error)?;
        }
    }
    let after = folders
        .inspect(root, ProjectFolderIntent::AttachExisting)
        .map_err(folder_error)?;
    folder_to_observation(after, unix_time_ms())
}

fn folder_to_inspection(
    folder: ProjectFolderInspection,
    registration_kind: ProjectRegistrationKind,
    observed_at_unix_ms: u64,
    expires_at_unix_ms: u64,
) -> Result<ProjectLocationInspection, ProjectLocationError> {
    ensure_complete(&folder)?;
    let portable_metadata_state = portable_state(folder.portable_metadata);
    let portable_project_id = portable_project_id(folder.portable_metadata);
    let location = canonical_location(&folder.canonical_location)?;
    let suggested_display_name = Path::new(&folder.canonical_location)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("Dennett project")
        .to_owned();
    Ok(ProjectLocationInspection {
        inspection_id: ProjectInspectionId::new(),
        registration_kind,
        location,
        suggested_display_name,
        location_exists: folder.location_exists,
        location_empty: folder.location_empty,
        source_identity: folder.source_identity.map(encode_source_identity),
        prospective_parent_identity: folder
            .prospective_parent_identity
            .map(encode_source_identity),
        workspace_kind: workspace_kind(folder.git)?,
        availability: if folder.location_exists {
            WorkspaceAvailability::Available
        } else {
            WorkspaceAvailability::Missing
        },
        access_mode: access_mode(&folder.canonical_location),
        portable_metadata_state,
        portable_project_id,
        shared_memory_state: match folder.shared_memory {
            SharedMemoryPack::Absent => SharedProjectMemoryState::Absent,
            SharedMemoryPack::ManifestPresent => SharedProjectMemoryState::Present,
        },
        minimal_structure_creation_available: portable_metadata_state
            == PortableProjectMetadataState::Absent,
        // The empty instruction set has a real fingerprint too. Persisting
        // SHA-256(empty) lets removal of the last instruction file invalidate
        // a previously non-empty fingerprint instead of looking like
        // "nothing was observed".
        instruction_fingerprint: Some(parse_instruction_fingerprint(
            folder.instructions.aggregate_sha256.as_deref(),
        )?),
        instruction_source_count: u32::try_from(folder.instructions.sources.len())
            .map_err(|_| ProjectLocationError::InspectionIncomplete)?,
        instruction_discovery_incomplete: false,
        observed_at_unix_ms,
        expires_at_unix_ms,
    })
}

fn folder_to_observation(
    folder: ProjectFolderInspection,
    observed_at_unix_ms: u64,
) -> Result<RegistrationFilesystemObservation, ProjectLocationError> {
    ensure_complete(&folder)?;
    Ok(RegistrationFilesystemObservation {
        location: canonical_location(&folder.canonical_location)?,
        source_identity: folder.source_identity.map(encode_source_identity),
        workspace_kind: workspace_kind(folder.git)?,
        availability: WorkspaceAvailability::Available,
        access_mode: access_mode(&folder.canonical_location),
        portable_metadata_state: portable_state(folder.portable_metadata),
        portable_project_id: portable_project_id(folder.portable_metadata),
        instruction_fingerprint: Some(parse_instruction_fingerprint(
            folder.instructions.aggregate_sha256.as_deref(),
        )?),
        instruction_source_count: u32::try_from(folder.instructions.sources.len())
            .map_err(|_| ProjectLocationError::InspectionIncomplete)?,
        observed_at_unix_ms,
    })
}

fn ensure_complete(folder: &ProjectFolderInspection) -> Result<(), ProjectLocationError> {
    if matches!(
        folder.instructions.completeness,
        InspectionCompleteness::Incomplete(_)
    ) {
        Err(ProjectLocationError::InspectionIncomplete)
    } else {
        Ok(())
    }
}

fn folder_intent(kind: ProjectRegistrationKind) -> ProjectFolderIntent {
    match kind {
        ProjectRegistrationKind::CreateEmpty => ProjectFolderIntent::CreateEmpty,
        ProjectRegistrationKind::AttachExisting => ProjectFolderIntent::AttachExisting,
    }
}

fn workspace_kind(git: GitLayout) -> Result<WorkspaceKind, ProjectLocationError> {
    match git {
        GitLayout::None => Ok(WorkspaceKind::Folder),
        GitLayout::Directory | GitLayout::WorktreeFile => Ok(WorkspaceKind::VersionedCheckout),
        GitLayout::Invalid => Err(ProjectLocationError::InvalidRequest),
    }
}

fn portable_state(metadata: PortableProjectMetadata) -> PortableProjectMetadataState {
    match metadata {
        PortableProjectMetadata::Absent => PortableProjectMetadataState::Absent,
        PortableProjectMetadata::Valid { .. } => PortableProjectMetadataState::PresentValid,
        PortableProjectMetadata::Invalid => PortableProjectMetadataState::Invalid,
        PortableProjectMetadata::Unsupported { .. } => {
            PortableProjectMetadataState::UnsupportedVersion
        }
    }
}

fn portable_project_id(metadata: PortableProjectMetadata) -> Option<ProjectId> {
    match metadata {
        PortableProjectMetadata::Valid { project_id } => Some(ProjectId(project_id)),
        _ => None,
    }
}

fn canonical_location(value: &str) -> Result<CanonicalWorkspaceLocation, ProjectLocationError> {
    let mut normalized = value.replace('\\', "/");
    if cfg!(windows) {
        normalized.make_ascii_lowercase();
    }
    let key: [u8; 32] = Sha256::digest(normalized.as_bytes()).into();
    Ok(CanonicalWorkspaceLocation {
        path: SensitiveAbsolutePath::new(value.to_owned())
            .map_err(|_| ProjectLocationError::InvalidRequest)?,
        key: CanonicalLocationKey::new(key),
    })
}

pub(crate) fn encode_source_identity(source: SourceIdentity) -> WorkspaceSourceIdentity {
    let mut bytes = [0_u8; 32];
    bytes[..8].copy_from_slice(&source.volume.to_le_bytes());
    bytes[8..16].copy_from_slice(&source.file.to_le_bytes());
    let checksum = Sha256::digest(&bytes[..16]);
    bytes[16..].copy_from_slice(&checksum[..16]);
    WorkspaceSourceIdentity::new(bytes)
}

pub(crate) fn decode_source_identity(
    source: WorkspaceSourceIdentity,
) -> Result<SourceIdentity, ProjectLocationError> {
    let bytes = source.as_bytes();
    let checksum = Sha256::digest(&bytes[..16]);
    if bytes[16..] != checksum[..16] {
        return Err(ProjectLocationError::IdentityChanged);
    }
    Ok(SourceIdentity {
        volume: u64::from_le_bytes(bytes[..8].try_into().expect("fixed source volume")),
        file: u64::from_le_bytes(bytes[8..16].try_into().expect("fixed source file")),
    })
}

fn parse_instruction_fingerprint(value: Option<&str>) -> Result<[u8; 32], ProjectLocationError> {
    value.map_or_else(|| Ok(Sha256::digest([]).into()), parse_sha256)
}

fn parse_sha256(value: &str) -> Result<[u8; 32], ProjectLocationError> {
    if value.len() != 64 {
        return Err(ProjectLocationError::InspectionIncomplete);
    }
    let mut bytes = [0_u8; 32];
    for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
        let text =
            std::str::from_utf8(chunk).map_err(|_| ProjectLocationError::InspectionIncomplete)?;
        bytes[index] =
            u8::from_str_radix(text, 16).map_err(|_| ProjectLocationError::InspectionIncomplete)?;
    }
    Ok(bytes)
}

fn access_mode(path: &str) -> WorkspaceAccessMode {
    if std::fs::metadata(path).is_ok_and(|metadata| metadata.permissions().readonly()) {
        WorkspaceAccessMode::ReadOnly
    } else {
        WorkspaceAccessMode::ReadWrite
    }
}

fn folder_error(error: ProjectFolderError) -> ProjectLocationError {
    match error {
        ProjectFolderError::LocationMissing => ProjectLocationError::Missing,
        ProjectFolderError::LinkedEntry => ProjectLocationError::UnsafeLink,
        ProjectFolderError::SourceIdentityChanged => ProjectLocationError::IdentityChanged,
        ProjectFolderError::PortableMetadataConflict => {
            ProjectLocationError::PortableMetadataConflict
        }
        ProjectFolderError::PostconditionFailed => ProjectLocationError::RecoveryRequired,
        ProjectFolderError::InvalidRoot
        | ProjectFolderError::RootNotDirectory
        | ProjectFolderError::CreateRootNotEmpty
        | ProjectFolderError::InvalidProjectId
        | ProjectFolderError::IdentityUnchanged => ProjectLocationError::InvalidRequest,
        ProjectFolderError::Io { .. } => ProjectLocationError::AdapterUnavailable,
    }
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    use dennett_contracts::{CommandId, ProjectTrustState, RebindPortableMetadataAction};
    use dennett_head::project::{
        ProjectApplication, ProjectApplicationError, RebindProjectCommand, RegisterProjectCommand,
        SetProjectTrustCommand,
    };
    use dennett_head::session::SessionCoordinator;
    use dennett_head::system::{ProjectState, SystemProjection, SystemSnapshot, SystemStatePort};
    use dennett_memory_core::session::SessionJournal;
    use dennett_storage_sqlite::SqliteControlStore;
    use dennett_trust_core::project_registry::{
        BRIDGE_ATTESTED_PROJECT_TRUST_DECISION_KIND, ProjectLifecycleEventKind,
        ProjectLocationInspection, ProjectRegistryPort, RegistrationOperationState,
        TrustDecisionRef,
    };
    use serde_json::Value;
    use std::fs;
    use std::sync::Arc;
    use std::time::Duration;
    use tempfile::TempDir;

    const AUTHORITY_EPOCH: u64 = 17;
    const OBSERVED_AT_UNIX_MS: u64 = 1_000;

    struct RealProjectApplication {
        store: Arc<SqliteControlStore>,
        sessions: SessionCoordinator,
        system: Arc<SystemProjection>,
        projects: ProjectApplication,
    }

    impl RealProjectApplication {
        async fn open(database: &Path) -> Self {
            let store = Arc::new(
                SqliteControlStore::open(database)
                    .await
                    .expect("open real SQLite control store"),
            );
            let sessions =
                SessionCoordinator::new(SessionJournal::new(store.clone()), AUTHORITY_EPOCH, 16);
            let system = Arc::new(SystemProjection::new(
                SystemSnapshot::empty(AUTHORITY_EPOCH),
                16,
            ));
            let projects = ProjectApplication::new(
                store.clone(),
                Arc::new(NodeProjectLocationAdapter::default()),
                sessions.clone(),
                system.clone(),
            );
            Self {
                store,
                sessions,
                system,
                projects,
            }
        }

        async fn inspect_existing(&self, root: &Path) -> ProjectLocationInspection {
            self.projects
                .inspect_location(InspectProjectLocationCommand {
                    registration_kind: ProjectRegistrationKind::AttachExisting,
                    root_uri: root.to_string_lossy().into_owned(),
                    observed_at_unix_ms: OBSERVED_AT_UNIX_MS,
                    expires_at_unix_ms: OBSERVED_AT_UNIX_MS + 60_000,
                })
                .await
                .expect("inspect existing project folder")
        }
    }

    fn registration_command(
        inspection: &ProjectLocationInspection,
        command_id: CommandId,
        action: PortableMetadataAction,
        trusted: bool,
    ) -> RegisterProjectCommand {
        RegisterProjectCommand {
            command_id,
            correlation_id: "test.project_registration".to_owned(),
            intent_sha256: [23; 32],
            inspection_id: inspection.inspection_id,
            display_name: inspection.suggested_display_name.clone(),
            portable_metadata_action: action,
            initial_trust_state: trusted.then_some(ProjectTrustState::TrustedBounded),
            trust_decision: trusted.then(|| bridge_trust_decision(command_id)),
            committed_at_unix_ms: inspection.observed_at_unix_ms + 1,
        }
    }

    fn bridge_trust_decision(command_id: CommandId) -> TrustDecisionRef {
        TrustDecisionRef::new(
            BRIDGE_ATTESTED_PROJECT_TRUST_DECISION_KIND,
            command_id.0.to_string(),
        )
        .expect("bridge-attested trust decision")
    }

    fn child_names(path: &Path) -> Vec<String> {
        let mut names = fs::read_dir(path)
            .expect("read directory")
            .map(|entry| {
                entry
                    .expect("read directory entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    async fn register_existing(
        application: &RealProjectApplication,
        root: &Path,
        action: PortableMetadataAction,
        trusted: bool,
    ) -> (
        ProjectLocationInspection,
        RegisterProjectCommand,
        dennett_head::project::RegisteredProject,
    ) {
        let inspection = application.inspect_existing(root).await;
        let command = registration_command(&inspection, CommandId::new(), action, trusted);
        let registered = application
            .projects
            .register_project(command.clone())
            .await
            .expect("register project");
        (inspection, command, registered)
    }

    #[test]
    fn source_identity_encoding_round_trips_and_detects_corruption() {
        let source = SourceIdentity {
            volume: 7,
            file: 11,
        };
        let encoded = encode_source_identity(source);
        assert_eq!(decode_source_identity(encoded), Ok(source));

        let mut corrupt = *encoded.as_bytes();
        corrupt[31] ^= 1;
        assert_eq!(
            decode_source_identity(WorkspaceSourceIdentity::new(corrupt)),
            Err(ProjectLocationError::IdentityChanged)
        );
    }

    #[test]
    fn canonical_alias_text_has_one_local_binding_key() {
        if cfg!(windows) {
            let direct = canonical_location("C:\\Work\\Project").unwrap();
            let slash = canonical_location("C:/Work/Project").unwrap();
            assert_eq!(direct.key, slash.key);
            let key_debug = format!("{:?}", direct.key);
            assert!(!key_debug.contains("C:/Work/Project"));
            assert!(!key_debug.contains("c:/work/project"));
        } else {
            let direct = canonical_location("/tmp/Project").unwrap();
            let slash = canonical_location("/tmp/Project").unwrap();
            assert_eq!(direct.key, slash.key);
            assert!(!format!("{:?}", direct.key).contains("/tmp/Project"));
        }
    }

    #[tokio::test]
    async fn attach_existing_without_portable_metadata_is_a_non_mutating_preview() {
        let temp = TempDir::new().expect("temporary test root");
        let project_root = temp.path().join("existing-project");
        fs::create_dir(&project_root).expect("create project root");
        fs::write(project_root.join("notes.txt"), b"owner data").expect("seed owner file");
        let before = child_names(&project_root);
        let application = RealProjectApplication::open(&temp.path().join("control.sqlite")).await;

        let inspection = application.inspect_existing(&project_root).await;

        assert_eq!(
            inspection.portable_metadata_state,
            PortableProjectMetadataState::Absent
        );
        assert!(inspection.minimal_structure_creation_available);
        assert!(inspection.location_exists);
        assert!(!inspection.location_empty);
        assert_eq!(child_names(&project_root), before);
        assert!(!project_root.join(".dennett").exists());
        assert_eq!(
            fs::read(project_root.join("notes.txt")).expect("read owner file"),
            b"owner data"
        );
        assert!(
            application
                .projects
                .list_projects()
                .await
                .unwrap()
                .is_empty()
        );
        assert!(application.sessions.restore_all().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn create_minimal_registers_exact_structure_once_and_replay_is_idempotent() {
        let temp = TempDir::new().expect("temporary test root");
        let project_root = temp.path().join("portable-project");
        fs::create_dir(&project_root).expect("create project root");
        fs::write(project_root.join("keep.txt"), b"keep").expect("seed owner file");
        let application = RealProjectApplication::open(&temp.path().join("control.sqlite")).await;
        let inspection = application.inspect_existing(&project_root).await;
        let command = registration_command(
            &inspection,
            CommandId::new(),
            PortableMetadataAction::CreateMinimal,
            false,
        );

        let first = application
            .projects
            .register_project(command.clone())
            .await
            .expect("first registration");
        let replay = application
            .projects
            .register_project(command)
            .await
            .expect("idempotent registration replay");

        assert_eq!(
            first.project.project.project_id,
            replay.project.project.project_id
        );
        assert_eq!(
            first.project.project.primary_binding_id,
            replay.project.project.primary_binding_id
        );
        assert_eq!(
            first.direct_session.session.session_id,
            replay.direct_session.session.session_id
        );
        assert_eq!(first.operation.state, RegistrationOperationState::Committed);
        assert_eq!(
            replay.operation.state,
            RegistrationOperationState::Committed
        );

        let dennett = project_root.join(".dennett");
        assert_eq!(
            child_names(&dennett),
            vec!["memory".to_owned(), "project.json".to_owned()]
        );
        assert_eq!(
            child_names(&dennett.join("memory")),
            vec!["README.md".to_owned()]
        );
        assert!(!dennett.join("memory/manifest.yaml").exists());
        assert_eq!(
            fs::read(project_root.join("keep.txt")).expect("read owner file"),
            b"keep"
        );

        let metadata: Value = serde_json::from_slice(
            &fs::read(dennett.join("project.json")).expect("read portable metadata"),
        )
        .expect("parse portable metadata");
        let metadata = metadata.as_object().expect("metadata object");
        assert_eq!(metadata.len(), 2);
        assert_eq!(metadata["format_version"], 1);
        assert_eq!(
            metadata["project_id"],
            first.project.project.project_id.0.to_string()
        );

        let projects = application.projects.list_projects().await.unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].bindings.len(), 1);
        let sessions = application.sessions.restore_all().await.unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(
            sessions[0].session.project_id,
            Some(first.project.project.project_id)
        );
        let system = application.system.bootstrap().await.unwrap();
        assert_eq!(system.projects.len(), 1);
        assert_eq!(system.recent_sessions.len(), 1);
    }

    #[tokio::test]
    async fn leave_absent_registers_local_only_without_creating_dennett_files() {
        let temp = TempDir::new().expect("temporary test root");
        let project_root = temp.path().join("local-only-project");
        fs::create_dir(&project_root).expect("create project root");
        fs::write(project_root.join("model.bin"), b"weights").expect("seed owner file");
        let before = child_names(&project_root);
        let application = RealProjectApplication::open(&temp.path().join("control.sqlite")).await;

        let (_, _, registered) = register_existing(
            &application,
            &project_root,
            PortableMetadataAction::LeaveAbsent,
            false,
        )
        .await;

        assert_eq!(child_names(&project_root), before);
        assert!(!project_root.join(".dennett").exists());
        assert_eq!(registered.project.bindings.len(), 1);
        assert_eq!(
            registered.project.bindings[0].portable_metadata_state,
            PortableProjectMetadataState::Absent
        );
        assert_eq!(
            registered.project.access_policy.trust_state,
            ProjectTrustState::Restricted
        );
        assert_eq!(application.projects.list_projects().await.unwrap().len(), 1);
        assert_eq!(application.sessions.restore_all().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn restricted_denies_workspace_then_bridge_trust_allows_and_rename_persists_missing() {
        let temp = TempDir::new().expect("temporary test root");
        let project_root = temp.path().join("trusted-project");
        let renamed_root = temp.path().join("trusted-project-moved");
        fs::create_dir(&project_root).expect("create project root");
        let database = temp.path().join("control.sqlite");
        let application = RealProjectApplication::open(&database).await;
        let (_, _, registered) = register_existing(
            &application,
            &project_root,
            PortableMetadataAction::CreateMinimal,
            false,
        )
        .await;
        let project_id = registered.project.project.project_id;

        assert!(matches!(
            application
                .projects
                .prepare_agent_workspace(project_id, "test.restricted".to_owned())
                .await,
            Err(ProjectApplicationError::ProjectRestricted)
        ));

        let trust_command_id = CommandId::new();
        let trusted = application
            .projects
            .set_project_trust(SetProjectTrustCommand {
                command_id: trust_command_id,
                correlation_id: "test.trust_project".to_owned(),
                project_id,
                target_state: ProjectTrustState::TrustedBounded,
                expected_policy_revision: registered.project.access_policy.revision,
                trust_decision: bridge_trust_decision(trust_command_id),
                committed_at_unix_ms: OBSERVED_AT_UNIX_MS + 2,
            })
            .await
            .expect("apply bridge-attested trust");
        assert_eq!(trusted.trust_state, ProjectTrustState::TrustedBounded);

        let workspace = application
            .projects
            .prepare_agent_workspace(project_id, "test.trusted".to_owned())
            .await
            .expect("prepare trusted workspace");
        assert_eq!(
            canonical_location(&workspace.absolute_path).unwrap().key,
            registered.project.bindings[0].location.key
        );

        fs::rename(&project_root, &renamed_root).expect("move project folder");
        assert!(matches!(
            application
                .projects
                .prepare_agent_workspace(project_id, "test.missing".to_owned())
                .await,
            Err(ProjectApplicationError::ProjectMissing)
        ));

        let missing = application.projects.get_project(project_id).await.unwrap();
        assert_eq!(
            missing.bindings[0].availability,
            WorkspaceAvailability::Missing
        );
        let system = application.system.bootstrap().await.unwrap();
        assert_eq!(system.projects.len(), 1);
        assert_eq!(system.projects[0].state, ProjectState::Missing);

        application.store.close().await;
        drop(application);
        let reopened = RealProjectApplication::open(&database).await;
        let durable = reopened.projects.get_project(project_id).await.unwrap();
        assert_eq!(
            durable.bindings[0].availability,
            WorkspaceAvailability::Missing
        );
        assert_eq!(
            durable.access_policy.trust_state,
            ProjectTrustState::TrustedBounded
        );
    }

    #[tokio::test]
    async fn workspace_authority_permit_orders_provider_admission_before_revocation() {
        let temp = TempDir::new().expect("temporary test root");
        let project_root = temp.path().join("authority-project");
        fs::create_dir(&project_root).expect("create project root");
        let application = RealProjectApplication::open(&temp.path().join("control.sqlite")).await;
        let (_, _, registered) = register_existing(
            &application,
            &project_root,
            PortableMetadataAction::CreateMinimal,
            true,
        )
        .await;
        let project_id = registered.project.project.project_id;

        let workspace = application
            .projects
            .prepare_agent_workspace(project_id, "test.authority.admit".to_owned())
            .await
            .expect("prepare trusted workspace");
        let revoke_command_id = CommandId::new();
        let projects = application.projects.clone();
        let revoke = tokio::spawn(async move {
            projects
                .set_project_trust(SetProjectTrustCommand {
                    command_id: revoke_command_id,
                    correlation_id: "test.authority.revoke".to_owned(),
                    project_id,
                    target_state: ProjectTrustState::Revoked,
                    expected_policy_revision: registered.project.access_policy.revision,
                    trust_decision: bridge_trust_decision(revoke_command_id),
                    committed_at_unix_ms: OBSERVED_AT_UNIX_MS + 2,
                })
                .await
        });

        tokio::time::sleep(Duration::from_millis(25)).await;
        assert!(
            !revoke.is_finished(),
            "revocation must not cross an admitted workspace effect"
        );
        drop(workspace);

        let revoked = tokio::time::timeout(Duration::from_secs(2), revoke)
            .await
            .expect("revocation completes after provider admission")
            .expect("revocation task joins")
            .expect("revocation succeeds");
        assert_eq!(revoked.trust_state, ProjectTrustState::Revoked);
        assert!(matches!(
            application
                .projects
                .prepare_agent_workspace(project_id, "test.authority.denied".to_owned())
                .await,
            Err(ProjectApplicationError::ProjectRevoked)
        ));
    }

    #[tokio::test]
    async fn changing_then_removing_last_agents_file_advances_instruction_revision() {
        let temp = TempDir::new().expect("temporary test root");
        let project_root = temp.path().join("instruction-project");
        fs::create_dir(&project_root).expect("create project root");
        let agents = project_root.join("AGENTS.md");
        fs::write(&agents, b"first instruction").expect("write initial instructions");
        let application = RealProjectApplication::open(&temp.path().join("control.sqlite")).await;
        let (_, _, registered) = register_existing(
            &application,
            &project_root,
            PortableMetadataAction::LeaveAbsent,
            true,
        )
        .await;
        let project_id = registered.project.project.project_id;
        let initial_project_revision = registered.project.project.revision;
        let initial = registered
            .project
            .instruction_fingerprints
            .first()
            .expect("initial instruction fingerprint")
            .clone();
        assert_eq!(initial.source_count, 1);
        assert_eq!(initial.revision, 1);

        fs::write(&agents, b"second instruction").expect("change instructions");
        application
            .projects
            .prepare_agent_workspace(project_id, "test.instructions.changed".to_owned())
            .await
            .expect("refresh changed instructions");
        let changed_project = application.projects.get_project(project_id).await.unwrap();
        let changed = changed_project
            .instruction_fingerprints
            .first()
            .expect("changed instruction fingerprint")
            .clone();
        assert_ne!(changed.sha256, initial.sha256);
        assert_eq!(changed.source_count, 1);
        assert_eq!(changed.revision, initial.revision + 1);
        assert!(changed_project.project.revision > initial_project_revision);

        fs::remove_file(&agents).expect("remove last instruction file");
        application
            .projects
            .prepare_agent_workspace(project_id, "test.instructions.removed".to_owned())
            .await
            .expect("refresh removed instructions");
        let removed_project = application.projects.get_project(project_id).await.unwrap();
        let removed = removed_project
            .instruction_fingerprints
            .first()
            .expect("empty instruction fingerprint");
        assert_ne!(removed.sha256, changed.sha256);
        assert_eq!(removed.sha256, <[u8; 32]>::from(Sha256::digest([])));
        assert_eq!(removed.source_count, 0);
        assert_eq!(removed.revision, changed.revision + 1);
        assert!(removed_project.project.revision > changed_project.project.revision);

        let instruction_events = application
            .store
            .list_lifecycle_events(project_id)
            .await
            .unwrap()
            .into_iter()
            .filter(|event| event.kind == ProjectLifecycleEventKind::InstructionFingerprintChanged)
            .count();
        assert_eq!(instruction_events, 3);
    }

    #[tokio::test]
    async fn reopening_sqlite_and_recreating_application_preserves_one_registration() {
        let temp = TempDir::new().expect("temporary test root");
        let project_root = temp.path().join("restart-project");
        fs::create_dir(&project_root).expect("create project root");
        let database = temp.path().join("control.sqlite");
        let first_application = RealProjectApplication::open(&database).await;
        let inspection = first_application.inspect_existing(&project_root).await;
        let command = registration_command(
            &inspection,
            CommandId::new(),
            PortableMetadataAction::CreateMinimal,
            false,
        );
        let first = first_application
            .projects
            .register_project(command.clone())
            .await
            .expect("initial registration");
        let expected_project_id = first.project.project.project_id;
        let expected_binding_id = first.project.project.primary_binding_id;
        let expected_session_id = first.direct_session.session.session_id;

        first_application.store.close().await;
        drop(first_application);

        let reopened = RealProjectApplication::open(&database).await;
        let restored_projects = reopened.projects.list_projects().await.unwrap();
        assert_eq!(restored_projects.len(), 1);
        assert_eq!(restored_projects[0].project.project_id, expected_project_id);
        assert_eq!(
            restored_projects[0].project.primary_binding_id,
            expected_binding_id
        );
        let restored_sessions = reopened.sessions.restore_all().await.unwrap();
        assert_eq!(restored_sessions.len(), 1);
        assert_eq!(restored_sessions[0].session.session_id, expected_session_id);

        let replay = reopened
            .projects
            .register_project(command)
            .await
            .expect("replay registration after restart");
        assert_eq!(replay.project.project.project_id, expected_project_id);
        assert_eq!(
            replay.project.project.primary_binding_id,
            expected_binding_id
        );
        assert_eq!(
            replay.direct_session.session.session_id,
            expected_session_id
        );
        assert_eq!(reopened.projects.list_projects().await.unwrap().len(), 1);
        assert_eq!(reopened.sessions.restore_all().await.unwrap().len(), 1);
        let system = reopened.system.bootstrap().await.unwrap();
        assert_eq!(system.projects.len(), 1);
        assert_eq!(system.recent_sessions.len(), 1);
    }

    #[tokio::test]
    async fn rebinding_the_same_portable_folder_updates_the_existing_binding() {
        let temp = TempDir::new().expect("temporary test root");
        let project_root = temp.path().join("same-folder-rebind");
        fs::create_dir(&project_root).expect("create project root");
        let application = RealProjectApplication::open(&temp.path().join("control.sqlite")).await;
        let (_, _, registered) = register_existing(
            &application,
            &project_root,
            PortableMetadataAction::CreateMinimal,
            false,
        )
        .await;
        let project_id = registered.project.project.project_id;
        let original_binding = registered.project.bindings[0].clone();
        let observed_at_unix_ms = unix_time_ms();
        let inspection = application
            .projects
            .inspect_location(InspectProjectLocationCommand {
                registration_kind: ProjectRegistrationKind::AttachExisting,
                root_uri: project_root.to_string_lossy().into_owned(),
                observed_at_unix_ms,
                expires_at_unix_ms: observed_at_unix_ms + 60_000,
            })
            .await
            .expect("inspect the same portable folder");

        let receipt = application
            .projects
            .rebind_project(RebindProjectCommand {
                command_id: CommandId::new(),
                correlation_id: "test.same_folder_rebind".to_owned(),
                intent_sha256: [41; 32],
                project_id,
                current_binding_id: original_binding.binding_id,
                inspection_id: inspection.inspection_id,
                portable_metadata_action: RebindPortableMetadataAction::UseExisting,
                committed_at_unix_ms: observed_at_unix_ms + 1,
            })
            .await
            .expect("rebind the same portable folder");

        assert_eq!(
            receipt.primary_binding.binding_id,
            original_binding.binding_id
        );
        assert!(receipt.primary_binding.record_revision > original_binding.record_revision);
        let project = application.projects.get_project(project_id).await.unwrap();
        assert_eq!(
            project.project.primary_binding_id,
            original_binding.binding_id
        );
        assert_eq!(project.bindings.len(), 1);
    }

    #[tokio::test]
    async fn local_only_project_can_add_minimal_portable_structure_explicitly() {
        let temp = TempDir::new().expect("temporary test root");
        let project_root = temp.path().join("add-portable-metadata");
        fs::create_dir(&project_root).expect("create project root");
        let application = RealProjectApplication::open(&temp.path().join("control.sqlite")).await;
        let (_, _, registered) = register_existing(
            &application,
            &project_root,
            PortableMetadataAction::LeaveAbsent,
            false,
        )
        .await;
        let project_id = registered.project.project.project_id;
        let original_binding = registered.project.bindings[0].clone();
        let observed_at_unix_ms = unix_time_ms();
        let inspection = application
            .projects
            .inspect_location(InspectProjectLocationCommand {
                registration_kind: ProjectRegistrationKind::AttachExisting,
                root_uri: project_root.to_string_lossy().into_owned(),
                observed_at_unix_ms,
                expires_at_unix_ms: observed_at_unix_ms + 60_000,
            })
            .await
            .expect("inspect local-only project");
        assert!(inspection.minimal_structure_creation_available);

        let receipt = application
            .projects
            .rebind_project(RebindProjectCommand {
                command_id: CommandId::new(),
                correlation_id: "test.add_portable_metadata".to_owned(),
                intent_sha256: [43; 32],
                project_id,
                current_binding_id: original_binding.binding_id,
                inspection_id: inspection.inspection_id,
                portable_metadata_action: RebindPortableMetadataAction::CreateMinimal,
                committed_at_unix_ms: observed_at_unix_ms + 1,
            })
            .await
            .expect("create minimal portable project structure");

        assert_eq!(
            receipt.primary_binding.binding_id,
            original_binding.binding_id
        );
        assert_eq!(
            receipt.primary_binding.portable_metadata_state,
            PortableProjectMetadataState::PresentValid
        );
        assert_eq!(
            receipt.primary_binding.portable_project_id,
            Some(project_id)
        );
        assert!(project_root.join(".dennett/project.json").is_file());
        assert!(project_root.join(".dennett/memory/README.md").is_file());
        assert_eq!(application.projects.list_projects().await.unwrap().len(), 1);
        assert_eq!(application.sessions.restore_all().await.unwrap().len(), 1);
    }
}
