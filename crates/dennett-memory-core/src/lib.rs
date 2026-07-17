//! One logical Memory Fabric with replaceable embedded/service persistence adapters.

use async_trait::async_trait;
use dennett_contracts::{MemoryEventId, ProjectId, SessionId};
use dennett_kernel::DennettResult;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemoryEvent {
    pub event_id: MemoryEventId,
    pub project_id: ProjectId,
    pub session_id: SessionId,
    pub kind: String,
    pub summary: String,
}

#[async_trait]
pub trait MemoryPort: Send + Sync {
    async fn append(&self, event: MemoryEvent) -> DennettResult<()>;
    async fn recent_for_project(
        &self,
        project_id: ProjectId,
        limit: usize,
    ) -> DennettResult<Vec<MemoryEvent>>;
}

#[derive(Clone, Default)]
pub struct InMemoryMemory {
    events: Arc<RwLock<Vec<MemoryEvent>>>,
}

#[async_trait]
impl MemoryPort for InMemoryMemory {
    async fn append(&self, event: MemoryEvent) -> DennettResult<()> {
        self.events.write().await.push(event);
        Ok(())
    }

    async fn recent_for_project(
        &self,
        project_id: ProjectId,
        limit: usize,
    ) -> DennettResult<Vec<MemoryEvent>> {
        let events = self.events.read().await;
        Ok(events
            .iter()
            .rev()
            .filter(|e| e.project_id == project_id)
            .take(limit)
            .cloned()
            .collect())
    }
}
