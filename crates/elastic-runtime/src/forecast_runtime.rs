//! Forecast-aware orchestration over the trusted transactional runtime.
//!
//! [`Runtime`] remains the single executor for validation, physical actuation,
//! verification, commit, and rollback. This module adds the missing
//! `OBSERVE -> FORECAST -> PLAN` boundary without duplicating transaction
//! semantics: observations are collected once, a [`Forecaster`] produces the
//! planner-facing context, and a gate prevents the planner from proposing any
//! transition when forecast evidence is unavailable.

use std::time::{Duration, Instant};

use elastic_eir::{EirResource, PlanOutcome, PlanningContext, TransitionPlanner};

use crate::{
    Cadence, CancellationToken, Controller, CurrentStateForecaster, CycleResult, Forecast,
    Forecaster, LoopStopReason, Observation, ObservationSnapshot, Observer, Runtime, RuntimeClock,
    RuntimeError, RuntimeEvent, RuntimeEventKind, RuntimeMode, SystemClock, TransactionalActuator,
};

#[derive(Clone, Debug)]
struct FrozenObserver {
    context: PlanningContext,
    observations: Vec<Observation>,
}

impl FrozenObserver {
    fn new(context: PlanningContext, observations: Vec<Observation>) -> Self {
        Self {
            context,
            observations,
        }
    }
}

impl Observer for FrozenObserver {
    fn observe(&self) -> (PlanningContext, Vec<Observation>) {
        (self.context.clone(), self.observations.clone())
    }
}

struct ForecastGatePlanner<'a, P> {
    planner: &'a P,
    available: bool,
    unavailable_detail: String,
}

impl<P> ForecastGatePlanner<'_, P>
where
    P: TransitionPlanner,
{
    fn unavailable(&self) -> PlanOutcome {
        PlanOutcome::InsufficientEvidence {
            detail: self.unavailable_detail.clone(),
        }
    }
}

impl<P> TransitionPlanner for ForecastGatePlanner<'_, P>
where
    P: TransitionPlanner,
{
    fn propose_transition(&self, resource: &EirResource) -> PlanOutcome {
        if self.available {
            self.planner.propose_transition(resource)
        } else {
            self.unavailable()
        }
    }

    fn propose_transition_with_context(
        &self,
        resource: &EirResource,
        context: &PlanningContext,
    ) -> PlanOutcome {
        if self.available {
            self.planner
                .propose_transition_with_context(resource, context)
        } else {
            self.unavailable()
        }
    }
}

/// One forecast-aware cycle.
///
/// `transaction` is the authoritative trusted runtime result. `forecast` is
/// `None` only in `ObserveOnly` mode, where forecasting and planning are
/// intentionally skipped.
#[derive(Clone, Debug)]
pub struct ForecastCycleResult {
    pub forecast: Option<Forecast>,
    pub forecast_event: Option<RuntimeEvent>,
    pub transaction: CycleResult,
}

impl ForecastCycleResult {
    /// Iterate the forecast event followed by the trusted transaction events.
    pub fn events(&self) -> impl Iterator<Item = &RuntimeEvent> {
        self.forecast_event
            .iter()
            .chain(self.transaction.events.iter())
    }
}

/// Result of a bounded forecast-aware controller invocation.
#[derive(Clone, Debug)]
pub struct ForecastRunResult {
    pub cycles: Vec<ForecastCycleResult>,
    pub events: Vec<RuntimeEvent>,
    pub stop_reason: LoopStopReason,
}

/// Forecast-aware orchestration layer around a trusted [`Runtime`].
#[derive(Debug)]
pub struct ForecastRuntime<F> {
    runtime: Runtime,
    forecaster: F,
}

impl ForecastRuntime<CurrentStateForecaster> {
    #[must_use]
    pub fn current_state(runtime: Runtime) -> Self {
        Self::new(runtime, CurrentStateForecaster)
    }
}

impl<F> ForecastRuntime<F> {
    #[must_use]
    pub const fn new(runtime: Runtime, forecaster: F) -> Self {
        Self {
            runtime,
            forecaster,
        }
    }

    #[must_use]
    pub const fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    #[must_use]
    pub const fn forecaster(&self) -> &F {
        &self.forecaster
    }

    #[must_use]
    pub fn into_parts(self) -> (Runtime, F) {
        (self.runtime, self.forecaster)
    }
}

