use elastic::{
    DimensionId, ModelExecutionCapabilitiesV1, ModelExecutionResourcePlanV1,
    TransitionMechanism, MODEL_EXECUTION_ACTIVATION_BUDGET_DIMENSION,
    MODEL_EXECUTION_ACTIVE_EXPERTS_DIMENSION, MODEL_EXECUTION_EXPERT_WIDTH_DIMENSION,
};

#[test]
fn public_facade_builds_and_lowers_model_execution_resource_plan() {
    let capabilities = ModelExecutionCapabilitiesV1::new(
        "reference-backend",
        "model-rev-a",
        64,
        vec![1, 2, 4],
        vec![2_500, 5_000, 10_000],
        vec![2_500, 5_000, 10_000],
    )
    .unwrap();
    let plan = ModelExecutionResourcePlanV1::new(&capabilities, 2, 5_000, 5_000).unwrap();
    let spec = plan.resource_spec("conditional-model").unwrap();

    for text in [
        MODEL_EXECUTION_ACTIVE_EXPERTS_DIMENSION,
        MODEL_EXECUTION_EXPERT_WIDTH_DIMENSION,
        MODEL_EXECUTION_ACTIVATION_BUDGET_DIMENSION,
    ] {
        let dimension = DimensionId::custom(text).unwrap();
        assert!(spec.is_elastic(&dimension));
        assert!(spec.admits(TransitionMechanism::Reinterpret, &dimension));
    }
}
