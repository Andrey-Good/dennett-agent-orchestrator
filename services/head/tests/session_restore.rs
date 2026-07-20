use dennett_contracts::{CommandId, ProjectId, SessionEventId};
use dennett_head::session::SessionCoordinator;
use dennett_memory_core::session::{
    SessionJournal, SessionResult, SessionTurnOutcome, SessionTurnState,
};
use dennett_storage_sqlite::SqliteControlStore;
use dennett_sync_core::draft::{DraftCachePort, DraftRecord};
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn embedded_head_restores_session_and_sends_recovered_draft_once() {
    let temp = TempDir::new().expect("temporary profile");
    let path = temp.path().join("control.sqlite");
    let project_id = ProjectId::new();

    let store = SqliteControlStore::open(&path)
        .await
        .expect("open control store");
    let coordinator = SessionCoordinator::new(SessionJournal::new(Arc::new(store.clone())), 1, 8);
    let created = coordinator
        .create_session(CommandId::new(), project_id, "Recovered".to_owned(), 1)
        .await
        .expect("create session");
    let session_id = created.snapshot.session.session_id;
    let draft = DraftRecord {
        project_id,
        session_id,
        command_id: CommandId::new(),
        text: "resume after restart".to_owned(),
        revision: 1,
        updated_at_unix_ms: 2,
    };
    store
        .save(draft.clone())
        .await
        .expect("persist unsent draft");
    drop(coordinator);
    store.close().await;

    let reopened = SqliteControlStore::open(&path)
        .await
        .expect("reopen control store");
    let restored_draft = reopened
        .load(session_id)
        .await
        .expect("load draft")
        .expect("draft survives restart");
    assert_eq!(restored_draft, draft);
    let coordinator =
        SessionCoordinator::new(SessionJournal::new(Arc::new(reopened.clone())), 1, 8);
    let accepted = coordinator
        .accept_turn(
            restored_draft.command_id,
            project_id,
            session_id,
            restored_draft.text.clone(),
            None,
            3,
        )
        .await
        .expect("send recovered draft");
    let retry = coordinator
        .accept_turn(
            restored_draft.command_id,
            project_id,
            session_id,
            restored_draft.text,
            None,
            3,
        )
        .await
        .expect("idempotent send retry");
    assert_eq!(retry.agent_turn_id, accepted.agent_turn_id);
    assert_eq!(retry.commit.snapshot.session.revision, 2);
    reopened
        .discard(session_id, restored_draft.command_id)
        .await
        .expect("discard sent draft");

    coordinator
        .append_agent_text(
            SessionEventId::new(),
            session_id,
            accepted.agent_turn_id,
            "partial answer".to_owned(),
            4,
        )
        .await
        .expect("persist partial answer");
    coordinator
        .finish_turn(
            SessionEventId::new(),
            session_id,
            accepted.agent_turn_id,
            SessionTurnState::TimedOut,
            Some(SessionTurnOutcome::Result(SessionResult {
                summary: "partial answer".to_owned(),
                partial: true,
                artifact_handles: Vec::new(),
                evidence_handles: Vec::new(),
            })),
            5,
        )
        .await
        .expect("persist timeout");
    drop(coordinator);
    reopened.close().await;

    let final_store = SqliteControlStore::open(&path).await.expect("final reopen");
    let final_head =
        SessionCoordinator::new(SessionJournal::new(Arc::new(final_store.clone())), 1, 8);
    let restored = final_head.restore_all().await.expect("restore sessions");
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].session.session_id, session_id);
    assert_eq!(restored[0].session.revision, 4);
    assert_eq!(
        restored[0].turns.last().expect("agent turn").state,
        SessionTurnState::TimedOut
    );
    assert_eq!(
        restored[0].turns.last().expect("agent turn").text,
        "partial answer"
    );
    assert_eq!(
        final_store.load(session_id).await.expect("draft lookup"),
        None
    );
}
