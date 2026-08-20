# Denning & Buzen: The Operational Analysis of Queueing Network Models

## Status

**SOURCE-DERIVED MECHANISM REVIEW + ELASTIC RELATION.**

Reference: Peter J. Denning and Jeffrey P. Buzen, *The Operational Analysis of Queueing Network Models*, ACM Computing Surveys 10(3), 1978, DOI 10.1145/356733.356735.

Primary accessible copy: https://www.columbia.edu/~ww2040/8100S12/DenningBuzen1978.pdf

PDF screenshots were attempted for the operational-equations and bottleneck-example pages, but the screenshot service returned cache misses. Claims below are grounded in the paper text, not successful visual inspection.

## 1. Problem and methodological contribution

The paper develops queueing-network performance analysis using an **operational** approach: quantities are defined through finite observations of a system, and assumptions are stated so they can be tested against measurements.

The authors contrast this with traditional Markovian queueing-network derivations that invoke assumptions such as stationarity, independence, Markov routing, stochastic equilibrium, exponential service times, and ergodicity. Their point is not that stochastic modelling is useless, but that several such assumptions cannot be conclusively established from a finite observation period.

The operational approach seeks relations whose applicability depends on assumptions that can be checked against the observed system.

## 2. Operational principles

The paper emphasizes three broad operational principles:

1. quantities should be precisely measurable and assumptions directly testable;
2. job flow should be balanced over the observation period where flow-balance results are used;
3. devices should satisfy the stated homogeneity conditions where homogeneous-device results are used.

**Elastic relation — ADOPT the evidence discipline:** a performance model should expose the assumptions under which an estimate is valid rather than silently present every prediction as a fact.

Candidate shape:

```text
ModelEstimate {
    value,
    assumptions,
    evidence_window,
    context,
    uncertainty?,
}
```

This is an **ELASTIC PROPOSAL**.

## 3. Utilization law

For service center `i`, the paper derives the operational utilization relationship:

```text
U_i = X_i S_i
```

where `X_i` is the completion rate over the observation period and `S_i` the mean service time per completion.

This distinguishes **busy service demand** from elapsed time that may include waiting elsewhere.

The relation is directly relevant to DS2's later distinction between useful processing time and observed wall-clock throughput.

## 4. Little's law

The paper derives the operational form:

```text
N_i = X_i R_i
```

and applies Little's law at whole-system level as well.

This relates mean population, throughput, and mean response/sojourn time without requiring an exponential service-time assumption for the identity itself.

## 5. Forced flow

Under the paper's flow-balance conditions, visit ratios relate device throughput to system throughput:

```text
X_i = V_i X_0
```

This allows device demand to be connected to global useful progress.

**Elastic inference:** resource-local pressure should be interpreted in relation to workload flow/dependency structure when such structure is known. A high local utilization at a non-critical component is not equivalent to a global bottleneck.

## 6. Response-time relations

The paper gives the general response-time relation:

```text
R = Σ_i V_i R_i
```

and, for the terminal/interactive model under flow-balance assumptions:

```text
R = M / X_0 - Z
```

where `M` is the active terminal/user population and `Z` the mean think time.

These relations demonstrate that response time is structurally tied to workload circulation, not just one resource's utilization.

## 7. Service demand

A core derived quantity is service demand:

```text
D_i = V_i S_i
```

It represents mean service time demanded from device `i` per system-level completion.

This is a particularly useful abstraction for ElasticXxx because it separates:

```text
resource usage per useful completion
```

from raw utilization or throughput.

It is conceptually parallel to the useful-progress lesson from Pollux: optimizing a local busy metric does not necessarily optimize end-to-end progress.

## 8. Bottleneck analysis

For the closed-system model considered by the paper, the largest service demand determines the asymptotic bottleneck under the stated assumptions. The saturation throughput bound is related to:

```text
1 / max_i(D_i)
```

The paper's Figure 11 example has visit ratios and service times yielding demands approximately:

```text
CPU   1.00 s
Disk  0.88 s
Drum  0.32 s
```

so the CPU is the bottleneck and the sum of no-wait service demands is `2.20 s`.

**Elastic relation — ADOPT the structural principle:** an intervention should be evaluated against the current limiting mechanism. Improving a non-bottleneck resource can have little or no effect on the asymptotic throughput objective.

Do not generalize the exact bottleneck formula to arbitrary heterogeneous/distributed resources without validating its assumptions.

## 9. Capacity, delivery, and demand

This review reinforces three distinct notions:

```text
ObservedDelivery
    what completed during the measurement window

ServiceDemand
    resource work required per useful system completion

EffectiveCapacity
    sustainable delivery under declared operating assumptions
```

None is automatically interchangeable with the others.

For ElasticXxx this supports:

```text
Observation
  ↓
Operational / domain model
  ↓
Demand + Capacity + Bottleneck estimates
  ↓
Impact model
  ↓
Candidate interventions
```

rather than `high utilization → scale`.

## 10. Model assumptions as first-class data

The most valuable general lesson is methodological. A model prediction should ideally carry:

```text
what was measured
when it was measured
which assumptions were checked
which assumptions remain unverified
which operating regime it describes
```

An old service-demand estimate can become invalid when representation, workload mix, hardware, routing, precision, batching, or contention changes.

This fits the existing Elastic notions of generation, context, observation epoch, confidence, and provenance.

## 11. Calculation versus prediction

The paper distinguishes uses of operational results for calculating unmeasured quantities from observed data and for predicting performance under changed conditions. Prediction necessarily relies on additional assumptions about which measured quantities remain invariant after the hypothetical change.

**Elastic relation — ADOPT:** do not confuse an identity that reconstructs a current metric with a causal model predicting the outcome of a transition.

This maps cleanly to:

```text
MeasurementIdentity
CurrentStateEstimate
TransitionEffectModel
```

as separate concepts.

## 12. Elastic classification

### ADOPT

- operationally measurable/testable quantities;
- explicit model assumptions;
- utilization/Little/flow identities where domain conditions apply;
- service demand as useful-progress-normalized resource work;
- bottleneck-aware intervention reasoning;
- separation of current-state calculation from change prediction.

### ADAPT

- fixed queueing-network service centers → typed resource/dependency domains;
- point measurements → versioned/contextual estimates with uncertainty and cost;
- static bottleneck identity → bottleneck/critical-path model appropriate to each domain.

### REJECT from generic core

- universal queueing-network topology;
- universal flow-balance/homogeneity assumptions;
- assuming every Elastic resource can be reduced to one service center;
- assuming the largest scalar service demand always determines system performance.

## 13. Experiment

**EXPERIMENT REQUIRED.** Create a pipeline with three resources where one resource is the true bottleneck and another has high observed utilization due to unrelated work.

Compare planners using:

1. highest utilization;
2. observed throughput only;
3. service-demand/bottleneck analysis;
4. service-demand analysis plus transition cost and uncertainty.

Measure useful throughput, unnecessary transitions, control cost, prediction error, and time to identify a changed bottleneck.

## 14. SciRust relation

The review corrected an earlier preliminary gap assessment.

SciRust already contains `scirust-sim::stochastic::MM1Queue`, a deterministic-seed discrete-event M/M/1 simulation whose tests validate utilization, mean population, mean sojourn time, and Little's law against classical formulas.

Therefore **queueing itself is not absent from SciRust**.

The missing reusable layer identified by repository inspection was narrower: the operational identities and service-demand/bottleneck analysis were present only implicitly as formulas/oracles rather than as a general API.

A dedicated SciRust PR now investigates this minimal general enrichment. This remains R&D tooling; ElasticXxx has zero SciRust runtime dependency.
