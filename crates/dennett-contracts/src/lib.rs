
//! Stable identifiers and cross-process contracts for the Dennett skeleton.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);
        impl $name {
            #[must_use]
            pub fn new() -> Self { Self(Uuid::now_v7()) }
        }
        impl Default for $name { fn default() -> Self { Self::new() } }
    };
}

id_type!(ProjectId);
id_type!(SessionId);
id_type!(TaskId);
id_type!(RunId);
id_type!(CommandId);
id_type!(MemoryEventId);
id_type!(DeviceId);
id_type!(EffectId);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeadEligibility {
    None,
    Emergency,
    Full,
}

impl Default for HeadEligibility {
    fn default() -> Self { Self::None }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryDeploymentMode {
    ClientCache,
    EmbeddedSingleDeviceCanonical,
    CanonicalService,
    FullReplicaCandidate,
    EmergencySubset,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceManifest {
    pub device_id: DeviceId,
    pub display_name: String,
    pub head_eligibility: HeadEligibility,
    pub memory_mode: MemoryDeploymentMode,
    pub authority_epoch_seen: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectChatCommand {
    pub command_id: CommandId,
    pub project_id: ProjectId,
    pub session_id: SessionId,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResultEnvelope {
    pub command_id: CommandId,
    pub summary: String,
    pub partial: bool,
    pub artifact_handles: Vec<String>,
    pub evidence_handles: Vec<String>,
}
