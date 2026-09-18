# Boolean guard operator configuration v1

`GuardConfigV1` is the BE10 operator-facing persisted form for Boolean eligibility policy. It is descriptive, bounded, versioned, and non-actuating. Durable policy refers to predicates by stable `PredicateKey` components; compact process-local `PredicateId` values are never persisted.

Schema v1 supports stable resource, dimension, and transition guard scopes plus threshold predicates over declared observation signals. Each threshold records an explicit unit string, comparison, finite numeric threshold, and freshness age. The unit is semantic metadata attached to the configured signal threshold; schema v1 does **not** perform unit conversion and does not infer that an observation producer used the declared unit. Domain integrations must bind a signal to its documented unit at their trusted observation boundary before relying on the threshold.

Loading is fail-closed. The decoder bounds encoded bytes and JSON nesting before deserialization, rejects unknown or duplicate fields through the strict Serde schema, rejects future schema versions, bounds predicate/guard/expression sizes, rejects duplicate or undeclared predicate keys, rejects non-finite thresholds in programmatically constructed configuration, and preserves the built-in/custom distinction for dimensions and observation signals.

`GuardConfigV1::lower` produces the existing public `PredicateRegistry`, `BooleanGuard`, and `ObservationThresholdPredicate` semantics. It does not sample observers, validate a physical plan, call an adapter, or authorize actuation.

The BE10 operator surface now provides `guard-check`, `guard-list`, `guard-eval`, `guard-explain`, `guard-fingerprint`, and `guard-plan-dry-run`. The first five commands are read-only inspection/evaluation frontends. `guard-plan-dry-run` performs Boolean pruning followed by numeric planning against the selected resource's **declared initial observation state**. It does not construct a physical adapter, run trusted validation, enter a runtime cycle, or authorize actuation.

A controller in `OperatorConfig` v1 may attach one optional `guard_config` object using this exact schema. File-backed operator documents pass through a 256 KiB aggregate/depth preflight before nested deserialization, so embedding policy does not bypass the standalone guard decoder's allocation budget. Programmatically constructed policies are re-encoded through the bounded guard validator during `OperatorConfig::validate`. The declaration-only planning view lowers the policy again at the planning boundary. The CLI may then use it directly:

```text
elastic guard-plan-dry-run --operator-config operator.json --resource ram-budget
```

For compatibility with separately managed policy files, `--guard-config guards.json` remains supported when the selected controller has no embedded policy. Supplying both an embedded policy and `--guard-config` is rejected as ambiguous rather than silently choosing one.

State-changing configured runtime execution is intentionally **not** guard-aware in BE10. `OperatorConfig::build_controller`, bulk controller construction, and therefore normal configured execution fail closed when a controller carries `guard_config`. This prevents an attached guard from being silently ignored. End-to-end trusted validation, actuation, verification, and rollback integration belongs to BE14; Boolean eligibility alone never grants actuation authority.
