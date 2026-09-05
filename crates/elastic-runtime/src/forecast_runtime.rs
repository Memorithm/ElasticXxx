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
    Cadence, CancellationToken, Controller, CurrentStateForecaster, CycleAttempt, CycleFailure,
    CycleResult, Forecast, Forecaster, LoopStopReason, Observation, ObservationSnapshot, Observer,
    Runtime, RuntimeClock, RuntimeError, RuntimeEvent, RuntimeEventKind, RuntimeMode, SystemClock,
    TransactionalActuator,
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

/// One completed or failed forecast-aware cycle attempt.
#[derive(Debug)]
pub enum ForecastCycleAttempt {
    /// Forecasting and the trusted runtime completed normally.
    Completed(Box<ForecastCycleResult>),
    /// The attempt failed either before entering the trusted transaction or
    /// inside the trusted transaction.
    Failed(Box<ForecastCycleFailure>),
}

impl ForecastCycleAttempt {
    /// Convert back to the existing `Result<ForecastCycleResult, RuntimeError>`
    /// surface without changing success/error semantics.
    pub fn into_result(self) -> Result<ForecastCycleResult, RuntimeError> {
        match self {
            Self::Completed(result) => Ok(*result),
            Self::Failed(failure) => Err(failure.into_error()),
        }
    }

    #[must_use]
    pub const fn is_completed(&self) -> bool {
        matches!(self, Self::Completed(_))
    }
}

/// Auditable failure phase for a forecast-aware cycle.
#[derive(Debug)]
pub enum ForecastCycleFailure {
    /// The forecaster returned an error before the trusted runtime was entered.
    Forecast {
        /// Exact resource for which forecasting was attempted.
        resource: EirResource,
        /// Raw observation snapshot supplied to the forecaster.
        forecast_input: ObservationSnapshot,
        /// Authoritative forecast/runtime error.
        error: RuntimeError,
    },
    /// Forecasting completed and the trusted runtime later returned an error.
    Transaction {
        /// Raw observation snapshot supplied to the forecaster. `None` is used
        /// only in `ObserveOnly`, where forecasting is intentionally skipped.
        forecast_input: Option<ObservationSnapshot>,
        /// Forecast that gated planning, absent only in `ObserveOnly`.
        forecast: Option<Forecast>,
        /// Audit event describing the forecast, absent only in `ObserveOnly`.
        forecast_event: Option<RuntimeEvent>,
        /// Exact trusted-runtime failure audit retained by [`CycleFailure`].
        failure: Box<CycleFailure>,
    },
}

impl ForecastCycleFailure {
    /// Authoritative error that terminated this cycle attempt.
    #[must_use]
    pub const fn error(&self) -> &RuntimeError {
        match self {
            Self::Forecast { error, .. } => error,
            Self::Transaction { failure, .. } => &failure.error,
        }
    }

    fn into_error(self) -> RuntimeError {
        match self {
            Self::Forecast { error, .. } => error,
            Self::Transaction { failure, .. } => failure.error,
        }
    }

    fn append_events(&self, events: &mut Vec<RuntimeEvent>) {
        if let Self::Transaction {
            forecast_event,
            failure,
            ..
        } = self
        {
            if let Some(event) = forecast_event {
                events.push(event.clone());
            }
            events.extend(failure.events.iter().cloned());
        }
    }
}

/// Result of a bounded forecast-aware controller invocation.
#[derive(Clone, Debug)]
pub struct ForecastRunResult {
    pub cycles: Vec<ForecastCycleResult>,
    pub events: Vec<RuntimeEvent>,
    pub stop_reason: LoopStopReason,
}

/// Completed or failed attempt to execute a bounded forecast-aware run.
#[derive(Debug)]
pub enum ForecastRunAttempt {
    /// The bounded run completed or was cooperatively cancelled.
    Completed(Box<ForecastRunResult>),
    /// The run failed before normal completion.
    Failed(Box<ForecastRunFailure>),
}

