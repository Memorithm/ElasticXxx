//! Runtime observation system.
//!
//! Generic observer model for collecting telemetry snapshots from resource
//! adapters. Observations are the input to the planning pipeline and must
//! never fabricate values when telemetry is unavailable.

use std::fmt;
use std::time::Instant;

use elastic_core::resource::{LogicalResourceId, ObservationSignalId};
use elastic_eir::PlanningContext;

/// Identity of the component that produced an observation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObservationSource {
    /// A concrete logical elastic resource.
    Resource(LogicalResourceId),
    /// Host-level telemetry supplied by an OS-specific provider.
    Host { provider: String },
    /// Runtime-local telemetry such as controller timing.
    Runtime { component: String },
}

impl ObservationSource {
    #[must_use]
    pub fn host(provider: impl Into<String>) -> Self {
        Self::Host {
            provider: provider.into(),
        }
    }

    #[must_use]
    pub fn runtime(component: impl Into<String>) -> Self {
        Self::Runtime {
            component: component.into(),
        }
    }
}

impl fmt::Display for ObservationSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Resource(resource) => write!(f, "resource:{}", resource.as_str()),
            Self::Host { provider } => write!(f, "host:{provider}"),
            Self::Runtime { component } => write!(f, "runtime:{component}"),
        }
    }
}

/// A single telemetry reading from a resource or runtime provider.
///
/// Unsupported telemetry is represented explicitly through `valid = false`
/// and `unsupported_reason`; its numeric field is `NaN` rather than a
/// fabricated zero. Concrete runtime providers always use an explicit
/// [`ObservationSource`].
#[derive(Clone, Debug, PartialEq)]
pub struct Observation {
    /// Identity of the provider that produced this reading.
    pub source: ObservationSource,
    /// The observation signal identifier.
    pub signal: ObservationSignalId,
    /// The observed value. Unit-ful; meaning is fixed by the signal and its adapter.
    pub value: f64,
    /// Monotonic time at which this observation was taken.
    pub timestamp: Instant,
    /// Optional confidence/quality metadata. `None` means unknown, not "100%".
    pub quality: Option<f64>,
    /// Whether this observation is valid (as opposed to unsupported).
    pub valid: bool,
    /// Why the signal was unsupported, when known.
    pub unsupported_reason: Option<String>,
}

impl Observation {
    /// Compatibility constructor for callers that have not yet supplied
    /// observation provenance explicitly.
    ///
    /// New providers should prefer [`Observation::from_source`].
    #[must_use]
    pub fn new(signal: ObservationSignalId, value: f64, timestamp: Instant) -> Self {
        Self::from_source(
            ObservationSource::runtime("legacy-unspecified"),
            signal,
            value,
            timestamp,
        )
    }

    /// Create a valid observation from an explicit provider identity.
    #[must_use]
    pub fn from_source(
        source: ObservationSource,
        signal: ObservationSignalId,
        value: f64,
        timestamp: Instant,
    ) -> Self {
        Self {
            source,
            signal,
            value,
            timestamp,
            quality: None,
            valid: true,
            unsupported_reason: None,
        }
    }

    /// Compatibility constructor for unsupported telemetry without explicit
    /// provenance.
    #[must_use]
    pub fn unsupported(signal: ObservationSignalId, timestamp: Instant) -> Self {
        Self::unsupported_from_source(
            ObservationSource::runtime("legacy-unspecified"),
            signal,
            timestamp,
            "provider did not expose this signal",
        )
    }

    /// Create an unsupported observation without inventing a numeric value.
    #[must_use]
    pub fn unsupported_from_source(
        source: ObservationSource,
        signal: ObservationSignalId,
        timestamp: Instant,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            source,
            signal,
            value: f64::NAN,
            timestamp,
            quality: None,
            valid: false,
            unsupported_reason: Some(reason.into()),
        }
    }

    /// Whether this observation represents a valid, non-fabricated reading.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.valid
    }

    /// Whether this observation is unsupported.
    #[must_use]
    pub fn is_unsupported(&self) -> bool {
        !self.valid
    }

    #[must_use]
    pub const fn source(&self) -> &ObservationSource {
        &self.source
    }

    #[must_use]
    pub const fn signal(&self) -> &ObservationSignalId {
        &self.signal
    }

    #[must_use]
    pub const fn value(&self) -> f64 {
        self.value
    }

    #[must_use]
    pub const fn timestamp(&self) -> &Instant {
        &self.timestamp
    }

    #[must_use]
    pub const fn quality(&self) -> Option<f64> {
        self.quality
    }

    #[must_use]
    pub fn unsupported_reason(&self) -> Option<&str> {
        self.unsupported_reason.as_deref()
    }

    /// Convert this observation into one planning-context entry.
    #[must_use]
    pub fn into_planning_context(self) -> PlanningContext {
        let context = PlanningContext::new();
        if self.valid {
            context.observe(self.signal, self.value)
        } else {
            context
        }
    }
}

