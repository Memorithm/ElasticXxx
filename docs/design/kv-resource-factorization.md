# KV Resource Factorization — Representation, Residency, Selection, and Contextual Utility

## Status

**ELASTIC PROPOSAL / RESEARCH DESIGN NOTE.** This note synthesizes mechanisms observed in vLLM/PagedAttention, FlexGen, H2O, Quest, InfiniGen, Llumnix, DistServe, Mooncake, IMPRESS, and the current SciRust KV research stack. It does not claim novelty.

## Motivation

The literature makes clear that "KV-cache state" is not one scalar resource dimension. Different systems independently manipulate:

- logical-to-physical block mapping;
- representation/compression;
- selected subset of entries;
- physical residency;
- replication;
- recomputability;
- version/epoch;
- request placement and execution phase.

At the same time, H2O and Quest show that **utility/importance is not necessarily intrinsic state**: it may depend on historical attention, the current query, layer, model, and other execution context.

A generic Elastic model should therefore avoid an enum such as:

```text
KvState = { HOT, WARM, COLD }
```

when those labels collapse multiple independent dimensions.

## Proposed factorization

```text
LogicalKvState =
    Identity
  × Representation
  × Residency
  × Redundancy
  × Persistence
  × Recomputation
  × Version
```

This notation is a conceptual factorization, **not an independence assumption**. The real admissible state set is generally a constrained subset:

```text
S_KV ⊆ Identity × Representation × Residency × ...
```

because dimensions can be coupled by capabilities, semantic constraints and cost interactions.

FlexGen provides a concrete example: enabling fine-grained 4-bit compression changes compression/decompression cost enough that CPU compute delegation becomes unattractive in the paper's implementation. Representation and compute placement therefore cannot be treated as independent additive choices.

### Identity

Stable logical object/block/token-range identity. It must not depend on current address or representation.

### Representation

Examples:

- full precision;
- INT8 / INT4;
- latent rank;
- residual slots;
- grouped scales;
- semantic HOT/WARM/COLD representation policy.

This is the axis strongly exercised by SciRust's current Elastic/Adaptive Latent KV research.

### Residency

Examples:

- GPU VRAM;
- local DRAM;
- remote DRAM;
- SSD;
- potentially multiple simultaneous locations during migration.

Mooncake, Llumnix, FlexGen, and vLLM exercise this axis more directly.

### Redundancy

Replica count and replica placement independent of primary logical identity. Mooncake's `change_replica` is a concrete prior-art mechanism.

### Persistence / recomputation

A logical resource may be physically absent yet reconstructible from retained prompt/model state. vLLM's swap-versus-recompute decision demonstrates that eviction need not imply loss of logical recoverability.

### Version

Representation models/bases/policies may be epoch-scoped. SciRust's committed-basis handoff already demonstrates this requirement in a concrete KV implementation.

## Selection state is distinct from object state

H2O, Quest, InfiniGen and IMPRESS all demonstrate that a bounded active subset may be selected from a larger logical resource.

Conceptually:

```text
LogicalResource
    └── ActiveSubset(context, policy, budget)
```

The subset is planning/runtime state, but it should not silently redefine the identity of the underlying logical resource.

For H2O, the selected set itself evolves under a bounded one-swap-like transition neighborhood. For Quest, the selected page set can change for every query.

## Importance is contextual utility, not intrinsic state

Quest provides especially strong evidence that a token/page can be unimportant for one query and critical for the next. H2O estimates usefulness from accumulated historical attention. IMPRESS combines access frequency with observed importance.

Therefore ElasticXxx should avoid an intrinsic field such as:

```text
resource.importance = ...
```

as a universal abstraction.

A better decomposition is:

```text
ResourceState
Context
UtilityModel
      ↓
UtilityEstimate(resource or subset | context)
```

or mathematically:

```text
U_t(S | c_t)
```

where `c_t` can include the current query, workload phase, topology, pressure, deadline, semantic contract, and other relevant state.

This permits utility to change without fabricating a physical resource-state transition.

## Utility need not be additive

H2O formulates bounded KV retention as a variant of dynamic submodular maximization. This demonstrates that generic planning should not assume:

```text
U(S) = Σ U(item)
```

Possible objective structures include:

```text
Additive
MonotoneSubmodular
Complementary / supermodular
Arbitrary black-box or learned
```

Planner-backend selection may depend on objective structure while resource semantics remain unchanged.

## Transition neighborhoods

For high-frequency adaptation, searching all admissible states can be unnecessary or too expensive.

H2O supplies a concrete pattern: consecutive retained sets differ only locally. Generalize this as:

```text
N(s) = { s' | s → s' is a legal cheap next transition }
```

A fast-path policy may search `N(s)` while a slower planner reasons about a much larger `ElasticSpace`.

