# Sinan: ML-Based and QoS-Aware Resource Management for Cloud Microservices

## Status

**SOURCE-DERIVED MECHANISM REVIEW + ELASTIC RELATION.**

Reference: Yanqi Zhang, Weizhe Hua, Zhuangzhuang Zhou, G. Edward Suh, Christina Delimitrou, *Sinan: ML-Based and QoS-Aware Resource Management for Cloud Microservices*, ASPLOS 2021, DOI 10.1145/3445814.3446693.

Primary source: https://www.csl.cornell.edu/~delimitrou/papers/2021.asplos.sinan.pdf

PDF screenshot inspection was attempted for the queue-inertia, two-stage-model and scheduler pages, but the screenshot service returned cache misses. Claims below are grounded in the paper text, not successful visual inspection.

## 1. Problem

Sinan manages CPU resources for interactive applications composed of many dependent microservice tiers while preserving end-to-end tail-latency QoS.

The paper argues that per-tier utilization or queue metrics can be misleading because dependencies and backpressure allow one tier's poor state to be a downstream symptom of a different culprit.

This is an independent confirmation of the rule already seen in Dhalion and StreamOps:

```text
Symptom != RootCause != Action
```

## 2. Queue state is distributed across the stack

The paper notes that queues exist at multiple levels including NIC, OS/network stack, and application. Exact queue-length instrumentation can therefore be expensive, intrusive, or impossible for third-party/public-cloud components.

More importantly, the longest ingress queue is not necessarily the causal bottleneck in a dependent microservice graph.

**Elastic relation — ADOPT:** observation availability and observation cost are themselves part of the control problem. A planner should not assume it can read a perfect scalar queue state for every resource.

## 3. Delayed queueing effect

Sinan emphasizes system inertia.

If allocated capacity falls below input demand, QoS may initially remain satisfied while queues accumulate. Conversely, once a large queue exists, immediately restoring resources does not instantly restore QoS because the backlog must drain.

Conceptually:

```text
bad allocation
    ↓
latent queue growth
    ↓
future QoS violation
```

and:

```text
corrective allocation
    ↓
capacity restored
    ↓
backlog still present
    ↓
delayed QoS recovery
```

**Elastic relation — ADOPT:** `T_transition` is not the complete time-to-benefit. The planner may need:

```text
T_effect = T_transition + T_state_recovery + T_settling
```

where state recovery includes backlog drain, cache warmup, thermal recovery, replication catch-up, etc.

## 4. Different horizons need different prediction targets

Sinan initially considers detailed latency prediction over future timesteps but reports degrading accuracy as the prediction horizon grows.

The final design separates two tasks:

```text
short horizon:
    predict detailed end-to-end latency

longer horizon:
    predict probability of a QoS violation
```

using a CNN for immediate-future latency and Boosted Trees for longer-horizon violation probability.

This yields a strong general lesson:

> the scientifically appropriate prediction target can change with the control horizon.

**Elastic proposal:** a forecast interface should not require every horizon to emit the same value type or precision.

For example:

```text
NearTermEstimate<Latency>
RiskEstimate<QosViolation>
```

can coexist in one planner.

## 5. Resource-space pruning

Sinan does not search arbitrary CPU allocations. The scheduler chooses from predefined CPU adjustment operations and enforces upper utilization limits to avoid unsafe/aggressive downsizing.

This is direct prior art for a **bounded candidate action space**.

It reinforces ElasticXxx's current architecture:

```text
full theoretical state space
        ↓
semantic/capability constraints
        ↓
prevalidated local action neighborhood
        ↓
planner evaluation
```

Do not claim constrained action-space pruning as novel.

## 6. DO NOTHING is explicitly evaluated

The scheduler evaluates maintaining the current allocation as one possible operation.

This independently reinforces:

```text
NoAction
```

as a first-class candidate rather than an absence of a decision.

## 7. Separate safety thresholds

Sinan rejects operations whose predicted tail latency or long-term QoS violation probability are unsafe according to thresholds. Among acceptable operations, it chooses the one using the least resources.

This supports the distinction:

```text
constraint / admissibility
    before
objective optimization
```

rather than folding QoS correctness into one weighted scalar objective.

