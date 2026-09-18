use std::{hint::black_box, time::Instant};

use elastic_core::resource::{
    AdmissibleTransition, CapabilityRequirement, ContractId, DimensionId, Invariant, InvariantKind,
    LogicalResourceId, RepresentationalDeclaration, ResourceClassId, ResourceSpec,
};
use elastic_core::{
    CapabilitySet, ObservationEpoch, RepresentationEpoch, RepresentationId, RepresentationState,
    RepresentationTransition, ResourceGeneration, TransitionAttestations, TransitionMechanism,
};
use elastic_eir::PlanningContext;
use elastic_runtime::{
    representation_precision_floor_signal, BooleanRepresentationPrecisionPreplannerV1, Observation,
    ObservationSnapshot, ObservationSource, RepresentationPrecisionCandidateV1,
};

const DEFAULT_WARMUP: u64 = if cfg!(debug_assertions) { 10 } else { 10_000 };
const DEFAULT_ITERATIONS: u64 = if cfg!(debug_assertions) {
    1_000
} else {
    100_000
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BenchPath {
    UnguardedDeclaredValidate,
    GuardedPreplanValidate,
}

impl BenchPath {
    const ALL: [Self; 2] = [
        Self::UnguardedDeclaredValidate,
        Self::GuardedPreplanValidate,
    ];

    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "unguarded_declared_validate" => Ok(Self::UnguardedDeclaredValidate),
            "guarded_preplan_validate" => Ok(Self::GuardedPreplanValidate),
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
            Self::UnguardedDeclaredValidate => "unguarded_declared_validate",
            Self::GuardedPreplanValidate => "guarded_preplan_validate",
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
                    "usage: be14e_representation_precision [--warmup N] [--iterations N] [--path NAME]"
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

struct Fixture {
    declaration: RepresentationalDeclaration,
    preplanner: BooleanRepresentationPrecisionPreplannerV1,
    current: RepresentationState,
    target: RepresentationId,
    capabilities: CapabilitySet,
    planning_context: PlanningContext,
    observations: ObservationSnapshot,
    now: Instant,
}

fn declaration() -> RepresentationalDeclaration {
    let spec = ResourceSpec::builder(
        ResourceClassId::REPRESENTATIONAL,
        LogicalResourceId::new("be14e-benchmark").expect("static fixture id is valid"),
    )
    .allow(DimensionId::REPRESENTATION)
    .observe(representation_precision_floor_signal())
    .preserve(
        Invariant::new(InvariantKind::UpholdContract(
            ContractId::new("be14e.benchmark.semantic-contract")
                .expect("static contract id is valid"),
        ))
        .along(DimensionId::REPRESENTATION),
    )
    .admit(AdmissibleTransition::new(
        TransitionMechanism::Reencode,
        DimensionId::REPRESENTATION,
    ))
    .require_capability(CapabilityRequirement::new(
        TransitionMechanism::Reencode,
        DimensionId::REPRESENTATION,
    ))
    .build()
    .expect("static benchmark resource declaration is valid");

    RepresentationalDeclaration::new(
        spec,
        [
            (
                RepresentationId::new("tensor.fp16").expect("static representation id is valid"),
                1,
            ),
            (
                RepresentationId::new("tensor.int8").expect("static representation id is valid"),
                1,
            ),
            (
                RepresentationId::new("tensor.int4").expect("static representation id is valid"),
                1,
            ),
        ],
    )
    .expect("static benchmark declaration is valid")
}

fn candidate(id: &str, rank: u32, bits: u16) -> RepresentationPrecisionCandidateV1 {
    RepresentationPrecisionCandidateV1::new(
        id,
        rank,
        RepresentationId::new(id).expect("static candidate id is valid"),
        1,
        TransitionMechanism::Reencode,
        bits,
    )
    .expect("static benchmark candidate is valid")
}

fn fixture() -> Fixture {
    let declaration = declaration();
    let preplanner = BooleanRepresentationPrecisionPreplannerV1::new(
        declaration.clone(),
        vec![
            candidate("tensor.int4", 0, 4),
            candidate("tensor.int8", 10, 8),
        ],
    )
    .expect("static benchmark preplanner is valid");
    let current = RepresentationState::new(
        RepresentationId::new("tensor.fp16").expect("static source id is valid"),
        1,
        RepresentationEpoch::new(7),
    );
    let target = RepresentationId::new("tensor.int8").expect("static target id is valid");
    let mut capabilities = CapabilitySet::new();
    for id in ["tensor.fp16", "tensor.int8", "tensor.int4"] {
        capabilities.insert(
            RepresentationId::new(id).expect("static capability id is valid"),
            1,
        );
    }
    let now = Instant::now();
    let signal = representation_precision_floor_signal();
    let planning_context = PlanningContext::new().observe(signal.clone(), 8.0);
    let observations = ObservationSnapshot::new(
        now,
        vec![Observation::from_source(
            ObservationSource::runtime("be14e-portable-benchmark"),
            signal,
            8.0,
            now,
        )],
    );
    Fixture {
        declaration,
        preplanner,
        current,
        target,
        capabilities,
        planning_context,
        observations,
        now,
    }
}

fn attestations() -> TransitionAttestations {
    TransitionAttestations::none().attest_reencoder_available()
}

fn unguarded_declared_validate(fixture: &Fixture) -> usize {
    let target = fixture
        .declaration
        .derive_target(
            black_box(&fixture.current),
            black_box(&fixture.target),
            1,
            TransitionMechanism::Reencode,
        )
        .expect("declared baseline target remains valid");
    let transition = RepresentationTransition {
        from: fixture.current.clone(),
        to: target,
        mechanism: TransitionMechanism::Reencode,
    };
    transition
        .validate(black_box(&fixture.capabilities), attestations())
        .expect("trusted baseline validation remains valid");
    black_box(transition.to.id.as_str().len())
}

fn guarded_preplan_validate(fixture: &Fixture) -> usize {
    let report = fixture
        .preplanner
        .screen(
            black_box(&fixture.current),
            black_box(&fixture.capabilities),
            black_box(&fixture.planning_context),
            black_box(&fixture.observations),
            fixture.now,
            ObservationEpoch::new(11),
            ResourceGeneration::new(3),
        )
        .expect("guarded benchmark screening remains valid");
    let transition = fixture
        .preplanner
        .selected_transition(black_box(&fixture.current), black_box(&report))
        .expect("guarded selected transition resolves")
        .expect("fixed fixture selects tensor.int8");
    transition
        .validate(black_box(&fixture.capabilities), attestations())
        .expect("authoritative guarded validation remains valid");
    black_box(transition.to.id.as_str().len())
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

    let baseline_result = unguarded_declared_validate(&fixture);
    let guarded_result = guarded_preplan_validate(&fixture);
    if baseline_result != guarded_result {
        return Err(std::io::Error::other("guarded and baseline target results diverged").into());
    }

    for _ in 0..config.warmup {
        if selected(config, BenchPath::UnguardedDeclaredValidate) {
            black_box(unguarded_declared_validate(black_box(&fixture)));
        }
        if selected(config, BenchPath::GuardedPreplanValidate) {
            black_box(guarded_preplan_validate(black_box(&fixture)));
        }
    }

    println!("path,elapsed_ns,iterations,ns_per_iteration,target_id_bytes");
    if selected(config, BenchPath::UnguardedDeclaredValidate) {
        let (elapsed, result) = timed(config.iterations, || unguarded_declared_validate(&fixture));
        emit(
            BenchPath::UnguardedDeclaredValidate,
            elapsed,
            config.iterations,
            result,
        );
    }
    if selected(config, BenchPath::GuardedPreplanValidate) {
        let (elapsed, result) = timed(config.iterations, || guarded_preplan_validate(&fixture));
        emit(
            BenchPath::GuardedPreplanValidate,
            elapsed,
            config.iterations,
            result,
        );
    }

    eprintln!(
        "NOTE: this benchmark measures one synthetic host-side planning+validation fixture only. It does not measure physical re-encoding, memory savings, model quality, device behavior, energy, allocations, branch misses, or end-to-end workload performance."
    );
    Ok(())
}
