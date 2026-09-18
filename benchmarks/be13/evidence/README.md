# BE13 retained portable benchmark evidence

This directory retains raw development measurements for the portable Boolean
screening harness. Every evidence set records an exact source commit, Rust
version, kernel/hardware identity, CPU affinity, collection bounds, checksums and
an explicit interpretation boundary.

Two schemas are supported:

- `elasticxxx-be13-portable-evidence/v1`: historical repeated timing with
  before/after CPU-frequency samples. Existing v1 evidence remains valid and is
  not silently reinterpreted.
- `elasticxxx-be13-portable-evidence/v2`: one-path-per-process timing with
  deterministic path rotation, optional transactional CPUFreq lock control,
  continuous CPUFreq samples, a preregistered block-median timing-drift gate,
  and separately scoped direct process metrics.

For v2, `comparison_qualified=true` means only that the declared CPUFreq control
was accepted, sampled stable at the target throughout collection, restored
afterwards, and that every path passed the BE13 timing-drift policy (minimum 30
repetitions; non-overlapping blocks of 5; block-median spread at most 10% of the
overall median; no post-hoc row deletion). Linux `scaling_cur_freq` is a CPUFreq
policy report and is not claimed to be an exact instantaneous hardware-frequency
measurement.

`process_metrics.csv` may contain direct user-space branch-miss counts from
Linux `perf_event_open(PERF_COUNT_HW_BRANCH_MISSES)` and whole-process peak RSS
from `wait4(2)/ru_maxrss`. Those measurements include loader/setup/warmup/output
and therefore have a wider scope than the `raw.csv` Rust timed loop. Allocation
count remains `unmeasured` until a separately reviewed region-scoped method is
qualified.

Retained evidence is descriptive. It is not, by itself, a speedup, hardware,
energy, scientific-novelty or actuation claim. Unmeasured fields remain the
literal string `unmeasured` and must never be interpreted as zero.

## BE13e acceleration-gate evidence

The `2026-09-18-be13e-*` directories retain the architecture-acceleration gate
comparison. Optional v2 metadata fields `codegen_profile` and `rustflags`
distinguish the normal portable build from the `target-cpu=native` probe. The
feature-probe fields record Rust `std::arch` runtime detection only; they do not
authorize a specialized execution path.

See `docs/boolean-be13-acceleration-gate.md` for the bounded interpretation and
gate decision. The conclusion is intentionally conservative: no specialized
SIMD/SVE/native dispatch is enabled by these measurements.

### BE13e provenance hardening

The original `2026-09-18-be13e-e8268e8-{portable,native}` files predate
collector-level codegen attestation. They remain archival controlled timing
sets, but their post-hoc codegen labels are not sufficient for codegen
attribution. The source commit itself is retained by
`refs/tags/elasticxxx-be13e-source-e8268e8`.

The corrected `2026-09-18-be13e-attested-32415254-{portable,native}` sets bind to
`refs/tags/elasticxxx-be13e-source-32415254` and include collector-generated
rustflag metadata, `cargo rustc -- --print cfg` output, Cargo-config inventory,
and SHA-256 bindings. These are the evidence sets used by the current BE13e gate
decision.
