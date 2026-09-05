use std::error::Error;
use std::fmt;

use elastic::{
    Cadence, Fingerprint, ModelExecutionAtomicProfilePlannerV1,
    ModelExecutionCapabilitiesV1, ModelExecutionProfileBackendV1,
    ModelExecutionProfileEnvelopeV1, ModelExecutionProfileSelectionV1,
    ModelExecutionProfileSelectorV1, ModelExecutionProfileSetV1, ModelExecutionProfileV1,
    PlannerConfig, Runtime, RuntimeConfig, RuntimeMode, TransactionalModelExecution,
    VerificationResult,
};

#[derive(Debug)]
struct BackendError;

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("backend error")
    }
}

impl Error for BackendError {}

struct PublicBackend {
    provider: String,
    revision: String,
    capabilities: Fingerprint,
    profiles: Fingerprint,
    current_rank: u32,
}

impl ModelExecutionProfileBackendV1 for PublicBackend {
    type Error = BackendError;

    fn name(&self) -> &str {
        "public-test-backend"
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

#[test]
fn public_backend_runs_through_existing_transactional_runtime() {
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
    let selection = ModelExecutionProfileSelectorV1
        .select(
            &profiles,
            ModelExecutionProfileEnvelopeV1::new(2, 5_000, 5_000).unwrap(),
        )
        .unwrap();
    let ModelExecutionProfileSelectionV1::Selected(target) = selection else {
        panic!("expected target profile")
    };

    let backend = PublicBackend {
        provider: profiles.provider_id().to_owned(),
        revision: profiles.model_revision().to_owned(),
        capabilities: profiles.capability_fingerprint(),
        profiles: profiles.fingerprint(),
        current_rank: 0,
    };
    let mut actuator =
        TransactionalModelExecution::new("model-runtime", profiles.clone(), backend).unwrap();
    let observer = actuator.clone();
    let ir = actuator.ir();
    let spec = profiles.atomic_resource_spec("model-runtime").unwrap();
    let planner = ModelExecutionAtomicProfilePlannerV1::new(&target);
    let runtime = Runtime::new(RuntimeConfig {
        resource_spec: spec,
        ir_resource: ir.clone(),
        planner_config: PlannerConfig::None,
        cadence: Cadence::OneShot,
        mode: RuntimeMode::Apply,
        max_cycles: 0,
        interval_ms: 1_000,
        emit_events: true,
        dry_run: false,
    });

    let result = runtime
        .cycle(&ir, &planner, &observer, &mut actuator)
        .unwrap();

    assert!(result.commit.is_some());
    assert!(result.rollback.is_none());
    assert_eq!(actuator.current_profile_rank().unwrap(), 10);
}
