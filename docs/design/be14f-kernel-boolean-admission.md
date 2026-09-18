# BE14f kernel-realization Boolean admission — first slice

Status: second candidate slice. It does not close BE14f.

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

This slice still does **not** claim real backend actuation,
latency/throughput improvement, GPU execution, energy reduction, model-quality
equivalence, or BE14f completion. Trusted transaction/real-consumer
VALIDATE->ACT->VERIFY->COMMIT/ROLLBACK, an explicit unguarded differential
baseline, and portable benchmark evidence remain separate slices.
