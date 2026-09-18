# BE13 portable Boolean screening benchmark protocol

Status: **portable harness qualified by PR #124; reproducible measurement evidence in progress; no performance claim**.

The dependency-free `be13_portable` benchmark compares five declared execution
paths on the same three-valued conjunction and stable fact assignment:
`scalar_if_chain`, generic `BoolExpr`, `u64_compiled_guard`,
`multiword_guard`, and `batch_filter`.

Run it only from a clean exact revision and record that revision beside the raw
output:

```bash
git rev-parse HEAD
cargo bench -p elastic-core --bench be13_portable -- --warmup 10000 --iterations 200000
```

The CSV fields report elapsed nanoseconds, evaluated guards, derived
`ns_per_guard`, derived candidates/second, and the Rust stack size of the guard
value. `stack_bytes_per_guard` is **not** total retained memory and excludes heap
storage.

Allocation count, peak memory and branch misses are deliberately emitted as
`unmeasured`. They must not be interpreted as zero. A later measurement slice
may add a reviewed portable/instrumented allocator or platform counter path,
but only if its dependencies, licences and measurement semantics are explicit.

The benchmark performs no actuation and grants no validation authority. Timing
numbers are host- and build-specific development evidence only. No speedup claim
is valid unless raw output, toolchain/build mode, hardware identity and exact
commit SHA are retained and the compared paths are semantically equivalent.


## Reproducible evidence collector

Use `scripts/collect-be13-portable-evidence.sh` from a clean exact revision to
retain repeated raw rows together with toolchain and hardware provenance. The
collector defaults to 30 repetitions, 10,000 warmup evaluations, 500,000 timed
iterations, and CPU 0 affinity when `taskset` is available. The repetition,
warmup, iteration and CPU settings are explicit environment variables.

```bash
BE13_REPETITIONS=30 \
BE13_WARMUP=10000 \
BE13_ITERATIONS=500000 \
BE13_CPU=0 \
./scripts/collect-be13-portable-evidence.sh /tmp/be13-evidence
```

The collector refuses a dirty worktree and a non-empty destination. It validates
that every repetition contains the five declared paths and preserves the expected
`True` semantic result. `allocations`, `memory_peak_bytes`, and `branch_misses`
remain explicit `unmeasured` fields until a separately reviewed measurement path
exists. Retained timing evidence is descriptive development evidence for one
source SHA and host configuration; it does not by itself authorize BE13e or any
speedup claim.

Retained evidence is checked by the dedicated `BE13 retained evidence integrity`
workflow. The gate validates the complete five-path repetition matrix, explicit
`unmeasured` fields, finite numeric fields, per-repetition CPU-frequency
samples when available, and SHA-256 binding of raw CSV, frequency samples and
metadata. It does not reinterpret the measurements as a speedup claim.
