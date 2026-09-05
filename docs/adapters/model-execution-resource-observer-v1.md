# Model-execution resource telemetry observer v1

`ModelExecutionResourceObserverV1<T>` is the runtime boundary between backend-owned resource telemetry and the generic observations consumed by `ModelExecutionAdaptivePlannerV1`.

It does **not** probe CUDA, GPU drivers, host memory, accelerators, or model state on its own. A downstream provider implements `ModelExecutionResourceTelemetryV1` and returns:

- explicit `ObservationSource` provenance;
- a validated `ModelExecutionResourceSnapshotV1`;
- or a backend-specific error.

The observer then publishes:

- `FREE_CAPACITY` in the policy-declared native capacity unit;
- `UTILIZATION` as a `0.0..=1.0` runtime fraction.

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

This keeps model state and hardware/resource telemetry as distinct ownership boundaries while producing the single `PlanningContext` required by the adaptive planner.
