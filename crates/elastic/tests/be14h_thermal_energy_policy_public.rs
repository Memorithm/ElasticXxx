#![cfg(target_os = "linux")]

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use elastic::prelude::*;
use elastic::{ObservationEpoch, ResourceGeneration};

fn temp_fixture(name: &str) -> std::path::PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let unique = NEXT.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "elastic-public-be14h-policy-{name}-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&path).expect("temporary public BE14h policy fixture");
    path
}

fn write(path: &Path, value: &str) {
    std::fs::write(path, value).expect("write public BE14h policy fixture");
}

fn policy_spec() -> ResourceSpec {
    ResourceSpec::builder(
        ResourceClassId::CONFIGURATIONAL,
        LogicalResourceId::new("public-be14h-policy").unwrap(),
    )
    .allow(DimensionId::ENERGY)
    .admit(AdmissibleTransition::new(
        TransitionMechanism::Reinterpret,
        DimensionId::ENERGY,
    ))
    .require_capability(CapabilityRequirement::new(
        TransitionMechanism::Reinterpret,
        DimensionId::ENERGY,
    ))
    .observe(ObservationSignalId::THERMAL_MARGIN)
    .observe(ObservationSignalId::ENERGY_RATE)
    .build()
    .unwrap()
}

#[test]
fn public_observers_feed_source_bound_thermal_energy_policy_without_actuation() {
    let root = temp_fixture("eligible");
    let zone = root.join("thermal_zone0");
    std::fs::create_dir_all(&zone).unwrap();
    write(&zone.join("type"), "cpu-thermal\n");
    write(&zone.join("temp"), "50000\n");
    write(&zone.join("trip_point_0_type"), "critical\n");
    write(&zone.join("trip_point_0_temp"), "100000\n");
    let power = root.join("power1_input");
    write(&power, "25000000\n");

    let thermal = LinuxThermalMarginObserver::new(&zone);
    let energy = LinuxHwmonPowerObserver::new(&power);
    let thermal_source = thermal.source();
    let energy_source = energy.source();
    let mut observers = ObserverSet::new();
    observers.push(&thermal);
    observers.push(&energy);
    let (context, observed) = observers.observe();
    let now = Instant::now();
    let snapshot = ObservationSnapshot::new(now, observed);

    let policy = BooleanThermalEnergyPreplannerV1::new(
        policy_spec(),
        TransitionMechanism::Reinterpret,
        DimensionId::ENERGY,
        10.0,
        30.0,
        thermal_source,
        energy_source,
        Duration::from_secs(1),
    )
    .unwrap();
    let report = policy
        .evaluate(
            &context,
            &snapshot,
            now,
            ObservationEpoch::new(11),
            ResourceGeneration::new(4),
        )
        .unwrap();

    assert_eq!(report.status, BooleanThermalEnergyStatusV1::Eligible);
    assert_eq!(report.evidence.thermal_truth, "true");
    assert_eq!(report.evidence.energy_truth, "true");
    assert_eq!(report.evidence.combined_truth, "true");
    let trace = DecisionTrace::from_bounded_json(report.evidence.decision_trace_json.as_bytes())
        .expect("public BE14h trace decodes strictly");
    assert!(trace.selected().is_some());
    assert_eq!(trace.observation_epoch(), ObservationEpoch::new(11));
    assert_eq!(trace.resource_generation(), ResourceGeneration::new(4));

    std::fs::remove_dir_all(root).unwrap();
}
