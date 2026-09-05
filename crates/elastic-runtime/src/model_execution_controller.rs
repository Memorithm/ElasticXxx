//! High-level assembly for adaptive transactional model execution.
//!
//! This module composes the existing qualified boundaries without replacing
//! any of them:
//!
//! `current profile + typed resource telemetry -> forecast -> adaptive plan
//! -> transactional model actuation`.
//!
//! Backends still own physical profile switching and telemetry collection.
//! ElasticXxx only wires those explicit contracts into the existing
//! [`ForecastController`] and trusted transaction lifecycle.

use elastic_adapters::{
    ModelExecutionAdaptivePlannerV1, ModelExecutionEnvelopePolicyV1,
    ModelExecutionProfileSetV1,
};
use elastic_eir::{PlanningContext, TransitionPlanner};

use crate::{
    CadenceConfig, CancellationToken, CurrentStateForecaster, ExecutionModeConfig, ForecastController,
    ForecastCycleResult, ForecastRunResult, Forecaster, ModelExecutionProfileBackendV1,
    ModelExecutionResourceObserverV1, ModelExecutionResourceTelemetryV1, Observation, Observer,
    ObserverSet, PlannerConfig, Runtime, RuntimeConfig, RuntimeError, TransactionalModelExecution,
};

/// Owned observation bundle for one adaptive model-execution controller.
///
/// The model actuator contributes the current profile rank. The typed resource
/// observer contributes `FREE_CAPACITY` and `UTILIZATION`. Composition delegates
/// to the existing ordered [`ObserverSet`], so duplicate-signal authority and
/// audit behavior remain defined in one place.
pub struct ModelExecutionObserverBundleV1<B, T> {
    model: TransactionalModelExecution<B>,
    resources: ModelExecutionResourceObserverV1<T>,
}

impl<B, T> ModelExecutionObserverBundleV1<B, T> {
    #[must_use]
    pub const fn new(
        model: TransactionalModelExecution<B>,
        resources: ModelExecutionResourceObserverV1<T>,
    ) -> Self {
        Self { model, resources }
    }

    #[must_use]
    pub const fn model(&self) -> &TransactionalModelExecution<B> {
        &self.model
    }

    #[must_use]
    pub const fn resources(&self) -> &ModelExecutionResourceObserverV1<T> {
        &self.resources
    }
}

impl<B, T> Observer for ModelExecutionObserverBundleV1<B, T>
where
    B: ModelExecutionProfileBackendV1,
    T: ModelExecutionResourceTelemetryV1,
{
    fn observe(&self) -> (PlanningContext, Vec<Observation>) {
        let mut observers = ObserverSet::new();
        observers.push(&self.model);
        observers.push(&self.resources);
        observers.observe()
    }
}

/// Fully assembled adaptive model-execution controller.
///
/// This is an ergonomic owner around the generic [`ForecastController`]. It
/// does not add a new execution state machine: every physical change still
/// flows through [`TransactionalModelExecution`].
pub struct ModelExecutionControllerV1<B, T, F> {
    inner: ForecastController<
        ModelExecutionAdaptivePlannerV1,
        ModelExecutionObserverBundleV1<B, T>,
        TransactionalModelExecution<B>,
        F,
    >,
}

impl<B, T> ModelExecutionControllerV1<B, T, CurrentStateForecaster>
where
    B: ModelExecutionProfileBackendV1,
    T: ModelExecutionResourceTelemetryV1,
{
    /// Assemble a controller using the current-state forecaster.
    ///
    /// This is the direct operational path for backends that want decisions
    /// from the latest observed resource state without predictive smoothing.
    pub fn current_state(
        resource_id: &str,
        profiles: ModelExecutionProfileSetV1,
        policy: ModelExecutionEnvelopePolicyV1,
        backend: B,
        telemetry: T,
        cadence: CadenceConfig,
        mode: ExecutionModeConfig,
    ) -> Result<Self, RuntimeError> {
        Self::new(
            resource_id,
            profiles,
            policy,
            backend,
            telemetry,
            CurrentStateForecaster,
            cadence,
            mode,
        )
    }
}

