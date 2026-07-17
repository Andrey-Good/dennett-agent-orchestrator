use dennett_agent_core::FakeAgentRuntime;
use dennett_contracts::{CommandId, ProjectChatCommand, ProjectId, SessionId};
use dennett_head::HeadApplication;
use dennett_kernel::ProjectChatUseCase;
use dennett_memory_core::{InMemoryMemory, MemoryPort};
use std::sync::Arc;

#[tokio::test]
async fn credential_free_fake_conversation_returns_result_and_commits_memory() {
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
            text: "hello from the integration test".to_owned(),
        })
        .await
        .expect("the fake runtime should not need cloud credentials");

    assert_eq!(result.command_id, command_id);
    assert_eq!(
        result.summary,
        "Dennett skeleton received: hello from the integration test"
    );
    assert!(!result.partial);
    assert!(result.artifact_handles.is_empty());
    assert!(result.evidence_handles.is_empty());

    let events = memory
        .recent_for_project(project_id, 10)
        .await
        .expect("in-memory canonical memory should be available");
    assert_eq!(events.len(), 1);
    let event = &events[0];
    assert_eq!(event.project_id, project_id);
    assert_eq!(event.session_id, session_id);
    assert_eq!(event.kind, "project_chat_completed");
    assert_eq!(event.summary, result.summary);
}
