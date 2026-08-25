# Rust Surface Model v0.1

**Status:** normative for the implemented code in `crates/elastic-core` (`resource`
module), version 0.1. Statements marked **[H]** are research hypotheses from the
whitepaper and are explicitly *not* implemented semantics.

---

## 1. Purpose

The Rust Surface Model is the application-facing layer between ordinary Rust
code and the Elastic runtime machinery. It lets a program declare, in typed
Rust:

- what a resource logically *is*;
- which properties may change (elastic dimensions);
- which transitions are admissible;
- which properties must remain true (invariants);
- what the runtime may try to improve (objectives);
- which trusted capabilities are required;
- which observations may inform adaptation.

The guiding principle:

> Application code declares what may change, what must remain true, and what it
> wants optimized. The Elastic system may choose only transitions that are
> explicitly admissible and validated.

```text
APPLICATION INTENT (ordinary Rust or macros)
        ↓
RUST SURFACE MODEL      ← this document (elastic_core::resource)
        ↓
NORMALIZED ELASTIC MODEL / EIR   (later layer)
        ↓
VALIDATED PLAN / CONTRACTS       (existing representation/frontier contracts)
        ↓
RESOURCE-SPECIFIC ADAPTER        (e.g. elastic-kv)
        ↓
PHYSICAL ACTION                  (trusted runtime boundary; out of scope here)
```

## 2. Concepts

### Resource

A logical adaptive entity identified by a stable [`LogicalResourceId`] and a
semantic [`ResourceClassId`] (stock, capacity, rate, exclusive, shared,
stateful, representational, configurational — mirroring whitepaper §4).
Logical identity is independent of physical realization: the same logical
resource may change residency, representation, or other elastic dimensions
while remaining the same resource. **[H]** this separation remains sound under
migration and replication.

Implemented: `ResourceSpec::builder(class, id)`; identifiers are validated
non-empty texts.

### State

v0.1 deliberately does **not** introduce a universal concrete state type.
The admissible state space S is treated as *derived* from the declared
dimensions D, admissible transitions T, and invariants I. Concrete materialized
states remain resource-adapter-specific: representational resources keep using
`RepresentationState` (id, schema version, epoch), which already carries the
trusted-boundary discipline. A generic state payload would either be `dyn Any`
or duplicate adapter concepts weakly; both are rejected for v0.1.

### Dimension

One axis along which a resource may legally change. Built-in dimensions:
`capacity, concurrency, residency, locality, representation, precision,
parallelism, routing, redundancy, persistence, recomputability, bandwidth,
energy`.

Normative rules implemented in v0.1:

- A declaration must allow at least one dimension (a rigid resource gains
  nothing from the elastic model).
- Dimensions are unique within a declaration.
- Admissible transitions and capability requirements may only concern
  dimensions that are declared elastic.

The set is open: any crate may define additional `DimensionId::custom("...")`
terms without modifying `elastic-core`. Custom terms never shadow built-ins.

### Invariant

A property adaptation is forbidden to violate. Invariants are constraints —
they are structurally distinct types from objectives and can never be traded
off against them. Implemented kinds: `PreserveContents`,
`PreserveIdentity`, `UpholdContract(ContractId)` where the contract is an
externally defined semantic contract (e.g. a KV reuse contract) whose
interpretation belongs to the resource adapter.

An invariant applies to all transitions, or is scoped along one elastic
dimension. Scoping to a non-elastic dimension is rejected as vacuous.

**[H]** whether these three kinds plus adapters suffice for all useful
contracts is an open research question.

### Objective

Something the runtime may try to improve. Built-ins: `latency, throughput,
memory-footprint, energy, migration-cost, stability`; open set via custom
objectives.

Objectives carry a strict priority order (first declared = highest priority).
There is deliberately **no** universal scalar cost model in v0.1: ordered
typed objectives are the conservative representation, and planners may refine
within an objective but must not invent cross-objective exchange rates.

### Capability