impl ForecastRunAttempt {
    /// Convert back to the historical `Result<ForecastRunResult, RuntimeError>`
    /// surface without changing the returned error.
    pub fn into_result(self) -> Result<ForecastRunResult, RuntimeError> {
        match self {
            Self::Completed(result) => Ok(*result),
            Self::Failed(failure) => Err(failure.into_error()),
        }
    }

    #[must_use]
    pub const fn is_completed(&self) -> bool {
        matches!(self, Self::Completed(_))
    }
}

/// Auditable failure of a bounded forecast-aware run.
#[derive(Debug)]
pub enum ForecastRunFailure {
    /// Run configuration was invalid before the control loop started.
    Setup {
        /// Authoritative configuration/runtime error.
        error: RuntimeError,
    },
    /// A cycle failed after zero or more earlier cycles completed.
    Cycle {
        /// Completed cycles retained in original execution order.
        completed_cycles: Vec<ForecastCycleResult>,
        /// Ordered run and cycle events retained up to the stop decision.
        events: Vec<RuntimeEvent>,
        /// Exact failed cycle attempt.
        failed_cycle: Box<ForecastCycleFailure>,
    },
}

impl ForecastRunFailure {
    /// Authoritative error that terminated this run attempt.
    #[must_use]
    pub const fn error(&self) -> &RuntimeError {
        match self {
            Self::Setup { error } => error,
            Self::Cycle { failed_cycle, .. } => failed_cycle.error(),
        }
    }

