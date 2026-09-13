//! Stable predicate identities and deterministic registries.
//!
//! Runtime [`crate::PredicateId`] values are compact indices. They are not a
//! durable identity by themselves: durable identity comes from a validated
//! [`PredicateKey`]. A [`PredicateRegistry`] sorts and deduplicates keys before
//! assigning compact IDs, so construction order cannot change the mapping.

use crate::PredicateId;
use std::fmt;

/// Schema version for the first stable Boolean predicate registry contract.
pub const BOOLEAN_PREDICATE_SCHEMA_V1: u32 = 1;

/// Current maximum number of predicates in the dependency-free `u64` core.
pub const MAX_REGISTERED_PREDICATES: usize = u64::BITS as usize;

/// Maximum bytes accepted for either key component.
pub const MAX_PREDICATE_COMPONENT_BYTES: usize = 64;

/// Stable, byte-exact identity of one Boolean predicate.
///
/// Components intentionally use a restricted ASCII vocabulary. This avoids
/// locale and Unicode-normalization ambiguity in fingerprints while keeping
/// identifiers readable in traces and future wire formats.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PredicateKey {
    namespace: String,
    name: String,
}

impl PredicateKey {
    /// Construct a validated stable predicate key.
    ///
    /// Both components must be non-empty ASCII strings containing only
    /// lowercase letters, decimal digits, `.`, `_`, or `-`.
    ///
    /// # Errors
    ///
    /// Returns [`PredicateRegistryError::InvalidComponent`] for malformed
    /// components.
    pub fn new(
        namespace: impl Into<String>,
        name: impl Into<String>,
    ) -> Result<Self, PredicateRegistryError> {
        let namespace = namespace.into();
        let name = name.into();
        validate_component(PredicateComponent::Namespace, &namespace)?;
        validate_component(PredicateComponent::Name, &name)?;
        Ok(Self { namespace, name })
    }

    /// Stable namespace component.
    #[must_use]
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// Stable local-name component.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl fmt::Display for PredicateKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}::{}", self.namespace, self.name)
    }
}

/// Predicate-key component named by a validation failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PredicateComponent {
    /// Namespace component.
    Namespace,
    /// Local-name component.
    Name,
}

impl fmt::Display for PredicateComponent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Namespace => f.write_str("namespace"),
            Self::Name => f.write_str("name"),
        }
    }
}

/// Deterministic predicate-registry construction and lookup failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PredicateRegistryError {
    /// One stable-key component violated the canonical syntax.
    InvalidComponent {
        component: PredicateComponent,
        reason: PredicateComponentError,
    },
    /// More predicates were requested than the current compact core supports.
    TooManyPredicates { max: usize, actual: usize },
}

impl fmt::Display for PredicateRegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidComponent { component, reason } => {
                write!(f, "invalid predicate {component}: {reason}")
            }
            Self::TooManyPredicates { max, actual } => write!(
                f,
                "predicate registry contains {actual} unique keys; maximum is {max}"
            ),
        }
    }
}

impl std::error::Error for PredicateRegistryError {}

/// Canonical component validation failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PredicateComponentError {
    /// Component was empty.
    Empty,
    /// Component exceeded the bounded byte length.
    TooLong { max_bytes: usize },
    /// Component contained a byte outside the canonical ASCII vocabulary.
    InvalidCharacter,
}

impl fmt::Display for PredicateComponentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("component is empty"),
            Self::TooLong { max_bytes } => {
                write!(f, "component exceeds {max_bytes} bytes")
            }
            Self::InvalidCharacter => f.write_str(
                "component must contain only lowercase ASCII letters, digits, '.', '_', or '-'",
            ),
        }
    }
}

/// Deterministic mapping from stable predicate keys to compact runtime IDs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PredicateRegistry {
    keys: Vec<PredicateKey>,
}

