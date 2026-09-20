//! Concrete observation providers for the operational runtime.
//!
//! Resource observers adapt the existing trusted in-process adapters. Host
//! telemetry is deliberately isolated here rather than in `elastic-core` or
//! EIR. Linux memory discovery uses `/proc/meminfo`; unsupported platforms or
//! unavailable fields produce explicit unsupported observations.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Instant;

use elastic_adapters::{ConcurrencyPermits, RamBudget};
use elastic_core::resource::ObservationSignalId;
use elastic_eir::PlanningContext;

use crate::observation::{Observation, ObservationSource, Observer};

fn signal(id: &str) -> ObservationSignalId {
    ObservationSignalId::custom(id).expect("runtime observation signal identifiers are valid")
}

#[must_use]
pub fn ram_configured_min_bytes_signal() -> ObservationSignalId {
    signal("ram-configured-min-bytes")
}

#[must_use]
pub fn ram_configured_max_bytes_signal() -> ObservationSignalId {
    signal("ram-configured-max-bytes")
}

#[must_use]
pub fn ram_in_use_bytes_signal() -> ObservationSignalId {
    signal("ram-in-use-bytes")
}

#[must_use]
pub fn concurrency_capacity_signal() -> ObservationSignalId {
    signal("concurrency-capacity")
}

#[must_use]
pub fn concurrency_width_signal() -> ObservationSignalId {
    signal("concurrency-width")
}

#[must_use]
pub fn active_permits_signal() -> ObservationSignalId {
    signal("active-permits")
}

#[must_use]
pub fn host_memory_total_bytes_signal() -> ObservationSignalId {
    signal("host-memory-total-bytes")
}

#[must_use]
pub fn host_memory_available_bytes_signal() -> ObservationSignalId {
    signal("host-memory-available-bytes")
}

#[must_use]
pub fn host_memory_used_bytes_signal() -> ObservationSignalId {
    signal("host-memory-used-bytes")
}

#[must_use]
pub fn host_memory_utilization_signal() -> ObservationSignalId {
    signal("host-memory-utilization")
}

#[must_use]
pub fn runtime_uptime_seconds_signal() -> ObservationSignalId {
    signal("runtime-uptime-seconds")
}

/// Number of logical CPUs currently permitted by Linux process affinity.
#[must_use]
pub fn linux_cpu_affinity_allowed_cpus_signal() -> ObservationSignalId {
    signal("linux-cpu-affinity-allowed-cpus")
}

/// Finite cgroup-v2 CPU quota expressed as logical CPU equivalents.
#[must_use]
pub fn linux_cpu_quota_cores_signal() -> ObservationSignalId {
    signal("linux-cpu-quota-cores")
}

/// Whether the current cgroup-v2 CPU quota is explicitly unlimited (`1`) or finite (`0`).
#[must_use]
pub fn linux_cpu_quota_unlimited_signal() -> ObservationSignalId {
    signal("linux-cpu-quota-unlimited")
}

/// Linux PSI CPU `some avg10` converted from percent to fraction.
#[must_use]
pub fn linux_cpu_pressure_some_avg10_signal() -> ObservationSignalId {
    signal("linux-cpu-pressure-some-avg10")
}

/// Linux PSI CPU `full avg10` converted from percent to fraction when exposed.
#[must_use]
pub fn linux_cpu_pressure_full_avg10_signal() -> ObservationSignalId {
    signal("linux-cpu-pressure-full-avg10")
}

/// Unit for CPU affinity count observations.
pub const LINUX_CPU_AFFINITY_SOURCE_UNIT: &str = "logical-cpus";
/// Unit for finite CPU quota observations.
pub const LINUX_CPU_QUOTA_SOURCE_UNIT: &str = "cpu-cores";
/// Unit for the quota-unlimited discriminator.
pub const LINUX_CPU_QUOTA_UNLIMITED_SOURCE_UNIT: &str = "boolean";
/// Unit for PSI averages after conversion from kernel percent to fraction.
pub const LINUX_CPU_PRESSURE_SOURCE_UNIT: &str = "fraction";

/// Unit emitted for [`ObservationSignalId::THERMAL_MARGIN`].
///
/// Linux thermal sysfs exposes temperatures in millidegrees Celsius; the
/// observer converts both the current temperature and the lowest critical trip
/// point to degrees Celsius before computing `critical - current`.
pub const THERMAL_MARGIN_SOURCE_UNIT: &str = "degrees-celsius";

/// Unit emitted for [`ObservationSignalId::ENERGY_RATE`].
///
/// Linux hwmon `power*_input` channels are instantaneous power readings in
/// microwatts; the observer converts them to watts. No voltage×current value is
/// synthesized when a direct power channel is absent.
pub const ENERGY_RATE_SOURCE_UNIT: &str = "watts";

/// Linux thermal-zone provider for the built-in `thermal-margin` signal.
///
/// The configured path must be one thermal-zone directory containing `temp`,
/// `type`, and at least one `trip_point_*_type=critical` with a matching
/// `trip_point_*_temp`. Missing or malformed sysfs data produces an explicit
/// unsupported observation rather than a fabricated numeric value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinuxThermalMarginObserver {
    thermal_zone: PathBuf,
}

impl LinuxThermalMarginObserver {
    /// Bind the observer to one explicit Linux thermal-zone directory.
    #[must_use]
    pub fn new(thermal_zone: impl Into<PathBuf>) -> Self {
        Self {
            thermal_zone: thermal_zone.into(),
        }
    }

    /// Configured thermal-zone directory.
    #[must_use]
    pub fn thermal_zone(&self) -> &Path {
        &self.thermal_zone
    }

    /// Stable observation source identity derived only from the configured path.
    #[must_use]
    pub fn source(&self) -> ObservationSource {
        ObservationSource::host(format!("linux:thermal:{}", self.thermal_zone.display()))
    }
}

impl Observer for LinuxThermalMarginObserver {
    fn observe(&self) -> (PlanningContext, Vec<Observation>) {
        let now = Instant::now();
        let source = self.source();
        match thermal_margin_celsius(&self.thermal_zone) {
            Ok(margin) => {
                let observation = Observation::from_source(
                    source,
                    ObservationSignalId::THERMAL_MARGIN,
                    margin,
                    now,
                );
                (
                    PlanningContext::new().observe(ObservationSignalId::THERMAL_MARGIN, margin),
                    vec![observation],
                )
            }
            Err(error) => (
                PlanningContext::new(),
                vec![Observation::unsupported_from_source(
                    source,
                    ObservationSignalId::THERMAL_MARGIN,
                    now,
                    error,
                )],
            ),
        }
    }
}

