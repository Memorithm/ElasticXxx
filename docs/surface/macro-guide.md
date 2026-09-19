# Elastic Macro Guide v0.3

**Status:** normative for `#[derive(ElasticResource)]` and the `elastic! { ... }`
embedded language surface in `crates/elastic-macros`, including standalone
resources, multi-resource documents and ELANG3 groups. All are re-exported by
the `elastic` facade.

---

## 1. One semantic implementation

The derive macro is pure syntax sugar over the ordinary Rust API. It expands to
a single associated function that calls exactly the builder a programmer would
have written:

```rust
#[derive(ElasticResource)]
#[elastic(
    class(representational),
    id("session-kv"),
    allow(representation, residency),
    preserve(contents),
    preserve(contract("kv.reuse-contract") along representation),
    optimize(latency, memory_footprint),
    admit(reencode @ representation),
    capability(reencode @ representation),
    observe(free_capacity),
    label("workload", "slha-v2"),
)]
struct SessionKv;
```

expands to (conceptually):

```rust
impl SessionKv {
    pub fn resource_spec()
        -> Result<elastic::resource::ResourceSpec,
                  elastic::resource::ResourceSpecError>
    {
        let builder = ::elastic::resource::ResourceSpec::builder(
            ResourceClassId::REPRESENTATIONAL,
            LogicalResourceId::new("session-kv")?,
        )
        .allow(DimensionId::REPRESENTATION)
        .allow(DimensionId::RESIDENCY)
        .preserve(Invariant::new(InvariantKind::PreserveContents))
        .preserve(Invariant::new(InvariantKind::UpholdContract(
            ContractId::new("kv.reuse-contract")?,
        )).along(DimensionId::REPRESENTATION))
        .optimize(ObjectiveId::LATENCY)
        .optimize(ObjectiveId::MEMORY_FOOTPRINT)
        .admit(AdmissibleTransition::new(
            TransitionMechanism::Reencode, DimensionId::REPRESENTATION))
        .require_capability(CapabilityRequirement::new(
            TransitionMechanism::Reencode, DimensionId::REPRESENTATION))
        .observe(ObservationSignalId::FREE_CAPACITY)
        .label("workload", "slha-v2");
        builder.build()
    }
}
```

All validation still happens in `ResourceSpecBuilder::build`; the macro adds
no rules of its own and cannot express anything the ordinary API cannot.
`crates/elastic/tests/equivalence.rs` asserts spec-level and EIR-level
equality between both forms, and `examples/macro_declaration.rs` demonstrates
the same at runtime.

## 2. Attribute grammar

| Key | Payload | Repeatable | Lowers to |
|---|---|---|---|
| `class(...)` | built-in class ident or `custom("...")` | no (compile error) | `ResourceClassId` |
| `id("...")` | string literal | no (compile error) | `LogicalResourceId`; default: struct name |
| `allow(dims...)` | dimension idents or `custom("...")`, comma-separated | yes | `.allow(DimensionId::…)` |
| `preserve(...)` | `contents` \| `identity` \| `contract("id")`, optionally `along <dim>` | yes | `Invariant::new(...)[.along(...)]` |
| `optimize(objs...)` | objective idents or `custom("...")` | yes (priority = declaration order across all keys) | `.optimize(ObjectiveId::…)` |
| `admit(mech @ dim)` | mechanism ∈ {`reinterpret`,`reencode`,`recompute`} | yes | `.admit(AdmissibleTransition::new(...))` |
| `capability(mech @ dim)` | same as `admit` | yes | `.require_capability(CapabilityRequirement::new(...))` |
| `observe(sigs...)` | signal idents or `custom("...")` | yes | `.observe(ObservationSignalId::…)` |
| `label("k","v")` | two string literals | yes | `.label(k, v)` |

Mandatory at compile time: exactly one `class(...)`, and at least one
non-empty `allow(...)`. Everything else is validated at build time by the
typed core, exactly as in the manual API (duplicates, vacuous scopes,
ungrounded capabilities return structured errors from `resource_spec()`).

Unknown identifiers produce compile errors listing valid names plus the
`custom("...")` escape hatch, so typos cannot silently become open-set terms.

## 3. Hygiene

- Generated code references only absolute paths (`::core::…`,
  `::elastic::…`) — always through the **user-facing facade**, never through
  implementation crates. A downstream project depending only on `elastic`
  can use the derive; `crates/elastic-downstream` is a workspace member
  that fails to compile if this contract ever regresses.
- The impl lives inside `const _: () = { … }`, so nothing else enters the
  module namespace.
- Generics and where-clauses are preserved via `split_for_impl`
  (`LockHandle<T: Clone>` works; tested).
- Visibility is untouched: the generated function is visible wherever the
  struct is.
- No `unsafe`, no `unwrap`/`expect`: fallible fragments use `?` against
  `ResourceSpecError`.
