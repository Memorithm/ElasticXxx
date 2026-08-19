# Pollux — Co-adaptive Cluster Scheduling for Goodput-Optimized Deep Learning

**Citation:** Aurick Qiao, Sang Keun Choe, Suhas Jayaram Subramanya, Willie Neiswanger, Qirong Ho, Hao Zhang, Gregory R. Ganger, and Eric P. Xing. *Pollux: Co-adaptive Cluster Scheduling for Goodput-Optimized Deep Learning.* 15th USENIX Symposium on Operating Systems Design and Implementation (OSDI 21), pages 1–18, July 2021.

**Primary sources:**
- USENIX publication page: https://www.usenix.org/conference/osdi21/presentation/qiao
- Paper: https://www.usenix.org/system/files/osdi21-qiao.pdf
- OSDI'21 artifact branch: https://github.com/petuum/adaptdl/tree/osdi21-artifact

**Status in ElasticXxx review:** mechanism review completed; several mechanisms selected for ADOPT/ADAPT; proposed generalization requires experiments.

---

## 1. Problem

**SOURCE-DERIVED.** Pollux addresses shared deep-learning clusters in which resource allocation and training configuration are interdependent. Existing schedulers either keep the resource allocation fixed or adapt resource allocation without jointly adapting training parameters. Pollux argues that the number and placement of GPUs, batch size, gradient accumulation, and learning-rate scaling should be co-adapted because their best values change with cluster contention and training progress.

The paper's central metric is **goodput**, defined at training iteration `t` as:

\[
GOODPUT_t(*) = THROUGHPUT(*) \times EFFICIENCY_t(M(*))
\]

where throughput is training examples processed per wall-clock time and statistical efficiency estimates useful training progress per example relative to a baseline batch size.

The implementation optimizes three principal configuration variables in the goodput model:

- resource allocation / GPU placement `a`;
- per-GPU batch size `m`;
- gradient accumulation steps `s`.

Learning rate is adapted via a plug-in scaling rule as total batch size changes.

---

## 2. Resource model

**SOURCE-DERIVED.** Pollux's resource model is deliberately specialized to distributed DL training. At cluster level, the primary elastic resource is the GPU allocation and placement of each job. The throughput model is parameterized by the number and co-locality of GPUs, batch size, and gradient accumulation.

The paper explicitly states that its throughput model does **not** model accelerator heterogeneity. It also identifies possible divergence on specialized hardware, sophisticated synchronization algorithms, different parallelization strategies, larger scales, and hidden resource contention outside the modeled network synchronization effects.

Pollux therefore demonstrates a strong adaptive mechanism without claiming a universal resource model.

**ELASTIC RELATION: ADAPT.** ElasticXxx should treat Pollux's GPU allocation as one implementation of a more general resource state rather than as the resource abstraction itself.

---

## 3. Observability

**SOURCE-DERIVED.** Each `PolluxAgent` continuously observes quantities including:

- time per training iteration;
- system throughput;
- gradient statistics / pre-conditioned gradient noise scale (PGNS);
- configurations encountered during execution.

The agent periodically fits the job's throughput model to all collected measurements and reports model parameters and current gradient statistics to the cluster scheduler.

Pollux therefore validates an important architectural premise: a resource controller can learn workload-specific performance behavior online rather than require a complete offline profile.

**ELASTIC RELATION: ADOPT.** Online observation and model refinement should be a first-class Elastic capability.

---

## 4. Goodput and useful progress

### 4.1 Pollux mechanism

**SOURCE-DERIVED.** Pollux does not optimize raw throughput. It multiplies system throughput by statistical efficiency so that work which processes more examples but makes proportionally less training progress is discounted.

For the studied DL workloads, Pollux models statistical efficiency through the pre-conditioned gradient noise scale and predicts how efficiency changes under different total batch sizes. The resulting goodput estimates the useful portion of throughput.

This is a major conceptual result for ElasticXxx: resource utilization is not necessarily equivalent to application progress.

### 4.2 Elastic generalization

**ELASTIC PROPOSAL.** `GOODPUT` should not become a universal Elastic primitive because its particular definition depends on DL training statistics. Instead, ElasticXxx should generalize the underlying concept into a workload-supplied **Useful Progress** model.

