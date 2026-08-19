# Alpa: Automating Inter- and Intra-Operator Parallelism for Distributed Deep Learning

**Paper:** Lianmin Zheng, Zhuohan Li, Hao Zhang, Yonghao Zhuang, Zhifeng Chen, Yanping Huang, Yida Wang, Yuanzhong Xu, Danyang Zhuo, Eric P. Xing, Joseph E. Gonzalez, Ion Stoica. *Alpa: Automating Inter- and Intra-Operator Parallelism for Distributed Deep Learning*. OSDI 2022, pp. 559–578.

**Primary source:** USENIX OSDI 2022 paper: https://www.usenix.org/system/files/osdi22-zheng-lianmin.pdf

**Review status:** initial mechanism-level review complete. Claims below distinguish source-derived results from ElasticXxx proposals and open questions.

---

## 1. Problem

**SOURCE-DERIVED.** Alpa addresses automatic model-parallel training of large deep-learning models. The authors identify the central difficulty as a combinatorial execution-plan space involving data, operator, and pipeline parallelism, model partitioning, stage construction, device placement, tensor sharding, and device-mesh assignment.

The paper's central observation is that these parallelization techniques can be reorganized into two hierarchical levels:

- **intra-operator parallelism** — partition operators along tensor axes and execute partitions across devices;
- **inter-operator parallelism** — divide the computation graph into stages and pipeline stages across different sets of devices.

The hierarchy is matched to hardware topology: intra-op communication is mapped preferentially to high-bandwidth local device groups, while inter-op communication can cross slower links.

---

## 2. Resource model

**SOURCE-DERIVED.** Alpa's resource model is specialized to distributed DL execution. The principal physical resources are compute devices organized into a cluster and partitioned into **device meshes**. Important resource properties include:

- number of devices;
- mesh shape;
- local versus cross-node communication bandwidth;
- device memory capacity;
- mapping of pipeline stages to meshes.

The computational objects being mapped onto those resources are operators, tensors, stages, sharding specifications, and microbatches.

**INFERENCE.** Alpa does not present a general-purpose resource abstraction comparable to the emerging ElasticXxx model. Instead, it constructs a domain-specific execution-plan space whose resource semantics are encoded in compiler passes, mesh abstractions, and DL-specific cost functions.

---

## 3. Observability and cost information

**SOURCE-DERIVED.** At the intra-op level, Alpa models total graph cost as the sum of:

- compute cost;
- communication cost of a selected operator strategy;
- resharding cost between producer and consumer strategies.

For the ILP formulation used in the paper, communication and resharding costs are estimated from communicated bytes divided by mesh bandwidth. Compute costs are set to zero under the paper's stated assumptions about the compared parallel algorithms.

At the inter-op level, the optimizer repeatedly invokes the intra-op pass for candidate stage–mesh pairs. The resulting plan is compiled and profiled to obtain stage execution latency and memory requirements. The paper also uses a simple XLA-instruction-level, piecewise-linear cost model to accelerate profiling during compilation.

**ELASTIC RELATION.** This strongly supports separating:

1. the admissible decision space;
2. the optimization algorithm;
3. the cost model used to rank candidates;
4. measurements used to validate or refine those estimates.

That separation is directly relevant to ElasticXxx, but Alpa's specific cost models are not general enough to serve as an Elastic resource model.

---

## 4. Decision variables

**SOURCE-DERIVED.** Alpa can decide, among other things:

- operator sharding strategy;
- tensor layouts;
- resharding operations;
- grouping of operators into pipeline stages;
- number and shapes of device meshes;
- mapping of stages to meshes;
- intra-op plan within each stage–mesh pair.

The number of microbatches is a hyperparameter in the paper's inter-op formulation and is not jointly optimized there.

---

## 5. Objective

**SOURCE-DERIVED.** The intra-op pass minimizes execution cost for a stage on a given device mesh. The inter-op pass minimizes end-to-end pipeline latency. For a pipeline with stage latencies `t_i` and `B` microbatches, the formulation combines the sum of stage latencies with the repeated cost of the slowest stage.

