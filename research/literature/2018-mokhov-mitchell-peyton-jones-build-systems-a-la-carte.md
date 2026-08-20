# Build Systems à la Carte

**Paper:** Andrey Mokhov, Neil Mitchell, Simon Peyton Jones. *Build Systems à la Carte*. Proc. ACM Program. Lang. 2(ICFP), 2018.

**Primary source:** https://simon.peytonjones.org/assets/pdfs/build-systems-original.pdf

## Problem

**SOURCE-DERIVED.** The paper develops a common executable framework for understanding and composing build-system mechanisms rather than treating Make, Shake, Bazel, Buck, Nix, Excel and related systems as unrelated designs.

## Central decomposition

The paper separates two concerns:

- **Scheduler** — which keys/tasks are rebuilt and in what order;
- **Rebuilder** — whether a key actually needs rebuilding and what persistent build information is used to make that decision.

These components are designed to be recombinable.

## Validity metadata

The paper studies several rebuild strategies. Particularly relevant are **verifying traces**, which record hashes of the values/dependencies observed during a previous build. A later build can check that trace to decide whether an existing value remains up to date without necessarily storing a complete explanatory provenance record.

This yields a strong Elastic distinction:

```text
DerivationProvenance
    answers: how was this artifact derived?

ReuseWitness / VerificationTrace
    answers: is this existing materialization still valid enough to reuse?
```

These objects can overlap but need not be identical.

## Scheduler versus validity logic

**ADOPT / GENERALIZE.** The build-system separation suggests a similar decomposition for derived Elastic resources:

```text
Validity / Reuse Policy
        ↓
Repair/Rebuild Candidates
        ↓
Repair Scheduler
```

The component deciding *whether* an object is stale should not necessarily decide the global ordering or resource allocation of repairs.

## Dynamic dependencies

Different build systems vary in whether dependencies are known statically or discovered during task execution. This matters for Elastic because derivation dependency structure can itself be runtime state.

## Correctness

A correct build must produce a value consistent with the current dependency state; minimality is a separate optimization criterion. This reinforces the Elastic rule:

```text
correctness / validity constraint
    !=
minimal repair work objective
```

## Elastic disposition

- Separate scheduler from rebuild/validity policy — **ADOPT / GENERALIZE**.
- Persistent verification traces — **ADOPT / GENERALIZE**.
- Hash-only witnesses when semantically sufficient — **ADAPT**, domain validator decides sufficiency.
- Build-tool-specific task/key model — **ADAPT**, not a universal Elastic API.

## SciRust check

The build mechanisms are systems/runtime architecture rather than missing scientific primitives. No SciRust gap is declared from this paper.
