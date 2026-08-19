# Elastic Resources: Toward a General, Type-Safe Programming Model for Adaptive Resource Management

**Research White Paper — v0.1**  
**Status:** working document; hypotheses and terminology are provisional unless explicitly identified as established prior work.

## Abstract

Modern software increasingly executes over heterogeneous and time-varying resource environments. Memory may span host RAM, accelerator memory, persistent storage, and remote nodes. Computation may execute across CPUs, GPUs, specialized accelerators, and distributed workers. Parallelism, batching, placement, representation, bandwidth allocation, replication, persistence, and execution topology may each admit multiple valid configurations during one execution.

Existing systems have already demonstrated important pieces of resource-aware and adaptive computing. A foundational example is Moreau and Queinnec's *Resource Aware Programming* (ACM TOPLAS, 2005), which introduced a language-independent framework in which programs can monitor resource use and express resource-management policies through hierarchical groups, resource descriptors, a resource algebra, and asynchronous notifications for resource exhaustion and computation termination [1].

ElasticXxx investigates a broader research hypothesis: **resource elasticity may be expressible as a general programming abstraction rather than as a collection of resource-specific runtime mechanisms**.

The emerging Elastic model treats a resource not merely as a consumable quantity but as an entity with resource semantics, an admissible state space, a set of elastic dimensions, legal transitions, invariants, measurable effects, and transition costs. An application declares what must remain true and what it wishes to optimize. A trusted runtime may then observe the environment, forecast when appropriate, construct candidate plans, validate transitions, execute them, verify their effects, and commit or roll back.

Rust is the initial host language because its ownership, lifetimes, traits, and type system provide a promising substrate for expressing capabilities and preventing illegal resource manipulation. SLHAv2 is the initial incubation environment, but the core model is intended to remain independent of LLM-specific workloads and any single accelerator stack.

This document defines the initial problem statement, design principles, provisional formal model, relationship to prior work, research hypotheses, and evaluation agenda.

---

## 1. Motivation

### 1.1 The static-resource assumption is increasingly fragile

Application code frequently embeds decisions such as:

- number of worker threads;
- CPU/GPU placement;
- memory tier;
- buffer capacity;
- batch size;
- queue depth;
- degree and form of parallelism;
- replication factor;
- prefetch depth;
- offload policy;
- representation or precision;
- checkpoint frequency;
- routing across devices or nodes.

These decisions are often reasonable for one hardware topology and poor for another. Even on the same machine, contention, memory pressure, I/O load, thermal state, external quotas, and concurrent workloads can change during execution.

The central motivation of ElasticXxx is therefore to separate:

```text
WHAT MUST REMAIN TRUE
        from
HOW THE CURRENT MACHINE SATISFIES IT
```

### 1.2 Resource orchestration leaks into application logic

A conventional program may contain policy such as:

```rust
if available_vram() < required {
    offload_to_host();
}
```

This directly couples application logic to a particular resource and mechanism.

The Elastic direction is instead conceptually closer to:

```rust
preserve(SemanticContract::Exact);
optimize(Objective::Latency);
```

The exact API is not yet fixed. The key principle is that **the application declares constraints and objectives while the runtime selects only among explicitly admissible mechanisms**.

### 1.3 Adaptation must not mean semantic drift

Elasticity is not permission for a runtime to silently weaken program semantics.

The following are constraints on elasticity rather than elastic properties themselves:

- memory safety;
- type safety;
- authorization and capability boundaries;
- cryptographic guarantees;
- transactional invariants;
- protocol correctness;
- declared numerical or semantic guarantees.

For example, a conversion from FP64 to FP16 is not a transparent optimization when an exact or bounded-error contract forbids it.

---

## 2. Design Principles

### 2.1 Intent over mechanism

Applications should express invariants, constraints, and objectives rather than hard-code resource-management strategy whenever a safe runtime alternative exists.

### 2.2 Explicit admissibility

A runtime may change only dimensions that have been declared elastic and only through legal transitions.

### 2.3 No silent semantic degradation

Semantic changes require explicit authorization through a contract.

### 2.4 Observable and explainable decisions

Every significant runtime adaptation should be attributable to observations, constraints, objectives, and a predicted or measured cost/benefit.

### 2.5 Adaptation has a cost

Observation, planning, migration, synchronization, verification, and rollback consume resources. The system must be able to conclude that doing nothing is preferable.

### 2.6 Reversibility where possible

