//! Backend-neutral transition authorization for physical model-execution profiles.
//!
//! Specialized runtimes may publish multiple qualified execution plans without
//! supporting a live in-place transition between already-materialized models.
//! This module lets those backends expose that fact to ElasticXxx without moving
//! specialized model/runtime semantics into the generic transaction state machine.

use std::error::Error;
use std::fmt;

use elastic_adapters::ModelExecutionProfileV1;
use elastic_eir::Fingerprint;
use serde::{Deserialize, Serialize};

use crate::{ModelExecutionProfileBackendV1, VerificationResult};

/// Physical transition class exposed by a specialized model backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelExecutionTransitionModeV1 {
    /// The backend authorizes an in-place transition that can participate in
    /// ElasticXxx's validate/apply/verify/rollback transaction lifecycle.
    LiveTransactional,
    /// Reaching the target profile requires constructing another model/runtime
    /// instance rather than mutating the currently controlled instance in place.
    ModelRebuildRequired,
}

impl ModelExecutionTransitionModeV1 {
    #[must_use]
    pub const fn is_live_transactional(self) -> bool {
        matches!(self, Self::LiveTransactional)
    }
}

impl fmt::Display for ModelExecutionTransitionModeV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LiveTransactional => f.write_str("live_transactional"),
            Self::ModelRebuildRequired => f.write_str("model_rebuild_required"),
        }
    }
}

/// Backend-neutral policy queried before a profile transition may be applied.
///
/// The policy is intentionally evaluated against the backend's freshly reported
/// current rank and the complete correlated target profile. It should contain
/// only transition authorization owned by the specialized backend or adapter;
/// profile feasibility remains the responsibility of
/// [`ModelExecutionProfileBackendV1::validate_profile`].
pub trait ModelExecutionTransitionPolicyV1: Send {
    fn transition_mode(
        &self,
        current_profile_rank: u32,
        target: &ModelExecutionProfileV1,
    ) -> ModelExecutionTransitionModeV1;
}

/// Fixed policy useful when a specialized backend publishes one transition mode
/// for every currently qualified profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixedModelExecutionTransitionPolicyV1 {
    mode: ModelExecutionTransitionModeV1,
}

impl FixedModelExecutionTransitionPolicyV1 {
    #[must_use]
    pub const fn new(mode: ModelExecutionTransitionModeV1) -> Self {
        Self { mode }
    }

    #[must_use]
    pub const fn live_transactional() -> Self {
        Self::new(ModelExecutionTransitionModeV1::LiveTransactional)
    }

    #[must_use]
    pub const fn model_rebuild_required() -> Self {
        Self::new(ModelExecutionTransitionModeV1::ModelRebuildRequired)
    }

    #[must_use]
    pub const fn mode(self) -> ModelExecutionTransitionModeV1 {
        self.mode
    }
}

impl ModelExecutionTransitionPolicyV1 for FixedModelExecutionTransitionPolicyV1 {
    fn transition_mode(
        &self,
        _current_profile_rank: u32,
        _target: &ModelExecutionProfileV1,
    ) -> ModelExecutionTransitionModeV1 {
        self.mode
    }
}

/// Failure surfaced by a transition-guarded backend.
#[derive(Debug)]
pub enum TransitionGuardedModelExecutionBackendError<E> {
    Backend(E),
    TransitionRejected {
        current_profile_rank: u32,
        target_profile_rank: u32,
        mode: ModelExecutionTransitionModeV1,
    },
}

impl<E> fmt::Display for TransitionGuardedModelExecutionBackendError<E>
where
    E: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backend(error) => write!(f, "backend error: {error}"),
            Self::TransitionRejected {
                current_profile_rank,
                target_profile_rank,
                mode,
            } => write!(
                f,
                "model transition from profile rank {current_profile_rank} to {target_profile_rank} is not live-transactional: backend requires {mode}"
            ),
        }
    }
}

impl<E> Error for TransitionGuardedModelExecutionBackendError<E>
where
    E: Error + Send + Sync + 'static,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Backend(error) => Some(error),
            Self::TransitionRejected { .. } => None,
        }
    }
}

/// Decorates an existing model backend with explicit transition authorization.
///
/// `TransactionalModelExecution` already calls `validate_profile` during trusted
/// validation, preparation, and immediately before `apply_profile`. This wrapper
/// reuses those gates instead of introducing another transaction state machine.
/// It also repeats the transition check inside `apply_profile` so direct use of
/// the wrapper cannot silently bypass the policy.
pub struct TransitionGuardedModelExecutionBackendV1<B, P> {
    backend: B,
    policy: P,
}

