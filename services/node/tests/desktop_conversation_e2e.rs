#![cfg(windows)]

use dennett_local_ipc::protocol::dennett::control::v1::{
    ComposerDraft, ComposerDraftWriteState, SessionSnapshot, TurnState, session_mutation,
    session_watch_frame,
};
use dennett_local_ipc::{AuthenticatedSystemClient, ClientCommand, ClientConfig};
use dennett_node::{
    AGENT_RUNTIME_ENV, AUTHORITY_EPOCH_ENV, DATA_DIR_ENV, INSTALLATION_ID_ENV, PROJECT_ROOT_ENV,
};
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
async fn test_m01_desktop_conversation_e2e_001_restores_draft_and_completed_turn() {
    let profile = TempDir::new().expect("temporary desktop profile");
    let project = TempDir::new().expect("temporary project");
    let installation_id = format!("desktop-conversation-{}", uuid::Uuid::now_v7());
    let mut node = spawn_node(&installation_id, profile.path(), project.path());
    let mut first = connect(&installation_id, "desktop-window-one").await;
    let runtime = first
        .bootstrap()
        .runtime
        .as_ref()
        .expect("runtime descriptor is visible to the desktop");
    assert_eq!(runtime.adapter_id, "dennett.fake");
    assert!(!runtime.streaming);
    let project_id = first.bootstrap().active_project_id.clone();
    assert!(!project_id.is_empty());
    tokio::time::sleep(Duration::from_millis(2)).await;
    let created = first
        .create_session(
            ClientCommand::new("create-desktop-e2e-session", None),
            project_id.clone(),
            "Desktop E2E conversation".to_owned(),
        )
        .await
        .expect("create project chat through local IPC");
    let session_id = created.session_id;
    assert!(!session_id.is_empty());

    let draft_command_id = uuid::Uuid::now_v7().to_string();
    let saved = first
        .save_composer_draft(
            ClientCommand::new("draft-save-one", None),
            ComposerDraft {
                project_id: project_id.clone(),
                session_id: session_id.clone(),
                command_id: draft_command_id.clone(),
                text: "recover and send exactly once".to_owned(),
                updated_at: Some(timestamp(unix_time_ms())),
                revision: 1,
            },
        )
        .await
        .expect("persist composer draft");
    assert_eq!(
        ComposerDraftWriteState::try_from(saved.state).expect("draft state"),
        ComposerDraftWriteState::Saved
    );
    drop(first);

    let mut reopened = connect(&installation_id, "desktop-window-two").await;
    let recovered = reopened
        .get_composer_draft(project_id.clone(), session_id.clone())
        .await
        .expect("load recovered draft")
        .expect("draft survives UI disconnect");
    assert_eq!(recovered.command_id, draft_command_id);
    assert_eq!(recovered.text, "recover and send exactly once");

    let mut watch = reopened
        .watch_session(session_id.clone(), None)
        .await
        .expect("watch restored session");
    let initial = next_snapshot(&mut watch).await;
    let revision = initial.session.as_ref().expect("session summary").revision;
    let accepted = reopened
        .send_turn(
            ClientCommand {
                command_id: draft_command_id.clone(),
                correlation_id: "send-recovered-draft".to_owned(),
                created_at_unix_ms: unix_time_ms(),
                expected_revision: Some(revision),
            },
            project_id.clone(),
            session_id.clone(),
            recovered.text.clone(),
            Vec::new(),
        )
        .await
        .expect("send recovered draft");
    assert!(!accepted.turn_id.is_empty());
    let accepted_revision = accepted
        .command
        .as_ref()
        .expect("durable command admission")
        .accepted_revision;
    wait_for_completed_turn(&mut watch).await;
    assert!(
        reopened
            .get_composer_draft(project_id.clone(), session_id.clone())
            .await
            .expect("draft lookup after send")
            .is_none(),
        "durably accepted SendTurn must consume its matching draft"
    );
    drop(watch);
    drop(reopened);

    node.stop();
    let _restarted_node = spawn_node(&installation_id, profile.path(), project.path());
    let mut restored = connect(&installation_id, "desktop-after-node-restart").await;
    assert_eq!(restored.bootstrap().active_session_id, session_id);
    let mut restored_watch = restored
        .watch_session(session_id.clone(), None)
        .await
        .expect("watch after Node restart");
    let snapshot = next_snapshot(&mut restored_watch).await;
    let agent_turn = snapshot.turns.last().expect("restored agent turn");
    assert_eq!(
        TurnState::try_from(agent_turn.state).expect("terminal turn state"),
        TurnState::Completed
    );
    assert_eq!(
        agent_turn.text,
        "Dennett skeleton received: recover and send exactly once"
    );
    let replayed = restored
        .send_turn(
            ClientCommand {
                command_id: draft_command_id,
                correlation_id: "send-recovered-draft-after-restart".to_owned(),
                created_at_unix_ms: unix_time_ms(),
                expected_revision: Some(revision),
            },
            project_id.clone(),
            session_id.clone(),
            recovered.text,
            Vec::new(),
        )
        .await
        .expect("replay admitted command after restart");
    assert_eq!(replayed.turn_id, accepted.turn_id);
    assert_eq!(
        replayed
            .command
            .as_ref()
            .expect("replayed admission")
            .accepted_revision,
        accepted_revision
    );
    assert!(
        restored
            .get_composer_draft(project_id, session_id)
            .await
            .expect("draft lookup after restart")
            .is_none()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_desktop_draft_recovery_001_discard_does_not_mutate_session_history() {
    let profile = TempDir::new().expect("temporary desktop profile");
    let project = TempDir::new().expect("temporary project");
    let installation_id = format!("desktop-draft-discard-{}", uuid::Uuid::now_v7());
    let _node = spawn_node(&installation_id, profile.path(), project.path());
    let mut client = connect(&installation_id, "desktop-draft-discard").await;
    let project_id = client.bootstrap().active_project_id.clone();
    let session_id = client.bootstrap().active_session_id.clone();
    let before = session_snapshot(&mut client, &session_id).await;
    let command_id = uuid::Uuid::now_v7().to_string();

    client
        .save_composer_draft(
            ClientCommand::new("save-discarded-draft", None),
            ComposerDraft {
                project_id: project_id.clone(),
                session_id: session_id.clone(),
                command_id: command_id.clone(),
                text: "never becomes session history".to_owned(),
                updated_at: Some(timestamp(unix_time_ms())),
                revision: 1,
            },
        )
        .await
        .expect("save draft before discard");
    assert!(
        client
            .discard_composer_draft(
                ClientCommand::new("discard-draft", None),
                project_id,
                session_id.clone(),
                command_id,
            )
            .await
            .expect("discard draft")
            .existed
    );
    let after = session_snapshot(&mut client, &session_id).await;
    assert_eq!(after.session, before.session);
    assert_eq!(after.turns, before.turns);
}

fn spawn_node(
    installation_id: &str,
    data_dir: &std::path::Path,
    project: &std::path::Path,
) -> ChildGuard {
    ChildGuard(
        Command::new(env!("CARGO_BIN_EXE_dennett-node"))
            .env(INSTALLATION_ID_ENV, installation_id)
            .env(AUTHORITY_EPOCH_ENV, "31")
            .env(DATA_DIR_ENV, data_dir)
            .env(PROJECT_ROOT_ENV, project)
            .env(AGENT_RUNTIME_ENV, "fake")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start dennett-node"),
    )
}

async fn connect(installation_id: &str, device_id: &str) -> AuthenticatedSystemClient {
    tokio::time::timeout(
        Duration::from_secs(5),
        AuthenticatedSystemClient::connect(ClientConfig::m01(
            installation_id,
            device_id,
            "desktop-conversation-e2e",
        )),
    )
    .await
    .expect("Node connection timed out")
    .expect("authenticated Node connection")
}

async fn next_snapshot(
    watch: &mut dennett_local_ipc::AuthenticatedSessionWatch,
) -> SessionSnapshot {
    let response = tokio::time::timeout(Duration::from_secs(5), watch.message())
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

async fn session_snapshot(
    client: &mut AuthenticatedSystemClient,
    session_id: &str,
) -> SessionSnapshot {
    let mut watch = client
        .watch_session(session_id.to_owned(), None)
        .await
        .expect("watch session snapshot");
    next_snapshot(&mut watch).await
}

async fn wait_for_completed_turn(watch: &mut dennett_local_ipc::AuthenticatedSessionWatch) {
    tokio::time::timeout(Duration::from_secs(5), async {
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
            if delta.mutations.iter().any(|mutation| {
                matches!(
                    mutation.mutation,
                    Some(session_mutation::Mutation::FinishTurn(ref terminal))
                        if terminal.state == TurnState::Completed as i32
                )
            }) {
                return;
            }
        }
    })
    .await
    .expect("completed turn timeout");
}

fn timestamp(unix_ms: u64) -> prost_types::Timestamp {
    prost_types::Timestamp {
        seconds: (unix_ms / 1_000).try_into().unwrap_or(i64::MAX),
        nanos: ((unix_ms % 1_000) * 1_000_000) as i32,
    }
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}
