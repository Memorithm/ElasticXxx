//! Facade-only BE14d consumer with a concrete host-memory KV page backend.
//!
//! The backend physically rewrites one bounded page between two explicit,
//! lossless byte encodings. It exists to prove that a downstream crate can bind
//! the public KV transaction SPI to real mutable storage while depending only on
//! `elastic`. It is not a production cache and makes no latency, bandwidth,
//! memory-saving, hardware, or model-quality claim.

use elastic::kv::boolean_admission::{
    BooleanKvCapacityPreflightControllerV1, BooleanKvCapacityReportV2,
    BooleanKvTransitionPreflightV2, KvCapacityObservationV1,
};
use elastic::kv::{
    CapabilitySet, KeyEncodingPipeline, KeyTransformScope, KvPageDescriptor, KvPageId, KvPrecision,
    KvRecoverySource, KvResidency, KvTargetMaterialization, KvTransitionBackendV1,
    KvTransitionPlan, RepresentationEpoch, RepresentationId, RepresentationState,
    TransactionalKvPageV1, TransitionAttestations,
};
use elastic::prelude::*;
use elastic::resource::RepresentationalDeclaration;
use elastic::{
    representation_precision_floor_signal, Actuation, BooleanRepresentationPrecisionPreplannerV1,
    BooleanRepresentationPrecisionReportV2, CommitRecord, InvariantCheck, Observation,
    ObservationEpoch, ObservationSnapshot, ObservationSource, Plan,
    RepresentationPrecisionCandidateV1, ResourceGeneration, RollbackRecord, ValidatedPlan,
    VerificationResult,
};
use std::time::{Duration, Instant};

const SOURCE_REPRESENTATION: &str = "downstream.host-u16-le";
const TARGET_REPRESENTATION: &str = "downstream.host-u16-be";
const VALUES: [u16; 4] = [0x0102, 0x2345, 0x6789, 0xabcd];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ByteOrder {
    Little,
    Big,
}

fn state(name: &str, epoch: u64) -> RepresentationState {
    RepresentationState::new(
        RepresentationId::new(name).expect("bounded static representation id"),
        1,
        RepresentationEpoch::new(epoch),
    )
}

fn order_for(descriptor: &KvPageDescriptor) -> Result<ByteOrder, String> {
    match descriptor.representation.id.as_str() {
        SOURCE_REPRESENTATION => Ok(ByteOrder::Little),
        TARGET_REPRESENTATION => Ok(ByteOrder::Big),
        other => Err(format!("unsupported host KV representation {other:?}")),
    }
}

fn encode(values: &[u16], order: ByteOrder) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| match order {
            ByteOrder::Little => value.to_le_bytes(),
            ByteOrder::Big => value.to_be_bytes(),
        })
        .collect()
}

fn decode(bytes: &[u8], order: ByteOrder) -> Result<Vec<u16>, String> {
    if bytes.len() % 2 != 0 {
        return Err("host KV payload has an odd byte length".into());
    }
    bytes
        .chunks_exact(2)
        .map(|chunk| {
            let pair = [chunk[0], chunk[1]];
            Ok(match order {
                ByteOrder::Little => u16::from_le_bytes(pair),
                ByteOrder::Big => u16::from_be_bytes(pair),
            })
        })
        .collect()
}

#[derive(Clone, Debug)]
struct HostKvPageBackend {
    descriptor: KvPageDescriptor,
    bytes: Vec<u8>,
    prepared: Option<Vec<u8>>,
    expected_values: Vec<u16>,
    fail_semantic_verification: bool,
}

impl HostKvPageBackend {
    fn new(descriptor: KvPageDescriptor) -> Self {
        let expected_values = VALUES.to_vec();
        let bytes = encode(
            &expected_values,
            order_for(&descriptor).expect("fixture representation is supported"),
        );
        Self {
            descriptor,
            bytes,
            prepared: None,
            expected_values,
            fail_semantic_verification: false,
        }
    }

    fn semantic_values(&self) -> Result<Vec<u16>, String> {
        decode(&self.bytes, order_for(&self.descriptor)?)
    }
}

impl KvTransitionBackendV1 for HostKvPageBackend {
    fn name(&self) -> &str {
        "downstream-host-kv-v1"
    }

    fn read_page(&self, page: KvPageId) -> Result<KvPageDescriptor, String> {
        if page != self.descriptor.page {
            return Err("unknown host KV page".into());
        }
        Ok(self.descriptor.clone())
    }

