# Elastic numeric objective metadata and planner hints v0.1

Status: ELANG5c advisory metadata. This layer does not change eligibility,
transition admission, invariant authority, or planner correctness requirements.

## Numeric objective metadata

`ResourceSpec::objectives()` remains the sole cross-objective priority order.
There is deliberately no universal scalar cost model.

`PolicyNumericObjective` may enrich an objective that is already declared by the
resource with:

- `PolicyObjectiveDirection::{Minimize, Maximize}`;
- `PolicyMetricScale { unit, quantum }`.

The scale expresses how a planner that explicitly understands this metadata
interprets integer metric ticks. It does not define conversion between different
objectives or make one objective outweigh an invariant.

ELANG rejects numeric metadata for an objective not already present in the
resource and rejects duplicate metadata for the same typed `ObjectiveId`.
Metadata is normalized into the existing `ResourceSpec` priority order,
regardless of caller input order.

Built-in and custom objectives with the same display text remain distinct typed
values and have distinct EIR fingerprints.

## Budget units

Pseudo-Boolean budgets keep using the existing
`PseudoBooleanScale { unit, quantum }` authority. ELANG5 does not create a second
budget-unit model. Resource-policy constraints are lowered by the same
`EirPseudoBooleanConstraint` machinery used before the policy language.

## Planner hints

`PlannerHint` is bounded metadata:

```text
canonical key -> trimmed bounded value
```

Keys use lowercase ASCII letters, digits, `.`, `_` and `-`. Hints are sorted by
key and duplicate keys fail closed.

Hints are **advisory only**. A planner may explicitly opt in to a known hint, but
a hint cannot:

- admit an undeclared transition;
- satisfy a Boolean guard;
- change a pseudo-Boolean truth value;
- turn `Unknown` into `False` or `True`;
- override an invariant;
- bypass trusted validation;
- authorize physical actuation.

## EIR identity separation

`EirResourcePolicy` remains the semantic policy fingerprint for target + guards
+ constraints.

`EirResourcePolicyAdvisory` adds a separate envelope fingerprint over:

- the semantic policy fingerprint;
- ordered numeric objective metadata;
- canonical planner hints.

Changing only a hint changes the advisory-envelope fingerprint while leaving the
semantic `EirResourcePolicy::fingerprint()` unchanged. This prevents advisory
metadata from being confused with eligibility semantics while still making it
safe for caching, replay diagnostics and configuration comparison.
