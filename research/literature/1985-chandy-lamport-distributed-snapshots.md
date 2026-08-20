# Chandy & Lamport (1985) — Distributed Snapshots

## Status

**SOURCE-DERIVED mechanism review with ELASTIC PROPOSALS clearly separated.**

Primary source: K. Mani Chandy and Leslie Lamport, *Distributed Snapshots: Determining Global States of Distributed Systems*, ACM TOCS 3(1), 1985.

## Problem

A distributed system has no single shared clock or memory from which one process can instantaneously read a complete global state. Nevertheless, many tasks require a meaningful consistent global state, including stable-property detection and checkpointing.

The paper asks how processes can record local process state plus communication-channel state **while the underlying computation continues**.

## System model

The paper models a finite set of processes connected by directed communication channels. The principal presentation assumes channels are reliable and FIFO, with arbitrary finite message delay.

A global state consists of:

```text
all process states
+
all channel states (messages sent but not yet received)
```

A snapshot is therefore not merely the tuple of local memory images. In-flight communication is part of the distributed state when required by the consistency cut.

## Marker protocol

When a process records its local state, it sends a special **marker** on outgoing channels before sending subsequent ordinary messages.

When another process receives its first marker, it records its local state. For each channel, the recorded channel state is determined by messages received between the process's local-state recording point and the marker arrival, according to the protocol rules.

The marker does not change the underlying application computation.

## Consistency

The algorithm constructs a global state consistent with the communication ordering even though local components are not all recorded at one physical instant.

This is the key lesson for ElasticXxx:

```text
consistent checkpoint
    !=
all components copied at the same wall-clock instant
```

What matters is the consistency relation among recorded component states and in-flight effects.

## Stable properties

The paper motivates global snapshots partly through stable properties: predicates that, once true, remain true in all later reachable states. Termination and deadlock are examples in the paper's model.

ElasticXxx should not generalize every invariant as a stable property; many resource conditions are transient. But the distinction is useful when a planner or verifier reasons about a predicate whose truth is monotonic over execution.

## Checkpointing lesson

A recovery point for a distributed computation may require:

```text
ComponentState
Channel/InFlightState
ConsistencyRelation
```

The exact shape depends on communication semantics and snapshot protocol.

Therefore a generic `CheckpointState = Vec<ComponentSnapshot>` model is insufficient unless the domain can prove that in-flight effects are either absent, reproducible, already reflected exactly once, or otherwise safely excluded.

## Assumptions matter

The classic algorithm relies on assumptions including reliable FIFO channels in its presentation. An Elastic checkpoint protocol must make such assumptions explicit as capabilities/preconditions rather than silently inheriting them.

Potential protocol properties include:

```text
FIFO
reliable delivery
sequence-numbered
idempotent replay
exactly-once sink
recomputable messages
transactional channel
```

Different properties permit different checkpoint representations.

## Elastic relation

### ADOPT

- Define checkpoint correctness by a **consistent cut/state**, not simultaneous physical copying.
- Treat in-flight effects as part of recoverability unless a proof/protocol permits their omission.
- Make communication assumptions explicit.
- Keep snapshot logic concurrent with useful execution where feasible.

### ADAPT

ElasticXxx should generalize beyond message channels to any transition with in-flight effects:

```text
DMA transfer
storage write
network RPC
GPU kernel
migration copy
queued task
replication stream
```

The generic question becomes:

> What state/effects must be captured or reconstructible so recovery corresponds to a legal execution state?

### REJECT

Do not hardcode Chandy-Lamport markers as the universal snapshot implementation. Later systems exploit topology and domain properties to reduce what must be persisted.

## Elastic proposal: RecoveryClosure

A future checkpoint model can distinguish the requested logical resource state from the **closure of state required to restore it consistently**:

```text
RecoveryClosure(target_cut) =
    component state
  + necessary in-flight effects
  + source offsets / replay positions
  + metadata required to suppress duplicates
  + external side-effect commitments
```

This is an ELASTIC PROPOSAL derived from distributed snapshot prior art.

## Elastic proposal: CheckpointContract

A checkpoint transition should state something conceptually like:

```text
CheckpointContract {
    consistency,
    channel_assumptions,
    replay_semantics,
    external_side_effect_semantics,
    recovery_scope,
}
```

For `SemanticContract::Exact`, a restored execution must preserve the required observable semantics, not merely restore similar memory contents.

## Experiment required

Implement a two-process test harness with messages deliberately in flight during checkpointing. Compare:

1. local-state-only checkpoint;
2. stop-the-world snapshot;
3. marker-based consistent snapshot;
4. sequence-number/replay-based recovery.

Inject failures at adversarial points and verify duplicate, lost, and reordered effects.
