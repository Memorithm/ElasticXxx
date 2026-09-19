//! ELANG4b composite prepare boundary and pre-actuation checkpoints.
//!
//! This module deliberately stops before physical actuation. It takes an
//! already validated [`crate::CompositePlanEnvelope`], requires one trusted
//! backend per targeted resource, performs all trusted validations first,
//! captures every pre-actuation state second, and only then prepares every
//! concrete actuation. A prepare failure aborts earlier preparations in reverse
//! order and releases every captured checkpoint.
//!
//! A successful [`CompositePreparedEnvelope`] is **not** actuation authority.
//! ELANG4c owns coordinated act/verify/commit-or-rollback semantics.

use std::collections::BTreeSet;
use std::fmt;

use elastic_eir::Fingerprint;

use crate::plan::validate_with_checks;
use crate::{Actuation, CompositePlanEnvelope, RuntimeError, TransactionalActuator, ValidatedPlan};

/// Schema version of the composite prepare/checkpoint contract.
pub const COMPOSITE_PREPARE_SCHEMA_V1: u16 = 1;

/// Opaque binding to backend-owned state captured immediately before prepare.
///
/// The generic runtime records identity only; the backend retains whatever
/// concrete bytes/handles are necessary for later rollback. The two numeric
/// fields are backend-defined diagnostics, not cryptographic authentication.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompositePreActState {
    resource_id: String,
    adapter_name: String,
    backend_instance_id: String,
    generation: u64,
    state_fingerprint: u64,
}

impl CompositePreActState {
    /// Construct a backend-owned pre-actuation state token.
    #[must_use]
    pub fn new(
        resource_id: impl Into<String>,
        adapter_name: impl Into<String>,
        backend_instance_id: impl Into<String>,
        generation: u64,
        state_fingerprint: u64,
    ) -> Self {
        Self {
            resource_id: resource_id.into(),
            adapter_name: adapter_name.into(),
            backend_instance_id: backend_instance_id.into(),
            generation,
            state_fingerprint,
        }
    }

    #[must_use]
    pub fn resource_id(&self) -> &str {
        &self.resource_id
    }

    #[must_use]
    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    /// Stable backend-issued identity of the concrete state owner.
    #[must_use]
    pub fn backend_instance_id(&self) -> &str {
        &self.backend_instance_id
    }

    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    pub const fn state_fingerprint(&self) -> u64 {
        self.state_fingerprint
    }
}

/// Additional trusted boundary required for a resource to participate in a
/// composite prepare phase.
///
/// Implementations inherit ordinary single-resource trusted validation and
/// preparation from [`TransactionalActuator`]. They additionally capture and
/// release backend-owned rollback state and must be able to undo a preparation
/// that never reached physical actuation.
pub trait CompositePrepareBackend: TransactionalActuator {
    /// Exact logical resource this backend controls.
    fn resource_id(&self) -> &str;

    /// Stable unique identity of this concrete backend instance.
    ///
    /// Replacements that expose the same resource and adapter name must use a
    /// different identity. The value must remain stable while recovery tokens
    /// issued by this instance can still exist.
    fn backend_instance_id(&self) -> &str;

    /// Capture rollback-relevant state without changing visible resource state.
    fn capture_pre_act_state(
        &mut self,
        plan: &ValidatedPlan,
    ) -> Result<CompositePreActState, RuntimeError>;

    /// Undo non-physical preparation/reservation after a later prepare failure
    /// or explicit cancellation. This must not assume physical actuation ran.
    fn abort_prepare(
        &mut self,
        actuation: &Actuation,
        checkpoint: &CompositePreActState,
        reason: &str,
    ) -> Result<(), RuntimeError>;

    /// Release backend-owned checkpoint state after prepare abort/cancellation.
    fn release_pre_act_state(
        &mut self,
        checkpoint: &CompositePreActState,
    ) -> Result<(), RuntimeError>;
}

/// One fully validated, checkpointed and prepared subplan.
#[derive(Debug, PartialEq)]
pub struct CompositePreparedSubplan {
    resource_id: String,
    adapter_name: String,
    backend_instance_id: String,
    validated_plan: ValidatedPlan,
    checkpoint: CompositePreActState,
    actuation: Actuation,
}

impl CompositePreparedSubplan {
    #[must_use]
    pub fn resource_id(&self) -> &str {
        &self.resource_id
    }

    /// Trusted adapter identity that produced this prepared state.
    #[must_use]
    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    /// Concrete backend instance that owns the opaque checkpoint/actuation.
    #[must_use]
    pub fn backend_instance_id(&self) -> &str {
        &self.backend_instance_id
    }

    #[must_use]
    pub const fn validated_plan(&self) -> &ValidatedPlan {
        &self.validated_plan
    }

    #[must_use]
    pub const fn checkpoint(&self) -> &CompositePreActState {
        &self.checkpoint
    }

    #[must_use]
    pub const fn actuation(&self) -> &Actuation {
        &self.actuation
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        String,
        String,
        String,
        ValidatedPlan,
        CompositePreActState,
        Actuation,
    ) {
        (
            self.resource_id,
            self.adapter_name,
            self.backend_instance_id,
            self.validated_plan,
            self.checkpoint,
            self.actuation,
        )
    }
}

/// Composite envelope after validation, checkpoint capture and prepare only.
#[derive(Debug, PartialEq)]
pub struct CompositePreparedEnvelope {
    schema_version: u16,
    group_id: String,
    source_plan_fingerprint: Fingerprint,
    subplans: Vec<CompositePreparedSubplan>,
}

impl CompositePreparedEnvelope {
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

    /// Prepared entries remain in forward composite execution order.
    #[must_use]
    pub fn subplans(&self) -> &[CompositePreparedSubplan] {
        &self.subplans
    }

