# ElasticXxx cross-repository compatibility matrix

Status: exact-source pre-release evidence. This matrix records contracts that have actually been qualified; it is not a promise that arbitrary newer revisions remain compatible.

| Consumer/source | Exact qualified source | Elastic boundary | Qualification scope | Explicitly not established |
| --- | --- | --- | --- | --- |
| BooleanLab | `2646e7ca675d3dcde71abed10c7531e1d36c93b8` | immutable BE15a exact-vector snapshot | destination-owned differential checks of generic/compiled/multiword/symbolic Boolean evaluation | runtime dependency, discovery/novelty, actuation authority |
| TDI | `7dab3bfa97e74eeff7965cefcada56c59b4322ab` | BE15b TDI-9.3 non-final representation carrier | differential `True`/`False`/`Unknown` transport on non-final surfaces | preregistration/holdout/final-stage authority, scientific confirmation |
| Forge | `da68e9703d7a523f5d3703ebedd3de85b39cb153` | BE15d bounded candidate guard/constraint intake | Elastic-owned decoding, limits and semantic revalidation | Forge trust, executed evidence, actuation or scientific authority |
| ExtremEngine | merge `51efeb57bd12ee6419cec7e9b530c3b0c485a7a6`, consuming Elastic `8441991feea3a2aae19f62a8e51c89e7f0d6f969` | public `elastic` facade, adaptive fixed-step admission | real consumer metric availability, hysteresis, cooldown, fresh validation, verification, rollback/fault latch; structural scheduler-effect harness | speedup, FPS, GPU, energy, visual-quality or scientific claim |

## Compatibility rule

A new upstream or downstream commit is not compatible merely because an older pin was qualified. Update the corresponding adapter/fixture or consumer pin explicitly, run destination-owned differential/contract tests, preserve authority boundaries, and require exact-head CI before recording the newer revision here.

The canonical machine-readable pins are in [`PRODUCTIZATION-V1.json`](PRODUCTIZATION-V1.json). The public-semver and MSRV rules are in [`COMPATIBILITY.md`](COMPATIBILITY.md).
