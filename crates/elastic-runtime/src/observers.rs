//! Concrete observation providers for the operational runtime.
//!
//! Resource observers adapt the existing trusted in-process adapters. Host
//! telemetry is deliberately isolated here rather than in `elastic-core` or
//! EIR. Linux memory discovery uses `/proc/meminfo`; unsupported platforms or
//! unavailable fields produce explicit unsupported observations.

use std::collections::BTreeSet;
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
}