/// Linux hwmon direct-power provider for the built-in `energy-rate` signal.
///
/// The configured file must be a direct `power*_input` channel whose kernel
/// ABI value is microwatts. The constructor intentionally accepts an explicit
/// path instead of guessing a board/rail sensor. Missing or malformed data is
/// represented as unsupported telemetry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinuxHwmonPowerObserver {
    power_input: PathBuf,
}

impl LinuxHwmonPowerObserver {
    /// Bind the observer to one explicit Linux hwmon `power*_input` file.
    #[must_use]
    pub fn new(power_input: impl Into<PathBuf>) -> Self {
        Self {
            power_input: power_input.into(),
        }
    }

    /// Configured direct power-input file.
    #[must_use]
    pub fn power_input(&self) -> &Path {
        &self.power_input
    }

    /// Stable observation source identity derived only from the configured path.
    #[must_use]
    pub fn source(&self) -> ObservationSource {
        ObservationSource::host(format!("linux:hwmon:{}", self.power_input.display()))
    }
}

impl Observer for LinuxHwmonPowerObserver {
    fn observe(&self) -> (PlanningContext, Vec<Observation>) {
        let now = Instant::now();
        let source = self.source();
        match direct_power_watts(&self.power_input) {
            Ok(watts) => {
                let observation =
                    Observation::from_source(source, ObservationSignalId::ENERGY_RATE, watts, now);
                (
                    PlanningContext::new().observe(ObservationSignalId::ENERGY_RATE, watts),
                    vec![observation],
                )
            }
            Err(error) => (
                PlanningContext::new(),
                vec![Observation::unsupported_from_source(
                    source,
                    ObservationSignalId::ENERGY_RATE,
                    now,
                    error,
                )],
            ),
        }
    }
}

#[cfg(target_os = "linux")]
fn thermal_margin_celsius(zone: &Path) -> Result<f64, String> {
    use std::fs;

    let zone_type = fs::read_to_string(zone.join("type"))
        .map_err(|error| format!("cannot read thermal zone type: {error}"))?
        .trim()
        .to_owned();
    if zone_type.is_empty() {
        return Err("thermal zone type is empty".to_owned());
    }
    let current_millicelsius = read_sysfs_i64(&zone.join("temp"), "thermal temperature")?;

    let entries = fs::read_dir(zone)
        .map_err(|error| format!("cannot enumerate thermal trip points: {error}"))?;
    let mut critical_millicelsius: Option<i64> = None;
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("cannot read thermal directory entry: {error}"))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(index) = name
            .strip_prefix("trip_point_")
            .and_then(|rest| rest.strip_suffix("_type"))
        else {
            continue;
        };
        if index.is_empty() || !index.bytes().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        let trip_type = fs::read_to_string(entry.path())
            .map_err(|error| format!("cannot read thermal trip type {index}: {error}"))?;
        if !trip_type.trim().eq_ignore_ascii_case("critical") {
            continue;
        }
        let temperature = read_sysfs_i64(
            &zone.join(format!("trip_point_{index}_temp")),
            "critical thermal trip temperature",
        )?;
        critical_millicelsius =
            Some(critical_millicelsius.map_or(temperature, |current| current.min(temperature)));
    }

    let critical_millicelsius = critical_millicelsius
        .ok_or_else(|| "thermal zone exposes no critical trip point".to_owned())?;
    let delta = i128::from(critical_millicelsius) - i128::from(current_millicelsius);
    let margin = delta as f64 / 1000.0;
    if !margin.is_finite() {
        return Err("thermal margin is non-finite".to_owned());
    }
    Ok(margin)
}

#[cfg(not(target_os = "linux"))]
fn thermal_margin_celsius(_zone: &Path) -> Result<f64, String> {
    Err("Linux thermal sysfs is unavailable on this platform".to_owned())
}

#[cfg(target_os = "linux")]
fn direct_power_watts(power_input: &Path) -> Result<f64, String> {
    let file_name = power_input
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "hwmon power input path has no UTF-8 file name".to_owned())?;
    if !(file_name.starts_with("power") && file_name.ends_with("_input")) {
        return Err("hwmon power observer requires a direct power*_input channel".to_owned());
    }
    let microwatts = read_sysfs_u64(power_input, "hwmon power input")?;
    if microwatts > (1_u64 << 53) {
        return Err("hwmon power input exceeds exact f64 integer range".to_owned());
    }
    let watts = microwatts as f64 / 1_000_000.0;
    if !watts.is_finite() {
        return Err("hwmon power input converts to a non-finite value".to_owned());
    }
    Ok(watts)
}

#[cfg(not(target_os = "linux"))]
fn direct_power_watts(_power_input: &Path) -> Result<f64, String> {
    Err("Linux hwmon power telemetry is unavailable on this platform".to_owned())
}

#[cfg(target_os = "linux")]
fn read_sysfs_i64(path: &Path, label: &str) -> Result<i64, String> {
    let raw =
        std::fs::read_to_string(path).map_err(|error| format!("cannot read {label}: {error}"))?;
    raw.trim()
        .parse::<i64>()
        .map_err(|_| format!("{label} is not a valid integer"))
}

#[cfg(target_os = "linux")]
fn read_sysfs_u64(path: &Path, label: &str) -> Result<u64, String> {
    let raw =
        std::fs::read_to_string(path).map_err(|error| format!("cannot read {label}: {error}"))?;
    raw.trim()
        .parse::<u64>()
        .map_err(|_| format!("{label} is not a valid non-negative integer"))
}

/// Read-only Linux CPU environment observer for embedded/edge qualification.
///
/// The provider independently reports process CPU affinity, cgroup-v2 quota,
/// and CPU PSI pressure. Missing telemetry is emitted as an explicit
/// unsupported observation per signal; one missing source never fabricates a
/// value for another source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinuxCpuEnvironmentObserver {
    proc_status: PathBuf,
    proc_self_cgroup: PathBuf,
    cgroup_root: PathBuf,
    proc_pressure_cpu: PathBuf,
}

