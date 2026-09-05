# Model-execution cycle evidence v1

`elastic.model-execution.cycle-evidence@1.0.0` is the durable evidence contract for one completed adaptive model-execution control cycle.

Media type:

```text
application/vnd.elastic.model-execution-cycle-evidence.v1+json
```

The model-specific payload is carried inside the existing bounded `elastic-runtime-evidence-v1` envelope with `command = "run"`. This reuses the runtime-owned size, shape, event, commit/rollback, summary, and diff validation instead of creating a parallel evidence container.

## Purpose

`ModelExecutionControllerV1::cycle_with_evidence()` executes the normal trusted runtime cycle first and then captures the resulting evidence against the exact controller contracts and final physical profile rank.

The artifact records what actually happened:

```text
OBSERVE
  -> FORECAST
  -> PLAN
  -> VALIDATE
  -> ACT
  -> VERIFY
  -> COMMIT / ROLLBACK
```

Stages that were not reached remain absent. Evidence is never synthesized to make an incomplete cycle appear complete.

## Identity binding

Every artifact is bound to:

- the logical `resource_id`;
- the exact atomic EIR resource fingerprint produced from that resource id and profile set;
- provider id;
- exact model revision;
- base capability fingerprint;
- correlated profile-set fingerprint;
- resource-envelope policy fingerprint.

Offline revalidation rebuilds the atomic resource from the supplied current `ModelExecutionControllerContractsV1` and rejects a changed resource id, changed resource fingerprint, provider/model mismatch, or stale capability/profile/policy identity.

The fingerprint is structural identity, not authentication or a cryptographic signature.

## Observation evidence

Runtime `Instant` values are process-local and are not serialized as fake wall-clock timestamps.

For each observation, the artifact stores:

- provider/source identity;
- signal identity;
- finite numeric value for a valid observation;
- explicit unsupported state and reason when no value was available;
- `age_nanos`, the monotonic age of the observation relative to the runtime observation snapshot that consumed it.

This preserves the freshness relationship actually used by the control cycle without inventing globally portable clock values.

A valid observation cannot carry an unsupported reason. An unsupported observation cannot carry a numeric fallback and must carry a nonblank reason. `all_signals_valid` is rechecked against the individual observation states during replay validation.

## Forecast and planning evidence

Forecast evidence records:

- available / unsupported / inconclusive status;
- method identity;
- horizon in nanoseconds;
- optional finite confidence;
- diagnostic detail.

Plan evidence records:

- the finite planner-facing context in deterministic signal order;
- the honest planning outcome;
- reasoning text;
- trusted validation state;
- invariant checks.

When the outcome is a candidate, the artifact includes the complete correlated model-execution tuple:

- provider profile id;
- preference rank;
- active expert count;
- expert-width basis points;
- activation-budget basis points.

Replay revalidation requires that tuple to match a currently published exact profile in the bound profile set. For the current atomic model-execution contract, the candidate must also use the `reinterpret` mechanism on `model-execution.profile`.

## Transaction evidence

If the runtime reached physical preparation/application, the artifact records:

- adapter identity;
- target profile rank;
- verification result;
- commit rationale or rollback rationale;
- whether rollback restored invariants;
- final physically observed published profile rank;
- ordered runtime events.

A completed artifact with actuation must have come from a trusted validated candidate, must contain verification evidence, and must terminate in either commit or rollback.

A committed cycle requires:

- actuation evidence;
- passing verification;
- `CommitExecuted` runtime event evidence;
- final physical profile rank equal to the target profile rank.

A rolled-back cycle requires:

- actuation evidence;
- `RollbackExecuted` runtime event evidence;
- restored invariants;
- an observed initial profile rank;
- final physical profile rank equal to that initial rank.

A cycle without actuation cannot claim verification, commit, or rollback.

## Offline validation is not physical replay

`ModelExecutionCycleEvidenceV1::from_json()` and `from_runtime_evidence()` are deliberately read-only.

They answer:

> Is this bounded historical artifact structurally and semantically compatible with these exact current model-execution contracts?

They do **not** answer:

> Is it safe to apply this historical target again now?

No evidence replay method calls `ModelExecutionProfileBackendV1`, performs hardware probing, or invokes `apply_profile`. A future physical replay path would require fresh capability identity, fresh telemetry, fresh action-time validation, verification, and the normal transactional runtime. Historical evidence alone is never an actuation authorization.

## Failure boundary

`cycle_with_evidence()` captures evidence only after a normal runtime cycle returns successfully and the final physical profile rank can be read and validated.

A catastrophic runtime error such as an unrecoverable rollback failure is therefore not currently materialized by this helper as a completed cycle artifact. This contract must not be presented as evidence that every possible runtime failure is durably captured.

## Qualification boundary

This contract proves generic evidence plumbing and fail-closed replay validation. It does not qualify a real ML backend and does not satisfy ElasticXxx's 5/5 requirement by itself.

In particular, the current NNIS audit does not establish a qualified MoE/expert backend implementing elastic expert count, expert width, or activation-budget transitions. Fake/test backends demonstrate the contract lifecycle only.

No latency, throughput, memory, energy, or model-quality improvement is claimed by this evidence format.
