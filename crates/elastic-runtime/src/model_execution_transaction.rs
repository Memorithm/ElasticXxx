//! Transactional runtime bridge for physical model-execution backends.
//!
//! The correlated-profile adapters define what a qualified model profile means
//! and lower it to one atomic runtime transition. This module binds that atomic
//! transition to ElasticXxx's existing [`TransactionalActuator`] lifecycle.
//!
//! Backend implementations supply only resource-specific physical primitives:
//! current profile, action-time validation, apply, verify, and restore. The
//! runtime remains responsible for prepare/actuate/verify/commit/rollback
//! orchestration and never trusts a backend profile outside the exact bound
//! capability/profile-set fingerprints.

use std::error::Error;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use elastic_adapters::{
    model_execution_current_profile_rank_signal, model_execution_profile_dimension,
    ModelExecutionProfileSetV1, ModelExecutionProfileV1,
};
use elastic_eir::{EirResource, Fingerprint, PlanningContext};

use crate::{
    Actuation, CommitRecord, InvariantCheck, Observation, ObservationSource, Observer, Plan,
    RollbackRecord, RuntimeError, TransactionalActuator, ValidatedPlan, VerificationResult,
};

/// Physical backend contract for one exact correlated model-execution profile set.
///
/// Implementations own the semantics of switching their model. They must not
/// report a different provider/model/capability/profile-set identity than the
/// concrete state they actually control.
pub trait ModelExecutionProfileBackendV1: Send {
    /// Backend-specific failure type.
    type Error: Error + Send + Sync + 'static;

    /// Human-readable backend identity used in diagnostics.
    fn name(&self) -> &str;

    /// Provider identity of the controlled model implementation.
    fn provider_id(&self) -> &str;

    /// Exact controlled model revision.
    fn model_revision(&self) -> &str;

    /// Base model-execution capability fingerprint this backend implements.
    fn capability_fingerprint(&self) -> Fingerprint;

    /// Exact correlated profile-set fingerprint this backend implements.
    fn profile_set_fingerprint(&self) -> Fingerprint;

    /// Current physically active profile rank.
    fn current_profile_rank(&self) -> Result<u32, Self::Error>;

    /// Re-check physical/action-time feasibility for `target` without applying it.
    fn validate_profile(&self, target: &ModelExecutionProfileV1) -> Result<(), Self::Error>;

    /// Apply one complete correlated profile as a physical unit.
    fn apply_profile(&mut self, target: &ModelExecutionProfileV1) -> Result<(), Self::Error>;

    /// Verify the post-action model state and backend-specific invariants.
    fn verify_profile(
        &self,
        target: &ModelExecutionProfileV1,
    ) -> Result<VerificationResult, Self::Error>;

