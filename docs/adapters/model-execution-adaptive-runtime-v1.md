# Adaptive model-execution runtime planner v1

`ModelExecutionAdaptivePlannerV1` composes the already-qualified model-execution contracts into one runtime planning step:

```text
PlanningContext
  FREE_CAPACITY + UTILIZATION + current profile rank
        |
        v
ModelExecutionResourceSnapshotV1
        |
        v
ModelExecutionEnvelopePolicyV1
        |
        v
ModelExecutionProfileSetV1
        |
        v
atomic model-execution profile candidate
```

The planner does not infer a universal relationship between hardware resources and model quality. The backend/operator still owns:

- the `capacity_unit` named by the envelope policy;
- resource thresholds;
- rule ordering;
- correlated profile definitions;
- physical profile-switch semantics.

## Observation semantics

The adaptive planner consumes the generic EIR signals already used by ElasticXxx planners:

- `FREE_CAPACITY`: an exact non-negative integer in the policy's declared native capacity unit;
- `UTILIZATION`: a finite fraction in `0.0..=1.0`;
- `model-execution.current-profile-rank`: the currently active correlated profile rank.

Because `PlanningContext` stores numeric observations as `f64`, `FREE_CAPACITY` is accepted only through `2^53`, the largest integer with exact IEEE-754 representation. Values above that bound are rejected instead of silently losing integer precision.

`UTILIZATION` is explicitly quantized to the nearest integer basis point before constructing `ModelExecutionResourceSnapshotV1`, because the envelope policy uses basis-point thresholds.

## Decision semantics

For each planning call the planner:

1. validates the required observations;
2. constructs a typed resource snapshot using the policy's capacity-unit identity;
3. resolves the backend-supplied envelope policy;
4. selects only a complete profile already present in the exact correlated profile set;
5. delegates the selected plan to `ModelExecutionAtomicProfilePlannerV1`;
6. emits at most one capability-grounded `model-execution.profile` transition.

`NoMatchingRule` becomes `PlanOutcome::NoCandidate`. Missing/invalid observations, stale policy identity, or an impossible matched rule fail closed as `PlanOutcome::InsufficientEvidence`.

## Transactional execution

When paired with `TransactionalModelExecution<B>`, the resulting runtime path is:

```text
OBSERVE resource state
  -> adaptive profile selection
  -> atomic profile plan
  -> VALIDATE
  -> PREPARE
  -> ACTUATE
  -> VERIFY
  -> COMMIT / ROLLBACK
```

A downstream TDI/ASSR, NNIS, or other backend may implement `ModelExecutionProfileBackendV1`, but this contract does not claim that any such backend is already qualified.
