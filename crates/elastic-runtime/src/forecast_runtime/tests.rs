use std::cell::{Cell, RefCell};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use elastic_core::resource::ObservationSignalId;
use elastic_eir::{EirResource, PlanOutcome, PlanningContext, TransitionPlanner};

use super::*;
use crate::{
    Actuation, CommitRecord, EwmaForecaster, InvariantCheck, ObservationSource, RollbackRecord,
    ValidatedPlan, VerificationResult,
};

struct FixedObserver(f64);

impl Observer for FixedObserver {
    fn observe(&self) -> (PlanningContext, Vec<Observation>) {
        let now = Instant::now();
        (
            PlanningContext::new().observe(ObservationSignalId::UTILIZATION, self.0),
            vec![Observation::from_source(
                ObservationSource::runtime("forecast-test"),
                ObservationSignalId::UTILIZATION,
                self.0,
                now,
            )],
        )
    }
}

struct SequenceObserver {
    values: Vec<f64>,
    index: Mutex<usize>,
}

impl Observer for SequenceObserver {
    fn observe(&self) -> (PlanningContext, Vec<Observation>) {
        let mut index = self.index.lock().unwrap();
        let value = self.values[(*index).min(self.values.len() - 1)];
        *index = index.saturating_add(1);
        FixedObserver(value).observe()
    }
}

struct OverrideForecaster(f64);

impl Forecaster for OverrideForecaster {
    fn forecast(
        &self,
        _observations: &ObservationSnapshot,
        _current: &PlanningContext,
    ) -> Result<Forecast, RuntimeError> {
        Ok(Forecast::available(
            PlanningContext::new().observe(ObservationSignalId::UTILIZATION, self.0),
            Duration::ZERO,
            "override-test",
            "test forecast context",
        ))
    }
}

struct UnsupportedForecaster;

impl Forecaster for UnsupportedForecaster {
    fn forecast(
        &self,
        _observations: &ObservationSnapshot,
        _current: &PlanningContext,
    ) -> Result<Forecast, RuntimeError> {
        Ok(Forecast::unsupported(
            Duration::from_secs(1),
            "unsupported-test",
            "forecast intentionally unavailable",
        ))
    }
}

struct FailingForecaster;

impl Forecaster for FailingForecaster {
    fn forecast(
        &self,
        _observations: &ObservationSnapshot,
        _current: &PlanningContext,
    ) -> Result<Forecast, RuntimeError> {
        Err(RuntimeError::planning("injected forecast failure"))
    }
}

struct RecordingPlanner {
    seen: RefCell<Vec<f64>>,
}

impl RecordingPlanner {
    fn new() -> Self {
        Self {
            seen: RefCell::new(Vec::new()),
        }
    }
}

impl TransitionPlanner for RecordingPlanner {
    fn propose_transition(&self, _resource: &EirResource) -> PlanOutcome {
        PlanOutcome::InsufficientEvidence {
            detail: "context required".to_owned(),
        }
    }

    fn propose_transition_with_context(
        &self,
        _resource: &EirResource,
        context: &PlanningContext,
    ) -> PlanOutcome {
        if let Some(value) = context.get(ObservationSignalId::UTILIZATION) {
            self.seen.borrow_mut().push(value);
        }
        PlanOutcome::NoCandidate
    }
}

struct CountingActuator {
    validations: Cell<usize>,
}

impl CountingActuator {
    fn new() -> Self {
        Self {
            validations: Cell::new(0),
        }
    }
}

impl TransactionalActuator for CountingActuator {
    fn name(&self) -> &str {
        "counting"
    }

    fn validate(&self, plan: &crate::Plan) -> Result<Vec<InvariantCheck>, RuntimeError> {
        self.validations.set(self.validations.get() + 1);
        Ok(plan
            .resource
            .invariants()
            .iter()
            .cloned()
            .map(|invariant| InvariantCheck::new(invariant, true, None))
            .collect())
    }

    fn prepare(&mut self, plan: &ValidatedPlan) -> Result<Actuation, RuntimeError> {
        Ok(Actuation::new(plan.clone(), None, self.name()))
    }

    fn actuate(&mut self, _actuation: &Actuation) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn verify(&self, _actuation: &Actuation) -> Result<VerificationResult, RuntimeError> {
        Ok(VerificationResult::Pass)
    }

