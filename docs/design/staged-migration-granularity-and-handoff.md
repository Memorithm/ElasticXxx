# Staged Migration, Granularity, and Frontier-Gated Handoff

## Status

**ELASTIC PROPOSAL / RESEARCH DESIGN NOTE.**

This note synthesizes Megaphone (PVLDB 2019) with prior ElasticXxx findings from MPI malleability, NOMAD transactional migration, Naiad/timely progress frontiers, distributed snapshots, and multiscale scheduling. It does not claim novelty.

## 1. Migration is not necessarily one action

A high-level request:

```text
MIGRATE resource/configuration A -> B
```

may represent a set of independently movable state units.

Instead of one large transition, the runtime may construct a staged migration:

```text
A
 -> A1
 -> A2
 -> ...
 -> B
```

where each intermediate state is legal and useful execution continues between stages.

Megaphone provides a concrete prior-art instance through all-at-once, fluid, and batched migration policies over key/bin assignments.

## 2. Migration unit

A generic migration-capable resource adapter may expose a partition such as:

```text
MigrationUnitId
MigrationUnitState
```

Possible domain-specific units:

```text
memory page
NUMA region
KV block/token range
model shard
key range
stream-state bin
task queue partition
worker-owned state
storage segment
replica group
```

The core should not prescribe the unit.

## 3. Granularity is a planning variable

Finer migration units can reduce instantaneous disruption but increase:

```text
metadata
routing table size
planning work
serialization calls
coordination events
per-unit protocol overhead
fragmentation
```

Coarser units can reduce control overhead but increase:

```text
latency spikes
temporary memory
bandwidth bursts
recovery scope
rollback scope
```

Therefore:

```text
MigrationGranularity
```

is neither a pure performance constant nor “always finer is better.” It belongs in the candidate-plan space subject to adapter-supported bounds.

## 4. Static versus adaptive partitioning

Megaphone's evaluated implementation fixes the number of bins at startup. The paper notes dynamic split/merge as a possible alternative.

ElasticXxx should distinguish:

```text
FixedPartition
AdaptivePartition { Split, Merge }
```

Adaptive partitioning is itself a state transition with cost and invariants. It must not be assumed available for every resource.

## 5. Ownership schedule

A target configuration may be represented conceptually as:

```text
Owner(unit, logical_version) -> location
```

or, more generally:

```text
Placement(unit, logical_version) -> placement state
```

This separates:

```text
target placement policy
```

from:

```text
physical migration execution
```

The planner can propose a future ownership schedule without directly mutating resources.

## 6. Frontier-gated handoff

A migration unit cannot safely move merely because the transfer channel is available.

For state with ordered mutations, a handoff cut must establish that the source state includes all writes that semantically precede the cut.

Conceptually:

```text
WAIT_UNTIL(source_progress >= cut)
    ↓
source state is closed for pre-cut mutations
    ↓
EXTRACT
TRANSFER
INSTALL
    ↓
new owner serves post-cut mutations
```

The actual condition may use:

```text
progress frontier
safe point
quiescence protocol
transaction epoch
version fence
application-specific barrier
```

The key invariant is the ownership cut, not one specific mechanism.

## 7. State closure of a migration unit

The bytes physically stored in an object may be insufficient to migrate its semantics.

Megaphone migrates both operator state and future/pending records for the key/bin.

Generalize as:

```text
MigrationClosure(unit, cut) =
    materialized state
  + pending work owned by unit
  + necessary metadata
  + version/provenance state
  + protocol state required after handoff
```

This parallels the previously defined `RecoveryClosure`, but its purpose is ownership transfer rather than recovery.

`MigrationClosure` and `RecoveryClosure` may overlap physically without being semantically identical.

## 8. Configuration uncertainty

If a future target placement is not yet final, routing should not silently assume it.

Possible legal mechanisms:

```text
BUFFER_UNTIL_CONFIGURATION_STABLE
ROUTE_TO_OLD_OWNER_WITH_RECONCILIATION
DUAL_WRITE_UNDER_PROTOCOL
VERSIONED_FORWARDING
```

The available options depend on the resource's mutation semantics and semantic contract.

Megaphone uses buffering until the relevant configuration frontier establishes certainty.

