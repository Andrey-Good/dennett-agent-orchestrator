#![cfg(windows)]

use dennett_local_ipc::protocol::dennett::control::v1::{
    ComposerDraft, ComposerDraftWriteState, SessionSnapshot, TurnActivityStatus, TurnDeliveryMode,
    TurnRole, TurnState, session_mutation, session_watch_frame,
};
use dennett_local_ipc::{
    AuthenticatedSystemClient, ClientCommand, ClientConfig, ClientSendTurnRequest,
};
use dennett_node::{
    AGENT_RUNTIME_ENV, AUTHORITY_EPOCH_ENV, DATA_DIR_ENV, INSTALLATION_ID_ENV, PROJECT_ROOT_ENV,
    RUNTIME_HOST_SCRIPT_ENV,
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
        .send_turn(ClientSendTurnRequest {
            command: ClientCommand {
                command_id: draft_command_id.clone(),
                correlation_id: "send-recovered-draft".to_owned(),
                created_at_unix_ms: unix_time_ms(),
                expected_revision: Some(revision),
            },
            project_id: project_id.clone(),
            session_id: session_id.clone(),
            text: recovered.text.clone(),
            attachments: Vec::new(),
            runtime_controls: Vec::new(),
            delivery_mode: TurnDeliveryMode::NewTurn,
            expected_active_turn_id: None,
        })
        .await
        .expect("send recovered draft");
    assert!(!accepted.turn_id.is_empty());
    let accepted_revision = accepted
        .command
        .as_ref()
        .expect("durable command admission")
        .accepted_revision;
    wait_for_completed_turn_id(&mut watch, &accepted.turn_id).await;
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
        .send_turn(ClientSendTurnRequest {
            command: ClientCommand {
                command_id: draft_command_id,
                correlation_id: "send-recovered-draft-after-restart".to_owned(),
                created_at_unix_ms: unix_time_ms(),
                expected_revision: Some(revision),
            },
            project_id: project_id.clone(),
            session_id: session_id.clone(),
            text: recovered.text,
            attachments: Vec::new(),
            runtime_controls: Vec::new(),
            delivery_mode: TurnDeliveryMode::NewTurn,
            expected_active_turn_id: None,
        })
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
async fn test_m01_standalone_chat_001_is_listed_and_runs_outside_the_project() {
    let profile = TempDir::new().expect("temporary desktop profile");
    let project = TempDir::new().expect("temporary project");
    let installation_id = format!("desktop-standalone-{}", uuid::Uuid::now_v7());
    let _node = spawn_node(&installation_id, profile.path(), project.path());
    let mut client = connect(&installation_id, "desktop-standalone-create").await;
    let created = client
        .create_session(
            ClientCommand::new("create-standalone-session", None),
            String::new(),
            "Standalone E2E".to_owned(),
        )
        .await
        .expect("create standalone chat through local IPC");
    let session_id = created.session_id;
    let mut watch = client
        .watch_session(session_id.clone(), None)
        .await
        .expect("watch standalone chat");
    let initial = next_snapshot(&mut watch).await;
    let summary = initial.session.as_ref().expect("standalone summary");
    assert!(summary.project_id.is_empty());

    client
        .send_turn(ClientSendTurnRequest {
            command: ClientCommand::new("send-standalone-turn", Some(summary.revision)),
            project_id: String::new(),
            session_id: session_id.clone(),
            text: "standalone prompt".to_owned(),
            attachments: Vec::new(),
            runtime_controls: Vec::new(),
            delivery_mode: TurnDeliveryMode::NewTurn,
            expected_active_turn_id: None,
        })
        .await
        .expect("send standalone turn");
    wait_for_completed_turn(&mut watch).await;
    drop(watch);
    let completed = session_snapshot(&mut client, &session_id).await;
    assert_eq!(
        completed.turns.last().expect("standalone agent turn").text,
        "Dennett skeleton received: standalone prompt"
    );
    assert!(profile.path().join("standalone-workspace").is_dir());

    let listed = connect(&installation_id, "desktop-standalone-list").await;
    assert!(
        listed
            .bootstrap()
            .recent_sessions
            .iter()
            .any(|session| { session.session_id == session_id && session.project_id.is_empty() })
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_m01_native_steer_001_keeps_one_active_turn_under_concurrent_progress() {
    let profile = TempDir::new().expect("temporary desktop profile");
    let project = TempDir::new().expect("temporary project");
    let fixture = TempDir::new().expect("temporary runtime fixture");
    let script = fixture.path().join("native-steer-runtime-host.mjs");
    std::fs::write(&script, NATIVE_STEER_RUNTIME_HOST).expect("write native steer fixture");
    let installation_id = format!("desktop-native-steer-{}", uuid::Uuid::now_v7());
    let _node = spawn_hosted_node(&installation_id, profile.path(), project.path(), &script);
    let mut client = connect(&installation_id, "desktop-native-steer").await;
    let runtime = client
        .bootstrap()
        .runtime
        .as_ref()
        .expect("runtime descriptor");
    assert_eq!(runtime.adapter_id, "fixture.native-steer");
    assert_eq!(runtime.steering, "native");
    let project_id = client.bootstrap().active_project_id.clone();
    let session_id = client.bootstrap().active_session_id.clone();
    let initial = session_snapshot(&mut client, &session_id).await;
    let revision = initial.session.as_ref().expect("session summary").revision;
    let mut watch = client
        .watch_session(session_id.clone(), Some(revision))
        .await
        .expect("watch native steer session");
    let accepted = client
        .send_turn(ClientSendTurnRequest {
            command: ClientCommand::new("start-native-steer", Some(revision)),
            project_id: project_id.clone(),
            session_id: session_id.clone(),
            text: "Start fixture work".to_owned(),
            attachments: Vec::new(),
            runtime_controls: Vec::new(),
            delivery_mode: TurnDeliveryMode::NewTurn,
            expected_active_turn_id: None,
        })
        .await
        .expect("start fixture turn");
    wait_for_running_activity(&mut watch, &accepted.turn_id).await;
    tokio::time::sleep(Duration::from_millis(20)).await;

    let steered = client
        .send_turn(ClientSendTurnRequest {
            command: ClientCommand::new("steer-native-turn", None),
            project_id: project_id.clone(),
            session_id: session_id.clone(),
            text: "Honor this clarification".to_owned(),
            attachments: Vec::new(),
            runtime_controls: Vec::new(),
            delivery_mode: TurnDeliveryMode::SteerNow,
            expected_active_turn_id: Some(accepted.turn_id.clone()),
        })
        .await;
    let steered = match steered {
        Ok(steered) => steered,
        Err(error) => {
            let observed = session_snapshot(&mut client, &session_id).await;
            panic!(
                "steer fixture turn through SQLite-backed Node: {error:?}; snapshot: {observed:?}"
            );
        }
    };
    assert_eq!(steered.turn_id, accepted.turn_id);
    wait_for_completed_turn_id(&mut watch, &accepted.turn_id).await;
    drop(watch);

    let completed = session_snapshot(&mut client, &session_id).await;
    assert!(completed.turns.iter().any(|turn| {
        turn.role == TurnRole::User as i32
            && turn.text == "Honor this clarification"
            && turn.state == TurnState::Completed as i32
    }));
    let agent_turns = completed
        .turns
        .iter()
        .filter(|turn| turn.role == TurnRole::Agent as i32)
        .collect::<Vec<_>>();
    assert_eq!(agent_turns.len(), 1);
    assert_eq!(agent_turns[0].turn_id, accepted.turn_id);
    assert_eq!(agent_turns[0].text, "STEER-RECEIVED");
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

fn spawn_hosted_node(
    installation_id: &str,
    data_dir: &std::path::Path,
    project: &std::path::Path,
    runtime_host: &std::path::Path,
) -> ChildGuard {
    ChildGuard(
        Command::new(env!("CARGO_BIN_EXE_dennett-node"))
            .env(INSTALLATION_ID_ENV, installation_id)
            .env(AUTHORITY_EPOCH_ENV, "31")
            .env(DATA_DIR_ENV, data_dir)
            .env(PROJECT_ROOT_ENV, project)
            .env(AGENT_RUNTIME_ENV, "codex")
            .env(RUNTIME_HOST_SCRIPT_ENV, runtime_host)
            .env("RUST_LOG", "error")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("start hosted dennett-node"),
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
    tokio::time::timeout(Duration::from_secs(10), async {
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

async fn wait_for_completed_turn_id(
    watch: &mut dennett_local_ipc::AuthenticatedSessionWatch,
    turn_id: &str,
) {
    tokio::time::timeout(Duration::from_secs(10), async {
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
                        if terminal.turn_id == turn_id
                            && terminal.state == TurnState::Completed as i32
                )
            }) {
                return;
            }
        }
    })
    .await
    .expect("target turn completion timeout");
}

async fn wait_for_running_activity(
    watch: &mut dennett_local_ipc::AuthenticatedSessionWatch,
    turn_id: &str,
) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let response = watch
                .message()
                .await
                .expect("session watch")
                .expect("session activity response");
            let Some(session_watch_frame::Frame::Delta(delta)) =
                response.frame.and_then(|frame| frame.frame)
            else {
                continue;
            };
            if delta.mutations.iter().any(|mutation| {
                matches!(
                    mutation.mutation,
                    Some(session_mutation::Mutation::UpsertTurnActivity(ref update))
                        if update.turn_id == turn_id
                            && update.activity.as_ref().is_some_and(|activity| matches!(
                                TurnActivityStatus::try_from(activity.status),
                                Ok(TurnActivityStatus::Started | TurnActivityStatus::Updated)
                            ))
                )
            }) {
                return;
            }
        }
    })
    .await
    .expect("running activity timeout");
}

