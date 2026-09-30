//! Planning-only minimum-gain gate for structural representation payload changes.
//!
//! This gate sits between exact payload selection and transition stability. It
//! prevents a unique lower-payload target from being treated as transition-worthy
//! when the structural saving is below an explicit absolute and/or relative floor.
//! It never grants transition or actuation authority.

use crate::representation_payload_selector::RepresentationPayloadCandidateV1;
use crate::{
    evaluate_representation_payload_v1, RepresentationPayloadDecisionError,
    RepresentationPayloadDecisionV1,
};
use std::fmt;

/// Versioned identity of the structural minimum-gain gate.
pub const REPRESENTATION_PAYLOAD_GAIN_GATE_V1: &str =
    "elastic.representation-payload-gain-gate@1.0.0";

/// Maximum relative saving threshold in basis points.
pub const MAX_REPRESENTATION_PAYLOAD_GAIN_BPS_V1: u16 = 10_000;

/// Planning-only minimum-benefit policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RepresentationPayloadGainPolicyV1 {
    minimum_absolute_savings_bits: u64,
    minimum_relative_savings_bps: u16,
}

impl Default for RepresentationPayloadGainPolicyV1 {
    fn default() -> Self {
        Self {
            minimum_absolute_savings_bits: 0,
            minimum_relative_savings_bps: 0,
        }
    }
}

impl RepresentationPayloadGainPolicyV1 {
    /// Construct one bounded gain policy.
    ///
    /// # Errors
    ///
    /// Rejects relative thresholds above 10,000 basis points.
    pub const fn new(
        minimum_absolute_savings_bits: u64,
        minimum_relative_savings_bps: u16,
    ) -> Result<Self, RepresentationPayloadGainPolicyError> {
        if minimum_relative_savings_bps > MAX_REPRESENTATION_PAYLOAD_GAIN_BPS_V1 {
            return Err(
                RepresentationPayloadGainPolicyError::RelativeThresholdOutOfRange {
                    observed_bps: minimum_relative_savings_bps,
                    maximum_bps: MAX_REPRESENTATION_PAYLOAD_GAIN_BPS_V1,
                },
            );
        }
        Ok(Self {
            minimum_absolute_savings_bits,
            minimum_relative_savings_bps,
        })
    }

    #[must_use]
    pub const fn minimum_absolute_savings_bits(self) -> u64 {
        self.minimum_absolute_savings_bits
    }

    #[must_use]
    pub const fn minimum_relative_savings_bps(self) -> u16 {
        self.minimum_relative_savings_bps
    }
}

/// Planning-only outcome after the minimum-gain gate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RepresentationPayloadGainDecisionV1 {
    HoldBaseDecision {
        decision: RepresentationPayloadDecisionV1,
    },
    InsufficientGain {
        decision: RepresentationPayloadDecisionV1,
        current_payload_bits: u64,
        target_payload_bits: u64,
        absolute_savings_bits: u64,
        relative_savings_bps: u16,
    },
    EligibleTransition {
        decision: RepresentationPayloadDecisionV1,
        current_payload_bits: u64,
        target_payload_bits: u64,
        absolute_savings_bits: u64,
        relative_savings_bps: u16,
    },
}

impl RepresentationPayloadGainDecisionV1 {
    #[must_use]
    pub const fn is_transition_eligible(&self) -> bool {
        matches!(self, Self::EligibleTransition { .. })
    }

    /// This gate never carries transition or commit authority.
    #[must_use]
    pub const fn carries_authority(&self) -> bool {
        false
    }
}

