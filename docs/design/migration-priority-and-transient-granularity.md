# Migration Priority, Queue Amplification, and Transient Granularity

## Status

**ELASTIC PROPOSAL / RESEARCH DESIGN NOTE.**

This note synthesizes Meces (USENIX ATC 2022) with Megaphone, contextual utility lessons from KV-cache work, multiscale scheduling, and the existing staged-migration model. It does not claim novelty.

## 1. Migration plan has more than a destination

A migration plan must potentially choose:

```text
what final placement?
what partition/granularity?
which unit first?
how many units concurrently?
when to pause/pacing-gap?
how is mutable authority handed off?
```

Therefore a target state alone does not specify a migration.

## 2. Order is a first-class variable

Given units `u1..un`, two sequences can move exactly the same state and consume similar aggregate transfer work while having radically different service impact.

Reason:

```text
blocked critical request
    -> queue accumulation
    -> many otherwise-ready requests delayed
```

Thus:

```text
ServiceImpact(sequence)
!=
Σ TransferCost(unit)
```

This is analogous to earlier findings that hotness is not performance criticality and throughput is not useful progress.

## 3. Contextual transition priority

Avoid a permanent field:

```text
unit.hot = true
```

Prefer a contextual estimate:

```text
Priority(unit | context)
```

Possible context:

```text
pending work requiring unit
queue position / fan-out
critical-path membership
SLO/deadline slack
current topology
predicted near-term reuse
recompute alternative
transfer path congestion
```

Priority can change without changing the logical state of the unit.

## 4. Background trajectory + reactive override

A useful architecture is:

```text
BACKGROUND MIGRATION TRAJECTORY
    planned order / batches

        +

ON-DEMAND FAST PATH
    fetch/advance one urgently required unit
```

The fast path is only available when the resource adapter's handoff protocol can preserve invariants.

After an override, the regional controller can update the remaining trajectory rather than restarting global planning.

## 5. Queue amplification model

A transition cost model should separate:

```text
DirectTransitionCost
DownstreamBlockingCost
QueueAmplificationCost
InterferenceCost
```

Potential predicted impact:

```text
Impact(u,c) =
    direct_fetch_latency
  + expected_waiters_delayed
  + critical_path_delay
  + induced_queue_growth
```

The exact model is domain-specific.

A simple additive migration-byte objective cannot express this effect.

## 6. Steady versus transition granularity

Meces shows a hierarchical representation in which coarse key-groups are used in normal operation while fine sub-groups become addressable for migration.

Generalize:

```text
SteadyStateGranularity
TransitionGranularity
```

They need not be equal.

This is important because fine steady-state granularity can impose permanent costs:

```text
routing metadata
lookup cost
fragmentation
checkpoint metadata
synchronization
```

A transient fine subdivision can localize those costs to adaptation windows.

## 7. Transient subdivision lifecycle

Potential lifecycle:

```text
COARSE_RESOURCE
    ↓ transition requested
CREATE / EXPOSE TRANSIENT SUBDIVISION
    ↓
MIGRATE / REPAIR units independently
    ↓
VERIFY TARGET
    ↓
COLLAPSE / RETIRE TRANSIENT METADATA
```

The subdivision itself is maintenance/control state and must be included in transition cost.

## 8. Single mutable authority

For mutable state, demand-driven routing to old/new locations is safe only with a clear mutation-ownership rule.

Potential invariant:

```text
At every logical cut, exactly one authority may commit mutations for unit u.
```

Other protocols are possible (transactions, replication with consensus, CRDT-style convergence), but they must be explicit.

A typed capability can restrict authority, while dynamic generation/frontier checks establish which capability is current.

## 9. Temporary routing state

During staged migration the routing representation may differ from both source and final target routing.

Conceptually:

```text
StableRoutingBefore
TransitionRouting
StableRoutingAfter
```

`TransitionRouting` is a form of temporary maintenance/control state and may include per-unit exceptions, forwarding entries, or dual-route reconciliation rules.

Its footprint and lookup overhead must be self-accounted.

## 10. Joint optimization of destination and path

A destination configuration that minimizes steady-state cost may require excessive migration.

Therefore compare:

```text
SteadyStateBenefit(target)
-
TransitionPathCost(source -> target)
```

rather than optimizing target placement first and treating migration as an afterthought.

This connects Meces's repartitioning results with the general Elastic planner objective.

## 11. Candidate planning structure

```text
MigrationPlan {
    target_configuration,
    steady_partition,
    transition_partition,
    background_order,
    priority_model,
    batch_size,
    concurrency_budget,
    reactive_override_policy,
    temporary_routing,
    handoff_protocol,
}
```

This is conceptual vocabulary, not an API commitment.

## 12. Multi-speed controller

```text
GLOBAL PLANNER
    choose target + coarse path

REGIONAL CONTROLLER
    update order / batch size / pacing

LOCAL FAST PATH
    fetch or advance currently required unit
```

This is a concrete application of the multiscale architecture derived earlier from Cilk/A-STEAL/BWoS.

## 13. Relationship to useful progress

Migration priority should optimize avoided useful-progress loss rather than raw state “temperature”.

For example, a large frequently accessed unit may not be urgent if requests using it have ample slack, while a small rarely accessed unit can become immediately critical if it blocks the head of a high-priority queue.

Thus:

```text
activity != urgency != useful-progress impact
```

## 14. Safety boundary

The planner may change priority dynamically, but cannot bypass:

```text
semantic contract
single-authority/consistency protocol
generation/frontier validity
capacity constraints
allowed transfer destinations
```

Urgency never authorizes an illegal transition.

## 15. SciRust relationship

No new SciRust implementation is justified by this note.

Queueing/performance-impact modelling is scientifically general enough to investigate later, but one system paper is insufficient justification for a new SciRust module. Existing optimization/statistics/simulation tooling should first be audited against a concrete experimental model.

## 16. Experiments

Use identical migration bytes and destination layout while varying only sequence/pacing.

Collect per-request timelines so direct transfer waiting can be separated from queue-amplified waiting.

Test transient subdivision with increasing fineness and identify the crossover where reduced migration stalls are outweighed by metadata/routing/coordination overhead.
