//! Boundary-enforced, symlink/reparse-point-safe workspace file listing (`GET /v1/projects/{id}/files`).
//!
//! Security guarantees:
//! - Rejects `..` traversal, NUL bytes, drive overrides, and UNC/device paths (`\\?\UNC\`, `\\.\`, `\\host\`).
//! - Calls `fs::symlink_metadata` BEFORE descending into any root, subpath segment, or child entry,
//!   rejecting or skipping symlinks AND Windows NTFS reparse points / junctions (`0x0400`).
//! - Verifies canonical root containment (`canonical_target.starts_with(&canonical_root)`).
//! - Enforces strict anti-DoS caps (`MAX_LISTING_DEPTH`, `MAX_LISTING_ENTRIES`, `MAX_VISITED_FS_NODES`)
//!   and skips bulky technical directories (`.git`, `node_modules`, `target`, `.venv`).

use std::collections::VecDeque;
use std::fs;
use std::path::{Component, Path, PathBuf};

use dennett_contracts::{
    DennettError, ListProjectFilesResponse, ProjectId, WorkspaceFileEntry, WorkspaceFileKind,
};

/// Maximum recursion depth below the listed directory.
pub const MAX_LISTING_DEPTH: u32 = 3;
/// Maximum number of file/directory entries returned in one response.
pub const MAX_LISTING_ENTRIES: usize = 200;
/// Maximum total filesystem entries inspected before truncating (prevents `DoS` on huge repos).
pub const MAX_VISITED_FS_NODES: usize = 1_500;

#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;

const SKIPPED_DIR_NAMES: [&str; 6] = [
    ".git",
    "node_modules",
    "target",
    ".venv",
    "__pycache__",
    ".next",
];

#[cfg(windows)]
fn is_windows_reparse_point(meta: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    (meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT) != 0
}

#[cfg(not(windows))]
const fn is_windows_reparse_point(_meta: &fs::Metadata) -> bool {
    false
}

/// Returns `true` when `meta` is a symlink OR (on Windows) an NTFS reparse point / junction.
#[must_use]
pub fn is_symlink_or_reparse_point(meta: &fs::Metadata) -> bool {
    meta.file_type().is_symlink() || is_windows_reparse_point(meta)
}

