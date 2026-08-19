# Mechanism Review — Invasive Computing: Common Terms and Granularity of Invasion

**Paper:** Jürgen Teich, Wolfgang Schröder-Preikschat, Andreas Herkersdorf, *Invasive Computing — Common Terms and Granularity of Invasion*, arXiv:1304.6067, submitted 22 Apr 2013.

**Status:** mechanism review complete for this paper; broader Invasive Computing corpus still requires separate evaluation.

## Evidence discipline

Labels used below:

- **SOURCE-DERIVED** — directly supported by this paper.
- **INFERENCE** — interpretation derived from the source.
- **ELASTIC PROPOSAL** — proposed ElasticXxx direction; not attributed to the paper.
- **OPEN QUESTION** — not established.
- **EXPERIMENT REQUIRED** — must be tested.

This paper is principally a terminology, programming-model, runtime, and granularity paper. It discusses expected benefits and trade-offs, but it is not itself a quantitative benchmark paper demonstrating an end-to-end performance advantage. Claims of measured speedup or efficiency must therefore come from other Invasive Computing publications, not from this paper.

---

## 1. Problem

**SOURCE-DERIVED.** The paper targets future many-core MPSoCs where resource efficiency, predictability, faultiness, thermal effects, aging, power, and interference between concurrently running applications make static programming and resource sharing problematic.

A primary concern is predictability of non-functional properties such as execution time, safety, and security. The authors argue that isolation and exclusive resource use can reduce interference and make execution behavior more predictable.

---

## 2. Resource model

**SOURCE-DERIVED.** The core abstraction is a **claim**: a set of hardware resources made available to an invading process on demand and according to selected constraints.

A claim is not limited to processor cores. The paper explicitly states that a claim may contain:

- processing resources;
- memory resources;
- communication resources.

The dominant discussion, however, concerns physical processing elements, especially cores and tiles in MPSoCs.

**INFERENCE.** The model is therefore broader than a simple "number of CPU cores" allocator, but its fundamental unit remains a hardware-resource claim tied to an execution phase.

---

## 3. Core execution mechanism: invade → infect → retreat

**SOURCE-DERIVED.** The state model is:

```text
start
  ↓
invade
  ↓
infect
  ↓
retreat / invade again
  ↓
exit when the claim becomes empty
```

### `invade`

Constructs or expands a claim by acquiring resources according to constraints.

### `infect`

Dispatches application code onto the previously allocated claim. The parallel execution units are called **i-lets**.

### `retreat`

Shrinks the claim and cleans the affected processing elements from i-let entities. A retreat that empties the claim terminates the application execution.

A different program may also be infected onto the same claim if the degree of parallelism does not change.

**Classification:** **ADAPT.** ElasticXxx should retain the explicit distinction between allocation/state transition and execution, but should not make `claim` acquisition the universal form of elasticity.

---

## 4. Observability

**SOURCE-DERIVED.** Resource decisions may depend on temporal demand and resource state such as:

- utilization;
- load;
- temperature;
- faultiness;
- resource usage;
- permissions.

**Classification:** **ADOPT / GENERALIZE.** ElasticXxx should preserve explicit observation of both capacity and non-capacity state, while extending the observation model to heterogeneous resource dimensions such as memory pressure, locality, bandwidth, queueing, device residency, thermal state, energy, and transition history.

---

## 5. Constraints: a crucial similarity with ElasticXxx

**SOURCE-DERIVED.** The paper distinguishes **mandatory** and **optional** constraints at invasion time.

Mandatory constraints declare resource demands for an imminent computation phase and may indicate expected functional or non-functional benefit.

Optional constraints express tolerance or willingness regarding sharing, temporary undersupply, and temporary oversupply. Resources are exclusive by default, but exclusivity may be loosened through optional constraints.

The paper gives a concrete example equivalent to requesting an exclusive claim of four cores on the same tile.

**IMPORTANT CORRECTION TO EARLIER ELASTIC FRAMING.** It would be inaccurate to characterize Invasive Computing as merely an imperative "application asks for N cores" system while Elastic alone provides constraints. Invasive Computing already exposes a constraint-oriented claim interface.

The scientifically meaningful distinction must be narrower and stronger.

**ELASTIC PROPOSAL.** ElasticXxx should separate:

1. **semantic invariants** — never violated;
2. **physical constraints** — admissible hardware/runtime states;
3. **policy constraints** — latency, energy, isolation, sharing, deadlines, etc.;
4. **objectives** — quantities to optimize;
5. **resource mechanisms** — transitions available to satisfy the above.

