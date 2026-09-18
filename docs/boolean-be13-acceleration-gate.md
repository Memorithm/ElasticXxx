# BE13e architecture-specific Boolean acceleration gate

Status: **qualified gate decision; portable path retained; no architecture-specific dispatch enabled; no speedup claim**.

BE13e asks whether the portable Boolean machinery should gain a CPU-specific
execution path after BE13d established controlled measurements. The gate is a
measurement and correctness decision, not an instruction to add SIMD by default.

## Qualified host and feature detection

The qualification host is the same NVIDIA Jetson AGX Thor / AArch64 environment
used for the retained BE13d measurements. Rust 1.89 reports `neon` as a
compile-time target feature. The runtime detector in `BooleanCpuFeatures`
reported:

```text
compile_time_neon=true
runtime_neon=true
runtime_sve=true
runtime_sve2=true
```

The detector is diagnostic only. Detection never authorizes a different Boolean
result, validation, or actuation path.

## Portable kernel change under test

Before considering a specialized path, the multiword conjunction/disjunction
kernel was simplified from two word traversals to one. The single pass maintains
separate accumulators for decisive evidence and missing evidence, then applies
the same strong-Kleene precedence:

- conjunction: any contradiction -> `False`; otherwise missing evidence ->
  `Unknown`; otherwise `True`;
- disjunction: any satisfying literal -> `True`; otherwise missing evidence ->
  `Unknown`; otherwise `False`.

The exhaustive semantic tests remain authoritative. Timing does not justify the
semantic change; the change is accepted only because the one-pass formulation is
semantically equivalent and structurally simpler.

## Retained controlled evidence

All three retained sets use the BE13 v2 collector with:

- 30 timing repetitions per path;
- 10 process-metric repetitions per path;
- CPU 0 affinity;
- CPUFreq `lock-max` control at 2,601,000 kHz;
- continuous frequency sampling and verified policy restoration;
- direct Linux generalized hardware branch-miss counters when available;
- whole-process peak RSS via `wait4(2)`;
- explicit `unmeasured` allocation counts;
- preregistered timing-stability threshold of 0.10.

The source sets are:

| Set | Source SHA | Codegen |
| --- | --- | --- |
| parent portable | `e3cb2141260dce36495db4b736d2654d4eac959f` | normal portable AArch64 |
| candidate portable | `e8268e81f882503a07dd7163fa97578d55f60514` | normal portable AArch64 |
| candidate native | `e8268e81f882503a07dd7163fa97578d55f60514` | `-C target-cpu=native` |

Observed median `ns_per_guard` values:

| Path | Parent portable | Candidate portable | Candidate native |
| --- | ---: | ---: | ---: |
| scalar_if_chain | 1.682631 | 1.682401 | 1.682480 |
| generic_bool_expr | 15.070390 | 16.428681 | 14.959818 |
| u64_compiled_guard | 1.830464 | 1.774945 | 1.831598 |
| multiword_guard | 8.941499 | 8.151631 | 8.240145 |
| batch_filter | 9.535709 | 6.784151 | 6.992903 |

Worst preregistered block-median spread ratios were `0.072670099`,
`0.073688569`, and `0.079590609` respectively; all remain below the existing
0.10 stability gate.

These values are retained decision evidence, not a public speedup claim. In
particular, unchanged paths also move between code layouts/build profiles, so
this table must not be interpreted as a causal percentage attribution to one
source edit.

## Gate decision

The native/SVE-capable build does not dominate the portable candidate on the
multiword paths that BE13e is intended to accelerate: its medians are slightly
higher for both `multiword_guard` and `batch_filter`, and `u64_compiled_guard`
is also higher. It does improve the generic AST path, demonstrating that
`target-cpu=native` changes code generation, but not in a uniformly beneficial
way for the target workload.

Therefore BE13e does **not** enable a `target-cpu=native`, SVE, SVE2, or explicit
SIMD dispatch in the production core. The normal portable path remains the
runtime path and fallback. No `unsafe`, nightly portable-SIMD dependency, or
architecture-specific semantic implementation is introduced.

A future specialized path may reopen the gate only with a new implementation,
semantic parity tests, controlled raw evidence, and a measured benefit on the
specific path it replaces.
