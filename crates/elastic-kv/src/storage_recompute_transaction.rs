//! EX-SR-3 trusted transaction boundary for storage-versus-recomputation.
//!
//! An EX-SR-2 plan is explanatory screening evidence, not actuation authority.
//! This module re-screens the exact candidate and cost vector, binds them to an
//! authoritative backend source state, then drives VALIDATE -> ACT -> VERIFY ->
//! COMMIT or ROLLBACK. Domain runtimes retain ownership of storage mechanics,
//! reconstruction, and semantic or quality verification.

use elastic_eir::Fingerprint;
use std::fmt;

use crate::{
    StorageRecomputeActionV1, StorageRecomputeCandidateV1, StorageRecomputeCostVectorV1,
    StorageRecomputePlanV1, MAX_STORAGE_RECOMPUTE_ID_BYTES,
};

/// Stable schema identity for EX-SR-3 transaction evidence.
pub const STORAGE_RECOMPUTE_TRANSACTION_V1: &str = "elastic.kv.storage-recompute-transaction@1.0.0";

/// Authoritative backend state identity at a transaction boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageRecomputeBackendStateV1 {
    semantic_contract_id: String,
    generation: u64,
    state_id: String,
    fingerprint: Fingerprint,
}

impl StorageRecomputeBackendStateV1 {
    /// Construct a bounded backend state identity.
    pub fn new(
        semantic_contract_id: impl Into<String>,
        generation: u64,
        state_id: impl Into<String>,
    ) -> Result<Self, StorageRecomputeTransactionFailureV1> {
        let semantic_contract_id = semantic_contract_id.into();
        let state_id = state_id.into();
        validate_id("semantic_contract_id", &semantic_contract_id)?;
        validate_id("state_id", &state_id)?;
        let fingerprint = Fingerprint::EMPTY
            .text(STORAGE_RECOMPUTE_TRANSACTION_V1)
            .text(&semantic_contract_id)
            .number(generation)
            .text(&state_id);
        Ok(Self {
            semantic_contract_id,
            generation,
            state_id,
            fingerprint,
        })
    }

    #[must_use]
    pub fn semantic_contract_id(&self) -> &str {
        &self.semantic_contract_id
    }

    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    pub fn state_id(&self) -> &str {
        &self.state_id
    }

    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
}

/// Explicit action-time binding between a screened candidate and its exact
/// planned source state.
///
/// Generation and semantic contract are insufficient when multiple resources
/// share them. The source fingerprint includes the concrete state identity and
/// prevents applying a candidate planned for one resource to another.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageRecomputeTransactionBindingV1 {
    candidate_fingerprint: Fingerprint,
    source_fingerprint: Fingerprint,
    fingerprint: Fingerprint,
}

impl StorageRecomputeTransactionBindingV1 {
    #[must_use]
    pub fn new(
        candidate: &StorageRecomputeCandidateV1,
        source: &StorageRecomputeBackendStateV1,
    ) -> Self {
        let candidate_fingerprint = candidate.fingerprint();
        let source_fingerprint = source.fingerprint();
        let fingerprint = Fingerprint::EMPTY
            .text(STORAGE_RECOMPUTE_TRANSACTION_V1)
            .text("binding")
            .number(candidate_fingerprint.bits())
            .number(source_fingerprint.bits());
        Self {
            candidate_fingerprint,
            source_fingerprint,
            fingerprint,
        }
    }

    #[must_use]
    pub const fn candidate_fingerprint(&self) -> Fingerprint {
        self.candidate_fingerprint
    }

    #[must_use]
    pub const fn source_fingerprint(&self) -> Fingerprint {
        self.source_fingerprint
    }

    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
}

/// Failure classification for the atomic compare-and-apply boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum StorageRecomputeApplyErrorV1 {
    /// The source comparison failed before mutation. Rollback would be unsafe
    /// because it could overwrite the concurrent authoritative writer.
    SourceMismatch(String),
    /// The exact target was installed atomically before a later actuation
    /// failure. Rollback may restore the source only while that target remains
    /// authoritative.
    TargetApplied(String),
}

impl StorageRecomputeApplyErrorV1 {
    #[must_use]
    pub fn reason(&self) -> &str {
        match self {
            Self::SourceMismatch(reason) | Self::TargetApplied(reason) => reason,
        }
    }
}

impl fmt::Display for StorageRecomputeApplyErrorV1 {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str(self.reason())
    }
}

/// Trusted backend boundary for one already-authorized local storage surface.
///
/// `apply_if_source` must compare `source` and perform any mutation under the
/// same backend concurrency boundary. It must distinguish a clean comparison
/// failure from an error after the exact target was installed.
pub trait StorageRecomputeTransactionBackendV1 {
    /// Read the current authoritative state.
    fn read_state(&self) -> Result<StorageRecomputeBackendStateV1, String>;

