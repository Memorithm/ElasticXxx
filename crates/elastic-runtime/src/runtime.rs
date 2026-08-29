//! Runtime orchestrator.

use crate::actuation::Actuation;
use crate::commit::{CommitRecord, RollbackRecord};
use crate::config::{RuntimeConfig, RuntimeMode};
use crate::control_loop::observe_and_plan;
use crate::error::RuntimeError;
use crate::events::{RuntimeEvent, RuntimeEventKind};
use crate::observation::{ObservationSnapshot, Observer};
use crate::plan::{validate_with_checks, ValidatedPlan};
use crate::transaction::TransactionalActuator;
use crate::verification::VerificationResult;
use elastic_eir::{EirResource, TransitionPlanner};

/// High-level runtime orchestrator.
#[derive(Clone, Debug)]
pub struct Runtime {
    config: RuntimeConfig,
}

impl Runtime {
    pub fn new(config: RuntimeConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &RuntimeConfig {
        &self.config
    }

    /// Run one transaction cycle:
    /// observe → plan → validate → prepare → actuate → verify → commit/rollback.
    ///
    /// A plan without a candidate is an honest non-actuating cycle. A plan is
    /// never actuated unless the trusted actuator supplies successful checks
    /// for every applicable invariant. Verification failure or inconclusive
    /// verification always triggers rollback. Rollback failure is returned as
    /// an explicit [`RuntimeError::Rollback`].
    pub fn cycle<P, O, A>(
        &self,
        resource: &EirResource,
        planner: &P,
        observer: &O,
        actuator: &mut A,
    ) -> Result<CycleResult, RuntimeError>
    where
        P: TransitionPlanner,
        O: Observer,
        A: TransactionalActuator,
    {
        let mut events = vec![RuntimeEvent::new(
            RuntimeEventKind::CycleStarted,
            "cycle started",
        )];

        let (snapshot, plan) = observe_and_plan(planner, resource, observer)?;
        events.push(RuntimeEvent::new(
            RuntimeEventKind::ObservationCollected,
            format!("collected {} observations", snapshot.len()),
        ));

        if plan.candidate().is_none() {
            events.push(RuntimeEvent::new(
                RuntimeEventKind::PlanRejected,
                plan.outcome.to_string(),
            ));
            events.push(RuntimeEvent::new(
                RuntimeEventKind::CycleCompleted,
                "cycle completed without actuation",
            ));
            return Ok(CycleResult {
                observations: vec![snapshot],
                plan: Some(ValidatedPlan::new(plan, Vec::new(), false)),
                actuation: None,
                verification: None,
                commit: None,
                rollback: None,
                events,
            });
        }

        events.push(RuntimeEvent::new(
            RuntimeEventKind::PlanSelected,
            plan.reasoning.clone(),
        ));

        let checks = actuator.validate(&plan)?;
        for check in &checks {
            events.push(RuntimeEvent::new(
                RuntimeEventKind::InvariantChecked,
                format!("{}: {}", check.invariant, check.holds),
            ));
        }
        let validated = validate_with_checks(plan, checks);

        if !validated.validated {
            events.push(RuntimeEvent::new(
                RuntimeEventKind::PlanRejected,
                "trusted invariant validation did not authorize actuation",
            ));
            events.push(RuntimeEvent::new(
                RuntimeEventKind::CycleCompleted,
                "cycle completed without actuation",
            ));
            return Ok(CycleResult {
                observations: vec![snapshot],
                plan: Some(validated),
                actuation: None,
                verification: None,
                commit: None,
                rollback: None,
                events,
            });
        }

        events.push(RuntimeEvent::new(
            RuntimeEventKind::PlanValidated,
            "trusted validation authorized candidate",
        ));

        if self.config.dry_run
            || matches!(
                self.config.mode,
                RuntimeMode::DryRun | RuntimeMode::ObserveOnly
            )
        {
            events.push(RuntimeEvent::new(
                RuntimeEventKind::CycleCompleted,
                "non-actuating runtime mode completed before physical actuation",
            ));
            return Ok(CycleResult {
                observations: vec![snapshot],
                plan: Some(validated),
                actuation: None,
                verification: None,
                commit: None,
                rollback: None,
                events,
            });
        }

        let actuation = actuator.prepare(&validated)?;
        if !actuation.is_valid() {
            return Err(RuntimeError::validation(
                "trusted actuator prepared an invalid actuation",
            ));
        }
        events.push(RuntimeEvent::new(
            RuntimeEventKind::ActuationPrepared,
            format!("prepared by {}", actuator.name()),
        ));

        let verification = match actuator.actuate(&actuation) {
            Ok(()) => {
                events.push(RuntimeEvent::new(
                    RuntimeEventKind::ActuationApplied,
                    format!("applied by {}", actuator.name()),
                ));
                match actuator.verify(&actuation) {
                    Ok(result) => result,
                    Err(error) => VerificationResult::Inconclusive {
                        detail: error.to_string(),
                    },
                }
            }
            Err(error) => VerificationResult::Inconclusive {
                detail: format!("actuation failed and may be partial: {error}"),
            },
        };
        events.push(RuntimeEvent::new(
            RuntimeEventKind::VerificationPerformed,
            format!("{verification:?}"),
        ));

        if verification.is_pass() {
            match actuator.commit(&actuation) {
                Ok(commit) => {
                    events.push(RuntimeEvent::new(
                        RuntimeEventKind::CommitExecuted,
                        commit.rationale.clone(),
                    ));
                    events.push(RuntimeEvent::new(
                        RuntimeEventKind::CycleCompleted,
                        "cycle committed",
                    ));
                    return Ok(CycleResult {
                        observations: vec![snapshot],
                        plan: Some(validated),
                        actuation: Some(actuation),
                        verification: Some(verification),
                        commit: Some(commit),
                        rollback: None,
                        events,
                    });
                }
                Err(commit_error) => {
                    let rollback =
                        actuator
                            .rollback(&actuation, &verification)
                            .map_err(|error| {
                                RuntimeError::rollback(format!(
                                    "commit failed ({commit_error}); rollback also failed ({error})"
                                ))
                            })?;
                    let rollback = require_restored_rollback(
                        rollback,
                        &format!("commit failed ({commit_error})"),
                    )?;
                    events.push(RuntimeEvent::new(
                        RuntimeEventKind::RollbackExecuted,
                        rollback.rationale.clone(),
                    ));
                    events.push(RuntimeEvent::new(
                        RuntimeEventKind::CycleCompleted,
                        "commit failed; actuation rolled back",
                    ));
                    return Ok(CycleResult {
                        observations: vec![snapshot],
                        plan: Some(validated),
                        actuation: Some(actuation),
                        verification: Some(verification),
                        commit: None,
                        rollback: Some(rollback),
                        events,
                    });
                }
            }
        }

        let rollback = actuator
            .rollback(&actuation, &verification)
            .map_err(|error| {
                RuntimeError::rollback(format!(
                    "verification did not pass ({verification:?}); rollback failed ({error})"
                ))
            })?;
        let rollback = require_restored_rollback(
            rollback,
            &format!("verification did not pass ({verification:?})"),
        )?;
        events.push(RuntimeEvent::new(
            RuntimeEventKind::RollbackExecuted,
            rollback.rationale.clone(),
        ));
        events.push(RuntimeEvent::new(
            RuntimeEventKind::CycleCompleted,
            "verification did not pass; actuation rolled back",
        ));

        Ok(CycleResult {
            observations: vec![snapshot],
            plan: Some(validated),
            actuation: Some(actuation),
            verification: Some(verification),
            commit: None,
            rollback: Some(rollback),
            events,
        })
    }
}

fn require_restored_rollback(
    rollback: RollbackRecord,
    failure_context: &str,
) -> Result<RollbackRecord, RuntimeError> {
    if rollback.invariants_restored {
        Ok(rollback)
    } else {
        Err(RuntimeError::rollback(format!(
            "{failure_context}; rollback completed without restoring invariants"
        )))
    }
}

/// Result of a runtime cycle.
#[derive(Clone, Debug)]
pub struct CycleResult {
    pub observations: Vec<ObservationSnapshot>,
    pub plan: Option<ValidatedPlan>,
    pub actuation: Option<Actuation>,
    pub verification: Option<VerificationResult>,
    pub commit: Option<CommitRecord>,
    pub rollback: Option<RollbackRecord>,
    pub events: Vec<RuntimeEvent>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InvariantCheck, Plan};
    use elastic_eir::FirstGroundedPlanner;

