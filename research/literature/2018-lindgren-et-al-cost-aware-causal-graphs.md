# Lindgren et al. 2018 — Experimental Design for Cost-Aware Learning of Causal Graphs

## Classification

**ADAPT / INVESTIGATE** for ElasticXxx; **CONFIRMS A GENERAL SCIRUST GAP** in cost-aware causal experimental design.

## Evidence status

- **SOURCE-DERIVED** unless marked otherwise.
- Primary source: Erik M. Lindgren, Murat Kocaoglu, Alexandros G. Dimakis, Sriram Vishwanath, NeurIPS 2018.
- PDF text was inspected. Screenshot retrieval was attempted on the algorithm/results pages but failed with a source cache miss; no visual-only claim is used.

## Problem

Given the undirected/chordal component of an essential graph and a cost `w_v` for intervening on each variable, find a set of interventions that orients the graph at minimum total intervention cost.

The paper's cost objective is additive:

```text
cost(I) = sum over interventions I_j of sum over v in I_j of w_v
```

with an upper bound on the number of interventions.

A sparse variant also limits how many variables may be manipulated in any single intervention.

## Structural reduction

The paper relates intervention design to graph-separating systems / graph coloring. This allows the causal intervention problem to be studied as a combinatorial optimization problem.

## Hardness

The paper proves that the minimum-cost intervention-design problem is NP-hard even on chordal graphs, and even when vertex costs are equal.

This matters for ElasticXxx: once experiment cost is introduced, "pick the cheapest informative probe" is not generally equivalent to finding a globally minimum-cost diagnostic campaign.

## Approximation algorithm

The paper proposes a greedy weighted-coloring procedure with quantization.

Under its stated condition on the available number of interventions, Theorem 9 gives a `(2 + epsilon)` approximation to the minimum-cost solution while using a logarithmic-scale number of interventions plus an `O(log log n)` term.

The exact theorem conditions are specific to the graph-separating formulation and must not be generalized to arbitrary Elastic experiment graphs.

## Empirical results

On random chordal graphs in the paper's experiments, the greedy method is reported close to the integer-programming optimum and better than the chosen baseline.

For one runtime experiment on graphs with 10,000 vertices, maximum degree 20 and 5 interventions, the paper reports roughly 5 s for the greedy method versus 128 s for the Gurobi integer-programming solution.

The sparse-intervention experiment explicitly illustrates a trade-off between number of interventions and total intervention cost.

## Limitations

- assumes the relevant essential-graph structure is known;
- objective is full edge-direction recovery rather than a targeted runtime decision;
- intervention cost is scalar/additive at the variable level;
- perfect intervention semantics are assumed by the causal abstraction;
- does not model service disruption, transition settling, semantic invariants or intervention failure.

## Elastic relation

### ADOPT

- intervention cost belongs in the planning problem, not only in post-hoc telemetry;
- multiple low-cost local probes can dominate one apparently direct expensive probe;
- exact global optimization may be computationally inappropriate on a fast control path.

### ADAPT

ElasticXxx requires multi-dimensional, state-dependent experiment cost:

```text
C_exp(e | s) = [
    latency,
    resource_use,
    transition_cost,
    disruption,
    energy,
    risk,
]
```

and hard feasibility constraints.

The experiment graph can also change after each observation or resource transition, so sequential replanning is more natural than solving one immutable design problem once.

### INVESTIGATE

A graph-structural approximation backend inspired by cost-aware intervention design may be useful for a slow diagnostic-planning path, but only after a concrete Elastic diagnostic graph and cost semantics exist.

## SciRust implication

`scirust-causal::experiment_design` already ranks perfect single-target experiments by guaranteed and optimistic orientation counts and supports feasible target lists. Its own documentation explicitly states that it has **no cost model, no sample-size model, and no ethics/feasibility semantics beyond the supplied target list**.

After He & Geng, ABCD, and this paper, the missing scientific capability is no longer speculative:

```text
SCIRUST-GAP-CAUSAL-EXP-001
Cost/budget-aware causal experimental design
Status: CONFIRMED GAP / DESIGN REQUIRED
```

Do not implement one opaque heuristic. At least two distinct scientific backends are justified by the literature:

1. graph-structural minimum-cost intervention design;
2. Bayesian targeted information-gain design under finite budgets/rounds.

They have different inputs, assumptions, objectives and guarantees.
