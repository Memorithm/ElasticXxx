use elastic::{
    BuiltinObservationSignalConfigV1, ForgeCandidateSourceV1, ForgePseudoBooleanConstraintV1,
    ForgePseudoBooleanRelationV1, ForgeSearchCandidateV1, ForgeWeightedPredicateV1, GuardConfigV1,
    GuardExprConfigV1, GuardRuleConfigV1, GuardScopeConfigV1, ObservationSignalConfigV1,
    PredicateConfigV1, PredicateKeyConfigV1, ThresholdComparisonConfigV1,
    FORGE_SEARCH_CANDIDATE_SCHEMA_V1, FORGE_SEARCH_PRODUCER_REPOSITORY_V1, GUARD_CONFIG_SCHEMA_V1,
};

fn key() -> PredicateKeyConfigV1 {
    PredicateKeyConfigV1 {
        namespace: "elastic.public-forge".to_owned(),
        name: "capacity-ok".to_owned(),
    }
}

fn candidate() -> ForgeSearchCandidateV1 {
    let predicate = key();
    ForgeSearchCandidateV1 {
        schema_version: FORGE_SEARCH_CANDIDATE_SCHEMA_V1,
        source: ForgeCandidateSourceV1 {
            repository: FORGE_SEARCH_PRODUCER_REPOSITORY_V1.to_owned(),
            commit_id: "a".repeat(40),
            candidate_id: "b".repeat(64),
            source_sha256: "c".repeat(64),
            envelope_fingerprint: "d".repeat(64),
        },
        guard_config: GuardConfigV1 {
            schema_version: GUARD_CONFIG_SCHEMA_V1,
            predicates: vec![PredicateConfigV1::ObservationThreshold {
                key: predicate.clone(),
                signal: ObservationSignalConfigV1::Builtin {
                    name: BuiltinObservationSignalConfigV1::FreeCapacity,
                },
                comparison: ThresholdComparisonConfigV1::GreaterOrEqual,
                threshold: 1.0,
                unit: "bytes".to_owned(),
                max_age_ms: 1_000,
            }],
            guards: vec![GuardRuleConfigV1 {
                scope: GuardScopeConfigV1::Resource,
                expression: GuardExprConfigV1::Atom {
                    predicate: predicate.clone(),
                },
            }],
        },
        constraints: vec![ForgePseudoBooleanConstraintV1 {
            terms: vec![ForgeWeightedPredicateV1 {
                predicate,
                weight: "1".to_owned(),
            }],
            relation: ForgePseudoBooleanRelationV1::LessOrEqual,
            threshold: "1".to_owned(),
            unit: "count".to_owned(),
            quantum: 1,
        }],
    }
}

#[test]
fn public_facade_revalidates_forge_candidate_without_actuation_authority() {
    let policy = candidate().revalidate().expect("valid candidate");
    assert_eq!(policy.guard_config().guards().len(), 1);
    assert_eq!(policy.constraints().len(), 1);
}

#[test]
fn public_facade_rejects_forge_constraint_outside_declared_elastic_predicates() {
    let mut value = candidate();
    value.constraints[0].terms[0].predicate.name = "undeclared".to_owned();
    assert!(value.revalidate().is_err());
}
