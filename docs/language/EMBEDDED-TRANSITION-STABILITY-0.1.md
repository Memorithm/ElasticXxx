# Transition stability / anti-thrashing contract v0.1

Status: ELANG7g generic runtime admission layer.

ElasticXxx distinguishes **transition legality** from **transition stability**.
A transition can be structurally legal, capability-supported, and still be a
bad control action because the system is oscillating or changing too quickly.
The transition-stability gate adds an independent fail-closed precondition. It
never grants authority that the normal validator would reject.

## Policy identity

`TransitionStabilityPolicyV1` is bound to one exact pair:

- `TransitionMechanism`;
- `DimensionId`.

A policy may enable any combination of:

- source-bound, freshness-bound hysteresis;
- minimum post-commit cooldown;
- rolling transition-rate limit.

A policy with none of the three is rejected instead of becoming an implicit
allow-all policy.

## Hysteresis

`HysteresisPolicyV1` names an exact `ObservationSource` and
`ObservationSignalId`. Evidence from another provider cannot satisfy the band.

For a rising band:

```text
trigger: signal >= trigger_threshold
re-arm:  signal <= release_threshold
require: release_threshold < trigger_threshold
```

For a falling band:

```text
trigger: signal <= trigger_threshold
re-arm:  signal >= release_threshold
require: release_threshold > trigger_threshold
```

After a committed transition the band becomes disarmed. It cannot emit another
permit until the signal has crossed the release side of the dead band and later
reached the trigger side again. Unsupported, non-finite, missing, ambiguous,
future-dated or stale observations fail closed.

## Cooldown

An optional non-zero cooldown enforces a minimum monotonic duration between two
recorded commits. It is checked both when issuing a permit and again when the
commit is recorded.

## Rolling rate limit

`TransitionRateLimitV1` caps committed transitions inside one rolling monotonic
window. The maximum retained history is bounded by
`MAX_TRANSITION_RATE_LIMIT_COMMITS` (1024). Old timestamps are pruned when they
leave the configured window. If no rate limit is configured, no history queue is
retained.

## Single-use generation permit

`TransitionStabilityGateV1::check()` returns an auditable report and, only when
all configured conditions pass, a non-cloneable `TransitionStabilityPermitV1`.
The permit carries the gate generation and exact mechanism/dimension.

`record_commit()` consumes the permit **after** the caller has actually committed
the transition. A different recorded commit advances the generation and makes
older permits stale. For hysteresis policies, the permit also expires with the
source observation that produced it; holding an old permit does not extend
telemetry freshness.

The permit is not an actuation capability. The required ordering remains:

```text
stable admission
    -> trusted transition validation
    -> ACT
    -> VERIFY
    -> COMMIT
    -> record_commit(stability_permit)
```

If physical actuation fails or rolls back, the stability commit must not be
recorded.

## Durable report

`TransitionStabilityReportV1` contains no absolute `Instant`; monotonic instants
are process-local. It records:

- status/reason;
- mechanism and dimension;
- whether hysteresis is armed;
- finite observed value when available;
- remaining cooldown milliseconds;
- current rate-window count and configured bound;
- gate generation.

The possible blocking statuses distinguish insufficient evidence, trigger not
reached, awaiting hysteresis release, cooldown, rate limiting and clock
regression.

## Non-goals

This contract does not:

- decide whether a candidate is semantically desirable;
- validate capabilities or attestations;
- replace source-bound domain predicates;
- perform actuation;
- verify physical state;
- persist `Instant` values across processes;
- claim that one set of thresholds is appropriate for every domain.

Domain owners choose their thresholds and observations. ElasticXxx supplies the
generic, bounded anti-thrashing mechanism.
