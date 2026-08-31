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
mod tests;
