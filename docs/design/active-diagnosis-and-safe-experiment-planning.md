# Active Diagnosis and Safe Experiment Planning

## Status

**ELASTIC PROPOSAL / RESEARCH DESIGN NOTE.**

This note synthesizes Sage-style counterfactual diagnosis with active causal intervention design from He & Geng (2008), Lindgren et al. (2018), and ABCD (Agrawal et al., 2019). It does not claim novelty for active causal discovery, Bayesian experimental design, cost-aware intervention design, minimax intervention selection, or information gain.

## 1. Diagnosis and experiment planning are different operations

A diagnostic model may conclude:

```text
H1: CPU contention in service A
H2: downstream blocking in service B
H3: memory-pressure-induced queueing in service C
```

without enough evidence to decide safely among corrective actions.

The next operation may therefore be neither `Act` nor `NoAction`, but:

```text
AcquireEvidence
```

through a safe intervention or targeted measurement.

## 2. Proposed runtime branch

```text
OBSERVE
   ↓
DIAGNOSE
   ↓
Is current evidence sufficient for a safe decision?
   ├─ yes → PLAN CORRECTIVE ACTION
   └─ no  → PLAN DIAGNOSTIC EXPERIMENT
                   ↓
              VALIDATE
                   ↓
                EXECUTE
                   ↓
             SETTLE / OBSERVE
                   ↓
             UPDATE EVIDENCE
                   ↓
                DIAGNOSE
```

A diagnostic experiment is itself an Elastic transition and must obey the same semantic/safety boundary as any other action.

## 3. Diagnostic state

Candidate representation:

```text
DiagnosticState {
    hypotheses,
    evidence,
    unresolved_alternatives,
    model_version,
    assumptions,
    observation_epoch,
    confidence,
    decision_equivalence_classes,
}
```

`decision_equivalence_classes` groups hypotheses that imply the same admissible action. This matters because full causal identification may be unnecessary if all remaining hypotheses prescribe the same safe action.

## 4. Candidate experiment

```text
DiagnosticExperiment {
    id,
    target,
    intervention,
    observation_plan,
    expected_information_gain,
    worst_case_ambiguity_reduction,
    targeted_decision_gain,
    cost,
    risk,
    expected_duration,
    settling_policy,
    reversibility,
    required_attestations,
}
```

The fields are proposals, not a committed Rust API.

## 5. Experiment objectives

Do not hard-code one objective.

```text
ExperimentObjective::FullIdentification
ExperimentObjective::TargetHypothesis(...)
ExperimentObjective::DistinguishActions(...)
ExperimentObjective::ReduceDecisionRisk(...)
```

### Full identification

Useful for offline science or when future reuse of the causal model justifies the expense.

### Targeted hypothesis

Learn only a particular causal relation or graph feature.

### Distinguish actions

Collect only enough information to determine which currently legal control action should be selected.

### Reduce decision risk

Prefer evidence that most reduces expected loss from choosing the wrong action.

## 6. Information value is not action value

An intervention can be maximally informative and still be a bad runtime experiment.

Therefore:

```text
InformationUtility(e)
    !=
ProductionUtility(e)
```

Candidate production objective:

```text
ExpectedExperimentValue(e) =
    ExpectedReductionInDecisionLoss(e)
  - ExperimentExecutionCost(e)
  - ExpectedDisruption(e)
  - RiskPenalty(e)
```

subject to hard semantic/safety constraints.

`NoExperiment` remains first-class if every informative intervention is too expensive, unsafe, stale, or too slow to affect the current decision.

## 7. Experiment cost

A scalar price is insufficient for the runtime core.

Candidate vector:

```text
ExperimentCost {
    planning_time,
    execution_time,
    settling_time,
    compute,
    memory,
    bandwidth,
    energy,
    service_disruption,
    monetary?,
}
```

Risk remains separate because a rare catastrophic outcome should not automatically collapse into an additive average cost.

## 8. Budget

```text
ExperimentBudget {
    max_rounds,
    max_total_time,
    max_samples?,
    max_disruption,
    max_energy?,
    max_concurrent_interventions,
    permitted_targets,
    risk_ceiling,
}
```

Different backends may use only a subset.

## 9. Sequential versus batch experiments

He & Geng motivate the distinction explicitly.

### Batch

