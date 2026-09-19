//! Typed cross-resource grouping and dependency declarations.
//!
//! Groups are pure intent. They name a bounded set of logical resources and
//! directed dependency edges between those members. They do not schedule,
//! allocate, plan, or actuate anything. Cross-resource execution semantics live
//! in later layers.

use super::{ContractId, LogicalResourceId};
use crate::{
    PredicateKey, PseudoBooleanBindingError, PseudoBooleanConstraintDeclaration,
    PseudoBooleanScale, WeightedPredicateKey,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Maximum UTF-8 byte length of one resource-group identity.
pub const MAX_RESOURCE_GROUP_ID_BYTES: usize = 256;
/// Maximum number of resource members in one group.
pub const MAX_RESOURCE_GROUP_MEMBERS: usize = 256;
/// Maximum number of directed dependency edges in one group.
pub const MAX_RESOURCE_GROUP_DEPENDENCIES: usize = 1024;
/// Maximum number of shared budgets in one group.
pub const MAX_RESOURCE_GROUP_SHARED_BUDGETS: usize = 64;
/// Maximum number of cross-resource invariants in one group.
pub const MAX_RESOURCE_GROUP_CROSS_INVARIANTS: usize = 64;
/// Maximum UTF-8 byte length of one shared-budget identity.
pub const MAX_SHARED_BUDGET_ID_BYTES: usize = 256;

/// Stable identity of one resource group.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResourceGroupId(String);

impl ResourceGroupId {
    /// Construct a non-empty, trimmed, bounded resource-group identity.
    pub fn new(value: impl Into<String>) -> Result<Self, ResourceGroupError> {
        let value = value.into();
        if value.trim().is_empty() || value.trim() != value {
            return Err(ResourceGroupError::InvalidGroupId);
        }
        if value.len() > MAX_RESOURCE_GROUP_ID_BYTES {
            return Err(ResourceGroupError::GroupIdTooLong {
                bytes: value.len(),
                maximum: MAX_RESOURCE_GROUP_ID_BYTES,
            });
        }
        Ok(Self(value))
    }

    /// Borrow the canonical group identity text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ResourceGroupId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Directed dependency `dependent -> required` between two group members.
///
/// The edge means that a future planner/transaction may not treat `dependent`
/// as independent from `required`. This type alone does not define execution
/// ordering or actuation authority.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResourceDependency {
    dependent: LogicalResourceId,
    required: LogicalResourceId,
}

impl ResourceDependency {
    /// Declare one directed dependency.
    #[must_use]
    pub const fn new(dependent: LogicalResourceId, required: LogicalResourceId) -> Self {
        Self {
            dependent,
            required,
        }
    }

    /// Resource whose policy/execution depends on another member.
    #[must_use]
    pub const fn dependent(&self) -> &LogicalResourceId {
        &self.dependent
    }

    /// Resource that the dependent requires.
    #[must_use]
    pub const fn required(&self) -> &LogicalResourceId {
        &self.required
    }
}

impl fmt::Display for ResourceDependency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} -> {}", self.dependent, self.required)
    }
}

/// Stable identity of one shared group budget.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SharedBudgetId(String);

impl SharedBudgetId {
    /// Construct a non-empty, trimmed, bounded budget identity.
    pub fn new(value: impl Into<String>) -> Result<Self, SharedBudgetError> {
        let value = value.into();
        if value.trim().is_empty() || value.trim() != value {
            return Err(SharedBudgetError::InvalidBudgetId);
        }
        if value.len() > MAX_SHARED_BUDGET_ID_BYTES {
            return Err(SharedBudgetError::BudgetIdTooLong {
                bytes: value.len(),
                maximum: MAX_SHARED_BUDGET_ID_BYTES,
            });
        }
        Ok(Self(value))
    }

    /// Canonical budget identity text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SharedBudgetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One resource-owned contribution to a shared pseudo-Boolean budget.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SharedBudgetTerm {
    resource: LogicalResourceId,
    predicate: PredicateKey,
    weight: i128,
}

