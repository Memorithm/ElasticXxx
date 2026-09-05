use std::error::Error;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use elastic::{
    CadenceConfig, EvidenceCommand, ExecutionModeConfig, Fingerprint, ModelExecutionCapabilitiesV1,
    ModelExecutionControllerContractsV1, ModelExecutionControllerV1, ModelExecutionCycleEvidenceV1,
    ModelExecutionEnvelopePolicyV1, ModelExecutionEnvelopeRuleV1, ModelExecutionProfileBackendV1,
    ModelExecutionProfileEnvelopeV1, ModelExecutionProfileSetV1, ModelExecutionProfileV1,
    ModelExecutionResourceSnapshotV1, ModelExecutionResourceTelemetrySampleV1,
    ModelExecutionResourceTelemetryV1, ObservationSource, PlanOutcome, VerificationResult,
};

#[derive(Clone, Copy, Debug)]
struct BackendError;

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("backend error")
    }
}

impl Error for BackendError {}

struct FakeBackend {
    provider: String,
    revision: String,
    capabilities: Fingerprint,
    profiles: Fingerprint,
    current_rank: u32,
}

impl ModelExecutionProfileBackendV1 for FakeBackend {
    type Error = BackendError;

    fn name(&self) -> &str {
        "assembled-controller-test"
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
        target: &ModelExecutionProfileV1,
    ) -> Result<VerificationResult, Self::Error> {
        if self.current_rank == target.preference_rank() {
            Ok(VerificationResult::Pass)
        } else {
            Ok(VerificationResult::Fail {
                detail: "current profile does not match target".to_owned(),
            })
        }
    }

