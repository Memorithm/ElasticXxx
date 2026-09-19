//! Public support types for the embedded Elastic language.
//!
//! Syntax is implemented by `elastic-macros`, but semantic authority remains in
//! the ordinary `ResourceSpec` and EIR types. This module only aggregates
//! structured errors needed when one language document contains multiple
//! independently validated resources.

use std::fmt;

use elastic_core::resource::{
    CrossResourceInvariantError, ResourceGroupError, ResourceSpecError, SharedBudgetError,
};
use elastic_core::{PredicateRegistryError, PseudoBooleanError};
use elastic_eir::{GroupLoweringError, ValidationError};

/// Failure while materializing one multi-resource Elastic language document.
#[derive(Debug)]
pub enum ElasticDocumentError {
    /// One named child resource failed ordinary `ResourceSpec` validation.
    Resource {
        /// Rust module name of the child declaration.
        resource: &'static str,
        /// Authoritative typed resource validation error.
        source: ResourceSpecError,
    },
    /// The independently valid resources failed EIR document validation.
    Eir(ValidationError),
}

impl ElasticDocumentError {
    /// Attach one resource declaration error to the language child that
    /// produced it. This does not reinterpret the underlying error.
    #[must_use]
    pub const fn resource(resource: &'static str, source: ResourceSpecError) -> Self {
        Self::Resource { resource, source }
    }

    /// Child resource module name when failure happened before EIR assembly.
    #[must_use]
    pub const fn resource_name(&self) -> Option<&'static str> {
        match self {
            Self::Resource { resource, .. } => Some(resource),
            Self::Eir(_) => None,
        }
    }
}

impl fmt::Display for ElasticDocumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Resource { resource, source } => {
                write!(f, "elastic resource {resource} is invalid: {source}")
            }
            Self::Eir(source) => write!(f, "elastic multi-resource document is invalid: {source}"),
        }
    }
}

impl std::error::Error for ElasticDocumentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Resource { source, .. } => Some(source),
            Self::Eir(source) => Some(source),
        }
    }
}

impl From<ValidationError> for ElasticDocumentError {
    fn from(source: ValidationError) -> Self {
        Self::Eir(source)
    }
}

/// Failure while materializing ELANG3 groups over one validated language document.
#[derive(Debug)]
pub enum ElasticGroupDocumentError {
    Document(ElasticDocumentError),
    Resource(ResourceSpecError),
    Group(ResourceGroupError),
    SharedBudget(SharedBudgetError),
    CrossInvariant(CrossResourceInvariantError),
    Predicate(PredicateRegistryError),
    PseudoBoolean(PseudoBooleanError),
    EirGroup(GroupLoweringError),
}

impl fmt::Display for ElasticGroupDocumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Document(error) => error.fmt(f),
            Self::Resource(error) => write!(f, "elastic group resource term is invalid: {error}"),
            Self::Group(error) => write!(f, "elastic resource group is invalid: {error}"),
            Self::SharedBudget(error) => write!(f, "elastic shared budget is invalid: {error}"),
            Self::CrossInvariant(error) => {
                write!(f, "elastic cross-resource invariant is invalid: {error}")
            }
            Self::Predicate(error) => write!(f, "elastic group predicate is invalid: {error}"),
            Self::PseudoBoolean(error) => {
                write!(f, "elastic group pseudo-Boolean scale is invalid: {error}")
            }
            Self::EirGroup(error) => write!(f, "elastic grouped EIR is invalid: {error}"),
        }
    }
}

impl std::error::Error for ElasticGroupDocumentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Document(error) => Some(error),
            Self::Resource(error) => Some(error),
            Self::Group(error) => Some(error),
            Self::SharedBudget(error) => Some(error),
            Self::CrossInvariant(error) => Some(error),
            Self::Predicate(error) => Some(error),
            Self::PseudoBoolean(error) => Some(error),
            Self::EirGroup(error) => Some(error),
        }
    }
}

impl From<ElasticDocumentError> for ElasticGroupDocumentError {
    fn from(value: ElasticDocumentError) -> Self {
        Self::Document(value)
    }
}
impl From<ResourceSpecError> for ElasticGroupDocumentError {
    fn from(value: ResourceSpecError) -> Self {
        Self::Resource(value)
    }
}
impl From<ResourceGroupError> for ElasticGroupDocumentError {
    fn from(value: ResourceGroupError) -> Self {
        Self::Group(value)
    }
}
impl From<SharedBudgetError> for ElasticGroupDocumentError {
    fn from(value: SharedBudgetError) -> Self {
        Self::SharedBudget(value)
    }
}
impl From<CrossResourceInvariantError> for ElasticGroupDocumentError {
    fn from(value: CrossResourceInvariantError) -> Self {
        Self::CrossInvariant(value)
    }
}
impl From<PredicateRegistryError> for ElasticGroupDocumentError {
    fn from(value: PredicateRegistryError) -> Self {
        Self::Predicate(value)
    }
}
impl From<PseudoBooleanError> for ElasticGroupDocumentError {
    fn from(value: PseudoBooleanError) -> Self {
        Self::PseudoBoolean(value)
    }
}
impl From<GroupLoweringError> for ElasticGroupDocumentError {
    fn from(value: GroupLoweringError) -> Self {
        Self::EirGroup(value)
    }
}
