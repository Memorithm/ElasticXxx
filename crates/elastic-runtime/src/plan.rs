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

/// Validate a plan from explicit trusted invariant checks.
///
/// A plan is validated only when it contains a declared, capability-grounded
/// candidate and every invariant applicable to that candidate has an explicit
/// successful check. Missing checks are failures; the runtime never assumes an
/// invariant holds merely because the validator omitted it.
#[must_use]
pub fn validate_with_checks(plan: Plan, invariant_checks: Vec<InvariantCheck>) -> ValidatedPlan {
    let Some(candidate) = plan.candidate() else {
        return ValidatedPlan::new(plan, invariant_checks, false);
    };

    if !candidate.is_declared_in(&plan.resource) {
        return ValidatedPlan::new(plan, invariant_checks, false);
    }

    let applicable_invariants = plan.resource.invariants().iter().filter(|invariant| {
        invariant
            .scope()
            .is_none_or(|scope| scope == candidate.dimension())
    });

    let validated = applicable_invariants.clone().all(|invariant| {
        invariant_checks
            .iter()
            .any(|check| check.invariant == *invariant && check.holds)
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
    use elastic_eir::FirstGroundedPlanner;

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
}
