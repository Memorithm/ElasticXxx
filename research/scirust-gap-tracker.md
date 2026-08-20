# SciRust Capability Gap Tracker

SciRust is treated here as a scientific R&D environment used to design, analyze, validate, benchmark, and improve algorithms and models. **ElasticXxx must never require SciRust as a runtime dependency.**

This tracker records only scientifically general capabilities revealed while working on ElasticXxx. Project-specific runtime mechanisms stay in the target project.

## Status vocabulary

- **INVESTIGATE** — possible gap; evidence is not yet sufficient for implementation.
- **CONFIRMED GAP** — repository/state-of-the-art audit establishes that a required general capability is missing or insufficient.
- **EXTERNAL FIRST** — mature external tooling should be preferred to reimplementation.
- **IMPLEMENT** — evidence justifies adding a general capability.
- **CLOSED** — capability already exists or the research need disappeared.

---

# Candidate gaps

## SCIRUST-GAP-OPT-001 — Generic LP / ILP / MILP capability

**Status:** INVESTIGATE

**Origins:** Alpa; Multivariate Amortized Resource Analysis; independently reinforced by FlexGen.

### Need revealed

- Alpa uses ILP for a structured intra-operator planning subproblem.
- Multivariate amortized resource analysis uses LP/linear constraints to infer quantitative resource bounds.
- FlexGen uses LP after enumerating a small number of discrete batch/block configurations to jointly choose GPU/CPU/disk placement fractions.

### Current evidence

Inspection of `scirust-solvers` found continuous optimizers including BFGS, gradient methods, Nelder–Mead and SPG, plus specialized combinatorial branch-and-bound functionality, but no clearly exposed general LP/ILP/MILP modelling/solver API.

A search for `simplex` finds the **Simplex safety architecture** in `scirust-func-safety`, not the simplex LP algorithm. A direct `linprog` search previously returned no general solver.

### Why still only INVESTIGATE

1. related functionality could exist under another abstraction;
2. mature external Rust/native solver interfaces may be scientifically preferable;
3. the useful SciRust contribution may be a modelling/interface layer rather than an in-house full solver;
4. ElasticXxx itself has not yet demonstrated that LP/MIP is the best production backend for one concrete planning domain.

### General usefulness independent of ElasticXxx

Scheduling, assignment, packing, routing, placement, experimental design, resource allocation and static resource-bound inference.

---

## SCIRUST-GAP-CONTROL-001 — Advanced nonlinear / explicit / multi-rate predictive control

**Status:** INVESTIGATE

**Origins:** Mandal et al. predictive/learned resource control; Zanini et al. thermal MPC.

### Existing SciRust capabilities verified

- `scirust-control/src/mpc.rs`: condensed finite-horizon **linear MPC** for `x_{k+1}=Ax_k+Bu_k`, quadratic objective and hard box input constraints via box QP;
- `scirust-estimation/src/rls.rs`: deterministic recursive least squares with forgetting factor and allocation-free update hot loop;
- `scirust-sim/src/thermal.rs`: basic thermal dynamic models, including Newton cooling and 1-D transient heat conduction.

Generic MPC, online RLS and basic thermal simulation must therefore not be described as missing.

### Capability not identified in current inspection

- nonlinear MPC (NMPC);
- explicit MPC / explicit-NMPC policy generation or approximation;
- reusable multi-rate predictive-control framework;
- reusable adaptive system-identification/sensitivity-model ↔ constrained-MPC integration.

### Why still only INVESTIGATE

Related pieces may exist elsewhere, external solver integrations may be preferable, and no concrete ElasticXxx dynamics model yet demonstrates that simpler control is insufficient.

### General usefulness independent of ElasticXxx

Robotics, process control, energy systems, thermal control, autonomous systems, vehicles and industrial optimization.

---

## SCIRUST-GAP-STATS-001 — Survival / censored time-to-event analysis

**Status:** INVESTIGATE

