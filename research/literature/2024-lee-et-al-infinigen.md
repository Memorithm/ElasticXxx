# InfiniGen: Dynamic KV Cache Management

**Paper:** Wonbeom Lee, Jungi Lee, Junghwan Seo, Jaewoong Sim. *InfiniGen: Efficient Generative Inference of Large Language Models with Dynamic KV Cache Management*. OSDI 2024.

**Primary source:** https://www.usenix.org/system/files/osdi24-lee.pdf

## Problem

**SOURCE-DERIVED.** CPU-offloaded KV caches permit longer contexts but make CPU→GPU KV transfer a major bottleneck. Fetching the full cache every decoding layer wastes bandwidth because only a subset of tokens contributes materially to the next attention computation.

## Mechanism

**SOURCE-DERIVED.** InfiniGen predicts important KV entries for the next attention layer by performing a small rehearsal in the preceding layer. It transforms query/key matrices offline using SVD-derived structure so a small subset of channels can predict important attention entries efficiently. At runtime it keeps the KV pool in CPU memory, speculates the critical entries, prefetches only those entries to GPU, and dynamically removes infrequently used entries.

The paper calls the short-lived selection **ephemeral pruning**: the retained subset for GPU attention is selected dynamically rather than permanently defining the stored global KV representation.

## Objective / trade-off

**SOURCE-DERIVED.** InfiniGen trades prediction work and potentially omitted attention entries against reduced host→device transfer. The authors report up to 3.00× speedup over compared KV-management methods and up to a 32.6 percentage-point accuracy improvement over selected prior methods in their evaluation.

## Elastic relation

- importance-aware selective prefetch: **ADOPT / GENERALIZE**;
- dynamic action intensity (`how much KV to move`): **ADOPT principle**;
- use predicted future access rather than current hotness alone: **ADOPT principle**;
- model-specific weight transformation/SVD scheme: **ADAPT / domain-specific**;
- dropping noncritical entries without external semantic contract: **ADAPT**.

**KEY LESSON.** Residency need not be selected for an entire logical object uniformly. A resource may expose a set of subobjects with different predicted future utility, allowing partial migration/prefetch.

## Comparison with SciRust

**CURRENT REPOSITORY EVIDENCE.** SciRust already explores a different, richer representation axis: two-level INT4 KV compression, grouped adaptive scaling, budget-bounded storage, latent K/V ranks, sparse residual slots, F32/INT8/INT4 format selection, hysteresis, and material HOT/WARM/COLD recompression.

InfiniGen contributes something different: **dynamic token-entry selection and selective physical prefetch from host memory to GPU**. The current inspection has not established that SciRust contains an equivalent importance-prediction / partial-prefetch mechanism.

This is not yet a SciRust gap. The mechanism may belong primarily to an inference runtime, while the scientific questions—importance prediction, uncertainty, utility estimation, calibration—can already be studied with existing mathematical/learning tooling.

## Elastic hypothesis

A future KV resource state may separate:

```text
Representation(KV)
Residency(KV subset)
PredictedUtility(KV subset, horizon)
```

This would allow the planner to choose between compressing a large fraction, migrating a critical subset, recomputing, or doing nothing.

## Experiment

Compare under the same memory/network budget:

1. representation-only adaptation;
2. selective-prefetch-only adaptation;
3. combined representation + selective prefetch;
4. full-cache migration baseline.

Measure model quality, TTFT/TBT, bytes transferred, GPU memory, CPU memory, planner/prediction overhead, and prediction error.
