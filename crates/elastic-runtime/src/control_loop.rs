//! Control loop implementation.

use crate::error::RuntimeError;
use crate::observation::{Observation, ObservationSnapshot, Observer};
use crate::plan::{plan_with_context, Plan, ValidatedPlan};
use elastic_eir::{EirResource, PlanningContext, TransitionPlanner};

/// Run a single planning cycle given observations.
pub fn run_planning_cycle<P: TransitionPlanner, O: Observer>(
    planner: &P,
    resource: &EirResource,
    observer: &O,
) -> Result<Plan, RuntimeError> {
    let (_ctx, observations) = observer.observe();
    // Build planning context from observations
    let mut context = PlanningContext::new();
    for obs in &observations {
        if obs.is_valid() {
            context = context.observe(obs.signal.clone(), obs.value);
        }
    }
    Ok(plan_with_context(planner, resource, &context))
}

/// Validate a plan against invariants (stub).
pub fn validate_plan(plan: Plan) -> ValidatedPlan {
    // Placeholder validation
    ValidatedPlan::new(plan, vec![], true)
}
