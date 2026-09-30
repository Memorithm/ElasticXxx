//! Composition of structural representation-payload decisions with transition stability.
//!
//! Structural payload evidence remains planning-only. This controller submits
//! only a unique non-current minimum to the existing representation/Reencode
//! stability gate. Holds and ambiguous ties do not consume stability state.
//! Even an admitted outcome is not an actuation capability.

use crate::representation_payload_decision::{
    RepresentationPayloadDecisionError, RepresentationPayloadDecisionV1,
};
use crate::representation_payload_gain::{
    evaluate_representation_payload_gain_v1, RepresentationPayloadGainDecisionV1,
    RepresentationPayloadGainError, RepresentationPayloadGainPolicyV1,
};
use crate::representation_payload_selector::RepresentationPayloadCandidateV1;
use crate::{
    ObservationSnapshot, TransitionStabilityError, TransitionStabilityGateV1,
    TransitionStabilityPermitV1, TransitionStabilityPolicyV1, TransitionStabilityReportV1,
};
use elastic_core::{DimensionId, TransitionMechanism};
use std::fmt;
use std::time::Instant;

/// Versioned identity of structural-payload/stability composition.
pub const REPRESENTATION_PAYLOAD_STABILITY_V1: &str =
    "elastic.representation-payload-stability@1.0.0";

/// Non-actuating composed outcome.
#[derive(Debug)]
pub enum RepresentationPayloadStableDecisionV1 {
    Hold {
        decision: RepresentationPayloadDecisionV1,
    },
    Ambiguous {
        decision: RepresentationPayloadDecisionV1,
    },
    InsufficientGain {
        decision: RepresentationPayloadDecisionV1,
        current_payload_bits: u64,
        target_payload_bits: u64,
        absolute_savings_bits: u64,
        relative_savings_bps: u16,
    },
    Deferred {
        decision: RepresentationPayloadDecisionV1,
        stability: TransitionStabilityReportV1,
    },
    Admitted {
        decision: RepresentationPayloadDecisionV1,
        stability: TransitionStabilityReportV1,
        permit: TransitionStabilityPermitV1,
    },
}

impl RepresentationPayloadStableDecisionV1 {
    #[must_use]
    pub const fn is_admitted_transition(&self) -> bool {
        matches!(self, Self::Admitted { .. })
    }

    /// This composed decision still carries no actuation authority.
    #[must_use]
    pub const fn carries_actuation_authority(&self) -> bool {
        false
    }
}

/// Stateful composition of structural payload planning and generic stability.
#[derive(Debug)]
pub struct RepresentationPayloadStabilityControllerV1 {
    gate: TransitionStabilityGateV1,
    gain_policy: RepresentationPayloadGainPolicyV1,
}

impl RepresentationPayloadStabilityControllerV1 {
    pub fn new(
        stability_policy: TransitionStabilityPolicyV1,
    ) -> Result<Self, RepresentationPayloadStabilityError> {
        Self::new_with_gain_policy(
            stability_policy,
            RepresentationPayloadGainPolicyV1::default(),
        )
    }

    pub fn new_with_gain_policy(
        stability_policy: TransitionStabilityPolicyV1,
        gain_policy: RepresentationPayloadGainPolicyV1,
    ) -> Result<Self, RepresentationPayloadStabilityError> {
        if stability_policy.mechanism() != TransitionMechanism::Reencode {
            return Err(RepresentationPayloadStabilityError::MechanismMismatch {
                observed: stability_policy.mechanism(),
            });
        }
        if stability_policy.dimension() != &DimensionId::REPRESENTATION {
            return Err(RepresentationPayloadStabilityError::DimensionMismatch {
                observed: stability_policy.dimension().clone(),
            });
        }

        Ok(Self {
            gate: TransitionStabilityGateV1::new(stability_policy),
            gain_policy,
        })
    }

    #[must_use]
    pub const fn stability_gate(&self) -> &TransitionStabilityGateV1 {
        &self.gate
    }

    #[must_use]
    pub const fn gain_policy(&self) -> RepresentationPayloadGainPolicyV1 {
        self.gain_policy
    }

