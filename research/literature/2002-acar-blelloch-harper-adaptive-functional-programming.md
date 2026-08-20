# Adaptive Functional Programming

**Paper:** Umut A. Acar, Guy E. Blelloch, Robert Harper. *Adaptive Functional Programming*. POPL 2002.

**Primary source:** https://www.cs.cmu.edu/~guyb/papers/popl02.pdf

## Problem

**SOURCE-DERIVED.** The paper asks how a computation can update its output after an input change without reevaluating the whole program.

## Core mechanism

The execution records dependencies through modifiable references and reads. The implementation maintains an augmented dependency graph / trace. When an input changes, change propagation invalidates and reevaluates only affected reads/subexpressions, repairs the execution trace, and skips unaffected regions.

The paper proves that change propagation yields the same result as complete reevaluation under its language semantics. It also analyzes propagation cost in terms of invalidated/updated dependencies rather than total original computation size.

## Elastic relation

**ADOPT / GENERALIZE.** This is strong prior art for `REPAIR_DERIVED_STATE`: a derived state may be repaired by following dependency information instead of being rebuilt from scratch.

The important abstraction is not “recompute some tokens” but:

```text
source changes
    ↓
dependency invalidation
    ↓
identify affected derived region
    ↓
re-evaluate only affected computation
    ↓
repair dependency trace
    ↓
result equivalent to full reevaluation
```

## New distinction for Elastic

A dependency graph is not the derived resource itself. It is **maintenance state** retained in order to make future repair cheaper.

Proposed separation:

```text
DerivedMaterialization
DerivationProvenance
ReuseWitness
MaintenanceIndex / DependencyTrace
```

The maintenance index has its own memory, update and synchronization costs and should therefore be resource-accounted.

## Correctness requirement

**ADOPT.** Partial repair should have an explicit correctness criterion. Under an Exact semantic contract, the gold standard is equivalence to valid full rematerialization, not merely empirical similarity.

## SciRust check

No generic self-adjusting/change-propagation runtime was identified in the current SciRust search. This is not classified as a SciRust gap: dynamic dependency tracking and trace repair are primarily systems/runtime mechanisms unless an independent general scientific-computing need emerges.