Provisional abstraction:

```rust
trait ElasticProgressModel {
    type State;
    type Observation;
    type Progress;

    fn observe_progress(&self, observation: &Self::Observation) -> Self::Progress;

    fn predict_progress(
        &self,
        state: &Self::State,
    ) -> ElasticPrediction<Self::Progress>;
}
```

The exact API is not fixed.

Examples of possible workload-specific useful-progress semantics include:

- DL training: statistical training progress per unit wall-clock time;
- model serving: requests or tokens satisfying correctness/SLO constraints per unit time;
- numerical solver: residual reduction or converged iterations per unit time;
- compiler: completed dependency-respecting work per unit time;
- database: committed useful transactions satisfying latency/durability constraints;
- storage system: application-visible useful I/O rather than raw device traffic.

**OPEN QUESTION.** A sufficiently general definition of useful progress may not exist for every workload. ElasticXxx should therefore support workload-specific progress models rather than force a universal scalar.

**Classification: ADAPT.** Preserve Pollux's principle, generalize its metric.

---

## 5. Local and global co-adaptation

### 5.1 Pollux mechanism

**SOURCE-DERIVED.** Pollux uses two cooperating control levels:

1. `PolluxAgent`, running with each training job, fits throughput/statistical-efficiency models and tunes the job's batch size and learning rate for its current allocation.
2. `PolluxSched`, operating cluster-wide, periodically reallocates resources using the reported goodput functions while accounting for fairness, contention, and reallocation overhead.

The two components co-adapt: changing cluster allocation changes the best job configuration, while the job's ability to retune changes which resource allocation is attractive.

### 5.2 Elastic generalization

**ELASTIC PROPOSAL.** This is stronger than a simple centralized scheduler and should influence ElasticXxx, but the hierarchy should not be hard-coded to exactly `job ↔ cluster`.

A general Elastic system may need multiple nested or graph-related planning domains:

```text
resource-local model
        ↕
workload-local planner
        ↕
node planner
        ↕
cluster planner
        ↕
organization / policy plane
```

A memory-residency planner and a compute-parallelism planner may also need to co-adapt without one being a strict child of the other.

**Classification: ADOPT principle / ADAPT structure.**

---

## 6. Online model fitting and exploration

### 6.1 Pollux mechanism

**SOURCE-DERIVED.** Pollux fits a parameterized throughput model online using observations collected during training. It minimizes root mean squared logarithmic error with L-BFGS-B. At the beginning of a job, before measurements are available, Pollux uses priors that optimistically assume good scalability and encourages exploration of additional GPUs.

To avoid immediately scaling a new job to arbitrarily many GPUs, Pollux restricts the maximum GPU allocation to at most twice the maximum allocation the job has experienced previously. The authors report that this simple prior-driven exploration performs within 2–5% of an idealized offline-fitted scenario in their experiments.

### 6.2 Elastic alternative

**ELASTIC PROPOSAL.** Elastic should not standardize one exploration rule such as "at most 2× previous allocation". Exploration should be represented explicitly as a policy with:

- uncertainty/confidence;
- exploration budget;
- maximum semantic or availability risk;
- transition cost;
- reversibility;
- expected information gain where available.

A candidate transition should be able to be useful either because it improves execution immediately or because it safely reduces uncertainty about the Elastic Space.

A provisional planning score might therefore distinguish exploitation from information-gathering value:

\[
Value(P) = E[UsefulProgress(P)] - Cost(P) - Risk(P) + \lambda\,InformationGain(P)
\]

This is an **ELASTIC PROPOSAL**, not a claim from Pollux and not yet a validated formulation.

**Classification: ADAPT / INVESTIGATE.**

---

## 7. Cluster-wide optimization and fairness

### 7.1 Pollux mechanism

**SOURCE-DERIVED.** Pollux defines each job's speedup relative to a fair-resource allocation, then aggregates speedups through a generalized power mean. The exponent `p` acts as a fairness knob. The paper reports that `p = -1` preserves most of the goodput improvement while providing reasonable finish-time fairness in its experiments.

