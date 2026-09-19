use elastic::prelude::*;

elastic! {
    pub document inference_stack {
        resource workers {
            class(shared);
            id("worker-pool");
            allow(parallelism);
            admit(reinterpret @ parallelism);
            capability(reinterpret @ parallelism);
        }
        resource cache {
            class(representational);
            id("session-kv");
            allow(representation, residency);
            preserve(contents);
            optimize(latency, memory_footprint);
            admit(reencode @ representation);
            capability(reencode @ representation);
            observe(free_capacity);
        }
    }
}

elastic! {
    document reverse_stack {
        resource cache_reverse {
            class(representational);
            id("session-kv");
            allow(representation, residency);
            preserve(contents);
            optimize(latency, memory_footprint);
            admit(reencode @ representation);
            capability(reencode @ representation);
            observe(free_capacity);
        }
        resource workers_reverse {
            class(shared);
            id("worker-pool");
            allow(parallelism);
            admit(reinterpret @ parallelism);
            capability(reinterpret @ parallelism);
        }
    }
}

elastic! {
    document duplicate_identity {
        resource first {
            class(shared);
            id("same-logical-resource");
            allow(capacity);
        }
        resource second {
            class(stateful);
            id("same-logical-resource");
            allow(concurrency);
        }
    }
}

elastic! {
    document invalid_child {
        resource good {
            class(shared);
            allow(capacity);
        }
        resource broken {
            class(representational);
            allow(capacity);
            preserve(contents along representation);
        }
    }
}

fn manual_document() -> EirDocument {
    let workers = ResourceSpec::builder(
        ResourceClassId::SHARED,
        LogicalResourceId::new("worker-pool").unwrap(),
    )
    .allow(DimensionId::PARALLELISM)
    .admit(AdmissibleTransition::new(
        TransitionMechanism::Reinterpret,
        DimensionId::PARALLELISM,
    ))
    .require_capability(CapabilityRequirement::new(
        TransitionMechanism::Reinterpret,
        DimensionId::PARALLELISM,
    ))
    .build()
    .unwrap();
    let cache = ResourceSpec::builder(
        ResourceClassId::REPRESENTATIONAL,
        LogicalResourceId::new("session-kv").unwrap(),
    )
    .allow(DimensionId::REPRESENTATION)
    .allow(DimensionId::RESIDENCY)
    .preserve(Invariant::new(InvariantKind::PreserveContents))
    .optimize(ObjectiveId::LATENCY)
    .optimize(ObjectiveId::MEMORY_FOOTPRINT)
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
    .unwrap();

    let mut builder = EirDocumentBuilder::new();
    builder.push(&workers).unwrap();
    builder.push(&cache).unwrap();
    builder.finish().unwrap()
}

#[test]
fn language_document_equals_manual_eir_document() {
    let manual = manual_document();
    let language = inference_stack::document().unwrap();

    assert_eq!(language, manual);
    assert_eq!(language.fingerprint(), manual.fingerprint());
    assert_eq!(
        language
            .resources()
            .iter()
            .map(|resource| resource.identity().as_str())
            .collect::<Vec<_>>(),
        ["session-kv", "worker-pool"]
    );
    assert_eq!(
        inference_stack::workers::resource_spec()
            .unwrap()
            .resource_id()
            .as_str(),
        "worker-pool"
    );
}

#[test]
fn language_document_syntax_order_does_not_change_eir_identity() {
    let forward = inference_stack::document().unwrap();
    let reverse = reverse_stack::document().unwrap();
    assert_eq!(forward, reverse);
    assert_eq!(forward.fingerprint(), reverse.fingerprint());
}

#[test]
fn duplicate_logical_identity_is_rejected_by_existing_eir_authority() {
    match duplicate_identity::document().unwrap_err() {
        ElasticDocumentError::Eir(ValidationError::DuplicateResourceIdentity { identity }) => {
            assert_eq!(identity, "same-logical-resource")
        }
        other => panic!("expected duplicate EIR identity, got {other:?}"),
    }
}

#[test]
fn invalid_child_resource_preserves_child_name_and_core_error() {
    match invalid_child::document().unwrap_err() {
        ElasticDocumentError::Resource { resource, source } => {
            assert_eq!(resource, "broken");
            assert!(matches!(source, ResourceSpecError::VacuousInvariant { .. }));
        }
        other => panic!("expected child resource error, got {other:?}"),
    }
}

#[test]
fn eir_document_bound_is_public_through_facade() {
    assert_eq!(MAX_EIR_DOCUMENT_RESOURCES, 256);
}
