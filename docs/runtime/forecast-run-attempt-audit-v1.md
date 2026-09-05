# Bounded forecast run attempt audit v1

`ForecastRuntime` exposes a bounded-run attempt surface for callers that need to retain audit state when a forecast-aware control loop fails before normal completion.

The trusted transaction state machine is unchanged. Planning, validation, actuation, verification, commit, and rollback remain owned by `Runtime`. Forecast execution still flows through `ForecastRuntime::cycle_attempt()`, which delegates the trusted transaction to `Runtime::cycle_attempt()`.

## Public types

`ForecastRunAttempt` has two outcomes:

- `Completed(ForecastRunResult)` for a normal one-shot/bounded completion or cooperative cancellation;
- `Failed(ForecastRunFailure)` when setup or a cycle fails.

`ForecastRunFailure` distinguishes two phases:

- `Setup`: the run schedule was invalid before the control loop started. No cycle is fabricated.
- `Cycle`: zero or more earlier `ForecastCycleResult` values completed, followed by one exact `ForecastCycleFailure`.

A cycle failure retains:

- every completed cycle in execution order;
- the run-level `ControlLoopStarted` event;
- events from every completed cycle;
- the failed cycle's available forecast/runtime events;
- an explicit `ErrorEncountered` event;
- an explicit `ControlLoopStopped` event.

The failure path does not synthesize `RollbackExecuted`, `CommitExecuted`, or `CycleCompleted` when the trusted runtime did not emit them.

## Single loop implementation

`ForecastRuntime::run_with_clock_attempt()` owns the bounded loop. Existing methods are compatibility wrappers:

- `run()` calls `run_attempt().into_result()`;
- `run_with_clock()` calls `run_with_clock_attempt().into_result()`.

This preserves the historical `Result<ForecastRunResult, RuntimeError>` contract while making failure audit state available to callers that opt into the attempt API.

`ForecastController::run_attempt()` exposes the same behavior for the owned high-level controller.

## Failure semantics

The authoritative error is available through `ForecastRunFailure::error()` and `ForecastCycleFailure::error()`.

`into_result()` returns that same error. The audit wrapper does not replace backend, forecast, validation, actuation, verification, commit, or rollback errors with a new category.

A setup failure contains no fake cycles or control-loop start event because the schedule failed before the loop began.

A failed cycle keeps earlier completed cycles but is not inserted into the completed-cycle vector. Its partial audit remains represented by `ForecastCycleFailure`.

## Scope limits

This contract does not:

- reconstruct structured transaction fields that an escaped `RuntimeError` did not return;
- claim that partial physical effects were restored when rollback failed;
- authorize replay from historical audit data;
- add model-specific E/S/A semantics;
- add a real hardware/model backend;
- make performance or quality claims.

Model-execution-specific durable failure envelopes can build on this generic retained run state, but must remain separately versioned and contract-bound.
