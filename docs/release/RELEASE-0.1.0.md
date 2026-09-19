# ElasticXxx 0.1.0 release-candidate notes

Status: **metadata frozen; publication not authorized**.

The 0.1.0 line packages the existing ElasticXxx adaptive-resource runtime behind its supported Rust facade. The public source-level crate remains `elastic`; the selected registry package is `memorithm-elastic`. The registry-visible implementation dependency packages are `memorithm-elastic-core`, `memorithm-elastic-macros`, `memorithm-elastic-eir`, `memorithm-elastic-adapters`, `memorithm-elastic-runtime`, and `memorithm-elastic-kv`. Direct use of those implementation packages is not the supported public API boundary.

The release line declares Rust 1.89 as its MSRV and retains the repository PolyForm Noncommercial 1.0.0 license file. Commercial licensing is a separate written agreement as described in `LICENSING.md`.

## What this freeze establishes

- version target: `0.1.0` for every registry-visible package in the facade chain;
- release line: `0.1.x` under the pre-1.0 compatibility policy;
- required changelog and release-note documents are present and digest-pinned by the productization checker;
- package publication remains disabled in Cargo manifests.

## Gates that remain open

The candidate is not publishable solely because these notes exist. The following remain unresolved and fail closed in the productization contract:

1. recheck all selected crates.io names at release time;
2. perform any dependency-order registry publication only after separate release approval;
3. inspect non-leaf package archives once their registry dependencies make those archives constructible;
4. verify a clean downstream project can consume the published facade without workspace paths.

## Recurring exact-commit qualification

The exact-commit gate is implemented and is no longer an unresolved publication blocker. Main commit `01058ad907139e4bc21f181b63024b689a24f076` passed `ci` (`35431038327`), `packageability` (`35431038403`), and `release-candidate-prepublication` (`35431038316`) on that exact SHA. Any later release-candidate payload must independently pass the same three checks on its own exact commit; this evidence is not transferable across payload changes.

No performance, FPS, latency, memory-saving, energy, hardware, scientific novelty, registry-ownership, or compatibility-with-unpublished-artifacts claim is made by this document.
