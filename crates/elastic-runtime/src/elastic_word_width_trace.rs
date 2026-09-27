//! Typed, non-actuating evidence for one ElasticWord width planning decision.
//!
//! The trace copies the planning/stability explanation while deliberately
//! excluding the single-use permit itself. It can therefore be inspected
//! without gaining commit or actuation authority.

use crate::{
    ElasticWordWidthBlockReasonV1, ElasticWordWidthDirectionV1, ElasticWordWidthStableSelectionV1,
    TransitionStabilityReportV1,
};
use elastic_core::ElasticWordWidthV1;

/// Versioned identity of the typed width-decision trace.
pub const ELASTIC_WORD_WIDTH_DECISION_TRACE_V1: &str = "elastic.word-width-decision-trace@1.0.0";

/// Stable outcome class retained by the trace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElasticWordWidthDecisionOutcomeV1 {
    Hold,
    Blocked,
    Deferred,
    Admitted,
}

/// Read-only evidence for one width decision.
#[derive(Clone, Debug, PartialEq)]
pub struct ElasticWordWidthDecisionTraceV1 {
    current: ElasticWordWidthV1,
    required_floor: ElasticWordWidthV1,
    maximum_budget: ElasticWordWidthV1,
    target: Option<ElasticWordWidthV1>,
    outcome: ElasticWordWidthDecisionOutcomeV1,
    direction: Option<ElasticWordWidthDirectionV1>,
    block_reason: Option<ElasticWordWidthBlockReasonV1>,
    stability: Option<TransitionStabilityReportV1>,
    permit_generation: Option<u64>,
}

impl ElasticWordWidthDecisionTraceV1 {
    /// Capture a typed trace from a stable-selection result.
    ///
    /// The single-use permit is not copied. Only its generation is retained as
    /// diagnostic identity when a transition was admitted.
    #[must_use]
    pub fn capture(
        current: ElasticWordWidthV1,
        required_floor: ElasticWordWidthV1,
        maximum_budget: ElasticWordWidthV1,
        selection: &ElasticWordWidthStableSelectionV1,
    ) -> Self {
        match selection {
            ElasticWordWidthStableSelectionV1::Hold { width } => Self {
                current,
                required_floor,
                maximum_budget,
                target: Some(*width),
                outcome: ElasticWordWidthDecisionOutcomeV1::Hold,
                direction: None,
                block_reason: None,
                stability: None,
                permit_generation: None,
            },
            ElasticWordWidthStableSelectionV1::Blocked {
                required_floor: selected_floor,
                maximum_budget: selected_budget,
                reason,
            } => {
                debug_assert_eq!(required_floor, *selected_floor);
                debug_assert_eq!(maximum_budget, *selected_budget);
                Self {
                    current,
                    required_floor,
                    maximum_budget,
                    target: None,
                    outcome: ElasticWordWidthDecisionOutcomeV1::Blocked,
                    direction: None,
                    block_reason: Some(*reason),
                    stability: None,
                    permit_generation: None,
                }
            }
            ElasticWordWidthStableSelectionV1::Deferred {
                from,
                to,
                direction,
                stability,
            } => {
                debug_assert_eq!(current, *from);
                Self {
                    current,
                    required_floor,
                    maximum_budget,
                    target: Some(*to),
                    outcome: ElasticWordWidthDecisionOutcomeV1::Deferred,
                    direction: Some(*direction),
                    block_reason: None,
                    stability: Some(stability.clone()),
                    permit_generation: None,
                }
            }
            ElasticWordWidthStableSelectionV1::Admitted {
                from,
                to,
                direction,
                stability,
                permit,
            } => {
                debug_assert_eq!(current, *from);
                Self {
                    current,
                    required_floor,
                    maximum_budget,
                    target: Some(*to),
                    outcome: ElasticWordWidthDecisionOutcomeV1::Admitted,
                    direction: Some(*direction),
                    block_reason: None,
                    stability: Some(stability.clone()),
                    permit_generation: Some(permit.generation()),
                }
            }
        }
    }

    #[must_use]
    pub const fn current(&self) -> ElasticWordWidthV1 {
        self.current
    }

    #[must_use]
    pub const fn required_floor(&self) -> ElasticWordWidthV1 {
        self.required_floor
    }

    #[must_use]
    pub const fn maximum_budget(&self) -> ElasticWordWidthV1 {
        self.maximum_budget
    }

    #[must_use]
    pub const fn target(&self) -> Option<ElasticWordWidthV1> {
        self.target
    }

    #[must_use]
    pub const fn outcome(&self) -> ElasticWordWidthDecisionOutcomeV1 {
        self.outcome
    }

