# BE14g batch/device Boolean admission — planning slice

Status: **planning-only candidate; no placement actuation or performance claim**.

This slice introduces a bounded Boolean preplanner for declared `(batch size, placement)` candidates. A downstream trusted observer or explicit test provider supplies per-placement available capacity in the exact unit `batch-items`. Placement IDs are opaque stable policy identifiers; ElasticXxx does not discover physical devices or infer topology from them.

For every candidate, capacity is evaluated as `True`, `False`, or `Unknown`. Missing, stale, future, unsupported, non-finite, fractional, negative, over-precise, or unit-mismatched evidence is `Unknown`. `False` is pruned before numeric provider preference ranking. Any `Unknown` blocks selection rather than silently changing placement semantics. Only candidates with complete `True` evidence enter the provider-owned score ranking.

A `Selected` result is explanatory planning evidence only. It cannot reserve a device, change a batch size, dispatch a worker, acquire a lease, or authorize any other actuation. Device/worker orchestration remains downstream/Hub-owned. Later BE14g slices must add a trusted validation/actuation/verification/rollback boundary, a differential non-Boolean baseline, and a portable benchmark before any performance claim.

No claim is made here about GPU availability, memory bytes, throughput, latency, model quality, energy, placement optimality, or end-to-end behavior.

## Durable decision-only trace

The next slice adds `BooleanBatchDeviceDecisionTraceV1`. The trace uses strict bounded JSON, rejects unknown/duplicate fields, future schema versions, oversized inputs and internally inconsistent truth/reason/outcome combinations, and binds the complete declared candidate policy to an opaque provider `source_generation`. The legacy snapshot constructor remains source-compatible and uses generation `0`; providers that have a real generation/epoch should use `new_with_generation`.

`validate_explanatory_context` recomputes the planning decision and requires exact trace equality. This is deliberately replay-like evidence validation, not placement authorization: a decoded or matching trace cannot reserve a device, dispatch work, create a Hub lease/fencing token, or establish that historical capacity is still fresh. The trusted transaction/actuation boundary remains a later BE14g slice.