    /// Revalidate domain and backend invariants without changing visible state.
    fn validate_action(
        &mut self,
        candidate: &StorageRecomputeCandidateV1,
        costs: &StorageRecomputeCostVectorV1,
        plan: &StorageRecomputePlanV1,
        source: &StorageRecomputeBackendStateV1,
        target: &StorageRecomputeBackendStateV1,
    ) -> Result<(), String>;

    /// Atomically compare the authoritative source and apply the declared action.
    fn apply_if_source(
        &mut self,
        candidate: &StorageRecomputeCandidateV1,
        plan: &StorageRecomputePlanV1,
        source: &StorageRecomputeBackendStateV1,
        target: &StorageRecomputeBackendStateV1,
    ) -> Result<(), StorageRecomputeApplyErrorV1>;

    /// Verify the physical result plus domain-owned semantic/quality invariants.
    fn verify_action(
        &mut self,
        candidate: &StorageRecomputeCandidateV1,
        plan: &StorageRecomputePlanV1,
        target: &StorageRecomputeBackendStateV1,
    ) -> Result<(), String>;

    /// Publish the already-verified target as authoritative.
    fn commit_action(
        &mut self,
        candidate: &StorageRecomputeCandidateV1,
        plan: &StorageRecomputePlanV1,
        target: &StorageRecomputeBackendStateV1,
    ) -> Result<(), String>;

    /// Restore the source only if the exact transaction target is still current.
    ///
    /// `Ok(false)` means another writer replaced the target; implementations
    /// must preserve that authoritative concurrent state.
    fn rollback_if_target(
        &mut self,
        candidate: &StorageRecomputeCandidateV1,
        target: &StorageRecomputeBackendStateV1,
        source: &StorageRecomputeBackendStateV1,
        reason: &str,
    ) -> Result<bool, String>;
}

/// Stable lifecycle stage retained by failure evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum StorageRecomputeTransactionStageV1 {
    Bind,
    Validate,
    Act,
    Verify,
    Commit,
}

impl StorageRecomputeTransactionStageV1 {
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

/// Evidence for one verified and committed storage/recompute transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommittedStorageRecomputeTransactionV1 {
    binding_fingerprint: Fingerprint,
    candidate_fingerprint: Fingerprint,
    cost_fingerprint: Fingerprint,
    plan_fingerprint: Fingerprint,
    source: StorageRecomputeBackendStateV1,
    target: StorageRecomputeBackendStateV1,
    action: StorageRecomputeActionV1,
    fingerprint: Fingerprint,
}

impl CommittedStorageRecomputeTransactionV1 {
    #[must_use]
    pub const fn binding_fingerprint(&self) -> Fingerprint {
        self.binding_fingerprint
    }

    #[must_use]
    pub const fn candidate_fingerprint(&self) -> Fingerprint {
        self.candidate_fingerprint
    }

    #[must_use]
    pub const fn cost_fingerprint(&self) -> Fingerprint {
        self.cost_fingerprint
    }

    #[must_use]
    pub const fn plan_fingerprint(&self) -> Fingerprint {
        self.plan_fingerprint
    }

    #[must_use]
    pub const fn source(&self) -> &StorageRecomputeBackendStateV1 {
        &self.source
    }

    #[must_use]
    pub const fn target(&self) -> &StorageRecomputeBackendStateV1 {
        &self.target
    }

    #[must_use]
    pub const fn action(&self) -> StorageRecomputeActionV1 {
        self.action
    }

    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
}

/// Fail-closed evidence from a transaction that did not commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageRecomputeTransactionFailureV1 {
    stage: StorageRecomputeTransactionStageV1,
    reason: String,
    candidate_fingerprint: Option<Fingerprint>,
    plan_fingerprint: Option<Fingerprint>,
    source_fingerprint: Option<Fingerprint>,
    rollback_attempted: bool,
    rollback_restored_source: bool,
    backend_rollback_error: Option<String>,
}

impl StorageRecomputeTransactionFailureV1 {
    #[must_use]
    pub const fn stage(&self) -> StorageRecomputeTransactionStageV1 {
        self.stage
    }

    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }

    #[must_use]
    pub const fn candidate_fingerprint(&self) -> Option<Fingerprint> {
        self.candidate_fingerprint
    }

    #[must_use]
    pub const fn plan_fingerprint(&self) -> Option<Fingerprint> {
        self.plan_fingerprint
    }

    #[must_use]
    pub const fn source_fingerprint(&self) -> Option<Fingerprint> {
        self.source_fingerprint
    }

    #[must_use]
    pub const fn rollback_attempted(&self) -> bool {
        self.rollback_attempted
    }

    #[must_use]
    pub const fn rollback_restored_source(&self) -> bool {
        self.rollback_restored_source
    }

    #[must_use]
    pub fn backend_rollback_error(&self) -> Option<&str> {
        self.backend_rollback_error.as_deref()
    }
}

