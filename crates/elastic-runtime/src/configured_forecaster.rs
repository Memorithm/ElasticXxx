//! Materialization of versioned operator forecast selections.
//!
//! Configuration validation and runtime execution meet here: a validated
//! [`ForecasterSelection`] is converted into the concrete forecaster instance
//! that is retained by forecast-aware orchestration.

use crate::{
    CurrentStateForecaster, EwmaForecaster, Forecast, Forecaster, ForecasterSelection,
    ObservationSnapshot, RuntimeError,
};
use elastic_eir::PlanningContext;
use std::time::Duration;

/// Concrete forecaster built from operator configuration.
#[derive(Debug)]
pub enum ConfiguredForecaster {
    CurrentState(CurrentStateForecaster),
    Ewma(EwmaForecaster),
}

impl Forecaster for ConfiguredForecaster {
    fn forecast(
        &self,
        observations: &ObservationSnapshot,
        current: &PlanningContext,
    ) -> Result<Forecast, RuntimeError> {
        match self {
            Self::CurrentState(forecaster) => forecaster.forecast(observations, current),
            Self::Ewma(forecaster) => forecaster.forecast(observations, current),
        }
    }
}

impl ForecasterSelection {
    /// Build the runtime forecaster selected by operator configuration.
    ///
    /// # Errors
    ///
    /// Returns a configuration error for invalid EWMA parameters. Callers do
    /// not need to assume prior validation; construction re-checks its own
    /// invariants.
    pub fn build(&self) -> Result<ConfiguredForecaster, RuntimeError> {
        match self {
            Self::CurrentState => Ok(ConfiguredForecaster::CurrentState(CurrentStateForecaster)),
            Self::Ewma { alpha, horizon_ms } => Ok(ConfiguredForecaster::Ewma(
                EwmaForecaster::new(*alpha, Duration::from_millis(*horizon_ms))?,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use elastic_core::resource::ObservationSignalId;

    #[test]
    fn current_state_selection_materializes_without_confidence_claim() {
        let forecaster = ForecasterSelection::CurrentState.build().unwrap();
        let current = PlanningContext::new().observe(ObservationSignalId::UTILIZATION, 0.25);
        let snapshot = ObservationSnapshot::new(std::time::Instant::now(), Vec::new());

        let forecast = forecaster.forecast(&snapshot, &current).unwrap();

        assert!(forecast.is_available());
        assert_eq!(forecast.confidence, None);
        assert_eq!(
            forecast
                .planning_context()
                .and_then(|context| context.get(ObservationSignalId::UTILIZATION)),
            Some(0.25)
        );
    }

    #[test]
    fn ewma_selection_materializes_one_stateful_instance() {
        let forecaster = ForecasterSelection::Ewma {
            alpha: 0.5,
            horizon_ms: 1000,
        }
        .build()
        .unwrap();
        let snapshot = ObservationSnapshot::new(std::time::Instant::now(), Vec::new());

        let first = forecaster
            .forecast(
                &snapshot,
                &PlanningContext::new().observe(ObservationSignalId::UTILIZATION, 0.2),
            )
            .unwrap();
        let second = forecaster
            .forecast(
                &snapshot,
                &PlanningContext::new().observe(ObservationSignalId::UTILIZATION, 0.8),
            )
            .unwrap();

        assert_eq!(
            first
                .planning_context()
                .and_then(|context| context.get(ObservationSignalId::UTILIZATION)),
            Some(0.2)
        );
        assert_eq!(
            second
                .planning_context()
                .and_then(|context| context.get(ObservationSignalId::UTILIZATION)),
            Some(0.5)
        );
    }

    #[test]
    fn invalid_ewma_selection_cannot_be_materialized() {
        assert!(ForecasterSelection::Ewma {
            alpha: f64::NAN,
            horizon_ms: 1,
        }
        .build()
        .is_err());
    }
}
