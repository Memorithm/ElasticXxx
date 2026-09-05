use elastic::{
    ModelExecutionCapabilitiesV1, ModelExecutionProfileEnvelopeV1,
    ModelExecutionProfileSelectionV1, ModelExecutionProfileSelectorV1, ModelExecutionProfileSetV1,
    ModelExecutionProfileV1,
};

#[test]
fn public_facade_selects_only_published_correlated_profiles() {
    let capabilities = ModelExecutionCapabilitiesV1::new(
        "reference-backend",
        "model-rev-a",
        64,
        vec![1, 2, 4],
        vec![2_500, 5_000, 10_000],
        vec![2_500, 5_000, 10_000],
    )
    .unwrap();
    assert!(capabilities.supports(4, 2_500, 10_000));

    let profiles = ModelExecutionProfileSetV1::new(
        &capabilities,
        vec![
            ModelExecutionProfileV1::new("full", 0, 4, 10_000, 10_000).unwrap(),
            ModelExecutionProfileV1::new("balanced", 10, 2, 5_000, 5_000).unwrap(),
            ModelExecutionProfileV1::new("minimal", 20, 1, 2_500, 2_500).unwrap(),
        ],
    )
    .unwrap();

    assert!(profiles.profile_for_tuple(4, 2_500, 10_000).is_none());

    let selection = ModelExecutionProfileSelectorV1
        .select(
            &profiles,
            ModelExecutionProfileEnvelopeV1::new(2, 5_000, 6_000).unwrap(),
        )
        .unwrap();
    let ModelExecutionProfileSelectionV1::Selected(plan) = selection else {
        panic!("expected correlated profile selection")
    };

    assert_eq!(plan.profile_id(), "balanced");
    assert_eq!(plan.resource_plan().active_experts(), 2);
    assert_eq!(plan.resource_plan().expert_width_bps(), 5_000);
    assert_eq!(plan.resource_plan().activation_budget_bps(), 5_000);
    plan.resource_spec("conditional-model").unwrap();
}
