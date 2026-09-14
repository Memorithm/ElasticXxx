//! Validated planning views containing only an explicit candidate subset.
//!
//! This is structural restriction, not Boolean evaluation or authorization.
//! Callers must obtain their candidate set from the appropriate fresh policy
//! evaluation. The original resource remains authoritative for validation and
//! actuation; a narrowed view receives its own structural fingerprint.

use crate::{EirResource, EirResourceParts, TransitionCandidate, ValidationError};
use std::fmt;

/// Failure to construct a grounded subset of one resource's admissions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanningSubsetError {
    /// A supplied candidate is ungrounded or not declared by this resource.
    InvalidCandidate(TransitionCandidate),
    /// The existing EIR structural validator rejected the projected parts.
    Validation(ValidationError),
}

impl fmt::Display for PlanningSubsetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCandidate(candidate) => {
                write!(f, "invalid candidate in planning subset: {candidate}")
            }
            Self::Validation(error) => write!(f, "invalid planning subset: {error}"),
        }
    }
}

impl std::error::Error for PlanningSubsetError {}

impl EirResource {
    /// Construct a planning-only resource exposing exactly these admissions.
    ///
    /// Candidate order, duplicates and advisory magnitudes do not alter the
    /// admitted set. Original canonical order is retained. Identity, class,
    /// dimensions, invariants, objective priority, observations and labels are
    /// preserved. Capability requirements for excluded transitions are removed
    /// so no orphan requirement can survive structural validation.
    ///
    /// The result is rebuilt through [`EirResource::from_parts`], never assigned
    /// the original fingerprint after its contents change. A complete grounded
    /// candidate set reconstructs the original resource exactly. An empty set
    /// produces a valid view with no transitions or capabilities.
    ///
    /// This method does not evaluate guards, establish freshness, or authorize
    /// actuation. Candidates currently describe mechanism/dimension/grounding,
    /// not source resource identity; matching candidates are checked against
    /// this resource, not authenticated as originating from it.
    ///
    /// # Errors
    ///
    /// Rejects any ungrounded or undeclared candidate before constructing a
    /// view, and propagates the existing structural validator's errors.
    pub fn restrict_to_candidates(
        &self,
        candidates: &[TransitionCandidate],
    ) -> Result<Self, PlanningSubsetError> {
        for candidate in candidates {
            if !candidate.is_declared_in(self) {
                return Err(PlanningSubsetError::InvalidCandidate(candidate.clone()));
            }
        }

        let transitions: Vec<_> = self
            .transitions()
            .iter()
            .filter(|admitted| {
                candidates.iter().any(|candidate| {
                    candidate.mechanism() == admitted.transition().mechanism()
                        && candidate.dimension() == admitted.transition().dimension()
                })
            })
            .map(|admitted| admitted.transition().clone())
            .collect();
        let capabilities = self
            .capabilities()
            .iter()
            .filter(|capability| {
                transitions.iter().any(|transition| {
                    transition.mechanism() == capability.mechanism()
                        && transition.dimension() == capability.dimension()
                })
            })
            .cloned()
            .collect();

        Self::from_parts(EirResourceParts {
            identity: self.identity().as_str().to_owned(),
            class: self.class().clone(),
            dimensions: self.dimensions().to_vec(),
            invariants: self.invariants().to_vec(),
            objectives: self
                .objective_ranking()
                .iter()
                .map(|entry| entry.objective().clone())
                .collect(),
            transitions,
            capabilities,
            observations: self.observations().to_vec(),
            labels: self
                .iter_labels()
                .map(|(key, value)| (key.to_owned(), value.to_owned()))
                .collect(),
        })
        .map_err(PlanningSubsetError::Validation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use elastic_core::resource::{
        AdmissibleTransition, CapabilityRequirement, DimensionId, Invariant, InvariantKind,
        LogicalResourceId, ObjectiveId, ObservationSignalId, ResourceClassId, ResourceSpec,
    };
    use elastic_core::TransitionMechanism;

    fn fixture(grounded: bool) -> EirResource {
        let mut builder = ResourceSpec::builder(
            ResourceClassId::CAPACITY_RESOURCE,
            LogicalResourceId::new("planning-subset").unwrap(),
        )
        .allow(DimensionId::CAPACITY)
        .allow(DimensionId::CONCURRENCY)
        .preserve(Invariant::new(InvariantKind::PreserveIdentity))
        .preserve(Invariant::new(InvariantKind::PreserveContents).along(DimensionId::CAPACITY))
        .optimize(ObjectiveId::LATENCY)
        .optimize(ObjectiveId::MEMORY_FOOTPRINT)
        .observe(ObservationSignalId::UTILIZATION)
        .label("purpose", "subset-contract");
        for dimension in [DimensionId::CAPACITY, DimensionId::CONCURRENCY] {
            builder = builder.admit(AdmissibleTransition::new(
                TransitionMechanism::Reinterpret,
                dimension.clone(),
            ));
            if grounded {
                builder = builder.require_capability(CapabilityRequirement::new(
                    TransitionMechanism::Reinterpret,
                    dimension,
                ));
            }
        }
        crate::lower(&builder.build().unwrap()).unwrap().resources()[0].clone()
    }

    #[test]
    fn every_subset_preserves_constraints_and_original_canonical_order() {
        let resource = fixture(true);
        let original = resource.clone();
        let candidates: Vec<_> = resource
            .transitions()
            .iter()
            .map(TransitionCandidate::from_admitted)
            .collect();
        for mask in 0_u8..4 {
            let mut selected: Vec<_> = candidates
                .iter()
                .enumerate()
                .filter(|(index, _)| mask & (1 << index) != 0)
                .map(|(_, candidate)| candidate.clone())
                .collect();
            selected.reverse();
            let view = resource.restrict_to_candidates(&selected).unwrap();
            assert_eq!(view.identity(), resource.identity());
            assert_eq!(view.class(), resource.class());
            assert_eq!(view.dimensions(), resource.dimensions());
            assert_eq!(view.invariants(), resource.invariants());
            assert_eq!(view.objective_ranking(), resource.objective_ranking());
            assert_eq!(view.observations(), resource.observations());
            assert_eq!(view.label("purpose"), Some("subset-contract"));
            assert_eq!(view.transitions().len(), selected.len());
            assert_eq!(view.capabilities().len(), selected.len());
            assert!(view.transitions().windows(2).all(|pair| pair[0] < pair[1]));
            for admitted in view.transitions() {
                let candidate = TransitionCandidate::from_admitted(admitted);
                assert!(candidate.is_declared_in(&resource));
                assert!(selected.contains(&candidate));
            }
            if mask == 3 {
                assert_eq!(view, resource);
            } else {
                assert_ne!(view.fingerprint(), resource.fingerprint());
            }
            assert_eq!(resource, original);
        }
    }

    #[test]
    fn duplicate_order_and_magnitude_do_not_change_the_view() {
        let resource = fixture(true);
        let candidate = TransitionCandidate::from_admitted(&resource.transitions()[0]);
        let first = resource
            .restrict_to_candidates(std::slice::from_ref(&candidate))
            .unwrap();
        let second = resource
            .restrict_to_candidates(&[candidate.clone(), candidate.with_magnitude(123)])
            .unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn ungrounded_candidate_cannot_borrow_grounding_from_the_same_pair() {
        let resource = fixture(true);
        let ungrounded = fixture(false);
        let candidate = TransitionCandidate::from_admitted(&ungrounded.transitions()[0]);
        assert!(matches!(
            resource.restrict_to_candidates(&[candidate]),
            Err(PlanningSubsetError::InvalidCandidate(_))
        ));
    }

    #[test]
    fn excluded_candidate_cannot_be_added_back_to_a_restricted_view() {
        let resource = fixture(true);
        let first = TransitionCandidate::from_admitted(&resource.transitions()[0]);
        let excluded = TransitionCandidate::from_admitted(&resource.transitions()[1]);
        let view = resource.restrict_to_candidates(&[first]).unwrap();
        assert!(matches!(
            view.restrict_to_candidates(&[excluded]),
            Err(PlanningSubsetError::InvalidCandidate(_))
        ));
    }
}
