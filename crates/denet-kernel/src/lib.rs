
//! Composition-level interfaces shared by application services.

use async_trait::async_trait;
use denet_contracts::{ProjectChatCommand, ResultEnvelope};

#[derive(Debug, thiserror::Error)]
pub enum DenetError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("dependency unavailable: {0}")]
    Unavailable(String),
    #[error("operation cancelled")]
    Cancelled,
    #[error("internal error: {0}")]
    Internal(String),
}

pub type DenetResult<T> = Result<T, DenetError>;

#[async_trait]
pub trait ProjectChatUseCase: Send + Sync {
    async fn execute(&self, command: ProjectChatCommand) -> DenetResult<ResultEnvelope>;
}
