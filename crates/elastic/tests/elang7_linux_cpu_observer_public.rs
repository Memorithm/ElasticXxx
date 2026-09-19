#![cfg(target_os = "linux")]

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use elastic::prelude::*;

fn fixture() -> std::path::PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "elastic-elang7-cpu-public-{}-{id}",
        std::process::id()
    ));
    std::fs::create_dir_all(root.join("cgroupfs/edge.scope")).unwrap();
    root
}

fn write(path: &Path, value: &str) {
    std::fs::write(path, value).unwrap();
}

#[test]
fn public_facade_exposes_independent_affinity_quota_and_pressure_evidence() {
    let root = fixture();
    let status = root.join("status");
    let cgroup = root.join("cgroup");
    let cgroup_root = root.join("cgroupfs");
    let current = cgroup_root.join("edge.scope");
    let proc_pressure = root.join("system-pressure");
    write(&status, "Cpus_allowed_list:\t0-1,4\n");
    write(&cgroup, "0::/edge.scope\n");
    write(&current.join("cpu.max"), "150000 100000\n");
    write(
        &current.join("cpu.pressure"),
        "some avg10=5.00 avg60=2.00 avg300=1.00 total=10\n",
    );

    let observer =
        LinuxCpuEnvironmentObserver::with_paths(&status, &cgroup, &cgroup_root, &proc_pressure);
    let (context, observations) = observer.observe();

    assert_eq!(
        context.get(linux_cpu_affinity_allowed_cpus_signal()),
        Some(3.0)
    );
    assert_eq!(context.get(linux_cpu_quota_cores_signal()), Some(1.5));
    assert_eq!(context.get(linux_cpu_quota_unlimited_signal()), Some(0.0));
    assert_eq!(
        context.get(linux_cpu_pressure_some_avg10_signal()),
        Some(0.05)
    );
    assert_eq!(context.get(linux_cpu_pressure_full_avg10_signal()), None);
    assert!(observations
        .iter()
        .find(|item| item.signal() == &linux_cpu_pressure_full_avg10_signal())
        .unwrap()
        .is_unsupported());
    assert_eq!(LINUX_CPU_AFFINITY_SOURCE_UNIT, "logical-cpus");
    assert_eq!(LINUX_CPU_QUOTA_SOURCE_UNIT, "cpu-cores");
    assert_eq!(LINUX_CPU_PRESSURE_SOURCE_UNIT, "fraction");

    std::fs::remove_dir_all(root).unwrap();
}