impl SharedBudgetTerm {
    /// Construct one strictly positive capacity-cost term.
    pub fn new(
        resource: LogicalResourceId,
        predicate: PredicateKey,
        weight: i128,
    ) -> Result<Self, SharedBudgetError> {
        if weight <= 0 {
            return Err(SharedBudgetError::NonPositiveWeight {
                resource,
                predicate,
                weight,
            });
        }
        Ok(Self {
            resource,
            predicate,
            weight,
        })
    }

    /// Resource whose candidate/state owns this cost term.
    #[must_use]
    pub const fn resource(&self) -> &LogicalResourceId {
        &self.resource
    }

    /// Stable predicate whose truth activates the cost.
    #[must_use]
    pub const fn predicate(&self) -> &PredicateKey {
        &self.predicate
    }

    /// Positive cost in the budget scale's integer ticks.
    #[must_use]
    pub const fn weight(&self) -> i128 {
        self.weight
    }
}

/// One shared group capacity budget lowered through the existing pseudo-Boolean core.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharedBudget {
    id: SharedBudgetId,
    terms: Vec<SharedBudgetTerm>,
    maximum: i128,
    scale: PseudoBooleanScale,
    constraint: PseudoBooleanConstraintDeclaration,
}

impl SharedBudget {
    /// Construct a canonical non-negative shared capacity budget.
    pub fn new(
        id: SharedBudgetId,
        mut terms: Vec<SharedBudgetTerm>,
        maximum: i128,
        scale: PseudoBooleanScale,
    ) -> Result<Self, SharedBudgetError> {
        if terms.is_empty() {
            return Err(SharedBudgetError::EmptyBudget { budget: id });
        }
        if maximum < 0 {
            return Err(SharedBudgetError::NegativeMaximum {
                budget: id,
                maximum,
            });
        }
        terms.sort();
        for pair in terms.windows(2) {
            if pair[0].predicate == pair[1].predicate {
                return Err(SharedBudgetError::DuplicatePredicate {
                    budget: id,
                    predicate: pair[0].predicate.clone(),
                });
            }
        }
        let declaration_terms = terms
            .iter()
            .map(|term| WeightedPredicateKey::new(term.predicate.clone(), term.weight))
            .collect::<Result<Vec<_>, _>>()?;
        let constraint = PseudoBooleanConstraintDeclaration::capacity_budget(
            declaration_terms,
            maximum,
            scale.clone(),
        )?;
        Ok(Self {
            id,
            terms,
            maximum,
            scale,
            constraint,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &SharedBudgetId {
        &self.id
    }

    #[must_use]
    pub fn terms(&self) -> &[SharedBudgetTerm] {
        &self.terms
    }

    #[must_use]
    pub const fn maximum(&self) -> i128 {
        self.maximum
    }

    #[must_use]
    pub const fn scale(&self) -> &PseudoBooleanScale {
        &self.scale
    }

    /// Existing durable pseudo-Boolean constraint carrying the actual budget semantics.
    #[must_use]
    pub const fn constraint(&self) -> &PseudoBooleanConstraintDeclaration {
        &self.constraint
    }
}

/// Explicitly owned invariant spanning two or more resources.
///
/// `contract` remains an external semantic contract. Elastic records ownership
/// and participant identity but does not interpret or prove the contract here.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CrossResourceInvariant {
    contract: ContractId,
    owner: LogicalResourceId,
    participants: Vec<LogicalResourceId>,
}

impl CrossResourceInvariant {
    /// Construct a canonical cross-resource contract with one explicit owner.
    pub fn new(
        contract: ContractId,
        owner: LogicalResourceId,
        mut participants: Vec<LogicalResourceId>,
    ) -> Result<Self, CrossResourceInvariantError> {
        if participants.len() < 2 {
            return Err(CrossResourceInvariantError::TooFewParticipants { contract });
        }
        participants.sort();
        for pair in participants.windows(2) {
            if pair[0] == pair[1] {
                return Err(CrossResourceInvariantError::DuplicateParticipant {
                    contract,
                    participant: pair[0].clone(),
                });
            }
        }
        if participants.binary_search(&owner).is_err() {
            return Err(CrossResourceInvariantError::OwnerNotParticipant { contract, owner });
        }
        Ok(Self {
            contract,
            owner,
            participants,
        })
    }

    #[must_use]
    pub const fn contract(&self) -> &ContractId {
        &self.contract
    }

    #[must_use]
    pub const fn owner(&self) -> &LogicalResourceId {
        &self.owner
    }

    #[must_use]
    pub fn participants(&self) -> &[LogicalResourceId] {
        &self.participants
    }
}

/// Shared-budget construction failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SharedBudgetError {
    InvalidBudgetId,
    BudgetIdTooLong {
        bytes: usize,
        maximum: usize,
    },
    EmptyBudget {
        budget: SharedBudgetId,
    },
    NegativeMaximum {
        budget: SharedBudgetId,
        maximum: i128,
    },
    NonPositiveWeight {
        resource: LogicalResourceId,
        predicate: PredicateKey,
        weight: i128,
    },
    DuplicatePredicate {
        budget: SharedBudgetId,
        predicate: PredicateKey,
    },
    PseudoBoolean(PseudoBooleanBindingError),
}

impl From<PseudoBooleanBindingError> for SharedBudgetError {
    fn from(value: PseudoBooleanBindingError) -> Self {
        Self::PseudoBoolean(value)
    }
}

impl fmt::Display for SharedBudgetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBudgetId => {
                write!(f, "shared-budget identity must be non-empty and trimmed")
            }
            Self::BudgetIdTooLong { bytes, maximum } => write!(
                f,
                "shared-budget identity length {bytes} exceeds maximum {maximum} bytes"
            ),
            Self::EmptyBudget { budget } => write!(f, "shared budget {budget} has no cost terms"),
            Self::NegativeMaximum { budget, maximum } => {
                write!(f, "shared budget {budget} has negative maximum {maximum}")
            }
            Self::NonPositiveWeight {
                resource,
                predicate,
                weight,
            } => write!(
                f,
                "shared-budget term {resource}/{predicate} has non-positive weight {weight}"
            ),
            Self::DuplicatePredicate { budget, predicate } => write!(
                f,
                "shared budget {budget} declares predicate {predicate} more than once"
            ),
            Self::PseudoBoolean(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for SharedBudgetError {}

/// Cross-resource invariant construction failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CrossResourceInvariantError {
    TooFewParticipants {
        contract: ContractId,
    },
    DuplicateParticipant {
        contract: ContractId,
        participant: LogicalResourceId,
    },
    OwnerNotParticipant {
        contract: ContractId,
        owner: LogicalResourceId,
    },
}

impl fmt::Display for CrossResourceInvariantError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooFewParticipants { contract } => write!(
                f,
                "cross-resource invariant {contract} requires at least two participants"
            ),
            Self::DuplicateParticipant {
                contract,
                participant,
            } => write!(
                f,
                "cross-resource invariant {contract} repeats participant {participant}"
            ),
            Self::OwnerNotParticipant { contract, owner } => write!(
                f,
                "cross-resource invariant {contract} owner {owner} must also be a participant"
            ),
        }
    }
}
impl std::error::Error for CrossResourceInvariantError {}

