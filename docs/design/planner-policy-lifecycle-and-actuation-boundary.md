# Planner / Policy Lifecycle and Actuation Boundary

**Status:** provisional architecture note derived from literature review. This document records a design hypothesis, not an established ElasticXxx theorem or novelty claim.

## 1. Motivation

The cloud-autoscaling literature reinforces two distinctions that should be explicit in ElasticXxx:

1. **recommendation is not actuation**;
2. **a planner/policy is itself a stateful component whose validity can change over time**.

Google Autopilot separates recommenders from the service/actuator/Borg mechanisms that apply resource settings. AWARE similarly separates its RL recommender from the Kubernetes mechanisms that execute horizontal and vertical scaling, and additionally manages the lifecycle of the learned policy through offline training, online training, serving and retraining.

ElasticXxx should generalize these principles beyond cloud autoscaling.

---

## 2. Proposed trust decomposition

```text
Application intent / semantic contract
                ↓
        Admissible ElasticSpace
                ↓
            Planner
                ↓
         Recommendation
                ↓
       Trusted Validator
                ↓
            Actuator
                ↓
      Transition Protocol
                ↓
            Verify
                ↓
            Commit
```

### Planner

Searches or predicts what should be done.

A planner may be:

- heuristic;
- rule-based;
- dynamic-programming based;
- optimization based;
- MPC;
- learned;
- cached/replayed;
- a composition of specialized subplanners.

The planner is **not automatically trusted to perform resource mutations**.

### Recommendation

A planner output describing a desired candidate transition or plan together with available evidence.

A recommendation is not an authority token and does not mutate the resource.

### Trusted validator

Checks properties that must hold independently of planner quality:

- capability/authority validity;
- semantic contracts;
- current generation/epoch;
- physical feasibility;
- topology constraints;
- hard resource constraints;
- transition preconditions;
- policy gates.

### Actuator

Owns the privileged mechanism that requests/applies the transition. It may interact with OS, allocator, accelerator runtime, scheduler, network, filesystem or remote resource manager.

### Transition protocol

Carries the stateful mechanics established by earlier reviews:

```text
REQUEST / PREPARE
      ↓
PENDING / READY
      ↓
SAFEPOINT / QUIESCE if required
      ↓
APPLY / TRANSFER
      ↓
VERIFY
      ↓
COMMIT / ABORT / COMPENSATE
```

---

## 3. Recommendation object

**ELASTIC PROPOSAL.** A generic recommendation may eventually need fields such as:

```rust
struct ElasticRecommendation<A> {
    action: A,
    planner_id: PlannerId,
    planner_epoch: PlannerEpoch,
    observation_epoch: ObservationEpoch,
    predicted_utility: UtilityEstimate,
    predicted_cost: CostEstimate,
    risk: RiskEstimate,
    confidence: Confidence,
    evidence: DecisionEvidence,
}
```

Exact APIs are not fixed.

Important principle:

> The recommendation records *why* a planner prefers an action; it does not prove the action remains feasible at actuation time.

---

## 4. Planner lifecycle

AWARE demonstrates that a serving learned policy can become unsuitable after workload change and should be moved back into training/recalibration. The general lesson is not RL-specific.

A provisional generic lifecycle is:

```text
UNINITIALIZED
     ↓
CALIBRATING
     ↓
VALIDATED
     ↓
SERVING
     ↓
DEGRADED / STALE
     ↓
RECALIBRATING
```

Alternative planner types may use only a subset. For example, a fixed deterministic heuristic may always be `SERVING` unless configuration/capability assumptions are invalidated.

---

## 5. What may invalidate a planner

Potential invalidation signals include:

- prediction error exceeds a bound;
- useful-progress regret rises;
- validation rejection rate rises;
- transition failures increase;
- workload distribution shifts;
- hardware/topology changes;
- resource capabilities change;
- planner calibration data becomes stale;
- uncertainty exceeds policy tolerance;
- objective/semantic contract changes.

These are candidate signals, not yet a required universal API.

---

## 6. Planner epoch

**ELASTIC PROPOSAL.** Plans/recommendations should likely carry a `PlannerEpoch` or equivalent version identifier.

This addresses a common stale-policy problem:

```text
t0: planner model v17 recommends A

t1: planner recalibrated → v18

t2: delayed recommendation from v17 arrives
```

The trusted layer may reject or revalidate recommendations produced by obsolete planner epochs.

This complements resource capability generations discussed in the static/dynamic safety boundary note:

```text
ResourceGeneration  → is the resource/capability view current?
PlannerEpoch        → is the recommending policy current?
ObservationEpoch    → was the recommendation based on current-enough evidence?
```

---

## 7. Fallback planners

AWARE uses HPA/VPA while an RL controller is not ready or is retraining. Elastic should generalize the concept but not prescribe the implementation.

Potential pattern:

```text
Primary Planner
     ↓
readiness / confidence gate
  /                \
usable             unusable
 ↓                    ↓
recommend       Fallback Planner
       \             /
        Trusted Validator
               ↓
            Actuator
```

Fallback requirements are an open policy question. A fallback may optimize less aggressively while preserving stronger conservatism.

---

## 8. Relationship to Simplex-style safety

The architecture resembles the broad Simplex safety pattern: an advanced controller can operate while a safer fallback remains available. SciRust currently contains a `SimplexMonitor` for certified controller envelopes in a different domain.

ElasticXxx should not assume that all resource safety can be represented as a numeric output interval; semantic contracts and legal transitions are more general. Nevertheless, the high-level decomposition—advanced planner plus externally enforced safe fallback/envelope—is worth investigating.

---

## 9. Planner quality versus transition legality

The architecture must preserve this distinction:

```text
planner quality:
"Is this a good action?"

validator legality:
"Is this action allowed and feasible now?"
```

A poor planner can choose a suboptimal legal action. It must not be able to manufacture an illegal privileged transition.

This separation allows experimental planners to evolve without enlarging the trusted computing base.

---

## 10. Churn and policy-switch cost

Autopilot explicitly penalizes changing resource limits and changing its selected model. Elastic should therefore consider:

```text
TransitionCost
PlannerSwitchCost
RecalibrationCost
ObservationCost
```

when deciding whether replacing a serving policy or applying a new recommendation is worthwhile.

`DO_NOTHING` and `KEEP_CURRENT_PLANNER` should remain legitimate outcomes.

---

## 11. Research questions

1. Which planner types genuinely require lifecycle state?
2. What minimal telemetry detects planner degradation without excessive overhead?
3. Should planner validity be represented as a hard state or probabilistic confidence?
4. How should fallback planners be selected and verified?
5. Can planner epochs and observation epochs make stale recommendations reliably rejectable?
6. How should cached/replayed plans interact with changed capabilities and planner versions?
7. Can a planner be replaced without interrupting the data plane?

---

## 12. Evaluation plan

A future prototype should inject:

- sudden workload shifts;
- topology/resource loss;
- intentionally biased cost models;
- stale observations;
- delayed recommendations;
- planner regression.

Compare architectures with and without lifecycle management and measure:

- invariant violations;
- rejected stale recommendations;
- SLO / useful-progress impact;
- recovery time;
- fallback duration;
- planner-switch count;
- recalibration overhead;
- transition churn.

---

## 13. Current conclusion

ElasticXxx should treat **resource state**, **planner state**, and **actuation authority** as separate concerns.

A compact working principle is:

> **Planners recommend; validators authorize; actuators perform; verifiers commit.**

This is a provisional ElasticXxx architecture direction supported by prior systems mechanisms, not a novelty claim.
