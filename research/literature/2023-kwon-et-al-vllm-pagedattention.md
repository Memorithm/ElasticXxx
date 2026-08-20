# vLLM / PagedAttention

**Paper:** Woosuk Kwon, Zhuohan Li, Siyuan Zhuang, Ying Sheng, Lianmin Zheng, Cody Hao Yu, Joseph E. Gonzalez, Hao Zhang, Ion Stoica. *Efficient Memory Management for Large Language Model Serving with PagedAttention*. SOSP 2023 / arXiv:2309.06180.

**Primary source:** https://arxiv.org/pdf/2309.06180

## Problem

**SOURCE-DERIVED.** Autoregressive LLM serving suffers from large, dynamically growing KV caches. Contiguous per-request allocation causes internal/external fragmentation, limits batching, and prevents flexible KV sharing.

## Mechanism

**SOURCE-DERIVED.** PagedAttention separates logical KV blocks from physical KV blocks. A per-sequence block table maps logical positions to physical blocks. Physical blocks need not be contiguous and are allocated on demand. The same abstraction enables block-granularity sharing between sequences/requests.

vLLM couples this storage mechanism to continuous batching and request scheduling. Under memory pressure it can preempt requests and recover either by recomputing KV state or swapping KV blocks through CPU memory.

## Transition choice: recompute versus swap

**SOURCE-DERIVED.** The paper explicitly compares recomputation and CPU/GPU swapping. Small block sizes cause many small transfers and poor effective PCIe bandwidth, making recomputation preferable; swapping becomes preferable at larger block sizes, while medium sizes can be comparable. The important mechanism is therefore not a universal `SWAP` rule but choosing between alternative recovery transitions according to transition cost.

## Elastic relation

- logical/physical indirection: **ADOPT / GENERALIZE**;
- block-granularity resource state: **ADOPT**;
- sharing/reference-counted physical state: **ADOPT / GENERALIZE**;
- swap versus recompute as alternative transitions: **ADOPT principle**;
- fixed LLM-specific block model: **ADAPT**;
- preemption directly coupled to engine policy: **ADAPT into planner / validator / actuator separation**.

**KEY LESSON.** A logical resource identity need not determine physical layout. `IDENTITY`, `RESIDENCY`, and physical allocation should remain separate concepts.

## SciRust comparison

**CURRENT REPOSITORY EVIDENCE.** SciRust already contains `PagedKvCache`, explicitly based on PagedAttention, with logical-to-physical block indirection and tests asserting bit-identical attention versus a contiguous reference under fragmented physical placement. It also exposes this implementation through the common `AttentionBackend` interface.

Therefore PagedAttention itself does **not** establish a SciRust gap.

SciRust additionally contains representation-adaptive KV mechanisms that are outside vLLM's central contribution: compressed `ElasticKvCache`, adaptive latent K/V representation planning, and HOT/WARM/COLD recompression tiers.

## Open question for ElasticXxx

Can a generic logical-resource abstraction expose paging/sharing/recomputation without hard-coding KV-cache semantics, while retaining enough locality information for low-overhead execution?

## Experiment

For a KV-domain prototype, compare `KEEP`, `RECOMPRESS`, `MIGRATE`, `EVICT+RECOMPUTE`, and `SWAP` using measured transfer/recompute costs. Test whether a generic Elastic cost model chooses the same transition regimes as specialized vLLM heuristics without adding material planner overhead.
