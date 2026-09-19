//! ELANG4a deterministic multi-resource plan envelopes.
//!
//! This layer is deliberately non-actuating. It binds candidate-bearing plans
//! to one exact [`EirGroupedDocument`], rejects structural conflicts and orders
//! targeted resources according to ELANG3 dependency topology. Physical
//! prepare/actuate/verify/commit-or-rollback semantics are separate later
//! slices and must not be inferred from this envelope alone.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use elastic_core::resource::MAX_RESOURCE_GROUP_MEMBERS;
use elastic_core::TransitionMechanism;
use elastic_eir::{EirGroupedDocument, EirResourceGroup, Fingerprint};

use crate::Plan;

/// Schema version of the composite-plan envelope.
pub const COMPOSITE_PLAN_ENVELOPE_SCHEMA_V1: u16 = 1;
/// Maximum targeted resources in one composite plan.
pub const MAX_COMPOSITE_SUBPLANS: usize = MAX_RESOURCE_GROUP_MEMBERS;

/// A deterministic, non-authoritative envelope over several resource plans.
///
/// Plans are retained in execution order: resources required by another
/// targeted resource appear before their dependents; independent resources are
/// ordered lexicographically by logical identity. This ordering is planning
/// data only. It does not authorize or perform physical mutation.
#[derive(Clone, Debug, PartialEq)]
pub struct CompositePlanEnvelope {
    group_id: String,
    grouped_document_fingerprint: Fingerprint,
    subplans: Vec<Plan>,
    fingerprint: Fingerprint,
}

impl CompositePlanEnvelope {
    /// Validate, canonicalize and bind several candidate-bearing plans to one
    /// exact ELANG3 resource group.
    pub fn new(
        grouped: &EirGroupedDocument,
        group_id: &str,
        plans: Vec<Plan>,
    ) -> Result<Self, CompositePlanError> {
        let group = grouped
            .group(group_id)
            .ok_or_else(|| CompositePlanError::UnknownGroup {
                group: group_id.to_owned(),
            })?;
        if plans.is_empty() {
            return Err(CompositePlanError::EmptyEnvelope {
                group: group_id.to_owned(),
            });
        }
        if plans.len() > MAX_COMPOSITE_SUBPLANS || plans.len() > group.members().len() {
            return Err(CompositePlanError::TooManySubplans {
                group: group_id.to_owned(),
                plans: plans.len(),
                maximum: MAX_COMPOSITE_SUBPLANS.min(group.members().len()),
            });
        }

        let mut by_resource = BTreeMap::<String, Plan>::new();
        for plan in plans {
            let resource_id = plan.resource.identity().as_str().to_owned();
            if !group.contains(&resource_id) {
                return Err(CompositePlanError::ResourceOutsideGroup {
                    group: group_id.to_owned(),
                    resource: resource_id,
                });
            }
            let authoritative =
                grouped
                    .group_resource(group_id, &resource_id)
                    .ok_or_else(|| CompositePlanError::ResourceOutsideGroup {
                        group: group_id.to_owned(),
                        resource: resource_id.clone(),
                    })?;
            if authoritative != &plan.resource {
                return Err(CompositePlanError::ResourceDefinitionMismatch {
                    group: group_id.to_owned(),
                    resource: resource_id,
                    expected: authoritative.fingerprint(),
                    observed: plan.resource.fingerprint(),
                });
            }
            let Some(candidate) = plan.candidate() else {
                return Err(CompositePlanError::MissingCandidate {
                    resource: resource_id,
                });
            };
            if !candidate.is_declared_in(authoritative) {
                return Err(CompositePlanError::UndeclaredCandidate {
                    resource: resource_id,
                });
            }
            for (signal, value) in plan.context.iter() {
                if !value.is_finite() {
                    return Err(CompositePlanError::NonFinitePlanningValue {
                        resource: resource_id,
                        signal: signal.as_str().to_owned(),
                    });
                }
            }
            if by_resource.insert(resource_id.clone(), plan).is_some() {
                return Err(CompositePlanError::DuplicateResourcePlan {
                    resource: resource_id,
                });
            }
        }

        let targeted = by_resource.keys().cloned().collect::<BTreeSet<_>>();
        let order = composite_execution_order(group, &targeted)?;
        let mut subplans = Vec::with_capacity(order.len());
        for resource in order {
            let plan = by_resource.remove(&resource).ok_or_else(|| {
                CompositePlanError::OrderingLostResource {
                    resource: resource.clone(),
                }
            })?;
            subplans.push(plan);
        }
        if !by_resource.is_empty() {
            return Err(CompositePlanError::OrderingIncomplete);
        }

        let fingerprint = composite_plan_fingerprint(grouped, group_id, &subplans)?;
        Ok(Self {
            group_id: group_id.to_owned(),
            grouped_document_fingerprint: grouped.fingerprint(),
            subplans,
            fingerprint,
        })
    }