    /// Evaluate structural payload evidence and apply stability only to a unique
    /// non-current minimum.
    ///
    /// # Errors
    ///
    /// Returns structural decision errors unchanged. Holds and ambiguous ties
    /// return without calling the stability gate.
    pub fn evaluate(
        &mut self,
        current_profile_id: &str,
        candidates: impl IntoIterator<Item = RepresentationPayloadCandidateV1>,
        observations: &ObservationSnapshot,
        now: Instant,
    ) -> Result<RepresentationPayloadStableDecisionV1, RepresentationPayloadDecisionError> {
        let decision = evaluate_representation_payload_gain_v1(
            current_profile_id,
            candidates,
            self.gain_policy,
        )
        .map_err(map_gain_error)?;

        match decision {
            RepresentationPayloadGainDecisionV1::HoldBaseDecision { decision } => match decision {
                RepresentationPayloadDecisionV1::HoldCurrentMinimum { .. } => {
                    Ok(RepresentationPayloadStableDecisionV1::Hold { decision })
                }
                RepresentationPayloadDecisionV1::AmbiguousMinimum { .. } => {
                    Ok(RepresentationPayloadStableDecisionV1::Ambiguous { decision })
                }
                RepresentationPayloadDecisionV1::UniqueTransitionCandidate { .. } => {
                    Err(RepresentationPayloadDecisionError::GainGateInvariant {
                        reason: "unique_transition_returned_as_base_hold",
                    })
                }
            },
            RepresentationPayloadGainDecisionV1::InsufficientGain {
                decision,
                current_payload_bits,
                target_payload_bits,
                absolute_savings_bits,
                relative_savings_bps,
            } => Ok(RepresentationPayloadStableDecisionV1::InsufficientGain {
                decision,
                current_payload_bits,
                target_payload_bits,
                absolute_savings_bits,
                relative_savings_bps,
            }),
            RepresentationPayloadGainDecisionV1::EligibleTransition { decision, .. } => {
                let (stability, permit) = self.gate.check(observations, now);
                Ok(match permit {
                    Some(permit) => RepresentationPayloadStableDecisionV1::Admitted {
                        decision,
                        stability,
                        permit,
                    },
                    None => RepresentationPayloadStableDecisionV1::Deferred {
                        decision,
                        stability,
                    },
                })
            }
        }
    }

    /// Record stability only after a separately validated physical profile
    /// transition has actually committed.
    pub fn record_commit(
        &mut self,
        permit: TransitionStabilityPermitV1,
        committed_at: Instant,
    ) -> Result<(), TransitionStabilityError> {
        self.gate.record_commit(permit, committed_at)
    }
}

/// Construction errors for structural-payload/stability composition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RepresentationPayloadStabilityError {
    MechanismMismatch { observed: TransitionMechanism },
    DimensionMismatch { observed: DimensionId },
}

impl fmt::Display for RepresentationPayloadStabilityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MechanismMismatch { observed } => write!(
                formatter,
                "representation payload stability requires Reencode mechanism, observed {observed:?}"
            ),
            Self::DimensionMismatch { observed } => write!(
                formatter,
                "representation payload stability requires representation dimension, observed {}",
                observed.as_str()
            ),
        }
    }
}

impl std::error::Error for RepresentationPayloadStabilityError {}

