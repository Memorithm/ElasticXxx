# Controller Effectiveness, Effective Capacity, and Adaptation Timescales

## Status

**ELASTIC PROPOSAL / RESEARCH DESIGN NOTE.**

This note synthesizes Dhalion (PVLDB 2017), DS2 (OSDI 2018), StreamOps, Pollux, FlexMem, Beyond Hotness, predictive-control work, and ElasticXxx's existing transition-cost model. It does not claim novelty for closed-loop policy evaluation, capacity estimation, or timescale separation.

## 1. Three different questions

Do not collapse:

```text
Did the actuator command complete?
Did the target resource state change as requested?
Did the system improve for the reason predicted?
```

These correspond to different verification layers.

Candidate vocabulary:

```text
ActuationResult
TransitionVerification
ControlOutcome
```

A syscall/API can succeed while the resource transition is incomplete. A transition can complete while application performance gets worse. A performance improvement can occur for reasons unrelated to the action.

## 2. Extended runtime loop

Earlier ElasticXxx work used:

```text
OBSERVE → FORECAST → PLAN → VALIDATE → ACT → VERIFY → COMMIT
```

Dhalion motivates a second, slower feedback loop after commit:

```text
OBSERVE
  ↓
DIAGNOSE / FORECAST
  ↓
PLAN
  ↓
VALIDATE
  ↓
ACT
  ↓
VERIFY TRANSITION
  ↓
COMMIT
  ↓
SETTLE / OBSERVE EFFECT
  ↓
EVALUATE CONTROL OUTCOME
  ↓
UPDATE MODEL / POLICY MEMORY
```

Do not merge `VERIFY TRANSITION` with `EVALUATE CONTROL OUTCOME`.

## 3. Contextual action-outcome memory

Dhalion stores action history and can blacklist diagnosis/action pairs whose interventions repeatedly fail.

ElasticXxx should retain the general principle but avoid a context-free permanent blacklist.

Candidate record:

```text
ActionOutcomeRecord {
    recommendation_id,
    planner_epoch,
    observation_epoch,
    resource_generations,
    diagnosis_context,
    state_before,
    action,
    predicted_effect,
    transition_result,
    state_after,
    measured_effect,
    settling_interval,
    confidence,
    outcome_class,
}
```

Possible outcome classes:

```text
Beneficial
Neutral
Harmful
Inconclusive
EnvironmentChanged
MeasurementInvalid
```

A failed expectation can update:

- action-effect models;
- diagnosis confidence;
- cooldown/hysteresis;
- planner risk;
- temporary contextual bans.

It must not silently weaken correctness invariants.

## 4. Observation is not effective capacity

DS2 demonstrates that observed throughput can be below an operator's sustainable service rate because observed time includes waiting.

Generalize cautiously:

```text
ObservedPerformance
       ↓
Capacity / Service Model
       ↓
EffectiveCapacityEstimate
```

Potential causes of divergence include:

```text
input starvation
output blocking
contention
throttling
backpressure
queueing
placement
workload mix
thermal/power limits
```

An `ElasticObservation` should therefore not automatically become a capability.

## 5. Candidate estimate contract

```text
Estimate<T> {
    value,
    context,
    observation_window,
    generation_or_epoch,
    confidence,
    uncertainty,
    estimation_cost,
}
```

For effective capacity specifically:

```text
EffectiveCapacityEstimate<ResourceDomain>
```

may be derived by a domain adapter/model.

The generic core should not define one formula for every resource.

## 6. Capability, availability, and observed delivery

Distinguish:

```text
Capability
    what the resource can support under declared conditions

AvailableCapacity
    capability currently allocatable after reservations/constraints

ObservedDelivery
    what was actually delivered during the measurement interval
```

These can differ substantially.

Example:

```text
GPU kernel capability      200 units/s
available under power cap  150 units/s
observed under starvation   60 units/s
```

A planner that treats `60` as intrinsic capability may over-scale or migrate unnecessarily.

## 7. Timescale viability

A legal transition is not automatically worth executing.

Define approximate control latency:

```text
T_control =
    T_observation
  + T_diagnosis
  + T_planning
  + T_validation
  + T_transition
  + T_settling
```

If the motivating condition is expected to persist for `T_env`, then adaptation is suspect when:

```text
T_control >= T_env
```

In uncertain environments use a distribution / probability rather than one exact scalar.

Candidate viability quantity:

```text
P(condition persists until useful effect)
```

and include it in expected benefit/risk.

## 8. Multi-timescale control

Different resources require different control periods.

```text
fast path
    queue depth / local scheduling / lightweight routing

medium path
    cache movement / worker resize / batch adaptation

slow path
    large state migration / distributed reconfiguration / model retraining
```

Do not force all actions through the same observation window, settling time or planner.

A global planner may publish bounded local policies for fast-path execution rather than participate in every action.

## 9. Settling semantics

After a transition, metrics may be temporarily misleading because of:

- cold caches;
- queue drain/fill;
- JIT/warmup;
- migration overlap;
- delayed load redistribution;
- network reconvergence;
- state restoration.

Therefore each intervention/domain may define:

```text
SettlingPolicy {
    minimum_time?,
    observation_count?,
    frontier_condition?,
    stability_condition?,
    timeout,
}
```

A fixed sleep is only one implementation.

## 10. Causal caution

A before/after performance difference does not prove the action caused it.

At minimum retain:

- concurrent environment changes;
- competing actions;
- workload phase changes;
- uncertainty/confidence.

For high-value policies, controlled experiments or causal/statistical models may be warranted. The runtime core should not pretend to solve causal identification generically.

## 11. Planner objective

Extend the earlier risk-adjusted formulation conceptually:

```text
ExpectedNetValue(plan) =
    P(effect arrives in time)
  * E[UsefulProgressBenefit]
  - TransitionCost
  - ControlCost
  - ResourceCost
  - RiskCost
```

subject to semantic and safety invariants.

`DO NOTHING` remains a first-class candidate.

## 12. Model update boundary

A planner/model may learn from `ActionOutcomeRecord`, but learned state never bypasses validation.

```text
measured outcome
      ↓
model/policy update
      ↓
future recommendation
      ↓
trusted validator
      ↓
actuator
```

This applies whether the model is analytical, statistical, learned, or heuristic.

## 13. Experiments

### A. Observed versus effective capacity

Inject waiting/contention while keeping intrinsic service cost constant. Compare raw-throughput and corrected-capacity planners.

### B. Timescale mismatch

Generate transient pressure whose lifetime is shorter than a large migration. Confirm that a cost/timescale-aware controller selects `NoAction` or a faster alternative.

### C. Outcome memory

Create a diagnosis/action pair that succeeds in one context and fails in another. Compare permanent blacklist, no memory, and contextual memory.

### D. Settling

Measure false policy updates when evaluating immediately after transition versus a domain-appropriate settling condition.

## 14. SciRust

The runtime concepts here belong to ElasticXxx.

A separate possible scientific gap is generic queueing/service-capacity/performance modelling. Repository search did not identify such a SciRust family during the DS2 review. Keep it **INVESTIGATE** until broader scientific literature and an independent domain justify a reusable API.
