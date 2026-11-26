//! Isolated profile generation, `agy` version detection, workspace write-policy gate
//! configuration (`settings.json` + `config/hooks.json`), and explicit rehydration helpers.

use core::fmt::{self, Write as _};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use dennett_contracts::redact::redact_secrets;
use dennett_contracts::{
    DennettError, MessageRecord, RuntimeAuthProfile, SessionId, WorkspaceWritePolicy,
};
use serde_json::{Value, json};
use tokio::process::Command;
use tokio::time;

use crate::port::ActiveSkillPayload;

/// Minimum supported `agy` CLI version required for production ACP v1 execution.
pub const MIN_AGY_VERSION: Semver = Semver::new(1, 5, 0);
/// Relative application data subdirectory name passed via `--app-data-dir`.
pub const ISOLATED_APP_DATA_REL: &str = "dennett-acp";
/// Timeout applied when checking `<agy> --version`.
const VERSION_PROBE_TIMEOUT: Duration = Duration::from_secs(10);
/// Maximum persisted messages included in a rehydrated `systemPrompt.append` block.
const MAX_REHYDRATE_MESSAGES: usize = 40;
/// Maximum byte budget for the rehydrated transcript excerpt.
const MAX_REHYDRATE_BYTES: usize = 24 * 1024;

/// Parsed semantic version triple (`major.minor.patch`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Semver {
    /// Major version component.
    pub major: u64,
    /// Minor version component.
    pub minor: u64,
    /// Patch version component.
    pub patch: u64,
}

