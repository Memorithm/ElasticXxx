# RAM and storage capacity budget contracts v0.1

Status: ELANG7b static resource-policy contract.

ElasticXxx now exposes typed byte budgets for two capacity domains:

- `CapacityBudgetKind::Ram` with canonical unit `ram-bytes`;
- `CapacityBudgetKind::Storage` with canonical unit `storage-bytes`.

The domains intentionally remain distinct even though both use byte counts.
Equal numeric costs do not make a storage budget interchangeable with a RAM
budget, and their lowered EIR fingerprints remain different.

## Contract

A `CapacityBudgetContract` contains:

- a stable `SharedBudgetId`;
- one explicit capacity kind (`Ram` or `Storage`);
- an upper bound in bytes;
- one or more positive `CapacityBudgetTerm`s;
- each term binds a logical resource, a stable `PredicateKey`, and a byte cost.

Example:

```rust
use elastic::prelude::*;

let inference = LogicalResourceId::new("inference")?;
let kv = LogicalResourceId::new("kv")?;

let ram = CapacityBudgetContract::ram(
    SharedBudgetId::new("runtime-ram")?,
    vec![
        CapacityBudgetTerm::new(
            inference.clone(),
            PredicateKey::new("app", "large-model")?,
            6 * 1024 * 1024 * 1024,
        )?,
        CapacityBudgetTerm::new(
            kv.clone(),
            PredicateKey::new("app", "large-kv")?,
            2 * 1024 * 1024 * 1024,
        )?,
    ],
    7 * 1024 * 1024 * 1024,
)?;

let group = ResourceGroupBuilder::new(ResourceGroupId::new("edge-runtime")?)
    .members([inference, kv])
    .capacity_budget(ram)
    .build()?;
```

The example declares that the simultaneously true candidate predicates must fit
inside the static RAM policy. It does **not** establish that the host currently
has 7 GiB free.

## Existing pseudo-Boolean semantics remain authoritative

The typed contract lowers to the existing `SharedBudget` and then to the
existing non-negative pseudo-Boolean capacity constraint:

```text
Σ candidate_cost_bytes * predicate_truth <= maximum_bytes
```

There is no second budget evaluator. Missing predicate facts remain `Unknown`
under the existing three-valued pseudo-Boolean semantics. Negative costs are
impossible and zero-cost terms are rejected instead of being silently removed.

The scale quantum is exactly one byte. The semantic unit is part of the lowered
constraint identity, so otherwise-identical RAM and storage declarations cannot
alias in EIR fingerprints.

## Resource ownership

Every term names the logical resource that owns its cost. When the budget is
attached to a `ResourceGroup`, the existing group validator rejects any term
owned by a resource outside that group. This prevents a shared budget from
silently charging or depending on an undeclared external resource.

## Static / dynamic safety boundary

A capacity budget is static policy intent. It is suitable for pruning candidate
combinations and for documenting operator limits.

It is **not** evidence of:

- current free RAM;
- cgroup memory availability;
- filesystem free space;
- storage media health or writability;
- temporary allocation headroom;
- successful physical reservation.

Those facts remain dynamic observations and trusted-validation concerns. A
candidate that satisfies the static budget may still fail closed at the runtime
boundary if current physical evidence is missing, stale, or insufficient.

## Relationship to reservations

This v0.1 contract is an upper-bound budget. It does not yet model immutable
safety-critical reservations, minimum protected capacity, leases, or resource
transfer. Those are separate ELANG7 contracts and must not be encoded by
subtracting undocumented constants from this budget.

## Authority boundary

Constructing or satisfying a `CapacityBudgetContract` grants no actuation
authority. Normal Elastic trusted validation immediately before physical effect,
verification, and rollback remain mandatory.