/// Validated, deterministic resource group.
///
/// Members and dependencies are stored in canonical sorted order. Every
/// dependency endpoint must be a member of the group, self-dependencies and
/// duplicates are rejected, and the directed dependency graph must be acyclic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceGroup {
    id: ResourceGroupId,
    members: Vec<LogicalResourceId>,
    dependencies: Vec<ResourceDependency>,
    shared_budgets: Vec<SharedBudget>,
    cross_invariants: Vec<CrossResourceInvariant>,
}

impl ResourceGroup {
    /// Construct and validate one resource group.
    pub fn new(
        id: ResourceGroupId,
        members: Vec<LogicalResourceId>,
        dependencies: Vec<ResourceDependency>,
    ) -> Result<Self, ResourceGroupError> {
        ResourceGroupBuilder::new(id)
            .members(members)
            .dependencies(dependencies)
            .build()
    }

    /// Stable group identity.
    #[must_use]
    pub const fn id(&self) -> &ResourceGroupId {
        &self.id
    }

    /// Canonically sorted logical members.
    #[must_use]
    pub fn members(&self) -> &[LogicalResourceId] {
        &self.members
    }

    /// Canonically sorted dependency edges.
    #[must_use]
    pub fn dependencies(&self) -> &[ResourceDependency] {
        &self.dependencies
    }

