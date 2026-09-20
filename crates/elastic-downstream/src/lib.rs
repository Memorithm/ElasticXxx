//! Compile-time guard for the advertised single-dependency contract.
//!
//! This crate deliberately depends **only** on [`mod@elastic`]. It exercises both
//! declaration and operational runtime types so workspace CI catches accidental
//! leaks of implementation-crate dependencies into downstream code.

#![forbid(unsafe_code)]

use elastic::prelude::*;
use elastic::{ObservationEpoch, ResourceGeneration};

#[derive(ElasticResource)]
#[elastic(
    class(representational),
    id("downstream-kv"),
    allow(representation),
    preserve(contents),
    optimize(latency),
    admit(reencode @ representation),
    capability(reencode @ representation)
)]
pub struct DownstreamKv;

elastic! {
    pub resource downstream_elastic_language {
        class(configurational);
        id("downstream-elastic-language");
        allow(concurrency, energy);
        preserve(identity);
        optimize(latency);
        observe(utilization, thermal_margin, energy_rate);
        admit(reinterpret @ concurrency);
        capability(reinterpret @ concurrency);
    }
}

elastic! {
    pub document downstream_language_document {
        resource worker_pool {
            class(shared);
            id("downstream-worker-pool");
            allow(parallelism);
            optimize(latency);
            admit(reinterpret @ parallelism);
            capability(reinterpret @ parallelism);
        }
        resource cache {
            class(representational);
            id("downstream-cache");
            allow(representation);
            preserve(contents);
            admit(reencode @ representation);
            capability(reencode @ representation);
        }
        policy worker_policy {
            id("downstream.worker-policy");
            version(1, 0, 0);
            target(worker_pool);
            predicate(capacity_ok, "elastic.downstream", "capacity-ok");
            predicate(burst_mode, "elastic.downstream", "burst-mode");
            guard transition(reinterpret @ parallelism) when(capacity_ok && !burst_mode);
            constraint at_most(1, capacity_ok, burst_mode);
            constraint budget {
                unit("workers");
                quantum(1);
                maximum(8);
                term(burst_mode, 4);
            }
            objective latency minimize unit("microseconds") quantum(1);
            hint("search.mode", "balanced");
        }
    }
}

/// Compile-time proof that the embedded `elastic!` language is available from
/// the single public facade dependency and lowers to ordinary `ResourceSpec`.
pub fn public_elastic_language_surface_smoke() {
    let spec = downstream_elastic_language::resource_spec().unwrap();
    assert_eq!(spec.resource_id().as_str(), "downstream-elastic-language");
    assert_eq!(spec.class(), &ResourceClassId::CONFIGURATIONAL);
    assert!(spec.admits(TransitionMechanism::Reinterpret, &DimensionId::CONCURRENCY));
    let eir = lower(&spec).unwrap();
    assert!(eir
        .resource("downstream-elastic-language")
        .unwrap()
        .transitions()[0]
        .capability_grounded());
}

/// Proof that a downstream crate can build and execute a real configured,
/// forecast-aware controller while depending only on `elastic`.
pub fn public_surface_smoke() {
    let config = OperatorConfig {
        version: OPERATOR_CONFIG_VERSION,
        resources: vec![ResourceConfig::Ram {
            id: "downstream-ram".into(),
            host_total: 4096,
            min: 512,
            max: 4096,
            initial: 1024,
            max_step: Some(2048),
        }],
        controllers: vec![ControllerConfig {
            resource: "downstream-ram".into(),
            planner: PlannerSelection::Headroom {
                headroom_fraction: 0.5,
                deadband_fraction: 0.0,
            },
            forecaster: ForecasterSelection::Ewma {
                alpha: 0.5,
                horizon_ms: 1_000,
            },
            cadence: CadenceConfig::OneShot,
            mode: ExecutionModeConfig::Apply,
            guard_config: None,
        }],
    };
    let mut controller = config
        .build_controller("downstream-ram")
        .expect("valid public operator config should materialize");
    let result = controller
        .cycle()
        .expect("facade-only configured controller cycle should succeed");

    assert!(result.forecast.is_some());
    assert!(result.transaction.commit.is_some());
    assert_eq!(
        controller.actuator().state().unwrap(),
        ConfiguredResourceState::Ram {
            committed_bytes: 2048
        }
    );
}

/// Compile-time proof that a multi-resource `elastic! document` lowers through
/// the public EIR document surface with only the `elastic` dependency.
pub fn public_elastic_document_surface_smoke() {
    let document = downstream_language_document::document().unwrap();
    assert_eq!(document.resources().len(), 2);
    assert!(document.resource("downstream-worker-pool").is_some());
    assert!(document.resource("downstream-cache").is_some());
    assert_eq!(MAX_EIR_DOCUMENT_RESOURCES, 256);
}

