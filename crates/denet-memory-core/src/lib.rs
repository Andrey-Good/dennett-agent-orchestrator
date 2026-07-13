
//! One logical Memory Fabric with replaceable embedded/service persistence adapters.

use async_trait::async_trait;
use denet_contracts::{MemoryEventId, ProjectId, SessionId};
use denet_kernel::DenetResult;
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
    async fn append(&self, event: MemoryEvent) -> DenetResult<()>;
    async fn recent_for_project(&self, project_id: ProjectId, limit: usize) -> DenetResult<Vec<MemoryEvent>>;
}

#[derive(Clone, Default)]
pub struct InMemoryMemory {
    events: Arc<RwLock<Vec<MemoryEvent>>>,
}

#[async_trait]
impl MemoryPort for InMemoryMemory {
    async fn append(&self, event: MemoryEvent) -> DenetResult<()> {
        self.events.write().await.push(event);
        Ok(())
    }

    async fn recent_for_project(&self, project_id: ProjectId, limit: usize) -> DenetResult<Vec<MemoryEvent>> {
        let events = self.events.read().await;
        Ok(events.iter().rev().filter(|e| e.project_id == project_id).take(limit).cloned().collect())
    }
}
