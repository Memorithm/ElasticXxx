//! The general Elastic resource declaration: [`ResourceSpec`].
//!
//! A `ResourceSpec` is the typed, validated form of the provisional model
//! R = (K, S, D, T, I, M) from the whitepaper:
//!
//! | Model element | Representation |
//! |---|---|
//! | K — semantics/kind | [`ResourceClassId`] + [`LogicalResourceId`] |
//! | S — admissible state space | derived from D × T × I; concrete materialized states stay resource-adapter-specific |
//! | D — elastic dimensions | declared via [`ResourceSpecBuilder::allow`] |
//! | T — legal transitions | [`AdmissibleTransition`] declarations |
//! | I — invariants | [`Invariant`] declarations |
//! | M — observations/costs | [`ObservationSignalId`] declarations + ordered [`ObjectiveId`] priorities |
//!
//! Declarations are intent. They never execute anything and they never make a
//! transition legal by themselves; execution remains gated by trusted
//! capabilities and validation in the resource-specific adapter layers.

use super::error::ResourceSpecError;
use super::invariant::Invariant;
use super::terms::{
    DimensionId, LogicalResourceId, ObjectiveId, ObservationSignalId, ResourceClassId,
};
use super::transition::{AdmissibleTransition, CapabilityRequirement};
use crate::representation::TransitionMechanism;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// A validated declaration of one logical elastic resource.
///
/// Construct through [`ResourceSpec::builder`]. All collections are normalized
/// (sorted; objectives keep their declared priority order), so two specs built
/// with equal declarations compare equal regardless of construction order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceSpec {
    class: ResourceClassId,
    resource_id: LogicalResourceId,
    dimensions: Vec<DimensionId>,
    invariants: Vec<Invariant>,
    objectives: Vec<ObjectiveId>,
    transitions: Vec<AdmissibleTransition>,
    capabilities: Vec<CapabilityRequirement>,
    observations: Vec<ObservationSignalId>,
    labels: BTreeMap<String, String>,
}

impl ResourceSpec {
    /// Begin building a resource declaration.
    ///
    /// # Examples
    ///
    /// ```
    /// use elastic_core::resource::{
    ///     AdmissibleTransition, CapabilityRequirement, DimensionId, Invariant, InvariantKind,
    ///     LogicalResourceId, ObjectiveId, ObservationSignalId, ResourceClassId, ResourceSpec,
    /// };
    /// use elastic_core::TransitionMechanism;
    ///
    /// let spec = ResourceSpec::builder(
    ///         ResourceClassId::REPRESENTATIONAL,
    ///         LogicalResourceId::new("session-kv")?,
    ///     )
    ///     .allow(DimensionId::REPRESENTATION)
    ///     .allow(DimensionId::RESIDENCY)
    ///     .preserve(Invariant::new(InvariantKind::PreserveContents))
    ///     .preserve(
    ///         Invariant::new(InvariantKind::UpholdContract(
    ///             elastic_core::resource::ContractId::new("kv.reuse-contract")?,
    ///         ))
    ///         .along(DimensionId::REPRESENTATION),
    ///     )
    ///     .optimize(ObjectiveId::LATENCY)
    ///     .admit(AdmissibleTransition::new(
    ///         TransitionMechanism::Reencode,
    ///         DimensionId::REPRESENTATION,
    ///     ))
    ///     .require_capability(CapabilityRequirement::new(
    ///         TransitionMechanism::Reencode,
    ///         DimensionId::REPRESENTATION,
    ///     ))
    ///     .observe(ObservationSignalId::FREE_CAPACITY)
    ///     .build()?;
    ///
    /// assert!(spec.is_elastic(&DimensionId::REPRESENTATION));
    /// assert_eq!(spec.objectives(), &[ObjectiveId::LATENCY]);
    /// # Ok::<(), elastic_core::ResourceSpecError>(())
    /// ```
    #[must_use]
    pub fn builder(class: ResourceClassId, resource_id: LogicalResourceId) -> ResourceSpecBuilder {
        ResourceSpecBuilder {
            class,
            resource_id,
            dimensions: Vec::new(),
            invariants: Vec::new(),
            objectives: Vec::new(),
            transitions: Vec::new(),
            capabilities: Vec::new(),
            observations: Vec::new(),
            labels: Vec::new(),
        }
    }