impl Default for LinuxCpuEnvironmentObserver {
    fn default() -> Self {
        Self {
            proc_status: PathBuf::from("/proc/self/status"),
            proc_self_cgroup: PathBuf::from("/proc/self/cgroup"),
            cgroup_root: PathBuf::from("/sys/fs/cgroup"),
            proc_pressure_cpu: PathBuf::from("/proc/pressure/cpu"),
        }
    }
}

impl LinuxCpuEnvironmentObserver {
    /// Construct the ordinary host observer using standard Linux procfs/sysfs paths.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct an observer over explicit filesystem paths for qualification/tests.
    #[must_use]
    pub fn with_paths(
        proc_status: impl Into<PathBuf>,
        proc_self_cgroup: impl Into<PathBuf>,
        cgroup_root: impl Into<PathBuf>,
        proc_pressure_cpu: impl Into<PathBuf>,
    ) -> Self {
        Self {
            proc_status: proc_status.into(),
            proc_self_cgroup: proc_self_cgroup.into(),
            cgroup_root: cgroup_root.into(),
            proc_pressure_cpu: proc_pressure_cpu.into(),
        }
    }

    /// Stable provider identity derived only from configured filesystem roots.
    #[must_use]
    pub fn source(&self) -> ObservationSource {
        ObservationSource::host(format!(
            "linux:cpu-environment:{}:{}:{}:{}",
            self.proc_status.display(),
            self.proc_self_cgroup.display(),
            self.cgroup_root.display(),
            self.proc_pressure_cpu.display()
        ))
    }
}

impl Observer for LinuxCpuEnvironmentObserver {
    fn observe(&self) -> (PlanningContext, Vec<Observation>) {
        let now = Instant::now();
        let source = self.source();
        let mut context = PlanningContext::new();
        let mut observations = Vec::with_capacity(5);

        push_result(
            &mut context,
            &mut observations,
            source.clone(),
            linux_cpu_affinity_allowed_cpus_signal(),
            linux_cpu_affinity_count(&self.proc_status),
            now,
        );

        match linux_current_cpu_quota(&self.proc_self_cgroup, &self.cgroup_root) {
            Ok(LinuxCpuQuota::Finite(cores)) => {
                push_result(
                    &mut context,
                    &mut observations,
                    source.clone(),
                    linux_cpu_quota_cores_signal(),
                    Ok(cores),
                    now,
                );
                push_result(
                    &mut context,
                    &mut observations,
                    source.clone(),
                    linux_cpu_quota_unlimited_signal(),
                    Ok(0.0),
                    now,
                );
            }
            Ok(LinuxCpuQuota::Unlimited) => {
                push_result(
                    &mut context,
                    &mut observations,
                    source.clone(),
                    linux_cpu_quota_cores_signal(),
                    Err("cgroup-v2 cpu.max explicitly declares unlimited quota".to_owned()),
                    now,
                );
                push_result(
                    &mut context,
                    &mut observations,
                    source.clone(),
                    linux_cpu_quota_unlimited_signal(),
                    Ok(1.0),
                    now,
                );
            }
            Err(error) => {
                for signal in [
                    linux_cpu_quota_cores_signal(),
                    linux_cpu_quota_unlimited_signal(),
                ] {
                    push_result(
                        &mut context,
                        &mut observations,
                        source.clone(),
                        signal,
                        Err(error.clone()),
                        now,
                    );
                }
            }
        }

        match linux_cpu_pressure_text(
            &self.proc_self_cgroup,
            &self.cgroup_root,
            &self.proc_pressure_cpu,
        ) {
            Ok(text) => {
                push_result(
                    &mut context,
                    &mut observations,
                    source.clone(),
                    linux_cpu_pressure_some_avg10_signal(),
                    parse_psi_avg10(&text, "some"),
                    now,
                );
                push_result(
                    &mut context,
                    &mut observations,
                    source,
                    linux_cpu_pressure_full_avg10_signal(),
                    parse_psi_avg10(&text, "full"),
                    now,
                );
            }
            Err(error) => {
                for signal in [
                    linux_cpu_pressure_some_avg10_signal(),
                    linux_cpu_pressure_full_avg10_signal(),
                ] {
                    push_result(
                        &mut context,
                        &mut observations,
                        source.clone(),
                        signal,
                        Err(error.clone()),
                        now,
                    );
                }
            }
        }

        (context, observations)
    }
}

