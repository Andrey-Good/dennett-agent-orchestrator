use async_trait::async_trait;
use dennett_agent_core::{
    AgentRuntimePort, CancelDisposition, CancelRuntimeTurnRequest, FakeRuntimeStep,
    OpaqueContinuation, RuntimeDeadline, RuntimeError, RuntimeErrorCode, RuntimeEvent,
    RuntimeEventKind, RuntimeEventStream, RuntimeTerminalOutcome, RuntimeTurn, RuntimeTurnRequest,
    RuntimeUsage, ScriptedFakeAgentRuntime,
};
use serde::Deserialize;
use std::{collections::VecDeque, time::Duration};

#[derive(Deserialize)]
struct ConformanceDocument {
    version: u32,
    cases: Vec<ConformanceCase>,
}

#[derive(Deserialize)]
struct ConformanceCase {
    id: String,
    #[serde(default)]
    expected_events: Vec<String>,
    expected_error: Option<String>,
    expected_terminal_code: Option<String>,
    expected_retryable: Option<bool>,
    expected_recoverable: Option<bool>,
}

fn conformance() -> ConformanceDocument {
    serde_json::from_str(include_str!(
        "../../../tests/contracts/agent_runtime_conformance.json"
    ))
    .expect("shared runtime conformance fixture must be valid JSON")
}

fn case(id: &str) -> ConformanceCase {
    let document = conformance();
    assert_eq!(document.version, 1);
    document
        .cases
        .into_iter()
        .find(|candidate| candidate.id == id)
        .unwrap_or_else(|| panic!("missing conformance case {id}"))
}

fn request(session_id: &str, turn_id: &str) -> RuntimeTurnRequest {
    RuntimeTurnRequest {
        session_id: session_id.to_owned(),
        turn_id: turn_id.to_owned(),
        prompt: "private test prompt".to_owned(),
        workspace_path: "C:/synthetic/project".to_owned(),
        context_handles: Vec::new(),
        continuation: None,
        deadline: RuntimeDeadline::after(Duration::from_secs(5))
            .expect("test deadline should be valid"),
    }
}

async fn collect(mut turn: RuntimeTurn) -> Result<Vec<RuntimeEvent>, RuntimeError> {
    let mut events = Vec::new();
    while let Some(event) = turn.next_event().await {
        events.push(event?);
    }
    Ok(events)
}

fn event_labels(events: &[RuntimeEvent]) -> Vec<String> {
    events
        .iter()
        .map(|event| match &event.kind {
            RuntimeEventKind::Started { .. } => "started",
            RuntimeEventKind::TextDelta { .. } => "text_delta",
            RuntimeEventKind::Progress { .. } => "progress",
            RuntimeEventKind::Usage(_) => "usage",
            RuntimeEventKind::Warning { .. } => "warning",
            RuntimeEventKind::Terminal(terminal) => match terminal.outcome {
                RuntimeTerminalOutcome::Completed => "completed",
                RuntimeTerminalOutcome::Cancelled { .. } => "cancelled",
                RuntimeTerminalOutcome::TimedOut { .. } => "timed_out",
                RuntimeTerminalOutcome::Failed { .. } => "failed",
            },
        })
        .map(str::to_owned)
        .collect()
}

#[tokio::test]
async fn test_agent_runtime_stream_001_orders_provider_neutral_events() {
    let contract = case("ordered_stream");
    let runtime = ScriptedFakeAgentRuntime::default();
    runtime
        .push_script([
            FakeRuntimeStep::TextDelta("hello".to_owned()),
            FakeRuntimeStep::Progress {
                phase: "working".to_owned(),
                message: None,
            },
            FakeRuntimeStep::Usage(RuntimeUsage {
                input_tokens: 3,
                cached_input_tokens: 0,
                output_tokens: 2,
                reasoning_output_tokens: 0,
            }),
            FakeRuntimeStep::Complete,
        ])
        .expect("test script should be accepted");

    let events = collect(
        runtime
            .start_turn(request("session-a", "turn-a"))
            .await
            .expect("scripted fake should start"),
    )
    .await
    .expect("ordered stream should validate");

    assert_eq!(event_labels(&events), contract.expected_events);
    assert_eq!(
        events
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5]
    );
    assert!(
        events
            .iter()
            .all(|event| { event.session_id == "session-a" && event.turn_id == "turn-a" })
    );
}

