//! Boolean-gated numeric planning over fresh runtime facts.
//!
//! This module composes the BE6 preplanner with existing numeric
//! [`TransitionPlanner`] implementations. Boolean eligibility is evaluated
//! first. When a configured target is rejected or unknown, the wrapped planner
//! is not invoked at all. When it is eligible, the legacy numeric planner runs
//! unchanged and its candidate is filtered against the same pruning report
//! before it can leave this boundary.

use crate::{BooleanGuardPreplanner, FactSnapshot, GuardPreplannerError};
use elastic_core::resource::DimensionId;
use elastic_core::{FreshnessSnapshot, TransitionMechanism};
use elastic_eir::{
    EirGuardedResource, PlanOutcome, PlanningContext, TransitionCandidate, TransitionPlanner,
    TransitionPruningReport,
};

/// Boolean eligibility scope that must survive before a wrapped numeric planner
/// is allowed to run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GuardPlannerTarget {
    /// Allow the planner to run when at least one declared transition is
    /// Boolean-eligible.
    AnyEligible,
    /// Require one exact transition to be Boolean-eligible before invoking the
    /// wrapped planner.
    Transition {
        /// Required transition mechanism.
        mechanism: TransitionMechanism,
        /// Required elastic dimension.
        dimension: DimensionId,
    },
}

/// Composition layer that performs Boolean pruning before numeric planning.
///
/// Existing planners keep their original [`TransitionPlanner`] implementation;
/// this wrapper adds a fail-closed Boolean gate without changing their numeric
/// control law.
#[derive(Clone, Debug, PartialEq)]
pub struct BooleanGuardPlanner<P> {
    inner: P,
    target: GuardPlannerTarget,
}

impl<P> BooleanGuardPlanner<P> {
    /// Wrap a planner and permit it to run when any transition survives Boolean
    /// pruning.
    #[must_use]
    pub fn new(inner: P) -> Self {
        Self {
            inner,
            target: GuardPlannerTarget::AnyEligible,
        }
    }

    /// Wrap a planner whose work is meaningful only for one exact transition.
    #[must_use]
    pub fn for_transition(
        inner: P,
        mechanism: TransitionMechanism,
        dimension: DimensionId,
    ) -> Self {
        Self {
            inner,
            target: GuardPlannerTarget::Transition {
                mechanism,
                dimension,
            },
        }
    }

    /// Convenience constructor for the current capacity planners, which target
    /// `Reinterpret@capacity`.
    #[must_use]
    pub fn for_capacity(inner: P) -> Self {
        Self::for_transition(
            inner,
            TransitionMechanism::Reinterpret,
            DimensionId::CAPACITY,
        )
    }

    /// Borrow the wrapped legacy planner.
    #[must_use]
    pub const fn inner(&self) -> &P {
        &self.inner
    }

    /// Boolean target checked before numeric planning.
    #[must_use]
    pub const fn target(&self) -> &GuardPlannerTarget {
        &self.target
    }
}

impl<P: TransitionPlanner> BooleanGuardPlanner<P> {
    /// Prune candidates from fresh facts, then invoke the wrapped numeric
    /// planner only when its Boolean target survives.
    ///
    /// A returned candidate is checked a second time against the same report.
    /// This protects against a custom planner selecting a transition outside
    /// the Boolean-eligible subset even after another transition allowed the
    /// planner to run.
    ///
    /// # Errors
    ///
    /// Returns [`GuardPreplannerError`] when fact provenance/freshness or the
    /// bounded Boolean evaluator fails. Numeric planning is never invoked in
    /// that case.
    pub fn propose_transition_with_context(
        &self,
        resource: &EirGuardedResource,
        context: &PlanningContext,
        facts: &FactSnapshot,
        freshness: &FreshnessSnapshot,
    ) -> Result<PlanOutcome, GuardPreplannerError> {
        let report = BooleanGuardPreplanner.prune(resource, facts, freshness)?;

        if let Some(blocked) = preplanning_block(resource, &report, &self.target) {
            return Ok(blocked);
        }

        let outcome = self
            .inner
            .propose_transition_with_context(resource.resource(), context);
        Ok(filter_planner_outcome(outcome, &report))
    }
}

