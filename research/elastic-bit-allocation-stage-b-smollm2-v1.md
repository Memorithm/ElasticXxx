# ElasticBitAllocation Stage B — SmolLM2 fixed-baseline preregistration v1

Status: preregistered / not executed

This document freezes the first real-model measurement protocol for
ElasticBitAllocation issue #29. It does not authorize an elastic allocator,
search, final-test execution, a performance claim, or a low-bit production
runtime.

The machine-readable source of truth is:

`research/elastic-bit-allocation-stage-b-smollm2-v1.json`

CI validates that manifest with:

`tools/validate_elastic_bit_stage_b.py`

## 1. Research question

Before implementing elastic allocation, measure fixed representation baselines
for one exact real model under one exact data and runtime protocol. The purpose
is to learn the actual quality/storage/runtime trade-off surface without changing
the protocol after seeing an elastic result.

Stage A already proves that the local accounting harness can represent and
freeze deterministic synthetic dense, low-bit, sparse, low-rank, codebook and
residual candidates. Stage B asks a different question: which fixed candidates
can be implemented, exactly accounted and executed on the pinned real-model
workload?

## 2. Frozen model identity

The model is the same SmolLM2-135M checkpoint already used by NNIS qualification:

- repository: `HuggingFaceTB/SmolLM2-135M`;
- revision: `93efa2f097d58c2a74874c7e644dbc9b0cee75a2`;
- source model SHA-256:
  `80521b40281d6ce74e35c9282c22539e75aa0ac8578892b2a59955ef78d55da1`;
- source weights: BF16;
- tokenizer SHA-256:
  `9ca9acddb6525a194ec8ac7a87f24fbba7232a9a15ffa1af0c1224fcd888e47c`;
- declared model license: Apache-2.0.

A candidate is incomparable if any of these identities differ unless a new
protocol version explicitly reopens the workload definition.

## 3. Frozen data identity and leakage boundary

Dataset:

- repository: `Salesforce/wikitext`;
- revision: `b08601e04326c79dfdd32d625aee71d232d685c3`;
- config: `wikitext-2-raw-v1`;
- declared repository license metadata: `cc-by-sa-3.0`, `gfdl`.

The exact train, validation and test parquet SHA-256 values are frozen in the
JSON manifest. License strings are retained as provenance metadata and are not
a legal interpretation by ElasticXxx.

The splits have non-overlapping roles:

1. **Calibration — train.** Concatenate exact UTF-8 `text` values in parquet row
   order with no inserted separator and no text normalization. Tokenize with the
   pinned tokenizer, `add_special_tokens=false`. Use exactly the first 262,144
   token IDs, partitioned into 128 non-overlapping blocks of 2,048 tokens.
2. **Development — validation.** Apply the same exact concatenation and
   tokenization, partition into all complete non-overlapping 2,048-token blocks,
   and drop only the final incomplete block. Candidate fitting is forbidden.
   Fixed-baseline evidence from this split may later be used to choose acceptance
   thresholds.
3. **Final test — test.** Same deterministic block construction, but final-test
   execution is locked in preregistration v1. It cannot be used for candidate
   fitting or threshold selection.

A separate follow-up threshold record must freeze all acceptance values before
final-test execution is authorized. Changing a candidate, metric, dataset rule,
threshold or workload after final-test access invalidates that confirmatory run.

## 4. Quality metric

Primary quality metric: mean next-token negative log likelihood.

For each complete 2,048-token block, predict target positions 1..2047 from the
preceding tokens in the same block. Aggregate the loss over every scored target
token, not by averaging per-block means. Perplexity is only the derived
`exp(mean NLL)` presentation metric.

The dense reference is mandatory. A fixed representation cannot be admitted if
its reconstruction/materialization semantics are not deterministic and tied to
the exact checkpoint identity.

## 5. NNIS runtime and physical target

Runtime owner: `Memorithm/NNIS` at revision
`b9d2f1e74bb68dfa90ca499a24ce857d15e7fb02`.

The preregistration deliberately reuses NNIS-owned model/runtime semantics
instead of copying them into ElasticXxx. Existing relevant NNIS surfaces are:

