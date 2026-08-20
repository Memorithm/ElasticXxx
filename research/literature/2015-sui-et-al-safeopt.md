# Sui et al. 2015 — Safe Exploration for Optimization with Gaussian Processes (SafeOpt)

## Classification

**ADAPT** for ElasticXxx diagnostic experimentation; **do not use as a substitute for hard semantic safety**.

## Evidence status

- **SOURCE-DERIVED** unless explicitly marked otherwise.
- Primary source: Yanan Sui, Alkis Gotovos, Joel W. Burdick, Andreas Krause, ICML 2015 / PMLR 37.
- PDF pages covering reachability and Algorithm 1 were visually inspected successfully.

## Problem

Sequentially optimize an unknown scalar reward function `f(x)` from noisy observations while requiring every sampled decision to satisfy a safety threshold:

```text
f(x_t) >= h
```

for all rounds.

The safe set is initially unknown except for a supplied seed set `S0` containing at least one known-safe decision.

## Model

SafeOpt models the unknown reward using a Gaussian process posterior and assumes regularity sufficient to transfer evidence between nearby decisions:

- bounded norm in the RKHS associated with the kernel;
- Lipschitz continuity with known constant `L` / metric `d` in the paper's analysis;
- noisy observations.

The resulting safety guarantee is therefore conditional on these assumptions and confidence bounds.

## Reachability

The paper does not promise the globally optimal decision over all `D`.

It defines a one-step safe reachability operator of the form:

```text
R_epsilon(S)
  = S union { x | exists x' in S:
                    f(x') - epsilon - L d(x',x) >= h }
```

and optimizes relative to the closure of the safely reachable region from `S0`.

This is a key distinction: safety constraints may make parts of the decision space unreachable without violating the safety policy.

## Confidence sets

At each round, GP posterior intervals are intersected with earlier confidence intervals, so confidence sets shrink monotonically.

The algorithm maintains:

```text
S_t   established-safe decisions
G_t   safe expanders that may certify new safe decisions
M_t   potential maximizers within the safe set
```

It samples only from `G_t union M_t`, choosing the candidate with the largest remaining confidence width.

Thus exploration serves two purposes:

1. identify the best safe decision;
2. expand what can itself be certified safe.

## Guarantee scope

Under the paper's regularity/noise/confidence assumptions, SafeOpt provides a high-probability guarantee of sampling only safe decisions and convergence to a near-optimal decision in the safely reachable region.

This is **not** a formal program-safety guarantee and does not cover arbitrary stateful transitions.

## Important limitation for ElasticXxx

The paper explicitly works in a bandit-style setting where decisions do **not** induce persistent state transitions. This gives stronger/simpler guarantees than safe exploration in a general MDP/control system.

Elastic resource transitions can alter queues, residency, placement, state replicas, thermal conditions and later admissible actions. SafeOpt therefore cannot be transplanted directly as the Elastic runtime safety mechanism.

## Elastic relation

### ADOPT

- maintain an explicit set of experiments currently certified safe;
- require a known-safe seed / fallback region for learned exploration unless another safety proof exists;
- distinguish globally admissible actions from actions currently **established safe under a learned model**;
- make unreachable safe regions explicit instead of pretending every legal state can be safely explored online.

### ADAPT

ElasticXxx needs at least three different predicates:

```text
SemanticallyLegal(action)
HardSafetyValid(action)
LearnedOperationallySafe(action, model, confidence)
```

Only the last is SafeOpt-like.

A learned safety model may further restrict the hard-valid set; it may never expand beyond hard semantic/safety constraints.

### REJECT

Do not represent a GP confidence bound as an `Invariant` or trusted capability proof.

Do not treat smoothness/kernel assumptions as universal across heterogeneous resources.

## Elastic proposal

Define a layered admissibility relation:

```text
HardAdmissibleSet(s)
    = actions satisfying type, semantic, ownership, capability and hard safety rules

CertifiedOperationalSet_t(s)
    subseteq HardAdmissibleSet(s)
    = actions whose learned/empirical operational risk satisfies the active policy
```

**ELASTIC PROPOSAL:** model-based safe exploration must be monotone only relative to its evidence/model epoch, not globally forever. If the workload, topology, model version or assumptions change, prior operational-safety certificates may become stale.

## Experiment required

Create a synthetic resource surface with:

- known hard-invalid actions;
- unknown operational performance threshold;
- one initially known-safe region;
- workload phase changes.

Compare unconstrained BO, SafeOpt-like exploration, and no exploration. Measure violations, coverage of safely reachable actions, time to useful decision and stale-certificate failures after environment changes.
