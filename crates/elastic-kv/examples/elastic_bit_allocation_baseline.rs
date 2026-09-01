//! Generate the frozen Stage-A ElasticBitAllocation baseline records.
//!
//! This executable evaluates the closed candidate list only.  It does not
//! search, allocate, select a winner, or claim real-model quality/latency.

use elastic_kv::{run_fixed_baseline, SyntheticCorpus};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let corpus = SyntheticCorpus::deterministic();
    for result in run_fixed_baseline(&corpus)? {
        println!("{}", result.canonical_record());
    }
    Ok(())
}
