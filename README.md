# ElasticXxx

**ElasticXxx** is a research project exploring a general, type-safe programming model and runtime for **adaptive computational resources**, initially embedded in Rust and incubated through SLHAv2.

The project asks a deliberately broad systems/programming-languages question:

> Can heterogeneous computational resources be represented as constrained adaptive state spaces, so that a runtime may change resource quantity, placement, representation, parallelism, and related properties while preserving explicit program invariants?

ElasticXxx is currently in the **research and specification phase**. No claim of scientific novelty should be inferred before the literature review, formal model, implementation, and experimental evaluation are complete.

## Core idea

Application code should primarily describe:

- what must remain true (**invariants / semantic contracts**),
- what may change (**elastic dimensions**),
- what the application wants to optimize (**objectives**).

The Elastic runtime is responsible for selecting and applying admissible resource-management mechanisms.

A provisional control loop is:

```text
OBSERVE
   ↓
FORECAST
   ↓
PLAN
   ↓
VALIDATE
   ↓
ACT
   ↓
VERIFY
   ↓
COMMIT / ROLLBACK
```

## Provisional model

An Elastic Resource is currently modeled as:

```text
R = (K, S, D, T, I, M)
```

where:

- `K` — resource semantics / kind and capabilities,
- `S` — admissible state space,
- `D` — elastic dimensions,
- `T` — legal transitions,
- `I` — invariants that must be preserved,
- `M` — observations and cost model.

The model is intentionally provisional and will evolve as the literature review and implementation progress.

## Research documents

- [Research White Paper v0.1](docs/whitepaper/elastic-resources-whitepaper-v0.1.md)
- [Literature review](research/literature/README.md)
- [Moreau & Queinnec (2005) — Resource Aware Programming](research/literature/2005-moreau-queinnec-resource-aware-programming.md)
- [Rust Surface Model v0.1](docs/surface/rust-surface-model-v0.1.md) — the first user-facing typed declaration layer (`elastic_core::resource`)

## Research method

Prior mechanisms are classified as:

- **ADOPT** — retain substantially unchanged;
- **ADAPT** — retain the principle but generalize or alter the mechanism;
- **REJECT** — incompatible with the emerging Elastic model or unnecessary;
- **INVESTIGATE** — promising, but requiring formal or experimental comparison.

The goal is not to differ from prior work for its own sake. Where an existing mechanism is stronger, ElasticXxx should reuse it and cite it.

## Scope

Candidate elastic domains include memory, compute, accelerators, concurrency, batching, scheduling, storage, I/O, networking, locality, bandwidth, caches, context/KV-cache management, model placement, routing, representation, precision, parallelism, replication, recomputation, checkpointing, energy, thermal constraints, distributed resources, and agent/LLM execution.

Not everything is elastic. Correctness, memory safety, type safety, authorization, cryptographic guarantees, and explicitly declared semantic invariants constrain adaptation rather than being silently weakened by it.

## Incubation

SLHAv2 is intended to be the first demanding reference environment. The core Elastic resource model itself should remain independent of LLM-specific assumptions, CUDA, or any single hardware topology.

## Status

Research prototype / specification work in progress.
