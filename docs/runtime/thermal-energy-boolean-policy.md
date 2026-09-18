# BE14h thermal/energy Boolean eligibility

Status: planning-only policy layer over the real Linux observer foundation. This
contract does not perform fan, clock, power-mode, scheduler, device, or other
physical actuation.

## Policy contract

`BooleanThermalEnergyPreplannerV1` binds two existing built-in observation
signals to one already-declared Elastic transition:

```text
thermal-margin >= application minimum
AND
energy-rate <= application maximum
```

The threshold values are supplied by the embedding application/operator.
ElasticXxx does **not** publish a universal safe thermal margin or power budget.
The constructor rejects non-finite values, negative minimum thermal margins,
negative power budgets, zero freshness windows, undeclared or capability-ungrounded
transitions, and resources that did not declare both source signals.

Stable predicate keys:

- `elastic.thermal-energy::thermal-margin-sufficient`
- `elastic.thermal-energy::energy-rate-within-budget`

Default live-evidence freshness constant: `THERMAL_ENERGY_MAX_AGE = 1s`. Callers
may provide a different non-zero bound when constructing the preplanner.

## Source binding

A policy instance is bound to exact `ObservationSource` identities for thermal
and direct-power telemetry. A numerically valid observation from a different
source is not interchangeable evidence. The evaluator searches for the exact
configured `(signal, source)` pair, so an unrelated provider cannot shadow valid
source-bound evidence; duplicate records from the configured source fail closed
as ambiguous.

The planner-facing numeric value must also be bit-identical to the corresponding
source-bound `Observation` value. A separately altered `PlanningContext` cannot
turn a real observation into a positive Boolean result.

The real Linux observers added by the BE14h foundation expose stable source
identities derived only from their configured sysfs paths, so success and
unsupported observations from one provider retain the same identity.

## Three-valued behavior

The two predicates use existing `ObservationThresholdPredicate` strong runtime
freshness semantics plus the source/identity checks above:

- both `True` -> the declared transition may survive Boolean pruning;
- any `False` with no dominating unresolved input -> rejected;
- missing, unsupported, stale, future, foreign-source, non-finite, or
  context-mismatched evidence -> `Unknown` / insufficient evidence.

The conjunction uses the existing strong-Kleene Boolean core. It does not
collapse missing evidence into `False`.

## Forecast boundary

The policy explicitly passes the current source-backed planning context through
`CurrentStateForecaster`. This is a zero-horizon compatibility forecast and does
not claim prediction confidence.

## Evidence and authority

Each evaluation records:

- exact configured source identities;
- source units (`degrees-celsius`, `watts`);
- operator thresholds;
- individual and combined three-valued facts;
- forecast method/horizon/confidence status;
- a bounded strict `DecisionTrace` for the declared transition.

A selected trace is still **eligibility evidence only**. It cannot authorize a
physical power/thermal action. A future actuator must independently define and
qualify capabilities, trusted validation immediately before effect,
post-actuation verification, rollback/fail-closed behavior, and a differential
baseline.

## Public usage

The types are exported through `elastic` and `elastic::prelude::*`. A typical
consumer can combine `LinuxThermalMarginObserver` and
`LinuxHwmonPowerObserver` in an `ObserverSet`, retain each observer's stable
`source()`, build an `ObservationSnapshot`, and evaluate the policy using only
the public `elastic` dependency.
