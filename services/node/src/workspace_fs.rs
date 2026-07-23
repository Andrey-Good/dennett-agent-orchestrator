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
use dennett_contracts::{ProjectRelativePath, WorkspaceOperationId};
use dennett_effect_core::workspace::{
    ContentSha256, FileMutationKind, MAX_STAGED_FILE_BYTES, MetadataSha256,
    PortableFilePermissions, ResolvedFileChangeProposal, WorkspaceBlob, WorkspaceCheckpointEntry,
    WorkspaceFileEffectPlan, WorkspaceManifestEntry, WorkspacePathState,
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
        blobs: &[WorkspaceBlob],
    ) -> Result<WorkspaceObservation, WorkspaceFilesystemError> {
        let scope = scope.clone();
        let plan = plan.clone();
        let blobs = blobs.to_vec();
        tokio::task::spawn_blocking(move || apply_file_effect_sync(&scope, &plan, &blobs))
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
    ) -> Result<(), WorkspaceFilesystemError> {
        let scope = scope.clone();
        let plan = plan.clone();
        tokio::task::spawn_blocking(move || {
            let opened = open_scope(&scope)?;
            let observations = observe_transitions_opened(&opened, &plan)?;
            if !all_transitions_after(&plan, &observations) {
                return Err(WorkspaceFilesystemError::RecoveryRequired);
            }
            cleanup_sidecars(&opened, &plan)?;
            opened.revalidate_location().map_err(map_project_error)
        })
        .await
        .map_err(|_| WorkspaceFilesystemError::AdapterUnavailable)?
    }

    async fn cleanup_unapplied_file_effect(
        &self,
        scope: &WorkspaceFilesystemScope,
        plan: &WorkspaceFileEffectPlan,
    ) -> Result<(), WorkspaceFilesystemError> {
        let scope = scope.clone();
        let plan = plan.clone();
        tokio::task::spawn_blocking(move || {
            let opened = open_scope(&scope)?;
            let observations = observe_transitions_opened(&opened, &plan)?;
            if !all_transitions_before(&plan, &observations) {
                return Err(WorkspaceFilesystemError::RecoveryRequired);
            }
            cleanup_sidecars(&opened, &plan)?;
            opened.revalidate_location().map_err(map_project_error)
        })
        .await
        .map_err(|_| WorkspaceFilesystemError::AdapterUnavailable)?
    }

    async fn cleanup_recovery_file_effect(
        &self,
        scope: &WorkspaceFilesystemScope,
        plan: &WorkspaceFileEffectPlan,
    ) -> Result<(), WorkspaceFilesystemError> {
        let scope = scope.clone();
        let plan = plan.clone();
        tokio::task::spawn_blocking(move || {
            let opened = open_scope(&scope)?;
            let observations = observe_transitions_opened(&opened, &plan)?;
            if !all_transitions_recognized(&plan, &observations) {
                return Err(WorkspaceFilesystemError::Conflict);
            }
            cleanup_sidecars(&opened, &plan)?;
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
            read_regular_state(dir, &name, &mut state.hashed_bytes, false)?.0
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

    for (index, change) in changes.into_iter().enumerate() {
        let current = manifest
            .get(change.path.as_str())
            .cloned()
            .cloned()
            .unwrap_or(WorkspacePathState::Absent);
        let content = match change.content {
            Some(bytes) => {
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
            FileMutationKind::Add => {
                change
                    .resulting_permissions
                    .or(Some(PortableFilePermissions {
                        read_only: false,
                        executable: false,
                    }))
            }
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
        capture_checkpoint_entry(&opened, &path, expected, &mut entries, &mut blobs)?;
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
                    blobs.insert(id, blob);
                }
            }
            Some(reference)
        }
        None => None,
    };
    entries.insert(
        path.as_str().to_owned(),
        WorkspaceCheckpointEntry {
            path: path.clone(),
            state: observed,
            content,
        },
    );
    Ok(())
}

fn apply_file_effect_sync(
    scope: &WorkspaceFilesystemScope,
    plan: &WorkspaceFileEffectPlan,
    blobs: &[WorkspaceBlob],
) -> Result<WorkspaceObservation, WorkspaceFilesystemError> {
    apply_file_effect_sync_with_failure(scope, plan, blobs, None)
}

fn apply_file_effect_sync_with_failure(
    scope: &WorkspaceFilesystemScope,
    plan: &WorkspaceFileEffectPlan,
    blobs: &[WorkspaceBlob],
    fail_after_publications: Option<usize>,
) -> Result<WorkspaceObservation, WorkspaceFilesystemError> {
    if !scope.writable || scope.project_id != plan.project_id || scope.binding_id != plan.binding_id
    {
        return Err(WorkspaceFilesystemError::ScopeDenied);
    }
    let opened = open_scope(scope)?;
    let blob_map = validated_blob_map(blobs)?;
    preflight_transitions(&opened, plan)?;
    stage_after_images(&opened, plan, &blob_map)?;

    for (published, change) in plan.changes.iter().enumerate() {
        if fail_after_publications == Some(published) {
            return Err(WorkspaceFilesystemError::RecoveryRequired);
        }
        let result = match change.kind {
            FileMutationKind::Add => publish_staged_add(&opened, plan, &change.path),
            FileMutationKind::Modify => publish_staged_modify(&opened, plan, &change.path),
            FileMutationKind::Delete => publish_delete(&opened, plan, &change.path),
            FileMutationKind::Rename => publish_rename(
                &opened,
                plan,
                change
                    .previous_path
                    .as_ref()
                    .ok_or(WorkspaceFilesystemError::RecoveryRequired)?,
                &change.path,
            ),
        };
        if result.is_err() {
            // Sidecars intentionally remain for deterministic reconciliation.
            return Err(WorkspaceFilesystemError::RecoveryRequired);
        }
    }

    let observations = observe_transitions_opened(&opened, plan)?;
    if !all_transitions_after(plan, &observations) {
        return Err(WorkspaceFilesystemError::RecoveryRequired);
    }
    cleanup_sidecars(&opened, plan)?;
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
    for blob in blobs {
        blob.validate()
            .map_err(|_| WorkspaceFilesystemError::RecoveryRequired)?;
        if let Some(existing) =
            result.insert(blob.reference.content_id.as_str(), blob.bytes.as_slice())
            && existing != blob.bytes.as_slice()
        {
            return Err(WorkspaceFilesystemError::RecoveryRequired);
        }
    }
    Ok(result)
}

fn preflight_transitions(
    opened: &OpenProjectRoot,
    plan: &WorkspaceFileEffectPlan,
) -> Result<(), WorkspaceFilesystemError> {
    for transition in &plan.transitions {
        let (observed, _) = state_at(&opened.dir, &transition.path, false)?;
        if observed != transition.before {
            return Err(WorkspaceFilesystemError::Conflict);
        }
    }
    opened.revalidate_location().map_err(map_project_error)
}

fn stage_after_images(
    opened: &OpenProjectRoot,
    plan: &WorkspaceFileEffectPlan,
    blobs: &BTreeMap<&str, &[u8]>,
) -> Result<(), WorkspaceFilesystemError> {
    let mut staged: Vec<(Dir, OsString)> = Vec::new();
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
        let (parent, _) = open_parent(&opened.dir, &transition.path)?;
        let temporary = sidecar_name(plan.operation_id, &transition.path, SidecarKind::After);
        if optional_symlink_metadata(&parent, &temporary)
            .map_err(map_project_error)?
            .is_some()
        {
            return Err(WorkspaceFilesystemError::RecoveryRequired);
        }
        if let Err(error) = write_staged_file(&parent, &temporary, bytes, permissions) {
            for (parent, temporary) in staged {
                let _ = parent.remove_file(&temporary);
            }
            return Err(error);
        }
        staged.push((parent, temporary));
    }
    Ok(())
}

