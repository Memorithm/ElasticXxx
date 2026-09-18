//! BE14b public-facade contract for guarded concurrency adaptation.

use elastic::{
    concurrency_headroom_predicate_key, BooleanConcurrencyResizeControllerV1,
    CONCURRENCY_HEADROOM_SOURCE_UNIT,
};

#[test]
fn public_facade_executes_guarded_concurrency_cycle_without_internal_crates() {
    let mut controller =
        BooleanConcurrencyResizeControllerV1::new("public-workers", 8, 4).expect("controller");
    let report = controller.resize(6).expect("guarded resize");

    assert_eq!(
        report.guard.predicate_key,
        concurrency_headroom_predicate_key().to_string()
    );
    assert_eq!(report.guard.source_unit, CONCURRENCY_HEADROOM_SOURCE_UNIT);
    assert_eq!(report.guard.truth, "true");
    assert_eq!(report.resize.committed, Some(true));
    assert_eq!(report.resize.final_width, 6);
}

#[test]
fn public_facade_false_guard_preserves_live_width() {
    let mut controller =
        BooleanConcurrencyResizeControllerV1::new("public-workers", 8, 4).expect("controller");
    let permits = controller.permits();
    permits.acquire().expect("holder one");
    permits.acquire().expect("holder two");
    permits.acquire().expect("holder three");

    let report = controller.resize(2).expect("guarded rejection");
    assert_eq!(report.guard.truth, "false");
    assert_eq!(report.resize.committed, Some(false));
    assert_eq!(report.resize.final_width, 4);
}
