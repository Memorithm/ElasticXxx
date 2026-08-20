# AWARE: Automate Workload Autoscaling with Reinforcement Learning in Production Cloud Systems

**Paper:** Haoran Qiu, Weichao Mao, Chen Wang, Hubertus Franke, Alaa Youssef, Zbigniew T. Kalbarczyk, Tamer Başar, Ravishankar K. Iyer. *AWARE: Automate Workload Autoscaling with Reinforcement Learning in Production Cloud Systems*. USENIX ATC 2023.

**Primary sources:**

- https://www.usenix.org/system/files/atc23-qiu-haoran.pdf
- accessible archival copy used for detailed reading: https://par.nsf.gov/servlets/purl/10465144

**Review status:** mechanism-level review complete.

---

## 1. Problem

**SOURCE-DERIVED.** AWARE addresses a practical weakness of reinforcement-learning-based cloud resource controllers: a policy that works after convergence may be unsafe or inefficient during early training, may degrade when the application/workload changes, and may be expensive to adapt to a new application/environment pair.

The paper uses multidimensional Kubernetes workload autoscaling as the concrete system task.

---

## 2. Resource model and action space

**SOURCE-DERIVED.** Each RL controller manages a Kubernetes Deployment. The state includes resource limits, resource utilization, SLO preservation and observed load changes. Actions adjust:

- CPU limits;
- memory limits;
- replica count.

AWARE's MPA (multi-dimensional Pod autoscaler) exposes a unified interface through which an RL policy can issue horizontal and vertical scaling recommendations.

**ELASTIC RELATION.** This is another clear example that quantity, per-instance capacity and concurrency can be coupled decision variables.

---

## 3. Reward / objective

**SOURCE-DERIVED.** The controller is designed to combine application SLO preservation and resource utilization. SLOs may concern latency or throughput. The reward function weights SLO preservation and resource utilization.

**ELASTIC RELATION — ADAPT.** Elastic should not collapse hard semantic invariants into a learned scalar reward. Objectives can be weighted, but invariants / hard constraints belong outside the reward and must be validated independently.

This is a crucial distinction between a generic safe Elastic planner and the specific RL formulation used by AWARE/FIRM.

---

## 4. Why naive online RL is unsafe

**SOURCE-DERIVED.** The paper characterizes early-stage RL training and finds severe degradation relative to a rule-based HPA/VPA baseline. In the first 100 training episodes of the characterized setup, RL caused 56.1× more SLO violations than the rule-based approach. The authors attribute the problem to trial-and-error exploration and under-provisioning / oscillating decisions during early training.

**ELASTIC RELATION — ADOPT PRINCIPLE.** Exploration in a real resource system must be constrained by an external safety envelope or executed off-path/offline when the consequence is unacceptable.

A learned planner must not gain authority simply because it optimizes a reward.

---

## 5. Bootstrapping and fallback controller

**SOURCE-DERIVED.** AWARE introduces an RL bootstrapper. During offline bootstrapping, Kubernetes HPA and VPA remain the effective controllers while trajectories are collected / reused to train the RL agent. HPA/VPA can also remain fallback controllers for high-stakes workloads during retraining.

The paper explicitly intercepts the RL action path so that fallback actions rather than the unready RL policy reach the actuator.

**ELASTIC RELATION — ADOPT / GENERALIZE.** Elastic should support a conceptually similar safety pattern:

```text
candidate planner
      ↓
policy readiness / confidence gate
      ↓
trusted validator
   /        \
valid      invalid/unready
  ↓             ↓
actuator      fallback planner
```

The fallback need not be rule-based; the important property is that the system retains a known-safe operational policy when a more sophisticated planner is not trustworthy.

---

## 6. Planner lifecycle

**SOURCE-DERIVED.** AWARE models the RL agent lifecycle using explicit stages:

- `INITIALIZED`;
- `OFFLINE` training;
- `ONLINE` training;
- `SERVING`.

Transitions between these stages depend on recent reward statistics (mean and variance) and user-configured thresholds. A serving policy is moved back to online training when measured reward quality drops or variability grows too large.

**KEY ELASTIC LESSON.** A planner/policy has a **validity lifecycle**. A policy that was previously accepted may become stale when:

- workload characteristics change;
- hardware/topology changes;
- observed prediction error increases;
- reward/utility degrades;
- uncertainty rises.

**ELASTIC PROPOSAL.** Planner state should be separated from resource state. A possible generic lifecycle is:

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

This is not claimed as novel; it is a generalization of mechanisms such as AWARE's controller lifecycle.

---

## 7. Incremental retraining

**SOURCE-DERIVED.** AWARE continuously monitors recent rewards. If performance or variability crosses configured thresholds, it switches the policy from serving back to training. In the evaluated instability scenarios, this retraining mechanism improves utilization and reduces SLO violations versus an RL controller with no retraining.

**ELASTIC RELATION — ADOPT PRINCIPLE.** The system should monitor **prediction/planner error**, not only managed resource pressure. A planner can itself degrade and needs telemetry.

Potential Elastic telemetry:

