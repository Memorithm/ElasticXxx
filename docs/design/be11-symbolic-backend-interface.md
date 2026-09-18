# BE11 symbolic-backend boundary

Status: **BE11c interface candidate**. This is an engineering boundary, not solver qualification or scientific evidence.

## Scope

`elastic-core::SymbolicBackend` is a dependency-free, analysis-only SAT interface. It accepts a bounded Boolean expression plus explicit finite configuration and returns only `Sat`, `Unsat`, `Unknown`, `Timeout`, or `ResourceLimit`.

There is no validation, actuation, commit, publication, or policy-authorization method. `Unknown`, timeout and resource exhaustion remain non-results.

The configuration makes timeout, clause, node/search-state, memory and deterministic seed policy explicit. Zero resource values are rejected rather than acquiring an undocumented solver-specific “unlimited” meaning.

## Dependency and licence gate

No external SAT, BDD or SMT backend is added by BE11c. A BE11d backend must be reviewed at its exact selected revision before introduction, including root licence, Rust-binding licence where separate, transitive dependency licences, native/build-tool surface, maintenance state, MSRV impact, deterministic/resource-limit controls and packageability.

Microsoft Research currently describes upstream Z3 as MIT-licensed. That fact alone does not select or qualify Z3 for ElasticXxx and does not audit any Rust binding, transitive dependency, or native packaging surface.

Rust SAT/BDD candidates are likewise not approved here because no exact revision has yet been selected and audited.

Therefore BE11d remains gated: choosing a backend requires a dedicated dependency/licence review and differential tests against the dependency-free exact small-domain oracle. A backend whose timeout, unknown, or resource-exhaustion state cannot be preserved explicitly is inadmissible.

Reference checked 2026-09-18: Microsoft Research, “Z3”, https://www.microsoft.com/en-us/research/project/z3-3/ .
