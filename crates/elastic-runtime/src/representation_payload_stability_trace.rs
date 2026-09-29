//! Typed read-only evidence for structural payload decisions after stability gating.
//!
//! This trace retains the planning decision plus copied stability evidence.
//! It never carries the single-use stability permit itself and cannot authorize
//! an actuation or commit.

use crate::{
    RepresentationPayloadDecisionTraceV1, RepresentationPayloadStableDecisionV1,
    TransitionStabilityReportV1,
};

/// Versioned identity of the stable payload-decision trace.
pub const REPRESENTATION_PAYLOAD_STABILITY_TRACE_V1: &str =
    "elastic.representation-payload-stability-trace@1.0.0";

/// Stable outcome class after the generic transition-stability gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepresentationPayloadStabilityOutcomeV1 {
    Hold,
    Ambiguous,
    Deferred,
    Admitted,
}

/// Read-only trace of one structural payload decision after stability gating.
#[derive(Clone, Debug, PartialEq)]
pub struct RepresentationPayloadStabilityTraceV1 {
    decision: RepresentationPayloadDecisionTraceV1,
    outcome: RepresentationPayloadStabilityOutcomeV1,
    stability: Option<TransitionStabilityReportV1>,
    permit_generation: Option<u64>,
}

impl RepresentationPayloadStabilityTraceV1 {
    /// Capture a stable-decision trace without copying permit authority.
    #[must_use]
    pub fn capture(decision: &RepresentationPayloadStableDecisionV1) -> Self {
        match decision {
            RepresentationPayloadStableDecisionV1::Hold { decision } => Self {
                decision: RepresentationPayloadDecisionTraceV1::capture(decision),
                outcome: RepresentationPayloadStabilityOutcomeV1::Hold,
                stability: None,
                permit_generation: None,
            },
            RepresentationPayloadStableDecisionV1::Ambiguous { decision } => Self {
                decision: RepresentationPayloadDecisionTraceV1::capture(decision),
                outcome: RepresentationPayloadStabilityOutcomeV1::Ambiguous,
                stability: None,
                permit_generation: None,
            },
            RepresentationPayloadStableDecisionV1::Deferred {
                decision,
                stability,
            } => Self {
                decision: RepresentationPayloadDecisionTraceV1::capture(decision),
                outcome: RepresentationPayloadStabilityOutcomeV1::Deferred,
                stability: Some(stability.clone()),
                permit_generation: None,
            },
            RepresentationPayloadStableDecisionV1::Admitted {
                decision,
                stability,
                permit,
            } => Self {
                decision: RepresentationPayloadDecisionTraceV1::capture(decision),
                outcome: RepresentationPayloadStabilityOutcomeV1::Admitted,
                stability: Some(stability.clone()),
                permit_generation: Some(permit.generation()),
            },
        }
    }

    #[must_use]
    pub const fn outcome(&self) -> RepresentationPayloadStabilityOutcomeV1 {
        self.outcome
    }

    #[must_use]
    pub const fn decision(&self) -> &RepresentationPayloadDecisionTraceV1 {
        &self.decision
    }

    #[must_use]
    pub const fn stability(&self) -> Option<&TransitionStabilityReportV1> {
        self.stability.as_ref()
    }

    #[must_use]
    pub const fn permit_generation(&self) -> Option<u64> {
        self.permit_generation
    }

    /// The trace never carries transition or commit authority.
    #[must_use]
    pub const fn carries_authority(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ObservationSnapshot, RepresentationPayloadStabilityControllerV1,
        TransitionStabilityPolicyV1,
    };
    use crate::representation_payload_selector::RepresentationPayloadCandidateV1;
    use elastic_core::{DimensionId, TransitionMechanism};
    use std::time::{Duration, Instant};

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
    fn admitted_trace_keeps_permit_generation_but_not_authority() {
        let now = Instant::now();
        let observations = ObservationSnapshot::new(now, vec![]);
        let mut controller = controller();
        let decision = controller
            .evaluate(
                "dense",
                [
                    candidate("dense", 8192),
                    candidate("hybrid", 4288),
                    candidate("sparse", 512),
                ],
                &observations,
                now,
            )
            .unwrap();

        let trace = RepresentationPayloadStabilityTraceV1::capture(&decision);
        assert_eq!(
            trace.outcome(),
            RepresentationPayloadStabilityOutcomeV1::Admitted
        );
        assert_eq!(trace.permit_generation(), Some(0));
        assert!(trace.stability().is_some());
        assert_eq!(trace.decision().unique_target_profile_id(), Some("sparse"));
        assert!(!trace.carries_authority());
    }

    #[test]
    fn hold_and_ambiguity_have_no_stability_or_permit() {
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
        let hold_trace = RepresentationPayloadStabilityTraceV1::capture(&hold);
        assert_eq!(
            hold_trace.outcome(),
            RepresentationPayloadStabilityOutcomeV1::Hold
        );
        assert!(hold_trace.stability().is_none());
        assert_eq!(hold_trace.permit_generation(), None);

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
        let ambiguous_trace = RepresentationPayloadStabilityTraceV1::capture(&ambiguous);
        assert_eq!(
            ambiguous_trace.outcome(),
            RepresentationPayloadStabilityOutcomeV1::Ambiguous
        );
        assert!(ambiguous_trace.stability().is_none());
        assert_eq!(ambiguous_trace.permit_generation(), None);
    }

    #[test]
    fn deferred_trace_retains_copied_stability_evidence() {
        let start = Instant::now();
        let observations = ObservationSnapshot::new(start, vec![]);
        let mut controller = controller();

        let first = controller
            .evaluate(
                "dense",
                [candidate("dense", 512), candidate("sparse", 128)],
                &observations,
                start,
            )
            .unwrap();
        let permit = match first {
            RepresentationPayloadStableDecisionV1::Admitted { permit, .. } => permit,
            other => panic!("expected admitted decision, observed {other:?}"),
        };
        controller.record_commit(permit, start).unwrap();

        let later = start + Duration::from_secs(1);
        let later_observations = ObservationSnapshot::new(later, vec![]);
        let deferred = controller
            .evaluate(
                "sparse",
                [candidate("sparse", 512), candidate("dense", 128)],
                &later_observations,
                later,
            )
            .unwrap();

        let trace = RepresentationPayloadStabilityTraceV1::capture(&deferred);
        assert_eq!(
            trace.outcome(),
            RepresentationPayloadStabilityOutcomeV1::Deferred
        );
        assert!(trace.stability().is_some());
        assert_eq!(trace.permit_generation(), None);
        assert!(!trace.carries_authority());
    }
}
