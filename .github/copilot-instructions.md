# ElasticXxx repository agent instructions

Before making repository changes, fetch and read the persistent off-main ecosystem roadmap:

```bash
git fetch origin agent/ecosystem-roadmap && \
git show origin/agent/ecosystem-roadmap:.agent/ELASTICXXX_ECOSYSTEM_ROADMAP.yaml
```

For ML resource, representation, precision, device, batching, memory, kernel, KV, benchmark, or cross-repository ML work, also read:

```bash
git show origin/agent/ecosystem-roadmap:.agent/ML_MATURITY_5_OF_5.yaml
```

Treat `AGENTS.md` at repository root as mandatory bootstrap policy.

Reread the roadmap and applicable ML overlay at every session start, before a new runtime phase, before cross-repository integration, after strategy/invariant/ML-priority changes, and before PR or merge decisions affecting runtime state, representation, kernels, adapters, or contracts.

If the roadmap or applicable ML overlay is unavailable, fail closed for major architecture, runtime-state-machine, representation-format, cross-repository integration, or merge decisions. Do not substitute guesses for missing roadmap state.

ElasticXxx owns the generic adaptive-control loop and must preserve existing foundations. It must not absorb domain semantics from SciRust, SLHAv2, FLAT-ATTENTION, NNIS, Forge, SciRust Hub, SciCapsule, SciRust-Verify, or scientific research projects. A `5/5` claim requires real ML actuation through the full verify/commit-or-rollback loop and evidence against static baselines.
