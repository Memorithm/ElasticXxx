# Elastic Runtime Configuration Schema v1

`OperatorConfig` v1 is the versioned JSON configuration consumed by the
Elastic runtime and by `elastic run --config`.

The schema configures concrete adapters and controllers. It does **not** define
a second semantic resource language: resource class, dimensions, invariants,
objectives, admitted transitions, capabilities, and observation vocabulary come
from the selected adapter's canonical `ResourceSpec`/EIR declaration.

## Run it

The repository includes `docs/config/operator-v1.example.json`.

```text
elastic run --config docs/config/operator-v1.example.json
```

The example uses `"mode": "dry-run"`, so it plans and performs trusted
validation without applying a physical resource transition. Change the mode to
`"apply"` only when physical adaptation is intended.

When a file contains multiple controllers, `elastic run --config FILE` executes
them in canonical resource-id order. To select one controller:

```text
elastic run --config FILE --resource ram-budget
```

Each controller has its own trusted transaction boundary; the CLI does not
claim cross-resource atomicity.

## Top-level object

```json
{
  "version": 1,
  "resources": [],
  "controllers": []
}
```

Unknown fields are rejected. `version` must currently be the integer `1`.

## Resources

### RAM

```json
{
  "adapter": "ram",
  "id": "ram-budget",
  "host_total": 1073741824,
  "min": 67108864,
  "max": 1073741824,
  "initial": 268435456,
  "max_step": 134217728
}
```

Required bounds are `0 < min <= initial <= max <= host_total`. `max_step` is
optional and limits the absolute size of one resize.

### Concurrency

```json
{
  "adapter": "concurrency",
  "id": "workers",
  "max_width": 16,
  "initial_width": 4
}
```

Required bounds are `0 < initial_width <= max_width`.

Resource IDs must be valid `LogicalResourceId` values and must be unique.

## Controllers

Each controller references exactly one configured resource:

```json
{
  "resource": "ram-budget",
  "planner": {
    "kind": "headroom",
    "headroom_fraction": 0.5,
    "deadband_fraction": 0.05
  },
  "forecaster": {
    "kind": "ewma",
    "alpha": 0.5,
    "horizon_ms": 1000
  },
  "cadence": {
    "kind": "one-shot"
  },
  "mode": "dry-run"
}
```

A resource may have at most one configured controller in v1.

### Planner selection

Supported planner objects are:

```json
{ "kind": "first-grounded" }
```

```json
{
  "kind": "headroom",
  "headroom_fraction": 0.5,
  "deadband_fraction": 0.05
}
```

```json
{
  "kind": "threshold",
  "low_watermark": 0.3,
  "high_watermark": 0.8,
  "step_fraction": 0.2
}
```

`headroom` and `threshold` are capacity planners and are rejected for a
concurrency resource. `first-grounded` is target-free: it is accepted for
`observe-only`/`plan-only` exploration but rejected for `dry-run` and `apply`,
because quantitative reference adapters require an explicit target magnitude.
The runtime never invents that target.

### Forecast selection

Current-state projection:

```json
{ "kind": "current-state" }
```

EWMA projection:

```json
{
  "kind": "ewma",
  "alpha": 0.5,
  "horizon_ms": 1000
}
```

EWMA requires finite `alpha` with `0 < alpha <= 1`. Forecasting remains
advisory evidence; unsupported or inconclusive forecast evidence cannot authorize
a transition.

### Cadence

One shot:

```json
{ "kind": "one-shot" }
```

Bounded periodic execution:

```json
{
  "kind": "periodic",
  "interval_ms": 1000,
  "max_cycles": 20
}
```

Periodic execution requires both `interval_ms > 0` and `max_cycles > 0`.
There is no `max_cycles = 0` infinite-loop convention in v1.

### Execution mode

Supported strings are:

- `"observe-only"`: collect observations only;
- `"plan-only"`: observe, forecast, and plan without trusted validation;
- `"dry-run"`: include trusted validation, but stop before physical actuation;
- `"apply"`: permit the full validate → actuate → verify → commit/rollback path.

## Complete example

See [`operator-v1.example.json`](operator-v1.example.json) for the maintained
executable example. CI parses equivalent v1 JSON and exercises the configured
runtime path so schema drift is detected by workspace tests.
