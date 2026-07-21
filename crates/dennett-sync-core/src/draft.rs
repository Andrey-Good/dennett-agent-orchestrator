use async_trait::async_trait;
use dennett_contracts::{CommandId, ProjectId, SessionId};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DraftRecord {
    pub project_id: Option<ProjectId>,
    pub session_id: SessionId,
    pub command_id: CommandId,
    pub text: String,
    pub revision: u64,
    pub updated_at_unix_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftCacheSaveOutcome {
    Saved,
    StaleIgnored,
    Discarded,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DraftCacheError {
    #[error("draft command identity changed for an existing session")]
    StableCommandMismatch,
    #[error("draft cache storage is unavailable")]
    StorageUnavailable,
    #[error("draft cache schema could not be migrated safely")]
    MigrationFailure,
}

#[async_trait]
pub trait DraftCachePort: Send + Sync {
    async fn save(&self, draft: DraftRecord) -> Result<DraftCacheSaveOutcome, DraftCacheError>;
    async fn load(&self, session_id: SessionId) -> Result<Option<DraftRecord>, DraftCacheError>;
    async fn discard(
        &self,
        session_id: SessionId,
        command_id: CommandId,
    ) -> Result<bool, DraftCacheError>;
}

#[derive(Clone, Default)]
pub struct InMemoryDraftCache {
    state: Arc<RwLock<InMemoryDraftState>>,
}

#[derive(Default)]
struct InMemoryDraftState {
    drafts: HashMap<SessionId, DraftRecord>,
    discarded_commands: HashSet<CommandId>,
}

#[async_trait]
impl DraftCachePort for InMemoryDraftCache {
    async fn save(&self, draft: DraftRecord) -> Result<DraftCacheSaveOutcome, DraftCacheError> {
        if draft.revision == 0 {
            return Err(DraftCacheError::StorageUnavailable);
        }
        let mut state = self.state.write().await;
        if state.discarded_commands.contains(&draft.command_id) {
            return Ok(DraftCacheSaveOutcome::Discarded);
        }
        if state.drafts.values().any(|existing| {
            existing.session_id != draft.session_id && existing.command_id == draft.command_id
        }) || state
            .drafts
            .get(&draft.session_id)
            .is_some_and(|existing| existing.command_id != draft.command_id)
        {
            return Err(DraftCacheError::StableCommandMismatch);
        }
        if state
            .drafts
            .get(&draft.session_id)
            .is_some_and(|existing| existing.revision >= draft.revision)
        {
            return Ok(DraftCacheSaveOutcome::StaleIgnored);
        }
        state.drafts.insert(draft.session_id, draft);
        Ok(DraftCacheSaveOutcome::Saved)
    }

    async fn load(&self, session_id: SessionId) -> Result<Option<DraftRecord>, DraftCacheError> {
        Ok(self.state.read().await.drafts.get(&session_id).cloned())
    }

    async fn discard(
        &self,
        session_id: SessionId,
        command_id: CommandId,
    ) -> Result<bool, DraftCacheError> {
        let mut state = self.state.write().await;
        if state
            .drafts
            .get(&session_id)
            .is_some_and(|draft| draft.command_id != command_id)
        {
            return Err(DraftCacheError::StableCommandMismatch);
        }
        let existed = state.drafts.remove(&session_id).is_some();
        state.discarded_commands.insert(command_id);
        Ok(existed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn draft_keeps_one_stable_command_and_discard_is_local() {
        let cache = InMemoryDraftCache::default();
        let session_id = SessionId::new();
        let command_id = CommandId::new();
        let mut draft = DraftRecord {
            project_id: Some(ProjectId::new()),
            session_id,
            command_id,
            text: "first".to_owned(),
            revision: 1,
            updated_at_unix_ms: 1,
        };
        cache.save(draft.clone()).await.expect("save draft");
        draft.text = "second".to_owned();
        draft.revision = 2;
        cache.save(draft.clone()).await.expect("update draft");
        assert_eq!(
            cache.load(session_id).await.expect("load draft"),
            Some(draft.clone())
        );

        let conflicting = DraftRecord {
            command_id: CommandId::new(),
            ..cache
                .load(session_id)
                .await
                .expect("load existing")
                .expect("draft exists")
        };
        assert_eq!(
            cache.save(conflicting).await,
            Err(DraftCacheError::StableCommandMismatch)
        );
        cache
            .discard(session_id, draft.command_id)
            .await
            .expect("discard draft");
        assert_eq!(
            cache.load(session_id).await.expect("load after discard"),
            None
        );
        assert_eq!(
            cache.save(draft).await.expect("late save is classified"),
            DraftCacheSaveOutcome::Discarded
        );
        assert_eq!(
            cache.load(session_id).await.expect("load after late save"),
            None
        );
    }

    #[tokio::test]
    async fn stale_save_cannot_replace_newer_text() {
        let cache = InMemoryDraftCache::default();
        let newer = DraftRecord {
            project_id: Some(ProjectId::new()),
            session_id: SessionId::new(),
            command_id: CommandId::new(),
            text: "newer".to_owned(),
            revision: 2,
            updated_at_unix_ms: 2,
        };
        cache.save(newer.clone()).await.expect("save newer");
        let stale = DraftRecord {
            text: "stale".to_owned(),
            revision: 1,
            updated_at_unix_ms: 3,
            ..newer.clone()
        };
        assert_eq!(
            cache.save(stale).await.expect("ignore stale"),
            DraftCacheSaveOutcome::StaleIgnored
        );
        assert_eq!(
            cache.load(newer.session_id).await.expect("load"),
            Some(newer)
        );
    }
}
