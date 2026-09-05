use std::error::Error;
use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use elastic::{
    Cadence, Fingerprint, FixedModelExecutionTransitionPolicyV1,
    ModelExecutionAtomicProfilePlannerV1, ModelExecutionCapabilitiesV1,
    ModelExecutionProfileBackendV1, ModelExecutionProfileEnvelopeV1,
    ModelExecutionProfileSelectionV1, ModelExecutionProfileSelectorV1, ModelExecutionProfileSetV1,
    ModelExecutionProfileV1, ModelExecutionTransitionModeV1, ModelExecutionTransitionPolicyV1,
    PlannerConfig, Runtime, RuntimeConfig, RuntimeMode, TransactionalModelExecution,
    TransitionGuardedModelExecutionBackendV1, VerificationResult,
};

#[derive(Debug)]
struct BackendError;

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("backend error")
    }
}

impl Error for BackendError {}

struct GuardedBackend {
    provider: String,
    revision: String,
    capabilities: Fingerprint,
    profiles: Fingerprint,
    current_rank: u32,
    apply_calls: Arc<AtomicUsize>,
}

impl ModelExecutionProfileBackendV1 for GuardedBackend {
    type Error = BackendError;

    fn name(&self) -> &str {
        "transition-guard-test-backend"
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
        self.apply_calls.fetch_add(1, Ordering::SeqCst);
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

struct FlipBeforeApplyPolicy {
    calls: Arc<AtomicUsize>,
}

impl ModelExecutionTransitionPolicyV1 for FlipBeforeApplyPolicy {
    fn transition_mode(
        &self,
        _current_profile_rank: u32,
        _target: &ModelExecutionProfileV1,
    ) -> ModelExecutionTransitionModeV1 {
        let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        if call >= 3 {
            ModelExecutionTransitionModeV1::ModelRebuildRequired
        } else {
            ModelExecutionTransitionModeV1::LiveTransactional
        }
    }
}

fn profiles_and_target() -> (ModelExecutionProfileSetV1, ModelExecutionProfileV1) {
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
    (profiles, target)
}

fn backend(profiles: &ModelExecutionProfileSetV1, apply_calls: Arc<AtomicUsize>) -> GuardedBackend {
    GuardedBackend {
        provider: profiles.provider_id().to_owned(),
        revision: profiles.model_revision().to_owned(),
        capabilities: profiles.capability_fingerprint(),
        profiles: profiles.fingerprint(),
        current_rank: 0,
        apply_calls,
    }
}

fn runtime_for(
    profiles: &ModelExecutionProfileSetV1,
    actuator: &TransactionalModelExecution<impl ModelExecutionProfileBackendV1>,
) -> Runtime {
    Runtime::new(RuntimeConfig {
        resource_spec: profiles.atomic_resource_spec("model-runtime").unwrap(),
        ir_resource: actuator.ir(),
        planner_config: PlannerConfig::None,
        cadence: Cadence::OneShot,
        mode: RuntimeMode::Apply,
        max_cycles: 0,
        interval_ms: 1_000,
        emit_events: true,
        dry_run: false,
    })
}

#[test]
fn rebuild_required_backend_fails_closed_without_physical_apply() {
    let (profiles, target) = profiles_and_target();
    let apply_calls = Arc::new(AtomicUsize::new(0));
    let guarded = TransitionGuardedModelExecutionBackendV1::new(
        backend(&profiles, Arc::clone(&apply_calls)),
        FixedModelExecutionTransitionPolicyV1::model_rebuild_required(),
    );
    let mut actuator =
        TransactionalModelExecution::new("model-runtime", profiles.clone(), guarded).unwrap();
    let observer = actuator.clone();
    let ir = actuator.ir();
    let runtime = runtime_for(&profiles, &actuator);
    let planner = ModelExecutionAtomicProfilePlannerV1::new(&target);

    let error = runtime
        .cycle(&ir, &planner, &observer, &mut actuator)
        .unwrap_err();

    assert!(error.to_string().contains("model_rebuild_required"));
    assert_eq!(apply_calls.load(Ordering::SeqCst), 0);
    assert_eq!(actuator.current_profile_rank().unwrap(), 0);
}

#[test]
fn transition_policy_is_rechecked_immediately_before_apply() {
    let (profiles, target) = profiles_and_target();
    let apply_calls = Arc::new(AtomicUsize::new(0));
    let policy_calls = Arc::new(AtomicUsize::new(0));
    let guarded = TransitionGuardedModelExecutionBackendV1::new(
        backend(&profiles, Arc::clone(&apply_calls)),
        FlipBeforeApplyPolicy {
            calls: Arc::clone(&policy_calls),
        },
    );
    let mut actuator =
        TransactionalModelExecution::new("model-runtime", profiles.clone(), guarded).unwrap();
    let observer = actuator.clone();
    let ir = actuator.ir();
    let runtime = runtime_for(&profiles, &actuator);
    let planner = ModelExecutionAtomicProfilePlannerV1::new(&target);

    let error = runtime
        .cycle(&ir, &planner, &observer, &mut actuator)
        .unwrap_err();

    assert!(error.to_string().contains("model_rebuild_required"));
    assert_eq!(policy_calls.load(Ordering::SeqCst), 3);
    assert_eq!(apply_calls.load(Ordering::SeqCst), 0);
    assert_eq!(actuator.current_profile_rank().unwrap(), 0);
}
