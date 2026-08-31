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

/// Proof that a downstream crate can build and execute a real forecast-aware
/// controller while depending only on `elastic`.
pub fn public_surface_smoke() {
    let adapter = TransactionalRam::new("downstream-ram", 4096, 512, 4096, 1024, Some(2048))
        .expect("valid downstream RAM fixture");
    let resource = adapter
        .ir()
        .expect("downstream RAM EIR should be available");
    let observer = adapter.clone();
    let actuator = adapter.clone();
    let planner = HeadroomPlanner::new(0.5, 0.0).expect("valid headroom policy");
    let runtime = Runtime::new(RuntimeConfig {
        mode: RuntimeMode::Apply,
        dry_run: false,
        ..RuntimeConfig::default()
    });
    let forecaster = ForecasterSelection::Ewma {
        alpha: 0.5,
        horizon_ms: 1_000,
    }
    .build()
    .expect("valid configured EWMA forecaster");
    let mut controller = Controller::new(runtime, resource, planner, observer, actuator)
        .with_forecaster(forecaster);
    let result = controller
        .cycle()
        .expect("facade-only forecast controller cycle should succeed");

    assert!(result.forecast.is_some());
    assert!(result.transaction.commit.is_some());
    assert_eq!(adapter.committed().unwrap(), 2048);
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