impl fmt::Display for Observation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.valid {
            write!(
                f,
                "observation source={} signal={} value={} quality={:?}",
                self.source, self.signal, self.value, self.quality
            )
        } else {
            write!(
                f,
                "observation unsupported source={} signal={} reason={}",
                self.source,
                self.signal,
                self.unsupported_reason.as_deref().unwrap_or("unspecified")
            )
        }
    }
}

/// A snapshot of observations collected at a point in the runtime cycle.
#[derive(Clone, Debug, PartialEq)]
pub struct ObservationSnapshot {
    /// The runtime instant at which this snapshot was taken.
    pub timestamp: Instant,
    /// The observations collected.
    pub observations: Vec<Observation>,
    /// Whether all emitted signals were successfully observed.
    pub all_signals_valid: bool,
}

impl ObservationSnapshot {
    #[must_use]
    pub fn new(timestamp: Instant, observations: Vec<Observation>) -> Self {
        let all_signals_valid = observations.iter().all(Observation::is_valid);
        Self {
            timestamp,
            observations,
            all_signals_valid,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Observation> {
        self.observations.iter()
    }

    #[must_use]
    pub fn get(&self, signal: ObservationSignalId) -> Option<&Observation> {
        self.observations
            .iter()
            .find(|observation| observation.signal == signal)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.observations.len()
    }

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
        for observation in &self.observations {
            writeln!(f, "  - {observation}")?;
        }
        if !self.observations.is_empty() && !self.all_signals_valid {
            writeln!(f, "  some signals were unsupported (not fabricated)")?;
        }
        Ok(())
    }
}

/// Provider contract for runtime observations.
///
/// The returned [`PlanningContext`] is the exact planner-facing view; the
/// observation records are its auditable telemetry evidence plus any
/// unsupported signals that could not be represented in the context.
pub trait Observer: Send + Sync {
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
    fn explicit_source_is_preserved() {
        let source = ObservationSource::host("test-provider");
        let observation = Observation::from_source(
            source.clone(),
            ObservationSignalId::UTILIZATION,
            0.75,
            Instant::now(),
        );

        assert_eq!(observation.source(), &source);
        assert!(observation.is_valid());
        assert_eq!(observation.value(), 0.75);
    }

    #[test]
    fn unsupported_observation_carries_reason_without_zero() {
        let observation = Observation::unsupported_from_source(
            ObservationSource::host("test-provider"),
            ObservationSignalId::FREE_CAPACITY,
            Instant::now(),
            "signal unavailable",
        );

        assert!(observation.is_unsupported());
        assert!(observation.value().is_nan());
        assert_eq!(observation.unsupported_reason(), Some("signal unavailable"));
    }

    #[test]
    fn valid_observation_enters_planning_context() {
        let observation = Observation::from_source(
            ObservationSource::runtime("test"),
            ObservationSignalId::UTILIZATION,
            0.75,
            Instant::now(),
        );
        let context = observation.into_planning_context();

        assert_eq!(context.get(ObservationSignalId::UTILIZATION), Some(0.75));
    }

    #[test]
    fn snapshot_reports_unsupported_signals() {
        let now = Instant::now();
        let snapshot = ObservationSnapshot::new(
            now,
            vec![
                Observation::from_source(
                    ObservationSource::runtime("test"),
                    ObservationSignalId::UTILIZATION,
                    0.75,
                    now,
                ),
                Observation::unsupported_from_source(
                    ObservationSource::runtime("test"),
                    ObservationSignalId::FREE_CAPACITY,
                    now,
                    "not exposed",
                ),
            ],
        );

        assert_eq!(snapshot.len(), 2);
        assert!(!snapshot.all_signals_valid);
    }
}
