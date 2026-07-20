use dennett_agent_core::{AgentRequest, AgentRuntimePort};
use dennett_contracts::{MemoryEventId, ProjectChatCommand, ResultEnvelope};
use dennett_kernel::{DennettResult, ProjectChatUseCase};
use dennett_memory_core::{MemoryEvent, MemoryPort};
use std::sync::Arc;

pub mod conversation;
pub mod draft;
pub mod session;
pub mod system;

pub struct HeadApplication<A: AgentRuntimePort, M: MemoryPort> {
    agent: Arc<A>,
    memory: Arc<M>,
}

impl<A: AgentRuntimePort, M: MemoryPort> HeadApplication<A, M> {
    pub fn new(agent: Arc<A>, memory: Arc<M>) -> Self {
        Self { agent, memory }
    }
}

#[async_trait::async_trait]
impl<A: AgentRuntimePort, M: MemoryPort> ProjectChatUseCase for HeadApplication<A, M> {
    async fn execute(&self, command: ProjectChatCommand) -> DennettResult<ResultEnvelope> {
        let response = self
            .agent
            .respond(AgentRequest {
                prompt: command.text.clone(),
                context_handles: Vec::new(),
            })
            .await?;

        self.memory
            .append(MemoryEvent {
                event_id: MemoryEventId::new(),
                project_id: command.project_id,
                session_id: command.session_id,
                kind: "project_chat_completed".to_owned(),
                summary: response.text.clone(),
            })
            .await?;

        Ok(ResultEnvelope {
            command_id: command.command_id,
            summary: response.text,
            partial: false,
            artifact_handles: Vec::new(),
            evidence_handles: response.evidence_handles,
        })
    }
}
