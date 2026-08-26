#![forbid(unsafe_code)]

use elastic_core::{
    RepresentationEpoch, RepresentationId, RepresentationState, RepresentationTransition,
    TransitionError, TransitionMechanism,
};
use elastic_kv::{
    build_epoch_delta, KeyEncodingPipeline, KeyTransformScope, KvCacheCompatibility, KvPageId,
    KvRecoverySource, KvTransitionError, KvTransitionPlan, PageTransition,
};

fn state(epoch: u64) -> RepresentationState {
    RepresentationState::new(
        RepresentationId::new("kv.int4").expect("valid representation id"),
        1,
        RepresentationEpoch::new(epoch),
    )
}

#[test]
fn epoch_delta_rejects_regression_in_hand_assembled_batch() {
    let from = state(9);
    let to = state(8);
    let plan = KvTransitionPlan {
        representation: RepresentationTransition {
            from: from.clone(),
            to: to.clone(),
            mechanism: TransitionMechanism::Reencode,
        },
        target_key_transform_scope: KeyTransformScope::Raw,
        target_key_encoding_pipeline: KeyEncodingPipeline::Raw,
        target_recovery_source: KvRecoverySource::StoredCanonicalRaw,
        compatibility: KvCacheCompatibility::EpochReencodable,
    };

    let error = build_epoch_delta(&[PageTransition {
        page: KvPageId::new(7),
        plan,
    }])
    .expect_err("an epoch delta must never move backwards");

    assert_eq!(
        error,
        KvTransitionError::Core(TransitionError::EpochRegression {
            from: RepresentationEpoch::new(9),
            to: RepresentationEpoch::new(8),
        })
    );
}
