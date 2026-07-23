//! Capability-scoped workspace snapshots and version-bound file publication.

use crate::{
    project_fs::{
        OpenProjectRoot, ProjectFolderError, directory_identity, optional_symlink_metadata,
        sync_directory,
    },
    project_location::decode_source_identity,
};
use async_trait::async_trait;
use cap_fs_ext::{DirExt, FollowSymlinks, MetadataExt, OpenOptionsFollowExt, OpenOptionsSyncExt};
#[cfg(unix)]
use cap_std::fs::PermissionsExt as _;
use cap_std::fs::{Dir, Metadata, OpenOptions};
use dennett_contracts::ProjectRelativePath;
use dennett_effect_core::workspace::{
    ContentSha256, FileMutationKind, MAX_STAGED_FILE_BYTES, MAX_STAGED_OPERATION_BYTES,
    MetadataSha256, PortableFilePermissions, ResolvedFileChangeProposal, WorkspaceBlob,
    WorkspaceCheckpointEntry, WorkspaceFileEffectPlan, WorkspaceManifestEntry,
    WorkspaceObjectIdentity, WorkspacePathState, WorkspaceStagedObjectKind,
    WorkspaceStagedObjectReceipt, WorkspaceStagingReceipt,
};
use dennett_head::workspace::{
    CapturedWorkspaceCheckpoint, PreparedWorkspaceFileEffect, WorkspaceFileChangeInput,
    WorkspaceFilesystemError, WorkspaceFilesystemPort, WorkspaceFilesystemScope,
    WorkspaceObservation, WorkspaceTransitionObservation,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    io::{self, Read, Write},
    path::Path,
};

const SNAPSHOT_SCOPE_POLICY: &[u8] = b"dennett.workspace-scope.v1;exclude=.git;links=leaf;mounts=deny;max_entries=200000;max_depth=96;max_file_bytes=2147483648;max_total_bytes=8589934592";
const MAX_SNAPSHOT_ENTRIES: usize = 200_000;
const MAX_SNAPSHOT_DEPTH: usize = 96;
const MAX_SNAPSHOT_FILE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_SNAPSHOT_TOTAL_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const STAGING_MARKER_NAME: &str = "owner.marker";

#[derive(Clone, Debug, Default)]
pub(crate) struct NodeWorkspaceFilesystemAdapter;

#[async_trait]
impl WorkspaceFilesystemPort for NodeWorkspaceFilesystemAdapter {
    async fn observe_workspace(
        &self,
        scope: &WorkspaceFilesystemScope,
    ) -> Result<WorkspaceObservation, WorkspaceFilesystemError> {
        let scope = scope.clone();
        tokio::task::spawn_blocking(move || observe_workspace_sync(&scope))
            .await
            .map_err(|_| WorkspaceFilesystemError::AdapterUnavailable)?
    }

    async fn prepare_file_effect(
        &self,
        scope: &WorkspaceFilesystemScope,
        changes: Vec<WorkspaceFileChangeInput>,
    ) -> Result<PreparedWorkspaceFileEffect, WorkspaceFilesystemError> {
        let scope = scope.clone();
        tokio::task::spawn_blocking(move || prepare_file_effect_sync(&scope, changes))
            .await
            .map_err(|_| WorkspaceFilesystemError::AdapterUnavailable)?
    }

    async fn apply_file_effect(
        &self,
        scope: &WorkspaceFilesystemScope,
        plan: &WorkspaceFileEffectPlan,
        staging: &WorkspaceStagingReceipt,
    ) -> Result<WorkspaceObservation, WorkspaceFilesystemError> {
        let scope = scope.clone();
        let plan = plan.clone();
        let staging = staging.clone();
        tokio::task::spawn_blocking(move || apply_file_effect_sync(&scope, &plan, &staging))
            .await
            .map_err(|_| WorkspaceFilesystemError::AdapterUnavailable)?
    }

    async fn stage_file_effect(
        &self,
        scope: &WorkspaceFilesystemScope,
        plan: &WorkspaceFileEffectPlan,
        blobs: &[WorkspaceBlob],
    ) -> Result<WorkspaceStagingReceipt, WorkspaceFilesystemError> {
        let scope = scope.clone();
        let plan = plan.clone();
        let blobs = blobs.to_vec();
        tokio::task::spawn_blocking(move || stage_file_effect_sync(&scope, &plan, &blobs))
            .await
            .map_err(|_| WorkspaceFilesystemError::AdapterUnavailable)?
    }

    async fn observe_transitions(
        &self,
        scope: &WorkspaceFilesystemScope,
        plan: &WorkspaceFileEffectPlan,
    ) -> Result<Vec<WorkspaceTransitionObservation>, WorkspaceFilesystemError> {
        let scope = scope.clone();
        let plan = plan.clone();
        tokio::task::spawn_blocking(move || observe_transitions_sync(&scope, &plan))
            .await
            .map_err(|_| WorkspaceFilesystemError::AdapterUnavailable)?
    }

    async fn cleanup_file_effect(
        &self,
        scope: &WorkspaceFilesystemScope,
        plan: &WorkspaceFileEffectPlan,
        staging: Option<&WorkspaceStagingReceipt>,
    ) -> Result<(), WorkspaceFilesystemError> {
        let scope = scope.clone();
        let plan = plan.clone();
        let staging = staging.cloned();
        tokio::task::spawn_blocking(move || {
            let opened = open_scope(&scope)?;
            let observations = observe_transitions_opened(&opened, &plan)?;
            if !all_transitions_after(&plan, &observations) {
                return Err(WorkspaceFilesystemError::RecoveryRequired);
            }
            cleanup_staging(&opened, &plan, staging.as_ref())?;
            opened.revalidate_location().map_err(map_project_error)
        })
        .await
        .map_err(|_| WorkspaceFilesystemError::AdapterUnavailable)?
    }

    async fn cleanup_unapplied_file_effect(
        &self,
        scope: &WorkspaceFilesystemScope,
        plan: &WorkspaceFileEffectPlan,
        staging: Option<&WorkspaceStagingReceipt>,
    ) -> Result<(), WorkspaceFilesystemError> {
        let scope = scope.clone();
        let plan = plan.clone();
        let staging = staging.cloned();
        tokio::task::spawn_blocking(move || {
            let opened = open_scope(&scope)?;
            let observations = observe_transitions_opened(&opened, &plan)?;
            if !all_transitions_before(&plan, &observations) {
                return Err(WorkspaceFilesystemError::RecoveryRequired);
            }
            cleanup_staging(&opened, &plan, staging.as_ref())?;
            opened.revalidate_location().map_err(map_project_error)
        })
        .await
        .map_err(|_| WorkspaceFilesystemError::AdapterUnavailable)?
    }

    async fn cleanup_recovery_file_effect(
        &self,
        scope: &WorkspaceFilesystemScope,
        plan: &WorkspaceFileEffectPlan,
        staging: Option<&WorkspaceStagingReceipt>,
    ) -> Result<(), WorkspaceFilesystemError> {
        let scope = scope.clone();
        let plan = plan.clone();
        let staging = staging.cloned();
        tokio::task::spawn_blocking(move || {
            let opened = open_scope(&scope)?;
            repair_interrupted_publications(&opened, &plan, staging.as_ref())?;
            let observations = observe_transitions_opened(&opened, &plan)?;
            if !all_transitions_recognized(&plan, &observations) {
                return Err(WorkspaceFilesystemError::Conflict);
            }
            cleanup_staging(&opened, &plan, staging.as_ref())?;
            opened.revalidate_location().map_err(map_project_error)
        })
        .await
        .map_err(|_| WorkspaceFilesystemError::AdapterUnavailable)?
    }

    async fn capture_checkpoint(
        &self,
        scope: &WorkspaceFilesystemScope,
        paths: Vec<ProjectRelativePath>,
    ) -> Result<CapturedWorkspaceCheckpoint, WorkspaceFilesystemError> {
        let scope = scope.clone();
        tokio::task::spawn_blocking(move || capture_checkpoint_sync(&scope, paths))
            .await
            .map_err(|_| WorkspaceFilesystemError::AdapterUnavailable)?
    }
}

fn open_scope(
    scope: &WorkspaceFilesystemScope,
) -> Result<OpenProjectRoot, WorkspaceFilesystemError> {
    let source = decode_source_identity(scope.source_identity)
        .map_err(|_| WorkspaceFilesystemError::Conflict)?;
    OpenProjectRoot::open_verified(Path::new(&scope.absolute_path), source)
        .map_err(map_project_error)
}

fn observe_workspace_sync(
    scope: &WorkspaceFilesystemScope,
) -> Result<WorkspaceObservation, WorkspaceFilesystemError> {
    let opened = open_scope(scope)?;
    let first = scan_workspace(&opened)?;
    opened.revalidate_location().map_err(map_project_error)?;
    let second = scan_workspace(&opened)?;
    opened.revalidate_location().map_err(map_project_error)?;
    if first != second {
        return Err(WorkspaceFilesystemError::Conflict);
    }
    Ok(first)
}

fn scan_workspace(
    opened: &OpenProjectRoot,
) -> Result<WorkspaceObservation, WorkspaceFilesystemError> {
    let root_identity = directory_identity(&opened.dir).map_err(map_project_error)?;
    let root_mount = directory_mount_identity(&opened.dir)?;
    let mut state = SnapshotScanState::default();
    scan_directory(
        &opened.dir,
        "",
        0,
        root_identity.volume,
        root_mount,
        &mut state,
    )?;
    Ok(WorkspaceObservation {
        scope_sha256: ContentSha256(Sha256::digest(SNAPSHOT_SCOPE_POLICY).into()),
        complete: true,
        entries: state.entries,
    })
}

#[derive(Default)]
struct SnapshotScanState {
    entries: Vec<WorkspaceManifestEntry>,
    hashed_bytes: u64,
}

