//! Normalized per-resource IR nodes.

use crate::error::ValidationError;
use crate::fingerprint::Fingerprint;
use crate::validate_resource_parts;
use elastic_core::resource::{
    AdmissibleTransition, CapabilityRequirement, DimensionId, Invariant, LogicalResourceId,
    ObjectiveId, ObservationSignalId, ResourceClassId,
};
use std::collections::BTreeMap;
use std::fmt;

/// Raw, unnormalized parts of one resource node.
///
/// Used by tooling that assembles EIR from non-surface sources
/// ([`EirResource::from_parts`]). Surface users normally go through
/// [`crate::lower`](crate::lower), which fills these from a
/// [`elastic_core::resource::ResourceSpec`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EirResourceParts {
    /// Logical resource identity text (non-empty).
    pub identity: String,
    /// Semantic resource class.
    pub class: ResourceClassId,
    /// Declared elastic dimensions.
    pub dimensions: Vec<DimensionId>,
    /// Declared invariants.
    pub invariants: Vec<Invariant>,
    /// Optimization objectives in priority order (first = highest).
    pub objectives: Vec<ObjectiveId>,
    /// Admitted transitions.
    pub transitions: Vec<AdmissibleTransition>,
    /// Required trusted capabilities.
    pub capabilities: Vec<CapabilityRequirement>,
    /// Relevant observation signals.
    pub observations: Vec<ObservationSignalId>,
    /// Diagnostic labels (semantics-free).
    pub labels: BTreeMap<String, String>,
}

/// One optimization objective with its explicit normalized priority rank.
///
/// Rank `0` is the highest-priority objective. Ranks make the priority order
/// explicit data instead of positional knowledge, which keeps downstream
/// consumers deterministic.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectiveRank {
    rank: u32,
    objective: ObjectiveId,
}

impl ObjectiveRank {
    /// The priority rank (0 = highest).
    #[must_use]
    pub const fn rank(&self) -> u32 {
        self.rank
    }

    /// The ranked objective.
    #[must_use]
    pub const fn objective(&self) -> &ObjectiveId {
        &self.objective
    }
}

impl fmt::Display for ObjectiveRank {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{} {}", self.rank, self.objective)
    }
}

/// One admitted transition in normalized form, together with the derived
/// fact of whether a required capability grounds its executability.
///
/// Grounding does **not** mean a capability exists; it means the declaration
/// requires one for exactly this transition, so planning knows the transition
/// has a declared execution path.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AdmittedTransition {
    transition: AdmissibleTransition,
    capability_grounded: bool,
}

impl AdmittedTransition {
    /// The admitted mechanism/dimension pair.
    #[must_use]
    pub const fn transition(&self) -> &AdmissibleTransition {
        &self.transition
    }

    /// Whether a capability requirement covers exactly this transition.
    #[must_use]
    pub const fn capability_grounded(&self) -> bool {
        self.capability_grounded
    }
}

impl fmt::Display for AdmittedTransition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.transition)
    }
}

/// Normalized EIR node for one logical elastic resource.
///
/// Constructed only through validated paths ([`EirResource::from_parts`] or
/// [`crate::lower`](crate::lower)); invalid content cannot silently become an
/// `EirResource`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EirResource {
    identity: LogicalResourceId,
    class: ResourceClassId,
    dimensions: Vec<DimensionId>,
    invariants: Vec<Invariant>,
    objective_ranking: Vec<ObjectiveRank>,
    transitions: Vec<AdmittedTransition>,
    capabilities: Vec<CapabilityRequirement>,
    observations: Vec<ObservationSignalId>,
    labels: BTreeMap<String, String>,
    fingerprint: Fingerprint,
}

impl EirResource {
    /// Validate raw parts and normalize them into an IR node.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError`] describing the first structural problem
    /// (see [`crate::validate_resource_parts`]).
    pub fn from_parts(parts: EirResourceParts) -> Result<Self, ValidationError> {
        let identity = LogicalResourceId::new(parts.identity.clone())
            .map_err(|_| ValidationError::EmptyResourceIdentity)?;
        validate_resource_parts(&parts)?;
        Ok(Self::normalize(identity, parts))
    }

