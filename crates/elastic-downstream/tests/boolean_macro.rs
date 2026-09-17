//! Only the public `elastic` dependency is available to this integration test.

use elastic::prelude::*;
use elastic::{GuardFactSource, PlanOutcome};
use std::cell::Cell;
use std::collections::BTreeMap;

fn fixture() -> (ElasticPredicates, PredicateKey, PredicateKey, PredicateKey) {
    let a = predicate("downstream.logic", "a").unwrap();
    let b = predicate("downstream.logic", "b").unwrap();
    let c = predicate("downstream.logic", "c").unwrap();
    let predicates = ElasticPredicates::new([c.clone(), a.clone(), b.clone()]).unwrap();
    (predicates, a, b, c)
}

#[test]
fn macro_and_manual_guards_match_for_all_three_valued_assignments() {
    let (predicates, a, b, c) = fixture();
    let mixed = elastic_guard! {
        predicates: predicates, scope: GuardScope::Resource, when: (a || b && !c)
    }
    .unwrap();
    let xor = elastic_guard! {
        predicates: predicates, scope: GuardScope::Resource, when: (a ^ b ^ c)
    }
    .unwrap();
    let implication = elastic_guard! {
        predicates: predicates, scope: GuardScope::Resource, when: (implies((a && b), (c)))
    }
    .unwrap();
    let atom_a = predicates.atom(&a).unwrap();
    let atom_b = predicates.atom(&b).unwrap();
    let atom_c = predicates.atom(&c).unwrap();
    let builder = ElasticGuard::resource(predicates.clone());
    let manual = [
        builder
            .when(ElasticGuard::any([
                atom_a.clone(),
                ElasticGuard::all([atom_b.clone(), ElasticGuard::not(atom_c.clone())]),
            ]))
            .unwrap(),
        builder
            .when(ElasticGuard::xor(
                atom_a.clone(),
                ElasticGuard::xor(atom_b.clone(), atom_c.clone()),
            ))
            .unwrap(),
        builder
            .when(ElasticGuard::implies(
                ElasticGuard::all([atom_a, atom_b]),
                atom_c,
            ))
            .unwrap(),
    ];
    let generated = [mixed, xor, implication];
    for (actual, expected) in generated.iter().zip(&manual) {
        assert_eq!(actual, expected);
        assert_eq!(actual.fingerprint(), expected.fingerprint());
        for x in [TruthValue::True, TruthValue::False, TruthValue::Unknown] {
            for y in [TruthValue::True, TruthValue::False, TruthValue::Unknown] {
                for z in [TruthValue::True, TruthValue::False, TruthValue::Unknown] {
                    let facts = BTreeMap::from([(a.clone(), x), (b.clone(), y), (c.clone(), z)]);
                    assert_eq!(actual.evaluate(&facts), expected.evaluate(&facts));
                }
            }
        }
    }
}

#[test]
fn parentheses_override_precedence_and_preserve_unknown() {
    let (predicates, a, b, _) = fixture();
    let guard = elastic_guard! {
        predicates: &predicates, scope: GuardScope::Resource, when: ((a || b) && !a),
    }
    .unwrap();
    let contradiction = elastic_guard! {
        predicates: predicates, scope: GuardScope::Resource, when: (a && !a)
    }
    .unwrap();
    let facts = BTreeMap::<PredicateKey, TruthValue>::new();
    assert_eq!(guard.evaluate(&facts).unwrap(), TruthValue::Unknown);
    assert_eq!(contradiction.evaluate(&facts).unwrap(), TruthValue::Unknown);
}

#[test]
fn unregistered_key_is_rejected_even_behind_absorbing_constants() {
    let (predicates, _, _, _) = fixture();
    let foreign = predicate("downstream.logic", "foreign").unwrap();
    let rejected = elastic_guard! {
        predicates: predicates, scope: GuardScope::Resource, when: (false && foreign)
    };
    assert!(matches!(
        rejected,
        Err(ElasticGuardError::UnknownPredicate { .. })
    ));
    let rejected = elastic_guard! {
        predicates: predicates, scope: GuardScope::Resource, when: (true || foreign)
    };
    assert!(matches!(
        rejected,
        Err(ElasticGuardError::UnknownPredicate { .. })
    ));
}

#[test]
fn macro_borrows_keys_and_evaluates_header_expressions_once() {
    let (predicates, a, _, _) = fixture();
    let registry_calls = Cell::new(0);
    let scope_calls = Cell::new(0);
    let guard = elastic_guard! {
        predicates: {
            registry_calls.set(registry_calls.get() + 1);
            &predicates
        },
        scope: {
            scope_calls.set(scope_calls.get() + 1);
            GuardScope::Dimension(DimensionId::CAPACITY)
        },
        when: (a),
    }
    .unwrap();
    assert_eq!(registry_calls.get(), 1);
    assert_eq!(scope_calls.get(), 1);
    assert_eq!(guard.scope(), &GuardScope::Dimension(DimensionId::CAPACITY));
    assert!(predicates.atom(&a).is_ok());
}

#[test]
fn macro_hygiene_supports_alias_and_colliding_local_names() {
    use elastic as renamed;
    let (__elastic_predicates, __elastic_scope, __left, __right) = fixture();
    let guard = renamed::elastic_guard! {
        predicates: __elastic_predicates,
        scope: renamed::GuardScope::Resource,
        when: (__elastic_scope && (__left || __right)),
    }
    .unwrap();
    let facts = BTreeMap::from([
        (__elastic_scope, TruthValue::True),
        (__left, TruthValue::False),
        (__right, TruthValue::True),
    ]);
    assert_eq!(guard.evaluate(&facts).unwrap(), TruthValue::True);
}

#[test]
fn constant_guards_need_no_predicates() {
    let predicates = ElasticPredicates::empty();
    let guard = elastic_guard! {
        predicates: predicates, scope: GuardScope::Resource, when: (true && !false)
    }
    .unwrap();
    let facts = BTreeMap::<PredicateKey, TruthValue>::new();
    assert_eq!(guard.evaluate(&facts).unwrap(), TruthValue::True);
}

#[test]
fn macro_guard_reaches_eir_pruning_through_facade_only() {
    let (predicates, a, _, _) = fixture();
    let spec = ResourceSpec::builder(
        ResourceClassId::CAPACITY_RESOURCE,
        LogicalResourceId::new("macro-capacity").unwrap(),
    )
    .allow(DimensionId::CAPACITY)
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
    let guard = elastic_guard! {
        predicates: predicates,
        scope: GuardScope::Transition {
            mechanism: TransitionMechanism::Reinterpret,
            dimension: DimensionId::CAPACITY,
        },
        when: (a),
    }
    .unwrap();
    let guarded = lower_guarded(&GuardedResourceSpec::new(spec, vec![guard]).unwrap()).unwrap();
    let facts = BTreeMap::from([(a.clone(), TruthValue::True)]);
    assert_eq!(facts.truth(&a), TruthValue::True);
    let report = prune_transition_candidates(&guarded, &facts).unwrap();
    let legacy = FirstGroundedPlanner.propose_transition(guarded.resource());
    assert_eq!(
        report.first_eligible().cloned().map(PlanOutcome::Candidate),
        Some(legacy)
    );
    let missing = BTreeMap::<PredicateKey, TruthValue>::new();
    let report = prune_transition_candidates(&guarded, &missing).unwrap();
    assert!(report.eligible().is_empty());
    assert_eq!(report.unknown().len(), 1);
}
