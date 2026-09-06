# ElasticXxx packageability gate

Status: pre-release / no publication authorized

This document defines reversible packageability checks for the first ElasticXxx crate release. It does not authorize `cargo publish`, select a legal license, reserve crate names, or tag a release.

## Public dependency chain

The user-facing `elastic` facade currently depends on the following internal crates:

1. `elastic-core`;
2. `elastic-macros`;
3. `elastic-eir` (`elastic-core`);
4. `elastic-adapters` (`elastic-core`, `elastic-eir`);
5. `elastic-runtime` (`elastic-core`, `elastic-eir`, `elastic-adapters`);
6. `elastic` (`elastic-core`, `elastic-eir`, `elastic-adapters`, `elastic-runtime`, `elastic-macros`).

All internal publish-path dependencies must carry both a compatible version and a local `path`. The path is used inside the workspace; Cargo removes it from a packaged manifest and retains the version for registry resolution.

The first real registry publication, if separately authorized, must therefore occur in dependency order. A facade-only first publication is not valid because Cargo resolves packaged path dependencies through the registry.

## What CI can prove before the first publication

Cargo cannot fully prepare a registry upload for a crate whose internal registry dependencies have never been published. In particular, `cargo package` for `elastic-eir` legitimately fails before `elastic-core` exists in the selected registry, even when the workspace path dependency has a valid version.

The pre-release packageability workflow therefore proves only what can be proved without fabricating a registry:

- every publish-path workspace dependency has both the expected local `path` and version `0.1.0`;
- `cargo package --list` succeeds for the complete facade dependency chain, so Cargo can determine each package file set;
- real `cargo package --no-verify` archives are built for the first-publish leaf crates `elastic-core` and `elastic-macros`;
- normal workspace CI remains authoritative for compile, tests, Clippy, rustdoc and runtime semantics.

This is intentionally weaker than claiming the full chain has already been upload-prepared. After a real registry contains the leaf crates, the same gate can advance one level at a time (`elastic-eir`, then `elastic-adapters`, then `elastic-runtime`, then `elastic`).

No fake local registry or committed `[patch.crates-io]` is used to turn an unavailable dependency into a false positive.

## Explicit blockers before real publication

Real publication remains blocked until all of the following are resolved deliberately:

- a repository license is selected and represented consistently by a license file and Cargo package metadata;
- ownership/availability of every intended crates.io package name is verified at release time;
- the intended public/private crate topology is explicitly approved;
- package archives are inspected for unintended files or missing documentation;
- the exact release commit passes the normal required CI and the packageability workflow;
- versions/changelog/release notes are frozen for that release;
- a clean downstream sample can consume the published facade without workspace paths.

No CI job may infer that a missing license or unavailable crate name is acceptable. Those conditions remain unresolved release blockers until explicitly resolved.

## Non-goals

This gate does not:

- publish any crate;
- reserve a registry name;
- choose MIT, Apache-2.0 or any other license;
- promise semver stability beyond the declared pre-1.0 policy;
- alter runtime, model-execution, representation or adapter semantics.