    /// Prepared resources in reverse order for abort/rollback coordination.
    pub fn reverse_resources(&self) -> impl Iterator<Item = &str> {
        self.subplans.iter().rev().map(|entry| entry.resource_id())
    }

    pub(crate) fn into_parts(self) -> (String, Fingerprint, Vec<CompositePreparedSubplan>) {
        (self.group_id, self.source_plan_fingerprint, self.subplans)
    }
}

/// One retained cleanup token after a failed prepare/abort cleanup.
///
/// When `actuation` is present the backend preparation must be aborted before
/// the checkpoint may be released. When it is absent only checkpoint release
/// remains. This type is intentionally non-`Clone` so the generic retry API is
/// a one-shot state transition.
#[derive(Debug, PartialEq)]
pub struct CompositePrepareRecoveryEntry {
    resource_id: String,
    adapter_name: String,
    backend_instance_id: String,
    checkpoint: CompositePreActState,
    actuation: Option<Actuation>,
}

impl CompositePrepareRecoveryEntry {
    #[must_use]
    pub fn resource_id(&self) -> &str {
        &self.resource_id
    }

    #[must_use]
    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    /// Concrete backend instance required to consume this recovery entry.
    #[must_use]
    pub fn backend_instance_id(&self) -> &str {
        &self.backend_instance_id
    }

    #[must_use]
    pub const fn checkpoint(&self) -> &CompositePreActState {
        &self.checkpoint
    }

    #[must_use]
    pub const fn actuation(&self) -> Option<&Actuation> {
        self.actuation.as_ref()
    }
}

/// Linear recovery state retained when composite prepare cleanup could not
/// fully finish. Pass it to [`retry_composite_prepare_cleanup`]; do not discard
/// it while the backend still owns reservation/checkpoint state.
#[derive(Debug, PartialEq)]
pub struct CompositePrepareRecoveryEnvelope {
    group_id: String,
    entries: Vec<CompositePrepareRecoveryEntry>,
}

impl CompositePrepareRecoveryEnvelope {
    #[must_use]
    pub fn group_id(&self) -> &str {
        &self.group_id
    }

    /// Outstanding cleanup entries in reverse composite cleanup order.
    #[must_use]
    pub fn entries(&self) -> &[CompositePrepareRecoveryEntry] {
        &self.entries
    }

    fn from_prepared(prepared: CompositePreparedEnvelope) -> Self {
        let entries = prepared
            .subplans
            .into_iter()
            .rev()
            .map(|entry| CompositePrepareRecoveryEntry {
                resource_id: entry.resource_id,
                adapter_name: entry.adapter_name,
                backend_instance_id: entry.backend_instance_id,
                checkpoint: entry.checkpoint,
                actuation: Some(entry.actuation),
            })
            .collect();
        Self {
            group_id: prepared.group_id,
            entries,
        }
    }
}

/// Cleanup operation that failed while unwinding a non-actuating prepare phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompositePrepareCleanupOperation {
    AbortPrepare,
    ReleaseCheckpoint,
}

impl CompositePrepareCleanupOperation {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AbortPrepare => "abort-prepare",
            Self::ReleaseCheckpoint => "release-checkpoint",
        }
    }
}

/// One cleanup failure collected while still attempting all remaining cleanup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompositePrepareCleanupFailure {
    resource_id: String,
    operation: CompositePrepareCleanupOperation,
    detail: String,
}

impl CompositePrepareCleanupFailure {
    #[must_use]
    pub fn resource_id(&self) -> &str {
        &self.resource_id
    }

    #[must_use]
    pub const fn operation(&self) -> CompositePrepareCleanupOperation {
        self.operation
    }

    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

/// Stage at which the composite prepare boundary failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompositePrepareStage {
    Bind,
    Validate,
    Capture,
    Prepare,
    Abort,
}

impl CompositePrepareStage {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bind => "bind",
            Self::Validate => "validate",
            Self::Capture => "capture",
            Self::Prepare => "prepare",
            Self::Abort => "abort",
        }
    }
}

/// Fail-closed composite prepare failure with best-effort cleanup evidence.
#[derive(Debug, PartialEq)]
pub struct CompositePrepareFailure {
    stage: CompositePrepareStage,
    resource_id: Option<String>,
    detail: String,
    cleanup_failures: Vec<CompositePrepareCleanupFailure>,
    recovery: Option<Box<CompositePrepareRecoveryEnvelope>>,
}

impl CompositePrepareFailure {
    fn new(
        stage: CompositePrepareStage,
        resource_id: Option<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            stage,
            resource_id,
            detail: detail.into(),
            cleanup_failures: Vec::new(),
            recovery: None,
        }
    }

    fn with_recovery(mut self, recovery: CompositePrepareRecoveryEnvelope) -> Self {
        self.recovery = Some(Box::new(recovery));
        self
    }

    fn with_cleanup_result(mut self, cleanup: CleanupResult) -> Self {
        self.cleanup_failures = cleanup.failures;
        if !cleanup.retained.is_empty() {
            self.recovery = Some(Box::new(CompositePrepareRecoveryEnvelope {
                group_id: cleanup.group_id,
                entries: cleanup.retained,
            }));
        }
        self
    }

    #[must_use]
    pub const fn stage(&self) -> CompositePrepareStage {
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
    pub fn cleanup_failures(&self) -> &[CompositePrepareCleanupFailure] {
        &self.cleanup_failures
    }

    /// Outstanding cleanup state retained for a later one-shot retry.
    #[must_use]
    pub fn recovery(&self) -> Option<&CompositePrepareRecoveryEnvelope> {
        self.recovery.as_deref()
    }

    /// Consume the failure and recover ownership of any outstanding cleanup state.
    #[must_use]
    pub fn into_recovery(self) -> Option<CompositePrepareRecoveryEnvelope> {
        self.recovery.map(|recovery| *recovery)
    }
}

