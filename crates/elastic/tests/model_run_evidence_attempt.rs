use std::cell::Cell;
use std::collections::VecDeque;
use std::error::Error;
use std::fmt;
use std::sync::Mutex;

use elastic::{
    CadenceConfig, CancellationToken, ExecutionModeConfig, Fingerprint, ForecastRunFailure,
    ModelExecutionCapabilitiesV1, ModelExecutionControllerContractsV1, ModelExecutionControllerV1,
    ModelExecutionCycleEvidenceV1, ModelExecutionEnvelopePolicyV1, ModelExecutionEnvelopeRuleV1,
    ModelExecutionProfileBackendV1, ModelExecutionProfileEnvelopeV1, ModelExecutionProfileSetV1,
    ModelExecutionProfileV1, ModelExecutionResourceSnapshotV1,
    ModelExecutionResourceTelemetryV1, ModelExecutionRunEvidenceAttemptV1,
    ModelExecutionRunEvidenceFailureV1, ObservationSource, RuntimeError, VerificationResult,
};

#[derive(Clone, Copy, Debug)]
struct BackendError;

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("backend error")
    }
}

impl Error for BackendError {}

struct FailingSecondTransitionBackend {
    provider: String,
    revision: String,
    capabilities: Fingerprint,
    profiles: Fingerprint,
    current_rank: u32,
    verify_calls: Cell<usize>,
}

impl ModelExecutionProfileBackendV1 for FailingSecondTransitionBackend {
    type Error = BackendError;

    fn name(&self) -> &str {
        "model-run-evidence-attempt-test"
    }

    fn provider_id(&self) -> &str {
        &self.provider
    }

    fn model_revision(&self) -> &str {
        &self.revision
    }

    fn capability_fingerprint(&self) -> Fingerprint {
        self.capabilities
    }

    fn profile_set_fingerprint(&self) -> Fingerprint {
        self.profiles
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
        _target: &ModelExecutionProfileV1,
    ) -> Result<VerificationResult, Self::Error> {
        let call = self.verify_calls.get();
        self.verify_calls.set(call + 1);
        if call < 2 {
            Ok(VerificationResult::Pass)
        } else {
            Ok(VerificationResult::Fail {
                detail: "injected second-transition verification failure".to_owned(),
            })
        }
    }

    fn restore_profile(&mut self, _previous: &ModelExecutionProfileV1) -> Result<(), Self::Error> {
        Err(BackendError)
    }
}

#[derive(Clone, Copy, Debug)]
struct TelemetryError;

impl fmt::Display for TelemetryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("telemetry error")
    }
}

impl Error for TelemetryError {}

struct SequenceTelemetry {
    snapshots: Mutex<VecDeque<ModelExecutionResourceSnapshotV1>>,
}

impl SequenceTelemetry {
    fn new(snapshots: Vec<ModelExecutionResourceSnapshotV1>) -> Self {
        Self {
            snapshots: Mutex::new(snapshots.into()),
        }
    }
}

impl ModelExecutionResourceTelemetryV1 for SequenceTelemetry {
    type Error = TelemetryError;

    fn source(&self) -> ObservationSource {
        ObservationSource::host("model-run-evidence-attempt-test")
    }

    fn snapshot(&self) -> Result<ModelExecutionResourceSnapshotV1, Self::Error> {
        let mut snapshots = self.snapshots.lock().map_err(|_| TelemetryError)?;
        snapshots.pop_front().ok_or(TelemetryError)
    }
}

fn profiles() -> ModelExecutionProfileSetV1 {
    let capabilities = ModelExecutionCapabilitiesV1::new(
        "reference-backend",
        "model-rev-a",
        64,
        vec![1, 2, 4],
        vec![2_500, 5_000, 10_000],
        vec![2_500, 5_000, 10_000],
    )
    .unwrap();
    ModelExecutionProfileSetV1::new(
        &capabilities,
        vec![
            ModelExecutionProfileV1::new("full", 0, 4, 10_000, 10_000).unwrap(),
            ModelExecutionProfileV1::new("balanced", 10, 2, 5_000, 5_000).unwrap(),
            ModelExecutionProfileV1::new("minimal", 20, 1, 2_500, 2_500).unwrap(),
        ],
    )
    .unwrap()
}

