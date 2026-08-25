//! The versioned EIR document: a validated set of normalized resource nodes.

use crate::error::ValidationError;
use crate::fingerprint::Fingerprint;
use crate::resource::{EirResource, EirResourceParts};
use crate::{SchemaVersion, EIR_SCHEMA_VERSION};
use elastic_core::resource::ResourceSpec;
use std::collections::BTreeSet;

/// A validated, versioned EIR document.
///
/// Resources are stored sorted by identity text. Construction is only possible
/// through validated paths ([`lower`], [`EirDocument::from_parts`], or
/// [`EirDocumentBuilder`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EirDocument {
    schema_version: SchemaVersion,
    resources: Vec<EirResource>,
    fingerprint: Fingerprint,
}

/// Lower one surface declaration into a validated single-resource document.
///
/// # Errors
///
/// Returns [`ValidationError`] if the declaration (or the derived IR) fails
/// structural validation. A valid `ResourceSpec` normally lowers cleanly; the
/// check exists so EIR-level rules (such as capability grounding) are enforced
/// uniformly on every path.
pub fn lower(spec: &ResourceSpec) -> Result<EirDocument, ValidationError> {
    let mut builder = EirDocumentBuilder::new();
    builder.push(spec)?;
    builder.finish()
}

impl EirDocument {
    /// Assemble a document from raw parts (for tools and tests).
    ///
    /// Every part passes the same validation as lowered content, and logical
    /// identities must be unique within the document.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError`] for the first invalid part, duplicate
    /// identity, or an empty document.
    pub fn from_parts(parts: Vec<EirResourceParts>) -> Result<Self, ValidationError> {
        let resources = parts
            .into_iter()
            .map(EirResource::from_parts)
            .collect::<Result<Vec<_>, _>>()?;
        Self::assemble(resources)
    }

    pub(crate) fn assemble(mut resources: Vec<EirResource>) -> Result<Self, ValidationError> {
        if resources.is_empty() {
            return Err(ValidationError::EmptyDocument);
        }
        let mut seen = BTreeSet::new();
        for resource in &resources {
            if !seen.insert(resource.identity().as_str()) {
                return Err(ValidationError::DuplicateResourceIdentity {
                    identity: resource.identity().as_str().to_owned(),
                });
            }
        }
        resources.sort_by_key(|resource| resource.identity().as_str().to_owned());

        let mut fingerprint = Fingerprint::EMPTY.text("eir-document");
        fingerprint = fingerprint.number(u64::from(EIR_SCHEMA_VERSION));
        for resource in &resources {
            fingerprint = fingerprint.number(resource.fingerprint().bits());
        }

        Ok(Self {
            schema_version: SchemaVersion::LATEST,
            resources,
            fingerprint,
        })
    }

    /// The schema version of this document.
    #[must_use]
    pub const fn schema_version(&self) -> SchemaVersion {
        self.schema_version
    }

    /// The normalized resources, sorted by identity text.
    #[must_use]
    pub fn resources(&self) -> &[EirResource] {
        &self.resources
    }

    /// Look up one resource by logical identity text.
    #[must_use]
    pub fn resource(&self, identity: &str) -> Option<&EirResource> {
        // Sorted by identity: binary search keeps lookup deterministic.
        self.resources
            .binary_search_by(|resource| resource.identity().as_str().cmp(identity))
            .ok()
            .map(|index| &self.resources[index])
    }

    /// Structural fingerprint over schema version and all resource nodes.
    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
}

impl std::fmt::Display for EirDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} [{} resources]",
            self.schema_version,
            self.resources.len()
        )
    }
}

/// Accumulates surface declarations and produces one validated document.
#[derive(Clone, Debug, Default)]
pub struct EirDocumentBuilder {
    resources: Vec<EirResource>,
}

impl EirDocumentBuilder {
    /// Start an empty document.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Lower and append one surface declaration.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError`] if lowering fails structural validation.
    pub fn push(&mut self, spec: &ResourceSpec) -> Result<(), ValidationError> {
        let identity = spec.resource_id().as_str().to_owned();
        let class = spec.class().clone();
        let labels = spec
            .iter_labels()
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect();
        let resource = EirResource::from_parts(EirResourceParts {
            identity,
            class,
            dimensions: spec.elastic_dimensions().to_vec(),
            invariants: spec.invariants().to_vec(),
            objectives: spec.objectives().to_vec(),
            transitions: spec.admissible_transitions().to_vec(),
            capabilities: spec.required_capabilities().to_vec(),
            observations: spec.observed_signals().to_vec(),
            labels,
        })?;
        self.resources.push(resource);
        Ok(())
    }

    /// Validate cross-resource uniqueness and finish the document.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError::EmptyDocument`] when nothing was pushed, or
    /// [`ValidationError::DuplicateResourceIdentity`] when two resources share
    /// one logical identity.
    pub fn finish(self) -> Result<EirDocument, ValidationError> {
        EirDocument::assemble(self.resources)
    }
}