impl Semver {
    /// Constructs a semantic version from numeric components.
    #[must_use]
    pub const fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

impl fmt::Display for Semver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Extracts the first `MAJOR.MINOR.PATCH` semantic version from `agy --version` output.
///
/// # Errors
///
/// Returns [`DennettError::InvalidInput`] if no valid `MAJOR.MINOR.PATCH` triple is found.
pub fn parse_agy_version(raw: &str) -> Result<Semver, DennettError> {
    for token in raw.split_whitespace() {
        let trimmed = token
            .trim_start_matches('v')
            .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.' && c != '-');
        let core = trimmed.split(['-', '+']).next().unwrap_or(trimmed);
        let mut parts = core.split('.');
        if let (Some(maj), Some(min), Some(pat), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
            && let (Ok(major), Ok(minor), Ok(patch)) =
                (maj.parse::<u64>(), min.parse::<u64>(), pat.parse::<u64>())
        {
            return Ok(Semver::new(major, minor, patch));
        }
    }

    Err(DennettError::InvalidInput {
        field: String::from("agy_version"),
        reason: format!("could not parse semantic version from `{}`", raw.trim()),
    })
}

/// Parses `raw` and enforces [`MIN_AGY_VERSION`].
///
/// # Errors
///
/// Returns [`DennettError::InvalidInput`] on unparseable output or
/// [`DennettError::RuntimeVersionUnsupported`] when below [`MIN_AGY_VERSION`].
pub fn ensure_agy_version_supported(raw: &str) -> Result<Semver, DennettError> {
    let parsed = parse_agy_version(raw)?;
    if parsed < MIN_AGY_VERSION {
        return Err(DennettError::RuntimeVersionUnsupported {
            found: parsed.to_string(),
            minimum: MIN_AGY_VERSION.to_string(),
        });
    }
    Ok(parsed)
}

/// Resolves the `agy` binary path from `DENNETT_AGY_BIN` or defaults to `"agy"` on `PATH`.
#[must_use]
pub fn resolve_agy_binary() -> PathBuf {
    env::var_os("DENNETT_AGY_BIN")
        .filter(|v| !v.is_empty())
        .map_or_else(|| PathBuf::from("agy"), PathBuf::from)
}

/// Executes `<binary> --version` and confirms it meets [`MIN_AGY_VERSION`].
///
/// # Errors
///
/// Returns [`DennettError::RuntimeProtocol`], [`DennettError::Timeout`], or
/// [`DennettError::RuntimeVersionUnsupported`].
pub async fn detect_agy_version(binary: &Path) -> Result<Semver, DennettError> {
    let mut cmd = Command::new(binary);
    cmd.arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let output = time::timeout(VERSION_PROBE_TIMEOUT, cmd.output())
        .await
        .map_err(|_| DennettError::Timeout {
            operation: String::from("agy --version"),
        })?
        .map_err(|err| DennettError::RuntimeProtocol {
            code: None,
            detail: redact_secrets(
                &format!("failed to execute `{}`: {err}", binary.display()),
                &[],
            ),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let safe_stderr = redact_secrets(stderr.trim(), &[]);
        return Err(DennettError::RuntimeProtocol {
            code: None,
            detail: format!(
                "`agy --version` exited with {}: {safe_stderr}",
                output.status
            ),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    ensure_agy_version_supported(&stdout)
}

/// Materialized isolated `agy acp` profile paths for a single Dennett session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsolatedProfilePaths {
    /// Directory passed via `--gemini-dir`.
    pub gemini_dir: PathBuf,
    /// Relative directory name passed via `--app-data-dir`.
    pub app_data_rel: String,
    /// Full path `<gemini_dir>/<app_data_rel>`.
    pub app_data_dir: PathBuf,
    /// Full path `<gemini_dir>/config`.
    pub config_dir: PathBuf,
    /// Isolated ACP diagnostic log file path (`--log-file`).
    pub log_file: PathBuf,
}

/// Generates the JSON contents for isolated `settings.json` / `config/config.json` given an
/// auth profile and workspace write policy.
///
/// - For [`RuntimeAuthProfile::GeminiApiKey`], pins `"modelProvider": "gemini"` so `agy acp`
///   fails closed (`-32602` / `-32000`) when `GEMINI_API_KEY` is missing or invalid and NEVER
///   silently falls back to `oauth-personal`.
/// - Encodes explicit `permissions.deny` / `permissions.ask` rules matching the agent's
///   [`WorkspaceWritePolicy`].
#[must_use]
pub fn build_settings_json(
    auth_profile: RuntimeAuthProfile,
    write_policy: &WorkspaceWritePolicy,
) -> Value {
    let mut root = serde_json::Map::new();
    if auth_profile == RuntimeAuthProfile::GeminiApiKey {
        root.insert(
            String::from("modelProvider"),
            Value::String(String::from("gemini")),
        );
    }

    let permissions = match write_policy {
        WorkspaceWritePolicy::DenyAll => json!({
            "deny": ["write_file(*)", "edit_file(*)"],
            "ask": ["command(*)"]
        }),
        _ => json!({
            "ask": ["write_file(*)", "edit_file(*)", "command(*)"]
        }),
    };
    root.insert(String::from("permissions"), permissions);

    Value::Object(root)
}

/// Generates `<gemini-dir>/config/hooks.json` so `agy acp` routes every native workspace file
/// write/edit through either hard-deny (`DenyAll`) or `force_ask` -> ACP
/// `session/request_permission` -> Dennett's external `PolicyEngine`!
///
/// Addresses the D0 security finding where `agy acp 1.5.15` `default` mode otherwise executes
/// `write_file` inside `cwd` without invoking `session/request_permission`.
#[must_use]
pub fn build_write_gate_hooks_json(write_policy: &WorkspaceWritePolicy) -> Value {
    let (decision, reason) = match write_policy {
        WorkspaceWritePolicy::DenyAll => (
            "deny",
            "Dennett workspace write policy denies file writes in this session",
        ),
        _ => (
            "force_ask",
            "Dennett external PolicyEngine must approve workspace file writes",
        ),
    };

    let payload = format!(r#"{{"decision":"{decision}","reason":"{reason}"}}"#);
    let command = if cfg!(windows) {
        // Executed via `cmd /c`; `echo` prints the exact JSON payload cleanly without quotes.
        format!("echo {payload}")
    } else {
        format!("printf '%s\\n' '{payload}'")
    };

    json!({
        "dennett-workspace-write-gate": {
            "enabled": true,
            "PreToolUse": [
                {
                    "matcher": "write_.*|edit_.*|replace_.*|multi_replace_.*",
                    "hooks": [
                        {
                            "type": "command",
                            "command": command,
                            "timeout": 5
                        }
                    ]
                }
            ]
        }
    })
}

/// Materializes an isolated per-session `agy acp` profile directory on disk without touching
/// the user's `~/.gemini` directory or polluting the project workspace.
///
/// # Errors
///
/// Returns [`DennettError::Internal`] if creating directories or writing config files fails.
pub fn prepare_isolated_profile(
    profile_root: &Path,
    session_id: SessionId,
    auth_profile: RuntimeAuthProfile,
    write_policy: &WorkspaceWritePolicy,
) -> Result<IsolatedProfilePaths, DennettError> {
    prepare_isolated_profile_with_skills(profile_root, session_id, auth_profile, write_policy, &[])
}

/// Materializes an isolated per-session `agy acp` profile directory on disk including active A08
/// skills strictly under `<gemini_dir>/config/skills/<dir>/SKILL.md` (NEVER touching `~/.gemini/`).
///
/// Removes stale skill folders before materializing `active_skills` so disabling a skill ensures
/// it is immediately absent from the isolated runtime profile.
///
/// # Errors
///
/// Returns [`DennettError::InvalidInput`] on unsafe skill slugs or [`DennettError::Internal`]
/// if filesystem operations fail.
pub fn prepare_isolated_profile_with_skills(
    profile_root: &Path,
    session_id: SessionId,
    auth_profile: RuntimeAuthProfile,
    write_policy: &WorkspaceWritePolicy,
    active_skills: &[ActiveSkillPayload],
) -> Result<IsolatedProfilePaths, DennettError> {
    let gemini_dir = profile_root.join(format!("session-{session_id}"));
    let app_data_rel = String::from(ISOLATED_APP_DATA_REL);
    let app_data_dir = gemini_dir.join(&app_data_rel);
    let config_dir = gemini_dir.join("config");
    let log_file = gemini_dir.join("acp.log");

    for dir in [&gemini_dir, &app_data_dir, &config_dir] {
        fs::create_dir_all(dir).map_err(|err| DennettError::Internal {
            detail: format!(
                "failed creating isolated ACP profile directory `{}`: {err}",
                dir.display()
            ),
        })?;
    }

    let settings = build_settings_json(auth_profile, write_policy);
    let settings_bytes =
        serde_json::to_vec_pretty(&settings).map_err(|err| DennettError::Internal {
            detail: format!("failed serializing isolated ACP settings: {err}"),
        })?;

    for target in [
        gemini_dir.join("settings.json"),
        app_data_dir.join("settings.json"),
        config_dir.join("config.json"),
    ] {
        fs::write(&target, &settings_bytes).map_err(|err| DennettError::Internal {
            detail: format!(
                "failed writing isolated ACP settings `{}`: {err}",
                target.display()
            ),
        })?;
    }

    let hooks = build_write_gate_hooks_json(write_policy);
    let hooks_bytes = serde_json::to_vec_pretty(&hooks).map_err(|err| DennettError::Internal {
        detail: format!("failed serializing isolated ACP hooks: {err}"),
    })?;
    let hooks_path = config_dir.join("hooks.json");
    fs::write(&hooks_path, &hooks_bytes).map_err(|err| DennettError::Internal {
        detail: format!(
            "failed writing isolated ACP write-gate hooks `{}`: {err}",
            hooks_path.display()
        ),
    })?;

    materialize_isolated_skills(&config_dir, active_skills)?;

    Ok(IsolatedProfilePaths {
        gemini_dir,
        app_data_rel,
        app_data_dir,
        config_dir,
        log_file,
    })
}

/// Derives a safe cross-platform directory name for a skill slug (rejects path traversal and maps
/// `:` to `__` so Windows directory creation always succeeds).
///
/// # Errors
///
/// Returns [`DennettError::InvalidInput`] if `slug` is empty, contains traversal (`..`, `/`, `\`),
/// or contains disallowed characters.
pub fn safe_skill_dir_name(slug: &str) -> Result<String, DennettError> {
    let trimmed = slug.trim();
    if trimmed.is_empty()
        || trimmed == "."
        || trimmed.contains("..")
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.as_bytes().contains(&0)
        || !trimmed
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
    {
        return Err(DennettError::InvalidInput {
            field: String::from("skill.slug"),
            reason: format!("unsafe or invalid skill slug `{trimmed}`"),
        });
    }
    Ok(trimmed.replace(':', "__"))
}

const MAX_SKILL_COMPANION_DEPTH: usize = 8;
const MAX_SKILL_COMPANION_FILES: usize = 64;
const MAX_SKILL_COMPANION_TOTAL_BYTES: u64 = 2 * 1024 * 1024;
const ALLOWED_COMPANION_SUBDIRS: [&str; 2] = ["scripts", "references"];

fn is_symlink_or_reparse_point(meta: &fs::Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        if (meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT) != 0 {
            return true;
        }
    }
    false
}

fn validate_safe_entry_name(name: &std::ffi::OsStr) -> Result<String, DennettError> {
    let raw = name.to_str().ok_or_else(|| DennettError::AccessDenied {
        scope: String::from("skill_overlay"),
        reason: String::from("non-UTF-8 companion filename in skill directory"),
    })?;
    if raw.is_empty()
        || raw == "."
        || raw == ".."
        || raw.contains('/')
        || raw.contains('\\')
        || raw.contains('\0')
        || raw.contains(':')
    {
        return Err(DennettError::AccessDenied {
            scope: String::from("skill_overlay"),
            reason: format!("unsafe companion entry name `{raw}` in skill directory"),
        });
    }
    Ok(raw.to_owned())
}

#[expect(
    clippy::too_many_lines,
    reason = "recursively copies companion scripts/references with explicit symlink, reparse, containment, file-count, and byte-size guards"
)]
fn copy_companion_tree_bounded(
    canonical_root: &Path,
    src_dir: &Path,
    dst_dir: &Path,
    depth: usize,
    files_copied: &mut usize,
    bytes_copied: &mut u64,
) -> Result<(), DennettError> {
    if depth > MAX_SKILL_COMPANION_DEPTH {
        return Err(DennettError::AccessDenied {
            scope: String::from("skill_overlay"),
            reason: format!(
                "companion directory depth exceeded maximum ({MAX_SKILL_COMPANION_DEPTH})"
            ),
        });
    }
    let dir_meta = fs::symlink_metadata(src_dir).map_err(|err| DennettError::Internal {
        detail: format!(
            "failed reading metadata for skill companion dir `{}`: {err}",
            src_dir.display()
        ),
    })?;
    if is_symlink_or_reparse_point(&dir_meta) || !dir_meta.is_dir() {
        return Err(DennettError::AccessDenied {
            scope: String::from("skill_overlay"),
            reason: format!(
                "refusing symlink/reparse companion directory `{}`",
                src_dir.display()
            ),
        });
    }
    let canonical_dir = fs::canonicalize(src_dir).map_err(|err| DennettError::AccessDenied {
        scope: String::from("skill_overlay"),
        reason: format!(
            "failed canonicalizing skill companion dir `{}`: {err}",
            src_dir.display()
        ),
    })?;
    if !canonical_dir.starts_with(canonical_root) {
        return Err(DennettError::AccessDenied {
            scope: String::from("skill_overlay"),
            reason: format!(
                "skill companion directory `{}` escapes skill root",
                src_dir.display()
            ),
        });
    }

    fs::create_dir_all(dst_dir).map_err(|err| DennettError::Internal {
        detail: format!(
            "failed creating destination skill companion dir `{}`: {err}",
            dst_dir.display()
        ),
    })?;

    let entries = fs::read_dir(src_dir).map_err(|err| DennettError::Internal {
        detail: format!(
            "failed listing skill companion dir `{}`: {err}",
            src_dir.display()
        ),
    })?;

    for entry_res in entries {
        let entry = entry_res.map_err(|err| DennettError::Internal {
            detail: format!("failed iterating skill companion dir: {err}"),
        })?;
        let name = validate_safe_entry_name(&entry.file_name())?;
        let child_src = entry.path();
        let child_dst = dst_dir.join(&name);
        let meta = fs::symlink_metadata(&child_src).map_err(|err| DennettError::Internal {
            detail: format!(
                "failed reading metadata for companion entry `{}`: {err}",
                child_src.display()
            ),
        })?;
        if is_symlink_or_reparse_point(&meta) {
            return Err(DennettError::AccessDenied {
                scope: String::from("skill_overlay"),
                reason: format!(
                    "refusing symlink/reparse companion file `{}` in skill overlay",
                    child_src.display()
                ),
            });
        }
        if meta.is_dir() {
            copy_companion_tree_bounded(
                canonical_root,
                &child_src,
                &child_dst,
                depth + 1,
                files_copied,
                bytes_copied,
            )?;
        } else if meta.is_file() {
            let canonical_file =
                fs::canonicalize(&child_src).map_err(|err| DennettError::AccessDenied {
                    scope: String::from("skill_overlay"),
                    reason: format!(
                        "failed canonicalizing companion file `{}`: {err}",
                        child_src.display()
                    ),
                })?;
            if !canonical_file.starts_with(canonical_root) {
                return Err(DennettError::AccessDenied {
                    scope: String::from("skill_overlay"),
                    reason: format!(
                        "companion file `{}` escapes skill root",
                        child_src.display()
                    ),
                });
            }
            *files_copied = files_copied.saturating_add(1);
            if *files_copied > MAX_SKILL_COMPANION_FILES {
                return Err(DennettError::AccessDenied {
                    scope: String::from("skill_overlay"),
                    reason: format!(
                        "skill companion file count exceeds maximum ({MAX_SKILL_COMPANION_FILES})"
                    ),
                });
            }
            *bytes_copied = bytes_copied.saturating_add(meta.len());
            if *bytes_copied > MAX_SKILL_COMPANION_TOTAL_BYTES {
                return Err(DennettError::AccessDenied {
                    scope: String::from("skill_overlay"),
                    reason: format!(
                        "skill companion files exceed total byte limit ({MAX_SKILL_COMPANION_TOTAL_BYTES})"
                    ),
                });
            }
            fs::copy(&child_src, &child_dst).map_err(|err| DennettError::Internal {
                detail: format!(
                    "failed copying companion file `{}` to `{}`: {err}",
                    child_src.display(),
                    child_dst.display()
                ),
            })?;
        }
    }
    Ok(())
}

fn copy_skill_companion_subdirs(src_skill_dir: &Path, dst_skill_dir: &Path) -> Result<(), DennettError> {
    let root_meta = match fs::symlink_metadata(src_skill_dir) {
        Ok(m) => m,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => {
            return Err(DennettError::Internal {
                detail: format!(
                    "failed inspecting skill source dir `{}`: {err}",
                    src_skill_dir.display()
                ),
            });
        }
    };
    if is_symlink_or_reparse_point(&root_meta) || !root_meta.is_dir() {
        return Err(DennettError::AccessDenied {
            scope: String::from("skill_overlay"),
            reason: format!(
                "refusing symlink/non-directory skill source root `{}`",
                src_skill_dir.display()
            ),
        });
    }
    let canonical_root =
        fs::canonicalize(src_skill_dir).map_err(|err| DennettError::AccessDenied {
            scope: String::from("skill_overlay"),
            reason: format!(
                "failed canonicalizing skill source root `{}`: {err}",
                src_skill_dir.display()
            ),
        })?;
    let mut files_copied = 0_usize;
    let mut bytes_copied = 0_u64;
    for sub in ALLOWED_COMPANION_SUBDIRS {
        let sub_src = src_skill_dir.join(sub);
        if fs::symlink_metadata(&sub_src).is_ok() {
            let sub_dst = dst_skill_dir.join(sub);
            copy_companion_tree_bounded(
                &canonical_root,
                &sub_src,
                &sub_dst,
                0,
                &mut files_copied,
                &mut bytes_copied,
            )?;
        }
    }
    Ok(())
}

/// Materializes `active_skills` strictly into `<config_dir>/skills/<safe_slug>/SKILL.md`
/// (plus safe read-only copies of companion `scripts/` and `references/` subdirectories when
/// `source_dir` is provided), removing any previously materialized skills first so disabled skills
/// never linger.
///
/// # Errors
///
/// Returns [`DennettError::InvalidInput`] on invalid slugs, [`DennettError::AccessDenied`] on
/// symlink/traversal companion entries, or [`DennettError::Internal`] on IO error.
pub fn materialize_isolated_skills(
    config_dir: &Path,
    active_skills: &[ActiveSkillPayload],
) -> Result<(), DennettError> {
    let skills_root = config_dir.join("skills");
    if skills_root.exists() {
        fs::remove_dir_all(&skills_root).map_err(|err| DennettError::Internal {
            detail: format!(
                "failed clearing stale isolated skills directory `{}`: {err}",
                skills_root.display()
            ),
        })?;
    }
    if active_skills.is_empty() {
        return Ok(());
    }

    fs::create_dir_all(&skills_root).map_err(|err| DennettError::Internal {
        detail: format!(
            "failed creating isolated skills directory `{}`: {err}",
            skills_root.display()
        ),
    })?;

    for skill in active_skills {
        let dir_name = safe_skill_dir_name(&skill.slug)?;
        let skill_dir = skills_root.join(dir_name);
        fs::create_dir_all(&skill_dir).map_err(|err| DennettError::Internal {
            detail: format!(
                "failed creating isolated skill folder `{}`: {err}",
                skill_dir.display()
            ),
        })?;
        let skill_file = skill_dir.join("SKILL.md");
        fs::write(&skill_file, skill.skill_markdown.as_bytes()).map_err(|err| {
            DennettError::Internal {
                detail: format!(
                    "failed writing isolated skill file `{}`: {err}",
                    skill_file.display()
                ),
            }
        })?;
        if let Some(ref src_dir) = skill.source_dir {
            copy_skill_companion_subdirs(src_dir, &skill_dir)?;
        }
    }

    Ok(())
}

/// Normalizes a workspace path into a stable canonical string suitable for `cwd` comparison
/// and ACP wire payloads (strips Windows extended-length `\\?\` prefix).
#[must_use]
pub fn normalize_workspace_path(path: &Path) -> String {
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let raw = canonical.to_string_lossy();
    let stripped = raw.strip_prefix(r"\\?\").unwrap_or(&raw);
    if cfg!(windows) {
        stripped.replace('\\', "/")
    } else {
        stripped.to_owned()
    }
}

/// Compares two workspace path strings for equivalence (case-insensitive on Windows).
#[must_use]
pub fn workspace_paths_match(a: &str, b: &str) -> bool {
    let norm_a = a
        .trim()
        .strip_prefix(r"\\?\")
        .unwrap_or(a.trim())
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_owned();
    let norm_b = b
        .trim()
        .strip_prefix(r"\\?\")
        .unwrap_or(b.trim())
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_owned();
    if cfg!(windows) {
        norm_a.eq_ignore_ascii_case(&norm_b)
    } else {
        norm_a == norm_b
    }
}

/// Composes the effective `systemPrompt.append` payload combining the agent's explicit system
/// prompt with a bounded visible transcript when Dennett performs honest context rehydration.
///
/// Never includes hidden reasoning; respects UTF-8 boundaries when truncating older turns.
#[must_use]
pub fn build_effective_system_prompt(
    base_prompt: &str,
    history: &[MessageRecord],
    rehydrate_reason: Option<&str>,
) -> String {
    let Some(reason) = rehydrate_reason else {
        return base_prompt.to_owned();
    };
    if history.is_empty() {
        return format!(
            "{base_prompt}\n\n[Dennett Session Rehydration Notice]\n\
             Previous provider session could not be resumed ({reason}). Continuing cleanly in a fresh runtime session."
        );
    }

    let start_idx = history.len().saturating_sub(MAX_REHYDRATE_MESSAGES);
    let mut transcript = String::new();
    for msg in &history[start_idx..] {
        let entry = format!("[{}] {}\n", msg.role, msg.content);
        if transcript.len() + entry.len() > MAX_REHYDRATE_BYTES {
            let remaining = MAX_REHYDRATE_BYTES.saturating_sub(transcript.len());
            if remaining > 32 {
                let mut cut = remaining;
                while cut > 0 && !entry.is_char_boundary(cut) {
                    cut -= 1;
                }
                transcript.push_str(&entry[..cut]);
                transcript.push_str("\n...[truncated]...\n");
            }
            break;
        }
        transcript.push_str(&entry);
    }

    format!(
        "{base_prompt}\n\n[Dennett Session Rehydration Notice]\n\
         Previous provider session was not resumed ({reason}). \
         Below is the persisted visible conversation history from Dennett:\n\
         --- BEGIN PERSISTED HISTORY ---\n\
         {transcript}\
         --- END PERSISTED HISTORY ---"
    )
}

/// Composes the effective system prompt including active A08 skills (for runtimes without an
/// on-disk `--gemini-dir` skill folder such as `local-http-runtime`) and optional rehydration
/// history.
#[must_use]
pub fn build_effective_system_prompt_with_skills(
    base_prompt: &str,
    active_skills: &[ActiveSkillPayload],
    history: &[MessageRecord],
    rehydrate_reason: Option<&str>,
) -> String {
    let mut prompt = base_prompt.to_owned();
    if !active_skills.is_empty() {
        prompt.push_str("\n\n[Dennett Active Skills]");
        for skill in active_skills {
            let _ = write!(
                prompt,
                "\n--- SKILL {} (v{}) ---\n{}\n--- END SKILL {} ---",
                skill.slug, skill.version, skill.skill_markdown, skill.slug
            );
        }
    }
    build_effective_system_prompt(&prompt, history, rehydrate_reason)
}

#[cfg(test)]
mod tests {
    use dennett_contracts::{CorrelationId, MessageId, MessageRole, TimestampMs};

    use super::*;

    #[test]
    fn settings_generation_separates_auth_profiles_and_write_policies() {
        let oauth_ask = build_settings_json(
            RuntimeAuthProfile::OAuthPersonal,
            &WorkspaceWritePolicy::AskBeforeWrite,
        );
        assert!(
            oauth_ask.get("modelProvider").is_none(),
            "oauth-personal must not force gemini API modelProvider"
        );
        assert!(
            oauth_ask["permissions"]["ask"]
                .as_array()
                .is_some_and(|arr| arr.iter().any(|v| v == "write_file(*)"))
        );

        let api_deny = build_settings_json(
            RuntimeAuthProfile::GeminiApiKey,
            &WorkspaceWritePolicy::DenyAll,
        );
        assert_eq!(api_deny["modelProvider"], "gemini");
        assert!(
            api_deny["permissions"]["deny"]
                .as_array()
                .is_some_and(|arr| arr.iter().any(|v| v == "write_file(*)"))
        );

        let deny_hooks = build_write_gate_hooks_json(&WorkspaceWritePolicy::DenyAll);
        let deny_cmd =
            deny_hooks["dennett-workspace-write-gate"]["PreToolUse"][0]["hooks"][0]["command"]
                .as_str()
                .expect("hook command");
        assert!(deny_cmd.contains("\"decision\":\"deny\""));

        let ask_hooks = build_write_gate_hooks_json(&WorkspaceWritePolicy::AskBeforeWrite);
        let ask_cmd =
            ask_hooks["dennett-workspace-write-gate"]["PreToolUse"][0]["hooks"][0]["command"]
                .as_str()
                .expect("hook command");
        assert!(ask_cmd.contains("\"decision\":\"force_ask\""));
    }

    #[test]
    fn rehydrated_system_prompt_includes_visible_history_and_handles_utf8_safely() {
        let sid = SessionId::new();
        let history = vec![
            MessageRecord {
                message_id: MessageId::new(),
                session_id: sid,
                role: MessageRole::User,
                content: String::from("Привет, запомни ограничение PG16!"),
                run_id: None,
                client_message_id: None,
                correlation_id: CorrelationId::generate(),
                created_at: TimestampMs::now(),
            },
            MessageRecord {
                message_id: MessageId::new(),
                session_id: sid,
                role: MessageRole::Assistant,
                content: String::from("Понял, PostgreSQL 16 обязателен."),
                run_id: None,
                client_message_id: None,
                correlation_id: CorrelationId::generate(),
                created_at: TimestampMs::now(),
            },
        ];

        let no_rehydrate = build_effective_system_prompt("You are a botanist.", &history, None);
        assert_eq!(no_rehydrate, "You are a botanist.");

        let rehydrated = build_effective_system_prompt(
            "You are a botanist.",
            &history,
            Some("provider session expired"),
        );
        assert!(rehydrated.contains("You are a botanist."));
        assert!(rehydrated.contains("provider session expired"));
        assert!(rehydrated.contains("Привет, запомни ограничение PG16!"));
        assert!(rehydrated.contains("Понял, PostgreSQL 16 обязателен."));
    }

    #[test]
    fn agy_version_parser_enforces_minimum() {
        assert_eq!(
            ensure_agy_version_supported("1.5.15").expect("supported"),
            Semver::new(1, 5, 15)
        );
        assert!(matches!(
            ensure_agy_version_supported("agy 1.4.9"),
            Err(DennettError::RuntimeVersionUnsupported { .. })
        ));
        assert!(matches!(
            parse_agy_version("unknown"),
            Err(DennettError::InvalidInput { .. })
        ));
    }

    #[test]
    fn prepare_isolated_profile_overwrites_hooks_and_settings_on_policy_update() {
        let tmp = std::env::temp_dir().join(format!(
            "dennett-profile-overwrite-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let sid = SessionId::new();

        let p1 = prepare_isolated_profile(
            &tmp,
            sid,
            RuntimeAuthProfile::OAuthPersonal,
            &WorkspaceWritePolicy::DenyAll,
        )
        .expect("initial DenyAll profile");
        let hooks1 = fs::read_to_string(p1.config_dir.join("hooks.json")).expect("read hooks1");
        let settings1 = fs::read_to_string(p1.gemini_dir.join("settings.json")).expect("settings1");
        assert!(
            hooks1.contains(r#"\"decision\":\"deny\""#) || hooks1.contains(r#""decision":"deny""#)
        );
        assert!(settings1.contains(r#""deny""#));

        let p2 = prepare_isolated_profile(
            &tmp,
            sid,
            RuntimeAuthProfile::OAuthPersonal,
            &WorkspaceWritePolicy::AllowPaths {
                allowed_prefixes: vec![String::from("docs")],
            },
        )
        .expect("updated AllowPaths profile");
        let hooks2 = fs::read_to_string(p2.config_dir.join("hooks.json")).expect("read hooks2");
        let settings2 = fs::read_to_string(p2.gemini_dir.join("settings.json")).expect("settings2");
        assert!(
            hooks2.contains(r#"\"decision\":\"force_ask\""#)
                || hooks2.contains(r#""decision":"force_ask""#)
        );
        assert!(!settings2.contains(r#""deny""#));

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "verifies isolated skill overlay materialization, companion scripts/references copying, symlink blocking, and stale cleanup"
    )]
    fn a08_isolated_skills_materializes_and_cleans_removed_skills_without_touching_home() {
        let tmp = std::env::temp_dir().join(format!(
            "dennett-profile-skills-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let sid = SessionId::new();
        let skill_a = ActiveSkillPayload {
            slug: String::from("rust-audit"),
            version: String::from("1.0.0"),
            content_sha256: "a".repeat(64),
            skill_markdown: String::from("# Rust Audit\nCheck invariants."),
            source_dir: None,
        };
        let skill_b = ActiveSkillPayload {
            slug: String::from("plugin:db-check"),
            version: String::from("2.0.0"),
            content_sha256: "b".repeat(64),
            skill_markdown: String::from("# DB Check\nVerify PG16."),
            source_dir: None,
        };

        let p1 = prepare_isolated_profile_with_skills(
            &tmp,
            sid,
            RuntimeAuthProfile::OAuthPersonal,
            &WorkspaceWritePolicy::DenyAll,
            &[skill_a.clone(), skill_b],
        )
        .expect("profile with two skills");
        assert_eq!(
            fs::read_to_string(
                p1.config_dir
                    .join("skills")
                    .join("rust-audit")
                    .join("SKILL.md")
            )
            .expect("read rust-audit"),
            skill_a.skill_markdown
        );
        assert!(
            p1.config_dir
                .join("skills")
                .join("plugin__db-check")
                .join("SKILL.md")
                .exists()
        );

        // Removing `plugin:db-check` must wipe its stale folder on next materialization.
        let p2 = prepare_isolated_profile_with_skills(
            &tmp,
            sid,
            RuntimeAuthProfile::OAuthPersonal,
            &WorkspaceWritePolicy::DenyAll,
            &[skill_a],
        )
        .expect("profile with one skill");
        assert!(
            p2.config_dir
                .join("skills")
                .join("rust-audit")
                .join("SKILL.md")
                .exists()
        );
        assert!(
            !p2.config_dir
                .join("skills")
                .join("plugin__db-check")
                .exists()
        );

        // Traversal slug MUST be rejected.
        let bad = ActiveSkillPayload {
            slug: String::from("../escape"),
            version: String::from("1.0.0"),
            content_sha256: "c".repeat(64),
            skill_markdown: String::from("bad"),
            source_dir: None,
        };
        assert!(matches!(
            materialize_isolated_skills(&p2.config_dir, &[bad]),
            Err(DennettError::InvalidInput { .. })
        ));

        // Companion `scripts/` and `references/` are copied safely while arbitrary sibling dirs are skipped.
        let src_skill = tmp.join("src-skill");
        fs::create_dir_all(src_skill.join("scripts")).expect("scripts dir");
        fs::create_dir_all(src_skill.join("references")).expect("references dir");
        fs::create_dir_all(src_skill.join("private")).expect("private dir");
        fs::write(src_skill.join("scripts").join("check.py"), b"print('ok')").expect("script");
        fs::write(src_skill.join("references").join("guide.md"), b"# Guide").expect("ref");
        fs::write(src_skill.join("private").join("secret.txt"), b"do not copy").expect("private");

        let skill_with_companions = ActiveSkillPayload {
            slug: String::from("companion-skill"),
            version: String::from("1.0.0"),
            content_sha256: "d".repeat(64),
            skill_markdown: String::from("# Companion Skill"),
            source_dir: Some(src_skill.clone()),
        };
        materialize_isolated_skills(&p2.config_dir, &[skill_with_companions])
            .expect("materialize companion skill");
        assert_eq!(
            fs::read_to_string(
                p2.config_dir
                    .join("skills")
                    .join("companion-skill")
                    .join("scripts")
                    .join("check.py")
            )
            .expect("copied script"),
            "print('ok')"
        );
        assert_eq!(
            fs::read_to_string(
                p2.config_dir
                    .join("skills")
                    .join("companion-skill")
                    .join("references")
                    .join("guide.md")
            )
            .expect("copied ref"),
            "# Guide"
        );
        assert!(
            !p2.config_dir
                .join("skills")
                .join("companion-skill")
                .join("private")
                .exists(),
            "only scripts/ and references/ may be copied"
        );

        let _ = fs::remove_dir_all(&tmp);
    }
}