    /// Canonically ordered shared budgets.
    #[must_use]
    pub fn shared_budgets(&self) -> &[SharedBudget] {
        &self.shared_budgets
    }

    /// Canonically ordered cross-resource invariants.
    #[must_use]
    pub fn cross_invariants(&self) -> &[CrossResourceInvariant] {
        &self.cross_invariants
    }

    /// Whether this group contains a logical resource.
    #[must_use]
    pub fn contains(&self, resource: &LogicalResourceId) -> bool {
        self.members.binary_search(resource).is_ok()
    }
}

/// Builder for one bounded resource group.
#[derive(Clone, Debug)]
pub struct ResourceGroupBuilder {
    id: ResourceGroupId,
    members: Vec<LogicalResourceId>,
    dependencies: Vec<ResourceDependency>,
    shared_budgets: Vec<SharedBudget>,
    cross_invariants: Vec<CrossResourceInvariant>,
}

impl ResourceGroupBuilder {
    /// Start a group declaration.
    #[must_use]
    pub fn new(id: ResourceGroupId) -> Self {
        Self {
            id,
            members: Vec::new(),
            dependencies: Vec::new(),
            shared_budgets: Vec::new(),
            cross_invariants: Vec::new(),
        }
    }

    /// Append one member.
    #[must_use]
    pub fn member(mut self, member: LogicalResourceId) -> Self {
        self.members.push(member);
        self
    }

    /// Append several members.
    #[must_use]
    pub fn members(mut self, members: impl IntoIterator<Item = LogicalResourceId>) -> Self {
        self.members.extend(members);
        self
    }

    /// Append one directed dependency.
    #[must_use]
    pub fn dependency(mut self, dependency: ResourceDependency) -> Self {
        self.dependencies.push(dependency);
        self
    }

    /// Append several directed dependencies.
    #[must_use]
    pub fn dependencies(
        mut self,
        dependencies: impl IntoIterator<Item = ResourceDependency>,
    ) -> Self {
        self.dependencies.extend(dependencies);
        self
    }

    /// Append one shared capacity budget.
    #[must_use]
    pub fn shared_budget(mut self, budget: SharedBudget) -> Self {
        self.shared_budgets.push(budget);
        self
    }

    /// Append one explicitly owned cross-resource invariant.
    #[must_use]
    pub fn cross_invariant(mut self, invariant: CrossResourceInvariant) -> Self {
        self.cross_invariants.push(invariant);
        self
    }

