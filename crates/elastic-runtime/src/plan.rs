//! Planning types connecting observations to transition candidates.

use elastic_core::resource::Invariant;
use elastic_eir::{
    EirResource, PlanOutcome, PlanningContext, TransitionCandidate, TransitionPlanner,
};

/// Result of a planning step with explanation.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub resource: EirResource,
    pub context: PlanningContext,
    pub outcome: PlanOutcome,
    pub reasoning: String,
}

impl Plan {
    pub fn new(
        resource: EirResource,
        context: PlanningContext,
        outcome: PlanOutcome,
        reasoning: String,
    ) -> Self {
        Self {
            resource,
            context,
            outcome,
            reasoning,
        }
    }

    pub fn candidate(&self) -> Option<&TransitionCandidate> {
        match &self.outcome {
            PlanOutcome::Candidate(candidate) => Some(candidate),
            _ => None,
        }
    }

    pub fn is_noop(&self) -> bool {
        self.candidate().is_none()
    }
}

/// Validated plan ready for actuation.
#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedPlan {
    pub plan: Plan,
    pub invariant_checks: Vec<InvariantCheck>,
    pub validated: bool,
}

impl ValidatedPlan {
    pub fn new(plan: Plan, invariant_checks: Vec<InvariantCheck>, validated: bool) -> Self {
        Self {
            plan,
            invariant_checks,
            validated,
        }
    }
}

/// An invariant check result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvariantCheck {
    pub invariant: Invariant,
    pub holds: bool,
    pub detail: Option<String>,
}

impl InvariantCheck {
    pub fn new(invariant: Invariant, holds: bool, detail: Option<String>) -> Self {
        Self {
            invariant,
            holds,
            detail,
        }
    }
}

/// Whether `invariant` applies to `candidate` under the runtime validation
/// contract.
pub(crate) fn invariant_applies_to_candidate(
    invariant: &Invariant,
    candidate: &TransitionCandidate,
) -> bool {
    invariant
        .scope()
        .is_none_or(|scope| scope == candidate.dimension())
}

/// Validate a plan from explicit trusted invariant checks.
///
/// A plan is validated only when it contains a declared, capability-grounded
/// candidate and every applicable invariant has at least one explicit check,
/// with all matching checks successful. An explicit failure cannot be hidden
/// by another successful check for the same invariant, regardless of order.
/// Missing checks fail closed. Repeated agreeing successful checks are allowed;
/// checks for invariants outside this candidate's scope do not change its result.
/// Boolean prechecks never substitute for these trusted checks.
#[must_use]
pub fn validate_with_checks(plan: Plan, invariant_checks: Vec<InvariantCheck>) -> ValidatedPlan {
    let Some(candidate) = plan.candidate() else {
        return ValidatedPlan::new(plan, invariant_checks, false);
    };

    if !candidate.is_declared_in(&plan.resource) {
        return ValidatedPlan::new(plan, invariant_checks, false);
    }

    let mut applicable_invariants = plan
        .resource
        .invariants()
        .iter()
        .filter(|invariant| invariant_applies_to_candidate(invariant, candidate));

    let validated = applicable_invariants.all(|invariant| {
        let mut matching = invariant_checks
            .iter()
            .filter(|check| check.invariant == *invariant);
        matching.next().is_some_and(|check| check.holds) && matching.all(|check| check.holds)
    });

    ValidatedPlan::new(plan, invariant_checks, validated)
}

/// Helper to run planning with context.
pub fn plan_with_context<P: TransitionPlanner>(
    planner: &P,
    resource: &EirResource,
    context: &PlanningContext,
) -> Plan {
    let outcome = planner.propose_transition_with_context(resource, context);
    let reasoning = format!("Planner proposed outcome: {outcome}");
    Plan::new(resource.clone(), context.clone(), outcome, reasoning)
}

#[cfg(test)]
mod tests {
    use super::*;
    use elastic_core::resource::{
        AdmissibleTransition, CapabilityRequirement, DimensionId, InvariantKind,
        LogicalResourceId, ResourceClassId, ResourceSpec,
    };
    use elastic_core::TransitionMechanism;
    use elastic_eir::{lower, FirstGroundedPlanner};

