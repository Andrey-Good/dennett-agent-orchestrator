use async_trait::async_trait;
use dennett_agent_core::{
    AgentRequest, AgentResponse, AgentRuntimePort, CancelDisposition, CancelRuntimeTurnRequest,
    CancellationAcknowledgement, FakeRuntimeStep, RuntimeCapabilities, RuntimeControlSelection,
    RuntimeDescriptor, RuntimeError, RuntimeErrorCode, RuntimeEvent, RuntimeEventKind,
    RuntimeEventStream, RuntimeKind, RuntimeSteeringMode, RuntimeTerminal, RuntimeTerminalOutcome,
    RuntimeTurn, RuntimeTurnRequest, ScriptedFakeAgentRuntime, SteerRuntimeTurnRequest,
    SteeringAcknowledgement,
};
use dennett_contracts::{CommandId, ProjectId, SessionId};
use dennett_head::conversation::{
    ConversationApplication, ConversationError, ConversationTurnRequest, LocalProject,
    TraceContext, TurnDeliveryMode,
};
use dennett_head::session::SessionCoordinator;
use dennett_head::system::{SystemProjection, SystemSnapshot, SystemStatePort};
use dennett_kernel::DennettResult;
use dennett_memory_core::session::{
    InMemorySessionEventStore, ProjectSessionSnapshot, SessionActivityStatus, SessionEventBody,
    SessionJournal, SessionTurnOutcome, SessionTurnState,
};
use dennett_sync_core::watch::WatchFrame;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::Notify;
use tracing::Subscriber;
use tracing::field::{Field, Visit};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::LookupSpan;

fn application(
    runtime: Arc<dyn AgentRuntimePort>,
    timeout: Duration,
) -> (ConversationApplication, ProjectId) {
    let _ = trace_capture();
    let project_id = ProjectId::new();
    let coordinator = SessionCoordinator::new(
        SessionJournal::new(Arc::new(InMemorySessionEventStore::default())),
        7,
        16,
    );
    let system = Arc::new(SystemProjection::new(SystemSnapshot::empty(7), 16));
    (
        ConversationApplication::new(
            coordinator,
            system,
            runtime,
            LocalProject {
                project_id,
                display_name: "Test project".to_owned(),
                workspace_path: "C:\\test-project".to_owned(),
                standalone_workspace_path: "C:\\test-scratch".to_owned(),
            },
        )
        .with_unregistered_project_fixture_for_tests()
        .with_turn_timeout(timeout),
        project_id,
    )
}

fn trace(correlation_id: &str) -> TraceContext {
    TraceContext {
        installation_id: "installation-test".to_owned(),
        device_id: "device-test".to_owned(),
        correlation_id: correlation_id.to_owned(),
        authority_epoch: 7,
    }
}

fn turn_request(
    correlation_id: &str,
    command_id: CommandId,
    project_id: ProjectId,
    session_id: SessionId,
    expected_revision: Option<u64>,
    text: &str,
) -> ConversationTurnRequest {
    ConversationTurnRequest {
        trace: trace(correlation_id),
        command_id,
        project_id: Some(project_id),
        session_id,
        expected_revision,
        text: text.to_owned(),
        context_handles: Vec::new(),
        runtime_controls: Vec::new(),
        delivery_mode: TurnDeliveryMode::NewTurn,
        expected_active_turn_id: None,
    }
}

#[tokio::test]
async fn native_steer_keeps_the_provider_turn_running_and_is_idempotently_durable() {
    let runtime = Arc::new(ControlledRuntime::new(true));
    let (application, project_id) = application(runtime.clone(), Duration::from_secs(2));
    let initialized = application
        .initialize(CommandId::new(), "Native steer".to_owned())
        .await
        .expect("initialize");
    let session_id = initialized.session.session_id;
    let accepted = application
        .send_turn(turn_request(
            "correlation-start-steer",
            CommandId::new(),
            project_id,
            session_id,
            Some(1),
            "Start the long task",
        ))
        .await
        .expect("start turn");
    runtime.wait_until_streaming().await;

    let steer_command = CommandId::new();
    let mut steer = turn_request(
        "correlation-native-steer",
        steer_command,
        project_id,
        session_id,
        None,
        "Add this constraint without restarting",
    );
    steer.delivery_mode = TurnDeliveryMode::SteerNow;
    steer.expected_active_turn_id = Some(accepted.agent_turn_id);
    steer.runtime_controls = vec![RuntimeControlSelection {
        control_id: "model".to_owned(),
        choice_id: "next-model".to_owned(),
    }];
    assert!(matches!(
        application.send_turn(steer.clone()).await,
        Err(ConversationError::ScopeMismatch)
    ));
    steer.runtime_controls.clear();
    let steered = application
        .send_turn(steer.clone())
        .await
        .expect("steer active provider turn");
    assert_eq!(steered.agent_turn_id, accepted.agent_turn_id);
    assert_eq!(runtime.cancel_calls(), 0);
    assert_eq!(
        runtime.state.steers.lock().expect("steers lock").as_slice(),
        &[SteerRuntimeTurnRequest {
            session_id: session_id.0.to_string(),
            turn_id: accepted.agent_turn_id.0.to_string(),
            message_id: steer_command.0.to_string(),
            text: "Add this constraint without restarting".to_owned(),
        }]
    );
    let snapshot = application
        .restore(session_id)
        .await
        .expect("restore steer");
    assert_eq!(
        snapshot.session.active_turn_id,
        Some(accepted.agent_turn_id)
    );
    let steer_turn = snapshot
        .turns
        .iter()
        .find(|turn| turn.turn_id == steered.user_turn_id)
        .expect("durable steer user turn");
    assert_eq!(steer_turn.state, SessionTurnState::Completed);

    let replayed = application
        .send_turn(steer)
        .await
        .expect("replay accepted steer");
    assert_eq!(replayed.user_turn_id, steered.user_turn_id);
    assert_eq!(runtime.state.steers.lock().expect("steers lock").len(), 1);
    application
        .cancel_turn(Some(project_id), session_id, accepted.agent_turn_id)
        .await
        .expect("stop test turn");
}

