# BE14b — guarded concurrency width

BE14b integrates the Boolean eligibility layer with the existing in-process concurrency permit controller. It does not introduce a scheduler and it does not grant actuation authority to a Boolean result.

## Stable predicate contract

The stable predicate key is `elastic.concurrency::target-holds-active-permits`.

Its source is the existing `active-permits` runtime observation, measured in **permits**. For a requested target width `w`, the predicate is `active_permits <= w`. Missing, unsupported, stale, non-finite, or otherwise unusable evidence evaluates to `Unknown`; `False` and `Unknown` stop before the trusted runtime transaction.

The immutable configured maximum is validated separately before Boolean evaluation. A `True` predicate only preserves the concurrency `reinterpret` candidate through Boolean pruning. The existing `TransactionalConcurrency` adapter then revalidates the live holder count and width bounds immediately before mutation, applies the change, verifies the resulting width, and commits or rolls back through the ordinary runtime transaction.

## Forecast and evidence boundary

The first slice uses `CurrentStateForecaster`: a zero-horizon current-state projection with no calibrated-confidence claim. The durable report records the predicate key, source signal/unit, requested target, explicit `true`/`false`/`unknown` result, forecast metadata, and a bounded `DecisionTrace/v1` JSON payload. The trace is explanatory and replay evidence only; it cannot authorize actuation.

## Qualification cases

The runtime tests cover:

- `True`: a live observation admits the declared candidate, then the ordinary transactional runtime performs validate, act, verify, and commit;
- `False`: shrinking below live holders is rejected before runtime planning or mutation;
- `Unknown`: missing/unsupported/stale observation evidence is rejected without mutation;
- differential baseline: the admitted guarded path reaches the same committed width as the unguarded trusted runtime for the same explicit target;
- fail-closed bounds: zero or above-maximum target widths are rejected before observation or mutation.

No throughput, latency, energy, branch-prediction, or speedup claim is made by this slice. A performance claim would require a separate reproducible benchmark against the non-Boolean baseline.
