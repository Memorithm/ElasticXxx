use elastic_kv::{
    authorize_allocator, authorize_partition_access, load_stage_b_preregistration,
    run_stage_b_dry_run, CandidateAdmission, StageBError, StageBPartition,
    STAGE_B_FIXED_CANDIDATE_SLATE, STAGE_B_MEASUREMENT_SCHEMA,
};

const FROZEN_MANIFEST: &str =
    include_str!("../../../research/elastic-bit-allocation-stage-b-smollm2-v1.json");

#[test]
fn frozen_stage_b_manifest_dry_run_is_regression_stable() {
    let report = run_stage_b_dry_run(FROZEN_MANIFEST).expect("frozen Stage B dry-run must succeed");
    assert_eq!(report.schema, STAGE_B_MEASUREMENT_SCHEMA);
    assert_eq!(report.partition, StageBPartition::Development);
    assert_eq!(report.candidates.len(), STAGE_B_FIXED_CANDIDATE_SLATE.len());
    assert!(matches!(
        report.candidates[0].admission,
        CandidateAdmission::PlannedDenseReference
    ));
    assert!(report.candidates.iter().skip(1).all(|candidate| matches!(
        candidate.admission,
        CandidateAdmission::BlockedMissingBackendCapability { .. }
    )));

    let json = report.canonical_json();
    assert!(json.contains("\"schema\": \"elastic-bit-allocation-stage-b-measurement-v1\""));
    assert!(json.contains("\"fabricated_numeric_metrics\": false"));
    assert!(json.contains("\"final_test_execution_authorized\": false"));
    assert!(json.contains("\"elastic_allocator_or_search_authorized\": false"));
    assert!(json.contains("\"status\": \"not_executed\""));
    assert!(json.contains("\"status\": \"blocked\""));
    assert!(!json.contains("\"value\": 0"));
}

#[test]
fn stage_b_gates_remain_fail_closed() {
    let preregistration =
        load_stage_b_preregistration(FROZEN_MANIFEST).expect("manifest must validate");
    assert_eq!(
        authorize_partition_access(&preregistration, StageBPartition::FinalTest),
        Err(StageBError::FinalTestLocked)
    );
    assert_eq!(
        authorize_allocator(&preregistration),
        Err(StageBError::AllocatorUnauthorized)
    );
}
