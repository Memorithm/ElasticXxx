use std::error::Error;
use std::fmt;
use std::time::Instant;

use elastic::{
    Cadence, Fingerprint, ModelExecutionAdaptivePlannerV1, ModelExecutionCapabilitiesV1,
    ModelExecutionEnvelopePolicyV1, ModelExecutionEnvelopeRuleV1, ModelExecutionProfileBackendV1,
    ModelExecutionProfileEnvelopeV1, ModelExecutionProfileSetV1, ModelExecutionProfileV1,
    Observation, ObservationSignalId, ObservationSource, Observer, PlannerConfig, PlanningContext,
    Runtime, RuntimeConfig, RuntimeMode, TransactionalModelExecution, VerificationResult,
};

#[derive(Debug)]
struct BackendError;

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("backend error")
    }
}

impl Error for BackendError {}

struct AdaptiveBackend {
    provider: String,
    revision: String,
    capabilities: Fingerprint,
    profiles: Fingerprint,
    current_rank: u32,
}

impl ModelExecutionProfileBackendV1 for AdaptiveBackend {
    type Error = BackendError;

    fn name(&self) -> &str {
        "adaptive-public-test"
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
                detail: "wrong current profile".to_owned(),
            })
        }
    }

    fn restore_profile(&mut self, previous: &ModelExecutionProfileV1) -> Result<(), Self::Error> {
        self.current_rank = previous.preference_rank();
        Ok(())
    }
}

struct AdaptiveObserver {
    model: TransactionalModelExecution<AdaptiveBackend>,
    free_capacity: f64,
    utilization: f64,
}

impl Observer for AdaptiveObserver {
    fn observe(&self) -> (PlanningContext, Vec<Observation>) {
        let (context, mut observations) = self.model.observe();
        let now = Instant::now();
        let source = ObservationSource::host("adaptive-public-test");
        observations.push(Observation::from_source(
            source.clone(),
            ObservationSignalId::FREE_CAPACITY,
            self.free_capacity,
            now,
        ));
        observations.push(Observation::from_source(
            source,
            ObservationSignalId::UTILIZATION,
            self.utilization,
            now,
        ));
        (
            context
                .observe(ObservationSignalId::FREE_CAPACITY, self.free_capacity)
                .observe(ObservationSignalId::UTILIZATION, self.utilization),
            observations,
        )
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
fn runtime_reselects_and_commits_profiles_from_current_resource_evidence() {
    let profiles = profiles();
    let backend = AdaptiveBackend {
        provider: profiles.provider_id().to_owned(),
        revision: profiles.model_revision().to_owned(),
        capabilities: profiles.capability_fingerprint(),
        profiles: profiles.fingerprint(),
        current_rank: 0,
    };
    let mut actuator =
        TransactionalModelExecution::new("model-runtime", profiles.clone(), backend).unwrap();
    let ir = actuator.ir();
    let planner = ModelExecutionAdaptivePlannerV1::new(policy(&profiles), profiles.clone()).unwrap();
    let runtime = Runtime::new(RuntimeConfig {
        resource_spec: profiles.atomic_resource_spec("model-runtime").unwrap(),
        ir_resource: ir.clone(),
        planner_config: PlannerConfig::None,
        cadence: Cadence::OneShot,
        mode: RuntimeMode::Apply,
        max_cycles: 0,
        interval_ms: 1_000,
        emit_events: true,
        dry_run: false,
    });

    let constrained = AdaptiveObserver {
        model: actuator.clone(),
        free_capacity: 3_000.0,
        utilization: 0.80,
    };
    let first = runtime
        .cycle(&ir, &planner, &constrained, &mut actuator)
        .unwrap();
    assert!(first.commit.is_some());
    assert_eq!(actuator.current_profile_rank().unwrap(), 10);

    let rich = AdaptiveObserver {
        model: actuator.clone(),
        free_capacity: 9_000.0,
        utilization: 0.60,
    };
    let second = runtime
        .cycle(&ir, &planner, &rich, &mut actuator)
        .unwrap();
    assert!(second.commit.is_some());
    assert_eq!(actuator.current_profile_rank().unwrap(), 0);
}