fn scan_directory(
    dir: &Dir,
    parent: &str,
    depth: usize,
    root_volume: u64,
    root_mount: MountIdentity,
    state: &mut SnapshotScanState,
) -> Result<(), WorkspaceFilesystemError> {
    if depth > MAX_SNAPSHOT_DEPTH {
        return Err(WorkspaceFilesystemError::BoundExceeded);
    }
    let remaining = MAX_SNAPSHOT_ENTRIES.saturating_sub(state.entries.len());
    if remaining == 0 {
        return Err(WorkspaceFilesystemError::BoundExceeded);
    }
    let mut names = dir
        .entries()
        .map_err(|_| WorkspaceFilesystemError::AdapterUnavailable)?
        .take(remaining.saturating_add(1))
        .map(|entry| {
            entry
                .map(|entry| entry.file_name())
                .map_err(|_| WorkspaceFilesystemError::AdapterUnavailable)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if names.len() > remaining {
        return Err(WorkspaceFilesystemError::BoundExceeded);
    }
    names.sort();

    for name in names {
        if os_eq_ignore_ascii_case(&name, ".git") {
            continue;
        }
        let name_text = name
            .to_str()
            .ok_or(WorkspaceFilesystemError::UnsupportedObject)?;
        let relative = if parent.is_empty() {
            name_text.to_owned()
        } else {
            format!("{parent}/{name_text}")
        };
        let path = ProjectRelativePath::try_from(relative.as_str())
            .map_err(|_| WorkspaceFilesystemError::ScopeDenied)?;
        let metadata = dir
            .symlink_metadata(&name)
            .map_err(|_| WorkspaceFilesystemError::Conflict)?;
        let path_state = if metadata.file_type().is_symlink() {
            WorkspacePathState::Link {
                metadata_sha256: metadata_hash("link", &metadata, &link_target_bytes(dir, &name)?),
            }
        } else if metadata.is_dir() {
            let child = dir
                .open_dir_nofollow(&name)
                .map_err(|_| WorkspaceFilesystemError::ScopeDenied)?;
            let identity = directory_identity(&child).map_err(map_project_error)?;
            if identity.volume != root_volume {
                return Err(WorkspaceFilesystemError::ScopeDenied);
            }
            require_same_mount(root_mount, &child)?;
            let directory_state = WorkspacePathState::Directory {
                metadata_sha256: metadata_hash("directory", &metadata, &[]),
            };
            state.entries.push(WorkspaceManifestEntry {
                path,
                state: directory_state,
            });
            scan_directory(&child, &relative, depth + 1, root_volume, root_mount, state)?;
            continue;
        } else if metadata.is_file() {
            read_regular_state(dir, &name, &mut state.hashed_bytes, false, false)?.0
        } else {
            WorkspacePathState::Other {
                metadata_sha256: metadata_hash("other", &metadata, &[]),
            }
        };
        state.entries.push(WorkspaceManifestEntry {
            path,
            state: path_state,
        });
    }
    Ok(())
}

fn prepare_file_effect_sync(
    scope: &WorkspaceFilesystemScope,
    changes: Vec<WorkspaceFileChangeInput>,
) -> Result<PreparedWorkspaceFileEffect, WorkspaceFilesystemError> {
    if !scope.writable {
        return Err(WorkspaceFilesystemError::ScopeDenied);
    }
    let observation = observe_workspace_sync(scope)?;
    let opened = open_scope(scope)?;
    let manifest = observation
        .entries
        .iter()
        .map(|entry| (entry.path.as_str(), &entry.state))
        .collect::<BTreeMap<_, _>>();
    let mut proposals = Vec::with_capacity(changes.len());
    let mut proposed_blobs = Vec::new();
    let mut checkpoint_entries = BTreeMap::<String, WorkspaceCheckpointEntry>::new();
    let mut checkpoint_blobs = BTreeMap::<String, WorkspaceBlob>::new();
    let mut staged_bytes = 0_u64;

    for (index, change) in changes.into_iter().enumerate() {
        let current = manifest
            .get(change.path.as_str())
            .cloned()
            .cloned()
            .unwrap_or(WorkspacePathState::Absent);
        let content = match change.content {
            Some(bytes) => {
                reserve_staged_bytes(
                    &mut staged_bytes,
                    u64::try_from(bytes.len())
                        .map_err(|_| WorkspaceFilesystemError::BoundExceeded)?,
                )?;
                let blob = WorkspaceBlob::from_bytes(
                    format!("proposed-{index}-{}", hex_hash(&Sha256::digest(&bytes))),
                    bytes,
                )
                .map_err(|_| WorkspaceFilesystemError::BoundExceeded)?;
                let reference = blob.reference.clone();
                proposed_blobs.push(blob);
                Some(reference)
            }
            None => None,
        };
        let resulting_permissions = match change.kind {
            FileMutationKind::Add => change
                .resulting_permissions
                .or(Some(default_file_permissions())),
            FileMutationKind::Modify => Some(change.resulting_permissions.unwrap_or(
                permissions_from_state_and_path(&opened, &change.path, &current)?,
            )),
            FileMutationKind::Delete | FileMutationKind::Rename => change.resulting_permissions,
        };
        capture_checkpoint_entry(
            &opened,
            &change.path,
            current,
            &mut checkpoint_entries,
            &mut checkpoint_blobs,
            &mut staged_bytes,
        )?;
        if let Some(previous_path) = &change.previous_path {
            let previous = manifest
                .get(previous_path.as_str())
                .cloned()
                .cloned()
                .unwrap_or(WorkspacePathState::Absent);
            capture_checkpoint_entry(
                &opened,
                previous_path,
                previous,
                &mut checkpoint_entries,
                &mut checkpoint_blobs,
                &mut staged_bytes,
            )?;
        }
        proposals.push(ResolvedFileChangeProposal {
            kind: change.kind,
            path: change.path,
            previous_path: change.previous_path,
            content,
            expected_content_sha256: change.expected_content_sha256,
            resulting_permissions,
        });
    }
    opened.revalidate_location().map_err(map_project_error)?;
    Ok(PreparedWorkspaceFileEffect {
        observation,
        proposals,
        proposed_blobs,
        checkpoint_entries: checkpoint_entries.into_values().collect(),
        checkpoint_blobs: checkpoint_blobs.into_values().collect(),
    })
}

fn capture_checkpoint_sync(
    scope: &WorkspaceFilesystemScope,
    paths: Vec<ProjectRelativePath>,
) -> Result<CapturedWorkspaceCheckpoint, WorkspaceFilesystemError> {
    let observation = observe_workspace_sync(scope)?;
    let opened = open_scope(scope)?;
    let manifest = observation
        .entries
        .iter()
        .map(|entry| (entry.path.as_str(), &entry.state))
        .collect::<BTreeMap<_, _>>();
    let mut entries = BTreeMap::<String, WorkspaceCheckpointEntry>::new();
    let mut blobs = BTreeMap::<String, WorkspaceBlob>::new();
    let mut staged_bytes = 0_u64;
    for path in paths {
        let expected = manifest
            .get(path.as_str())
            .cloned()
            .cloned()
            .unwrap_or(WorkspacePathState::Absent);
        if matches!(
            expected,
            WorkspacePathState::Directory { .. }
                | WorkspacePathState::Link { .. }
                | WorkspacePathState::Other { .. }
        ) {
            return Err(WorkspaceFilesystemError::UnsupportedObject);
        }
        capture_checkpoint_entry(
            &opened,
            &path,
            expected,
            &mut entries,
            &mut blobs,
            &mut staged_bytes,
        )?;
    }
    opened.revalidate_location().map_err(map_project_error)?;
    Ok(CapturedWorkspaceCheckpoint {
        observation,
        entries: entries.into_values().collect(),
        blobs: blobs.into_values().collect(),
    })
}

fn capture_checkpoint_entry(
    opened: &OpenProjectRoot,
    path: &ProjectRelativePath,
    expected: WorkspacePathState,
    entries: &mut BTreeMap<String, WorkspaceCheckpointEntry>,
    blobs: &mut BTreeMap<String, WorkspaceBlob>,
    staged_bytes: &mut u64,
) -> Result<(), WorkspaceFilesystemError> {
    if entries.contains_key(path.as_str()) {
        return Ok(());
    }
    let (observed, bytes) = state_at(&opened.dir, path, true)?;
    if observed != expected {
        return Err(WorkspaceFilesystemError::Conflict);
    }
    let content = match bytes {
        Some(bytes) => {
            let id = format!("before-{}", hex_hash(&Sha256::digest(&bytes)));
            let blob = WorkspaceBlob::from_bytes(id.clone(), bytes)
                .map_err(|_| WorkspaceFilesystemError::BoundExceeded)?;
            let reference = blob.reference.clone();
            match blobs.get(&id) {
                Some(existing) if existing != &blob => {
                    return Err(WorkspaceFilesystemError::RecoveryRequired);
                }
                Some(_) => {}
                None => {
                    reserve_staged_bytes(staged_bytes, reference.byte_size)?;
                    blobs.insert(id, blob);
                }
            }
            Some(reference)
        }
        None => None,
    };
    let permissions = match &observed {
        WorkspacePathState::RegularFile { .. } => {
            Some(permissions_from_state_and_path(opened, path, &observed)?)
        }
        _ => None,
    };
    entries.insert(
        path.as_str().to_owned(),
        WorkspaceCheckpointEntry {
            path: path.clone(),
            state: observed,
            content,
            permissions,
        },
    );
    Ok(())
}

fn reserve_staged_bytes(total: &mut u64, additional: u64) -> Result<(), WorkspaceFilesystemError> {
    *total = total
        .checked_add(additional)
        .filter(|value| *value <= MAX_STAGED_OPERATION_BYTES)
        .ok_or(WorkspaceFilesystemError::BoundExceeded)?;
    Ok(())
}

fn apply_file_effect_sync(
    scope: &WorkspaceFilesystemScope,
    plan: &WorkspaceFileEffectPlan,
    staging: &WorkspaceStagingReceipt,
) -> Result<WorkspaceObservation, WorkspaceFilesystemError> {
    apply_file_effect_sync_with_failure(scope, plan, staging, None)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PublicationFailure {
    AfterModifyBackup(usize),
}

fn stage_file_effect_sync(
    scope: &WorkspaceFilesystemScope,
    plan: &WorkspaceFileEffectPlan,
    blobs: &[WorkspaceBlob],
) -> Result<WorkspaceStagingReceipt, WorkspaceFilesystemError> {
    if !scope.writable || scope.project_id != plan.project_id || scope.binding_id != plan.binding_id
    {
        return Err(WorkspaceFilesystemError::ScopeDenied);
    }
    let opened = open_scope(scope)?;
    let blob_map = validated_blob_map(blobs)?;
    preflight_transitions(&opened, plan)?;
    let (staging_dir, directory_identity) = create_staging_directory(&opened, plan)?;
    let marker_bytes = staging_marker_bytes(plan);
    let marker_name = OsStr::new(STAGING_MARKER_NAME);
    let mut created = Vec::new();
    let mut marker_identity = None;

    let result = (|| {
        write_staged_file(
            &staging_dir,
            marker_name,
            &marker_bytes,
            default_file_permissions(),
        )?;
        let observed_marker_identity = regular_file_identity(&staging_dir, marker_name)?;
        marker_identity = Some(observed_marker_identity);
        stage_after_images(&staging_dir, plan, &blob_map, &mut created)?;
        let mut objects = created.clone();
        objects.extend(capture_before_identities(&opened, plan)?);
        objects.sort_by(|left, right| {
            left.path
                .as_str()
                .cmp(right.path.as_str())
                .then_with(|| left.kind.cmp(&right.kind))
        });
        let receipt = WorkspaceStagingReceipt {
            directory_identity,
            marker_identity: observed_marker_identity,
            objects,
        };
        validate_staging_entries(&staging_dir, plan, &receipt)?;
        preflight_transitions(&opened, plan)?;
        opened.revalidate_location().map_err(map_project_error)?;
        Ok(receipt)
    })();

    match result {
        Ok(receipt) => Ok(receipt),
        Err(error) => {
            if cleanup_created_staging(&opened, plan, directory_identity, &created, marker_identity)
                .is_err()
            {
                Err(WorkspaceFilesystemError::RecoveryRequired)
            } else {
                Err(error)
            }
        }
    }
}

fn apply_file_effect_sync_with_failure(
    scope: &WorkspaceFilesystemScope,
    plan: &WorkspaceFileEffectPlan,
    staging: &WorkspaceStagingReceipt,
    failure: Option<PublicationFailure>,
) -> Result<WorkspaceObservation, WorkspaceFilesystemError> {
    if !scope.writable || scope.project_id != plan.project_id || scope.binding_id != plan.binding_id
    {
        return Err(WorkspaceFilesystemError::ScopeDenied);
    }
    let opened = open_scope(scope)?;
    preflight_transitions(&opened, plan)?;
    let staging_dir = open_staging_directory(&opened, plan, staging)?
        .ok_or(WorkspaceFilesystemError::RecoveryRequired)?;
    validate_staging_for_publication(&opened, &staging_dir, plan, staging)?;

    for (published, change) in plan.changes.iter().enumerate() {
        let result = match change.kind {
            FileMutationKind::Add => {
                publish_staged_add(&opened, &staging_dir, plan, staging, &change.path)
            }
            FileMutationKind::Modify => publish_staged_modify(
                &opened,
                &staging_dir,
                plan,
                staging,
                &change.path,
                failure == Some(PublicationFailure::AfterModifyBackup(published)),
            ),
            FileMutationKind::Delete => {
                publish_delete(&opened, &staging_dir, plan, staging, &change.path)
            }
            FileMutationKind::Rename => publish_rename(
                &opened,
                &staging_dir,
                plan,
                staging,
                change
                    .previous_path
                    .as_ref()
                    .ok_or(WorkspaceFilesystemError::RecoveryRequired)?,
                &change.path,
            ),
        };
        if result.is_err() {
            // Receipt-owned staging remains for deterministic reconciliation.
            return Err(WorkspaceFilesystemError::RecoveryRequired);
        }
    }

    let observations = observe_transitions_opened(&opened, plan)?;
    if !all_transitions_after(plan, &observations) {
        return Err(WorkspaceFilesystemError::RecoveryRequired);
    }
    drop(staging_dir);
    cleanup_staging(&opened, plan, Some(staging))?;
    opened.revalidate_location().map_err(map_project_error)?;
    observe_workspace_sync(scope)
}

fn observe_transitions_sync(
    scope: &WorkspaceFilesystemScope,
    plan: &WorkspaceFileEffectPlan,
) -> Result<Vec<WorkspaceTransitionObservation>, WorkspaceFilesystemError> {
    if scope.project_id != plan.project_id || scope.binding_id != plan.binding_id {
        return Err(WorkspaceFilesystemError::ScopeDenied);
    }
    let opened = open_scope(scope)?;
    let observations = observe_transitions_opened(&opened, plan)?;
    opened.revalidate_location().map_err(map_project_error)?;
    Ok(observations)
}

fn observe_transitions_opened(
    opened: &OpenProjectRoot,
    plan: &WorkspaceFileEffectPlan,
) -> Result<Vec<WorkspaceTransitionObservation>, WorkspaceFilesystemError> {
    plan.transitions
        .iter()
        .map(|transition| {
            let (state, _) = state_at(&opened.dir, &transition.path, false)?;
            Ok(WorkspaceTransitionObservation {
                path: transition.path.clone(),
                state,
            })
        })
        .collect()
}

fn all_transitions_after(
    plan: &WorkspaceFileEffectPlan,
    observations: &[WorkspaceTransitionObservation],
) -> bool {
    let by_path = observations
        .iter()
        .map(|observation| (observation.path.as_str(), &observation.state))
        .collect::<BTreeMap<_, _>>();
    plan.transitions
        .iter()
        .all(|transition| by_path.get(transition.path.as_str()).copied() == Some(&transition.after))
}

fn all_transitions_recognized(
    plan: &WorkspaceFileEffectPlan,
    observations: &[WorkspaceTransitionObservation],
) -> bool {
    let by_path = observations
        .iter()
        .map(|observation| (observation.path.as_str(), &observation.state))
        .collect::<BTreeMap<_, _>>();
    observations.len() == plan.transitions.len()
        && plan.transitions.iter().all(|transition| {
            by_path
                .get(transition.path.as_str())
                .is_some_and(|state| *state == &transition.before || *state == &transition.after)
        })
}

fn validated_blob_map(
    blobs: &[WorkspaceBlob],
) -> Result<BTreeMap<&str, &[u8]>, WorkspaceFilesystemError> {
    let mut result = BTreeMap::new();
    let mut staged_bytes = 0_u64;
    for blob in blobs {
        blob.validate()
            .map_err(|_| WorkspaceFilesystemError::RecoveryRequired)?;
        if result
            .insert(blob.reference.content_id.as_str(), blob.bytes.as_slice())
            .is_some()
        {
            return Err(WorkspaceFilesystemError::RecoveryRequired);
        }
        reserve_staged_bytes(&mut staged_bytes, blob.reference.byte_size)?;
    }
    Ok(result)
}

fn preflight_transitions(
    opened: &OpenProjectRoot,
    plan: &WorkspaceFileEffectPlan,
) -> Result<(), WorkspaceFilesystemError> {
    for transition in &plan.transitions {
        let (observed, _) = supported_state_at(&opened.dir, &transition.path)?;
        if observed != transition.before {
            return Err(WorkspaceFilesystemError::Conflict);
        }
    }
    opened.revalidate_location().map_err(map_project_error)
}

fn create_staging_directory(
    opened: &OpenProjectRoot,
    plan: &WorkspaceFileEffectPlan,
) -> Result<(Dir, WorkspaceObjectIdentity), WorkspaceFilesystemError> {
    let name = staging_directory_name(plan);
    if optional_symlink_metadata(&opened.dir, &name)
        .map_err(map_project_error)?
        .is_some()
    {
        return Err(WorkspaceFilesystemError::RecoveryRequired);
    }
    opened
        .dir
        .create_dir(&name)
        .map_err(|source| map_publication_error(source, true))?;
    sync_directory(&opened.dir, "sync_workspace_staging_directory_create")
        .map_err(map_project_error)?;
    let staging_dir = opened
        .dir
        .open_dir_nofollow(&name)
        .map_err(|_| WorkspaceFilesystemError::RecoveryRequired)?;
    ensure_opened_directory_name(&staging_dir, &name)?;
    let root_identity = directory_identity(&opened.dir).map_err(map_project_error)?;
    let staging_identity = directory_identity(&staging_dir).map_err(map_project_error)?;
    if root_identity.volume != staging_identity.volume {
        return Err(WorkspaceFilesystemError::ScopeDenied);
    }
    require_same_mount(directory_mount_identity(&opened.dir)?, &staging_dir)?;
    secure_staging_directory(&staging_dir)?;
    sync_directory(&staging_dir, "sync_workspace_staging_directory_permissions")
        .map_err(map_project_error)?;
    Ok((
        staging_dir,
        WorkspaceObjectIdentity {
            volume: staging_identity.volume,
            file: staging_identity.file,
        },
    ))
}

fn open_staging_directory(
    opened: &OpenProjectRoot,
    plan: &WorkspaceFileEffectPlan,
    receipt: &WorkspaceStagingReceipt,
) -> Result<Option<Dir>, WorkspaceFilesystemError> {
    let name = staging_directory_name(plan);
    let Some(metadata) =
        optional_symlink_metadata(&opened.dir, &name).map_err(map_project_error)?
    else {
        return Ok(None);
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(WorkspaceFilesystemError::RecoveryRequired);
    }
    let staging_dir = opened
        .dir
        .open_dir_nofollow(&name)
        .map_err(|_| WorkspaceFilesystemError::RecoveryRequired)?;
    ensure_opened_directory_name(&staging_dir, &name)?;
    if workspace_directory_identity(&staging_dir)? != receipt.directory_identity {
        return Err(WorkspaceFilesystemError::RecoveryRequired);
    }
    let root_identity = directory_identity(&opened.dir).map_err(map_project_error)?;
    if receipt.directory_identity.volume != root_identity.volume {
        return Err(WorkspaceFilesystemError::ScopeDenied);
    }
    require_same_mount(directory_mount_identity(&opened.dir)?, &staging_dir)?;
    verify_staging_marker(&staging_dir, plan, receipt)?;
    Ok(Some(staging_dir))
}

#[cfg(unix)]
fn secure_staging_directory(dir: &Dir) -> Result<(), WorkspaceFilesystemError> {
    use cap_std::fs::{Permissions, PermissionsExt as _};
    dir.set_permissions(".", Permissions::from_mode(0o700))
        .map_err(|_| WorkspaceFilesystemError::AdapterUnavailable)
}

#[cfg(not(unix))]
fn secure_staging_directory(_dir: &Dir) -> Result<(), WorkspaceFilesystemError> {
    Ok(())
}

fn staging_marker_bytes(plan: &WorkspaceFileEffectPlan) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(b"dennett.workspace-staging-marker.v1\0");
    hasher.update(plan.operation_id.0.as_bytes());
    hasher.update(plan.command_id.0.as_bytes());
    hasher.update(plan.staging_nonce.0);
    hasher.update(plan.intent_sha256.0);
    hasher.update(plan.scope_sha256.0);
    hasher.update(plan.safety_checkpoint_id.0.as_bytes());
    hasher.finalize().to_vec()
}

fn workspace_directory_identity(
    dir: &Dir,
) -> Result<WorkspaceObjectIdentity, WorkspaceFilesystemError> {
    let identity = directory_identity(dir).map_err(map_project_error)?;
    Ok(WorkspaceObjectIdentity {
        volume: identity.volume,
        file: identity.file,
    })
}

fn regular_file_identity(
    parent: &Dir,
    name: &OsStr,
) -> Result<WorkspaceObjectIdentity, WorkspaceFilesystemError> {
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let file = parent
        .open_with(name, &options)
        .map_err(|_| WorkspaceFilesystemError::Conflict)?;
    ensure_opened_file_name(&file, name)?;
    let metadata = file
        .metadata()
        .map_err(|_| WorkspaceFilesystemError::Conflict)?;
    if !metadata.is_file() {
        return Err(WorkspaceFilesystemError::RecoveryRequired);
    }
    require_regular_file_scope(parent, &file, &metadata)?;
    Ok(WorkspaceObjectIdentity {
        volume: MetadataExt::dev(&metadata),
        file: MetadataExt::ino(&metadata),
    })
}

fn verify_regular_file_evidence(
    parent: &Dir,
    name: &OsStr,
    expected_state: &WorkspacePathState,
    expected_identity: Option<WorkspaceObjectIdentity>,
) -> Result<WorkspaceObjectIdentity, WorkspaceFilesystemError> {
    let (first, _) = supported_state_at_parent(parent, name)?;
    if first != *expected_state {
        return Err(WorkspaceFilesystemError::Conflict);
    }
    let identity = regular_file_identity(parent, name)?;
    if expected_identity.is_some_and(|expected| expected != identity) {
        return Err(WorkspaceFilesystemError::RecoveryRequired);
    }
    let (second, _) = supported_state_at_parent(parent, name)?;
    if second != first {
        return Err(WorkspaceFilesystemError::Conflict);
    }
    Ok(identity)
}

fn verify_staging_marker(
    staging_dir: &Dir,
    plan: &WorkspaceFileEffectPlan,
    receipt: &WorkspaceStagingReceipt,
) -> Result<(), WorkspaceFilesystemError> {
    let marker = OsStr::new(STAGING_MARKER_NAME);
    let identity = regular_file_identity(staging_dir, marker)?;
    if identity != receipt.marker_identity {
        return Err(WorkspaceFilesystemError::RecoveryRequired);
    }
    let (_, bytes) = state_at_parent(staging_dir, marker, true)?;
    if bytes.as_deref() != Some(staging_marker_bytes(plan).as_slice()) {
        return Err(WorkspaceFilesystemError::RecoveryRequired);
    }
    Ok(())
}

fn expected_state_for_staged_object<'a>(
    plan: &'a WorkspaceFileEffectPlan,
    receipt: &WorkspaceStagedObjectReceipt,
) -> Result<&'a WorkspacePathState, WorkspaceFilesystemError> {
    let transition = transition_for(plan, &receipt.path)?;
    Ok(match receipt.kind {
        WorkspaceStagedObjectKind::Before => &transition.before,
        WorkspaceStagedObjectKind::After => &transition.after,
    })
}