    fn into_error(self) -> RuntimeError {
        match self {
            Self::Setup { error } => error,
            Self::Cycle { failed_cycle, .. } => failed_cycle.into_error(),
        }
    }
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
        self.cycle_attempt(resource, planner, observer, actuator)
            .into_result()
    }

    /// Execute one forecast-aware cycle while preserving the exact failure phase
    /// and the audit context that exists at that phase.
    ///
    /// This method owns the single forecast orchestration path used by
    /// [`ForecastRuntime::cycle`]. The trusted transaction itself is still
    /// executed only by [`Runtime::cycle_attempt`]/`Runtime::cycle_with_sink`.
    pub fn cycle_attempt<P, O, A>(
        &self,
        resource: &EirResource,
        planner: &P,
        observer: &O,
        actuator: &mut A,
    ) -> ForecastCycleAttempt
    where
        P: TransitionPlanner,
        O: Observer,
        A: TransactionalActuator,
    {
        if matches!(self.runtime.config().mode, RuntimeMode::ObserveOnly) {
            return match self
                .runtime
                .cycle_attempt(resource, planner, observer, actuator)
            {
                CycleAttempt::Completed(transaction) => {
                    ForecastCycleAttempt::Completed(Box::new(ForecastCycleResult {
                        forecast: None,
                        forecast_event: None,
                        transaction: *transaction,
                    }))
                }
                CycleAttempt::Failed(failure) => {
                    ForecastCycleAttempt::Failed(Box::new(ForecastCycleFailure::Transaction {
                        forecast_input: None,
                        forecast: None,
                        forecast_event: None,
                        failure,
                    }))
                }
            };
        }

        let (current, observations) = observer.observe();
        let forecast_input = ObservationSnapshot::new(Instant::now(), observations.clone());
        let forecast = match self.forecaster.forecast(&forecast_input, &current) {
            Ok(forecast) => forecast,
            Err(error) => {
                return ForecastCycleAttempt::Failed(Box::new(ForecastCycleFailure::Forecast {
                    resource: resource.clone(),
                    forecast_input,
                    error,
                }));
            }
        };
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

        match self
            .runtime
            .cycle_attempt(resource, &gated_planner, &frozen_observer, actuator)
        {
            CycleAttempt::Completed(transaction) => {
                ForecastCycleAttempt::Completed(Box::new(ForecastCycleResult {
                    forecast: Some(forecast),
                    forecast_event: Some(forecast_event),
                    transaction: *transaction,
                }))
            }
            CycleAttempt::Failed(failure) => {
                ForecastCycleAttempt::Failed(Box::new(ForecastCycleFailure::Transaction {
                    forecast_input: Some(forecast_input),
                    forecast: Some(forecast),
                    forecast_event: Some(forecast_event),
                    failure,
                }))
            }
        }
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
        self.run_attempt(resource, planner, observer, actuator, cancellation)
            .into_result()
    }

    /// Execute a bounded forecast-aware loop while retaining a failed cycle and
    /// all earlier completed cycles.
    pub fn run_attempt<P, O, A>(
        &self,
        resource: &EirResource,
        planner: &P,
        observer: &O,
        actuator: &mut A,
        cancellation: &CancellationToken,
    ) -> ForecastRunAttempt
    where
        P: TransitionPlanner,
        O: Observer,
        A: TransactionalActuator,
    {
        self.run_with_clock_attempt(
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
        self.run_with_clock_attempt(resource, planner, observer, actuator, cancellation, clock)
            .into_result()
    }

    /// Execute the single bounded forecast-aware loop implementation while
    /// retaining setup or cycle failure audit state.
    pub fn run_with_clock_attempt<P, O, A, C>(
        &self,
        resource: &EirResource,
        planner: &P,
        observer: &O,
        actuator: &mut A,
        cancellation: &CancellationToken,
        clock: &C,
    ) -> ForecastRunAttempt
    where
        P: TransitionPlanner,
        O: Observer,
        A: TransactionalActuator,
        C: RuntimeClock,
    {
        let (cycle_limit, interval) = match forecast_loop_schedule(self.runtime.config()) {
            Ok(schedule) => schedule,
            Err(error) => {
                return ForecastRunAttempt::Failed(Box::new(ForecastRunFailure::Setup { error }));
            }
        };
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
                return ForecastRunAttempt::Completed(Box::new(ForecastRunResult {
                    cycles,
                    events,
                    stop_reason: LoopStopReason::Cancelled,
                }));
            }

            match self.cycle_attempt(resource, planner, observer, actuator) {
                ForecastCycleAttempt::Completed(cycle) => {
                    let cycle = *cycle;
                    events.extend(cycle.events().cloned());
                    cycles.push(cycle);
                }
                ForecastCycleAttempt::Failed(failed_cycle) => {
                    let error_detail = failed_cycle.error().to_string();
                    failed_cycle.append_events(&mut events);
                    events.push(RuntimeEvent::new(
                        RuntimeEventKind::ErrorEncountered,
                        error_detail,
                    ));
                    events.push(RuntimeEvent::new(
                        RuntimeEventKind::ControlLoopStopped,
                        "forecast control loop stopped after cycle error",
                    ));
                    return ForecastRunAttempt::Failed(Box::new(ForecastRunFailure::Cycle {
                        completed_cycles: cycles,
                        events,
                        failed_cycle,
                    }));
                }
            }

            if cancellation.is_cancelled() {
                events.push(RuntimeEvent::new(
                    RuntimeEventKind::CancellationObserved,
                    "cancellation observed after completed forecast cycle",
                ));
                events.push(RuntimeEvent::new(
                    RuntimeEventKind::ControlLoopStopped,
                    "forecast control loop cancelled",
                ));
                return ForecastRunAttempt::Completed(Box::new(ForecastRunResult {
                    cycles,
                    events,
                    stop_reason: LoopStopReason::Cancelled,
                }));
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
                return ForecastRunAttempt::Completed(Box::new(ForecastRunResult {
                    cycles,
                    events,
                    stop_reason,
                }));
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

    pub fn cycle_attempt(&mut self) -> ForecastCycleAttempt {
        self.runtime.cycle_attempt(
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

    pub fn run_attempt(&mut self, cancellation: &CancellationToken) -> ForecastRunAttempt {
        self.runtime.run_attempt(
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
mod tests;
