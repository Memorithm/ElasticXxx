# Reconfiguration Serializability and Consistency Closure

## Status

**ELASTIC PROPOSAL / RESEARCH DESIGN NOTE.**

This note synthesizes Fries (PVLDB 2022), Chi (PVLDB 2018), Megaphone, staged migration, version frontiers, and the trusted planner/validator/actuator boundary. It does not claim novelty.

## 1. Reconfiguration is a transaction, not a bag of mutations

A complex runtime change may contain several operations:

```text
change function/configuration
change representation
move authority
change routing
create/delete replica
resize allocation
migrate state
```

Even if every operation is individually legal, their interleaving with application work can violate semantics.

Therefore model the high-level operation as a `ReconfigurationTransaction` with an explicit consistency model.

## 2. Application work also has transaction scope

Fries demonstrates that one source event can create a partially ordered family of downstream operations. Similar scopes can occur outside dataflow:

```text
one request -> several service calls
one tensor -> several pipeline stages
one logical write -> replicas/index updates
one job -> several resource-domain actions
```

A consistency analysis must therefore understand the semantic scope of application work, not just mutable objects.

## 3. Effect model

Potential conservative effects:

```text
Read(R)
Write(R)
MoveAuthority(R)
ChangeRepresentation(R)
ChangeRouting(R)
ChangeProtocol(R)
CreateReplica(R)
DropReplica(R)
```

Two operations can conflict when their effects cannot commute while preserving the declared semantic contract.

The default rule should be conservative. Domain adapters can prove narrower compatibility/commutativity.

## 4. Consistency model is explicit

Candidate models may include:

```text
Atomic
ConflictSerializable
SnapshotIsolated-like
SingleWriterOrdered
CommutativeConvergent
DomainSpecific
```

These names are research vocabulary, not committed APIs.

Fries provides strong prior art specifically for **conflict serializability** of function-update and data transactions. ElasticXxx should not impose that model universally.

## 5. Consistency closure

Define conceptually:

```text
ConsistencyClosure(T, G, semantics)
```

as the smallest **validated** region of resources, dependencies, or operations that must coordinate to ensure transaction `T` satisfies its consistency model.

This differs from:

```text
RecoveryClosure   // enough state to recover
MigrationClosure  // enough state/work to hand off ownership
ConsistencyClosure // enough scope to order concurrent effects correctly
```

The physical metadata may overlap, but their correctness obligations differ.

## 6. Conservative construction

Possible pipeline:

```text
transaction operations
    ↓
changed/effected resources
    ↓
conservative dependency/reachability closure
    ↓
expand for semantic fan-out / transaction scope
    ↓
partition into independent components
    ↓
prune using proven independence / commutativity / uniqueness
    ↓
validated consistency closure
```

Fries's Minimal Covering Sub-DAG is a concrete prior-art instance for one-to-one DAG dataflows. Its extension for one-to-many operators demonstrates the need for semantic expansion; its pruning rules demonstrate safe semantic reduction.

## 7. Why topology alone is insufficient

A graph can hide multiplicity semantics.

If one upstream operation emits several descendant operations belonging to the same semantic transaction, an update inserted between those descendants can create a mixed configuration even when only one downstream resource changes.

Therefore graph reachability is only a conservative structural input.

Required metadata may include:

```text
fan-out semantics
uniqueness/key constraints
commutativity
transaction grouping
ownership rules
version/provenance dependencies
```

## 8. Fast control plus local synchronization

A desirable architecture is:

```text
FAST CONTROL DELIVERY
    directly to heads of required consistency components

LOCAL CONSISTENCY PROTOCOL
    only inside each component
```

This preserves low control latency without globally bypassing ordering.

In Fries, FCMs reach component heads quickly while epoch markers synchronize only inside the MCS component.

ElasticXxx can generalize this without requiring control and data to share one transport.

## 9. Independent components

If the validated closure decomposes into independent components:

```text
C1, C2, ..., Cn
```

they may execute concurrently when the consistency model permits it.

This gives a principled basis for avoiding unnecessary global serialization of control operations.

## 10. Concurrency between reconfiguration transactions

Chi requires serializability among concurrent control operations. ElasticXxx needs a more explicit transaction scheduler/validator.

Potential relation:

```text
conflicts(T1,T2)
commutes(T1,T2)
disjoint(T1,T2)
depends_on(T1,T2)
```

