# Adaptive Work Stealing with Parallelism Feedback

**Paper:** Kunal Agrawal, Yuxiong He, Charles E. Leiserson. *Adaptive Work Stealing with Parallelism Feedback*. PPoPP 2007. Extended journal version with Wen-Jing Hsu: ACM TOCS 2008.

**Primary sources:**

- https://www.microsoft.com/en-us/research/wp-content/uploads/2016/02/asteal.pdf
- https://www.microsoft.com/en-us/research/publication/adaptive-work-stealing-with-parallelism-feedback/

**Review status:** mechanism-level review complete.

---

## 1. Problem

**SOURCE-DERIVED.** A-STEAL targets fork-join multithreaded jobs in a shared multiprogramming environment where the number of processors available to one job can change while the job is running.

The scheduling architecture has two levels:

1. a **job scheduler** decides how many processors a job receives;
2. a **thread scheduler** schedules the job's ready work on those allotted processors.

A-STEAL's contribution is to make the thread scheduler provide continual **parallelism feedback** to the job scheduler in the form of processor requests.

---

## 2. Resource model

**SOURCE-DERIVED.** The controlled resource is processor allotment.

Important quantities per scheduling quantum include:

- `d_q` — processor **desire** requested by the job;
- `a_q` — processor **allotment** actually granted;
- work/non-steal usage during the quantum;
- quantum length `L`;
- utilization threshold `δ`;
- responsiveness factor `ρ`.

The job scheduler may grant fewer processors than requested. The analysis explicitly treats external availability/allocation as adversarial.

**ELASTIC LESSON.** Desired resource state and granted physical state are separate objects:

```text
DESIRED CAPACITY != ALLOTTED CAPACITY
```

This generalizes naturally to remote/cloud/GPU resource requests that may remain partially satisfied or pending.

---

## 3. Adaptive work-stealing mechanism

**SOURCE-DERIVED.** During each quantum, A-STEAL uses randomized work stealing on the processors currently allotted to the job.

Two modifications handle changing allotment:

### Allotment gain

When new processors are granted, their queues begin empty and they immediately steal work.

### Allotment loss

When processors are removed, ready work may remain in queues that no longer have an associated processor. A-STEAL introduces **mugging**: an active processor can take over an entire orphaned ready queue before falling back to normal stealing.

This is an important example of an adaptation mechanism that handles both resource growth and shrinkage while preserving executable work.

---

## 4. Parallelism feedback / desire estimation

**SOURCE-DERIVED.** A-STEAL classifies the previous quantum into three used cases:

- **inefficient**;
- **efficient and satisfied**;
- **efficient and deprived**.

The next desire is updated multiplicatively:

```text
inefficient:
    desire /= ρ

efficient + satisfied:
    desire *= ρ

efficient + deprived:
    desire unchanged
```

A quantum is satisfied when allotment equals desire. It is deprived when the allocator grants fewer processors than requested. Efficiency is determined from observed non-steal usage relative to the allotted processor-time, using `δ` as the utilization threshold.

Typical values discussed in the paper are roughly 80–95% for `δ` and 1.2–2.0 for `ρ`.

---

## 5. Why this matters for ElasticXxx

A-STEAL introduces a clean separation between:

```text
MICRO SCHEDULER
    work stealing every time local work runs out

        ↓ aggregated feedback

MACRO CONTROLLER
    processor desire once per quantum

        ↓

EXTERNAL ALLOCATOR
    actual processor allotment
```

**ELASTIC DECISION: ADOPT / GENERALIZE.** This is strong prior art for a multi-timescale Elastic architecture.

ElasticXxx should not claim novelty for "local runtime feedback driving higher-level resource allocation".

---

## 6. Feedback is intentionally low-dimensional

**SOURCE-DERIVED.** A-STEAL does not send the entire internal ready-task graph or run a global optimizer. It reduces recent execution behavior to a small amount of feedback: utilization/classification and a processor desire.

**ELASTIC PROPOSAL.** Fast-path mechanisms should expose **compressed sufficient feedback** to slower control layers rather than forcing those layers to observe every task event.

Potential generic examples:

```text
parallelism_demand
steal_pressure
queue_starvation
locality_miss_rate
contention_rate
```

These names are Elastic proposals, not A-STEAL terminology.

