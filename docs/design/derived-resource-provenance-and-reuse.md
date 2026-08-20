# Derived-Resource Provenance, Compatibility, and Repair

## Status

**ELASTIC PROPOSAL / RESEARCH DESIGN NOTE.** This note is motivated by CacheBlend (EuroSys 2025), Provenance Semirings (PODS 2007), Adaptive Functional Programming (POPL 2002), DBToaster (PVLDB 2012), Build Systems à la Carte (ICFP 2018), and earlier version/epoch lessons from SciRust's KV research, NOMAD, and planner-generation work. It does not claim novelty.

## 1. Problem

A logical source object and a materialized derivative are not necessarily the same resource identity.

CacheBlend supplies a concrete counterexample. Let `C` be a reusable text chunk. A KV cache produced for `C` in isolation can differ from the KV cache produced for the same `C` when `C` follows another context chunk, because the latter derivation includes cross-attention to preceding tokens.

Therefore:

```text
same source content
    !=
same derived materialization
```

and:

```text
source identity
    !=
derivation provenance
```

The database, incremental-computation and build-system literature independently shows that this distinction generalizes beyond KV caches.

## 2. Source identity versus materialization identity

A generic derived resource can be modelled conceptually as:

```text
DerivedResource {
    logical_source,
    derivation_provenance,
    materialized_state,
}
```

The source can remain stable while different valid materializations exist.

Examples beyond KV caches include:

- compiled artifacts produced under different target/compiler settings;
- memoized function results with different dependency versions;
- materialized database views;
- derived indexes;
- checkpoints derived from different upstream states;
- generated embeddings/features under different model versions.

## 3. Derivation provenance

The generic core should not assume that provenance is one hash or one flat tuple of identifiers.

Provenance Semirings provides prior art showing that an output can have multiple alternative derivations and derivations that jointly depend on several inputs. Algebraic provenance can therefore be structurally richer than a single source id.

A domain-specific provenance object may include:

```text
DerivationProvenance {
    source_ids,
    source_versions,
    dependency_context,
    transform_id,
    transform_version,
    ordering_or_position_context,
    model_or_kernel_version,
    generation_epoch,
    alternative_or_joint_derivations,
}
```

For a KV cache, relevant fields can include text/token identity, model/layer identity, positional context, preceding-context dependencies, representation/basis epoch, and attention-related derivation assumptions.

The exact representation may be opaque to Elastic core and interpreted by a domain adapter.

## 4. Provenance, reuse witness, and maintenance state are distinct

The literature supports three related but non-identical objects.

### 4.1 Derivation provenance

Answers:

```text
How was this materialization derived?
```

It can be explanatory, algebraic, multi-source, or otherwise rich.

### 4.2 Reuse witness / verification trace

Answers:

```text
Is the existing materialization still valid for this requested use?
```

Build Systems à la Carte gives a concrete prior-art mechanism: verifying traces can retain dependency hashes sufficient to decide whether a key needs rebuilding, without storing every semantic detail of its complete lineage.

A reuse witness may therefore be much smaller than full provenance.

### 4.3 Maintenance state / repair index

Answers:

```text
If something changed, how can affected derived state be repaired cheaply?
```

Adaptive Functional Programming maintains dependency/trace information so change propagation can identify affected computations. DBToaster materializes first- and higher-order delta views to make future updates cheap.

Maintenance state is an optimization structure, not the derived result itself.

### Consequence

Avoid collapsing these into one field:

```text
provenance != reuse_witness != maintenance_state
```

They may share information in a concrete implementation, but their semantics and lifecycle differ.

## 5. Reuse is a relation

Avoid a universal Boolean field:

```text
reusable: bool
```

Reuse depends on both cached provenance/witnesses and the requested target context.

Conceptually:

```text
ReuseCompatibility(cached, target_context)
```

can produce:

```text
Exact
Repairable(RepairPlan)
Invalid
```

These names are Elastic proposals, not CacheBlend terminology.

### Exact

The cached artifact can be reused without changing the relevant semantics under the active contract.

### Repairable

The cached artifact is not directly valid, but a legal repair transition can materialize a compatible state more cheaply than rebuilding everything.

CacheBlend's selective KV recomputation and self-adjusting computation's change propagation are concrete examples of the broader repair idea.

### Invalid

No currently authorized repair path establishes compatibility; rebuild/recompute from a valid source or reject reuse.

## 6. Partial repair

A derived object need not be rebuilt atomically as a whole.

Conceptually:

```text
REPAIR_DERIVED_STATE {
    target_provenance,
    affected_subset,
    method,
}
```

A repair protocol must specify:

- which subset is stale/incompatible;
- how repaired elements are produced;
- whether unrepaired elements remain valid;
- whether old/new representations may coexist temporarily;
- verification criteria;
- failure/rollback/compensation behavior.

Adaptive Functional Programming provides a particularly strong correctness reference: affected subexpressions are reevaluated while unaffected trace regions are reused, and the result is proven equivalent to complete reevaluation under its semantics.

## 7. Repairability can be purchased with auxiliary state

DBToaster demonstrates that extra materialized delta views can drastically reduce future view-maintenance work. Self-adjusting computation similarly retains dependency information to accelerate change propagation.

Therefore:

```text
cheap future repair
```

often requires paying today for:

```text
maintenance memory
metadata updates
synchronization
persistence
validation
```

A generic planner should be allowed to reason about whether maintaining repair accelerators is worthwhile.

Conceptually:

```text
MaintenanceState {
    footprint,
    update_cost,
    validity,
    expected_future_repair_benefit,
}
```

This is an Elastic proposal, not one paper's API.

