# Kernel-realization planning (elastic-kernel)

Status: **first deterministic vertical slice — implemented and tested.**
Related crates: [`crates/elastic-kernel`](../../crates/elastic-kernel).
Design-adjacent notes: [planner-policy-lifecycle-and-actuation-boundary.md](planner-policy-lifecycle-and-actuation-boundary.md)
(proposal), [kv-resource-factorization.md](kv-resource-factorization.md) (adapter
precedent).

## Problem

An executable kernel is an elastic resource: one logical computation keeps a
stable identity while its physical realization may legitimately change with
hardware capabilities, constraints, and objectives. Before this slice, the
workspace could *declare* such resources ([`ResourceSpec`], EIR) but had no
executable machinery to select among concrete realizations of one.

The first consumer is FLAT-ATTENTION: its fused attention kernels come in
families (portable tiled, subgroup-assisted, vectorized) whose admissibility
depends on device limits. The Elastic layer must be able to answer:

> given this capability snapshot, this declared contract, and this objective
> priority order — which realization may run now, and why?

without ever learning what attention, a tile, or a vendor is.

## What is implemented

Everything below lives in `crates/elastic-kernel` and is covered by unit +
integration tests.

### Capability snapshots (`capability.rs`)

`CapabilitySnapshot` is a normalized, backend-neutral record of execution-
boundary facts: workgroup geometry limits, binding limits, subgroup support
with width range, and tri-state feature reports (`shader-f16`, `matrix-ops`).

Honesty rules enforced by construction:

- unknown ≠ false: [`FeatureSupport::Unknown`] is distinct from
  `Known(false)`, and fingerprints differ;
- subgroup declarations are internally consistent by validation
  (unsupported ⇒ no width range; supported ⇒ non-empty range);
- all mandatory limits are positive.

Snapshots fingerprint deterministically through the framed FNV-1a discipline
of `elastic-eir::Fingerprint`. They record declarations; they never
authenticate the declarer (same trust rule as `elastic-core::CapabilitySet`).

### Candidate requirements (`requirements.rs`)

`KernelRequirements` states what a realization needs from any boundary:
invocation counts (total + per axis), staged workgroup storage bytes,
bind groups, largest storage-buffer binding, optional subgroup dependence
(`Some(min_width)`), and per-feature strength
(`FeatureRequirement::{NotRequired, Required}`).

`check_against(&CapabilitySnapshot)` returns the first typed rejection reason
in a fixed evaluation order. Unknown features produce
`RejectionReason::FeatureUnknown`, deliberately distinct from
`FeatureUnsupported`.

### Candidates (`candidate.rs`)

`KernelCandidate` couples:

- logical identity (`LogicalResourceId`, reused from core);
- realization identity (`RealizationIdentity`);
- schema version;
- requirements;
- the semantic `ContractId` it upholds;
- per-objective `ObjectiveEvidence`.

Evidence has three epistemic variants — `Measured(MeasuredQuantity)` (protocol
tagged), `StaticEstimate(StaticQuantity)` (provably derived static model),
`Unknown` — so a guessed latency can never inhabit the measured type, and
units keep distinct quantities incomparable.

### Deterministic planner (`planner.rs`)

`plan(...)` implements the honest outcome set of the surface model:
`Selected(SelectionRecord) | NoCandidate | InsufficientEvidence |
Unsupported`.

Decision procedure (deterministic end to end):

1. refuse to run if any policy objective has no defined comparison direction
   for built-ins (`Unsupported`); custom objectives are never guessed;
2. filter candidates on logical-resource identity, contract equality, and
   capability feasibility (rejection reasons retained verbatim);
3. sort survivors lexicographically along the policy's ordered objectives.
   Evidence tier dominates magnitude: measured > static > unknown, so
   measurements are never mixed with estimates in one comparison. Within one
   tier and objective, magnitudes compare under the objective's direction;
   ties fall through to the next objective and finally to realization
   identity;
4. honesty gates before declaring success: an all-unknown primary objective
   yields `InsufficientEvidence`; static-only evidence yields
   `InsufficientEvidence` when the policy forbids estimates; measured
   candidates disagreeing on physical units yield `InsufficientEvidence`
   instead of aliasing magnitudes.

`SelectionRecord` carries workload/capability/candidate-set fingerprints, the
selected realization, deterministically sorted rejections, the objective
order, decisive-evidence class, planner version, and a record fingerprint —
identical inputs produce byte-identical records regardless of candidate offer
order. There is no scalarization anywhere: objective order stays lexicographic,
matching the core model's "ordering is the only cross-objective structure"
rule.

### Realization lifecycle (`lifecycle.rs`)

A selection is a recommendation until it survives
`Proposed → Validated → Activated → Verified → CommittedRealization`, each
step requiring its own named attestation (`StageAttestations`, private-field
construction like core's `TransitionAttestations`). Any stage can roll back;
rollbacks record where they stopped and why. A failed compile or parity check
therefore can never become a committed realization, and the states cannot be
collapsed into one boolean because they are different types.

Kernel-realization switching is deliberately modeled as this lifecycle rather
than as `Reinterpret`/`Reencode`/`Recompute`: swapping an executable
implementation transforms no stored data, so none of those data-transition
mechanisms describes it truthfully. Extending the core taxonomy remains future
work (see below).

## What is *not* implemented (deliberately)

- No forecasting, learning, or autotuning: measured evidence arrives from the
  outside; the planner only compares what it is given.
- No serialization/persistence of snapshots or records (follows the current
  workspace policy of deferring serde).
- No new `TransitionMechanism` variant in core for realization swaps. When a
  second consumer exists, a dedicated mechanism (for example "replace
  realization") should be added once, conservatively, with its epoch semantics
  specified.
- No persistent benchmark cache; `SelectionRecord.fingerprint()` is designed
  to key one later.

## Relationship to other layers

| Layer | Responsibility |
| --- | --- |
| SciRust | logical tensor/scientific semantics; representation plans |
| FLAT-ATTENTION | attention kernel IR, candidate generation, WGSL codegen, oracle |
| elastic-kernel (this crate) | generic capability filtering, objective-ordered selection, auditable evidence, lifecycle |
| Runtime/backend | activation, execution, measurement feeding evidence back |

The dependency direction is strict: adapters depend on this crate; this crate
depends only on `elastic-core` + `elastic-eir`; nothing here imports WGPU,
Naga, SciRust, or FLAT types.