fn map_gain_error(error: RepresentationPayloadGainError) -> RepresentationPayloadDecisionError {
    match error {
        RepresentationPayloadGainError::CurrentProfileMissing { profile_id } => {
            RepresentationPayloadDecisionError::CurrentProfileMissing { profile_id }
        }
        RepresentationPayloadGainError::Decision(error) => error,
        RepresentationPayloadGainError::NonDecreasingTarget { .. } => {
            RepresentationPayloadDecisionError::GainGateInvariant {
                reason: "non_decreasing_target",
            }
        }
        RepresentationPayloadGainError::RelativeSavingsOverflow => {
            RepresentationPayloadDecisionError::GainGateInvariant {
                reason: "relative_savings_overflow",
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TransitionStabilityStatusV1;
    use std::time::Duration;

    fn candidate(id: &str, bits: u64) -> RepresentationPayloadCandidateV1 {
        RepresentationPayloadCandidateV1::new(id, bits).unwrap()
    }

    fn controller() -> RepresentationPayloadStabilityControllerV1 {
        RepresentationPayloadStabilityControllerV1::new(
            TransitionStabilityPolicyV1::new(
                TransitionMechanism::Reencode,
                DimensionId::REPRESENTATION,
                None,
                Some(Duration::from_secs(10)),
                None,
            )
            .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn hold_and_ambiguity_do_not_touch_stability_generation() {
        let now = Instant::now();
        let observations = ObservationSnapshot::new(now, vec![]);
        let mut controller = controller();

        let hold = controller
            .evaluate(
                "dense",
                [
                    candidate("dense", 384),
                    candidate("hybrid", 384),
                    candidate("sparse", 1536),
                ],
                &observations,
                now,
            )
            .unwrap();
        assert!(matches!(
            hold,
            RepresentationPayloadStableDecisionV1::Hold { .. }
        ));
        assert_eq!(controller.stability_gate().generation(), 0);

        let ambiguous = controller
            .evaluate(
                "sparse",
                [
                    candidate("dense", 384),
                    candidate("hybrid", 384),
                    candidate("sparse", 1536),
                ],
                &observations,
                now,
            )
            .unwrap();
        assert!(matches!(
            ambiguous,
            RepresentationPayloadStableDecisionV1::Ambiguous { .. }
        ));
        assert_eq!(controller.stability_gate().generation(), 0);
    }

    #[test]
    fn unique_transition_uses_existing_cooldown_after_commit() {
        let start = Instant::now();
        let observations = ObservationSnapshot::new(start, vec![]);
        let mut controller = controller();

        let first = controller
            .evaluate(
                "dense",
                [
                    candidate("dense", 8192),
                    candidate("hybrid", 4288),
                    candidate("sparse", 512),
                ],
                &observations,
                start,
            )
            .unwrap();

        let permit = match first {
            RepresentationPayloadStableDecisionV1::Admitted { permit, .. } => permit,
            other => panic!("expected admitted profile transition, observed {other:?}"),
        };
        controller.record_commit(permit, start).unwrap();

        let later = start + Duration::from_secs(1);
        let later_observations = ObservationSnapshot::new(later, vec![]);
        let second = controller
            .evaluate(
                "sparse",
                [
                    candidate("dense", 128),
                    candidate("hybrid", 256),
                    candidate("sparse", 512),
                ],
                &later_observations,
                later,
            )
            .unwrap();

        match second {
            RepresentationPayloadStableDecisionV1::Deferred { stability, .. } => {
                assert_eq!(
                    stability.status,
                    TransitionStabilityStatusV1::CooldownActive
                );
            }
            other => panic!("expected deferred profile transition, observed {other:?}"),
        }
    }

    #[test]
    fn configured_gain_floor_blocks_marginal_target_without_touching_stability() {
        let now = Instant::now();
        let observations = ObservationSnapshot::new(now, vec![]);
        let stability_policy = TransitionStabilityPolicyV1::new(
            TransitionMechanism::Reencode,
            DimensionId::REPRESENTATION,
            None,
            Some(Duration::from_secs(10)),
            None,
        )
        .unwrap();
        let gain_policy = RepresentationPayloadGainPolicyV1::new(64, 500).unwrap();
        let mut controller = RepresentationPayloadStabilityControllerV1::new_with_gain_policy(
            stability_policy,
            gain_policy,
        )
        .unwrap();

        let result = controller
            .evaluate(
                "dense",
                [candidate("dense", 1_000), candidate("hybrid", 960)],
                &observations,
                now,
            )
            .unwrap();

        assert!(matches!(
            result,
            RepresentationPayloadStableDecisionV1::InsufficientGain { .. }
        ));
        assert_eq!(controller.stability_gate().generation(), 0);
        assert_eq!(controller.gain_policy(), gain_policy);
    }

    #[test]
    fn composed_outcome_never_carries_actuation_authority() {
        let now = Instant::now();
        let observations = ObservationSnapshot::new(now, vec![]);
        let mut controller = controller();
        let result = controller
            .evaluate(
                "dense",
                [candidate("dense", 512), candidate("sparse", 128)],
                &observations,
                now,
            )
            .unwrap();

        assert!(result.is_admitted_transition());
        assert!(!result.carries_actuation_authority());
    }

    #[test]
    fn wrong_transition_class_is_rejected() {
        let wrong_mechanism = TransitionStabilityPolicyV1::new(
            TransitionMechanism::Reinterpret,
            DimensionId::REPRESENTATION,
            None,
            Some(Duration::from_secs(1)),
            None,
        )
        .unwrap();
        assert!(matches!(
            RepresentationPayloadStabilityControllerV1::new(wrong_mechanism),
            Err(RepresentationPayloadStabilityError::MechanismMismatch { .. })
        ));

        let wrong_dimension = TransitionStabilityPolicyV1::new(
            TransitionMechanism::Reencode,
            DimensionId::CAPACITY,
            None,
            Some(Duration::from_secs(1)),
            None,
        )
        .unwrap();
        assert!(matches!(
            RepresentationPayloadStabilityControllerV1::new(wrong_dimension),
            Err(RepresentationPayloadStabilityError::DimensionMismatch { .. })
        ));
    }
}