impl fmt::Display for CompositePrepareFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "composite prepare failed at {}", self.stage.as_str())?;
        if let Some(resource) = &self.resource_id {
            write!(f, " for {resource}")?;
        }
        write!(f, ": {}", self.detail)?;
        if !self.cleanup_failures.is_empty() {
            write!(f, "; {} cleanup failure(s)", self.cleanup_failures.len())?;
        }
        Ok(())
    }
}

impl std::error::Error for CompositePrepareFailure {}

struct CleanupResult {
    group_id: String,
    failures: Vec<CompositePrepareCleanupFailure>,
    retained: Vec<CompositePrepareRecoveryEntry>,
}

impl CleanupResult {
    fn new(group_id: &str) -> Self {
        Self {
            group_id: group_id.to_owned(),
            failures: Vec::new(),
            retained: Vec::new(),
        }
    }

    fn merge(&mut self, mut other: Self) {
        self.failures.append(&mut other.failures);
        self.retained.append(&mut other.retained);
    }

    fn is_clean(&self) -> bool {
        self.failures.is_empty() && self.retained.is_empty()
    }
}

/// Validate, checkpoint and prepare all targeted resources without actuating any.
pub fn prepare_composite_plan(
    envelope: &CompositePlanEnvelope,
    backends: &mut [&mut dyn CompositePrepareBackend],
) -> Result<CompositePreparedEnvelope, CompositePrepareFailure> {
    validate_backend_inventory(envelope, backends)?;

    // Phase 1: every trusted validation must pass before any checkpoint or
    // preparation is requested from any backend.
    let mut validated = Vec::with_capacity(envelope.subplans().len());
    for plan in envelope.subplans() {
        let resource = plan.resource.identity().as_str();
        let index = backend_index(backends, resource).ok_or_else(|| {
            CompositePrepareFailure::new(
                CompositePrepareStage::Bind,
                Some(resource.to_owned()),
                "no trusted composite backend is bound to this resource",
            )
        })?;
        let checks = backends[index].validate(plan).map_err(|error| {
            CompositePrepareFailure::new(
                CompositePrepareStage::Validate,
                Some(resource.to_owned()),
                error.to_string(),
            )
        })?;
        let validated_plan = validate_with_checks(plan.clone(), checks);
        if !validated_plan.validated {
            return Err(CompositePrepareFailure::new(
                CompositePrepareStage::Validate,
                Some(resource.to_owned()),
                "trusted invariant validation did not authorize preparation",
            ));
        }
        validated.push(validated_plan);
    }

    // Phase 2: capture all rollback-relevant state before the first prepare.
    let mut checkpoints = Vec::with_capacity(validated.len());
    for plan in &validated {
        let resource = plan.plan.resource.identity().as_str();
        let index = backend_index(backends, resource).expect("inventory was validated");
        let checkpoint = match backends[index].capture_pre_act_state(plan) {
            Ok(checkpoint) => checkpoint,
            Err(error) => {
                let cleanup = release_checkpoints(envelope.group_id(), backends, &checkpoints);
                return Err(CompositePrepareFailure::new(
                    CompositePrepareStage::Capture,
                    Some(resource.to_owned()),
                    error.to_string(),
                )
                .with_cleanup_result(cleanup));
            }
        };
        if checkpoint.resource_id() != resource
            || checkpoint.adapter_name() != backends[index].name()
            || checkpoint.backend_instance_id() != backends[index].backend_instance_id()
        {
            let mut cleanup = CleanupResult::new(envelope.group_id());
            if let Err(error) = backends[index].release_pre_act_state(&checkpoint) {
                cleanup.failures.push(CompositePrepareCleanupFailure {
                    resource_id: resource.to_owned(),
                    operation: CompositePrepareCleanupOperation::ReleaseCheckpoint,
                    detail: error.to_string(),
                });
                cleanup.retained.push(CompositePrepareRecoveryEntry {
                    resource_id: resource.to_owned(),
                    adapter_name: backends[index].name().to_owned(),
                    backend_instance_id: backends[index].backend_instance_id().to_owned(),
                    checkpoint,
                    actuation: None,
                });
            }
            cleanup.merge(release_checkpoints(
                envelope.group_id(),
                backends,
                &checkpoints,
            ));
            return Err(CompositePrepareFailure::new(
                CompositePrepareStage::Capture,
                Some(resource.to_owned()),
                "pre-actuation checkpoint binding does not match resource/backend",
            )
            .with_cleanup_result(cleanup));
        }
        checkpoints.push(checkpoint);
    }

    // Phase 3: concrete preparation is allowed only after every checkpoint was
    // captured. Physical actuation remains outside this function.
    let mut prepared = Vec::<CompositePreparedSubplan>::with_capacity(validated.len());
    for (validated_plan, checkpoint) in validated.into_iter().zip(checkpoints.iter().cloned()) {
        let resource = validated_plan.plan.resource.identity().as_str().to_owned();
        let index = backend_index(backends, &resource).expect("inventory was validated");
        let actuation = match backends[index].prepare(&validated_plan) {
            Ok(actuation) => actuation,
            Err(error) => {
                let cleanup = cleanup_prepared(
                    envelope.group_id(),
                    backends,
                    &prepared,
                    &checkpoints,
                    &error.to_string(),
                );
                return Err(CompositePrepareFailure::new(
                    CompositePrepareStage::Prepare,
                    Some(resource),
                    error.to_string(),
                )
                .with_cleanup_result(cleanup));
            }
        };
        if !actuation.is_valid()
            || actuation.plan != validated_plan
            || actuation.adapter_name != backends[index].name()
        {
            let current = CompositePreparedSubplan {
                resource_id: resource.clone(),
                adapter_name: backends[index].name().to_owned(),
                backend_instance_id: backends[index].backend_instance_id().to_owned(),
                validated_plan,
                checkpoint,
                actuation,
            };
            prepared.push(current);
            let cleanup = cleanup_prepared(
                envelope.group_id(),
                backends,
                &prepared,
                &checkpoints,
                "prepared actuation binding is invalid",
            );
            return Err(CompositePrepareFailure::new(
                CompositePrepareStage::Prepare,
                Some(resource),
                "prepared actuation does not match the validated plan/backend",
            )
            .with_cleanup_result(cleanup));
        }
        prepared.push(CompositePreparedSubplan {
            resource_id: resource,
            adapter_name: backends[index].name().to_owned(),
            backend_instance_id: backends[index].backend_instance_id().to_owned(),
            validated_plan,
            checkpoint,
            actuation,
        });
    }

    Ok(CompositePreparedEnvelope {
        schema_version: COMPOSITE_PREPARE_SCHEMA_V1,
        group_id: envelope.group_id().to_owned(),
        source_plan_fingerprint: envelope.fingerprint(),
        subplans: prepared,
    })
}

