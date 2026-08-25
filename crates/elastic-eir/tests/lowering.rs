//! Integration tests for deterministic lowering and validation.

use elastic_core::resource::{
    AdmissibleTransition, CapabilityRequirement, DimensionId, Invariant, InvariantKind,
    LogicalResourceId, ObjectiveId, ObservationSignalId, ResourceClassId, ResourceSpec,
};
use elastic_core::TransitionMechanism;
use elastic_eir::{lower, ValidationError};

fn reencode_repr() -> AdmissibleTransition {
    AdmissibleTransition::new(TransitionMechanism::Reencode, DimensionId::REPRESENTATION)
}

fn kv_spec_a() -> ResourceSpec {
    ResourceSpec::builder(
        ResourceClassId::REPRESENTATIONAL,
        LogicalResourceId::new("session-kv").unwrap(),
    )
    .allow(DimensionId::REPRESENTATION)
    .allow(DimensionId::RESIDENCY)
    .preserve(Invariant::new(InvariantKind::PreserveContents))
    .optimize(ObjectiveId::LATENCY)
    .optimize(ObjectiveId::ENERGY)
    .admit(reencode_repr())
    .require_capability(CapabilityRequirement::new(
        TransitionMechanism::Reencode,
        DimensionId::REPRESENTATION,
    ))
    .observe(ObservationSignalId::FREE_CAPACITY)
    .label("workload", "slha-v2")
    .build()
    .unwrap()
}

/// Same declaration as `kv_spec_a`, assembled in a different order.
fn kv_spec_b() -> ResourceSpec {
    ResourceSpec::builder(
        ResourceClassId::REPRESENTATIONAL,
        LogicalResourceId::new("session-kv").unwrap(),
    )
    .allow(DimensionId::RESIDENCY)
    .allow(DimensionId::REPRESENTATION)
    .observe(ObservationSignalId::FREE_CAPACITY)
    .admit(reencode_repr())
    .preserve(Invariant::new(InvariantKind::PreserveContents))
    .optimize(ObjectiveId::LATENCY)
    .optimize(ObjectiveId::ENERGY)
    .require_capability(CapabilityRequirement::new(
        TransitionMechanism::Reencode,
        DimensionId::REPRESENTATION,
    ))
    .label("workload", "slha-v2")
    .build()
    .unwrap()
}

#[test]
fn equivalent_declarations_lower_to_identical_documents() {
    let a = lower(&kv_spec_a()).unwrap();
    let b = lower(&kv_spec_b()).unwrap();

    assert_eq!(a, b);
    assert_eq!(a.fingerprint(), b.fingerprint());
    assert_eq!(a.to_string(), b.to_string());
    assert_eq!(format!("{a:?}"), format!("{b:?}"));
}

#[test]
fn normalization_makes_priorities_and_ordering_explicit() {
    let doc = lower(&kv_spec_b()).unwrap();
    assert_eq!(doc.resources().len(), 1);
    let resource = doc.resource("session-kv").unwrap();

    // Objective priorities become explicit ranks regardless of insertion.
    let ranking = resource.objective_ranking();
    assert_eq!(ranking[0].rank(), 0);
    assert_eq!(ranking[0].objective(), &ObjectiveId::LATENCY);
    assert_eq!(ranking[1].rank(), 1);
    assert_eq!(ranking[1].objective(), &ObjectiveId::ENERGY);

    // Dimensions sorted canonically even though spec B inserted residency first.
    // (Built-in declaration order: residency precedes representation.)
    assert_eq!(
        resource.dimensions(),
        &[DimensionId::RESIDENCY, DimensionId::REPRESENTATION]
    );

    // Derived grounding fact.
    let transitions = resource.transitions();
    assert_eq!(transitions.len(), 1);
    assert!(transitions[0].capability_grounded());
    assert_eq!(
        resource.capabilities(),
        &[CapabilityRequirement::new(
            TransitionMechanism::Reencode,
            DimensionId::REPRESENTATION
        )]
    );
}

#[test]
fn ungrounded_capability_requirement_is_rejected() {
    let spec = ResourceSpec::builder(
        ResourceClassId::STATEFUL,
        LogicalResourceId::new("pool").unwrap(),
    )
    .allow(DimensionId::CAPACITY)
    .admit(AdmissibleTransition::new(
        TransitionMechanism::Reencode,
        DimensionId::CAPACITY,
    ))
    .require_capability(CapabilityRequirement::new(
        TransitionMechanism::Reinterpret,
        DimensionId::CAPACITY,
    ))
    .build()
    .unwrap();

    assert_eq!(
        lower(&spec),
        Err(ValidationError::CapabilityNotGroundedInAdmission {
            requirement: CapabilityRequirement::new(
                TransitionMechanism::Reinterpret,
                DimensionId::CAPACITY
            )
        })
    );
}

