//! Example B — the same intent through `#[derive(ElasticResource)]`.
//!
//! Asserts that the macro declaration and the manual builder produce equal
//! specs and equivalent normalized EIR, proving there is exactly one semantic
//! implementation behind both surfaces.

use elastic::prelude::*;

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
    observe(free_capacity)
)]
struct SessionKv;

fn manual() -> Result<ResourceSpec, ResourceSpecError> {
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
    .require_capability(CapabilityRequirement::new(
        TransitionMechanism::Reencode,
        DimensionId::REPRESENTATION,
    ))
    .observe(ObservationSignalId::FREE_CAPACITY)
    .build()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let from_macro = SessionKv::resource_spec()?;
    let from_manual = manual()?;

    assert_eq!(
        from_macro, from_manual,
        "one semantic model behind both APIs"
    );

    // Both normalize to identical EIR.
    let macro_doc = lower(&from_macro)?;
    let manual_doc = lower(&from_manual)?;
    assert_eq!(macro_doc.fingerprint(), manual_doc.fingerprint());

    println!("macro spec == manual spec: {}", from_macro);
    println!("identical EIR: {} {}", macro_doc, macro_doc.fingerprint());
    Ok(())
}
