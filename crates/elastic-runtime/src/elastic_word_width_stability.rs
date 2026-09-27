//! Composition of ElasticWord width planning with transition stability.
//!
//! The width selector remains pure planning. This controller adds only the
//! existing anti-thrashing admission gate. Even an admitted outcome is not an
//! actuation capability: callers must still perform capability/invariant
//! validation and trusted transactional actuation.

use crate::{
    ElasticWordWidthBlockReasonV1, ElasticWordWidthDirectionV1, ElasticWordWidthSelectionV1,
    ElasticWordWidthSelectorV1, ObservationSnapshot, TransitionStabilityError,
    TransitionStabilityGateV1, TransitionStabilityPermitV1, TransitionStabilityPolicyV1,
    TransitionStabilityReportV1,
};
use elastic_core::{DimensionId, ElasticWordWidthV1, TransitionMechanism};
use std::fmt;
use std::time::Instant;

/// Versioned identity of the planning + stability composition.
pub const ELASTIC_WORD_WIDTH_STABILITY_COMPOSITION_V1: &str =
    "elastic.word-width-stability@1.0.0";

/// Non-actuating composed outcome.
#[derive(Debug)]
pub enum ElasticWordWidthStableSelectionV1 {
    Hold {
        width: ElasticWordWidthV1,
    },
    Blocked {
        required_floor: ElasticWordWidthV1,
        maximum_budget: ElasticWordWidthV1,
        reason: ElasticWordWidthBlockReasonV1,
    },
    Deferred {
        from: ElasticWordWidthV1,
        to: ElasticWordWidthV1,
        direction: ElasticWordWidthDirectionV1,
        stability: TransitionStabilityReportV1,
    },
    Admitted {
        from: ElasticWordWidthV1,
        to: ElasticWordWidthV1,
        direction: ElasticWordWidthDirectionV1,
        stability: TransitionStabilityReportV1,
        permit: TransitionStabilityPermitV1,
    },
}

impl ElasticWordWidthStableSelectionV1 {
    #[must_use]
    pub const fn target(&self) -> Option<ElasticWordWidthV1> {
        match self {
            Self::Hold { width } => Some(*width),
            Self::Blocked { .. } => None,
            Self::Deferred { to, .. } | Self::Admitted { to, .. } => Some(*to),
        }
    }

    #[must_use]
    pub const fn is_admitted_transition(&self) -> bool {
        matches!(self, Self::Admitted { .. })
    }
}

/// Stateful composition of pure width selection and the existing stability gate.
#[derive(Debug)]
pub struct ElasticWordWidthStabilityControllerV1 {
    selector: ElasticWordWidthSelectorV1,
    gate: TransitionStabilityGateV1,
}

impl ElasticWordWidthStabilityControllerV1 {
    pub fn new(
        selector: ElasticWordWidthSelectorV1,
        stability_policy: TransitionStabilityPolicyV1,
    ) -> Result<Self, ElasticWordWidthStabilityError> {
        if stability_policy.mechanism() != TransitionMechanism::Reencode {
            return Err(ElasticWordWidthStabilityError::MechanismMismatch {
                observed: stability_policy.mechanism(),
            });
        }
        if stability_policy.dimension() != &DimensionId::REPRESENTATION {
            return Err(ElasticWordWidthStabilityError::DimensionMismatch {
                observed: stability_policy.dimension().clone(),
            });
        }
        Ok(Self {
            selector,
            gate: TransitionStabilityGateV1::new(stability_policy),
        })
    }

    #[must_use]
    pub const fn selector(&self) -> &ElasticWordWidthSelectorV1 {
        &self.selector
    }

    #[must_use]
    pub const fn stability_gate(&self) -> &TransitionStabilityGateV1 {
        &self.gate
    }

