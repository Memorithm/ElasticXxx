# DiffKV: Differentiated Memory Management for Large Language Models with Parallel KV Compaction

**Paper:** Yanqi Zhang, Yuwei Hu, Runyuan Zhao, John C.S. Lui, Haibo Chen. *DiffKV: Differentiated Memory Management for Large Language Models with Parallel KV Compaction*. SOSP 2025, pp. 431–445.

**Primary sources:**
- arXiv v3: https://arxiv.org/abs/2412.03131
- SOSP 2025 accepted papers: https://sigops.org/s/conferences/sosp/2025/accepted.html
- code: https://github.com/zyqCSL/DiffKV

**Bibliographic note.** arXiv `2412.03131` was first submitted in December 2024 and revised as DiffKV in 2025. Some older search/index metadata may still expose the earlier name *LeanKV*. This note uses the current paper title and the SOSP 2025 publication record.

**Review status:** mechanism-level review complete. Web PDF screenshot requests were attempted during review but the arXiv screenshot backend returned a cache-miss error; textual PDF extraction and the SOSP publication record were available.

---

## 1. Problem

**SOURCE-DERIVED.** DiffKV argues that uniform KV-cache quantization or pruning misses three forms of heterogeneity:

1. keys and values have different effects on attention;
2. tokens have different importance;
3. attention sparsity varies dynamically across heads and requests.

The system therefore jointly adapts **which tokens survive, at what K/V precision, and how much memory each head/request receives**.

This makes DiffKV a substantially closer prior-art comparator to SciRust's adaptive KV research than systems concerned mainly with paging/offload/physical placement.

---

## 2. Differentiated K/V representation

**SOURCE-DERIVED.** DiffKV stores keys at higher precision than values. Its main evaluated two-level policy uses:

```text
important token:        K8V4
moderately important:   K4V2
least important:        pruned
```

The paper reports that K8V4 is close to the FP16 baseline in its calibration and that K4V2 is used for lower-significance tokens because it gives a more useful efficiency/quality trade-off than tested alternatives.

**ELASTIC RELATION: ADOPT / GENERALIZE.** `Representation` is not necessarily a scalar precision field. A representation can be channel-structured:

```text
Representation {
    key: ...,
    value: ...,
}
```

and different logical subresources can have different representations simultaneously.

---

## 3. Importance-conditioned representation and pruning

**SOURCE-DERIVED.** DiffKV uses attention-derived significance scores. In the prompt phase it classifies tokens with sequence-length-dependent thresholds into high precision, low precision, or pruned. The most recent window is protected from premature compression.

During generation, a newly aging token can enter the high- or low-precision cache or be pruned. The least-significant token in the affected precision region can then be downgraded again:

```text
high precision
    ↓
low precision
    ↓
pruned
```

when its significance crosses the corresponding thresholds.

This is important prior art for a **smooth degradation path** rather than direct full-precision→evicted transitions.

**ELASTIC CONSEQUENCE.** Representation transitions can form an ordered lattice/graph of degradation states, but an application-independent runtime must not assume that this order is semantically admissible. The semantic contract and resource adapter define the legal path.

---

## 4. Dynamic per-head and per-request allocation

**SOURCE-DERIVED.** DiffKV defines critical tokens using a target fraction of accumulated attention score and observes that the number of critical tokens varies substantially across layers, KV heads and requests. Instead of assigning a fixed equal budget to every head, it dynamically lets each head/request consume memory according to its observed sparsity.

The compression policy is executed per request and per head, while the main threshold parameters are calibrated offline per model rather than independently per head in the evaluated implementation.

**ELASTIC LESSON.** Adaptation granularity is itself multidimensional:

```text
request × layer × head × token × K/V channel
```

A resource model should identify the granularity at which a policy may vary independently. It should not confuse this with allocation granularity or execution granularity.

---

## 5. Irregular representation creates a physical-memory problem

**SOURCE-DERIVED.** Once different heads contain different counts of high- and low-precision tokens, memory consumption becomes irregular. DiffKV argues that conventional PagedAttention-style uniform pages are insufficient because a fixed high-precision format wastes space for lower-precision tokens.

Its on-GPU manager therefore introduces:

- **unified pages**, dynamically interpreted according to K/V precision;
- a GPU-resident circular free-page list;
- a bidirectional page table;
- **parallel KV compaction** to coordinate per-head memory requirements efficiently.

The paper characterizes the management work as scaling with `#requests × #heads` rather than only `#requests`, and moves planning/coordination onto the GPU to prevent memory-management overhead from dominating generation.

**ELASTIC LESSON.** Informational adaptation can create **physical fragmentation and metadata costs**. A representation planner cannot optimize compressed bytes alone; it must account for the allocator/page-manager consequences of heterogeneous representations.

---

## 6. Planning versus coordination

**SOURCE-DERIVED.** DiffKV decomposes KV compaction into:

1. a planning phase where each head determines its memory requirement independently;
2. a coordination phase that maps the resulting heterogeneous requirements to physical memory.

The planning work is naturally parallel; coordination is made parallel using prefix-sum-style mechanisms and a contiguous free-page region.

**ELASTIC RELATION: ADOPT / GENERALIZE.** This reinforces the distinction:

```text
local requirement inference
        ↓
global resource reconciliation
        ↓
physical allocation
```

which fits the emerging multiscale planner architecture better than one centralized per-object solver.

---

## 7. Semantic effects

The K8V4/K4V2/prune hierarchy is approximate relative to FP16 dense KV-cache attention.

