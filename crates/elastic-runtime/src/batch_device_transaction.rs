//! Trusted BE14g transaction boundary for declared batch/device placement.
//!
//! Boolean admission and durable traces are explanatory planning evidence only.
//! This module rebinds the exact selected candidate to fresh capacity evidence,
//! performs backend-owned validation immediately before local configuration
//! actuation, and drives VALIDATE → ACT → VERIFY → COMMIT or ROLLBACK.
//!
//! The backend contract is intentionally narrower than orchestration. It does
//! not acquire or renew Hub leases/fencing tokens, dispatch workers, transport
//! data, discover devices, or confer ownership of a placement. Those concerns
//! remain outside this module. A caller may use this boundary only for a local
//! configuration surface it already has authority to control.

use std::fmt;
use std::time::Instant;

use elastic_core::TruthValue;

use crate::batch_device_boolean_admission::{
    BatchDeviceCandidateV1, BatchDeviceCapacitySnapshotV1, BooleanBatchDeviceDecisionTraceV1,
    BooleanBatchDeviceOutcomeV1, BooleanBatchDevicePreplannerV1,
};

/// Local trusted backend boundary for one already-authorized placement surface.
pub trait BatchDevicePlacementBackendV1 {
    /// Re-check backend-specific feasibility without changing visible state.
    fn validate_candidate(
        &mut self,
        candidate: &BatchDeviceCandidateV1,
        fresh_capacity: &BatchDeviceCapacitySnapshotV1,
    ) -> Result<(), String>;

    /// Apply the declared local batch/placement configuration.
    ///
    /// This method must not acquire a Hub lease or transport/dispatch work.
    fn actuate_candidate(&mut self, candidate: &BatchDeviceCandidateV1) -> Result<(), String>;

    /// Verify local post-actuation state and semantic invariants.
    fn verify_candidate(&mut self, candidate: &BatchDeviceCandidateV1) -> Result<(), String>;

    /// Mark the already-verified local configuration authoritative.
    fn commit_candidate(&mut self, candidate: &BatchDeviceCandidateV1) -> Result<(), String>;

    /// Restore the pre-transaction local configuration after partial actuation.
    fn rollback_candidate(
        &mut self,
        candidate: &BatchDeviceCandidateV1,
        reason: &str,
    ) -> Result<(), String>;
}

/// Stable trusted-transaction stage used in fail-closed evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BatchDeviceTransactionStageV1 {
    Bind,
    Validate,
    Act,
    Verify,
    Commit,
}

impl BatchDeviceTransactionStageV1 {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bind => "bind",
            Self::Validate => "validate",
            Self::Act => "act",
            Self::Verify => "verify",
            Self::Commit => "commit",
        }
    }
}

/// Committed local configuration identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommittedBatchDeviceSelectionV1 {
    candidate_id: String,
    placement_id: String,
    batch_size: u32,
    preference_score: u64,
    source_generation: u64,
}

impl CommittedBatchDeviceSelectionV1 {
    #[must_use]
    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }

    #[must_use]
    pub fn placement_id(&self) -> &str {
        &self.placement_id
    }

    #[must_use]
    pub const fn batch_size(&self) -> u32 {
        self.batch_size
    }

    #[must_use]
    pub const fn preference_score(&self) -> u64 {
        self.preference_score
    }

    /// Capacity-evidence generation revalidated immediately before actuation.
    #[must_use]
    pub const fn source_generation(&self) -> u64 {
        self.source_generation
    }
}

/// Failure evidence from the trusted local transaction boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BatchDeviceTransactionFailureV1 {
    stage: BatchDeviceTransactionStageV1,
    reason: String,
    candidate_id: String,
    rollback_attempted: bool,
    backend_rollback_error: Option<String>,
}

