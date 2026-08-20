# Wang et al. (2022) — Fries: Fast and Consistent Runtime Reconfiguration in Dataflow Systems with Transactional Guarantees

## Status

**SOURCE-DERIVED mechanism review with ELASTIC PROPOSALS clearly separated.**

Primary source: Zuozhi Wang, Shengquan Ni, Avinash Kumar, Chen Li, *Fries: Fast and Consistent Runtime Reconfiguration in Dataflow Systems with Transactional Guarantees*, PVLDB 16(2):256–268, 2022, DOI 10.14778/3565816.3565827.

Artifact reported by the paper: `Texera/Fries-Flink`.

## Problem

Fast control messages can reach selected operators without waiting behind all upstream data, reducing reconfiguration delay. But applying configuration updates independently at multiple operators can produce an execution in which one logical source tuple is processed partly under the old configuration and partly under the new one.

Fries asks how to preserve a transactional consistency property while synchronizing only the portion of the dataflow that actually needs ordering.

## Data transactions

For a source tuple `t`, the paper defines its **scope** as all tuples causally produced from `t`, with the DAG-induced partial order.

A data operation is processing one tuple at one operator:

```text
φ(tuple, operator)
```

The **data transaction** for source tuple `t` is the partially ordered set of all data operations in its scope.

This is important: one logical transaction can branch and contain several downstream tuple operations.

## Function-update transaction

A runtime reconfiguration over operators `{o1,...,on}` is modelled as a **function-update transaction**:

```text
{ μ(o1), ..., μ(on) }
```

where `μ(o)` applies the new configuration/function at operator `o`.

Updates to distinct operators are independent within the update transaction in the paper's model.

## Conflicts

A data operation and a function-update operation conflict exactly when they refer to the same operator:

```text
φ(t,o) conflicts with μ(o)
```

but:

```text
φ(t,o1) does not conflict with μ(o2), o1 != o2
```

## Conflict serializability

A schedule is **conflict-serializable** if it is conflict-equivalent to a serial schedule of the same transactions.

This gives Fries a precise consistency target rather than an informal rule such as “avoid mixing configurations.”

A bad schedule can have:

```text
data at operator A under old config
update A
update B
data from same transaction at B under new config
```

with conflicting-operation orderings that cannot correspond to any serial ordering of the data transaction and update transaction.

Epoch-based schedulers guarantee conflict-serializable schedules but can impose high delay because epoch markers travel from sources through the full relevant dataflow.

## Fast Control Messages are not sufficient alone

The naive FCM scheduler can update a downstream operator quickly without synchronizing causally related operations at another reconfigured operator. The paper shows such a schedule can violate conflict serializability.

Therefore:

```text
fast control delivery
!=
safe reconfiguration
```

A correctness closure is still required.

## Minimal Covering Sub-DAG (MCS)

For DAG `G=(V,E)` and set `M` of reconfigured operators, Fries defines the **Minimal Covering Sub-DAG** `G'=(V',E')` such that:

1. all operators in `M` are included;
2. if there is a path from reconfigured operator `A` to reconfigured operator `B`, all vertices/edges on that path are included;
3. no proper sub-DAG satisfies the first two conditions.

The paper states the MCS is unique and computable in `O(V+E)`.

The MCS is then divided into connected components (ignoring edge direction).

This is the key optimization: synchronization is confined to components that connect reconfiguration operations whose relative ordering can affect one data transaction.

## Fries scheduler for one-to-one operators

For each MCS component:

1. the controller sends an FCM directly to each **head** operator of the component;
2. the head applies its new configuration;
3. an epoch marker propagates only **inside the component**;
4. internal operators align markers on relevant component inputs before applying their updates;
5. marker propagation stops at the component boundary.

Thus upstream operators outside the MCS do not delay the initial control message.

The paper proves the resulting schedules are conflict-serializable for the one-to-one dataflow case (full proof referenced to the extended version).

When the MCS is the whole graph, Fries degenerates toward the epoch-based scheduler. Its advantage therefore depends on the synchronization closure being much smaller than the whole graph.

## One-to-many complication

A one-to-many operator can cause several downstream data operations from the **same source transaction** to reach one reconfiguration operator.

A naive MCS containing only the reconfigured downstream operator is insufficient: an FCM can arrive between two data operations belonging to the same source transaction, so one is processed under the old configuration and another under the new one.

Fries therefore expands the synchronization set by adding the **earliest one-to-many ancestors** of reconfiguration operators before computing the MCS.

This is a crucial lesson:

```text
consistency closure
```

cannot be computed from only the set of mutated resources. It also depends on the semantics of how one logical transaction expands through the graph.

## MCS pruning

The extended closure can become overly conservative, so the paper introduces pruning rules.

One rule permits pruning a one-to-many ancestor when, for the affected reconfiguration path, it behaves effectively as one-to-one with respect to each relevant transaction and only one output branch is affected.

The general lesson is that the safe synchronization closure should be **semantic**, not merely topological.

Graph reachability gives a conservative candidate closure; operator/data semantics can prove that some nodes do not need synchronization.

## Parallelism and fault tolerance

