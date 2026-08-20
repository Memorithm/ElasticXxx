# Version Frontiers, Delta Traces, and Consistent Checkpoints

## Status

**ELASTIC PROPOSAL / RESEARCH DESIGN NOTE.**

This note synthesizes mechanisms from Differential Dataflow, Naiad, Chandy–Lamport distributed snapshots, Asynchronous Barrier Snapshotting, and earlier ElasticXxx work on derived-resource provenance, maintenance state, transactional transitions, and epochs. It does not claim novelty.

## 1. Why a scalar epoch is not enough

A scalar `Generation` is useful for one local question:

```text
Is this capability / handle / materialization stale?
```

But distributed and nested computations can evolve along several independent dimensions. Differential Dataflow uses partially ordered logical versions, and Naiad tracks progress using logical timestamps plus causal graph location.

Therefore distinguish:

```text
ResourceGeneration
    local freshness / revocation generation

LogicalVersion
    domain-specific partially ordered version

ProgressFrontier<V>
    minimal boundary of outstanding / still-possible causal work
```

A global scalar counter should remain available as the cheap default when total ordering is sufficient.

## 2. Logical version is not wall-clock time

`LogicalVersion` describes computation structure, not physical time.

Examples:

```text
(input_epoch, loop_iteration)
(dataset_version, optimization_round)
(checkpoint_generation, replay_round)
(branch_version, merge_generation)
```

The order may be partial:

```text
(a1, b1) <= (a2, b2)
iff
a1 <= a2 && b1 <= b2
```

Two versions can therefore be incomparable.

## 3. Frontier

A frontier is conceptually an antichain of incomparable minimal points that bound progress.

It can answer questions such as:

```text
Can any earlier work still appear?
Can this notification be published?
Can history before this boundary be compacted?
Can an old replica/version be released?
```

The generic Elastic core should not infer frontier semantics by itself. A domain adapter must define its version order and the conditions under which advancing a frontier is safe.

## 4. Safe frontier gates

Potential trusted-runtime operation:

```text
SafeAfter(frontier, operation)
```

Examples:

```text
COMPACT_HISTORY before F
DROP_REPLICA after F
PUBLISH_DERIVED_STATE after F
COMMIT_MIGRATION after pre-F writers drain
RECLAIM_METADATA before F
```

This should be treated as a correctness gate, not merely a planner score.

## 5. Delta trace as maintenance state

Differential computation motivates:

```text
MaterializedState
+
DeltaTrace<LogicalVersion, Change>
```

The trace can preserve differences that allow future states to reuse historical computation.

A delta trace is therefore a specialization of the existing Elastic concept `MaintenanceState`:

```text
MaintenanceState {
    purpose = IncrementalReconstruction,
    footprint,
    update_cost,
    version_domain,
    compaction_policy,
    validity_frontier,
}
```

## 6. Maintenance-state elasticity

The current Rust differential-dataflow implementation distinguishes **logical compaction** from **physical compaction**.

Generalize this distinction:

```text
LogicalCompaction
    forget distinctions that future semantics can no longer observe

PhysicalCompaction
    merge/repack representation without changing logical meaning
```

This is useful beyond dataflow:

```text
provenance histories
checkpoint deltas
telemetry windows
cache metadata
incremental indexes
versioned scientific results
```

The planner may optimize when/how to compact, but a trusted validity/frontier rule determines **what may legally be forgotten**.

## 7. Checkpoint is a consistency cut, not a memory dump

Chandy–Lamport establishes that a distributed snapshot may need process state plus channel/in-flight state to represent a consistent global state.

Therefore:

```text
Checkpoint
    !=
Vec<MemoryImage>
```

A more general model is:

```text
Checkpoint = ConsistentCut + RecoveryClosure
```

where `RecoveryClosure` contains all state/effects needed to restore a legal equivalent execution state.

## 8. Recovery closure

Conceptually:

```text
RecoveryClosure {
    component_state,
    in_flight_effects,
    source_offsets,
    replay_positions,
    duplicate_suppression_state,
    external_commit_state,
    required_versions,
    maintenance_state,
}
```

Not every field is required for every protocol.

