# Boolean survivor-only planning

This library slice closes the input-pool gap in BE6 without replacing the
existing `TransitionPlanner` trait or numeric control laws. It is a planning
contract, not an actuation permit or a performance result.

## Order of operations

`BooleanGuardPlanner::propose_transition_with_context` performs:

1. Check the fact snapshot's existing resource binding and epoch/generation
   freshness through `BooleanGuardPreplanner`.
2. Classify the original guarded resource's admissions using the single EIR
   Boolean evaluator. Missing evidence remains `Unknown`.
3. Reject an unsupported target or stop on false/unknown eligibility before
   invoking the numeric planner.
4. Intersect the eligible set with the configured target.
5. Build a validated EIR planning view through
   `EirResource::restrict_to_candidates`.
6. Pass only that view to the existing numeric planner.
7. Revalidate the returned candidate's grounding, original declaredness,
   exact target and membership in the restricted view.

A planner that previously chose a rejected best candidate and lost an eligible
runner-up now sees only the runner-up. Filtering the final answer alone could
not provide that property.

## Projection contract

The view preserves identity, class, dimensions, invariants, ordered objectives,
observation declarations and diagnostic labels. Only transitions and their
associated capability requirements are reduced. Reconstruction uses the normal
`EirResource::from_parts` structural validator and recomputes the fingerprint;
the original fingerprint is never attached to a different admitted set.

Candidate input order, duplicates and advisory magnitudes do not change the
projected admission set. Invalid or ungrounded candidates are rejected. A fully
grounded complete set reconstructs the original EIR exactly. Empty subsets are
structurally valid, but the runtime wrapper never invokes numeric planning when
no scoped survivor exists.

## Outcome and compatibility rules

- An empty declaration or undeclared exact target returns `Unsupported` without
  calling the planner. This deliberately tightens the earlier wrapper, which
  delegated those cases to the legacy planner.
- Explicit false eligibility returns `NoCandidate`; unknown eligibility returns
  `InsufficientEvidence`.
- `for_transition` and `for_capacity` constrain input and output, not just the
  condition for starting the planner.
- An ungrounded or undeclared custom output, or one outside the configured
  target, returns `Unsupported`. A cached rejected selection cannot escape;
  a cached unknown selection yields `InsufficientEvidence`.
- Accepted magnitudes remain advisory and unchanged. Real adapter bounds and
  semantic invariants still have to be validated before physical effects.

Planners that cache by EIR fingerprint must treat the projected fingerprint as
an input-view identity. Evidence capture and later validation must use the
original guarded resource, not pretend that the projection is its full policy.
Candidate values themselves currently contain no source resource identity;
structural membership checks are not authentication or provenance checks.

The separate numeric `PlanningContext` remains caller-supplied. This slice does
not prove that it was derived from the same observations as `FactSnapshot`.
Cycle-level coherent-context binding and immediate pre-actuation revalidation
remain separate integration work. No new live controller is claimed here.

## Public Rust surface and regression suite

Downstream code still needs only `elastic`. Freshness/epoch/generation types and
`PlanningSubsetError` are re-exported at the facade root. The existing guarded
planner method now supplies the restricted view automatically.

Reproduce the required checks with the repository's pinned toolchain:

```sh
cargo +1.89.0 fmt --all -- --check
cargo +1.89.0 clippy --workspace --all-targets -- -D warnings
cargo +1.89.0 test -p elastic-eir planning_subset
cargo +1.89.0 test -p elastic-downstream --test survivor_planning
cargo +1.89.0 test --workspace
```

The suite covers all four subsets of a two-transition resource, all nine
three-valued eligibility assignments, preserving a valid runner-up, exact scope,
custom cached outputs, ungrounded output borrowing, empty/ungrounded resources,
stale and cross-resource facts, original-EIR evidence capture, and 64 numerical
parity comparisons for Threshold/Headroom controllers under true/absent guards.
These are test specifications; only an executed successful run is validation
evidence. No latency, allocation or end-to-end speedup is claimed. Projection
currently clones metadata and allocates vectors, which must be included in
later BE13 measurements.

## Coordination

Follow `AGENTS.md` and the off-main Boolean roadmap. Keep pending code separate
from merged/qualified evidence, including when required runners are unavailable.
The generic restriction method is reusable by other policy consumers only as a
structural subset operation; it does not import BooleanLab or TDI semantics.