    fn validate_transition(
        &self,
        runtime_plan: &Plan,
        source: &KvPageDescriptor,
        target: &KvPageDescriptor,
        _transition: &KvTransitionPlan,
    ) -> Result<Vec<InvariantCheck>, String> {
        if source != &self.descriptor {
            return Err("host KV validation source drift".into());
        }
        let source_values = self.semantic_values()?;
        if source_values != self.expected_values {
            return Err("host KV source semantic contents are not authoritative".into());
        }
        let _ = order_for(target)?;
        Ok(runtime_plan
            .resource
            .invariants()
            .iter()
            .cloned()
            .map(|invariant| {
                InvariantCheck::new(
                    invariant,
                    true,
                    Some("host KV bytes decode to the bound semantic values".into()),
                )
            })
            .collect())
    }

    fn prepare_transition(
        &mut self,
        source: &KvPageDescriptor,
        target: &KvPageDescriptor,
        _transition: &KvTransitionPlan,
    ) -> Result<(), String> {
        if source != &self.descriptor {
            return Err("host KV prepare source drift".into());
        }
        let values = self.semantic_values()?;
        if values != self.expected_values {
            return Err("host KV prepare semantic mismatch".into());
        }
        self.prepared = Some(encode(&values, order_for(target)?));
        Ok(())
    }

    fn apply_transition_if_source(
        &mut self,
        source: &KvPageDescriptor,
        target: &KvPageDescriptor,
        _transition: &KvTransitionPlan,
    ) -> Result<(), String> {
        if source != &self.descriptor {
            return Err("atomic host KV source comparison rejected drift".into());
        }
        let bytes = self
            .prepared
            .take()
            .ok_or_else(|| "host KV target was not prepared".to_string())?;
        self.bytes = bytes;
        self.descriptor = target.clone();
        Ok(())
    }

    fn verify_transition(
        &self,
        target: &KvPageDescriptor,
        _transition: &KvTransitionPlan,
    ) -> Result<VerificationResult, String> {
        if &self.descriptor != target {
            return Ok(VerificationResult::Fail {
                detail: "host KV descriptor differs from target".into(),
            });
        }
        if self.fail_semantic_verification {
            return Ok(VerificationResult::Fail {
                detail: "injected downstream semantic verification failure".into(),
            });
        }
        if self.semantic_values()? != self.expected_values {
            return Ok(VerificationResult::Fail {
                detail: "host KV re-encode changed semantic values".into(),
            });
        }
        Ok(VerificationResult::Pass)
    }

    fn restore_page(&mut self, source: &KvPageDescriptor) -> Result<(), String> {
        self.bytes = encode(&self.expected_values, order_for(source)?);
        self.descriptor = source.clone();
        self.prepared = None;
        Ok(())
    }
}

fn source_page() -> KvPageDescriptor {
    KvPageDescriptor {
        page: KvPageId::new(71),
        representation: state(SOURCE_REPRESENTATION, 1),
        precision: KvPrecision::Custom("u16-exact-fixture".into()),
        residency: KvResidency::Host,
        key_transform_scope: KeyTransformScope::Raw,
        key_encoding_pipeline: KeyEncodingPipeline::Raw,
        recovery_source: KvRecoverySource::StoredCanonicalRaw,
    }
}

fn fixture() -> (
    ResourceSpec,
    EirResource,
    KvPageDescriptor,
    KvTransitionPlan,
    CapabilitySet,
    TransitionAttestations,
) {
    let resource_id = LogicalResourceId::new("downstream-host-kv").unwrap();
    let spec = ResourceSpec::builder(ResourceClassId::REPRESENTATIONAL, resource_id)
        .allow(DimensionId::REPRESENTATION)
        .preserve(Invariant::new(InvariantKind::PreserveContents))
        .admit(AdmissibleTransition::new(
            TransitionMechanism::Reencode,
            DimensionId::REPRESENTATION,
        ))
        .require_capability(CapabilityRequirement::new(
            TransitionMechanism::Reencode,
            DimensionId::REPRESENTATION,
        ))
        .observe(ObservationSignalId::FREE_CAPACITY)
        .observe(representation_precision_floor_signal())
        .build()
        .unwrap();
    let eir = lower(&spec).unwrap().resources()[0].clone();
    let source = source_page();
    let target = state(TARGET_REPRESENTATION, 2);
    let mut capabilities = CapabilitySet::new();
    capabilities.insert(target.id.clone(), target.schema_version);
    let attestations = TransitionAttestations::none().attest_reencoder_available();
    let transition = source
        .validate_reusable_representation_change(
            target,
            TransitionMechanism::Reencode,
            &capabilities,
            attestations,
            KvTargetMaterialization::new(
                KeyTransformScope::Raw,
                KeyEncodingPipeline::Raw,
                KvRecoverySource::StoredCanonicalRaw,
            ),
        )
        .unwrap();
    (spec, eir, source, transition, capabilities, attestations)
}