Memory capacity is treated as a feasibility constraint when profiling stage–mesh candidates.

**ELASTIC RELATION.** Alpa cleanly demonstrates the difference between:

- an optimization objective, such as execution latency;
- feasibility constraints, such as fitting in device memory.

ElasticXxx should preserve an even stronger distinction between semantic invariants, physical constraints, policy constraints, and optimization objectives.

---

## 6. Planner decomposition

### 6.1 Intra-operator planner

**SOURCE-DERIVED.** Alpa formulates intra-op strategy selection as an **integer linear program (ILP)**. Each operator has a set of possible parallel strategies. The decision variables select one strategy per operator, while additional variables linearize resharding decisions across graph edges. The paper reports solving this formulation optimally with an off-the-shelf ILP solver.

The graph is simplified by merging computationally trivial operators in order to reduce ILP size.

### 6.2 Inter-operator planner

**SOURCE-DERIVED.** Alpa formulates stage construction and mesh assignment using **dynamic programming (DP)**. The DP queries the intra-op optimizer for the cost of candidate stage–mesh pairs. The paper introduces reductions in the allowed mesh shapes, early pruning, and operator clustering to keep this search tractable.

### 6.3 Joint optimality

**SOURCE-DERIVED.** The authors explicitly state that each hierarchical level is solved near-optimally as a tractable subproblem, but the resulting joint plan is **not guaranteed globally optimal**.

**ELASTIC DECISION: ADOPT / GENERALIZE.** The important mechanism is not "use ILP" or "use DP". The important mechanism is:

> **factor a combinatorial planning problem into subproblems whose mathematical structure admits different specialized solvers, then compose their results.**

ElasticXxx should investigate this as a general planner architecture.

---

## 7. Planning granularity versus execution granularity

**SOURCE-DERIVED.** Alpa reasons at several granularities:

- individual operator sharding decisions;
- grouped pipeline stages;
- logical device meshes;
- physical device mappings;
- static runtime instructions on each mesh.

This reinforces the lesson from Invasive Computing that allocation, planning, and execution need not share one granularity.

**ELASTIC PROPOSAL.** ElasticXxx should explicitly model at least the possibility of distinct:

- semantic granularity;
- planning granularity;
- allocation granularity;
- transition granularity;
- execution granularity.

Whether these become first-class API concepts remains an open design question.

---

## 8. Transition model

**SOURCE-DERIVED.** Alpa is primarily a compiler-generated execution-planning system rather than a continuously adaptive runtime. After the plan is produced, each stage is compiled into an executable for its mesh and Alpa generates static instructions for memory allocation/deallocation, communication, synchronization, and computation. These instruction lists are dispatched to workers before execution to avoid coordination overhead during runtime.

The paper does not provide a general runtime transition algebra for changing from one already-running global execution plan to another.

**ELASTIC RELATION: ADAPT.** ElasticXxx can reuse the idea of generating a structured execution plan, but its intended scope requires transitions between admissible runtime states, including validation, execution, verification, and potentially rollback or compensation.

---

## 9. Static versus adaptive planning

**SOURCE-DERIVED.** The limitations section states that Alpa:

- uses a static linear pipeline schedule;
- does not optimize more dynamic schedules such as executing different graph branches on different devices;
- does not optimize the best overlap of computation and communication;
- handles static computational graphs with tensor shapes known at compilation time;
- does not include cross-stage communication cost in the main optimization because doing so would enlarge the state/search space substantially;
- leaves the microbatch count outside the main optimization formulation.

**ELASTIC PROPOSAL.** ElasticXxx should not assume that the full global plan must be recomputed continuously. A more plausible architecture is hybrid:

1. stable portions of the resource plan may be compiled/cached;
2. local or incremental decisions may use a fast path;
3. a larger planner is invoked only when the current admissible region changes materially;
4. replanning can be triggered by events, pressure, prediction, topology change, or invalidated assumptions.

This is a hypothesis for ElasticXxx, not a result established by Alpa.

---

## 10. Planning cost

**SOURCE-DERIVED.** Planning is not cheap. For the GPT-39B / 64-GPU case reported in Table 5, Alpa reports approximately:

