# Online Adaptive Learning for Runtime Resource Management of Heterogeneous SoCs

**Paper:** Sumit K. Mandal, Umit Y. Ogras, Janardhan Rao Doppa, Raid Z. Ayoub, Michael Kishinevsky, Partha P. Pande. *Online Adaptive Learning for Runtime Resource Management of Heterogeneous SoCs*. Proceedings of DAC 2020.

**Primary source:** arXiv:2008.09728, author-uploaded conference paper: https://arxiv.org/pdf/2008.09728

**Important attribution note:** this DAC paper is partly a synthesis of prior work by the authors and collaborators. In particular, its explicit-NMPC GPU result summarizes Mercati et al., *Multi-variable Dynamic Power Management for the GPU Subsystem*, DAC 2017. Quantitative results are therefore attributed below to the mechanism paper when appropriate rather than silently treated as new experiments of the 2020 paper.

---

## 1. Problem

**SOURCE-DERIVED.** Heterogeneous SoCs expose many runtime control variables: active processing elements, voltage/frequency states, GPU slices and other accelerator-related knobs. The configuration space can be enormous and the best configuration depends on a workload that may change rapidly or may not have been known at design time.

The paper studies two families of adaptive control:

1. **model-guided online imitation learning (IL)**;
2. **explicit nonlinear model predictive control (ENMPC)**.

Both depend on predictive models of system behavior rather than only threshold reactions.

---

## 2. Resource model

**SOURCE-DERIVED.** The resource-management problem is specialized to heterogeneous SoCs. Relevant controlled resources include:

- number of active CPU/GPU processing elements;
- voltage/frequency states;
- number of active GPU slices;
- power states.

Observable state includes hardware/performance counters and sensors such as:

- utilization;
- retired instructions;
- CPU cycles;
- cache misses;
- memory traffic/bandwidth;
- chip power;
- temperature.

**INFERENCE.** The paper does not define a general logical-resource model. Resource semantics are encoded implicitly by the platform-specific state variables and control knobs.

---

## 3. Predictive models and controller are distinct objects

**SOURCE-DERIVED.** A central mechanism is separation between:

- models of power, performance and temperature;
- models of sensitivity of objectives to control-variable changes;
- the policy/controller that selects a configuration.

Models can be initialized offline and adapted online. The paper specifically discusses recursive least-squares-based performance models with forgetting factors for changing workloads.

This separation is essential for ElasticXxx.

**ELASTIC DECISION: ADOPT / GENERALIZE.** Elastic should distinguish:

```text
Observation model
System / transition model
Impact / objective model
Controller / planner
```

A learned model must not be conflated with the planner that consumes it.

---

## 4. Model-guided online imitation learning

**SOURCE-DERIVED.** The IL path starts from an offline policy and predictive power/performance models. During runtime:

1. state/counter data are collected;
2. power and performance models are continuously updated;
3. before a control decision, candidate configurations near the current configuration are evaluated by the analytical models;
4. the configuration predicted to minimize energy is used as a runtime approximation of an Oracle decision;
5. those newly generated state/action examples are buffered;
6. the policy is periodically updated using that supervision.

The paper explicitly observes that predicting how hardware counters themselves change under a hypothetical configuration remains a difficult system-dynamics/state-transition problem.

**KEY ELASTIC LESSON.** Counterfactual evaluation is not free. A planner that asks "what would happen if I chose B instead of A?" needs a transition/dynamics model, not just a model of the current state.

---

## 5. Why the paper rejects naive RL for this context

**SOURCE-DERIVED.** The authors argue that conventional model-free RL is poorly suited to fast-changing SoC resource management because:

- trial-and-error exploration may take too long to converge relative to workload changes;
- reward-function design is difficult and strongly influences controller quality.

This is a domain-specific conclusion, not a universal rejection of RL.

**ELASTIC DECISION: ADOPT AS SAFETY CAUTION.** ElasticXxx should not allow unconstrained exploratory learning to directly manipulate trusted resource state. Exploration must remain inside legal transitions and semantic invariants, and unsafe actions must be rejected independently of a learned policy.

A later production system, AWARE (USENIX ATC 2023), reinforces this deployment concern by explicitly adding bootstrapping for safer RL exploration and reports materially fewer SLO violations during training. AWARE should be reviewed separately under cloud autoscaling / learned production control before using its mechanisms more deeply.

---

## 6. Explicit nonlinear MPC

**SOURCE-DERIVED.** The DAC 2020 paper explains that the underlying GPU power-management problem coordinates control variables with different actuation times and costs:

- DVFS can be changed relatively quickly;
- changing the number of active GPU slices is slower and more expensive.