Choose several interventions before receiving any intermediate outcome.

Advantages:
- fewer control round trips;
- useful when experiments can run concurrently;
- predictable scheduling.

Disadvantages:
- later experiments cannot exploit evidence from earlier ones;
- may spend budget resolving ambiguity that has already disappeared.

### Sequential

Choose one intervention, update evidence, then re-plan.

Advantages:
- adaptive;
- naturally compatible with ElasticXxx receding-horizon control;
- can terminate as soon as the decision is determined.

Disadvantages:
- additional latency and planning cost;
- feedback may arrive too late for fast-changing workloads.

## 10. Minimax versus expected information

Two useful but different attitudes:

```text
Minimax:
    optimize worst-case ambiguity / decision loss

Expected-information:
    optimize expected entropy / information gain
```

Neither should become the universal planner.

A reliability-critical domain may prefer minimax. A low-risk offline experiment may prefer expected information gain.

## 11. Targeted causal learning

ABCD provides a key general lesson: the target need not be the whole graph.

For ElasticXxx this suggests:

```text
learn enough to make the resource decision
```

rather than:

```text
learn every causal edge in the system
```

This can make active diagnosis substantially cheaper if many causal hypotheses are decision-equivalent.

## 12. Trusted validation boundary

No learned or causal planner may bypass the validator.

```text
causal/diagnostic planner
        ↓
Candidate DiagnosticExperiment
        ↓
trusted semantic + safety validation
        ↓
actuator
```

The planner can rank an experiment as highly informative; the validator can still reject it.

Examples of prohibited probes unless explicitly allowed:

- corrupting live state;
- silently changing numerical precision under an Exact contract;
- exceeding a thermal/power safety limit;
- dropping required replicas;
- inducing an unbounded service outage;
- bypassing ownership/capability rules.

## 13. Dual-use production actions

A corrective action may also generate diagnostic evidence.

Example:

```text
reduce one worker pool by 5%
```

could both mitigate resource contention and reveal whether a suspected dependency is causal.

But do not retroactively call every control action an experiment.

Candidate distinction:

```text
CorrectiveAction
DiagnosticExperiment
DualPurposeIntervention
```

with explicit intended evidence collection for the latter.

## 14. Evidence update

After an intervention:

```text
ExperimentResult {
    experiment_id,
    intervention_epoch,
    observed_outcome,
    measurement_quality,
    environment_changes,
    settling_complete,
    model_assumptions_still_valid,
}
```

Evidence generated while the environment materially changed may be inconclusive rather than positive/negative.

## 15. Relationship to controller outcome memory

`ActionOutcomeRecord` asks whether an action achieved its predicted operational effect.

`ExperimentResult` asks what new evidence the intervention provides about competing hypotheses.

One physical intervention may populate both records, but the semantics differ.

## 16. Planner architecture

Recommended multi-speed structure:

```text
Fast Path
    no active causal experiment
    use established local policy / cached plan

Diagnostic Planning Path
    safe low-cost probes
    sequential ambiguity reduction

Research Path
    richer causal discovery, counterfactual analysis,
    Bayesian experimental design, offline simulation
```

The common case must not require causal structure discovery.

## 17. Research hypotheses

### Candidate H9 — Decision-Focused Active Diagnosis

A resource runtime can reduce diagnostic cost and time by selecting safe interventions according to their expected effect on the **control decision**, rather than requiring full causal-system identification.

**Status: HYPOTHESIS / EXPERIMENT REQUIRED.**

### Candidate H10 — Constraint-Preserving Diagnostic Experimentation

Active diagnostic interventions can be expressed as ordinary legal resource transitions and filtered by the same type/semantic/safety contracts as corrective actions.

**Status: DESIGN HYPOTHESIS / EXPERIMENT REQUIRED.**

## 18. Evaluation plan

Build a synthetic resource graph with controlled ground-truth causal mechanisms and several ambiguous symptoms.

Compare:

1. passive diagnosis only;
2. random legal probes;
3. minimax structural probes;
4. entropy / information-gain probes;
5. decision-focused value-of-information probes.

Measure:

- probability of selecting the correct corrective action;
- time-to-decision;
- experiment count;
- total transition cost;
- disruption caused by probes;
- planner overhead;
- residual causal uncertainty;
- number of cases where full graph uncertainty remains but the correct action is already determined.
