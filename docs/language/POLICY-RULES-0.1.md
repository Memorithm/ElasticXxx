# Elastic resource policy rules v0.1

Status: ELANG5b typed resource-policy composition. This layer reuses existing
Boolean guard and pseudo-Boolean constraint authorities; it does not introduce a
second policy evaluator.

## Resource policy contract

`ResourcePolicySpec` contains:

```text
PolicyHeader
+ GuardedResourceSpec
+ [PseudoBooleanConstraintDeclaration]
```

Construction requires the `PolicyHeader` target to be a `Resource` whose
`LogicalResourceId` exactly matches the supplied `ResourceSpec`.

A `Group` target is rejected by this resource-policy type. ELANG does not
silently reinterpret resource-scoped guards as group-scoped rules.

## Boolean guards

Guards are ordinary `BooleanGuard` values. Binding is delegated directly to
`GuardedResourceSpec::new`, which remains authoritative for:

- one guard per scope;
- elastic-dimension validity;
- transition admission validity;
- stable `PredicateKey` registries;
- canonical Boolean expression/fingerprint semantics;
- strong-Kleene `True / False / Unknown` evaluation later at runtime.

A resource policy cannot use a guard to create a transition that the resource
did not already admit.

## Pseudo-Boolean constraints

Constraints are ordinary durable `PseudoBooleanConstraintDeclaration` values.
Their existing contracts remain authoritative for stable predicate keys,
cardinality helpers, signed/unsigned weights, explicit unit/quantum scaling,
overflow checks and binding to a concrete predicate registry.

One resource policy accepts at most `MAX_RESOURCE_POLICY_CONSTRAINTS` (64)
constraints. `MAX_EIR_PSEUDO_BOOLEAN_CONSTRAINTS` is sourced from the same core
bound so core and EIR cannot silently drift.

## EIR

`lower_resource_policy()` produces `EirResourcePolicy` by composing:

```text
EirPolicyHeader
+ EirConstrainedResource
```

The combined fingerprint therefore changes when any of the following changes:

- policy ID/version;
- exact target resource definition;
- Boolean guard policy;
- pseudo-Boolean constraint terms/relation/threshold/unit/quantum.

Constraint input order is normalized by the existing constrained EIR and does
not change semantic identity.

## Authority boundary

ELANG5b still does not:

- acquire observations or facts;
- evaluate a guard or constraint;
- collapse missing facts to `False`;
- rank numeric objectives;
- select a planner;
- validate hard invariants;
- authorize `ACT`, `COMMIT`, or registry mutation.

Runtime fact derivation, Boolean/pseudo-Boolean evaluation, numeric planning and
trusted adapter validation remain separate existing authorities.
