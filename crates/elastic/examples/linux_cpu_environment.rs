//! Print read-only Linux CPU affinity, cgroup quota and PSI observations.

use elastic::{LinuxCpuEnvironmentObserver, Observer};

fn main() {
    let (_, observations) = LinuxCpuEnvironmentObserver::new().observe();
    for observation in observations {
        println!("{observation}");
    }
}
