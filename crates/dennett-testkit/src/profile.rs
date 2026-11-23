//! Isolated test profile configuration and fail-closed fault-injection guard.

use std::env;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use dennett_contracts::{DennettError, ProfileMode};
use uuid::Uuid;

/// Environment variable that must equal `"1"` before fault injection or destructive
/// test-profile probes may execute.
pub const TEST_PROFILE_ENV: &str = "DENNETT_TEST_PROFILE";

/// Default isolated PostgreSQL database name for memoryd tests.
pub const DEFAULT_TEST_MEMORY_DB: &str = "dennett_memory_test";
/// Default isolated PostgreSQL database name for control-plane tests.
pub const DEFAULT_TEST_CONTROL_DB: &str = "dennett_control_test";
/// Default least-privilege PostgreSQL role for memoryd tests.
pub const DEFAULT_TEST_MEMORY_ROLE: &str = "dennett_memory_role";
/// Default least-privilege PostgreSQL role for control-plane tests.
pub const DEFAULT_TEST_CONTROL_ROLE: &str = "dennett_control_role";

/// Directory name prefix required beneath `temp_dir` when `temp_dir` overlaps `HOME`/`USERPROFILE`.
const ALLOWED_ISOLATED_PREFIXES: [&str; 2] = ["dennett-test-", "dennett-d0-"];

/// Immutable descriptor of an execution profile for tests and fault-injection probes.
///
/// Fields are private so callers cannot silently mutate paths or safety flags after
/// construction; [`FaultInjectionGuard::acquire`] still re-validates all paths against
/// live filesystem state to prevent symlink, junction, or TOCTOU bypasses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestProfile {
    mode: ProfileMode,
    root_dir: PathBuf,
    data_dir: PathBuf,
    workspace_dir: PathBuf,
    postgres_memory_database: String,
    postgres_control_database: String,
    postgres_memory_role: String,
    postgres_control_role: String,
    memoryd_port: u16,
    allow_live_external_effects: bool,
}

impl TestProfile {
    /// Builds a fresh isolated test profile rooted beneath the system temporary directory.
    ///
    /// Does not touch the filesystem; call [`Self::create_on_disk`] when a physical
    /// directory tree with automatic RAII cleanup is needed.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `label` contains unsafe path characters.
    pub fn new_isolated(label: &str) -> Result<Self, DennettError> {
        validate_label(label)?;
        let unique = format!("{label}-{}", Uuid::new_v4().simple());
        let root_dir = env::temp_dir().join("dennett-test-isolated").join(unique);
        let data_dir = root_dir.join("data");
        let workspace_dir = root_dir.join("workspace");

        Ok(Self {
            mode: ProfileMode::TestIsolated,
            root_dir,
            data_dir,
            workspace_dir,
            postgres_memory_database: String::from(DEFAULT_TEST_MEMORY_DB),
            postgres_control_database: String::from(DEFAULT_TEST_CONTROL_DB),
            postgres_memory_role: String::from(DEFAULT_TEST_MEMORY_ROLE),
            postgres_control_role: String::from(DEFAULT_TEST_CONTROL_ROLE),
            memoryd_port: 0,
            allow_live_external_effects: false,
        })
    }

    /// Creates an isolated profile on disk and returns a RAII cleanup guard.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] for invalid labels,
    /// [`DennettError::SafetyGuardRejected`] if the target path fails sandbox validation,
    /// or [`DennettError::Internal`] if directory creation fails.
    pub fn create_on_disk(label: &str) -> Result<(Self, TempProfileDir), DennettError> {
        let profile = Self::new_isolated(label)?;
        profile.validate_paths()?;
        fs::create_dir_all(&profile.data_dir).map_err(|err| DennettError::Internal {
            detail: format!("failed to create test data dir: {err}"),
        })?;
        fs::create_dir_all(&profile.workspace_dir).map_err(|err| DennettError::Internal {
            detail: format!("failed to create test workspace dir: {err}"),
        })?;
        let guard = TempProfileDir {
            root_dir: profile.root_dir.clone(),
        };
        Ok((profile, guard))
    }

    /// Constructs a production-mode descriptor for negative guard tests.
    #[must_use]
    pub fn production_like(root_dir: PathBuf) -> Self {
        let data_dir = root_dir.join("data");
        let workspace_dir = root_dir.join("workspace");
        Self {
            mode: ProfileMode::Production,
            root_dir,
            data_dir,
            workspace_dir,
            postgres_memory_database: String::from("dennett_memory"),
            postgres_control_database: String::from("dennett_control"),
            postgres_memory_role: String::from(DEFAULT_TEST_MEMORY_ROLE),
            postgres_control_role: String::from(DEFAULT_TEST_CONTROL_ROLE),
            memoryd_port: 18080,
            allow_live_external_effects: false,
        }
    }