/// Returns `true` if a raw filesystem path contains NUL bytes, UNC shares, or Windows device prefixes.
#[must_use]
pub fn is_unsafe_raw_workspace_path(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.as_bytes().contains(&0) {
        return true;
    }
    let upper = trimmed.to_ascii_uppercase();
    upper.starts_with(r"\\?\UNC\")
        || upper.starts_with("//?/UNC/")
        || upper.starts_with(r"\\.\")
        || upper.starts_with("//./")
        || trimmed.starts_with(r"\\")
        || trimmed.starts_with("//")
}

#[cfg(windows)]
fn strip_windows_verbatim_prefix(raw: String) -> String {
    if let Some(stripped) = raw.strip_prefix(r"\\?\")
        && !stripped.to_ascii_uppercase().starts_with("UNC\\")
    {
        return stripped.to_owned();
    }
    raw
}

#[cfg(not(windows))]
fn strip_windows_verbatim_prefix(raw: String) -> String {
    raw
}

/// Strips the Windows `\\?\` verbatim prefix for clean UI display when safe (never UNC).
#[must_use]
pub fn clean_display_path(path: &Path) -> String {
    strip_windows_verbatim_prefix(path.to_string_lossy().into_owned())
}

/// Validates and canonicalizes a candidate project workspace root directory for safe binding
/// (`PATCH /v1/projects/{id}`) and file listing (`GET /v1/projects/{id}/files`).
///
/// Rejects UNC/device paths, `..` traversal components, symlinks, Windows NTFS junctions /
/// reparse points, and non-directory targets.
///
/// # Errors
///
/// Returns [`DennettError::AccessDenied`] on unsafe paths, symlinks/junctions, or non-directories,
/// or [`DennettError::InvalidInput`] if the path is empty or contains NUL.
pub fn validate_safe_workspace_root(raw: &str, scope_label: &str) -> Result<PathBuf, DennettError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.as_bytes().contains(&0) {
        return Err(DennettError::InvalidInput {
            field: String::from("workspace_path"),
            reason: String::from("must be non-empty without NUL bytes"),
        });
    }
    if is_unsafe_raw_workspace_path(trimmed) {
        return Err(DennettError::AccessDenied {
            scope: scope_label.to_owned(),
            reason: String::from("UNC or Windows device paths are forbidden for workspace roots"),
        });
    }

    let path = Path::new(trimmed);
    if !path.is_absolute() {
        return Err(DennettError::AccessDenied {
            scope: scope_label.to_owned(),
            reason: String::from("workspace root must be an absolute local directory path"),
        });
    }
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(DennettError::AccessDenied {
            scope: scope_label.to_owned(),
            reason: String::from("workspace root must not contain `..` traversal segments"),
        });
    }

    // Inspect the target itself via symlink_metadata BEFORE canonicalizing so symlink/junction roots fail closed.
    let meta = fs::symlink_metadata(path).map_err(|err| DennettError::AccessDenied {
        scope: scope_label.to_owned(),
        reason: format!("workspace directory is inaccessible: {err}"),
    })?;
    if is_symlink_or_reparse_point(&meta) {
        return Err(DennettError::AccessDenied {
            scope: scope_label.to_owned(),
            reason: String::from(
                "workspace root cannot be a symbolic link or NTFS reparse point / junction",
            ),
        });
    }
    if !meta.is_dir() {
        return Err(DennettError::AccessDenied {
            scope: scope_label.to_owned(),
            reason: String::from("workspace root must be an existing directory"),
        });
    }

    let canonical = fs::canonicalize(path).map_err(|err| DennettError::AccessDenied {
        scope: scope_label.to_owned(),
        reason: format!("failed canonicalizing workspace directory: {err}"),
    })?;
    let canonical_display = clean_display_path(&canonical);
    if is_unsafe_raw_workspace_path(&canonical_display) {
        return Err(DennettError::AccessDenied {
            scope: scope_label.to_owned(),
            reason: String::from(
                "canonical workspace root resolves to a forbidden UNC or device path",
            ),
        });
    }
    if !canonical.is_dir() {
        return Err(DennettError::AccessDenied {
            scope: scope_label.to_owned(),
            reason: String::from("canonical workspace root is not a directory"),
        });
    }
    Ok(canonical)
}

/// Validates an optional relative `subpath` strictly within `canonical_root`, verifying every
/// segment with `symlink_metadata` before descending and checking canonical containment.
fn resolve_safe_subpath(
    canonical_root: &Path,
    raw_subpath: &str,
    scope_label: &str,
) -> Result<PathBuf, DennettError> {
    if raw_subpath.as_bytes().contains(&0) {
        return Err(DennettError::AccessDenied {
            scope: scope_label.to_owned(),
            reason: String::from("subpath must not contain NUL bytes"),
        });
    }
    let normalized = raw_subpath.trim().replace('\\', "/");
    if normalized.is_empty() || normalized == "." {
        return Ok(canonical_root.to_path_buf());
    }
    if normalized.starts_with('/') || normalized.contains(':') {
        return Err(DennettError::AccessDenied {
            scope: scope_label.to_owned(),
            reason: String::from("subpath must be relative to the project workspace"),
        });
    }

    let mut current = canonical_root.to_path_buf();
    for segment in normalized.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." || segment == "~" {
            return Err(DennettError::AccessDenied {
                scope: scope_label.to_owned(),
                reason: format!("subpath traversal segment `{segment}` is forbidden"),
            });
        }
        current.push(segment);
        let meta = fs::symlink_metadata(&current).map_err(|err| DennettError::AccessDenied {
            scope: scope_label.to_owned(),
            reason: format!("subpath `{raw_subpath}` is inaccessible: {err}"),
        })?;
        if is_symlink_or_reparse_point(&meta) {
            return Err(DennettError::AccessDenied {
                scope: scope_label.to_owned(),
                reason: format!(
                    "subpath `{raw_subpath}` traverses a symbolic link or NTFS reparse point"
                ),
            });
        }
        if !meta.is_dir() {
            return Err(DennettError::AccessDenied {
                scope: scope_label.to_owned(),
                reason: format!("subpath `{raw_subpath}` is not a directory"),
            });
        }
    }

    let canonical_target =
        fs::canonicalize(&current).map_err(|err| DennettError::AccessDenied {
            scope: scope_label.to_owned(),
            reason: format!("failed canonicalizing subpath: {err}"),
        })?;
    if !canonical_target.starts_with(canonical_root) {
        return Err(DennettError::AccessDenied {
            scope: scope_label.to_owned(),
            reason: String::from("subpath escapes canonical project workspace root"),
        });
    }
    Ok(canonical_target)
}

