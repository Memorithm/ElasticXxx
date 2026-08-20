# H2O: Heavy-Hitter Oracle for Efficient Generative Inference of Large Language Models

**Paper:** Zhenyu Zhang, Ying Sheng, Tianyi Zhou, Tianlong Chen, Lianmin Zheng, Ruisi Cai, Zhao Song, Yuandong Tian, Christopher Ré, Clark Barrett, Zhangyang Wang, Beidi Chen. NeurIPS 2023.

**Primary source:** https://proceedings.neurips.cc/paper_files/paper/2023/file/6ceefa7b15572587b78ecfcebb2827f8-Paper-Conference.pdf

**Review status:** mechanism-level review complete.

---

## 1. Problem

**SOURCE-DERIVED.** H2O addresses the growing KV-cache footprint during autoregressive generation by retaining only a bounded subset of past KV entries while preserving model quality.

The central empirical observation is that accumulated attention scores are highly skewed: a relatively small set of tokens receives a large share of attention over time. H2O calls these tokens **heavy hitters (H2)** and combines them with recent tokens in a bounded cache.

---

## 2. Resource model

The primary state is a subset of token-associated KV entries:

```text
S_i ⊆ {tokens observed up to step i}
|S_i| = k
```

where `k` is the cache budget.

**KEY ELASTIC LESSON.** This is not merely a scalar capacity state. The resource state is a **selected set** whose composition changes over time.

---

## 3. Transition neighborhood

**SOURCE-DERIVED.** H2O constrains consecutive states so that at most one newly added element changes the retained set at each decoding step. Informally:

```text
|S_i \ S_{i-1}| ≤ 1
```

and the cache remains at fixed size once full.

The algorithm considers the previous set plus the new token and evicts one element according to its score rule.

**ELASTIC PROPOSAL.** Introduce the notion of a **transition neighborhood**:

```text
N(s) = { s' | s → s' is a legal cheap next transition }
```

A planner need not search the entire admissible space at every step. High-frequency controllers can search only a local legal neighborhood.

This is consistent with the multiscale fast-path architecture derived from work stealing.

---

## 4. Non-additive utility

**SOURCE-DERIVED.** H2O formulates its online KV eviction problem as a variant of **dynamic submodular maximization**. The paper assumes diminishing marginal returns with respect to the retained set and derives a greedy policy from local accumulated attention statistics.

The theoretical section gives an informal guarantee of the form

```text
f(S_i) ≥ (1 - α)(1 - 1/e) OPT_k - β
```

under the paper's assumptions and parameters.

**ELASTIC LESSON.** A generic planner must not assume:

```text
Utility(S) = Σ utility(item)
```

because item interactions may matter. Possible objective structures include:

- additive;
- submodular / diminishing returns;
- supermodular / complementary;
- arbitrary black-box / learned utility.

This should be a property of the planning problem, not hardcoded into `ElasticResource`.

---

## 5. Observation and online score

**SOURCE-DERIVED.** H2O avoids knowledge of future requests by maintaining local online statistics: accumulated attention scores over already observed decoding steps. The paper reports that this local statistic performs similarly to a global statistic that uses future information in its studied setting.

The resulting controller is cheap enough to apply per decoding step.

**ELASTIC RELATION.** This is a concrete instance of a **fast local policy driven by incremental summaries**, rather than a global planner.

---

## 6. Semantic effect

Eviction is not semantically exact in the strict numerical sense because future attention no longer has access to evicted K/V entries. H2O's contribution is to select entries that empirically preserve task/model quality under a much smaller cache.

**ELASTIC CONSEQUENCE.** H2O-like eviction belongs under an approximate semantic contract unless exact equivalence can be proven for a particular workload/model.

An `Exact` contract cannot silently admit such pruning merely because empirical average quality is high.

---

## 7. Results

**SOURCE-DERIVED.** The paper reports that with a 20% KV-cache budget H2O can retain performance close to the full cache across its evaluated tasks, while improving throughput over FlexGen by up to 3× and over DeepSpeed Zero-Inference / Hugging Face Accelerate by up to 29× in the specified OPT-6.7B/30B experiments. With matched batch size, the paper reports up to 1.9× lower latency than FlexGen.

These results depend on model, tasks, cache budget and implementation and should not be generalized beyond the reported evaluation.

---

## 8. Relationship to SciRust's KV stack

SciRust already has stronger representation adaptation than H2O in a different dimension:

```text
rank / residuals / precision / HOT-WARM-COLD representation
```

H2O contributes a complementary dimension:

```text
which logical KV entries remain represented at all
```

A combined experiment can therefore separate:

```text
selection      = which items?
representation = in what form?
residency       = where?
```

rather than collapsing all three into one cache policy.

---

## 9. SciRust enrichment triggered by this review

**SCIRUST-GAP-OPT — RESOLVED AT BASIC LEVEL.** Repository inspection found no general submodular-optimization primitive. The paper revealed a scientifically general need independent of LLMs.

SciRust was therefore enriched with:

```text
scirust-solvers/src/combinatorial/submodular.rs
```

providing deterministic greedy monotone-submodular maximization under a cardinality constraint.

The implementation exposes the classical `(1 - 1/e)` guarantee **conditionally**: the caller is responsible for supplying exact marginal gains for a normalized, monotone, submodular objective. SciRust does not pretend to infer/prove those black-box properties from samples.

This primitive is general enough for sensor placement, summarization, experimental design, feature selection, caching and other scientific subset-selection problems.

**OPEN QUESTION.** Dynamic submodular optimization, non-monotone variants, matroid/knapsack constraints and streaming algorithms are not automatically justified by this one paper. Add them only when independent research needs appear.

---

## 10. Elastic disposition

| H2O mechanism | ElasticXxx disposition |
|---|---|
| Bounded selected-set resource state | **ADOPT / GENERALIZE** |
| Incremental online importance statistic | **ADOPT pattern / domain-specific score** |
| Heavy-hitter + recency heuristic | **ADAPT** |
| Dynamic submodular formulation | **ADOPT as objective-structure example** |
| Local one-swap transition neighborhood | **ADOPT / GENERALIZE** |
| Approximate eviction | **ADAPT under semantic contract** |
| Greedy low-cost fast path | **ADOPT principle** |

---

## 11. New Elastic abstraction candidate

**ELASTIC PROPOSAL.** Planner problems may expose a structural objective class:

```text
ObjectiveStructure =
    Additive
  | MonotoneSubmodular
  | Other
```

This should guide planner-backend selection without changing resource semantics.

Similarly:

```text
TransitionNeighborhood<R>
```

can expose legal cheap local successors separately from the full `ElasticSpace<R>`.

---

## 12. Experiment

**EXPERIMENT REQUIRED.** On small KV traces where exhaustive search is possible, compare:

1. exact best subset;
2. SciRust additive budgeted-selection oracle when utility is forced additive;
3. SciRust monotone-submodular greedy baseline;
4. H2O heavy-hitter policy;
5. representation adaptation alone;
6. joint selection × representation.

Measure utility/quality, memory, per-step policy overhead, transition churn, and regret relative to the exact small-instance oracle.