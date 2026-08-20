# DistServe: Disaggregating Prefill and Decoding for Goodput-Optimized LLM Serving

**Paper:** Yinmin Zhong, Shengyu Liu, Junda Chen, Jianbo Hu, Yibo Zhu, Xuanzhe Liu, Xin Jin, Hao Zhang. *DistServe: Disaggregating Prefill and Decoding for Goodput-optimized Large Language Model Serving*. OSDI 2024.

**Primary source:** https://www.usenix.org/system/files/osdi24-zhong-yinmin.pdf

## Problem

**SOURCE-DERIVED.** Prefill and decode have different computational characteristics and latency objectives. Co-locating them creates interference and couples resource-allocation and parallelism decisions. DistServe separates them onto different GPUs and optimizes each phase independently while accounting for KV transfer.

## Resource / objective decomposition

**SOURCE-DERIVED.** DistServe treats TTFT and TPOT as distinct SLO dimensions. It searches parallel configurations separately for prefill and decode, estimates per-configuration goodput through simulation/profiling, and replicates the selected instance types to satisfy a target traffic rate.

This is another strong example of decomposing a system by **operational phase**, not merely by physical resource type.

## Planner

**SOURCE-DERIVED.** For high node-affinity/high-bandwidth clusters, DistServe enumerates feasible intra-/inter-op parallel configurations, uses simulators for prefill and decode to estimate goodput under workload and SLO constraints, and selects configurations based on goodput per GPU. The paper reports `O(NM^2)` search complexity and solving time below 1.3 minutes in its largest reported setting.

For lower cross-node bandwidth, placement becomes constrained by the need to transfer KV between corresponding prefill/decode stages. DistServe co-optimizes model parallelism and placement so corresponding stages can exploit high-bandwidth intra-node links where needed.

## KV as transition state

**SOURCE-DERIVED.** KV state is the material dependency connecting the two otherwise disaggregated planning domains. Disaggregation therefore creates a transition edge whose cost depends on amount of KV state and network topology.

## Results

**SOURCE-DERIVED.** USENIX reports that DistServe can serve up to 7.4× more requests or meet up to 12.6× tighter SLOs than compared systems while keeping over 90% of requests within latency constraints in the evaluated settings.

## Elastic relation

- phase/domain decomposition: **ADOPT / GENERALIZE**;
- separate objectives/constraints by phase: **ADOPT**;
- cost-bearing state handoff between domains: **ADOPT / GENERALIZE**;
- topology-aware placement: **ADOPT / Resource Graph**;
- simulator-backed candidate evaluation: **ADOPT principle**;
- fixed prefill/decode domain split: **ADAPT / domain-specific**;
- exhaustive specialized configuration enumeration: **INVESTIGATE as backend**.

## Strong Elastic lesson

A planning-domain graph should permit **state-carrying edges**:

```text
PREFILL DOMAIN
   |
   | KV state transfer
   | cost = f(bytes, topology, bandwidth, representation)
   v
DECODE DOMAIN
```

The representation selected for the transferred resource can itself affect transition cost. This is directly relevant to combining SciRust's representation-adaptive KV research with physical/distributed residency mechanisms.

## Comparison with SciRust

SciRust already studies KV representation, paging, compression, latent ranks, residuals, strict budgets, hysteresis, material HOT/WARM/COLD recompression and epochs. DistServe contributes primarily a system-level decomposition and placement problem across GPU instances rather than a missing KV representation primitive.

No new SciRust gap is declared from this paper alone.
