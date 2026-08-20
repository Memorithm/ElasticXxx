# DS2: Fast, Accurate Automatic Scaling for Distributed Streaming Dataflows

## Status

**SOURCE-DERIVED MECHANISM REVIEW + ELASTIC RELATION.**

Reference: Vasiliki Kalavri, John Liagouris, Moritz Hoffmann, Desislava Dimitrova, Matthew Forshaw, Timothy Roscoe, *Three steps is all you need: fast, accurate, automatic scaling decisions for distributed streaming dataflows*, OSDI 2018.

Primary source: https://www.usenix.org/system/files/osdi18-kalavri.pdf

Relevant PDF pages were visually inspected successfully during this review, including the true-rate model page. One later screenshot attempt hit a cache miss; textual extraction remained available.

## 1. Problem

DS2 targets automatic scaling of long-running streaming dataflows. The authors argue that coarse metrics such as observed throughput, backpressure, and CPU utilization can produce inaccurate provisioning, oscillation, and slow convergence.

DS2 combines:

- a logical dataflow performance model;
- lightweight runtime instrumentation;
- estimates of per-operator true processing/output rates;
- a calculation of required operator parallelism.

## 2. Observed rate is not capacity

DS2's core distinction is between **observed time** and **useful time**.

Useful time is the time spent deserializing, processing, and serializing. It excludes waiting on input or output.

For one operator instance over observed window `W`, with useful time `Wu`:

```text
true processing rate    = records processed / Wu
observed processing rate = records processed / W
```

and analogously for output rate.

Since `Wu ≤ W`, the observed rate can be lower than the true rate simply because the operator is blocked or starved.

**SOURCE-DERIVED:** DS2 interprets true rate as the instance's sustainable processing/output capacity for the current workload.

## 3. Elastic correction: observation is not capability

This reinforces a distinction already emerging elsewhere in ElasticXxx:

```text
ObservedMetric
    !=
EffectiveCapabilityEstimate
```

Examples beyond streaming:

```text
observed GPU throughput != isolated kernel capacity
observed network throughput != available path capacity
observed disk bandwidth != device/service capacity
observed worker throughput != processing capability
```

A resource may look slow because it is waiting, throttled, contended, starved, backpressured, or operating under a different workload mix.

Candidate pipeline:

```text
RawObservation
   ↓
ObservationModel
   ↓
EffectiveCapacityEstimate
   ↓
Impact / candidate plan
```

This is an **ELASTIC PROPOSAL** generalized from DS2.

## 4. Graph-aware scaling calculation

DS2 models a logical acyclic dataflow graph. It uses source rates and aggregated true processing/output rates to compute target parallelism for downstream operators, propagating operator selectivity through the graph.

The output is the estimated optimal number of instances per logical operator required to sustain the target source rates.

This is not a generic Elastic planner; it is a domain-specific analytical model over a known graph.

**Elastic relation — ADOPT the principle, ADAPT the model:** domain-aware analytical models can be preferable to generic black-box reactive policies where the mechanism admits useful structure.

## 5. Assumptions and conditional guarantees

DS2's strongest properties assume perfect/linear scaling of true rates with instance count. The paper notes that real scaling is often sub-linear and can occasionally be super-linear.

Under the stated non-superlinear/linear assumptions, DS2 establishes properties described as:

- no overshoot on scale-up;
- no undershoot on scale-down;
- monotonic convergence rather than oscillation;
- one-step convergence when true rates scale linearly and the target rate is correctly estimated.

These guarantees are **conditional on the model assumptions**. They must not be generalized to arbitrary Elastic resources.

## 6. Controller timescale

DS2 explicitly states that a controller should target workload changes occurring on a timescale greater than its convergence time; reacting to faster changes can create inefficient fluctuations. The paper notes that buffering/backpressure can be preferable to dynamic scaling for changes shorter than the controller's convergence timescale.

This suggests an important Elastic condition:

