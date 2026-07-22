//! Capability-scoped project-folder inspection and portable identity effects.
//!
//! This module deliberately knows nothing about trust policy. Project files are
//! untrusted input: the only portable project metadata accepted here is a
//! format version and a project UUID.

use cap_fs_ext::{DirExt, FollowSymlinks, MetadataExt, OpenOptionsFollowExt, OpenOptionsSyncExt};
use cap_std::{
    ambient_authority,
    fs::{Dir, Metadata, OpenOptions},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    ffi::{OsStr, OsString},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
};
use uuid::Uuid;

const PROJECT_METADATA_PATH: &str = "project.json";
const PROJECT_METADATA_LIMIT: u64 = 16 * 1024;
const MEMORY_MANIFEST_LIMIT: u64 = 256 * 1024;
const GIT_WORKTREE_FILE_LIMIT: u64 = 16 * 1024;
const MEMORY_README: &[u8] = b"# Dennett shared project memory\n\nThis directory is reserved for project knowledge that is useful to every collaborator and safe to publish with the repository.\n\nDo not store personal chats, credentials, secrets, machine paths, local permissions, provider settings or other user-specific state here.\n";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProjectFolderIntent {
    CreateEmpty,
    AttachExisting,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct InspectionLimits {
    pub(crate) max_depth: usize,
    pub(crate) max_entries: usize,
    pub(crate) max_instruction_file_bytes: u64,
    pub(crate) max_instruction_bytes: u64,
}

impl Default for InspectionLimits {
    fn default() -> Self {
        Self {
            max_depth: 16,
            max_entries: 20_000,
            max_instruction_file_bytes: 512 * 1024,
            max_instruction_bytes: 4 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProjectFolderAdapter {
    limits: InspectionLimits,
}

impl Default for ProjectFolderAdapter {
    fn default() -> Self {
        Self::new(InspectionLimits::default())
    }
}

impl ProjectFolderAdapter {
    pub(crate) const fn new(limits: InspectionLimits) -> Self {
        Self { limits }
    }

    /// Inspects an existing directory or a prospective absent child without
    /// mutating it or executing project code.
    pub(crate) fn inspect(
        &self,
        root: &Path,
        intent: ProjectFolderIntent,
    ) -> Result<ProjectFolderInspection, ProjectFolderError> {
        let normalized = normalize_absolute(root)?;
        let prospective = inspect_prospective_location(&normalized)?;
        if !prospective.location_exists {
            if intent != ProjectFolderIntent::CreateEmpty {
                return Err(ProjectFolderError::LocationMissing);
            }
            return Ok(ProjectFolderInspection {
                intent,
                location_exists: false,
                location_empty: true,
                source_identity: None,
                prospective_parent_identity: Some(prospective.parent_identity),
                canonical_location: prospective.canonical_location,
                git: GitLayout::None,
                portable_metadata: PortableProjectMetadata::Absent,
                shared_memory: SharedMemoryPack::Absent,
                instructions: empty_instruction_inspection(),
            });
        }

        let opened = OpenProjectRoot::open(root)?;
        let location_empty = directory_is_empty(&opened.dir)?;
        if intent == ProjectFolderIntent::CreateEmpty && !location_empty {
            return Err(ProjectFolderError::CreateRootNotEmpty);
        }
        let git = inspect_git(&opened.dir)?;
        let portable_metadata = inspect_portable_metadata(&opened.dir)?;
        let shared_memory = inspect_shared_memory(&opened.dir)?;
        let instructions = scan_instructions(&opened.dir, self.limits)?;

        opened.revalidate_location()?;
        Ok(ProjectFolderInspection {
            intent,
            location_exists: true,
            location_empty,
            source_identity: Some(opened.identity),
            prospective_parent_identity: None,
            canonical_location: opened.canonical_location,
            git,
            portable_metadata,
            shared_memory,
            instructions,
        })
    }

    /// Idempotently creates exactly the selected empty directory.
    pub(crate) fn create_empty_root(
        &self,
        root: &Path,
        expected: CreateEmptyPrecondition,
    ) -> Result<CreatedEmptyRoot, ProjectFolderError> {
        let normalized = normalize_absolute(root)?;
        match expected {
            CreateEmptyPrecondition::ExistingEmpty { source_identity } => {
                let opened = OpenProjectRoot::open_verified(&normalized, source_identity)?;
                if !directory_is_empty(&opened.dir)? {
                    return Err(ProjectFolderError::CreateRootNotEmpty);
                }
                opened.revalidate_location()?;
                Ok(CreatedEmptyRoot {
                    source_identity: opened.identity,
                    canonical_location: opened.canonical_location,
                    created: false,
                })
            }
            CreateEmptyPrecondition::Absent {
                parent_source_identity,
            } => create_absent_empty_root(&normalized, parent_source_identity),
        }
    }

    /// Creates the minimal portable identity and shared-memory container.
    ///
    /// Existing valid matching metadata is preserved. Invalid, unsupported, or
    /// conflicting metadata is never overwritten.
    pub(crate) fn create_minimal(
        &self,
        root: &Path,
        expected_source_identity: SourceIdentity,
        project_id: Uuid,
    ) -> Result<MinimalStructureEffect, ProjectFolderError> {
        validate_project_id(project_id)?;
        let opened = OpenProjectRoot::open_verified(root, expected_source_identity)?;
        preflight_minimal_structure(&opened.dir, project_id)?;
        let dennett = open_or_create_directory(&opened.dir, OsStr::new(".dennett"))?;

        let metadata_created = ensure_project_metadata(&dennett, project_id)?;
        let memory = open_or_create_directory(&dennett, OsStr::new("memory"))?;
        let readme_created = ensure_regular_file(&memory, OsStr::new("README.md"), MEMORY_README)?;

        opened.revalidate_location()?;
        let observed = inspect_portable_metadata(&opened.dir)?;
        if observed != (PortableProjectMetadata::Valid { project_id }) {
            return Err(ProjectFolderError::PostconditionFailed);
        }

        Ok(MinimalStructureEffect {
            metadata_created,
            readme_created,
        })
    }

    /// Rewrites only the portable project UUID for an explicit fork operation.
    pub(crate) fn rewrite_project_identity(
        &self,
        root: &Path,
        expected_source_identity: SourceIdentity,
        expected_project_id: Uuid,
        new_project_id: Uuid,
    ) -> Result<(), ProjectFolderError> {
        validate_project_id(expected_project_id)?;
        validate_project_id(new_project_id)?;
        if expected_project_id == new_project_id {
            return Err(ProjectFolderError::IdentityUnchanged);
        }

        let opened = OpenProjectRoot::open_verified(root, expected_source_identity)?;
        let dennett = open_required_directory(&opened.dir, OsStr::new(".dennett"))?;
        let current = read_project_metadata_from(&dennett)?;
        if current
            != (PortableProjectMetadata::Valid {
                project_id: expected_project_id,
            })
        {
            return Err(ProjectFolderError::PortableMetadataConflict);
        }

        // Re-read immediately before publication so an unexpected edit is not
        // silently accepted as the fork source.
        if read_project_metadata_from(&dennett)?
            != (PortableProjectMetadata::Valid {
                project_id: expected_project_id,
            })
        {
            return Err(ProjectFolderError::PortableMetadataConflict);
        }

        let bytes = serialize_project_metadata(new_project_id)?;
        atomic_replace_regular_file(&dennett, OsStr::new(PROJECT_METADATA_PATH), &bytes)?;
        opened.revalidate_location()?;
        if inspect_portable_metadata(&opened.dir)?
            != (PortableProjectMetadata::Valid {
                project_id: new_project_id,
            })
        {
            return Err(ProjectFolderError::PostconditionFailed);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProjectFolderInspection {
    pub(crate) intent: ProjectFolderIntent,
    pub(crate) location_exists: bool,
    pub(crate) location_empty: bool,
    pub(crate) source_identity: Option<SourceIdentity>,
    pub(crate) prospective_parent_identity: Option<SourceIdentity>,
    /// Canonicalized local binding key. It is not portable project metadata.
    pub(crate) canonical_location: String,
    pub(crate) git: GitLayout,
    pub(crate) portable_metadata: PortableProjectMetadata,
    pub(crate) shared_memory: SharedMemoryPack,
    pub(crate) instructions: InstructionInspection,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct SourceIdentity {
    pub(crate) volume: u64,
    pub(crate) file: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CreateEmptyPrecondition {
    Absent {
        parent_source_identity: SourceIdentity,
    },
    ExistingEmpty {
        source_identity: SourceIdentity,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CreatedEmptyRoot {
    pub(crate) source_identity: SourceIdentity,
    pub(crate) canonical_location: String,
    pub(crate) created: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GitLayout {
    None,
    Directory,
    WorktreeFile,
    Invalid,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PortableProjectMetadata {
    Absent,
    Valid { project_id: Uuid },
    Invalid,
    Unsupported { format_version: u64 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SharedMemoryPack {
    Absent,
    ManifestPresent,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InstructionInspection {
    pub(crate) sources: Vec<InstructionSource>,
    pub(crate) aggregate_sha256: Option<String>,
    pub(crate) completeness: InspectionCompleteness,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InstructionSource {
    pub(crate) relative_path: String,
    pub(crate) byte_len: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum InspectionCompleteness {
    Complete,
    Incomplete(Vec<InspectionLimitReached>),
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum InspectionLimitReached {
    Depth,
    Entries,
    InstructionFileBytes,
    InstructionTotalBytes,
    NonUnicodePath,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MinimalStructureEffect {
    pub(crate) metadata_created: bool,
    pub(crate) readme_created: bool,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ProjectFolderError {
    #[error("project root must be an absolute normalized location")]
    InvalidRoot,
    #[error("project root or a protected project entry is a link or reparse point")]
    LinkedEntry,
    #[error("project root is not a directory")]
    RootNotDirectory,
    #[error("project location does not exist")]
    LocationMissing,
    #[error("create-empty requires an absent or empty project directory")]
    CreateRootNotEmpty,
    #[error("the project folder no longer refers to the inspected source")]
    SourceIdentityChanged,
    #[error("portable project metadata conflicts with the requested operation")]
    PortableMetadataConflict,
    #[error("project UUID must be non-nil")]
    InvalidProjectId,
    #[error("fork identity must differ from the current identity")]
    IdentityUnchanged,
    #[error("a project-folder write did not satisfy its postcondition")]
    PostconditionFailed,
    #[error("project filesystem operation failed: {operation}")]
    Io {
        operation: &'static str,
        #[source]
        source: io::Error,
    },
}

impl ProjectFolderError {
    fn io(operation: &'static str, source: io::Error) -> Self {
        Self::Io { operation, source }
    }
}

struct OpenProjectRoot {
    dir: Dir,
    normalized_location: PathBuf,
    canonical_location: String,
    identity: SourceIdentity,
}

struct ProspectiveLocation {
    location_exists: bool,
    parent_identity: SourceIdentity,
    canonical_location: String,
}

impl OpenProjectRoot {
    fn open(root: &Path) -> Result<Self, ProjectFolderError> {
        let normalized_location = normalize_absolute(root)?;
        let dir = open_absolute_directory_nofollow(&normalized_location)?;
        let identity = directory_identity(&dir)?;

        // Canonicalization is used only after every supplied component has
        // been opened no-follow. Reopen and compare the final handle identity
        // so an alias or concurrent path replacement cannot change the owner.
        let canonical_path = std::fs::canonicalize(&normalized_location)
            .map_err(|source| ProjectFolderError::io("canonicalize_project_root", source))?;
        let canonical_dir = open_absolute_directory_nofollow(&canonical_path)?;
        if directory_identity(&canonical_dir)? != identity {
            return Err(ProjectFolderError::SourceIdentityChanged);
        }
        let canonical_location = canonical_location_string(&canonical_path)?;

        Ok(Self {
            dir,
            normalized_location,
            canonical_location,
            identity,
        })
    }

    fn open_verified(root: &Path, expected: SourceIdentity) -> Result<Self, ProjectFolderError> {
        let opened = Self::open(root)?;
        if opened.identity != expected {
            return Err(ProjectFolderError::SourceIdentityChanged);
        }
        Ok(opened)
    }

    fn revalidate_location(&self) -> Result<(), ProjectFolderError> {
        let current = open_absolute_directory_nofollow(&self.normalized_location)?;
        if directory_identity(&current)? != self.identity {
            return Err(ProjectFolderError::SourceIdentityChanged);
        }
        Ok(())
    }
}

fn inspect_prospective_location(
    normalized: &Path,
) -> Result<ProspectiveLocation, ProjectFolderError> {
    let (parent_path, child_name) = parent_and_child(normalized)?;
    let parent = open_absolute_directory_nofollow(parent_path)?;
    let parent_identity = directory_identity(&parent)?;
    let canonical_parent = canonicalize_verified_directory(parent_path, parent_identity)?;
    let canonical_location = canonical_location_string(&canonical_parent.join(child_name))?;

    let location_exists = match optional_symlink_metadata(&parent, child_name)? {
        None => false,
        Some(metadata) => {
            reject_link_metadata(&metadata)?;
            if !metadata.is_dir() {
                return Err(ProjectFolderError::RootNotDirectory);
            }
            // Opening the final component no-follow catches Windows reparse
            // directories that are not reported as ordinary symlinks.
            parent
                .open_dir_nofollow(child_name)
                .map_err(map_nofollow_error)?;
            true
        }
    };
    revalidate_directory_location(parent_path, parent_identity)?;
    Ok(ProspectiveLocation {
        location_exists,
        parent_identity,
        canonical_location,
    })
}

fn create_absent_empty_root(
    normalized: &Path,
    expected_parent_identity: SourceIdentity,
) -> Result<CreatedEmptyRoot, ProjectFolderError> {
    let (parent_path, child_name) = parent_and_child(normalized)?;
    let parent = open_absolute_directory_nofollow(parent_path)?;
    if directory_identity(&parent)? != expected_parent_identity {
        return Err(ProjectFolderError::SourceIdentityChanged);
    }

    let created = match optional_symlink_metadata(&parent, child_name)? {
        None => match parent.create_dir(child_name) {
            Ok(()) => true,
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => false,
            Err(source) => {
                return Err(ProjectFolderError::io("create_empty_project_root", source));
            }
        },
        Some(metadata) => {
            reject_link_metadata(&metadata)?;
            if !metadata.is_dir() {
                return Err(ProjectFolderError::RootNotDirectory);
            }
            false
        }
    };
    if created {
        sync_directory(&parent, "sync_empty_project_parent")?;
    }

    let child = parent
        .open_dir_nofollow(child_name)
        .map_err(map_nofollow_error)?;
    if !directory_is_empty(&child)? {
        return Err(ProjectFolderError::CreateRootNotEmpty);
    }
    let created_identity = directory_identity(&child)?;
    revalidate_directory_location(parent_path, expected_parent_identity)?;
    let opened = OpenProjectRoot::open(normalized)?;
    if opened.identity != created_identity {
        return Err(ProjectFolderError::SourceIdentityChanged);
    }
    opened.revalidate_location()?;
    Ok(CreatedEmptyRoot {
        source_identity: created_identity,
        canonical_location: opened.canonical_location,
        created,
    })
}

fn parent_and_child(path: &Path) -> Result<(&Path, &OsStr), ProjectFolderError> {
    let parent = path.parent().ok_or(ProjectFolderError::InvalidRoot)?;
    let child = path.file_name().ok_or(ProjectFolderError::InvalidRoot)?;
    if child.is_empty() {
        return Err(ProjectFolderError::InvalidRoot);
    }
    Ok((parent, child))
}

fn canonicalize_verified_directory(
    path: &Path,
    expected_identity: SourceIdentity,
) -> Result<PathBuf, ProjectFolderError> {
    let canonical = std::fs::canonicalize(path)
        .map_err(|source| ProjectFolderError::io("canonicalize_project_parent", source))?;
    let reopened = open_absolute_directory_nofollow(&canonical)?;
    if directory_identity(&reopened)? != expected_identity {
        return Err(ProjectFolderError::SourceIdentityChanged);
    }
    Ok(canonical)
}

fn revalidate_directory_location(
    path: &Path,
    expected_identity: SourceIdentity,
) -> Result<(), ProjectFolderError> {
    let reopened = open_absolute_directory_nofollow(path)?;
    if directory_identity(&reopened)? != expected_identity {
        return Err(ProjectFolderError::SourceIdentityChanged);
    }
    Ok(())
}

fn normalize_absolute(path: &Path) -> Result<PathBuf, ProjectFolderError> {
    if !path.is_absolute() {
        return Err(ProjectFolderError::InvalidRoot);
    }

    let mut output = PathBuf::new();
    let mut normal_components = 0usize;
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => output.push(prefix.as_os_str()),
            Component::RootDir => output.push(Path::new(std::path::MAIN_SEPARATOR_STR)),
            Component::CurDir => {}
            Component::ParentDir => {
                if normal_components == 0 || !output.pop() {
                    return Err(ProjectFolderError::InvalidRoot);
                }
                normal_components -= 1;
            }
            Component::Normal(name) => {
                output.push(name);
                normal_components += 1;
            }
        }
    }
    if normal_components == 0 {
        return Err(ProjectFolderError::InvalidRoot);
    }
    Ok(output)
}

fn absolute_path_parts(path: &Path) -> Result<(PathBuf, Vec<OsString>), ProjectFolderError> {
    let anchor = path
        .ancestors()
        .filter(|candidate| candidate.has_root())
        .last()
        .ok_or(ProjectFolderError::InvalidRoot)?
        .to_path_buf();
    let components = path
        .strip_prefix(&anchor)
        .map_err(|_| ProjectFolderError::InvalidRoot)?
        .components()
        .map(|component| match component {
            Component::Normal(name) => Ok(name.to_os_string()),
            _ => Err(ProjectFolderError::InvalidRoot),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((anchor, components))
}

fn open_absolute_directory_nofollow(path: &Path) -> Result<Dir, ProjectFolderError> {
    let (anchor, components) = absolute_path_parts(path)?;
    let mut current = Dir::open_ambient_dir(anchor, ambient_authority())
        .map_err(|source| ProjectFolderError::io("open_project_anchor", source))?;
    for component in components {
        current = current
            .open_dir_nofollow(&component)
            .map_err(map_nofollow_error)?;
    }
    Ok(current)
}

fn map_nofollow_error(source: io::Error) -> ProjectFolderError {
    if matches!(
        source.kind(),
        io::ErrorKind::InvalidInput | io::ErrorKind::NotADirectory
    ) || matches!(source.raw_os_error(), Some(5 | 40 | 4390 | 1920))
    {
        ProjectFolderError::LinkedEntry
    } else {
        ProjectFolderError::io("open_project_directory_nofollow", source)
    }
}

fn directory_identity(dir: &Dir) -> Result<SourceIdentity, ProjectFolderError> {
    let metadata = dir
        .dir_metadata()
        .map_err(|source| ProjectFolderError::io("read_project_root_identity", source))?;
    if !metadata.is_dir() {
        return Err(ProjectFolderError::RootNotDirectory);
    }
    Ok(SourceIdentity {
        volume: MetadataExt::dev(&metadata),
        file: MetadataExt::ino(&metadata),
    })
}

fn canonical_location_string(path: &Path) -> Result<String, ProjectFolderError> {
    let text = path
        .to_str()
        .ok_or(ProjectFolderError::InvalidRoot)?
        .to_owned();
    #[cfg(windows)]
    {
        let text = text
            .strip_prefix(r"\\?\UNC\")
            .map(|rest| format!(r"\\{rest}"))
            .or_else(|| text.strip_prefix(r"\\?\").map(ToOwned::to_owned))
            .unwrap_or(text);
        Ok(text.to_lowercase())
    }
    #[cfg(not(windows))]
    {
        Ok(text)
    }
}

fn directory_is_empty(dir: &Dir) -> Result<bool, ProjectFolderError> {
    let mut entries = dir
        .entries()
        .map_err(|source| ProjectFolderError::io("list_project_root", source))?;
    match entries.next() {
        None => Ok(true),
        Some(Ok(_)) => Ok(false),
        Some(Err(source)) => Err(ProjectFolderError::io("read_project_root_entry", source)),
    }
}

fn inspect_git(root: &Dir) -> Result<GitLayout, ProjectFolderError> {
    let Some(metadata) = optional_symlink_metadata(root, OsStr::new(".git"))? else {
        return Ok(GitLayout::None);
    };
    reject_link_metadata(&metadata)?;
    if metadata.is_dir() {
        open_required_directory(root, OsStr::new(".git"))?;
        return Ok(GitLayout::Directory);
    }
    if !metadata.is_file() {
        return Ok(GitLayout::Invalid);
    }
    let bytes = read_bounded_regular(root, OsStr::new(".git"), GIT_WORKTREE_FILE_LIMIT)?;
    if bytes
        .split(|byte| *byte == b'\n')
        .next()
        .is_some_and(|line| line.starts_with(b"gitdir:"))
    {
        Ok(GitLayout::WorktreeFile)
    } else {
        Ok(GitLayout::Invalid)
    }
}

fn inspect_portable_metadata(root: &Dir) -> Result<PortableProjectMetadata, ProjectFolderError> {
    let Some(dennett_metadata) = optional_symlink_metadata(root, OsStr::new(".dennett"))? else {
        return Ok(PortableProjectMetadata::Absent);
    };
    reject_link_metadata(&dennett_metadata)?;
    if !dennett_metadata.is_dir() {
        return Ok(PortableProjectMetadata::Invalid);
    }
    let dennett = open_required_directory(root, OsStr::new(".dennett"))?;
    read_project_metadata_from(&dennett)
}

fn read_project_metadata_from(
    dennett: &Dir,
) -> Result<PortableProjectMetadata, ProjectFolderError> {
    let Some(metadata) = optional_symlink_metadata(dennett, OsStr::new(PROJECT_METADATA_PATH))?
    else {
        return Ok(PortableProjectMetadata::Absent);
    };
    reject_link_metadata(&metadata)?;
    if !metadata.is_file() || metadata.len() > PROJECT_METADATA_LIMIT {
        return Ok(PortableProjectMetadata::Invalid);
    }
    let bytes = read_bounded_regular(
        dennett,
        OsStr::new(PROJECT_METADATA_PATH),
        PROJECT_METADATA_LIMIT,
    )?;
    classify_project_metadata(&bytes)
}

fn classify_project_metadata(bytes: &[u8]) -> Result<PortableProjectMetadata, ProjectFolderError> {
    let value: serde_json::Value = match serde_json::from_slice(bytes) {
        Ok(value) => value,
        Err(_) => return Ok(PortableProjectMetadata::Invalid),
    };
    let Some(version) = value.get("format_version").and_then(|value| value.as_u64()) else {
        return Ok(PortableProjectMetadata::Invalid);
    };
    if version != 1 {
        return Ok(PortableProjectMetadata::Unsupported {
            format_version: version,
        });
    }
    let metadata: PortableMetadataV1 = match serde_json::from_value(value) {
        Ok(metadata) => metadata,
        Err(_) => return Ok(PortableProjectMetadata::Invalid),
    };
    if metadata.project_id.is_nil() {
        return Ok(PortableProjectMetadata::Invalid);
    }
    Ok(PortableProjectMetadata::Valid {
        project_id: metadata.project_id,
    })
}

fn inspect_shared_memory(root: &Dir) -> Result<SharedMemoryPack, ProjectFolderError> {
    let Some(dennett_metadata) = optional_symlink_metadata(root, OsStr::new(".dennett"))? else {
        return Ok(SharedMemoryPack::Absent);
    };
    reject_link_metadata(&dennett_metadata)?;
    if !dennett_metadata.is_dir() {
        return Ok(SharedMemoryPack::Absent);
    }
    let dennett = open_required_directory(root, OsStr::new(".dennett"))?;
    let Some(memory_metadata) = optional_symlink_metadata(&dennett, OsStr::new("memory"))? else {
        return Ok(SharedMemoryPack::Absent);
    };
    reject_link_metadata(&memory_metadata)?;
    if !memory_metadata.is_dir() {
        return Ok(SharedMemoryPack::Absent);
    }
    let memory = open_required_directory(&dennett, OsStr::new("memory"))?;
    let Some(manifest_metadata) = optional_symlink_metadata(&memory, OsStr::new("manifest.yaml"))?
    else {
        return Ok(SharedMemoryPack::Absent);
    };
    reject_link_metadata(&manifest_metadata)?;
    if !manifest_metadata.is_file() || manifest_metadata.len() > MEMORY_MANIFEST_LIMIT {
        return Ok(SharedMemoryPack::Absent);
    }
    // Open no-follow and read bounded so a swapped link cannot be accepted.
    read_bounded_regular(&memory, OsStr::new("manifest.yaml"), MEMORY_MANIFEST_LIMIT)?;
    Ok(SharedMemoryPack::ManifestPresent)
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PortableMetadataV1 {
    format_version: u64,
    project_id: Uuid,
}

fn serialize_project_metadata(project_id: Uuid) -> Result<Vec<u8>, ProjectFolderError> {
    let mut bytes = serde_json::to_vec_pretty(&PortableMetadataV1 {
        format_version: 1,
        project_id,
    })
    .map_err(|source| {
        ProjectFolderError::io("serialize_project_metadata", io::Error::other(source))
    })?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn validate_project_id(project_id: Uuid) -> Result<(), ProjectFolderError> {
    if project_id.is_nil() {
        Err(ProjectFolderError::InvalidProjectId)
    } else {
        Ok(())
    }
}

fn preflight_minimal_structure(root: &Dir, project_id: Uuid) -> Result<(), ProjectFolderError> {
    match inspect_portable_metadata(root)? {
        PortableProjectMetadata::Absent => {}
        PortableProjectMetadata::Valid {
            project_id: existing,
        } if existing == project_id => {}
        _ => return Err(ProjectFolderError::PortableMetadataConflict),
    }

    let Some(dennett_metadata) = optional_symlink_metadata(root, OsStr::new(".dennett"))? else {
        return Ok(());
    };
    reject_link_metadata(&dennett_metadata)?;
    if !dennett_metadata.is_dir() {
        return Err(ProjectFolderError::PortableMetadataConflict);
    }
    let dennett = open_required_directory(root, OsStr::new(".dennett"))?;
    let Some(memory_metadata) = optional_symlink_metadata(&dennett, OsStr::new("memory"))? else {
        return Ok(());
    };
    reject_link_metadata(&memory_metadata)?;
    if !memory_metadata.is_dir() {
        return Err(ProjectFolderError::PortableMetadataConflict);
    }
    let memory = open_required_directory(&dennett, OsStr::new("memory"))?;
    if let Some(readme_metadata) = optional_symlink_metadata(&memory, OsStr::new("README.md"))? {
        reject_link_metadata(&readme_metadata)?;
        if !readme_metadata.is_file() {
            return Err(ProjectFolderError::PortableMetadataConflict);
        }
    }
    Ok(())
}

fn ensure_project_metadata(dennett: &Dir, project_id: Uuid) -> Result<bool, ProjectFolderError> {
    match read_project_metadata_from(dennett)? {
        PortableProjectMetadata::Absent => {
            let bytes = serialize_project_metadata(project_id)?;
            match atomic_publish_new_regular_file(
                dennett,
                OsStr::new(PROJECT_METADATA_PATH),
                &bytes,
            ) {
                Ok(created) => Ok(created),
                Err(ProjectFolderError::PortableMetadataConflict) => {
                    if read_project_metadata_from(dennett)?
                        == (PortableProjectMetadata::Valid { project_id })
                    {
                        Ok(false)
                    } else {
                        Err(ProjectFolderError::PortableMetadataConflict)
                    }
                }
                Err(error) => Err(error),
            }
        }
        PortableProjectMetadata::Valid {
            project_id: existing,
        } if existing == project_id => Ok(false),
        _ => Err(ProjectFolderError::PortableMetadataConflict),
    }
}

fn ensure_regular_file(dir: &Dir, name: &OsStr, bytes: &[u8]) -> Result<bool, ProjectFolderError> {
    match optional_symlink_metadata(dir, name)? {
        None => atomic_publish_new_regular_file(dir, name, bytes),
        Some(metadata) => {
            reject_link_metadata(&metadata)?;
            if metadata.is_file() {
                Ok(false)
            } else {
                Err(ProjectFolderError::PortableMetadataConflict)
            }
        }
    }
}

fn open_or_create_directory(parent: &Dir, name: &OsStr) -> Result<Dir, ProjectFolderError> {
    let created = match optional_symlink_metadata(parent, name)? {
        Some(metadata) => {
            reject_link_metadata(&metadata)?;
            if !metadata.is_dir() {
                return Err(ProjectFolderError::PortableMetadataConflict);
            }
            false
        }
        None => match parent.create_dir(name) {
            Ok(()) => true,
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => false,
            Err(source) => {
                return Err(ProjectFolderError::io(
                    "create_project_metadata_directory",
                    source,
                ));
            }
        },
    };
    if created {
        sync_directory(parent, "sync_project_metadata_parent")?;
    }
    open_required_directory(parent, name)
}

fn open_required_directory(parent: &Dir, name: &OsStr) -> Result<Dir, ProjectFolderError> {
    parent.open_dir_nofollow(name).map_err(map_nofollow_error)
}

fn optional_symlink_metadata(
    dir: &Dir,
    name: &OsStr,
) -> Result<Option<Metadata>, ProjectFolderError> {
    match dir.symlink_metadata(name) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(ProjectFolderError::io("inspect_project_entry", source)),
    }
}

fn reject_link_metadata(metadata: &Metadata) -> Result<(), ProjectFolderError> {
    if metadata.file_type().is_symlink() {
        Err(ProjectFolderError::LinkedEntry)
    } else {
        Ok(())
    }
}

fn read_bounded_regular(
    dir: &Dir,
    name: &OsStr,
    max_bytes: u64,
) -> Result<Vec<u8>, ProjectFolderError> {
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let mut file = dir
        .open_with(name, &options)
        .map_err(|source| ProjectFolderError::io("open_project_file_nofollow", source))?;
    let metadata = file
        .metadata()
        .map_err(|source| ProjectFolderError::io("read_project_file_metadata", source))?;
    if !metadata.is_file() {
        return Err(ProjectFolderError::LinkedEntry);
    }
    if metadata.len() > max_bytes {
        return Err(ProjectFolderError::io(
            "read_bounded_project_file",
            io::Error::new(
                io::ErrorKind::FileTooLarge,
                "bounded project file is too large",
            ),
        ));
    }
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    Read::by_ref(&mut file)
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|source| ProjectFolderError::io("read_bounded_project_file", source))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max_bytes {
        return Err(ProjectFolderError::io(
            "read_bounded_project_file",
            io::Error::new(
                io::ErrorKind::FileTooLarge,
                "bounded project file is too large",
            ),
        ));
    }
    Ok(bytes)
}

fn scan_instructions(
    root: &Dir,
    limits: InspectionLimits,
) -> Result<InstructionInspection, ProjectFolderError> {
    let mut state = InstructionScanState::default();
    scan_instruction_directory(root, Path::new(""), 0, limits, &mut state)?;
    state.finish()
}

fn empty_instruction_inspection() -> InstructionInspection {
    InstructionInspection {
        sources: Vec::new(),
        aggregate_sha256: None,
        completeness: InspectionCompleteness::Complete,
    }
}

#[derive(Default)]
struct InstructionScanState {
    entries: usize,
    bytes: u64,
    files: Vec<HashedInstruction>,
    limits_reached: BTreeSet<InspectionLimitReached>,
    stop: bool,
}

struct HashedInstruction {
    path: String,
    bytes: Vec<u8>,
}

impl InstructionScanState {
    fn finish(mut self) -> Result<InstructionInspection, ProjectFolderError> {
        self.files.sort_by(|left, right| left.path.cmp(&right.path));
        let aggregate_sha256 = if self.files.is_empty() {
            None
        } else {
            let mut hasher = Sha256::new();
            for source in &self.files {
                let path = source.path.as_bytes();
                hasher.update(u64::try_from(path.len()).unwrap_or(u64::MAX).to_le_bytes());
                hasher.update(path);
                hasher.update(
                    u64::try_from(source.bytes.len())
                        .unwrap_or(u64::MAX)
                        .to_le_bytes(),
                );
                hasher.update(&source.bytes);
            }
            let digest = hasher.finalize();
            Some(hex_sha256(&digest))
        };
        let sources = self
            .files
            .into_iter()
            .map(|file| InstructionSource {
                relative_path: file.path,
                byte_len: u64::try_from(file.bytes.len()).unwrap_or(u64::MAX),
            })
            .collect();
        let completeness = if self.limits_reached.is_empty() {
            InspectionCompleteness::Complete
        } else {
            InspectionCompleteness::Incomplete(self.limits_reached.into_iter().collect())
        };
        Ok(InstructionInspection {
            sources,
            aggregate_sha256,
            completeness,
        })
    }
}

fn scan_instruction_directory(
    dir: &Dir,
    relative: &Path,
    depth: usize,
    limits: InspectionLimits,
    state: &mut InstructionScanState,
) -> Result<(), ProjectFolderError> {
    if state.stop {
        return Ok(());
    }
    let remaining_entries = limits.max_entries.saturating_sub(state.entries);
    if remaining_entries == 0 {
        state.limits_reached.insert(InspectionLimitReached::Entries);
        state.stop = true;
        return Ok(());
    }
    let entries = dir
        .entries()
        .map_err(|source| ProjectFolderError::io("list_instruction_directory", source))?;
    let mut names = entries
        .take(remaining_entries.saturating_add(1))
        .map(|entry| {
            entry
                .map(|entry| entry.file_name())
                .map_err(|source| ProjectFolderError::io("read_instruction_entry", source))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if names.len() > remaining_entries {
        state.limits_reached.insert(InspectionLimitReached::Entries);
        state.stop = true;
        return Ok(());
    }
    names.sort();

    for name in names {
        state.entries += 1;
        let child_relative = relative.join(&name);
        let metadata = dir
            .symlink_metadata(&name)
            .map_err(|source| ProjectFolderError::io("inspect_instruction_entry", source))?;
        if metadata.file_type().is_symlink() {
            if is_instruction_candidate(&child_relative) || is_protected_container(&child_relative)
            {
                return Err(ProjectFolderError::LinkedEntry);
            }
            continue;
        }
        if metadata.is_dir() {
            if should_skip_directory(&child_relative) {
                continue;
            }
            if depth >= limits.max_depth {
                state.limits_reached.insert(InspectionLimitReached::Depth);
                continue;
            }
            let child = match dir.open_dir_nofollow(&name) {
                Ok(child) => child,
                Err(source)
                    if matches!(
                        source.kind(),
                        io::ErrorKind::InvalidInput | io::ErrorKind::NotADirectory
                    ) =>
                {
                    return Err(ProjectFolderError::LinkedEntry);
                }
                Err(source) => {
                    return Err(ProjectFolderError::io(
                        "open_instruction_directory_nofollow",
                        source,
                    ));
                }
            };
            scan_instruction_directory(&child, &child_relative, depth + 1, limits, state)?;
            continue;
        }
        if !metadata.is_file() || !is_instruction_candidate(&child_relative) {
            continue;
        }
        let Some(path) = portable_relative_path(&child_relative) else {
            state
                .limits_reached
                .insert(InspectionLimitReached::NonUnicodePath);
            continue;
        };
        if metadata.len() > limits.max_instruction_file_bytes {
            state
                .limits_reached
                .insert(InspectionLimitReached::InstructionFileBytes);
            continue;
        }
        if state.bytes.saturating_add(metadata.len()) > limits.max_instruction_bytes {
            state
                .limits_reached
                .insert(InspectionLimitReached::InstructionTotalBytes);
            state.stop = true;
            break;
        }
        let bytes = read_bounded_regular(dir, &name, limits.max_instruction_file_bytes)?;
        state.bytes = state.bytes.saturating_add(bytes.len() as u64);
        state.files.push(HashedInstruction { path, bytes });
    }
    Ok(())
}

fn is_instruction_candidate(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(OsStr::to_str) else {
        return false;
    };
    name.eq_ignore_ascii_case("AGENTS.md")
        || name.eq_ignore_ascii_case("CLAUDE.md")
        || path_is_under_claude_rules(path)
}

fn path_is_under_claude_rules(path: &Path) -> bool {
    let components = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>();
    components
        .windows(2)
        .any(|pair| pair[0].eq_ignore_ascii_case(".claude") && pair[1] == "rules")
}

fn is_protected_container(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(
            component,
            Component::Normal(name)
                if name == OsStr::new(".dennett")
                    || name == OsStr::new(".git")
                    || name == OsStr::new(".claude")
        )
    })
}

fn should_skip_directory(path: &Path) -> bool {
    let Some(name) = path.file_name() else {
        return false;
    };
    [
        ".git",
        ".dennett",
        "node_modules",
        "target",
        ".venv",
        "vendor",
        "dist",
        "build",
    ]
    .iter()
    .any(|candidate| name == OsStr::new(candidate))
}

fn portable_relative_path(path: &Path) -> Option<String> {
    path.to_str().map(|path| path.replace('\\', "/"))
}

fn hex_sha256(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn atomic_publish_new_regular_file(
    dir: &Dir,
    target: &OsStr,
    bytes: &[u8],
) -> Result<bool, ProjectFolderError> {
    if optional_symlink_metadata(dir, target)?.is_some() {
        return Err(ProjectFolderError::PortableMetadataConflict);
    }
    let temporary = temporary_name(target);
    write_new_temporary_file(dir, &temporary, bytes)?;

    // A capability-relative hard-link publishes a fully written inode and is
    // atomic/no-replace on both supported OS families. Removing the temporary
    // name leaves the target as the sole tracked name.
    let publish = dir.hard_link(&temporary, dir, target);
    let cleanup = dir.remove_file(&temporary);
    match publish {
        Ok(()) => {
            cleanup.map_err(|source| ProjectFolderError::io("remove_project_temp", source))?;
            sync_published_regular_file(dir, target)?;
            sync_directory(dir, "sync_published_project_directory")?;
            Ok(true)
        }
        Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {
            let _ = cleanup;
            Err(ProjectFolderError::PortableMetadataConflict)
        }
        Err(source) => {
            let _ = cleanup;
            Err(ProjectFolderError::io("publish_project_file", source))
        }
    }
}

fn atomic_replace_regular_file(
    dir: &Dir,
    target: &OsStr,
    bytes: &[u8],
) -> Result<(), ProjectFolderError> {
    let temporary = temporary_name(target);
    write_new_temporary_file(dir, &temporary, bytes)?;
    match atomic_replace_platform(dir, &temporary, target) {
        Ok(()) => {
            sync_directory(dir, "sync_replaced_project_directory")?;
            Ok(())
        }
        Err(error) => {
            let _ = dir.remove_file(&temporary);
            Err(error)
        }
    }
}

fn temporary_name(target: &OsStr) -> OsString {
    let mut name = OsString::from(".");
    name.push(target);
    name.push(".");
    name.push(Uuid::now_v7().to_string());
    name.push(".tmp");
    name
}

fn write_new_temporary_file(
    dir: &Dir,
    name: &OsStr,
    bytes: &[u8],
) -> Result<(), ProjectFolderError> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .create_new(true)
        .follow(FollowSymlinks::No);
    configure_replaceable_file(&mut options);
    let mut file = dir
        .open_with(name, &options)
        .map_err(|source| ProjectFolderError::io("create_project_temp", source))?;
    file.write_all(bytes)
        .map_err(|source| ProjectFolderError::io("write_project_temp", source))?;
    file.sync_all()
        .map_err(|source| ProjectFolderError::io("sync_project_temp", source))?;
    Ok(())
}

#[cfg(unix)]
fn configure_replaceable_file(options: &mut OpenOptions) {
    use cap_std::fs::OpenOptionsExt;
    options.mode(0o644).sync(true);
}

#[cfg(windows)]
fn configure_replaceable_file(options: &mut OpenOptions) {
    use cap_std::fs::OpenOptionsExt;
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
fn configure_replaceable_file(_options: &mut OpenOptions) {}

#[cfg(windows)]
fn sync_published_regular_file(dir: &Dir, target: &OsStr) -> Result<(), ProjectFolderError> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).follow(FollowSymlinks::No);
    configure_replaceable_file(&mut options);
    dir.open_with(target, &options)
        .and_then(|file| file.sync_all())
        .map_err(|source| ProjectFolderError::io("sync_published_project_file", source))
}

#[cfg(not(windows))]
fn sync_published_regular_file(_dir: &Dir, _target: &OsStr) -> Result<(), ProjectFolderError> {
    Ok(())
}

#[cfg(unix)]
fn sync_directory(dir: &Dir, operation: &'static str) -> Result<(), ProjectFolderError> {
    // `cap_std::fs::Dir` may intentionally hold an `O_PATH` descriptor on
    // Linux. That handle is safe for capability-relative traversal but cannot
    // itself be fsynced (`EBADF`). Reopen `.` relative to the already verified
    // capability as an ordinary read-only directory handle; this preserves the
    // no-path-re-resolution boundary while making namespace durability real.
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    dir.open_with(Path::new("."), &options)
        .and_then(|file| file.sync_all())
        .map_err(|source| ProjectFolderError::io(operation, source))
}

#[cfg(windows)]
fn sync_directory(_dir: &Dir, _operation: &'static str) -> Result<(), ProjectFolderError> {
    // Windows does not expose a portable directory-fsync equivalent. Files
    // participating in publication are opened write-through and flushed
    // after link/rename instead; NTFS then journals the namespace change.
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn sync_directory(dir: &Dir, operation: &'static str) -> Result<(), ProjectFolderError> {
    dir.try_clone()
        .and_then(|clone| clone.into_std_file().sync_all())
        .map_err(|source| ProjectFolderError::io(operation, source))
}

#[cfg(not(windows))]
fn atomic_replace_platform(
    dir: &Dir,
    temporary: &OsStr,
    target: &OsStr,
) -> Result<(), ProjectFolderError> {
    dir.rename(temporary, dir, target)
        .map_err(|source| ProjectFolderError::io("replace_project_identity", source))
}

#[cfg(windows)]
fn atomic_replace_platform(
    dir: &Dir,
    temporary: &OsStr,
    target: &OsStr,
) -> Result<(), ProjectFolderError> {
    use std::{mem, os::windows::ffi::OsStrExt, os::windows::io::AsRawHandle, ptr};
    use windows_sys::Win32::{
        Foundation::{HANDLE, RtlNtStatusToDosError},
        Storage::FileSystem::FILE_RENAME_INFO,
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
    configure_replaceable_file(&mut options);
    let file = dir
        .open_with(temporary, &options)
        .map_err(|source| ProjectFolderError::io("open_project_temp_for_replace", source))?;
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
    // SAFETY: `storage` is aligned and large enough for the fixed header and
    // UTF-16 target copied below. Both handles remain alive for the call.
    unsafe {
        (*info).Anonymous.ReplaceIfExists = true;
        (*info).RootDirectory = dir.as_raw_handle().cast::<core::ffi::c_void>() as HANDLE;
        (*info).FileNameLength =
            u32::try_from(target_wide.len() * 2).map_err(|_| ProjectFolderError::InvalidRoot)?;
        ptr::copy_nonoverlapping(
            target_wide.as_ptr(),
            ptr::addr_of_mut!((*info).FileName).cast::<u16>(),
            target_wide.len(),
        );
        let status = NtSetInformationFile(
            file.as_raw_handle().cast::<core::ffi::c_void>() as HANDLE,
            &mut io_status,
            info.cast(),
            u32::try_from(byte_len).map_err(|_| ProjectFolderError::InvalidRoot)?,
            10, // FileRenameInformation
        );
        if status < 0 {
            return Err(ProjectFolderError::io(
                "replace_project_identity",
                io::Error::from_raw_os_error(RtlNtStatusToDosError(status) as i32),
            ));
        }
    }
    file.sync_all()
        .map_err(|source| ProjectFolderError::io("sync_replaced_project_file", source))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn adapter() -> ProjectFolderAdapter {
        ProjectFolderAdapter::default()
    }

    fn inspect(root: &Path) -> ProjectFolderInspection {
        adapter()
            .inspect(root, ProjectFolderIntent::AttachExisting)
            .expect("inspection succeeds")
    }

    fn source_identity(inspection: &ProjectFolderInspection) -> SourceIdentity {
        inspection
            .source_identity
            .expect("existing project has source identity")
    }

    #[test]
    fn inspection_does_not_mutate_project() {
        let temp = TempDir::new().expect("tempdir");
        fs::write(temp.path().join("source.txt"), b"unchanged").expect("fixture");

        let first = inspect(temp.path());
        let second = inspect(temp.path());

        assert!(first.location_exists);
        assert!(!first.location_empty);
        assert_eq!(first, second);
        assert_eq!(
            fs::read(temp.path().join("source.txt")).unwrap(),
            b"unchanged"
        );
        assert!(!temp.path().join(".dennett").exists());
    }

    #[test]
    fn inspects_and_idempotently_creates_absent_empty_child() {
        let temp = TempDir::new().expect("tempdir");
        let selected = temp.path().join("new-project");

        let prospective = adapter()
            .inspect(&selected, ProjectFolderIntent::CreateEmpty)
            .expect("prospective inspection");

        assert!(!prospective.location_exists);
        assert!(prospective.location_empty);
        assert_eq!(prospective.source_identity, None);
        assert!(!selected.exists());
        let precondition = CreateEmptyPrecondition::Absent {
            parent_source_identity: prospective
                .prospective_parent_identity
                .expect("parent identity"),
        };

        let created = adapter()
            .create_empty_root(&selected, precondition)
            .expect("create selected directory");
        assert!(created.created);
        assert!(selected.is_dir());
        assert!(directory_is_empty(&open_absolute_directory_nofollow(&selected).unwrap()).unwrap());

        let repeated = adapter()
            .create_empty_root(&selected, precondition)
            .expect("idempotent retry");
        assert!(!repeated.created);
        assert_eq!(repeated.source_identity, created.source_identity);
        assert_eq!(repeated.canonical_location, created.canonical_location);
    }

    #[test]
    fn create_empty_rejects_existing_non_empty_location() {
        let temp = TempDir::new().expect("tempdir");
        let selected = temp.path().join("project");
        fs::create_dir(&selected).unwrap();
        let existing = inspect(&selected);
        fs::write(selected.join("content.txt"), b"content").unwrap();

        assert!(matches!(
            adapter().inspect(&selected, ProjectFolderIntent::CreateEmpty),
            Err(ProjectFolderError::CreateRootNotEmpty)
        ));
        assert!(matches!(
            adapter().create_empty_root(
                &selected,
                CreateEmptyPrecondition::ExistingEmpty {
                    source_identity: source_identity(&existing),
                },
            ),
            Err(ProjectFolderError::CreateRootNotEmpty)
        ));
        assert_eq!(fs::read(selected.join("content.txt")).unwrap(), b"content");
    }

    #[test]
    fn creates_only_minimal_portable_structure() {
        let temp = TempDir::new().expect("tempdir");
        let before = inspect(temp.path());
        let project_id = Uuid::now_v7();

        let effect = adapter()
            .create_minimal(temp.path(), source_identity(&before), project_id)
            .expect("minimal structure");

        assert_eq!(
            effect,
            MinimalStructureEffect {
                metadata_created: true,
                readme_created: true,
            }
        );
        let after = inspect(temp.path());
        assert_eq!(
            after.portable_metadata,
            PortableProjectMetadata::Valid { project_id }
        );
        assert_eq!(after.shared_memory, SharedMemoryPack::Absent);
        assert!(temp.path().join(".dennett/memory/README.md").is_file());
        assert!(!temp.path().join(".dennett/memory/manifest.yaml").exists());
        let paths = relative_tree(temp.path());
        assert_eq!(
            paths,
            vec![
                ".dennett/".to_owned(),
                ".dennett/memory/".to_owned(),
                ".dennett/memory/README.md".to_owned(),
                ".dennett/project.json".to_owned(),
            ]
        );
    }

    #[test]
    fn minimal_creation_is_idempotent() {
        let temp = TempDir::new().expect("tempdir");
        let before = inspect(temp.path());
        let project_id = Uuid::now_v7();
        adapter()
            .create_minimal(temp.path(), source_identity(&before), project_id)
            .unwrap();
        let metadata_before = fs::read(temp.path().join(".dennett/project.json")).unwrap();

        let repeated = adapter()
            .create_minimal(temp.path(), source_identity(&before), project_id)
            .unwrap();

        assert_eq!(
            repeated,
            MinimalStructureEffect {
                metadata_created: false,
                readme_created: false,
            }
        );
        assert_eq!(
            fs::read(temp.path().join(".dennett/project.json")).unwrap(),
            metadata_before
        );
    }

    #[test]
    fn conflicting_or_invalid_metadata_is_preserved() {
        let temp = TempDir::new().expect("tempdir");
        fs::create_dir(temp.path().join(".dennett")).unwrap();
        let path = temp.path().join(".dennett/project.json");
        fs::write(&path, b"not-json").unwrap();
        let before = inspect(temp.path());

        let error = adapter()
            .create_minimal(temp.path(), source_identity(&before), Uuid::now_v7())
            .expect_err("invalid metadata is not overwritten");

        assert!(matches!(
            error,
            ProjectFolderError::PortableMetadataConflict
        ));
        assert_eq!(fs::read(path).unwrap(), b"not-json");
        assert!(!temp.path().join(".dennett/memory").exists());
    }

    #[test]
    fn preflight_conflict_does_not_leave_partial_minimal_structure() {
        let temp = TempDir::new().expect("tempdir");
        fs::create_dir_all(temp.path().join(".dennett/memory/README.md")).unwrap();
        let before = inspect(temp.path());

        let error = adapter()
            .create_minimal(temp.path(), source_identity(&before), Uuid::now_v7())
            .expect_err("conflicting readme shape is rejected before mutation");

        assert!(matches!(
            error,
            ProjectFolderError::PortableMetadataConflict
        ));
        assert!(temp.path().join(".dennett/memory/README.md").is_dir());
        assert!(!temp.path().join(".dennett/project.json").exists());
    }

    #[test]
    fn lexical_alias_has_same_identity_and_binding_key() {
        let temp = TempDir::new().expect("tempdir");
        fs::create_dir(temp.path().join("child")).unwrap();
        let direct = inspect(temp.path());
        let alias = inspect(&temp.path().join("child").join(".."));

        assert_eq!(direct.source_identity, alias.source_identity);
        assert_eq!(direct.canonical_location, alias.canonical_location);
    }

    #[test]
    fn fingerprint_changes_only_for_instruction_files() {
        let temp = TempDir::new().expect("tempdir");
        fs::write(temp.path().join("AGENTS.md"), b"first").unwrap();
        let first = inspect(temp.path()).instructions.aggregate_sha256.unwrap();

        fs::write(temp.path().join("ordinary.rs"), b"source changed").unwrap();
        let ordinary = inspect(temp.path()).instructions.aggregate_sha256.unwrap();
        assert_eq!(first, ordinary);

        fs::write(temp.path().join("AGENTS.md"), b"second").unwrap();
        let instruction = inspect(temp.path()).instructions.aggregate_sha256.unwrap();
        assert_ne!(ordinary, instruction);
    }

    #[test]
    fn detects_git_and_real_memory_manifest_without_treating_readme_as_pack() {
        let temp = TempDir::new().expect("tempdir");
        fs::create_dir(temp.path().join(".git")).unwrap();
        fs::create_dir_all(temp.path().join(".dennett/memory")).unwrap();
        fs::write(temp.path().join(".dennett/memory/README.md"), b"container").unwrap();
        assert_eq!(inspect(temp.path()).git, GitLayout::Directory);
        assert_eq!(inspect(temp.path()).shared_memory, SharedMemoryPack::Absent);

        fs::write(
            temp.path().join(".dennett/memory/manifest.yaml"),
            b"format_version: 1\n",
        )
        .unwrap();
        assert_eq!(
            inspect(temp.path()).shared_memory,
            SharedMemoryPack::ManifestPresent
        );
    }

    #[test]
    fn explicit_fork_rewrites_only_expected_identity() {
        let temp = TempDir::new().expect("tempdir");
        let before = inspect(temp.path());
        let original = Uuid::now_v7();
        let fork = Uuid::now_v7();
        adapter()
            .create_minimal(temp.path(), source_identity(&before), original)
            .unwrap();
        let readme = fs::read(temp.path().join(".dennett/memory/README.md")).unwrap();

        adapter()
            .rewrite_project_identity(temp.path(), source_identity(&before), original, fork)
            .expect("fork identity rewrite");

        assert_eq!(
            inspect(temp.path()).portable_metadata,
            PortableProjectMetadata::Valid { project_id: fork }
        );
        assert_eq!(
            fs::read(temp.path().join(".dennett/memory/README.md")).unwrap(),
            readme
        );
        assert!(matches!(
            adapter().rewrite_project_identity(
                temp.path(),
                source_identity(&before),
                original,
                Uuid::now_v7(),
            ),
            Err(ProjectFolderError::PortableMetadataConflict)
        ));
    }

    #[test]
    fn reports_bounded_incomplete_scan() {
        let temp = TempDir::new().expect("tempdir");
        fs::write(temp.path().join("a.txt"), b"a").unwrap();
        fs::write(temp.path().join("b.txt"), b"b").unwrap();
        let bounded = ProjectFolderAdapter::new(InspectionLimits {
            max_depth: 1,
            max_entries: 1,
            max_instruction_file_bytes: 8,
            max_instruction_bytes: 8,
        });

        let result = bounded
            .inspect(temp.path(), ProjectFolderIntent::AttachExisting)
            .unwrap();

        assert_eq!(
            result.instructions.completeness,
            InspectionCompleteness::Incomplete(vec![InspectionLimitReached::Entries])
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_root_and_instruction_source() {
        use std::os::unix::fs::symlink;
        let temp = TempDir::new().expect("tempdir");
        let target = temp.path().join("target");
        fs::create_dir(&target).unwrap();
        let root_link = temp.path().join("root-link");
        symlink(&target, &root_link).unwrap();
        assert!(matches!(
            adapter().inspect(&root_link, ProjectFolderIntent::AttachExisting),
            Err(ProjectFolderError::LinkedEntry)
        ));

        let outside = temp.path().join("outside");
        fs::write(&outside, b"outside").unwrap();
        symlink(&outside, target.join("AGENTS.md")).unwrap();
        assert!(matches!(
            adapter().inspect(&target, ProjectFolderIntent::AttachExisting),
            Err(ProjectFolderError::LinkedEntry)
        ));
    }

    #[cfg(windows)]
    #[test]
    fn rejects_windows_reparse_root_when_test_can_create_one() {
        use std::os::windows::fs::symlink_dir;
        let temp = TempDir::new().expect("tempdir");
        let target = temp.path().join("target");
        fs::create_dir(&target).unwrap();
        let root_link = temp.path().join("root-link");
        if symlink_dir(&target, &root_link).is_err() {
            // Windows may require Developer Mode or an elevated token.
            return;
        }
        assert!(matches!(
            adapter().inspect(&root_link, ProjectFolderIntent::AttachExisting),
            Err(ProjectFolderError::LinkedEntry)
        ));
    }

    fn relative_tree(root: &Path) -> Vec<String> {
        fn visit(root: &Path, current: &Path, output: &mut Vec<String>) {
            let mut entries = fs::read_dir(current)
                .unwrap()
                .map(Result::unwrap)
                .collect::<Vec<_>>();
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries {
                let path = entry.path();
                let mut relative = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                if path.is_dir() {
                    relative.push('/');
                }
                output.push(relative);
                if path.is_dir() {
                    visit(root, &path, output);
                }
            }
        }
        let mut output = Vec::new();
        visit(root, root, &mut output);
        output.sort();
        output
    }
}
