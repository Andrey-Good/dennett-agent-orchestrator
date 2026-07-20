use super::installation::InstallationMetadata;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const NODE_EXECUTABLE_ENV: &str = "DENNETT_NODE_EXECUTABLE";
const INSTALLATION_ID_ENV: &str = "DENNETT_INSTALLATION_ID";
const AUTHORITY_EPOCH_ENV: &str = "DENNETT_AUTHORITY_EPOCH";

pub(super) async fn start(
    metadata: InstallationMetadata,
    data_dir: PathBuf,
) -> Result<(), NodeStartError> {
    tokio::task::spawn_blocking(move || start_blocking(&metadata, &data_dir))
        .await
        .map_err(|_| NodeStartError::StartFailed)?
}

#[cfg(windows)]
fn start_blocking(metadata: &InstallationMetadata, data_dir: &Path) -> Result<(), NodeStartError> {
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::System::Threading::{CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW};

    let executable = locate_executable()?;
    let child = Command::new(executable)
        .env(INSTALLATION_ID_ENV, &metadata.installation_id)
        .env(AUTHORITY_EPOCH_ENV, metadata.authority_epoch.to_string())
        .current_dir(data_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW)
        .spawn()
        .map_err(|_| NodeStartError::StartFailed)?;
    tracing::info!(
        phase = "node_spawned",
        process_id = child.id(),
        "started independently owned Dennett Node"
    );
    drop(child);
    Ok(())
}

#[cfg(not(windows))]
fn start_blocking(
    _metadata: &InstallationMetadata,
    _data_dir: &Path,
) -> Result<(), NodeStartError> {
    Err(NodeStartError::UnsupportedPlatform)
}

fn locate_executable() -> Result<PathBuf, NodeStartError> {
    if let Some(path) = std::env::var_os(NODE_EXECUTABLE_ENV).map(PathBuf::from) {
        return executable(path).ok_or(NodeStartError::ExecutableMissing);
    }

    let executable_name = if cfg!(windows) {
        "dennett-node.exe"
    } else {
        "dennett-node"
    };
    let mut candidates = Vec::new();
    if let Ok(current_executable) = std::env::current_exe()
        && let Some(parent) = current_executable.parent()
    {
        candidates.push(parent.join(executable_name));
    }
    if let Ok(current_directory) = std::env::current_dir() {
        for ancestor in current_directory.ancestors().take(8) {
            if ancestor.join("services/node/Cargo.toml").is_file() {
                candidates.push(ancestor.join("target/release").join(executable_name));
                candidates.push(ancestor.join("target/debug").join(executable_name));
                break;
            }
        }
    }
    candidates
        .into_iter()
        .find_map(executable)
        .ok_or(NodeStartError::ExecutableMissing)
}

fn executable(path: PathBuf) -> Option<PathBuf> {
    path.is_file().then(|| path.canonicalize().unwrap_or(path))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum NodeStartError {
    ExecutableMissing,
    StartFailed,
    #[cfg(not(windows))]
    UnsupportedPlatform,
}
