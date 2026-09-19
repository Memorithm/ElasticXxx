//! Versioned cross-resource group representation in EIR.
//!
//! This envelope extends an already validated [`EirDocument`] with canonical
//! resource-group membership and dependency topology. It is pure data: no
//! execution ordering, planning, locking, transaction, or actuation authority
//! is created by lowering a group.

use crate::{EirDocument, EirPseudoBooleanConstraint, EirResource, Fingerprint};
use elastic_core::resource::{
    CrossResourceInvariant, ResourceDependency, ResourceGroup, SharedBudget, SharedBudgetTerm,
};
use elastic_core::PredicateKey;
use std::fmt;

/// Schema version of the resource-group EIR envelope.
pub const EIR_RESOURCE_GROUP_SCHEMA_VERSION: u16 = 1;
/// Maximum number of groups accepted in one grouped EIR document.
pub const MAX_EIR_RESOURCE_GROUPS: usize = 64;

/// Canonical dependency edge retained in EIR.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EirResourceDependency {
    dependent: String,
    required: String,
}

impl EirResourceDependency {
    fn from_dependency(value: &ResourceDependency) -> Self {
        Self {
            dependent: value.dependent().as_str().to_owned(),
            required: value.required().as_str().to_owned(),
        }
    }

    /// Dependent logical resource identity.
    #[must_use]
    pub fn dependent(&self) -> &str {
        &self.dependent
    }

    /// Required logical resource identity.
    #[must_use]
    pub fn required(&self) -> &str {
        &self.required
    }
}

/// One source-mapped resource contribution to a shared budget in EIR.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct EirSharedBudgetTerm {
    resource: String,
    predicate: PredicateKey,
    weight: i128,
}

impl EirSharedBudgetTerm {
    fn from_term(term: &SharedBudgetTerm) -> Self {
        Self {
            resource: term.resource().as_str().to_owned(),
            predicate: term.predicate().clone(),
            weight: term.weight(),
        }
    }

    #[must_use]
    pub fn resource(&self) -> &str {
        &self.resource
    }

    #[must_use]
    pub const fn predicate(&self) -> &PredicateKey {
        &self.predicate
    }

    #[must_use]
    pub const fn weight(&self) -> i128 {
        self.weight
    }
}

/// One shared group budget in EIR.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct EirSharedBudget {
    id: String,
    terms: Vec<EirSharedBudgetTerm>,
    constraint: EirPseudoBooleanConstraint,
    fingerprint: Fingerprint,
}

impl EirSharedBudget {
    fn lower(budget: &SharedBudget) -> Self {
        let terms = budget
            .terms()
            .iter()
            .map(EirSharedBudgetTerm::from_term)
            .collect::<Vec<_>>();
        let constraint = EirPseudoBooleanConstraint::from_declaration(budget.constraint());
        let mut fingerprint = Fingerprint::EMPTY
            .text("eir-shared-budget")
            .number(u64::from(EIR_RESOURCE_GROUP_SCHEMA_VERSION))
            .text(budget.id().as_str())
            .number(constraint.fingerprint().bits())
            .number(terms.len() as u64);
        for term in &terms {
            fingerprint = fingerprint
                .text(term.resource())
                .text(term.predicate().namespace())
                .text(term.predicate().name())
                .text(&term.weight().to_string());
        }
        Self {
            id: budget.id().as_str().to_owned(),
            terms,
            constraint,
            fingerprint,
        }
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    #[must_use]
    pub fn terms(&self) -> &[EirSharedBudgetTerm] {
        &self.terms
    }

    #[must_use]
    pub const fn constraint(&self) -> &EirPseudoBooleanConstraint {
        &self.constraint
    }

    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
}

/// One explicitly owned cross-resource invariant in EIR.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct EirCrossResourceInvariant {
    contract: String,
    owner: String,
    participants: Vec<String>,
    fingerprint: Fingerprint,
}

impl EirCrossResourceInvariant {
    fn lower(invariant: &CrossResourceInvariant) -> Self {
        let participants = invariant
            .participants()
            .iter()
            .map(|participant| participant.as_str().to_owned())
            .collect::<Vec<_>>();
        let mut fingerprint = Fingerprint::EMPTY
            .text("eir-cross-resource-invariant")
            .number(u64::from(EIR_RESOURCE_GROUP_SCHEMA_VERSION))
            .text(invariant.contract().as_str())
            .text(invariant.owner().as_str())
            .number(participants.len() as u64);
        for participant in &participants {
            fingerprint = fingerprint.text(participant);
        }
        Self {
            contract: invariant.contract().as_str().to_owned(),
            owner: invariant.owner().as_str().to_owned(),
            participants,
            fingerprint,
        }
    }