    /// Restore one previously active complete profile during rollback.
    fn restore_profile(&mut self, previous: &ModelExecutionProfileV1) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PreparedModelExecution {
    previous_rank: u32,
    target_rank: u32,
}

struct ModelExecutionState<B> {
    backend: B,
    profiles: ModelExecutionProfileSetV1,
    prepared: Option<PreparedModelExecution>,
}

/// Cloneable transactional handle around one physical model-execution backend.
///
/// Clones share one protected backend state. One clone can therefore serve as
/// an [`Observer`] while another is passed mutably to the runtime as the
/// [`TransactionalActuator`], matching the RAM/concurrency reference pattern.
pub struct TransactionalModelExecution<B> {
    state: Arc<Mutex<ModelExecutionState<B>>>,
    source: ObservationSource,
    ir: EirResource,
    adapter_name: String,
}

impl<B> Clone for TransactionalModelExecution<B> {
    fn clone(&self) -> Self {
        Self {
            state: Arc::clone(&self.state),
            source: self.source.clone(),
            ir: self.ir.clone(),
            adapter_name: self.adapter_name.clone(),
        }
    }
}

impl<B> TransactionalModelExecution<B>
where
    B: ModelExecutionProfileBackendV1,
{
    /// Bind a physical backend to one exact correlated profile set.
    ///
    /// Construction validates provider/model/fingerprint identity and requires
    /// the backend's current profile rank to be present in the bound profile set.
    ///
    /// # Errors
    ///
    /// Returns a configuration error on stale identity, unknown current profile,
    /// backend read failure, or invalid atomic resource lowering.
    pub fn new(
        resource_id: &str,
        profiles: ModelExecutionProfileSetV1,
        backend: B,
    ) -> Result<Self, RuntimeError> {
        validate_backend_identity(&backend, &profiles, RuntimeError::configuration)?;
        let current_rank = backend.current_profile_rank().map_err(|error| {
            RuntimeError::configuration(format!(
                "model backend could not report current profile: {error}"
            ))
        })?;
        require_profile(&profiles, current_rank, RuntimeError::configuration)?;

        let spec = profiles
            .atomic_resource_spec(resource_id)
            .map_err(|error| RuntimeError::configuration(error.to_string()))?;
        let document = elastic_eir::lower(&spec)
            .map_err(|error| RuntimeError::configuration(error.to_string()))?;
        let ir = document
            .resource(resource_id)
            .ok_or_else(|| {
                RuntimeError::configuration("atomic model resource missing after lower")
            })?
            .clone();
        let source = ObservationSource::Resource(ir.identity().clone());
        let adapter_name = format!("transactional-model-execution:{}", backend.name());

        Ok(Self {
            state: Arc::new(Mutex::new(ModelExecutionState {
                backend,
                profiles,
                prepared: None,
            })),
            source,
            ir,
            adapter_name,
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, ModelExecutionState<B>>, RuntimeError> {
        self.state
            .lock()
            .map_err(|_| RuntimeError::actuation("model backend shared state lock was poisoned"))
    }

    /// Clone the exact atomic EIR resource controlled by this adapter.
    #[must_use]
    pub fn ir(&self) -> EirResource {
        self.ir.clone()
    }

    /// Read the current physical profile rank through the backend.
    ///
    /// # Errors
    ///
    /// Returns an observation error if the shared state or backend cannot be read,
    /// or if the backend reports a rank outside the exact profile set.
    pub fn current_profile_rank(&self) -> Result<u32, RuntimeError> {
        let state = self
            .lock()
            .map_err(|error| RuntimeError::observation(error.to_string()))?;
        let rank = state.backend.current_profile_rank().map_err(|error| {
            RuntimeError::observation(format!("model backend profile read failed: {error}"))
        })?;
        require_profile(&state.profiles, rank, RuntimeError::observation)?;
        Ok(rank)
    }

    fn ensure_plan_resource(&self, plan: &Plan) -> Result<(), RuntimeError> {
        if plan.resource.fingerprint() != self.ir.fingerprint() {
            return Err(RuntimeError::validation(
                "model actuator received a plan for a different atomic resource",
            ));
        }
        Ok(())
    }

    fn ensure_actuation(&self, actuation: &Actuation) -> Result<u32, RuntimeError> {
        if actuation.adapter_name != self.adapter_name {
            return Err(RuntimeError::actuation(format!(
                "model actuation belongs to adapter {:?}, not {:?}",
                actuation.adapter_name, self.adapter_name
            )));
        }
        if actuation.plan.plan.resource.fingerprint() != self.ir.fingerprint() {
            return Err(RuntimeError::actuation(
                "model actuation resource fingerprint does not match this adapter",
            ));
        }
        let raw = actuation
            .target
            .ok_or_else(|| RuntimeError::actuation("model actuation has no target profile rank"))?;
        u32::try_from(raw)
            .map_err(|_| RuntimeError::actuation("model target profile rank does not fit u32"))
    }
}

impl<B> Observer for TransactionalModelExecution<B>
where
    B: ModelExecutionProfileBackendV1,
{
    fn observe(&self) -> (PlanningContext, Vec<Observation>) {
        let signal = model_execution_current_profile_rank_signal();
        let timestamp = Instant::now();
        match self.state.lock() {
            Ok(state) => match state.backend.current_profile_rank() {
                Ok(rank) if profile_by_rank(&state.profiles, rank).is_some() => {
                    let value = f64::from(rank);
                    (
                        PlanningContext::new().observe(signal.clone(), value),
                        vec![Observation::from_source(
                            self.source.clone(),
                            signal,
                            value,
                            timestamp,
                        )],
                    )
                }
                Ok(rank) => (
                    PlanningContext::new(),
                    vec![Observation::unsupported_from_source(
                        self.source.clone(),
                        signal,
                        timestamp,
                        format!("backend reported unpublished profile rank {rank}"),
                    )],
                ),
                Err(error) => (
                    PlanningContext::new(),
                    vec![Observation::unsupported_from_source(
                        self.source.clone(),
                        signal,
                        timestamp,
                        format!("backend current-profile observation failed: {error}"),
                    )],
                ),
            },
            Err(_) => (
                PlanningContext::new(),
                vec![Observation::unsupported_from_source(
                    self.source.clone(),
                    signal,
                    timestamp,
                    "model backend shared state lock was poisoned",
                )],
            ),
        }
    }
}

impl<B> TransactionalActuator for TransactionalModelExecution<B>
where
    B: ModelExecutionProfileBackendV1,
{
    fn name(&self) -> &str {
        &self.adapter_name
    }

    fn validate(&self, plan: &Plan) -> Result<Vec<InvariantCheck>, RuntimeError> {
        self.ensure_plan_resource(plan)?;
        let target_rank = candidate_profile_rank(plan)?;
        let state = self.lock()?;
        validate_backend_identity(&state.backend, &state.profiles, RuntimeError::validation)?;
        let target = require_profile(&state.profiles, target_rank, RuntimeError::validation)?;
        let current_rank = state.backend.current_profile_rank().map_err(|error| {
            RuntimeError::validation(format!(
                "model backend current-profile read failed: {error}"
            ))
        })?;
        require_profile(&state.profiles, current_rank, RuntimeError::validation)?;
        state.backend.validate_profile(target).map_err(|error| {
            RuntimeError::validation(format!("model backend rejected target profile: {error}"))
        })?;
        Ok(successful_checks(plan))
    }

    fn prepare(&mut self, plan: &ValidatedPlan) -> Result<Actuation, RuntimeError> {
        if !plan.validated {
            return Err(RuntimeError::validation(
                "model execution actuation requires a validated plan",
            ));
        }
        self.ensure_plan_resource(&plan.plan)?;
        let target_rank = candidate_profile_rank(&plan.plan)?;
        let mut state = self.lock()?;
        if state.prepared.is_some() {
            return Err(RuntimeError::actuation(
                "model backend already has a prepared transaction",
            ));
        }
        validate_backend_identity(&state.backend, &state.profiles, RuntimeError::validation)?;
        let target =
            require_profile(&state.profiles, target_rank, RuntimeError::validation)?.clone();
        let previous_rank = state.backend.current_profile_rank().map_err(|error| {
            RuntimeError::validation(format!(
                "model backend current-profile read failed: {error}"
            ))
        })?;
        require_profile(&state.profiles, previous_rank, RuntimeError::validation)?;
        state.backend.validate_profile(&target).map_err(|error| {
            RuntimeError::validation(format!("model backend rejected target profile: {error}"))
        })?;
        state.prepared = Some(PreparedModelExecution {
            previous_rank,
            target_rank,
        });
        Ok(Actuation::new(
            plan.clone(),
            Some(u64::from(target_rank)),
            self.name(),
        ))
    }

    fn actuate(&mut self, actuation: &Actuation) -> Result<(), RuntimeError> {
        let target_rank = self.ensure_actuation(actuation)?;
        let mut state = self.lock()?;
        let prepared = state.prepared.ok_or_else(|| {
            RuntimeError::actuation("model actuation was not prepared transactionally")
        })?;
        if prepared.target_rank != target_rank {
            return Err(RuntimeError::actuation(format!(
                "model target rank {target_rank} does not match prepared target {}",
                prepared.target_rank
            )));
        }
        validate_backend_identity(&state.backend, &state.profiles, RuntimeError::actuation)?;
        let target =
            require_profile(&state.profiles, target_rank, RuntimeError::actuation)?.clone();
        state.backend.apply_profile(&target).map_err(|error| {
            RuntimeError::actuation(format!("model backend apply failed: {error}"))
        })
    }

    fn verify(&self, actuation: &Actuation) -> Result<VerificationResult, RuntimeError> {
        let target_rank = self.ensure_actuation(actuation)?;
        let state = self.lock()?;
        let prepared = state.prepared.ok_or_else(|| {
            RuntimeError::verification("model backend has no prepared transaction to verify")
        })?;
        if prepared.target_rank != target_rank {
            return Err(RuntimeError::verification(format!(
                "model verification rank {target_rank} does not match prepared target {}",
                prepared.target_rank
            )));
        }
        validate_backend_identity(&state.backend, &state.profiles, RuntimeError::verification)?;
        let target = require_profile(&state.profiles, target_rank, RuntimeError::verification)?;
        let backend_result = state.backend.verify_profile(target).map_err(|error| {
            RuntimeError::verification(format!("model backend verification failed: {error}"))
        })?;
        if !backend_result.is_pass() {
            return Ok(backend_result);
        }
        let current_rank = state.backend.current_profile_rank().map_err(|error| {
            RuntimeError::verification(format!(
                "model backend current-profile read failed: {error}"
            ))
        })?;
        if current_rank == target_rank {
            Ok(VerificationResult::Pass)
        } else {
            Ok(VerificationResult::Fail {
                detail: format!(
                    "backend verification passed but current profile rank is {current_rank}, expected {target_rank}"
                ),
            })
        }
    }

    fn commit(&mut self, actuation: &Actuation) -> Result<CommitRecord, RuntimeError> {
        let target_rank = self.ensure_actuation(actuation)?;
        let mut state = self.lock()?;
        let prepared = state.prepared.ok_or_else(|| {
            RuntimeError::commit("model backend has no prepared transaction to commit")
        })?;
        if prepared.target_rank != target_rank {
            return Err(RuntimeError::commit(format!(
                "model commit rank {target_rank} does not match prepared target {}",
                prepared.target_rank
            )));
        }
        validate_backend_identity(&state.backend, &state.profiles, RuntimeError::commit)?;
        let target = require_profile(&state.profiles, target_rank, RuntimeError::commit)?;
        let verification = state.backend.verify_profile(target).map_err(|error| {
            RuntimeError::commit(format!("model backend commit verification failed: {error}"))
        })?;
        if !verification.is_pass() {
            return Err(RuntimeError::commit(format!(
                "model backend target is not verifiably committable: {verification:?}"
            )));
        }
        let current_rank = state.backend.current_profile_rank().map_err(|error| {
            RuntimeError::commit(format!(
                "model backend current-profile read failed: {error}"
            ))
        })?;
        if current_rank != target_rank {
            return Err(RuntimeError::commit(format!(
                "model backend current rank {current_rank} does not match commit target {target_rank}"
            )));
        }
        state.prepared = None;
        Ok(CommitRecord::new(
            self.name(),
            format!("verified model execution profile rank {target_rank} committed"),
        ))
    }

    fn rollback(
        &mut self,
        actuation: &Actuation,
        _verification: &VerificationResult,
    ) -> Result<RollbackRecord, RuntimeError> {
        let target_rank = self
            .ensure_actuation(actuation)
            .map_err(|error| RuntimeError::rollback(error.to_string()))?;
        let mut state = self
            .lock()
            .map_err(|error| RuntimeError::rollback(error.to_string()))?;
        let prepared = state.prepared.ok_or_else(|| {
            RuntimeError::rollback("model backend has no prepared transaction to roll back")
        })?;
        if prepared.target_rank != target_rank {
            return Err(RuntimeError::rollback(format!(
                "model rollback rank {target_rank} does not match prepared target {}",
                prepared.target_rank
            )));
        }
        validate_backend_identity(&state.backend, &state.profiles, RuntimeError::rollback)?;
        let previous = require_profile(
            &state.profiles,
            prepared.previous_rank,
            RuntimeError::rollback,
        )?
        .clone();
        state.backend.restore_profile(&previous).map_err(|error| {
            RuntimeError::rollback(format!("model backend restore failed: {error}"))
        })?;
        let backend_verification = state.backend.verify_profile(&previous).map_err(|error| {
            RuntimeError::rollback(format!(
                "model backend rollback verification failed: {error}"
            ))
        })?;
        let current_rank = state.backend.current_profile_rank().map_err(|error| {
            RuntimeError::rollback(format!(
                "model backend rollback profile read failed: {error}"
            ))
        })?;
        let restored = backend_verification.is_pass() && current_rank == prepared.previous_rank;
        if restored {
            state.prepared = None;
        }
        Ok(RollbackRecord::new(
            self.name(),
            format!(
                "restored model execution profile rank {}",
                prepared.previous_rank
            ),
            restored,
        ))
    }
}

fn candidate_profile_rank(plan: &Plan) -> Result<u32, RuntimeError> {
    let candidate = plan
        .candidate()
        .ok_or_else(|| RuntimeError::validation("model plan has no candidate"))?;
    if candidate.dimension() != &model_execution_profile_dimension() {
        return Err(RuntimeError::validation(format!(
            "model actuator requires dimension {}, got {}",
            model_execution_profile_dimension(),
            candidate.dimension()
        )));
    }
    let raw = candidate
        .magnitude()
        .ok_or_else(|| RuntimeError::validation("model candidate has no target profile rank"))?;
    u32::try_from(raw)
        .map_err(|_| RuntimeError::validation("model target profile rank does not fit u32"))
}

fn profile_by_rank(
    profiles: &ModelExecutionProfileSetV1,
    rank: u32,
) -> Option<&ModelExecutionProfileV1> {
    profiles
        .profiles()
        .iter()
        .find(|profile| profile.preference_rank() == rank)
}

fn require_profile<F>(
    profiles: &ModelExecutionProfileSetV1,
    rank: u32,
    error: F,
) -> Result<&ModelExecutionProfileV1, RuntimeError>
where
    F: Fn(String) -> RuntimeError,
{
    profile_by_rank(profiles, rank).ok_or_else(|| {
        error(format!(
            "profile rank {rank} is not published by the bound profile set"
        ))
    })
}

fn validate_backend_identity<B, F>(
    backend: &B,
    profiles: &ModelExecutionProfileSetV1,
    error: F,
) -> Result<(), RuntimeError>
where
    B: ModelExecutionProfileBackendV1,
    F: Fn(String) -> RuntimeError,
{
    if backend.provider_id() != profiles.provider_id() {
        return Err(error(format!(
            "model backend provider mismatch: expected {:?}, got {:?}",
            profiles.provider_id(),
            backend.provider_id()
        )));
    }
    if backend.model_revision() != profiles.model_revision() {
        return Err(error(format!(
            "model backend revision mismatch: expected {:?}, got {:?}",
            profiles.model_revision(),
            backend.model_revision()
        )));
    }
    if backend.capability_fingerprint() != profiles.capability_fingerprint() {
        return Err(error(format!(
            "model backend capability fingerprint mismatch: expected {}, got {}",
            profiles.capability_fingerprint(),
            backend.capability_fingerprint()
        )));
    }
    if backend.profile_set_fingerprint() != profiles.fingerprint() {
        return Err(error(format!(
            "model backend profile-set fingerprint mismatch: expected {}, got {}",
            profiles.fingerprint(),
            backend.profile_set_fingerprint()
        )));
    }
    Ok(())
}

fn successful_checks(plan: &Plan) -> Vec<InvariantCheck> {
    plan.resource
        .invariants()
        .iter()
        .cloned()
        .map(|invariant| {
            InvariantCheck::new(
                invariant,
                true,
                Some("model backend action-time profile validation passed".to_owned()),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt;

    use elastic_adapters::{
        ModelExecutionAtomicProfilePlannerV1, ModelExecutionCapabilitiesV1,
        ModelExecutionProfileEnvelopeV1, ModelExecutionProfileSelectionV1,
        ModelExecutionProfileSelectorV1,
    };

    use crate::{Cadence, PlannerConfig, Runtime, RuntimeConfig, RuntimeMode};

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct FakeBackendError(String);

    impl fmt::Display for FakeBackendError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(&self.0)
        }
    }

    impl Error for FakeBackendError {}

    struct FakeBackend {
        provider_id: String,
        model_revision: String,
        capability_fingerprint: Fingerprint,
        profile_set_fingerprint: Fingerprint,
        current_rank: u32,
        fail_verification_rank: Option<u32>,
    }

    impl FakeBackend {
        fn new(profiles: &ModelExecutionProfileSetV1, current_rank: u32) -> Self {
            Self {
                provider_id: profiles.provider_id().to_owned(),
                model_revision: profiles.model_revision().to_owned(),
                capability_fingerprint: profiles.capability_fingerprint(),
                profile_set_fingerprint: profiles.fingerprint(),
                current_rank,
                fail_verification_rank: None,
            }
        }
    }

    impl ModelExecutionProfileBackendV1 for FakeBackend {
        type Error = FakeBackendError;

        fn name(&self) -> &str {
            "fake-model"
        }

        fn provider_id(&self) -> &str {
            &self.provider_id
        }

        fn model_revision(&self) -> &str {
            &self.model_revision
        }

        fn capability_fingerprint(&self) -> Fingerprint {
            self.capability_fingerprint
        }

        fn profile_set_fingerprint(&self) -> Fingerprint {
            self.profile_set_fingerprint
        }

        fn current_profile_rank(&self) -> Result<u32, Self::Error> {
            Ok(self.current_rank)
        }

        fn validate_profile(&self, _target: &ModelExecutionProfileV1) -> Result<(), Self::Error> {
            Ok(())
        }

        fn apply_profile(&mut self, target: &ModelExecutionProfileV1) -> Result<(), Self::Error> {
            self.current_rank = target.preference_rank();
            Ok(())
        }

        fn verify_profile(
            &self,
            target: &ModelExecutionProfileV1,
        ) -> Result<VerificationResult, Self::Error> {
            if self.fail_verification_rank == Some(target.preference_rank()) {
                return Ok(VerificationResult::Fail {
                    detail: "injected backend verification failure".to_owned(),
                });
            }
            if self.current_rank == target.preference_rank() {
                Ok(VerificationResult::Pass)
            } else {
                Ok(VerificationResult::Fail {
                    detail: format!(
                        "current rank {} != target {}",
                        self.current_rank,
                        target.preference_rank()
                    ),
                })
            }
        }

        fn restore_profile(
            &mut self,
            previous: &ModelExecutionProfileV1,
        ) -> Result<(), Self::Error> {
            self.current_rank = previous.preference_rank();
            Ok(())
        }
    }

    fn fixture() -> (ModelExecutionProfileSetV1, ModelExecutionProfileV1) {
        let capabilities = ModelExecutionCapabilitiesV1::new(
            "reference-backend",
            "model-rev-a",
            64,
            vec![1, 2, 4],
            vec![2_500, 5_000, 10_000],
            vec![2_500, 5_000, 10_000],
        )
        .unwrap();
        let profiles = ModelExecutionProfileSetV1::new(
            &capabilities,
            vec![
                ModelExecutionProfileV1::new("full", 0, 4, 10_000, 10_000).unwrap(),
                ModelExecutionProfileV1::new("balanced", 10, 2, 5_000, 5_000).unwrap(),
                ModelExecutionProfileV1::new("minimal", 20, 1, 2_500, 2_500).unwrap(),
            ],
        )
        .unwrap();
        let target = profiles
            .profiles()
            .iter()
            .find(|profile| profile.profile_id() == "balanced")
            .unwrap()
            .clone();
        (profiles, target)
    }

    fn selected_plan(
        profiles: &ModelExecutionProfileSetV1,
    ) -> elastic_adapters::ModelExecutionProfilePlanV1 {
        let selection = ModelExecutionProfileSelectorV1
            .select(
                profiles,
                ModelExecutionProfileEnvelopeV1::new(2, 5_000, 5_000).unwrap(),
            )
            .unwrap();
        let ModelExecutionProfileSelectionV1::Selected(plan) = selection else {
            panic!("expected selected profile")
        };
        plan
    }

    fn runtime_config(profiles: &ModelExecutionProfileSetV1, ir: EirResource) -> RuntimeConfig {
        RuntimeConfig {
            resource_spec: profiles.atomic_resource_spec("model-runtime").unwrap(),
            ir_resource: ir,
            planner_config: PlannerConfig::None,
            cadence: Cadence::OneShot,
            mode: RuntimeMode::Apply,
            max_cycles: 0,
            interval_ms: 1_000,
            emit_events: true,
            dry_run: false,
        }
    }

    #[test]
    fn runtime_commits_verified_profile_switch() {
        let (profiles, _) = fixture();
        let target = selected_plan(&profiles);
        let backend = FakeBackend::new(&profiles, 0);
        let mut actuator =
            TransactionalModelExecution::new("model-runtime", profiles.clone(), backend).unwrap();
        let observer = actuator.clone();
        let ir = actuator.ir();
        let planner = ModelExecutionAtomicProfilePlannerV1::new(&target);
        let runtime = Runtime::new(runtime_config(&profiles, ir.clone()));

        let result = runtime
            .cycle(&ir, &planner, &observer, &mut actuator)
            .unwrap();

        assert!(result.commit.is_some());
        assert!(result.rollback.is_none());
        assert_eq!(actuator.current_profile_rank().unwrap(), 10);
    }

    #[test]
    fn runtime_rolls_back_failed_profile_verification() {
        let (profiles, _) = fixture();
        let target = selected_plan(&profiles);
        let mut backend = FakeBackend::new(&profiles, 0);
        backend.fail_verification_rank = Some(10);
        let mut actuator =
            TransactionalModelExecution::new("model-runtime", profiles.clone(), backend).unwrap();
        let observer = actuator.clone();
        let ir = actuator.ir();
        let planner = ModelExecutionAtomicProfilePlannerV1::new(&target);
        let runtime = Runtime::new(runtime_config(&profiles, ir.clone()));

        let result = runtime
            .cycle(&ir, &planner, &observer, &mut actuator)
            .unwrap();

        assert!(result.commit.is_none());
        assert!(result.rollback.is_some());
        assert!(result.rollback.unwrap().invariants_restored);
        assert_eq!(actuator.current_profile_rank().unwrap(), 0);
    }

    #[test]
    fn constructor_rejects_stale_backend_profile_set_identity() {
        let (profiles, _) = fixture();
        let mut backend = FakeBackend::new(&profiles, 0);
        backend.profile_set_fingerprint = Fingerprint::EMPTY;
        let error = TransactionalModelExecution::new("model-runtime", profiles, backend)
            .err()
            .expect("stale backend must fail");
        assert!(matches!(error, RuntimeError::Configuration(_)));
    }
}
