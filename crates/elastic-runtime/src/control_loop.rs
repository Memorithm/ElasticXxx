//! Control-loop primitives.

use crate::error::RuntimeError;
use crate::observation::{ObservationSnapshot, Observer};
use crate::plan::{plan_with_context, Plan};
use elastic_eir::{EirResource, PlanningContext, TransitionPlanner};
use std::time::Instant;

/// Collect one observation snapshot and its exact planner-facing context.
#[must_use]
pub fn collect_observations<O: Observer>(observer: &O) -> (PlanningContext, ObservationSnapshot) {
    let (context, observations) = observer.observe();
    let snapshot = ObservationSnapshot::new(Instant::now(), observations);
    (context, snapshot)
}

/// Observe a resource and run one deterministic planning step.
pub fn observe_and_plan<P: TransitionPlanner, O: Observer>(
    planner: &P,
    resource: &EirResource,
    observer: &O,
) -> Result<(ObservationSnapshot, Plan), RuntimeError> {
    let (context, snapshot) = collect_observations(observer);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observation::{Observation, ObservationSource};
    use elastic_core::resource::ObservationSignalId;
    use elastic_eir::FirstGroundedPlanner;

    struct TestObserver;

    impl Observer for TestObserver {
        fn observe(&self) -> (PlanningContext, Vec<Observation>) {
            let observation = Observation::from_source(
                ObservationSource::runtime("test"),
                ObservationSignalId::UTILIZATION,
                0.5,
                Instant::now(),
            );
            (
                PlanningContext::new().observe(ObservationSignalId::UTILIZATION, 0.5),
                vec![observation],
            )
        }
    }

    #[test]
    fn collect_observations_preserves_context_and_evidence() {
        let (context, snapshot) = collect_observations(&TestObserver);

        assert_eq!(context.get(ObservationSignalId::UTILIZATION), Some(0.5));
        assert_eq!(snapshot.len(), 1);
        assert!(snapshot.all_signals_valid);
    }

    #[test]
    fn observe_and_plan_uses_provider_context() {
        let resource = crate::RuntimeConfig::default().ir_resource;
        let (snapshot, plan) = observe_and_plan(&FirstGroundedPlanner, &resource, &TestObserver)
            .expect("observation and planning should succeed");

        assert_eq!(snapshot.len(), 1);
        assert_eq!(
            plan.context.get(ObservationSignalId::UTILIZATION),
            Some(0.5)
        );
    }
}
