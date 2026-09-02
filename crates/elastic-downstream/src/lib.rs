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

/// Proof that a downstream crate can build and execute a real configured,
/// forecast-aware controller while depending only on `elastic`.
pub fn public_surface_smoke() {
    let config = OperatorConfig {
        version: OPERATOR_CONFIG_VERSION,
        resources: vec![ResourceConfig::Ram {
            id: "downstream-ram".into(),
            host_total: 4096,
            min: 512,
            max: 4096,
            initial: 1024,
            max_step: Some(2048),
        }],
        controllers: vec![ControllerConfig {
            resource: "downstream-ram".into(),
            planner: PlannerSelection::Headroom {
                headroom_fraction: 0.5,
                deadband_fraction: 0.0,
            },
            forecaster: ForecasterSelection::Ewma {
                alpha: 0.5,
                horizon_ms: 1_000,
            },
            cadence: CadenceConfig::OneShot,
            mode: ExecutionModeConfig::Apply,
        }],
    };
    let mut controller = config
        .build_controller("downstream-ram")
        .expect("valid public operator config should materialize");
    let result = controller
        .cycle()
        .expect("facade-only configured controller cycle should succeed");

    assert!(result.forecast.is_some());
    assert!(result.transaction.commit.is_some());
    assert_eq!(
        controller.actuator().state().unwrap(),
        ConfiguredResourceState::Ram {
            committed_bytes: 2048
        }
    );
}

/// Compile-time proof that durable runtime evidence is available through only
/// the public `elastic` facade.
pub fn public_evidence_surface_smoke() {
    let schema = EvidenceSchema::V1;
    let command = EvidenceCommand::Run;
    assert_eq!(schema.as_str(), EVIDENCE_SCHEMA_V1);
    assert_eq!(command.as_str(), "run");
    let _bounded_ingest_limit = MAX_EVIDENCE_BYTES;
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
        public_evidence_surface_smoke();
    }
}
