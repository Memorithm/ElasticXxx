//! Runtime derivation of stable Boolean facts from explicit telemetry evidence.
//!
//! This layer converts existing [`PlanningContext`] and [`ObservationSnapshot`]
//! evidence into bounded, provenance-bound three-valued facts. It never turns
//! missing, unsupported, stale, NaN, or infinite telemetry into a positive
//! Boolean assertion. The resulting [`FactSnapshot`] implements
//! [`GuardFactSource`] and can therefore feed the BE4 guard evaluator without a
//! second policy semantics.

use crate::{ObservationSnapshot, ObservationSource};
use elastic_core::resource::{LogicalResourceId, ObservationSignalId};
use elastic_core::{
    FreshnessSnapshot, GuardFactSource, ObservationEpoch, PredicateKey, ResourceGeneration,
    TruthValue, MAX_REGISTERED_PREDICATES,
};
use elastic_eir::PlanningContext;
use std::collections::BTreeMap;
use std::fmt;
use std::time::{Duration, Instant};

/// Maximum number of facts materialized in one snapshot while the core uses a
/// single `u64` predicate mask.
pub const MAX_FACTS_PER_SNAPSHOT: usize = MAX_REGISTERED_PREDICATES;

/// Maximum byte length of a runtime fact-source identity.
pub const MAX_FACT_SOURCE_ID_BYTES: usize = 256;

/// Stable human-readable identity of the runtime component deriving a fact set.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FactSourceId(String);

impl FactSourceId {
    /// Construct a non-empty, trimmed, bounded source identity.
    pub fn new(value: impl Into<String>) -> Result<Self, FactDerivationError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(FactDerivationError::InvalidSourceId);
        }
        if value.len() > MAX_FACT_SOURCE_ID_BYTES {
            return Err(FactDerivationError::SourceIdTooLong {
                max_bytes: MAX_FACT_SOURCE_ID_BYTES,
                actual_bytes: value.len(),
            });
        }
        if value.trim() != value {
            return Err(FactDerivationError::InvalidSourceId);
        }
        Ok(Self(value))
    }

    /// Borrow the stable source identity.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FactSourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Optional logical-resource generation bound to a fact snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FactResourceBinding {
    resource: LogicalResourceId,
    generation: ResourceGeneration,
}

impl FactResourceBinding {
    /// Bind a fact derivation to one observed resource generation.
    #[must_use]
    pub fn new(resource: LogicalResourceId, generation: ResourceGeneration) -> Self {
        Self {
            resource,
            generation,
        }
    }

    /// Logical resource whose state influenced the facts.
    #[must_use]
    pub const fn resource(&self) -> &LogicalResourceId {
        &self.resource
    }

    /// Resource generation observed during derivation.
    #[must_use]
    pub const fn generation(&self) -> ResourceGeneration {
        self.generation
    }
}

/// Inputs available to one deterministic predicate evaluator.
pub struct PredicateEvaluationInput<'a> {
    planning_context: &'a PlanningContext,
    observations: &'a ObservationSnapshot,
    now: Instant,
}

impl<'a> PredicateEvaluationInput<'a> {
    /// Bind one planner-facing numeric context to its auditable observation
    /// evidence at a caller-supplied monotonic instant.
    #[must_use]
    pub fn new(
        planning_context: &'a PlanningContext,
        observations: &'a ObservationSnapshot,
        now: Instant,
    ) -> Self {
        Self {
            planning_context,
            observations,
            now,
        }
    }

    /// Planner-facing numeric observations.
    #[must_use]
    pub const fn planning_context(&self) -> &PlanningContext {
        self.planning_context
    }

    /// Auditable observation records including unsupported signals.
    #[must_use]
    pub const fn observations(&self) -> &ObservationSnapshot {
        self.observations
    }

    /// Monotonic instant used for freshness evaluation.
    #[must_use]
    pub const fn now(&self) -> Instant {
        self.now
    }
}

/// One deterministic, side-effect-free Boolean predicate derivation.
pub trait PredicateEvaluator: Send + Sync {
    /// Stable predicate identity produced by this evaluator.
    fn key(&self) -> &PredicateKey;

