//! Versioned, serializable operator configuration.
//!
//! Configuration is descriptive only. [`OperatorConfig::validate`] checks all
//! ids, resource bounds, controller references, planner/forecaster parameters,
//! and bounded periodic rules before any resource is instantiated or actuated.

use std::collections::BTreeSet;
use std::time::Duration;

use elastic_adapters::{HeadroomPlanner, ThresholdPlanner};
use elastic_core::resource::LogicalResourceId;
use serde::{Deserialize, Serialize};

use crate::{Cadence, EwmaForecaster, RuntimeError, RuntimeMode};

/// Current supported configuration schema version.
pub const OPERATOR_CONFIG_VERSION: u32 = 1;

/// Complete operator configuration for resources and controllers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperatorConfig {
    pub version: u32,
    pub resources: Vec<ResourceConfig>,
    pub controllers: Vec<ControllerConfig>,
}

impl OperatorConfig {
    /// Validate the complete configuration without performing physical effects.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::Configuration`] for unsupported schema versions,
    /// invalid/duplicate resource ids, invalid adapter bounds, unresolved
    /// controller resources, incompatible/invalid planner/forecaster parameters,
    /// or unbounded/zero-interval periodic cadence.
    pub fn validate(&self) -> Result<(), RuntimeError> {
        if self.version != OPERATOR_CONFIG_VERSION {
            return Err(RuntimeError::configuration(format!(
                "unsupported operator config version {}; expected {}",
                self.version, OPERATOR_CONFIG_VERSION
            )));
        }

        let mut resource_ids = BTreeSet::new();
        for resource in &self.resources {
            resource.validate()?;
            let id = resource.id();
            if !resource_ids.insert(id.to_owned()) {
                return Err(RuntimeError::configuration(format!(
                    "duplicate configured resource '{id}'"
                )));
            }
        }

        let mut controller_resources = BTreeSet::new();
        for controller in &self.controllers {
            controller.validate()?;
            let resource = self
                .resources
                .iter()
                .find(|resource| resource.id() == controller.resource)
                .ok_or_else(|| {
                    RuntimeError::configuration(format!(
                        "controller references unknown resource '{}'",
                        controller.resource
                    ))
                })?;
            controller.validate_resource_compatibility(resource)?;
            if !controller_resources.insert(controller.resource.clone()) {
                return Err(RuntimeError::configuration(format!(
                    "resource '{}' has more than one configured controller",
                    controller.resource
                )));
            }
        }

        Ok(())
    }
}

/// Concrete reference resource configuration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "adapter", rename_all = "kebab-case")]
pub enum ResourceConfig {
    Ram {
        id: String,
        host_total: u64,
        min: u64,
        max: u64,
        initial: u64,
        max_step: Option<u64>,
    },
    Concurrency {
        id: String,
        max_width: usize,
        initial_width: usize,
    },
}

impl ResourceConfig {
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::Ram { id, .. } | Self::Concurrency { id, .. } => id,
        }
    }

    fn validate(&self) -> Result<(), RuntimeError> {
        LogicalResourceId::new(self.id()).map_err(|error| {
            RuntimeError::configuration(format!("invalid resource id '{}': {error}", self.id()))
        })?;

        match self {
            Self::Ram {
                host_total,
                min,
                max,
                initial,
                ..
            } => {
                if *min == 0 || min > max || max > host_total {
                    return Err(RuntimeError::configuration(format!(
                        "invalid RAM bounds for '{}': require 0 < min <= max <= host_total",
                        self.id()
                    )));
                }
                if initial < min || initial > max {
                    return Err(RuntimeError::configuration(format!(
                        "invalid RAM initial commitment for '{}': require min <= initial <= max",
                        self.id()
                    )));
                }
            }
            Self::Concurrency {
                max_width,
                initial_width,
                ..
            } => {
                if *max_width == 0 || *initial_width == 0 || initial_width > max_width {
                    return Err(RuntimeError::configuration(format!(
                        "invalid concurrency bounds for '{}': require 0 < initial_width <= max_width",
                        self.id()
                    )));
                }
            }
        }
        Ok(())
    }
}

