# ElasticXxx packageability gate

Status: pre-release / no publication authorized

This document defines the reversible packageability checks for the first ElasticXxx crate release. It does not authorize `cargo publish`, select a legal license, reserve crate names, or tag a release.

## Public dependency chain

The user-facing `elastic` facade currently depends on the following internal crates:

1. `elastic-core`;
2. `elastic-macros`;
3. `elastic-eir` (`elastic-core`);
4. `elastic-adapters` (`elastic-core`, `elastic-eir`);
5. `elastic-runtime` (`elastic-core`, `elastic-eir`, `elastic-adapters`);
6. `elastic` (`elastic-core`, `elastic-eir`, `elastic-adapters`, `elastic-runtime`, `elastic-macros`).

All internal publish-path dependencies must carry both an exact compatible version and a local `path`. The path is used inside the workspace; Cargo removes it from a packaged manifest and retains the version for registry resolution.

The first real registry publication, if separately authorized, must therefore occur in dependency order. A facade-only first publication is not valid because Cargo resolves packaged path dependencies through the registry.

## CI packageability simulation

Before a release is authorized, CI may simulate the registry having the same internal `0.1.0` crates by injecting temporary `[patch.crates-io]` entries in the CI checkout. Those entries are test scaffolding only and must not be committed to the product manifest.

The simulation must run `cargo package --no-verify` for every crate in dependency order. This proves that Cargo can construct upload archives and rewrite each local versioned dependency into a registry dependency without publishing anything.

Normal workspace CI remains authoritative for build, test, lint and documentation correctness. Package simulation is an additional release gate, not a replacement for exact-head CI.

## Explicit blockers before real publication

Real publication remains blocked until all of the following are resolved deliberately:

- a repository license is selected and represented consistently by a license file and Cargo package metadata;
- ownership/availability of every intended crates.io package name is verified at release time;
- the intended public/private crate topology is explicitly approved;
- package archives are inspected for unintended files or missing documentation;
- the exact release commit passes the normal required CI and the packageability workflow;
- versions/changelog/release notes are frozen for that release;
- a clean downstream sample can consume the published facade without workspace paths.

No CI job may infer that a missing license or unavailable crate name is acceptable. It may only report those conditions as unresolved release blockers.

## Non-goals

This gate does not:

- publish any crate;
- reserve a registry name;
- choose MIT, Apache-2.0 or any other license;
- promise semver stability beyond the declared pre-1.0 policy;
- alter runtime, model-execution, representation or adapter semantics.
