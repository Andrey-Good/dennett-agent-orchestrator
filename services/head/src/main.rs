use dennett_agent_core::FakeAgentRuntime;
use dennett_contracts::{CommandId, ProjectChatCommand, ProjectId, SessionId};
use dennett_head::HeadApplication;
use dennett_kernel::ProjectChatUseCase;
use dennett_memory_core::InMemoryMemory;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dennett_observability::init("dennett-head");
    let app = HeadApplication::new(
        Arc::new(FakeAgentRuntime),
        Arc::new(InMemoryMemory::default()),
    );
    let result = app
        .execute(ProjectChatCommand {
            command_id: CommandId::new(),
            project_id: ProjectId::new(),
            session_id: SessionId::new(),
            text: "hello from the executable skeleton".to_owned(),
        })
        .await?;
    println!("{}", result.summary);
    Ok(())
}
