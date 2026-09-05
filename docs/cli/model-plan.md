# `elastic model-plan`

`elastic model-plan` performs one **non-actuating** model-execution planning decision from strict versioned JSON contracts.

The preferred input is one validated aggregate controller-contract bundle:

```text
model-execution controller-contracts JSON
        |
        v
capabilities -> correlated profiles -> envelope policy revalidation
        |
        v
explicit resource snapshot + current profile rank
        |
        v
selected correlated profile / no change / no candidate
```

The historical three-file capabilities/profile-set/policy form remains supported and is validated through the same native controller-contract bundle before planning.

The command does not load weights, route tokens, resize experts, probe a GPU, or invoke a physical backend.

## Preferred usage

```text
elastic model-plan \
  --contracts model-controller-contracts.json \
  --capacity-unit bytes \
  --free-capacity 3000 \
  --utilization-bps 8000 \
  --current-profile-rank 0
```

`--contracts` expects `elastic.model-execution.controller-contracts@1.0.0`. The aggregate contains the strict v1 capabilities, correlated profile set, and envelope policy and revalidates their complete identity/fingerprint chain when loaded.

## Historical split usage

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

The contract source modes are exclusive. `--contracts` cannot be combined with any split-contract option. Without `--contracts`, all three of `--capabilities`, `--profiles`, and `--policy` are required.

Inputs:

- `--contracts`: preferred aggregate `elastic.model-execution.controller-contracts@1.0.0` document;
- `--capabilities`: historical `elastic.model-execution.capabilities@1.0.0` wire document;
- `--profiles`: historical `elastic.model-execution.profile-set@1.0.0` wire document bound to the exact capabilities fingerprint;
- `--policy`: historical `elastic.model-execution.envelope-policy@1.0.0` wire document bound to the exact profile-set fingerprint;
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

Prefer constructing `ModelExecutionControllerContractsV1` from validated `ModelExecutionProfileSetV1` and `ModelExecutionEnvelopePolicyV1`, then serialize it with `to_pretty_json()`. The aggregate uses the existing nested `to_wire()` contracts and avoids manual fingerprint copying.

The historical split documents should likewise be generated through their typed `to_wire()` APIs rather than editing structural fingerprints manually.

Physical execution remains a separate step implemented through `ModelExecutionProfileBackendV1` and `ModelExecutionControllerV1` by a backend that actually owns the model semantics.
