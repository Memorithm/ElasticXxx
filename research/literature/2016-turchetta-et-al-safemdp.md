# Turchetta, Berkenkamp & Krause 2016 — Safe Exploration in Finite Markov Decision Processes with Gaussian Processes (SafeMDP)

## Classification

**ADAPT** for ElasticXxx stateful diagnostic experimentation and recovery planning.

## Evidence status

- **SOURCE-DERIVED** unless explicitly marked otherwise.
- Primary source: Matteo Turchetta, Felix Berkenkamp, Andreas Krause, NeurIPS 2016.
- PDF text was inspected in detail. Screenshot retrieval was attempted for the reachability/returnability and algorithm pages but failed with a source cache miss; no visual-only claim is made.

## Problem

SafeOpt considers a bandit-style decision problem where any decision can be sampled directly and decisions do not induce persistent state transitions.

SafeMDP instead considers a finite deterministic MDP. The next experiment/action is constrained by the current state and transition graph.

The unknown safety feature must remain above threshold at every visited state/action.

## Key structural addition

A state is not sufficient for safe exploration merely because its safety value can be certified above threshold.

The paper combines three conditions:

```text
R_safe(S)     states classified above the safety threshold
R_reach(S)    states reachable from the current safe region
R_ret(S)      states from which a safe path can return to the prior safe region
```

The safely explorable set is their intersection, conceptually:

```text
SafeExplorable(S)
    = SafetyCertified(S)
      ∩ Reachable(S)
      ∩ SafelyReturnable(S)
```

This prevents the explorer from entering an apparently safe state that has no safe route out.

## Returnability

Returnability is a path property, not necessarily an inverse-action property.

The paper defines one-step returnability and repeatedly applies it to capture multi-step routes through safe states back to a reference safe set.

This is stronger than evaluating the endpoint in isolation.

## SafeMDP algorithm

The method maintains:

```text
S_t       states classified as satisfying the safety constraint
S_hat_t   states that are additionally reachable and safely returnable
G_t       candidate safe expanders
```

It samples an uncertain expander and navigates to it along a safe path, then updates the GP safety model from the observation.

## Assumptions / guarantee scope

The analyzed setting assumes, among other conditions:

- finite state/action space;
- known deterministic transition model;
- initially safe seed region;
- unknown safety feature satisfying GP/RKHS and Lipschitz-style regularity assumptions;
- noisy safety measurements.

Under the stated assumptions, the paper proves high-probability safe exploration of the safely reachable region without getting stuck.

These assumptions do not describe arbitrary distributed resource runtimes.

## Elastic relation

### ADOPT

- reason about **transition paths**, not only target states;
- distinguish reachability from safety;
- require a recovery/returnability argument for exploratory actions when policy demands recoverability;
- retain a trusted safe region/fallback set.

### ADAPT

ElasticXxx needs richer notions than the deterministic finite-MDP model:

```text
Reachable(target | state, capabilities)
OperationallyCertified(path | model, context)
Recoverable(target -> SafeRegion | state, context)
```

with uncertain/asynchronous transitions and partial failure.

### REJECT

Do not require every production transition to be exactly reversible.

Do not equate SafeMDP returnability with `rollback`.

An irreversible transition may still be legal when the semantic contract allows it and a safe forward recovery/compensation path exists.

## Elastic proposal: reversal versus recovery

Separate:

```text
Reversible
    exact inverse or restoration of the prior logical state

RollbackCapable
    transaction can be aborted/reverted before commit

Compensatable
    another operation restores required invariants/effect without exact inverse

Recoverable
    there exists an admissible path to a declared safe operating region

Returnable
    special case of Recoverable where the path returns to a previous/reference safe set
```

These are not synonyms.

## Elastic proposal: RecoveryEnvelope

Candidate planning object:

```text
RecoveryEnvelope {
    safe_region,
    recovery_paths,
    path_preconditions,
    max_recovery_cost,
    max_recovery_time,
    required_capabilities,
    operational_certificates,
    validity_epoch,
}
```

An exploratory transition may be rejected if it would consume or invalidate the only known recovery path.

## Transition-path validation

Candidate sequence:

```text
Candidate transition
    ↓
Hard legality of target
    ↓
Hard legality of transition path
    ↓
Operational certification of transient states
    ↓
Recovery-envelope validation
    ↓
Execute
```

Endpoint-only safety is insufficient.

## Important distributed-systems consequence

**ELASTIC PROPOSAL / INFERENCE:** resource actions may alter their own future recoverability.

Examples include:

- releasing the last replica that enables rollback;
- migrating state across a link and then saturating that link;
- consuming memory needed for restoration;
- changing topology/routing so the prior node is no longer reachable;
- dropping a shadow copy after a transactional migration.

Therefore recovery capability belongs to state, not merely to the transition kind.

## Experiment required

Create a resource-state graph containing:

1. safe target states with no safe return path;
2. reversible transitions;
3. irreversible-but-compensatable transitions;
4. actions that consume their own recovery reserve.

Compare endpoint-only validation against recovery-envelope-aware planning. Measure invariant violations, stranded states, recovery success, useful reachable region and conservative false rejections.
