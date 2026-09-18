# BE13 portable Boolean screening benchmark protocol

Status: **portable harness qualified; v1 timing retained as DVFS-confounded negative evidence; v2 controlled measurement evidence retained and stability-qualified; no performance claim**.

The dependency-free `be13_portable` benchmark compares five declared execution
paths on the same three-valued conjunction and stable fact assignment:
`scalar_if_chain`, generic `BoolExpr`, `u64_compiled_guard`,
`multiword_guard`, and `batch_filter`.

The benchmark supports the historical five-path run and a single-path mode:

```bash
git rev-parse HEAD
cargo +1.89.0 bench -p elastic-core --bench be13_portable -- \
  --warmup 10000 --iterations 200000
cargo +1.89.0 bench -p elastic-core --bench be13_portable -- \
  --warmup 10000 --iterations 200000 --path u64_compiled_guard
```

`raw.csv` timing fields report elapsed nanoseconds, evaluated guards, derived
`ns_per_guard`, derived candidates/second, and Rust stack size of the guard
value. `stack_bytes_per_guard` is **not** total retained memory and excludes heap
storage. Allocation count, peak memory and branch misses in `raw.csv` remain the
literal `unmeasured`: differently scoped instrumentation must not be silently
mixed into the Rust `Instant` timing region.

## Evidence schemas

`elasticxxx-be13-portable-evidence/v1` is retained for historical evidence. Its
CPU-frequency samples are before/after samples only. The retained v1 Jetson AGX
Thor set varies from 972000 to 2601000 kHz and is therefore explicitly
DVFS-confounded.

`elasticxxx-be13-portable-evidence/v2` adds controls and supplementary metrics
without changing v1 evidence:

- one selected path per benchmark process;
- deterministic rotation of path order across repetitions;
- optional transactional CPUFreq `lock-max` control with original policy
  restoration on success, failure, or interruption;
- continuous `scaling_cur_freq` sampling plus before/after samples for each path;
- a preregistered timing-drift gate: at least 30 repetitions, contiguous non-overlapping blocks of 5, and per-path block-median spread no greater than 10% of the overall median, with no row deletion;
- a separate `process_metrics.csv` obtained through a small Linux C helper;
- direct generalized hardware `PERF_COUNT_HW_BRANCH_MISSES` counting when the
  PMU permits it, user-space only;
- whole-process peak RSS from `wait4(2)` / `ru_maxrss`;
- allocation count remains explicit `unmeasured` until a reviewed region-scoped
  allocation method exists.

Linux CPUFreq's `performance` governor requests the highest frequency permitted
by the policy. The v2 collector additionally sets `scaling_min_freq` and
`scaling_max_freq` to the same target while measurements run and restores the
previous governor and limits afterwards. `scaling_cur_freq` is retained as a
kernel CPUFreq policy report; it is **not claimed to be exact instantaneous
hardware frequency**. A v2 set is marked `comparison_qualified=true` only when
the `lock-max` policy was accepted, all continuous and edge samples matched the
target, the original policy was successfully restored, and every benchmark path
passes the preregistered block-median timing-drift gate. The 10% limit is an
ElasticXxx engineering qualification threshold, not a statistical significance
test or an external standard.

The direct branch-miss and RSS measurements have a deliberately wider scope
than `raw.csv`: they cover the whole selected-path process, including dynamic
loader, setup, path-specific warmup, timed region, and output. They are retained
separately and must not be interpreted as branch misses or resident bytes of the
timed loop alone.

## Controlled evidence collection

The portable default remains observation-only and requires no privileged
CPU-frequency change. Every newly emitted v2 attestation requires an explicit
permanent source tag resolving to the exact clean source commit:

```bash
BE13_SOURCE_REF=refs/tags/<permanent-source-tag> \
BE13_REPETITIONS=30 \
BE13_WARMUP=10000 \
BE13_ITERATIONS=500000 \
BE13_CPU=0 \
./scripts/collect-be13-portable-evidence.sh /tmp/be13-observed
```

The tag must already exist and resolve exactly to `git rev-parse HEAD`; an
untagged run is rejected before benchmark compilation. This keeps the source of
retained attested evidence reachable after branch deletion or squash merging.

On a reviewed host where the selected CPU exposes a writable CPUFreq policy,
controlled comparison evidence can be requested explicitly:

```bash
BE13_REPETITIONS=30 \
BE13_METRIC_REPETITIONS=10 \
BE13_WARMUP=10000 \
BE13_ITERATIONS=500000 \
BE13_CPU=0 \
BE13_FREQUENCY_MODE=lock-max \
BE13_PROCESS_METRICS=required \
BE13_SOURCE_REF=refs/tags/<permanent-source-tag> \
BE13_CODEGEN_PROFILE_EXPECTED=portable \
BE13_REQUIRE_QUALIFIED=1 \
./scripts/collect-be13-portable-evidence.sh /tmp/be13-controlled
```