fn admit_with_boolean_representation_precision(
    spec: &ResourceSpec,
    source: &KvPageDescriptor,
    transition: &KvTransitionPlan,
    capabilities: &CapabilitySet,
    attestations: TransitionAttestations,
) -> BooleanRepresentationPrecisionReportV2 {
    let declaration = RepresentationalDeclaration::new(
        spec.clone(),
        [
            (
                source.representation.id.clone(),
                source.representation.schema_version,
            ),
            (
                transition.representation.to.id.clone(),
                transition.representation.to.schema_version,
            ),
        ],
    )
    .unwrap();
    let preplanner = BooleanRepresentationPrecisionPreplannerV1::new(
        declaration,
        vec![RepresentationPrecisionCandidateV1::new(
            "downstream-host-u16-be-fixed-width",
            0,
            transition.representation.to.id.clone(),
            transition.representation.to.schema_version,
            transition.representation.mechanism,
            16,
        )
        .unwrap()],
    )
    .unwrap();
    let now = Instant::now();
    let signal = representation_precision_floor_signal();
    let context = PlanningContext::new().observe(signal.clone(), 16.0);
    let observations = ObservationSnapshot::new(
        now,
        vec![Observation::from_source(
            ObservationSource::runtime("downstream-be14e-host-u16"),
            signal,
            16.0,
            now,
        )],
    );
    let report = preplanner
        .screen_with_trace(
            &source.representation,
            capabilities,
            &context,
            &observations,
            now,
            ObservationEpoch::new(73),
            ResourceGeneration::new(5),
        )
        .unwrap();
    let trace = DecisionTrace::from_bounded_json(
        report.candidate_traces[0]
            .decision_trace_json
            .as_deref()
            .expect("BE14e evaluated candidate must retain a durable trace")
            .as_bytes(),
    )
    .unwrap();
    assert!(trace.selected().is_some());
    assert_eq!(trace.observation_epoch(), ObservationEpoch::new(73));
    assert_eq!(trace.resource_generation(), ResourceGeneration::new(5));

    let selected = preplanner
        .selected_transition(&source.representation, &report.planning)
        .unwrap()
        .expect("BE14e fixed-width policy must select the declared host representation");
    assert_eq!(selected, transition.representation);
    selected
        .validate(capabilities, attestations)
        .expect("trusted representation validation remains authoritative after Boolean admission");
    report
}

fn runtime(spec: ResourceSpec, eir: EirResource) -> Runtime {
    Runtime::new(RuntimeConfig {
        resource_spec: spec,
        ir_resource: eir,
        mode: RuntimeMode::Apply,
        dry_run: false,
        max_cycles: 1,
        ..RuntimeConfig::default()
    })
}

fn admit_with_boolean_capacity(
    spec: &ResourceSpec,
    source: &KvPageDescriptor,
    transition: &KvTransitionPlan,
    capabilities: &CapabilitySet,
    attestations: TransitionAttestations,
) -> (BooleanKvCapacityReportV2, KvTransitionPlan) {
    let now = Instant::now();
    let observation = KvCapacityObservationV1::measured(spec.resource_id().clone(), 4096, now);
    let mut gate = BooleanKvCapacityPreflightControllerV1::new(
        spec.clone(),
        TransitionMechanism::Reencode,
        Duration::from_secs(1),
    )
    .unwrap();
    match gate
        .validate_candidate_v2(
            source,
            transition.representation.to.clone(),
            capabilities,
            attestations,
            KvTargetMaterialization::new(
                transition.target_key_transform_scope,
                transition.target_key_encoding_pipeline,
                transition.target_recovery_source,
            ),
            observation.planning_context(),
            observation.observations(),
            2048,
            now,
        )
        .unwrap()
    {
        BooleanKvTransitionPreflightV2::Candidate { report, plan } => {
            assert_eq!(report.evidence.truth, "true");
            assert_eq!(report.evidence.forecast_method, "current-state");
            assert_eq!(report.evidence.forecast_horizon_milliseconds, 0);
            assert!(!report.evidence.forecast_confidence_claimed);
            (report, plan)
        }
        BooleanKvTransitionPreflightV2::Blocked(report) => {
            panic!("sufficient measured capacity unexpectedly blocked: {report:?}")
        }
    }
}