/// Compile-time proof that ELANG3 resource groups, dependencies, shared budgets,
/// cross-resource invariants and grouped EIR are reachable through only the
/// public `elastic` dependency.
pub fn public_grouped_document_surface_smoke() {
    let document = downstream_language_document::document().unwrap();
    let worker = LogicalResourceId::new("downstream-worker-pool").unwrap();
    let cache = LogicalResourceId::new("downstream-cache").unwrap();
    let budget = SharedBudget::new(
        SharedBudgetId::new("memory").unwrap(),
        vec![
            SharedBudgetTerm::new(
                worker.clone(),
                PredicateKey::new("elastic.downstream", "workers-expanded").unwrap(),
                4,
            )
            .unwrap(),
            SharedBudgetTerm::new(
                cache.clone(),
                PredicateKey::new("elastic.downstream", "cache-expanded").unwrap(),
                6,
            )
            .unwrap(),
        ],
        8,
        PseudoBooleanScale::new("units", 1).unwrap(),
    )
    .unwrap();
    let invariant = CrossResourceInvariant::new(
        ContractId::new("downstream-coherence").unwrap(),
        worker.clone(),
        vec![worker.clone(), cache.clone()],
    )
    .unwrap();
    let group = ResourceGroupBuilder::new(ResourceGroupId::new("downstream-stack").unwrap())
        .members([worker.clone(), cache.clone()])
        .dependency(ResourceDependency::new(cache, worker))
        .shared_budget(budget)
        .cross_invariant(invariant)
        .build()
        .unwrap();
    let grouped = EirGroupedDocument::new(document, &[group]).unwrap();

    assert_eq!(grouped.groups().len(), 1);
    assert_eq!(grouped.groups()[0].shared_budgets().len(), 1);
    assert_eq!(grouped.groups()[0].cross_invariants().len(), 1);
    assert!(grouped
        .group_resource("downstream-stack", "downstream-cache")
        .is_some());
}

/// Compile-time and semantic proof that ELANG7 RAM/storage byte budgets lower
/// through only the public `elastic` facade and remain semantically distinct.
pub fn public_capacity_budget_surface_smoke() {
    let document = downstream_language_document::document().unwrap();
    let worker = LogicalResourceId::new("downstream-worker-pool").unwrap();
    let cache = LogicalResourceId::new("downstream-cache").unwrap();
    let ram = CapacityBudgetContract::ram(
        SharedBudgetId::new("ram").unwrap(),
        vec![
            CapacityBudgetTerm::new(
                worker.clone(),
                PredicateKey::new("elastic.downstream", "workers-expanded").unwrap(),
                4 * 1024,
            )
            .unwrap(),
            CapacityBudgetTerm::new(
                cache.clone(),
                PredicateKey::new("elastic.downstream", "cache-expanded").unwrap(),
                6 * 1024,
            )
            .unwrap(),
        ],
        8 * 1024,
    )
    .unwrap();
    let storage = CapacityBudgetContract::storage(
        SharedBudgetId::new("storage").unwrap(),
        vec![CapacityBudgetTerm::new(
            cache.clone(),
            PredicateKey::new("elastic.downstream", "cache-persisted").unwrap(),
            32 * 1024,
        )
        .unwrap()],
        64 * 1024,
    )
    .unwrap();
    let group = ResourceGroupBuilder::new(ResourceGroupId::new("edge-budget").unwrap())
        .members([worker, cache])
        .capacity_budget(ram)
        .capacity_budget(storage)
        .build()
        .unwrap();
    let grouped = EirGroupedDocument::new(document, &[group]).unwrap();
    let budgets = grouped.group("edge-budget").unwrap().shared_budgets();
    assert_eq!(budgets.len(), 2);
    assert_eq!(
        budgets
            .iter()
            .find(|budget| budget.id() == "ram")
            .unwrap()
            .constraint()
            .scale()
            .unit(),
        RAM_CAPACITY_BUDGET_UNIT
    );
    assert_eq!(
        budgets
            .iter()
            .find(|budget| budget.id() == "storage")
            .unwrap()
            .constraint()
            .scale()
            .unit(),
        STORAGE_CAPACITY_BUDGET_UNIT
    );
}

/// Compile-time and semantic proof that ELANG4 composite-plan ordering is
/// usable through only the public `elastic` dependency.
pub fn public_composite_plan_surface_smoke() {
    let document = downstream_language_document::document().unwrap();
    let worker_id = LogicalResourceId::new("downstream-worker-pool").unwrap();
    let cache_id = LogicalResourceId::new("downstream-cache").unwrap();
    let group = ResourceGroupBuilder::new(ResourceGroupId::new("runtime").unwrap())
        .members([worker_id.clone(), cache_id.clone()])
        .dependency(ResourceDependency::new(cache_id, worker_id))
        .build()
        .unwrap();
    let grouped = EirGroupedDocument::new(document, &[group]).unwrap();
    let worker = grouped
        .group_resource("runtime", "downstream-worker-pool")
        .unwrap();
    let cache = grouped
        .group_resource("runtime", "downstream-cache")
        .unwrap();
    let worker_plan = Plan::new(
        worker.clone(),
        PlanningContext::new(),
        FirstGroundedPlanner.propose_transition(worker),
        "downstream worker".into(),
    );
    let cache_plan = Plan::new(
        cache.clone(),
        PlanningContext::new(),
        FirstGroundedPlanner.propose_transition(cache),
        "downstream cache".into(),
    );
    let envelope =
        CompositePlanEnvelope::new(&grouped, "runtime", vec![cache_plan, worker_plan]).unwrap();
    assert_eq!(
        envelope.execution_order().collect::<Vec<_>>(),
        vec!["downstream-worker-pool", "downstream-cache"]
    );
}

struct DownstreamCompositePrepareBackend {
    resource: String,
    name: String,
    backend_instance_id: String,
    checkpoint_active: bool,
    prepared: bool,
    fail_release_once: bool,
}