The closure is determined by:

```text
execution topology
transport semantics
mutation semantics
replay/recompute capability
external side effects
semantic contract
```

## 9. Topology-aware minimization

Asynchronous Barrier Snapshotting shows that under FIFO assumptions an acyclic dataflow can snapshot only operator state, whereas cycles require selected in-flight loop records.

Therefore checkpoint state can be **topology-dependent**.

A generic runtime may classify edges:

```text
QuiescentAtCut
Replayable
Logged
Recomputable
Transactional
ExternallyCommitted
```

and include only the state required to close the consistency proof.

## 10. Capture, recovery, and exactly-once are distinct

Keep separate:

```text
CheckpointCapture
RecoveryProtocol
ExternalEffectProtocol
```

A consistent snapshot does not automatically make arbitrary external effects exactly once.

For example, recovery may additionally require:

```text
sequence numbers
idempotency keys
transaction ids
sink commit protocol
compensation
```

## 11. Candidate checkpoint strategies

The admissible strategy space may contain:

```text
StopTheWorld
MarkerSnapshot
BarrierAligned
SelectiveEdgeLogging
SourceReplay
Recompute
IncrementalCheckpoint
Hybrid
```

Each strategy has different requirements and costs.

## 12. Checkpoint objective

After legality is established, a planner can minimize something like:

```text
ExpectedCheckpointCost =
    runtime_interference
  + capture_latency
  + persistent_bytes
  + network_bytes
  + expected_recovery_time * failure_probability
  + replay/recompute_cost
  + coordination_overhead
```

subject to hard constraints:

```text
consistent recovery
semantic contract
external side-effect correctness
resource limits
```

## 13. Interaction with `DerivedResource`

A derived resource may have:

```text
provenance
reuse_witness
maintenance_state
materialization
```

Checkpoint metadata is related but distinct.

A checkpoint captures a recoverable execution cut. Provenance explains derivation. A reuse witness establishes compatibility. Maintenance state accelerates future repair/reuse.

One physical structure may encode several roles, but the semantics should remain separate.

## 14. Interaction with transition protocol

A large transition may itself require a progress frontier or checkpoint cut:

```text
PROPOSE
VALIDATE
WAIT_FOR_FRONTIER
PREPARE
CAPTURE_RECOVERY_POINT?
ACT
VERIFY
COMMIT
```

For very cheap local transitions this machinery is unnecessary. The runtime must preserve the multiscale principle: use frontier/checkpoint coordination only where the semantics require it.

## 15. Proposed Rust vocabulary

Not yet an API commitment:

```rust
trait LogicalOrder {
    type Version;

    fn less_equal(a: &Self::Version, b: &Self::Version) -> bool;
}

struct VersionFrontier<V> {
    minimal: Vec<V>,
}

struct CheckpointPlan<R> {
    target: R,
    // protocol/domain-specific validated closure
}
```

The actual implementation should reuse Rust's ownership/capability model and avoid imposing heap allocation or generic graph machinery on simple resources.

## 16. SciRust relationship

SciRust remains external R&D tooling and is never an ElasticXxx runtime dependency.

This literature pass revealed a genuinely general algebraic need. `scirust-algebra` did not expose a partial-order/lattice/antichain family, so SciRust was enriched with general mathematical primitives:

```text
PartiallyOrdered
JoinSemilattice
MeetSemilattice
Lattice
TotalOrder<T>
ProductOrder2<A,B>
Antichain<T>
```

No timely-dataflow or Elastic-specific timestamp type was added.

## 17. Experiments

### A. Version model

Compare scalar epoch versus product-order versions on a two-axis incremental workload.

Measure reuse, metadata, coordination, and correctness.

### B. Compaction

Compare:

```text
no compaction
physical-only compaction
frontier-safe logical compaction
premature logical compaction (negative control)
```

### C. Checkpoint topology

Run logically equivalent DAG and cyclic graphs under:

```text
full channel logging
operator-only snapshot
selective edge logging
source replay
```

### D. Failures

Inject failures during capture, transfer, repair, and commit. Verify no lost/duplicated externally visible effects under an Exact semantic contract.