Transitions should expose whether they are reversible. The runtime should support transactional application and rollback where the underlying mechanism allows it.

### 2.7 Reproducibility

Adaptive execution must remain debuggable. The project therefore investigates deterministic decision recording and replay.

### 2.8 Hardware independence at the core

`elastic-core` should not depend conceptually on CUDA, LLMs, a particular GPU family, or one operating system. Hardware-specific mechanisms belong behind capabilities and adapters.

---

## 3. Foundation: Resource Aware Programming

Moreau and Queinnec define the Resource Aware Programming framework as a way for users to monitor resources used by their programs and programmatically express management policies [1]. The paper's abstract identifies four major elements:

1. **hierarchical groups** acting as resource containers for computations;
2. **asynchronous notifications** for resource exhaustion and computation termination;
3. **resource descriptors** manipulated by the programmer;
4. operations specified by a **resource algebra**.

The authors also provide a language-independent abstract-machine semantics intended to model both shared-memory and distributed-memory environments, and they discuss a Java prototype [1].

ElasticXxx treats this work as a foundational predecessor, not as something to be rediscovered under new terminology.

### 3.1 Mechanisms provisionally adopted

#### Separation between program-visible description and trusted resource state

The Resource Aware Programming framework distinguishes program-manipulable descriptors from system-controlled resource values. ElasticXxx adopts the underlying principle: application code should not be able to fabricate physical capabilities merely by constructing an ordinary value.

A Rust implementation may strengthen this through private constructors, typed capabilities, ownership, and lifetime constraints.

**Classification: ADOPT / STRENGTHEN.**

#### Resource accounting

Management logic itself consumes resources. ElasticXxx retains the principle that runtime overhead must be measurable rather than treated as free.

**Classification: ADOPT.**

#### Language-independent semantic core

Moreau and Queinnec explicitly describe a language-independent abstract machine [1]. ElasticXxx likewise aims to separate its semantic model from its Rust syntax so that a future intermediate representation or bindings can exist independently of the initial embedding.

**Classification: ADOPT.**

### 3.2 Mechanisms provisionally adapted

#### Hierarchical groups → resource graph

Hierarchical resource groups naturally model nested delegation. Modern heterogeneous systems also contain relationships that are not trees: NUMA topology, shared caches, PCIe, NVLink-class links, replicated data, distributed storage, and peer devices.

ElasticXxx therefore investigates an **Elastic Resource Graph** whose nodes are resources or resource-bearing entities and whose edges represent relations such as:

- owns;
- shares;
- located-on;
- connected-to;
- depends-on;
- replicates;
- competes-with;
- can-migrate-to.

A hierarchy becomes one graph shape rather than the universal shape.

**Classification: ADAPT.**

#### Exhaustion-centered reaction → proactive adaptation

Resource Aware Programming explicitly includes notifications for resource exhaustion and computation termination [1]. ElasticXxx retains these as observable events but investigates a proactive loop intended, when sufficiently reliable, to adapt *before* exhaustion:

```text
OBSERVE
   ↓
FORECAST
   ↓
PLAN
   ↓
VALIDATE
   ↓
ACT
   ↓
VERIFY
   ↓
COMMIT / ROLLBACK
```

Forecasting is optional rather than authoritative: when prediction is unreliable, the runtime must fall back to observed state and conservative policies.

**Classification: ADAPT / INVESTIGATE.**

#### User-coded management policy → declarative intent plus planner

Resource Aware Programming allows programmers to express management policies programmatically [1]. ElasticXxx investigates a stronger separation: application code declares admissible behavior and objectives, while a planner chooses the mechanism.

The project must experimentally determine how much workload-specific information is required before such a planner remains competitive with hand-written resource policy.

**Classification: ADAPT / INVESTIGATE.**

---

## 4. A Broader Resource Semantics

A single quantity-and-consumption abstraction is insufficient for every resource ElasticXxx intends to model.

The project therefore proposes a provisional taxonomy. This taxonomy is **an ElasticXxx research proposal**, not a claim derived from Moreau and Queinnec.

### 4.1 Stock

A quantity that can be consumed or depleted.

Examples may include:

- credits;
- energy budgets;
- quotas.

### 4.2 Capacity

A finite capacity that can be occupied and released.

Examples:

- RAM;
- VRAM;
- slots;
- queue storage.

### 4.3 Rate

A capacity expressed over time.

Examples:

- memory bandwidth;
- network throughput;
- IOPS budget.