impl<B, P> TransitionGuardedModelExecutionBackendV1<B, P> {
    #[must_use]
    pub const fn new(backend: B, policy: P) -> Self {
        Self { backend, policy }
    }

    #[must_use]
    pub const fn backend(&self) -> &B {
        &self.backend
    }

    #[must_use]
    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    #[must_use]
    pub const fn policy(&self) -> &P {
        &self.policy
    }

    #[must_use]
    pub fn into_parts(self) -> (B, P) {
        (self.backend, self.policy)
    }
}

impl<B, P> TransitionGuardedModelExecutionBackendV1<B, P>
where
    B: ModelExecutionProfileBackendV1,
    P: ModelExecutionTransitionPolicyV1,
{
    fn require_live_transition(
        &self,
        current_profile_rank: u32,
        target: &ModelExecutionProfileV1,
    ) -> Result<(), TransitionGuardedModelExecutionBackendError<B::Error>> {
        let mode = self.policy.transition_mode(current_profile_rank, target);
        if mode.is_live_transactional() {
            Ok(())
        } else {
            Err(
                TransitionGuardedModelExecutionBackendError::TransitionRejected {
                    current_profile_rank,
                    target_profile_rank: target.preference_rank(),
                    mode,
                },
            )
        }
    }
}

impl<B, P> ModelExecutionProfileBackendV1 for TransitionGuardedModelExecutionBackendV1<B, P>
where
    B: ModelExecutionProfileBackendV1,
    P: ModelExecutionTransitionPolicyV1,
{
    type Error = TransitionGuardedModelExecutionBackendError<B::Error>;

    fn name(&self) -> &str {
        self.backend.name()
    }

    fn provider_id(&self) -> &str {
        self.backend.provider_id()
    }

    fn model_revision(&self) -> &str {
        self.backend.model_revision()
    }

    fn capability_fingerprint(&self) -> Fingerprint {
        self.backend.capability_fingerprint()
    }

    fn profile_set_fingerprint(&self) -> Fingerprint {
        self.backend.profile_set_fingerprint()
    }

    fn current_profile_rank(&self) -> Result<u32, Self::Error> {
        self.backend
            .current_profile_rank()
            .map_err(TransitionGuardedModelExecutionBackendError::Backend)
    }

    fn validate_profile(&self, target: &ModelExecutionProfileV1) -> Result<(), Self::Error> {
        let current_profile_rank = self
            .backend
            .current_profile_rank()
            .map_err(TransitionGuardedModelExecutionBackendError::Backend)?;
        self.require_live_transition(current_profile_rank, target)?;
        self.backend
            .validate_profile(target)
            .map_err(TransitionGuardedModelExecutionBackendError::Backend)
    }

    fn apply_profile(&mut self, target: &ModelExecutionProfileV1) -> Result<(), Self::Error> {
        let current_profile_rank = self
            .backend
            .current_profile_rank()
            .map_err(TransitionGuardedModelExecutionBackendError::Backend)?;
        self.require_live_transition(current_profile_rank, target)?;
        self.backend
            .apply_profile(target)
            .map_err(TransitionGuardedModelExecutionBackendError::Backend)
    }

    fn verify_profile(
        &self,
        target: &ModelExecutionProfileV1,
    ) -> Result<VerificationResult, Self::Error> {
        self.backend
            .verify_profile(target)
            .map_err(TransitionGuardedModelExecutionBackendError::Backend)
    }

    fn restore_profile(&mut self, previous: &ModelExecutionProfileV1) -> Result<(), Self::Error> {
        // Rollback is deliberately not transition-policy gated. If an actuation
        // has already happened, the trusted runtime must still attempt the
        // backend's existing restore path rather than refuse recovery because a
        // policy changed after application.
        self.backend
            .restore_profile(previous)
            .map_err(TransitionGuardedModelExecutionBackendError::Backend)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_policy_round_trips_strictly() {
        let policy = FixedModelExecutionTransitionPolicyV1::model_rebuild_required();
        let json = serde_json::to_string(&policy).unwrap();
        let decoded: FixedModelExecutionTransitionPolicyV1 = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, policy);
        assert_eq!(
            decoded.mode(),
            ModelExecutionTransitionModeV1::ModelRebuildRequired
        );

        let with_unknown = r#"{"mode":"model_rebuild_required","unknown":true}"#;
        assert!(
            serde_json::from_str::<FixedModelExecutionTransitionPolicyV1>(with_unknown).is_err()
        );
    }
}