/// Evaluate exact payload evidence and require explicit structural benefit.
///
/// Holds and ambiguous minima remain unchanged and do not enter the gain gate.
/// A unique lower-payload target is eligible only when both configured
/// thresholds are met.
///
/// # Errors
///
/// Returns base payload-decision errors unchanged, rejects a missing current
/// candidate, and fails closed if relative-savings arithmetic would overflow.
pub fn evaluate_representation_payload_gain_v1(
    current_profile_id: &str,
    candidates: impl IntoIterator<Item = RepresentationPayloadCandidateV1>,
    policy: RepresentationPayloadGainPolicyV1,
) -> Result<RepresentationPayloadGainDecisionV1, RepresentationPayloadGainError> {
    let candidates = candidates.into_iter().collect::<Vec<_>>();
    let current_payload_bits = candidates
        .iter()
        .find(|candidate| candidate.profile_id() == current_profile_id)
        .map(RepresentationPayloadCandidateV1::payload_bits)
        .ok_or_else(|| RepresentationPayloadGainError::CurrentProfileMissing {
            profile_id: current_profile_id.to_owned(),
        })?;

    let decision = evaluate_representation_payload_v1(current_profile_id, candidates)
        .map_err(RepresentationPayloadGainError::Decision)?;

    let RepresentationPayloadDecisionV1::UniqueTransitionCandidate {
        target,
        minimum_payload_bits,
        ..
    } = &decision
    else {
        return Ok(RepresentationPayloadGainDecisionV1::HoldBaseDecision { decision });
    };

    let target_payload_bits = target.payload_bits();
    debug_assert_eq!(target_payload_bits, *minimum_payload_bits);
    let absolute_savings_bits = current_payload_bits
        .checked_sub(target_payload_bits)
        .ok_or(RepresentationPayloadGainError::NonDecreasingTarget {
            current_payload_bits,
            target_payload_bits,
        })?;

    let relative_savings_bps = if current_payload_bits == 0 {
        0
    } else {
        let numerator = u128::from(absolute_savings_bits)
            .checked_mul(u128::from(MAX_REPRESENTATION_PAYLOAD_GAIN_BPS_V1))
            .ok_or(RepresentationPayloadGainError::RelativeSavingsOverflow)?;
        let basis_points = numerator / u128::from(current_payload_bits);
        u16::try_from(basis_points)
            .map_err(|_| RepresentationPayloadGainError::RelativeSavingsOverflow)?
    };

    let sufficient = absolute_savings_bits >= policy.minimum_absolute_savings_bits
        && relative_savings_bps >= policy.minimum_relative_savings_bps;

    if sufficient {
        Ok(RepresentationPayloadGainDecisionV1::EligibleTransition {
            decision,
            current_payload_bits,
            target_payload_bits,
            absolute_savings_bits,
            relative_savings_bps,
        })
    } else {
        Ok(RepresentationPayloadGainDecisionV1::InsufficientGain {
            decision,
            current_payload_bits,
            target_payload_bits,
            absolute_savings_bits,
            relative_savings_bps,
        })
    }
}

/// Fail-closed policy-construction errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepresentationPayloadGainPolicyError {
    RelativeThresholdOutOfRange { observed_bps: u16, maximum_bps: u16 },
}

impl fmt::Display for RepresentationPayloadGainPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RelativeThresholdOutOfRange {
                observed_bps,
                maximum_bps,
            } => write!(
                formatter,
                "representation payload relative-gain threshold {observed_bps} bps exceeds maximum {maximum_bps} bps"
            ),
        }
    }
}

impl std::error::Error for RepresentationPayloadGainPolicyError {}

/// Fail-closed gain-evaluation errors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RepresentationPayloadGainError {
    CurrentProfileMissing {
        profile_id: String,
    },
    Decision(RepresentationPayloadDecisionError),
    NonDecreasingTarget {
        current_payload_bits: u64,
        target_payload_bits: u64,
    },
    RelativeSavingsOverflow,
}