    #[must_use]
    pub fn contract(&self) -> &str {
        &self.contract
    }

    #[must_use]
    pub fn owner(&self) -> &str {
        &self.owner
    }

    #[must_use]
    pub fn participants(&self) -> &[String] {
        &self.participants
    }

    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
}

/// One canonical resource group in EIR.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct EirResourceGroup {
    id: String,
    members: Vec<String>,
    dependencies: Vec<EirResourceDependency>,
    shared_budgets: Vec<EirSharedBudget>,
    cross_invariants: Vec<EirCrossResourceInvariant>,
    fingerprint: Fingerprint,
}

impl EirResourceGroup {
    fn lower(group: &ResourceGroup, document: &EirDocument) -> Result<Self, GroupLoweringError> {
        for member in group.members() {
            if document.resource(member.as_str()).is_none() {
                return Err(GroupLoweringError::UnknownResource {
                    group: group.id().as_str().to_owned(),
                    resource: member.as_str().to_owned(),
                });
            }
        }

        let members = group
            .members()
            .iter()
            .map(|member| member.as_str().to_owned())
            .collect::<Vec<_>>();
        let dependencies = group
            .dependencies()
            .iter()
            .map(EirResourceDependency::from_dependency)
            .collect::<Vec<_>>();
        let shared_budgets = group
            .shared_budgets()
            .iter()
            .map(EirSharedBudget::lower)
            .collect::<Vec<_>>();
        let cross_invariants = group
            .cross_invariants()
            .iter()
            .map(EirCrossResourceInvariant::lower)
            .collect::<Vec<_>>();
        let mut fingerprint = Fingerprint::EMPTY
            .text("eir-resource-group")
            .number(u64::from(EIR_RESOURCE_GROUP_SCHEMA_VERSION))
            .text(group.id().as_str())
            .number(members.len() as u64);
        for member in &members {
            fingerprint = fingerprint.text(member);
        }
        fingerprint = fingerprint.number(dependencies.len() as u64);
        for dependency in &dependencies {
            fingerprint = fingerprint
                .text(dependency.dependent())
                .text(dependency.required());
        }
        fingerprint = fingerprint.number(shared_budgets.len() as u64);
        for budget in &shared_budgets {
            fingerprint = fingerprint.number(budget.fingerprint().bits());
        }
        fingerprint = fingerprint.number(cross_invariants.len() as u64);
        for invariant in &cross_invariants {
            fingerprint = fingerprint.number(invariant.fingerprint().bits());
        }

        Ok(Self {
            id: group.id().as_str().to_owned(),
            members,
            dependencies,
            shared_budgets,
            cross_invariants,
            fingerprint,
        })
    }

    /// Stable group identity.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Canonically ordered member resource identities.
    #[must_use]
    pub fn members(&self) -> &[String] {
        &self.members
    }

    /// Canonically ordered dependency edges.
    #[must_use]
    pub fn dependencies(&self) -> &[EirResourceDependency] {
        &self.dependencies
    }

    /// Canonical shared-budget contracts.
    #[must_use]
    pub fn shared_budgets(&self) -> &[EirSharedBudget] {
        &self.shared_budgets
    }

    /// Canonical explicitly owned cross-resource invariants.
    #[must_use]
    pub fn cross_invariants(&self) -> &[EirCrossResourceInvariant] {
        &self.cross_invariants
    }

    /// Structural group fingerprint.
    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }

    /// Whether this group contains the named logical resource.
    #[must_use]
    pub fn contains(&self, resource: &str) -> bool {
        self.members
            .binary_search_by(|member| member.as_str().cmp(resource))
            .is_ok()
    }
}

/// Validated EIR document plus canonical cross-resource groups.
///
/// The base document retains its historical schema/fingerprint unchanged. This
/// envelope binds that document identity to a separate v1 group topology.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EirGroupedDocument {
    document: EirDocument,
    groups: Vec<EirResourceGroup>,
    fingerprint: Fingerprint,
}

