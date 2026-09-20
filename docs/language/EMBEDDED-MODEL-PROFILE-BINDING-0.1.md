# Model-execution profile policy binding v0.1

Status: ELANG7c composition contract. Provider-owned model semantics remain in
`elastic-adapters`; the generic policy core receives only stable profile-state
predicates and ordinary pseudo-Boolean constraints.

## Purpose

`ModelExecutionProfilePolicyBindingV1` binds:

- one exact `PolicyIdentity` targeting one logical resource;
- one exact provider identity;
- one exact model revision;
- one exact base capability fingerprint;
- one exact correlated `ModelExecutionProfileSetV1` fingerprint;
- the provider's published correlated profiles.

It does not derive model profiles from generic Elastic dimensions and does not
construct a Cartesian product of individually supported axis values.

## Stable profile predicates

Each provider profile maps to one predicate in namespace:

```text
elastic.model-execution.profile-active
```

The predicate local name is derived from the provider's unique preference rank,
for example `rank-10`. The mapping record retains the actual `profile_id` and
complete qualified tuple. This avoids imposing the stricter `PredicateKey`
character grammar on provider-owned profile IDs.

A profile set larger than the current compact predicate core capacity fails
closed rather than silently truncating the policy.

## Exactly-one policy

The bridge adds one ordinary core pseudo-Boolean constraint:

```text
Σ active_profile_predicate == 1
```

No new constraint evaluator exists. `ResourcePolicySpec` and the existing
pseudo-Boolean core remain authoritative, including `Unknown` behavior when
facts are missing.

The generated resource declaration contains both:

- `model-execution.capability-fingerprint`;
- `model-execution.profile-set-fingerprint`.

Therefore changing the correlated profile tuples changes the generic policy EIR
fingerprint even if profile ids and preference ranks stay unchanged.

## Plan validation

`validate_plan()` does not duplicate model-execution validation. It converts the
selected `ModelExecutionProfilePlanV1` to its existing strict replay envelope and
revalidates that envelope against the exact bound `ModelExecutionProfileSetV1`.
Provider id, model revision, capability fingerprint, profile-set fingerprint and
profile id must all still match.

Successful validation returns the profile's stable predicate key. It does not
apply the profile.

## Authority boundary

This binding is policy composition only. It never:

- invents or ranks profiles;
- changes provider preference order;
- infers model quality from rank;
- actuates a model;
- bypasses model-execution envelope/guard validation;
- treats an exactly-one fact assignment as physical proof of active model state.

The existing model-execution selector, trusted backend validation, transactional
apply, verification and rollback remain authoritative for physical execution.
