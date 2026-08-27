//! Runtime observation system.
//!
//! Generic observer model for collecting telemetry snapshots from resource
//! adapters. Observations are the input to the planning pipeline and must
//! never fabricate values when telemetry is unavailable.
//!
//! # Observation Semantics
//!
//! Every observation has:
//! - A timestamp (monotonic, sourced from the runtime clock)
//! - A source identity (resource + signal identity)
//! - A validity flag (never fabricate zeros for unavailable telemetry)
//! - Optional confidence/quality metadata
//! - An explicit "unsupported" state when the signal cannot be observed
//!
//! # Traits / Adapters
//!
//! Resource adapters implement the [`Observer`] trait to produce
//! `Observation` values from their concrete state. The runtime does not
//! probe the OS directly — adapters supply plain numbers derived from
//! their materialization plus operator-supplied configuration.

use std::fmt;
use std::time::Instant;

use elastic_core::resource::ObservationSignalId;
use elastic_eir::PlanningContext;

/// A single telemetry reading from a resource.
///
/// Invariants:
/// - `value` is never fabricated; if the signal is unavailable, the
///   observer returns [`Observation::Unsupported`].
/// - `source` identifies the resource and signal that produced this reading.
/// - `quality` is `None` when confidence cannot be determined; never fabricated.
/// - `timestamp` is monotonic within a runtime instance.
#[derive(Clone, Debug, PartialEq)]
pub struct Observation {
    /// The observation signal identifier.
    pub signal: ObservationSignalId,
    /// The observed value. Unit-ful; meaning is fixed by the signal and its adapter.
    pub value: f64,
    /// Monotonic time at which this observation was taken.
    pub timestamp: Instant,
    /// Optional confidence/quality metadata. `None` means unknown, not "100%".
    pub quality: Option<f64>,
    /// Whether this observation is valid (as opposed to "unsupported").
    ///
    /// When an observer cannot produce a value for a signal, it returns
    /// [`Observation::unsupported`] with `quality = None`.
    pub valid: bool,
}

impl Observation {
    /// Create a new observation with the given signal and value.
    #[must_use]
    pub fn new(signal: ObservationSignalId, value: f64, timestamp: Instant) -> Self {
        Self {
            signal,
            value,
            timestamp,
            quality: None,
            valid: true,
        }
    }

    /// Create an unsupported observation for a signal that cannot be observed.
    ///
    /// This is the correct way to signal "no telemetry available" rather
    /// than fabricating a zero or ignoring the signal entirely.
    #[must_use]
    pub fn unsupported(signal: ObservationSignalId, timestamp: Instant) -> Self {
        Self {
            signal,
            value: f64::NAN,
            timestamp,
            quality: None,
            valid: false,
        }
    }

    /// Whether this observation represents a valid (non-fabricated) reading.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.valid
    }

    /// Whether this observation is unsupported (no telemetry available).
    #[must_use]
    pub fn is_unsupported(&self) -> bool {
        !self.valid
    }

    /// The signal identifier.
    #[must_use]
    pub const fn signal(&self) -> &ObservationSignalId {
        &self.signal
    }

    /// The observed value.
    #[must_use]
    pub const fn value(&self) -> f64 {
        self.value
    }

    /// The observation timestamp.
    #[must_use]
    pub const fn timestamp(&self) -> &Instant {
        &self.timestamp
    }

    /// Optional quality/confidence metadata.
    #[must_use]
    pub const fn quality(&self) -> Option<f64> {
        self.quality
    }

    /// Convert this observation into an [`PlanningContext`] entry.
    ///
    /// The value is inserted under the signal's canonical ID. If the
    /// observation is unsupported, the context records `None` for that signal.
    #[must_use]
    pub fn into_planning_context(self) -> PlanningContext {
        let ctx = PlanningContext::new();
        if self.valid {
            ctx.observe(self.signal, self.value)
        } else {
            // For unsupported observations, we do not insert a value;
            // the PlanningContext will lack this signal, which is honest.
            ctx
        }
    }
}

impl fmt::Display for Observation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.valid {
            write!(
                f,
                "observation signal={} value={} quality={:?}",
                self.signal, self.value, self.quality
            )
        } else {
            write!(f, "observation unsupported signal={}", self.signal)
        }
    }
}

/// A snapshot of observations collected at a point in the runtime cycle.
///
/// Observations are deterministic: the same observations at the same
/// program point produce the same snapshot. Quality metadata is optional
/// and never fabricated.
#[derive(Clone, Debug, PartialEq)]
pub struct ObservationSnapshot {
    /// The runtime instant at which this snapshot was taken.
    pub timestamp: Instant,
    /// The observations collected.
    pub observations: Vec<Observation>,
    /// Whether all signals in the spec were successfully observed.
    pub all_signals_valid: bool,
}

