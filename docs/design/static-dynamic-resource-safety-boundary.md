# Static / Dynamic Resource Safety Boundary

**Status:** design note derived from literature review; not a claim of novelty.

## Motivation

The current ElasticXxx literature review now includes three complementary programming-language lines:

1. static quantitative resource bounds through amortized resource analysis;
2. linear resource-aware protocols in concurrent systems;
3. Rust ownership/borrowing semantics through Oxide.

Together they suggest that ElasticXxx should not ask the type system to solve the runtime planning problem, and should not ask the runtime planner to decide legality.

## Three-layer boundary

```text
COMPILE-TIME / TYPE LAYER
    ownership and aliasing discipline
    transition authority / capabilities
    protocol or typestate restrictions where expressible
    semantic contracts expressible statically
    optional static resource bounds
              │
              ▼
TRUSTED RUNTIME VALIDATION LAYER
    physical capability discovery
    current state and capacity
    topology and residency
    epochs / stale-handle checks
    current invariants
    transition protocol legality
              │
              ▼
PLANNER / CONTROL LAYER
    optimize among already-admissible candidates
    use observations, predictions, costs, risks and objectives
```

The planner is never the authority that makes an illegal transition legal.

## Static evidence

Static evidence may include:

- ownership of a resource handle;
- possession of a non-duplicable transition capability;
- a protocol/typestate phase;
- upper bounds on temporary memory or work when statically derivable;
- proof that a representation conversion preserves a declared semantic contract for a restricted class of inputs.

Static evidence may be used to **prune** `ElasticSpace` before dynamic planning.

## Dynamic evidence

Some facts are intrinsically runtime-dependent:

- free RAM / VRAM;
- device or node availability;
- topology changes;
- contention;
- current residency;
- bandwidth;
- queue pressure;
- thermal and power state;
- actual transition latency;
- prediction error and uncertainty.

These must be observed and checked by trusted runtime code.

## Capability direction

A possible Rust direction is:

```rust
pub struct Capability<R, Op> {
    resource: ResourceId,
    generation: Generation,
    _resource: PhantomData<R>,
    _operation: PhantomData<Op>,
}
```

with private construction and non-`Copy` / non-`Clone` behavior by default.

This expresses **authority**, not physical feasibility.

A valid capability may still fail at runtime because the physical state changed. Therefore capabilities should likely carry or resolve against generations, epochs, leases, or another stale-authority mechanism.

## Observation versus mutation

Rust's shared / unique borrowing distinction suggests a useful API discipline:

```text
shared observation
    &Resource / &Handle

local state mutation / transition assembly
    &mut Transaction / unique capability
```

However, distributed or asynchronous physical resources cannot in general be represented by one long-lived `&mut` borrow. A runtime transaction object may own the authority while external state changes are tracked using epochs and verification.

## Typestate direction

Typestate may prevent API-level phase errors:

```text
Prepared -> Requested -> Ready -> Applying -> Verified -> Committed
```

but typestate must not be confused with truth about external hardware. A `Ready` value may become stale if a lease expires or a device disappears. Runtime validation remains authoritative.

## Static quantitative bounds

Amortized resource analysis demonstrates that some quantitative bounds can be inferred statically against an explicit cost semantics.

ElasticXxx may consume such facts as planner evidence, for example:

```text
transition scratch memory <= 64 MiB
maximum messages <= f(n)
maximum temporary replicas <= 2
```

These bounds constrain candidates but do not choose the best runtime action.

## Resource budgets versus physical resources

Resource-aware session types show that accounting potential can be transferred with communication. ElasticXxx may eventually define transferable budget/capability tokens for consumable resources.

This must remain separate from non-consumable state dimensions such as:

- residency;
- locality;
- representation;
- redundancy;
- routing.

There is no evidence yet for one universal resource algebra covering both.

## Refined H5

The Type-Safe Elasticity hypothesis should be interpreted as:

> Rust ownership and type mechanisms can prevent some classes of unauthorized or structurally illegal resource transition, while dynamic physical feasibility and remaining invariants are enforced at a narrow trusted runtime boundary.

This is intentionally weaker and more falsifiable than claiming that all resource legality is statically decidable.

## Experiments required

1. Non-duplicable capabilities versus runtime permission IDs.
2. Typestate versus runtime state enum for transition protocols.
3. Static resource-bound evidence as `ElasticSpace` pruning input.
4. Async cancellation and stale-capability behavior.
5. Multi-resource transactions with partially static and partially dynamic validation.
6. Compile-time rejection coverage versus API complexity and compilation cost.

## Prior work anchors

- Hoffmann, Aehlig & Hofmann, *Multivariate Amortized Resource Analysis*, POPL 2011.
- Das, Hoffmann & Pfenning, *Work Analysis with Resource-Aware Session Types*, LICS 2018.
- Weiss, Gierczak, Patterson & Ahmed, *Oxide: The Essence of Rust*, arXiv v4, 2021.
