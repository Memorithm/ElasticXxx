//! Public-facade ELANG7 representation/precision ↔ KV composition contract.

use std::time::Instant;

use elastic::kv::{
    KeyEncodingPipeline, KeyTransformScope, KvPageDescriptor, KvPageId, KvPrecision,
    KvRecoverySource, KvResidency, KvTargetMaterialization, RepresentationPrecisionKvBindingV1,
    TargetContract,
};
use elastic::resource::{
    AdmissibleTransition, CapabilityRequirement, DimensionId, LogicalResourceId,
    RepresentationalDeclaration, ResourceClassId, ResourceSpec,
};
use elastic::{
    representation_precision_floor_signal, BooleanRepresentationPrecisionPreplannerV1,
    CapabilitySet, Observation, ObservationEpoch, ObservationSnapshot, ObservationSource,
    PlanningContext, RepresentationEpoch, RepresentationId, RepresentationPrecisionCandidateV1,
    RepresentationState, ResourceGeneration, TransitionAttestations, TransitionMechanism,
};

#[test]
fn facade_only_selected_precision_candidate_binds_exact_kv_plan() {
    let spec = ResourceSpec::builder(
        ResourceClassId::REPRESENTATIONAL,
        LogicalResourceId::new("public-elang7-kv").unwrap(),
    )
    .allow(DimensionId::REPRESENTATION)
    .observe(representation_precision_floor_signal())
    .admit(AdmissibleTransition::new(
        TransitionMechanism::Reencode,
        DimensionId::REPRESENTATION,
    ))
    .require_capability(CapabilityRequirement::new(
        TransitionMechanism::Reencode,
        DimensionId::REPRESENTATION,
    ))
    .build()
    .unwrap();
    let declaration = RepresentationalDeclaration::new(
        spec,
        [
            (RepresentationId::new("tensor.fp16").unwrap(), 1),
            (RepresentationId::new("tensor.int8").unwrap(), 1),
        ],
    )
    .unwrap();
    let candidate = RepresentationPrecisionCandidateV1::new(
        "public-int8",
        0,
        RepresentationId::new("tensor.int8").unwrap(),
        1,
        TransitionMechanism::Reencode,
        8,
    )
    .unwrap();
    let preplanner =
        BooleanRepresentationPrecisionPreplannerV1::new(declaration, vec![candidate.clone()])
            .unwrap();
    let current = RepresentationState::new(
        RepresentationId::new("tensor.fp16").unwrap(),
        1,
        RepresentationEpoch::new(11),
    );
    let mut capabilities = CapabilitySet::new();
    capabilities.insert(RepresentationId::new("tensor.int8").unwrap(), 1);
    let now = Instant::now();
    let signal = representation_precision_floor_signal();
    let context = PlanningContext::new().observe(signal.clone(), 8.0);
    let observations = ObservationSnapshot::new(
        now,
        vec![Observation::from_source(
            ObservationSource::runtime("public-elang7-kv"),
            signal,
            8.0,
            now,
        )],
    );
    let report = preplanner
        .screen_with_trace(
            &current,
            &capabilities,
            &context,
            &observations,
            now,
            ObservationEpoch::new(1),
            ResourceGeneration::new(1),
        )
        .unwrap();

    let target = current
        .derive_target(
            TargetContract::New {
                id: RepresentationId::new("tensor.int8").unwrap(),
                schema_version: 1,
            },
            TransitionMechanism::Reencode,
        )
        .unwrap();
    let page = KvPageDescriptor {
        page: KvPageId::new(7),
        representation: current,
        precision: KvPrecision::F16,
        residency: KvResidency::Accelerator,
        key_transform_scope: KeyTransformScope::Raw,
        key_encoding_pipeline: KeyEncodingPipeline::Raw,
        recovery_source: KvRecoverySource::StoredCanonicalRaw,
    };
    let plan = page
        .validate_reusable_representation_change(
            target,
            TransitionMechanism::Reencode,
            &capabilities,
            TransitionAttestations::none().attest_reencoder_available(),
            KvTargetMaterialization::new(
                KeyTransformScope::Raw,
                KeyEncodingPipeline::Raw,
                KvRecoverySource::StoredCanonicalRaw,
            ),
        )
        .unwrap();

    let binding = RepresentationPrecisionKvBindingV1::new(candidate, report, plan).unwrap();
    assert_eq!(binding.kv_plan().representation.from.epoch.get(), 11);
    assert_eq!(binding.kv_plan().representation.to.epoch.get(), 12);
    assert_eq!(
        binding.kv_plan().representation.to.id.as_str(),
        "tensor.int8"
    );
    assert_ne!(binding.fingerprint().bits(), 0);
}