#[tokio::test]
async fn native_steer_waits_for_the_accepted_turn_to_reach_the_runtime() {
    let runtime =
        Arc::new(ControlledRuntime::new(true).with_start_delay(Duration::from_millis(100)));
    let (application, project_id) = application(runtime.clone(), Duration::from_secs(2));
    let initialized = application
        .initialize(CommandId::new(), "Immediate steer".to_owned())
        .await
        .expect("initialize");
    let session_id = initialized.session.session_id;
    let accepted = application
        .send_turn(turn_request(
            "correlation-start-delayed",
            CommandId::new(),
            project_id,
            session_id,
            Some(1),
            "Start and register slowly",
        ))
        .await
        .expect("accept initial turn");

    let steer_command = CommandId::new();
    let mut steer = turn_request(
        "correlation-immediate-steer",
        steer_command,
        project_id,
        session_id,
        None,
        "Apply this immediately",
    );
    steer.delivery_mode = TurnDeliveryMode::SteerNow;
    steer.expected_active_turn_id = Some(accepted.agent_turn_id);
    let steered = application
        .send_turn(steer)
        .await
        .expect("wait for runtime registration and steer");

    assert_eq!(steered.agent_turn_id, accepted.agent_turn_id);
    assert_eq!(
        runtime.state.steers.lock().expect("steers lock").as_slice(),
        &[SteerRuntimeTurnRequest {
            session_id: session_id.0.to_string(),
            turn_id: accepted.agent_turn_id.0.to_string(),
            message_id: steer_command.0.to_string(),
            text: "Apply this immediately".to_owned(),
        }]
    );
    application
        .cancel_turn(Some(project_id), session_id, accepted.agent_turn_id)
        .await
        .expect("stop delayed test turn");
}

#[tokio::test]
async fn native_steer_timeout_becomes_a_visible_durable_failure() {
    let runtime = Arc::new(ControlledRuntime::new(true).with_steer_delay(Duration::from_secs(5)));
    let (application, project_id) = application(runtime.clone(), Duration::from_secs(5));
    let initialized = application
        .initialize(CommandId::new(), "Steer timeout".to_owned())
        .await
        .expect("initialize");
    let session_id = initialized.session.session_id;
    let accepted = application
        .send_turn(turn_request(
            "correlation-start-timeout",
            CommandId::new(),
            project_id,
            session_id,
            Some(1),
            "Keep working",
        ))
        .await
        .expect("start turn");
    runtime.wait_until_streaming().await;

    let steer_command = CommandId::new();
    let mut steer = turn_request(
        "correlation-steer-timeout",
        steer_command,
        project_id,
        session_id,
        None,
        "This delivery will time out",
    );
    steer.delivery_mode = TurnDeliveryMode::SteerNow;
    steer.expected_active_turn_id = Some(accepted.agent_turn_id);
    let error = application
        .send_turn(steer)
        .await
        .expect_err("timed-out steer must not look accepted");
    assert!(matches!(
        error,
        ConversationError::Runtime(RuntimeError {
            code: RuntimeErrorCode::ProviderUnavailable,
            ..
        })
    ));

    let snapshot = application
        .restore(session_id)
        .await
        .expect("restore timeout");
    let failed = snapshot
        .turns
        .iter()
        .find(|turn| turn.command_id == steer_command)
        .expect("durable steer turn");
    assert_eq!(failed.state, SessionTurnState::Failed);
    assert!(matches!(
        failed.outcome,
        Some(SessionTurnOutcome::Error(ref safe))
            if safe.code == RuntimeErrorCode::ProviderUnavailable.as_str()
    ));
    application
        .cancel_turn(Some(project_id), session_id, accepted.agent_turn_id)
        .await
        .expect("stop timeout test turn");
}

#[tokio::test]
async fn standalone_chat_runs_in_the_node_owned_scratch_workspace() {
    let runtime = Arc::new(ControlledRuntime::new(true));
    let (application, project_id) = application(runtime.clone(), Duration::from_secs(2));
    application
        .initialize(CommandId::new(), "Project chat".to_owned())
        .await
        .expect("initialize project");
    let standalone = application
        .create_session(CommandId::new(), None, "Standalone chat".to_owned())
        .await
        .expect("create standalone session");
    assert_eq!(standalone.snapshot.session.project_id, None);
    let session_id = standalone.snapshot.session.session_id;
    let mut request = turn_request(
        "correlation-standalone",
        CommandId::new(),
        project_id,
        session_id,
        Some(1),
        "Explore this idea",
    );
    request.project_id = None;
    let accepted = application
        .send_turn(request)
        .await
        .expect("send standalone turn");
    runtime.wait_until_streaming().await;
    assert_eq!(
        runtime
            .state
            .workspace_paths
            .lock()
            .expect("workspace paths lock")
            .as_slice(),
        &["C:\\test-scratch".to_owned()]
    );
    application
        .cancel_turn(None, session_id, accepted.agent_turn_id)
        .await
        .expect("stop standalone test turn");
}

