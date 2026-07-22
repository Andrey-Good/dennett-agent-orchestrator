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
    PreparedWorkspaceFileEffect, WorkspaceFileChangeInput, WorkspaceFilesystemError,
    WorkspaceFilesystemPort, WorkspaceFilesystemScope, WorkspaceObservation,
    WorkspaceTransitionObservation,
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
    let mut state = SnapshotScanState::default();
    scan_directory(&opened.dir, "", 0, root_identity.volume, &mut state)?;
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
        if parent.is_empty() && os_eq_ignore_ascii_case(&name, ".git") {
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
            let directory_state = WorkspacePathState::Directory {
                metadata_sha256: metadata_hash("directory", &metadata, &[]),
            };
            state.entries.push(WorkspaceManifestEntry {
                path,
                state: directory_state,
            });
            scan_directory(&child, &relative, depth + 1, root_volume, state)?;
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
            FileMutationKind::Add => Some(PortableFilePermissions {
                read_only: false,
                executable: false,
            }),
            FileMutationKind::Modify => Some(permissions_from_state_and_path(
                &opened,
                &change.path,
                &current,
            )?),
            FileMutationKind::Delete | FileMutationKind::Rename => None,
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
    if !scope.writable || scope.project_id != plan.project_id || scope.binding_id != plan.binding_id
    {
        return Err(WorkspaceFilesystemError::ScopeDenied);
    }
    let opened = open_scope(scope)?;
    let blob_map = validated_blob_map(blobs)?;
    preflight_transitions(&opened, plan)?;
    stage_after_images(&opened, plan, &blob_map)?;

    for change in &plan.changes {
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

fn validated_blob_map<'a>(
    blobs: &'a [WorkspaceBlob],
) -> Result<BTreeMap<&'a str, &'a [u8]>, WorkspaceFilesystemError> {
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
        current = child;
    }
    Err(WorkspaceFilesystemError::ScopeDenied)
}

#[cfg(windows)]
fn ensure_exact_case_if_present(
    dir: &Dir,
    requested: &OsStr,
) -> Result<(), WorkspaceFilesystemError> {
    let requested_text = requested
        .to_str()
        .ok_or(WorkspaceFilesystemError::ScopeDenied)?;
    for entry in dir
        .entries()
        .map_err(|_| WorkspaceFilesystemError::AdapterUnavailable)?
    {
        let name = entry
            .map_err(|_| WorkspaceFilesystemError::AdapterUnavailable)?
            .file_name();
        let Some(name_text) = name.to_str() else {
            continue;
        };
        if name_text.eq_ignore_ascii_case(requested_text) && name_text != requested_text {
            return Err(WorkspaceFilesystemError::ScopeDenied);
        }
    }
    Ok(())
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
