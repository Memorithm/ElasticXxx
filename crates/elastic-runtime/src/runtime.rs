//! Runtime orchestrator.

use std::time::Duration;

use crate::actuation::Actuation;
use crate::cancellation::CancellationToken;
use crate::clock::{RuntimeClock, SystemClock};
use crate::commit::{CommitRecord, RollbackRecord};
use crate::config::{Cadence, RuntimeConfig, RuntimeMode};
use crate::control_loop::collect_observations;
use crate::error::RuntimeError;
use crate::events::{NoopEventSink, RuntimeEvent, RuntimeEventKind, RuntimeEventSink};
use crate::observation::{ObservationSnapshot, Observer};
use crate::plan::{plan_with_context, validate_with_checks, ValidatedPlan};
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

    /// Run one control cycle and collect its audit events.
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
        let mut sink = NoopEventSink;
        self.cycle_with_sink(resource, planner, observer, actuator, &mut sink)
    }

    /// Run one control cycle while streaming events to `sink` as they occur.
    pub fn cycle_with_sink<P, O, A, S>(
        &self,
        resource: &EirResource,
        planner: &P,
        observer: &O,
        actuator: &mut A,
        sink: &mut S,
    ) -> Result<CycleResult, RuntimeError>
    where
        P: TransitionPlanner,
        O: Observer,
        A: TransactionalActuator,
        S: RuntimeEventSink,
    {
        let mut events = Vec::new();
        record_event(
            &mut events,
            sink,
            RuntimeEventKind::CycleStarted,
            "cycle started",
        );

        let (context, snapshot) = collect_observations(observer);
        record_event(
            &mut events,
            sink,
            RuntimeEventKind::ObservationCollected,
            format!("collected {} observations", snapshot.len()),
        );

        if matches!(&self.config.mode, RuntimeMode::ObserveOnly) {
            record_event(
                &mut events,
                sink,
                RuntimeEventKind::CycleCompleted,
                "observe-only cycle completed before planning",
            );
            return Ok(CycleResult {
                observations: vec![snapshot],
                plan: None,
                actuation: None,
                verification: None,
                commit: None,
                rollback: None,
                events,
            });
        }

        let plan = plan_with_context(planner, resource, &context);
        if plan.candidate().is_some() {
            record_event(
                &mut events,
                sink,
                RuntimeEventKind::PlanSelected,
                plan.reasoning.clone(),
            );
        } else {
            record_event(
                &mut events,
                sink,
                RuntimeEventKind::PlanRejected,
                plan.outcome.to_string(),
            );
        }

        if matches!(&self.config.mode, RuntimeMode::PlanOnly) {
            record_event(
                &mut events,
                sink,
                RuntimeEventKind::CycleCompleted,
                "plan-only cycle completed before trusted validation",
            );
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

        if plan.candidate().is_none() {
            record_event(
                &mut events,
                sink,
                RuntimeEventKind::CycleCompleted,
                "cycle completed without actuation",
            );
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

        let checks = actuator.validate(&plan)?;
        for check in &checks {
            record_event(
                &mut events,
                sink,
                RuntimeEventKind::InvariantChecked,
                format!("{}: {}", check.invariant, check.holds),
            );
        }
        let validated = validate_with_checks(plan, checks);

        if !validated.validated {
            record_event(
                &mut events,
                sink,
                RuntimeEventKind::PlanRejected,
                "trusted invariant validation did not authorize actuation",
            );
            record_event(
                &mut events,
                sink,
                RuntimeEventKind::CycleCompleted,
                "cycle completed without actuation",
            );
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

        record_event(
            &mut events,
            sink,
            RuntimeEventKind::PlanValidated,
            "trusted validation authorized candidate",
        );

        if self.config.dry_run || matches!(&self.config.mode, RuntimeMode::DryRun) {
            record_event(
                &mut events,
                sink,
                RuntimeEventKind::CycleCompleted,
                "dry-run cycle completed before physical actuation",
            );
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
        record_event(
            &mut events,
            sink,
            RuntimeEventKind::ActuationPrepared,
            format!("prepared by {}", actuator.name()),
        );

        let verification = match actuator.actuate(&actuation) {
            Ok(()) => {
                record_event(
                    &mut events,
                    sink,
                    RuntimeEventKind::ActuationApplied,
                    format!("applied by {}", actuator.name()),
                );
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
        record_event(
            &mut events,
            sink,
            RuntimeEventKind::VerificationPerformed,
            format!("{verification:?}"),
        );

        if verification.is_pass() {
            match actuator.commit(&actuation) {
                Ok(commit) => {
                    record_event(
                        &mut events,
                        sink,
                        RuntimeEventKind::CommitExecuted,
                        commit.rationale.clone(),
                    );
                    record_event(
                        &mut events,
                        sink,
                        RuntimeEventKind::CycleCompleted,
                        "cycle committed",
                    );
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
                    record_event(
                        &mut events,
                        sink,
                        RuntimeEventKind::RollbackExecuted,
                        rollback.rationale.clone(),
                    );
                    record_event(
                        &mut events,
                        sink,
                        RuntimeEventKind::CycleCompleted,
                        "commit failed; actuation rolled back",
                    );
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
        record_event(
            &mut events,
            sink,
            RuntimeEventKind::RollbackExecuted,
            rollback.rationale.clone(),
        );
        record_event(
            &mut events,
            sink,
            RuntimeEventKind::CycleCompleted,
            "verification did not pass; actuation rolled back",
        );

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

    /// Run a bounded controller using the system clock.
    pub fn run<P, O, A>(
        &self,
        resource: &EirResource,
        planner: &P,
        observer: &O,
        actuator: &mut A,
        cancellation: &CancellationToken,
    ) -> Result<RunResult, RuntimeError>
    where
        P: TransitionPlanner,
        O: Observer,
        A: TransactionalActuator,
    {
        let mut sink = NoopEventSink;
        self.run_with_clock_and_sink(
            resource,
            planner,
            observer,
            actuator,
            cancellation,
            &SystemClock,
            &mut sink,
        )
    }

    /// Run a bounded controller with an injectable clock.
    pub fn run_with_clock<P, O, A, C>(
        &self,
        resource: &EirResource,
        planner: &P,
        observer: &O,
        actuator: &mut A,
        cancellation: &CancellationToken,
        clock: &C,
    ) -> Result<RunResult, RuntimeError>
    where
        P: TransitionPlanner,
        O: Observer,
        A: TransactionalActuator,
        C: RuntimeClock,
    {
        let mut sink = NoopEventSink;
        self.run_with_clock_and_sink(
            resource,
            planner,
            observer,
            actuator,
            cancellation,
            clock,
            &mut sink,
        )
    }

    /// Run a bounded controller with deterministic timing and streamed events.
    pub fn run_with_clock_and_sink<P, O, A, C, S>(
        &self,
        resource: &EirResource,
        planner: &P,
        observer: &O,
        actuator: &mut A,
        cancellation: &CancellationToken,
        clock: &C,
        sink: &mut S,
    ) -> Result<RunResult, RuntimeError>
    where
        P: TransitionPlanner,
        O: Observer,
        A: TransactionalActuator,
        C: RuntimeClock,
        S: RuntimeEventSink,
    {
        let (cycle_limit, interval) = self.loop_schedule()?;
        // Do not reserve from an operator-controlled cycle count. A very large
        // bound must not cause a large allocation before cancellation or the
        // first cycle has even been observed.
        let mut cycles = Vec::new();
        let mut events = Vec::new();

        record_event(
            &mut events,
            sink,
            RuntimeEventKind::ControlLoopStarted,
            format!("bounded control loop started with max_cycles={cycle_limit}"),
        );

        loop {
            if cancellation.is_cancelled() {
                record_event(
                    &mut events,
                    sink,
                    RuntimeEventKind::CancellationObserved,
                    "cancellation observed before next cycle",
                );
                record_event(
                    &mut events,
                    sink,
                    RuntimeEventKind::ControlLoopStopped,
                    "control loop cancelled",
                );
                return Ok(RunResult {
                    cycles,
                    events,
                    stop_reason: LoopStopReason::Cancelled,
                });
            }

            let cycle = match self.cycle_with_sink(resource, planner, observer, actuator, sink) {
                Ok(cycle) => cycle,
                Err(error) => {
                    record_event(
                        &mut events,
                        sink,
                        RuntimeEventKind::ErrorEncountered,
                        error.to_string(),
                    );
                    record_event(
                        &mut events,
                        sink,
                        RuntimeEventKind::ControlLoopStopped,
                        "control loop stopped after cycle error",
                    );
                    return Err(error);
                }
            };
            events.extend(cycle.events.iter().cloned());
            cycles.push(cycle);

            if cancellation.is_cancelled() {
                record_event(
                    &mut events,
                    sink,
                    RuntimeEventKind::CancellationObserved,
                    "cancellation observed after completed cycle",
                );
                record_event(
                    &mut events,
                    sink,
                    RuntimeEventKind::ControlLoopStopped,
                    "control loop cancelled",
                );
                return Ok(RunResult {
                    cycles,
                    events,
                    stop_reason: LoopStopReason::Cancelled,
                });
            }

            if cycles.len() as u64 >= cycle_limit {
                let stop_reason = if interval.is_some() {
                    LoopStopReason::MaxCyclesReached
                } else {
                    LoopStopReason::OneShotCompleted
                };
                record_event(
                    &mut events,
                    sink,
                    RuntimeEventKind::ControlLoopStopped,
                    "bounded control loop completed",
                );
                return Ok(RunResult {
                    cycles,
                    events,
                    stop_reason,
                });
            }

            if let Some(interval) = interval {
                clock.sleep(interval);
            }
        }
    }

    fn loop_schedule(&self) -> Result<(u64, Option<Duration>), RuntimeError> {
        let cadence_interval = match &self.config.cadence {
            Cadence::OneShot => None,
            Cadence::Periodic(interval) => Some(*interval),
        };
        let interval = cadence_interval.or_else(|| {
            matches!(&self.config.mode, RuntimeMode::Periodic)
                .then(|| Duration::from_millis(self.config.interval_ms))
        });

        let Some(interval) = interval else {
            return Ok((1, None));
        };

        if interval.is_zero() {
            return Err(RuntimeError::configuration(
                "periodic control-loop interval must be greater than zero",
            ));
        }
        if self.config.max_cycles == 0 {
            return Err(RuntimeError::configuration(
                "periodic control loops must set max_cycles > 0",
            ));
        }

        Ok((self.config.max_cycles, Some(interval)))
    }
}

fn record_event<S: RuntimeEventSink>(
    events: &mut Vec<RuntimeEvent>,
    sink: &mut S,
    kind: RuntimeEventKind,
    details: impl Into<String>,
) {
    let event = RuntimeEvent::new(kind, details);
    sink.emit(&event);
    events.push(event);
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

/// Why a bounded control loop terminated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopStopReason {
    OneShotCompleted,
    MaxCyclesReached,
    Cancelled,
}

/// Result of a bounded runtime invocation.
#[derive(Clone, Debug)]
pub struct RunResult {
    pub cycles: Vec<CycleResult>,
    pub events: Vec<RuntimeEvent>,
    pub stop_reason: LoopStopReason,
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::{InvariantCheck, Plan};
    use elastic_eir::FirstGroundedPlanner;

    struct MockActuator {
        verification: VerificationResult,
        fail_actuation: bool,
        fail_commit: bool,
        fail_rollback: bool,
        restore_invariants: bool,
        validation_calls: std::cell::Cell<usize>,
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
                validation_calls: std::cell::Cell::new(0),
                committed: false,
                rolled_back: false,
            }
        }
    }

    impl TransactionalActuator for MockActuator {
        fn name(&self) -> &str {
            "mock"
        }

        fn validate(&self, plan: &crate::Plan) -> Result<Vec<InvariantCheck>, RuntimeError> {
            self.validation_calls
                .set(self.validation_calls.get().saturating_add(1));
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

    #[derive(Default)]
    struct FakeClock {
        sleeps: Mutex<Vec<Duration>>,
    }

    impl RuntimeClock for FakeClock {
        fn sleep(&self, duration: Duration) {
            self.sleeps
                .lock()
                .expect("fake clock mutex should not be poisoned")
                .push(duration);
        }
    }

    struct CancellingSink {
        cancellation: CancellationToken,
        completed_cycles: usize,
    }

    impl RuntimeEventSink for CancellingSink {
        fn emit(&mut self, event: &RuntimeEvent) {
            if event.kind == RuntimeEventKind::CycleCompleted {
                self.completed_cycles += 1;
                if self.completed_cycles == 1 {
                    self.cancellation.cancel();
                }
            }
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
    fn observe_only_stops_before_planning_validation_boundary() {
        let runtime = Runtime::new(RuntimeConfig {
            mode: RuntimeMode::ObserveOnly,
            dry_run: false,
            ..RuntimeConfig::default()
        });
        let resource = runtime.config().ir_resource.clone();
        let mut actuator = MockActuator::new(VerificationResult::Pass);

        let result = runtime
            .cycle(&resource, &FirstGroundedPlanner, &(), &mut actuator)
            .expect("observe-only cycle should complete");

        assert!(result.plan.is_none());
        assert_eq!(actuator.validation_calls.get(), 0);
        assert!(result.actuation.is_none());
    }

    #[test]
    fn plan_only_stops_before_trusted_validation() {
        let runtime = Runtime::new(RuntimeConfig {
            mode: RuntimeMode::PlanOnly,
            dry_run: false,
            ..RuntimeConfig::default()
        });
        let resource = runtime.config().ir_resource.clone();
        let mut actuator = MockActuator::new(VerificationResult::Pass);

        let result = runtime
            .cycle(&resource, &FirstGroundedPlanner, &(), &mut actuator)
            .expect("plan-only cycle should complete");

        assert!(result.plan.as_ref().is_some_and(|plan| !plan.validated));
        assert_eq!(actuator.validation_calls.get(), 0);
        assert!(result.actuation.is_none());
    }

    #[test]
    fn periodic_loop_is_bounded_and_sleeps_between_cycles() {
        let runtime = Runtime::new(RuntimeConfig {
            cadence: Cadence::Periodic(Duration::from_millis(5)),
            mode: RuntimeMode::DryRun,
            max_cycles: 3,
            dry_run: false,
            ..RuntimeConfig::default()
        });
        let resource = runtime.config().ir_resource.clone();
        let mut actuator = MockActuator::new(VerificationResult::Pass);
        let clock = FakeClock::default();
        let cancellation = CancellationToken::new();

        let result = runtime
            .run_with_clock(
                &resource,
                &FirstGroundedPlanner,
                &(),
                &mut actuator,
                &cancellation,
                &clock,
            )
            .expect("bounded periodic loop should complete");

        assert_eq!(result.cycles.len(), 3);
        assert_eq!(result.stop_reason, LoopStopReason::MaxCyclesReached);
        assert_eq!(
            clock
                .sleeps
                .lock()
                .expect("fake clock mutex should not be poisoned")
                .as_slice(),
            &[Duration::from_millis(5), Duration::from_millis(5)]
        );
    }

    #[test]
    fn cancellation_stops_before_sleeping_again() {
        let runtime = Runtime::new(RuntimeConfig {
            cadence: Cadence::Periodic(Duration::from_millis(5)),
            mode: RuntimeMode::DryRun,
            max_cycles: 5,
            dry_run: false,
            ..RuntimeConfig::default()
        });
        let resource = runtime.config().ir_resource.clone();
        let mut actuator = MockActuator::new(VerificationResult::Pass);
        let clock = FakeClock::default();
        let cancellation = CancellationToken::new();
        let mut sink = CancellingSink {
            cancellation: cancellation.clone(),
            completed_cycles: 0,
        };

        let result = runtime
            .run_with_clock_and_sink(
                &resource,
                &FirstGroundedPlanner,
                &(),
                &mut actuator,
                &cancellation,
                &clock,
                &mut sink,
            )
            .expect("cancelled loop should stop cleanly");

        assert_eq!(result.cycles.len(), 1);
        assert_eq!(result.stop_reason, LoopStopReason::Cancelled);
        assert!(clock
            .sleeps
            .lock()
            .expect("fake clock mutex should not be poisoned")
            .is_empty());
    }

    #[test]
    fn pre_cancelled_extreme_cycle_limit_does_not_preallocate() {
        let runtime = Runtime::new(RuntimeConfig {
            cadence: Cadence::Periodic(Duration::from_millis(1)),
            mode: RuntimeMode::DryRun,
            max_cycles: u64::MAX,
            dry_run: false,
            ..RuntimeConfig::default()
        });
        let resource = runtime.config().ir_resource.clone();
        let mut actuator = MockActuator::new(VerificationResult::Pass);
        let clock = FakeClock::default();
        let cancellation = CancellationToken::new();
        cancellation.cancel();

        let result = runtime
            .run_with_clock(
                &resource,
                &FirstGroundedPlanner,
                &(),
                &mut actuator,
                &cancellation,
                &clock,
            )
            .expect("pre-cancelled loop should stop without allocating from the cycle limit");

        assert!(result.cycles.is_empty());
        assert_eq!(result.stop_reason, LoopStopReason::Cancelled);
        assert!(clock
            .sleeps
            .lock()
            .expect("fake clock mutex should not be poisoned")
            .is_empty());
    }

    #[test]
    fn periodic_loop_rejects_unbounded_configuration() {
        let runtime = Runtime::new(RuntimeConfig {
            cadence: Cadence::Periodic(Duration::from_millis(5)),
            max_cycles: 0,
            ..RuntimeConfig::default()
        });
        let resource = runtime.config().ir_resource.clone();
        let mut actuator = MockActuator::new(VerificationResult::Pass);
        let clock = FakeClock::default();
        let cancellation = CancellationToken::new();

        let error = runtime
            .run_with_clock(
                &resource,
                &FirstGroundedPlanner,
                &(),
                &mut actuator,
                &cancellation,
                &clock,
            )
            .expect_err("periodic loop must require an explicit bound");

        assert!(matches!(error, RuntimeError::Configuration(_)));
    }
}
