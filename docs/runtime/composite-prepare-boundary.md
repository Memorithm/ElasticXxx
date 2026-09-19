# ELANG4b composite prepare and pre-actuation checkpoints

Status: trusted multi-resource validation/checkpoint/prepare boundary. This slice
**does not perform physical actuation**.

ELANG4a introduced `CompositePlanEnvelope`: several candidate-bearing
single-resource plans bound to one exact ELANG3 group and ordered according to
its complete dependency topology. ELANG4b adds the next trust boundary without
collapsing prepare and actuation into one operation.

## Phase barriers

`prepare_composite_plan()` is deliberately split into three global phases:

```text
ALL VALIDATE
    ↓
ALL CAPTURE PRE-ACT STATE
    ↓
ALL PREPARE
    ↓
STOP — no physical effect in ELANG4b
```

The runtime does not checkpoint or prepare any resource until every targeted
subplan has passed the ordinary trusted `TransactionalActuator::validate`
boundary and `validate_with_checks` has authorized that plan.

It then captures rollback-relevant state for every resource before the first
prepare. Only after every capture succeeds may concrete preparations begin.

This prevents a later validation or checkpoint failure from occurring after
some other resource has already entered a prepared state.

## Backend contract

`CompositePrepareBackend` extends the existing `TransactionalActuator` boundary
with four pieces of composite-specific information/behavior:

- exact logical `resource_id()`;
- `capture_pre_act_state()`;
- `abort_prepare()` for a preparation that never reached physical actuation;
- `release_pre_act_state()` when a checkpoint is no longer needed.

The generic runtime never attempts to copy arbitrary backend state. A
`CompositePreActState` is an opaque binding token containing resource identity,
adapter identity, a backend generation and a non-cryptographic state
fingerprint. The backend remains responsible for retaining the concrete bytes,
handles or other rollback material associated with that token.

The exact backend set must equal the resources targeted by the composite plan:
missing, duplicate or foreign bindings fail closed before trusted validation.
Every prepared subplan separately retains the trusted adapter identity that
actually produced it. Opaque checkpoint/actuation metadata is checked against
that identity before a successful prepared envelope is returned. If a backend
returns malformed metadata during capture/prepare, cleanup is still routed to
the known producer rather than trusting the malformed token.

## Successful result

`CompositePreparedEnvelope` contains, in forward ELANG4a execution order:

- each trusted `ValidatedPlan`;
- its `CompositePreActState`;
- the concrete prepared `Actuation`;
- the source `CompositePlanEnvelope` fingerprint.

A successful prepared envelope still grants **no physical actuation authority**.
ELANG4c must independently define coordinated actuation, verification,
commit/rollback and partial-failure semantics.

## Failure cleanup

Checkpoint or prepare failures are fail-closed.

- Capture failure releases every earlier checkpoint in reverse order.
- Prepare failure aborts every successful earlier preparation in reverse order,
  then releases every checkpoint in reverse order.
- A prepared actuation whose validated plan or adapter identity does not match
  the expected binding is treated as a prepare failure and is unwound.
- Cleanup continues after an individual cleanup error; all failures are retained
  in `CompositePrepareFailure::cleanup_failures()`.

`abort_composite_prepare()` provides the explicit cancellation path before
physical actuation. The prepared envelope is consumed, making abort a one-shot
state transition at the public API boundary.

If `abort_prepare()` fails for a resource, that resource's checkpoint is **not**
released. The failure returns a `CompositePrepareRecoveryEnvelope` retaining the
actuation+checkpoint pair. `retry_composite_prepare_cleanup()` consumes that
recovery envelope and retries only the still-outstanding operations. Likewise,
if abort succeeded but checkpoint release failed, recovery retains a
release-only token so retry does not invoke abort a second time.

Cleanup also rechecks adapter identity. A backend with the same logical
`resource_id` but a different adapter name never receives another adapter's
opaque actuation/checkpoint state. Binding failure returns all prepared state as
linear recovery data for retry with the correct backends.

## Deliberate non-goals

ELANG4b does not:

- call `TransactionalActuator::actuate`;
- call post-actuation `verify`;
- commit any sub-resource;
- invoke physical rollback;
- claim distributed or crash-safe atomicity;
- define cancellation after the first physical effect;
- interpret ELANG3 cross-resource contracts as domain semantics.

Tests use backends whose `actuate`, `verify`, `commit` and physical `rollback`
methods panic. The prepare/checkpoint test suite therefore fails immediately if
this boundary accidentally crosses into ELANG4c behavior.
