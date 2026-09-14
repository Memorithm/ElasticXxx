//! Plan-bound, word-parallel aggregation of invariant prechecks.
//!
//! This is an optional compiled form of the scalar precheck, not an invariant
//! validator. Predicate derivation stays outside this module. Mask positions
//! identify applicable invariants in canonical resource order, not durable
//! predicate IDs. Different invariants may intentionally share a predicate.

use super::{
    invariant_binding_map, validate_fact_snapshot, InvariantPrecheckEntry,
    InvariantPrecheckError, InvariantPrecheckReport, InvariantPrecheckStatus,
};
use crate::plan::invariant_applies_to_candidate;
use crate::{FactSnapshot, Plan};
use elastic_core::resource::Invariant;
use elastic_core::{
    FactMask, FactSet, FreshnessSnapshot, GuardFactSource, InvariantPredicateBinding,
    LogicError, PredicateId, PredicateKey, TruthValue, FAST_PREDICATE_CAPACITY,
};
use std::fmt;

/// Maximum applicable invariants in the optional single-word compiled path.
/// Larger declarations may still use the scalar precheck; no implicit fallback
/// or truncation occurs here.
pub const MAX_COMPILED_INVARIANTS: usize = FAST_PREDICATE_CAPACITY as usize;

/// Compact diagnostic result. A `Passed` status only permits continued trusted
/// validation; this type is never a validation or actuation token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvariantMaskSummary {
    required: FactMask,
    values: FactSet,
}

impl InvariantMaskSummary {
    /// Required invariant slots in canonical applicable-invariant order.
    #[must_use]
    pub const fn required_bits(self) -> u64 {
        self.required.bits()
    }

    /// Slots with explicit positive predicate evidence.
    #[must_use]
    pub const fn true_bits(self) -> u64 {
        self.values.known_true().bits() & self.required.bits()
    }

    /// Slots with explicit negative predicate evidence.
    #[must_use]
    pub const fn false_bits(self) -> u64 {
        self.values.known_false().bits() & self.required.bits()
    }

    /// Missing bindings and missing/unknown facts remain explicitly unknown.
    #[must_use]
    pub const fn unknown_bits(self) -> u64 {
        self.required.bits() & !(self.true_bits() | self.false_bits())
    }

    /// Strong-Kleene conjunction: false dominates unknown; only all-true
    /// applicable facts pass. An empty applicable set passes vacuously, but
    /// only after the same provenance/freshness checks as the scalar path.
    #[must_use]
    pub const fn status(self) -> InvariantPrecheckStatus {
        if self.values.known_false().intersects(self.required) {
            InvariantPrecheckStatus::Rejected
        } else if self.values.known_true().contains_all(self.required) {
            InvariantPrecheckStatus::Passed
        } else {
            InvariantPrecheckStatus::InsufficientEvidence
        }
    }
}

/// Reusable applicability/binding layout for one exact planned candidate.
///
/// Reuse checks the complete EIR resource, candidate including magnitude, and
/// numerical context (floating-point bit patterns). Diagnostic reasoning text
/// is not semantic identity. Fresh fact snapshots may be supplied on each call.
///
/// This binding prevents accidental reuse of a compiled layout for a changed
/// plan. It does NOT authenticate that a caller-derived fact is a proof about
/// the candidate's target state: `FactSnapshot` still has resource/epoch, not
/// candidate-specific attestation, semantics. Adapters must validate effects.
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledInvariantPrecheck {
    plan: Plan,
    slots: Vec<(Invariant, Option<PredicateKey>)>,
    required: FactMask,
}

impl CompiledInvariantPrecheck {
    /// Compile applicability and stable-key bindings once. All applicable
    /// invariants occupy a slot, including invariants with no Boolean binding.
    ///
    /// # Errors
    /// Rejects absent/undeclared/ungrounded candidates, duplicate or oversized
    /// binding lists, and more than 64 applicable invariants. Unrelated bindings
    /// are ignored, matching the existing scalar API.
    pub fn compile(
        plan: &Plan,
        bindings: &[InvariantPredicateBinding],
    ) -> Result<Self, CompiledInvariantPrecheckError> {
        let candidate = plan
            .candidate()
            .ok_or(CompiledInvariantPrecheckError::NoCandidate)?;
        if !candidate.is_declared_in(&plan.resource) {
            return Err(CompiledInvariantPrecheckError::InvalidCandidate);
        }
        let by_invariant = invariant_binding_map(bindings)?;
        let mut slots = Vec::new();
        let mut required = FactMask::empty();
        for invariant in plan
            .resource
            .invariants()
            .iter()
            .filter(|invariant| invariant_applies_to_candidate(invariant, candidate))
        {
            if slots.len() == MAX_COMPILED_INVARIANTS {
                return Err(CompiledInvariantPrecheckError::TooManyApplicableInvariants {
                    max: MAX_COMPILED_INVARIANTS,
                });
            }
            required.insert(PredicateId::new(slots.len() as u32))?;
            slots.push((invariant.clone(), by_invariant.get(invariant).cloned()));
        }
        Ok(Self {
            plan: plan.clone(),
            slots,
            required,
        })
    }

