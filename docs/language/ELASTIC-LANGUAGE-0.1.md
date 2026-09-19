# Elastic embedded language v0.1

Status: first embedded Rust language slice. This syntax is a convenience surface
over the existing typed ElasticXxx semantic core; it is **not** a second runtime
or a second resource model.

## Goal

The first language slice makes an Elastic resource declaration look like a
resource declaration rather than an implementation detail of a Rust struct:

```rust
use elastic::prelude::*;

elastic! {
    pub resource session_kv {
        class(representational);
        id("session-kv");
        allow(representation, residency);
        preserve(contents);
        optimize(latency, memory_footprint);
        admit(reencode @ representation);
        capability(reencode @ representation);
        observe(free_capacity, queue_depth);
    }
}

let spec = session_kv::resource_spec()?;
```

`resource_spec()` returns the same validated `ResourceSpec` that the manual
builder and `#[derive(ElasticResource)]` produce.

## Grammar v0.1

One invocation declares exactly one resource:

```text
elastic-invocation := visibility? "resource" IDENT "{" declaration* "}"
declaration       := derive-fragment ";"
```

The optional visibility is ordinary Rust visibility such as `pub` or
`pub(crate)`. Version 0.1 accepts the same fragments already supported by the
derive surface:

```text
class(...)
id("...")
allow(...)
preserve(...)
optimize(...)
admit(<mechanism> @ <dimension>)
capability(<mechanism> @ <dimension>)
observe(...)
label("key", "value")
```

Top-level declarations use semicolons. Commas remain valid inside fragment
payloads, for example `allow(representation, residency);`.

When `id("...")` is omitted, the resource identifier defaults to the Rust
resource module name. This differs only in spelling from the derive surface,
whose default is the struct name; an explicit `id` makes all three surfaces
identical.

## Single semantic implementation

The proc macro does not independently translate resource semantics. It:

1. parses only the outer `visibility? resource NAME { ... }` language shape;
2. normalizes semicolon-delimited body fragments;
3. validates those fragments with the existing derive parser;
4. emits an internal declaration using `#[derive(ElasticResource)]`;
5. exposes the resulting validated spec through `NAME::resource_spec()`.

Therefore:

```text
manual ResourceSpec builder
          ==
#[derive(ElasticResource)]
          ==
elastic! { resource ... }
          ↓
      ResourceSpec
          ↓
          EIR
```

Workspace tests require equality of the `ResourceSpec`, lowered EIR, and EIR
fingerprint for equivalent declarations.

## Explicit non-goals for v0.1

This slice does not yet add:

- multiple resources inside one invocation;
- resource groups;
- cross-resource invariants or shared budgets;
- policy blocks or planner hints;
- implicit runtime startup;
- implicit observation, validation, actuation, or rollback;
- new transition semantics;
- compiler or `rustc` modifications.

Those features must first obtain typed core/EIR semantics and then extend this
same lowering path. In particular, the next multi-resource language slice must
not create a parallel orchestration model inside the macro.

## Diagnostics and bounds

Malformed resource fragments use the existing derive diagnostics. The outer DSL
also rejects:

- more than one resource per v0.1 invocation;
- an empty resource body;
- top-level comma separators (use `;`);
- empty declarations caused by duplicate semicolons.

All typed resource validation remains authoritative in `ResourceSpecBuilder`.