/// Explicitly abort a successful composite prepare without physical actuation.
///
/// The prepared envelope is consumed. This makes cleanup a one-shot state
/// transition at the public API boundary. If cleanup cannot finish, the error
/// returns a linear [`CompositePrepareRecoveryEnvelope`] for an explicit retry.
pub fn abort_composite_prepare(
    prepared: CompositePreparedEnvelope,
    backends: &mut [&mut dyn CompositePrepareBackend],
    reason: &str,
) -> Result<(), CompositePrepareFailure> {
    if let Err(failure) = validate_prepared_backend_inventory(&prepared, backends) {
        return Err(
            failure.with_recovery(CompositePrepareRecoveryEnvelope::from_prepared(prepared))
        );
    }
    let checkpoints = prepared
        .subplans
        .iter()
        .map(|entry| entry.checkpoint.clone())
        .collect::<Vec<_>>();
    let cleanup = cleanup_prepared(
        prepared.group_id(),
        backends,
        &prepared.subplans,
        &checkpoints,
        reason,
    );
    if cleanup.is_clean() {
        Ok(())
    } else {
        Err(CompositePrepareFailure::new(
            CompositePrepareStage::Abort,
            None,
            "one or more prepared resources could not be fully released",
        )
        .with_cleanup_result(cleanup))
    }
}

/// Retry only the cleanup operations retained by a previous prepare/abort
/// failure. The recovery envelope is consumed and a new reduced envelope is
/// returned only when some cleanup still cannot be proven complete.
pub fn retry_composite_prepare_cleanup(
    recovery: CompositePrepareRecoveryEnvelope,
    backends: &mut [&mut dyn CompositePrepareBackend],
    reason: &str,
) -> Result<(), CompositePrepareFailure> {
    if let Err(failure) = validate_recovery_backend_inventory(&recovery, backends) {
        return Err(failure.with_recovery(recovery));
    }

    let group_id = recovery.group_id.clone();
    let mut cleanup = CleanupResult::new(&group_id);
    for mut entry in recovery.entries {
        let index =
            backend_index(backends, &entry.resource_id).expect("recovery inventory validated");
        if let Some(actuation) = entry.actuation.as_ref() {
            if let Err(error) = backends[index].abort_prepare(actuation, &entry.checkpoint, reason)
            {
                cleanup.failures.push(CompositePrepareCleanupFailure {
                    resource_id: entry.resource_id.clone(),
                    operation: CompositePrepareCleanupOperation::AbortPrepare,
                    detail: error.to_string(),
                });
                cleanup.retained.push(entry);
                continue;
            }
            entry.actuation = None;
        }

        if let Err(error) = backends[index].release_pre_act_state(&entry.checkpoint) {
            cleanup.failures.push(CompositePrepareCleanupFailure {
                resource_id: entry.resource_id.clone(),
                operation: CompositePrepareCleanupOperation::ReleaseCheckpoint,
                detail: error.to_string(),
            });
            cleanup.retained.push(entry);
        }
    }

    if cleanup.is_clean() {
        Ok(())
    } else {
        Err(CompositePrepareFailure::new(
            CompositePrepareStage::Abort,
            None,
            "composite prepare cleanup retry remains incomplete",
        )
        .with_cleanup_result(cleanup))
    }
}

fn validate_backend_inventory(
    envelope: &CompositePlanEnvelope,
    backends: &[&mut dyn CompositePrepareBackend],
) -> Result<(), CompositePrepareFailure> {
    let expected = envelope
        .execution_order()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    validate_backend_set(&expected, backends)
}

fn validate_prepared_backend_inventory(
    prepared: &CompositePreparedEnvelope,
    backends: &[&mut dyn CompositePrepareBackend],
) -> Result<(), CompositePrepareFailure> {
    let expected = prepared
        .subplans
        .iter()
        .map(|entry| entry.resource_id.clone())
        .collect::<BTreeSet<_>>();
    validate_backend_set(&expected, backends)?;
    for entry in &prepared.subplans {
        let index = backend_index(backends, entry.resource_id()).expect("backend set validated");
        let backend = &backends[index];
        if entry.adapter_name() != backend.name()
            || entry.backend_instance_id() != backend.backend_instance_id()
            || entry.checkpoint.resource_id() != entry.resource_id()
            || entry.checkpoint.adapter_name() != entry.adapter_name()
            || entry.checkpoint.backend_instance_id() != entry.backend_instance_id()
            || entry.actuation.adapter_name != entry.adapter_name()
        {
            return Err(CompositePrepareFailure::new(
                CompositePrepareStage::Bind,
                Some(entry.resource_id().to_owned()),
                "prepared checkpoint/actuation is bound to a different adapter",
            ));
        }
    }
    Ok(())
}

