# Sage: Practical & Scalable ML-Driven Performance Debugging in Microservices

## Status

**SOURCE-DERIVED MECHANISM REVIEW + ELASTIC RELATION.**

Reference: Yu Gan, Mingyu Liang, Sundar Dev, David Lo, Christina Delimitrou, *Sage: Practical & Scalable ML-Driven Performance Debugging in Microservices*, ASPLOS 2021.

Primary source: https://www.csl.cornell.edu/~delimitrou/papers/2021.asplos.sage.pdf

PDF visual inspection was attempted. The counterfactual page returned a cache miss, while the Sage system-design page rendered successfully and was inspected. Claims below are grounded in the paper text and, where relevant, that successful system-design figure inspection.

## 1. Problem

Sage diagnoses the root cause of end-to-end QoS violations in applications composed of dependent microservices. Its central challenge is that local abnormality and causal responsibility are not the same thing: a tier can exhibit high utilization or queueing because an upstream/downstream dependency is the true source of the problem.

This independently reinforces:

```text
Symptom != RootCause != Action
```

## 2. Dependency structure

Sage constructs a Causal Bayesian Network (CBN) from RPC-level traces and the microservice dependency topology. The model includes service, network, latency, and resource-related variables and represents propagation from backend services toward frontend/end-to-end latency.

**Elastic relation — ADOPT the structural principle:** root-cause inference should use dependency/causal structure where justified rather than treating observations as an unordered metric vector.

The exact CBN formalism is not a generic Elastic requirement.

## 3. Counterfactual diagnosis

Sage's distinguishing mechanism is counterfactual diagnosis.

Conceptually it asks:

```text
Observed world:
    QoS violated

Counterfactual intervention:
    set service/resource X to values associated with healthy operation

Question:
    would end-to-end QoS now be restored?
```

The paper generates counterfactual latency distributions and applies "but-for" tests. If changing a suspected service/resource to a healthy counterfactual state raises the probability of meeting QoS beyond a threshold, Sage treats the intervened variables as causal for the violation under its model.

This is significantly stronger than:

```text
metric is correlated with latency
```

or:

```text
feature has high importance
```

but it remains conditional on the correctness/validity of Sage's learned causal/generative model and observed operating domain.

## 4. Two-level root-cause localization

For scalability, Sage first searches at service level, then searches resources inside the culprit service.

```text
end-to-end violation
      ↓
service-level counterfactuals
      ↓
culprit service(s)
      ↓
resource-level counterfactuals
      ↓
culprit resource(s)
```

The paper also handles cases where multiple microservices jointly contribute by exploring combinations iteratively.

**Elastic relation — ADOPT / GENERALIZE:** diagnosis can itself be hierarchical and budgeted. A control plane need not run the most detailed diagnostic over every resource immediately.

## 5. Root cause should not be a bare enum

A generic runtime should not represent diagnosis only as:

```text
RootCause::Cpu
RootCause::Memory
RootCause::Network
```

Candidate Elastic structure:

```text
RootCauseEstimate {
    hypothesis,
    affected_resources,
    evidence,
    model_version,
    assumptions,
    operating_domain,
    confidence,
    counterfactual_effect?,
    alternatives,
}
```

This is an **ELASTIC PROPOSAL**.

A counterfactual effect can express something like:

```text
P(QoS healthy | do(candidate := healthy_state))
```

when the diagnostic backend can justify such a quantity.

The generic core must not pretend every diagnosis backend is causal.

## 6. Diagnosis levels

A useful generic distinction is:

```text
Association
Prediction
InterventionEstimate
CounterfactualEstimate
ExperimentallyValidatedCause
```

These levels should not be conflated.

A metric correlation or predictive feature importance may justify investigation; it does not become an intervention guarantee merely because a planner would like to act on it.

## 7. Diagnosis as an intervention-selection aid

Sage connects diagnosed root causes to an actuator that adjusts the implicated service's resources.

For ElasticXxx, diagnosis should instead narrow or rank candidate interventions, after which normal validation still applies:

```text
RootCauseEstimate
      ↓
Candidate intervention set
      ↓
semantic/capability validation
      ↓
cost/risk/timescale evaluation
      ↓
ControlOutcome
```

A causal diagnosis never grants authority to violate a semantic contract.

## 8. Observation and tracing cost

Sage uses Jaeger/Prometheus-style tracing and metrics. The paper emphasizes robustness to lower tracing frequencies compared with techniques that require per-request/high-frequency labeled traces.

**Elastic relation:** diagnosis has an observation/inference cost and may operate at a slower cadence than local fast-path control.

This reinforces multi-rate control:

```text
cheap local observations
      ↓
fast controller

expensive dependency/causal diagnosis
      ↓
slower supervisory controller
```

## 9. Model lifecycle under topology changes

Sage discusses adapting when an application's design/deployment changes, including partial/incremental retraining rather than blindly treating the old learned structure as permanently valid.

This reinforces the `ModelTrustState` and operating-domain/version concepts already introduced from Sinan.

A changed topology can invalidate a causal model even if the model artifact bytes themselves have not changed.

## 10. Limitations

The paper explicitly notes an important limitation: data-driven methods cannot identify a source of performance trouble if they have never observed a similar situation; Sage can flag the problematic job/region but may not determine a new underlying cause. Its main scope is deployment/configuration/resource-related issues, and non-resource software bugs may require developer intervention.

**Elastic relation — ADOPT:**

```text
Escalate(diagnosis)
```

is a valid control outcome when available evidence does not support a safe automated repair.

## 11. Elastic classification

### ADOPT

- dependency-aware diagnosis;
- counterfactual/interventional reasoning as stronger evidence than correlation;
- hierarchical diagnosis to control cost;
- explicit model/operating-domain validity;
- multiple joint causes as possible;
- escalation when no supported automated diagnosis/action exists.

### ADAPT

- Sage-specific CBN/GVAE → pluggable diagnostic backends;
- fixed QoS-restoration threshold → domain-specific evidence policy;
- service/resource hierarchy → arbitrary typed resource/dependency graph;
- direct diagnosed-resource actuation → normal Elastic candidate/validation pipeline.

### REJECT from generic core

- assumption that every root-cause backend is causal;
- treating counterfactual output from an unvalidated model as proof;
- hardcoding microservice/RPC semantics;
- requiring ML for diagnosis.

## 12. Experiment

**EXPERIMENT REQUIRED.** Construct a resource graph where two nodes become highly utilized but only one is causally limiting useful progress.

Compare:

1. threshold diagnosis;
2. correlation/feature-importance diagnosis;
3. dependency-aware predictive diagnosis;
4. counterfactual diagnosis relative to a known-good structural model;
5. the same counterfactual procedure with an intentionally misspecified model.

Measure wrong-action rate, causal-ranking accuracy, recovery time, observation/inference cost, and failure-to-escalate rate.

## 13. SciRust

No new SciRust gap is justified.

`scirust-causal` already contains a broad causal R&D stack including typed interventions/environments and assumption registries, equivalence-class discovery, backdoor identification/effect estimation, sensitivity analysis, invariant causal prediction, structural simulation and unit-level counterfactuals, experiment design, theory revision, and causal-claim auditing.

This is more than sufficient to investigate counterfactual root-cause mechanisms scientifically. ElasticXxx remains independent: any selected runtime diagnosis implementation must be autonomous in the target runtime.
