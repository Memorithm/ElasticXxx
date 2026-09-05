use std::cell::Cell;
use std::time::{Duration, Instant};

use elastic::{
    Actuation, Cadence, CancellationToken, CommitRecord, FirstGroundedPlanner, Forecast,
    ForecastRunAttempt, ForecastRunFailure, Forecaster, InvariantCheck, Observation,
    ObservationSignalId, ObservationSnapshot, ObservationSource, Observer, PlanningContext,
    RollbackRecord, Runtime, RuntimeClock, RuntimeConfig, RuntimeError, RuntimeEventKind,
    RuntimeMode, TransactionalActuator, ValidatedPlan, VerificationResult,
};

struct FixedObserver;

impl Observer for FixedObserver {
    fn observe(&self) -> (PlanningContext, Vec<Observation>) {
        let now = Instant::now();
        (
            PlanningContext::new().observe(ObservationSignalId::UTILIZATION, 0.5),
            vec![Observation::from_source(
                ObservationSource::runtime("forecast-run-attempt-test"),
                ObservationSignalId::UTILIZATION,
                0.5,
                now,
            )],
        )
    }
}

struct FixedForecaster;

impl Forecaster for FixedForecaster {
    fn forecast(
        &self,
        _observations: &ObservationSnapshot,
        _current: &PlanningContext,
    ) -> Result<Forecast, RuntimeError> {
        Ok(Forecast::available(
            PlanningContext::new().observe(ObservationSignalId::UTILIZATION, 0.5),
            Duration::ZERO,
            "forecast-run-attempt-test",
            "fixed test forecast",
        ))
    }
}

struct SecondCycleFailingActuator {
    verifications: Cell<usize>,
}

impl SecondCycleFailingActuator {
    fn new() -> Self {
        Self {
            verifications: Cell::new(0),
        }
    }
}

impl TransactionalActuator for SecondCycleFailingActuator {
    fn name(&self) -> &str {
        "second-cycle-failing"
    }

    fn validate(&self, plan: &elastic::Plan) -> Result<Vec<InvariantCheck>, RuntimeError> {
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
        let call = self.verifications.get();
        self.verifications.set(call + 1);
        if call == 0 {
            Ok(VerificationResult::Pass)
        } else {
            Ok(VerificationResult::Fail {
                detail: "injected second-cycle verification failure".to_owned(),
            })
        }
    }

    fn commit(&mut self, _actuation: &Actuation) -> Result<CommitRecord, RuntimeError> {
        Ok(CommitRecord::new(self.name(), "verified first cycle"))
    }

    fn rollback(
        &mut self,
        _actuation: &Actuation,
        _verification: &VerificationResult,
    ) -> Result<RollbackRecord, RuntimeError> {
        Err(RuntimeError::rollback(
            "injected second-cycle rollback failure",
        ))
    }
}

struct NoopClock;

impl RuntimeClock for NoopClock {
    fn sleep(&self, _duration: Duration) {}
}

#[test]
fn bounded_run_attempt_retains_completed_cycles_and_failed_cycle_audit() {
    let runtime = Runtime::new(RuntimeConfig {
        cadence: Cadence::Periodic(Duration::from_millis(1)),
        mode: RuntimeMode::Apply,
        max_cycles: 3,
        dry_run: false,
        ..RuntimeConfig::default()
    });
    let forecast_runtime = elastic::ForecastRuntime::new(runtime, FixedForecaster);
    let resource = forecast_runtime.runtime().config().ir_resource.clone();
    let mut actuator = SecondCycleFailingActuator::new();

    let attempt = forecast_runtime.run_with_clock_attempt(
        &resource,
        &FirstGroundedPlanner,
        &FixedObserver,
        &mut actuator,
        &CancellationToken::new(),
        &NoopClock,
    );

    let ForecastRunAttempt::Failed(failure) = attempt else {
        panic!("second-cycle rollback failure must retain a failed run attempt")
    };

    match *failure {
        ForecastRunFailure::Setup { .. } => {
            panic!("runtime cycle failure must not be reported as setup failure")
        }
        ForecastRunFailure::Cycle {
            completed_cycles,
            events,
            failed_cycle,
        } => {
            assert_eq!(completed_cycles.len(), 1);
            assert!(completed_cycles[0].transaction.commit.is_some());
            assert!(matches!(failed_cycle.error(), RuntimeError::Rollback(_)));
            assert!(events
                .iter()
                .any(|event| event.kind == RuntimeEventKind::ControlLoopStarted));
            assert!(events
                .iter()
                .any(|event| event.kind == RuntimeEventKind::CycleCompleted));
            assert!(events
                .iter()
                .any(|event| event.kind == RuntimeEventKind::VerificationPerformed));
            assert!(events
                .iter()
                .any(|event| event.kind == RuntimeEventKind::ErrorEncountered));
            assert!(events
                .iter()
                .any(|event| event.kind == RuntimeEventKind::ControlLoopStopped));
            assert!(!events
                .iter()
                .any(|event| event.kind == RuntimeEventKind::RollbackExecuted));
        }
    }
}

#[test]
fn invalid_bounded_run_schedule_is_setup_failure_without_fake_cycle() {
    let runtime = Runtime::new(RuntimeConfig {
        cadence: Cadence::Periodic(Duration::ZERO),
        mode: RuntimeMode::PlanOnly,
        max_cycles: 2,
        dry_run: true,
        ..RuntimeConfig::default()
    });
    let forecast_runtime = elastic::ForecastRuntime::new(runtime, FixedForecaster);
    let resource = forecast_runtime.runtime().config().ir_resource.clone();
    let mut actuator = SecondCycleFailingActuator::new();

    let attempt = forecast_runtime.run_with_clock_attempt(
        &resource,
        &FirstGroundedPlanner,
        &FixedObserver,
        &mut actuator,
        &CancellationToken::new(),
        &NoopClock,
    );

    let ForecastRunAttempt::Failed(failure) = attempt else {
        panic!("zero periodic interval must fail before the first cycle")
    };
    match *failure {
        ForecastRunFailure::Setup { error } => {
            assert!(matches!(error, RuntimeError::Configuration(_)));
        }
        ForecastRunFailure::Cycle { .. } => {
            panic!("invalid schedule must not fabricate a failed cycle")
        }
    }
}