impl BatchDeviceTransactionFailureV1 {
    #[must_use]
    pub const fn stage(&self) -> BatchDeviceTransactionStageV1 {
        self.stage
    }

    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }

    #[must_use]
    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }

    #[must_use]
    pub const fn rollback_attempted(&self) -> bool {
        self.rollback_attempted
    }

    #[must_use]
    pub fn backend_rollback_error(&self) -> Option<&str> {
        self.backend_rollback_error.as_deref()
    }
}

impl fmt::Display for BatchDeviceTransactionFailureV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "batch/device transaction failed at {} for {}: {}",
            self.stage.as_str(),
            self.candidate_id,
            self.reason
        )?;
        if let Some(error) = &self.backend_rollback_error {
            write!(f, "; backend rollback also failed: {error}")?;
        }
        Ok(())
    }
}

impl std::error::Error for BatchDeviceTransactionFailureV1 {}

/// Non-actuating reason why a guarded plan cannot enter the trusted backend.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BatchDeviceTransactionBlockV1 {
    InsufficientEvidence,
    NoCandidate,
    TraceMismatch(String),
    SelectedCandidateMissing,
    SelectedCandidateMismatch,
}

/// Result of attempting one Boolean-planned local configuration transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum GuardedBatchDeviceTransactionOutcomeV1 {
    Committed(CommittedBatchDeviceSelectionV1),
    Blocked(BatchDeviceTransactionBlockV1),
}

fn failure_without_rollback(
    candidate: &BatchDeviceCandidateV1,
    stage: BatchDeviceTransactionStageV1,
    reason: impl Into<String>,
) -> BatchDeviceTransactionFailureV1 {
    BatchDeviceTransactionFailureV1 {
        stage,
        reason: reason.into(),
        candidate_id: candidate.candidate_id().to_owned(),
        rollback_attempted: false,
        backend_rollback_error: None,
    }
}

fn fail_after_possible_mutation<B: BatchDevicePlacementBackendV1>(
    backend: &mut B,
    candidate: &BatchDeviceCandidateV1,
    stage: BatchDeviceTransactionStageV1,
    reason: String,
) -> BatchDeviceTransactionFailureV1 {
    let backend_rollback_error = backend.rollback_candidate(candidate, &reason).err();
    BatchDeviceTransactionFailureV1 {
        stage,
        reason,
        candidate_id: candidate.candidate_id().to_owned(),
        rollback_attempted: true,
        backend_rollback_error,
    }
}

fn committed(
    candidate: &BatchDeviceCandidateV1,
    fresh_capacity: &BatchDeviceCapacitySnapshotV1,
) -> CommittedBatchDeviceSelectionV1 {
    CommittedBatchDeviceSelectionV1 {
        candidate_id: candidate.candidate_id().to_owned(),
        placement_id: candidate.placement_id().to_owned(),
        batch_size: candidate.batch_size(),
        preference_score: candidate.preference_score(),
        source_generation: fresh_capacity.source_generation(),
    }
}

