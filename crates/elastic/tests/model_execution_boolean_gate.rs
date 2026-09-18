use std::error::Error;
use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use elastic::{
    BooleanModelExecutionProfileControllerV1, CadenceConfig, DecisionTrace, ExecutionModeConfig,
    Fingerprint, ModelExecutionCapabilitiesV1, ModelExecutionControllerV1,
    ModelExecutionEnvelopePolicyV1, ModelExecutionEnvelopeRuleV1, ModelExecutionProfileBackendV1,
    ModelExecutionProfileEnvelopeV1, ModelExecutionProfileSetV1, ModelExecutionProfileV1,
    ModelExecutionResourceSnapshotV1, ModelExecutionResourceTelemetrySampleV1,
    ModelExecutionResourceTelemetryV1, ObservationSource, VerificationResult,
};

#[derive(Debug)]
struct BackendError;

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("test backend error")
    }
}

impl Error for BackendError {}

#[derive(Default)]
struct BackendCalls {
    validate: AtomicUsize,
    apply: AtomicUsize,
    verify: AtomicUsize,
    restore: AtomicUsize,
}

struct Backend {
    provider: String,
    revision: String,
    capabilities: Fingerprint,
    profiles: Fingerprint,
    current_rank: u32,
    verify_fail: bool,
    calls: Arc<BackendCalls>,
}

impl ModelExecutionProfileBackendV1 for Backend {
    type Error = BackendError;

    fn name(&self) -> &str {
        "be14c-test-backend"
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
        self.calls.validate.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn apply_profile(&mut self, target: &ModelExecutionProfileV1) -> Result<(), Self::Error> {
        self.calls.apply.fetch_add(1, Ordering::SeqCst);
        self.current_rank = target.preference_rank();
        Ok(())
    }

    fn verify_profile(
        &self,
        target: &ModelExecutionProfileV1,
    ) -> Result<VerificationResult, Self::Error> {
        self.calls.verify.fetch_add(1, Ordering::SeqCst);
        if self.verify_fail && target.preference_rank() == 0 {
            Ok(VerificationResult::Fail {
                detail: "injected target-profile verification failure".to_owned(),
            })
        } else if self.current_rank == target.preference_rank() {
            Ok(VerificationResult::Pass)
        } else {
            Ok(VerificationResult::Fail {
                detail: "wrong active profile".to_owned(),
            })
        }
    }

    fn restore_profile(&mut self, previous: &ModelExecutionProfileV1) -> Result<(), Self::Error> {
        self.calls.restore.fetch_add(1, Ordering::SeqCst);
        self.current_rank = previous.preference_rank();
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum TelemetryMode {
    Live {
        free_capacity: u64,
        utilization_bps: u16,
    },
    Failed,
    Expired {
        free_capacity: u64,
        utilization_bps: u16,
    },
}

#[derive(Debug)]
struct TelemetryError;

impl fmt::Display for TelemetryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("injected telemetry failure")
    }
}

impl Error for TelemetryError {}

struct Telemetry {
    mode: TelemetryMode,
    calls: Arc<AtomicUsize>,
}

impl ModelExecutionResourceTelemetryV1 for Telemetry {
    type Error = TelemetryError;

    fn source(&self) -> ObservationSource {
        ObservationSource::host("be14c-test-telemetry")
    }

    fn snapshot(&self) -> Result<ModelExecutionResourceSnapshotV1, Self::Error> {
        match self.mode {
            TelemetryMode::Live {
                free_capacity,
                utilization_bps,
            }
            | TelemetryMode::Expired {
                free_capacity,
                utilization_bps,
            } => ModelExecutionResourceSnapshotV1::new("bytes", free_capacity, utilization_bps)
                .map_err(|_| TelemetryError),
            TelemetryMode::Failed => Err(TelemetryError),
        }
    }

