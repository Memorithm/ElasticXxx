# KVCache Cache in the Wild: Characterizing and Optimizing KVCache Cache at a Large Cloud Provider

**Paper:** Jiahao Wang, Jinbo Han, Xingda Wei, Sijie Shen, Dingyan Zhang, Chenguang Fang, Rong Chen, Wenyuan Yu, Haibo Chen. *KVCache Cache in the Wild: Characterizing and Optimizing KVCache Cache at a Large Cloud Provider*. USENIX ATC 2025, pp. 465–482.

**Primary sources:**
- USENIX: https://www.usenix.org/conference/atc25/presentation/wang-jiahao
- arXiv: https://arxiv.org/abs/2506.02634
- sample traces: https://github.com/alibaba-edu/qwen-bailian-usagetraces-anon

**Review status:** mechanism-level review complete. The USENIX/arXiv PDF is larger than the available web-PDF fetch limit, so screenshot retrieval could not be completed; the official USENIX page and arXiv HTML version were used for source-grounded mechanism inspection.

---

## 1. Why this paper matters

**SOURCE-DERIVED.** Most KV-cache systems are evaluated on synthetic or benchmark-generated traffic. This paper characterizes production traces from a large cloud LLM provider and asks whether real reuse behavior supports the assumptions behind cache policies.

The traces include timestamps, request category, user/session relations, token counts and privacy-preserving token hashes, enabling real prefix-reuse analysis rather than synthetic request replay alone.

**ELASTIC IMPORTANCE.** This paper tests whether a policy model remains valid when the workload distribution is not controlled by the experimenter.

---

## 2. Reuse is highly skewed

**SOURCE-DERIVED.** The paper reports substantial but lower-than-synthetic ideal reuse rates in the two studied traces and finds reuse highly skewed. Its summary reports that roughly 10% of KV blocks account for 77% of reuses; the detailed trace analysis also shows a small fraction of users/requests contributing most hits.

It additionally finds that single-turn traffic can be a major source of reuse. In the API-dominated to-B trace, single-turn requests contribute 97% of cache hits despite negligible multi-turn traffic, largely because programs reuse system prompts.

**ELASTIC LESSON.** A cache policy should not hardcode a conversational/multi-turn workload model. `Context` must be able to represent request category and workload class without treating them as properties of the cached KV object itself.

---

## 3. Reuse probability is workload-conditioned

