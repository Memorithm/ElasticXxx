# ElasticBitAllocation research program

Status: design-only / pre-implementation gate

This document defines the research and engineering preconditions for a future
ElasticBitAllocation capability across SciRust and ElasticXxx. It deliberately
does **not** define a new repository, allocator implementation, optimizer, or
claim that 0.5 bit/value has been achieved.

## 1. Scope and non-claim

The research target is adaptive physical representation: different logical
tensors, blocks, or representation components may use different mechanisms and
budgets. A future system may combine quantization, sparsity, shared structure,
low-rank factors, residual streams, entropy coding, and placement.

"0.5 bit/value" is only meaningful as an **aggregate effective storage rate**
over an explicitly declared accounting scope. It never means that each scalar
owns half of a physical bit. No result may use that phrase unless every byte of
payload, indices, scales, codebooks, metadata, padding, alignment and auxiliary
residual state in the declared scope is counted.

## 2. Current architecture audit

### 2.1 SciRust tensor Representation IR

The current `scirust-tensor-ir` representation layer already has the right
architectural separation: `TensorType` describes the logical value while
`RepresentationPlan` is a side table describing physical representation.
Representations form an acyclic declaration graph through typed
`RepresentationComponent` references. The plan supports deterministic
interning, exact integer `StorageBits`, node assignments, atomic re-planning,
and exact whole-graph aggregation for representation families whose
reconstruction contract is defined.

Current family status:

| Capability | Current status | Research implication |
|---|---|---|
| Heterogeneous per-component precision | Partial/usable | Components have independent tensor types and representations. This is sufficient structurally for mixed dtypes, but not packed arbitrary sub-bit streams. |
| Zero-bit/elided components | Missing | Needs an explicit reconstruction-preserving elision/implicit-value contract; zero storage must not mean "unknown". |
| Shared dictionaries/codebooks | Missing | Existing recursive accounting sums component storage per use. It has no physical ownership/share scope, so shared storage would be double-counted or ambiguously counted. |
| Low-rank factors | Supported | `Factorized { left, right }` is bindable when matrix contraction reconstructs the logical shape. |
| Sparse structures | Skeleton only | `Sparse { indices, values }` validates component dtypes but has no logical layout/format geometry, so it is intentionally not bindable. |
| Residual streams | Missing as a first-class family | Composition is extensible, but no reconstruction contract currently describes base + residual. |
| Entropy-coded streams | Missing | No packed-bitstream length, decoder contract, or exact stored-length semantics. |
| Metadata overhead | Not modeled generically | `StorageBits` explicitly requires physical metadata to be counted eventually, but current families do not expose accounting categories. |
| Exact physical StorageBits | Supported for dense/factorized; incomplete globally | Integer checked arithmetic is correct for implemented families. Quantized/sparse physical layouts remain undefined. |
| Alignment/padding overhead | Missing | Dense accounting is logical elements × dtype width; no serialized or resident allocation alignment is represented. |
| Mixed representations | Supported structurally | Different graph nodes can be rebound to different declared representations; composite families can depend on earlier representations. |
| Representation transitions | Supported across ecosystem | SciRust can atomically rebind plan assignments; ElasticXxx models representation state, version, epoch and `reinterpret`/`reencode`/`recompute` transitions. |
| Reconstruction/error contracts | Partial | Dense identity and factorized shape reconstruction exist. No generic numerical distortion/error contract exists for lossy representations. |

Conclusion: **do not build a new representation IR**. Extend SciRust's existing
representation layer only when a concrete benchmarked format requires semantics
that cannot be represented today. Keep transition/policy semantics in
ElasticXxx rather than duplicating them in SciRust.

### 2.2 ElasticXxx

ElasticXxx already owns the runtime/control-plane side of representation
change. `RepresentationState` carries a representation family, schema version
and materialization epoch. `RepresentationTransition` distinguishes
`Reinterpret`, `Reencode` and `Recompute`; capability checks and evidence-bound
attestations prevent silent contract changes. The representation bridge maps
this into the general elastic resource model and version frontier.

Therefore ElasticBitAllocation should eventually produce **candidate target
representation decisions and constraints**, then use the existing transition
machinery for validation/actuation. It must not create a second transition
state machine.

## 3. Normative metric: effective bits per logical value

Let an accounting scope `S` contain a set of logical values with total count

