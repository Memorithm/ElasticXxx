//! Tests for the extensible planning interface.

use elastic_core::resource::{
    AdmissibleTransition, CapabilityRequirement, DimensionId, Invariant, InvariantKind,
    LogicalResourceId, ObjectiveId, ResourceClassId, ResourceSpec,
};
use elastic_core::TransitionMechanism::{Reencode, Reinterpret};
use elastic_eir::{
    lower, EirResourceParts, FirstGroundedPlanner, PlanOutcome, TransitionCandidate,
    TransitionPlanner,
};

fn grounded_kv_resource() -> elastic_eir::EirResource {
    let spec = ResourceSpec::builder(
        ResourceClassId::REPRESENTATIONAL,
        LogicalResourceId::new("session-kv").unwrap(),
    )
    .allow(DimensionId::REPRESENTATION)
    .preserve(Invariant::new(InvariantKind::PreserveContents))
    .optimize(ObjectiveId::LATENCY)
    .admit(AdmissibleTransition::new(
        Reinterpret,
        DimensionId::REPRESENTATION,
    ))
    .admit(AdmissibleTransition::new(
        Reencode,
        DimensionId::REPRESENTATION,
    ))
    .require_capability(CapabilityRequirement::new(
        Reencode,
        DimensionId::REPRESENTATION,
    ))
    .build()
    .unwrap();
    lower(&spec)
        .unwrap()
        .resource("session-kv")
        .unwrap()
        .clone()
}

fn ungrounded_resource() -> elastic_eir::EirResource {
    let parts = EirResourceParts {
        identity: "buf".to_owned(),
        class: ResourceClassId::STOCK,
        dimensions: vec![DimensionId::CAPACITY],
        invariants: Vec::new(),
        objectives: vec![ObjectiveId::MEMORY_FOOTPRINT],
        transitions: vec![AdmissibleTransition::new(Reencode, DimensionId::CAPACITY)],
        capabilities: Vec::new(),
        observations: Vec::new(),
        labels: Default::default(),
    };
    elastic_eir::EirDocument::from_parts(vec![parts])
        .unwrap()
        .resource("buf")
        .unwrap()
        .clone()
}

#[test]
fn decision_table_of_the_reference_planner_is_honest() {
    // Grounded admission exists -> candidate (first in canonical order).
    let outcome = FirstGroundedPlanner.propose_transition(&grounded_kv_resource());
    match &outcome {
        PlanOutcome::Candidate(candidate) => {
            assert_eq!(candidate.mechanism(), Reencode);
            assert_eq!(candidate.dimension(), &DimensionId::REPRESENTATION);
            assert!(candidate.capability_grounded());
        }
        other => panic!("expected candidate, got {other}"),
    }
    assert!(outcome.declares_valid_candidate(&grounded_kv_resource()));

    // Only ungrounded admissions -> insufficient evidence.
    assert_eq!(
        FirstGroundedPlanner.propose_transition(&ungrounded_resource()),
        PlanOutcome::InsufficientEvidence {
            detail: "every admitted transition lacks a required capability".to_owned()
        }
    );

    // Nothing admitted at all -> unsupported.
    let rigid = ResourceSpec::builder(
        ResourceClassId::SHARED,
        LogicalResourceId::new("pool").unwrap(),
    )
    .allow(DimensionId::PARALLELISM)
    .build()
    .unwrap();
    let rigid_resource = lower(&rigid).unwrap().resource("pool").unwrap().clone();
    assert_eq!(
        FirstGroundedPlanner.propose_transition(&rigid_resource),
        PlanOutcome::Unsupported
    );
}

#[test]
fn reference_planner_is_deterministic_and_never_invents_transitions() {
    let resource = grounded_kv_resource();
    let first = FirstGroundedPlanner.propose_transition(&resource);
    for _ in 0..16 {
        assert_eq!(FirstGroundedPlanner.propose_transition(&resource), first);
    }
    assert!(first.declares_valid_candidate(&resource));

    // The same candidate is NOT declared by a resource that never admitted it.
    assert!(!first.declares_valid_candidate(&ungrounded_resource()));

    // An ungrounded admission never becomes a valid candidate, even though
    // the resource itself declares that admission: the contract demands
    // grounded proposals, not merely agreeing grounding flags.
    let ungrounded = ungrounded_resource();
    let rogue = TransitionCandidate::from_admitted(&ungrounded.transitions()[0]);
    assert!(!rogue.capability_grounded());
    assert!(!rogue.is_declared_in(&ungrounded));
    assert!(!PlanOutcome::Candidate(rogue).declares_valid_candidate(&ungrounded));
}

/// Downstream planners implement the trait; nothing here is sealed.
struct PreferReinterpretPlanner;

impl TransitionPlanner for PreferReinterpretPlanner {
    fn propose_transition(&self, resource: &elastic_eir::EirResource) -> PlanOutcome {
        resource
            .transitions()
            .iter()
            .find(|admitted| {
                admitted.transition().mechanism() == Reinterpret && admitted.capability_grounded()
            })
            .map(TransitionCandidate::from_admitted)
            .map_or(PlanOutcome::NoCandidate, |candidate| {
                if candidate.is_declared_in(resource) {
                    PlanOutcome::Candidate(candidate)
                } else {
                    PlanOutcome::NoCandidate
                }
            })
    }
}

#[test]
fn custom_planners_extend_without_touching_the_core() {
    let outcome = PreferReinterpretPlanner.propose_transition(&grounded_kv_resource());
    // kv resource grounds only the reencode admission, so reinterpret is not selectable.
    assert_eq!(outcome, PlanOutcome::NoCandidate);
}

#[test]
fn outcomes_display_and_are_plain_data() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<PlanOutcome>();
    assert_send_sync::<elastic_eir::TransitionCandidate>();

    let text = FirstGroundedPlanner
        .propose_transition(&grounded_kv_resource())
        .to_string();
    assert_eq!(text, "candidate reencode@representation");
}
