# Elastic diagnostics and `cargo elastic check` v0.1

Status: ELANG6 developer-tooling foundation. This layer reports language errors;
it does not define resource, policy, planning, validation, or actuation semantics.

## Stable diagnostic codes

Elastic diagnostic codes are compatibility identifiers. Human-readable wording
may improve, but once shipped a code retains the same semantic category.

Current language codes:

| Code | Meaning |
| --- | --- |
| `ELX-LANG-0001` | unknown resource/target reference |
| `ELX-LANG-0002` | duplicate or colliding declaration |
| `ELX-LANG-0003` | undeclared predicate alias |
| `ELX-LANG-0004` | missing required declaration field |
| `ELX-LANG-0005` | malformed/unsupported declaration shape |

The public typed registry lives in `elastic-core` as `ElasticDiagnosticCode` with
schema `ELASTIC_DIAGNOSTIC_SCHEMA_V1 = 1`. To preserve the prepublication
package topology, `elastic-macros` remains a leaf proc-macro crate and mirrors
only the private code spellings it must emit during expansion; a workspace
contract test checks those spellings against the public core registry. This
avoids introducing a registry dependency from the proc-macro to core before the
core package exists on crates.io.

Proc-macro diagnostics prefix their ordinary rustc message with `[ELX-...]`;
rustc remains authoritative for source spans and rendering.

## Cargo frontend

The workspace ships the `cargo-elastic` executable, so Cargo discovers it as a
custom command:

```bash
cargo elastic check
cargo elastic check --manifest-path path/to/Cargo.toml
cargo elastic check -p my-package --all-targets
```

`cargo elastic check` delegates compilation to:

```text
cargo check --message-format=json
```

It does not parse Elastic business semantics itself. It filters rustc
`compiler-message` records whose message starts with a stable `[ELX-...]` code,
preserves the primary rustc span, and emits one machine-readable JSON report.

Schema v0.1:

```json
{
  "schema": "elastic-diagnostics/v1",
  "command": "check",
  "cargo_success": false,
  "elastic_diagnostic_count": 1,
  "diagnostics": [
    {
      "code": "ELX-LANG-0001",
      "level": "error",
      "message": "policy `bad` targets unknown document resource `missing`",
      "primary_span": {
        "file": "src/main.rs",
        "line_start": 12,
        "column_start": 20,
        "line_end": 12,
        "column_end": 27
      }
    }
  ]
}
```

A non-Elastic Rust error still makes Cargo fail, but `cargo-elastic` never
relabels it with an Elastic diagnostic code. This keeps ownership boundaries
clear between rustc/Cargo and the Elastic language.

## Exit status

- `0`: Cargo check succeeded;
- `1`: Cargo check failed (with or without Elastic diagnostics);
- `2`: `cargo-elastic` itself could not execute the check or process its output.

## Non-goals

This slice does not implement macro expansion output, policy analysis, graph
rendering, dynamic configuration, registry/network operations, or any runtime
mutation. Later `cargo elastic analyze/graph/...` commands must call existing
core/EIR authorities rather than recreate their semantics in the CLI.