    /// Derive one three-valued fact from explicit runtime evidence.
    ///
    /// Evaluators must return [`TruthValue::Unknown`] when required evidence is
    /// absent or unusable rather than fabricating a Boolean value.
    fn evaluate(&self, input: &PredicateEvaluationInput<'_>) -> TruthValue;
}

/// Numeric comparison used by [`ObservationThresholdPredicate`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThresholdComparison {
    /// `value < threshold`.
    LessThan,
    /// `value <= threshold`.
    LessOrEqual,
    /// `value > threshold`.
    GreaterThan,
    /// `value >= threshold`.
    GreaterOrEqual,
}

impl ThresholdComparison {
    fn evaluate(self, value: f64, threshold: f64) -> TruthValue {
        let result = match self {
            Self::LessThan => value < threshold,
            Self::LessOrEqual => value <= threshold,
            Self::GreaterThan => value > threshold,
            Self::GreaterOrEqual => value >= threshold,
        };
        if result {
            TruthValue::True
        } else {
            TruthValue::False
        }
    }
}

/// Threshold predicate whose numeric value comes from the existing
/// [`PlanningContext`] and whose validity/freshness is proven by the matching
/// observation record.
#[derive(Clone, Debug)]
pub struct ObservationThresholdPredicate {
    key: PredicateKey,
    signal: ObservationSignalId,
    comparison: ThresholdComparison,
    threshold: f64,
    max_age: Duration,
}

impl ObservationThresholdPredicate {
    /// Construct a bounded threshold predicate.
    ///
    /// Non-finite thresholds are rejected because no deterministic runtime fact
    /// should encode NaN/Infinity comparison semantics.
    pub fn new(
        key: PredicateKey,
        signal: ObservationSignalId,
        comparison: ThresholdComparison,
        threshold: f64,
        max_age: Duration,
    ) -> Result<Self, FactDerivationError> {
        if !threshold.is_finite() {
            return Err(FactDerivationError::NonFiniteThreshold);
        }
        Ok(Self {
            key,
            signal,
            comparison,
            threshold,
            max_age,
        })
    }
}

impl PredicateEvaluator for ObservationThresholdPredicate {
    fn key(&self) -> &PredicateKey {
        &self.key
    }

    fn evaluate(&self, input: &PredicateEvaluationInput<'_>) -> TruthValue {
        let Some(observation) = input.observations.get(self.signal.clone()) else {
            return TruthValue::Unknown;
        };
        if !observation.is_valid() || !observation.value().is_finite() {
            return TruthValue::Unknown;
        }
        let Some(age) = input.now.checked_duration_since(*observation.timestamp()) else {
            return TruthValue::Unknown;
        };
        if age > self.max_age {
            return TruthValue::Unknown;
        }
        let Some(value) = input.planning_context.get(self.signal.clone()) else {
            return TruthValue::Unknown;
        };
        if !value.is_finite() {
            return TruthValue::Unknown;
        }
        self.comparison.evaluate(value, self.threshold)
    }
}

/// Predicate that records whether one concrete signal has explicit usable
/// evidence in the current observation snapshot.
#[derive(Clone, Debug)]
pub struct ObservationPresencePredicate {
    key: PredicateKey,
    signal: ObservationSignalId,
}

impl ObservationPresencePredicate {
    /// Create an evidence-presence predicate.
    #[must_use]
    pub fn new(key: PredicateKey, signal: ObservationSignalId) -> Self {
        Self { key, signal }
    }
}

impl PredicateEvaluator for ObservationPresencePredicate {
    fn key(&self) -> &PredicateKey {
        &self.key
    }

    fn evaluate(&self, input: &PredicateEvaluationInput<'_>) -> TruthValue {
        let Some(observation) = input.observations.get(self.signal.clone()) else {
            return TruthValue::Unknown;
        };
        if observation.is_valid() && observation.value().is_finite() {
            TruthValue::True
        } else {
            TruthValue::False
        }
    }
}