fn publish_staged_add(
    opened: &OpenProjectRoot,
    plan: &WorkspaceFileEffectPlan,
    path: &ProjectRelativePath,
) -> Result<(), WorkspaceFilesystemError> {
    let (parent, target) = open_parent(&opened.dir, path)?;
    let temporary = sidecar_name(plan.operation_id, path, SidecarKind::After);
    parent
        .hard_link(&temporary, &parent, &target)
        .map_err(|source| map_publication_error(source, false))?;
    parent
        .remove_file(&temporary)
        .map_err(|_| WorkspaceFilesystemError::RecoveryRequired)?;
    sync_directory(&parent, "sync_workspace_add").map_err(map_project_error)
}

fn publish_staged_modify(
    opened: &OpenProjectRoot,
    plan: &WorkspaceFileEffectPlan,
    path: &ProjectRelativePath,
) -> Result<(), WorkspaceFilesystemError> {
    let (parent, target) = open_parent(&opened.dir, path)?;
    let backup = sidecar_name(plan.operation_id, path, SidecarKind::Before);
    move_noreplace(&parent, &target, &backup)?;
    let transition = transition_for(plan, path)?;
    let (backed_up, _) = state_at_parent(&parent, &backup, false)?;
    if backed_up != transition.before {
        let _ = move_noreplace(&parent, &backup, &target);
        return Err(WorkspaceFilesystemError::Conflict);
    }
    let temporary = sidecar_name(plan.operation_id, path, SidecarKind::After);
    if let Err(error) = move_noreplace(&parent, &temporary, &target) {
        let _ = move_noreplace(&parent, &backup, &target);
        return Err(error);
    }
    sync_directory(&parent, "sync_workspace_modify").map_err(map_project_error)
}

