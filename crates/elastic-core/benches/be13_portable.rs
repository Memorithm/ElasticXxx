use std::{hint::black_box, mem::size_of_val, time::Instant};

use elastic_core::{
    BoolExpr, CompiledGuard, FactSet, MultiwordCompiledGuard, MultiwordFactSet,
    MultiwordGuardBatch, PredicateId, TruthValue,
};

const DEFAULT_WARMUP: u64 = if cfg!(debug_assertions) { 10 } else { 10_000 };
const DEFAULT_ITERATIONS: u64 = if cfg!(debug_assertions) {
    1_000
} else {
    200_000
};
const BATCH_GUARDS: usize = 64;

#[derive(Clone, Copy)]
struct Config {
    warmup: u64,
    iterations: u64,
}

fn parse_config() -> Result<Config, String> {
    let mut config = Config {
        warmup: DEFAULT_WARMUP,
        iterations: DEFAULT_ITERATIONS,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let target = match arg.as_str() {
            "--warmup" => &mut config.warmup,
            "--iterations" => &mut config.iterations,
            "--bench" => continue,
            "--help" | "-h" => {
                println!("usage: be13_portable [--warmup N] [--iterations N]");
                std::process::exit(0);
            }
            _ => return Err(format!("unknown argument: {arg}")),
        };
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {arg}"))?;
        *target = value
            .parse::<u64>()
            .map_err(|_| format!("invalid integer for {arg}: {value}"))?;
    }
    if config.iterations == 0 {
        return Err("--iterations must be greater than zero".to_owned());
    }
    Ok(config)
}

fn scalar_guard(a: TruthValue, b: TruthValue, c: TruthValue) -> TruthValue {
    a.kleene_and(b.negated()).kleene_and(c)
}

fn timed<F>(iterations: u64, mut f: F) -> (u128, TruthValue)
where
    F: FnMut() -> TruthValue,
{
    let start = Instant::now();
    let mut last = TruthValue::Unknown;
    for _ in 0..iterations {
        last = black_box(f());
    }
    (start.elapsed().as_nanos(), last)
}

fn emit(name: &str, elapsed_ns: u128, evaluations: u64, result: TruthValue, bytes: f64) {
    let ns_per_guard = elapsed_ns as f64 / evaluations as f64;
    let candidates_per_second = if elapsed_ns == 0 {
        f64::INFINITY
    } else {
        evaluations as f64 * 1_000_000_000.0 / elapsed_ns as f64
    };
    println!(
        "{name},{elapsed_ns},{evaluations},{ns_per_guard:.6},{candidates_per_second:.3},{bytes:.3},unmeasured,unmeasured,unmeasured,{result:?}"
    );
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = parse_config().map_err(std::io::Error::other)?;
    let a = PredicateId::new(1);
    let b = PredicateId::new(2);
    let c = PredicateId::new(3);
    let expression = BoolExpr::all([
        BoolExpr::atom(a),
        BoolExpr::negate(BoolExpr::atom(b)),
        BoolExpr::atom(c),
    ]);

    let facts = FactSet::new()
        .with(a, TruthValue::True)?
        .with(b, TruthValue::False)?
        .with(c, TruthValue::True)?;
    let compiled = CompiledGuard::compile(&expression)?;
    let multi_facts = MultiwordFactSet::new(130)?
        .with(a, TruthValue::True)?
        .with(b, TruthValue::False)?
        .with(c, TruthValue::True)?;
    let multi = MultiwordCompiledGuard::compile(&expression, 130)?;
    let expressions = vec![expression.clone(); BATCH_GUARDS];
    let batch = MultiwordGuardBatch::compile(&expressions, 130)?;

    let scalar_values = [TruthValue::True, TruthValue::False, TruthValue::True];
    for _ in 0..config.warmup {
        let values = black_box(&scalar_values);
        black_box(scalar_guard(
            black_box(values[0]),
            black_box(values[1]),
            black_box(values[2]),
        ));
        black_box(black_box(&expression).evaluate(black_box(&facts))?);
        black_box(black_box(&compiled).evaluate(black_box(&facts))?);
        black_box(black_box(&multi).evaluate(black_box(&multi_facts))?);
        black_box(black_box(&batch).evaluate(black_box(&multi_facts))?);
    }

    println!("path,elapsed_ns,evaluations,ns_per_guard,candidates_per_second,stack_bytes_per_guard,allocations,memory_peak_bytes,branch_misses,result");

    let (elapsed, result) = timed(config.iterations, || {
        let values = black_box(&scalar_values);
        scalar_guard(
            black_box(values[0]),
            black_box(values[1]),
            black_box(values[2]),
        )
    });
    emit("scalar_if_chain", elapsed, config.iterations, result, 0.0);

    let (elapsed, result) = timed(config.iterations, || {
        black_box(&expression).evaluate(black_box(&facts)).unwrap()
    });
    emit(
        "generic_bool_expr",
        elapsed,
        config.iterations,
        result,
        size_of_val(&expression) as f64,
    );

    let (elapsed, result) = timed(config.iterations, || {
        black_box(&compiled).evaluate(black_box(&facts)).unwrap()
    });
    emit(
        "u64_compiled_guard",
        elapsed,
        config.iterations,
        result,
        size_of_val(&compiled) as f64,
    );

    let (elapsed, result) = timed(config.iterations, || {
        black_box(&multi).evaluate(black_box(&multi_facts)).unwrap()
    });
    emit(
        "multiword_guard",
        elapsed,
        config.iterations,
        result,
        size_of_val(&multi) as f64,
    );

    let start = Instant::now();
    let mut last = TruthValue::Unknown;
    for _ in 0..config.iterations {
        let screen = black_box(black_box(&batch).evaluate(black_box(&multi_facts))?);
        last = *screen.outcomes().last().unwrap_or(&TruthValue::Unknown);
        black_box(screen);
    }
    let elapsed = start.elapsed().as_nanos();
    let evaluations = config
        .iterations
        .checked_mul(BATCH_GUARDS as u64)
        .ok_or_else(|| std::io::Error::other("batch evaluation count overflow"))?;
    emit(
        "batch_filter",
        elapsed,
        evaluations,
        last,
        size_of_val(&batch) as f64 / BATCH_GUARDS as f64,
    );

    eprintln!(
        "NOTE: allocation count, peak memory and branch misses are emitted as unmeasured; do not infer zero. stack_bytes_per_guard excludes heap allocations."
    );
    Ok(())
}
