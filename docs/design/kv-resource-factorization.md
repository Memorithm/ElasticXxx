# KV Resource Factorization — Representation vs Physical Residency

## Status

**ELASTIC PROPOSAL / RESEARCH DESIGN NOTE.** This note synthesizes mechanisms observed in vLLM/PagedAttention, InfiniGen, Llumnix, DistServe, Mooncake, and the current SciRust KV research stack. It does not claim novelty.

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

Mooncake, Llumnix, and vLLM exercise this axis more directly.

### Redundancy

Replica count and replica placement independent of primary logical identity. Mooncake's `change_replica` is a concrete prior-art mechanism.

### Persistence / recomputation

A logical resource may be physically absent yet reconstructible from retained prompt/model state. vLLM's swap-versus-recompute decision demonstrates that eviction need not imply loss of logical recoverability.

### Version

Representation models/bases/policies may be epoch-scoped. SciRust's committed-basis handoff already demonstrates this requirement in a concrete KV implementation.

## Partial residency

InfiniGen demonstrates that residency may apply to a selected subset rather than the whole logical cache. Therefore a resource may need a subresource partition:

```text
Logical KV block set
    ├── critical subset → GPU
    └── remaining subset → CPU
```

The partition may be recomputed per query/layer and should not automatically redefine logical identity.

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
  + planner/prediction overhead
  + expected recomputation cost
  + risk / uncertainty
```

The effective bandwidth may itself depend on transfer size, concurrency and topology, as demonstrated by vLLM's swap/recompute comparison and Mooncake's RDMA/topology results.

## SciRust relationship

SciRust is an external scientific R&D platform and is never a runtime dependency of ElasticXxx.

Current SciRust repository evidence already covers:

- `PagedKvCache`;
- `ElasticKvCache`;
- adaptive latent K/V planning under strict budget;
- material HOT/WARM/COLD recompression;
- committed basis versions and epoch-scoped runtime handoff.

During this literature pass, SciRust was additionally enriched with a general exact deterministic **budgeted subset selection** primitive. It is scientifically useful independently of KV caches and can support experiments involving utility/cost selection without coupling ElasticXxx to SciRust.

Distributed RDMA storage, live serving migration and request orchestration remain systems mechanisms unless future evidence reveals a reusable scientific primitive that belongs in SciRust.

## Experimental program

For a first cross-domain KV stress test, compare:

1. physical-only adaptation;
2. representation-only adaptation;
3. selective-residency adaptation;
4. joint representation × residency optimization;
5. joint representation × residency × replication;
6. specialized domain heuristics as baselines.

Measure:

- TTFT / TBT / throughput;
- bytes transferred;
- VRAM/DRAM/SSD occupancy;
- compression/decompression cost;
- quality / semantic error;
- planner cost;
- migration downtime;
- cache hit/reuse rate;
- transition churn;
- prediction error;
- failure/rollback behavior.