fn validate_staging_entries(
    staging_dir: &Dir,
    plan: &WorkspaceFileEffectPlan,
    receipt: &WorkspaceStagingReceipt,
) -> Result<(), WorkspaceFilesystemError> {
    let mut allowed = receipt
        .objects
        .iter()
        .map(|object| staged_object_name(&object.path, object.kind))
        .collect::<std::collections::BTreeSet<_>>();
    if !allowed.insert(OsString::from(STAGING_MARKER_NAME)) {
        return Err(WorkspaceFilesystemError::RecoveryRequired);
    }
    for entry in staging_dir
        .entries()
        .map_err(|_| WorkspaceFilesystemError::AdapterUnavailable)?
    {
        let name = entry
            .map_err(|_| WorkspaceFilesystemError::AdapterUnavailable)?
            .file_name();
        if !allowed.contains(&name) {
            return Err(WorkspaceFilesystemError::RecoveryRequired);
        }
    }
    verify_staging_marker(staging_dir, plan, receipt)?;
    for object in &receipt.objects {
        let name = staged_object_name(&object.path, object.kind);
        if optional_symlink_metadata(staging_dir, &name)
            .map_err(map_project_error)?
            .is_some()
        {
            verify_regular_file_evidence(
                staging_dir,
                &name,
                expected_state_for_staged_object(plan, object)?,
                Some(object.identity),
            )?;
        }
    }
    Ok(())
}

fn validate_staging_for_publication(
    opened: &OpenProjectRoot,
    staging_dir: &Dir,
    plan: &WorkspaceFileEffectPlan,
    receipt: &WorkspaceStagingReceipt,
) -> Result<(), WorkspaceFilesystemError> {
    validate_staging_entries(staging_dir, plan, receipt)?;
    for object in &receipt.objects {
        let staged_name = staged_object_name(&object.path, object.kind);
        match object.kind {
            WorkspaceStagedObjectKind::After => {
                verify_regular_file_evidence(
                    staging_dir,
                    &staged_name,
                    expected_state_for_staged_object(plan, object)?,
                    Some(object.identity),
                )?;
            }
            WorkspaceStagedObjectKind::Before => {
                if optional_symlink_metadata(staging_dir, &staged_name)
                    .map_err(map_project_error)?
                    .is_some()
                {
                    return Err(WorkspaceFilesystemError::RecoveryRequired);
                }
                let (parent, target) = open_parent(&opened.dir, &object.path)?;
                verify_regular_file_evidence(
                    &parent,
                    &target,
                    expected_state_for_staged_object(plan, object)?,
                    Some(object.identity),
                )?;
            }
        }
    }
    preflight_transitions(opened, plan)
}

fn remove_staged_object(
    staging_dir: &Dir,
    plan: &WorkspaceFileEffectPlan,
    receipt: &WorkspaceStagedObjectReceipt,
) -> Result<(), WorkspaceFilesystemError> {
    let name = staged_object_name(&receipt.path, receipt.kind);
    verify_regular_file_evidence(
        staging_dir,
        &name,
        expected_state_for_staged_object(plan, receipt)?,
        Some(receipt.identity),
    )?;
    staging_dir
        .remove_file(&name)
        .map_err(|_| WorkspaceFilesystemError::RecoveryRequired)
}

fn ensure_only_marker_entry(staging_dir: &Dir) -> Result<(), WorkspaceFilesystemError> {
    let entries = staging_dir
        .entries()
        .map_err(|_| WorkspaceFilesystemError::AdapterUnavailable)?
        .map(|entry| {
            entry
                .map(|entry| entry.file_name())
                .map_err(|_| WorkspaceFilesystemError::AdapterUnavailable)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if entries == [OsString::from(STAGING_MARKER_NAME)] {
        Ok(())
    } else {
        Err(WorkspaceFilesystemError::RecoveryRequired)
    }
}

fn ensure_staging_empty(staging_dir: &Dir) -> Result<(), WorkspaceFilesystemError> {
    if staging_dir
        .entries()
        .map_err(|_| WorkspaceFilesystemError::AdapterUnavailable)?
        .next()
        .is_none()
    {
        Ok(())
    } else {
        Err(WorkspaceFilesystemError::RecoveryRequired)
    }
}

fn cleanup_created_staging(
    opened: &OpenProjectRoot,
    plan: &WorkspaceFileEffectPlan,
    expected_directory_identity: WorkspaceObjectIdentity,
    created: &[WorkspaceStagedObjectReceipt],
    expected_marker_identity: Option<WorkspaceObjectIdentity>,
) -> Result<(), WorkspaceFilesystemError> {
    let name = staging_directory_name(plan);
    let staging_dir = opened
        .dir
        .open_dir_nofollow(&name)
        .map_err(|_| WorkspaceFilesystemError::RecoveryRequired)?;
    if workspace_directory_identity(&staging_dir)? != expected_directory_identity {
        return Err(WorkspaceFilesystemError::RecoveryRequired);
    }
    let mut allowed = created
        .iter()
        .map(|object| staged_object_name(&object.path, object.kind))
        .collect::<std::collections::BTreeSet<_>>();
    allowed.insert(OsString::from(STAGING_MARKER_NAME));
    for entry in staging_dir
        .entries()
        .map_err(|_| WorkspaceFilesystemError::AdapterUnavailable)?
    {
        let entry = entry.map_err(|_| WorkspaceFilesystemError::AdapterUnavailable)?;
        if !allowed.contains(&entry.file_name()) {
            return Err(WorkspaceFilesystemError::RecoveryRequired);
        }
    }
    for object in created {
        let object_name = staged_object_name(&object.path, object.kind);
        if optional_symlink_metadata(&staging_dir, &object_name)
            .map_err(map_project_error)?
            .is_some()
        {
            remove_staged_object(&staging_dir, plan, object)?;
        }
    }
    if let Some(expected_marker_identity) = expected_marker_identity {
        if regular_file_identity(&staging_dir, OsStr::new(STAGING_MARKER_NAME))?
            != expected_marker_identity
        {
            return Err(WorkspaceFilesystemError::RecoveryRequired);
        }
        let (_, marker) = state_at_parent(&staging_dir, OsStr::new(STAGING_MARKER_NAME), true)?;
        if marker.as_deref() != Some(staging_marker_bytes(plan).as_slice()) {
            return Err(WorkspaceFilesystemError::RecoveryRequired);
        }
        staging_dir
            .remove_file(STAGING_MARKER_NAME)
            .map_err(|_| WorkspaceFilesystemError::RecoveryRequired)?;
    } else if optional_symlink_metadata(&staging_dir, OsStr::new(STAGING_MARKER_NAME))
        .map_err(map_project_error)?
        .is_some()
    {
        return Err(WorkspaceFilesystemError::RecoveryRequired);
    }
    ensure_staging_empty(&staging_dir)?;
    drop(staging_dir);
    opened
        .dir
        .remove_dir(&name)
        .map_err(|_| WorkspaceFilesystemError::RecoveryRequired)?;
    sync_directory(&opened.dir, "sync_workspace_failed_staging_cleanup").map_err(map_project_error)
}

fn stage_after_images(
    staging_dir: &Dir,
    plan: &WorkspaceFileEffectPlan,
    blobs: &BTreeMap<&str, &[u8]>,
    created: &mut Vec<WorkspaceStagedObjectReceipt>,
) -> Result<(), WorkspaceFilesystemError> {
    for transition in &plan.transitions {
        let Some(content) = &transition.content else {
            continue;
        };
        let bytes = blobs
            .get(content.content_id.as_str())
            .ok_or(WorkspaceFilesystemError::RecoveryRequired)?;
        let permissions = transition
            .resulting_permissions
            .ok_or(WorkspaceFilesystemError::RecoveryRequired)?;
        let temporary = staged_object_name(&transition.path, WorkspaceStagedObjectKind::After);
        write_staged_file(staging_dir, &temporary, bytes, permissions)?;
        let identity =
            verify_regular_file_evidence(staging_dir, &temporary, &transition.after, None)?;
        created.push(WorkspaceStagedObjectReceipt {
            path: transition.path.clone(),
            kind: WorkspaceStagedObjectKind::After,
            identity,
        });
    }
    Ok(())
}

fn capture_before_identities(
    opened: &OpenProjectRoot,
    plan: &WorkspaceFileEffectPlan,
) -> Result<Vec<WorkspaceStagedObjectReceipt>, WorkspaceFilesystemError> {
    let mut receipts = Vec::new();
    for transition in &plan.transitions {
        if !transition.before.is_regular_file() {
            continue;
        }
        let (parent, target) = open_parent(&opened.dir, &transition.path)?;
        let identity = verify_regular_file_evidence(&parent, &target, &transition.before, None)?;
        receipts.push(WorkspaceStagedObjectReceipt {
            path: transition.path.clone(),
            kind: WorkspaceStagedObjectKind::Before,
            identity,
        });
    }
    Ok(receipts)
}

fn publish_staged_add(
    opened: &OpenProjectRoot,
    staging_dir: &Dir,
    plan: &WorkspaceFileEffectPlan,
    staging: &WorkspaceStagingReceipt,
    path: &ProjectRelativePath,
) -> Result<(), WorkspaceFilesystemError> {
    let (parent, target) = open_parent(&opened.dir, path)?;
    let receipt = staged_receipt(staging, path, WorkspaceStagedObjectKind::After)?;
    let transition = transition_for(plan, path)?;
    let temporary = staged_object_name(path, WorkspaceStagedObjectKind::After);
    verify_regular_file_evidence(
        staging_dir,
        &temporary,
        &transition.after,
        Some(receipt.identity),
    )?;
    staging_dir
        .hard_link(&temporary, &parent, &target)
        .map_err(|source| map_publication_error(source, false))?;
    remove_staged_object(staging_dir, plan, receipt)?;
    sync_directory(staging_dir, "sync_workspace_add_staging").map_err(map_project_error)?;
    sync_directory(&parent, "sync_workspace_add").map_err(map_project_error)
}

fn publish_staged_modify(
    opened: &OpenProjectRoot,
    staging_dir: &Dir,
    plan: &WorkspaceFileEffectPlan,
    staging: &WorkspaceStagingReceipt,
    path: &ProjectRelativePath,
    fail_after_backup: bool,
) -> Result<(), WorkspaceFilesystemError> {
    let (parent, target) = open_parent(&opened.dir, path)?;
    let transition = transition_for(plan, path)?;
    let before_receipt = staged_receipt(staging, path, WorkspaceStagedObjectKind::Before)?;
    verify_regular_file_evidence(
        &parent,
        &target,
        &transition.before,
        Some(before_receipt.identity),
    )?;
    let backup = staged_object_name(path, WorkspaceStagedObjectKind::Before);
    move_noreplace(&parent, &target, staging_dir, &backup)?;
    verify_moved_backup(
        staging_dir,
        &backup,
        &parent,
        &target,
        &transition.before,
        before_receipt.identity,
    )?;
    if fail_after_backup {
        return Err(WorkspaceFilesystemError::RecoveryRequired);
    }
    let after_receipt = staged_receipt(staging, path, WorkspaceStagedObjectKind::After)?;
    let temporary = staged_object_name(path, WorkspaceStagedObjectKind::After);
    verify_regular_file_evidence(
        staging_dir,
        &temporary,
        &transition.after,
        Some(after_receipt.identity),
    )?;
    if let Err(error) = move_noreplace(staging_dir, &temporary, &parent, &target) {
        let _ = move_noreplace(staging_dir, &backup, &parent, &target);
        return Err(error);
    }
    sync_directory(staging_dir, "sync_workspace_modify_staging").map_err(map_project_error)?;
    sync_directory(&parent, "sync_workspace_modify").map_err(map_project_error)
}

fn publish_delete(
    opened: &OpenProjectRoot,
    staging_dir: &Dir,
    plan: &WorkspaceFileEffectPlan,
    staging: &WorkspaceStagingReceipt,
    path: &ProjectRelativePath,
) -> Result<(), WorkspaceFilesystemError> {
    let (parent, target) = open_parent(&opened.dir, path)?;
    let transition = transition_for(plan, path)?;
    let receipt = staged_receipt(staging, path, WorkspaceStagedObjectKind::Before)?;
    verify_regular_file_evidence(&parent, &target, &transition.before, Some(receipt.identity))?;
    let backup = staged_object_name(path, WorkspaceStagedObjectKind::Before);
    move_noreplace(&parent, &target, staging_dir, &backup)?;
    verify_moved_backup(
        staging_dir,
        &backup,
        &parent,
        &target,
        &transition.before,
        receipt.identity,
    )?;
    sync_directory(staging_dir, "sync_workspace_delete_staging").map_err(map_project_error)?;
    sync_directory(&parent, "sync_workspace_delete").map_err(map_project_error)
}

fn publish_rename(
    opened: &OpenProjectRoot,
    staging_dir: &Dir,
    plan: &WorkspaceFileEffectPlan,
    staging: &WorkspaceStagingReceipt,
    source: &ProjectRelativePath,
    target: &ProjectRelativePath,
) -> Result<(), WorkspaceFilesystemError> {
    let (source_parent, source_name) = open_parent(&opened.dir, source)?;
    let (target_parent, target_name) = open_parent(&opened.dir, target)?;
    let source_identity = directory_identity(&source_parent).map_err(map_project_error)?;
    let target_identity = directory_identity(&target_parent).map_err(map_project_error)?;
    if source_identity.volume != target_identity.volume {
        return Err(WorkspaceFilesystemError::ScopeDenied);
    }
    let source_transition = transition_for(plan, source)?;
    let receipt = staged_receipt(staging, source, WorkspaceStagedObjectKind::Before)?;
    verify_regular_file_evidence(
        &source_parent,
        &source_name,
        &source_transition.before,
        Some(receipt.identity),
    )?;
    let backup = staged_object_name(source, WorkspaceStagedObjectKind::Before);
    move_noreplace(&source_parent, &source_name, staging_dir, &backup)?;
    verify_moved_backup(
        staging_dir,
        &backup,
        &source_parent,
        &source_name,
        &source_transition.before,
        receipt.identity,
    )?;
    if let Err(error) = staging_dir.hard_link(&backup, &target_parent, &target_name) {
        let _ = move_noreplace(staging_dir, &backup, &source_parent, &source_name);
        return Err(map_publication_error(error, false));
    }
    sync_directory(staging_dir, "sync_workspace_rename_staging").map_err(map_project_error)?;
    sync_directory(&source_parent, "sync_workspace_rename_source").map_err(map_project_error)?;
    sync_directory(&target_parent, "sync_workspace_rename_target").map_err(map_project_error)
}

fn repair_interrupted_publications(
    opened: &OpenProjectRoot,
    plan: &WorkspaceFileEffectPlan,
    staging: Option<&WorkspaceStagingReceipt>,
) -> Result<(), WorkspaceFilesystemError> {
    let staging = staging.ok_or(WorkspaceFilesystemError::RecoveryRequired)?;
    let staging_dir = open_staging_directory(opened, plan, staging)?
        .ok_or(WorkspaceFilesystemError::RecoveryRequired)?;
    validate_staging_entries(&staging_dir, plan, staging)?;
    for change in &plan.changes {
        if change.kind != FileMutationKind::Modify {
            continue;
        }
        let transition = transition_for(plan, &change.path)?;
        let (parent, target) = open_parent(&opened.dir, &change.path)?;
        let (target_state, _) = state_at_parent(&parent, &target, false)?;
        if target_state != WorkspacePathState::Absent {
            continue;
        }
        let before_receipt =
            staged_receipt(staging, &change.path, WorkspaceStagedObjectKind::Before)?;
        let after_receipt =
            staged_receipt(staging, &change.path, WorkspaceStagedObjectKind::After)?;
        let before = staged_object_name(&change.path, WorkspaceStagedObjectKind::Before);
        let after = staged_object_name(&change.path, WorkspaceStagedObjectKind::After);
        verify_regular_file_evidence(
            &staging_dir,
            &before,
            &transition.before,
            Some(before_receipt.identity),
        )?;
        verify_regular_file_evidence(
            &staging_dir,
            &after,
            &transition.after,
            Some(after_receipt.identity),
        )?;
        move_noreplace(&staging_dir, &before, &parent, &target)?;
        sync_directory(&staging_dir, "sync_workspace_interrupted_modify_staging")
            .map_err(map_project_error)?;
        sync_directory(&parent, "sync_workspace_interrupted_modify_repair")
            .map_err(map_project_error)?;
    }
    Ok(())
}

fn verify_moved_backup(
    staging_dir: &Dir,
    backup: &OsStr,
    target_parent: &Dir,
    target: &OsStr,
    expected: &WorkspacePathState,
    expected_identity: WorkspaceObjectIdentity,
) -> Result<(), WorkspaceFilesystemError> {
    let observation =
        verify_regular_file_evidence(staging_dir, backup, expected, Some(expected_identity));
    match observation {
        Ok(_) => Ok(()),
        Err(WorkspaceFilesystemError::Conflict) => {
            restore_moved_backup(staging_dir, backup, target_parent, target)?;
            Err(WorkspaceFilesystemError::Conflict)
        }
        Err(error) => {
            restore_moved_backup(staging_dir, backup, target_parent, target)?;
            Err(error)
        }
    }
}

fn restore_moved_backup(
    staging_dir: &Dir,
    backup: &OsStr,
    target_parent: &Dir,
    target: &OsStr,
) -> Result<(), WorkspaceFilesystemError> {
    move_noreplace(staging_dir, backup, target_parent, target)
        .map_err(|_| WorkspaceFilesystemError::RecoveryRequired)?;
    sync_directory(staging_dir, "sync_workspace_backup_restore_staging")
        .map_err(map_project_error)?;
    sync_directory(target_parent, "sync_workspace_backup_restore").map_err(map_project_error)
}

fn cleanup_staging(
    opened: &OpenProjectRoot,
    plan: &WorkspaceFileEffectPlan,
    staging: Option<&WorkspaceStagingReceipt>,
) -> Result<(), WorkspaceFilesystemError> {
    let name = staging_directory_name(plan);
    let Some(staging) = staging else {
        return if optional_symlink_metadata(&opened.dir, &name)
            .map_err(map_project_error)?
            .is_none()
        {
            Ok(())
        } else {
            Err(WorkspaceFilesystemError::RecoveryRequired)
        };
    };
    let Some(staging_dir) = open_staging_directory(opened, plan, staging)? else {
        return Ok(());
    };
    validate_staging_entries(&staging_dir, plan, staging)?;
    for object in &staging.objects {
        let object_name = staged_object_name(&object.path, object.kind);
        if optional_symlink_metadata(&staging_dir, &object_name)
            .map_err(map_project_error)?
            .is_none()
        {
            continue;
        }
        remove_staged_object(&staging_dir, plan, object)?;
        sync_directory(&staging_dir, "sync_workspace_staging_object_cleanup")
            .map_err(map_project_error)?;
    }
    validate_staging_entries(&staging_dir, plan, staging)?;
    ensure_only_marker_entry(&staging_dir)?;
    verify_staging_marker(&staging_dir, plan, staging)?;
    staging_dir
        .remove_file(STAGING_MARKER_NAME)
        .map_err(|_| WorkspaceFilesystemError::RecoveryRequired)?;
    sync_directory(&staging_dir, "sync_workspace_staging_marker_cleanup")
        .map_err(map_project_error)?;
    ensure_staging_empty(&staging_dir)?;
    if workspace_directory_identity(&staging_dir)? != staging.directory_identity {
        return Err(WorkspaceFilesystemError::RecoveryRequired);
    }
    drop(staging_dir);
    opened
        .dir
        .remove_dir(&name)
        .map_err(|_| WorkspaceFilesystemError::RecoveryRequired)?;
    sync_directory(&opened.dir, "sync_workspace_staging_directory_cleanup")
        .map_err(map_project_error)?;
    Ok(())
}

fn transition_for<'a>(
    plan: &'a WorkspaceFileEffectPlan,
    path: &ProjectRelativePath,
) -> Result<&'a dennett_effect_core::workspace::WorkspacePathTransition, WorkspaceFilesystemError> {
    plan.transitions
        .iter()
        .find(|transition| transition.path == *path)
        .ok_or(WorkspaceFilesystemError::RecoveryRequired)
}

