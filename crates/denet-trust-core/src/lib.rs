
//! Deterministic trust decisions for the skeleton.

use denet_contracts::{HeadEligibility, MemoryDeploymentMode};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthenticationAssurance { Low, Normal, Strong }

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HeadPromotionDecision {
    Allow,
    Deny(&'static str),
}

#[must_use]
pub fn evaluate_head_promotion(
    eligibility: HeadEligibility,
    assurance: AuthenticationAssurance,
    memory_mode: MemoryDeploymentMode,
    canonical_data_ready: bool,
    fencing_ready: bool,
) -> HeadPromotionDecision {
    if eligibility != HeadEligibility::Full {
        return HeadPromotionDecision::Deny("device was not explicitly authorized as a full Head candidate");
    }
    if assurance != AuthenticationAssurance::Strong {
        return HeadPromotionDecision::Deny("strong owner authentication is required");
    }
    if !matches!(memory_mode, MemoryDeploymentMode::CanonicalService | MemoryDeploymentMode::FullReplicaCandidate) {
        return HeadPromotionDecision::Deny("device does not have a complete canonical memory profile");
    }
    if !canonical_data_ready { return HeadPromotionDecision::Deny("canonical data is not ready"); }
    if !fencing_ready { return HeadPromotionDecision::Deny("authority fencing is not ready"); }
    HeadPromotionDecision::Allow
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_client_cannot_become_head() {
        assert!(matches!(
            evaluate_head_promotion(HeadEligibility::None, AuthenticationAssurance::Strong, MemoryDeploymentMode::ClientCache, true, true),
            HeadPromotionDecision::Deny(_)
        ));
    }

    #[test]
    fn explicit_full_candidate_can_become_head_when_ready() {
        assert_eq!(
            evaluate_head_promotion(HeadEligibility::Full, AuthenticationAssurance::Strong, MemoryDeploymentMode::FullReplicaCandidate, true, true),
            HeadPromotionDecision::Allow
        );
    }
}
