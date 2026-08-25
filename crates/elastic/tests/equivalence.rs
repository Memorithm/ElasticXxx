//! Macro and manual API must produce semantically identical declarations.

use elastic::prelude::*;

/// Manual declaration (Example A style).
pub fn manual_session_kv() -> Result<ResourceSpec, ResourceSpecError> {
    ResourceSpec::builder(
        ResourceClassId::REPRESENTATIONAL,
        LogicalResourceId::new("session-kv")?,
    )
    .allow(DimensionId::REPRESENTATION)
    .allow(DimensionId::RESIDENCY)
    .preserve(Invariant::new(InvariantKind::PreserveContents))
    .preserve(
        Invariant::new(InvariantKind::UpholdContract(ContractId::new(
            "kv.reuse-contract",
        )?))
        .along(DimensionId::REPRESENTATION),
    )
    .optimize(ObjectiveId::LATENCY)
    .optimize(ObjectiveId::MEMORY_FOOTPRINT)
    .admit(AdmissibleTransition::new(
        TransitionMechanism::Reencode,
        DimensionId::REPRESENTATION,
    ))
    .admit(AdmissibleTransition::new(
        TransitionMechanism::Reinterpret,
        DimensionId::RESIDENCY,
    ))
    .require_capability(CapabilityRequirement::new(
        TransitionMechanism::Reencode,
        DimensionId::REPRESENTATION,
    ))
    .observe(ObservationSignalId::FREE_CAPACITY)
    .observe(ObservationSignalId::QUEUE_DEPTH)
    .label("workload", "slha-v2")
    .build()
}

/// The identical declaration through the derive macro.
#[derive(ElasticResource)]
#[elastic(
    class(representational),
    id("session-kv"),
    allow(representation, residency),
    preserve(contents),
    preserve(contract("kv.reuse-contract") along representation),
    optimize(latency, memory_footprint),
    admit(reencode @ representation),
    capability(reencode @ representation),
    admit(reinterpret @ residency),
    observe(free_capacity, queue_depth),
    label("workload", "slha-v2"),
)]
struct SessionKv;

#[test]
fn macro_and_manual_api_agree() {
    let manual = manual_session_kv().unwrap();
    let generated = SessionKv::resource_spec().unwrap();

    assert_eq!(manual, generated);
    assert_eq!(manual.to_string(), generated.to_string());

    // And both normalize to equivalent EIR.
    let manual_doc = elastic::lower(&manual).unwrap();
    let generated_doc = elastic::lower(&generated).unwrap();
    assert_eq!(manual_doc, generated_doc);
    assert_eq!(manual_doc.fingerprint(), generated_doc.fingerprint());
}

#[test]
fn default_identity_is_the_struct_name() {
    #[derive(ElasticResource)]
    #[elastic(class(stateful), allow(capacity))]
    struct WorkerPool;

    let spec = WorkerPool::resource_spec().unwrap();
    assert_eq!(spec.resource_id().as_str(), "WorkerPool");
}

#[test]
fn custom_terms_lower_through_the_macro() {
    #[derive(ElasticResource)]
    #[elastic(
        class(custom("agent-memory")),
        allow(custom("thermal-envelope"), capacity),
        preserve(contents along custom("thermal-envelope")),
        optimize(custom("tail-latency-p99")),
        observe(custom("page-fault-rate"))
    )]
    struct HotSet;

    let spec = HotSet::resource_spec().unwrap();
    assert_eq!(spec.class().as_str(), "agent-memory");
    let thermal = DimensionId::custom("thermal-envelope").unwrap();
    let tail = ObjectiveId::custom("tail-latency-p99").unwrap();
    // Canonical order: built-ins first, custom terms after.
    assert_eq!(
        spec.elastic_dimensions(),
        &[DimensionId::CAPACITY, thermal.clone()]
    );
    // Scoped invariant with a custom dimension survives lowering.
    assert_eq!(spec.invariants()[0].scope(), Some(&thermal));
    assert_eq!(spec.objectives(), &[tail]);
}

mod nested {
    pub mod deep {
        use elastic::prelude::*;

        #[derive(ElasticResource)]
        #[elastic(class(exclusive), allow(concurrency), preserve(identity))]
        pub struct LockHandle<T: Clone> {
            _marker: std::marker::PhantomData<T>,
        }

        impl<T: Clone> LockHandle<T> {
            pub fn new() -> Self {
                Self {
                    _marker: std::marker::PhantomData,
                }
            }
        }
    }
}

#[test]
fn generics_and_module_nesting_are_preserved() {
    let _handle = nested::deep::LockHandle::<String>::new();
    let spec = nested::deep::LockHandle::<String>::resource_spec().unwrap();
    assert_eq!(spec.class(), &ResourceClassId::EXCLUSIVE);
    assert_eq!(spec.elastic_dimensions(), &[DimensionId::CONCURRENCY]);
    // Invariants survive generic expansion.
    assert_eq!(spec.invariants().len(), 1);
}