## 9. Migration trajectory

Potential planning object:

```text
MigrationTrajectory {
    target_configuration,
    partition,
    ordered_or_partial_batches,
    handoff_cuts,
    pacing_policy,
    concurrency_budget,
    bandwidth_budget,
    temporary_memory_budget,
    deadline,
}
```

This is an ELASTIC PROPOSAL, not a Megaphone API.

Only the next validated stage needs to be committed. After each stage the runtime can re-observe and replan, analogous to receding-horizon control.

## 10. Pacing

Staged migration exposes another action-intensity dimension:

```text
units per batch
concurrent transfers
bytes per interval
gap / dwell between batches
```

A planner/controller may adapt these values using observed:

```text
service latency
queue depth
bandwidth contention
temporary memory
migration completion rate
```

This connects Megaphone's staged migration to earlier FlexMem/Alto lessons about action intensity.

## 11. Latency-constrained objective

A useful migration objective may be:

```text
minimize total migration completion time
```

subject to:

```text
service_latency <= L
memory_spike <= M
migration_bandwidth <= B
semantic invariants
handoff correctness
```

or a multi-objective equivalent.

The shortest-duration migration is not necessarily the best migration if it causes unacceptable transient disruption.

## 12. Flow control and self-accounting

Megaphone's batched/fluid policies naturally limit temporary state by waiting before launching more migrations. This can be generalized as migration flow control.

The runtime must account for:

```text
serialization buffers
network queues
target staging memory
routing metadata
pending updates
verification state
```

A migration plan that fits steady-state memory but exceeds transient memory is infeasible.

## 13. Interaction with transition protocol

For a staged unit:

```text
PROPOSE_UNIT
VALIDATE_UNIT
WAIT_FOR_HANDOFF_CONDITION
PREPARE_TARGET
CLOSE_SOURCE_AT_CUT
EXTRACT_CLOSURE
TRANSFER
INSTALL
VERIFY
COMMIT_OWNERSHIP
RELEASE_OLD_STATE
```

Failure may require:

```text
ABORT
RETRY
FORWARD
ROLLBACK
RECONCILE
```

The exact sequence remains adapter-specific.

## 14. Interaction with multiscale control

Migration planning may occur at several scales:

```text
GLOBAL PLANNER
    target placement / partition / deadline

REGIONAL MIGRATION CONTROLLER
    batch size / concurrency / pacing

LOCAL TRANSFER FAST PATH
    prevalidated copy/forward/install operations
```

This avoids invoking an expensive global planner for every migration unit.

## 15. Interaction with checkpoints

A checkpoint is not required before every migration unit, but some transitions may benefit from a recovery point.

Possible relation:

```text
MigrationTrajectory
    ├── handoff cuts
    ├── optional recovery cuts
    └── unit ownership commits
```

The runtime should not conflate:

```text
ownership transfer correctness
```

with:

```text
failure recovery correctness
```

although they may share version/frontier machinery.

## 16. Rust/type boundary

Types/capabilities can potentially restrict which unit classes and destinations a caller may request.

Runtime validation must still establish dynamic facts such as:

```text
frontier reached
target memory available
network path live
generation current
source closure valid
```

A typed `MigrationCapability<R>` is authority, not proof that a particular staged handoff can succeed now.

## 17. SciRust relationship

SciRust is external R&D tooling only.

General order/lattice/antichain primitives now available in `scirust-algebra` can be used to model frontier mathematics experimentally. Migration binning, network transfer, routing, ownership handoff, and pacing belong in target runtimes unless an independent scientific need later reveals a reusable mathematical primitive.

No new SciRust gap is established by Megaphone.

## 18. Experimental matrix

At minimum compare:

```text
all-at-once
fluid one-unit-at-a-time
fixed batches
adaptive batch size
adaptive unit split/merge
```

under identical final placement and workload.

Measure:

```text
p50/p95/p99/max service latency
migration duration
throughput
temporary memory
network burstiness
queue buildup
routing metadata
control-plane overhead
handoff verification latency
failure/recovery behavior
```

A negative-control test should intentionally hand off a unit before the safe cut and demonstrate that the validator/test oracle detects lost/reordered state.