    /// The semantic class of the resource.
    #[must_use]
    pub const fn class(&self) -> &ResourceClassId {
        &self.class
    }

    /// The stable logical identity of the resource.
    #[must_use]
    pub const fn resource_id(&self) -> &LogicalResourceId {
        &self.resource_id
    }

    /// The declared elastic dimensions, in canonical (sorted) order.
    #[must_use]
    pub fn elastic_dimensions(&self) -> &[DimensionId] {
        &self.dimensions
    }

    /// Whether `dimension` may legally change for this resource.
    #[must_use]
    pub fn is_elastic(&self, dimension: &DimensionId) -> bool {
        self.dimensions.contains(dimension)
    }

    /// The declared invariants, in canonical (sorted) order.
    #[must_use]
    pub fn invariants(&self) -> &[Invariant] {
        &self.invariants
    }

    /// The optimization objectives in priority order: the first entry has the
    /// highest priority.
    ///
    /// There is deliberately no universal scalar cost model; the ordering is
    /// the only cross-objective structure this version defines.
    #[must_use]
    pub fn objectives(&self) -> &[ObjectiveId] {
        &self.objectives
    }

    /// The admitted transition mechanisms, in canonical (sorted) order.
    #[must_use]
    pub fn admissible_transitions(&self) -> &[AdmissibleTransition] {
        &self.transitions
    }

    /// The required runtime capabilities, in canonical (sorted) order.
    #[must_use]
    pub fn required_capabilities(&self) -> &[CapabilityRequirement] {
        &self.capabilities
    }

    /// The observation signals relevant to this resource, in canonical
    /// (sorted) order.
    #[must_use]
    pub fn observed_signals(&self) -> &[ObservationSignalId] {
        &self.observations
    }

    /// The value of a diagnostic label, if declared.
    ///
    /// Labels are metadata for display and diagnostics only; they never
    /// contribute semantics.
    #[must_use]
    pub fn label(&self, key: &str) -> Option<&str> {
        self.labels.get(key).map(String::as_str)
    }

    /// Iterate over all diagnostic labels in key order.
    pub fn iter_labels(&self) -> impl Iterator<Item = (&str, &str)> {
        self.labels.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Whether the spec admits `mechanism` along `dimension`.
    #[must_use]
    pub fn admits(&self, mechanism: TransitionMechanism, dimension: &DimensionId) -> bool {
        self.transitions
            .iter()
            .any(|t| t.mechanism() == mechanism && t.dimension() == dimension)
    }

    /// Whether every requirement of `capability` is declared.
    #[must_use]
    pub fn requires_capability(&self, capability: &CapabilityRequirement) -> bool {
        self.capabilities.contains(capability)
    }
}

impl fmt::Display for ResourceSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "resource {} ({}) elastic=[{}]",
            self.resource_id,
            self.class,
            self.dimensions
                .iter()
                .map(DimensionId::as_str)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

/// Builder for [`ResourceSpec`].
///
/// Each declaration method appends one fragment; validation happens once at
/// [`ResourceSpecBuilder::build`], returning structured errors instead of
/// panicking for ordinary invalid input.
#[derive(Clone, Debug)]
pub struct ResourceSpecBuilder {
    class: ResourceClassId,
    resource_id: LogicalResourceId,
    dimensions: Vec<DimensionId>,
    invariants: Vec<Invariant>,
    objectives: Vec<ObjectiveId>,
    transitions: Vec<AdmissibleTransition>,
    capabilities: Vec<CapabilityRequirement>,
    observations: Vec<ObservationSignalId>,
    labels: Vec<(String, String)>,
}

impl ResourceSpecBuilder {
    /// Allow the resource to change along `dimension`.
    #[must_use]
    pub fn allow(mut self, dimension: DimensionId) -> Self {
        self.dimensions.push(dimension);
        self
    }

    /// Require an invariant to hold across transitions.
    #[must_use]
    pub fn preserve(mut self, invariant: Invariant) -> Self {
        self.invariants.push(invariant);
        self
    }

    /// Add an optimization objective.
    ///
    /// The first objective added has the highest priority; later objectives
    /// act as tie-breakers in declaration order. Duplicate objectives are
    /// rejected at build time.
    #[must_use]
    pub fn optimize(mut self, objective: ObjectiveId) -> Self {
        self.objectives.push(objective);
        self
    }

