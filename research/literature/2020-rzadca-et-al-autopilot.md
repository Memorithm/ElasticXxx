# Autopilot: Workload Autoscaling at Google

**Paper:** Krzysztof Rzadca, Pawel Findeisen, Jacek Swiderski, Przemyslaw Zych, Przemyslaw Broniek, Jarek Kusmierek, Pawel Nowak, Beata Strack, Piotr Witusowski, Steven Hand, John Wilkes. *Autopilot: workload autoscaling at Google*. EuroSys 2020.

**Primary source:** https://john.e-wilkes.com/papers/2020-EuroSys-Autopilot.pdf

**Review status:** mechanism-level review complete.

---

## 1. Problem

**SOURCE-DERIVED.** Autopilot automatically configures both horizontal scale (number of concurrent tasks) and vertical CPU / memory limits for Google workloads. The paper focuses primarily on vertical memory scaling. Its central objective is to reduce resource **slack** while avoiding excessive out-of-memory failures and CPU throttling.

This is not merely an utilization maximizer. It is an explicit risk/cost trade-off between under-provisioning and over-provisioning.

---

## 2. Architecture

**SOURCE-DERIVED.** The published dataflow separates:

```text
resource usage history
        ↓
recommenders
        ↓
Autopilot service
        ↓
actuator
        ↓
Borgmaster / Borglets
```

The recommendation logic and the mechanism that applies task limits or starts/stops tasks are separate system components.

**ELASTIC RELATION — ADOPT / GENERALIZE.** ElasticXxx should distinguish a planner/recommender output from the authority and mechanism that performs an actuation. A recommendation is not itself a transition.

---

## 3. Observations and preprocessing

**SOURCE-DERIVED.** Raw per-task resource measurements are sampled at roughly one-second resolution, then aggregated by the monitoring system into approximately five-minute histograms. Memory is represented using peak usage over a window because under-provisioning memory can terminate a task, whereas CPU under-provisioning usually causes throttling instead.

The system aggregates task-level histograms to a per-job representation because tasks are generally interchangeable replicas and receive the same limits in the common case.

**ELASTIC RELATION.** Observation semantics depend on the failure mode of the resource. Elastic should not assume that all pressure metrics should use the same aggregation function, sampling cadence or summary statistic.

---

## 4. Moving-window controller

**SOURCE-DERIVED.** Autopilot's moving-window recommender deliberately reacts asymmetrically:

- increase limits quickly when usage rises;
- reduce them slowly after usage falls;
- exponentially decay older observations;
- use a 12-hour half-life for CPU and a 48-hour half-life for memory in the configuration described in the paper.

The statistic used also varies by job class. CPU limits can use average or high-percentile load depending on workload class and latency sensitivity. Memory uses different percentiles / maxima according to OOM tolerance.

Raw recommendations are further protected by a safety margin (roughly 10–15% in the described policy) and a one-hour maximum/stabilization rule to reduce fluctuations.

**ELASTIC RELATION — ADOPT PRINCIPLE.** Hysteresis, safety margin and asymmetric up/down dynamics are first-class control concepts, not implementation noise.

---

## 5. ML recommender

**SOURCE-DERIVED.** The ML recommender does not use one opaque universal model. It maintains an ensemble of simple parameterized models and periodically chooses the model that minimizes a cost over historical behavior. Important parameters include decay rate, safety margin and downscaling stabilization behavior.

The optimization cost contains at least:

- overrun cost;
- underrun cost;
- a penalty for changing the resource limit;
- a penalty for changing the chosen model.

Autopilot therefore treats reconfiguration itself as costly.

**ELASTIC RELATION — ADOPT / GENERALIZE.** A generic Elastic objective should not be only `utility(target_state)`. It should include the cost of instability / churn and possibly the cost of changing the planner's own operating regime.

A provisional abstraction is:

```text
DecisionScore =
    ExpectedUsefulProgress
  - UnderProvisionRisk
  - OverProvisionCost
  - TransitionCost
  - ChurnPenalty
  - ModelSwitchPenalty
```

This is an **ELASTIC PROPOSAL**, not Autopilot terminology.

---

## 6. Explainability

**SOURCE-DERIVED.** The paper explicitly treats explainability as operationally important: job owners must be able to understand why a resource limit was chosen. The use of many simple models was intended in part to preserve interpretability.

