# Mai et al. (2018) — Chi: A Scalable and Programmable Control Plane for Distributed Stream Processing Systems

## Status

**SOURCE-DERIVED mechanism review with ELASTIC PROPOSALS clearly separated.**

Primary source: Luo Mai et al., *Chi: A Scalable and Programmable Control Plane for Distributed Stream Processing Systems*, PVLDB 11(10), 2018.

## Problem

Long-running streaming computations need continuous monitoring and dynamic control operations such as scaling, checkpointing, parameter tuning, state repartitioning, and query-plan changes. Traditional control planes either expose only fixed operations or require global synchronization / freeze-the-world behavior.

Chi explores a programmable control plane whose control messages are carried through the same ordered dataflow channels as ordinary data.

## Control operation lifecycle

The paper describes three broad phases:

1. a controller makes a control decision and serializes the operation into a control message;
2. source operators receive/broadcast the message through the dataflow, and each operator executes local control actions; operators may attach state/configuration payload to the message;
3. sink-side completion messages return to the controller, which runs final processing and marks the operation complete.

This yields a feedback loop rather than a one-shot external mutation.

## Ordered control messages

Chi assumes channels support FIFO and exactly-once delivery in its implementation model. Control and data messages share those channels and ordering.

For state-affecting operations, a **blocking control message** can temporarily block an input channel until control actions for that operator complete. Non-blocking control messages allow data to continue and are useful for monitoring or operations that do not require the same state-consistency boundary.

This is not zero-cost control: at high data-plane load, blocking control messages can be delayed and can temporarily block input processing.

## Graph transformation

Chi models a control operation as a graph transformation:

```text
G -> G*
```

with state transformation functions mapping old operator states to new operator states.

Instead of stopping `G` and separately starting `G*`, Chi constructs an intermediate **meta-topology** `G'` containing relevant parts of the old and new graphs plus edges expressing state dependencies between them. Control messages propagate through `G'`; old-only operators shut down after their control actions, while new-only operators begin data processing after their initialization/control actions complete.

The general construction can be large, so Chi prunes it using invariance conditions such as unchanged state and acyclic graph preservation.

## Example: scale-out with state repartition

In the paper's word-count example, a control message introduces a third reducer.

The operation requires different local actions:

```text
mappers    -> update routing
old reducers -> checkpoint/split state and attach migrated partitions
new reducer  -> receive/merge state and install new function/configuration
controller   -> wait for completion acknowledgements
```

The operation therefore cannot be represented as one homogeneous command broadcast to every node. It is a distributed protocol with role-specific actions.

## Reactive control API

Chi exposes controller-side and operator-side event handlers for begin/next/complete/dispose stages of a control operation. Configurations can differ per operator.

The runtime manages invocation order and control-operation state; user logic specifies what each role should do.

This is relevant to Elastic because a generic transition may require **role-specific adapter actions** while preserving one higher-level transaction/operation identity.

## Correctness properties

The paper states termination and causal completion-order properties for a control operation. For “safe blocking control operations,” where control actions access operator state in the completion handler under the stated discipline, the semantics are equivalent to freeze-the-world execution according to the paper.

This is useful prior art for asynchronous execution with a synchronous semantic reference model.

## Multiple controllers

Chi permits several concurrent controllers, for example monitoring, checkpointing, and reconfiguration controllers. The paper explicitly requires **serializability of control messages for different control operations**.

This is a strong general lesson:

```text
legal operation A
+
legal operation B
```

is not enough to establish that arbitrary concurrent interleavings of A and B are legal.

## Congestion interaction

Because control messages share data channels, backpressure/congestion can delay the very control operation intended to relieve congestion. The paper discusses prioritizing control traffic but notes that separate priority queues would break the ordering relationship with data messages and complicate consistency, so the implementation preserves ordering.

This exposes a genuine control-plane design trade-off:

```text
fast emergency control
vs
causal ordering with data
```

A generic runtime must not assume both are simultaneously free.

## Fault tolerance

Chi integrates control messages with data-plane recovery. Its default failure policy checkpoints/replays dataflow state and reinserts lost control messages. The paper notes controller timeout/restart behavior for network partitions.

This reinforces that **control-operation state itself may need recovery semantics**.

## Evaluation

Reported results include:

- under the paper's high control load of 100 control messages/s, Chi remains low-overhead relative to the evaluated workloads; on the computation-intensive workload at 60 million events/s it reports under 20% latency penalty even at this high control rate;
- control-message completion time remains below 100 ms in the reported scalability test with 8,192 sources using broadcast/aggregation trees;
- in one 32-server scale-out experiment after doubling ingestion rate, Chi reports a temporary latency spike to 5.8 s and recovery to stable state within 6 s, versus the evaluated Flink Savepoint path dropping throughput to zero for five seconds and peaking at 35.6 s latency;
- in the skewed streaming-join reconfiguration example, the resulting topology reports 26% higher throughput and 61% lower latency after reconfiguration.

These are workload/system-specific experimental results, not universal guarantees.

## Elastic relation

### ADOPT

- Treat a complex reconfiguration as a **distributed control operation**, not a bag of unrelated mutations.
- Give the operation a stable identity/lifecycle and recoverability state.
- Make role-specific actions explicit.
- Require serializability or another explicit concurrency correctness model between simultaneous state-affecting control operations.
- Treat control-plane congestion/ordering as part of the cost and safety model.

### ADAPT

Generalize Chi's meta-topology to an Elastic **TransitionOperationGraph** that can contain both current and target resource/configuration nodes plus temporary transformation dependencies.

The graph need not be a streaming topology and its edges need not all be channels.

### REJECT

Do not require Elastic control messages to share data-plane transport. Chi shows benefits of this design, but it also couples control latency to data-plane congestion. ElasticXxx should support ordered/coordinated semantics without hardcoding one transport architecture.

Do not grant arbitrary control code direct authority over resources. Elastic's trusted validator/actuator boundary remains stronger than Chi's generic user control-operation model.

## Elastic proposal: reconfiguration transaction

A high-level object may conceptually be:

```text
ReconfigurationTransaction {
    id,
    source_configuration,
    target_configuration,
    transition_graph,
    participant_roles,
    consistency_mode,
    concurrency_class,
    recovery_state,
}
```

The planner proposes it; trusted validation establishes legality and concurrency compatibility; actuators execute role-specific stages.

## Elastic proposal: control-operation serializability

Potential concurrency classes:

```text
ReadOnlyObservation
CommutativeUpdate
DisjointResourceMutation
SerializedMutation
TransactionalMultiResource
```

This is design vocabulary, not a claim derived directly from Chi's API.

The validator should reject or order operations whose effects conflict.

## Interaction with staged migration

Megaphone decomposes one ownership migration into fine-grained units. Chi shows that a larger graph reconfiguration may simultaneously require:

```text
create destination resources
change routing
split/merge state
migrate units
install new logic
retire old resources
```

Therefore staged migration can be a subgraph inside a larger reconfiguration transaction.

## SciRust relationship

No new SciRust gap is established. Meta-topology execution, control-message transport, actor lifecycle, and distributed state transformation are runtime mechanisms. Existing graph/algebra/solver tools may support R&D experiments, but no Chi-specific infrastructure should be added to SciRust without an independent scientific need.

## Experiment required

Build two concurrent Elastic control operations that individually validate but conflict when interleaved (e.g. migrate a shard while another transaction changes its representation or destination).

Compare:

1. unconstrained concurrency;
2. global serialization;
3. resource-set conflict locking;
4. dependency-graph/transaction scheduling.

Measure correctness failures, unnecessary serialization, coordination overhead, tail latency, and recovery behavior.