The paper extends the method to parallel execution and discusses fault tolerance in the Flink implementation. Fries uses RPC messages for FCMs and Flink checkpoint barriers for epoch markers inside MCS components.

The correctness property is therefore implemented with a hybrid:

```text
fast direct control to synchronization head
+
ordered/barrier synchronization only inside consistency closure
```

## Evaluation

The experiments use Apache Flink 1.13 on a GCP Dataproc cluster with one coordinator and ten workers, plus a separate HDFS cluster.

Representative reported results include:

- for a W2 reconfiguration of operators `J1,J4`, Fries reports 1,702 ms versus 12,361 ms for the epoch scheduler;
- for W3 `J5,J6`, where each MCS component contains only one operator, Fries reports 127 ms versus 8,352 ms for the epoch scheduler;
- reconfiguration delay grows with the longest path length in an MCS component;
- in the surge-handling experiment, epoch alignment is dominated by stragglers, while an MCS consisting only of the target operator allows Fries to send FCMs directly and avoid that upstream epoch delay;
- the paper's one-to-many MCS pruning experiments show very large reductions in some cases when expensive irrelevant ancestor paths can be removed.

These are workload/system-specific measurements, not generic performance guarantees.

## Limitations / assumptions

- The formalism is built around DAG dataflows and function-update reconfigurations.
- Conflict is defined at an operator/data-operation granularity specific to the model.
- The initial scheduler needs extension for one-to-many operators.
- Slow operators/stragglers inside the required MCS can still dominate delay.
- If the consistency closure is the whole graph, the benefit over epoch scheduling largely disappears.
- The pruning rules exploit specific operator semantics; arbitrary semantic pruning requires proof/validation.

## Elastic relation

### ADOPT

- Treat a complex state-affecting reconfiguration as a **transaction** with a correctness relation across operations.
- Compute a **minimal synchronization/consistency closure** rather than globally synchronizing unrelated resources.
- Separate fast control delivery from consistency enforcement.
- Allow semantic information to shrink a conservative graph-derived closure.
- Let independent consistency components reconfigure in parallel when their operations cannot conflict.

### ADAPT

Generalize Fries's operator conflict relation to Elastic resource transition effects:

```text
Read(resource)
Write(resource)
ChangeRepresentation(resource)
MoveAuthority(resource)
ChangeRoute(resource)
ChangeProtocol(resource)
...
```

A conflict model can use read/write/effect sets plus resource-specific commutativity/compatibility declarations.

Generalize MCS to:

```text
ConsistencyClosure(transaction, execution_graph, semantic_effects)
```

which returns the minimum validated dependency region that must observe one coherent ordering.

### REJECT

Do not hardcode conflict serializability as the universal consistency model for all Elastic operations. Some domains may require stronger atomicity; some may admit commutative/convergent operations with weaker coordination.

Do not infer closure only from graph topology. One-to-many behavior demonstrates that semantic transaction scope matters.

Do not let an urgent FCM-like fast path bypass the validator. Fast control is safe only when the required consistency closure is preserved.

## Elastic proposal: reconfiguration transaction

```text
ReconfigurationTransaction {
    id,
    operations,
    effect_sets,
    semantic_contract,
    consistency_model,
    consistency_closure,
    concurrency_dependencies,
    recovery_state,
}
```

The planner can optimize placement/order/timing only after a trusted layer determines which interleavings are legal.

## Elastic proposal: ConsistencyClosure

Conceptually:

```text
ConsistencyClosure(T) =
    minimal set of resources / edges / operations
    whose relative ordering must be coordinated
    so T satisfies its declared consistency model
```

Possible construction pipeline:

```text
changed-resource set
    ↓
conservative dependency/reachability closure
    ↓
expand for transaction fan-out / shared derivation
    ↓
prune using proven independence / commutativity / uniqueness
    ↓
validated consistency components
```

The closure itself can be represented as several independent connected components and executed in parallel.

## Elastic proposal: effect-aware static/runtime split

Rust types or adapter declarations may provide conservative effect metadata:

```text
reads
writes
moves ownership
changes representation
changes routing
```

But actual topology, resource generations, current routing, and dynamic fan-out may require runtime closure validation.

Therefore:

```text
static effect approximation
+
dynamic dependency graph
+
trusted consistency validator
```

is more realistic than compile-time-only serializability.

## SciRust relationship

No new SciRust gap is established by Fries.

The MCS computation is a graph-analysis mechanism and conflict serializability is a system consistency model. SciRust already contains broad graph and algebraic tooling that can be used during R&D; no Fries-specific scheduler or transaction runtime belongs in SciRust under the standing architecture rule.

## Experiment required

Build a resource graph with:

- two affected resources joined by a long dependency path;
- an unrelated expensive branch;
- a fan-out node producing multiple effects within one semantic transaction;
- some provably independent operations.

Compare:

1. global barrier;
2. topology-only minimal closure;
3. fan-out-aware closure;
4. semantic-pruned closure;
5. intentionally under-approximated closure as a negative correctness control.

Measure:

- reconfiguration delay;
- synchronization scope size;
- tail service latency;
- concurrency preserved outside the closure;
- validator/planner overhead;
- serializability/invariant violations;
- straggler amplification.
