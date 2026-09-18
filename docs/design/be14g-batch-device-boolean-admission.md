# BE14g batch/device Boolean admission — planning slice

Status: **planning-only candidate; no placement actuation or performance claim**.

This slice introduces a bounded Boolean preplanner for declared `(batch size, placement)` candidates. A downstream trusted observer or explicit test provider supplies per-placement available capacity in the exact unit `batch-items`. Placement IDs are opaque stable policy identifiers; ElasticXxx does not discover physical devices or infer topology from them.

For every candidate, capacity is evaluated as `True`, `False`, or `Unknown`. Missing, stale, future, unsupported, non-finite, fractional, negative, over-precise, or unit-mismatched evidence is `Unknown`. `False` is pruned before numeric provider preference ranking. Any `Unknown` blocks selection rather than silently changing placement semantics. Only candidates with complete `True` evidence enter the provider-owned score ranking.

A `Selected` result is explanatory planning evidence only. It cannot reserve a device, change a batch size, dispatch a worker, acquire a lease, or authorize any other actuation. Device/worker orchestration remains downstream/Hub-owned. Later BE14g slices must add a trusted validation/actuation/verification/rollback boundary, durable decision-trace binding, differential non-Boolean baseline, and portable benchmark before any performance claim.

No claim is made here about GPU availability, memory bytes, throughput, latency, model quality, energy, placement optimality, or end-to-end behavior.
