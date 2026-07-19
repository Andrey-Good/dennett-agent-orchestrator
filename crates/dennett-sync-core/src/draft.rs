use async_trait::async_trait;
use dennett_contracts::{CommandId, ProjectId, SessionId};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DraftRecord {
    pub project_id: ProjectId,
    pub session_id: SessionId,
    pub command_id: CommandId,
    pub text: String,
    pub updated_at_unix_ms: u64,
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
    async fn save(&self, draft: DraftRecord) -> Result<(), DraftCacheError>;
    async fn load(&self, session_id: SessionId) -> Result<Option<DraftRecord>, DraftCacheError>;
    async fn discard(&self, session_id: SessionId) -> Result<(), DraftCacheError>;
}

#[derive(Clone, Default)]
pub struct InMemoryDraftCache {
    drafts: Arc<RwLock<HashMap<SessionId, DraftRecord>>>,
}

#[async_trait]
impl DraftCachePort for InMemoryDraftCache {
    async fn save(&self, draft: DraftRecord) -> Result<(), DraftCacheError> {
        let mut drafts = self.drafts.write().await;
        if drafts.values().any(|existing| {
            existing.session_id != draft.session_id && existing.command_id == draft.command_id
        }) || drafts
            .get(&draft.session_id)
            .is_some_and(|existing| existing.command_id != draft.command_id)
        {
            return Err(DraftCacheError::StableCommandMismatch);
        }
        drafts.insert(draft.session_id, draft);
        Ok(())
    }

    async fn load(&self, session_id: SessionId) -> Result<Option<DraftRecord>, DraftCacheError> {
        Ok(self.drafts.read().await.get(&session_id).cloned())
    }

    async fn discard(&self, session_id: SessionId) -> Result<(), DraftCacheError> {
        self.drafts.write().await.remove(&session_id);
        Ok(())
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
            project_id: ProjectId::new(),
            session_id,
            command_id,
            text: "first".to_owned(),
            updated_at_unix_ms: 1,
        };
        cache.save(draft.clone()).await.expect("save draft");
        draft.text = "second".to_owned();
        cache.save(draft.clone()).await.expect("update draft");
        assert_eq!(
            cache.load(session_id).await.expect("load draft"),
            Some(draft)
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
        cache.discard(session_id).await.expect("discard draft");
        assert_eq!(
            cache.load(session_id).await.expect("load after discard"),
            None
        );
    }
}
