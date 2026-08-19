# Tiered Memory Management Beyond Hotness

**Paper:** Jinshu Liu, Hamid Hadian, Hanchen Xu, Huaicheng Li. *Tiered Memory Management Beyond Hotness*. OSDI 2025.

**Primary source:** USENIX OSDI 2025 paper: https://www.usenix.org/system/files/osdi25-liu.pdf

## 1. Problem

**SOURCE-DERIVED.** The paper challenges the dominant assumption that access frequency ("hotness") is a reliable proxy for performance-criticality. Modern CPUs can hide portions of memory latency using memory-level parallelism (MLP), so frequently accessed pages are not necessarily the pages that most affect end-to-end performance.

## 2. AOL metric

**SOURCE-DERIVED.** The authors introduce **Amortized Offcore Latency (AOL)** to combine offcore access latency and MLP. In their formulation:

```text
Latency = A2 / A3
MLP     = A2 / A1
AOL     = Latency / MLP = A1 / A3
```

where the quantities are derived from hardware counters. They combine AOL with an LLC-stall-pressure term to predict workload slowdown. The paper reports Pearson correlation 0.951 between the resulting model and observed slowdown across its validation set.

## 3. Hotness versus performance contribution

**SOURCE-DERIVED.** In the motivating benchmark, sequential pages are 13.6× hotter than pointer-chasing pages, but placing the hot sequential pages in fast memory produces substantially worse performance than placing the colder latency-sensitive pointer-chasing pages there. The paper reports a 34% performance advantage for the "cold-on-fast-tier" placement over the idealized hotness-driven placement in that benchmark.

**KEY ELASTIC LESSON.** A frequently changing resource signal is not necessarily an optimization objective. `pressure`, `utilization`, `queue length`, `access frequency`, and similar metrics may only be proxies.

## 4. Soar

**SOURCE-DERIVED.** Soar is a profile-guided allocation mechanism that ranks long-lived objects by their estimated contribution to application performance and places higher-ranked objects in the fast tier. It is largely an offline/static allocation mechanism and avoids runtime migration for workloads that fit its assumptions.

**ELASTIC RELATION: ADAPT.** The principle of ranking state/resource choices by expected contribution to useful application progress is highly relevant, but object placement and AOL are memory/CPU-specific.

## 5. Alto

**SOURCE-DERIVED.** Alto is a runtime migration regulator that can be layered over existing tiering systems such as TPP, Nomad, NBT, and Colloid. It measures AOL periodically and changes migration intensity:

```text
AOL <= low threshold   -> disable/limit promotions
AOL >= high threshold  -> full promotions
otherwise              -> partial promotion rate
```

The key idea is that migration should become aggressive only when slow-tier accesses are actually performance-sensitive.

## 6. Results

**SOURCE-DERIVED.** The paper reports up to 12.4× improvement over compared tiering designs across its workloads, with a small number of cases where Soar/Alto underperform by no more than 3%. For Alto specifically, the paper reports reductions in page promotions up to 127.4× relative to TPP while preserving or improving performance for most evaluated cases.

The authors explicitly note reduced gains under high bandwidth contention because queuing delay inflates AOL; threshold tuning under contention is left as future work.

## 7. Relationship to Pollux

**INFERENCE.** This paper reinforces the same systems principle previously observed in Pollux:

```text
raw activity metric != useful progress metric
```

Pollux replaces raw throughput with DL training goodput; Liu et al. replace memory access hotness with a metric more closely connected to performance impact.

**ELASTIC PROPOSAL.** ElasticXxx should therefore distinguish:

- `ObservationMetric` — what can be measured cheaply;
- `ProxyMetric` — a signal correlated with an outcome;
- `ProgressMetric` / `UtilityMetric` — what the planner is actually trying to improve.

A planner should not silently equate these categories.

## 8. Resource decision model

The paper suggests a generic pattern:

```text
observe local activity
      ↓
estimate actual performance sensitivity
      ↓
regulate adaptation intensity
      ↓
avoid transitions whose cost exceeds expected benefit
```

This is more general than page migration and should be investigated as part of ElasticXxx's planning semantics.

## 9. Elastic relation

| Mechanism | ElasticXxx disposition |
|---|---|
| Reject raw hotness as universal objective | **ADOPT principle** |
| Performance-contribution metric | **ADOPT / GENERALIZE** |
| Explicit predictor from hardware counters | **ADAPT** |
| Migration-intensity regulator | **ADOPT / GENERALIZE** |
| Offline threshold calibration | **INVESTIGATE** |
| Memory-specific AOL | **REJECT as universal metric** |
| Layering regulator over existing mechanisms | **ADOPT architectural principle** |

## 10. New design consequence for ElasticXxx

**ELASTIC PROPOSAL.** `ElasticPressure` should not itself be treated as utility. Pressure is an observed condition indicating possible risk or scarcity. A separate `ElasticProgressModel` / `ElasticUtilityModel` should estimate the consequence of candidate transitions.

For example:

```text
VRAM pressure = 94%
```

is not sufficient to conclude:

```text
migrate immediately
```

The planner should additionally ask whether current accesses are performance-critical, whether migration is on the critical path, and whether another action has greater expected benefit.

## 11. SciRust gap check

No confirmed SciRust gap is established by this paper. The AOL model is mathematically simple and can be analyzed with existing numerical/statistical capabilities. A future need for online uncertainty-aware performance modelling may reveal a general SciRust gap, but that should be demonstrated by experiments before being entered as a missing capability.

## 12. Experiments suggested

For Elastic memory management, compare decision policies driven by:

1. raw capacity pressure;
2. access hotness;
3. latency sensitivity / useful-progress estimate;
4. expected net utility = predicted benefit - transition cost.

Measure end-to-end useful progress, migration count, migration bytes, blocked time, fast-tier hit rate, and planner overhead. The experiment should deliberately include cases where hotness and performance-criticality diverge.