    /// Select a width and, only for an actual transition, apply anti-thrashing
    /// admission against the supplied observation snapshot.
    ///
    /// A semantic-floor or budget block does not consume stability state. A
    /// hold decision also leaves the stability gate untouched.
    pub fn select(
        &mut self,
        current: ElasticWordWidthV1,
        required_floor: ElasticWordWidthV1,
        maximum_budget: ElasticWordWidthV1,
        observations: &ObservationSnapshot,
        now: Instant,
    ) -> ElasticWordWidthStableSelectionV1 {
        match self
            .selector
            .select(current, required_floor, maximum_budget)
        {
            ElasticWordWidthSelectionV1::Hold { width } => {
                ElasticWordWidthStableSelectionV1::Hold { width }
            }
            ElasticWordWidthSelectionV1::Blocked {
                required_floor,
                maximum_budget,
                reason,
            } => ElasticWordWidthStableSelectionV1::Blocked {
                required_floor,
                maximum_budget,
                reason,
            },
            ElasticWordWidthSelectionV1::Transition {
                from,
                to,
                direction,
            } => {
                let (stability, permit) = self.gate.check(observations, now);
                match permit {
                    Some(permit) => ElasticWordWidthStableSelectionV1::Admitted {
                        from,
                        to,
                        direction,
                        stability,
                        permit,
                    },
                    None => ElasticWordWidthStableSelectionV1::Deferred {
                        from,
                        to,
                        direction,
                        stability,
                    },
                }
            }
        }
    }

    /// Record stability state only after the caller has actually committed the
    /// independently validated physical transition.
    pub fn record_commit(
        &mut self,
        permit: TransitionStabilityPermitV1,
        committed_at: Instant,
    ) -> Result<(), TransitionStabilityError> {
        self.gate.record_commit(permit, committed_at)
    }
}

/// Construction failures for width/stability composition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ElasticWordWidthStabilityError {
    MechanismMismatch { observed: TransitionMechanism },
    DimensionMismatch { observed: DimensionId },
}

impl fmt::Display for ElasticWordWidthStabilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MechanismMismatch { observed } => write!(
                f,
                "ElasticWord width stability requires Reencode mechanism, observed {observed:?}"
            ),
            Self::DimensionMismatch { observed } => write!(
                f,
                "ElasticWord width stability requires representation dimension, observed {}",
                observed.as_str()
            ),
        }
    }
}

