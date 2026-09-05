# Transactional model-execution backend v1

Status: generic backend integration boundary. This document does not claim that a concrete TDI, ASSR, NNIS, CUDA, or other production backend currently implements it.

## Purpose

The model-execution work now has three distinct layers:

1. qualified axis capabilities and correlated profile sets;
2. hardware-guided profile selection and one atomic runtime transition;
3. physical execution through ElasticXxx's existing transaction lifecycle.

`ModelExecutionProfileBackendV1` and `TransactionalModelExecution<B>` implement the third integration seam without creating a second model-specific transaction protocol.

The lifecycle remains the generic ElasticXxx lifecycle:

`VALIDATE -> PREPARE -> ACTUATE -> VERIFY -> COMMIT / ROLLBACK`

## Backend-owned primitives

A concrete backend implements `ModelExecutionProfileBackendV1` and provides:

- backend name;
- provider id;
- exact model revision;
- base capability fingerprint;
- correlated profile-set fingerprint;
- current physical profile rank;
- side-effect-free action-time target validation;
- complete-profile apply;
- post-action verification;
- previous-profile restoration.

These methods are the backend's physical/domain boundary. ElasticXxx does not implement the model switch on the backend's behalf.

## Runtime-owned authority

`TransactionalModelExecution<B>` retains authority over:

- exact resource fingerprint matching;
- provider/model/capability/profile-set identity matching;
- target-rank lookup in the exact profile set;
- action-time revalidation;
- one prepared transaction at a time;
- actuation identity and target checks;
- immediate pre-apply feasibility revalidation;
- post-action current-profile verification;
- commit only after successful verification;
- rollback to the previous complete profile when verification or commit fails;
- auditable commit and rollback records.

The wrapper also implements `Observer`. All clones share the same backend behind `Arc<Mutex<...>>`, so planner telemetry and physical actuation read and mutate one state rather than independent shadow copies.

## Current profile observation

The observer emits `model-execution.current-profile-rank` only when the backend returns a rank published by the exact correlated profile set.

If backend observation fails or the backend reports an unpublished rank, the signal is emitted as unsupported and no numeric fallback is fabricated.

## Validation

Before a plan can actuate, the wrapper verifies that:

- the plan targets the exact atomic EIR resource controlled by the adapter;
- the candidate dimension is `model-execution.profile`;
- the target magnitude fits `u32` and names a published profile rank;
- the backend's provider, model revision, capability fingerprint, and profile-set fingerprint still match;
- the backend's current profile is also published;
- the backend revalidates the target profile during trusted validation and preparation.

Only then are invariant checks returned to the generic runtime validator.

## Prepare-to-actuate revalidation

Preparation records both the previous and target profile ranks, but a successful prepare does not permanently authorize the later physical mutation. External backend state can change between preparation and application.

Immediately before `apply_profile`, `actuate` therefore:

1. rechecks backend/provider/model/capability/profile-set identity;
2. resolves the exact prepared target profile again;
3. invokes `validate_profile(target)` again;
4. calls `apply_profile(target)` only if that final validation succeeds.

If the backend rejects this last validation, no apply call is made. The generic runtime treats the actuation failure as a fail-closed transaction and follows its rollback path for the prepared state.

`validate_profile` must therefore be side-effect free and evaluate current physical feasibility each time it is called. A backend must not cache a prior success and treat it as perpetual authorization.

## Actuation and verification

After the immediate pre-apply validation passes, `actuate` resolves the target rank back to the complete correlated profile and calls the backend's `apply_profile` method.

`verify` requires both:

- backend-specific verification to pass;
- the backend's current physical profile rank to equal the target rank.

A backend-specific verification pass is therefore insufficient if the observable physical state does not match the target.

## Commit and rollback

`commit` rechecks backend identity, backend verification, and current target rank before clearing the prepared transaction.

`rollback` resolves the previously active rank back to the complete previous profile, calls `restore_profile`, then verifies both backend semantics and the restored current rank. The rollback record reports `invariants_restored = true` only when both checks pass.

This behavior uses the generic runtime's existing failure handling. In particular, an actuation error or non-passing verification becomes a rollback path rather than a silent commit.

## Public implementation surface

Downstream code can implement the backend trait from the public `elastic` crate:

```rust
use elastic::{
    Fingerprint, ModelExecutionProfileBackendV1, ModelExecutionProfileV1,
    VerificationResult,
};

# #[derive(Debug)]
# struct BackendError;
# impl std::fmt::Display for BackendError {
#     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
#         f.write_str("backend error")
#     }
# }
# impl std::error::Error for BackendError {}
# struct Backend {
#     capability: Fingerprint,
#     profiles: Fingerprint,
#     current: u32,
# }
impl ModelExecutionProfileBackendV1 for Backend {
    type Error = BackendError;

    fn name(&self) -> &str { "my-backend" }
    fn provider_id(&self) -> &str { "my-provider" }
    fn model_revision(&self) -> &str { "model-revision-123" }
    fn capability_fingerprint(&self) -> Fingerprint { self.capability }
    fn profile_set_fingerprint(&self) -> Fingerprint { self.profiles }
    fn current_profile_rank(&self) -> Result<u32, Self::Error> { Ok(self.current) }

    fn validate_profile(&self, _target: &ModelExecutionProfileV1) -> Result<(), Self::Error> {
        // Re-read whatever live backend state determines whether the complete
        // target remains physically admissible. Do not mutate backend state here.
        Ok(())
    }

    fn apply_profile(&mut self, target: &ModelExecutionProfileV1) -> Result<(), Self::Error> {
        self.current = target.preference_rank();
        Ok(())
    }

    fn verify_profile(
        &self,
        target: &ModelExecutionProfileV1,
    ) -> Result<VerificationResult, Self::Error> {
        if self.current == target.preference_rank() {
            Ok(VerificationResult::Pass)
        } else {
            Ok(VerificationResult::Fail { detail: "profile mismatch".into() })
        }
    }

    fn restore_profile(&mut self, previous: &ModelExecutionProfileV1) -> Result<(), Self::Error> {
        self.current = previous.preference_rank();
        Ok(())
    }
}
```

The example is an interface illustration, not a real hardware backend.

## Reuse across Memorithm projects

The interface is designed so specialized repositories can supply backend semantics without moving those semantics into ElasticXxx. A future NNIS adapter could own CUDA/model-state operations; a TDI/ASSR adapter could own ASSR-specific profile semantics; SLHAv2 could expose its own complete profile operations if appropriate.

No such implementation is claimed to exist until the corresponding repository publishes and tests it.

The current NNIS audit does not establish a qualified MoE/expert execution surface, so this document must not be read as claiming NNIS already implements elastic expert count, width, or activation profiles.

## Non-goals

This slice does not:

- implement a real CUDA or accelerator backend;
- define ASSR/TDI model semantics;
- train elastic experts;
- infer safe profile transitions;
- bypass backend action-time validation;
- bypass ElasticXxx verification or rollback;
- claim latency, throughput, memory, energy, or accuracy improvements.