impl fmt::Display for StorageRecomputeTransactionFailureV1 {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            output,
            "storage/recompute transaction failed at {}: {}",
            self.stage.as_str(),
            self.reason
        )?;
        if let Some(error) = &self.backend_rollback_error {
            write!(output, "; backend rollback also failed: {error}")?;
        } else if self.rollback_attempted && !self.rollback_restored_source {
            output.write_str("; rollback did not restore the bound source")?;
        }
        Ok(())
    }
}

impl std::error::Error for StorageRecomputeTransactionFailureV1 {}

fn failure_without_rollback(
    stage: StorageRecomputeTransactionStageV1,
    reason: impl Into<String>,
) -> StorageRecomputeTransactionFailureV1 {
    StorageRecomputeTransactionFailureV1 {
        stage,
        reason: reason.into(),
        candidate_fingerprint: None,
        plan_fingerprint: None,
        source_fingerprint: None,
        rollback_attempted: false,
        rollback_restored_source: false,
        backend_rollback_error: None,
    }
}

fn bound_failure_without_rollback(
    candidate: &StorageRecomputeCandidateV1,
    plan: &StorageRecomputePlanV1,
    source: Option<&StorageRecomputeBackendStateV1>,
    stage: StorageRecomputeTransactionStageV1,
    reason: impl Into<String>,
) -> StorageRecomputeTransactionFailureV1 {
    StorageRecomputeTransactionFailureV1 {
        stage,
        reason: reason.into(),
        candidate_fingerprint: Some(candidate.fingerprint()),
        plan_fingerprint: Some(plan.fingerprint()),
        source_fingerprint: source.map(StorageRecomputeBackendStateV1::fingerprint),
        rollback_attempted: false,
        rollback_restored_source: false,
        backend_rollback_error: None,
    }
}

fn fail_after_possible_mutation<B: StorageRecomputeTransactionBackendV1>(
    backend: &mut B,
    candidate: &StorageRecomputeCandidateV1,
    plan: &StorageRecomputePlanV1,
    source: &StorageRecomputeBackendStateV1,
    target: &StorageRecomputeBackendStateV1,
    stage: StorageRecomputeTransactionStageV1,
    reason: String,
) -> StorageRecomputeTransactionFailureV1 {
    let rollback_result = backend.rollback_if_target(candidate, target, source, &reason);
    let backend_rollback_error = rollback_result.as_ref().err().cloned();
    let rollback_restored_source = rollback_result.is_ok_and(|restored| restored)
        && backend.read_state().is_ok_and(|current| current == *source);
    StorageRecomputeTransactionFailureV1 {
        stage,
        reason,
        candidate_fingerprint: Some(candidate.fingerprint()),
        plan_fingerprint: Some(plan.fingerprint()),
        source_fingerprint: Some(source.fingerprint()),
        rollback_attempted: true,
        rollback_restored_source,
        backend_rollback_error,
    }
}

fn validate_id(
    field: &'static str,
    value: &str,
) -> Result<(), StorageRecomputeTransactionFailureV1> {
    if value.is_empty() {
        return Err(failure_without_rollback(
            StorageRecomputeTransactionStageV1::Bind,
            format!("{field} must not be empty"),
        ));
    }
    if value.len() > MAX_STORAGE_RECOMPUTE_ID_BYTES {
        return Err(failure_without_rollback(
            StorageRecomputeTransactionStageV1::Bind,
            format!(
                "{field} uses {} bytes, maximum is {MAX_STORAGE_RECOMPUTE_ID_BYTES}",
                value.len()
            ),
        ));
    }
    Ok(())
}

fn derive_target(
    candidate: &StorageRecomputeCandidateV1,
    plan: &StorageRecomputePlanV1,
    source: &StorageRecomputeBackendStateV1,
) -> Result<StorageRecomputeBackendStateV1, StorageRecomputeTransactionFailureV1> {
    if candidate.semantic_contract_id() != source.semantic_contract_id() {
        return Err(bound_failure_without_rollback(
            candidate,
            plan,
            Some(source),
            StorageRecomputeTransactionStageV1::Bind,
            "candidate semantic contract differs from the authoritative backend state",
        ));
    }
    if candidate.source_generation() != source.generation() {
        return Err(bound_failure_without_rollback(
            candidate,
            plan,
            Some(source),
            StorageRecomputeTransactionStageV1::Bind,
            "candidate source generation differs from the authoritative backend state",
        ));
    }

    if candidate.action() == StorageRecomputeActionV1::Keep {
        if candidate.target_state_id() != source.state_id() {
            return Err(bound_failure_without_rollback(
                candidate,
                plan,
                Some(source),
                StorageRecomputeTransactionStageV1::Bind,
                "KEEP must retain the authoritative source state identity",
            ));
        }
        return Ok(source.clone());
    }

    let target_generation = source.generation().checked_add(1).ok_or_else(|| {
        bound_failure_without_rollback(
            candidate,
            plan,
            Some(source),
            StorageRecomputeTransactionStageV1::Bind,
            "target generation overflow",
        )
    })?;
    StorageRecomputeBackendStateV1::new(
        source.semantic_contract_id(),
        target_generation,
        candidate.target_state_id(),
    )
}