### 4.4 Exclusive resource

A resource temporarily owned by one execution domain.

Examples:

- an exclusively claimed accelerator;
- a dedicated core.

### 4.5 Shared resource

A resource whose effective availability depends on concurrent consumers.

Examples:

- CPU capacity;
- shared network link;
- shared cache or storage bandwidth.

### 4.6 State resource

A property whose value describes placement or execution state.

Examples:

- residency;
- locality;
- device placement.

### 4.7 Representation resource

A physical representation that may be transformed while preserving an authorized semantic contract.

Examples:

- compression;
- tensor layout;
- numerical representation.

### 4.8 Configuration resource

A structural execution parameter.

Examples:

- worker count;
- batch size;
- queue depth;
- degree of parallelism.

A major research question is whether these are genuinely distinct resource semantics, whether some collapse into a common algebra, or whether a different taxonomy is required.

---

## 5. Provisional Elastic Resource Model

We provisionally define a resource as:

\[
R = (K,S,D,T,I,M)
\]

where:

- \(K\) — resource semantics, kind, and capabilities;
- \(S\) — admissible state space;
- \(D\) — dimensions that may adapt;
- \(T\) — legal transitions;
- \(I\) — invariants that must be preserved;
- \(M\) — observation and cost model.

This notation is provisional and will be revised against prior formal models.

### 5.1 Elastic dimensions

Candidate dimensions currently include:

- capacity;
- concurrency;
- residency;
- locality;
- representation;
- precision;
- parallelism;
- routing;
- priority;
- redundancy;
- persistence;
- recomputability;
- bandwidth allocation;
- latency budget;
- energy;
- reliability.

Not every resource supports every dimension.

### 5.2 Elastic Space

For a resource \(R\), let \(\mathcal{E}(R)\) denote the set of states permitted by its capabilities and invariants.

A legal adaptation is a transition:

\[
s_i \rightarrow s_j
\]

such that:

\[
s_i,s_j \in \mathcal{E}(R)
\]

and the transition belongs to the legal transition relation \(T\).

The model separates logical identity from physical realization. A logical object may therefore remain the same resource while changing residence, representation, replication, or other explicitly elastic dimensions.

### 5.3 Elastic Plan

An Elastic Plan is a sequence of candidate transitions:

\[
P=(t_1,\ldots,t_n)
\]

that maps a current state to an admissible target state while preserving invariants.

A generic planning objective may be written provisionally as:

\[
\max_P \; U(s_{target}) - Cost(P)
\]

subject to:

\[
\forall i \in I,\; i(s_{target}) = true
\]

where `U` is workload- and policy-dependent utility and `Cost(P)` includes the cost of planning and executing the adaptation.

No claim is made yet that this formulation is novel or sufficient. It is a working model to be compared against optimization, control, scheduling, and resource-aware programming literature.

---

## 6. Runtime Architecture

The provisional runtime pipeline is:

### Observe

Collect current resource state and measurements.

### Forecast

Estimate future demand or pressure only when the prediction mechanism has a usable confidence model.

### Plan

Generate one or more candidate target states or transition sequences.

### Validate

Reject transitions that violate capabilities, invariants, physical limits, permissions, or semantic contracts.

### Act

Execute the selected transitions.

### Verify

Measure postconditions and compare actual effects with the plan.

### Commit / Rollback

Accept the new state, or restore a prior state when the transition is reversible and verification fails.

The planner is not assumed to require one universal algorithm. Different subspaces may use heuristics, mathematical optimization, control theory, online learning, dynamic programming, or specialized planners. The architecture should permit these strategies without exposing them as application policy.

---

## 7. Rust as the Initial Host Language

Rust is not merely an implementation language for ElasticXxx; it is part of the research hypothesis.

Rust already reasons statically about:

- ownership;
- aliasing;
- mutability;
- lifetimes;
- thread-safety traits;
- destruction.

ElasticXxx asks whether part of resource adaptation can be expressed as an extension of this discipline:

```text
Rust:
WHO owns it?
WHEN may it be accessed?

Elastic:
WHERE may it reside?
WHICH properties may change?
WHICH transitions are legal?
WHAT invariants must remain true?
```

The initial implementation path should avoid premature compiler modification:

1. ordinary Rust traits and types;
2. procedural macros and attributes;
3. a declarative `elastic!` DSL if justified;
4. an Elastic Intermediate Representation (EIR);
5. only after evidence, investigation of compiler or language integration.

