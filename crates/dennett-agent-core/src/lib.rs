//! Provider-neutral agent runtime contracts and deterministic fakes.

mod fake;
mod runtime;

pub use fake::{FakeAgentRuntime, FakeRuntimeStep, ScriptedFakeAgentRuntime};
pub use runtime::{
    AgentRequest, AgentResponse, AgentRuntimePort, CancelDisposition, CancelRuntimeTurnRequest,
    CancellationAcknowledgement, InMemoryRuntimeContinuationStore, NativeExtension,
    OpaqueContinuation, RuntimeActivityStatus, RuntimeCapabilities, RuntimeContinuationError,
    RuntimeContinuationPort, RuntimeControlChoice, RuntimeControlCondition,
    RuntimeControlDescriptor, RuntimeControlSelection, RuntimeDeadline, RuntimeDescriptor,
    RuntimeError, RuntimeErrorCode, RuntimeEvent, RuntimeEventKind, RuntimeEventStream,
    RuntimeEventValidator, RuntimeKind, RuntimeSteeringMode, RuntimeTerminal, RuntimeTerminalKind,
    RuntimeTerminalOutcome, RuntimeTurn, RuntimeTurnRequest, RuntimeUsage, SteerRuntimeTurnRequest,
    SteeringAcknowledgement,
};
