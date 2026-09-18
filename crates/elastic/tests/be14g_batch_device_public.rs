use std::time::Instant;

use elastic::{
    BatchDeviceCandidateV1, BatchDeviceCapacitySampleV1, BatchDeviceCapacitySnapshotV1,
    BooleanBatchDeviceOutcomeV1, BooleanBatchDevicePreplannerV1, BATCH_DEVICE_CAPACITY_SOURCE_UNIT,
};

#[test]
fn public_facade_prunes_capacity_before_declared_preference_ranking() {
    let planner = BooleanBatchDevicePreplannerV1::new(vec![
        BatchDeviceCandidateV1::new("preferred", "placement-a", 8, 1).unwrap(),
        BatchDeviceCandidateV1::new("survivor", "placement-b", 4, 10).unwrap(),
    ])
    .unwrap();
    let now = Instant::now();
    let snapshot = BatchDeviceCapacitySnapshotV1::new(
        "public-test-provider",
        BATCH_DEVICE_CAPACITY_SOURCE_UNIT,
        vec![
            BatchDeviceCapacitySampleV1::valid("placement-a", 2.0, now).unwrap(),
            BatchDeviceCapacitySampleV1::valid("placement-b", 8.0, now).unwrap(),
        ],
    )
    .unwrap();

    let report = planner.screen(&snapshot, now);
    assert!(matches!(
        report.outcome,
        BooleanBatchDeviceOutcomeV1::Selected {
            ref candidate_id,
            ref placement_id,
            batch_size: 4,
            preference_score: 10,
        } if candidate_id == "survivor" && placement_id == "placement-b"
    ));
}

#[test]
fn public_facade_missing_placement_capacity_is_unknown_not_false() {
    let planner = BooleanBatchDevicePreplannerV1::new(vec![BatchDeviceCandidateV1::new(
        "candidate-a",
        "placement-a",
        1,
        1,
    )
    .unwrap()])
    .unwrap();
    let now = Instant::now();
    let snapshot = BatchDeviceCapacitySnapshotV1::new(
        "public-test-provider",
        BATCH_DEVICE_CAPACITY_SOURCE_UNIT,
        vec![],
    )
    .unwrap();

    let report = planner.screen(&snapshot, now);
    assert!(matches!(
        report.outcome,
        BooleanBatchDeviceOutcomeV1::InsufficientEvidence { .. }
    ));
    assert_eq!(report.candidates[0].truth, "unknown");
}