async fn wait_for_terminal(
    application: &ConversationApplication,
    session_id: dennett_contracts::SessionId,
) -> ProjectSessionSnapshot {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let snapshot = application
                .restore(session_id)
                .await
                .expect("restore session");
            if snapshot
                .turns
                .last()
                .is_some_and(|turn| turn.state.is_terminal())
            {
                return snapshot;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("terminal state timeout")
}

#[tokio::test]
async fn complete_turn_streams_ordered_deltas_and_command_retry_does_not_rerun_provider() {
    let runtime = Arc::new(ScriptedFakeAgentRuntime::default());
    runtime
        .push_script([
            FakeRuntimeStep::ProgressWithNativeExtension {
                phase: "reasoning_summary".to_owned(),
                message: Some("Checked the request".to_owned()),
                extension: dennett_agent_core::NativeExtension {
                    namespace: "fixture.activity".to_owned(),
                    schema_version: "1".to_owned(),
                    payload: br#"{"status":"completed"}"#.to_vec(),
                },
            },
            FakeRuntimeStep::TextDelta("Hello ".to_owned()),
            FakeRuntimeStep::TextDelta("owner".to_owned()),
            FakeRuntimeStep::Complete,
        ])
        .expect("script runtime");
    let (application, project_id) = application(runtime, Duration::from_secs(1));
    let initialized = application
        .initialize(CommandId::new(), "Conversation".to_owned())
        .await
        .expect("initialize");
    let session_id = initialized.session.session_id;
    let mut watch = application.subscribe(session_id).await.expect("subscribe");
    assert!(matches!(
        watch.take_initial(),
        Some(WatchFrame::Snapshot { revision: 1, .. })
    ));

    let command_id = CommandId::new();
    let accepted = application
        .send_turn(turn_request(
            "correlation-complete",
            command_id,
            project_id,
            session_id,
            Some(1),
            "Say hello",
        ))
        .await
        .expect("accept turn");
    assert!(!accepted.replayed);

    let mut bodies = Vec::new();
    while bodies.len() < 5 {
        let frame = watch.recv().await.expect("watch").expect("watch frame");
        if let WatchFrame::Delta { delta, .. } = frame {
            bodies.push(delta.body);
        }
    }
    assert!(matches!(
        &bodies[0],
        SessionEventBody::TurnAccepted { command_id: accepted_command, .. }
            if *accepted_command == command_id
    ));
    assert!(matches!(
        &bodies[1],
        SessionEventBody::AgentActivityUpserted {
            phase,
            status: SessionActivityStatus::Completed,
            native_extensions,
            ..
        } if phase == "reasoning_summary"
            && native_extensions.first().is_some_and(|extension|
                extension.namespace == "fixture.activity"
                    && extension.json_value == r#"{"status":"completed"}"#)
    ));
    assert!(
        matches!(bodies[2], SessionEventBody::AgentTextAppended { ref text, .. } if text == "Hello ")
    );
    assert!(
        matches!(bodies[3], SessionEventBody::AgentTextAppended { ref text, .. } if text == "owner")
    );
    assert!(matches!(
        bodies[4],
        SessionEventBody::TurnFinished {
            state: SessionTurnState::Completed,
            ..
        }
    ));

    let final_snapshot = application
        .restore(session_id)
        .await
        .expect("final snapshot");
    assert_eq!(final_snapshot.session.revision, 6);
    assert_eq!(
        final_snapshot.turns.last().expect("agent turn").text,
        "Hello owner"
    );
    assert_eq!(
        final_snapshot.turns.last().expect("agent turn").command_id,
        command_id,
        "the terminal Result Envelope must remain joined to the submitted command"
    );
    assert_eq!(
        final_snapshot.turns.last().expect("agent turn").activities[0]
            .message
            .as_deref(),
        Some("Checked the request")
    );

    let retry = application
        .send_turn(turn_request(
            "correlation-retry",
            command_id,
            project_id,
            session_id,
            Some(1),
            "Say hello",
        ))
        .await
        .expect("idempotent retry");
    assert!(retry.replayed);
    assert_eq!(retry.agent_turn_id, accepted.agent_turn_id);
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(
        application
            .restore(session_id)
            .await
            .expect("retry snapshot")
            .session
            .revision,
        6
    );
}

#[tokio::test]
async fn project_scope_fails_closed_without_project_registry() {
    let coordinator = SessionCoordinator::new(
        SessionJournal::new(Arc::new(InMemorySessionEventStore::default())),
        7,
        16,
    );
    let project_id = ProjectId::new();
    let system = Arc::new(SystemProjection::new(SystemSnapshot::empty(7), 16));
    let application = ConversationApplication::new(
        coordinator,
        system.clone(),
        Arc::new(ScriptedFakeAgentRuntime::default()),
        LocalProject {
            project_id,
            display_name: "Unregistered project".to_owned(),
            workspace_path: "C:\\unregistered".to_owned(),
            standalone_workspace_path: "C:\\scratch".to_owned(),
        },
    );

    let active = application
        .initialize(CommandId::new(), "Standalone".to_owned())
        .await
        .expect("initialize standalone fallback");
    assert_eq!(active.session.project_id, None);
    assert!(
        system
            .bootstrap()
            .await
            .expect("system snapshot")
            .projects
            .is_empty()
    );

    let error = application
        .create_session(CommandId::new(), Some(project_id), "Denied".to_owned())
        .await
        .expect_err("unregistered project scope must fail closed");
    assert!(matches!(error, ConversationError::ScopeMismatch));
}

#[tokio::test]
async fn initialization_and_restore_are_isolated_to_one_project() {
    let coordinator = SessionCoordinator::new(
        SessionJournal::new(Arc::new(InMemorySessionEventStore::default())),
        7,
        16,
    );
    let owned_project = ProjectId::new();
    let other_project = ProjectId::new();
    let owned = coordinator
        .create_session(CommandId::new(), Some(owned_project), "Owned".to_owned(), 1)
        .await
        .expect("create owned session");
    let other = coordinator
        .create_session(CommandId::new(), Some(other_project), "Other".to_owned(), 2)
        .await
        .expect("create other session");
    let system = Arc::new(SystemProjection::new(SystemSnapshot::empty(7), 16));
    let application = ConversationApplication::new(
        coordinator,
        system.clone(),
        Arc::new(ScriptedFakeAgentRuntime::default()),
        LocalProject {
            project_id: owned_project,
            display_name: "Owned project".to_owned(),
            workspace_path: "C:\\owned".to_owned(),
            standalone_workspace_path: "C:\\scratch".to_owned(),
        },
    )
    .with_unregistered_project_fixture_for_tests();
    let active = application
        .initialize(CommandId::new(), "Unused".to_owned())
        .await
        .expect("initialize owned project");
    assert_eq!(active.session.session_id, owned.snapshot.session.session_id);
    let projected = system.bootstrap().await.expect("system snapshot");
    assert_eq!(projected.projects.len(), 1);
    assert_eq!(projected.recent_sessions.len(), 1);
    assert_eq!(
        projected.recent_sessions[0].session_id,
        owned.snapshot.session.session_id.0.to_string()
    );
    assert!(
        application
            .restore(other.snapshot.session.session_id)
            .await
            .is_err()
    );
    assert!(
        application
            .subscribe(other.snapshot.session.session_id)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn stop_is_scoped_and_terminal_retry_is_idempotent() {
    let runtime = Arc::new(ControlledRuntime::new(true));
    let (application, project_id) = application(runtime.clone(), Duration::from_secs(1));
    let initialized = application
        .initialize(CommandId::new(), "Cancelable".to_owned())
        .await
        .expect("initialize");
    let session_id = initialized.session.session_id;
    let accepted = application
        .send_turn(turn_request(
            "correlation-cancel",
            CommandId::new(),
            project_id,
            session_id,
            Some(1),
            "Begin",
        ))
        .await
        .expect("accept turn");
    runtime.wait_until_streaming().await;

    assert!(
        application
            .cancel_turn(
                Some(project_id),
                session_id,
                dennett_contracts::TurnId::new()
            )
            .await
            .is_err()
    );
    let first = application
        .cancel_turn(Some(project_id), session_id, accepted.agent_turn_id)
        .await
        .expect("cancel active turn");
    assert_eq!(first.disposition, CancelDisposition::Requested);
    let final_snapshot = wait_for_terminal(&application, session_id).await;
    assert_eq!(
        final_snapshot.turns.last().expect("agent turn").state,
        SessionTurnState::Cancelled
    );
    assert!(matches!(
        final_snapshot.turns.last().expect("agent turn").outcome,
        Some(SessionTurnOutcome::Result(ref result)) if result.partial && result.summary == "partial"
    ));

    let retry = application
        .cancel_turn(Some(project_id), session_id, accepted.agent_turn_id)
        .await
        .expect("terminal cancel retry");
    assert_eq!(
        retry.disposition,
        CancelDisposition::AlreadyTerminal(dennett_agent_core::RuntimeTerminalKind::Cancelled)
    );
    assert_eq!(runtime.cancel_calls(), 1);
}

#[tokio::test]
async fn provider_control_selections_reach_the_runtime_turn_unchanged() {
    let runtime = Arc::new(ControlledRuntime::new(true));
    let (application, project_id) = application(runtime.clone(), Duration::from_secs(1));
    let initialized = application
        .initialize(CommandId::new(), "Runtime controls".to_owned())
        .await
        .expect("initialize");
    let session_id = initialized.session.session_id;
    let mut request = turn_request(
        "correlation-runtime-controls",
        CommandId::new(),
        project_id,
        session_id,
        Some(1),
        "Use selected runtime settings",
    );
    request.runtime_controls = vec![
        RuntimeControlSelection {
            control_id: "model".to_owned(),
            choice_id: "gpt-new".to_owned(),
        },
        RuntimeControlSelection {
            control_id: "reasoning_effort".to_owned(),
            choice_id: "ultra".to_owned(),
        },
    ];
    let accepted = application.send_turn(request).await.expect("accept turn");
    runtime.wait_until_streaming().await;
    assert_eq!(
        *runtime
            .state
            .runtime_controls
            .lock()
            .expect("runtime controls lock"),
        vec![
            RuntimeControlSelection {
                control_id: "model".to_owned(),
                choice_id: "gpt-new".to_owned(),
            },
            RuntimeControlSelection {
                control_id: "reasoning_effort".to_owned(),
                choice_id: "ultra".to_owned(),
            },
        ]
    );
    application
        .cancel_turn(Some(project_id), session_id, accepted.agent_turn_id)
        .await
        .expect("cancel captured turn");
}

#[tokio::test]
async fn stop_interrupts_a_turn_while_the_provider_is_still_starting() {
    let runtime = Arc::new(HangingStartRuntime::default());
    let (application, project_id) = application(runtime.clone(), Duration::from_secs(1));
    let initialized = application
        .initialize(CommandId::new(), "Pending cancellation".to_owned())
        .await
        .expect("initialize");
    let session_id = initialized.session.session_id;
    let accepted = application
        .send_turn(turn_request(
            "correlation-pending-cancel",
            CommandId::new(),
            project_id,
            session_id,
            Some(1),
            "Start slowly",
        ))
        .await
        .expect("accept turn");
    runtime.wait_until_starting().await;

    let acknowledgement = application
        .cancel_turn(Some(project_id), session_id, accepted.agent_turn_id)
        .await
        .expect("cancel pending turn");
    assert_eq!(acknowledgement.disposition, CancelDisposition::Requested);
    let final_snapshot = wait_for_terminal(&application, session_id).await;
    assert_eq!(
        final_snapshot.turns.last().expect("agent turn").state,
        SessionTurnState::Cancelled
    );
    assert_eq!(runtime.cancel_calls.load(Ordering::Acquire), 1);
    assert!(runtime.cancelled.load(Ordering::Acquire));
}

#[tokio::test]
async fn concurrent_stop_retries_after_the_first_provider_cancellation_fails() {
    let runtime = Arc::new(FlakyCancelRuntime::new());
    let (application, project_id) = application(runtime.clone(), Duration::from_secs(1));
    let initialized = application
        .initialize(CommandId::new(), "Concurrent cancellation".to_owned())
        .await
        .expect("initialize");
    let session_id = initialized.session.session_id;
    let accepted = application
        .send_turn(turn_request(
            "correlation-concurrent-cancel",
            CommandId::new(),
            project_id,
            session_id,
            Some(1),
            "Begin",
        ))
        .await
        .expect("accept turn");
    runtime.inner.wait_until_streaming().await;

    let first_application = application.clone();
    let turn_id = accepted.agent_turn_id;
    let first = tokio::spawn(async move {
        first_application
            .cancel_turn(Some(project_id), session_id, turn_id)
            .await
    });
    runtime.wait_until_first_cancel().await;
    let second_application = application.clone();
    let second = tokio::spawn(async move {
        second_application
            .cancel_turn(Some(project_id), session_id, turn_id)
            .await
    });
    tokio::task::yield_now().await;
    assert!(
        !second.is_finished(),
        "the second caller must wait for an authoritative result"
    );

    runtime.release_first_cancel();
    assert!(first.await.expect("first cancellation task").is_err());
    let acknowledgement = second
        .await
        .expect("second cancellation task")
        .expect("second cancellation retries");
    assert_eq!(acknowledgement.disposition, CancelDisposition::Requested);
    assert_eq!(runtime.cancel_calls.load(Ordering::Acquire), 2);
    let final_snapshot = wait_for_terminal(&application, session_id).await;
    assert_eq!(
        final_snapshot.turns.last().expect("agent turn").state,
        SessionTurnState::Cancelled
    );
}

#[tokio::test]
async fn head_deadline_turns_a_hung_provider_into_visible_timeout() {
    let runtime = Arc::new(ControlledRuntime::new(true));
    let (application, project_id) = application(runtime.clone(), Duration::from_millis(80));
    let initialized = application
        .initialize(CommandId::new(), "Timeout".to_owned())
        .await
        .expect("initialize");
    let session_id = initialized.session.session_id;
    application
        .send_turn(turn_request(
            "correlation-timeout",
            CommandId::new(),
            project_id,
            session_id,
            Some(1),
            "Wait forever",
        ))
        .await
        .expect("accept turn");
    runtime.wait_until_streaming().await;

    let final_snapshot = wait_for_terminal(&application, session_id).await;
    let agent_turn = final_snapshot.turns.last().expect("agent turn");
    assert_eq!(agent_turn.state, SessionTurnState::TimedOut);
    assert!(matches!(
        agent_turn.outcome,
        Some(SessionTurnOutcome::Result(ref result)) if result.partial && result.summary == "partial"
    ));
    assert_eq!(runtime.cancel_calls(), 1);
    tokio::time::sleep(Duration::from_millis(30)).await;
    let after_late_provider_window = application
        .restore(session_id)
        .await
        .expect("restore timeout");
    assert_eq!(
        after_late_provider_window
            .turns
            .last()
            .expect("agent turn")
            .state,
        SessionTurnState::TimedOut,
        "a provider event after the deadline must not replace the authoritative timeout"
    );
}

#[tokio::test]
async fn head_deadline_reports_failure_when_the_provider_cannot_be_fenced() {
    let runtime = Arc::new(FailingCancelRuntime {
        inner: ControlledRuntime::new(true),
    });
    let (application, project_id) = application(runtime.clone(), Duration::from_millis(80));
    let initialized = application
        .initialize(CommandId::new(), "Unfenced timeout".to_owned())
        .await
        .expect("initialize");
    let session_id = initialized.session.session_id;
    application
        .send_turn(turn_request(
            "correlation-unfenced-timeout",
            CommandId::new(),
            project_id,
            session_id,
            Some(1),
            "Do not claim a safe timeout",
        ))
        .await
        .expect("accept turn");
    runtime.inner.wait_until_streaming().await;

    let final_snapshot = wait_for_terminal(&application, session_id).await;
    let agent_turn = final_snapshot.turns.last().expect("agent turn");
    assert_eq!(agent_turn.state, SessionTurnState::Failed);
    assert!(matches!(
        agent_turn.outcome,
        Some(SessionTurnOutcome::Error(ref safe))
            if safe.code == RuntimeErrorCode::ProviderUnavailable.as_str()
    ));
}

#[tokio::test]
async fn trace_joins_turn_scope_provider_and_terminal_memory_event_without_content() {
    let runtime = Arc::new(ScriptedFakeAgentRuntime::default());
    runtime
        .push_script([
            FakeRuntimeStep::TextDelta("private response".to_owned()),
            FakeRuntimeStep::Complete,
        ])
        .expect("script runtime");
    let (application, project_id) = application(runtime, Duration::from_secs(1));
    let initialized = application
        .initialize(CommandId::new(), "Trace".to_owned())
        .await
        .expect("initialize");
    let session_id = initialized.session.session_id;
    let command_id = CommandId::new();
    let captured = trace_capture();

    application
        .send_turn(turn_request(
            "correlation-trace",
            command_id,
            project_id,
            session_id,
            Some(1),
            "private prompt",
        ))
        .await
        .expect("accept traced turn");
    wait_for_terminal(&application, session_id).await;
    let fields = wait_for_trace_fields(&captured, "correlation-trace").await;

    assert_eq!(
        fields.get("dennett.installation.id").map(String::as_str),
        Some("installation-test")
    );
    assert_eq!(
        fields.get("dennett.device.id").map(String::as_str),
        Some("device-test")
    );
    assert_eq!(
        fields.get("dennett.project.id").map(String::as_str),
        Some(project_id.0.to_string().as_str())
    );
    assert_eq!(
        fields.get("dennett.session.id").map(String::as_str),
        Some(session_id.0.to_string().as_str())
    );
    assert_eq!(
        fields.get("dennett.command.id").map(String::as_str),
        Some(command_id.0.to_string().as_str())
    );
    let terminal_snapshot = application
        .restore(session_id)
        .await
        .expect("restore traced session");
    let runtime_turn_id = terminal_snapshot
        .turns
        .last()
        .expect("agent turn")
        .turn_id
        .0
        .to_string();
    assert_eq!(
        fields.get("dennett.runtime.turn.id").map(String::as_str),
        Some(runtime_turn_id.as_str())
    );
    assert_eq!(
        fields.get("dennett.provider.id").map(String::as_str),
        Some("dennett.fake"),
        "captured fields: {fields:?}"
    );
    assert_eq!(
        fields.get("correlation_id").map(String::as_str),
        Some("correlation-trace")
    );
    assert_eq!(
        fields.get("dennett.component").map(String::as_str),
        Some("dennett-head")
    );
    assert_eq!(
        fields.get("dennett.protocol.version").map(String::as_str),
        Some("1")
    );
    assert_eq!(
        fields
            .get("dennett.turn.terminal_state")
            .map(String::as_str),
        Some("completed")
    );
    assert!(
        fields
            .get("dennett.memory.event.id")
            .is_some_and(|value| !value.is_empty())
    );
    let diagnostic = format!("{fields:?}");
    assert!(!diagnostic.contains("private prompt"));
    assert!(!diagnostic.contains("private response"));
}

#[tokio::test]
async fn cancelled_turn_keeps_the_same_privacy_safe_trace_chain() {
    let runtime = Arc::new(ControlledRuntime::new(true));
    let (application, project_id) = application(runtime.clone(), Duration::from_secs(1));
    let initialized = application
        .initialize(CommandId::new(), "Cancelled trace".to_owned())
        .await
        .expect("initialize");
    let session_id = initialized.session.session_id;
    let command_id = CommandId::new();
    let captured = trace_capture();
    let accepted = application
        .send_turn(turn_request(
            "correlation-cancel-trace",
            command_id,
            project_id,
            session_id,
            Some(1),
            "private cancelled prompt",
        ))
        .await
        .expect("accept traced turn");
    runtime.wait_until_streaming().await;
    application
        .cancel_turn(Some(project_id), session_id, accepted.agent_turn_id)
        .await
        .expect("cancel traced turn");
    wait_for_terminal(&application, session_id).await;

    let fields = wait_for_trace_fields(&captured, "correlation-cancel-trace").await;
    let command_id = command_id.0.to_string();
    let runtime_turn_id = accepted.agent_turn_id.0.to_string();
    assert_eq!(
        fields.get("dennett.command.id").map(String::as_str),
        Some(command_id.as_str())
    );
    assert_eq!(
        fields.get("dennett.runtime.turn.id").map(String::as_str),
        Some(runtime_turn_id.as_str())
    );
    assert_eq!(
        fields.get("dennett.provider.id").map(String::as_str),
        Some("dennett.test.controlled")
    );
    assert_eq!(
        fields
            .get("dennett.turn.terminal_state")
            .map(String::as_str),
        Some("cancelled")
    );
    assert!(
        fields
            .get("dennett.memory.event.id")
            .is_some_and(|value| !value.is_empty())
    );
    assert!(!format!("{fields:?}").contains("private cancelled prompt"));
}

#[derive(Clone)]
struct ControlledRuntime {
    state: Arc<ControlledState>,
    start_delay: Duration,
    steer_delay: Duration,
}

#[derive(Clone)]
struct FlakyCancelRuntime {
    inner: ControlledRuntime,
    cancel_calls: Arc<AtomicUsize>,
    first_cancel_entered: Arc<Notify>,
    release_first: Arc<Notify>,
}

#[derive(Clone)]
struct FailingCancelRuntime {
    inner: ControlledRuntime,
}

#[async_trait]
impl AgentRuntimePort for FailingCancelRuntime {
    async fn respond(&self, request: AgentRequest) -> DennettResult<AgentResponse> {
        self.inner.respond(request).await
    }

    async fn describe(&self) -> Result<RuntimeDescriptor, RuntimeError> {
        self.inner.describe().await
    }

    async fn start_turn(&self, request: RuntimeTurnRequest) -> Result<RuntimeTurn, RuntimeError> {
        self.inner.start_turn(request).await
    }

    async fn cancel_turn(
        &self,
        _request: CancelRuntimeTurnRequest,
    ) -> Result<CancellationAcknowledgement, RuntimeError> {
        Err(RuntimeError::retryable(
            RuntimeErrorCode::ProviderUnavailable,
        ))
    }
}

impl FlakyCancelRuntime {
    fn new() -> Self {
        Self {
            inner: ControlledRuntime::new(true),
            cancel_calls: Arc::new(AtomicUsize::new(0)),
            first_cancel_entered: Arc::new(Notify::new()),
            release_first: Arc::new(Notify::new()),
        }
    }

    async fn wait_until_first_cancel(&self) {
        tokio::time::timeout(Duration::from_secs(1), self.first_cancel_entered.notified())
            .await
            .expect("first provider cancellation was not attempted");
    }

    fn release_first_cancel(&self) {
        self.release_first.notify_waiters();
    }
}

#[async_trait]
impl AgentRuntimePort for FlakyCancelRuntime {
    async fn respond(&self, request: AgentRequest) -> DennettResult<AgentResponse> {
        self.inner.respond(request).await
    }

    async fn describe(&self) -> Result<RuntimeDescriptor, RuntimeError> {
        self.inner.describe().await
    }

    async fn start_turn(&self, request: RuntimeTurnRequest) -> Result<RuntimeTurn, RuntimeError> {
        self.inner.start_turn(request).await
    }

    async fn cancel_turn(
        &self,
        request: CancelRuntimeTurnRequest,
    ) -> Result<CancellationAcknowledgement, RuntimeError> {
        let call = self.cancel_calls.fetch_add(1, Ordering::AcqRel);
        if call == 0 {
            let released = self.release_first.notified();
            self.first_cancel_entered.notify_waiters();
            released.await;
            return Err(RuntimeError::retryable(
                RuntimeErrorCode::ProviderUnavailable,
            ));
        }
        self.inner.cancel_turn(request).await
    }
}

#[derive(Clone, Default)]
struct HangingStartRuntime {
    starting: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
    changed: Arc<Notify>,
    cancel_calls: Arc<AtomicUsize>,
}

impl HangingStartRuntime {
    async fn wait_until_starting(&self) {
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let changed = self.changed.notified();
                if self.starting.load(Ordering::Acquire) {
                    return;
                }
                changed.await;
            }
        })
        .await
        .expect("runtime did not enter start_turn");
    }
}

#[async_trait]
impl AgentRuntimePort for HangingStartRuntime {
    async fn respond(&self, request: AgentRequest) -> DennettResult<AgentResponse> {
        Ok(AgentResponse {
            text: request.prompt,
            evidence_handles: request.context_handles,
        })
    }

    async fn describe(&self) -> Result<RuntimeDescriptor, dennett_agent_core::RuntimeError> {
        Ok(RuntimeDescriptor {
            adapter_id: "dennett.test.hanging-start".to_owned(),
            runtime_kind: RuntimeKind::GenericLoop,
            capabilities: RuntimeCapabilities {
                streaming: true,
                continuation: false,
                scoped_cancellation: true,
                deadlines: true,
                steering: RuntimeSteeringMode::Unsupported,
                native_extension_schemas: Vec::new(),
            },
            controls: Vec::new(),
        })
    }

    async fn start_turn(
        &self,
        _request: RuntimeTurnRequest,
    ) -> Result<RuntimeTurn, dennett_agent_core::RuntimeError> {
        self.starting.store(true, Ordering::Release);
        self.changed.notify_waiters();
        std::future::pending().await
    }

    async fn cancel_turn(
        &self,
        request: CancelRuntimeTurnRequest,
    ) -> Result<CancellationAcknowledgement, dennett_agent_core::RuntimeError> {
        self.cancel_calls.fetch_add(1, Ordering::AcqRel);
        self.cancelled.store(true, Ordering::Release);
        Ok(CancellationAcknowledgement {
            session_id: request.session_id,
            turn_id: request.turn_id,
            disposition: CancelDisposition::Requested,
        })
    }
}

struct ControlledState {
    emit_partial: bool,
    cancel_requested: AtomicBool,
    terminal: AtomicBool,
    cancel_calls: AtomicUsize,
    streaming: AtomicBool,
    changed: Notify,
    turn: Mutex<Option<(String, String)>>,
    runtime_controls: Mutex<Vec<RuntimeControlSelection>>,
    steers: Mutex<Vec<SteerRuntimeTurnRequest>>,
    workspace_paths: Mutex<Vec<String>>,
}

impl ControlledRuntime {
    fn new(emit_partial: bool) -> Self {
        Self {
            state: Arc::new(ControlledState {
                emit_partial,
                cancel_requested: AtomicBool::new(false),
                terminal: AtomicBool::new(false),
                cancel_calls: AtomicUsize::new(0),
                streaming: AtomicBool::new(false),
                changed: Notify::new(),
                turn: Mutex::new(None),
                runtime_controls: Mutex::new(Vec::new()),
                steers: Mutex::new(Vec::new()),
                workspace_paths: Mutex::new(Vec::new()),
            }),
            start_delay: Duration::ZERO,
            steer_delay: Duration::ZERO,
        }
    }

    fn with_start_delay(mut self, delay: Duration) -> Self {
        self.start_delay = delay;
        self
    }

    fn with_steer_delay(mut self, delay: Duration) -> Self {
        self.steer_delay = delay;
        self
    }

    async fn wait_until_streaming(&self) {
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let changed = self.state.changed.notified();
                if self.state.streaming.load(Ordering::Acquire) {
                    return;
                }
                changed.await;
            }
        })
        .await
        .expect("runtime did not start streaming");
    }

    fn cancel_calls(&self) -> usize {
        self.state.cancel_calls.load(Ordering::Acquire)
    }
}

