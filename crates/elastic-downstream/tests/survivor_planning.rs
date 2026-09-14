//! Survivor-only planning exercised through the public `elastic` dependency.

use elastic::prelude::*;
use elastic::{
    FreshnessSnapshot, ObservationEpoch, ObservationSnapshot, PlanOutcome, PlannerEpoch,
    PlanningSubsetError, ResourceGeneration, TransitionCandidate,
};
use std::cell::{Cell, RefCell};
use std::time::Instant;

fn key(dimension: &DimensionId) -> PredicateKey {
    predicate("downstream.survivors", dimension.as_str()).unwrap()
}

fn resource(id: &str, guards: bool, grounded: bool) -> EirGuardedResource {
    let mut builder = ResourceSpec::builder(
        ResourceClassId::CAPACITY_RESOURCE,
        LogicalResourceId::new(id).unwrap(),
    )
    .allow(DimensionId::CAPACITY)
    .allow(DimensionId::CONCURRENCY)
    .preserve(Invariant::new(InvariantKind::PreserveIdentity))
    .preserve(Invariant::new(InvariantKind::PreserveContents).along(DimensionId::CAPACITY))
    .optimize(ObjectiveId::LATENCY)
    .optimize(ObjectiveId::MEMORY_FOOTPRINT)
    .observe(ObservationSignalId::UTILIZATION)
    .label("purpose", "survivor-only");
    let mut policies = Vec::new();
    for dimension in [DimensionId::CAPACITY, DimensionId::CONCURRENCY] {
        builder = builder.admit(AdmissibleTransition::new(
            TransitionMechanism::Reinterpret,
            dimension.clone(),
        ));
        if grounded {
            builder = builder.require_capability(CapabilityRequirement::new(
                TransitionMechanism::Reinterpret,
                dimension.clone(),
            ));
        }
        if guards {
            let predicate = key(&dimension);
            let registry = ElasticPredicates::new([predicate.clone()]).unwrap();
            policies.push(
                ElasticGuard::transition(TransitionMechanism::Reinterpret, dimension, registry)
                    .requires(&predicate)
                    .unwrap(),
            );
        }
    }
    let guarded = GuardedResourceSpec::new(builder.build().unwrap(), policies).unwrap();
    lower_guarded(&guarded).unwrap()
}

fn evidence(
    resource: &EirGuardedResource,
    values: [Option<bool>; 2],
) -> (FactSnapshot, FreshnessSnapshot) {
    let context = PlanningContext::new();
    let now = Instant::now();
    let observations = ObservationSnapshot::new(now, Vec::new());
    let input = PredicateEvaluationInput::new(&context, &observations, now);
    let capacity = CapabilityPredicate::new(key(&DimensionId::CAPACITY), values[0]);
    let concurrency = CapabilityPredicate::new(key(&DimensionId::CONCURRENCY), values[1]);
    let identity = resource.resource().identity().clone();
    let facts = FactSnapshot::derive(
        FactSourceId::new("test:survivors").unwrap(),
        ObservationEpoch::new(7),
        Some(FactResourceBinding::new(
            identity.clone(),
            ResourceGeneration::new(2),
        )),
        &input,
        &[&capacity, &concurrency],
    )
    .unwrap();
    let freshness = FreshnessSnapshot::new(PlannerEpoch::new(3), ObservationEpoch::new(7))
        .with_resource_generation(identity, ResourceGeneration::new(2));
    (facts, freshness)
}

fn candidate(resource: &EirResource, dimension: &DimensionId) -> TransitionCandidate {
    TransitionCandidate::from_admitted(
        resource
            .transitions()
            .iter()
            .find(|admitted| admitted.transition().dimension() == dimension)
            .unwrap(),
    )
}

#[derive(Default)]
struct RecordingRanker {
    calls: Cell<usize>,
    seen: RefCell<Option<EirResource>>,
}

