# Llumnix: Dynamic Scheduling for Large Language Model Serving

**Paper:** Biao Sun, Ziming Huang, Hanyu Zhao, Wencong Xiao, Xinyi Zhang, Yong Li, Wei Lin. *Llumnix: Dynamic Scheduling for Large Language Model Serving*. OSDI 2024.

**Primary source:** https://www.usenix.org/system/files/osdi24-sun-biao.pdf

## Problem

**SOURCE-DERIVED.** LLM requests have heterogeneous and unpredictable input/output lengths, latency requirements, execution times, and growing KV-cache footprints. One-shot dispatch creates load imbalance, memory fragmentation, preemptions, and difficulty enforcing priorities/SLOs.

## Core mechanism

**SOURCE-DERIVED.** Llumnix introduces runtime request rescheduling across model instances and realizes it through live migration of the request together with its in-memory KV state.

The key observation is that the KV cache is append-only during decode: previously generated KV blocks remain unchanged while only newly generated tokens append new blocks. Llumnix exploits this property to pipeline copying of old KV blocks with continued decoding on the source. Migration proceeds in multiple stages; only the final remainder requires a short suspension. The paper reports downtime that is near-zero and effectively independent of total sequence length because only the final iteration's newly generated state must be copied while suspended.

## Migration protocol

**SOURCE-DERIVED.** Migration is a coordinated protocol, not a blind copy:

1. source pre-announces the number of blocks;
2. destination attempts to allocate/reserve them;
3. destination returns proceed/abort;
4. after each stage the source rechecks whether the request completed or was preempted;
5. either side aborts on peer failure;
6. on success the source releases local blocks;
7. destination commits and resumes execution.

This is strong evidence for `PREPARE / RESERVE / COPY / RECHECK / COMMIT-or-ABORT` transition semantics.

## Scheduling architecture

**SOURCE-DERIVED.** Llumnix separates a cluster-level global scheduler from instance-level local schedulers/migration coordinators. The global scheduler uses aggregate load information and chooses source/destination instance pairs, while local components choose concrete requests and execute migration.

The scheduling policy introduces **virtual usage**, allowing several goals—load balancing, de-fragmentation, prioritization, and autoscaling—to be expressed through modified resource-usage values and handled by a common load-balancing mechanism.

## Results

**SOURCE-DERIVED.** The paper reports up to 14.8× improvement in P99 latency in one evaluated setting, up to 1.5× acceleration for high-priority requests, and about 36% cost saving while maintaining similar P99 prefill latency in the autoscaling experiment.

## Elastic relation

- append-only-state-aware live migration: **ADOPT / GENERALIZE**;
- preallocation/reservation before transfer: **ADOPT**;
- commit/abort handshake: **ADOPT**;
- overlapping transfer with useful execution: **ADOPT / GENERALIZE**;
- global/local scheduler split: **ADOPT principle / ADAPT topology**;
- virtual usage as a unifying heuristic: **INVESTIGATE / ADAPT**;
- LLM-specific append-only assumptions: **REJECT as universal assumption**.

## Comparison with SciRust

SciRust already contains rich local/adaptive KV representation machinery, but this review did not establish an equivalent cross-instance live migration protocol for an actively decoding request. That absence is not automatically a SciRust gap: distributed live migration is primarily a systems-runtime mechanism.

## Elastic hypothesis

A legal `MIGRATE` transition should be able to expose overlap opportunities and consistency properties:

```text
source_mutability = APPEND_ONLY
stable_prefix      = migratable concurrently
mutable_tail       = final synchronization set
```

This could permit a generic runtime to derive a staged migration protocol for resources whose mutation semantics are known.

**OPEN QUESTION:** how much of such a protocol can be generalized beyond append-only KV state without domain-specific adapters?
