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

If the roadmap cannot be fetched or read, fail closed for major architecture, runtime-state-machine, representation-format, cross-repository integration, or merge decisions. Read-only diagnosis is allowed.

## Mandatory reread points

Reread the roadmap:

1. at the start of every agent session;
2. before selecting the next major runtime phase;
3. before any cross-repository integration;
4. after any user instruction that changes resource semantics, invariants, objectives, or ecosystem role;
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
- required CI must be green on the exact PR head before merge.

## Mandatory roadmap maintenance

Update the off-main roadmap when:

- a phase changes status;
- an ecosystem contract is published, changed, or rejected;
- a runtime failure or negative result changes the next action;
- a new invariant or objective changes admissible adaptation;
- public semver, MSRV, or productization policy changes.

Do not merge the roadmap itself into the default branch unless the user explicitly requests it.

This file is only the bootstrap pointer. The off-main roadmap is the persistent source of current strategy and ecosystem state.
