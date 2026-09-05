# Model execution resource plan v1

Status: pre-execution contract. No physical model actuation is claimed by this document.

## Purpose

`elastic.model-execution.resource-plan@1.0.0` is a backend-neutral ElasticXxx boundary for conditional model execution. It represents three resource axes that a model/backend may explicitly qualify:

- active expert count;
- active expert width;
- activation-compute budget.

The contract does not assume that every MoE, recurrent model, ASSR experiment, or other architecture supports these axes. A provider must publish the exact discrete levels that are valid for an exact model revision before ElasticXxx accepts a plan.

## Capability identity

Capabilities use `elastic.model-execution.capabilities@1.0.0` and carry:

- `provider_id`;
- `model_revision`;
- total expert count;
- qualified active-expert counts;
- qualified expert-width levels;
- qualified activation-budget levels.

The validated capability object computes a deterministic structural fingerprint over this content. The fingerprint is non-cryptographic and is used only to detect accidental/stale capability mismatch inside the Elastic trust model; it is not an authentication primitive.

A resource-plan envelope carries the provider, model revision, and capability fingerprint. Revalidation fails closed if any identity differs.

## Exact units

Expert count is an integer in `1..=total_experts`.

Expert width and activation budget are integer basis points in `1..=10_000`:

- `10_000` means the provider-declared full level;
- `5_000` means one half of that provider-declared level;
- `2_500` means one quarter.

These numbers are configuration coordinates, not performance or quality claims. A lower value is legal only when the provider has explicitly published that exact value as qualified for the model revision.

For expert width, a provider should publish sub-widths only when the model/backend has a defined way to execute that nested width (for example an explicitly trained/qualified nested expert realization). ElasticXxx does not infer that property from an ordinary dense expert.

For activation budget, the provider owns the meaning of the full activation-compute envelope and must keep it stable for the capability fingerprint. ElasticXxx treats the value as an exact discrete resource coordinate and does not infer model quality from it.

## Elastic dimensions

The plan maps to the open-set generic resource model with these custom dimensions:

- `model-execution.active-expert-count`;
- `model-execution.expert-width-bps`;
- `model-execution.activation-budget-bps`.

They intentionally remain custom dimensions rather than new `elastic-core` built-ins. `DimensionId` is already extensible, and specialized model semantics do not belong in the generic core.

The mapped resource is configurational, preserves logical identity, upholds the versioned resource-plan contract, and declares free-capacity, utilization, and queue-depth observations as relevant inputs.

`TransitionMechanism::Reinterpret` is used only for the pre-execution declaration: the existing model materialization is configured to use another already-qualified execution level. This does not authorize changing a live model. A future backend-specific adapter must publish and validate a separate live-actuation capability, revalidate relevant invariants immediately before action, verify the post-action state, and participate in ElasticXxx commit/rollback semantics.

## ASSR / TDI boundary

This contract is deliberately architecture-neutral. It creates the ElasticXxx runtime seam needed by a future ASSR implementation without asserting that TDI-8.1 has already qualified elastic expert count, elastic expert width, or elastic activation as scientific model semantics.

TDI remains the owner of ASSR reference semantics and experimental evidence. ElasticXxx owns generic resource planning, capability matching, runtime actuation boundaries, verification, and rollback. A future ASSR integration should therefore publish its qualified execution levels from the TDI/downstream model implementation into this contract rather than moving ASSR semantics into `elastic-core`.

## Non-goals of v1

v1 does not:

- define an expert router;
- define Top-K or threshold-routing algorithms;
- train nested experts;
- define model-quality invariants;
- infer safe levels for existing models;
- move weights between CPU, GPU, RAM, or NVMe;
- alter numerical precision or representation;
- execute per-token dynamic changes;
- claim latency, throughput, memory, or accuracy improvements.

Those require separately qualified model/backend contracts and measured evidence.

## Public Rust surface

Typical construction is:

```rust
use elastic::{ModelExecutionCapabilitiesV1, ModelExecutionResourcePlanV1};

let capabilities = ModelExecutionCapabilitiesV1::new(
    "my-backend",
    "model-revision-123",
    64,
    vec![1, 2, 4],
    vec![2_500, 5_000, 10_000],
    vec![2_500, 5_000, 10_000],
)?;

let plan = ModelExecutionResourcePlanV1::new(
    &capabilities,
    2,
    5_000,
    5_000,
)?;

let spec = plan.resource_spec("inference-model")?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

The stable JSON media types are:

- `application/vnd.elastic.model-execution-capabilities.v1+json`;
- `application/vnd.elastic.model-execution-resource-plan.v1+json`.

Both wire structures reject unknown JSON fields. Native validation is reused after deserialization.
