//! Typed resource-telemetry boundary for adaptive model execution.
//!
//! The adaptive model planner consumes generic `FREE_CAPACITY` and `UTILIZATION`
//! observations. This module lets a downstream backend publish those values as a
//! validated [`ModelExecutionResourceSnapshotV1`] without teaching ElasticXxx how
//! to probe that backend's hardware.
//!
//! The provider owns telemetry semantics and provenance. The adapter only checks
//! the declared capacity-unit identity, preserves unsupported/error states, and
//! converts the validated snapshot into the generic runtime observation vocabulary.

use std::error::Error;
use std::time::Instant;

use elastic_adapters::ModelExecutionResourceSnapshotV1;
use elastic_core::resource::ObservationSignalId;
use elastic_eir::PlanningContext;

use crate::{Observation, ObservationSource, Observer, RuntimeError};

/// Largest integer that can be represented exactly in the runtime's `f64`
/// [`PlanningContext`] observation value.
const MAX_EXACT_F64_INTEGER_U64: u64 = 1_u64 << 53;

/// Backend-owned resource telemetry provider for adaptive model execution.
///
/// Implementations may read GPU, accelerator, process, host, simulator, or other
/// resource state, but that behavior remains downstream. ElasticXxx receives only
/// a validated typed snapshot plus explicit provenance.
pub trait ModelExecutionResourceTelemetryV1: Send + Sync {
    /// Backend-specific telemetry failure.
    type Error: Error + Send + Sync + 'static;

    /// Stable observation provenance for audit records.
    fn source(&self) -> ObservationSource;

    /// Read one current typed resource snapshot.
    fn snapshot(&self) -> Result<ModelExecutionResourceSnapshotV1, Self::Error>;
}

/// Runtime [`Observer`] that adapts typed backend telemetry to the generic
/// resource signals consumed by `ModelExecutionAdaptivePlannerV1`.
pub struct ModelExecutionResourceObserverV1<T> {
    telemetry: T,
    expected_capacity_unit: String,
}

impl<T> ModelExecutionResourceObserverV1<T>
where
    T: ModelExecutionResourceTelemetryV1,
{
    /// Bind a telemetry provider to the exact capacity unit expected by the
    /// active model-execution envelope policy.
    ///
    /// # Errors
    ///
    /// Returns a configuration error for an invalid/blank capacity unit.
    pub fn new(
        expected_capacity_unit: impl Into<String>,
        telemetry: T,
    ) -> Result<Self, RuntimeError> {
        let expected_capacity_unit = expected_capacity_unit.into();
        let probe = ModelExecutionResourceSnapshotV1::new(&expected_capacity_unit, 0, 0)
            .map_err(|error| RuntimeError::configuration(error.to_string()))?;
        Ok(Self {
            telemetry,
            expected_capacity_unit: probe.capacity_unit().to_owned(),
        })
    }

    /// Exact policy-owned capacity-unit identity expected from the provider.
    #[must_use]
    pub fn expected_capacity_unit(&self) -> &str {
        &self.expected_capacity_unit
    }

    /// Borrow the downstream telemetry provider.
    #[must_use]
    pub const fn telemetry(&self) -> &T {
        &self.telemetry
    }
}