    /// Validate and normalize the group.
    pub fn build(mut self) -> Result<ResourceGroup, ResourceGroupError> {
        if self.members.is_empty() {
            return Err(ResourceGroupError::EmptyGroup {
                group: self.id.clone(),
            });
        }
        if self.members.len() > MAX_RESOURCE_GROUP_MEMBERS {
            return Err(ResourceGroupError::TooManyMembers {
                group: self.id.clone(),
                members: self.members.len(),
                maximum: MAX_RESOURCE_GROUP_MEMBERS,
            });
        }
        if self.dependencies.len() > MAX_RESOURCE_GROUP_DEPENDENCIES {
            return Err(ResourceGroupError::TooManyDependencies {
                group: self.id.clone(),
                dependencies: self.dependencies.len(),
                maximum: MAX_RESOURCE_GROUP_DEPENDENCIES,
            });
        }

        if self.shared_budgets.len() > MAX_RESOURCE_GROUP_SHARED_BUDGETS {
            return Err(ResourceGroupError::TooManySharedBudgets {
                group: self.id.clone(),
                budgets: self.shared_budgets.len(),
                maximum: MAX_RESOURCE_GROUP_SHARED_BUDGETS,
            });
        }
        if self.cross_invariants.len() > MAX_RESOURCE_GROUP_CROSS_INVARIANTS {
            return Err(ResourceGroupError::TooManyCrossInvariants {
                group: self.id.clone(),
                invariants: self.cross_invariants.len(),
                maximum: MAX_RESOURCE_GROUP_CROSS_INVARIANTS,
            });
        }

        self.members.sort();
        for pair in self.members.windows(2) {
            if pair[0] == pair[1] {
                return Err(ResourceGroupError::DuplicateMember {
                    group: self.id.clone(),
                    member: pair[0].clone(),
                });
            }
        }

        let member_set = self.members.iter().cloned().collect::<BTreeSet<_>>();
        for dependency in &self.dependencies {
            if dependency.dependent == dependency.required {
                return Err(ResourceGroupError::SelfDependency {
                    group: self.id.clone(),
                    resource: dependency.dependent.clone(),
                });
            }
            for endpoint in [&dependency.dependent, &dependency.required] {
                if !member_set.contains(endpoint) {
                    return Err(ResourceGroupError::UnknownDependencyMember {
                        group: self.id.clone(),
                        member: endpoint.clone(),
                    });
                }
            }
        }

        self.dependencies.sort();
        for pair in self.dependencies.windows(2) {
            if pair[0] == pair[1] {
                return Err(ResourceGroupError::DuplicateDependency {
                    group: self.id.clone(),
                    dependency: pair[0].clone(),
                });
            }
        }
        reject_cycles(&self.id, &self.members, &self.dependencies)?;

        self.shared_budgets
            .sort_by(|left, right| left.id.cmp(&right.id));
        for pair in self.shared_budgets.windows(2) {
            if pair[0].id == pair[1].id {
                return Err(ResourceGroupError::DuplicateSharedBudget {
                    group: self.id.clone(),
                    budget: pair[0].id.clone(),
                });
            }
        }
        for budget in &self.shared_budgets {
            for term in budget.terms() {
                if !member_set.contains(term.resource()) {
                    return Err(ResourceGroupError::UnknownSharedBudgetMember {
                        group: self.id.clone(),
                        budget: budget.id.clone(),
                        member: term.resource().clone(),
                    });
                }
            }
        }

        self.cross_invariants.sort();
        for pair in self.cross_invariants.windows(2) {
            if pair[0].contract == pair[1].contract {
                return Err(ResourceGroupError::DuplicateCrossInvariant {
                    group: self.id.clone(),
                    contract: pair[0].contract.clone(),
                });
            }
        }
        for invariant in &self.cross_invariants {
            for participant in invariant.participants() {
                if !member_set.contains(participant) {
                    return Err(ResourceGroupError::UnknownCrossInvariantMember {
                        group: self.id.clone(),
                        contract: invariant.contract.clone(),
                        member: participant.clone(),
                    });
                }
            }
        }

        Ok(ResourceGroup {
            id: self.id,
            members: self.members,
            dependencies: self.dependencies,
            shared_budgets: self.shared_budgets,
            cross_invariants: self.cross_invariants,
        })
    }
}

fn reject_cycles(
    group: &ResourceGroupId,
    members: &[LogicalResourceId],
    dependencies: &[ResourceDependency],
) -> Result<(), ResourceGroupError> {
    let indexes = members
        .iter()
        .enumerate()
        .map(|(index, member)| (member.clone(), index))
        .collect::<BTreeMap<_, _>>();
    let mut outgoing = vec![Vec::<usize>::new(); members.len()];

    // `dependent -> required` retains the declared direction. The graph is
    // already bounded and dependencies are canonical, so a deterministic DFS
    // can report an actual cycle rather than every node merely blocked by one.
    for dependency in dependencies {
        let from = indexes[dependency.dependent()];
        let to = indexes[dependency.required()];
        outgoing[from].push(to);
    }

    let Some(mut cycle) = find_dependency_cycle(&outgoing) else {
        return Ok(());
    };
    cycle.sort_unstable();
    cycle.dedup();
    let cycle_members = cycle
        .into_iter()
        .map(|index| members[index].clone())
        .collect();
    Err(ResourceGroupError::DependencyCycle {
        group: group.clone(),
        members: cycle_members,
    })
}