fn staged_receipt<'a>(
    staging: &'a WorkspaceStagingReceipt,
    path: &ProjectRelativePath,
    kind: WorkspaceStagedObjectKind,
) -> Result<&'a WorkspaceStagedObjectReceipt, WorkspaceFilesystemError> {
    staging
        .objects
        .iter()
        .find(|receipt| receipt.path == *path && receipt.kind == kind)
        .ok_or(WorkspaceFilesystemError::RecoveryRequired)
}

fn staged_object_name(path: &ProjectRelativePath, kind: WorkspaceStagedObjectKind) -> OsString {
    let mut hasher = Sha256::new();
    hasher.update(b"dennett.workspace-staged-object.v1\0");
    hasher.update([match kind {
        WorkspaceStagedObjectKind::Before => 1,
        WorkspaceStagedObjectKind::After => 2,
    }]);
    hasher.update(path.as_str().as_bytes());
    let digest = hasher.finalize();
    let suffix = match kind {
        WorkspaceStagedObjectKind::Before => "before",
        WorkspaceStagedObjectKind::After => "after",
    };
    OsString::from(format!("{}-{suffix}.tmp", hex_hash(&digest)))
}

fn staging_directory_name(plan: &WorkspaceFileEffectPlan) -> OsString {
    OsString::from(format!(
        ".dennett-ws-stage-{}",
        hex_hash(&plan.staging_nonce.0)
    ))
}

fn state_at(
    root: &Dir,
    path: &ProjectRelativePath,
    capture_bytes: bool,
) -> Result<(WorkspacePathState, Option<Vec<u8>>), WorkspaceFilesystemError> {
    let (parent, name) = open_parent(root, path)?;
    state_at_parent(&parent, &name, capture_bytes)
}

fn supported_state_at(
    root: &Dir,
    path: &ProjectRelativePath,
) -> Result<(WorkspacePathState, Option<Vec<u8>>), WorkspaceFilesystemError> {
    let (parent, name) = open_parent(root, path)?;
    supported_state_at_parent(&parent, &name)
}

fn state_at_parent(
    parent: &Dir,
    name: &OsStr,
    capture_bytes: bool,
) -> Result<(WorkspacePathState, Option<Vec<u8>>), WorkspaceFilesystemError> {
    state_at_parent_with_policy(parent, name, capture_bytes, capture_bytes)
}

fn supported_state_at_parent(
    parent: &Dir,
    name: &OsStr,
) -> Result<(WorkspacePathState, Option<Vec<u8>>), WorkspaceFilesystemError> {
    state_at_parent_with_policy(parent, name, false, true)
}

fn state_at_parent_with_policy(
    parent: &Dir,
    name: &OsStr,
    capture_bytes: bool,
    require_supported_metadata: bool,
) -> Result<(WorkspacePathState, Option<Vec<u8>>), WorkspaceFilesystemError> {
    let Some(metadata) = optional_symlink_metadata(parent, name).map_err(map_project_error)? else {
        return Ok((WorkspacePathState::Absent, None));
    };
    if metadata.file_type().is_symlink() {
        return Ok((
            WorkspacePathState::Link {
                metadata_sha256: metadata_hash(
                    "link",
                    &metadata,
                    &link_target_bytes(parent, name)?,
                ),
            },
            None,
        ));
    }
    if metadata.is_dir() {
        return Ok((
            WorkspacePathState::Directory {
                metadata_sha256: metadata_hash("directory", &metadata, &[]),
            },
            None,
        ));
    }
    if !metadata.is_file() {
        return Ok((
            WorkspacePathState::Other {
                metadata_sha256: metadata_hash("other", &metadata, &[]),
            },
            None,
        ));
    }
    let mut ignored_total = 0;
    read_regular_state(
        parent,
        name,
        &mut ignored_total,
        capture_bytes,
        require_supported_metadata,
    )
}

fn read_regular_state(
    parent: &Dir,
    name: &OsStr,
    total_bytes: &mut u64,
    capture_bytes: bool,
    require_supported_metadata: bool,
) -> Result<(WorkspacePathState, Option<Vec<u8>>), WorkspaceFilesystemError> {
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let mut file = parent
        .open_with(name, &options)
        .map_err(|_| WorkspaceFilesystemError::Conflict)?;
    ensure_opened_file_name(&file, name)?;
    let before = file
        .metadata()
        .map_err(|_| WorkspaceFilesystemError::Conflict)?;
    if !before.is_file() || before.len() > MAX_SNAPSHOT_FILE_BYTES {
        return Err(WorkspaceFilesystemError::BoundExceeded);
    }
    require_regular_file_scope(parent, &file, &before)?;
    if capture_bytes && before.len() > MAX_STAGED_FILE_BYTES {
        return Err(WorkspaceFilesystemError::BoundExceeded);
    }
    if require_supported_metadata {
        ensure_checkpoint_metadata_supported(&file)?;
    }
    let next_total = total_bytes
        .checked_add(before.len())
        .filter(|value| *value <= MAX_SNAPSHOT_TOTAL_BYTES)
        .ok_or(WorkspaceFilesystemError::BoundExceeded)?;
    let mut bytes = if capture_bytes {
        let capacity =
            usize::try_from(before.len()).map_err(|_| WorkspaceFilesystemError::BoundExceeded)?;
        Some(Vec::with_capacity(capacity))
    } else {
        None
    };
    let mut hasher = Sha256::new();
    let mut observed_len = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| WorkspaceFilesystemError::Conflict)?;
        if read == 0 {
            break;
        }
        observed_len = observed_len
            .checked_add(u64::try_from(read).expect("read buffer length fits u64"))
            .filter(|length| *length <= before.len())
            .ok_or(WorkspaceFilesystemError::Conflict)?;
        hasher.update(&buffer[..read]);
        if let Some(bytes) = &mut bytes {
            bytes.extend_from_slice(&buffer[..read]);
        }
    }
    let after = file
        .metadata()
        .map_err(|_| WorkspaceFilesystemError::Conflict)?;
    if !same_open_file_observation(&before, &after) || observed_len != after.len() {
        return Err(WorkspaceFilesystemError::Conflict);
    }
    *total_bytes = next_total;
    let permissions = portable_permissions(&after);
    Ok((
        WorkspacePathState::RegularFile {
            content_sha256: ContentSha256(hasher.finalize().into()),
            metadata_sha256: permissions.metadata_sha256(),
            byte_size: after.len(),
        },
        bytes,
    ))
}

fn same_open_file_observation(left: &Metadata, right: &Metadata) -> bool {
    MetadataExt::dev(left) == MetadataExt::dev(right)
        && MetadataExt::ino(left) == MetadataExt::ino(right)
        && left.len() == right.len()
        && left.modified().ok() == right.modified().ok()
        && portable_permissions(left) == portable_permissions(right)
}

#[cfg(unix)]
fn ensure_checkpoint_metadata_supported(
    file: &cap_std::fs::File,
) -> Result<(), WorkspaceFilesystemError> {
    let mut names = [0_u8; 64 * 1024];
    let count = rustix::fs::flistxattr(file, &mut names)
        .map_err(|_| WorkspaceFilesystemError::UnsupportedObject)?;
    if count == 0 {
        Ok(())
    } else {
        Err(WorkspaceFilesystemError::UnsupportedObject)
    }
}

#[cfg(windows)]
fn ensure_checkpoint_metadata_supported(
    file: &cap_std::fs::File,
) -> Result<(), WorkspaceFilesystemError> {
    ensure_windows_file_has_only_default_stream(file)?;
    ensure_windows_file_has_inherited_acl(file)
}

#[cfg(not(any(unix, windows)))]
fn ensure_checkpoint_metadata_supported(
    _file: &cap_std::fs::File,
) -> Result<(), WorkspaceFilesystemError> {
    Err(WorkspaceFilesystemError::UnsupportedObject)
}

#[cfg(windows)]
fn ensure_windows_file_has_only_default_stream(
    file: &cap_std::fs::File,
) -> Result<(), WorkspaceFilesystemError> {
    use std::{mem::size_of, os::windows::io::AsRawHandle as _, slice};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_STREAM_INFO, FileStreamInfo, GetFileInformationByHandleEx,
    };

    const STREAM_BUFFER_BYTES: usize = 64 * 1024;
    let mut buffer = vec![0_usize; STREAM_BUFFER_BYTES / size_of::<usize>()];
    // SAFETY: `file` owns a valid handle for the call, the aligned buffer is
    // writable for its full declared byte length, and the API does not retain it.
    let success = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle().cast(),
            FileStreamInfo,
            buffer.as_mut_ptr().cast(),
            u32::try_from(STREAM_BUFFER_BYTES).expect("stream buffer fits u32"),
        )
    };
    if success == 0 {
        return Err(WorkspaceFilesystemError::UnsupportedObject);
    }

    // SAFETY: a successful call initialized at least one FILE_STREAM_INFO
    // record in the suitably aligned output buffer.
    let info = unsafe { &*buffer.as_ptr().cast::<FILE_STREAM_INFO>() };
    if info.NextEntryOffset != 0 {
        return Err(WorkspaceFilesystemError::UnsupportedObject);
    }
    let name_bytes = usize::try_from(info.StreamNameLength)
        .map_err(|_| WorkspaceFilesystemError::UnsupportedObject)?;
    if name_bytes % size_of::<u16>() != 0
        || name_bytes > STREAM_BUFFER_BYTES - size_of::<FILE_STREAM_INFO>()
    {
        return Err(WorkspaceFilesystemError::UnsupportedObject);
    }
    let name_len = name_bytes / size_of::<u16>();
    // SAFETY: StreamNameLength was validated against the initialized output
    // buffer and Windows stores stream names as UTF-16 code units.
    let name = unsafe { slice::from_raw_parts(info.StreamName.as_ptr(), name_len) };
    let default_stream = "::$DATA".encode_utf16().collect::<Vec<_>>();
    if name == default_stream {
        Ok(())
    } else {
        Err(WorkspaceFilesystemError::UnsupportedObject)
    }
}

#[cfg(windows)]
fn ensure_windows_file_has_inherited_acl(
    file: &cap_std::fs::File,
) -> Result<(), WorkspaceFilesystemError> {
    use std::{
        ffi::c_void,
        mem::{size_of, zeroed},
        os::windows::io::AsRawHandle as _,
        ptr,
    };
    use windows_sys::Win32::{
        Foundation::{ERROR_SUCCESS, LocalFree},
        Security::{
            ACE_HEADER, ACL, ACL_SIZE_INFORMATION, AclSizeInformation,
            Authorization::{GetSecurityInfo, SE_FILE_OBJECT},
            DACL_SECURITY_INFORMATION, GetAce, GetAclInformation, GetSecurityDescriptorControl,
            INHERITED_ACE, PSECURITY_DESCRIPTOR, SE_DACL_PROTECTED,
        },
    };

    struct SecurityDescriptorGuard(PSECURITY_DESCRIPTOR);
    impl Drop for SecurityDescriptorGuard {
        fn drop(&mut self) {
            // SAFETY: GetSecurityInfo allocates this descriptor with LocalAlloc.
            unsafe {
                LocalFree(self.0);
            }
        }
    }

    let mut dacl: *mut ACL = ptr::null_mut();
    let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
    // SAFETY: the file handle remains valid and the output pointers live for
    // the duration of the call.
    let status = unsafe {
        GetSecurityInfo(
            file.as_raw_handle().cast(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            ptr::null_mut(),
            &mut dacl,
            ptr::null_mut(),
            &mut descriptor,
        )
    };
    if status != ERROR_SUCCESS || descriptor.is_null() || dacl.is_null() {
        return Err(WorkspaceFilesystemError::UnsupportedObject);
    }
    let _descriptor = SecurityDescriptorGuard(descriptor);

    let mut control = 0_u16;
    let mut revision = 0_u32;
    // SAFETY: descriptor is owned by the guard and valid until function exit.
    if unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) } == 0
        || control & SE_DACL_PROTECTED != 0
    {
        return Err(WorkspaceFilesystemError::UnsupportedObject);
    }

    // SAFETY: ACL_SIZE_INFORMATION is a plain output structure and dacl points
    // into the guarded security descriptor.
    let mut information: ACL_SIZE_INFORMATION = unsafe { zeroed() };
    if unsafe {
        GetAclInformation(
            dacl,
            (&mut information as *mut ACL_SIZE_INFORMATION).cast::<c_void>(),
            u32::try_from(size_of::<ACL_SIZE_INFORMATION>()).expect("ACL info size fits u32"),
            AclSizeInformation,
        )
    } == 0
    {
        return Err(WorkspaceFilesystemError::UnsupportedObject);
    }

    for index in 0..information.AceCount {
        let mut ace: *mut c_void = ptr::null_mut();
        // SAFETY: index is bounded by the ACE count returned for this DACL.
        if unsafe { GetAce(dacl, index, &mut ace) } == 0 || ace.is_null() {
            return Err(WorkspaceFilesystemError::UnsupportedObject);
        }
        // SAFETY: every ACE begins with ACE_HEADER.
        let header = unsafe { &*ace.cast::<ACE_HEADER>() };
        if u32::from(header.AceFlags) & INHERITED_ACE == 0 {
            return Err(WorkspaceFilesystemError::UnsupportedObject);
        }
    }
    Ok(())
}

