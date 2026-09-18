# BE14e representation / precision Boolean admission

Status: **active first slice; not domain-qualified**.

This slice adds a versioned, planning-only Boolean front end for fixed-width representation candidates. It is deliberately narrower than “precision” in the scientific sense: the numeric quantity is a declared scalar storage width, not an accuracy, information-content, model-quality, entropy, or error guarantee.

## Contract

The resource declaration must explicitly observe the custom signal `representation-required-precision-bits`. Its unit is `declared-bits-per-scalar`. The stable predicate is `elastic.representation-precision::declared-precision-floor-satisfied`.

A candidate is considered only when:

1. its target representation/schema and mechanism are admitted by the existing `RepresentationalDeclaration`;
2. the current trusted `CapabilitySet` supports the exact derived target state;
3. the precision-floor observation is present, valid, finite, fresh (at most one second old), bit-identical to the planning-context value, an exact positive integer, and no greater than the candidate's declared fixed width; and
4. the compiled Boolean guard leaves the representation transition eligible.

Missing, stale, non-finite, fractional, context-mismatched, or otherwise unusable numeric evidence is `Unknown`. An `Unknown` preferred candidate blocks later candidates rather than silently changing representation semantics. A target absent from the declaration or trusted capability snapshot is conclusively ineligible.

The candidate's `declared_precision_bits` is policy metadata only. Variable-width, codebook, entropy-coded, mixed-precision, low-rank, residual, or other representations that cannot honestly be described by one fixed scalar width must use a different qualified contract instead of inventing a number.

## Authority boundary

`BooleanRepresentationPrecisionPreplannerV1` never mutates a `VersionFrontier` and never actuates. Its `selected_transition` method returns an **unvalidated** structural `RepresentationTransition`. The existing trusted boundary must still revalidate current capabilities and exact mechanism attestations immediately before any actuation. Tests explicitly verify that Boolean `True` cannot bypass the required re-encoder attestation.

This slice therefore does not close BE14e. Remaining qualification work includes durable decision-trace binding, a real representation/precision consumer with authoritative validation/VERIFY/rollback where actuation exists, an unguarded differential baseline, and benchmark evidence before any performance claim.

No latency, throughput, memory saving, hardware acceleration, energy, numerical-quality, or model-quality claim is made by this contract.
