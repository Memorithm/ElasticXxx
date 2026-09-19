# Elastic policy identity and target binding v0.1

Status: ELANG5a typed foundation. This contract introduces policy identity,
versioning and target binding only. It grants **no** planning, validation or
actuation authority.

## Typed identity

A policy revision is represented by:

```text
PolicyId + PolicyVersion -> PolicyIdentity
```

`PolicyId` is a bounded canonical lowercase-ASCII lineage identifier using only
letters, digits, `.`, `_` and `-`. `PolicyVersion` is a dependency-free
`major.minor.patch` value. Version is kept separate from lineage identity so an
upgrade does not silently create a different policy family.

Example:

```text
embedded.flight-control@0.1.0
embedded.flight-control@0.2.0
```

These are two exact revisions of one lineage.

## Typed target

`PolicyTarget` is either:

```text
Resource(LogicalResourceId)
Group(ResourceGroupId)
```

Target kind is semantic. A resource and group with the same display text do not
alias. Fingerprints and future wire formats must retain the target-kind
discriminator.

`PolicyHeader` contains exactly `PolicyIdentity + PolicyTarget`. It contains no
hidden guards, constraints, planner hints or transitions.

## EIR binding

`EirPolicyHeader` binds the typed header to the exact current EIR target:

- resource policy -> exact `EirResource::fingerprint()`;
- group policy -> exact `EirResourceGroup::fingerprint()`.

The policy-binding fingerprint includes:

- policy ID;
- major/minor/patch version;
- target kind;
- target identity;
- exact target structural fingerprint.

Therefore a changed resource definition or changed ELANG3 group topology cannot
reuse the same EIR policy binding unnoticed.

Unknown targets and target-kind mismatches fail closed.

## Authority boundary

ELANG5a deliberately does **not**:

- create or admit transitions;
- make a guard `True`;
- collapse `Unknown` to `False`;
- define pseudo-Boolean constraint semantics;
- rank objectives;
- select a planner;
- validate invariants;
- authorize physical actuation.

Later ELANG5 slices must reference the already existing Boolean guard,
pseudo-Boolean constraint, objective and runtime validation authorities rather
than reimplementing them inside the policy language.

Registry publication/mutation remains disabled independently of this language
work.
