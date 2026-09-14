//! Differential and fail-closed contracts for the optional compiled precheck.

use elastic_core::resource::{
    AdmissibleTransition, CapabilityRequirement, ContractId, DimensionId, Invariant,
    InvariantKind, LogicalResourceId, ObservationSignalId, ResourceClassId, ResourceSpec,
};
use elastic_core::{
    FreshnessSnapshot, InvariantPredicateBinding, ObservationEpoch, PlannerEpoch,
    PredicateKey, ResourceGeneration, TransitionMechanism,
};
use elastic_eir::{lower, PlanOutcome, PlanningContext, TransitionCandidate};
use elastic_runtime::invariant_precheck::{
    CompiledInvariantPrecheck, CompiledInvariantPrecheckError, MAX_COMPILED_INVARIANTS,
};
use elastic_runtime::plan::validate_with_checks;
use elastic_runtime::{
    precheck_plan_invariants, CapabilityPredicate, FactResourceBinding, FactSnapshot,
    FactSourceId, InvariantPrecheckError, InvariantPrecheckStatus, ObservationSnapshot,
    Plan, PredicateEvaluationInput, PredicateEvaluator,
};
use std::time::Instant;

const ID: &str = "compiled-invariant-test";

fn invariants(count: usize) -> Vec<Invariant> {
    (0..count)
        .map(|index| {
            Invariant::new(InvariantKind::UpholdContract(
                ContractId::new(format!("invariant-{index:03}")).unwrap(),
            ))
        })
        .collect()
}

fn plan_for(invariants: &[Invariant], grounded: bool, id: &str) -> Plan {
    let mut builder = ResourceSpec::builder(
        ResourceClassId::CAPACITY_RESOURCE,
        LogicalResourceId::new(id).unwrap(),
    )
    .allow(DimensionId::CAPACITY)
    .allow(DimensionId::RESIDENCY);
    for invariant in invariants {
        builder = builder.preserve(invariant.clone());
    }
    for (mechanism, dimension) in [
        (TransitionMechanism::Reinterpret, DimensionId::CAPACITY),
        (TransitionMechanism::Reencode, DimensionId::RESIDENCY),
    ] {
        builder = builder.admit(AdmissibleTransition::new(mechanism, dimension.clone()));
        if grounded {
            builder = builder.require_capability(CapabilityRequirement::new(mechanism, dimension));
        }
    }
    let resource = lower(&builder.build().unwrap()).unwrap().resources()[0].clone();
    let admitted = resource
        .transitions()
        .iter()
        .find(|entry| entry.transition().dimension() == &DimensionId::CAPACITY)
        .unwrap();
    let candidate = TransitionCandidate::from_admitted(admitted).with_magnitude(1024);
    Plan::new(
        resource,
        PlanningContext::new().observe(ObservationSignalId::UTILIZATION, 0.5),
        PlanOutcome::Candidate(candidate),
        "synthetic precheck fixture".into(),
    )
}

fn bindings_for(invariants: &[Invariant]) -> Vec<InvariantPredicateBinding> {
    invariants
        .iter()
        .enumerate()
        .map(|(index, invariant)| {
            InvariantPredicateBinding::new(
                invariant.clone(),
                PredicateKey::new("test.invariant", format!("fact-{index:03}")).unwrap(),
            )
        })
        .collect()
}

fn facts(
    values: &[(PredicateKey, Option<bool>)],
    resource: Option<(&str, u64)>,
    epoch: u64,
) -> FactSnapshot {
    let now = Instant::now();
    let observations = ObservationSnapshot::new(now, Vec::new());
    let context = PlanningContext::new();
    let input = PredicateEvaluationInput::new(&context, &observations, now);
    // Synthetic truth sources only; these are not real capability attestations.
    let evaluators: Vec<_> = values
        .iter()
        .map(|(key, value)| CapabilityPredicate::new(key.clone(), *value))
        .collect();
    let references: Vec<&dyn PredicateEvaluator> = evaluators
        .iter()
        .map(|value| value as &dyn PredicateEvaluator)
        .collect();
    FactSnapshot::derive(
        FactSourceId::new("test:compiled-invariants").unwrap(),
        ObservationEpoch::new(epoch),
        resource.map(|(id, generation)| {
            FactResourceBinding::new(
                LogicalResourceId::new(id).unwrap(),
                ResourceGeneration::new(generation),
            )
        }),
        &input,
        &references,
    )
    .unwrap()
}

