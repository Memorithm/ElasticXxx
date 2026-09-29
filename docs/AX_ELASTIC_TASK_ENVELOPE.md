# AX-inspired elastic task envelope

ElasticXxx owns adaptive resource policy for declared tasks. It does not own
task identity, source provenance or backend containment.

## Envelope

A task may expose an elastic envelope containing:

- invariant identity and provenance requirements;
- admissible CPU, RAM, GPU, concurrency, token and energy dimensions;
- objective priorities and hard limits;
- observation schema and forecast horizon;
- allowed actuation mechanisms;
- rollback and verification requirements.

The envelope is a policy input, not permission to exceed a backend capability.

## Control loop

```text
OBSERVE → FORECAST → PLAN → VALIDATE → ACT → VERIFY → COMMIT / ROLLBACK
```

Every plan must retain the task identity, exact workspace identity, capability
snapshot, objective revision and evidence references. If a remote action may
have succeeded but its acknowledgement is lost, ElasticXxx must emit an
ambiguous result and require reconciliation instead of issuing a duplicate
actuation.

## AX boundary

AX-inspired Task/Workspace/Model declarations are consumed as typed input.
RemoteOps remains authoritative for host enforcement. SciRust Hub remains
authoritative for orchestration and provenance. ElasticXxx may select only
plans that are explicitly admitted by both.

No performance, safety or sandbox claim follows from this document. Those
claims require backend-specific evidence and independent benchmarks.


## RemoteOps host inventory v2

The public facade exposes a strict wire contract for
remoteops-sandbox host-resource inventory schema version 2. Consumers deserialize
the wire DTO, then call its validation method before using the observation.
Unknown fields, omitted nullable fields, and unsupported versions fail closed.
Unknown limits remain distinct from explicitly unbounded limits.

The contract preserves host logical CPU count and total memory separately from
the cgroup CPU quota and memory limit. Those values describe observations and
ceilings; they are not free capacity, a placement decision, or proof of
resource enforcement. GPU, network and runtime qualification remain outside
this v2 contract.


## AXE-1 typed task resource envelope

The public facade exposes a native Rust
TaskResourceEnvelopeV1 binding an opaque caller-owned task reference and
workspace reference to optional CPU, memory, wall-clock, GPU, model-token and
concurrency bounds. References are preserved exactly and validated as bounded,
non-secret labels. A zero GPU count is an explicit CPU-only request; other
specified numeric bounds must be positive.

The envelope records caller intent only. It does not own task identity, prove
host capacity, choose a worker, or authorize actuation.