The artifact implements the allocation search as a population-based optimizer. In the OSDI'21 artifact branch, `PolluxPolicy` uses NSGA-II through `pymoo`; candidate allocation matrices are mutated, crossed over, repaired to satisfy constraints, and ranked by objectives. The implementation explicitly repairs candidate states for pinned jobs, node resource capacity, min/max replicas, and the restriction that at most one distributed job occupies a node.

### 7.2 Elastic alternative

**ELASTIC PROPOSAL.** ElasticXxx should not bake one fairness function or one search algorithm into the semantic core.

Instead:

```text
Elastic semantic core
    defines legal states/transitions

Elastic workload model
    defines useful progress

Elastic resource models
    define capacities/costs/contention

Elastic policy
    defines priorities, fairness, SLOs, budgets

Elastic planner
    selects an optimization/search strategy
```

The Pollux power mean could then be one `ElasticPolicy` implementation for a particular class of multi-tenant workloads.

Similarly, NSGA-II or another population-based search may be a planner backend, not part of the programming model.

**Classification:**
- fairness as explicit policy: **ADOPT**;
- Pollux's specific power-mean fairness: **ADAPT / optional policy**;
- population search: **INVESTIGATE as planner backend**.

---

## 8. Transition-cost accounting

### 8.1 Pollux mechanism

**SOURCE-DERIVED.** Reallocating a training job requires reconfiguration. Using checkpoint-restart, the authors measured delays between 15 and 120 seconds depending on model size and initialization work. Pollux therefore penalizes allocation candidates that require a restart. The penalty depends on job age, prior number of reallocations, and an estimated reallocation delay.

In the main testbed configuration, the scheduler runs every 60 seconds and uses a 30-second reallocation-delay estimate. The paper reports an average of one resource reallocation per job every seven minutes and an average runtime overhead of approximately 8% due to checkpoint-restarts. The cluster scheduler itself spent about one second of one vCPU per 60-second interval on its optimization in the reported testbed.

This is direct evidence that adaptation cost is large enough to change the optimal decision.

### 8.2 Elastic generalization

**ELASTIC PROPOSAL.** Transition cost should be attached to each legal transition rather than folded into one scheduler-specific penalty.

A transition model should be able to expose at least:

```text
latency cost
compute cost
memory cost
network / I/O cost
energy cost
availability interruption
semantic risk
reversibility
uncertainty
```

A migration from VRAM to RAM, a batch-size change, a GPU reassignment, a representation change, and a replication operation have fundamentally different transition costs.

Elastic's planner should therefore compare complete transition paths, not only target states.

**Classification: ADOPT principle / ADAPT representation.**

---

## 9. Interference and resource relationships

### 9.1 Pollux mechanism

**SOURCE-DERIVED.** Pollux handles a known network-interference case with a hard scheduling constraint: two distributed DL jobs may not share the same node. In the authors' interference experiment, disabling avoidance under a simulated 50% interference slowdown increases average JCT by 1.4×, whereas avoidance prevents the degradation.

### 9.2 Elastic alternative

**ELASTIC PROPOSAL.** Hard constraints are appropriate when a relationship is prohibited, but not every interference relationship should be encoded as a special-case scheduler rule.

ElasticXxx's proposed Resource Graph should represent relationships such as:

```text
SHARES(link)
COMPETES_WITH(resource)
LOCATED_ON(node)
CONNECTED_TO(device)
```

A contention model can then predict or observe how concurrent consumers change effective capacity. A policy may still promote a predicted pathological regime into a hard constraint when appropriate.

This would make Pollux's "at most one distributed job per node" rule one concrete instance of a general resource-relation constraint.

**Classification: ADAPT.**

---

## 10. Adaptation cadence

### 10.1 Pollux mechanism

**SOURCE-DERIVED.** Both agent reporting and cluster optimization are periodic. The testbed uses a 60-second scheduler interval and 30-second agent reporting. In sensitivity experiments, average JCT remains similar for scheduling intervals up to about two minutes, while longer intervals degrade performance. The authors attribute only roughly half of that degradation to queue delay, indicating a benefit from relatively frequent reallocation decisions.

### 10.2 Elastic alternative

