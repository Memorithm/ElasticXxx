# Public package naming and topology decision

Status: explicit pre-release decision; publication remains unauthorized.

ElasticXxx keeps `elastic` as the supported Rust facade crate name in source code, but the existing crates.io package names `elastic` and `elastic_macros` are owned by an unrelated project. Memorithm will therefore use an organization-prefixed registry package namespace for the first public release instead of attempting to publish under those occupied names.

## Selected registry package names

| Workspace package | Intended crates.io package | Registry role | Supported user boundary |
| --- | --- | --- | --- |
| `elastic-core` | `memorithm-elastic-core` | public implementation dependency | no |
| `elastic-macros` | `memorithm-elastic-macros` | public implementation dependency | no |
| `elastic-eir` | `memorithm-elastic-eir` | public implementation dependency | no |
| `elastic-adapters` | `memorithm-elastic-adapters` | public implementation dependency | no |
| `elastic-runtime` | `memorithm-elastic-runtime` | public implementation dependency | no |
| `elastic-kv` | `memorithm-elastic-kv` | public implementation dependency | no |
| `elastic` | `memorithm-elastic` | public facade package | yes |

All seven packages must be registry-visible because Cargo resolves packaged path dependencies through the registry. This does not make the six implementation packages stable user APIs. The supported public Rust boundary remains the `elastic` facade; downstream source code may continue to import it as `elastic` after Cargo package aliases are introduced.

No private-registry split is selected for the 0.1.x public release line. A mixed public/private topology would make the public facade non-installable from crates.io alone unless the private dependencies were replaced or vendored, which is not the current architecture. The dependency-order publication model remains: leaves first, then each non-leaf only after its registry dependencies exist and its archive can be built and inspected.

## Point-in-time availability observation

At `2026-09-19T05:58:13Z`, read-only GET requests to the crates.io API returned `404 Not Found` for all seven selected names: `memorithm-elastic-core`, `memorithm-elastic-macros`, `memorithm-elastic-eir`, `memorithm-elastic-adapters`, `memorithm-elastic-runtime`, `memorithm-elastic-kv`, and `memorithm-elastic`.

The requests used the identifying user agent `Memorithm-release-audit/1.0 contact@checkupauto.fr`. These observations are not reservations or ownership proof. Every selected name must be rechecked immediately before any separately authorized publish operation. Registry mutation remains forbidden by the current productization contract.

## Applied Cargo mapping

The selected registry names are now the `[package].name` values of the seven registry-visible crates. Source-level crate names remain stable through explicit `[lib].name` declarations (`elastic`, `elastic_core`, `elastic_eir`, `elastic_adapters`, `elastic_runtime`, `elastic_kv`, `elastic_macros`) and Cargo dependency aliases. Workspace path dependencies retain both `path` and `version = "0.1.0"` while declaring the corresponding `package = "memorithm-elastic-*"` identity. Non-published workspace consumers use the same explicit aliases.

This mapping is a packaging change, not a registry publication. Every selected crates.io name must still be rechecked immediately before any separately authorized publish operation, `publish = false` remains in force, and the dependency-order publication, non-leaf archive inspection, clean downstream-install, and exact-release-commit CI/packageability gates remain unresolved. The 0.1.0 release-metadata freeze itself is complete.

The prior audit in `REGISTRY-NAME-AUDIT.md` remains provenance for why the unprefixed names are not used. This document records the explicit replacement naming and topology decision; it does not rewrite or erase that negative collision evidence.