`N(S) = sum_i num_logical_values(i)`.

Let the exact physical stored bits in that same scope be decomposed as

`B(S) = B_payload + B_indices + B_scales + B_codebooks + B_metadata + B_padding + B_alignment + B_residual + B_aux`.

Define

`effective_bits_per_logical_value(S) = B(S) / N(S)`.

The canonical representation of this metric must be the exact rational pair
`(B(S), N(S))`. Decimal values such as `0.5` are derived presentation only.
No floating-point value is authoritative for acceptance tests.

### 3.1 Required accounting scopes

At least two scopes must be distinguished:

1. **Serialized storage scope**: exact bytes/bits of the persisted or transferred
   representation, including headers, tables, padding and final byte alignment.
2. **Resident execution scope**: exact bytes reserved/materialized by a target
   backend, including allocation alignment and any expanded lookup tables or
   auxiliary state required for execution.

A paper-style payload bit rate is allowed only as a diagnostic sub-metric and
must never be labeled effective bits/value when overhead is excluded.

### 3.2 Shared resources

A codebook/dictionary/shared factor must have explicit physical ownership. It is
counted once in a declared accounting scope and then referenced by consumers.
The denominator must cover exactly the logical values benefiting from that
shared resource. Cross-model or cross-request amortization is forbidden unless
that lifetime and scope are declared in the benchmark protocol.

### 3.3 Alignment

Alignment is a property of a materialization, not of the logical tensor. For a
segment with raw size `b` bits and required alignment `a` bits, the accounting
layer must record the actual padded segment size, not infer savings from raw
payload size. Backend-specific resident alignment and portable serialized
alignment are separate quantities.

## 4. Research baselines

The first experimental program must include fixed-format baselines before an
elastic allocator is implemented. Primary sources and reference implementations
should be pinned by commit when experiments start.

| Technique | Primary reference / verified implementation | Relevance |
|---|---|---|
| Ternary / 1.58-bit training | Ma et al., *The Era of 1-bit LLMs: All Large Language Models are in 1.58 Bits*, arXiv:2402.17764 | Ternary weights; important comparison against claims that heterogeneous compression is required for sub-2-bit rates. |
| Extremely low-bit comparison | Liu et al., *ParetoQ: Scaling Laws in Extremely Low-bit LLM Quantization*, arXiv:2502.02631 | Unified 1/1.58/2/3/4-bit comparison and evidence of a regime change below 3 bits. |
| 1-bit PTQ + residual structure | Huang et al., *BiLLM*, arXiv:2402.04291, github.com/Aaronhuang-778/BiLLM | Binary residual approximation and salient-weight separation; reports ~1.08-bit weights. |
| Second-order PTQ | Frantar et al., *GPTQ*, arXiv:2210.17323, github.com/IST-DASLab/gptq | Strong fixed low-bit PTQ baseline and Hessian-aware objective. |
| Activation-aware PTQ | Lin et al., *AWQ*, arXiv:2306.00978 | Saliency-aware scaling without mixed-precision storage. |
| Sparse pruning | Frantar & Alistarh, *SparseGPT*, arXiv:2301.00774, github.com/IST-DASLab/sparsegpt | Quantization-compatible sparsity baseline. |
| Dense + sparse outliers | Kim et al., *SqueezeLLM*, arXiv:2306.07629, github.com/SqueezeAILab/SqueezeLLM | Explicit heterogeneous dense/sparse representation and sensitivity-aware allocation. |
| Sparse-quantized outliers | Dettmers et al., *SpQR*, arXiv:2306.03078 | Higher-precision sparse outliers plus low-bit dense bulk. |
| Additive / multi-codebook quantization | Egiazarian et al., *Extreme Compression of Large Language Models via Additive Quantization*, arXiv:2401.06118, github.com/Vahe1994/AQLM | Codebook-sharing and vector/additive quantization baseline in the 2–4 bit regime. |
| Lattice/vector quantization | Tseng et al., *QuIP#*, arXiv:2402.04396, github.com/Cornell-RelaxML/quip-sharp | Vector codebooks and incoherence transforms. |
| Product quantization | Jégou, Douze & Schmid, IEEE TPAMI 33(1), 2011, DOI:10.1109/TPAMI.2010.57 | Canonical product-codebook construction. |
| Additive quantization | Babenko & Lempitsky, CVPR 2014, *Additive Quantization for Extreme Vector Compression* | Sum-of-codewords representation. |
| Quantized constants / double quantization | Dettmers et al., *QLoRA*, arXiv:2305.14314 | Demonstrates why scale/codebook overhead itself must be compressed and counted. |
| Pruning + sharing + entropy coding | Han, Mao & Dally, *Deep Compression*, arXiv:1510.00149 | Historical combined baseline for sparsity, shared weights and Huffman coding. |

