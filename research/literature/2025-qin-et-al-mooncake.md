# Mooncake: Trading More Storage for Less Computation

**Paper:** Ruoyu Qin, Zheming Li, Weiran He, Jialei Cui, Feng Ren, Mingxing Zhang, Yongwei Wu, Weimin Zheng, Xinran Xu. *Mooncake: Trading More Storage for Less Computation — A KVCache-centric Architecture for Serving LLM Chatbot*. FAST 2025, Best Paper.

**Primary source:** https://www.usenix.org/system/files/fast25-qin.pdf

**Primary implementation:** https://github.com/kvcache-ai/Mooncake

## Problem

**SOURCE-DERIVED.** Long-context serving makes recomputing prefixes expensive and KV state very large. Mooncake separates prefill and decode clusters and pools underused CPU, DRAM, SSD and NIC resources into a distributed KV cache so reusable prefixes can be retained and moved rather than recomputed.

## Physical residency model

**SOURCE-DERIVED.** Mooncake's architecture explicitly spans:

- GPU/VRAM paged KV caches;
- CPU/DRAM;
- SSD;
- remote nodes connected through RDMA/NIC resources.

The paper's KV transfer engine provides transfers for DRAM and GPU VRAM, supports GPU Direct RDMA when appropriate, exposes asynchronous completion state, and performs topology-aware path selection.

## Cache-object model

**SOURCE-DERIVED.** KV is stored as paged blocks. Blocks have hash/prefix-derived keys for deduplication and may have multiple replicas on different nodes. When capacity is full, Mooncake uses LRU eviction unless the block is currently in use. Its object API includes `put`, `get`, and `change_replica`, allowing the scheduler to change replica counts for hot blocks to aggregate bandwidth and reduce access latency.

This makes `RESIDENCY` and `REDUNDANCY` independently variable dimensions of one logical cached object.

## Different optimization domains

**SOURCE-DERIVED.** Mooncake assigns distinct goals to the two serving phases:

- prefill: maximize cache reuse subject to TTFT SLO, MFU lower bound, and DRAM capacity;
- decode: maximize throughput subject to TBT SLO and VRAM capacity.

The central Conductor pairs prefill/decode instances and coordinates cache-aware prefill scheduling, cache balancing, and decode load balancing.

## Network / topology

**SOURCE-DERIVED.** Transfers are routed using topology information covering CPU sockets, DRAM, GPU and NIC relationships. The scheduler can respond to congestion by increasing replica counts for hot KV blocks. The paper reports that performance degrades substantially when available communication bandwidth falls below the regime needed to hide KV movement behind computation.

## Results

**SOURCE-DERIVED.** The USENIX paper reports effective-request-capacity improvements of 59%–498% over baseline methods across real-trace tests while meeting SLOs, and production improvements of 115% and 107% more handled requests on A800 and H800 clusters respectively versus the prior system. The published system was operating at thousands-of-node scale and over 100 billion tokens per day.

## Elastic relation

- physical multi-tier residency: **ADOPT / GENERALIZE**;
- independent replica count: **ADOPT**;
- topology-aware transfer paths: **ADOPT**;
- async transfer status: **ADOPT**;
- cache object identity independent of address: **ADOPT / GENERALIZE**;
- phase-specific objectives/constraints: **ADOPT**;
- LRU as universal eviction policy: **INVESTIGATE / ADAPT**;
- KV/prefix-specific hash semantics: **domain-specific**.

## Comparison with SciRust

SciRust's current KV stack already explores a different axis that Mooncake does not make its central contribution:

- representation adaptation;
- two-level INT4 compression;
- independent K/V latent ranks;
- sparse residuals;
- F32/INT8/INT4 choices;
- strict budgeted planning;
- hysteresis;
- material HOT/WARM/COLD recompression;
- epoch-scoped learned bases.

Mooncake is substantially more mature on **physical/distributed adaptation**: remote residency, multi-node replication, RDMA transfer, topology and serving-scale scheduling.

The most interesting research direction is therefore the product space rather than choosing one design over the other.

## Elastic proposal: factorized KV state

```text
LogicalKvState =
    Representation
  × Residency
  × Redundancy
  × Persistence
  × RecomputationStatus
  × Version
```

Example:

```text
representation = latent(INT8 coefficients, INT4 residual)
semantic_tier  = WARM
residency      = remote DRAM
replicas       = 2
recomputable   = true
basis_epoch    = 17
```

This is an **ELASTIC PROPOSAL**, not a novelty claim.

## SciRust gap decision

No immediate SciRust gap is declared. RDMA, live distributed storage, replica movement and serving orchestration are primarily systems-runtime mechanisms. A scientific tool should enter SciRust only if the literature/experiments reveal a reusable mathematical primitive independent of a specific serving runtime.
