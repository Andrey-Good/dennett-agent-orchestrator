//! Provider-neutral agent runtime port and a deterministic fake.

use async_trait::async_trait;
use dennett_kernel::DennettResult;

#[derive(Clone, Debug)]
pub struct AgentRequest {
    pub prompt: String,
    pub context_handles: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct AgentResponse {
    pub text: String,
    pub evidence_handles: Vec<String>,
}

#[async_trait]
pub trait AgentRuntimePort: Send + Sync {
    async fn respond(&self, request: AgentRequest) -> DennettResult<AgentResponse>;
    async fn cancel(&self) -> DennettResult<()> {
        Ok(())
    }
}

#[derive(Default)]
pub struct FakeAgentRuntime;

#[async_trait]
impl AgentRuntimePort for FakeAgentRuntime {
    async fn respond(&self, request: AgentRequest) -> DennettResult<AgentResponse> {
        Ok(AgentResponse {
            text: format!("Dennett skeleton received: {}", request.prompt),
            evidence_handles: request.context_handles,
        })
    }
}
