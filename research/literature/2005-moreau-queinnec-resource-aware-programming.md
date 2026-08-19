# Moreau & Queinnec (2005) — Resource Aware Programming

**Status:** initial mechanism review  
**Venue:** ACM Transactions on Programming Languages and Systems (TOPLAS), 27(3), 441–476, May 2005  
**DOI:** 10.1145/1065887.1065891  
**Author-accepted manuscript record:** https://eprints.soton.ac.uk/259447/

> Evidence note: this first review is deliberately limited to mechanisms verified from the authoritative publication record and manuscript text available through the University of Southampton record/searchable manuscript. A deeper line-by-line treatment of the full resource algebra and abstract-machine rules remains a separate task.

---

## 1. Problem

### SOURCE-DERIVED

Moreau and Queinnec introduce a framework intended to let users monitor resources consumed by programs and programmatically express resource-management policies. The paper frames resource management as relevant both to controlling potentially untrusted computations and to distributed/composed systems in which resources must be managed across computations.

The publication abstract explicitly states that the framework is built around hierarchical groups, asynchronous notifications for exhaustion and termination, resource descriptors governed by a resource algebra, a language-independent abstract machine, and a Java prototype.

### ELASTIC RELATION

The problem is foundationally relevant to ElasticXxx because it treats resource management as a programming-model concern rather than only as an operating-system scheduler problem.

**Classification: ADOPT as foundational prior work.**

---

## 2. Resource model

### SOURCE-DERIVED

The paper distinguishes two views:

- **resource descriptors**, programmatic entities through which programmers refer to quantities of resources;
- **concrete resource values**, which remain under control of the resource-management system / hosting environment.

The manuscript states that resources should not be created ex nihilo. Resource transfer therefore moves available resource values among computations/groups rather than allowing arbitrary fabrication.

Groups act as resource containers for the computations they sponsor. A computation consumes resources from its sponsoring group.

### ELASTIC CRITIQUE

This is a strong model for consumable or budget-like resources, but it does not by itself settle the semantics needed for all resources ElasticXxx wants to model.

Examples that motivate a broader model include:

- a CPU core, which can be occupied and released rather than permanently consumed;
- bandwidth, which behaves as a rate/capacity over time;
- VRAM residency, which is a placement state;
- parallelism degree, which is an execution configuration;
- representation/precision, which is a transformable state constrained by semantics.

This critique is an ElasticXxx design inference, not a claim made by Moreau and Queinnec.

### ELASTIC PROPOSAL

Investigate multiple resource semantic classes rather than forcing every resource into one quantity/consumption algebra.

Provisional candidates:

- Stock
- Capacity
- Rate
- Exclusive
- Shared
- State
- Representation
- Configuration

**Classification: ADAPT / INVESTIGATE.**

---

## 3. Hierarchical groups

### SOURCE-DERIVED

The framework is organized around a hierarchy of groups. Groups act as resource containers and sponsor computations. The manuscript describes group creation as transferring resources from the parent to the new group while also charging the cost of the group-creation operation.

The model therefore combines:

- resource ownership/delegation;
- computation sponsorship;
- accounting;
- hierarchy.

### ELASTIC CRITIQUE

Hierarchies remain useful for delegation, tenants, tasks, and nested scopes, but many modern heterogeneous resource relationships are not trees.

Examples:

- NUMA topology;
- shared caches;
- GPU peer links;
- PCIe topology;
- replicated state;
- distributed storage;
- resources shared by multiple execution domains.

### ELASTIC PROPOSAL

Represent the global environment as an **Elastic Resource Graph**.

Candidate relation types:

- `OWNS`
- `SHARES`
- `LOCATED_ON`
- `CONNECTED_TO`
- `DEPENDS_ON`
- `REPLICATES`
- `COMPETES_WITH`
- `CAN_MIGRATE_TO`

A hierarchy remains representable as a constrained graph shape.

**Classification: ADAPT.**

### EXPERIMENT REQUIRED

Compare a hierarchy-only planner and graph-aware planner on at least:

1. NUMA placement;
2. multi-GPU topology;
3. memory-tier migration.

