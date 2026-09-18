use std::{
    hint::black_box,
    time::{Duration, Instant},
};

use elastic_core::{BuiltinObjective, ContractId, LogicalResourceId, ObjectiveId};
use elastic_eir::Fingerprint;
use elastic_kernel::{
    execute_guarded_kernel_transaction, execute_kernel_transaction,
    plan_with_boolean_admission_traced, BindingLimits, CapabilitySnapshot, Evidence, EvidenceUnit,
    FeatureRequirement, FeatureSupport, GuardedKernelTransactionOutcomeV1, KernelCandidate,
    KernelRealizationBackendV1, KernelRequirements, ObjectiveEvidence, RealizationIdentity,
    SelectionOutcome, SelectionPolicy, StaticQuantity, SubgroupSupport, WorkgroupLimits,
};

const DEFAULT_WARMUP: u64 = if cfg!(debug_assertions) { 10 } else { 10_000 };
const DEFAULT_ITERATIONS: u64 = if cfg!(debug_assertions) {
    1_000
} else {
    100_000
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BenchPath {
    UnguardedPlanTransaction,
    GuardedBooleanPlanTransaction,
}

impl BenchPath {
    const ALL: [Self; 2] = [
        Self::UnguardedPlanTransaction,
        Self::GuardedBooleanPlanTransaction,
    ];

    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "unguarded_plan_transaction" => Ok(Self::UnguardedPlanTransaction),
            "guarded_boolean_plan_transaction" => Ok(Self::GuardedBooleanPlanTransaction),
            _ => Err(format!(
                "unknown --path {value:?}; expected one of {}",
                Self::ALL
                    .iter()
                    .map(|path| path.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::UnguardedPlanTransaction => "unguarded_plan_transaction",
            Self::GuardedBooleanPlanTransaction => "guarded_boolean_plan_transaction",
        }
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
                println!(
                    "usage: be14f_kernel_realization [--warmup N] [--iterations N] [--path NAME]"
                );
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
    activate_count: usize,
    verify_count: usize,
    commit_count: usize,
}

impl KernelRealizationBackendV1 for BenchBackend {
    fn validate_candidate(
        &mut self,
        candidate: &KernelCandidate,
        fresh_capabilities: &CapabilitySnapshot,
    ) -> Result<(), String> {
        black_box(candidate.realization().as_str());
        black_box(fresh_capabilities.fingerprint());
        self.validate_count += 1;
        Ok(())
    }

    fn activate_candidate(&mut self, candidate: &KernelCandidate) -> Result<(), String> {
        black_box(candidate.realization().as_str());
        self.activate_count += 1;
        Ok(())
    }

    fn verify_candidate(&mut self, candidate: &KernelCandidate) -> Result<(), String> {
        black_box(candidate.contract().as_str());
        self.verify_count += 1;
        Ok(())
    }

    fn commit_candidate(&mut self, candidate: &KernelCandidate) -> Result<(), String> {
        black_box(candidate.realization().as_str());
        self.commit_count += 1;
        Ok(())
    }

    fn rollback_candidate(
        &mut self,
        _candidate: &KernelCandidate,
        _reason: &str,
    ) -> Result<(), String> {
        Err("benchmark success fixture must not roll back".to_owned())
    }
}

struct Fixture {
    resource: LogicalResourceId,
    workload: Fingerprint,
    policy: SelectionPolicy,
    candidates: Vec<KernelCandidate>,
    capabilities: CapabilitySnapshot,
    observed_at: Instant,
    now: Instant,
}

fn latency() -> ObjectiveId {
    ObjectiveId::builtin(BuiltinObjective::Latency)
}

fn requirements(workgroup_storage_bytes: u64) -> KernelRequirements {
    KernelRequirements {
        invocations_per_workgroup: 64,
        invocations_per_axis: [64, 1, 1],
        workgroup_storage_bytes,
        bind_groups: 2,
        max_storage_buffer_binding_bytes: 4096,
        subgroup_min_width: None,
        shader_f16: FeatureRequirement::NotRequired,
        matrix_ops: FeatureRequirement::NotRequired,
    }
}

fn candidate(
    resource: &LogicalResourceId,
    contract: &ContractId,
    realization: &str,
    workgroup_storage_bytes: u64,
    latency_ns: u64,
) -> KernelCandidate {
    KernelCandidate::new(
        resource.clone(),
        RealizationIdentity::new(realization).expect("static realization is valid"),
        1,
        requirements(workgroup_storage_bytes),
        contract.clone(),
        ObjectiveEvidence::new().with(
            latency(),
            Evidence::StaticEstimate(StaticQuantity {
                magnitude: latency_ns,
                unit: EvidenceUnit::Nanoseconds,
            }),
        ),
    )
    .expect("static candidate is valid")
}

fn fixture() -> Fixture {
    let resource = LogicalResourceId::new("be14f-kernel-benchmark").expect("static id is valid");
    let contract = ContractId::new("be14f-kernel-contract-v1").expect("static contract is valid");
    let policy = SelectionPolicy::new(vec![latency()], contract.clone(), true)
        .expect("static policy is valid");
    let candidates = vec![
        candidate(&resource, &contract, "too-large", 64 << 10, 50),
        candidate(&resource, &contract, "portable", 1024, 100),
    ];
    let capabilities = CapabilitySnapshot {
        workgroup_limits: WorkgroupLimits {
            max_invocations_per_axis: [64, 64, 64],
            max_invocations_per_workgroup: 256,
            max_workgroups_per_axis: 65_535,
            max_workgroup_storage_bytes: 32_768,
        },
        binding_limits: BindingLimits {
            max_bind_groups: 8,
            max_storage_buffer_binding_bytes: 128 << 20,
        },
        subgroup_support: SubgroupSupport::unsupported(),
        shader_f16: FeatureSupport::Known(false),
        matrix_ops: FeatureSupport::Known(false),
    };
    let now = Instant::now();
    Fixture {
        resource,
        workload: Fingerprint::EMPTY.text("be14f/portable-benchmark-workload"),
        policy,
        candidates,
        capabilities,
        observed_at: now,
        now,
    }
}

fn unguarded_plan_transaction(fixture: &Fixture) -> usize {
    let outcome = elastic_kernel::plan(
        black_box(&fixture.resource),
        black_box(fixture.workload),
        black_box(&fixture.capabilities),
        black_box(&fixture.policy),
        black_box(&fixture.candidates),
    );
    let SelectionOutcome::Selected(record) = outcome else {
        panic!("fixed unguarded fixture must select the portable candidate");
    };
    let candidate = fixture
        .candidates
        .iter()
        .find(|candidate| candidate.realization() == record.selected_realization())
        .expect("selected candidate exists");
    let mut backend = BenchBackend::default();
    let committed = execute_kernel_transaction(
        black_box(&record),
        black_box(candidate),
        black_box(&fixture.capabilities),
        black_box(&mut backend),
    )
    .expect("fixed unguarded transaction commits");
    black_box(
        committed.realization().as_str().len()
            + backend.validate_count
            + backend.activate_count
            + backend.verify_count
            + backend.commit_count,
    )
}

fn guarded_boolean_plan_transaction(fixture: &Fixture) -> usize {
    let (outcome, trace) = plan_with_boolean_admission_traced(
        black_box(&fixture.resource),
        black_box(fixture.workload),
        black_box(&fixture.policy),
        black_box(&fixture.candidates),
        Some(black_box(&fixture.capabilities)),
        Some(fixture.observed_at),
        fixture.now,
        Duration::from_secs(1),
    )
    .expect("fixed guarded planning succeeds");
    let mut backend = BenchBackend::default();
    let committed = execute_guarded_kernel_transaction(
        black_box(&outcome),
        black_box(&trace),
        black_box(&fixture.candidates),
        black_box(&fixture.capabilities),
        black_box(&mut backend),
    )
    .expect("fixed guarded transaction succeeds");
    let GuardedKernelTransactionOutcomeV1::Committed(committed) = committed else {
        panic!("fixed guarded transaction must commit");
    };
    black_box(
        committed.realization().as_str().len()
            + backend.validate_count
            + backend.activate_count
            + backend.verify_count
            + backend.commit_count,
    )
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

    let baseline_result = unguarded_plan_transaction(&fixture);
    let guarded_result = guarded_boolean_plan_transaction(&fixture);
    if baseline_result != guarded_result {
        return Err(
            std::io::Error::other("guarded and unguarded benchmark outcomes diverged").into(),
        );
    }

    for _ in 0..config.warmup {
        if selected(config, BenchPath::UnguardedPlanTransaction) {
            black_box(unguarded_plan_transaction(black_box(&fixture)));
        }
        if selected(config, BenchPath::GuardedBooleanPlanTransaction) {
            black_box(guarded_boolean_plan_transaction(black_box(&fixture)));
        }
    }

    println!("path,elapsed_ns,iterations,ns_per_iteration,outcome_sanity");
    if selected(config, BenchPath::UnguardedPlanTransaction) {
        let (elapsed, result) = timed(config.iterations, || unguarded_plan_transaction(&fixture));
        emit(
            BenchPath::UnguardedPlanTransaction,
            elapsed,
            config.iterations,
            result,
        );
    }
    if selected(config, BenchPath::GuardedBooleanPlanTransaction) {
        let (elapsed, result) = timed(config.iterations, || {
            guarded_boolean_plan_transaction(&fixture)
        });
        emit(
            BenchPath::GuardedBooleanPlanTransaction,
            elapsed,
            config.iterations,
            result,
        );
    }

    eprintln!(
        "NOTE: this benchmark measures one synthetic host-side planning plus test-provider transaction fixture only. It does not measure a physical kernel, GPU execution, throughput, bandwidth, memory, energy, model quality, allocations, branch misses, or end-to-end workload performance."
    );
    Ok(())
}
