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

## Rule for future additions

Before adding a capability to SciRust, ask:

> Would this scientific tool remain generally useful if ElasticXxx did not exist?

If the answer is no, the mechanism belongs in ElasticXxx or another project-specific repository rather than SciRust.
