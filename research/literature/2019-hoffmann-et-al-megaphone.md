# Hoffmann et al. (2019) — Megaphone: Latency-conscious State Migration for Distributed Streaming Dataflows

## Status

**SOURCE-DERIVED mechanism review with ELASTIC PROPOSALS clearly separated.**

Primary source: Moritz Hoffmann, Andrea Lattuada, Frank McSherry, Vasiliki Kalavri, John Liagouris, Timothy Roscoe, *Megaphone: Latency-conscious state migration for distributed streaming dataflows*, PVLDB 12(9), 2019.

## Problem

Stateful distributed streaming systems may need to change worker assignment while continuing to serve records. Coarse migration can create large service-latency and temporary-memory spikes because substantial state is serialized and transferred at once.

Megaphone asks whether migration can be decomposed and coordinated by logical progress rather than by coarse pause/restart barriers.

## Computation model

The mechanism assumes deterministic data-parallel operators whose inputs are timestamped `(key, value)` records. Each key owns state and possibly future/pending records, and operations for that key are applied in timestamp order.

Megaphone introduces a time-indexed configuration function:

```text
configuration(time, key) -> worker
```

Its migration correctness requirement is that updates for a key at logical time `t` execute at the worker selected by `configuration(t,key)`.

The paper separately states correctness, migration, and completion/liveness properties. This separation is important: placing state on the desired worker is not enough if records are lost/reordered or progress can deadlock.

## Configuration updates are data

Configuration changes themselves enter the timely dataflow as timestamped records:

```text
(time, key, worker)
```

This has two consequences:

1. a reconfiguration can be prepared ahead of the logical time at which it becomes effective;
2. a large target reconfiguration can be revealed incrementally rather than atomically.

This is the paper's key mechanism for moving expensive control/coordination work away from one disruptive instant.

## Three migration strategies

The paper describes:

### All-at-once

All changed key assignments share one logical migration time. This approximates coarse partial pause-and-restart behavior.

### Fluid

One changed key/bin is migrated, completion is awaited, and then the next is migrated. This minimizes instantaneous disruption at the expense of migration duration.

### Batched

Several changed assignments share one logical time; the next batch waits for completion of the current batch. The evaluation uses batched migration as Megaphone's optimized trade-off between latency and total migration duration.

These are policies expressed through the same correctness mechanism, not three different migration protocols.

## Frontier-gated handoff

A central correctness rule is that state for a key must not migrate at logical time `t` until all updates strictly earlier than `t` have been absorbed by the current state.

Megaphone uses the downstream output frontier to establish that condition. Only once `t` is present in the relevant frontier does the routing operator uninstall the current state and transfer it, tagged with time `t`, to the new worker.

The migrated payload includes:

```text
current per-key state
+
pending future (value,time) records
```

The receiving operator installs the state and applies data/pending records in timestamp order subject to the data/state input frontiers.

This provides a concrete prior-art instance of a **frontier-gated ownership handoff**.

## Routing during reconfiguration

An upstream `F` operator consumes both data and configuration-update streams. It routes `(key,value)` records according to the configuration that applies at each record's timestamp.

If the configuration for a logical time is not yet certain because the configuration frontier has not advanced far enough, `F` buffers those records rather than guessing a route.

This is an important safety pattern:

```text
uncertain future configuration
    -> buffer / wait
    != route optimistically without reconciliation
```

## Operator decomposition

Megaphone replaces one stateful operator `L` with two cooperating operators:

```text
F  routing + configuration + migration initiation
S  state hosting + application of operator logic L
```

`F` and `S` exchange both data and migrating state. The separation makes routing/control state explicit while allowing user logic to remain close to timely's stateful operator interface.

## Migration granularity: bins

The abstract mechanism is per-key, but managing millions/billions of keys individually is expensive. Megaphone groups keys into configurable **bins** and changes the configuration function to:

```text
configuration(time, bin) -> worker
```

Keys are statically assigned to bins in the evaluated implementation. The number of bins is selected at startup and cannot be changed during runtime in that implementation.

Therefore granularity is a first-class performance parameter:

```text
finer bins
    -> smaller migration units / lower latency spikes
    -> more routing/configuration/metadata overhead
```

The paper explicitly notes dynamic split/merge of bins as an alternative design, not as functionality delivered by the evaluated implementation.

## Controller boundary

Megaphone deliberately does not prescribe the elasticity/control policy. An external controller supplies configuration updates in the required format.

This is consistent with a clean separation:

```text
controller / planner
    -> desired timestamped ownership schedule
migration mechanism
    -> realizes it correctly and live
```

The paper also discusses grouping non-interfering migrations with bipartite matching and adding gaps between migrations so queued records can drain. These are migration-policy optimizations on top of the core mechanism.