---

## 8. Initial Research Hypotheses

### H1 — General Elastic Resource Hypothesis

Heterogeneous computational resources that differ in physical nature can be represented through a common programming model by separating resource semantics, elastic dimensions, admissible states, legal transitions, and semantic invariants.

### H2 — Declarative Adaptation Hypothesis

Useful runtime adaptation can be expressed without requiring application code to encode the mechanism that performs the adaptation.

### H3 — Proactive Adaptation Hypothesis

Observation and bounded forecasting can enable adaptation before exhaustion or severe contention while preserving declared invariants.

### H4 — Accounted Adaptation Hypothesis

The cost of observation, planning, migration, synchronization, verification, and rollback can itself be represented, allowing the runtime to reject adaptations whose expected benefit does not justify their cost.

### H5 — Type-Safe Elasticity Hypothesis

Rust's ownership and type mechanisms can constrain capability access and some classes of resource transition such that illegal manipulations are prevented statically or at a narrow trusted-runtime boundary.

### H6 — Reproducible Adaptation Hypothesis

Recording the observations and decisions that drive adaptation can make adaptive execution sufficiently reproducible for debugging, benchmarking, and scientific workflows.

---

## 9. Initial Evaluation Questions

The research program must eventually answer at least the following experimentally:

1. Can one common core represent memory, compute, storage, networking, concurrency, and accelerator resources without resource-specific hacks in the planner interface?
2. How much resource-management code can be removed from applications?
3. What runtime overhead does Elastic introduce when no adaptation is necessary?
4. Can the planner match or exceed hand-written resource strategies on representative workloads?
5. Does proactive adaptation avoid measurable stalls or failures better than threshold-only reaction?
6. Can exact semantic contracts be maintained across migrations, recomputation, sharding, and other transformations?
7. Can bounded-approximation contracts be verified efficiently?
8. Does deterministic replay reproduce enough of an adaptive execution to support debugging and research reproducibility?
9. How should planning be decomposed when the full Elastic Space is combinatorial?
10. At what point does automatic planning become too expensive relative to the expected gain?

---

## 10. Literature Review Method

Every relevant prior system will be studied at the mechanism level, not merely cited by title.

For each work we record:

- problem;
- resource model;
- observations;
- decision variables;
- objective;
- constraints;
- planning/control algorithm;
- transition model;
- adaptation granularity;
- cost accounting;
- safety mechanism;
- reversibility;
- reported experimental results;
- acknowledged limitations;
- relationship to ElasticXxx.

Mechanisms are classified as:

- **ADOPT** — retain substantially unchanged;
- **ADAPT** — retain the principle while generalizing or altering the mechanism;
- **REJECT** — incompatible or unnecessary;
- **INVESTIGATE** — promising but requiring formal or experimental comparison.

Scientific novelty will only be claimed after this review is sufficiently broad and the corresponding Elastic contribution is implemented and evaluated.

---

## 11. Immediate Research Agenda

1. Analyze the resource algebra of Moreau & Queinnec in detail.
2. Determine whether Elastic requires multiple resource algebras or a more general common construction.
3. Review Invasive Computing and compare explicit resource claiming with declarative Elastic intent.
4. Review dynamic/malleable HPC resource models.
5. Review heterogeneous-memory runtimes.
6. Review automatic parallelization and planner decomposition.
7. Review online control, model-predictive control, and learned resource models.
8. Review resource-aware type systems.
9. Build the first Resource Atlas and Elastic Dimension taxonomy.
10. Define the minimal trusted runtime boundary for Rust.

---

## 12. Scope and Non-Claims

ElasticXxx is presently a research project in an early specification phase.

This document does **not** claim that:

- resource-aware programming was invented by ElasticXxx;
- dynamic resource allocation is novel;
- proactive resource control is novel;
- automatic planning is novel;
- the proposed taxonomy is complete;
- the provisional formalism is sufficient;
- ElasticXxx currently outperforms specialized runtimes;
- ElasticXxx currently warrants integration into the Rust language or standard library.

These are questions to be investigated, not conclusions to be assumed.

---

## References

[1] Luc Moreau and Christian Queinnec. **Resource Aware Programming.** *ACM Transactions on Programming Languages and Systems*, 27(3):441–476, May 2005. DOI: 10.1145/1065887.1065891. Author-accepted manuscript and bibliographic record: https://eprints.soton.ac.uk/259447/