fn push_result(
    context: &mut PlanningContext,
    observations: &mut Vec<Observation>,
    source: ObservationSource,
    signal: ObservationSignalId,
    value: Result<f64, String>,
    now: Instant,
) {
    match value {
        Ok(value) if value.is_finite() => {
            *context = context.clone().observe(signal.clone(), value);
            observations.push(Observation::from_source(source, signal, value, now));
        }
        Ok(_) => observations.push(Observation::unsupported_from_source(
            source,
            signal,
            now,
            "Linux CPU telemetry converted to a non-finite value",
        )),
        Err(error) => observations.push(Observation::unsupported_from_source(
            source, signal, now, error,
        )),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum LinuxCpuQuota {
    Finite(f64),
    Unlimited,
}

#[cfg(target_os = "linux")]
fn linux_cpu_affinity_count(proc_status: &Path) -> Result<f64, String> {
    let text = read_small_text(proc_status, "process status")?;
    let list = text
        .lines()
        .find_map(|line| line.strip_prefix("Cpus_allowed_list:"))
        .map(str::trim)
        .ok_or_else(|| "process status does not expose Cpus_allowed_list".to_owned())?;
    let count = parse_cpu_list_count(list)?;
    if count == 0 {
        return Err("process affinity allows zero CPUs".to_owned());
    }
    Ok(count as f64)
}

#[cfg(not(target_os = "linux"))]
fn linux_cpu_affinity_count(_proc_status: &Path) -> Result<f64, String> {
    Err("Linux process affinity telemetry is unavailable on this platform".to_owned())
}

#[cfg(target_os = "linux")]
fn linux_current_cpu_quota(
    proc_self_cgroup: &Path,
    cgroup_root: &Path,
) -> Result<LinuxCpuQuota, String> {
    let directory = resolve_current_cgroup_v2(proc_self_cgroup, cgroup_root)?;
    let text = read_small_text(&directory.join("cpu.max"), "cgroup-v2 cpu.max")?;
    parse_cpu_max(&text)
}

#[cfg(not(target_os = "linux"))]
fn linux_current_cpu_quota(
    _proc_self_cgroup: &Path,
    _cgroup_root: &Path,
) -> Result<LinuxCpuQuota, String> {
    Err("Linux cgroup CPU quota telemetry is unavailable on this platform".to_owned())
}

#[cfg(target_os = "linux")]
fn linux_cpu_pressure_text(
    proc_self_cgroup: &Path,
    cgroup_root: &Path,
    proc_pressure_cpu: &Path,
) -> Result<String, String> {
    if let Ok(directory) = resolve_current_cgroup_v2(proc_self_cgroup, cgroup_root) {
        let cgroup_pressure = directory.join("cpu.pressure");
        if cgroup_pressure.is_file() {
            return read_small_text(&cgroup_pressure, "cgroup-v2 CPU pressure");
        }
    }
    if proc_pressure_cpu.is_file() {
        return read_small_text(proc_pressure_cpu, "system CPU pressure");
    }
    Err("neither current cgroup cpu.pressure nor /proc/pressure/cpu is available".to_owned())
}

#[cfg(not(target_os = "linux"))]
fn linux_cpu_pressure_text(
    _proc_self_cgroup: &Path,
    _cgroup_root: &Path,
    _proc_pressure_cpu: &Path,
) -> Result<String, String> {
    Err("Linux CPU pressure telemetry is unavailable on this platform".to_owned())
}

#[cfg(target_os = "linux")]
fn resolve_current_cgroup_v2(
    proc_self_cgroup: &Path,
    cgroup_root: &Path,
) -> Result<PathBuf, String> {
    let text = read_small_text(proc_self_cgroup, "process cgroup membership")?;
    let relative = text
        .lines()
        .find_map(|line| {
            let mut parts = line.splitn(3, ':');
            match (parts.next(), parts.next(), parts.next()) {
                (Some("0"), Some(""), Some(path)) => Some(path),
                _ => None,
            }
        })
        .ok_or_else(|| "process does not expose a cgroup-v2 unified membership".to_owned())?;
    let relative = relative.trim_start_matches('/');
    Ok(if relative.is_empty() {
        cgroup_root.to_path_buf()
    } else {
        cgroup_root.join(relative)
    })
}

#[cfg(target_os = "linux")]
fn read_small_text(path: &Path, label: &str) -> Result<String, String> {
    const MAX_BYTES: u64 = 64 * 1024;
    let file = std::fs::File::open(path)
        .map_err(|error| format!("cannot open {label} {}: {error}", path.display()))?;
    let mut text = String::new();
    file.take(MAX_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|error| format!("cannot read {label} {}: {error}", path.display()))?;
    if text.len() as u64 > MAX_BYTES {
        return Err(format!("{label} exceeds {MAX_BYTES} byte telemetry bound"));
    }
    Ok(text)
}

#[cfg(target_os = "linux")]
fn parse_cpu_list_count(value: &str) -> Result<usize, String> {
    const MAX_CPU_INDEX: u32 = 1_048_575;
    let mut cpus = BTreeSet::new();
    if value.is_empty() {
        return Err("Cpus_allowed_list is empty".to_owned());
    }
    for segment in value.split(',') {
        let segment = segment.trim();
        if segment.is_empty() {
            return Err("CPU affinity list contains an empty segment".to_owned());
        }
        let (start, end) = if let Some((start, end)) = segment.split_once('-') {
            let start = start
                .parse::<u32>()
                .map_err(|_| format!("invalid CPU affinity index {start:?}"))?;
            let end = end
                .parse::<u32>()
                .map_err(|_| format!("invalid CPU affinity index {end:?}"))?;
            if start > end {
                return Err(format!("CPU affinity range {segment:?} is descending"));
            }
            (start, end)
        } else {
            let cpu = segment
                .parse::<u32>()
                .map_err(|_| format!("invalid CPU affinity index {segment:?}"))?;
            (cpu, cpu)
        };
        if end > MAX_CPU_INDEX {
            return Err(format!(
                "CPU affinity index {end} exceeds supported bound {MAX_CPU_INDEX}"
            ));
        }
        for cpu in start..=end {
            cpus.insert(cpu);
        }
    }
    Ok(cpus.len())
}

#[cfg(target_os = "linux")]
fn parse_cpu_max(value: &str) -> Result<LinuxCpuQuota, String> {
    let mut fields = value.split_whitespace();
    let quota = fields.next().ok_or_else(|| "cpu.max is empty".to_owned())?;
    let period = fields
        .next()
        .ok_or_else(|| "cpu.max is missing period".to_owned())?;
    if fields.next().is_some() {
        return Err("cpu.max contains unexpected trailing fields".to_owned());
    }
    let period = period
        .parse::<u64>()
        .map_err(|_| "cpu.max period is not an unsigned integer".to_owned())?;
    if period == 0 {
        return Err("cpu.max period must be non-zero".to_owned());
    }
    if quota == "max" {
        return Ok(LinuxCpuQuota::Unlimited);
    }
    let quota = quota
        .parse::<u64>()
        .map_err(|_| "cpu.max quota is neither `max` nor an unsigned integer".to_owned())?;
    let cores = quota as f64 / period as f64;
    if !cores.is_finite() || cores < 0.0 {
        return Err("cpu.max quota converts to an invalid CPU-core value".to_owned());
    }
    Ok(LinuxCpuQuota::Finite(cores))
}

#[cfg(target_os = "linux")]
fn parse_psi_avg10(value: &str, class: &str) -> Result<f64, String> {
    let line = value
        .lines()
        .find(|line| line.split_whitespace().next() == Some(class))
        .ok_or_else(|| format!("CPU PSI does not expose {class} pressure"))?;
    let avg = line
        .split_whitespace()
        .find_map(|field| field.strip_prefix("avg10="))
        .ok_or_else(|| format!("CPU PSI {class} pressure is missing avg10"))?;
    let percent = avg
        .parse::<f64>()
        .map_err(|_| format!("CPU PSI {class} avg10 is not numeric"))?;
    if !percent.is_finite() || !(0.0..=100.0).contains(&percent) {
        return Err(format!("CPU PSI {class} avg10 percent is outside [0,100]"));
    }
    Ok(percent / 100.0)
}

/// Observer over a live [`RamBudget`].
#[derive(Clone, Copy, Debug)]
pub struct RamBudgetObserver<'a> {
    budget: &'a RamBudget,
}

impl<'a> RamBudgetObserver<'a> {
    #[must_use]
    pub const fn new(budget: &'a RamBudget) -> Self {
        Self { budget }
    }
}

impl Observer for RamBudgetObserver<'_> {
    fn observe(&self) -> (PlanningContext, Vec<Observation>) {
        let now = Instant::now();
        let source = ObservationSource::Resource(self.budget.spec().resource_id().clone());
        let mut context = self.budget.observe();
        let mut observations = context
            .iter()
            .map(|(signal, value)| {
                Observation::from_source(source.clone(), signal.clone(), value, now)
            })
            .collect::<Vec<_>>();

        let (min, max) = self.budget.bounds();
        let extra = [
            (ram_configured_min_bytes_signal(), min as f64),
            (ram_configured_max_bytes_signal(), max as f64),
            (ram_in_use_bytes_signal(), self.budget.in_use() as f64),
        ];
        for (signal, value) in extra {
            context = context.observe(signal.clone(), value);
            observations.push(Observation::from_source(source.clone(), signal, value, now));
        }

        (context, observations)
    }
}

