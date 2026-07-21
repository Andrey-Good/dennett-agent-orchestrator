#![cfg(windows)]

use dennett_local_ipc::protocol::dennett::control::v1::{
    SessionSnapshot, TurnActivityStatus, TurnDeliveryMode, TurnRole, TurnState, session_mutation,
    session_watch_frame,
};
use dennett_local_ipc::{
    AuthenticatedSystemClient, ClientCommand, ClientConfig, ClientSendTurnRequest,
};
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
    let full_access_target = TempDir::new().expect("temporary full-access target");
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
    assert_eq!(
        runtime
            .controls
            .iter()
            .map(|control| control.label.as_str())
            .collect::<Vec<_>>(),
        vec!["Agent access", "Model", "Reasoning", "Speed"]
    );
    let runtime_controls = runtime
        .controls
        .iter()
        .map(|control| (control.id.clone(), control.default_choice_id.clone()))
        .collect::<Vec<_>>();
    let project_id = client.bootstrap().active_project_id.clone();
    let session_id = client.bootstrap().active_session_id.clone();
    let first = send_live_turn(
        &mut client,
        &project_id,
        &session_id,
        "Create dennett-live-auto-approve.txt in the current project with the exact text `auto-approve works`, then reply with one short sentence. Use a tool; do not only explain the command.",
        &runtime_controls,
    )
    .await;
    assert_completed_nonempty(&first);
    assert_live_file(
        &project.path().join("dennett-live-auto-approve.txt"),
        "auto-approve works",
        &first,
        "Auto-approve writes inside the project sandbox",
    );
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

    let full_access_controls = runtime_controls
        .iter()
        .map(|(control_id, choice_id)| {
            if control_id == "dennett.access_mode" {
                (control_id.clone(), "full_access".to_owned())
            } else {
                (control_id.clone(), choice_id.clone())
            }
        })
        .collect::<Vec<_>>();
    let full_access_file = full_access_target
        .path()
        .join("dennett-live-full-access.txt");
    let second_prompt = format!(
        "Create the file `{}` outside the current project with the exact text `full access works`, then reply briefly. Use a tool; do not only explain the command.",
        full_access_file.display()
    );
    let second = send_live_turn(
        &mut restored,
        &project_id,
        &session_id,
        &second_prompt,
        &full_access_controls,
    )
    .await;
    assert_completed_nonempty(&second);
    assert_live_file(
        &full_access_file,
        "full access works",
        &second,
        "Full access writes outside the project sandbox",
    );
    assert!(
        second.turns.len() > first.turns.len(),
        "the restored session must append rather than replace history"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires ChatGPT subscription authentication and live Codex access"]
async fn live_desktop_ipc_steers_active_codex_turn_without_restart() {
    let profile = TempDir::new().expect("temporary desktop profile");
    let project = TempDir::new().expect("temporary project");
    let installation_id = format!("desktop-codex-live-steer-{}", uuid::Uuid::now_v7());
    let runtime_host = runtime_host_script();
    assert!(
        runtime_host.is_file(),
        "build @dennett/adapter-host-node before the live canary"
    );

    let _node = spawn_node(
        &installation_id,
        profile.path(),
        project.path(),
        &runtime_host,
    );
    let mut client = connect(&installation_id, "desktop-codex-live-steer").await;
    let runtime = client
        .bootstrap()
        .runtime
        .as_ref()
        .expect("Codex runtime descriptor is visible to the desktop");
    assert_eq!(runtime.steering, "native");
    let runtime_controls = runtime
        .controls
        .iter()
        .map(|control| (control.id.clone(), control.default_choice_id.clone()))
        .collect::<Vec<_>>();
    let project_id = client.bootstrap().active_project_id.clone();
    let session_id = client.bootstrap().active_session_id.clone();

    let steered =
        send_live_steered_turn(&mut client, &project_id, &session_id, &runtime_controls).await;
    let agent_turns = steered
        .turns
        .iter()
        .filter(|turn| turn.role == TurnRole::Agent as i32)
        .collect::<Vec<_>>();
    let final_agent = agent_turns.last().expect("steered agent turn");
    assert!(
        final_agent.text.contains("STEER-RECEIVED"),
        "the final response must honor the in-flight steer: {:?}",
        final_agent.text
    );
    assert!(
        steered.turns.iter().any(|turn| {
            turn.role == TurnRole::User as i32
                && turn.text.contains("STEER-RECEIVED")
                && TurnState::try_from(turn.state).ok() == Some(TurnState::Completed)
        }),
        "the in-flight user steer must be durably visible in session history"
    );
}

async fn send_live_steered_turn(
    client: &mut AuthenticatedSystemClient,
    project_id: &str,
    session_id: &str,
    runtime_controls: &[(String, String)],
) -> SessionSnapshot {
    let initial = session_snapshot(client, session_id).await;
    let revision = initial.session.as_ref().expect("session summary").revision;
    let mut watch = client
        .watch_session(session_id.to_owned(), Some(revision))
        .await
        .expect("watch live steer session");
    let accepted = client
        .send_turn(ClientSendTurnRequest {
            command: ClientCommand {
                command_id: uuid::Uuid::now_v7().to_string(),
                correlation_id: format!("codex-live-steer-start-{}", uuid::Uuid::now_v7()),
                created_at_unix_ms: unix_time_ms(),
                expected_revision: Some(revision),
            },
            project_id: project_id.to_owned(),
            session_id: session_id.to_owned(),
            text: "Run exactly this PowerShell command with the shell tool: `Start-Sleep -Seconds 20`. While it runs, accept any new user constraint in this same response. After the command finishes, reply with the exact token `BEFORE-STEER` unless the user changes that token while you are working."
                .to_owned(),
            attachments: Vec::new(),
            runtime_controls: runtime_controls.to_vec(),
            delivery_mode: TurnDeliveryMode::NewTurn,
            expected_active_turn_id: None,
        })
        .await
        .expect("start live steer turn");
    assert!(!accepted.turn_id.is_empty());
    wait_for_running_activity(&mut watch, &accepted.turn_id).await;

    let steered = client
        .send_turn(ClientSendTurnRequest {
            command: ClientCommand {
                command_id: uuid::Uuid::now_v7().to_string(),
                correlation_id: format!("codex-live-steer-update-{}", uuid::Uuid::now_v7()),
                created_at_unix_ms: unix_time_ms(),
                expected_revision: None,
            },
            project_id: project_id.to_owned(),
            session_id: session_id.to_owned(),
            text: "Change the required final reply token to `STEER-RECEIVED`. Continue the current work; do not start it over."
                .to_owned(),
            attachments: Vec::new(),
            runtime_controls: Vec::new(),
            delivery_mode: TurnDeliveryMode::SteerNow,
            expected_active_turn_id: Some(accepted.turn_id.clone()),
        })
        .await;
    let steered = match steered {
        Ok(steered) => steered,
        Err(error) => {
            let observed = session_snapshot(client, session_id).await;
            panic!(
                "steer the active live Codex turn: {error:?}; active turn: {:?}; turns: {:?}",
                observed
                    .session
                    .as_ref()
                    .and_then(|session| (!session.active_turn_id.is_empty())
                        .then_some(session.active_turn_id.as_str())),
                observed.turns
            );
        }
    };
    assert_eq!(
        steered.turn_id, accepted.turn_id,
        "native steering must keep the original provider turn"
    );
    wait_for_completed_turn_id(&mut watch, &accepted.turn_id).await;
    drop(watch);
    session_snapshot(client, session_id).await
}

async fn send_live_turn(
    client: &mut AuthenticatedSystemClient,
    project_id: &str,
    session_id: &str,
    prompt: &str,
    runtime_controls: &[(String, String)],
) -> SessionSnapshot {
    let initial = session_snapshot(client, session_id).await;
    let revision = initial.session.as_ref().expect("session summary").revision;
    let mut watch = client
        .watch_session(session_id.to_owned(), Some(revision))
        .await
        .expect("watch live session");
    client
        .send_turn(ClientSendTurnRequest {
            command: ClientCommand {
                command_id: uuid::Uuid::now_v7().to_string(),
                correlation_id: format!("codex-live-{}", uuid::Uuid::now_v7()),
                created_at_unix_ms: unix_time_ms(),
                expected_revision: Some(revision),
            },
            project_id: project_id.to_owned(),
            session_id: session_id.to_owned(),
            text: prompt.to_owned(),
            attachments: Vec::new(),
            runtime_controls: runtime_controls.to_vec(),
            delivery_mode: TurnDeliveryMode::NewTurn,
            expected_active_turn_id: None,
        })
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

async fn wait_for_completed_turn_id(
    watch: &mut dennett_local_ipc::AuthenticatedSessionWatch,
    turn_id: &str,
) {
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
                if let Some(session_mutation::Mutation::FinishTurn(terminal)) = mutation.mutation
                    && terminal.turn_id == turn_id
                {
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
    .expect("live Codex target terminal timeout");
}

async fn wait_for_running_activity(
    watch: &mut dennett_local_ipc::AuthenticatedSessionWatch,
    turn_id: &str,
) {
    tokio::time::timeout(Duration::from_secs(45), async {
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
            for mutation in delta.mutations {
                match mutation.mutation {
                    Some(session_mutation::Mutation::UpsertTurnActivity(update))
                        if update.turn_id == turn_id
                            && update.activity.as_ref().is_some_and(|activity| {
                                matches!(
                                    TurnActivityStatus::try_from(activity.status),
                                    Ok(TurnActivityStatus::Started | TurnActivityStatus::Updated)
                                )
                            }) =>
                    {
                        return;
                    }
                    Some(session_mutation::Mutation::FinishTurn(terminal))
                        if terminal.turn_id == turn_id =>
                    {
                        panic!(
                            "live Codex turn finished before it could be steered: state={:?}, outcome={:?}",
                            TurnState::try_from(terminal.state),
                            terminal.outcome
                        );
                    }
                    _ => {}
                }
            }
        }
    })
    .await
    .expect("live Codex never exposed a running activity for steering");
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

fn assert_live_file(path: &Path, expected: &str, snapshot: &SessionSnapshot, context: &str) {
    let observed = std::fs::read_to_string(path).unwrap_or_else(|error| {
        panic!(
            "{context}: {error}; final agent turn: {:?}",
            snapshot.turns.last()
        )
    });
    assert_eq!(
        observed.trim_end_matches(['\r', '\n']),
        expected,
        "{context}"
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
