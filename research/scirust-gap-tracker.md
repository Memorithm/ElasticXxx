# SciRust Capability Gap Tracker

SciRust is treated here as a scientific R&D environment used to design, analyze, validate, and improve algorithms and models. ElasticXxx must not require SciRust as a runtime dependency.

This tracker records only **general scientific capabilities** that may be missing from SciRust and that are revealed while working on ElasticXxx. Project-specific Elastic mechanisms do not belong here.

## Status vocabulary

- **INVESTIGATE** — possible gap; not yet demonstrated.
- **CONFIRMED GAP** — current SciRust audit establishes that a required general capability is missing or insufficient.
- **EXTERNAL FIRST** — capability exists in mature external Rust/native tooling and integration should be preferred over reimplementation.
- **IMPLEMENT** — evidence justifies adding a general capability to SciRust.
- **CLOSED** — capability already exists or the research need disappeared.

## Candidate gaps

### SCIRUST-GAP-OPT-001 — Generic ILP / MILP capability

**Status:** INVESTIGATE

**Origin:** Alpa literature review.

**Need revealed:** Alpa uses integer linear programming as a specialized solver for a structured parallel-planning subproblem. Future ElasticXxx planning domains may also expose assignment, scheduling, packing, placement, routing, or selection problems naturally expressible with integer variables.

**Current evidence:** An inspection of `scirust-solvers` found continuous optimizers (including BFGS, gradient-based optimization, Nelder–Mead and SPG) and specialized combinatorial branch-and-bound functionality, but did not identify a clearly exposed general-purpose ILP/MILP solver.

**Why this is not yet confirmed:**

1. SciRust may expose related functionality elsewhere under another abstraction.
2. Mature external solver bindings/interfaces may be scientifically preferable to implementing a solver from scratch.
3. ElasticXxx has not yet demonstrated that ILP/MILP is the best solver family for any concrete planning domain.

**General usefulness independent of ElasticXxx:** scheduling, assignment, routing, packing, planning, resource allocation, experimental design and many operations-research problems.

**Next evidence required:** deeper SciRust audit plus state-of-the-art review of Rust-accessible LP/MIP solver interfaces before any implementation decision.

---

## Reviews that produced no confirmed SciRust gap

### Huber et al. (2024) — HPC Dynamic Resource Management design principles

No concrete gap established. Future work may exercise general modelling, optimization, energy/scalability-model, and decentralized-control capabilities, but a specific missing scientific primitive has not yet been demonstrated.

### Sandås et al. (2026) — Production MPI malleability / DMRv2

No concrete gap established. The paper motivates future investigation of controller stability, hysteresis, stochastic acquisition delay, and transition-cost modelling, but these needs must be mapped against existing SciRust capabilities before declaring a gap.

### Xiang et al. (2024) — NOMAD transactional page migration

No SciRust gap established. NOMAD primarily contributes an OS/runtime transition mechanism (transactional copy, validation, commit/abort, retained shadows), not a missing general scientific primitive. Its lessons belong primarily in ElasticXxx transition semantics.

### Xu et al. (2024) — FlexMem adaptive tiering

No SciRust gap established. FlexMem uses feedback, exponential moving averages, threshold adaptation, and anti-ping-pong logic. Current SciRust code search confirms EWMA functionality in `scirust-spc/src/ewma.rs` and CUSUM functionality in `scirust-spc/src/cusum.rs`. A broader online sensor-fusion or uncertainty framework should only be considered if future experiments establish a concrete need.

### Liu et al. (2025) — Tiered Memory Management Beyond Hotness

No SciRust gap established. AOL and the slowdown predictor are mathematically lightweight. Future ElasticXxx experiments may reveal a need for more general online uncertainty-aware performance modelling, but no missing general SciRust capability is demonstrated by this paper alone.

## Rule for future additions

Before adding a capability to SciRust, ask:

> Would this scientific tool remain generally useful if ElasticXxx did not exist?

If the answer is no, the mechanism belongs in ElasticXxx or another project-specific repository rather than SciRust.
