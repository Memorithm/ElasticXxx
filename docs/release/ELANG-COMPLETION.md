# Elastic Language program completion gate

Status: engineering completion record; registry publication remains disabled.

The ELANG program is complete only when the implemented language/runtime slices
and the final hardening slice are all qualified on an exact commit. Completion
does not remove `publish = false` and does not authorize crates.io publication.

## Implemented slices

- ELANG1-3: typed resource declarations, normalized EIR, multi-resource groups,
  dependency topology, budgets and cross-resource invariants.
- ELANG4: deterministic composite planning plus prepare/checkpoint and coordinated
  act/verify/commit-or-restore transaction semantics.
- ELANG5: versioned policy identity, Boolean rules, numeric objective metadata,
  advisory hints and declarative policy lowering.
- ELANG6: stable developer tooling, diagnostics, static analysis, graphing and
  shared stable expansion.
- ELANG7: embedded/edge observations and composition, immutable reservations,
  anti-thrashing admission and CPU-only reference qualification.
- ELANG8: versioned external-consumer convergence through the public facade and
  explicit adapter contracts, with domain semantics retained by consumers.
- ELANG9: compatibility and malformed-input hardening, clean external Git
  consumption, bounded persisted decoders, parser/expander fuzzing, remaining
  bounded trace/candidate fuzz coverage, and targeted Miri gates.

## ELANG9 exact-head exit gate

An exact ELANG9 completion commit must satisfy:

1. Rust 1.89 formatting, Clippy, tests and rustdoc;
2. `cargo +nightly fuzz build` for every registered fuzz target;
3. bounded fuzz execution for the final parser/decoder surfaces on the exact PR head;
4. targeted Miri execution for language, kernel and runtime decoder tests on the
   same exact PR head;
5. the clean exact-revision external consumer gate;
6. ordinary packageability/release-candidate checks while all registry-visible
   manifests remain `publish = false`.

Recurring scheduled/manual hardening retains the longer fuzz campaigns and wider
Miri coverage. A successful completion gate is evidence for ELANG engineering
closure only; it is not a registry-release authorization.

## Post-ELANG rule

New elastic features discovered after this gate are ordinary product evolution;
they do not reopen an already qualified ELANG slice unless they invalidate one
of its recorded contracts. crates.io remains a separate final product-release
decision after all elastic functionality intended for that release is complete.
