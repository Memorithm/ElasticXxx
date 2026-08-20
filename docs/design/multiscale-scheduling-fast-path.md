# Multiscale Scheduling and the Elastic Fast Path

**Status:** provisional architecture note derived from the Cilk, A-STEAL, and BWoS literature reviews. This is a design hypothesis, not a novelty claim.

## 1. Motivation

The reviewed literature spans radically different decision frequencies:

- cloud autoscaling: seconds to minutes;
- malleable HPC: seconds to job-phase timescales;
- memory migration: page/tier event timescales;
- task scheduling/work stealing: potentially per-task and therefore extremely frequent.

A single heavyweight `OBSERVE → FORECAST → PLAN → VALIDATE → ACT → VERIFY` implementation path is inappropriate for all of these scales.

Cilk demonstrates that a very cheap local scheduling rule can provide strong behavior for structured computations. A-STEAL demonstrates that local work stealing can feed a slower processor-allocation controller. BWoS demonstrates that at fine granularity, atomic operations, barriers, cache-line movement, and queue metadata contention become first-order costs.

ElasticXxx should therefore explicitly investigate a **multiscale control architecture**.

---

## 2. Proposed decision classes

### 2.1 Local Fast Path

Characteristics:

- invoked at very high frequency;
- local or near-local observations;
- bounded decision complexity;
- prevalidated action family;
- no global combinatorial search;
- minimal telemetry on the synchronous path;
- optimized concurrency implementation.

Examples:

- steal a task;
- choose among a few ready workers;
- local queue backpressure action;
- cache/prefetch action with a bounded target set.

### 2.2 Regional / Control Path

Characteristics:

- aggregates fast-path feedback;
- operates over milliseconds/seconds or another resource-specific interval;
- can adjust capacity, concurrency, quotas, thresholds, or local-policy parameters;
- may use simple control theory, heuristics, or small optimization problems.

Examples:

- change worker count;
- update steal aggressiveness;
- change migration budget;
- select active memory tier policy.

### 2.3 Global / Planning Path

Characteristics:

- broad system snapshot;
- potentially expensive modelling/search;
- lower invocation frequency;
- may coordinate multiple resource dimensions/domains;
- may use ILP/DP/MPC/evolutionary/learned methods.

Examples:

- repartition a model across devices;
- large residency/topology change;
- choose a new distributed execution plan.

The names are provisional.

---

## 3. Core principle

> **The common case should not require global planning.**

This principle was already emerging from the Alpa/planner-overhead review. Work-stealing literature makes it a hard performance requirement for fine-grained scheduling.

The architecture should permit sophisticated planning without forcing its cost onto every operation.

---

## 4. Feedback hierarchy

A-STEAL provides a useful general pattern:

```text
LOCAL EVENTS
(task starvation, steals, queue activity)
        ↓
compressed / aggregated feedback
        ↓
REGIONAL DEMAND ESTIMATE
(desired concurrency)
        ↓
resource request
        ↓
PHYSICAL ALLOTMENT
(actual cores/workers)
```

**ELASTIC PROPOSAL.** Each control layer should expose only the information required by the next layer rather than exporting all raw events.

Potential generic feedback objects could include:

```text
ParallelismDemand
ContentionPressure
LocalityPressure
QueuePressure
MigrationPressure
```

Exact types are not fixed.

---

## 5. Desired state versus granted state

A-STEAL distinguishes processor desire from processor allotment.

ElasticXxx should generalize this distinction where applicable:

```text
DesiredState
RequestedState
GrantedState
ObservedState
```

These may differ because:

- physical capacity is unavailable;
- allocation is asynchronous;
- policy/quota limits apply;
- only a partial request can be satisfied;
- the environment changes after the request.

This should integrate with the existing pending/transition protocol model rather than being hidden as an implementation detail.

---

## 6. Fast-path authorization

A local fast path should not need the full validator every time if its action space has already been safely constrained.

**ELASTIC PROPOSAL.** Investigate **prevalidated transition families**.

Conceptually:

```text
Trusted setup / validation
        ↓
construct bounded local policy capability
        ↓
FAST PATH
  choose only among pre-authorized actions
        ↓
cheap generation/epoch check if required
        ↓
actuate locally
```

For example, a worker may be authorized to steal tasks only from a specific worker set and only using a verified queue mechanism.

This does not eliminate validation; it moves expensive validation out of the per-operation hot path.

