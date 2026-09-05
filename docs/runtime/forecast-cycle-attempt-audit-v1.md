# Forecast-aware cycle-attempt audit v1

`ForecastCycleAttempt` preserves the failure phase of one forecast-aware ElasticXxx cycle without creating another transaction executor.

The execution ownership remains:

```text
observer
  -> forecaster
  -> forecast gate / planner
  -> Runtime::cycle_attempt()
  -> existing trusted Runtime::cycle_with_sink()
```

`ForecastRuntime::cycle()` is implemented by converting `cycle_attempt()` back to the existing `Result<ForecastCycleResult, RuntimeError>` surface. The attempt API therefore adds audit context without defining alternate success/error semantics.

## Outcomes

`ForecastCycleAttempt` has two outcomes:

- `Completed(ForecastCycleResult)` — forecast orchestration and the trusted runtime both completed normally;
- `Failed(ForecastCycleFailure)` — the attempt failed before or during the trusted transaction.

Both payloads are boxed so the public enum remains small despite the richer audit structures.

## Forecast failure

`ForecastCycleFailure::Forecast` means the forecaster returned `RuntimeError` before the trusted runtime was entered.

It preserves:

- the exact EIR resource being considered;
- the raw `ObservationSnapshot` supplied to the forecaster;
- the authoritative forecast error.

This phase cannot contain transaction audit because no trusted transaction was entered. The attempt must not fabricate plan, actuation, verification, commit, or rollback state.

## Transaction failure after forecast

`ForecastCycleFailure::Transaction` means forecasting completed and the trusted runtime later returned an error.

It preserves:

- the raw observation snapshot that was supplied to the forecaster;
- the successful forecast that gated planning;
- the `ForecastGenerated` event;
- the exact `CycleFailure` returned by `Runtime::cycle_attempt()`, including attempted EIR resource, authoritative runtime error, and ordered transaction events emitted before failure.

The forecast-input snapshot is specifically the snapshot consumed by the forecaster. It must not be described as the later trusted-runtime observation snapshot: the runtime receives a frozen observer and constructs its own snapshot internally.

## Observe-only mode

`ObserveOnly` deliberately skips forecasting. Its attempt path delegates directly to `Runtime::cycle_attempt()`:

- completed attempts contain no forecast or forecast event;
- failed attempts are represented as transaction failures with no forecast input, forecast, or forecast event.

This preserves the existing semantic meaning of observe-only mode rather than inventing a forecast stage.

## Failure evidence boundary

This slice combines two previously separate audit boundaries:

1. input evidence for a failed forecast before physical transaction entry;
2. forecast evidence plus `CycleFailure` when the trusted transaction fails.

It still does not reconstruct structured plan/actuation/verification objects that the trusted runtime does not return when a `RuntimeError` escapes. Those objects remain absent rather than being parsed from diagnostic text.

The bounded `ForecastRuntime::run` path also still returns `RuntimeError` on a failed cycle and does not yet return a run-attempt object containing previously completed cycles plus the failed cycle attempt. That is a separate follow-on gap.

## Model-execution relationship

The assembled model controller can use this generic attempt surface as the foundation for a later model-specific catastrophic-attempt artifact bound to exact provider/model/capability/profile-set/policy identities.

Such an artifact must remain distinct from `ModelExecutionCycleEvidenceV1`, which represents completed cycles, and from any future physical replay authorization.

No historical success or failure evidence authorizes a new physical action.

## Qualification boundary

This is generic runtime/forecast audit plumbing. It does not qualify a real model backend, does not define ASSR/TDI expert semantics, and makes no latency, throughput, memory, energy, or quality claim.