impl TransitionPlanner for RecordingRanker {
    fn propose_transition(&self, resource: &EirResource) -> PlanOutcome {
        self.calls.set(self.calls.get() + 1);
        *self.seen.borrow_mut() = Some(resource.clone());
        // A deterministic test objective prefers capacity over concurrency.
        // It must never see a disallowed winner, then discard a valid runner-up.
        resource
            .transitions()
            .iter()
            .filter(|admitted| admitted.capability_grounded())
            .max_by_key(|admitted| {
                if admitted.transition().dimension() == &DimensionId::CAPACITY {
                    100_u64
                } else {
                    50_u64
                }
            })
            .map(|admitted| {
                PlanOutcome::Candidate(
                    TransitionCandidate::from_admitted(admitted).with_magnitude(321),
                )
            })
            .unwrap_or(PlanOutcome::Unsupported)
    }
}

#[test]
fn all_nine_assignments_rank_exactly_the_survivor_pool() {
    let guarded = resource("ranked", true, true);
    let original = guarded.clone();
    for capacity in [Some(true), Some(false), None] {
        for concurrency in [Some(true), Some(false), None] {
            let (facts, freshness) = evidence(&guarded, [capacity, concurrency]);
            let planner = BooleanGuardPlanner::new(RecordingRanker::default());
            let outcome = planner
                .propose_transition_with_context(
                    &guarded,
                    &PlanningContext::new(),
                    &facts,
                    &freshness,
                )
                .unwrap();
            let expected: Vec<_> = guarded
                .resource()
                .transitions()
                .iter()
                .filter(|admitted| {
                    let value = if admitted.transition().dimension() == &DimensionId::CAPACITY {
                        capacity
                    } else {
                        concurrency
                    };
                    value == Some(true)
                })
                .map(TransitionCandidate::from_admitted)
                .collect();
            if expected.is_empty() {
                assert_eq!(planner.inner().calls.get(), 0);
                assert!(planner.inner().seen.borrow().is_none());
                if capacity.is_none() || concurrency.is_none() {
                    assert!(matches!(outcome, PlanOutcome::InsufficientEvidence { .. }));
                } else {
                    assert_eq!(outcome, PlanOutcome::NoCandidate);
                }
                continue;
            }

            assert_eq!(planner.inner().calls.get(), 1);
            let seen = planner.inner().seen.borrow();
            let seen = seen.as_ref().unwrap();
            let actual: Vec<_> = seen
                .transitions()
                .iter()
                .map(TransitionCandidate::from_admitted)
                .collect();
            assert_eq!(actual, expected);
            assert_eq!(seen.invariants(), guarded.resource().invariants());
            assert_eq!(
                seen.objective_ranking(),
                guarded.resource().objective_ranking()
            );
            assert_eq!(seen.observations(), guarded.resource().observations());
            assert_eq!(seen.label("purpose"), Some("survivor-only"));
            let winner = if capacity == Some(true) {
                DimensionId::CAPACITY
            } else {
                DimensionId::CONCURRENCY
            };
            let PlanOutcome::Candidate(selected) = outcome else {
                panic!("a surviving candidate must remain selectable");
            };
            assert_eq!(selected.dimension(), &winner);
            assert_eq!(selected.magnitude(), Some(321));
            assert!(selected.is_declared_in(guarded.resource()));
            assert!(selected.is_declared_in(seen));
            assert!(capture_decision_trace(&guarded, &facts, &freshness, Some(&selected)).is_ok());
            assert_eq!(guarded, original);
        }
    }
}

struct CachedOutputPlanner {
    candidate: TransitionCandidate,
    calls: Cell<usize>,
}

impl TransitionPlanner for CachedOutputPlanner {
    fn propose_transition(&self, _resource: &EirResource) -> PlanOutcome {
        self.calls.set(self.calls.get() + 1);
        PlanOutcome::Candidate(self.candidate.clone())
    }
}

#[test]
fn custom_output_cannot_reintroduce_rejected_or_unknown_candidates() {
    let guarded = resource("cached-output", true, true);
    for capacity in [Some(false), None] {
        let (facts, freshness) = evidence(&guarded, [capacity, Some(true)]);
        let planner = BooleanGuardPlanner::new(CachedOutputPlanner {
            candidate: candidate(guarded.resource(), &DimensionId::CAPACITY),
            calls: Cell::new(0),
        });
        let outcome = planner
            .propose_transition_with_context(&guarded, &PlanningContext::new(), &facts, &freshness)
            .unwrap();
        assert_eq!(planner.inner().calls.get(), 1);
        if capacity.is_none() {
            assert!(matches!(outcome, PlanOutcome::InsufficientEvidence { .. }));
        } else {
            assert_eq!(outcome, PlanOutcome::NoCandidate);
        }
    }
}

