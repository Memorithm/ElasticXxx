use elastic_adapters::{
    SmlElasticWeightAdapterError, SmlElasticWeightPlanEnvelopeV1, SmlElasticWeightPlanFieldV1,
    SmlElasticWeightPlanV1, SmlElasticWeightTransitionV1, SmlWeightPrecisionV1,
    SmlWeightResidencyV1, SML_ELASTIC_WEIGHT_PLAN_V1, SML_ELASTIC_WEIGHT_SOURCE_COMMIT_V1,
    SML_WEIGHT_REPRESENTATION_SCHEMA_V1, SML_WEIGHT_RESIDUAL4_REPRESENTATION_V1,
};
use elastic_core::{
    CapabilitySet, RepresentationId, TransitionAttestations, TransitionError, TransitionMechanism,
};

fn transition_hot() -> SmlElasticWeightTransitionV1 {
    SmlElasticWeightTransitionV1 {
        page_id: 0,
        parameters: 128,
        from_representation_version: 0,
        to_representation_version: 1,
        from_precision: SmlWeightPrecisionV1::Boolean,
        to_precision: SmlWeightPrecisionV1::Residual4,
        from_residency: SmlWeightResidencyV1::Disk,
        to_residency: SmlWeightResidencyV1::Vram,
        from_active: false,
        to_active: true,
        target_payload_bytes: 64,
    }
}

fn transition_warm() -> SmlElasticWeightTransitionV1 {
    SmlElasticWeightTransitionV1 {
        page_id: 1,
        parameters: 256,
        from_representation_version: 0,
        to_representation_version: 1,
        from_precision: SmlWeightPrecisionV1::Boolean,
        to_precision: SmlWeightPrecisionV1::Ternary,
        from_residency: SmlWeightResidencyV1::Disk,
        to_residency: SmlWeightResidencyV1::Ram,
        from_active: false,
        to_active: true,
        target_payload_bytes: 64,
    }
}

fn transition_cold() -> SmlElasticWeightTransitionV1 {
    SmlElasticWeightTransitionV1 {
        page_id: 2,
        parameters: 512,
        from_representation_version: 0,
        to_representation_version: 0,
        from_precision: SmlWeightPrecisionV1::Boolean,
        to_precision: SmlWeightPrecisionV1::Boolean,
        from_residency: SmlWeightResidencyV1::Disk,
        to_residency: SmlWeightResidencyV1::Disk,
        from_active: false,
        to_active: false,
        target_payload_bytes: 64,
    }
}

fn qualified_envelope() -> SmlElasticWeightPlanEnvelopeV1 {
    SmlElasticWeightPlanEnvelopeV1 {
        contract: SML_ELASTIC_WEIGHT_PLAN_V1.to_owned(),
        source_commit: SML_ELASTIC_WEIGHT_SOURCE_COMMIT_V1.to_owned(),
        base_generation: 0,
        epoch: 11,
        ram_limit_bytes: 64,
        vram_limit_bytes: 64,
        max_active_parameters: 384,
        transitions: vec![transition_hot(), transition_warm(), transition_cold()],
        ram_bytes: 64,
        vram_bytes: 64,
        active_parameters: 384,
        active_storage_bits: 1_024,
        bytes_moved_to_ram: 64,
        bytes_moved_to_vram: 64,
        precision_changes: 2,
    }
}

#[test]
fn qualified_sml_plan_is_independently_revalidated() {
    let plan = SmlElasticWeightPlanV1::validate(qualified_envelope()).unwrap();

    assert_eq!(plan.source_commit(), SML_ELASTIC_WEIGHT_SOURCE_COMMIT_V1);
    assert_eq!(plan.base_generation(), 0);
    assert_eq!(plan.epoch(), 11);
    assert_eq!(plan.transitions().len(), 3);
    assert_eq!(plan.ram_bytes(), 64);
    assert_eq!(plan.vram_bytes(), 64);
    assert_eq!(plan.active_parameters(), 384);
    assert_eq!(plan.active_storage_bits(), 1_024);
    assert_eq!(plan.precision_changes(), 2);
}

