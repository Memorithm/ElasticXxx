# ElasticXxx Agent Bootstrap Contract

This repository has a persistent agent-oriented ecosystem roadmap that is intentionally kept off the default branch.

## Mandatory first step

Before any autonomous coding, runtime-state-machine work, resource-policy change, representation/kernel integration, architectural decision, PR creation, or merge decision, read:

`origin/agent/ecosystem-roadmap:.agent/ELASTICXXX_ECOSYSTEM_ROADMAP.yaml`

Recommended command:

```bash
git fetch origin agent/ecosystem-roadmap && \
git show origin/agent/ecosystem-roadmap:.agent/ELASTICXXX_ECOSYSTEM_ROADMAP.yaml
```

For any ML resource, representation, precision, device, batching, memory, kernel, KV, benchmark, or cross-repository ML work, also read:

`origin/agent/ecosystem-roadmap:.agent/ML_MATURITY_5_OF_5.yaml`

```bash
git fetch origin agent/ecosystem-roadmap && \
git show origin/agent/ecosystem-roadmap:.agent/ML_MATURITY_5_OF_5.yaml
```

The ML overlay makes 5/5 an evidence-backed exit criterion. Elastic adaptation is mature only when the full control loop executes against real ML workloads, every actuation is verified and rollback-capable, and measured policy outcomes are compared with static baselines under declared objectives.

If the roadmap or applicable ML overlay cannot be fetched or read, fail closed for major architecture, runtime-state-machine, representation-format, cross-repository integration, or merge decisions. Read-only diagnosis is allowed.

## Mandatory reread points

Reread the roadmap and, for ML work, the ML overlay:

1. at the start of every agent session;
2. before selecting the next major runtime phase;
3. before any cross-repository integration;
4. after any user instruction that changes resource semantics, invariants, objectives, ecosystem role, or ML maturity priorities;
5. before opening or merging runtime, representation, kernel, adapter, or contract PRs.

## Repository role

ElasticXxx owns the generic adaptive resource runtime and its typed control loop:

`OBSERVE -> FORECAST -> PLAN -> VALIDATE -> ACT -> VERIFY -> COMMIT / ROLLBACK`

Do not rewrite existing foundations merely to create activity. Build on the existing `elastic-core`, `elastic-eir`, `elastic-macros`, facade, adapters, KV, kernel, downstream guards, validation, actuation, representation, and rollback concepts unless a concrete contract gap is demonstrated.

ElasticXxx must not absorb domain semantics owned by SciRust, SLHAv2, FLAT-ATTENTION, NNIS, Forge, SciRust Hub, SciCapsule, SciRust-Verify, or scientific research repositories.

## Core constraints

- correctness and declared semantic invariants dominate optimization;
- no fabricated performance or scientific novelty;
- forecasts are advisory and may never override hard invariants;
- every physical actuation must pass validation immediately before application;
- failed post-actuation verification must rollback or fail closed explicitly;
- external candidates and measurements require compatible identities/fingerprints and independent invariant revalidation;
- kernel elasticity and physical representation elasticity are separate axes;
- required CI must be green on the exact PR head before merge;
- planned adaptive dimensions, proxy metrics, or unverified forecast quality never count as 5/5 maturity.

## Mandatory roadmap maintenance

Update the off-main roadmap and ML overlay when applicable when:

- a phase or ML maturity phase changes status;
- an ecosystem contract is published, changed, or rejected;
- a runtime failure or negative result changes the next action;
- a new invariant or objective changes admissible adaptation;
- an audited ML gap is closed, regresses, or is re-scoped;
- public semver, MSRV, or productization policy changes.

Do not merge the roadmap or ML maturity overlay itself into the default branch unless the user explicitly requests it.

This file is only the bootstrap pointer. The off-main roadmap plus ML maturity overlay are the persistent sources of current strategy, ecosystem state, and ML execution priorities.
