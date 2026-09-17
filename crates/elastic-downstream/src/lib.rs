//! Compile-time guard for the advertised single-dependency contract.
//!
//! This crate deliberately depends **only** on [`elastic`]. It exercises both
//! declaration and operational runtime types so workspace CI catches accidental
//! leaks of implementation-crate dependencies into downstream code.

#![forbid(unsafe_code)]

use elastic::prelude::*;

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
        public_stable_guard_surface_smoke();
    }
}
