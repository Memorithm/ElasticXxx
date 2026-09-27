//! Non-actuating ElasticWord width selection.
//!
//! This module selects the narrowest declared ElasticWord width that satisfies
//! a caller-provided semantic floor and an explicit maximum-width budget.
//! Selection is deliberately separate from transition stability, validation,
//! actuation, verification, commit and rollback.
//!
//! In particular, this module does not implement its own cooldown/hysteresis.
//! Callers compose a contraction recommendation with the existing
//! `TransitionStabilityPolicyV1` machinery before any physical change.

use elastic_core::ElasticWordWidthV1;
use std::fmt;

/// Versioned identity of the first generic ElasticWord width selector.
pub const ELASTIC_WORD_WIDTH_SELECTOR_V1: &str = "elastic.word-width-selector@1.0.0";

/// Maximum width candidates admitted by ElasticWord v1.
pub const MAX_ELASTIC_WORD_WIDTH_CANDIDATES_V1: usize = 6;

/// Direction of a selected representation-width change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElasticWordWidthDirectionV1 {
    Expand,
    Contract,
}

impl ElasticWordWidthDirectionV1 {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Expand => "expand",
            Self::Contract => "contract",
        }
    }
}

/// Pure planning outcome.
///
/// This value never authorizes mutation. A transition result still requires
/// normal stability, capability, invariant, trusted-adapter and transaction
/// checks before any actuation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElasticWordWidthSelectionV1 {
    Hold {
        width: ElasticWordWidthV1,
    },
    Transition {
        from: ElasticWordWidthV1,
        to: ElasticWordWidthV1,
        direction: ElasticWordWidthDirectionV1,
    },
    Blocked {
        required_floor: ElasticWordWidthV1,
        maximum_budget: ElasticWordWidthV1,
        reason: ElasticWordWidthBlockReasonV1,
    },
}

/// Fail-closed reason for a planning-only blocked outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElasticWordWidthBlockReasonV1 {
    FloorExceedsBudget,
    NoDeclaredCandidate,
}

impl ElasticWordWidthBlockReasonV1 {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FloorExceedsBudget => "floor-exceeds-budget",
            Self::NoDeclaredCandidate => "no-declared-candidate",
        }
    }
}

/// Deterministic declared-candidate selector.
///
/// Candidate order supplied by the caller is not semantic: construction sorts
/// by width and rejects duplicates. Selection always chooses the narrowest
/// declared width that is at least the semantic floor and at most the explicit
/// width budget.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElasticWordWidthSelectorV1 {
    candidates: Vec<ElasticWordWidthV1>,
}

impl ElasticWordWidthSelectorV1 {
    pub fn new(
        mut candidates: Vec<ElasticWordWidthV1>,
    ) -> Result<Self, ElasticWordWidthSelectorError> {
        if candidates.is_empty() {
            return Err(ElasticWordWidthSelectorError::EmptyCandidateSet);
        }
        if candidates.len() > MAX_ELASTIC_WORD_WIDTH_CANDIDATES_V1 {
            return Err(ElasticWordWidthSelectorError::TooManyCandidates {
                observed: candidates.len(),
                maximum: MAX_ELASTIC_WORD_WIDTH_CANDIDATES_V1,
            });
        }

        candidates.sort_unstable();
        for pair in candidates.windows(2) {
            if pair[0] == pair[1] {
                return Err(ElasticWordWidthSelectorError::DuplicateCandidate {
                    bits: pair[0].bits(),
                });
            }
        }

        Ok(Self { candidates })
    }

    /// Build the complete ElasticWord v1 width envelope.
    pub fn full_v1() -> Self {
        let candidates = [64_u16, 128, 256, 512, 1024, 2048]
            .into_iter()
            .map(|bits| {
                ElasticWordWidthV1::from_bits(bits)
                    .expect("ElasticWord v1 frozen width is structurally valid")
            })
            .collect();
        Self { candidates }
    }

    #[must_use]
    pub fn candidates(&self) -> &[ElasticWordWidthV1] {
        &self.candidates
    }

    /// Select one admissible target without mutating state.
    ///
    /// `required_floor` is domain-owned semantic evidence. `maximum_budget`
    /// is an explicit width ceiling. This selector does not infer either value.
    pub fn select(
        &self,
        current: ElasticWordWidthV1,
        required_floor: ElasticWordWidthV1,
        maximum_budget: ElasticWordWidthV1,
    ) -> ElasticWordWidthSelectionV1 {
        if required_floor > maximum_budget {
            return ElasticWordWidthSelectionV1::Blocked {
                required_floor,
                maximum_budget,
                reason: ElasticWordWidthBlockReasonV1::FloorExceedsBudget,
            };
        }

        let Some(target) = self
            .candidates
            .iter()
            .copied()
            .find(|candidate| *candidate >= required_floor && *candidate <= maximum_budget)
        else {
            return ElasticWordWidthSelectionV1::Blocked {
                required_floor,
                maximum_budget,
                reason: ElasticWordWidthBlockReasonV1::NoDeclaredCandidate,
            };
        };

        if target == current {
            ElasticWordWidthSelectionV1::Hold { width: current }
        } else {
            let direction = if target > current {
                ElasticWordWidthDirectionV1::Expand
            } else {
                ElasticWordWidthDirectionV1::Contract
            };
            ElasticWordWidthSelectionV1::Transition {
                from: current,
                to: target,
                direction,
            }
        }
    }
}