    /// Resource group whose topology orders this envelope.
    #[must_use]
    pub fn group_id(&self) -> &str {
        &self.group_id
    }

    /// Exact ELANG3 grouped-document identity used during construction.
    #[must_use]
    pub const fn grouped_document_fingerprint(&self) -> Fingerprint {
        self.grouped_document_fingerprint
    }

    /// Candidate-bearing subplans in deterministic required-first order.
    #[must_use]
    pub fn subplans(&self) -> &[Plan] {
        &self.subplans
    }

    /// Find a targeted subplan by logical resource identity.
    #[must_use]
    pub fn subplan(&self, resource: &str) -> Option<&Plan> {
        self.subplans
            .iter()
            .find(|plan| plan.resource.identity().as_str() == resource)
    }

    /// Resource identities in forward composite execution order.
    pub fn execution_order(&self) -> impl DoubleEndedIterator<Item = &str> {
        self.subplans
            .iter()
            .map(|plan| plan.resource.identity().as_str())
    }

    /// Resource identities in the rollback order required if each forward
    /// action must be unwound in reverse.
    pub fn rollback_order(&self) -> impl Iterator<Item = &str> {
        self.execution_order().rev()
    }

    /// Structural identity of grouped document, topology and subplan semantics.
    ///
    /// The fingerprint is non-cryptographic and grants no actuation authority.
    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
}

fn composite_execution_order(
    group: &EirResourceGroup,
    targeted: &BTreeSet<String>,
) -> Result<Vec<String>, CompositePlanError> {
    let mut indegree = targeted
        .iter()
        .cloned()
        .map(|resource| (resource, 0_usize))
        .collect::<BTreeMap<_, _>>();
    let mut outgoing = targeted
        .iter()
        .cloned()
        .map(|resource| (resource, BTreeSet::<String>::new()))
        .collect::<BTreeMap<_, _>>();

    // ELANG3 declares `dependent -> required`. Composite execution uses the
    // inverse edge so required resources are processed before dependents when
    // both are targeted by this envelope.
    for dependency in group.dependencies() {
        if !targeted.contains(dependency.dependent()) || !targeted.contains(dependency.required()) {
            continue;
        }
        outgoing
            .get_mut(dependency.required())
            .ok_or_else(|| CompositePlanError::OrderingLostResource {
                resource: dependency.required().to_owned(),
            })?
            .insert(dependency.dependent().to_owned());
        let degree = indegree.get_mut(dependency.dependent()).ok_or_else(|| {
            CompositePlanError::OrderingLostResource {
                resource: dependency.dependent().to_owned(),
            }
        })?;
        *degree = degree.saturating_add(1);
    }

    let mut ready = indegree
        .iter()
        .filter_map(|(resource, degree)| (*degree == 0).then_some(resource.clone()))
        .collect::<BTreeSet<_>>();
    let mut ordered = Vec::with_capacity(targeted.len());
    while let Some(resource) = ready.pop_first() {
        ordered.push(resource.clone());
        let dependents =
            outgoing
                .get(&resource)
                .ok_or_else(|| CompositePlanError::OrderingLostResource {
                    resource: resource.clone(),
                })?;
        for dependent in dependents {
            let degree = indegree.get_mut(dependent).ok_or_else(|| {
                CompositePlanError::OrderingLostResource {
                    resource: dependent.clone(),
                }
            })?;
            *degree = degree.saturating_sub(1);
            if *degree == 0 {
                ready.insert(dependent.clone());
            }
        }
    }

    if ordered.len() == targeted.len() {
        Ok(ordered)
    } else {
        Err(CompositePlanError::OrderingCycle {
            group: group.id().to_owned(),
        })
    }
}

