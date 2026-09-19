# Elastic policy DSL v0.1

Status: ELANG5 declarative policy surface over the existing typed policy,
Boolean guard, pseudo-Boolean constraint, objective metadata, and EIR contracts.
The syntax owns no independent eligibility or actuation semantics.

## Example

```rust
elastic! {
    pub document inference_runtime {
        resource inference {
            class(configurational);
            id("inference");
            allow(capacity);
            optimize(latency, throughput);
            admit(reinterpret @ capacity);
            capability(reinterpret @ capacity);
        }

        policy adaptive {
            id("runtime.inference");
            version(1, 0, 0);
            target(inference);

            predicate(capacity_ok, "elastic.inference", "capacity-ok");
            predicate(high_mode, "elastic.inference", "high-mode");

            guard transition(reinterpret @ capacity) when(capacity_ok && !high_mode);
            constraint at_most(1, capacity_ok, high_mode);
            constraint budget {
                unit("MiB");
                quantum(1);
                maximum(8);
                term(high_mode, 4);
            }

            objective latency minimize unit("microseconds") quantum(1);
            objective throughput maximize unit("ops-per-second") quantum(1);
            hint("search.mode", "balanced");
        }
    }
}

let typed = inference_runtime::adaptive::policy_spec()?;
let eir = inference_runtime::adaptive::policy_eir()?;
```

## Stable predicate identity

`predicate(alias, "namespace", "name")` introduces a local DSL alias for one
ordinary `PredicateKey`. The alias has no runtime identity of its own. Every
`guard` and pseudo-Boolean `constraint` references only aliases declared inside
the same policy block. Unknown aliases are compile-time errors.

The guard expression is lowered through the public `elastic_guard!` macro and
therefore preserves its existing strong-Kleene semantics. Missing runtime facts
remain `Unknown`; the DSL never rewrites missing facts to `False`.

## Constraints

The v0.1 syntax directly invokes the existing core constructors:

- `constraint at_most(N, ...)` -> `at_most_keys`;
- `constraint at_least(N, ...)` -> `at_least_keys`;
- `constraint exactly(N, ...)` -> `exactly_keys`;
- `constraint requires(a, b)` -> `requires_key`;
- `constraint equivalent(a, b)` -> `equivalent_keys`;
- `constraint budget { ... }` -> `capacity_budget` with explicit integer
  weights, unit, quantum, and maximum.

No constraint evaluator exists in the proc macro. Bounds, canonicalization,
stable-key binding, and three-valued evaluation remain owned by
`elastic-core`/`elastic-eir`.

## Numeric objectives and planner hints

`objective` and `hint` declarations lower through ELANG5c
`ResourcePolicyAdvisorySpec`. Numeric objective metadata must refer to an
objective already declared by the target resource. Planner hints are canonical,
bounded metadata only: changing a hint can change the advisory fingerprint but
cannot change the semantic policy fingerprint, admit a transition, satisfy a
guard/constraint, or bypass trusted validation.

## Target and authority

Policy DSL v0.1 targets one resource module declared in the same `elastic!
document`. The target is resolved to the resource's actual `LogicalResourceId`;
the Rust module name is never substituted for an explicit `id("...")`.

`policy_spec()` returns the ordinary `ResourcePolicyAdvisorySpec` and
`policy_eir()` calls the ordinary `lower_resource_policy_advisory` path. The
same typed errors are preserved through `ElasticPolicyDocumentError`.

## Rejected hidden semantics

The proc macro rejects at compilation:

- unknown policy target modules;
- duplicate predicate aliases;
- guard atoms that were not declared with `predicate(...)`;
- constraint terms that were not declared with `predicate(...)`;
- missing mandatory policy identity/version/target declarations.

Resource-level semantic errors that require the fully built target resource
(for example an objective metadata entry that was never declared in
`optimize(...)`) remain typed fail-closed errors from `policy_spec()`.

## Non-goals

This slice does not introduce dynamic policy loading, inheritance, group-level
policy execution, implicit telemetry derivation, hidden numeric comparisons,
a universal scalar objective score, or any new actuation authority.
