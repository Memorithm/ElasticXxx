# ElasticXxx cross-repository compatibility matrix

Status: exact-source pre-release evidence. This matrix records contracts that have actually been qualified; it is not a promise that arbitrary newer revisions remain compatible.

| Consumer/source | Exact qualified source | Elastic boundary | Qualification scope | Explicitly not established |
| --- | --- | --- | --- | --- |
| BooleanLab | `2646e7ca675d3dcde71abed10c7531e1d36c93b8` | immutable BE15a exact-vector snapshot | destination-owned differential checks of generic/compiled/multiword/symbolic Boolean evaluation | runtime dependency, discovery/novelty, actuation authority |
| TDI | `7dab3bfa97e74eeff7965cefcada56c59b4322ab` | BE15b TDI-9.3 non-final representation carrier | differential `True`/`False`/`Unknown` transport on non-final surfaces | preregistration/holdout/final-stage authority, scientific confirmation |
| Forge | `da68e9703d7a523f5d3703ebedd3de85b39cb153` | BE15d bounded candidate guard/constraint intake | Elastic-owned decoding, limits and semantic revalidation | Forge trust, executed evidence, actuation or scientific authority |
| ExtremEngine | merge `51efeb57bd12ee6419cec7e9b530c3b0c485a7a6`, consuming Elastic `8441991feea3a2aae19f62a8e51c89e7f0d6f969` | public `elastic` facade, adaptive fixed-step admission | real consumer metric availability, hysteresis, cooldown, fresh validation, verification, rollback/fault latch; structural scheduler-effect harness | speedup, FPS, GPU, energy, visual-quality or scientific claim |
| SLHAv2 | merge `5fb53928ce2219d7659446a68f41a008eaa1124d`, consuming Elastic `354cfb372f568338b29a357b0671bf9315097b1d` | public `memorithm-elastic` facade; source-bound KV capacity + BE14e fixed-width precision/KV composition | real physical HOT→WARM KV transaction; exact slot-generation/source checks; INT4 4-bit declaration; capacity + precision evidence converge on one KV plan; verified commit and exact reconstructable rollback | GPU residency, NF4/MIXED/TQ3/MIX3 fixed-width precision, performance, memory-product, model-quality or cost claim |
| FLAT-ATTENTION | merge `7a5db9127bd9b76f6f4e58a47a371658f0c8f5e5`, consuming Elastic `354cfb372f568338b29a357b0671bf9315097b1d` | `flat-elastic-kernel` generic capability/evidence/freshness bridge | deterministic FLAT candidate translation, Elastic capability filtering/selection, dispatch-grid rejection and contextual freshness; exact-head cross-platform CI | attention performance, hardware superiority, model quality, execution authority or transfer of FLAT kernel semantics |
| SciRust | merge `00ed3be56685841c42fbdcaa4b5451b73b105b3a`, consuming Elastic `354cfb372f568338b29a357b0671bf9315097b1d` through FLAT `7a5db9127bd9b76f6f4e58a47a371658f0c8f5e5` | host-only `flat-autotune` contextual advisory rail via public `memorithm-elastic` facade | 6 contextual planner tests plus exact-head SciRust CI including Miri, MSRV, Clippy, rustfmt, Rustdoc, ARM64 and cross-platform checks | runtime execution, WGPU-generation unification, performance, model quality, actuation authority or maturity promotion |

## Compatibility rule

A new upstream or downstream commit is not compatible merely because an older pin was qualified. Update the corresponding adapter/fixture or consumer pin explicitly, run destination-owned differential/contract tests, preserve authority boundaries, and require exact-head CI before recording the newer revision here.

The canonical machine-readable pins are in [`PRODUCTIZATION-V1.json`](PRODUCTIZATION-V1.json). The public-semver and MSRV rules are in [`COMPATIBILITY.md`](COMPATIBILITY.md).
