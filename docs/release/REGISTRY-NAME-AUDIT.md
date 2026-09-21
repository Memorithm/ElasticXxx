# crates.io package-name audit

Status: observed blocker evidence; publication remains unauthorized.

This record captures point-in-time read-only crates.io lookups for ElasticXxx
registry naming. It is not a reservation, ownership proof, publication
authorization, or substitute for a release-time recheck.

## Historical collision evidence (unprefixed names)

Audit time: `2026-09-19T03:33:46Z`.

| Intended package | API observation |
| --- | --- |
| `elastic-core` | `https://crates.io/api/v1/crates/elastic-core` -> `404 Not Found` |
| `elastic-macros` | `https://crates.io/api/v1/crates/elastic-macros` -> existing `elastic_macros 0.0.0`, repository `https://github.com/elastic-rs/elastic` |
| `elastic-eir` | `https://crates.io/api/v1/crates/elastic-eir` -> `404 Not Found` |
| `elastic-adapters` | `https://crates.io/api/v1/crates/elastic-adapters` -> `404 Not Found` |
| `elastic-runtime` | `https://crates.io/api/v1/crates/elastic-runtime` -> `404 Not Found` |
| `elastic-kv` | `https://crates.io/api/v1/crates/elastic-kv` -> `404 Not Found` |
| `elastic` | `https://crates.io/api/v1/crates/elastic` -> existing `elastic 0.21.0-pre.5`, repository `https://github.com/elastic-rs/elastic` |

Cargo documents that crates.io performs case-insensitive collision detection and that crates.io prevents differences of `-` vs `_`. Therefore the observed `elastic_macros` package conflicts with the intended `elastic-macros` package name, and `elastic` is already occupied. No Memorithm ownership of either existing registry package is established by this audit.

The five `404 Not Found` responses are point-in-time absence observations only. They do not reserve names. Those collisions motivated the organization-prefixed `memorithm-elastic*` replacement names recorded in [PACKAGE-NAMING-V1.md](PACKAGE-NAMING-V1.md).

## Selected `memorithm-elastic*` recheck (all eight topology names)

Recheck time: `2026-09-21T13:34:02Z`.

Read-only GET requests with User-Agent `Memorithm-release-audit/1.0 contact@checkupauto.fr`:

| Selected registry package | API observation |
| --- | --- |
| `memorithm-elastic-core` | `404 Not Found` |
| `memorithm-elastic-language-syntax` | `404 Not Found` |
| `memorithm-elastic-macros` | `404 Not Found` |
| `memorithm-elastic-eir` | `404 Not Found` |
| `memorithm-elastic-adapters` | `404 Not Found` |
| `memorithm-elastic-runtime` | `404 Not Found` |
| `memorithm-elastic-kv` | `404 Not Found` |
| `memorithm-elastic` | `404 Not Found` |

Unrelated occupied names reconfirmed at the same instant: `elastic` -> `200`, `elastic_macros` -> `200`.

Every `404` above is a non-reserving absence observation. All eight selected names must be rechecked again immediately before any separately authorized publish operation. Registry mutation remains forbidden by the current productization contract.

The lookup used only read-only HTTP GET requests. No registry credentials or mutation were used.

Publication remains fail-closed. Choosing alternate public names or establishing a legitimate ownership/transfer path is a separate explicit release decision. No crate is renamed by this audit and no publish operation is authorized.

References: Cargo registry-index name restrictions (`https://doc.rust-lang.org/cargo/reference/registry-index.html#name-restrictions`) and Cargo publishing guidance (`https://doc.rust-lang.org/cargo/reference/publishing.html`).