fn policy(profiles: &ModelExecutionProfileSetV1) -> ModelExecutionEnvelopePolicyV1 {
    ModelExecutionEnvelopePolicyV1::new(
        profiles,
        "bytes",
        vec![
            ModelExecutionEnvelopeRuleV1::new(
                "rich",
                0,
                8_000,
                7_000,
                ModelExecutionProfileEnvelopeV1::new(4, 10_000, 10_000).unwrap(),
            )
            .unwrap(),
            ModelExecutionEnvelopeRuleV1::new(
                "balanced",
                10,
                2_000,
                9_000,
                ModelExecutionProfileEnvelopeV1::new(2, 5_000, 5_000).unwrap(),
            )
            .unwrap(),
            ModelExecutionEnvelopeRuleV1::new(
                "survival",
                20,
                0,
                10_000,
                ModelExecutionProfileEnvelopeV1::new(1, 2_500, 2_500).unwrap(),
            )
            .unwrap(),
        ],
    )
    .unwrap()
}

#[test]
fn failed_model_run_retains_only_completed_cycle_evidence() {
    let profiles = profiles();
    let contracts =
        ModelExecutionControllerContractsV1::new(profiles.clone(), policy(&profiles)).unwrap();
    let backend = FailingSecondTransitionBackend {
        provider: profiles.provider_id().to_owned(),
        revision: profiles.model_revision().to_owned(),
        capabilities: profiles.capability_fingerprint(),
        profiles: profiles.fingerprint(),
        current_rank: 0,
        verify_calls: Cell::new(0),
    };
    let telemetry = SequenceTelemetry::new(vec![
        ModelExecutionResourceSnapshotV1::new("bytes", 3_000, 8_000).unwrap(),
        ModelExecutionResourceSnapshotV1::new("bytes", 9_000, 6_000).unwrap(),
    ]);
    let mut controller = ModelExecutionControllerV1::current_state_from_contracts(
        "model-runtime",
        contracts.clone(),
        backend,
        telemetry,
        CadenceConfig::Periodic {
            interval_ms: 1,
            max_cycles: 3,
        },
        ExecutionModeConfig::Apply,
    )
    .unwrap();

    let attempt = controller.run_with_evidence_attempt(&CancellationToken::new());

    let ModelExecutionRunEvidenceAttemptV1::Failed(failure) = attempt else {
        panic!("second model transition must fail and retain run evidence")
    };

    match *failure {
        ModelExecutionRunEvidenceFailureV1::Evidence { .. } => {
            panic!("completed prefix evidence should remain representable")
        }
        ModelExecutionRunEvidenceFailureV1::Runtime {
            completed_evidence,
            failure,
        } => {
            assert_eq!(completed_evidence.len(), 1);
            let artifact = &completed_evidence[0];
            assert!(artifact.committed());
            assert_eq!(artifact.initial_profile_rank(), Some(0));
            assert_eq!(artifact.final_profile_rank(), 10);

            let json = artifact.to_pretty_json().unwrap();
            let replayed =
                ModelExecutionCycleEvidenceV1::from_json(json.as_bytes(), &contracts).unwrap();
            assert_eq!(replayed.final_profile_rank(), 10);

            assert!(matches!(failure.error(), RuntimeError::Rollback(_)));
            match *failure {
                ForecastRunFailure::Setup { .. } => {
                    panic!("second-cycle rollback failure must be a cycle failure")
                }
                ForecastRunFailure::Cycle {
                    completed_cycles,
                    failed_cycle,
                    ..
                } => {
                    assert_eq!(completed_cycles.len(), 1);
                    assert!(matches!(failed_cycle.error(), RuntimeError::Rollback(_)));
                }
            }
        }
    }

    // The failed second transition applied rank 0 and could not restore rank 10.
    // The durable prefix evidence therefore must not be interpreted as the final
    // physical state of the failed run.
    assert_eq!(controller.current_profile_rank().unwrap(), 0);
}