#[async_trait]
impl AgentRuntimePort for ControlledRuntime {
    async fn respond(&self, request: AgentRequest) -> DennettResult<AgentResponse> {
        Ok(AgentResponse {
            text: request.prompt,
            evidence_handles: request.context_handles,
        })
    }

    async fn describe(&self) -> Result<RuntimeDescriptor, dennett_agent_core::RuntimeError> {
        Ok(RuntimeDescriptor {
            adapter_id: "dennett.test.controlled".to_owned(),
            runtime_kind: RuntimeKind::GenericLoop,
            capabilities: RuntimeCapabilities {
                streaming: true,
                continuation: false,
                scoped_cancellation: true,
                deadlines: true,
                steering: RuntimeSteeringMode::Native,
                native_extension_schemas: Vec::new(),
            },
            controls: Vec::new(),
        })
    }

    async fn start_turn(
        &self,
        request: RuntimeTurnRequest,
    ) -> Result<RuntimeTurn, dennett_agent_core::RuntimeError> {
        request.validate()?;
        tokio::time::sleep(self.start_delay).await;
        *self
            .state
            .runtime_controls
            .lock()
            .expect("runtime controls lock") = request.runtime_controls.clone();
        self.state
            .workspace_paths
            .lock()
            .expect("workspace paths lock")
            .push(request.workspace_path.clone());
        *self.state.turn.lock().expect("turn lock") =
            Some((request.session_id.clone(), request.turn_id.clone()));
        Ok(RuntimeTurn::from_stream(
            request.session_id,
            request.turn_id,
            Box::new(ControlledStream {
                state: Arc::clone(&self.state),
                sequence: 1,
                stage: 0,
            }),
        ))
    }

