# Hard Safety vs Learned Operational Safety

## Status

**ELASTIC PROPOSAL / RESEARCH DESIGN NOTE.**

This note synthesizes ElasticXxx's semantic-contract boundary with SafeOpt (Sui et al., 2015) and StageOpt (Sui et al., 2018). It does not claim novelty for safe Bayesian optimization, confidence-bound safety, reachable safe sets, or stagewise safe-region expansion.

## 1. The word `safe` is overloaded

Do not represent all of the following with one boolean:

```text
Type / ownership legality
Semantic correctness
Physical hard limit
Authorization / capability
Operational SLO acceptability
Statistically predicted low risk
Historically observed benign behavior
```

They have different evidence, failure modes and revocation rules.

## 2. Hard admissibility

`HardAdmissible(action, state)` is determined by trusted rules that the learning subsystem cannot relax.

Examples:

- Rust ownership/lifetime constraints;
- semantic contract (`Exact`, bounded approximation, etc.);
- representation compatibility;
- required capabilities/attestations;
- absolute device limits;
- protocol invariants;
- explicit operator policy prohibitions.

Candidate rule:

```text
Learned evidence may REMOVE actions from HardAdmissibleSet.
Learned evidence may NEVER ADD an action outside HardAdmissibleSet.
```

## 3. Operational safety

Operational safety is contextual and may be uncertain.

Examples:

- probability of missing a latency SLO;
- thermal margin under a workload;
- likelihood of queue instability;
- expected error rate under a routing change;
- probability that a migration finishes before a deadline;
- confidence that a diagnostic probe will not cause unacceptable disruption.

A model may estimate these quantities from data.

## 4. Two nested sets

```text
AllRepresentableActions
        ⊇
HardAdmissibleSet(state)
        ⊇
OperationallyCertifiedSet(state, model_epoch, policy)
```

The last set is model- and policy-dependent.

This nesting is important. A SafeOpt-like algorithm reasons only inside the hard-admissible set supplied by ElasticXxx.

## 5. Learned safe-set reachability

SafeOpt shows that under uncertainty, the set that can be **established** safe from an initial trusted seed can be smaller than the true safe set.

ElasticXxx should preserve this distinction:

```text
TrueOperationallyAcceptableSet      unknown
CertifiedOperationalSet_t          known under model/confidence assumptions
ReachableCertifiedSet_t            safely discoverable from current evidence/policy
```

Failure to certify an action does not prove it is unsafe.

## 6. Certificate shape

Candidate evidence record:

```text
OperationalSafetyCertificate {
    subject,
    model_id,
    model_epoch,
    observation_epoch,
    environment_fingerprint,
    assumptions,
    constraint_estimates,
    lower_or_upper_confidence_bounds,
    confidence_policy,
    valid_region,
    issued_at,
    invalidation_conditions,
}
```

This record is **planner evidence**, not a capability token and not a hard-safety proof.

## 7. Staleness and revocation

SafeOpt's mathematical analysis assumes one underlying function satisfying the model assumptions. Runtime systems may be nonstationary.

Therefore an Elastic operational certificate should be invalidated or downgraded when relevant context changes, for example:

```text
workload phase
hardware/topology
power cap
software/model version
representation epoch
concurrent tenants
observation distribution
safety-model version
```

An old certificate must not silently authorize a new environment.

## 8. Safety and utility are separate models

StageOpt explicitly separates unknown utility and unknown safety functions.

ElasticXxx should therefore avoid a universal scalar such as:

```text
score = performance - lambda * safety
```

for hard constraints.

Candidate ordering:

```text
1. reject hard-illegal actions
2. reject / quarantine operationally uncertified actions according to policy
3. optimize useful progress among the remaining candidates
```

Soft risk can enter the objective only after hard validity.

## 9. Safety-learning phase versus utility phase

Possible planner states:

```text
CertifyOperationalSafety
Diagnose
ExpandKnownSafeRegion
OptimizeUtility
VerifyModel
Fallback
```

Unlike fixed two-stage StageOpt, ElasticXxx may switch phases according to control deadline, uncertainty, cost and workload persistence.

## 10. Safe seed / fallback

A learned exploration backend generally needs a starting region whose safety is established by evidence stronger than the learner's own extrapolation.

Possible sources:

- static baseline configuration;
- operator-certified plan;
- previously validated replay plan under matching environment fingerprint;
- analytical hard bound;
- conservative runtime mode.

If no trusted seed exists, a backend that requires one must return `Unavailable`, not invent safety.

## 11. Stateful transitions

SafeOpt's bandit formulation explicitly avoids persistent state transitions. Elastic actions do not.

Therefore candidate safety must consider trajectories:

```text
state_before
   -> transition path
   -> transient state(s)
   -> settled state
```

It is insufficient for only the final state to look safe.

Candidate predicate:

```text
OperationallyCertifiedTransition(path | context)
```

rather than only `OperationallyCertifiedState(target)`.

## 12. Failure semantics

Possible outcomes of operational certification:

```text
Certified
UncertifiedInsufficientEvidence
PredictedUnsafe
ModelOutOfDomain
CertificateStale
ModelInvalid
```

Do not collapse `UncertifiedInsufficientEvidence` into `Unsafe`.

## 13. Relationship to trusted attestations

`TransitionAttestations` in the first executable Elastic slice concern trusted-boundary claims needed for structural transition validation.

`OperationalSafetyCertificate` is different:

```text
TransitionAttestation
    trusted structural declaration consumed by validator

OperationalSafetyCertificate
    uncertain empirical/model evidence consumed by planner/policy
```

The second must not be convertible into the first without an explicit trusted validation procedure.

## 14. Research hypotheses

### H11 — Nested Safety Domains

Separating hard admissibility from learned operational certification prevents uncertain models from weakening semantic safety while still enabling conservative online exploration.

**Status: DESIGN HYPOTHESIS / EXPERIMENT REQUIRED.**

### H12 — Revocable Operational Safety

Versioned, context-bound operational certificates can reduce repeated exploration cost without reusing stale safety evidence after material environment changes.

**Status: HYPOTHESIS / EXPERIMENT REQUIRED.**

## 15. Evaluation

Inject an operational threshold unknown to the controller and compare:

1. unconstrained exploration;
2. hard constraints only;
3. SafeOpt-like certification inside hard constraints;
4. certification with environment/version invalidation.

Then change workload/topology mid-run.

Measure:

- hard-invariant violations (must remain zero);
- operational threshold violations;
- useful region discovered;
- false `Unsafe` vs `Uncertified` classifications;
- stale-certificate reuse;
- time/control cost to recover a certified region after environment change.