#[test]
fn ungrounded_custom_output_cannot_borrow_an_eligible_pairs_grounding() {
    let guarded = resource("grounded", true, true);
    let ungrounded = resource("ungrounded", false, false);
    let bad = candidate(ungrounded.resource(), &DimensionId::CAPACITY);
    assert!(!bad.capability_grounded());
    assert!(matches!(
        guarded
            .resource()
            .restrict_to_candidates(std::slice::from_ref(&bad)),
        Err(PlanningSubsetError::InvalidCandidate(_))
    ));
    let (facts, freshness) = evidence(&guarded, [Some(true), Some(true)]);
    let planner = BooleanGuardPlanner::new(CachedOutputPlanner {
        candidate: bad,
        calls: Cell::new(0),
    });
    let outcome = planner
        .propose_transition_with_context(&guarded, &PlanningContext::new(), &facts, &freshness)
        .unwrap();
    assert_eq!(outcome, PlanOutcome::Unsupported);
    assert_eq!(planner.inner().calls.get(), 1);
}

#[test]
fn exact_target_restricts_both_input_and_custom_output() {
    let guarded = resource("exact-target", true, true);
    let (facts, freshness) = evidence(&guarded, [Some(true), Some(true)]);
    let planner = BooleanGuardPlanner::for_capacity(RecordingRanker::default());
    let outcome = planner
        .propose_transition_with_context(&guarded, &PlanningContext::new(), &facts, &freshness)
        .unwrap();
    assert!(outcome.declares_valid_candidate(guarded.resource()));
    let seen = planner.inner().seen.borrow();
    let seen = seen.as_ref().unwrap();
    assert_eq!(seen.transitions().len(), 1);
    assert_eq!(
        seen.transitions()[0].transition().dimension(),
        &DimensionId::CAPACITY
    );

    let dishonest = BooleanGuardPlanner::for_capacity(CachedOutputPlanner {
        candidate: candidate(guarded.resource(), &DimensionId::CONCURRENCY),
        calls: Cell::new(0),
    });
    let outcome = dishonest
        .propose_transition_with_context(&guarded, &PlanningContext::new(), &facts, &freshness)
        .unwrap();
    assert_eq!(outcome, PlanOutcome::Unsupported);
}

#[test]
fn blocked_or_undeclared_exact_targets_never_run_the_numeric_planner() {
    let guarded = resource("blocked-target", true, true);
    for capacity in [Some(false), None] {
        let (facts, freshness) = evidence(&guarded, [capacity, Some(true)]);
        let planner = BooleanGuardPlanner::for_capacity(RecordingRanker::default());
        let outcome = planner
            .propose_transition_with_context(&guarded, &PlanningContext::new(), &facts, &freshness)
            .unwrap();
        assert_eq!(planner.inner().calls.get(), 0);
        if capacity.is_none() {
            assert!(matches!(outcome, PlanOutcome::InsufficientEvidence { .. }));
        } else {
            assert_eq!(outcome, PlanOutcome::NoCandidate);
        }
    }
    let (facts, freshness) = evidence(&guarded, [Some(true), Some(true)]);
    let planner = BooleanGuardPlanner::for_transition(
        RecordingRanker::default(),
        TransitionMechanism::Reencode,
        DimensionId::REPRESENTATION,
    );
    let outcome = planner
        .propose_transition_with_context(&guarded, &PlanningContext::new(), &facts, &freshness)
        .unwrap();
    assert_eq!(outcome, PlanOutcome::Unsupported);
    assert_eq!(planner.inner().calls.get(), 0);
}

