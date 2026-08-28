//! Control-loop primitives.

use crate::error::RuntimeError;
use crate::observation::{ObservationSnapshot, Observer};
use crate::plan::{plan_with_context, Plan};
use elastic_eir::{EirResource, TransitionPlanner};
use std::time::Instant;

/// Observe a resource and run one deterministic planning step.
pub fn observe_and_plan<P: TransitionPlanner, O: Observer>(
    planner: &P,
    resource: &EirResource,
    observer: &O,
) -> Result<(ObservationSnapshot, Plan), RuntimeError> {
    let (mut context, observations) = observer.observe();
    let snapshot = ObservationSnapshot::new(Instant::now(), observations);

    for observation in snapshot.iter() {
        if observation.is_valid() {
            context = context.observe(observation.signal.clone(), observation.value);
        }
    }

    let plan = plan_with_context(planner, resource, &context);
    Ok((snapshot, plan))
}

/// Run a single planning cycle and return only the plan.
///
/// This compatibility helper performs no validation or actuation.
pub fn run_planning_cycle<P: TransitionPlanner, O: Observer>(
    planner: &P,
    resource: &EirResource,
    observer: &O,
) -> Result<Plan, RuntimeError> {
    observe_and_plan(planner, resource, observer).map(|(_, plan)| plan)
}
