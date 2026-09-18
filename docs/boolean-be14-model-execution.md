# BE14c model-execution Boolean screening

BE14c adds a fail-closed Boolean front end to the existing model-execution profile policy. It does not replace `ModelExecutionAdaptivePlannerV1`, the transactional model actuator, validation, verification, rollback, or durable cycle evidence.

The source signals are the existing `FREE_CAPACITY` value in the backend-owned capacity unit declared by `ModelExecutionEnvelopePolicyV1`, and `UTILIZATION` as a finite fraction in `[0, 1]`. Provider rules continue to declare minimum free capacity in that policy unit and maximum utilization in integer basis points.

For every provider rule, BE14c derives two stable predicates keyed by the rule preference rank: `elastic.model-execution::rule-<rank>-free-capacity` and `elastic.model-execution::rule-<rank>-utilization`. A rule is `True` only when both fresh observations satisfy its thresholds, `False` when usable evidence disproves at least one threshold, and `Unknown` when required evidence is missing, stale, unsupported, non-finite, or outside the numeric contract.

Screening follows provider preference order. A `False` rule is pruned. An `Unknown` rule blocks resolution of lower-priority matches because skipping it could change policy semantics. Only complete `True` evidence allows the existing numeric/profile planner to run, and that planner re-resolves the same policy. Boolean evidence is explanatory only and cannot authorize actuation.

This slice intentionally makes no performance claim and does not complete BE14c. Controller wiring, durable generic `DecisionTrace` binding, full end-to-end actuation/VERIFY/rollback qualification, and benchmark evidence remain separate exit criteria.
