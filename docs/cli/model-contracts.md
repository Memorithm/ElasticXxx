# `elastic model-contracts`

`elastic model-contracts` manages the strict persisted contract bundle consumed by `elastic model-plan --contracts` and `ModelExecutionControllerV1::current_state_from_contracts(...)`.

The command never actuates a model and never probes hardware.

## Build

```text
elastic model-contracts build \
  --capabilities capabilities.json \
  --profiles profiles.json \
  --policy policy.json \
  --output model-controller-contracts.json
```

`build` parses the three historical v1 wire contracts and constructs a `ModelExecutionControllerContractsWireV1`. It then runs the complete semantic validation chain:

```text
capabilities
    -> correlated profile set + capability identity/fingerprint
    -> envelope policy + capability/profile-set identity/fingerprints
    -> aggregate controller-contract bundle
```

Only after that chain succeeds is the aggregate JSON materialized.

The output path is opened with create-new semantics. An existing file is never silently replaced.

The written file is raw reusable `elastic.model-execution.controller-contracts@1.0.0` JSON, not a runtime-evidence envelope.

## Validate

```text
elastic model-contracts validate model-controller-contracts.json
```

`validate` performs full aggregate and nested semantic revalidation. On success it emits the normal bounded Elastic runtime-evidence envelope containing a non-actuating summary:

- provider id;
- exact model revision;
- capability fingerprint;
- profile-set fingerprint;
- policy fingerprint;
- capacity-unit identity;
- correlated profile count;
- envelope-rule count.

Malformed JSON, unknown fields, unsupported contract versions, stale fingerprints, provider/model mismatches, invalid profile tuples, or invalid policy rules fail closed.

## Use with `model-plan`

After building the bundle:

```text
elastic model-plan \
  --contracts model-controller-contracts.json \
  --capacity-unit bytes \
  --free-capacity 3000 \
  --utilization-bps 8000 \
  --current-profile-rank 0
```

This replaces the need to pass `--capabilities`, `--profiles`, and `--policy` separately. The historical split syntax remains supported for compatibility.

## Scope

The bundle contains declarations only. It does not contain:

- weights;
- device handles;
- a physical `ModelExecutionProfileBackendV1`;
- a telemetry provider;
- credentials or secrets;
- live runtime state.

Physical execution still requires an explicitly integrated backend and remains governed by the transactional validate/actuate/verify/commit-or-rollback path.
