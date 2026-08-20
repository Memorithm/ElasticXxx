# BWoS: Formally Verified Block-based Work Stealing for Parallel Processing

**Paper:** Jiawei Wang, Bohdan Trach, Ming Fu, Diogo Behrens, Jonathan Schwender, Yutao Liu, Jitang Lei, Viktor Vafeiadis, Hermann Härtig, Haibo Chen. *BWoS: Formally Verified Block-based Work Stealing for Parallel Processing*. OSDI 2023.

**Primary sources:**

- https://www.usenix.org/conference/osdi23/presentation/wang-jiawei
- https://wangjwchn.github.io/papers/OSDI2023.pdf

**Review status:** mechanism-level review complete for queue structure, synchronization, weak-memory correctness and implications for fine-grained Elastic scheduling.

---

## 1. Problem

**SOURCE-DERIVED.** Work stealing reduces idle time by maintaining per-core queues and allowing idle cores to steal tasks from other queues. BWoS observes that modern workloads can contain extremely small tasks, making the work-stealing queue itself a significant runtime bottleneck.

The authors identify four sources of inefficiency:

1. synchronization overhead;
2. thief-induced cache misses / owner interference;
3. victim-selection overhead;
4. correctness and barrier cost under weak memory models.

The paper explicitly discusses language runtimes including Go and Rust Tokio.

---

## 2. Resource model

**SOURCE-DERIVED.** The scheduling resource remains multicore CPU execution capacity. Each core owns a queue and alternates between executing local work and stealing when idle.

BWoS focuses not on higher-level processor allotment but on the **data structure and synchronization protocol** implementing the work-sharing mechanism.

This makes the paper important for ElasticXxx because at sufficiently fine granularity, runtime metadata/cache-coherence traffic is itself part of adaptation cost.

---

## 3. Block-based queue design

**SOURCE-DERIVED.** BWoS divides a per-core queue into multiple blocks with independent metadata. Owner and thieves synchronize at block granularity.

In the common case where owner operations stay within a block that thieves are not touching, synchronization operations can be elided or substantially reduced, approaching sequential-queue performance.

The design also permits thieves to steal from the middle of a queue so owner and thieves are more likely to operate on distinct blocks.

**ELASTIC LESSON — ADOPT PRINCIPLE.** Fast-path data structures should be designed so adaptation affects the common execution path as little as possible.

---

## 4. Victim selection

**SOURCE-DERIVED.** Naively scanning all queues to identify the longest queue can improve load balancing but causes metadata contention and owner interference.

BWoS instead uses a probabilistic stealing policy that makes longer queues more likely victims while avoiding the full scanning cost. The design can incorporate NUMA awareness and batching.

**ELASTIC RELATION.** This is a concrete example of a recurring trade-off:

```text
better information
      ↕
higher observation / coordination cost
```

A planner should not assume that obtaining a more accurate global state is free.

---

## 5. Weak-memory correctness

**SOURCE-DERIVED.** BWoS treats weak-memory correctness as a first-class problem. The authors verify the design with the GenMC model checker and optimize memory barriers using the VSync toolchain.

The paper notes that insufficient barriers can produce correctness bugs while redundant barriers can significantly degrade performance; it also points to a historical work-stealing fix in Rust Tokio.

**KEY ELASTIC LESSON.** At fine granularity, `SemanticContract`/correctness cannot be separated from the concrete memory-ordering implementation. A logically sound scheduling policy can still be incorrectly realized through an unsafe concurrent queue.

ElasticXxx must therefore distinguish:

```text
POLICY CORRECTNESS
    from
MECHANISM / MEMORY-MODEL CORRECTNESS
```

---

## 6. Synchronization cost belongs in transition cost

For a fine-grained scheduling transition, the cost is not just task movement.

A useful decomposition is:

```text
C_steal =
    queue metadata synchronization
  + atomic / barrier cost
  + cache-coherence cost
  + victim-selection cost
  + task-transfer cost
  + locality / NUMA impact
```

This equation is an **ELASTIC PROPOSAL**, motivated by the mechanisms measured in BWoS.

It reinforces the broader principle that the cost model must include the implementation path used to realize a transition, not only its logical state change.

---

## 7. Results

**SOURCE-DERIVED.** The paper reports:

