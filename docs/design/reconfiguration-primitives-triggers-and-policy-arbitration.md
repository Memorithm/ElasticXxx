# Reconfiguration Primitives, Triggers, and Policy Arbitration

## Status

**ELASTIC PROPOSAL / RESEARCH DESIGN NOTE.**

This note synthesizes Trisk (SoCC 2021), StreamOps (PVLDB 2023), Chi, Fries, Megaphone, Meces, and earlier ElasticXxx work on typed transitions, control-plane separation and consistency closures. It does not claim novelty for policy/mechanism separation, primitive reconfiguration APIs, triggers, or priority arbitration.

## 1. Three different objects

Do not collapse:

```text
PolicyRecommendation
TransitionProgram
PrimitiveOperation
```

A policy expresses a desired intervention. A validated transition program defines how to realize it. Primitive operations are the small actuator-facing steps used by that program.

Conceptually:

```text
Policy / Planner
    ↓
Recommendation
    ↓
Trusted validation
    ↓
TransitionProgram
    ↓
Primitive operations
    ↓
Actuators
```

## 2. Primitive-operation prior art

Trisk directly exposes low-level operations for synchronization, resource updates, key mapping, state transfer and function updates beneath an abstract execution-plan API.

Therefore composable runtime primitives are prior art.

ElasticXxx should investigate a more domain-independent typed contract rather than claim the mechanism itself.

## 3. Candidate primitive contract

Not yet an API commitment:

```text
ReconfigurationPrimitive {
    id,
    preconditions,
    required_capabilities,
    effects,
    required_consistency,
    estimated_cost,
    idempotency,
    cancellation_semantics,
    apply,
    verify,
    rollback_or_compensate,
}
```

Different resource domains may expose different concrete primitives. The core should not require one universal imperative instruction set if doing so erases domain semantics.

## 4. Effect sets

Primitive composition needs conservative effect metadata.

Candidate effect vocabulary:

```text
Read(R)
Write(R)
Acquire(R)
Release(R)
MoveAuthority(R)
ChangeRepresentation(R)
ChangeRouting(R)
CreateReplica(R)
DropReplica(R)
ChangeProtocol(R)
ExternalEffect(E)
```

The domain validator determines whether effects conflict or commute. The planner may not assert commutativity merely to improve performance.

## 5. Transition program

A large reconfiguration can be represented as a dependency graph rather than a flat list:

```text
TransitionProgram {
    operations,
    dependency_edges,
    consistency_closure,
    migration_closure?,
    recovery_closure?,
    commit_condition,
}
```

This generalizes the previously proposed `TransitionOperationGraph`.

Cost should reflect overlap, contention and critical path where appropriate, not merely sum primitive latencies.

## 6. Trigger semantics

StreamOps demonstrates scheduled, conditional and manual triggers. Earlier Elastic work also identified multi-rate control and progress frontiers.

Candidate trigger model:

```text
TriggerSpec {
    Scheduled(schedule),
    Conditional(condition),
    External(event_source),
    Frontier(progress_condition),
}
```

`Frontier` is an Elastic extension.

A trigger means:

```text
consider whether control is needed now
```

not:

```text
perform a transition now
```

The control path may legitimately return no action.

## 7. Symptom, diagnosis and action

StreamOps gives production evidence that the same symptom can arise from different causes.

Keep separate:

```text
Observation
Symptom / Pressure
RootCauseEstimate
ImpactEstimate
Recommendation
```

For example:

```text
high queue / lag
    ├── insufficient capacity → resize
    ├── bad placement / straggler → migrate
    ├── skew → rebalance/reroute
    ├── downstream failure → repair/escalate
    └── transient harmless burst → no action
```

A pressure metric is not a diagnosis.

## 8. Control outcomes

A controller should not be forced to manufacture an actuator command.

Candidate:

```text
ControlOutcome {
    Recommend(plan),
    Escalate(diagnosis),
    NoAction(reason),
}
```

This supports domains where a symptom is diagnosable but no safe automated transition is currently authorized.

## 9. Policy arbitration

StreamOps uses a conservative production rule: several policies may execute, but one decision is applied at a time according to policy priority.

This is a useful safe baseline.

ElasticXxx should investigate a richer arbiter:

```text
RecommendationSet
      ↓
priority / objectives
      ↓
effect conflicts
      ↓
resource generations
      ↓
ConsistencyClosure
      ↓
compatible transaction batch
```

Possible outcomes:

```text
serialize all
choose one
merge compatible recommendations
reject stale recommendation
reject semantic conflict
```

Concurrent execution is permitted only after trusted conflict/consistency validation.

## 10. Recommendation freshness

A recommendation should carry enough context to reject stale execution:

```text
RecommendationContext {
    planner_epoch,
    observation_epoch,
    resource_generations,
    target_versions,
}
```

Before application, the validator checks that its assumptions still hold.

## 11. Control-plane self-accounting

The control plane itself consumes resources.

Account at least:

```text
observation / retrieval
model update
root-cause diagnosis
candidate generation
planning
validation
coordination
primitive bookkeeping
telemetry
```

A plan whose expected benefit does not exceed control + transition + verification cost should be rejected or deferred.

## 12. Multiscale execution

Do not route every fast local action through the global policy machinery.

```text
Global policies / planner
        ↓
validated bounded policies
        ↓
regional controllers
        ↓
prevalidated local fast path
```

Triggers and arbitration in this document principally apply where control-plane intervention is actually warranted.

## 13. Rust direction

Potential use of Rust types:

- capabilities constrain which primitive families can be constructed;
- typestate constrains protocol ordering;
- non-Clone authorities can encode exclusive ownership where appropriate;
- generations/leases validate external freshness dynamically.

Rust types do not prove external topology, availability or arbitrary commutativity of runtime effects.

## 14. Experiments

**EXPERIMENT REQUIRED.**

### A. Primitive coverage

Encode reconfigurations for memory migration, worker resize, KV representation change, routing change and replica movement.

### B. Arbitration

Compare:

```text
priority-only serialization
naive concurrent execution (negative control)
effect-disjoint batching
ConsistencyClosure-validated batching
```

### C. Triggering

Compare periodic-only control with event/conditional and multi-rate triggering. Measure missed events, false activations and control-plane cost.

### D. Root cause

Construct identical pressure symptoms from distinct causes and measure wrong-action rate for pressure-only versus diagnose-before-act controllers.

## 15. SciRust

No generic SciRust runtime facility is implied. These are target runtime/control-plane semantics. Scientific diagnosis algorithms should be audited against existing SciRust statistics/causal/learning capabilities only when a concrete reusable need appears.