    async fn cancel_turn(
        &self,
        request: CancelRuntimeTurnRequest,
    ) -> Result<CancellationAcknowledgement, dennett_agent_core::RuntimeError> {
        request.validate()?;
        self.state.cancel_calls.fetch_add(1, Ordering::AcqRel);
        let matches = self
            .state
            .turn
            .lock()
            .expect("turn lock")
            .as_ref()
            .is_some_and(|turn| turn.0 == request.session_id && turn.1 == request.turn_id);
        let disposition = if !matches {
            CancelDisposition::NotFound
        } else if self.state.terminal.load(Ordering::Acquire) {
            CancelDisposition::AlreadyTerminal(dennett_agent_core::RuntimeTerminalKind::Cancelled)
        } else if self.state.cancel_requested.swap(true, Ordering::AcqRel) {
            CancelDisposition::AlreadyRequested
        } else {
            CancelDisposition::Requested
        };
        self.state.changed.notify_waiters();
        Ok(CancellationAcknowledgement {
            session_id: request.session_id,
            turn_id: request.turn_id,
            disposition,
        })
    }

    async fn steer_turn(
        &self,
        request: SteerRuntimeTurnRequest,
    ) -> Result<SteeringAcknowledgement, dennett_agent_core::RuntimeError> {
        request.validate()?;
        tokio::time::sleep(self.steer_delay).await;
        let matches = self
            .state
            .turn
            .lock()
            .expect("turn lock")
            .as_ref()
            .is_some_and(|turn| turn.0 == request.session_id && turn.1 == request.turn_id);
        if !matches || self.state.terminal.load(Ordering::Acquire) {
            return Err(RuntimeError::recoverable(RuntimeErrorCode::ScopeMismatch));
        }
        self.state
            .steers
            .lock()
            .expect("steers lock")
            .push(request.clone());
        Ok(SteeringAcknowledgement {
            session_id: request.session_id,
            turn_id: request.turn_id,
            message_id: request.message_id,
        })
    }
}