impl EirGroupedDocument {
    /// Bind one validated multi-resource EIR document to typed resource groups.
    pub fn new(
        document: EirDocument,
        groups: &[ResourceGroup],
    ) -> Result<Self, GroupLoweringError> {
        if groups.is_empty() {
            return Err(GroupLoweringError::EmptyGroupEnvelope);
        }
        if groups.len() > MAX_EIR_RESOURCE_GROUPS {
            return Err(GroupLoweringError::TooManyGroups {
                groups: groups.len(),
                maximum: MAX_EIR_RESOURCE_GROUPS,
            });
        }

        let mut lowered = groups
            .iter()
            .map(|group| EirResourceGroup::lower(group, &document))
            .collect::<Result<Vec<_>, _>>()?;
        lowered.sort_by(|left, right| left.id.cmp(&right.id));
        for pair in lowered.windows(2) {
            if pair[0].id == pair[1].id {
                return Err(GroupLoweringError::DuplicateGroupIdentity {
                    group: pair[0].id.clone(),
                });
            }
        }

        let mut fingerprint = Fingerprint::EMPTY
            .text("eir-grouped-document")
            .number(u64::from(EIR_RESOURCE_GROUP_SCHEMA_VERSION))
            .number(document.fingerprint().bits())
            .number(lowered.len() as u64);
        for group in &lowered {
            fingerprint = fingerprint.number(group.fingerprint().bits());
        }

        Ok(Self {
            document,
            groups: lowered,
            fingerprint,
        })
    }

    /// Base ELANG2 multi-resource EIR document.
    #[must_use]
    pub const fn document(&self) -> &EirDocument {
        &self.document
    }

    /// Canonically sorted groups.
    #[must_use]
    pub fn groups(&self) -> &[EirResourceGroup] {
        &self.groups
    }

    /// Look up one group by stable identity.
    #[must_use]
    pub fn group(&self, id: &str) -> Option<&EirResourceGroup> {
        self.groups
            .binary_search_by(|group| group.id.as_str().cmp(id))
            .ok()
            .map(|index| &self.groups[index])
    }

    /// Resolve one group member back to its authoritative EIR resource node.
    ///
    /// This is the explicit source mapping from group topology to the ELANG2
    /// document. `None` means either the group does not exist or the resource is
    /// not a member of that group.
    #[must_use]
    pub fn group_resource(&self, group: &str, resource: &str) -> Option<&EirResource> {
        let group = self.group(group)?;
        if !group.contains(resource) {
            return None;
        }
        self.document.resource(resource)
    }

    /// Structural identity of base document plus group topology.
    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
}

/// Lowering/validation failures for the EIR group envelope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GroupLoweringError {
    EmptyGroupEnvelope,
    TooManyGroups { groups: usize, maximum: usize },
    DuplicateGroupIdentity { group: String },
    UnknownResource { group: String, resource: String },
}

impl fmt::Display for GroupLoweringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyGroupEnvelope => write!(f, "grouped EIR requires at least one resource group"),
            Self::TooManyGroups { groups, maximum } => write!(
                f,
                "grouped EIR contains {groups} groups; maximum is {maximum}"
            ),
            Self::DuplicateGroupIdentity { group } => {
                write!(f, "grouped EIR contains duplicate group identity {group}")
            }
            Self::UnknownResource { group, resource } => write!(
                f,
                "resource group {group} references {resource}, which is absent from the EIR document"
            ),
        }
    }
}