    /// Returns the execution mode of this profile.
    #[must_use]
    pub const fn mode(&self) -> ProfileMode {
        self.mode
    }

    /// Returns the root directory of this profile.
    #[must_use]
    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    /// Returns the data directory inside this profile.
    #[must_use]
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Returns the disposable workspace directory inside this profile.
    #[must_use]
    pub fn workspace_dir(&self) -> &Path {
        &self.workspace_dir
    }

    /// Returns the PostgreSQL memory database name.
    #[must_use]
    pub fn postgres_memory_database(&self) -> &str {
        &self.postgres_memory_database
    }

    /// Returns the PostgreSQL control database name.
    #[must_use]
    pub fn postgres_control_database(&self) -> &str {
        &self.postgres_control_database
    }

    /// Returns the PostgreSQL memory role name.
    #[must_use]
    pub fn postgres_memory_role(&self) -> &str {
        &self.postgres_memory_role
    }

    /// Returns the PostgreSQL control role name.
    #[must_use]
    pub fn postgres_control_role(&self) -> &str {
        &self.postgres_control_role
    }

    /// Returns the configured memoryd bind port (`0` selects an ephemeral port).
    #[must_use]
    pub const fn memoryd_port(&self) -> u16 {
        self.memoryd_port
    }

    /// Returns whether live external effects are permitted.
    #[must_use]
    pub const fn allow_live_external_effects(&self) -> bool {
        self.allow_live_external_effects
    }

    /// Validates that `root_dir`, `data_dir`, and `workspace_dir` satisfy all sandbox,
    /// traversal, repository-isolation, and symlink/reparse-point invariants.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::SafetyGuardRejected`] if any path violates isolation rules.
    pub fn validate_paths(&self) -> Result<(), DennettError> {
        let ctx = PathValidationContext::from_process()?;
        let _ = validate_all_profile_paths(self, &ctx)?;
        Ok(())
    }
}

/// RAII cleanup handle for an isolated on-disk [`TestProfile`] directory.
///
/// Cleanup in [`Drop`] is best-effort and never panics.
#[derive(Debug)]
pub struct TempProfileDir {
    root_dir: PathBuf,
}

impl TempProfileDir {
    /// Returns the managed root directory path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.root_dir
    }
}

impl Drop for TempProfileDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root_dir);
    }
}

/// Proof token demonstrating that all isolation preconditions passed before running
/// destructive probes or fault-injection tests.
#[derive(Debug)]
pub struct FaultInjectionGuard {
    raw_root: PathBuf,
    profile_root: PathBuf,
    data_dir: PathBuf,
    workspace_dir: PathBuf,
    memory_db: String,
    control_db: String,
}

impl FaultInjectionGuard {
    /// Acquires a [`FaultInjectionGuard`] using the live process environment variable
    /// [`TEST_PROFILE_ENV`].
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::SafetyGuardRejected`] if any isolation invariant fails.
    pub fn acquire(profile: &TestProfile) -> Result<Self, DennettError> {
        let env_flag = env::var(TEST_PROFILE_ENV).ok();
        let ctx = PathValidationContext::from_process()?;
        Self::acquire_internal(profile, env_flag.as_deref(), &ctx)
    }

    fn acquire_internal(
        profile: &TestProfile,
        env_flag: Option<&str>,
        ctx: &PathValidationContext,
    ) -> Result<Self, DennettError> {
        if profile.mode != ProfileMode::TestIsolated {
            return Err(DennettError::SafetyGuardRejected {
                reason: format!(
                    "profile mode must be `test_isolated`, got `{}`",
                    profile.mode
                ),
            });
        }

        if env_flag != Some("1") {
            return Err(DennettError::SafetyGuardRejected {
                reason: format!(
                    "environment variable {TEST_PROFILE_ENV} must be set to \"1\" for isolated fault injection"
                ),
            });
        }

        if profile.allow_live_external_effects {
            return Err(DennettError::SafetyGuardRejected {
                reason: String::from(
                    "allow_live_external_effects must be false in fault-injection profiles",
                ),
            });
        }

        validate_test_database_name(
            "postgres_memory_database",
            &profile.postgres_memory_database,
        )?;
        validate_test_database_name(
            "postgres_control_database",
            &profile.postgres_control_database,
        )?;
        if profile.postgres_memory_database == profile.postgres_control_database {
            return Err(DennettError::SafetyGuardRejected {
                reason: String::from(
                    "postgres_memory_database and postgres_control_database must be distinct databases",
                ),
            });
        }

        let validated = validate_all_profile_paths(profile, ctx)?;

        Ok(Self {
            raw_root: profile.root_dir.clone(),
            profile_root: validated.root,
            data_dir: validated.data,
            workspace_dir: validated.workspace,
            memory_db: profile.postgres_memory_database.clone(),
            control_db: profile.postgres_control_database.clone(),
        })
    }

    /// Validates that `candidate` is strictly contained within this guard's isolated
    /// root directory, has no `..` traversal, and crosses no symlink or NTFS junction.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::SafetyGuardRejected`] if `candidate` escapes the profile
    /// root or traverses a symlink/reparse point.
    pub fn validate_subpath(&self, candidate: &Path) -> Result<PathBuf, DennettError> {
        validate_child_within_root(candidate, &self.raw_root, &self.profile_root, "subpath")
    }

    /// Returns the validated isolated root directory.
    #[must_use]
    pub fn profile_root(&self) -> &Path {
        &self.profile_root
    }

    /// Returns the validated isolated data directory.
    #[must_use]
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Returns the validated isolated workspace directory.
    #[must_use]
    pub fn workspace_dir(&self) -> &Path {
        &self.workspace_dir
    }

    /// Returns the validated memory test database name.
    #[must_use]
    pub fn memory_db(&self) -> &str {
        &self.memory_db
    }

    /// Returns the validated control test database name.
    #[must_use]
    pub fn control_db(&self) -> &str {
        &self.control_db
    }
}

