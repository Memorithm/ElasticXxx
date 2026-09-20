//! ELANG4c coordinated composite act/verify/commit-or-restore semantics.
//!
//! Composite commit is only reported after every targeted resource has been
//! actuated, verified, and locally committed. Until then, each backend must
//! retain an exact pre-actuation checkpoint and guarantee that it can restore
//! that checkpoint even after a local `commit()` has succeeded. This explicit
//! reversible-commit window prevents the generic runtime from claiming atomic
//! multi-resource semantics that the backends cannot actually provide.

use std::collections::BTreeSet;
use std::fmt;

use elastic_eir::Fingerprint;

use crate::{
    Actuation, CancellationToken, CommitRecord, CompositePreActState, CompositePrepareBackend,
    CompositePreparedEnvelope, RuntimeError,
};

/// Schema version of the composite transaction contract.
pub const COMPOSITE_TRANSACTION_SCHEMA_V1: u16 = 1;

/// Additional backend guarantee required for composite atomic recovery.
pub trait CompositeTransactionBackend: CompositePrepareBackend {
    /// Restore the exact pre-actuation state represented by `checkpoint`.
    ///
    /// This method must remain valid after `actuate()` and after a successful
    /// local `commit()` until `release_pre_act_state()` succeeds. Returning a
    /// rollback record with `invariants_restored = false` is treated as a
    /// recovery failure.
    fn restore_pre_act_state(
        &mut self,
        actuation: &Actuation,
        checkpoint: &CompositePreActState,
        reason: &str,
    ) -> Result<crate::RollbackRecord, RuntimeError>;
}

/// Final successful composite commit report.
#[derive(Debug, PartialEq)]
pub struct CompositeCommitReport {
    schema_version: u16,
    group_id: String,
    source_plan_fingerprint: Fingerprint,
    commits: Vec<CompositeResourceCommit>,
    checkpoint_cleanup: Option<CompositeCommitCleanupEnvelope>,
}

impl CompositeCommitReport {
    #[must_use]
    pub const fn schema_version(&self) -> u16 {
        self.schema_version
    }

    #[must_use]
    pub fn group_id(&self) -> &str {
        &self.group_id
    }

    #[must_use]
    pub const fn source_plan_fingerprint(&self) -> Fingerprint {
        self.source_plan_fingerprint
    }

    #[must_use]
    pub fn commits(&self) -> &[CompositeResourceCommit] {
        &self.commits
    }

    /// Non-semantic checkpoint cleanup retained after all resource commits
    /// succeeded. Visible resource state is already globally committed.
    #[must_use]
    pub fn checkpoint_cleanup(&self) -> Option<&CompositeCommitCleanupEnvelope> {
        self.checkpoint_cleanup.as_ref()
    }

    /// Consume the report and recover outstanding post-commit checkpoint cleanup.
    #[must_use]
    pub fn into_checkpoint_cleanup(self) -> Option<CompositeCommitCleanupEnvelope> {
        self.checkpoint_cleanup
    }
}

/// One locally committed resource in a successful composite transaction.
#[derive(Clone, Debug, PartialEq)]
pub struct CompositeResourceCommit {
    resource_id: String,
    record: CommitRecord,
}

impl CompositeResourceCommit {
    #[must_use]
    pub fn resource_id(&self) -> &str {
        &self.resource_id
    }

    #[must_use]
    pub const fn record(&self) -> &CommitRecord {
        &self.record
    }
}

/// Cleanup-only state after visible composite commit already succeeded.
#[derive(Debug, PartialEq)]
pub struct CompositeCommitCleanupEnvelope {
    group_id: String,
    entries: Vec<CompositeCommitCleanupEntry>,
}

impl CompositeCommitCleanupEnvelope {
    #[must_use]
    pub fn group_id(&self) -> &str {
        &self.group_id
    }

    #[must_use]
    pub fn entries(&self) -> &[CompositeCommitCleanupEntry] {
        &self.entries
    }
}

/// One checkpoint that still needs release after successful global commit.
#[derive(Debug, PartialEq)]
pub struct CompositeCommitCleanupEntry {
    resource_id: String,
    adapter_name: String,
    backend_instance_id: String,
    checkpoint: CompositePreActState,
}

impl CompositeCommitCleanupEntry {
    #[must_use]
    pub fn resource_id(&self) -> &str {
        &self.resource_id
    }

    #[must_use]
    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    #[must_use]
    pub fn backend_instance_id(&self) -> &str {
        &self.backend_instance_id
    }

    #[must_use]
    pub const fn checkpoint(&self) -> &CompositePreActState {
        &self.checkpoint
    }
}

/// Transaction stage that triggered coordinated recovery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompositeTransactionStage {
    Bind,
    Cancelled,
    Actuate,
    Verify,
    Commit,
    Restore,
}

impl CompositeTransactionStage {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bind => "bind",
            Self::Cancelled => "cancelled",
            Self::Actuate => "actuate",
            Self::Verify => "verify",
            Self::Commit => "commit",
            Self::Restore => "restore",
        }
    }
}

