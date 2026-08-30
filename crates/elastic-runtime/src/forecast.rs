//! Forecasting contracts between observation and planning.
//!
//! Forecasting is explicit and auditable. It is not synonymous with machine
//! learning: deterministic policies may project the current state, smooth
//! measurements, or report that forecasting is unsupported. Unknown forecast
//! evidence is never converted into a fabricated planner input.

use std::sync::Mutex;
use std::time::Duration;

use elastic_core::resource::ObservationSignalId;
use elastic_eir::PlanningContext;

use crate::{ObservationSnapshot, RuntimeError};

/// Whether a forecast produced planner-facing evidence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ForecastStatus {
    Available,
    Unsupported,
    Inconclusive,
}

/// Auditable output of a forecasting stage.
#[derive(Clone, Debug, PartialEq)]
pub struct Forecast {
    pub status: ForecastStatus,
    pub context: Option<PlanningContext>,
    pub horizon: Duration,
    pub method: String,
    /// Optional model confidence. `None` means no calibrated confidence claim.
    pub confidence: Option<f64>,
    pub detail: Option<String>,
}

impl Forecast {
    #[must_use]
    pub fn available(
        context: PlanningContext,
        horizon: Duration,
        method: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            status: ForecastStatus::Available,
            context: Some(context),
            horizon,
            method: method.into(),
            confidence: None,
            detail: Some(detail.into()),
        }
    }

    #[must_use]
    pub fn unsupported(
        horizon: Duration,
        method: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            status: ForecastStatus::Unsupported,
            context: None,
            horizon,
            method: method.into(),
            confidence: None,
            detail: Some(detail.into()),
        }
    }

    #[must_use]
    pub fn inconclusive(
        horizon: Duration,
        method: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            status: ForecastStatus::Inconclusive,
            context: None,
            horizon,
            method: method.into(),
            confidence: None,
            detail: Some(detail.into()),
        }
    }

    #[must_use]
    pub fn planning_context(&self) -> Option<&PlanningContext> {
        self.context.as_ref()
    }

    #[must_use]
    pub fn is_available(&self) -> bool {
        self.status == ForecastStatus::Available && self.context.is_some()
    }
}

/// Forecast provider invoked after observation and before planning.
pub trait Forecaster: Send + Sync {
    fn forecast(
        &self,
        observations: &ObservationSnapshot,
        current: &PlanningContext,
    ) -> Result<Forecast, RuntimeError>;
}

/// Compatibility forecast: expose the current valid observation context at a
/// zero horizon without claiming prediction confidence.
#[derive(Clone, Copy, Debug, Default)]
pub struct CurrentStateForecaster;

impl Forecaster for CurrentStateForecaster {
    fn forecast(
        &self,
        _observations: &ObservationSnapshot,
        current: &PlanningContext,
    ) -> Result<Forecast, RuntimeError> {
        Ok(Forecast::available(
            current.clone(),
            Duration::ZERO,
            "current-state",
            "zero-horizon projection of current valid observations",
        ))
    }
}

/// Deterministic exponentially weighted projection of observed signals.
///
/// This is a simple control baseline, not a claim of statistical optimality.
/// Each valid planner-facing signal is updated as
/// `alpha * current + (1 - alpha) * previous` and the resulting smoothed value
/// is projected over the configured horizon.
#[derive(Debug)]
pub struct EwmaForecaster {
    alpha: f64,
    horizon: Duration,
    state: Mutex<Vec<(ObservationSignalId, f64)>>,
}

impl EwmaForecaster {
    /// Construct an EWMA forecaster.
    ///
    /// # Errors
    ///
    /// Returns a configuration error unless `alpha` is finite and in `(0, 1]`.
    pub fn new(alpha: f64, horizon: Duration) -> Result<Self, RuntimeError> {
        if !alpha.is_finite() || alpha <= 0.0 || alpha > 1.0 {
            return Err(RuntimeError::configuration(
                "EWMA alpha must be finite and satisfy 0 < alpha <= 1",
            ));
        }
        Ok(Self {
            alpha,
            horizon,
            state: Mutex::new(Vec::new()),
        })
    }
}

impl Forecaster for EwmaForecaster {
    fn forecast(
        &self,
        _observations: &ObservationSnapshot,
        current: &PlanningContext,
    ) -> Result<Forecast, RuntimeError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| RuntimeError::planning("EWMA forecast state lock was poisoned"))?;
        let mut projected = PlanningContext::new();

        for (signal, current_value) in current.iter() {
            let value = if let Some((_, previous)) = state
                .iter_mut()
                .find(|(known_signal, _)| known_signal == signal)
            {
                let smoothed = self.alpha.mul_add(current_value, (1.0 - self.alpha) * *previous);
                *previous = smoothed;
                smoothed
            } else {
                state.push((signal.clone(), current_value));
                current_value
            };
            projected = projected.observe(signal.clone(), value);
        }

        Ok(Forecast::available(
            projected,
            self.horizon,
            "ewma",
            format!("deterministic EWMA projection with alpha={}", self.alpha),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use elastic_core::resource::ObservationSignalId;

    #[test]
    fn current_state_forecaster_preserves_context_without_confidence_claim() {
        let context = PlanningContext::new().observe(ObservationSignalId::UTILIZATION, 0.75);
        let snapshot = ObservationSnapshot::new(std::time::Instant::now(), Vec::new());
        let forecast = CurrentStateForecaster
            .forecast(&snapshot, &context)
            .expect("current-state forecast should succeed");

        assert!(forecast.is_available());
        assert_eq!(forecast.horizon, Duration::ZERO);
        assert_eq!(forecast.confidence, None);
        assert_eq!(
            forecast
                .planning_context()
                .and_then(|context| context.get(ObservationSignalId::UTILIZATION)),
            Some(0.75)
        );
    }

    #[test]
    fn ewma_is_deterministic_across_updates() {
        let forecaster = EwmaForecaster::new(0.5, Duration::from_secs(1)).unwrap();
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
    fn invalid_ewma_alpha_is_rejected() {
        assert!(EwmaForecaster::new(0.0, Duration::ZERO).is_err());
        assert!(EwmaForecaster::new(1.1, Duration::ZERO).is_err());
        assert!(EwmaForecaster::new(f64::NAN, Duration::ZERO).is_err());
    }
}
