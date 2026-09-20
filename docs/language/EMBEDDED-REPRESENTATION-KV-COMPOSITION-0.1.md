# Representation/precision and KV composition v0.1

Status: ELANG7d structural composition contract.

`RepresentationPrecisionKvBindingV1` proves that two existing planning artifacts
refer to the **same** representation transition:

1. a candidate actually selected by the BE14e fixed-width
   representation/precision preplanner; and
2. an existing `KvTransitionPlan` produced by the KV contract.

It does not select a representation, run KV capacity admission, validate a
trusted capability snapshot at the actuation boundary, or mutate KV storage.

## Required coherence

Construction fails closed unless all of the following agree:

- BE14e report schema and precision source metadata;
- selected candidate id and preference rank;
- current representation id, schema version and materialization epoch;
- candidate target representation and schema version;
- transition mechanism;
- declared precision bits;
- canonical precision predicate/signal/unit;
- structural and capability eligibility recorded as true;
- exactly one strict `DecisionTrace/v1` for the selected candidate;
- trace-selected dimension is `representation` and mechanism matches;
- target representation state derived by `RepresentationState::derive_target`,
  including exact epoch semantics, equals the KV plan target.

The binding therefore rejects a KV plan whose target contract looks similar but
whose epoch, mechanism, source state or candidate evidence differs.

## Why the decision trace is part of the binding

A planning report without its selected-candidate trace is insufficient for this
composition. The trace is parsed through the existing bounded strict
`DecisionTrace::from_bounded_json` decoder. The binding records the exact trace
bytes in its structural fingerprint.

The fingerprint is deterministic local structural identity, not authentication.

## Representation precision boundary

The BE14e precision value remains **declared fixed-width bits per scalar**. It is
not a model-quality, reconstruction-error, entropy, or scientific-fidelity
claim. Codebook/variable-width representations require a separately qualified
contract instead of manufacturing a fixed-width number.

## KV ownership boundary

KV-specific semantics remain owned by `elastic-kv`, including:

- key transform scope;
- transform/codec ordering;
- recovery source;
- reusable-cache compatibility;
- capacity preflight;
- physical backend transaction and rollback.

A successful representation/KV binding never authorizes physical movement or
re-encoding. Trusted capability/attestation validation, source-state comparison,
capacity evidence, verification and rollback remain mandatory immediately around
physical actuation.
