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

/// Compile-time proof that a downstream crate can name runtime and adapter
/// types without directly depending on implementation crates.
pub fn public_surface_smoke() {
    let _runtime = Runtime::new(RuntimeConfig::default());
    let _cancellation = CancellationToken::new();
    let _host = HostMemoryObserver;
    let _budget = RamBudget::new("downstream-ram", 4096, 512, 4096, 1024, Some(2048))
        .expect("valid downstream RAM fixture");
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
    }
}
