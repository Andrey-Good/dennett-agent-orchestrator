#![cfg(windows)]

use dennett_local_ipc::{AuthenticatedSystemClient, ClientConfig};
use dennett_node::{AGENT_RUNTIME_ENV, AUTHORITY_EPOCH_ENV, INSTALLATION_ID_ENV};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Threading::{
    CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW, OpenProcess, PROCESS_SYNCHRONIZE,
    PROCESS_TERMINATE, TerminateProcess, WaitForSingleObject,
};

const CRASH_LAUNCHER_MODE: &str = "DENNETT_TEST_CRASH_LAUNCHER";
const CRASH_LAUNCHER_DATA_DIR: &str = "DENNETT_TEST_LAUNCHER_DATA_DIR";
const CRASH_LAUNCHER_PID_FILE: &str = "DENNETT_TEST_NODE_PID_FILE";

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct DetachedProcessGuard(u32);

impl Drop for DetachedProcessGuard {
    fn drop(&mut self) {
        // SAFETY: the handle is closed below and is used only for the process
        // created by this test's launcher helper.
        let handle = unsafe { OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, 0, self.0) };
        if handle.is_null() {
            return;
        }
        // SAFETY: handle refers to the test-owned Node process.
        unsafe {
            TerminateProcess(handle, 0);
            WaitForSingleObject(handle, 5_000);
            CloseHandle(handle);
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn desktop_disconnect_does_not_stop_node_and_a_new_session_reconnects() {
    let installation_id = format!("node-lifecycle-{}", uuid::Uuid::now_v7());
    let child = Command::new(env!("CARGO_BIN_EXE_dennett-node"))
        .env(INSTALLATION_ID_ENV, &installation_id)
        .env(AUTHORITY_EPOCH_ENV, "17")
        .env(AGENT_RUNTIME_ENV, "fake")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start dennett-node");
    let mut node = ChildGuard(child);

    let first = connect(&installation_id, "desktop-session-one").await;
    assert_usable_bootstrap(first.bootstrap(), 17);
    drop(first);

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        node.0.try_wait().expect("query Node process").is_none(),
        "closing the desktop-side connection must not terminate Node"
    );

    let second = connect(&installation_id, "desktop-session-two").await;
    assert_usable_bootstrap(second.bootstrap(), 17);
    assert!(
        node.0.try_wait().expect("query Node process").is_none(),
        "Node must remain alive after a fresh authenticated UI session"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn abrupt_desktop_launcher_exit_leaves_node_available_for_reconnect() {
    let directory = tempfile::tempdir().expect("temporary Node data directory");
    let installation_id = format!("node-launcher-crash-{}", uuid::Uuid::now_v7());
    let pid_file = directory.path().join("node.pid");
    let launcher = Command::new(std::env::current_exe().expect("test executable"))
        .arg("--exact")
        .arg("crash_launcher_helper")
        .arg("--nocapture")
        .env(CRASH_LAUNCHER_MODE, &installation_id)
        .env(CRASH_LAUNCHER_DATA_DIR, directory.path())
        .env(CRASH_LAUNCHER_PID_FILE, &pid_file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start crash launcher");
    let status = tokio::task::spawn_blocking(move || {
        let mut launcher = launcher;
        launcher.wait()
    })
    .await
    .expect("join crash launcher")
    .expect("wait for crash launcher");
    assert!(!status.success(), "launcher must terminate abruptly");

    let process_id = read_process_id(&pid_file).await;
    let _node = DetachedProcessGuard(process_id);
    let first = connect(&installation_id, "desktop-after-launcher-crash").await;
    assert_usable_bootstrap(first.bootstrap(), 23);
    drop(first);

    let second = connect(&installation_id, "desktop-reconnected").await;
    assert_usable_bootstrap(second.bootstrap(), 23);
}

fn assert_usable_bootstrap(
    bootstrap: &dennett_local_ipc::protocol::dennett::control::v1::BootstrapSnapshot,
    authority_epoch: u64,
) {
    assert_eq!(bootstrap.authority_epoch, authority_epoch);
    assert!(bootstrap.revision >= 2);
    assert_eq!(bootstrap.projects.len(), 1);
    assert_eq!(bootstrap.recent_sessions.len(), 1);
    assert!(!bootstrap.active_project_id.is_empty());
    assert!(!bootstrap.active_session_id.is_empty());
}

#[test]
fn crash_launcher_helper() {
    let Ok(installation_id) = std::env::var(CRASH_LAUNCHER_MODE) else {
        return;
    };
    let data_dir =
        PathBuf::from(std::env::var_os(CRASH_LAUNCHER_DATA_DIR).expect("launcher data directory"));
    let pid_file =
        PathBuf::from(std::env::var_os(CRASH_LAUNCHER_PID_FILE).expect("launcher PID file"));
    let child = Command::new(env!("CARGO_BIN_EXE_dennett-node"))
        .env(INSTALLATION_ID_ENV, installation_id)
        .env(AUTHORITY_EPOCH_ENV, "23")
        .env(AGENT_RUNTIME_ENV, "fake")
        .current_dir(data_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW)
        .spawn()
        .expect("launcher starts detached Node");
    std::fs::write(pid_file, child.id().to_string()).expect("persist detached Node PID");
    drop(child);
    std::process::abort();
}

async fn read_process_id(pid_file: &std::path::Path) -> u32 {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(value) = tokio::fs::read_to_string(pid_file).await
                && let Ok(process_id) = value.parse::<u32>()
            {
                return process_id;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("detached Node PID timeout")
}

async fn connect(installation_id: &str, device_id: &str) -> AuthenticatedSystemClient {
    tokio::time::timeout(
        Duration::from_secs(5),
        AuthenticatedSystemClient::connect(ClientConfig::m01(
            installation_id,
            device_id,
            "desktop-lifecycle-test",
        )),
    )
    .await
    .expect("Node connection timed out")
    .expect("authenticated Node connection")
}
