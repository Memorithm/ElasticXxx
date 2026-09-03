//! Public-facade guard for the SOUP EX7 resource-plan boundary.

#![forbid(unsafe_code)]

use elastic::{
    DimensionId, SoupAutoBatchStrategy, SoupBatchSize, SoupLayerStreamingV1,
    SoupRunResourcePlanV1, SoupStreamSource, SOUP_HUB_RESOURCE_CONTRACT_V1,
    SOUP_QUALIFIED_UPSTREAM_COMMIT, SOUP_RESOURCE_PLAN_V1,
};

#[test]
fn downstream_can_build_soup_resource_plan_from_elastic_only() {
    let stream = SoupLayerStreamingV1::new(SoupStreamSource::Ram, 2).unwrap();
    let plan = SoupRunResourcePlanV1::qualified(
        "sft",
        SoupBatchSize::Fixed(1),
        SoupAutoBatchStrategy::Auto,
        Some(stream),
    )
    .unwrap();
    let spec = plan.resource_spec("training/model-residency").unwrap();

    assert_eq!(plan.task(), "sft");
    assert!(spec.is_elastic(&DimensionId::CAPACITY));
    assert!(spec.is_elastic(&DimensionId::RESIDENCY));
    assert_eq!(spec.label("external.contract"), Some(SOUP_RESOURCE_PLAN_V1));
    assert_eq!(
        spec.label("external.upstream_commit"),
        Some(SOUP_QUALIFIED_UPSTREAM_COMMIT)
    );
    assert_eq!(
        SOUP_HUB_RESOURCE_CONTRACT_V1,
        "hub.ml.resource-requirements@1.0.0"
    );
}

#[test]
fn future_soup_revision_is_not_silently_accepted() {
    let error = SoupRunResourcePlanV1::from_external(
        "unreviewed-future-revision",
        "sft",
        SoupBatchSize::Auto,
        SoupAutoBatchStrategy::Auto,
        None,
    )
    .unwrap_err();

    assert!(error.to_string().contains("not qualified"));
}