#[test]
fn stale_or_foreign_facts_never_run_the_numeric_planner() {
    let guarded = resource("freshness", true, true);
    let (facts, _) = evidence(&guarded, [Some(true), Some(true)]);
    for (epoch, generation) in [(8, 2), (7, 3)] {
        let stale = FreshnessSnapshot::new(PlannerEpoch::new(3), ObservationEpoch::new(epoch))
            .with_resource_generation(
                guarded.resource().identity().clone(),
                ResourceGeneration::new(generation),
            );
        let planner = BooleanGuardPlanner::new(RecordingRanker::default());
        assert!(matches!(
            planner.propose_transition_with_context(
                &guarded,
                &PlanningContext::new(),
                &facts,
                &stale,
            ),
            Err(GuardPreplannerError::StaleFacts(_))
        ));
        assert_eq!(planner.inner().calls.get(), 0);
    }
    let other = resource("foreign-facts", true, true);
    let (_, freshness) = evidence(&guarded, [Some(true), Some(true)]);
    let planner = BooleanGuardPlanner::new(RecordingRanker::default());
    assert!(matches!(
        planner.propose_transition_with_context(&other, &PlanningContext::new(), &facts, &freshness),
        Err(GuardPreplannerError::ResourceBindingMismatch { .. })
    ));
    assert_eq!(planner.inner().calls.get(), 0);
}

#[test]
fn no_admissions_and_no_grounding_cannot_start_numeric_work() {
    let spec = ResourceSpec::builder(
        ResourceClassId::CAPACITY_RESOURCE,
        LogicalResourceId::new("no-admissions").unwrap(),
    )
    .allow(DimensionId::CAPACITY)
    .build()
    .unwrap();
    let empty = lower_guarded(&GuardedResourceSpec::new(spec, Vec::new()).unwrap()).unwrap();
    let ungrounded = resource("no-grounding", true, false);
    for guarded in [empty, ungrounded] {
        let (facts, freshness) = evidence(&guarded, [Some(true), Some(true)]);
        let planner = BooleanGuardPlanner::new(RecordingRanker::default());
        let outcome = planner
            .propose_transition_with_context(&guarded, &PlanningContext::new(), &facts, &freshness)
            .unwrap();
        assert_eq!(planner.inner().calls.get(), 0);
        if guarded.resource().transitions().is_empty() {
            assert_eq!(outcome, PlanOutcome::Unsupported);
        } else {
            assert!(matches!(outcome, PlanOutcome::InsufficientEvidence { .. }));
        }
    }
}

fn assert_legacy_parity<P: TransitionPlanner>(
    numeric: P,
    resource: &EirGuardedResource,
    context: &PlanningContext,
    facts: &FactSnapshot,
    freshness: &FreshnessSnapshot,
) {
    let legacy = numeric.propose_transition_with_context(resource.resource(), context);
    let guarded = BooleanGuardPlanner::for_capacity(numeric)
        .propose_transition_with_context(resource, context, facts, freshness)
        .unwrap();
    assert_eq!(guarded, legacy);
}

#[test]
fn true_and_absent_guards_preserve_capacity_controller_results() {
    for guards in [false, true] {
        let guarded = resource("numeric-parity", guards, true);
        let (facts, freshness) = evidence(&guarded, [Some(true), Some(true)]);
        for utilization in [0.0, 0.25, 0.5, 0.75, 1.0] {
            for committed in [1.0, 10.0, 100.0] {
                let context = PlanningContext::new()
                    .observe(ObservationSignalId::UTILIZATION, utilization)
                    .observe(ObservationSignalId::FREE_CAPACITY, 200.0 - committed)
                    .observe(
                        ObservationSignalId::custom("committed-bytes").unwrap(),
                        committed,
                    )
                    .observe(ObservationSignalId::custom("host-total-bytes").unwrap(), 200.0);
                assert_legacy_parity(
                    ThresholdPlanner::new(0.25, 0.75, 0.2).unwrap(),
                    &guarded,
                    &context,
                    &facts,
                    &freshness,
                );
                assert_legacy_parity(
                    HeadroomPlanner::new(0.25, 0.05).unwrap(),
                    &guarded,
                    &context,
                    &facts,
                    &freshness,
                );
            }
        }
        assert_legacy_parity(
            ThresholdPlanner::new(0.25, 0.75, 0.2).unwrap(),
            &guarded,
            &PlanningContext::new(),
            &facts,
            &freshness,
        );
        assert_legacy_parity(
            HeadroomPlanner::new(0.25, 0.05).unwrap(),
            &guarded,
            &PlanningContext::new(),
            &facts,
            &freshness,
        );
    }
}