Possible execution policy:

```text
independent -> concurrent
commutative -> concurrent under protocol
conflicting -> serialize or transactional coordination
ordered dependency -> enforce edge
```

A planner cannot decide that two operations commute merely because concurrency is faster.

## 11. Static and dynamic split

Rust can help expose conservative authority/effect information through types and capabilities.

Static declaration may answer:

```text
what classes of resources may be mutated?
what operation family is authorized?
what effects are possible?
```

Runtime validation must answer:

```text
which concrete resources are involved now?
what is current topology/routing?
what generations are current?
what fan-out/transaction dependencies apply?
what operations are concurrently active?
```

Thus:

```text
STATIC EFFECT BOUNDS
        ↓
DYNAMIC CONSISTENCY CLOSURE
        ↓
TRUSTED TRANSACTION VALIDATOR
```

## 12. Relationship to `TransitionOperationGraph`

Chi's meta-topology and Fries's MCS suggest separating two graphs:

```text
Execution/Resource Graph
    persistent topology of the running system

TransitionOperationGraph
    temporary graph of operations/dependencies needed for one reconfiguration
```

A transition graph can reference the `ConsistencyClosure` but should not be identical to it.

For example:

```text
ConsistencyClosure
    says which resources must coordinate

TransitionOperationGraph
    says how prepare/copy/align/update/verify/commit are executed
```

## 13. Relationship to staged migration

A staged migration can be one subtransaction or subgraph inside a broader reconfiguration:

```text
ReconfigurationTransaction
    ├── create target resources
    ├── staged state migration
    ├── update routing
    ├── install representation
    └── retire source resources
```

The consistency closure can span operations around the migration rather than only the bytes being moved.

## 14. Consistency-closure minimization is constrained optimization

The runtime would like to minimize synchronization scope because large closures increase:

```text
control delay
barrier waiting
straggler sensitivity
buffering
lost concurrency
```

But minimizing too aggressively is a correctness failure.

Therefore:

```text
minimize CoordinationCost(closure)
subject to closure proves consistency model
```

The proof/validation side is a hard constraint, not a weighted objective.

## 15. Choke points and stragglers

Fries shows that even a valid minimal closure can have high delay when it contains expensive/straggling operators.

A planner can therefore optimize which **legal** control path/protocol to use, but cannot prune a required resource solely because it is slow.

Potential alternatives:

```text
wait/barrier
versioned dual execution
temporary replica
transactional forwarding
recompute
```

only when supported by the adapter and semantic contract.

## 16. Proposed conceptual API

Not an implementation commitment:

```rust
struct ReconfigurationTransaction<T> {
    id: TransactionId,
    operations: Vec<T>,
    consistency: ConsistencyModel,
}

struct ConsistencyClosure<R> {
    components: Vec<ConsistencyComponent<R>>,
}

trait ConsistencyValidator<R, T> {
    fn derive_and_validate(
        &self,
        tx: &ReconfigurationTransaction<T>,
        graph: &ResourceGraph<R>,
    ) -> Result<ConsistencyClosure<R>, ConsistencyError>;
}
```

The actual design must avoid imposing heap-heavy graph structures on fast/local resource operations.

## 17. Planner boundary

Planner responsibilities:

```text
choose among already legal protocols
schedule independent components
optimize timing/order/pacing
predict cost and useful-progress impact
```

Trusted validator responsibilities:

```text
derive/conservatively check conflict scope
ensure consistency closure is sufficient
check generations/capabilities
reject unsafe pruning/interleavings
```

Actuator responsibilities:

```text
execute role-specific transition steps
preserve protocol ordering
report actual outcome
```

## 18. SciRust relationship

No runtime dependency.

Fries does not establish a missing SciRust capability. Generic graph and algebra tools can support offline/R&D study of closure algorithms. A transaction/control runtime is project-specific systems infrastructure.

## 19. Experimental program

Build a reconfiguration benchmark containing:

```text
independent branches
fan-out/fan-in
expensive irrelevant branch
straggling relevant branch
several simultaneous control transactions
```

Compare:

```text
global barrier
structural closure
semantic-expanded closure
semantic-pruned closure
concurrent independent components
multi-transaction serialization strategies
```

Include intentional unsafe under-approximations as negative controls.

Measure correctness first, then synchronization scope, control delay, tail latency, buffering, lost concurrency, and validation/planning overhead.