    #[must_use]
    pub const fn direction(&self) -> Option<ElasticWordWidthDirectionV1> {
        self.direction
    }

    #[must_use]
    pub const fn block_reason(&self) -> Option<ElasticWordWidthBlockReasonV1> {
        self.block_reason
    }

    #[must_use]
    pub const fn stability(&self) -> Option<&TransitionStabilityReportV1> {
        self.stability.as_ref()
    }

    #[must_use]
    pub const fn permit_generation(&self) -> Option<u64> {
        self.permit_generation
    }

    /// Whether this trace itself carries any actuation or commit capability.
    #[must_use]
    pub const fn carries_authority(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ElasticWordWidthSelectorV1, ElasticWordWidthStabilityControllerV1, ObservationSnapshot,
        TransitionStabilityPolicyV1,
    };
    use elastic_core::{DimensionId, TransitionMechanism};
    use std::time::{Duration, Instant};

    fn width(bits: u16) -> ElasticWordWidthV1 {
        ElasticWordWidthV1::from_bits(bits).unwrap()
    }

    fn controller() -> ElasticWordWidthStabilityControllerV1 {
        ElasticWordWidthStabilityControllerV1::new(
            ElasticWordWidthSelectorV1::full_v1(),
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
    fn admitted_trace_copies_explanation_but_not_permit_authority() {
        let now = Instant::now();
        let observations = ObservationSnapshot::new(now, vec![]);
        let mut controller = controller();
        let selection = controller.select(width(512), width(64), width(2048), &observations, now);
        let trace = ElasticWordWidthDecisionTraceV1::capture(
            width(512),
            width(64),
            width(2048),
            &selection,
        );

        assert_eq!(trace.outcome(), ElasticWordWidthDecisionOutcomeV1::Admitted);
        assert_eq!(trace.current().bits(), 512);
        assert_eq!(trace.target().unwrap().bits(), 64);
        assert_eq!(
            trace.direction(),
            Some(ElasticWordWidthDirectionV1::Contract)
        );
        assert_eq!(trace.permit_generation(), Some(0));
        assert!(trace.stability().is_some());
        assert!(!trace.carries_authority());
    }

    #[test]
    fn hold_trace_contains_no_stability_or_permit() {
        let now = Instant::now();
        let observations = ObservationSnapshot::new(now, vec![]);
        let mut controller = controller();
        let selection = controller.select(width(128), width(128), width(512), &observations, now);
        let trace = ElasticWordWidthDecisionTraceV1::capture(
            width(128),
            width(128),
            width(512),
            &selection,
        );

        assert_eq!(trace.outcome(), ElasticWordWidthDecisionOutcomeV1::Hold);
        assert_eq!(trace.target().unwrap().bits(), 128);
        assert!(trace.stability().is_none());
        assert_eq!(trace.permit_generation(), None);
    }

    #[test]
    fn semantic_budget_block_is_retained_exactly() {
        let now = Instant::now();
        let observations = ObservationSnapshot::new(now, vec![]);
        let mut controller = controller();
        let selection = controller.select(width(128), width(512), width(256), &observations, now);
        let trace = ElasticWordWidthDecisionTraceV1::capture(
            width(128),
            width(512),
            width(256),
            &selection,
        );

        assert_eq!(trace.outcome(), ElasticWordWidthDecisionOutcomeV1::Blocked);
        assert_eq!(
            trace.block_reason(),
            Some(ElasticWordWidthBlockReasonV1::FloorExceedsBudget)
        );
        assert_eq!(trace.target(), None);
        assert!(trace.stability().is_none());
    }

    #[test]
    fn deferred_trace_retains_stability_status_after_commit() {
        let start = Instant::now();
        let observations = ObservationSnapshot::new(start, vec![]);
        let mut controller = controller();

        let first = controller.select(width(512), width(64), width(2048), &observations, start);
        let permit = match first {
            ElasticWordWidthStableSelectionV1::Admitted { permit, .. } => permit,
            other => panic!("expected admitted transition, got {other:?}"),
        };
        controller.record_commit(permit, start).unwrap();

        let later = start + Duration::from_secs(1);
        let later_observations = ObservationSnapshot::new(later, vec![]);
        let deferred = controller.select(
            width(64),
            width(128),
            width(2048),
            &later_observations,
            later,
        );
        let trace =
            ElasticWordWidthDecisionTraceV1::capture(width(64), width(128), width(2048), &deferred);

        assert_eq!(trace.outcome(), ElasticWordWidthDecisionOutcomeV1::Deferred);
        assert!(trace.stability().is_some());
        assert_eq!(trace.permit_generation(), None);
    }
}
