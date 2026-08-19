# Sandås et al. (2026) — Seamless Execution of Malleable Applications in Controlled and Production HPC Environments

**Paper:** Petter Sandås, Sergio Iserte, Guillaume Houzeaux, Antonio J. Peña. *Seamless Execution of Malleable Applications in Controlled and Production HPC Environments*. arXiv:2606.13266v2, July 2026.

**Primary source:** arXiv full text.

**Review status:** mechanism-level review complete.

---

## 1. Problem

**SOURCE-DERIVED.** The paper targets a practical deployment barrier: many HPC applications have time-varying resource needs, but production Slurm deployments generally expose rigid allocations and administrators are reluctant to deploy custom malleability-aware scheduler forks.

The authors extend the Dynamic Management of Resources (DMR) framework into DMRv2 so the same malleable MPI application can run in controlled testbeds and on production systems using unmodified site-wide resource managers.

The key systems contribution is **non-invasive malleability**: decouple application/process malleability from native scheduler-level job resizing by coordinating multiple independent allocations at user level.

---

## 2. Runtime resource model

**SOURCE-DERIVED.** DMRv2 operates primarily on MPI process counts and node allocations. Expansion creates or obtains additional resources and reshapes the MPI process layout. Shrink removes resources/ranks when supported by the deployment model.

The framework can use:

- checkpoint/restart (C/R) data redistribution;
- in-memory MPI data redistribution;
- controlled Slurm4DMR environments;
- production DMR@Jobs environments using unmodified Slurm.

**ELASTIC RELATION: ADAPT.** DMRv2 demonstrates real runtime transitions, but its resource semantics remain centered on malleable MPI process layouts and node counts. ElasticXxx intends a more general transition model spanning other resource dimensions.

---

## 3. Expansion is asynchronous

**SOURCE-DERIVED.** In production DMR@Jobs mode, expansions request an independent "expander" job. Resource availability can be delayed by the global scheduler. Rather than block, DMRv2 lets the application continue executing while an expansion remains pending. Once resources arrive, execution is suspended at a suitable point, the new process layout is instantiated, application state is redistributed, and execution resumes.

**IMPORTANT ELASTIC CONSEQUENCE.** A transition cannot be modeled solely as:

```text
State A -> State B
```

A realistic resource transition may have a lifecycle such as:

```text
PROPOSED
  -> REQUESTED
  -> PENDING
  -> READY
  -> QUIESCING / SAFEPOINT
  -> APPLYING
  -> REDISTRIBUTING
  -> VERIFYING
  -> COMMITTED
```

with failure/cancellation/rollback/compensation branches.

This lifecycle is an **ELASTIC PROPOSAL** derived from the source mechanism; the exact state machine remains open.

---

## 4. Shrink and expand are asymmetric

**SOURCE-DERIVED.** DMRv2's production experiments show that expansions may wait unpredictably for resources under scheduler contention, while release/shrink operations can occur immediately or effectively in constant time in the evaluated configuration.

**ELASTIC CONSEQUENCE.** Transition cost and availability are directional. In general:

```text
Cost(A -> B) != Cost(B -> A)
```

and:

```text
Availability(A -> B) != Availability(B -> A)
```

This strengthens the need for transition-specific rather than state-only cost models.

---

## 5. Safe reconfiguration points

**SOURCE-DERIVED.** `dmr_check(...)` may indicate that reconfiguration is ready; the application can then invoke reconfiguration at a convenient synchronization point. The DMR_AUTO helper automates common follow-up actions and calls application-supplied redistribution/restart/finalization handlers.

**ELASTIC DISPOSITION: ADOPT / GENERALIZE.** Some transitions require a quiescent state or application-defined safepoint. ElasticXxx should not assume every legal transition can be applied at every instant.

A candidate transition description may therefore need:

- static preconditions;
- runtime readiness conditions;
- required safepoint/quiescence class;
- cancellation rules;
- timeout/deadline behavior;
- transition-specific state-transfer handler;
- postcondition verification.

---

## 6. Data redistribution is application-specific

**SOURCE-DERIVED.** DMRv2 deliberately lets users supply data redistribution logic. The same high-level API can use either in-memory MPI redistribution or checkpoint/restart, but the actual mapping of application state into the new process layout is application-specific.

**CRITICAL ELASTIC LESSON.** Resource reconfiguration cannot always be separated from semantic state movement. A generic runtime may orchestrate the transition but still require a resource/workload-specific adapter to preserve application state correctly.

ElasticXxx should therefore avoid promising fully automatic arbitrary migration unless a resource-specific transition adapter exists and its semantic contract is known.

---

## 7. Mechanism-independent high-level API

**SOURCE-DERIVED.** DMRv2 lets the same malleable code use different reconfiguration mechanisms/deployment environments. It separates the application-visible malleability API from the lower-level way resources are acquired/released.

**ELASTIC DISPOSITION: ADOPT.** This strongly supports the current Elastic principle:

```text
application/resource intent
        !=
physical transition mechanism
```

The runtime/adapters should choose among mechanisms only when they satisfy the same declared semantics.

---

## 8. Reconfiguration policies

**SOURCE-DERIVED.** DMRv2 includes several policies:

- `ROUND_POLICY`: deliberately cycles between minimum and maximum allocations for development/testing;
- `CE_POLICY`: uses TALP communication efficiency and adapts MPI ranks approximately linearly according to deviation from a target communication-efficiency value;
- `QUEUE_POLICY`: uses queue/idle-node information where scheduler visibility permits;
- policies/suggestions can be changed during runtime without recompiling the application.

**ELASTIC RELATION.** These mechanisms show that runtime policy is already treated as swappable and workload/system observations can drive resource resizing. ElasticXxx should not claim this principle as novel.

