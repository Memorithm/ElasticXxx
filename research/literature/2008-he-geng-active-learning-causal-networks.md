# He & Geng 2008 — Active Learning of Causal Networks with Intervention Experiments and Optimal Designs

## Classification

**ADAPT** for ElasticXxx diagnostic planning.

## Evidence status

- **SOURCE-DERIVED** unless explicitly marked otherwise.
- Primary source: Yang-Bo He and Zhi Geng, *Active Learning of Causal Networks with Intervention Experiments and Optimal Designs*, JMLR 9 (2008), 2523–2547.
- PDF visual inspection succeeded for the sequential-design / maximum-entropy page (paper page 2535 / PDF page 13).

## Problem

Observational data generally identify a causal DAG only up to a Markov equivalence class (MEC). The paper asks which external interventions should be performed to orient the remaining undirected edges efficiently.

## State of knowledge

The observationally learned MEC is represented by an essential graph / chain graph. Directed edges are already compelled; undirected edges mark unresolved orientations.

A key structural result is that unresolved orientations can be treated locally inside chain components: if no cycle or illegal v-structure is created within a chain component, orientation there does not create one elsewhere in the graph.

## Intervention models

The paper distinguishes:

1. **Randomized intervention** — manipulation disconnects the target variable from its parents.
2. **Quasi-experiment** — the target distribution is changed but can remain dependent on its parents.

The distinction matters because the statistical test needed to infer edge direction differs.

## Batch design

A batch design selects, before seeing new outcomes, a smallest sufficient set of manipulated variables intended to orient all unresolved edges.

This is useful when feedback between experiments is unavailable or costly.

## Sequential design

The sequential design chooses one intervention, observes the resulting subclass of the current MEC, then chooses the next intervention from the updated state of knowledge.

Two criteria are studied:

### Minimax

Choose the target whose worst possible post-intervention subclass is as small as possible.

Interpretation: conservative ambiguity reduction.

### Maximum entropy

Choose the target that maximizes the entropy of the partition of candidate DAGs induced by possible intervention outcomes. In the paper, if `l_i` is the number of DAGs in outcome subclass `i` and `L = sum_i l_i`, then:

```text
H_V = - sum_i (l_i / L) log(l_i / L)
```

The goal is to make outcome subclasses both small and balanced, thereby reducing uncertainty.

## Results

The paper reports that its sequential intervention designs are more efficient than the batch design in its simulations, and that minimax / maximum-entropy designs outperform random sequential selection. The exact advantage is experiment-specific and must not be generalized to arbitrary production systems.

## Limitations / assumptions

- causal sufficiency / no latent variables in the studied formulation;
- faithfulness;
- correct observational equivalence class;
- perfect enough randomized or quasi-interventions for the specified tests;
- exact criteria can require enumeration of candidate DAGs, with complexity depending on the chain-component size and unresolved-edge count.

## Elastic relation

### ADOPT

- represent diagnostic uncertainty explicitly rather than collapsing immediately to one root cause;
- sequentially re-plan after receiving intervention evidence;
- make `NoExperiment` a valid result when no safe/feasible informative intervention exists.

### ADAPT

ElasticXxx cannot assume arbitrary perfect interventions. A production intervention can have latency, semantic effects, risk, cost, and delayed settling. Therefore an informative experiment must also be a legal resource transition.

Proposed separation:

```text
DiagnosticState
    -> CandidateExperiment
    -> Safety / semantic validation
    -> Execute intervention
    -> Settle / observe
    -> EvidenceUpdate
    -> DiagnosticState'
```

### REJECT

Do not make "orient the entire causal graph" the universal Elastic objective. Runtime control often only needs enough evidence to choose safely among current actions.

## Elastic proposal

Introduce a decision-focused experiment objective:

```text
ExperimentObjective {
    FullIdentification,
    TargetHypothesis(...),
    DistinguishActions(...),
    ReduceDecisionRisk(...),
}
```

**ELASTIC PROPOSAL:** the default runtime use should favor the smallest amount of information needed to discriminate among materially different safe decisions, rather than full causal reconstruction.

## Experiment required

Compare on a controlled resource graph:

1. random safe probes;
2. minimax ambiguity-reduction probes;
3. entropy / expected-information probes;
4. no active probing.

Measure diagnostic error, intervention count, total experiment cost, time-to-correct-action, and harm caused by the experiments themselves.
