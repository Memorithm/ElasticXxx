# BE13e architecture-specific Boolean acceleration gate

Status: **qualified conservative gate; portable production path retained; no architecture-specific dispatch enabled; no speedup claim**.

BE13e asks whether the portable Boolean machinery should gain a CPU-specific
execution path after BE13d established controlled measurements. The gate is a
correctness/provenance/measurement decision, not an instruction to add SIMD.

## Production change under evaluation

The multiword conjunction/disjunction kernel was simplified from two word
traversals to one. The single pass keeps separate accumulators for decisive
and missing evidence, then preserves strong-Kleene precedence:

- conjunction: contradiction -> `False`; otherwise missing evidence ->
  `Unknown`; otherwise `True`;
- disjunction: satisfying literal -> `True`; otherwise missing evidence ->
  `Unknown`; otherwise `False`.

Exhaustive semantic tests remain authoritative. Timing does not justify the
semantic change.

`BooleanCpuFeatures` also exposes safe diagnostic feature detection. Detection
never changes Boolean semantics, validation or actuation authority.

## Provenance correction after PR #130 review

The first retained `e8268e8` portable/native comparison was collected under
controlled CPU-frequency conditions, but the `codegen_profile`/`rustflags`
labels were appended after collection rather than emitted by the collector.
Those two datasets remain archival timing evidence but are **not sufficient for
codegen attribution**.

The measured source `e8268e81f882503a07dd7163fa97578d55f60514` is retained by
the permanent tag:

```text
refs/tags/elasticxxx-be13e-source-e8268e8
```

The corrected collector source is:

```text
324152544479094f76112bb0f1419bb492e6c779
refs/tags/elasticxxx-be13e-source-32415254
```

The corrected collector emits, before checksumming the run:

- the permanent `source_ref`;
- a collector-derived `codegen_profile`;
- base64-encoded `RUSTFLAGS`, `CARGO_ENCODED_RUSTFLAGS`, and host-target
  `CARGO_TARGET_*_RUSTFLAGS`;
- `compiler_cfg.txt`, produced by `cargo rustc -p elastic-core --bench
  be13_portable -- --print cfg` under the same build environment;
- `cargo_config_inventory.txt` and its SHA-256;
- SHA-256 bindings for both files.

Qualified `portable/native` attestation currently requires the Cargo config
inventory to be `none`; otherwise the profile is not inferred because config
files may inject additional rustflags.

## Corrected attested evidence

Both corrected sets use source `324152544479094f76112bb0f1419bb492e6c779`,
30 timing repetitions per path, 10 PMU/RSS repetitions per path, CPU 0 affinity,
CPUFreq lock-max at 2,601,000 kHz, continuous frequency sampling, and verified
policy restoration.

The effective compiler cfg establishes:

```text
portable: target_feature="neon"
native:   target_feature="neon", target_feature="sve", target_feature="sve2", ...
```

Observed median `ns_per_guard` values:

| Path | Attested portable | Attested native |
| --- | ---: | ---: |
| scalar_if_chain | 1.682435 | 1.682804 |
| generic_bool_expr | 16.267042 | 14.950487 |
| u64_compiled_guard | 1.773483 | 1.831371 |
| multiword_guard | 8.255534 | 8.239687 |
| batch_filter | 6.816522 | 6.998007 |

The preregistered worst block-median spread ratios are `0.033795819` for the
portable build and `0.019796140` for the native build, both below the existing
0.10 stability gate.

These values are retained decision evidence, not a public speedup claim.

## Gate decision

`target-cpu=native` does not dominate the portable build on the workload BE13e
is intended to accelerate: it is slightly lower for `multiword_guard`, but
higher for `u64_compiled_guard` and `batch_filter`. It improves the generic AST
path, proving the effective codegen differs, but not in a uniformly beneficial
way for the target fast paths.

Therefore BE13e does **not** enable a `target-cpu=native`, SVE, SVE2, explicit
SIMD, or other architecture-specific production dispatch. The portable path
remains authoritative and fallback. No `unsafe` or nightly portable-SIMD
dependency is introduced.

A future specialized implementation may reopen the gate only with semantic
parity tests, source-reachable attested evidence, and a measured benefit on the
specific path it replaces.
