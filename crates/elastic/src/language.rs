//! Public support types for the embedded Elastic language.
//!
//! Syntax is implemented by `elastic-macros`, but semantic authority remains in
//! the ordinary `ResourceSpec` and EIR types. This module only aggregates
//! structured errors needed when one language document contains multiple
//! independently validated resources.

use std::fmt;

use elastic_core::resource::ResourceSpecError;
use elastic_eir::ValidationError;

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
