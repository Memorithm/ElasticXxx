# Stable Elastic language expansion v0.1

Status: ELANG6 developer-tooling surface.

`cargo elastic expand` expands only `elastic!` embedded-language invocations. It
is **not** a generic Rust macro expander and does not attempt to replace rustc.

```bash
cargo elastic expand --source src/lib.rs
cargo elastic expand --source src/lib.rs --format json
```

## Single expansion authority

The parser and Rust-token generator previously resident only in the proc-macro
crate are factored into the internal `memorithm-elastic-language-syntax` crate.
Both:

- `elastic-macros` during normal compilation; and
- `cargo elastic expand`

call the same `expand_elastic_tokens()` implementation. The CLI therefore does
not maintain a second language grammar or lowering implementation.

This design intentionally avoids:

- nightly rustc;
- `-Zunpretty`;
- `RUSTC_BOOTSTRAP`;
- an undeclared dependency on `cargo-expand`;
- filesystem side effects from procedural macros.

## Source discovery and output

Version 0.1 requires one explicit Rust source file. It parses ordinary Rust with
`syn`, visits function-like macros whose last path segment is `elastic`, and
expands each invocation independently through the shared expander.

The default output is stable formatted Rust. `--format json` emits schema
`elastic-expansion/v1` with one formatted Rust expansion per invocation.

The expansion command is read-only. Generated Rust still uses the ordinary
public Elastic builders/EIR contracts; displaying it grants no runtime or
actuation authority.

## Non-goals

This command does not expand arbitrary third-party macros, `macro_rules!`, or
`#[derive(...)]` implementations. It does not compile, execute, validate, or
actuate user code. Broader compiler expansion remains rustc tooling territory.