Measure whether the richer topology materially improves decisions enough to justify planning complexity.

---

## 4. Resource accounting

### SOURCE-DERIVED

The manuscript explicitly states the principle that every action in a resource-management framework should be accounted for and therefore costed. The example of group creation subtracts both the transferred resource amount and the cost of creating the group from the parent.

### ELASTIC INTERPRETATION

This is highly compatible with ElasticXxx.

Elastic adaptation itself consumes resources:

- observation costs CPU/time;
- forecasting costs CPU/GPU/memory;
- planning costs compute and latency;
- migration costs bandwidth/time/energy;
- synchronization can stall work;
- verification costs additional work;
- rollback can be expensive.

A planner that ignores its own cost can adapt too often and reduce useful progress.

### ELASTIC PROPOSAL

Make adaptation cost first-class:

```text
ExpectedNetBenefit(plan)
    = ExpectedUtility(target)
    - PlanningCost
    - TransitionCost
    - VerificationCost
    - RiskPenalty
```

`NO_OP` must always be a valid candidate action.

**Classification: ADOPT / GENERALIZE.**

---

## 5. Resource exhaustion and notifications

### SOURCE-DERIVED

The publication abstract identifies asynchronous notifications for:

- resource exhaustion;
- computation termination.

Handlers may be arbitrary user code, and that code is itself executed under the hierarchical resource-control structure.

### ELASTIC CRITIQUE

Exhaustion is an important event but need not be the preferred normal trigger for adaptation.

If the runtime can detect rising pressure and estimate future demand with sufficient confidence, adaptation may occur before exhaustion.

### ELASTIC PROPOSAL

Provisional control loop:

```text
OBSERVE
   ↓
FORECAST (optional / confidence-bounded)
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

Exhaustion becomes one signal among many rather than the sole normal adaptation boundary.

**Classification: ADAPT / INVESTIGATE.**

### EXPERIMENT REQUIRED

Compare:

- exhaustion-triggered adaptation;
- fixed-threshold adaptation;
- pressure-trend adaptation;
- forecast-assisted adaptation.

Measure failure/stall avoidance, overhead, false adaptations, and useful progress.

---

## 6. User policy versus runtime planning

### SOURCE-DERIVED

The framework is explicitly designed to let users programmatically express resource-management policies. Arbitrary user code may handle resource events.

### ELASTIC CRITIQUE

Programmable policy is powerful, but it can preserve the coupling ElasticXxx wants to reduce: application code may still contain hardware/resource orchestration strategy.

### ELASTIC PROPOSAL

Separate:

```text
APPLICATION
    ↓
intent + invariants + objectives
    ↓
TRUSTED ELASTIC RUNTIME
    ↓
mechanism selection + transition planning
    ↓
resources / hardware
```

Workload-specific policy modules may still exist, but the core application should not be required to encode mechanisms such as `offload_to_host_when_vram_below_x`.

**Classification: ADAPT / INVESTIGATE.**

### OPEN QUESTION

How much workload-specific semantic information must an application expose before an automatic planner can compete with expert hand-written policies?

---

## 7. Language-independent abstract machine

### SOURCE-DERIVED

The paper describes the semantics through a language-independent abstract machine intended to model both shared- and distributed-memory environments.

### ELASTIC PROPOSAL

ElasticXxx should preserve the separation between semantic model and Rust surface syntax.

Candidate layering:

```text
Rust API / traits / proc-macros / DSL
              ↓
Elastic Intermediate Representation (EIR)
              ↓
Elastic semantic model
              ↓