/// Predicate that proves whether one valid finite observation is no older than
/// a configured monotonic age bound.
#[derive(Clone, Debug)]
pub struct ObservationFreshnessPredicate {
    key: PredicateKey,
    signal: ObservationSignalId,
    max_age: Duration,
}

impl ObservationFreshnessPredicate {
    /// Create a signal freshness predicate.
    #[must_use]
    pub fn new(key: PredicateKey, signal: ObservationSignalId, max_age: Duration) -> Self {
        Self {
            key,
            signal,
            max_age,
        }
    }
}

impl PredicateEvaluator for ObservationFreshnessPredicate {
    fn key(&self) -> &PredicateKey {
        &self.key
    }

    fn evaluate(&self, input: &PredicateEvaluationInput<'_>) -> TruthValue {
        let Some(observation) = input.observations.get(self.signal.clone()) else {
            return TruthValue::Unknown;
        };
        if !observation.is_valid() || !observation.value().is_finite() {
            return TruthValue::Unknown;
        }
        let Some(age) = input.now.checked_duration_since(*observation.timestamp()) else {
            return TruthValue::Unknown;
        };
        if age <= self.max_age {
            TruthValue::True
        } else {
            TruthValue::False
        }
    }
}

/// Predicate derived from an explicitly discovered capability state.
///
/// `None` means the capability snapshot did not establish availability or
/// absence. Callers must not construct `Some(true)` from planner guesses.
#[derive(Clone, Debug)]
pub struct CapabilityPredicate {
    key: PredicateKey,
    available: Option<bool>,
}

impl CapabilityPredicate {
    /// Bind a predicate to an explicit capability-discovery result.
    #[must_use]
    pub fn new(key: PredicateKey, available: Option<bool>) -> Self {
        Self { key, available }
    }
}

impl PredicateEvaluator for CapabilityPredicate {
    fn key(&self) -> &PredicateKey {
        &self.key
    }

    fn evaluate(&self, _input: &PredicateEvaluationInput<'_>) -> TruthValue {
        match self.available {
            Some(true) => TruthValue::True,
            Some(false) => TruthValue::False,
            None => TruthValue::Unknown,
        }
    }
}

/// A bounded, deterministic fact set bound to one observation epoch and
/// optional resource generation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FactSnapshot {
    source: FactSourceId,
    observation_epoch: ObservationEpoch,
    observation_timestamp: Instant,
    derived_at: Instant,
    resource_binding: Option<FactResourceBinding>,
    facts: BTreeMap<PredicateKey, TruthValue>,
}

impl FactSnapshot {
    /// Derive a fact snapshot in stable predicate-key order.
    ///
    /// # Errors
    ///
    /// Rejects duplicate predicate outputs and snapshots exceeding the current
    /// compact-core capacity.
    pub fn derive(
        source: FactSourceId,
        observation_epoch: ObservationEpoch,
        resource_binding: Option<FactResourceBinding>,
        input: &PredicateEvaluationInput<'_>,
        evaluators: &[&dyn PredicateEvaluator],
    ) -> Result<Self, FactDerivationError> {
        if evaluators.len() > MAX_FACTS_PER_SNAPSHOT {
            return Err(FactDerivationError::TooManyFacts {
                max: MAX_FACTS_PER_SNAPSHOT,
                actual: evaluators.len(),
            });
        }
        let mut facts = BTreeMap::new();
        for evaluator in evaluators {
            let key = evaluator.key().clone();
            if facts
                .insert(key.clone(), evaluator.evaluate(input))
                .is_some()
            {
                return Err(FactDerivationError::DuplicatePredicate { key });
            }
        }
        Ok(Self {
            source,
            observation_epoch,
            observation_timestamp: input.observations.timestamp,
            derived_at: input.now,
            resource_binding,
            facts,
        })
    }

    /// Runtime component that derived this snapshot.
    #[must_use]
    pub const fn source(&self) -> &FactSourceId {
        &self.source
    }

    /// Observation epoch used by this snapshot.
    #[must_use]
    pub const fn observation_epoch(&self) -> ObservationEpoch {
        self.observation_epoch
    }

    /// Timestamp of the underlying observation snapshot.
    #[must_use]
    pub const fn observation_timestamp(&self) -> Instant {
        self.observation_timestamp
    }

