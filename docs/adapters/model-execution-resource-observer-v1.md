# Model-execution resource telemetry observer v1

`ModelExecutionResourceObserverV1<T>` is the runtime boundary between backend-owned resource telemetry and the generic observations consumed by `ModelExecutionAdaptivePlannerV1`.

It does **not** probe CUDA, GPU drivers, host memory, accelerators, or model state on its own. A downstream provider implements `ModelExecutionResourceTelemetryV1` and supplies explicit `ObservationSource` provenance plus typed resource telemetry.

The observer then publishes:

- `FREE_CAPACITY` in the policy-declared native capacity unit;
- `UTILIZATION` as a `0.0..=1.0` runtime fraction.

## Measurement time and freshness

`ModelExecutionResourceTelemetrySampleV1` carries:

- one validated `ModelExecutionResourceSnapshotV1`;
- the monotonic instant at which that snapshot was actually measured;
- an optional provider-owned `valid_until` instant.

The observer preserves `observed_at` on the emitted runtime observations. It does not replace a cached or remote measurement timestamp with the later time at which ElasticXxx happened to consume the value.

A sample is rejected from `PlanningContext` when:

- `observed_at` is in the future relative to evaluation time;
- `valid_until` precedes `observed_at`;
- evaluation occurs after `valid_until`.

Both planner-facing resource signals are then emitted as unsupported. The stale value remains auditable, but it cannot authorize a model-execution transition.

`valid_until` is deliberately provider-owned. ElasticXxx does not invent a universal acceptable age for GPU, accelerator, process, host, remote, or simulator telemetry.

### Existing synchronous providers

The trait remains source-compatible for existing direct/live providers. The default `sample()` implementation calls `snapshot()` and wraps the returned value with `ModelExecutionResourceTelemetrySampleV1::current(...)`.

That default is appropriate only when `snapshot()` actually performs or returns a current synchronous measurement.

A provider that serves cached, asynchronous, buffered, or remote telemetry **must override `sample()`** if it needs freshness to be enforceable. It must preserve the real measurement instant and should attach `valid_until` whenever its domain has an explicit freshness bound. ElasticXxx cannot infer hidden cache age from the legacy `snapshot()` return value alone.

## Capacity-unit binding

Construct the observer with the capacity unit of the exact envelope policy:

```text
ModelExecutionResourceObserverV1::new(policy.capacity_unit(), telemetry)
```

If the provider later returns a snapshot with a different capacity-unit identity, free capacity is emitted as unsupported and is omitted from `PlanningContext`. The utilization observation remains available because it is dimensionless, but the adaptive planner cannot make a resource decision without the missing free-capacity signal.

## Numeric precision

`ModelExecutionResourceSnapshotV1` stores free capacity as `u64`, while the generic EIR planning context stores numeric observations as `f64`. The observer refuses to insert free-capacity integers above `2^53` into the planning context because they cannot all be represented exactly by `f64`.

No rounding is performed. The unsupported observation remains visible in audit evidence.

## Telemetry failures

A provider error produces explicit unsupported observations for both planner-facing signals. No zero or default telemetry value is fabricated.

## Composition with current model state

`TransactionalModelExecution<B>` already implements `Observer` for the currently active correlated profile rank. Use the existing ordered `ObserverSet` to compose both providers:

```text
model observer (current profile rank)
        +
resource telemetry observer (free capacity + utilization)
        |
        v
ObserverSet
        |
        v
ModelExecutionAdaptivePlannerV1
```

`ObserverSet` preserves all observations for audit and gives the first registered provider authority if two providers claim the same planner-facing signal.

This keeps model state and hardware/resource telemetry as distinct ownership boundaries while producing the single `PlanningContext` required by the adaptive planner. In the assembled controller, expired timestamped resource telemetry therefore yields insufficient planning evidence and cannot reach physical actuation or commit.
