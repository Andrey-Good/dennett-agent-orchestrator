use dennett_agent_core::FakeAgentRuntime;
use dennett_contracts::{CommandId, ProjectChatCommand, ProjectId, SessionId};
use dennett_head::HeadApplication;
use dennett_kernel::ProjectChatUseCase;
use dennett_memory_core::{InMemoryMemory, MemoryPort};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dennett_observability::init("dennett-head");
    let memory = Arc::new(InMemoryMemory::default());
    let app = HeadApplication::new(Arc::new(FakeAgentRuntime), Arc::clone(&memory));
    let command_id = CommandId::new();
    let project_id = ProjectId::new();
    let session_id = SessionId::new();
    let result = app
        .execute(ProjectChatCommand {
            command_id,
            project_id,
            session_id,
            text: "hello from the credential-free demo".to_owned(),
        })
        .await?;
    let event = memory
        .recent_for_project(project_id, 1)
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| std::io::Error::other("fake conversation did not commit a memory event"))?;

    if result.command_id != command_id
        || event.project_id != project_id
        || event.session_id != session_id
        || event.summary != result.summary
    {
        return Err(std::io::Error::other("fake conversation correlation check failed").into());
    }

    println!(
        "fake_chat command_id={} result_command_id={} memory_event_id={} project_id={} session_id={}",
        command_id.0, result.command_id.0, event.event_id.0, project_id.0, session_id.0
    );
    println!("summary={}", result.summary);
    Ok(())
}
