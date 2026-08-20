# Agrawal et al. 2019 — ABCD-Strategy: Budgeted Experimental Design for Targeted Causal Structure Discovery

## Classification

**ADAPT / INVESTIGATE** for ElasticXxx active diagnosis.

## Evidence status

- **SOURCE-DERIVED** unless marked otherwise.
- Primary source: Raj Agrawal, Chandler Squires, Karren Yang, Karthikeyan Shanmugam, Caroline Uhler, AISTATS 2019 / PMLR 89.
- PDF text was inspected. Screenshot retrieval was attempted on the method page but failed with a source cache miss; no visual-only claim is made here.

## Problem

The paper asks how to select interventions when experiments are expensive and the experimenter has finite samples, finite rounds and feasibility constraints.

Crucially, the objective need not be recovery of the entire causal DAG. The experimenter may care about a function `f(G)` of the graph, such as whether one node is downstream of another.

## State and uncertainty

The method maintains a Bayesian posterior over causal graphs given the observational and interventional data collected so far.

The quantity of interest is the posterior uncertainty of a target function `f(G)`.

## Experimental-design objective

ABCD uses expected utility based on mutual information. Maximizing this utility is equivalent to maximizing expected entropy reduction of the target `f(G)`.

Conceptually:

```text
current posterior
    + candidate intervention allocation
    -> hypothetical outcomes
    -> expected posterior
    -> expected information gain about f(G)
```

## Budget model

The framework explicitly permits constraints such as:

- finite number of samples per round;
- finite number of experimental rounds;
- maximum number of distinct intervention targets in a batch;
- feasibility restrictions on allowed experiments.

Thus `informative` and `feasible under the experiment budget` are separate predicates.

## Sequential / batched feedback

Experiments can be allocated across multiple batches. More rounds allow evidence from earlier batches to influence later choices.

This is important for ElasticXxx because a fixed diagnostic campaign and an adaptive campaign are not equivalent when intervention outcomes are informative.

## Computational problem

Exact expected-utility computation requires integrating over a large graph / parameter hypothesis space and is generally intractable.

ABCD makes the computation tractable using approximations including graph sampling / weighted importance sampling and greedy optimization. The paper proves submodularity for its approximate utility, giving approximation/optimization guarantees for the resulting algorithm.

It also proves a form of budgeted-batch consistency for its mutual-information utility under the paper's conditions and single-node intervention setting.

## Results

The paper evaluates synthetic DAGs and DREAM4 gene-network data.

In one synthetic setting, with 192 samples and 3 batches, the paper reports that ABCD learns most tested graphs with complete certainty. This is a scenario-specific result, not a general sample-complexity claim.

On DREAM4, it reports improvements over random selection for several target genes while noting high variability due to small sample size.

## Limitations

- Bayesian model and prior assumptions matter.
- The demonstrated causal models are not production resource-control models.
- Approximate inference is required for tractability.
- Some experiments exclude large MECs to keep enumeration manageable.
- Information gain does not include production safety or semantic harm by itself.

## Elastic relation

### ADOPT

- make experiment budget and feasibility explicit;
- support sequential evidence-driven replanning;
- allow a **targeted diagnostic objective** instead of requiring full system identification;
- distinguish information utility from execution cost.

### ADAPT

ElasticXxx needs a richer experiment cost vector:

```text
ExperimentCost {
    latency,
    compute,
    memory,
    bandwidth,
    energy,
    monetary?,
    service_disruption,
    semantic_risk,
}
```

and a separate safety contract.

### REJECT

Do not use posterior entropy as the sole production objective. A high-information experiment may be unacceptable if it violates a semantic invariant, SLO, safety limit, or disruption budget.

## Elastic proposal: decision-focused value of information

**ELASTIC PROPOSAL:** active diagnosis should optimize information only insofar as it changes a future decision.

Candidate value:

```text
ExpectedExperimentValue(e) =
    ExpectedReductionInDecisionLoss(e)
  - ExperimentCost(e)
  - ExperimentRisk(e)
```

subject to hard semantic and safety constraints.

This differs from optimizing full graph entropy when several remaining graph hypotheses all imply the same safe resource action.

## Experiment required

Construct multiple causal hypotheses that imply:

1. identical control actions;
2. different control actions;
3. one harmful intervention and one safe intervention.

Compare graph-entropy maximization against decision-focused information value and measure whether the latter spends fewer resources / causes less disruption before choosing the same correct action.
