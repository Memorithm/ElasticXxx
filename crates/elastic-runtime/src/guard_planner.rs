//! Boolean-gated numeric planning over fresh runtime facts.
//!
//! Boolean eligibility is evaluated before invoking an existing
//! [`TransitionPlanner`]. The planner receives a validated EIR view containing
//! only eligible transitions within the configured target, not the original
//! admission set. Returned candidates are checked against both the original
//! declaration and this restricted view. No actuation occurs here.

use crate::{BooleanGuardPreplanner, FactSnapshot, GuardPreplannerError};
use elastic_core::resource::{DimensionId, ObservationSignalId};
use elastic_core::{FreshnessSnapshot, TransitionMechanism};
use elastic_eir::{
    EirGuardedResource, EirResource, Fingerprint, PlanOutcome, PlanningContext,
    TransitionCandidate, TransitionPlanner, TransitionPruningReport,
};
use std::fmt;

/// Non-cryptographic structural identity of the numeric planning context.
///
/// The fingerprint absorbs every observation in canonical signal order, the
/// built-in/custom signal discriminator, and the exact IEEE-754 bit pattern of
/// each value. It is suitable for cycle-local identity checks and durable
/// diagnostics, not authentication or actuation authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlanningContextFingerprint(u64);

impl PlanningContextFingerprint {
    /// Raw structural fingerprint bits for diagnostics and persisted evidence.
    #[must_use]
    pub const fn bits(self) -> u64 {
        self.0
    }
}

impl fmt::Display for PlanningContextFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "context-fp:{:016x}", self.0)
    }
}

/// Compute deterministic identity for one numeric planning context.
#[must_use]
pub fn planning_context_fingerprint(context: &PlanningContext) -> PlanningContextFingerprint {
    let mut fingerprint = Fingerprint::EMPTY
        .text("runtime-planning-context")
        .number(1);
    for (signal, value) in context.iter() {
        fingerprint = fingerprint
            .text(observation_signal_kind(signal))
            .text(signal.as_str())
            .number(value.to_bits());
    }
    PlanningContextFingerprint(fingerprint.bits())
}

/// Exact guarded-planning result captured at the Boolean/numeric planning
/// boundary before any trusted validation or actuation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuardedPlanningDecision {
    outcome: PlanOutcome,
    pruning_report: TransitionPruningReport,
    planning_context_fingerprint: PlanningContextFingerprint,
}

impl GuardedPlanningDecision {
    fn new(
        outcome: PlanOutcome,
        pruning_report: TransitionPruningReport,
        planning_context_fingerprint: PlanningContextFingerprint,
    ) -> Self {
        Self {
            outcome,
            pruning_report,
            planning_context_fingerprint,
        }
    }

    /// Final post-filter numeric planning outcome.
    #[must_use]
    pub const fn outcome(&self) -> &PlanOutcome {
        &self.outcome
    }

    /// Source-bound Boolean pruning report used for this exact planning call.
    #[must_use]
    pub const fn pruning_report(&self) -> &TransitionPruningReport {
        &self.pruning_report
    }

    /// Structural identity of the numeric observations used by the planner.
    #[must_use]
    pub const fn planning_context_fingerprint(&self) -> PlanningContextFingerprint {
        self.planning_context_fingerprint
    }

    /// Consume the detailed result and retain the legacy planner outcome only.
    #[must_use]
    pub fn into_outcome(self) -> PlanOutcome {
        self.outcome
    }
}

/// Boolean eligibility scope that must survive before a wrapped numeric planner
/// is allowed to run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GuardPlannerTarget {
    /// Rank any transition in the Boolean-eligible subset.
    AnyEligible,
    /// Require and rank only this exact Boolean-eligible transition.
    Transition {
        /// Required transition mechanism.
        mechanism: TransitionMechanism,
        /// Required elastic dimension.
        dimension: DimensionId,
    },
}

impl GuardPlannerTarget {
    fn accepts(&self, candidate: &TransitionCandidate) -> bool {
        match self {
            Self::AnyEligible => true,
            Self::Transition {
                mechanism,
                dimension,
            } => candidate_matches(candidate, *mechanism, dimension),
        }
    }
}

