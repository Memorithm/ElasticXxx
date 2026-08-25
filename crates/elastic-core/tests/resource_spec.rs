//! Integration tests for the general elastic resource model.

use elastic_core::resource::{
    AdmissibleTransition, BuiltinDimension, CapabilityRequirement, ContractId, DimensionId,
    Invariant, InvariantKind, LogicalResourceId, ObjectiveId, ObservationSignalId, ResourceClassId,
    ResourceSpec, ResourceSpecError,
};
use elastic_core::TransitionMechanism;

fn spec_builder(id: &str) -> elastic_core::resource::ResourceSpecBuilder {
    ResourceSpec::builder(
        ResourceClassId::REPRESENTATIONAL,
        LogicalResourceId::new(id).unwrap(),
    )
}

fn kv_like_spec() -> ResourceSpec {
    spec_builder("session-kv")
        .allow(DimensionId::REPRESENTATION)
        .allow(DimensionId::RESIDENCY)
        .preserve(Invariant::new(InvariantKind::PreserveContents))
        .preserve(
            Invariant::new(InvariantKind::UpholdContract(
                ContractId::new("kv.reuse-contract").unwrap(),
            ))
            .along(DimensionId::REPRESENTATION),
        )
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
        .label("workload", "slha-v2")
        .build()
        .unwrap()
}

#[test]
fn valid_declaration_round_trips_accessors() {
    let spec = kv_like_spec();

    assert_eq!(spec.resource_id().as_str(), "session-kv");
    assert_eq!(spec.class(), &ResourceClassId::REPRESENTATIONAL);
    assert_eq!(
        spec.elastic_dimensions(),
        // Canonical order follows built-in declaration order.
        &[DimensionId::RESIDENCY, DimensionId::REPRESENTATION]
    );
    assert_eq!(
        spec.objectives(),
        &[ObjectiveId::LATENCY, ObjectiveId::MEMORY_FOOTPRINT]
    );
    assert_eq!(spec.invariants().len(), 2);
    assert_eq!(
        spec.admissible_transitions(),
        &[AdmissibleTransition::new(
            TransitionMechanism::Reencode,
            DimensionId::REPRESENTATION,
        )]
    );
    assert_eq!(
        spec.observed_signals(),
        &[ObservationSignalId::FREE_CAPACITY]
    );
    assert_eq!(spec.label("workload"), Some("slha-v2"));
    assert_eq!(spec.label("missing"), None);

    assert!(spec.is_elastic(&DimensionId::REPRESENTATION));
    assert!(!spec.is_elastic(&DimensionId::CAPACITY));
    assert!(spec.admits(TransitionMechanism::Reencode, &DimensionId::REPRESENTATION));
    assert!(!spec.admits(TransitionMechanism::Recompute, &DimensionId::REPRESENTATION));
    assert!(!spec.admits(TransitionMechanism::Reencode, &DimensionId::RESIDENCY));
    assert!(spec.requires_capability(&CapabilityRequirement::new(
        TransitionMechanism::Reencode,
        DimensionId::REPRESENTATION,
    )));
}

#[test]
fn declaration_order_does_not_affect_normalized_content() {
    let a = spec_builder("r")
        .allow(DimensionId::RESIDENCY)
        .allow(DimensionId::REPRESENTATION)
        .allow(DimensionId::ENERGY)
        .optimize(ObjectiveId::LATENCY)
        .admit(AdmissibleTransition::new(
            TransitionMechanism::Reinterpret,
            DimensionId::REPRESENTATION,
        ))
        .build()
        .unwrap();
    let b = spec_builder("r")
        .allow(DimensionId::ENERGY)
        .allow(DimensionId::REPRESENTATION)
        .allow(DimensionId::RESIDENCY)
        .optimize(ObjectiveId::LATENCY)
        .admit(AdmissibleTransition::new(
            TransitionMechanism::Reinterpret,
            DimensionId::REPRESENTATION,
        ))
        .build()
        .unwrap();

    assert_eq!(a, b);
    assert_eq!(
        a.elastic_dimensions(),
        b.elastic_dimensions(),
        "canonical iteration order must be identical"
    );
    assert_eq!(a.to_string(), b.to_string());
}