- Diagnostics are span-accurate (they point at the offending key) and
  combined so multiple problems surface together.

## 4. Compile-fail coverage

`crates/elastic/tests/ui.rs` (trybuild): unknown attribute key, duplicate
mutually exclusive keys, missing class, missing elasticity, unknown
dimension/objective identifiers, empty lists, malformed `mech @ dim`, derive
on non-struct.

## 5. Function-like `elastic! { … }` embedded DSL

The function-like `elastic!` macro supports a standalone resource and a
multi-resource `document` form. Its purpose is to let resource-control intent
read as a declaration rather than requiring carrier Rust structs:

```rust
elastic! {
    pub resource session_kv {
        class(representational);
        id("session-kv");
        allow(representation, residency);
        preserve(contents);
        optimize(latency);
        admit(reencode @ representation);
        capability(reencode @ representation);
    }
}

let spec = session_kv::resource_spec()?;
```

This does **not** introduce a second semantic implementation. The outer DSL
parser understands only `visibility? resource NAME { ... }` and semicolon
delimiters. Each body fragment is parsed by the same declaration model as the
derive surface, and expansion emits an internal `#[derive(ElasticResource)]`
declaration. `ResourceSpecBuilder::build` therefore remains authoritative.

The standalone resource form still owns no group or runtime semantics. Group
and shared-constraint syntax exists only inside the multi-resource document form
and lowers through the typed ELANG3 core/EIR contracts described below. Policy
blocks, implicit runtime startup and composite transaction semantics remain
absent.

`crates/elastic/tests/equivalence.rs` requires manual builder, derive macro and
`elastic!` to produce equal `ResourceSpec`, equal lowered EIR and equal EIR
fingerprints. Trybuild coverage also locks the outer DSL diagnostics.

The complete language-v0.1 grammar, default-identity rule and non-goals are in
[`docs/language/ELASTIC-LANGUAGE-0.1.md`](../language/ELASTIC-LANGUAGE-0.1.md).

### 5.1 Multi-resource `document` form

The next syntax layer groups independently validated resources into the existing
`EirDocument` model:

```rust
elastic! {
    pub document inference_stack {
        resource workers {
            class(shared);
            allow(parallelism);
        }
        resource cache {
            class(representational);
            allow(representation, residency);
            preserve(contents);
        }
    }
}

let eir = inference_stack::document()?;
```

Each child is a public resource module with the same `resource_spec()` function
as the standalone form. `document()` pushes those ordinary specs into the
existing `EirDocumentBuilder`; uniqueness, normalization, sorting and the
document fingerprint therefore remain EIR semantics rather than macro semantics.
The EIR-level `MAX_EIR_DOCUMENT_RESOURCES` bound is also asserted by generated
code rather than copied into the proc-macro crate.

A plain document is still only a declaration/IR container: co-membership alone
does **not** imply shared fate. ELANG3 adds explicit `group NAME { ... }` blocks
when dependency/budget/invariant semantics are intended. `document()` continues
to return the unchanged v0.2 `EirDocument`; a document with groups additionally
exposes `grouped_document()` returning the separate `EirGroupedDocument`
envelope.

The full v0.2 document contract is in
[`docs/language/ELASTIC-LANGUAGE-0.2.md`](../language/ELASTIC-LANGUAGE-0.2.md).
The ELANG3 group grammar and semantics are in
[`docs/language/ELASTIC-LANGUAGE-0.3.md`](../language/ELASTIC-LANGUAGE-0.3.md).

### 5.2 Resource groups, dependencies, budgets and invariants

Inside an `elastic! document`, ELANG3 accepts explicit groups:

```rust
elastic! {
    document drone {
        resource flight { class(configurational); allow(capacity); }
        resource inference { class(configurational); allow(capacity); }

        group runtime {
            members(flight, inference);
            depends(inference -> flight);
            budget memory {
                unit("mib");
                quantum(1);
                maximum(4096);
                term(inference, predicate("elastic.drone", "large-model"), 3072);
            }
            invariant(
                contract("flight-priority"),
                owner(flight),
                participants(flight, inference)
            );
        }
    }
}
```

The proc macro validates resource-module references and grammar only. Dependency
cycles, member legality, shared-budget semantics and invariant ownership remain
`elastic-core` validation. Group lowering lives in `elastic-eir`, and no group
syntax authorizes physical actuation or composite commit/rollback.

## 6. Crate layout

```text
crates/
├── elastic            facade: prelude + re-exports (this guide's entry point)
├── elastic-core       typed model + validation   ← single source of truth
├── elastic-eir        normalized IR lowering
└── elastic-macros     #[derive(ElasticResource)] → elastic-core calls only
```

Dependency direction is acyclic; the facade depends on all three, the macro
crate depends on none of them at runtime (it only emits paths).
