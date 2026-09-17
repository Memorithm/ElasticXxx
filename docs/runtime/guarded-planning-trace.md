# Integrated guarded planning trace

Status: implemented by `elastic-runtime` for Boolean roadmap slice BE8c.

The integrated trace binds the Boolean pruning decision to the exact numeric planning call **without replaying the planner and without performing actuation**. It complements, rather than changes, the already-versioned `elastic-boolean-decision-trace-v1` wire contract.

## Planning boundary

`BooleanGuardPlanner::propose_transition_detailed_with_context` performs the ordinary guarded planning operation once and retains:

- the final `PlanOutcome`;
- the exact source-bound `TransitionPruningReport`;
- `PlanningContextFingerprint`, a structural identity of the numeric observation map.

The legacy `propose_transition_with_context` delegates to this method and discards the extra evidence, so both surfaces share one semantic implementation.

## Identities

`TransitionPruningReport::fingerprint` absorbs the original guarded-resource fingerprint and the complete eligible/rejected/unknown partition, including guard scopes and capability grounding. Built-in and custom dimension terms are explicitly discriminated. An unbound/default report has no fingerprint.

`planning_context_fingerprint` iterates the `PlanningContext` in canonical signal order, distinguishes built-in from custom observation terms, and absorbs each `f64::to_bits()` value. Therefore ordering cannot change identity, while `+0.0` and `-0.0` remain observably distinct inputs.

Both fingerprints are non-cryptographic diagnostics. Neither authenticates a source or authorizes actuation.

## Integrated capture

`capture_guarded_planning_trace` accepts an already-produced `GuardedPlanningDecision` and verifies that:

- the fact snapshot is still fresh and bound to the same logical resource;
- the retained pruning report belongs to the exact original `EirGuardedResource`;
- the supplied numeric context fingerprints exactly as the context used during planning;
- a final selected candidate remains in the retained Boolean-eligible set.

It then builds the ordinary `DecisionTrace` directly from the retained pruning report. It does **not** call the Boolean preplanner or numeric planner again.

The outer `GuardedPlanningTrace` records the exact final outcome as one of `Candidate`, `NoCandidate`, `Unsupported`, or `InsufficientEvidence`. This separate outcome is necessary because the v1 `DecisionTrace::stop_reason` describes the lower-level Boolean partition; for example, an exact target may be unsupported while another resource transition remains Boolean-eligible.

## Invariant precheck summary

Capture also evaluates the existing pure `precheck_plan_invariants` against the same resource, numeric context, facts and outcome. The trace stores only a non-authoritative summary: status plus true/false/unknown counts.

A summary status of `Passed` means only **continue to trusted validation**. It is not a `ValidatedPlan`, does not replace `validate_with_checks`, and cannot authorize an adapter call.

## Side-effect contract

Integrated capture performs no adapter call, actuation, verification, commit or rollback. Tests use a counting numeric planner and prove that the call count cannot increase during capture for `Candidate`, `NoCandidate`, `Unsupported`, or `InsufficientEvidence` outcomes.
