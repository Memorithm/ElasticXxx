# BE14f kernel-realization Boolean admission — first slice

Status: candidate implementation only. It does not close BE14f.

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

This first slice intentionally does **not** claim durable decision-trace
qualification, real backend actuation, latency/throughput improvement, GPU
execution, energy reduction, model-quality equivalence, or BE14f completion.
Those require later slices and exact-head evidence.
