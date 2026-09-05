use elastic::{
    ModelExecutionCapabilitiesV1, ModelExecutionEnvelopePolicyV1, ModelExecutionEnvelopeRuleV1,
    ModelExecutionHardwarePlannerV1, ModelExecutionHardwareSelectionV1,
    ModelExecutionProfileEnvelopeV1, ModelExecutionProfileSetV1, ModelExecutionProfileV1,
    ModelExecutionResourceSnapshotV1,
};

#[test]
fn public_facade_resolves_snapshot_to_correlated_profile() {
    let capabilities = ModelExecutionCapabilitiesV1::new(
        "reference-backend",
        "model-rev-a",
        64,
        vec![1, 2, 4],
        vec![2_500, 5_000, 10_000],
        vec![2_500, 5_000, 10_000],
    )
    .unwrap();
    let profiles = ModelExecutionProfileSetV1::new(
        &capabilities,
        vec![
            ModelExecutionProfileV1::new("full", 0, 4, 10_000, 10_000).unwrap(),
            ModelExecutionProfileV1::new("balanced", 10, 2, 5_000, 5_000).unwrap(),
            ModelExecutionProfileV1::new("minimal", 20, 1, 2_500, 2_500).unwrap(),
        ],
    )
    .unwrap();

    let policy = ModelExecutionEnvelopePolicyV1::new(
        &profiles,
        "bytes",
        vec![
            ModelExecutionEnvelopeRuleV1::new(
                "rich",
                0,
                8_000,
                7_000,
                ModelExecutionProfileEnvelopeV1::new(4, 10_000, 10_000).unwrap(),
            )
            .unwrap(),
            ModelExecutionEnvelopeRuleV1::new(
                "balanced",
                10,
                2_000,
                9_000,
                ModelExecutionProfileEnvelopeV1::new(2, 5_000, 5_000).unwrap(),
            )
            .unwrap(),
            ModelExecutionEnvelopeRuleV1::new(
                "survival",
                20,
                0,
                10_000,
                ModelExecutionProfileEnvelopeV1::new(1, 2_500, 2_500).unwrap(),
            )
            .unwrap(),
        ],
    )
    .unwrap();

    let snapshot = ModelExecutionResourceSnapshotV1::new("bytes", 3_000, 8_000).unwrap();
    let selection = ModelExecutionHardwarePlannerV1
        .select(&policy, &profiles, &snapshot)
        .unwrap();

    let ModelExecutionHardwareSelectionV1::Selected {
        rule_id,
        rule_rank,
        plan,
    } = selection
    else {
        panic!("expected hardware-guided correlated profile")
    };

    assert_eq!(rule_id, "balanced");
    assert_eq!(rule_rank, 10);
    assert_eq!(plan.profile_id(), "balanced");
    assert_eq!(plan.resource_plan().active_experts(), 2);
    assert_eq!(plan.resource_plan().expert_width_bps(), 5_000);
    assert_eq!(plan.resource_plan().activation_budget_bps(), 5_000);
}
