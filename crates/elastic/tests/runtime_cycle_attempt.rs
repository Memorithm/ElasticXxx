use elastic::{
    Actuation, CommitRecord, CycleAttempt, FirstGroundedPlanner, InvariantCheck, RollbackRecord,
    Runtime, RuntimeConfig, RuntimeError, RuntimeEventKind, RuntimeMode, TransactionalActuator,
    ValidatedPlan, VerificationResult,
};

struct PublicFailingRollback;

impl TransactionalActuator for PublicFailingRollback {
    fn name(&self) -> &str {
        "public-failing-rollback"
    }

    fn validate(&self, plan: &elastic::Plan) -> Result<Vec<InvariantCheck>, RuntimeError> {
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
        Ok(())
    }

    fn verify(&self, _actuation: &Actuation) -> Result<VerificationResult, RuntimeError> {
        Ok(VerificationResult::Fail {
            detail: "public injected verification failure".to_owned(),
        })
    }

    fn commit(&mut self, _actuation: &Actuation) -> Result<CommitRecord, RuntimeError> {
        Err(RuntimeError::commit("commit must not be reached"))
    }

    fn rollback(
        &mut self,
        _actuation: &Actuation,
        _verification: &VerificationResult,
    ) -> Result<RollbackRecord, RuntimeError> {
        Err(RuntimeError::rollback("public injected rollback failure"))
    }
}

#[test]
fn downstream_runtime_user_can_retain_catastrophic_cycle_audit_events() {
    let runtime = Runtime::new(RuntimeConfig {
        dry_run: false,
        mode: RuntimeMode::Apply,
        ..RuntimeConfig::default()
    });
    let resource = runtime.config().ir_resource.clone();
    let mut actuator = PublicFailingRollback;

    let attempt = runtime.cycle_attempt(
        &resource,
        &FirstGroundedPlanner,
        &(),
        &mut actuator,
    );

    let CycleAttempt::Failed(failure) = attempt else {
        panic!("rollback failure must be retained as failed attempt")
    };
    assert_eq!(failure.resource.fingerprint(), resource.fingerprint());
    assert!(matches!(failure.error, RuntimeError::Rollback(_)));
    assert!(failure
        .events
        .iter()
        .any(|event| event.kind == RuntimeEventKind::ActuationApplied));
    assert!(failure
        .events
        .iter()
        .any(|event| event.kind == RuntimeEventKind::VerificationPerformed));
    assert!(!failure
        .events
        .iter()
        .any(|event| event.kind == RuntimeEventKind::RollbackExecuted));
    assert!(!failure
        .events
        .iter()
        .any(|event| event.kind == RuntimeEventKind::CycleCompleted));
}
