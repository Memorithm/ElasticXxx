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

For any Boolean predicate, guard, eligibility, invariant precheck, candidate pruning, policy expression, symbolic analysis, SAT/BDD, pseudo-Boolean resource constraint, or BooleanLab/TDI Boolean-policy integration work, also read:

`origin/agent/ecosystem-roadmap:.agent/BOOLEAN_ELASTICITY_ROADMAP.yaml`

```bash
git fetch origin agent/ecosystem-roadmap && \
git show origin/agent/ecosystem-roadmap:.agent/BOOLEAN_ELASTICITY_ROADMAP.yaml
```

The Boolean roadmap is mandatory implementation state. Its ordered phases are: bootstrap/ownership; dependency-free three-valued Boolean core; canonicalization and stable identity; EIR guards; transition guards; runtime fact derivation; planner pruning; invariant Boolean prechecks; decision traces/evidence; public Rust API/macros; CLI/operator configuration; bounded symbolic analysis; pseudo-Boolean constraints; hardware-friendly parallel evaluation; domain integration; and cross-repository research promotion/productization. Agents must advance the earliest unblocked phase, preserve already merged foundations, update the roadmap after each merged slice, and never treat a planned phase as implemented evidence.

The ML overlay makes 5/5 an evidence-backed exit criterion. Elastic adaptation is mature only when the full control loop executes against real ML workloads, every actuation is verified and rollback-capable, and measured policy outcomes are compared with static baselines under declared objectives.

If the roadmap, applicable ML overlay, or applicable Boolean roadmap cannot be fetched or read, fail closed for major architecture, runtime-state-machine, representation-format, Boolean-policy, symbolic-analysis, cross-repository integration, or merge decisions. Read-only diagnosis is allowed.

## Mandatory reread points

Reread the roadmap and any applicable overlay:

1. at the start of every agent session;
2. before selecting the next major runtime or Boolean-elasticity phase;
3. before any cross-repository integration;
4. after any user instruction that changes resource semantics, invariants, objectives, ecosystem role, Boolean-policy direction, or ML maturity priorities;
5. before opening or merging runtime, representation, kernel, adapter, Boolean-policy, symbolic-analysis, or contract PRs.

## Repository role

ElasticXxx owns the generic adaptive resource runtime and its typed control loop:

`OBSERVE -> FORECAST -> PLAN -> VALIDATE -> ACT -> VERIFY -> COMMIT / ROLLBACK`

Do not rewrite existing foundations merely to create activity. Build on the existing `elastic-core`, `elastic-eir`, `elastic-macros`, facade, adapters, KV, kernel, downstream guards, validation, actuation, representation, and rollback concepts unless a concrete contract gap is demonstrated.

Boolean logic is an eligibility and control layer, not a replacement for continuous measurements or numeric objectives. Missing evidence is not false: Boolean decision surfaces must preserve explicit `Unknown` semantics and fail closed before actuation. A compiled guard may reject or prune candidates but may never make an undeclared or invariant-violating transition legal. Trusted adapter validation immediately before physical actuation remains authoritative.

ElasticXxx must not absorb domain semantics owned by SciRust, SLHAv2, FLAT-ATTENTION, NNIS, Forge, SciRust Hub, SciCapsule, SciRust-Verify, BooleanLab, TDI, or other scientific research repositories. BooleanLab owns experimental Boolean-function discovery/calibration; TDI owns its scientific Boolean-policy lineage and preregistration boundaries. Promote only versioned, tested, genuinely generic contracts.

## Core constraints

- correctness and declared semantic invariants dominate optimization;
- no fabricated performance or scientific novelty;
- forecasts are advisory and may never override hard invariants;
- every physical actuation must pass validation immediately before application;
- failed post-actuation verification must rollback or fail closed explicitly;
- external candidates and measurements require compatible identities/fingerprints and independent invariant revalidation;
- kernel elasticity and physical representation elasticity are separate axes;
- Boolean fast paths must be observationally equivalent to the generic evaluator;
- SAT/BDD/symbolic-analysis backends must be optional, bounded, and unable to authorize actuation on solver failure or timeout;
- pseudo-Boolean integer scaling and units must be explicit and overflow-safe;
- required CI must be green on the exact PR head before merge;
- planned adaptive dimensions, proxy metrics, Boolean speedups, or unverified forecast quality never count as maturity evidence.

## Mandatory roadmap maintenance

Update the off-main ecosystem roadmap, Boolean roadmap, and ML overlay when applicable when:

- a phase or ML maturity phase changes status;
- an ecosystem contract is published, changed, or rejected;
- a runtime failure or negative result changes the next action;
- a new invariant, predicate, guard, or objective changes admissible adaptation;
- a Boolean optimization is proven equivalent, disproven, or benchmarked;
- an audited ML gap is closed, regresses, or is re-scoped;
- public semver, MSRV, or productization policy changes.

Do not merge the roadmap or overlays themselves into the default branch unless the user explicitly requests it.

This file is the mandatory bootstrap pointer and condensed execution contract. The off-main ecosystem roadmap, Boolean elasticity roadmap, and applicable ML overlay are the persistent sources of current strategy, ecosystem state, and implementation priorities.
