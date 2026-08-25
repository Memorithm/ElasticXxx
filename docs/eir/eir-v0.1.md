# EIR v0.1 — Elastic Intermediate Representation

**Status:** normative for the implemented code in `crates/elastic-eir`
(schema version 1). Statements marked **[H]** are research hypotheses, not
implemented semantics.

---

## 1. Purpose

EIR is the deterministic, inspectable data form of declared Elastic intent. It
decouples the typed Rust surface model from future consumers (validators,
planners, adapters, diagnostics) so those consumers can be built against a
stable, versioned shape instead of against live Rust API evolution.

```text
ResourceSpec (typed declaration, elastic-core)
        ↓ elastic_eir::lower / EirDocumentBuilder   — normalize + validate
EirDocument v1                                       — pure data
        ↓ (later missions: planning interfaces, resource adapters)
```

## 2. What it contains

Per resource node (`EirResource`):

- logical identity and semantic class;
- declared elastic dimensions (sorted);
- invariants with optional dimension scope (sorted);
- objectives with explicit priority ranks (`ObjectiveRank`, rank 0 = highest);
- admitted transitions enriched with the derived `capability_grounded` fact;
- required trusted capabilities (sorted);
- relevant observation signals (sorted);
- semantics-free diagnostic labels;
- a structural fingerprint of the normalized content.

Documents (`EirDocument`) carry an explicit [`SchemaVersion`] and store
resources sorted by logical identity, plus a document-level fingerprint over
version and node fingerprints.

## 3. Lowering and normalization

Lowering maps a validated surface declaration onto the IR and normalizes it:

1. unordered collections become sorted, deduplicated sequences;
2. objective priority becomes explicit numeric ranks rather than positional
   knowledge;
3. each admitted transition records whether a capability requirement grounds
   exactly that transition (same mechanism *and* dimension);
4. labels become a sorted key/value map.

Equivalent declarations lower to identical documents regardless of
construction order; equality, debug output, display, and fingerprints all
coincide. This is tested by construction-order permutation tests.

## 4. Determinism guarantees

- No hash-map iteration order influences content (`BTreeMap` only).
- No addresses, thread scheduling, randomness, or environment input.
- Fingerprints are **FNV-1a 64-bit structural fingerprints** absorbed in a
  fixed field order with unambiguous `[tag][length][payload]` framing, so
  distinct field sequences cannot collide by concatenation. They are suitable for equality checks,
  caching keys, and tests inside one trust domain. They are explicitly **not**
  cryptographic and must never authenticate anything across trust domains —
  the same disclaimer as the representation-layer evidence fingerprint.
- Fingerprint collisions are possible by design; they are an optimization,
  not a correctness mechanism. Correctness uses full structural comparison.

## 5. Validation

Every construction path validates:

- `lower(spec)` — lowers and re-validates (defense in depth; also enforces
  EIR-only rules on surface content);
- `EirDocumentBuilder::push/finish` — per-resource validation plus
  cross-resource uniqueness;
- `EirDocument::from_parts` — tooling path for non-surface sources; identical
  validation rules.

Validation is structural only: duplicates, empty elasticity, blank
identifiers/labels, vacuous invariant scopes, transitions/capabilities beyond
elastic dimensions, duplicate document identities, empty documents, and the
EIR v0.1 normative rule that **every required capability must be grounded in
at least one admitted transition** of the same resource. Invalid EIR cannot be
constructed through public API; there is no unchecked constructor.

Validation never solves planning problems and never authenticates
capabilities.

## 6. Neutrality and portability

- Backend-neutral, runtime-neutral, hardware-neutral: no CUDA/WGPU/NUMA/LLM
  concepts, no OS handles, no I/O, no clocks.
- Term vocabularies are reused from `elastic_core::resource` so semantic
  definitions exist exactly once. When serialization is introduced (future
  work), terms will gain canonical text encodings defined once, next to their
  definitions.
- No new external dependencies; `elastic-eir` depends only on `elastic-core`.
- `#![forbid(unsafe_code)]`; all types are plain immutable data, `Send +
  Sync` by construction.

## 7. Versioning

`SchemaVersion` is embedded in every document; this crate produces
`eir-v1` (`EIR_SCHEMA_VERSION = 1`).

What v1 commits to within this development epoch:

- presence and meaning of the listed fields;
- normalization and ordering rules;
- validation rules including capability grounding.

Not promised yet: wire/storage compatibility across schema versions, stable
fingerprint values across versions (fingerprints absorb the version), and any
serialization format. Version upgrades will document migration explicitly.

## 8. Planning interface

`elastic-eir` defines the *contract* a planner must satisfy — not any
algorithm. `TransitionPlanner::propose_transition(&EirResource)` returns one
of four honest outcomes: `Candidate`, `NoCandidate`, `InsufficientEvidence`,
or `Unsupported`. Candidates can only restate transitions the resource itself
admits and that are capability-grounded (`AdmittedTransition` has no external
constructor); custom planners should verify output with
`PlanOutcome::declares_valid_candidate`.

The bundled `FirstGroundedPlanner` is a deliberately trivial deterministic
selector (first grounded admission, canonical order) demonstrating the
contract; it weighs no objectives and performs no search.

## 9. Non-goals (v0.1)

No execution, no planning algorithms or optimizers, no observation sampling,
no live resource state, no distributed concerns, no solver integration.
**[H]** Whether EIR needs richer policy metadata (budgets, deadlines,
arbitration weights) before real planners land is an open question
deliberately deferred until a planner consumes it.