/// Execute one exact declared candidate through the trusted local lifecycle.
///
/// This is the explicit non-Boolean reference path used for differential tests.
/// It performs no Boolean candidate pruning/ranking. The exact supplied candidate
/// must still have complete `True` capacity evidence in the fresh snapshot before
/// any backend call, then the backend owns validation and physical/local state.
///
/// # Errors
///
/// Fails closed on fresh-capacity drift or any trusted backend stage.
pub fn execute_unguarded_batch_device_transaction<B: BatchDevicePlacementBackendV1>(
    planner: &BooleanBatchDevicePreplannerV1,
    candidate: &BatchDeviceCandidateV1,
    fresh_capacity: &BatchDeviceCapacitySnapshotV1,
    now: Instant,
    backend: &mut B,
) -> Result<CommittedBatchDeviceSelectionV1, BatchDeviceTransactionFailureV1> {
    let Some(declared) = planner.candidate_by_id(candidate.candidate_id()) else {
        return Err(failure_without_rollback(
            candidate,
            BatchDeviceTransactionStageV1::Bind,
            "candidate is not declared by the bound placement policy",
        ));
    };
    if declared != candidate {
        return Err(failure_without_rollback(
            candidate,
            BatchDeviceTransactionStageV1::Bind,
            "candidate fields differ from the bound placement policy",
        ));
    }

    match planner.evaluate_candidate_truth(candidate, fresh_capacity, now) {
        TruthValue::True => {}
        TruthValue::False => {
            return Err(failure_without_rollback(
                candidate,
                BatchDeviceTransactionStageV1::Bind,
                "fresh capacity conclusively rejects selected candidate",
            ));
        }
        TruthValue::Unknown => {
            return Err(failure_without_rollback(
                candidate,
                BatchDeviceTransactionStageV1::Bind,
                "fresh capacity for selected candidate is unknown",
            ));
        }
    }

    if let Err(error) = backend.validate_candidate(candidate, fresh_capacity) {
        return Err(failure_without_rollback(
            candidate,
            BatchDeviceTransactionStageV1::Validate,
            format!("trusted validation failed: {error}"),
        ));
    }

    if let Err(error) = backend.actuate_candidate(candidate) {
        return Err(fail_after_possible_mutation(
            backend,
            candidate,
            BatchDeviceTransactionStageV1::Act,
            format!("backend actuation failed: {error}"),
        ));
    }

    if let Err(error) = backend.verify_candidate(candidate) {
        return Err(fail_after_possible_mutation(
            backend,
            candidate,
            BatchDeviceTransactionStageV1::Verify,
            format!("backend verification failed: {error}"),
        ));
    }

    if let Err(error) = backend.commit_candidate(candidate) {
        return Err(fail_after_possible_mutation(
            backend,
            candidate,
            BatchDeviceTransactionStageV1::Commit,
            format!("backend commit failed: {error}"),
        ));
    }

    Ok(committed(candidate, fresh_capacity))
}