fn find_dependency_cycle(outgoing: &[Vec<usize>]) -> Option<Vec<usize>> {
    fn visit(
        node: usize,
        outgoing: &[Vec<usize>],
        state: &mut [u8],
        stack: &mut Vec<usize>,
        stack_position: &mut [Option<usize>],
    ) -> Option<Vec<usize>> {
        state[node] = 1;
        stack_position[node] = Some(stack.len());
        stack.push(node);

        for &next in &outgoing[node] {
            match state[next] {
                0 => {
                    if let Some(cycle) = visit(next, outgoing, state, stack, stack_position) {
                        return Some(cycle);
                    }
                }
                1 => {
                    let Some(start) = stack_position[next] else {
                        // A visiting node without a stack position would mean
                        // internal DFS state corruption. Fail closed by
                        // reporting that node as cyclic instead of panicking.
                        return Some(vec![next]);
                    };
                    return Some(stack[start..].to_vec());
                }
                _ => {}
            }
        }

        stack.pop();
        stack_position[node] = None;
        state[node] = 2;
        None
    }

    let mut state = vec![0_u8; outgoing.len()];
    let mut stack = Vec::with_capacity(outgoing.len());
    let mut stack_position = vec![None; outgoing.len()];
    for node in 0..outgoing.len() {
        if state[node] == 0 {
            if let Some(cycle) = visit(node, outgoing, &mut state, &mut stack, &mut stack_position)
            {
                return Some(cycle);
            }
        }
    }
    None
}

/// Validation failures for resource-group declarations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResourceGroupError {
    InvalidGroupId,
    GroupIdTooLong {
        bytes: usize,
        maximum: usize,
    },
    EmptyGroup {
        group: ResourceGroupId,
    },
    TooManyMembers {
        group: ResourceGroupId,
        members: usize,
        maximum: usize,
    },
    DuplicateMember {
        group: ResourceGroupId,
        member: LogicalResourceId,
    },
    TooManyDependencies {
        group: ResourceGroupId,
        dependencies: usize,
        maximum: usize,
    },
    UnknownDependencyMember {
        group: ResourceGroupId,
        member: LogicalResourceId,
    },
    SelfDependency {
        group: ResourceGroupId,
        resource: LogicalResourceId,
    },
    DuplicateDependency {
        group: ResourceGroupId,
        dependency: ResourceDependency,
    },
    DependencyCycle {
        group: ResourceGroupId,
        members: Vec<LogicalResourceId>,
    },
    TooManySharedBudgets {
        group: ResourceGroupId,
        budgets: usize,
        maximum: usize,
    },
    DuplicateSharedBudget {
        group: ResourceGroupId,
        budget: SharedBudgetId,
    },
    UnknownSharedBudgetMember {
        group: ResourceGroupId,
        budget: SharedBudgetId,
        member: LogicalResourceId,
    },
    TooManyCrossInvariants {
        group: ResourceGroupId,
        invariants: usize,
        maximum: usize,
    },
    DuplicateCrossInvariant {
        group: ResourceGroupId,
        contract: ContractId,
    },
    UnknownCrossInvariantMember {
        group: ResourceGroupId,
        contract: ContractId,
        member: LogicalResourceId,
    },
}