    fn commit(&mut self, _actuation: &Actuation) -> Result<CommitRecord, RuntimeError> {
        Ok(CommitRecord::new(self.name(), "test commit"))
    }

    fn rollback(
        &mut self,
        _actuation: &Actuation,
        _verification: &VerificationResult,
    ) -> Result<RollbackRecord, RuntimeError> {
        Ok(RollbackRecord::new(self.name(), "test rollback", true))
    }
}

struct FailingRollbackActuator;

impl TransactionalActuator for FailingRollbackActuator {
    fn name(&self) -> &str {
        "forecast-failing-rollback"
    }

    fn validate(&self, plan: &crate::Plan) -> Result<Vec<InvariantCheck>, RuntimeError> {
        Ok(plan
            .resource
            .invariants()
            .iter()
            .cloned()
            .map(|invariant| InvariantCheck::new(invariant, true, None))
            .collect())
    }

    fn prepare(&mut self, plan: &ValidatedPlan) -> Result<Actuation, RuntimeError> {
        let target = plan
            .plan
            .candidate()
            .and_then(|candidate| candidate.magnitude());
        Ok(Actuation::new(plan.clone(), target, self.name()))
    }

    fn actuate(&mut self, _actuation: &Actuation) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn verify(&self, _actuation: &Actuation) -> Result<VerificationResult, RuntimeError> {
        Ok(VerificationResult::Fail {
            detail: "injected transaction verification failure".to_owned(),
        })
    }

    fn commit(&mut self, _actuation: &Actuation) -> Result<CommitRecord, RuntimeError> {
        Err(RuntimeError::commit("commit must not be reached"))
    }

    fn rollback(
        &mut self,
        _actuation: &Actuation,
        _verification: &VerificationResult,
    ) -> Result<RollbackRecord, RuntimeError> {
        Err(RuntimeError::rollback("injected transaction rollback failure"))
    }
}

#[derive(Default)]
struct FakeClock {
    sleeps: Mutex<Vec<Duration>>,
}

impl RuntimeClock for FakeClock {
    fn sleep(&self, duration: Duration) {
        self.sleeps.lock().unwrap().push(duration);
    }
}

#[test]
fn planner_consumes_forecast_context_not_raw_observation() {
    let runtime = ForecastRuntime::new(
        Runtime::new(crate::RuntimeConfig::default()),
        OverrideForecaster(0.9),
    );
    let resource = runtime.runtime().config().ir_resource.clone();
    let planner = RecordingPlanner::new();
    let mut actuator = CountingActuator::new();

    let result = runtime
        .cycle(&resource, &planner, &FixedObserver(0.1), &mut actuator)
        .unwrap();

    assert_eq!(planner.seen.borrow().as_slice(), &[0.9]);
    assert_eq!(result.forecast.as_ref().unwrap().method, "override-test");
    assert_eq!(actuator.validations.get(), 0);
}

#[test]
fn unavailable_forecast_gates_even_context_free_planner() {
    let runtime = ForecastRuntime::new(
        Runtime::new(crate::RuntimeConfig::default()),
        UnsupportedForecaster,
    );
    let resource = runtime.runtime().config().ir_resource.clone();
    let mut actuator = CountingActuator::new();

    let result = runtime
        .cycle(
            &resource,
            &elastic_eir::FirstGroundedPlanner,
            &FixedObserver(0.5),
            &mut actuator,
        )
        .unwrap();

    assert_eq!(actuator.validations.get(), 0);
    assert!(result.transaction.actuation.is_none());
    assert!(result
        .forecast_event
        .as_ref()
        .is_some_and(|event| event.kind == RuntimeEventKind::ForecastGenerated));
    assert!(result
        .transaction
        .plan
        .as_ref()
        .is_some_and(|plan| plan.plan.candidate().is_none()));
}

