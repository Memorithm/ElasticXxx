# Changelog

This changelog records the public release line of ElasticXxx. It does not by itself authorize publication to any package registry.

## 0.1.0 — release-candidate metadata freeze (2026-09-19)

### Public contract

- The supported Rust user boundary is the `elastic` crate, packaged for the selected public registry topology as `memorithm-elastic`.
- Registry-visible implementation packages use the `memorithm-elastic-{core,macros,eir,adapters,runtime,kv}` package identities while preserving their existing Rust crate names.
- The declared MSRV is Rust 1.89.
- Repository-owned code remains under the repository's PolyForm Noncommercial 1.0.0 terms, with commercial licensing handled separately as documented in `LICENSING.md`.

### Qualification boundary

- Boolean-elasticity BE0 through BE14 are recorded as completed in the off-main execution roadmap; BE15 productization remains active.
- This freeze records version/changelog/release-note intent only. It does not publish or reserve a crate, authorize registry publication, establish registry ownership, or turn any benchmark observation into a performance claim.
- Release-time registry-name recheck, dependency-order publication prerequisites, non-leaf archive inspection, a clean registry downstream install, and exact release-commit CI/packageability remain unresolved gates.