impl<F> ForecastRuntime<F>
where
    F: Forecaster,
{
    /// Execute one `OBSERVE -> FORECAST -> PLAN -> trusted transaction` cycle.
    ///
    /// # Errors
    ///
    /// Returns observation/forecast/runtime errors. A forecast error occurs
    /// before the trusted runtime is entered, so it cannot leave a partial
    /// physical transition. Unsupported or inconclusive forecasts are normal
    /// fail-closed outcomes and yield a transaction with no candidate.
    pub fn cycle<P, O, A>(
        &self,
        resource: &EirResource,
        planner: &P,
        observer: &O,
        actuator: &mut A,
    ) -> Result<ForecastCycleResult, RuntimeError>
    where
        P: TransitionPlanner,
        O: Observer,
        A: TransactionalActuator,
    {
        if matches!(self.runtime.config().mode, RuntimeMode::ObserveOnly) {
            let transaction = self.runtime.cycle(resource, planner, observer, actuator)?;
            return Ok(ForecastCycleResult {
                forecast: None,
                forecast_event: None,
                transaction,
            });
        }

        let (current, observations) = observer.observe();
        let snapshot = ObservationSnapshot::new(Instant::now(), observations.clone());
        let forecast = self.forecaster.forecast(&snapshot, &current)?;
        let available = forecast.is_available();
        let context = forecast
            .planning_context()
            .cloned()
            .unwrap_or_else(PlanningContext::new);
        let unavailable_detail = format!(
            "forecast status {:?} from '{}' did not provide planner-authorizing evidence",
            forecast.status, forecast.method
        );
        let gated_planner = ForecastGatePlanner {
            planner,
            available,
            unavailable_detail,
        };
        let frozen_observer = FrozenObserver::new(context, observations);
        let forecast_event = RuntimeEvent::new(
            RuntimeEventKind::ForecastGenerated,
            format!(
                "method={} status={:?} horizon_ms={} confidence={:?}",
                forecast.method,
                forecast.status,
                forecast.horizon.as_millis(),
                forecast.confidence
            ),
        );

        let transaction =
            self.runtime
                .cycle(resource, &gated_planner, &frozen_observer, actuator)?;

        Ok(ForecastCycleResult {
            forecast: Some(forecast),
            forecast_event: Some(forecast_event),
            transaction,
        })
    }

    /// Execute a bounded forecast-aware loop using the system clock.
    pub fn run<P, O, A>(
        &self,
        resource: &EirResource,
        planner: &P,
        observer: &O,
        actuator: &mut A,
        cancellation: &CancellationToken,
    ) -> Result<ForecastRunResult, RuntimeError>
    where
        P: TransitionPlanner,
        O: Observer,
        A: TransactionalActuator,
    {
        self.run_with_clock(
            resource,
            planner,
            observer,
            actuator,
            cancellation,
            &SystemClock,
        )
    }

    /// Execute a bounded forecast-aware loop with an injectable clock.
    pub fn run_with_clock<P, O, A, C>(
        &self,
        resource: &EirResource,
        planner: &P,
        observer: &O,
        actuator: &mut A,
        cancellation: &CancellationToken,
        clock: &C,
    ) -> Result<ForecastRunResult, RuntimeError>
    where
        P: TransitionPlanner,
        O: Observer,
        A: TransactionalActuator,
        C: RuntimeClock,
    {
        let (cycle_limit, interval) = forecast_loop_schedule(self.runtime.config())?;
        let mut cycles = Vec::new();
        let mut events = vec![RuntimeEvent::new(
            RuntimeEventKind::ControlLoopStarted,
            format!("bounded forecast control loop started with max_cycles={cycle_limit}"),
        )];

        loop {
            if cancellation.is_cancelled() {
                events.push(RuntimeEvent::new(
                    RuntimeEventKind::CancellationObserved,
                    "cancellation observed before next forecast cycle",
                ));
                events.push(RuntimeEvent::new(
                    RuntimeEventKind::ControlLoopStopped,
                    "forecast control loop cancelled",
                ));
                return Ok(ForecastRunResult {
                    cycles,
                    events,
                    stop_reason: LoopStopReason::Cancelled,
                });
            }

            let cycle = self.cycle(resource, planner, observer, actuator)?;
            events.extend(cycle.events().cloned());
            cycles.push(cycle);

            if cancellation.is_cancelled() {
                events.push(RuntimeEvent::new(
                    RuntimeEventKind::CancellationObserved,
                    "cancellation observed after completed forecast cycle",
                ));
                events.push(RuntimeEvent::new(
                    RuntimeEventKind::ControlLoopStopped,
                    "forecast control loop cancelled",
                ));
                return Ok(ForecastRunResult {
                    cycles,
                    events,
                    stop_reason: LoopStopReason::Cancelled,
                });
            }

            if cycles.len() as u64 >= cycle_limit {
                let stop_reason = if interval.is_some() {
                    LoopStopReason::MaxCyclesReached
                } else {
                    LoopStopReason::OneShotCompleted
                };
                events.push(RuntimeEvent::new(
                    RuntimeEventKind::ControlLoopStopped,
                    "bounded forecast control loop completed",
                ));
                return Ok(ForecastRunResult {
                    cycles,
                    events,
                    stop_reason,
                });
            }

            if let Some(interval) = interval {
                clock.sleep(interval);
            }
        }
    }
}