---

## 7. Scheduler cost model

At fine granularity, transition cost should include implementation effects:

```text
C_local =
    decision instructions
  + synchronization
  + atomics / fences
  + cache coherence
  + metadata reads
  + task/data movement
  + locality loss
  + retry / failed steal cost
```

A policy that chooses a theoretically better victim but scans many shared queues may lose overall because observation itself perturbs the system.

Therefore:

> **Observation can be an intervention.**

This is an important refinement of the existing observation-cost principle.

---

## 8. Correctness layers

For fine-grained concurrent transitions, distinguish:

```text
POLICY LEGALITY
"is stealing from this worker allowed?"

ALGORITHM CORRECTNESS
"does this queue protocol preserve task semantics?"

MEMORY-MODEL CORRECTNESS
"do the atomics/fences implement the protocol on this architecture?"
```

Rust's type system helps with memory safety and ownership structure but does not by itself prove all weak-memory concurrent algorithms correct.

Trusted fast-path primitives should therefore be small enough to stress-test, model-check, or formally verify where justified.

---

## 9. Relationship to Elastic Planner lifecycle

A local fast-path policy is still a policy, but its lifecycle differs from an expensive learned planner.

Possible invalidation events include:

- worker set changed;
- NUMA topology changed;
- queue implementation changed;
- capability generation changed;
- contention regime changed enough that the selected local policy is no longer appropriate.

A higher-level controller may replace or reparameterize the local policy without changing application semantics.

---

## 10. Local policy replacement

A useful pattern is:

```text
FastPolicy v3
    ↓ telemetry aggregate
Regional controller detects poor locality
    ↓
validate FastPolicy v4
    ↓
atomic / epoch-based policy swap
    ↓
FastPolicy v4
```

This makes the **policy itself** elastic while keeping per-event overhead bounded.

This is an ELASTIC PROPOSAL and should not be claimed as established novelty.

---

## 11. Interaction with planning domains

Alpa suggested decomposing a large planning problem into specialized domains. The work-stealing literature adds a temporal dimension:

```text
Planning Domain
  = coupled resource variables
  + appropriate solver/policy
  + appropriate timescale
```

Two domains may interact while operating at different frequencies.

Example:

```text
Task scheduling domain       every micro/millisecond
Worker-capacity domain       every 10–100 ms
NUMA placement domain        every 100 ms–seconds
Global topology domain       event-driven / seconds+
```

Exact times are workload/platform dependent.

---

## 12. H7 refinement — Low-Overhead Elasticity

A stronger but still provisional form of H7 is:

> **H7 — Multiscale Low-Overhead Elasticity.** Elastic adaptation can remain practical across multiple granularities if high-frequency decisions are restricted to prevalidated local policies with bounded observation and synchronization cost, while expensive planners operate asynchronously or at lower frequencies on aggregated feedback.

This requires experiments.

---

## 13. Required benchmark dimensions

A prototype must measure the control system itself, not only workload completion time:

- local policy latency;
- atomics/fences per action;
- cache misses/coherence;
- failed action/steal rate;
- feedback aggregation cost;
- controller interval;
- global planner CPU time;
- number of policy replacements;
- actuation latency;
- useful-work fraction;
- p50/p95/p99 task latency;
- fairness;
- energy where measurable.

---

## 14. Open questions

1. What is the minimal common interface between fast local policies and slower planners?
2. Can action families be prevalidated safely enough to avoid heavyweight validation per event?
3. Which resource dimensions admit local policies, and which inherently require global coordination?
4. How should feedback be compressed without losing signals required by higher levels?
5. Can we guarantee bounded per-event overhead in Rust?
6. How should policy epochs and resource generations interact on the hot path?
7. Can a fast policy be swapped without draining or quiescing the runtime?
8. Should the EIR encode timescale/decision-budget metadata?

---

## 15. Current conclusion

ElasticXxx should treat **timescale and decision cost as first-class properties of adaptation**.

The working architecture is now:

```text
FAST LOCAL POLICY
      ↓ feedback
REGIONAL CONTROL
      ↓ aggregated state
GLOBAL PLANNING
```

with authority and validation boundaries appropriate to each level.

The goal is not to turn every work-steal into an `ElasticPlan`; it is to let task stealing, worker resizing, topology changes, and other adaptive mechanisms coexist under one semantic model **without forcing one runtime cost model onto all of them**.