fn preplanning_block(
    resource: &EirGuardedResource,
    report: &TransitionPruningReport,
    target: &GuardPlannerTarget,
) -> Option<PlanOutcome> {
    match target {
        GuardPlannerTarget::AnyEligible => {
            if resource.resource().transitions().is_empty() || !report.eligible().is_empty() {
                return None;
            }
            if report.unknown().is_empty() {
                Some(PlanOutcome::NoCandidate)
            } else {
                Some(PlanOutcome::InsufficientEvidence {
                    detail: "Boolean guards leave no eligible transition and at least one transition is unknown"
                        .to_owned(),
                })
            }
        }
        GuardPlannerTarget::Transition {
            mechanism,
            dimension,
        } => {
            let declared = resource.resource().transitions().iter().any(|admitted| {
                admitted.transition().mechanism() == *mechanism
                    && admitted.transition().dimension() == dimension
            });
            if !declared {
                // Preserve the wrapped planner's historical unsupported/missing
                // evidence behavior when its target is outside this resource.
                return None;
            }
            if report.contains_eligible(*mechanism, dimension) {
                return None;
            }
            if report
                .unknown()
                .iter()
                .any(|entry| candidate_matches(entry.candidate(), *mechanism, dimension))
            {
                Some(PlanOutcome::InsufficientEvidence {
                    detail: format!(
                        "Boolean eligibility for {}@{} is unknown",
                        mechanism_text(*mechanism),
                        dimension
                    ),
                })
            } else {
                Some(PlanOutcome::NoCandidate)
            }
        }
    }
}

fn filter_planner_outcome(outcome: PlanOutcome, report: &TransitionPruningReport) -> PlanOutcome {
    let PlanOutcome::Candidate(candidate) = outcome else {
        return outcome;
    };

    if report
        .eligible()
        .iter()
        .any(|eligible| same_transition(eligible, &candidate))
    {
        return PlanOutcome::Candidate(candidate);
    }

    if report
        .unknown()
        .iter()
        .any(|entry| same_transition(entry.candidate(), &candidate))
    {
        return PlanOutcome::InsufficientEvidence {
            detail: format!(
                "numeric planner selected {}@{} whose Boolean eligibility is unknown",
                mechanism_text(candidate.mechanism()),
                candidate.dimension()
            ),
        };
    }

    PlanOutcome::NoCandidate
}

fn candidate_matches(
    candidate: &TransitionCandidate,
    mechanism: TransitionMechanism,
    dimension: &DimensionId,
) -> bool {
    candidate.mechanism() == mechanism && candidate.dimension() == dimension
}

fn same_transition(left: &TransitionCandidate, right: &TransitionCandidate) -> bool {
    left.mechanism() == right.mechanism() && left.dimension() == right.dimension()
}

