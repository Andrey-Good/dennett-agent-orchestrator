//! Domain-level operation log shapes; transport and storage are adapters.

pub mod admission;
pub mod draft;
pub mod watch;

use dennett_contracts::{CommandId, DeviceId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OfflineOperation {
    pub command_id: CommandId,
    pub device_id: DeviceId,
    pub authority_epoch_seen: u64,
    pub base_revision: Option<u64>,
    pub payload_type: String,
    pub payload_json: serde_json::Value,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationDisposition {
    Accept,
    RejectStaleEpoch,
    RequireRevalidation,
}

#[must_use]
pub fn classify_operation(
    op: &OfflineOperation,
    current_epoch: u64,
    consequential: bool,
) -> OperationDisposition {
    if op.authority_epoch_seen != current_epoch {
        return OperationDisposition::RejectStaleEpoch;
    }
    if consequential {
        return OperationDisposition::RequireRevalidation;
    }
    OperationDisposition::Accept
}