    fn sample(&self) -> Result<ModelExecutionResourceTelemetrySampleV1, Self::Error> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match self.mode {
            TelemetryMode::Failed => Err(TelemetryError),
            TelemetryMode::Live {
                free_capacity,
                utilization_bps,
            } => {
                let snapshot =
                    ModelExecutionResourceSnapshotV1::new("bytes", free_capacity, utilization_bps)
                        .map_err(|_| TelemetryError)?;
                Ok(ModelExecutionResourceTelemetrySampleV1::current(snapshot))
            }
            TelemetryMode::Expired {
                free_capacity,
                utilization_bps,
            } => {
                let snapshot =
                    ModelExecutionResourceSnapshotV1::new("bytes", free_capacity, utilization_bps)
                        .map_err(|_| TelemetryError)?;
                let now = Instant::now();
                let observed_at = now
                    .checked_sub(Duration::from_secs(2))
                    .expect("test monotonic clock supports two seconds");
                let valid_until = now
                    .checked_sub(Duration::from_secs(1))
                    .expect("test monotonic clock supports one second");
                Ok(
                    ModelExecutionResourceTelemetrySampleV1::new(snapshot, observed_at)
                        .with_valid_until(valid_until),
                )
            }
        }
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
        vec![ModelExecutionEnvelopeRuleV1::new(
            "rich-only",
            0,
            5_000,
            7_000,
            ModelExecutionProfileEnvelopeV1::new(4, 10_000, 10_000).unwrap(),
        )
        .unwrap()],
    )
    .unwrap()
}

fn backend(
    profiles: &ModelExecutionProfileSetV1,
    current_rank: u32,
    verify_fail: bool,
    calls: Arc<BackendCalls>,
) -> Backend {
    Backend {
        provider: profiles.provider_id().to_owned(),
        revision: profiles.model_revision().to_owned(),
        capabilities: profiles.capability_fingerprint(),
        profiles: profiles.fingerprint(),
        current_rank,
        verify_fail,
        calls,
    }
}

fn telemetry(mode: TelemetryMode, calls: Arc<AtomicUsize>) -> Telemetry {
    Telemetry { mode, calls }
}

