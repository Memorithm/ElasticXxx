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

#[test]
fn public_facade_persists_and_rechecks_decision_only_trace() {
    let planner = BooleanBatchDevicePreplannerV1::new(vec![BatchDeviceCandidateV1::new(
        "candidate-a",
        "placement-a",
        2,
        1,
    )
    .unwrap()])
    .unwrap();
    let now = Instant::now();
    let snapshot = BatchDeviceCapacitySnapshotV1::new_with_generation(
        "public-test-provider",
        BATCH_DEVICE_CAPACITY_SOURCE_UNIT,
        9,
        vec![BatchDeviceCapacitySampleV1::valid("placement-a", 4.0, now).unwrap()],
    )
    .unwrap();

    let trace = planner.decision_trace(&snapshot, now).unwrap();
    let encoded = trace.to_bounded_json().unwrap();
    let decoded =
        elastic::BooleanBatchDeviceDecisionTraceV1::from_bounded_json(encoded.as_bytes()).unwrap();
    decoded
        .validate_explanatory_context(&planner, &snapshot, now)
        .unwrap();

    let changed = BatchDeviceCapacitySnapshotV1::new_with_generation(
        "public-test-provider",
        BATCH_DEVICE_CAPACITY_SOURCE_UNIT,
        10,
        vec![BatchDeviceCapacitySampleV1::valid("placement-a", 4.0, now).unwrap()],
    )
    .unwrap();
    assert!(decoded
        .validate_explanatory_context(&planner, &changed, now)
        .is_err());
}

#[derive(Default)]
struct PublicBackend {
    applied: Option<(String, u32)>,
    committed: Option<(String, u32)>,
}

impl elastic::BatchDevicePlacementBackendV1 for PublicBackend {
    fn validate_candidate(
        &mut self,
        _candidate: &BatchDeviceCandidateV1,
        _fresh_capacity: &BatchDeviceCapacitySnapshotV1,
    ) -> Result<(), String> {
        Ok(())
    }

    fn actuate_candidate(&mut self, candidate: &BatchDeviceCandidateV1) -> Result<(), String> {
        self.applied = Some((candidate.placement_id().to_owned(), candidate.batch_size()));
        Ok(())
    }

    fn verify_candidate(&mut self, candidate: &BatchDeviceCandidateV1) -> Result<(), String> {
        if self.applied == Some((candidate.placement_id().to_owned(), candidate.batch_size())) {
            Ok(())
        } else {
            Err("public test backend observed unexpected applied state".into())
        }
    }

    fn commit_candidate(&mut self, _candidate: &BatchDeviceCandidateV1) -> Result<(), String> {
        self.committed = self.applied.clone();
        Ok(())
    }

    fn rollback_candidate(
        &mut self,
        _candidate: &BatchDeviceCandidateV1,
        _reason: &str,
    ) -> Result<(), String> {
        self.applied = None;
        self.committed = None;
        Ok(())
    }
}

#[test]
fn public_facade_guarded_transaction_rebinds_trace_before_local_actuation() {
    let planner = BooleanBatchDevicePreplannerV1::new(vec![
        BatchDeviceCandidateV1::new("preferred", "placement-a", 8, 1).unwrap(),
        BatchDeviceCandidateV1::new("survivor", "placement-b", 4, 10).unwrap(),
    ])
    .unwrap();
    let now = Instant::now();
    let snapshot = BatchDeviceCapacitySnapshotV1::new_with_generation(
        "public-transaction-provider",
        BATCH_DEVICE_CAPACITY_SOURCE_UNIT,
        3,
        vec![
            BatchDeviceCapacitySampleV1::valid("placement-a", 2.0, now).unwrap(),
            BatchDeviceCapacitySampleV1::valid("placement-b", 8.0, now).unwrap(),
        ],
    )
    .unwrap();
    let trace = planner.decision_trace(&snapshot, now).unwrap();
    let mut backend = PublicBackend::default();

    let outcome = elastic::execute_guarded_batch_device_transaction(
        &planner,
        &trace,
        &snapshot,
        now,
        &mut backend,
    )
    .unwrap();
    let elastic::GuardedBatchDeviceTransactionOutcomeV1::Committed(committed) = outcome else {
        panic!("public fixed fixture must commit");
    };
    assert_eq!(committed.candidate_id(), "survivor");
    assert_eq!(backend.committed, Some(("placement-b".into(), 4)));

    let changed = BatchDeviceCapacitySnapshotV1::new_with_generation(
        "public-transaction-provider",
        BATCH_DEVICE_CAPACITY_SOURCE_UNIT,
        4,
        vec![
            BatchDeviceCapacitySampleV1::valid("placement-a", 2.0, now).unwrap(),
            BatchDeviceCapacitySampleV1::valid("placement-b", 8.0, now).unwrap(),
        ],
    )
    .unwrap();
    let before = backend.committed.clone();
    let blocked = elastic::execute_guarded_batch_device_transaction(
        &planner,
        &trace,
        &changed,
        now,
        &mut backend,
    )
    .unwrap();
    assert!(matches!(
        blocked,
        elastic::GuardedBatchDeviceTransactionOutcomeV1::Blocked(
            elastic::BatchDeviceTransactionBlockV1::TraceMismatch(_)
        )
    ));
    assert_eq!(backend.committed, before);
}
