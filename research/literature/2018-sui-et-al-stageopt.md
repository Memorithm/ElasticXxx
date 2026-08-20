# Sui et al. 2018 — Stagewise Safe Bayesian Optimization with Gaussian Processes (StageOpt)

## Classification

**ADAPT** for ElasticXxx safe diagnostic experimentation.

## Evidence status

- **SOURCE-DERIVED** unless explicitly marked otherwise.
- Primary source: Yanan Sui, Vincent Zhuang, Joel W. Burdick, Yisong Yue, ICML 2018 / PMLR 80.
- The PDF page describing the two-stage algorithm and confidence-set construction was visually inspected successfully.

## Problem

Optimize an unknown utility function

```text
f(x)
```

subject to one or more unknown safety functions:

```text
g_i(x) >= h_i
```

for every sampled decision.

Unlike SafeOpt's single reward-threshold formulation, StageOpt explicitly allows utility and safety to be different functions.

## Core mechanism

StageOpt models utility and safety functions with separate Gaussian processes and separates the procedure into two stages:

1. **safe-region expansion**;
2. **utility maximization inside the established safe region**.

The paper argues this is natural when safety and utility lie on different scales or are measured through different feedback channels.

## Safe-set construction

Like SafeOpt, the algorithm starts from a known-safe seed set and uses lower confidence bounds plus Lipschitz regularity to expand an increasing sequence of established-safe sets.

The safe region reachable from the seed is limited by finite horizon, evidence quality and the topology/regularity of the safety functions. It need not contain the global utility optimum.

## Why the stage separation matters

Interleaving utility optimization with safe-set expansion implicitly couples two objectives that may have different measurement costs and semantics.

StageOpt instead permits a dedicated expansion budget `T0`, followed by optimization budget `T1 = T - T0`.

For ElasticXxx, the broader lesson is not that every controller needs exactly two fixed stages. The lesson is that **safety learning and utility learning are different inference problems** and may deserve separate observations, models and budgets.

## Guarantee scope

The paper provides probabilistic safety and convergence guarantees under its GP/RKHS, Lipschitz and noise assumptions.

Those guarantees are conditional statistical guarantees, not type-system or semantic-contract guarantees.

## Elastic relation

### ADOPT

- separate safety models from utility/progress models;
- permit different observation channels for safety and utility;
- budget safe-set expansion separately from utility optimization when useful;
- maintain explicit safe seed / safely reachable region concepts for learned operational constraints.

### ADAPT

ElasticXxx should generalize from two fixed stages to a planner state such as:

```text
ExplorationPhase {
    CertifySafety,
    ImproveUtility,
    Diagnose,
    VerifyModel,
}
```

with the phase chosen according to evidence and control deadlines rather than permanently hard-coded.

### REJECT

Do not let a learned `g_i(x) >= h_i` replace hard semantic legality.

Do not assume one GP or one smooth metric can represent safety across CPU, memory, GPU, network, thermal and distributed state transitions.

## Elastic proposal: two safety layers

```text
HardSafetyContract
    compile-time / runtime trusted rules
    never relaxed by learned evidence

OperationalSafetyModel
    empirical/statistical estimate
    versioned, contextual, uncertain, revocable
```

Candidate certificate:

```text
OperationalSafetyCertificate {
    action_or_region,
    model_id,
    model_epoch,
    observation_epoch,
    assumptions,
    lower_confidence_bounds,
    threshold_policy,
    environment_fingerprint,
    expires_or_invalidates_on,
}
```

This certificate is evidence for the planner, not a trusted authorization token.

## Experiment required

Compare:

- one joint utility/safety score;
- interleaved safe exploration;
- stagewise safety then utility;
- Elastic adaptive-phase scheduling.

Use separate safety and utility observables and include abrupt workload/topology changes. Measure hard violations, operational-threshold violations, time spent certifying safety, useful progress and stale-certificate invalidation latency.