struct ControlledStream {
    state: Arc<ControlledState>,
    sequence: u64,
    stage: u8,
}

#[async_trait]
impl RuntimeEventStream for ControlledStream {
    async fn next_event(
        &mut self,
    ) -> Option<Result<RuntimeEvent, dennett_agent_core::RuntimeError>> {
        let (session_id, turn_id) = self.state.turn.lock().expect("turn lock").clone()?;
        let kind = match self.stage {
            0 => RuntimeEventKind::Started { continuation: None },
            1 if self.state.emit_partial => {
                self.state.streaming.store(true, Ordering::Release);
                self.state.changed.notify_waiters();
                RuntimeEventKind::TextDelta {
                    text: "partial".to_owned(),
                }
            }
            1 | 2 => {
                loop {
                    let changed = self.state.changed.notified();
                    if self.state.cancel_requested.load(Ordering::Acquire) {
                        break;
                    }
                    changed.await;
                }
                self.state.terminal.store(true, Ordering::Release);
                RuntimeEventKind::Terminal(RuntimeTerminal {
                    outcome: RuntimeTerminalOutcome::Cancelled {
                        partial: self.state.emit_partial,
                    },
                    continuation: None,
                })
            }
            _ => return None,
        };
        let event = RuntimeEvent {
            session_id,
            turn_id,
            sequence: self.sequence,
            kind,
            native_extensions: Vec::new(),
        };
        self.sequence += 1;
        self.stage += 1;
        Some(Ok(event))
    }
}

