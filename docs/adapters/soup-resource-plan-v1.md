# SOUP run-resource plan v1

Status: EX7 pre-execution ecosystem boundary.

Contract: `elastic.soup.run-resource-plan@1.0.0`

Wire media type: `application/vnd.elastic.soup.run-resource-plan.v1+json`.

Qualified SOUP revision: `05b646523727925990530667e7012ede50bd30b2` (release line v0.73.3).

Related Hub declaration identity: `hub.ml.resource-requirements@1.0.0`.

## Scope

This contract lets ElasticXxx represent and revalidate the resource knobs of a SOUP training run without importing SOUP training semantics or Hub scheduling semantics.

The v1 axes are:

- batch capacity: `training.batch_size = auto | integer >= 1`;
- SOUP-owned auto-batch resolution: `auto | static | probe`;
- optional base-model residency via `training.stream_layers`;
- streaming source: `auto | ram | disk`;
- streaming VRAM buffer count: integer `2..=8`.

SOUP's qualified layer-stream planner explicitly admits `sft`, `dpo`, `orpo`, `simpo`, and `kto`. Streaming is rejected for other tasks by this boundary. Resident runs are not restricted by that streaming allowlist.

The generic Elastic mapping is deliberately narrow:

- batch size -> `capacity`;
- layer streaming -> `residency`;
- external contract identity is preserved by `UpholdContract(elastic.soup.run-resource-plan@1.0.0)`;
- free-capacity and utilization observations may inform a later planner.

## Wire JSON

The stable v1 JSON envelope is strict: unknown fields are rejected during deserialization, and successful deserialization is still not trusted until conversion back through the native validated contract.

Example fixed-batch streaming plan:

```json
{
  "contract": "elastic.soup.run-resource-plan@1.0.0",
  "upstream_commit": "05b646523727925990530667e7012ede50bd30b2",
  "task": "sft",
  "batch_size": {
    "mode": "fixed",
    "value": 1
  },
  "auto_batch_strategy": "auto",
  "streaming": {
    "source": "ram",
    "buffers": 2
  }
}
```

An automatic resident plan uses:

```json
{
  "contract": "elastic.soup.run-resource-plan@1.0.0",
  "upstream_commit": "05b646523727925990530667e7012ede50bd30b2",
  "task": "grpo",
  "batch_size": {
    "mode": "auto"
  },
  "auto_batch_strategy": "probe",
  "streaming": null
}
```

The Rust wire type is `SoupRunResourcePlanWireV1`. `SoupRunResourcePlanV1::to_wire()` emits the qualified identity fields, and `SoupRunResourcePlanWireV1::into_validated()` re-runs the contract/upstream/task/batch/streaming checks before returning a native plan.

## Non-goals and safety boundary

This is not a SOUP config parser, config writer, launcher, or live training actuator. It does not claim that batch size, stream source, or stream buffers can be changed safely in the middle of a training step. V1 is a pre-execution plan boundary only.

It does not own or rewrite quantization, dtype, optimizer, reward, model, dataset, checkpoint, or evaluation semantics. Those remain SOUP-owned. It does not perform Hub worker placement; Hub's published SOUP components currently declare component-preflight enforcement for `hub.ml.resource-requirements@1.0.0`.

Unknown contract identities and unknown SOUP revisions fail closed. A future revision must be reviewed against its actual schema and runtime behavior before this contract is widened.

No Jetson, ARM64, CUDA, throughput, memory-saving, or performance qualification claim is made by this ElasticXxx contract.

## Source facts used for v1

At the qualified SOUP revision:

- `TrainingConfig.batch_size` accepts `auto` or an integer, with integer values required to be at least 1;
- `auto_batch_size_strategy` accepts `auto`, `static`, or `probe`;
- the layer-stream planner defines `MIN_STREAM_BUFFERS=2`, `MAX_STREAM_BUFFERS=8`, `DEFAULT_STREAM_BUFFERS=2`;
- the planner describes the frozen base as living in CPU RAM and streaming one decoder layer at a time through a bounded VRAM pool;
- the explicit streamed-task allowlist is `sft`, `dpo`, `orpo`, `simpo`, `kto`, while rollout tasks such as GRPO/PPO are explicitly excluded from layer streaming.

The duplicated numeric/task guards in ElasticXxx are intentional boundary revalidation, not a second implementation of SOUP's planner.