fn permissions_from_state_and_path(
    opened: &OpenProjectRoot,
    path: &ProjectRelativePath,
    state: &WorkspacePathState,
) -> Result<PortableFilePermissions, WorkspaceFilesystemError> {
    if !matches!(state, WorkspacePathState::RegularFile { .. }) {
        return Err(WorkspaceFilesystemError::Conflict);
    }
    let (parent, name) = open_parent(&opened.dir, path)?;
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let metadata = parent
        .open_with(&name, &options)
        .and_then(|file| file.metadata())
        .map_err(|_| WorkspaceFilesystemError::Conflict)?;
    let permissions = portable_permissions(&metadata);
    let WorkspacePathState::RegularFile {
        metadata_sha256, ..
    } = state
    else {
        return Err(WorkspaceFilesystemError::Conflict);
    };
    if permissions.metadata_sha256() != *metadata_sha256 {
        return Err(WorkspaceFilesystemError::Conflict);
    }
    Ok(permissions)
}

fn open_parent(
    root: &Dir,
    path: &ProjectRelativePath,
) -> Result<(Dir, OsString), WorkspaceFilesystemError> {
    let mut segments = path.as_str().split('/').peekable();
    let root_identity = directory_identity(root).map_err(map_project_error)?;
    let root_mount = directory_mount_identity(root)?;
    let mut current = root
        .try_clone()
        .map_err(|_| WorkspaceFilesystemError::AdapterUnavailable)?;
    while let Some(segment) = segments.next() {
        let name = OsString::from(segment);
        if segments.peek().is_none() {
            ensure_exact_case_if_present(&current, &name)?;
            return Ok((current, name));
        }
        ensure_exact_case_if_present(&current, &name)?;
        let metadata = current
            .symlink_metadata(&name)
            .map_err(|_| WorkspaceFilesystemError::ScopeDenied)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(WorkspaceFilesystemError::ScopeDenied);
        }
        let child = current
            .open_dir_nofollow(&name)
            .map_err(|_| WorkspaceFilesystemError::ScopeDenied)?;
        ensure_opened_directory_name(&child, &name)?;
        let identity = directory_identity(&child).map_err(map_project_error)?;
        if identity.volume != root_identity.volume {
            return Err(WorkspaceFilesystemError::ScopeDenied);
        }
        require_same_mount(root_mount, &child)?;
        current = child;
    }
    Err(WorkspaceFilesystemError::ScopeDenied)
}

type MountIdentity = u64;

#[cfg(target_os = "linux")]
fn directory_mount_identity(dir: &Dir) -> Result<MountIdentity, WorkspaceFilesystemError> {
    use rustix::fs::{AtFlags, StatxFlags, statx};

    let stat = statx(
        dir,
        "",
        AtFlags::EMPTY_PATH | AtFlags::NO_AUTOMOUNT,
        StatxFlags::MNT_ID,
    )
    .map_err(|_| WorkspaceFilesystemError::ScopeDenied)?;
    if stat.stx_mask & StatxFlags::MNT_ID.bits() == 0 {
        return Err(WorkspaceFilesystemError::ScopeDenied);
    }
    Ok(stat.stx_mnt_id)
}

#[cfg(not(target_os = "linux"))]
fn directory_mount_identity(_dir: &Dir) -> Result<MountIdentity, WorkspaceFilesystemError> {
    Ok(0)
}

#[cfg(target_os = "linux")]
fn require_same_mount(expected: MountIdentity, dir: &Dir) -> Result<(), WorkspaceFilesystemError> {
    if directory_mount_identity(dir)? == expected {
        Ok(())
    } else {
        Err(WorkspaceFilesystemError::ScopeDenied)
    }
}

#[cfg(not(target_os = "linux"))]
fn require_same_mount(
    _expected: MountIdentity,
    _dir: &Dir,
) -> Result<(), WorkspaceFilesystemError> {
    Ok(())
}

#[cfg(target_os = "linux")]
fn require_regular_file_scope(
    parent: &Dir,
    file: &cap_std::fs::File,
    metadata: &Metadata,
) -> Result<(), WorkspaceFilesystemError> {
    if MetadataExt::dev(metadata)
        != directory_identity(parent)
            .map_err(map_project_error)?
            .volume
    {
        return Err(WorkspaceFilesystemError::ScopeDenied);
    }
    if file_mount_identity(file)? != directory_mount_identity(parent)? {
        return Err(WorkspaceFilesystemError::ScopeDenied);
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn file_mount_identity(
    file: &cap_std::fs::File,
) -> Result<MountIdentity, WorkspaceFilesystemError> {
    use rustix::fs::{AtFlags, StatxFlags, statx};

    let stat = statx(
        file,
        "",
        AtFlags::EMPTY_PATH | AtFlags::NO_AUTOMOUNT,
        StatxFlags::MNT_ID,
    )
    .map_err(|_| WorkspaceFilesystemError::ScopeDenied)?;
    if stat.stx_mask & StatxFlags::MNT_ID.bits() == 0 {
        return Err(WorkspaceFilesystemError::ScopeDenied);
    }
    Ok(stat.stx_mnt_id)
}

#[cfg(not(target_os = "linux"))]
fn require_regular_file_scope(
    _parent: &Dir,
    _file: &cap_std::fs::File,
    _metadata: &Metadata,
) -> Result<(), WorkspaceFilesystemError> {
    Ok(())
}

#[cfg(windows)]
fn ensure_exact_case_if_present(
    dir: &Dir,
    requested: &OsStr,
) -> Result<(), WorkspaceFilesystemError> {
    for entry in dir
        .entries()
        .map_err(|_| WorkspaceFilesystemError::AdapterUnavailable)?
    {
        let name = entry
            .map_err(|_| WorkspaceFilesystemError::AdapterUnavailable)?
            .file_name();
        if windows_names_equal_ignore_case(&name, requested)? && name != requested {
            return Err(WorkspaceFilesystemError::ScopeDenied);
        }
    }
    Ok(())
}

#[cfg(windows)]
fn windows_names_equal_ignore_case(
    first: &OsStr,
    second: &OsStr,
) -> Result<bool, WorkspaceFilesystemError> {
    use std::os::windows::ffi::OsStrExt as _;
    use windows_sys::Win32::Globalization::{CSTR_EQUAL, CompareStringOrdinal};

    let first = first.encode_wide().collect::<Vec<_>>();
    let second = second.encode_wide().collect::<Vec<_>>();
    let first_len =
        i32::try_from(first.len()).map_err(|_| WorkspaceFilesystemError::ScopeDenied)?;
    let second_len =
        i32::try_from(second.len()).map_err(|_| WorkspaceFilesystemError::ScopeDenied)?;
    // SAFETY: both UTF-16 buffers remain alive for the duration of the call,
    // and the explicit lengths keep the API from reading beyond them.
    let comparison = unsafe {
        CompareStringOrdinal(
            first.as_ptr(),
            first_len,
            second.as_ptr(),
            second_len,
            true.into(),
        )
    };
    if comparison == 0 {
        Err(WorkspaceFilesystemError::AdapterUnavailable)
    } else {
        Ok(comparison == CSTR_EQUAL)
    }
}

#[cfg(windows)]
fn ensure_opened_file_name(
    file: &cap_std::fs::File,
    requested: &OsStr,
) -> Result<(), WorkspaceFilesystemError> {
    use std::os::windows::io::AsRawHandle as _;
    ensure_opened_name(file.as_raw_handle().cast(), requested)
}

#[cfg(windows)]
fn ensure_opened_directory_name(
    dir: &Dir,
    requested: &OsStr,
) -> Result<(), WorkspaceFilesystemError> {
    use std::os::windows::io::AsRawHandle as _;
    ensure_opened_name(dir.as_raw_handle().cast(), requested)
}

#[cfg(windows)]
fn ensure_opened_name(
    handle: windows_sys::Win32::Foundation::HANDLE,
    requested: &OsStr,
) -> Result<(), WorkspaceFilesystemError> {
    use std::os::windows::ffi::OsStringExt as _;
    use windows_sys::Win32::Storage::FileSystem::GetFinalPathNameByHandleW;

    // SAFETY: the borrowed handle remains valid for both calls and a null
    // output buffer with length zero requests the required UTF-16 length.
    let required = unsafe { GetFinalPathNameByHandleW(handle, std::ptr::null_mut(), 0, 0) };
    if required == 0 {
        return Err(WorkspaceFilesystemError::ScopeDenied);
    }
    let mut path = vec![
        0_u16;
        usize::try_from(required)
            .map_err(|_| WorkspaceFilesystemError::ScopeDenied)?
            .saturating_add(1)
    ];
    // SAFETY: `path` is writable for its declared length and the API does not
    // retain the pointer.
    let written = unsafe {
        GetFinalPathNameByHandleW(
            handle,
            path.as_mut_ptr(),
            u32::try_from(path.len()).map_err(|_| WorkspaceFilesystemError::ScopeDenied)?,
            0,
        )
    };
    let written = usize::try_from(written).map_err(|_| WorkspaceFilesystemError::ScopeDenied)?;
    if written == 0 || written >= path.len() {
        return Err(WorkspaceFilesystemError::ScopeDenied);
    }
    let canonical = OsString::from_wide(&path[..written]);
    if Path::new(&canonical).file_name() == Some(requested) {
        Ok(())
    } else {
        Err(WorkspaceFilesystemError::ScopeDenied)
    }
}

#[cfg(not(windows))]
fn ensure_exact_case_if_present(
    _dir: &Dir,
    _requested: &OsStr,
) -> Result<(), WorkspaceFilesystemError> {
    Ok(())
}

#[cfg(not(windows))]
fn ensure_opened_file_name(
    _file: &cap_std::fs::File,
    _requested: &OsStr,
) -> Result<(), WorkspaceFilesystemError> {
    Ok(())
}

#[cfg(not(windows))]
fn ensure_opened_directory_name(
    _dir: &Dir,
    _requested: &OsStr,
) -> Result<(), WorkspaceFilesystemError> {
    Ok(())
}

fn portable_permissions(metadata: &Metadata) -> PortableFilePermissions {
    let unix_mode = portable_unix_mode(metadata);
    PortableFilePermissions {
        read_only: unix_mode.map_or_else(
            || metadata.permissions().readonly(),
            |mode| mode & 0o222 == 0,
        ),
        executable: unix_mode.is_some_and(|mode| mode & 0o111 != 0),
        unix_mode,
    }
}

#[cfg(unix)]
fn portable_unix_mode(metadata: &Metadata) -> Option<u32> {
    use cap_std::fs::MetadataExt as _;
    Some(metadata.mode() & 0o7777)
}

#[cfg(not(unix))]
fn portable_unix_mode(_metadata: &Metadata) -> Option<u32> {
    None
}

#[cfg(unix)]
fn default_file_permissions() -> PortableFilePermissions {
    PortableFilePermissions {
        read_only: false,
        executable: false,
        unix_mode: Some(0o644),
    }
}

#[cfg(not(unix))]
fn default_file_permissions() -> PortableFilePermissions {
    PortableFilePermissions {
        read_only: false,
        executable: false,
        unix_mode: None,
    }
}

fn metadata_hash(kind: &str, metadata: &Metadata, extra: &[u8]) -> MetadataSha256 {
    let mut hasher = Sha256::new();
    hasher.update(b"dennett.workspace-metadata.v1\0");
    hasher.update(kind.as_bytes());
    hasher.update([u8::from(metadata.permissions().readonly())]);
    hasher.update(metadata.len().to_be_bytes());
    hasher.update(extra);
    MetadataSha256(hasher.finalize().into())
}

fn link_target_bytes(dir: &Dir, name: &OsStr) -> Result<Vec<u8>, WorkspaceFilesystemError> {
    let target = dir
        .read_link(name)
        .map_err(|_| WorkspaceFilesystemError::Conflict)?;
    Ok(os_string_bytes(target.as_os_str()))
}

#[cfg(unix)]
fn os_string_bytes(value: &OsStr) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt as _;
    value.as_bytes().to_vec()
}

#[cfg(windows)]
fn os_string_bytes(value: &OsStr) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt as _;
    value
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>()
}

#[cfg(not(any(unix, windows)))]
fn os_string_bytes(value: &OsStr) -> Vec<u8> {
    value.to_string_lossy().as_bytes().to_vec()
}

fn write_staged_file(
    parent: &Dir,
    name: &OsStr,
    bytes: &[u8],
    permissions: PortableFilePermissions,
) -> Result<(), WorkspaceFilesystemError> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .create_new(true)
        .follow(FollowSymlinks::No);
    configure_staged_open(&mut options);
    let mut file = parent
        .open_with(name, &options)
        .map_err(|source| map_publication_error(source, true))?;
    file.write_all(bytes)
        .map_err(|_| WorkspaceFilesystemError::AdapterUnavailable)?;
    set_portable_permissions(&file, permissions)?;
    file.sync_all()
        .map_err(|_| WorkspaceFilesystemError::AdapterUnavailable)
}

#[cfg(unix)]
fn configure_staged_open(options: &mut OpenOptions) {
    use cap_std::fs::OpenOptionsExt as _;
    options.mode(0o600).sync(true);
}

#[cfg(windows)]
fn configure_staged_open(options: &mut OpenOptions) {
    use cap_std::fs::OpenOptionsExt as _;
    use windows_sys::Win32::Storage::FileSystem::{
        DELETE, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE,
    };
    options
        .access_mode(FILE_GENERIC_READ | FILE_GENERIC_WRITE | DELETE)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .sync(true);
}

#[cfg(not(any(unix, windows)))]
fn configure_staged_open(_options: &mut OpenOptions) {}

fn set_portable_permissions(
    file: &cap_std::fs::File,
    permissions: PortableFilePermissions,
) -> Result<(), WorkspaceFilesystemError> {
    let mut mode = file
        .metadata()
        .map_err(|_| WorkspaceFilesystemError::AdapterUnavailable)?
        .permissions();
    set_permission_bits(&mut mode, permissions)?;
    file.set_permissions(mode)
        .map_err(|_| WorkspaceFilesystemError::AdapterUnavailable)
}

#[cfg(unix)]
fn set_permission_bits(
    mode: &mut cap_std::fs::Permissions,
    permissions: PortableFilePermissions,
) -> Result<(), WorkspaceFilesystemError> {
    permissions
        .validate()
        .map_err(|_| WorkspaceFilesystemError::RecoveryRequired)?;
    let bits = permissions
        .unix_mode
        .ok_or(WorkspaceFilesystemError::RecoveryRequired)?;
    mode.set_mode(bits);
    Ok(())
}

#[cfg(not(unix))]
fn set_permission_bits(
    mode: &mut cap_std::fs::Permissions,
    permissions: PortableFilePermissions,
) -> Result<(), WorkspaceFilesystemError> {
    permissions
        .validate()
        .map_err(|_| WorkspaceFilesystemError::RecoveryRequired)?;
    if permissions.unix_mode.is_some() {
        return Err(WorkspaceFilesystemError::RecoveryRequired);
    }
    mode.set_readonly(permissions.read_only);
    Ok(())
}

#[cfg(target_os = "linux")]
fn move_noreplace(
    source_dir: &Dir,
    source: &OsStr,
    target_dir: &Dir,
    target: &OsStr,
) -> Result<(), WorkspaceFilesystemError> {
    use rustix::fs::{RenameFlags, renameat_with};
    renameat_with(
        source_dir,
        source,
        target_dir,
        target,
        RenameFlags::NOREPLACE,
    )
    .map_err(|error| {
        map_publication_error(io::Error::from_raw_os_error(error.raw_os_error()), false)
    })
}

fn all_transitions_before(
    plan: &WorkspaceFileEffectPlan,
    observations: &[WorkspaceTransitionObservation],
) -> bool {
    observations.len() == plan.transitions.len()
        && plan.transitions.iter().all(|transition| {
            observations
                .iter()
                .find(|item| item.path == transition.path)
                .is_some_and(|item| item.state == transition.before)
        })
}

