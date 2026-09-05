# Runtime cycle-attempt audit v1

`CycleAttempt` is the fail-closed audit surface for one invocation of the existing trusted ElasticXxx runtime cycle.

It does not introduce another control loop. `Runtime::cycle_attempt()` delegates execution to `Runtime::cycle_with_sink()` and only retains information that the existing executor actually returned or emitted.

## Outcomes

A cycle attempt has exactly two outcomes:

- `CycleAttempt::Completed(CycleResult)` — the existing runtime completed and returned its authoritative structured result;
- `CycleAttempt::Failed(CycleFailure)` — the existing runtime returned `RuntimeError` after possibly emitting partial audit events.

`CycleAttempt::into_result()` maps directly back to the legacy `Result<CycleResult, RuntimeError>` semantics.

## Failed-attempt evidence

`CycleFailure` currently preserves:

- the exact EIR resource used by the attempted cycle, including its structural fingerprint;
- the authoritative `RuntimeError` returned by the trusted runtime;
- every ordered `RuntimeEvent` that `cycle_with_sink()` emitted before the error escaped.

This is sufficient to retain evidence that a failed attempt reached stages such as `ActuationApplied` or `VerificationPerformed` even when an unrecoverable rollback prevents a normal `CycleResult` from existing.

A missing event is not rewritten into a positive or negative claim. In particular, when rollback itself fails, the attempt must not fabricate `RollbackExecuted` or `CycleCompleted`.

## Deliberate limitation

The current trusted `cycle_with_sink()` error type does not return its local observation snapshot, validated plan, actuation object, or verification object when an error escapes. `CycleAttempt` therefore does not reconstruct those structures from diagnostic event strings.

This slice closes loss of the existing ordered failure event stream and exact resource identity. Persisting the full structured partial state of catastrophic attempts requires a later runtime-core change and remains an explicit gap.

## Relationship to model-execution evidence

`ModelExecutionCycleEvidenceV1` remains the richer durable contract for completed model-execution cycles. It records observations, forecast, plan, correlated E/S/A profile, validation, actuation, verification, and terminal outcome.

`CycleAttempt` does not pretend that a catastrophic runtime failure is a completed model cycle. A future model-specific catastrophic-attempt evidence contract may consume `CycleFailure`, exact controller contracts, and whatever physical state can still be read, but must preserve the distinction between:

- completed verified transaction evidence;
- partial failure audit evidence;
- fresh authorization for any later physical action.

Historical failure evidence never authorizes replay.

## Qualification boundary

This surface is generic runtime audit plumbing. It does not qualify a real ML backend, does not add model semantics, and does not make latency, throughput, memory, energy, or quality claims.