- 8–11× throughput improvement over compared state-of-the-art queue designs in microbenchmarks;
- up to 25% performance improvement when integrated with Java G1GC workloads;
- 25.8% average JSON-processing speedup across nine Go libraries;
- when integrated into Rust Tokio for a Hyper HTTP server: 12.3% higher throughput, 6.74% lower latency and 60.9% lower CPU utilization in the reported setup;
- in the motivating Go JSON case, useful-work CPU ratio rises from 51% to 71% while scheduling/GC/idle costs decrease.

These numbers are implementation/workload-specific and should not be generalized into a universal benefit for block-based queues.

---

## 8. Fine-grained elasticity implication

BWoS demonstrates that an adaptive scheduling mechanism can be invoked so frequently that even a single extra atomic operation or cache-line transfer becomes significant.

Therefore ElasticXxx cannot route every adaptation through:

```text
collect global snapshot
→ build candidate graph
→ score alternatives
→ emit rich telemetry
→ validate through heavyweight machinery
```

for micro-transitions such as task stealing.

**ELASTIC PROPOSAL.** The trusted runtime needs prevalidated fast-path transition families whose legal action space is established ahead of time and whose individual execution requires only minimal dynamic checks.

Example concept:

```text
PrevalidatedLocalPolicy<StealTask>
```

This is a provisional design direction, not an API commitment.

---

## 9. Interaction with the static/dynamic safety boundary

The work-stealing queue itself may be implemented with Rust types and atomics, but Rust's ownership type system alone does not prove the queue correct under every intended weak-memory interleaving.

BWoS's formal verification work is useful evidence that concurrent adaptation mechanisms may require verification below the source-language type layer.

**ELASTIC INFERENCE.** The trusted runtime should keep concurrency primitives small, explicit, benchmarked, and where practical model-checked or otherwise formally validated.

---

## 10. Elastic disposition

| BWoS mechanism | ElasticXxx disposition |
|---|---|
| Block separation of owner/thief metadata | **ADOPT principle for hot-path isolation** |
| Minimize synchronization on common local path | **ADOPT strongly** |
| Probabilistic approximate victim selection | **ADOPT principle / INVESTIGATE policy variants** |
| NUMA-aware victim selection | **ADOPT / Resource Graph relation** |
| Batch stealing | **ADAPT according to task granularity/locality** |
| Weak-memory formal verification | **ADOPT principle for trusted concurrency primitives** |
| Global metadata scans on hot path | **REJECT when avoidable** |
| Scheduler overhead treated as negligible | **REJECT** |

---

## 11. Consequence for H7 — Low-Overhead Elasticity

BWoS makes H7 more concrete but does not prove it for ElasticXxx.

A plausible hypothesis is now:

> Fine-grained elasticity is practical only when the common adaptation path is implemented as a local, bounded, prevalidated mechanism whose synchronization and observation costs are comparable to the underlying scheduling operation.

This must be tested against Rust task runtimes and other resources.

---

## 12. Experiment suggested

**EXPERIMENT REQUIRED.** Build a Rust microbenchmark around an Elastic-compatible task scheduler and compare:

1. no stealing / static partition;
2. conventional Chase-Lev-style work stealing;
3. block-based or similarly isolated work stealing;
4. topology-aware victim selection;
5. higher-level parallelism feedback layered above the same queue.

Measure separately:

- local push/pop latency;
- steal latency and success rate;
- atomics/fences per operation;
- cache misses / coherence traffic;
- NUMA remote accesses;
- throughput;
- p99 scheduling latency;
- useful-work ratio;
- energy if available;
- correctness under stress/model checking.

---

## 13. SciRust gap check

No SciRust gap is established by BWoS. Work-stealing queue design, atomic memory ordering and runtime task scheduling are systems-runtime mechanisms rather than missing general mathematical/scientific primitives.

The current SciRust repository search did not identify a dedicated work-stealing runtime, but adding one merely for ElasticXxx would violate the project's SciRust gap rule unless a broader scientific-computing need is independently established.

---

## 14. Current conclusion

BWoS provides a modern systems-level warning for ElasticXxx: **at fine scheduling granularity, the implementation cost of adaptation can dominate the work being adapted**. The correct abstraction boundary is therefore not just between planner and actuator; it is also between slow resource planning and aggressively optimized, prevalidated local runtime policies.

Its Rust Tokio results make this directly relevant to an ElasticXxx implementation in Rust, while its weak-memory verification demonstrates that high-performance local adaptation still requires rigorous correctness beneath the planner layer.