**ELASTIC PROPOSAL.** A single fixed period is unlikely to be optimal across resource types. Elastic should investigate hybrid triggers:

- periodic observations;
- event-triggered replanning;
- pressure threshold crossings;
- forecasted constraint violation;
- topology change;
- significant prediction error;
- completion of a costly transition;
- minimum dwell times to avoid oscillation.

Different resources may operate at different control timescales.

**Classification: ADAPT / EXPERIMENT REQUIRED.**

---

## 11. Safety, invariants, and semantic contracts

**SOURCE-DERIVED.** Pollux protects final training quality operationally through its statistical-efficiency model, learning-rate scaling rules, and an application-provided maximum batch-size limit for cases where LR scaling may cease to preserve model quality. The authors report similar best validation metrics across the tested batch sizes, generally within ±1% relative difference, except DeepSpeech2 at ±4%.

However, Pollux is not a general semantic-contract system. It does not provide typed transition legality, general post-transition verification, transaction semantics, or rollback of arbitrary resource adaptations.

**ELASTIC PROPOSAL.** ElasticXxx should separate optimization objectives from semantic invariants:

```text
Useful progress  → optimize
Latency          → optimize or constrain
Cost             → optimize or constrain
Fairness         → policy
Exactness        → invariant
Type safety      → invariant
Authorized error → contract
```

A planner may optimize only after the candidate state and transitions have passed invariant validation.

**Classification: ADAPT strongly.**

---

## 12. Reversibility

**SOURCE-DERIVED.** Pollux can later change a job's allocation again, but the paper does not define a transactional rollback protocol for failed adaptation. Reallocation is implemented with checkpoint-restart, which provides restartability rather than a general rollback abstraction.

**ELASTIC PROPOSAL.** Elastic transitions should declare whether they are:

- reversible;
- compensatable;
- restartable;
- irreversible.

Rollback should never be assumed when the underlying transition cannot support it.

**Classification: EXTEND beyond Pollux.**

---

## 13. Experimental results

**SOURCE-DERIVED.** The paper reports the following results:

- Pollux is an OSDI'21 Best Paper.
- In testbed experiments, compared with well-tuned `Optimus+Oracle` and `Tiresias` configurations, Pollux reports **50% and 37% shorter average job completion time**, respectively.
- Against more realistic baseline configurations, the paper reports reductions in average JCT of up to **72–73%**.
- The paper reports finish-time-fairness improvements of **1.5×–5.4×**.
- In trace-driven simulation, Pollux reports **48% and 32%** lower average JCT than `Optimus+Oracle+TunedJobs` and `Tiresias+TunedJobs`, respectively, similar in direction to the testbed results.
- Its prior-driven online exploration is reported to be within **2–5%** of an idealized offline-fitted throughput model.
- In a cloud auto-scaling experiment, Pollux's goodput-based policy reports **25% lower training cost** than the compared throughput-based auto-scaler, at **6% longer completion time**. The paper explicitly presents the broader autoscaling design as preliminary/future work.
- In the reported HPO experiment, Pollux completes the workload **30% faster** while reaching similar top-trial accuracy.

These are results for the Pollux workload/model assumptions. They are evidence for co-adaptation and useful-progress-aware scheduling, not evidence that the same quantitative gains will hold for ElasticXxx.

---

## 14. Explicit limitations relevant to ElasticXxx

**SOURCE-DERIVED.** Important limitations acknowledged or visible in the design include:

1. the throughput model does not account for accelerator heterogeneity;
2. the modeled dimensions are specific to data-parallel DL training;
3. the simple throughput model may diverge on specialized hardware, alternative synchronization or parallelization strategies, larger scales, or unmodeled contention;
4. application-specific learning-rate scaling remains a plug-in responsibility;
5. maximum safe batch size can require application knowledge;
6. resource changes can incur expensive checkpoint-restart overhead;
7. the full cloud auto-scaling system is left as future work;
8. a full evaluation across HPO algorithm classes is left as future work.

These limitations make Pollux a strong systems precedent without making it a general resource-elastic programming model.

---

## 15. ElasticXxx classification

