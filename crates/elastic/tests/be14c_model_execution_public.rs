//! BE14c public-facade contract for fail-closed model-execution rule screening.

use std::time::Instant;

use elastic::{
    BooleanModelExecutionPreplannerV1, BooleanModelExecutionScreenOutcomeV1,
    ModelExecutionCapabilitiesV1, ModelExecutionEnvelopePolicyV1, ModelExecutionEnvelopeRuleV1,
    ModelExecutionProfileEnvelopeV1, ModelExecutionProfileSetV1, ModelExecutionProfileV1,
    Observation, ObservationSignalId, ObservationSnapshot, ObservationSource, PlanningContext,
};

fn preplanner() -> BooleanModelExecutionPreplannerV1 {
    let capabilities = ModelExecutionCapabilitiesV1::new(
        "public-backend",
        "model-rev-a",
        64,
        vec![1, 2],
        vec![5_000, 10_000],
        vec![5_000, 10_000],
    )
    .unwrap();
    let profiles = ModelExecutionProfileSetV1::new(
        &capabilities,
        vec![
            ModelExecutionProfileV1::new("full", 0, 2, 10_000, 10_000).unwrap(),
            ModelExecutionProfileV1::new("reduced", 10, 1, 5_000, 5_000).unwrap(),
        ],
    )
    .unwrap();
    let policy = ModelExecutionEnvelopePolicyV1::new(
        &profiles,
        "bytes",
        vec![
            ModelExecutionEnvelopeRuleV1::new(
                "full-capacity",
                0,
                8_000,
                7_000,
                ModelExecutionProfileEnvelopeV1::new(2, 10_000, 10_000).unwrap(),
            )
            .unwrap(),
            ModelExecutionEnvelopeRuleV1::new(
                "reduced-capacity",
                10,
                2_000,
                9_000,
                ModelExecutionProfileEnvelopeV1::new(1, 5_000, 5_000).unwrap(),
            )
            .unwrap(),
        ],
    )
    .unwrap();
    BooleanModelExecutionPreplannerV1::new(policy, profiles).unwrap()
}

#[test]
fn public_facade_screens_complete_evidence_before_numeric_planning() {
    let preplanner = preplanner();
    let now = Instant::now();
    let context = PlanningContext::new()
        .observe(ObservationSignalId::FREE_CAPACITY, 9_000.0)
        .observe(ObservationSignalId::UTILIZATION, 0.60);
    let observations = ObservationSnapshot::new(
        now,
        vec![
            Observation::from_source(
                ObservationSource::runtime("public-be14c"),
                ObservationSignalId::FREE_CAPACITY,
                9_000.0,
                now,
            ),
            Observation::from_source(
                ObservationSource::runtime("public-be14c"),
                ObservationSignalId::UTILIZATION,
                0.60,
                now,
            ),
        ],
    );
    let report = preplanner.screen(&context, &observations, now);
    assert_eq!(report.capacity_unit, "bytes");
    assert_eq!(
        report.outcome,
        BooleanModelExecutionScreenOutcomeV1::Selected {
            rule_id: "full-capacity".into(),
            rule_rank: 0,
        }
    );
}

#[test]
fn public_facade_missing_evidence_is_unknown_not_a_match() {
    let preplanner = preplanner();
    let now = Instant::now();
    let report = preplanner.screen(
        &PlanningContext::new(),
        &ObservationSnapshot::new(now, Vec::new()),
        now,
    );
    assert!(matches!(
        report.outcome,
        BooleanModelExecutionScreenOutcomeV1::InsufficientEvidence { .. }
    ));
}