fn validate_recovery_backend_inventory(
    recovery: &CompositePrepareRecoveryEnvelope,
    backends: &[&mut dyn CompositePrepareBackend],
) -> Result<(), CompositePrepareFailure> {
    let expected = recovery
        .entries
        .iter()
        .map(|entry| entry.resource_id.clone())
        .collect::<BTreeSet<_>>();
    validate_backend_set(&expected, backends)?;
    for entry in &recovery.entries {
        let index = backend_index(backends, &entry.resource_id).expect("backend set validated");
        if backends[index].name() != entry.adapter_name
            || backends[index].backend_instance_id() != entry.backend_instance_id
        {
            return Err(CompositePrepareFailure::new(
                CompositePrepareStage::Bind,
                Some(entry.resource_id.clone()),
                "recovery state is bound to a different backend instance",
            ));
        }
    }
    Ok(())
}

fn validate_backend_set(
    expected: &BTreeSet<String>,
    backends: &[&mut dyn CompositePrepareBackend],
) -> Result<(), CompositePrepareFailure> {
    if backends.len() != expected.len() {
        return Err(CompositePrepareFailure::new(
            CompositePrepareStage::Bind,
            None,
            format!(
                "composite backend count mismatch: expected {}, got {}",
                expected.len(),
                backends.len()
            ),
        ));
    }
    let mut observed = BTreeSet::new();
    let mut observed_instances = BTreeSet::new();
    for backend in backends {
        let resource = backend.resource_id().to_owned();
        let instance = backend.backend_instance_id();
        if instance.is_empty() {
            return Err(CompositePrepareFailure::new(
                CompositePrepareStage::Bind,
                Some(resource),
                "trusted backend instance identity must not be empty",
            ));
        }
        if !observed_instances.insert(instance.to_owned()) {
            return Err(CompositePrepareFailure::new(
                CompositePrepareStage::Bind,
                Some(resource),
                "duplicate trusted backend instance identity",
            ));
        }
        if !observed.insert(resource.clone()) {
            return Err(CompositePrepareFailure::new(
                CompositePrepareStage::Bind,
                Some(resource),
                "duplicate trusted backend binding",
            ));
        }
        if !expected.contains(&resource) {
            return Err(CompositePrepareFailure::new(
                CompositePrepareStage::Bind,
                Some(resource),
                "trusted backend is not targeted by this composite envelope",
            ));
        }
    }
    if observed != *expected {
        let missing = expected.difference(&observed).next().cloned();
        return Err(CompositePrepareFailure::new(
            CompositePrepareStage::Bind,
            missing,
            "missing trusted backend binding",
        ));
    }
    Ok(())
}

fn backend_index(backends: &[&mut dyn CompositePrepareBackend], resource: &str) -> Option<usize> {
    backends
        .iter()
        .position(|backend| backend.resource_id() == resource)
}

fn release_checkpoints(
    group_id: &str,
    backends: &mut [&mut dyn CompositePrepareBackend],
    checkpoints: &[CompositePreActState],
) -> CleanupResult {
    let mut cleanup = CleanupResult::new(group_id);
    for checkpoint in checkpoints.iter().rev() {
        let resource = checkpoint.resource_id().to_owned();
        let Some(index) = backend_index(backends, &resource) else {
            cleanup.failures.push(CompositePrepareCleanupFailure {
                resource_id: resource.clone(),
                operation: CompositePrepareCleanupOperation::ReleaseCheckpoint,
                detail: "checkpoint backend disappeared during cleanup".to_owned(),
            });
            cleanup.retained.push(CompositePrepareRecoveryEntry {
                resource_id: resource,
                adapter_name: checkpoint.adapter_name().to_owned(),
                backend_instance_id: checkpoint.backend_instance_id().to_owned(),
                checkpoint: checkpoint.clone(),
                actuation: None,
            });
            continue;
        };
        if backends[index].name() != checkpoint.adapter_name()
            || backends[index].backend_instance_id() != checkpoint.backend_instance_id()
        {
            cleanup.failures.push(CompositePrepareCleanupFailure {
                resource_id: resource.clone(),
                operation: CompositePrepareCleanupOperation::ReleaseCheckpoint,
                detail: "checkpoint belongs to a different adapter; release refused".to_owned(),
            });
            cleanup.retained.push(CompositePrepareRecoveryEntry {
                resource_id: resource,
                adapter_name: checkpoint.adapter_name().to_owned(),
                backend_instance_id: checkpoint.backend_instance_id().to_owned(),
                checkpoint: checkpoint.clone(),
                actuation: None,
            });
            continue;
        }
        if let Err(error) = backends[index].release_pre_act_state(checkpoint) {
            cleanup.failures.push(CompositePrepareCleanupFailure {
                resource_id: resource.clone(),
                operation: CompositePrepareCleanupOperation::ReleaseCheckpoint,
                detail: error.to_string(),
            });
            cleanup.retained.push(CompositePrepareRecoveryEntry {
                resource_id: resource,
                adapter_name: checkpoint.adapter_name().to_owned(),
                backend_instance_id: checkpoint.backend_instance_id().to_owned(),
                checkpoint: checkpoint.clone(),
                actuation: None,
            });
        }
    }
    cleanup
}