**Origin:** production KV-cache lifetime/reuse modelling in Wang et al. (USENIX ATC 2025), plus SciRust's stated statistics/reliability scope.

### Need revealed

Real resource-management problems often concern a future event time rather than a scalar regression target:

```text
when will this object be reused?
when will this component fail?
when will this event occur?
```

Production KV-cache traces show that reuse probability and lifetime are meaningful policy variables and can be workload-conditioned.

### Existing SciRust capability verified

`scirust-stats` already exposes continuous distributions including `Exponential`, with PDF, CDF, survival function, quantile and moments. Therefore **parametric exponential probability modelling is not missing**.

### Capability not identified in repository search

No general module was found for censored time-to-event analysis such as:

- Kaplan–Meier survival estimation;
- Nelson–Aalen cumulative hazard;
- log-rank comparison;
- Cox proportional hazards or related semiparametric modelling;
- explicit right-censoring data types/likelihood utilities.

### Why not implemented immediately

The KV-cache paper itself does not require censored survival analysis; it primarily fits empirical/exponential reuse distributions. A dedicated survival-analysis module should be added only after a broader state-of-the-art audit and an independent scientific use case confirms the need.

### General usefulness independent of ElasticXxx

Reliability engineering, predictive maintenance, medical/event-time statistics, component lifetime analysis and reuse/lifetime modelling.

---

# Implemented general enrichments revealed by this research

These are **not ElasticXxx dependencies** and are deliberately not target-specific.

## SCIRUST-ENRICH-ALGEBRA-001 — Generic semiring abstraction

**Status:** IMPLEMENTED on SciRust `master`; no CI status was yet reported for the integration commit when this tracker was updated.

**Origin:** Green, Karvounarakis & Tannen, *Provenance Semirings* (PODS 2007), followed by direct inspection of `scirust-algebra`.

Repository inspection showed `Magma`, `Semigroup`, `Monoid`, `Group`, `Ring` and `Field`, but no general `Semiring` abstraction.

Added:

```text
scirust-algebra/src/semiring.rs
```

with:

- `Semiring`;
- `CommutativeSemiring`;
- `RingSemiring<T>` as a non-breaking adapter from the existing `Ring` hierarchy;
- `BooleanSemiring` as an exact concrete example.

The abstraction is intentionally general. No database-specific provenance representation was added.

General uses include algebraic dynamic programming, weighted automata, shortest-path/tropical methods, formal-language algorithms, provenance, probabilistic/algebraic computation and other semiring-generic algorithms.

## SCIRUST-ENRICH-ALGEBRA-002 — Partial orders, lattices, product orders, and antichains

**Status:** IMPLEMENTED on SciRust `master`; GitHub reported no CI status for the integration commit when this tracker was updated.

**Origins:** Differential Dataflow and Naiad, followed by direct inspection of `scirust-algebra`.

Repository searches found no reusable mathematical abstraction for partial orders, joins/meets, lattices, coordinate-wise product orders, or antichains. Rust's standard ordering traits are not sufficient for every scientific order relation; for example, a coordinate-wise product order intentionally permits incomparable tuples.

Added:

```text
scirust-algebra/src/order.rs
```

with:

- `PartiallyOrdered`;
- `JoinSemilattice`;
- `MeetSemilattice`;
- `Lattice`;
- `TotalOrder<T>`;
- `ProductOrder2<A,B>`;
- deterministic finite `Antichain<T>` of minimal elements.

The abstraction is intentionally general. No timely-dataflow, differential-dataflow, or Elastic-specific timestamp type was added.

General uses include causal/logical-time orders, distributed progress frontiers, Pareto/minimal boundaries, dependency/version orders, abstract interpretation, order-theoretic algorithms, and fixed-point methods.

## SCIRUST-ENRICH-OPT-001 — Exact additive subset selection under a budget

**Status:** IMPLEMENTED on SciRust `master`; local/CI validation status must be checked separately before treating the change as release-qualified.

Module:

```text
scirust-solvers/src/combinatorial/budgeted_selection.rs
```