/// Composition layer that performs Boolean pruning before numeric planning.
///
/// Existing planners keep their original [`TransitionPlanner`] implementation
/// and numeric control law, but see only surviving admissions. Resource
/// invariants, objective priorities, observations and labels are preserved.
/// The projection is planning-only; original declarations and trusted adapter
/// checks remain authoritative for validation and actuation.
#[derive(Clone, Debug, PartialEq)]
pub struct BooleanGuardPlanner<P> {
    inner: P,
    target: GuardPlannerTarget,
}

impl<P> BooleanGuardPlanner<P> {
    /// Wrap a planner and permit it to rank all Boolean survivors.
    #[must_use]
    pub fn new(inner: P) -> Self {
        Self {
            inner,
            target: GuardPlannerTarget::AnyEligible,
        }
    }

    /// Wrap a planner and restrict its input and output to one transition.
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

    /// Boolean target checked before and after numeric planning.
    #[must_use]
    pub const fn target(&self) -> &GuardPlannerTarget {
        &self.target
    }
}

impl<P: TransitionPlanner> BooleanGuardPlanner<P> {
    /// Prune candidates from fresh facts and rank only eligible admissions.
    ///
    /// Empty admissions or an undeclared exact target yield `Unsupported`
    /// without invoking the wrapped planner. False targets yield `NoCandidate`;
    /// unknown targets yield `InsufficientEvidence`. A structural projection
    /// failure also yields `InsufficientEvidence` without numeric planning.
    ///
    /// A selected candidate must be grounded, declared in the original resource,
    /// within the configured target, and present in the restricted view. Its
    /// advisory magnitude is preserved for later trusted adapter validation.
    ///
    /// The projected resource has its own fingerprint. Decision traces must
    /// still bind the original guarded resource and the original fact snapshot.
    /// This method does not bind the separate numeric context to a fact epoch;
    /// the caller must provide coherent observations for the same planning cycle.
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
        self.propose_transition_detailed_with_context(resource, context, facts, freshness)
            .map(GuardedPlanningDecision::into_outcome)
    }

    /// Perform the same guarded planning operation while retaining the exact
    /// source-bound pruning report and numeric-context identity used to reach
    /// the final outcome.
    ///
    /// This is the evidence-producing form of [`Self::propose_transition_with_context`].
    /// It does not execute the inner numeric planner more than once, performs no
    /// validation or actuation, and keeps the original guarded resource outside
    /// the projected numeric planning view.
    ///
    /// # Errors
    ///
    /// Returns [`GuardPreplannerError`] under exactly the same freshness and
    /// Boolean-evaluation failures as the legacy outcome-only method.
    pub fn propose_transition_detailed_with_context(
        &self,
        resource: &EirGuardedResource,
        context: &PlanningContext,
        facts: &FactSnapshot,
        freshness: &FreshnessSnapshot,
    ) -> Result<GuardedPlanningDecision, GuardPreplannerError> {
        let report = BooleanGuardPreplanner.prune(resource, facts, freshness)?;
        let context_fingerprint = planning_context_fingerprint(context);

        if let Some(blocked) = preplanning_block(resource, &report, &self.target) {
            return Ok(GuardedPlanningDecision::new(
                blocked,
                report,
                context_fingerprint,
            ));
        }

        let candidates: Vec<_> = report
            .eligible()
            .iter()
            .filter(|candidate| self.target.accepts(candidate))
            .cloned()
            .collect();
        let planning_resource = match resource.restrict_to_eligible(&report, &candidates) {
            Ok(view) => view,
            Err(error) => {
                return Ok(GuardedPlanningDecision::new(
                    PlanOutcome::InsufficientEvidence {
                        detail: format!("cannot construct Boolean planning subset: {error}"),
                    },
                    report,
                    context_fingerprint,
                ));
            }
        };
        let outcome = self
            .inner
            .propose_transition_with_context(&planning_resource, context);
        let outcome = filter_planner_outcome(
            outcome,
            resource.resource(),
            &planning_resource,
            &report,
            &self.target,
        );
        Ok(GuardedPlanningDecision::new(
            outcome,
            report,
            context_fingerprint,
        ))
    }
}

