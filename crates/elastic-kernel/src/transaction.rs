//! Trusted BE14f transaction boundary for kernel realization switching.
//!
//! Boolean admission and numerical planning are recommendations only. This
//! module re-binds the exact selected candidate to a fresh capability snapshot,
//! performs backend-owned validation immediately before activation, and drives
//! VALIDATE → ACT → VERIFY → COMMIT or ROLLBACK.

use std::fmt;

use crate::boolean_admission::{BooleanKernelDecisionTraceV1, BooleanKernelPlanOutcomeV1};
use crate::candidate::KernelCandidate;
use crate::capability::CapabilitySnapshot;
use crate::lifecycle::{
    CommittedRealization, RealizationProposal, RolledBackRealization, StageAttestations,
};
use crate::planner::{SelectionOutcome, SelectionRecord};

/// Physical/backend boundary for one concrete kernel realization.
pub trait KernelRealizationBackendV1 {
    /// Re-check action-time feasibility without changing visible state.
    fn validate_candidate(
        &mut self,
        candidate: &KernelCandidate,
        fresh_capabilities: &CapabilitySnapshot,
    ) -> Result<(), String>;

    /// Activate/compile the selected realization.
    fn activate_candidate(&mut self, candidate: &KernelCandidate) -> Result<(), String>;

    /// Verify behavior and backend-specific invariants after activation.
    fn verify_candidate(&mut self, candidate: &KernelCandidate) -> Result<(), String>;

    /// Make the verified realization authoritative.
    fn commit_candidate(&mut self, candidate: &KernelCandidate) -> Result<(), String>;

    /// Restore the pre-transaction state after a partial action.
    fn rollback_candidate(
        &mut self,
        candidate: &KernelCandidate,
        reason: &str,
    ) -> Result<(), String>;
}

/// Canonical transaction stage used in fail-closed error evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum KernelTransactionStageV1 {
    Bind,
    Validate,
    Act,
    Verify,
    Commit,
}

impl KernelTransactionStageV1 {
    /// Stable diagnostic token.
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

/// Failure of a trusted kernel-realization transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KernelTransactionFailureV1 {
    stage: KernelTransactionStageV1,
    reason: String,
    rollback: RolledBackRealization,
    backend_rollback_error: Option<String>,
}

impl KernelTransactionFailureV1 {
    #[must_use]
    pub const fn stage(&self) -> KernelTransactionStageV1 {
        self.stage
    }

    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }

    #[must_use]
    pub const fn rollback(&self) -> &RolledBackRealization {
        &self.rollback
    }

    #[must_use]
    pub fn backend_rollback_error(&self) -> Option<&str> {
        self.backend_rollback_error.as_deref()
    }
}

impl fmt::Display for KernelTransactionFailureV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "kernel transaction failed at {}: {}",
            self.stage.as_str(),
            self.reason
        )?;
        if let Some(error) = &self.backend_rollback_error {
            write!(f, "; backend rollback also failed: {error}")?;
        }
        Ok(())
    }
}

impl std::error::Error for KernelTransactionFailureV1 {}

/// Non-actuating reason why a Boolean planning result cannot enter actuation.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum KernelTransactionBlockV1 {
    BooleanInsufficientEvidence,
    NoCandidate,
    PlannerInsufficientEvidence,
    PlannerUnsupported,
    TraceMismatch(String),
    SelectedCandidateMissing,
}

/// Result of trying to execute one Boolean-planned kernel realization.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum GuardedKernelTransactionOutcomeV1 {
    Committed(CommittedRealization),
    Blocked(KernelTransactionBlockV1),
}

fn binding_failure(
    candidate: &KernelCandidate,
    record: &SelectionRecord,
    reason: impl Into<String>,
) -> KernelTransactionFailureV1 {
    let reason = reason.into();
    let rollback = RealizationProposal::start(candidate.clone(), record.fingerprint())
        .rollback(reason.clone());
    KernelTransactionFailureV1 {
        stage: KernelTransactionStageV1::Bind,
        reason,
        rollback,
        backend_rollback_error: None,
    }
}

