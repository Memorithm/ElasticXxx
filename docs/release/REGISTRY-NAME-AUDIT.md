# crates.io package-name audit

Status: observed blocker evidence; publication remains unauthorized.

This record captures a point-in-time read-only crates.io lookup for the public ElasticXxx dependency chain. It is not a reservation, ownership proof, publication authorization, or substitute for a release-time recheck.

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

The five `404 Not Found` responses are point-in-time absence observations only. They do not reserve names. All intended package names must be rechecked at the exact release decision point.

The lookup used only read-only HTTP GET requests to the endpoints above with the identifying user agent `Memorithm-release-audit/1.0 contact@checkupauto.fr`. No registry credentials or mutation were used.

Publication remains fail-closed. Choosing alternate public names or establishing a legitimate ownership/transfer path is a separate explicit release decision. No crate is renamed by this audit and no publish operation is authorized.

References: Cargo registry-index name restrictions (`https://doc.rust-lang.org/cargo/reference/registry-index.html#name-restrictions`) and Cargo publishing guidance (`https://doc.rust-lang.org/cargo/reference/publishing.html`).
