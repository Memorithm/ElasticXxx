//! Public-facade BE14e representation/precision admission contract.

use std::time::Instant;

use elastic::resource::{
    AdmissibleTransition, CapabilityRequirement, DimensionId, LogicalResourceId,
    RepresentationalDeclaration, ResourceClassId, ResourceSpec,
};
use elastic::{
    representation_precision_floor_signal, BooleanRepresentationPrecisionOutcomeV1,
    BooleanRepresentationPrecisionPreplannerV1, CapabilitySet, Observation, ObservationEpoch,
    ObservationSnapshot, ObservationSource, PlanningContext, RepresentationEpoch, RepresentationId,
    RepresentationPrecisionCandidateV1, RepresentationState, ResourceGeneration,
    TransitionMechanism,
};

fn declaration() -> RepresentationalDeclaration {
    let spec = ResourceSpec::builder(
        ResourceClassId::REPRESENTATIONAL,
        LogicalResourceId::new("public-be14e-fixture").unwrap(),
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
    RepresentationalDeclaration::new(
        spec,
        [
            (RepresentationId::new("tensor.fp16").unwrap(), 1),
            (RepresentationId::new("tensor.int8").unwrap(), 1),
        ],
    )
    .unwrap()
}

#[test]
fn public_facade_exposes_fail_closed_fixed_width_preplanning() {
    let preplanner = BooleanRepresentationPrecisionPreplannerV1::new(
        declaration(),
        vec![RepresentationPrecisionCandidateV1::new(
            "tensor.int8",
            0,
            RepresentationId::new("tensor.int8").unwrap(),
            1,
            TransitionMechanism::Reencode,
            8,
        )
        .unwrap()],
    )
    .unwrap();
    let current = RepresentationState::new(
        RepresentationId::new("tensor.fp16").unwrap(),
        1,
        RepresentationEpoch::new(4),
    );
    let mut capabilities = CapabilitySet::new();
    capabilities.insert(RepresentationId::new("tensor.int8").unwrap(), 1);

    let now = Instant::now();
    let signal = representation_precision_floor_signal();
    let context = PlanningContext::new().observe(signal.clone(), 8.0);
    let observations = ObservationSnapshot::new(
        now,
        vec![Observation::from_source(
            ObservationSource::runtime("public-be14e-test"),
            signal,
            8.0,
            now,
        )],
    );
    let report = preplanner
        .screen(
            &current,
            &capabilities,
            &context,
            &observations,
            now,
            ObservationEpoch::new(1),
            ResourceGeneration::new(1),
        )
        .unwrap();

    assert_eq!(
        report.outcome,
        BooleanRepresentationPrecisionOutcomeV1::Selected {
            candidate_id: "tensor.int8".into(),
            preference_rank: 0,
        }
    );
}

#[test]
fn public_facade_missing_numeric_evidence_is_unknown() {
    let preplanner = BooleanRepresentationPrecisionPreplannerV1::new(
        declaration(),
        vec![RepresentationPrecisionCandidateV1::new(
            "tensor.int8",
            0,
            RepresentationId::new("tensor.int8").unwrap(),
            1,
            TransitionMechanism::Reencode,
            8,
        )
        .unwrap()],
    )
    .unwrap();
    let current = RepresentationState::new(
        RepresentationId::new("tensor.fp16").unwrap(),
        1,
        RepresentationEpoch::new(4),
    );
    let mut capabilities = CapabilitySet::new();
    capabilities.insert(RepresentationId::new("tensor.int8").unwrap(), 1);
    let now = Instant::now();

    let report = preplanner
        .screen(
            &current,
            &capabilities,
            &PlanningContext::new(),
            &ObservationSnapshot::new(now, vec![]),
            now,
            ObservationEpoch::new(1),
            ResourceGeneration::new(1),
        )
        .unwrap();

    assert!(matches!(
        report.outcome,
        BooleanRepresentationPrecisionOutcomeV1::InsufficientEvidence { .. }
    ));
}
