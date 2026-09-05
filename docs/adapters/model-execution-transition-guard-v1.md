# Model execution transition guard v1

ElasticXxx model profiles describe complete correlated execution configurations. A specialized backend may publish multiple qualified configurations without supporting an in-place transition between already-materialized model instances.

`elastic-runtime` therefore exposes a backend-neutral transition authorization layer:

- `ModelExecutionTransitionModeV1::LiveTransactional` means the backend authorizes the target transition through the existing validate/apply/verify/rollback lifecycle;
- `ModelExecutionTransitionModeV1::ModelRebuildRequired` means the target requires construction of another model/runtime instance and is not admissible to `TransactionalModelExecution` as a live profile switch;
- `ModelExecutionTransitionPolicyV1` determines the current transition mode from a freshly observed current profile rank and complete target profile;
- `FixedModelExecutionTransitionPolicyV1` represents backends whose currently qualified profile set shares one transition class;
- `TransitionGuardedModelExecutionBackendV1<B, P>` decorates an existing `ModelExecutionProfileBackendV1` without introducing another runtime state machine.

## Fail-closed placement

`TransactionalModelExecution` already invokes `ModelExecutionProfileBackendV1::validate_profile` during trusted validation, again during transaction preparation, and immediately before `apply_profile`.

The transition guard reuses that existing contract. It rejects every non-`LiveTransactional` transition from `validate_profile`, so a specialized adapter cannot reach physical application through the trusted runtime while declaring `ModelRebuildRequired`.

The wrapper repeats the authorization check inside its own `apply_profile` implementation as a defense against direct wrapper use outside `TransactionalModelExecution`.

Rollback is deliberately not transition-policy gated. If physical application has already happened, ElasticXxx must still attempt the backend's existing `restore_profile` path rather than refuse recovery because policy changed after the action.

## NNIS boundary

NNIS PR #117 publishes `F16ExecutionTransitionRequirementsV1` for the current F16 execution plans. Its schema-v1 contract declares `model_rebuild_required`, requires source logical weights, does not preserve active sessions or KV state, and does not authorize live in-place transition.

An external NNIS adapter can map that validated NNIS-owned contract to:

```text
FixedModelExecutionTransitionPolicyV1::model_rebuild_required()
```

and wrap its Elastic model backend with `TransitionGuardedModelExecutionBackendV1`.

This mapping does not add an NNIS dependency to ElasticXxx and does not add an ElasticXxx dependency to NNIS. The specialized repository remains the source of truth for whether a plan is live-switchable.

The current NNIS F16 contract therefore must fail closed before `apply_profile` in the Elastic transactional path. It is not evidence that model rebuild, active-session migration, KV migration, or rebuild-based rollback exists.

## Existing backend compatibility

This feature does not change `ModelExecutionProfileBackendV1` and does not alter existing `TransactionalModelExecution` behavior. Existing backend implementations that already provide physical `apply_profile` and `restore_profile` remain source-compatible.

A transition guard is added when a specialized backend has an independent, versioned transition-authorization contract that must be enforced before physical application.

## Scope

This contract does not:

- implement NNIS model reconstruction;
- make NNIS F16 plans live-switchable;
- define NNIS kernel or model semantics;
- define MoE expert-count, expert-width, or activation-budget semantics;
- migrate sessions or KV state;
- replace backend-specific feasibility validation;
- make latency, throughput, memory, quality, or scientific claims.

The next physical-integration step requires a specialized backend transition that is genuinely `LiveTransactional`, or a separate explicitly designed rebuild transaction class with its own quiescence, state-migration, verification, and rollback semantics.
