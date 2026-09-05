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
    model_execution_current_profile_rank_signal, ModelExecutionAdaptivePlannerV1,
    ModelExecutionEnvelopePolicyV1, ModelExecutionProfileSetV1,
};
use elastic_eir::PlanningContext;

use crate::{
    CadenceConfig, CancellationToken, CurrentStateForecaster, ExecutionModeConfig,
    ForecastController, ForecastCycleFailure, ForecastCycleResult, ForecastRunAttempt,
    ForecastRunFailure, ForecastRunResult, Forecaster, ModelExecutionControllerContractsV1,
    ModelExecutionCycleEvidenceV1, ModelExecutionProfileBackendV1,
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

/// Successful bounded model run together with one durable artifact per
/// completed cycle.
#[derive(Debug)]
pub struct ModelExecutionRunEvidenceResultV1 {
    pub run: ForecastRunResult,
    pub evidence: Vec<ModelExecutionCycleEvidenceV1>,
}

/// Completed or failed attempt to execute a bounded model run with durable
/// evidence for every cycle that actually completed.
#[derive(Debug)]
pub enum ModelExecutionRunEvidenceAttemptV1 {
    Completed(Box<ModelExecutionRunEvidenceResultV1>),
    Failed(Box<ModelExecutionRunEvidenceFailureV1>),
}

impl ModelExecutionRunEvidenceAttemptV1 {
    /// Convert back to the historical `run_with_evidence` result surface.
    ///
    /// When both a runtime failure and a later evidence-conversion failure are
    /// retained, the original runtime failure keeps legacy precedence.
    pub fn into_result(
        self,
    ) -> Result<(ForecastRunResult, Vec<ModelExecutionCycleEvidenceV1>), RuntimeError> {
        match self {
            Self::Completed(result) => Ok((result.run, result.evidence)),
            Self::Failed(failure) => Err(failure.into_legacy_error()),
        }
    }

    #[must_use]
    pub const fn is_completed(&self) -> bool {
        matches!(self, Self::Completed(_))
    }
}

/// Failure while retaining as much trustworthy model-run evidence as exists.
#[derive(Debug)]
pub enum ModelExecutionRunEvidenceFailureV1 {
    /// The generic forecast-aware run failed. Evidence for every earlier
    /// completed cycle was captured successfully.
    Runtime {
        completed_evidence: Vec<ModelExecutionCycleEvidenceV1>,
        failure: Box<ForecastRunFailure>,
    },
    /// Converting completed cycles to durable model evidence, rebuilding the
    /// contract binding, or final physical reconciliation failed.
    Evidence {
        completed_evidence: Vec<ModelExecutionCycleEvidenceV1>,
        /// Present when the generic run itself completed before evidence failed.
        run: Option<Box<ForecastRunResult>>,
        /// Present when a generic runtime failure happened first and evidence
        /// conversion of its completed prefix then also failed.
        runtime_failure: Option<Box<ForecastRunFailure>>,
        error: RuntimeError,
    },
}

impl ModelExecutionRunEvidenceFailureV1 {
    /// Error that the historical `run_with_evidence` API would return.
    #[must_use]
    pub fn error(&self) -> &RuntimeError {
        match self {
            Self::Runtime { failure, .. } => failure.error(),
            Self::Evidence {
                runtime_failure: Some(failure),
                ..
            } => failure.error(),
            Self::Evidence { error, .. } => error,
        }
    }

    /// Evidence-layer error, when evidence conversion or reconciliation itself
    /// failed. This is distinct from an earlier retained runtime failure.
    #[must_use]
    pub const fn evidence_error(&self) -> Option<&RuntimeError> {
        match self {
            Self::Runtime { .. } => None,
            Self::Evidence { error, .. } => Some(error),
        }
    }

    fn into_legacy_error(self) -> RuntimeError {
        match self {
            Self::Runtime { failure, .. } => forecast_run_failure_into_error(*failure),
            Self::Evidence {
                runtime_failure: Some(failure),
                ..
            } => forecast_run_failure_into_error(*failure),
            Self::Evidence { error, .. } => error,
        }
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

    /// Assemble a current-state controller from one fully revalidated persisted
    /// contract bundle.
    pub fn current_state_from_contracts(
        resource_id: &str,
        contracts: ModelExecutionControllerContractsV1,
        backend: B,
        telemetry: T,
        cadence: CadenceConfig,
        mode: ExecutionModeConfig,
    ) -> Result<Self, RuntimeError> {
        let (profiles, policy) = contracts.into_execution_parts();
        Self::current_state(
            resource_id,
            profiles,
            policy,
            backend,
            telemetry,
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

    /// Assemble a controller with an explicit forecaster from one fully
    /// revalidated persisted contract bundle.
    #[allow(clippy::too_many_arguments)]
    pub fn from_contracts(
        resource_id: &str,
        contracts: ModelExecutionControllerContractsV1,
        backend: B,
        telemetry: T,
        forecaster: F,
        cadence: CadenceConfig,
        mode: ExecutionModeConfig,
    ) -> Result<Self, RuntimeError> {
        let (profiles, policy) = contracts.into_execution_parts();
        Self::new(
            resource_id,
            profiles,
            policy,
            backend,
            telemetry,
            forecaster,
            cadence,
            mode,
        )
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

    /// Execute one complete cycle and capture a durable, contract-bound evidence
    /// artifact for the resulting physical state.
    ///
    /// Evidence capture happens only after the trusted cycle has completed and
    /// the backend's final published profile rank has been read. Revalidating the
    /// returned artifact later is read-only and never authorizes a new actuation.
    pub fn cycle_with_evidence(
        &mut self,
    ) -> Result<(ForecastCycleResult, ModelExecutionCycleEvidenceV1), RuntimeError> {
        let result = self.inner.cycle()?;
        let final_profile_rank = self.current_profile_rank()?;
        let contracts = self.controller_contracts()?;
        let resource_id = self.resource_id();
        let evidence = ModelExecutionCycleEvidenceV1::capture(
            &contracts,
            resource_id,
            &result,
            final_profile_rank,
        )?;
        Ok((result, evidence))
    }

    /// Execute the configured one-shot or bounded periodic control loop.
    pub fn run(
        &mut self,
        cancellation: &CancellationToken,
    ) -> Result<ForecastRunResult, RuntimeError> {
        self.inner.run(cancellation)
    }

    /// Execute the configured bounded run and capture one durable evidence
    /// artifact for every completed cycle.
    pub fn run_with_evidence(
        &mut self,
        cancellation: &CancellationToken,
    ) -> Result<(ForecastRunResult, Vec<ModelExecutionCycleEvidenceV1>), RuntimeError> {
        self.run_with_evidence_attempt(cancellation).into_result()
    }

    /// Execute the configured bounded run while retaining durable evidence for
    /// every cycle that completed before a later runtime or evidence failure.
    ///
    /// A catastrophically failed cycle is retained through [`ForecastRunFailure`]
    /// and never receives a fabricated `ModelExecutionCycleEvidenceV1` because
    /// its final physical profile may be unknown.
    pub fn run_with_evidence_attempt(
        &mut self,
        cancellation: &CancellationToken,
    ) -> ModelExecutionRunEvidenceAttemptV1 {
        let run_attempt = self.inner.run_attempt(cancellation);
        let contracts = match self.controller_contracts() {
            Ok(contracts) => contracts,
            Err(error) => {
                return model_evidence_failure_after_run_attempt(run_attempt, Vec::new(), error);
            }
        };
        let resource_id = self.resource_id();

        match run_attempt {
            ForecastRunAttempt::Completed(run) => {
                let run = *run;
                let evidence =
                    match capture_completed_model_evidence(&contracts, &resource_id, &run.cycles) {
                        Ok(evidence) => evidence,
                        Err((completed_evidence, error)) => {
                            return ModelExecutionRunEvidenceAttemptV1::Failed(Box::new(
                                ModelExecutionRunEvidenceFailureV1::Evidence {
                                    completed_evidence,
                                    run: Some(Box::new(run)),
                                    runtime_failure: None,
                                    error,
                                },
                            ));
                        }
                    };

                if let Some(last_profile_rank) = evidence
                    .last()
                    .map(ModelExecutionCycleEvidenceV1::final_profile_rank)
                {
                    let physical_rank = match self.current_profile_rank() {
                        Ok(rank) => rank,
                        Err(error) => {
                            return ModelExecutionRunEvidenceAttemptV1::Failed(Box::new(
                                ModelExecutionRunEvidenceFailureV1::Evidence {
                                    completed_evidence: evidence,
                                    run: Some(Box::new(run)),
                                    runtime_failure: None,
                                    error,
                                },
                            ));
                        }
                    };
                    if physical_rank != last_profile_rank {
                        return ModelExecutionRunEvidenceAttemptV1::Failed(Box::new(
                            ModelExecutionRunEvidenceFailureV1::Evidence {
                                completed_evidence: evidence,
                                run: Some(Box::new(run)),
                                runtime_failure: None,
                                error: RuntimeError::verification(format!(
                                    "bounded model run ended at physical profile rank {physical_rank}, but durable cycle evidence ended at {last_profile_rank}"
                                )),
                            },
                        ));
                    }
                }

                ModelExecutionRunEvidenceAttemptV1::Completed(Box::new(
                    ModelExecutionRunEvidenceResultV1 { run, evidence },
                ))
            }
            ForecastRunAttempt::Failed(failure) => {
                let failure = *failure;
                let completed_cycles = match &failure {
                    ForecastRunFailure::Setup { .. } => &[][..],
                    ForecastRunFailure::Cycle {
                        completed_cycles, ..
                    } => completed_cycles.as_slice(),
                };
                match capture_completed_model_evidence(&contracts, &resource_id, completed_cycles) {
                    Ok(completed_evidence) => ModelExecutionRunEvidenceAttemptV1::Failed(Box::new(
                        ModelExecutionRunEvidenceFailureV1::Runtime {
                            completed_evidence,
                            failure: Box::new(failure),
                        },
                    )),
                    Err((completed_evidence, error)) => ModelExecutionRunEvidenceAttemptV1::Failed(
                        Box::new(ModelExecutionRunEvidenceFailureV1::Evidence {
                            completed_evidence,
                            run: None,
                            runtime_failure: Some(Box::new(failure)),
                            error,
                        }),
                    ),
                }
            }
        }
    }

    fn controller_contracts(&self) -> Result<ModelExecutionControllerContractsV1, RuntimeError> {
        ModelExecutionControllerContractsV1::new(
            self.inner.planner().profiles().clone(),
            self.inner.planner().policy().clone(),
        )
        .map_err(|error| RuntimeError::validation(error.to_string()))
    }

    fn resource_id(&self) -> String {
        self.inner.resource().identity().as_str().to_owned()
    }
}

fn model_evidence_failure_after_run_attempt(
    run_attempt: ForecastRunAttempt,
    completed_evidence: Vec<ModelExecutionCycleEvidenceV1>,
    error: RuntimeError,
) -> ModelExecutionRunEvidenceAttemptV1 {
    match run_attempt {
        ForecastRunAttempt::Completed(run) => ModelExecutionRunEvidenceAttemptV1::Failed(Box::new(
            ModelExecutionRunEvidenceFailureV1::Evidence {
                completed_evidence,
                run: Some(run),
                runtime_failure: None,
                error,
            },
        )),
        ForecastRunAttempt::Failed(failure) => ModelExecutionRunEvidenceAttemptV1::Failed(
            Box::new(ModelExecutionRunEvidenceFailureV1::Evidence {
                completed_evidence,
                run: None,
                runtime_failure: Some(failure),
                error,
            }),
        ),
    }
}

fn capture_completed_model_evidence(
    contracts: &ModelExecutionControllerContractsV1,
    resource_id: &str,
    cycles: &[ForecastCycleResult],
) -> Result<Vec<ModelExecutionCycleEvidenceV1>, (Vec<ModelExecutionCycleEvidenceV1>, RuntimeError)>
{
    let mut evidence = Vec::with_capacity(cycles.len());
    for cycle in cycles {
        let final_profile_rank = match completed_cycle_terminal_profile_rank(cycle) {
            Ok(rank) => rank,
            Err(error) => return Err((evidence, error)),
        };
        match ModelExecutionCycleEvidenceV1::capture(
            contracts,
            resource_id.to_owned(),
            cycle,
            final_profile_rank,
        ) {
            Ok(artifact) => evidence.push(artifact),
            Err(error) => return Err((evidence, error)),
        }
    }
    Ok(evidence)
}

fn forecast_run_failure_into_error(failure: ForecastRunFailure) -> RuntimeError {
    match failure {
        ForecastRunFailure::Setup { error } => error,
        ForecastRunFailure::Cycle { failed_cycle, .. } => {
            forecast_cycle_failure_into_error(*failed_cycle)
        }
    }
}

fn forecast_cycle_failure_into_error(failure: ForecastCycleFailure) -> RuntimeError {
    match failure {
        ForecastCycleFailure::Forecast { error, .. } => error,
        ForecastCycleFailure::Transaction { failure, .. } => failure.error,
    }
}

fn completed_cycle_terminal_profile_rank(cycle: &ForecastCycleResult) -> Result<u32, RuntimeError> {
    if cycle.transaction.commit.is_some() {
        let actuation = cycle.transaction.actuation.as_ref().ok_or_else(|| {
            RuntimeError::verification(
                "committed model cycle is missing its authoritative actuation record",
            )
        })?;
        let raw = actuation.target.ok_or_else(|| {
            RuntimeError::verification("committed model cycle actuation has no target profile rank")
        })?;
        return u32::try_from(raw).map_err(|_| {
            RuntimeError::verification("committed model cycle target profile rank does not fit u32")
        });
    }

    if cycle.transaction.rollback.is_some() || cycle.transaction.actuation.is_none() {
        return observed_initial_profile_rank(cycle);
    }

    Err(RuntimeError::verification(
        "completed model cycle with actuation has neither commit nor rollback outcome",
    ))
}

fn observed_initial_profile_rank(cycle: &ForecastCycleResult) -> Result<u32, RuntimeError> {
    let signal = model_execution_current_profile_rank_signal();
    for snapshot in &cycle.transaction.observations {
        for observation in &snapshot.observations {
            if observation.signal() == &signal && observation.is_valid() {
                let value = observation.value();
                if !value.is_finite()
                    || value < 0.0
                    || value.fract() != 0.0
                    || value > u32::MAX as f64
                {
                    return Err(RuntimeError::verification(format!(
                        "model run current-profile observation is not an exact u32 rank: {value}"
                    )));
                }
                return Ok(value as u32);
            }
        }
    }
    Err(RuntimeError::verification(
        "model run cycle has no valid initial current-profile observation",
    ))
}
