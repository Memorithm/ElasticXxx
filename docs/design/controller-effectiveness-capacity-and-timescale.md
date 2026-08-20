# Controller Effectiveness, Effective Capacity, and Adaptation Timescales

## Status

**ELASTIC PROPOSAL / RESEARCH DESIGN NOTE.**

This note synthesizes Dhalion (PVLDB 2017), Denning & Buzen (ACM Computing Surveys 1978), DS2 (OSDI 2018), Sinan (ASPLOS 2021), StreamOps, Pollux, FlexMem, Beyond Hotness, predictive-control work, and ElasticXxx's existing transition-cost model. It does not claim novelty for closed-loop policy evaluation, capacity estimation, operational performance laws, model-trust fallback, or timescale separation.

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

## 5. Operational demand is another distinct quantity

Denning & Buzen's operational analysis introduces directly measurable/testable relationships such as:

```text
U_i = X_i S_i
N_i = X_i R_i
X_i = V_i X_0
D_i = V_i S_i
```

where service demand `D_i` measures resource work per system-level completion.

For ElasticXxx, distinguish:

```text
ObservedDelivery
EffectiveCapacityEstimate
ServiceDemandEstimate
```

A highly utilized resource is not necessarily the limiting resource, and improving a non-bottleneck component may yield little end-to-end benefit.

Operational identities used to reconstruct the current system are also different from transition-effect models used to predict a changed configuration:

```text
MeasurementIdentity
CurrentStateEstimate
TransitionEffectModel
```

The last object requires assumptions about what remains invariant after the proposed transition.

## 6. Candidate estimate contract

```text
Estimate<T> {
    value,
    context,
    observation_window,
    generation_or_epoch,
    assumptions,
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

## 7. Capability, availability, observed delivery and demand

Distinguish:

```text
Capability
    what the resource can support under declared conditions

AvailableCapacity
    capability currently allocatable after reservations/constraints

ObservedDelivery
    what was actually delivered during the measurement interval

ServiceDemand
    resource work required per useful system completion
```

These can differ substantially.

Example:

```text
GPU kernel capability      200 units/s
available under power cap  150 units/s
observed under starvation   60 units/s
service demand               4 ms/useful item
```

A planner that treats `60` as intrinsic capability may over-scale or migrate unnecessarily.

## 8. Timescale viability

A legal transition is not automatically worth executing.

Separate approximate latencies:

```text
T_control =
    T_observation
  + T_diagnosis
  + T_planning
  + T_validation

T_effect =
    T_control
  + T_transition
  + T_recovery
  + T_settling
```

Sinan independently demonstrates why `T_recovery` matters: after insufficient capacity creates backlog, restoring resources does not remove the accumulated queue immediately.

If the motivating condition is expected to persist for `T_env`, adaptation is suspect when:

```text
T_effect >= T_env
```

In uncertain environments use a distribution / probability rather than one exact scalar.

Candidate viability quantity:

```text
P(condition persists until useful effect)
```

and include it in expected benefit/risk.

## 9. Multi-timescale control and horizon-specific forecasts

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

Sinan adds a further lesson: the prediction target itself may depend on horizon. In that system, detailed near-term latency prediction and longer-horizon violation probability are separate models because precise long-range latency prediction degraded.

Candidate generic structure:

```text
ForecastBundle {
    near_term_effect,
    longer_horizon_risk,
    horizons,
    model_trust,
}
```

A global planner may publish bounded local policies for fast-path execution rather than participate in every action.

## 10. Settling and recovery semantics

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

## 11. Model trust is runtime state

Sinan tracks prediction failures and includes a conservative fallback when predictions miss QoS violations. This motivates a general model-trust lifecycle.

Candidate:

```text
ModelTrustState {
    model_version,
    validation_epoch,
    operating_domain,
    recent_error,
    miss_rate,
    confidence,
    fallback_policy,
}
```

Training/validation provenance may include workload distribution, explored action region, hardware/runtime context and objective definition.

A model outside its validated operating domain should not retain the same trust merely because its artifact bytes are unchanged.

The trusted validator remains authoritative regardless of model trust.

## 12. Causal caution

A before/after performance difference does not prove the action caused it.

At minimum retain:

- concurrent environment changes;
- competing actions;
- workload phase changes;
- uncertainty/confidence.

Likewise, a model feature-importance score or queue hotspot is a diagnosis aid, not proof of causal responsibility.

For high-value policies, controlled experiments or causal/statistical models may be warranted. The runtime core should not pretend to solve causal identification generically.

## 13. Planner objective

Extend the earlier risk-adjusted formulation conceptually:

```text
ExpectedNetValue(plan) =
    P(effect arrives in time)
  * E[UsefulProgressBenefit]
  - TransitionCost
  - RecoveryCost
  - ControlCost
  - ResourceCost
  - RiskCost
```

subject to semantic and safety invariants.

`DO NOTHING` remains a first-class candidate.

## 14. Model update boundary

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

## 15. Experiments

### A. Observed versus effective capacity

Inject waiting/contention while keeping intrinsic service cost constant. Compare raw-throughput and corrected-capacity planners.

### B. Bottleneck versus utilization

Create a multi-resource path where the highest-utilization component is not the component whose service demand limits useful throughput. Compare utilization-driven and demand/bottleneck-aware policies.

### C. Timescale mismatch

Generate transient pressure whose lifetime is shorter than a large migration. Confirm that a cost/timescale-aware controller selects `NoAction` or a faster alternative.

### D. Outcome memory

Create a diagnosis/action pair that succeeds in one context and fails in another. Compare permanent blacklist, no memory, and contextual memory.

### E. Settling/recovery

Measure false policy updates when evaluating immediately after transition versus a domain-appropriate settling/recovery condition.

### F. Model trust

Induce workload shift outside a model's validation domain. Compare fixed trust against validation-epoch/domain-aware trust with a conservative fallback.

## 16. SciRust

The runtime concepts here belong to ElasticXxx.

SciRust already contains an M/M/1 discrete-event queue simulator. The Denning–Buzen/DS2 review identified a narrower missing R&D layer for generic operational performance identities and bottleneck analysis; SciRust PR #1291 currently adds that minimal layer and is undergoing CI validation.

Broader queueing networks, fitting, uncertainty and response-time modelling remain **INVESTIGATE**, not assumed missing requirements.
