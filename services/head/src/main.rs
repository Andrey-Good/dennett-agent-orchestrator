
use denet_agent_core::FakeAgentRuntime;
use denet_contracts::{CommandId, ProjectChatCommand, ProjectId, SessionId};
use denet_head::HeadApplication;
use denet_kernel::ProjectChatUseCase;
use denet_memory_core::InMemoryMemory;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    denet_observability::init("denet-head");
    let app = HeadApplication::new(Arc::new(FakeAgentRuntime), Arc::new(InMemoryMemory::default()));
    let result = app.execute(ProjectChatCommand {
        command_id: CommandId::new(),
        project_id: ProjectId::new(),
        session_id: SessionId::new(),
        text: "hello from the executable skeleton".to_owned(),
    }).await?;
    println!("{}", result.summary);
    Ok(())
}