struct ValidatedPaths {
    root: PathBuf,
    data: PathBuf,
    workspace: PathBuf,
}

struct PathValidationContext {
    temp_dir_raw: PathBuf,
    temp_dir_norm: PathBuf,
    home_dirs_norm: Vec<PathBuf>,
    cwd_norm: PathBuf,
    repo_root_norm: Option<PathBuf>,
}

impl PathValidationContext {
    fn from_process() -> Result<Self, DennettError> {
        let temp_dir_raw = env::temp_dir();
        let temp_dir_norm = canonicalize_existing_dir(&temp_dir_raw, "system temp_dir")?;

        let cwd_raw = env::current_dir().map_err(|err| DennettError::SafetyGuardRejected {
            reason: format!("failed to determine current working directory: {err}"),
        })?;
        let cwd_norm = canonicalize_existing_dir(&cwd_raw, "current working directory")?;
        let repo_root_norm = find_repo_root(&cwd_raw)
            .map(|root| canonicalize_existing_dir(&root, "repository root"))
            .transpose()?;

        let mut home_dirs_norm = Vec::new();
        for home_var in ["USERPROFILE", "HOME"] {
            if let Ok(val) = env::var(home_var)
                && !val.trim().is_empty()
            {
                let path = PathBuf::from(val);
                if let Ok(norm) = canonicalize_existing_dir(&path, home_var) {
                    home_dirs_norm.push(norm);
                }
            }
        }

        Ok(Self {
            temp_dir_raw,
            temp_dir_norm,
            home_dirs_norm,
            cwd_norm,
            repo_root_norm,
        })
    }
}

