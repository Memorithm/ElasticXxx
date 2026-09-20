use elastic_core::resource::{
    AdmissibleTransition, CapabilityRequirement, DimensionId, LogicalResourceId, ResourceClassId,
    ResourceGroupBuilder, ResourceGroupId, ResourceSpec,
};
use elastic_core::TransitionMechanism;
use elastic_eir::{
    EirDocumentBuilder, EirGroupedDocument, FirstGroundedPlanner, PlanningContext,
    TransitionPlanner,
};
use elastic_runtime::{
    execute_composite_transaction, prepare_composite_plan, Actuation, CancellationToken,
    CommitRecord, CompositeFailureDisposition, CompositePlanEnvelope, CompositePreActState,
    CompositePrepareBackend, CompositeTransactionBackend, CompositeTransactionStage,
    InvariantCheck, Plan, RollbackRecord, Runtime, RuntimeConfig, RuntimeError, RuntimeMode,
    TransactionalActuator, ValidatedPlan, VerificationResult,
};

#[derive(Clone, Copy, Debug)]
enum FailureMode {
    None,
    Actuate,
    Verify,
    Commit,
}

struct DifferentialBackend {
    resource: String,
    name: String,
    instance: String,
    visible: u64,
    committed: bool,
    checkpoint_active: bool,
    mode: FailureMode,
}

impl DifferentialBackend {
    fn new(resource: &str, mode: FailureMode) -> Self {
        Self {
            resource: resource.to_owned(),
            name: format!("differential-{resource}"),
            instance: format!("differential-instance-{resource}"),
            visible: 0,
            committed: false,
            checkpoint_active: false,
            mode,
        }
    }
}

impl TransactionalActuator for DifferentialBackend {
    fn name(&self) -> &str {
        &self.name
    }

    fn validate(&self, _plan: &Plan) -> Result<Vec<InvariantCheck>, RuntimeError> {
        Ok(Vec::new())
    }

    fn prepare(&mut self, plan: &ValidatedPlan) -> Result<Actuation, RuntimeError> {
        let target = plan
            .plan
            .candidate()
            .and_then(|candidate| candidate.magnitude());
        Ok(Actuation::new(plan.clone(), target, self.name.clone()))
    }

    fn actuate(&mut self, _actuation: &Actuation) -> Result<(), RuntimeError> {
        // Deliberately mutate first: an error must be treated as possibly partial
        // by both the historical and composite transaction paths.
        self.visible = 1;
        if matches!(self.mode, FailureMode::Actuate) {
            Err(RuntimeError::actuation("differential actuation failure"))
        } else {
            Ok(())
        }
    }

    fn verify(&self, _actuation: &Actuation) -> Result<VerificationResult, RuntimeError> {
        if matches!(self.mode, FailureMode::Verify) {
            Ok(VerificationResult::Fail {
                detail: "differential verification failure".into(),
            })
        } else if self.visible == 1 {
            Ok(VerificationResult::Pass)
        } else {
            Ok(VerificationResult::Fail {
                detail: "unexpected visible state".into(),
            })
        }
    }

    fn commit(&mut self, _actuation: &Actuation) -> Result<CommitRecord, RuntimeError> {
        if matches!(self.mode, FailureMode::Commit) {
            Err(RuntimeError::commit("differential commit failure"))
        } else {
            self.committed = true;
            Ok(CommitRecord::new(
                self.resource.clone(),
                "differential commit",
            ))
        }
    }

    fn rollback(
        &mut self,
        _actuation: &Actuation,
        _verification: &VerificationResult,
    ) -> Result<RollbackRecord, RuntimeError> {
        self.visible = 0;
        self.committed = false;
        Ok(RollbackRecord::new(
            self.resource.clone(),
            "historical exact restore",
            true,
        ))
    }
}

impl CompositePrepareBackend for DifferentialBackend {
    fn resource_id(&self) -> &str {
        &self.resource
    }

    fn backend_instance_id(&self) -> &str {
        &self.instance
    }

    fn capture_pre_act_state(
        &mut self,
        _plan: &ValidatedPlan,
    ) -> Result<CompositePreActState, RuntimeError> {
        self.checkpoint_active = true;
        Ok(CompositePreActState::new(
            self.resource.clone(),
            self.name.clone(),
            self.instance.clone(),
            self.visible,
            self.visible ^ 0x91,
        ))
    }

    fn abort_prepare(
        &mut self,
        _actuation: &Actuation,
        _checkpoint: &CompositePreActState,
        _reason: &str,
    ) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn release_pre_act_state(
        &mut self,
        _checkpoint: &CompositePreActState,
    ) -> Result<(), RuntimeError> {
        self.checkpoint_active = false;
        Ok(())
    }
}

impl CompositeTransactionBackend for DifferentialBackend {
    fn restore_pre_act_state(
        &mut self,
        _actuation: &Actuation,
        checkpoint: &CompositePreActState,
        _reason: &str,
    ) -> Result<RollbackRecord, RuntimeError> {
        self.visible = checkpoint.generation();
        self.committed = false;
        Ok(RollbackRecord::new(
            self.resource.clone(),
            "composite exact restore",
            true,
        ))
    }
}

