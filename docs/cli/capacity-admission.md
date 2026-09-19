# Capacity admission v1

`elastic admit-capacity` reads a strict JSON request from stdin (maximum 16 KiB).
It calls the public `CapacityAdmissionControllerV1` and emits a versioned decision
with the complete request, previous/proposed/final width, verification, commit or
rollback status and ordered runtime events. A rejected decision is printed and
returns exit 2; malformed requests return exit 2 without an admitted decision.
If a runtime failure prevents an authoritative transaction result, `committed`
and `rolled_back` are `null`, with the emitted events and failure retained. An
unknown transaction state never authorizes new work.

```json
{
  "schema_version": 1,
  "plan_id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "max_concurrency": 4,
  "memory_bytes_per_trial": 1048576,
  "reserve_memory_bytes": 1048576,
  "max_age_milliseconds": 1000,
  "observation": {
    "observation_id": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    "sensor": "example-fixture/v1",
    "environment_id": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
    "age_milliseconds": 0,
    "capacity": {"status": "available", "cpu_slots": 2, "available_memory_bytes": 4194304}
  }
}
```

The hashes above are fixture identities, not measured hardware evidence. Run a
saved request with the separately bound expected identities:

```sh
elastic-cli admit-capacity \
  --expected-plan-id aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
  --expected-environment-id cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc \
  < request.json
```

The embedding control plane must bind its intended plan and executor environment
independently of incoming observations. The public controller freezes those two
identities at construction; a mismatch produces a rejected report before any
transition. Changing the target requires a new controller. These bindings check
identity equality; sensor authenticity remains the embedding's responsibility.

Production callers
must supply real sensor provenance, environment identity and measurement age.
`unknown` and `unavailable` states instead contain a nonempty `reason` and always
reject. Stale readings and zero effective capacity also reject without actuation.

The planner chooses the minimum of the declared maximum, available CPU slots and
`floor(max(available_memory_bytes - reserve_memory_bytes, 0) / memory_bytes_per_trial)`.
This treats each admitted trial as one CPU slot with a caller-declared memory
envelope. It is an admission policy, not an estimate of actual trial memory or a
physical RAM reservation. Width is limited to 1..256; the freshness bound is
1..60000 milliseconds. Precision, representation, horizon and scientific arms
are not part of this contract and cannot be changed by the controller.

The existing `Runtime::cycle_attempt` executes the trusted state machine against
`TransactionalConcurrency`. In a Rust embedding, acquire and release the handle
returned by `permits()` around real work. Width reductions that would strand
active holders are rejected with runtime events. Existing actuation checks,
post-action verification and rollback remain authoritative; this controller does
not reimplement them. Admission is not a kernel/OS memory quota or isolation.

The CLI starts an empty local permit ledger. A separate process consumer must
enforce the returned width in its own qualified executor, bind it to the same
plan/environment, and recheck freshness at dispatch. The CLI does not resize
another process's pool. TDI's local Hub admission adapter is such a consumer;
remote placement needs node-specific measurements and a separate contract.

Freshness is checked on entry to each library call. Sensor honesty, elapsed time
between sampling and calling, and updates after admission are caller-owned. The
report is an evidence envelope, not an attestation. Retain rejected and failed
decisions; they must never be relabeled as completed work or favorable timing.

Qualification: actual permit acquisition at the new width, reduction blocked by
live holders, successful reduction after release, stale/unknown/unavailable/
insufficient capacity, zero per-trial budget and saturating subtraction. Run:

```sh
cargo test --locked -p memorithm-elastic-runtime capacity_admission
cargo build --locked -p memorithm-elastic-cli
```
