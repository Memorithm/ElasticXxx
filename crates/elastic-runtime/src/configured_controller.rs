//! Deterministic materialization of validated operator configuration.
//!
//! This module turns the descriptive [`OperatorConfig`] schema into concrete
//! runtime components without inventing resources, planner targets, or
//! capabilities. Full configuration validation always runs before adapters are
//! instantiated. The resulting controller still delegates every state-changing
//! operation to the existing trusted [`TransactionalActuator`] boundary.

use elastic_adapters::{ConcurrencyPermits, HeadroomPlanner, RamBudget, ThresholdPlanner};
use elastic_core::resource::{ObservationSignalId, ResourceSpec};
use elastic_eir::{
    lower, EirResource, FirstGroundedPlanner, PlanOutcome, PlanningContext, TransitionPlanner,
};

use crate::{
    active_permits_signal, concurrency_capacity_signal, concurrency_width_signal,
    ram_configured_max_bytes_signal, ram_configured_min_bytes_signal, ram_in_use_bytes_signal,
    Actuation, CommitRecord, ConfiguredForecaster, ControllerConfig, ForecastController,
    ForecasterSelection, InvariantCheck, Observation, ObservationSource, Observer, OperatorConfig,
    Plan, PlannerConfig, PlannerSelection, ResourceConfig, RollbackRecord, Runtime, RuntimeConfig,
    RuntimeError, TransactionalActuator, TransactionalConcurrency, TransactionalRam, ValidatedPlan,
    VerificationResult,
};

/// Planner implementation selected from the versioned operator schema.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ConfiguredPlanner {
    FirstGrounded(FirstGroundedPlanner),
    Headroom(HeadroomPlanner),
    Threshold(ThresholdPlanner),
}

/// Pure declaration-only planning view derived from validated operator configuration.
///
/// This view contains the normalized resource declaration, configured planner,
/// and the resource's declared initial observation state without constructing a
/// physical adapter. It is suitable for read-only planning tools that must not
/// allocate RAM, acquire permits, or create an actuation boundary.
#[derive(Clone, Debug)]
pub struct ConfiguredPlanningView {
    resource_spec: ResourceSpec,
    ir: EirResource,
    planner: ConfiguredPlanner,
    context: PlanningContext,
    observations: Vec<Observation>,
}

impl ConfiguredPlanningView {
    #[must_use]
    pub const fn resource_spec(&self) -> &ResourceSpec {
        &self.resource_spec
    }

    #[must_use]
    pub const fn ir(&self) -> &EirResource {
        &self.ir
    }

    #[must_use]
    pub const fn planner(&self) -> ConfiguredPlanner {
        self.planner
    }

    #[must_use]
    pub const fn context(&self) -> &PlanningContext {
        &self.context
    }

    #[must_use]
    pub fn observations(&self) -> &[Observation] {
        &self.observations
    }
}

impl TransitionPlanner for ConfiguredPlanner {
    fn propose_transition(&self, resource: &EirResource) -> PlanOutcome {
        match self {
            Self::FirstGrounded(planner) => planner.propose_transition(resource),
            Self::Headroom(planner) => planner.propose_transition(resource),
            Self::Threshold(planner) => planner.propose_transition(resource),
        }
    }

    fn propose_transition_with_context(
        &self,
        resource: &EirResource,
        context: &PlanningContext,
    ) -> PlanOutcome {
        match self {
            Self::FirstGrounded(planner) => {
                planner.propose_transition_with_context(resource, context)
            }
            Self::Headroom(planner) => planner.propose_transition_with_context(resource, context),
            Self::Threshold(planner) => planner.propose_transition_with_context(resource, context),
        }
    }
}

/// Concrete reference resource selected from operator configuration.
///
/// Clones of this enum preserve the adapters' shared-state semantics, so the
/// observer and actuator sides of a configured controller always refer to the
/// same physical resource state.
#[derive(Clone, Debug)]
pub enum ConfiguredResource {
    Ram(TransactionalRam),
    Concurrency(TransactionalConcurrency),
}

/// Observable committed state of a configured reference resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfiguredResourceState {
    Ram { committed_bytes: u64 },
    Concurrency { width: usize },
}

