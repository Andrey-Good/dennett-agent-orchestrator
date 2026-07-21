use super::installation::InstallationMetadata;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const NODE_EXECUTABLE_ENV: &str = "DENNETT_NODE_EXECUTABLE";
const INSTALLATION_ID_ENV: &str = "DENNETT_INSTALLATION_ID";
const AUTHORITY_EPOCH_ENV: &str = "DENNETT_AUTHORITY_EPOCH";
const DATA_DIR_ENV: &str = "DENNETT_DATA_DIR";
const PROJECT_ROOT_ENV: &str = "DENNETT_PROJECT_ROOT";
const RUNTIME_HOST_SCRIPT_ENV: &str = "DENNETT_RUNTIME_HOST_SCRIPT";

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
    let runtime_host_script = locate_runtime_host_script()?;
    let project_root = locate_project_root().unwrap_or_else(|| data_dir.to_path_buf());
    let child = Command::new(executable)
        .env(INSTALLATION_ID_ENV, &metadata.installation_id)
        .env(AUTHORITY_EPOCH_ENV, metadata.authority_epoch.to_string())
        .env(DATA_DIR_ENV, data_dir)
        .env(PROJECT_ROOT_ENV, project_root)
        .env(RUNTIME_HOST_SCRIPT_ENV, runtime_host_script)
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

fn locate_project_root() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(PROJECT_ROOT_ENV).map(PathBuf::from) {
        return path.is_dir().then_some(path);
    }
    std::env::current_exe()
        .ok()
        .and_then(|executable| executable.parent().and_then(repository_root_from))
        .or_else(|| {
            std::env::current_dir()
                .ok()
                .as_deref()
                .and_then(repository_root_from)
        })
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
    let executable_root = std::env::current_exe()
        .ok()
        .and_then(|executable| executable.parent().and_then(repository_root_from));
    let working_root = std::env::current_dir()
        .ok()
        .as_deref()
        .and_then(repository_root_from);
    for root in executable_root.into_iter().chain(working_root) {
        candidates.push(root.join("target/release").join(executable_name));
        candidates.push(root.join("target/debug").join(executable_name));
    }
    candidates
        .into_iter()
        .find_map(executable)
        .ok_or(NodeStartError::ExecutableMissing)
}

fn locate_runtime_host_script() -> Result<PathBuf, NodeStartError> {
    if let Some(path) = std::env::var_os(RUNTIME_HOST_SCRIPT_ENV).map(PathBuf::from) {
        return executable(path).ok_or(NodeStartError::RuntimeHostMissing);
    }
    std::env::current_exe()
        .ok()
        .and_then(|executable| executable.parent().and_then(repository_root_from))
        .map(|root| root.join("services/adapter-host-node/dist/index.js"))
        .and_then(executable)
        .ok_or(NodeStartError::RuntimeHostMissing)
}

fn repository_root_from(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .take(12)
        .find(|candidate| candidate.join("services/node/Cargo.toml").is_file())
        .map(Path::to_path_buf)
}

fn executable(path: PathBuf) -> Option<PathBuf> {
    path.is_file().then(|| path.canonicalize().unwrap_or(path))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum NodeStartError {
    ExecutableMissing,
    RuntimeHostMissing,
    StartFailed,
    #[cfg(not(windows))]
    UnsupportedPlatform,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_root_is_discovered_from_a_nested_desktop_binary_directory() {
        let directory = tempfile::tempdir().expect("temporary repository");
        let node_manifest = directory.path().join("services/node/Cargo.toml");
        std::fs::create_dir_all(node_manifest.parent().expect("manifest parent"))
            .expect("Node package directory");
        std::fs::write(node_manifest, "[package]\nname = \"dennett-node\"\n")
            .expect("Node manifest marker");
        let desktop_binary_directory = directory
            .path()
            .join("apps/desktop/src-tauri/target/release");
        std::fs::create_dir_all(&desktop_binary_directory).expect("desktop binary directory");

        assert_eq!(
            repository_root_from(&desktop_binary_directory),
            Some(directory.path().to_path_buf())
        );
    }
}