fn resource_spec() -> ResourceSpec {
    ResourceSpec::builder(
        ResourceClassId::CONFIGURATIONAL,
        LogicalResourceId::new("differential").unwrap(),
    )
    .allow(DimensionId::CAPACITY)
    .admit(AdmissibleTransition::new(
        TransitionMechanism::Reinterpret,
        DimensionId::CAPACITY,
    ))
    .require_capability(CapabilityRequirement::new(
        TransitionMechanism::Reinterpret,
        DimensionId::CAPACITY,
    ))
    .build()
    .unwrap()
}

fn run_single(mode: FailureMode) -> (DifferentialBackend, bool, bool) {
    let spec = resource_spec();
    let document = elastic_eir::lower(&spec).unwrap();
    let resource = document.resource("differential").unwrap().clone();
    let runtime = Runtime::new(RuntimeConfig {
        resource_spec: spec,
        ir_resource: resource.clone(),
        mode: RuntimeMode::Apply,
        dry_run: false,
        max_cycles: 1,
        ..RuntimeConfig::default()
    });
    let mut backend = DifferentialBackend::new("differential", mode);
    let result = runtime
        .cycle(&resource, &FirstGroundedPlanner, &(), &mut backend)
        .unwrap();
    (backend, result.commit.is_some(), result.rollback.is_some())
}

fn run_composite(
    mode: FailureMode,
) -> (
    DifferentialBackend,
    Result<elastic_runtime::CompositeCommitReport, elastic_runtime::CompositeTransactionFailure>,
) {
    let spec = resource_spec();
    let mut builder = EirDocumentBuilder::new();
    builder.push(&spec).unwrap();
    let document = builder.finish().unwrap();
    let group = ResourceGroupBuilder::new(ResourceGroupId::new("single").unwrap())
        .member(LogicalResourceId::new("differential").unwrap())
        .build()
        .unwrap();
    let grouped = EirGroupedDocument::new(document, &[group]).unwrap();
    let resource = grouped.group_resource("single", "differential").unwrap();
    let plan = Plan::new(
        resource.clone(),
        PlanningContext::new(),
        FirstGroundedPlanner.propose_transition(resource),
        "differential composite plan".into(),
    );
    let envelope = CompositePlanEnvelope::new(&grouped, "single", vec![plan]).unwrap();
    let mut backend = DifferentialBackend::new("differential", mode);
    let prepared = {
        let mut prepare_backends: [&mut dyn CompositePrepareBackend; 1] = [&mut backend];
        prepare_composite_plan(&envelope, &mut prepare_backends).unwrap()
    };
    let result = {
        let mut tx_backends: [&mut dyn CompositeTransactionBackend; 1] = [&mut backend];
        execute_composite_transaction(prepared, &mut tx_backends, &CancellationToken::new())
    };
    (backend, result)
}

#[test]
fn single_resource_success_matches_historical_transaction_final_state() {
    let (single, single_committed, single_rolled_back) = run_single(FailureMode::None);
    let (composite, result) = run_composite(FailureMode::None);
    let report = result.unwrap();

    assert!(single_committed && !single_rolled_back);
    assert_eq!(report.commits().len(), 1);
    assert_eq!(single.visible, composite.visible);
    assert_eq!(single.committed, composite.committed);
    assert_eq!(single.visible, 1);
}

#[test]
fn single_resource_actuation_failure_matches_historical_rollback_state() {
    let (single, single_committed, single_rolled_back) = run_single(FailureMode::Actuate);
    let (composite, result) = run_composite(FailureMode::Actuate);
    let failure = result.unwrap_err();

    assert!(!single_committed && single_rolled_back);
    assert_eq!(failure.stage(), CompositeTransactionStage::Actuate);
    assert_eq!(
        failure.disposition(),
        CompositeFailureDisposition::RolledBack
    );
    assert_eq!(single.visible, composite.visible);
    assert_eq!(single.committed, composite.committed);
    assert_eq!(single.visible, 0);
}

#[test]
fn single_resource_verification_failure_matches_historical_rollback_state() {
    let (single, single_committed, single_rolled_back) = run_single(FailureMode::Verify);
    let (composite, result) = run_composite(FailureMode::Verify);
    let failure = result.unwrap_err();

    assert!(!single_committed && single_rolled_back);
    assert_eq!(failure.stage(), CompositeTransactionStage::Verify);
    assert_eq!(
        failure.disposition(),
        CompositeFailureDisposition::RolledBack
    );
    assert_eq!(single.visible, composite.visible);
    assert_eq!(single.committed, composite.committed);
    assert_eq!(single.visible, 0);
}

#[test]
fn single_resource_commit_failure_matches_historical_rollback_state() {
    let (single, single_committed, single_rolled_back) = run_single(FailureMode::Commit);
    let (composite, result) = run_composite(FailureMode::Commit);
    let failure = result.unwrap_err();

    assert!(!single_committed && single_rolled_back);
    assert_eq!(failure.stage(), CompositeTransactionStage::Commit);
    assert_eq!(
        failure.disposition(),
        CompositeFailureDisposition::RolledBack
    );
    assert_eq!(single.visible, composite.visible);
    assert_eq!(single.committed, composite.committed);
    assert_eq!(single.visible, 0);
}