/// Cleanup action still required after a failed composite transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompositeTransactionRecoveryAction {
    AbortPrepare,
    RestorePreActState,
    ReleaseCheckpoint,
}

impl CompositeTransactionRecoveryAction {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AbortPrepare => "abort-prepare",
            Self::RestorePreActState => "restore-pre-act-state",
            Self::ReleaseCheckpoint => "release-checkpoint",
        }
    }
}

/// One linear recovery token retained after incomplete coordinated recovery.
#[derive(Debug, PartialEq)]
pub struct CompositeTransactionRecoveryEntry {
    resource_id: String,
    adapter_name: String,
    backend_instance_id: String,
    checkpoint: CompositePreActState,
    actuation: Actuation,
    next_action: CompositeTransactionRecoveryAction,
}

impl CompositeTransactionRecoveryEntry {
    #[must_use]
    pub fn resource_id(&self) -> &str {
        &self.resource_id
    }

    #[must_use]
    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    #[must_use]
    pub fn backend_instance_id(&self) -> &str {
        &self.backend_instance_id
    }

    #[must_use]
    pub const fn checkpoint(&self) -> &CompositePreActState {
        &self.checkpoint
    }

    #[must_use]
    pub const fn actuation(&self) -> &Actuation {
        &self.actuation
    }

    #[must_use]
    pub const fn next_action(&self) -> CompositeTransactionRecoveryAction {
        self.next_action
    }
}

/// Linear outstanding state when exact restoration could not be proven.
#[derive(Debug, PartialEq)]
pub struct CompositeTransactionRecoveryEnvelope {
    group_id: String,
    entries: Vec<CompositeTransactionRecoveryEntry>,
}

impl CompositeTransactionRecoveryEnvelope {
    #[must_use]
    pub fn group_id(&self) -> &str {
        &self.group_id
    }

    #[must_use]
    pub fn entries(&self) -> &[CompositeTransactionRecoveryEntry] {
        &self.entries
    }
}

/// Whether the failed composite transaction proved rollback or still needs recovery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompositeFailureDisposition {
    RolledBack,
    RecoveryRequired,
}

/// Failure evidence from a composite transaction.
#[derive(Debug, PartialEq)]
pub struct CompositeTransactionFailure {
    stage: CompositeTransactionStage,
    resource_id: Option<String>,
    detail: String,
    disposition: CompositeFailureDisposition,
    recovery_failures: Vec<CompositeRecoveryFailure>,
    recovery: Option<Box<CompositeTransactionRecoveryEnvelope>>,
}

impl CompositeTransactionFailure {
    fn with_recovery_result(
        stage: CompositeTransactionStage,
        resource_id: Option<String>,
        detail: impl Into<String>,
        result: RecoveryResult,
    ) -> Self {
        let disposition = if result.retained.is_empty() {
            CompositeFailureDisposition::RolledBack
        } else {
            CompositeFailureDisposition::RecoveryRequired
        };
        let recovery = (!result.retained.is_empty()).then_some(Box::new(
            CompositeTransactionRecoveryEnvelope {
                group_id: result.group_id,
                entries: result.retained,
            },
        ));
        Self {
            stage,
            resource_id,
            detail: detail.into(),
            disposition,
            recovery_failures: result.failures,
            recovery,
        }
    }

    #[must_use]
    pub const fn stage(&self) -> CompositeTransactionStage {
        self.stage
    }

    #[must_use]
    pub fn resource_id(&self) -> Option<&str> {
        self.resource_id.as_deref()
    }

    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }

    #[must_use]
    pub const fn disposition(&self) -> CompositeFailureDisposition {
        self.disposition
    }

    #[must_use]
    pub fn recovery_failures(&self) -> &[CompositeRecoveryFailure] {
        &self.recovery_failures
    }

    #[must_use]
    pub fn recovery(&self) -> Option<&CompositeTransactionRecoveryEnvelope> {
        self.recovery.as_deref()
    }

    #[must_use]
    pub fn into_recovery(self) -> Option<CompositeTransactionRecoveryEnvelope> {
        self.recovery.map(|recovery| *recovery)
    }
}

impl fmt::Display for CompositeTransactionFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "composite transaction failed at {}", self.stage.as_str())?;
        if let Some(resource) = &self.resource_id {
            write!(f, " for {resource}")?;
        }
        write!(f, ": {}; disposition={:?}", self.detail, self.disposition)
    }
}

impl std::error::Error for CompositeTransactionFailure {}

/// One failed recovery action; all other recoveries are still attempted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompositeRecoveryFailure {
    resource_id: String,
    action: CompositeTransactionRecoveryAction,
    detail: String,
}

impl CompositeRecoveryFailure {
    #[must_use]
    pub fn resource_id(&self) -> &str {
        &self.resource_id
    }