    /// Admit a transition mechanism along one elastic dimension.
    #[must_use]
    pub fn admit(mut self, transition: AdmissibleTransition) -> Self {
        self.transitions.push(transition);
        self
    }

    /// Require a trusted capability that can execute an admissible transition.
    #[must_use]
    pub fn require_capability(mut self, capability: CapabilityRequirement) -> Self {
        self.capabilities.push(capability);
        self
    }

    /// Declare a runtime observation relevant to adaptation decisions.
    #[must_use]
    pub fn observe(mut self, signal: ObservationSignalId) -> Self {
        self.observations.push(signal);
        self
    }

    /// Attach a diagnostic label. Labels never contribute semantics.
    #[must_use]
    pub fn label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.push((key.into(), value.into()));
        self
    }

    /// Validate the accumulated declarations and produce a [`ResourceSpec`].
    ///
    /// # Errors
    ///
    /// Returns a structured [`ResourceSpecError`] describing the first
    /// detected problem with the declaration (in a fixed checking order):
    /// invalid labels, duplicate declarations, empty elasticity, transitions
    /// or capabilities beyond the elastic dimensions, and invariants scoped to
    /// dimensions that cannot change.
    pub fn build(self) -> Result<ResourceSpec, ResourceSpecError> {
        for (key, _) in &self.labels {
            if key.trim().is_empty() {
                return Err(ResourceSpecError::InvalidLabelKey);
            }
        }
        let labels: BTreeMap<String, String> = self.labels.into_iter().collect();

        reject_duplicate(&self.dimensions, |dimension| {
            ResourceSpecError::DuplicateDimension { dimension }
        })?;
        if self.dimensions.is_empty() {
            return Err(ResourceSpecError::NoElasticDimensions);
        }
        let mut dimensions = self.dimensions;
        dimensions.sort_unstable();
        dimensions.dedup();

        reject_duplicate(&self.objectives, |objective| {
            ResourceSpecError::DuplicateObjective { objective }
        })?;
        let objectives = self.objectives;

        reject_duplicate(&self.invariants, |invariant| {
            ResourceSpecError::DuplicateInvariant { invariant }
        })?;
        for invariant in &self.invariants {
            if let Some(scope) = invariant.scope() {
                if !dimensions.contains(scope) {
                    return Err(ResourceSpecError::VacuousInvariant {
                        invariant: invariant.clone(),
                    });
                }
            }
        }
        let mut invariants = self.invariants;
        invariants.sort_unstable();
        invariants.dedup();

        reject_duplicate(&self.transitions, |transition| {
            ResourceSpecError::DuplicateAdmissibleTransition { transition }
        })?;
        for transition in &self.transitions {
            if !dimensions.contains(transition.dimension()) {
                return Err(ResourceSpecError::TransitionBeyondElasticDimensions {
                    transition: transition.clone(),
                });
            }
        }
        let mut transitions = self.transitions;
        transitions.sort_unstable();
        transitions.dedup();

        reject_duplicate(&self.capabilities, |requirement| {
            ResourceSpecError::DuplicateCapabilityRequirement { requirement }
        })?;
        for capability in &self.capabilities {
            if !dimensions.contains(capability.dimension()) {
                return Err(ResourceSpecError::CapabilityBeyondElasticDimensions {
                    requirement: capability.clone(),
                });
            }
        }
        let mut capabilities = self.capabilities;
        capabilities.sort_unstable();
        capabilities.dedup();

        reject_duplicate(&self.observations, |signal| {
            ResourceSpecError::DuplicateObservation { signal }
        })?;
        let mut observations = self.observations;
        observations.sort_unstable();
        observations.dedup();

        Ok(ResourceSpec {
            class: self.class,
            resource_id: self.resource_id,
            dimensions,
            invariants,
            objectives,
            transitions,
            capabilities,
            observations,
            labels,
        })
    }
}

/// Report the first duplicated element of `items` through `error`.
fn reject_duplicate<T: Clone + Ord, F>(items: &[T], error: F) -> Result<(), ResourceSpecError>
where
    F: Fn(T) -> ResourceSpecError,
{
    let mut seen = BTreeSet::new();
    for item in items {
        if !seen.insert(item) {
            return Err(error(item.clone()));
        }
    }
    Ok(())
}
