//! Fail-closed audit surface for runtime cycle attempts.
//!
//! The trusted [`crate::Runtime::cycle_with_sink`] method remains the single
//! cycle executor. This module wraps that existing state machine so callers can
//! retain the exact resource identity and ordered events already emitted before
//! a catastrophic [`crate::RuntimeError`] escapes.
//!
//! Version 1 deliberately does not reconstruct observations, plans, actuation
//! state, or rollback state that the trusted runtime did not return. Missing
//! structured state remains missing rather than being inferred from diagnostic
//! event text.

use elastic_eir::{EirResource, TransitionPlanner};

use crate::{
    CycleResult, Observer, Runtime, RuntimeError, RuntimeEvent, TransactionalActuator,
};

/// A completed or failed attempt to execute exactly one trusted runtime cycle.
#[derive(Debug)]
pub enum CycleAttempt {
    /// The trusted runtime completed normally and returned its authoritative
    /// structured [`CycleResult`].
    Completed(CycleResult),
    /// The trusted runtime returned an error after possibly emitting partial
    /// audit events.
    Failed(CycleFailure),
}

impl CycleAttempt {
    /// Convert back to the legacy `Result` surface.
    ///
    /// This is useful to prove that the attempt API does not alter success/error
    /// semantics: it only preserves additional failure audit context.
    pub fn into_result(self) -> Result<CycleResult, RuntimeError> {
        match self {
            Self::Completed(result) => Ok(result),
            Self::Failed(failure) => Err(failure.error),
        }
    }

    /// Whether the trusted cycle completed without escaping a runtime error.
    #[must_use]
    pub const fn is_completed(&self) -> bool {
        matches!(self, Self::Completed(_))
    }
}

/// Auditable state retained when a trusted cycle returns `RuntimeError`.
#[derive(Debug)]
pub struct CycleFailure {
    /// Exact EIR resource against which the failed cycle was attempted.
    pub resource: EirResource,
    /// Authoritative runtime error returned by the existing executor.
    pub error: RuntimeError,
    /// Ordered runtime events emitted before the error escaped.
    ///
    /// Absence of a later-stage event is meaningful only as absence; callers
    /// must not infer that an unrecorded operation definitely did or did not
    /// happen after a backend reported a possibly-partial physical failure.
    pub events: Vec<RuntimeEvent>,
}

