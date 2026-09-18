use std::time::Instant;

use elastic::{
    execute_guarded_thermal_energy_transaction, AdmissibleTransition,
    BooleanThermalEnergyPreplannerV1, CapabilityRequirement, DimensionId,
    GuardedThermalEnergyTransactionOutcomeV1, LogicalResourceId, Observation, ObservationEpoch,
    ObservationSignalId, ObservationSnapshot, ObservationSource, PlanningContext, ResourceClassId,
    ResourceGeneration, ResourceSpec, ThermalEnergyTransitionBackendV1, TransitionCandidate,
    TransitionMechanism, THERMAL_ENERGY_MAX_AGE,
};

#[derive(Default)]
struct PublicBackend {
    calls: Vec<&'static str>,
}

impl ThermalEnergyTransitionBackendV1 for PublicBackend {
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
        Ok(())
    }

    fn verify_transition(&mut self, _candidate: &TransitionCandidate) -> Result<(), String> {
        self.calls.push("verify");
        Ok(())
    }

    fn commit_transition(&mut self, _candidate: &TransitionCandidate) -> Result<(), String> {
        self.calls.push("commit");
        Ok(())
    }

    fn rollback_transition(
        &mut self,
        _candidate: &TransitionCandidate,
        _reason: &str,
    ) -> Result<(), String> {
        self.calls.push("rollback");
        Ok(())
    }
}

#[test]
fn facade_only_guarded_transaction_revalidates_and_commits_test_backend() {
    let spec = ResourceSpec::builder(
        ResourceClassId::CONFIGURATIONAL,
        LogicalResourceId::new("public-be14h-transaction").unwrap(),
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
    let thermal_source = ObservationSource::host("public:thermal");
    let energy_source = ObservationSource::host("public:power");
    let planner = BooleanThermalEnergyPreplannerV1::new(
        spec,
        TransitionMechanism::Reinterpret,
        DimensionId::ENERGY,
        8.0,
        75.0,
        thermal_source.clone(),
        energy_source.clone(),
        THERMAL_ENERGY_MAX_AGE,
    )
    .unwrap();
    let now = Instant::now();
    let context = PlanningContext::new()
        .observe(ObservationSignalId::THERMAL_MARGIN, 12.0)
        .observe(ObservationSignalId::ENERGY_RATE, 60.0);
    let observations = ObservationSnapshot::new(
        now,
        vec![
            Observation::from_source(
                thermal_source,
                ObservationSignalId::THERMAL_MARGIN,
                12.0,
                now,
            ),
            Observation::from_source(energy_source, ObservationSignalId::ENERGY_RATE, 60.0, now),
        ],
    );
    let mut backend = PublicBackend::default();
    let outcome = execute_guarded_thermal_energy_transaction(
        &planner,
        &context,
        &observations,
        now,
        ObservationEpoch::new(23),
        ResourceGeneration::new(9),
        &mut backend,
    )
    .unwrap();
    let GuardedThermalEnergyTransactionOutcomeV1::Committed(committed) = outcome else {
        panic!("eligible public transaction must commit through the explicit test backend");
    };
    assert_eq!(backend.calls, ["validate", "act", "verify", "commit"]);
    assert_eq!(committed.observation_epoch(), ObservationEpoch::new(23));
    assert_eq!(committed.resource_generation(), ResourceGeneration::new(9));
    assert!(!committed.decision_trace_json().is_empty());
}
