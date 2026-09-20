# ElasticXxx compatibility policy

Status: pre-release policy for the `0.1.x` line; no registry publication is authorized by this document.

This policy defines the compatibility promises used by CI and release review while ElasticXxx remains below 1.0. It does not convert research maturity into production readiness and does not authorize `cargo publish`.

## Supported public boundary

The supported downstream Rust boundary is the `elastic` facade. Ordinary consumers should not need direct imports from implementation crates to execute the documented resource declaration, planning, runtime, evidence, and configuration flows.

Internal crates may be packaged because the facade depends on them, but their implementation details are not independently promoted as a stable user-facing API unless they are documented and re-exported through `elastic`.

Versioned wire contracts are separate compatibility boundaries. A v1 configuration, evidence, or decision-trace schema keeps its declared semantics; incompatible shape or semantic changes require a new schema version rather than silent reinterpretation.

## Pre-1.0 semantic versioning

The current release line is `0.1.x`.

- Patch releases (`0.1.z`) must not intentionally break documented public `elastic` APIs or existing v1 wire contracts. Correctness, safety, or soundness defects may require a narrowly scoped behavior change; that change must be called out in release notes.
- A change that intentionally removes, renames, or incompatibly changes a documented public API requires the next pre-1.0 minor line (`0.2.0` or later) with migration notes.
- New additive APIs may land in a patch release only when they preserve existing behavior and do not weaken validation, invariants, fail-closed gates, or wire compatibility.
- Undocumented implementation details are not compatibility promises.

This is deliberately stricter than treating all `0.y.z` releases as freely breaking.

## MSRV

The declared minimum supported Rust version is `1.89`. CI executes the principal Rust checks with Rust `1.89.0`, and the packageability gate verifies that every workspace package resolves the same `rust-version`.

Before 1.0, raising the MSRV is not permitted in a patch release. An MSRV increase requires a pre-1.0 minor release, an explicit changelog entry, and exact-head CI on the new minimum toolchain.

Using a newer compiler locally does not establish a new MSRV.

## Deprecation and removals

Where practical, a documented public Rust API should be deprecated for at least one pre-1.0 minor line before removal. Immediate removal is reserved for cases where retaining the API would preserve an unsound, unsafe, security-sensitive, or materially incorrect contract; such exceptions require explicit release documentation.

A deprecated convenience surface must continue to lower to the same typed semantic core until it is removed.

## Configuration and evidence compatibility

Versioned persisted contracts are fail-closed boundaries.

- Unknown schema versions are rejected.
- Existing schema identities are not reused for incompatible semantics.
- Strict readers are allowed to reject unknown fields; therefore adding a field to a strict v1 object is itself a compatibility decision and normally requires a new schema version.
- Historical evidence remains explanatory unless a separate contract explicitly authorizes physical replay after current identity and capability revalidation.

## Pre-1.0 parser and memory-safety hardening

Persisted/untrusted decoding boundaries are part of the compatibility surface and
must fail closed under malformed input. The continuous-hardening program therefore
covers:

- representation/issuer identifiers and transition-pipeline structure;
- bounded `DecisionTrace` JSON decoding;
- bounded `OperatorConfig` JSON decoding and semantic validation;
- bounded `GuardConfigV1` JSON decoding, stable-key validation and deterministic lowering;
- bounded `ModelExecutionControllerContractsV1` JSON decoding and full provider/model/fingerprint revalidation;
- bounded runtime `EvidenceEnvelope` JSON decoding and semantic validation.

Pull requests compile every cargo-fuzz target on nightly. Scheduled and manually
dispatched hardening runs execute bounded fuzz campaigns; a green PR fuzz-build is
therefore **not** reported as executed fuzz-time evidence.

Miri remains a scheduled/manual gate. It interprets `elastic-core` and `elastic-kv`
tests and additionally exercises the bounded operator-config, guard-config,
model-execution-contract and runtime-evidence decoder tests. This broadens interpreter coverage without implying that Miri proves
the absence of all unsafe behavior in external dependencies or hardware backends.

## Release gate

A release candidate must, on its exact commit:

1. pass the repository CI on the declared MSRV;
2. pass packageability checks for the public facade dependency chain;
3. preserve explicit versions on all non-dev internal registry dependencies;
4. document intentional public API, schema, MSRV, and migration changes;
5. satisfy the separate legal, registry-name, topology, archive, and downstream-install blockers in [PACKAGEABILITY.md](PACKAGEABILITY.md).

Until those blockers are deliberately cleared, `publish = false` remains the repository state and no publication readiness claim is implied.