#[test]
fn unknown_sml_source_revision_fails_closed() {
    let mut envelope = qualified_envelope();
    envelope.source_commit = "deadbeef".to_owned();

    assert_eq!(
        SmlElasticWeightPlanV1::validate(envelope).unwrap_err(),
        SmlElasticWeightAdapterError::SourceCommit {
            expected: SML_ELASTIC_WEIGHT_SOURCE_COMMIT_V1,
            actual: "deadbeef".to_owned(),
        }
    );
}

#[test]
fn tampered_aggregate_fails_closed() {
    let mut envelope = qualified_envelope();
    envelope.ram_bytes = 65;

    assert_eq!(
        SmlElasticWeightPlanV1::validate(envelope).unwrap_err(),
        SmlElasticWeightAdapterError::Accounting {
            field: SmlElasticWeightPlanFieldV1::RamBytes,
            expected: 64,
            actual: 65,
        }
    );
}

#[test]
fn active_disk_page_is_rejected() {
    let mut envelope = qualified_envelope();
    envelope.transitions[0].to_residency = SmlWeightResidencyV1::Disk;

    assert_eq!(
        SmlElasticWeightPlanV1::validate(envelope).unwrap_err(),
        SmlElasticWeightAdapterError::ActivePageOnDisk { page_id: 0 }
    );
}

#[test]
fn precision_change_projects_into_generic_representation_transition() {
    let transition = transition_hot();
    let projected = transition
        .representation_transition(TransitionMechanism::Recompute)
        .unwrap()
        .unwrap();

    assert_eq!(projected.from.epoch.get(), 0);
    assert_eq!(projected.to.epoch.get(), 1);
    assert_eq!(
        projected.to.id.as_str(),
        SML_WEIGHT_RESIDUAL4_REPRESENTATION_V1
    );
    assert_eq!(
        projected.to.schema_version,
        SML_WEIGHT_REPRESENTATION_SCHEMA_V1
    );
    assert_eq!(projected.mechanism, TransitionMechanism::Recompute);
}

#[test]
fn precision_transition_requires_generic_capability_and_materialization_evidence() {
    let transition = transition_hot();
    let mut capabilities = CapabilitySet::new();
    capabilities.insert(
        RepresentationId::new(SML_WEIGHT_RESIDUAL4_REPRESENTATION_V1).unwrap(),
        SML_WEIGHT_REPRESENTATION_SCHEMA_V1,
    );

    let missing = transition
        .validate_representation_transition(
            TransitionMechanism::Recompute,
            &capabilities,
            TransitionAttestations::none(),
        )
        .unwrap_err();
    assert_eq!(
        missing,
        SmlElasticWeightAdapterError::Representation(
            TransitionError::MissingRecomputeSourceAttestation
        )
    );

    let accepted = transition
        .validate_representation_transition(
            TransitionMechanism::Recompute,
            &capabilities,
            TransitionAttestations::none().attest_recompute_source_available(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        accepted.to.id.as_str(),
        SML_WEIGHT_RESIDUAL4_REPRESENTATION_V1
    );
}

#[test]
fn precision_change_never_assumes_reinterpretation() {
    assert_eq!(
        transition_hot()
            .representation_transition(TransitionMechanism::Reinterpret)
            .unwrap_err(),
        SmlElasticWeightAdapterError::ReinterpretPrecisionChange
    );
}

#[test]
fn residency_only_change_is_not_misrepresented_as_precision_transition() {
    let mut transition = transition_cold();
    transition.to_residency = SmlWeightResidencyV1::Ram;
    transition.to_active = true;

    assert_eq!(
        transition
            .representation_transition(TransitionMechanism::Recompute)
            .unwrap(),
        None
    );
}
