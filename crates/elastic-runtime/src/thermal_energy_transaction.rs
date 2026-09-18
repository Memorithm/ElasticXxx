//! Trusted BE14h transaction boundary for thermal/energy-gated local transitions.
//!
//! Boolean admission remains a precheck only. Immediately before any backend
//! mutation this module independently re-evaluates the source-bound numeric
//! thermal/energy policy from the fresh planning context and observations, then
//! enters VALIDATE → ACT → VERIFY → COMMIT or ROLLBACK. No concrete physical
//! thermal/power actuator is implemented here; callers must provide a backend
//! for a local surface they already have authority to control.

use std::fmt;
use std::time::Instant;

use elastic_core::{ObservationEpoch, ResourceGeneration, TruthValue};
use elastic_eir::{PlanningContext, TransitionCandidate};

use crate::{
    BooleanThermalEnergyPreplannerV1, BooleanThermalEnergyReportV1, BooleanThermalEnergyStatusV1,
    ObservationSnapshot,
};

/// Local trusted backend for one already-authorized transition surface.
pub trait ThermalEnergyTransitionBackendV1 {
    /// Revalidate backend-specific invariants without changing visible state.
    fn validate_transition(
        &mut self,
        candidate: &TransitionCandidate,
        planning_context: &PlanningContext,
        observations: &ObservationSnapshot,
        now: Instant,
    ) -> Result<(), String>;

    /// Apply the declared local transition.
    fn actuate_transition(&mut self, candidate: &TransitionCandidate) -> Result<(), String>;

    /// Verify post-actuation state and application invariants.
    fn verify_transition(&mut self, candidate: &TransitionCandidate) -> Result<(), String>;

    /// Mark the already-verified transition authoritative for this backend.
    fn commit_transition(&mut self, candidate: &TransitionCandidate) -> Result<(), String>;

    /// Restore the pre-transaction state after a possible mutation.
    fn rollback_transition(
        &mut self,
        candidate: &TransitionCandidate,
        reason: &str,
    ) -> Result<(), String>;
}

/// Stable trusted-transaction stage retained on failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ThermalEnergyTransactionStageV1 {
    Bind,
    Validate,
    Act,
    Verify,
    Commit,
}

impl ThermalEnergyTransactionStageV1 {
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

/// Exact transition committed after fresh numeric revalidation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommittedThermalEnergyTransitionV1 {
    candidate: TransitionCandidate,
    observation_epoch: ObservationEpoch,
    resource_generation: ResourceGeneration,
    decision_trace_json: String,
}

impl CommittedThermalEnergyTransitionV1 {
    #[must_use]
    pub const fn candidate(&self) -> &TransitionCandidate {
        &self.candidate
    }

    #[must_use]
    pub const fn observation_epoch(&self) -> ObservationEpoch {
        self.observation_epoch
    }

    #[must_use]
    pub const fn resource_generation(&self) -> ResourceGeneration {
        self.resource_generation
    }

    /// Fresh Boolean trace retained only as explanatory evidence.
    #[must_use]
    pub fn decision_trace_json(&self) -> &str {
        &self.decision_trace_json
    }
}

/// Failure evidence from the trusted transaction lifecycle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThermalEnergyTransactionFailureV1 {
    stage: ThermalEnergyTransactionStageV1,
    reason: String,
    rollback_attempted: bool,
    backend_rollback_error: Option<String>,
}

impl ThermalEnergyTransactionFailureV1 {
    #[must_use]
    pub const fn stage(&self) -> ThermalEnergyTransactionStageV1 {
        self.stage
    }

    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
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

impl fmt::Display for ThermalEnergyTransactionFailureV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "thermal/energy transaction failed at {}: {}",
            self.stage.as_str(),
            self.reason
        )?;
        if let Some(error) = &self.backend_rollback_error {
            write!(f, "; backend rollback also failed: {error}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ThermalEnergyTransactionFailureV1 {}

/// Non-actuating outcome from fresh Boolean preplanning.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ThermalEnergyTransactionBlockV1 {
    Rejected(Box<BooleanThermalEnergyReportV1>),
    InsufficientEvidence(Box<BooleanThermalEnergyReportV1>),
}

/// Guarded BE14h transaction result.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum GuardedThermalEnergyTransactionOutcomeV1 {
    Committed(CommittedThermalEnergyTransitionV1),
    Blocked(ThermalEnergyTransactionBlockV1),
}

