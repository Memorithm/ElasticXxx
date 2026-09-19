# Elastic static analysis frontend v0.1

Status: ELANG6 read-only developer tooling. This layer does not validate runtime
invariants and cannot authorize planning, actuation, commit, registry mutation,
or any business decision.

## `cargo elastic analyze`

```bash
cargo elastic analyze --config guards.json
```

The command decodes the existing bounded `GuardConfigV1`, lowers it through the
public Elastic runtime contract, then delegates all semantics to
`ExactKleeneOracle`. The CLI only serializes results.

The v0.1 analysis uses exact strong-Kleene assignments, so `Unknown` remains a
first-class truth value. It reports:

- satisfiability and dead guards;
- tautologies under three-valued semantics;
- pairwise explicit-True implication;
- exact semantic equivalence including `Unknown`;
- pairwise mutual exclusion.

Stable analysis diagnostics:

| Code | Meaning |
| --- | --- |
| `ELX-ANALYZE-0001` | guard can never evaluate explicitly `True` |
| `ELX-ANALYZE-0002` | guard is explicitly `True` for every strong-Kleene assignment |
| `ELX-ANALYZE-0003` | two guards are exactly equivalent including `Unknown` |
| `ELX-ANALYZE-0004` | explicit-True eligibility of one guard implies another |
| `ELX-ANALYZE-0005` | two guards are mutually exclusive |

Oracle work is explicitly bounded by `--max-variables` and
`--max-assignments`. Exceeding either bound fails closed instead of switching to
an approximate solver.

## `cargo elastic graph`

```bash
cargo elastic graph --config guards.json
cargo elastic graph --config guards.json --format dot
```

`graph` performs no truth evaluation. It emits canonical predicate nodes, guard
nodes (scope + expression fingerprint), and predicate-to-guard reference edges.
JSON is the default machine-readable format; DOT is provided for visualization.

The graph is deliberately structural: it does not infer ownership, causality,
performance impact, or application semantics from predicate names.

## Authority boundary

Both commands are read-only. Analysis findings are diagnostics only; trusted
runtime validation immediately before physical effect remains authoritative.
