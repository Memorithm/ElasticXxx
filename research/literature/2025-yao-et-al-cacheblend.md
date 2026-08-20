# CacheBlend: Fast Large Language Model Serving for RAG with Cached Knowledge Fusion

**Paper:** Jiayi Yao, Hanchen Li, Yuhan Liu, Siddhant Ray, Yihua Cheng, Qizheng Zhang, Kuntai Du, Shan Lu, Junchen Jiang. *CacheBlend: Fast Large Language Model Serving for RAG with Cached Knowledge Fusion*. EuroSys 2025.

**Primary sources:**
- EuroSys/arXiv PDF: https://arxiv.org/pdf/2405.16444
- DOI: https://doi.org/10.1145/3689031.3696098
- code referenced by paper: https://github.com/LMCache/LMCache

**Review status:** mechanism-level review complete; relevant PDF pages were visually inspected.

---

## 1. Problem

**SOURCE-DERIVED.** CacheBlend studies reuse of precomputed KV caches for inputs composed of multiple reusable text chunks, as in RAG.

Prefix caching is exact for the prefix because that prefix's KV does not depend on succeeding text. Naively concatenating independently precomputed KV caches for non-prefix chunks is different: the later chunks were computed without cross-attention to their actual preceding chunks.

Thus a cached KV object can correspond to the same source text and still be semantically stale/incomplete in a new composition context.

---

## 2. Fundamental identity lesson: source identity is not materialization identity

This paper reveals a distinction that the current Elastic KV factorization did not make explicit enough.

For a source chunk `C`, these are not equivalent concepts:

```text
SourceIdentity(C)
MaterializedKV(C | derivation context)
```

The KV representation is a **derived resource**. Its value depends on at least:

```text
source content
model/version
layer
position/positional transform
preceding context / attention dependencies
representation policy
```

Therefore:

```text
same source bytes/text
    ≠
interchangeable derived KV state
```

**ELASTIC PROPOSAL.** Add explicit **derivation provenance / dependency context** for derived resources rather than overloading logical identity.

Conceptually:

```text
DerivedResource {
    source_identity,
    derivation_provenance,
    materialized_state,
}
```

This is not a novelty claim; CacheBlend is direct prior art demonstrating why provenance matters.

---

## 3. Reuse is a compatibility predicate, not a Boolean property

A cache entry should not generically carry:

```text
reusable = true
```

because reuse validity depends on the target context.

A more accurate abstraction is:

```text
ReuseCompatibility(cached_state, target_context)
    -> Exact | Repairable | Invalid
```

For CacheBlend:

- a true prefix cache can be **Exact**;
- a non-prefix independently precomputed chunk is **Repairable** through selective KV recomputation;
- other derived states may be **Invalid** if no admissible repair exists.

The exact enum is an Elastic proposal, not terminology from the paper.

---

## 4. Selective repair instead of full recomputation

**SOURCE-DERIVED.** CacheBlend reuses most precomputed KV values but selectively recomputes a small fraction of tokens on each layer to restore information lost by independent chunk precomputation.

The paper defines KV deviation relative to a fully recomputed reference and shows empirically that recomputing tokens with high KV deviation causes the largest reduction in forward-attention deviation. It calls these **High-KV-Deviation (HKVD)** tokens.

The paper reports that recomputing roughly 10–20% of tokens is often sufficient in its evaluated workloads, with a default around 15% in several system experiments.

**ELASTIC LESSON.** `RECOMPUTE` need not be all-or-nothing. A transition can **repair a subset of a derived resource** while preserving/reusing the remainder.

Possible generic action:

```text
REPAIR_DERIVED_STATE(subset, provenance_target)
```

where the resource adapter defines what repair means and how equivalence/error is verified.

---

## 5. Progressive selection across layers

**SOURCE-DERIVED.** Knowing the true HKVD set would require the fully recomputed KV, defeating the optimization. CacheBlend instead exploits empirical correlation of high-deviation tokens across neighboring layers.

Its gradual-filtering mechanism:

1. recomputes broadly on the first layer;
2. selects candidate HKVD tokens;
3. recomputes only those candidates on the next layer;
4. progressively filters the candidate set over subsequent layers.

This is a useful systems pattern:

```text
expensive exact oracle unavailable
      ↓
obtain broad initial evidence
      ↓
propagate/refine candidate set incrementally
```

**ELASTIC RELATION.** This resembles multistage/adaptive observation where the system buys more information only for candidates that remain plausible.

---

## 6. Repair cost can be hidden behind retrieval

**SOURCE-DERIVED.** CacheBlend pipelines selective recomputation of one layer with fetching the next layer's precomputed KV cache. If loading latency is larger than selective recomputation latency, the repair computation can be hidden from the TTFT critical path.

This lets CacheBlend consider slower/larger storage devices without necessarily increasing end-to-end latency.