fn composite_plan_fingerprint(
    grouped: &EirGroupedDocument,
    group_id: &str,
    subplans: &[Plan],
) -> Result<Fingerprint, CompositePlanError> {
    let mut fingerprint = Fingerprint::EMPTY
        .text("elastic-composite-plan-envelope")
        .number(u64::from(COMPOSITE_PLAN_ENVELOPE_SCHEMA_V1))
        .number(grouped.fingerprint().bits())
        .text(group_id)
        .number(subplans.len() as u64);

    for plan in subplans {
        let candidate = plan
            .candidate()
            .ok_or_else(|| CompositePlanError::MissingCandidate {
                resource: plan.resource.identity().as_str().to_owned(),
            })?;
        fingerprint = fingerprint
            .text(plan.resource.identity().as_str())
            .number(plan.resource.fingerprint().bits())
            .number(match candidate.mechanism() {
                TransitionMechanism::Reinterpret => 0,
                TransitionMechanism::Reencode => 1,
                TransitionMechanism::Recompute => 2,
            })
            .text(candidate.dimension().as_str())
            .number(u64::from(candidate.capability_grounded()));
        match candidate.magnitude() {
            Some(magnitude) => {
                fingerprint = fingerprint.number(1).number(magnitude);
            }
            None => {
                fingerprint = fingerprint.number(0);
            }
        }
        let context_len = plan.context.iter().count();
        fingerprint = fingerprint.number(context_len as u64);
        for (signal, value) in plan.context.iter() {
            fingerprint = fingerprint.text(signal.as_str()).number(value.to_bits());
        }
    }
    Ok(fingerprint)
}

/// Fail-closed structural validation errors for composite plan construction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompositePlanError {
    UnknownGroup {
        group: String,
    },
    EmptyEnvelope {
        group: String,
    },
    TooManySubplans {
        group: String,
        plans: usize,
        maximum: usize,
    },
    ResourceOutsideGroup {
        group: String,
        resource: String,
    },
    ResourceDefinitionMismatch {
        group: String,
        resource: String,
        expected: Fingerprint,
        observed: Fingerprint,
    },
    DuplicateResourcePlan {
        resource: String,
    },
    MissingCandidate {
        resource: String,
    },
    UndeclaredCandidate {
        resource: String,
    },
    NonFinitePlanningValue {
        resource: String,
        signal: String,
    },
    OrderingLostResource {
        resource: String,
    },
    OrderingIncomplete,
    OrderingCycle {
        group: String,
    },
}

impl fmt::Display for CompositePlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownGroup { group } => write!(f, "unknown composite resource group {group}"),
            Self::EmptyEnvelope { group } => {
                write!(f, "composite resource group {group} has no targeted subplans")
            }
            Self::TooManySubplans {
                group,
                plans,
                maximum,
            } => write!(
                f,
                "composite resource group {group} has {plans} subplans; maximum is {maximum}"
            ),
            Self::ResourceOutsideGroup { group, resource } => write!(
                f,
                "composite plan resource {resource} is not a member of group {group}"
            ),
            Self::ResourceDefinitionMismatch {
                group,
                resource,
                expected,
                observed,
            } => write!(
                f,
                "composite plan resource {resource} does not match group {group} EIR: expected {expected}, observed {observed}"
            ),
            Self::DuplicateResourcePlan { resource } => {
                write!(f, "composite plan contains multiple subplans for resource {resource}")
            }
            Self::MissingCandidate { resource } => {
                write!(f, "composite subplan for {resource} has no transition candidate")
            }
            Self::UndeclaredCandidate { resource } => write!(
                f,
                "composite subplan for {resource} does not contain a declared capability-grounded candidate"
            ),
            Self::NonFinitePlanningValue { resource, signal } => write!(
                f,
                "composite subplan for {resource} contains non-finite planning signal {signal}"
            ),
            Self::OrderingLostResource { resource } => write!(
                f,
                "composite ordering lost targeted resource {resource}"
            ),
            Self::OrderingIncomplete => write!(f, "composite ordering left unconsumed subplans"),
            Self::OrderingCycle { group } => write!(
                f,
                "composite group {group} unexpectedly contains a dependency cycle"
            ),
        }
    }
}