#[cfg(windows)]
fn move_noreplace(
    source_dir: &Dir,
    source: &OsStr,
    target_dir: &Dir,
    target: &OsStr,
) -> Result<(), WorkspaceFilesystemError> {
    use std::{mem, os::windows::ffi::OsStrExt as _, os::windows::io::AsRawHandle, ptr};
    use windows_sys::Win32::{
        Foundation::{HANDLE, RtlNtStatusToDosError},
        Storage::FileSystem::{
            DELETE, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_RENAME_INFO, FILE_SHARE_DELETE,
            FILE_SHARE_READ, FILE_SHARE_WRITE,
        },
    };

    #[repr(C)]
    struct IoStatusBlock {
        status_or_pointer: usize,
        information: usize,
    }

    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn NtSetInformationFile(
            file_handle: HANDLE,
            io_status_block: *mut IoStatusBlock,
            file_information: *const core::ffi::c_void,
            length: u32,
            file_information_class: i32,
        ) -> i32;
    }

    let mut options = OpenOptions::new();
    options.read(true).write(true).follow(FollowSymlinks::No);
    use cap_std::fs::OpenOptionsExt as _;
    options
        .access_mode(FILE_GENERIC_READ | FILE_GENERIC_WRITE | DELETE)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .sync(true);
    let file = source_dir
        .open_with(source, &options)
        .map_err(|source| map_publication_error(source, false))?;
    let target_wide = target.encode_wide().collect::<Vec<_>>();
    let header = mem::offset_of!(FILE_RENAME_INFO, FileName);
    let byte_len = header + target_wide.len() * mem::size_of::<u16>();
    let words = byte_len.div_ceil(mem::size_of::<usize>());
    let mut storage = vec![0usize; words];
    let info = storage.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    let mut io_status = IoStatusBlock {
        status_or_pointer: 0,
        information: 0,
    };
    // SAFETY: storage is aligned and sized for the fixed header plus the
    // copied UTF-16 target. Both capability handles outlive the syscall.
    unsafe {
        (*info).Anonymous.ReplaceIfExists = false;
        (*info).RootDirectory = target_dir.as_raw_handle().cast::<core::ffi::c_void>() as HANDLE;
        (*info).FileNameLength = u32::try_from(target_wide.len() * 2)
            .map_err(|_| WorkspaceFilesystemError::ScopeDenied)?;
        ptr::copy_nonoverlapping(
            target_wide.as_ptr(),
            ptr::addr_of_mut!((*info).FileName).cast::<u16>(),
            target_wide.len(),
        );
        let status = NtSetInformationFile(
            file.as_raw_handle().cast::<core::ffi::c_void>() as HANDLE,
            &mut io_status,
            info.cast(),
            u32::try_from(byte_len).map_err(|_| WorkspaceFilesystemError::ScopeDenied)?,
            10,
        );
        if status < 0 {
            return Err(map_publication_error(
                io::Error::from_raw_os_error(RtlNtStatusToDosError(status) as i32),
                false,
            ));
        }
    }
    file.sync_all()
        .map_err(|_| WorkspaceFilesystemError::AdapterUnavailable)
}

#[cfg(not(any(target_os = "linux", windows)))]
fn move_noreplace(
    _source_dir: &Dir,
    _source: &OsStr,
    _target_dir: &Dir,
    _target: &OsStr,
) -> Result<(), WorkspaceFilesystemError> {
    Err(WorkspaceFilesystemError::UnsupportedObject)
}

fn map_publication_error(source: io::Error, staging: bool) -> WorkspaceFilesystemError {
    if source.kind() == io::ErrorKind::AlreadyExists {
        if staging {
            WorkspaceFilesystemError::RecoveryRequired
        } else {
            WorkspaceFilesystemError::Conflict
        }
    } else if source.kind() == io::ErrorKind::NotFound {
        WorkspaceFilesystemError::Conflict
    } else if matches!(
        source.kind(),
        io::ErrorKind::InvalidInput
            | io::ErrorKind::NotADirectory
            | io::ErrorKind::PermissionDenied
    ) {
        WorkspaceFilesystemError::ScopeDenied
    } else {
        WorkspaceFilesystemError::AdapterUnavailable
    }
}

fn map_project_error(error: ProjectFolderError) -> WorkspaceFilesystemError {
    match error {
        ProjectFolderError::LocationMissing => WorkspaceFilesystemError::LocationMissing,
        ProjectFolderError::LinkedEntry | ProjectFolderError::InvalidRoot => {
            WorkspaceFilesystemError::ScopeDenied
        }
        ProjectFolderError::SourceIdentityChanged => WorkspaceFilesystemError::Conflict,
        ProjectFolderError::RootNotDirectory
        | ProjectFolderError::CreateRootNotEmpty
        | ProjectFolderError::PortableMetadataConflict
        | ProjectFolderError::InvalidProjectId
        | ProjectFolderError::IdentityUnchanged
        | ProjectFolderError::PostconditionFailed => WorkspaceFilesystemError::UnsupportedObject,
        ProjectFolderError::Io { .. } => WorkspaceFilesystemError::AdapterUnavailable,
    }
}

fn os_eq_ignore_ascii_case(value: &OsStr, expected: &str) -> bool {
    value
        .to_str()
        .is_some_and(|value| value.eq_ignore_ascii_case(expected))
}