const fn mechanism_text(mechanism: TransitionMechanism) -> &'static str {
    match mechanism {
        TransitionMechanism::Reinterpret => "reinterpret",
        TransitionMechanism::Reencode => "reencode",
        TransitionMechanism::Recompute => "recompute",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CapabilityPredicate, FactResourceBinding, FactSourceId, ObservationSnapshot,
        PredicateEvaluationInput,
    };
    use elastic_adapters::{HeadroomPlanner, ThresholdPlanner};
    use elastic_core::resource::{
        AdmissibleTransition, CapabilityRequirement, LogicalResourceId, ObservationSignalId,
        ResourceClassId, ResourceSpec,
    };
    use elastic_core::{
        BoolExpr, BooleanGuard, GuardScope, GuardedResourceSpec, ObservationEpoch, PlannerEpoch,
        PredicateKey, PredicateRegistry, ResourceGeneration,
    };
    use elastic_eir::lower_guarded;
    use std::cell::Cell;
    use std::time::Instant;

    fn custom_signal(name: &str) -> ObservationSignalId {
        ObservationSignalId::custom(name).unwrap()
    }

    fn fixture(guard_value: Option<bool>) -> (EirGuardedResource, FactSnapshot, FreshnessSnapshot) {
        let resource_id = LogicalResourceId::new("guarded-numeric-capacity").unwrap();
        let spec = ResourceSpec::builder(ResourceClassId::CAPACITY_RESOURCE, resource_id.clone())
            .allow(DimensionId::CAPACITY)
            .admit(AdmissibleTransition::new(
                TransitionMechanism::Reinterpret,
                DimensionId::CAPACITY,
            ))
            .require_capability(CapabilityRequirement::new(
                TransitionMechanism::Reinterpret,
                DimensionId::CAPACITY,
            ))
            .build()
            .unwrap();

        let key = PredicateKey::new("elastic.planner", "capacity-ok").unwrap();
        let registry = PredicateRegistry::from_keys([key.clone()]).unwrap();
        let predicate_id = registry.id(&key).unwrap();
        let guard = BooleanGuard::new(
            GuardScope::Transition {
                mechanism: TransitionMechanism::Reinterpret,
                dimension: DimensionId::CAPACITY,
            },
            registry,
            BoolExpr::atom(predicate_id),
        )
        .unwrap();
        let guarded = lower_guarded(&GuardedResourceSpec::new(spec, vec![guard]).unwrap()).unwrap();

        let now = Instant::now();
        let observations = ObservationSnapshot::new(now, Vec::new());
        let empty_context = PlanningContext::new();
        let input = PredicateEvaluationInput::new(&empty_context, &observations, now);
        let evaluator = CapabilityPredicate::new(key, guard_value);
        let facts = FactSnapshot::derive(
            FactSourceId::new("runtime:guarded-numeric-test").unwrap(),
            ObservationEpoch::new(8),
            Some(FactResourceBinding::new(
                resource_id.clone(),
                ResourceGeneration::new(3),
            )),
            &input,
            &[&evaluator],
        )
        .unwrap();
        let freshness = FreshnessSnapshot::new(PlannerEpoch::new(5), ObservationEpoch::new(8))
            .with_resource_generation(resource_id, ResourceGeneration::new(3));
        (guarded, facts, freshness)
    }

    fn threshold_context() -> PlanningContext {
        PlanningContext::new()
            .observe(ObservationSignalId::UTILIZATION, 0.9)
            .observe(custom_signal("committed-bytes"), 100.0)
    }

    fn headroom_context() -> PlanningContext {
        PlanningContext::new()
            .observe(ObservationSignalId::FREE_CAPACITY, 10.0)
            .observe(custom_signal("committed-bytes"), 100.0)
            .observe(custom_signal("host-total-bytes"), 200.0)
    }

    #[test]
    fn true_guard_preserves_threshold_planner_output() {
        let (resource, facts, freshness) = fixture(Some(true));
        let numeric = ThresholdPlanner::new(0.25, 0.75, 0.20).unwrap();
        let context = threshold_context();
        let legacy = numeric.propose_transition_with_context(resource.resource(), &context);
        let guarded = BooleanGuardPlanner::for_capacity(numeric)
            .propose_transition_with_context(&resource, &context, &facts, &freshness)
            .unwrap();
        assert_eq!(guarded, legacy);
    }

    #[test]
    fn true_guard_preserves_headroom_planner_output() {
        let (resource, facts, freshness) = fixture(Some(true));
        let numeric = HeadroomPlanner::new(0.25, 0.05).unwrap();
        let context = headroom_context();
        let legacy = numeric.propose_transition_with_context(resource.resource(), &context);
        let guarded = BooleanGuardPlanner::for_capacity(numeric)
            .propose_transition_with_context(&resource, &context, &facts, &freshness)
            .unwrap();
        assert_eq!(guarded, legacy);
    }

    struct CountingPlanner<'a> {
        calls: &'a Cell<usize>,
    }

    impl TransitionPlanner for CountingPlanner<'_> {
        fn propose_transition(&self, resource: &elastic_eir::EirResource) -> PlanOutcome {
            self.propose_transition_with_context(resource, &PlanningContext::new())
        }

        fn propose_transition_with_context(
            &self,
            resource: &elastic_eir::EirResource,
            _context: &PlanningContext,
        ) -> PlanOutcome {
            self.calls.set(self.calls.get() + 1);
            resource
                .transitions()
                .first()
                .map(|admitted| {
                    PlanOutcome::Candidate(TransitionCandidate::from_admitted(admitted))
                })
                .unwrap_or(PlanOutcome::Unsupported)
        }
    }

    #[test]
    fn false_guard_skips_numeric_planner_entirely() {
        let (resource, facts, freshness) = fixture(Some(false));
        let calls = Cell::new(0);
        let planner = BooleanGuardPlanner::for_capacity(CountingPlanner { calls: &calls });
        let outcome = planner
            .propose_transition_with_context(&resource, &threshold_context(), &facts, &freshness)
            .unwrap();
        assert_eq!(outcome, PlanOutcome::NoCandidate);
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn unknown_guard_skips_numeric_planner_and_reports_insufficient_evidence() {
        let (resource, facts, freshness) = fixture(None);
        let calls = Cell::new(0);
        let planner = BooleanGuardPlanner::for_capacity(CountingPlanner { calls: &calls });
        let outcome = planner
            .propose_transition_with_context(&resource, &threshold_context(), &facts, &freshness)
            .unwrap();
        assert!(matches!(outcome, PlanOutcome::InsufficientEvidence { .. }));
        assert_eq!(calls.get(), 0);
    }
}