    struct MockActuator {
        verification: VerificationResult,
        fail_actuation: bool,
        fail_commit: bool,
        fail_rollback: bool,
        restore_invariants: bool,
        committed: bool,
        rolled_back: bool,
    }

    impl MockActuator {
        fn new(verification: VerificationResult) -> Self {
            Self {
                verification,
                fail_actuation: false,
                fail_commit: false,
                fail_rollback: false,
                restore_invariants: true,
                committed: false,
                rolled_back: false,
            }
        }
    }

    impl TransactionalActuator for MockActuator {
        fn name(&self) -> &str {
            "mock"
        }

        fn validate(&self, plan: &Plan) -> Result<Vec<InvariantCheck>, RuntimeError> {
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
            if self.fail_actuation {
                return Err(RuntimeError::actuation("mock actuation failure"));
            }
            Ok(())
        }

        fn verify(&self, _actuation: &Actuation) -> Result<VerificationResult, RuntimeError> {
            Ok(self.verification.clone())
        }

        fn commit(&mut self, _actuation: &Actuation) -> Result<CommitRecord, RuntimeError> {
            if self.fail_commit {
                return Err(RuntimeError::commit("mock commit failure"));
            }
            self.committed = true;
            Ok(CommitRecord::new("mock", "verified mock transition"))
        }