#[test]
fn raw_part_assembly_validates_like_lowering() {
    use elastic_eir::EirResourceParts;

    let mut parts = EirResourceParts {
        identity: "buf".to_owned(),
        class: ResourceClassId::STOCK,
        dimensions: vec![DimensionId::CAPACITY],
        invariants: Vec::new(),
        objectives: vec![ObjectiveId::MEMORY_FOOTPRINT],
        transitions: Vec::new(),
        capabilities: Vec::new(),
        observations: Vec::new(),
        labels: Default::default(),
    };
    assert!(elastic_eir::EirDocument::from_parts(vec![parts.clone()]).is_ok());

    parts.dimensions.clear();
    match elastic_eir::EirDocument::from_parts(vec![parts]) {
        Err(ValidationError::NoElasticDimensions) => {}
        other => panic!("expected NoElasticDimensions, got {other:?}"),
    }
}

#[test]
fn document_rejects_duplicate_identities_and_empty_content() {
    use elastic_eir::EirDocumentBuilder;

    let mut builder = EirDocumentBuilder::new();
    builder.push(&kv_spec_a()).unwrap();
    builder.push(&kv_spec_b()).unwrap();
    assert_eq!(
        builder.finish(),
        Err(ValidationError::DuplicateResourceIdentity {
            identity: "session-kv".to_owned()
        })
    );

    assert_eq!(
        EirDocumentBuilder::new().finish(),
        Err(ValidationError::EmptyDocument)
    );
}

#[test]
fn multi_resource_documents_sort_by_identity_and_fingerprint_content() {
    use elastic_eir::EirDocumentBuilder;

    let worker = ResourceSpec::builder(
        ResourceClassId::SHARED,
        LogicalResourceId::new("worker-pool").unwrap(),
    )
    .allow(DimensionId::PARALLELISM)
    .admit(AdmissibleTransition::new(
        TransitionMechanism::Reinterpret,
        DimensionId::PARALLELISM,
    ))
    .build()
    .unwrap();

    let mut builder = EirDocumentBuilder::new();
    builder.push(&worker).unwrap();
    builder.push(&kv_spec_a()).unwrap();
    let doc = builder.finish().unwrap();

    let identities: Vec<_> = doc
        .resources()
        .iter()
        .map(|r| r.identity().as_str())
        .collect();
    assert_eq!(identities, ["session-kv", "worker-pool"]);
    assert!(doc.resource("worker-pool").is_some());
    assert!(doc.resource("missing").is_none());

    // Re-inserting in reverse order produces an identical document.
    let mut reversed = EirDocumentBuilder::new();
    reversed.push(&kv_spec_a()).unwrap();
    reversed.push(&worker).unwrap();
    let doc2 = reversed.finish().unwrap();
    assert_eq!(doc, doc2);
    assert_eq!(doc.fingerprint(), doc2.fingerprint());
}

#[test]
fn schema_version_is_explicit_and_ordered() {
    let doc = lower(&kv_spec_a()).unwrap();
    assert_eq!(doc.schema_version(), elastic_eir::SchemaVersion::LATEST);
    assert_eq!(doc.schema_version().get(), 1);
    assert_eq!(doc.schema_version().to_string(), "eir-v1");
    assert!(elastic_eir::SchemaVersion::new(2) > doc.schema_version());
}

#[test]
fn fingerprints_are_stable_across_processes_and_non_cryptographic_display() {
    let doc = lower(&kv_spec_a()).unwrap();
    let text = doc.fingerprint().to_string();
    assert!(text.starts_with("fp:"), "{text}");
    assert_eq!(text.len(), 19);
    // Deterministic across repeated lowering within and across processes.
    for _ in 0..8 {
        assert_eq!(
            lower(&kv_spec_a()).unwrap().fingerprint(),
            doc.fingerprint()
        );
    }
}

#[test]
fn ir_nodes_are_send_sync_plain_data() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<elastic_eir::EirDocument>();
    assert_send_sync::<elastic_eir::EirResource>();
    assert_send_sync::<elastic_eir::Fingerprint>();
    assert_send_sync::<elastic_eir::ValidationError>();
}

#[test]
fn raw_part_permutation_of_transitions_preserves_determinism() {
    use elastic_core::resource::{AdmissibleTransition, DimensionId, ResourceClassId};
    use elastic_eir::EirResourceParts;

    let make_parts = |reversed: bool| EirResourceParts {
        identity: "buf".to_owned(),
        class: ResourceClassId::STOCK,
        dimensions: vec![DimensionId::CAPACITY],
        invariants: Vec::new(),
        objectives: vec![ObjectiveId::MEMORY_FOOTPRINT],
        transitions: if reversed {
            vec![
                AdmissibleTransition::new(TransitionMechanism::Reinterpret, DimensionId::CAPACITY),
                AdmissibleTransition::new(TransitionMechanism::Reencode, DimensionId::CAPACITY),
            ]
        } else {
            vec![
                AdmissibleTransition::new(TransitionMechanism::Reencode, DimensionId::CAPACITY),
                AdmissibleTransition::new(TransitionMechanism::Reinterpret, DimensionId::CAPACITY),
            ]
        },
        capabilities: Vec::new(),
        observations: Vec::new(),
        labels: Default::default(),
    };

    let a = elastic_eir::EirDocument::from_parts(vec![make_parts(false)]).unwrap();
    let b = elastic_eir::EirDocument::from_parts(vec![make_parts(true)]).unwrap();
    assert_eq!(a, b);
    assert_eq!(a.fingerprint(), b.fingerprint());
}
