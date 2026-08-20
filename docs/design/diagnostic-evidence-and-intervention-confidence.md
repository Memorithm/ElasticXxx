# Diagnostic Evidence and Intervention Confidence

## Status

**ELASTIC PROPOSAL / RESEARCH DESIGN NOTE.**

This note synthesizes Dhalion, StreamOps, Sinan, Sage, causal-inference methodology, and ElasticXxx's existing trusted-validation architecture. It does not claim novelty for causal graphs, counterfactual diagnosis, confidence calibration, or escalation.

## 1. A diagnosis is not a fact by default

Do not collapse:

```text
metric anomaly
correlation
prediction
causal-effect estimate
counterfactual estimate
experimentally validated cause
```

All can be useful, but they justify different levels of intervention confidence.

## 2. Candidate evidence levels

Not yet an API commitment:

```text
DiagnosticEvidenceLevel {
    Symptom,
    Association,
    Predictive,
    InterventionalModel,
    CounterfactualModel,
    Experimental,
}
```

The ordering is **not** intended as a universal total order of truth. For example, a badly misspecified counterfactual model can be less trustworthy than a well-validated predictive model for a specific operational task.

The level states the *kind of claim*, while confidence/trust states how much evidence currently supports that claim.

## 3. Root-cause estimate

Candidate shape:

```text
RootCauseEstimate {
    hypothesis,
    affected_resources,
    evidence_level,
    evidence_refs,
    model_id?,
    model_version?,
    assumptions,
    operating_domain,
    confidence,
    alternatives,
    intervention_effect?,
    counterfactual_effect?,
    freshness,
}
```

A runtime should preserve enough provenance to explain why an automatic intervention was considered legitimate.

## 4. Intervention effect is different from root-cause label

A planner often cares more about:

```text
"will intervention A improve useful progress safely?"
```

than:

```text
"what single label is the true root cause?"
```

These questions can diverge.

Multiple causes may interact, and a safe mitigation may improve the outcome without removing the deepest underlying cause.

Therefore keep separate:

```text
RootCauseEstimate
InterventionEffectEstimate
```

## 5. Counterfactuals are conditional claims

Sage demonstrates a useful pattern:

```text
P(target healthy | hypothetical intervention)
```

But such an estimate is conditional on the causal/generative model, its assumptions, training domain, and current topology.

ElasticXxx should never encode:

```text
counterfactual_result => proven_cause
```

without preserving those conditions.

## 6. Model trust and causal claim trust

A causal model can be internally valid as an artifact while no longer valid for the current runtime state.

Trust should therefore consider:

```text
model version
assumption registry
resource/topology generations
operating domain
recent predictive/interventional errors
training/validation provenance
known contradictions
```

This complements `ModelTrustState` from the controller-effectiveness note.

## 7. Automatic-action policy

A domain may require a minimum evidence policy before certain actions.

Example only:

```text
cheap reversible throttle
    may accept Predictive evidence

large live migration
    may require stronger validated effect evidence

irreversible semantic degradation
    forbidden unless SemanticContract explicitly authorizes it,
    regardless of diagnostic confidence
```

Evidence level never overrides semantic or safety constraints.

## 8. Escalation

When diagnosis confidence is inadequate, a valid control outcome is:

```text
Escalate {
    symptom,
    candidate_causes,
    missing_evidence,
    suggested_measurement_or_experiment,
}
```

This is preferable to manufacturing an automatic action from unsupported evidence.

## 9. Active diagnosis

When a safe reversible intervention can distinguish competing hypotheses, diagnosis itself can become a planning problem:

```text
choose observation / probe / intervention
that maximizes expected information gain
subject to cost and safety constraints
```

This connects naturally to experimental-design tooling, but it is an **OPEN QUESTION** for ElasticXxx runtime design.

Do not automatically experiment on production resources merely because a scientific tool can propose an intervention.

## 10. Trusted boundary

```text
Diagnostic backend
      ↓
RootCauseEstimate / InterventionEffectEstimate
      ↓
Evidence-policy check
      ↓
normal semantic + capability + consistency validation
      ↓
cost/risk/timescale planner
      ↓
actuator
```

A diagnosis backend — causal, statistical, learned, or heuristic — is never an authority-capability issuer.

## 11. Experiments

**EXPERIMENT REQUIRED.**

Construct identical performance symptoms from:

1. true CPU saturation;
2. downstream backpressure;
3. network delay;
4. workload skew;
5. a non-resource software stall.

Compare diagnostic backends at different evidence levels and measure:

- root-cause ranking accuracy;
- intervention success rate;
- harmful-action rate;
- escalation rate;
- diagnosis cost;
- time to useful mitigation;
- calibration of reported confidence.

## 12. SciRust

No new SciRust gap is implied. `scirust-causal` already provides a broad scientific stack for causal contracts, equivalence-class discovery, identification/estimation, sensitivity analysis, invariant causal prediction, structural counterfactuals, experimental design, theory revision, and claim auditing.

Those facilities can support offline/R&D experiments. ElasticXxx remains independent and must implement any selected runtime mechanism autonomously.