#[tokio::test]
async fn test_agent_runtime_cancel_001_is_scoped_idempotent_and_terminal() {
    let contract = case("scoped_cancellation");
    let runtime = ScriptedFakeAgentRuntime::default();
    runtime
        .push_script([
            FakeRuntimeStep::TextDelta("late-a".to_owned()),
            FakeRuntimeStep::Complete,
        ])
        .expect("first test script should be accepted");
    runtime
        .push_script([
            FakeRuntimeStep::TextDelta("kept-b".to_owned()),
            FakeRuntimeStep::Complete,
        ])
        .expect("second test script should be accepted");
    let mut turn_a = runtime
        .start_turn(request("session-a", "turn-a"))
        .await
        .expect("first turn should start");
    let turn_b = runtime
        .start_turn(request("session-b", "turn-b"))
        .await
        .expect("second turn should start");

    let started_a = turn_a
        .next_event()
        .await
        .expect("start event")
        .expect("valid start event");
    assert!(matches!(started_a.kind, RuntimeEventKind::Started { .. }));
    let wrong_scope = runtime
        .cancel_turn(CancelRuntimeTurnRequest {
            session_id: "session-b".to_owned(),
            turn_id: "turn-a".to_owned(),
        })
        .await
        .expect("unknown scope is an acknowledged no-op");
    assert_eq!(wrong_scope.disposition, CancelDisposition::NotFound);

    let first = runtime
        .cancel_turn(CancelRuntimeTurnRequest {
            session_id: "session-a".to_owned(),
            turn_id: "turn-a".to_owned(),
        })
        .await
        .expect("cancel should be acknowledged");
    let second = runtime
        .cancel_turn(CancelRuntimeTurnRequest {
            session_id: "session-a".to_owned(),
            turn_id: "turn-a".to_owned(),
        })
        .await
        .expect("repeat cancel should be acknowledged");
    assert_eq!(first.disposition, CancelDisposition::Requested);
    assert_eq!(second.disposition, CancelDisposition::AlreadyRequested);

    let cancelled = turn_a
        .next_event()
        .await
        .expect("terminal event")
        .expect("valid terminal event");
    assert_eq!(
        event_labels(&[started_a, cancelled]),
        contract.expected_events
    );
    assert!(turn_a.next_event().await.is_none());
    let after_terminal = runtime
        .cancel_turn(CancelRuntimeTurnRequest {
            session_id: "session-a".to_owned(),
            turn_id: "turn-a".to_owned(),
        })
        .await
        .expect("post-terminal cancel should be acknowledged");
    assert_eq!(
        after_terminal.disposition,
        CancelDisposition::AlreadyTerminal(dennett_agent_core::RuntimeTerminalKind::Cancelled)
    );

    let events_b = collect(turn_b)
        .await
        .expect("cancelling turn A must not affect turn B");
    assert_eq!(
        event_labels(&events_b),
        ["started", "text_delta", "completed"]
    );
}

#[tokio::test]
async fn test_agent_runtime_timeout_001_preserves_partial_output_and_rejects_late_completion() {
    let contract = case("partial_timeout");
    let runtime = ScriptedFakeAgentRuntime::default();
    runtime
        .push_script([
            FakeRuntimeStep::TextDelta("partial".to_owned()),
            FakeRuntimeStep::Advance(Duration::from_secs(6)),
            FakeRuntimeStep::Complete,
        ])
        .expect("timeout script should be accepted");

    let events = collect(
        runtime
            .start_turn(request("session-timeout", "turn-timeout"))
            .await
            .expect("timeout turn should start"),
    )
    .await
    .expect("timeout is a terminal event, not a stream transport error");

    assert_eq!(event_labels(&events), contract.expected_events);
    assert!(matches!(
        &events.last().expect("terminal event").kind,
        RuntimeEventKind::Terminal(terminal)
            if terminal.outcome == RuntimeTerminalOutcome::TimedOut { partial: true }
    ));
}