- `crates/nnis-bench/examples/smollm2_e2e.rs` for real end-to-end inference;
- `crates/nnis-bench/examples/smollm2_lm_head_weight_representation.rs` for the
  existing F32/BF16 LM-head representation probe.

The target device is NVIDIA Jetson AGX Thor, device ordinal 0, power mode MAXN.
Every compared candidate must use the same physical device and campaign-level
power/clock/thermal policy. `nvpmodel`, `jetson_clocks --show`, CUDA driver,
NVRTC and CUDA identity must be retained in evidence.

The existing NNIS audit does **not** establish a qualified full-model Int4,
Int2-or-lower, sparse, low-rank, codebook or heterogeneous-residual runtime.
Those capabilities remain missing until their owning backend implements and
qualifies them. Missing capability is a blocked candidate, never a zero-cost or
zero-latency observation.

## 6. Performance protocol

The fixed performance probe reuses the existing NNIS SmolLM2 workload:

- prompt: `Gravity is`;
- prompt IDs: `[22007, 6463, 314]`;
- greedy decoding;
- 32 generated steps;
- model loading excluded from `request_total`;
- fresh session per measured request;
- 2 warmups per process;
- 5 measured requests per process;
- 5 independent processes.

Primary latency is `request_total_ms`. Report median and p95 over the measured
requests. Throughput is derived from generated tokens divided by request-total
time under the same definition.

Conversion/materialization time is separate and mandatory. Peak temporary memory
for conversion/materialization and execution is separate and mandatory. Memory
traffic counters are recorded when supported; unsupported counters must remain
explicitly unsupported rather than becoming zero.

No latency may be inferred from a storage bit rate.

## 7. Exact weight-representation accounting

Stage B is a **weight** representation experiment. KV cache remains a separate
experiment with a different lifetime and denominator.

The logical denominator is the count of unique logical checkpoint parameter
values after resolving declared weight ties once. This denominator is fixed by
the source checkpoint, not by a candidate's physical materialization.

Serialized accounting includes every representation-owned weight payload,
index, scale, codebook, header/metadata, padding, alignment, residual and
auxiliary state needed to reconstruct the weights.

Resident accounting includes every backend-owned runtime weight segment and
representation auxiliary allocation with its exact physical allocation size.
Shared physical segments are counted once by canonical segment identity.

Observational `cuMemGetInfo` free-memory deltas may be retained as runtime
telemetry, but they are not exact ownership/accounting evidence. A candidate
without exact serialized **and** resident accounting is inadmissible for the
allocator gate.

Tokenizer files, runtime binaries, activations and KV cache are outside the
weight bits/value numerator and must be reported separately when relevant.

## 8. Fixed baseline gate before any allocator

Before allocator/search work can become GO, the real-model campaign must contain
at least:

- the dense reference;
- one verified fixed 4-bit baseline;
- one verified fixed <=2-bit baseline;
- at least one structural baseline from sparse, low-rank, codebook, or
  heterogeneous/residual representation.

Every baseline gets the same checkpoint, tokenizer, data partitions, hardware,
runtime measurement protocol and calibration opportunity permitted by this
preregistration.

Stage-A synthetic candidates do not satisfy this real-model gate by themselves.
An isolated LM-head BF16 experiment also does not satisfy the full-model low-bit
or structural gate.

## 9. Acceptance thresholds are intentionally absent

All five acceptance-threshold fields in preregistration v1 are `null`:

- maximum allowed quality degradation;
- minimum physical-storage improvement;
- maximum steady-state latency regression;
- maximum conversion/setup cost or amortization horizon;
- reproducibility tolerance/run-count policy.

This is deliberate. The values must be derived from fixed dense/fixed baseline
evidence on the development split and frozen in a new follow-up record **before**
the final test is executed and before an elastic allocator/search implementation
is authorized.

The validator rejects a numeric threshold inserted into v1, an unlocked final
test, a fabricated low-bit qualification, or premature allocator authorization.

## 10. Current stop condition

This protocol slice is complete when its exact PR head passes normal ElasticXxx
CI and the fail-closed preregistration validator.

After merge, the next engineering task is not the allocator. It is to close the
first missing real representation-baseline capability and exact resident
accounting gap in its owning backend, then collect fixed development evidence.