impl std::error::Error for GroupLoweringError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EirDocumentBuilder;
    use elastic_core::resource::{
        ContractId, CrossResourceInvariant, DimensionId, LogicalResourceId, ResourceClassId,
        ResourceDependency, ResourceGroup, ResourceGroupBuilder, ResourceGroupId, ResourceSpec,
        SharedBudget, SharedBudgetId, SharedBudgetTerm,
    };
    use elastic_core::{PredicateKey, PseudoBooleanScale};

    fn spec(id: &str) -> ResourceSpec {
        ResourceSpec::builder(
            ResourceClassId::CONFIGURATIONAL,
            LogicalResourceId::new(id).unwrap(),
        )
        .allow(DimensionId::CAPACITY)
        .build()
        .unwrap()
    }

    fn document() -> EirDocument {
        let mut builder = EirDocumentBuilder::new();
        for id in ["flight", "inference", "vision"] {
            builder.push(&spec(id)).unwrap();
        }
        builder.finish().unwrap()
    }

    fn id(value: &str) -> LogicalResourceId {
        LogicalResourceId::new(value).unwrap()
    }

    #[test]
    fn group_order_does_not_change_grouped_document_fingerprint() {
        let flight = ResourceGroup::new(
            ResourceGroupId::new("critical").unwrap(),
            vec![id("flight")],
            vec![],
        )
        .unwrap();
        let ai = ResourceGroup::new(
            ResourceGroupId::new("ai").unwrap(),
            vec![id("vision"), id("inference"), id("flight")],
            vec![
                ResourceDependency::new(id("vision"), id("flight")),
                ResourceDependency::new(id("inference"), id("flight")),
            ],
        )
        .unwrap();
        let left = EirGroupedDocument::new(document(), &[flight.clone(), ai.clone()]).unwrap();
        let right = EirGroupedDocument::new(document(), &[ai, flight]).unwrap();
        assert_eq!(left, right);
        assert_eq!(left.fingerprint(), right.fingerprint());
        assert_eq!(left.groups()[0].id(), "ai");
        assert_eq!(
            left.group_resource("ai", "inference")
                .unwrap()
                .identity()
                .as_str(),
            "inference"
        );
        assert!(left.group_resource("critical", "vision").is_none());
    }

    #[test]
    fn unknown_document_member_and_duplicate_group_id_fail_closed() {
        let unknown = ResourceGroup::new(
            ResourceGroupId::new("unknown").unwrap(),
            vec![id("outside")],
            vec![],
        )
        .unwrap();
        assert!(matches!(
            EirGroupedDocument::new(document(), &[unknown]),
            Err(GroupLoweringError::UnknownResource { .. })
        ));

        let first = ResourceGroup::new(
            ResourceGroupId::new("same").unwrap(),
            vec![id("flight")],
            vec![],
        )
        .unwrap();
        let second = ResourceGroup::new(
            ResourceGroupId::new("same").unwrap(),
            vec![id("vision")],
            vec![],
        )
        .unwrap();
        assert!(matches!(
            EirGroupedDocument::new(document(), &[first, second]),
            Err(GroupLoweringError::DuplicateGroupIdentity { .. })
        ));
    }

    fn predicate(name: &str) -> PredicateKey {
        PredicateKey::new("elastic.group.eir-test", name).unwrap()
    }

    fn ai_group(vision_weight: i128, inference_weight: i128) -> ResourceGroup {
        let budget = SharedBudget::new(
            SharedBudgetId::new("memory").unwrap(),
            vec![
                SharedBudgetTerm::new(id("vision"), predicate("vision-high"), vision_weight)
                    .unwrap(),
                SharedBudgetTerm::new(
                    id("inference"),
                    predicate("inference-high"),
                    inference_weight,
                )
                .unwrap(),
            ],
            10,
            PseudoBooleanScale::new("gib", 1).unwrap(),
        )
        .unwrap();
        let invariant = CrossResourceInvariant::new(
            ContractId::new("flight-priority-preserved").unwrap(),
            id("flight"),
            vec![id("vision"), id("flight"), id("inference")],
        )
        .unwrap();
        ResourceGroupBuilder::new(ResourceGroupId::new("ai").unwrap())
            .members([id("vision"), id("inference"), id("flight")])
            .dependency(ResourceDependency::new(id("vision"), id("flight")))
            .dependency(ResourceDependency::new(id("inference"), id("flight")))
            .shared_budget(budget)
            .cross_invariant(invariant)
            .build()
            .unwrap()
    }

    #[test]
    fn shared_budget_and_cross_invariant_are_source_mapped_into_eir() {
        let grouped = EirGroupedDocument::new(document(), &[ai_group(6, 8)]).unwrap();
        let group = grouped.group("ai").unwrap();
        assert_eq!(group.shared_budgets().len(), 1);
        let budget = &group.shared_budgets()[0];
        assert_eq!(budget.id(), "memory");
        assert_eq!(budget.constraint().threshold(), 10);
        assert_eq!(budget.constraint().scale().unit(), "gib");
        assert_eq!(budget.terms()[0].resource(), "inference");
        assert_eq!(budget.terms()[1].resource(), "vision");
        assert_eq!(group.cross_invariants().len(), 1);
        let invariant = &group.cross_invariants()[0];
        assert_eq!(invariant.contract(), "flight-priority-preserved");
        assert_eq!(invariant.owner(), "flight");
        assert_eq!(invariant.participants(), &["flight", "inference", "vision"]);
    }

    #[test]
    fn source_cost_assignment_changes_group_fingerprint() {
        let left = EirGroupedDocument::new(document(), &[ai_group(6, 8)]).unwrap();
        let right = EirGroupedDocument::new(document(), &[ai_group(8, 6)]).unwrap();
        assert_ne!(
            left.groups()[0].fingerprint(),
            right.groups()[0].fingerprint()
        );
        assert_ne!(left.fingerprint(), right.fingerprint());
    }
}