        fn rollback(
            &mut self,
            _actuation: &Actuation,
            _verification: &VerificationResult,
        ) -> Result<RollbackRecord, RuntimeError> {
            if self.fail_rollback {
                return Err(RuntimeError::rollback("mock rollback failure"));
            }
            self.rolled_back = true;
            Ok(RollbackRecord::new(
                "mock",
                "restored mock state",
                self.restore_invariants,
            ))
        }
    }

    fn applying_runtime() -> Runtime {
        Runtime::new(RuntimeConfig {
            dry_run: false,
            ..RuntimeConfig::default()
        })
    }

    #[test]
    fn verified_transaction_commits() {
        let runtime = applying_runtime();
        let resource = runtime.config().ir_resource.clone();
        let mut actuator = MockActuator::new(VerificationResult::Pass);

        let result = runtime
            .cycle(&resource, &FirstGroundedPlanner, &(), &mut actuator)
            .expect("verified transaction should complete");

        assert!(actuator.committed);
        assert!(!actuator.rolled_back);
        assert!(result.commit.is_some());
        assert!(result.rollback.is_none());
    }

    #[test]
    fn failed_verification_rolls_back_without_commit() {
        let runtime = applying_runtime();
        let resource = runtime.config().ir_resource.clone();
        let mut actuator = MockActuator::new(VerificationResult::Fail {
            detail: "injected verification failure".to_owned(),
        });

        let result = runtime
            .cycle(&resource, &FirstGroundedPlanner, &(), &mut actuator)
            .expect("rollback should restore the transaction");

        assert!(!actuator.committed);
        assert!(actuator.rolled_back);
        assert!(result.commit.is_none());
        assert!(result.rollback.is_some());
    }