Runtime adapters / platform mechanisms
```

This could later permit bindings or frontends other than Rust without redefining the resource semantics.

**Classification: ADOPT.**

---

## 8. Safety and trusted state

### SOURCE-DERIVED

The distinction between program-visible descriptors and system-controlled concrete resource values is intended to prevent arbitrary creation/manipulation of actual resources.

The Java prototype described by the paper uses mechanisms available in the Java environment of the period; the searchable manuscript text also notes limitations in interception coverage and performance for that prototype approach.

### ELASTIC PROPOSAL

Rust gives a different implementation opportunity:

```rust
pub struct ElasticHandle<R: ElasticResource> {
    id: ResourceId,
    capability: Capability<R>,
}
```

with trusted construction and transition APIs.

Potentially useful Rust mechanisms include:

- private constructors;
- ownership;
- lifetimes;
- trait bounds;
- `Send` / `Sync` constraints;
- capability types;
- typestate where practical.

**Classification: ADOPT principle / REIMPLEMENT mechanism.**

### OPEN QUESTION

Which illegal Elastic transitions can be made unrepresentable statically, and which necessarily require dynamic validation because they depend on runtime topology and resource state?

---

## 9. Reported results

### SOURCE-DERIVED

This paper is primarily a semantic/framework contribution and discusses a Java prototype. The authoritative publication record does not present the work as a performance study, and this initial review does not identify a benchmark result analogous to later systems papers such as scheduler speedups.

### REVIEW RULE

Do **not** cite Moreau & Queinnec (2005) as evidence that adaptive resource management improves performance by a particular percentage. Use it as prior work for:

- programmable resource awareness;
- hierarchical resource containers;
- resource descriptors;
- resource algebra;
- exhaustion/termination notification;
- accounting;
- language-independent semantics.

---

## 10. Elastic decision table

| Mechanism | Decision | Reason |
|---|---|---|
| Program-visible resource descriptors vs trusted resource values | **ADOPT / strengthen** | Strong capability boundary; Rust may encode it more strongly |
| Resource accounting | **ADOPT** | Adaptation cost must not be treated as free |
| Language-independent semantics | **ADOPT** | Keeps Elastic core independent from Rust syntax |
| Hierarchical groups | **ADAPT** | Useful special case, insufficient for general heterogeneous topology |
| Single resource algebra | **INVESTIGATE / generalize** | Need to test whether Stock/Capacity/Rate/State/etc. require distinct structures |
| Exhaustion notifications | **ADOPT as signal** | Important event, but should not be the whole control strategy |
| Exhaustion-centered reaction | **ADAPT** | Explore proactive pressure/forecast-driven planning |
| User-coded management mechanism | **ADAPT** | Prefer declarative intent and trusted planner while retaining extensibility |
| Java prototype mechanism | **REIMPLEMENT** | Historical implementation constraints differ substantially from Rust/runtime goals |

---

## 11. Immediate next investigation: the resource algebra

The most important unresolved part of this paper for ElasticXxx is its **resource algebra**.

Questions to answer from a full rule-by-rule reading:

1. What exact algebraic laws are required by RAP descriptors?
2. Which laws rely on resource quantities being additive/subtractive?
3. How are incomparable or structured resource values represented?
4. Can a reusable-capacity resource fit naturally into the same algebra?
5. Can a state-like property such as residency fit without artificial encoding?
6. Does Elastic require a family of algebras indexed by resource semantics?
7. Could a more general ordered/state-transition structure subsume those algebras?

This analysis should be completed before finalizing the `ElasticResourceSemantics` taxonomy.

---

## 12. Provisional research hypotheses produced by this review

### H1 — General Elastic Resource Hypothesis

Heterogeneous resources can be represented through a common programming model if resource semantics are separated from elastic dimensions, admissible states, legal transitions, and invariants.

### H2 — Declarative Adaptation Hypothesis

Application code need not encode the mechanism of adaptation when it can instead expose sufficient intent, constraints, and workload utility to a trusted planner.

### H3 — Proactive Adaptation Hypothesis

Pressure observation and bounded forecasting can trigger useful adaptation before resource exhaustion while maintaining declared invariants.

### H4 — Accounted Adaptation Hypothesis

Planning and transition overhead can be represented in the same decision model, allowing `NO_OP` when adaptation has negative expected value.

These are ElasticXxx hypotheses. Moreau & Queinnec (2005) should not be cited as having established them.

---

## Reference

Luc Moreau and Christian Queinnec. **Resource Aware Programming.** *ACM Transactions on Programming Languages and Systems*, 27(3):441–476, May 2005. DOI: 10.1145/1065887.1065891. Author-accepted manuscript record: https://eprints.soton.ac.uk/259447/
