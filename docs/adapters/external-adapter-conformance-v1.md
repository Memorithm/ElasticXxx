# External adapter conformance v1

Status: supported pre-1.0 third-party implementer contract; registry publication remains suspended.

ElasticXxx now exposes a deliberately narrow adapter-authoring namespace:

```rust
use elastic::external_adapter_v1::*;
```

Its `CONTRACT_VERSION` is `1`. The namespace is the reviewed integration boundary for
external Rust resource adapters during the 0.1.x line. It is narrower than
`elastic::prelude` and the full `elastic::runtime` namespace so downstream adapters do
not need to bind themselves to unrelated implementation details.

This is not a claim that the complete ElasticXxx API has reached 1.0 semver stability.
A material change to the callback meanings or trust boundary described here should
introduce a new versioned adapter namespace and migration evidence rather than silently
changing the v1 contract.

## Authority model

`Observer` supplies planning evidence. Observation values do not authorize a physical
effect.

`TransactionalActuator` is the trusted resource-specific boundary. The callback order
for an applying runtime cycle is:

```text
observe -> plan -> validate -> prepare -> actuate -> verify
                                      -> commit
                                      -> rollback on failure/inconclusive verification
```

The adapter owns resource-specific action-time feasibility and physical semantics.
The generic planner remains advisory.

### validate

`validate(&Plan)` re-checks hard invariants and action-time preconditions. Applicable
invariants must have explicit `InvariantCheck` evidence. Missing or failed applicable
checks keep the plan unvalidated.

### prepare

`prepare(&ValidatedPlan)` may construct an `Actuation`, but it must not apply the
physical effect.

The runtime now enforces two binding rules before `actuate` is called:

1. `Actuation.plan` must equal the exact `ValidatedPlan` passed to `prepare`;
2. `Actuation.adapter_name` must equal the active adapter's `name()`.

A foreign plan or adapter identity is rejected as `RuntimeError::Validation` before
physical actuation. These checks prevent a third-party implementation from accidentally
substituting another already-valid plan at the prepare boundary.

### actuate

`actuate` performs the physical effect. An error is treated conservatively as possibly
partial, so the runtime proceeds through an inconclusive verification state to rollback.

### verify

`verify` reports `Pass`, `Fail`, or `Inconclusive`. Only `Pass` can reach
`commit`.

### commit and rollback

`commit` runs only after successful verification. If commit itself fails, the runtime
attempts rollback.

`rollback` must report whether invariants were restored. A rollback record with
`invariants_restored = false` is itself a runtime rollback error; it is never converted
into a successful cycle.

## Curated v1 symbols

The v1 namespace re-exports the types needed to implement and exercise the contract:

- `Observer`, `Observation`, `ObservationSource`, `PlanningContext`;
- `TransactionalActuator`, `Plan`, `ValidatedPlan`, `InvariantCheck`;
- `Actuation`, `VerificationResult`, `CommitRecord`, `RollbackRecord`,
  `RuntimeError`;
- `Runtime`, `RuntimeConfig`, `RuntimeMode`, `Controller`, `CycleResult`;
- `EirResource`, `TransitionPlanner`, `FirstGroundedPlanner`.

This list is intentionally small. Domain-specific adapters may consume other versioned
Elastic contracts when their domain requires them, but those are separate compatibility
surfaces and must not be inferred from this generic adapter v1 namespace.

## Standalone qualification

`fixtures/external-adapter-v1` is deliberately outside the repository Cargo workspace.
Its manifest declares exactly one direct dependency:

```toml
elastic = { package = "memorithm-elastic", path = "../../crates/elastic" }
```

The fixture implements an external observer and actuator using only
`elastic::external_adapter_v1`. It proves four lifecycle cases:

1. verified actuation commits;
2. failed verification rolls back without commit;
3. failed commit rolls back;
4. a misbound adapter identity is rejected before `actuate`.

`scripts/check-external-adapter-v1.py` rejects extra direct dependencies, direct imports
of implementation crates, fixture publication, MSRV drift, or loss of the versioned
facade namespace.

The dedicated GitHub workflow builds and tests the fixture as a standalone Cargo project
on Rust 1.89.0. The normal workspace CI separately qualifies the runtime changes on the
same exact pull-request head.

## External installation boundary

The fixture currently uses a repository path dependency because all registry-visible
ElasticXxx packages remain `publish = false`. It therefore proves facade-only source
consumption, not installation from crates.io.

A clean registry-based external project remains a release-time gate and requires explicit
publication authorization, actual publication of the dependency chain, and fresh
exact-version qualification. This conformance contract does not authorize publication.
