# ELANG4a composite plan envelope

Status: deterministic, non-actuating multi-resource planning envelope.

`CompositePlanEnvelope` is the first ELANG4 runtime contract. It binds several
ordinary single-resource `Plan` values to one exact ELANG3
`EirGroupedDocument` and one resource group.

It does **not** prepare, actuate, verify, commit, or roll back anything.

## Construction rules

A composite envelope fails closed unless:

- the named group exists;
- at least one subplan is supplied and the bounded plan count is respected;
- every subplan targets a member of that group;
- the complete `EirResource` in each plan equals the authoritative resource node
  from the grouped document, not merely the same logical resource name;
- every subplan carries a declared capability-grounded candidate;
- no logical resource appears more than once;
- every planning-context value is finite.

The envelope stores the exact grouped-document fingerprint. A plan built against
a different EIR definition of the same resource ID is rejected.

## Dependency ordering

ELANG3 dependencies are declared as:

```text
dependent -> required
```

For resources that are both targeted in one composite envelope, ELANG4a orders
the required resource before its dependent. Independent ready resources are
ordered lexicographically by logical resource identity, making the result
independent of input plan order.

A dependency does **not** force the required resource to mutate. If only the
dependent has a candidate in a given composite operation, the edge creates no
artificial no-op mutation. Later trusted composite validation is responsible for
rechecking any cross-resource conditions that involve unchanged participants.

`rollback_order()` is the exact reverse of the forward order. This is only
structural planning data in ELANG4a; the actual rollback protocol is a later
ELANG4 transaction slice.

## Structural fingerprint

The non-cryptographic `Fingerprint` binds:

- schema identity;
- exact `EirGroupedDocument` fingerprint;
- group identity;
- ordered target-resource identities and EIR fingerprints;
- candidate mechanism, dimension, capability grounding and optional magnitude;
- canonical finite `PlanningContext` signal/value bits.

Planner `reasoning` text is intentionally excluded because it is explanatory,
not transition semantics. The fingerprint is an audit/cache identity inside one
trust domain and never authorizes actuation.

## Shared budgets and invariants

The group fingerprint transitively binds ELANG3 shared budgets and
cross-resource invariants. ELANG4a does not evaluate those constraints or claim
they hold. The later trusted composite transaction must validate all applicable
cross-resource contracts immediately before physical effect.

## Non-goals

This slice does not yet provide:

- checkpoint/pre-act state capture;
- multi-resource prepare;
- physical actuation;
- coordinated verification;
- atomic or staged commit;
- rollback after partial failure;
- irrecoverable failure state;
- cancellation during a composite transaction.

Those contracts are deliberately separated so plan ordering cannot be mistaken
for execution authority.
