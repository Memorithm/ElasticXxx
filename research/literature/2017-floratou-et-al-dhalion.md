# Dhalion: Self-Regulating Stream Processing in Heron

## Status

**SOURCE-DERIVED MECHANISM REVIEW + ELASTIC RELATION.**

Reference: Avrilia Floratou, Ashvin Agrawal, Bill Graham, Sriram Rao, Karthik Ramasamy, *Dhalion: Self-Regulating Stream Processing in Heron*, PVLDB 10(12), 2017.

Primary source: https://www.vldb.org/pvldb/vol10/p1825-floratou.pdf

PDF screenshot inspection was attempted during this review. The text source was accessible, but the screenshot service returned a cache miss for this PDF; claims below are grounded in the paper text rather than a successful visual inspection.

## 1. Problem

Dhalion targets long-running stream-processing applications whose configuration must be adjusted while workloads, resource availability, and machine/software performance vary.

The paper frames self-regulation through three broad capabilities:

- self-tuning;
- self-stabilizing;
- self-healing.

Its concrete Heron policies tune topology resources while trying to preserve throughput objectives and diagnose degraded execution.

## 2. Policy pipeline

Dhalion decomposes a policy into three major phases:

```text
Symptom Detection
    ↓
Diagnosis Generation
    ↓
Resolution
```

Symptom detectors observe metrics. Diagnosers map sets of symptoms to possible root causes. Resolvers select and execute interventions.

**SOURCE-DERIVED:** Dhalion predates StreamOps' later production use of the same high-level `detect → diagnose → resolve` pattern. ElasticXxx must not attribute that separation to StreamOps alone or claim it as novel.

## 3. Observation

Representative observations include:

- pending packets / queue state;
- backpressure;
- per-instance processing rate;
- skew among instances.

The Dynamic Resource Provisioning policy evaluates metrics over a 300-second interval in the configuration described by the paper.

Dhalion's detection is therefore intentionally smoothed and relatively slow compared with later controllers such as DS2.

## 4. Symptom is not diagnosis

The paper explicitly distinguishes visible symptoms such as backpressure from causes including:

- resource under-provisioning;
- resource over-provisioning;
- data skew;
- slow instances / degraded hosts.

Multiple diagnoses may explain the same symptoms.

**Elastic relation — ADOPT:**

```text
Observation
   ↓
Symptom
   ↓
RootCauseEstimate
   ↓
Candidate intervention
```

Pressure/symptom metrics must not directly authorize an action.

## 5. Action Log

Dhalion records actions together with their time and the diagnosis that triggered them.

The Action Log is used for:

- debugging;
- reporting;
- evaluating whether a policy intervention was beneficial.

This is direct prior art for retaining causal context around runtime actions.

**Elastic relation — ADOPT / GENERALIZE:** every committed control action should be associated with its decision context, predicted outcome, measured outcome, and relevant state/generation identifiers.

## 6. Post-action evaluation and blacklist

This is Dhalion's most important mechanism for ElasticXxx.

After an action, the Health Manager waits for the topology to reach a new steady state and evaluates whether the action improved the policy objective. The system tracks, per diagnosis/action pair, how often the action was not beneficial. Once the ineffective-action ratio exceeds a configurable threshold, that pair can be blacklisted.

A blacklisted Resolver is not invoked again for a similar diagnosis; a policy may choose an alternative resolver or wait for a later invocation.

Conceptually:

```text
DIAGNOSE
   ↓
ACT
   ↓
WAIT FOR SETTLING
   ↓
MEASURE OUTCOME
   ↓
COMPARE WITH EXPECTED EFFECT
   ↓
UPDATE ACTION MEMORY
```

### Elastic relation — ADOPT the closed-loop principle

An actuation result (`system call succeeded`) is not the same as a successful control outcome (`system improved as intended`).

Candidate distinction:

```text
ActuationResult
ControlOutcome
```

### Elastic relation — ADAPT the blacklist

A permanent or coarse diagnosis/action blacklist is too blunt for a general runtime. An action may fail because its context, resource generation, model version, workload phase, topology, or transition cost changed.

Prefer a contextual outcome record such as:

```text
ActionOutcomeRecord {
    recommendation,
    diagnosis_context,
    state_before,
    action,
    predicted_effect,
    state_after,
    measured_effect,
    settling_interval,
    outcome,
}
```

and derive cooldown, down-weighting, blacklisting, or model updates from validated context.

## 7. Action failure is information

A wrong or ineffective action is not merely a runtime error. It reveals that one or more assumptions were wrong:

- diagnosis may have been wrong;
- action-effect model may have been wrong;
- transition may have been too weak/strong;
- environment may have changed before measurement;
- settling interval may have been inadequate.

This supports a general feedback loop:

```text
OBSERVE
FORECAST / DIAGNOSE
PLAN
VALIDATE
ACT
VERIFY ACTUATION
OBSERVE SETTLED EFFECT
EVALUATE CONTROL OUTCOME
UPDATE MODEL/POLICY MEMORY
```

The final two stages are an **ELASTIC PROPOSAL** generalized from Dhalion.

## 8. Safety and limitations

Dhalion is modular and extensible but its demonstrated policies are Heron/stream specific and often depend on backpressure.

Its diagnosers can miss scenarios when a topology is considered healthy even though local skew or slowness exists. The paper also describes cases where a threshold leads to an initially incorrect diagnosis and the blacklist mechanism later helps the policy reach a better diagnosis/action path.

This is strong evidence that diagnosis should carry uncertainty rather than be treated as an invariant.

## 9. Relation to later DS2

DS2 recreates Dhalion's word-count autoscaling benchmark and reports that Dhalion performs six scale-up decisions, taking about 2000 seconds to settle at 22 FlatMap and 30 Count instances, whereas DS2 predicts the minimal 10/20 configuration after one 60-second measurement interval in that experiment.

These are results from the DS2 evaluation of Dhalion, not from the original Dhalion paper itself; keep the attribution explicit.

## 10. Elastic classification

### ADOPT

- symptom/diagnosis/action separation;
- action log with decision context;
- post-action outcome evaluation;
- settling period before judging an intervention;
- ineffective-action memory;
- diagnosis as fallible runtime information.

### ADAPT

- coarse diagnosis/action blacklist → contextual action-outcome memory;
- fixed/slow policy intervals → multi-rate triggering;
- Heron-specific symptoms/resolvers → typed resource/domain observations and interventions.

### REJECT from generic core

- Heron-specific queue/backpressure metrics;
- assuming every useful controller can be expressed as backpressure repair;
- assuming one fixed settling interval is appropriate for all resources.

## 11. Experiment

**EXPERIMENT REQUIRED.** Construct a workload where the same pressure symptom has two causes and where one intervention is beneficial in one context but harmful in the other.

Compare:

1. pressure→action rule;
2. diagnosis-aware rule;
3. diagnosis-aware rule + coarse blacklist;
4. contextual action-outcome memory.

Measure wrong-action rate, recovery time, oscillation, repeated ineffective actions, resource cost, and control overhead.

## 12. SciRust

Dhalion itself does not justify a new SciRust primitive. Its action log/blacklist is primarily runtime policy state.

The subsequent DS2 review reveals a possible broader scientific need around service-capacity / queueing-performance models; that is tracked separately and remains **INVESTIGATE**, not implemented from Dhalion alone.
