# Berkenkamp et al. 2017 — Safe Model-based Reinforcement Learning with Stability Guarantees

## Classification

**ADAPT / INVESTIGATE** for ElasticXxx dynamic-resource safety envelopes.

## Evidence status

- **SOURCE-DERIVED** unless explicitly marked otherwise.
- Primary source: Felix Berkenkamp, Matteo Turchetta, Angela P. Schoellig, Andreas Krause, NeurIPS 2017.
- PDF text was inspected in detail. Screenshot retrieval was attempted on the Lyapunov/region-of-attraction pages but failed with a source cache miss; no visual-only claim is made.

## Problem

Safely learn and improve a control policy for a continuous-state/action dynamical system while guaranteeing stability during learning.

The unknown part of the dynamics is learned from data, but policy exploration is restricted so that the closed-loop system remains inside a certified region of attraction.

## Dynamics

The paper considers a deterministic discrete-time system:

```text
x_{t+1} = f(x_t, u_t) = h(x_t,u_t) + g(x_t,u_t)
```

where:

- `h` is a known prior model;
- `g` is an unknown model error learned statistically.

A policy `pi(x)` selects actions.

## Safety definition

Safety is defined in terms of asymptotic stability / region of attraction.

For a fixed policy, a region of attraction is a state subset such that trajectories starting inside it:

1. remain inside it for all future times (forward invariance);
2. converge to the target equilibrium.

This is stronger than merely having some safe recovery path.

## Lyapunov certificate

A Lyapunov function `v(x)` is used to certify the region of attraction.

The key decrease condition is conceptually:

```text
v(f(x, pi(x))) < v(x)
```

throughout a level set `V(c)`.

If this holds under the theorem's assumptions, the level set is a region of attraction.

## Learned dynamics + confidence

Because the true dynamics are uncertain, the paper constructs confidence bounds on the next-state / Lyapunov value from a calibrated statistical model.

The analysis assumes a model whose confidence intervals contain the true dynamics with high probability, plus Lipschitz regularity.

The paper notes that for safety, the statistical model need not specifically be a GP if another well-calibrated model supplies the required confidence property; GP assumptions are used for the exploration result.

## Initial safe policy

Learning starts from an initial controller `pi_0` that is already known to stabilize a small region around the equilibrium.

Exploration then collects informative data only inside the certified region, improves the dynamics model, updates the policy, and expands the estimated region of attraction.

## Policy-dependent safety

A central lesson is:

```text
SafeRegion != property of state alone
SafeRegion = property of state + dynamics + policy + certificate/model
```

Changing the policy can change the region that is safe.

## Guarantee scope / assumptions

The guarantee is conditional on assumptions including:

- deterministic discrete-time dynamics in the analyzed formulation;
- Lipschitz dynamics and policies;
- a suitable Lyapunov function supplied/constructed;
- calibrated statistical error bounds;
- initial locally stabilizing policy;
- discretization / approximation conditions used to verify the continuous region.

Finding a good Lyapunov function is itself nontrivial and is not eliminated by the method.

## Elastic relation

### ADOPT

- represent some forms of operational safety as **invariant regions**, not endpoint predicates;
- bind safety evidence to the controller/policy that generated it;
- keep an initial validated fallback controller/operating mode for learned expansion;
- use model uncertainty explicitly in any learned dynamic-safety certificate.

### ADAPT

ElasticXxx should not require Lyapunov structure for every resource.

It should permit domain-specific certificate mechanisms such as:

```text
ThresholdCertificate
ReachabilityCertificate
RecoveryEnvelope
StabilityCertificate
RobustMpcCertificate
QueueStabilityCertificate
ThermalEnvelopeCertificate
```

with common provenance/version/invalidation semantics.

### REJECT

Do not claim that a generic resource state is "stable" unless a domain defines the dynamics, target set and stability notion.

Do not equate a learned high-probability region-of-attraction estimate with a hard type/semantic invariant.

## Elastic proposal: policy-bound operational certificate

```text
OperationalSafetyCertificate {
    subject_region,
    controller_or_policy_id,
    dynamics_model_id,
    model_epoch,
    certificate_method,
    assumptions,
    confidence,
    environment_fingerprint,
    validity_region,
    invalidation_conditions,
}
```

Changing controller/policy ID invalidates a certificate unless the certificate explicitly covers the new policy family.

## Recovery versus invariance

```text
Recoverable state:
    there exists an admissible path to a safe region

Forward-invariant safe region:
    under the declared policy, every future trajectory remains inside the region

Region of attraction:
    forward invariant + converges to a declared target/equilibrium
```

These form increasingly strong guarantees and should not share one flag.

## Candidate resource applications

**ELASTIC PROPOSAL / INVESTIGATE:** stability-style certificates may be useful for domains with meaningful dynamics, for example:

- queue/backlog control;
- thermal management;
- power-control loops;
- memory-pressure feedback;
- rate/congestion controllers.

They are probably inappropriate for many discrete representational transitions.

## SciRust implication

Current `scirust-control` exposes PID, LQR, box-constrained QP and linear MPC with certified input-constraint satisfaction, but the inspected public API does not expose Lyapunov stability verification or region-of-attraction analysis.

Keep a candidate `SCIRUST-GAP-CONTROL` for Lyapunov / invariant-region analysis under **INVESTIGATE** until the wider SciRust tree and representative control literature are audited. Do not implement from this paper alone.

## Experiment required

Build one dynamic-resource plant (e.g. queue or thermal model) and compare:

1. endpoint threshold policy;
2. recovery-envelope policy;
3. invariant-region/stability-aware policy.

Introduce model mismatch and abrupt disturbances. Measure violations, recovery success, conservatism, controller cost and how quickly stale certificates are invalidated.
