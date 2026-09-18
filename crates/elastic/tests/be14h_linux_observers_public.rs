#![cfg(target_os = "linux")]

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use elastic::{
    LinuxHwmonPowerObserver, LinuxThermalMarginObserver, ObservationSignalId, Observer,
    ENERGY_RATE_SOURCE_UNIT, THERMAL_MARGIN_SOURCE_UNIT,
};

fn temp_fixture(name: &str) -> std::path::PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let unique = NEXT.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "elastic-public-be14h-{name}-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&path).expect("temporary public BE14h fixture");
    path
}

fn write(path: &Path, value: &str) {
    std::fs::write(path, value).expect("write public BE14h fixture");
}

#[test]
fn public_facade_exposes_real_signal_units_and_fail_closed_observers() {
    let root = temp_fixture("observers");
    let zone = root.join("thermal_zone0");
    std::fs::create_dir_all(&zone).unwrap();
    write(&zone.join("type"), "cpu-thermal\n");
    write(&zone.join("temp"), "50000\n");
    write(&zone.join("trip_point_0_type"), "critical\n");
    write(&zone.join("trip_point_0_temp"), "100000\n");
    let power = root.join("power1_input");
    write(&power, "25000000\n");

    let (thermal_context, thermal_observations) = LinuxThermalMarginObserver::new(&zone).observe();
    let (power_context, power_observations) = LinuxHwmonPowerObserver::new(&power).observe();

    assert_eq!(
        thermal_context.get(ObservationSignalId::THERMAL_MARGIN),
        Some(50.0)
    );
    assert_eq!(
        power_context.get(ObservationSignalId::ENERGY_RATE),
        Some(25.0)
    );
    assert!(thermal_observations[0].is_valid());
    assert!(power_observations[0].is_valid());
    assert_eq!(THERMAL_MARGIN_SOURCE_UNIT, "degrees-celsius");
    assert_eq!(ENERGY_RATE_SOURCE_UNIT, "watts");

    std::fs::remove_file(&power).unwrap();
    let (missing_context, missing) = LinuxHwmonPowerObserver::new(&power).observe();
    assert_eq!(missing_context.get(ObservationSignalId::ENERGY_RATE), None);
    assert!(missing[0].is_unsupported());

    std::fs::remove_dir_all(root).unwrap();
}
