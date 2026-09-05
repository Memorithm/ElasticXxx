# `elastic model-plan`

`elastic model-plan` performs one **non-actuating** model-execution planning decision from strict versioned JSON contracts.

It validates this identity chain before selecting anything:

```text
model-execution capabilities JSON
        |
        v
correlated profile-set JSON
        |
        v
resource-envelope policy JSON
        |
        v
explicit resource snapshot + current profile rank
        |
        v
selected correlated profile / no change / no candidate
```

The command does not load weights, route tokens, resize experts, probe a GPU, or invoke a physical backend.

## Usage

```text
elastic model-plan \
  --capabilities capabilities.json \
  --profiles profiles.json \
  --policy policy.json \
  --capacity-unit bytes \
  --free-capacity 3000 \
  --utilization-bps 8000 \
  --current-profile-rank 0
```

Inputs:

- `--capabilities`: `elastic.model-execution.capabilities@1.0.0` wire document;
- `--profiles`: `elastic.model-execution.profile-set@1.0.0` wire document bound to the exact capabilities fingerprint;
- `--policy`: `elastic.model-execution.envelope-policy@1.0.0` wire document bound to the exact profile-set fingerprint;
- `--capacity-unit`: backend-owned capacity-unit identity; it must exactly match the policy;
- `--free-capacity`: current free capacity in that declared unit;
- `--utilization-bps`: current utilization in integer basis points, `0..=10000`;
- `--current-profile-rank`: currently active provider preference rank, which must be published by the validated profile set.

## Outcomes

The bounded JSON evidence output reports one of:

- `selected`: the policy selected a different qualified correlated profile;
- `no-change`: the policy-selected profile is already current;
- `no-candidate`: no policy rule matches the supplied snapshot.

A malformed/stale contract, capacity-unit mismatch, unpublished current rank, invalid utilization, or defensive `NoFeasibleProfile` condition returns a command error instead of fabricating a plan.

For a selected profile, output includes the matched policy rule and the complete qualified tuple:

- active expert count;
- expert-width basis points;
- activation-budget basis points.

The command emits the normal bounded Elastic runtime-evidence envelope, so the result can be captured and inspected using the existing evidence tooling.

## Contract generation

Do not hand-copy structural fingerprints between documents. Construct and validate `ModelExecutionCapabilitiesV1`, `ModelExecutionProfileSetV1`, and `ModelExecutionEnvelopePolicyV1`, then serialize their respective `to_wire()` values. This ensures each downstream contract is bound to the exact preceding declaration.

Physical execution remains a separate step implemented through `ModelExecutionProfileBackendV1` and `TransactionalModelExecution<B>` by a backend that actually owns the model semantics.
