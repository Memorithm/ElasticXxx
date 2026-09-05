use elastic::{
    lower, model_execution_current_profile_rank_signal, model_execution_profile_dimension,
    ModelExecutionAtomicProfilePlannerV1, ModelExecutionCapabilitiesV1,
    ModelExecutionProfileEnvelopeV1, ModelExecutionProfileSelectionV1,
    ModelExecutionProfileSelectorV1, ModelExecutionProfileSetV1, ModelExecutionProfileV1,
    PlanOutcome, PlanningContext, TransitionPlanner,
};

#[test]
fn public_facade_lowers_correlated_profile_to_one_atomic_transition() {
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
    let selection = ModelExecutionProfileSelectorV1
        .select(
            &profiles,
            ModelExecutionProfileEnvelopeV1::new(2, 5_000, 5_000).unwrap(),
        )
        .unwrap();
    let ModelExecutionProfileSelectionV1::Selected(target) = selection else {
        panic!("expected selected profile")
    };

    let spec = profiles.atomic_resource_spec("conditional-model-runtime").unwrap();
    assert_eq!(spec.elastic_dimensions(), &[model_execution_profile_dimension()]);
    let doc = lower(&spec).unwrap();
    let resource = doc.resource("conditional-model-runtime").unwrap();
    let planner = ModelExecutionAtomicProfilePlannerV1::new(&target);
    let context = PlanningContext::new()
        .observe(model_execution_current_profile_rank_signal(), 0.0);

    let outcome = planner.propose_transition_with_context(resource, &context);
    let PlanOutcome::Candidate(candidate) = outcome else {
        panic!("expected atomic runtime candidate")
    };
    assert_eq!(candidate.dimension(), &model_execution_profile_dimension());
    assert_eq!(candidate.magnitude(), Some(10));
    assert!(candidate.is_declared_in(resource));
}