/// Observer over a live [`ConcurrencyPermits`] ledger.
#[derive(Clone, Copy, Debug)]
pub struct ConcurrencyPermitsObserver<'a> {
    permits: &'a ConcurrencyPermits,
}

impl<'a> ConcurrencyPermitsObserver<'a> {
    #[must_use]
    pub const fn new(permits: &'a ConcurrencyPermits) -> Self {
        Self { permits }
    }
}

impl Observer for ConcurrencyPermitsObserver<'_> {
    fn observe(&self) -> (PlanningContext, Vec<Observation>) {
        let now = Instant::now();
        let source = ObservationSource::Resource(self.permits.spec().resource_id().clone());
        let mut context = self.permits.observe();
        let mut observations = context
            .iter()
            .map(|(signal, value)| {
                Observation::from_source(source.clone(), signal.clone(), value, now)
            })
            .collect::<Vec<_>>();

        let extra = [
            (
                concurrency_capacity_signal(),
                self.permits.max_width() as f64,
            ),
            (concurrency_width_signal(), self.permits.width() as f64),
            (active_permits_signal(), self.permits.active() as f64),
        ];
        for (signal, value) in extra {
            context = context.observe(signal.clone(), value);
            observations.push(Observation::from_source(source.clone(), signal, value, now));
        }

        (context, observations)
    }
}

/// Host memory provider.
///
/// Linux reads `/proc/meminfo`. Other platforms expose the same signals as
/// unsupported rather than manufacturing zero-valued telemetry.
#[derive(Clone, Copy, Debug, Default)]
pub struct HostMemoryObserver;

impl Observer for HostMemoryObserver {
    fn observe(&self) -> (PlanningContext, Vec<Observation>) {
        let now = Instant::now();
        let observations = host_memory_observations(now);
        let mut context = PlanningContext::new();
        for observation in &observations {
            if observation.is_valid() {
                context = context.observe(observation.signal.clone(), observation.value);
            }
        }
        (context, observations)
    }
}

/// Monotonic runtime timing provider.
#[derive(Clone, Copy, Debug)]
pub struct RuntimeTimingObserver {
    started: Instant,
}

impl RuntimeTimingObserver {
    #[must_use]
    pub const fn new(started: Instant) -> Self {
        Self { started }
    }
}

impl Default for RuntimeTimingObserver {
    fn default() -> Self {
        Self::new(Instant::now())
    }
}

impl Observer for RuntimeTimingObserver {
    fn observe(&self) -> (PlanningContext, Vec<Observation>) {
        let now = Instant::now();
        let signal = runtime_uptime_seconds_signal();
        let source = ObservationSource::runtime("elastic-runtime");
        let value = now.duration_since(self.started).as_secs_f64();
        let observation = Observation::from_source(source, signal.clone(), value, now);
        (
            PlanningContext::new().observe(signal, value),
            vec![observation],
        )
    }
}

/// Deterministic ordered composition of several observation providers.
///
/// [`PlanningContext`] is keyed only by signal identity, not by source. When
/// several providers publish the same planner-facing signal, the first
/// registered provider therefore keeps authority for that signal. All emitted
/// observations are still retained for auditability, so the disagreement is
/// visible rather than silently overwriting the planner input.
pub struct ObserverSet<'a> {
    observers: Vec<&'a dyn Observer>,
}

impl<'a> ObserverSet<'a> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            observers: Vec::new(),
        }
    }

    pub fn push(&mut self, observer: &'a dyn Observer) {
        self.observers.push(observer);
    }
}

impl<'a> Default for ObserverSet<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl Observer for ObserverSet<'_> {
    fn observe(&self) -> (PlanningContext, Vec<Observation>) {
        let mut context = PlanningContext::new();
        let mut observations = Vec::new();
        let mut claimed_signals = BTreeSet::new();

        for observer in &self.observers {
            let (provider_context, mut provider_observations) = observer.observe();
            for (signal, value) in provider_context.iter() {
                if claimed_signals.insert(signal.clone()) {
                    context = context.observe(signal.clone(), value);
                }
            }
            observations.append(&mut provider_observations);
        }

        (context, observations)
    }
}

#[cfg(target_os = "linux")]
fn host_memory_observations(now: Instant) -> Vec<Observation> {
    use std::fs;

    let source = ObservationSource::host("linux:/proc/meminfo");
    let content = match fs::read_to_string("/proc/meminfo") {
        Ok(content) => content,
        Err(error) => {
            return unsupported_host_memory(
                source,
                now,
                format!("cannot read /proc/meminfo: {error}"),
            );
        }
    };

    observations_from_meminfo(&content, source, now)
}

#[cfg(not(target_os = "linux"))]
fn host_memory_observations(now: Instant) -> Vec<Observation> {
    unsupported_host_memory(
        ObservationSource::host("unsupported-platform"),
        now,
        "host memory telemetry is not implemented for this platform",
    )
}

fn unsupported_host_memory(
    source: ObservationSource,
    now: Instant,
    reason: impl Into<String>,
) -> Vec<Observation> {
    let reason = reason.into();
    [
        host_memory_total_bytes_signal(),
        host_memory_available_bytes_signal(),
        host_memory_used_bytes_signal(),
        host_memory_utilization_signal(),
    ]
    .into_iter()
    .map(|signal| Observation::unsupported_from_source(source.clone(), signal, now, reason.clone()))
    .collect()
}

