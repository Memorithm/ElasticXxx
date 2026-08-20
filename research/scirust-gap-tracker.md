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

### SCIRUST-GAP-OPT-001 — Generic LP / ILP / MILP capability

**Status:** INVESTIGATE

**Origins:** Alpa literature review; Multivariate Amortized Resource Analysis review.

**Need revealed:**

- Alpa uses integer linear programming as a specialized solver for a structured parallel-planning subproblem.
- Multivariate amortized resource analysis uses linear programming / linear constraint solving to infer quantitative resource bounds.

Future research projects may expose scheduling, assignment, packing, placement, routing, selection, resource-bound inference, or other operations-research problems naturally expressible as LP/ILP/MILP models.

**Current evidence:** An inspection of `scirust-solvers` found continuous optimizers (including BFGS, gradient-based optimization, Nelder–Mead and SPG) and specialized combinatorial branch-and-bound functionality, but did not identify a clearly exposed general-purpose LP/ILP/MILP solver API. A search for `simplex` found `scirust-func-safety/src/simplex.rs`, but that module implements the **Simplex safety architecture** for controller fallback, not the simplex linear-programming algorithm. A direct repository search for `linprog` returned no result.

**Why this is not yet confirmed:**

1. SciRust may expose related functionality elsewhere under another abstraction.
2. Mature external solver bindings/interfaces may be scientifically preferable to implementing a full solver stack from scratch.
3. ElasticXxx has not yet demonstrated that LP/ILP/MILP is the best solver family for any concrete runtime planning domain.
4. The desired boundary may be a generic modelling/interface layer rather than an in-house optimizer implementation.

**General usefulness independent of ElasticXxx:** scheduling, assignment, routing, packing, planning, resource allocation, experimental design, static resource-bound inference and many operations-research problems.

**Next evidence required:** deeper SciRust audit plus state-of-the-art review of Rust-accessible LP/MIP modelling and solver interfaces before any implementation decision.

---

### SCIRUST-GAP-CONTROL-001 — Advanced nonlinear / explicit / multi-rate predictive control

**Status:** INVESTIGATE

**Origin:** Mandal et al. (2020) predictive and learned runtime-resource-control review, including the explicit-NMPC GPU mechanism it summarizes.

**Need revealed:** several runtime resource-management problems combine nonlinear dynamics, multiple control inputs, hard constraints, different actuation timescales, online-adapted system/sensitivity models, and a requirement for a very low-overhead deployed controller.

**Existing SciRust capabilities verified:**

- `scirust-control/src/mpc.rs` provides a condensed finite-horizon **linear MPC** for `x_{k+1}=Ax_k+Bu_k`, quadratic cost and hard box input constraints solved by box QP;
- `scirust-estimation/src/rls.rs` provides deterministic multi-channel **recursive least squares** with forgetting factor and zero heap allocations in the update hot loop.

Therefore neither generic MPC nor online RLS should be described as missing.

**Capability not identified in the current search:** a clearly exposed, general implementation of one or more of:

- nonlinear MPC (NMPC);
- explicit MPC / explicit-NMPC control-law generation or approximation;
- generic multi-rate predictive control;
- a reusable integration layer between adaptive sensitivity/system-identification models and constrained MPC.

**Why this is not yet confirmed:**

1. related functionality may exist elsewhere in SciRust under another name;
2. the capabilities may belong as separate modules rather than one monolithic feature;
3. mature external nonlinear/QP/NLP solver integrations may be preferable to reimplementation;
4. ElasticXxx has not yet produced a concrete dynamics model requiring NMPC rather than simpler MPC/heuristics.

**General usefulness independent of ElasticXxx:** robotics, process control, energy systems, thermal management, embedded systems, autonomous systems, vehicle control and industrial optimization.

**Next evidence required:** deeper SciRust control/solver audit, survey of Rust-accessible nonlinear optimization/MPC tooling, and at least one concrete non-Elastic scientific use case before moving beyond INVESTIGATE.

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

### Hoffmann, Aehlig & Hofmann (2011) — Multivariate amortized resource analysis

No separate gap created. The paper strengthens `SCIRUST-GAP-OPT-001` by showing a general scientific use for LP beyond runtime scheduling: automatic inference of static quantitative resource bounds.

### Das, Hoffmann & Pfenning (2018) — Resource-aware session types

No SciRust gap established. The contribution is primarily type-system and protocol semantics; its linear-resource constraints do not by themselves imply a new missing mathematical primitive beyond the existing LP/MIP investigation.

### Weiss et al. (2021) — Oxide / Rust ownership semantics

No SciRust gap established. Oxide contributes programming-language semantics for ownership and borrowing rather than scientific-computing functionality.

## Rule for future additions

Before adding a capability to SciRust, ask:

> Would this scientific tool remain generally useful if ElasticXxx did not exist?

If the answer is no, the mechanism belongs in ElasticXxx or another project-specific repository rather than SciRust.
