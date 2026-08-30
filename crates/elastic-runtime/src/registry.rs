//! Deterministic registry of validated elastic resources.
//!
//! Registration always lowers a [`ResourceSpec`] into EIR before publishing it
//! in the registry. A resource therefore cannot be discoverable here unless
//! both its surface declaration and normalized IR are available together.

use std::collections::BTreeMap;

use elastic_core::resource::{LogicalResourceId, ResourceSpec};
use elastic_eir::{lower, EirResource};

use crate::RuntimeError;

/// One validated registry entry.
#[derive(Clone, Debug, PartialEq)]
pub struct RegisteredResource {
    spec: ResourceSpec,
    eir: EirResource,
}

impl RegisteredResource {
    #[must_use]
    pub const fn spec(&self) -> &ResourceSpec {
        &self.spec
    }

    #[must_use]
    pub const fn eir(&self) -> &EirResource {
        &self.eir
    }

    #[must_use]
    pub const fn id(&self) -> &LogicalResourceId {
        self.eir.identity()
    }
}

/// Deterministic collection of validated resource declarations.
#[derive(Clone, Debug, Default)]
pub struct ResourceRegistry {
    resources: BTreeMap<LogicalResourceId, RegisteredResource>,
}

impl ResourceRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Validate, lower, and register one resource declaration.
    ///
    /// # Errors
    ///
    /// Returns a configuration error when the logical resource id is already
    /// registered, EIR lowering fails, or lowering unexpectedly omits the
    /// declared resource.
    pub fn register(&mut self, spec: ResourceSpec) -> Result<(), RuntimeError> {
        let id = spec.resource_id().clone();
        if self.resources.contains_key(&id) {
            return Err(RuntimeError::configuration(format!(
                "resource '{}' is already registered",
                id.as_str()
            )));
        }

        let document = lower(&spec).map_err(|error| {
            RuntimeError::configuration(format!(
                "failed to lower resource '{}' into EIR: {error}",
                id.as_str()
            ))
        })?;
        let eir = document.resource(id.as_str()).cloned().ok_or_else(|| {
            RuntimeError::configuration(format!(
                "lowering resource '{}' produced no matching EIR node",
                id.as_str()
            ))
        })?;

        self.resources.insert(id, RegisteredResource { spec, eir });
        Ok(())
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&RegisteredResource> {
        self.resources
            .iter()
            .find_map(|(resource_id, entry)| (resource_id.as_str() == id).then_some(entry))
    }

    #[must_use]
    pub fn contains(&self, id: &str) -> bool {
        self.get(id).is_some()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.resources.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.resources.is_empty()
    }

    /// Iterate entries in canonical logical-resource-id order.
    pub fn iter(&self) -> impl Iterator<Item = &RegisteredResource> {
        self.resources.values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use elastic_adapters::RamBudget;

    fn ram_spec(id: &str) -> ResourceSpec {
        RamBudget::new(id, 4096, 512, 4096, 1024, Some(2048))
            .unwrap()
            .spec()
            .clone()
    }

    #[test]
    fn registry_lowers_and_resolves_multiple_resources_deterministically() {
        let mut registry = ResourceRegistry::new();
        registry.register(ram_spec("z-resource")).unwrap();
        registry.register(ram_spec("a-resource")).unwrap();

        assert_eq!(registry.len(), 2);
        assert_eq!(
            registry
                .iter()
                .map(|entry| entry.id().as_str())
                .collect::<Vec<_>>(),
            vec!["a-resource", "z-resource"]
        );
        assert_eq!(
            registry.get("a-resource").unwrap().eir().identity().as_str(),
            "a-resource"
        );
    }

    #[test]
    fn duplicate_resource_identity_is_rejected_without_replacement() {
        let mut registry = ResourceRegistry::new();
        registry.register(ram_spec("ram")).unwrap();
        let error = registry
            .register(ram_spec("ram"))
            .expect_err("duplicate identity must fail");

        assert!(matches!(error, RuntimeError::Configuration(_)));
        assert_eq!(registry.len(), 1);
    }
}
