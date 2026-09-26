# Elastic multi-lane word width v1

Status: research contract, no performance claim.

## Problem

Some control-plane representations need more metadata than a fixed 64-bit word
can carry, while hot paths should not pay permanently for a 256-, 512-, 1024-
or 2048-bit descriptor when a narrower representation is sufficient.

The v1 contract therefore treats **width as elastic representation state**. It
does not introduce six independent descriptor object models.

## Representation

The physical backing unit is always a native Rust `u64` lane.

| Logical width | Native lanes | Bytes/word |
| ---: | ---: | ---: |
| 64 | 1 | 8 |
| 128 | 2 | 16 |
| 256 | 4 | 32 |
| 512 | 8 | 64 |
| 1024 | 16 | 128 |
| 2048 | 32 | 256 |

A contiguous plane stores the width once and lays logical words back-to-back in
one flat `Vec<u64>`. No per-word enum/tag is required.

```text
plane width = 256

[u64 u64 u64 u64][u64 u64 u64 u64][u64 u64 u64 u64]...
\_____ word 0 ____/\_____ word 1 ____/\_____ word 2 ____/
```

The v1 width set is deliberately bounded and power-of-two. This is a research
envelope, not a claim that 256+ bit values are native scalar integer operations
on every CPU or accelerator.

## Transition semantics

Width belongs to representation state and therefore changes only through an
explicit transition.

- same-width reinterpretation is structurally admissible;
- a width change changes physical stride and cannot be `Reinterpret`;
- width changes require `Reencode` or `Recompute` plus the caller's normal
  capability, invariant, actuation, verification and rollback contracts;
- the generic layer does not assign KV, attention, codec, routing or provenance
  semantics to any lane.

The reference repacker is intentionally conservative. Expansion preserves low
lanes and zero-fills new high lanes. Contraction succeeds only when all
discarded high lanes are already zero. Domain adapters may define stronger
materialization rules, but those rules remain outside the generic contract.

## ElasticXxx lifecycle

```text
OBSERVE
   ↓
FORECAST
   ↓
PLAN         select admissible word width
   ↓
VALIDATE     representation + domain invariants
   ↓
ACT          trusted backend materializes target width
   ↓
VERIFY       read back / re-check authoritative state
   ↓
COMMIT
   ↘ ROLLBACK on failed verification
```

A future KV adapter can therefore expose 64/128/256/512/1024/2048-bit
candidates without moving KV semantics into ElasticXxx.

## Crate boundary

The generic width type lives in `elastic-core`. `elastic-kv` re-exports the
contract for compatibility but does not own it. This keeps consumers that only
need flat lane/width semantics free from runtime, serde and KV-specific
dependencies.

## Repository boundaries

- **ElasticXxx / `elastic-core`** owns the dependency-free generic width state;
  higher layers own planning/validation composition and transaction/evidence
  contracts.
- **KVLab** owns controlled experiments that determine when width adaptation is
  scientifically useful for KV control/index structures.
- **FLAT-ATTENTION** owns attention/paged-KV execution and measured kernel costs.
- **NNIS** owns NVIDIA-native realization/measurement.
- **SciRust** may own reusable exact packed-bit/math primitives once independent
  consumers justify promotion.
- **SLHAv2** remains a demanding real KV elasticity consumer and owns its tile
  and quality semantics.

No speedup, cache-hit, bandwidth, TTFT, TPOT, memory-saving or model-quality
claim follows from this structural contract.