    fn restore_profile(&mut self, previous: &ModelExecutionProfileV1) -> Result<(), Self::Error> {
        self.current_rank = previous.preference_rank();
        Ok(())
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

#[derive(Clone)]
struct MutableTelemetry {
    state: Arc<Mutex<(u64, u16)>>,
}

impl MutableTelemetry {
    fn new(free_capacity: u64, utilization_bps: u16) -> Self {
        Self {
            state: Arc::new(Mutex::new((free_capacity, utilization_bps))),
        }
    }

    fn set(&self, free_capacity: u64, utilization_bps: u16) {
        *self.state.lock().unwrap() = (free_capacity, utilization_bps);
    }
}

impl ModelExecutionResourceTelemetryV1 for MutableTelemetry {
    type Error = TelemetryError;

    fn source(&self) -> ObservationSource {
        ObservationSource::host("assembled-controller-test")
    }

    fn snapshot(&self) -> Result<ModelExecutionResourceSnapshotV1, Self::Error> {
        let (free_capacity, utilization_bps) = *self.state.lock().map_err(|_| TelemetryError)?;
        ModelExecutionResourceSnapshotV1::new("bytes", free_capacity, utilization_bps)
            .map_err(|_| TelemetryError)
    }
}

#[derive(Clone)]
struct TimestampedTelemetry {
    sample: ModelExecutionResourceTelemetrySampleV1,
}

impl ModelExecutionResourceTelemetryV1 for TimestampedTelemetry {
    type Error = TelemetryError;

    fn source(&self) -> ObservationSource {
        ObservationSource::host("timestamped-controller-test")
    }

    fn snapshot(&self) -> Result<ModelExecutionResourceSnapshotV1, Self::Error> {
        Ok(self.sample.snapshot().clone())
    }

    fn sample(&self) -> Result<ModelExecutionResourceTelemetrySampleV1, Self::Error> {
        Ok(self.sample.clone())
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
fn assembled_controller_replans_and_commits_from_live_resource_telemetry() {
    let profiles = profiles();
    let backend = FakeBackend {
        provider: profiles.provider_id().to_owned(),
        revision: profiles.model_revision().to_owned(),
        capabilities: profiles.capability_fingerprint(),
        profiles: profiles.fingerprint(),
        current_rank: 0,
    };
    let telemetry = MutableTelemetry::new(3_000, 8_000);
    let telemetry_handle = telemetry.clone();

    let mut controller = ModelExecutionControllerV1::current_state(
        "model-runtime",
        profiles.clone(),
        policy(&profiles),
        backend,
        telemetry,
        CadenceConfig::OneShot,
        ExecutionModeConfig::Apply,
    )
    .unwrap();

    let constrained = controller.cycle().unwrap();
    assert!(constrained.transaction.commit.is_some());
    assert_eq!(controller.current_profile_rank().unwrap(), 10);

    telemetry_handle.set(9_000, 6_000);
    let rich = controller.cycle().unwrap();
    assert!(rich.transaction.commit.is_some());
    assert_eq!(controller.current_profile_rank().unwrap(), 0);
}

#[test]
fn completed_model_cycle_evidence_round_trips_against_exact_contracts() {
    let profiles = profiles();
    let contracts =
        ModelExecutionControllerContractsV1::new(profiles.clone(), policy(&profiles)).unwrap();
    let backend = FakeBackend {
        provider: profiles.provider_id().to_owned(),
        revision: profiles.model_revision().to_owned(),
        capabilities: profiles.capability_fingerprint(),
        profiles: profiles.fingerprint(),
        current_rank: 0,
    };
    let telemetry = MutableTelemetry::new(3_000, 8_000);
    let mut controller = ModelExecutionControllerV1::current_state_from_contracts(
        "model-runtime",
        contracts.clone(),
        backend,
        telemetry,
        CadenceConfig::OneShot,
        ExecutionModeConfig::Apply,
    )
    .unwrap();

    let (cycle, evidence) = controller.cycle_with_evidence().unwrap();

    assert!(cycle.transaction.commit.is_some());
    assert!(evidence.committed());
    assert!(!evidence.rolled_back());
    assert_eq!(evidence.initial_profile_rank(), Some(0));
    assert_eq!(evidence.final_profile_rank(), 10);
    assert_eq!(evidence.resource_id(), "model-runtime");
    assert!(!evidence.observations().is_empty());

    let envelope = evidence.to_runtime_evidence().unwrap();
    let summary = envelope.summary().unwrap();
    assert_eq!(summary.command, EvidenceCommand::Run);
    assert_eq!(summary.resource_ids, ["model-runtime"]);
    assert_eq!(summary.commit_count, 1);
    assert_eq!(summary.rollback_count, 0);

    let json = evidence.to_pretty_json().unwrap();
    let replayed = ModelExecutionCycleEvidenceV1::from_json(json.as_bytes(), &contracts).unwrap();
    assert_eq!(replayed.initial_profile_rank(), Some(0));
    assert_eq!(replayed.final_profile_rank(), 10);
    assert!(replayed.committed());
}

#[test]
fn stale_timestamped_telemetry_never_actuates_model_profile() {
    let profiles = profiles();
    let backend = FakeBackend {
        provider: profiles.provider_id().to_owned(),
        revision: profiles.model_revision().to_owned(),
        capabilities: profiles.capability_fingerprint(),
        profiles: profiles.fingerprint(),
        current_rank: 0,
    };
    let observed_at = Instant::now() - Duration::from_secs(2);
    let valid_until = observed_at + Duration::from_secs(1);
    let snapshot = ModelExecutionResourceSnapshotV1::new("bytes", 3_000, 8_000).unwrap();
    let telemetry = TimestampedTelemetry {
        sample: ModelExecutionResourceTelemetrySampleV1::new(snapshot, observed_at)
            .with_valid_until(valid_until),
    };

    let mut controller = ModelExecutionControllerV1::current_state(
        "model-runtime",
        profiles.clone(),
        policy(&profiles),
        backend,
        telemetry,
        CadenceConfig::OneShot,
        ExecutionModeConfig::Apply,
    )
    .unwrap();

    let result = controller.cycle().unwrap();

    assert!(result.transaction.actuation.is_none());
    assert!(result.transaction.commit.is_none());
    assert_eq!(controller.current_profile_rank().unwrap(), 0);
    let plan = result.transaction.plan.as_ref().unwrap();
    assert!(matches!(
        plan.plan.outcome,
        PlanOutcome::InsufficientEvidence { .. }
    ));
    let source = ObservationSource::host("timestamped-controller-test");
    let stale = result.transaction.observations[0]
        .iter()
        .filter(|observation| observation.source() == &source)
        .collect::<Vec<_>>();
    assert_eq!(stale.len(), 2);
    assert!(stale.iter().all(|observation| observation.is_unsupported()));
}

#[test]
fn persisted_contracts_revalidate_before_physical_controller_construction() {
    let profiles = profiles();
    let contracts =
        ModelExecutionControllerContractsV1::new(profiles.clone(), policy(&profiles)).unwrap();
    let json = contracts.to_pretty_json().unwrap();
    let replayed = ModelExecutionControllerContractsV1::from_json(&json).unwrap();

    let backend = FakeBackend {
        provider: replayed.profiles().provider_id().to_owned(),
        revision: replayed.profiles().model_revision().to_owned(),
        capabilities: replayed.profiles().capability_fingerprint(),
        profiles: replayed.profiles().fingerprint(),
        current_rank: 0,
    };
    let telemetry = MutableTelemetry::new(3_000, 8_000);

    let mut controller = ModelExecutionControllerV1::current_state_from_contracts(
        "model-runtime",
        replayed,
        backend,
        telemetry,
        CadenceConfig::OneShot,
        ExecutionModeConfig::Apply,
    )
    .unwrap();

    let result = controller.cycle().unwrap();
    assert!(result.transaction.commit.is_some());
    assert_eq!(controller.current_profile_rank().unwrap(), 10);
}