impl<B, T, F> ModelExecutionControllerV1<B, T, F>
where
    B: ModelExecutionProfileBackendV1,
    T: ModelExecutionResourceTelemetryV1,
{
    /// Assemble one exact model backend, telemetry provider, adaptive policy,
    /// forecaster, and trusted transactional runtime.
    ///
    /// # Errors
    ///
    /// Fails closed when backend/profile identity is stale, the policy does not
    /// belong to the supplied profile set, the telemetry capacity unit cannot be
    /// bound to the policy, the resource declaration cannot lower, or periodic
    /// cadence is unbounded/zero interval.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        resource_id: &str,
        profiles: ModelExecutionProfileSetV1,
        policy: ModelExecutionEnvelopePolicyV1,
        backend: B,
        telemetry: T,
        forecaster: F,
        cadence: CadenceConfig,
        mode: ExecutionModeConfig,
    ) -> Result<Self, RuntimeError> {
        let planner = ModelExecutionAdaptivePlannerV1::new(policy, profiles.clone())
            .map_err(|error| RuntimeError::configuration(error.to_string()))?;
        let actuator = TransactionalModelExecution::new(resource_id, profiles.clone(), backend)?;
        let resource_observer =
            ModelExecutionResourceObserverV1::new(planner.capacity_unit(), telemetry)?;
        let observer = ModelExecutionObserverBundleV1::new(actuator.clone(), resource_observer);

        let spec = profiles
            .atomic_resource_spec(resource_id)
            .map_err(|error| RuntimeError::configuration(error.to_string()))?;
        let ir = actuator.ir();
        let (runtime_cadence, max_cycles) = cadence.runtime_values();
        let interval_ms = match cadence {
            CadenceConfig::OneShot => 0,
            CadenceConfig::Periodic {
                interval_ms,
                max_cycles,
            } => {
                if interval_ms == 0 {
                    return Err(RuntimeError::configuration(
                        "periodic model-execution controller requires interval_ms > 0",
                    ));
                }
                if max_cycles == 0 {
                    return Err(RuntimeError::configuration(
                        "periodic model-execution controller requires max_cycles > 0",
                    ));
                }
                interval_ms
            }
        };

        let runtime = Runtime::new(RuntimeConfig {
            resource_spec: spec,
            ir_resource: ir.clone(),
            planner_config: PlannerConfig::None,
            cadence: runtime_cadence,
            mode: mode.runtime_mode(),
            max_cycles,
            interval_ms,
            emit_events: true,
            dry_run: mode.dry_run(),
        });

        Ok(Self {
            inner: ForecastController::new(runtime, ir, planner, observer, actuator, forecaster),
        })
    }

    #[must_use]
    pub const fn inner(
        &self,
    ) -> &ForecastController<
        ModelExecutionAdaptivePlannerV1,
        ModelExecutionObserverBundleV1<B, T>,
        TransactionalModelExecution<B>,
        F,
    > {
        &self.inner
    }

    #[must_use]
    pub fn into_inner(
        self,
    ) -> ForecastController<
        ModelExecutionAdaptivePlannerV1,
        ModelExecutionObserverBundleV1<B, T>,
        TransactionalModelExecution<B>,
        F,
    > {
        self.inner
    }

    #[must_use]
    pub const fn planner(&self) -> &ModelExecutionAdaptivePlannerV1 {
        self.inner.planner()
    }

    #[must_use]
    pub const fn observer(&self) -> &ModelExecutionObserverBundleV1<B, T> {
        self.inner.observer()
    }

    #[must_use]
    pub const fn actuator(&self) -> &TransactionalModelExecution<B> {
        self.inner.actuator()
    }

    pub fn actuator_mut(&mut self) -> &mut TransactionalModelExecution<B> {
        self.inner.actuator_mut()
    }

    /// Current physically active qualified profile rank.
    pub fn current_profile_rank(&self) -> Result<u32, RuntimeError> {
        self.inner.actuator().current_profile_rank()
    }
}

impl<B, T, F> ModelExecutionControllerV1<B, T, F>
where
    B: ModelExecutionProfileBackendV1,
    T: ModelExecutionResourceTelemetryV1,
    F: Forecaster,
{
    /// Execute one complete forecast-aware adaptive transaction cycle.
    pub fn cycle(&mut self) -> Result<ForecastCycleResult, RuntimeError> {
        self.inner.cycle()
    }

    /// Execute the configured one-shot or bounded periodic control loop.
    pub fn run(
        &mut self,
        cancellation: &CancellationToken,
    ) -> Result<ForecastRunResult, RuntimeError> {
        self.inner.run(cancellation)
    }
}