---

## 7. Performance guarantees

**SOURCE-DERIVED.** The paper analyzes A-STEAL using trimmed availability and proves near-optimal expected/probabilistic bounds under its assumptions. The bound depends on work `T1`, span `T∞`, scheduling quantum length, processor availability, and the responsiveness parameter.

The authors also prove bounded waste and show that the scheduler can achieve near-linear speedup when job parallelism dominates trimmed processor availability.

**LIMITATION.** These guarantees rely on the fork-join/work-stealing model, the assumed quantum structure, and analysis assumptions such as simplified steal costs. They are not generic guarantees for arbitrary resource transitions.

---

## 8. Empirical/simulation result

**SOURCE-DERIVED.** In the reported 1000-processor multiprogramming simulation, A-STEAL combined with dynamic equipartitioning consistently obtains higher utilization than the compared ABP + equipartitioning setup. The paper reports that mean completion time under ABP+EQ is nearly 50% slower in the shown workload distributions.

This is evidence for the usefulness of parallelism feedback in that simulated environment, not a universal performance result.

---

## 9. Hysteresis / responsiveness interpretation

A-STEAL's `ρ` parameter explicitly controls how quickly demand estimates expand or contract. Combined with fixed scheduling quanta, this is a simple discrete feedback controller.

**ELASTIC RELATION — ADOPT PRINCIPLE / ADAPT.** ElasticXxx already needs:

- response rate;
- dwell/quantum time;
- hysteresis;
- adaptation intensity.

A-STEAL demonstrates that these need not require complex forecasting or ML.

---

## 10. Pending / partial satisfaction

The `desire`/`allotment` distinction is especially relevant to our transition semantics.

A requested transition such as:

```text
workers: 8 → 16
```

may produce:

```text
requested = 16
granted   = 12
```

or remain pending until more capacity appears.

**ELASTIC PROPOSAL.** Resource requests should be able to represent partial satisfaction where the resource semantics permit it, rather than forcing every request into binary success/failure.

---

## 11. Elastic disposition

| A-STEAL mechanism | ElasticXxx disposition |
|---|---|
| Two-level scheduling | **ADOPT / GENERALIZE to multi-level control** |
| Work stealing inside allotment | **ADOPT for task/concurrency fast path** |
| Processor desire feedback | **ADAPT into generic resource demand feedback** |
| Desired vs allotted resource state | **ADOPT** |
| Multiplicative responsiveness | **ADOPT as simple baseline / INVESTIGATE alternatives** |
| Scheduling quanta | **ADOPT concept / ADAPT per resource timescale** |
| Mugging orphaned queues | **ADAPT as task-preservation mechanism during shrink** |
| Adversarial allocator model | **ADOPT as robustness mindset** |
| Processor-only resource model | **REJECT as general Elastic assumption** |

---

## 12. New Elastic architecture implication

**ELASTIC PROPOSAL.** We should distinguish three signals:

```text
LOCAL PRESSURE
    e.g. worker starved / queue empty

AGGREGATED DEMAND
    e.g. desired concurrency

GRANTED CAPACITY
    e.g. actual workers/cores available
```

The local scheduler reacts immediately to local pressure. A regional controller periodically converts aggregated demand into a recommendation. The physical allocator/actuator determines granted capacity.

This avoids forcing fine-grained events through a global planner.

---

## 13. Experiment suggested

**EXPERIMENT REQUIRED.** For an Elastic task runtime, compare:

1. fixed worker count;
2. ordinary work stealing;
3. work stealing + A-STEAL-style processor desire;
4. work stealing + richer pressure/locality feedback;
5. global planner with the same resource budget.

Inject dynamic changes in available cores and measure:

- completion time;
- useful processor utilization;
- wasted steal cycles;
- adaptation lag;
- number of worker-count changes;
- queue migration cost;
- p99 task latency;
- scheduler overhead.

---

## 14. Current conclusion

A-STEAL is direct prior art for one of ElasticXxx's emerging architectural ideas: **fast local scheduling can feed a slower resource-allocation controller without exposing every fine-grained event to that controller**.

The important generalization for ElasticXxx is not the specific multiplicative desire heuristic. It is the separation of **local execution policy, aggregated resource demand, and externally granted capacity**, each operating at an appropriate timescale.
