# Boolean decision trace wire contract v1

Status: implemented by `elastic-runtime` as `elastic-boolean-decision-trace-v1`.

A `DecisionTrace` is durable **decision evidence**, not an authority token. Importing or decoding a trace never observes hardware, invokes a planner, validates a transition, calls an adapter, or actuates a resource. Any reuse of a historical decision identity must independently pass the current freshness/resource/policy checks and the normal trusted validation immediately before actuation.

## Canonical producer and consumer

- Producer: `DecisionTrace::to_bounded_json`.
- Consumer: `DecisionTrace::from_bounded_json`.
- Replay identity check: `DecisionTrace::validate_replay_identity`.

The encoder and decoder share the same typed runtime contract. There is no CLI-only wire semantics.

## Required top-level fields

The v1 JSON object contains all of the following fields, including nullable `selected` and `stop_reason` fields:

- `schema` — exactly `elastic-boolean-decision-trace-v1`;
- `resource_id`;
- `guarded_resource_fingerprint`;
- `fact_snapshot_fingerprint`;
- `fact_source`;
- `observation_epoch`;
- `resource_generation`;
- `predicates`;
- `eligible`;
- `rejected`;
- `unknown`;
- `selected`;
- `stop_reason`.

Unknown fields, duplicate fields, missing required fields, invalid enum values, and future schema identifiers are rejected.

## Bounds before JSON materialization

The decoder performs a non-allocating lexical preflight over the input before Serde materializes the document. It enforces the shared runtime-evidence limits for total bytes, nesting depth, nodes, collection items, and string token size. Numeric tokens are additionally bounded to the decimal width required by the v1 unsigned 64-bit counters.

This preflight is not a second JSON parser: malformed syntax is still rejected by `serde_json`; its purpose is to prevent an untrusted document from requesting unbounded materialization.

## Semantic validation after decoding

The typed decoder also verifies that:

- predicate keys are valid, strictly ordered, and unique;
- a predicate absent from the materialized fact snapshot is `Unknown`;
- candidate classifications are disjoint;
- eligible/selected candidates are capability-grounded;
- a selected candidate belongs to the eligible transition set;
- `selected` and `stop_reason` are mutually consistent;
- the stop reason matches the eligible/rejected/unknown classification;
- the persisted fact fingerprint exactly matches the decoded materialized facts, source, epoch, resource id, and generation.

The guarded-resource fingerprint is reconstructed only as a non-cryptographic structural identity. It is not authentication.

## Round-trip invariant

Every trace successfully emitted by the bounded v1 encoder is intended to decode back to the same typed `DecisionTrace`. Tests cover built-in dimensions as well as custom dimension text containing the separators used by the human-readable guard-scope encoding.

## Trust boundary

A decoded trace can explain or compare a historical decision. It cannot make an undeclared transition legal, cannot turn stale evidence into current evidence, and cannot bypass adapter validation, verification, commit/rollback, or fail-closed behavior.