impl DownstreamCompositePrepareBackend {
    fn new(resource: &str) -> Self {
        Self {
            resource: resource.to_owned(),
            name: format!("downstream-{resource}"),
            backend_instance_id: format!("downstream-instance-{resource}"),
            checkpoint_active: false,
            prepared: false,
            fail_release_once: false,
        }
    }
}

impl TransactionalActuator for DownstreamCompositePrepareBackend {
    fn name(&self) -> &str {
        &self.name
    }

    fn validate(&self, plan: &Plan) -> Result<Vec<InvariantCheck>, RuntimeError> {
        let Some(candidate) = plan.candidate() else {
            return Err(RuntimeError::validation(
                "composite downstream plan has no candidate",
            ));
        };
        Ok(plan
            .resource
            .invariants()
            .iter()
            .filter(|invariant| {
                invariant
                    .scope()
                    .is_none_or(|scope| scope == candidate.dimension())
            })
            .cloned()
            .map(|invariant| {
                InvariantCheck::new(invariant, true, Some("downstream test invariant".into()))
            })
            .collect())
    }

    fn prepare(&mut self, plan: &ValidatedPlan) -> Result<Actuation, RuntimeError> {
        if !self.checkpoint_active {
            return Err(RuntimeError::actuation(
                "prepare requires captured checkpoint",
            ));
        }
        self.prepared = true;
        Ok(Actuation::new(plan.clone(), Some(1), self.name.clone()))
    }

    fn actuate(&mut self, _actuation: &Actuation) -> Result<(), RuntimeError> {
        panic!("ELANG4b downstream smoke must not actuate")
    }

    fn verify(&self, _actuation: &Actuation) -> Result<VerificationResult, RuntimeError> {
        panic!("ELANG4b downstream smoke must not verify")
    }

    fn commit(&mut self, _actuation: &Actuation) -> Result<CommitRecord, RuntimeError> {
        panic!("ELANG4b downstream smoke must not commit")
    }

    fn rollback(
        &mut self,
        _actuation: &Actuation,
        _verification: &VerificationResult,
    ) -> Result<RollbackRecord, RuntimeError> {
        panic!("ELANG4b downstream smoke must not execute physical rollback")
    }
}

impl CompositePrepareBackend for DownstreamCompositePrepareBackend {
    fn resource_id(&self) -> &str {
        &self.resource
    }

    fn backend_instance_id(&self) -> &str {
        &self.backend_instance_id
    }

    fn capture_pre_act_state(
        &mut self,
        _plan: &ValidatedPlan,
    ) -> Result<CompositePreActState, RuntimeError> {
        self.checkpoint_active = true;
        Ok(CompositePreActState::new(
            self.resource.clone(),
            self.name.clone(),
            self.backend_instance_id.clone(),
            1,
            self.resource.len() as u64,
        ))
    }

    fn abort_prepare(
        &mut self,
        _actuation: &Actuation,
        _checkpoint: &CompositePreActState,
        _reason: &str,
    ) -> Result<(), RuntimeError> {
        self.prepared = false;
        Ok(())
    }

    fn release_pre_act_state(
        &mut self,
        _checkpoint: &CompositePreActState,
    ) -> Result<(), RuntimeError> {
        if self.fail_release_once {
            self.fail_release_once = false;
            return Err(RuntimeError::rollback(
                "downstream one-shot release failure",
            ));
        }
        self.checkpoint_active = false;
        Ok(())
    }
}

/// Semantic proof that ELANG4b validates, checkpoints and prepares all
/// composite resources through only the public facade, then aborts cleanly
/// without invoking any physical-actuation method.
pub fn public_composite_prepare_surface_smoke() {
    let document = downstream_language_document::document().unwrap();
    let worker_id = LogicalResourceId::new("downstream-worker-pool").unwrap();
    let cache_id = LogicalResourceId::new("downstream-cache").unwrap();
    let group = ResourceGroupBuilder::new(ResourceGroupId::new("runtime-prepare").unwrap())
        .members([worker_id.clone(), cache_id.clone()])
        .dependency(ResourceDependency::new(cache_id, worker_id))
        .build()
        .unwrap();
    let grouped = EirGroupedDocument::new(document, &[group]).unwrap();
    let plans = ["downstream-cache", "downstream-worker-pool"]
        .into_iter()
        .map(|resource| {
            let node = grouped.group_resource("runtime-prepare", resource).unwrap();
            Plan::new(
                node.clone(),
                PlanningContext::new(),
                FirstGroundedPlanner.propose_transition(node),
                format!("downstream prepare {resource}"),
            )
        })
        .collect();
    let envelope = CompositePlanEnvelope::new(&grouped, "runtime-prepare", plans).unwrap();
    let mut worker = DownstreamCompositePrepareBackend::new("downstream-worker-pool");
    let mut cache = DownstreamCompositePrepareBackend::new("downstream-cache");
    {
        let mut backends: [&mut dyn CompositePrepareBackend; 2] = [&mut cache, &mut worker];
        let prepared = prepare_composite_plan(&envelope, &mut backends).unwrap();
        assert_eq!(prepared.schema_version(), COMPOSITE_PREPARE_SCHEMA_V1);
        assert_eq!(
            prepared
                .subplans()
                .iter()
                .map(CompositePreparedSubplan::resource_id)
                .collect::<Vec<_>>(),
            vec!["downstream-worker-pool", "downstream-cache"]
        );
        abort_composite_prepare(prepared, &mut backends, "downstream smoke complete").unwrap();
    }
    assert!(!worker.prepared && !worker.checkpoint_active);
    assert!(!cache.prepared && !cache.checkpoint_active);
}

