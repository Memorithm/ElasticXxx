//! Planning types connecting observations to transition candidates.

use elastic_core::resource::Invariant;
use elastic_eir::{EirResource, PlanOutcome, PlanningContext, TransitionCandidate, TransitionPlanner};

/// Result of planning step with explanation.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub resource: EirResource,
    pub context: PlanningContext,
    pub outcome: PlanOutcome,
    pub reasoning: String,
}

impl Plan {
    pub fn new(resource: EirResource, context: PlanningContext, outcome: PlanOutcome, reasoning: String) -> Self {
        Self { resource, context, outcome, reasoning }
    }

    pub fn candidate(&self) -> Option<&TransitionCandidate> {
        match &self.outcome {
            PlanOutcome::Candidate(c) => Some(c),
            _ => None,
        }
    }

    pub fn is_noop(&self) -> bool {
        matches!(self.outcome, PlanOutcome::NoCandidate) || self.candidate().is_none()
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
        Self { plan, invariant_checks, validated }
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
        Self { invariant, holds, detail }
    }
}

/// Helper to run planning with context.
pub fn plan_with_context<P: TransitionPlanner>(
    planner: &P,
    resource: &EirResource,
    context: &PlanningContext,
) -> Plan {
    let outcome = planner.propose_transition_with_context(resource, context);
    let reasoning = format!("Planner proposed outcome: {}", outcome);
    Plan::new(resource.clone(), context.clone(), outcome, reasoning)
}
