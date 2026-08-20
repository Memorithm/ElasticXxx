# Gu et al. (2022) — Meces: Latency-efficient Rescaling via Prioritized State Migration

## Status

**SOURCE-DERIVED mechanism review with ELASTIC PROPOSALS clearly separated.**

Primary source: Rong Gu, Han Yin, Weichang Zhong, Chunfeng Yuan, Yihua Huang, *Meces: Latency-efficient Rescaling via Prioritized State Migration for Stateful Distributed Stream Processing Systems*, USENIX ATC 2022.

## Problem

Fine-grained migration alone does not determine service latency. When downstream processing is FIFO and a record arrives before the state it needs, that record blocks and can cause all subsequent records to accumulate behind it.

Meces studies **migration order** as a first-class performance variable during stream-processor rescaling.

## Key observation: order changes queuing cost

The paper decomposes processing latency during rescaling into:

```text
Job-Cost
Migration-Cost
Queuing-Cost
```

An order-unaware migration can incur one or several long waits for missing state. Even records whose state is already local can then suffer large `Queuing-Cost` because they sit behind a blocked record.

Meces prioritizes state needed by records currently being processed or about to be processed. The objective is not necessarily to reduce the total bytes or even total migration service time, but to reduce the **duration of individual blocking episodes** and therefore prevent queue amplification.

This is a critical systems lesson:

```text
same aggregate transition work
!=
same service impact
```

## Fetch-on-demand

During rescaling, a destination operator may receive a record whose state is not local. Instead of waiting for the background migration order to reach that state, the operator immediately fetches the corresponding state.

The background path still pushes states in batches for migration throughput; active fetch provides a fast path for currently needed state.

Conceptually:

```text
BACKGROUND PUSH
    high migration throughput

ON-DEMAND FETCH
    current useful-progress unblock
```

This is a concrete example of combining proactive/background adaptation with reactive demand-driven adaptation.

## Consistency protocol

Meces uses control messages to coordinate a migration stage. A rescaling operator progresses through `Aligning` and `Aligned` phases.

For a key migrating from source `I1` to destination `I2`, records may temporarily be routed to either instance during alignment, but the paper maintains that only one instance holds/mutates the key state locally at a time. State is flushed/“borrowed” only after processing of the current record completes; after alignment, subsequent records route only to `I2`.

This is important prior art for a **single mutable authority** invariant during dynamic, demand-driven migration.

## Hierarchical state organization

Modern stream processors commonly maintain a manageable number of coarse key-groups. Making every key its own globally routed partition would create excessive metadata/routing/checkpoint overhead.

Meces therefore adds sub-groups inside key-groups:

```text
KeyGroup
    ├── SubGroup 1
    ├── SubGroup 2
    └── ...
```

Steady-state routing remains coarse at key-group granularity. During rescaling, fetches can operate at finer sub-group granularity.

This creates an important distinction:

```text
SteadyStateGranularity
!=
TransitionGranularity
```

The fine partition can exist specifically to improve transition behavior without forcing the full steady-state data path to pay fine-grained routing overhead.

## Gradual migration

Meces also splits the migration stage into micro-batches. The `batch_size` parameter limits how many key-groups an instance disposes at one gradual migration step.

Changing `batch_size` trades migration throughput against latency spikes, similarly to Megaphone's fluid/batched policies.

Meces therefore combines two orthogonal mechanisms:

```text
which state first?     -> demand-based priority
how much at once?      -> gradual micro-batch size
```

## Temporary routing

At each gradual-fetch step, upstream instances receive information about the currently migrated keys and build temporary routing tables. Most state remains unaffected and continues normal processing.

This reinforces the idea that a transition can carry its own **temporary routing/control representation** that need not become the permanent steady-state representation.

## Repartitioning interaction

The paper also demonstrates that the final partitioning strategy changes migration cost. In one experiment with 128 key-groups, consistent hashing reduces migrated groups from 115 to 15 and produces up to 70% shorter rescaling duration and 90% lower max latency relative to the paper's uniform repartition setup.

Therefore:

```text
final placement quality
```

and

```text
transition distance / amount of state moved
```

must be jointly considered. A planner should not optimize only the destination state and ignore the path required to reach it.