/// Selector configuration errors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ElasticWordWidthSelectorError {
    EmptyCandidateSet,
    TooManyCandidates { observed: usize, maximum: usize },
    DuplicateCandidate { bits: u16 },
}

impl fmt::Display for ElasticWordWidthSelectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyCandidateSet => {
                f.write_str("ElasticWord width selector requires at least one candidate")
            }
            Self::TooManyCandidates { observed, maximum } => write!(
                f,
                "ElasticWord width selector received {observed} candidates; maximum is {maximum}"
            ),
            Self::DuplicateCandidate { bits } => {
                write!(f, "ElasticWord width selector contains duplicate W{bits}")
            }
        }
    }
}

impl std::error::Error for ElasticWordWidthSelectorError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn width(bits: u16) -> ElasticWordWidthV1 {
        ElasticWordWidthV1::from_bits(bits).unwrap()
    }

    #[test]
    fn full_v1_contains_the_frozen_width_envelope() {
        let selector = ElasticWordWidthSelectorV1::full_v1();
        assert_eq!(
            selector
                .candidates()
                .iter()
                .map(|width| width.bits())
                .collect::<Vec<_>>(),
            vec![64, 128, 256, 512, 1024, 2048]
        );
    }

    #[test]
    fn chooses_narrowest_declared_width_at_or_above_floor() {
        let selector = ElasticWordWidthSelectorV1::full_v1();
        assert_eq!(
            selector.select(width(512), width(128), width(1024)),
            ElasticWordWidthSelectionV1::Transition {
                from: width(512),
                to: width(128),
                direction: ElasticWordWidthDirectionV1::Contract,
            }
        );
    }

    #[test]
    fn expansion_is_selected_when_current_is_below_semantic_floor() {
        let selector = ElasticWordWidthSelectorV1::full_v1();
        assert_eq!(
            selector.select(width(64), width(512), width(2048)),
            ElasticWordWidthSelectionV1::Transition {
                from: width(64),
                to: width(512),
                direction: ElasticWordWidthDirectionV1::Expand,
            }
        );
    }

    #[test]
    fn exact_width_holds() {
        let selector = ElasticWordWidthSelectorV1::full_v1();
        assert_eq!(
            selector.select(width(256), width(256), width(512)),
            ElasticWordWidthSelectionV1::Hold { width: width(256) }
        );
    }

    #[test]
    fn floor_above_budget_fails_closed() {
        let selector = ElasticWordWidthSelectorV1::full_v1();
        assert_eq!(
            selector.select(width(128), width(512), width(256)),
            ElasticWordWidthSelectionV1::Blocked {
                required_floor: width(512),
                maximum_budget: width(256),
                reason: ElasticWordWidthBlockReasonV1::FloorExceedsBudget,
            }
        );
    }

    #[test]
    fn candidate_gaps_fail_closed_instead_of_inventing_a_width() {
        let selector =
            ElasticWordWidthSelectorV1::new(vec![width(64), width(128), width(1024)]).unwrap();
        assert_eq!(
            selector.select(width(128), width(256), width(512)),
            ElasticWordWidthSelectionV1::Blocked {
                required_floor: width(256),
                maximum_budget: width(512),
                reason: ElasticWordWidthBlockReasonV1::NoDeclaredCandidate,
            }
        );
    }

    #[test]
    fn candidate_order_is_canonicalized_and_duplicates_are_rejected() {
        let selector =
            ElasticWordWidthSelectorV1::new(vec![width(512), width(64), width(128)]).unwrap();
        assert_eq!(
            selector
                .candidates()
                .iter()
                .map(|width| width.bits())
                .collect::<Vec<_>>(),
            vec![64, 128, 512]
        );

        assert_eq!(
            ElasticWordWidthSelectorV1::new(vec![width(64), width(64)]),
            Err(ElasticWordWidthSelectorError::DuplicateCandidate { bits: 64 })
        );
    }

    #[test]
    fn selection_is_independent_of_transition_stability_and_actuation() {
        let selector = ElasticWordWidthSelectorV1::full_v1();
        let decision = selector.select(width(512), width(64), width(2048));
        assert!(matches!(
            decision,
            ElasticWordWidthSelectionV1::Transition {
                direction: ElasticWordWidthDirectionV1::Contract,
                ..
            }
        ));
        // A caller must still submit the recommendation through the existing
        // TransitionStabilityPolicyV1 + transactional validation path.
    }
}
