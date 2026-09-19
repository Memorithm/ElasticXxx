# Boolean survivor-only planning

This library slice closes the input-pool gap in BE6 without replacing the
existing `TransitionPlanner` trait or numeric control laws. It is a planning
contract, not an actuation permit or a performance result. Implementation on a
PR branch is not qualified or merged evidence until exact-head CI succeeds.

## Order of operations

`BooleanGuardPlanner::propose_transition_with_context` performs:

1. Check the fact snapshot's resource binding and epoch/generation freshness
   through `BooleanGuardPreplanner`.
2. Classify the original guarded resource's admissions using the single EIR
   Boolean evaluator. Missing evidence remains `Unknown`. The pruning report
   retains its complete guarded-EIR source, not only mechanism/dimension pairs.
3. Reject an unsupported target or stop on false/unknown eligibility before
   invoking the numeric planner.
4. Intersect the eligible set with the configured target.
5. Build a validated EIR planning view through
   `EirGuardedResource::restrict_to_eligible(&report, &candidates)`, which verifies
   the report's source and the eligibility of every requested candidate.
6. Pass only that view to the existing numeric planner.
7. Revalidate the returned candidate's grounding, original declaredness,
   exact target and membership in the restricted view.

A planner that previously chose a rejected best candidate and lost an eligible
runner-up now sees only the runner-up. Filtering the final answer alone could
not provide that property.

## Source binding and projection

The public boundary requires a `TransitionPruningReport` derived for exactly
the same `EirGuardedResource`. Full structural equality includes logical
identity, EIR content and guard policy; non-cryptographic fingerprint equality
alone is insufficient. The default report has no source and is rejected, even
for an empty projection. A report for another resource, a previous guard policy
or changed resource metadata produces `PlanningSubsetError::SourceMismatch`.

A candidate must also be declared, capability-grounded and inside the bound
report's eligible partition. Declared but rejected/unknown candidates cannot be
added back. The raw `EirResource::restrict_to_candidates` helper is crate-private;
a compile-fail doctest prevents external callers bypassing the guarded boundary.

The view preserves identity, class, dimensions, invariants, ordered objectives,
observation declarations and diagnostic labels. Only transitions and their
associated capability requirements are reduced. Reconstruction uses the normal
`EirResource::from_parts` structural validator and recomputes the fingerprint;
the original fingerprint is never attached to a different admitted set.

Candidate input order, duplicates and advisory magnitudes do not change the
projected admission set. A fully grounded, fully eligible complete set
reconstructs the original EIR exactly. Empty subsets are structurally valid,
but the runtime wrapper never invokes numeric planning when no scoped survivor
exists.

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
Candidate values themselves still have no source resource identity. A raw
candidate is not a transferable eligibility proof; only the correctly bound
report determines membership in the eligible pool of the current declaration.

The pure EIR report does not establish observation freshness or authenticate
external data. Runtime code must regenerate it from fresh resource-bound facts;
`BooleanGuardPlanner` does so on every call. The separate numeric
`PlanningContext` remains caller-supplied, and this slice does not prove that it
was derived from the same observations as `FactSnapshot`. Cycle-level coherent
context/fact binding and immediate pre-actuation revalidation remain separate
integration work. No new live controller is claimed here.

## Public Rust surface and regression suite

Downstream code still needs only `elastic`. Freshness/epoch/generation types and
`PlanningSubsetError` are re-exported at the facade root. The existing guarded
planner method supplies the restricted view automatically.

Reproduce the required checks with the repository's pinned toolchain:

```sh
cargo +1.89.0 fmt --all -- --check
cargo +1.89.0 clippy --workspace --all-targets -- -D warnings
cargo +1.89.0 test -p memorithm-elastic-eir planning_subset
cargo +1.89.0 test -p memorithm-elastic-eir --test source_bound_planning
cargo +1.89.0 test -p elastic-downstream --test survivor_planning
cargo +1.89.0 test --workspace
```

The original suite covers all four subsets of a two-transition resource, all
nine three-valued eligibility assignments, preserving a valid runner-up, exact
scope, custom cached outputs, ungrounded output borrowing, empty/ungrounded
resources, stale and cross-resource facts, original-EIR evidence capture, and
64 numerical parity comparisons for Threshold/Headroom controllers under
true/absent guards.

Six additional source-binding regression tests cover foreign resources with
identical transition pairs, changed guard policy with identical base EIR,
changed resource metadata under the same identity, unbound default reports,
rejected/unknown candidate reinsertion, and valid duplicate/magnitude handling
without borrowed grounding. The facade test uses the same source-bound public
projection. These are written test specifications, not execution evidence.

No latency, allocation or end-to-end speedup is claimed. Reports now clone the
source guarded declaration, and projection clones metadata and allocates
vectors. All these costs must be included in later BE13 measurements.

## Coordination

Follow `AGENTS.md` and the off-main Boolean roadmap. Keep pending code separate
from merged/qualified evidence, including when required runners are unavailable.
The P1 source-binding review is addressed by code and regression specifications;
its qualification still requires the corrected exact head to pass CI. Reuse by
other consumers must preserve this bound-report contract and runtime freshness,
not copy BooleanLab or TDI scientific semantics into the production runtime.
