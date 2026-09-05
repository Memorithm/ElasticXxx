# Model run evidence attempt v1

`ModelExecutionControllerV1` exposes a bounded-run evidence attempt surface for callers that need to retain durable evidence from cycles that completed before a later model-execution failure.

The generic forecast-aware run remains authoritative for execution. Model evidence is derived only after the generic run attempt has returned, and only from cycles that the generic runtime marked completed.

## Public surface

`ModelExecutionRunEvidenceAttemptV1` has two outcomes:

- `Completed(ModelExecutionRunEvidenceResultV1)` contains the completed `ForecastRunResult` and one `ModelExecutionCycleEvidenceV1` per completed cycle;
- `Failed(ModelExecutionRunEvidenceFailureV1)` retains either the generic runtime failure or an evidence-layer failure together with every completed evidence artifact captured before that failure.

`ModelExecutionRunEvidenceFailureV1` distinguishes:

- `Runtime`: the generic forecast-aware run failed, while all earlier completed cycles were converted successfully to durable model-cycle evidence;
- `Evidence`: rebuilding the exact controller contract binding, converting a completed cycle, or reconciling the final physical profile failed.

When a generic runtime failure happens first and evidence conversion of its completed prefix later also fails, both failures are retained. The historical `run_with_evidence()` compatibility API continues to return the original runtime failure first.

## Compatibility

`ModelExecutionControllerV1::run_with_evidence()` is implemented through:

`run_with_evidence_attempt(...).into_result()`

The existing success type and error precedence therefore remain unchanged for callers that do not opt into the attempt API.

## Completed prefix only

A catastrophically failed cycle does not receive `ModelExecutionCycleEvidenceV1`.

That artifact requires a trustworthy completed-cycle terminal profile rank. After an escaped actuation, verification, commit, or rollback failure, the final physical model profile can be unknown or partially changed. Version 1 therefore records durable evidence only for the prefix of cycles that actually completed.

The failed cycle itself remains available through the retained `ForecastRunFailure` / `ForecastCycleFailure` audit structure.

## Physical-state warning

The last completed evidence artifact is not necessarily the physical state after a failed later cycle.

For example:

1. cycle 1 commits profile rank `10` and gets durable evidence;
2. cycle 2 applies profile rank `0`;
3. verification fails;
4. rollback to rank `10` fails.

The durable completed prefix correctly ends at rank `10`, while the backend may physically remain at rank `0`. Consumers must not interpret the completed evidence prefix as the terminal state of the failed run.

Final evidence-versus-backend reconciliation is therefore performed only when the generic bounded run completed normally or cooperatively cancelled.

## Evidence failures

Evidence conversion is fail-closed. A failure preserves any already captured durable prefix and records the evidence-layer error.

If a generic runtime failure existed first, it is retained separately and keeps compatibility precedence. This prevents a secondary serialization/contract-validation problem from hiding the physical runtime failure that occurred earlier.

## Scope limits

This contract does not:

- serialize a complete failed-cycle or failed-run artifact;
- reconstruct missing structured runtime state from diagnostic events;
- fabricate a terminal profile rank for a catastrophically failed cycle;
- claim rollback success when restoration failed;
- authorize physical replay from historical evidence;
- add model/provider-specific E/S/A semantics;
- add a real hardware/model backend;
- make performance or quality claims.

A future versioned failed-run evidence envelope may serialize the retained failure audit, but only after the runtime exposes the structured state needed to do so without inference.