    #[must_use]
    pub const fn action(&self) -> CompositeTransactionRecoveryAction {
        self.action
    }

    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EntryPhase {
    Prepared,
    Actuated,
    Committed,
}

struct TransactionEntry {
    resource_id: String,
    adapter_name: String,
    backend_instance_id: String,
    checkpoint: CompositePreActState,
    actuation: Actuation,
    phase: EntryPhase,
}

struct RecoveryResult {
    group_id: String,
    failures: Vec<CompositeRecoveryFailure>,
    retained: Vec<CompositeTransactionRecoveryEntry>,
}

impl RecoveryResult {
    fn new(group_id: &str) -> Self {
        Self {
            group_id: group_id.to_owned(),
            failures: Vec::new(),
            retained: Vec::new(),
        }
    }
}

/// Execute one prepared composite transaction through ACT -> VERIFY -> COMMIT.
///
/// On any failure or cancellation before global completion, every entry is
/// recovered in reverse order. A `Committed` result is returned only after all
/// local commits succeeded. Checkpoint-release failures after that point are
/// cleanup-only and are retained in the successful commit report.
pub fn execute_composite_transaction(
    prepared: CompositePreparedEnvelope,
    backends: &mut [&mut dyn CompositeTransactionBackend],
    cancellation: &CancellationToken,
) -> Result<CompositeCommitReport, CompositeTransactionFailure> {
    let (group_id, source_plan_fingerprint, subplans) = prepared.into_parts();
    let mut entries =
        subplans
            .into_iter()
            .map(|subplan| {
                let (
                    resource_id,
                    adapter_name,
                    backend_instance_id,
                    _validated,
                    checkpoint,
                    actuation,
                ) = subplan.into_parts();
                TransactionEntry {
                    resource_id,
                    adapter_name,
                    backend_instance_id,
                    checkpoint,
                    actuation,
                    phase: EntryPhase::Prepared,
                }
            })
            .collect::<Vec<_>>();

    if let Err(detail) = validate_transaction_backends(&entries, backends) {
        let result = recover_entries(&group_id, backends, entries, "backend binding mismatch");
        return Err(CompositeTransactionFailure::with_recovery_result(
            CompositeTransactionStage::Bind,
            None,
            detail,
            result,
        ));
    }

    if cancellation.is_cancelled() {
        let result = recover_entries(&group_id, backends, entries, "cancelled before actuation");
        return Err(CompositeTransactionFailure::with_recovery_result(
            CompositeTransactionStage::Cancelled,
            None,
            "cancellation observed before first actuation",
            result,
        ));
    }

    for index in 0..entries.len() {
        if cancellation.is_cancelled() {
            let resource = entries[index].resource_id.clone();
            let result =
                recover_entries(&group_id, backends, entries, "cancelled during actuation");
            return Err(CompositeTransactionFailure::with_recovery_result(
                CompositeTransactionStage::Cancelled,
                Some(resource),
                "cancellation observed before next actuation",
                result,
            ));
        }
        let resource = entries[index].resource_id.clone();
        let backend = backend_index(backends, &resource).expect("bindings validated");
        // Conservatively mark the entry actuated before calling the backend:
        // an error may still have left a partial physical effect.
        entries[index].phase = EntryPhase::Actuated;
        if let Err(error) = backends[backend].actuate(&entries[index].actuation) {
            let result = recover_entries(
                &group_id,
                backends,
                entries,
                &format!("actuation failed: {error}"),
            );
            return Err(CompositeTransactionFailure::with_recovery_result(
                CompositeTransactionStage::Actuate,
                Some(resource),
                error.to_string(),
                result,
            ));
        }
    }

    for index in 0..entries.len() {
        if cancellation.is_cancelled() {
            let resource = entries[index].resource_id.clone();
            let result = recover_entries(
                &group_id,
                backends,
                entries,
                "cancelled before verify/commit",
            );
            return Err(CompositeTransactionFailure::with_recovery_result(
                CompositeTransactionStage::Cancelled,
                Some(resource),
                "cancellation observed after actuation and before global commit",
                result,
            ));
        }
        let resource = entries[index].resource_id.clone();
        let backend = backend_index(backends, &resource).expect("bindings validated");
        let verification = match backends[backend].verify(&entries[index].actuation) {
            Ok(result) => result,
            Err(error) => {
                let result = recover_entries(
                    &group_id,
                    backends,
                    entries,
                    &format!("verification error: {error}"),
                );
                return Err(CompositeTransactionFailure::with_recovery_result(
                    CompositeTransactionStage::Verify,
                    Some(resource),
                    error.to_string(),
                    result,
                ));
            }
        };
        if !verification.is_pass() {
            let detail = format!("verification did not pass: {verification:?}");
            let result = recover_entries(&group_id, backends, entries, &detail);
            return Err(CompositeTransactionFailure::with_recovery_result(
                CompositeTransactionStage::Verify,
                Some(resource),
                detail,
                result,
            ));
        }
    }

    let mut commits = Vec::with_capacity(entries.len());
    for index in 0..entries.len() {
        if cancellation.is_cancelled() {
            let resource = entries[index].resource_id.clone();
            let result = recover_entries(
                &group_id,
                backends,
                entries,
                "cancelled during commit window",
            );
            return Err(CompositeTransactionFailure::with_recovery_result(
                CompositeTransactionStage::Cancelled,
                Some(resource),
                "cancellation observed before all local commits completed",
                result,
            ));
        }
        let resource = entries[index].resource_id.clone();
        let backend = backend_index(backends, &resource).expect("bindings validated");
        match backends[backend].commit(&entries[index].actuation) {
            Ok(record) => {
                entries[index].phase = EntryPhase::Committed;
                commits.push(CompositeResourceCommit {
                    resource_id: resource,
                    record,
                });
            }
            Err(error) => {
                let result = recover_entries(
                    &group_id,
                    backends,
                    entries,
                    &format!("local commit failed: {error}"),
                );
                return Err(CompositeTransactionFailure::with_recovery_result(
                    CompositeTransactionStage::Commit,
                    Some(resource),
                    error.to_string(),
                    result,
                ));
            }
        }
    }

    let checkpoint_cleanup = release_committed_checkpoints(&group_id, backends, entries);
    Ok(CompositeCommitReport {
        schema_version: COMPOSITE_TRANSACTION_SCHEMA_V1,
        group_id,
        source_plan_fingerprint,
        commits,
        checkpoint_cleanup,
    })
}

/// Retry an incomplete exact-state restoration after a failed transaction.
pub fn retry_composite_transaction_recovery(
    recovery: CompositeTransactionRecoveryEnvelope,
    backends: &mut [&mut dyn CompositeTransactionBackend],
    reason: &str,
) -> Result<(), CompositeTransactionFailure> {
    if let Err(detail) = validate_recovery_backends(&recovery.entries, backends) {
        return Err(CompositeTransactionFailure {
            stage: CompositeTransactionStage::Bind,
            resource_id: None,
            detail,
            disposition: CompositeFailureDisposition::RecoveryRequired,
            recovery_failures: Vec::new(),
            recovery: Some(Box::new(recovery)),
        });
    }
    let group_id = recovery.group_id;
    let result = recover_recovery_entries(&group_id, backends, recovery.entries, reason);
    if result.retained.is_empty() {
        Ok(())
    } else {
        Err(CompositeTransactionFailure::with_recovery_result(
            CompositeTransactionStage::Restore,
            None,
            "composite transaction recovery retry remains incomplete",
            result,
        ))
    }
}

/// Retry checkpoint release after visible global commit already succeeded.
pub fn retry_composite_commit_cleanup(
    cleanup: CompositeCommitCleanupEnvelope,
    backends: &mut [&mut dyn CompositeTransactionBackend],
) -> Result<(), CompositeCommitCleanupEnvelope> {
    let mut retained = Vec::new();
    for entry in cleanup.entries {
        let Some(index) = backend_index(backends, &entry.resource_id) else {
            retained.push(entry);
            continue;
        };
        if backends[index].name() != entry.adapter_name
            || backends[index].backend_instance_id() != entry.backend_instance_id
            || backends[index]
                .release_pre_act_state(&entry.checkpoint)
                .is_err()
        {
            retained.push(entry);
        }
    }
    if retained.is_empty() {
        Ok(())
    } else {
        Err(CompositeCommitCleanupEnvelope {
            group_id: cleanup.group_id,
            entries: retained,
        })
    }
}

fn validate_transaction_backends(
    entries: &[TransactionEntry],
    backends: &[&mut dyn CompositeTransactionBackend],
) -> Result<(), String> {
    if entries.len() != backends.len() {
        return Err(format!(
            "composite transaction backend count mismatch: expected {}, got {}",
            entries.len(),
            backends.len()
        ));
    }
    let mut resources = BTreeSet::new();
    let mut instances = BTreeSet::new();
    for entry in entries {
        if !resources.insert(entry.resource_id.clone()) {
            return Err(format!("duplicate transaction entry {}", entry.resource_id));
        }
        let Some(index) = backend_index(backends, &entry.resource_id) else {
            return Err(format!("missing backend for {}", entry.resource_id));
        };
        let backend = &backends[index];
        if !instances.insert(backend.backend_instance_id().to_owned()) {
            return Err("duplicate composite backend instance identity".to_owned());
        }
        if backend.name() != entry.adapter_name
            || backend.backend_instance_id() != entry.backend_instance_id
            || entry.checkpoint.resource_id() != entry.resource_id
            || entry.checkpoint.adapter_name() != entry.adapter_name
            || entry.checkpoint.backend_instance_id() != entry.backend_instance_id
            || entry.actuation.adapter_name != entry.adapter_name
        {
            return Err(format!(
                "prepared transaction entry {} is bound to a different backend instance",
                entry.resource_id
            ));
        }
    }
    Ok(())
}

fn validate_recovery_backends(
    entries: &[CompositeTransactionRecoveryEntry],
    backends: &[&mut dyn CompositeTransactionBackend],
) -> Result<(), String> {
    if entries.len() != backends.len() {
        return Err("recovery backend count mismatch".to_owned());
    }
    for entry in entries {
        let Some(index) = backend_index(backends, &entry.resource_id) else {
            return Err(format!(
                "missing recovery backend for {}",
                entry.resource_id
            ));
        };
        if backends[index].name() != entry.adapter_name
            || backends[index].backend_instance_id() != entry.backend_instance_id
        {
            return Err(format!(
                "recovery token for {} belongs to a different backend instance",
                entry.resource_id
            ));
        }
    }
    Ok(())
}

fn backend_index(
    backends: &[&mut dyn CompositeTransactionBackend],
    resource: &str,
) -> Option<usize> {
    backends
        .iter()
        .position(|backend| backend.resource_id() == resource)
}

fn recover_entries(
    group_id: &str,
    backends: &mut [&mut dyn CompositeTransactionBackend],
    entries: Vec<TransactionEntry>,
    reason: &str,
) -> RecoveryResult {
    let recovery = entries
        .into_iter()
        .rev()
        .map(|entry| CompositeTransactionRecoveryEntry {
            resource_id: entry.resource_id,
            adapter_name: entry.adapter_name,
            backend_instance_id: entry.backend_instance_id,
            checkpoint: entry.checkpoint,
            actuation: entry.actuation,
            next_action: match entry.phase {
                EntryPhase::Prepared => CompositeTransactionRecoveryAction::AbortPrepare,
                EntryPhase::Actuated | EntryPhase::Committed => {
                    CompositeTransactionRecoveryAction::RestorePreActState
                }
            },
        })
        .collect();
    recover_recovery_entries(group_id, backends, recovery, reason)
}

fn recover_recovery_entries(
    group_id: &str,
    backends: &mut [&mut dyn CompositeTransactionBackend],
    entries: Vec<CompositeTransactionRecoveryEntry>,
    reason: &str,
) -> RecoveryResult {
    let mut result = RecoveryResult::new(group_id);
    for mut entry in entries {
        let Some(index) = backend_index(backends, &entry.resource_id) else {
            result.failures.push(CompositeRecoveryFailure {
                resource_id: entry.resource_id.clone(),
                action: entry.next_action,
                detail: "backend missing during composite recovery".to_owned(),
            });
            result.retained.push(entry);
            continue;
        };
        if backends[index].name() != entry.adapter_name
            || backends[index].backend_instance_id() != entry.backend_instance_id
        {
            result.failures.push(CompositeRecoveryFailure {
                resource_id: entry.resource_id.clone(),
                action: entry.next_action,
                detail: "backend instance mismatch during composite recovery".to_owned(),
            });
            result.retained.push(entry);
            continue;
        }

        match entry.next_action {
            CompositeTransactionRecoveryAction::AbortPrepare => {
                if let Err(error) =
                    backends[index].abort_prepare(&entry.actuation, &entry.checkpoint, reason)
                {
                    result.failures.push(CompositeRecoveryFailure {
                        resource_id: entry.resource_id.clone(),
                        action: CompositeTransactionRecoveryAction::AbortPrepare,
                        detail: error.to_string(),
                    });
                    result.retained.push(entry);
                    continue;
                }
                entry.next_action = CompositeTransactionRecoveryAction::ReleaseCheckpoint;
            }
            CompositeTransactionRecoveryAction::RestorePreActState => {
                match backends[index].restore_pre_act_state(
                    &entry.actuation,
                    &entry.checkpoint,
                    reason,
                ) {
                    Ok(record) if record.invariants_restored => {
                        entry.next_action = CompositeTransactionRecoveryAction::ReleaseCheckpoint;
                    }
                    Ok(_) => {
                        result.failures.push(CompositeRecoveryFailure {
                            resource_id: entry.resource_id.clone(),
                            action: CompositeTransactionRecoveryAction::RestorePreActState,
                            detail: "backend rollback record did not prove invariants restored"
                                .to_owned(),
                        });
                        result.retained.push(entry);
                        continue;
                    }
                    Err(error) => {
                        result.failures.push(CompositeRecoveryFailure {
                            resource_id: entry.resource_id.clone(),
                            action: CompositeTransactionRecoveryAction::RestorePreActState,
                            detail: error.to_string(),
                        });
                        result.retained.push(entry);
                        continue;
                    }
                }
            }
            CompositeTransactionRecoveryAction::ReleaseCheckpoint => {}
        }

        if let Err(error) = backends[index].release_pre_act_state(&entry.checkpoint) {
            result.failures.push(CompositeRecoveryFailure {
                resource_id: entry.resource_id.clone(),
                action: CompositeTransactionRecoveryAction::ReleaseCheckpoint,
                detail: error.to_string(),
            });
            entry.next_action = CompositeTransactionRecoveryAction::ReleaseCheckpoint;
            result.retained.push(entry);
        }
    }
    result
}

fn release_committed_checkpoints(
    group_id: &str,
    backends: &mut [&mut dyn CompositeTransactionBackend],
    entries: Vec<TransactionEntry>,
) -> Option<CompositeCommitCleanupEnvelope> {
    let mut retained = Vec::new();
    for entry in entries.into_iter().rev() {
        let Some(index) = backend_index(backends, &entry.resource_id) else {
            retained.push(CompositeCommitCleanupEntry {
                resource_id: entry.resource_id,
                adapter_name: entry.adapter_name,
                backend_instance_id: entry.backend_instance_id,
                checkpoint: entry.checkpoint,
            });
            continue;
        };
        if backends[index].name() != entry.adapter_name
            || backends[index].backend_instance_id() != entry.backend_instance_id
            || backends[index]
                .release_pre_act_state(&entry.checkpoint)
                .is_err()
        {
            retained.push(CompositeCommitCleanupEntry {
                resource_id: entry.resource_id,
                adapter_name: entry.adapter_name,
                backend_instance_id: entry.backend_instance_id,
                checkpoint: entry.checkpoint,
            });
        }
    }
    (!retained.is_empty()).then_some(CompositeCommitCleanupEnvelope {
        group_id: group_id.to_owned(),
        entries: retained,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CompositePlanEnvelope, InvariantCheck, RollbackRecord, TransactionalActuator,
        ValidatedPlan, VerificationResult,
    };
    use elastic_core::resource::{
        AdmissibleTransition, CapabilityRequirement, DimensionId, LogicalResourceId,
        ResourceClassId, ResourceDependency, ResourceGroupBuilder, ResourceGroupId, ResourceSpec,
    };
    use elastic_core::TransitionMechanism;
    use elastic_eir::{
        EirDocumentBuilder, EirGroupedDocument, FirstGroundedPlanner, PlanningContext,
        TransitionPlanner,
    };
    use std::cell::RefCell;
    use std::rc::Rc;

    fn spec(id: &str) -> ResourceSpec {
        ResourceSpec::builder(
            ResourceClassId::CONFIGURATIONAL,
            LogicalResourceId::new(id).unwrap(),
        )
        .allow(DimensionId::CAPACITY)
        .admit(AdmissibleTransition::new(
            TransitionMechanism::Reinterpret,
            DimensionId::CAPACITY,
        ))
        .require_capability(CapabilityRequirement::new(
            TransitionMechanism::Reinterpret,
            DimensionId::CAPACITY,
        ))
        .build()
        .unwrap()
    }

    fn envelope() -> CompositePlanEnvelope {
        let mut builder = EirDocumentBuilder::new();
        for id in ["base", "worker"] {
            builder.push(&spec(id)).unwrap();
        }
        let document = builder.finish().unwrap();
        let group = ResourceGroupBuilder::new(ResourceGroupId::new("stack").unwrap())
            .members([
                LogicalResourceId::new("base").unwrap(),
                LogicalResourceId::new("worker").unwrap(),
            ])
            .dependency(ResourceDependency::new(
                LogicalResourceId::new("worker").unwrap(),
                LogicalResourceId::new("base").unwrap(),
            ))
            .build()
            .unwrap();
        let grouped = EirGroupedDocument::new(document, &[group]).unwrap();
        let plans = ["worker", "base"]
            .into_iter()
            .map(|id| {
                let resource = grouped.group_resource("stack", id).unwrap();
                crate::Plan::new(
                    resource.clone(),
                    PlanningContext::new(),
                    FirstGroundedPlanner.propose_transition(resource),
                    format!("plan {id}"),
                )
            })
            .collect();
        CompositePlanEnvelope::new(&grouped, "stack", plans).unwrap()
    }

    struct Backend {
        resource: String,
        name: String,
        instance: String,
        events: Rc<RefCell<Vec<String>>>,
        visible: u64,
        prepared: bool,
        checkpoint_active: bool,
        locally_committed: bool,
        fail_act: bool,
        fail_verify: bool,
        fail_commit: bool,
        fail_restore: bool,
        fail_release: bool,
        cancel_on_act: Option<CancellationToken>,
    }

    impl Backend {
        fn new(resource: &str, events: Rc<RefCell<Vec<String>>>) -> Self {
            Self {
                resource: resource.to_owned(),
                name: format!("tx-{resource}"),
                instance: format!("instance-{resource}"),
                events,
                visible: 0,
                prepared: false,
                checkpoint_active: false,
                locally_committed: false,
                fail_act: false,
                fail_verify: false,
                fail_commit: false,
                fail_restore: false,
                fail_release: false,
                cancel_on_act: None,
            }
        }

        fn event(&self, stage: &str) {
            self.events
                .borrow_mut()
                .push(format!("{stage}:{}", self.resource));
        }
    }

    impl TransactionalActuator for Backend {
        fn name(&self) -> &str {
            &self.name
        }

        fn validate(&self, _plan: &crate::Plan) -> Result<Vec<InvariantCheck>, RuntimeError> {
            self.event("validate");
            Ok(Vec::new())
        }

        fn prepare(&mut self, plan: &ValidatedPlan) -> Result<Actuation, RuntimeError> {
            self.event("prepare");
            self.prepared = true;
            let target = plan
                .plan
                .candidate()
                .and_then(|candidate| candidate.magnitude());
            Ok(Actuation::new(plan.clone(), target, self.name.clone()))
        }

        fn actuate(&mut self, _actuation: &Actuation) -> Result<(), RuntimeError> {
            self.event("act");
            self.visible = 1;
            if let Some(token) = &self.cancel_on_act {
                token.cancel();
            }
            if self.fail_act {
                Err(RuntimeError::actuation("forced act failure"))
            } else {
                Ok(())
            }
        }

        fn verify(&self, _actuation: &Actuation) -> Result<VerificationResult, RuntimeError> {
            self.event("verify");
            if self.fail_verify {
                Ok(VerificationResult::Fail {
                    detail: "forced verify failure".into(),
                })
            } else if self.visible == 1 {
                Ok(VerificationResult::Pass)
            } else {
                Ok(VerificationResult::Fail {
                    detail: "visible state mismatch".into(),
                })
            }
        }

        fn commit(&mut self, _actuation: &Actuation) -> Result<CommitRecord, RuntimeError> {
            self.event("commit");
            if self.fail_commit {
                Err(RuntimeError::commit("forced commit failure"))
            } else {
                self.locally_committed = true;
                Ok(CommitRecord::new(&self.resource, "local composite commit"))
            }
        }

        fn rollback(
            &mut self,
            _actuation: &Actuation,
            _verification: &VerificationResult,
        ) -> Result<RollbackRecord, RuntimeError> {
            panic!("ELANG4c uses exact checkpoint restoration, not legacy rollback")
        }
    }

    impl CompositePrepareBackend for Backend {
        fn resource_id(&self) -> &str {
            &self.resource
        }
        fn backend_instance_id(&self) -> &str {
            &self.instance
        }

        fn capture_pre_act_state(
            &mut self,
            _plan: &ValidatedPlan,
        ) -> Result<CompositePreActState, RuntimeError> {
            self.event("capture");
            self.checkpoint_active = true;
            Ok(CompositePreActState::new(
                self.resource.clone(),
                self.name.clone(),
                self.instance.clone(),
                self.visible,
                self.visible ^ 0x55,
            ))
        }

        fn abort_prepare(
            &mut self,
            _actuation: &Actuation,
            _checkpoint: &CompositePreActState,
            _reason: &str,
        ) -> Result<(), RuntimeError> {
            self.event("abort");
            self.prepared = false;
            Ok(())
        }

        fn release_pre_act_state(
            &mut self,
            _checkpoint: &CompositePreActState,
        ) -> Result<(), RuntimeError> {
            self.event("release");
            if self.fail_release {
                return Err(RuntimeError::rollback("forced release failure"));
            }
            self.checkpoint_active = false;
            Ok(())
        }
    }

    impl CompositeTransactionBackend for Backend {
        fn restore_pre_act_state(
            &mut self,
            _actuation: &Actuation,
            checkpoint: &CompositePreActState,
            _reason: &str,
        ) -> Result<RollbackRecord, RuntimeError> {
            self.event("restore");
            if self.fail_restore {
                return Err(RuntimeError::rollback("forced restore failure"));
            }
            self.visible = checkpoint.generation();
            self.prepared = false;
            self.locally_committed = false;
            Ok(RollbackRecord::new(
                &self.resource,
                "restored exact checkpoint",
                true,
            ))
        }
    }

    fn prepared(base: &mut Backend, worker: &mut Backend) -> CompositePreparedEnvelope {
        let envelope = envelope();
        let mut backends: [&mut dyn CompositePrepareBackend; 2] = [base, worker];
        crate::prepare_composite_plan(&envelope, &mut backends).unwrap()
    }

    #[test]
    fn success_commits_every_resource_before_releasing_checkpoints() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut base = Backend::new("base", events.clone());
        let mut worker = Backend::new("worker", events.clone());
        let prepared = prepared(&mut base, &mut worker);
        events.borrow_mut().clear();
        let token = CancellationToken::new();
        let mut backends: [&mut dyn CompositeTransactionBackend; 2] = [&mut worker, &mut base];
        let report = execute_composite_transaction(prepared, &mut backends, &token).unwrap();
        assert_eq!(report.commits().len(), 2);
        assert!(report.checkpoint_cleanup().is_none());
        assert_eq!(base.visible, 1);
        assert_eq!(worker.visible, 1);
        assert!(base.locally_committed && worker.locally_committed);
        assert_eq!(
            events.borrow().as_slice(),
            [
                "act:base",
                "act:worker",
                "verify:base",
                "verify:worker",
                "commit:base",
                "commit:worker",
                "release:worker",
                "release:base"
            ]
        );
    }

    #[test]
    fn partial_actuation_error_restores_current_and_prior_resources() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut base = Backend::new("base", events.clone());
        let mut worker = Backend::new("worker", events.clone());
        worker.fail_act = true;
        let prepared = prepared(&mut base, &mut worker);
        events.borrow_mut().clear();
        let mut backends: [&mut dyn CompositeTransactionBackend; 2] = [&mut base, &mut worker];
        let failure =
            execute_composite_transaction(prepared, &mut backends, &CancellationToken::new())
                .unwrap_err();
        assert_eq!(failure.stage(), CompositeTransactionStage::Actuate);
        assert_eq!(
            failure.disposition(),
            CompositeFailureDisposition::RolledBack
        );
        assert!(failure.recovery().is_none());
        assert_eq!((base.visible, worker.visible), (0, 0));
        assert_eq!(
            events.borrow().as_slice(),
            [
                "act:base",
                "act:worker",
                "restore:worker",
                "release:worker",
                "restore:base",
                "release:base"
            ]
        );
    }