    pub(crate) fn normalize(identity: LogicalResourceId, parts: EirResourceParts) -> Self {
        let mut dimensions = parts.dimensions;
        dimensions.sort_unstable();
        dimensions.dedup();

        let mut invariants = parts.invariants;
        invariants.sort_unstable();
        invariants.dedup();

        let mut ranking: Vec<ObjectiveRank> = parts
            .objectives
            .iter()
            .cloned()
            .enumerate()
            .map(|(rank, objective)| ObjectiveRank {
                rank: u32::try_from(rank).unwrap_or(u32::MAX),
                objective,
            })
            .collect();
        ranking.sort_unstable();

        let mut transitions: Vec<AdmittedTransition> = parts
            .transitions
            .iter()
            .cloned()
            .map(|transition| {
                let capability_grounded = parts.capabilities.iter().any(|capability| {
                    capability.mechanism() == transition.mechanism()
                        && capability.dimension() == transition.dimension()
                });
                AdmittedTransition {
                    transition,
                    capability_grounded,
                }
            })
            .collect();
        // Normalization must hold on every construction path, including
        // `from_parts` tooling input: sort admitted transitions so permuted
        // input order cannot change equality or fingerprints.
        transitions.sort_unstable();
        transitions.dedup();

        let mut capabilities = parts.capabilities;
        capabilities.sort_unstable();
        capabilities.dedup();

        let mut observations = parts.observations;
        observations.sort_unstable();
        observations.dedup();

        let mut fingerprint = Fingerprint::EMPTY.text("eir-resource");
        fingerprint = fingerprint.text(identity.as_str());
        fingerprint = fingerprint.text(parts.class.as_str());
        for dimension in &dimensions {
            fingerprint = fingerprint.text(dimension.as_str());
        }
        for invariant in &invariants {
            fingerprint = fingerprint.text(&invariant.to_string());
        }
        for entry in &ranking {
            fingerprint = fingerprint
                .number(u64::from(entry.rank()))
                .text(entry.objective().as_str());
        }
        for admitted in &transitions {
            fingerprint = fingerprint.text(&admitted.transition.to_string());
            fingerprint = fingerprint.number(u64::from(admitted.capability_grounded));
        }
        for capability in &capabilities {
            fingerprint = fingerprint.text(&capability.to_string());
        }
        for signal in &observations {
            fingerprint = fingerprint.text(signal.as_str());
        }
        for (key, value) in &parts.labels {
            fingerprint = fingerprint.text(key).text(value);
        }

        Self {
            identity,
            class: parts.class,
            dimensions,
            invariants,
            objective_ranking: ranking,
            transitions,
            capabilities,
            observations,
            labels: parts.labels,
            fingerprint,
        }
    }

    /// The logical identity of the resource.
    #[must_use]
    pub const fn identity(&self) -> &LogicalResourceId {
        &self.identity
    }

    /// The semantic resource class.
    #[must_use]
    pub const fn class(&self) -> &ResourceClassId {
        &self.class
    }

    /// Declared elastic dimensions, sorted.
    #[must_use]
    pub fn dimensions(&self) -> &[DimensionId] {
        &self.dimensions
    }

    /// Declared invariants, sorted.
    #[must_use]
    pub fn invariants(&self) -> &[Invariant] {
        &self.invariants
    }

    /// Objectives with explicit priority ranks (0 = highest), sorted by rank.
    #[must_use]
    pub fn objective_ranking(&self) -> &[ObjectiveRank] {
        &self.objective_ranking
    }

    /// Admitted transitions with derived capability grounding, sorted.
    #[must_use]
    pub fn transitions(&self) -> &[AdmittedTransition] {
        &self.transitions
    }

    /// Required trusted capabilities, sorted.
    #[must_use]
    pub fn capabilities(&self) -> &[CapabilityRequirement] {
        &self.capabilities
    }

    /// Relevant observation signals, sorted.
    #[must_use]
    pub fn observations(&self) -> &[ObservationSignalId] {
        &self.observations
    }

    /// Diagnostic label lookup.
    #[must_use]
    pub fn label(&self, key: &str) -> Option<&str> {
        self.labels.get(key).map(String::as_str)
    }

    /// Iterate diagnostic labels in key order.
    pub fn iter_labels(&self) -> impl Iterator<Item = (&str, &str)> {
        self.labels.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Structural fingerprint of this node's normalized content.
    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
}

impl fmt::Display for EirResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "eir {} ({}) dims=[{}] objectives=[{}] transitions=[{}]",
            self.identity,
            self.class,
            self.dimensions
                .iter()
                .map(DimensionId::as_str)
                .collect::<Vec<_>>()
                .join(","),
            self.objective_ranking
                .iter()
                .map(|entry| entry.objective.as_str())
                .collect::<Vec<_>>()
                .join(","),
            self.transitions
                .iter()
                .map(AdmittedTransition::to_string)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}
