# BE14e portable representation-admission benchmark protocol

Status: **portable benchmark harness; no performance claim**.

This benchmark compares the exact BE14e fixed-width Boolean admission path with a non-Boolean declared-transition baseline. It exists to make the control-layer cost measurable before any performance statement is allowed. It is not a codec benchmark and it does not close the broader representation/precision problem by itself.

The fixture is intentionally narrow and deterministic. The current representation is `tensor.fp16` schema 1. The policy considers `tensor.int4` at four declared bits first and `tensor.int8` at eight declared bits second. The required fixed-width floor is exactly eight declared bits, so the first candidate is `False` and the second is `True`. Both paths end at the same `tensor.int8` structural transition and both execute the same authoritative `RepresentationTransition::validate` boundary with the same capability snapshot and re-encoder attestation.

The two measured paths are:

- `unguarded_declared_validate`: derive the already-declared `tensor.int8` target and execute authoritative structural validation;
- `guarded_preplan_validate`: run the BE14e Boolean screening over the false-then-true candidate ladder, resolve the selected transition, then execute the same authoritative structural validation.

The benchmark prints raw elapsed nanoseconds, iteration count, nanoseconds per iteration and a small target-identity sanity value. The two paths are checked for the same selected target before timing. Warmup and iteration counts are explicit command-line inputs. The dedicated CI smoke runs both paths from the exact pull-request head on the trusted ElasticXxx ARM64 runner and validates the emitted schema and path order.

Example:

```bash
cargo +1.89.0 bench -p memorithm-elastic-runtime --bench be14e_representation_precision -- \
  --warmup 1000 --iterations 10000
```

A single path can be selected with `--path unguarded_declared_validate` or `--path guarded_preplan_validate`.

## Interpretation boundary

These timings are observations for this synthetic host-side planning/validation fixture only. They do not measure physical u16 byte-order re-encoding, representation size reduction, bandwidth, device transfer, GPU behavior, numerical error, model quality, energy, allocation counts, peak memory, branch misses, or end-to-end workload performance. They must not be used to claim that Boolean admission accelerates representation changes. A later real-consumer performance claim requires a separately retained, source-bound, reproducible measurement campaign with the relevant physical operation and confounders recorded.

The authoritative correctness boundary remains `RepresentationTransition::validate`; the benchmark cannot authorize actuation and cannot convert a Boolean `True` into a legal transition without trusted validation.
