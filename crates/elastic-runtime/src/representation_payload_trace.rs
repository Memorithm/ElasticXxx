//! Typed, non-actuating trace for structural representation payload decisions.
//!
//! The trace preserves the current profile, exact minimum payload, the minimum
//! profile set, and the decision outcome. It does not carry transition,
//! stability, semantic-admissibility, or commit authority.

use crate::representation_payload_selector::RepresentationPayloadCandidateV1;
use crate::RepresentationPayloadDecisionV1;

/// Versioned identity of the structural payload decision trace.
pub const REPRESENTATION_PAYLOAD_DECISION_TRACE_V1: &str =
    "elastic.representation-payload-decision-trace@1.0.0";

/// Stable trace outcome class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepresentationPayloadDecisionOutcomeV1 {
    HoldCurrentMinimum,
    UniqueTransitionCandidate,
    AmbiguousMinimum,
}

/// Read-only evidence for one structural payload decision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepresentationPayloadDecisionTraceV1 {
    current_profile_id: String,
    outcome: RepresentationPayloadDecisionOutcomeV1,
    minimum_payload_bits: u64,
    minimum_profile_ids: Vec<String>,
    unique_target_profile_id: Option<String>,
}

impl RepresentationPayloadDecisionTraceV1 {
    /// Capture a trace without carrying any actuation authority.
    #[must_use]
    pub fn capture(decision: &RepresentationPayloadDecisionV1) -> Self {
        match decision {
            RepresentationPayloadDecisionV1::HoldCurrentMinimum {
                current_profile_id,
                minimum,
            } => Self {
                current_profile_id: current_profile_id.clone(),
                outcome: RepresentationPayloadDecisionOutcomeV1::HoldCurrentMinimum,
                minimum_payload_bits: minimum.minimum_payload_bits(),
                minimum_profile_ids: minimum
                    .profiles()
                    .iter()
                    .map(RepresentationPayloadCandidateV1::profile_id)
                    .map(str::to_owned)
                    .collect(),
                unique_target_profile_id: None,
            },
            RepresentationPayloadDecisionV1::UniqueTransitionCandidate {
                current_profile_id,
                target,
                minimum_payload_bits,
            } => Self {
                current_profile_id: current_profile_id.clone(),
                outcome: RepresentationPayloadDecisionOutcomeV1::UniqueTransitionCandidate,
                minimum_payload_bits: *minimum_payload_bits,
                minimum_profile_ids: vec![target.profile_id().to_owned()],
                unique_target_profile_id: Some(target.profile_id().to_owned()),
            },
            RepresentationPayloadDecisionV1::AmbiguousMinimum {
                current_profile_id,
                minimum,
            } => Self {
                current_profile_id: current_profile_id.clone(),
                outcome: RepresentationPayloadDecisionOutcomeV1::AmbiguousMinimum,
                minimum_payload_bits: minimum.minimum_payload_bits(),
                minimum_profile_ids: minimum
                    .profiles()
                    .iter()
                    .map(RepresentationPayloadCandidateV1::profile_id)
                    .map(str::to_owned)
                    .collect(),
                unique_target_profile_id: None,
            },
        }
    }

    #[must_use]
    pub fn current_profile_id(&self) -> &str {
        &self.current_profile_id
    }

    #[must_use]
    pub const fn outcome(&self) -> RepresentationPayloadDecisionOutcomeV1 {
        self.outcome
    }

    #[must_use]
    pub const fn minimum_payload_bits(&self) -> u64 {
        self.minimum_payload_bits
    }

    #[must_use]
    pub fn minimum_profile_ids(&self) -> &[String] {
        &self.minimum_profile_ids
    }

    #[must_use]
    pub fn unique_target_profile_id(&self) -> Option<&str> {
        self.unique_target_profile_id.as_deref()
    }

    /// Traces never contain transition or commit authority.
    #[must_use]
    pub const fn carries_authority(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evaluate_representation_payload_v1;
    use crate::representation_payload_selector::RepresentationPayloadCandidateV1;

    fn candidate(id: &str, bits: u64) -> RepresentationPayloadCandidateV1 {
        RepresentationPayloadCandidateV1::new(id, bits).unwrap()
    }

    #[test]
    fn hold_trace_preserves_tied_minimum_set() {
        let decision = evaluate_representation_payload_v1(
            "dense",
            [
                candidate("sparse", 1536),
                candidate("dense", 384),
                candidate("hybrid", 384),
            ],
        )
        .unwrap();

        let trace = RepresentationPayloadDecisionTraceV1::capture(&decision);
        assert_eq!(
            trace.outcome(),
            RepresentationPayloadDecisionOutcomeV1::HoldCurrentMinimum
        );
        assert_eq!(trace.current_profile_id(), "dense");
        assert_eq!(trace.minimum_payload_bits(), 384);
        assert_eq!(
            trace.minimum_profile_ids(),
            &["dense".to_owned(), "hybrid".to_owned()]
        );
        assert_eq!(trace.unique_target_profile_id(), None);
        assert!(!trace.carries_authority());
    }

    #[test]
    fn unique_transition_trace_retains_target_without_authority() {
        let decision = evaluate_representation_payload_v1(
            "dense",
            [
                candidate("sparse", 512),
                candidate("dense", 8192),
                candidate("hybrid", 4288),
            ],
        )
        .unwrap();

        let trace = RepresentationPayloadDecisionTraceV1::capture(&decision);
        assert_eq!(
            trace.outcome(),
            RepresentationPayloadDecisionOutcomeV1::UniqueTransitionCandidate
        );
        assert_eq!(trace.minimum_payload_bits(), 512);
        assert_eq!(trace.minimum_profile_ids(), &["sparse".to_owned()]);
        assert_eq!(trace.unique_target_profile_id(), Some("sparse"));
        assert!(!trace.carries_authority());
    }

    #[test]
    fn ambiguous_trace_retains_all_exact_minima() {
        let decision = evaluate_representation_payload_v1(
            "sparse",
            [
                candidate("sparse", 1536),
                candidate("dense", 384),
                candidate("hybrid", 384),
            ],
        )
        .unwrap();

        let trace = RepresentationPayloadDecisionTraceV1::capture(&decision);
        assert_eq!(
            trace.outcome(),
            RepresentationPayloadDecisionOutcomeV1::AmbiguousMinimum
        );
        assert_eq!(
            trace.minimum_profile_ids(),
            &["dense".to_owned(), "hybrid".to_owned()]
        );
        assert_eq!(trace.unique_target_profile_id(), None);
    }
}