    #[test]
    fn missing_invariant_check_never_validates_candidate() {
        let resource = crate::RuntimeConfig::default().ir_resource;
        let plan = plan_with_context(&FirstGroundedPlanner, &resource, &PlanningContext::new());
        let validated = validate_with_checks(plan, Vec::new());
        assert!(!validated.validated);
    }

    #[test]
    fn explicit_successful_check_validates_declared_candidate() {
        let resource = crate::RuntimeConfig::default().ir_resource;
        let plan = plan_with_context(&FirstGroundedPlanner, &resource, &PlanningContext::new());
        let checks = resource
            .invariants()
            .iter()
            .cloned()
            .map(|invariant| InvariantCheck::new(invariant, true, None))
            .collect();
        let validated = validate_with_checks(plan, checks);
        assert!(validated.validated);
    }

    fn scoped_plan() -> (Plan, Invariant, Invariant) {
        let global = Invariant::new(InvariantKind::PreserveContents);
        let residency =
            Invariant::new(InvariantKind::PreserveIdentity).along(DimensionId::RESIDENCY);
        let spec = ResourceSpec::builder(
            ResourceClassId::CAPACITY_RESOURCE,
            LogicalResourceId::new("trusted-check-conflicts").unwrap(),
        )
        .allow(DimensionId::CAPACITY)
        .allow(DimensionId::RESIDENCY)
        .preserve(global.clone())
        .preserve(residency.clone())
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
        let resource = lower(&spec).unwrap().resources()[0].clone();
        let plan = plan_with_context(&FirstGroundedPlanner, &resource, &PlanningContext::new());
        (plan, global, residency)
    }

    #[test]
    fn conflicting_checks_fail_in_both_orders_and_preserve_evidence() {
        let (plan, invariant, _) = scoped_plan();
        for values in [[true, false], [false, true]] {
            let checks: Vec<_> = values
                .into_iter()
                .map(|holds| InvariantCheck::new(invariant.clone(), holds, None))
                .collect();
            let validated = validate_with_checks(plan.clone(), checks.clone());
            assert!(!validated.validated);
            assert_eq!(validated.invariant_checks, checks);
            assert_eq!(validated.plan, plan);
        }
    }

    #[test]
    fn exhaustive_check_sequences_require_nonempty_unanimous_success() {
        let (plan, invariant, _) = scoped_plan();
        for len in 0..=5 {
            for bits in 0..(1_usize << len) {
                let checks = (0..len)
                    .map(|index| {
                        InvariantCheck::new(invariant.clone(), bits & (1 << index) != 0, None)
                    })
                    .collect();
                let expected = len > 0 && bits == (1_usize << len) - 1;
                assert_eq!(validate_with_checks(plan.clone(), checks).validated, expected);
            }
        }
    }

    #[test]
    fn nonapplicable_failed_check_does_not_poison_or_cover_applicable_invariant() {
        let (plan, global, residency) = scoped_plan();
        let unrelated = InvariantCheck::new(residency, false, None);
        assert!(!validate_with_checks(plan.clone(), vec![unrelated.clone()]).validated);
        let validated = validate_with_checks(
            plan,
            vec![InvariantCheck::new(global, true, None), unrelated],
        );
        assert!(validated.validated);
    }

    #[test]
    fn successful_checks_cannot_validate_a_noop_or_ungrounded_candidate() {
        let (mut plan, invariant, _) = scoped_plan();
        let checks = vec![InvariantCheck::new(invariant, true, None)];
        plan.outcome = PlanOutcome::NoCandidate;
        assert!(!validate_with_checks(plan.clone(), checks.clone()).validated);

        let spec = ResourceSpec::builder(
            ResourceClassId::CAPACITY_RESOURCE,
            LogicalResourceId::new("ungrounded").unwrap(),
        )
        .allow(DimensionId::CAPACITY)
        .admit(AdmissibleTransition::new(
            TransitionMechanism::Reinterpret,
            DimensionId::CAPACITY,
        ))
        .build()
        .unwrap();
        let document = lower(&spec).unwrap();
        plan.outcome = PlanOutcome::Candidate(TransitionCandidate::from_admitted(
            &document.resources()[0].transitions()[0],
        ));
        assert!(!validate_with_checks(plan, checks).validated);
    }
}
