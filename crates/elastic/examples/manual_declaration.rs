//! Example A — ordinary Rust API, no macros.
//!
//! Declares a KV-cache-like representational resource entirely through the
//! typed builder, then lowers it to validated EIR.

use elastic::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let spec = ResourceSpec::builder(
        ResourceClassId::REPRESENTATIONAL,
        LogicalResourceId::new("session-kv")?,
    )
    // WHAT MAY CHANGE
    .allow(DimensionId::REPRESENTATION)
    .allow(DimensionId::RESIDENCY)
    // WHAT MUST REMAIN TRUE
    .preserve(Invariant::new(InvariantKind::PreserveContents))
    .preserve(
        Invariant::new(InvariantKind::UpholdContract(ContractId::new(
            "kv.reuse-contract",
        )?))
        .along(DimensionId::REPRESENTATION),
    )
    // WHAT THE RUNTIME MAY OPTIMIZE (priority order)
    .optimize(ObjectiveId::LATENCY)
    .optimize(ObjectiveId::MEMORY_FOOTPRINT)
    // HOW CHANGES MAY HAPPEN, AND WHICH TRUSTED CAPABILITIES THEY NEED
    .admit(AdmissibleTransition::new(
        TransitionMechanism::Reencode,
        DimensionId::REPRESENTATION,
    ))
    .require_capability(CapabilityRequirement::new(
        TransitionMechanism::Reencode,
        DimensionId::REPRESENTATION,
    ))
    .observe(ObservationSignalId::FREE_CAPACITY)
    .build()?;

    println!("declared: {spec}");
    for invariant in spec.invariants() {
        println!("  invariant: {invariant}");
    }

    let document = lower(&spec)?;
    let resource = document.resource("session-kv").expect("just lowered");
    println!(
        "eir: {} fingerprint={} transitions={}",
        document,
        document.fingerprint(),
        resource
            .transitions()
            .iter()
            .map(|admitted| format!(
                "{}(grounded={})",
                admitted.transition(),
                admitted.capability_grounded()
            ))
            .collect::<Vec<_>>()
            .join(", ")
    );
    Ok(())
}