The cited mechanism therefore uses a **multi-rate controller**:

- a **slow-rate controller** jointly controls DVFS and active slices at coarse granularity;
- a **fast-rate controller** adjusts DVFS at finer granularity.

The nonlinear constrained control problem can be formulated as NMPC, but solving the NMPC optimization directly at runtime is considered too expensive for firmware/hardware deployment. **Explicit NMPC** approximates the NMPC control surface with lightweight regression models so runtime action selection is cheap.

**ELASTIC DECISION: ADOPT PRINCIPLE / ADAPT MECHANISM.** Elastic should support different control timescales and should allow an expensive planner to be distilled/approximated into a low-overhead fast path, provided the approximate policy remains inside independently enforced invariants.

---

## 7. Multi-rate adaptation is a first-class systems issue

**SOURCE-DERIVED.** The paper explicitly motivates multi-rate control because control knobs have different actuation latency and energy overhead.

This matters directly to ElasticXxx. A global scheduler tick is likely insufficient when resources differ by orders of magnitude in:

- observation latency;
- decision latency;
- transition latency;
- minimum dwell time;
- reversibility;
- transition cost.

**ELASTIC PROPOSAL.** Instead of one universal adaptation frequency, a resource or transition should be able to expose timing semantics such as:

```text
observation_period
minimum_dwell
expected_transition_latency
cooldown
validity_horizon
```

The exact API is an open question.

---

## 8. MPC changes the objective from an action to a trajectory

Classical reactive selection can be represented conceptually as:

```text
state_t -> best action now
```

MPC-style planning instead reasons over a finite future horizon:

```text
state_t
  -> action_t
  -> predicted state_{t+1}
  -> action_{t+1}
  -> ...
```

and optimizes the trajectory subject to constraints.

**ELASTIC PROPOSAL.** Elastic should therefore distinguish at least two planner outputs:

1. an immediately executable `ElasticAction` / transition;
2. an optional predicted `ElasticTrajectory` or receding-horizon plan.

Only the first action should normally be committed; the rest should be treated as predictions subject to replanning after new observations.

---

## 9. Receding horizon versus fixed plan

**ELASTIC INFERENCE FROM MPC.** A predicted future plan should not become a binding script when the environment is uncertain. The architecture should naturally support:

```text
OBSERVE
PREDICT H steps
OPTIMIZE H steps
EXECUTE first legal transition
OBSERVE again
REPLAN
```

This is especially compatible with the earlier literature lessons:

- Pollux: useful progress rather than raw utilization;
- Alpa: structured/decomposed planning rather than monolithic search;
- HPC malleability: transitions may remain pending and require safepoints;
- NOMAD: prepared transitions may abort at commit time;
- FlexMem: hysteresis and confidence matter.

Thus an Elastic trajectory must be *advisory and continuously revalidated*, not treated as a guaranteed future state sequence.

---

## 10. Approximate controller versus trusted constraints

**SOURCE-DERIVED.** Explicit NMPC reduces online computation by approximating an expensive NMPC control surface with regression models.

**KEY ELASTIC DISTINCTION.** This is attractive for Elastic fast paths, but approximation of the policy and enforcement of correctness must remain separate:

```text
learned / approximate policy
          |
          v
candidate transition
          |
          v
trusted legality + semantic validation
          |
          +---- reject
          |
          v
execute
```

The learned approximation may decide poorly; it must not be able to make an illegal transition legal.

---

## 11. Quantitative results, attributed precisely

**SOURCE-DERIVED THROUGH THE DAC 2020 SYNTHESIS.** For model-guided online IL, the paper reports that its cited online-IL implementation reaches close to the Oracle policy within 6 seconds in the shown adaptation experiment; the paper also reports a sub-20 KB training buffer for 100 epochs in that setup.

**SOURCE-DERIVED FROM THE SUMMARIZED MERCATI ET AL. GPU MECHANISM.** The explicit-NMPC GPU result summarized by Mandal et al. reports:

- GPU energy savings from 5% to 58% across tested applications;
- average GPU energy saving of about 25%;
- about 15% savings at package and package+DRAM levels;
- approximately 0.4% performance overhead.

These numbers are specific to the evaluated Intel Core i5 GPU subsystem and workloads. They must not be generalized to ElasticXxx.

---

## 12. Limitations and open problems stated in the paper

**SOURCE-DERIVED.** The paper ends by identifying open problems including:

- low-cost implementations of IL and RL suitable for firmware;
- generalization of explicit MPC to broader classes of systems;
- higher-dimensional spaces of control inputs and outputs.

