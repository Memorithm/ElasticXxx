# GEOPM: Global Extensible Open Power Manager

**Paper:** Jonathan Eastep et al. *Global Extensible Open Power Manager: A Vehicle for HPC Community Collaboration on Co-Designed Energy Management Solutions*. ISC High Performance 2017. A closely related 2016 PMBS version was used for detailed mechanism inspection.

**Primary/authoritative sources:**

- https://doi.org/10.1007/978-3-319-58667-0_21
- https://www.dcs.warwick.ac.uk/pmbs/pmbs16/PMBS/papers/paper6.pdf
- https://geopm.github.io/

**Review status:** mechanism-level review complete.

## 1. Problem

**SOURCE-DERIVED.** GEOPM addresses job-level power/energy management on large HPC systems. Its core design goal is scalable coordination of hardware/software power and performance controls across many compute nodes.

## 2. Resource model

The important controlled quantity in the reviewed power-balancing example is a **job power budget** distributed among nodes. Per-node RAPL controls enforce local limits.

This is not a consumable stock in the same sense as total energy. It behaves primarily as a rate/capacity constraint:

```text
sum(node_power_caps) <= job_power_cap
```

The distribution can change while the global cap remains fixed.

## 3. Observability

**SOURCE-DERIVED.** GEOPM is application-aware. It monitors platform metrics and can use application regions/progress. The power-balancing strategy identifies imbalance and critical-path behavior from progress/stall characteristics and redirects power toward nodes where extra power can reduce overall time-to-solution.

## 4. Planner / control structure

**SOURCE-DERIVED.** GEOPM uses a **tree-hierarchical** runtime architecture whose depth/fan-out can scale with deployment size. Its plug-in architecture allows different optimization strategies and different hardware/software control knobs.

The reviewed power-balancing plug-in reallocates a fixed power budget rather than simply increasing every node's cap.

## 5. Critical-path redistribution

**SOURCE-DERIVED.** The power-balancing algorithm can divert power from nodes outside the MPI critical path toward a critical-path node. The paper's traces show larger allocations to the slower/critical node until iteration runtimes become better balanced.

**KEY ELASTIC LESSON.** A resource can be elastic through **redistribution under a conservation constraint**, not only through grow/shrink.

A general action family therefore needs concepts such as:

```text
TRANSFER_BUDGET
REBALANCE
BORROW
RETURN
```

where appropriate to the semantics of that resource.

## 6. Power versus energy

**ELASTIC INFERENCE.** This paper reinforces a distinction that must be explicit in ElasticXxx:

```text
Power  = instantaneous/rate-like quantity (W)
Energy = accumulated quantity over time (J, Wh)
```

Formally:

```text
E(t1,t2) = integral(P(t), t1..t2)
```

A power cap can be respected while total energy still changes substantially depending on completion time.

Therefore `POWER` and `ENERGY` must not be represented as the same elastic dimension.

## 7. Hierarchical control

**ELASTIC DECISION: ADOPT / GENERALIZE.** GEOPM strongly supports hierarchical or multi-scale control for large systems.

However ElasticXxx should not hard-code one tree as its universal resource topology. Earlier work already motivates a more general resource graph. A tree may be an efficient control overlay over that graph.

## 8. Results

The reviewed PMBS version reports up to about 32% runtime improvement for selected CORAL procurement benchmarks on a power-limited Xeon Phi cluster; the later ISC/SC descriptions commonly summarize results as up to about 30% depending on version/setup.

These results establish that reallocating a fixed power budget can improve useful progress under heterogeneity/imbalance. They do not establish universal gains for arbitrary workloads.

## 9. Elastic disposition

| GEOPM mechanism | ElasticXxx disposition |
|---|---|
| Explicit global power budget | **ADOPT / GENERALIZE** |
| Runtime redistribution of budget | **ADOPT** |
| Application progress awareness | **ADOPT** |
| Critical-path-aware allocation | **ADOPT principle / GENERALIZE** |
| Tree-hierarchical scalable control | **ADOPT as possible control overlay** |
| Plug-in optimizer architecture | **ADOPT principle** |
| Hardware RAPL-specific actuation | **ADAPT behind actuator abstraction** |
| Power and energy treated interchangeably | **REJECT as Elastic terminology** |

## 10. Experiment suggested for ElasticXxx

**EXPERIMENT REQUIRED.** Under one fixed total power budget, compare:

1. equal static allocation;
2. local-only feedback;
3. hierarchical redistribution;
4. an Elastic planner using useful-progress estimates and transition/churn cost.

Measure total time, energy-to-solution, power-cap violations, synchronization stall time, decision overhead, and allocation churn.

## 11. Current conclusion

GEOPM is strong prior art for hierarchical, application-aware redistribution of a constrained power budget. For ElasticXxx, the strongest lessons are that **resource elasticity includes redistribution**, and that **power is a rate constraint distinct from accumulated energy**.
