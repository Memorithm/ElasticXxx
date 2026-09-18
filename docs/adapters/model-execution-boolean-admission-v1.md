# Model-execution Boolean admission v1

Status: BE14c implementation contract. No performance, model-quality, hardware, or scientific-novelty claim.

## Purpose

The BE14c gate adds one Boolean eligibility decision in front of the existing correlated model-execution profile planner.

Stable predicate:

`elastic.model-execution::resource-envelope-available`

It answers only whether the current fresh resource snapshot satisfies at least one published `ModelExecutionEnvelopeRuleV1`.

It does **not** select a rule, a profile, or an actuation.

## Evidence sources

The predicate consumes the same generic signals already used by `ModelExecutionAdaptivePlannerV1`:

- `free-capacity`, in the policy's declared capacity unit;
- `utilization`, represented as a fraction in `0.0..=1.0`.

The gate requires valid finite observation records and exact agreement between those observation values and the planner-facing `PlanningContext`.

The typed context-to-snapshot conversion is shared with the numeric planner through `ModelExecutionAdaptivePlannerV1::resource_snapshot_from_context`.

Missing, provider-rejected, expired, future-dated, non-finite, inexact, unit-mismatched, or context-unbound evidence becomes `Unknown` and never authorizes planning.

A valid snapshot that matches no published envelope rule is `False`.

A valid snapshot matching at least one published rule is `True`.

## Execution order

For one BE14c cycle:

```text
OBSERVE ONCE
    ↓
CURRENT-STATE FORECAST
    ↓
DERIVE FACT SNAPSHOT
    ↓
BOOLEAN GUARD / PRUNING
    ├── False   → stop before numeric planner
    ├── Unknown → stop before numeric planner
    └── True
          ↓
    existing adaptive profile planner
          ↓
    trusted backend validation
          ↓
    apply
          ↓
    verify
       ↙     ↘
   commit   rollback
```

The physical telemetry provider is read once. The `True` path reuses the already captured `PlanningContext` and observation records through the forecast/runtime frozen-observation path; it does not perform a second physical observation.

## Authority boundary

Boolean `True` only permits the existing planner to continue.

`ModelExecutionAdaptivePlannerV1` remains responsible for deterministic policy-rule and correlated-profile selection.

`TransactionalModelExecution` remains the only boundary that may:

- revalidate the target profile;
- apply the physical profile;
- verify the resulting state;
- commit;
- restore the previous profile on failure.

A Boolean guard therefore cannot make an undeclared profile transition legal and cannot turn a failed trusted validation into an actuation.

## Evidence

`BooleanModelExecutionProfileReportV1` contains:

- stable predicate identity;
- source signal identities and units;
- `True / False / Unknown`;
- current-state forecast metadata;
- bounded `DecisionTrace` JSON;
- previous and final profile ranks;
- commit / rollback status;
- verification summary and runtime events;
- existing `ModelExecutionCycleEvidenceV1` JSON on completed trusted cycles.

The model-cycle evidence remains governed by its existing versioned contract. The BE14c report does not replace it.

## Qualification coverage

The public-facade integration tests cover:

1. `True` with a verified profile switch;
2. differential parity with the unguarded model controller;
3. `False` with no numeric-planner/backend call;
4. telemetry failure and expiry mapping to `Unknown`;
5. verification failure followed by successful rollback;
6. one physical telemetry read per guarded cycle;
7. expected `DecisionTrace` stop reasons.

No throughput or quality comparison is part of BE14c qualification.