impl ObservationSnapshot {
    /// Create a new observation snapshot.
    #[must_use]
    pub fn new(timestamp: Instant, observations: Vec<Observation>) -> Self {
        let all_valid = observations.iter().all(|obs| obs.is_valid());
        Self {
            timestamp,
            observations,
            all_signals_valid: all_valid,
        }
    }

    /// Iterate over observations in canonical signal order.
    pub fn iter(&self) -> impl Iterator<Item = &Observation> {
        self.observations.iter()
    }

    /// Look up an observation by signal ID.
    #[must_use]
    pub fn get(&self, signal: ObservationSignalId) -> Option<&Observation> {
        self.observations.iter().find(|obs| obs.signal == signal)
    }

    /// The number of observations in this snapshot.
    #[must_use]
    pub fn len(&self) -> usize {
        self.observations.len()
    }

    /// Whether this snapshot contains no observations.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.observations.is_empty()
    }
}

impl fmt::Display for ObservationSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "ObservationSnapshot @{}",
            self.timestamp.elapsed().as_millis()
        )?;
        for obs in &self.observations {
            writeln!(f, "  - {}", obs)?;
        }
        if !self.observations.is_empty() && !self.all_signals_valid {
            writeln!(f, "  ⚠ Some signals were unsupported (not fabricated)")?;
        }
        Ok(())
    }
}

/// The trait that resource adapters implement to produce observations.
///
/// Adapters are the **only** source of observation values for the runtime.
/// The runtime never probes the OS directly; it asks adapters to observe
/// their own materialized state and supply plain numbers.
pub trait Observer: Send + Sync {
    /// Observe the resource and produce a [`PlanningContext`] plus any
    /// unsupported observations that could not be resolved.
    ///
    /// The default implementation returns an empty context and no observations.
    /// Adapters override this to supply concrete telemetry.
    fn observe(&self) -> (PlanningContext, Vec<Observation>);
}

impl Observer for () {
    fn observe(&self) -> (PlanningContext, Vec<Observation>) {
        (PlanningContext::new(), Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_observation_creation() {
        let obs = Observation::new(
            elastic_core::resource::ObservationSignalId::UTILIZATION,
            0.75,
            std::time::Instant::now(),
        );
        assert!(obs.is_valid());
        assert_eq!(obs.signal().as_str(), "utilization");
        assert_eq!(obs.value(), 0.75);
    }

    #[test]
    fn test_unsupported_observation() {
        let obs = Observation::unsupported(
            elastic_core::resource::ObservationSignalId::FREE_CAPACITY,
            std::time::Instant::now(),
        );
        assert!(!obs.is_valid());
        assert!(obs.is_unsupported());
        assert!(obs.value().is_nan());
    }

    #[test]
    fn test_observation_into_context() {
        let obs = Observation::new(
            elastic_core::resource::ObservationSignalId::UTILIZATION,
            0.75,
            std::time::Instant::now(),
        );
        let ctx = obs.into_planning_context();
        assert_eq!(
            ctx.get(elastic_core::resource::ObservationSignalId::UTILIZATION),
            Some(0.75)
        );
    }

    #[test]
    fn test_observation_display() {
        let obs = Observation::new(
            elastic_core::resource::ObservationSignalId::UTILIZATION,
            0.75,
            std::time::Instant::now(),
        );
        let s = format!("{}", obs);
        assert!(s.contains("utilization"));
        assert!(s.contains("0.75"));
    }

    #[test]
    fn test_observation_snapshot() {
        let now = std::time::Instant::now();
        let snapshot = ObservationSnapshot::new(
            now,
            vec![
                Observation::new(
                    elastic_core::resource::ObservationSignalId::UTILIZATION,
                    0.75,
                    now,
                ),
                Observation::unsupported(
                    elastic_core::resource::ObservationSignalId::FREE_CAPACITY,
                    now,
                ),
            ],
        );
        assert_eq!(snapshot.len(), 2);
        assert!(!snapshot.all_signals_valid);
        let displayed = format!("{}", snapshot);
        assert!(displayed.contains("⚠ Some signals were unsupported"));
    }

    #[test]
    fn test_observer_trait() {
        struct DummyAdapter;
        impl Observer for DummyAdapter {
            fn observe(&self) -> (PlanningContext, Vec<Observation>) {
                let ctx = PlanningContext::new();
                let obs = Observation::new(
                    elastic_core::resource::ObservationSignalId::UTILIZATION,
                    0.5,
                    std::time::Instant::now(),
                );
                (ctx, vec![obs])
            }
        }
        let adapter = DummyAdapter;
        let (_ctx, obs) = adapter.observe();
        assert_eq!(obs.len(), 1);
        assert!(obs[0].is_valid());
    }
}