impl ConfiguredResource {
    /// Clone the normalized EIR node owned by this configured adapter.
    ///
    /// # Errors
    ///
    /// Returns an actuation error if the adapter's protected state cannot be
    /// accessed.
    pub fn ir(&self) -> Result<EirResource, RuntimeError> {
        match self {
            Self::Ram(resource) => resource.ir(),
            Self::Concurrency(resource) => resource.ir(),
        }
    }

    /// Read the current committed resource state.
    ///
    /// # Errors
    ///
    /// Returns an actuation error if the adapter's protected state cannot be
    /// accessed.
    pub fn state(&self) -> Result<ConfiguredResourceState, RuntimeError> {
        match self {
            Self::Ram(resource) => Ok(ConfiguredResourceState::Ram {
                committed_bytes: resource.committed()?,
            }),
            Self::Concurrency(resource) => Ok(ConfiguredResourceState::Concurrency {
                width: resource.width()?,
            }),
        }
    }
}

impl Observer for ConfiguredResource {
    fn observe(&self) -> (PlanningContext, Vec<Observation>) {
        match self {
            Self::Ram(resource) => resource.observe(),
            Self::Concurrency(resource) => resource.observe(),
        }
    }
}

impl TransactionalActuator for ConfiguredResource {
    fn name(&self) -> &str {
        match self {
            Self::Ram(resource) => resource.name(),
            Self::Concurrency(resource) => resource.name(),
        }
    }

    fn validate(&self, plan: &Plan) -> Result<Vec<InvariantCheck>, RuntimeError> {
        match self {
            Self::Ram(resource) => resource.validate(plan),
            Self::Concurrency(resource) => resource.validate(plan),
        }
    }

    fn prepare(&mut self, plan: &ValidatedPlan) -> Result<Actuation, RuntimeError> {
        match self {
            Self::Ram(resource) => resource.prepare(plan),
            Self::Concurrency(resource) => resource.prepare(plan),
        }
    }

    fn actuate(&mut self, actuation: &Actuation) -> Result<(), RuntimeError> {
        match self {
            Self::Ram(resource) => resource.actuate(actuation),
            Self::Concurrency(resource) => resource.actuate(actuation),
        }
    }

    fn verify(&self, actuation: &Actuation) -> Result<VerificationResult, RuntimeError> {
        match self {
            Self::Ram(resource) => resource.verify(actuation),
            Self::Concurrency(resource) => resource.verify(actuation),
        }
    }

    fn commit(&mut self, actuation: &Actuation) -> Result<CommitRecord, RuntimeError> {
        match self {
            Self::Ram(resource) => resource.commit(actuation),
            Self::Concurrency(resource) => resource.commit(actuation),
        }
    }

    fn rollback(
        &mut self,
        actuation: &Actuation,
        verification: &VerificationResult,
    ) -> Result<RollbackRecord, RuntimeError> {
        match self {
            Self::Ram(resource) => resource.rollback(actuation, verification),
            Self::Concurrency(resource) => resource.rollback(actuation, verification),
        }
    }
}

/// Fully materialized controller produced by [`OperatorConfig`].
pub type ConfiguredController = ForecastController<
    ConfiguredPlanner,
    ConfiguredResource,
    ConfiguredResource,
    ConfiguredForecaster,
>;