#[cfg(target_os = "linux")]
fn observations_from_meminfo(
    content: &str,
    source: ObservationSource,
    now: Instant,
) -> Vec<Observation> {
    let total = meminfo_bytes(content, "MemTotal:");
    let available = meminfo_bytes(content, "MemAvailable:");
    let used = total.zip(available).and_then(|(total, available)| {
        total
            .checked_sub(available)
            .map(|used| (total, available, used))
    });

    let mut observations = Vec::with_capacity(4);
    observations.push(optional_memory_observation(
        source.clone(),
        host_memory_total_bytes_signal(),
        total,
        now,
        "MemTotal is unavailable or invalid",
    ));
    observations.push(optional_memory_observation(
        source.clone(),
        host_memory_available_bytes_signal(),
        available,
        now,
        "MemAvailable is unavailable or invalid",
    ));

    match used {
        Some((total, _available, used)) => {
            observations.push(Observation::from_source(
                source.clone(),
                host_memory_used_bytes_signal(),
                used as f64,
                now,
            ));
            if total == 0 {
                observations.push(Observation::unsupported_from_source(
                    source,
                    host_memory_utilization_signal(),
                    now,
                    "MemTotal is zero",
                ));
            } else {
                observations.push(Observation::from_source(
                    source,
                    host_memory_utilization_signal(),
                    used as f64 / total as f64,
                    now,
                ));
            }
        }
        None => {
            observations.push(Observation::unsupported_from_source(
                source.clone(),
                host_memory_used_bytes_signal(),
                now,
                "used memory requires valid MemTotal and MemAvailable",
            ));
            observations.push(Observation::unsupported_from_source(
                source,
                host_memory_utilization_signal(),
                now,
                "utilization requires valid MemTotal and MemAvailable",
            ));
        }
    }

    observations
}

#[cfg(target_os = "linux")]
fn optional_memory_observation(
    source: ObservationSource,
    signal: ObservationSignalId,
    value: Option<u64>,
    now: Instant,
    reason: &str,
) -> Observation {
    match value {
        Some(value) => Observation::from_source(source, signal, value as f64, now),
        None => Observation::unsupported_from_source(source, signal, now, reason),
    }
}

