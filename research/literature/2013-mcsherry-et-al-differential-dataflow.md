# McSherry et al. (2013) — Differential Dataflow

## Status

**SOURCE-DERIVED mechanism review with ELASTIC PROPOSALS clearly separated.**

Primary source: Frank McSherry, Derek G. Murray, Rebecca Isaacs, Michael Isard, *Differential Dataflow*, CIDR 2013.

## Problem

Traditional incremental computation usually advances through a total sequence of versions. That model becomes awkward when a computation has several independent axes of change, especially when external input changes coexist with nested iterative loops.

Differential computation generalizes incremental computation so that collection versions belong to a **partial order**. Changes are represented explicitly as differences associated with versions, rather than immediately being folded into one current state and discarded.

## Resource / state model

A collection `A` varies over versions `t` from a partially ordered set `(T, <=)`.

The state at version `t` is reconstructed from prior differences:

```text
A_t = Σ_{s <= t} δA_s
```

Differences may be positive or negative, so updates can insert or retract records.

For an external input round `i` and loop iteration `j`, the paper gives a product order such as:

```text
(i1, j1) <= (i2, j2)
iff
i1 <= i2 && j1 <= j2
```

This permits state at `(i,j)` to reuse work from several incomparable predecessor directions rather than selecting one arbitrary predecessor in a total order.

## Maintenance state

The prototype stores complete input **difference traces** for sufficiently general operators. The paper describes a sparse in-memory index keyed by key, lattice version, and record. Non-zero counts are retained so arbitrary relevant collection versions can be reconstructed.

This is important: historical differences are not merely logs for observability. They are **operational maintenance state** required to derive future results efficiently.

Operator implementations may maintain less state when algebraic structure permits it. For example, the paper notes that some aggregations can use compact cumulative state, whereas `Min`/`Max` may need richer historical traces because retracting the current extremum can expose a previous value.

## Scheduling and causality

The scheduler must reconcile cyclic dependencies. It orders outstanding differences using both:

- their logical version;
- the dataflow topology / causal possibility that processing one difference may generate another.

The paper identifies minimal outstanding work items and schedules from that causally minimal set. Fixed-point convergence is represented by absence of further differences rather than a separate global convergence test.

## Modern Rust implementation check

The current Rust `differential-dataflow` implementation retains the same conceptual machinery and makes compaction explicit through trace frontiers.

`TraceReader` exposes separate logical and physical compaction frontiers. Current documentation states that **logical compaction** allows historical timestamp distinctions to be coalesced once future queries no longer need to distinguish them, while **physical compaction** controls merging of physical batches.

This modern implementation evidence strengthens an important conclusion: retained historical distinctions themselves have a lifecycle and can be safely discarded only after progress/usage constraints permit it.

## Results

The CIDR paper demonstrates large reductions in update work for iterative graph computations under small input changes. In its Twitter connected-components example, sliding the input window by one second requires only a very small fraction of the work of full prioritized reevaluation. These are workload-specific results, not a generic performance guarantee.

## Limitations / costs

- General operators can require substantial indexed historical state.
- Partial-order versioning complicates operator logic and scheduling.
- Benefits depend on changes being small enough and prior state being reusable enough to amortize maintenance overhead.
- Historical state cannot grow without bound; implementation-level compaction policy is therefore essential.

## Elastic relation

### ADOPT

- Distinguish **current materialized state** from a **versioned delta trace** used to maintain it.
- Permit logical versions that are only partially ordered.
- Treat historical maintenance state as a first-class cost, not free metadata.
- Compact historical distinctions only after a progress/validity frontier proves they are no longer observable or required.

### ADAPT

ElasticXxx should not impose differential-dataflow semantics on all resources. Instead, the generic model should permit a resource adapter to expose:

```text
LogicalVersion
Delta / ChangeSet
MaintenanceTrace
CompactionFrontier
```

when the domain benefits from incremental reconstruction.

### REJECT

Do not require every adaptive resource to retain all transitions indefinitely or to reconstruct its state as an additive sum of deltas. Differential traces are one maintenance strategy, not the universal Elastic state model.

## Elastic proposal: version frontier versus scalar epoch

A scalar generation remains useful for local freshness/revocation checks. But a distributed or nested computation may require a partially ordered logical version and an **antichain frontier** representing several incomparable minimal outstanding versions.

Therefore distinguish conceptually:

```text
ResourceGeneration      // local stale-handle/revocation generation
LogicalVersion          // domain-specific partially ordered version
VersionFrontier<V>      // boundary of incomplete / still-relevant versions
```

This is an ELASTIC PROPOSAL derived from the prior-art mechanisms; it is not a novelty claim.

## Elastic proposal: maintenance-state elasticity

The modern differential-dataflow trace API suggests a deeper generalization:

```text
MaintenanceState
    ├── logical history distinctions
    ├── physical representation / batches
    └── compaction frontiers
```

Maintenance state may therefore itself be elastic. A planner can trade memory footprint against future incremental-repair/reuse capability, but compaction must remain constrained by correctness/frontier semantics.

## Experiment required

Build a small Elastic derived-resource prototype with two independent version axes and compare:

1. total-order generations + full recomputation;
2. partial-order delta trace;
3. delta trace with frontier-driven logical compaction;
4. aggressive compaction that intentionally violates the frontier, as a negative-control correctness test.

Measure maintenance memory, update latency, compaction cost, reconstruction cost, planner overhead, and semantic correctness.