**SOURCE-DERIVED.** DiffKV calibrates thresholds offline and evaluates task accuracy across general, coding, mathematical and long-context benchmarks, including reasoning-oriented models. The paper reports near-FP16 accuracy in its chosen configurations while using materially less KV memory.

**ELASTIC CONSEQUENCE.** The system is strong evidence that differentiated lossy representation can work empirically, but it does not make such transitions valid under a generic `Exact` semantic contract. ElasticXxx must keep permission to approximate separate from resource pressure or performance benefit.

---

## 8. Results

**SOURCE-DERIVED.** The SOSP 2025 paper reports:

- KV-cache compression of approximately **2.7×–5.7×** in the evaluated configurations;
- throughput improvement of approximately **1.9×–5.4×**;
- on QwQ-32B, a reported **5.4×** throughput improvement over vLLM in the specified experiment;
- memory-management overhead below about **0.2%** of prompt latency and below **0.9%** of generation latency in the reported breakdown;
- up to roughly three orders of magnitude lower memory-management latency than the paper's CPU multi-threaded alternative for the tested compaction workload.

These are paper-specific implementation and workload results, not generic guarantees for differentiated compression.

---

## 9. Comparison with SciRust's current KV research

This paper corrects an over-broad earlier contrast.

### Mechanisms clearly overlapping at a high level

Both DiffKV and SciRust's adaptive KV work explore some combination of:

```text
K/V differentiation
representation adaptation
bounded memory
heterogeneous representation states
runtime policy
```

Therefore **"SciRust adapts representation while prior systems only adapt placement" is false** after considering DiffKV.

### DiffKV mechanisms not currently demonstrated by SciRust's inspected stack

DiffKV contributes:

- significance-conditioned per-token high/low precision/pruning;
- dynamic per-request/per-head sparsity-driven memory allocation;
- GPU-resident unified pages and bidirectional page table;
- parallel on-GPU KV compaction;
- custom attention/runtime integration evaluated at serving scale.

### SciRust mechanisms not represented by DiffKV's core design

Current SciRust repository evidence includes:

- independent K/V **latent ranks**;
- independent K/V sparse residual slot counts;
- independent coefficient/residual formats (`FP32/INT8/INT4`);
- exhaustive deterministic plan selection under a strict persistent-memory budget;
- confirmation hysteresis before plan changes;
- material HOT/WARM/COLD **rank/format/residual recompression**;
- deterministic online basis learning with immutable committed versions and epoch-scoped handoff.

These are different mechanism choices, not evidence of superiority.

### Fair research question

A meaningful experiment is not "which project has more features?" but:

> For a fixed memory/latency/semantic-error budget, when is importance-conditioned precision/pruning preferable to latent-rank/residual adaptation, and can their state spaces be composed without excessive planner and memory-management overhead?

---

## 10. New general Elastic lesson: representation granularity

**ELASTIC PROPOSAL.** Add explicit representation-policy granularity:

```text
RepresentationGranularity =
    Global
  | Resource
  | Request
  | Layer
  | Head
  | TokenRange
  | Token
  | Channel
  | Composition(...)
```

This is conceptual rather than necessarily a literal Rust enum. DiffKV shows that the useful granularity can be a composition such as:

```text
request × head × token × {K,V}
```

and that finer granularity directly increases metadata/allocation complexity.

Thus granularity itself participates in the cost model:

```text
Benefit(finer policy)
    - metadata cost
    - planning cost
    - allocator fragmentation
    - synchronization/coordination cost
```

---

## 11. SciRust gap check

**No new SciRust gap is established.**

The on-GPU page manager, compaction and vLLM integration are systems mechanisms rather than missing general scientific primitives.

The paper's threshold calibration, importance scoring and per-head allocation can be experimentally studied with existing statistics/optimization tooling plus the recently added general subset-selection primitives.

DiffKV does reinforce the need for rigorous multiobjective experimental comparison of competing representation policies, but no new general-purpose SciRust component is justified solely by this paper.

---

## 12. Elastic disposition

| DiffKV mechanism | ElasticXxx disposition |
|---|---|
| Differentiated K/V precision | **ADOPT / GENERALIZE representation structure** |
| Importance→high/low/pruned hierarchy | **ADOPT pattern / ADAPT under semantic contract** |
| Smooth downgrade path | **ADOPT transition-pattern prior art** |
| Per-head/request dynamic allocation | **ADOPT granularity lesson** |
| Offline threshold calibration | **ADAPT / planner-model choice** |
| Unified precision-aware pages | **Domain-specific mechanism / INVESTIGATE for KV prototype** |
| On-GPU parallel compaction | **Domain-specific fast-path mechanism** |
| Approximate pruning | **ADAPT; forbidden under Exact unless equivalence proven** |
| Local planning + global coordination | **ADOPT / GENERALIZE** |

---

## 13. Experiments suggested

**EXPERIMENT REQUIRED.** Compare on the same models/traces and matched memory budgets:

1. DiffKV-style high/low/pruned representation;
2. SciRust latent-rank/residual representation;
3. mixed precision without pruning;
4. latent adaptation without selection;
5. joint importance-based selection × latent representation;
6. fixed per-head budgets versus dynamic per-head budgets;
7. CPU planner versus GPU/local fast-path requirement computation where feasible.

Measure semantic/task error, KV bytes, fragmentation, metadata overhead, planner time, allocator time, attention-kernel latency, throughput, TTFT/TBT, transition churn and sensitivity to workload changes.