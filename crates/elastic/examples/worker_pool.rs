//! Example D — a generic non-LLM resource through the full pipeline.
//!
//! A worker pool whose `parallelism` is elastic. The example shows that
//! nothing in the declaration → EIR → validation pipeline assumes KV caches,
//! attention, transformers, accelerators, or any hardware topology.
//!
//! Layer separation is explicit:
//! - declaration + EIR lowering = planning metadata;
//! - `admits_action` = structural validation of a candidate action;
//! - `apply` = an **in-memory simulation** standing in for a physical actuator
//!   (here: adjusting a counter). It runs only after validation and never
//!   bypasses it.

use elastic::prelude::*;

#[derive(ElasticResource)]
#[elastic(
    class(shared),
    id("worker-pool"),
    allow(parallelism),
    preserve(identity),
    optimize(throughput),
    optimize(stability),
    admit(reinterpret @ parallelism),
    capability(reinterpret @ parallelism),
    observe(queue_depth),
    observe(utilization)
)]
struct WorkerPool;

/// Simulated live state of the pool (plain application data).
struct Pool {
    workers: u32,
}

impl Pool {
    /// Structural check only: is this parallelism change admitted at all?
    fn admits_action(&self, spec: &ResourceSpec, to: u32) -> Result<(), String> {
        let dimension = DimensionId::PARALLELISM;
        if !spec.is_elastic(&dimension) {
            return Err("parallelism is not elastic for this resource".into());
        }
        if !spec.admits(TransitionMechanism::Reinterpret, &dimension) {
            return Err("reinterpret is not admitted along parallelism".into());
        }
        let _ = to;
        Ok(())
    }

    /// Simulated actuator. Runs only behind `admits_action`.
    fn apply(&mut self, to: u32) {
        self.workers = to;
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let spec = WorkerPool::resource_spec()?;
    let document = lower(&spec)?;
    println!("eir: {} {}", document, document.fingerprint());
    println!(
        "objectives: {:?} (priority order)",
        spec.objectives()
            .iter()
            .map(ObjectiveId::as_str)
            .collect::<Vec<_>>()
    );

    let mut pool = Pool { workers: 4 };

    // Admissible change: reinterpret along parallelism.
    pool.admits_action(&spec, 8)?;
    pool.apply(8);
    println!("scaled pool to {} workers", pool.workers);

    // The same pipeline rejects actions outside the declaration.
    let rigid_spec = ResourceSpec::builder(
        ResourceClassId::EXCLUSIVE,
        LogicalResourceId::new("single-writer-lock")?,
    )
    .allow(DimensionId::LOCALITY)
    .build()?;
    assert!(pool.admits_action(&rigid_spec, 16).is_err());
    println!("rejected resize against a declaration without elastic parallelism");

    // Observation metadata is recorded even though sampling is out of scope.
    assert_eq!(
        spec.observed_signals(),
        &[
            ObservationSignalId::UTILIZATION,
            ObservationSignalId::QUEUE_DEPTH
        ]
    );
    Ok(())
}