fn commit_record(
    binding: &StorageRecomputeTransactionBindingV1,
    candidate: &StorageRecomputeCandidateV1,
    costs: &StorageRecomputeCostVectorV1,
    plan: &StorageRecomputePlanV1,
    source: StorageRecomputeBackendStateV1,
    target: StorageRecomputeBackendStateV1,
) -> CommittedStorageRecomputeTransactionV1 {
    let fingerprint = Fingerprint::EMPTY
        .text(STORAGE_RECOMPUTE_TRANSACTION_V1)
        .number(binding.fingerprint().bits())
        .number(candidate.fingerprint().bits())
        .number(costs.fingerprint().bits())
        .number(plan.fingerprint().bits())
        .number(source.fingerprint().bits())
        .number(target.fingerprint().bits());
    CommittedStorageRecomputeTransactionV1 {
        binding_fingerprint: binding.fingerprint(),
        candidate_fingerprint: candidate.fingerprint(),
        cost_fingerprint: costs.fingerprint(),
        plan_fingerprint: plan.fingerprint(),
        source,
        target,
        action: candidate.action(),
        fingerprint,
    }
}

/// Execute one exact screened candidate through the trusted backend lifecycle.
///
/// The plan is re-derived from the supplied candidate and action-time cost
/// evidence before any backend method. The explicit transaction binding must
/// match both the candidate and the exact authoritative source fingerprint.
/// Only failures after the exact target was installed attempt conditional
/// rollback, which preserves any later authoritative writer.
pub fn execute_storage_recompute_transaction<B: StorageRecomputeTransactionBackendV1>(
    binding: &StorageRecomputeTransactionBindingV1,
    candidate: &StorageRecomputeCandidateV1,
    costs: &StorageRecomputeCostVectorV1,
    plan: &StorageRecomputePlanV1,
    backend: &mut B,
) -> Result<CommittedStorageRecomputeTransactionV1, StorageRecomputeTransactionFailureV1> {
    if binding.candidate_fingerprint() != candidate.fingerprint() {
        return Err(bound_failure_without_rollback(
            candidate,
            plan,
            None,
            StorageRecomputeTransactionStageV1::Bind,
            "transaction binding belongs to a different candidate",
        ));
    }
    let authoritative_plan = StorageRecomputePlanV1::screen(candidate, costs, plan.limits())
        .map_err(|error| {
            bound_failure_without_rollback(
                candidate,
                plan,
                None,
                StorageRecomputeTransactionStageV1::Bind,
                format!("action-time plan screening failed: {error}"),
            )
        })?;
    if authoritative_plan != *plan {
        return Err(bound_failure_without_rollback(
            candidate,
            plan,
            None,
            StorageRecomputeTransactionStageV1::Bind,
            "supplied plan differs from action-time authoritative screening",
        ));
    }

    let source = backend.read_state().map_err(|error| {
        bound_failure_without_rollback(
            candidate,
            plan,
            None,
            StorageRecomputeTransactionStageV1::Bind,
            format!("authoritative source read failed: {error}"),
        )
    })?;
    if binding.source_fingerprint() != source.fingerprint() {
        return Err(bound_failure_without_rollback(
            candidate,
            plan,
            Some(&source),
            StorageRecomputeTransactionStageV1::Bind,
            "transaction binding differs from the exact authoritative source state",
        ));
    }
    let target = derive_target(candidate, plan, &source)?;

    if let Err(error) = backend.validate_action(candidate, costs, plan, &source, &target) {
        return Err(bound_failure_without_rollback(
            candidate,
            plan,
            Some(&source),
            StorageRecomputeTransactionStageV1::Validate,
            format!("trusted backend validation failed: {error}"),
        ));
    }

    // Re-read after validation and immediately before the first possibly
    // mutating call. The backend must repeat this comparison atomically inside
    // `apply_if_source`.
    let current = backend.read_state().map_err(|error| {
        bound_failure_without_rollback(
            candidate,
            plan,
            Some(&source),
            StorageRecomputeTransactionStageV1::Validate,
            format!("pre-actuation source read failed: {error}"),
        )
    })?;
    if current != source {
        return Err(bound_failure_without_rollback(
            candidate,
            plan,
            Some(&source),
            StorageRecomputeTransactionStageV1::Validate,
            "authoritative source drifted after validation",
        ));
    }

    if let Err(error) = backend.apply_if_source(candidate, plan, &source, &target) {
        return Err(match error {
            StorageRecomputeApplyErrorV1::SourceMismatch(reason) => bound_failure_without_rollback(
                candidate,
                plan,
                Some(&source),
                StorageRecomputeTransactionStageV1::Act,
                format!("backend source comparison failed before mutation: {reason}"),
            ),
            StorageRecomputeApplyErrorV1::TargetApplied(reason) => {
                fail_after_possible_mutation(
                    backend,
                    candidate,
                    plan,
                    &source,
                    &target,
                    StorageRecomputeTransactionStageV1::Act,
                    format!("backend actuation failed after installing the target: {reason}"),
                )
            }
        });
    }

    let observed = backend.read_state().map_err(|error| {
        fail_after_possible_mutation(
            backend,
            candidate,
            plan,
            &source,
            &target,
            StorageRecomputeTransactionStageV1::Verify,
            format!("post-actuation state read failed: {error}"),
        )
    })?;
    if observed != target {
        return Err(fail_after_possible_mutation(
            backend,
            candidate,
            plan,
            &source,
            &target,
            StorageRecomputeTransactionStageV1::Verify,
            "post-actuation state differs from the bound target".to_owned(),
        ));
    }
    if let Err(error) = backend.verify_action(candidate, plan, &target) {
        return Err(fail_after_possible_mutation(
            backend,
            candidate,
            plan,
            &source,
            &target,
            StorageRecomputeTransactionStageV1::Verify,
            format!("backend verification failed: {error}"),
        ));
    }

    if let Err(error) = backend.commit_action(candidate, plan, &target) {
        return Err(fail_after_possible_mutation(
            backend,
            candidate,
            plan,
            &source,
            &target,
            StorageRecomputeTransactionStageV1::Commit,
            format!("backend commit failed: {error}"),
        ));
    }

    Ok(commit_record(
        binding, candidate, costs, plan, source, target,
    ))
}

