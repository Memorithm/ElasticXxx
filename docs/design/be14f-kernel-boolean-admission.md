# BE14f kernel-realization Boolean admission — guarded transaction

Status: third candidate slice. It does not close BE14f.

This slice adds a fail-closed Boolean front-end to the existing `elastic-kernel`
planner. The front-end evaluates the stable predicate
`elastic.kernel/capability-compatible` for each declared realization and keeps
three-valued semantics:

- `True`: structural identity/contract matches and the fresh capability snapshot
  satisfies the candidate requirements;
- `False`: a grounded structural or capability requirement is incompatible;
- `Unknown`: the relevant capability snapshot is missing, has no capture time,
  is future-dated, stale, internally invalid, or leaves a required optional
  feature unobserved.

The capability source is `elastic-kernel/capability-snapshot/v1`. Numeric
limits keep their native units (`invocations`, `bind-groups`, `bytes`) and
optional features retain explicit `known-true | known-false | unknown` state.
No unit conversion or proxy measurement is introduced.

`plan_with_boolean_admission` runs the existing objective planner only when all
structurally relevant candidates are grounded. It prunes conclusive `False`
candidates before objective ranking and blocks the whole ranking pass when any
relevant candidate is `Unknown`; silently skipping such a candidate could
change the winner. The existing kernel lifecycle remains the only path from a
selection to validation, activation, post-activation verification and commit or
rollback.

The second slice adds `plan_with_boolean_admission_traced` and the strict,
bounded `BooleanKernelDecisionTraceV1` JSON contract. It binds the logical
resource, workload fingerprint, capability fingerprint when grounded, policy,
Boolean candidate classifications, planner outcome, selected realization and
selection fingerprint. Unknown/duplicate JSON fields, future schemas,
oversized inputs, invalid truth values, unordered candidate identities and
inconsistent selected outcomes fail closed. Decoding is explicitly explanatory:
it has no lifecycle authority and cannot replace current capability discovery or
the trusted kernel lifecycle.

The third slice adds `KernelRealizationBackendV1` and
`execute_guarded_kernel_transaction`. A selected plan is rebound to the exact
candidate and an unchanged, freshly supplied capability fingerprint before any
backend method runs. The backend must then discharge action-time validation,
activation, verification and commit. Activation/verification/commit failures
produce lifecycle rollback evidence and invoke backend restoration. Boolean
`Unknown`, conclusive `False`/no-candidate, planner non-results, trace mismatch,
and capability drift do not enter backend actuation.

The test provider exercises `True`, `False` and `Unknown`, fresh-capability drift,
verification rollback, and an explicit guarded-vs-unguarded differential
baseline. The differential test establishes only semantic equality of the
selected/committed realization in the deterministic host test fixture; it is not
a performance result or physical-device qualification.

This slice still does **not** claim a real GPU/backend deployment,
latency/throughput improvement, energy reduction, model-quality equivalence, or
BE14f completion. Portable benchmark evidence remains required before any
performance claim, and physical consumers must separately qualify their own
backend semantics and hardware observations.
