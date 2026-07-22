//! External effect claims prevent blind duplicate sends, payments or publications.

pub mod workspace;

use dennett_contracts::EffectId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectState {
    Prepared,
    Dispatching,
    Confirmed,
    Failed,
    Unknown,
    Compensating,
    Compensated,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EffectClaim {
    pub effect_id: EffectId,
    pub idempotency_key: String,
    pub exact_target: String,
    pub state: EffectState,
    pub authority_epoch: u64,
}

impl EffectClaim {
    pub fn mark_unknown(&mut self) {
        self.state = EffectState::Unknown;
    }
    #[must_use]
    pub fn may_retry(&self) -> bool {
        matches!(self.state, EffectState::Failed)
    }
}