fn publish_delete(
    opened: &OpenProjectRoot,
    plan: &WorkspaceFileEffectPlan,
    path: &ProjectRelativePath,
) -> Result<(), WorkspaceFilesystemError> {
    let (parent, target) = open_parent(&opened.dir, path)?;
    let backup = sidecar_name(plan.operation_id, path, SidecarKind::Before);
    move_noreplace(&parent, &target, &backup)?;
    let transition = transition_for(plan, path)?;
    let (backed_up, _) = state_at_parent(&parent, &backup, false)?;
    if backed_up != transition.before {
        let _ = move_noreplace(&parent, &backup, &target);
        return Err(WorkspaceFilesystemError::Conflict);
    }
    sync_directory(&parent, "sync_workspace_delete").map_err(map_project_error)
}

fn publish_rename(
    opened: &OpenProjectRoot,
    plan: &WorkspaceFileEffectPlan,
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
    let backup = sidecar_name(plan.operation_id, source, SidecarKind::Before);
    move_noreplace(&source_parent, &source_name, &backup)?;
    let source_transition = transition_for(plan, source)?;
    let (backed_up, _) = state_at_parent(&source_parent, &backup, false)?;
    if backed_up != source_transition.before {
        let _ = move_noreplace(&source_parent, &backup, &source_name);
        return Err(WorkspaceFilesystemError::Conflict);
    }
    if let Err(error) = source_parent.hard_link(&backup, &target_parent, &target_name) {
        let _ = move_noreplace(&source_parent, &backup, &source_name);
        return Err(map_publication_error(error, false));
    }
    sync_directory(&source_parent, "sync_workspace_rename_source").map_err(map_project_error)?;
    sync_directory(&target_parent, "sync_workspace_rename_target").map_err(map_project_error)
}