    /// Number of applicable invariant slots, not number of unique predicates.
    #[must_use]
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Whether the declared candidate has no applicable invariants.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Evaluate without constructing the detailed per-invariant report.
    /// Each slot is refreshed from the supplied snapshot; nothing is cached as
    /// true across calls. The success path creates no diagnostic vectors/maps.
    ///
    /// # Errors
    /// Rejects changed plans and missing, mismatched or stale resource facts.
    /// Structural equality, not a non-cryptographic fingerprint, binds the plan.
    pub fn evaluate_summary(
        &self,
        plan: &Plan,
        facts: &FactSnapshot,
        freshness: &FreshnessSnapshot,
    ) -> Result<InvariantMaskSummary, CompiledInvariantPrecheckError> {
        let same_context = self
            .plan
            .context
            .iter()
            .map(|(signal, value)| (signal, value.to_bits()))
            .eq(plan.context.iter().map(|(signal, value)| (signal, value.to_bits())));
        if self.plan.resource != plan.resource
            || self.plan.outcome != plan.outcome
            || !same_context
        {
            return Err(CompiledInvariantPrecheckError::PlanChanged);
        }
        validate_fact_snapshot(plan, facts, freshness)?;
        let mut values = FactSet::new();
        for (index, (_, predicate)) in self.slots.iter().enumerate() {
            let truth = predicate
                .as_ref()
                .map_or(TruthValue::Unknown, |key| facts.truth(key));
            values.set(PredicateId::new(index as u32), truth)?;
        }
        Ok(InvariantMaskSummary {
            required: self.required,
            values,
        })
    }

    /// Produce the same detailed report as the scalar precheck, with canonical
    /// order and explicit missing bindings. This optional diagnostic path clones
    /// entries; use `evaluate_summary` when the compact result is sufficient.
    pub fn evaluate(
        &self,
        plan: &Plan,
        facts: &FactSnapshot,
        freshness: &FreshnessSnapshot,
    ) -> Result<InvariantPrecheckReport, CompiledInvariantPrecheckError> {
        let summary = self.evaluate_summary(plan, facts, freshness)?;
        let mut entries = Vec::with_capacity(self.slots.len());
        for (index, (invariant, predicate)) in self.slots.iter().enumerate() {
            entries.push(InvariantPrecheckEntry {
                invariant: invariant.clone(),
                predicate: predicate.clone(),
                truth: summary.values.get(PredicateId::new(index as u32))?,
            });
        }
        Ok(InvariantPrecheckReport {
            status: summary.status(),
            entries,
        })
    }
}

/// Fail-closed construction/reuse errors for the optional compiled path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompiledInvariantPrecheckError {
    /// No transition candidate exists to bind the layout to.
    NoCandidate,
    /// The candidate is undeclared or lacks required capability grounding.
    InvalidCandidate,
    /// The applicable invariant set exceeds the single-word capacity.
    TooManyApplicableInvariants { max: usize },
    /// Resource content, candidate/magnitude or numerical context changed.
    PlanChanged,
    /// Shared scalar binding/provenance/freshness validation failed.
    Precheck(InvariantPrecheckError),
    /// A checked core mask operation failed.
    Logic(LogicError),
}

impl fmt::Display for CompiledInvariantPrecheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoCandidate => f.write_str("cannot compile invariant checks without a candidate"),
            Self::InvalidCandidate => f.write_str("cannot compile an undeclared candidate"),
            Self::TooManyApplicableInvariants { max } => {
                write!(f, "compiled precheck supports at most {max} applicable invariants")
            }
            Self::PlanChanged => f.write_str("compiled invariant precheck plan has changed"),
            Self::Precheck(error) => error.fmt(f),
            Self::Logic(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for CompiledInvariantPrecheckError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Precheck(error) => Some(error),
            Self::Logic(error) => Some(error),
            _ => None,
        }
    }
}

impl From<InvariantPrecheckError> for CompiledInvariantPrecheckError {
    fn from(error: InvariantPrecheckError) -> Self {
        Self::Precheck(error)
    }
}

impl From<LogicError> for CompiledInvariantPrecheckError {
    fn from(error: LogicError) -> Self {
        Self::Logic(error)
    }
}