impl Runtime {
    /// Execute one trusted cycle while preserving audit context on failure.
    ///
    /// This method delegates execution to [`Runtime::cycle_with_sink`]. It does
    /// not duplicate planning, validation, actuation, verification, commit, or
    /// rollback logic.
    pub fn cycle_attempt<P, O, A>(
        &self,
        resource: &EirResource,
        planner: &P,
        observer: &O,
        actuator: &mut A,
    ) -> CycleAttempt
    where
        P: TransitionPlanner,
        O: Observer,
        A: TransactionalActuator,
    {
        let mut events = Vec::new();
        match self.cycle_with_sink(resource, planner, observer, actuator, &mut events) {
            Ok(result) => CycleAttempt::Completed(result),
            Err(error) => CycleAttempt::Failed(CycleFailure {
                resource: resource.clone(),
                error,
                events,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Actuation, CommitRecord, InvariantCheck, RollbackRecord, RuntimeConfig, RuntimeMode,
        ValidatedPlan, VerificationResult,
    };
    use elastic_eir::FirstGroundedPlanner;

    struct FailingRollbackActuator;

    impl TransactionalActuator for FailingRollbackActuator {
        fn name(&self) -> &str {
            "failing-rollback"
        }

        fn validate(&self, plan: &crate::Plan) -> Result<Vec<InvariantCheck>, RuntimeError> {
            Ok(plan
                .resource
                .invariants()
                .iter()
                .cloned()
                .map(|invariant| InvariantCheck::new(invariant, true, None))
                .collect())
        }

        fn prepare(&mut self, plan: &ValidatedPlan) -> Result<Actuation, RuntimeError> {
            let target = plan
                .plan
                .candidate()
                .and_then(|candidate| candidate.magnitude());
            Ok(Actuation::new(plan.clone(), target, self.name()))
        }

        fn actuate(&mut self, _actuation: &Actuation) -> Result<(), RuntimeError> {
            Ok(())
        }

        fn verify(&self, _actuation: &Actuation) -> Result<VerificationResult, RuntimeError> {
            Ok(VerificationResult::Fail {
                detail: "injected verification failure".to_owned(),
            })
        }

        fn commit(&mut self, _actuation: &Actuation) -> Result<CommitRecord, RuntimeError> {
            Err(RuntimeError::commit("commit must not be reached"))
        }

        fn rollback(
            &mut self,
            _actuation: &Actuation,
            _verification: &VerificationResult,
        ) -> Result<RollbackRecord, RuntimeError> {
            Err(RuntimeError::rollback("injected rollback failure"))
        }
    }

    fn applying_runtime() -> Runtime {
        Runtime::new(RuntimeConfig {
            dry_run: false,
            mode: RuntimeMode::Apply,
            ..RuntimeConfig::default()
        })
    }

    #[test]
    fn failed_attempt_preserves_events_before_unrecoverable_rollback_error() {
        let runtime = applying_runtime();
        let resource = runtime.config().ir_resource.clone();
        let mut actuator = FailingRollbackActuator;

        let attempt = runtime.cycle_attempt(
            &resource,
            &FirstGroundedPlanner,
            &(),
            &mut actuator,
        );

        let CycleAttempt::Failed(failure) = attempt else {
            panic!("rollback failure must remain a failed attempt")
        };
        assert_eq!(failure.resource.fingerprint(), resource.fingerprint());
        assert!(matches!(failure.error, RuntimeError::Rollback(_)));
        assert!(failure
            .events
            .iter()
            .any(|event| event.kind == crate::RuntimeEventKind::ActuationApplied));
        assert!(failure
            .events
            .iter()
            .any(|event| event.kind == crate::RuntimeEventKind::VerificationPerformed));
        assert!(!failure
            .events
            .iter()
            .any(|event| event.kind == crate::RuntimeEventKind::RollbackExecuted));
        assert!(!failure
            .events
            .iter()
            .any(|event| event.kind == crate::RuntimeEventKind::CycleCompleted));
    }

    #[test]
    fn completed_attempt_retains_legacy_cycle_result() {
        struct PassingActuator;

        impl TransactionalActuator for PassingActuator {
            fn name(&self) -> &str {
                "passing"
            }

            fn validate(&self, plan: &crate::Plan) -> Result<Vec<InvariantCheck>, RuntimeError> {
                Ok(plan
                    .resource
                    .invariants()
                    .iter()
                    .cloned()
                    .map(|invariant| InvariantCheck::new(invariant, true, None))
                    .collect())
            }

            fn prepare(&mut self, plan: &ValidatedPlan) -> Result<Actuation, RuntimeError> {
                let target = plan
                    .plan
                    .candidate()
                    .and_then(|candidate| candidate.magnitude());
                Ok(Actuation::new(plan.clone(), target, self.name()))
            }

            fn actuate(&mut self, _actuation: &Actuation) -> Result<(), RuntimeError> {
                Ok(())
            }

            fn verify(&self, _actuation: &Actuation) -> Result<VerificationResult, RuntimeError> {
                Ok(VerificationResult::Pass)
            }

            fn commit(&mut self, _actuation: &Actuation) -> Result<CommitRecord, RuntimeError> {
                Ok(CommitRecord::new("passing", "verified"))
            }

            fn rollback(
                &mut self,
                _actuation: &Actuation,
                _verification: &VerificationResult,
            ) -> Result<RollbackRecord, RuntimeError> {
                Err(RuntimeError::rollback("rollback must not be reached"))
            }
        }

        let runtime = applying_runtime();
        let resource = runtime.config().ir_resource.clone();
        let mut actuator = PassingActuator;
        let attempt = runtime.cycle_attempt(
            &resource,
            &FirstGroundedPlanner,
            &(),
            &mut actuator,
        );

        assert!(attempt.is_completed());
        let result = attempt.into_result().expect("attempt should complete");
        assert!(result.commit.is_some());
        assert!(result.rollback.is_none());
    }
}