The literature does **not** establish that a general 0.5-bit/value representation
is achievable for arbitrary LLM weights at acceptable quality and useful
latency. Any future result must be measured for a named workload and accounting
scope.

## 5. Elastic allocation problem

No optimizer is selected here. First define the feasible decision space.

For each allocation unit `i` (tensor, block, channel group, KV tile, or other
explicit unit), choose a representation candidate `r_i` from a finite,
validated candidate set `R_i`. A candidate carries exact measured or derived
properties under a named backend/workload:

- exact serialized `StorageBits`;
- exact resident `StorageBits`;
- reconstruction/error metrics;
- measured conversion cost;
- measured execution latency and memory traffic where an executable kernel
  exists;
- required capabilities and placement constraints;
- provenance for every measurement.

Candidate parameters may include precision, rank, sparsity pattern, residual
budget, codebook count/size, entropy coder, and placement. These are parameters
of a representation candidate, not free variables unless the representation
contract defines their legal domain.

### 5.1 Constraint forms

Examples, to be instantiated by the benchmark rather than baked into the IR:

`sum_i B_resident(i, r_i) <= memory_budget`

`sum_i B_serialized(i, r_i) <= storage_budget`

`quality_loss(r_1...r_n) <= epsilon`

`latency(r_1...r_n, backend) <= L_max`

`capabilities(backend) satisfy requirements(r_i)`

### 5.2 Objective forms

Valid studies may minimize quality loss subject to storage/latency budgets,
minimize storage under a quality bound, minimize latency under quality/storage
bounds, or compute a Pareto frontier. A scalar weighted objective is not the
default because arbitrary weights can hide regressions. The first allocator
study should therefore expose the Pareto set or use an explicit constrained
objective.

## 6. Minimal IR extensions known in advance

Do not implement these yet. They are the smallest semantic gaps identified by
the audit and should only be added when the reference workload exercises them.

1. **Physical segment accounting**: a representation must be able to report a
   breakdown of owned physical segments and exact stored/resident bits,
   including padding/alignment and metadata.
2. **Owned vs shared components**: references need ownership/share semantics so
   shared dictionaries/codebooks are counted exactly once per accounting scope.
3. **Packed bitstream component**: exact bit length plus serialized padding and
   decoder contract, independent of scalar `DType`.
4. **Explicit elision/implicit-value representation**: zero stored bits only
   when reconstruction semantics define the omitted value/structure.
5. **Reconstruction/error contract**: a representation family must state how a
   logical value is reconstructed and which error measurements are meaningful.
6. **Concrete quantized and sparse layouts**: promote the existing skeletons
   only through named, fully specified layouts rather than making the generic
   skeletons permissive.
7. **Residual composition**: base + residual as an explicit reconstruction
   family if the benchmark requires it.

These should be additive extensions to SciRust `RepresentationPlan`; ElasticXxx
continues to own selection policy and transitions.

## 7. Experimental design

### 7.1 Stage A: deterministic representation benchmark

Before a real-model allocator, construct a deterministic tensor/block corpus
from fixed seeds plus frozen real weight blocks when licensing permits. For each
block, evaluate a fixed set of representations with no elastic choice.

Required fixed baselines:

- dense FP32/BF16/FP16 as applicable;
- integer fixed-width quantization at 8/4/3/2 bits where a verified reference
  implementation exists;
- ternary/binary baseline where semantically applicable;
- at least one sparse baseline;
- at least one low-rank baseline;
- at least one codebook/vector-quantized baseline;
- at least one heterogeneous/residual baseline before claiming an elastic
  advantage.

Record exact serialized and resident storage, reconstruction MSE/relative error
and any task-independent operator error. This stage exists to validate the IR
and accounting, not to claim LLM quality.

### 7.2 Stage B: real workload