fn cleanup_prepared(
    group_id: &str,
    backends: &mut [&mut dyn CompositePrepareBackend],
    prepared: &[CompositePreparedSubplan],
    checkpoints: &[CompositePreActState],
    reason: &str,
) -> CleanupResult {
    let mut cleanup = CleanupResult::new(group_id);
    let mut retain_without_release = BTreeSet::new();

    for entry in prepared.iter().rev() {
        let resource = entry.resource_id().to_owned();
        let Some(index) = backend_index(backends, &resource) else {
            cleanup.failures.push(CompositePrepareCleanupFailure {
                resource_id: resource.clone(),
                operation: CompositePrepareCleanupOperation::AbortPrepare,
                detail: "prepared backend disappeared during cleanup".to_owned(),
            });
            retain_without_release.insert(resource.clone());
            cleanup.retained.push(recovery_from_prepared(entry));
            continue;
        };
        if backends[index].name() != entry.adapter_name()
            || backends[index].backend_instance_id() != entry.backend_instance_id()
        {
            cleanup.failures.push(CompositePrepareCleanupFailure {
                resource_id: resource.clone(),
                operation: CompositePrepareCleanupOperation::AbortPrepare,
                detail: "prepared state belongs to a different adapter; cleanup refused".to_owned(),
            });
            retain_without_release.insert(resource.clone());
            cleanup.retained.push(recovery_from_prepared(entry));
            continue;
        }
        if let Err(error) =
            backends[index].abort_prepare(entry.actuation(), entry.checkpoint(), reason)
        {
            cleanup.failures.push(CompositePrepareCleanupFailure {
                resource_id: resource.clone(),
                operation: CompositePrepareCleanupOperation::AbortPrepare,
                detail: error.to_string(),
            });
            // Critical: do not release this checkpoint. The failed abort may be
            // transient and still needs the same backend-owned recovery state.
            retain_without_release.insert(resource.clone());
            cleanup.retained.push(recovery_from_prepared(entry));
        }
    }

    for checkpoint in checkpoints.iter().rev() {
        if retain_without_release.contains(checkpoint.resource_id()) {
            continue;
        }
        let resource = checkpoint.resource_id().to_owned();
        let Some(index) = backend_index(backends, &resource) else {
            cleanup.failures.push(CompositePrepareCleanupFailure {
                resource_id: resource.clone(),
                operation: CompositePrepareCleanupOperation::ReleaseCheckpoint,
                detail: "checkpoint backend disappeared during cleanup".to_owned(),
            });
            cleanup.retained.push(CompositePrepareRecoveryEntry {
                resource_id: resource,
                adapter_name: checkpoint.adapter_name().to_owned(),
                backend_instance_id: checkpoint.backend_instance_id().to_owned(),
                checkpoint: checkpoint.clone(),
                actuation: None,
            });
            continue;
        };
        if backends[index].name() != checkpoint.adapter_name()
            || backends[index].backend_instance_id() != checkpoint.backend_instance_id()
        {
            cleanup.failures.push(CompositePrepareCleanupFailure {
                resource_id: resource.clone(),
                operation: CompositePrepareCleanupOperation::ReleaseCheckpoint,
                detail: "checkpoint belongs to a different adapter; release refused".to_owned(),
            });
            cleanup.retained.push(CompositePrepareRecoveryEntry {
                resource_id: resource,
                adapter_name: checkpoint.adapter_name().to_owned(),
                backend_instance_id: checkpoint.backend_instance_id().to_owned(),
                checkpoint: checkpoint.clone(),
                actuation: None,
            });
            continue;
        }
        if let Err(error) = backends[index].release_pre_act_state(checkpoint) {
            cleanup.failures.push(CompositePrepareCleanupFailure {
                resource_id: resource.clone(),
                operation: CompositePrepareCleanupOperation::ReleaseCheckpoint,
                detail: error.to_string(),
            });
            cleanup.retained.push(CompositePrepareRecoveryEntry {
                resource_id: resource,
                adapter_name: checkpoint.adapter_name().to_owned(),
                backend_instance_id: checkpoint.backend_instance_id().to_owned(),
                checkpoint: checkpoint.clone(),
                actuation: None,
            });
        }
    }
    cleanup
}

