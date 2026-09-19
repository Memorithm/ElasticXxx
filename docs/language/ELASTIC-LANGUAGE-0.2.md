# Elastic embedded language v0.2 — multi-resource documents

Status: typed multi-resource declaration over the existing EIR document model.
This version adds **no** group, shared-budget, dependency, or composite-actuation
semantics.

## Syntax

```rust
use elastic::prelude::*;

elastic! {
    pub document inference_stack {
        resource ram {
            class(capacity_resource);
            allow(capacity);
        }
        resource kv {
            class(representational);
            allow(representation, residency);
            preserve(contents);
        }
    }
}

let document = inference_stack::document()?;
```

A document contains one or more `resource NAME { ... }` blocks. Child resource
bodies are exactly the v0.1 resource grammar and lower through the same
`ElasticResource`/`ResourceSpecBuilder` path.

## Semantic model

No new `ResourceSet` type is introduced. ElasticXxx already has the correct
multi-resource semantic container: `EirDocument` and `EirDocumentBuilder`.
Therefore the lowering path is:

```text
resource block ─→ ResourceSpec ─┐
resource block ─→ ResourceSpec ─┼→ EirDocumentBuilder → EirDocument
resource block ─→ ResourceSpec ─┘
```

`EirDocument` remains authoritative for cross-resource structural properties in
this version:

- logical resource identities are unique;
- resources are normalized in identity order;
- insertion/syntax order does not alter the document;
- the document fingerprint absorbs every normalized resource fingerprint;
- an empty document is invalid.

The language module exposes each resource as a public child module, so callers
may still request an individual `ResourceSpec` when needed.

## Structural bound

One EIR document is bounded to `MAX_EIR_DOCUMENT_RESOURCES = 256`. The bound is
implemented in EIR itself, not duplicated in the proc macro, and applies to
`EirDocumentBuilder`, raw-parts construction and final assembly.

This is a resource-exhaustion bound, not a claim that an orchestration system
may control only 256 resources. Larger systems must partition resource
ownership into multiple documents until a higher-level orchestration contract
is explicitly defined.

## Error boundary

`document()` returns `Result<EirDocument, ElasticDocumentError>`.

- `ElasticDocumentError::Resource` preserves the Rust child module name and the
  underlying `ResourceSpecError` when one child declaration is invalid;
- `ElasticDocumentError::Eir` preserves authoritative EIR document validation
  failures such as duplicate logical resource identity or the resource-count
  bound.

No error is converted into a fallback/default resource.

## Explicit non-goals

v0.2 does not define:

- resource groups;
- dependency edges;
- shared memory/energy/worker budgets;
- cross-resource invariants;
- transition ordering between resources;
- atomic or best-effort multi-resource actuation;
- composite rollback;
- policy blocks.

Those belong to ELANG-3/4 and must first obtain typed semantics. Merely placing
resources in the same `document` does **not** imply that they share fate,
budgets, transaction boundaries, or execution order.