fn fail_after_backend<B: KernelRealizationBackendV1>(
    backend: &mut B,
    candidate: &KernelCandidate,
    stage: KernelTransactionStageV1,
    reason: String,
    rollback: RolledBackRealization,
) -> KernelTransactionFailureV1 {
    let backend_rollback_error = backend.rollback_candidate(candidate, &reason).err();
    KernelTransactionFailureV1 {
        stage,
        reason,
        rollback,
        backend_rollback_error,
    }
}

/// Execute one selected candidate through the trusted lifecycle.
///
/// The selection is rebound to the exact candidate and a fresh capability
/// fingerprint before any backend method is called. Capability drift therefore
/// invalidates the old selection rather than silently actuating it.
///
/// # Errors
///
/// Fails closed on identity/capability drift or any trusted backend stage.
pub fn execute_kernel_transaction<B: KernelRealizationBackendV1>(
    record: &SelectionRecord,
    candidate: &KernelCandidate,
    fresh_capabilities: &CapabilitySnapshot,
    backend: &mut B,
) -> Result<CommittedRealization, KernelTransactionFailureV1> {
    if record.logical_resource_id() != candidate.logical_resource_id() {
        return Err(binding_failure(
            candidate,
            record,
            "logical resource mismatch",
        ));
    }
    if record.selected_realization() != candidate.realization()
        || record.selected_schema_version() != candidate.schema_version()
        || record.selected_contract() != candidate.contract()
    {
        return Err(binding_failure(
            candidate,
            record,
            "selected realization identity/schema/contract mismatch",
        ));
    }
    if record.capability_fingerprint() != fresh_capabilities.fingerprint() {
        return Err(binding_failure(
            candidate,
            record,
            "fresh capability fingerprint differs from planning snapshot",
        ));
    }
    if let Err(rejection) = candidate.requirements().check_against(fresh_capabilities) {
        return Err(binding_failure(
            candidate,
            record,
            format!("fresh capabilities reject selected candidate: {rejection}"),
        ));
    }

    let proposed = RealizationProposal::start(candidate.clone(), record.fingerprint());
    if let Err(error) = backend.validate_candidate(candidate, fresh_capabilities) {
        let reason = format!("trusted validation failed: {error}");
        return Err(KernelTransactionFailureV1 {
            stage: KernelTransactionStageV1::Validate,
            reason: reason.clone(),
            rollback: proposed.rollback(reason),
            backend_rollback_error: None,
        });
    }
    let validated = proposed
        .validate(StageAttestations::none().attesting_validation())
        .expect("validation attestation was supplied");

    if let Err(error) = backend.activate_candidate(candidate) {
        let reason = format!("backend activation failed: {error}");
        let rollback = validated.rollback(reason.clone());
        return Err(fail_after_backend(
            backend,
            candidate,
            KernelTransactionStageV1::Act,
            reason,
            rollback,
        ));
    }
    let activated = validated
        .activate(StageAttestations::none().attesting_activation())
        .expect("activation attestation was supplied");

    if let Err(error) = backend.verify_candidate(candidate) {
        let reason = format!("backend verification failed: {error}");
        let rollback = activated.rollback(reason.clone());
        return Err(fail_after_backend(
            backend,
            candidate,
            KernelTransactionStageV1::Verify,
            reason,
            rollback,
        ));
    }
    let verified = activated
        .verify(StageAttestations::none().attesting_verification())
        .expect("verification attestation was supplied");

    if let Err(error) = backend.commit_candidate(candidate) {
        let reason = format!("backend commit failed: {error}");
        let rollback = verified.rollback(reason.clone());
        return Err(fail_after_backend(
            backend,
            candidate,
            KernelTransactionStageV1::Commit,
            reason,
            rollback,
        ));
    }

    Ok(verified.commit())
}

