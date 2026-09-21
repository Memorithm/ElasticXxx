# SML elastic-weight plan adapter v1

Status: qualified representation/resource boundary. This adapter performs no physical re-encoding, page transfer, GPU action, quality verification, or performance optimization.

## Qualified source

ElasticXxx consumes the SML contract:

- repository: `Memorithm/SML-GENIUS`
- contract: `sml.elastic-weight-plan@1.0.0`
- qualified merged revision: `3e04239862c9d14cb369227f4abbe84b5c7e1d5e`

Unknown source revisions and contract identifiers fail closed.

## Ownership boundary

SML owns:

- logical weight-page identity;
- learned parameter counts;
- Boolean / ternary / residual4 page semantics;
- importance and selected-page policy;
- the source plan and its generation.

ElasticXxx owns:

- independent validation of the accepted contract;
- generic representation transition validation;
- capability and evidence checks before any future actuation;
- the generic OBSERVE -> FORECAST -> PLAN -> VALIDATE -> ACT -> VERIFY -> COMMIT / ROLLBACK runtime.

The adapter does not import SML as a runtime dependency and does not reproduce SML's planner.

## Independent validation

`SmlElasticWeightPlanV1::validate` recalculates from page transitions:

- target payload bytes;
- RAM and VRAM payload totals;
- active learned parameters;
- active semantic storage bits;
- bytes moving to RAM and VRAM;
- precision-change count;
- representation-version progression.

It also rejects duplicate page identifiers, zero-sized pages, active pages targeting disk, aggregate tampering, and declared budget overflow.

## Representation transition mapping

A page precision change can be projected into ElasticXxx's existing `RepresentationTransition` model.

The adapter never invents a materialization mechanism:

- `Reinterpret` is rejected for SML precision changes;
- `Reencode` requires the generic reencoder attestation;
- `Recompute` requires the generic trusted-source attestation;
- the target representation must be present in the trusted capability set.

Residency-only changes return no representation transition because residency and physical representation are separate adaptive axes.

## Current non-claims

This slice does not establish:

- a live SML page mover;
- a Boolean/ternary/residual4 codec;
- a master-weight reconstruction source;
- GPU or disk transfer correctness;
- post-actuation model-quality preservation;
- memory, latency, energy, throughput, or quality gains.

Those require a specialized backend implementing physical ACT/VERIFY/ROLLBACK against this validated boundary.