#[test]
fn forecast_failure_attempt_preserves_input_before_transaction_entry() {
    let runtime = ForecastRuntime::new(
        Runtime::new(crate::RuntimeConfig::default()),
        FailingForecaster,
    );
    let resource = runtime.runtime().config().ir_resource.clone();
    let planner = RecordingPlanner::new();
    let mut actuator = CountingActuator::new();

    let attempt = runtime.cycle_attempt(
        &resource,
        &planner,
        &FixedObserver(0.4),
        &mut actuator,
    );

    let ForecastCycleAttempt::Failed(failure) = attempt else {
        panic!("forecast error must remain a failed forecast attempt")
    };
    match *failure {
        ForecastCycleFailure::Forecast {
            resource: failed_resource,
            forecast_input,
            error,
        } => {
            assert_eq!(failed_resource.fingerprint(), resource.fingerprint());
            assert_eq!(forecast_input.len(), 1);
            assert!(matches!(error, RuntimeError::Planning(_)));
            assert_eq!(actuator.validations.get(), 0);
        }
        ForecastCycleFailure::Transaction { .. } => {
            panic!("forecast failure must not be represented as transaction failure")
        }
    }
}

#[test]
fn transaction_failure_attempt_preserves_forecast_and_runtime_audit() {
    let trusted = Runtime::new(crate::RuntimeConfig {
        mode: RuntimeMode::Apply,
        dry_run: false,
        ..crate::RuntimeConfig::default()
    });
    let runtime = ForecastRuntime::new(trusted, OverrideForecaster(0.9));
    let resource = runtime.runtime().config().ir_resource.clone();
    let mut actuator = FailingRollbackActuator;

    let attempt = runtime.cycle_attempt(
        &resource,
        &elastic_eir::FirstGroundedPlanner,
        &FixedObserver(0.2),
        &mut actuator,
    );

    let ForecastCycleAttempt::Failed(failure) = attempt else {
        panic!("rollback error must remain a failed forecast transaction attempt")
    };
    match *failure {
        ForecastCycleFailure::Transaction {
            forecast_input,
            forecast,
            forecast_event,
            failure,
        } => {
            assert_eq!(forecast_input.as_ref().map(ObservationSnapshot::len), Some(1));
            assert_eq!(forecast.as_ref().map(|value| value.method.as_str()), Some("override-test"));
            assert!(forecast_event
                .as_ref()
                .is_some_and(|event| event.kind == RuntimeEventKind::ForecastGenerated));
            assert!(matches!(failure.error, RuntimeError::Rollback(_)));
            assert_eq!(failure.resource.fingerprint(), resource.fingerprint());
            assert!(failure
                .events
                .iter()
                .any(|event| event.kind == RuntimeEventKind::ActuationApplied));
            assert!(failure
                .events
                .iter()
                .any(|event| event.kind == RuntimeEventKind::VerificationPerformed));
            assert!(!failure
                .events
                .iter()
                .any(|event| event.kind == RuntimeEventKind::RollbackExecuted));
        }
        ForecastCycleFailure::Forecast { .. } => {
            panic!("transaction rollback failure must preserve successful forecast context")
        }
    }
}

#[test]
fn ewma_state_is_reused_across_bounded_cycles() {
    let runtime = Runtime::new(crate::RuntimeConfig {
        cadence: Cadence::Periodic(Duration::from_millis(1)),
        mode: RuntimeMode::PlanOnly,
        max_cycles: 2,
        dry_run: false,
        ..crate::RuntimeConfig::default()
    });
    let forecast_runtime = ForecastRuntime::new(
        runtime,
        EwmaForecaster::new(0.5, Duration::from_secs(1)).unwrap(),
    );
    let resource = forecast_runtime.runtime().config().ir_resource.clone();
    let planner = RecordingPlanner::new();
    let observer = SequenceObserver {
        values: vec![0.0, 1.0],
        index: Mutex::new(0),
    };
    let mut actuator = CountingActuator::new();
    let cancellation = CancellationToken::new();
    let clock = FakeClock::default();

    let result = forecast_runtime
        .run_with_clock(
            &resource,
            &planner,
            &observer,
            &mut actuator,
            &cancellation,
            &clock,
        )
        .unwrap();

    assert_eq!(result.cycles.len(), 2);
    assert_eq!(planner.seen.borrow().as_slice(), &[0.0, 0.5]);
    assert_eq!(result.stop_reason, LoopStopReason::MaxCyclesReached);
    assert_eq!(
        clock.sleeps.lock().unwrap().as_slice(),
        &[Duration::from_millis(1)]
    );
}