const NATIVE_STEER_RUNTIME_HOST: &str = r#"
import readline from "node:readline";

const turns = new Map();
const write = value => process.stdout.write(JSON.stringify(value) + "\n");
const event = (turn, kind) => write({
  v: 1,
  event: "runtime_event",
  payload: {
    sessionId: turn.sessionId,
    turnId: turn.turnId,
    sequence: ++turn.sequence,
    kind,
    nativeExtensions: [],
  },
});

readline.createInterface({ input: process.stdin }).on("line", line => {
  const request = JSON.parse(line);
  if (request.method === "health") {
    write({ v: 1, id: request.id, result: { status: "healthy", protocolVersion: 1 } });
    return;
  }
  if (request.method === "describe") {
    write({ v: 1, id: request.id, result: {
      adapterId: "fixture.native-steer",
      runtimeKind: "native_agent",
      capabilities: {
        streaming: true,
        continuation: true,
        scopedCancellation: true,
        deadlines: true,
        steering: "native",
        nativeExtensionSchemas: [],
      },
      controls: [],
    }});
    return;
  }
  if (request.method === "start_turn") {
    const params = request.params;
    const turn = {
      sessionId: params.sessionId,
      turnId: params.turnId,
      sequence: 0,
      updates: 0,
      timer: undefined,
    };
    turns.set(turn.turnId, turn);
    write({ v: 1, id: request.id, result: { started: true } });
    event(turn, {
      type: "started",
      continuation: { adapterId: "fixture.native-steer", handle: "fixture-thread" },
    });
    event(turn, {
      type: "progress",
      activityId: "fixture-command",
      phase: "Running command",
      message: "Fixture command is running",
      status: "started",
    });
    turn.timer = setInterval(() => {
      turn.updates += 1;
      if (turn.updates > 30) {
        clearInterval(turn.timer);
        return;
      }
      event(turn, {
        type: "progress",
        activityId: "fixture-command",
        phase: "Running command",
        message: "Fixture command is running",
        status: "updated",
      });
    }, 2);
    return;
  }
  if (request.method === "steer_turn") {
    const params = request.params;
    const turn = turns.get(params.turnId);
    if (!turn || turn.sessionId !== params.sessionId) {
      write({ v: 1, id: request.id, error: {
        code: "scope_mismatch", retryable: false, recoverable: true,
      }});
      return;
    }
    clearInterval(turn.timer);
    write({ v: 1, id: request.id, result: {
      sessionId: params.sessionId,
      turnId: params.turnId,
      messageId: params.messageId,
    }});
    setTimeout(() => {
      event(turn, {
        type: "progress",
        activityId: "fixture-command",
        phase: "Running command",
        message: "Fixture command completed",
        status: "completed",
      });
      event(turn, { type: "text_delta", text: "STEER-RECEIVED" });
      event(turn, {
        type: "terminal",
        outcome: { type: "completed" },
        continuation: { adapterId: "fixture.native-steer", handle: "fixture-thread" },
      });
      turns.delete(turn.turnId);
    }, 25);
    return;
  }
  if (request.method === "cancel_turn") {
    write({ v: 1, id: request.id, result: {
      sessionId: request.params.sessionId,
      turnId: request.params.turnId,
      disposition: { type: "requested" },
    }});
  }
});
"#;

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