## Evaluation

The paper reports across its evaluated workloads that Meces reduces processing-latency peaks during rescaling by roughly 95% compared with selected baselines.

Specific observations include:

- in the key-count experiment with 10^8 unique keys, Meces keeps latency below roughly 600 ms during prioritized migration while Native Flink and the order-unaware baseline exhibit peaks about three orders of magnitude above normal latency;
- in the latency-breakdown experiment, the Meces peak is below roughly 400 ms and long migration stalls are transformed into many short fetch operations, greatly reducing downstream queueing cost;
- normal/non-rescaling execution has little measured latency overhead in the reported setup, although temporary routing during rescaling increases routing cost;
- state migration can create allocation/GC pressure in the Java implementation, and the authors recommend low-latency GC or preallocated object pools.

These are implementation/workload-specific results, not universal ratios.

## Relation to Megaphone

Megaphone primarily exposes **migration granularity and pacing** using timestamped configuration changes.

Meces argues that order matters even with fine-grained state movement and adds **demand-driven priority**.

A useful synthesis is:

```text
MigrationPlan =
    partition/granularity
  + destination
  + ordering/priority
  + batch/concurrency intensity
  + handoff protocol
```

No one dimension substitutes for the others.

## Elastic relation

### ADOPT

- Make migration **ordering/priority** explicit rather than incidental.
- Estimate service impact from queueing/critical-path effects, not only aggregate migration bytes/time.
- Permit a reactive on-demand fast path to override background migration order when legal.
- Distinguish steady-state granularity from transition-only granularity.
- Preserve a single mutable authority or another explicit consistency protocol during live handoff.

### ADAPT

Generalize “hot key” into contextual utility / blocking impact:

```text
Priority(unit | execution_context)
```

Possible context:

```text
pending requests
queue position
critical path
SLO slack
recomputation cost
future use probability
```

The runtime need not use FIFO queues or keys for this mechanism to apply.

### REJECT

Do not make `hotness` a permanent property of a resource. As with Quest/IMPRESS, urgency is contextual.

Do not assume on-demand fetch is always superior. Random small fetches may perform badly on high-latency storage/network or create request amplification; the mechanism must be chosen from actual cost/topology capabilities.

## Elastic proposal: transition utility and queue amplification

A candidate migration order should account for the useful-progress impact of blocking a unit:

```text
TransitionPriority(u | c) =
    expected_unblocked_useful_progress
  + avoided_queueing_penalty
  + deadline/SLO urgency
  - immediate_fetch_cost
  - interference_cost
```

This is an ELASTIC PROPOSAL, not the Meces formula.

The important structural point is that migration-cost distribution matters:

```text
CostImpact(sequence)
!=
Σ Cost(unit)
```

because queueing and parallelism introduce nonlinear interactions.

## Elastic proposal: transient hierarchy

A resource can expose:

```text
SteadyPartition
TransitionSubdivision
```

where fine transition units are addressable only while a transition is active.

This may reduce normal-path metadata costs while preserving fine migration granularity.

Examples beyond streaming keys:

```text
large memory region -> temporary page groups
model shard -> temporary tensor chunks
KV block group -> token/page subgroups
storage segment -> transfer extents
worker state -> task/state subsets
```

## SciRust relationship

No new SciRust capability is implemented from this paper.

A repository search did not identify a clear generic queueing-theory module, but Meces alone is insufficient evidence to add one. Queueing models could be scientifically general, yet a state-of-the-art audit and independent use case are required before creating a SciRust gap or implementation.

The priority/fetch/routing protocol itself is a runtime mechanism and does not belong in SciRust.

## Experiment required

For identical source/target placement and identical bytes moved, compare:

1. random/order-unaware unit order;
2. static largest-first / smallest-first;
3. current-demand-first;
4. deadline/slack-aware priority;
5. demand-first + fixed gradual batches;
6. demand-first + adaptive batch/concurrency intensity.

Measure:

```text
aggregate migration work
p50/p95/p99/max request latency
queue length / waiting time
number and size of on-demand fetches
background migration duration
network IOPS and bandwidth
metadata/routing overhead
semantic correctness
```

A key test is whether equal total migration work produces materially different useful progress because of sequencing and queue effects.