fn failure_without_rollback(
    stage: ThermalEnergyTransactionStageV1,
    reason: impl Into<String>,
) -> ThermalEnergyTransactionFailureV1 {
    ThermalEnergyTransactionFailureV1 {
        stage,
        reason: reason.into(),
        rollback_attempted: false,
        backend_rollback_error: None,
    }
}

fn fail_after_possible_mutation<B: ThermalEnergyTransitionBackendV1>(
    backend: &mut B,
    candidate: &TransitionCandidate,
    stage: ThermalEnergyTransactionStageV1,
    reason: String,
) -> ThermalEnergyTransactionFailureV1 {
    let backend_rollback_error = backend.rollback_transition(candidate, &reason).err();
    ThermalEnergyTransactionFailureV1 {
        stage,
        reason,
        rollback_attempted: true,
        backend_rollback_error,
    }
}

struct TransactionInputs<'a> {
    planning_context: &'a PlanningContext,
    observations: &'a ObservationSnapshot,
    now: Instant,
    observation_epoch: ObservationEpoch,
    resource_generation: ResourceGeneration,
}

fn execute_after_numeric_validation<B: ThermalEnergyTransitionBackendV1>(
    planner: &BooleanThermalEnergyPreplannerV1,
    inputs: TransactionInputs<'_>,
    decision_trace_json: String,
    backend: &mut B,
) -> Result<CommittedThermalEnergyTransitionV1, ThermalEnergyTransactionFailureV1> {
    execute_after_numeric_validation_with_action_clock(
        planner,
        inputs,
        decision_trace_json,
        backend,
        Instant::now,
    )
}

fn execute_after_numeric_validation_with_action_clock<
    B: ThermalEnergyTransitionBackendV1,
    F: FnOnce() -> Instant,
>(
    planner: &BooleanThermalEnergyPreplannerV1,
    inputs: TransactionInputs<'_>,
    decision_trace_json: String,
    backend: &mut B,
    action_now: F,
) -> Result<CommittedThermalEnergyTransitionV1, ThermalEnergyTransactionFailureV1> {
    match planner.direct_policy_truth(inputs.planning_context, inputs.observations, inputs.now) {
        TruthValue::True => {}
        TruthValue::False => {
            return Err(failure_without_rollback(
                ThermalEnergyTransactionStageV1::Validate,
                "fresh numeric thermal/energy policy rejects transition",
            ));
        }
        TruthValue::Unknown => {
            return Err(failure_without_rollback(
                ThermalEnergyTransactionStageV1::Validate,
                "fresh numeric thermal/energy evidence is unknown",
            ));
        }
    }

    let candidate = planner.declared_candidate();
    if let Err(error) = backend.validate_transition(
        &candidate,
        inputs.planning_context,
        inputs.observations,
        inputs.now,
    ) {
        return Err(failure_without_rollback(
            ThermalEnergyTransactionStageV1::Validate,
            format!("trusted backend validation failed: {error}"),
        ));
    }

    // Backend validation may itself take long enough for telemetry that was fresh
    // at planning time to expire. Re-evaluate against the real action-time clock
    // after validation and immediately before the first possibly mutating call.
    match planner.direct_policy_truth(inputs.planning_context, inputs.observations, action_now()) {
        TruthValue::True => {}
        TruthValue::False => {
            return Err(failure_without_rollback(
                ThermalEnergyTransactionStageV1::Validate,
                "action-time numeric thermal/energy policy rejects transition after backend validation",
            ));
        }
        TruthValue::Unknown => {
            return Err(failure_without_rollback(
                ThermalEnergyTransactionStageV1::Validate,
                "action-time numeric thermal/energy evidence is unknown after backend validation",
            ));
        }
    }

    if let Err(error) = backend.actuate_transition(&candidate) {
        return Err(fail_after_possible_mutation(
            backend,
            &candidate,
            ThermalEnergyTransactionStageV1::Act,
            format!("backend actuation failed: {error}"),
        ));
    }
    if let Err(error) = backend.verify_transition(&candidate) {
        return Err(fail_after_possible_mutation(
            backend,
            &candidate,
            ThermalEnergyTransactionStageV1::Verify,
            format!("backend verification failed: {error}"),
        ));
    }
    if let Err(error) = backend.commit_transition(&candidate) {
        return Err(fail_after_possible_mutation(
            backend,
            &candidate,
            ThermalEnergyTransactionStageV1::Commit,
            format!("backend commit failed: {error}"),
        ));
    }

    Ok(CommittedThermalEnergyTransitionV1 {
        candidate,
        observation_epoch: inputs.observation_epoch,
        resource_generation: inputs.resource_generation,
        decision_trace_json,
    })
}

