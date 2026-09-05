use std::error::Error;
use std::fmt;
use std::time::Instant;

use elastic::{
    lower, model_execution_current_profile_rank_signal, ModelExecutionAdaptivePlannerV1,
    ModelExecutionCapabilitiesV1, ModelExecutionEnvelopePolicyV1, ModelExecutionEnvelopeRuleV1,
    ModelExecutionProfileEnvelopeV1, ModelExecutionProfileSetV1, ModelExecutionProfileV1,
    ModelExecutionResourceObserverV1, ModelExecutionResourceSnapshotV1,
    ModelExecutionResourceTelemetryV1, Observation, ObservationSource, Observer, ObserverSet,
    PlanOutcome, PlanningContext, TransitionPlanner,
};

#[derive(Debug)]
struct TelemetryError;

impl fmt::Display for TelemetryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("telemetry error")
    }
}

impl Error for TelemetryError {}

struct Telemetry;

impl ModelExecutionResourceTelemetryV1 for Telemetry {
    type Error = TelemetryError;

    fn source(&self) -> ObservationSource {
        ObservationSource::host("public-model-telemetry")
    }

    fn snapshot(&self) -> Result<ModelExecutionResourceSnapshotV1, Self::Error> {
        Ok(ModelExecutionResourceSnapshotV1::new("bytes", 3_000, 8_000).unwrap())
    }
}

struct CurrentProfile;

impl Observer for CurrentProfile {
    fn observe(&self) -> (PlanningContext, Vec<Observation>) {
        let signal = model_execution_current_profile_rank_signal();
        let value = 0.0;
        (
            PlanningContext::new().observe(signal.clone(), value),
            vec![Observation::from_source(
                ObservationSource::runtime("public-model-state"),
                signal,
                value,
                Instant::now(),
            )],
        )
    }
}

#[test]
fn public_observer_set_drives_adaptive_profile_selection() {
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
    let policy = ModelExecutionEnvelopePolicyV1::new(
        &profiles,
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
        ],
    )
    .unwrap();
    let planner = ModelExecutionAdaptivePlannerV1::new(policy, profiles.clone()).unwrap();
    let resource = lower(&profiles.atomic_resource_spec("model-runtime").unwrap())
        .unwrap()
        .resource("model-runtime")
        .unwrap()
        .clone();
    let current = CurrentProfile;
    let telemetry = ModelExecutionResourceObserverV1::new("bytes", Telemetry).unwrap();
    let mut observers = ObserverSet::new();
    observers.push(&current);
    observers.push(&telemetry);

    let (context, evidence) = observers.observe();
    let outcome = planner.propose_transition_with_context(&resource, &context);

    let PlanOutcome::Candidate(candidate) = outcome else {
        panic!("expected balanced atomic profile candidate")
    };
    assert_eq!(candidate.magnitude(), Some(10));
    assert_eq!(evidence.len(), 3);
    assert!(evidence.iter().all(Observation::is_valid));
}