fn find_repo_root(start: &Path) -> Option<PathBuf> {
    for ancestor in start.ancestors() {
        if ancestor.join(".git").exists()
            || (ancestor.join("AGENTS.md").is_file() && ancestor.join("Cargo.toml").is_file())
        {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

fn validate_all_profile_paths(
    profile: &TestProfile,
    ctx: &PathValidationContext,
) -> Result<ValidatedPaths, DennettError> {
    let root_norm = validate_isolated_root_dir(&profile.root_dir, ctx)?;
    let data_norm =
        validate_child_within_root(&profile.data_dir, &profile.root_dir, &root_norm, "data_dir")?;
    let workspace_norm = validate_child_within_root(
        &profile.workspace_dir,
        &profile.root_dir,
        &root_norm,
        "workspace_dir",
    )?;

    if data_norm == workspace_norm {
        return Err(DennettError::SafetyGuardRejected {
            reason: String::from("data_dir and workspace_dir must be distinct subdirectories"),
        });
    }

    Ok(ValidatedPaths {
        root: root_norm,
        data: data_norm,
        workspace: workspace_norm,
    })
}

fn validate_label(label: &str) -> Result<(), DennettError> {
    if label.is_empty()
        || label.len() > 64
        || !label
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    {
        return Err(DennettError::InvalidInput {
            field: String::from("label"),
            reason: String::from("must be 1..=64 ASCII alphanumeric, '-', or '_' characters"),
        });
    }
    Ok(())
}

fn validate_test_database_name(field: &str, name: &str) -> Result<(), DennettError> {
    if name.len() <= "_test".len() || !name.ends_with("_test") {
        return Err(DennettError::SafetyGuardRejected {
            reason: format!("{field} `{name}` must end with `_test`"),
        });
    }
    if !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return Err(DennettError::SafetyGuardRejected {
            reason: format!("{field} `{name}` contains invalid database identifier characters"),
        });
    }
    Ok(())
}

fn validate_isolated_root_dir(
    root_dir: &Path,
    ctx: &PathValidationContext,
) -> Result<PathBuf, DennettError> {
    ensure_clean_absolute_path(root_dir, "root_dir")?;
    reject_symlinks_from_anchor(root_dir, &ctx.temp_dir_raw, &ctx.temp_dir_norm, "root_dir")?;

    let normalized_root =
        normalize_fail_closed(root_dir, &ctx.temp_dir_raw, &ctx.temp_dir_norm, "root_dir")?;

    if normalized_root.parent().is_none() {
        return Err(DennettError::SafetyGuardRejected {
            reason: String::from("root_dir must not be a filesystem root"),
        });
    }

    for home_norm in &ctx.home_dirs_norm {
        if normalized_root == *home_norm {
            return Err(DennettError::SafetyGuardRejected {
                reason: String::from("root_dir must not match the user home directory"),
            });
        }
    }

    if is_same_or_inside(&normalized_root, &ctx.cwd_norm) {
        return Err(DennettError::SafetyGuardRejected {
            reason: String::from(
                "root_dir must not match or reside inside the current working directory",
            ),
        });
    }

    if let Some(repo_root_norm) = &ctx.repo_root_norm
        && is_same_or_inside(&normalized_root, repo_root_norm)
    {
        return Err(DennettError::SafetyGuardRejected {
            reason: String::from(
                "root_dir must not match or reside inside the repository directory",
            ),
        });
    }

    if !is_strictly_inside(&normalized_root, &ctx.temp_dir_norm) {
        return Err(DennettError::SafetyGuardRejected {
            reason: String::from(
                "root_dir must reside strictly inside the system temporary directory",
            ),
        });
    }

    // If the system temp_dir overlaps a user home directory (e.g. TMPDIR=$HOME),
    // require an explicit `dennett-test-*` or `dennett-d0-*` top-level namespace.
    let temp_overlaps_home = ctx.home_dirs_norm.contains(&ctx.temp_dir_norm);
    if temp_overlaps_home {
        let rel = normalized_root
            .strip_prefix(&ctx.temp_dir_norm)
            .map_err(|_| DennettError::SafetyGuardRejected {
                reason: String::from("failed computing relative sandbox path under temp_dir"),
            })?;
        let first_segment = rel
            .components()
            .next()
            .and_then(|c| c.as_os_str().to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !ALLOWED_ISOLATED_PREFIXES
            .iter()
            .any(|prefix| first_segment.starts_with(prefix))
        {
            return Err(DennettError::SafetyGuardRejected {
                reason: String::from(
                    "when temp_dir overlaps HOME/USERPROFILE, root_dir must sit beneath a `dennett-test-*` or `dennett-d0-*` subdirectory",
                ),
            });
        }
    }

    Ok(normalized_root)
}

fn validate_child_within_root(
    child: &Path,
    raw_root: &Path,
    normalized_root: &Path,
    label: &str,
) -> Result<PathBuf, DennettError> {
    ensure_clean_absolute_path(child, label)?;
    reject_symlinks_from_anchor(child, raw_root, normalized_root, label)?;
    let normalized_child = normalize_fail_closed(child, raw_root, normalized_root, label)?;

    if !is_strictly_inside(&normalized_child, normalized_root) {
        return Err(DennettError::SafetyGuardRejected {
            reason: format!("{label} must reside strictly inside root_dir"),
        });
    }
    Ok(normalized_child)
}

fn ensure_clean_absolute_path(path: &Path, label: &str) -> Result<(), DennettError> {
    if !path.is_absolute() {
        return Err(DennettError::SafetyGuardRejected {
            reason: format!("{label} must be an absolute path inside an isolated sandbox"),
        });
    }
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(DennettError::SafetyGuardRejected {
            reason: format!("{label} must not contain parent directory traversal (`..`)"),
        });
    }
    Ok(())
}

/// Checks whether filesystem metadata describes a symlink or Windows reparse point (NTFS junction).
#[cfg(windows)]
fn is_symlink_or_reparse_point(meta: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    meta.file_type().is_symlink() || (meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
}

/// Checks whether filesystem metadata describes a symlink.
#[cfg(not(windows))]
fn is_symlink_or_reparse_point(meta: &fs::Metadata) -> bool {
    meta.file_type().is_symlink()
}

/// Checks every path prefix strictly below `anchor_raw` / `anchor_norm` using
/// `symlink_metadata` so live and dangling symlinks (and Windows NTFS junctions) are
/// rejected fail-closed while allowing an OS-level symlink/junction above `anchor_raw`
/// (such as macOS `/var -> /private/var` or a redirected `TEMP`).
fn reject_symlinks_from_anchor(
    candidate: &Path,
    anchor_raw: &Path,
    anchor_norm: &Path,
    label: &str,
) -> Result<(), DennettError> {
    let mut to_check: Vec<&Path> = Vec::new();
    for ancestor in candidate.ancestors() {
        if paths_lexically_equal(ancestor, anchor_raw)
            || paths_lexically_equal(ancestor, anchor_norm)
        {
            break;
        }
        to_check.push(ancestor);
    }

    for path in to_check.into_iter().rev() {
        match fs::symlink_metadata(path) {
            Ok(meta) => {
                if is_symlink_or_reparse_point(&meta) {
                    return Err(DennettError::SafetyGuardRejected {
                        reason: format!(
                            "{label} contains a symlink or reparse point and is rejected fail-closed"
                        ),
                    });
                }
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => {
                return Err(DennettError::SafetyGuardRejected {
                    reason: format!("failed inspecting metadata for {label} (fail-closed): {err}"),
                });
            }
        }
    }
    Ok(())
}

fn canonicalize_existing_dir(path: &Path, label: &str) -> Result<PathBuf, DennettError> {
    let canonical = fs::canonicalize(path).map_err(|err| DennettError::SafetyGuardRejected {
        reason: format!("failed to canonicalize {label} (fail-closed): {err}"),
    })?;
    Ok(normalize_platform_path(&canonical))
}

/// Normalizes a path by locating the longest existing ancestor via `symlink_metadata`,
/// rejecting symlinks/junctions strictly below `anchor_raw`/`anchor_norm`, and
/// canonicalizing that ancestor without any silent fallback.
fn normalize_fail_closed(
    path: &Path,
    anchor_raw: &Path,
    anchor_norm: &Path,
    label: &str,
) -> Result<PathBuf, DennettError> {
    let mut existing = path.to_path_buf();
    let mut tail: Vec<std::ffi::OsString> = Vec::new();

    let anchor_raw_norm = normalize_platform_path(anchor_raw);
    let anchor_norm_clean = normalize_platform_path(anchor_norm);

    loop {
        match fs::symlink_metadata(&existing) {
            Ok(meta) => {
                let existing_clean = normalize_platform_path(&existing);
                let strictly_below_anchor = is_strictly_inside(&existing_clean, &anchor_raw_norm)
                    || is_strictly_inside(&existing_clean, &anchor_norm_clean);
                if strictly_below_anchor && is_symlink_or_reparse_point(&meta) {
                    return Err(DennettError::SafetyGuardRejected {
                        reason: format!(
                            "{label} points through a symlink or reparse point and is rejected"
                        ),
                    });
                }
                break;
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                let Some(name) = existing.file_name() else {
                    return Err(DennettError::SafetyGuardRejected {
                        reason: format!("no existing filesystem root found for {label}"),
                    });
                };
                tail.push(name.to_os_string());
                if !existing.pop() {
                    return Err(DennettError::SafetyGuardRejected {
                        reason: format!("could not resolve existing ancestor for {label}"),
                    });
                }
            }
            Err(err) => {
                return Err(DennettError::SafetyGuardRejected {
                    reason: format!("metadata check failed for {label}: {err}"),
                });
            }
        }
    }

    let mut resolved =
        fs::canonicalize(&existing).map_err(|err| DennettError::SafetyGuardRejected {
            reason: format!("failed to canonicalize ancestor of {label} (fail-closed): {err}"),
        })?;
    for segment in tail.into_iter().rev() {
        resolved.push(segment);
    }

    Ok(normalize_platform_path(&resolved))
}

fn is_strictly_inside(candidate: &Path, allowed_parent: &Path) -> bool {
    candidate != allowed_parent && candidate.starts_with(allowed_parent)
}

fn is_same_or_inside(candidate: &Path, target: &Path) -> bool {
    candidate == target || candidate.starts_with(target)
}

fn paths_lexically_equal(a: &Path, b: &Path) -> bool {
    normalize_platform_path(a) == normalize_platform_path(b)
}

fn normalize_platform_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let raw = path.to_string_lossy();
        let stripped = raw.strip_prefix(r"\\?\").unwrap_or(&raw);
        let trimmed = stripped.trim_end_matches(['\\', '/']);
        if trimmed.len() == 2 && trimmed.as_bytes()[1] == b':' {
            PathBuf::from(format!("{}\\", trimmed.to_ascii_lowercase()))
        } else {
            PathBuf::from(trimmed.to_ascii_lowercase())
        }
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx() -> PathValidationContext {
        match PathValidationContext::from_process() {
            Ok(c) => c,
            Err(err) => panic!("failed creating process validation context: {err}"),
        }
    }

    /// Creates an unprivileged directory link (NTFS junction on Windows, symlink on Unix).
    fn create_unprivileged_dir_link(link_path: &Path, target_path: &Path) {
        #[cfg(windows)]
        {
            let link_str = link_path.to_string_lossy();
            let target_str = target_path.to_string_lossy();
            let output = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J", &link_str, &target_str])
                .output();
            match output {
                Ok(out) if out.status.success() => {}
                Ok(out) => panic!(
                    "mklink /J failed ({}): stdout=`{}` stderr=`{}`",
                    out.status,
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                ),
                Err(err) => panic!("failed spawning cmd /C mklink /J: {err}"),
            }
        }
        #[cfg(unix)]
        {
            if let Err(err) = std::os::unix::fs::symlink(target_path, link_path) {
                panic!(
                    "failed creating unix symlink `{}` -> `{}`: {err}",
                    link_path.display(),
                    target_path.display()
                );
            }
        }
    }

    #[test]
    fn guard_accepts_valid_isolated_profile_and_subpaths() {
        let profile = match TestProfile::new_isolated("mod-ok") {
            Ok(p) => p,
            Err(err) => panic!("profile build failed: {err}"),
        };
        let ctx = test_ctx();
        let guard = match FaultInjectionGuard::acquire_internal(&profile, Some("1"), &ctx) {
            Ok(g) => g,
            Err(err) => panic!("valid isolated profile must pass guard: {err}"),
        };
        assert_eq!(guard.memory_db(), DEFAULT_TEST_MEMORY_DB);
        assert_eq!(guard.control_db(), DEFAULT_TEST_CONTROL_DB);

        let nested = profile.workspace_dir().join("nested").join("file.txt");
        assert!(
            guard.validate_subpath(&nested).is_ok(),
            "nested path under workspace_dir must validate"
        );
        let escaped = env::temp_dir().join("other-dir").join("escape.txt");
        assert!(matches!(
            guard.validate_subpath(&escaped),
            Err(DennettError::SafetyGuardRejected { .. })
        ));
    }

    #[test]
    fn guard_rejects_every_boundary_violation() {
        let base = match TestProfile::new_isolated("mod-neg") {
            Ok(p) => p,
            Err(err) => panic!("profile build failed: {err}"),
        };
        let ctx = test_ctx();

        assert!(matches!(
            FaultInjectionGuard::acquire_internal(&base, None, &ctx),
            Err(DennettError::SafetyGuardRejected { .. })
        ));
        assert!(matches!(
            FaultInjectionGuard::acquire_internal(&base, Some("0"), &ctx),
            Err(DennettError::SafetyGuardRejected { .. })
        ));

        let mut prod = base.clone();
        prod.mode = ProfileMode::Production;
        assert!(matches!(
            FaultInjectionGuard::acquire_internal(&prod, Some("1"), &ctx),
            Err(DennettError::SafetyGuardRejected { .. })
        ));

        let mut live = base.clone();
        live.allow_live_external_effects = true;
        assert!(matches!(
            FaultInjectionGuard::acquire_internal(&live, Some("1"), &ctx),
            Err(DennettError::SafetyGuardRejected { .. })
        ));

        let mut bad_mem = base.clone();
        bad_mem.postgres_memory_database = String::from("dennett_memory");
        assert!(matches!(
            FaultInjectionGuard::acquire_internal(&bad_mem, Some("1"), &ctx),
            Err(DennettError::SafetyGuardRejected { .. })
        ));

        let mut bad_ctrl = base.clone();
        bad_ctrl.postgres_control_database = String::from("dennett_control");
        assert!(matches!(
            FaultInjectionGuard::acquire_internal(&bad_ctrl, Some("1"), &ctx),
            Err(DennettError::SafetyGuardRejected { .. })
        ));

        let mut dup_db = base.clone();
        dup_db.postgres_control_database = dup_db.postgres_memory_database.clone();
        assert!(matches!(
            FaultInjectionGuard::acquire_internal(&dup_db, Some("1"), &ctx),
            Err(DennettError::SafetyGuardRejected { .. })
        ));

        let mut escaped_data = base.clone();
        escaped_data.data_dir = env::temp_dir().join("outside-data");
        assert!(matches!(
            FaultInjectionGuard::acquire_internal(&escaped_data, Some("1"), &ctx),
            Err(DennettError::SafetyGuardRejected { .. })
        ));

        let mut escaped_ws = base.clone();
        escaped_ws.workspace_dir = base.root_dir.join("..").join("sibling");
        assert!(matches!(
            FaultInjectionGuard::acquire_internal(&escaped_ws, Some("1"), &ctx),
            Err(DennettError::SafetyGuardRejected { .. })
        ));

        let mut dup_dirs = base.clone();
        dup_dirs.workspace_dir = dup_dirs.data_dir.clone();
        assert!(matches!(
            FaultInjectionGuard::acquire_internal(&dup_dirs, Some("1"), &ctx),
            Err(DennettError::SafetyGuardRejected { .. })
        ));

        let simulated_repo_in_temp = ctx.temp_dir_norm.join("fake-repo-in-tmp");
        let mut ctx_repo_in_tmp = test_ctx();
        ctx_repo_in_tmp.repo_root_norm = Some(simulated_repo_in_temp.clone());
        let mut inside_repo = base.clone();
        inside_repo.root_dir = simulated_repo_in_temp.join("dennett-test-inside");
        inside_repo.data_dir = inside_repo.root_dir.join("data");
        inside_repo.workspace_dir = inside_repo.root_dir.join("workspace");
        assert!(matches!(
            FaultInjectionGuard::acquire_internal(&inside_repo, Some("1"), &ctx_repo_in_tmp),
            Err(DennettError::SafetyGuardRejected { .. })
        ));

        let mut bare_temp = base.clone();
        bare_temp.root_dir = env::temp_dir();
        bare_temp.data_dir = bare_temp.root_dir.join("data");
        bare_temp.workspace_dir = bare_temp.root_dir.join("workspace");
        assert!(matches!(
            FaultInjectionGuard::acquire_internal(&bare_temp, Some("1"), &ctx),
            Err(DennettError::SafetyGuardRejected { .. })
        ));
    }

    #[test]
    fn guard_requires_dennett_prefix_when_temp_overlaps_home() {
        let base_ctx = test_ctx();
        let fake_home = base_ctx.temp_dir_norm.clone();
        let ctx_home_eq_temp = PathValidationContext {
            temp_dir_raw: base_ctx.temp_dir_raw.clone(),
            temp_dir_norm: fake_home.clone(),
            home_dirs_norm: vec![fake_home.clone()],
            cwd_norm: base_ctx.cwd_norm.clone(),
            repo_root_norm: base_ctx.repo_root_norm.clone(),
        };

        let mut bad_home_sub = match TestProfile::new_isolated("home-overlap") {
            Ok(p) => p,
            Err(err) => panic!("profile build failed: {err}"),
        };
        bad_home_sub.root_dir = base_ctx.temp_dir_raw.join(".gemini");
        bad_home_sub.data_dir = bad_home_sub.root_dir.join("data");
        bad_home_sub.workspace_dir = bad_home_sub.root_dir.join("workspace");
        assert!(matches!(
            FaultInjectionGuard::acquire_internal(&bad_home_sub, Some("1"), &ctx_home_eq_temp),
            Err(DennettError::SafetyGuardRejected { .. })
        ));

        let good = match TestProfile::new_isolated("home-ok") {
            Ok(p) => p,
            Err(err) => panic!("profile build failed: {err}"),
        };
        assert!(
            FaultInjectionGuard::acquire_internal(&good, Some("1"), &ctx_home_eq_temp).is_ok(),
            "dennett-test-* prefix must be accepted when temp_dir overlaps HOME"
        );
    }

    #[test]
    fn h0_07_guard_rejects_dangling_and_live_symlinks_and_junctions() {
        let (profile, _tmp) = match TestProfile::create_on_disk("symlink-probe") {
            Ok(pair) => pair,
            Err(err) => panic!("failed creating on-disk profile: {err}"),
        };
        let (external_profile, _ext_tmp) = match TestProfile::create_on_disk("symlink-target") {
            Ok(pair) => pair,
            Err(err) => panic!("failed creating external target dir: {err}"),
        };
        let ctx = test_ctx();

        let external_target = external_profile.workspace_dir().join("outside_real");
        if let Err(err) = fs::create_dir_all(&external_target) {
            panic!("failed creating external link target: {err}");
        }

        let dir_link = profile.workspace_dir().join("evil_link");
        create_unprivileged_dir_link(&dir_link, &external_target);

        let guard = match FaultInjectionGuard::acquire_internal(&profile, Some("1"), &ctx) {
            Ok(g) => g,
            Err(err) => panic!("baseline profile should pass guard: {err}"),
        };

        // 1. Live symlink/junction pointing outside must be rejected fail-closed.
        assert!(
            matches!(
                guard.validate_subpath(&dir_link),
                Err(DennettError::SafetyGuardRejected { .. })
            ),
            "live symlink/junction subpath must be rejected fail-closed"
        );
        let mut live_link_data = profile.clone();
        live_link_data.data_dir = dir_link.clone();
        assert!(
            matches!(
                FaultInjectionGuard::acquire_internal(&live_link_data, Some("1"), &ctx),
                Err(DennettError::SafetyGuardRejected { .. })
            ),
            "profile with live symlink/junction data_dir must be rejected fail-closed"
        );

        // 2. Remove target so the link becomes dangling; must STILL be rejected fail-closed.
        if let Err(err) = fs::remove_dir(&external_target) {
            panic!("failed removing target dir to make link dangling: {err}");
        }
        assert!(
            matches!(
                guard.validate_subpath(&dir_link),
                Err(DennettError::SafetyGuardRejected { .. })
            ),
            "dangling symlink/junction subpath must be rejected fail-closed"
        );
        let mut dangling_link_data = profile.clone();
        dangling_link_data.data_dir = dir_link;
        assert!(
            matches!(
                FaultInjectionGuard::acquire_internal(&dangling_link_data, Some("1"), &ctx),
                Err(DennettError::SafetyGuardRejected { .. })
            ),
            "profile with dangling symlink/junction data_dir must be rejected fail-closed"
        );
    }

    #[test]
    fn h0_06_guard_accepts_profile_when_temp_dir_traverses_junction_or_symlink() {
        let (holder, _holder_tmp) = match TestProfile::create_on_disk("h06-holder") {
            Ok(pair) => pair,
            Err(err) => panic!("failed creating holder dir: {err}"),
        };
        let real_temp_target = holder.workspace_dir().join("real-temp");
        if let Err(err) = fs::create_dir_all(&real_temp_target) {
            panic!("failed creating real temp target: {err}");
        }

        let linked_temp = holder.workspace_dir().join("linked-temp");
        create_unprivileged_dir_link(&linked_temp, &real_temp_target);

        let base_ctx = test_ctx();
        let linked_temp_norm = match canonicalize_existing_dir(&linked_temp, "linked_temp") {
            Ok(p) => p,
            Err(err) => panic!("failed canonicalizing linked temp: {err}"),
        };
        let ctx = PathValidationContext {
            temp_dir_raw: linked_temp.clone(),
            temp_dir_norm: linked_temp_norm,
            home_dirs_norm: base_ctx.home_dirs_norm,
            cwd_norm: base_ctx.cwd_norm,
            repo_root_norm: base_ctx.repo_root_norm,
        };

        let root_under_linked = linked_temp.join("dennett-test-isolated").join("case-h06");
        let mut profile = match TestProfile::new_isolated("case-h06") {
            Ok(p) => p,
            Err(err) => panic!("profile build failed: {err}"),
        };
        profile.root_dir = root_under_linked.clone();
        profile.data_dir = root_under_linked.join("data");
        profile.workspace_dir = root_under_linked.join("workspace");

        // Must succeed before directory creation on disk...
        let pre_guard = match FaultInjectionGuard::acquire_internal(&profile, Some("1"), &ctx) {
            Ok(g) => g,
            Err(err) => {
                panic!("profile under symlinked/junctioned TEMP must validate before mkdir: {err}")
            }
        };
        assert!(
            pre_guard
                .validate_subpath(&profile.workspace_dir().join("note.txt"))
                .is_ok()
        );

        // ...and after physical directory creation on disk.
        if let Err(err) = fs::create_dir_all(&profile.data_dir)
            .and_then(|()| fs::create_dir_all(&profile.workspace_dir))
        {
            panic!("failed creating profile subdirs under linked temp: {err}");
        }
        let post_guard = match FaultInjectionGuard::acquire_internal(&profile, Some("1"), &ctx) {
            Ok(g) => g,
            Err(err) => {
                panic!("profile under symlinked/junctioned TEMP must validate after mkdir: {err}")
            }
        };
        assert!(
            post_guard
                .validate_subpath(&profile.workspace_dir().join("nested").join("file.txt"))
                .is_ok()
        );
    }
}