impl fmt::Display for ResourceGroupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidGroupId => {
                write!(f, "resource-group identity must be non-empty and trimmed")
            }
            Self::GroupIdTooLong { bytes, maximum } => write!(
                f,
                "resource-group identity length {bytes} exceeds maximum {maximum} bytes"
            ),
            Self::EmptyGroup { group } => write!(f, "resource group {group} has no members"),
            Self::TooManyMembers {
                group,
                members,
                maximum,
            } => write!(
                f,
                "resource group {group} contains {members} members; maximum is {maximum}"
            ),
            Self::DuplicateMember { group, member } => {
                write!(
                    f,
                    "resource group {group} contains duplicate member {member}"
                )
            }
            Self::TooManyDependencies {
                group,
                dependencies,
                maximum,
            } => write!(
                f,
                "resource group {group} contains {dependencies} dependencies; maximum is {maximum}"
            ),
            Self::UnknownDependencyMember { group, member } => write!(
                f,
                "resource group {group} dependency references non-member {member}"
            ),
            Self::SelfDependency { group, resource } => write!(
                f,
                "resource group {group} resource {resource} cannot depend on itself"
            ),
            Self::DuplicateDependency { group, dependency } => write!(
                f,
                "resource group {group} contains duplicate dependency {dependency}"
            ),
            Self::DependencyCycle { group, members } => write!(
                f,
                "resource group {group} contains a dependency cycle involving {}",
                members
                    .iter()
                    .map(LogicalResourceId::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::TooManySharedBudgets { group, budgets, maximum } => write!(f, "resource group {group} contains {budgets} shared budgets; maximum is {maximum}"),
            Self::DuplicateSharedBudget { group, budget } => write!(f, "resource group {group} repeats shared budget {budget}"),
            Self::UnknownSharedBudgetMember { group, budget, member } => write!(f, "resource group {group} shared budget {budget} references non-member {member}"),
            Self::TooManyCrossInvariants { group, invariants, maximum } => write!(f, "resource group {group} contains {invariants} cross-resource invariants; maximum is {maximum}"),
            Self::DuplicateCrossInvariant { group, contract } => write!(f, "resource group {group} repeats cross-resource invariant {contract}"),
            Self::UnknownCrossInvariantMember { group, contract, member } => write!(f, "resource group {group} invariant {contract} references non-member {member}"),
        }
    }
}

