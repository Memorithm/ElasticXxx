# SCIRUST-GAP-PERF — Queueing / Service-Capacity / Response-Time Models

## Status

**INVESTIGATE — not confirmed, not scheduled for implementation.**

## Origin

The immediate trigger is the DS2 OSDI 2018 review. DS2 distinguishes observed throughput from an operator's true processing/output rates by removing waiting time and treating the remaining rate as current sustainable service capacity.

Direct SciRust repository searches during the review found no obvious generic queueing/service-rate modelling family under terms including queueing/queuing, service rate, Little's law, arrival rate, and birth-death models.

Absence from these searches is not sufficient to declare a confirmed gap.

## Candidate scientific scope

A genuinely general capability might eventually cover a subset of:

```text
arrival processes
service-time distributions
utilization
service capacity
Little's law
M/M/1, M/M/c, M/G/1 or related analytical models
queue-length / waiting-time / response-time relationships
bottleneck / operational analysis
confidence / fitting from traces
```

The correct scope is deliberately unresolved.

## Why this passes the first generality test

Such models remain useful without ElasticXxx in:

- computer/network performance analysis;
- service systems;
- operations research;
- manufacturing;
- call centers;
- storage systems;
- distributed systems;
- capacity planning.

## Why implementation is premature

1. DS2 is only one direct motivating system paper.
2. A broader queueing/performance-modelling literature review is required.
3. The appropriate API boundary inside SciRust is unknown.
4. Existing crates or numerical primitives may already be preferable to a new in-house implementation.
5. ElasticXxx currently only needs the conceptual distinction `observed delivery != effective capacity`; it does not require a queueing-theory runtime dependency.

## Next evidence required

Before promotion to `CONFIRMED GAP` or `IMPLEMENT`:

- inspect SciRust modules more broadly for equivalent functionality under different names;
- review at least one foundational/general queueing-performance source;
- identify an independent scientific use case outside stream autoscaling;
- compare mature Rust/native libraries;
- define validation against analytical cases and simulation.

## Architectural rule

Even if SciRust later gains these models, ElasticXxx remains independent. SciRust may be used during R&D to formulate, compare, fit, and validate capacity models; any selected runtime implementation must remain autonomous in the target project.
