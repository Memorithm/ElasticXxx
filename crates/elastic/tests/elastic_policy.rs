use elastic::prelude::*;

elastic! {
    pub document policy_stack {
        resource inference {
            class(configurational);
            id("inference");
            allow(capacity);
            optimize(latency, throughput);
            admit(reinterpret @ capacity);
            capability(reinterpret @ capacity);
        }

        policy adaptive_runtime {
            id("runtime.inference");
            version(1, 2, 3);
            target(inference);

            predicate(capacity_ok, "elastic.inference", "capacity-ok");
            predicate(mode_a, "elastic.inference", "mode-a");
            predicate(mode_b, "elastic.inference", "mode-b");

            guard transition(reinterpret @ capacity) when(capacity_ok && !mode_b);

            constraint at_most(1, mode_a, mode_b);
            constraint at_least(1, mode_a, mode_b);
            constraint exactly(1, mode_a, mode_b);
            constraint requires(mode_a, capacity_ok);
            constraint equivalent(mode_a, mode_b);
            constraint budget {
                unit("MiB");
                quantum(1);
                maximum(12);
                term(mode_a, 4);
                term(mode_b, 8);
            }

            objective latency minimize unit("microseconds") quantum(1);
            objective throughput maximize unit("ops-per-second") quantum(1);
            hint("search.mode", "balanced");
            hint("candidate.limit", "16");
        }
    }
}

fn manual_policy() -> ResourcePolicyAdvisorySpec {
    let resource = policy_stack::inference::resource_spec().unwrap();
    let capacity_ok = PredicateKey::new("elastic.inference", "capacity-ok").unwrap();
    let mode_a = PredicateKey::new("elastic.inference", "mode-a").unwrap();
    let mode_b = PredicateKey::new("elastic.inference", "mode-b").unwrap();
    let predicates =
        ElasticPredicates::new([capacity_ok.clone(), mode_a.clone(), mode_b.clone()]).unwrap();
    let guard = elastic_guard! {
        predicates: predicates,
        scope: GuardScope::Transition {
            mechanism: TransitionMechanism::Reinterpret,
            dimension: DimensionId::CAPACITY,
        },
        when: (capacity_ok && !mode_b),
    }
    .unwrap();

    let constraints = vec![
        PseudoBooleanConstraintDeclaration::at_most_keys([mode_a.clone(), mode_b.clone()], 1)
            .unwrap(),
        PseudoBooleanConstraintDeclaration::at_least_keys([mode_a.clone(), mode_b.clone()], 1)
            .unwrap(),
        PseudoBooleanConstraintDeclaration::exactly_keys([mode_a.clone(), mode_b.clone()], 1)
            .unwrap(),
        PseudoBooleanConstraintDeclaration::requires_key(mode_a.clone(), capacity_ok.clone())
            .unwrap(),
        PseudoBooleanConstraintDeclaration::equivalent_keys(mode_a.clone(), mode_b.clone())
            .unwrap(),
        PseudoBooleanConstraintDeclaration::capacity_budget(
            vec![
                WeightedPredicateKey::new(mode_a, 4).unwrap(),
                WeightedPredicateKey::new(mode_b, 8).unwrap(),
            ],
            12,
            PseudoBooleanScale::new("MiB", 1).unwrap(),
        )
        .unwrap(),
    ];
    let policy = ResourcePolicySpec::new(
        PolicyHeader::new(
            PolicyIdentity::new(
                PolicyId::new("runtime.inference").unwrap(),
                PolicyVersion::new(1, 2, 3),
            ),
            PolicyTarget::resource(resource.resource_id().clone()),
        ),
        resource,
        vec![guard],
        constraints,
    )
    .unwrap();
    ResourcePolicyAdvisorySpec::new(
        policy,
        vec![
            PolicyNumericObjective::new(
                ObjectiveId::LATENCY,
                PolicyObjectiveDirection::Minimize,
                PolicyMetricScale::new("microseconds", 1).unwrap(),
            ),
            PolicyNumericObjective::new(
                ObjectiveId::THROUGHPUT,
                PolicyObjectiveDirection::Maximize,
                PolicyMetricScale::new("ops-per-second", 1).unwrap(),
            ),
        ],
        vec![
            PlannerHint::new(PlannerHintKey::new("search.mode").unwrap(), "balanced").unwrap(),
            PlannerHint::new(PlannerHintKey::new("candidate.limit").unwrap(), "16").unwrap(),
        ],
    )
    .unwrap()
}

#[test]
fn policy_dsl_equals_manual_core_and_eir() {
    let language = policy_stack::adaptive_runtime::policy_spec().unwrap();
    let manual = manual_policy();
    assert_eq!(language, manual);

    let language_eir = policy_stack::adaptive_runtime::policy_eir().unwrap();
    let manual_eir = lower_resource_policy_advisory(&manual).unwrap();
    assert_eq!(language_eir, manual_eir);
    assert_eq!(language_eir.fingerprint(), manual_eir.fingerprint());
    assert_eq!(
        language_eir.policy().fingerprint(),
        manual_eir.policy().fingerprint()
    );
    assert_eq!(language_eir.numeric_objectives().len(), 2);
    assert_eq!(language_eir.planner_hints()[0].key(), "candidate.limit");
}

elastic! {
    document bad_objective_stack {
        resource inference {
            class(configurational);
            allow(capacity);
            optimize(latency);
        }
        policy bad_objective {
            id("runtime.bad-objective");
            version(1, 0, 0);
            target(inference);
            objective throughput maximize unit("ops-per-second") quantum(1);
        }
    }
}

#[test]
fn undeclared_numeric_objective_remains_typed_fail_closed_error() {
    assert!(matches!(
        bad_objective_stack::bad_objective::policy_spec(),
        Err(ElasticPolicyDocumentError::Advisory(
            PolicyAdvisoryError::UndeclaredNumericObjective { .. }
        ))
    ));
}
