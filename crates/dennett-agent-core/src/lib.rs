//! Provider-neutral agent runtime contracts and deterministic fakes.

mod fake;
mod runtime;

pub use fake::{FakeAgentRuntime, FakeRuntimeStep, ScriptedFakeAgentRuntime};
pub use runtime::{
    AgentRequest, AgentResponse, AgentRuntimePort, CancelDisposition, CancelRuntimeTurnRequest,
    CancellationAcknowledgement, NativeExtension, OpaqueContinuation, RuntimeCapabilities,
    RuntimeDeadline, RuntimeDescriptor, RuntimeError, RuntimeErrorCode, RuntimeEvent,
    RuntimeEventKind, RuntimeEventStream, RuntimeEventValidator, RuntimeKind, RuntimeTerminal,
    RuntimeTerminalKind, RuntimeTerminalOutcome, RuntimeTurn, RuntimeTurnRequest, RuntimeUsage,
};