impl std::error::Error for CompositePlanError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::plan_with_context;
    use elastic_core::resource::{
        AdmissibleTransition, CapabilityRequirement, DimensionId, LogicalResourceId,
        ResourceClassId, ResourceDependency, ResourceGroupBuilder, ResourceGroupId, ResourceSpec,
    };
    use elastic_eir::{
        lower, EirDocumentBuilder, EirGroupedDocument, FirstGroundedPlanner, PlanOutcome,
        PlanningContext,
    };

    fn spec(id: &str) -> ResourceSpec {
        ResourceSpec::builder(
            ResourceClassId::CONFIGURATIONAL,
            LogicalResourceId::new(id).unwrap(),
        )
        .allow(DimensionId::CAPACITY)
        .admit(AdmissibleTransition::new(
            TransitionMechanism::Reinterpret,
            DimensionId::CAPACITY,
        ))
        .require_capability(CapabilityRequirement::new(
            TransitionMechanism::Reinterpret,
            DimensionId::CAPACITY,
        ))
        .build()
        .unwrap()
    }

    fn grouped() -> EirGroupedDocument {
        let mut builder = EirDocumentBuilder::new();
        for id in ["base", "independent", "worker"] {
            builder.push(&spec(id)).unwrap();
        }
        let document = builder.finish().unwrap();
        let group = ResourceGroupBuilder::new(ResourceGroupId::new("stack").unwrap())
            .members([
                LogicalResourceId::new("base").unwrap(),
                LogicalResourceId::new("independent").unwrap(),
                LogicalResourceId::new("worker").unwrap(),
            ])
            .dependency(ResourceDependency::new(
                LogicalResourceId::new("worker").unwrap(),
                LogicalResourceId::new("base").unwrap(),
            ))
            .build()
            .unwrap();
        EirGroupedDocument::new(document, &[group]).unwrap()
    }

    fn plan(grouped: &EirGroupedDocument, resource: &str, value: f64) -> Plan {
        let node = grouped.group_resource("stack", resource).unwrap();
        plan_with_context(
            &FirstGroundedPlanner,
            node,
            &PlanningContext::new().observe(
                elastic_core::resource::ObservationSignalId::UTILIZATION,
                value,
            ),
        )
    }

    #[test]
    fn required_resources_precede_dependents_and_independent_ties_are_canonical() {
        let grouped = grouped();
        let envelope = CompositePlanEnvelope::new(
            &grouped,
            "stack",
            vec![
                plan(&grouped, "worker", 0.3),
                plan(&grouped, "independent", 0.2),
                plan(&grouped, "base", 0.1),
            ],
        )
        .unwrap();
        assert_eq!(
            envelope.execution_order().collect::<Vec<_>>(),
            vec!["base", "independent", "worker"]
        );
        assert_eq!(
            envelope.rollback_order().collect::<Vec<_>>(),
            vec!["worker", "independent", "base"]
        );
    }

    #[test]
    fn input_order_does_not_change_envelope_or_fingerprint() {
        let grouped = grouped();
        let a = plan(&grouped, "base", 0.1);
        let b = plan(&grouped, "worker", 0.2);
        let left =
            CompositePlanEnvelope::new(&grouped, "stack", vec![a.clone(), b.clone()]).unwrap();
        let right = CompositePlanEnvelope::new(&grouped, "stack", vec![b, a]).unwrap();
        assert_eq!(left, right);
        assert_eq!(left.fingerprint(), right.fingerprint());
    }

    #[test]
    fn diagnostic_reasoning_does_not_change_semantic_fingerprint() {
        let grouped = grouped();
        let mut first = plan(&grouped, "base", 0.1);
        let mut second = first.clone();
        first.reasoning = "planner explanation A".to_owned();
        second.reasoning = "planner explanation B".to_owned();
        let left = CompositePlanEnvelope::new(&grouped, "stack", vec![first]).unwrap();
        let right = CompositePlanEnvelope::new(&grouped, "stack", vec![second]).unwrap();
        assert_ne!(left, right);
        assert_eq!(left.fingerprint(), right.fingerprint());
    }

    #[test]
    fn planning_context_changes_composite_fingerprint() {
        let grouped = grouped();
        let left = CompositePlanEnvelope::new(&grouped, "stack", vec![plan(&grouped, "base", 0.1)])
            .unwrap();
        let right =
            CompositePlanEnvelope::new(&grouped, "stack", vec![plan(&grouped, "base", 0.2)])
                .unwrap();
        assert_ne!(left.fingerprint(), right.fingerprint());
    }

    #[test]
    fn duplicate_foreign_mismatched_and_no_candidate_plans_fail_closed() {
        let grouped = grouped();
        let base = plan(&grouped, "base", 0.1);
        assert!(matches!(
            CompositePlanEnvelope::new(&grouped, "stack", vec![base.clone(), base]),
            Err(CompositePlanError::DuplicateResourcePlan { .. })
        ));

        let foreign_spec = spec("foreign");
        let foreign_eir = lower(&foreign_spec).unwrap().resources()[0].clone();
        let foreign =
            plan_with_context(&FirstGroundedPlanner, &foreign_eir, &PlanningContext::new());
        assert!(matches!(
            CompositePlanEnvelope::new(&grouped, "stack", vec![foreign]),
            Err(CompositePlanError::ResourceOutsideGroup { .. })
        ));

        let modified_spec = ResourceSpec::builder(
            ResourceClassId::CONFIGURATIONAL,
            LogicalResourceId::new("base").unwrap(),
        )
        .allow(DimensionId::CAPACITY)
        .allow(DimensionId::ENERGY)
        .admit(AdmissibleTransition::new(
            TransitionMechanism::Reinterpret,
            DimensionId::CAPACITY,
        ))
        .require_capability(CapabilityRequirement::new(
            TransitionMechanism::Reinterpret,
            DimensionId::CAPACITY,
        ))
        .build()
        .unwrap();
        let modified_eir = lower(&modified_spec).unwrap().resources()[0].clone();
        let mismatched = plan_with_context(
            &FirstGroundedPlanner,
            &modified_eir,
            &PlanningContext::new(),
        );
        assert!(matches!(
            CompositePlanEnvelope::new(&grouped, "stack", vec![mismatched]),
            Err(CompositePlanError::ResourceDefinitionMismatch { .. })
        ));

        let base_node = grouped.group_resource("stack", "base").unwrap().clone();
        let no_candidate = Plan::new(
            base_node,
            PlanningContext::new(),
            PlanOutcome::NoCandidate,
            "no candidate".to_owned(),
        );
        assert!(matches!(
            CompositePlanEnvelope::new(&grouped, "stack", vec![no_candidate]),
            Err(CompositePlanError::MissingCandidate { .. })
        ));
    }

    #[test]
    fn nonfinite_context_fails_closed() {
        let grouped = grouped();
        let plan = plan(&grouped, "base", f64::NAN);
        assert!(matches!(
            CompositePlanEnvelope::new(&grouped, "stack", vec![plan]),
            Err(CompositePlanError::NonFinitePlanningValue { .. })
        ));
    }
}
