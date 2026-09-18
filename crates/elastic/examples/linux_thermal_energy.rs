//! Read one Linux thermal margin and one direct hwmon power channel.
//!
//! Usage:
//! `cargo run -p elastic --example linux_thermal_energy -- /sys/class/thermal/thermal_zoneN /sys/class/hwmon/hwmonN/power1_input`

use std::process::ExitCode;

use elastic::{LinuxHwmonPowerObserver, LinuxThermalMarginObserver, Observer};

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let Some(thermal_zone) = args.next() else {
        eprintln!("missing thermal-zone directory argument");
        return ExitCode::from(2);
    };
    let Some(power_input) = args.next() else {
        eprintln!("missing direct hwmon power*_input argument");
        return ExitCode::from(2);
    };
    if args.next().is_some() {
        eprintln!("expected exactly two arguments");
        return ExitCode::from(2);
    }

    let thermal = LinuxThermalMarginObserver::new(thermal_zone);
    let power = LinuxHwmonPowerObserver::new(power_input);
    let (_, thermal_observations) = thermal.observe();
    let (_, power_observations) = power.observe();

    for observation in thermal_observations.iter().chain(power_observations.iter()) {
        println!("{observation}");
    }

    if thermal_observations
        .iter()
        .all(|observation| observation.is_valid())
        && power_observations
            .iter()
            .all(|observation| observation.is_valid())
    {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