/// One controller attached to one configured resource.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControllerConfig {
    pub resource: String,
    pub planner: PlannerSelection,
    pub forecaster: ForecasterSelection,
    pub cadence: CadenceConfig,
    pub mode: ExecutionModeConfig,
}

impl ControllerConfig {
    fn validate(&self) -> Result<(), RuntimeError> {
        LogicalResourceId::new(&self.resource).map_err(|error| {
            RuntimeError::configuration(format!(
                "invalid controller resource id '{}': {error}",
                self.resource
            ))
        })?;
        self.planner.validate()?;
        self.forecaster.validate()?;
        self.cadence.validate()?;
        Ok(())
    }

    fn validate_resource_compatibility(
        &self,
        resource: &ResourceConfig,
    ) -> Result<(), RuntimeError> {
        if matches!(resource, ResourceConfig::Concurrency { .. })
            && matches!(
                &self.planner,
                PlannerSelection::Headroom { .. } | PlannerSelection::Threshold { .. }
            )
        {
            return Err(RuntimeError::configuration(format!(
                "planner for resource '{}' requires a capacity resource, but the configured adapter is concurrency",
                self.resource
            )));
        }

        if matches!(&self.planner, PlannerSelection::FirstGrounded)
            && matches!(
                self.mode,
                ExecutionModeConfig::DryRun | ExecutionModeConfig::Apply
            )
        {
            return Err(RuntimeError::configuration(format!(
                "first-grounded planner for resource '{}' does not provide a quantitative target; use observe-only/plan-only or select an actionable planner",
                self.resource
            )));
        }

        Ok(())
    }
}

/// Supported planner selection in operator configuration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PlannerSelection {
    FirstGrounded,
    Headroom {
        headroom_fraction: f64,
        deadband_fraction: f64,
    },
    Threshold {
        low_watermark: f64,
        high_watermark: f64,
        step_fraction: f64,
    },
}

impl PlannerSelection {
    fn validate(&self) -> Result<(), RuntimeError> {
        match self {
            Self::FirstGrounded => Ok(()),
            Self::Headroom {
                headroom_fraction,
                deadband_fraction,
            } => HeadroomPlanner::new(*headroom_fraction, *deadband_fraction)
                .map(|_| ())
                .map_err(|error| RuntimeError::configuration(error.to_string())),
            Self::Threshold {
                low_watermark,
                high_watermark,
                step_fraction,
            } => ThresholdPlanner::new(*low_watermark, *high_watermark, *step_fraction)
                .map(|_| ())
                .map_err(|error| RuntimeError::configuration(error.to_string())),
        }
    }
}

/// Supported forecast selection in operator configuration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ForecasterSelection {
    CurrentState,
    Ewma { alpha: f64, horizon_ms: u64 },
}

impl ForecasterSelection {
    fn validate(&self) -> Result<(), RuntimeError> {
        match self {
            Self::CurrentState => Ok(()),
            Self::Ewma { alpha, horizon_ms } => {
                EwmaForecaster::new(*alpha, Duration::from_millis(*horizon_ms)).map(|_| ())
            }
        }
    }
}

/// One-shot or explicitly bounded periodic cadence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum CadenceConfig {
    OneShot,
    Periodic { interval_ms: u64, max_cycles: u64 },
}