fn forecast_loop_schedule(
    config: &crate::RuntimeConfig,
) -> Result<(u64, Option<Duration>), RuntimeError> {
    let cadence_interval = match &config.cadence {
        Cadence::OneShot => None,
        Cadence::Periodic(interval) => Some(*interval),
    };
    let interval = cadence_interval.or_else(|| {
        matches!(&config.mode, RuntimeMode::Periodic)
            .then(|| Duration::from_millis(config.interval_ms))
    });

    let Some(interval) = interval else {
        return Ok((1, None));
    };
    if interval.is_zero() {
        return Err(RuntimeError::configuration(
            "periodic forecast control-loop interval must be greater than zero",
        ));
    }
    if config.max_cycles == 0 {
        return Err(RuntimeError::configuration(
            "periodic forecast control loops must set max_cycles > 0",
        ));
    }
    Ok((config.max_cycles, Some(interval)))
}

/// High-level controller that owns a forecaster across cycles.
///
/// Stateful forecasters such as EWMA therefore retain history for the entire
/// bounded controller run rather than being reconstructed per iteration.
#[derive(Debug)]
pub struct ForecastController<P, O, A, F> {
    runtime: ForecastRuntime<F>,
    resource: EirResource,
    planner: P,
    observer: O,
    actuator: A,
}

impl<P, O, A, F> ForecastController<P, O, A, F> {
    #[must_use]
    pub const fn new(
        runtime: Runtime,
        resource: EirResource,
        planner: P,
        observer: O,
        actuator: A,
        forecaster: F,
    ) -> Self {
        Self {
            runtime: ForecastRuntime::new(runtime, forecaster),
            resource,
            planner,
            observer,
            actuator,
        }
    }

    #[must_use]
    pub const fn forecast_runtime(&self) -> &ForecastRuntime<F> {
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
}

impl<P, O, A, F> ForecastController<P, O, A, F>
where
    P: TransitionPlanner,
    O: Observer,
    A: TransactionalActuator,
    F: Forecaster,
{
    pub fn cycle(&mut self) -> Result<ForecastCycleResult, RuntimeError> {
        self.runtime.cycle(
            &self.resource,
            &self.planner,
            &self.observer,
            &mut self.actuator,
        )
    }

    pub fn run(
        &mut self,
        cancellation: &CancellationToken,
    ) -> Result<ForecastRunResult, RuntimeError> {
        self.runtime.run(
            &self.resource,
            &self.planner,
            &self.observer,
            &mut self.actuator,
            cancellation,
        )
    }
}

impl<P, O, A> Controller<P, O, A> {
    /// Upgrade an existing compatibility controller to explicit forecast-aware
    /// orchestration without rebuilding its owned components.
    #[must_use]
    pub fn with_forecaster<F>(self, forecaster: F) -> ForecastController<P, O, A, F> {
        let (runtime, resource, planner, observer, actuator) = self.into_parts();
        ForecastController::new(runtime, resource, planner, observer, actuator, forecaster)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::sync::Mutex;

    use elastic_core::resource::ObservationSignalId;

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
        index: Cell<usize>,
    }

    impl Observer for SequenceObserver {
        fn observe(&self) -> (PlanningContext, Vec<Observation>) {
            let index = self.index.get();
            let value = self.values[index.min(self.values.len() - 1)];
            self.index.set(index.saturating_add(1));
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
            index: Cell::new(0),
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
}