## Evaluation

The evaluation runs on four machines, each with four Xeon E5-4650 v2 CPUs and 512 GiB RAM, using four timely workers per process pinned to one CPU socket.

Key results include:

- On NEXMark at 4 million updates/s, stateful queries generally show substantially lower reconfiguration latency with batched Megaphone than all-at-once when state is large. In Q3, the reported reconfiguration spike is more than 100 ms for all-at-once versus about 10 ms for batched in the shown run.
- In the key-count microbenchmark, increasing migration granularity (more, smaller bins) lowers maximum latency for fluid/batched strategies without materially increasing total migration duration over a useful range.
- With state per bin held constant while total state grows, maximum latency for fluid and batched migration remains approximately fixed while total migration duration grows; all-at-once latency and duration grow.
- For the reported throughput experiment, fluid and batched migration sustain up to 4 million records/s under a 1 s latency target and satisfy latency targets 10–100x lower than all-at-once at similar throughput.
- In the 16-billion-key / 4096-bin memory experiment, batched/fluid stay around 35 GiB RSS without a large migration spike, whereas all-at-once allocates roughly 30 GiB extra during migrations.

All of these results are workload/hardware/implementation-specific and should not be generalized as universal ratios.

## Limitations / assumptions

- The mechanism relies on event/logical time, progress tracking/frontiers, and extractable state.
- The evaluated implementation's bin count is fixed at startup.
- Very fine binning has steady-state overhead; the evaluation observes noticeable degradation at sufficiently high bin counts.
- State must be isolated so that state plus pending per-key work can be transferred consistently.
- Megaphone inherits fault-tolerance behavior from the host dataflow runtime rather than providing a complete independent recovery system.

## Elastic relation

### ADOPT

- Make **migration granularity** an explicit transition/planning parameter.
- Separate desired ownership/configuration schedule from the trusted migration mechanism.
- Use a correctness/progress gate before ownership handoff.
- Include pending future work that semantically belongs to migrated state.
- Allow a large reconfiguration to be represented as a sequence/trajectory of smaller legal transitions.

### ADAPT

Generalize bins to arbitrary migration units whose legal partitioning is resource-specific:

```text
MigrationUnit = page | shard | key-range | tensor shard | KV block | worker state | ...
```

The unit may itself be split/merged when the resource adapter supports it.

Generalize timestamped ownership schedules to a `MigrationTrajectory` or staged transition plan rather than forcing every resource to use timely-dataflow timestamps.

### REJECT

Do not assume finer granularity is always better. Finer units increase metadata, routing, scheduling, serialization and coordination overhead.

Do not make the planner responsible for proving handoff correctness. The planner selects a legal schedule; a trusted protocol enforces the ownership cut.

## Elastic proposal: staged migration

A monolithic transition:

```text
MIGRATE(A -> B)
```

can be decomposed as:

```text
MigrationTrajectory {
    target_configuration,
    units: [u1, u2, ...],
    batches,
    handoff_conditions,
    pacing,
}
```

For each unit:

```text
WAIT_FOR_FRONTIER
FREEZE_OLD_OWNERSHIP_AT_CUT
EXTRACT_STATE + PENDING_WORK
TRANSFER
INSTALL_NEW_OWNER
VERIFY
ADVANCE_ROUTING
```

The exact protocol varies by resource; this vocabulary is an ELASTIC PROPOSAL.

## Elastic proposal: latency-constrained migration planning

The planner should potentially optimize migration pace/granularity under constraints such as:

```text
max service latency <= L
max temporary memory <= M
max migration bandwidth <= B
completion deadline <= D
```

rather than optimizing only total migration duration.

This turns migration into a trajectory/control problem:

```text
minimize completion time / resource cost
subject to transient service constraints
```

A valid solution may intentionally take longer overall to reduce instantaneous disruption.

## SciRust relationship

No new SciRust gap is established by Megaphone.

The newly added general `PartiallyOrdered` / lattice / `Antichain` primitives are sufficient to prototype order/frontier mathematics in SciRust R&D. Binning, state transfer, routing, buffering, and distributed migration are runtime-system mechanisms and should not be moved into SciRust merely because ElasticXxx studies them.

## Experiment required

Create one Elastic migration test workload with partitionable state and compare:

1. all-at-once;
2. one-unit-at-a-time;
3. fixed-size batches;
4. latency-budget feedback pacing;
5. adaptive split/merge of migration units.

Measure:

- p50/p95/p99/max service latency;
- total migration duration;
- throughput;
- temporary memory;
- network bytes/rate;
- queue growth;
- planner/metadata overhead;
- stale/misrouted updates;
- correctness after adversarial failures at handoff boundaries.
