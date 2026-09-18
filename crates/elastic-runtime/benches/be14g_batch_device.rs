use std::{hint::black_box, time::Instant};

use elastic_runtime::{
    execute_guarded_batch_device_transaction, execute_unguarded_batch_device_transaction,
    BatchDeviceCandidateV1, BatchDeviceCapacitySampleV1, BatchDeviceCapacitySnapshotV1,
    BatchDevicePlacementBackendV1, BooleanBatchDevicePreplannerV1, CommittedBatchDeviceSelectionV1,
    GuardedBatchDeviceTransactionOutcomeV1, BATCH_DEVICE_CAPACITY_SOURCE_UNIT,
};

const DEFAULT_WARMUP: u64 = if cfg!(debug_assertions) { 10 } else { 10_000 };
const DEFAULT_ITERATIONS: u64 = if cfg!(debug_assertions) {
    1_000
} else {
    100_000
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BenchPath {
    UnguardedExactTransaction,
    GuardedPreplanTraceTransaction,
}

impl BenchPath {
    const ALL: [Self; 2] = [
        Self::UnguardedExactTransaction,
        Self::GuardedPreplanTraceTransaction,
    ];

    const fn as_str(self) -> &'static str {
        match self {
            Self::UnguardedExactTransaction => "unguarded_exact_transaction",
            Self::GuardedPreplanTraceTransaction => "guarded_preplan_trace_transaction",
        }
    }

    fn parse(value: &str) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == value)
            .ok_or_else(|| {
                format!(
                    "unknown --path {value:?}; expected one of {}",
                    Self::ALL
                        .iter()
                        .map(|path| path.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
    }
}

#[derive(Clone, Copy)]
struct Config {
    warmup: u64,
    iterations: u64,
    path: Option<BenchPath>,
}

fn next_u64(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<u64, String> {
    let value = args
        .next()
        .ok_or_else(|| format!("missing value for {flag}"))?;
    value
        .parse::<u64>()
        .map_err(|_| format!("invalid integer for {flag}: {value}"))
}

fn parse_config() -> Result<Config, String> {
    let mut config = Config {
        warmup: DEFAULT_WARMUP,
        iterations: DEFAULT_ITERATIONS,
        path: None,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--warmup" => config.warmup = next_u64(&mut args, &arg)?,
            "--iterations" => config.iterations = next_u64(&mut args, &arg)?,
            "--path" => {
                let value = args
                    .next()
                    .ok_or_else(|| "missing value for --path".to_owned())?;
                config.path = Some(BenchPath::parse(&value)?);
            }
            "--bench" => {}
            "--help" | "-h" => {
                println!("usage: be14g_batch_device [--warmup N] [--iterations N] [--path NAME]");
                println!(
                    "paths: {}",
                    BenchPath::ALL
                        .iter()
                        .map(|path| path.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                std::process::exit(0);
            }
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }
    if config.iterations == 0 {
        return Err("--iterations must be greater than zero".to_owned());
    }
    Ok(config)
}

fn selected(config: Config, path: BenchPath) -> bool {
    config.path.is_none_or(|selected| selected == path)
}

#[derive(Default)]
struct BenchBackend {
    validate_count: usize,
    act_count: usize,
    verify_count: usize,
    commit_count: usize,
}

impl BatchDevicePlacementBackendV1 for BenchBackend {
    fn validate_candidate(
        &mut self,
        candidate: &BatchDeviceCandidateV1,
        fresh_capacity: &BatchDeviceCapacitySnapshotV1,
    ) -> Result<(), String> {
        black_box(candidate.candidate_id());
        black_box(fresh_capacity.source_generation());
        self.validate_count += 1;
        Ok(())
    }

    fn actuate_candidate(&mut self, candidate: &BatchDeviceCandidateV1) -> Result<(), String> {
        black_box(candidate.placement_id());
        black_box(candidate.batch_size());
        self.act_count += 1;
        Ok(())
    }

    fn verify_candidate(&mut self, candidate: &BatchDeviceCandidateV1) -> Result<(), String> {
        black_box(candidate.preference_score());
        self.verify_count += 1;
        Ok(())
    }

    fn commit_candidate(&mut self, candidate: &BatchDeviceCandidateV1) -> Result<(), String> {
        black_box(candidate.candidate_id());
        self.commit_count += 1;
        Ok(())
    }

    fn rollback_candidate(
        &mut self,
        _candidate: &BatchDeviceCandidateV1,
        _reason: &str,
    ) -> Result<(), String> {
        Err("benchmark success fixture must not roll back".to_owned())
    }
}

struct Fixture {
    planner: BooleanBatchDevicePreplannerV1,
    exact_reference_candidate: BatchDeviceCandidateV1,
    capacity: BatchDeviceCapacitySnapshotV1,
    now: Instant,
}

fn fixture() -> Fixture {
    let preferred = BatchDeviceCandidateV1::new("preferred", "device-a", 8, 1)
        .expect("static preferred candidate is valid");
    let survivor = BatchDeviceCandidateV1::new("survivor", "device-b", 4, 10)
        .expect("static survivor candidate is valid");
    let planner = BooleanBatchDevicePreplannerV1::new(vec![preferred, survivor.clone()])
        .expect("static candidate policy is valid");
    let now = Instant::now();
    let capacity = BatchDeviceCapacitySnapshotV1::new_with_generation(
        "be14g-benchmark-provider",
        BATCH_DEVICE_CAPACITY_SOURCE_UNIT,
        7,
        vec![
            BatchDeviceCapacitySampleV1::valid("device-a", 2.0, now)
                .expect("static device-a sample is valid"),
            BatchDeviceCapacitySampleV1::valid("device-b", 8.0, now)
                .expect("static device-b sample is valid"),
        ],
    )
    .expect("static capacity snapshot is valid");
    Fixture {
        planner,
        exact_reference_candidate: survivor,
        capacity,
        now,
    }
}

fn outcome_sanity(committed: &CommittedBatchDeviceSelectionV1, backend: &BenchBackend) -> usize {
    committed.candidate_id().len()
        + committed.placement_id().len()
        + committed.batch_size() as usize
        + committed.preference_score() as usize
        + committed.source_generation() as usize
        + backend.validate_count
        + backend.act_count
        + backend.verify_count
        + backend.commit_count
}

fn unguarded_exact_transaction(
    fixture: &Fixture,
) -> (CommittedBatchDeviceSelectionV1, BenchBackend) {
    let mut backend = BenchBackend::default();
    let committed = execute_unguarded_batch_device_transaction(
        black_box(&fixture.planner),
        black_box(&fixture.exact_reference_candidate),
        black_box(&fixture.capacity),
        fixture.now,
        black_box(&mut backend),
    )
    .expect("fixed unguarded reference transaction commits");
    (committed, backend)
}

fn guarded_preplan_trace_transaction(
    fixture: &Fixture,
) -> (CommittedBatchDeviceSelectionV1, BenchBackend) {
    let trace = fixture
        .planner
        .decision_trace(black_box(&fixture.capacity), fixture.now)
        .expect("fixed guarded trace is valid");
    let mut backend = BenchBackend::default();
    let outcome = execute_guarded_batch_device_transaction(
        black_box(&fixture.planner),
        black_box(&trace),
        black_box(&fixture.capacity),
        fixture.now,
        black_box(&mut backend),
    )
    .expect("fixed guarded transaction succeeds");
    let GuardedBatchDeviceTransactionOutcomeV1::Committed(committed) = outcome else {
        panic!("fixed guarded transaction must commit");
    };
    (committed, backend)
}

fn unguarded_sanity(fixture: &Fixture) -> usize {
    let (committed, backend) = unguarded_exact_transaction(fixture);
    black_box(outcome_sanity(&committed, &backend))
}

fn guarded_sanity(fixture: &Fixture) -> usize {
    let (committed, backend) = guarded_preplan_trace_transaction(fixture);
    black_box(outcome_sanity(&committed, &backend))
}

fn timed(iterations: u64, mut f: impl FnMut() -> usize) -> (u128, usize) {
    let start = Instant::now();
    let mut result = 0;
    for _ in 0..iterations {
        result = black_box(f());
    }
    (start.elapsed().as_nanos(), result)
}

fn emit(path: BenchPath, elapsed_ns: u128, iterations: u64, result: usize) {
    let ns_per_iteration = elapsed_ns as f64 / iterations as f64;
    println!(
        "{},{elapsed_ns},{iterations},{ns_per_iteration:.6},{result}",
        path.as_str()
    );
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = parse_config().map_err(std::io::Error::other)?;
    let fixture = fixture();

    let (reference_commit, reference_backend) = unguarded_exact_transaction(&fixture);
    let (guarded_commit, guarded_backend) = guarded_preplan_trace_transaction(&fixture);
    if reference_commit != guarded_commit {
        return Err(std::io::Error::other(
            "guarded and unguarded benchmark committed selections diverged",
        )
        .into());
    }
    let reference = outcome_sanity(&reference_commit, &reference_backend);
    let guarded = outcome_sanity(&guarded_commit, &guarded_backend);
    if reference != guarded {
        return Err(std::io::Error::other(
            "guarded and unguarded benchmark backend lifecycle evidence diverged",
        )
        .into());
    }

    for _ in 0..config.warmup {
        if selected(config, BenchPath::UnguardedExactTransaction) {
            black_box(unguarded_sanity(black_box(&fixture)));
        }
        if selected(config, BenchPath::GuardedPreplanTraceTransaction) {
            black_box(guarded_sanity(black_box(&fixture)));
        }
    }

    println!("path,elapsed_ns,iterations,ns_per_iteration,outcome_sanity");
    if selected(config, BenchPath::UnguardedExactTransaction) {
        let (elapsed, result) = timed(config.iterations, || unguarded_sanity(&fixture));
        emit(
            BenchPath::UnguardedExactTransaction,
            elapsed,
            config.iterations,
            result,
        );
    }
    if selected(config, BenchPath::GuardedPreplanTraceTransaction) {
        let (elapsed, result) = timed(config.iterations, || guarded_sanity(&fixture));
        emit(
            BenchPath::GuardedPreplanTraceTransaction,
            elapsed,
            config.iterations,
            result,
        );
    }

    eprintln!(
        "NOTE: this benchmark measures one synthetic host-side planning/trace plus explicit test-provider transaction fixture only. The unguarded path receives the exact reference candidate while the guarded path also performs Boolean screening and trace capture. It does not measure a production placement backend, scheduler, Hub lease/transport, physical device, throughput, bandwidth, memory, energy, model quality, allocations, branch misses, or end-to-end workload performance."
    );
    Ok(())
}