const fn observation_signal_kind(signal: &ObservationSignalId) -> &'static str {
    if signal.builtin_part().is_some() {
        "builtin"
    } else {
        "custom"
    }
}

fn preplanning_block(
    resource: &EirGuardedResource,
    report: &TransitionPruningReport,
    target: &GuardPlannerTarget,
) -> Option<PlanOutcome> {
    match target {
        GuardPlannerTarget::AnyEligible => {
            if resource.resource().transitions().is_empty() {
                return Some(PlanOutcome::Unsupported);
            }
            if !report.eligible().is_empty() {
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
                return Some(PlanOutcome::Unsupported);
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

fn filter_planner_outcome(
    outcome: PlanOutcome,
    original: &EirResource,
    planning_resource: &EirResource,
    report: &TransitionPruningReport,
    target: &GuardPlannerTarget,
) -> PlanOutcome {
    let PlanOutcome::Candidate(candidate) = outcome else {
        return outcome;
    };

    if !candidate.is_declared_in(original) || !target.accepts(&candidate) {
        return PlanOutcome::Unsupported;
    }
    if candidate.is_declared_in(planning_resource) {
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
    fn planning_context_fingerprint_is_order_independent_bit_exact_and_term_typed() {
        let first = PlanningContext::new()
            .observe(ObservationSignalId::UTILIZATION, 0.5)
            .observe(custom_signal("zeta"), -0.0);
        let reordered = PlanningContext::new()
            .observe(custom_signal("zeta"), -0.0)
            .observe(ObservationSignalId::UTILIZATION, 0.5);
        let positive_zero = PlanningContext::new()
            .observe(ObservationSignalId::UTILIZATION, 0.5)
            .observe(custom_signal("zeta"), 0.0);
        let builtin = PlanningContext::new().observe(ObservationSignalId::UTILIZATION, 0.5);
        let custom_same_text = PlanningContext::new()
            .observe(ObservationSignalId::custom("utilization").unwrap(), 0.5);

        assert_eq!(
            planning_context_fingerprint(&first),
            planning_context_fingerprint(&reordered)
        );
        assert_ne!(
            planning_context_fingerprint(&first),
            planning_context_fingerprint(&positive_zero)
        );
        assert_ne!(
            planning_context_fingerprint(&builtin),
            planning_context_fingerprint(&custom_same_text)
        );
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
    fn detailed_planning_retains_exact_report_context_and_single_numeric_call() {
        let (resource, facts, freshness) = fixture(Some(true));
        let calls = Cell::new(0);
        let planner = BooleanGuardPlanner::for_capacity(CountingPlanner { calls: &calls });
        let context = threshold_context();
        let detailed = planner
            .propose_transition_detailed_with_context(&resource, &context, &facts, &freshness)
            .unwrap();

        assert_eq!(calls.get(), 1);
        assert!(matches!(detailed.outcome(), PlanOutcome::Candidate(_)));
        assert!(detailed.pruning_report().is_for_resource(&resource));
        assert!(detailed.pruning_report().fingerprint().is_some());
        assert_eq!(
            detailed.planning_context_fingerprint(),
            planning_context_fingerprint(&context)
        );
    }

    #[test]
    fn detailed_blocked_outcomes_never_call_numeric_planner() {
        for guard_value in [Some(false), None] {
            let (resource, facts, freshness) = fixture(guard_value);
            let calls = Cell::new(0);
            let planner = BooleanGuardPlanner::for_capacity(CountingPlanner { calls: &calls });
            let detailed = planner
                .propose_transition_detailed_with_context(
                    &resource,
                    &threshold_context(),
                    &facts,
                    &freshness,
                )
                .unwrap();
            assert_eq!(calls.get(), 0);
            assert!(matches!(
                detailed.outcome(),
                PlanOutcome::NoCandidate | PlanOutcome::InsufficientEvidence { .. }
            ));
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