## 8. Model trust lifecycle and fallback

Sinan includes a safety mechanism for model prediction error.

If a QoS violation is missed, the system immediately scales resources across all tiers. It also tracks prediction errors/missed violations and can reduce its trust in the model, becoming more conservative in future resource reclamation.

**Elastic relation — ADOPT / GENERALIZE:** model trust should be dynamic state, not an eternal property of a trained estimator.

Candidate:

```text
ModelTrustState {
    validation_epoch,
    recent_error,
    miss_rate,
    operating_domain,
    confidence,
    fallback_policy,
}
```

A learned model may influence recommendations but never bypass semantic/safety validation.

## 9. Training distribution is part of model validity

Sinan deliberately explores boundary configurations and QoS-violation regions during data collection. The paper reports serious overfitting/misprediction when the training set contains insufficient QoS-violating examples.

This suggests that model provenance includes more than model weights:

```text
training workload distribution
explored action region
hardware/runtime context
objective/label definition
```

**Elastic inference:** model validity should be checked against an operating-domain witness or provenance before trusting predictions far outside the validated region.

## 10. Explainability as diagnosis aid

Sinan uses model interpretation to identify tiers/resources associated with unpredictable QoS behavior. In the evaluated Social Network case, the model points toward Redis/cache-memory behavior rather than simply the most CPU-intensive tier, which leads the authors to identify periodic Redis persistence/log synchronization as an important source of stalls.

This is evidence that explanatory models can aid diagnosis, but explanation importance is not itself a formal causal proof.

**Elastic relation:** keep `RootCauseEstimate` distinct from semantic invariant or proven cause.

## 11. Evaluation

For the local-cluster Hotel Reservation experiments described in the paper, Sinan and the conservative autoscaler meet QoS throughout, while Sinan reduces CPU usage by 25.9% on average and up to 46.0% relative to that conservative baseline in the reported scenarios.

The paper also reports Boosted-Trees validation accuracy above 94% for predicting a QoS violation over the next five one-second intervals for the two evaluated applications, and CNN inference time within roughly 1% of the one-second decision interval.

These results are specific to the applications, workloads, models, training process and allocation granularity used in the paper; they are not generic Elastic performance expectations.

## 12. Elastic classification

### ADOPT

- dependency-aware end-to-end impact modelling;
- queue/backlog inertia in time-to-benefit;
- horizon-specific prediction targets;
- constrained/predefined action neighborhoods;
- no-action as an evaluated candidate;
- safety/admissibility before resource minimization;
- model trust lifecycle and conservative fallback;
- model-training provenance/operating-domain awareness.

### ADAPT

- CPU-only action set → typed domain-specific actions;
- CNN/Boosted-Trees architecture → pluggable model backends;
- scale-all fallback → resource/domain-specific safe fallback;
- fixed one-second scheduling → multi-rate resource-specific triggering.

### REJECT from generic core

- assumption that ML is required for every resource;
- fixed CPU-step operations;
- treating explainability scores as proof of causality;
- assuming queueing state can always be represented by one observed tier metric.

## 13. Design implications

A candidate generic structure becomes:

```text
ForecastBundle {
    near_term_effect,
    longer_horizon_risk,
    model_trust,
    operating_domain,
}
```

and planner viability should consider:

```text
T_effect =
    T_control
  + T_transition
  + T_recovery
  + T_settling
```

against the expected persistence of the motivating condition.

These are **ELASTIC PROPOSALS**, not Sinan terminology.

## 14. Experiments

**EXPERIMENT REQUIRED.** Construct a three-stage service graph with hidden backlog and a downstream symptom.

Compare:

1. local utilization controller;
2. local queue controller;
3. dependency-aware near-term predictor;
4. near-term predictor + long-horizon risk;
5. the above with explicit model-trust degradation/fallback.

Measure QoS misses, false interventions, resource use, backlog recovery, wrong-root-cause rate, and planner overhead.

## 15. SciRust

No new SciRust gap is justified by Sinan. SciRust already has broad statistics, learning, graph and simulation capabilities. A concrete future experiment may reveal a generic missing primitive, but CNN/XGBoost microservice policy code would be project-specific rather than a scientific core requirement.