/// Deterministic in-memory reference backend.
///
/// This backend demonstrates transaction semantics only. It performs no file,
/// device, network, codec, KV, model, latency, or quality work and therefore is
/// not performance or domain-correctness evidence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceStorageRecomputeBackendV1 {
    state: StorageRecomputeBackendStateV1,
    committed_fingerprint: Option<Fingerprint>,
}

impl ReferenceStorageRecomputeBackendV1 {
    #[must_use]
    pub const fn new(state: StorageRecomputeBackendStateV1) -> Self {
        Self {
            state,
            committed_fingerprint: None,
        }
    }

    #[must_use]
    pub const fn state(&self) -> &StorageRecomputeBackendStateV1 {
        &self.state
    }

    #[must_use]
    pub const fn committed_fingerprint(&self) -> Option<Fingerprint> {
        self.committed_fingerprint
    }
}

impl StorageRecomputeTransactionBackendV1 for ReferenceStorageRecomputeBackendV1 {
    fn read_state(&self) -> Result<StorageRecomputeBackendStateV1, String> {
        Ok(self.state.clone())
    }

    fn validate_action(
        &mut self,
        candidate: &StorageRecomputeCandidateV1,
        costs: &StorageRecomputeCostVectorV1,
        plan: &StorageRecomputePlanV1,
        source: &StorageRecomputeBackendStateV1,
        target: &StorageRecomputeBackendStateV1,
    ) -> Result<(), String> {
        if self.state != *source {
            return Err("reference source drifted".into());
        }
        if candidate.fingerprint() != plan.candidate_fingerprint()
            || costs.fingerprint() != plan.cost_fingerprint()
        {
            return Err("reference binding mismatch".into());
        }
        if candidate.action() == StorageRecomputeActionV1::Keep && target != source {
            return Err("reference KEEP target mutates state".into());
        }
        Ok(())
    }

    fn apply_if_source(
        &mut self,
        _candidate: &StorageRecomputeCandidateV1,
        _plan: &StorageRecomputePlanV1,
        source: &StorageRecomputeBackendStateV1,
        target: &StorageRecomputeBackendStateV1,
    ) -> Result<(), StorageRecomputeApplyErrorV1> {
        if self.state != *source {
            return Err(StorageRecomputeApplyErrorV1::SourceMismatch(
                "reference source comparison failed".into(),
            ));
        }
        self.state = target.clone();
        Ok(())
    }

    fn verify_action(
        &mut self,
        _candidate: &StorageRecomputeCandidateV1,
        _plan: &StorageRecomputePlanV1,
        target: &StorageRecomputeBackendStateV1,
    ) -> Result<(), String> {
        (self.state == *target)
            .then_some(())
            .ok_or_else(|| "reference target verification failed".into())
    }

    fn commit_action(
        &mut self,
        _candidate: &StorageRecomputeCandidateV1,
        plan: &StorageRecomputePlanV1,
        target: &StorageRecomputeBackendStateV1,
    ) -> Result<(), String> {
        if self.state != *target {
            return Err("reference commit target mismatch".into());
        }
        self.committed_fingerprint = Some(plan.fingerprint());
        Ok(())
    }

    fn rollback_if_target(
        &mut self,
        _candidate: &StorageRecomputeCandidateV1,
        target: &StorageRecomputeBackendStateV1,
        source: &StorageRecomputeBackendStateV1,
        _reason: &str,
    ) -> Result<bool, String> {
        if self.state != *target {
            return Ok(false);
        }
        self.state = source.clone();
        self.committed_fingerprint = None;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ByteCostEvidenceV1, ByteEvidenceScopeV1, CostEvidenceBasisV1, DurationCostEvidenceV1,
        QualityGuardEvidenceV1, ReplayContractV1, ReplayReconstructionV1,
        StorageRecomputePlanLimitsV1,
    };

