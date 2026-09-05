# Model execution controller v1

`ModelExecutionControllerV1` is the high-level Rust assembly for the qualified adaptive model-execution path.

It does not define a model format, probe hardware, or implement CUDA/model kernels. A downstream integration still owns two explicit contracts:

- `ModelExecutionProfileBackendV1`: physical profile validation, apply, verification, and rollback;
- `ModelExecutionResourceTelemetryV1`: current resource telemetry plus provenance.

The controller composes those contracts with the existing ElasticXxx machinery:

```text
TransactionalModelExecution current profile
              +
ModelExecutionResourceObserverV1 telemetry
              |
              v
          ObserverSet
              |
              v
           Forecaster
              |
              v
ModelExecutionAdaptivePlannerV1
              |
              v
     correlated atomic profile
              |
              v
Runtime VALIDATE -> ACT -> VERIFY -> COMMIT / ROLLBACK
```

## Construction

The current-state convenience constructor is:

```rust
let mut controller = ModelExecutionControllerV1::current_state(
    "model-runtime",
    profiles,
    policy,
    backend,
    telemetry,
    CadenceConfig::OneShot,
    ExecutionModeConfig::Apply,
)?;
```

Use `ModelExecutionControllerV1::new` to supply another existing `Forecaster` implementation.

Construction fails closed when:

- backend provider/model/capability/profile-set identity does not match the supplied correlated profile set;
- the backend reports a current profile rank that is not published by that set;
- the envelope policy is not bound to the same profile set;
- the telemetry capacity unit cannot be bound to the policy;
- the atomic model resource cannot be constructed/lowered;
- periodic cadence has a zero interval or zero maximum-cycle bound.

## Cycles

`cycle()` executes one forecast-aware adaptive cycle through the existing trusted transaction runtime. `run()` executes the configured one-shot or bounded periodic loop and accepts the existing `CancellationToken`.

`current_profile_rank()` reads the actual current profile through the physical backend. It is not a cached planner value.

## Observation ownership

The controller does not introduce another observation-merging policy. `ModelExecutionObserverBundleV1` constructs the existing ordered `ObserverSet` for each observation pass:

1. `TransactionalModelExecution` publishes the current qualified profile rank;
2. `ModelExecutionResourceObserverV1` publishes `FREE_CAPACITY` and `UTILIZATION` from downstream typed telemetry.

All existing fail-closed and provenance rules therefore remain unchanged.

## Planning ownership

The controller does not infer that a particular amount of VRAM/RAM must correspond to a particular expert count, width, or activation budget. The validated `ModelExecutionEnvelopePolicyV1` remains authoritative for resource thresholds and the validated `ModelExecutionProfileSetV1` remains authoritative for complete correlated tuples.

## Physical ownership

A selected profile is still only an Elastic plan until `TransactionalModelExecution` validates it against the backend and runs the normal prepare/actuate/verify/commit lifecycle. Verification failure uses the same rollback path introduced for the transactional model backend.
