# Model execution envelope policy v1

Status: backend-supplied pre-execution planning contract. No physical model actuation is claimed by this document.

## Purpose

`elastic.model-execution.envelope-policy@1.0.0` connects explicit resource observations to the correlated model-execution profile contract.

The flow is:

`trusted resource snapshot -> versioned backend rule -> profile envelope -> correlated profile selector -> validated model-execution plan`

This is the first hardware-guided planning seam for the model-execution work, but hardware semantics remain outside the generic Elastic core. ElasticXxx does not infer that a particular amount of RAM, VRAM, free capacity, utilization, thermal margin, or power headroom makes a particular model configuration safe.

## Resource snapshot

`ModelExecutionResourceSnapshotV1` carries:

- a backend-owned `capacity_unit` string;
- an observed `free_capacity` integer in that exact unit;
- utilization in integer basis points from `0` through `10_000`.

The snapshot is supplied by a trusted observer/backend. This contract does not probe the operating system, CUDA, a GPU driver, or device memory directly.

The capacity-unit identity is opaque to ElasticXxx. Examples such as `bytes` or `cuda-device-bytes` may be chosen by a backend, but the generic implementation only enforces exact unit identity equality between the policy and the observation. It does not convert units or assign hardware meaning to them.

## Backend rules

Each `ModelExecutionEnvelopeRuleV1` contains:

- a stable rule id;
- a unique provider-defined preference rank;
- a minimum free-capacity threshold;
- a maximum utilization threshold;
- one `ModelExecutionProfileEnvelopeV1`.

A rule matches only when both conditions hold:

- `free_capacity >= min_free_capacity`;
- `utilization_bps <= max_utilization_bps`.

Rules are evaluated in provider-defined preference order. ElasticXxx does not reorder them according to an inferred performance, quality, memory, energy, or accuracy objective.

Policy construction rejects a rule whose envelope cannot select any profile from the exact correlated profile set. This keeps the hardware-policy table grounded in configurations that the backend has actually published.

## Identity and staleness

A validated policy is bound to:

- `provider_id`;
- exact `model_revision`;
- the base model-execution capability fingerprint;
- the correlated profile-set fingerprint;
- the declared capacity unit;
- the complete ordered rule table.

The policy computes its own structural fingerprint. Structural fingerprints are non-cryptographic identities and must not be used for authentication.

Selection fails closed if the policy is presented with a different provider, model revision, capability set, profile set, or capacity unit.

## Deterministic hardware-guided selection

`ModelExecutionHardwarePlannerV1` performs two deterministic stages:

1. find the first provider rule matching the supplied resource snapshot;
2. pass that rule's envelope to `ModelExecutionProfileSelectorV1`.

The second stage preserves the complete-tuple correlation contract. The hardware policy therefore cannot synthesize a new `(active_experts, expert_width_bps, activation_budget_bps)` combination.

Outcomes are explicit:

- `Selected`: a matching rule produced one published correlated profile;
- `NoMatchingRule`: no backend rule covers the observed state;
- `NoFeasibleProfile`: retained as a fail-closed outcome if profile identity changes or later implementations alter feasibility behavior.

## Example

```rust
use elastic::{
    ModelExecutionCapabilitiesV1, ModelExecutionEnvelopePolicyV1,
    ModelExecutionEnvelopeRuleV1, ModelExecutionHardwarePlannerV1,
    ModelExecutionHardwareSelectionV1, ModelExecutionProfileEnvelopeV1,
    ModelExecutionProfileSetV1, ModelExecutionProfileV1,
    ModelExecutionResourceSnapshotV1,
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

let policy = ModelExecutionEnvelopePolicyV1::new(
    &profiles,
    "bytes",
    vec![
        ModelExecutionEnvelopeRuleV1::new(
            "rich",
            0,
            8_000,
            7_000,
            ModelExecutionProfileEnvelopeV1::new(4, 10_000, 10_000)?,
        )?,
        ModelExecutionEnvelopeRuleV1::new(
            "balanced",
            10,
            2_000,
            9_000,
            ModelExecutionProfileEnvelopeV1::new(2, 5_000, 5_000)?,
        )?,
    ],
)?;

let snapshot = ModelExecutionResourceSnapshotV1::new("bytes", 3_000, 8_000)?;
let result = ModelExecutionHardwarePlannerV1.select(&policy, &profiles, &snapshot)?;

if let ModelExecutionHardwareSelectionV1::Selected { plan, .. } = result {
    assert_eq!(plan.profile_id(), "balanced");
}

# Ok::<(), Box<dyn std::error::Error>>(())
```

The numerical threshold values in this example are illustrative configuration values only. They are not hardware recommendations and do not establish performance, memory, quality, or safety properties for a real model.

## Reuse across the Memorithm ecosystem

The contract is deliberately backend-neutral so the same pattern can be consumed by future TDI/ASSR, NNIS, SLHAv2, or other specialized runtimes without moving their semantics into ElasticXxx. Each backend remains responsible for publishing its own observation units, qualified rule thresholds, correlated model profiles, and evidence.

This reuse is architectural only. No current TDI, NNIS, SLHAv2, or ASSR implementation is claimed here to publish this contract until its repository explicitly does so.

## Non-goals

v1 does not:

- probe host RAM or accelerator memory;
- query CUDA or another device runtime;
- translate physical bytes into safe model widths automatically;
- define thermal or power control;
- learn policy thresholds;
- infer model quality;
- move weights or resize experts;
- route tokens;
- actuate a live model;
- make latency, throughput, memory, energy, or accuracy claims.

A future backend adapter may supply real observations and qualified rules to this contract. Live actuation remains a separate step that must revalidate invariants, verify the resulting state, and participate in ElasticXxx commit/rollback semantics.
