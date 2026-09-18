# BE14f portable kernel-realization benchmark protocol

Status: **portable benchmark harness; no performance claim**.

This benchmark makes the BE14f Boolean kernel-admission overhead measurable before any performance statement is allowed. It compares a non-Boolean kernel planner plus the trusted transaction boundary with the Boolean capability preplanner, durable trace construction/checking, the same numerical planner, and the same trusted transaction boundary. It is not a physical-kernel benchmark.

The fixture is intentionally synthetic and deterministic. It declares two realizations of one logical kernel: `too-large` requires 64 KiB of workgroup storage while the fixture capability limit is 32 KiB, and `portable` requires 1 KiB. The lower static-latency `too-large` candidate is therefore ineligible; both paths must resolve `portable` under the same contract and capability snapshot before timing.

The two measured paths are:

- `unguarded_plan_transaction`: run the existing deterministic numerical/capability-aware planner, then execute `execute_kernel_transaction` with the test provider;
- `guarded_boolean_plan_transaction`: run `plan_with_boolean_admission_traced`, require its durable selection trace, then execute `execute_guarded_kernel_transaction` with the same test provider.

The test provider implements the authoritative validate/activate/verify/commit interface without invoking a physical device. Both paths are checked for an identical committed outcome before timing. Warmup and iteration counts are explicit CLI inputs, and the benchmark emits raw elapsed nanoseconds, iteration count, nanoseconds per iteration, and a small outcome-sanity value.

Example:

```bash
cargo +1.89.0 bench -p elastic-kernel --bench be14f_kernel_realization -- \
  --warmup 1000 --iterations 10000
```

A single path can be selected with `--path unguarded_plan_transaction` or `--path guarded_boolean_plan_transaction`.

## Interpretation boundary

These timings are observations for this synthetic host-side planning plus test-provider transaction fixture only. They do not measure physical kernel compilation or execution, GPU behavior, throughput, bandwidth, memory footprint, energy, allocation counts, branch misses, numerical accuracy, model quality, or end-to-end workload performance. They must not be used to claim that Boolean admission accelerates kernel realization.

The benchmark does not create actuation authority. Boolean eligibility can only reduce the candidate set; exact candidate binding, fresh-capability revalidation, backend validation, verification, commit, and rollback remain authoritative in the production transaction path.