impl std::error::Error for ElasticWordWidthStabilityError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TransitionRateLimitV1, TransitionStabilityStatusV1};
    use std::time::Duration;

    fn width(bits: u16) -> ElasticWordWidthV1 {
        ElasticWordWidthV1::from_bits(bits).unwrap()
    }

    fn cooldown_policy() -> TransitionStabilityPolicyV1 {
        TransitionStabilityPolicyV1::new(
            TransitionMechanism::Reencode,
            DimensionId::REPRESENTATION,
            None,
            Some(Duration::from_secs(10)),
            None,
        )
        .unwrap()
    }

    #[test]
    fn hold_and_width_block_do_not_touch_stability_generation() {
        let mut controller = ElasticWordWidthStabilityControllerV1::new(
            ElasticWordWidthSelectorV1::full_v1(),
            cooldown_policy(),
        )
        .unwrap();
        let now = Instant::now();
        let empty = ObservationSnapshot::new(now, vec![]);

        assert!(matches!(
            controller.select(width(128), width(128), width(512), &empty, now),
            ElasticWordWidthStableSelectionV1::Hold { .. }
        ));
        assert!(matches!(
            controller.select(width(128), width(512), width(256), &empty, now),
            ElasticWordWidthStableSelectionV1::Blocked {
                reason: ElasticWordWidthBlockReasonV1::FloorExceedsBudget,
                ..
            }
        ));
        assert_eq!(controller.stability_gate().generation(), 0);
    }

    #[test]
    fn committed_transition_activates_existing_cooldown_for_next_width_change() {
        let mut controller = ElasticWordWidthStabilityControllerV1::new(
            ElasticWordWidthSelectorV1::full_v1(),
            cooldown_policy(),
        )
        .unwrap();
        let start = Instant::now();
        let empty = ObservationSnapshot::new(start, vec![]);

        let first = controller.select(width(512), width(64), width(2048), &empty, start);
        let permit = match first {
            ElasticWordWidthStableSelectionV1::Admitted {
                direction: ElasticWordWidthDirectionV1::Contract,
                permit,
                ..
            } => permit,
            other => panic!("expected admitted contraction, observed {other:?}"),
        };
        controller.record_commit(permit, start).unwrap();

        let blocked_at = start + Duration::from_secs(5);
        let blocked_snapshot = ObservationSnapshot::new(blocked_at, vec![]);
        let deferred = controller.select(
            width(64),
            width(128),
            width(2048),
            &blocked_snapshot,
            blocked_at,
        );
        match deferred {
            ElasticWordWidthStableSelectionV1::Deferred {
                direction: ElasticWordWidthDirectionV1::Expand,
                stability,
                ..
            } => assert_eq!(stability.status, TransitionStabilityStatusV1::CooldownActive),
            other => panic!("expected deferred expansion, observed {other:?}"),
        }

        let eligible_at = start + Duration::from_secs(11);
        let eligible_snapshot = ObservationSnapshot::new(eligible_at, vec![]);
        assert!(matches!(
            controller.select(
                width(64),
                width(128),
                width(2048),
                &eligible_snapshot,
                eligible_at,
            ),
            ElasticWordWidthStableSelectionV1::Admitted {
                direction: ElasticWordWidthDirectionV1::Expand,
                ..
            }
        ));
    }

    #[test]
    fn rate_limit_is_reused_without_width_specific_reimplementation() {
        let policy = TransitionStabilityPolicyV1::new(
            TransitionMechanism::Reencode,
            DimensionId::REPRESENTATION,
            None,
            None,
            Some(TransitionRateLimitV1::new(1, Duration::from_secs(60)).unwrap()),
        )
        .unwrap();
        let mut controller = ElasticWordWidthStabilityControllerV1::new(
            ElasticWordWidthSelectorV1::full_v1(),
            policy,
        )
        .unwrap();
        let start = Instant::now();
        let empty = ObservationSnapshot::new(start, vec![]);
        let first = controller.select(width(128), width(64), width(2048), &empty, start);
        let permit = match first {
            ElasticWordWidthStableSelectionV1::Admitted { permit, .. } => permit,
            other => panic!("expected first permit, observed {other:?}"),
        };
        controller.record_commit(permit, start).unwrap();

        let later = start + Duration::from_secs(1);
        let snapshot = ObservationSnapshot::new(later, vec![]);
        let second = controller.select(width(64), width(128), width(2048), &snapshot, later);
        match second {
            ElasticWordWidthStableSelectionV1::Deferred { stability, .. } => {
                assert_eq!(stability.status, TransitionStabilityStatusV1::RateLimited);
            }
            other => panic!("expected rate-limited transition, observed {other:?}"),
        }
    }

    #[test]
    fn wrong_transition_class_is_rejected_at_composition_boundary() {
        let wrong_mechanism = TransitionStabilityPolicyV1::new(
            TransitionMechanism::Reinterpret,
            DimensionId::REPRESENTATION,
            None,
            Some(Duration::from_secs(1)),
            None,
        )
        .unwrap();
        assert!(matches!(
            ElasticWordWidthStabilityControllerV1::new(
                ElasticWordWidthSelectorV1::full_v1(),
                wrong_mechanism
            ),
            Err(ElasticWordWidthStabilityError::MechanismMismatch { .. })
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
            ElasticWordWidthStabilityControllerV1::new(
                ElasticWordWidthSelectorV1::full_v1(),
                wrong_dimension
            ),
            Err(ElasticWordWidthStabilityError::DimensionMismatch { .. })
        ));
    }
}
