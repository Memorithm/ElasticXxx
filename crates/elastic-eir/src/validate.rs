//! Structural validation of raw EIR resource parts.
//!
//! This is the single authority for what constitutes valid EIR content.
//! Every construction path — lowering from the surface model, assembling
//! parts directly, or future deserialization — must pass through here, so
//! invalid EIR cannot silently reach runtime planning.

use crate::error::ValidationError;
use crate::resource::EirResourceParts;
use std::collections::BTreeSet;

/// Validate one resource's raw parts structurally.
///
/// Checks, in fixed order: non-empty identity and label keys, non-empty
/// elasticity, duplicate dimensions/objectives/invariants/transitions/
/// capabilities/observations, invariants scoped to non-elastic dimensions,
/// transitions and capabilities beyond the elastic dimensions, and capability
/// requirements not grounded in any admitted transition.
///
/// # Errors
///
/// Returns [`ValidationError`] for the first violated rule. Validation proves
/// structural consistency only; it never authenticates capabilities and never
/// solves planning problems.
pub fn validate_resource_parts(parts: &EirResourceParts) -> Result<(), ValidationError> {
    if parts.identity.trim().is_empty() {
        return Err(ValidationError::EmptyResourceIdentity);
    }
    if parts.labels.keys().any(|key| key.trim().is_empty()) {
        return Err(ValidationError::InvalidLabelKey);
    }

    reject_duplicate(&parts.dimensions, |dimension| {
        ValidationError::DuplicateDimension { dimension }
    })?;
    if parts.dimensions.is_empty() {
        return Err(ValidationError::NoElasticDimensions);
    }

    reject_duplicate(&parts.objectives, |objective| {
        ValidationError::DuplicateObjective { objective }
    })?;

    reject_duplicate(&parts.invariants, |invariant| {
        ValidationError::DuplicateInvariant { invariant }
    })?;
    for invariant in &parts.invariants {
        if let Some(scope) = invariant.scope() {
            if !parts.dimensions.contains(scope) {
                return Err(ValidationError::VacuousInvariant {
                    invariant: invariant.clone(),
                });
            }
        }
    }

    reject_duplicate(&parts.transitions, |transition| {
        ValidationError::DuplicateTransition { transition }
    })?;
    for transition in &parts.transitions {
        if !parts.dimensions.contains(transition.dimension()) {
            return Err(ValidationError::TransitionBeyondElasticDimensions {
                transition: transition.clone(),
            });
        }
    }

    reject_duplicate(&parts.capabilities, |requirement| {
        ValidationError::DuplicateCapabilityRequirement { requirement }
    })?;
    for capability in &parts.capabilities {
        if !parts.dimensions.contains(capability.dimension()) {
            return Err(ValidationError::CapabilityBeyondElasticDimensions {
                requirement: capability.clone(),
            });
        }
        let grounded = parts.transitions.iter().any(|transition| {
            transition.mechanism() == capability.mechanism()
                && transition.dimension() == capability.dimension()
        });
        if !grounded {
            return Err(ValidationError::CapabilityNotGroundedInAdmission {
                requirement: capability.clone(),
            });
        }
    }

    reject_duplicate(&parts.observations, |signal| {
        ValidationError::DuplicateObservation { signal }
    })?;

    Ok(())
}

/// Report the first duplicated element of `items` through `error`.
fn reject_duplicate<T: Clone + Ord, F>(items: &[T], error: F) -> Result<(), ValidationError>
where
    F: Fn(T) -> ValidationError,
{
    let mut seen = BTreeSet::new();
    for item in items {
        if !seen.insert(item) {
            return Err(error(item.clone()));
        }
    }
    Ok(())
}