    fn bytes(value: u64) -> ByteCostEvidenceV1 {
        ByteCostEvidenceV1::new(
            value,
            ByteEvidenceScopeV1::Physical,
            CostEvidenceBasisV1::Measured,
            "bytes-1",
        )
        .unwrap()
    }

    fn duration(value: u64) -> DurationCostEvidenceV1 {
        DurationCostEvidenceV1::new(value, CostEvidenceBasisV1::Measured, "duration-1").unwrap()
    }

    fn fixture(
        action: StorageRecomputeActionV1,
    ) -> (
        StorageRecomputeCandidateV1,
        StorageRecomputeCostVectorV1,
        StorageRecomputePlanV1,
        StorageRecomputeBackendStateV1,
    ) {
        let (target, replay) = match action {
            StorageRecomputeActionV1::Keep => ("source", None),
            StorageRecomputeActionV1::DropAndReplay => (
                "replayed",
                Some(ReplayContractV1::new(8, ReplayReconstructionV1::Exact, None).unwrap()),
            ),
            StorageRecomputeActionV1::Compress => ("compressed", None),
            StorageRecomputeActionV1::Offload => ("offloaded", None),
        };
        let candidate = StorageRecomputeCandidateV1::new(
            "candidate-1",
            "domain.state.v1",
            7,
            target,
            action,
            replay,
        )
        .unwrap();
        let (transfer_bytes, transfer_latency, recompute_latency) = match action {
            StorageRecomputeActionV1::Offload => (Some(bytes(8)), Some(duration(10)), None),
            StorageRecomputeActionV1::DropAndReplay => (None, None, Some(duration(20))),
            StorageRecomputeActionV1::Keep | StorageRecomputeActionV1::Compress => {
                (None, None, None)
            }
        };
        let costs = StorageRecomputeCostVectorV1::new(
            &candidate,
            Some(bytes(4)),
            transfer_bytes,
            transfer_latency,
            recompute_latency,
            Some(duration(5)),
            QualityGuardEvidenceV1::NotAttached,
        )
        .unwrap();
        let limits = StorageRecomputePlanLimitsV1::new(
            Some(16),
            (action == StorageRecomputeActionV1::Offload).then_some(16),
            Some(64),
            false,
        )
        .unwrap();
        let plan = StorageRecomputePlanV1::screen(&candidate, &costs, limits).unwrap();
        let source = StorageRecomputeBackendStateV1::new("domain.state.v1", 7, "source").unwrap();
        (candidate, costs, plan, source)
    }

    fn execute<B: StorageRecomputeTransactionBackendV1>(
        candidate: &StorageRecomputeCandidateV1,
        costs: &StorageRecomputeCostVectorV1,
        plan: &StorageRecomputePlanV1,
        source: &StorageRecomputeBackendStateV1,
        backend: &mut B,
    ) -> Result<CommittedStorageRecomputeTransactionV1, StorageRecomputeTransactionFailureV1> {
        let binding = StorageRecomputeTransactionBindingV1::new(candidate, source);
        execute_storage_recompute_transaction(&binding, candidate, costs, plan, backend)
    }

    #[test]
    fn reference_backend_commits_every_declared_action_family_member() {
        for action in [
            StorageRecomputeActionV1::Keep,
            StorageRecomputeActionV1::Compress,
            StorageRecomputeActionV1::Offload,
            StorageRecomputeActionV1::DropAndReplay,
        ] {
            let (candidate, costs, plan, source) = fixture(action);
            let mut backend = ReferenceStorageRecomputeBackendV1::new(source.clone());
            let committed = execute(&candidate, &costs, &plan, &source, &mut backend).unwrap();

            assert_eq!(committed.action(), action);
            assert_eq!(committed.source(), &source);
            assert_eq!(committed.plan_fingerprint(), plan.fingerprint());
            assert_eq!(backend.state(), committed.target());
            assert_eq!(backend.committed_fingerprint(), Some(plan.fingerprint()));
            if action == StorageRecomputeActionV1::Keep {
                assert_eq!(committed.target(), &source);
            } else {
                assert_eq!(committed.target().generation(), 8);
                assert_eq!(committed.target().state_id(), candidate.target_state_id());
            }
        }
    }

    #[test]
    fn stale_source_generation_fails_before_backend_validation() {
        let (candidate, costs, plan, _) = fixture(StorageRecomputeActionV1::Compress);
        let stale = StorageRecomputeBackendStateV1::new("domain.state.v1", 8, "source").unwrap();
        let mut backend = ReferenceStorageRecomputeBackendV1::new(stale.clone());

        let binding_source =
            StorageRecomputeBackendStateV1::new("domain.state.v1", 7, "source").unwrap();
        let failure =
            execute(&candidate, &costs, &plan, &binding_source, &mut backend).unwrap_err();
        assert_eq!(failure.stage(), StorageRecomputeTransactionStageV1::Bind);
        assert!(!failure.rollback_attempted());
        assert_eq!(backend.state(), &stale);
    }

