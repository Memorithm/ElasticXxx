# SCIRUST-GAP-PERF — Queueing / Service-Capacity / Response-Time Models

## Status

**PARTIALLY RESOLVED / INVESTIGATE REMAINDER.**

A narrow general operational-analysis layer is now being validated in SciRust PR #1291. Broader queueing-network, fitting, uncertainty, and response-time modelling remain **INVESTIGATE**.

## Origin

DS2 (OSDI 2018) exposed the distinction between observed throughput and sustainable processing capacity by measuring useful rather than waiting time.

Denning & Buzen (ACM Computing Surveys 1978) independently established a broader operational-performance framework based on measurable/testable quantities, including utilization, Little's law, forced flow, service demand, response-time relations, and bottleneck analysis.

Together these sources established that the scientific need is broader than stream autoscaling.

## Important repository-audit correction

The initial search was incomplete.

SciRust **already contains queueing functionality** in:

```text
scirust-sim/src/stochastic.rs
```

including:

- `MM1Queue`;
- deterministic discrete-event simulation with explicit seed;
- traffic intensity `rho = lambda / mu`;
- time-average system population;
- utilization;
- mean sojourn time;
- tests against classical M/M/1 formulas;
- an explicit Little's-law cross-check.

Therefore statements such as:

```text
SciRust has no queueing support
```

are false and must not be repeated.

## Narrow capability identified as missing

What was not exposed as a reusable API was the **distribution-agnostic operational-analysis layer** used to relate directly measured quantities:

```text
U = X S
N = X R
X_i = V_i X_0
D_i = V_i S_i
R = M/X - Z
```

plus deterministic service-demand/bottleneck analysis.

These relations were partially present only as formulas/oracles around the M/M/1 simulation rather than as a general scientific interface.

## Current SciRust enrichment under validation

SciRust PR **#1291**, branch `feat/operational-performance-laws`, adds a minimal:

```text
scirust-sim::operational
```

with:

- utilization law;
- Little's law in population/response forms;
- forced-flow law;
- interactive response-time relation;
- `ServiceDemand`;
- deterministic bottleneck analysis;
- saturation-throughput bound;
- Denning–Buzen example tests.

The PR deliberately does **not** add an Elastic planner or general queueing-network solver.

Until CI and rustdoc are green and the PR is merged, this enrichment is not release-qualified.

## Remaining scientific scope — INVESTIGATE

Possible future general capabilities include some subset of:

```text
M/M/c
M/G/1
queueing networks
open/closed network solvers
multiple service centers/classes
distribution fitting from traces
confidence intervals / uncertainty
bottleneck inference from noisy measurements
response-time prediction under proposed configuration changes
```

No claim is made that SciRust needs all of these.

## Why the remainder passes the generality test

Such models remain useful without ElasticXxx in:

- computer/network performance analysis;
- operations research;
- manufacturing/service systems;
- storage systems;
- telecommunications;
- capacity planning;
- reliability/availability modelling when queues interact with repair/service resources.

## Evidence still required before another implementation

1. broader queueing/performance-modelling literature review;
2. a concrete independent scientific use case;
3. repository audit for equivalent functionality under other SciRust modules;
4. comparison with mature Rust/native libraries;
5. clear validation against analytical cases and simulation;
6. evidence that the new abstraction is preferable to simply composing existing SciRust primitives.

## Architectural rule

ElasticXxx remains independent of SciRust. SciRust may be used during R&D to formulate, compare, fit and validate performance/capacity models; any selected runtime implementation remains autonomous in the target project.