The collector refuses a dirty worktree, a non-empty destination, or a missing
permanent source tag. Before assigning a `portable`, `native`, or `custom`
codegen profile it captures all supported build-context channels that can alter
this benchmark: `RUSTFLAGS`, `CARGO_ENCODED_RUSTFLAGS`, the host-target
`CARGO_TARGET_*_RUSTFLAGS`, `CARGO_BUILD_RUSTFLAGS`, `CARGO_BUILD_TARGET`,
`CARGO_INCREMENTAL`, every `CARGO_PROFILE_BENCH_*` environment override, Cargo
config files discovered from the workspace directory through all ancestors plus
Cargo home, and the effective `cargo rustc -- --print cfg` output. Bench-profile
overrides are retained in `build_env_inventory.txt`; only hashes and paths of
Cargo config files are retained, never their contents.

A run is classified `portable` only when those build-context channels are clean;
`native` additionally requires exactly `RUSTFLAGS='-C target-cpu=native'` and no
other captured override. Everything else is `custom`. Qualified collection
requires an explicit expected `portable` or `native` profile and fails closed on
mismatch.

After attestation, the collector builds the benchmark before applying any
CPUFreq lock, pins the benchmark to the selected CPU when `taskset` is
available, rotates path order, verifies the five semantic `True` results,
samples the CPUFreq policy during the campaign, and restores the original CPU
policy before finalizing metadata. A lock file prevents two BE13 collectors
from changing the same policy concurrently.

The retained-evidence validator supports historical v1/v2 evidence and the
current `cargo-build-context-v2` attestation. For current v2 attestation it
recomputes the codegen classification from the captured Rustflags channels,
bench-profile environment inventory and Cargo-config inventory; verifies the
permanent tag-to-SHA binding; validates the exact repetition/path matrices and
deterministic order; recomputes the timing stability summary from raw rows;
checks continuous frequency samples, policy restoration, process-counter
semantics and SHA-256 files; and checks the historical collector/helper/stability
analyzer blobs from the recorded source SHA. The evidence CI fetches full
history and tags so these bindings can be checked.

This benchmark performs no actuation and grants no validation authority. No
speedup, hardware, energy, or scientific-novelty claim is valid merely because a
v2 set is comparison-qualified; any such claim still requires review of the raw
data, measurement scope, semantic parity, and exact source SHA.

## Retained controlled evidence (2026-09-18)

The retained set
`benchmarks/be13/evidence/2026-09-18-jetson-agx-thor-aarch64-468eb52`
was collected from exact source
`468eb52f646cae0e70a7342cd9b7374ddb01ef15` with 30 timing repetitions and
10 process-metric repetitions per path. Its validator-derived facts are:

- `comparison_qualified=true`;
- 3,682 continuous CPUFreq samples, all reporting 2,601,000 kHz;
- CPUFreq policy restored to the recorded pre-collection values;
- all five paths pass the preregistered timing-drift gate;
- worst block-median spread ratio is approximately 0.069218 (6.92%), below the
  0.10 engineering limit;
- all 50 direct generalized branch-miss counter rows are measured;
- whole-process peak RSS is measured;
- allocation count remains explicit `unmeasured`.

These facts qualify the **measurement protocol** for controlled portable
comparison. They do not establish a speedup or an architecture-specific benefit.

## BE13e architecture-specific acceleration gate

BE13d controlled evidence now permits architecture-specific candidates to be
measured, but not assumed beneficial. The BE13e gate and retained portable vs
`target-cpu=native` comparison are documented in
[`boolean-be13-acceleration-gate.md`](boolean-be13-acceleration-gate.md). The
qualified decision keeps the portable production path and enables no SVE/SVE2
or native-codegen dispatch because the specialized probe did not dominate the
portable candidate on the target multiword paths.

### BE13e codegen provenance correction

PR #130's automated review identified that the first BE13e codegen labels were
post-hoc and that squash merging could make the measured source unreachable.
The source is now retained by a permanent tag, and the corrected collector
captures effective Cargo/rustc configuration during collection. A later
hardening pass closes additional build-context channels identified by automated
review: `CARGO_BUILD_RUSTFLAGS`, ancestor `.cargo/config*`,
`CARGO_PROFILE_BENCH_*`, and untagged attestation are now fail-closed inputs for
portable/native qualification. The attested replacement datasets and
conservative gate decision are documented in
`docs/boolean-be13-acceleration-gate.md`.