fn current(epoch: u64, generation: Option<u64>) -> FreshnessSnapshot {
    let snapshot = FreshnessSnapshot::new(PlannerEpoch::new(1), ObservationEpoch::new(epoch));
    match generation {
        Some(generation) => snapshot.with_resource_generation(
            LogicalResourceId::new(ID).unwrap(),
            ResourceGeneration::new(generation),
        ),
        None => snapshot,
    }
}

#[test]
fn exhaustive_reports_match_scalar_for_truth_binding_and_presence_combinations() {
    let plan = plan_for(&invariants(3), true, ID);
    let all_bindings = bindings_for(plan.resource.invariants());
    let freshness = current(7, Some(4));
    let mut comparisons = 0;
    for assignment in 0..27_usize {
        for binding_bits in 0..8 {
            let bindings: Vec<_> = all_bindings
                .iter()
                .enumerate()
                .filter(|(index, _)| binding_bits & (1 << index) != 0)
                .map(|(_, binding)| binding.clone())
                .collect();
            let compiled = CompiledInvariantPrecheck::compile(&plan, &bindings).unwrap();
            assert_eq!(compiled.len(), 3);
            for presence_bits in 0..8 {
                let values: Vec<_> = all_bindings
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| presence_bits & (1 << index) != 0)
                    .map(|(index, binding)| {
                        let value = match assignment / 3_usize.pow(index as u32) % 3 {
                            0 => Some(false),
                            1 => Some(true),
                            _ => None,
                        };
                        (binding.predicate().clone(), value)
                    })
                    .collect();
                let snapshot = facts(&values, Some((ID, 4)), 7);
                let scalar = precheck_plan_invariants(&plan, &bindings, &snapshot, &freshness)
                    .unwrap();
                let actual = compiled.evaluate(&plan, &snapshot, &freshness).unwrap();
                assert_eq!(actual, scalar);
                let masks = compiled.evaluate_summary(&plan, &snapshot, &freshness).unwrap();
                assert_eq!(masks.status(), scalar.status());
                assert_eq!(masks.required_bits(), 7);
                assert_eq!(masks.true_bits() & masks.false_bits(), 0);
                assert_eq!(masks.true_bits() & masks.unknown_bits(), 0);
                assert_eq!(masks.false_bits() & masks.unknown_bits(), 0);
                assert_eq!(masks.true_bits() | masks.false_bits() | masks.unknown_bits(), 7);
                comparisons += 1;
            }
        }
    }
    assert_eq!(comparisons, 1728);
}

#[test]
fn all_sixty_four_slots_and_shared_predicate_aliases_are_preserved() {
    let plan = plan_for(&invariants(MAX_COMPILED_INVARIANTS), true, ID);
    let key = PredicateKey::new("test.invariant", "shared").unwrap();
    let bindings: Vec<_> = plan
        .resource
        .invariants()
        .iter()
        .map(|invariant| InvariantPredicateBinding::new(invariant.clone(), key.clone()))
        .collect();
    let compiled = CompiledInvariantPrecheck::compile(&plan, &bindings).unwrap();
    for value in [Some(true), Some(false), None] {
        let snapshot = facts(&[(key.clone(), value)], Some((ID, 4)), 7);
        let masks = compiled.evaluate_summary(&plan, &snapshot, &current(7, Some(4))).unwrap();
        assert_eq!(masks.required_bits(), u64::MAX);
        assert_eq!(masks.true_bits(), if value == Some(true) { u64::MAX } else { 0 });
        assert_eq!(masks.false_bits(), if value == Some(false) { u64::MAX } else { 0 });
        assert_eq!(masks.unknown_bits(), if value.is_none() { u64::MAX } else { 0 });
        assert_eq!(
            compiled.evaluate(&plan, &snapshot, &current(7, Some(4))).unwrap(),
            precheck_plan_invariants(&plan, &bindings, &snapshot, &current(7, Some(4))).unwrap()
        );
    }
    let larger = plan_for(&invariants(MAX_COMPILED_INVARIANTS + 1), true, ID);
    assert!(matches!(
        CompiledInvariantPrecheck::compile(&larger, &[]),
        Err(CompiledInvariantPrecheckError::TooManyApplicableInvariants { max: 64 })
    ));
}

