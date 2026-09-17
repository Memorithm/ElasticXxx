# ElasticXxx repository agent instructions

Before making repository changes, fetch and read the persistent off-main ecosystem roadmap:

```bash
git fetch origin agent/ecosystem-roadmap && \
git show origin/agent/ecosystem-roadmap:.agent/ELASTICXXX_ECOSYSTEM_ROADMAP.yaml
```

Treat `AGENTS.md` at repository root as mandatory bootstrap policy.

For predicates, Boolean expressions, guards, invariant prechecks, candidate pruning, decision traces, macros, guard configuration, symbolic or pseudo-Boolean work, also read:

```bash
git show origin/agent/ecosystem-roadmap:.agent/BOOLEAN_ELASTICITY_ROADMAP.yaml
```

Reread the applicable roadmaps at every session start, before a new runtime or Boolean phase, before cross-repository integration, after strategy/invariant changes, and before PR or merge decisions affecting runtime state, representation, kernels, adapters, macros, or contracts.

If a required roadmap is unavailable, fail closed for major architecture, runtime-state-machine, representation-format, Boolean-policy, cross-repository integration, or merge decisions. Do not substitute guesses for missing roadmap state.

A merged PR proves only its implemented slice, not every deliverable of the enclosing BE phase. Record the exact merge SHA, validated head and remaining work. In particular, a guard capture/JSON encoder is not a qualified persisted decoder or an end-to-end guarded controller. A scalar invariant precheck is not a bulk mask implementation. An API wrapper is not proof that arbitrary numeric planners receive only the surviving candidate pool.

Run formatting, Clippy, workspace tests, and all applicable hardening/packageability checks before merge. A queued workflow, missing runner, skipped required check, or successful package-file listing is not a successful Rust test run. Never relax runner restrictions or required checks to report progress.

Public builders and `elastic_guard!` must lower to the same typed Boolean core. Unknown facts must remain Unknown. A true Boolean precheck cannot replace trusted invariant validation. Conflicting trusted checks must fail closed independently of their order.

ElasticXxx owns the generic adaptive-control loop and must preserve existing foundations. It must not absorb domain semantics from SciRust, SLHAv2, FLAT-ATTENTION, NNIS, Forge, SciRust Hub, SciCapsule, SciRust-Verify, BooleanLab, TDI, or other scientific research projects. Reuse versioned public contracts instead of copying an evaluator.
