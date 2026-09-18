use std::{
    hint::black_box,
    time::{Duration, Instant},
};

use elastic_core::{
    resource::{
        AdmissibleTransition, CapabilityRequirement, DimensionId, LogicalResourceId,
        ObservationSignalId, ResourceClassId, ResourceSpec,
    },
    ObservationEpoch, ResourceGeneration, TransitionMechanism,
};
use elastic_eir::{PlanningContext, TransitionCandidate};
use elastic_runtime::{
    execute_guarded_thermal_energy_transaction, execute_unguarded_thermal_energy_transaction,
    BooleanThermalEnergyPreplannerV1, CommittedThermalEnergyTransitionV1,
    GuardedThermalEnergyTransactionOutcomeV1, Observation, ObservationSnapshot, ObservationSource,
    ThermalEnergyTransitionBackendV1,
};

const DEFAULT_WARMUP: u64 = if cfg!(debug_assertions) { 10 } else { 10_000 };
const DEFAULT_ITERATIONS: u64 = if cfg!(debug_assertions) {
    1_000
} else {
    100_000
};
const BENCH_MAX_AGE: Duration = Duration::from_secs(3_600);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BenchPath {
    UnguardedNumericTransaction,
    GuardedPreplanTraceTransaction,
}

impl BenchPath {
    const ALL: [Self; 2] = [
        Self::UnguardedNumericTransaction,
        Self::GuardedPreplanTraceTransaction,
    ];

    const fn as_str(self) -> &'static str {
        match self {
            Self::UnguardedNumericTransaction => "unguarded_numeric_transaction",
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
                println!("usage: be14h_thermal_energy [--warmup N] [--iterations N] [--path NAME]");
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

impl ThermalEnergyTransitionBackendV1 for BenchBackend {
    fn validate_transition(
        &mut self,
        candidate: &TransitionCandidate,
        planning_context: &PlanningContext,
        observations: &ObservationSnapshot,
        now: Instant,
    ) -> Result<(), String> {
        black_box(candidate);
        black_box(planning_context);
        black_box(observations);
        black_box(now);
        self.validate_count += 1;
        Ok(())
    }

    fn actuate_transition(&mut self, candidate: &TransitionCandidate) -> Result<(), String> {
        black_box(candidate);
        self.act_count += 1;
        Ok(())
    }

    fn verify_transition(&mut self, candidate: &TransitionCandidate) -> Result<(), String> {
        black_box(candidate);
        self.verify_count += 1;
        Ok(())
    }

    fn commit_transition(&mut self, candidate: &TransitionCandidate) -> Result<(), String> {
        black_box(candidate);
        self.commit_count += 1;
        Ok(())
    }

    fn rollback_transition(
        &mut self,
        _candidate: &TransitionCandidate,
        _reason: &str,
    ) -> Result<(), String> {
        Err("benchmark success fixture must not roll back".to_owned())
    }
}

struct Fixture {
    planner: BooleanThermalEnergyPreplannerV1,
    context: PlanningContext,
    observations: ObservationSnapshot,
    now: Instant,
    epoch: ObservationEpoch,
    generation: ResourceGeneration,
}

fn fixture() -> Fixture {
    let spec = ResourceSpec::builder(
        ResourceClassId::CONFIGURATIONAL,
        LogicalResourceId::new("be14h-benchmark-resource")
            .expect("static benchmark resource identity is valid"),
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
    .expect("static benchmark resource specification is valid");

    let thermal_source = ObservationSource::host("benchmark:thermal");
    let power_source = ObservationSource::host("benchmark:power");
    let planner = BooleanThermalEnergyPreplannerV1::new(
        spec,
        TransitionMechanism::Reinterpret,
        DimensionId::ENERGY,
        8.0,
        75.0,
        thermal_source.clone(),
        power_source.clone(),
        BENCH_MAX_AGE,
    )
    .expect("static thermal/energy policy is valid");
    let now = Instant::now();
    let context = PlanningContext::new()
        .observe(ObservationSignalId::THERMAL_MARGIN, 12.0)
        .observe(ObservationSignalId::ENERGY_RATE, 60.0);
    let observations = ObservationSnapshot::new(
        now,
        vec![
            Observation::from_source(
                thermal_source,
                ObservationSignalId::THERMAL_MARGIN,
                12.0,
                now,
            ),
            Observation::from_source(power_source, ObservationSignalId::ENERGY_RATE, 60.0, now),
        ],
    );

    Fixture {
        planner,
        context,
        observations,
        now,
        epoch: ObservationEpoch::new(17),
        generation: ResourceGeneration::new(23),
    }
}

fn outcome_sanity(committed: &CommittedThermalEnergyTransitionV1, backend: &BenchBackend) -> usize {
    black_box(committed.candidate());
    committed.observation_epoch().get() as usize
        + committed.resource_generation().get() as usize
        + backend.validate_count
        + backend.act_count
        + backend.verify_count
        + backend.commit_count
}

fn unguarded_numeric_transaction(
    fixture: &Fixture,
) -> (CommittedThermalEnergyTransitionV1, BenchBackend) {
    let mut backend = BenchBackend::default();
    let committed = execute_unguarded_thermal_energy_transaction(
        black_box(&fixture.planner),
        black_box(&fixture.context),
        black_box(&fixture.observations),
        fixture.now,
        fixture.epoch,
        fixture.generation,
        black_box(&mut backend),
    )
    .expect("fixed unguarded numeric reference transaction commits");
    (committed, backend)
}

fn guarded_preplan_trace_transaction(
    fixture: &Fixture,
) -> (CommittedThermalEnergyTransitionV1, BenchBackend) {
    let mut backend = BenchBackend::default();
    let outcome = execute_guarded_thermal_energy_transaction(
        black_box(&fixture.planner),
        black_box(&fixture.context),
        black_box(&fixture.observations),
        fixture.now,
        fixture.epoch,
        fixture.generation,
        black_box(&mut backend),
    )
    .expect("fixed guarded thermal/energy transaction succeeds");
    let GuardedThermalEnergyTransactionOutcomeV1::Committed(committed) = outcome else {
        panic!("fixed guarded thermal/energy policy must commit");
    };
    (committed, backend)
}

fn unguarded_sanity(fixture: &Fixture) -> usize {
    let (committed, backend) = unguarded_numeric_transaction(fixture);
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

    let (reference_commit, reference_backend) = unguarded_numeric_transaction(&fixture);
    let (guarded_commit, guarded_backend) = guarded_preplan_trace_transaction(&fixture);
    if reference_commit.candidate() != guarded_commit.candidate()
        || reference_commit.observation_epoch() != guarded_commit.observation_epoch()
        || reference_commit.resource_generation() != guarded_commit.resource_generation()
    {
        return Err(std::io::Error::other(
            "guarded and unguarded benchmark committed transitions diverged",
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
        if selected(config, BenchPath::UnguardedNumericTransaction) {
            black_box(unguarded_sanity(black_box(&fixture)));
        }
        if selected(config, BenchPath::GuardedPreplanTraceTransaction) {
            black_box(guarded_sanity(black_box(&fixture)));
        }
    }

    println!("path,elapsed_ns,iterations,ns_per_iteration,outcome_sanity");
    if selected(config, BenchPath::UnguardedNumericTransaction) {
        let (elapsed, result) = timed(config.iterations, || unguarded_sanity(&fixture));
        emit(
            BenchPath::UnguardedNumericTransaction,
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

    Ok(())
}
