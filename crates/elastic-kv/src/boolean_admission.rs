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

use elastic_core::resource::{DimensionId, ObservationSignalId, ResourceSpec};
use elastic_core::{
    BoolExpr, BooleanGuard, CapabilitySet, FreshnessSnapshot, GuardFactSource, GuardScope,
    GuardedResourceSpec, ObservationEpoch, PlannerEpoch, PredicateKey, PredicateRegistry,
    RepresentationState, ResourceGeneration, TransitionAttestations, TransitionMechanism,
    TruthValue,
};
use elastic_eir::{lower_guarded, EirGuardedResource, TransitionCandidate};
use elastic_runtime::{
    capture_decision_trace, BooleanGuardPreplanner, FactResourceBinding, FactSnapshot,
    FactSourceId, ObservationSnapshot, ObservationSource, PredicateEvaluationInput,
    PredicateEvaluator,
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

    pub fn evaluate(
        &mut self,
        planning_context: &elastic_eir::PlanningContext,
        observations: &ObservationSnapshot,
        target_materialized_bytes: u64,
        now: Instant,
    ) -> Result<BooleanKvCapacityReportV1, BooleanKvCapacityError> {
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
        let input = PredicateEvaluationInput::new(planning_context, observations, now);
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
        Ok(BooleanKvCapacityReportV1 {
            schema_version: 1,
            status,
            reason: reason.to_owned(),
            evidence: BooleanKvCapacityEvidenceV1 {
                schema_version: 1,
                predicate_key: key.to_string(),
                source_signal: ObservationSignalId::FREE_CAPACITY.as_str().to_owned(),
                source_unit: KV_CAPACITY_SOURCE_UNIT.to_owned(),
                target_materialized_bytes,
                truth: truth_text(truth).to_owned(),
                decision_trace_json,
            },
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn validate_candidate(
        &mut self,
        descriptor: &KvPageDescriptor,
        target: RepresentationState,
        capabilities: &CapabilitySet,
        attestations: TransitionAttestations,
        target_materialization: KvTargetMaterialization,
        planning_context: &elastic_eir::PlanningContext,
        observations: &ObservationSnapshot,
        target_materialized_bytes: u64,
        now: Instant,
    ) -> Result<BooleanKvTransitionPreflightV1, BooleanKvCapacityError> {
        let report = self.evaluate(
            planning_context,
            observations,
            target_materialized_bytes,
            now,
        )?;
        if report.status != BooleanKvCapacityStatusV1::Eligible {
            return Ok(BooleanKvTransitionPreflightV1::Blocked(report));
        }
        let plan = descriptor.validate_reusable_representation_change(
            target,
            self.mechanism,
            capabilities,
            attestations,
            target_materialization,
        )?;
        Ok(BooleanKvTransitionPreflightV1::Candidate { report, plan })
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
    use elastic_core::resource::{
        AdmissibleTransition, CapabilityRequirement, LogicalResourceId, ResourceClassId,
    };
    use elastic_core::{
        EvidenceKind, EvidenceToken, IssuerId, RepresentationEpoch, RepresentationId,
    };
    use elastic_eir::PlanningContext;
    use elastic_runtime::Observation;

    use crate::{
        KeyEncodingPipeline, KeyTransformScope, KvPrecision, KvRecoverySource, KvResidency,
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