impl CadenceConfig {
    fn validate(&self) -> Result<(), RuntimeError> {
        if let Self::Periodic {
            interval_ms,
            max_cycles,
        } = self
        {
            if *interval_ms == 0 {
                return Err(RuntimeError::configuration(
                    "periodic operator config requires interval_ms > 0",
                ));
            }
            if *max_cycles == 0 {
                return Err(RuntimeError::configuration(
                    "periodic operator config requires max_cycles > 0",
                ));
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn runtime_values(self) -> (Cadence, u64) {
        match self {
            Self::OneShot => (Cadence::OneShot, 0),
            Self::Periodic {
                interval_ms,
                max_cycles,
            } => (
                Cadence::Periodic(Duration::from_millis(interval_ms)),
                max_cycles,
            ),
        }
    }
}

/// Runtime execution mode selected by configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutionModeConfig {
    ObserveOnly,
    PlanOnly,
    DryRun,
    Apply,
}

impl ExecutionModeConfig {
    #[must_use]
    pub const fn runtime_mode(self) -> RuntimeMode {
        match self {
            Self::ObserveOnly => RuntimeMode::ObserveOnly,
            Self::PlanOnly => RuntimeMode::PlanOnly,
            Self::DryRun => RuntimeMode::DryRun,
            Self::Apply => RuntimeMode::Apply,
        }
    }

    #[must_use]
    pub const fn dry_run(self) -> bool {
        !matches!(self, Self::Apply)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_config() -> OperatorConfig {
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
                    deadband_fraction: 0.05,
                },
                forecaster: ForecasterSelection::Ewma {
                    alpha: 0.5,
                    horizon_ms: 1000,
                },
                cadence: CadenceConfig::Periodic {
                    interval_ms: 50,
                    max_cycles: 10,
                },
                mode: ExecutionModeConfig::DryRun,
            }],
        }
    }

    #[test]
    fn complete_valid_configuration_is_accepted() {
        valid_config().validate().unwrap();
    }

    #[test]
    fn unsupported_schema_version_is_rejected() {
        let mut config = valid_config();
        config.version = OPERATOR_CONFIG_VERSION + 1;
        assert!(matches!(
            config.validate(),
            Err(RuntimeError::Configuration(_))
        ));
    }

    #[test]
    fn duplicate_resource_ids_are_rejected() {
        let mut config = valid_config();
        config.resources.push(config.resources[0].clone());
        assert!(config.validate().is_err());
    }

    #[test]
    fn unknown_controller_resource_is_rejected() {
        let mut config = valid_config();
        config.controllers[0].resource = "missing".into();
        assert!(config.validate().is_err());
    }

    #[test]
    fn capacity_planner_on_concurrency_resource_is_rejected() {
        let config = OperatorConfig {
            version: OPERATOR_CONFIG_VERSION,
            resources: vec![ResourceConfig::Concurrency {
                id: "workers".into(),
                max_width: 8,
                initial_width: 4,
            }],
            controllers: vec![ControllerConfig {
                resource: "workers".into(),
                planner: PlannerSelection::Headroom {
                    headroom_fraction: 0.5,
                    deadband_fraction: 0.05,
                },
                forecaster: ForecasterSelection::CurrentState,
                cadence: CadenceConfig::OneShot,
                mode: ExecutionModeConfig::DryRun,
            }],
        };

        assert!(matches!(
            config.validate(),
            Err(RuntimeError::Configuration(_))
        ));
    }

    #[test]
    fn first_grounded_planner_is_allowed_for_non_actuating_concurrency_mode() {
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

        config.validate().unwrap();
    }

    #[test]
    fn first_grounded_planner_is_rejected_for_action_modes() {
        for mode in [ExecutionModeConfig::DryRun, ExecutionModeConfig::Apply] {
            let mut config = valid_config();
            config.controllers[0].planner = PlannerSelection::FirstGrounded;
            config.controllers[0].cadence = CadenceConfig::OneShot;
            config.controllers[0].mode = mode;

            let error = config
                .validate()
                .expect_err("targetless planner must fail before runtime construction");
            assert!(error
                .to_string()
                .contains("does not provide a quantitative target"));
        }
    }

    #[test]
    fn unbounded_periodic_controller_is_rejected() {
        let mut config = valid_config();
        config.controllers[0].cadence = CadenceConfig::Periodic {
            interval_ms: 50,
            max_cycles: 0,
        };
        assert!(config.validate().is_err());
    }
}
