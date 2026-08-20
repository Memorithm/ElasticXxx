# Power, Energy, and Thermal Semantics

**Status:** provisional ElasticXxx design note derived from literature review. This is not a novelty claim.

## 1. Motivation

Power-aware and thermal-aware systems expose several quantities that are frequently conflated but have different semantics:

- power;
- energy;
- temperature;
- thermal headroom;
- cooling capacity;
- application quality/accuracy.

ElasticXxx should not force these into one generic scalar `resource` interpretation.

## 2. Working distinctions

### Energy

Energy is accumulated work/consumption over time.

```text
E(t1,t2) = integral(P(t), t1..t2)
```

An energy allowance naturally behaves like a **stock/budget** that can be consumed over an interval.

### Power

Power is an instantaneous/rate-like quantity.

A job may have a fixed total power cap while the runtime redistributes that cap among devices/nodes:

```text
sum(local_power_limits) <= global_power_limit
```

Power therefore supports **allocation and redistribution under a rate constraint**.

### Temperature

Temperature is primarily a **dynamic physical state**, not a fungible allocatable quantity. It has inertia and spatial coupling:

```text
T(t+1) = f(T(t), power(t), environment, topology, ...)
```

The relevant runtime constraint is often:

```text
T_i(t) <= T_i,max
```

### Thermal headroom

Thermal headroom is a derived safety margin:

```text
headroom_i = T_i,max - T_i
```

It may be useful to planners but should not be confused with a conserved stock.

### Cooling capacity

Cooling capacity may be a separately actuated/allocated physical resource depending on platform scope (fan speed, pump capacity, rack/site cooling allocation, etc.).

## 3. Semantic degradation is orthogonal

PowerDial demonstrates that some systems reduce computation by accepting lower application quality.

ElasticXxx must represent that as a semantic effect, not as an ordinary resource mutation.

```text
resource benefit != semantic permission
```

A lossy action is absent from an `Exact` admissible space unless equivalence has been established.

## 4. Proposed quantity categories

The existing provisional taxonomy can be refined with:

```text
STOCK         energy budget, credits
RATE          power, bandwidth, IOPS
CAPACITY      RAM, VRAM, cores, slots
DYNAMIC_STATE temperature, queue occupancy, battery SOC, etc.
DERIVED_MARGIN thermal headroom, spare capacity, deadline slack
CONFIGURATION frequency, voltage, fan state, batch size
SEMANTIC_MODE exact/approximation contract-controlled modes
```

These categories are not necessarily disjoint implementation types; they capture different legal operations and invariants.

## 5. Transition families

Different semantics imply different transitions.

### Stock

```text
CONSUME
REFILL
TRANSFER_CREDIT
RESERVE
RELEASE_RESERVATION
```

### Rate / budget

```text
SET_LIMIT
TRANSFER_BUDGET
REBALANCE
BORROW
RETURN
```

### Dynamic state

Temperature itself is generally not directly set. The system acts through control inputs:

```text
CHANGE_FREQUENCY
CHANGE_VOLTAGE
MOVE_WORK
THROTTLE
CHANGE_COOLING
```

and predicts their effect on future temperature.

## 6. Hard constraints versus objectives

A thermal-control problem illustrates the intended ordering:

```text
hard:
    temperature <= Tmax
    semantic contract preserved

objectives:
    maximize useful progress
    minimize energy
    minimize transition/churn
    smooth control
```

An optimizer is not permitted to trade a hard temperature or semantic constraint against a weighted utility gain unless the contract explicitly defines such behavior.

## 7. Temporal model

For resources with dynamics, an observation snapshot is insufficient.

A generic planner may need:

```text
state x_t
control u_t
x_(t+1) = f(x_t, u_t, disturbances)
```

with uncertainty and horizon-specific validity.

This motivates keeping `ElasticDynamicsModel` separate from `ElasticTransitionCostModel` and `ElasticImpactModel`.

## 8. Multi-scale control

Power/thermal management reinforces the multi-scale architecture:

```text
site / cluster budget
        ↓
job budget
        ↓
node/device budget
        ↓
fast local controls (DVFS, throttling, etc.)
```

The topology may be represented as a graph while a tree/hierarchy is used as an efficient control overlay.

## 9. Open design questions

1. Which quantity categories deserve first-class Rust types versus metadata/traits?
2. Should `DynamicState` be an Elastic resource kind or a separate observed-state abstraction?
3. How should conserved budgets be represented across concurrent transitions?
4. Can redistribution operations be transactional across multiple resource owners?
5. How should uncertainty in thermal/power models affect admissibility margins?
6. Which semantic-degradation actions should be statically excluded under `Exact`?

## 10. Current principle

> **Power is a rate constraint, energy is accumulated consumption, temperature is a dynamic physical state, and semantic quality is an independent contract dimension.**

ElasticXxx should preserve these distinctions throughout its type, planner, and runtime design.