**ELASTIC RELATION — ADOPT.** Explainability should remain part of the contract of a significant adaptation decision. The planner should be able to report observations, constraints, selected candidate, predicted effect and relevant trade-offs.

---

## 7. User constraints

**SOURCE-DERIVED.** Users can influence behavior through job attributes and optional parameters, including upper and lower bounds on limits. Autopilot also distinguishes latency tolerance and OOM tolerance.

**ELASTIC RELATION.** This is prior art for the principle that an application/user supplies admissible bounds and sensitivity information while the resource manager selects values within those bounds.

ElasticXxx must not claim that this intent/constraint separation is novel merely because it is generalized beyond cloud resource limits.

---

## 8. Transition frequency and disruption budget

**SOURCE-DERIVED.** Autopilot changes resource limits far more frequently than human operators, yet the paper reports that roughly 70% of job-days have no limit change, while the 99th percentile has only around six to seven limit changes per day in the evaluated sample. The authors interpret this as a reasonable disruption cost given the resource savings.

**ELASTIC RELATION — ADOPT.** Transition count / disruption rate is itself an optimization metric. `DO_NOTHING` remains a normal candidate, and a system may maintain an explicit adaptation or disruption budget.

---

## 9. Results

**SOURCE-DERIVED.** The paper reports:

- average relative slack around 23% for ML-managed jobs versus 46% for manually managed soft-limit jobs in the compared fleet samples;
- after migration of a separate job sample to Autopilot, average relative slack fell from 75% in the preceding month to 20% in the following month;
- job-days with at least one OOM in that migration sample fell from 348 before migration to 48 after migration;
- Autopiloted jobs accounted for more than 48% of Google's fleet-wide resource use at the time of writing.

The comparisons are production evidence, but the paper also discusses sampling and workload-composition biases; these values should not be generalized to unrelated systems.

---

## 10. Important conceptual lessons for ElasticXxx

### 10.1 Objective asymmetry

Memory under-provisioning can kill a task; CPU under-provisioning may merely throttle it. Therefore:

```text
same numerical pressure
≠
same semantic / operational risk
```

Elastic resource kinds need resource-specific consequence models behind a common planner interface.

### 10.2 Change penalties are part of utility

Autopilot demonstrates a production reason to penalize limit changes. Elastic should generalize this to transition cost, cooldown, dwell time and disruption budget.

### 10.3 Recommendation is not actuation

A planner computes a desired configuration; an actuator with the required authority performs the system operation. This distinction should remain explicit in Elastic.

### 10.4 Planner explainability matters operationally

The best numerical choice is not enough if operators cannot understand or trust it.

---

## 11. Elastic disposition

| Autopilot mechanism | ElasticXxx disposition |
|---|---|
| Horizontal + vertical resource adaptation | **ADOPT / GENERALIZE** |
| User-supplied upper/lower bounds | **ADOPT** |
| Resource-specific risk semantics | **ADOPT / GENERALIZE** |
| Exponentially weighted history | **INVESTIGATE as one estimator** |
| Asymmetric scale-up / scale-down | **ADOPT principle** |
| Safety margins | **ADOPT / policy-dependent** |
| Stabilization window | **ADOPT / generalize to dwell/hysteresis** |
| Cost of overrun and underrun | **ADOPT / generalize** |
| Penalty for limit changes | **ADOPT / transition-cost model** |
| Ensemble of simple per-job models | **INVESTIGATE** |
| Explainable recommendations | **ADOPT** |
| Borg-specific actuator | **REJECT as core assumption** |

---

## 12. Experiment suggested for ElasticXxx

**EXPERIMENT REQUIRED.** For a simple elastic memory or concurrency resource, compare:

1. threshold-only controller;
2. asymmetric moving-window controller;
3. cost-based controller with explicit transition penalty;
4. predictive planner.

Measure:

- UsefulProgress;
- SLO / invariant violations;
- average reserved slack;
- number of adaptations;
- adaptation cost;
- recovery time after a workload change;
- explainability / decision provenance completeness.

---

## 13. Current conclusion

Autopilot establishes that mature elasticity is a **risk-aware, history-aware and churn-aware optimization problem**, not simply a threshold around utilization. Its separation of recommendation and actuation, explicit penalties for changes, workload-specific sensitivity, and operational explainability are all strong prior mechanisms that ElasticXxx should generalize rather than rediscover.
