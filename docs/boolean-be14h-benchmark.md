# BE14h portable thermal/energy transaction benchmark protocol

Status: **portable benchmark harness; no performance or energy claim**.

This harness makes one bounded BE14h host-side overhead surface measurable after qualification of the trusted thermal/energy transaction. It uses a deterministic synthetic observation snapshot and an explicit in-process test provider. It does not control a fan, clock, power mode, scheduler, thermal zone, hardware power limit, CPU, GPU or other physical device.

The fixed fixture declares one `Reinterpret` transition on the energy dimension. Its policy requires a thermal margin of at least 8 °C and an energy rate of at most 75 W. The synthetic source-bound observations are 12 °C and 60 W. A deliberately long one-hour freshness bound keeps the immutable benchmark fixture valid while repeated host-side transactions are measured; that bound is benchmark policy metadata, not a recommended operating threshold.

Measured paths:

- `unguarded_numeric_transaction`: evaluates the source-bound numeric policy directly, including the same action-time freshness check, then runs the trusted test-provider `VALIDATE -> ACT -> VERIFY -> COMMIT` lifecycle;
- `guarded_preplan_trace_transaction`: first derives the Boolean eligibility decision and durable decision trace, then independently re-evaluates the same source-bound numeric policy and runs the same trusted test-provider lifecycle.

Before timing begins, the harness requires both paths to commit the same declared transition, observation epoch and resource generation and to execute the same backend lifecycle counts. The guarded trace is explanatory evidence only and is intentionally absent from the unguarded reference result.

Example:

```bash
cargo +1.89.0 bench -p memorithm-elastic-runtime --bench be14h_thermal_energy -- \
  --warmup 1000 --iterations 10000
```

A single path can be selected with `--path unguarded_numeric_transaction` or `--path guarded_preplan_trace_transaction`.

The benchmark emits raw elapsed nanoseconds, iteration count, nanoseconds per iteration and a shared outcome-sanity value. These are host-side observations for this exact synthetic fixture only.

## Interpretation boundary

The timing difference includes Boolean fact derivation, guard evaluation and durable trace capture that are absent from the direct numeric reference. It is not an intrinsic solver or hardware-control comparison. The harness does not measure physical actuation, thermal response, energy consumption or savings, fan/clock control, power-cap effectiveness, GPU/CPU throughput, memory traffic, allocations, branch misses, model accuracy, model quality or end-to-end workload performance. It authorizes no safe operating threshold and no production placement or actuator choice.
