#![cfg(windows)]

use dennett_local_ipc::protocol::dennett::control::v1::{
    SessionSnapshot, TurnState, session_mutation, session_watch_frame,
};
use dennett_local_ipc::{AuthenticatedSystemClient, ClientCommand, ClientConfig};
use dennett_node::{
    AGENT_RUNTIME_ENV, AUTHORITY_EPOCH_ENV, DATA_DIR_ENV, INSTALLATION_ID_ENV, PROJECT_ROOT_ENV,
    RUNTIME_HOST_SCRIPT_ENV,
};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tempfile::TempDir;

struct ChildGuard(Child);

impl ChildGuard {
    fn stop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        self.stop();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires ChatGPT subscription authentication and live Codex access"]
async fn live_desktop_ipc_survives_node_restart_and_continues_codex_session() {
    let profile = TempDir::new().expect("temporary desktop profile");
    let project = TempDir::new().expect("temporary project");
    let installation_id = format!("desktop-codex-live-{}", uuid::Uuid::now_v7());
    let runtime_host = runtime_host_script();
    assert!(
        runtime_host.is_file(),
        "build @dennett/adapter-host-node before the live canary"
    );

    let mut node = spawn_node(
        &installation_id,
        profile.path(),
        project.path(),
        &runtime_host,
    );
    let mut client = connect(&installation_id, "desktop-codex-live-one").await;
    let runtime = client
        .bootstrap()
        .runtime
        .as_ref()
        .expect("Codex runtime descriptor is visible to the desktop");
    assert_eq!(runtime.adapter_id, "openai.codex.sdk");
    assert!(runtime.streaming);
    let project_id = client.bootstrap().active_project_id.clone();
    let session_id = client.bootstrap().active_session_id.clone();
    let first = send_live_turn(
        &mut client,
        &project_id,
        &session_id,
        "Reply with one short sentence confirming that the Dennett live conversation reached you.",
    )
    .await;
    assert_completed_nonempty(&first);
    drop(client);

    node.stop();
    let _restarted = spawn_node(
        &installation_id,
        profile.path(),
        project.path(),
        &runtime_host,
    );
    let mut restored = connect(&installation_id, "desktop-codex-live-two").await;
    assert_eq!(restored.bootstrap().active_project_id, project_id);
    assert_eq!(restored.bootstrap().active_session_id, session_id);
    let restored_before_second = session_snapshot(&mut restored, &session_id).await;
    assert_completed_nonempty(&restored_before_second);

    let second = send_live_turn(
        &mut restored,
        &project_id,
        &session_id,
        "Reply briefly that this second message continued the restored Dennett session.",
    )
    .await;
    assert_completed_nonempty(&second);
    assert!(
        second.turns.len() > first.turns.len(),
        "the restored session must append rather than replace history"
    );
}

async fn send_live_turn(
    client: &mut AuthenticatedSystemClient,
    project_id: &str,
    session_id: &str,
    prompt: &str,
) -> SessionSnapshot {
    let initial = session_snapshot(client, session_id).await;
    let revision = initial.session.as_ref().expect("session summary").revision;
    let mut watch = client
        .watch_session(session_id.to_owned(), Some(revision))
        .await
        .expect("watch live session");
    client
        .send_turn(
            ClientCommand {
                command_id: uuid::Uuid::now_v7().to_string(),
                correlation_id: format!("codex-live-{}", uuid::Uuid::now_v7()),
                created_at_unix_ms: unix_time_ms(),
                expected_revision: Some(revision),
            },
            project_id.to_owned(),
            session_id.to_owned(),
            prompt.to_owned(),
            Vec::new(),
        )
        .await
        .expect("Codex turn admitted through local IPC");
    wait_for_completed_turn(&mut watch).await;
    drop(watch);
    session_snapshot(client, session_id).await
}

async fn session_snapshot(
    client: &mut AuthenticatedSystemClient,
    session_id: &str,
) -> SessionSnapshot {
    let mut watch = client
        .watch_session(session_id.to_owned(), None)
        .await
        .expect("watch session snapshot");
    let response = tokio::time::timeout(Duration::from_secs(10), watch.message())
        .await
        .expect("session snapshot timeout")
        .expect("session watch")
        .expect("session snapshot response");
    match response
        .frame
        .expect("session frame")
        .frame
        .expect("session payload")
    {
        session_watch_frame::Frame::Snapshot(snapshot) => snapshot,
        other => panic!("expected snapshot, got {other:?}"),
    }
}

async fn wait_for_completed_turn(watch: &mut dennett_local_ipc::AuthenticatedSessionWatch) {
    tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            let response = watch
                .message()
                .await
                .expect("session watch")
                .expect("session delta response");
            let Some(session_watch_frame::Frame::Delta(delta)) =
                response.frame.and_then(|frame| frame.frame)
            else {
                continue;
            };
            for mutation in delta.mutations {
                if let Some(session_mutation::Mutation::FinishTurn(terminal)) = mutation.mutation {
                    let state = TurnState::try_from(terminal.state).expect("terminal state");
                    assert_eq!(
                        state,
                        TurnState::Completed,
                        "live Codex turn did not complete: {:?}",
                        terminal.outcome
                    );
                    return;
                }
            }
        }
    })
    .await
    .expect("live Codex terminal timeout");
}

fn assert_completed_nonempty(snapshot: &SessionSnapshot) {
    let turn = snapshot.turns.last().expect("agent turn");
    assert_eq!(
        TurnState::try_from(turn.state).expect("turn state"),
        TurnState::Completed
    );
    assert!(
        !turn.text.trim().is_empty(),
        "Codex response must be visible"
    );
}

fn spawn_node(
    installation_id: &str,
    data_dir: &Path,
    project: &Path,
    runtime_host: &Path,
) -> ChildGuard {
    ChildGuard(
        Command::new(env!("CARGO_BIN_EXE_dennett-node"))
            .env(INSTALLATION_ID_ENV, installation_id)
            .env(AUTHORITY_EPOCH_ENV, "43")
            .env(DATA_DIR_ENV, data_dir)
            .env(PROJECT_ROOT_ENV, project)
            .env(AGENT_RUNTIME_ENV, "codex")
            .env(RUNTIME_HOST_SCRIPT_ENV, runtime_host)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start Codex-backed dennett-node"),
    )
}

async fn connect(installation_id: &str, device_id: &str) -> AuthenticatedSystemClient {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if let Ok(client) = AuthenticatedSystemClient::connect(ClientConfig::m01(
                installation_id,
                device_id,
                "desktop-codex-live-e2e",
            ))
            .await
            {
                return client;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("Node connection timed out")
}

fn runtime_host_script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repository root")
        .join("services/adapter-host-node/dist/index.js")
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}
