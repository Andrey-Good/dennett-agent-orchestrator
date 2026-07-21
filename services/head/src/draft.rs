use crate::session::SessionCoordinator;
use dennett_contracts::{CommandId, ProjectId, SessionId};
use dennett_memory_core::session::SessionJournalError;
use dennett_sync_core::draft::{
    DraftCacheError, DraftCachePort, DraftCacheSaveOutcome, DraftRecord,
};
use std::{
    collections::HashMap,
    sync::{Arc, Weak},
};
use tokio::sync::{Mutex, OwnedMutexGuard};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftSaveOutcome {
    Saved,
    AlreadyAccepted,
}

#[derive(Clone, Default)]
pub struct SessionOperationLocks {
    locks: Arc<Mutex<HashMap<SessionId, Weak<Mutex<()>>>>>,
}

impl SessionOperationLocks {
    pub async fn acquire(&self, session_id: SessionId) -> OwnedMutexGuard<()> {
        let lock = {
            let mut locks = self.locks.lock().await;
            locks.retain(|_, lock| lock.strong_count() > 0);
            if let Some(lock) = locks.get(&session_id).and_then(Weak::upgrade) {
                lock
            } else {
                let lock = Arc::new(Mutex::new(()));
                locks.insert(session_id, Arc::downgrade(&lock));
                lock
            }
        };
        lock.lock_owned().await
    }
}

#[derive(Clone)]
pub struct ComposerDraftApplication {
    sessions: SessionCoordinator,
    cache: Arc<dyn DraftCachePort>,
    locks: SessionOperationLocks,
}

impl ComposerDraftApplication {
    #[must_use]
    pub fn new(
        sessions: SessionCoordinator,
        cache: Arc<dyn DraftCachePort>,
        locks: SessionOperationLocks,
    ) -> Self {
        Self {
            sessions,
            cache,
            locks,
        }
    }

    pub async fn load(
        &self,
        project_id: Option<ProjectId>,
        session_id: SessionId,
    ) -> Result<Option<DraftRecord>, DraftApplicationError> {
        let _guard = self.locks.acquire(session_id).await;
        self.require_session(project_id, session_id).await?;
        let draft = self.cache.load(session_id).await?;
        if let Some(draft) = &draft
            && let Some(event) = self.sessions.event_for_command(draft.command_id).await?
        {
            if event.session_id != session_id {
                return Err(DraftApplicationError::StableCommandMismatch);
            }
            self.cache.discard(session_id, draft.command_id).await?;
            return Ok(None);
        }
        if draft
            .as_ref()
            .is_some_and(|draft| draft.project_id != project_id || draft.session_id != session_id)
        {
            return Err(DraftApplicationError::ScopeMismatch);
        }
        Ok(draft)
    }

    pub async fn save(
        &self,
        draft: DraftRecord,
    ) -> Result<DraftSaveOutcome, DraftApplicationError> {
        let _guard = self.locks.acquire(draft.session_id).await;
        self.require_session(draft.project_id, draft.session_id)
            .await?;
        if let Some(event) = self.sessions.event_for_command(draft.command_id).await? {
            if event.session_id != draft.session_id {
                return Err(DraftApplicationError::StableCommandMismatch);
            }
            self.cache
                .discard(draft.session_id, draft.command_id)
                .await?;
            return Ok(DraftSaveOutcome::AlreadyAccepted);
        }
        match self.cache.save(draft).await? {
            DraftCacheSaveOutcome::Saved | DraftCacheSaveOutcome::StaleIgnored => {
                Ok(DraftSaveOutcome::Saved)
            }
            DraftCacheSaveOutcome::Discarded => Ok(DraftSaveOutcome::AlreadyAccepted),
        }
    }

    pub async fn discard(
        &self,
        project_id: Option<ProjectId>,
        session_id: SessionId,
        command_id: CommandId,
    ) -> Result<bool, DraftApplicationError> {
        let _guard = self.locks.acquire(session_id).await;
        self.require_session(project_id, session_id).await?;
        if let Some(existing) = self.cache.load(session_id).await?
            && (existing.project_id != project_id || existing.command_id != command_id)
        {
            return Err(DraftApplicationError::StableCommandMismatch);
        }
        Ok(self.cache.discard(session_id, command_id).await?)
    }

    pub(crate) async fn acquire(&self, session_id: SessionId) -> OwnedMutexGuard<()> {
        self.locks.acquire(session_id).await
    }

    pub(crate) async fn discard_accepted(
        &self,
        session_id: SessionId,
        command_id: CommandId,
    ) -> Result<(), DraftCacheError> {
        self.cache.discard(session_id, command_id).await?;
        Ok(())
    }

    async fn require_session(
        &self,
        project_id: Option<ProjectId>,
        session_id: SessionId,
    ) -> Result<(), DraftApplicationError> {
        let snapshot = self.sessions.restore(session_id).await?;
        if snapshot.session.project_id != project_id {
            return Err(DraftApplicationError::ScopeMismatch);
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DraftApplicationError {
    #[error("draft scope does not match the project session")]
    ScopeMismatch,
    #[error("draft command identity is already owned by another session")]
    StableCommandMismatch,
    #[error(transparent)]
    Session(#[from] SessionJournalError),
    #[error(transparent)]
    Cache(#[from] DraftCacheError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use dennett_memory_core::session::{InMemorySessionEventStore, SessionJournal};
    use dennett_sync_core::draft::InMemoryDraftCache;

    async fn application() -> (
        ComposerDraftApplication,
        SessionCoordinator,
        ProjectId,
        SessionId,
    ) {
        let sessions = SessionCoordinator::new(
            SessionJournal::new(Arc::new(InMemorySessionEventStore::default())),
            1,
            8,
        );
        let project_id = ProjectId::new();
        let created = sessions
            .create_session(CommandId::new(), Some(project_id), "Draft".to_owned(), 1)
            .await
            .expect("create session");
        let session_id = created.snapshot.session.session_id;
        let application = ComposerDraftApplication::new(
            sessions.clone(),
            Arc::new(InMemoryDraftCache::default()),
            SessionOperationLocks::default(),
        );
        (application, sessions, project_id, session_id)
    }

    #[tokio::test]
    async fn accepted_command_cannot_be_resurrected_by_a_late_draft_save() {
        let (application, sessions, project_id, session_id) = application().await;
        let command_id = CommandId::new();
        let draft = DraftRecord {
            project_id: Some(project_id),
            session_id,
            command_id,
            text: "send once".to_owned(),
            revision: 1,
            updated_at_unix_ms: 2,
        };
        assert_eq!(
            application.save(draft.clone()).await.expect("save"),
            DraftSaveOutcome::Saved
        );
        sessions
            .accept_turn(
                command_id,
                Some(project_id),
                session_id,
                draft.text.clone(),
                Some(1),
                3,
            )
            .await
            .expect("accept command");
        assert_eq!(
            application.save(draft).await.expect("late save"),
            DraftSaveOutcome::AlreadyAccepted
        );
        assert_eq!(
            application
                .load(Some(project_id), session_id)
                .await
                .expect("load"),
            None
        );
    }
}
