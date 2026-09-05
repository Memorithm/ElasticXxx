# Correlated model-execution profile set v1

Status: pre-execution selection contract. No physical model actuation is claimed by this document.

## Purpose

`elastic.model-execution.profile-set@1.0.0` closes a correlation gap that intentionally remains outside the existing per-axis `elastic.model-execution.capabilities@1.0.0` contract.

The v1 axis contract can state that expert counts `{1, 2, 4}`, expert widths `{2500, 5000, 10000}` basis points, and activation budgets `{2500, 5000, 10000}` basis points are individually qualified. That does not prove that every Cartesian combination is a qualified model execution configuration.

A profile set therefore publishes complete tuples explicitly. For example:

- `full`: `(4, 10000, 10000)`;
- `balanced`: `(2, 5000, 5000)`;
- `minimal`: `(1, 2500, 2500)`.

The tuple `(4, 2500, 10000)` remains rejected by the correlation layer unless the provider publishes it as its own profile, even though each individual coordinate exists in the underlying axis capability sets.

## Validation

Every profile is validated through `ModelExecutionResourcePlanV1::new`. The correlation layer therefore cannot bypass the existing provider/model capability contract.

A profile set rejects:

- an empty profile set;
- blank profile identities;
- duplicate profile identities;
- duplicate provider preference ranks;
- duplicate complete tuples;
- any tuple rejected by the underlying model-execution capabilities.

The validated set is bound to the exact capability fingerprint and computes its own structural profile-set fingerprint. Both fingerprints are non-cryptographic structural identities and are not authentication primitives.

## Provider preference rank

Each profile carries a unique integer `preference_rank`; lower values are considered first.

The rank is provider-supplied policy data. ElasticXxx does not infer that a higher-compute profile has better quality, nor does it derive the ranking from parameter count, latency, accuracy, or another hidden objective.

This keeps model semantics with the backend/model owner while making selection deterministic and auditable.

## Explicit resource envelope

`ModelExecutionProfileEnvelopeV1` supplies three upper bounds:

- maximum active experts;
- maximum expert-width basis points;
- maximum activation-budget basis points.

The envelope is already resolved input. v1 does not guess those bounds from host RAM, GPU memory, power, thermal state, or latency observations because the mapping from physical observations to safe model limits remains backend-specific.

The reference `ModelExecutionProfileSelectorV1` scans profiles in provider preference order and selects the first complete tuple within all three bounds. If none fits, it returns `NoFeasibleProfile` rather than synthesizing a new tuple.

## Replay identity

A selected profile uses `elastic.model-execution.profile-plan@1.0.0` and is bound to:

- provider identity;
- exact model revision;
- base capability fingerprint;
- correlated profile-set fingerprint;
- selected profile identity.

Revalidation fails closed when either capability or profile-set identity changed.

## Public Rust example

```rust
use elastic::{
    ModelExecutionCapabilitiesV1, ModelExecutionProfileEnvelopeV1,
    ModelExecutionProfileSelectionV1, ModelExecutionProfileSelectorV1,
    ModelExecutionProfileSetV1, ModelExecutionProfileV1,
};

let capabilities = ModelExecutionCapabilitiesV1::new(
    "my-backend",
    "model-revision-123",
    64,
    vec![1, 2, 4],
    vec![2_500, 5_000, 10_000],
    vec![2_500, 5_000, 10_000],
)?;

let profiles = ModelExecutionProfileSetV1::new(
    &capabilities,
    vec![
        ModelExecutionProfileV1::new("full", 0, 4, 10_000, 10_000)?,
        ModelExecutionProfileV1::new("balanced", 10, 2, 5_000, 5_000)?,
        ModelExecutionProfileV1::new("minimal", 20, 1, 2_500, 2_500)?,
    ],
)?;

let envelope = ModelExecutionProfileEnvelopeV1::new(2, 5_000, 6_000)?;
let selection = ModelExecutionProfileSelectorV1.select(&profiles, envelope)?;

if let ModelExecutionProfileSelectionV1::Selected(plan) = selection {
    assert_eq!(plan.profile_id(), "balanced");
}

# Ok::<(), Box<dyn std::error::Error>>(())
```

## Non-goals

This contract does not:

- infer hardware-to-envelope mappings;
- inspect GPU or host memory directly;
- perform token routing;
- resize experts;
- move model weights;
- define ASSR or TDI scientific semantics;
- rank profiles by an inferred quality metric;
- actuate a live model;
- claim performance, memory, energy, or accuracy improvements.

A later backend-specific layer can resolve observations into a safe envelope and feed that envelope into this selector while preserving the same correlated-profile contract.
