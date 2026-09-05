# Atomic model-execution profile transition v1

Status: runtime-planning bridge. No physical model actuation is claimed by this document.

## Purpose

A correlated model profile is one complete configuration tuple. Changing its active-expert count, expert width, and activation budget as independent runtime transitions would weaken the correlation contract introduced by `elastic.model-execution.profile-set@1.0.0`.

`elastic.model-execution.atomic-profile@1.0.0` therefore represents a runtime profile switch as one Elastic dimension:

`model-execution.profile`

The transition magnitude is the profile's unique provider-defined `preference_rank` inside one exact correlated profile-set fingerprint.

The rank is not a performance score. It is a compact deterministic target identifier whose meaning is valid only together with the exact profile-set fingerprint.

## Runtime resource

`ModelExecutionProfileSetV1::atomic_resource_spec(...)` produces a configurational Elastic resource that:

- has exactly one elastic dimension, `model-execution.profile`;
- preserves logical identity;
- upholds the atomic-profile contract;
- admits a capability-grounded `Reinterpret` transition on the atomic profile dimension;
- records provider, model revision, base capability fingerprint, and correlated profile-set fingerprint as labels;
- observes the current profile rank, free capacity, and utilization.

This resource does not replace the detailed three-axis resource declaration. The three-axis declaration describes the qualified model coordinates; the atomic runtime declaration describes how one complete qualified tuple is switched as a unit.

## Current-profile observation

The runtime observation signal is:

`model-execution.current-profile-rank`

The value must be an exact non-negative `u32` represented as a number. Fractional, negative, non-finite, or out-of-range values are rejected as insufficient evidence. No rounding is performed.

A `u32` rank is exactly representable by the planning context's `f64` observation value, so no integer precision is lost in this range.

If the observed current rank already equals the selected target rank, the planner returns `NoCandidate` rather than issuing a redundant transition.

## Atomic planner

`ModelExecutionAtomicProfilePlannerV1` is constructed from an already validated `ModelExecutionProfilePlanV1`.

Before producing a candidate it verifies that the EIR resource carries the exact:

- atomic-profile contract;
- provider id;
- model revision;
- capability fingerprint;
- profile-set fingerprint.

A mismatch fails closed with `InsufficientEvidence`.

When identities match and the current profile differs from the target, the planner selects the declared capability-grounded `Reinterpret@model-execution.profile` transition and attaches the target profile rank as its magnitude.

The resulting candidate is therefore consumable by the existing ElasticXxx planning/validation machinery without inventing a second planning representation.

## Why this precedes live actuation

ElasticXxx already owns the generic `TransactionalActuator` lifecycle:

`VALIDATE -> PREPARE -> ACTUATE -> VERIFY -> COMMIT / ROLLBACK`

A model-specific implementation should reuse that lifecycle. Creating a separate model transaction protocol would duplicate the runtime semantics and risk weakening rollback or invariant handling.

The atomic profile bridge solves the prerequisite representation gap: a complete correlated model profile can now appear as one generic runtime transition with one target magnitude.

A later backend adapter can bind that target rank back to the exact profile set and perform the physical switch through `TransactionalActuator`.

## Example

```rust
use elastic::{
    lower, model_execution_current_profile_rank_signal,
    ModelExecutionAtomicProfilePlannerV1, ModelExecutionCapabilitiesV1,
    ModelExecutionProfileEnvelopeV1, ModelExecutionProfileSelectionV1,
    ModelExecutionProfileSelectorV1, ModelExecutionProfileSetV1,
    ModelExecutionProfileV1, PlanningContext, TransitionPlanner,
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

let selection = ModelExecutionProfileSelectorV1.select(
    &profiles,
    ModelExecutionProfileEnvelopeV1::new(2, 5_000, 5_000)?,
)?;
let ModelExecutionProfileSelectionV1::Selected(target) = selection else {
    return Ok(());
};

let spec = profiles.atomic_resource_spec("model-runtime")?;
let eir = lower(&spec)?;
let resource = eir.resource("model-runtime").expect("resource present");
let planner = ModelExecutionAtomicProfilePlannerV1::new(&target);
let context = PlanningContext::new()
    .observe(model_execution_current_profile_rank_signal(), 0.0);

let outcome = planner.propose_transition_with_context(resource, &context);
assert!(outcome.declares_valid_candidate(resource));

# Ok::<(), Box<dyn std::error::Error>>(())
```

## Non-goals

v1 does not:

- perform a physical profile change;
- define a model backend;
- move weights;
- resize experts directly;
- route tokens;
- probe hardware;
- infer profile quality;
- commit or rollback physical state itself;
- alter the existing `TransactionalActuator` lifecycle;
- claim any performance, memory, energy, or accuracy improvement.

The next backend-facing slice should map the atomic target rank back to a profile and implement the existing transaction lifecycle, with action-time revalidation and post-actuation verification.