This is compatible with the multiscale scheduling architecture derived from Cilk/A-STEAL/BWoS.

## Partial residency

InfiniGen and Quest demonstrate that residency/use may apply to a selected subset rather than the whole logical cache. Therefore a resource may need a subresource partition:

```text
Logical KV block set
    ├── currently critical subset → GPU / active attention
    └── remaining subset          → CPU / colder representation / inactive
```

The partition may be recomputed per query/layer and should not automatically redefine logical identity.

## Summary-based observability

Quest maintains per-page min/max key vectors and uses them with the current query to estimate an upper bound on page criticality before loading the entire page.

This suggests a general pattern:

```text
Resource
   ├── full state/data      // expensive to read/use
   └── maintained summary  // cheap selection metadata
```

A future abstraction might expose:

```text
ElasticSummary<R> / SelectionMetadata<R>
```

with explicit metadata such as:

```text
validity_epoch
covered_generation
update_cost
read_cost
statement_kind = Exact | Estimate | LowerBound | UpperBound
error/confidence semantics
```

The summary is not free: its maintenance and access cost must be accounted for.

## State-carrying planning-domain edges

DistServe demonstrates a useful abstraction:

```text
PlanningDomain A
    |
    | state handoff
    v
PlanningDomain B
```

The edge has properties such as:

```text
bytes
representation
source residency
target residency
network path
bandwidth
latency
recomputability
semantic impact
```

For LLM serving the edge is prefill→decode KV transfer. ElasticXxx should investigate whether the same abstraction generalizes to other domain boundaries.

## Migration protocol traits

Llumnix's append-only live migration suggests that transition protocols can exploit mutation semantics:

```text
MutableStateClass =
    Immutable
  | AppendOnly
  | Versioned
  | ArbitraryMutable
```

An append-only source permits concurrent copying of the stable prefix and a short final synchronization of the mutable tail. This could become a reusable optimization only when the resource adapter proves or declares the required mutation property.

## Candidate actions

A KV-domain Elastic prototype should distinguish at least:

```text
KEEP
RECOMPRESS
MIGRATE
SELECT_SUBSET
PREFETCH_SUBSET
REPLICATE
DROP_REPLICA
EVICT
EVICT_RECOMPUTABLE
RECOMPUTE
SWAP
DO_NOTHING
```

Actions may compose, for example `RECOMPRESS + MIGRATE`.

## Cost model

A candidate transition should not be ranked only by bytes moved. At minimum:

```text
TransitionCost =
    encode/decode cost
  + bytes / effective bandwidth
  + synchronization cost
  + topology penalty
  + lost locality
  + summary/observation cost
  + planner/prediction overhead
  + expected recomputation cost
  + risk / uncertainty
```

The effective bandwidth may itself depend on transfer size, concurrency and topology, as demonstrated by vLLM's swap/recompute comparison and Mooncake's RDMA/topology results.

FlexGen additionally demonstrates cross-costs: changing representation can change which compute-placement choices remain worthwhile.

## SciRust relationship

SciRust is an external scientific R&D platform and is never a runtime dependency of ElasticXxx.

Current SciRust repository evidence already covers:

- `PagedKvCache`;
- `ElasticKvCache`;
- adaptive latent K/V planning under strict budget;
- material HOT/WARM/COLD recompression;
- committed basis versions and epoch-scoped runtime handoff.

During this literature pass, SciRust was additionally enriched with two general combinatorial primitives:

1. exact deterministic **budgeted additive subset selection** using a sparse Pareto frontier;
2. deterministic greedy **monotone-submodular maximization under a cardinality budget**, with the classical `(1 - 1/e)` approximation guarantee stated only under the documented normalized/monotone/submodular assumptions.

These primitives are scientifically useful independently of KV caches and can support controlled experiments without coupling ElasticXxx to SciRust.

Distributed RDMA storage, live serving migration, request orchestration and attention-specific query scoring remain systems/domain mechanisms unless future evidence reveals a reusable scientific primitive that belongs in SciRust.

## Experimental program

For a first cross-domain KV stress test, compare:

1. physical-only adaptation;
2. representation-only adaptation;
3. selective-residency adaptation;
4. joint representation × residency optimization;
5. joint representation × residency × selection;
6. joint representation × residency × replication;
7. additive exact-oracle versus submodular greedy versus domain heuristics;
8. static utility versus historical utility versus query-conditioned utility;
9. full observation versus maintained summary/bound-based selection.

Measure:

- TTFT / TBT / throughput;
- bytes transferred/read;
- VRAM/DRAM/SSD occupancy;
- compression/decompression cost;
- quality / semantic error;
- planner and summary-maintenance cost;
- migration downtime;
- cache hit/reuse rate;
- selection recall/regret;
- transition churn;
- prediction error;
- failure/rollback behavior.
