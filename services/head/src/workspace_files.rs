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
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use dennett_contracts::{
    DennettError, ListProjectFilesResponse, ProjectFileContentResponse, ProjectId, Sha256Hex,
    WorkspaceFileEntry, WorkspaceFileKind,
};
use sha2::{Digest, Sha256};

/// Maximum recursion depth below the listed directory.
pub const MAX_LISTING_DEPTH: u32 = 3;
/// Maximum number of file/directory entries returned in one response.
pub const MAX_LISTING_ENTRIES: usize = 200;
/// Maximum total filesystem entries inspected before truncating (prevents `DoS` on huge repos).
pub const MAX_VISITED_FS_NODES: usize = 1_500;
/// Maximum UTF-8 preview bytes returned inline by `GET /v1/projects/{id}/file?path=...`.
pub const MAX_FILE_PREVIEW_BYTES: usize = 256 * 1024;
/// Maximum bytes hashed when computing SHA-256 on oversized workspace files (16 MiB anti-DoS cap).
const MAX_FILE_HASH_BYTES: usize = 16 * 1024 * 1024;

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

/// Validates and reads a bounded, symlink/reparse-point-safe preview of a workspace-relative file
/// (`GET /v1/projects/{id}/file?path=...`).
///
/// Security guarantees:
/// - Rejects `..`, `~`, leading `/`, drive prefixes (`:`), NUL bytes, symlinks, and Windows NTFS
///   reparse points / junctions on EVERY intermediate segment AND on the target file itself.
/// - Verifies canonical containment (`canonical_target.starts_with(&canonical_root)`).
/// - Rejects directories and non-regular files.
/// - Caps inline UTF-8 content at [`MAX_FILE_PREVIEW_BYTES`] (256 KiB), handles multi-byte UTF-8
///   boundaries cleanly without false binary classification, and detects binary payloads.
///
/// Intended to be invoked inside `tokio::task::spawn_blocking`.
///
/// # Errors
///
/// Returns [`DennettError::InvalidInput`], [`DennettError::NotFound`], or [`DennettError::AccessDenied`].
#[expect(
    clippy::too_many_lines,
    reason = "segment-by-segment symlink verification, canonical containment, bounded streaming SHA-256, and binary/UTF-8 classification"
)]
pub fn read_project_workspace_file(
    project_id: ProjectId,
    workspace_path: Option<&str>,
    raw_rel_path: &str,
) -> Result<ProjectFileContentResponse, DennettError> {
    let scope_label = format!("project:{project_id}");
    let Some(raw_root) = workspace_path.map(str::trim).filter(|s| !s.is_empty()) else {
        return Err(DennettError::AccessDenied {
            scope: scope_label,
            reason: String::from("project has no bound workspace folder"),
        });
    };

    if raw_rel_path.as_bytes().contains(&0) {
        return Err(DennettError::AccessDenied {
            scope: scope_label,
            reason: String::from("file path must not contain NUL bytes"),
        });
    }
    let normalized = raw_rel_path.trim().replace('\\', "/");
    if normalized.is_empty() || normalized == "." {
        return Err(DennettError::InvalidInput {
            field: String::from("path"),
            reason: String::from("file path must be non-empty"),
        });
    }
    if normalized.starts_with('/') || normalized.contains(':') {
        return Err(DennettError::AccessDenied {
            scope: scope_label,
            reason: String::from("file path must be relative to the project workspace"),
        });
    }

    let canonical_root = validate_safe_workspace_root(raw_root, &scope_label)?;

    let mut segments: Vec<&str> = Vec::new();
    for segment in normalized.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." || segment == "~" {
            return Err(DennettError::AccessDenied {
                scope: scope_label,
                reason: format!("file path traversal segment `{segment}` is forbidden"),
            });
        }
        segments.push(segment);
    }
    if segments.is_empty() {
        return Err(DennettError::InvalidInput {
            field: String::from("path"),
            reason: String::from("file path must reference a file inside the project workspace"),
        });
    }

    let mut current = canonical_root.clone();
    let last_idx = segments.len() - 1;
    for (idx, segment) in segments.iter().enumerate() {
        current.push(segment);
        let meta = fs::symlink_metadata(&current).map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                DennettError::NotFound {
                    resource: String::from("workspace_file"),
                    id: normalized.clone(),
                }
            } else {
                DennettError::AccessDenied {
                    scope: scope_label.clone(),
                    reason: format!("file path `{normalized}` is inaccessible: {err}"),
                }
            }
        })?;
        if is_symlink_or_reparse_point(&meta) {
            return Err(DennettError::AccessDenied {
                scope: scope_label,
                reason: format!(
                    "file path `{normalized}` traverses or targets a symbolic link or NTFS reparse point"
                ),
            });
        }
        if idx < last_idx {
            if !meta.is_dir() {
                return Err(DennettError::AccessDenied {
                    scope: scope_label,
                    reason: format!("intermediate segment `{segment}` is not a directory"),
                });
            }
        } else if !meta.is_file() {
            return Err(DennettError::InvalidInput {
                field: String::from("path"),
                reason: format!("`{normalized}` is not a regular file"),
            });
        }
    }

    let canonical_target =
        fs::canonicalize(&current).map_err(|err| DennettError::AccessDenied {
            scope: scope_label.clone(),
            reason: format!("failed canonicalizing file path: {err}"),
        })?;
    if !canonical_target.starts_with(&canonical_root) {
        return Err(DennettError::AccessDenied {
            scope: scope_label,
            reason: String::from("file path escapes canonical project workspace root"),
        });
    }

    let file_meta =
        fs::symlink_metadata(&canonical_target).map_err(|err| DennettError::AccessDenied {
            scope: scope_label.clone(),
            reason: format!("failed inspecting target file: {err}"),
        })?;
    if is_symlink_or_reparse_point(&file_meta) || !file_meta.is_file() {
        return Err(DennettError::AccessDenied {
            scope: scope_label,
            reason: String::from("target is not a regular non-symlink workspace file"),
        });
    }
    let byte_size = file_meta.len();

    let mut file = fs::File::open(&canonical_target).map_err(|err| DennettError::AccessDenied {
        scope: scope_label,
        reason: format!("failed opening workspace file: {err}"),
    })?;

    let mut preview_bytes: Vec<u8> = Vec::new();
    let mut hasher = Sha256::new();
    let mut hashed_bytes: usize = 0;
    let mut buf = [0_u8; 8192];
    let mut saw_more_than_preview = false;
    loop {
        let n = file.read(&mut buf).map_err(|err| DennettError::Internal {
            detail: format!("failed reading workspace file: {err}"),
        })?;
        if n == 0 {
            break;
        }
        let chunk = &buf[..n];
        if preview_bytes.len() < MAX_FILE_PREVIEW_BYTES {
            let remaining = MAX_FILE_PREVIEW_BYTES - preview_bytes.len();
            if n <= remaining {
                preview_bytes.extend_from_slice(chunk);
            } else {
                preview_bytes.extend_from_slice(&chunk[..remaining]);
                saw_more_than_preview = true;
            }
        } else {
            saw_more_than_preview = true;
        }
        if hashed_bytes < MAX_FILE_HASH_BYTES {
            let hash_take = (MAX_FILE_HASH_BYTES - hashed_bytes).min(n);
            hasher.update(&chunk[..hash_take]);
            hashed_bytes += hash_take;
        } else {
            break;
        }
    }

    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for b in digest {
        use core::fmt::Write as _;
        let _ = write!(&mut hex, "{b:02x}");
    }
    let sha256 = Sha256Hex::try_new(hex)?;

    let truncated = saw_more_than_preview || byte_size > MAX_FILE_PREVIEW_BYTES as u64;
    let (is_binary, content) = classify_and_decode_preview(&preview_bytes, truncated);
    let relative_path = segments.join("/");
    let mime_type = infer_workspace_mime_type(&relative_path, is_binary).to_owned();

    Ok(ProjectFileContentResponse {
        relative_path,
        mime_type,
        byte_size,
        sha256,
        content,
        truncated,
        is_binary,
    })
}

