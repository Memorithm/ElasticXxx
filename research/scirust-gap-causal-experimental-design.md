# SCIRUST-GAP-CAUSAL-EXP-001 — Cost/Budget-Aware Causal Experimental Design

## Status

**CONFIRMED GAP — DESIGN REQUIRED, NOT YET IMPLEMENTED.**

## Existing SciRust capability

`scirust-causal::experiment_design` already provides a strong graph-structural experimental-design primitive:

- feasible single-variable intervention targets;
- guaranteed and optimistic orientation counts;
- explicit outcome-enumeration cap;
- deterministic ranking;
- worst-case greedy experiment sequences;
- causal assumptions and certificate output.

Its documentation explicitly states that it has:

- no intervention cost model;
- no ethics model;
- no sample-size model;
- no feasibility semantics beyond the supplied target list.

Therefore this is an extension gap, not an absence of causal experimental design.

## Independent literature support

### He & Geng 2008

Shows batch versus sequential intervention planning and minimax / maximum-entropy selection. Sequential planning reuses evidence from previous interventions.

### Lindgren et al. 2018

Formalizes minimum-cost intervention design with variable-specific costs, proves NP-hardness and develops an approximation algorithm with stated guarantees.

### Agrawal et al. 2019 (ABCD)

Treats targeted Bayesian causal discovery under finite samples, finite rounds and intervention constraints, using expected information gain / entropy reduction and tractable approximations.

The gap is therefore supported by multiple independent research formulations and is scientifically useful without ElasticXxx.

## Why one API is not enough

At least two distinct scientific problems must remain separate.

### A. Structural minimum-cost design

Inputs:

```text
essential / chordal graph
per-target intervention costs
max number of interventions
optional intervention sparsity
```

Objective:

```text
orient the desired graph structure at minimum total cost
```

Possible algorithm family:

```text
graph separating systems
weighted coloring
approximation algorithms
exact optimization for bounded cases
```

### B. Bayesian targeted budgeted design

Inputs:

```text
posterior / graph hypothesis distribution
target function f(G)
intervention likelihood model
sample budget
round budget
feasible intervention set
```

Objective:

```text
maximize expected information / target uncertainty reduction
under budget
```

Possible algorithm family:

```text
mutual information
Bayesian experimental design
submodular approximation
graph / parameter sampling
```

Do not hide these behind one method named `best_experiment()` with undocumented assumptions.

## Proposed SciRust direction

A generic architecture could eventually expose separate traits / modules such as:

```text
StructuralInterventionDesign
BayesianExperimentDesign
ExperimentBudget
InterventionCost
ExperimentUtility
```

but this naming is provisional.

The first implementation should preserve each backend's assumptions and guarantees rather than force a universal score.

## Validation requirements

Any implementation should include:

- tiny exact graph cases with exhaustive optimal solutions as oracle;
- comparison against existing `plan_next_experiment` when all costs are equal and the objective is purely structural;
- explicit tests for infeasible budgets;
- deterministic tie-breaking;
- overflow / non-finite cost rejection;
- documentation of perfect-intervention assumptions;
- comparison against exhaustive enumeration or integer programming on small instances where practical;
- no silent conversion of cost-sensitive ranking into information-gain ranking.

## Architectural boundary

SciRust remains scientific R&D tooling. ElasticXxx must not depend on SciRust at runtime. If an experimental-design backend proves useful for ElasticXxx, a validated autonomous implementation belongs in the target project or a target-specific component.