#[test]
fn facade_only_consumer_physically_reencodes_and_commits_semantically_equal_kv_bytes() {
    let (spec, eir, source, transition, capabilities, attestations) = fixture();
    let (guard_report, transition) =
        admit_with_boolean_capacity(&spec, &source, &transition, &capabilities, attestations);
    let precision_report = admit_with_boolean_representation_precision(
        &spec,
        &source,
        &transition,
        &capabilities,
        attestations,
    );
    assert_eq!(precision_report.schema_version, 2);
    let trace =
        DecisionTrace::from_bounded_json(guard_report.evidence.decision_trace_json.as_bytes())
            .expect("BE14d durable guard trace must decode through the public facade");
    assert!(trace.selected().is_some());

    let source_bytes = encode(&VALUES, ByteOrder::Little);
    let target_bytes = encode(&VALUES, ByteOrder::Big);
    assert_ne!(source_bytes, target_bytes);

    let backend = HostKvPageBackend::new(source.clone());
    assert_eq!(backend.bytes, source_bytes);
    let mut actuator = TransactionalKvPageV1::new(
        backend,
        &eir,
        source,
        transition,
        &capabilities,
        attestations,
    )
    .unwrap();

    let result = runtime(spec, eir.clone())
        .cycle(&eir, &FirstGroundedPlanner, &(), &mut actuator)
        .unwrap();

    assert!(result.commit.is_some());
    assert!(result.rollback.is_none());
    assert_eq!(actuator.backend().bytes, target_bytes);
    assert_eq!(actuator.backend().semantic_values().unwrap(), VALUES);

    // Explicit differential baseline: the same declared and trusted physical
    // transition executes without the Boolean pruning layer. The guard is only
    // allowed to shrink eligibility; it must not alter the committed semantics.
    let (
        baseline_spec,
        baseline_eir,
        baseline_source,
        baseline_transition,
        baseline_capabilities,
        baseline_attestations,
    ) = fixture();
    let baseline_backend = HostKvPageBackend::new(baseline_source.clone());
    let mut baseline_actuator = TransactionalKvPageV1::new(
        baseline_backend,
        &baseline_eir,
        baseline_source,
        baseline_transition,
        &baseline_capabilities,
        baseline_attestations,
    )
    .unwrap();
    let baseline_result = runtime(baseline_spec, baseline_eir.clone())
        .cycle(
            &baseline_eir,
            &FirstGroundedPlanner,
            &(),
            &mut baseline_actuator,
        )
        .unwrap();

    assert!(baseline_result.commit.is_some());
    assert!(baseline_result.rollback.is_none());
    assert_eq!(baseline_actuator.backend().bytes, actuator.backend().bytes);
    assert_eq!(
        baseline_actuator.backend().semantic_values().unwrap(),
        actuator.backend().semantic_values().unwrap()
    );
}

#[test]
fn facade_only_consumer_rolls_back_exact_source_bytes_after_semantic_failure() {
    let (spec, eir, source, transition, capabilities, attestations) = fixture();
    let precision_report = admit_with_boolean_representation_precision(
        &spec,
        &source,
        &transition,
        &capabilities,
        attestations,
    );
    assert_eq!(precision_report.schema_version, 2);
    let source_bytes = encode(&VALUES, ByteOrder::Little);
    let mut backend = HostKvPageBackend::new(source.clone());
    backend.fail_semantic_verification = true;
    let mut actuator = TransactionalKvPageV1::new(
        backend,
        &eir,
        source.clone(),
        transition,
        &capabilities,
        attestations,
    )
    .unwrap();

    let result = runtime(spec, eir.clone())
        .cycle(&eir, &FirstGroundedPlanner, &(), &mut actuator)
        .unwrap();

    assert!(result.commit.is_none());
    assert!(matches!(
        result.verification,
        Some(VerificationResult::Fail { .. })
    ));
    assert!(result
        .rollback
        .as_ref()
        .is_some_and(|record| record.invariants_restored));
    assert_eq!(actuator.backend().descriptor, source);
    assert_eq!(actuator.backend().bytes, source_bytes);
    assert_eq!(actuator.backend().semantic_values().unwrap(), VALUES);
}

// Compile-time guard: the public transaction SPI uses these runtime records but
// the downstream crate still imports them through `elastic`, never through an
// implementation crate.
#[allow(dead_code)]
fn facade_type_surface(
    _actuation: Option<Actuation>,
    _validated: Option<ValidatedPlan>,
    _commit: Option<CommitRecord>,
    _rollback: Option<RollbackRecord>,
) {
}