fn cleanup_sidecars(
    opened: &OpenProjectRoot,
    plan: &WorkspaceFileEffectPlan,
) -> Result<(), WorkspaceFilesystemError> {
    for transition in &plan.transitions {
        let (parent, _) = open_parent(&opened.dir, &transition.path)?;
        for kind in [SidecarKind::After, SidecarKind::Before] {
            let name = sidecar_name(plan.operation_id, &transition.path, kind);
            let Some(_) = optional_symlink_metadata(&parent, &name).map_err(map_project_error)?
            else {
                continue;
            };
            let (state, _) = state_at_parent(&parent, &name, false)?;
            let expected = match kind {
                SidecarKind::After => &transition.after,
                SidecarKind::Before => &transition.before,
            };
            if state != *expected {
                return Err(WorkspaceFilesystemError::RecoveryRequired);
            }
            parent
                .remove_file(&name)
                .map_err(|_| WorkspaceFilesystemError::RecoveryRequired)?;
            sync_directory(&parent, "sync_workspace_sidecar_cleanup").map_err(map_project_error)?;
        }
    }
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

#[derive(Clone, Copy)]
enum SidecarKind {
    Before,
    After,
}

fn sidecar_name(
    operation_id: WorkspaceOperationId,
    path: &ProjectRelativePath,
    kind: SidecarKind,
) -> OsString {
    let digest = Sha256::digest(path.as_str().as_bytes());
    let suffix = match kind {
        SidecarKind::Before => "before",
        SidecarKind::After => "after",
    };
    OsString::from(format!(
        ".dennett-ws-{}-{}-{suffix}.tmp",
        operation_id.0,
        hex_hash(&digest[..8])
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

fn state_at_parent(
    parent: &Dir,
    name: &OsStr,
    capture_bytes: bool,
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
    read_regular_state(parent, name, &mut ignored_total, capture_bytes)
}

fn read_regular_state(
    parent: &Dir,
    name: &OsStr,
    total_bytes: &mut u64,
    capture_bytes: bool,
) -> Result<(WorkspacePathState, Option<Vec<u8>>), WorkspaceFilesystemError> {
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let mut file = parent
        .open_with(name, &options)
        .map_err(|_| WorkspaceFilesystemError::Conflict)?;
    let before = file
        .metadata()
        .map_err(|_| WorkspaceFilesystemError::Conflict)?;
    if !before.is_file() || before.len() > MAX_SNAPSHOT_FILE_BYTES {
        return Err(WorkspaceFilesystemError::BoundExceeded);
    }
    if capture_bytes && before.len() > MAX_STAGED_FILE_BYTES {
        return Err(WorkspaceFilesystemError::BoundExceeded);
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

#[cfg(not(windows))]
fn ensure_exact_case_if_present(
    _dir: &Dir,
    _requested: &OsStr,
) -> Result<(), WorkspaceFilesystemError> {
    Ok(())
}

fn portable_permissions(metadata: &Metadata) -> PortableFilePermissions {
    PortableFilePermissions {
        read_only: metadata.permissions().readonly(),
        executable: metadata_is_executable(metadata),
    }
}

#[cfg(unix)]
fn metadata_is_executable(metadata: &Metadata) -> bool {
    use cap_std::fs::MetadataExt as _;
    metadata.mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn metadata_is_executable(_metadata: &Metadata) -> bool {
    false
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
    set_permission_bits(&mut mode, permissions);
    file.set_permissions(mode)
        .map_err(|_| WorkspaceFilesystemError::AdapterUnavailable)
}

#[cfg(unix)]
fn set_permission_bits(mode: &mut cap_std::fs::Permissions, permissions: PortableFilePermissions) {
    let mut bits = if permissions.executable { 0o755 } else { 0o644 };
    if permissions.read_only {
        bits &= !0o222;
    }
    mode.set_mode(bits);
}

#[cfg(not(unix))]
fn set_permission_bits(mode: &mut cap_std::fs::Permissions, permissions: PortableFilePermissions) {
    mode.set_readonly(permissions.read_only);
}

#[cfg(target_os = "linux")]
fn move_noreplace(
    dir: &Dir,
    source: &OsStr,
    target: &OsStr,
) -> Result<(), WorkspaceFilesystemError> {
    use rustix::fs::{RenameFlags, renameat_with};
    renameat_with(dir, source, dir, target, RenameFlags::NOREPLACE).map_err(|error| {
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
    dir: &Dir,
    source: &OsStr,
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
    let file = dir
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
        (*info).RootDirectory = dir.as_raw_handle().cast::<core::ffi::c_void>() as HANDLE;
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
    _dir: &Dir,
    _source: &OsStr,
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
        WorkspaceOperationRecord,
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
                safety_checkpoint_id: CheckpointId::new(),
                prepared_at_unix_ms: 1,
                changes: prepared.proposals.clone(),
            },
        )
        .expect("valid workspace effect plan")
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
            let sessions = SessionCoordinator::new(SessionJournal::new(store.clone()), 1, 16);
            let system = Arc::new(SystemProjection::new(SystemSnapshot::empty(1), 16));
            let projects = Arc::new(ProjectApplication::new(
                store.clone(),
                Arc::new(crate::project_location::NodeProjectLocationAdapter::default()),
                sessions,
                system,
            ));
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
    fn applies_exact_multi_file_changes_and_removes_private_sidecars() {
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

        let after = apply_file_effect_sync(&scope, &plan, &prepared.proposed_blobs)
            .expect("apply file effect");

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
            apply_file_effect_sync(&scope, &plan, &prepared.proposed_blobs),
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
            state: DurableWorkspaceOperationState::Prepared,
            resulting_revision: None,
            failure: None,
            completed_at_unix_ms: None,
        };
        let mut blobs = prepared.proposed_blobs;
        blobs.extend(prepared.checkpoint_blobs);
        real.store
            .prepare_file_effect(operation.clone(), checkpoint, blobs.clone())
            .await
            .expect("persist prepared operation");
        assert_eq!(
            apply_file_effect_sync_with_failure(&scope, &operation.plan, &blobs, Some(1)),
            Err(WorkspaceFilesystemError::RecoveryRequired)
        );

        assert_eq!(
            fs::read(real.root.join("tracked.txt")).unwrap(),
            b"first agent change"
        );
        assert_eq!(
            fs::read(real.root.join("tracked-too.txt")).unwrap(),
            b"second original"
        );
        fs::write(real.root.join("unrelated.txt"), b"human change")
            .expect("external unrelated edit");

        let restarted = WorkspaceApplication::new(
            real.projects.clone(),
            real.store.clone(),
            Arc::new(NodeWorkspaceFilesystemAdapter),
        );
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
                project_id: real.project_id,
                binding_id: real.binding_id,
                checkpoint_id,
                expected_current_revision: base.manifest.revision,
                prepared_at_unix_ms: 6,
            })
            .await
            .expect("restore partial operation");

        assert!(matches!(restored, CheckpointRestoreOutcome::Applied(_)));
        assert_eq!(
            fs::read(real.root.join("tracked.txt")).unwrap(),
            b"original"
        );
        assert_eq!(
            fs::read(real.root.join("tracked-too.txt")).unwrap(),
            b"second original"
        );
        assert_eq!(
            fs::read(real.root.join("unrelated.txt")).unwrap(),
            b"human change"
        );
        assert!(fs::read_dir(&real.root).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".dennett-ws-")
        }));
    }
}