impl OperatorConfig {
    /// Materialize one configured controller by logical resource id.
    ///
    /// Full configuration validation is completed before any adapter is
    /// instantiated, so malformed configuration cannot leave a partially
    /// materialized operator state.
    ///
    /// # Errors
    ///
    /// Build a declaration-only read-only planning view for one resource.
    ///
    /// Unlike [`OperatorConfig::build_controller`], this does not instantiate
    /// `TransactionalRam` or `TransactionalConcurrency`; therefore it cannot
    /// allocate the configured RAM commitment or expose an actuation boundary.
    /// The observation snapshot represents only the explicitly configured
    /// initial state.
    ///
    /// # Errors
    ///
    /// Returns a configuration error for invalid configuration, an unknown or
    /// uncontrolled resource id, or a declaration that cannot be lowered.
    pub fn build_planning_view(
        &self,
        resource_id: &str,
    ) -> Result<ConfiguredPlanningView, RuntimeError> {
        self.validate()?;
        let controller = self
            .controllers
            .iter()
            .find(|controller| controller.resource == resource_id)
            .ok_or_else(|| {
                RuntimeError::configuration(format!(
                    "no configured controller for resource '{resource_id}'"
                ))
            })?;
        let resource = self
            .resources
            .iter()
            .find(|resource| resource.id() == resource_id)
            .ok_or_else(|| {
                RuntimeError::configuration(format!(
                    "controller resource '{resource_id}' disappeared after validation"
                ))
            })?;

        let resource_spec = declaration_resource_spec(resource)?;
        let document = lower(&resource_spec).map_err(|error| {
            RuntimeError::configuration(format!(
                "configured declaration cannot lower to EIR: {error}"
            ))
        })?;
        let ir = document
            .resource(resource_id)
            .ok_or_else(|| RuntimeError::configuration("configured declaration lost its EIR node"))?
            .clone();
        let planner = materialize_planner(&controller.planner)?;
        let (context, observations) = declaration_observations(resource, &resource_spec);

        Ok(ConfiguredPlanningView {
            resource_spec,
            ir,
            planner,
            context,
            observations,
        })
    }

    pub fn build_controller(
        &self,
        resource_id: &str,
    ) -> Result<ConfiguredController, RuntimeError> {
        self.validate()?;
        let controller = self
            .controllers
            .iter()
            .find(|controller| controller.resource == resource_id)
            .ok_or_else(|| {
                RuntimeError::configuration(format!(
                    "no configured controller for resource '{resource_id}'"
                ))
            })?;
        let resource = self
            .resources
            .iter()
            .find(|resource| resource.id() == resource_id)
            .ok_or_else(|| {
                RuntimeError::configuration(format!(
                    "controller resource '{resource_id}' disappeared after validation"
                ))
            })?;
        build_controller(controller, resource)
    }

    /// Materialize every configured controller in canonical resource-id order.
    ///
    /// # Errors
    ///
    /// Returns the first validation or construction failure. No controller is
    /// constructed until the complete operator configuration has validated.
    pub fn build_controllers(&self) -> Result<Vec<ConfiguredController>, RuntimeError> {
        self.validate()?;
        let mut controllers = self.controllers.iter().collect::<Vec<_>>();
        controllers.sort_by(|left, right| left.resource.cmp(&right.resource));
        controllers
            .into_iter()
            .map(|controller| {
                let resource = self
                    .resources
                    .iter()
                    .find(|resource| resource.id() == controller.resource)
                    .ok_or_else(|| {
                        RuntimeError::configuration(format!(
                            "controller resource '{}' disappeared after validation",
                            controller.resource
                        ))
                    })?;
                build_controller(controller, resource)
            })
            .collect()
    }
}

fn build_controller(
    controller: &ControllerConfig,
    resource_config: &ResourceConfig,
) -> Result<ConfiguredController, RuntimeError> {
    let resource = materialize_resource(resource_config)?;
    let ir = resource.ir()?;
    let spec = resource_spec_from_eir(&ir)?;
    let planner = materialize_planner(&controller.planner)?;
    let forecaster = materialize_forecaster(&controller.forecaster)?;
    let (cadence, max_cycles) = controller.cadence.runtime_values();
    let interval_ms = match controller.cadence {
        crate::CadenceConfig::OneShot => 0,
        crate::CadenceConfig::Periodic { interval_ms, .. } => interval_ms,
    };
    let runtime = Runtime::new(RuntimeConfig {
        resource_spec: spec,
        ir_resource: ir.clone(),
        planner_config: planner_runtime_config(&controller.planner),
        cadence,
        mode: controller.mode.runtime_mode(),
        max_cycles,
        interval_ms,
        emit_events: true,
        dry_run: controller.mode.dry_run(),
    });
    let observer = resource.clone();
    let actuator = resource;
    Ok(ForecastController::new(
        runtime, ir, planner, observer, actuator, forecaster,
    ))
}

