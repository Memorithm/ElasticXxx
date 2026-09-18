//! BE14d fail-closed Boolean capacity preflight for KV representation transitions.
//!
//! This module answers one narrow question before a KV representation candidate
//! is structurally validated: does fresh explicit `free-capacity` evidence
//! establish enough bytes for the caller-declared target materialization?
//! `False` and `Unknown` stop before KV transition validation. `True` only
//! permits the existing `KvPageDescriptor` validator to run. It never authorizes
//! physical movement, re-encoding, cache reuse, or publication.

use std::fmt;
use std::time::{Duration, Instant};

use elastic_core::resource::{DimensionId, LogicalResourceId, ObservationSignalId, ResourceSpec};
use elastic_core::{
    BoolExpr, BooleanGuard, CapabilitySet, FreshnessSnapshot, GuardFactSource, GuardScope,
    GuardedResourceSpec, ObservationEpoch, PlannerEpoch, PredicateKey, PredicateRegistry,
    RepresentationState, ResourceGeneration, TransitionAttestations, TransitionMechanism,
    TruthValue,
};
use elastic_eir::{lower_guarded, EirGuardedResource, PlanningContext, TransitionCandidate};
use elastic_runtime::{
    capture_decision_trace, BooleanGuardPreplanner, CurrentStateForecaster, FactResourceBinding,
    FactSnapshot, FactSourceId, Forecaster, Observation, ObservationSnapshot, ObservationSource,
    PredicateEvaluationInput, PredicateEvaluator,
};
use serde::{Deserialize, Serialize};

use crate::{KvPageDescriptor, KvTargetMaterialization, KvTransitionError, KvTransitionPlan};

pub const KV_CAPACITY_PREDICATE_NAMESPACE: &str = "elastic.kv";
pub const KV_CAPACITY_PREDICATE_NAME: &str = "target-materialization-fits";
pub const KV_CAPACITY_SOURCE_UNIT: &str = "bytes";
pub const KV_CAPACITY_DEFAULT_MAX_AGE: Duration = Duration::from_secs(1);
const MAX_EXACT_F64_INTEGER_U64: u64 = 1_u64 << 53;

pub fn kv_capacity_predicate_key() -> PredicateKey {
    PredicateKey::new(KV_CAPACITY_PREDICATE_NAMESPACE, KV_CAPACITY_PREDICATE_NAME)
        .expect("static BE14d PredicateKey is valid")
}

/// Versioned, source-bound input for the BE14d KV capacity guard.
///
/// Consumers should prefer this helper over assembling a [`PlanningContext`]
/// and [`ObservationSnapshot`] independently. It keeps the numeric context and
/// telemetry record bit-identical and binds `free-capacity` to the exact
/// [`LogicalResourceId`] that produced it. Values that cannot be represented
/// exactly by the current `f64` observation transport are emitted as explicit
/// unsupported evidence instead of being rounded.
#[derive(Clone, Debug, PartialEq)]
pub struct KvCapacityObservationV1 {
    planning_context: PlanningContext,
    observations: ObservationSnapshot,
}

impl KvCapacityObservationV1 {
    /// Build a valid byte-capacity reading when the integer can cross the
    /// current observation transport without loss. Larger values fail closed
    /// to unsupported/`Unknown` evidence.
    #[must_use]
    pub fn measured(
        resource: LogicalResourceId,
        free_capacity_bytes: u64,
        observed_at: Instant,
    ) -> Self {
        let source = ObservationSource::Resource(resource);
        if free_capacity_bytes > MAX_EXACT_F64_INTEGER_U64 {
            return Self {
                planning_context: PlanningContext::new(),
                observations: ObservationSnapshot::new(
                    observed_at,
                    vec![Observation::unsupported_from_source(
                        source,
                        ObservationSignalId::FREE_CAPACITY,
                        observed_at,
                        "free-capacity bytes exceed exact f64 integer range",
                    )],
                ),
            };
        }

        let value = free_capacity_bytes as f64;
        Self {
            planning_context: PlanningContext::new()
                .observe(ObservationSignalId::FREE_CAPACITY, value),
            observations: ObservationSnapshot::new(
                observed_at,
                vec![Observation::from_source(
                    source,
                    ObservationSignalId::FREE_CAPACITY,
                    value,
                    observed_at,
                )],
            ),
        }
    }