impl std::error::Error for ResourceGroupError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(value: &str) -> LogicalResourceId {
        LogicalResourceId::new(value).unwrap()
    }

    #[test]
    fn group_normalizes_member_and_dependency_order() {
        let first = ResourceGroup::new(
            ResourceGroupId::new("drone").unwrap(),
            vec![id("vision"), id("flight"), id("inference")],
            vec![
                ResourceDependency::new(id("vision"), id("flight")),
                ResourceDependency::new(id("inference"), id("flight")),
            ],
        )
        .unwrap();
        let second = ResourceGroup::new(
            ResourceGroupId::new("drone").unwrap(),
            vec![id("inference"), id("flight"), id("vision")],
            vec![
                ResourceDependency::new(id("inference"), id("flight")),
                ResourceDependency::new(id("vision"), id("flight")),
            ],
        )
        .unwrap();
        assert_eq!(first, second);
        assert_eq!(
            first
                .members()
                .iter()
                .map(LogicalResourceId::as_str)
                .collect::<Vec<_>>(),
            vec!["flight", "inference", "vision"]
        );
    }

    #[test]
    fn unknown_self_duplicate_and_cycle_dependencies_fail_closed() {
        let group = ResourceGroupId::new("g").unwrap();
        assert!(matches!(
            ResourceGroup::new(
                group.clone(),
                vec![id("a"), id("b")],
                vec![ResourceDependency::new(id("a"), id("outside"))]
            ),
            Err(ResourceGroupError::UnknownDependencyMember { .. })
        ));
        assert!(matches!(
            ResourceGroup::new(
                group.clone(),
                vec![id("a")],
                vec![ResourceDependency::new(id("a"), id("a"))]
            ),
            Err(ResourceGroupError::SelfDependency { .. })
        ));
        assert!(matches!(
            ResourceGroup::new(
                group.clone(),
                vec![id("a"), id("b")],
                vec![
                    ResourceDependency::new(id("a"), id("b")),
                    ResourceDependency::new(id("a"), id("b")),
                ]
            ),
            Err(ResourceGroupError::DuplicateDependency { .. })
        ));
        assert!(matches!(
            ResourceGroup::new(
                group,
                vec![id("a"), id("b"), id("c")],
                vec![
                    ResourceDependency::new(id("a"), id("b")),
                    ResourceDependency::new(id("b"), id("c")),
                    ResourceDependency::new(id("c"), id("a")),
                ]
            ),
            Err(ResourceGroupError::DependencyCycle { .. })
        ));
        let error = ResourceGroup::new(
            ResourceGroupId::new("precise-cycle").unwrap(),
            vec![id("a"), id("b"), id("c")],
            vec![
                ResourceDependency::new(id("a"), id("b")),
                ResourceDependency::new(id("a"), id("c")),
                ResourceDependency::new(id("b"), id("a")),
            ],
        )
        .unwrap_err();
        match error {
            ResourceGroupError::DependencyCycle { members, .. } => assert_eq!(
                members
                    .iter()
                    .map(LogicalResourceId::as_str)
                    .collect::<Vec<_>>(),
                vec!["a", "b"]
            ),
            other => panic!("expected precise dependency cycle, got {other:?}"),
        }
    }

    fn predicate(name: &str) -> PredicateKey {
        PredicateKey::new("elastic.group.test", name).unwrap()
    }

    #[test]
    fn shared_budget_reuses_pseudo_boolean_capacity_semantics() {
        let budget = SharedBudget::new(
            SharedBudgetId::new("memory").unwrap(),
            vec![
                SharedBudgetTerm::new(id("vision"), predicate("vision-high"), 6).unwrap(),
                SharedBudgetTerm::new(id("inference"), predicate("inference-high"), 8).unwrap(),
            ],
            10,
            PseudoBooleanScale::new("gib", 1).unwrap(),
        )
        .unwrap();
        assert_eq!(budget.maximum(), 10);
        assert_eq!(budget.scale().unit(), "gib");
        assert_eq!(budget.constraint().threshold(), 10);
        assert_eq!(budget.constraint().terms().len(), 2);
        assert!(budget
            .constraint()
            .terms()
            .iter()
            .all(|term| term.weight() > 0));
    }

    #[test]
    fn group_rejects_shared_budget_and_invariant_members_outside_group() {
        let budget = SharedBudget::new(
            SharedBudgetId::new("memory").unwrap(),
            vec![SharedBudgetTerm::new(id("outside"), predicate("outside"), 1).unwrap()],
            2,
            PseudoBooleanScale::count(),
        )
        .unwrap();
        assert!(matches!(
            ResourceGroupBuilder::new(ResourceGroupId::new("g").unwrap())
                .members([id("a"), id("b")])
                .shared_budget(budget)
                .build(),
            Err(ResourceGroupError::UnknownSharedBudgetMember { .. })
        ));

        let invariant = CrossResourceInvariant::new(
            ContractId::new("safety").unwrap(),
            id("a"),
            vec![id("a"), id("outside")],
        )
        .unwrap();
        assert!(matches!(
            ResourceGroupBuilder::new(ResourceGroupId::new("g").unwrap())
                .members([id("a"), id("b")])
                .cross_invariant(invariant)
                .build(),
            Err(ResourceGroupError::UnknownCrossInvariantMember { .. })
        ));
    }

    #[test]
    fn cross_resource_invariant_requires_explicit_participating_owner() {
        assert!(matches!(
            CrossResourceInvariant::new(
                ContractId::new("safety").unwrap(),
                id("owner"),
                vec![id("a"), id("b")],
            ),
            Err(CrossResourceInvariantError::OwnerNotParticipant { .. })
        ));
        let invariant = CrossResourceInvariant::new(
            ContractId::new("safety").unwrap(),
            id("owner"),
            vec![id("worker"), id("owner")],
        )
        .unwrap();
        assert_eq!(invariant.owner().as_str(), "owner");
        assert_eq!(
            invariant
                .participants()
                .iter()
                .map(LogicalResourceId::as_str)
                .collect::<Vec<_>>(),
            vec!["owner", "worker"]
        );
    }
}