The first LLM study must use a frozen, redistributable model + tokenizer,
deterministic calibration/evaluation sets, and a baseline runtime capable of
running the original representation. Candidate ecosystem execution paths are
NNIS for native NVIDIA inference and, once its real-model integration exists,
SLHAv2 for KV-cache experiments. FLAT-ATTENTION can supply attention execution
measurements when the representation has a supported kernel.

Weight-allocation and KV-cache-allocation are separate studies and must not
share a bits/value claim unless their denominators and lifetimes are explicitly
combined.

For language models measure:

- exact storage and resident bits/value;
- reconstruction metrics by tensor/block;
- perplexity or task loss on frozen evaluation data;
- end-to-end tokens/s and latency distributions;
- bytes moved / memory-bandwidth counters where available;
- one-time and amortized conversion cost;
- peak temporary memory during conversion/materialization.

### 7.3 Fairness rules

- Same model checkpoint, tokenizer, prompts/dataset and decoding settings.
- Same hardware, software commit, power/performance policy and warm-up rules.
- Fixed representations are tuned with no less calibration opportunity than the
  elastic method.
- Include all metadata and shared tables in the same accounting scope.
- Report both quality at equal storage and storage at equal quality when
  possible.
- Report conversion/setup separately from steady-state execution, then provide
  a declared amortization horizon rather than hiding conversion cost.
- Never infer latency from bit rate; measure it.

## 8. Acceptance metrics and go/no-go gate

Implementation of an allocator remains **NO-GO** until all five conditions are
met:

1. **IR expressibility** — each candidate format in the first experiment has a
   precise representation contract in the existing SciRust IR or a reviewed,
   minimal extension design.
2. **Exact accounting** — serialized and resident storage can be computed from
   owned physical segments with no uncounted metadata, sharing, padding or
   alignment.
3. **Reference workload** — a deterministic runnable workload and frozen input
   corpus/checkpoint exist.
4. **Baselines** — at least dense + fixed low-bit + one structural compression
   baseline run under the same protocol.
5. **Acceptance thresholds** — the experiment states quality and resource
   thresholds before elastic search begins.

Suggested first acceptance test for the *research harness*, not for a claimed
0.5-bit result:

- accounting oracle agrees bit-for-bit with serialized artifact length;
- resident accounting agrees with measured allocation sizes for the selected
  backend;
- reconstruction is deterministic for a frozen candidate;
- fixed baselines reproduce within declared numerical tolerances;
- elastic selection is compared against the fixed baseline Pareto frontier and
  only considered useful if it produces a non-dominated point after conversion
  cost and all overhead are included.

A target such as <= 0.5 effective bits/value must **not** be an acceptance
criterion for the infrastructure. It may later be a research hypothesis.

## 9. Concrete implementation proposal after GO

When the gate turns green, use three narrow layers instead of a new project:

1. **SciRust representation semantics** — additive concrete representation
   families/segments, exact physical accounting and reconstruction contracts.
2. **ElasticXxx candidate model + policy** — consume candidate measurements,
   constraints and exact storage; produce an auditable representation plan;
   reuse existing `RepresentationTransition` machinery to enact changes.
3. **Backend adapters / benchmark harness** — NNIS, FLAT-ATTENTION, SLHAv2 or CPU
   reference implementations materialize candidates and return measured
   evidence. Backend support is capability-driven and never assumed from the IR.

The first optimizer should be chosen only after the candidate-space structure
is measured. For a small finite candidate set, exhaustive/Pareto dynamic
programming or constrained enumeration is preferable because it supplies an
oracle for later heuristics. Bayesian, evolutionary, greedy or learned
allocation should only be introduced when the exact finite baseline becomes
computationally inadequate.

## 10. Current decision

**NO-GO for allocator implementation.**

The ecosystem has strong prerequisites already: a compositional SciRust
representation side table, exact integer accounting for implemented families,
atomic re-planning, and ElasticXxx representation transitions. But exact shared
storage, packed bitstreams, zero-bit reconstruction, metadata/alignment
accounting, concrete quantized/sparse layouts and generic error contracts are
not yet complete. A fair first reference experiment and pre-registered
acceptance thresholds also need to be frozen before search is implemented.

The next engineering change should therefore be driven by the first benchmark
format that crosses one of these semantic gaps, not by a speculative
ElasticBitAllocation optimizer or a repository advertising a target bit rate.
