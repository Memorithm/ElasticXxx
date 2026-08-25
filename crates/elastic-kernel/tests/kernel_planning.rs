//! End-to-end kernel-planning flows through the public `elastic-kernel`
//! surface: capability snapshots, candidate normalization, deterministic
//! selection, auditable evidence, and the realization lifecycle.
//!
//! These tests exercise exactly what downstream adapters can reach: no
//! crate-private constructors, no plan internals.

#![forbid(unsafe_code)]

use elastic::ContractId;
use elastic_core::{BuiltinObjective, LogicalResourceId, ObjectiveId};
use elastic_eir::Fingerprint;
use elastic_kernel::{
    plan, BindingLimits, CapabilitySnapshot, CommittedRealization, DecisiveEvidence, Evidence,
    EvidenceUnit, FeatureRequirement, FeatureSupport, KernelCandidate, KernelRequirements,
    ObjectiveEvidence, RejectedReason, SelectionOutcome, SelectionPolicy, StageAttestations,
    StaticQuantity, SubgroupSupport, WorkgroupLimits,
};

const CONTRACT_V1: &str = "attention-forward-v1";

fn latency() -> ObjectiveId {
    ObjectiveId::builtin(BuiltinObjective::Latency)
}

fn memory() -> ObjectiveId {
    ObjectiveId::builtin(BuiltinObjective::MemoryFootprint)
}

fn resource() -> LogicalResourceId {
    LogicalResourceId::new("attention#42").expect("valid logical identity")
}

/// Profile A: a portable baseline boundary.
fn profile_a_portable() -> CapabilitySnapshot {
    let snapshot = CapabilitySnapshot {
        workgroup_limits: WorkgroupLimits {
            max_invocations_per_axis: [256, 256, 64],
            max_invocations_per_workgroup: 256,
            max_workgroups_per_axis: 65535,
            max_workgroup_storage_bytes: 32_768,
        },
        binding_limits: BindingLimits {
            max_bind_groups: 8,
            max_storage_buffer_binding_bytes: 128 << 20,
        },
        subgroup_support: SubgroupSupport::unsupported(),
        shader_f16: FeatureSupport::Known(false),
        matrix_ops: FeatureSupport::Unknown,
    };
    CapabilitySnapshot::new(snapshot).expect("profile A is internally consistent")
}

/// Profile B: a subgroup-capable boundary with f16 unreported.
fn profile_b_subgroup() -> CapabilitySnapshot {
    let mut snapshot = profile_a_portable();
    snapshot.subgroup_support = SubgroupSupport::supported(4, 64).expect("valid range");
    // Profile B can execute subgroups but its discovery layer cannot report
    // on native f16 at all: unknown, not false.
    snapshot.shader_f16 = FeatureSupport::Unknown;
    CapabilitySnapshot::new(snapshot).expect("profile B is internally consistent")
}

/// Profile C: a richer boundary; f16 known-present, larger storage.
fn profile_c_rich() -> CapabilitySnapshot {
    let mut snapshot = profile_b_subgroup();
    snapshot.shader_f16 = FeatureSupport::Known(true);
    snapshot.workgroup_limits.max_workgroup_storage_bytes = 64 << 10;
    CapabilitySnapshot::new(snapshot).expect("profile C is internally consistent")
}

fn requirements(subgroup_min_width: Option<u32>) -> KernelRequirements {
    KernelRequirements {
        invocations_per_workgroup: 64,
        invocations_per_axis: [64, 1, 1],
        workgroup_storage_bytes: 24_576,
        bind_groups: 2,
        max_storage_buffer_binding_bytes: 4096,
        subgroup_min_width,
        shader_f16: FeatureRequirement::NotRequired,
        matrix_ops: FeatureRequirement::NotRequired,
    }
    .validate_expect()
}

trait RequirementFixture {
    fn validate_expect(self) -> KernelRequirements;
}

impl RequirementFixture for KernelRequirements {
    fn validate_expect(self) -> KernelRequirements {
        self.validate()
            .expect("fixture requirements are consistent");
        self
    }
}

fn candidate(
    realization: &str,
    subgroup_min_width: Option<u32>,
    latency_nanoseconds: u64,
) -> KernelCandidate {
    let mut evidence = ObjectiveEvidence::new();
    evidence.attach(
        latency(),
        Evidence::StaticEstimate(StaticQuantity {
            magnitude: latency_nanoseconds,
            unit: EvidenceUnit::Nanoseconds,
        }),
    );
    if realization == "f16-fast-path" {
        // Only admissible where f16 is reported present.
        evidence.attach(memory(), Evidence::Unknown);
    }
    KernelCandidate::new(
        resource(),
        elastic_kernel::RealizationIdentity::new(realization).expect("valid"),
        1,
        requirements(subgroup_min_width),
        ContractId::new(CONTRACT_V1).expect("valid"),
        evidence,
    )
    .expect("candidate fixture is valid")
}