```text
control useful only if

T_observe
+ T_decide
+ T_validate
+ T_transition
+ T_settle
< relevant environment/workload timescale
```

More precisely, this should be probabilistic when the environment timescale is uncertain.

A plan can be semantically legal yet operationally irrational because it cannot finish before the condition that motivated it disappears.

## 7. Reconfiguration latency can dominate model speed

The paper notes that DS2's model calculation can be fast enough that responsiveness becomes limited by the underlying stream processor's scaling mechanism. Systems evaluated in the paper commonly scale via checkpoint, redeploy, and restore.

This strongly reinforces ElasticXxx's existing rule:

```text
planner speed != adaptation speed
```

and the need to account for transition latency explicitly.

## 8. Comparison with Dhalion

In the Heron word-count recreation:

- Dhalion performs six scale-up decisions;
- reaches 22 FlatMap and 30 Count instances after roughly 2000 seconds;
- DS2, with a 60-second decision interval, predicts 10 FlatMap and 20 Count instances after one interval;
- the paper identifies 10/20 as the minimum configuration handling the configured input rate in that experiment.

The authors attribute part of Dhalion's slow response to queue filling/backpressure detection, whereas DS2 depends on its aggregation interval.

These are scenario-specific experimental results, not universal scaling claims.

## 9. Convergence on broader experiments

The paper reports that across 36 Nexmark experiments used for convergence testing, DS2 required at most three steps; 19 converged in one step, 14 in two, and 3 in three.

This is useful evidence that one-step convergence is not unconditional in practice even for the evaluated domain.

## 10. Instrumentation overhead

At the smallest 10-second decision interval used in the paper, instrumentation overhead was reported as at most:

- 13% on Flink, about 40 ms absolute latency difference;
- 20% on Timely, about 5 ms absolute latency difference;

across the evaluated queries.

Thus observation cost is not automatically negligible.

**Elastic relation — ADOPT:** observation and capacity-estimation overhead belong in `ControlCost`.

## 11. Model validity lifecycle

A true-rate estimate is conditional on the current workload and operating regime. Record size/content, maintained state, contention and machine behavior can alter processing cost.

Therefore an Elastic capacity estimate should carry context/freshness information rather than become an eternal property of the resource.

Candidate:

```text
CapacityEstimate {
    value,
    workload_context,
    observation_window,
    timestamp_or_epoch,
    confidence,
    estimation_cost,
}
```

This is an **ELASTIC PROPOSAL**.

## 12. Elastic classification

### ADOPT

- observed performance distinct from effective service capacity;
- useful-time / waiting-time separation where meaningful;
- graph/domain structure in analytical planning;
- controller convergence timescale must match environment timescale;
- measurement overhead is part of control cost;
- transition mechanism may dominate controller responsiveness.

### ADAPT

- one stream-specific true-rate model → typed domain-specific `CapacityModel` backends;
- scalar point estimates → estimates with context, freshness, confidence and cost;
- static logical DAG assumption → general resource/dependency graphs only where the domain supports them.

### REJECT from generic core

- universal assumption of linear scaling;
- universal acyclic dataflow requirement;
- treating backpressure-free throughput as the objective for every resource.

## 13. Experiment

**EXPERIMENT REQUIRED.** Build a synthetic resource where observed throughput is artificially lowered by waiting/contention while isolated service capacity remains constant.

Compare planners using:

1. observed rate directly;
2. corrected effective-capacity estimate;
3. corrected estimate with uncertainty;
4. corrected estimate whose observation cost is explicitly accounted.

Measure provisioning error, oscillation, transition count, useful progress and total control overhead.

## 14. SciRust

Repository searches performed during this review found no obvious generic queueing/service-capacity modelling module in SciRust.

A possible future capability is therefore:

```text
SCIRUST-GAP-PERF — queueing / service-capacity / response-time models
STATUS: INVESTIGATE
```

Do **not** implement this from DS2 alone. The useful scientific abstraction must be established against broader queueing/performance-modelling literature and at least one independent domain before adding a new SciRust family.