What the trusted runtime boundary must provide so an admitted transition can
execute. Applications may *require* capabilities
(`CapabilityRequirement::new(mechanism, dimension)`); they cannot fabricate
them. This mirrors the existing rule that `CapabilitySet` snapshots come from
authoritative discovery, and that transition execution needs trusted-boundary
attestations (`TransitionAttestations`, `EvidenceToken`). Requirements are
recorded intent; nothing executes on their word.

### Transition

An admitted way of changing the resource: one shared mechanism class applied
along one elastic dimension. The mechanism vocabulary is the same as the
representation layer's (`Reinterpret`, `Reencode`, `Recompute`) because those
are general semantic classes, not representation-specific details:
reuse-the-materialization, transform-in-place, regenerate-from-source.

Declaring a transition does not make a specific target state legal. Planning
and validation stay distinct; validation happens against capabilities and
attestations at the adapter boundary (see the frontier).

### Observation

A signal the declaring resource expects to be relevant to adaptation decisions
(free capacity, utilization, queue depth, latency samples, thermal margin,
energy rate, topology change; open set). v0.1 records relevance only; no
sampling interface is defined yet. **[H]** observation-driven planning is
future work.

### Plan

Not part of the surface model in v0.1. The mission boundary is explicit: this
layer defines intent, state, admissibility, lowering, and validation.
Planners come later behind extensible interfaces whose honest outcome set is
{candidate, no candidate, insufficient evidence, unsupported}.

## 3. Mapping to R = (K, S, D, T, I, M)

| Model element | Surface model representation |
|---|---|
| K — kind/capabilities | `ResourceClassId` + `LogicalResourceId` + `CapabilityRequirement`s |
| S — state space | derived from D × T × I (no concrete generic state type) |
| D — dimensions | `allow(dimension)` declarations |
| T — transitions | `admit(AdmissibleTransition)` declarations |
| I — invariants | `preserve(invariant)` declarations |
| M — observations/costs | `observe(signal)` + ordered `optimize(objective)` |

## 4. Extensibility balance

Pure string keys ("dimension(\"memory\")") would silently drift into core
semantics and are rejected. One giant closed enum of every future device would
force core edits per downstream resource and is equally rejected. The chosen
balance:

1. built-in semantics = typed enum variants with canonical text;
2. extension terms = validated custom identifiers that order after built-ins;
3. core validation matches on structure and uniqueness, and on built-in
   semantics only when a rule is genuinely universal.

Downstream crates get compile-time-checked built-ins plus open extension
without touching `elastic-core`.

## 5. Validation guarantees (implemented)

`ResourceSpecBuilder::build` returns structured errors
(`elastic_core::ResourceSpecError`) for: blank resource/term identifiers,
blank label keys, empty elasticity, duplicate dimensions/objectives/
invariants/transitions/capabilities/signals, invariants scoped to non-elastic
dimensions (vacuous), transitions beyond elastic dimensions, capability
requirements beyond elastic dimensions, and capability requirements that
ground no admitted transition.

No panic path exists for ordinary invalid input. Validation proves structural
consistency of the declaration only — never satisfiability of a plan, never
authenticity of capabilities.

## 6. Cross-cutting properties

- **No unsafe:** `elastic-core` is `#![forbid(unsafe_code)]`.
- **Determinism:** unordered collections normalize to sorted order at build;
  equal declarations compare equal and iterate identically regardless of
  construction order. Objectives intentionally keep priority order.
- **Thread safety:** all declaration types are immutable plain data; they are
  `Send + Sync` by construction (tested). No locks live in the semantic core.
- **Portability:** no filesystem, network, environment, clock, or OS handle
  dependencies were introduced; the module stays compatible with a future
  `no_std` evolution (only `std::error::Error` plumbing is std-bound today,
  matching existing crate style).
- **Trusted boundary:** declarations are claims. Capability discovery,
  attestation issuance, and physical action remain with the trusted runtime,
  exactly as established by the representation layer.

[`LogicalResourceId`]: ../../crates/elastic-core/src/resource/terms.rs
[`ResourceClassId`]: ../../crates/elastic-core/src/resource/terms.rs
