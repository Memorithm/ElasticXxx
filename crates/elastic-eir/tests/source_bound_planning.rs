//! The public projection boundary must not accept foreign eligibility reports.

use elastic_core::resource::{
    AdmissibleTransition, CapabilityRequirement, DimensionId, LogicalResourceId, ResourceClassId,
    ResourceSpec,
};
use elastic_core::{
    BoolExpr, BooleanGuard, GuardScope, GuardedResourceSpec, PredicateKey, PredicateRegistry,
    TransitionMechanism, TruthValue,
};
use elastic_eir::{
    lower_guarded, prune_transition_candidates, EirGuardedResource, PlanningSubsetError,
    TransitionCandidate, TransitionPruningReport,
};
use std::collections::BTreeMap;

fn fixture(id: &str, revision: &str, policy: Option<bool>, grounded: bool) -> EirGuardedResource {
    let mut builder = ResourceSpec::builder(
        ResourceClassId::CAPACITY_RESOURCE,
        LogicalResourceId::new(id).unwrap(),
    )
    .allow(DimensionId::CAPACITY)
    .label("revision", revision)
    .admit(AdmissibleTransition::new(
        TransitionMechanism::Reinterpret,
        DimensionId::CAPACITY,
    ));
    if grounded {
        builder = builder.require_capability(CapabilityRequirement::new(
            TransitionMechanism::Reinterpret,
            DimensionId::CAPACITY,
        ));
    }
    let key = PredicateKey::new("test.projection", "eligible").unwrap();
    let registry = PredicateRegistry::from_keys([key.clone()]).unwrap();
    let expression = match policy {
        Some(value) => BoolExpr::Const(value),
        None => BoolExpr::atom(registry.id(&key).unwrap()),
    };
    let guard = BooleanGuard::new(GuardScope::Resource, registry, expression).unwrap();
    lower_guarded(&GuardedResourceSpec::new(builder.build().unwrap(), vec![guard]).unwrap())
        .unwrap()
}

fn empty_facts() -> BTreeMap<PredicateKey, TruthValue> {
    BTreeMap::new()
}

#[test]
fn same_transition_pairs_do_not_allow_foreign_resource_report_reuse() {
    let original = fixture("original", "v1", Some(true), true);
    let foreign = fixture("foreign", "v1", Some(true), true);
    let report = prune_transition_candidates(&original, &empty_facts()).unwrap();
    assert!(report.is_for_resource(&original));
    assert!(!report.is_for_resource(&foreign));
    assert_eq!(
        original.resource().transitions(),
        foreign.resource().transitions()
    );
    assert_eq!(
        foreign
            .restrict_to_eligible(&report, report.eligible())
            .unwrap_err(),
        PlanningSubsetError::SourceMismatch
    );
    assert_eq!(
        foreign.restrict_to_eligible(&report, &[]).unwrap_err(),
        PlanningSubsetError::SourceMismatch
    );
}

#[test]
fn guard_policy_changes_invalidate_report_even_when_base_eir_is_identical() {
    let allowed = fixture("same-resource", "v1", Some(true), true);
    let blocked = fixture("same-resource", "v1", Some(false), true);
    let report = prune_transition_candidates(&allowed, &empty_facts()).unwrap();
    assert_eq!(allowed.resource(), blocked.resource());
    assert_ne!(allowed.fingerprint(), blocked.fingerprint());
    assert_eq!(
        blocked
            .restrict_to_eligible(&report, report.eligible())
            .unwrap_err(),
        PlanningSubsetError::SourceMismatch
    );
}

#[test]
fn resource_content_change_invalidates_report_without_renaming_resource() {
    let before = fixture("same-resource", "v1", Some(true), true);
    let after = fixture("same-resource", "v2", Some(true), true);
    let report = prune_transition_candidates(&before, &empty_facts()).unwrap();
    assert_eq!(before.resource().identity(), after.resource().identity());
    assert_eq!(before.guards(), after.guards());
    assert_eq!(
        after
            .restrict_to_eligible(&report, report.eligible())
            .unwrap_err(),
        PlanningSubsetError::SourceMismatch
    );
}

#[test]
fn unbound_default_report_is_rejected_even_for_empty_projection() {
    let resource = fixture("default-report", "v1", Some(true), true);
    let report = TransitionPruningReport::default();
    assert!(!report.is_for_resource(&resource));
    assert_eq!(
        resource.restrict_to_eligible(&report, &[]).unwrap_err(),
        PlanningSubsetError::SourceMismatch
    );
}

#[test]
fn bound_report_cannot_expand_rejected_or_unknown_partition() {
    for policy in [Some(false), None] {
        let resource = fixture("ineligible", "v1", policy, true);
        let report = prune_transition_candidates(&resource, &empty_facts()).unwrap();
        let candidate = TransitionCandidate::from_admitted(&resource.resource().transitions()[0]);
        assert!(candidate.is_declared_in(resource.resource()));
        assert!(report.eligible().is_empty());
        assert!(matches!(
            resource.restrict_to_eligible(&report, &[candidate]),
            Err(PlanningSubsetError::CandidateNotEligible(_))
        ));
        let empty = resource.restrict_to_eligible(&report, &[]).unwrap();
        assert!(empty.transitions().is_empty());
        assert!(empty.capabilities().is_empty());
        assert_eq!(empty.label("revision"), Some("v1"));
    }
}

#[test]
fn valid_report_preserves_projection_and_cannot_lend_capability_grounding() {
    let resource = fixture("valid", "v1", Some(true), true);
    let report = prune_transition_candidates(&resource, &empty_facts()).unwrap();
    let first = report.eligible()[0].clone();
    let candidates = [first.clone(), first.with_magnitude(123)];
    let view = resource.restrict_to_eligible(&report, &candidates).unwrap();
    assert_eq!(&view, resource.resource());
    let ungrounded = fixture("valid", "v1", Some(true), false);
    let bad = TransitionCandidate::from_admitted(&ungrounded.resource().transitions()[0]);
    assert!(matches!(
        resource.restrict_to_eligible(&report, &[bad]),
        Err(PlanningSubsetError::InvalidCandidate(_))
    ));
}
