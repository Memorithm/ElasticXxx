//! Example — a RAM budget through the full adapter discipline:
//!
//! declaration (typed spec) → EIR → observations → candidate proposal →
//! validated physical action → invariant enforcement.
//!
//! The allocation is real: growth reserves actual memory, shrink releases
//! it. `PreserveContents` is enforced by the adapter at action time, no
//! matter what was proposed.

use elastic_adapters::RamBudget;
use elastic_core::resource::ObservationSignalId;

fn mib(bytes: u64) -> String {
    format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
}

fn main() -> Result<(), elastic_adapters::AdapterError> {
    // Operator configuration stands in for trusted capability discovery.
    let mut budget = RamBudget::new(
        "inference-cache",
        64 << 20,
        256 << 10,
        32 << 20,
        4 << 20,
        Some(8 << 20),
    )?;

    println!("declared: {}", budget.spec());
    println!(
        "bounds: [{}, {}], one admitted transition, EIR node: {}",
        mib(budget.bounds().0),
        mib(budget.bounds().1),
        budget.ir()
    );
    println!("committed: {}", mib(budget.committed()));

    // Application takes protected ownership of part of the commitment.
    budget.record_use(3 << 20)?;
    println!("in use: {}", mib(budget.in_use()));

    // Observations feed planners.
    let context = budget.observe();
    println!(
        "observed: utilization={:.3} free={}",
        context.get(ObservationSignalId::UTILIZATION).unwrap_or(0.0),
        mib(context
            .get(ObservationSignalId::FREE_CAPACITY)
            .unwrap_or(0.0) as u64)
    );

    // A planner proposes growing to 12 MiB; the adapter re-validates bounds,
    // the step limit, and contents before acting.
    if let Some(proposal) = budget.candidate(12 << 20) {
        println!("proposal: {proposal}");
        let (from, to) = budget.apply(proposal.magnitude().unwrap_or_default())?;
        println!("applied: {} -> {}", mib(from), mib(to));
    }

    // Invariant enforcement: shrinking below in-use bytes is refused even
    // though the transition itself is admitted.
    let blocked = budget.apply(1 << 20);
    println!("refused: {}", blocked.unwrap_err());

    budget.release_use(2 << 20)?;
    let (from, to) = budget.apply(6 << 20)?;
    println!("after release: {from} -> {} ({})", to, mib(to));

    Ok(())
}
