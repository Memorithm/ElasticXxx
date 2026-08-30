# Elastic adapter SPI

Elastic adapters are the trusted boundary between advisory planning and physical effects.
Third-party adapters should depend on the public `elastic` facade, not on private workspace crates.

## Supported contracts

A complete operational adapter normally provides:

1. `Observer` — collects current evidence and preserves provenance;
2. `TransactionalActuator` — validates, prepares, actuates, verifies, commits, and rolls back;
3. an `EirResource` produced from a validated `ResourceSpec` describing the dimensions, invariants, admitted transitions, and required capabilities of the resource.

`TransitionPlanner` is intentionally separate. Planner output is advisory and cannot authorize an effect that the adapter rejects.

## Observation semantics

Adapters must not manufacture telemetry.

- **available**: emit a valid `Observation` with the measured/configured value and an explicit `ObservationSource`;
- **unsupported/unavailable**: emit an unsupported observation with a reason when the signal is part of the provider contract but cannot be obtained;
- **failed provider**: surface a runtime/provider error when continuing would make the control decision untrustworthy.

Unknown is not zero and unsupported is not success.

## Transaction semantics

`TransactionalActuator` implementations must follow these rules:

- `validate` re-checks every applicable hard invariant and action-time precondition;
- `prepare` records enough trusted state to either perform or undo the proposed change and must not perform the physical effect itself;
- `actuate` performs only the prepared effect and re-checks any condition that can become stale between validation and action;
- `verify` observes the post-action state and returns `Pass`, `Fail`, or `Inconclusive` honestly;
- `commit` is permitted only after successful verification;
- `rollback` attempts to restore the pre-action state and reports whether invariants were actually restored.

A rollback that cannot restore invariants is an error, not a successful transaction.

## Freshness

When a planner recommendation carries `RecommendationContext`, use the public freshness gate before the physical effect. The recommendation must explicitly track the actuated resource and its planner/observation/resource generations must still match the trusted current snapshot.

Freshness does not replace adapter legality checks. A fresh recommendation may still be rejected by bounds, active holders, invariants, permissions, device state, or another trusted action-time condition.

## Conformance expectations

A third-party adapter should have tests covering at least:

- a legal verified transition commits;
- invalid targets fail before the effect;
- failed or inconclusive verification triggers rollback;
- rollback restores the previous state and declared invariants;
- rollback failure is surfaced;
- plans for a different logical resource are rejected;
- unsupported telemetry is not converted into a fabricated numeric value.

`crates/elastic-downstream` contains an executable third-party-style adapter implemented while depending only on `elastic`; workspace CI uses it as a facade/SPI guard.