| Pollux mechanism | ElasticXxx decision |
|---|---|
| Optimize useful progress rather than raw utilization | **ADOPT / GENERALIZE** |
| Online workload observation and model fitting | **ADOPT** |
| Co-adaptation across interdependent variables | **ADOPT** |
| Two-level job/cluster controller | **ADAPT** into composable planning domains |
| Goodput formula specific to DL | **ADAPT** into workload-specific `UsefulProgress` |
| Explicit reallocation-cost penalty | **ADOPT / GENERALIZE** into typed transition costs |
| Power-mean fairness knob | **ADAPT** into a policy plug-in |
| Population-based / NSGA-II allocation search | **INVESTIGATE** as one planner backend |
| Prior-driven exploration | **ADAPT** into uncertainty/risk/information-aware exploration |
| Fixed periodic scheduling | **ADAPT** into hybrid event/time-driven control |
| Hard-coded network interference rule | **ADAPT** into Resource Graph relationships + contention models |
| Checkpoint-restart reallocation | **ADAPT** as one transition mechanism |
| General semantic verification / rollback | **NOT PROVIDED — EXTEND** |
| Type-safe resource capabilities | **NOT PROVIDED — EXTEND** |

---

## 16. Emerging Elastic formulation inspired by Pollux

**ELASTIC PROPOSAL.** Pollux suggests that the optimization objective should represent useful application progress, but ElasticXxx needs to separate workload semantics from resource-management semantics.

A provisional decomposition is:

\[
\text{WorkloadValue}(s) = E[UsefulProgress(s)]
\]

\[
\text{PlanCost}(P) = TransitionCost(P) + RuntimeOverhead(P) + ResourceCost(P)
\]

and candidate plans are compared only after invariant validation:

\[
P^* = \arg\max_P \left(\text{WorkloadValue}(s_P)-\text{PlanCost}(P)-Risk(P)\right)
\]

subject to:

\[
\forall i\in I,\quad i(s_P)=true.
\]

For multi-tenant systems, fairness or organizational policy should be applied by a higher-level policy aggregation function rather than embedded into the workload's progress model.

This formulation is provisional and requires comparison against control theory, scheduling theory, utility-based computing, multi-objective optimization, and decision-making under uncertainty before any novelty claim.

---

## 17. Experiments suggested for ElasticXxx

### E-POLLUX-1 — Useful progress vs raw throughput

Build one workload where a higher raw processing rate can produce less useful progress. Compare:

- throughput-only policy;
- workload-specific `UsefulProgress` policy;
- hand-tuned oracle.

Measure completion time, resource consumption, transition count, and semantic quality.

### E-POLLUX-2 — Transition-aware planning

Inject resource states in which migration/reallocation is beneficial only when its cost is low enough. Compare:

- target-state-only planner;
- fixed transition penalty;
- measured transition-cost model.

### E-POLLUX-3 — Periodic vs event-driven planning

Compare fixed scheduling intervals against hybrid event/pressure/prediction-triggered replanning under time-varying load.

### E-POLLUX-4 — Planner backend independence

Run the same Elastic resource model and policy using multiple planner backends, e.g. greedy/heuristic, population search, mathematical optimization where tractable. Determine whether the semantic core remains planner-independent.

### E-POLLUX-5 — Cross-resource co-adaptation

Construct a workload where memory residency and compute parallelism interact. Compare independent single-resource controllers against a co-adaptive planner.

---

## 18. Research conclusion

Pollux provides strong experimental evidence for three principles central to ElasticXxx:

1. **resource decisions and workload configuration may need to be optimized jointly**;
2. **useful application progress is a better optimization target than raw resource utilization or throughput alone**;
3. **adaptation cost must be included in the decision because reconfiguration can be expensive**.

ElasticXxx should not copy Pollux's DL-specific goodput model or GPU-specific allocation space into its core. The stronger direction is to separate a workload-specific useful-progress model from generic resource state, transition-cost, invariant, uncertainty, and policy models.

The key research question opened by this paper is therefore:

> Can Pollux's successful co-adaptive principle be generalized into a resource-agnostic runtime in which workload-specific useful progress and resource-specific transition models compose through a common, type-safe Elastic planning interface?