/// Semantic proof that ELANG4b cleanup recovery is nameable, retainable and
/// retryable through only the public `elastic` facade.
pub fn public_composite_prepare_recovery_surface_smoke() {
    let document = downstream_language_document::document().unwrap();
    let worker_id = LogicalResourceId::new("downstream-worker-pool").unwrap();
    let cache_id = LogicalResourceId::new("downstream-cache").unwrap();
    let group = ResourceGroupBuilder::new(ResourceGroupId::new("runtime-recovery").unwrap())
        .members([worker_id.clone(), cache_id.clone()])
        .dependency(ResourceDependency::new(cache_id, worker_id))
        .build()
        .unwrap();
    let grouped = EirGroupedDocument::new(document, &[group]).unwrap();
    let plans = ["downstream-cache", "downstream-worker-pool"]
        .into_iter()
        .map(|resource| {
            let node = grouped
                .group_resource("runtime-recovery", resource)
                .unwrap();
            Plan::new(
                node.clone(),
                PlanningContext::new(),
                FirstGroundedPlanner.propose_transition(node),
                format!("downstream recovery {resource}"),
            )
        })
        .collect();
    let envelope = CompositePlanEnvelope::new(&grouped, "runtime-recovery", plans).unwrap();
    let mut worker = DownstreamCompositePrepareBackend::new("downstream-worker-pool");
    let mut cache = DownstreamCompositePrepareBackend::new("downstream-cache");
    cache.fail_release_once = true;

    let prepared = {
        let mut backends: [&mut dyn CompositePrepareBackend; 2] = [&mut cache, &mut worker];
        prepare_composite_plan(&envelope, &mut backends).unwrap()
    };
    let failure = {
        let mut backends: [&mut dyn CompositePrepareBackend; 2] = [&mut cache, &mut worker];
        abort_composite_prepare(prepared, &mut backends, "downstream forced recovery").unwrap_err()
    };
    let recovery: CompositePrepareRecoveryEnvelope = failure
        .into_recovery()
        .expect("one-shot release failure must return recovery state");
    let entries: &[CompositePrepareRecoveryEntry] = recovery.entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].resource_id(), "downstream-cache");
    assert!(entries[0].actuation().is_none());

    let mut retry_backends: [&mut dyn CompositePrepareBackend; 1] = [&mut cache];
    retry_composite_prepare_cleanup(recovery, &mut retry_backends, "downstream retry").unwrap();
    assert!(!cache.prepared && !cache.checkpoint_active);
    assert!(!worker.prepared && !worker.checkpoint_active);
}

struct DownstreamCompositeTransactionBackend {
    resource: String,
    name: String,
    instance: String,
    visible: u64,
    checkpoint_active: bool,
    prepared: bool,
    committed: bool,
}

impl DownstreamCompositeTransactionBackend {
    fn new(resource: &str) -> Self {
        Self {
            resource: resource.to_owned(),
            name: format!("downstream-tx-{resource}"),
            instance: format!("downstream-tx-instance-{resource}"),
            visible: 0,
            checkpoint_active: false,
            prepared: false,
            committed: false,
        }
    }
}

impl TransactionalActuator for DownstreamCompositeTransactionBackend {
    fn name(&self) -> &str {
        &self.name
    }

    fn validate(&self, plan: &Plan) -> Result<Vec<InvariantCheck>, RuntimeError> {
        let Some(candidate) = plan.candidate() else {
            return Err(RuntimeError::validation(
                "composite transaction plan has no candidate",
            ));
        };
        Ok(plan
            .resource
            .invariants()
            .iter()
            .filter(|invariant| {
                invariant
                    .scope()
                    .is_none_or(|scope| scope == candidate.dimension())
            })
            .cloned()
            .map(|invariant| {
                InvariantCheck::new(invariant, true, Some("downstream composite tx".into()))
            })
            .collect())
    }

    fn prepare(&mut self, plan: &ValidatedPlan) -> Result<Actuation, RuntimeError> {
        if !self.checkpoint_active {
            return Err(RuntimeError::actuation(
                "composite tx prepare requires checkpoint",
            ));
        }
        self.prepared = true;
        Ok(Actuation::new(plan.clone(), Some(1), self.name.clone()))
    }

    fn actuate(&mut self, actuation: &Actuation) -> Result<(), RuntimeError> {
        self.visible = actuation.target.unwrap_or(1);
        Ok(())
    }

    fn verify(&self, actuation: &Actuation) -> Result<VerificationResult, RuntimeError> {
        if self.visible == actuation.target.unwrap_or(1) {
            Ok(VerificationResult::Pass)
        } else {
            Ok(VerificationResult::Fail {
                detail: "downstream composite visible state mismatch".into(),
            })
        }
    }

    fn commit(&mut self, _actuation: &Actuation) -> Result<CommitRecord, RuntimeError> {
        self.prepared = false;
        self.committed = true;
        Ok(CommitRecord::new(
            self.resource.clone(),
            "downstream composite commit",
        ))
    }