    /// Monotonic time at which Boolean derivation ran.
    #[must_use]
    pub const fn derived_at(&self) -> Instant {
        self.derived_at
    }

    /// Optional logical resource/generation dependency.
    #[must_use]
    pub const fn resource_binding(&self) -> Option<&FactResourceBinding> {
        self.resource_binding.as_ref()
    }

    /// Stable ordered fact iterator.
    pub fn iter(&self) -> impl Iterator<Item = (&PredicateKey, TruthValue)> {
        self.facts.iter().map(|(key, value)| (key, *value))
    }

    /// Number of materialized facts.
    #[must_use]
    pub fn len(&self) -> usize {
        self.facts.len()
    }

    /// Whether no facts were derived.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.facts.is_empty()
    }

    /// Reject stale epoch or resource-generation bindings before a snapshot is
    /// used by planning/actuation-adjacent code.
    pub fn validate_freshness(
        &self,
        current: &FreshnessSnapshot,
    ) -> Result<(), FactFreshnessError> {
        if self.observation_epoch != current.observation_epoch() {
            return Err(FactFreshnessError::ObservationEpochMismatch {
                snapshot: self.observation_epoch,
                current: current.observation_epoch(),
            });
        }
        if let Some(binding) = &self.resource_binding {
            let Some(current_generation) = current.resource_generation(binding.resource()) else {
                return Err(FactFreshnessError::MissingResourceGeneration {
                    resource: binding.resource().clone(),
                });
            };
            if binding.generation() != current_generation {
                return Err(FactFreshnessError::ResourceGenerationMismatch {
                    resource: binding.resource().clone(),
                    snapshot: binding.generation(),
                    current: current_generation,
                });
            }
        }
        Ok(())
    }
}

impl GuardFactSource for FactSnapshot {
    fn truth(&self, key: &PredicateKey) -> TruthValue {
        self.facts.get(key).copied().unwrap_or(TruthValue::Unknown)
    }
}

/// Runtime fact-derivation construction failures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FactDerivationError {
    /// Source identity was empty, untrimmed, or otherwise unusable.
    InvalidSourceId,
    /// Source identity exceeded the bounded byte length.
    SourceIdTooLong {
        /// Maximum allowed bytes.
        max_bytes: usize,
        /// Actual byte count.
        actual_bytes: usize,
    },
    /// A numeric threshold was NaN or infinite.
    NonFiniteThreshold,
    /// Two evaluators attempted to produce the same stable predicate key.
    DuplicatePredicate {
        /// Duplicated stable identity.
        key: PredicateKey,
    },
    /// Evaluator count exceeded the current fact-mask capacity.
    TooManyFacts {
        /// Maximum supported facts.
        max: usize,
        /// Requested evaluator count.
        actual: usize,
    },
}

impl fmt::Display for FactDerivationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSourceId => {
                f.write_str("fact source identity must be non-empty and trimmed")
            }
            Self::SourceIdTooLong {
                max_bytes,
                actual_bytes,
            } => write!(
                f,
                "fact source identity has {actual_bytes} bytes; maximum is {max_bytes}"
            ),
            Self::NonFiniteThreshold => f.write_str("predicate threshold must be finite"),
            Self::DuplicatePredicate { key } => {
                write!(f, "multiple evaluators derive predicate {key}")
            }
            Self::TooManyFacts { max, actual } => {
                write!(
                    f,
                    "fact snapshot requested {actual} predicates; maximum is {max}"
                )
            }
        }
    }
}

impl std::error::Error for FactDerivationError {}

/// Stale fact-snapshot binding detected before guarded planning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FactFreshnessError {
    /// Facts were derived from a different observation epoch.
    ObservationEpochMismatch {
        /// Epoch bound to the fact snapshot.
        snapshot: ObservationEpoch,
        /// Current trusted observation epoch.
        current: ObservationEpoch,
    },
    /// The logical resource dependency disappeared from the current snapshot.
    MissingResourceGeneration {
        /// Missing resource.
        resource: LogicalResourceId,
    },
    /// The logical resource has changed since fact derivation.
    ResourceGenerationMismatch {
        /// Changed resource.
        resource: LogicalResourceId,
        /// Generation bound to the facts.
        snapshot: ResourceGeneration,
        /// Current trusted generation.
        current: ResourceGeneration,
    },
}

