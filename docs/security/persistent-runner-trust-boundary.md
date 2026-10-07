# Persistent runner trust boundary

ElasticXxx uses persistent ARM64 and Jetson Thor runners for repository-owned
qualification that cannot be reproduced on a GitHub-hosted runner. Pull request
workflows therefore enforce the following boundary:

- workflow definitions are loaded from the default branch with
  `pull_request_target`;
- persistent-runner jobs are allocated only for pull requests whose head
  repository is `Memorithm/ElasticXxx`;
- each validation checkout names the pull request head repository and exact
  head SHA explicitly and disables persisted credentials;
- fork-originated pull requests use the hosted `fork-validation` job for every
  portable gate; and
- Thor thermal and INA238 checks are hardware qualifications, not hosted-runner
  security substitutes.

Runner labels, job-local Cargo directories, and bounded timeouts are
defense-in-depth. They do not make execution of arbitrary fork code safe on a
persistent host. The trusted default-branch workflow and the same-repository
condition are the controls that prevent that allocation.

## Change and validation procedure

Any change to this boundary must update this document and the affected workflow
in the same pull request. The path filters include this document so the
installed default-branch workflows exercise their policy when the boundary is
reviewed. Evidence must identify the exact pull request head SHA, the workflow
runs that executed it, and any hardware-only gate that was intentionally not
represented by a hosted run.

This policy does not assert that a fork was publicly executable, that GitHub's
approval settings were bypassed, or that a runner was compromised. Those claims
require separate evidence.