Potential generalization lies in composing multiple resource dimensions, typed transitions, explicit semantic invariants and richer planning domains.

---

## 9. Hysteresis / inhibition periods

**SOURCE-DERIVED.** The experiments use an "inhibition period" that sets a minimum spacing between reconfigurations. The paper also notes that communication-efficiency variability can induce small oscillations in node count and that policy tolerance controls aggressiveness.

**ELASTIC DISPOSITION: ADOPT PRINCIPLE.** ElasticXxx should explicitly model anti-oscillation mechanisms such as:

- minimum dwell time;
- hysteresis;
- transition cooldown;
- confidence thresholds;
- expected-benefit-over-transition-cost thresholds.

The exact controller design remains an open research question.

---

## 10. Reconfiguration cost can dominate

**SOURCE-DERIVED.** In the deliberately high-frequency 50-job workload experiment, the average time in the `RECONF` state is reported as **107.14 seconds**, and reconfiguration time dominates useful execution under the intentionally short inhibition periods.

This is strong production-scale evidence for the Elastic principle:

> **Adaptation is not free, and a correct planner must be allowed to choose DO NOTHING.**

The cost model must include not just process creation/removal but state redistribution, synchronization, waiting, checkpointing, restart and orchestration overhead.

---

## 11. Production results

**SOURCE-DERIVED.** On MareNostrum 5 ACC, the MPDATA production run reported approximately 3.0 node-hours versus 11.5 node-hours for the dedicated controlled environment while taking 41 minutes versus 40 minutes, corresponding to a reported **74% reduction in node-hour consumption**.

For Alya on MareNostrum 5 GPP, Table II reports:

- low case: 40.20 node-hours controlled vs. 30.09 production, **25.10% reduction**;
- high case: 81.84 node-hours controlled vs. 36.87 production, **55.15% reduction**.

The paper reports that production runs converge toward similar efficient configurations while avoiding constant reservation of peak resources.

**EVIDENCE LIMITATION.** The authors explicitly state that these large-scale experiments use one run per configuration due to expense and that production queueing, placement and interference vary between executions. These results are therefore important production demonstrations, but should not be treated as statistically replicated microbenchmark estimates.

---

## 12. Deterministic/clean transition strategy

**SOURCE-DERIVED.** DMRv2 describes its methodology as centered on respawning: each reconfiguration restarts the process set under the new layout to ensure clean and deterministic transitions. The framework also supports alternative in-memory mechanisms.

**ELASTIC DISPOSITION: INVESTIGATE / GENERALIZE.** Elastic transitions may need multiple execution strategies:

```text
in-place
copy-and-switch
checkpoint/restart
respawn
shadow/replica then commit
transactional swap
```

The selected mechanism should expose its atomicity, downtime, reversibility and state-transfer cost.

---

## 13. A more realistic transition model for ElasticXxx

Combining this paper with earlier work suggests that the current simple `apply()` notion is insufficient.

**ELASTIC PROPOSAL:** a transition should be treated as a protocol with explicit phases and observable state.

Conceptually:

```text
ElasticTransitionSpec
    identity
    source_state
    target_state
    preconditions
    required_capabilities
    safepoint_requirement
    acquisition_mode
    state_transfer_strategy
    expected_cost
    reversibility
        ↓
prepare/request
        ↓
pending (optional)
        ↓
ready
        ↓
quiesce
        ↓
apply + redistribute
        ↓
verify
        ↓
commit | rollback | compensate
```

This should remain provisional until compared against transactional reconfiguration, live migration, storage systems, distributed systems and control-plane literature.

---

## 14. Relationship table

| DMRv2 mechanism | ElasticXxx disposition |
|---|---|
| Runtime grow/shrink | **ADOPT as established primitive / GENERALIZE** |
| Asynchronous expansion | **ADOPT / model pending transition state** |
| Continue work while acquisition is pending | **ADOPT principle** |
| Directional grow/shrink cost | **ADOPT / GENERALIZE** |
| Safepoint before reconfiguration | **ADOPT / GENERALIZE** |
| User-defined state redistribution | **ADOPT necessity / encapsulate in adapter** |
| C/R and in-memory under same API | **ADOPT mechanism independence** |
| Swappable policies | **ADOPT prior art** |
| CE-driven resizing | **INVESTIGATE as baseline controller** |
| Inhibition period | **ADOPT principle / generalize to hysteresis/dwell time** |
| Respawn for clean deterministic transitions | **INVESTIGATE as transition strategy** |
| MPI process count as primary elastic dimension | **ADAPT strongly** |
| Node-hour optimization | **ADOPT as one possible objective, not universal** |

---

## 15. SciRust gap check

No SciRust gap is confirmed by this paper alone.

However, the paper gives us future scientific questions that may exercise SciRust during R&D:

- controller stability and oscillation analysis;
- adaptive/hysteretic control;
- stochastic waiting-time models for acquisition;
- transition-cost estimation;
- multiobjective policies balancing useful progress, resource cost and reconfiguration overhead.

If SciRust lacks a general capability required to investigate one of these rigorously, we should record that as a separate gap at the moment the need becomes concrete.

---

## 16. Current conclusion

This paper supplies something earlier work did not: a strong production demonstration that malleability can work on real, heavily shared HPC systems without requiring a custom site-wide scheduler.

For ElasticXxx, its most valuable lesson is not merely that resources can grow and shrink, but that **runtime transitions are asynchronous protocols with safepoints, state transfer, environmental uncertainty, directional cost and potentially long reconfiguration phases**.

That observation materially strengthens the emerging Elastic transition model.
