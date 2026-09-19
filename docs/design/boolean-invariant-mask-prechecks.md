# Compiled Boolean invariant prechecks (BE7 slice)

Status: implementation candidate; qualification requires the exact-head Rust CI.
This slice is independent of PR90 (trusted-check conflicts), PR91 (macro), and
PR92 (survivor-only planning). Their tests and merges remain separate gates.

## Public surface

Applications need only the `elastic` facade:

```rust,ignore
use elastic::runtime::invariant_precheck::CompiledInvariantPrecheck;

let compiled = CompiledInvariantPrecheck::compile(&plan, &bindings)?;
let summary = compiled.evaluate_summary(&plan, &facts, &freshness)?;
let detailed = compiled.evaluate(&plan, &facts, &freshness)?;
```

`plan`, `bindings`, `facts`, and `freshness` are explicit caller inputs. This
snippet describes composition, not a live controller or runnable standalone
program. The downstream compile test constructs the layout through the facade;
the runtime integration tests exercise evaluation. Root-level facade exports
for core freshness types are tracked separately in PR92.

The scalar `precheck_plan_invariants` remains supported. It and the compiled
path share binding validation, resource-source checks, freshness validation,
and the existing invariant applicability rule. No new dependency is added.

## Compilation and identity

Compilation requires a declared, capability-grounded candidate. It selects the
invariants applicable to that candidate in canonical resource order, resolves
their stable predicate keys, and allocates a slot for each applicable invariant.
A missing binding still occupies a slot and evaluates to Unknown. Multiple
invariants bound to one predicate remain distinct slots. Duplicate bindings for
one invariant and oversized binding lists are errors, including duplicates
outside the candidate's dimension scope.

The optional compiled path supports at most 64 applicable invariants, including
missing bindings. The limit applies AFTER dimension filtering. Exceeding it
returns a typed error; it does not truncate, silently fall back, or weaken the
scalar path. The existing binding-list limit remains unchanged.

A layout owns a copy of the original plan. Reuse checks full EIR equality,
exact candidate equality including target magnitude, and the signal/value bit
patterns of the numeric context. It therefore rejects changed resource content
even under the same logical identity, changed scope/mechanism/magnitude, changed
context, and a candidate replaced by a no-op. Signed zero is distinguished.
Human-readable reasoning text is not semantic identity.

No hash equality substitutes for these checks. This is same-process structural
binding, not origin authentication of a candidate or a fact.

## Evaluation

Every evaluation rechecks the snapshot's resource binding, observation epoch,
and resource generation using the scalar path's shared helper. Facts are then
read afresh. A previous True never survives merely because the layout is reused.
Even an empty applicable set requires valid resource provenance and freshness.

Let R be the required-slot mask, T the known-true mask, and F the known-false
mask. Unknown slots are `U = R & !(T | F)`:

- `F & R != 0`: Rejected.
- Otherwise `(T & R) == R`: Passed.
- Otherwise: InsufficientEvidence.

This is the strong-Kleene conjunction already used by the scalar path. False
has priority over Unknown. An empty applicable set is vacuously Passed only
after provenance checks, matching the scalar contract.

`evaluate_summary` returns private-field masks and status without building a
new diagnostic map/vector on its success path. `evaluate` additionally clones
the ordered invariant/key entries into the existing `InvariantPrecheckReport`.
The compiled layout itself allocates and clones the source plan. No latency,
allocation-count benchmark, or end-to-end speedup is claimed by this PR.

## Trust boundary and remaining work

Passed does NOT create a `ValidatedPlan`, satisfy a missing trusted check, or
permit actuation. The existing adapter validation and post-actuation
verification/rollback remain authoritative. PR90's conflicting-trusted-check
fix is not replaced by this feature.

The compiled LAYOUT is bound to a candidate; the existing FactSnapshot still
attests only caller-supplied derivation/source, epoch, and optional resource
generation. This module does not prove that a fact describes the target state
of that candidate or that numeric context and facts came from one acquisition.
A live guarded controller and candidate-specific source attestation still need
separate contracts and qualification. Historical summaries are diagnostic data,
not reusable authorizations. There is no new persisted decoder in this slice.

## Regression specifications

- Exact detailed-report parity for 27 three-valued assignments times 8 binding
  subsets times 8 materialized-fact subsets: 1,728 combinations.
- True/False/Unknown mask disjointness and complete coverage of required slots.
- All 64 bits including bit 63, shared-key aliases, and typed rejection at 65.
- Capacity counted after invariant scoping; unrelated negative facts ignored.
- Duplicate and oversized bindings rejected with scalar-compatible rules.
- Shared failures for missing/foreign resource binding, stale/future epochs,
  stale generation and missing generation evidence.
- Layout reuse denied after plan/resource/candidate/magnitude/context changes;
  diagnostic-only text changes remain permitted.
- Fresh snapshot reuse cannot cache True; successful prechecks still fail
  trusted validation with absent checks.
- Empty invariant sets, no-op plans and ungrounded candidates.
- Facade-only downstream construction without an internal-crate dependency.

Required commands (from the repository root):

```bash
cargo +1.89.0 fmt --all -- --check
cargo +1.89.0 clippy --workspace --all-targets -- -D warnings
cargo +1.89.0 test -p memorithm-elastic-runtime --test compiled_invariant_precheck
cargo +1.89.0 test -p elastic-downstream --test compiled_invariants
cargo +1.89.0 test --workspace
```

A separate Python calculation of the mask rule covered the same 1,728 abstract
truth/binding/presence combinations without disagreement during development.
That is algebraic cross-checking only, not execution of this Rust code, rustfmt,
Clippy, workspace tests, or a substitute for official CI. Rust was unavailable
in the coding session; no local Rust execution is claimed.

## Reuse assessment

The concrete consumer in this slice is ElasticXxx's invariant precheck API and
its facade-only downstream crate. BooleanLab could independently qualify the
mask equivalence; TDI could later consume a versioned generic precheck contract
on permitted non-final surfaces. Neither external integration is claimed here.
Do not copy the evaluator into either research bench or change their scientific
protocols to work around absent qualification. Controller integration, persisted
evidence and performance qualification remain open BE7/BE8/BE13/BE14 work.