- compilation: 1582.66 s;
- profiling: 804.48 s;
- stage-construction DP: 1.65 s;
- other: 4.47 s;
- total: 2393.26 s.

Without the described compilation/profiling optimizations, the paper reports more than 40 hours total. The authors argue that several hours of planning is acceptable because the target training workloads can last several weeks.

**KEY ELASTIC LESSON.** The optimizer's algorithmic complexity is not necessarily the dominant planning cost. Candidate generation, compilation, profiling, measurement, and evaluation can dominate.

This strengthens the ElasticXxx principle:

> **adaptation and planning are resources that must themselves be accounted for.**

It also suggests that Elastic planning should make the cost of obtaining better information explicit, rather than modeling only the eventual transition cost.

---

## 11. Results

**SOURCE-DERIVED.** The paper evaluates Alpa on an 8-node / 64-GPU AWS cluster. Reported results include:

- GPT plans that match or slightly outperform the specialized Megatron-LM configurations in several settings;
- on GShard MoE, 3.5× speedup over DeepSpeed on 2 nodes and 9.7× on 4 nodes;
- approximately 80% scaling efficiency on Wide-ResNet at 32 GPUs, for a model family without a manually designed specialized plan;
- in the inter-op ablation on Wide-ResNet / 32 GPUs, the full DP outperforms the "equal operator" and "equal layer" alternatives by 2.6× and 1.6× respectively;
- a local-all-gather cross-mesh optimization gives a reported 2.0× speedup on 32 GPUs in the corresponding resharding experiment.

These results establish that structured automatic planning can compete with or exceed strong manually tuned domain-specific plans in the evaluated settings. They do **not** establish that hierarchical decomposition is universally optimal for arbitrary resource-management problems.

---

## 12. Safety and correctness

**SOURCE-DERIVED.** Alpa states that it does not change the semantics of synchronous gradient descent, so its evaluation focuses on system throughput rather than convergence differences. Memory feasibility is enforced while evaluating candidate stage–mesh combinations.

**INFERENCE.** The safety model is domain-specific and largely compiler/runtime based. The paper does not provide a general semantic-contract system analogous to what ElasticXxx is investigating.

**ELASTIC RELATION: ADAPT.** ElasticXxx should treat a planner's feasible search space as derived from explicit invariants and legal transitions. Optimization must operate *inside* the admissible space rather than repair semantic violations after selection.

---

## 13. The strongest lesson for ElasticXxx

Alpa changes the planning question from:

```text
search the entire combinatorial space
```

to:

```text
identify structure in the space
    ↓
decompose it into planning domains
    ↓
choose a solver appropriate to each domain
    ↓
compose the local results
```

**ELASTIC HYPOTHESIS.** ElasticXxx should investigate whether an `ElasticSpace` can expose enough structure to permit this decomposition automatically or semi-automatically.

A possible future abstraction is a **planning-domain graph** rather than a fixed planner hierarchy. For example:

```text
VRAM residency ───────┐
                      ├─ memory-pressure planning domain
recomputation ────────┘

network topology ─────┐
                      ├─ placement/parallelism planning domain
parallelism ──────────┘

queue pressure ───────┐
                      ├─ concurrency planning domain
worker count ─────────┘
```

Different domains could use different solvers and exchange boundary costs or constraints.

This is an **ELASTIC PROPOSAL / OPEN QUESTION**, not a claim of novelty.

---

## 14. Potential difference from Alpa: pressure-dependent decomposition

**ELASTIC HYPOTHESIS.** A fixed decomposition may not be appropriate for a general adaptive runtime. The relevant coupling can change with current pressure:

- under VRAM pressure, residency, recomputation, representation, and batch size may become tightly coupled;
- under network pressure, locality, routing, sharding, and replication may dominate;
- under thermal or power pressure, compute placement, parallelism, frequency, and batching may become coupled.

ElasticXxx should therefore investigate whether the **decomposition of the optimization problem can itself be selected or adapted** according to the active constraints and pressure domains.

No reviewed paper so far establishes that this mechanism is novel or superior. It requires broader literature review and experiments.

