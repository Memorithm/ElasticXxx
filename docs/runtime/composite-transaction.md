# ELANG4c coordinated composite transaction

Status: typed runtime contract for coordinated multi-resource
`ACT -> VERIFY -> COMMIT-or-RESTORE` over an ELANG4b
`CompositePreparedEnvelope`.

This layer does not claim a distributed ACID transaction across arbitrary
external systems. Its atomicity claim is narrower and explicit: **every backend
participating in one composite transaction must retain an exact pre-actuation
checkpoint and guarantee restoration from that checkpoint until the composite
runtime releases it**. That restoration guarantee must remain valid even after
a local `commit()` succeeds.

## Trust boundary

A resource joins ELANG4c through `CompositeTransactionBackend`, which extends
`CompositePrepareBackend` and therefore keeps the exact resource, adapter and
backend-instance identity established during ELANG4b.

The additional contract is:

```text
restore_pre_act_state(actuation, checkpoint, reason)
    -> RollbackRecord { invariants_restored = true }
```

A backend that cannot restore after a successful local commit must not implement
this interface. The generic runtime will not infer reversibility from an adapter
name, a resource identifier, or a normal single-resource rollback method.

## Forward lifecycle

For a prepared composite envelope, `execute_composite_transaction()` performs:

```text
BIND exact backend instances
  -> ACT all resources in ELANG4a required-first order
  -> VERIFY all resources
  -> COMMIT all resources
  -> RELEASE pre-actuation checkpoints in reverse order
```

No local commit starts until every resource has been actuated and every
verification passed.

A successful `CompositeCommitReport` is emitted only after every local
`commit()` succeeded. Its commit records therefore describe the same composite
attempt.

## Failure and restoration

Any error before global completion enters reverse-order recovery.

For entries that were never actuated:

```text
abort_prepare -> release checkpoint
```

For entries whose actuation may have started — including an `actuate()` call
that returned an error — and for entries whose local commit already succeeded:

```text
restore_pre_act_state -> require invariants_restored=true -> release checkpoint
```

This conservative treatment assumes an actuation error may have left a partial
physical effect.

If every resource is restored and every checkpoint is released, the failure has
`CompositeFailureDisposition::RolledBack`.

If any restoration/cleanup cannot be proven complete, the failure has
`CompositeFailureDisposition::RecoveryRequired` and returns a **linear**
`CompositeTransactionRecoveryEnvelope`. The token records the exact next action
per backend (`AbortPrepare`, `RestorePreActState`, or `ReleaseCheckpoint`) so a
retry does not repeat a cleanup action already proven successful.

`retry_composite_transaction_recovery()` consumes that token and only retries
outstanding work.

## Cancellation

A `CancellationToken` is checked:

- before the first physical actuation;
- before each subsequent actuation;
- between actuation and verification;
- before every local commit while the reversible commit window remains open.

Cancellation never produces a partial successful result. Prepared-only entries
are aborted; mutated or locally committed entries are restored from their exact
checkpoints. If restoration cannot complete, cancellation returns the same
`RecoveryRequired` state as other failures.

## Commit failure after earlier local commits

A later local commit can fail after an earlier backend already returned a
successful `CommitRecord`. ELANG4c does **not** declare the earlier commit final
at that point. All mutated resources are restored in reverse order using their
pre-actuation checkpoints.

This is why `CompositeTransactionBackend::restore_pre_act_state` is stronger
than the ordinary single-resource transaction API.

## Post-commit checkpoint cleanup

Checkpoint release happens only after every local commit succeeded. A release
failure at this point does not turn an already successful composite commit into
a rollback result: visible state is globally committed.

Instead, `CompositeCommitReport` may carry a linear
`CompositeCommitCleanupEnvelope`. `retry_composite_commit_cleanup()` retries
only checkpoint release. It never actuates, verifies, commits, or restores
visible resource state.

## Backend identity

Every prepared entry remains bound to:

- logical resource ID;
- human-readable adapter name;
- backend-issued instance identity;
- checkpoint resource/adapter/instance identity;
- prepared actuation adapter identity.

A replacement backend that happens to use the same resource ID or adapter name
cannot consume another instance's checkpoint or recovery token.

## What ELANG4c still does not do

This contract intentionally does not yet add:

- durable crash-recovery persistence across process restart;
- distributed consensus or two-phase commit across independent services;
- cross-process locking;
- automatic backend capability discovery;
- DSL syntax for transaction policy;
- performance claims.

Those require separate evidence and must not be inferred from the in-process
reversible transaction contract.
