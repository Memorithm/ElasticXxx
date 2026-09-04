# ElasticXxx Hub runtime process v1

## Purpose

`elastic.hub.run@1.0.0` is the process boundary for orchestrators that need to execute an existing ElasticXxx operator configuration and retain the resulting runtime evidence as an immutable artifact.

This contract does not create a second runtime. `elastic hub-run` uses the same versioned `OperatorConfig`, controller materialization, planner, forecaster, validation, actuation, verification, commit, rollback, and event surfaces as `elastic run --config`.

## Invocation

```text
elastic hub-run \
  --config <operator-config-v1.json> \
  [--resource <resource-id>] \
  --evidence-output <new-runtime-evidence.json>
```

The deployment path is selected by the orchestrator. A Hub installation may pin the executable at `/opt/scirust-hub/libexec/elastic`.

## Input

`--config` is one versioned ElasticXxx operator configuration defined by `docs/config/schema-v1.md`.

When `--resource` is absent, configured controllers execute in canonical resource-id order. Each controller remains an independent transaction boundary. The command does not claim cross-resource atomicity.

When `--resource` is present, it must identify a controller declared in the supplied operator configuration.

## Output

`--evidence-output` materializes one bounded JSON document with:

- schema: `elastic-runtime-evidence-v1`
- media type: `application/vnd.elastic.runtime-evidence.v1+json`
- size bound: `MAX_EVIDENCE_BYTES` from the public runtime evidence contract
- source command: `run`
- observation, forecast, plan, validation, actuation, verification, commit/rollback, stop and final-state records represented by the existing runtime evidence contract

The output path must not already exist. The process uses exclusive creation and fails closed rather than replacing an existing artifact.

## Ownership and semantics

ElasticXxx remains authoritative for:

- operator configuration validation;
- adaptive policy and resource objectives;
- physical actuation boundaries;
- post-actuation verification;
- commit and rollback decisions;
- runtime evidence semantics.

An orchestrator such as SciRust Hub may invoke the process and retain the resulting artifact, but it must not reinterpret a runtime `COMMIT` or `ROLLBACK` as its own resource-policy decision.

SciRust-Verify may ingest the evidence through a separately versioned adapter, but a Verify dossier verdict must remain distinct from ElasticXxx's source runtime decision.

## Non-claims

This process contract does not establish:

- OS sandboxing or hostile-code isolation;
- multi-host or distributed resource coordination;
- resource-aware Hub worker placement;
- cross-resource transaction atomicity;
- model-quality or performance superiority;
- hardware portability;
- ElasticXxx ML maturity 5/5.

Those require separate executed evidence and contracts.