The application should not need to request a particular hardware claim when several fundamentally different resource transformations could satisfy the same intent.

Example:

```text
Intent:
  preserve Exact semantics
  latency <= 20 ms
  minimize energy

Possible runtime mechanisms:
  use 2 CPU cores
  use 4 CPU cores
  move work to GPU
  increase parallelism
  change residency
  prefetch
  change batching
  combine several transitions
```

This is a proposed Elastic distinction and requires comparison against the broader Invasive Computing literature before any novelty claim.

**Classification:** **ADOPT** the explicit constraint concept; **ADAPT** the claim-centered interface.

---

## 6. User-oriented versus system-oriented objectives

**SOURCE-DERIVED.** The paper explicitly discusses conflict between user-oriented criteria such as response/cycle time and system-oriented criteria such as utilization. In iRTSS, predictable runtime behavior is prioritized, so user-oriented criteria dominate system utilization where necessary.

The runtime even tolerates internal tile fragmentation to preserve isolation and predictability.

**Classification:** **ADOPT AS PRINCIPLE.** ElasticXxx must not optimize utilization at the expense of explicit semantic or service constraints.

**ELASTIC PROPOSAL.** This supports a strict ordering:

```text
invariants / semantic contracts
        > hard constraints
        > policy priorities
        > optimization objectives
```

Utilization is therefore never an implicit supreme objective.

---

## 7. Isolation and exclusivity

**SOURCE-DERIVED.** Exclusive resource allocation is the default. This is used to reduce interference and improve predictability, including timing and other non-functional qualities. Optional constraints can loosen exclusivity and permit temporary oversupply or undersupply.

**INFERENCE.** Invasive Computing treats isolation as a first-class resource-allocation concern rather than merely a scheduler side effect.

**Classification:** **ADOPT / GENERALIZE.** ElasticXxx should represent isolation and sharing explicitly in capability/constraint metadata rather than treating them as incidental deployment properties.

Potential Elastic concepts:

```text
SharingMode::Exclusive
SharingMode::Partitioned
SharingMode::Shared
SharingMode::Opportunistic
```

These names are provisional.

---

## 8. Granularity: one of the strongest ideas to retain

**SOURCE-DERIVED.** The paper separates **granularity of invasion** from **granularity of infection**. Resource allocation may occur at tile granularity at one system layer while application-visible allocation appears at core granularity. Dispatch itself may occur at core granularity.

The authors explicitly call this a separation of concerns and allow different granularities at different abstraction levels.

**Classification:** **ADOPT / GENERALIZE.** ElasticXxx should not force one adaptation granularity across the stack.

**ELASTIC PROPOSAL.** Distinguish at least:

- **semantic granularity** — logical object or workload unit;
- **planning granularity** — state-space partition used by the planner;
- **allocation granularity** — units the underlying system can allocate;
- **transition granularity** — smallest movable/resizable/transformable unit;
- **execution granularity** — unit of actual dispatch/work.

Example:

```text
logical:       KV cache
planning:      hot / warm / cold regions
allocation:    memory pages / GPU allocations
transition:    chunks
execution:     tensor/kernel operations
```

**OPEN QUESTION.** Can these granularities be represented by one generic abstraction without excessive complexity?

---

## 9. Application/runtime split

**SOURCE-DERIVED.** Invasive Computing gives applications explicit ability to invade, infect, and retreat at points in program execution. Application development and algorithm design must therefore be aware of the paradigm.

**ELASTIC PROPOSAL.** ElasticXxx should investigate whether the application can specify **admissible adaptation** and **intent**, while the runtime decides when and how to traverse the resource state space.

This does **not** mean removing application authority. The application remains authoritative over semantic invariants and permitted transformations. The proposed difference is where mechanism selection lives.

Conceptually:

```text
Invasive model (simplified):
application
  → invade constrained resources
  → infect work
  → retreat resources

Elastic proposal:
application
  → declare invariants + constraints + objectives + admissible dimensions
runtime
  → observe
  → choose legal state/transition plan
  → execute/verify
application
  → continues against stable logical resource identity
```

**Classification:** **ADAPT / INVESTIGATE.** This is a candidate core distinction, but it must be compared against later declarative and autonomic resource-management work before claiming novelty.

---

## 10. Claim versus Elastic Resource

This is currently the most important conceptual distinction.

**SOURCE-DERIVED.** A claim designates hardware resources allocated to an invading process according to constraints.

**ELASTIC PROPOSAL.** An `ElasticResource` is intended to represent a stable logical resource whose admissible physical realization may change.