#[test]
fn capacity_applies_after_dimension_filtering_and_unrelated_failures_are_ignored() {
    let global = Invariant::new(InvariantKind::PreserveContents);
    let mut declarations = vec![global.clone()];
    declarations.extend(invariants(65).into_iter().map(|i| i.along(DimensionId::RESIDENCY)));
    let plan = plan_for(&declarations, true, ID);
    let keys = bindings_for(&[global, declarations[1].clone()]);
    let values = vec![
        (keys[0].predicate().clone(), Some(true)),
        (keys[1].predicate().clone(), Some(false)),
    ];
    let snapshot = facts(&values, Some((ID, 4)), 7);
    let compiled = CompiledInvariantPrecheck::compile(&plan, &keys).unwrap();
    assert_eq!(compiled.len(), 1);
    assert_eq!(
        compiled.evaluate(&plan, &snapshot, &current(7, Some(4))).unwrap(),
        precheck_plan_invariants(&plan, &keys, &snapshot, &current(7, Some(4))).unwrap()
    );
}

#[test]
fn duplicate_and_oversized_bindings_fail_even_when_not_applicable() {
    let plan = plan_for(&invariants(1), true, ID);
    let binding = bindings_for(&invariants(2)).pop().unwrap();
    let duplicates = vec![binding.clone(), binding.clone()];
    assert!(matches!(
        CompiledInvariantPrecheck::compile(&plan, &duplicates),
        Err(CompiledInvariantPrecheckError::Precheck(
            InvariantPrecheckError::DuplicateBinding { .. }
        ))
    ));
    assert!(matches!(
        CompiledInvariantPrecheck::compile(&plan, &vec![binding; 65]),
        Err(CompiledInvariantPrecheckError::Precheck(
            InvariantPrecheckError::TooManyBindings { .. }
        ))
    ));
}

#[test]
fn missing_mismatched_stale_and_future_facts_have_scalar_error_parity() {
    let plan = plan_for(&invariants(1), true, ID);
    let bindings = bindings_for(plan.resource.invariants());
    let compiled = CompiledInvariantPrecheck::compile(&plan, &bindings).unwrap();
    for (snapshot, freshness) in [
        (facts(&[], None, 7), current(7, Some(4))),
        (facts(&[], Some(("foreign", 4)), 7), current(7, Some(4))),
        (facts(&[], Some((ID, 4)), 6), current(7, Some(4))),
        (facts(&[], Some((ID, 4)), 8), current(7, Some(4))),
        (facts(&[], Some((ID, 3)), 7), current(7, Some(4))),
        (facts(&[], Some((ID, 4)), 7), current(7, None)),
    ] {
        let scalar = precheck_plan_invariants(&plan, &bindings, &snapshot, &freshness).unwrap_err();
        assert_eq!(
            compiled.evaluate_summary(&plan, &snapshot, &freshness).unwrap_err(),
            CompiledInvariantPrecheckError::Precheck(scalar)
        );
    }
}

