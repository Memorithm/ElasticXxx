//! Public-facade ELANG7 anti-thrashing contract.

use std::time::{Duration, Instant};

use elastic::prelude::*;

fn snapshot(source: &ObservationSource, value: f64, at: Instant) -> ObservationSnapshot {
    ObservationSnapshot::new(
        at,
        vec![Observation::from_source(
            source.clone(),
            ObservationSignalId::UTILIZATION,
            value,
            at,
        )],
    )
}

#[test]
fn facade_only_transition_stability_enforces_hysteresis_cooldown_and_rate_limit() {
    let source = ObservationSource::runtime("public-anti-thrash");
    let policy = TransitionStabilityPolicyV1::new(
        TransitionMechanism::Reinterpret,
        DimensionId::CAPACITY,
        Some(
            HysteresisPolicyV1::new(
                source.clone(),
                ObservationSignalId::UTILIZATION,
                HysteresisDirectionV1::Rising,
                0.80,
                0.60,
                Duration::from_secs(2),
            )
            .unwrap(),
        ),
        Some(Duration::from_millis(100)),
        Some(TransitionRateLimitV1::new(1, Duration::from_secs(1)).unwrap()),
    )
    .unwrap();
    let mut gate = TransitionStabilityGateV1::new(policy);
    let start = Instant::now();

    let (eligible, permit) = gate.check(&snapshot(&source, 0.90, start), start);
    assert_eq!(eligible.status, TransitionStabilityStatusV1::Eligible);
    assert_eq!(eligible.schema_version, TRANSITION_STABILITY_SCHEMA_V1);
    assert_eq!(eligible.dimension, "capacity");
    gate.record_commit(permit.unwrap(), start).unwrap();

    let high = start + Duration::from_millis(150);
    let (waiting_release, _) = gate.check(&snapshot(&source, 0.90, high), high);
    assert_eq!(
        waiting_release.status,
        TransitionStabilityStatusV1::HysteresisAwaitingRelease
    );

    let released_at = start + Duration::from_millis(200);
    let (released, _) = gate.check(&snapshot(&source, 0.50, released_at), released_at);
    assert_eq!(
        released.status,
        TransitionStabilityStatusV1::HysteresisTriggerNotReached
    );
    assert!(released.hysteresis_armed);

    let retrigger_at = start + Duration::from_millis(300);
    let (rate_limited, _) = gate.check(&snapshot(&source, 0.90, retrigger_at), retrigger_at);
    assert_eq!(
        rate_limited.status,
        TransitionStabilityStatusV1::RateLimited
    );

    let next_window = start + Duration::from_millis(1_050);
    let (eligible_again, permit) = gate.check(&snapshot(&source, 0.90, next_window), next_window);
    assert_eq!(eligible_again.status, TransitionStabilityStatusV1::Eligible);
    assert!(permit.is_some());
}

#[test]
fn facade_only_stale_hysteresis_evidence_never_issues_a_permit() {
    let source = ObservationSource::runtime("public-anti-thrash");
    let policy = TransitionStabilityPolicyV1::new(
        TransitionMechanism::Reinterpret,
        DimensionId::CAPACITY,
        Some(
            HysteresisPolicyV1::new(
                source.clone(),
                ObservationSignalId::UTILIZATION,
                HysteresisDirectionV1::Rising,
                0.80,
                0.60,
                Duration::from_millis(50),
            )
            .unwrap(),
        ),
        None,
        None,
    )
    .unwrap();
    let mut gate = TransitionStabilityGateV1::new(policy);
    let now = Instant::now();
    let old = now.checked_sub(Duration::from_millis(60)).unwrap();
    let (report, permit) = gate.check(&snapshot(&source, 0.95, old), now);
    assert_eq!(
        report.status,
        TransitionStabilityStatusV1::InsufficientEvidence
    );
    assert!(permit.is_none());
}