impl PredicateRegistry {
    /// Build a canonical registry from an arbitrary key order.
    ///
    /// Duplicate keys are removed before capacity validation and ID assignment.
    /// IDs therefore depend only on the sorted unique key set, never insertion
    /// order.
    ///
    /// # Errors
    ///
    /// Returns [`PredicateRegistryError::TooManyPredicates`] when the unique
    /// key set exceeds [`MAX_REGISTERED_PREDICATES`].
    pub fn from_keys(
        keys: impl IntoIterator<Item = PredicateKey>,
    ) -> Result<Self, PredicateRegistryError> {
        let mut keys: Vec<_> = keys.into_iter().collect();
        keys.sort();
        keys.dedup();
        if keys.len() > MAX_REGISTERED_PREDICATES {
            return Err(PredicateRegistryError::TooManyPredicates {
                max: MAX_REGISTERED_PREDICATES,
                actual: keys.len(),
            });
        }
        Ok(Self { keys })
    }

    /// Empty registry.
    #[must_use]
    pub const fn empty() -> Self {
        Self { keys: Vec::new() }
    }

    /// Number of unique registered predicates.
    #[must_use]
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Whether no predicates are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Look up the compact ID assigned to a stable key.
    #[must_use]
    pub fn id(&self, key: &PredicateKey) -> Option<PredicateId> {
        self.keys
            .binary_search(key)
            .ok()
            .map(|index| PredicateId::new(index as u32))
    }

    /// Resolve a compact ID back to its stable key.
    #[must_use]
    pub fn key(&self, id: PredicateId) -> Option<&PredicateKey> {
        self.keys.get(id.index() as usize)
    }

    /// Iterate `(PredicateId, PredicateKey)` pairs in canonical ID order.
    pub fn iter(&self) -> impl Iterator<Item = (PredicateId, &PredicateKey)> {
        self.keys
            .iter()
            .enumerate()
            .map(|(index, key)| (PredicateId::new(index as u32), key))
    }
}

fn validate_component(
    component: PredicateComponent,
    value: &str,
) -> Result<(), PredicateRegistryError> {
    if value.is_empty() {
        return Err(PredicateRegistryError::InvalidComponent {
            component,
            reason: PredicateComponentError::Empty,
        });
    }
    if value.len() > MAX_PREDICATE_COMPONENT_BYTES {
        return Err(PredicateRegistryError::InvalidComponent {
            component,
            reason: PredicateComponentError::TooLong {
                max_bytes: MAX_PREDICATE_COMPONENT_BYTES,
            },
        });
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte))
    {
        return Err(PredicateRegistryError::InvalidComponent {
            component,
            reason: PredicateComponentError::InvalidCharacter,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(namespace: &str, name: &str) -> PredicateKey {
        PredicateKey::new(namespace, name).unwrap()
    }

    #[test]
    fn registry_assignment_is_independent_of_input_order_and_duplicates() {
        let a = key("elastic.ram", "capacity-ok");
        let b = key("elastic.ram", "pressure-critical");
        let first = PredicateRegistry::from_keys([b.clone(), a.clone(), a.clone()]).unwrap();
        let second = PredicateRegistry::from_keys([a.clone(), b.clone()]).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.len(), 2);
        assert_eq!(first.id(&a), second.id(&a));
        assert_eq!(first.id(&b), second.id(&b));
        assert_eq!(first.key(first.id(&a).unwrap()), Some(&a));
    }

    #[test]
    fn key_syntax_is_canonical_and_bounded() {
        assert!(PredicateKey::new("elastic.ram", "capacity-ok").is_ok());
        assert!(PredicateKey::new("Elastic.RAM", "capacity-ok").is_err());
        assert!(PredicateKey::new("elastic.ram", "").is_err());
        assert!(PredicateKey::new("elastic/rust", "capacity-ok").is_err());
    }

    #[test]
    fn registry_capacity_applies_after_deduplication() {
        let repeated = key("elastic.test", "same");
        let registry = PredicateRegistry::from_keys(
            std::iter::repeat_n(repeated, MAX_REGISTERED_PREDICATES + 1),
        )
        .unwrap();
        assert_eq!(registry.len(), 1);
    }
}
