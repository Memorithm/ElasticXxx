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