#[test]
fn objective_priority_order_is_preserved_and_distinct_from_invariants() {
    let spec = kv_like_spec();

    // First-declared objective has highest priority.
    assert_eq!(spec.objectives().first(), Some(&ObjectiveId::LATENCY));

    // Objectives never leak into invariants and vice versa: the invariant set
    // contains only declared invariants even though an objective shares the
    // "energy" name with a dimension.
    let energy_objective_only = ResourceSpec::builder(
        ResourceClassId::STATEFUL,
        LogicalResourceId::new("pool").unwrap(),
    )
    .allow(DimensionId::PARALLELISM)
    .optimize(ObjectiveId::ENERGY)
    .build()
    .unwrap();
    assert_eq!(energy_objective_only.objectives(), &[ObjectiveId::ENERGY]);
    assert!(energy_objective_only.invariants().is_empty());
}

#[test]
fn duplicate_declarations_are_rejected_with_structured_errors() {
    let duplicate_dimension = spec_builder("d")
        .allow(DimensionId::CAPACITY)
        .allow(DimensionId::CAPACITY)
        .build();
    assert_eq!(
        duplicate_dimension,
        Err(ResourceSpecError::DuplicateDimension {
            dimension: DimensionId::CAPACITY
        })
    );

    let duplicate_objective = spec_builder("d")
        .allow(DimensionId::CAPACITY)
        .optimize(ObjectiveId::THROUGHPUT)
        .optimize(ObjectiveId::THROUGHPUT)
        .build();
    assert_eq!(
        duplicate_objective,
        Err(ResourceSpecError::DuplicateObjective {
            objective: ObjectiveId::THROUGHPUT
        })
    );

    let invariant = Invariant::new(InvariantKind::PreserveContents);
    let duplicate_invariant = spec_builder("d")
        .allow(DimensionId::CAPACITY)
        .preserve(invariant.clone())
        .preserve(invariant)
        .build();
    assert_eq!(
        duplicate_invariant,
        Err(ResourceSpecError::DuplicateInvariant {
            invariant: Invariant::new(InvariantKind::PreserveContents)
        })
    );

    let transition =
        AdmissibleTransition::new(TransitionMechanism::Reencode, DimensionId::CAPACITY);
    let duplicate_transition = spec_builder("d")
        .allow(DimensionId::CAPACITY)
        .admit(transition.clone())
        .admit(transition.clone())
        .build();
    assert_eq!(
        duplicate_transition,
        Err(ResourceSpecError::DuplicateAdmissibleTransition { transition })
    );

    let requirement =
        CapabilityRequirement::new(TransitionMechanism::Reencode, DimensionId::CAPACITY);
    let duplicate_capability = spec_builder("d")
        .allow(DimensionId::CAPACITY)
        .require_capability(requirement.clone())
        .require_capability(requirement.clone())
        .build();
    assert_eq!(
        duplicate_capability,
        Err(ResourceSpecError::DuplicateCapabilityRequirement { requirement })
    );

    let duplicate_signal = spec_builder("d")
        .allow(DimensionId::CAPACITY)
        .observe(ObservationSignalId::UTILIZATION)
        .observe(ObservationSignalId::UTILIZATION)
        .build();
    assert_eq!(
        duplicate_signal,
        Err(ResourceSpecError::DuplicateObservation {
            signal: ObservationSignalId::UTILIZATION
        })
    );
}

#[test]
fn empty_elasticity_is_rejected() {
    assert_eq!(
        spec_builder("rigid").build(),
        Err(ResourceSpecError::NoElasticDimensions)
    );
}

