# CLI evidence, replay and diff

Every JSON-producing `elastic` command emits the additive
`evidence_schema: "elastic-runtime-evidence-v1"` field. The record can be
captured directly from standard output and inspected later without access to
the original resource.

## Capture and replay

```text
elastic doctor default > doctor.json
elastic replay doctor.json
```

`replay` parses the record, checks that every resource identifier and runtime
event has the expected shape, and prints a compact summary. It is strictly
read-only: it does not construct an adapter, call a planner, or actuate a
resource. This makes it safe for offline audit and CI evidence checks.

The current validator accepts records emitted by `inspect`, `observe`, `plan`,
`doctor`, `validate`, `apply`, `run`, `watch`, and `explain`. An event record
contains a non-empty `kind` and string `details`; malformed records fail
closed.

## Deterministic comparison

```text
elastic diff before.json after.json
```

`diff` validates both inputs and reports JSON paths that changed. Object key
ordering is ignored, while array order remains significant because cycle and
event order carries meaning. At most 512 changed paths are reported; the
`truncated` flag makes a larger difference explicit.

The command does not infer whether a change is beneficial. It only compares
the captured evidence so policy and scientific interpretation remain separate
from the transport format.