#[tokio::test]
async fn test_codex_sdk_continuation_001_keeps_continuation_opaque_and_resumable() {
    let contract = case("opaque_continuation");
    let runtime = ScriptedFakeAgentRuntime::default();
    runtime
        .push_script([
            FakeRuntimeStep::TextDelta("continued".to_owned()),
            FakeRuntimeStep::Usage(RuntimeUsage {
                input_tokens: 1,
                cached_input_tokens: 0,
                output_tokens: 1,
                reasoning_output_tokens: 0,
            }),
            FakeRuntimeStep::Complete,
        ])
        .expect("continuation script should be accepted");
    let continuation = OpaqueContinuation::new("dennett.fake", "private-provider-handle")
        .expect("test continuation should be valid");
    let mut turn_request = request("session-continuation", "turn-continuation");
    turn_request.continuation = Some(continuation.clone());

    let events = collect(
        runtime
            .start_turn(turn_request)
            .await
            .expect("matching adapter should resume"),
    )
    .await
    .expect("continuation stream should validate");
    assert_eq!(event_labels(&events), contract.expected_events);
    assert!(!format!("{continuation:?}").contains("private-provider-handle"));
    assert!(matches!(
        &events[0].kind,
        RuntimeEventKind::Started { continuation: Some(observed) } if observed == &continuation
    ));

    let wrong_adapter = OpaqueContinuation::new("other.adapter", "private-provider-handle")
        .expect("test continuation should be valid");
    let mut rejected_request = request("session-continuation", "turn-rejected");
    rejected_request.continuation = Some(wrong_adapter);
    let error = match runtime.start_turn(rejected_request).await {
        Ok(_) => panic!("foreign continuation must not be accepted"),
        Err(error) => error,
    };
    assert_eq!(error.code, RuntimeErrorCode::ContinuationUnavailable);
    assert!(error.recoverable);
}

#[tokio::test]
async fn test_agent_runtime_stream_001_distinguishes_recoverable_provider_failure() {
    let contract = case("provider_failure");
    let runtime = ScriptedFakeAgentRuntime::default();
    runtime
        .push_script([FakeRuntimeStep::Fail {
            code: "rate_limit".to_owned(),
            retryable: true,
            recoverable: true,
        }])
        .expect("provider failure script should be accepted");

    let events = collect(
        runtime
            .start_turn(request("session-failure", "turn-failure"))
            .await
            .expect("failure turn should start"),
    )
    .await
    .expect("provider failure is a terminal outcome");

    assert_eq!(event_labels(&events), contract.expected_events);
    let RuntimeEventKind::Terminal(terminal) = &events.last().expect("terminal event").kind else {
        panic!("last event must be terminal");
    };
    let RuntimeTerminalOutcome::Failed {
        code,
        retryable,
        recoverable,
        partial,
    } = &terminal.outcome
    else {
        panic!("terminal event must be a provider failure");
    };
    assert_eq!(
        code,
        &contract.expected_terminal_code.expect("failure code")
    );
    assert_eq!(
        *retryable,
        contract.expected_retryable.expect("retryability")
    );
    assert_eq!(
        *recoverable,
        contract.expected_recoverable.expect("recoverability")
    );
    assert!(!partial);
}

struct RawEventStream {
    events: VecDeque<RuntimeEvent>,
}

#[async_trait]
impl RuntimeEventStream for RawEventStream {
    async fn next_event(&mut self) -> Option<Result<RuntimeEvent, RuntimeError>> {
        self.events.pop_front().map(Ok)
    }
}

#[tokio::test]
async fn test_agent_runtime_stream_001_rejects_late_or_malformed_events() {
    let contract = case("malformed_late_event");
    let raw = RawEventStream {
        events: VecDeque::from([
            RuntimeEvent {
                session_id: "session".to_owned(),
                turn_id: "turn".to_owned(),
                sequence: 1,
                kind: RuntimeEventKind::Started { continuation: None },
                native_extensions: Vec::new(),
            },
            RuntimeEvent {
                session_id: "session".to_owned(),
                turn_id: "turn".to_owned(),
                sequence: 2,
                kind: RuntimeEventKind::Terminal(dennett_agent_core::RuntimeTerminal {
                    outcome: RuntimeTerminalOutcome::Completed,
                    continuation: None,
                }),
                native_extensions: Vec::new(),
            },
            RuntimeEvent {
                session_id: "session".to_owned(),
                turn_id: "turn".to_owned(),
                sequence: 3,
                kind: RuntimeEventKind::TextDelta {
                    text: "late".to_owned(),
                },
                native_extensions: Vec::new(),
            },
        ]),
    };
    let mut turn = RuntimeTurn::from_stream("session", "turn", Box::new(raw));
    assert!(turn.next_event().await.expect("start event").is_ok());
    assert!(turn.next_event().await.expect("terminal event").is_ok());
    let error = turn
        .next_event()
        .await
        .expect("late event must yield an error")
        .expect_err("late event must fail closed");
    assert_eq!(
        error.code.as_str(),
        contract.expected_error.as_deref().unwrap()
    );
    assert!(turn.next_event().await.is_none());
}
