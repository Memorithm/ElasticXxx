# Multicore Thermal Management with Model Predictive Control

**Paper:** Francesco Zanini, David Atienza, Luca Benini, Giovanni De Micheli. *Multicore Thermal Management with Model Predictive Control*. ECCTD 2009.

**Primary source:** https://si2.epfl.ch/~demichel/publications/archive/2009/C2L-C3-9245.pdf

**Review status:** mechanism-level review complete.

## 1. Problem

**SOURCE-DERIVED.** The paper addresses thermal management of multicore processors under time-varying performance demand. The controller must respect a maximum temperature constraint while tracking requested per-core workload and avoiding abrupt DVFS changes and thermal cycles.

## 2. Thermal state is dynamic

**SOURCE-DERIVED.** Temperature is modeled using a thermal RC-style state-space model. Heat propagates between neighboring thermal cells and depends on prior temperature, power dissipation, spatial coupling, and ambient conditions.

The essential property is:

```text
Temperature(t+1) depends on Temperature(t) + power history + spatial coupling
```

not simply on instantaneous utilization.

**KEY ELASTIC LESSON.** `temperature` should not be modeled as a fungible allocatable resource. It is primarily a **derived physical state** with inertia, while `thermal headroom` is a derived margin against a constraint and cooling capacity may itself be an allocatable resource.

## 3. Control variables

**SOURCE-DERIVED.** The controller adjusts per-core operating frequency/voltage. It receives time-varying workload requirements from a higher layer and computes frequency assignments.

## 4. Objective and constraints

The paper separates:

- **hard constraint:** maximum temperature;
- **objective:** track required performance/workload;
- additional preference: smoother temperature/frequency evolution and lower power.

This separation strongly matches the emerging Elastic distinction between invariants/constraints and objectives.

## 5. Predictive control

**SOURCE-DERIVED.** The MPC optimizes over a horizon of future steps using current temperature/frequency state and required workload. It generates a sequence of future control moves, applies only the first, then solves again with new measurements.

This is standard **receding-horizon control** and reinforces the Elastic trajectory model:

```text
predict trajectory
   ↓
optimize sequence
   ↓
apply first transition only
   ↓
observe again
   ↓
replan
```

## 6. Explicit versus implicit MPC

**SOURCE-DERIVED.** The paper discusses two implementations:

- **implicit MPC:** solve the optimization problem online;
- **explicit MPC:** precompute a piecewise-affine control law / coefficients offline and evaluate it cheaply online.

**ELASTIC RELATION.** This is strong prior art for the architecture we have been converging toward:

```text
expensive planning / precomputation
        ↓
cheap serving policy
        ↓
fast runtime actuation
```

Therefore this idea cannot be presented as novel in itself.

## 7. Results

The authors report that both the compared convex method and MPC satisfy the temperature bound, while MPC tracks demanding workloads better near the thermal limit. In one highlighted point, the convex policy supplies a workload about 15% below the required level while MPC tracks the requirement. They report approximately 2.5×–5× improvements in several control-smoothness indices over the compared convex thermal policy.

These are results for their modeled/evaluated multicore setting, not universal guarantees.

## 8. Elastic disposition

| Thermal MPC mechanism | ElasticXxx disposition |
|---|---|
| Temperature as dynamic state | **ADOPT** |
| Maximum temperature as hard constraint | **ADOPT** |
| Receding-horizon control | **ADOPT as planner family** |
| Thermal state-space model | **ADAPT per hardware/domain** |
| Explicit MPC fast policy | **ADOPT principle / INVESTIGATE backend** |
| Frequency/voltage actuation | **ADAPT behind device actuator** |
| Temperature treated as allocatable resource | **REJECT** |

## 9. Consequence for the Elastic resource taxonomy

**ELASTIC PROPOSAL.** Distinguish at least:

```text
EnergyBudget      -> stock / accumulated budget
PowerBudget       -> rate / instantaneous capacity constraint
TemperatureState  -> dynamic physical state
ThermalHeadroom   -> derived safety margin
CoolingCapacity   -> potentially allocatable/control resource
```

The API names are provisional; the semantic distinction is the important part.

## 10. SciRust relation

SciRust already contains basic thermal-dynamics tooling in `scirust-sim` (including Newton cooling and 1-D transient heat conduction) and a finite-horizon linear MPC in `scirust-control`.

Therefore this paper does **not** establish a new SciRust gap. It provides an additional independent motivation for the already-open investigation into explicit/nonlinear/multi-rate predictive control.

## 11. Experiment suggested for ElasticXxx

**EXPERIMENT REQUIRED.** Build a thermal mock/plant with known dynamics and compare:

1. instantaneous threshold controller;
2. hysteresis controller;
3. linear receding-horizon MPC;
4. precomputed/approximated serving policy.

Measure temperature-limit violations, useful progress, power, energy-to-solution, control smoothness, decision latency, and sensitivity to model error.

## 12. Current conclusion

Thermal management demonstrates why Elastic state cannot be reduced to current resource counters. Some constraints have **memory and physical dynamics**. A safe controller must reason about future trajectories and keep derived physical states inside hard limits while optimizing useful progress.