The online-IL section additionally identifies prediction of counter changes under alternative configurations as an open dynamics/state-transition problem.

These limitations map unusually closely to Elastic's intended scope and therefore deserve direct attention rather than being hidden behind a generic "learned planner" abstraction.

---

## 13. Relationship to the emerging Elastic model

| Mechanism | ElasticXxx disposition |
|---|---|
| Runtime analytical models | **ADOPT / GENERALIZE** |
| Online model adaptation | **ADOPT where justified** |
| Controller separate from model | **ADOPT** |
| Sensitivity models | **INVESTIGATE / GENERALIZE** |
| Counterfactual candidate evaluation | **ADOPT principle** |
| Model-guided IL | **INVESTIGATE as optional planner technique** |
| Unconstrained model-free RL exploration | **REJECT for trusted direct actuation** |
| MPC / finite-horizon planning | **ADOPT as planner family** |
| Explicit MPC / policy approximation | **ADOPT principle for fast path** |
| Multi-rate control | **ADOPT / GENERALIZE** |
| Platform-specific SoC state representation | **REJECT as core abstraction** |
| Learned policy as safety authority | **REJECT** |

---

## 14. Consequence for `ElasticTransitionModel`

The previous working idea `C(t | state)` is now insufficient as a complete dynamics interface.

A predictive controller may need an estimate of the next state:

```text
P(state_{t+1} | state_t, transition_t)
```

or, deterministically where appropriate:

```text
state_{t+1} = f(state_t, transition_t)
```

and separately estimates for:

```text
transition cost
useful-progress effect
risk
uncertainty
```

**ELASTIC PROPOSAL.** A future model interface may therefore separate:

- `ElasticDynamicsModel` — predicts next state / distribution;
- `ElasticImpactModel` — predicts useful-progress/objective impact;
- `ElasticTransitionCostModel` — predicts actuation cost;
- `ElasticPlanner` — selects a legal action or trajectory using those models.

No novelty is claimed for this decomposition yet.

---

## 15. SciRust capability check

SciRust remains an R&D environment and is never a required ElasticXxx runtime dependency.

### Existing capabilities verified

Current SciRust code already includes:

- `scirust-control::LinearMpc`: finite-horizon linear MPC for `x_{k+1}=Ax_k+Bu_k`, quadratic cost, hard box input constraints, box-QP solution;
- `scirust-estimation::RlsFilter`: deterministic multi-channel recursive least squares with forgetting factor and zero heap allocations in the update hot loop.

Therefore "MPC" and "online RLS" are **not SciRust gaps**.

### Candidate gap revealed

**SCIRUST-GAP-CONTROL-001 — advanced nonlinear / explicit / multi-rate predictive control**

**Status: INVESTIGATE.** Repository search during this review did not reveal a clearly exposed generic capability for:

- nonlinear MPC;
- explicit MPC policy construction/approximation;
- generic multi-rate MPC;
- adaptive sensitivity-model integration.

This is not yet a confirmed gap. Before implementation we must determine whether these should be one general SciRust capability, several specialized components, or external-solver integrations, and whether other ongoing projects independently require them.

The scientific usefulness would be general beyond ElasticXxx: robotics, energy systems, thermal management, process control, autonomous systems, and embedded control.

---

## 16. Experiments suggested for ElasticXxx

**EXPERIMENT REQUIRED.** Once a dynamic resource prototype exists, compare on the same controlled workload:

1. reactive threshold controller;
2. greedy one-step planner;
3. linear MPC where the dynamics are approximately linear;
4. receding-horizon planner with online-adapted dynamics;
5. distilled/explicit fast-path approximation of the expensive planner.

Measure:

- useful progress;
- SLO / invariant violations;
- energy/resource cost;
- number and cost of transitions;
- prediction error over horizon;
- planner latency and memory;
- stability / oscillation;
- recovery under workload regime changes;
- divergence between full planner and approximate fast path.

---

## 17. Current conclusion

This paper materially changes ElasticXxx's planning model in three ways.

First, **prediction must model dynamics, not merely score the current state**. Second, **different resource controls may require different timescales**. Third, **an expensive optimal/predictive controller can be separated from a lightweight runtime approximation**, provided safety and semantic legality remain independently enforced.

The emerging Elastic control loop is therefore better represented as:

```text
OBSERVE
  -> UPDATE MODELS
  -> PREDICT FUTURE STATES
  -> OPTIMIZE A TRAJECTORY
  -> VALIDATE FIRST TRANSITION
  -> EXECUTE
  -> VERIFY
  -> OBSERVE AGAIN / REPLAN
```

rather than a single reactive `pressure -> action` mapping.