#[cfg(target_os = "linux")]
fn meminfo_bytes(content: &str, key: &str) -> Option<u64> {
    let line = content.lines().find(|line| line.starts_with(key))?;
    let mut fields = line[key.len()..].split_whitespace();
    let kib = fields.next()?.parse::<u64>().ok()?;
    let unit = fields.next()?;
    if unit != "kB" {
        return None;
    }
    kib.checked_mul(1024)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    fn temp_fixture(name: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};

        static NEXT: AtomicU64 = AtomicU64::new(0);
        let unique = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "elastic-be14h-{name}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("temporary BE14h fixture directory");
        path
    }

    #[cfg(target_os = "linux")]
    fn write(path: &Path, value: &str) {
        std::fs::write(path, value).expect("write BE14h sysfs fixture");
    }

    #[test]
    fn ram_observer_exposes_budget_and_usage() {
        let mut budget =
            RamBudget::new("ram", 4096, 512, 4096, 1024, Some(2048)).expect("valid RAM budget");
        budget.record_use(256).expect("usage fits budget");
        let observer = RamBudgetObserver::new(&budget);

        let (context, observations) = observer.observe();

        assert_eq!(context.get(ram_configured_min_bytes_signal()), Some(512.0));
        assert_eq!(context.get(ram_configured_max_bytes_signal()), Some(4096.0));
        assert_eq!(context.get(ram_in_use_bytes_signal()), Some(256.0));
        assert!(observations.iter().all(Observation::is_valid));
        assert!(observations.iter().all(|observation| matches!(
            observation.source(),
            ObservationSource::Resource(resource) if resource.as_str() == "ram"
        )));
    }

    #[test]
    fn concurrency_observer_exposes_capacity_width_and_active_permits() {
        let mut permits = ConcurrencyPermits::new("workers", 8, 4).expect("valid permits");
        permits.acquire().expect("first permit");
        permits.acquire().expect("second permit");
        let observer = ConcurrencyPermitsObserver::new(&permits);

        let (context, _) = observer.observe();

        assert_eq!(context.get(concurrency_capacity_signal()), Some(8.0));
        assert_eq!(context.get(concurrency_width_signal()), Some(4.0));
        assert_eq!(context.get(active_permits_signal()), Some(2.0));
        assert_eq!(context.get(ObservationSignalId::UTILIZATION), Some(0.5));
    }

    #[test]
    fn observer_set_merges_disjoint_provider_contexts() {
        let budget = RamBudget::new("ram", 4096, 512, 4096, 1024, None).expect("valid RAM budget");
        let timing = RuntimeTimingObserver::default();
        let ram = RamBudgetObserver::new(&budget);
        let mut set = ObserverSet::new();
        set.push(&ram);
        set.push(&timing);

        let (context, observations) = set.observe();

        assert!(context.get(runtime_uptime_seconds_signal()).is_some());
        assert!(context.get(ram_configured_max_bytes_signal()).is_some());
        assert!(!observations.is_empty());
    }

    #[test]
    fn observer_set_keeps_first_value_for_duplicate_planning_signal() {
        struct FixedObserver(f64);

        impl Observer for FixedObserver {
            fn observe(&self) -> (PlanningContext, Vec<Observation>) {
                let signal = ObservationSignalId::UTILIZATION;
                let now = Instant::now();
                (
                    PlanningContext::new().observe(signal.clone(), self.0),
                    vec![Observation::from_source(
                        ObservationSource::runtime(format!("fixed-{}", self.0)),
                        signal,
                        self.0,
                        now,
                    )],
                )
            }
        }

        let first = FixedObserver(0.25);
        let second = FixedObserver(0.75);
        let mut set = ObserverSet::new();
        set.push(&first);
        set.push(&second);

        let (context, observations) = set.observe();

        assert_eq!(context.get(ObservationSignalId::UTILIZATION), Some(0.25));
        assert_eq!(observations.len(), 2);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn meminfo_parser_uses_bytes_and_never_missing_field_zero() {
        let sample = "MemTotal:       1000 kB\nMemAvailable:    250 kB\n";
        let now = Instant::now();
        let observations =
            observations_from_meminfo(sample, ObservationSource::host("test:/proc/meminfo"), now);

        let total = observations
            .iter()
            .find(|observation| observation.signal == host_memory_total_bytes_signal())
            .expect("total observation");
        let used = observations
            .iter()
            .find(|observation| observation.signal == host_memory_used_bytes_signal())
            .expect("used observation");
        assert_eq!(total.value, 1_024_000.0);
        assert_eq!(used.value, 768_000.0);

        let missing = observations_from_meminfo(
            "MemTotal: 1000 kB\n",
            ObservationSource::host("test:/proc/meminfo"),
            now,
        );
        let available = missing
            .iter()
            .find(|observation| observation.signal == host_memory_available_bytes_signal())
            .expect("available observation");
        assert!(available.is_unsupported());
        assert!(available.value.is_nan());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn meminfo_parser_rejects_missing_or_unknown_units() {
        let now = Instant::now();
        for sample in [
            "MemTotal: 1000\nMemAvailable: 250 kB\n",
            "MemTotal: 1000 bytes\nMemAvailable: 250 kB\n",
        ] {
            let observations = observations_from_meminfo(
                sample,
                ObservationSource::host("test:/proc/meminfo"),
                now,
            );
            let total = observations
                .iter()
                .find(|observation| observation.signal == host_memory_total_bytes_signal())
                .expect("total observation");
            assert!(total.is_unsupported());
            assert!(total.value.is_nan());
        }
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn thermal_margin_observer_uses_lowest_critical_trip_and_degrees_celsius() {
        let zone = temp_fixture("thermal-valid");
        write(&zone.join("type"), "cpu-thermal\n");
        write(&zone.join("temp"), "50906\n");
        write(&zone.join("trip_point_0_type"), "passive\n");
        write(&zone.join("trip_point_0_temp"), "109000\n");
        write(&zone.join("trip_point_1_type"), "critical\n");
        write(&zone.join("trip_point_1_temp"), "114500\n");
        write(&zone.join("trip_point_2_type"), "critical\n");
        write(&zone.join("trip_point_2_temp"), "120000\n");

        let observer = LinuxThermalMarginObserver::new(&zone);
        let (context, observations) = observer.observe();

        let margin = context
            .get(ObservationSignalId::THERMAL_MARGIN)
            .expect("thermal margin enters planning context");
        assert!((margin - 63.594).abs() < 1e-9);
        assert_eq!(observations.len(), 1);
        assert!(observations[0].is_valid());
        assert_eq!(
            observations[0].signal(),
            &ObservationSignalId::THERMAL_MARGIN
        );
        assert_eq!(observations[0].source(), &observer.source());
        assert!(observations[0]
            .source()
            .to_string()
            .contains("linux:thermal:"));
        assert_eq!(THERMAL_MARGIN_SOURCE_UNIT, "degrees-celsius");

        std::fs::remove_dir_all(zone).expect("remove temporary thermal fixture");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn thermal_margin_observer_can_report_negative_margin_without_clamping() {
        let zone = temp_fixture("thermal-over-limit");
        write(&zone.join("type"), "cpu-thermal\n");
        write(&zone.join("temp"), "116000\n");
        write(&zone.join("trip_point_0_type"), "critical\n");
        write(&zone.join("trip_point_0_temp"), "114500\n");

        let (context, observations) = LinuxThermalMarginObserver::new(&zone).observe();

        assert_eq!(context.get(ObservationSignalId::THERMAL_MARGIN), Some(-1.5));
        assert!(observations[0].is_valid());
        std::fs::remove_dir_all(zone).expect("remove temporary thermal fixture");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn thermal_margin_observer_fails_closed_without_critical_trip() {
        let zone = temp_fixture("thermal-missing-critical");
        write(&zone.join("type"), "cpu-thermal\n");
        write(&zone.join("temp"), "50000\n");
        write(&zone.join("trip_point_0_type"), "passive\n");
        write(&zone.join("trip_point_0_temp"), "109000\n");

        let (context, observations) = LinuxThermalMarginObserver::new(&zone).observe();

        assert_eq!(context.get(ObservationSignalId::THERMAL_MARGIN), None);
        assert_eq!(observations.len(), 1);
        assert!(observations[0].is_unsupported());
        assert!(observations[0]
            .unsupported_reason()
            .is_some_and(|reason| reason.contains("no critical trip point")));
        std::fs::remove_dir_all(zone).expect("remove temporary thermal fixture");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn hwmon_power_observer_reads_direct_microwatts_as_watts() {
        let hwmon = temp_fixture("power-valid");
        let power = hwmon.join("power1_input");
        write(&power, "46792000\n");

        let observer = LinuxHwmonPowerObserver::new(&power);
        let (context, observations) = observer.observe();

        let watts = context
            .get(ObservationSignalId::ENERGY_RATE)
            .expect("energy rate enters planning context");
        assert!((watts - 46.792).abs() < 1e-12);
        assert_eq!(observations.len(), 1);
        assert!(observations[0].is_valid());
        assert_eq!(ENERGY_RATE_SOURCE_UNIT, "watts");
        std::fs::remove_dir_all(hwmon).expect("remove temporary power fixture");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn hwmon_power_observer_rejects_non_power_channel_and_invalid_values() {
        let hwmon = temp_fixture("power-invalid");
        let current = hwmon.join("curr1_input");
        write(&current, "1650\n");
        let (context, observations) = LinuxHwmonPowerObserver::new(&current).observe();
        assert_eq!(context.get(ObservationSignalId::ENERGY_RATE), None);
        assert!(observations[0].is_unsupported());
        assert!(observations[0]
            .unsupported_reason()
            .is_some_and(|reason| reason.contains("power*_input")));

        let power = hwmon.join("power1_input");
        write(&power, "not-a-number\n");
        let (context, observations) = LinuxHwmonPowerObserver::new(&power).observe();
        assert_eq!(context.get(ObservationSignalId::ENERGY_RATE), None);
        assert!(observations[0].is_unsupported());
        assert!(observations[0].value().is_nan());
        std::fs::remove_dir_all(hwmon).expect("remove temporary power fixture");
    }

    #[cfg(target_os = "linux")]
    fn cpu_fixture(name: &str) -> (PathBuf, PathBuf, PathBuf, PathBuf, PathBuf) {
        let root = temp_fixture(name);
        let status = root.join("status");
        let cgroup = root.join("cgroup");
        let cgroup_root = root.join("cgroupfs");
        let current = cgroup_root.join("user.slice/test.scope");
        let proc_pressure = root.join("pressure-cpu");
        std::fs::create_dir_all(&current).expect("create cgroup fixture");
        (root, status, cgroup, cgroup_root, proc_pressure)
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_cpu_observer_reads_affinity_finite_quota_and_cgroup_psi() {
        let (root, status, cgroup, cgroup_root, proc_pressure) = cpu_fixture("cpu-valid");
        write(&status, "Name:\ttest\nCpus_allowed_list:\t0-3,8,10-11\n");
        write(&cgroup, "0::/user.slice/test.scope\n");
        let current = cgroup_root.join("user.slice/test.scope");
        write(&current.join("cpu.max"), "250000 100000\n");
        write(
            &current.join("cpu.pressure"),
            "some avg10=12.50 avg60=6.00 avg300=1.00 total=100\nfull avg10=1.25 avg60=0.50 avg300=0.10 total=10\n",
        );

        let observer =
            LinuxCpuEnvironmentObserver::with_paths(&status, &cgroup, &cgroup_root, &proc_pressure);
        let (context, observations) = observer.observe();

        assert_eq!(
            context.get(linux_cpu_affinity_allowed_cpus_signal()),
            Some(7.0)
        );
        assert_eq!(context.get(linux_cpu_quota_cores_signal()), Some(2.5));
        assert_eq!(context.get(linux_cpu_quota_unlimited_signal()), Some(0.0));
        assert_eq!(
            context.get(linux_cpu_pressure_some_avg10_signal()),
            Some(0.125)
        );
        assert_eq!(
            context.get(linux_cpu_pressure_full_avg10_signal()),
            Some(0.0125)
        );
        assert_eq!(observations.len(), 5);
        assert!(observations.iter().all(Observation::is_valid));
        assert!(observations
            .iter()
            .all(|item| item.source() == &observer.source()));
        assert_eq!(LINUX_CPU_AFFINITY_SOURCE_UNIT, "logical-cpus");
        assert_eq!(LINUX_CPU_QUOTA_SOURCE_UNIT, "cpu-cores");
        assert_eq!(LINUX_CPU_QUOTA_UNLIMITED_SOURCE_UNIT, "boolean");
        assert_eq!(LINUX_CPU_PRESSURE_SOURCE_UNIT, "fraction");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_cpu_observer_preserves_unlimited_quota_and_missing_psi_as_distinct_evidence() {
        let (root, status, cgroup, cgroup_root, proc_pressure) = cpu_fixture("cpu-unlimited");
        write(&status, "Cpus_allowed_list:\t2-5\n");
        write(&cgroup, "0::/user.slice/test.scope\n");
        let current = cgroup_root.join("user.slice/test.scope");
        write(&current.join("cpu.max"), "max 100000\n");

        let (context, observations) =
            LinuxCpuEnvironmentObserver::with_paths(&status, &cgroup, &cgroup_root, &proc_pressure)
                .observe();

        assert_eq!(
            context.get(linux_cpu_affinity_allowed_cpus_signal()),
            Some(4.0)
        );
        assert_eq!(context.get(linux_cpu_quota_unlimited_signal()), Some(1.0));
        assert_eq!(context.get(linux_cpu_quota_cores_signal()), None);
        assert_eq!(context.get(linux_cpu_pressure_some_avg10_signal()), None);
        let quota = observations
            .iter()
            .find(|item| item.signal() == &linux_cpu_quota_cores_signal())
            .unwrap();
        assert!(quota.is_unsupported());
        assert!(quota
            .unsupported_reason()
            .is_some_and(|reason| reason.contains("unlimited quota")));
        let pressure = observations
            .iter()
            .find(|item| item.signal() == &linux_cpu_pressure_some_avg10_signal())
            .unwrap();
        assert!(pressure.is_unsupported());
        assert!(pressure.value().is_nan());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_cpu_observer_uses_system_psi_fallback_without_hiding_missing_full_line() {
        let (root, status, cgroup, cgroup_root, proc_pressure) = cpu_fixture("cpu-psi-fallback");
        write(&status, "Cpus_allowed_list:\t0\n");
        write(&cgroup, "0::/user.slice/test.scope\n");
        let current = cgroup_root.join("user.slice/test.scope");
        write(&current.join("cpu.max"), "100000 100000\n");
        write(
            &proc_pressure,
            "some avg10=25.00 avg60=10.00 avg300=2.00 total=100\n",
        );

        let (context, observations) =
            LinuxCpuEnvironmentObserver::with_paths(&status, &cgroup, &cgroup_root, &proc_pressure)
                .observe();

        assert_eq!(
            context.get(linux_cpu_pressure_some_avg10_signal()),
            Some(0.25)
        );
        assert_eq!(context.get(linux_cpu_pressure_full_avg10_signal()), None);
        assert!(observations
            .iter()
            .find(|item| item.signal() == &linux_cpu_pressure_full_avg10_signal())
            .unwrap()
            .is_unsupported());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn malformed_affinity_does_not_invalidate_independent_quota_signal() {
        let (root, status, cgroup, cgroup_root, proc_pressure) = cpu_fixture("cpu-bad-affinity");
        write(&status, "Cpus_allowed_list:\t7-3\n");
        write(&cgroup, "0::/user.slice/test.scope\n");
        let current = cgroup_root.join("user.slice/test.scope");
        write(&current.join("cpu.max"), "50000 100000\n");

        let (context, observations) =
            LinuxCpuEnvironmentObserver::with_paths(&status, &cgroup, &cgroup_root, &proc_pressure)
                .observe();

        assert_eq!(context.get(linux_cpu_affinity_allowed_cpus_signal()), None);
        assert_eq!(context.get(linux_cpu_quota_cores_signal()), Some(0.5));
        assert!(observations
            .iter()
            .find(|item| item.signal() == &linux_cpu_affinity_allowed_cpus_signal())
            .unwrap()
            .is_unsupported());
        std::fs::remove_dir_all(root).unwrap();
    }
}
