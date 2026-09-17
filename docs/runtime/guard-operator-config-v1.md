# Boolean guard operator configuration v1

`GuardConfigV1` is the BE10 operator-facing persisted form for Boolean eligibility policy. It is descriptive, bounded, versioned, and non-actuating. Durable policy refers to predicates by stable `PredicateKey` components; compact process-local `PredicateId` values are never persisted.

Schema v1 supports stable resource, dimension, and transition guard scopes plus threshold predicates over declared observation signals. Each threshold records an explicit unit string, comparison, finite numeric threshold, and freshness age. The unit is semantic metadata attached to the configured signal threshold; schema v1 does **not** perform unit conversion and does not infer that an observation producer used the declared unit. Domain integrations must bind a signal to its documented unit at their trusted observation boundary before relying on the threshold.

Loading is fail-closed. The decoder bounds encoded bytes and JSON nesting before deserialization, rejects unknown or duplicate fields through the strict Serde schema, rejects future schema versions, bounds predicate/guard/expression sizes, rejects duplicate or undeclared predicate keys, rejects non-finite thresholds in programmatically constructed configuration, and preserves the built-in/custom distinction for dimensions and observation signals.

`GuardConfigV1::lower` produces the existing public `PredicateRegistry`, `BooleanGuard`, and `ObservationThresholdPredicate` semantics. It does not sample observers, rank candidates, validate a physical plan, call an adapter, or authorize actuation. Later BE10 slices may add read-only inspection/evaluation commands and non-actuating dry-run planning, but those commands must remain thin frontends over these public library semantics.