#[test]
fn changed_plan_resource_candidate_magnitude_or_context_cannot_reuse_layout() {
    let plan = plan_for(&invariants(1), true, ID);
    let compiled = CompiledInvariantPrecheck::compile(&plan, &[]).unwrap();
    let snapshot = facts(&[], Some((ID, 4)), 7);
    let mut changes = Vec::new();
    let mut changed = plan.clone();
    changed.resource = plan_for(&invariants(2), true, ID).resource;
    changes.push(changed);
    let mut changed = plan.clone();
    changed.resource = plan_for(&invariants(1), true, "foreign").resource;
    changes.push(changed);
    let mut changed = plan.clone();
    changed.outcome = PlanOutcome::Candidate(plan.candidate().unwrap().clone().with_magnitude(2048));
    changes.push(changed);
    let mut changed = plan.clone();
    let other = plan.resource.transitions().iter().find(|entry| {
        entry.transition().dimension() == &DimensionId::RESIDENCY
    }).unwrap();
    changed.outcome = PlanOutcome::Candidate(TransitionCandidate::from_admitted(other));
    changes.push(changed);
    let mut changed = plan.clone();
    changed.context = PlanningContext::new().observe(ObservationSignalId::UTILIZATION, 0.6);
    changes.push(changed);
    let mut changed = plan.clone();
    changed.outcome = PlanOutcome::NoCandidate;
    changes.push(changed);
    for changed in changes {
        assert_eq!(
            compiled.evaluate_summary(&changed, &snapshot, &current(7, Some(4))).unwrap_err(),
            CompiledInvariantPrecheckError::PlanChanged
        );
    }
    let mut diagnostic_only = plan.clone();
    diagnostic_only.reasoning.push_str("; diagnostic note");
    assert!(compiled.evaluate_summary(&diagnostic_only, &snapshot, &current(7, Some(4))).is_ok());
}

#[test]
fn numeric_context_identity_distinguishes_signed_zero() {
    let mut plan = plan_for(&invariants(0), true, ID);
    plan.context = PlanningContext::new().observe(ObservationSignalId::UTILIZATION, 0.0);
    let compiled = CompiledInvariantPrecheck::compile(&plan, &[]).unwrap();
    plan.context = PlanningContext::new().observe(ObservationSignalId::UTILIZATION, -0.0);
    let snapshot = facts(&[], Some((ID, 4)), 7);
    assert_eq!(
        compiled.evaluate_summary(&plan, &snapshot, &current(7, Some(4))).unwrap_err(),
        CompiledInvariantPrecheckError::PlanChanged
    );
}

#[test]
fn reuse_never_caches_true_and_never_grants_trusted_validation() {
    let plan = plan_for(&invariants(1), true, ID);
    let before = plan.clone();
    let bindings = bindings_for(plan.resource.invariants());
    let compiled = CompiledInvariantPrecheck::compile(&plan, &bindings).unwrap();
    for (epoch, value, expected) in [
        (7, Some(true), InvariantPrecheckStatus::Passed),
        (8, Some(false), InvariantPrecheckStatus::Rejected),
        (9, None, InvariantPrecheckStatus::InsufficientEvidence),
    ] {
        let snapshot = facts(&[(bindings[0].predicate().clone(), value)], Some((ID, 4)), epoch);
        let report = compiled.evaluate(&plan, &snapshot, &current(epoch, Some(4))).unwrap();
        assert_eq!(report.status(), expected);
        assert!(!validate_with_checks(plan.clone(), Vec::new()).validated);
    }
    assert_eq!(plan, before);
}

#[test]
fn empty_applicable_set_still_requires_fresh_resource_provenance() {
    let plan = plan_for(&[], true, ID);
    let compiled = CompiledInvariantPrecheck::compile(&plan, &[]).unwrap();
    assert!(compiled.is_empty());
    let snapshot = facts(&[], Some((ID, 4)), 7);
    let summary = compiled.evaluate_summary(&plan, &snapshot, &current(7, Some(4))).unwrap();
    assert_eq!(summary.required_bits(), 0);
    assert_eq!(summary.status(), InvariantPrecheckStatus::Passed);
    assert!(compiled.evaluate_summary(&plan, &facts(&[], None, 7), &current(7, Some(4))).is_err());
}

#[test]
fn absent_or_ungrounded_candidate_is_not_compilable() {
    let mut plan = plan_for(&invariants(1), true, ID);
    for outcome in [PlanOutcome::NoCandidate, PlanOutcome::Unsupported] {
        plan.outcome = outcome;
        assert!(matches!(
            CompiledInvariantPrecheck::compile(&plan, &[]),
            Err(CompiledInvariantPrecheckError::NoCandidate)
        ));
    }
    let ungrounded = plan_for(&invariants(1), false, ID);
    assert!(matches!(
        CompiledInvariantPrecheck::compile(&ungrounded, &[]),
        Err(CompiledInvariantPrecheckError::InvalidCandidate)
    ));
}