```text
planner_prediction_error
planner_regret
validation_rejection_rate
transition_failure_rate
utility_drift
observation_distribution_shift
```

These are **ELASTIC PROPOSALS**.

---

## 8. Fast adaptation with meta-learning

**SOURCE-DERIVED.** AWARE uses a meta-learner to produce workload embeddings that encode spatial resource-performance characteristics and temporal load characteristics. The base learner is then adapted to a new application/environment pair with limited exposure.

The evaluated AWARE configuration adapts 5.5× faster than the transfer-learning baseline and uses fewer CPU cycles in adaptation. The paper is explicit that transfer depends on the new environment coming from the same distribution or sharing similar patterns with training environments.

**ELASTIC RELATION — INVESTIGATE.** Planner specialization to workload/hardware classes may be valuable, but out-of-distribution behavior must remain visible and conservative. Learned generalization cannot replace capability/invariant validation.

---

## 9. Results

**SOURCE-DERIVED.** Reported results include:

- 5.5× faster adaptation than the compared transfer-learning approach;
- during adaptation, 7.1× fewer SLO violations than the enhanced transfer-learning comparison reported in the paper;
- with continuous monitoring/retraining, 9.6% higher CPU utilization and 14.8% higher memory utilization, with 3.1× fewer SLO violations than an RL agent without retraining in the evaluated instability scenarios;
- bootstrapping yields 47.5% and 39.2% higher CPU and memory utilization respectively and 16.9× fewer SLO violations than the RL agent without bootstrapping before convergence;
- serving performance remains within 3.6% average reward of the converged-policy comparison under the selected retraining threshold.

These are workload- and setup-specific experimental results, not generic guarantees for RL autoscaling.

---

## 10. Recommendation / actuation separation

**SOURCE-DERIVED.** AWARE's MPA design explicitly separates scaling recommendation from Kubernetes actuation. A recommender generates horizontal/vertical settings; Kubernetes mechanisms apply them.

**ELASTIC RELATION — ADOPT.** This reinforces the Autopilot lesson:

```text
Planner != Actuator
```

A planner may be replaced, retrained, approximated or disabled without changing the trusted transition mechanism.

---

## 11. Safety interpretation

**SOURCE-DERIVED.** AWARE improves production robustness by bootstrapping, fallbacks, monitoring and retraining. It does not provide a general proof that an arbitrary RL action preserves semantic/resource invariants.

**ELASTIC INFERENCE.** A learned planner should therefore sit **below** an invariant/capability validator in the trust hierarchy:

```text
learned / heuristic / optimal planner
               ↓
       candidate plan
               ↓
       trusted validator
               ↓
           actuator
```

This allows research planners to evolve without expanding the trusted computing base.

---

## 12. Elastic disposition

| AWARE mechanism | ElasticXxx disposition |
|---|---|
| Joint horizontal + vertical control | **ADOPT / GENERALIZE** |
| Learned sequential policy | **INVESTIGATE as planner backend** |
| SLO + utilization reward | **ADAPT: objectives separate from hard invariants** |
| Offline bootstrapping | **ADOPT principle** |
| Safe fallback controller | **ADOPT / GENERALIZE** |
| Planner lifecycle states | **ADOPT / GENERALIZE** |
| Continuous performance monitoring | **ADOPT** |
| Retraining trigger | **ADAPT to generic planner invalidation/recalibration** |
| Meta-learning workload embedding | **INVESTIGATE** |
| Direct RL exploration in high-stakes runtime | **REJECT unless safety-gated** |
| Recommendation separated from actuation | **ADOPT** |

---

## 13. New design questions for ElasticXxx

### Planner validity

Should every nontrivial planner expose a validity state and calibration epoch?

### Planner confidence

Can planner uncertainty / prediction error be represented generically enough to gate execution?

### Safe fallback

Should `ElasticPlanner` optionally declare a deterministic fallback, or should fallback selection belong to a higher policy layer?

### Policy generation

If an expensive research planner produces a cheap serving policy, how is that derived policy versioned, validated and invalidated?

These are **OPEN QUESTIONS**.

---

## 14. Experiment suggested for ElasticXxx

**EXPERIMENT REQUIRED.** Evaluate a controller-switching architecture under abrupt workload drift:

1. fixed threshold planner;
2. learned planner with no lifecycle management;
3. learned planner with drift detection and recalibration;
4. learned planner + deterministic fallback + trusted action validator.

Measure:

- UsefulProgress;
- SLO/invariant violations;
- time-to-recover from distribution shift;
- planner invalidations;
- fallback occupancy time;
- planning / retraining CPU cost;
- validation rejection rate;
- transition churn.

---

## 15. Current conclusion

AWARE's strongest lesson for ElasticXxx is not that reinforcement learning should manage resources. It is that **a sophisticated adaptive planner is itself a managed component**: it has training/calibration state, validity, degradation modes, monitoring requirements and a fallback path. ElasticXxx should generalize this lifecycle while keeping planner logic outside the narrow trusted actuation and invariant-enforcement boundary.
