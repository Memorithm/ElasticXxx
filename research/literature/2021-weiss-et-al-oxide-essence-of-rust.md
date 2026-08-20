# Oxide: The Essence of Rust

**Paper:** Aaron Weiss, Olek Gierczak, Daniel Patterson, Amal Ahmed. *Oxide: The Essence of Rust*. arXiv:1903.00982, v4 (2021).

**Primary source:** https://arxiv.org/pdf/1903.00982

## Why this paper matters for ElasticXxx

ElasticXxx is intended to be integrated into Rust, so claims such as “Rust can make elasticity type-safe” must be grounded in what Rust ownership and borrowing actually provide.

Oxide is a formal, source-level model of core safe Rust's ownership and borrow-checking discipline. It is therefore useful as a boundary marker: it tells us what kinds of guarantees come naturally from Rust's substructural ownership model and what ElasticXxx would have to add separately.

## SOURCE-DERIVED mechanism

Oxide models ownership, moves, shared and unique borrowing, regions/provenances, and non-lexical lifetimes using a control-flow-based substructural typing judgment. The work proves syntactic type safety using progress and preservation and validates its semantics against a supported subset of Rust borrow-checking behavior.

A moved non-copyable value cannot be used again. Shared borrows permit aliasing but prevent unguarded mutation through those references; unique borrows preserve uniqueness for mutation. Regions approximate the origins/provenances of references rather than representing physical memory tiers.

The paper explicitly positions Rust in a lineage of linear types, ownership types, and region-based memory management, while emphasizing Rust's practical balance of expressivity and usability.

Oxide focuses on safe core Rust and intentionally omits several orthogonal mechanisms, including concurrency in the core model and much unsafe/library machinery.

## What Rust ownership gives ElasticXxx

**INFERENCE grounded in Oxide.** Rust is naturally useful for controlling **authority over resource-manipulating values**:

- a capability can be non-`Copy` and non-`Clone`;
- ownership transfer can move exclusive transition authority;
- shared borrowing can expose observation without granting mutation;
- unique borrowing can guard state-changing operations;
- lifetimes can prevent handles from outliving the authority or resource context they borrow from.

These are strong implementation tools for ElasticXxx.

## What Rust ownership does not automatically give ElasticXxx

Oxide does not provide quantitative bounds for memory, time, bandwidth, energy, or migration cost, and it does not reason about dynamic GPU/NUMA/NVMe residency, free capacity, contention, thermal state, or future availability.

Therefore the following statement would be too strong:

> “Rust's type system proves that an Elastic transition is physically feasible.”

A more accurate target is:

> **Rust types may prove or enforce that code possesses the correct authority and follows some structural transition rules; a trusted runtime must still validate dynamic physical feasibility and current-state invariants.**

## ElasticXxx relation

### ADOPT — ownership for transition authority

A candidate direction is an affine capability handle:

```rust
pub struct ElasticHandle<R: ElasticResource> {
    id: ResourceId,
    capability: Capability<R>,
}
```

with private construction and operation-specific capability types.

This is an **ELASTIC PROPOSAL**, not an Oxide construct.

### ADOPT — borrow distinction for observe versus mutate

Observation can often use shared access, while a state-changing transition may require unique access to the local resource state or transaction object.

This maps naturally onto Rust but must be reconciled with asynchronous execution and distributed resources, where unique physical authority cannot always be represented by one ordinary `&mut` borrow.

### ADAPT — typestate for lifecycle phases

Elastic transitions may have phases such as `Prepared`, `Pending`, `Ready`, `Applying`, `Verified`, and `Committed`. Rust typestate could prevent certain API-level phase errors.

However, hardware/runtime truth may change asynchronously. Typestate alone cannot guarantee that an external GPU allocation still exists or that a remote node remains reachable. Runtime generations/epochs, capability validation, or leases may therefore still be necessary.

### REJECT — equating ownership with residency

Ownership answers who controls a value. Residency answers where its physical representation currently exists. They must remain distinct dimensions.

```text
ownership != residency
lifetime  != placement lifetime
borrow    != resource allocation
```

This distinction is fundamental for ElasticXxx.

## Static versus dynamic safety boundary

The combined literature now suggests a layered model:

```text
COMPILE-TIME / TYPE LAYER
    ownership
    aliasing discipline
    transition authority
    protocol / typestate rules where expressible
    optional static resource bounds

TRUSTED RUNTIME LAYER
    physical capability discovery
    current capacity
    topology
    dynamic contention
    transactional transition protocol
    epochs / stale-handle detection
    final invariant validation

PLANNER LAYER
    optimization among already-admissible candidates
```

The planner should not be able to bypass either safety layer.

## Research consequence for H5

The existing H5 Type-Safe Elasticity hypothesis should be interpreted narrowly enough to be falsifiable:

> Rust ownership and type mechanisms can prevent some classes of unauthorized or structurally illegal resource transition, while remaining dynamic conditions are checked at a narrow trusted runtime boundary.

ElasticXxx should **not** hypothesize that all resource legality can be decided statically.

## SciRust gap check

No SciRust gap. Oxide is a programming-language semantics result rather than a missing scientific-computing capability.

## Current classification

| Oxide/Rust mechanism | ElasticXxx disposition |
|---|---|
| Move / affine ownership | **ADOPT for capability authority** |
| Shared versus unique borrowing | **ADOPT / ADAPT for observe versus mutate** |
| Regions / provenance-style reasoning | **INVESTIGATE for handle validity; do not equate with residency** |
| Substructural typing | **ADOPT principle** |
| Ownership as physical resource model | **REJECT** |
| Compile-time proof of dynamic hardware availability | **REJECT** |

## Open experiments

1. Prototype non-duplicable `Capability<R, Op>` values and measure API ergonomics.
2. Compare typestate transitions against a runtime enum/state-machine API.
3. Test async cancellation, partial failure, stale handles, and multi-resource transactions to find the exact boundary where runtime epochs/leases become unavoidable.
4. Verify that the safe API cannot construct or replay a transition using invalid authority without entering an explicit `unsafe` or trusted adapter boundary.