## 8. Repair is not automatically cheaper than recompute

DBToaster explicitly motivates cost-based materialization because some delta computations can cost more than reevaluating the original expression.

Therefore the action set should contain competing alternatives:

```text
REUSE
REPAIR_INCREMENTALLY
REPAIR_USING_MAINTENANCE_STATE
FULL_RECOMPUTE
DO_NOTHING
```

The planner should compare actual expected costs rather than assume incremental work is always preferable.

## 9. Non-compositional correctness

Individually valid artifacts are not necessarily valid when concatenated/composed.

This means a generic rule such as:

```text
valid(A) && valid(B) => valid(compose(A,B))
```

is unsafe unless the resource algebra/protocol proves compositionality.

For derived resources, composition validation may need to inspect:

```text
source compatibility
version compatibility
dependency closure
ordering/position context
semantic contract
transform compatibility
```

## 10. Provenance and epochs are different

`Version`/`Epoch` answers roughly:

```text
which committed generation produced/authorizes this state?
```

`Provenance` answers:

```text
from what inputs/dependencies/context was this state derived?
```

Two artifacts can share an epoch yet have incompatible derivation contexts. Conversely, two different epochs may still be semantically reusable if a compatibility rule establishes equivalence.

Therefore provenance should not be collapsed into `ResourceGeneration` or `PlannerEpoch`.

## 11. Provenance and logical identity are different

The stable logical identity should remain stable across legal representation/residency changes.

Provenance instead characterizes a **materialization lineage**.

One possible conceptual split:

```text
LogicalResourceId
MaterializationId
DerivationProvenanceId
ResourceGeneration
```

The exact Rust API remains an open design question.

## 12. Validity policy and repair scheduling are separate

Build Systems à la Carte separates a **rebuilder** (whether/how a key should be rebuilt using persistent build information) from a **scheduler** (which keys are processed and in what order).

Elastic should investigate an analogous split:

```text
Compatibility / Validity Validator
        ↓
Repair/Rebuild Candidates
        ↓
Repair Scheduler / Planner
```

Correctness of the materialization is a constraint; minimizing repair work, latency, energy or resource cost is an objective.

## 13. Cost of repair can overlap with transfer

CacheBlend pipelines selective recomputation with retrieval of the next KV layer. This means transition cost can require a DAG rather than a scalar sum.

Instead of only:

```text
Cost = transfer + repair
```

we may need:

```text
TransitionOperationGraph
    nodes = encode / transfer / recompute / verify / materialize / ...
    edges = dependencies
    resources = CPU / GPU / network / storage / ...
```

and estimate:

```text
Cost(plan) = critical_path / makespan under contention
```

This also generalizes to prefetch-overlap, checkpointing, migration, compilation and asynchronous provisioning.

## 14. Higher-order maintenance state

DBToaster demonstrates that maintenance state can itself be derived and maintained incrementally:

```text
Q
├── ΔQ
├── Δ²Q
└── ...
```

This suggests that an Elastic resource graph may contain resources whose purpose is solely to accelerate maintenance of another resource.

Such resources still consume capacity, bandwidth, persistence and update effort and must therefore be observable/accounted.

## 15. Trusted validation

A planner can recommend reuse or repair, but it must not decide semantic compatibility by fiat.

Preferred boundary:

```text
Planner
  -> ReuseRecommendation
  -> Domain Compatibility Validator
  -> Exact reuse / validated repair / rebuild
```

The validator can reject stale provenance even if the planner predicts reuse would be faster.

## 16. Interaction with semantic contracts

`Exact` reuse requires the domain to establish equivalence appropriate to the contract.

Empirical quality preservation is not automatically an `Exact` proof.

For approximate repair:

```text
BoundedApproximation(error_bound)
BestEffort(...)
```

may admit repair methods that an `Exact` contract rejects.

## 17. Updated generic model

A more complete conceptual derived-resource model is now:

```text
DerivedResource {
    logical_identity,
    materialization,
    provenance,
    reuse_witness,
    maintenance_state,
    generation,
}
```

with domain-defined operations:

```text
validate_reuse(target_context)
identify_affected_region(change)
repair(...)
full_recompute(...)
verify(...)
```

This is intentionally a semantic model, not yet a Rust API.

## 18. Relationship to SciRust

SciRust remains an external scientific R&D platform, never an ElasticXxx runtime dependency.

The Provenance Semirings review exposed a genuinely general algebraic omission: `scirust-algebra` had Magma/Semigroup/Monoid/Group and Ring/Field abstractions but no Semiring. A generic `Semiring` / `CommutativeSemiring` abstraction and non-breaking ring adapter were therefore added to SciRust. No database-specific provenance mechanism was added.

Generic change propagation, build traces and DB view maintenance were not added to SciRust because the current evidence identifies them primarily as systems/runtime mechanisms rather than missing scientific primitives.

## 19. Experiments

**EXPERIMENT REQUIRED.** Build a derived-resource test harness with intentionally stale/incompatible materializations and compare:

1. unconditional reuse;
2. exact compatibility validation;
3. full rebuild;
4. partial repair;
5. provenance-aware planner + trusted validator;
6. stale version but compatible provenance;
7. same version but incompatible provenance;
8. sequential versus overlapped repair/transfer;
9. full provenance validation versus compact reuse witness;
10. no maintenance state versus maintained dependency/delta structures;
11. incremental repair versus full recompute as change size varies.

Measure false reuse acceptance, false rejection, semantic error, repair fraction, witness/provenance overhead, maintenance-state memory/update cost, validator overhead, critical-path transition cost and total useful progress.