A claim answers primarily:

```text
Which hardware resources may this execution use now?
```

An Elastic resource is intended to answer:

```text
What logical resource exists?
Which dimensions of its realization may change?
Which states are admissible?
Which transitions are legal?
What invariants must hold across those transitions?
```

For example, a logical context may remain the same while residency changes:

```text
VRAM
  → VRAM + RAM
  → RAM
  → RAM + storage
  → prefetched back to VRAM
```

No claim is made yet that this separation is novel; it is the current Elastic research direction.

---

## 11. Transition model

**SOURCE-DERIVED.** Invasive Computing defines a compact transition vocabulary centered on:

- acquire/expand (`invade`);
- dispatch (`infect`);
- shrink/release (`retreat`).

**ELASTIC PROPOSAL.** ElasticXxx requires a broader transition algebra because not all adaptation is acquisition/release:

```text
Resize
Move
Split
Merge
Replicate
DropReplica
Compress
Decompress
ConvertRepresentation
Prefetch
Evict
Recompute
Checkpoint
Batch
Unbatch
Parallelize
Serialize
Route
Reroute
Pin
Unpin
```

Only transitions authorized by capabilities and semantic contracts may be considered.

**Classification:** **ADAPT STRONGLY.** `invade/retreat` become special cases of a more general transition relation rather than universal primitives.

---

## 12. Resource topology

**SOURCE-DERIVED.** The paper is topology-aware: cores belong to tiles; tiles communicate differently than cores within a tile; constraints can require cores from the same tile; allocation granularity can depend on topology and isolation requirements.

**Classification:** **ADOPT / GENERALIZE.** This strongly supports the proposed Elastic Resource Graph. Physical topology and interference relationships must influence legal transitions and cost models.

---

## 13. Temporary over- and undersupply

**SOURCE-DERIVED.** The paper explicitly models temporary oversupply and undersupply as phenomena that may be tolerated through optional constraints. Oversupply can arise from dispatching work to otherwise spare cores; undersupply may arise, for example, when an overheated core is temporarily masked.

**ELASTIC PROPOSAL.** ElasticXxx should represent such deviations explicitly rather than silently treating them as ordinary states. A policy may specify whether temporary constraint relaxation is legal and for how long.

Potential model:

```text
HardConstraint
SoftConstraint { penalty, max_duration }
```

This is provisional terminology.

---

## 14. Planning/controller mechanism

**SOURCE-DERIVED.** The paper describes iRTSS/OctoPOS/CiC responsibilities and rule-based dispatch behavior, but it does not present one general optimization algorithm over a heterogeneous resource state space.

The runtime enforces claim constraints and distinguishes user-oriented from system-oriented criteria.

**INFERENCE.** The planning problem is therefore narrower than the proposed Elastic planner: it primarily concerns satisfying and dispatching within claims rather than selecting arbitrary sequences of representation, residency, locality, replication, precision, or recomputation transitions.

---

## 15. Cost and overhead

**SOURCE-DERIVED.** The authors explicitly state that expected performance/resource-utilization benefits must be traded against overhead relative to static mapping.

However, this paper does not provide a quantitative transition-cost model or benchmark establishing those gains.

**Classification:** **ADOPT THE PRINCIPLE; EXTEND THE MODEL.** ElasticXxx should make transition cost a first-class property of candidate plans and verify predicted versus observed cost.

---

## 16. Safety, verification, and reversibility

**SOURCE-DERIVED.** Isolation, permissions, constraints, and rule-constrained dispatch are central. `retreat` explicitly releases resources and cleans i-let state from processing elements.

The paper does not define the generic transactional validation/verification/rollback protocol currently proposed for ElasticXxx across heterogeneous state transformations.

**ELASTIC PROPOSAL.** Elastic transitions should expose preconditions, postconditions, reversibility, verification, and failure/rollback semantics.

**OPEN QUESTION.** Which classes of transition can be made transactionally reversible without unacceptable overhead?

---

## 17. Results actually demonstrated in this paper

**SOURCE-DERIVED.** This paper establishes terminology, architecture concepts, constraint semantics, and granularity considerations for Invasive Computing.

It discusses expected benefits including improved utilization, performance, fault tolerance, power management, and predictability, but it explicitly notes that efficiency benefits must be carefully analyzed against overhead.

**Do not cite this paper as quantitative evidence that Invasive Computing improves performance by a particular percentage.** Separate evaluation papers are required for that.

---

## 18. Elastic mechanism decision matrix