fn hex_hash(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        project_fs::{OpenProjectRoot, directory_identity},
        project_location::encode_source_identity,
    };
    use dennett_contracts::{
        CheckpointId, CommandId, PortableMetadataAction, ProjectId, ProjectTrustState,
        WorkspaceBindingId, WorkspaceOperationId, WorkspaceSnapshotId,
    };
    use dennett_effect_core::workspace::{
        DurableCheckpointState, DurableWorkspaceOperationState, WorkspaceCheckpointRecord,
        WorkspaceFileEffectRequest, WorkspaceJournalPort, WorkspaceManifest,
        WorkspaceOperationRecord, WorkspaceStagingNonce,
    };
    use dennett_head::{
        project::{
            InspectProjectLocationCommand, ProjectApplication, RegisterProjectCommand,
            SetProjectTrustCommand,
        },
        session::SessionCoordinator,
        system::{SystemProjection, SystemSnapshot},
        workspace::{
            ApplyWorkspaceFileChangesCommand, CheckpointRestoreOutcome,
            CreateWorkspaceCheckpointCommand, RestoreWorkspaceCheckpointCommand,
            WorkspaceApplication, WorkspaceApplicationError,
        },
    };
    use dennett_memory_core::session::SessionJournal;
    use dennett_storage_sqlite::SqliteControlStore;
    use dennett_trust_core::project_registry::{
        BRIDGE_ATTESTED_PROJECT_TRUST_DECISION_KIND, ProjectRegistrationKind, TrustDecisionRef,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn path(value: &str) -> ProjectRelativePath {
        ProjectRelativePath::try_from(value).expect("valid project path")
    }

    fn scope(root: &Path) -> WorkspaceFilesystemScope {
        let opened = OpenProjectRoot::open(root).expect("open project root");
        let identity = directory_identity(&opened.dir).expect("project identity");
        WorkspaceFilesystemScope {
            project_id: ProjectId::new(),
            binding_id: WorkspaceBindingId::new(),
            absolute_path: root.to_string_lossy().into_owned(),
            source_identity: encode_source_identity(identity),
            writable: true,
        }
    }

    fn plan(
        scope: &WorkspaceFilesystemScope,
        observation: WorkspaceObservation,
        prepared: &PreparedWorkspaceFileEffect,
    ) -> WorkspaceFileEffectPlan {
        let revision = dennett_contracts::WorkspaceRevision::new(
            scope.binding_id,
            WorkspaceSnapshotId::new(),
            1,
        )
        .expect("workspace revision");
        let manifest = WorkspaceManifest::new(
            revision,
            observation.scope_sha256,
            observation.complete,
            observation.entries,
        )
        .expect("workspace manifest");
        WorkspaceFileEffectPlan::build(
            &manifest,
            WorkspaceFileEffectRequest {
                operation_id: WorkspaceOperationId::new(),
                command_id: CommandId::new(),
                correlation_id: "test.workspace_effect".to_owned(),
                project_id: scope.project_id,
                binding_id: scope.binding_id,
                base_revision: revision,
                intent_sha256: ContentSha256([9; 32]),
                staging_nonce: WorkspaceStagingNonce([8; 32]),
                safety_checkpoint_id: CheckpointId::new(),
                prepared_at_unix_ms: 1,
                changes: prepared.proposals.clone(),
            },
        )
        .expect("valid workspace effect plan")
    }

    fn stage(
        scope: &WorkspaceFilesystemScope,
        plan: &WorkspaceFileEffectPlan,
        prepared: &PreparedWorkspaceFileEffect,
    ) -> WorkspaceStagingReceipt {
        stage_file_effect_sync(scope, plan, &prepared.proposed_blobs)
            .expect("stage workspace effect")
    }

    fn change(
        kind: FileMutationKind,
        target: &str,
        previous: Option<&str>,
        content: Option<&[u8]>,
    ) -> WorkspaceFileChangeInput {
        WorkspaceFileChangeInput {
            kind,
            path: path(target),
            previous_path: previous.map(path),
            content: content.map(ToOwned::to_owned),
            expected_content_sha256: None,
            resulting_permissions: None,
        }
    }

    struct RealWorkspaceApplication {
        _temp: TempDir,
        root: PathBuf,
        application: WorkspaceApplication,
        projects: Arc<ProjectApplication>,
        store: Arc<SqliteControlStore>,
        project_id: ProjectId,
        binding_id: WorkspaceBindingId,
    }

    fn project_application(store: Arc<SqliteControlStore>) -> Arc<ProjectApplication> {
        let sessions = SessionCoordinator::new(SessionJournal::new(store.clone()), 1, 16);
        let system = Arc::new(SystemProjection::new(SystemSnapshot::empty(1), 16));
        Arc::new(ProjectApplication::new(
            store,
            Arc::new(crate::project_location::NodeProjectLocationAdapter::default()),
            sessions,
            system,
        ))
    }

    impl RealWorkspaceApplication {
        async fn open() -> Self {
            let temp = TempDir::new().expect("temporary workspace application");
            let root = temp.path().join("project");
            fs::create_dir(&root).expect("create project root");
            fs::write(root.join("tracked.txt"), b"original").expect("seed tracked file");
            fs::write(root.join("tracked-too.txt"), b"second original")
                .expect("seed second tracked file");
            fs::write(root.join("unrelated.txt"), b"before").expect("seed unrelated file");
            let store = Arc::new(
                SqliteControlStore::open(temp.path().join("control.sqlite"))
                    .await
                    .expect("open control store"),
            );
            let projects = project_application(store.clone());
            let inspection = projects
                .inspect_location(InspectProjectLocationCommand {
                    registration_kind: ProjectRegistrationKind::AttachExisting,
                    root_uri: root.to_string_lossy().into_owned(),
                    observed_at_unix_ms: 1,
                    expires_at_unix_ms: 60_001,
                })
                .await
                .expect("inspect project");
            let registered = projects
                .register_project(RegisterProjectCommand {
                    command_id: CommandId::new(),
                    correlation_id: "test.workspace.register".to_owned(),
                    intent_sha256: [3; 32],
                    inspection_id: inspection.inspection_id,
                    display_name: "Workspace test".to_owned(),
                    portable_metadata_action: PortableMetadataAction::LeaveAbsent,
                    initial_trust_state: None,
                    trust_decision: None,
                    committed_at_unix_ms: 2,
                })
                .await
                .expect("register project");
            let project_id = registered.project.project.project_id;
            let binding_id = registered.project.project.primary_binding_id;
            let trust_command_id = CommandId::new();
            projects
                .set_project_trust(SetProjectTrustCommand {
                    command_id: trust_command_id,
                    correlation_id: "test.workspace.trust".to_owned(),
                    project_id,
                    target_state: ProjectTrustState::TrustedBounded,
                    expected_policy_revision: registered.project.access_policy.revision,
                    trust_decision: TrustDecisionRef::new(
                        BRIDGE_ATTESTED_PROJECT_TRUST_DECISION_KIND,
                        trust_command_id.0.to_string(),
                    )
                    .expect("trust decision"),
                    committed_at_unix_ms: 3,
                })
                .await
                .expect("trust project");
            let application = WorkspaceApplication::new(
                projects.clone(),
                store.clone(),
                Arc::new(NodeWorkspaceFilesystemAdapter),
            );
            Self {
                _temp: temp,
                root,
                application,
                projects,
                store,
                project_id,
                binding_id,
            }
        }
    }

    #[test]
    fn applies_exact_multi_file_changes_and_removes_private_staging() {
        let temp = TempDir::new().expect("temporary project");
        fs::write(temp.path().join("modify.txt"), b"old modify").expect("seed modify");
        fs::write(temp.path().join("delete.txt"), b"old delete").expect("seed delete");
        fs::write(temp.path().join("rename.txt"), b"old rename").expect("seed rename");
        let scope = scope(temp.path());
        let prepared = prepare_file_effect_sync(
            &scope,
            vec![
                change(FileMutationKind::Add, "added.txt", None, Some(b"added")),
                change(
                    FileMutationKind::Modify,
                    "modify.txt",
                    None,
                    Some(b"modified"),
                ),
                change(FileMutationKind::Delete, "delete.txt", None, None),
                change(
                    FileMutationKind::Rename,
                    "renamed.txt",
                    Some("rename.txt"),
                    None,
                ),
            ],
        )
        .expect("prepare file effect");
        let plan = plan(&scope, prepared.observation.clone(), &prepared);
        let staging = stage(&scope, &plan, &prepared);

        let after = apply_file_effect_sync(&scope, &plan, &staging).expect("apply file effect");

        assert!(after.complete);
        assert_eq!(fs::read(temp.path().join("added.txt")).unwrap(), b"added");
        assert_eq!(
            fs::read(temp.path().join("modify.txt")).unwrap(),
            b"modified"
        );
        assert!(!temp.path().join("delete.txt").exists());
        assert!(!temp.path().join("rename.txt").exists());
        assert_eq!(
            fs::read(temp.path().join("renamed.txt")).unwrap(),
            b"old rename"
        );
        assert!(fs::read_dir(temp.path()).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".dennett-ws-")
        }));
    }

    #[test]
    fn legacy_sidecar_name_collision_is_preserved_as_user_content() {
        let temp = TempDir::new().expect("temporary project");
        let scope = scope(temp.path());
        let target_path = path("owned.txt");
        let prepared = prepare_file_effect_sync(
            &scope,
            vec![change(
                FileMutationKind::Add,
                target_path.as_str(),
                None,
                Some(b"agent"),
            )],
        )
        .expect("prepare add");
        let plan = plan(&scope, prepared.observation.clone(), &prepared);
        let digest = Sha256::digest(target_path.as_str().as_bytes());
        let collision_name = format!(
            ".dennett-ws-{}-{}-after.tmp",
            plan.operation_id.0,
            hex_hash(&digest[..8])
        );
        let collision = temp.path().join(collision_name);
        fs::write(&collision, b"agent").expect("seed legacy sidecar collision");

        let staging = stage(&scope, &plan, &prepared);
        apply_file_effect_sync(&scope, &plan, &staging).expect("apply add");

        assert_eq!(fs::read(temp.path().join("owned.txt")).unwrap(), b"agent");
        assert_eq!(
            fs::read(collision).unwrap(),
            b"agent",
            "an adjacent user file must never be inferred as operation-owned"
        );
    }

    #[test]
    fn staging_directory_collision_is_preserved_without_publication() {
        let temp = TempDir::new().expect("temporary project");
        let scope = scope(temp.path());
        let prepared = prepare_file_effect_sync(
            &scope,
            vec![change(
                FileMutationKind::Add,
                "owned.txt",
                None,
                Some(b"agent"),
            )],
        )
        .expect("prepare add");
        let plan = plan(&scope, prepared.observation.clone(), &prepared);
        let collision = temp.path().join(staging_directory_name(&plan));
        fs::create_dir(&collision).expect("seed staging directory collision");
        fs::write(collision.join("user.txt"), b"keep").expect("seed collision content");

        assert_eq!(
            stage_file_effect_sync(&scope, &plan, &prepared.proposed_blobs),
            Err(WorkspaceFilesystemError::RecoveryRequired)
        );
        assert_eq!(fs::read(collision.join("user.txt")).unwrap(), b"keep");
        assert!(!temp.path().join("owned.txt").exists());
    }

    #[test]
    fn replaced_staged_file_with_identical_bytes_is_not_published_or_deleted() {
        let temp = TempDir::new().expect("temporary project");
        let scope = scope(temp.path());
        let target_path = path("owned.txt");
        let prepared = prepare_file_effect_sync(
            &scope,
            vec![change(
                FileMutationKind::Add,
                target_path.as_str(),
                None,
                Some(b"agent"),
            )],
        )
        .expect("prepare add");
        let plan = plan(&scope, prepared.observation.clone(), &prepared);
        let staging = stage(&scope, &plan, &prepared);
        let staged_path = temp
            .path()
            .join(staging_directory_name(&plan))
            .join(staged_object_name(
                &target_path,
                WorkspaceStagedObjectKind::After,
            ));
        fs::remove_file(&staged_path).expect("remove owned staged file");
        fs::write(&staged_path, b"agent").expect("replace with same bytes");

        assert_eq!(
            apply_file_effect_sync(&scope, &plan, &staging),
            Err(WorkspaceFilesystemError::RecoveryRequired)
        );
        assert_eq!(fs::read(&staged_path).unwrap(), b"agent");
        assert!(!temp.path().join("owned.txt").exists());
    }

    #[test]
    fn touched_path_race_fails_before_any_publication() {
        let temp = TempDir::new().expect("temporary project");
        fs::write(temp.path().join("owned.txt"), b"base").expect("seed file");
        let scope = scope(temp.path());
        let prepared = prepare_file_effect_sync(
            &scope,
            vec![
                change(FileMutationKind::Modify, "owned.txt", None, Some(b"agent")),
                change(FileMutationKind::Add, "new.txt", None, Some(b"new")),
            ],
        )
        .expect("prepare file effect");
        let plan = plan(&scope, prepared.observation.clone(), &prepared);
        fs::write(temp.path().join("owned.txt"), b"human").expect("external edit");

        assert_eq!(
            stage_file_effect_sync(&scope, &plan, &prepared.proposed_blobs),
            Err(WorkspaceFilesystemError::Conflict)
        );
        assert_eq!(fs::read(temp.path().join("owned.txt")).unwrap(), b"human");
        assert!(!temp.path().join("new.txt").exists());
    }

    #[test]
    fn test_m02_filesystem_scope_001_rejects_linked_parent_without_external_write() {
        let temp = TempDir::new().expect("temporary scope test");
        let project = temp.path().join("project");
        let outside = temp.path().join("outside");
        fs::create_dir(&project).expect("create project");
        fs::create_dir(&outside).expect("create outside root");
        fs::write(outside.join("secret.txt"), b"outside").expect("seed outside file");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, project.join("escape"))
            .expect("create directory symlink");
        #[cfg(windows)]
        if std::os::windows::fs::symlink_dir(&outside, project.join("escape")).is_err() {
            return;
        }
        let scope = scope(&project);

        assert!(matches!(
            prepare_file_effect_sync(
                &scope,
                vec![change(
                    FileMutationKind::Modify,
                    "escape/secret.txt",
                    None,
                    Some(b"agent"),
                )],
            ),
            Err(WorkspaceFilesystemError::ScopeDenied | WorkspaceFilesystemError::Conflict)
        ));
        assert_eq!(fs::read(outside.join("secret.txt")).unwrap(), b"outside");
    }

    #[test]
    fn test_m02_filesystem_scope_001_rejects_a_different_root_for_the_granted_identity() {
        let first = TempDir::new().expect("first project");
        let second = TempDir::new().expect("second project");
        let mut mismatched_scope = scope(first.path());
        mismatched_scope.absolute_path = second.path().to_string_lossy().into_owned();

        assert_eq!(
            prepare_file_effect_sync(
                &mismatched_scope,
                vec![change(
                    FileMutationKind::Add,
                    "ungranted.txt",
                    None,
                    Some(b"agent"),
                )],
            ),
            Err(WorkspaceFilesystemError::Conflict)
        );
        assert!(!first.path().join("ungranted.txt").exists());
        assert!(!second.path().join("ungranted.txt").exists());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_m02_filesystem_scope_001_distinguishes_linux_mounts() {
        let root =
            Dir::open_ambient_dir("/", cap_std::ambient_authority()).expect("open filesystem root");
        let proc =
            Dir::open_ambient_dir("/proc", cap_std::ambient_authority()).expect("open proc mount");
        let root_mount = directory_mount_identity(&root).expect("root mount identity");
        let proc_mount = directory_mount_identity(&proc).expect("proc mount identity");

        assert_ne!(root_mount, proc_mount);
        assert_eq!(
            require_same_mount(root_mount, &proc),
            Err(WorkspaceFilesystemError::ScopeDenied)
        );

        let mut options = OpenOptions::new();
        options.read(true).follow(FollowSymlinks::No);
        let proc_file = proc
            .open_with("version", &options)
            .expect("open regular file on proc mount");
        let metadata = proc_file.metadata().expect("proc file metadata");
        assert_eq!(
            require_regular_file_scope(&root, &proc_file, &metadata),
            Err(WorkspaceFilesystemError::ScopeDenied)
        );
        assert_eq!(
            require_regular_file_scope(&proc, &proc_file, &metadata),
            Ok(())
        );
    }

    #[cfg(unix)]
    #[test]
    fn modifying_a_private_file_preserves_its_exact_unix_mode() {
        use std::os::unix::fs::PermissionsExt as _;

        let temp = TempDir::new().expect("temporary project");
        let target = temp.path().join("private.txt");
        fs::write(&target, b"private before").expect("seed private file");
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).expect("set private mode");
        let scope = scope(temp.path());
        let prepared = prepare_file_effect_sync(
            &scope,
            vec![change(
                FileMutationKind::Modify,
                "private.txt",
                None,
                Some(b"private after"),
            )],
        )
        .expect("prepare private file modification");
        assert_eq!(
            prepared.checkpoint_entries[0]
                .permissions
                .expect("checkpoint permissions")
                .unix_mode,
            Some(0o600)
        );
        let plan = plan(&scope, prepared.observation.clone(), &prepared);
        let staging = stage(&scope, &plan, &prepared);
        apply_file_effect_sync(&scope, &plan, &staging).expect("apply private file modification");

        assert_eq!(fs::read(&target).unwrap(), b"private after");
        assert_eq!(
            fs::metadata(target).unwrap().permissions().mode() & 0o7777,
            0o600
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn checkpoint_capture_fails_closed_for_extended_file_metadata() {
        let temp = TempDir::new().expect("temporary project");
        let target = temp.path().join("attributed.txt");
        fs::write(&target, b"human").expect("seed attributed file");
        let scope = scope(temp.path());
        let prepared = prepare_file_effect_sync(
            &scope,
            vec![change(
                FileMutationKind::Modify,
                "attributed.txt",
                None,
                Some(b"agent"),
            )],
        )
        .expect("prepare before external metadata race");
        let plan = plan(&scope, prepared.observation.clone(), &prepared);
        rustix::fs::setxattr(
            &target,
            "user.dennett-test",
            b"keep",
            rustix::fs::XattrFlags::empty(),
        )
        .expect("set test extended attribute");

        assert_eq!(
            stage_file_effect_sync(&scope, &plan, &prepared.proposed_blobs),
            Err(WorkspaceFilesystemError::UnsupportedObject)
        );
        assert_eq!(fs::read(target).unwrap(), b"human");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn publication_restores_a_file_when_metadata_changes_after_preflight() {
        let temp = TempDir::new().expect("temporary project");
        let target = temp.path().join("raced.txt");
        fs::write(&target, b"human").expect("seed raced file");
        let scope = scope(temp.path());
        let prepared = prepare_file_effect_sync(
            &scope,
            vec![change(
                FileMutationKind::Modify,
                "raced.txt",
                None,
                Some(b"agent"),
            )],
        )
        .expect("prepare before publication race");
        let plan = plan(&scope, prepared.observation.clone(), &prepared);
        let staging = stage(&scope, &plan, &prepared);
        let opened = open_scope(&scope).expect("open project scope");
        let staging_dir = open_staging_directory(&opened, &plan, &staging)
            .expect("open staged effect")
            .expect("staging directory exists");
        rustix::fs::setxattr(
            &target,
            "user.dennett-race",
            b"keep",
            rustix::fs::XattrFlags::empty(),
        )
        .expect("add metadata after preflight");

        assert_eq!(
            publish_staged_modify(
                &opened,
                &staging_dir,
                &plan,
                &staging,
                &path("raced.txt"),
                false,
            ),
            Err(WorkspaceFilesystemError::UnsupportedObject)
        );
        assert_eq!(fs::read(&target).unwrap(), b"human");
        assert!(
            target.exists(),
            "the original file must be restored in place"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn cleanup_preserves_a_backup_changed_after_publication_validation() {
        let temp = TempDir::new().expect("temporary project");
        let target_path = path("late-race.txt");
        let target = temp.path().join(target_path.as_str());
        fs::write(&target, b"human").expect("seed late-race file");
        let scope = scope(temp.path());
        let prepared = prepare_file_effect_sync(
            &scope,
            vec![change(
                FileMutationKind::Modify,
                target_path.as_str(),
                None,
                Some(b"agent"),
            )],
        )
        .expect("prepare late metadata race");
        let plan = plan(&scope, prepared.observation.clone(), &prepared);
        let staging = stage(&scope, &plan, &prepared);
        let opened = open_scope(&scope).expect("open project scope");
        let staging_dir = open_staging_directory(&opened, &plan, &staging)
            .expect("open staged effect")
            .expect("staging directory exists");
        let (parent, target_name) =
            open_parent(&opened.dir, &target_path).expect("open target parent");
        let backup = staged_object_name(&target_path, WorkspaceStagedObjectKind::Before);
        let after = staged_object_name(&target_path, WorkspaceStagedObjectKind::After);
        move_noreplace(&parent, &target_name, &staging_dir, &backup)
            .expect("move original to backup");
        let before_receipt =
            staged_receipt(&staging, &target_path, WorkspaceStagedObjectKind::Before)
                .expect("before receipt");
        verify_moved_backup(
            &staging_dir,
            &backup,
            &parent,
            &target_name,
            &transition_for(&plan, &target_path).unwrap().before,
            before_receipt.identity,
        )
        .expect("validate moved backup");
        move_noreplace(&staging_dir, &after, &parent, &target_name)
            .expect("publish staged replacement");
        let backup_path = temp
            .path()
            .join(staging_directory_name(&plan))
            .join(&backup);
        rustix::fs::setxattr(
            &backup_path,
            "user.dennett-late-race",
            b"keep",
            rustix::fs::XattrFlags::empty(),
        )
        .expect("change original after publication validation");

        assert_eq!(
            cleanup_staging(&opened, &plan, Some(&staging)),
            Err(WorkspaceFilesystemError::UnsupportedObject)
        );
        assert!(backup_path.exists(), "changed backup must not be deleted");
        assert_eq!(fs::read(target).unwrap(), b"agent");
    }

    #[cfg(windows)]
    #[test]
    fn test_m02_filesystem_scope_001_rejects_windows_case_aliases() {
        let temp = TempDir::new().expect("temporary project");
        fs::write(temp.path().join("Owned.txt"), b"human").expect("seed case-sensitive name");
        let scope = scope(temp.path());
        let opened = OpenProjectRoot::open(temp.path()).expect("open project root");
        assert_eq!(
            ensure_exact_case_if_present(&opened.dir, OsStr::new("owned.txt")),
            Err(WorkspaceFilesystemError::ScopeDenied)
        );
        fs::write(temp.path().join("Тест.txt"), b"unicode")
            .expect("seed Unicode case-sensitive name");
        assert_eq!(
            ensure_exact_case_if_present(&opened.dir, OsStr::new("тест.txt")),
            Err(WorkspaceFilesystemError::ScopeDenied)
        );

        assert!(matches!(
            prepare_file_effect_sync(
                &scope,
                vec![change(
                    FileMutationKind::Modify,
                    "owned.txt",
                    None,
                    Some(b"agent"),
                )],
            ),
            Err(WorkspaceFilesystemError::ScopeDenied | WorkspaceFilesystemError::Conflict)
        ));
        assert_eq!(fs::read(temp.path().join("Owned.txt")).unwrap(), b"human");
    }

    #[cfg(windows)]
    #[test]
    fn workspace_mutation_rejects_a_caller_assigned_short_name_for_git() {
        use std::os::windows::{
            ffi::{OsStrExt as _, OsStringExt as _},
            fs::OpenOptionsExt as _,
            io::AsRawHandle as _,
        };
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
            FILE_SHARE_WRITE, FILE_WRITE_ATTRIBUTES, GetShortPathNameW, SetFileShortNameW,
        };

        let temp = TempDir::new().expect("temporary project");
        let git = temp.path().join(".git");
        fs::create_dir_all(git.join("hooks")).expect("create protected Git directory");
        let directory = fs::OpenOptions::new()
            .access_mode(FILE_READ_ATTRIBUTES | FILE_WRITE_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(&git)
            .expect("open Git directory for short-name test");
        // SAFETY: the directory handle remains valid. A null short-name pointer
        // removes an automatically assigned alias before the custom alias is
        // attempted.
        let _ = unsafe { SetFileShortNameW(directory.as_raw_handle().cast(), std::ptr::null()) };
        let custom = ["GIT", "GITBOX", "DGIT0001"].into_iter().find(|candidate| {
            let short_name = format!("{candidate}\0").encode_utf16().collect::<Vec<_>>();
            // SAFETY: the directory handle remains valid and short_name is
            // NUL-terminated UTF-16 for the duration of the call.
            unsafe { SetFileShortNameW(directory.as_raw_handle().cast(), short_name.as_ptr()) != 0 }
        });
        let assigned = if let Some(custom) = custom {
            custom.to_owned()
        } else {
            let long_name = git
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect::<Vec<_>>();
            // SAFETY: long_name is a valid NUL-terminated UTF-16 path and the
            // null output buffer requests the required size.
            let required =
                unsafe { GetShortPathNameW(long_name.as_ptr(), std::ptr::null_mut(), 0) };
            if required == 0 {
                eprintln!("volume exposes no usable 8.3 alias; live test skipped");
                return;
            }
            let mut short_path = vec![0_u16; required as usize + 1];
            // SAFETY: short_path is writable for its declared capacity and the
            // API does not retain either pointer.
            let written = unsafe {
                GetShortPathNameW(
                    long_name.as_ptr(),
                    short_path.as_mut_ptr(),
                    short_path.len() as u32,
                )
            };
            let Some(alias) = PathBuf::from(OsString::from_wide(&short_path[..written as usize]))
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .filter(|name| !name.eq_ignore_ascii_case(".git"))
            else {
                eprintln!("volume exposes no distinct 8.3 alias; live test skipped");
                return;
            };
            alias
        };
        assert!(temp.path().join(&assigned).exists());
        let scope = scope(temp.path());
        let aliased_target = format!("{assigned}/hooks/agent.txt");

        assert_eq!(
            prepare_file_effect_sync(
                &scope,
                vec![change(
                    FileMutationKind::Add,
                    &aliased_target,
                    None,
                    Some(b"agent"),
                )],
            ),
            Err(WorkspaceFilesystemError::ScopeDenied)
        );
        assert!(!git.join("hooks").join("agent.txt").exists());
    }

    #[cfg(windows)]
    #[test]
    fn checkpoint_capture_fails_closed_for_alternate_data_streams() {
        let temp = TempDir::new().expect("temporary project");
        let target = temp.path().join("streamed.txt");
        let stream = PathBuf::from(format!("{}:dennett-test", target.to_string_lossy()));
        fs::write(&target, b"human").expect("seed primary stream");
        let scope = scope(temp.path());
        let prepared = prepare_file_effect_sync(
            &scope,
            vec![change(
                FileMutationKind::Modify,
                "streamed.txt",
                None,
                Some(b"agent"),
            )],
        )
        .expect("prepare before alternate stream race");
        let plan = plan(&scope, prepared.observation.clone(), &prepared);
        fs::write(&stream, b"preserve me").expect("seed alternate stream");

        assert_eq!(
            stage_file_effect_sync(&scope, &plan, &prepared.proposed_blobs),
            Err(WorkspaceFilesystemError::UnsupportedObject)
        );
        assert_eq!(fs::read(target).unwrap(), b"human");
        assert_eq!(fs::read(stream).unwrap(), b"preserve me");
    }

    #[cfg(windows)]
    #[test]
    fn workspace_mutation_rejects_a_directory_junction() {
        use std::process::Command;

        let temp = TempDir::new().expect("temporary project");
        let project = temp.path().join("project");
        let outside = temp.path().join("outside");
        let junction = project.join("junction");
        fs::create_dir(&project).expect("create project");
        fs::create_dir(&outside).expect("create outside");
        fs::write(outside.join("secret.txt"), b"human").expect("seed outside file");
        let output = Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(&junction)
            .arg(&outside)
            .output()
            .expect("create directory junction");
        assert!(
            output.status.success(),
            "mklink failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let scope = scope(&project);

        assert!(matches!(
            prepare_file_effect_sync(
                &scope,
                vec![change(
                    FileMutationKind::Modify,
                    "junction/secret.txt",
                    None,
                    Some(b"agent"),
                )],
            ),
            Err(WorkspaceFilesystemError::ScopeDenied | WorkspaceFilesystemError::Conflict)
        ));
        assert_eq!(fs::read(outside.join("secret.txt")).unwrap(), b"human");
        fs::remove_dir(junction).expect("remove junction without traversing it");
    }

    #[cfg(windows)]
    #[test]
    fn checkpoint_capture_fails_closed_for_explicit_windows_acl() {
        use std::process::Command;

        let temp = TempDir::new().expect("temporary project");
        let target = temp.path().join("acl.txt");
        fs::write(&target, b"human").expect("seed ACL file");
        let scope = scope(temp.path());
        let prepared = prepare_file_effect_sync(
            &scope,
            vec![change(
                FileMutationKind::Modify,
                "acl.txt",
                None,
                Some(b"agent"),
            )],
        )
        .expect("prepare before ACL race");
        let plan = plan(&scope, prepared.observation.clone(), &prepared);
        let output = Command::new("icacls")
            .arg(&target)
            .arg("/inheritance:d")
            .output()
            .expect("run built-in ACL editor");
        assert!(
            output.status.success(),
            "icacls failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        assert_eq!(
            stage_file_effect_sync(&scope, &plan, &prepared.proposed_blobs),
            Err(WorkspaceFilesystemError::UnsupportedObject)
        );
        assert_eq!(fs::read(target).unwrap(), b"human");
    }

    #[test]
    fn complete_snapshot_excludes_git_control_state() {
        let temp = TempDir::new().expect("temporary project");
        fs::create_dir(temp.path().join(".git")).expect("create git directory");
        fs::write(temp.path().join(".git").join("index"), b"private git state")
            .expect("seed git state");
        fs::create_dir_all(temp.path().join("vendor").join(".git"))
            .expect("create nested git directory");
        fs::write(
            temp.path().join("vendor").join(".git").join("config"),
            b"nested private git state",
        )
        .expect("seed nested git state");
        fs::write(temp.path().join("visible.txt"), b"visible").expect("seed project file");
        let observation = observe_workspace_sync(&scope(temp.path())).expect("observe workspace");

        assert!(observation.complete);
        assert!(
            observation
                .entries
                .iter()
                .any(|entry| entry.path == path("visible.txt"))
        );
        assert!(
            observation
                .entries
                .iter()
                .all(|entry| !entry.path.as_str().contains(".git"))
        );
    }

    #[test]
    fn checkpoint_rejects_a_before_image_larger_than_the_recovery_bound() {
        let temp = TempDir::new().expect("temporary project");
        fs::write(
            temp.path().join("large.bin"),
            vec![7_u8; usize::try_from(MAX_STAGED_FILE_BYTES + 1).unwrap()],
        )
        .expect("seed large file");
        let scope = scope(temp.path());

        assert_eq!(
            capture_checkpoint_sync(&scope, vec![path("large.bin")]),
            Err(WorkspaceFilesystemError::BoundExceeded)
        );
    }

    #[tokio::test]
    async fn checkpoint_restore_is_forward_only_and_preserves_unrelated_human_edits() {
        let real = RealWorkspaceApplication::open().await;
        let base = real
            .application
            .observe(
                real.project_id,
                real.binding_id,
                "test.workspace.observe".to_owned(),
            )
            .await
            .expect("observe base workspace");
        let checkpoint_id = CheckpointId::new();
        let checkpoint = real
            .application
            .create_checkpoint(CreateWorkspaceCheckpointCommand {
                checkpoint_id,
                project_id: real.project_id,
                binding_id: real.binding_id,
                base_revision: base.manifest.revision,
                correlation_id: "test.workspace.checkpoint".to_owned(),
                label: "Before edit".to_owned(),
                request_summary: "Restore tracked file only".to_owned(),
                touched_paths: vec![path("tracked.txt")],
                artifacts: vec![],
                external_effects: vec![],
                provider_continuation: None,
                created_at_unix_ms: 4,
            })
            .await
            .expect("create checkpoint");
        assert_eq!(checkpoint.captured_revision, base.manifest.revision);

        let applied = real
            .application
            .apply_file_changes(ApplyWorkspaceFileChangesCommand {
                operation_id: WorkspaceOperationId::new(),
                command_id: CommandId::new(),
                correlation_id: "test.workspace.apply".to_owned(),
                project_id: real.project_id,
                binding_id: real.binding_id,
                base_revision: base.manifest.revision,
                changes: vec![change(
                    FileMutationKind::Modify,
                    "tracked.txt",
                    None,
                    Some(b"agent change"),
                )],
                request_intent_sha256: None,
                prepared_at_unix_ms: 5,
            })
            .await
            .expect("apply tracked edit");
        let applied_revision = applied.resulting_revision.expect("resulting revision");
        fs::write(real.root.join("unrelated.txt"), b"human change")
            .expect("external unrelated edit");

        let restored = real
            .application
            .restore_checkpoint(RestoreWorkspaceCheckpointCommand {
                operation_id: WorkspaceOperationId::new(),
                command_id: CommandId::new(),
                correlation_id: "test.workspace.restore".to_owned(),
                project_id: real.project_id,
                binding_id: real.binding_id,
                checkpoint_id,
                expected_current_revision: applied_revision,
                prepared_at_unix_ms: 6,
            })
            .await
            .expect("restore checkpoint");
        assert!(matches!(restored, CheckpointRestoreOutcome::Applied(_)));
        assert_eq!(
            fs::read(real.root.join("tracked.txt")).unwrap(),
            b"original"
        );
        assert_eq!(
            fs::read(real.root.join("unrelated.txt")).unwrap(),
            b"human change"
        );
        let comparison = real
            .application
            .compare_checkpoint(
                real.project_id,
                real.binding_id,
                checkpoint_id,
                "test.workspace.compare".to_owned(),
            )
            .await
            .expect("compare checkpoint");
        assert_eq!(comparison.matching_paths, vec![path("tracked.txt")]);
        assert!(comparison.changed_paths.is_empty());
    }

    #[tokio::test]
    async fn test_m02_workspace_snapshot_001_reuses_exact_facts_and_rejects_stale_touched_state() {
        let real = RealWorkspaceApplication::open().await;
        let base = real
            .application
            .observe(
                real.project_id,
                real.binding_id,
                "test.workspace.snapshot.base".to_owned(),
            )
            .await
            .expect("observe base workspace");
        let unchanged = real
            .application
            .observe(
                real.project_id,
                real.binding_id,
                "test.workspace.snapshot.unchanged".to_owned(),
            )
            .await
            .expect("observe unchanged workspace");
        assert_eq!(unchanged.manifest.revision, base.manifest.revision);

        fs::write(real.root.join("tracked.txt"), b"human change")
            .expect("external touched-path change");
        let changed = real
            .application
            .observe(
                real.project_id,
                real.binding_id,
                "test.workspace.snapshot.changed".to_owned(),
            )
            .await
            .expect("observe changed workspace");
        assert_eq!(
            changed.manifest.revision.sequence(),
            base.manifest.revision.sequence() + 1
        );

        let result = real
            .application
            .apply_file_changes(ApplyWorkspaceFileChangesCommand {
                operation_id: WorkspaceOperationId::new(),
                command_id: CommandId::new(),
                correlation_id: "test.workspace.snapshot.stale".to_owned(),
                project_id: real.project_id,
                binding_id: real.binding_id,
                base_revision: base.manifest.revision,
                changes: vec![change(
                    FileMutationKind::Modify,
                    "tracked.txt",
                    None,
                    Some(b"agent change"),
                )],
                request_intent_sha256: None,
                prepared_at_unix_ms: 5,
            })
            .await;
        assert!(matches!(
            result,
            Err(WorkspaceApplicationError::Conflict(_))
        ));
        assert_eq!(
            fs::read(real.root.join("tracked.txt")).unwrap(),
            b"human change"
        );
    }

    #[tokio::test]
    async fn unrelated_external_change_is_preserved_in_the_resulting_revision() {
        let real = RealWorkspaceApplication::open().await;
        let base = real
            .application
            .observe(
                real.project_id,
                real.binding_id,
                "test.workspace.unrelated.base".to_owned(),
            )
            .await
            .expect("observe base workspace");

        fs::write(real.root.join("unrelated.txt"), b"human change")
            .expect("create unrelated human edit");
        let applied = real
            .application
            .apply_file_changes(ApplyWorkspaceFileChangesCommand {
                operation_id: WorkspaceOperationId::new(),
                command_id: CommandId::new(),
                correlation_id: "test.workspace.unrelated.apply".to_owned(),
                project_id: real.project_id,
                binding_id: real.binding_id,
                base_revision: base.manifest.revision,
                changes: vec![change(
                    FileMutationKind::Modify,
                    "tracked.txt",
                    None,
                    Some(b"agent change"),
                )],
                request_intent_sha256: None,
                prepared_at_unix_ms: 5,
            })
            .await
            .expect("apply against unchanged touched path");
        assert_eq!(applied.state, DurableWorkspaceOperationState::Succeeded);
        assert_eq!(
            fs::read(real.root.join("tracked.txt")).unwrap(),
            b"agent change"
        );
        assert_eq!(
            fs::read(real.root.join("unrelated.txt")).unwrap(),
            b"human change"
        );

        let resulting_revision = applied.resulting_revision.expect("resulting revision");
        let observed = real
            .application
            .observe(
                real.project_id,
                real.binding_id,
                "test.workspace.unrelated.result".to_owned(),
            )
            .await
            .expect("observe resulting workspace");
        assert_eq!(observed.manifest.revision, resulting_revision);
        assert_eq!(
            resulting_revision.sequence(),
            base.manifest.revision.sequence() + 2
        );
    }

    #[tokio::test]
    async fn test_m02_checkpoint_recovery_001_stops_on_touched_path_divergence() {
        let real = RealWorkspaceApplication::open().await;
        let base = real
            .application
            .observe(
                real.project_id,
                real.binding_id,
                "test.workspace.divergence.observe".to_owned(),
            )
            .await
            .expect("observe base workspace");
        let checkpoint_id = CheckpointId::new();
        real.application
            .create_checkpoint(CreateWorkspaceCheckpointCommand {
                checkpoint_id,
                project_id: real.project_id,
                binding_id: real.binding_id,
                base_revision: base.manifest.revision,
                correlation_id: "test.workspace.divergence.checkpoint".to_owned(),
                label: "Before agent edit".to_owned(),
                request_summary: "Protect human divergence".to_owned(),
                touched_paths: vec![path("tracked.txt")],
                artifacts: vec![],
                external_effects: vec![],
                provider_continuation: None,
                created_at_unix_ms: 4,
            })
            .await
            .expect("create checkpoint");
        let applied = real
            .application
            .apply_file_changes(ApplyWorkspaceFileChangesCommand {
                operation_id: WorkspaceOperationId::new(),
                command_id: CommandId::new(),
                correlation_id: "test.workspace.divergence.apply".to_owned(),
                project_id: real.project_id,
                binding_id: real.binding_id,
                base_revision: base.manifest.revision,
                changes: vec![change(
                    FileMutationKind::Modify,
                    "tracked.txt",
                    None,
                    Some(b"agent change"),
                )],
                request_intent_sha256: None,
                prepared_at_unix_ms: 5,
            })
            .await
            .expect("apply agent edit");
        fs::write(real.root.join("tracked.txt"), b"new human change")
            .expect("create touched-path divergence");

        let result = real
            .application
            .restore_checkpoint(RestoreWorkspaceCheckpointCommand {
                operation_id: WorkspaceOperationId::new(),
                command_id: CommandId::new(),
                correlation_id: "test.workspace.divergence.restore".to_owned(),
                project_id: real.project_id,
                binding_id: real.binding_id,
                checkpoint_id,
                expected_current_revision: applied.resulting_revision.unwrap(),
                prepared_at_unix_ms: 6,
            })
            .await;
        assert!(matches!(
            result,
            Err(WorkspaceApplicationError::Conflict(_))
        ));
        assert_eq!(
            fs::read(real.root.join("tracked.txt")).unwrap(),
            b"new human change"
        );
    }

    #[tokio::test]
    async fn interrupted_partial_publication_is_reconciled_after_restart_and_restorable() {
        let real = RealWorkspaceApplication::open().await;
        let base = real
            .application
            .observe(
                real.project_id,
                real.binding_id,
                "test.workspace.partial.observe".to_owned(),
            )
            .await
            .expect("observe base workspace");

        let opened = OpenProjectRoot::open(&real.root).expect("open registered workspace");
        let scope = WorkspaceFilesystemScope {
            project_id: real.project_id,
            binding_id: real.binding_id,
            absolute_path: real.root.to_string_lossy().into_owned(),
            source_identity: encode_source_identity(
                directory_identity(&opened.dir).expect("registered workspace identity"),
            ),
            writable: true,
        };
        let prepared = prepare_file_effect_sync(
            &scope,
            vec![
                change(
                    FileMutationKind::Modify,
                    "tracked.txt",
                    None,
                    Some(b"first agent change"),
                ),
                change(
                    FileMutationKind::Modify,
                    "tracked-too.txt",
                    None,
                    Some(b"second agent change"),
                ),
            ],
        )
        .expect("prepare interrupted operation");
        let checkpoint_id = CheckpointId::new();
        let plan = WorkspaceFileEffectPlan::build(
            &base.manifest,
            WorkspaceFileEffectRequest {
                operation_id: WorkspaceOperationId::new(),
                command_id: CommandId::new(),
                correlation_id: "test.workspace.partial.apply".to_owned(),
                project_id: real.project_id,
                binding_id: real.binding_id,
                base_revision: base.manifest.revision,
                intent_sha256: ContentSha256([9; 32]),
                staging_nonce: WorkspaceStagingNonce([7; 32]),
                safety_checkpoint_id: checkpoint_id,
                prepared_at_unix_ms: 5,
                changes: prepared.proposals,
            },
        )
        .expect("build interrupted operation");
        let checkpoint = WorkspaceCheckpointRecord {
            checkpoint_id,
            project_id: real.project_id,
            binding_id: real.binding_id,
            base_revision: base.manifest.revision,
            captured_revision: base.manifest.revision,
            state: DurableCheckpointState::Available,
            label: "Automatic safety checkpoint".to_owned(),
            request_summary: "Before interrupted test mutation".to_owned(),
            entries: prepared.checkpoint_entries,
            artifacts: vec![],
            external_effects: vec![],
            provider_continuation: None,
            created_at_unix_ms: 5,
        };
        let operation = WorkspaceOperationRecord {
            plan,
            staging: None,
            state: DurableWorkspaceOperationState::Prepared,
            resulting_revision: None,
            failure: None,
            completed_at_unix_ms: None,
        };
        let mut blobs = prepared.proposed_blobs;
        blobs.extend(prepared.checkpoint_blobs);
        let operation = real
            .store
            .prepare_file_effect(operation.clone(), checkpoint, blobs.clone())
            .await
            .expect("persist prepared operation");
        let staging = stage_file_effect_sync(&scope, &operation.plan, &blobs)
            .expect("stage interrupted operation");
        let mut operation = operation;
        operation.staging = Some(staging.clone());
        let operation = real
            .store
            .transition_operation(DurableWorkspaceOperationState::Prepared, operation, None)
            .await
            .expect("persist staging receipts");
        assert_eq!(
            apply_file_effect_sync_with_failure(
                &scope,
                &operation.plan,
                &staging,
                Some(PublicationFailure::AfterModifyBackup(1)),
            ),
            Err(WorkspaceFilesystemError::RecoveryRequired)
        );

        assert_eq!(
            fs::read(real.root.join("tracked.txt")).unwrap(),
            b"first agent change"
        );
        assert!(
            optional_symlink_metadata(
                &OpenProjectRoot::open(&real.root)
                    .expect("open interrupted workspace")
                    .dir,
                OsStr::new("tracked-too.txt"),
            )
            .expect("inspect interrupted target")
            .is_none()
        );
        fs::write(real.root.join("unrelated.txt"), b"human change")
            .expect("external unrelated edit");

        let database = real._temp.path().join("control.sqlite");
        let root = real.root.clone();
        let project_id = real.project_id;
        let binding_id = real.binding_id;
        drop(real.application);
        drop(real.projects);
        real.store.close().await;
        drop(real.store);

        let store = Arc::new(
            SqliteControlStore::open(&database)
                .await
                .expect("reopen control store after simulated process exit"),
        );
        let projects = project_application(store.clone());
        let restarted =
            WorkspaceApplication::new(projects, store, Arc::new(NodeWorkspaceFilesystemAdapter));
        let reconciled = restarted
            .reconcile_unfinished()
            .await
            .expect("reconcile after simulated restart");
        assert_eq!(reconciled.len(), 1);
        assert_eq!(
            reconciled[0].state,
            DurableWorkspaceOperationState::RecoveryRequired
        );

        let restored = restarted
            .restore_checkpoint(RestoreWorkspaceCheckpointCommand {
                operation_id: WorkspaceOperationId::new(),
                command_id: CommandId::new(),
                correlation_id: "test.workspace.partial.restore".to_owned(),
                project_id,
                binding_id,
                checkpoint_id,
                expected_current_revision: base.manifest.revision,
                prepared_at_unix_ms: 6,
            })
            .await
            .expect("restore partial operation");

        assert!(matches!(restored, CheckpointRestoreOutcome::Applied(_)));
        assert_eq!(fs::read(root.join("tracked.txt")).unwrap(), b"original");
        assert_eq!(
            fs::read(root.join("tracked-too.txt")).unwrap(),
            b"second original"
        );
        assert_eq!(
            fs::read(root.join("unrelated.txt")).unwrap(),
            b"human change"
        );
        assert!(fs::read_dir(&root).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".dennett-ws-")
        }));
    }
}
