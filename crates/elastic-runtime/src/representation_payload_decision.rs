//! Planning-only interpretation of structural representation payload minima.
//!
//! This module decides whether the current profile is already structurally
//! minimal, whether exactly one lower-payload target exists, or whether an exact
//! tie must remain ambiguous. It never grants transition or actuation authority.

use crate::{
    select_minimum_payload_v1, RepresentationPayloadCandidateV1, RepresentationPayloadMinimumV1,
    RepresentationPayloadSelectorError,
};
use std::fmt;

/// Versioned identity of the structural payload decision contract.
pub const REPRESENTATION_PAYLOAD_DECISION_V1: &str =
    "elastic.representation-payload-decision@1.0.0";

/// Planning-only interpretation of one exact structural minimum set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RepresentationPayloadDecisionV1 {
    /// The current profile is already one of the exact minima.
    HoldCurrentMinimum {
        current_profile_id: String,
        minimum: RepresentationPayloadMinimumV1,
    },
    /// Exactly one minimum exists and it is not the current profile.
    UniqueTransitionCandidate {
        current_profile_id: String,
        target: RepresentationPayloadCandidateV1,
        minimum_payload_bits: u64,
    },
    /// Multiple exact minima exist and the current profile is not among them.
    AmbiguousMinimum {
        current_profile_id: String,
        minimum: RepresentationPayloadMinimumV1,
    },
}

impl RepresentationPayloadDecisionV1 {
    /// Whether the structural evidence by itself recommends no profile change.
    #[must_use]
    pub const fn is_hold(&self) -> bool {
        matches!(self, Self::HoldCurrentMinimum { .. })
    }

    /// Unique structural target, when the minimum is unambiguous.
    #[must_use]
    pub fn unique_target(&self) -> Option<&RepresentationPayloadCandidateV1> {
        match self {
            Self::UniqueTransitionCandidate { target, .. } => Some(target),
            Self::HoldCurrentMinimum { .. } | Self::AmbiguousMinimum { .. } => None,
        }
    }

    /// This planning object never carries transition or commit authority.
    #[must_use]
    pub const fn carries_authority(&self) -> bool {
        false
    }
}

/// Evaluate one current profile against a bounded structural candidate set.
///
/// Exact ties are preserved. If the current profile participates in a tie, the
/// result is a hold rather than an arbitrary switch between equivalent minima.
///
/// # Errors
///
/// Returns selector validation errors unchanged and rejects a current profile
/// that is absent from the supplied candidate set.
pub fn evaluate_representation_payload_v1(
    current_profile_id: &str,
    candidates: impl IntoIterator<Item = RepresentationPayloadCandidateV1>,
) -> Result<RepresentationPayloadDecisionV1, RepresentationPayloadDecisionError> {
    let candidates = candidates.into_iter().collect::<Vec<_>>();

    if !candidates
        .iter()
        .any(|candidate| candidate.profile_id() == current_profile_id)
    {
        return Err(RepresentationPayloadDecisionError::CurrentProfileMissing {
            profile_id: current_profile_id.to_owned(),
        });
    }

    let minimum =
        select_minimum_payload_v1(candidates).map_err(RepresentationPayloadDecisionError::Selector)?;
    let current_is_minimum = minimum
        .profiles()
        .iter()
        .any(|candidate| candidate.profile_id() == current_profile_id);

    if current_is_minimum {
        return Ok(RepresentationPayloadDecisionV1::HoldCurrentMinimum {
            current_profile_id: current_profile_id.to_owned(),
            minimum,
        });
    }

    if minimum.is_unique() {
        return Ok(RepresentationPayloadDecisionV1::UniqueTransitionCandidate {
            current_profile_id: current_profile_id.to_owned(),
            target: minimum.profiles()[0].clone(),
            minimum_payload_bits: minimum.minimum_payload_bits(),
        });
    }

    Ok(RepresentationPayloadDecisionV1::AmbiguousMinimum {
        current_profile_id: current_profile_id.to_owned(),
        minimum,
    })
}

/// Fail-closed payload-decision errors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RepresentationPayloadDecisionError {
    CurrentProfileMissing { profile_id: String },
    Selector(RepresentationPayloadSelectorError),
}

impl fmt::Display for RepresentationPayloadDecisionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CurrentProfileMissing { profile_id } => write!(
                formatter,
                "current representation profile {profile_id:?} is absent from the candidate set"
            ),
            Self::Selector(error) => write!(formatter, "payload selector failed: {error}"),
        }
    }
}

impl std::error::Error for RepresentationPayloadDecisionError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: &str, bits: u64) -> RepresentationPayloadCandidateV1 {
        RepresentationPayloadCandidateV1::new(id, bits).unwrap()
    }

    #[test]
    fn unique_current_minimum_holds() {
        let decision = evaluate_representation_payload_v1(
            "sparse",
            [
                candidate("sparse", 512),
                candidate("dense", 8192),
                candidate("hybrid", 4288),
            ],
        )
        .unwrap();

        assert!(decision.is_hold());
        assert!(decision.unique_target().is_none());
        assert!(!decision.carries_authority());
    }

    #[test]
    fn current_member_of_exact_tie_holds() {
        let decision = evaluate_representation_payload_v1(
            "dense",
            [
                candidate("sparse", 1536),
                candidate("dense", 384),
                candidate("hybrid", 384),
            ],
        )
        .unwrap();

        match decision {
            RepresentationPayloadDecisionV1::HoldCurrentMinimum { minimum, .. } => {
                assert_eq!(minimum.profiles().len(), 2);
            }
            other => panic!("expected hold on tied minimum, observed {other:?}"),
        }
    }

    #[test]
    fn unique_noncurrent_minimum_yields_one_transition_candidate() {
        let decision = evaluate_representation_payload_v1(
            "dense",
            [
                candidate("sparse", 512),
                candidate("dense", 8192),
                candidate("hybrid", 4288),
            ],
        )
        .unwrap();

        let target = decision.unique_target().unwrap();
        assert_eq!(target.profile_id(), "sparse");
        assert_eq!(target.payload_bits(), 512);
        assert!(!decision.carries_authority());
    }

    #[test]
    fn noncurrent_exact_tie_stays_ambiguous() {
        let decision = evaluate_representation_payload_v1(
            "sparse",
            [
                candidate("sparse", 1536),
                candidate("dense", 384),
                candidate("hybrid", 384),
            ],
        )
        .unwrap();

        match decision {
            RepresentationPayloadDecisionV1::AmbiguousMinimum { minimum, .. } => {
                assert_eq!(
                    minimum
                        .profiles()
                        .iter()
                        .map(RepresentationPayloadCandidateV1::profile_id)
                        .collect::<Vec<_>>(),
                    vec!["dense", "hybrid"]
                );
            }
            other => panic!("expected ambiguity, observed {other:?}"),
        }
    }

    #[test]
    fn missing_current_profile_fails_closed() {
        assert_eq!(
            evaluate_representation_payload_v1(
                "missing",
                [candidate("dense", 128), candidate("hybrid", 64)]
            ),
            Err(RepresentationPayloadDecisionError::CurrentProfileMissing {
                profile_id: "missing".to_owned(),
            })
        );
    }
}
