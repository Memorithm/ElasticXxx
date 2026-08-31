//! Owned high-level controller for ordinary downstream applications.
//!
//! [`Controller`] keeps the runtime policy, normalized resource, planner,
//! observer, and trusted actuator together. This removes repetitive wiring
//! from applications while preserving the same trusted transaction boundary:
//! planners remain advisory and every physical effect still flows through the
//! actuator's validation/verification/commit-or-rollback contract.

use elastic_eir::{EirResource, TransitionPlanner};

use crate::{
    CancellationToken, CycleResult, Observer, RunResult, Runtime, RuntimeError,
    TransactionalActuator,
};

/// Owned operational controller for one logical elastic resource.
#[derive(Debug)]
pub struct Controller<P, O, A> {
    runtime: Runtime,
    resource: EirResource,
    planner: P,
    observer: O,
    actuator: A,
}

impl<P, O, A> Controller<P, O, A> {
    #[must_use]
    pub fn new(
        runtime: Runtime,
        resource: EirResource,
        planner: P,
        observer: O,
        actuator: A,
    ) -> Self {
        Self {
            runtime,
            resource,
            planner,
            observer,
            actuator,
        }
    }

    #[must_use]
    pub const fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    #[must_use]
    pub const fn resource(&self) -> &EirResource {
        &self.resource
    }

    #[must_use]
    pub const fn planner(&self) -> &P {
        &self.planner
    }

    #[must_use]
    pub const fn observer(&self) -> &O {
        &self.observer
    }

    #[must_use]
    pub const fn actuator(&self) -> &A {
        &self.actuator
    }

    pub fn actuator_mut(&mut self) -> &mut A {
        &mut self.actuator
    }

    #[must_use]
    pub fn into_parts(self) -> (Runtime, EirResource, P, O, A) {
        (
            self.runtime,
            self.resource,
            self.planner,
            self.observer,
            self.actuator,
        )
    }
}

impl<P, O, A> Controller<P, O, A>
where
    P: TransitionPlanner,
    O: Observer,
    A: TransactionalActuator,
{
    /// Execute one full control cycle.
    ///
    /// # Errors
    ///
    /// Propagates observation, planning, trusted validation, actuation,
    /// verification, commit, and rollback failures from the underlying runtime.
    pub fn cycle(&mut self) -> Result<CycleResult, RuntimeError> {
        self.runtime.cycle(
            &self.resource,
            &self.planner,
            &self.observer,
            &mut self.actuator,
        )
    }

    /// Execute the runtime's configured bounded control loop.
    ///
    /// # Errors
    ///
    /// Propagates runtime configuration or cycle failures. Cancellation is a
    /// normal stop reason and is represented in [`RunResult`].
    pub fn run(&mut self, cancellation: &CancellationToken) -> Result<RunResult, RuntimeError> {
        self.runtime.run(
            &self.resource,
            &self.planner,
            &self.observer,
            &mut self.actuator,
            cancellation,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RuntimeConfig, RuntimeMode, TransactionalRam};
    use elastic_adapters::HeadroomPlanner;

    #[test]
    fn controller_owns_and_executes_a_real_transactional_resource() {
        let adapter =
            TransactionalRam::new("controller-ram", 4096, 512, 4096, 1024, Some(2048)).unwrap();
        let observer = adapter.clone();
        let actuator = adapter.clone();
        let resource = adapter.ir().unwrap();
        let planner = HeadroomPlanner::new(0.5, 0.0).unwrap();
        let runtime = Runtime::new(RuntimeConfig {
            mode: RuntimeMode::Apply,
            dry_run: false,
            ..RuntimeConfig::default()
        });
        let mut controller = Controller::new(runtime, resource, planner, observer, actuator);

        let result = controller.cycle().unwrap();

        assert!(result.commit.is_some());
        assert_eq!(adapter.committed().unwrap(), 2048);
    }
}