| Invasive Computing mechanism | ElasticXxx decision | Reason |
|---|---|---|
| Runtime resource awareness | **ADOPT** | Fundamental requirement |
| Constraints on resource acquisition | **ADOPT / GENERALIZE** | Already close to Elastic intent |
| Mandatory vs optional constraints | **ADAPT** | Useful distinction; Elastic needs richer invariant/constraint/objective semantics |
| Exclusive claims / isolation | **ADOPT / GENERALIZE** | Predictability and interference matter |
| `invade` | **ADAPT** | Becomes one possible acquisition/resize transition |
| `infect` | **ADAPT** | Preserve allocation/execution separation, not universal API |
| `retreat` | **ADAPT** | Becomes release/shrink transition family |
| Claim as central abstraction | **ADAPT STRONGLY** | Elastic centers logical resource + admissible state space |
| Topology-aware allocation | **ADOPT / GENERALIZE** | Supports Resource Graph |
| Multi-layer granularity | **ADOPT / GENERALIZE** | Strong design principle |
| User-oriented criteria over utilization | **ADOPT** | Hard constraints precede optimization |
| Temporary under/oversupply | **ADAPT** | Model as explicit policy/soft constraints |
| Rule-based dispatch | **INVESTIGATE** | Useful fast path, insufficient as universal planner |
| Application-triggered resource mechanism | **ADAPT / INVESTIGATE** | Elastic aims to move mechanism choice toward runtime while preserving app authority |

---

## 19. Candidate Elastic contributions exposed by this comparison

These are **research hypotheses**, not novelty claims.

### C1 — Claim-independent resource identity

Represent a resource independently from its current physical claim/allocation, allowing its realization to move across admissible states without changing logical identity.

### C2 — General transition algebra

Treat acquire/release as two transitions among a broader family including migration, representation change, replication, recomputation, routing, and parallelism changes.

### C3 — Separation of semantic contract from resource request

Allow the application to specify what must remain true and what may change, without necessarily prescribing which concrete hardware resources must be claimed.

### C4 — Multi-dimensional elasticity

Represent capacity, residency, locality, representation, parallelism, redundancy, persistence, and other dimensions in one programming model.

### C5 — Planning over admissible state trajectories

Select a legal sequence of transitions rather than a single resource claim, accounting for transition cost and uncertainty.

### C6 — Typed legality in Rust

Use Rust types/capabilities to make some illegal transitions unrepresentable or rejectable at a narrow trusted boundary.

Each candidate contribution requires broader related-work search before any novelty statement.

---

## 20. Experiments suggested by the paper

### Experiment A — Claim API vs intent API

Implement the same adaptive workload with:

1. explicit claim-style resource acquisition;
2. Elastic declarative constraints/objectives.

Measure application resource-management code, runtime overhead, achieved objectives, and portability across hardware configurations.

### Experiment B — Multi-layer granularity

Compare fixed global granularity with distinct planning/allocation/transition/execution granularities.

Measure planner search size, transition overhead, fragmentation, and latency.

### Experiment C — Isolation versus utilization

Create competing workloads with configurable exclusivity/sharing policies.

Measure predictability, p99 latency, throughput, fragmentation, and utilization.

### Experiment D — General transitions beyond claims

Use a workload where resource acquisition alone is insufficient and adaptation requires residency migration, recomputation, or representation change. Compare a claim-centered controller with an Elastic transition planner.

---

## 21. SciRust gap check

No SciRust dependency is implied or desired.

For this paper, no immediate SciRust capability gap is established. When ElasticXxx reaches experimental work on multi-objective constrained planning, interference modeling, or online transition-cost estimation, we must check whether SciRust already provides sufficiently general research tools. Any missing **general scientific capability** may become a SciRust improvement; Elastic-specific runtime mechanisms remain in ElasticXxx.

---

## 22. Bottom line

Invasive Computing is a major conceptual predecessor for ElasticXxx. It already includes runtime resource awareness, dynamic expansion/shrinkage, topology, constraints, isolation, application-level quality requirements, optional sharing, and multi-layer granularity.

Therefore ElasticXxx must **not** claim novelty for dynamic resource acquisition, application-specified resource constraints, runtime resource awareness, or adaptive expansion/shrinkage.

The strongest current Elastic distinction is instead the attempted shift from a **claim-centered execution model** toward a **general constrained state-and-transition model over stable logical resources**, covering resource changes that are not naturally expressed as acquiring or releasing hardware.

Whether this distinction is novel and useful remains an open research question to be tested against broader programming-language, autonomic-computing, malleable-runtime, heterogeneous-memory, control, and adaptive-systems literature.
