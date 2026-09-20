# Immutable safety-critical capacity reservations v0.1

Status: ELANG7 safety policy contract. This is static policy intent, not a
physical reservation primitive.

## Motivation

An embedded/edge application may have resources that adaptation is never
allowed to consume. For example, flight control may require RAM that inference
or KV growth must not absorb.

Encoding this as an undocumented subtraction such as `available_ram - 2 GiB`
hides the safety contract from policy identity, EIR fingerprints and review.
ELANG7 instead records the reservation explicitly.

## Contract

`ImmutableCapacityReservation` contains:

- a stable `SafetyReservationId`;
- a capacity kind (`Ram` or `Storage`);
- the logical resource that owns the reservation;
- an application/domain `ContractId` explaining the safety semantic;
- a strictly positive reserved byte count.

`SafetyCapacityEnvelope` binds one or more immutable reservations to one static
capacity policy total and derives the adaptive budget exactly:

```text
reserved = Σ immutable_reservation_bytes
adaptive_ceiling = total_policy_bytes - reserved
```

The envelope fails closed when:

- no reservation exists;
- reservation ids collide;
- RAM/storage kinds are mixed;
- the reservation byte sum overflows;
- reservations exceed the declared total;
- adaptive budget terms are invalid under the existing capacity-budget rules.

When attached to a `ResourceGroup`, every reservation owner must be a declared
group member. The generated adaptive budget is also installed as the ordinary
`SharedBudget`; existing pseudo-Boolean semantics remain authoritative for
candidate costs.

## EIR provenance

The grouped EIR retains the envelope and every reservation with:

- reservation id;
- capacity kind;
- owner resource;
- external safety `ContractId`;
- reserved bytes;
- total policy bytes;
- total reserved bytes;
- remaining adaptive ceiling;
- adaptive budget id and fingerprint.

Changing a reservation amount or safety contract changes the grouped EIR
fingerprint even when the adaptive resource set is otherwise unchanged.

## Immutability meaning

`Immutable` means the policy object has no mutation/rebalancing API after
construction. A different safety reservation requires a new declaration/policy
identity and therefore a new structural fingerprint.

It does **not** mean external hardware state is immutable.

## Static/dynamic boundary

`total_policy_bytes` is operator/application policy input. It is not evidence of
current physical capacity. A valid safety envelope does not prove that RAM was
allocated, storage was reserved, a cgroup limit is available, or another
process cannot consume capacity.

Runtime actuation still requires current observation plus trusted validation.
If physical capacity falls below the static envelope, adaptation must fail
closed rather than consume the safety reservation.

## Leases and physical reservations

This v0.1 contract does not acquire a lease, pin pages, allocate memory, reserve
a filesystem extent, set cgroup controls, or fence another controller. Those
are physical mechanisms owned by adapters/orchestrators and require separate
capability, freshness, verification and rollback contracts.

The static reservation is therefore the **minimum protected policy intent**;
the runtime remains responsible for proving that the physical world can honor
it before effect.