#[test]
fn true_gate_reuses_one_snapshot_and_matches_unguarded_controller() {
    let profiles = profiles();
    let policy = policy(&profiles);
    let guarded_backend_calls = Arc::new(BackendCalls::default());
    let guarded_telemetry_calls = Arc::new(AtomicUsize::new(0));
    let mut guarded = BooleanModelExecutionProfileControllerV1::new(
        "model-runtime",
        profiles.clone(),
        policy.clone(),
        backend(&profiles, 10, false, guarded_backend_calls.clone()),
        telemetry(
            TelemetryMode::Live {
                free_capacity: 9_000,
                utilization_bps: 6_000,
            },
            guarded_telemetry_calls.clone(),
        ),
        CadenceConfig::OneShot,
        ExecutionModeConfig::Apply,
    )
    .unwrap();

    let report = guarded.cycle().unwrap();
    assert_eq!(report.guard.truth, "true");
    assert_eq!(report.guard.forecast_method, "current-state");
    assert_eq!(report.guard.forecast_horizon_milliseconds, 0);
    assert!(!report.guard.forecast_confidence_claimed);
    assert_eq!(report.previous_profile_rank, Some(10));
    assert_eq!(report.final_profile_rank, Some(0));
    assert_eq!(report.committed, Some(true));
    assert_eq!(report.rolled_back, Some(false));
    assert!(report.verification.is_some());
    assert!(report.model_cycle_evidence_json.is_some());
    assert_eq!(guarded_telemetry_calls.load(Ordering::SeqCst), 1);
    assert_eq!(guarded_backend_calls.apply.load(Ordering::SeqCst), 1);

    let trace =
        DecisionTrace::from_bounded_json(report.guard.decision_trace_json.as_bytes()).unwrap();
    assert!(trace.selected().is_some());

    let baseline_telemetry_calls = Arc::new(AtomicUsize::new(0));
    let mut baseline = ModelExecutionControllerV1::current_state(
        "model-runtime",
        profiles.clone(),
        policy,
        backend(&profiles, 10, false, Arc::new(BackendCalls::default())),
        telemetry(
            TelemetryMode::Live {
                free_capacity: 9_000,
                utilization_bps: 6_000,
            },
            baseline_telemetry_calls.clone(),
        ),
        CadenceConfig::OneShot,
        ExecutionModeConfig::Apply,
    )
    .unwrap();
    let (_cycle, evidence) = baseline.cycle_with_evidence().unwrap();
    assert_eq!(
        evidence.final_profile_rank(),
        report.final_profile_rank.unwrap()
    );
    assert_eq!(baseline_telemetry_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn false_gate_blocks_before_numeric_planning_and_backend_calls() {
    let profiles = profiles();
    let backend_calls = Arc::new(BackendCalls::default());
    let telemetry_calls = Arc::new(AtomicUsize::new(0));
    let mut controller = BooleanModelExecutionProfileControllerV1::new(
        "model-runtime",
        profiles.clone(),
        policy(&profiles),
        backend(&profiles, 10, false, backend_calls.clone()),
        telemetry(
            TelemetryMode::Live {
                free_capacity: 1_000,
                utilization_bps: 8_000,
            },
            telemetry_calls.clone(),
        ),
        CadenceConfig::OneShot,
        ExecutionModeConfig::Apply,
    )
    .unwrap();

    let report = controller.cycle().unwrap();
    assert_eq!(report.guard.truth, "false");
    assert_eq!(report.status, "rejected");
    assert_eq!(report.reason, "boolean-model-envelope-false");
    assert_eq!(report.previous_profile_rank, Some(10));
    assert_eq!(report.final_profile_rank, Some(10));
    assert_eq!(report.committed, Some(false));
    assert!(report.events.is_empty());
    assert!(report.model_cycle_evidence_json.is_none());
    assert_eq!(telemetry_calls.load(Ordering::SeqCst), 1);
    assert_eq!(backend_calls.validate.load(Ordering::SeqCst), 0);
    assert_eq!(backend_calls.apply.load(Ordering::SeqCst), 0);

    let trace =
        DecisionTrace::from_bounded_json(report.guard.decision_trace_json.as_bytes()).unwrap();
    assert!(trace.selected().is_none());
    assert_eq!(
        trace.stop_reason(),
        Some(elastic::DecisionStopReason::AllCandidatesRejected)
    );
}

#[test]
fn failed_and_expired_telemetry_are_unknown_and_never_actuate() {
    for mode in [
        TelemetryMode::Failed,
        TelemetryMode::Expired {
            free_capacity: 9_000,
            utilization_bps: 6_000,
        },
    ] {
        let profiles = profiles();
        let backend_calls = Arc::new(BackendCalls::default());
        let telemetry_calls = Arc::new(AtomicUsize::new(0));
        let mut controller = BooleanModelExecutionProfileControllerV1::new(
            "model-runtime",
            profiles.clone(),
            policy(&profiles),
            backend(&profiles, 10, false, backend_calls.clone()),
            telemetry(mode, telemetry_calls.clone()),
            CadenceConfig::OneShot,
            ExecutionModeConfig::Apply,
        )
        .unwrap();

        let report = controller.cycle().unwrap();
        assert_eq!(report.guard.truth, "unknown");
        assert_eq!(report.status, "rejected");
        assert_eq!(report.reason, "boolean-model-envelope-unknown");
        assert_eq!(report.committed, Some(false));
        assert!(report.events.is_empty());
        assert_eq!(telemetry_calls.load(Ordering::SeqCst), 1);
        assert_eq!(backend_calls.apply.load(Ordering::SeqCst), 0);
        let trace =
            DecisionTrace::from_bounded_json(report.guard.decision_trace_json.as_bytes()).unwrap();
        assert!(trace.selected().is_none());
        assert_eq!(
            trace.stop_reason(),
            Some(elastic::DecisionStopReason::InsufficientEvidence)
        );
    }
}

#[test]
fn verification_failure_rolls_back_and_keeps_guard_trace_explanatory() {
    let profiles = profiles();
    let backend_calls = Arc::new(BackendCalls::default());
    let telemetry_calls = Arc::new(AtomicUsize::new(0));
    let mut controller = BooleanModelExecutionProfileControllerV1::new(
        "model-runtime",
        profiles.clone(),
        policy(&profiles),
        backend(&profiles, 10, true, backend_calls.clone()),
        telemetry(
            TelemetryMode::Live {
                free_capacity: 9_000,
                utilization_bps: 6_000,
            },
            telemetry_calls.clone(),
        ),
        CadenceConfig::OneShot,
        ExecutionModeConfig::Apply,
    )
    .unwrap();

    let report = controller.cycle().unwrap();
    assert_eq!(report.guard.truth, "true");
    assert_eq!(report.status, "rolled-back");
    assert_eq!(report.committed, Some(false));
    assert_eq!(report.rolled_back, Some(true));
    assert_eq!(report.previous_profile_rank, Some(10));
    assert_eq!(report.final_profile_rank, Some(10));
    assert_eq!(controller.current_profile_rank().unwrap(), 10);
    assert_eq!(telemetry_calls.load(Ordering::SeqCst), 1);
    assert_eq!(backend_calls.apply.load(Ordering::SeqCst), 1);
    assert_eq!(backend_calls.verify.load(Ordering::SeqCst), 2);
    assert_eq!(backend_calls.restore.load(Ordering::SeqCst), 1);
    assert!(report.model_cycle_evidence_json.is_some());

    let trace =
        DecisionTrace::from_bounded_json(report.guard.decision_trace_json.as_bytes()).unwrap();
    assert!(trace.selected().is_some());
}