fn declaration_resource_spec(config: &ResourceConfig) -> Result<ResourceSpec, RuntimeError> {
    match config {
        ResourceConfig::Ram { id, .. } => RamBudget::declaration(id),
        ResourceConfig::Concurrency { id, .. } => ConcurrencyPermits::declaration(id),
    }
    .map_err(|error| RuntimeError::configuration(error.to_string()))
}

fn declaration_observations(
    config: &ResourceConfig,
    spec: &ResourceSpec,
) -> (PlanningContext, Vec<Observation>) {
    let values: Vec<(ObservationSignalId, f64)> = match config {
        ResourceConfig::Ram {
            host_total,
            min,
            max,
            initial,
            ..
        } => vec![
            (
                ObservationSignalId::FREE_CAPACITY,
                host_total.saturating_sub(*initial) as f64,
            ),
            (
                ObservationSignalId::UTILIZATION,
                *initial as f64 / *host_total as f64,
            ),
            (fixed_signal("committed-bytes"), *initial as f64),
            (fixed_signal("host-total-bytes"), *host_total as f64),
            (ram_configured_min_bytes_signal(), *min as f64),
            (ram_configured_max_bytes_signal(), *max as f64),
            (ram_in_use_bytes_signal(), 0.0),
        ],
        ResourceConfig::Concurrency {
            max_width,
            initial_width,
            ..
        } => vec![
            (ObservationSignalId::UTILIZATION, 0.0),
            (concurrency_capacity_signal(), *max_width as f64),
            (concurrency_width_signal(), *initial_width as f64),
            (active_permits_signal(), 0.0),
        ],
    };
    let now = std::time::Instant::now();
    let source = ObservationSource::Resource(spec.resource_id().clone());
    let mut context = PlanningContext::new();
    let observations = values
        .into_iter()
        .map(|(signal, value)| {
            context = context.clone().observe(signal.clone(), value);
            Observation::from_source(source.clone(), signal, value, now)
        })
        .collect();
    (context, observations)
}

fn fixed_signal(id: &str) -> ObservationSignalId {
    ObservationSignalId::custom(id).expect("fixed configured observation signal is valid")
}

fn materialize_resource(config: &ResourceConfig) -> Result<ConfiguredResource, RuntimeError> {
    match config {
        ResourceConfig::Ram {
            id,
            host_total,
            min,
            max,
            initial,
            max_step,
        } => TransactionalRam::new(id, *host_total, *min, *max, *initial, *max_step)
            .map(ConfiguredResource::Ram)
            .map_err(|error| RuntimeError::configuration(error.to_string())),
        ResourceConfig::Concurrency {
            id,
            max_width,
            initial_width,
        } => TransactionalConcurrency::new(id, *max_width, *initial_width)
            .map(ConfiguredResource::Concurrency)
            .map_err(|error| RuntimeError::configuration(error.to_string())),
    }
}

fn materialize_planner(selection: &PlannerSelection) -> Result<ConfiguredPlanner, RuntimeError> {
    match selection {
        PlannerSelection::FirstGrounded => {
            Ok(ConfiguredPlanner::FirstGrounded(FirstGroundedPlanner))
        }
        PlannerSelection::Headroom {
            headroom_fraction,
            deadband_fraction,
        } => HeadroomPlanner::new(*headroom_fraction, *deadband_fraction)
            .map(ConfiguredPlanner::Headroom)
            .map_err(|error| RuntimeError::configuration(error.to_string())),
        PlannerSelection::Threshold {
            low_watermark,
            high_watermark,
            step_fraction,
        } => ThresholdPlanner::new(*low_watermark, *high_watermark, *step_fraction)
            .map(ConfiguredPlanner::Threshold)
            .map_err(|error| RuntimeError::configuration(error.to_string())),
    }
}

fn materialize_forecaster(
    selection: &ForecasterSelection,
) -> Result<ConfiguredForecaster, RuntimeError> {
    selection.build()
}

