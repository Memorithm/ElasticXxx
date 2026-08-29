//! Strategy tests: honest outcomes, deterministic control laws, closed-loop
//! safety with a real adapter.

use elastic_adapters::{
    AdapterError, HeadroomPlanner, PlannerConfigError, RamBudget, ThresholdPlanner,
};
use elastic_core::resource::{DimensionId, ObservationSignalId, ResourceClassId, ResourceSpec};
use elastic_core::TransitionMechanism::Reinterpret;
use elastic_eir::{lower, PlanOutcome, PlanningContext, TransitionPlanner};

use elastic_core::resource::{AdmissibleTransition, CapabilityRequirement, LogicalResourceId};

fn capacity_resource() -> elastic_eir::EirResource {
    let spec = ResourceSpec::builder(
        ResourceClassId::CAPACITY_RESOURCE,
        LogicalResourceId::new("cache").unwrap(),
    )
    .allow(DimensionId::CAPACITY)
    .admit(AdmissibleTransition::new(
        Reinterpret,
        DimensionId::CAPACITY,
    ))
    .require_capability(CapabilityRequirement::new(
        Reinterpret,
        DimensionId::CAPACITY,
    ))
    .build()
    .unwrap();
    lower(&spec).unwrap().resource("cache").unwrap().clone()
}

fn context_of(budget: &RamBudget) -> PlanningContext {
    budget.observe()
}

#[test]
fn threshold_configuration_is_validated() {
    assert_eq!(
        ThresholdPlanner::new(0.8, 0.3, 0.5),
        Err(PlannerConfigError::InvalidWatermarks {
            low_watermark: 0.8,
            high_watermark: 0.3,
            step_fraction: 0.5
        })
    );
    assert_eq!(
        ThresholdPlanner::new(0.2, 1.2, 0.5),
        Err(PlannerConfigError::InvalidWatermarks {
            low_watermark: 0.2,
            high_watermark: 1.2,
            step_fraction: 0.5
        })
    );
}

#[test]
fn threshold_planner_decision_table_is_honest() {
    let planner = ThresholdPlanner::new(0.30, 0.80, 0.5).unwrap();
    let resource = capacity_resource();

    // Without context, the strategy says so honestly.
    assert!(matches!(
        planner.propose_transition(&resource),
        PlanOutcome::InsufficientEvidence { .. }
    ));

    // High committed pressure: reservation is 80% of host total -> grow by
    // the step fraction. Numeric adapter bounds remain a separate action-time
    // concern, so the advisory target may exceed this test adapter's max.
    let budget = RamBudget::new("cache", 6_250, 100, 6_000, 5_000, None).unwrap();
    match planner.propose_transition_with_context(&resource, &context_of(&budget)) {
        PlanOutcome::Candidate(candidate) => {
            assert_eq!(candidate.magnitude(), Some(7500));
            assert!(candidate.is_declared_in(&resource));
        }
        other => panic!("expected grow candidate, got {other}"),
    }

    // Low pressure: a 25% commitment -> shrink.
    let low = RamBudget::new("cache", 10_000, 100, 10_000, 2_500, None).unwrap();
    match planner.propose_transition_with_context(&resource, &context_of(&low)) {
        PlanOutcome::Candidate(candidate) => {
            assert_eq!(candidate.magnitude(), Some(1250));
        }
        other => panic!("expected shrink candidate, got {other}"),
    }

    // Inside the band -> explicit stability (50% commitment sits between the
    // watermarks).
    let mid = RamBudget::new("cache", 10_000, 100, 10_000, 5_000, None).unwrap();
    assert_eq!(
        planner.propose_transition_with_context(&resource, &context_of(&mid)),
        PlanOutcome::NoCandidate
    );
}

#[test]
fn missing_evidence_and_inapplicable_vocabulary_are_reported() {
    let planner = ThresholdPlanner::new(0.3, 0.8, 0.5).unwrap();
    let resource = capacity_resource();

    let empty_context = PlanningContext::new();
    assert_eq!(
        planner.propose_transition_with_context(&resource, &empty_context),
        PlanOutcome::InsufficientEvidence {
            detail: "missing utilization observation".to_owned()
        }
    );

    // A resource with no capacity admission is outside this strategy's
    // vocabulary entirely.
    let spec = ResourceSpec::builder(
        ResourceClassId::SHARED,
        elastic_core::resource::LogicalResourceId::new("pool").unwrap(),
    )
    .allow(DimensionId::CONCURRENCY)
    .admit(elastic_core::resource::AdmissibleTransition::new(
        Reinterpret,
        DimensionId::CONCURRENCY,
    ))
    .require_capability(elastic_core::resource::CapabilityRequirement::new(
        Reinterpret,
        DimensionId::CONCURRENCY,
    ))
    .build()
    .unwrap();
    let pool_ir = lower(&spec).unwrap().resource("pool").unwrap().clone();
    let context = PlanningContext::new().observe(ObservationSignalId::UTILIZATION, 0.9);
    assert_eq!(
        planner.propose_transition_with_context(&pool_ir, &context),
        PlanOutcome::Unsupported
    );
}

#[test]
fn headroom_regulator_settles_inside_the_deadband() {
    let planner = HeadroomPlanner::new(0.5, 0.05).unwrap();
    let resource = capacity_resource();

    // Too much free space -> grow toward the 50% headroom line.
    let mut budget = RamBudget::new("cache", 10_000, 100, 10_000, 2_500, None).unwrap();
    let outcome = planner.propose_transition_with_context(&resource, &context_of(&budget));
    let PlanOutcome::Candidate(candidate) = outcome else {
        panic!("expected regulation candidate");
    };
    assert_eq!(candidate.magnitude(), Some(5000));
    let (_, to) = budget.apply(candidate.magnitude().unwrap()).unwrap();

    // After acting, the regulator reports stability.
    let settled = planner.propose_transition_with_context(&resource, &context_of(&budget));
    assert_eq!(settled, PlanOutcome::NoCandidate);
    assert_eq!(budget.committed(), to);

    // Not enough headroom -> shrink toward the line.
    let mut hot = RamBudget::new("hot", 10_000, 100, 10_000, 9_000, None).unwrap();
    // Protect more than the regulator's shrink target: contents win.
    hot.record_use(6_000).unwrap();
    // Shrink below in-use bytes must be refused even when the planner wants it.
    let outcome = planner.propose_transition_with_context(&resource, &context_of(&hot));
    let PlanOutcome::Candidate(candidate) = outcome else {
        panic!("expected shrink candidate");
    };
    assert!(matches!(
        hot.apply(candidate.magnitude().unwrap()),
        Err(AdapterError::WouldViolateContents { .. })
    ));
}

#[test]
fn strategies_are_deterministic_pure_functions() {
    let planner = ThresholdPlanner::new(0.3, 0.8, 0.25).unwrap();
    let resource = capacity_resource();
    let budget = RamBudget::new("cache", 10_000, 100, 10_000, 9_000, None).unwrap();
    let context = context_of(&budget);
    let first = planner.propose_transition_with_context(&resource, &context);
    for _ in 0..32 {
        assert_eq!(
            planner.propose_transition_with_context(&resource, &context),
            first
        );
    }
}