    #[test]
    fn binding_rejects_a_different_source_identity_with_same_contract_and_generation() {
        let (candidate, costs, plan, source) = fixture(StorageRecomputeActionV1::Compress);
        let other =
            StorageRecomputeBackendStateV1::new("domain.state.v1", 7, "other-source").unwrap();
        let binding = StorageRecomputeTransactionBindingV1::new(&candidate, &source);
        let mut backend = ReferenceStorageRecomputeBackendV1::new(other.clone());

        let failure = execute_storage_recompute_transaction(
            &binding,
            &candidate,
            &costs,
            &plan,
            &mut backend,
        )
        .unwrap_err();

        assert_eq!(failure.stage(), StorageRecomputeTransactionStageV1::Bind);
        assert!(failure.reason().contains("exact authoritative source"));
        assert!(!failure.rollback_attempted());
        assert_eq!(backend.state(), &other);
    }

    #[test]
    fn plan_from_another_cost_vector_fails_closed() {
        let (candidate, costs, plan, source) = fixture(StorageRecomputeActionV1::Compress);
        let changed = StorageRecomputeCostVectorV1::new(
            &candidate,
            Some(bytes(5)),
            None,
            None,
            None,
            Some(duration(5)),
            QualityGuardEvidenceV1::NotAttached,
        )
        .unwrap();
        let mut backend = ReferenceStorageRecomputeBackendV1::new(source.clone());

        let failure = execute(&candidate, &changed, &plan, &source, &mut backend).unwrap_err();
        assert_eq!(failure.stage(), StorageRecomputeTransactionStageV1::Bind);
        assert!(failure.reason().contains("differs"));
        assert_eq!(backend.state(), &source);
        assert_ne!(costs.fingerprint(), changed.fingerprint());
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum FailAt {
        None,
        Validate,
        ConcurrentSourceMismatch,
        ConcurrentAfterAct,
        Act,
        Verify,
        Commit,
        Rollback,
    }

    struct FaultBackend {
        inner: ReferenceStorageRecomputeBackendV1,
        fail_at: FailAt,
    }

    impl StorageRecomputeTransactionBackendV1 for FaultBackend {
        fn read_state(&self) -> Result<StorageRecomputeBackendStateV1, String> {
            self.inner.read_state()
        }

        fn validate_action(
            &mut self,
            candidate: &StorageRecomputeCandidateV1,
            costs: &StorageRecomputeCostVectorV1,
            plan: &StorageRecomputePlanV1,
            source: &StorageRecomputeBackendStateV1,
            target: &StorageRecomputeBackendStateV1,
        ) -> Result<(), String> {
            if self.fail_at == FailAt::Validate {
                return Err("injected validation failure".into());
            }
            self.inner
                .validate_action(candidate, costs, plan, source, target)
        }

        fn apply_if_source(
            &mut self,
            candidate: &StorageRecomputeCandidateV1,
            plan: &StorageRecomputePlanV1,
            source: &StorageRecomputeBackendStateV1,
            target: &StorageRecomputeBackendStateV1,
        ) -> Result<(), StorageRecomputeApplyErrorV1> {
            if self.fail_at == FailAt::ConcurrentSourceMismatch {
                self.inner.state = StorageRecomputeBackendStateV1::new(
                    source.semantic_contract_id(),
                    source.generation() + 1,
                    "concurrent-writer",
                )
                .unwrap();
                return Err(StorageRecomputeApplyErrorV1::SourceMismatch(
                    "injected concurrent writer".into(),
                ));
            }
            self.inner
                .apply_if_source(candidate, plan, source, target)?;
            if self.fail_at == FailAt::Act {
                return Err(StorageRecomputeApplyErrorV1::TargetApplied(
                    "injected post-mutation actuation failure".into(),
                ));
            }
            Ok(())
        }

        fn verify_action(
            &mut self,
            candidate: &StorageRecomputeCandidateV1,
            plan: &StorageRecomputePlanV1,
            target: &StorageRecomputeBackendStateV1,
        ) -> Result<(), String> {
            if self.fail_at == FailAt::ConcurrentAfterAct {
                self.inner.state = StorageRecomputeBackendStateV1::new(
                    target.semantic_contract_id(),
                    target.generation() + 1,
                    "concurrent-after-act",
                )
                .unwrap();
                return Err("injected concurrent writer after actuation".into());
            }
            if self.fail_at == FailAt::Verify {
                return Err("injected verification failure".into());
            }
            self.inner.verify_action(candidate, plan, target)
        }

        fn commit_action(
            &mut self,
            candidate: &StorageRecomputeCandidateV1,
            plan: &StorageRecomputePlanV1,
            target: &StorageRecomputeBackendStateV1,
        ) -> Result<(), String> {
            if self.fail_at == FailAt::Commit {
                return Err("injected commit failure".into());
            }
            self.inner.commit_action(candidate, plan, target)
        }

        fn rollback_if_target(
            &mut self,
            candidate: &StorageRecomputeCandidateV1,
            target: &StorageRecomputeBackendStateV1,
            source: &StorageRecomputeBackendStateV1,
            reason: &str,
        ) -> Result<bool, String> {
            if self.fail_at == FailAt::Rollback {
                return Err("injected rollback failure".into());
            }
            self.inner
                .rollback_if_target(candidate, target, source, reason)
        }
    }

    fn fault_backend(source: StorageRecomputeBackendStateV1, fail_at: FailAt) -> FaultBackend {
        FaultBackend {
            inner: ReferenceStorageRecomputeBackendV1::new(source),
            fail_at,
        }
    }

    #[test]
    fn validation_failure_never_attempts_rollback() {
        let (candidate, costs, plan, source) = fixture(StorageRecomputeActionV1::Compress);
        let mut backend = fault_backend(source.clone(), FailAt::Validate);
        let failure = execute(&candidate, &costs, &plan, &source, &mut backend).unwrap_err();

        assert_eq!(
            failure.stage(),
            StorageRecomputeTransactionStageV1::Validate
        );
        assert!(!failure.rollback_attempted());
        assert_eq!(backend.inner.state(), &source);
    }

    #[test]
    fn clean_atomic_source_mismatch_preserves_the_concurrent_writer() {
        let (candidate, costs, plan, source) = fixture(StorageRecomputeActionV1::Compress);
        let mut backend = fault_backend(source.clone(), FailAt::ConcurrentSourceMismatch);

        let failure = execute(&candidate, &costs, &plan, &source, &mut backend).unwrap_err();

        assert_eq!(failure.stage(), StorageRecomputeTransactionStageV1::Act);
        assert!(!failure.rollback_attempted());
        assert_eq!(backend.inner.state().generation(), source.generation() + 1);
        assert_eq!(backend.inner.state().state_id(), "concurrent-writer");
    }

    #[test]
    fn conditional_rollback_preserves_a_writer_after_actuation() {
        let (candidate, costs, plan, source) = fixture(StorageRecomputeActionV1::Compress);
        let mut backend = fault_backend(source.clone(), FailAt::ConcurrentAfterAct);

        let failure = execute(&candidate, &costs, &plan, &source, &mut backend).unwrap_err();

        assert_eq!(failure.stage(), StorageRecomputeTransactionStageV1::Verify);
        assert!(failure.rollback_attempted());
        assert!(!failure.rollback_restored_source());
        assert_eq!(backend.inner.state().generation(), source.generation() + 2);
        assert_eq!(backend.inner.state().state_id(), "concurrent-after-act");
        assert_eq!(failure.backend_rollback_error(), None);
    }

    #[test]
    fn every_post_mutation_failure_rolls_back_and_verifies_source() {
        for (fail_at, expected_stage) in [
            (FailAt::Act, StorageRecomputeTransactionStageV1::Act),
            (FailAt::Verify, StorageRecomputeTransactionStageV1::Verify),
            (FailAt::Commit, StorageRecomputeTransactionStageV1::Commit),
        ] {
            let (candidate, costs, plan, source) = fixture(StorageRecomputeActionV1::Compress);
            let mut backend = fault_backend(source.clone(), fail_at);
            let failure = execute(&candidate, &costs, &plan, &source, &mut backend).unwrap_err();

            assert_eq!(failure.stage(), expected_stage);
            assert!(failure.rollback_attempted());
            assert!(failure.rollback_restored_source());
            assert_eq!(backend.inner.state(), &source);
        }
    }

    #[test]
    fn rollback_failure_is_preserved_without_claiming_restoration() {
        let (candidate, costs, plan, source) = fixture(StorageRecomputeActionV1::Compress);
        let mut backend = fault_backend(source.clone(), FailAt::Rollback);
        backend.fail_at = FailAt::Act;
        let failure = execute(&candidate, &costs, &plan, &source, &mut backend).unwrap_err();
        assert!(failure.rollback_restored_source());

        let mut backend = fault_backend(source.clone(), FailAt::Rollback);
        let target = StorageRecomputeBackendStateV1::new("domain.state.v1", 8, "mutated").unwrap();
        backend.inner.state = target;
        let failure = fail_after_possible_mutation(
            &mut backend,
            &candidate,
            &plan,
            &source,
            &target,
            StorageRecomputeTransactionStageV1::Verify,
            "injected".into(),
        );
        assert!(failure.rollback_attempted());
        assert!(!failure.rollback_restored_source());
        assert_eq!(
            failure.backend_rollback_error(),
            Some("injected rollback failure")
        );
    }

    #[test]
    fn no_fault_mode_remains_available_for_test_backends() {
        let (candidate, costs, plan, source) = fixture(StorageRecomputeActionV1::Compress);
        let mut backend = fault_backend(source.clone(), FailAt::None);
        assert!(execute(&candidate, &costs, &plan, &source, &mut backend).is_ok());
    }
}