impl fmt::Display for FactFreshnessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ObservationEpochMismatch { snapshot, current } => write!(
                f,
                "fact observation epoch {snapshot} does not match current epoch {current}"
            ),
            Self::MissingResourceGeneration { resource } => write!(
                f,
                "fact snapshot depends on resource {} whose current generation is unavailable",
                resource.as_str()
            ),
            Self::ResourceGenerationMismatch {
                resource,
                snapshot,
                current,
            } => write!(
                f,
                "fact snapshot resource {} generation {snapshot} does not match current generation {current}",
                resource.as_str()
            ),
        }
    }
}

impl std::error::Error for FactFreshnessError {}

/// Return the concrete observation source for one signal when present.
#[must_use]
pub fn observation_source_for(
    snapshot: &ObservationSnapshot,
    signal: ObservationSignalId,
) -> Option<&ObservationSource> {
    snapshot.get(signal).map(|observation| observation.source())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Observation, ObservationSource};
    use elastic_core::{PlannerEpoch, PredicateKey};

    fn key(name: &str) -> PredicateKey {
        PredicateKey::new("elastic.fact", name).unwrap()
    }

    fn input<'a>(
        context: &'a PlanningContext,
        snapshot: &'a ObservationSnapshot,
        now: Instant,
    ) -> PredicateEvaluationInput<'a> {
        PredicateEvaluationInput::new(context, snapshot, now)
    }

    #[test]
    fn threshold_requires_finite_fresh_explicit_observation() {
        let now = Instant::now();
        let signal = ObservationSignalId::UTILIZATION;
        let predicate = ObservationThresholdPredicate::new(
            key("util-high"),
            signal.clone(),
            ThresholdComparison::GreaterOrEqual,
            0.8,
            Duration::from_secs(5),
        )
        .unwrap();

        let context = PlanningContext::new().observe(signal.clone(), 0.9);
        let fresh = ObservationSnapshot::new(
            now,
            vec![Observation::from_source(
                ObservationSource::runtime("test"),
                signal.clone(),
                0.9,
                now,
            )],
        );
        assert_eq!(
            predicate.evaluate(&input(&context, &fresh, now)),
            TruthValue::True
        );

        let missing = ObservationSnapshot::new(now, Vec::new());
        assert_eq!(
            predicate.evaluate(&input(&context, &missing, now)),
            TruthValue::Unknown
        );

        let stale_time = now.checked_sub(Duration::from_secs(10)).unwrap();
        let stale = ObservationSnapshot::new(
            stale_time,
            vec![Observation::from_source(
                ObservationSource::runtime("test"),
                signal.clone(),
                0.9,
                stale_time,
            )],
        );
        assert_eq!(
            predicate.evaluate(&input(&context, &stale, now)),
            TruthValue::Unknown
        );

        let nonfinite_context = PlanningContext::new().observe(signal.clone(), f64::NAN);
        assert_eq!(
            predicate.evaluate(&input(&nonfinite_context, &fresh, now)),
            TruthValue::Unknown
        );
    }

    #[test]
    fn presence_and_capability_predicates_do_not_invent_true() {
        let now = Instant::now();
        let signal = ObservationSignalId::FREE_CAPACITY;
        let unsupported = ObservationSnapshot::new(
            now,
            vec![Observation::unsupported_from_source(
                ObservationSource::runtime("test"),
                signal.clone(),
                now,
                "not exposed",
            )],
        );
        let context = PlanningContext::new();
        let presence = ObservationPresencePredicate::new(key("capacity-present"), signal);
        assert_eq!(
            presence.evaluate(&input(&context, &unsupported, now)),
            TruthValue::False
        );

        let unknown_capability = CapabilityPredicate::new(key("capability"), None);
        assert_eq!(
            unknown_capability.evaluate(&input(&context, &unsupported, now)),
            TruthValue::Unknown
        );
        let present_capability = CapabilityPredicate::new(key("capability-present"), Some(true));
        assert_eq!(
            present_capability.evaluate(&input(&context, &unsupported, now)),
            TruthValue::True
        );
    }

    #[test]
    fn snapshots_are_order_independent_bounded_and_guard_compatible() {
        let now = Instant::now();
        let observations = ObservationSnapshot::new(now, Vec::new());
        let context = PlanningContext::new();
        let input = input(&context, &observations, now);
        let a = CapabilityPredicate::new(key("a"), Some(true));
        let b = CapabilityPredicate::new(key("b"), Some(false));
        let source = FactSourceId::new("runtime:test").unwrap();
        let first = FactSnapshot::derive(
            source.clone(),
            ObservationEpoch::new(7),
            None,
            &input,
            &[&b, &a],
        )
        .unwrap();
        let second =
            FactSnapshot::derive(source, ObservationEpoch::new(7), None, &input, &[&a, &b])
                .unwrap();
        assert_eq!(first, second);
        assert_eq!(first.truth(&key("a")), TruthValue::True);
        assert_eq!(first.truth(&key("b")), TruthValue::False);
        assert_eq!(first.truth(&key("missing")), TruthValue::Unknown);
    }

    #[test]
    fn duplicate_predicates_fail_closed() {
        let now = Instant::now();
        let observations = ObservationSnapshot::new(now, Vec::new());
        let context = PlanningContext::new();
        let input = input(&context, &observations, now);
        let a = CapabilityPredicate::new(key("same"), Some(true));
        let b = CapabilityPredicate::new(key("same"), Some(false));
        assert_eq!(
            FactSnapshot::derive(
                FactSourceId::new("runtime:test").unwrap(),
                ObservationEpoch::new(1),
                None,
                &input,
                &[&a, &b],
            ),
            Err(FactDerivationError::DuplicatePredicate { key: key("same") })
        );
    }

    #[test]
    fn resource_and_observation_epoch_binding_reject_stale_facts() {
        let now = Instant::now();
        let observations = ObservationSnapshot::new(now, Vec::new());
        let context = PlanningContext::new();
        let input = input(&context, &observations, now);
        let resource = LogicalResourceId::new("ram").unwrap();
        let predicate = CapabilityPredicate::new(key("ram-ready"), Some(true));
        let snapshot = FactSnapshot::derive(
            FactSourceId::new("runtime:test").unwrap(),
            ObservationEpoch::new(3),
            Some(FactResourceBinding::new(
                resource.clone(),
                ResourceGeneration::new(8),
            )),
            &input,
            &[&predicate],
        )
        .unwrap();

        let current = FreshnessSnapshot::new(PlannerEpoch::new(1), ObservationEpoch::new(3))
            .with_resource_generation(resource.clone(), ResourceGeneration::new(8));
        assert_eq!(snapshot.validate_freshness(&current), Ok(()));

        let stale_epoch = FreshnessSnapshot::new(PlannerEpoch::new(1), ObservationEpoch::new(4))
            .with_resource_generation(resource.clone(), ResourceGeneration::new(8));
        assert!(matches!(
            snapshot.validate_freshness(&stale_epoch),
            Err(FactFreshnessError::ObservationEpochMismatch { .. })
        ));

        let stale_resource = FreshnessSnapshot::new(PlannerEpoch::new(1), ObservationEpoch::new(3))
            .with_resource_generation(resource, ResourceGeneration::new(9));
        assert!(matches!(
            snapshot.validate_freshness(&stale_resource),
            Err(FactFreshnessError::ResourceGenerationMismatch { .. })
        ));
    }

    #[test]
    fn nonfinite_threshold_is_rejected() {
        assert_eq!(
            ObservationThresholdPredicate::new(
                key("bad-threshold"),
                ObservationSignalId::UTILIZATION,
                ThresholdComparison::GreaterThan,
                f64::NAN,
                Duration::from_secs(1),
            )
            .unwrap_err(),
            FactDerivationError::NonFiniteThreshold
        );
    }
}