type CapturedSpans = Arc<Mutex<Vec<HashMap<String, String>>>>;

static TRACE_CAPTURE: OnceLock<CapturedSpans> = OnceLock::new();

fn trace_capture() -> CapturedSpans {
    Arc::clone(TRACE_CAPTURE.get_or_init(|| {
        let captured = Arc::new(Mutex::new(Vec::new()));
        tracing::subscriber::set_global_default(tracing_subscriber::registry().with(
            CaptureLayer {
                captured: Arc::clone(&captured),
            },
        ))
        .expect("install conversation test tracing subscriber");
        captured
    }))
}

async fn wait_for_trace_fields(
    captured: &CapturedSpans,
    correlation_id: &str,
) -> HashMap<String, String> {
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if let Some(fields) = captured
                .lock()
                .expect("capture lock")
                .iter()
                .rev()
                .find(|fields| {
                    fields.get("correlation_id").map(String::as_str) == Some(correlation_id)
                        && fields.contains_key("dennett.memory.event.id")
                        && fields.contains_key("dennett.turn.terminal_state")
                })
                .cloned()
            {
                return fields;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "terminal trace fields for {correlation_id}: {:?}",
            captured.lock().expect("capture lock").last()
        )
    })
}

#[derive(Clone)]
struct CaptureLayer {
    captured: CapturedSpans,
}

#[derive(Default)]
struct CapturedFields(HashMap<String, String>);

impl Visit for CapturedFields {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_owned(), value.to_owned());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name().to_owned(), value.to_string());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.insert(field.name().to_owned(), format!("{value:?}"));
    }
}

impl<S> Layer<S> for CaptureLayer
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_new_span(
        &self,
        attributes: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        context: Context<'_, S>,
    ) {
        if attributes.metadata().name() != "project_chat_turn" {
            return;
        }
        let mut fields = CapturedFields::default();
        attributes.record(&mut fields);
        self.captured
            .lock()
            .expect("capture lock")
            .push(fields.0.clone());
        if let Some(span) = context.span(id) {
            span.extensions_mut().insert(fields);
        }
    }

    fn on_record(
        &self,
        id: &tracing::span::Id,
        values: &tracing::span::Record<'_>,
        context: Context<'_, S>,
    ) {
        if let Some(span) = context.span(id)
            && let Some(fields) = span.extensions_mut().get_mut::<CapturedFields>()
        {
            values.record(fields);
            self.captured
                .lock()
                .expect("capture lock")
                .push(fields.0.clone());
        }
    }
}