fn recovery_from_prepared(entry: &CompositePreparedSubplan) -> CompositePrepareRecoveryEntry {
    CompositePrepareRecoveryEntry {
        resource_id: entry.resource_id.clone(),
        adapter_name: entry.adapter_name.clone(),
        backend_instance_id: entry.backend_instance_id.clone(),
        checkpoint: entry.checkpoint.clone(),
        actuation: Some(entry.actuation.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CommitRecord, InvariantCheck, RollbackRecord, VerificationResult};
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

    struct TestBackend {
        resource: String,
        name: String,
        backend_instance_id: String,
        events: Rc<RefCell<Vec<String>>>,
        generation: u64,
        fail_validate: bool,
        fail_capture: bool,
        fail_prepare: bool,
        fail_abort: bool,
        fail_release: bool,
        mismatch_checkpoint: bool,
        mismatch_actuation_adapter: bool,
    }

    impl TestBackend {
        fn new(resource: &str, events: Rc<RefCell<Vec<String>>>) -> Self {
            Self {
                resource: resource.to_owned(),
                name: format!("test-{resource}"),
                backend_instance_id: format!("test-instance-{resource}"),
                events,
                generation: 7,
                fail_validate: false,
                fail_capture: false,
                fail_prepare: false,
                fail_abort: false,
                fail_release: false,
                mismatch_checkpoint: false,
                mismatch_actuation_adapter: false,
            }
        }

        fn record(&self, stage: &str) {
            self.events
                .borrow_mut()
                .push(format!("{stage}:{}", self.resource));
        }
    }

    impl TransactionalActuator for TestBackend {
        fn name(&self) -> &str {
            &self.name
        }

        fn validate(&self, _plan: &crate::Plan) -> Result<Vec<InvariantCheck>, RuntimeError> {
            self.record("validate");
            if self.fail_validate {
                Err(RuntimeError::validation("forced validation failure"))
            } else {
                Ok(Vec::new())
            }
        }

        fn prepare(&mut self, plan: &ValidatedPlan) -> Result<Actuation, RuntimeError> {
            self.record("prepare");
            if self.fail_prepare {
                Err(RuntimeError::actuation("forced prepare failure"))
            } else {
                Ok(Actuation::new(
                    plan.clone(),
                    Some(self.generation + 1),
                    if self.mismatch_actuation_adapter {
                        "wrong-adapter".to_owned()
                    } else {
                        self.name.clone()
                    },
                ))
            }
        }

        fn actuate(&mut self, _actuation: &Actuation) -> Result<(), RuntimeError> {
            panic!("ELANG4b must never actuate")
        }

        fn verify(&self, _actuation: &Actuation) -> Result<VerificationResult, RuntimeError> {
            panic!("ELANG4b must never verify post-actuation state")
        }

        fn commit(&mut self, _actuation: &Actuation) -> Result<CommitRecord, RuntimeError> {
            panic!("ELANG4b must never commit")
        }

        fn rollback(
            &mut self,
            _actuation: &Actuation,
            _verification: &VerificationResult,
        ) -> Result<RollbackRecord, RuntimeError> {
            panic!("ELANG4b must never execute physical rollback")
        }
    }

    impl CompositePrepareBackend for TestBackend {
        fn resource_id(&self) -> &str {
            &self.resource
        }

        fn backend_instance_id(&self) -> &str {
            &self.backend_instance_id
        }

        fn capture_pre_act_state(
            &mut self,
            _plan: &ValidatedPlan,
        ) -> Result<CompositePreActState, RuntimeError> {
            self.record("capture");
            if self.fail_capture {
                return Err(RuntimeError::validation("forced capture failure"));
            }
            Ok(CompositePreActState::new(
                if self.mismatch_checkpoint {
                    "wrong-resource"
                } else {
                    &self.resource
                },
                self.name.clone(),
                self.backend_instance_id.clone(),
                self.generation,
                self.generation ^ 0xa5a5,
            ))
        }

        fn abort_prepare(
            &mut self,
            _actuation: &Actuation,
            _checkpoint: &CompositePreActState,
            _reason: &str,
        ) -> Result<(), RuntimeError> {
            self.record("abort");
            if self.fail_abort {
                Err(RuntimeError::rollback("forced prepare-abort failure"))
            } else {
                Ok(())
            }
        }

        fn release_pre_act_state(
            &mut self,
            _checkpoint: &CompositePreActState,
        ) -> Result<(), RuntimeError> {
            self.record("release");
            if self.fail_release {
                Err(RuntimeError::rollback("forced checkpoint-release failure"))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn all_validation_and_capture_finish_before_any_prepare() {
        let envelope = envelope();
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut base = TestBackend::new("base", events.clone());
        let mut worker = TestBackend::new("worker", events.clone());
        let mut backends: [&mut dyn CompositePrepareBackend; 2] = [&mut worker, &mut base];

        let prepared = prepare_composite_plan(&envelope, &mut backends).unwrap();
        assert_eq!(
            prepared
                .subplans()
                .iter()
                .map(CompositePreparedSubplan::resource_id)
                .collect::<Vec<_>>(),
            vec!["base", "worker"]
        );
        assert_eq!(
            events.borrow().as_slice(),
            [
                "validate:base",
                "validate:worker",
                "capture:base",
                "capture:worker",
                "prepare:base",
                "prepare:worker",
            ]
        );
        assert_eq!(prepared.source_plan_fingerprint(), envelope.fingerprint());
    }

    #[test]
    fn validation_failure_happens_before_any_checkpoint_or_prepare() {
        let envelope = envelope();
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut base = TestBackend::new("base", events.clone());
        let mut worker = TestBackend::new("worker", events.clone());
        worker.fail_validate = true;
        let mut backends: [&mut dyn CompositePrepareBackend; 2] = [&mut base, &mut worker];

        let failure = prepare_composite_plan(&envelope, &mut backends).unwrap_err();
        assert_eq!(failure.stage(), CompositePrepareStage::Validate);
        assert_eq!(failure.resource_id(), Some("worker"));
        assert_eq!(
            events.borrow().as_slice(),
            ["validate:base", "validate:worker"]
        );
    }

    #[test]
    fn prepare_failure_aborts_prior_prepares_and_releases_all_checkpoints_in_reverse() {
        let envelope = envelope();
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut base = TestBackend::new("base", events.clone());
        let mut worker = TestBackend::new("worker", events.clone());
        worker.fail_prepare = true;
        let mut backends: [&mut dyn CompositePrepareBackend; 2] = [&mut base, &mut worker];

        let failure = prepare_composite_plan(&envelope, &mut backends).unwrap_err();
        assert_eq!(failure.stage(), CompositePrepareStage::Prepare);
        assert!(failure.cleanup_failures().is_empty());
        assert_eq!(
            events.borrow().as_slice(),
            [
                "validate:base",
                "validate:worker",
                "capture:base",
                "capture:worker",
                "prepare:base",
                "prepare:worker",
                "abort:base",
                "release:worker",
                "release:base",
            ]
        );
    }

    #[test]
    fn checkpoint_binding_mismatch_fails_closed_and_releases_every_capture() {
        let envelope = envelope();
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut base = TestBackend::new("base", events.clone());
        let mut worker = TestBackend::new("worker", events.clone());
        worker.mismatch_checkpoint = true;
        let mut backends: [&mut dyn CompositePrepareBackend; 2] = [&mut base, &mut worker];

        let failure = prepare_composite_plan(&envelope, &mut backends).unwrap_err();
        assert_eq!(failure.stage(), CompositePrepareStage::Capture);
        assert_eq!(
            events.borrow().as_slice(),
            [
                "validate:base",
                "validate:worker",
                "capture:base",
                "capture:worker",
                "release:worker",
                "release:base",
            ]
        );
        assert!(failure.cleanup_failures().is_empty());
    }

    #[test]
    fn explicit_abort_unwinds_preparation_and_checkpoints_in_reverse() {
        let envelope = envelope();
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut base = TestBackend::new("base", events.clone());
        let mut worker = TestBackend::new("worker", events.clone());
        let mut backends: [&mut dyn CompositePrepareBackend; 2] = [&mut base, &mut worker];
        let prepared = prepare_composite_plan(&envelope, &mut backends).unwrap();
        events.borrow_mut().clear();

        abort_composite_prepare(prepared, &mut backends, "cancel before actuation").unwrap();
        assert_eq!(
            events.borrow().as_slice(),
            [
                "abort:worker",
                "abort:base",
                "release:worker",
                "release:base",
            ]
        );
    }

    #[test]
    fn failed_prepare_abort_retains_checkpoint_until_retry_succeeds() {
        let envelope = envelope();
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut base = TestBackend::new("base", events.clone());
        let mut worker = TestBackend::new("worker", events.clone());
        base.fail_abort = true;
        worker.fail_prepare = true;
        let mut backends: [&mut dyn CompositePrepareBackend; 2] = [&mut base, &mut worker];

        let failure = prepare_composite_plan(&envelope, &mut backends).unwrap_err();
        assert_eq!(failure.stage(), CompositePrepareStage::Prepare);
        assert_eq!(failure.cleanup_failures().len(), 1);
        assert_eq!(
            failure.cleanup_failures()[0].operation(),
            CompositePrepareCleanupOperation::AbortPrepare
        );
        assert_eq!(
            events.borrow().as_slice(),
            [
                "validate:base",
                "validate:worker",
                "capture:base",
                "capture:worker",
                "prepare:base",
                "prepare:worker",
                "abort:base",
                "release:worker",
            ]
        );
        let recovery = failure
            .into_recovery()
            .expect("failed abort retains recovery state");
        assert_eq!(recovery.entries().len(), 1);
        assert_eq!(recovery.entries()[0].resource_id(), "base");
        assert!(recovery.entries()[0].actuation().is_some());

        base.fail_abort = false;
        events.borrow_mut().clear();
        let mut retry_backends: [&mut dyn CompositePrepareBackend; 1] = [&mut base];
        retry_composite_prepare_cleanup(recovery, &mut retry_backends, "retry abort").unwrap();
        assert_eq!(events.borrow().as_slice(), ["abort:base", "release:base"]);
    }

    #[test]
    fn release_failure_retries_release_without_repeating_successful_abort() {
        let envelope = envelope();
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut base = TestBackend::new("base", events.clone());
        let mut worker = TestBackend::new("worker", events.clone());
        base.fail_release = true;
        let prepared = {
            let mut backends: [&mut dyn CompositePrepareBackend; 2] = [&mut base, &mut worker];
            prepare_composite_plan(&envelope, &mut backends).unwrap()
        };
        events.borrow_mut().clear();
        let failure = {
            let mut backends: [&mut dyn CompositePrepareBackend; 2] = [&mut base, &mut worker];
            abort_composite_prepare(prepared, &mut backends, "cancel").unwrap_err()
        };
        let recovery = failure
            .into_recovery()
            .expect("failed release retains checkpoint");
        assert_eq!(recovery.entries().len(), 1);
        assert_eq!(recovery.entries()[0].resource_id(), "base");
        assert!(recovery.entries()[0].actuation().is_none());
        assert_eq!(
            events.borrow().as_slice(),
            [
                "abort:worker",
                "abort:base",
                "release:worker",
                "release:base"
            ]
        );

        base.fail_release = false;
        events.borrow_mut().clear();
        let mut retry_backends: [&mut dyn CompositePrepareBackend; 1] = [&mut base];
        retry_composite_prepare_cleanup(recovery, &mut retry_backends, "retry release").unwrap();
        assert_eq!(events.borrow().as_slice(), ["release:base"]);
    }

    #[test]
    fn abort_refuses_replacement_with_same_resource_and_adapter_name() {
        let envelope = envelope();
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut base = TestBackend::new("base", events.clone());
        let mut worker = TestBackend::new("worker", events.clone());
        let prepared = {
            let mut backends: [&mut dyn CompositePrepareBackend; 2] = [&mut base, &mut worker];
            prepare_composite_plan(&envelope, &mut backends).unwrap()
        };
        events.borrow_mut().clear();

        let mut replacement_base = TestBackend::new("base", events.clone());
        replacement_base.backend_instance_id = "replacement-instance-base".to_owned();
        let failure = {
            let mut wrong_backends: [&mut dyn CompositePrepareBackend; 2] =
                [&mut replacement_base, &mut worker];
            abort_composite_prepare(prepared, &mut wrong_backends, "replacement backend")
                .unwrap_err()
        };
        assert_eq!(failure.stage(), CompositePrepareStage::Bind);
        assert!(events.borrow().is_empty());
        let recovery = failure
            .into_recovery()
            .expect("backend-instance mismatch retains all prepared state");
        assert_eq!(recovery.entries().len(), 2);

        let mut correct_backends: [&mut dyn CompositePrepareBackend; 2] = [&mut base, &mut worker];
        retry_composite_prepare_cleanup(recovery, &mut correct_backends, "correct adapter retry")
            .unwrap();
        assert_eq!(
            events.borrow().as_slice(),
            [
                "abort:worker",
                "release:worker",
                "abort:base",
                "release:base"
            ]
        );
    }

    #[test]
    fn malformed_prepared_actuation_is_cleaned_by_known_producer_backend() {
        let envelope = envelope();
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut base = TestBackend::new("base", events.clone());
        let mut worker = TestBackend::new("worker", events.clone());
        worker.mismatch_actuation_adapter = true;
        let mut backends: [&mut dyn CompositePrepareBackend; 2] = [&mut base, &mut worker];

        let failure = prepare_composite_plan(&envelope, &mut backends).unwrap_err();
        assert_eq!(failure.stage(), CompositePrepareStage::Prepare);
        assert!(failure.cleanup_failures().is_empty());
        assert!(failure.recovery().is_none());
        assert_eq!(
            events.borrow().as_slice(),
            [
                "validate:base",
                "validate:worker",
                "capture:base",
                "capture:worker",
                "prepare:base",
                "prepare:worker",
                "abort:worker",
                "abort:base",
                "release:worker",
                "release:base",
            ]
        );
    }
}
