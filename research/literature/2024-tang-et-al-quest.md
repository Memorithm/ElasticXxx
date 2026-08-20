# Quest: Query-Aware Sparsity for Efficient Long-Context LLM Inference

**Paper:** Jiaming Tang, Yilong Zhao, Kan Zhu, Guangxuan Xiao, Baris Kasikci, Song Han. *QUEST: Query-Aware Sparsity for Efficient Long-Context LLM Inference*. ICML 2024.

**Primary sources:**
- PMLR: https://proceedings.mlr.press/v235/tang24l.html
- arXiv: https://arxiv.org/abs/2406.10774

**Review status:** mechanism-level review complete.

---

## 1. Problem

**SOURCE-DERIVED.** Quest targets the memory-movement cost of long-context self-attention. Its key observation is that the KV entries critical to one query can be different from those critical to another query. Therefore a query-agnostic permanent notion of token importance is insufficient.

Quest dynamically selects only the KV pages estimated to be critical for the **current query**.

---

## 2. Page-level resource model

**SOURCE-DERIVED.** Quest builds on paged KV-cache management. Each page maintains compact metadata: per-dimension minima and maxima of the key vectors in that page.

At attention time, Quest combines the current query with this metadata to compute an upper-bound-like page criticality score, ranks pages, chooses the top-K pages, and performs normal attention only over those selected pages.

The selection granularity is therefore a page rather than an individual token.

---

## 3. Query-conditioned criticality

This paper provides strong evidence against representing importance as an intrinsic persistent property:

```text
importance(token)  // insufficient
```

The more appropriate abstraction is:

```text
utility(token | current query, layer, model state, ...)
```

**ELASTIC PROPOSAL.** Keep contextual utility outside the intrinsic resource state:

```text
ResourceState      = what the resource currently is
Context            = current workload / query / environment
UtilityEstimate    = predicted value of a resource/subset in that context
```

A resource may be unchanged while its estimated utility changes dramatically.

---

## 4. Cheap summary instead of full observation

**SOURCE-DERIVED.** Quest does not load the entire KV cache to determine what should be loaded. It maintains page summaries incrementally on insertion and reads only the min/max metadata to estimate page criticality.

The paper models the additional data read for criticality estimation as roughly proportional to two vectors per page, after which only the selected top-K pages are fetched for normal attention.

This is a strong instance of a general systems principle:

> **Do not require full observation of an expensive resource in order to decide whether it is worth observing/using fully.**

**ELASTIC PROPOSAL.** Investigate a generic concept such as:

```text
ElasticSummary<R>
```

or

```text
SelectionMetadata<R>
```

with properties such as:

```text
update_cost
read_cost
validity_epoch
error / bound semantics
covered resource generation
```

The summary is not the resource itself and must have its own consistency semantics.

---

## 5. Bounded score semantics

**SOURCE-DERIVED.** Quest's page metadata is not merely a heuristic feature vector. The min/max key values are combined with the query to estimate an upper bound on the page's possible attention contribution before loading every key in the page.

**ELASTIC LESSON.** Observations can have different epistemic meanings:

```text
exact value
estimate
lower bound
upper bound
confidence interval
```

This should not be collapsed into one untyped scalar telemetry channel.

A future `ElasticObservation`/summary API should record what kind of statement the observation represents.

---

## 6. Approximate selection and semantics

Quest performs full attention only over selected pages, so it is an approximate method relative to dense attention unless the omitted pages can be proven irrelevant.

The paper reports strong accuracy preservation in its evaluated long-context tasks, but this remains empirical/model-specific evidence.

**ELASTIC CONSEQUENCE.** Query-aware sparse attention requires an approximate semantic contract unless a stronger domain-specific equivalence proof is available.

---

## 7. Results

**SOURCE-DERIVED.** Quest reports up to 7.03× reduction in self-attention latency and 2.23× end-to-end inference speedup compared with the FlashInfer baseline in the evaluated setup. The paper also reports close-to-full-cache performance on long-dependency retrieval tasks with substantially smaller token budgets in the tested models.

These are configuration-specific results and should not be interpreted as universal speedups.

---

## 8. Relationship to H2O

H2O and Quest expose different forms of utility estimation:

```text
H2O:
    historical accumulated attention
    → persistent-ish online heavy-hitter estimate

Quest:
    current query × page summary
    → per-query criticality estimate
```

This suggests that a generic resource framework should not standardize one universal notion of "hotness" or "importance".

Instead:

```text
UtilityModel<R, Context>
```

should be pluggable and explicit.

---

## 9. Relationship to SciRust KV research

SciRust already adapts **representation** under budget:

```text
rank / residuals / FP32-INT8-INT4 / HOT-WARM-COLD representation
```

Quest adapts **which physical/logical pages participate in the next attention computation**.

A meaningful combined experiment is therefore:

```text
query-aware selected subset
        ×
adaptive representation per subset/tier
```

For example, currently critical pages might use a higher-fidelity representation while low-criticality pages remain compressed or non-resident.

This is an experiment proposal, not a claim that such a combination is superior.

---

## 10. SciRust gap check

No new SciRust gap is established by Quest alone.

Its min/max page summary and query-conditioned upper-bound score are attention-specific mechanisms. They should not be copied into SciRust as a generic feature merely because ElasticXxx studies them.

The paper does, however, motivate future investigation of **generic bounded summaries / sketches / uncertainty-bearing observations** if multiple independent scientific projects require such primitives.

---

## 11. Elastic disposition

| Quest mechanism | ElasticXxx disposition |
|---|---|
| Query-conditioned utility | **ADOPT / GENERALIZE** |
| Page-granularity selection | **ADOPT granularity principle / domain-specific mechanism** |
| Compact maintained summaries | **ADOPT / GENERALIZE** |
| Upper-bound score semantics | **ADOPT observation-semantics lesson** |
| Top-K selector | **ADAPT / planner backend choice** |
| Sparse approximate attention | **ADAPT under semantic contract** |
| PagedAttention dependency | **Domain-specific** |

---

## 12. New conceptual rule

> **Importance is generally not a resource state; it is an estimate of contextual utility.**

For ElasticXxx, this avoids confusing:

```text
what the resource is
```

with:

```text
how useful the resource is right now
```

and makes context changes possible without fabricating resource-state transitions.

---

## 13. Experiment

**EXPERIMENT REQUIRED.** Compare for the same trace:

1. static importance;
2. history-based importance;
3. query-conditioned importance;
4. exact small-instance utility oracle;
5. query-conditioned selection + static representation;
6. query-conditioned selection + adaptive SciRust-style representation.

Measure prediction/selection recall, semantic error, bytes read, summary-maintenance overhead, planner overhead, and end-to-end useful progress.