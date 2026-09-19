# Elastic embedded language v0.3 — resource groups and shared constraints

Status: typed cross-resource grouping over the v0.2 multi-resource document.
This version adds group topology, shared pseudo-Boolean capacity budgets and
explicitly owned cross-resource invariants. It does **not** add composite
actuation or transaction atomicity.

## Syntax

```rust
use elastic::prelude::*;

elastic! {
    pub document drone_stack {
        resource flight {
            class(configurational);
            id("flight");
            allow(capacity);
        }
        resource inference {
            class(configurational);
            id("inference");
            allow(capacity, energy);
        }
        resource vision {
            class(configurational);
            id("vision");
            allow(capacity, energy);
        }

        group drone_runtime {
            members(flight, inference, vision);
            depends(inference -> flight);
            depends(vision -> flight);

            budget memory {
                unit("gib");
                quantum(1);
                maximum(10);
                term(inference, predicate("elastic.drone", "inference-high"), 8);
                term(vision, predicate("elastic.drone", "vision-high"), 6);
            }

            invariant(
                contract("flight-priority-preserved"),
                owner(flight),
                participants(flight, inference, vision)
            );
        }
    }
}

let grouped = drone_stack::grouped_document()?;
```

Resource references inside a group are Rust child-module names from the same
document. The macro resolves each child module through its ordinary
`resource_spec()` function and uses the resulting `LogicalResourceId`. This
avoids duplicating or guessing logical IDs inside group syntax.

## Group semantics

`ResourceGroup` is the typed authority for one group. It owns:

- a stable `ResourceGroupId`;
- a non-empty canonical member set;
- directed `ResourceDependency` edges;
- zero or more `SharedBudget` contracts;
- zero or more `CrossResourceInvariant` contracts.

Members, dependencies, budgets and invariants are normalized deterministically.
Construction rejects duplicate members, unknown dependency endpoints,
self-dependencies, duplicate dependency edges and dependency cycles.

A logical resource may belong to more than one group in the same grouped document.
Groups are overlapping policy/topology scopes in v0.3, not exclusive ownership or
transaction partitions. Any future rule requiring exclusive ownership must be an
explicit ELANG4+ contract rather than inferred from group membership.

`depends(a -> b)` means **resource `a` depends on resource `b`**. In v0.3 this
is structural intent only. It is not yet an instruction to prepare, actuate,
verify, commit or roll back in a particular order. Composite execution semantics
belong to ELANG4.

## Shared budgets

A shared budget is a source-mapped capacity constraint over stable predicate
keys. Each term identifies the resource that owns the cost:

```text
sum(weight_i * predicate_i) <= maximum
```

All weights are strictly positive and use one explicit `PseudoBooleanScale`
(unit + integer quantum). The actual constraint is built through the existing
`PseudoBooleanConstraintDeclaration::capacity_budget` implementation; ELANG3
does not create a second pseudo-Boolean evaluator.

Example:

```text
budget memory {
    unit("mib");
    quantum(1);
    maximum(4096);
    term(inference, predicate("elastic.drone", "inference-high"), 3072);
    term(vision, predicate("elastic.drone", "vision-high"), 2048);
}
```

The resource-to-predicate attribution is preserved in EIR separately from the
pseudo-Boolean constraint itself. Swapping which resource owns two equal/global
cost terms therefore changes the group fingerprint.

## Cross-resource invariants

A cross-resource invariant records:

- an external `ContractId`;
- exactly one owning resource;
- two or more canonical participants.

The owner must itself be a participant. Every participant must be a member of
the surrounding group.

ElasticXxx does not interpret the contents of the external contract in this
layer. The owner identifies which future trusted adapter/transaction boundary is
responsible for validating the contract before physical effect. This is an
explicit ownership record, not proof that the contract currently holds.

## EIR lowering

The v0.2 `EirDocument` remains unchanged and retains its historical
fingerprint. Group data is carried by a separate versioned envelope:

```text
EirDocument
    + ResourceGroup[]
        -> EirGroupedDocument
```

`EirGroupedDocument` validates that every group member exists in the base
document and stores canonical `EirResourceGroup` entries. Group fingerprints
bind:

- group identity;
- member identities;
- dependency topology;
- source-mapped shared-budget terms and pseudo-Boolean constraint identity;
- cross-resource contract, owner and participants.

`group_resource(group, resource)` maps a group member back to the authoritative
`EirResource` from the base document.

Group input order and resource declaration order do not change the resulting
fingerprints.

## Error boundary

`grouped_document()` returns
`Result<EirGroupedDocument, ElasticGroupDocumentError>`.

The error preserves underlying typed failures from:

- ordinary child `ResourceSpec` validation;
- resource-group validation;
- shared-budget validation;
- cross-resource invariant validation;
- predicate-key validation;
- pseudo-Boolean scale validation;
- grouped-EIR validation.

The macro validates only syntax-level references, such as a group naming a
resource module that does not exist. Semantic rules such as dependency-cycle
rejection remain in `elastic-core` and are not duplicated in the proc macro.

## Bounds

The contracts remain explicitly bounded:

- `MAX_RESOURCE_GROUP_MEMBERS = 256`;
- `MAX_RESOURCE_GROUP_DEPENDENCIES = 1024`;
- `MAX_RESOURCE_GROUP_SHARED_BUDGETS = 64`;
- `MAX_RESOURCE_GROUP_CROSS_INVARIANTS = 64`;
- `MAX_EIR_RESOURCE_GROUPS = 64`.

These are structural/resource-exhaustion bounds, not claims about the maximum
size of a distributed control system.

## Explicit non-goals

v0.3 does not define:

- composite plan envelopes;
- prepare ordering between resources;
- coordinated multi-resource validation;
- atomic commit across resources;
- rollback after partial multi-resource failure;
- irrecoverable partial-failure semantics;
- cancellation/shutdown semantics for an in-flight composite transaction;
- policy blocks or planner hints.

Those are ELANG4/5 responsibilities and must consume the typed ELANG3 topology
rather than inventing another group model.
