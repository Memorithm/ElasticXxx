# SCIRUST-GAP-OPT-BO-001 — Safe / Constrained Sequential Bayesian Optimization

## Status

**CONFIRMED GAP — DESIGN REQUIRED, NOT YET IMPLEMENTED.**

## Existing SciRust capability

The gap is narrower than "Bayesian optimization" or "Gaussian processes".

SciRust already provides:

### `scirust-gp`

- exact Gaussian-process regression;
- posterior mean and variance;
- RBF and Matérn kernels;
- deterministic, dependency-free implementation.

### `scirust-automl`

- a simplified Gaussian-process Bayesian optimizer;
- Expected Improvement acquisition;
- bounded continuous search;
- AutoML hyperparameter optimization.

### `elastic-autotuner`

- deterministic candidate generation;
- static constraint filtering;
- analytical ranking;
- correctness + measurement evidence before plan promotion.

These are useful building blocks, but none of the inspected APIs provide SafeOpt/StageOpt-style unknown-constraint certification.

## Missing scientific capability

A general scientific API for sequential optimization in which:

- utility is unknown and learned online;
- one or more safety/feasibility functions are also unknown;
- every queried point must satisfy a confidence-based safety policy;
- exploration starts from a known-safe seed set;
- safe-set reachability/expansion is explicit;
- utility and safety may use different models/feedback channels;
- guarantee scope and model assumptions are surfaced rather than hidden.

## Independent literature support

### SafeOpt — Sui et al. 2015

Introduces confidence-bound safe-set expansion from a known-safe seed and convergence to a near-optimum in the safely reachable region under stated GP/RKHS/Lipschitz assumptions.

### StageOpt — Sui et al. 2018

Separates safe-region expansion from utility maximization and allows multiple unknown safety constraints distinct from utility.

These methods are independently useful in experimental design, robotics, clinical/biomedical optimization and tuning, so the capability passes the "useful without ElasticXxx" test.

## Why static constraints are not enough

`elastic-autotuner` can reject a candidate through a deterministic `ElasticConstraintSolver` before measurement.

Safe BO addresses a different problem:

```text
constraint value itself is unknown
        ↓
measure cautiously
        ↓
update uncertainty
        ↓
certify additional query points
```

Do not overload a static constraint trait with probabilistic model semantics.

## Why ordinary Bayesian optimization is not enough

Expected Improvement or UCB can deliberately sample poor/unsafe regions to gain information.

Safe optimization adds a query-admissibility rule derived from lower/upper confidence bounds and an established-safe set.

## Proposed SciRust direction

Do not implement one monolithic `safe_bayesian_optimize()` before separating the concepts.

Potential generic pieces:

```text
SequentialSurrogate
ConfidenceInterval
UnknownConstraintModel
SafeSeed
CertifiedRegion
ReachabilityModel
SafeAcquisitionPolicy
SafeOptimizationTrace
```

Naming is provisional.

At least two backends may be justified:

1. SafeOpt-like interleaved expansion/optimization;
2. StageOpt-like separate safety-expansion and utility stages.

The generic layer should expose assumptions and guarantee type rather than imply universal safety.

## Safety terminology requirement

Any future SciRust API must avoid calling its result simply `Safe` without qualification.

Prefer terminology such as:

```text
ConfidenceCertified
ModelCertifiedSafe
ProbabilisticallyCertified
```

because guarantees are conditional on model/regularity/confidence assumptions.

## Validation requirements

Before implementation is accepted:

- reproduce small SafeOpt examples with an exactly known synthetic function;
- verify no sampled point violates the known synthetic threshold under conditions where the theorem assumptions are met;
- test disconnected safe regions that are unreachable from the seed;
- test insufficient/empty safe seed errors;
- test multiple safety functions for StageOpt-like backend;
- deterministic candidate/tie handling;
- reject non-finite kernels, thresholds and observations;
- expose confidence parameters in traces;
- include adversarial tests where smoothness/model assumptions are wrong and explicitly demonstrate that the statistical guarantee no longer applies;
- never present model-based certification as a formal or physical hard-safety proof.

## Relationship to ElasticXxx

SciRust may be used during R&D to compare safe-exploration algorithms and validate statistical models.

ElasticXxx must remain independent at runtime. If a SafeOpt-like backend is eventually useful for Elastic diagnostic probing, it must be implemented autonomously behind Elastic's own hard validator and cannot enlarge the hard-admissible action set.
