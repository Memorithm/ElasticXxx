# IMPRESS: Importance-Informed Multi-Tier Prefix KV Storage

**Paper:** Weijian Chen, Shuibing He, Haoyang Qu, Ruidong Zhang, Siling Yang, Ping Chen, Yi Zheng, Baoxing Huai, Gang Chen. *IMPRESS: An Importance-Informed Multi-Tier Prefix KV Storage System for Large Language Model Inference*. FAST 2025.

**Primary source:** https://www.usenix.org/system/files/fast25-chen-weijian-impress.pdf

## Problem

**SOURCE-DERIVED.** Reusable prefix KV state can reduce recomputation, but when prefix KVs spill beyond CPU memory to SSD, disk→CPU→GPU I/O can dominate TTFT. Loading every stored prefix KV can be more expensive than recomputation.

## Importance-guided selection

**SOURCE-DERIVED.** IMPRESS exploits the observation that important-token index sets can be similar across attention heads within a layer. It uses a small set of probe heads to approximate important token sets for remaining heads when measured similarity exceeds a threshold. This reduces the number of keys that must be loaded merely to identify which KV entries matter.

The paper reports high similarity in evaluated settings but explicitly does not claim a mathematical proof that the observation holds universally; it validates practicality across multiple models/datasets.

## Quality / I/O guard

**SOURCE-DERIVED.** The similarity threshold controls a quality-versus-I/O trade-off. The paper gives an example where reducing the threshold cuts loaded keys by 4× while accuracy decreases from 79.1% to 77.5%, illustrating that more aggressive selection is a semantic-quality decision rather than a free systems optimization.

## Multi-tier cache policy

**SOURCE-DERIVED.** IMPRESS stores prefix KV across GPU, CPU and disk. After a disk chunk is loaded into CPU memory, only important K/V vectors are sent to GPU. Its cache-admission score combines request/access frequency with the fraction of important K/V vectors in a chunk. The importance ratio is maintained as an online moving average.

GPU and CPU cache replacement are managed with score-ordered structures; disk retains replicas. Depending on score and current residency, a chunk can be promoted to GPU, remain in CPU, or remain on disk.

## Results

**SOURCE-DERIVED.** IMPRESS reports up to 3.8× lower I/O time and up to 2.8× lower TTFT than compared prefix-KV storage systems, with less than 0.2% inference-accuracy loss in the reported evaluation.

## Elastic relation

- importance rather than frequency alone: **ADOPT / GENERALIZE**;
- partial/subresource migration: **ADOPT**;
- explicit quality/I/O trade-off: **ADOPT but place behind SemanticContract**;
- dynamic online score: **ADOPT principle**;
- multi-tier residency: **ADOPT**;
- probe-head similarity heuristic: **ADAPT / domain-specific**;
- fixed score `frequency × importance_ratio`: **INVESTIGATE**, not universal.

## Relation to prior Elastic lessons

IMPRESS independently reinforces lessons from Pollux and *Tiered Memory Management Beyond Hotness*:

```text
raw utilization / frequency / hotness
    !=
useful performance impact
```

A planner should optimize expected useful progress or semantic value under resource costs rather than blindly rank by access count.

## Comparison with SciRust

SciRust already provides:

- representation-adaptive KV storage;
- deterministic compressed / latent variants;
- HOT/WARM/COLD material recompression;
- strict memory budgets and hysteresis;
- general EWMA capability elsewhere in SciRust;
- during this literature pass, a new exact deterministic `budgeted_selection` solver in `scirust-solvers`.

IMPRESS adds a system-specific importance-identification mechanism and physical GPU/CPU/disk placement. These should not be copied wholesale into SciRust.

The new general SciRust budgeted-selection primitive is nevertheless directly useful for scientific experiments such as:

```text
item    = candidate token/chunk/subresource
cost    = bytes or predicted transfer cost
utility = fixed-point predicted semantic/performance value
budget  = VRAM / bandwidth / I/O budget
```

This can benchmark exact additive selection against IMPRESS-like heuristics on tractable instances without making SciRust an LLM runtime.

## Experiment required

Compare:

1. frequency-only admission;
2. IMPRESS-style frequency × importance heuristic;
3. exact additive budgeted selection with calibrated fixed-point utility;
4. representation-aware budgeted selection where each logical chunk has multiple representation candidates;
5. joint selection + residency under measured I/O cost.

Measure quality, TTFT, I/O bytes, transfer count, solver overhead, cache hit rate, prediction error and sensitivity to utility-model misspecification.