/// Lists real files and directories inside a project's bound workspace with strict symlink,
/// junction/reparse-point, canonical containment, and anti-DoS depth/count bounds.
///
/// Intended to be invoked inside `tokio::task::spawn_blocking`.
///
/// # Errors
///
/// Returns [`DennettError::AccessDenied`] if the root or requested `subpath` violates workspace
/// containment or traverses a symlink/reparse point.
#[expect(
    clippy::too_many_lines,
    reason = "bounded BFS traversal checks canonical root containment, skips symlinks and Windows reparse points, and builds sorted UI entries"
)]
pub fn list_project_workspace_files(
    project_id: ProjectId,
    workspace_path: Option<&str>,
    subpath: Option<&str>,
) -> Result<ListProjectFilesResponse, DennettError> {
    let Some(raw_root) = workspace_path.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(ListProjectFilesResponse {
            project_id,
            workspace_bound: false,
            workspace_path: None,
            entries: Vec::new(),
            truncated: false,
        });
    };

    let scope_label = format!("project:{project_id}");
    let canonical_root = validate_safe_workspace_root(raw_root, &scope_label)?;
    let start_dir = match subpath.map(str::trim).filter(|s| !s.is_empty()) {
        Some(sub) => resolve_safe_subpath(&canonical_root, sub, &scope_label)?,
        None => canonical_root.clone(),
    };

    let mut entries: Vec<WorkspaceFileEntry> = Vec::new();
    let mut truncated = false;
    let mut visited_nodes: usize = 0;

    // BFS with per-directory deterministic sorting (directories first, then files, case-insensitive).
    let mut queue: VecDeque<(PathBuf, u32)> = VecDeque::new();
    queue.push_back((start_dir, 1));

    while let Some((dir_path, depth)) = queue.pop_front() {
        if entries.len() >= MAX_LISTING_ENTRIES || visited_nodes >= MAX_VISITED_FS_NODES {
            truncated = true;
            break;
        }
        let Ok(read_dir) = fs::read_dir(&dir_path) else {
            continue;
        };

        let mut children: Vec<(String, PathBuf, fs::Metadata)> = Vec::new();
        for item in read_dir {
            visited_nodes += 1;
            if visited_nodes > MAX_VISITED_FS_NODES {
                truncated = true;
                break;
            }
            let Ok(dir_entry) = item else {
                continue;
            };
            let child_path = dir_entry.path();
            let Ok(meta) = fs::symlink_metadata(&child_path) else {
                continue;
            };
            // Never follow or leak symlinks or Windows NTFS junctions/reparse points.
            if is_symlink_or_reparse_point(&meta) {
                continue;
            }
            let name = dir_entry.file_name().to_string_lossy().into_owned();
            if meta.is_dir() && SKIPPED_DIR_NAMES.contains(&name.as_str()) {
                continue;
            }
            children.push((name, child_path, meta));
        }

        children.sort_by(|(name_a, _, meta_a), (name_b, _, meta_b)| {
            meta_b
                .is_dir()
                .cmp(&meta_a.is_dir())
                .then_with(|| {
                    name_a
                        .to_ascii_lowercase()
                        .cmp(&name_b.to_ascii_lowercase())
                })
                .then_with(|| name_a.cmp(name_b))
        });

        for (name, child_path, meta) in children {
            if entries.len() >= MAX_LISTING_ENTRIES {
                truncated = true;
                break;
            }

            let Ok(rel) = child_path.strip_prefix(&canonical_root) else {
                continue;
            };
            let rel_posix = rel
                .components()
                .filter_map(|c| match c {
                    Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("/");
            if rel_posix.is_empty() {
                continue;
            }

            if meta.is_dir() {
                // Re-verify canonical containment before queueing subdirectory traversal.
                if let Ok(canon_child) = fs::canonicalize(&child_path)
                    && canon_child.starts_with(&canonical_root)
                {
                    entries.push(WorkspaceFileEntry {
                        relative_path: rel_posix,
                        name,
                        kind: WorkspaceFileKind::Directory,
                        size_bytes: None,
                        depth,
                    });
                    if depth < MAX_LISTING_DEPTH {
                        queue.push_back((canon_child, depth + 1));
                    }
                }
            } else if meta.is_file() {
                entries.push(WorkspaceFileEntry {
                    relative_path: rel_posix,
                    name,
                    kind: WorkspaceFileKind::File,
                    size_bytes: Some(meta.len()),
                    depth,
                });
            }
        }
    }

    Ok(ListProjectFilesResponse {
        project_id,
        workspace_bound: true,
        workspace_path: Some(clean_display_path(&canonical_root)),
        entries,
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use std::env;
    use uuid::Uuid;

    use super::*;

    fn make_temp_dir(label: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!(
            "dennett-ws-files-{label}-{}",
            Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn folderless_project_returns_unbound_empty_listing() {
        let pid = ProjectId::new();
        let resp = list_project_workspace_files(pid, None, None).expect("unbound listing");
        assert!(!resp.workspace_bound);
        assert!(resp.workspace_path.is_none());
        assert_eq!(resp.entries, Vec::new());
        assert!(!resp.truncated);
    }

    #[test]
    fn rejects_traversal_unc_and_device_paths() {
        let root = make_temp_dir("trav");
        let pid = ProjectId::new();

        for bad_sub in [
            "../outside",
            "a/../../outside",
            "/etc",
            "C:\\Windows",
            "a\0b",
        ] {
            let err =
                list_project_workspace_files(pid, Some(&root.to_string_lossy()), Some(bad_sub));
            assert!(
                matches!(err, Err(DennettError::AccessDenied { .. })),
                "subpath `{bad_sub}` must be rejected with AccessDenied, got: {err:?}"
            );
        }

        for bad_root in [
            ".",
            "services",
            "C:relative-dir",
            "../outside",
            r"\\?\UNC\server\share",
            r"\\.\PhysicalDrive0",
            r"\\evil\share",
        ] {
            let err = validate_safe_workspace_root(bad_root, "project:test");
            assert!(
                matches!(err, Err(DennettError::AccessDenied { .. })),
                "unsafe root `{bad_root}` must fail closed, got: {err:?}"
            );
        }

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn lists_real_files_skips_git_and_truncates_above_cap() {
        let root = make_temp_dir("cap");
        fs::create_dir_all(root.join(".git")).expect("git dir");
        fs::write(root.join(".git/HEAD"), "ref: refs/heads/main").expect("git head");
        fs::create_dir_all(root.join("src")).expect("src dir");
        fs::write(root.join("src/main.rs"), "fn main() {}").expect("main.rs");

        // Create > MAX_LISTING_ENTRIES files to verify anti-DoS cap.
        fs::create_dir_all(root.join("many")).expect("many dir");
        for i in 0..(MAX_LISTING_ENTRIES + 25) {
            fs::write(root.join("many").join(format!("f-{i:03}.txt")), "x").expect("write");
        }

        let resp =
            list_project_workspace_files(ProjectId::new(), Some(&root.to_string_lossy()), None)
                .expect("list files");
        assert!(resp.workspace_bound);
        assert!(
            resp.truncated,
            "must truncate when exceeding MAX_LISTING_ENTRIES"
        );
        assert_eq!(resp.entries.len(), MAX_LISTING_ENTRIES);
        assert!(
            !resp
                .entries
                .iter()
                .any(|e| e.relative_path.starts_with(".git")),
            ".git directory must be skipped"
        );
        assert!(
            resp.entries.iter().any(|e| e.relative_path == "src"),
            "real directory src must be present"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_and_skips_symlinks_and_ntfs_junctions() {
        let root = make_temp_dir("sym-root");
        let outside = make_temp_dir("sym-outside");
        fs::write(outside.join("secret.txt"), "top-secret").expect("secret");
        fs::write(root.join("readme.md"), "# ok").expect("readme");

        let link_path = root.join("escape_link");
        let created_link = create_dir_link(&link_path, &outside);

        if created_link {
            // 1. Listing workspace root must SKIP the symlink/junction cleanly.
            let resp =
                list_project_workspace_files(ProjectId::new(), Some(&root.to_string_lossy()), None)
                    .expect("list root");
            assert!(
                !resp
                    .entries
                    .iter()
                    .any(|e| e.name == "escape_link" || e.name == "secret.txt"),
                "symlink/junction escape must never appear in listing: {:?}",
                resp.entries
            );
            assert!(resp.entries.iter().any(|e| e.relative_path == "readme.md"));

            // 2. Requesting subpath through the symlink/junction MUST fail closed with AccessDenied.
            let sub_err = list_project_workspace_files(
                ProjectId::new(),
                Some(&root.to_string_lossy()),
                Some("escape_link"),
            );
            assert!(
                matches!(sub_err, Err(DennettError::AccessDenied { .. })),
                "symlink/junction subpath must be rejected, got: {sub_err:?}"
            );

            // 3. Binding the symlink/junction directly as workspace root MUST fail closed with AccessDenied.
            let root_err =
                validate_safe_workspace_root(&link_path.to_string_lossy(), "project:test");
            assert!(
                matches!(root_err, Err(DennettError::AccessDenied { .. })),
                "symlink/junction root must be rejected, got: {root_err:?}"
            );
        }

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&outside);
    }

    fn create_dir_link(link: &Path, target: &Path) -> bool {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link).is_ok()
        }
        #[cfg(windows)]
        {
            // NTFS directory junction works without Developer Mode / admin privileges on Windows!
            std::process::Command::new("cmd")
                .args([
                    "/C",
                    "mklink",
                    "/J",
                    &link.to_string_lossy(),
                    &target.to_string_lossy(),
                ])
                .output()
                .is_ok_and(|out| out.status.success())
        }
    }
}
