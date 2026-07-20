use async_trait::async_trait;
use dennett_contracts::CommandId;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandAdmissionRequest {
    pub command_id: CommandId,
    pub idempotency_key: String,
    pub correlation_id: String,
    pub operation_kind: String,
    pub intent_hash: [u8; 32],
    pub admitted_at_unix_ms: u64,
}

impl CommandAdmissionRequest {
    pub fn validate(&self) -> Result<(), CommandAdmissionError> {
        if self.idempotency_key.trim().is_empty()
            || self.correlation_id.trim().is_empty()
            || self.operation_kind.trim().is_empty()
        {
            return Err(CommandAdmissionError::InvalidRequest);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandAdmissionReceipt {
    pub command_id: CommandId,
    pub operation_id: CommandId,
    pub correlation_id: String,
    pub accepted_revision: u64,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum CommandAdmissionError {
    #[error("command admission request is invalid")]
    InvalidRequest,
    #[error("command or idempotency identity was reused for another intent")]
    IdempotencyConflict,
    #[error("command admission storage is unavailable")]
    StorageUnavailable,
    #[error("command admission storage is inconsistent")]
    IntegrityFailure,
}

#[async_trait]
pub trait CommandAdmissionPort: Send + Sync {
    async fn admit(
        &self,
        request: CommandAdmissionRequest,
    ) -> Result<CommandAdmissionReceipt, CommandAdmissionError>;
}

#[derive(Clone, Default)]
pub struct InMemoryCommandAdmissionStore {
    state: Arc<Mutex<InMemoryAdmissionState>>,
}

#[derive(Default)]
struct InMemoryAdmissionState {
    records: Vec<AdmissionRecord>,
    next_revision: u64,
}

#[derive(Clone)]
struct AdmissionRecord {
    command_id: CommandId,
    idempotency_key: String,
    operation_kind: String,
    intent_hash: [u8; 32],
    accepted_revision: u64,
}

#[async_trait]
impl CommandAdmissionPort for InMemoryCommandAdmissionStore {
    async fn admit(
        &self,
        request: CommandAdmissionRequest,
    ) -> Result<CommandAdmissionReceipt, CommandAdmissionError> {
        request.validate()?;
        let mut state = self.state.lock().await;
        if let Some(existing) = state.records.iter().find(|record| {
            record.command_id == request.command_id
                || record.idempotency_key == request.idempotency_key
        }) {
            if existing.command_id != request.command_id
                || existing.idempotency_key != request.idempotency_key
                || existing.operation_kind != request.operation_kind
                || existing.intent_hash != request.intent_hash
            {
                return Err(CommandAdmissionError::IdempotencyConflict);
            }
            return Ok(receipt(&request, existing.accepted_revision));
        }
        state.next_revision = state
            .next_revision
            .checked_add(1)
            .ok_or(CommandAdmissionError::IntegrityFailure)?;
        let accepted_revision = state.next_revision;
        state.records.push(AdmissionRecord {
            command_id: request.command_id,
            idempotency_key: request.idempotency_key.clone(),
            operation_kind: request.operation_kind.clone(),
            intent_hash: request.intent_hash,
            accepted_revision,
        });
        Ok(receipt(&request, accepted_revision))
    }
}

fn receipt(request: &CommandAdmissionRequest, accepted_revision: u64) -> CommandAdmissionReceipt {
    CommandAdmissionReceipt {
        command_id: request.command_id,
        operation_id: request.command_id,
        correlation_id: request.correlation_id.clone(),
        accepted_revision,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(command_id: CommandId, key: &str, intent: u8) -> CommandAdmissionRequest {
        CommandAdmissionRequest {
            command_id,
            idempotency_key: key.to_owned(),
            correlation_id: "correlation".to_owned(),
            operation_kind: "send_turn".to_owned(),
            intent_hash: [intent; 32],
            admitted_at_unix_ms: 1,
        }
    }

    #[tokio::test]
    async fn admission_is_monotonic_and_idempotent() {
        let store = InMemoryCommandAdmissionStore::default();
        let command_id = CommandId::new();
        let first = store
            .admit(request(command_id, "key-a", 1))
            .await
            .expect("first admission");
        let replay = store
            .admit(request(command_id, "key-a", 1))
            .await
            .expect("replayed admission");
        let second = store
            .admit(request(CommandId::new(), "key-b", 2))
            .await
            .expect("second admission");
        assert_eq!(first.accepted_revision, replay.accepted_revision);
        assert_eq!(second.accepted_revision, first.accepted_revision + 1);
    }

    #[tokio::test]
    async fn reused_identity_with_changed_intent_is_rejected() {
        let store = InMemoryCommandAdmissionStore::default();
        let command_id = CommandId::new();
        store
            .admit(request(command_id, "key", 1))
            .await
            .expect("first admission");
        assert_eq!(
            store.admit(request(command_id, "key", 2)).await,
            Err(CommandAdmissionError::IdempotencyConflict)
        );
        assert_eq!(
            store.admit(request(CommandId::new(), "key", 1)).await,
            Err(CommandAdmissionError::IdempotencyConflict)
        );
    }
}
