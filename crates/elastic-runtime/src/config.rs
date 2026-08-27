//! Runtime configuration.
//!
//! Immutable configuration that defines the behavior of a [`Runtime`] instance.
//! This includes resource declarations, adapter settings, planner configuration,
//! and control loop parameters.

use std::time::Duration;

use elastic_core::resource::ResourceSpec;
use elastic_eir::EirResource;

/// Configuration for a [`Runtime`] instance.
///
/// This is immutable after construction and defines the full policy boundary
/// for the runtime control loop.
#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeConfig {
    /// The resource specification that defines what may change and what must be preserved.
    pub resource_spec: ResourceSpec,
    /// The IR resource (normalized, validated EIR node) derived from the spec.
    pub ir_resource: EirResource,
    /// Planner configuration — which planner strategy to use and its parameters.
    pub planner_config: PlannerConfig,
    /// Control loop cadence.
    pub cadence: Cadence,
    /// Mode flags for the control loop.
    pub mode: RuntimeMode,
    /// Maximum number of cycles to run before stopping (0 = infinite).
    pub max_cycles: u64,
    /// Cycle interval in milliseconds for periodic mode.
    pub interval_ms: u64,
    /// Whether to collect and emit structured events.
    pub emit_events: bool,
    /// Whether to run in dry-run mode (no physical actuation).
    pub dry_run: bool,
}

impl RuntimeConfig {}

/// Planner configuration parameters.
///
/// Currently supports the `ThresholdPlanner` parameters, but is designed
/// to be extensible with `#[serde(flatten)]` for future planner types.
#[derive(Clone, Debug, PartialEq)]
pub enum PlannerConfig {
    /// Threshold planner with high/low watermarks and step fraction.
    Threshold {
        high_watermark: f64,
        low_watermark: f64,
        step_fraction: f64,
    },
    /// No planner configured (observe-only mode will produce no candidates).
    None,
}

/// Control loop execution mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeMode {
    /// One-shot: run a single cycle and stop.
    OneShot,
    /// Periodic: run cycles at the configured interval.
    Periodic,
    /// Dry-run: plan and validate but never actuate physical resources.
    DryRun,
    /// Observe-only: collect observations, produce plans, but never commit.
    ObserveOnly,
    /// Apply mode: full cycle with commit/rollback.
    Apply,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        let spec = ResourceSpec::builder(
            elastic_core::resource::ResourceClassId::CAPACITY_RESOURCE,
            elastic_core::resource::LogicalResourceId::new("default").unwrap(),
        )
        .allow(elastic_core::resource::DimensionId::CAPACITY)
        .preserve(elastic_core::resource::Invariant::new(
            elastic_core::resource::InvariantKind::PreserveContents,
        ))
        .optimize(elastic_core::resource::ObjectiveId::MEMORY_FOOTPRINT)
        .admit(elastic_core::resource::AdmissibleTransition::new(
            elastic_core::TransitionMechanism::Reinterpret,
            elastic_core::resource::DimensionId::CAPACITY,
        ))
        .require_capability(elastic_core::resource::CapabilityRequirement::new(
            elastic_core::TransitionMechanism::Reinterpret,
            elastic_core::resource::DimensionId::CAPACITY,
        ))
        .build()
        .expect("default spec should be valid");

        let doc = elastic_eir::lower(&spec).expect("default spec should lower");
        let ir = doc.resource("default").expect("resource present").clone();

        Self {
            resource_spec: spec,
            ir_resource: ir,
            planner_config: PlannerConfig::Threshold {
                high_watermark: 0.8,
                low_watermark: 0.3,
                step_fraction: 0.2,
            },
            cadence: Cadence::default(),
            mode: RuntimeMode::OneShot,
            max_cycles: 0,
            interval_ms: 1000,
            emit_events: true,
            dry_run: true,
        }
    }
}

/// Control loop cadence / timing.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum Cadence {
    /// Run one cycle immediately and stop (one-shot).
    #[default]
    OneShot,
    /// Run cycles periodically at the configured interval (milliseconds).
    Periodic(Duration),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = RuntimeConfig::default();
        assert!(config
            .resource_spec
            .elastic_dimensions()
            .contains(&elastic_core::resource::DimensionId::CAPACITY));
        assert_eq!(config.mode, RuntimeMode::OneShot);
        assert!(config.emit_events);
        assert!(config.dry_run);
    }
}
