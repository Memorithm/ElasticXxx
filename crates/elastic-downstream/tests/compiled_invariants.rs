//! No implementation-crate dependency is available to this test.

use elastic::prelude::*;
use elastic::runtime::invariant_precheck::{
    CompiledInvariantPrecheck, CompiledInvariantPrecheckError, MAX_COMPILED_INVARIANTS,
};
use elastic::{Plan, PlanOutcome};

#[test]
fn compiled_invariant_layout_is_available_through_the_public_facade() {
    let invariant = Invariant::new(InvariantKind::PreserveContents);
    let spec = ResourceSpec::builder(
        ResourceClassId::CAPACITY_RESOURCE,
        LogicalResourceId::new("downstream-compiled-invariants").unwrap(),
    )
    .allow(DimensionId::CAPACITY)
    .preserve(invariant.clone())
    .admit(AdmissibleTransition::new(
        TransitionMechanism::Reinterpret,
        DimensionId::CAPACITY,
    ))
    .require_capability(CapabilityRequirement::new(
        TransitionMechanism::Reinterpret,
        DimensionId::CAPACITY,
    ))
    .build()
    .unwrap();
    let resource = lower(&spec).unwrap().resources()[0].clone();
    let outcome = FirstGroundedPlanner.propose_transition(&resource);
    let mut plan = Plan::new(resource, PlanningContext::new(), outcome, "facade test".into());
    let binding = InvariantPredicateBinding::new(
        invariant,
        predicate("downstream.invariant", "preserve-contents").unwrap(),
    );
    let compiled = CompiledInvariantPrecheck::compile(&plan, &[binding]).unwrap();
    assert_eq!(compiled.len(), 1);
    assert!(!compiled.is_empty());
    assert_eq!(MAX_COMPILED_INVARIANTS, 64);
    plan.outcome = PlanOutcome::NoCandidate;
    assert!(matches!(
        CompiledInvariantPrecheck::compile(&plan, &[]),
        Err(CompiledInvariantPrecheckError::NoCandidate)
    ));
}
