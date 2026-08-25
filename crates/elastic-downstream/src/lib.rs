//! Compile-time guard for the advertised single-dependency contract.
//!
//! This crate deliberately depends **only** on [`elastic`]. If the
//! `ElasticResource` expansion ever emits paths through a crate that is not a
//! direct dependency of downstream users (for example `elastic-core`), this
//! member stops compiling and `cargo check --workspace` fails, protecting the
//! facade contract.
//!
//! [`elastic`]: https://docs.rs/elastic

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_works_through_the_facade_alone() {
        let spec = DownstreamKv::resource_spec().unwrap();
        assert_eq!(spec.resource_id().as_str(), "downstream-kv");
        assert!(spec.admits(TransitionMechanism::Reencode, &DimensionId::REPRESENTATION));

        // EIR lowering is also part of the public facade surface.
        let document = lower(&spec).unwrap();
        assert!(document.resource("downstream-kv").unwrap().transitions()[0].capability_grounded());
    }
}