fn planner_runtime_config(selection: &PlannerSelection) -> PlannerConfig {
    match selection {
        PlannerSelection::FirstGrounded => PlannerConfig::FirstGrounded,
        PlannerSelection::Headroom {
            headroom_fraction,
            deadband_fraction,
        } => PlannerConfig::Headroom {
            headroom_fraction: *headroom_fraction,
            deadband_fraction: *deadband_fraction,
        },
        PlannerSelection::Threshold {
            low_watermark,
            high_watermark,
            step_fraction,
        } => PlannerConfig::Threshold {
            high_watermark: *high_watermark,
            low_watermark: *low_watermark,
            step_fraction: *step_fraction,
        },
    }
}

/// Reconstruct the canonical surface declaration represented by normalized EIR.
///
/// EIR contains every semantic field of `ResourceSpec`. The reconstructed spec
/// is lowered again and its fingerprint must match the source EIR before it is
/// accepted, preventing silent drift or metadata invention.
fn resource_spec_from_eir(ir: &EirResource) -> Result<ResourceSpec, RuntimeError> {
    let mut builder = ResourceSpec::builder(ir.class().clone(), ir.identity().clone());
    for dimension in ir.dimensions() {
        builder = builder.allow(dimension.clone());
    }
    for invariant in ir.invariants() {
        builder = builder.preserve(invariant.clone());
    }
    for objective in ir.objective_ranking() {
        builder = builder.optimize(objective.objective().clone());
    }
    for admitted in ir.transitions() {
        builder = builder.admit(admitted.transition().clone());
    }
    for capability in ir.capabilities() {
        builder = builder.require_capability(capability.clone());
    }
    for signal in ir.observations() {
        builder = builder.observe(signal.clone());
    }
    for (key, value) in ir.iter_labels() {
        builder = builder.label(key, value);
    }
    let spec = builder.build().map_err(|error| {
        RuntimeError::configuration(format!("EIR cannot round-trip to ResourceSpec: {error}"))
    })?;
    let document = lower(&spec).map_err(|error| {
        RuntimeError::configuration(format!("round-tripped ResourceSpec cannot lower: {error}"))
    })?;
    let round_tripped = document.resource(ir.identity().as_str()).ok_or_else(|| {
        RuntimeError::configuration("round-tripped ResourceSpec lost its EIR node")
    })?;
    if round_tripped.fingerprint() != ir.fingerprint() {
        return Err(RuntimeError::configuration(
            "EIR to ResourceSpec round-trip changed the normalized fingerprint",
        ));
    }
    Ok(spec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CadenceConfig, ExecutionModeConfig, ForecasterSelection, OPERATOR_CONFIG_VERSION};

    fn ram_config(mode: ExecutionModeConfig) -> OperatorConfig {
        OperatorConfig {
            version: OPERATOR_CONFIG_VERSION,
            resources: vec![ResourceConfig::Ram {
                id: "ram".into(),
                host_total: 4096,
                min: 512,
                max: 4096,
                initial: 1024,
                max_step: Some(2048),
            }],
            controllers: vec![ControllerConfig {
                resource: "ram".into(),
                planner: PlannerSelection::Headroom {
                    headroom_fraction: 0.5,
                    deadband_fraction: 0.0,
                },
                forecaster: ForecasterSelection::Ewma {
                    alpha: 0.5,
                    horizon_ms: 1000,
                },
                cadence: CadenceConfig::OneShot,
                mode,
            }],
        }
    }

    #[test]
    fn configured_ram_controller_executes_verified_transaction() {
        let config = ram_config(ExecutionModeConfig::Apply);
        let mut controller = config.build_controller("ram").unwrap();

        let result = controller.cycle().unwrap();

        assert!(result.forecast.is_some());
        assert!(result.transaction.commit.is_some());
        assert_eq!(
            controller.actuator().state().unwrap(),
            ConfiguredResourceState::Ram {
                committed_bytes: 2048
            }
        );
    }

    #[test]
    fn materialized_runtime_config_matches_configured_resource() {
        let config = ram_config(ExecutionModeConfig::DryRun);
        let controller = config.build_controller("ram").unwrap();
        let runtime_config = controller.forecast_runtime().runtime().config();

        assert_eq!(runtime_config.resource_spec.resource_id().as_str(), "ram");
        assert_eq!(runtime_config.ir_resource.identity().as_str(), "ram");
        assert_eq!(
            runtime_config.ir_resource.fingerprint(),
            controller.resource().fingerprint()
        );
        assert!(matches!(
            runtime_config.planner_config,
            PlannerConfig::Headroom { .. }
        ));
    }

    #[test]
    fn concurrency_plan_only_materializes_without_inventing_target() {
        let config = OperatorConfig {
            version: OPERATOR_CONFIG_VERSION,
            resources: vec![ResourceConfig::Concurrency {
                id: "workers".into(),
                max_width: 8,
                initial_width: 4,
            }],
            controllers: vec![ControllerConfig {
                resource: "workers".into(),
                planner: PlannerSelection::FirstGrounded,
                forecaster: ForecasterSelection::CurrentState,
                cadence: CadenceConfig::OneShot,
                mode: ExecutionModeConfig::PlanOnly,
            }],
        };
        let mut controller = config.build_controller("workers").unwrap();

        let result = controller.cycle().unwrap();

        assert!(result.transaction.plan.is_some());
        assert!(result.transaction.actuation.is_none());
        assert_eq!(
            controller.actuator().state().unwrap(),
            ConfiguredResourceState::Concurrency { width: 4 }
        );
    }

    #[test]
    fn controllers_materialize_in_canonical_resource_order() {
        let mut first = ram_config(ExecutionModeConfig::PlanOnly);
        first.resources[0] = ResourceConfig::Ram {
            id: "z-ram".into(),
            host_total: 4096,
            min: 512,
            max: 4096,
            initial: 1024,
            max_step: Some(2048),
        };
        first.controllers[0].resource = "z-ram".into();
        let mut second = ram_config(ExecutionModeConfig::PlanOnly);
        second.resources[0] = ResourceConfig::Ram {
            id: "a-ram".into(),
            host_total: 4096,
            min: 512,
            max: 4096,
            initial: 1024,
            max_step: Some(2048),
        };
        second.controllers[0].resource = "a-ram".into();
        let config = OperatorConfig {
            version: OPERATOR_CONFIG_VERSION,
            resources: vec![first.resources.remove(0), second.resources.remove(0)],
            controllers: vec![first.controllers.remove(0), second.controllers.remove(0)],
        };

        let controllers = config.build_controllers().unwrap();
        let ids = controllers
            .iter()
            .map(|controller| controller.resource().identity().as_str())
            .collect::<Vec<_>>();

        assert_eq!(ids, vec!["a-ram", "z-ram"]);
    }

    #[test]
    fn planning_view_matches_materialized_initial_ram_state_without_actuation() {
        let config = ram_config(ExecutionModeConfig::PlanOnly);
        let view = config.build_planning_view("ram").unwrap();
        let controller = config.build_controller("ram").unwrap();
        let (live_context, _) = controller.observer().observe();

        assert_eq!(view.ir().fingerprint(), controller.resource().fingerprint());
        for (signal, value) in live_context.iter() {
            assert_eq!(view.context().get(signal.clone()), Some(value));
        }
    }

    #[test]
    fn planning_view_accepts_huge_declared_ram_without_allocating_it() {
        let mut config = ram_config(ExecutionModeConfig::PlanOnly);
        config.resources[0] = ResourceConfig::Ram {
            id: "ram".into(),
            host_total: 1_u64 << 44,
            min: 1_u64 << 30,
            max: 1_u64 << 44,
            initial: 1_u64 << 43,
            max_step: Some(1_u64 << 30),
        };

        let view = config.build_planning_view("ram").unwrap();

        assert_eq!(
            view.context().get(fixed_signal("committed-bytes")),
            Some((1_u64 << 43) as f64)
        );
        assert_eq!(
            view.context().get(ObservationSignalId::FREE_CAPACITY),
            Some((1_u64 << 43) as f64)
        );
    }

    #[test]
    fn unknown_controller_is_rejected_before_materialization() {
        let config = ram_config(ExecutionModeConfig::PlanOnly);
        let error = config.build_controller("missing").unwrap_err();
        assert!(matches!(error, RuntimeError::Configuration(_)));
    }
}