#[test]
fn transitions_and_capabilities_must_stay_within_elastic_dimensions() {
    let transition_outside = spec_builder("t")
        .allow(DimensionId::REPRESENTATION)
        .admit(AdmissibleTransition::new(
            TransitionMechanism::Reencode,
            DimensionId::RESIDENCY,
        ))
        .build();
    assert_eq!(
        transition_outside,
        Err(ResourceSpecError::TransitionBeyondElasticDimensions {
            transition: AdmissibleTransition::new(
                TransitionMechanism::Reencode,
                DimensionId::RESIDENCY
            )
        })
    );

    let capability_outside = spec_builder("c")
        .allow(DimensionId::REPRESENTATION)
        .require_capability(CapabilityRequirement::new(
            TransitionMechanism::Reinterpret,
            DimensionId::LOCALITY,
        ))
        .build();
    assert_eq!(
        capability_outside,
        Err(ResourceSpecError::CapabilityBeyondElasticDimensions {
            requirement: CapabilityRequirement::new(
                TransitionMechanism::Reinterpret,
                DimensionId::LOCALITY
            )
        })
    );
}

#[test]
fn invariants_scoped_to_non_elastic_dimensions_are_vacuous() {
    let vacuous = spec_builder("v")
        .allow(DimensionId::REPRESENTATION)
        .preserve(Invariant::new(InvariantKind::PreserveContents).along(DimensionId::PARALLELISM))
        .build();
    assert_eq!(
        vacuous,
        Err(ResourceSpecError::VacuousInvariant {
            invariant: Invariant::new(InvariantKind::PreserveContents)
                .along(DimensionId::PARALLELISM)
        })
    );
}

#[test]
fn custom_extensions_participate_fully_in_the_model() {
    let dimension = DimensionId::custom("thermal-envelope").unwrap();
    let class = ResourceClassId::custom("agent-memory").unwrap();
    let objective = ObjectiveId::custom("tail-latency-p99").unwrap();
    let signal = ObservationSignalId::custom("page-fault-rate").unwrap();

    let spec = ResourceSpec::builder(class.clone(), LogicalResourceId::new("hotset").unwrap())
        .allow(dimension.clone())
        .optimize(objective.clone())
        .observe(signal.clone())
        .build()
        .unwrap();

    assert_eq!(spec.class(), &class);
    assert_eq!(spec.elastic_dimensions(), std::slice::from_ref(&dimension));
    assert!(dimension.builtin_part().is_none());
    assert_eq!(BuiltinDimension::Capacity.canonical(), "capacity");
    // Custom terms order after every built-in term.
    let mut mixed = [DimensionId::ENERGY, dimension];
    mixed.sort();
    assert_eq!(mixed[1].as_str(), "thermal-envelope");
    assert_eq!(spec.observed_signals(), &[signal]);
}

#[test]
fn blank_identifiers_and_labels_are_rejected() {
    assert_eq!(
        LogicalResourceId::new("  "),
        Err(ResourceSpecError::EmptyResourceId)
    );
    assert_eq!(
        spec_builder("x")
            .allow(DimensionId::CAPACITY)
            .label("", "value")
            .build(),
        Err(ResourceSpecError::InvalidLabelKey)
    );
}

#[test]
fn error_display_is_informative() {
    let error = ResourceSpecError::TransitionBeyondElasticDimensions {
        transition: AdmissibleTransition::new(
            TransitionMechanism::Reencode,
            DimensionId::RESIDENCY,
        ),
    };
    let text = error.to_string();
    assert!(text.contains("reencode@residency"), "{text}");
    assert!(text.contains("not declared elastic"), "{text}");
}

#[test]
fn specs_are_send_sync_cloneable_and_debuggable() {
    fn assert_send_sync<T: Send + Sync + Clone + std::fmt::Debug>() {}

    assert_send_sync::<ResourceSpec>();
    assert_send_sync::<ResourceSpecError>();
    assert_send_sync::<DimensionId>();
    assert_send_sync::<ObjectiveId>();
    assert_send_sync::<Invariant>();
    assert_send_sync::<AdmissibleTransition>();
    assert_send_sync::<CapabilityRequirement>();

    let spec = kv_like_spec();
    let clone = spec.clone();
    assert_eq!(clone, spec);
    let debug = format!("{spec:?}");
    assert!(debug.contains("session-kv"), "{debug}");
}