fn classify_and_decode_preview(bytes: &[u8], truncated: bool) -> (bool, Option<String>) {
    if bytes.contains(&0) {
        return (true, None);
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => (false, Some(s.to_owned())),
        Err(err) => {
            // If truncated right in the middle of a multi-byte UTF-8 sequence at the tail,
            // trim the trailing incomplete 1..=3 bytes and still return valid UTF-8 text.
            if truncated && err.error_len().is_none() {
                let valid = err.valid_up_to();
                if let Ok(prefix) = std::str::from_utf8(&bytes[..valid]) {
                    return (false, Some(prefix.to_owned()));
                }
            }
            (true, None)
        }
    }
}

fn infer_workspace_mime_type(rel_path: &str, is_binary: bool) -> &'static str {
    let ext = Path::new(rel_path)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    match ext.as_deref() {
        Some("md" | "markdown") => "text/markdown",
        Some("rs") => "text/x-rust",
        Some("ts" | "tsx") => "text/typescript",
        Some("js" | "jsx" | "mjs" | "cjs") => "text/javascript",
        Some("json") => "application/json",
        Some("toml") => "application/toml",
        Some("yaml" | "yml") => "application/yaml",
        Some("sql") => "application/sql",
        Some("html" | "htm") => "text/html",
        Some("css") => "text/css",
        Some("py") => "text/x-python",
        Some("sh" | "bash" | "zsh") => "text/x-shellscript",
        Some("ps1") => "text/x-powershell",
        Some("xml" | "svg") => "application/xml",
        Some("csv") => "text/csv",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("pdf") => "application/pdf",
        Some("txt" | "log" | "env" | "gitignore" | "lock") => "text/plain",
        _ if is_binary => "application/octet-stream",
        _ => "text/plain",
    }
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

    #[test]
    fn read_project_workspace_file_previews_truncates_detects_binary_and_blocks_escapes() {
        let root = make_temp_dir("file-preview");
        let outside = make_temp_dir("file-outside");
        fs::create_dir_all(root.join("src")).expect("src dir");
        fs::write(
            root.join("src/hello.rs"),
            "fn main() { println!(\"привет\"); }\n",
        )
        .expect("write rust");
        fs::write(root.join("bin.dat"), [0_u8, 1, 2, 255, 0, 42]).expect("write binary");
        // Oversized UTF-8 Cyrillic file (> 256 KiB) to verify boundary-safe truncation.
        let big_text = "Я".repeat(MAX_FILE_PREVIEW_BYTES);
        fs::write(root.join("big.txt"), &big_text).expect("write big");
        fs::write(outside.join("secret.txt"), "outside-secret").expect("write outside");

        let pid = ProjectId::new();
        let ws = root.to_string_lossy().into_owned();

        let ok = read_project_workspace_file(pid, Some(&ws), "src/hello.rs").expect("read rust");
        assert_eq!(ok.relative_path, "src/hello.rs");
        assert_eq!(ok.mime_type, "text/x-rust");
        assert!(!ok.is_binary);
        assert!(!ok.truncated);
        assert!(ok.content.as_deref().unwrap_or("").contains("привет"));

        let bin = read_project_workspace_file(pid, Some(&ws), "bin.dat").expect("read binary");
        assert!(bin.is_binary);
        assert!(bin.content.is_none());
        assert_eq!(bin.byte_size, 6);

        let big = read_project_workspace_file(pid, Some(&ws), "big.txt").expect("read big");
        assert!(
            !big.is_binary,
            "truncated multi-byte UTF-8 must NOT be classified binary"
        );
        assert!(big.truncated);
        assert!(
            big.content
                .as_ref()
                .is_some_and(|s| s.len() <= MAX_FILE_PREVIEW_BYTES && !s.is_empty())
        );

        for bad in [
            "../outside/secret.txt",
            "/etc/passwd",
            "C:\\Windows\\win.ini",
            "a\0b",
        ] {
            let err = read_project_workspace_file(pid, Some(&ws), bad);
            assert!(
                matches!(err, Err(DennettError::AccessDenied { .. })),
                "bad path `{bad}` must be denied, got: {err:?}"
            );
        }

        // Directory target must fail cleanly.
        let dir_err = read_project_workspace_file(pid, Some(&ws), "src");
        assert!(matches!(dir_err, Err(DennettError::InvalidInput { .. })));

        // Symlink/junction escape must fail closed.
        let link_path = root.join("escape_dir");
        if create_dir_link(&link_path, &outside) {
            let esc = read_project_workspace_file(pid, Some(&ws), "escape_dir/secret.txt");
            assert!(
                matches!(esc, Err(DennettError::AccessDenied { .. })),
                "symlink/junction escape must be rejected, got: {esc:?}"
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
