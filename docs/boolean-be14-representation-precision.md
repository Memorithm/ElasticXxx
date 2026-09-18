# BE14e representation / precision Boolean admission

Status: **active; planning and durable Boolean-trace slices present, not domain-qualified**.

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

`screen_with_trace` adds a versioned v2 evidence envelope while preserving the original v1 planning report unchanged. Candidates that reach the numeric Boolean guard retain strict bounded `DecisionTrace/v1` JSON bound to the exact observation epoch and resource generation. Structural or trusted-capability rejection records an explicit null trace rather than fabricating a precision fact for a guard that was never evaluated. Decoding/replaying this evidence remains purely explanatory and cannot validate or actuate a representation transition.

A facade-only downstream host-memory consumer now binds that BE14e decision to a concrete lossless u16 little-endian → u16 big-endian re-encode. The Boolean layer selects only the declared fixed-width representation candidate; the existing trusted representation validation remains authoritative before `TransactionalKvPageV1` stages or applies bytes. The same fixture verifies semantic equality before commit, injects a post-actuation semantic failure and restores the exact source bytes on rollback, and compares the guarded committed result against an otherwise identical unguarded transition. This is correctness/integration evidence only: changing byte order at the same 16-bit width is not a compression or numerical-quality result.

This slice therefore still does not close BE14e. Portable benchmark evidence remains required before any performance claim, and later representation families (variable-width, codebook, mixed precision, low-rank, residual, or lossy codecs) require their own honest contracts and evidence rather than inheriting this fixed-width fixture.

No latency, throughput, memory saving, hardware acceleration, energy, numerical-quality, or model-quality claim is made by this contract.