---

## 15. Relationship to the emerging Elastic model

| Alpa mechanism | ElasticXxx disposition |
|---|---|
| Hierarchical plan space | **ADOPT principle / ADAPT structure** |
| Separate subproblem solvers | **ADOPT / GENERALIZE** |
| ILP for intra-op strategy | **INVESTIGATE as planner backend** |
| DP for inter-op planning | **INVESTIGATE as planner backend** |
| Cost-query composition between levels | **ADOPT / GENERALIZE** |
| Graph simplification before search | **ADOPT / GENERALIZE** |
| Early pruning | **ADOPT** |
| Hardware-topology-aware planning | **ADOPT / Resource Graph** |
| Explicit memory feasibility | **ADOPT / GENERALIZE** |
| Static compile-time planning | **ADAPT toward hybrid runtime planning** |
| Static known tensor shapes | **REJECT as a general Elastic assumption** |
| DL-specific cost model | **ADAPT** |
| Fixed two-level hierarchy | **ADAPT / INVESTIGATE planning-domain graph** |
| Globally optimal final plan | **Not claimed by Alpa; do not claim for Elastic without proof** |

---

## 16. Implications for the Elastic planner architecture

**ELASTIC PROPOSAL.** The current working model should distinguish at least:

```text
ElasticSpace
    ↓
Domain decomposition
    ↓
Candidate subproblems
    ↓
Specialized planner backend per subproblem
    ↓
Composition / reconciliation
    ↓
Validated ElasticPlan
```

A planner backend might be:

- deterministic heuristic;
- dynamic programming;
- integer or mixed-integer optimization;
- graph algorithm;
- model-predictive control;
- evolutionary optimization;
- other specialized solver.

The resource semantics must remain independent of the chosen optimization backend.

---

## 17. SciRust gap check

SciRust is used as a scientific R&D environment, never as a required ElasticXxx dependency.

### Observed current capability

The current `scirust-solvers` tree contains continuous optimizers such as BFGS, gradient-based optimization, Nelder–Mead, and SPG, plus a dedicated combinatorial module containing a certified branch-and-bound clustering solver. This demonstrates useful optimization infrastructure but is not evidence of a generic ILP/MILP solver.

### Candidate gap

**SCIRUST-GAP-OPT — INVESTIGATE:** generic integer / mixed-integer linear optimization.

The repository search performed during this review did not reveal a clearly exposed general-purpose ILP/MILP solver. This is **not yet classified as a confirmed gap** because:

1. a deeper SciRust audit may uncover related functionality under another abstraction;
2. an external Rust solver interface may be preferable to implementing a full solver;
3. ElasticXxx has not yet demonstrated that ILP/MILP is the best method for its own planning domains.

The capability would nevertheless be scientifically general — scheduling, assignment, routing, packing, resource allocation, and many other problems can be formulated as integer programs — so it is a legitimate SciRust research question independent of ElasticXxx.

---

## 18. Experiments suggested for ElasticXxx

**EXPERIMENT REQUIRED.** Once a prototype `ElasticSpace` exists, compare on the same benchmark:

1. monolithic global search;
2. fixed hierarchical decomposition;
3. manually selected specialized subplanners;
4. pressure-dependent decomposition;
5. incremental replanning using a previous plan as a warm start.

Measure at least:

- solution quality / useful progress;
- planner CPU time;
- planner memory;
- number of candidate states evaluated;
- amount of profiling/measurement required;
- transition count and transition cost;
- invariant violations (must remain zero in valid designs);
- sensitivity to prediction/cost-model error.

---

## 19. Current conclusion

Alpa provides strong evidence that **the structure of the search space is itself a systems optimization opportunity**. Its most important contribution to ElasticXxx is not a particular parallelism technique, but the demonstration that a massive plan space can become tractable by exposing hierarchy, simplifying the graph, pruning choices, and assigning different mathematical solvers to different levels.

ElasticXxx should adopt this principle while investigating a more general formulation in which planning domains are derived from resource semantics, topology, coupling, and active constraints rather than fixed to DL intra-/inter-operator parallelism.