    /// Build explicit unavailable capacity evidence without fabricating a zero.
    #[must_use]
    pub fn unsupported(
        resource: LogicalResourceId,
        observed_at: Instant,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            planning_context: PlanningContext::new(),
            observations: ObservationSnapshot::new(
                observed_at,
                vec![Observation::unsupported_from_source(
                    ObservationSource::Resource(resource),
                    ObservationSignalId::FREE_CAPACITY,
                    observed_at,
                    reason,
                )],
            ),
        }
    }

    /// Numeric planning context paired with the source-bound observation.
    #[must_use]
    pub const fn planning_context(&self) -> &PlanningContext {
        &self.planning_context
    }

    /// Source-bound runtime observation snapshot.
    #[must_use]
    pub const fn observations(&self) -> &ObservationSnapshot {
        &self.observations
    }

    /// Split the provider value into the exact inputs accepted by the existing
    /// capacity preflight. This remains planning evidence only.
    #[must_use]
    pub fn into_parts(self) -> (PlanningContext, ObservationSnapshot) {
        (self.planning_context, self.observations)
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BooleanKvCapacityStatusV1 {
    Eligible,
    Rejected,
    InsufficientEvidence,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BooleanKvCapacityEvidenceV1 {
    pub schema_version: u16,
    pub predicate_key: String,
    pub source_signal: String,
    pub source_unit: String,
    pub target_materialized_bytes: u64,
    pub truth: String,
    pub decision_trace_json: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BooleanKvCapacityReportV1 {
    pub schema_version: u16,
    pub status: BooleanKvCapacityStatusV1,
    pub reason: String,
    pub evidence: BooleanKvCapacityEvidenceV1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BooleanKvTransitionPreflightV1 {
    Candidate {
        report: BooleanKvCapacityReportV1,
        plan: KvTransitionPlan,
    },
    Blocked(BooleanKvCapacityReportV1),
}

/// Version-2 durable BE14d evidence with an explicit forecast boundary.
///
/// V1 remains frozen for backward compatibility. The additional fields are
/// explanatory only and do not grant planning, validation, or actuation authority.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BooleanKvCapacityEvidenceV2 {
    pub schema_version: u16,
    pub predicate_key: String,
    pub source_signal: String,
    pub source_unit: String,
    pub target_materialized_bytes: u64,
    pub forecast_method: String,
    pub forecast_horizon_milliseconds: u64,
    pub forecast_confidence_claimed: bool,
    pub truth: String,
    pub decision_trace_json: String,
}

/// Version-2 BE14d report retaining zero-horizon forecast metadata.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BooleanKvCapacityReportV2 {
    pub schema_version: u16,
    pub status: BooleanKvCapacityStatusV1,
    pub reason: String,
    pub evidence: BooleanKvCapacityEvidenceV2,
}

/// Version-2 preflight result. The transition plan is unchanged from V1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BooleanKvTransitionPreflightV2 {
    Candidate {
        report: BooleanKvCapacityReportV2,
        plan: KvTransitionPlan,
    },
    Blocked(BooleanKvCapacityReportV2),
}

impl From<BooleanKvCapacityEvidenceV2> for BooleanKvCapacityEvidenceV1 {
    fn from(value: BooleanKvCapacityEvidenceV2) -> Self {
        Self {
            schema_version: 1,
            predicate_key: value.predicate_key,
            source_signal: value.source_signal,
            source_unit: value.source_unit,
            target_materialized_bytes: value.target_materialized_bytes,
            truth: value.truth,
            decision_trace_json: value.decision_trace_json,
        }
    }
}

impl From<BooleanKvCapacityReportV2> for BooleanKvCapacityReportV1 {
    fn from(value: BooleanKvCapacityReportV2) -> Self {
        Self {
            schema_version: 1,
            status: value.status,
            reason: value.reason,
            evidence: value.evidence.into(),
        }
    }
}

impl From<BooleanKvTransitionPreflightV2> for BooleanKvTransitionPreflightV1 {
    fn from(value: BooleanKvTransitionPreflightV2) -> Self {
        match value {
            BooleanKvTransitionPreflightV2::Candidate { report, plan } => Self::Candidate {
                report: report.into(),
                plan,
            },
            BooleanKvTransitionPreflightV2::Blocked(report) => Self::Blocked(report.into()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BooleanKvCapacityError {
    Contract(String),
    Transition(KvTransitionError),
}

impl fmt::Display for BooleanKvCapacityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Contract(message) => f.write_str(message),
            Self::Transition(error) => write!(f, "KV transition validation failed: {error}"),
        }
    }
}

impl std::error::Error for BooleanKvCapacityError {}

impl From<KvTransitionError> for BooleanKvCapacityError {
    fn from(value: KvTransitionError) -> Self {
        Self::Transition(value)
    }
}

struct TargetMaterializationFitsPredicate {
    key: PredicateKey,
    target_materialized_bytes: u64,
    max_age: Duration,
    expected_source: ObservationSource,
}

impl PredicateEvaluator for TargetMaterializationFitsPredicate {
    fn key(&self) -> &PredicateKey {
        &self.key
    }

    fn evaluate(&self, input: &PredicateEvaluationInput<'_>) -> TruthValue {
        if self.target_materialized_bytes > MAX_EXACT_F64_INTEGER_U64 {
            return TruthValue::Unknown;
        }
        let Some(observation) = input.observations().get(ObservationSignalId::FREE_CAPACITY) else {
            return TruthValue::Unknown;
        };
        if observation.source() != &self.expected_source
            || !observation.is_valid()
            || !observation.value().is_finite()
        {
            return TruthValue::Unknown;
        }
        let Some(age) = input.now().checked_duration_since(*observation.timestamp()) else {
            return TruthValue::Unknown;
        };
        if age > self.max_age {
            return TruthValue::Unknown;
        }
        let Some(context_value) = input
            .planning_context()
            .get(ObservationSignalId::FREE_CAPACITY)
        else {
            return TruthValue::Unknown;
        };
        if !context_value.is_finite()
            || context_value.to_bits() != observation.value().to_bits()
            || !(0.0..=MAX_EXACT_F64_INTEGER_U64 as f64).contains(&context_value)
            || context_value.fract() != 0.0
        {
            return TruthValue::Unknown;
        }
        if context_value >= self.target_materialized_bytes as f64 {
            TruthValue::True
        } else {
            TruthValue::False
        }
    }
}

/// Planning-only BE14d bridge for one declared KV representation transition.
///
/// This type owns no cache, allocator, codec, migration engine, or actuator.
/// A `True` preflight only permits the existing KV structural validator to run.
pub struct BooleanKvCapacityPreflightControllerV1 {
    guarded_resource: EirGuardedResource,
    mechanism: TransitionMechanism,
    max_age: Duration,
    next_observation_epoch: u64,
    resource_generation: u64,
}

impl BooleanKvCapacityPreflightControllerV1 {
    pub fn new(
        spec: ResourceSpec,
        mechanism: TransitionMechanism,
        max_age: Duration,
    ) -> Result<Self, BooleanKvCapacityError> {
        if max_age.is_zero() {
            return Err(BooleanKvCapacityError::Contract(
                "BE14d capacity freshness bound must be non-zero".into(),
            ));
        }
        if !spec.admits(mechanism, &DimensionId::REPRESENTATION) {
            return Err(BooleanKvCapacityError::Contract(
                "BE14d resource must declare the selected representation transition".into(),
            ));
        }
        if !spec
            .observed_signals()
            .contains(&ObservationSignalId::FREE_CAPACITY)
        {
            return Err(BooleanKvCapacityError::Contract(
                "BE14d resource must declare free-capacity observation".into(),
            ));
        }

        let predicate = kv_capacity_predicate_key();
        let registry = PredicateRegistry::from_keys([predicate.clone()])
            .map_err(|error| BooleanKvCapacityError::Contract(error.to_string()))?;
        let predicate_id = registry
            .id(&predicate)
            .expect("registry contains the BE14d predicate");
        let guard = BooleanGuard::new(
            GuardScope::Transition {
                mechanism,
                dimension: DimensionId::REPRESENTATION,
            },
            registry,
            BoolExpr::atom(predicate_id),
        )
        .map_err(|error| BooleanKvCapacityError::Contract(error.to_string()))?;
        let guarded_resource = lower_guarded(
            &GuardedResourceSpec::new(spec, vec![guard])
                .map_err(|error| BooleanKvCapacityError::Contract(error.to_string()))?,
        )
        .map_err(|error| BooleanKvCapacityError::Contract(error.to_string()))?;

        Ok(Self {
            guarded_resource,
            mechanism,
            max_age,
            next_observation_epoch: 1,
            resource_generation: 1,
        })
    }

    /// Evaluate one source-bound provider value without allowing the numeric
    /// planning context to drift from its observation provenance.
    ///
    /// V1 remains the frozen historical wire shape and is projected from the
    /// same V2 execution path that records the explicit forecast boundary.
    pub fn evaluate_observation(
        &mut self,
        observation: &KvCapacityObservationV1,
        target_materialized_bytes: u64,
        now: Instant,
    ) -> Result<BooleanKvCapacityReportV1, BooleanKvCapacityError> {
        self.evaluate_observation_v2(observation, target_materialized_bytes, now)
            .map(Into::into)
    }

    /// Evaluate one source-bound provider value and retain forecast metadata.
    pub fn evaluate_observation_v2(
        &mut self,
        observation: &KvCapacityObservationV1,
        target_materialized_bytes: u64,
        now: Instant,
    ) -> Result<BooleanKvCapacityReportV2, BooleanKvCapacityError> {
        self.evaluate_v2(
            observation.planning_context(),
            observation.observations(),
            target_materialized_bytes,
            now,
        )
    }

    /// Historical V1 evaluation surface projected from [`Self::evaluate_v2`].
    pub fn evaluate(
        &mut self,
        planning_context: &PlanningContext,
        observations: &ObservationSnapshot,
        target_materialized_bytes: u64,
        now: Instant,
    ) -> Result<BooleanKvCapacityReportV1, BooleanKvCapacityError> {
        self.evaluate_v2(
            planning_context,
            observations,
            target_materialized_bytes,
            now,
        )
        .map(Into::into)
    }

    /// Execute `OBSERVE -> FORECAST -> Boolean PLAN` for the KV capacity gate.
    ///
    /// The current-state forecaster is a zero-horizon compatibility boundary:
    /// it copies the current planning context and claims no calibrated
    /// confidence. `False` and `Unknown` remain fail-closed; `True` still grants
    /// only permission to continue to structural KV validation.
    pub fn evaluate_v2(
        &mut self,
        planning_context: &PlanningContext,
        observations: &ObservationSnapshot,
        target_materialized_bytes: u64,
        now: Instant,
    ) -> Result<BooleanKvCapacityReportV2, BooleanKvCapacityError> {
        let forecast = CurrentStateForecaster
            .forecast(observations, planning_context)
            .map_err(|error| BooleanKvCapacityError::Contract(error.to_string()))?;
        let forecast_context = forecast.planning_context().ok_or_else(|| {
            BooleanKvCapacityError::Contract(
                "BE14d current-state forecast produced no planning context".to_owned(),
            )
        })?;

        let epoch = self.next_epoch()?;
        let generation = ResourceGeneration::new(self.resource_generation);
        let key = kv_capacity_predicate_key();
        let evaluator = TargetMaterializationFitsPredicate {
            key: key.clone(),
            target_materialized_bytes,
            max_age: self.max_age,
            expected_source: ObservationSource::Resource(
                self.guarded_resource.resource().identity().clone(),
            ),
        };
        let input = PredicateEvaluationInput::new(forecast_context, observations, now);
        let facts = FactSnapshot::derive(
            FactSourceId::new("elastic-kv:be14d-capacity")
                .map_err(|error| BooleanKvCapacityError::Contract(error.to_string()))?,
            epoch,
            Some(FactResourceBinding::new(
                self.guarded_resource.resource().identity().clone(),
                generation,
            )),
            &input,
            &[&evaluator as &dyn PredicateEvaluator],
        )
        .map_err(|error| BooleanKvCapacityError::Contract(error.to_string()))?;
        let freshness = FreshnessSnapshot::new(PlannerEpoch::new(epoch.get()), epoch)
            .with_resource_generation(
                self.guarded_resource.resource().identity().clone(),
                generation,
            );
        let truth = facts.truth(&key);
        let pruning = BooleanGuardPreplanner
            .prune(&self.guarded_resource, &facts, &freshness)
            .map_err(|error| BooleanKvCapacityError::Contract(error.to_string()))?;

        let eligible = pruning.contains_eligible(self.mechanism, &DimensionId::REPRESENTATION);
        if truth == TruthValue::True && !eligible {
            return Err(BooleanKvCapacityError::Contract(
                "BE14d guard was true but the representation transition was not eligible".into(),
            ));
        }
        let selected = if truth == TruthValue::True {
            self.guarded_resource
                .resource()
                .transitions()
                .iter()
                .find(|entry| {
                    entry.transition().mechanism() == self.mechanism
                        && entry.transition().dimension() == &DimensionId::REPRESENTATION
                })
                .map(TransitionCandidate::from_admitted)
        } else {
            None
        };
        let trace = capture_decision_trace(
            &self.guarded_resource,
            &facts,
            &freshness,
            selected.as_ref(),
        )
        .map_err(|error| BooleanKvCapacityError::Contract(error.to_string()))?;
        let decision_trace_json = trace
            .to_bounded_json()
            .map_err(|error| BooleanKvCapacityError::Contract(error.to_string()))?;

        let (status, reason) = match truth {
            TruthValue::True => (BooleanKvCapacityStatusV1::Eligible, "capacity-established"),
            TruthValue::False => (BooleanKvCapacityStatusV1::Rejected, "insufficient-capacity"),
            TruthValue::Unknown => (
                BooleanKvCapacityStatusV1::InsufficientEvidence,
                "capacity-evidence-unknown",
            ),
        };
        Ok(BooleanKvCapacityReportV2 {
            schema_version: 2,
            status,
            reason: reason.to_owned(),
            evidence: BooleanKvCapacityEvidenceV2 {
                schema_version: 2,
                predicate_key: key.to_string(),
                source_signal: ObservationSignalId::FREE_CAPACITY.as_str().to_owned(),
                source_unit: KV_CAPACITY_SOURCE_UNIT.to_owned(),
                target_materialized_bytes,
                forecast_method: forecast.method.clone(),
                forecast_horizon_milliseconds: u64::try_from(forecast.horizon.as_millis())
                    .map_err(|_| {
                        BooleanKvCapacityError::Contract(
                            "BE14d forecast horizon exceeds u64 milliseconds".to_owned(),
                        )
                    })?,
                forecast_confidence_claimed: forecast.confidence.is_some(),
                truth: truth_text(truth).to_owned(),
                decision_trace_json,
            },
        })
    }

    /// Historical V1 transition preflight projected from [`Self::validate_candidate_v2`].
    #[allow(clippy::too_many_arguments)]
    pub fn validate_candidate(
        &mut self,
        descriptor: &KvPageDescriptor,
        target: RepresentationState,
        capabilities: &CapabilitySet,
        attestations: TransitionAttestations,
        target_materialization: KvTargetMaterialization,
        planning_context: &PlanningContext,
        observations: &ObservationSnapshot,
        target_materialized_bytes: u64,
        now: Instant,
    ) -> Result<BooleanKvTransitionPreflightV1, BooleanKvCapacityError> {
        self.validate_candidate_v2(
            descriptor,
            target,
            capabilities,
            attestations,
            target_materialization,
            planning_context,
            observations,
            target_materialized_bytes,
            now,
        )
        .map(Into::into)
    }

    /// Forecast-aware V2 transition preflight.
    #[allow(clippy::too_many_arguments)]
    pub fn validate_candidate_v2(
        &mut self,
        descriptor: &KvPageDescriptor,
        target: RepresentationState,
        capabilities: &CapabilitySet,
        attestations: TransitionAttestations,
        target_materialization: KvTargetMaterialization,
        planning_context: &PlanningContext,
        observations: &ObservationSnapshot,
        target_materialized_bytes: u64,
        now: Instant,
    ) -> Result<BooleanKvTransitionPreflightV2, BooleanKvCapacityError> {
        let report = self.evaluate_v2(
            planning_context,
            observations,
            target_materialized_bytes,
            now,
        )?;
        if report.status != BooleanKvCapacityStatusV1::Eligible {
            return Ok(BooleanKvTransitionPreflightV2::Blocked(report));
        }
        let plan = descriptor.validate_reusable_representation_change(
            target,
            self.mechanism,
            capabilities,
            attestations,
            target_materialization,
        )?;
        Ok(BooleanKvTransitionPreflightV2::Candidate { report, plan })
    }

    fn next_epoch(&mut self) -> Result<ObservationEpoch, BooleanKvCapacityError> {
        let epoch = ObservationEpoch::new(self.next_observation_epoch);
        self.next_observation_epoch =
            self.next_observation_epoch.checked_add(1).ok_or_else(|| {
                BooleanKvCapacityError::Contract("BE14d observation epoch exhausted".into())
            })?;
        Ok(epoch)
    }
}

fn truth_text(value: TruthValue) -> &'static str {
    match value {
        TruthValue::True => "true",
        TruthValue::False => "false",
        TruthValue::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        KeyEncodingPipeline, KeyTransformScope, KvPrecision, KvRecoverySource, KvResidency,
    };
    use elastic_core::resource::{AdmissibleTransition, CapabilityRequirement, ResourceClassId};
    use elastic_core::{
        EvidenceKind, EvidenceToken, IssuerId, RepresentationEpoch, RepresentationId,
    };

    fn spec(id: &str) -> ResourceSpec {
        ResourceSpec::builder(
            ResourceClassId::REPRESENTATIONAL,
            LogicalResourceId::new(id).unwrap(),
        )
        .allow(DimensionId::REPRESENTATION)
        .admit(AdmissibleTransition::new(
            TransitionMechanism::Reencode,
            DimensionId::REPRESENTATION,
        ))
        .require_capability(CapabilityRequirement::new(
            TransitionMechanism::Reencode,
            DimensionId::REPRESENTATION,
        ))
        .observe(ObservationSignalId::FREE_CAPACITY)
        .build()
        .unwrap()
    }

    fn state(name: &str, epoch: u64) -> RepresentationState {
        RepresentationState::new(
            RepresentationId::new(name).unwrap(),
            1,
            RepresentationEpoch::new(epoch),
        )
    }

    fn descriptor() -> KvPageDescriptor {
        KvPageDescriptor {
            page: crate::KvPageId::new(7),
            representation: state("kv.raw", 1),
            precision: KvPrecision::F16,
            residency: KvResidency::Accelerator,
            key_transform_scope: KeyTransformScope::TokenStable,
            key_encoding_pipeline: KeyEncodingPipeline::TransformThenCodec,
            recovery_source: KvRecoverySource::StoredCanonicalRaw,
        }
    }

    fn target_and_evidence(
        page: &KvPageDescriptor,
    ) -> (RepresentationState, CapabilitySet, EvidenceToken) {
        let target = state("kv.int8", 2);
        let mut capabilities = CapabilitySet::new();
        capabilities.insert(target.id.clone(), target.schema_version);
        let transition = elastic_core::RepresentationTransition {
            from: page.representation.clone(),
            to: target.clone(),
            mechanism: TransitionMechanism::Reencode,
        };
        let token = EvidenceToken::issue(
            IssuerId::new("be14d-test-validator").unwrap(),
            EvidenceKind::ReencoderAvailable,
            &transition,
        );
        (target, capabilities, token)
    }

    fn evidence(
        now: Instant,
        resource_id: &str,
        bytes: Option<f64>,
    ) -> (PlanningContext, ObservationSnapshot) {
        match bytes {
            Some(bytes) => {
                let context =
                    PlanningContext::new().observe(ObservationSignalId::FREE_CAPACITY, bytes);
                let observations = ObservationSnapshot::new(
                    now,
                    vec![Observation::from_source(
                        ObservationSource::Resource(LogicalResourceId::new(resource_id).unwrap()),
                        ObservationSignalId::FREE_CAPACITY,
                        bytes,
                        now,
                    )],
                );
                (context, observations)
            }
            None => (
                PlanningContext::new(),
                ObservationSnapshot::new(now, Vec::new()),
            ),
        }
    }

    fn materialization() -> KvTargetMaterialization {
        KvTargetMaterialization::new(
            KeyTransformScope::TokenStable,
            KeyEncodingPipeline::TransformThenCodec,
            KvRecoverySource::StoredCanonicalRaw,
        )
    }

    #[test]
    fn source_bound_capacity_provider_keeps_context_and_observation_identical() {
        let now = Instant::now();
        let resource = LogicalResourceId::new("kv-provider").unwrap();
        let input = KvCapacityObservationV1::measured(resource.clone(), 4096, now);

        assert_eq!(
            input
                .planning_context()
                .get(ObservationSignalId::FREE_CAPACITY),
            Some(4096.0)
        );
        let observation = input
            .observations()
            .get(ObservationSignalId::FREE_CAPACITY)
            .unwrap();
        assert_eq!(observation.source(), &ObservationSource::Resource(resource));
        assert_eq!(observation.value(), 4096.0);
        assert!(observation.is_valid());
    }

    #[test]
    fn source_bound_capacity_provider_drives_the_existing_preflight() {
        let now = Instant::now();
        let resource_id = "kv-provider-preflight";
        let input = KvCapacityObservationV1::measured(
            LogicalResourceId::new(resource_id).unwrap(),
            4096,
            now,
        );
        let mut controller = BooleanKvCapacityPreflightControllerV1::new(
            spec(resource_id),
            TransitionMechanism::Reencode,
            KV_CAPACITY_DEFAULT_MAX_AGE,
        )
        .unwrap();

        let report = controller.evaluate_observation(&input, 2048, now).unwrap();

        assert_eq!(report.status, BooleanKvCapacityStatusV1::Eligible);
        assert_eq!(report.evidence.truth, "true");
    }

    #[test]
    fn v2_records_explicit_zero_horizon_forecast_without_confidence_claim() {
        let now = Instant::now();
        let resource_id = "kv-provider-forecast";
        let input = KvCapacityObservationV1::measured(
            LogicalResourceId::new(resource_id).unwrap(),
            4096,
            now,
        );
        let mut controller = BooleanKvCapacityPreflightControllerV1::new(
            spec(resource_id),
            TransitionMechanism::Reencode,
            KV_CAPACITY_DEFAULT_MAX_AGE,
        )
        .unwrap();

        let report = controller
            .evaluate_observation_v2(&input, 2048, now)
            .unwrap();

        assert_eq!(report.schema_version, 2);
        assert_eq!(report.status, BooleanKvCapacityStatusV1::Eligible);
        assert_eq!(report.evidence.schema_version, 2);
        assert_eq!(report.evidence.forecast_method, "current-state");
        assert_eq!(report.evidence.forecast_horizon_milliseconds, 0);
        assert!(!report.evidence.forecast_confidence_claimed);
        assert_eq!(report.evidence.truth, "true");
        assert!(!report.evidence.decision_trace_json.is_empty());
    }

    #[test]
    fn v1_wire_shape_stays_frozen_while_v2_adds_forecast_metadata() {
        let now = Instant::now();
        let resource_id = "kv-provider-wire";
        let input = KvCapacityObservationV1::measured(
            LogicalResourceId::new(resource_id).unwrap(),
            4096,
            now,
        );
        let mut v1_controller = BooleanKvCapacityPreflightControllerV1::new(
            spec(resource_id),
            TransitionMechanism::Reencode,
            KV_CAPACITY_DEFAULT_MAX_AGE,
        )
        .unwrap();
        let mut v2_controller = BooleanKvCapacityPreflightControllerV1::new(
            spec(resource_id),
            TransitionMechanism::Reencode,
            KV_CAPACITY_DEFAULT_MAX_AGE,
        )
        .unwrap();

        let v1 = v1_controller
            .evaluate_observation(&input, 2048, now)
            .unwrap();
        let v2 = v2_controller
            .evaluate_observation_v2(&input, 2048, now)
            .unwrap();
        let v1_json = serde_json::to_value(&v1).unwrap();
        let v2_json = serde_json::to_value(&v2).unwrap();
        let v1_evidence = v1_json["evidence"].as_object().unwrap();
        assert_eq!(v1_json["schema_version"], 1);
        assert_eq!(v1_json["evidence"]["schema_version"], 1);
        assert!(!v1_evidence.contains_key("forecast_method"));
        assert!(!v1_evidence.contains_key("forecast_horizon_milliseconds"));
        assert!(!v1_evidence.contains_key("forecast_confidence_claimed"));
        assert_eq!(v2_json["schema_version"], 2);
        assert_eq!(v2_json["evidence"]["forecast_method"], "current-state");
        assert_eq!(v2_json["evidence"]["forecast_horizon_milliseconds"], 0);
        assert_eq!(v2_json["evidence"]["forecast_confidence_claimed"], false);

        let decoded: BooleanKvCapacityReportV1 = serde_json::from_value(v1_json.clone()).unwrap();
        assert_eq!(decoded, v1);
        assert!(serde_json::from_value::<BooleanKvCapacityReportV1>(v2_json).is_err());
    }

    #[test]
    fn v2_preserves_true_false_unknown_capacity_semantics() {
        let now = Instant::now();
        for (suffix, bytes, expected) in [
            ("true", Some(4096.0), "true"),
            ("false", Some(1024.0), "false"),
            ("unknown", None, "unknown"),
        ] {
            let resource_id = format!("kv-v2-{suffix}");
            let (context, observations) = evidence(now, &resource_id, bytes);
            let mut controller = BooleanKvCapacityPreflightControllerV1::new(
                spec(&resource_id),
                TransitionMechanism::Reencode,
                KV_CAPACITY_DEFAULT_MAX_AGE,
            )
            .unwrap();
            let v2 = controller
                .evaluate_v2(&context, &observations, 2048, now)
                .unwrap();
            assert_eq!(v2.evidence.truth, expected);
            assert_eq!(v2.evidence.forecast_method, "current-state");
            assert_eq!(v2.evidence.forecast_horizon_milliseconds, 0);
            assert!(!v2.evidence.forecast_confidence_claimed);
            let v1: BooleanKvCapacityReportV1 = v2.into();
            assert_eq!(v1.evidence.truth, expected);
        }
    }

    #[test]
    fn source_bound_capacity_provider_fails_closed_when_bytes_are_inexact() {
        let now = Instant::now();
        let input = KvCapacityObservationV1::measured(
            LogicalResourceId::new("kv-provider-large").unwrap(),
            MAX_EXACT_F64_INTEGER_U64 + 1,
            now,
        );

        assert_eq!(
            input
                .planning_context()
                .get(ObservationSignalId::FREE_CAPACITY),
            None
        );
        let observation = input
            .observations()
            .get(ObservationSignalId::FREE_CAPACITY)
            .unwrap();
        assert!(observation.is_unsupported());
        assert_eq!(
            observation.unsupported_reason(),
            Some("free-capacity bytes exceed exact f64 integer range")
        );
    }

    #[test]
    fn explicit_unsupported_capacity_has_no_numeric_planning_value() {
        let now = Instant::now();
        let resource = LogicalResourceId::new("kv-provider-unavailable").unwrap();
        let input = KvCapacityObservationV1::unsupported(resource.clone(), now, "offline");

        assert_eq!(
            input
                .planning_context()
                .get(ObservationSignalId::FREE_CAPACITY),
            None
        );
        let observation = input
            .observations()
            .get(ObservationSignalId::FREE_CAPACITY)
            .unwrap();
        assert_eq!(observation.source(), &ObservationSource::Resource(resource));
        assert!(observation.is_unsupported());
        assert_eq!(observation.unsupported_reason(), Some("offline"));
    }

    #[test]
    fn true_capacity_delegates_to_existing_kv_validator_and_retains_trace() {
        let page = descriptor();
        let (target, capabilities, token) = target_and_evidence(&page);
        let transition = elastic_core::RepresentationTransition {
            from: page.representation.clone(),
            to: target.clone(),
            mechanism: TransitionMechanism::Reencode,
        };
        let attestations = TransitionAttestations::from_evidence([&token], &transition);
        let now = Instant::now();
        let (context, observations) = evidence(now, "kv-be14d", Some(4096.0));
        let mut controller = BooleanKvCapacityPreflightControllerV1::new(
            spec("kv-be14d"),
            TransitionMechanism::Reencode,
            KV_CAPACITY_DEFAULT_MAX_AGE,
        )
        .unwrap();

        let result = controller
            .validate_candidate(
                &page,
                target.clone(),
                &capabilities,
                attestations,
                materialization(),
                &context,
                &observations,
                2048,
                now,
            )
            .unwrap();
        let BooleanKvTransitionPreflightV1::Candidate { report, plan } = result else {
            panic!("expected qualified candidate")
        };
        assert_eq!(report.status, BooleanKvCapacityStatusV1::Eligible);
        assert_eq!(report.evidence.truth, "true");
        assert_eq!(report.evidence.source_unit, "bytes");
        assert!(!report.evidence.decision_trace_json.is_empty());
        assert_eq!(plan.representation.to, target);
    }

    #[test]
    fn false_capacity_blocks_before_invalid_target_materialization_is_validated() {
        let page = descriptor();
        let (target, capabilities, token) = target_and_evidence(&page);
        let transition = elastic_core::RepresentationTransition {
            from: page.representation.clone(),
            to: target.clone(),
            mechanism: TransitionMechanism::Reencode,
        };
        let attestations = TransitionAttestations::from_evidence([&token], &transition);
        let now = Instant::now();
        let (context, observations) = evidence(now, "kv-be14d-false", Some(1024.0));
        let mut controller = BooleanKvCapacityPreflightControllerV1::new(
            spec("kv-be14d-false"),
            TransitionMechanism::Reencode,
            KV_CAPACITY_DEFAULT_MAX_AGE,
        )
        .unwrap();
        let invalid = KvTargetMaterialization::new(
            KeyTransformScope::QueryDependent,
            KeyEncodingPipeline::TransformThenCodec,
            KvRecoverySource::ModelRecompute,
        );

        let result = controller
            .validate_candidate(
                &page,
                target,
                &capabilities,
                attestations,
                invalid,
                &context,
                &observations,
                2048,
                now,
            )
            .unwrap();
        let BooleanKvTransitionPreflightV1::Blocked(report) = result else {
            panic!("insufficient capacity must block before KV validation")
        };
        assert_eq!(report.status, BooleanKvCapacityStatusV1::Rejected);
        assert_eq!(report.evidence.truth, "false");
    }

    #[test]
    fn missing_stale_mismatched_and_inexact_capacity_are_unknown() {
        let now = Instant::now();
        let mut controller = BooleanKvCapacityPreflightControllerV1::new(
            spec("kv-be14d-unknown"),
            TransitionMechanism::Reencode,
            KV_CAPACITY_DEFAULT_MAX_AGE,
        )
        .unwrap();

        let (missing_context, missing_observations) = evidence(now, "kv-be14d-unknown", None);
        let missing = controller
            .evaluate(&missing_context, &missing_observations, 1, now)
            .unwrap();
        assert_eq!(
            missing.status,
            BooleanKvCapacityStatusV1::InsufficientEvidence
        );

        let old = now.checked_sub(Duration::from_secs(2)).unwrap();
        let stale_context = PlanningContext::new().observe(ObservationSignalId::FREE_CAPACITY, 4.0);
        let stale_observations = ObservationSnapshot::new(
            old,
            vec![Observation::from_source(
                ObservationSource::Resource(LogicalResourceId::new("kv-be14d-unknown").unwrap()),
                ObservationSignalId::FREE_CAPACITY,
                4.0,
                old,
            )],
        );
        let stale = controller
            .evaluate(&stale_context, &stale_observations, 1, now)
            .unwrap();
        assert_eq!(stale.evidence.truth, "unknown");

        let mismatched_context =
            PlanningContext::new().observe(ObservationSignalId::FREE_CAPACITY, 8.0);
        let mismatched_observations = ObservationSnapshot::new(
            now,
            vec![Observation::from_source(
                ObservationSource::Resource(LogicalResourceId::new("kv-be14d-unknown").unwrap()),
                ObservationSignalId::FREE_CAPACITY,
                4.0,
                now,
            )],
        );
        let mismatched = controller
            .evaluate(&mismatched_context, &mismatched_observations, 1, now)
            .unwrap();
        assert_eq!(mismatched.evidence.truth, "unknown");

        let (context, observations) = evidence(now, "kv-be14d-unknown", Some((1_u64 << 53) as f64));
        let inexact = controller
            .evaluate(&context, &observations, (1_u64 << 53) + 1, now)
            .unwrap();
        assert_eq!(inexact.evidence.truth, "unknown");
    }

    #[test]
    fn capacity_from_a_different_resource_is_unknown_and_fail_closed() {
        let now = Instant::now();
        let resource_id = "kv-be14d-bound";
        let mut controller = BooleanKvCapacityPreflightControllerV1::new(
            spec(resource_id),
            TransitionMechanism::Reencode,
            KV_CAPACITY_DEFAULT_MAX_AGE,
        )
        .unwrap();
        let context = PlanningContext::new().observe(ObservationSignalId::FREE_CAPACITY, 4096.0);
        let observations = ObservationSnapshot::new(
            now,
            vec![Observation::from_source(
                ObservationSource::Resource(LogicalResourceId::new("kv-be14d-other").unwrap()),
                ObservationSignalId::FREE_CAPACITY,
                4096.0,
                now,
            )],
        );

        let report = controller
            .evaluate(&context, &observations, 2048, now)
            .unwrap();

        assert_eq!(
            report.status,
            BooleanKvCapacityStatusV1::InsufficientEvidence
        );
        assert_eq!(report.evidence.truth, "unknown");
        assert_eq!(report.reason, "capacity-evidence-unknown");
    }

    #[test]
    fn true_capacity_does_not_override_kv_structural_validation() {
        let page = descriptor();
        let (target, capabilities, token) = target_and_evidence(&page);
        let transition = elastic_core::RepresentationTransition {
            from: page.representation.clone(),
            to: target.clone(),
            mechanism: TransitionMechanism::Reencode,
        };
        let attestations = TransitionAttestations::from_evidence([&token], &transition);
        let now = Instant::now();
        let (context, observations) = evidence(now, "kv-be14d-structural", Some(4096.0));
        let mut controller = BooleanKvCapacityPreflightControllerV1::new(
            spec("kv-be14d-structural"),
            TransitionMechanism::Reencode,
            KV_CAPACITY_DEFAULT_MAX_AGE,
        )
        .unwrap();
        let invalid = KvTargetMaterialization::new(
            KeyTransformScope::QueryDependent,
            KeyEncodingPipeline::TransformThenCodec,
            KvRecoverySource::ModelRecompute,
        );

        let error = controller
            .validate_candidate(
                &page,
                target,
                &capabilities,
                attestations,
                invalid,
                &context,
                &observations,
                2048,
                now,
            )
            .unwrap_err();
        assert_eq!(
            error,
            BooleanKvCapacityError::Transition(
                KvTransitionError::QueryDependentCacheRepresentation
            )
        );
    }
}