    #[test]
    fn actuation_failure_attempts_rollback() {
        let runtime = applying_runtime();
        let resource = runtime.config().ir_resource.clone();
        let mut actuator = MockActuator::new(VerificationResult::Pass);
        actuator.fail_actuation = true;

        let result = runtime
            .cycle(&resource, &FirstGroundedPlanner, &(), &mut actuator)
            .expect("partial actuation failure should be recovered by rollback");

        assert!(!actuator.committed);
        assert!(actuator.rolled_back);
        assert!(matches!(
            result.verification,
            Some(VerificationResult::Inconclusive { .. })
        ));
        assert!(result.rollback.is_some());
    }

    #[test]
    fn rollback_failure_is_never_swallowed() {
        let runtime = applying_runtime();
        let resource = runtime.config().ir_resource.clone();
        let mut actuator = MockActuator::new(VerificationResult::Inconclusive {
            detail: "injected inconclusive verification".to_owned(),
        });
        actuator.fail_rollback = true;

        let error = runtime
            .cycle(&resource, &FirstGroundedPlanner, &(), &mut actuator)
            .expect_err("rollback failure must escape the cycle");

        assert!(matches!(error, RuntimeError::Rollback(_)));
        assert!(!actuator.committed);
    }

    #[test]
    fn rollback_without_restored_invariants_is_an_error() {
        let runtime = applying_runtime();
        let resource = runtime.config().ir_resource.clone();
        let mut actuator = MockActuator::new(VerificationResult::Fail {
            detail: "injected verification failure".to_owned(),
        });
        actuator.restore_invariants = false;

        let error = runtime
            .cycle(&resource, &FirstGroundedPlanner, &(), &mut actuator)
            .expect_err("unrestored invariants must escape the cycle");

        assert!(matches!(error, RuntimeError::Rollback(_)));
        assert!(actuator.rolled_back);
        assert!(!actuator.committed);
    }

    #[test]
    fn commit_failure_with_unrestored_rollback_is_an_error() {
        let runtime = applying_runtime();
        let resource = runtime.config().ir_resource.clone();
        let mut actuator = MockActuator::new(VerificationResult::Pass);
        actuator.fail_commit = true;
        actuator.restore_invariants = false;

        let error = runtime
            .cycle(&resource, &FirstGroundedPlanner, &(), &mut actuator)
            .expect_err("unrestored commit rollback must escape the cycle");

        assert!(matches!(error, RuntimeError::Rollback(_)));
        assert!(actuator.rolled_back);
        assert!(!actuator.committed);
    }

    #[test]
    fn dry_run_stops_before_prepare_and_actuation() {
        let runtime = Runtime::new(RuntimeConfig::default());
        let resource = runtime.config().ir_resource.clone();
        let mut actuator = MockActuator::new(VerificationResult::Pass);

        let result = runtime
            .cycle(&resource, &FirstGroundedPlanner, &(), &mut actuator)
            .expect("dry run should complete");

        assert!(result.plan.as_ref().is_some_and(|plan| plan.validated));
        assert!(result.actuation.is_none());
        assert!(result.verification.is_none());
        assert!(result.commit.is_none());
        assert!(!actuator.committed);
    }

    #[test]
    fn non_actuating_modes_override_the_legacy_dry_run_flag() {
        for mode in [RuntimeMode::DryRun, RuntimeMode::ObserveOnly] {
            let config = RuntimeConfig {
                dry_run: false,
                mode,
                ..RuntimeConfig::default()
            };
            let runtime = Runtime::new(config);
            let resource = runtime.config().ir_resource.clone();
            let mut actuator = MockActuator::new(VerificationResult::Pass);

            let result = runtime
                .cycle(&resource, &FirstGroundedPlanner, &(), &mut actuator)
                .expect("non-actuating mode should complete");

            assert!(result.plan.as_ref().is_some_and(|plan| plan.validated));
            assert!(result.actuation.is_none());
            assert!(result.verification.is_none());
            assert!(result.commit.is_none());
            assert!(!actuator.committed);
            assert!(!actuator.rolled_back);
        }
    }
}
