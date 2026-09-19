# ElasticXxx packageability gate

Status: pre-release / no publication authorized

This document defines reversible packageability checks for the first ElasticXxx crate release. It does not authorize `cargo publish`, select a legal license, reserve crate names, or tag a release.

## Public dependency chain

The user-facing source crate `elastic` is packaged as `memorithm-elastic`. Its registry-visible dependency chain is:

1. `memorithm-elastic-core` (library crate `elastic_core`);
2. `memorithm-elastic-macros` (library crate `elastic_macros`);
3. `memorithm-elastic-eir` (`memorithm-elastic-core`);
4. `memorithm-elastic-adapters` (`memorithm-elastic-core`, `memorithm-elastic-eir`);
5. `memorithm-elastic-runtime` (`memorithm-elastic-core`, `memorithm-elastic-eir`, `memorithm-elastic-adapters`);
6. `memorithm-elastic-kv` (`memorithm-elastic-core`, `memorithm-elastic-eir`, `memorithm-elastic-runtime`);
7. `memorithm-elastic` (the six implementation packages above).

All internal publish-path dependencies carry a compatible version, a local `path`, and an explicit Cargo package alias preserving the existing source-level crate names. The path is used inside the workspace; Cargo removes it from a packaged manifest and retains the version/package identity for registry resolution.

The first real registry publication, if separately authorized, must therefore occur in dependency order. A facade-only first publication is not valid because Cargo resolves packaged path dependencies through the registry.

## What CI can prove before the first publication

Cargo cannot fully prepare a registry upload for a crate whose internal registry dependencies have never been published. In particular, `cargo package` for `memorithm-elastic-eir` legitimately fails before `memorithm-elastic-core` exists in the selected registry, even when the workspace path dependency has a valid version.

The pre-release packageability workflow therefore proves only what can be proved without fabricating a registry:

- every publish-path workspace dependency has both the expected local `path` and version `0.1.0`;
- Cargo metadata resolves every non-dev internal dependency in the public facade chain to an explicit `^0.1.0` registry requirement rather than a path-only wildcard;
- every workspace crate resolves the declared MSRV `rust-version = "1.89"`;
- `cargo package --list` succeeds for the complete facade dependency chain, so Cargo can determine each package file set;
- real `cargo package --no-verify` archives are built for the first-publish leaf packages `memorithm-elastic-core` and `memorithm-elastic-macros`;
- those two leaf archives are inspected fail-closed against Cargo's exact `--list` file set, bounded archive/member sizes, safe package-relative paths, regular-file-only entries, exact packaged release metadata, repository `LICENSE.md`, crate-level rustdoc and exact Git-head VCS provenance;
- normal workspace CI remains authoritative for compile, tests, Clippy, rustdoc and runtime semantics.

This is intentionally weaker than claiming the full chain has already been upload-prepared. After a real registry contains the leaf packages, the same gate can advance one level at a time (`memorithm-elastic-eir`, then `memorithm-elastic-adapters`, then `memorithm-elastic-runtime`, then `memorithm-elastic-kv`, then `memorithm-elastic`).

No fake local registry or committed `[patch.crates-io]` is used to turn an unavailable dependency into a false positive.

## Licensing prerequisite

The repository license is selected: `LICENSE.md` contains PolyForm Noncommercial License 1.0.0 with the required notice, `LICENSING.md` documents the separate commercial-licensing path, and workspace package metadata resolves `license-file = "LICENSE.md"`. Packageability CI checks that the public facade dependency chain retains that license-file metadata.

This records repository consistency only. It is not legal advice and does not assert that any registry, distributor, or downstream use has been approved.

## Explicit blockers before real publication

Real publication remains blocked until all of the following are resolved deliberately:

- every intended organization-prefixed package name is rechecked at release time;
- the remaining non-leaf package archives are built and inspected after dependency-order publication makes those registry-dependent archives constructible;
- the exact release commit passes the normal required CI and the packageability workflow;
- versions/changelog/release notes are frozen for that release;
- a clean downstream sample can consume the published facade without workspace paths.

The leaf-archive inspection closes only the archive-content/documentation check for `elastic-core` and `elastic-macros`; it does not make the registry-dependent non-leaf archives inspectable before their dependencies exist in the registry. The selected public topology keeps all facade dependencies registry-visible while designating only the facade as the supported user boundary. The Cargo package mapping is now applied, but it does not authorize publication. No CI job may infer that a colliding or unverified crate name, incomplete release evidence, or failed remaining archive/downstream inspection is acceptable. Those conditions remain unresolved release blockers until explicitly resolved.

The compatibility and MSRV rules used by this gate are defined in [COMPATIBILITY.md](COMPATIBILITY.md). The machine-readable BE15f pre-release state, canonical SciRust license-source fingerprint, exact consumer/source pins and unresolved publication blockers are recorded in [PRODUCTIZATION-V1.json](PRODUCTIZATION-V1.json); CI validates that record with `scripts/check_release_productization.py`. The point-in-time registry lookup and its non-reservation limits are recorded in [REGISTRY-NAME-AUDIT.md](REGISTRY-NAME-AUDIT.md). The explicit replacement package names and public dependency topology are recorded in [PACKAGE-NAMING-V1.md](PACKAGE-NAMING-V1.md). Migration and cross-repository scope are documented in [MIGRATION-0.1.md](MIGRATION-0.1.md) and [CROSS_REPO_COMPATIBILITY.md](CROSS_REPO_COMPATIBILITY.md).

## Non-goals

This gate does not:

- publish any crate;
- reserve a registry name;
- choose MIT, Apache-2.0 or any other license;
- promise semver stability beyond the declared pre-1.0 policy;
- alter runtime, model-execution, representation or adapter semantics.
