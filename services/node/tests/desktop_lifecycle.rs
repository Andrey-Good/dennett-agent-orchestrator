#![cfg(windows)]

use dennett_local_ipc::{AuthenticatedSystemClient, ClientConfig};
use dennett_node::{AUTHORITY_EPOCH_ENV, INSTALLATION_ID_ENV};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn desktop_disconnect_does_not_stop_node_and_a_new_session_reconnects() {
    let installation_id = format!("node-lifecycle-{}", uuid::Uuid::now_v7());
    let child = Command::new(env!("CARGO_BIN_EXE_dennett-node"))
        .env(INSTALLATION_ID_ENV, &installation_id)
        .env(AUTHORITY_EPOCH_ENV, "17")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start dennett-node");
    let mut node = ChildGuard(child);

    let first = connect(&installation_id, "desktop-session-one").await;
    assert_eq!(first.bootstrap().authority_epoch, 17);
    assert_eq!(first.bootstrap().revision, 1);
    drop(first);

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        node.0.try_wait().expect("query Node process").is_none(),
        "closing the desktop-side connection must not terminate Node"
    );

    let second = connect(&installation_id, "desktop-session-two").await;
    assert_eq!(second.bootstrap().authority_epoch, 17);
    assert_eq!(second.bootstrap().revision, 1);
    assert!(
        node.0.try_wait().expect("query Node process").is_none(),
        "Node must remain alive after a fresh authenticated UI session"
    );
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