    #[test]
    fn verification_failure_prevents_every_commit_and_restores_all() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut base = Backend::new("base", events.clone());
        let mut worker = Backend::new("worker", events.clone());
        worker.fail_verify = true;
        let prepared = prepared(&mut base, &mut worker);
        events.borrow_mut().clear();
        let mut backends: [&mut dyn CompositeTransactionBackend; 2] = [&mut base, &mut worker];
        let failure =
            execute_composite_transaction(prepared, &mut backends, &CancellationToken::new())
                .unwrap_err();
        assert_eq!(failure.stage(), CompositeTransactionStage::Verify);
        assert_eq!(
            failure.disposition(),
            CompositeFailureDisposition::RolledBack
        );
        assert_eq!((base.visible, worker.visible), (0, 0));
        assert!(!base.locally_committed && !worker.locally_committed);
    }

    #[test]
    fn later_commit_failure_restores_earlier_successful_local_commit() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut base = Backend::new("base", events.clone());
        let mut worker = Backend::new("worker", events.clone());
        worker.fail_commit = true;
        let prepared = prepared(&mut base, &mut worker);
        events.borrow_mut().clear();
        let mut backends: [&mut dyn CompositeTransactionBackend; 2] = [&mut base, &mut worker];
        let failure =
            execute_composite_transaction(prepared, &mut backends, &CancellationToken::new())
                .unwrap_err();
        assert_eq!(failure.stage(), CompositeTransactionStage::Commit);
        assert_eq!(
            failure.disposition(),
            CompositeFailureDisposition::RolledBack
        );
        assert_eq!((base.visible, worker.visible), (0, 0));
        assert!(!base.locally_committed && !worker.locally_committed);
        assert!(events.borrow().iter().any(|event| event == "commit:base"));
        assert!(events.borrow().iter().any(|event| event == "restore:base"));
    }

    #[test]
    fn restore_failure_returns_linear_recovery_and_retry_finishes() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut base = Backend::new("base", events.clone());
        let mut worker = Backend::new("worker", events.clone());
        worker.fail_verify = true;
        base.fail_restore = true;
        let prepared = prepared(&mut base, &mut worker);
        let mut backends: [&mut dyn CompositeTransactionBackend; 2] = [&mut base, &mut worker];
        let failure =
            execute_composite_transaction(prepared, &mut backends, &CancellationToken::new())
                .unwrap_err();
        assert_eq!(
            failure.disposition(),
            CompositeFailureDisposition::RecoveryRequired
        );
        let recovery = failure.into_recovery().unwrap();
        assert_eq!(recovery.entries().len(), 1);
        assert_eq!(recovery.entries()[0].resource_id(), "base");
        assert_eq!(
            recovery.entries()[0].next_action(),
            CompositeTransactionRecoveryAction::RestorePreActState
        );
        base.fail_restore = false;
        let mut retry: [&mut dyn CompositeTransactionBackend; 1] = [&mut base];
        retry_composite_transaction_recovery(recovery, &mut retry, "retry restore").unwrap();
        assert_eq!(base.visible, 0);
        assert!(!base.checkpoint_active);
    }

    #[test]
    fn cancellation_between_actuations_rolls_back_acted_and_aborts_unacted() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let token = CancellationToken::new();
        let mut base = Backend::new("base", events.clone());
        let mut worker = Backend::new("worker", events.clone());
        base.cancel_on_act = Some(token.clone());
        let prepared = prepared(&mut base, &mut worker);
        events.borrow_mut().clear();
        let mut backends: [&mut dyn CompositeTransactionBackend; 2] = [&mut base, &mut worker];
        let failure = execute_composite_transaction(prepared, &mut backends, &token).unwrap_err();
        assert_eq!(failure.stage(), CompositeTransactionStage::Cancelled);
        assert_eq!(
            failure.disposition(),
            CompositeFailureDisposition::RolledBack
        );
        assert_eq!((base.visible, worker.visible), (0, 0));
        assert_eq!(
            events.borrow().as_slice(),
            [
                "act:base",
                "abort:worker",
                "release:worker",
                "restore:base",
                "release:base"
            ]
        );
    }

    #[test]
    fn post_commit_checkpoint_release_failure_does_not_turn_commit_into_rollback() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut base = Backend::new("base", events.clone());
        let mut worker = Backend::new("worker", events.clone());
        base.fail_release = true;
        let prepared = prepared(&mut base, &mut worker);
        let mut backends: [&mut dyn CompositeTransactionBackend; 2] = [&mut base, &mut worker];
        let report =
            execute_composite_transaction(prepared, &mut backends, &CancellationToken::new())
                .unwrap();
        assert_eq!((base.visible, worker.visible), (1, 1));
        let cleanup = report.into_checkpoint_cleanup().unwrap();
        assert_eq!(cleanup.entries().len(), 1);
        assert_eq!(cleanup.entries()[0].resource_id(), "base");
        base.fail_release = false;
        let mut retry: [&mut dyn CompositeTransactionBackend; 1] = [&mut base];
        retry_composite_commit_cleanup(cleanup, &mut retry).unwrap();
        assert!(!base.checkpoint_active);
    }
}