impl<T> Observer for ModelExecutionResourceObserverV1<T>
where
    T: ModelExecutionResourceTelemetryV1,
{
    fn observe(&self) -> (PlanningContext, Vec<Observation>) {
        let now = Instant::now();
        let source = self.telemetry.source();
        let free_signal = ObservationSignalId::FREE_CAPACITY;
        let utilization_signal = ObservationSignalId::UTILIZATION;

        let snapshot = match self.telemetry.snapshot() {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let reason = format!("model-execution resource telemetry failed: {error}");
                return (
                    PlanningContext::new(),
                    vec![
                        Observation::unsupported_from_source(
                            source.clone(),
                            free_signal,
                            now,
                            reason.clone(),
                        ),
                        Observation::unsupported_from_source(
                            source,
                            utilization_signal,
                            now,
                            reason,
                        ),
                    ],
                );
            }
        };

        let utilization = f64::from(snapshot.utilization_bps()) / 10_000.0;
        let mut context = PlanningContext::new().observe(utilization_signal.clone(), utilization);
        let mut observations = vec![Observation::from_source(
            source.clone(),
            utilization_signal,
            utilization,
            now,
        )];

        if snapshot.capacity_unit() != self.expected_capacity_unit {
            observations.push(Observation::unsupported_from_source(
                source,
                free_signal,
                now,
                format!(
                    "model-execution capacity unit mismatch: expected {:?}, got {:?}",
                    self.expected_capacity_unit,
                    snapshot.capacity_unit()
                ),
            ));
            return (context, observations);
        }

        if snapshot.free_capacity() > MAX_EXACT_F64_INTEGER_U64 {
            observations.push(Observation::unsupported_from_source(
                source,
                free_signal,
                now,
                format!(
                    "model-execution free capacity {} exceeds exact f64 planning limit 2^53",
                    snapshot.free_capacity()
                ),
            ));
            return (context, observations);
        }

        let free_capacity = snapshot.free_capacity() as f64;
        context = context.observe(free_signal.clone(), free_capacity);
        observations.push(Observation::from_source(
            source,
            free_signal,
            free_capacity,
            now,
        ));
        (context, observations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt;

    #[derive(Clone, Debug)]
    struct TelemetryError(&'static str);

    impl fmt::Display for TelemetryError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(self.0)
        }
    }

    impl Error for TelemetryError {}

    #[derive(Clone, Debug)]
    struct FakeTelemetry {
        unit: &'static str,
        free: u64,
        utilization_bps: u16,
        fail: bool,
    }

    impl ModelExecutionResourceTelemetryV1 for FakeTelemetry {
        type Error = TelemetryError;

        fn source(&self) -> ObservationSource {
            ObservationSource::host("fake-model-resource")
        }

        fn snapshot(&self) -> Result<ModelExecutionResourceSnapshotV1, Self::Error> {
            if self.fail {
                return Err(TelemetryError("injected failure"));
            }
            ModelExecutionResourceSnapshotV1::new(self.unit, self.free, self.utilization_bps)
                .map_err(|_| TelemetryError("invalid fake snapshot"))
        }
    }

    #[test]
    fn typed_snapshot_enters_generic_planning_context() {
        let observer = ModelExecutionResourceObserverV1::new(
            "bytes",
            FakeTelemetry {
                unit: "bytes",
                free: 3_000,
                utilization_bps: 8_000,
                fail: false,
            },
        )
        .unwrap();

        let (context, observations) = observer.observe();

        assert_eq!(context.get(ObservationSignalId::FREE_CAPACITY), Some(3_000.0));
        assert_eq!(context.get(ObservationSignalId::UTILIZATION), Some(0.8));
        assert_eq!(observations.len(), 2);
        assert!(observations.iter().all(Observation::is_valid));
        assert!(observations.iter().all(|observation| {
            observation.source() == &ObservationSource::host("fake-model-resource")
        }));
    }

    #[test]
    fn capacity_unit_mismatch_never_enters_free_capacity_context() {
        let observer = ModelExecutionResourceObserverV1::new(
            "bytes",
            FakeTelemetry {
                unit: "mib",
                free: 3_000,
                utilization_bps: 8_000,
                fail: false,
            },
        )
        .unwrap();

        let (context, observations) = observer.observe();

        assert_eq!(context.get(ObservationSignalId::FREE_CAPACITY), None);
        assert_eq!(context.get(ObservationSignalId::UTILIZATION), Some(0.8));
        assert!(observations.iter().any(|observation| {
            observation.signal() == &ObservationSignalId::FREE_CAPACITY
                && observation.is_unsupported()
        }));
    }

    #[test]
    fn telemetry_failure_is_explicit_and_context_is_empty() {
        let observer = ModelExecutionResourceObserverV1::new(
            "bytes",
            FakeTelemetry {
                unit: "bytes",
                free: 0,
                utilization_bps: 0,
                fail: true,
            },
        )
        .unwrap();

        let (context, observations) = observer.observe();

        assert!(context.is_empty());
        assert_eq!(observations.len(), 2);
        assert!(observations.iter().all(Observation::is_unsupported));
    }

    #[test]
    fn inexact_large_capacity_is_not_rounded_into_planning_context() {
        let observer = ModelExecutionResourceObserverV1::new(
            "bytes",
            FakeTelemetry {
                unit: "bytes",
                free: MAX_EXACT_F64_INTEGER_U64 + 1,
                utilization_bps: 5_000,
                fail: false,
            },
        )
        .unwrap();

        let (context, observations) = observer.observe();

        assert_eq!(context.get(ObservationSignalId::FREE_CAPACITY), None);
        assert_eq!(context.get(ObservationSignalId::UTILIZATION), Some(0.5));
        assert!(observations.iter().any(|observation| {
            observation.signal() == &ObservationSignalId::FREE_CAPACITY
                && observation.is_unsupported()
        }));
    }
}