Solves exactly and deterministically:

```text
maximize   Σ utility_i x_i
subject to Σ cost_i x_i ≤ budget
           x_i ∈ {0,1}
```

using a sparse Pareto-frontier dynamic program with deterministic tie-breaking.

General uses include bounded experimental selection, additive cache/item selection, task portfolios and exact small-instance baselines for heuristic evaluation.

## SCIRUST-ENRICH-OPT-002 — Greedy monotone-submodular selection

**Status:** IMPLEMENTED on SciRust `master`; local/CI validation status must be checked separately before treating the change as release-qualified.

Module:

```text
scirust-solvers/src/combinatorial/submodular.rs
```

Provides deterministic greedy maximization:

```text
maximize   F(S)
subject to |S| ≤ k
```

for a caller-supplied normalized, monotone, submodular objective with exact non-negative marginal gains.

The classical `(1 - 1/e)` guarantee is exposed **conditionally on those mathematical assumptions**. SciRust does not pretend to prove monotonicity/submodularity of an arbitrary black-box callback from samples.

General uses include coverage, diversity-aware selection, sensor placement, summarization, experimental design and caching with diminishing returns.

### Deliberately not generalized yet

No automatic addition has been made for dynamic/streaming submodular optimization, non-monotone variants, knapsack/matroid constraints, distributed submodular algorithms, noisy marginal oracles, automatic submodularity certification, complete lattices, distributive lattices, fixed-capacity antichains, or a distributed progress tracker. Add them only when independent scientific needs justify them.

---

# Reviews that did not justify a new SciRust capability

The following papers primarily revealed target-runtime architecture or were already covered by existing SciRust primitives:

- Moreau & Queinnec — resource semantics/accounting;
- Invasive Computing — allocation claims/granularity;
- Pollux — goodput/co-adaptation;
- Huber et al. / Sandås et al. — dynamic resource management and MPI malleability;
- NOMAD — transactional page migration;
- FlexMem — feedback tiering; SciRust already has EWMA/CUSUM primitives;
- Tiered Memory Beyond Hotness — lightweight performance-impact modelling;
- Resource-aware session types / Oxide — programming-language semantics;
- Autopilot — statistics/cost/safety-margin autoscaling;
- AWARE — RL/meta-learning capabilities already exist; lifecycle/fallback largely runtime architecture;
- Cilk / A-STEAL / BWoS — scheduling/concurrent-runtime mechanisms;
- PowerDial / GEOPM — semantic knobs and scalable power actuation;
- PagedAttention/vLLM — paging is already represented in SciRust KV research;
- InfiniGen / Quest / IMPRESS — attention-specific importance and selection mechanisms;
- Llumnix / DistServe / Mooncake — live migration, serving disaggregation and distributed physical KV placement;
- DiffKV — GPU compaction and attention-specific differentiated K/V compression;
- CacheGen — KV-specific transport codec and streaming adaptation;
- Adaptive Functional Programming — generic dependency-trace/change-propagation runtime; currently a systems/runtime mechanism rather than a missing scientific primitive;
- DBToaster — higher-order database-view maintenance and materialization policy; domain/runtime mechanism;
- Build Systems à la Carte — rebuild traces and scheduling architecture; systems/runtime mechanism;
- Chandy–Lamport / Asynchronous Barrier Snapshotting — consistent distributed snapshot and recovery protocols; systems/runtime mechanisms rather than missing scientific primitives.

These mechanisms can motivate experiments, but target-specific code must not be pushed into SciRust merely because it is useful to ElasticXxx.

---

# Rule for future additions

Before adding any capability to SciRust, ask:

> **Would this scientific tool remain generally useful if ElasticXxx and the current target project did not exist?**

If no, it belongs in the target project.

If yes, still require:

1. a concrete scientific need;
2. repository audit to confirm absence/insufficiency;
3. state-of-the-art review;
4. a clear reusable abstraction;
5. validation/benchmark plan;
6. no accidental runtime dependency from ElasticXxx back to SciRust.