fn f16_candidate(realization: &str) -> KernelCandidate {
    let inner = candidate(realization, None, 40);
    let requirements = {
        let mut r = *inner.requirements();
        r.shader_f16 = FeatureRequirement::Required;
        r
    };
    KernelCandidate::new(
        resource(),
        elastic_kernel::RealizationIdentity::new(realization).expect("valid"),
        inner.schema_version(),
        requirements,
        ContractId::new(CONTRACT_V1).expect("valid"),
        ObjectiveEvidence::new().with(
            latency(),
            Evidence::StaticEstimate(StaticQuantity {
                magnitude: 60,
                unit: EvidenceUnit::Nanoseconds,
            }),
        ),
    )
    .expect("f16 candidate fixture is valid")
}

fn policy(objectives: &[ObjectiveId]) -> SelectionPolicy {
    SelectionPolicy::new(
        objectives.to_vec(),
        ContractId::new(CONTRACT_V1).expect("valid"),
        true,
    )
    .expect("policy is valid")
}

fn workload_fingerprint() -> Fingerprint {
    Fingerprint::EMPTY.text("workload/b=1/h=8/n=128/d=64/causal=true/contract=attention-forward-v1")
}

#[test]
fn capability_profiles_admit_progressively_larger_candidate_sets() {
    let portable = candidate("portable-q4", None, 100);
    let subgroup = candidate("subgroup-q4", Some(4), 55);

    // Profile A: only the portable path survives.
    let outcome = plan(
        &resource(),
        workload_fingerprint(),
        &profile_a_portable(),
        &policy(&[latency()]),
        &[portable.clone(), subgroup.clone()],
    );
    let SelectionOutcome::Selected(record) = outcome else {
        panic!("profile A must select the portable path");
    };
    assert_eq!(record.selected_realization().as_str(), "portable-q4");
    assert_eq!(record.rejected().len(), 1);
    assert_eq!(
        *record.rejected()[0].reason(),
        RejectedReason::Infeasible(elastic_kernel::CapabilityRejectionReason::SubgroupUnsupported)
    );

    // Profile B: both paths are admissible; the faster estimate wins.
    let outcome = plan(
        &resource(),
        workload_fingerprint(),
        &profile_b_subgroup(),
        &policy(&[latency()]),
        &[portable.clone(), subgroup],
    );
    let SelectionOutcome::Selected(record) = outcome else {
        panic!("profile B must select a path");
    };
    assert_eq!(record.selected_realization().as_str(), "subgroup-q4");
    assert!(record.rejected().is_empty());
    assert_eq!(
        record.capability_fingerprint(),
        profile_b_subgroup().fingerprint()
    );

    // The logical identity never moved, even though the realization did.
    assert_eq!(*record.logical_resource_id(), resource());
}

#[test]
fn same_logical_resource_different_realizations_across_profiles() {
    // The defining Elastic property: one logical kernel resource, two
    // capability worlds, two different committed realizations.
    fn select_and_commit(
        snapshot: &CapabilitySnapshot,
        prefer_subgroup: bool,
    ) -> CommittedRealization {
        let candidates = vec![
            candidate("portable-q4", None, 100),
            candidate("subgroup-q4", Some(4), 55),
        ];
        let _ = prefer_subgroup;
        let selection_policy = policy(&[latency()]);
        let outcome = plan(
            &resource(),
            workload_fingerprint(),
            snapshot,
            &selection_policy,
            &candidates,
        );
        let SelectionOutcome::Selected(record) = outcome else {
            panic!("expected selection on this profile");
        };
        let proposal = elastic_kernel::lifecycle::RealizationProposal::start(
            candidates
                .iter()
                .find(|c| c.realization().as_str() == record.selected_realization().as_str())
                .expect("selected candidate exists")
                .clone(),
            record.fingerprint(),
        );
        proposal
            .validate(StageAttestations::none().attesting_validation())
            .expect("qualified")
            .activate(StageAttestations::none().attesting_activation())
            .expect("activated")
            .verify(StageAttestations::none().attesting_verification())
            .expect("verified")
            .commit()
    }

    // On profile A only the portable path is admissible; on profile C both
    // are, and the faster subgroup estimate wins.
    let on_profile_a = select_and_commit(&profile_a_portable(), false);
    let on_profile_c = select_and_commit(&profile_c_rich(), true);

    assert_eq!(
        on_profile_a.logical_resource_id(),
        on_profile_c.logical_resource_id()
    );
    assert_ne!(on_profile_a.realization(), on_profile_c.realization());
    assert_ne!(on_profile_a.fingerprint(), on_profile_c.fingerprint());
}