/// Enter the trusted local transaction only for a complete Boolean selection
/// whose durable explanatory trace exactly matches the fresh planning context.
///
/// `Unknown` and no-candidate outcomes stop before backend validation. A trace
/// mismatch, source-generation drift, candidate-policy drift, or selected-field
/// mismatch also stops before any backend method. A matching trace never grants
/// actuation authority by itself; this function merely permits entry into the
/// backend-owned trusted lifecycle.
pub fn execute_guarded_batch_device_transaction<B: BatchDevicePlacementBackendV1>(
    planner: &BooleanBatchDevicePreplannerV1,
    trace: &BooleanBatchDeviceDecisionTraceV1,
    fresh_capacity: &BatchDeviceCapacitySnapshotV1,
    now: Instant,
    backend: &mut B,
) -> Result<GuardedBatchDeviceTransactionOutcomeV1, BatchDeviceTransactionFailureV1> {
    if let Err(error) = trace.validate_explanatory_context(planner, fresh_capacity, now) {
        return Ok(GuardedBatchDeviceTransactionOutcomeV1::Blocked(
            BatchDeviceTransactionBlockV1::TraceMismatch(error),
        ));
    }

    let (candidate_id, placement_id, batch_size, preference_score) = match trace.outcome() {
        BooleanBatchDeviceOutcomeV1::InsufficientEvidence { .. } => {
            return Ok(GuardedBatchDeviceTransactionOutcomeV1::Blocked(
                BatchDeviceTransactionBlockV1::InsufficientEvidence,
            ));
        }
        BooleanBatchDeviceOutcomeV1::NoCandidate => {
            return Ok(GuardedBatchDeviceTransactionOutcomeV1::Blocked(
                BatchDeviceTransactionBlockV1::NoCandidate,
            ));
        }
        BooleanBatchDeviceOutcomeV1::Selected {
            candidate_id,
            placement_id,
            batch_size,
            preference_score,
        } => (candidate_id, placement_id, *batch_size, *preference_score),
    };

    let Some(candidate) = planner.candidate_by_id(candidate_id) else {
        return Ok(GuardedBatchDeviceTransactionOutcomeV1::Blocked(
            BatchDeviceTransactionBlockV1::SelectedCandidateMissing,
        ));
    };
    if candidate.placement_id() != placement_id
        || candidate.batch_size() != batch_size
        || candidate.preference_score() != preference_score
    {
        return Ok(GuardedBatchDeviceTransactionOutcomeV1::Blocked(
            BatchDeviceTransactionBlockV1::SelectedCandidateMismatch,
        ));
    }

    execute_unguarded_batch_device_transaction(planner, candidate, fresh_capacity, now, backend)
        .map(GuardedBatchDeviceTransactionOutcomeV1::Committed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use crate::batch_device_boolean_admission::{
        BatchDeviceCapacitySampleV1, BATCH_DEVICE_CAPACITY_SOURCE_UNIT,
    };

    #[derive(Default)]
    struct TestBackend {
        calls: Vec<&'static str>,
        applied: Option<(String, u32)>,
        committed: Option<(String, u32)>,
        fail_validate: bool,
        fail_act: bool,
        fail_verify: bool,
        fail_commit: bool,
        fail_rollback: bool,
    }

    impl BatchDevicePlacementBackendV1 for TestBackend {
        fn validate_candidate(
            &mut self,
            _candidate: &BatchDeviceCandidateV1,
            _fresh_capacity: &BatchDeviceCapacitySnapshotV1,
        ) -> Result<(), String> {
            self.calls.push("validate");
            if self.fail_validate {
                Err("injected validation failure".into())
            } else {
                Ok(())
            }
        }

        fn actuate_candidate(&mut self, candidate: &BatchDeviceCandidateV1) -> Result<(), String> {
            self.calls.push("act");
            self.applied = Some((candidate.placement_id().to_owned(), candidate.batch_size()));
            if self.fail_act {
                Err("injected actuation failure".into())
            } else {
                Ok(())
            }
        }

        fn verify_candidate(&mut self, candidate: &BatchDeviceCandidateV1) -> Result<(), String> {
            self.calls.push("verify");
            if self.fail_verify {
                Err("injected verification failure".into())
            } else if self.applied
                != Some((candidate.placement_id().to_owned(), candidate.batch_size()))
            {
                Err("applied state does not match candidate".into())
            } else {
                Ok(())
            }
        }

        fn commit_candidate(&mut self, candidate: &BatchDeviceCandidateV1) -> Result<(), String> {
            self.calls.push("commit");
            if self.fail_commit {
                Err("injected commit failure".into())
            } else {
                self.committed = self.applied.clone();
                if self.committed
                    != Some((candidate.placement_id().to_owned(), candidate.batch_size()))
                {
                    return Err("committed state does not match candidate".into());
                }
                Ok(())
            }
        }

        fn rollback_candidate(
            &mut self,
            _candidate: &BatchDeviceCandidateV1,
            _reason: &str,
        ) -> Result<(), String> {
            self.calls.push("rollback");
            self.applied = None;
            self.committed = None;
            if self.fail_rollback {
                Err("injected rollback failure".into())
            } else {
                Ok(())
            }
        }
    }

    fn candidate(id: &str, placement: &str, batch: u32, score: u64) -> BatchDeviceCandidateV1 {
        BatchDeviceCandidateV1::new(id, placement, batch, score).unwrap()
    }

    fn planner() -> BooleanBatchDevicePreplannerV1 {
        BooleanBatchDevicePreplannerV1::new(vec![
            candidate("preferred", "device-a", 8, 1),
            candidate("survivor", "device-b", 4, 10),
        ])
        .unwrap()
    }

    fn capacity(
        generation: u64,
        now: Instant,
        a: Option<f64>,
        b: Option<f64>,
    ) -> BatchDeviceCapacitySnapshotV1 {
        let mut samples = Vec::new();
        if let Some(value) = a {
            samples.push(BatchDeviceCapacitySampleV1::valid("device-a", value, now).unwrap());
        }
        if let Some(value) = b {
            samples.push(BatchDeviceCapacitySampleV1::valid("device-b", value, now).unwrap());
        }
        BatchDeviceCapacitySnapshotV1::new_with_generation(
            "be14g-transaction-test",
            BATCH_DEVICE_CAPACITY_SOURCE_UNIT,
            generation,
            samples,
        )
        .unwrap()
    }

    #[test]
    fn guarded_true_selection_executes_validate_act_verify_commit() {
        let planner = planner();
        let now = Instant::now();
        let snapshot = capacity(7, now, Some(2.0), Some(8.0));
        let trace = planner.decision_trace(&snapshot, now).unwrap();
        let mut backend = TestBackend::default();

        let result = execute_guarded_batch_device_transaction(
            &planner,
            &trace,
            &snapshot,
            now,
            &mut backend,
        )
        .unwrap();

        let GuardedBatchDeviceTransactionOutcomeV1::Committed(committed) = result else {
            panic!("fixed fixture must commit");
        };
        assert_eq!(committed.candidate_id(), "survivor");
        assert_eq!(committed.placement_id(), "device-b");
        assert_eq!(committed.batch_size(), 4);
        assert_eq!(committed.source_generation(), 7);
        assert_eq!(backend.calls, ["validate", "act", "verify", "commit"]);
        assert_eq!(backend.committed, Some(("device-b".into(), 4)));
    }

    #[test]
    fn unknown_and_no_candidate_stop_before_backend() {
        let planner = planner();
        let now = Instant::now();

        let unknown = capacity(1, now, None, Some(8.0));
        let unknown_trace = planner.decision_trace(&unknown, now).unwrap();
        let mut backend = TestBackend::default();
        assert_eq!(
            execute_guarded_batch_device_transaction(
                &planner,
                &unknown_trace,
                &unknown,
                now,
                &mut backend,
            )
            .unwrap(),
            GuardedBatchDeviceTransactionOutcomeV1::Blocked(
                BatchDeviceTransactionBlockV1::InsufficientEvidence
            )
        );
        assert!(backend.calls.is_empty());

        let none = capacity(2, now, Some(1.0), Some(1.0));
        let none_trace = planner.decision_trace(&none, now).unwrap();
        assert_eq!(
            execute_guarded_batch_device_transaction(
                &planner,
                &none_trace,
                &none,
                now,
                &mut backend,
            )
            .unwrap(),
            GuardedBatchDeviceTransactionOutcomeV1::Blocked(
                BatchDeviceTransactionBlockV1::NoCandidate
            )
        );
        assert!(backend.calls.is_empty());
    }

    #[test]
    fn source_generation_and_freshness_drift_block_before_backend() {
        let planner = planner();
        let planned_at = Instant::now();
        let snapshot = capacity(10, planned_at, Some(1.0), Some(8.0));
        let trace = planner.decision_trace(&snapshot, planned_at).unwrap();
        let mut backend = TestBackend::default();

        let generation_drift = capacity(11, planned_at, Some(1.0), Some(8.0));
        assert!(matches!(
            execute_guarded_batch_device_transaction(
                &planner,
                &trace,
                &generation_drift,
                planned_at,
                &mut backend,
            )
            .unwrap(),
            GuardedBatchDeviceTransactionOutcomeV1::Blocked(
                BatchDeviceTransactionBlockV1::TraceMismatch(_)
            )
        ));
        assert!(backend.calls.is_empty());

        let later = planned_at.checked_add(Duration::from_secs(2)).unwrap();
        assert!(matches!(
            execute_guarded_batch_device_transaction(
                &planner,
                &trace,
                &snapshot,
                later,
                &mut backend,
            )
            .unwrap(),
            GuardedBatchDeviceTransactionOutcomeV1::Blocked(
                BatchDeviceTransactionBlockV1::TraceMismatch(_)
            )
        ));
        assert!(backend.calls.is_empty());
    }

    #[test]
    fn validation_failure_never_actuates_or_rolls_back() {
        let planner = planner();
        let now = Instant::now();
        let snapshot = capacity(1, now, Some(1.0), Some(8.0));
        let trace = planner.decision_trace(&snapshot, now).unwrap();
        let mut backend = TestBackend {
            fail_validate: true,
            ..Default::default()
        };
        let error = execute_guarded_batch_device_transaction(
            &planner,
            &trace,
            &snapshot,
            now,
            &mut backend,
        )
        .unwrap_err();
        assert_eq!(error.stage(), BatchDeviceTransactionStageV1::Validate);
        assert!(!error.rollback_attempted());
        assert_eq!(backend.calls, ["validate"]);
        assert!(backend.applied.is_none());
    }

    #[test]
    fn verification_failure_rolls_back_and_retains_rollback_failure() {
        let planner = planner();
        let now = Instant::now();
        let snapshot = capacity(1, now, Some(1.0), Some(8.0));
        let trace = planner.decision_trace(&snapshot, now).unwrap();
        let mut backend = TestBackend {
            fail_verify: true,
            fail_rollback: true,
            ..Default::default()
        };
        let error = execute_guarded_batch_device_transaction(
            &planner,
            &trace,
            &snapshot,
            now,
            &mut backend,
        )
        .unwrap_err();
        assert_eq!(error.stage(), BatchDeviceTransactionStageV1::Verify);
        assert!(error.rollback_attempted());
        assert_eq!(
            error.backend_rollback_error(),
            Some("injected rollback failure")
        );
        assert_eq!(backend.calls, ["validate", "act", "verify", "rollback"]);
        assert!(backend.applied.is_none());
        assert!(backend.committed.is_none());
    }

    #[test]
    fn unguarded_reference_rejects_undeclared_or_modified_candidate_before_backend() {
        let planner = planner();
        let now = Instant::now();
        let snapshot = capacity(4, now, Some(8.0), Some(8.0));
        let mut backend = TestBackend::default();

        let undeclared = candidate("foreign", "device-c", 1, 0);
        let error = execute_unguarded_batch_device_transaction(
            &planner,
            &undeclared,
            &snapshot,
            now,
            &mut backend,
        )
        .unwrap_err();
        assert_eq!(error.stage(), BatchDeviceTransactionStageV1::Bind);
        assert!(backend.calls.is_empty());

        let modified = candidate("survivor", "device-b", 1, 10);
        let error = execute_unguarded_batch_device_transaction(
            &planner,
            &modified,
            &snapshot,
            now,
            &mut backend,
        )
        .unwrap_err();
        assert_eq!(error.stage(), BatchDeviceTransactionStageV1::Bind);
        assert!(backend.calls.is_empty());
    }

    #[test]
    fn guarded_and_unguarded_reference_commit_identical_local_state() {
        let planner = planner();
        let now = Instant::now();
        let snapshot = capacity(4, now, Some(1.0), Some(8.0));
        let trace = planner.decision_trace(&snapshot, now).unwrap();

        let mut guarded_backend = TestBackend::default();
        let guarded = execute_guarded_batch_device_transaction(
            &planner,
            &trace,
            &snapshot,
            now,
            &mut guarded_backend,
        )
        .unwrap();
        let GuardedBatchDeviceTransactionOutcomeV1::Committed(guarded) = guarded else {
            panic!("guarded fixture must commit");
        };

        let selected = planner.candidate_by_id("survivor").unwrap();
        let mut baseline_backend = TestBackend::default();
        let baseline = execute_unguarded_batch_device_transaction(
            &planner,
            selected,
            &snapshot,
            now,
            &mut baseline_backend,
        )
        .unwrap();

        assert_eq!(guarded, baseline);
        assert_eq!(guarded_backend.committed, baseline_backend.committed);
        assert_eq!(
            guarded_backend.calls,
            ["validate", "act", "verify", "commit"]
        );
        assert_eq!(guarded_backend.calls, baseline_backend.calls);
    }
}