impl fmt::Display for RepresentationPayloadGainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CurrentProfileMissing { profile_id } => write!(
                formatter,
                "current representation profile {profile_id:?} is absent from gain-gate candidates"
            ),
            Self::Decision(error) => write!(formatter, "payload decision failed: {error}"),
            Self::NonDecreasingTarget {
                current_payload_bits,
                target_payload_bits,
            } => write!(
                formatter,
                "payload target is not smaller than current profile: current={current_payload_bits} target={target_payload_bits}"
            ),
            Self::RelativeSavingsOverflow => {
                formatter.write_str("relative representation payload savings overflowed")
            }
        }
    }
}

impl std::error::Error for RepresentationPayloadGainError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: &str, bits: u64) -> RepresentationPayloadCandidateV1 {
        RepresentationPayloadCandidateV1::new(id, bits).unwrap()
    }

    #[test]
    fn absolute_and_relative_thresholds_must_both_pass() {
        let policy = RepresentationPayloadGainPolicyV1::new(100, 1_000).unwrap();
        let result = evaluate_representation_payload_gain_v1(
            "dense",
            [candidate("dense", 1_000), candidate("hybrid", 850)],
            policy,
        )
        .unwrap();

        match result {
            RepresentationPayloadGainDecisionV1::EligibleTransition {
                absolute_savings_bits,
                relative_savings_bps,
                ..
            } => {
                assert_eq!(absolute_savings_bits, 150);
                assert_eq!(relative_savings_bps, 1_500);
            }
            other => panic!("expected eligible transition, observed {other:?}"),
        }
    }

    #[test]
    fn marginal_unique_minimum_can_be_held() {
        let policy = RepresentationPayloadGainPolicyV1::new(64, 500).unwrap();
        let result = evaluate_representation_payload_gain_v1(
            "dense",
            [candidate("dense", 1_000), candidate("hybrid", 960)],
            policy,
        )
        .unwrap();

        match result {
            RepresentationPayloadGainDecisionV1::InsufficientGain {
                absolute_savings_bits,
                relative_savings_bps,
                ..
            } => {
                assert_eq!(absolute_savings_bits, 40);
                assert_eq!(relative_savings_bps, 400);
            }
            other => panic!("expected insufficient gain, observed {other:?}"),
        }
    }

    #[test]
    fn ties_and_current_minima_bypass_gain_gate_without_switching() {
        let policy = RepresentationPayloadGainPolicyV1::new(1, 1).unwrap();

        let tied = evaluate_representation_payload_gain_v1(
            "sparse",
            [
                candidate("sparse", 1_000),
                candidate("dense", 500),
                candidate("hybrid", 500),
            ],
            policy,
        )
        .unwrap();
        assert!(matches!(
            tied,
            RepresentationPayloadGainDecisionV1::HoldBaseDecision {
                decision: RepresentationPayloadDecisionV1::AmbiguousMinimum { .. }
            }
        ));

        let current_minimum = evaluate_representation_payload_gain_v1(
            "dense",
            [candidate("dense", 500), candidate("hybrid", 600)],
            policy,
        )
        .unwrap();
        assert!(matches!(
            current_minimum,
            RepresentationPayloadGainDecisionV1::HoldBaseDecision {
                decision: RepresentationPayloadDecisionV1::HoldCurrentMinimum { .. }
            }
        ));
    }

    #[test]
    fn zero_threshold_preserves_unique_minimum_behavior() {
        let policy = RepresentationPayloadGainPolicyV1::new(0, 0).unwrap();
        let result = evaluate_representation_payload_gain_v1(
            "dense",
            [candidate("dense", 2), candidate("hybrid", 1)],
            policy,
        )
        .unwrap();
        assert!(result.is_transition_eligible());
        assert!(!result.carries_authority());
    }

    #[test]
    fn relative_threshold_is_bounded() {
        assert_eq!(
            RepresentationPayloadGainPolicyV1::new(0, 10_001),
            Err(
                RepresentationPayloadGainPolicyError::RelativeThresholdOutOfRange {
                    observed_bps: 10_001,
                    maximum_bps: 10_000,
                }
            )
        );
    }
}