    fn rollback(
        &mut self,
        _actuation: &Actuation,
        _verification: &VerificationResult,
    ) -> Result<RollbackRecord, RuntimeError> {
        Err(RuntimeError::rollback(
            "legacy rollback is not used by ELANG4c composite transaction",
        ))
    }
}

impl CompositePrepareBackend for DownstreamCompositeTransactionBackend {
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
            self.visible ^ 0x44,
        ))
    }

    fn abort_prepare(
        &mut self,
        _actuation: &Actuation,
        _checkpoint: &CompositePreActState,
        _reason: &str,
    ) -> Result<(), RuntimeError> {
        self.prepared = false;
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

impl CompositeTransactionBackend for DownstreamCompositeTransactionBackend {
    fn restore_pre_act_state(
        &mut self,
        _actuation: &Actuation,
        checkpoint: &CompositePreActState,
        _reason: &str,
    ) -> Result<RollbackRecord, RuntimeError> {
        self.visible = checkpoint.generation();
        self.prepared = false;
        self.committed = false;
        Ok(RollbackRecord::new(
            self.resource.clone(),
            "downstream exact checkpoint restore",
            true,
        ))
    }
}

/// Semantic proof that ELANG4c composite ACT/VERIFY/COMMIT is usable through
/// only the public `elastic` facade.
pub fn public_composite_transaction_surface_smoke() {
    let document = downstream_language_document::document().unwrap();
    let worker_id = LogicalResourceId::new("downstream-worker-pool").unwrap();
    let cache_id = LogicalResourceId::new("downstream-cache").unwrap();
    let group = ResourceGroupBuilder::new(ResourceGroupId::new("runtime-transaction").unwrap())
        .members([worker_id.clone(), cache_id.clone()])
        .dependency(ResourceDependency::new(cache_id, worker_id))
        .build()
        .unwrap();
    let grouped = EirGroupedDocument::new(document, &[group]).unwrap();
    let plans = ["downstream-cache", "downstream-worker-pool"]
        .into_iter()
        .map(|resource| {
            let node = grouped
                .group_resource("runtime-transaction", resource)
                .unwrap();
            Plan::new(
                node.clone(),
                PlanningContext::new(),
                FirstGroundedPlanner.propose_transition(node),
                format!("downstream transaction {resource}"),
            )
        })
        .collect();
    let envelope = CompositePlanEnvelope::new(&grouped, "runtime-transaction", plans).unwrap();
    let mut worker = DownstreamCompositeTransactionBackend::new("downstream-worker-pool");
    let mut cache = DownstreamCompositeTransactionBackend::new("downstream-cache");
    let prepared = {
        let mut prepare_backends: [&mut dyn CompositePrepareBackend; 2] = [&mut cache, &mut worker];
        prepare_composite_plan(&envelope, &mut prepare_backends).unwrap()
    };
    let report = {
        let mut tx_backends: [&mut dyn CompositeTransactionBackend; 2] = [&mut cache, &mut worker];
        execute_composite_transaction(prepared, &mut tx_backends, &CancellationToken::new())
            .unwrap()
    };

    assert_eq!(report.schema_version(), COMPOSITE_TRANSACTION_SCHEMA_V1);
    assert_eq!(report.commits().len(), 2);
    assert!(report.checkpoint_cleanup().is_none());
    assert_eq!((worker.visible, cache.visible), (1, 1));
    assert!(worker.committed && cache.committed);
    assert!(!worker.checkpoint_active && !cache.checkpoint_active);
}

/// Semantic proof that ELANG5d policy blocks compile and lower through only
/// the public `elastic` facade dependency.
pub fn public_policy_dsl_surface_smoke() {
    let policy = downstream_language_document::worker_policy::policy_spec().unwrap();
    assert_eq!(
        policy.policy().header().identity().id().as_str(),
        "downstream.worker-policy"
    );
    assert_eq!(policy.policy().guarded_resource().guards().len(), 1);
    assert_eq!(policy.policy().constraints().len(), 2);
    assert_eq!(policy.numeric_objectives().len(), 1);
    assert_eq!(policy.planner_hints().len(), 1);

    let eir = downstream_language_document::worker_policy::policy_eir().unwrap();
    assert_eq!(eir.policy().constrained_resource().constraints().len(), 2);
    assert_eq!(
        eir.numeric_objectives()[0].objective(),
        &ObjectiveId::LATENCY
    );
    assert_eq!(eir.planner_hints()[0].key(), "search.mode");
    assert_eq!(eir.planner_hints()[0].value(), "balanced");
}

/// Semantic proof that ELANG5a policy identity/version/target binding is
/// available through only the public `elastic` dependency.
pub fn public_policy_identity_surface_smoke() {
    let document = downstream_language_document::document().unwrap();
    let resource_header = PolicyHeader::new(
        PolicyIdentity::new(
            PolicyId::new("downstream.runtime-balance").unwrap(),
            PolicyVersion::new(1, 2, 0),
        ),
        PolicyTarget::resource(LogicalResourceId::new("downstream-cache").unwrap()),
    );
    let resource_policy = EirPolicyHeader::lower_resource(&resource_header, &document).unwrap();
    assert_eq!(resource_policy.identity(), resource_header.identity());
    assert_eq!(resource_policy.target().kind(), PolicyTargetKind::Resource);
    assert_eq!(
        resource_policy.target_fingerprint(),
        document.resource("downstream-cache").unwrap().fingerprint()
    );

    let worker = LogicalResourceId::new("downstream-worker-pool").unwrap();
    let cache = LogicalResourceId::new("downstream-cache").unwrap();
    let group = ResourceGroupBuilder::new(ResourceGroupId::new("downstream-policy-group").unwrap())
        .members([worker, cache])
        .build()
        .unwrap();
    let grouped = EirGroupedDocument::new(document, &[group]).unwrap();
    let group_header = PolicyHeader::new(
        PolicyIdentity::new(
            PolicyId::new("downstream.runtime-balance").unwrap(),
            PolicyVersion::new(1, 2, 0),
        ),
        PolicyTarget::group(ResourceGroupId::new("downstream-policy-group").unwrap()),
    );
    let group_policy = EirPolicyHeader::lower_group(&group_header, &grouped).unwrap();
    assert_eq!(group_policy.target().kind(), PolicyTargetKind::Group);
    assert_eq!(
        group_policy.target_fingerprint(),
        grouped
            .group("downstream-policy-group")
            .unwrap()
            .fingerprint()
    );
    assert_ne!(resource_policy.fingerprint(), group_policy.fingerprint());
    assert_eq!(EIR_POLICY_HEADER_SCHEMA_VERSION, 1);
}

/// Semantic proof that ELANG5b resource-policy rules reuse the public Boolean
/// guard and pseudo-Boolean constraint authorities through only `elastic`.
pub fn public_resource_policy_rules_surface_smoke() {
    let resource = ResourceSpec::builder(
        ResourceClassId::CONFIGURATIONAL,
        LogicalResourceId::new("downstream-policy-resource").unwrap(),
    )
    .allow(DimensionId::CAPACITY)
    .admit(AdmissibleTransition::new(
        TransitionMechanism::Reinterpret,
        DimensionId::CAPACITY,
    ))
    .build()
    .unwrap();
    let capacity_ok = PredicateKey::new("elastic.downstream", "capacity-ok").unwrap();
    let high_mode = PredicateKey::new("elastic.downstream", "high-mode").unwrap();
    let registry = PredicateRegistry::from_keys([capacity_ok.clone()]).unwrap();
    let capacity_id = registry.id(&capacity_ok).unwrap();
    let guard = BooleanGuard::requires(
        GuardScope::Transition {
            mechanism: TransitionMechanism::Reinterpret,
            dimension: DimensionId::CAPACITY,
        },
        registry,
        capacity_id,
    )
    .unwrap();
    let constraint = PseudoBooleanConstraintDeclaration::capacity_budget(
        vec![WeightedPredicateKey::new(high_mode, 4).unwrap()],
        8,
        PseudoBooleanScale::new("units", 1).unwrap(),
    )
    .unwrap();
    let header = PolicyHeader::new(
        PolicyIdentity::new(
            PolicyId::new("downstream.capacity-policy").unwrap(),
            PolicyVersion::new(1, 0, 0),
        ),
        PolicyTarget::resource(LogicalResourceId::new("downstream-policy-resource").unwrap()),
    );
    let policy = ResourcePolicySpec::new(header, resource, vec![guard], vec![constraint]).unwrap();
    let eir = lower_resource_policy(&policy).unwrap();

    assert_eq!(
        eir.header().identity().id().as_str(),
        "downstream.capacity-policy"
    );
    assert_eq!(
        eir.constrained_resource().guarded_resource().guards().len(),
        1
    );
    assert_eq!(eir.constrained_resource().constraints().len(), 1);
    assert_eq!(eir.constrained_resource().constraints()[0].threshold(), 8);
    assert_eq!(
        eir.constrained_resource().constraints()[0].scale().unit(),
        "units"
    );
    assert_eq!(EIR_RESOURCE_POLICY_SCHEMA_VERSION, 1);
    assert_eq!(
        MAX_RESOURCE_POLICY_CONSTRAINTS,
        MAX_EIR_PSEUDO_BOOLEAN_CONSTRAINTS
    );
}

/// Semantic proof that ELANG5c numeric objective metadata and planner hints are
/// advisory and usable through only the public `elastic` dependency.
pub fn public_policy_advisory_surface_smoke() {
    let resource = ResourceSpec::builder(
        ResourceClassId::CONFIGURATIONAL,
        LogicalResourceId::new("downstream-advisory-resource").unwrap(),
    )
    .allow(DimensionId::CAPACITY)
    .optimize(ObjectiveId::LATENCY)
    .optimize(ObjectiveId::THROUGHPUT)
    .build()
    .unwrap();
    let policy = ResourcePolicySpec::new(
        PolicyHeader::new(
            PolicyIdentity::new(
                PolicyId::new("downstream.advisory").unwrap(),
                PolicyVersion::new(1, 0, 0),
            ),
            PolicyTarget::resource(LogicalResourceId::new("downstream-advisory-resource").unwrap()),
        ),
        resource,
        vec![],
        vec![],
    )
    .unwrap();
    let advisory = ResourcePolicyAdvisorySpec::new(
        policy,
        vec![
            PolicyNumericObjective::new(
                ObjectiveId::THROUGHPUT,
                PolicyObjectiveDirection::Maximize,
                PolicyMetricScale::new("ops-per-second", 1).unwrap(),
            ),
            PolicyNumericObjective::new(
                ObjectiveId::LATENCY,
                PolicyObjectiveDirection::Minimize,
                PolicyMetricScale::new("microseconds", 1).unwrap(),
            ),
        ],
        vec![
            PlannerHint::new(PlannerHintKey::new("candidate.limit").unwrap(), "16").unwrap(),
            PlannerHint::new(PlannerHintKey::new("search.mode").unwrap(), "balanced").unwrap(),
        ],
    )
    .unwrap();
    let lowered = lower_resource_policy_advisory(&advisory).unwrap();

    assert_eq!(
        lowered.numeric_objectives()[0].objective(),
        &ObjectiveId::LATENCY
    );
    assert_eq!(lowered.numeric_objectives()[0].rank(), 0);
    assert_eq!(lowered.numeric_objectives()[0].unit(), "microseconds");
    assert_eq!(
        lowered.numeric_objectives()[1].objective(),
        &ObjectiveId::THROUGHPUT
    );
    assert_eq!(lowered.planner_hints()[0].key(), "candidate.limit");
    assert_eq!(lowered.planner_hints()[1].key(), "search.mode");
    assert_eq!(EIR_RESOURCE_POLICY_ADVISORY_SCHEMA_VERSION, 1);

    let semantic = lowered.policy().fingerprint();
    let changed_hint = ResourcePolicyAdvisorySpec::new(
        advisory.policy().clone(),
        advisory.numeric_objectives().to_vec(),
        vec![PlannerHint::new(PlannerHintKey::new("search.mode").unwrap(), "exhaustive").unwrap()],
    )
    .unwrap();
    let changed = lower_resource_policy_advisory(&changed_hint).unwrap();
    assert_eq!(semantic, changed.policy().fingerprint());
    assert_ne!(lowered.fingerprint(), changed.fingerprint());
}

/// Semantic proof that an exact provider-owned model profile set can be bound
/// to generic policy through only the public `elastic` facade.
pub fn public_model_execution_profile_policy_binding_smoke() {
    let capabilities = ModelExecutionCapabilitiesV1::new(
        "downstream-nnis",
        "model-r1",
        4,
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
    let header = PolicyHeader::new(
        PolicyIdentity::new(
            PolicyId::new("downstream-model-profile").unwrap(),
            PolicyVersion::new(1, 0, 0),
        ),
        PolicyTarget::resource(LogicalResourceId::new("downstream-inference").unwrap()),
    );
    let binding = ModelExecutionProfilePolicyBindingV1::new(header, &profiles).unwrap();
    assert_eq!(binding.entries().len(), 3);
    assert_eq!(binding.policy().constraints().len(), 1);
    assert_eq!(
        binding.predicate_for_profile("balanced").unwrap().name(),
        "rank-10"
    );

    let selected = ModelExecutionProfileSelectorV1
        .select(
            &profiles,
            ModelExecutionProfileEnvelopeV1::new(2, 5_000, 5_000).unwrap(),
        )
        .unwrap();
    let ModelExecutionProfileSelectionV1::Selected(plan) = selected else {
        panic!("downstream balanced model profile should be selected");
    };
    let predicate = binding.validate_plan(&plan).unwrap();
    assert_eq!(
        predicate,
        binding.predicate_for_profile("balanced").unwrap()
    );

    let lowered = lower_resource_policy(binding.policy()).unwrap();
    assert_eq!(lowered.constrained_resource().constraints().len(), 1);
    let expected_profile_set_fingerprint = profiles.fingerprint().to_string();
    assert_eq!(
        lowered
            .constrained_resource()
            .guarded_resource()
            .resource()
            .label("model-execution.profile-set-fingerprint"),
        Some(expected_profile_set_fingerprint.as_str())
    );
}

/// Compile-time proof that durable runtime evidence is available through only
/// the public `elastic` facade.
pub fn public_evidence_surface_smoke() {
    let schema = EvidenceSchema::V1;
    let command = EvidenceCommand::Run;
    assert_eq!(schema.as_str(), EVIDENCE_SCHEMA_V1);
    assert_eq!(command.as_str(), "run");
    let _bounded_ingest_limit = MAX_EVIDENCE_BYTES;
}

/// Compile-time proof that integrated guarded-planning evidence remains
/// available through the single public `elastic` dependency.
pub fn public_guarded_planning_trace_surface_smoke() {
    let _capture = capture_guarded_planning_trace;
    let _context_fingerprint = planning_context_fingerprint;
    let _decision: Option<GuardedPlanningDecision> = None;
    let _outcome: Option<GuardedPlanningOutcomeTrace> = None;
    let _trace: Option<GuardedPlanningTrace> = None;
    let _summary: Option<InvariantPrecheckTraceSummary> = None;
    let _fingerprint: Option<PlanningContextFingerprint> = None;
}

/// Compile-time proof that typed decision-trace comparison is reachable through
/// the single public `elastic` dependency.
pub fn public_decision_trace_diff_surface_smoke() {
    let kind = DecisionTraceChangeKind::Policy;
    let _change: Option<DecisionTraceChange> = None;
    let _diff: Option<DecisionTraceDiff> = None;
    assert_eq!(kind, DecisionTraceChangeKind::Policy);
}

/// Compile-time and semantic proof that the low-level Boolean primitives remain
/// reachable through the public facade without importing `elastic-core`.
pub fn public_boolean_surface_smoke() {
    let cpu_features = BooleanCpuFeatures::detect();
    assert_eq!(cpu_features, BooleanCpuFeatures::detect());
    let _architecture: BooleanCpuArchitecture = cpu_features.architecture();

    let capacity_ok = PredicateId::new(0);
    let pressure_critical = PredicateId::new(1);
    let expression = BoolExpr::all([
        BoolExpr::atom(capacity_ok),
        BoolExpr::negate(BoolExpr::atom(pressure_critical)),
    ]);
    let guard = CompiledGuard::compile(&expression).expect("bounded guard should compile");
    let facts = FactSet::new()
        .with(capacity_ok, TruthValue::True)
        .expect("predicate is in range")
        .with(pressure_critical, TruthValue::False)
        .expect("predicate is in range");

    assert!(guard.uses_mask_fast_path());
    assert_eq!(
        guard
            .evaluate(&facts)
            .expect("guard evaluation should succeed"),
        TruthValue::True
    );
}

/// Compile-time and semantic proof that BE14h source-bound thermal/energy
/// eligibility is usable through only the public `elastic` dependency.
pub fn public_thermal_energy_policy_surface_smoke() {
    use std::time::{Duration, Instant};

    let resource = ResourceSpec::builder(
        ResourceClassId::CONFIGURATIONAL,
        LogicalResourceId::new("downstream-thermal-energy").unwrap(),
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
    let thermal_source = ObservationSource::host("downstream:thermal");
    let energy_source = ObservationSource::host("downstream:power");
    let now = Instant::now();
    let context = PlanningContext::new()
        .observe(ObservationSignalId::THERMAL_MARGIN, 20.0)
        .observe(ObservationSignalId::ENERGY_RATE, 40.0);
    let observations = ObservationSnapshot::new(
        now,
        vec![
            Observation::from_source(
                thermal_source.clone(),
                ObservationSignalId::THERMAL_MARGIN,
                20.0,
                now,
            ),
            Observation::from_source(
                energy_source.clone(),
                ObservationSignalId::ENERGY_RATE,
                40.0,
                now,
            ),
        ],
    );
    let policy = BooleanThermalEnergyPreplannerV1::new(
        resource,
        TransitionMechanism::Reinterpret,
        DimensionId::ENERGY,
        10.0,
        50.0,
        thermal_source,
        energy_source,
        Duration::from_secs(1),
    )
    .unwrap();
    let report = policy
        .evaluate(
            &context,
            &observations,
            now,
            ObservationEpoch::new(11),
            ResourceGeneration::new(4),
        )
        .unwrap();
    assert_eq!(report.status, BooleanThermalEnergyStatusV1::Eligible);
    assert_eq!(report.evidence.combined_truth, "true");
}

/// Compile-time and semantic proof that stable-key guarded EIR can be authored
/// through only the public `elastic` dependency.
pub fn public_stable_guard_surface_smoke() {
    let capacity_ok = predicate("elastic.downstream", "capacity-ok").unwrap();
    let pressure_critical = predicate("elastic.downstream", "pressure-critical").unwrap();
    let predicates =
        ElasticPredicates::new([capacity_ok.clone(), pressure_critical.clone()]).unwrap();
    let expression = ElasticGuard::all([
        predicates.atom(&capacity_ok).unwrap(),
        ElasticGuard::not(predicates.atom(&pressure_critical).unwrap()),
    ]);
    let guard = ElasticGuard::transition(
        TransitionMechanism::Reinterpret,
        DimensionId::CAPACITY,
        predicates,
    )
    .when(expression)
    .unwrap();

    let resource = ResourceSpec::builder(
        ResourceClassId::CAPACITY_RESOURCE,
        LogicalResourceId::new("downstream-guarded-ram").unwrap(),
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
    .unwrap();
    let guarded = GuardedResourceSpec::new(resource, vec![guard]).unwrap();
    let eir = lower_guarded(&guarded).unwrap();

    assert_eq!(eir.guards().len(), 1);
    assert!(eir.resource().transitions()[0].capability_grounded());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_and_runtime_work_through_the_facade_alone() {
        let spec = DownstreamKv::resource_spec().unwrap();
        assert_eq!(spec.resource_id().as_str(), "downstream-kv");
        assert!(spec.admits(TransitionMechanism::Reencode, &DimensionId::REPRESENTATION));

        let document = lower(&spec).unwrap();
        assert!(document.resource("downstream-kv").unwrap().transitions()[0].capability_grounded());

        public_surface_smoke();
        public_evidence_surface_smoke();
        public_guarded_planning_trace_surface_smoke();
        public_decision_trace_diff_surface_smoke();
        public_boolean_surface_smoke();
        public_elastic_language_surface_smoke();
        public_elastic_document_surface_smoke();
        public_composite_plan_surface_smoke();
        public_composite_prepare_surface_smoke();
        public_composite_prepare_recovery_surface_smoke();
        public_composite_transaction_surface_smoke();
        public_policy_identity_surface_smoke();
        public_resource_policy_rules_surface_smoke();
        public_policy_advisory_surface_smoke();
        public_policy_dsl_surface_smoke();
        public_thermal_energy_policy_surface_smoke();
        public_stable_guard_surface_smoke();
    }
}
