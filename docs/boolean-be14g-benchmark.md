# BE14g portable batch/device admission benchmark protocol

Status: **portable benchmark harness; no performance claim**.

This harness makes one bounded BE14g host-side overhead surface measurable before any performance statement is allowed. It uses a deterministic synthetic capacity snapshot and an explicit test provider. It does not execute a production placement backend, a physical device, or Hub orchestration.

The fixture declares two candidates. `preferred` requests batch size 8 on `device-a` with provider preference score 1, while the capacity sample exposes only 2 batch-items. `survivor` requests batch size 4 on `device-b` with score 10, while the capacity sample exposes 8 batch-items. The fixed guarded path must therefore prune `preferred`, select `survivor`, and commit the same local state as the reference path before timing begins.

Measured paths:

- `unguarded_exact_transaction`: the caller supplies the exact already-declared `survivor` candidate and runs the non-Boolean reference trusted transaction against fresh capacity;
- `guarded_preplan_trace_transaction`: runs Boolean capacity screening and numeric survivor ranking, captures the strict durable decision trace, rebinds it against the same fresh capacity snapshot, then runs the same trusted transaction boundary.

The unguarded path is deliberately an exact-candidate reference; it does **not** perform an alternative ranking algorithm. Therefore the timing difference includes Boolean screening/ranking and trace construction/revalidation that are absent from the reference path. It must not be interpreted as an intrinsic placement-algorithm speed comparison.

Example:

```bash
cargo +1.89.0 bench -p elastic-runtime --bench be14g_batch_device -- \
  --warmup 1000 --iterations 10000
```

A single path can be selected with `--path unguarded_exact_transaction` or `--path guarded_preplan_trace_transaction`.

The benchmark emits raw elapsed nanoseconds, iteration count, nanoseconds per iteration and a shared outcome-sanity value. Both paths must produce the same outcome-sanity value before measurements are accepted.

## Interpretation boundary

The emitted timings are observations for this synthetic host-side fixture only. They do not measure production placement actuation, scheduler behavior, Hub leases/fencing/transport, GPU/CPU device execution, throughput, bandwidth, memory footprint, energy, allocation counts, branch misses, numerical accuracy, model quality, placement optimality or end-to-end workload performance. The workflow is a regression/measurement surface, not evidence that Boolean admission accelerates or improves placement.