/// Execute the source-bound numeric policy directly without Boolean pruning.
///
/// This is the explicit non-Boolean differential reference path. It still
/// performs the same fresh source/freshness/value validation and the identical
/// trusted backend transaction. An empty trace string is intentional because
/// this path does not use Boolean planning evidence.
pub fn execute_unguarded_thermal_energy_transaction<B: ThermalEnergyTransitionBackendV1>(
    planner: &BooleanThermalEnergyPreplannerV1,
    planning_context: &PlanningContext,
    observations: &ObservationSnapshot,
    now: Instant,
    observation_epoch: ObservationEpoch,
    resource_generation: ResourceGeneration,
    backend: &mut B,
) -> Result<CommittedThermalEnergyTransitionV1, ThermalEnergyTransactionFailureV1> {
    execute_after_numeric_validation(
        planner,
        TransactionInputs {
            planning_context,
            observations,
            now,
            observation_epoch,
            resource_generation,
        },
        String::new(),
        backend,
    )
}

/// Re-evaluate fresh Boolean eligibility, then independently re-check the
/// source-bound numeric policy before entering the trusted backend lifecycle.
///
/// The fresh `DecisionTrace` is explanatory evidence only. `False` and
/// `Unknown` stop before every backend method. A Boolean `True` is not authority:
/// the direct numeric policy is evaluated before backend validation and again
/// against the action-time clock immediately before any possible mutation.
pub fn execute_guarded_thermal_energy_transaction<B: ThermalEnergyTransitionBackendV1>(
    planner: &BooleanThermalEnergyPreplannerV1,
    planning_context: &PlanningContext,
    observations: &ObservationSnapshot,
    now: Instant,
    observation_epoch: ObservationEpoch,
    resource_generation: ResourceGeneration,
    backend: &mut B,
) -> Result<GuardedThermalEnergyTransactionOutcomeV1, ThermalEnergyTransactionFailureV1> {
    let report = planner
        .evaluate(
            planning_context,
            observations,
            now,
            observation_epoch,
            resource_generation,
        )
        .map_err(|error| failure_without_rollback(ThermalEnergyTransactionStageV1::Bind, error))?;

    match report.status {
        BooleanThermalEnergyStatusV1::Rejected => {
            return Ok(GuardedThermalEnergyTransactionOutcomeV1::Blocked(
                ThermalEnergyTransactionBlockV1::Rejected(Box::new(report)),
            ));
        }
        BooleanThermalEnergyStatusV1::InsufficientEvidence => {
            return Ok(GuardedThermalEnergyTransactionOutcomeV1::Blocked(
                ThermalEnergyTransactionBlockV1::InsufficientEvidence(Box::new(report)),
            ));
        }
        BooleanThermalEnergyStatusV1::Eligible => {}
    }

    let decision_trace_json = report.evidence.decision_trace_json.clone();
    execute_after_numeric_validation(
        planner,
        TransactionInputs {
            planning_context,
            observations,
            now,
            observation_epoch,
            resource_generation,
        },
        decision_trace_json,
        backend,
    )
    .map(GuardedThermalEnergyTransactionOutcomeV1::Committed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use elastic_core::resource::{
        AdmissibleTransition, CapabilityRequirement, DimensionId, LogicalResourceId,
        ObservationSignalId, ResourceClassId, ResourceSpec,
    };
    use elastic_core::TransitionMechanism;

    use crate::{DecisionTrace, Observation, ObservationSource, THERMAL_ENERGY_MAX_AGE};

    fn planner() -> BooleanThermalEnergyPreplannerV1 {
        planner_with_max_age(THERMAL_ENERGY_MAX_AGE)
    }

    fn planner_with_max_age(max_age: Duration) -> BooleanThermalEnergyPreplannerV1 {
        let spec = ResourceSpec::builder(
            ResourceClassId::CONFIGURATIONAL,
            LogicalResourceId::new("be14h-transaction-test").unwrap(),
        )
        .allow(DimensionId::ENERGY)
        .admit(AdmissibleTransition::new(
            TransitionMechanism::Reinterpret,
            DimensionId::ENERGY,
        ))
        .require_capability(CapabilityRequirement::new(
            TransitionMechanism::Reinterpret,
            DimensionId::ENERGY,
        ))
        .observe(ObservationSignalId::THERMAL_MARGIN)
        .observe(ObservationSignalId::ENERGY_RATE)
        .build()
        .unwrap();
        BooleanThermalEnergyPreplannerV1::new(
            spec,
            TransitionMechanism::Reinterpret,
            DimensionId::ENERGY,
            8.0,
            75.0,
            ObservationSource::host("test:thermal"),
            ObservationSource::host("test:power"),
            max_age,
        )
        .unwrap()
    }

    fn evidence(now: Instant, thermal: f64, power: f64) -> (PlanningContext, ObservationSnapshot) {
        let context = PlanningContext::new()
            .observe(ObservationSignalId::THERMAL_MARGIN, thermal)
            .observe(ObservationSignalId::ENERGY_RATE, power);
        let snapshot = ObservationSnapshot::new(
            now,
            vec![
                Observation::from_source(
                    ObservationSource::host("test:thermal"),
                    ObservationSignalId::THERMAL_MARGIN,
                    thermal,
                    now,
                ),
                Observation::from_source(
                    ObservationSource::host("test:power"),
                    ObservationSignalId::ENERGY_RATE,
                    power,
                    now,
                ),
            ],
        );
        (context, snapshot)
    }

    #[derive(Default)]
    struct TestBackend {
        calls: Vec<&'static str>,
        applied: bool,
        committed: bool,
        fail_verify: bool,
    }

    impl ThermalEnergyTransitionBackendV1 for TestBackend {
        fn validate_transition(
            &mut self,
            _candidate: &TransitionCandidate,
            _planning_context: &PlanningContext,
            _observations: &ObservationSnapshot,
            _now: Instant,
        ) -> Result<(), String> {
            self.calls.push("validate");
            Ok(())
        }

        fn actuate_transition(&mut self, _candidate: &TransitionCandidate) -> Result<(), String> {
            self.calls.push("act");
            self.applied = true;
            Ok(())
        }

        fn verify_transition(&mut self, _candidate: &TransitionCandidate) -> Result<(), String> {
            self.calls.push("verify");
            if self.fail_verify {
                Err("injected verify failure".into())
            } else {
                Ok(())
            }
        }

        fn commit_transition(&mut self, _candidate: &TransitionCandidate) -> Result<(), String> {
            self.calls.push("commit");
            self.committed = true;
            Ok(())
        }

        fn rollback_transition(
            &mut self,
            _candidate: &TransitionCandidate,
            _reason: &str,
        ) -> Result<(), String> {
            self.calls.push("rollback");
            self.applied = false;
            self.committed = false;
            Ok(())
        }
    }

    #[test]
    fn true_guard_executes_trusted_cycle_and_matches_direct_reference() {
        let planner = planner();
        let now = Instant::now();
        let (context, snapshot) = evidence(now, 12.0, 60.0);
        let epoch = ObservationEpoch::new(41);
        let generation = ResourceGeneration::new(7);

        let mut guarded_backend = TestBackend::default();
        let guarded = execute_guarded_thermal_energy_transaction(
            &planner,
            &context,
            &snapshot,
            now,
            epoch,
            generation,
            &mut guarded_backend,
        )
        .unwrap();
        let GuardedThermalEnergyTransactionOutcomeV1::Committed(guarded) = guarded else {
            panic!("eligible guard must enter trusted transaction");
        };
        assert_eq!(
            guarded_backend.calls,
            ["validate", "act", "verify", "commit"]
        );
        assert!(guarded_backend.committed);
        assert_eq!(guarded.observation_epoch(), epoch);
        assert_eq!(guarded.resource_generation(), generation);
        let trace =
            DecisionTrace::from_bounded_json(guarded.decision_trace_json().as_bytes()).unwrap();
        assert_eq!(trace.observation_epoch(), epoch);
        assert_eq!(trace.resource_generation(), generation);

        let mut direct_backend = TestBackend::default();
        let direct = execute_unguarded_thermal_energy_transaction(
            &planner,
            &context,
            &snapshot,
            now,
            epoch,
            generation,
            &mut direct_backend,
        )
        .unwrap();
        assert_eq!(direct_backend.calls, guarded_backend.calls);
        assert_eq!(direct.candidate(), guarded.candidate());
        assert_eq!(direct.observation_epoch(), guarded.observation_epoch());
        assert_eq!(direct.resource_generation(), guarded.resource_generation());
        assert!(direct.decision_trace_json().is_empty());
    }

    #[test]
    fn false_and_unknown_stop_before_backend() {
        let planner = planner();
        let now = Instant::now();
        for (thermal, power, expected_rejected) in [(4.0, 60.0, true), (12.0, 90.0, true)] {
            let (context, snapshot) = evidence(now, thermal, power);
            let mut backend = TestBackend::default();
            let outcome = execute_guarded_thermal_energy_transaction(
                &planner,
                &context,
                &snapshot,
                now,
                ObservationEpoch::new(1),
                ResourceGeneration::new(1),
                &mut backend,
            )
            .unwrap();
            assert!(matches!(
                outcome,
                GuardedThermalEnergyTransactionOutcomeV1::Blocked(
                    ThermalEnergyTransactionBlockV1::Rejected(_)
                )
            ));
            assert!(expected_rejected);
            assert!(backend.calls.is_empty());
        }

        let (context, mut snapshot) = evidence(now, 12.0, 60.0);
        snapshot.observations[1] = Observation::unsupported_from_source(
            ObservationSource::host("test:power"),
            ObservationSignalId::ENERGY_RATE,
            now,
            "missing test sensor",
        );
        let mut backend = TestBackend::default();
        let outcome = execute_guarded_thermal_energy_transaction(
            &planner,
            &context,
            &snapshot,
            now,
            ObservationEpoch::new(2),
            ResourceGeneration::new(1),
            &mut backend,
        )
        .unwrap();
        assert!(matches!(
            outcome,
            GuardedThermalEnergyTransactionOutcomeV1::Blocked(
                ThermalEnergyTransactionBlockV1::InsufficientEvidence(_)
            )
        ));
        assert!(backend.calls.is_empty());
    }

    #[test]
    fn verification_failure_rolls_back() {
        let planner = planner();
        let now = Instant::now();
        let (context, snapshot) = evidence(now, 12.0, 60.0);
        let mut backend = TestBackend {
            fail_verify: true,
            ..TestBackend::default()
        };
        let error = execute_guarded_thermal_energy_transaction(
            &planner,
            &context,
            &snapshot,
            now,
            ObservationEpoch::new(3),
            ResourceGeneration::new(4),
            &mut backend,
        )
        .unwrap_err();
        assert_eq!(error.stage(), ThermalEnergyTransactionStageV1::Verify);
        assert!(error.rollback_attempted());
        assert_eq!(backend.calls, ["validate", "act", "verify", "rollback"]);
        assert!(!backend.applied);
        assert!(!backend.committed);
    }

    #[test]
    fn action_time_freshness_is_rechecked_after_backend_validation() {
        let max_age = Duration::from_millis(10);
        let planner = planner_with_max_age(max_age);
        let observed_at = Instant::now();
        let (context, snapshot) = evidence(observed_at, 12.0, 60.0);
        let mut backend = TestBackend::default();

        let error = execute_after_numeric_validation_with_action_clock(
            &planner,
            TransactionInputs {
                planning_context: &context,
                observations: &snapshot,
                now: observed_at,
                observation_epoch: ObservationEpoch::new(6),
                resource_generation: ResourceGeneration::new(2),
            },
            String::new(),
            &mut backend,
            || observed_at + max_age + Duration::from_nanos(1),
        )
        .unwrap_err();

        assert_eq!(error.stage(), ThermalEnergyTransactionStageV1::Validate);
        assert!(error.reason().contains("action-time"));
        assert!(!error.rollback_attempted());
        assert_eq!(backend.calls, ["validate"]);
        assert!(!backend.applied);
        assert!(!backend.committed);
    }

    #[test]
    fn stale_direct_reference_fails_closed_before_backend() {
        let planner = planner();
        let now = Instant::now();
        let old = now - THERMAL_ENERGY_MAX_AGE - Duration::from_millis(1);
        let context = PlanningContext::new()
            .observe(ObservationSignalId::THERMAL_MARGIN, 12.0)
            .observe(ObservationSignalId::ENERGY_RATE, 60.0);
        let snapshot = ObservationSnapshot::new(
            old,
            vec![
                Observation::from_source(
                    ObservationSource::host("test:thermal"),
                    ObservationSignalId::THERMAL_MARGIN,
                    12.0,
                    old,
                ),
                Observation::from_source(
                    ObservationSource::host("test:power"),
                    ObservationSignalId::ENERGY_RATE,
                    60.0,
                    old,
                ),
            ],
        );
        let mut backend = TestBackend::default();
        let error = execute_unguarded_thermal_energy_transaction(
            &planner,
            &context,
            &snapshot,
            now,
            ObservationEpoch::new(5),
            ResourceGeneration::new(1),
            &mut backend,
        )
        .unwrap_err();
        assert_eq!(error.stage(), ThermalEnergyTransactionStageV1::Validate);
        assert!(backend.calls.is_empty());
    }
}