**ELASTIC LESSON.** Transition cost cannot always be summed naïvely:

```text
C_total != C_transfer + C_repair
```

when operations overlap.

The cost model may instead require a dependency DAG / critical-path estimate:

```text
TransitionPlanCost = critical_path(operations, dependencies, resource contention)
```

This is a stronger formulation than purely additive transition cost.

---

## 7. Composition correctness

CacheBlend demonstrates that composing individually valid cached artifacts can produce an invalid/inferior combined artifact when hidden dependencies are omitted.

**GENERAL ELASTIC LESSON.** Resource correctness may be **non-compositional** unless dependency/provenance conditions hold.

For derived resources, a validator may need to check:

```text
source versions
transformation versions
upstream dependency identities
context compatibility
ordering/position metadata
semantic contract
```

before authorizing reuse.

This applies beyond KV caches: compiled artifacts, memoized computations, materialized views, distributed checkpoints and derived indexes can have analogous provenance constraints.

---

## 8. Semantic contract

**SOURCE-DERIVED.** CacheBlend's stated goal is to obtain generation quality comparable to full KV recomputation, and the paper reports strong empirical quality preservation in its evaluated datasets.

This does **not** constitute bitwise or formal semantic equivalence.

Therefore in generic Elastic terminology:

- prefix reuse can potentially be treated as exact when the domain establishes equivalence;
- selective fusion/repair requires whatever approximation/equivalence contract the domain validator can actually support;
- empirical quality alone must not silently upgrade an action to `Exact`.

---

## 9. Results

**SOURCE-DERIVED.** The EuroSys 2025 paper reports, in its evaluated models/tasks:

- **2.2–3.3×** lower TTFT than full KV recomputation / prefix-caching comparison points as specified in the paper;
- **2.8–5×** higher inference throughput;
- almost the same TTFT as full modular KV reuse while improving absolute QA F1 by roughly **0.1–0.2** and summarization Rouge-L by roughly **0.03–0.25** in the reported comparisons;
- selective recomputation fractions typically below about 15% in the authors' experience for same-quality responses in their studied cases.

These are empirical, model/task-specific results and must not be generalized to all long-context/RAG workloads.

---

## 10. Relationship to SciRust KV research

SciRust's inspected KV stack already represents:

```text
logical positions
representation plan
basis version / epoch
HOT-WARM-COLD material state
```

but the current repository search did not identify an explicit generic field modelling **derivation provenance such as the actual preceding-context dependency used to create a cached KV artifact**.

This is **not classified as a SciRust gap**. Provenance/compatibility of a concrete cached computational artifact is primarily a runtime/resource-semantics concern. If SciRust later needs generic scientific provenance tooling across independent projects, that should be evaluated separately.

A useful SciRust experiment can nonetheless compare selective repair criteria and representation policies without making ElasticXxx depend on SciRust.

---

## 11. Elastic disposition

| CacheBlend mechanism | ElasticXxx disposition |
|---|---|
| Context-dependent validity of derived KV | **ADOPT / GENERALIZE provenance semantics** |
| Selective KV recomputation | **ADOPT repair/recomputation pattern** |
| HKVD token heuristic | **ADAPT / domain-specific utility model** |
| Progressive filtering across layers | **ADOPT multistage-observation pattern** |
| Pipeline repair with retrieval | **ADOPT / GENERALIZE critical-path cost model** |
| Full reuse without cross-attention | **REJECT when provenance incompatible** |
| Empirical same-quality result | **Evidence, not generic Exact guarantee** |

---

## 12. New Elastic abstractions to investigate

### Derivation provenance

```text
DerivationProvenance {
    source_set,
    dependency_context,
    transform/model/version,
    ordering/position context,
    generation_epoch,
}
```

The exact schema must remain resource-domain specific; the generic core may only need an opaque provenance identity plus a compatibility interface.

### Reuse compatibility

```text
trait ReuseCompatibility<R, C> {
    fn classify(resource: &R, target: &C) -> ReuseClass;
}
```

Potential classes:

```text
Exact
Repairable(RepairPlan)
Invalid
```

### Critical-path transition cost

For overlapping transition operations:

```text
Cost(plan) = makespan / critical path under dependencies and contention
```

rather than a sum of independent action costs.

---

## 13. Experiments suggested

**EXPERIMENT REQUIRED.** For a derived-resource prototype:

1. exact prefix reuse;
2. invalid-context reuse without repair;
3. full recomputation;
4. selective repair at several fractions;
5. selection based on historical/query/provenance-sensitive utility;
6. selective repair + adaptive latent representation;
7. sequential transfer+repair versus pipelined transfer+repair;
8. deliberate stale provenance/version injection to test validator rejection.

Measure semantic/task error, repaired fraction, compute bytes/FLOPs, TTFT, throughput, transition critical path, validator overhead and stale-artifact detection.