/// Enter the trusted transaction only for a fully grounded Boolean plan whose
/// durable explanatory trace matches the exact selected record.
///
/// `False`, `Unknown`, no-candidate, insufficient-evidence and unsupported
/// planning outcomes return `Blocked` without invoking backend methods.
pub fn execute_guarded_kernel_transaction<B: KernelRealizationBackendV1>(
    outcome: &BooleanKernelPlanOutcomeV1,
    trace: &BooleanKernelDecisionTraceV1,
    candidates: &[KernelCandidate],
    fresh_capabilities: &CapabilitySnapshot,
    backend: &mut B,
) -> Result<GuardedKernelTransactionOutcomeV1, KernelTransactionFailureV1> {
    let record = match outcome {
        BooleanKernelPlanOutcomeV1::InsufficientEvidence { .. } => {
            return Ok(GuardedKernelTransactionOutcomeV1::Blocked(
                KernelTransactionBlockV1::BooleanInsufficientEvidence,
            ));
        }
        BooleanKernelPlanOutcomeV1::Planned {
            planner_outcome: SelectionOutcome::NoCandidate { .. },
            ..
        } => {
            return Ok(GuardedKernelTransactionOutcomeV1::Blocked(
                KernelTransactionBlockV1::NoCandidate,
            ));
        }
        BooleanKernelPlanOutcomeV1::Planned {
            planner_outcome: SelectionOutcome::InsufficientEvidence { .. },
            ..
        } => {
            return Ok(GuardedKernelTransactionOutcomeV1::Blocked(
                KernelTransactionBlockV1::PlannerInsufficientEvidence,
            ));
        }
        BooleanKernelPlanOutcomeV1::Planned {
            planner_outcome: SelectionOutcome::Unsupported { .. },
            ..
        } => {
            return Ok(GuardedKernelTransactionOutcomeV1::Blocked(
                KernelTransactionBlockV1::PlannerUnsupported,
            ));
        }
        BooleanKernelPlanOutcomeV1::Planned {
            planner_outcome: SelectionOutcome::Selected(record),
            ..
        } => record.as_ref(),
    };

    if trace.planner_outcome() != "selected"
        || trace.selected_realization() != Some(record.selected_realization().as_str())
        || trace.selection_fingerprint() != Some(record.fingerprint())
    {
        return Ok(GuardedKernelTransactionOutcomeV1::Blocked(
            KernelTransactionBlockV1::TraceMismatch(
                "durable trace does not match selected planner record".into(),
            ),
        ));
    }

    let mut matching = candidates
        .iter()
        .filter(|candidate| candidate.realization() == record.selected_realization());
    let Some(candidate) = matching.next() else {
        return Ok(GuardedKernelTransactionOutcomeV1::Blocked(
            KernelTransactionBlockV1::SelectedCandidateMissing,
        ));
    };
    if matching.next().is_some() {
        return Ok(GuardedKernelTransactionOutcomeV1::Blocked(
            KernelTransactionBlockV1::SelectedCandidateMissing,
        ));
    }

    execute_kernel_transaction(record, candidate, fresh_capabilities, backend)
        .map(GuardedKernelTransactionOutcomeV1::Committed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    use elastic_core::{BuiltinObjective, ContractId, LogicalResourceId, ObjectiveId};
    use elastic_eir::Fingerprint;

    use crate::boolean_admission::plan_with_boolean_admission_traced;
    use crate::candidate::{
        Evidence, EvidenceUnit, ObjectiveEvidence, RealizationIdentity, StaticQuantity,
    };
    use crate::capability::{BindingLimits, FeatureSupport, SubgroupSupport, WorkgroupLimits};
    use crate::planner::{plan, SelectionPolicy};
    use crate::requirements::{FeatureRequirement, KernelRequirements};

    #[derive(Default)]
    struct TestBackend {
        validate_count: usize,
        activate_count: usize,
        verify_count: usize,
        commit_count: usize,
        rollback_count: usize,
        fail_verify: bool,
    }

    impl KernelRealizationBackendV1 for TestBackend {
        fn validate_candidate(
            &mut self,
            _candidate: &KernelCandidate,
            _fresh_capabilities: &CapabilitySnapshot,
        ) -> Result<(), String> {
            self.validate_count += 1;
            Ok(())
        }

        fn activate_candidate(&mut self, _candidate: &KernelCandidate) -> Result<(), String> {
            self.activate_count += 1;
            Ok(())
        }

        fn verify_candidate(&mut self, _candidate: &KernelCandidate) -> Result<(), String> {
            self.verify_count += 1;
            if self.fail_verify {
                Err("injected verification failure".into())
            } else {
                Ok(())
            }
        }

        fn commit_candidate(&mut self, _candidate: &KernelCandidate) -> Result<(), String> {
            self.commit_count += 1;
            Ok(())
        }

        fn rollback_candidate(
            &mut self,
            _candidate: &KernelCandidate,
            _reason: &str,
        ) -> Result<(), String> {
            self.rollback_count += 1;
            Ok(())
        }
    }

    fn resource() -> LogicalResourceId {
        LogicalResourceId::new("be14f-kernel-test").unwrap()
    }

    fn contract() -> ContractId {
        ContractId::new("be14f-kernel-contract-v1").unwrap()
    }

    fn latency() -> ObjectiveId {
        ObjectiveId::builtin(BuiltinObjective::Latency)
    }

    fn workload() -> Fingerprint {
        Fingerprint::EMPTY.text("be14f/test-workload")
    }

    fn policy() -> SelectionPolicy {
        SelectionPolicy::new(vec![latency()], contract(), true).unwrap()
    }

    fn capabilities() -> CapabilitySnapshot {
        CapabilitySnapshot {
            workgroup_limits: WorkgroupLimits {
                max_invocations_per_axis: [64, 64, 64],
                max_invocations_per_workgroup: 256,
                max_workgroups_per_axis: 65_535,
                max_workgroup_storage_bytes: 32_768,
            },
            binding_limits: BindingLimits {
                max_bind_groups: 8,
                max_storage_buffer_binding_bytes: 128 << 20,
            },
            subgroup_support: SubgroupSupport::unsupported(),
            shader_f16: FeatureSupport::Known(false),
            matrix_ops: FeatureSupport::Known(false),
        }
    }

    fn candidate(realization: &str, workgroup_storage_bytes: u64) -> KernelCandidate {
        KernelCandidate::new(
            resource(),
            RealizationIdentity::new(realization).unwrap(),
            1,
            KernelRequirements {
                invocations_per_workgroup: 64,
                invocations_per_axis: [64, 1, 1],
                workgroup_storage_bytes,
                bind_groups: 2,
                max_storage_buffer_binding_bytes: 4096,
                subgroup_min_width: None,
                shader_f16: FeatureRequirement::NotRequired,
                matrix_ops: FeatureRequirement::NotRequired,
            },
            contract(),
            ObjectiveEvidence::new().with(
                latency(),
                Evidence::StaticEstimate(StaticQuantity {
                    magnitude: 100,
                    unit: EvidenceUnit::Nanoseconds,
                }),
            ),
        )
        .unwrap()
    }

    fn guarded(
        candidates: &[KernelCandidate],
        snapshot: Option<&CapabilitySnapshot>,
    ) -> (BooleanKernelPlanOutcomeV1, BooleanKernelDecisionTraceV1) {
        let now = Instant::now();
        plan_with_boolean_admission_traced(
            &resource(),
            workload(),
            &policy(),
            candidates,
            snapshot,
            snapshot.map(|_| now),
            now,
            Duration::from_secs(1),
        )
        .unwrap()
    }

    #[test]
    fn true_candidate_executes_full_trusted_cycle() {
        let snapshot = capabilities();
        let candidates = vec![candidate("portable", 1024)];
        let (outcome, trace) = guarded(&candidates, Some(&snapshot));
        let mut backend = TestBackend::default();
        let result = execute_guarded_kernel_transaction(
            &outcome,
            &trace,
            &candidates,
            &snapshot,
            &mut backend,
        )
        .unwrap();

        let GuardedKernelTransactionOutcomeV1::Committed(committed) = result else {
            panic!("grounded candidate must commit: {result:?}");
        };
        assert_eq!(committed.realization().as_str(), "portable");
        assert_eq!(
            (
                backend.validate_count,
                backend.activate_count,
                backend.verify_count,
                backend.commit_count,
                backend.rollback_count
            ),
            (1, 1, 1, 1, 0)
        );
    }

    #[test]
    fn unknown_capability_evidence_never_calls_backend() {
        let candidates = vec![candidate("portable", 1024)];
        let (outcome, trace) = guarded(&candidates, None);
        let snapshot = capabilities();
        let mut backend = TestBackend::default();
        let result = execute_guarded_kernel_transaction(
            &outcome,
            &trace,
            &candidates,
            &snapshot,
            &mut backend,
        )
        .unwrap();

        assert_eq!(
            result,
            GuardedKernelTransactionOutcomeV1::Blocked(
                KernelTransactionBlockV1::BooleanInsufficientEvidence
            )
        );
        assert_eq!(
            (
                backend.validate_count,
                backend.activate_count,
                backend.verify_count,
                backend.commit_count
            ),
            (0, 0, 0, 0)
        );
    }

    #[test]
    fn false_candidate_never_calls_backend() {
        let snapshot = capabilities();
        let candidates = vec![candidate("too-large", 64 << 10)];
        let (outcome, trace) = guarded(&candidates, Some(&snapshot));
        let mut backend = TestBackend::default();
        let result = execute_guarded_kernel_transaction(
            &outcome,
            &trace,
            &candidates,
            &snapshot,
            &mut backend,
        )
        .unwrap();

        assert_eq!(
            result,
            GuardedKernelTransactionOutcomeV1::Blocked(KernelTransactionBlockV1::NoCandidate)
        );
        assert_eq!(
            (
                backend.validate_count,
                backend.activate_count,
                backend.verify_count,
                backend.commit_count
            ),
            (0, 0, 0, 0)
        );
    }

    #[test]
    fn fresh_capability_drift_fails_before_backend_validation() {
        let snapshot = capabilities();
        let candidates = vec![candidate("portable", 1024)];
        let (outcome, trace) = guarded(&candidates, Some(&snapshot));
        let mut drifted = snapshot;
        drifted.workgroup_limits.max_workgroup_storage_bytes += 1;
        let mut backend = TestBackend::default();
        let error = execute_guarded_kernel_transaction(
            &outcome,
            &trace,
            &candidates,
            &drifted,
            &mut backend,
        )
        .unwrap_err();

        assert_eq!(error.stage(), KernelTransactionStageV1::Bind);
        assert_eq!(backend.validate_count, 0);
        assert_eq!(backend.activate_count, 0);
    }

    #[test]
    fn verification_failure_rolls_back_and_never_commits() {
        let snapshot = capabilities();
        let candidates = vec![candidate("portable", 1024)];
        let (outcome, trace) = guarded(&candidates, Some(&snapshot));
        let mut backend = TestBackend {
            fail_verify: true,
            ..TestBackend::default()
        };
        let error = execute_guarded_kernel_transaction(
            &outcome,
            &trace,
            &candidates,
            &snapshot,
            &mut backend,
        )
        .unwrap_err();

        assert_eq!(error.stage(), KernelTransactionStageV1::Verify);
        assert_eq!(error.rollback().stopped_at(), "activated");
        assert_eq!(backend.commit_count, 0);
        assert_eq!(backend.rollback_count, 1);
    }

    #[test]
    fn guarded_and_unguarded_paths_commit_same_realization() {
        let snapshot = capabilities();
        let candidates = vec![candidate("portable", 1024)];
        let (guarded_outcome, trace) = guarded(&candidates, Some(&snapshot));
        let mut guarded_backend = TestBackend::default();
        let guarded_result = execute_guarded_kernel_transaction(
            &guarded_outcome,
            &trace,
            &candidates,
            &snapshot,
            &mut guarded_backend,
        )
        .unwrap();
        let GuardedKernelTransactionOutcomeV1::Committed(guarded_commit) = guarded_result else {
            panic!("guarded path unexpectedly blocked");
        };

        let unguarded_outcome = plan(&resource(), workload(), &snapshot, &policy(), &candidates);
        let SelectionOutcome::Selected(record) = unguarded_outcome else {
            panic!("unguarded reference did not select");
        };
        let mut unguarded_backend = TestBackend::default();
        let unguarded_commit =
            execute_kernel_transaction(&record, &candidates[0], &snapshot, &mut unguarded_backend)
                .unwrap();

        assert_eq!(guarded_commit.realization(), unguarded_commit.realization());
        assert_eq!(guarded_commit.contract(), unguarded_commit.contract());
        assert_eq!(guarded_backend.commit_count, unguarded_backend.commit_count);
    }
}
