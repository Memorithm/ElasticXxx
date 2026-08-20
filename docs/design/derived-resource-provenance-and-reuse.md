# Derived-Resource Provenance, Compatibility, and Repair

## Status

**ELASTIC PROPOSAL / RESEARCH DESIGN NOTE.** This note is motivated directly by CacheBlend (EuroSys 2025) and by earlier version/epoch lessons from SciRust's KV research, NOMAD, and planner-generation work. It does not claim novelty.

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

The generic core should probably not hardcode a universal provenance schema. Instead, the resource adapter can expose an opaque provenance identity plus compatibility logic.

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
}
```

For a KV cache, relevant fields can include text/token identity, model/layer identity, positional context, preceding-context dependencies, representation/basis epoch, and attention-related derivation assumptions.

## 4. Reuse is a relation

Avoid a universal Boolean field:

```text
reusable: bool
```

Reuse depends on both cached provenance and the requested target context.

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

CacheBlend's selective KV recomputation is a concrete example.

### Invalid

No currently authorized repair path establishes compatibility; rebuild/recompute from a valid source or reject reuse.

## 5. Partial repair

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

CacheBlend demonstrates token- and layer-selective KV repair.

## 6. Non-compositional correctness

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

## 7. Provenance and epochs are different

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

## 8. Provenance and logical identity are different

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

## 9. Cost of repair can overlap with transfer

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

## 10. Trusted validation

A planner can recommend reuse or repair, but it must not decide semantic compatibility by fiat.

Preferred boundary:

```text
Planner
  -> ReuseRecommendation
  -> Domain Compatibility Validator
  -> Exact reuse / validated repair / rebuild
```

The validator can reject stale provenance even if the planner predicts reuse would be faster.

## 11. Interaction with semantic contracts

`Exact` reuse requires the domain to establish equivalence appropriate to the contract.

Empirical quality preservation is not automatically an `Exact` proof.

For approximate repair:

```text
BoundedApproximation(error_bound)
BestEffort(...)
```

may admit repair methods that an `Exact` contract rejects.

## 12. Relationship to SciRust

SciRust remains an external scientific R&D platform, never an ElasticXxx runtime dependency.

Current SciRust KV research already demonstrates explicit basis versions/epochs and deterministic handoff semantics. The repository inspection performed during the CacheBlend review did not identify a generic KV derivation-provenance abstraction for preceding-context dependency.

This is **not a SciRust gap by itself**. Provenance/compatibility is principally target-resource semantics. A general scientific provenance facility should only be added to SciRust if independent scientific workflows demonstrate a reusable need.

## 13. Experiments

**EXPERIMENT REQUIRED.** Build a derived-resource test harness with intentionally stale/incompatible materializations and compare:

1. unconditional reuse;
2. exact compatibility validation;
3. full rebuild;
4. partial repair;
5. provenance-aware planner + trusted validator;
6. stale version but compatible provenance;
7. same version but incompatible provenance;
8. sequential versus overlapped repair/transfer.

Measure false reuse acceptance, false rejection, semantic error, repair fraction, validator overhead, critical-path transition cost and total useful progress.