**SOURCE-DERIVED.** Globally, reuse-time and reuse-probability distributions are diverse. When conditioned on a request category (request type plus turn number in the paper's characterization), reuse time becomes substantially more predictable from history. The authors report that exponential distributions fit their category-conditioned reuse probabilities well enough to drive an eviction policy.

This gives a concrete form to contextual utility:

```text
P(reuse in future | request category, age, lifespan model)
```

rather than:

```text
intrinsic_hotness(block)
```

**ELASTIC GENERALIZATION.** Utility models may require a **context class** and a **time-to-event model**:

```text
U(resource | context, age, horizon)
```

and therefore have their own calibration epoch and drift behavior.

---

## 4. Ephemeral lifetime changes architecture choices

**SOURCE-DERIVED.** The paper reports a P99 KV-cache lifetime of approximately 97 seconds in its to-B workload. For that workload, a cache capacity of roughly two times per-GPU HBM was sufficient to approach the ideal infinite-cache hit rate on the model assumptions evaluated.

The authors argue that, in such a workload, a small GPU/host cache can be preferable to the additional complexity of a CPU–RDMA–SSD hierarchy.

**ELASTIC LESSON.** More elastic mechanisms/tiers are not automatically better. A capability should exist because it expands the admissible state space, but the planner must be allowed to conclude that a simpler state space is sufficient for a particular workload.

This is evidence for:

```text
mechanism availability ≠ mechanism desirability
```

and reinforces `DO NOTHING` / `DO NOT PROVISION EXTRA TIER` as first-class decisions.

---

## 5. Why frequency is a poor proxy here

**SOURCE-DERIVED.** The authors explicitly reject frequency as a primary priority signal for their workload-aware policy: a block may have been frequently reused in the past but have a very short remaining lifetime. This makes LFU vulnerable to retaining already-dead blocks.

This reinforces prior lessons from Pollux and tiered-memory work:

```text
utilization ≠ useful progress
hotness ≠ performance criticality
past frequency ≠ future reuse probability
```

The target variable should be as close as practical to the actual future value of retaining the object.

---

## 6. Workload-aware eviction policy

**SOURCE-DERIVED.** The proposed priority is a lexicographic tuple based on:

1. category-conditioned future reuse probability, estimated from recently sampled data and an exponential fit, regulated by expected lifespan;
2. an offset reflecting prefix/spatial locality.

The lowest-priority block is evicted first.

To avoid scanning all cached blocks, the implementation maintains an LRU-ordered priority queue inside each workload category. Only the least-recently-used candidate from each workload needs full cross-workload probability comparison. This reduces policy complexity from `O(N)` blocks to `O(W)` workload categories, where the paper says `W` is typically in the tens.

The reported policy calculation overhead is about 79 μs per eviction and approximately 1.2% of the serving engine's scheduling overhead in the evaluated setup.

**ELASTIC RELATION.** This is another strong example of hierarchical/decomposed planning:

```text
cheap total order inside one context class
        ↓
one candidate per class
        ↓
expensive comparison across classes
```

The structure of the probability model is used to reduce decision cost.

---

## 7. Policy lifecycle and drift

**SOURCE-DERIVED.** The policy fits reuse distributions from recent historical samples. The appendix reports similarities across workdays for several categories, but also notes categories such as image traffic are harder to predict. The paper explicitly limits its conclusions to the collected production period and notes that emerging workloads such as reasoning may differ.

**ELASTIC CONSEQUENCE.** A fitted utility/reuse model needs:

```text
model_version
calibration_window
validity_epoch
prediction_error / drift telemetry
fallback policy
```

This directly reinforces the previously proposed planner-policy lifecycle:

```text
CALIBRATING → VALIDATED → SERVING → DEGRADED/STALE → RECALIBRATING
```

---

## 8. Results

**SOURCE-DERIVED.** The paper reports that the workload-aware policy:

- improves hit rate by about **1.5–3.9 percentage points** over the best compared workload-agnostic baseline depending on capacity/trace;
- yields approximately **28.3–41.9%** QTTFT reduction in the evaluated serving experiments;
- is less beneficial on the API-dominated trace where request categories provide little additional discrimination and cache capacity is already sufficient, causing the method to behave much more like LRU.

The paper summary also describes up to roughly 41.4% mean response-time improvement in its comparison. These results are trace/capacity/model specific.

---

## 9. Important limitation for our research

**SOURCE-DERIVED.** The authors explicitly note that their production traces cover a limited period and that LLM workloads are evolving. Reasoning workloads are named as an example not covered by the characterization.

Therefore this paper should not be used to claim a universal production KV distribution. Its stronger methodological lesson is:

> characterize the workload first, then select/design the policy.

---

## 10. Relation to SciRust's KV work

SciRust's current adaptive KV stack primarily optimizes how a **live per-request KV state** is represented under a memory/quality budget.

This paper studies a distinct outer caching problem:

```text
completed/shared prefix KV object
        ↓
keep for possible future request?
```

The two can compose:

```text
future reuse probability
    ×
resident representation size/quality
    ×
recomputation cost
```

A future experiment can ask whether a block with low reuse probability should be:

```text
fully evicted
or
kept in a much cheaper representation
```

rather than forcing eviction policy and representation policy to be separate.

---

## 11. SciRust gap check

Current `scirust-stats` already exposes an `Exponential` distribution with PDF/CDF/survival function and the usual statistical primitives, so the parametric model used by this paper does not reveal a missing basic probability distribution.

Repository search did not identify a general **censored time-to-event / survival-analysis** module (Kaplan–Meier, hazard modelling, etc.).

**SCIRUST-GAP-STATS — INVESTIGATE:** nonparametric/semiparametric survival analysis.

This is not required by the paper's implementation and is therefore not being added immediately. It is scientifically general for reliability, predictive maintenance, medical/event-time studies and reuse/lifetime modelling, so it is a legitimate future SciRust investigation if independent projects require censored time-to-event inference.

---

## 12. Elastic disposition

| Mechanism/finding | ElasticXxx disposition |
|---|---|
| Workload-conditioned reuse model | **ADOPT / GENERALIZE contextual utility** |
| Historical probability fitting | **ADOPT pattern / model-specific** |
| Lifespan-aware priority | **ADOPT / GENERALIZE horizon-aware utility** |
| Spatial/prefix-locality offset | **ADAPT / domain-specific feature** |
| No frequency term | **ADOPT lesson: proxy must be validated** |
| Per-category queues + cross-category comparison | **ADOPT decomposition pattern** |
| Workload-aware policy lifecycle | **ADOPT / GENERALIZE** |
| Small cache sufficient in one workload | **ADOPT lesson: optional mechanisms are not mandatory** |
| Exponential fit | **Domain-specific model choice** |

---

## 13. Experiments suggested

**EXPERIMENT REQUIRED.** Using released/sample or generated traces:

1. LRU / LFU / FIFO;
2. category-conditioned reuse probability;
3. representation-aware expected value (`reuse_probability × recompute_saved / bytes`);
4. workload-conditioned representation + eviction jointly;
5. policy with stale distributions versus online recalibration;
6. one-tier versus multi-tier cache under short- and long-lifetime workloads.

Measure hit rate, TTFT/QTTFT, bytes retained, model fitting overhead, policy overhead, prediction calibration, drift sensitivity, and whether additional physical tiers have positive net value.