# Boolean decision trace comparison

Status: implemented by `elastic-runtime` as the typed `DecisionTrace::diff` surface.

The comparison API is read-only. It does not replay a decision, validate a transition, call an adapter, or actuate a resource. Its purpose is to explain **what semantic input or outcome changed between two bounded decision traces**.

## Change classes

`DecisionTraceChangeKind` distinguishes four classes:

- `Policy` — logical resource identity, guarded-EIR/guard-policy structural identity, or predicate guard membership changed;
- `Fact` — fact source, observation epoch, resource generation, fact structural identity, or a known truth value changed;
- `Candidate` — candidate classification/rejection detail/magnitude, numeric selection, or a non-unknown stop outcome changed;
- `Unknown` — missing/unknown predicate evidence, unknown candidate scopes/grounding, or an insufficient-evidence outcome changed.

A single comparison may contain several classes. Changes are sorted by stable semantic path and capped by the shared evidence diff-path bound. `DecisionTraceDiff::truncated()` explicitly reports when the cap was reached.

## Determinism

Candidate collections are compared by the typed `(TransitionMechanism, DimensionId)` identity rather than by JSON array position. Unknown guard scopes are normalized as sets for comparison. Therefore reordering semantically equivalent candidate arrays or unknown-scope arrays does not manufacture a difference.

Predicate entries are compared by stable `PredicateKey`. Selected candidates and stop reasons are compared separately as decision outcomes.

## Fingerprint limitation

The guarded-resource and fact-snapshot fingerprints are deliberately non-cryptographic structural identities. A fingerprint change is useful diagnostic evidence and is reported, but a matching fingerprint is not authentication, collision resistance, or authority to replay/actuate. Typed fields are compared independently wherever the trace carries them.

## Diagnostic values

Each `DecisionTraceChange` exposes a semantic path plus optional left/right textual values. Those value strings are diagnostic renderings, not a persistence schema. Persisted interchange remains governed by `elastic-boolean-decision-trace-v1`.