#[test]
fn unknown_capability_never_becomes_silent_support_or_absence() {
    // The f16 candidate requires shader-f16; profile B does not report it.
    let outcome = plan(
        &resource(),
        workload_fingerprint(),
        &profile_b_subgroup(),
        &policy(&[latency()]),
        &[f16_candidate("f16-fast-path")],
    );
    let SelectionOutcome::NoCandidate { rejections, .. } = outcome else {
        panic!("unknown f16 support must not admit the f16 candidate");
    };
    assert_eq!(
        rejections[0].reason().clone(),
        RejectedReason::Infeasible(elastic_kernel::CapabilityRejectionReason::FeatureUnknown {
            feature: elastic_kernel::Feature::ShaderF16,
        })
    );

    // On profile C the same candidate becomes admissible and wins on its
    // estimate.
    let outcome = plan(
        &resource(),
        workload_fingerprint(),
        &profile_c_rich(),
        &policy(&[latency()]),
        &[f16_candidate("f16-fast-path")],
    );
    let SelectionOutcome::Selected(record) = outcome else {
        panic!("known-true f16 must admit the f16 candidate");
    };
    assert_eq!(record.selected_realization().as_str(), "f16-fast-path");
}

#[test]
fn selection_evidence_is_deterministic_under_insertion_order_permutations() {
    let forward = vec![
        candidate("portable-q4", None, 100),
        candidate("vec4-q4", None, 90),
        candidate("subgroup-q4", Some(4), 55),
        candidate("wide-subgroup-q4", Some(8), 50),
    ];
    let mut reversed = forward.clone();
    reversed.reverse();

    let snapshot = profile_c_rich();
    let selection_policy = policy(&[latency()]);
    let first = plan(
        &resource(),
        workload_fingerprint(),
        &snapshot,
        &selection_policy,
        &forward,
    );
    let second = plan(
        &resource(),
        workload_fingerprint(),
        &snapshot,
        &selection_policy,
        &reversed,
    );
    assert_eq!(first, second);

    // On the rich profile all four candidates are admissible, so rejection
    // lists are empty; determinism of the full outcome (including the
    // candidate-set fingerprint) is asserted above by outcome equality.
    let SelectionOutcome::Selected(record) = second else {
        panic!("expected selection");
    };
    assert_eq!(record.planner_version(), elastic_kernel::PLANNER_VERSION);
    assert!(record.rejected().is_empty());
    assert_eq!(record.selected_realization().as_str(), "wide-subgroup-q4");
}

#[test]
fn measured_evidence_replaces_static_estimates_without_api_change() {
    // Today's world: static estimates decide. Tomorrow's world after a
    // benchmark run: measurements arrive through the same contract and
    // immediately dominate.
    let mut static_world = vec![
        candidate("portable-q4", None, 100),
        candidate("subgroup-q4", Some(4), 55),
    ];
    let snapshot = profile_c_rich();

    let outcome = plan(
        &resource(),
        workload_fingerprint(),
        &snapshot,
        &policy(&[latency()]),
        &static_world,
    );
    let SelectionOutcome::Selected(static_record) = outcome else {
        panic!("static world selects");
    };
    assert!(matches!(
        static_record.decisive_evidence(),
        Some(DecisiveEvidence::StaticEstimate { .. })
    ));

    static_world[0] = {
        KernelCandidate::new(
            resource(),
            elastic_kernel::RealizationIdentity::new("portable-q4").expect("valid"),
            1,
            requirements(None),
            ContractId::new(CONTRACT_V1).expect("valid"),
            ObjectiveEvidence::new().with(
                latency(),
                Evidence::Measured(elastic_kernel::MeasuredQuantity {
                    magnitude: 70,
                    unit: EvidenceUnit::Nanoseconds,
                    protocol_version: 1,
                    samples: 101,
                }),
            ),
        )
        .expect("measured candidate valid")
    };

    let outcome = plan(
        &resource(),
        workload_fingerprint(),
        &snapshot,
        &policy(&[latency()]),
        &static_world,
    );
    let SelectionOutcome::Selected(measured_record) = outcome else {
        panic!("measured world selects");
    };
    // The measured portable candidate now outranks the faster-but-only-
    // estimated subgroup candidate.
    assert_eq!(
        measured_record.selected_realization().as_str(),
        "portable-q4"
    );
    assert!(matches!(
        measured_record.decisive_evidence(),
        Some(DecisiveEvidence::Measured { .. })
    ));
}
